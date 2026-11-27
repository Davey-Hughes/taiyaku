import { act } from 'react'
import {
  afterAll,
  beforeAll,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { page } from 'vitest/browser'
import { BookReader } from '../src/BookReader'
import {
  type Anchor,
  addProgress,
  anchorOf,
  matchShows,
  resumeBlock,
  shows,
  topOf,
  wordNodes,
} from '../src/bookPages'
import '../src/index.css'
import { deferred } from './support/async'
import { typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { tapAt } from './support/touch'

vi.mock('../src/Account', () => ({ Account: () => null }))

// A figure's image from nowhere unless a test draws one, which fails at once:
// the dev server's proxy has no server behind it to ask, and says so on every
// figure drawn.
const figureImage = vi.hoisted(() => ({ url: 'data:,' }))
vi.mock('../src/book', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/book')>()),
  bookImageUrl: () => figureImage.url,
}))

// The reader's place through what lays its text out again, or might: each
// docked panel opened and closed, and a Settings Reset. Measured in Chromium,
// in a window where a panel sets the text in and reflows it and in the phone
// layout, where it goes over the text.

const SAID =
  '吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。'
const WORDS =
  'It was a bright cold day in April, and the clocks were striking thirteen. '

/** Two editions, aligned block by block, each with a figure before block 6. */
const book = {
  title: '猫',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: Array.from({ length: 30 }, () => {
        const text = SAID.repeat(3)
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
      figures: [{ before: 6, image: 0, alt: '地図' }],
    },
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 30 }, (_, i) => {
        // Of differing lengths, so a page or a screen starts inside one.
        const text = `Block ${i}. ${WORDS.repeat(9 + (i % 5))}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
      figures: [{ before: 6, image: 0, alt: 'a map' }],
    },
  },
  alignments: [
    {
      to: 'en',
      links: Array.from({ length: 30 }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: 30 }, (_, i) => [i, i + 1, i, i + 1]),
      pins: [],
    },
  ],
}

// The figure drawn with no image loaded. Paged, it starts a page and is too
// tall to leave room there for block 6's first line, so a place taken from
// the words after it would be the next page. Scrolled across, it is as wide
// as the column, so a panel's set-in that narrows the column draws it
// shorter, as a wide picture is; scrolled down, it keeps a size of its own.
const sized = document.createElement('style')
sized.textContent = `
  .book-text[data-paged] .book-figure { break-before: column }
  .book-text[data-paged]:not([data-vertical]) .book-figure img { width: 300px; height: 620px }
  .book-text:not([data-paged], [data-vertical]) .book-figure img { width: 100%; aspect-ratio: 1 / 2 }
  .book-text[data-vertical] .book-figure img { width: 500px; height: 300px }
`
beforeAll(() => {
  document.head.append(sized)
})
// After every mount is torn down, since a reader saves its place as it goes,
// to the stubbed server rather than the real one.
afterAll(() => {
  sized.remove()
  localStorage.removeItem('taiyaku.prefs.book-layout')
  localStorage.removeItem('taiyaku.prefs.book-text')
  vi.unstubAllGlobals()
})

interface View {
  name: string
  width: number
  height: number
  /** Whether an open docked panel sets the text in beside it, as `book.css` does in the sidebar layout. */
  setIn: boolean
}

/** Wide enough for the sidebar layout, narrow enough that the set-in reflows horizontal text. */
const SET_IN: View = {
  name: 'set in beside the panel, 1000 by 700',
  width: 1000,
  height: 700,
  setIn: true,
}
/** Where a docked panel goes over the text. */
const PHONE: View = {
  name: 'the phone layout, 800 by 700',
  width: 800,
  height: 700,
  setIn: false,
}

const FLOWS = [
  {
    name: 'horizontal, scrolled',
    edition: 'en',
    vertical: false,
    paged: false,
  },
  { name: 'horizontal, paged', edition: 'en', vertical: false, paged: true },
  { name: 'vertical, scrolled', edition: 'ja', vertical: true, paged: false },
  { name: 'vertical, paged', edition: 'ja', vertical: true, paged: true },
]

/** The notes the server holds on the book, for a test that opens one. */
let notesServed: object[] = []

/** The server's answer to a Japanese look-up, for a test that serves one. */
let dictionaryAt: object | undefined

/** The server's answers to a reader saved on `edition` at block `slot`, the book `sent` by way of `answer`. */
function served(
  edition: string,
  slot: number,
  sent: object,
  answer: (body: object) => Promise<Response> = async (body) =>
    Response.json(body),
) {
  return vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/api/works/w/book') return answer(sent)
    if (url.startsWith('/api/dictionary/')) {
      if (dictionaryAt && url.includes('/at/')) {
        return Response.json(dictionaryAt)
      }
      return new Response(null, { status: 404 })
    }
    if (url === '/api/works/w/notes') {
      // A note written is stored as sent, as the server answers.
      const doc =
        init?.method === 'PUT'
          ? JSON.parse(String(init.body))
          : { work: 'w', notes: notesServed }
      return Response.json(doc, { headers: { etag: '"n"' } })
    }
    if (init?.method === 'PUT') return new Response(null, { status: 204 })
    return Response.json({ positions: [{ edition, slot, updated: 'now' }] })
  })
}

/** The reader filling a `view`-sized window, on `edition` at block `slot`, set by `layout` and `typeset`. */
async function opened(
  view: View,
  edition: string,
  slot: number,
  layout: object,
  typeset: object,
  sent: object = book,
): Promise<HTMLElement> {
  await page.viewport(view.width, view.height)
  // Heard before the reader mounts: a test of a resize as the book opens sets
  // that race up itself.
  await settled()
  localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(layout))
  localStorage.setItem('taiyaku.prefs.book-text', JSON.stringify(typeset))
  vi.stubGlobal('fetch', served(edition, slot, sent))
  const host = mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  const at = `[data-block="${slot}"], [data-pick="left:${slot}"]`
  for (let i = 0; i < 100 && !host.querySelector(at); i++) {
    await act(() => new Promise((done) => setTimeout(done, 10)))
  }
  await settled()
  return host
}

/** Three frames inside `act`, for what a placing sent to be drawn and the scroll it sent to be heard. */
async function settled() {
  for (let i = 0; i < 3; i++) {
    await act(() => new Promise((done) => requestAnimationFrame(done)))
  }
}

/** `key` pressed on the window, and what it drew settled. */
async function pressed(key: string) {
  await act(async () => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key, cancelable: true, bubbles: true }),
    )
  })
  await settled()
}

const textOf = (host: HTMLElement) =>
  host.querySelector('.book-text') as HTMLElement

/** The docked panel open, as the reader names it on its root. */
const panelOf = (host: HTMLElement) =>
  (host.querySelector('.book-reader') as HTMLElement).dataset.panel

/**
 * A reader's own moves on until the screen starts inside a paragraph:
 * paged, a page at a time; scrolled, by a length no line's height divides,
 * so it stops part way into a line.
 */
async function readOn(text: HTMLElement, flow: (typeof FLOWS)[number]) {
  for (let i = 0; i < 8 && (i === 0 || topOf(text).at === 0); i++) {
    if (flow.paged) await pressed('PageDown')
    else {
      await act(async () => {
        text.scrollBy(flow.vertical ? { left: -97 } : { top: 97 })
      })
      await settled()
    }
  }
}

/** Where the reader is: how far the text is scrolled each way, and the first character not yet scrolled past. */
const placeOf = (text: HTMLElement) => ({
  left: text.scrollLeft,
  top: text.scrollTop,
  first: topOf(text),
})

/** How far past the screen's start `box` starts, along the reading axis. */
function startOf(text: HTMLElement, box: DOMRect): number {
  const screen = text.getBoundingClientRect()
  const sideways =
    (text.dataset.vertical !== undefined) !== (text.dataset.paged !== undefined)
  if (!sideways) return box.top - screen.top
  return text.dataset.vertical !== undefined
    ? screen.right - box.right
    : box.left - screen.left
}

/** The box of `anchor`'s character, in its block or the first column of a compared row. */
function charBox(text: HTMLElement, anchor: Anchor): DOMRect {
  const words = text.querySelector(
    `[data-block="${anchor.block}"] > [data-text], [data-pick="left:${anchor.block}"]`,
  ) as HTMLElement
  let left = anchor.at
  for (const node of wordNodes(words)) {
    if (left < node.length) {
      const range = document.createRange()
      range.setStart(node, left)
      range.setEnd(node, left + 1)
      return range.getBoundingClientRect()
    }
    left -= node.length
  }
  throw new Error('no such character')
}

/** Three characters of the top block selected and looked up, which opens the look-up panel. */
async function lookUp(host: HTMLElement) {
  const text = textOf(host)
  const top = topOf(text)
  const words = text.querySelector(
    `[data-block="${top.block}"] > [data-text]`,
  ) as HTMLElement
  const node = wordNodes(words)[0]
  const range = document.createRange()
  range.setStart(node, 0)
  range.setEnd(node, 3)
  window.getSelection()?.removeAllRanges()
  window.getSelection()?.addRange(range)
  await act(async () => {
    text.dispatchEvent(
      new PointerEvent('pointerdown', {
        clientX: 0,
        clientY: 0,
        bubbles: true,
      }),
    )
    text.dispatchEvent(
      new PointerEvent('pointerup', {
        clientX: 200,
        clientY: 200,
        bubbles: true,
      }),
    )
  })
  await settled()
  const button = [
    ...host.querySelectorAll('[aria-label="Selection"] button'),
  ].find((b) => b.textContent === 'Look up') as HTMLElement | undefined
  if (!button) throw new Error('no Look up on the selection bar')
  await act(async () => {
    button.click()
  })
  await settled()
}

/** Each docked panel, and how a reader opens it; each closes on Escape. */
const PANELS: { name: string; open: (host: HTMLElement) => Promise<void> }[] = [
  { name: 'help', open: () => pressed('?') },
  { name: 'contents', open: () => pressed('t') },
  { name: 'search', open: () => pressed('/') },
  { name: 'note', open: () => pressed('n') },
  { name: 'notes', open: () => pressed('a') },
  { name: 'lookup', open: lookUp },
]

describe('the reader’s place, through a docked panel opened and closed', () => {
  for (const view of [SET_IN, PHONE]) {
    for (const flow of FLOWS) {
      it(`is where it was after each panel and six in a row, ${flow.name}, ${view.name}`, async () => {
        const host = await opened(
          view,
          flow.edition,
          10,
          { paged: flow.paged },
          { vertical: flow.vertical },
        )
        const text = textOf(host)
        expect(text.dataset.vertical !== undefined).toBe(flow.vertical)
        await readOn(text, flow)
        const before = placeOf(text)
        expect(before.first.at, 'into a paragraph').toBeGreaterThan(0)
        const width = text.clientWidth
        // Scrolled, how far past the screen's start its first character
        // starts, which a set-in panel's re-layout must leave as it was.
        const line = startOf(text, charBox(text, before.first))
        for (const panel of PANELS) {
          await panel.open(host)
          expect(panelOf(host)).toBe(panel.name)
          // Set in, or gone over.
          if (view.setIn) expect(text.clientWidth).toBeLessThan(width)
          else expect(text.clientWidth).toBe(width)
          if (view.setIn) {
            expect(shows(text, before.first), `${panel.name} open`).toBe(true)
            if (!flow.paged) {
              const now = startOf(text, charBox(text, before.first))
              expect(
                Math.abs(now - line),
                `${panel.name} open, first line`,
              ).toBeLessThanOrEqual(1)
            }
          } else {
            expect(placeOf(text), `${panel.name} open`).toEqual(before)
          }
          await pressed('Escape')
          expect(panelOf(host)).toBeUndefined()
          expect(placeOf(text), `${panel.name} closed`).toEqual(before)
        }
        for (let n = 1; n <= 6; n++) {
          await pressed('t')
          await pressed('Escape')
          expect(placeOf(text), `contents toggled ${n} times`).toEqual(before)
        }
      })
    }
  }

  it('leaves a scroll not yet heard where it is as a panel opens over the text, the phone layout', async () => {
    const host = await opened(PHONE, 'en', 10, {}, {})
    const text = textOf(host)
    // Scrolled and the panel opened before the scroll's event is sent, as
    // a key pressed while a flick still runs on: nothing is laid out again,
    // so nothing may put the text back where the last event left it.
    let scrolled = 0
    await act(async () => {
      text.scrollBy({ top: 97 })
      scrolled = text.scrollTop
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: '?', cancelable: true }),
      )
    })
    await settled()
    expect(panelOf(host)).toBe('help')
    expect(text.scrollTop).toBe(scrolled)
  })

  for (const view of [SET_IN, PHONE]) {
    it(`keeps a page a figure starts on that page, ${view.name}`, async () => {
      const host = await opened(view, 'en', 5, { paged: true }, {})
      const text = textOf(host)
      const figure = text.querySelector('.book-figure') as HTMLElement
      const at = () => startOf(text, figure.getBoundingClientRect())
      for (let i = 0; i < 20 && at() >= text.clientWidth; i++) {
        await pressed('PageDown')
      }
      const before = placeOf(text)
      const was = at()
      expect(was).toBeGreaterThanOrEqual(0)
      expect(was).toBeLessThan(text.clientWidth / 2)
      // Paged, the words after the figure start the next page.
      expect(shows(text, { block: 6, at: 0 })).toBe(false)
      await pressed('?')
      expect(panelOf(host)).toBe('help')
      expect(at(), 'open').toBe(was)
      await pressed('Escape')
      expect(at(), 'closed').toBe(was)
      expect(placeOf(text)).toEqual(before)
    })

    for (const vertical of [false, true]) {
      it(`keeps a figure part way past the top where it was, ${vertical ? 'vertical' : 'horizontal'}, scrolled, ${view.name}`, async () => {
        const host = await opened(
          view,
          vertical ? 'ja' : 'en',
          5,
          {},
          { vertical },
        )
        const text = textOf(host)
        const figure = text.querySelector('.book-figure') as HTMLElement
        const at = () => startOf(text, figure.getBoundingClientRect())
        const length = () => {
          const box = figure.getBoundingClientRect()
          return vertical ? box.width : box.height
        }
        await act(async () => {
          const by = at() + 250
          text.scrollBy(vertical ? { left: -by } : { top: by })
        })
        await settled()
        const before = placeOf(text)
        const was = at()
        const drawn = length()
        expect(Math.abs(was + 250)).toBeLessThan(1)
        await pressed('t')
        expect(panelOf(host)).toBe('contents')
        if (view.setIn) {
          // The words before it laid out again, and across, the figure drawn
          // shorter: the screen starts as far into it, to the pixel it
          // rounds to.
          if (!vertical) expect(length()).toBeLessThan(drawn)
          const into = (was / drawn) * length()
          expect(Math.abs(at() - into), 'open').toBeLessThanOrEqual(1)
        } else expect(at(), 'open').toBe(was)
        await pressed('Escape')
        expect(at(), 'closed').toBe(was)
        expect(placeOf(text)).toEqual(before)
      })
    }
  }
})

/** The settings' section `name` reset, the settings closed again. */
async function resetSection(host: HTMLElement, name: string) {
  await pressed('s')
  const section = [
    ...host.querySelectorAll('[aria-label="Settings"] .reader-section'),
  ].find((s) => s.querySelector('h3')?.textContent === name)
  const button = section?.querySelector('.reader-section-head button')
  if (!button) throw new Error(`no reset on the ${name} section`)
  await act(async () => {
    ;(button as HTMLElement).click()
  })
  await settled()
  await pressed('Escape')
}

const resetText = (host: HTMLElement) => resetSection(host, 'Text')

describe('the reader’s place, through a Reset of the text’s size', () => {
  // Vertical is a setting of the Text section too, so its Reset sets the text
  // horizontal as well as back to its usual size.
  for (const flow of FLOWS) {
    it(`holds the character the reader was at, ${flow.name}`, async () => {
      const host = await opened(
        SET_IN,
        flow.edition,
        8,
        { paged: flow.paged },
        { textSize: 26, vertical: flow.vertical },
      )
      const text = textOf(host)
      await readOn(text, flow)
      const before = topOf(text)
      expect(before.at, 'into a paragraph').toBeGreaterThan(0)
      await resetText(host)
      expect(getComputedStyle(text).fontSize).toBe('18px')
      expect(text.dataset.vertical).toBeUndefined()
      expect(shows(text, before), 'on screen').toBe(true)
      // Scrolled, on the first line: past the screen's start by less than
      // half a line, which only the gap between two lines can be.
      if (!flow.paged) {
        const line = 18 * 1.6
        expect(startOf(text, charBox(text, before))).toBeLessThan(line / 2)
      }
    })
  }
})

/** The top bar's button for layout `mode` pressed, and the bars hidden again. */
async function switchTo(host: HTMLElement, mode: string) {
  await pressed('Tab')
  const button = [
    ...host.querySelectorAll('[aria-label="Layout"] button'),
  ].find((b) => b.textContent === mode)
  if (!button) throw new Error(`no ${mode} layout in the top bar`)
  await act(async () => {
    ;(button as HTMLElement).click()
  })
  await settled()
  await pressed('Escape')
}

/** Scrolled on until the screen starts part way into its first character. */
async function partWay(text: HTMLElement, flow: (typeof FLOWS)[number]) {
  await readOn(text, flow)
  const first = topOf(text)
  expect(first.at, 'into a paragraph').toBeGreaterThan(0)
  expect(
    startOf(text, charBox(text, first)),
    'its first character part scrolled past',
  ).toBeLessThan(-1)
  return first
}

describe('the reader’s place, from one layout to another', () => {
  for (const flow of [FLOWS[0], FLOWS[2]]) {
    it(`keeps the first line going into compare, ${flow.name}`, async () => {
      const host = await opened(
        SET_IN,
        flow.edition,
        10,
        {},
        { vertical: flow.vertical },
      )
      const text = textOf(host)
      const first = await partWay(text, flow)
      await switchTo(host, 'compare')
      expect(text.dataset.compared).toBeDefined()
      expect(Math.abs(startOf(text, charBox(text, first)))).toBeLessThanOrEqual(
        1,
      )
    })
  }

  it('shows the top row’s block whole coming out of compare', async () => {
    const host = await opened(SET_IN, 'en', 10, { mode: 'compare' }, {})
    const text = textOf(host)
    await act(async () => {
      text.scrollBy({ top: 150 })
    })
    await settled()
    const { block } = topOf(text)
    const row = text.querySelector(`[data-block="${block}"]`) as HTMLElement
    expect(startOf(text, row.getBoundingClientRect())).toBeLessThan(-1)
    await switchTo(host, 'scroll')
    expect(text.dataset.compared).toBeUndefined()
    // Past the screen's start only by the half-leading above its first line.
    const start = startOf(text, charBox(text, { block, at: 0 }))
    expect(start).toBeGreaterThanOrEqual(-1)
    expect(start).toBeLessThan((18 * 1.6) / 2)
  })
})

describe('the first line, as the text is laid out at a new size or width', () => {
  const flow = FLOWS[0]

  it('starts the screen after a text size key, horizontal', async () => {
    const host = await opened(SET_IN, flow.edition, 10, {}, {})
    const text = textOf(host)
    const first = await partWay(text, flow)
    await pressed('=')
    expect(Math.abs(startOf(text, charBox(text, first)))).toBeLessThanOrEqual(1)
  })

  it('starts the screen after a resize, horizontal', async () => {
    const host = await opened(SET_IN, flow.edition, 10, {}, {})
    const text = textOf(host)
    const first = await partWay(text, flow)
    await page.viewport(760, 700)
    await settled()
    expect(Math.abs(startOf(text, charBox(text, first)))).toBeLessThanOrEqual(1)
  })
})

/** Turned on a page at a time until a row begun on an earlier page is the first on this one; none past the book's end. */
async function onACarriedPage(text: HTMLElement) {
  for (let i = 0; i < 12; i++) {
    await pressed('PageDown')
    const { block } = topOf(text)
    const row = text.querySelector(`[data-block="${block}"]`) as HTMLElement
    if (startOf(text, row.getBoundingClientRect()) < 0) return row
  }
  return undefined
}

describe('the reader’s place, compared or interlinear', () => {
  // The widest window's text is set larger, so its rows still run over pages.
  for (const { view, typeset } of [
    { view: SET_IN, typeset: {} },
    {
      view: { name: '1366 by 768', width: 1366, height: 768, setIn: true },
      typeset: {},
    },
    {
      view: { name: '1920 by 1080', width: 1920, height: 1080, setIn: true },
      typeset: { textSize: 30 },
    },
    { view: PHONE, typeset: {} },
  ]) {
    it(`keeps the page through a panel, on the pages a row carries over onto, compared and paged, ${view.name}`, async () => {
      const host = await opened(
        view,
        'en',
        4,
        { mode: 'compare', paged: true },
        typeset,
      )
      const text = textOf(host)
      let held = 0
      for (let turn = 0; turn < 4; turn++) {
        const row = await onACarriedPage(text)
        if (!row) break
        const before = text.scrollLeft
        await pressed('t')
        expect(panelOf(host)).toBe('contents')
        const box = row.getBoundingClientRect()
        const screen = text.getBoundingClientRect()
        expect(box.left, 'the row still on the page, open').toBeLessThan(
          screen.right,
        )
        expect(box.right, 'the row still on the page, open').toBeGreaterThan(
          screen.left,
        )
        await pressed('Escape')
        expect(text.scrollLeft, `turn ${turn}`).toBe(before)
        held++
      }
      expect(held, 'pages a row carries over onto').toBeGreaterThan(0)
    })
  }

  it('keeps a row that starts a page on that page with a panel open, compared and paged', async () => {
    const starts = document.createElement('style')
    starts.textContent = '.book-row[data-block="8"] { break-before: column }'
    document.head.append(starts)
    try {
      const host = await opened(
        SET_IN,
        'en',
        4,
        { mode: 'compare', paged: true },
        {},
      )
      const text = textOf(host)
      const row = text.querySelector('[data-block="8"]') as HTMLElement
      for (
        let i = 0;
        i < 30 &&
        startOf(text, row.getBoundingClientRect()) >= text.clientWidth;
        i++
      ) {
        await pressed('PageDown')
      }
      expect(topOf(text).block).toBe(8)
      const before = text.scrollLeft
      await pressed('t')
      expect(panelOf(host)).toBe('contents')
      expect(row.getBoundingClientRect().left, 'open').toBeLessThan(
        text.getBoundingClientRect().right,
      )
      await pressed('Escape')
      expect(text.scrollLeft).toBe(before)
    } finally {
      starts.remove()
    }
  })

  for (const { mode, paged } of [
    { mode: 'compare', paged: false },
    { mode: 'interlinear', paged: false },
    { mode: 'interlinear', paged: true },
  ]) {
    it(`keeps the place through a panel, ${mode}, ${paged ? 'paged' : 'scrolled'}`, async () => {
      const host = await opened(SET_IN, 'en', 6, { mode, paged }, {})
      const text = textOf(host)
      await readOn(text, { ...FLOWS[0], paged })
      const before = placeOf(text)
      for (let n = 1; n <= 3; n++) {
        await pressed('t')
        expect(panelOf(host)).toBe('contents')
        await pressed('Escape')
        expect(placeOf(text), `contents toggled ${n} times`).toEqual(before)
      }
    })
  }

  for (const section of ['Motion', 'Keyboard help', 'Read aloud']) {
    it(`stays on the page through a Reset of ${section}, compared and paged`, async () => {
      const host = await opened(
        SET_IN,
        'en',
        4,
        { mode: 'compare', paged: true },
        {},
      )
      const text = textOf(host)
      expect(await onACarriedPage(text)).toBeDefined()
      const before = text.scrollLeft
      await resetSection(host, section)
      expect(text.scrollLeft).toBe(before)
    })

    it(`leaves a line part scrolled past as it is through a Reset of ${section}`, async () => {
      const host = await opened(SET_IN, 'en', 10, {}, {})
      const text = textOf(host)
      await partWay(text, FLOWS[0])
      const before = placeOf(text)
      await resetSection(host, section)
      expect(placeOf(text)).toEqual(before)
    })
  }

  it('stays on the page through a Reset of a Text section already at its defaults, compared and paged', async () => {
    const host = await opened(
      SET_IN,
      'en',
      4,
      { mode: 'compare', paged: true },
      {},
    )
    const text = textOf(host)
    expect(await onACarriedPage(text)).toBeDefined()
    const before = text.scrollLeft
    await resetText(host)
    expect(text.scrollLeft).toBe(before)
  })

  it('goes to the top row’s start through a Reset that sets the text’s size back, compared', async () => {
    const host = await opened(
      SET_IN,
      'en',
      10,
      { mode: 'compare' },
      { textSize: 26 },
    )
    const text = textOf(host)
    await act(async () => {
      text.scrollBy({ top: 150 })
    })
    await settled()
    const { block } = topOf(text)
    await resetText(host)
    expect(getComputedStyle(text).fontSize).toBe('18px')
    const row = text.querySelector(`[data-block="${block}"]`) as HTMLElement
    expect(
      Math.abs(startOf(text, row.getBoundingClientRect())),
    ).toBeLessThanOrEqual(1)
  })
})

const NEEDLE = 'Zanzibar'

/** Two editions compared, block 5's row far taller than the screen and `NEEDLE` near its end. */
const tall = {
  title: 'Tall',
  hub: 'en',
  editions: Object.fromEntries(
    ['en', 'fr'].map((key) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: 12 }, (_, i) => {
          const far = i === 5 && key === 'en' ? `${NEEDLE} ` : ''
          const text = `Block ${i}. ${WORDS.repeat(i === 5 ? 70 : 3)}${far}${WORDS}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
        figures: [],
      },
    ]),
  ),
  alignments: [
    {
      to: 'fr',
      links: Array.from({ length: 12 }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: 12 }, (_, i) => [i, i + 1, i, i + 1]),
      pins: [],
    },
  ],
}

/** Two editions, block 1 to 3 of the first drawn in one row, block 3 opening with `NEEDLE`. */
const merged = {
  title: 'Merged',
  hub: 'en',
  editions: Object.fromEntries(
    ['en', 'fr'].map((key) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: 8 }, (_, i) => {
          const far = i === 3 && key === 'en' ? `${NEEDLE} ` : ''
          const text = `${far}Block ${i}. ${WORDS.repeat(40)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
        figures: [],
      },
    ]),
  ),
  alignments: [
    {
      to: 'fr',
      links: [
        [0, 1, 0, 1],
        [1, 4, 1, 4],
        [4, 5, 4, 5],
        [5, 6, 5, 6],
        [6, 7, 6, 7],
        [7, 8, 7, 8],
      ],
      sentences: [],
      pins: [],
    },
  ],
}

/** One edition of eight blocks, `alpha` in the first and the last, a figure closing the book. */
const closing = {
  title: 'Closing',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 8 }, (_, i) => {
        const named = i === 0 || i === 7 ? 'alpha ' : ''
        const text = `${named}Block ${i}. ${WORDS.repeat(6)}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
      figures: [{ before: 8, image: 0, alt: 'the end' }],
    },
  },
  alignments: [],
}

/** The search panel opened and `query` typed, its results listed. */
async function searched(query: string): Promise<HTMLInputElement> {
  await pressed('/')
  const box = document.querySelector(
    '[role="dialog"][aria-label="Search"] input[type="search"]',
  ) as HTMLInputElement
  await typeInto(box, query)
  await act(() => new Promise((done) => setTimeout(done, 200)))
  await settled()
  return box
}

/** Enter pressed in the search box, which goes to the next match, or with Shift to the one before. */
async function entered(box: HTMLInputElement, shiftKey = false) {
  await act(async () => {
    box.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        shiftKey,
        cancelable: true,
        bubbles: true,
      }),
    )
  })
  await settled()
}

/** The blocks with a match lit, by the block each lit mark is drawn in. */
const litBlocks = (text: HTMLElement) =>
  [...text.querySelectorAll('.book-found')].map((mark) =>
    Number(mark.closest<HTMLElement>('[data-block]')?.dataset.block),
  )

describe('a match gone to', () => {
  for (const paged of [false, true]) {
    it(`in a row taller than the screen is brought onto it, compared, ${paged ? 'paged' : 'scrolled'}`, async () => {
      const host = await opened(
        SET_IN,
        'en',
        2,
        { mode: 'compare', paged },
        {},
        tall,
      )
      const text = textOf(host)
      await searched(NEEDLE)
      const result = document.querySelector(
        '[role="dialog"][aria-label="Search"] .book-contents button',
      ) as HTMLElement
      await act(async () => {
        result.click()
      })
      await settled()
      const marks = [...text.querySelectorAll('.book-found')]
      expect(marks.map((mark) => mark.textContent)).toEqual([NEEDLE])
      const box = marks[0].getBoundingClientRect()
      const screen = text.getBoundingClientRect()
      if (paged) {
        expect(box.left).toBeGreaterThanOrEqual(screen.left)
        expect(box.right).toBeLessThanOrEqual(screen.right)
      } else {
        expect(box.top).toBeGreaterThanOrEqual(screen.top - 1)
        expect(box.bottom).toBeLessThanOrEqual(screen.bottom)
      }
    })
  }
})

describe('a match at the start of a block, compared', () => {
  /** The first result of a search for `query` picked. */
  async function goneTo(query: string) {
    await searched(query)
    const result = document.querySelector(
      '[role="dialog"][aria-label="Search"] .book-contents button',
    ) as HTMLElement
    await act(async () => {
      result.click()
    })
    await settled()
  }

  it('is brought onto the screen where its block is not the first in its row', async () => {
    const host = await opened(SET_IN, 'en', 0, { mode: 'compare' }, {}, merged)
    const text = textOf(host)
    expect(
      text.querySelector('[data-block="1"] [data-pick="left:3"]'),
    ).not.toBe(null)
    await goneTo(NEEDLE)
    const marks = [...text.querySelectorAll('.book-found')]
    expect(marks.map((mark) => mark.textContent)).toEqual([NEEDLE])
    const box = marks[0].getBoundingClientRect()
    const screen = text.getBoundingClientRect()
    expect(box.top).toBeGreaterThanOrEqual(screen.top - 1)
    expect(box.bottom).toBeLessThanOrEqual(screen.bottom)
  })

  it('shows its row from the start where it opens the row, the figure set before its words with it', async () => {
    const host = await opened(SET_IN, 'en', 0, { mode: 'compare' }, {})
    const text = textOf(host)
    await goneTo('Block 6.')
    const row = text.querySelector('[data-block="6"]') as HTMLElement
    expect(row.querySelector('.book-figure')).not.toBe(null)
    expect(
      Math.abs(startOf(text, row.getBoundingClientRect())),
    ).toBeLessThanOrEqual(1)
  })
})

/** What each edition of `marked` opens every fifth block with. */
const MARK: Record<string, string> = { en: 'zebra', ja: '目印' }

/** Two editions of `length` aligned blocks, every `every`th of the first forty opening with its `MARK`, each block's words said `times` over. */
function markedOf(length: number, times = 1, every = 5) {
  const hasMark = (i: number) => i < 40 && i % every === 0
  return {
    title: '印',
    hub: 'ja',
    editions: {
      ja: {
        label: 'Japanese',
        language: 'ja',
        blocks: Array.from({ length }, (_, i) => {
          const text = `${hasMark(i) ? MARK.ja : ''}${SAID.repeat(times)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
      en: {
        label: 'English',
        language: 'en',
        blocks: Array.from({ length }, (_, i) => {
          const named = hasMark(i) ? `${MARK.en} ` : ''
          const text = `${named}Block ${i}. ${WORDS.repeat(3 * times)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
    },
    alignments: [
      {
        to: 'en',
        links: Array.from({ length }, (_, i) => [i, i + 1, i, i + 1]),
        sentences: Array.from({ length }, (_, i) => [i, i + 1, i, i + 1]),
        pins: [],
      },
    ],
  }
}

const marked = markedOf(40)

/** `marked` running on unmarked for more than a screen, so its last mark can be read past. */
const runsOn = markedOf(80)

/** `marked` with blocks so long that a page turn moves the top block on by three at most. */
const longBlocks = markedOf(40, 4)

/** Each way of reading that a step is measured in: the four flows, and compared and paged. */
const READINGS = [
  ...FLOWS.map((flow) => ({
    name: flow.name,
    edition: flow.edition,
    paged: flow.paged,
    layout: { paged: flow.paged },
    typeset: { vertical: flow.vertical },
  })),
  {
    name: 'compared and paged',
    edition: 'en',
    paged: true,
    layout: { mode: 'compare', paged: true },
    typeset: {},
  },
]

describe('the first step after opening a book', () => {
  for (const view of [SET_IN, PHONE]) {
    for (const { name, edition, layout, typeset } of READINGS) {
      for (const { back, block } of [
        { back: false, block: 15 },
        { back: true, block: 10 },
      ]) {
        it(`goes to block ${block} from a place saved at block 12 on ${back ? 'Shift+Enter' : 'Enter'}, ${name}, ${view.name}`, async () => {
          const host = await opened(view, edition, 12, layout, typeset, marked)
          await entered(await searched(MARK[edition]), back)
          expect(litBlocks(textOf(host))).toEqual([block])
        })
      }
    }
  }
})

describe('a step from the reader’s place', () => {
  it('starts from where the reader has scrolled to, not the match last gone to', async () => {
    // The phone layout, where a panel opened lays nothing out again and so
    // leaves the anchor as the reader's scroll set it.
    const host = await opened(PHONE, 'en', 0, {}, {})
    const text = textOf(host)
    await entered(await searched('Block 1.'))
    expect(litBlocks(text)).toEqual([1])
    await pressed('Escape')
    const far = text.querySelector('[data-block="15"]') as HTMLElement
    await act(async () => {
      far.scrollIntoView({ block: 'start' })
    })
    await settled()
    expect(topOf(text).block).toBe(15)
    await entered(await searched('Block 2'))
    expect(litBlocks(text)).toEqual([20])
  })

  for (const { key, block } of [
    { key: ']', block: 0 },
    { key: '[', block: 7 },
  ]) {
    it(`counts a figure closing the book as past its last block, ${key}`, async () => {
      const host = await opened(SET_IN, 'en', 0, {}, {}, closing)
      const text = textOf(host)
      await entered(await searched('alpha'))
      expect(litBlocks(text)).toEqual([0])
      await pressed('Escape')
      await act(async () => {
        text.scrollTo({ top: text.scrollHeight })
      })
      await settled()
      expect(anchorOf(text).figure, 'the screen starts in the figure').toBe(1)
      await searched('alph')
      await pressed('Escape')
      await pressed(key)
      expect(litBlocks(text)).toEqual([block])
    })
  }
})

/** `marked` with block 22 of each edition a heading, for the contents to go to. */
const headed = {
  ...marked,
  editions: Object.fromEntries(
    Object.entries(marked.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((block, i) => ({
          ...block,
          heading: i === 22,
        })),
      },
    ]),
  ),
}

/** `marked` with English's mark in both editions, for a search of all of them. */
const sameMark = {
  ...marked,
  editions: {
    ...marked.editions,
    ja: {
      ...marked.editions.ja,
      blocks: marked.editions.ja.blocks.map((block) => {
        const text = block.text.replace(MARK.ja, MARK.en)
        return { ...block, text, sentences: [[0, text.length]] }
      }),
    },
  },
}

/** The reader opened on `marked` at block 12, gone to the match in block 15, the search panel closed. */
async function atTheMatch(
  view: View,
  { edition, layout, typeset }: (typeof READINGS)[number],
  sent: object = marked,
): Promise<HTMLElement> {
  const host = await opened(view, edition, 12, layout, typeset, sent)
  await entered(await searched(MARK[edition]))
  expect(litBlocks(textOf(host))).toEqual([15])
  await pressed('Escape')
  return host
}

/** A reader's own moves on until the screen starts at block `block` or past it. */
async function readTo(text: HTMLElement, paged: boolean, block: number) {
  if (!paged) {
    await act(async () => {
      text
        .querySelector(`[data-block="${block}"]`)
        ?.scrollIntoView({ block: 'start' })
    })
    await settled()
    return
  }
  for (let i = 0; i < 60 && topOf(text).block < block; i++) {
    await pressed('PageDown')
  }
}

/** A reader's own move back to before the match, which leaves it on the screen. */
async function backALittle(text: HTMLElement, paged: boolean) {
  if (paged) {
    await pressed('PageDown')
    await pressed('PageUp')
    return
  }
  await act(async () => addProgress(text, -40))
  await settled()
}

/** Whether character `at` of block `block` is on the screen, in its block or the first column of a compared row. */
function onScreen(text: HTMLElement, block: number, at = 0): boolean {
  const char = charBox(text, { block, at })
  const screen = text.getBoundingClientRect()
  return (
    char.left < screen.right &&
    char.right > screen.left &&
    char.top < screen.bottom &&
    char.bottom > screen.top
  )
}

describe('a step once the reader has moved from the match gone to', () => {
  for (const view of [SET_IN, PHONE]) {
    for (const reading of READINGS) {
      const where = `${reading.name}, ${view.name}`
      for (const { key, block } of [
        { key: ']', block: 25 },
        { key: '[', block: 20 },
      ]) {
        it(`goes from the place to block ${block} on ${key} with the match read past, ${where}`, async () => {
          const host = await atTheMatch(view, reading, longBlocks)
          const text = textOf(host)
          await readTo(text, reading.paged, 22)
          expect(topOf(text).block).toBeGreaterThanOrEqual(22)
          expect(topOf(text).block).toBeLessThan(25)
          expect(onScreen(text, 15), 'the match is off the screen').toBe(false)
          await pressed(key)
          expect(litBlocks(text)).toEqual([block])
        })
      }

      it(`goes from the place to block 25 on Enter in the box reopened, ${where}`, async () => {
        const host = await atTheMatch(view, reading, longBlocks)
        const text = textOf(host)
        await readTo(text, reading.paged, 22)
        expect(topOf(text).block).toBeGreaterThanOrEqual(22)
        expect(topOf(text).block).toBeLessThan(25)
        await pressed('/')
        await entered(
          document.querySelector(
            '[role="dialog"][aria-label="Search"] input[type="search"]',
          ) as HTMLInputElement,
        )
        expect(litBlocks(text)).toEqual([25])
      })

      it(`steps on to block 20 while the match is on the screen and the place before it, ${where}`, async () => {
        const host = await atTheMatch(view, reading)
        const text = textOf(host)
        await backALittle(text, reading.paged)
        expect(onScreen(text, 15), 'the match is on the screen').toBe(true)
        expect(anchorOf(text).block, 'the place is before it').toBeLessThan(15)
        await pressed(']')
        expect(litBlocks(text)).toEqual([20])
      })
    }
  }

  for (const reading of READINGS) {
    for (const { key, block } of [
      { key: ']', block: 0 },
      { key: '[', block: 35 },
    ]) {
      it(`goes round to block ${block} on ${key} from past the last match, ${reading.name}`, async () => {
        const host = await atTheMatch(SET_IN, reading, runsOn)
        const text = textOf(host)
        await readTo(text, reading.paged, 36)
        expect(topOf(text).block).toBeGreaterThanOrEqual(36)
        expect(onScreen(text, 35), 'the last match is off the screen').toBe(
          false,
        )
        await pressed(key)
        expect(litBlocks(text)).toEqual([block])
      })
    }

    it(`goes from the place the contents went to, ${reading.name}`, async () => {
      const host = await atTheMatch(SET_IN, reading, headed)
      const text = textOf(host)
      await pressed('t')
      await act(async () => {
        document
          .querySelector<HTMLButtonElement>(
            '[role="dialog"][aria-label="Contents"] ol button',
          )
          ?.click()
      })
      await settled()
      await pressed('Escape')
      expect(onScreen(text, 15), 'the match is off the screen').toBe(false)
      await pressed(']')
      expect(litBlocks(text)).toEqual([25])
    })
  }

  it('goes from the place in the edition read once that is swapped by hand, all editions', async () => {
    const host = await opened(SET_IN, 'en', 12, {}, {}, sameMark)
    const box = await searched(MARK.en)
    await act(async () => {
      document
        .querySelector<HTMLInputElement>(
          '[role="dialog"][aria-label="Search"] input[type="checkbox"]',
        )
        ?.click()
    })
    await settled()
    await entered(box)
    expect(litBlocks(textOf(host))).toEqual([15])
    expect(textOf(host).lang).toBe('en')
    await pressed('Escape')
    await pressed(' ')
    expect(textOf(host).lang, 'the edition swapped to is read').toBe('ja')
    await pressed(']')
    expect(textOf(host).lang, 'the step stays in the edition read').toBe('ja')
    expect(litBlocks(textOf(host))).toEqual([15])
  })
})

/** What block 2 of `longRow` holds between its two marks: several screens of a compared column. */
const BETWEEN = WORDS.repeat(60)

/** Two editions of five blocks, block 2's row longer than a screen, opening with `opening` and then holding two marks. */
function longRow(opening: string) {
  const en = [
    `${MARK.en} ${WORDS}`,
    WORDS,
    `${opening}${MARK.en} ${BETWEEN}${MARK.en} ${BETWEEN}`,
    WORDS,
    `${MARK.en} ${WORDS}`,
  ]
  const blocks = (texts: string[]) =>
    texts.map((text) => ({
      text,
      heading: false,
      sentences: [[0, text.length]],
    }))
  return {
    title: 'Long row',
    hub: 'en',
    editions: {
      en: { label: 'English', language: 'en', blocks: blocks(en) },
      fr: {
        label: 'French',
        language: 'fr',
        blocks: blocks(en.map((_, i) => `Bloc ${i}.`)),
      },
    },
    alignments: [
      {
        to: 'fr',
        links: en.map((_, i) => [i, i + 1, i, i + 1]),
        sentences: en.map((_, i) => [i, i + 1, i, i + 1]),
        pins: [],
      },
    ],
  }
}

/** The one match lit: the block of the column read it is in, and its first character there. */
function litAt(text: HTMLElement) {
  const marks = text.querySelectorAll('.book-found')
  expect(marks.length, 'one match is lit').toBe(1)
  const words = marks[0].closest('[data-pick]') as HTMLElement
  const before = document.createRange()
  before.setStart(words, 0)
  before.setEndBefore(marks[0])
  return { pick: words.dataset.pick, at: before.toString().length }
}

describe('a step from inside a compared row longer than a screen', () => {
  for (const view of [SET_IN, PHONE]) {
    for (const paged of [true, false]) {
      for (const opening of ['', 'Block two says ']) {
        const first = opening.length
        const second = first + MARK.en.length + 1 + BETWEEN.length
        for (const { key, at } of [
          { key: ']', at: second },
          { key: '[', at: first },
        ]) {
          it(`goes to the mark at ${at} on ${key} with the mark at ${first} read past, ${paged ? 'paged' : 'scrolled'}, ${view.name}`, async () => {
            const host = await opened(
              view,
              'en',
              2,
              { mode: 'compare', paged },
              {},
              longRow(opening),
            )
            const text = textOf(host)
            await entered(await searched(MARK.en))
            expect(litAt(text)).toEqual({ pick: 'left:2', at: first })
            await pressed('Escape')
            if (paged) await pressed('PageDown')
            else {
              await act(async () => addProgress(text, text.clientHeight))
              await settled()
            }
            const place = (char: number) =>
              startOf(text, charBox(text, { block: 2, at: char }))
            expect(onScreen(text, 2, first), 'the first is off').toBe(false)
            expect(place(first), 'the first is behind').toBeLessThan(0)
            expect(onScreen(text, 2, second), 'the second is off').toBe(false)
            expect(place(second), 'the second is ahead').toBeGreaterThan(0)
            await pressed(key)
            expect(litAt(text)).toEqual({ pick: 'left:2', at })
          })
        }
      }
    }
  }

  for (const { key, block } of [
    { key: ']', block: 3 },
    { key: '[', block: 2 },
  ]) {
    it(`goes from the row’s start on ${key} where the page holds only the other column’s words`, async () => {
      const layout = { mode: 'compare', paged: true }
      const host = await opened(SET_IN, 'en', 3, layout, {}, lopsided)
      const text = textOf(host)
      await searched('Block')
      await pressed('Escape')
      await pressed('PageDown')
      const row = text.querySelector('[data-block="3"]') as HTMLElement
      const read = row.querySelector('[data-pick="left:3"]') as HTMLElement
      expect(startOf(text, row.getBoundingClientRect())).toBeLessThan(0)
      expect(read.getBoundingClientRect().right).toBeLessThan(
        text.getBoundingClientRect().left,
      )
      expect(anchorOf(text).read, 'none of the column is here').toBeUndefined()
      await pressed(key)
      expect(litBlocks(text)).toEqual([block])
    })
  }
})

/** `merged` with each block written as numbered sentences, enough that the row of blocks 1 to 3 runs on over pages. */
const sentencedMerged = {
  ...merged,
  editions: Object.fromEntries(
    Object.entries(merged.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((_, i) => {
          const said = Array.from(
            { length: 80 },
            (_, k) =>
              `Block ${i} says line ${k} on a bright cold day in April. `,
          )
          const sentences: [number, number][] = []
          for (const one of said) {
            const start = sentences.at(-1)?.[1] ?? 0
            sentences.push([start, start + one.length])
          }
          return { text: said.join(''), heading: false, sentences }
        }),
      },
    ]),
  ),
}

describe('the keys that act at the top of the page, on a later page of a merged row, compared and paged', () => {
  const compared = { mode: 'compare', paged: true }
  const blockTwo = sentencedMerged.editions.en.blocks[2]

  /** Opened at the row's first block, and turned on to a page that starts inside block 2, past its first sentence. */
  async function inBlockTwo(view: View) {
    const host = await opened(view, 'en', 1, compared, {}, sentencedMerged)
    const text = textOf(host)
    const [, firstEnd] = blockTwo.sentences[0]
    await turnedUntil(text, (shown) => {
      const read = anchorOf(shown).read
      return read?.block === 2 && read.at >= firstEnd
    })
    const read = anchorOf(text).read
    expect(read?.block, 'block 2 is carried onto the page').toBe(2)
    expect(read?.at, 'past its first sentence').toBeGreaterThanOrEqual(firstEnd)
    expect(topOf(text).block, 'the row starts with block 1').toBe(1)
    return { host, text, at: read?.at ?? 0 }
  }

  for (const view of [SET_IN, PHONE]) {
    it(`reads aloud on r from the sentence the page starts with, not the row's first, ${view.name}`, async () => {
      speaksNothing()
      const { at } = await inBlockTwo(view)
      const [from, to] = blockTwo.sentences.find(([, end]) => end > at) ?? []
      await pressed('r')
      expect(spoken.map((u) => u.text)).toEqual([blockTwo.text.slice(from, to)])
    })

    it(`notes on n the block the page starts in, not the row's first, ${view.name}`, async () => {
      await inBlockTwo(view)
      await pressed('n')
      await act(() => new Promise((done) => setTimeout(done, 1000)))
      const written = vi
        .mocked(fetch)
        .mock.calls.filter(
          ([url, init]) =>
            url === '/api/works/w/notes' && init?.method === 'PUT',
        )
        .map(([, init]) => JSON.parse(String(init?.body)))
      expect(
        written
          .at(-1)
          ?.notes.map((n: { edition: string; slot: number }) => [
            n.edition,
            n.slot,
          ]),
      ).toEqual([['en', 2]])
    })
  }
})

describe('r reading alone and paged, on a page that starts past a block’s first sentence', () => {
  const alone = { mode: 'scroll', paged: true }
  const { blocks } = sentencedMerged.editions.en

  // The edition's last block is the one the clamp at a compared last row of
  // only the other columns' blocks names, so a place inside it must pass the
  // clamp as it is, sentence and all.
  for (const [block, where] of [
    [2, 'a block before the last'],
    [blocks.length - 1, 'the edition’s last block'],
  ] as const) {
    it(`reads from the sentence the page starts with, not the first of its block, in ${where}`, async () => {
      speaksNothing()
      const host = await opened(SET_IN, 'en', block, alone, {}, sentencedMerged)
      const text = textOf(host)
      const [, firstEnd] = blocks[block].sentences[0]
      await turnedUntil(text, (shown) => {
        const top = topOf(shown)
        return top.block === block && top.at >= firstEnd
      })
      const top = topOf(text)
      expect(top.block, `block ${block} is carried onto the page`).toBe(block)
      expect(top.at, 'past its first sentence').toBeGreaterThanOrEqual(firstEnd)
      const [from, to] =
        blocks[block].sentences.find(([, end]) => end > top.at) ?? []
      await pressed('r')
      expect(spoken.map((u) => u.text)).toEqual([
        blocks[block].text.slice(from, to),
      ])
    })
  }
})

/** How many blocks `figured` has. */
const FIGURED = 24

/** Two editions of blocks of differing lengths, each opening with its number and every second one set after a figure. */
const figured = {
  title: 'Figured',
  hub: 'en',
  editions: Object.fromEntries(
    ['en', 'fr'].map((key) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: FIGURED }, (_, i) => {
          const text = `Block ${i}. ${WORDS.repeat(1 + ((i * 7) % 6))}${WORDS.slice(0, (i * 13) % 74)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
        figures: Array.from({ length: FIGURED / 2 - 1 }, (_, i) => ({
          before: (i + 1) * 2,
          image: 0,
          alt: 'a map',
        })),
      },
    ]),
  ),
  alignments: [
    {
      to: 'fr',
      links: Array.from({ length: FIGURED }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: FIGURED }, (_, i) => [
        i,
        i + 1,
        i,
        i + 1,
      ]),
      pins: [],
    },
  ],
}

// Short enough to share a page with what is before it and tall enough to
// leave no room there for the first line of the words it is set before.
const MID_HEIGHT_FIGURES = `
  .book-text[data-paged] .book-figure { break-before: auto !important }
  .book-text .book-figure img { width: 300px !important; height: 300px !important; aspect-ratio: auto !important }
`

describe('a walk through the matches, compared and paged, past rows that open with a figure', () => {
  const midHeight = document.createElement('style')
  midHeight.textContent = MID_HEIGHT_FIGURES
  beforeAll(() => {
    document.head.append(midHeight)
  })
  afterAll(() => {
    midHeight.remove()
  })

  it('goes through every match on ] and back through them on [', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 0, layout, {}, figured)
    const text = textOf(host)
    await entered(await searched('Block'))
    await pressed('Escape')
    expect(litBlocks(text)).toEqual([0])
    let pushed = 0
    for (let block = 1; block < FIGURED; block++) {
      await pressed(']')
      expect(litBlocks(text), `] to block ${block}`).toEqual([block])
      if (!onScreen(text, block)) pushed++
    }
    expect(pushed, 'matches a figure pushed off their page').toBeGreaterThan(0)
    for (let block = FIGURED - 2; block >= 0; block--) {
      await pressed('[')
      expect(litBlocks(text), `[ to block ${block}`).toEqual([block])
    }
  })

  it('goes on through every match on ] with a panel opened and closed at each', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 0, layout, {}, figured)
    const text = textOf(host)
    await entered(await searched('Block'))
    await pressed('Escape')
    let pushed = 0
    for (let block = 1; block < FIGURED; block++) {
      if (!onScreen(text, block - 1)) pushed++
      await pressed('t')
      await pressed('Escape')
      await pressed(']')
      expect(litBlocks(text), `] to block ${block}`).toEqual([block])
    }
    expect(pushed, 'matches a figure pushed off their page').toBeGreaterThan(0)
  })

  it('keeps a match a figure pushed off its page at its row’s start through a panel', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 0, layout, {}, figured)
    const text = textOf(host)
    await entered(await searched('Block'))
    await pressed('Escape')
    let pushed = 0
    let apart = 0
    for (let block = 1; block < FIGURED; block++) {
      await pressed(']')
      if (onScreen(text, block)) continue
      pushed++
      const first = firstRead(text)
      const before = text.scrollLeft
      await pressed('t')
      expect(panelOf(host), `block ${block}`).toBe('contents')
      const row = text.querySelector(`[data-block="${block}"]`) as HTMLElement
      const start = pageAt(text, row.getClientRects()[0].left + 1)
      expect(pageShown(text), `block ${block}`).toBe(start)
      if (first && pageOfRange(text, first) !== start) apart++
      await pressed('Escape')
      expect(text.scrollLeft, `block ${block}, closed`).toBe(before)
    }
    expect(pushed, 'matches a figure pushed off their page').toBeGreaterThan(0)
    // Where the page's first character stays on the row's start, this could
    // not tell a panel keeping the match from one keeping that character.
    expect(apart, 'panels the two part on').toBeGreaterThan(0)
  }, 120_000)
})

describe('a match a figure at the top of its page pushed onto the next', () => {
  it('] lights a match pushed off its page again once the edition read is swapped away and back', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 0, layout, {}, figured)
    const text = textOf(host)
    await entered(await searched('Block'))
    await pressed('Escape')
    let block = 0
    while (
      block < FIGURED - 1 &&
      (onScreen(text, block) || topOf(text).block !== block)
    ) {
      await pressed(']')
      block++
    }
    expect(onScreen(text, block), 'a figure pushed it off').toBe(false)
    expect(topOf(text).block, 'the page starts in its row').toBe(block)
    const read = () =>
      text.querySelector<HTMLElement>('.book-cell')?.dataset.column
    await pressed(' ')
    expect(read()).toBe('fr')
    await pressed(' ')
    expect(read()).toBe('en')
    await pressed(']')
    expect(litBlocks(text)).toEqual([block])
  })
})

/** Sixteen compared rows, each opening with `MARK.en`, long enough that some carry over onto the next page. */
const carried = {
  title: 'Carried',
  hub: 'en',
  editions: Object.fromEntries(
    ['en', 'fr'].map((key) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: 16 }, (_, i) => {
          const text = `${MARK.en} ${WORDS.repeat(6 + (i % 5))}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
    ]),
  ),
  alignments: [
    {
      to: 'fr',
      links: Array.from({ length: 16 }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: 16 }, (_, i) => [i, i + 1, i, i + 1]),
      pins: [],
    },
  ],
}

describe('a match read past, compared, and the text placed again', () => {
  /** Where the second mark of `longRow('')` starts in block 2. */
  const SECOND = MARK.en.length + 1 + BETWEEN.length

  /** A page turned, or a screen scrolled, on. */
  async function movedOn(text: HTMLElement, paged: boolean) {
    if (paged) await pressed('PageDown')
    else {
      await act(async () => addProgress(text, text.clientHeight))
      await settled()
    }
  }

  /** The reader on `longRow('')`, gone to the mark at character 0 of block 2, the search panel still open. */
  async function atTheRowsStart(paged: boolean, sent: object = longRow('')) {
    const layout = { mode: 'compare', paged }
    const host = await opened(SET_IN, 'en', 2, layout, {}, sent)
    const text = textOf(host)
    await entered(await searched(MARK.en))
    expect(litAt(text)).toEqual({ pick: 'left:2', at: 0 })
    return { host, text }
  }

  for (const paged of [true, false]) {
    const how = paged ? 'paged' : 'scrolled'

    it(`[ lights the mark read past after a panel is opened and closed, ${how}`, async () => {
      const { host, text } = await atTheRowsStart(paged)
      await pressed('Escape')
      await movedOn(text, paged)
      expect(onScreen(text, 2, 0), 'the mark is off').toBe(false)
      await pressed('t')
      expect(panelOf(host)).toBe('contents')
      await pressed('Escape')
      expect(onScreen(text, 2, 0), 'the mark is still off').toBe(false)
      await pressed('[')
      expect(litAt(text)).toEqual({ pick: 'left:2', at: 0 })
    })

    it(`[ lights the mark read past with the search panel open, then closed, ${how}`, async () => {
      const { host, text } = await atTheRowsStart(paged)
      await movedOn(text, paged)
      await movedOn(text, paged)
      await pressed('Escape')
      expect(panelOf(host)).toBeUndefined()
      expect(onScreen(text, 2, 0), 'the mark is off').toBe(false)
      await pressed('[')
      expect(litAt(text)).toEqual({ pick: 'left:2', at: 0 })
    })

    it(`] goes on to block 4 with both marks read past and a panel opened and closed, ${how}`, async () => {
      const { host, text } = await atTheRowsStart(paged)
      await pressed('Escape')
      const second = () =>
        startOf(text, charBox(text, { block: 2, at: SECOND }))
      for (let i = 0; i < 80 && second() >= 0; i++) await movedOn(text, paged)
      expect(second(), 'the second mark is behind').toBeLessThan(0)
      expect(topOf(text).block, 'still in the row').toBe(2)
      await pressed('t')
      expect(panelOf(host)).toBe('contents')
      await pressed('Escape')
      await pressed(']')
      expect(litBlocks(text)).toEqual([4])
    })
  }

  it('[ lights the mark read past after an image loads in the row, paged', async () => {
    const { text } = await atTheRowsStart(true)
    await pressed('Escape')
    await pressed('PageDown')
    expect(onScreen(text, 2, 0), 'the mark is off').toBe(false)
    const row = text.querySelector('[data-block="2"]') as HTMLElement
    const image = new Image()
    image.style.cssText = 'display: block; width: 1px; height: 1px'
    await act(async () => {
      row.querySelector('.book-cell')?.append(image)
      image.dispatchEvent(new Event('load'))
    })
    await settled()
    expect(onScreen(text, 2, 0), 'the mark is still off').toBe(false)
    await pressed('[')
    expect(litAt(text)).toEqual({ pick: 'left:2', at: 0 })
  })

  it('[ lights the mark of a row carried onto the next page after a panel is opened and closed', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 2, layout, {}, carried)
    const text = textOf(host)
    await entered(await searched(MARK.en))
    expect(litBlocks(text)).toEqual([2])
    await pressed('Escape')
    for (let i = 0; i < 4 && onScreen(text, 2, 0); i++) {
      await pressed('PageDown')
    }
    expect(anchorOf(text).block, 'on the row carried over').toBe(2)
    expect(onScreen(text, 2, 0), 'its mark is behind').toBe(false)
    await pressed('t')
    expect(panelOf(host)).toBe('contents')
    await pressed('Escape')
    expect(onScreen(text, 2, 0), 'its mark is still behind').toBe(false)
    await pressed('[')
    expect(litBlocks(text)).toEqual([2])
  })

  it('] goes from the row’s start once the contents go there from the second mark', async () => {
    const sent = longRow('')
    sent.editions.en.blocks[2].heading = true
    const { text } = await atTheRowsStart(true, sent)
    await pressed(']')
    expect(litAt(text)).toEqual({ pick: 'left:2', at: SECOND })
    await pressed('t')
    await act(async () => {
      document
        .querySelector<HTMLButtonElement>(
          '[role="dialog"][aria-label="Contents"] ol button',
        )
        ?.click()
    })
    await settled()
    await pressed('Escape')
    expect(onScreen(text, 2, SECOND), 'the second mark is off').toBe(false)
    await pressed(']')
    expect(litAt(text)).toEqual({ pick: 'left:2', at: 0 })
  })
})

/** Two editions compared, block 3's row running on over pages in its second column alone. */
const lopsided = {
  title: 'Lopsided',
  hub: 'en',
  editions: Object.fromEntries(
    ['en', 'fr'].map((key) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: 8 }, (_, i) => {
          const long = i === 3 && key === 'fr'
          const text = `Block ${i}. ${WORDS.repeat(long ? 90 : 3)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
        figures: [],
      },
    ]),
  ),
  alignments: [
    {
      to: 'fr',
      links: Array.from({ length: 8 }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: 8 }, (_, i) => [i, i + 1, i, i + 1]),
      pins: [],
    },
  ],
}

/** The page, from 0, that the screen position `x` is on. */
const pageAt = (text: HTMLElement, x: number) =>
  Math.floor(
    (x - text.getBoundingClientRect().left + text.scrollLeft) /
      text.clientWidth,
  )

const pageShown = (text: HTMLElement) =>
  Math.round(text.scrollLeft / text.clientWidth)

/** The page, from 0, that `range` is on as the text is now laid out. */
const pageOfRange = (text: HTMLElement, range: Range) =>
  pageAt(text, range.getBoundingClientRect().left + 1)

/**
 * The first character of the column read on the page shown, as a `Range`,
 * which stays on that character as the text is laid out again.
 */
function firstRead(text: HTMLElement): Range | undefined {
  const screen = text.getBoundingClientRect()
  for (const words of text.querySelectorAll('[data-pick^="left:"]')) {
    for (const node of wordNodes(words)) {
      for (let i = 0; i < node.length; i++) {
        const range = document.createRange()
        range.setStart(node, i)
        range.setEnd(node, i + 1)
        const box = range.getBoundingClientRect()
        const seen = box.left >= screen.left && box.right <= screen.right
        if (box.width > 0 && seen) return range
      }
    }
  }
  return undefined
}

describe('the character read, compared and paged, through a docked panel', () => {
  const COMPARED = { mode: 'compare', paged: true }

  // The widest window's text is set larger, so its rows still run over pages.
  for (const { view, typeset } of [
    { view: SET_IN, typeset: {} },
    {
      view: { name: '1366 by 768', width: 1366, height: 768, setIn: true },
      typeset: {},
    },
    {
      view: { name: '1920 by 1080', width: 1920, height: 1080, setIn: true },
      typeset: { textSize: 30 },
    },
  ]) {
    for (const edition of ['en', 'ja']) {
      it(`keeps the first character of the column read on the page through a panel, compared and paged, ${edition}, ${view.name}`, async () => {
        const host = await opened(view, edition, 4, COMPARED, typeset)
        const text = textOf(host)
        let sampled = 0
        let apart = 0
        for (let turn = 0; turn < 16; turn++) {
          const row = await onACarriedPage(text)
          if (!row) break
          const first = firstRead(text)
          if (!first) continue
          const into = anchorOf(text).into ?? 0
          const before = text.scrollLeft
          for (const time of ['opened', 'opened again']) {
            await pressed('t')
            expect(panelOf(host)).toBe('contents')
            const held = pageOfRange(text, first)
            expect(pageShown(text), `turn ${turn}, ${time}`).toBe(held)
            const pages = row.getClientRects()
            const begun = pageAt(text, pages[0].left + 1)
            const named = begun + Math.floor(into * pages.length)
            if (time === 'opened' && named !== held) apart++
            await pressed('Escape')
            expect(text.scrollLeft, `turn ${turn}, ${time}, closed`).toBe(
              before,
            )
          }
          sampled++
        }
        expect(sampled, 'pages a row carries over onto').toBeGreaterThan(0)
        // Where the row's fraction names the character's page every time, the
        // sweep could not tell the character from the fraction.
        if (edition === 'en') {
          expect(apart, 'pages the fraction gets wrong').toBeGreaterThan(0)
        }
      }, 240_000)
    }
  }

  for (const view of [SET_IN, PHONE]) {
    it(`holds the row’s first character on the page the row starts on, ${view.name}`, async () => {
      const host = await opened(view, 'en', 0, COMPARED, {})
      expect(anchorOf(textOf(host)).read).toEqual({ block: 0, at: 0 })
    })
  }

  it('goes by the row where the page holds only the other column’s words', async () => {
    const host = await opened(SET_IN, 'en', 3, COMPARED, {}, lopsided)
    const text = textOf(host)
    await pressed('PageDown')
    const screen = text.getBoundingClientRect()
    const row = text.querySelector('[data-block="3"]') as HTMLElement
    const read = row.querySelector('[data-pick="left:3"]') as HTMLElement
    const other = row.querySelector('[data-pick="right:3"]') as HTMLElement
    const next = text.querySelector('[data-block="4"]') as HTMLElement
    expect(startOf(text, row.getBoundingClientRect())).toBeLessThan(0)
    expect(read.getBoundingClientRect().right).toBeLessThan(screen.left)
    expect(
      [...other.getClientRects()].some(
        (box) => box.left < screen.right && box.right > screen.left,
      ),
    ).toBe(true)
    expect(startOf(text, next.getBoundingClientRect())).toBeGreaterThanOrEqual(
      text.clientWidth,
    )
    expect(anchorOf(text).read).toBeUndefined()
    const into = anchorOf(text).into ?? 0
    const before = text.scrollLeft
    await pressed('t')
    expect(panelOf(host)).toBe('contents')
    const box = row.getBoundingClientRect()
    const begun = box.left - screen.left + text.scrollLeft
    expect(pageShown(text), 'the page the row’s fraction names').toBe(
      Math.floor((begun + into * box.width + 0.5) / text.clientWidth),
    )
    await pressed('Escape')
    expect(text.scrollLeft).toBe(before)
  })

  /** `lopsided` with block 3 `times` long in the column read and two and a half times that in the other. */
  const longerOpposite = (times: number) => ({
    ...lopsided,
    editions: Object.fromEntries(
      Object.entries(lopsided.editions).map(([key, edition]) => [
        key,
        {
          ...edition,
          blocks: edition.blocks.map((block, i) => {
            if (i !== 3) return block
            const length = key === 'en' ? times : Math.ceil(times * 2.5)
            const text = `Block 3. ${WORDS.repeat(length)}`
            return { text, heading: false, sentences: [[0, text.length]] }
          }),
        },
      ]),
    ),
  })

  it('goes back to a page of only the other column’s words where the panel’s page holds the column read', async () => {
    const sent = longerOpposite(38)
    const host = await opened(SET_IN, 'en', 3, COMPARED, {}, sent)
    const text = textOf(host)
    let held = 0
    for (let turn = 0; turn < 8; turn++) {
      await pressed('PageDown')
      const anchor = anchorOf(text)
      if (anchor.block !== 3 || anchor.read !== undefined) continue
      const before = text.scrollLeft
      await pressed('t')
      expect(panelOf(host), `turn ${turn}`).toBe('contents')
      if (firstRead(text)) held++
      await pressed('Escape')
      expect(text.scrollLeft, `turn ${turn}, closed`).toBe(before)
    }
    // Set in, the column read runs on further down the row, so the page the
    // row's fraction names holds some of it where the page turned to held none.
    expect(held, 'panels showing the column read').toBeGreaterThan(0)
  })

  /** Two pages on into `tall`'s long row, where its start and the character read are pages apart. */
  async function deepInTheRow(host: HTMLElement) {
    const text = textOf(host)
    await pressed('PageDown')
    await pressed('PageDown')
    const row = text.querySelector('[data-block="5"]') as HTMLElement
    expect(startOf(text, row.getBoundingClientRect())).toBeLessThan(
      -text.clientWidth,
    )
    return row
  }

  /** Fails unless `row` starts on the page shown. */
  function expectStartsOnThePage(text: HTMLElement, row: HTMLElement) {
    const start = startOf(text, row.getBoundingClientRect())
    expect(start).toBeGreaterThanOrEqual(0)
    expect(start).toBeLessThan(text.clientWidth)
  }

  it('goes to the row’s start as the text size changes on a page it carries over onto', async () => {
    const host = await opened(SET_IN, 'en', 5, COMPARED, {}, tall)
    const row = await deepInTheRow(host)
    await pressed('=')
    expectStartsOnThePage(textOf(host), row)
  })

  it('shows the counterpart block’s row where the edition read is swapped with a panel open', async () => {
    const host = await opened(SET_IN, 'en', 5, COMPARED, {}, tall)
    const text = textOf(host)
    await deepInTheRow(host)
    await pressed('t')
    expect(panelOf(host)).toBe('contents')
    await pressed(' ')
    const first = text.querySelector('.book-cell') as HTMLElement
    expect(first.dataset.column, 'the edition swapped to is read').toBe('fr')
    await pressed('Escape')
    expect(panelOf(host)).toBeUndefined()
    const row = text.querySelector('[data-block="5"]') as HTMLElement
    expectStartsOnThePage(text, row)
  })
})

describe('the character read, compared and paged, as an image loads', () => {
  /** `lopsided` with its long second column `times` over, which sets how far onto the next row's page it carries. */
  const carried = (times: number) => ({
    ...lopsided,
    editions: {
      ...lopsided.editions,
      fr: {
        ...lopsided.editions.fr,
        blocks: lopsided.editions.fr.blocks.map((block, i) => {
          if (i !== 3) return block
          const text = `Block 3. ${WORDS.repeat(times)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
    },
  })

  for (const times of [60, 64, 68, 72, 76, 80, 84, 88]) {
    it(`stays on the page holding it as an image loads in the row after the anchor’s, a second column ${times} long`, async () => {
      const layout = { mode: 'compare', paged: true }
      const host = await opened(SET_IN, 'en', 3, layout, {}, carried(times))
      const text = textOf(host)
      const onTheNextRow = () => {
        const { block, read } = anchorOf(text)
        return block === 3 && read?.block === 4
      }
      for (let turn = 0; turn < 30 && !onTheNextRow(); turn++) {
        await pressed('PageDown')
      }
      expect(onTheNextRow(), 'row 3 carries onto row 4’s page').toBe(true)
      const first = firstRead(text) as Range
      const before = pageShown(text)
      const row = text.querySelector('[data-block="4"]') as HTMLElement
      const image = new Image()
      image.style.cssText = 'display: block; width: 100%; height: 600px'
      await act(async () => {
        row.querySelector('.book-cell')?.prepend(image)
        image.dispatchEvent(new Event('load'))
      })
      await settled()
      const held = pageOfRange(text, first)
      expect(held, 'the image pushed the character on').toBeGreaterThan(before)
      expect(pageShown(text)).toBe(held)
    })
  }

  for (const view of [PHONE, SET_IN]) {
    it(`stays on the page holding it as an image gains height in the row carried over, ${view.name}`, async () => {
      const host = await opened(
        view,
        'en',
        4,
        { mode: 'compare', paged: true },
        {},
      )
      const text = textOf(host)
      let sampled = 0
      let apart = 0
      for (let turn = 0; turn < 16; turn++) {
        const was = text.scrollLeft
        const row = await onACarriedPage(text)
        // The last page turns no further, so no scroll retakes the anchor
        // and it is still the one the image before this was placed by.
        if (!row || text.scrollLeft === was) break
        const first = firstRead(text)
        if (!first) continue
        const into = anchorOf(text).into ?? 0
        if (view.setIn) {
          await pressed('t')
          expect(panelOf(host)).toBe('contents')
        }
        const image = new Image()
        image.style.cssText = 'display: block; width: 100%; height: 420px'
        await act(async () => {
          row.querySelector('.book-cell')?.prepend(image)
          image.dispatchEvent(new Event('load'))
        })
        await settled()
        const held = pageOfRange(text, first)
        expect(pageShown(text), `turn ${turn}`).toBe(held)
        const box = row.getBoundingClientRect()
        const begun =
          box.left - text.getBoundingClientRect().left + text.scrollLeft
        const named = Math.floor(
          (begun + into * box.width + 0.5) / text.clientWidth,
        )
        if (named !== held) apart++
        await act(async () => {
          image.remove()
        })
        await settled()
        if (view.setIn) await pressed('Escape')
        sampled++
      }
      expect(sampled, 'pages a row carries over onto').toBeGreaterThan(0)
      // Where the row's fraction names the character's page every time, this
      // could not tell the character from the fraction.
      expect(apart, 'pages the fraction gets wrong').toBeGreaterThan(0)
    }, 240_000)
  }
})

describe('a step from the character read, compared and paged, with the search panel open', () => {
  const unit = `${MARK.en} ${WORDS.repeat(2)}`
  const blocks = (texts: string[]) =>
    texts.map((text) => ({
      text,
      heading: false,
      sentences: [[0, text.length]],
    }))
  /** Block 1's row runs on over pages in the column read, a mark opening every `unit` of it. */
  const dense = {
    title: 'Dense',
    hub: 'en',
    editions: {
      en: {
        label: 'English',
        language: 'en',
        blocks: blocks([WORDS, unit.repeat(80), WORDS]),
      },
      fr: {
        label: 'French',
        language: 'fr',
        blocks: blocks(['Bloc 0.', 'Bloc 1.', 'Bloc 2.']),
      },
    },
    alignments: [
      {
        to: 'fr',
        links: [0, 1, 2].map((i) => [i, i + 1, i, i + 1]),
        sentences: [0, 1, 2].map((i) => [i, i + 1, i, i + 1]),
        pins: [],
      },
    ],
  }

  /** How many characters of block 1's words in the column read come before `range`. */
  function charsBefore(text: HTMLElement, range: Range): number {
    const words = text.querySelector('[data-pick="left:1"]') as HTMLElement
    const passed = document.createRange()
    passed.setStart(words, 0)
    passed.setEnd(range.startContainer, range.startOffset)
    return passed.toString().length
  }

  /** The mark a step lands on from character `at` of block 1's words. */
  function markFrom(at: number, back: boolean) {
    const after = Math.ceil(at / unit.length) * unit.length
    return { pick: 'left:1', at: back ? after - unit.length : after }
  }

  /** `dense` after an opening row `times` long, which sets how far down a page block 1's row starts. */
  const startingLate = (times: number) => ({
    ...dense,
    editions: {
      ...dense.editions,
      en: {
        ...dense.editions.en,
        blocks: blocks([WORDS.repeat(times), unit.repeat(40), WORDS]),
      },
    },
  })

  for (const times of [12, 30]) {
    for (const back of [false, true]) {
      it(`goes from it ${back ? 'back' : 'on'} where its row starts on the page the panel shows, an opening ${times} long`, async () => {
        const layout = { mode: 'compare', paged: true }
        const sent = startingLate(times)
        const host = await opened(SET_IN, 'en', 0, layout, {}, sent)
        const text = textOf(host)
        for (let i = 0; i < 6 && anchorOf(text).read?.block !== 1; i++) {
          await pressed('PageDown')
        }
        const read = anchorOf(text).read
        expect(read?.block, 'on a page the row carries over onto').toBe(1)
        const box = await searched(MARK.en)
        // The row's start is on the page too, so only the character read says
        // the mark opening the row has been passed.
        const row = text.querySelector('[data-block="1"]') as HTMLElement
        const start = startOf(text, row.getClientRects()[0])
        expect(start).toBeGreaterThanOrEqual(0)
        expect(start).toBeLessThan(text.clientWidth)
        await entered(box, back)
        expect(litAt(text)).toEqual(markFrom(read?.at ?? 0, back))
      })
    }
  }

  for (const { key, back } of [
    { key: ']', back: false },
    { key: '[', back: true },
  ]) {
    it(`goes from the screen on ${key} once the character read is pushed off it`, async () => {
      const layout = { mode: 'compare', paged: true }
      const host = await opened(PHONE, 'en', 1, layout, {}, dense)
      const text = textOf(host)
      await searched(MARK.en)
      await pressed('Escape')
      await pressed('PageDown')
      await pressed('PageDown')
      const read = anchorOf(text).read
      expect(read?.block, 'on a page the row carries over onto').toBe(1)
      const character = firstRead(text) as Range
      const tall = document.createElement('div')
      tall.style.height = '600px'
      await act(async () => {
        text.querySelector('[data-block="1"] .book-cell')?.prepend(tall)
      })
      await settled()
      expect(pageOfRange(text, character)).not.toBe(pageShown(text))
      const shown = charsBefore(text, firstRead(text) as Range)
      // Only a mark between the two tells a step from either of them.
      expect(markFrom(shown, back)).not.toEqual(markFrom(read?.at ?? 0, back))
      await pressed(key)
      expect(litAt(text)).toEqual(markFrom(shown, back))
    })
  }

  for (const view of [
    SET_IN,
    { name: '1366 by 768', width: 1366, height: 768, setIn: true },
    PHONE,
  ]) {
    for (const back of [false, true]) {
      it(`goes to the mark ${back ? 'before it on Shift+Enter' : 'after it on Enter'}, ${view.name}`, async () => {
        const layout = { mode: 'compare', paged: true }
        const host = await opened(view, 'en', 1, layout, {}, dense)
        const text = textOf(host)
        await pressed('PageDown')
        await pressed('PageDown')
        const read = anchorOf(text).read
        expect(read?.block, 'on a page the row carries over onto').toBe(1)
        const at = read?.at ?? 0
        const box = await searched(MARK.en)
        const pageStart = charsBefore(text, firstRead(text) as Range)
        // Set in, the page shown starts earlier than the character read, and
        // only a mark between the two tells a step from either of them.
        if (view.setIn) {
          expect(Math.ceil(pageStart / unit.length)).toBeLessThan(
            Math.ceil(at / unit.length),
          )
        } else expect(pageStart).toBe(at)
        await entered(box, back)
        expect(litAt(text)).toEqual(markFrom(at, back))
      })
    }
  }
})

describe('the saved place, as the book opens', () => {
  /**
   * The reader saved at block 12, set by `layout`, hearing a window resize
   * once it holds the book's answer and before React has drawn it: `act`
   * keeps the commit back until the resize has been sent.
   */
  async function resizedAsItOpens(layout: object) {
    await page.viewport(SET_IN.width, SET_IN.height)
    await settled()
    localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(layout))
    localStorage.setItem('taiyaku.prefs.book-text', '{}')
    const held = deferred<void>()
    const read = deferred<void>()
    const fetched = served('en', 12, book, async (body) => {
      await held.promise
      const answer = Response.json(body)
      answer.json = async () => {
        read.resolve()
        return body
      }
      return answer
    })
    vi.stubGlobal('fetch', fetched)
    const host = mount(
      <div style={{ position: 'fixed', inset: 0 }}>
        <BookReader work="w" onLeave={() => {}} />
      </div>,
    )
    // The saved place answered, so the book's answer is the last one waited on.
    await settled()
    await act(async () => {
      held.resolve()
      await read.promise
      // A task, for the reader's own `then` to have run on what it read.
      await new Promise((done) => setTimeout(done))
      expect(host.querySelector('.book-text'), 'not drawn yet').toBeNull()
      window.dispatchEvent(new Event('resize'))
    })
    await settled()
    return { text: textOf(host), fetched }
  }

  /** Every block a progress `PUT` has saved, once the reader's wait before saving has passed. */
  async function savedSlots(fetched: ReturnType<typeof served>) {
    await act(() => new Promise((done) => setTimeout(done, 1200)))
    return fetched.mock.calls
      .filter(
        ([url, init]) =>
          url === '/api/works/w/progress' && init?.method === 'PUT',
      )
      .map(([, init]) => JSON.parse(String(init?.body)).slot as number)
  }

  for (const [name, layout] of [
    ['single column, paged', { mode: 'scroll', paged: true }],
    ['single column, scrolled', { mode: 'scroll', paged: false }],
    ['compared and paged', { mode: 'compare', paged: true }],
  ] as const) {
    it(`a window resize as the book opens keeps the saved place, ${name}`, async () => {
      const { text, fetched } = await resizedAsItOpens(layout)
      // Paged, the page holding block 12 can open on the end of a block before.
      expect(
        matchShows(text, { block: 12, start: 0 }),
        'block 12 begins on the screen',
      ).toBe(true)
      if (!layout.paged) expect(topOf(text).block).toBe(12)
      const saved = await savedSlots(fetched)
      expect(saved.length, 'the reader saved its place').toBeGreaterThan(0)
      expect(saved).not.toContain(0)
    })
  }
})

describe('a place not yet committed, as an image loads', () => {
  const longRow3 = {
    ...lopsided,
    editions: Object.fromEntries(
      Object.entries(lopsided.editions).map(([key, edition]) => [
        key,
        {
          ...edition,
          blocks: edition.blocks.map((block, i) => {
            if (i !== 3) return block
            const text = `Block 3. ${WORDS.repeat(90)}`
            return { text, heading: false, sentences: [[0, text.length]] }
          }),
        },
      ]),
    ),
  }

  it('leaves the row’s start the wheel kept for a new text size, compared and paged', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 3, layout, {}, longRow3)
    const text = textOf(host)
    for (let turn = 0; turn < 4; turn++) await pressed('PageDown')
    const read = firstRead(text) as Range
    const row = text.querySelector('[data-block="3"]') as HTMLElement
    const words = row.querySelector('[data-pick^="left:"]') as HTMLElement
    const opening = document.createRange()
    opening.setStart(wordNodes(words)[0], 0)
    opening.setEnd(wordNodes(words)[0], 1)
    const image = new Image()
    image.style.cssText = 'display: block; height: 0'
    // In one `act`, as an image's `load` can come before what a wheel set is drawn.
    await act(async () => {
      text.dispatchEvent(
        new WheelEvent('wheel', { deltaY: -100, cancelable: true }),
      )
      row.append(image)
      image.dispatchEvent(new Event('load'))
    })
    await settled()
    const start = pageOfRange(text, opening)
    expect(
      pageOfRange(text, read),
      'the character read is on a later page',
    ).toBeGreaterThan(start)
    expect(pageShown(text)).toBe(start)
  })
})

/** The windows a docked panel sets the text in at, for the sweeps below. */
const SET_IN_WINDOWS: View[] = [
  SET_IN,
  { name: '1366 by 768', width: 1366, height: 768, setIn: true },
]

/** Every image in `text` loaded or failed, and what that set off drawn. */
async function imagesDone(text: HTMLElement) {
  await act(async () => {
    await Promise.all(
      [...text.querySelectorAll('img')].map((image) =>
        image.complete
          ? undefined
          : new Promise((done) => {
              image.addEventListener('load', done, { once: true })
              image.addEventListener('error', done, { once: true })
            }),
      ),
    )
  })
  await settled()
}

/** The first character of block `block`'s words in the column read, as a `Range`. */
function openingOf(text: HTMLElement, block: number): Range {
  const words = text.querySelector(`[data-pick="left:${block}"]`) as HTMLElement
  const range = document.createRange()
  range.setStart(wordNodes(words)[0], 0)
  range.setEnd(wordNodes(words)[0], 1)
  return range
}

/** Twice over, a docked panel opened with `held` on the page it shows, and closed back onto the exact page. */
async function heldThroughAPanel(host: HTMLElement, held: Range, what: string) {
  const text = textOf(host)
  const before = text.scrollLeft
  for (const time of ['opened', 'opened again']) {
    await pressed('t')
    await imagesDone(text)
    expect(panelOf(host), `${what}, ${time}`).toBe('contents')
    expect(pageShown(text), `${what}, ${time}`).toBe(pageOfRange(text, held))
    await pressed('Escape')
    await imagesDone(text)
    expect(panelOf(host), `${what}, ${time}, closed`).toBeUndefined()
    expect(text.scrollLeft, `${what}, ${time}, closed`).toBe(before)
  }
}

/** `figured` with blocks of whole sentences, whose rows set a figure so that a panel's narrower page starts on it, its words pushed onto the next. */
const figuredWhole = {
  ...figured,
  editions: Object.fromEntries(
    Object.entries(figured.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((_, i) => {
          const text = `Block ${i}. ${WORDS.repeat(1 + ((i * 7) % 6))}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
    ]),
  ),
}

/** Whether the first row on the page shown starts on it and opens with a figure. */
function opensWithAFigure(text: HTMLElement): boolean {
  const screen = text.getBoundingClientRect()
  for (const row of text.querySelectorAll('.book-row')) {
    const boxes = [...row.getClientRects()]
    const seen = boxes.some(
      (box) =>
        box.width > 0 &&
        box.left < screen.right - 1 &&
        box.right > screen.left + 1,
    )
    if (!seen) continue
    const first = row.querySelector('.book-cell')?.firstElementChild
    return (
      boxes[0].left >= screen.left - 1 &&
      first?.matches('.book-figure') === true
    )
  }
  return false
}

describe('a book just opened, compared and paged', () => {
  const layout = { mode: 'compare', paged: true }

  for (const view of SET_IN_WINDOWS) {
    for (const block of [1, 2, 3, 5]) {
      it(`shows the first words of block ${block}, saved in a book with a merged row, and keeps them through a panel, ${view.name}`, async () => {
        const host = await opened(view, 'en', block, layout, {}, merged)
        const text = textOf(host)
        const words = openingOf(text, block)
        expect(pageShown(text), 'as the book opens').toBe(
          pageOfRange(text, words),
        )
        await heldThroughAPanel(host, words, `block ${block}`)
      })
    }

    it(`shows the words of a saved row a figure fills the first page of, and keeps them through a panel, ${view.name}`, async () => {
      const host = await opened(view, 'en', 6, layout, {})
      const text = textOf(host)
      const words = openingOf(text, 6)
      expect(pageShown(text), 'as the book opens').toBe(
        pageOfRange(text, words),
      )
      await heldThroughAPanel(host, words, 'block 6')
    })
  }

  it('goes back to the row’s start as the window is resized, from the words of a row a figure fills the first page of', async () => {
    const host = await opened(SET_IN, 'en', 6, layout, {})
    const text = textOf(host)
    const row = text.querySelector('[data-block="6"]') as HTMLElement
    const words = openingOf(text, 6)
    expect(pageShown(text), 'as the book opens').toBe(pageOfRange(text, words))
    await act(() => page.viewport(1100, 700))
    await settled()
    expect(pageShown(text)).toBe(pageAt(text, row.getClientRects()[0].left + 1))
    expect(
      pageOfRange(text, words),
      'the words are on a later page than the row’s start',
    ).toBeGreaterThan(pageShown(text))
  })

  for (const block of [2, 3]) {
    it(`shows the first words of block ${block}, saved inside a merged row, in the phone layout`, async () => {
      const host = await opened(PHONE, 'en', block, layout, {}, merged)
      const text = textOf(host)
      const row = text.querySelector('[data-block="1"]') as HTMLElement
      expect(pageShown(text)).toBe(pageOfRange(text, openingOf(text, block)))
      expect(
        pageShown(text),
        'a later page than the row’s start',
      ).toBeGreaterThan(pageAt(text, row.getClientRects()[0].left + 1))
    })
  }

  it('shows the words of a saved row a figure fills the first page of, in the phone layout', async () => {
    const host = await opened(PHONE, 'en', 6, layout, {})
    const text = textOf(host)
    const row = text.querySelector('[data-block="6"]') as HTMLElement
    expect(pageShown(text)).toBe(pageOfRange(text, openingOf(text, 6)))
    expect(
      pageShown(text),
      'a later page than the row’s start',
    ).toBeGreaterThan(pageAt(text, row.getClientRects()[0].left + 1))
  })

  for (const turns of [0, 1, 3]) {
    it(`places nothing as a docked panel opens and closes over the text, the phone layout, ${turns} pages on from opening`, async () => {
      const host = await opened(PHONE, 'en', 2, layout, {}, merged)
      const text = textOf(host)
      for (let turn = 0; turn < turns; turn++) await pressed('PageDown')
      const before = text.scrollLeft
      const width = text.clientWidth
      const placed = vi.spyOn(text, 'scrollTo')
      await pressed('t')
      expect(panelOf(host)).toBe('contents')
      expect(text.clientWidth, 'the text is not set in').toBe(width)
      expect(text.scrollLeft, 'with the panel open').toBe(before)
      await pressed('Escape')
      expect(panelOf(host)).toBeUndefined()
      expect(text.scrollLeft, 'with the panel closed').toBe(before)
      expect(placed).not.toHaveBeenCalled()
    })
  }

  it('opens at the row’s start, compared and scrolled, saved inside a merged row', async () => {
    const scrolled = { mode: 'compare', paged: false }
    const host = await opened(SET_IN, 'en', 2, scrolled, {}, merged)
    const text = textOf(host)
    expect(topOf(text).block).toBe(1)
    expect(
      startOf(text, openingOf(text, 2).getBoundingClientRect()),
      'the block’s words are further down the row',
    ).toBeGreaterThan(text.clientHeight)
  })
})

for (const figures of [
  {
    name: 'a real 300 by 300 image under the shipped stylesheet',
    url: `data:image/svg+xml,${encodeURIComponent(
      '<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300"><rect width="300" height="300" fill="#888"/></svg>',
    )}`,
    css: '',
  },
  {
    name: 'figures forced to 300 by 300',
    url: 'data:,',
    css: MID_HEIGHT_FIGURES,
  },
]) {
  describe(`rows opening with a figure, compared and paged, with ${figures.name}`, () => {
    const layout = { mode: 'compare', paged: true }
    const style = document.createElement('style')
    style.textContent = figures.css
    beforeAll(() => {
      sized.remove()
      document.head.append(style)
      figureImage.url = figures.url
    })
    afterAll(() => {
      style.remove()
      document.head.append(sized)
      figureImage.url = 'data:,'
    })

    for (const view of SET_IN_WINDOWS) {
      for (const block of [2, 10, 12, 13]) {
        it(`shows the first words of block ${block} as the book opens and keeps them through a panel, ${view.name}`, async () => {
          const host = await opened(view, 'en', block, layout, {}, figured)
          const text = textOf(host)
          await imagesDone(text)
          const words = openingOf(text, block)
          expect(pageShown(text), 'as the book opens').toBe(
            pageOfRange(text, words),
          )
          await heldThroughAPanel(host, words, `block ${block}`)
        })
      }

      it(`keeps each page’s first character read through a panel, ${view.name}`, async () => {
        const host = await opened(view, 'en', 0, layout, {}, figuredWhole)
        const text = textOf(host)
        await imagesDone(text)
        let sampled = 0
        let opening = 0
        for (let turn = 0; turn < 45; turn++) {
          const from = text.scrollLeft
          await pressed('PageDown')
          await imagesDone(text)
          if (text.scrollLeft === from) break
          const first = firstRead(text)
          if (!first) continue
          if (opensWithAFigure(text)) opening++
          await heldThroughAPanel(host, first, `turn ${turn}`)
          sampled++
        }
        expect(sampled, 'pages holding the column read').toBeGreaterThan(0)
        expect(opening, 'pages a row opens with a figure on').toBeGreaterThan(0)
      }, 240_000)

      for (const block of [4, 10, 16]) {
        it(`keeps what the text size placed through a panel, at block ${block}’s row, ${view.name}`, async () => {
          const host = await opened(view, 'en', block, layout, {}, figured)
          const text = textOf(host)
          await imagesDone(text)
          const row = anchorOf(text).block
          await pressed('=')
          await imagesDone(text)
          // The row's first words where they are on the page it was placed on,
          // and the page's own first character where a figure pushed them off.
          const opening = openingOf(text, row)
          const onThePage = pageOfRange(text, opening) === pageShown(text)
          const held = onThePage ? opening : (firstRead(text) as Range)
          await heldThroughAPanel(host, held, `block ${block}`)
        })
      }
    }
  })
}

describe('a match gone to, compared and paged, through a docked panel', () => {
  it('keeps a match later on the page on the page shown', async () => {
    const layout = { mode: 'compare', paged: true }
    const host = await opened(SET_IN, 'en', 0, layout, {}, carried)
    const text = textOf(host)
    await entered(await searched(MARK.en))
    await pressed('Escape')
    const lit = () => text.querySelector('.book-found') as HTMLElement
    let later = 0
    let apart = 0
    for (let step = 1; step < 16; step++) {
      await pressed(']')
      const first = firstRead(text) as Range
      if (!lit().contains(first.startContainer)) later++
      const before = text.scrollLeft
      for (const time of ['opened', 'opened again']) {
        await pressed('t')
        expect(panelOf(host), `step ${step}, ${time}`).toBe('contents')
        const shown = pageAt(text, lit().getBoundingClientRect().left + 1)
        expect(pageShown(text), `step ${step}, ${time}`).toBe(shown)
        if (pageOfRange(text, first) !== shown) apart++
        await pressed('Escape')
        expect(text.scrollLeft, `step ${step}, ${time}, closed`).toBe(before)
      }
    }
    expect(later, 'matches later on their page').toBeGreaterThan(0)
    // Where the page's first character stays beside the match every time, this
    // could not tell a panel keeping the match from one keeping that character.
    expect(apart, 'panels the two part on').toBeGreaterThan(0)
  }, 120_000)
})

/** Blocks of differing lengths, every seventh running on over several pages. */
const varied = {
  title: 'Varied',
  hub: 'en',
  editions: Object.fromEntries(
    (
      [
        ['en', WORDS, 1],
        ['ja', SAID, 0.6],
      ] as const
    ).map(([key, words, share]) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: 40 }, (_, i) => {
          const length = i % 7 === 3 ? 12000 : 200 + ((i * 317) % 1300)
          const text = `${i}. ${words.repeat(400)}`.slice(0, length * share)
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
        figures: [],
      },
    ]),
  ),
  alignments: [
    {
      to: 'ja',
      links: Array.from({ length: 40 }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: 40 }, (_, i) => [i, i + 1, i, i + 1]),
      pins: [],
    },
  ],
}

/** `varied` with each block's first character a link to that block. */
const linked = {
  ...varied,
  editions: Object.fromEntries(
    Object.entries(varied.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((block, i) => ({
          ...block,
          marks: [{ kind: 'link', start: 0, end: 1, block: i, note: false }],
        })),
      },
    ]),
  ),
}

/** `merged` with blocks long enough that a page inside the row can have none start on it. */
const longMerged = {
  ...merged,
  editions: Object.fromEntries(
    Object.entries(merged.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((_, i) => {
          const text = `Block ${i}. ${WORDS.repeat(150)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
    ]),
  ),
}

/** `longMerged` with a link at the start of block 1, the row's first, to block 2 further down the row. */
const linkedMerged = {
  ...longMerged,
  editions: Object.fromEntries(
    Object.entries(longMerged.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((block, i) =>
          i === 1
            ? {
                ...block,
                marks: [
                  { kind: 'link', start: 0, end: 1, block: 2, note: false },
                ],
              }
            : block,
        ),
      },
    ]),
  ),
}

/** `merged` with a first block filling its pages, and short blocks after it, so a page opens several blocks of the merged row. */
const sharedPage = {
  ...merged,
  editions: Object.fromEntries(
    Object.entries(merged.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((_, i) => {
          const text = `Block ${i}. ${WORDS.repeat(i === 0 ? 60 : 1)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
      },
    ]),
  ),
}

/** The page shown, from 0, along the way `text`'s pages run. */
const pageAlong = (text: HTMLElement) =>
  text.dataset.vertical === undefined
    ? pageShown(text)
    : Math.round(text.scrollTop / text.clientHeight)

/** The page, from 0, that `box` starts on, along the way `text`'s pages run. */
function pageStarting(text: HTMLElement, box: DOMRect): number {
  if (text.dataset.vertical === undefined) return pageAt(text, box.left + 1)
  const from = box.top + 1 - text.getBoundingClientRect().top
  return Math.floor((from + text.scrollTop) / text.clientHeight)
}

/** Each block and the box it opens on: its first line, or compared its first character in the column read. */
function openings(text: HTMLElement): { block: number; box: DOMRect }[] {
  if (text.dataset.compared === undefined) {
    return [...text.querySelectorAll<HTMLElement>(':scope > [data-block]')].map(
      (child) => ({
        block: Number(child.dataset.block),
        box: child.getClientRects()[0],
      }),
    )
  }
  return [...text.querySelectorAll<HTMLElement>('[data-pick^="left:"]')].map(
    (words) => {
      const block = Number(words.dataset.pick?.slice('left:'.length))
      return { block, box: openingOf(text, block).getBoundingClientRect() }
    },
  )
}

/** The blocks that open on the page shown, in order. */
const openingHere = (text: HTMLElement) =>
  openings(text)
    .filter(({ box }) => pageStarting(text, box) === pageAlong(text))
    .map(({ block }) => block)

/** The first character on the page shown: compared, of the column read. */
const firstShown = (text: HTMLElement) => anchorOf(text).read ?? topOf(text)

/** Where a reader is: the page shown and the first character on it. */
const shownPlace = (text: HTMLElement) => ({
  page: pageAlong(text),
  first: firstShown(text),
})

/** Whether the page shown holds a figure and no words of either column. */
function holdsOnlyAFigure(text: HTMLElement): boolean {
  const screen = text.getBoundingClientRect()
  const shown = (box: DOMRect) =>
    box.width > 0 &&
    box.height > 0 &&
    box.right > screen.left + 1 &&
    box.left < screen.right - 1 &&
    box.bottom > screen.top + 1 &&
    box.top < screen.bottom - 1
  const showing = (selector: string) =>
    [...text.querySelectorAll(selector)].some((found) =>
      [...found.getClientRects()].some(shown),
    )
  return showing('.book-figure') && !showing('[data-text], [data-pick]')
}

/** Pages turned on until `wanted` holds of the page shown. */
async function turnedUntil(
  text: HTMLElement,
  wanted: (text: HTMLElement) => boolean,
) {
  for (let i = 0; i < 150 && !wanted(text); i++) await pressed('PageDown')
}

/** The reader closed as one leaving it is: the block it last saved, or `slot` where it saved none. */
async function leftAt(slot: number): Promise<number> {
  rerender(null)
  await settled()
  const sent = progressSent().at(-1)?.[1]
  return sent ? (JSON.parse(String(sent.body)).slot as number) : slot
}

/** The progress `PUT`s this reader has sent. */
const progressSent = () =>
  vi
    .mocked(fetch)
    .mock.calls.filter(
      ([url, init]) =>
        url === '/api/works/w/progress' && init?.method === 'PUT',
    )

describe('reopening a paged book returns to the page it was left on', () => {
  const views: View[] = [
    SET_IN,
    { name: '390 by 844', width: 390, height: 844, setIn: false },
  ]
  const OPENS = 5
  const modes = [
    {
      name: 'single column',
      edition: 'en',
      layout: { mode: 'scroll', paged: true },
      typeset: {},
    },
    {
      name: 'compared',
      edition: 'en',
      layout: { mode: 'compare', paged: true },
      typeset: {},
    },
    {
      name: 'interlinear',
      edition: 'en',
      layout: { mode: 'interlinear', paged: true },
      typeset: {},
    },
    {
      name: 'vertical',
      edition: 'ja',
      layout: { mode: 'scroll', paged: true },
      typeset: { vertical: true },
    },
  ]
  const carriedOver = (text: HTMLElement) => firstShown(text).at > 0

  for (const view of views) {
    for (const { name, edition, layout, typeset } of modes) {
      const open = (slot: number) =>
        opened(view, edition, slot, layout, typeset, varied)

      it(`holds the page and its first character, opened and closed unread, ${name}, ${view.name}`, async () => {
        const found = textOf(await open(3))
        await turnedUntil(
          found,
          (text) => carriedOver(text) && openingHere(text).length > 0,
        )
        let slot = openingHere(found).at(-1) as number
        await leftAt(slot)
        const text = textOf(await open(slot))
        expect(
          carriedOver(text),
          'the page starts in a block begun earlier',
        ).toBe(true)
        expect(openingHere(text), 'a block opens on the page').toContain(slot)
        const first = shownPlace(text)
        for (let time = 1; time <= OPENS; time++) {
          slot = await leftAt(slot)
          const again = textOf(await open(slot))
          expect(shownPlace(again), `reopened ${time} times`).toEqual(first)
        }
      })

      it(`holds the page turned to and left, ${name}, ${view.name}`, async () => {
        const text = textOf(await open(3))
        await pressed('PageDown')
        await turnedUntil(
          text,
          (shown) => carriedOver(shown) && openingHere(shown).length > 0,
        )
        expect(
          carriedOver(text),
          'the page starts in a block begun earlier',
        ).toBe(true)
        expect(openingHere(text), 'a block opens on the page').not.toEqual([])
        const left = shownPlace(text)
        let slot = await leftAt(3)
        for (let time = 1; time <= OPENS; time++) {
          const again = textOf(await open(slot))
          expect(shownPlace(again), `reopened ${time} times`).toEqual(left)
          slot = await leftAt(slot)
        }
      })

      it(`goes to the first page of the block carried onto a page no block opens on, and holds there, ${name}, ${view.name}`, async () => {
        const text = textOf(await open(3))
        await turnedUntil(
          text,
          (shown) => carriedOver(shown) && openingHere(shown).length === 0,
        )
        expect(openingHere(text), 'no block opens on the page').toEqual([])
        const carried = firstShown(text).block
        let slot = await leftAt(3)
        const back = textOf(await open(slot))
        const start = openings(back).find(({ block }) => block === carried)
        expect(pageAlong(back), 'the block’s first page').toBe(
          pageStarting(back, start?.box as DOMRect),
        )
        const first = shownPlace(back)
        for (let time = 2; time <= OPENS; time++) {
          slot = await leftAt(slot)
          const again = textOf(await open(slot))
          expect(shownPlace(again), `reopened ${time} times`).toEqual(first)
        }
      })
    }

    const compared = { mode: 'compare', paged: true }

    for (const block of [2, 3]) {
      it(`keeps block ${block}’s first words and its saved block, saved inside a merged row, ${view.name}`, async () => {
        let slot = block
        for (let time = 1; time <= OPENS; time++) {
          const text = textOf(
            await opened(view, 'en', slot, compared, {}, merged),
          )
          expect(pageShown(text), `opened ${time} times`).toBe(
            pageOfRange(text, openingOf(text, block)),
          )
          expect(openingHere(text)[0], 'the first block to open here').toBe(
            block,
          )
          slot = await leftAt(slot)
          expect(slot, `saved after ${time} opens`).toBe(block)
        }
      })
    }

    it(`holds the page where a block saved inside a merged row shares it with an earlier one, ${view.name}`, async () => {
      const open = (slot: number) =>
        opened(view, 'en', slot, compared, {}, sharedPage)
      const text = textOf(await open(3))
      expect(openingHere(text).slice(0, 3), 'blocks 1 to 3 open here').toEqual([
        1, 2, 3,
      ])
      const held = shownPlace(text)
      let slot = await leftAt(3)
      expect(slot, 'the first block to open on the page').toBe(1)
      for (let time = 2; time <= OPENS; time++) {
        const again = textOf(await open(slot))
        expect(shownPlace(again), `reopened ${time} times`).toEqual(held)
        expect(pageShown(again), 'block 3’s first words are on it').toBe(
          pageOfRange(again, openingOf(again, 3)),
        )
        slot = await leftAt(slot)
        expect(slot, `saved after ${time} opens`).toBe(1)
      }
    })

    it(`goes to the first page of a block carried on inside a merged row, not the row’s start, ${view.name}`, async () => {
      const open = (slot: number) =>
        opened(view, 'en', slot, compared, {}, longMerged)
      const text = textOf(await open(2))
      await pressed('PageDown')
      await turnedUntil(text, (shown) => openingHere(shown).length === 0)
      expect(openingHere(text), 'no block opens on the page').toEqual([])
      expect(firstShown(text).block, 'block 2 is carried onto it').toBe(2)
      let slot = await leftAt(2)
      expect(slot, 'the block carried on, saved').toBe(2)
      const back = textOf(await open(slot))
      const first = shownPlace(back)
      expect(first.page).toBe(pageOfRange(back, openingOf(back, 2)))
      for (let time = 2; time <= OPENS; time++) {
        slot = await leftAt(slot)
        const again = textOf(await open(slot))
        expect(shownPlace(again), `reopened ${time} times`).toEqual(first)
      }
    })
  }

  for (const view of [...views, PHONE]) {
    for (const { name, edition, layout, typeset } of modes) {
      const needle = edition === 'ja' ? '名前' : 'clocks'
      const open = (slot: number) =>
        opened(view, edition, slot, layout, typeset, varied)

      /** Whether the next three matches on from the page's first character are all on the page. */
      const matchesHere = (text: HTMLElement) => {
        const { block, at } = firstShown(text)
        const words = varied.editions[edition].blocks[block].text
        let start = at - 1
        for (let match = 0; match < 3; match++) {
          start = words.indexOf(needle, start + 1)
          if (start < 0 || !matchShows(text, { block, start })) return false
        }
        return true
      }

      for (const stepped of [false, true]) {
        it(`holds the page ${stepped ? ']' : 'Enter'} goes to a match on, a block carried onto it, ${name}, ${view.name}`, async () => {
          const text = textOf(await open(3))
          await turnedUntil(
            text,
            (shown) =>
              carriedOver(shown) &&
              openingHere(shown).length > 0 &&
              matchesHere(shown),
          )
          expect(matchesHere(text), 'three matches on the page').toBe(true)
          await entered(await searched(needle))
          await pressed('Escape')
          if (stepped) await pressed(']')
          expect(
            carriedOver(text),
            'the page starts in a block begun earlier',
          ).toBe(true)
          expect(openingHere(text), 'a block opens on the page').not.toEqual([])
          const left = shownPlace(text)
          let slot = await leftAt(3)
          for (let time = 1; time <= OPENS; time++) {
            const again = textOf(await open(slot))
            expect(shownPlace(again), `reopened ${time} times`).toEqual(left)
            slot = await leftAt(slot)
          }
        })
      }

      it(`holds the page Back returns to from a link on it, a block carried onto it, ${name}, ${view.name}`, async () => {
        const openLinked = (slot: number) =>
          opened(view, edition, slot, layout, typeset, linked)
        const host = await openLinked(3)
        const text = textOf(host)
        await turnedUntil(
          text,
          (shown) => carriedOver(shown) && openingHere(shown).length > 0,
        )
        expect(
          carriedOver(text),
          'the page starts in a block begun earlier',
        ).toBe(true)
        const before = shownPlace(text)
        const block = openingHere(text)[0]
        const link = text.querySelector<HTMLElement>(
          `[data-block="${block}"] a, [data-pick="left:${block}"] a`,
        )
        expect(link, 'a link on the page').not.toBeNull()
        await act(async () => {
          link?.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
          )
        })
        await settled()
        expect(shownPlace(text), 'the link’s block opens on this page').toEqual(
          before,
        )
        const back = [
          ...host.querySelectorAll<HTMLElement>('.reader-bottom-bar button'),
        ].find((button) => button.textContent === 'Back')
        expect(back, 'Back is offered').toBeDefined()
        await act(async () => back?.click())
        await settled()
        const left = shownPlace(text)
        // Compared, Back goes to the start of the row it was left in.
        if (layout.mode !== 'compare') {
          expect(left, 'Back stays on the page').toEqual(before)
        }
        let slot = await leftAt(3)
        for (let time = 1; time <= OPENS; time++) {
          const again = textOf(await openLinked(slot))
          expect(shownPlace(again), `reopened ${time} times`).toEqual(left)
          slot = await leftAt(slot)
        }
      })
    }
  }

  describe('a block inside a merged row, gone to from the row’s first page', () => {
    const compared = { mode: 'compare', paged: true }
    const open = (view: View, slot: number) =>
      opened(view, 'en', slot, compared, {}, linkedMerged)

    /** Left where it is, and reopened: the page shown each time. */
    async function heldThroughReopening(view: View, shown: number) {
      let slot = await leftAt(1)
      for (let time = 1; time <= OPENS; time++) {
        const again = textOf(await open(view, slot))
        expect(pageShown(again), `reopened ${time} times`).toBe(shown)
        slot = await leftAt(slot)
      }
    }

    for (const view of views) {
      it(`holds the page a link leaves it on, ${view.name}`, async () => {
        const text = textOf(await open(view, 1))
        const shown = pageShown(text)
        const link = text.querySelector<HTMLElement>('[data-pick="left:1"] a')
        expect(link, 'a link on the page').not.toBeNull()
        await act(async () => {
          link?.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
          )
        })
        await settled()
        expect(pageShown(text), 'the row’s first page, not scrolled').toBe(
          shown,
        )
        await heldThroughReopening(view, shown)
      })
    }

    for (const view of [PHONE, views[1]]) {
      it(`holds the page a note leaves it on, ${view.name}`, async () => {
        notesServed = [
          {
            id: 'n',
            edition: 'en',
            slot: 2,
            body: 'a note',
            created: '2026-11-18T00:00:00Z',
            updated: '2026-11-18T00:00:00Z',
          },
        ]
        onTestFinished(() => {
          notesServed = []
        })
        const host = await open(view, 1)
        const text = textOf(host)
        const shown = pageShown(text)
        await pressed('a')
        const row = host.querySelector<HTMLElement>(
          '[aria-label="Notes"] li button',
        )
        expect(row, 'the note is listed').not.toBeNull()
        await act(async () => row?.click())
        await settled()
        expect(pageShown(text), 'the row’s first page, not scrolled').toBe(
          shown,
        )
        await heldThroughReopening(view, shown)
      })
    }
  })

  describe('a page of a merged row holding none of the column read', () => {
    const compared = { mode: 'compare', paged: true }
    const WIDE: View = {
      name: '1366 by 768',
      width: 1366,
      height: 768,
      setIn: true,
    }

    /** `merged` with its row's blocks, 1 to 3, `times` sentences long in each edition. */
    const withRow = (
      times: (edition: string) => number,
      figures: object[],
    ) => ({
      ...merged,
      editions: Object.fromEntries(
        Object.entries(merged.editions).map(([key, edition]) => [
          key,
          {
            ...edition,
            figures,
            blocks: edition.blocks.map((_, i) => {
              const inRow = i >= 1 && i <= 3
              const text = `Block ${i}. ${WORDS.repeat(inRow ? times(key) : 8)}`
              return { text, heading: false, sentences: [[0, text.length]] }
            }),
          },
        ]),
      ),
    })
    /** The column read ends pages before the other column does. */
    const otherRunsOn = withRow((key) => (key === 'en' ? 40 : 300), [])
    /** A figure before block 3, in both columns, has a page of the row to itself. */
    const figureInRow = withRow(
      () => 50,
      [{ before: 3, image: 0, alt: 'a map' }],
    )
    const holdsNoneRead = (text: HTMLElement) =>
      anchorOf(text).read === undefined

    /** Left on the page shown: `block` saved, and each reopening on that block's first page. */
    async function reopensOn(view: View, sent: object, block: number) {
      let slot = await leftAt(1)
      expect(slot, 'the block saved').toBe(block)
      for (let time = 1; time <= OPENS; time++) {
        const again = textOf(await opened(view, 'en', slot, compared, {}, sent))
        const first = pageOfRange(again, openingOf(again, block))
        expect(pageShown(again), `reopened ${time} times`).toBe(first)
        expect(first, 'a later page than the row’s start').toBeGreaterThan(
          pageOfRange(again, openingOf(again, 1)),
        )
        slot = await leftAt(slot)
      }
    }

    for (const view of [SET_IN, PHONE]) {
      it(`reopens a page holding only a figure on the first page of the block before the figure, ${view.name}`, async () => {
        const text = textOf(
          await opened(view, 'en', 1, compared, {}, figureInRow),
        )
        await turnedUntil(text, holdsOnlyAFigure)
        expect(holdsOnlyAFigure(text), 'only the figure is on the page').toBe(
          true,
        )
        await reopensOn(view, figureInRow, 2)
      }, 120_000)
    }

    for (const view of [SET_IN, WIDE]) {
      it(`reopens the first page holding only the other column on the first page of the last block read, ${view.name}`, async () => {
        const text = textOf(
          await opened(view, 'en', 1, compared, {}, otherRunsOn),
        )
        await turnedUntil(text, holdsNoneRead)
        expect(holdsNoneRead(text), 'none of the column read').toBe(true)
        await reopensOn(view, otherRunsOn, 3)
      }, 120_000)

      it(`reopens the last page holding only the other column on the first page of the last block read, ${view.name}`, async () => {
        const text = textOf(
          await opened(view, 'en', 1, compared, {}, otherRunsOn),
        )
        await turnedUntil(text, holdsNoneRead)
        await turnedUntil(text, (shown) => !holdsNoneRead(shown))
        await pressed('PageUp')
        expect(holdsNoneRead(text), 'none of the column read').toBe(true)
        await reopensOn(view, otherRunsOn, 3)
      }, 120_000)
    }
  })

  describe('a page holding only a figure, between blocks', () => {
    const figuredPages = {
      ...varied,
      editions: Object.fromEntries(
        Object.entries(varied.editions).map(([key, edition]) => [
          key,
          {
            ...edition,
            figures: [5, 9, 16, 23, 40].map((before) => ({
              before,
              image: 0,
              alt: 'a map',
            })),
          },
        ]),
      ),
    }

    for (const { name, edition, layout, typeset } of modes) {
      it(`reopens on the page of a block next to the figure, and holds there, ${name}`, async () => {
        const open = (slot: number) =>
          opened(SET_IN, edition, slot, layout, typeset, figuredPages)
        const text = textOf(await open(0))
        await turnedUntil(text, holdsOnlyAFigure)
        expect(holdsOnlyAFigure(text), 'only a figure is on the page').toBe(
          true,
        )
        const left = pageAlong(text)
        let slot = await leftAt(0)
        const back = textOf(await open(slot))
        const first = shownPlace(back)
        expect(Math.abs(first.page - left), 'the page beside it').toBe(1)
        expect(openingHere(back), 'the block saved opens there').toContain(slot)
        for (let time = 2; time <= OPENS; time++) {
          slot = await leftAt(slot)
          const again = textOf(await open(slot))
          expect(shownPlace(again), `reopened ${time} times`).toEqual(first)
        }
      }, 120_000)
    }
  })

  it('holds the page a match in the other edition goes to with no page turned', async () => {
    const single = { mode: 'scroll', paged: true }
    const needle = '鯨鯱'
    const found = textOf(await opened(PHONE, 'ja', 3, single, {}, varied))
    const tail = (text: HTMLElement) => {
      const { block, at } = firstShown(text)
      return varied.editions.ja.blocks[block].text.length - at
    }
    await turnedUntil(
      found,
      (text) =>
        carriedOver(text) && openingHere(text).length > 0 && tail(text) > 8,
    )
    const page = pageAlong(found)
    const { block, at } = firstShown(found)
    await leftAt(3)
    // Two characters for two, so the edition lays out as it did.
    const needled = structuredClone(varied)
    const words = needled.editions.ja.blocks[block]
    words.text = words.text.slice(0, at + 2) + needle + words.text.slice(at + 4)
    const host = await opened(PHONE, 'en', 0, single, {}, needled)
    await turnedUntil(textOf(host), (text) => pageAlong(text) >= page)
    expect(pageAlong(textOf(host)), 'the match’s page, in English').toBe(page)
    const box = await searched(needle)
    const every = document.querySelector<HTMLInputElement>(
      '[role="dialog"][aria-label="Search"] input[type="checkbox"]',
    )
    await act(async () => every?.click())
    await settled()
    await entered(box)
    await pressed('Escape')
    const text = textOf(host)
    expect(text.lang, 'the edition the match is in').toBe('ja')
    expect(pageAlong(text), 'no page turned').toBe(page)
    expect(openingHere(text), 'a block opens on the page').not.toEqual([])
    const left = shownPlace(text)
    let slot = await leftAt(0)
    for (let time = 1; time <= OPENS; time++) {
      const again = textOf(await opened(PHONE, 'ja', slot, single, {}, needled))
      expect(shownPlace(again), `reopened ${time} times`).toEqual(left)
      slot = await leftAt(slot)
    }
  })

  it('sends no progress as a resize places the text again after a match gone to', async () => {
    const single = { mode: 'scroll', paged: true }
    await opened(PHONE, 'en', 3, single, {}, varied)
    await entered(await searched('clocks'))
    await pressed('Escape')
    const waited = () =>
      act(() => new Promise((done) => setTimeout(done, 1200)))
    await waited()
    const sent = progressSent().length
    expect(sent, 'the match gone to is saved').toBeGreaterThan(0)
    await act(async () => {
      window.dispatchEvent(new Event('resize'))
    })
    await settled()
    await waited()
    expect(progressSent().length).toBe(sent)
  })

  it('saves the block of a match gone to at the book’s end, scrolled, where the placing cannot scroll', async () => {
    const scrolled = { mode: 'scroll', paged: false }
    const text = textOf(await opened(PHONE, 'en', 38, scrolled, {}, varied))
    await act(async () => text.scrollTo({ top: text.scrollHeight }))
    await settled()
    await entered(await searched('39. It'))
    await pressed('Escape')
    expect(topOf(text).block, 'the top block is an earlier one').toBeLessThan(
      39,
    )
    expect(await leftAt(38)).toBe(39)
  })

  for (const [name, layout] of [
    ['single column', { mode: 'scroll', paged: false }],
    ['compared', { mode: 'compare', paged: false }],
  ] as const) {
    it(`leaves the saved block as it was, scrolled, ${name}`, async () => {
      let slot = 12
      for (let time = 1; time <= OPENS; time++) {
        await opened(SET_IN, 'en', slot, layout, {}, varied)
        slot = await leftAt(slot)
        expect(slot, `saved after ${time} opens`).toBe(12)
      }
    })
  }
})

/** `marked` with a match opening each of its first forty blocks, so a screen holds several. */
const everyMarked = markedOf(60, 1, 1)

/** Blocks of uneven length, some longer than a page, so pages start inside blocks. */
const uneven = {
  title: 'Uneven',
  hub: 'en',
  editions: Object.fromEntries(
    ['en', 'ja'].map((key) => [
      key,
      {
        label: key,
        language: key,
        blocks: Array.from({ length: 36 }, (_, i) => {
          const text = `Block ${i}. ${WORDS.repeat(3 + ((i * 7) % 13) * 3)}`
          return { text, heading: false, sentences: [[0, text.length]] }
        }),
        figures: [],
      },
    ]),
  ),
  alignments: [
    {
      to: 'ja',
      links: Array.from({ length: 36 }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: Array.from({ length: 36 }, (_, i) => [i, i + 1, i, i + 1]),
      pins: [],
    },
  ],
}

const SCROLLED = { mode: 'scroll', paged: false }
const PAGED = { mode: 'scroll', paged: true }

/** The look-up mode on for this test, so a tap on a word looks it up. */
function lookUpOn() {
  localStorage.setItem('taiyaku.prefs.lookup', JSON.stringify({ on: true }))
  onTestFinished(() => localStorage.removeItem('taiyaku.prefs.lookup'))
}

/**
 * Every lower-case word of four letters or more but the mark, drawn whole on
 * the screen and topmost there, in reading order; paged, only those in the
 * middle third, since a tap in either outer third turns the page.
 */
function wordsInSight(text: HTMLElement): Range[] {
  const screen = text.getBoundingClientRect()
  const paged = text.dataset.paged !== undefined
  const third = screen.width / 3
  const found: Range[] = []
  for (const words of text.querySelectorAll<HTMLElement>(
    '[data-block] > [data-text]',
  )) {
    for (const node of wordNodes(words)) {
      for (const word of node.data.matchAll(/\b[a-z]{4,}\b/g)) {
        if (word[0] === MARK.en) continue
        const range = document.createRange()
        range.setStart(node, word.index)
        range.setEnd(node, word.index + word[0].length)
        const boxes = [...range.getClientRects()]
        if (boxes.length !== 1) continue
        const [box] = boxes
        const x = box.left + box.width / 2
        const y = box.top + box.height / 2
        if (
          box.left < screen.left + 2 ||
          box.right > screen.right - 2 ||
          box.top < screen.top + 2 ||
          box.bottom > screen.bottom - 2
        ) {
          continue
        }
        if (
          paged &&
          (x < screen.left + third + 4 || x > screen.left + 2 * third - 4)
        ) {
          continue
        }
        const hit = document.elementFromPoint(x, y)
        if (hit && words.contains(hit)) found.push(range)
      }
    }
  }
  return found
}

/** A real finger's tap on the middle of `word`, and the look-up it asks for answered. */
async function tappedOn(word: Range) {
  const [box] = word.getClientRects()
  await tapAt(box.left + box.width / 2, box.top + box.height / 2)
  await act(() => new Promise((done) => setTimeout(done, 400)))
  await settled()
}

/** The reader at 1000 by 700 on `sent`, at the match in block `block` stepped to with `]`. */
async function heldAt(layout: object, block: number): Promise<HTMLElement> {
  lookUpOn()
  const host = await opened(SET_IN, 'en', 20, layout, {}, everyMarked)
  const text = textOf(host)
  await entered(await searched(MARK.en))
  await pressed('Escape')
  for (let i = 0; i < 5 && litBlocks(text)[0] !== block; i++) {
    await pressed(']')
  }
  expect(litBlocks(text)).toEqual([block])
  return host
}

/** The block of the last progress `PUT` sent. */
const lastSaved = () =>
  JSON.parse(String(progressSent().at(-1)?.[1]?.body)).slot as number

/** Where the scroll stands, for a move back to exactly there. */
const spotOf = (text: HTMLElement) => ({
  left: text.scrollLeft,
  top: text.scrollTop,
})

/** The block of the first match at or after the anchor: each block opens with one. */
function nextMatchFrom(text: HTMLElement): number {
  const { block, at } = anchorOf(text)
  return at === 0 ? block : block + 1
}

// The look-up's sight check brings the word tapped into sight beside the
// panel. That scroll is the app moving the text, not the reader: it ends no
// hold on a match gone to, leaves the anchor on it and saves no place.
describe('the look-up’s own scroll, after a match gone to', () => {
  const CASES = [
    {
      name: 'scrolled, the last word on the screen',
      layout: SCROLLED,
      last: true,
    },
    { name: 'paged, the last word on the page', layout: PAGED, last: true },
    { name: 'paged, the first word on the page', layout: PAGED, last: false },
  ]
  for (const { name, layout, last } of CASES) {
    for (const closed of [false, true]) {
      for (const { key, block } of [
        { key: ']', block: 23 },
        { key: '[', block: 21 },
      ]) {
        it(`steps from the match to block ${block} on ${key}, ${name}, the panel ${closed ? 'closed' : 'open'}`, async () => {
          const host = await heldAt(layout, 22)
          const text = textOf(host)
          const words = wordsInSight(text)
          await tappedOn(last ? (words.at(-1) as Range) : words[0])
          expect(panelOf(host)).toBe('lookup')
          expect(
            onScreen(text, 22),
            'the look-up moved the match off the screen',
          ).toBe(false)
          if (closed) {
            await pressed('Escape')
            expect(panelOf(host)).toBeUndefined()
          }
          await pressed(key)
          expect(litBlocks(text)).toEqual([block])
        })
      }
    }

    it(`saves the block the match saved, ${name}, left with the panel open`, async () => {
      const kept = textOf(await heldAt(layout, 22))
      expect(onScreen(kept, 22)).toBe(true)
      const before = await leftAt(20)
      const text = textOf(await heldAt(layout, 22))
      const words = wordsInSight(text)
      await tappedOn(last ? (words.at(-1) as Range) : words[0])
      expect(onScreen(text, 22), 'the look-up moved the match off').toBe(false)
      expect(await leftAt(20)).toBe(before)
    })
  }

  it('ends the hold, moves the anchor and saves for a page turned with the panel open, and again for a turn back onto the page the look-up showed', async () => {
    const host = await heldAt(PAGED, 22)
    const text = textOf(host)
    await tappedOn(wordsInSight(text)[0])
    expect(onScreen(text, 22), 'the look-up moved the match off').toBe(false)
    const shown = spotOf(text)
    const sent = progressSent().length
    await pressed('PageUp')
    expect(panelOf(host)).toBe('lookup')
    const away = { slot: resumeBlock(text), first: anchorOf(text) }
    await pressed('PageDown')
    expect(spotOf(text), 'back on the page the look-up showed').toEqual(shown)
    await act(() => new Promise((done) => setTimeout(done, 1100)))
    expect(progressSent().length).toBeGreaterThan(sent)
    expect(lastSaved()).toBe(resumeBlock(text))
    expect(lastSaved(), 'not the page turned away to').not.toBe(away.slot)
    const first = anchorOf(text)
    expect(first, 'not the page turned away to').not.toEqual(away.first)
    const next = nextMatchFrom(text)
    expect(next, 'a step from the match would give the same').not.toBe(23)
    await pressed('Escape')
    expect(onScreen(text, first.block, first.at), 'the anchor moved').toBe(true)
    await pressed(']')
    expect(litBlocks(text)).toEqual([next])
  })

  it('ends the hold, moves the anchor and saves for a scroll back to exactly where the look-up left the text, after the panel closed', async () => {
    const host = await heldAt(SCROLLED, 22)
    const text = textOf(host)
    await tappedOn(wordsInSight(text).at(-1) as Range)
    expect(onScreen(text, 22), 'the look-up moved the match off').toBe(false)
    const shown = spotOf(text)
    await pressed('Escape')
    expect(onScreen(text, 22), 'closing went back to the match').toBe(true)
    const sent = progressSent().length
    await act(async () => text.scrollTo(shown))
    await settled()
    expect(spotOf(text), 'where the look-up left the text').toEqual(shown)
    await act(() => new Promise((done) => setTimeout(done, 1100)))
    expect(progressSent().length).toBeGreaterThan(sent)
    expect(lastSaved()).toBe(resumeBlock(text))
    const next = nextMatchFrom(text)
    expect(next, 'a step from the match would give the same').not.toBe(23)
    await pressed(']')
    expect(litBlocks(text)).toEqual([next])
  })
})

describe('the look-up’s own scroll, with no match gone to', () => {
  it('leaves a hand move that ends where it began the reader’s, after a look-up that moved nothing', async () => {
    lookUpOn()
    const text = textOf(
      await opened(PHONE, 'en', 20, SCROLLED, {}, everyMarked),
    )
    await act(async () => addProgress(text, 40))
    await settled()
    await act(() => new Promise((done) => setTimeout(done, 1100)))
    const words = wordsInSight(text)
    const before = spotOf(text)
    await tappedOn(words[Math.floor(words.length / 2)])
    expect(spotOf(text), 'the look-up moved nothing').toEqual(before)
    const sent = progressSent().length
    await act(async () => {
      addProgress(text, 40)
      addProgress(text, -40)
    })
    await settled()
    await act(() => new Promise((done) => setTimeout(done, 1100)))
    expect(progressSent().length).toBe(sent + 1)
  })

  for (const turned of [1, 2, 3, 5, 7]) {
    it(`reopens on the page read, left with the panel open, paged, ${turned} pages in`, async () => {
      await leftOpenReopens(SET_IN, 'en', {}, turned)
    })
  }

  it('saves the page reading aloud turns to, paged, with the panel open', async () => {
    speaksNothing()
    lookUpOn()
    const host = await opened(SET_IN, 'en', 2, PAGED, {}, uneven)
    const text = textOf(host)
    const words = wordsInSight(text)
    await tappedOn(words[Math.floor(words.length / 2)])
    expect(panelOf(host)).toBe('lookup')
    const before = { spot: spotOf(text), slot: resumeBlock(text) }
    await pressed('r')
    for (let i = 0; i < 12 && spotOf(text).left === before.spot.left; i++) {
      await act(async () =>
        spoken.at(-1)?.onend?.(new Event('end') as SpeechSynthesisEvent),
      )
      await act(async () =>
        spoken.at(-1)?.onstart?.(new Event('start') as SpeechSynthesisEvent),
      )
      await settled()
    }
    expect(spotOf(text), 'reading aloud turned the page').not.toEqual(
      before.spot,
    )
    expect(panelOf(host)).toBe('lookup')
    const read = resumeBlock(text)
    expect(read).toBeGreaterThan(before.slot)
    expect(await leftAt(2)).toBe(read)
  })
})

describe('the look-up’s own scroll, vertical and paged', () => {
  const VERTICAL = { vertical: true }
  for (const view of [
    SET_IN,
    { name: '1280 by 800', width: 1280, height: 800, setIn: true },
    { name: '1366 by 768', width: 1366, height: 768, setIn: true },
  ]) {
    for (const turned of [1, 2, 3, 5, 7]) {
      it(`reopens on the page read, left with the panel open, ${view.name}, ${turned} pages in`, async () => {
        await leftOpenReopens(view, 'ja', VERTICAL, turned)
      })
    }
  }

  for (const last of [true, false]) {
    it(`saves the block the match saved, the ${last ? 'last' : 'first'} word on the page, left with the panel open`, async () => {
      const kept = textOf(await heldAtInJapanese(22))
      expect(onScreen(kept, 22)).toBe(true)
      const before = await leftAt(20)
      const host = await heldAtInJapanese(22)
      const text = textOf(host)
      const runs = runsInSight(text)
      await tappedOnRun(last ? (runs.at(-1) as Range) : runs[0])
      expect(panelOf(host)).toBe('lookup')
      expect(await leftAt(20)).toBe(before)
    })
  }
})

describe('the look-up’s own scroll, the text laid out again with the panel open', () => {
  const BIG: View = {
    name: '1280 by 800',
    width: 1280,
    height: 800,
    setIn: true,
  }
  const SIZE = {
    name: 'a text size step',
    after: SET_IN,
    typeset: (typeset: object) => ({ ...typeset, textSize: 20 }),
    change: () => pressed('='),
  }
  const RESIZE = {
    name: 'a resize to 1280 by 800',
    after: BIG,
    typeset: (typeset: object) => typeset,
    change: async () => {
      await page.viewport(BIG.width, BIG.height)
      await settled()
    },
  }
  const VERTICAL = { edition: 'ja', typeset: { vertical: true } }
  const HORIZONTAL = { edition: 'en', typeset: {} }
  // Not a size step five pages into horizontal text, nor a resize five pages
  // into vertical: there the step saves a block of the narrowed layout, which
  // reopens a page on.
  for (const [flow, name, step, turned] of [
    [VERTICAL, 'vertical', SIZE, 2],
    [VERTICAL, 'vertical', SIZE, 5],
    [VERTICAL, 'vertical', RESIZE, 2],
    [HORIZONTAL, 'horizontal', SIZE, 2],
    [HORIZONTAL, 'horizontal', RESIZE, 2],
    [HORIZONTAL, 'horizontal', RESIZE, 5],
  ] as const) {
    it(`reopens as without a look-up after ${step.name}, ${name} and paged, ${turned} pages in`, async () => {
      const without = await laidOutAgain(flow, step, turned, false)
      expect(await laidOutAgain(flow, step, turned, true)).toEqual(without)
    })
  }
})

/**
 * Where the book reopens after `turned` pages into `uneven`, paged, `step`
 * laying the text out again and the book left, with a word looked up before
 * the step and the panel left open, or with no look-up.
 */
async function laidOutAgain(
  { edition, typeset }: { edition: string; typeset: object },
  step: {
    after: View
    typeset: (typeset: object) => object
    change: () => Promise<void>
  },
  turned: number,
  lookingUp: boolean,
) {
  lookUpOn()
  answersAt()
  const host = await opened(SET_IN, edition, 2, PAGED, typeset, uneven)
  const text = textOf(host)
  for (let i = 0; i < turned; i++) await pressed('PageDown')
  if (lookingUp) {
    const words = wordsInSight(text)
    await tappedOn(words[Math.floor(words.length / 2)])
    expect(panelOf(host)).toBe('lookup')
  }
  await act(() => new Promise((done) => setTimeout(done, 1100)))
  await step.change()
  await act(() => new Promise((done) => setTimeout(done, 1100)))
  if (lookingUp) expect(panelOf(host)).toBe('lookup')
  const left = await leftAt(2)
  const again = shownPlace(
    textOf(
      await opened(
        step.after,
        edition,
        left,
        PAGED,
        step.typeset(typeset),
        uneven,
      ),
    ),
  )
  await leftAt(left)
  return again
}

/**
 * `turned` pages into `uneven` in `edition`, paged, a word looked up and the
 * book left with the panel open, for the first, middle and last word on the
 * page: each saves the block leaving without a look-up saves, and reopens on
 * the same page.
 */
async function leftOpenReopens(
  view: View,
  edition: string,
  typeset: object,
  turned: number,
) {
  lookUpOn()
  answersAt()
  let text = textOf(await opened(view, edition, 2, PAGED, typeset, uneven))
  for (let i = 0; i < turned; i++) await pressed('PageDown')
  const read = await leftAt(2)
  text = textOf(await opened(view, edition, read, PAGED, typeset, uneven))
  const reopened = shownPlace(text)
  await leftAt(read)
  for (const which of ['first', 'middle', 'last'] as const) {
    const host = await opened(view, edition, 2, PAGED, typeset, uneven)
    text = textOf(host)
    for (let i = 0; i < turned; i++) await pressed('PageDown')
    const words = wordsInSight(text)
    await tappedOn(
      which === 'first'
        ? words[0]
        : which === 'last'
          ? (words.at(-1) as Range)
          : words[Math.floor(words.length / 2)],
    )
    expect(panelOf(host)).toBe('lookup')
    const left = await leftAt(2)
    expect(left, `the block saved, the ${which} word looked up`).toBe(read)
    text = textOf(await opened(view, edition, left, PAGED, typeset, uneven))
    expect(shownPlace(text), `reopened, the ${which} word`).toEqual(reopened)
    await leftAt(left)
  }
}

/** A Japanese ask served a two-character dictionary word, for this test. */
function answersAt() {
  dictionaryAt = { length: 2, entries: [] }
  onTestFinished(() => {
    dictionaryAt = undefined
  })
}

/** The utterances asked for by a speech engine that speaks nothing, each started and ended when a test says. */
let spoken: SpeechSynthesisUtterance[] = []

/** That speech engine, with an English voice, for this test. */
function speaksNothing() {
  const real = {
    synthesis: globalThis.speechSynthesis,
    utterance: globalThis.SpeechSynthesisUtterance,
  }
  spoken = []
  vi.stubGlobal('speechSynthesis', {
    getVoices: () => [{ lang: 'en-GB', name: 'Daniel' }],
    speak: (u: SpeechSynthesisUtterance) => spoken.push(u),
    cancel: () => {},
  })
  vi.stubGlobal(
    'SpeechSynthesisUtterance',
    class {
      lang = ''
      rate = 1
      voice: unknown = null
      onstart?: (event: Event) => void
      onend?: (event: Event) => void
      onerror?: (event: Event) => void
      readonly text: string
      constructor(text: string) {
        this.text = text
      }
    },
  )
  onTestFinished(() => {
    vi.stubGlobal('speechSynthesis', real.synthesis)
    vi.stubGlobal('SpeechSynthesisUtterance', real.utterance)
  })
}

/** Two-character runs of Japanese drawn whole in one column on the screen and topmost there, but the mark's; paged, only those in the middle third. */
function runsInSight(text: HTMLElement): Range[] {
  const screen = text.getBoundingClientRect()
  const paged = text.dataset.paged !== undefined
  const third = screen.width / 3
  const found: Range[] = []
  for (const words of text.querySelectorAll<HTMLElement>(
    '[data-block] > [data-text]',
  )) {
    for (const node of wordNodes(words)) {
      for (let i = 0; i + 2 <= node.data.length; i++) {
        const two = node.data.slice(i, i + 2)
        if (!/^[一-鿿ぁ-ゟ゠-ヿ]{2}$/.test(two)) continue
        if ([...MARK.ja].some((c) => two.includes(c))) continue
        const range = document.createRange()
        range.setStart(node, i)
        range.setEnd(node, i + 2)
        const boxes = [...range.getClientRects()]
        if (boxes.length !== 1) continue
        const [box] = boxes
        if (
          box.left < screen.left + 2 ||
          box.right > screen.right - 2 ||
          box.top < screen.top + 2 ||
          box.bottom > screen.bottom - 2
        ) {
          continue
        }
        const x = box.left + box.width / 2
        if (
          paged &&
          (x < screen.left + third + 4 || x > screen.left + 2 * third - 4)
        ) {
          continue
        }
        const [first] = firstOf(range).getClientRects()
        const hit = document.elementFromPoint(
          first.left + first.width / 2,
          first.top + first.height / 2,
        )
        if (hit && words.contains(hit)) found.push(range)
      }
    }
  }
  return found
}

/** The first character of `range`. */
function firstOf(range: Range): Range {
  const one = document.createRange()
  one.setStart(range.startContainer, range.startOffset)
  one.setEnd(range.startContainer, range.startOffset + 1)
  return one
}

/** A real finger's tap on the first character of `run`, which the served answer makes two long, and the look-up answered. */
async function tappedOnRun(run: Range) {
  const [box] = firstOf(run).getClientRects()
  await tapAt(box.left + box.width / 2, box.top + box.height / 2)
  await act(() => new Promise((done) => setTimeout(done, 400)))
  await settled()
}

/** The reader at 1000 by 700 on `everyMarked`'s Japanese, vertical and paged, at the match in block `block` stepped to with `]`. */
async function heldAtInJapanese(block: number): Promise<HTMLElement> {
  lookUpOn()
  answersAt()
  const host = await opened(
    SET_IN,
    'ja',
    20,
    PAGED,
    { vertical: true },
    everyMarked,
  )
  const text = textOf(host)
  expect(text.dataset.vertical).toBe('true')
  await entered(await searched(MARK.ja))
  await pressed('Escape')
  for (let i = 0; i < 5 && litBlocks(text)[0] !== block; i++) {
    await pressed(']')
  }
  expect(litBlocks(text)).toEqual([block])
  return host
}

/** Each paged reading mode, on `uneven`. */
const PAGED_MODES = [
  { name: 'single column', edition: 'en', layout: PAGED, typeset: {} },
  {
    name: 'compared',
    edition: 'en',
    layout: { mode: 'compare', paged: true },
    typeset: {},
  },
  {
    name: 'interlinear',
    edition: 'en',
    layout: { mode: 'interlinear', paged: true },
    typeset: {},
  },
  {
    name: 'vertical',
    edition: 'ja',
    layout: PAGED,
    typeset: { vertical: true },
  },
]
const [SINGLE, COMPARED, INTERLINEAR] = PAGED_MODES

/** A phone's window, where a docked panel goes over the text. */
const TALL: View = { name: '390 by 844', width: 390, height: 844, setIn: false }

/** What lays the text out again, the reader having read in `view`, and the window the book reopens in. */
interface Relaying {
  name: string
  make: (view: View) => Promise<void>
  after: (view: View) => View
}

const SIZE_STEP: Relaying = {
  name: 'a text size step',
  make: () => pressed('='),
  after: (view) => view,
}

/** The window made `height` high, its width kept, as a browser's bar or a phone's keyboard does. */
const shorterTo = (height: number): Relaying => ({
  name: `a resize to ${height} high`,
  make: async (view) => {
    await page.viewport(view.width, height)
    await settled()
  },
  after: (view) => ({ ...view, height }),
})

/**
 * `uneven` opened at block 2 in `view`, paged in `mode`, `turned` pages read,
 * the text laid out again by `change` and the book left: whether the scroll
 * stayed where it was, the page left and the blocks opening on it, and where
 * the book reopens in the window and at the text size it was left in.
 */
async function relaidAndLeft(
  view: View,
  { edition, layout, typeset }: (typeof PAGED_MODES)[number],
  change: Relaying,
  turned: number,
) {
  const text = textOf(await opened(view, edition, 2, layout, typeset, uneven))
  for (let i = 0; i < turned; i++) await pressed('PageDown')
  const spot = spotOf(text)
  await change.make(view)
  await act(() => new Promise((done) => setTimeout(done, 300)))
  const left = {
    still: spotOf(text).left === spot.left && spotOf(text).top === spot.top,
    shown: shownPlace(text),
    opening: openingHere(text),
    carried: firstShown(text).block,
  }
  const slot = await leftAt(2)
  const now = JSON.parse(
    localStorage.getItem('taiyaku.prefs.book-text') ?? '{}',
  )
  const back = textOf(
    await opened(change.after(view), edition, slot, layout, now, uneven),
  )
  const start = openings(back).find(({ block }) => block === left.carried)
  const reopened = {
    shown: shownPlace(back),
    carriedFrom: start && pageStarting(back, start.box),
  }
  await leftAt(slot)
  return { left, reopened }
}

/** Reopened on the page left, or on a page no block opens on, on the first page of the block carried onto it. */
function expectReopenedWhereLeft(
  { left, reopened }: Awaited<ReturnType<typeof relaidAndLeft>>,
  what: string,
) {
  if (left.opening.length > 0) {
    expect(reopened.shown, what).toEqual(left.shown)
  } else {
    expect(
      reopened.shown.page,
      `${what}, the first page of block ${left.carried}, carried onto page ${left.shown.page}`,
    ).toBe(reopened.carriedFrom)
  }
}

describe('reopening a paged book after the text is laid out again where it stands', () => {
  for (const [mode, view, change, turned] of [
    [COMPARED, TALL, shorterTo(760), 5],
    [SINGLE, SET_IN, shorterTo(620), 4],
    [SINGLE, TALL, SIZE_STEP, 3],
    [INTERLINEAR, TALL, SIZE_STEP, 3],
  ] as const) {
    it(`returns to the page left after ${change.name} that left the scroll where it was, ${mode.name}, ${view.name}, ${turned} pages in`, async () => {
      const found = await relaidAndLeft(view, mode, change, turned)
      expect(found.left.still, 'the scroll where it was').toBe(true)
      expectReopenedWhereLeft(found, 'reopened')
    })
  }

  for (const mode of PAGED_MODES) {
    for (const [view, height] of [
      [SET_IN, 620],
      [TALL, 760],
    ] as const) {
      for (const change of [SIZE_STEP, shorterTo(height)]) {
        it(`returns to the page left after ${change.name}, ${mode.name}, ${view.name}, one to five pages in`, async () => {
          for (let turned = 1; turned <= 5; turned++) {
            expectReopenedWhereLeft(
              await relaidAndLeft(view, mode, change, turned),
              `${turned} pages in`,
            )
          }
        })
      }
    }
  }
})

describe('the place a look-up moved the text to, vertical and scrolled, with the panel closed', () => {
  for (const view of [
    SET_IN,
    { name: '1366 by 768', width: 1366, height: 768, setIn: true },
  ]) {
    it(`is saved, and the book reopens at its block, ${view.name}`, async () => {
      lookUpOn()
      answersAt()
      const vertical = { vertical: true }
      const host = await opened(view, 'ja', 2, SCROLLED, vertical, uneven)
      const text = textOf(host)
      for (let i = 0; i < 2; i++) {
        await act(async () => addProgress(text, 457))
        await settled()
      }
      const before = topOf(text)
      await tappedOn(wordsInSight(text).at(-1) as Range)
      expect(panelOf(host)).toBe('lookup')
      const moved = topOf(text)
      expect(moved.block, 'the look-up moved the text on a block').toBe(
        before.block + 1,
      )
      await pressed('Escape')
      expect(panelOf(host)).not.toBe('lookup')
      expect(topOf(text), 'closing the panel moved nothing').toEqual(moved)
      const slot = await leftAt(2)
      expect(slot, 'the block saved').toBe(moved.block)
      const back = textOf(
        await opened(view, 'ja', slot, SCROLLED, vertical, uneven),
      )
      expect(topOf(back)).toEqual({ block: moved.block, at: 0 })
      await leftAt(slot)
    })
  }
})

describe('a book opened and left unread', () => {
  for (const mode of PAGED_MODES) {
    it(`sends no progress, opened on its first page at a block one opens before, ${mode.name}`, async () => {
      const { edition, layout, typeset } = mode
      let text = textOf(
        await opened(SET_IN, edition, 0, layout, typeset, uneven),
      )
      const later = openingHere(text).filter((block) => block > 0)
      expect(
        later,
        'blocks opening on the first page after block 0',
      ).not.toEqual([])
      await leftAt(0)
      text = textOf(
        await opened(SET_IN, edition, later[0], layout, typeset, uneven),
      )
      expect(pageAlong(text), 'the first page').toBe(0)
      expect(await leftAt(later[0])).toBe(later[0])
      expect(progressSent()).toEqual([])
    })
  }

  it('sends no progress for a book never read, laid out again at its start', async () => {
    await page.viewport(SET_IN.width, SET_IN.height)
    await settled()
    localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(PAGED))
    localStorage.setItem('taiyaku.prefs.book-text', '{}')
    const answers = served('en', 0, uneven)
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string, init?: RequestInit) =>
        url === '/api/works/w/progress' && init?.method !== 'PUT'
          ? new Response(null, { status: 204 })
          : answers(url, init),
      ),
    )
    const host = mount(
      <div style={{ position: 'fixed', inset: 0 }}>
        <BookReader work="w" onLeave={() => {}} />
      </div>,
    )
    for (let i = 0; i < 100 && !host.querySelector('[data-block="0"]'); i++) {
      await act(() => new Promise((done) => setTimeout(done, 10)))
    }
    await settled()
    const text = textOf(host)
    await pressed('t')
    expect(panelOf(host)).toBe('contents')
    await pressed('Escape')
    await shorterTo(620).make(SET_IN)
    await pressed('=')
    expect(pageAlong(text), 'the first page').toBe(0)
    await leftAt(0)
    expect(progressSent()).toEqual([])
  })
})

/** `everyMarked` with block 30 a heading, which the contents list, and block 20's words a link to block 35. */
const routed = {
  ...everyMarked,
  editions: Object.fromEntries(
    Object.entries(everyMarked.editions).map(([key, edition]) => [
      key,
      {
        ...edition,
        blocks: edition.blocks.map((block, i) => ({
          ...block,
          heading: i === 30,
          ...(i === 20 && {
            marks: [
              { kind: 'link', start: 6, end: 11, block: 35, note: false },
            ],
          }),
        })),
      },
    ]),
  ),
}

/** The reader in `view` on `sent`, opened at block 20 and stepped with `]` to the match in block 22, the search panel closed. */
async function steppedTo22(
  view: View,
  { edition, layout, typeset }: (typeof PAGED_MODES)[number],
  sent: object = everyMarked,
): Promise<HTMLElement> {
  const host = await opened(view, edition, 20, layout, typeset, sent)
  const text = textOf(host)
  await entered(await searched(MARK[edition]))
  await pressed('Escape')
  for (let i = 0; i < 5 && litBlocks(text)[0] !== 22; i++) await pressed(']')
  expect(litBlocks(text)).toEqual([22])
  return host
}

/** The block of the last match before `from`: each block opens with one. */
const matchBefore = ({ block, at }: { block: number; at: number }) =>
  at === 0 ? block - 1 : block

const SIZE_UP: Relaying = { ...SIZE_STEP, name: 'a text size step up' }
const SIZE_DOWN: Relaying = {
  ...SIZE_STEP,
  name: 'a text size step down',
  make: () => pressed('-'),
}

// Laying the text out again where it stands keeps the hold on a match gone
// to, though compared a size step goes back to the row's start, onto a page
// the match may not be on; only a hand move or a navigation ends it.
describe('a match gone to, through the text laid out again where it stands', () => {
  for (const mode of [SINGLE, COMPARED]) {
    for (const [view, height] of [
      [SET_IN, 620],
      [TALL, 760],
    ] as const) {
      const where = `${mode.name} and paged, ${view.name}`
      for (const change of [SIZE_UP, SIZE_DOWN, shorterTo(height)]) {
        for (const { key, block } of [
          { key: ']', block: 23 },
          { key: '[', block: 21 },
        ]) {
          it(`steps on from it to block ${block} on ${key} after ${change.name}, ${where}`, async () => {
            const text = textOf(await steppedTo22(view, mode))
            await change.make(view)
            await pressed(key)
            expect(litBlocks(text)).toEqual([block])
          })
        }
      }

      it(`goes from the place after a page turned by hand once the text size has changed, ${where}`, async () => {
        const text = textOf(await steppedTo22(view, mode))
        await pressed('=')
        await pressed('PageUp')
        expect(
          matchShows(text, { block: 22, start: 0 }),
          'the match is off the page',
        ).toBe(false)
        const before = matchBefore(firstShown(text))
        expect(before, 'a step from the match would give the same').not.toBe(21)
        await pressed('[')
        expect(litBlocks(text)).toEqual([before])
      })

      it(`goes from the place the contents went to after a match gone to, ${where}`, async () => {
        const host = await steppedTo22(view, mode, routed)
        const text = textOf(host)
        await pressed('t')
        await act(async () => {
          document
            .querySelector<HTMLButtonElement>(
              '[role="dialog"][aria-label="Contents"] ol button',
            )
            ?.click()
        })
        await settled()
        await pressed('Escape')
        expect(
          matchShows(text, { block: 30, start: 0 }),
          'the heading gone to',
        ).toBe(true)
        expect(
          matchShows(text, { block: 22, start: 0 }),
          'the match is off the page',
        ).toBe(false)
        await pressed('[')
        expect(litBlocks(text)).toEqual([29])
      })

      it(`goes from the place Back returns to after a match gone to, ${where}`, async () => {
        const { edition, layout, typeset } = mode
        const host = await opened(view, edition, 20, layout, typeset, routed)
        const text = textOf(host)
        // The reader's place is the first character read on the page shown:
        // compared, a row can run over many pages, so it need not open the row.
        const from = firstShown(text)
        const link = text.querySelector<HTMLElement>(
          '[data-block="20"] a[data-link-block], [data-pick="left:20"] a[data-link-block]',
        )
        expect(link, 'block 20’s link on the page').not.toBeNull()
        await act(async () => {
          link?.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
          )
        })
        await settled()
        await entered(await searched(MARK[edition]))
        await pressed('Escape')
        expect(litBlocks(text), 'the match the link led to').toEqual([35])
        const back = [
          ...host.querySelectorAll<HTMLElement>('.reader-bottom-bar button'),
        ].find((button) => button.textContent === 'Back')
        expect(back, 'Back is offered').toBeDefined()
        await act(async () => back?.click())
        await settled()
        expect(
          firstShown(text),
          'Back returns to the page the link was followed from',
        ).toEqual(from)
        expect(
          matchShows(text, { block: 35, start: 0 }),
          'the match is off the page',
        ).toBe(false)
        await pressed('[')
        expect(litBlocks(text)).toEqual([matchBefore(from)])
      })
    }
  }
})

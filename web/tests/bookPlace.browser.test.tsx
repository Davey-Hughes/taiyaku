import { act } from 'react'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import { BookReader } from '../src/BookReader'
import {
  type Anchor,
  anchorOf,
  shows,
  topOf,
  wordNodes,
} from '../src/bookPages'
import '../src/index.css'
import { typeInto } from './support/input'
import { mount } from './support/mount'

vi.mock('../src/Account', () => ({ Account: () => null }))

// A figure's image from nowhere, which fails at once: the dev server's proxy
// has no server behind it to ask, and says so on every figure drawn.
vi.mock('../src/book', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/book')>()),
  bookImageUrl: () => 'data:,',
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
  localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(layout))
  localStorage.setItem('taiyaku.prefs.book-text', JSON.stringify(typeset))
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(sent)
      if (url.startsWith('/api/dictionary/')) {
        return new Response(null, { status: 404 })
      }
      if (url === '/api/works/w/notes') {
        // A note written is stored as sent, as the server answers.
        const doc =
          init?.method === 'PUT'
            ? JSON.parse(String(init.body))
            : { work: 'w', notes: [] }
        return Response.json(doc, { headers: { etag: '"n"' } })
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [{ edition, slot, updated: 'now' }] })
    }),
  )
  const host = mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  const at = `[data-block="${slot}"]`
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

describe('the reader’s place, compared or interlinear', () => {
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

/** Enter pressed in the search box, which goes to the next match. */
async function entered(box: HTMLInputElement) {
  await act(async () => {
    box.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
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

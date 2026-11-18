import { act } from 'react'
import {
  afterAll,
  afterEach,
  beforeAll,
  describe,
  expect,
  it,
  vi,
} from 'vitest'
import { page } from 'vitest/browser'
import { BookReader } from '../src/BookReader'
import '../src/index.css'
import { typeInto } from './support/input'
import { mount } from './support/mount'

vi.mock('../src/Account', () => ({ Account: () => null }))

// What the reader sets over its text, a docked panel, the selection's bar and
// a note's mark, measured in Chromium against what they must not cover or
// leave: the match the search lit, the window, the text's own clip.

const SAID =
  '吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。'
const WORDS =
  'It was a bright cold day in April, and the clocks were striking thirteen. '

/** Two editions, aligned block by block: `ねずみ` in three of the Japanese blocks, `mouse` in their English. */
const book = {
  title: '猫',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: Array.from({ length: 30 }, (_, i) => {
        const said = SAID.repeat(3)
        const text = [4, 12, 20].includes(i)
          ? `${said.slice(0, 20)}ねずみ${said.slice(20)}`
          : said
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
    },
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 30 }, (_, i) => {
        const text = `${[4, 12, 20].includes(i) ? 'A mouse. ' : ''}${WORDS.repeat(12)}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
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

afterEach(() => {
  localStorage.removeItem('taiyaku.prefs.book-layout')
  localStorage.removeItem('taiyaku.prefs.book-text')
})

/**
 * Each request for the server made with no test's stub in place, which the
 * dev server would pass on to the real one. A reader saves its place as it is
 * unmounted, after its test has ended, so the stub must outlast the mount.
 */
const unstubbed: string[] = []
const realFetch = globalThis.fetch
beforeAll(() => {
  globalThis.fetch = (input, init) => {
    const url = input instanceof Request ? input.url : String(input)
    if (!new URL(url, location.href).pathname.startsWith('/api/')) {
      return realFetch(input, init)
    }
    unstubbed.push(url)
    return Promise.resolve(new Response(null, { status: 204 }))
  }
})
afterAll(() => {
  vi.unstubAllGlobals()
  globalThis.fetch = realFetch
  expect(unstubbed).toEqual([])
})

/**
 * Two windows in the sidebar layout: one wide enough that an open panel's
 * set-in leaves horizontal text its whole column, and one where it does not
 * and the text is laid out again beside the panel.
 */
const WIDE = { name: '1366 by 768', width: 1366, height: 768 }
const SET_IN = { name: '1000 by 700', width: 1000, height: 700 }

/** The reader filling a `view`-sized window, 1366 by 768 unless named, on `edition` of `sent`, set by `layout` and `typeset`. */
async function opened(
  edition: string,
  layout: object,
  typeset: object,
  notes: unknown[] = [],
  view = WIDE,
  sent: unknown = book,
): Promise<HTMLElement> {
  await page.viewport(view.width, view.height)
  localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(layout))
  localStorage.setItem('taiyaku.prefs.book-text', JSON.stringify(typeset))
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(sent)
      if (url === '/api/works/w/notes') {
        return Response.json({ work: 'w', notes }, { headers: { etag: '"n"' } })
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({
        positions: [{ edition, slot: 0, updated: 'now' }],
      })
    }),
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
  return host
}

/** Three frames inside `act`, for what a placing sent to be drawn. */
async function settled() {
  for (let i = 0; i < 3; i++) {
    await act(() => new Promise((done) => requestAnimationFrame(done)))
  }
}

function key(key: string, on: EventTarget = window, init = {}) {
  return act(async () => {
    on.dispatchEvent(
      new KeyboardEvent('keydown', {
        key,
        cancelable: true,
        bubbles: true,
        ...init,
      }),
    )
  })
}

const meets = (a: DOMRect, b: DOMRect) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom

const onScreen = (r: DOMRect) =>
  r.left >= 0 &&
  r.top >= 0 &&
  r.right <= window.innerWidth &&
  r.bottom <= window.innerHeight

/** `n` characters of block `block`'s words from `from` selected, and the pointer let go as a drag ends. */
async function select(
  host: HTMLElement,
  block: number,
  from: number,
  n: number,
) {
  const words = host.querySelector(
    `[data-block="${block}"] [data-text]`,
  ) as HTMLElement
  const walker = document.createTreeWalker(words, NodeFilter.SHOW_TEXT)
  const node = walker.nextNode() as Text
  const range = document.createRange()
  range.setStart(node, from)
  range.setEnd(node, from + n)
  window.getSelection()?.removeAllRanges()
  window.getSelection()?.addRange(range)
  const text = host.querySelector('.book-text') as HTMLElement
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
  return range.getBoundingClientRect()
}

const FLOWS = [
  { name: 'vertical, scrolled', edition: 'ja', vertical: true, paged: false },
  { name: 'vertical, paged', edition: 'ja', vertical: true, paged: true },
  { name: 'horizontal, paged', edition: 'en', vertical: false, paged: true },
  {
    name: 'horizontal, scrolled',
    edition: 'en',
    vertical: false,
    paged: false,
  },
]

const QUERY: Record<string, string> = { ja: 'ねずみ', en: 'mouse' }

const shown = (host: HTMLElement) =>
  host.querySelector('.book-text .book-found')?.getBoundingClientRect()

/** Whether `r` is drawn inside the text's own box, where it is not clipped. */
const inText = (host: HTMLElement, r: DOMRect) => {
  const text = (
    host.querySelector('.book-text') as HTMLElement
  ).getBoundingClientRect()
  return (
    r.left >= text.left - 0.5 &&
    r.right <= text.right + 0.5 &&
    r.top >= text.top - 0.5 &&
    r.bottom <= text.bottom + 0.5
  )
}

describe('a match stepped to with the search open', () => {
  for (const view of [WIDE, SET_IN]) {
    for (const flow of FLOWS) {
      it(`is drawn clear of the panel, ${flow.name}, and held there once it closes, ${view.name}`, async () => {
        const host = await opened(
          flow.edition,
          { paged: flow.paged },
          { vertical: flow.vertical },
          [],
          view,
        )
        await key('/')
        const box = host.querySelector(
          '.docked-panel input[type="search"]',
        ) as HTMLInputElement
        await typeInto(box, QUERY[flow.edition])
        await act(() => new Promise((done) => setTimeout(done, 150)))
        for (let n = 0; n < 3; n++) {
          await key('Enter', box)
          await settled()
          const found = shown(host)
          const panel = host
            .querySelector('.docked-panel')
            ?.getBoundingClientRect()
          expect(found, `match ${n}`).toBeDefined()
          expect(panel).toBeDefined()
          expect(meets(found as DOMRect, panel as DOMRect), `match ${n}`).toBe(
            false,
          )
          expect(inText(host, found as DOMRect), `match ${n}`).toBe(true)
          const text = (
            host.querySelector('.book-text') as HTMLElement
          ).getBoundingClientRect()
          expect(meets(text, panel as DOMRect), `text by match ${n}`).toBe(
            false,
          )
        }
        await key('Escape', box)
        await settled()
        expect(host.querySelector('.docked-panel')).toBeNull()
        const found = shown(host)
        expect(found).toBeDefined()
        expect(inText(host, found as DOMRect)).toBe(true)
      })
    }
  }
})

describe('the selection bar', () => {
  it('is on screen for a selection in the rightmost vertical column', async () => {
    const host = await opened('ja', {}, { vertical: true })
    const picked = await select(host, 0, 0, 4)
    const bar = host
      .querySelector('.book-selection-bar')
      ?.getBoundingClientRect() as DOMRect
    expect(bar).toBeDefined()
    expect(onScreen(bar)).toBe(true)
    expect(meets(bar, picked)).toBe(false)
    // Beside the column, at its words, not below the line's whole height.
    expect(bar.right).toBeLessThanOrEqual(picked.left)
    expect(bar.top).toBeLessThan(picked.bottom)
  })

  it('is on screen for a selection near the bottom of horizontal text', async () => {
    const host = await opened('en', {}, {})
    const text = host.querySelector('.book-text') as HTMLElement
    const block = host.querySelector('[data-block="3"]') as HTMLElement
    await act(async () => {
      text.scrollTop +=
        block.getBoundingClientRect().top - (window.innerHeight - 40)
    })
    await settled()
    const picked = await select(host, 3, 0, 6)
    expect(picked.bottom).toBeGreaterThan(window.innerHeight - 60)
    const bar = host
      .querySelector('.book-selection-bar')
      ?.getBoundingClientRect() as DOMRect
    expect(bar).toBeDefined()
    expect(onScreen(bar)).toBe(true)
    expect(meets(bar, picked)).toBe(false)
  })
})

/** A note on block 0 as a whole, which draws its mark beside the block. */
const blockNote = (edition: string) => ({
  id: '01JAAAAAAAAAAAAAAAAAAAAAAA',
  edition,
  slot: 0,
  color: 'yellow',
  body: 'a note',
  created: '2026-01-01T00:00:00.000Z',
  updated: '2026-01-01T00:00:00.000Z',
})

/** `book` with each edition's block 0 a chapter's heading, which is set larger than its text. */
const headed = {
  ...book,
  editions: {
    ja: {
      ...book.editions.ja,
      blocks: [
        { text: '第一章', heading: true, sentences: [[0, 3]] },
        ...book.editions.ja.blocks.slice(1),
      ],
    },
    en: {
      ...book.editions.en,
      blocks: [
        { text: 'Chapter One', heading: true, sentences: [[0, 11]] },
        ...book.editions.en.blocks.slice(1),
      ],
    },
  },
}

/** A phone's window, where the text is as wide as the screen. */
const PHONE = { name: '390 by 844', width: 390, height: 844 }

describe('a note’s mark', () => {
  const blocks = [
    { kind: 'a paragraph', tag: 'P', sent: book },
    { kind: 'a heading', tag: 'H2', sent: headed },
  ]
  const sizes = [
    { name: 'the default size', typeset: {}, view: PHONE },
    { name: 'the largest size', typeset: { textSize: 40 }, view: PHONE },
    { name: 'the largest size', typeset: { textSize: 40 }, view: WIDE },
  ]
  for (const { kind, tag, sent } of blocks) {
    for (const size of sizes) {
      for (const flow of FLOWS) {
        it(`beside ${kind} is drawn inside the text, ${flow.name}, at ${size.name}, ${size.view.name}`, async () => {
          const host = await opened(
            flow.edition,
            { paged: flow.paged },
            { vertical: flow.vertical, ...size.typeset },
            [blockNote(flow.edition)],
            size.view,
            sent,
          )
          const block = host.querySelector('[data-block="0"]')
          expect(block?.tagName).toBe(tag)
          const mark = block
            ?.querySelector('.book-note-mark')
            ?.getBoundingClientRect() as DOMRect
          expect(mark).toBeDefined()
          expect(mark.width).toBeGreaterThan(0)
          expect(inText(host, mark)).toBe(true)
        })
      }
    }
  }
})

import { act, type CSSProperties } from 'react'
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { BookReader } from '../src/BookReader'
import {
  type Anchor,
  kept,
  onPage,
  pagesOf,
  pageTurnedBy,
  readingArea,
  show,
  shows,
  topBlock,
  topOf,
  turn,
  wordNodes,
} from '../src/bookPages'
import '../src/index.css'
import { typeInto } from './support/input'
import { mount } from './support/mount'

vi.mock('../src/Account', () => ({ Account: () => null }))

// The book's image is set by the test once the text is drawn, so it loads
// after the first paint; until then it is one the browser cannot draw.
vi.mock('../src/book', async (original) => ({
  ...(await original<typeof import('../src/book')>()),
  bookImageUrl: () => 'data:,',
}))

/**
 * `bookPages.ts` against a real layout, which jsdom has none of: where a
 * page starts inside a paragraph, and the page that holds it once the text
 * is laid out again at another size.
 */

const WORDS = Array.from(
  { length: 400 },
  (_, i) => `word${i} the quick brown fox jumps over the lazy dog.`,
).join(' ')

/** A paged text, `book.css`'s columns at a fixed size, of one long paragraph between two short ones. */
function text(paged: boolean): HTMLElement {
  const host = mount(
    <div
      className="book-text"
      data-paged={paged || undefined}
      style={{
        boxSizing: 'border-box',
        width: 320,
        height: 240,
        padding: 24,
        fontSize: 16,
        lineHeight: 1.5,
        overflow: paged ? 'hidden' : 'auto',
        ...(paged ? { columnCount: 1, columnFill: 'auto', columnGap: 48 } : {}),
      }}
    >
      <p data-block={0}>
        <span data-text>A short opening.</span>
      </p>
      <p data-block={1}>
        <span data-text>
          {WORDS.slice(0, 900)}
          <mark>{WORDS.slice(900, 1200)}</mark>
          {WORDS.slice(1200)}
        </span>
      </p>
      <p data-block={2}>
        <span data-text>A short close.</span>
      </p>
    </div>,
  )
  return host.firstElementChild as HTMLElement
}

const pageOn = (el: HTMLElement) => Math.round(el.scrollLeft / el.clientWidth)

describe('bookPages, laid out', () => {
  it('finds the character a page starts on inside a paragraph', () => {
    const el = text(true)
    el.scrollTo({ left: 3 * el.clientWidth })
    const top = topOf(el)
    expect(top.block).toBe(1)
    expect(top.at).toBeGreaterThan(0)
    expect(shows(el, top)).toBe(true)
    expect(shows(el, { block: 1, at: Math.max(top.at - 1, 0) })).toBe(false)
  })

  it('keeps a page that starts inside a paragraph on screen as the text is sized up', () => {
    const el = text(true)
    el.scrollTo({ left: 4 * el.clientWidth })
    const top = topOf(el)
    el.style.fontSize = '22px'
    show(el, top.block, top.at)
    expect(shows(el, top)).toBe(true)
    // Placed by its block alone, the reader goes to the paragraph's first
    // line, pages earlier, and the page is lost.
    show(el, top.block)
    expect(shows(el, top)).toBe(false)
  })

  it('comes back to the same page on sizing the text up and down again', () => {
    const el = text(true)
    el.scrollTo({ left: 5 * el.clientWidth })
    const began = topOf(el)
    for (const size of ['20px', '24px', '18px', '16px']) {
      const anchor = kept(el, began)
      expect(anchor).toEqual(began)
      el.style.fontSize = size
      show(el, anchor.block, anchor.at)
    }
    expect(pageOn(el)).toBe(5)
    expect(topOf(el)).toEqual(began)
  })

  it('takes the top of the page once a turn has taken the anchor off it', () => {
    const el = text(true)
    el.scrollTo({ left: 2 * el.clientWidth })
    const before = topOf(el)
    el.scrollTo({ left: 6 * el.clientWidth })
    expect(kept(el, before)).toEqual(topOf(el))
    expect(kept(el, before)).not.toEqual(before)
  })

  it('keeps the line a scrolled text is at as it is sized up', () => {
    const el = text(false)
    el.scrollTo({ top: 1500 })
    const top = topOf(el)
    expect(top.at).toBeGreaterThan(0)
    el.style.fontSize = '22px'
    show(el, top.block, top.at)
    expect(shows(el, top)).toBe(true)
  })
})

const SAID =
  '吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。'

const FLOWS = [
  { vertical: false, paged: false },
  { vertical: false, paged: true },
  { vertical: true, paged: false },
  { vertical: true, paged: true },
]

const named = ({ vertical, paged }: (typeof FLOWS)[number]) =>
  `${vertical ? 'vertical' : 'horizontal'}, ${paged ? 'paged' : 'scrolled'}`

/**
 * `.book-text` as `book.css` sets it in a 400 by 300 reader, of `blocks`.
 * Paged, each block opens a page, so a block shown alone tops its page,
 * unless `breaks` is false.
 */
function flowing(
  { vertical, paged }: (typeof FLOWS)[number],
  blocks: string[],
  breaks = true,
): HTMLElement {
  const host = mount(
    <div
      className="reader"
      style={{
        display: 'flex',
        flexDirection: 'column',
        width: 400,
        height: 300,
      }}
    >
      <div
        className="book-text"
        data-vertical={vertical || undefined}
        data-paged={paged || undefined}
        lang="ja"
        style={
          {
            '--book-text-size': '16px',
            '--book-line-spacing': 1.6,
            '--book-column-width': '544px',
          } as CSSProperties
        }
      >
        {blocks.map((words, i) => (
          <p
            key={i}
            data-block={i}
            style={
              breaks && paged && blocks.length > 1
                ? { breakBefore: 'column' }
                : {}
            }
          >
            <span data-text>{words}</span>
          </p>
        ))}
      </div>
    </div>,
  )
  return host.querySelector('.book-text') as HTMLElement
}

const forty = Array.from({ length: 40 }, () => SAID.repeat(2))

describe('bookPages, in each of the four flows', () => {
  for (const flow of FLOWS) {
    describe(named(flow), () => {
      it('shows a block at the top', () => {
        const el = flowing(flow, forty)
        show(el, 20)
        expect(topBlock(el)).toBe(20)
        expect(topOf(el)).toEqual({ block: 20, at: 0 })
      })

      it('shows a character inside a block', () => {
        const el = flowing(flow, forty)
        show(el, 20, 37)
        expect(shows(el, { block: 20, at: 37 })).toBe(true)
        expect(shows(el, { block: 19, at: 0 })).toBe(false)
      })

      it('turns on, and back', () => {
        const el = flowing(flow, forty)
        show(el, 20)
        const page = pagesOf(el, [], 20)?.at
        turn(el, 1)
        const on = topBlock(el)
        expect(on).toBeGreaterThan(20)
        if (flow.paged) expect(pagesOf(el, [], on)?.at).toBe((page ?? 0) + 1)
        turn(el, -1)
        expect(topBlock(el)).toBeLessThan(on)
        if (flow.paged) expect(pagesOf(el, [], 20)?.at).toBe(page)
      })

      it(`counts pages ${flow.paged ? 'paged' : 'not at all, scrolled'}`, () => {
        const pages = pagesOf(flowing(flow, forty), [], 0)
        if (flow.paged) expect(pages?.of).toBeGreaterThanOrEqual(2)
        else expect(pages).toBeUndefined()
      })

      it('finds the character a screen starts on inside a paragraph', () => {
        const el = flowing(flow, [SAID.repeat(60)])
        turn(el, 1)
        turn(el, 1)
        const top = topOf(el)
        expect(top.block).toBe(0)
        expect(top.at).toBeGreaterThan(0)
        expect(shows(el, top)).toBe(true)
        expect(shows(el, { block: 0, at: top.at - 1 })).toBe(false)
        show(el, 0)
        show(el, top.block, top.at)
        expect(topOf(el)).toEqual(top)
      })

      if (flow.paged) {
        it(`turns ${flow.vertical ? 'on at the left third and back at the right' : 'back at the left third and on at the right'}`, () => {
          const el = flowing(flow, forty)
          const box = el.getBoundingClientRect()
          const at = (part: number) => box.left + part * box.width
          const left = flow.vertical ? 1 : -1
          expect(pageTurnedBy(el, at(0.1))).toBe(left)
          expect(pageTurnedBy(el, at(0.5))).toBe(0)
          expect(pageTurnedBy(el, at(0.9))).toBe(-left)
        })
      }
    })
  }
})

/**
 * A paged text 300 px wide in a reader 900 px wide, set in from the right by
 * `beside` px as `book.css` sets it beside a docked panel, and centred in
 * what is left, as a wide window draws it.
 */
function narrow(beside = 0): HTMLElement {
  const host = mount(
    <div
      className="reader"
      style={{
        display: 'flex',
        flexDirection: 'column',
        width: 900,
        height: 300,
      }}
    >
      <div
        className="book-text"
        data-paged
        style={{ width: 300, marginRight: beside }}
      >
        <p data-block={0}>
          <span data-text>{WORDS.slice(0, 3000)}</span>
        </p>
      </div>
    </div>,
  )
  return host.querySelector('.book-text') as HTMLElement
}

describe('the reading area a tap’s thirds are of', () => {
  it('is the reader’s box where the text is narrower, so the room beside the text is in an outer third and the text’s own left third in the middle', () => {
    const el = narrow()
    const reader = el.parentElement as HTMLElement
    expect(el.getBoundingClientRect().left).toBe(300)
    expect(readingArea(el)).toEqual({ left: 0, right: 900 })
    expect(pageTurnedBy(el, 100)).toBe(-1)
    expect(pageTurnedBy(el, 350)).toBe(0)
    expect(pageTurnedBy(el, 550)).toBe(0)
    expect(pageTurnedBy(el, 800)).toBe(1)
    expect(onPage(el, reader, 100)).toBe(true)
    expect(onPage(el, reader, 800)).toBe(true)
    expect(onPage(el, el.querySelector('[data-text]'), 350)).toBe(true)
    expect(onPage(el, document.body, 100)).toBe(false)
  })

  it('stops short of the room set beside the text for a docked panel, which is not the page’s', () => {
    const el = narrow(300)
    const reader = el.parentElement as HTMLElement
    expect(el.getBoundingClientRect().left).toBe(150)
    expect(readingArea(el)).toEqual({ left: 0, right: 600 })
    expect(pageTurnedBy(el, 180)).toBe(-1)
    expect(pageTurnedBy(el, 380)).toBe(0)
    expect(pageTurnedBy(el, 420)).toBe(1)
    expect(onPage(el, reader, 590)).toBe(true)
    expect(onPage(el, reader, 610)).toBe(false)
  })
})

/** Whether some line of block `block` of `el` lies inside its box, top to bottom. */
function drawnOnPage(el: HTMLElement, block: number): boolean {
  const box = el.getBoundingClientRect()
  const words = el.querySelector(`[data-block="${block}"] [data-text]`)
  const range = document.createRange()
  range.selectNodeContents(words as Node)
  return [...range.getClientRects()].some(
    (r) => r.height > 0 && r.top >= box.top - 1 && r.bottom <= box.bottom + 1,
  )
}

describe('bookPages, vertical paged, several short blocks a page', () => {
  it('shows a block on its page, the page’s blocks shown and the one before not', () => {
    const short = Array.from({ length: 40 }, (_, i) =>
      SAID.slice(0, 10 + (i % 4) * 7),
    )
    const el = flowing({ vertical: true, paged: true }, short, false)
    const between = [...Array(40).keys()].filter((b) => drawnOnPage(el, b))
    // A page holds several blocks, or this tests no more than one a page does.
    expect(between.length).toBeGreaterThan(3)
    for (const k of [9, 10, 11, 17, 23, 30]) {
      show(el, k)
      const on = [...Array(40).keys()].filter((b) => drawnOnPage(el, b))
      const top = topOf(el)
      expect(on).toContain(k)
      expect(topBlock(el)).toBe(on[0])
      expect(topBlock(el)).toBeLessThanOrEqual(k)
      for (const b of on) {
        expect(shows(el, { block: b, at: b === top.block ? top.at : 0 })).toBe(
          true,
        )
      }
      const before =
        top.at > 0
          ? { block: top.block, at: top.at - 1 }
          : { block: top.block - 1, at: short[top.block - 1].length - 1 }
      expect(shows(el, before)).toBe(false)
    }
  })
})

/** Twelve paragraphs of about a page each, and an image before the third. */
const illustrated = {
  title: 'Illustrated',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 12 }, (_, i) => {
        const text = `Block ${i} opens here. ${WORDS.slice(0, 300)}`
        return {
          text,
          heading: false,
          sentences: [
            [0, 20],
            [20, text.length],
          ],
        }
      }),
      figures: [{ before: 2, image: 0, alt: 'a map' }],
    },
  },
  alignments: [],
}

/** The book reader, 400 by 300, opened at block 5, scrolled or paged. */
async function reader(paged: boolean): Promise<HTMLElement> {
  localStorage.setItem(
    'taiyaku.prefs.book-layout',
    JSON.stringify({ mode: 'scroll', paged }),
  )
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(illustrated)
      if (url === '/api/works/w/notes') {
        return Response.json(
          { work: 'w', notes: [] },
          { headers: { etag: '"n"' } },
        )
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({
        positions: [{ edition: 'en', slot: 5, updated: 'now' }],
      })
    }),
  )
  const host = mount(
    <div style={{ width: 400, height: 300 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  // Each wait its own `act`, which draws what an update set only as it ends.
  for (let i = 0; i < 100 && !host.querySelector('[data-block="5"]'); i++) {
    await act(() => new Promise((done) => setTimeout(done, 10)))
  }
  await settled()
  return host.querySelector('.book-text') as HTMLElement
}

/** Two frames inside `act`, for the scroll a placing sent to be seen and drawn. */
async function settled() {
  for (let i = 0; i < 2; i++) {
    await act(() => new Promise((done) => requestAnimationFrame(done)))
  }
}

/** A tall image, a blob URL drawn on a canvas, as a book's full-page plate is. */
async function plate(): Promise<string> {
  const canvas = document.createElement('canvas')
  canvas.width = 300
  canvas.height = 900
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done))
  if (!blob) throw new Error('the canvas drew no image')
  return URL.createObjectURL(blob)
}

describe('an image loading after the page is drawn', () => {
  // After every mount is torn down, since a reader saves its place as it
  // goes, to the stubbed server rather than the real one.
  afterAll(() => {
    localStorage.removeItem('taiyaku.prefs.book-layout')
    vi.unstubAllGlobals()
  })

  it('leaves a scrolled reader where they scrolled to, not on the place they opened at', async () => {
    const text = await reader(false)
    await act(async () => {
      text.scrollBy({ top: -120 })
    })
    await settled()
    const scrolled = topOf(text)
    // Back into block 4, the opening line still in sight below, so the
    // reader's anchor is still that line.
    expect(scrolled.block).toBe(4)
    expect(shows(text, { block: 5, at: 0 })).toBe(true)
    const image = text.querySelector('.book-figure img') as HTMLImageElement
    const src = await plate()
    await act(async () => {
      const loaded = new Promise((done) =>
        image.addEventListener('load', done, { once: true }),
      )
      image.src = src
      await loaded
    })
    await settled()
    const now = topOf(text)
    expect(now.block).toBe(scrolled.block)
    // Within a line's worth of characters of where the reader was.
    expect(Math.abs(now.at - scrolled.at)).toBeLessThan(40)
    URL.revokeObjectURL(src)
  })

  for (const paged of [false, true]) {
    it(`leaves the reader on their sentence, ${paged ? 'paged' : 'scrolled'}`, async () => {
      const text = await reader(paged)
      const first = { block: 5, at: 0 }
      expect(shows(text, first)).toBe(true)
      if (!paged) expect(topOf(text)).toEqual(first)
      const image = text.querySelector('.book-figure img') as HTMLImageElement
      const src = await plate()
      await act(async () => {
        const loaded = new Promise((done) =>
          image.addEventListener('load', done, { once: true }),
        )
        image.src = src
        await loaded
      })
      await settled()
      const drawn = image.getBoundingClientRect().height
      expect(drawn).toBeGreaterThan(100)
      // Paged, a plate taller than the page is drawn no taller than it.
      if (paged) expect(drawn).toBeLessThanOrEqual(text.clientHeight)
      expect(shows(text, first)).toBe(true)
      if (!paged) expect(topOf(text)).toEqual(first)
      URL.revokeObjectURL(src)
    })
  }
})

/**
 * A Japanese hub and an English translation, thirty blocks each, block `i`
 * of one the translation of block `i` of the other. Block 17 of the Japanese
 * has `ねずみ` some way into it, past the page it opens.
 */
const pair = {
  title: '猫',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: Array.from({ length: 30 }, (_, i) => {
        const said = SAID.repeat(3)
        const text =
          i === 17 ? `${said.slice(0, 190)}ねずみ${said.slice(190)}` : said
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
    },
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 30 }, (_, i) => {
        const text = `Block ${i}. ${WORDS.slice(0, 400)}`
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

/** The book reader, 400 by 300, opened on `sent` at `saved`, its layout and text prefs `layout` and `typeset`. */
async function opening(
  sent: unknown,
  saved: { edition: string; slot: number },
  layout: object,
  typeset: object = {},
): Promise<HTMLElement> {
  localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(layout))
  localStorage.setItem('taiyaku.prefs.book-text', JSON.stringify(typeset))
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(sent)
      if (url === '/api/works/w/notes') {
        return Response.json(
          { work: 'w', notes: [] },
          { headers: { etag: '"n"' } },
        )
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [{ ...saved, updated: 'now' }] })
    }),
  )
  const host = mount(
    <div style={{ width: 400, height: 300 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  const at = `[data-block="${saved.slot}"]`
  for (let i = 0; i < 100 && !host.querySelector(at); i++) {
    await act(() => new Promise((done) => setTimeout(done, 10)))
  }
  await settled()
  return host
}

/** `key` pressed on the window, and what it drew settled. */
async function pressed(key: string) {
  await act(async () => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key, cancelable: true }),
    )
  })
  await settled()
}

/** A click on `el`, and what it drew settled. */
async function clicked(el: Element | null | undefined) {
  if (!el) throw new Error('nothing to click')
  await act(async () => {
    ;(el as HTMLElement).click()
  })
  await settled()
}

/** The settings' vertical switch flipped, the settings closed again. */
async function flipVertical(host: HTMLElement) {
  await pressed('s')
  const settings = host.querySelector('[aria-label="Settings"]')
  const row = [...(settings?.querySelectorAll('label') ?? [])].find(
    (label) => label.textContent === 'vertical',
  )
  await clicked(row?.querySelector('input[type="checkbox"]'))
  await pressed('Escape')
}

/** The settings' Text section reset, the settings closed again. */
async function resetText(host: HTMLElement) {
  await pressed('s')
  const text = [
    ...host.querySelectorAll('[aria-label="Settings"] .reader-section'),
  ].find((s) => s.querySelector('h3')?.textContent === 'Text')
  await clicked(text?.querySelector('.reader-section-head button'))
  await pressed('Escape')
}

/** The top bar's layout button `name` pressed, the bars hidden again. */
async function layoutTo(host: HTMLElement, name: string) {
  await pressed('Tab')
  const button = [
    ...host.querySelectorAll('[aria-label="Layout"] button'),
  ].find((b) => b.textContent === name)
  await clicked(button)
  await pressed('Escape')
}

const textOf = (host: HTMLElement) =>
  host.querySelector('.book-text') as HTMLElement

describe('the reader’s place, through a switch of how the text is set', () => {
  // Paged, each block opens a page, so the block placed is the page's top.
  const breaks = document.createElement('style')
  breaks.textContent =
    '.book-text[data-paged] > [data-block] { break-before: column }'
  beforeAll(() => {
    document.head.append(breaks)
  })
  afterAll(() => {
    breaks.remove()
    localStorage.removeItem('taiyaku.prefs.book-layout')
    localStorage.removeItem('taiyaku.prefs.book-text')
    vi.unstubAllGlobals()
  })

  for (const paged of [false, true]) {
    const how = paged ? 'paged' : 'scrolled'
    it(`holds the block at the top as vertical is turned on and off, ${how}`, async () => {
      const host = await opening(
        pair,
        { edition: 'ja', slot: 15 },
        { mode: 'scroll', paged },
      )
      expect(topBlock(textOf(host))).toBe(15)
      await flipVertical(host)
      expect(textOf(host).dataset.vertical).toBe('true')
      expect(topBlock(textOf(host))).toBe(15)
      await flipVertical(host)
      expect(textOf(host).dataset.vertical).toBeUndefined()
      expect(topBlock(textOf(host))).toBe(15)
    })

    it(`holds the block at the top through a swap to a horizontal edition and back, vertical and ${how}`, async () => {
      const host = await opening(
        pair,
        { edition: 'ja', slot: 15 },
        { mode: 'scroll', paged },
        { vertical: true },
      )
      expect(textOf(host).dataset.vertical).toBe('true')
      expect(topBlock(textOf(host))).toBe(15)
      await pressed(' ')
      expect(textOf(host).lang).toBe('en')
      expect(textOf(host).dataset.vertical).toBeUndefined()
      expect(topBlock(textOf(host))).toBe(15)
      await pressed(' ')
      expect(textOf(host).dataset.vertical).toBe('true')
      expect(topBlock(textOf(host))).toBe(15)
    })
  }

  it('holds the block at the top as the text’s settings are reset, vertical with them', async () => {
    const host = await opening(
      pair,
      { edition: 'ja', slot: 15 },
      { mode: 'scroll' },
      { vertical: true },
    )
    expect(topBlock(textOf(host))).toBe(15)
    await resetText(host)
    expect(textOf(host).dataset.vertical).toBeUndefined()
    expect(topBlock(textOf(host))).toBe(15)
  })

  /** Expect the reader's line after a switch to be `before`'s, within a line's worth of characters. */
  const near = (text: HTMLElement, before: Anchor) => {
    const after = topOf(text)
    expect(after.block).toBe(before.block)
    expect(Math.abs(after.at - before.at)).toBeLessThan(40)
  }

  it('holds the line a reader scrolled back to as vertical is turned on', async () => {
    const host = await opening(
      pair,
      { edition: 'ja', slot: 15 },
      { mode: 'scroll' },
    )
    await act(async () => {
      textOf(host).scrollBy({ top: -150 })
    })
    await settled()
    const before = topOf(textOf(host))
    // Back into block 14, its opening line still in sight below.
    expect(before.block).toBe(14)
    expect(shows(textOf(host), { block: 15, at: 0 })).toBe(true)
    await flipVertical(host)
    expect(textOf(host).dataset.vertical).toBe('true')
    near(textOf(host), before)
  })

  it('holds the line a reader scrolled back to as the text’s settings are reset', async () => {
    const host = await opening(
      pair,
      { edition: 'ja', slot: 15 },
      { mode: 'scroll' },
      { vertical: true },
    )
    await act(async () => {
      textOf(host).scrollBy({ left: 150 })
    })
    await settled()
    const before = topOf(textOf(host))
    expect(before.block).toBe(14)
    expect(shows(textOf(host), { block: 15, at: 0 })).toBe(true)
    await resetText(host)
    expect(textOf(host).dataset.vertical).toBeUndefined()
    near(textOf(host), before)
  })

  it('holds the block at the top from vertical to interlinear and back', async () => {
    const host = await opening(
      pair,
      { edition: 'ja', slot: 15 },
      { mode: 'scroll' },
      { vertical: true },
    )
    expect(topBlock(textOf(host))).toBe(15)
    await layoutTo(host, 'Each sentence over its translation')
    expect(textOf(host).querySelector('.book-interlinear')).not.toBeNull()
    expect(textOf(host).dataset.vertical).toBeUndefined()
    expect(topBlock(textOf(host))).toBe(15)
    await layoutTo(host, 'One edition')
    expect(textOf(host).dataset.vertical).toBe('true')
    expect(topBlock(textOf(host))).toBe(15)
  })

  it('turns to the page a match picked is on, vertical and paged', async () => {
    const host = await opening(
      pair,
      { edition: 'ja', slot: 2 },
      { mode: 'scroll', paged: true },
      { vertical: true },
    )
    await pressed('/')
    const search = host.querySelector('[role="dialog"][aria-label="Search"]')
    await typeInto(
      search?.querySelector('input[type="search"]') as HTMLInputElement,
      'ねずみ',
    )
    await act(() => new Promise((done) => setTimeout(done, 200)))
    await clicked(search?.querySelector('.book-contents button'))
    const found = textOf(host).querySelector('.book-found')
    expect(found?.closest('[data-block]')?.getAttribute('data-block')).toBe(
      '17',
    )
    const page = textOf(host).getBoundingClientRect()
    const lit = (found as Element).getBoundingClientRect()
    expect(lit.height).toBeGreaterThan(0)
    expect(lit.top).toBeGreaterThanOrEqual(page.top)
    expect(lit.bottom).toBeLessThanOrEqual(page.bottom)
    expect(lit.left).toBeGreaterThanOrEqual(page.left)
    expect(lit.right).toBeLessThanOrEqual(page.right)
    // Past the page the block opens, or the match's own page was not sought.
    expect(shows(textOf(host), { block: 17, at: 0 })).toBe(false)
  })
})

/** The box of the character `anchor` names. */
function charAt(text: HTMLElement, { block, at }: Anchor): DOMRect {
  const words = text.querySelector(`[data-block="${block}"] [data-text]`)
  let left = at
  for (const node of words ? wordNodes(words) : []) {
    if (left < node.length) {
      const range = document.createRange()
      range.setStart(node, left)
      range.setEnd(node, left + 1)
      return range.getBoundingClientRect()
    }
    left -= node.length
  }
  throw new Error(`block ${block} has no character ${at}`)
}

/** Twelve Japanese paragraphs of a page or so each, and an image before the third. */
const illustratedJa = {
  title: '挿絵',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: Array.from({ length: 12 }, () => {
        const text = SAID.repeat(3)
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
      figures: [{ before: 2, image: 0, alt: '地図' }],
    },
  },
  alignments: [],
}

describe('an image loading after a vertical text is drawn, scrolled', () => {
  afterAll(() => {
    localStorage.removeItem('taiyaku.prefs.book-layout')
    localStorage.removeItem('taiyaku.prefs.book-text')
    vi.unstubAllGlobals()
  })

  it('leaves the reader where they scrolled to', async () => {
    const host = await opening(
      illustratedJa,
      { edition: 'ja', slot: 5 },
      { mode: 'scroll', paged: false },
      { vertical: true },
    )
    const text = textOf(host)
    expect(text.dataset.vertical).toBe('true')
    await act(async () => {
      text.scrollBy({ left: -120 })
    })
    await settled()
    const scrolled = topOf(text)
    expect(scrolled.block).toBe(5)
    expect(scrolled.at).toBeGreaterThan(0)
    const before = charAt(text, scrolled).left
    const image = text.querySelector('.book-figure img') as HTMLImageElement
    const src = await plate()
    await act(async () => {
      const loaded = new Promise((done) =>
        image.addEventListener('load', done, { once: true }),
      )
      image.src = src
      await loaded
    })
    await settled()
    expect(image.getBoundingClientRect().width).toBeGreaterThan(50)
    expect(topOf(text).block).toBe(scrolled.block)
    // Where the reader's character is drawn, to the pixel, not a count of
    // characters: a vertical line holds few, so a count loose enough to
    // allow for a line's rounding passes the text moved on a line or more.
    expect(Math.abs(charAt(text, scrolled).left - before)).toBeLessThanOrEqual(
      1,
    )
    URL.revokeObjectURL(src)
  })
})

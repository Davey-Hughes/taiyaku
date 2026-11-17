import { act } from 'react'
import { afterAll, describe, expect, it, vi } from 'vitest'
import { BookReader } from '../src/BookReader'
import { kept, show, shows, topOf } from '../src/bookPages'
import '../src/index.css'
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

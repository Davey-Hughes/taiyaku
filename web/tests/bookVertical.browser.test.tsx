import { act, type CSSProperties, type ReactNode } from 'react'
import { afterAll, describe, expect, it, vi } from 'vitest'
import { selectionIn } from '../src/bookNotes'
import { wordNodes } from '../src/bookPages'
import { BookReader } from '../src/BookReader'
import '../src/index.css'
import { typeInto } from './support/input'
import { mount } from './support/mount'

vi.mock('../src/Account', () => ({ Account: () => null }))

/**
 * `book.css`'s vertical Japanese in Chromium, in a 400 by 300 box. Measured
 * axis facts, which `bookPages.ts` codes against:
 * - Scrolled: overflow is sideways only. `scrollLeft` is 0 at the start, the
 *   right edge, and negative leftward, to -(scrollWidth - clientWidth), within
 *   1px at either end.
 * - Paged (the same multicol): overflow is downward only; `scrollLeft` stays
 *   0, `scrollHeight` is whole `clientHeight`s, a page a `clientHeight` step.
 */

const SAID =
  '吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。'

const PAD = 32

/** `.book-text`, vertical, in a 400 by 300 reader, its text set as `BookReader` sets it. */
function vertical(
  children: ReactNode,
  {
    paged = false,
    indent = false,
    column = 544,
  }: { paged?: boolean; indent?: boolean; column?: number } = {},
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
        data-vertical
        data-paged={paged || undefined}
        data-indent={indent || undefined}
        lang="ja"
        style={
          {
            '--book-text-size': '16px',
            '--book-line-spacing': 1.6,
            '--book-column-width': `${column}px`,
          } as CSSProperties
        }
      >
        {children}
      </div>
    </div>,
  )
  return host.querySelector('.book-text') as HTMLElement
}

/** `count` paragraphs of the cat's opening, each four times over. */
const paragraphs = (count: number) =>
  Array.from({ length: count }, (_, i) => (
    <p key={i} data-block={i}>
      <span data-text>{SAID.repeat(4)}</span>
    </p>
  ))

/** The boxes of `el`'s text, a line each. */
function lines(el: Element): DOMRect[] {
  const range = document.createRange()
  range.selectNodeContents(el)
  return [...range.getClientRects()].filter((r) => r.width && r.height)
}

const meets = (a: DOMRect, b: DOMRect) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom

describe('vertical text, its axes', () => {
  it('scrolls sideways from 0 at its right edge, negative going left', () => {
    const el = vertical(paragraphs(12))
    expect(getComputedStyle(el).writingMode).toBe('vertical-rl')
    expect(el.scrollWidth).toBeGreaterThan(el.clientWidth)
    expect(el.scrollHeight).toBe(el.clientHeight)
    expect(el.scrollLeft).toBe(0)
    const first = el.querySelector('[data-block="0"]') as HTMLElement
    const box = el.getBoundingClientRect()
    expect(first.getBoundingClientRect().right).toBeCloseTo(box.right - PAD, 0)
    const before = first.getBoundingClientRect().right
    el.scrollTo({ left: -200 })
    expect(el.scrollLeft).toBe(-200)
    expect(first.getBoundingClientRect().right).toBeCloseTo(before + 200, 0)
    // The range is the width it overflows by, but Chromium can shift it a
    // pixel right of 0 where the text's fractional length rounds.
    el.scrollTo({ left: 200 })
    expect(el.scrollLeft).toBeGreaterThanOrEqual(0)
    expect(el.scrollLeft).toBeLessThanOrEqual(1)
    el.scrollTo({ left: -1e6 })
    const range = el.scrollWidth - el.clientWidth
    expect(Math.abs(el.scrollLeft + range)).toBeLessThanOrEqual(1)
  })

  it('pages downward, paged, a page its own height', () => {
    const el = vertical(paragraphs(12), { paged: true })
    expect(getComputedStyle(el).writingMode).toBe('vertical-rl')
    expect(el.scrollWidth).toBe(el.clientWidth)
    expect(el.scrollHeight).toBeGreaterThan(2 * el.clientHeight)
    expect(el.scrollHeight % el.clientHeight).toBe(0)
    expect(el.scrollTop).toBe(0)
    el.scrollTo({ left: -200 })
    expect(el.scrollLeft).toBe(0)
    el.scrollTo({ top: 1e6 })
    expect(el.scrollTop).toBe(el.scrollHeight - el.clientHeight)
    const box = el.getBoundingClientRect()
    for (const page of [1, 2]) {
      el.scrollTo({ top: page * el.clientHeight })
      expect(el.scrollTop).toBe(page * el.clientHeight)
      // The page's first line starts at its top padding, past none of it.
      const shown = [...el.querySelectorAll('[data-text]')]
        .flatMap(lines)
        .filter((r) => r.bottom > box.top && r.top < box.bottom)
      const top = Math.min(...shown.map((r) => r.top))
      expect(top).toBeCloseTo(box.top + PAD, 0)
      const bottom = Math.max(...shown.map((r) => r.bottom))
      expect(bottom).toBeLessThanOrEqual(box.bottom - PAD + 1)
    }
  })
})

describe('vertical text, set as a book sets it', () => {
  it('indents a paragraph at its top', () => {
    const el = vertical(paragraphs(2), { indent: true })
    const second = el.querySelector('[data-block="1"]') as HTMLElement
    const [line] = lines(second)
    const top = second.getBoundingClientRect().top
    expect(line.top - top).toBeCloseTo(1.5 * 16, 0)
    expect(lines(el.querySelector('[data-block="0"]') as Element)[0].top).toBe(
      el.querySelector('[data-block="0"]')?.getBoundingClientRect().top,
    )
  })

  it('caps a line at the column width, the text’s height, centred down the page', () => {
    const el = vertical(paragraphs(1), { column: 200 })
    const p = el.querySelector('p') as HTMLElement
    const box = el.getBoundingClientRect()
    expect(p.getBoundingClientRect().height).toBe(200)
    expect(p.getBoundingClientRect().top - box.top).toBeCloseTo(50, 0)
  })

  it('bounds a figure by the page, paged, its width the page’s block size', async () => {
    const el = vertical(
      <>
        <p data-block={0}>
          <span data-text>{SAID}</span>
        </p>
        <figure className="book-figure" data-figure>
          <img alt="wide" />
        </figure>
        <figure className="book-figure" data-figure>
          <img alt="tall" />
        </figure>
      </>,
      { paged: true },
    )
    const [wide, tall] = el.querySelectorAll('img')
    expect(getComputedStyle(wide).maxBlockSize).not.toBe('none')
    await Promise.all([drawn(wide, 900, 200), drawn(tall, 200, 900)])
    const page = {
      width: el.clientWidth - 2 * PAD,
      height: el.clientHeight - 2 * PAD,
    }
    for (const image of [wide, tall]) {
      const { width, height } = image.getBoundingClientRect()
      expect(width).toBeGreaterThan(50)
      expect(width).toBeLessThanOrEqual(page.width)
      expect(height).toBeLessThanOrEqual(page.height)
    }
  })

  it('sets a line’s number and a note’s mark beside the block, off its first line', () => {
    const el = vertical(
      <p data-block={0} data-line={12}>
        <span className="book-note-mark" />
        <span data-text>{SAID.repeat(2)}</span>
      </p>,
      { column: 200 },
    )
    const p = el.querySelector('p') as HTMLElement
    const [first] = lines(p.querySelector('[data-text]') as Element)
    const mark = (
      p.querySelector('.book-note-mark') as HTMLElement
    ).getBoundingClientRect()
    expect(mark.width).toBeGreaterThan(0)
    expect(meets(mark, first)).toBe(false)
    expect(mark.bottom).toBeLessThanOrEqual(p.getBoundingClientRect().top)
    const number = pseudoBox(p)
    expect(number.width).toBeGreaterThan(0)
    expect(meets(number, first)).toBe(false)
    expect(number.bottom).toBeLessThanOrEqual(p.getBoundingClientRect().top)
    // Beside the first line, not beside some later one.
    expect(number.left).toBeLessThan(first.right)
    expect(number.right).toBeGreaterThan(first.left)
  })
})

/** `el`'s `::before`, absolutely placed, as a box on the screen. */
function pseudoBox(el: HTMLElement): DOMRect {
  const style = getComputedStyle(el, '::before')
  const at = el.getBoundingClientRect()
  const px = (v: string) => Number.parseFloat(v)
  return new DOMRect(
    at.left + px(style.left),
    at.top + px(style.top),
    px(style.width),
    px(style.height),
  )
}

/** `image` set to a `width` by `height` picture and loaded. */
async function drawn(image: HTMLImageElement, width: number, height: number) {
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const loaded = new Promise((done) =>
    image.addEventListener('load', done, { once: true }),
  )
  image.src = canvas.toDataURL()
  await loaded
}

/** A Japanese edition, its one block `坊ちゃんの話` with a reading over 坊. */
const japanese = {
  title: '坊っちゃん',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: [
        {
          text: '坊ちゃんの話',
          heading: false,
          sentences: [[0, 6]],
          marks: [{ kind: 'ruby', start: 0, end: 1, reading: 'ぼっ' }],
        },
      ],
    },
  },
  alignments: [],
}

/**
 * The book reader, 400 by 300, on `sent`, its text set by `typeset`, its
 * notes `notes`. The stub is let go after every mount is torn down, since a
 * reader saves its place as it is unmounted.
 */
async function reader(
  sent: unknown,
  typeset: object = { vertical: true },
  notes: unknown[] = [],
): Promise<HTMLElement> {
  localStorage.setItem('taiyaku.prefs.book-text', JSON.stringify(typeset))
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(sent)
      // The library's answer for the work: filed in no series.
      if (url === '/api/library/works/w') {
        return Response.json({ series: null })
      }
      if (url === '/api/works/w/notes') {
        return Response.json({ work: 'w', notes }, { headers: { etag: '"n"' } })
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return new Response(null, { status: 204 })
    }),
  )
  const host = mount(
    <div style={{ width: 400, height: 300 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  for (let i = 0; i < 100 && !host.querySelector('[data-block="0"]'); i++) {
    await act(() => new Promise((done) => setTimeout(done, 10)))
  }
  return host.querySelector('.book-text') as HTMLElement
}

afterAll(() => {
  localStorage.removeItem('taiyaku.prefs.book-text')
  vi.unstubAllGlobals()
})

/** The text nodes of block `block`'s own words. */
const wordsIn = (text: HTMLElement, block = 0) =>
  wordNodes(
    text.querySelector(`[data-block="${block}"] [data-text]`) as Element,
  )

/** The box of character `at` of `node`. */
function charBox(node: Text, at: number): DOMRect {
  const range = document.createRange()
  range.setStart(node, at)
  range.setEnd(node, at + 1)
  return range.getBoundingClientRect()
}

/** A selection from `from` to `to`, each a text node and an offset into it. */
function selected(from: [Text, number], to: [Text, number]) {
  const range = document.createRange()
  range.setStart(...from)
  range.setEnd(...to)
  window.getSelection()?.removeAllRanges()
  window.getSelection()?.addRange(range)
}

describe('a selection in vertical text', () => {
  it('counts the base text alone, the reading beside it passed over', async () => {
    const text = await reader(japanese)
    expect(text.dataset.vertical).toBe('true')
    expect(getComputedStyle(text).writingMode).toBe('vertical-rl')
    const words = text.querySelector('[data-block="0"] [data-text]') as Element
    const nodes: Text[] = []
    const walker = document.createTreeWalker(words, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      nodes.push(node as Text)
    }
    expect(nodes.map((node) => node.data)).toEqual(['坊', 'ぼっ', 'ちゃんの話'])
    selected([nodes[2], 0], [nodes[2], 3])
    expect(window.getSelection()?.toString()).toBe('ちゃん')
    expect(selectionIn(text)?.range).toEqual({ start: 1, end: 4 })
  })
})

describe('furigana in vertical text', () => {
  it('is set to the right of its base, beside it', async () => {
    const text = await reader(japanese)
    expect(text.dataset.vertical).toBe('true')
    const [base] = wordsIn(text)
    expect(base.data).toBe('坊')
    const word = charBox(base, 0)
    // The base is drawn in an em square centred on its line, which the box of
    // its text, the font's whole height, overhangs; the reading is set
    // against the square, so it is measured from there, to within rounding.
    const em = Number.parseFloat(getComputedStyle(text).fontSize)
    const square = (word.left + word.right) / 2 + em / 2
    const reading = (
      text.querySelector('[data-block="0"] rt') as HTMLElement
    ).getBoundingClientRect()
    expect(reading.width).toBeGreaterThan(0)
    expect(reading.left).toBeGreaterThanOrEqual(square - 1)
    expect(reading.top).toBeLessThan(word.bottom)
    expect(reading.bottom).toBeGreaterThan(word.top)
  })
})

/** A Japanese edition, its one block holding numbers of two, four and one digits. */
const numbered = {
  title: '日付',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: [
        {
          text: '昭和12年と1234年に7人が来た',
          heading: false,
          sentences: [[0, 17]],
        },
      ],
    },
  },
  alignments: [],
}

/**
 * A Japanese edition whose first block reads はたち over 20歳, whose second
 * links 第12話 to it, and whose third sets 第12話 in bold.
 */
const based = {
  title: '年齢',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: [
        {
          text: '彼は20歳になった',
          heading: false,
          sentences: [[0, 9]],
          marks: [{ kind: 'ruby', start: 2, end: 5, reading: 'はたち' }],
        },
        {
          text: '第12話を読む',
          heading: false,
          sentences: [[0, 7]],
          marks: [{ kind: 'link', start: 0, end: 4, block: 0, note: false }],
        },
        {
          text: '第12話は短い',
          heading: false,
          sentences: [[0, 7]],
          marks: [{ kind: 'bold', start: 0, end: 4 }],
        },
      ],
    },
  },
  alignments: [],
}

/** Whether the two digits of `node` are set side by side across the line. */
function across(node: Text | undefined): boolean {
  if (node?.length !== 2) return false
  const [one, two] = [charBox(node, 0), charBox(node, 1)]
  return Math.abs(two.top - one.top) < 0.5 && two.left > one.left
}

/** The text node of block `block`'s own words that is `data` alone. */
const nodeOf = (text: HTMLElement, block: number, data: string) =>
  wordsIn(text, block).find((node) => node.data === data)

/** The box of characters `from` to `to` of block `block`'s own words. */
function boxOf(
  text: HTMLElement,
  block: number,
  from: number,
  to: number,
): DOMRect {
  const range = document.createRange()
  let at = 0
  for (const node of wordsIn(text, block)) {
    if (from >= at && from < at + node.length) range.setStart(node, from - at)
    if (to > at && to <= at + node.length) range.setEnd(node, to - at)
    at += node.length
  }
  return range.getBoundingClientRect()
}

describe('a number in vertical text', () => {
  it('of one or two digits is set upright across its line, a longer one along it', async () => {
    const text = await reader(numbered)
    expect(text.dataset.vertical).toBe('true')
    const em = Number.parseFloat(getComputedStyle(text).fontSize)
    const nodes = wordsIn(text)
    const two = nodes.find((node) => node.data === '12') as Text
    expect(two).toBeDefined()
    const [one, after] = [charBox(two, 0), charBox(two, 1)]
    // Side by side across the line, as wide together as one character is long.
    expect(Math.abs(after.top - one.top)).toBeLessThan(0.5)
    expect(after.left).toBeGreaterThan(one.left)
    expect(two.parentElement?.getBoundingClientRect().height).toBeCloseTo(em, 0)
    const single = nodes.find((node) => node.data === '7') as Text
    expect(single).toBeDefined()
    expect(single.parentElement?.getBoundingClientRect().height).toBeCloseTo(
      em,
      0,
    )
    const four = nodes.find((node) => node.data.includes('1234')) as Text
    const at = four.data.indexOf('1234')
    const tops = [0, 1, 2, 3].map((i) => charBox(four, at + i).top)
    for (let i = 1; i < 4; i++) expect(tops[i]).toBeGreaterThan(tops[i - 1])
  })

  it('keeps its block’s text, a note’s run, a match and a selection’s count as they were', async () => {
    const text = await reader(numbered, { vertical: true }, [
      {
        id: '01JAAAAAAAAAAAAAAAAAAAAAAA',
        edition: 'ja',
        slot: 0,
        text: { start: 2, end: 5 },
        color: 'yellow',
        body: '',
        created: '2026-01-01T00:00:00.000Z',
        updated: '2026-01-01T00:00:00.000Z',
      },
    ])
    expect(text.dataset.vertical).toBe('true')
    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: '/', cancelable: true }),
      )
    })
    const search = document.querySelector(
      '[role="dialog"][aria-label="Search"] input[type="search"]',
    ) as HTMLInputElement
    await typeInto(search, '2年')
    await act(() => new Promise((done) => setTimeout(done, 200)))
    await act(async () => {
      ;(
        document.querySelector(
          '[role="dialog"][aria-label="Search"] .book-contents button',
        ) as HTMLElement
      ).click()
    })
    const block = text.querySelector('[data-block="0"]') as HTMLElement
    const nodes = wordsIn(text)
    expect(nodes.map((node) => node.data).join('')).toBe(
      numbered.editions.ja.blocks[0].text,
    )
    expect(block.querySelectorAll('.book-upright').length).toBeGreaterThan(0)
    const lit = [...block.querySelectorAll('.book-found')]
    expect(lit.map((mark) => mark.textContent).join('')).toBe('2年')
    const noted = [...block.querySelectorAll('mark[data-note]')]
    expect(noted.map((mark) => mark.textContent).join('')).toBe('12年')
    const seven = nodes.find((node) => node.data === '7') as Text
    const before = nodes[nodes.indexOf(seven) - 1]
    selected([before, before.data.indexOf('1234')], [seven, 1])
    expect(window.getSelection()?.toString()).toBe('1234年に7')
    expect(selectionIn(text)?.range).toEqual({ start: 6, end: 13 })
  })

  it('inside a ruby’s base or a link is set upright, the reading centred on the whole base, the link one', async () => {
    const text = await reader(based)
    expect(text.dataset.vertical).toBe('true')
    const rubies = [...text.querySelectorAll('[data-block="0"] ruby')]
    expect(rubies).toHaveLength(1)
    const reading = rubies[0].querySelector('rt') as HTMLElement
    expect(rubies[0].textContent).toBe(`20歳${reading.textContent}`)
    expect(across(nodeOf(text, 0, '20')), 'the 20 of 20歳').toBe(true)
    const word = boxOf(text, 0, 2, 5)
    const over = reading.getBoundingClientRect()
    expect(
      Math.abs((over.top + over.bottom) / 2 - (word.top + word.bottom) / 2),
    ).toBeLessThanOrEqual(1)
    const links = [...text.querySelectorAll('[data-block="1"] a[data-link]')]
    expect(links.map((link) => link.textContent)).toEqual(['第12話'])
    expect(across(nodeOf(text, 1, '12')), 'the 12 of the link').toBe(true)
  })

  it('inside a ruby’s base is set upright with furigana off, as any other is', async () => {
    const text = await reader(based, { vertical: true, furigana: false })
    expect(text.dataset.vertical).toBe('true')
    expect(text.querySelector('[data-block="0"] rt')).toBeNull()
    expect(across(nodeOf(text, 0, '20')), 'the 20 of 20歳').toBe(true)
  })

  it('under a bold mark is set upright, as any other is', async () => {
    const text = await reader(based)
    expect(text.dataset.vertical).toBe('true')
    const bold = [...text.querySelectorAll('[data-block="2"] b')]
    expect(bold.map((b) => b.textContent)).toEqual(['第12話'])
    expect(across(nodeOf(text, 2, '12')), 'the 12 in bold').toBe(true)
  })

  it('is left in its run in horizontal text', async () => {
    const text = await reader(numbered, {})
    expect(text.dataset.vertical).toBeUndefined()
    expect(text.querySelector('.book-upright')).toBeNull()
    expect(wordsIn(text).map((node) => node.data)).toEqual([
      numbered.editions.ja.blocks[0].text,
    ])
  })
})

/** A Japanese edition of twelve paragraphs, the cat's opening four times over in each. */
const long = {
  title: '猫',
  hub: 'ja',
  editions: {
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: Array.from({ length: 12 }, () => {
        const text = SAID.repeat(4)
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
    },
  },
  alignments: [],
}

/** `long`, with a number starting a sentence in the middle of block 8. */
const numberedFar = {
  ...long,
  editions: {
    ja: {
      ...long.editions.ja,
      blocks: long.editions.ja.blocks.map((block, i) => {
        if (i !== 8) return block
        const text = `${SAID}27年${SAID}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
    },
  },
}

describe('a match starting on an upright number, vertical and scrolled', () => {
  it('lands with the number’s whole column on the screen', async () => {
    const text = await reader(numberedFar)
    expect(text.dataset.vertical).toBe('true')
    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: '/', cancelable: true }),
      )
    })
    const search = document.querySelector(
      '[role="dialog"][aria-label="Search"] input[type="search"]',
    ) as HTMLInputElement
    await typeInto(search, '27')
    await act(() => new Promise((done) => setTimeout(done, 200)))
    await act(async () => {
      ;(
        document.querySelector(
          '[role="dialog"][aria-label="Search"] .book-contents button',
        ) as HTMLElement
      ).click()
    })
    await act(() => new Promise((done) => requestAnimationFrame(done)))
    const upright = text.querySelector(
      '[data-block="8"] .book-upright',
    ) as HTMLElement
    expect(upright.textContent).toBe('27')
    const column = upright.getBoundingClientRect()
    const screen = text.getBoundingClientRect()
    expect(column.right).toBeLessThanOrEqual(screen.right + 1)
    expect(column.left).toBeGreaterThanOrEqual(screen.left)
  })
})

describe('a wheel over vertical scrolled text', () => {
  it('turned in lines, as Firefox sends it, scrolls on leftward a line of text a line', async () => {
    const text = await reader(long)
    expect(text.dataset.vertical).toBe('true')
    const columns = document.createRange()
    columns.selectNodeContents(wordsIn(text)[0])
    const [first, second] = [...columns.getClientRects()]
    const line = first.left - second.left
    expect(line).toBeGreaterThan(10)
    const before = text.scrollLeft
    const event = new WheelEvent('wheel', {
      deltaY: 3,
      deltaMode: WheelEvent.DOM_DELTA_LINE,
      cancelable: true,
      bubbles: true,
    })
    await act(async () => {
      text.dispatchEvent(event)
    })
    expect(event.defaultPrevented).toBe(true)
    expect(Math.abs(before - text.scrollLeft - 3 * line)).toBeLessThanOrEqual(1)
  })
})

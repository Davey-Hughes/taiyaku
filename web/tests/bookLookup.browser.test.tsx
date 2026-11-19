import { act } from 'react'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { offsetAtPoint, selectionIn } from '../src/bookNotes'
import { bringIntoSight, topOf } from '../src/bookPages'
import { BookReader } from '../src/BookReader'
import { GateContext } from '../src/gate'
import '../src/index.css'
import type { Note } from '../src/notes'
import { typeInto } from './support/input'
import { mount } from './support/mount'
import { mouse } from './support/mouse'
import { tapAt, touchDown, touchUp } from './support/touch'

vi.mock('../src/Account', () => ({ Account: () => null }))

/**
 * What a finger's tap does with the look-up mode on, against Chromium's own
 * layout and caret, which jsdom has neither of: the character it looks a
 * word up from, and whether it is on a character at all.
 */

/** Long enough to run over several lines at the width the tests draw at. */
const OPENING =
  '親譲りの無鉄砲で小供の時から損ばかりしている。小学校に居る時分学校の二階から飛び降りて一週間ほど腰を抜かした事がある。'
/** Its 猫 glossed, and 名前 highlighted (`highlight`). */
const CAT = '吾輩は猫である。名前はまだ無い。'
/** Opening on a kanji outside the BMP, a surrogate pair. */
const PAIR = '\u{20BB7}野家の牛丼を食べた。'
/** Opening on a kanji with an ideographic variation selector after it. */
const VARIANT = '葛\u{E0100}飾北斎の絵を見た。'
/** Ten characters, then a full stop, which no line may begin with. */
const COUNTED = '一二三四五六七八九十。あいう'

const block = (
  text: string,
  sentences: [number, number][] = [[0, text.length]],
  marks: object[] = [],
) => ({ text, heading: false, sentences, marks })

/** One Japanese edition, its blocks the texts above in that order. */
const book = {
  title: '坊っちゃん',
  hub: 'ja',
  editions: {
    ja: {
      label: '日本語',
      language: 'ja',
      blocks: [
        block(OPENING),
        block(
          CAT,
          [
            [0, 8],
            [8, 16],
          ],
          [{ kind: 'ruby', start: 3, end: 4, reading: 'ねこ' }],
        ),
        block(PAIR),
        block(VARIANT),
        block(COUNTED),
      ],
    },
  },
  alignments: [],
}

/** Two short sentences, each with its English under it when interlinear. */
const translated = {
  title: '数',
  hub: 'ja',
  editions: {
    ja: {
      label: '日本語',
      language: 'ja',
      blocks: [
        block('一二三。四五六。', [
          [0, 4],
          [4, 8],
        ]),
      ],
    },
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        block('One two three. Four five six.', [
          [0, 14],
          [15, 29],
        ]),
      ],
    },
  },
  alignments: [
    {
      to: 'en',
      pins: [],
      links: [[0, 1, 0, 1]],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 2],
      ],
    },
  ],
}

/** A highlight over CAT's 名前, so the far half of 前 ends one. */
const highlight: Note = {
  id: 'h',
  edition: 'ja',
  slot: 1,
  body: '',
  color: 'amber',
  created: '2026-11-18T10:00:00.000Z',
  updated: '2026-11-18T10:00:00.000Z',
  text: { start: 8, end: 10 },
}

const wait = (ms: number) =>
  act(() => new Promise<void>((done) => setTimeout(done, ms)))

/**
 * The book reader over the whole screen on `drawn`, with `notes`, the look-up
 * mode on unless `lookUp` is false, and `prefs` stored first under their
 * modules' ids. Its dictionary answers a longest-word search with `found`,
 * by default nothing at no length, so the look-up's title stays the character
 * tapped, and a word with `words`, by default no entries, or what `words` says
 * for its request's path, each with `status`; `asked` holds each request's
 * path, decoded. With `admin`, the reader is an administrator's, who can pin.
 */
async function openReader(
  drawn: object = book,
  {
    lookUp = true,
    prefs = {},
    notes = [],
    found = { length: 0, entries: [] },
    words = [],
    status = 200,
    admin = false,
  }: {
    admin?: boolean
    lookUp?: boolean
    prefs?: Record<string, object>
    notes?: Note[]
    found?: { length: number; entries: object[] }
    words?: object[] | ((url: string) => object[] | Promise<object[]>)
    status?: number
  } = {},
): Promise<{ host: HTMLElement; asked: string[] }> {
  const stored: Record<string, object> = { lookup: { on: lookUp }, ...prefs }
  for (const [id, value] of Object.entries(stored)) {
    localStorage.setItem(`taiyaku.prefs.${id}`, JSON.stringify(value))
  }
  onTestFinished(() => {
    for (const id of Object.keys(stored)) {
      localStorage.removeItem(`taiyaku.prefs.${id}`)
    }
  })
  const asked: string[] = []
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(drawn)
      if (url === '/api/works/w/notes') {
        return Response.json({ work: 'w', notes }, { headers: { etag: '"n"' } })
      }
      if (url.startsWith('/api/dictionary/')) {
        asked.push(decodeURIComponent(url))
        const entries =
          typeof words === 'function'
            ? await words(decodeURIComponent(url))
            : words
        return Response.json(url.includes('/at/') ? found : entries, { status })
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [] })
    }),
  )
  onTestFinished(() => {
    vi.unstubAllGlobals()
  })
  const reader = (
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>
  )
  const who = {
    who: { user_id: 1, admin: true },
    signOut: () => {},
  } as unknown as React.ComponentProps<typeof GateContext>['value']
  const host = mount(
    admin ? <GateContext value={who}>{reader}</GateContext> : reader,
  )
  for (
    let i = 0;
    i < 100 && !host.querySelector('[data-block] [data-text]');
    i++
  ) {
    await wait(10)
  }
  await wait(100)
  return { host, asked }
}

/** Block 0 set in columns, each 12 characters tall. */
const VERTICAL_BLOCK =
  '.book-text [data-block="0"] { writing-mode: vertical-rl; height: 12em; }'

/** `css` on the page for this test alone, before the reader is opened. */
function styled(css: string) {
  const sheet = document.createElement('style')
  sheet.textContent = css
  document.head.append(sheet)
  onTestFinished(() => sheet.remove())
}

/**
 * A real tap at `x`, `y`, then long enough for a look-up to be asked or a
 * plain tap's wait to show the bars, and for the next tap not to be the
 * second of a double tap.
 */
async function tapThenWait(x: number, y: number) {
  await tapAt(x, y)
  await wait(350)
}

/** Escape, which closes an open look-up, so nothing is over the next tap. */
async function closeLookUp() {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
  })
  await wait(20)
}

const barsShown = (host: HTMLElement) =>
  host.querySelector('.reader-top-bar') !== null

const lookUpTitle = () =>
  document.querySelector(
    '[role="dialog"][aria-label="Look up"] .reader-panel-title',
  )?.textContent

/** The request a tap on character `at` of `text` sends: 16 code points from it. */
const lookedUpFrom = (text: string, at: number) =>
  `/api/dictionary/ja/at/${[...text.slice(at)].slice(0, 16).join('')}`

/**
 * The box of block `n`'s own words from `from` up to `to`, which lie in one
 * text node, past the asides set among them.
 */
function characterBox(
  host: HTMLElement,
  n: number,
  from: number,
  to = from + 1,
): DOMRect {
  const own = host.querySelector(`[data-block="${n}"] > [data-text]`) as Node
  const walker = document.createTreeWalker(own, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node.parentElement?.closest('[data-aside]')
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT,
  })
  let left = from
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const length = (node as Text).length
    if (left < length) {
      const range = document.createRange()
      range.setStart(node, left)
      range.setEnd(node, left + to - from)
      return range.getClientRects()[0]
    }
    left -= length
  }
  throw new Error(`block ${n} has no character at ${from}`)
}

/**
 * The last character of block `n`'s first line: the last whose box starts
 * where the first's does across the lines, in x for vertical text.
 */
function lastOnFirstLine(host: HTMLElement, n: number, vertical: boolean) {
  const lineOf = (box: DOMRect) => (vertical ? box.right : box.top)
  const first = lineOf(characterBox(host, n, 0))
  let at = 0
  while (lineOf(characterBox(host, n, at + 1)) === first) at += 1
  return at
}

/** The point a fraction `f` along `box`, level with its middle. */
const across = (box: DOMRect, f: number): [number, number] => [
  box.left + box.width * f,
  box.top + box.height / 2,
]

/**
 * Six points on the character in `box`, by name: its near and far half along
 * the line, each in the middle of the glyph and 2 px out from it on either
 * side across the line, which is still in the line's leading. Vertical text
 * runs its lines down, so there the line runs along y.
 */
function quarterPoints(
  box: DOMRect,
  vertical: boolean,
): [name: string, x: number, y: number][] {
  const sides: [string, number][] = vertical
    ? [
        ['right of the glyph', box.right + 2],
        ['mid-glyph', box.left + box.width / 2],
        ['left of the glyph', box.left - 2],
      ]
    : [
        ['above the glyph', box.top - 2],
        ['mid-glyph', box.top + box.height / 2],
        ['below the glyph', box.bottom + 2],
      ]
  const out: [string, number, number][] = []
  for (const [half, f] of [
    ['near', 0.25],
    ['far', 0.75],
  ] as const) {
    const along = vertical ? box.top + box.height * f : box.left + box.width * f
    for (const [where, level] of sides) {
      out.push(
        vertical
          ? [`${half} half, ${where}`, level, along]
          : [`${half} half, ${where}`, along, level],
      )
    }
  }
  return out
}

/**
 * What a tap on each quarter point of block 0's characters 0, 4 and the last
 * of its first line asks, and what it should: a look-up from that character.
 */
async function gridOfTaps(
  host: HTMLElement,
  asked: string[],
  vertical: boolean,
) {
  const got: Record<string, string[]> = {}
  const want: Record<string, string[]> = {}
  for (const at of [0, 4, lastOnFirstLine(host, 0, vertical)]) {
    for (let point = 0; point < 6; point++) {
      const [name, x, y] = quarterPoints(characterBox(host, 0, at), vertical)[
        point
      ]
      const key = `${OPENING[at]} (${at}), ${name}`
      const before = asked.length
      await tapThenWait(x, y)
      got[key] = asked.slice(before)
      want[key] = [lookedUpFrom(OPENING, at)]
      await closeLookUp()
    }
  }
  return { got, want }
}

/**
 * A character of block 0's first line and the one level with it on the second,
 * with the middle of each glyph across the lines and where along the line
 * they are level. Paragraphs must not be indented, so the two lines align.
 */
function twoLines(host: HTMLElement, vertical: boolean) {
  const span = (box: DOMRect) =>
    vertical ? [box.top, box.bottom] : [box.left, box.right]
  const middle = (box: DOMRect) =>
    vertical ? box.left + box.width / 2 : box.top + box.height / 2
  const upper = characterBox(host, 0, 4)
  const along = (span(upper)[0] + span(upper)[1]) / 2
  let lower = lastOnFirstLine(host, 0, vertical) + 1
  while (span(characterBox(host, 0, lower))[1] < along) lower += 1
  return {
    characters: [4, lower],
    along,
    from: middle(upper),
    to: middle(characterBox(host, 0, lower)),
  }
}

/** How many points an eighth of a pixel apart, from one line's glyph to the next's, find neither character. */
function pointsOffBothLines(host: HTMLElement, vertical: boolean) {
  const text = host.querySelector('.book-text') as HTMLElement
  const { characters, along, from, to } = twoLines(host, vertical)
  const steps = Math.ceil(Math.abs(to - from) * 8)
  let off = 0
  for (let step = 0; step <= steps; step++) {
    const level = from + ((to - from) * step) / steps
    const found = vertical
      ? offsetAtPoint(text, level, along)
      : offsetAtPoint(text, along, level)
    if (found?.block !== 0 || !characters.includes(found.at)) off += 1
  }
  return off
}

/** The text sizes and line spacings the pitch between two lines is stepped across at. */
const TYPESETTINGS = [14, 18, 22, 26, 32].flatMap((textSize) =>
  [1.4, 1.6, 1.8, 2].map((lineSpacing) => ({ textSize, lineSpacing })),
)

/** Each typesetting of `drawn` with a point between two lines on neither, and how many. */
async function typesettingsWithPointsOff(drawn: object, vertical: boolean) {
  const faults: Record<string, number> = {}
  for (const type of TYPESETTINGS) {
    const { host } = await openReader(drawn, {
      prefs: { 'book-text': { ...type, indent: false } },
    })
    const off = pointsOffBothLines(host, vertical)
    if (off > 0) faults[`${type.textSize}px at ${type.lineSpacing}`] = off
    host.remove()
  }
  return faults
}

/** What real taps `step` px apart, up to `reach` either side of the middle of two lines, ask that is not one of their characters. */
async function tapsOffBothLines(
  vertical: boolean,
  reach: number,
  step: number,
) {
  const { host, asked } = await openReader(book, {
    prefs: { 'book-text': { indent: false } },
  })
  const { characters, along, from, to } = twoLines(host, vertical)
  const wanted = characters.map((at) => lookedUpFrom(OPENING, at))
  const wrong: Record<string, string[]> = {}
  for (let off = -reach; off <= reach; off += step) {
    const level = (from + to) / 2 + off
    const before = asked.length
    await (vertical ? tapThenWait(level, along) : tapThenWait(along, level))
    const sent = asked.slice(before)
    if (sent.length !== 1 || !wanted.includes(sent[0])) wrong[off] = sent
    await closeLookUp()
  }
  return wrong
}

/** OPENING with every third character glossed, so its lines stand taller than their spacing. */
const glossed = {
  ...book,
  editions: {
    ja: {
      ...book.editions.ja,
      blocks: [
        block(
          OPENING,
          [[0, OPENING.length]],
          Array.from({ length: 19 }, (_, i) => ({
            kind: 'ruby',
            start: i * 3,
            end: i * 3 + 1,
            reading: 'かな',
          })),
        ),
      ],
    },
  },
}

describe('a point between two lines', () => {
  it('is on a character at every step of the pitch, at every typesetting', async () => {
    expect(await typesettingsWithPointsOff(book, false)).toEqual({})
  }, 60_000)

  it('is on a character at every step of the pitch in vertical text, at every typesetting', async () => {
    styled(VERTICAL_BLOCK)
    expect(await typesettingsWithPointsOff(book, true)).toEqual({})
  }, 60_000)

  it('is on a character at every step of the pitch between lines with readings on them', async () => {
    expect(await typesettingsWithPointsOff(glossed, false)).toEqual({})
  }, 60_000)

  it('is on a character at every step of the pitch between columns with readings on them', async () => {
    styled(VERTICAL_BLOCK)
    expect(await typesettingsWithPointsOff(glossed, true)).toEqual({})
  }, 60_000)

  it('looks up from a line’s character for a real tap either side of the middle of two lines', async () => {
    expect(await tapsOffBothLines(false, 0.5, 0.25)).toEqual({})
  }, 30_000)

  it('looks up from a column’s character for a real tap either side of the middle of two columns', async () => {
    styled(VERTICAL_BLOCK)
    expect(await tapsOffBothLines(true, 1.5, 0.5)).toEqual({})
  }, 30_000)
})

describe('a tap with the look-up mode on', () => {
  it('looks up from the character tapped, on either half, in its glyph or in the leading above and below it', async () => {
    const { host, asked } = await openReader()
    const { got, want } = await gridOfTaps(host, asked, false)
    expect(got).toEqual(want)
  })

  it('looks up from the character tapped in vertical text, on either half, in its glyph or in the leading either side of it', async () => {
    styled(VERTICAL_BLOCK)
    const { host, asked } = await openReader()
    const own = host.querySelector('[data-block="0"] [data-text]') as Element
    expect(getComputedStyle(own).writingMode).toBe('vertical-rl')
    const { got, want } = await gridOfTaps(host, asked, true)
    expect(got).toEqual(want)
  })

  it('looks up from a character at the edge of a glyph taller than its line', async () => {
    styled('.book-text [data-block="0"] { line-height: 0.5; }')
    const { host, asked } = await openReader()
    const box = characterBox(host, 0, 0)
    await tapThenWait(box.left + box.width * 0.75, box.top + 1)
    expect(asked).toEqual([lookedUpFrom(OPENING, 0)])
  })

  it('looks up from a ruby’s base, a highlight’s last character, a reading, a surrogate pair and a kanji with its variation selector', async () => {
    const { host, asked } = await openReader(book, { notes: [highlight] })
    expect(host.querySelector('[data-block="1"] mark')?.textContent).toBe(
      '名前',
    )
    const cat = () => characterBox(host, 1, 3)
    const reading = () =>
      (
        host.querySelector('[data-block="1"] rt') as Element
      ).getBoundingClientRect()
    /** Each tap: its name, its point, and the text and character it is on. */
    const taps: [string, () => [number, number], string, number][] = [
      ['far half of 猫, a ruby’s base', () => across(cat(), 0.75), CAT, 3],
      [
        'far half of 猫, just above its glyph',
        () => [across(cat(), 0.75)[0], cat().top - 2],
        CAT,
        3,
      ],
      [
        'far half of 猫, just below its glyph',
        () => [across(cat(), 0.75)[0], cat().bottom + 2],
        CAT,
        3,
      ],
      [
        'near half of で, after the ruby',
        () => across(characterBox(host, 1, 4), 0.25),
        CAT,
        4,
      ],
      [
        'far half of 前, a highlight’s end',
        () => across(characterBox(host, 1, 9), 0.75),
        CAT,
        9,
      ],
      [
        'near half of は, after the highlight',
        () => across(characterBox(host, 1, 10), 0.25),
        CAT,
        10,
      ],
      ['middle of ねこ, the reading', () => across(reading(), 0.5), CAT, 3],
      [
        'near half of 𠮷',
        () => across(characterBox(host, 2, 0, 2), 0.25),
        PAIR,
        0,
      ],
      [
        'far half of 𠮷',
        () => across(characterBox(host, 2, 0, 2), 0.75),
        PAIR,
        0,
      ],
      [
        'near half of 葛 and its selector',
        () => across(characterBox(host, 3, 0, 3), 0.25),
        VARIANT,
        0,
      ],
      [
        'far half of 葛 and its selector',
        () => across(characterBox(host, 3, 0, 3), 0.75),
        VARIANT,
        0,
      ],
    ]
    const got: Record<string, { asked: string[]; title?: string | null }> = {}
    const want: typeof got = {}
    for (const [name, point, text, at] of taps) {
      const before = asked.length
      await tapThenWait(...point())
      got[name] = { asked: asked.slice(before), title: lookUpTitle() }
      want[name] = {
        asked: [lookedUpFrom(text, at)],
        title: String.fromCodePoint(text.codePointAt(at) ?? 0),
      }
      await closeLookUp()
    }
    expect(got).toEqual(want)
  })
})

/** One Korean paragraph, its edition's tag naming a region. */
const KOREAN_TEXT = '그는 10년 동안 2000 번 살았다.'
const korean = {
  title: '해리',
  hub: 'ko',
  editions: {
    ko: {
      label: '한국어',
      language: 'ko-KR',
      blocks: [block(KOREAN_TEXT)],
    },
  },
  alignments: [],
}

describe('a tap in a Korean book', () => {
  it('looks up the digits joined to a word and lights them with it, from a digit or a letter', async () => {
    const { host, asked } = await openReader(korean)
    for (const at of [3, 4, 5]) {
      const before = asked.length
      await tapThenWait(...across(characterBox(host, 0, at), 0.5))
      expect(asked.slice(before)).toEqual(['/api/dictionary/ko-KR/10년'])
      expect(lookUpTitle()).toBe('10년')
      expect(window.getSelection()?.toString()).toBe('10년')
      await closeLookUp()
    }
  })

  it('keeps a bare number’s tap as a tap', async () => {
    const { host, asked } = await openReader(korean)
    await tapThenWait(
      ...across(characterBox(host, 0, KOREAN_TEXT.indexOf('2000') + 1), 0.5),
    )
    expect(asked).toEqual([])
    expect(barsShown(host)).toBe(true)
  })
})

describe('a tap on no character', () => {
  /**
   * What a tap at `where` in a reader on `drawn` with `prefs` does, the
   * look-up mode off and then on: whether it shows the bars, and what it asks
   * the dictionary.
   */
  async function offAndOn(
    drawn: object,
    prefs: Record<string, object>,
    where: (host: HTMLElement) => [number, number],
  ) {
    const out: Record<string, { bars: boolean; asked: string[] }> = {}
    for (const lookUp of [false, true]) {
      const { host, asked } = await openReader(drawn, { lookUp, prefs })
      await tapThenWait(...where(host))
      out[lookUp ? 'on' : 'off'] = { bars: barsShown(host), asked }
      host.remove()
    }
    return out
  }

  const leftAlone = {
    off: { bars: true, asked: [] },
    on: { bars: true, asked: [] },
  }

  it('in the padding beside a full justified line keeps its meaning', async () => {
    const out = await offAndOn(book, {}, (host) => {
      const line = host.querySelector('[data-block="0"]') as Element
      const text = host.querySelector('.book-text') as Element
      const right = line.getBoundingClientRect().right
      return [
        (right + text.getBoundingClientRect().right) / 2,
        across(characterBox(host, 0, 0), 0.5)[1],
      ]
    })
    expect(out).toEqual(leftAlone)
  })

  it('beside a short last line, above the translation set under it, keeps its meaning', async () => {
    const out = await offAndOn(
      translated,
      { 'book-layout': { mode: 'interlinear' } },
      (host) => {
        const stop = characterBox(host, 0, 3)
        return [stop.right + 40, stop.top + stop.height / 2]
      },
    )
    expect(out).toEqual(leftAlone)
  })

  it('between two paragraphs keeps its meaning', async () => {
    const out = await offAndOn(
      book,
      { 'book-text': { indent: false } },
      (host) => {
        const above = (
          host.querySelector('[data-block="0"]') as Element
        ).getBoundingClientRect()
        const below = (
          host.querySelector('[data-block="1"]') as Element
        ).getBoundingClientRect()
        expect(below.top - above.bottom).toBeGreaterThan(10)
        return [
          across(characterBox(host, 1, 0), 0.5)[0],
          (above.bottom + below.top) / 2,
        ]
      },
    )
    expect(out).toEqual(leftAlone)
  })

  it('finds no character at any step across the gap between two paragraphs', async () => {
    const { host } = await openReader(book, {
      prefs: { 'book-text': { indent: false } },
    })
    const text = host.querySelector('.book-text') as HTMLElement
    const above = (
      host.querySelector('[data-block="0"]') as Element
    ).getBoundingClientRect()
    const below = (
      host.querySelector('[data-block="1"]') as Element
    ).getBoundingClientRect()
    const x = across(characterBox(host, 1, 0), 0.5)[0]
    const found: number[] = []
    for (let y = above.bottom + 0.25; y < below.top; y += 0.25) {
      if (offsetAtPoint(text, x, y)) found.push(y)
    }
    expect(below.top - above.bottom).toBeGreaterThan(10)
    expect(found).toEqual([])
  })

  it('in the margin under a translation keeps its meaning', async () => {
    styled('.book-interlinear { margin-bottom: 3em; }')
    const out = await offAndOn(
      translated,
      { 'book-layout': { mode: 'interlinear' } },
      (host) => {
        const four = characterBox(host, 0, 4)
        return [across(four, 0.5)[0], four.top - 15]
      },
    )
    expect(out).toEqual(leftAlone)
  })

  it('beyond the last column of vertical text keeps its meaning', async () => {
    // Set vertical, a paragraph's auto margins are its top and bottom and its
    // block end is its left side, which the indent rule zeroes, so it lies
    // against the screen's edge. This keeps the point past its last column on
    // the screen.
    styled(
      `${VERTICAL_BLOCK} .book-text[data-indent] [data-block="0"] { margin-left: 8em; }`,
    )
    // Ending on a word, so a tap taken for the last character would ask for it.
    const text = OPENING.slice(0, -1)
    const unstopped = {
      ...book,
      editions: { ja: { ...book.editions.ja, blocks: [block(text)] } },
    }
    const out = await offAndOn(unstopped, {}, (host) => {
      const last = characterBox(host, 0, text.length - 1)
      const x = last.left - last.width * 2
      expect(x).toBeGreaterThan(0)
      return [x, last.top + last.height / 2]
    })
    expect(out).toEqual(leftAlone)
  })

  it('in the gap a full stop leaves at the end of an unjustified line keeps its meaning', async () => {
    styled('.book-text [data-block="4"] { width: 10.5em; }')
    const out = await offAndOn(
      book,
      { 'book-text': { justify: false, indent: false } },
      (host) => {
        const nine = characterBox(host, 4, 8)
        // 十 went down with the full stop, leaving room on the line for it.
        expect(characterBox(host, 4, 9).top).toBeGreaterThan(nine.bottom)
        return [nine.right + 8, nine.top + nine.height / 2]
      },
    )
    expect(out).toEqual(leftAlone)
  })
})

/** Nothing but one kanji, so every point on a line is on a word, over several pages. */
const KANJI = block('猫'.repeat(3000))
const kanjiBook = {
  ...book,
  editions: { ja: { ...book.editions.ja, blocks: [KANJI] } },
}

/** One English paragraph, for a word found as soon as it is tapped. */
const english = {
  title: 'Sentences',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [block('Who asked. Nobody knew.')],
    },
  },
  alignments: [],
}

/** The window `width` by `height` for this test alone. */
async function windowOf(width: number, height: number) {
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, height)
}

const closeButton = () =>
  [
    ...document.querySelectorAll(
      '[role="dialog"][aria-label="Look up"] button',
    ),
  ].find((b) => b.textContent === 'Close') as HTMLElement

/** The first point inside `box` that is on a character of `text`. */
function characterIn(text: HTMLElement, box: DOMRect): [number, number] {
  for (let y = box.top + 2; y < box.bottom; y += 2) {
    for (let x = box.left + 2; x < box.right; x += 2) {
      if (offsetAtPoint(text, x, y)) return [x, y]
    }
  }
  throw new Error('no character lies under this box')
}

describe('the click a tap leaves behind', () => {
  it('does not press the Close the look-up draws under the finger beside the set-in text', async () => {
    await windowOf(...SET_IN)
    // Set vertically, whose columns run on under where the panel docks.
    const { host, asked } = await openReader(kanjiBook, {
      prefs: { 'book-text': { vertical: true } },
      found: { length: 2, entries: [] },
    })
    await tapThenWait(...across(characterBox(host, 0, 0), 0.5))
    const close = closeButton().getBoundingClientRect()
    await closeLookUp()
    const text = host.querySelector('.book-text') as HTMLElement
    const finger = characterIn(text, close)
    await tapThenWait(...finger)
    expect(document.elementFromPoint(...finger)).toBe(closeButton())
    expect(lookUpTitle()).toBe('猫猫')
    expect(asked).toHaveLength(2)
  })

  it.each([390, 1440])(
    'leaves the word tapped lit and the focus in the panel, where Tab goes on to its button, at %i px wide',
    async (width) => {
      await windowOf(width, 844)
      const { host } = await openReader(english)
      await tapThenWait(...across(characterBox(host, 0, 13), 0.5))
      expect(lookUpTitle()).toBe('Nobody')
      expect(window.getSelection()?.toString()).toBe('Nobody')
      const panel = document.querySelector(
        '[role="dialog"][aria-label="Look up"]',
      )
      expect(panel?.contains(document.activeElement)).toBe(true)
      await userEvent.keyboard('{Tab}')
      expect(document.activeElement).toBe(closeButton())
    },
  )
})

describe('a paged text with the look-up mode on', () => {
  it('turns the page from its outer thirds, over the words there, and looks up only from the middle', async () => {
    await windowOf(800, 600)
    const { host, asked } = await openReader(kanjiBook, {
      prefs: { 'book-layout': { mode: 'scroll', paged: true } },
    })
    const text = host.querySelector('.book-text') as HTMLElement
    expect(text.scrollWidth).toBeGreaterThan(text.clientWidth)
    const box = text.getBoundingClientRect()
    const third = (f: number) =>
      characterIn(text, {
        ...box.toJSON(),
        left: box.left + box.width * f,
        right: box.left + box.width * (f + 0.1),
        top: box.top + box.height / 2,
        bottom: box.top + box.height / 2 + 40,
      })
    await tapThenWait(...third(0.8))
    await expect.poll(() => text.scrollLeft).toBe(text.clientWidth)
    expect(asked).toEqual([])
    await tapThenWait(...third(0.1))
    await expect.poll(() => text.scrollLeft).toBe(0)
    expect(asked).toEqual([])
    expect(lookUpTitle()).toBeUndefined()
    await tapThenWait(...third(0.45))
    expect(asked).toHaveLength(1)
    expect(lookUpTitle()).toBe('猫')
    expect(text.scrollLeft).toBe(0)
  })
})

describe('a double tap with the look-up mode on', () => {
  /**
   * Two real taps, stamped `apart` ms from each other and sent 700 ms apart,
   * longer than any double tap, so that only the stamps can make them one,
   * however fast this machine is. The first is on `point`, the second where
   * `next` says once the first's panel has opened and the text has moved, by
   * default the same point of the window.
   */
  async function tapsStamped(
    point: [number, number],
    apart: number,
    next: () => [number, number] = () => point,
  ) {
    const first = Date.now() / 1000
    await tapAt(...point, first)
    await wait(700)
    await tapAt(...next(), first + apart / 1000)
    await wait(400)
  }

  it('closes the look-up the first tap opened, then peeks at the sentence', async () => {
    await windowOf(1440, 900)
    const { host, asked } = await openReader(translated)
    await tapsStamped(across(characterBox(host, 0, 1), 0.5), 150)
    expect(lookUpTitle()).toBeUndefined()
    expect(host.querySelector('.book-peek')?.textContent).toBe('One two three.')
    expect(asked).toHaveLength(1)
  })

  it('takes two taps too far apart as two look-ups, and peeks at nothing', async () => {
    await windowOf(1440, 900)
    const { host, asked } = await openReader(translated)
    await tapsStamped(across(characterBox(host, 0, 1), 0.5), 500, () =>
      across(characterBox(host, 0, 5), 0.5),
    )
    expect(lookUpTitle()).toBe('五')
    expect(host.querySelector('.book-peek')).toBeNull()
    expect(asked).toHaveLength(2)
  })

  it('peeks at the sentence the first tap was on when the second lands on another', async () => {
    await windowOf(1440, 900)
    const { host, asked } = await openReader(translated)
    await tapsStamped(across(characterBox(host, 0, 1), 0.5), 150, () =>
      across(characterBox(host, 0, 5), 0.5),
    )
    expect(lookUpTitle()).toBeUndefined()
    expect(host.querySelector('.book-peek')?.textContent).toBe('One two three.')
    expect(asked).toHaveLength(1)
  })

  it('with the mode off peeks at the sentence the second tap was on', async () => {
    // Clear of the top bar, which the first tap shows while the second waits.
    styled('.book-text [data-block="0"] { margin-top: 120px; }')
    await windowOf(1440, 900)
    const { host, asked } = await openReader(translated, { lookUp: false })
    await tapsStamped(across(characterBox(host, 0, 1), 0.5), 150, () =>
      across(characterBox(host, 0, 5), 0.5),
    )
    expect(host.querySelector('.book-peek')?.textContent).toBe('Four five six.')
    expect(asked).toEqual([])
  })

  it('does not follow a link the text has moved under the second tap', async () => {
    await windowOf(1440, 900)
    const linked = {
      ...translated,
      editions: {
        ...translated.editions,
        ja: {
          ...translated.editions.ja,
          blocks: [
            block(
              '一二三。四五六。',
              [
                [0, 4],
                [4, 8],
              ],
              [{ kind: 'link', start: 5, end: 6, block: 0, note: false }],
            ),
          ],
        },
      },
    }
    const { host } = await openReader(linked)
    await tapsStamped(across(characterBox(host, 0, 1), 0.5), 150, () =>
      across(
        (host.querySelector('[data-link]') as Element).getBoundingClientRect(),
        0.5,
      ),
    )
    expect(lookUpTitle()).toBeUndefined()
    expect(host.querySelector('.book-peek')?.textContent).toBe('One two three.')
  })
})

/** The prefs that set a text vertical, scrolled or `paged`. */
const verticalPrefs = (paged: boolean) => ({
  'book-text': { vertical: true },
  'book-layout': { mode: 'scroll', paged },
})

/** Kanji enough to fill a vertical page, every third one glossed. */
const GLOSSED = '猫犬鳥魚虫馬牛羊鹿熊'.repeat(130)
const glossedPage = {
  ...book,
  editions: {
    ja: {
      ...book.editions.ja,
      blocks: [
        block(
          GLOSSED,
          [[0, GLOSSED.length]],
          Array.from({ length: GLOSSED.length / 3 - 1 }, (_, i) => ({
            kind: 'ruby',
            start: i * 3,
            end: i * 3 + 1,
            reading: 'かな',
          })),
        ),
      ],
    },
  },
}

/**
 * A glossed character in the middle third of `text`, where a paged text looks
 * up, with the two after it below it in the same column.
 */
function glossedInMiddle(host: HTMLElement, text: HTMLElement) {
  const box = text.getBoundingClientRect()
  for (let at = 0; at + 2 < GLOSSED.length; at += 3) {
    const [base, next, last] = [at, at + 1, at + 2].map((n) =>
      characterBox(host, 0, n),
    )
    if (
      base.left > box.left + box.width * 0.4 &&
      base.right < box.left + box.width * 0.6 &&
      next.left === base.left &&
      last.left === base.left &&
      base.top >= box.top &&
      last.bottom <= Math.min(box.bottom, window.innerHeight)
    ) {
      return at
    }
  }
  throw new Error('no glossed character lies in the middle third')
}

describe.each([
  ['scrolled', false],
  ['paged', true],
])('a tap in vertical text, %s', (_, paged) => {
  it('looks up from the character at each quarter of a glossed base and the two after it, and from the base for its reading', async () => {
    await windowOf(1440, 900)
    const { host, asked } = await openReader(glossedPage, {
      prefs: verticalPrefs(paged),
    })
    const text = host.querySelector('.book-text') as HTMLElement
    expect(text.dataset.vertical).toBe('true')
    const base = glossedInMiddle(host, text)
    const got: Record<string, string[]> = {}
    const want: Record<string, string[]> = {}
    const tap = async (name: string, at: number, x: number, y: number) => {
      const before = asked.length
      await tapThenWait(x, y)
      got[name] = asked.slice(before)
      want[name] = [lookedUpFrom(GLOSSED, at)]
      await closeLookUp()
    }
    for (const at of [base, base + 1, base + 2]) {
      for (const [fx, fy] of [
        [0.25, 0.25],
        [0.75, 0.25],
        [0.25, 0.75],
        [0.75, 0.75],
      ]) {
        const box = characterBox(host, 0, at)
        await tap(
          `character ${at - base} at ${fx}, ${fy}`,
          at,
          box.left + box.width * fx,
          box.top + box.height * fy,
        )
      }
    }
    const reading = host
      .querySelectorAll('[data-block="0"] rt')
      [base / 3].getBoundingClientRect()
    await tap(
      'the reading',
      base,
      reading.left + reading.width / 2,
      reading.top + reading.height / 2,
    )
    expect(got).toEqual(want)
  }, 60_000)

  it('asks nothing for a point level with the last character, past the last column', async () => {
    await windowOf(1440, 900)
    const short = '猫犬鳥魚虫馬牛羊鹿熊'.repeat(4)
    const { host, asked } = await openReader(
      {
        ...book,
        editions: { ja: { ...book.editions.ja, blocks: [block(short)] } },
      },
      { prefs: verticalPrefs(paged) },
    )
    const text = (
      host.querySelector('.book-text') as HTMLElement
    ).getBoundingClientRect()
    const last = characterBox(host, 0, short.length - 1)
    const x = text.left + text.width / 2
    expect(x).toBeLessThan(last.left - last.width * 2)
    await tapThenWait(x, last.top + last.height / 2)
    expect(asked).toEqual([])
    expect(lookUpTitle()).toBeUndefined()
  })
})

describe('a paged vertical text with the look-up mode on', () => {
  it('turns on from its left third and back from its right, over the words there, and looks up only from the middle', async () => {
    await windowOf(800, 600)
    const { host, asked } = await openReader(kanjiBook, {
      prefs: verticalPrefs(true),
    })
    const text = host.querySelector('.book-text') as HTMLElement
    expect(text.scrollHeight).toBeGreaterThan(text.clientHeight)
    const box = text.getBoundingClientRect()
    const third = (f: number) =>
      characterIn(text, {
        ...box.toJSON(),
        left: box.left + box.width * f,
        right: box.left + box.width * (f + 0.1),
        top: box.top + box.height / 2,
        bottom: box.top + box.height / 2 + 40,
      })
    await tapThenWait(...third(0.1))
    await expect.poll(() => text.scrollTop).toBeGreaterThan(0)
    expect(asked).toEqual([])
    await tapThenWait(...third(0.8))
    await expect.poll(() => text.scrollTop).toBe(0)
    expect(asked).toEqual([])
    expect(lookUpTitle()).toBeUndefined()
    await tapThenWait(...third(0.45))
    expect(asked).toHaveLength(1)
    expect(lookUpTitle()).toBe('猫')
    expect(text.scrollTop).toBe(0)
  })
})

/** Twenty kana, a match for `ねずみ`, and more kana after it. */
const MOUSE =
  'あいうえおかきくけこさしすせそたちつてとねずみなにぬねのはひふへほ'

/** Lights `query`'s match by the search panel, then closes the panel. */
async function lightMatch(host: HTMLElement, query: string) {
  const key = (name: string, on: EventTarget = window) =>
    act(() => {
      on.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: name,
          bubbles: true,
          cancelable: true,
        }),
      )
    })
  key('/')
  await wait(20)
  const box = host.querySelector(
    '.docked-panel input[type="search"]',
  ) as HTMLInputElement
  await typeInto(box, query)
  await wait(250)
  key('Enter', box)
  await wait(50)
  key('Escape', box)
  key('Escape')
  await wait(100)
}

describe.each([
  ['vertical', true],
  ['horizontal', false],
])('a tap on a lit search match, %s', (_, vertical) => {
  it.each([
    ['the near half of its first character', 20, 0.25],
    ['the far half of its middle character', 21, 0.75],
    ['the far half of its last character', 22, 0.75],
    ['the near half of the character after it', 23, 0.25],
    ['the far half of the character before it', 19, 0.75],
  ])(
    'on %s looks up from there and puts out the match',
    async (_name, at, f) => {
      await windowOf(1366, 768)
      const { host, asked } = await openReader(
        {
          ...book,
          editions: { ja: { ...book.editions.ja, blocks: [block(MOUSE)] } },
        },
        {
          prefs: { 'book-text': { vertical, indent: false } },
          found: { length: 2, entries: [] },
        },
      )
      const text = host.querySelector('.book-text') as HTMLElement
      await lightMatch(host, 'ねずみ')
      expect(text.querySelector('.book-found')?.textContent).toBe('ねずみ')
      const box = characterBox(host, 0, at)
      await tapThenWait(
        vertical ? box.left + box.width / 2 : box.left + box.width * f,
        vertical ? box.top + box.height * f : box.top + box.height / 2,
      )
      expect(asked).toEqual([lookedUpFrom(MOUSE, at)])
      expect(selectionIn(text)?.range).toEqual({ start: at, end: at + 2 })
      expect(text.querySelector('.book-found')).toBeNull()
    },
  )
})

/** Its 12 and its 7 are set upright in vertical text. */
const NUMBERS = '昭和12年に7人が来た。吾輩は猫である。'

describe('a tap on a number set upright', () => {
  it('looks up from its first digit wherever it lands on the number, and from the character beside it off the number', async () => {
    await windowOf(1366, 768)
    const { host, asked } = await openReader(
      {
        ...book,
        editions: { ja: { ...book.editions.ja, blocks: [block(NUMBERS)] } },
      },
      { prefs: { 'book-text': { vertical: true } } },
    )
    const upright = (n: number) =>
      host
        .querySelectorAll('[data-block="0"] .book-upright')
        [n].getBoundingClientRect()
    expect(
      [...host.querySelectorAll('[data-block="0"] .book-upright')].map(
        (run) => run.textContent,
      ),
    ).toEqual(['12', '7'])
    const on = (n: number, fx: number, fy: number): [number, number] => {
      const box = upright(n)
      return [box.left + box.width * fx, box.top + box.height * fy]
    }
    const middle = (at: number, fy: number): [number, number] => {
      const box = characterBox(host, 0, at)
      return [box.left + box.width / 2, box.top + box.height * fy]
    }
    const taps: [string, () => [number, number], number][] = [
      ...[0.15, 0.35, 0.65, 0.85].flatMap((fx) =>
        [0.25, 0.75].map((fy): [string, () => [number, number], number] => [
          `12 at ${fx}, ${fy}`,
          () => on(0, fx, fy),
          2,
        ]),
      ),
      [
        'the leading right of 12',
        () => [upright(0).right + 3, on(0, 0, 0.5)[1]],
        2,
      ],
      [
        'the leading left of 12',
        () => [upright(0).left - 3, on(0, 0, 0.5)[1]],
        2,
      ],
      ['the middle of 7', () => on(1, 0.5, 0.5), 6],
      ['the far half of 和, before 12', () => middle(1, 0.75), 1],
      ['the near half of 年, after 12', () => middle(4, 0.25), 4],
      ['the far half of 年, after 12', () => middle(4, 0.75), 4],
      ['the near half of 人, after 7', () => middle(7, 0.25), 7],
    ]
    const got: Record<string, string[]> = {}
    const want: Record<string, string[]> = {}
    for (const [name, point, at] of taps) {
      const before = asked.length
      await tapThenWait(...point())
      got[name] = asked.slice(before)
      want[name] = [lookedUpFrom(NUMBERS, at)]
      await closeLookUp()
    }
    expect(got).toEqual(want)
  }, 30_000)
})

describe('a selection the reader made, when a tap looks a word up', () => {
  it.each([
    ['finds no word there', { found: { length: 0, entries: [] } }],
    ['has no dictionary for the language', { status: 404 }],
    ['is refused', { status: 500 }],
  ])('is cleared when the server %s', async (_, answer) => {
    const { host, asked } = await openReader(book, answer)
    const words = document
      .createTreeWalker(
        host.querySelector('[data-block="1"] [data-text]') as Node,
        NodeFilter.SHOW_TEXT,
      )
      .nextNode() as Text
    window.getSelection()?.setBaseAndExtent(words, 0, words, 3)
    expect(window.getSelection()?.toString()).toBe('吾輩は')
    await tapThenWait(...across(characterBox(host, 0, 4), 0.5))
    expect(asked).toEqual([lookedUpFrom(OPENING, 4)])
    expect(window.getSelection()?.toString()).toBe('')
  })
})

describe('a wheel over vertical scrolled text with the look-up mode on', () => {
  it('scrolls the text on and looks nothing up', async () => {
    await windowOf(1366, 768)
    const { host, asked } = await openReader(kanjiBook, {
      prefs: { 'book-text': { vertical: true } },
    })
    const text = host.querySelector('.book-text') as HTMLElement
    const box = text.getBoundingClientRect()
    const before = text.scrollLeft
    await mouse.moveTo(box.left + box.width / 2, box.top + box.height / 2)
    await mouse.wheel(120)
    await expect.poll(() => before - text.scrollLeft).toBeGreaterThan(50)
    expect(asked).toEqual([])
    expect(lookUpTitle()).toBeUndefined()
  })
})

/** Forty long paragraphs, to scroll or turn the pages of in every flow. */
const PARAGRAPH =
  '吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。'.repeat(8)
const longBook = {
  ...book,
  editions: {
    ja: {
      ...book.editions.ja,
      blocks: Array.from({ length: 40 }, () => block(PARAGRAPH)),
    },
  },
}

const FLOWS = [
  { flow: 'horizontal, scrolled', vertical: false, paged: false },
  { flow: 'horizontal, paged', vertical: false, paged: true },
  { flow: 'vertical, scrolled', vertical: true, paged: false },
  { flow: 'vertical, paged', vertical: true, paged: true },
]
type Flow = (typeof FLOWS)[number]

/** A window where the panel's set-in lays the text out afresh. */
const SET_IN = [1000, 700] as const
/** The narrowest beside a panel, where the set-in takes the most lines' room. */
const NARROW = [900, 700] as const
/** A window wide enough to keep horizontal text's column as it was. */
const WIDE = [1366, 768] as const
/** The phone layout, where the panel goes over the text. */
const PHONE = [390, 844] as const

/** `flow` of the long book in a window `size`, some way into the book. */
async function openFlow(
  { vertical, paged }: Flow,
  size: readonly [number, number],
  answer: {
    found?: { length: number; entries: object[] }
    lookUp?: boolean
    words?: () => Promise<object[]>
  } = {
    found: { length: 2, entries: [] },
  },
) {
  await windowOf(...size)
  const { host, asked } = await openReader(longBook, {
    prefs: {
      'book-text': { vertical },
      'book-layout': { mode: 'scroll', paged },
    },
    ...answer,
  })
  for (let n = 0; n < 3; n++) {
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'PageDown',
          bubbles: true,
          cancelable: true,
        }),
      )
    })
    await wait(80)
  }
  await wait(150)
  return {
    host,
    asked,
    text: host.querySelector('.book-text') as HTMLElement,
  }
}

type Spot = { block: number; at: number; box: DOMRect }

/** The box of the `length` characters of block `n` from `at`, over every line they take. */
function wordBox(host: HTMLElement, n: number, at: number, length = 2) {
  const boxes = Array.from({ length }, (_, i) => characterBox(host, n, at + i))
  const left = Math.min(...boxes.map((b) => b.left))
  const top = Math.min(...boxes.map((b) => b.top))
  return new DOMRect(
    left,
    top,
    Math.max(...boxes.map((b) => b.right)) - left,
    Math.max(...boxes.map((b) => b.bottom)) - top,
  )
}

/**
 * Every two characters of the long book wholly on screen and on one line, no
 * full stop among them: paged, only in the middle third, where a tap looks up.
 */
function spotsInSight(host: HTMLElement, text: HTMLElement, paged: boolean) {
  const screen = text.getBoundingClientRect()
  const spots: Spot[] = []
  for (const held of host.querySelectorAll<HTMLElement>('[data-block]')) {
    const drawn = held.getBoundingClientRect()
    if (drawn.bottom < screen.top || drawn.top > screen.bottom) continue
    if (drawn.right < screen.left || drawn.left > screen.right) continue
    const n = Number(held.dataset.block)
    for (let at = 0; at + 1 < PARAGRAPH.length; at++) {
      if (PARAGRAPH.slice(at, at + 2).includes('。')) continue
      const [a, b] = [characterBox(host, n, at), characterBox(host, n, at + 1)]
      if (a.top !== b.top && a.left !== b.left) continue
      const box = wordBox(host, n, at)
      const inside =
        box.left >= screen.left + 4 &&
        box.right <= screen.right - 4 &&
        box.top >= Math.max(screen.top, 0) + 4 &&
        box.bottom <= Math.min(screen.bottom, window.innerHeight) - 4
      const middle = (box.left + box.width / 2 - screen.left) / screen.width
      if (inside && (!paged || (middle > 0.38 && middle < 0.62))) {
        spots.push({ block: n, at, box })
      }
    }
  }
  return spots
}

/** The spot scoring highest by `score`. */
const best = (spots: Spot[], score: (box: DOMRect) => number) => {
  expect(spots.length).toBeGreaterThan(0)
  return spots.reduce((a, b) => (score(b.box) > score(a.box) ? b : a))
}

/**
 * A word on the line the bottom of the screen cuts through, its middle still
 * there to be tapped: at phone size the panel opens clear of a word, which
 * leaves the edge of the screen as what can hide one.
 */
async function cutByTheBottom(host: HTMLElement, text: HTMLElement) {
  const spot = best(spotsInSight(host, text, false), (box) => box.bottom)
  const { bottom } = text.getBoundingClientRect()
  act(() => {
    text.scrollBy({ top: spot.box.bottom - bottom - spot.box.height / 4 })
  })
  await wait(150)
  return spot
}

/**
 * The word a look-up is most in danger of losing in `flow`: on the side the
 * panel sets the text in from, or at phone width, cut by the edge of the
 * screen where the text can scroll to show it, and elsewhere low on the screen.
 */
async function spotToLose(
  host: HTMLElement,
  text: HTMLElement,
  { vertical, paged }: Flow,
  size: readonly [number, number],
) {
  const spots = spotsInSight(host, text, paged)
  if (size !== PHONE) {
    return best(spots, (box) => (vertical ? -box.left : box.bottom))
  }
  if (!vertical && !paged) return cutByTheBottom(host, text)
  return best(spots, (box) => box.bottom)
}

/** The `length` characters of the long book from `at`, as a range. */
function rangeAt(
  host: HTMLElement,
  { block: n, at }: Pick<Spot, 'block' | 'at'>,
  length = 2,
) {
  const words = document
    .createTreeWalker(
      host.querySelector(`[data-block="${n}"] > [data-text]`) as Node,
      NodeFilter.SHOW_TEXT,
    )
    .nextNode() as Text
  const range = document.createRange()
  range.setStart(words, at)
  range.setEnd(words, at + length)
  return range
}

const LOOK_UP = '[role="dialog"][aria-label="Look up"]'

const panelBox = () =>
  (document.querySelector(LOOK_UP) as Element).getBoundingClientRect()

const meets = (a: DOMRect, b: DOMRect) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom

/** Whether `box` is wholly inside the text's box and clear of the panel. */
function inSight(text: HTMLElement, box: DOMRect) {
  const screen = text.getBoundingClientRect()
  return {
    inText:
      box.left >= screen.left - 0.5 &&
      box.right <= screen.right + 0.5 &&
      box.top >= screen.top - 0.5 &&
      box.bottom <= screen.bottom + 0.5,
    clearOfPanel: !meets(box, panelBox()),
  }
}

/** `length` characters of the long book selected, and the pointer let go as a drag ends. */
async function selectSpot(
  host: HTMLElement,
  text: HTMLElement,
  { block: n, at }: Pick<Spot, 'block' | 'at'>,
  length = 2,
) {
  const words = document
    .createTreeWalker(
      host.querySelector(`[data-block="${n}"] > [data-text]`) as Node,
      NodeFilter.SHOW_TEXT,
    )
    .nextNode() as Text
  window.getSelection()?.setBaseAndExtent(words, at, words, at + length)
  act(() => {
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
  await wait(60)
}

/** The selection bar's Look up, pressed. */
async function pressLookUp(host: HTMLElement) {
  const button = [...host.querySelectorAll('.book-selection-bar button')].find(
    (b) => b.textContent === 'Look up',
  ) as HTMLElement
  act(() => button.click())
  await wait(350)
}

const tapSpot = ({ block: n, at }: Spot, host: HTMLElement) =>
  tapThenWait(...across(characterBox(host, n, at), 0.5))

const scrolledTo = (text: HTMLElement) => [text.scrollLeft, text.scrollTop]

describe.each(FLOWS)('the word a look-up lights, $flow', (flow) => {
  describe.each([
    ['900 by 700', NARROW],
    ['1000 by 700', SET_IN],
    ['1366 by 768', WIDE],
    ['390 by 844', PHONE],
  ] as const)('at %s', (_, size) => {
    it('stays in sight when a tap opens the look-up', async () => {
      const { host, text } = await openFlow(flow, size)
      const spot = await spotToLose(host, text, flow, size)
      await tapSpot(spot, host)
      await wait(150)
      expect(lookUpTitle()).toBeDefined()
      expect(inSight(text, wordBox(host, spot.block, spot.at))).toEqual({
        inText: true,
        clearOfPanel: true,
      })
    })

    it('stays in sight when the selection bar opens the look-up', async () => {
      const { host, text } = await openFlow(flow, size)
      const spot = await spotToLose(host, text, flow, size)
      await selectSpot(host, text, spot)
      await pressLookUp(host)
      await wait(150)
      expect(lookUpTitle()).toBe(PARAGRAPH.slice(spot.at, spot.at + 2))
      expect(inSight(text, wordBox(host, spot.block, spot.at))).toEqual({
        inText: true,
        clearOfPanel: true,
      })
    })
  })

  it('moves nothing at phone width when the word is in sight already', async () => {
    const { host, text } = await openFlow(flow, PHONE)
    const spot = best(spotsInSight(host, text, flow.paged), (box) => box.bottom)
    const before = scrolledTo(text)
    await tapSpot(spot, host)
    await wait(150)
    expect(lookUpTitle()).toBeDefined()
    expect(scrolledTo(text)).toEqual(before)
  })

  it('keeps the reader where the word took them when the panel closes', async () => {
    const { host, text } = await openFlow(flow, SET_IN)
    const spot = await spotToLose(host, text, flow, SET_IN)
    await tapSpot(spot, host)
    await wait(150)
    const here = topOf(text)
    await closeLookUp()
    await wait(200)
    const screen = text.getBoundingClientRect()
    if (flow.paged) {
      // Laid out afresh, the page may begin elsewhere, but it still holds the
      // character the page open beside the panel began with.
      const first = characterBox(host, here.block, here.at)
      expect(first.left).toBeGreaterThanOrEqual(screen.left - 0.5)
      expect(first.right).toBeLessThanOrEqual(screen.right + 0.5)
      expect(first.top).toBeGreaterThanOrEqual(screen.top - 0.5)
      expect(first.bottom).toBeLessThanOrEqual(screen.bottom + 0.5)
    } else {
      // Within a line, which the wider text may break elsewhere.
      const now = topOf(text)
      expect(now.block).toBe(here.block)
      expect(Math.abs(now.at - here.at)).toBeLessThan(60)
    }
    const word = wordBox(host, spot.block, spot.at)
    expect(word.left).toBeGreaterThanOrEqual(screen.left - 0.5)
    expect(word.right).toBeLessThanOrEqual(screen.right + 0.5)
    expect(word.top).toBeGreaterThanOrEqual(screen.top - 0.5)
    expect(word.bottom).toBeLessThanOrEqual(screen.bottom + 0.5)
  })
})

describe.each(FLOWS.filter((f) => !(f.vertical && f.paged)))(
  'a word in sight beside the set-in, $flow',
  (flow) => {
    it('moves nothing at 1366 by 768', async () => {
      const { host, text } = await openFlow(flow, WIDE)
      const screen = text.getBoundingClientRect()
      const spot = best(
        spotsInSight(host, text, flow.paged),
        (box) =>
          -Math.abs(box.top - (screen.top + screen.height / 2)) -
          Math.abs(box.right - screen.right + screen.width / 4),
      )
      const before = scrolledTo(text)
      await tapSpot(spot, host)
      await wait(150)
      expect(lookUpTitle()).toBeDefined()
      expect(scrolledTo(text)).toEqual(before)
    })
  },
)

describe('a word level with the panel docked beside the text', () => {
  it('moves nothing at 1366 by 768', async () => {
    const flow = FLOWS[0]
    const { host, text } = await openFlow(flow, WIDE)
    const spot = best(
      spotsInSight(host, text, flow.paged).filter(
        (s) => s.box.top > 80 && s.box.top < 150,
      ),
      (box) => box.left,
    )
    const before = scrolledTo(text)
    await tapSpot(spot, host)
    await wait(150)
    expect(lookUpTitle()).toBeDefined()
    expect(scrolledTo(text)).toEqual(before)
  })
})

describe.each(FLOWS.filter((f) => f.vertical || f.paged))(
  'a word near the top at phone width, $flow',
  (flow) => {
    it('moves nothing, since the panel opens across the screen from it', async () => {
      const { host, text } = await openFlow(flow, PHONE)
      const spot = best(
        spotsInSight(host, text, flow.paged).filter(
          (s) => s.box.top > 72 && s.box.top < 170,
        ),
        (box) => box.left,
      )
      const before = scrolledTo(text)
      await tapSpot(spot, host)
      await wait(150)
      expect(
        (document.querySelector(LOOK_UP) as HTMLElement).dataset.side,
      ).toBe('bottom')
      expect(
        inSight(text, wordBox(host, spot.block, spot.at)).clearOfPanel,
      ).toBe(true)
      expect(scrolledTo(text)).toEqual(before)
    })
  },
)

describe('the least move that brings a word into sight', () => {
  const edgeGap = (word: DOMRect, text: HTMLElement) => {
    const screen = text.getBoundingClientRect()
    return {
      'horizontal, scrolled': Math.abs(word.bottom - screen.bottom),
      'vertical, scrolled': Math.abs(word.left - screen.left),
      phone: Math.abs(word.bottom - screen.bottom),
    }
  }

  /**
   * A cover across the screen from `above` over `word` to `below` under it. A
   * seated panel lies against an edge of the screen, which leaves a word one
   * stretch to go to, so a choice between two ways has to be drawn.
   */
  const coverOver = (word: DOMRect, above: number, below: number) =>
    new DOMRect(
      0,
      word.top - above,
      window.innerWidth,
      above + word.height + below,
    )

  /** The word of the long book nearest 300 px down the screen. */
  const spotPartWayDown = (host: HTMLElement, text: HTMLElement) =>
    best(spotsInSight(host, text, false), (box) => -Math.abs(box.top - 300))

  it.each([
    ['horizontal, scrolled', FLOWS[0], NARROW],
    ['vertical, scrolled', FLOWS[2], SET_IN],
    ['phone', FLOWS[0], PHONE],
  ] as const)(
    'leaves the word at the edge it came in by, %s',
    async (name, flow, size) => {
      const { host, text } = await openFlow(flow, size)
      const spot = await spotToLose(host, text, flow, size)
      await tapSpot(spot, host)
      await wait(150)
      const gap = edgeGap(wordBox(host, spot.block, spot.at), text)[name]
      expect(gap).toBeLessThanOrEqual(2)
    },
  )

  it('goes the other way where the book ends before the shorter way', async () => {
    const { host, text } = await openFlow(FLOWS[0], PHONE)
    act(() => {
      text.scrollTo({ top: text.scrollHeight })
    })
    await wait(150)
    const spot = spotPartWayDown(host, text)
    const cover = coverOver(spot.box, 4, 200)
    const before = text.scrollTop
    bringIntoSight(text, rangeAt(host, spot), [cover])
    await wait(100)
    expect(text.scrollTop).toBeLessThan(before)
    expect(meets(wordBox(host, spot.block, spot.at), cover)).toBe(false)
  })

  it('takes the shorter way past a cover with room on both sides', async () => {
    const { host, text } = await openFlow(FLOWS[0], PHONE)
    const spot = spotPartWayDown(host, text)
    const cover = coverOver(spot.box, 4, 200)
    bringIntoSight(text, rangeAt(host, spot), [cover])
    await wait(100)
    const gap = cover.top - wordBox(host, spot.block, spot.at).bottom
    expect(gap).toBeGreaterThanOrEqual(0)
    expect(gap).toBeLessThanOrEqual(2)
  })

  it.each(FLOWS.filter((f) => f.paged))(
    'leaves a covered word on its own page, $flow',
    async (flow) => {
      const { host, text } = await openFlow(flow, PHONE)
      const spot = best(spotsInSight(host, text, true), (box) => box.top)
      const before = scrolledTo(text)
      bringIntoSight(text, rangeAt(host, spot), [spot.box])
      await wait(100)
      expect(scrolledTo(text)).toEqual(before)
    },
  )

  it('stops short of the bars while they are shown', async () => {
    const flow = FLOWS[0]
    const { host, text } = await openFlow(flow, PHONE)
    await tapThenWait(6, 350)
    expect(barsShown(host)).toBe(true)
    const topBar = (
      host.querySelector('.reader-top-bar') as Element
    ).getBoundingClientRect()
    // On the line the bar's lower edge cuts through, its middle still there
    // to be tapped: the way into sight ends at the bar, not the screen's edge.
    const spot = best(
      spotsInSight(host, text, false).filter((s) => s.box.top > topBar.bottom),
      (box) => -box.top,
    )
    act(() => {
      text.scrollBy({
        top: spot.box.top - topBar.bottom + spot.box.height / 4,
      })
    })
    await wait(150)
    expect(barsShown(host)).toBe(true)
    expect(meets(wordBox(host, spot.block, spot.at), topBar)).toBe(true)
    await tapSpot(spot, host)
    await wait(150)
    const word = wordBox(host, spot.block, spot.at)
    expect(inSight(text, word)).toEqual({ inText: true, clearOfPanel: true })
    expect(meets(word, topBar)).toBe(false)
  })
})

/** Thirty paragraphs of English, for a word found as soon as it is tapped. */
const PROSE = 'Nobody knew what the matter was with anyone. '.repeat(30)
const proseBook = {
  ...english,
  editions: {
    en: {
      ...english.editions.en,
      blocks: Array.from({ length: 30 }, () => block(PROSE)),
    },
  },
}

/** A dictionary's entry for `matter`, `senses` long. */
const matterEntry = (senses: number) => ({
  word: 'matter',
  pos: 'noun',
  glosses: Array.from({ length: senses }, (_, n) => `sense ${n}`),
})

/** The text of `proseBook`, scrolled some way in. */
async function scrolledIntoProse(host: HTMLElement) {
  const text = host.querySelector('.book-text') as HTMLElement
  act(() => {
    text.scrollTo({ top: 3000 })
  })
  await wait(150)
  return text
}

/** Where `word` of `PROSE` is drawn with its top from `above` to `below`. */
function proseWord(
  host: HTMLElement,
  word: string,
  above: number,
  below: number,
) {
  for (let n = 0; n < 30; n++) {
    const held = host.querySelector(`[data-block="${n}"]`) as Element
    const drawn = held.getBoundingClientRect()
    if (drawn.bottom < above || drawn.top > below) continue
    for (
      let at = PROSE.indexOf(word);
      at >= 0;
      at = PROSE.indexOf(word, at + 1)
    ) {
      const { top } = characterBox(host, n, at)
      if (top >= above && top <= below) return { block: n, at }
    }
  }
  throw new Error(`no ${word} is drawn from ${above} to ${below}`)
}

describe('a word whose answer changes what is in sight', () => {
  const horizontal = FLOWS[0]

  /** The last word of a line set on the text's last edge, the next out of sight. */
  const atLineEnd = async (host: HTMLElement, text: HTMLElement) => {
    const running = () =>
      spotsInSight(host, text, false).filter(
        (s) => s.at + 40 <= PARAGRAPH.length,
      )
    const lowest = best(running(), (box) => box.bottom).box.bottom
    act(() => {
      text.scrollBy({ top: lowest - (text.getBoundingClientRect().bottom - 6) })
    })
    await wait(150)
    const spots = running()
    const line = best(spots, (box) => box.bottom).box.bottom
    return best(
      spots.filter((s) => s.box.bottom === line),
      (box) => box.right,
    )
  }

  it('brings in the end of a word the answer runs on to the next line', async () => {
    const { host, text } = await openFlow(horizontal, WIDE, {
      found: { length: 6, entries: [] },
    })
    const spot = await atLineEnd(host, text)
    await tapSpot(spot, host)
    await wait(150)
    expect(inSight(text, wordBox(host, spot.block, spot.at, 6))).toEqual({
      inText: true,
      clearOfPanel: true,
    })
  })

  it.each([
    ['beside the panel', WIDE],
    ['at phone size', PHONE],
  ] as const)(
    'brings in the end of a word an answer lengthens late, the reader still where they asked, %s',
    async (_, size) => {
      let answer = (_: object[]) => {}
      const held = new Promise<object[]>((done) => {
        answer = done
      })
      const { host, text } = await openFlow(horizontal, size, {
        found: { length: 6, entries: [] },
        words: () => held,
      })
      const spot = await atLineEnd(host, text)
      await tapSpot(spot, host)
      await wait(300)
      expect(inSight(text, wordBox(host, spot.block, spot.at, 6)).inText).toBe(
        false,
      )
      await act(async () => answer([]))
      await wait(150)
      expect(inSight(text, wordBox(host, spot.block, spot.at, 6))).toEqual({
        inText: true,
        clearOfPanel: true,
      })
    },
  )

  it('brings in a word lengthened by a new look-up after the reader moved on from the last', async () => {
    let answer = (_: object[]) => {}
    const held = new Promise<object[]>((done) => {
      answer = done
    })
    const { host, text } = await openFlow(horizontal, WIDE, {
      found: { length: 6, entries: [] },
      words: () => held,
    })
    await tapSpot(
      best(spotsInSight(host, text, false), (box) => -box.top),
      host,
    )
    act(() => {
      text.scrollBy({ top: 1500 })
    })
    await wait(200)
    await act(async () => answer([]))
    await wait(200)
    await closeLookUp()
    await wait(300)
    const spot = await atLineEnd(host, text)
    await tapSpot(spot, host)
    await wait(150)
    expect(inSight(text, wordBox(host, spot.block, spot.at, 6))).toEqual({
      inText: true,
      clearOfPanel: true,
    })
  })

  it('leaves a reader who scrolled on towards the panel where they are as the answer grows it', async () => {
    await windowOf(...PHONE)
    let answer = (_: object[]) => {}
    const held = new Promise<object[]>((done) => {
      answer = done
    })
    const { host } = await openReader(proseBook, { words: () => held })
    const text = await scrolledIntoProse(host)
    const { block: n, at } = proseWord(host, 'matter', 260, 340)
    await tapThenWait(...across(characterBox(host, n, at), 0.5))
    // Scrolled on towards the panel while it is short: the answer then grows
    // it over the word, and the place is still the reader's.
    act(() => {
      text.scrollBy({ top: characterBox(host, n, at).top - 560 })
    })
    await wait(150)
    const movedTo = text.scrollTop
    await act(async () => answer([matterEntry(40)]))
    await wait(250)
    expect(panelBox().height).toBeGreaterThan(300)
    expect(text.scrollTop).toBe(movedTo)
  })
})

describe('a look-up opened after a long answer', () => {
  it.each(['a tap', 'the selection bar'])(
    'is measured against its own panel when %s opens it',
    async (opener) => {
      await windowOf(...PHONE)
      const { host } = await openReader(proseBook, {
        words: (url) => (url.endsWith('/matter') ? [matterEntry(40)] : []),
      })
      const text = await scrolledIntoProse(host)
      const long = proseWord(host, 'matter', 300, 700)
      await tapThenWait(...across(characterBox(host, long.block, long.at), 0.5))
      await wait(150)
      expect(panelBox().height).toBeGreaterThan(300)
      await closeLookUp()
      await wait(150)
      // Where the panel opens before it is seated, so that measuring it there
      // would scroll the book away from a word it is about to leave clear.
      const next = proseWord(host, 'anyone', 90, 180)
      const before = scrolledTo(text)
      if (opener === 'a tap') {
        await tapThenWait(
          ...across(characterBox(host, next.block, next.at), 0.5),
        )
      } else {
        await selectSpot(host, text, next, 6)
        await pressLookUp(host)
      }
      await wait(150)
      expect(lookUpTitle()).toBe('anyone')
      expect(meets(panelBox(), characterBox(host, next.block, next.at))).toBe(
        false,
      )
      expect(scrolledTo(text)).toEqual(before)
    },
  )
})

describe('a selection running off the screen, looked up from the bar', () => {
  it.each([
    ['its first character', 'over the top edge'],
    ['its last character', 'over the bottom edge'],
  ])('is brought in with %s %s', async (_, edge) => {
    const { host, text } = await openFlow(FLOWS[0], WIDE)
    const screen = text.getBoundingClientRect()
    const mid = best(
      spotsInSight(host, text, false).filter(
        (s) => s.at > 80 && s.at < PARAGRAPH.length - 80,
      ),
      (box) => -Math.abs(box.top - (screen.top + screen.height / 2)),
    )
    const onLine = (at: number) =>
      Math.abs(characterBox(host, mid.block, at).top - mid.box.top) < 5
    const over = edge === 'over the top edge'
    let end = mid.at
    while (onLine(end + (over ? -1 : 1))) end += over ? -1 : 1
    // The line at the edge in sight, the one beyond it wholly out of it.
    const by = over
      ? mid.box.top - screen.top - 3
      : mid.box.bottom - screen.bottom + 3
    act(() => {
      text.scrollBy({ top: by })
    })
    await wait(150)
    const start = end - 2
    const hidden = characterBox(host, mid.block, over ? start : start + 4)
    if (over) expect(hidden.bottom).toBeLessThanOrEqual(screen.top)
    else expect(hidden.top).toBeGreaterThanOrEqual(screen.bottom)
    await selectSpot(host, text, { block: mid.block, at: start }, 5)
    await pressLookUp(host)
    await wait(150)
    expect(inSight(text, wordBox(host, mid.block, start, 5))).toEqual({
      inText: true,
      clearOfPanel: true,
    })
  })
})

describe('the selection bar, after a jump away from its selection', () => {
  const chaptered = {
    ...longBook,
    editions: {
      ja: {
        ...longBook.editions.ja,
        blocks: longBook.editions.ja.blocks.map((b, n) => ({
          ...b,
          heading: n === 0 || n === 35,
        })),
      },
    },
  }

  it.each(FLOWS)('goes with it, $flow', async ({ vertical, paged }) => {
    await windowOf(1440, 900)
    const { host } = await openReader(chaptered, {
      prefs: {
        'book-text': { vertical },
        'book-layout': { mode: 'scroll', paged },
      },
    })
    const text = host.querySelector('.book-text') as HTMLElement
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 't', bubbles: true }),
      )
    })
    await wait(150)
    const spot = best(spotsInSight(host, text, false), (box) => -box.top)
    await selectSpot(host, text, spot)
    expect(host.querySelector('.book-selection-bar')).not.toBeNull()
    const entries = document.querySelectorAll<HTMLElement>(
      '[aria-label="Contents"] .book-contents button',
    )
    expect(entries).toHaveLength(2)
    act(() => entries[1].click())
    await wait(250)
    // The jump has taken the selection off the screen.
    expect(
      meets(wordBox(host, spot.block, spot.at), text.getBoundingClientRect()),
    ).toBe(false)
    expect(host.querySelector('.book-selection-bar')).toBeNull()
  })
})

describe('a look-up that opens under the finger', () => {
  const CATS = '猫'.repeat(400)
  const cats = {
    title: '猫',
    hub: 'ja',
    editions: {
      ja: {
        label: '日本語',
        language: 'ja',
        blocks: Array.from({ length: 20 }, () => block(CATS)),
      },
      en: {
        label: 'English',
        language: 'en',
        blocks: Array.from({ length: 20 }, (_, n) => block(`Cat number ${n}.`)),
      },
    },
    alignments: [
      {
        to: 'en',
        pins: [],
        links: Array.from({ length: 20 }, (_, n) => [n, n + 1, n, n + 1]),
        sentences: Array.from({ length: 20 }, (_, n) => [n, n + 1, n, n + 1]),
      },
    ],
  }
  const answer = {
    found: {
      length: 1,
      entries: [{ word: '猫', pos: 'noun', glosses: ['cat'] }],
    },
  }

  const saveWord = () =>
    [
      ...document.querySelectorAll(
        '[role="dialog"][aria-label="Look up"] button',
      ),
    ].find((b) => b.textContent === 'Save word') as HTMLElement

  /** A point on a character in the middle third, where a paged text looks up. */
  const inMiddle = (text: HTMLElement, top: number, height: number) => {
    const box = text.getBoundingClientRect()
    return characterIn(
      text,
      new DOMRect(box.left + box.width * 0.4, top, box.width * 0.2, height),
    )
  }

  const openCats = async (
    { vertical, paged }: Flow,
    size: readonly [number, number] = PHONE,
    admin = false,
  ) => {
    await windowOf(...size)
    const { host, asked } = await openReader(cats, {
      prefs: {
        'book-text': { vertical },
        'book-layout': { mode: 'scroll', paged },
      },
      admin,
      ...answer,
    })
    return {
      host,
      asked,
      text: host.querySelector('.book-text') as HTMLElement,
    }
  }

  /** Where a first tap opens this answer's panel at phone width. */
  const firstTap = (text: HTMLElement) => inMiddle(text, 500, 120)

  /**
   * Where the panel `firstTap` opens will be over a character at phone width,
   * learnt by opening it once. Scrolled, on Save word itself. Paged, under the
   * panel in the middle third, since a tap on the outer thirds turns the page.
   */
  const pointOnPanel = async (text: HTMLElement, { paged }: Flow) => {
    await tapThenWait(...firstTap(text))
    await wait(100)
    const save = saveWord().getBoundingClientRect()
    const panel = panelBox()
    await closeLookUp()
    await wait(400)
    return paged
      ? inMiddle(text, panel.top + 8, panel.height - 16)
      : characterIn(text, save)
  }

  /**
   * Holds the panel letting taps through until the function answered is
   * called, however slowly this machine sends the taps: only their stamps say
   * how far apart they were.
   */
  const holdTapsPassing = () => {
    const timer = window.setTimeout
    let expire = () => {}
    const held = vi.spyOn(window, 'setTimeout').mockImplementation(((
      run: () => void,
      ms?: number,
    ) => {
      if (ms !== 300) return timer(run, ms)
      expire = run
      return timer(() => {}, 0)
    }) as typeof window.setTimeout)
    onTestFinished(() => held.mockRestore())
    return () => act(() => expire())
  }

  /** A character a tap there looks up from, under where the panel opens if one is. */
  const underThePanel = (
    text: HTMLElement,
    { vertical, paged }: Flow,
    width: number,
  ) => {
    const box = text.getBoundingClientRect()
    const panelLeft = width < 900 || vertical ? 16 : width - 400
    const panelRight = panelLeft + Math.min(384, width - 32)
    const left = Math.max(
      panelLeft,
      paged ? box.left + box.width * 0.36 : box.left,
    )
    const right = Math.min(
      panelRight,
      paged ? box.left + box.width * 0.64 : box.right,
    )
    return right > left
      ? characterIn(text, new DOMRect(left, 90, right - left, 80))
      : inMiddle(text, 120, 200)
  }

  describe.each(['touch', 'mouse'] as const)('a double tap by %s', (by) => {
    const twice = async ([x, y]: [number, number]) => {
      if (by === 'touch') {
        const stamp = Date.now() / 1000
        await tapAt(x, y, stamp)
        await tapAt(x, y, stamp + 0.12)
        return
      }
      mouse.clock(0)
      await mouse.moveTo(x, y)
      await mouse.press()
      mouse.clock(20)
      await mouse.release()
      mouse.clock(120)
      await mouse.press()
      mouse.clock(140)
      await mouse.release()
      mouse.clock()
    }

    it.each([
      { flow: FLOWS[0], width: 1000, height: 700 },
      { flow: FLOWS[0], width: 1024, height: 768 },
      { flow: FLOWS[0], width: 1180, height: 820 },
      { flow: FLOWS[1], width: 900, height: 700 },
      { flow: FLOWS[1], width: 1000, height: 700 },
      { flow: FLOWS[2], width: 900, height: 700 },
      { flow: FLOWS[2], width: 1000, height: 700 },
      { flow: FLOWS[2], width: 1024, height: 768 },
      { flow: FLOWS[2], width: 1440, height: 900 },
      { flow: FLOWS[3], width: 900, height: 700 },
      { flow: FLOWS[3], width: 1000, height: 700 },
      { flow: FLOWS[3], width: 1024, height: 768 },
      ...FLOWS.map((flow) => ({ flow, width: 390, height: 844 })),
    ])(
      'closes the look-up and peeks, $flow.flow at $width by $height',
      async ({ flow, width, height }) => {
        const { host, text } = await openCats(flow, [width, height])
        const point = underThePanel(text, flow, width)
        const tapped = offsetAtPoint(text, ...point)?.block
        expect(tapped).toBeDefined()
        holdTapsPassing()
        await twice(point)
        await wait(400)
        expect(host.querySelector('[data-note]')).toBeNull()
        expect(lookUpTitle()).toBeUndefined()
        expect(host.querySelector('.book-peek')?.textContent).toBe(
          `Cat number ${tapped}.`,
        )
      },
    )
  })

  it.each(['touch', 'mouse'] as const)(
    'leaves the button of the bars to a second tap by %s that lands away from the first',
    async (by) => {
      const { host, text } = await openCats(FLOWS[0], SET_IN)
      await tapThenWait(6, 350)
      expect(barsShown(host)).toBe(true)
      const point = inMiddle(text, 300, 120)
      const settings = [
        ...host.querySelectorAll('.reader-top-bar button'),
      ].find((b) => b.textContent?.includes('Settings')) as HTMLElement
      const button = across(settings.getBoundingClientRect(), 0.5)
      holdTapsPassing()
      if (by === 'touch') {
        const stamp = Date.now() / 1000
        await tapAt(...point, stamp)
        await tapAt(...button, stamp + 0.12)
      } else {
        mouse.clock(0)
        await mouse.moveTo(...point)
        await mouse.press()
        mouse.clock(20)
        await mouse.release()
        mouse.clock(120)
        await mouse.moveTo(...button)
        await mouse.press()
        mouse.clock(140)
        await mouse.release()
        mouse.clock()
      }
      await wait(400)
      expect(document.querySelector('.reader-dialog')).not.toBeNull()
      expect(host.querySelector('.book-peek')).toBeNull()
    },
  )

  it('leaves the scrubber to a second tap that lands away from the first', async () => {
    const { host, text } = await openCats(FLOWS[0], SET_IN)
    await tapThenWait(6, 350)
    expect(barsShown(host)).toBe(true)
    const point = inMiddle(text, 300, 120)
    const line = (
      host.querySelector('.book-scrubber') as Element
    ).getBoundingClientRect()
    const seek = across(line, 0.25)
    expect(document.elementFromPoint(...seek)?.closest('.book-scrubber')).toBe(
      host.querySelector('.book-scrubber'),
    )
    const before = text.scrollTop
    const stamp = Date.now() / 1000
    await tapAt(...point, stamp)
    await tapAt(...seek, stamp + 0.12)
    await wait(400)
    expect(text.scrollTop - before).toBeGreaterThan(500)
    expect(lookUpTitle()).toBe('猫')
    expect(host.querySelector('.book-peek')).toBeNull()
  })

  it('takes a second tap on the panel near the first as the second of a double tap', async () => {
    const { host, text } = await openCats(FLOWS[0], SET_IN)
    await tapThenWait(...inMiddle(text, 300, 120))
    await wait(100)
    const { left: edge, top, height } = panelBox()
    await closeLookUp()
    await wait(400)
    const [x, y] = characterIn(
      text,
      new DOMRect(edge - 14, top + 6, 12, height - 12),
    )
    const tapped = offsetAtPoint(text, x, y)?.block
    expect(tapped).toBeDefined()
    const stamp = Date.now() / 1000
    await tapAt(x, y, stamp)
    // The stamps say a double tap, and by now the panel takes presses itself.
    await wait(450)
    const near: [number, number] = [edge + 6, y]
    expect(document.elementFromPoint(...near)?.closest(LOOK_UP)).not.toBeNull()
    await tapAt(...near, stamp + 0.12)
    await wait(400)
    expect(lookUpTitle()).toBeUndefined()
    expect(host.querySelector('.book-peek')?.textContent).toBe(
      `Cat number ${tapped}.`,
    )
    expect(host.querySelector('[data-note]')).toBeNull()
  })

  it.each(['mouse', 'touch'] as const)(
    'lets a %s press Cancel on the pinning bar where it lies over the look-up',
    async (by) => {
      const { host, text } = await openCats(FLOWS[0], [1024, 768], true)
      await tapThenWait(6, 350)
      await tapThenWait(...inMiddle(text, 300, 120))
      await wait(150)
      const compare = [...host.querySelectorAll('.reader-layout button')].find(
        (b) => /compar|side|column/i.test(b.textContent ?? ''),
      ) as HTMLElement
      await tapThenWait(...across(compare.getBoundingClientRect(), 0.5))
      expect(lookUpTitle()).toBe('猫')
      act(() => {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'c', bubbles: true }),
        )
      })
      await wait(150)
      const cancel = [...host.querySelectorAll('.book-pinning button')].find(
        (b) => b.textContent === 'Cancel',
      ) as HTMLElement
      const [x, y] = across(cancel.getBoundingClientRect(), 0.5)
      const box = panelBox()
      expect(x).toBeGreaterThan(box.left)
      expect(x).toBeLessThan(box.right)
      expect(y).toBeGreaterThan(box.top)
      expect(y).toBeLessThan(box.bottom)
      if (by === 'touch') await tapAt(x, y)
      else {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.release()
      }
      await wait(250)
      expect(host.querySelector('.book-pinning')).toBeNull()
    },
  )

  it('leaves the look-up open for a tap beside it once a double tap is over', async () => {
    const { host, text, asked } = await openCats(FLOWS[0], SET_IN)
    await tapThenWait(...inMiddle(text, 300, 120))
    await wait(100)
    const beside: [number, number] = [
      window.innerWidth - 100,
      window.innerHeight - 30,
    ]
    expect(document.elementFromPoint(...beside)).toBe(
      host.querySelector('.book-reader'),
    )
    await tapThenWait(...beside)
    expect(lookUpTitle()).toBe('猫')
    expect(host.querySelector('.book-peek')).toBeNull()
    expect(asked).toHaveLength(1)
  })

  it('takes a drag straight after the tap as no double tap', async () => {
    const flow = FLOWS[0]
    const { host, text } = await openCats(flow, SET_IN)
    const [x, y] = underThePanel(text, flow, SET_IN[0])
    holdTapsPassing()
    mouse.clock(0)
    await mouse.moveTo(x, y)
    await mouse.press()
    mouse.clock(20)
    await mouse.release()
    mouse.clock(120)
    await mouse.press()
    mouse.clock(130)
    await mouse.moveTo(x - 40, y)
    mouse.clock(140)
    await mouse.release()
    mouse.clock()
    await wait(400)
    expect(host.querySelector('.book-peek')).toBeNull()
  })

  describe.each(FLOWS)('$flow', (flow) => {
    it.each([
      ['pressed in time and lifted too late', 0.25, 0.33, true],
      [
        'pressed too late while the panel still lets it through',
        0.31,
        0.33,
        false,
      ],
    ])(
      'asks nothing about the text under the panel for a second tap %s',
      async (_, down, up, expires) => {
        const { host, text, asked } = await openCats(flow)
        const point = await pointOnPanel(text, flow)
        const before = asked.length
        const expire = holdTapsPassing()
        const stamp = Date.now() / 1000
        await tapAt(...firstTap(text), stamp)
        // The stamps below are then in the past, and the answer is drawn.
        await wait(450)
        await touchDown(...point, stamp + down)
        if (expires) expire()
        await touchUp(stamp + up)
        await wait(400)
        expect(asked.length - before).toBe(1)
        expect(lookUpTitle()).toBe('猫')
        expect(host.querySelector('.book-peek')).toBeNull()
        expect(host.querySelector('[data-note]')).toBeNull()
      },
    )

    it('takes a double tap as one, peeking and writing no note', async () => {
      const { host, text } = await openCats(flow)
      const point = await pointOnPanel(text, flow)
      const tapped = offsetAtPoint(text, ...point)?.block
      expect(tapped).toBeDefined()
      holdTapsPassing()
      const stamp = Date.now() / 1000
      await tapAt(...point, stamp)
      await tapAt(...point, stamp + 0.12)
      await wait(400)
      expect(host.querySelector('[data-note]')).toBeNull()
      expect(lookUpTitle()).toBeUndefined()
      expect(host.querySelector('.book-peek')?.textContent).toBe(
        `Cat number ${tapped}.`,
      )
    })

    it('presses the panel for a tap once a double tap is over', async () => {
      const { host, text } = await openCats(flow)
      await tapThenWait(...inMiddle(text, 500, 120))
      await wait(100)
      await tapThenWait(...across(saveWord().getBoundingClientRect(), 0.5))
      await wait(100)
      expect(lookUpTitle()).toBeUndefined()
      expect(host.querySelector('[data-note]')).not.toBeNull()
    })
  })
})

describe('the selection bar with the look-up mode off', () => {
  it('still goes when closing a panel puts the place back and scrolls the text', async () => {
    const flow = FLOWS[0]
    const { host, text } = await openFlow(flow, SET_IN, { lookUp: false })
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 't', bubbles: true }),
      )
    })
    await wait(150)
    const contents = () =>
      document.querySelector('[role="dialog"][aria-label="Contents"]')
    expect(contents()).not.toBeNull()
    const spot = best(spotsInSight(host, text, false), (box) => -box.top)
    await selectSpot(host, text, spot)
    expect(host.querySelector('.book-selection-bar')).not.toBeNull()
    const before = scrolledTo(text)
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 't', bubbles: true }),
      )
    })
    await wait(300)
    expect(contents()).toBeNull()
    expect(scrolledTo(text)).not.toEqual(before)
    expect(host.querySelector('.book-selection-bar')).toBeNull()
  })

  it('stays where it was drawn when closing a panel moves the words and scrolls nothing', async () => {
    const flow = FLOWS[0]
    const { host, text } = await openFlow(flow, SET_IN, { lookUp: false })
    act(() => {
      text.scrollTo({ top: 0 })
    })
    await wait(150)
    const toggleContents = () =>
      act(() => {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { key: 't', bubbles: true }),
        )
      })
    toggleContents()
    await wait(150)
    const spot = best(spotsInSight(host, text, false), (box) => -box.top)
    await selectSpot(host, text, spot)
    const bar = () =>
      host.querySelector('.book-selection-bar')?.getBoundingClientRect()
    const drawn = bar()?.left
    expect(drawn).toBeDefined()
    const words = wordBox(host, spot.block, spot.at).left
    toggleContents()
    await wait(300)
    expect(scrolledTo(text)[1]).toBe(0)
    expect(wordBox(host, spot.block, spot.at).left).not.toBe(words)
    expect(bar()?.left).toBe(drawn)
  })
})

describe('the selection bar, after a hold with the look-up open', () => {
  it.each([
    ['1440 by 900, horizontal', [1440, 900], FLOWS[0]],
    ['1000 by 700, horizontal', SET_IN, FLOWS[0]],
    ['1440 by 900, vertical', [1440, 900], FLOWS[2]],
    ['1000 by 700, vertical', SET_IN, FLOWS[2]],
  ] as const)('is beside the selection at %s', async (_, size, flow) => {
    const { host, text } = await openFlow(flow, size)
    await tapSpot(await spotToLose(host, text, flow, size), host)
    await wait(150)
    const screen = text.getBoundingClientRect()
    // Near the start of its line, where the bar has the window's room to its
    // right and the window does not push it off the selection.
    const spot = best(
      spotsInSight(host, text, false),
      (box) =>
        -Math.abs(box.top - (screen.top + screen.height / 2)) -
        Math.abs(box.left - (screen.left + screen.width / 5)),
    )
    await selectSpot(host, text, spot, 2)
    await wait(300)
    expect(lookUpTitle()).toBeUndefined()
    const picked = window.getSelection()?.getRangeAt(0).getBoundingClientRect()
    const bar = host
      .querySelector('.book-selection-bar')
      ?.getBoundingClientRect()
    expect(bar).toBeDefined()
    if (flow.vertical) {
      expect(
        Math.abs((bar as DOMRect).top - (picked as DOMRect).top),
      ).toBeLessThan(1)
      expect((bar as DOMRect).right).toBeLessThanOrEqual(
        (picked as DOMRect).left,
      )
      expect((picked as DOMRect).left - (bar as DOMRect).right).toBeLessThan(24)
    } else {
      expect(
        Math.abs((bar as DOMRect).left - (picked as DOMRect).left),
      ).toBeLessThan(1)
      expect((bar as DOMRect).top).toBeGreaterThanOrEqual(
        (picked as DOMRect).bottom,
      )
      expect((bar as DOMRect).top - (picked as DOMRect).bottom).toBeLessThan(24)
    }
  })
})

describe('an answer that lands after the reader has moved on', () => {
  const SPANISH =
    'La casa grande estaba cerca del río y los niños jugaban en el jardín todas las tardes de verano. '.repeat(
      4,
    )
  const edition = (label: string, language: string, text: string) => ({
    label,
    language,
    blocks: Array.from({ length: 30 }, () => block(text)),
  })
  const spanish = {
    title: 'Prueba',
    hub: 'es',
    editions: { es: edition('Español', 'es', SPANISH) },
    alignments: [],
  }
  const japanese = {
    title: '坊っちゃん',
    hub: 'ja',
    editions: { ja: edition('日本語', 'ja', OPENING.repeat(4)) },
    alignments: [],
  }
  const entry = { word: 'casa', pos: 'noun', glosses: ['house'] }

  const ANSWERS = {
    found: { words: [entry], length: 2, status: 200, says: 'house' },
    'not in the dictionary': {
      words: [],
      length: 0,
      status: 200,
      says: 'Not in the dictionary.',
    },
    'no dictionary': {
      words: [],
      length: 0,
      status: 404,
      says: 'This server has no dictionary',
    },
    failed: { words: [], length: 0, status: 500, says: 'looking up' },
  }
  type Answer = keyof typeof ANSWERS

  const CASES = [
    { name: 'mode off, scrolled', by: 'bar', drawn: spanish, paged: false },
    { name: 'mode off, paged', by: 'bar', drawn: spanish, paged: true },
    {
      name: 'a Spanish tap, scrolled',
      by: 'tap',
      drawn: spanish,
      paged: false,
    },
    { name: 'a Spanish tap, paged', by: 'tap', drawn: spanish, paged: true },
    {
      name: 'a Japanese tap, scrolled',
      by: 'tap',
      drawn: japanese,
      paged: false,
    },
    { name: 'a Japanese tap, paged', by: 'tap', drawn: japanese, paged: true },
  ] as const
  type Case = (typeof CASES)[number]

  /** A letter drawn mid-screen, in the middle third a paged text looks up from. */
  const letterInMiddle = (host: HTMLElement, text: HTMLElement) => {
    const screen = text.getBoundingClientRect()
    for (const held of host.querySelectorAll<HTMLElement>('[data-block]')) {
      const n = Number(held.dataset.block)
      const words = held.querySelector('[data-text]')?.textContent ?? ''
      for (let at = 0; at < words.length - 1; at++) {
        if (!/\p{L}{2}/u.test(words.slice(at, at + 2))) continue
        const box = characterBox(host, n, at)
        const middle = (box.left + box.width / 2 - screen.left) / screen.width
        if (
          middle > 0.42 &&
          middle < 0.58 &&
          box.top > screen.top + screen.height * 0.3 &&
          box.bottom < screen.top + screen.height * 0.7
        ) {
          return { block: n, at }
        }
      }
    }
    throw new Error('no letter is drawn in the middle of the screen')
  }

  /** The reader on `drawn`, a look-up asked by `by`, its answer held back. */
  const asked = async (
    { by, drawn, paged }: Case,
    answer: Answer = 'found',
  ) => {
    await windowOf(1280, 800)
    const { words, length, status } = ANSWERS[answer]
    let land = () => {}
    const held = new Promise<object[]>((done) => {
      land = () => done(words)
    })
    const { host } = await openReader(drawn, {
      lookUp: by === 'tap',
      prefs: { 'book-layout': { mode: 'scroll', paged } },
      found: { length, entries: words },
      words: () => held,
      status,
    })
    const text = host.querySelector('.book-text') as HTMLElement
    const spot = letterInMiddle(host, text)
    if (by === 'tap') {
      await tapThenWait(...across(characterBox(host, spot.block, spot.at), 0.5))
    } else {
      await selectSpot(host, text, spot, 2)
      await pressLookUp(host)
    }
    expect(document.querySelector(LOOK_UP)?.textContent).toContain(
      'Looking it up…',
    )
    return { host, text, land }
  }

  /** The reader's own move on: a page turned by its key, or a scroll. */
  const moveOn = async (text: HTMLElement, paged: boolean) => {
    const from = scrolledTo(text)
    act(() => {
      if (paged) {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }),
        )
      } else text.scrollBy({ top: 1500 })
    })
    await wait(300)
    expect(scrolledTo(text)).not.toEqual(from)
    return scrolledTo(text)
  }

  const landed = async (land: () => void, answer: Answer = 'found') => {
    await act(async () => land())
    await wait(300)
    const panel = document.querySelector(LOOK_UP)
    expect(panel?.textContent).not.toContain('Looking it up…')
    expect(panel?.textContent).toContain(ANSWERS[answer].says)
  }

  it.each(CASES)('stays where the reader went: $name', async (shape) => {
    const { text, land } = await asked(shape)
    const movedTo = await moveOn(text, shape.paged)
    await landed(land)
    expect(scrolledTo(text)).toEqual(movedTo)
  })

  describe.each(['not in the dictionary', 'no dictionary', 'failed'] as const)(
    'with the answer %s',
    (answer) => {
      // A Japanese answer naming no word lights nothing to bring back.
      it.each([CASES[0], CASES[2]])('stays: $name', async (shape) => {
        const { text, land } = await asked(shape, answer)
        const movedTo = await moveOn(text, shape.paged)
        await landed(land, answer)
        expect(scrolledTo(text)).toEqual(movedTo)
      })
    },
  )

  it('stays where the reader went when the window has laid the text out since', async () => {
    const { text, land } = await asked(CASES[2])
    await moveOn(text, false)
    await page.viewport(1200, 800)
    await wait(300)
    const settled = scrolledTo(text)
    await landed(land)
    expect(scrolledTo(text)).toEqual(settled)
  })

  it('stays where the scrubber took the reader', async () => {
    await windowOf(1280, 800)
    let land = () => {}
    const held = new Promise<object[]>((done) => {
      land = () => done([entry])
    })
    const { host } = await openReader(spanish, { words: () => held })
    const text = host.querySelector('.book-text') as HTMLElement
    await tapThenWait(6, 350)
    expect(barsShown(host)).toBe(true)
    const spot = letterInMiddle(host, text)
    await tapThenWait(...across(characterBox(host, spot.block, spot.at), 0.5))
    const line = (
      host.querySelector('.book-scrubber') as Element
    ).getBoundingClientRect()
    await mouse.moveTo(...across(line, 0.3))
    await mouse.press()
    await mouse.release()
    await wait(300)
    const movedTo = scrolledTo(text)
    expect(movedTo[1]).toBeGreaterThan(1000)
    await landed(land)
    expect(scrolledTo(text)).toEqual(movedTo)
  })

  it('stays when the answer lands before the move has sent its scroll', async () => {
    const { text, land } = await asked(CASES[2])
    const unsent = (event: Event) => event.stopImmediatePropagation()
    window.addEventListener('scroll', unsent, true)
    onTestFinished(() => window.removeEventListener('scroll', unsent, true))
    text.scrollBy({ top: 1500 })
    const movedTo = scrolledTo(text)
    await landed(land)
    expect(scrolledTo(text)).toEqual(movedTo)
  })
})

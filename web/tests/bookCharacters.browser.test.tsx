import { act } from 'react'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import type { CharacterCard } from '../src/book'
import { BookReader } from '../src/BookReader'
import '../src/index.css'
import { deferred, flush, settleUntil } from './support/async'
import { ADA, MARA, NED, naming } from './support/characters'
import { channels, tokenColour } from './support/colour'
import { apart, drawn, pixelAt } from './support/drawn'
import { mount } from './support/mount'
import { mouse } from './support/mouse'
import { pointerRulesAs } from './support/pointer'
import { swipeBetween, tapAt } from './support/touch'

vi.mock('../src/Account', () => ({ Account: () => null }))

/**
 * A character's card in the look-up's panel, against Chromium's own layout,
 * its real pointers and its highlight registry, which jsdom has none of:
 * where the card is drawn and when, what a press on a fact does, what the
 * text lights and marks while the card shows, and the look the stylesheet
 * gives each part.
 */

const para = (text: string) => ({
  text,
  heading: false,
  sentences: [[0, text.length]],
  marks: [],
})

/** Long enough that a block of it is several lines at every width here. */
const FILLER =
  'The wind came over the hill and the lamp burned low in its glass. '
    .repeat(6)
    .trim()
const fillers = (count: number) =>
  Array.from({ length: count }, () => para(FILLER))

/** The names the cards below are for, among words that name no one. */
const LINES = [
  'Ned Reed lit the lamp at dusk, and Ada Reed watched him from the stair.',
  'Mara Vell came up the hill that night, and Ned went down to meet her.',
  'N-N-Ned Reed, she said, the Reed house is dark, and Mara laughed.',
]
/** The block, screens below the first, that names Ned again. */
const FAR = LINES.length + 14

const keepers = {
  title: 'The Lamp Keepers',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      characters: true,
      blocks: [
        ...LINES.map(para),
        ...fillers(14),
        para('Far along the shore Ned trimmed the wick once more.'),
        ...fillers(14),
      ],
    },
  },
  alignments: [],
}

/** The name three times in every block, so one lies wherever on the screen a case needs it. */
const NIGHT =
  'The wind came over the hill and Ned Reed kept the lamp burning low in its glass through the night. '
    .repeat(3)
    .trim()
const nights = {
  ...keepers,
  editions: {
    en: {
      ...keepers.editions.en,
      blocks: Array.from({ length: 40 }, () => para(NIGHT)),
    },
  },
}

/** The same book with a second edition beside it, block for block, to compare. */
const twoEditions = {
  ...keepers,
  editions: {
    ...keepers.editions,
    plain: {
      label: 'Plain',
      language: 'en',
      blocks: keepers.editions.en.blocks.map((_, n) =>
        para(`Line ${n} of the plain telling.`),
      ),
    },
  },
  alignments: [
    {
      to: 'plain',
      pins: [],
      links: keepers.editions.en.blocks.map((_, n) => [n, n + 1, n, n + 1]),
      sentences: keepers.editions.en.blocks.map((_, n) => [n, n + 1, n, n + 1]),
    },
  ],
}

/** What the characters route answers: its cards, and where the name lies. */
type Named = { cards: CharacterCard[]; start?: number; length?: number }

const by = (
  card: CharacterCard,
  text: string,
  part: 'given' | 'surname',
): CharacterCard => ({ ...card, matched: { text, part } })

/** What the stubbed characters route answers for the text from a tap. */
function cast(text: string): Named {
  if (text.startsWith('Ned Reed')) return { cards: [NED], length: 8 }
  if (text.startsWith('Reed lit')) return { cards: [NED], start: -4, length: 8 }
  if (text.startsWith('N-N-Ned')) return { cards: [NED], start: 4, length: 8 }
  if (text.startsWith('Ned '))
    return { cards: [by(NED, 'Ned', 'given')], length: 3 }
  if (text.startsWith('Mara Vell')) return { cards: [MARA], length: 9 }
  if (text.startsWith('Reed house')) {
    return {
      cards: [by(NED, 'Reed', 'surname'), by(ADA, 'Reed', 'surname')],
      length: 4,
    }
  }
  return { cards: [] }
}

const sense = (n: number) => ({
  word: 'lamp',
  pos: 'noun',
  glosses: [`a light kept burning, sense ${n}`],
})

const DESK = [1366, 768] as const
/** Beside a docked panel, where the set-in takes the most lines' room. */
const NARROW = [900, 700] as const
const PHONE = [390, 844] as const
/** The share of the screen's height the panel is held under at phone size. */
const CAP = 0.45

const wait = (ms: number) =>
  act(() => new Promise<void>((done) => setTimeout(done, ms)))

async function windowOf(width: number, height: number) {
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, height)
  await wait(120)
}

/**
 * The book reader over the whole screen on `drawn`, the look-up mode on, in
 * a window `size`. The characters route answers `who` for the text asked
 * with, and the dictionary `entries` for every word.
 */
async function openBook({
  size = DESK,
  drawn: book = keepers,
  prefs = {},
  who = cast,
  entries = [sense(1)],
}: {
  size?: readonly [number, number]
  drawn?: object
  prefs?: Record<string, object>
  who?: (text: string) => Named | Promise<Named>
  entries?: object[]
} = {}) {
  await windowOf(...size)
  const stored: Record<string, object> = { lookup: { on: true }, ...prefs }
  for (const [id, value] of Object.entries(stored)) {
    localStorage.setItem(`taiyaku.prefs.${id}`, JSON.stringify(value))
  }
  onTestFinished(() => {
    for (const id of Object.keys(stored)) {
      localStorage.removeItem(`taiyaku.prefs.${id}`)
    }
    vi.unstubAllGlobals()
  })
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(book)
      if (url === '/api/works/w/notes') {
        return Response.json(
          { work: 'w', notes: [] },
          { headers: { etag: '"n"' } },
        )
      }
      if (url.includes('/book/characters?')) {
        const asked = new URL(url, location.href).searchParams.get('text')
        const { cards, ...name } = await who(asked ?? '')
        return Response.json({ ...naming(...cards), ...name })
      }
      if (url.startsWith('/api/dictionary/')) return Response.json(entries)
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [] })
    }),
  )
  const host = mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  await settleUntil(
    'the book',
    () => host.querySelector('[data-block] [data-text]') !== null,
    5000,
  )
  await wait(150)
  return { host, text: host.querySelector('.book-text') as HTMLElement }
}

const POINTERS = ['mouse', 'touch'] as const
type Pointer = (typeof POINTERS)[number]

/** Real inputs land outside `act`, as a reader's do. */
async function outsideAct(send: () => Promise<void>) {
  const global = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
  global.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await send()
  } finally {
    global.IS_REACT_ACT_ENVIRONMENT = true
  }
}

const moveTo = (x: number, y: number) => outsideAct(() => mouse.moveTo(x, y))

/**
 * A real click or a real tap at a point, then long enough for what it asks
 * to be answered, and for the next press not to be its second.
 */
async function press(pointer: Pointer, at: readonly [number, number]) {
  await pressNow(pointer, at)
  await wait(350)
}

/** The press of `press`, for a case about what is drawn straight after it. */
async function pressNow(pointer: Pointer, [x, y]: readonly [number, number]) {
  if (pointer === 'touch') await tapAt(x, y)
  else {
    await outsideAct(async () => {
      await mouse.moveTo(x, y)
      await mouse.press()
      await mouse.release()
    })
  }
}

const middle = (box: DOMRect): [number, number] => [
  box.left + box.width / 2,
  box.top + box.height / 2,
]
const middleOf = (el: Element) => middle(el.getBoundingClientRect())

const ownText = (host: HTMLElement, n: number) =>
  host.querySelector(`[data-block="${n}"] > [data-text]`) as HTMLElement

/** A range over block `n`'s words from `start` up to `end`, whatever nodes they lie in. */
function rangeOver(host: HTMLElement, n: number, start: number, end: number) {
  const walker = document.createTreeWalker(
    ownText(host, n),
    NodeFilter.SHOW_TEXT,
  )
  const range = document.createRange()
  let passed = 0
  let begun = false
  for (
    let node = walker.nextNode() as Text | null;
    node;
    node = walker.nextNode() as Text | null
  ) {
    const next = passed + node.length
    if (!begun && start < next) {
      range.setStart(node, start - passed)
      begun = true
    }
    if (begun && end <= next) {
      range.setEnd(node, end - passed)
      return range
    }
    passed = next
  }
  throw new Error(`block ${n} has no words from ${start} to ${end}`)
}

/** A range over the `nth` place block `n` writes `words`. */
function wordsIn(host: HTMLElement, n: number, words: string, nth = 0) {
  const text = ownText(host, n).textContent ?? ''
  let at = -1
  for (let i = 0; i <= nth; i++) {
    at = text.indexOf(words, at + 1)
    if (at < 0) throw new Error(`block ${n} does not write ${words}`)
  }
  return rangeOver(host, n, at, at + words.length)
}

/** A real press on the middle of the first line `words` are on. */
const pressWords = (
  pointer: Pointer,
  host: HTMLElement,
  n: number,
  words: string,
  nth = 0,
) => press(pointer, middle(wordsIn(host, n, words, nth).getClientRects()[0]))

const LOOK_UP = '[role="dialog"][aria-label="Look up"]'
const panelEl = () => document.querySelector<HTMLElement>(LOOK_UP)
const inPanel = <E extends Element = HTMLElement>(selector: string) => [
  ...(panelEl()?.querySelectorAll<E>(selector) ?? []),
]
const cardEls = () => inPanel('.book-lookup-character')
const entryEl = () => inPanel('.book-lookup-entry').at(0)
const rowEls = () => inPanel('.book-lookup-fact-row')
const noteOf = (row: Element) =>
  row.querySelector('.book-lookup-fact-note') as HTMLElement
const opened = (row: Element) => row.getAttribute('aria-expanded')
const moreOf = (row: Element) =>
  row.parentElement?.querySelector<HTMLElement>('.book-lookup-more') ?? null

/** Waits for the cards and the dictionary's entry, which are drawn together. */
async function cardsDrawn(count = 1) {
  await settleUntil(
    'the cards and the entry',
    () => cardEls().length === count && entryEl() !== undefined,
    4000,
  )
}

/** A key the reader hears from outside its panels, wherever the focus is. */
function key(name: string) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: name,
        bubbles: true,
        cancelable: true,
      }),
    )
  })
}

/** Escape, which closes the look-up. */
async function closeLookUp() {
  key('Escape')
  await wait(50)
}

const selected = () => window.getSelection()?.toString() ?? ''

/** The ranges the registry holds under `name`. */
function ranges(name: string) {
  const out: Range[] = []
  CSS.highlights.get(name)?.forEach((range) => {
    out.push(range as Range)
  })
  return out
}

/** What `name` marks, each as its block and the words marked there. */
const marks = (name: string) =>
  ranges(name).map((range) => {
    const block =
      range.startContainer.parentElement?.closest<HTMLElement>('[data-block]')
    return `${block?.dataset.block}:${range.toString()}`
  })

const scrolledTo = (text: HTMLElement) => [text.scrollLeft, text.scrollTop]

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
    clearOfPanel: !meets(
      box,
      (panelEl() as HTMLElement).getBoundingClientRect(),
    ),
  }
}

/** Whether `box` is inside the panel's box, as far as the panel shows. */
function shownInPanel(box: DOMRect) {
  const panel = (panelEl() as HTMLElement).getBoundingClientRect()
  return (
    box.top >= panel.top - 0.5 &&
    box.bottom <= panel.bottom + 0.5 &&
    box.left >= panel.left - 0.5 &&
    box.right <= panel.right + 0.5
  )
}

/**
 * What each frame draws from here on, read as the frame begins: `read`'s
 * answer for it. `stop` ends the watch.
 */
function eachFrame<T>(read: () => T) {
  const seen: T[] = []
  let on = true
  const tick = () => {
    if (!on) return
    seen.push(read())
    requestAnimationFrame(tick)
  }
  requestAnimationFrame(tick)
  return {
    seen,
    stop: () => {
      on = false
    },
  }
}

/**
 * The ground drawn behind the first of `words` in block `n`: a pixel
 * in the corner of its line's box, above and before the glyph's ink.
 */
async function groundBehind(host: HTMLElement, n: number, words: string) {
  const block = host.querySelector(`[data-block="${n}"]`) as HTMLElement
  const image = await drawn(block)
  const box = block.getBoundingClientRect()
  const word = wordsIn(host, n, words).getClientRects()[0]
  const scale = image.width / box.width
  return pixelAt(
    image,
    Math.round((word.left - box.left + 1.5) * scale),
    Math.round((word.top - box.top + 1.5) * scale),
  )
}

describe.each(POINTERS)('a character’s card, by a real %s', (pointer) => {
  it('is drawn above the entry for a press on its name', async () => {
    const { host } = await openBook()
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    const [card] = cardEls()
    expect(card.querySelector('h3')?.textContent).toBe('Ned Reed')
    const box = card.getBoundingClientRect()
    expect(box.height).toBeGreaterThan(0)
    expect(shownInPanel(box)).toBe(true)
    expect(box.bottom).toBeLessThanOrEqual(
      (entryEl() as HTMLElement).getBoundingClientRect().top + 0.5,
    )
  })

  it('moves nothing drawn when the cards come in time', async () => {
    const cards = deferred<void>()
    const { host } = await openBook({
      who: async (text) => {
        await cards.promise
        return cast(text)
      },
    })
    const frames = eachFrame(() => ({
      panel: panelEl() !== null,
      card: cardEls().length > 0,
      entry: entryEl()?.getBoundingClientRect().top,
    }))
    await pressNow(pointer, middle(wordsIn(host, 0, 'Ned').getClientRects()[0]))
    // Answered as soon as the panel is up, well inside the wait.
    await settleUntil('the panel', () => panelEl() !== null, 4000)
    await flush(() => cards.resolve())
    await cardsDrawn()
    await wait(120)
    frames.stop()
    const first = frames.seen.findIndex((frame) => frame.entry !== undefined)
    // The dictionary had answered at once: the frames before are the hold.
    expect(
      frames.seen.slice(0, first).filter((frame) => frame.panel).length,
    ).toBeGreaterThan(0)
    expect(frames.seen[first].card).toBe(true)
    expect(frames.seen[first + 2].entry).toBe(frames.seen[first].entry)
    expect(frames.seen.at(-1)?.entry).toBe(frames.seen[first].entry)
  })

  it('moves nothing, and draws no card, for cards that come after the wait', async () => {
    const cards = deferred<void>()
    const { host } = await openBook({
      who: async (text) => {
        await cards.promise
        return cast(text)
      },
    })
    await pressWords(pointer, host, 0, 'Ned')
    await settleUntil('the entry', () => entryEl() !== undefined, 4000)
    expect(cardEls()).toHaveLength(0)
    const top = (entryEl() as HTMLElement).getBoundingClientRect().top
    await flush(() => cards.resolve())
    await wait(250)
    expect(cardEls()).toHaveLength(0)
    expect((entryEl() as HTMLElement).getBoundingClientRect().top).toBe(top)
    expect(marks('book-lookup-word')).toEqual(['0:Ned'])
    expect(marks('book-mention')).toEqual([])
  })

  it('opens a fact for a press on its words and on the empty end of its row, and closes it for another', async () => {
    const { host, text } = await openBook()
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    const row = rowEls()[1]
    const words = middleOf(noteOf(row))
    const box = row.getBoundingClientRect()
    const end: [number, number] = [box.right - 12, box.top + box.height / 2]
    // The row runs on well past its words, and nothing else is there.
    expect(end[0] - noteOf(row).getBoundingClientRect().right).toBeGreaterThan(
      40,
    )
    expect(opened(row)).toBe('false')
    await press(pointer, words)
    expect(opened(row)).toBe('true')
    await press(pointer, end)
    expect(opened(row)).toBe('false')
    expect(moreOf(row)).toBeNull()
    await press(pointer, end)
    expect(opened(row)).toBe('true')
    expect(opened(rowEls()[0])).toBe('false')
    // What opened is where the reader can see it, by the panel's scroll alone.
    const book = scrolledTo(text)
    const more = moreOf(row) as HTMLElement
    expect(more.textContent).toContain('Tom Field showed Ned Reed the wick.')
    act(() => {
      more.scrollIntoView({ block: 'center' })
    })
    expect(shownInPanel(more.getBoundingClientRect())).toBe(true)
    expect(scrolledTo(text)).toEqual(book)
    await press(pointer, words)
    expect(opened(row)).toBe('false')
  })

  it.each([
    ['back over the word before the one pressed', 0, 'Reed', 'Ned Reed', cast],
    ['on from the word pressed', 0, 'Ned', 'Ned Reed', cast],
    ['over the stutter a name begins after', 2, 'Ned', 'N-N-Ned Reed', cast],
    [
      'over no less than the word pressed',
      1,
      'Mara',
      'Mara',
      (): Named => ({ cards: [MARA], length: 2 }),
    ],
  ])('lights the whole name, %s', async (_, n, pressed, name, who) => {
    const { host } = await openBook({ who })
    await pressWords(pointer, host, n, pressed)
    await cardsDrawn()
    expect(selected()).toBe(name)
    expect(marks('book-lookup-word')).toEqual([`${n}:${name}`])
    const lit = window.getSelection()?.getRangeAt(0).getBoundingClientRect()
    const glyphs = wordsIn(host, n, name).getBoundingClientRect()
    expect(lit?.left).toBeCloseTo(glyphs.left, 0)
    expect(lit?.right).toBeCloseTo(glyphs.right, 0)
    expect(glyphs.width).toBeGreaterThan(
      wordsIn(host, n, pressed).getBoundingClientRect().width - 0.5,
    )
  })

  it('keeps the name lit through a press in the panel, until the panel closes or another word is pressed', async () => {
    const { host } = await openBook()
    const block = host.querySelector('[data-block="0"]') as HTMLElement
    const accent = channels(tokenColour(block, '--accent'))
    const plain = await groundBehind(host, 0, 'Ned Reed')
    expect(apart(plain, accent)).toBeGreaterThan(60)
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    expect(marks('book-lookup-word')).toEqual(['0:Ned Reed'])
    await press(pointer, middleOf(noteOf(rowEls()[1])))
    expect(opened(rowEls()[1])).toBe('true')
    expect(marks('book-lookup-word')).toEqual(['0:Ned Reed'])
    // Whatever the press left selected, the name is lit without it.
    act(() => window.getSelection()?.removeAllRanges())
    await wait(50)
    expect(selected()).toBe('')
    expect(marks('book-lookup-word')).toEqual(['0:Ned Reed'])
    expect(apart(await groundBehind(host, 0, 'Ned Reed'), accent)).toBeLessThan(
      12,
    )
    await pressWords(pointer, host, 1, 'Mara')
    await cardsDrawn()
    expect(marks('book-lookup-word')).toEqual(['1:Mara Vell'])
    await closeLookUp()
    expect(panelEl()).toBeNull()
    expect(marks('book-lookup-word')).toEqual([])
    expect(apart(await groundBehind(host, 0, 'Ned Reed'), plain)).toBeLessThan(
      12,
    )
  })

  it('marks the other places near the screen that name the card’s character, and no one else’s', async () => {
    const { host } = await openBook()
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    expect(marks('book-mention')).toEqual(['1:Ned', '2:Ned Reed'])
    await pressWords(pointer, host, 1, 'Mara')
    await cardsDrawn()
    expect(cardEls()[0].querySelector('h3')?.textContent).toBe('Mara Vell')
    expect(marks('book-mention')).toEqual(['2:Mara'])
    // A surname two of the cast bear says no one.
    await pressWords(pointer, host, 2, 'Reed', 1)
    await cardsDrawn(2)
    expect(marks('book-lookup-word')).toEqual(['2:Reed'])
    expect(marks('book-mention')).toEqual([])
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    expect(marks('book-mention')).toEqual(['1:Ned', '2:Ned Reed'])
    await pressWords(pointer, host, 0, 'lamp')
    await settleUntil(
      'the entry alone',
      () => entryEl() !== undefined && cardEls().length === 0,
      4000,
    )
    expect(marks('book-lookup-word')).toEqual(['0:lamp'])
    expect(marks('book-mention')).toEqual([])
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    expect(marks('book-mention')).toEqual(['1:Ned', '2:Ned Reed'])
    await closeLookUp()
    expect(marks('book-mention')).toEqual([])
  })

  it('has a mention marked by the frame a scroll shows it', async () => {
    const { host, text } = await openBook()
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    const far = host.querySelector(`[data-block="${FAR}"]`) as HTMLElement
    const shown = () =>
      meets(far.getBoundingClientRect(), text.getBoundingClientRect())
    const mark = `${FAR}:Ned`
    expect(shown()).toBe(false)
    expect(marks('book-mention')).not.toContain(mark)
    const frames = eachFrame(() => ({
      shown: shown(),
      marked: marks('book-mention').includes(mark),
    }))
    const box = text.getBoundingClientRect()
    const x = box.left + box.width / 2
    for (let n = 0; n < 40 && !shown(); n++) {
      if (pointer === 'touch') {
        await swipeBetween([x, box.bottom - 80], [x, box.top + 80])
      } else {
        await outsideAct(async () => {
          await mouse.moveTo(x, box.top + box.height / 2)
          await mouse.wheel(300)
        })
      }
      await wait(60)
    }
    await wait(200)
    frames.stop()
    expect(shown()).toBe(true)
    expect(panelEl()).not.toBeNull()
    expect(frames.seen.filter((frame) => frame.shown).length).toBeGreaterThan(0)
    expect(frames.seen.filter((frame) => frame.shown && !frame.marked)).toEqual(
      [],
    )
    // The lamp's first lighting is more than a screen back, and let go.
    expect(marks('book-mention')).toEqual([mark])
  })

  it.each([
    ['beside a docked panel', NARROW, 1, true],
    ['under a panel at phone size', PHONE, 0.5, false],
  ] as const)(
    'keeps the word pressed in sight once its cards and entries are drawn together, %s',
    async (_, size, share, moved) => {
      const { host, text } = await openBook({
        size,
        drawn: nights,
        who: () => ({ cards: [NED], length: 8 }),
        entries: Array.from({ length: 12 }, (_, n) => sense(n)),
      })
      // The name nearest that share of the way down, whole on the screen.
      const screen = text.getBoundingClientRect()
      const level = screen.top + screen.height * share
      const names = [0, 1, 2, 3, 4, 5, 6, 7]
        .flatMap((n) =>
          [0, 1, 2].map((nth) => wordsIn(host, n, 'Ned Reed', nth)),
        )
        .filter((range) => {
          const box = range.getBoundingClientRect()
          return (
            range.getClientRects().length === 1 &&
            box.top >= screen.top &&
            box.bottom <= screen.bottom
          )
        })
        .sort(
          (a, b) =>
            Math.abs(a.getBoundingClientRect().bottom - level) -
            Math.abs(b.getBoundingClientRect().bottom - level),
        )
      const name = names[0].getBoundingClientRect()
      const place = scrolledTo(text)
      await press(pointer, [name.left + 8, name.top + name.height / 2])
      await cardsDrawn()
      await wait(150)
      expect(inPanel('.book-lookup-entry')).toHaveLength(12)
      expect(selected()).toBe('Ned Reed')
      // The word pressed is the first of the name the light now covers.
      const lit = window.getSelection()?.getRangeAt(0) as Range
      const word = document.createRange()
      word.setStart(lit.startContainer, lit.startOffset)
      word.setEnd(lit.startContainer, lit.startOffset + 3)
      expect(word.toString()).toBe('Ned')
      const boxes = [...word.getClientRects()]
      expect(boxes.length).toBeGreaterThan(0)
      for (const box of boxes) {
        expect(inSight(text, box)).toEqual({ inText: true, clearOfPanel: true })
      }
      // Beside a docked panel the set-in took the word off the screen first.
      expect(scrolledTo(text)[1] > place[1]).toBe(moved)
    },
  )

  it('marks nothing while two editions are compared under the open look-up, and marks again after', async () => {
    const { host } = await openBook({ drawn: twoEditions })
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    expect(marks('book-mention')).toEqual(['1:Ned', '2:Ned Reed'])
    key('Tab')
    await wait(250)
    const layout = (name: string) =>
      [...host.querySelectorAll<HTMLElement>('.reader-layout button')].find(
        (button) => button.textContent === name,
      ) as HTMLElement
    await press(pointer, middleOf(layout('compare')))
    expect(host.querySelector('[data-compared]')).not.toBeNull()
    expect(cardEls()).toHaveLength(1)
    expect(CSS.highlights.has('book-mention')).toBe(false)
    expect(CSS.highlights.has('book-lookup-word')).toBe(false)
    await press(pointer, middleOf(layout('scroll')))
    expect(host.querySelector('[data-compared]')).toBeNull()
    expect(cardEls()).toHaveLength(1)
    expect(marks('book-mention')).toEqual(['1:Ned', '2:Ned Reed'])
    expect(marks('book-lookup-word')).toEqual(['0:Ned Reed'])
  })

  it('stays inside the seat’s share at phone size, and scrolls inside it with a fact open', async () => {
    const { host, text } = await openBook({ size: PHONE })
    await pressWords(pointer, host, 0, 'Ned')
    await cardsDrawn()
    const panel = panelEl() as HTMLElement
    expect(panel.dataset.side).toBe('bottom')
    const row = rowEls()[0]
    act(() => {
      row.scrollIntoView({ block: 'center' })
    })
    await wait(50)
    await press(pointer, middleOf(noteOf(row)))
    expect(opened(row)).toBe('true')
    const box = panel.getBoundingClientRect()
    expect(box.height).toBeLessThanOrEqual(window.innerHeight * CAP + 1)
    expect(box.height).toBeGreaterThan(window.innerHeight * CAP - 4)
    expect(panel.scrollHeight).toBeGreaterThan(panel.clientHeight)
    const book = scrolledTo(text)
    act(() => {
      panel.scrollTop = 0
    })
    await wait(50)
    const [x] = middle(box)
    if (pointer === 'touch') {
      await swipeBetween([x, box.bottom - 30], [x, box.top + 90])
    } else {
      await outsideAct(async () => {
        await mouse.moveTo(x, box.top + box.height / 2)
        await mouse.wheel(160)
      })
    }
    await wait(400)
    expect(panel.scrollTop).toBeGreaterThan(0)
    expect(scrolledTo(text)).toEqual(book)
    // Each line the fact opened on can be brought inside the panel.
    const lines = [...(moreOf(row) as HTMLElement).children]
    expect(lines).toHaveLength(5)
    for (const line of lines) {
      act(() => {
        line.scrollIntoView({ block: 'center' })
      })
      expect(shownInPanel(line.getBoundingClientRect())).toBe(true)
    }
    expect(scrolledTo(text)).toEqual(book)
    expect(panel.getBoundingClientRect().height).toBeCloseTo(box.height, 0)
  })
})

describe('the marks under an open look-up', () => {
  /** The text nodes block `n`'s own words are drawn in. */
  const drawnIn = (host: HTMLElement, n: number) => {
    const walker = document.createTreeWalker(
      ownText(host, n),
      NodeFilter.SHOW_TEXT,
    )
    const nodes: Node[] = []
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      nodes.push(node)
    }
    return nodes
  }

  /** `words` of block `n` highlighted from the selection bar. */
  async function highlight(
    host: HTMLElement,
    text: HTMLElement,
    n: number,
    words: string,
  ) {
    const range = wordsIn(host, n, words)
    window
      .getSelection()
      ?.setBaseAndExtent(
        range.startContainer,
        range.startOffset,
        range.endContainer,
        range.endOffset,
      )
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
    await wait(100)
    const swatch = host.querySelector<HTMLElement>(
      '.book-selection-bar .reader-swatch',
    )
    act(() => swatch?.click())
    await wait(100)
  }

  it('are set again on the words a note draws anew', async () => {
    const { host, text } = await openBook()
    // Two highlights made and undone, for redo to put back under the look-up.
    await highlight(host, text, 0, 'Ned Reed lit the lamp')
    await highlight(host, text, 1, 'night, and Ned went')
    expect(drawnIn(host, 0).length).toBeGreaterThan(1)
    expect(drawnIn(host, 1).length).toBeGreaterThan(1)
    key('z')
    key('z')
    await wait(100)
    expect(drawnIn(host, 0)).toHaveLength(1)
    expect(drawnIn(host, 1)).toHaveLength(1)
    await pressWords('mouse', host, 1, 'Ned')
    await cardsDrawn()
    expect(marks('book-lookup-word')).toEqual(['1:Ned'])
    expect(marks('book-mention')).toEqual(['0:Ned Reed', '2:Ned Reed'])
    const before = [
      ...ranges('book-lookup-word'),
      ...ranges('book-mention'),
    ].map((range) => range.startContainer)
    key('Z')
    key('Z')
    await wait(150)
    expect(cardEls()).toHaveLength(1)
    // The name's words and the first mention's are in nodes drawn since.
    expect(drawnIn(host, 0).length).toBeGreaterThan(1)
    expect(drawnIn(host, 1).length).toBeGreaterThan(1)
    const after = [...ranges('book-lookup-word'), ...ranges('book-mention')]
    expect(after.map((range) => range.startContainer.isConnected)).toEqual([
      true,
      true,
      true,
    ])
    expect(after[0].startContainer).not.toBe(before[0])
    expect(after[1].startContainer).not.toBe(before[1])
    expect(marks('book-lookup-word')).toEqual(['1:Ned'])
    expect(marks('book-mention')).toEqual(['0:Ned Reed', '2:Ned Reed'])
  })

  it('are on a page’s mentions by the frame a page turn shows it, and off the pages left behind', async () => {
    const { host, text } = await openBook({
      drawn: nights,
      prefs: { 'book-layout': { mode: 'scroll', paged: true } },
      who: () => ({ cards: [NED], length: 8 }),
    })
    const screen = text.getBoundingClientRect()
    const blocks = [...text.querySelectorAll<HTMLElement>('[data-block]')]
    const shown = () =>
      blocks
        .filter((block) =>
          meets(block.getBoundingClientRect(), text.getBoundingClientRect()),
        )
        .map((block) => block.dataset.block ?? '')
    const marked = () => marks('book-mention').map((mark) => mark.split(':')[0])
    // A name in the middle third, where a press on a paged text looks up.
    const name = [0, 1, 2, 3]
      .flatMap((n) => [0, 1, 2].map((nth) => wordsIn(host, n, 'Ned Reed', nth)))
      .map((range) => range.getBoundingClientRect())
      .find(
        (box) =>
          box.left > screen.left + screen.width * 0.4 &&
          box.right < screen.left + screen.width * 0.6 &&
          box.top > screen.top &&
          box.bottom < screen.bottom,
      ) as DOMRect
    await press('mouse', [name.left + 8, name.top + name.height / 2])
    await cardsDrawn()
    const first = shown()
    expect(first).toContain('0')
    expect(marked()).toContain('0')
    const frames = eachFrame(() => {
      const have = marked()
      return shown().filter((block) => !have.includes(block))
    })
    for (let n = 0; n < 6; n++) {
      key('PageDown')
      await wait(150)
    }
    await wait(150)
    frames.stop()
    expect(cardEls()).toHaveLength(1)
    const last = shown()
    expect(last.length).toBeGreaterThan(0)
    expect(last.filter((block) => first.includes(block))).toEqual([])
    expect(frames.seen.filter((unmarked) => unmarked.length > 0)).toEqual([])
    expect(marked()).not.toContain('0')
  })
})

/** The length of a spacing token, in px, where the panel is. */
function space(n: 1 | 2 | 3 | 4) {
  const probe = document.createElement('div')
  probe.style.width = `var(--space-${n})`
  ;(panelEl() as HTMLElement).append(probe)
  const { width } = getComputedStyle(probe)
  probe.remove()
  return Number.parseFloat(width)
}

/** `colour` as the browser computes it for a ground inside `within`. */
function groundOf(within: Element, colour: string) {
  const probe = document.createElement('span')
  probe.style.backgroundColor = colour
  within.append(probe)
  const { backgroundColor } = getComputedStyle(probe)
  probe.remove()
  return backgroundColor
}

/** The text's own colour inside `within`, mixed to `share` per cent. */
const mixOf = (within: Element, share: number) =>
  groundOf(within, `color-mix(in srgb, currentColor ${share}%, transparent)`)

const css = (el: Element, pseudo?: string) => getComputedStyle(el, pseudo)
const px = (length: string) => Number.parseFloat(length)
const boxOf = (el: Element) => el.getBoundingClientRect()
const CLEAR = 'rgba(0, 0, 0, 0)'

const WICKHAND = {
  text: 'Old Wickhand',
  given: null,
  surname: null,
  title: null,
  titleSex: null,
}
/** Ned with another name, so the about list has its three rows. */
const CALLED: CharacterCard = { ...NED, aliases: [WICKHAND] }

const [KEPT, TAUGHT] = NED.relations[0].facts
/** Ned with a first fact too long for one line, taken back as the second is. */
const WORDY: CharacterCard = {
  ...NED,
  relations: [
    {
      with: NED.relations[0].with,
      facts: [
        {
          ...KEPT,
          note: 'they kept the lamp on the hill together through every one of the long winters the keepers still tell of',
        },
        KEPT,
        TAUGHT,
      ],
    },
    NED.relations[1],
  ],
}

/**
 * The look-up's panel on `card`, for a real click on its name in a window
 * `size`, the pointer taken off the panel afterwards.
 */
async function showCard(
  card: CharacterCard = NED,
  size: readonly [number, number] = DESK,
) {
  const { host, text } = await openBook({
    size,
    who: (asked) =>
      asked.startsWith('Ned Reed') ? { cards: [card], length: 8 } : cast(asked),
  })
  await pressWords('mouse', host, 0, 'Ned')
  await cardsDrawn()
  await moveTo(3, 3)
  return { host, text, card: cardEls()[0] }
}

/** A real click on `row`'s words, the pointer taken off it afterwards. */
async function clickRow(row: Element) {
  await press('mouse', middleOf(noteOf(row)))
  await moveTo(3, 3)
  await wait(50)
}

const one = (within: Element, selector: string) =>
  within.querySelector(selector) as HTMLElement
const all = (within: Element, selector: string) => [
  ...within.querySelectorAll<HTMLElement>(selector),
]

/** The about list's labels beside their values, in a window `size`. */
async function aboutBesideValues(size: readonly [number, number]) {
  const { card } = await showCard(CALLED, size)
  const about = one(card, '.book-lookup-about')
  const labels = all(about, 'dt')
  const values = all(about, 'dd')
  expect(labels.map((label) => label.textContent)).toEqual([
    'Also called',
    'First met',
    'Last seen',
  ])
  expect(css(about).display).toBe('grid')
  const list = boxOf(about)
  const line = px(css(about).lineHeight)
  const widest = Math.max(...labels.map((label) => boxOf(label).right))
  labels.forEach((label, i) => {
    const [name, value] = [boxOf(label), boxOf(values[i])]
    expect(value.top).toBeCloseTo(name.top, 0)
    expect(name.left).toBeCloseTo(list.left, 0)
    // The labels' column is as wide as its widest and no wider: each is one line.
    expect(name.height).toBeLessThan(line * 1.5)
    expect(value.left - widest).toBeCloseTo(space(3), 0)
    // The values take the rest of the row.
    expect(value.right).toBeCloseTo(list.right, 0)
    if (i > 0) {
      const above = Math.max(
        boxOf(labels[i - 1]).bottom,
        boxOf(values[i - 1]).bottom,
      )
      expect(name.top - above).toBeCloseTo(space(1), 0)
    }
  })
  const words = document.createRange()
  words.selectNodeContents(labels[0])
  expect(
    boxOf(labels[0]).width - words.getBoundingClientRect().width,
  ).toBeLessThan(1)
  expect(px(css(about).fontSize)).toBeCloseTo(px(css(card).fontSize) * 0.92, 1)
  expect(css(labels[0]).opacity).toBe('0.65')
  expect(css(values[0]).opacity).toBe('1')
}

/** What the stylesheet gives each part of a card, by what Chromium computes and lays out. */
const LOOKS: { name: string; holds: () => Promise<void> }[] = [
  {
    name: 'a card is padded above and below, and its lines are loose',
    holds: async () => {
      const { card } = await showCard()
      const drawnAs = css(card)
      expect(px(drawnAs.paddingBlockStart)).toBe(space(3))
      expect(px(drawnAs.paddingBlockEnd)).toBe(space(3))
      expect(px(drawnAs.lineHeight)).toBeCloseTo(px(drawnAs.fontSize) * 1.45, 1)
    },
  },
  {
    name: 'the name has no space over it, and the about list starts close under it',
    holds: async () => {
      const { card } = await showCard()
      const name = one(card, 'h3')
      const about = one(card, '.book-lookup-about')
      expect(css(name).marginTop).toBe('0px')
      expect(px(css(name).marginBottom)).toBe(space(2))
      expect(css(about).marginTop).toBe('0px')
      expect(css(about).marginBottom).toBe('0px')
      expect(boxOf(about).top - boxOf(name).bottom).toBeCloseTo(space(2), 0)
    },
  },
  {
    name: 'the about list sets each label beside its value, quieter and smaller, on a desk',
    holds: () => aboutBesideValues(DESK),
  },
  {
    name: 'the about list sets each label beside its value, quieter and smaller, at phone size',
    holds: () => aboutBesideValues(PHONE),
  },
  {
    name: 'a relation stands off what is above it, under the other character’s name in the card’s own size',
    holds: async () => {
      const { card } = await showCard()
      const [first, second] = all(card, '.book-lookup-relation')
      const about = one(card, '.book-lookup-about')
      expect(boxOf(first).top - boxOf(about).bottom).toBeCloseTo(space(3), 0)
      expect(boxOf(second).top - boxOf(first).bottom).toBeCloseTo(space(3), 0)
      const other = one(first, 'h4')
      expect(other.textContent).toBe('Tom Field')
      expect(css(other).marginTop).toBe('0px')
      expect(px(css(other).marginBottom)).toBe(space(1))
      expect(css(other).fontSize).toBe(css(card).fontSize)
      expect(css(other).fontWeight).toBe('600')
    },
  },
  {
    name: 'the facts’ list has no bullet, no space of its own and no indent',
    holds: async () => {
      const { card } = await showCard()
      const relation = one(card, '.book-lookup-relation')
      const list = one(relation, 'ul')
      const fact = one(list, 'li')
      expect(css(list).marginTop).toBe('0px')
      expect(css(list).marginBottom).toBe('0px')
      expect(css(list).listStyleType).toBe('none')
      expect(css(list).paddingLeft).toBe('0px')
      expect(boxOf(fact).left).toBeCloseTo(boxOf(relation).left, 0)
      expect(boxOf(fact).width).toBeCloseTo(boxOf(relation).width, 0)
    },
  },
  {
    name: 'a fact’s row looks like a line of the list, as wide as it, and not like a button',
    holds: async () => {
      const { card } = await showCard()
      const row = rowEls()[1]
      const [line, around] = [css(row), css(card)]
      expect(line.display).toBe('flex')
      expect(line.alignItems).toBe('baseline')
      expect(px(line.columnGap)).toBe(space(2))
      expect(boxOf(row).width).toBeCloseTo(boxOf(one(card, 'ul')).width, 0)
      expect(
        [
          line.paddingTop,
          line.paddingRight,
          line.paddingBottom,
          line.paddingLeft,
        ].map(px),
      ).toEqual([space(1), space(2), space(1), space(2)])
      expect([
        line.borderTopWidth,
        line.borderRightWidth,
        line.borderBottomWidth,
        line.borderLeftWidth,
      ]).toEqual(['0px', '0px', '0px', '0px'])
      expect(line.borderTopLeftRadius).toBe('4px')
      expect(line.backgroundColor).toBe(CLEAR)
      expect(line.color).toBe(around.color)
      expect([line.fontFamily, line.fontSize, line.fontWeight]).toEqual([
        around.fontFamily,
        around.fontSize,
        around.fontWeight,
      ])
      expect(line.lineHeight).toBe(around.lineHeight)
      expect(line.textAlign).toBe('start')
      expect(line.cursor).toBe('pointer')
    },
  },
  {
    name: 'a fact’s row is as tall as its line and its padding, under a fine pointer and under a coarse one',
    holds: async () => {
      await showCard()
      const row = rowEls()[1]
      const tall = px(css(row).lineHeight) + 2 * space(1)
      expect(boxOf(row).height).toBeCloseTo(tall, 0)
      onTestFinished(pointerRulesAs('phone'))
      await wait(50)
      // The coarse rules are on: Close keeps the floor the reader's controls do.
      expect(
        css(one(panelEl() as HTMLElement, '.reader-button')).minHeight,
      ).toBe('44px')
      expect(boxOf(row).height).toBeCloseTo(tall, 0)
    },
  },
  {
    name: 'the mark before a row is one em wide beside any note, fainter, and turns as the row opens',
    holds: async () => {
      await showCard(WORDY)
      const [long, , short] = rowEls()
      expect(css(short, '::before').content).toBe('"▸"')
      expect(css(short, '::before').opacity).toBe('0.55')
      const em = px(css(long).fontSize)
      // The long note takes more than a line, so it presses on the mark.
      expect(boxOf(noteOf(long)).height).toBeGreaterThan(
        px(css(long).lineHeight) * 1.5,
      )
      for (const row of [long, short]) {
        expect(boxOf(noteOf(row)).left - boxOf(row).left).toBeCloseTo(
          space(2) + em + space(2),
          0,
        )
      }
      await clickRow(short)
      expect(opened(short)).toBe('true')
      expect(css(short, '::before').content).toBe('"▾"')
      expect(css(long, '::before').content).toBe('"▸"')
    },
  },
  {
    name: 'a row under a real pointer, and an open row, have a ground, and a closed row left alone has none',
    holds: async () => {
      await showCard()
      const row = rowEls()[1]
      const ground = mixOf(row, 10)
      expect(css(row).backgroundColor).toBe(CLEAR)
      await moveTo(...middleOf(row))
      await wait(50)
      expect(row.matches(':hover')).toBe(true)
      expect(css(row).backgroundColor).toBe(ground)
      await moveTo(3, 3)
      await wait(50)
      expect(css(row).backgroundColor).toBe(CLEAR)
      await clickRow(row)
      expect(opened(row)).toBe('true')
      expect(row.matches(':hover')).toBe(false)
      expect(css(row).backgroundColor).toBe(ground)
      expect(css(rowEls()[0]).backgroundColor).toBe(CLEAR)
    },
  },
  {
    name: 'a row focused from the keyboard shows an outline inside its box',
    holds: async () => {
      await showCard()
      await userEvent.keyboard('{Tab}')
      await userEvent.keyboard('{Tab}')
      const row = rowEls()[0]
      expect(document.activeElement).toBe(row)
      expect(row.matches(':focus-visible')).toBe(true)
      const ring = css(row)
      expect(ring.outlineStyle).toBe('solid')
      expect(ring.outlineWidth).toBe('2px')
      expect(ring.outlineColor).toBe(ring.color)
      expect(ring.outlineOffset).toBe('-2px')
    },
  },
  {
    name: 'an overturned fact is struck through and fainter, and a standing one is neither',
    holds: async () => {
      await showCard()
      const [taken, standing] = rowEls().map(noteOf)
      expect(css(taken).textDecorationLine).toBe('line-through')
      expect(css(taken).opacity).toBe('0.7')
      expect(css(standing).textDecorationLine).toBe('none')
      expect(css(standing).opacity).toBe('1')
    },
  },
  {
    name: 'the overturned tag sits at its row’s end, small and bordered, and says what it means to a pointer resting on it',
    holds: async () => {
      const { card } = await showCard(WORDY)
      const tags = all(card, '.book-lookup-fact-tag')
      const rows = rowEls()
      expect(tags).toHaveLength(2)
      tags.forEach((tag, i) => {
        expect(boxOf(tag).right).toBeCloseTo(boxOf(rows[i]).right - space(2), 0)
      })
      // Beside a note that fills its row the tag is as wide as beside a short one.
      expect(boxOf(tags[0]).width).toBeCloseTo(boxOf(tags[1]).width, 1)
      const [, tag] = tags
      const drawnAs = css(tag)
      expect(
        [
          drawnAs.paddingTop,
          drawnAs.paddingRight,
          drawnAs.paddingBottom,
          drawnAs.paddingLeft,
        ].map(px),
      ).toEqual([0, space(1), 0, space(1)])
      expect(drawnAs.borderTopWidth).toBe('1px')
      expect(drawnAs.borderTopStyle).toBe('solid')
      expect(drawnAs.borderTopColor).toBe(mixOf(tag, 50))
      expect(drawnAs.borderTopLeftRadius).toBe('3px')
      expect(px(drawnAs.fontSize)).toBeCloseTo(
        px(css(rows[1]).fontSize) * 0.8,
        1,
      )
      // One word cannot be squeezed, so its not shrinking shows only as computed.
      expect([drawnAs.flexGrow, drawnAs.flexShrink]).toEqual(['0', '0'])
      expect(drawnAs.cursor).toBe('help')
      expect(document.elementFromPoint(...middleOf(tag))).toBe(tag)
      expect(tag.title).toBe('The story later showed this to be wrong')
      await clickRow(rows[1])
      const label = one(
        moreOf(rows[1]) as HTMLElement,
        '.book-lookup-overturn > span',
      )
      expect(document.elementFromPoint(...middleOf(label))).toBe(label)
      expect(label.title).toBe(tag.title)
    },
  },
  {
    name: 'what an open fact adds is set in under the note’s words, smaller, with space round it',
    holds: async () => {
      await showCard()
      const row = rowEls()[0]
      await clickRow(row)
      const more = moreOf(row) as HTMLElement
      const drawnAs = css(more)
      expect(
        [
          drawnAs.marginTop,
          drawnAs.marginRight,
          drawnAs.marginBottom,
          drawnAs.marginLeft,
        ].map(px),
      ).toEqual([space(1), 0, space(2), 0])
      const fact = more.parentElement as HTMLElement
      expect(px(drawnAs.fontSize)).toBeCloseTo(px(css(fact).fontSize) * 0.92, 1)
      expect(px(drawnAs.paddingInlineStart)).toBeCloseTo(
        2 * space(2) + px(drawnAs.fontSize),
        1,
      )
      const quote = one(more, '.book-lookup-quote')
      expect(
        Math.abs(boxOf(quote).left - boxOf(noteOf(row)).left),
      ).toBeLessThan(2)
    },
  },
  {
    name: 'the lines an open fact adds are close set: a quote in italics behind a rule, where it is said fainter, the overturn set off',
    holds: async () => {
      const { card } = await showCard()
      const row = rowEls()[0]
      await clickRow(row)
      const lines = [...(moreOf(row) as HTMLElement).children]
      expect(lines.map((line) => line.className)).toEqual([
        'book-lookup-quote',
        'book-lookup-where',
        'book-lookup-overturn',
        'book-lookup-quote',
        'book-lookup-where',
      ])
      const [quote, where, overturn] = lines
      for (const line of lines) {
        expect(px(css(line).marginBottom)).toBe(space(1))
        expect(css(line).marginLeft).toBe('0px')
        expect(css(line).marginRight).toBe('0px')
        expect(px(css(line).marginTop)).toBe(line === overturn ? space(2) : 0)
      }
      const drawnAs = css(quote)
      expect(px(drawnAs.paddingInlineStart)).toBe(space(3))
      expect(drawnAs.borderInlineStartWidth).toBe('2px')
      expect(drawnAs.borderInlineStartStyle).toBe('solid')
      expect(drawnAs.borderInlineStartColor).toBe(mixOf(quote, 30))
      // Stronger than the rule between cards, which is the same colour at 20%.
      expect(css(card).borderTopColor).toBe(mixOf(card, 20))
      expect(drawnAs.borderInlineStartColor).not.toBe(css(card).borderTopColor)
      expect(drawnAs.fontStyle).toBe('italic')
      expect(drawnAs.opacity).toBe('1')
      const said = quote.firstElementChild as HTMLElement
      expect(css(said).marginTop).toBe('0px')
      expect(css(said).marginBottom).toBe('0px')
      expect(css(where).opacity).toBe('0.65')
    },
  },
  {
    name: 'the name looked up is painted as the selection is, with the selection gone',
    holds: async () => {
      const { host } = await showCard()
      const block = one(host, '[data-block="0"]')
      const accent = tokenColour(block, '--accent')
      const lit = css(block, '::highlight(book-lookup-word)')
      expect(lit.backgroundColor).toBe(accent)
      expect(lit.color).toBe(tokenColour(block, '--on-accent'))
      act(() => window.getSelection()?.removeAllRanges())
      await wait(50)
      expect(marks('book-lookup-word')).toEqual(['0:Ned Reed'])
      expect(
        apart(await groundBehind(host, 0, 'Ned Reed'), channels(accent)),
      ).toBeLessThan(12)
    },
  },
  {
    name: 'a mention is painted as a line under a faint wash of the accent, and moves nothing',
    holds: async () => {
      const { host } = await showCard()
      const block = one(host, '[data-block="1"]')
      const accent = tokenColour(block, '--accent')
      const mention = css(block, '::highlight(book-mention)')
      expect(mention.backgroundColor).toBe(
        groundOf(block, 'color-mix(in srgb, var(--accent) 18%, transparent)'),
      )
      expect(mention.textDecorationLine).toBe('underline')
      expect(mention.textDecorationColor).toBe(accent)
      expect(mention.textDecorationThickness).toBe('2px')
      expect(px(mention.textUnderlineOffset)).toBeCloseTo(
        px(css(block).fontSize) * 0.2,
        1,
      )
      // The wash is drawn: the ground behind the mention is neither the page's nor the accent's.
      const washed = await groundBehind(host, 1, 'Ned')
      const page = await groundBehind(host, 1, 'Mara')
      expect(apart(washed, page)).toBeGreaterThan(12)
      expect(apart(washed, channels(accent))).toBeGreaterThan(60)
      // The marks are cleared by the reader's own path, a press on a word that
      // names no one with the panel still open, so whatever draws them goes
      // with them: deleting the registry's entries would leave in place an
      // element drawn round a mention.
      const boxes = () => [
        ...[0, 1, 2, 3].map((n) =>
          boxOf(one(host, `[data-block="${n}"]`)).toJSON(),
        ),
        ...[...wordsIn(host, 1, 'Ned').getClientRects()].map((box) =>
          box.toJSON(),
        ),
        ...[...wordsIn(host, 2, 'Ned Reed').getClientRects()].map((box) =>
          box.toJSON(),
        ),
      ]
      expect(marks('book-mention')).toEqual(['1:Ned', '2:Ned Reed'])
      const marked = boxes()
      await pressWords('mouse', host, 0, 'lamp')
      await settleUntil(
        'the entry alone',
        () => entryEl() !== undefined && cardEls().length === 0,
        4000,
      )
      await moveTo(3, 3)
      expect(marks('book-lookup-word')).toEqual(['0:lamp'])
      expect(marks('book-mention')).toEqual([])
      expect(boxes()).toEqual(marked)
      expect(apart(await groundBehind(host, 1, 'Ned'), page)).toBeLessThan(12)
    },
  },
]

describe('the look of a character’s card', () => {
  it.each(LOOKS)('$name', ({ holds }) => holds())
})

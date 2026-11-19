import { act } from 'react'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
import { offsetAtPoint } from '../src/bookNotes'
import { bringIntoSight } from '../src/bookPages'
import { BookReader } from '../src/BookReader'
import '../src/index.css'
import { mount } from './support/mount'
import { swipeBetween, tapAt } from './support/touch'

vi.mock('../src/Account', () => ({ Account: () => null }))

/**
 * Where a look-up's panel sits at phone size, against Chromium's own layout
 * and real touches: across the screen from its word, along the inline axis of
 * vertical text and the block axis of horizontal text alike.
 */

/** Every character the same word, so any point on the text looks one up. */
const CATS = '猫'.repeat(600)
const block = (text: string) => ({
  text,
  heading: false,
  sentences: [[0, text.length]],
  marks: [],
})
const book = {
  title: '猫',
  hub: 'ja',
  editions: {
    ja: {
      label: '日本語',
      language: 'ja',
      blocks: Array.from({ length: 40 }, () => block(CATS)),
    },
  },
  alignments: [],
}

type Answer = { length: number; entries: object[] }

const entry = (senses: number) => ({
  word: '猫',
  pos: 'noun',
  glosses: Array.from(
    { length: senses },
    (_, n) => `sense ${n} of a word with a fairly ordinary gloss to read`,
  ),
})
const SHORT: Answer = { length: 2, entries: [entry(1)] }
/** Taller than the screen, so only the panel's limits decide its height. */
const LONG: Answer = {
  length: 2,
  entries: Array.from({ length: 12 }, () => entry(8)),
}

const FLOWS = [
  { name: 'horizontal, scrolled', vertical: false, paged: false },
  { name: 'horizontal, paged', vertical: false, paged: true },
  { name: 'vertical, scrolled', vertical: true, paged: false },
  { name: 'vertical, paged', vertical: true, paged: true },
]
type Flow = (typeof FLOWS)[number]
const SCROLLED = FLOWS.filter((flow) => !flow.paged)
const VERTICAL = FLOWS.filter((flow) => flow.vertical)

const PHONE = [390, 844] as const
const SIDEBAR = [1366, 768] as const
/** The share of the screen's height the panel is held under. */
const CAP = 0.45
/** The clear space the panel keeps from its word. */
const GAP = 8

const wait = (ms: number) =>
  act(() => new Promise<void>((done) => setTimeout(done, ms)))

async function windowOf(width: number, height: number) {
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, height)
  await wait(120)
}

/** What the dictionary answers, once `gate`, if there is one, has opened. */
type Dictionary = { answer: Answer; gate?: Promise<void> }

/** The long book in `flow` on a window `size`, some way in. */
async function openFlow(
  { vertical, paged }: Flow,
  size: readonly [number, number],
  answer: Answer = SHORT,
) {
  await windowOf(...size)
  const prefs: Record<string, object> = {
    lookup: { on: true },
    'book-text': { vertical },
    'book-layout': { mode: 'scroll', paged },
  }
  for (const [id, value] of Object.entries(prefs)) {
    localStorage.setItem(`taiyaku.prefs.${id}`, JSON.stringify(value))
  }
  onTestFinished(() => {
    for (const id of Object.keys(prefs)) {
      localStorage.removeItem(`taiyaku.prefs.${id}`)
    }
    vi.unstubAllGlobals()
  })
  const dictionary: Dictionary = { answer }
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
      if (url.startsWith('/api/dictionary/')) {
        await dictionary.gate
        return Response.json(
          url.includes('/at/') ? dictionary.answer : dictionary.answer.entries,
        )
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [] })
    }),
  )
  const host = mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  for (
    let i = 0;
    i < 200 && !host.querySelector('[data-block] [data-text]');
    i++
  ) {
    await wait(10)
  }
  await wait(150)
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
  await wait(200)
  const text = host.querySelector('.book-text') as HTMLElement
  expect(getComputedStyle(text).writingMode).toBe(
    vertical ? 'vertical-rl' : 'horizontal-tb',
  )
  return { host, text, dictionary }
}

const dialog = () =>
  document.querySelector<HTMLElement>('[role="dialog"][aria-label="Look up"]')
const panelBox = () => (dialog() as HTMLElement).getBoundingClientRect()
const sideOf = () => dialog()?.dataset.side
const buttonOf = (name: string) =>
  [...(dialog()?.querySelectorAll('button') ?? [])].find(
    (button) => button.textContent === name,
  ) as HTMLElement

const meets = (a: DOMRect, b: DOMRect) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom

const scrolled = (text: HTMLElement) => [text.scrollLeft, text.scrollTop]

/** The middle of `box`. */
const middle = (box: DOMRect): [number, number] => [
  box.left + box.width / 2,
  box.top + box.height / 2,
]

/** A point on a character `share` down the screen, `along` across the text. */
function pointAt(
  text: HTMLElement,
  share: number,
  along = 0.5,
): [number, number] {
  const box = text.getBoundingClientRect()
  for (const dy of [0, 6, -6, 12, -12, 20, -20, 30, -30]) {
    for (const dx of [0, 8, -8, 16, -16, 24, -24]) {
      const x = box.left + box.width * along + dx
      const y = share * window.innerHeight + dy
      const on = document.elementFromPoint(x, y)
      if (on && text.contains(on) && offsetAtPoint(text, x, y)) return [x, y]
    }
  }
  throw new Error(`no character lies ${share} of the way down the screen`)
}

/** A real tap, then long enough for the next not to be its second. */
async function tap(x: number, y: number) {
  await tapAt(x, y)
  await wait(350)
}

/** The boxes of the word lit, each drawn on the screen. */
function litBoxes() {
  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
    return []
  }
  return [...selection.getRangeAt(0).getClientRects()].filter(
    (box) =>
      box.width > 0 &&
      box.bottom > 0 &&
      box.top < window.innerHeight &&
      box.right > 0 &&
      box.left < window.innerWidth,
  )
}

/** `length` characters of block `n` from `at`. */
function rangeOf(host: HTMLElement, n: number, at: number, length: number) {
  const words = host.querySelector(`[data-block="${n}"] > [data-text]`) as Node
  const node = document
    .createTreeWalker(words, NodeFilter.SHOW_TEXT)
    .nextNode() as Text
  const range = document.createRange()
  range.setStart(node, at)
  range.setEnd(node, at + length)
  return range
}

const charBox = (host: HTMLElement, n: number, at: number) =>
  rangeOf(host, n, at, 1).getClientRects()[0]

/** The last character on the line or column character `at` of block `n` is on. */
function lastOnLine(
  host: HTMLElement,
  n: number,
  at: number,
  vertical: boolean,
) {
  const lineOf = (box: DOMRect) => (vertical ? box.right : box.top)
  const line = lineOf(charBox(host, n, at))
  let last = at
  while (lineOf(charBox(host, n, last + 1)) === line) last += 1
  return last
}

/** The reader's top and bottom bars, shown by their key. */
async function showBars(host: HTMLElement) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Tab',
        bubbles: true,
        cancelable: true,
      }),
    )
  })
  await wait(250)
  expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  expect(host.querySelector('.reader-bottom-bar')).not.toBeNull()
}

/**
 * The word at `point`, selected, with the selection bar's Look up pressed
 * straight after `first`, in the one task, before any scroll it starts has
 * told the reader to take the bar away.
 */
async function lookUpSelection(
  host: HTMLElement,
  text: HTMLElement,
  point: [number, number],
  first: () => void = () => {},
) {
  const spot = offsetAtPoint(text, ...point) as { block: number; at: number }
  const word = rangeOf(host, spot.block, spot.at, 2)
  const boxes = [...word.getClientRects()]
  window
    .getSelection()
    ?.setBaseAndExtent(
      word.startContainer,
      word.startOffset,
      word.endContainer,
      word.endOffset,
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
  await wait(80)
  const button = [...host.querySelectorAll('.book-selection-bar button')].find(
    (b) => b.textContent === 'Look up',
  ) as HTMLElement
  first()
  act(() => button.click())
  await wait(350)
  return boxes
}

describe.each(FLOWS)('at phone size, $name', (flow) => {
  it.each([
    [0.2, 'bottom'],
    [0.8, 'top'],
  ])(
    'a tap %f of the way down the screen opens the panel at the %s',
    async (share, side) => {
      const { text } = await openFlow(flow, PHONE)
      await tap(...pointAt(text, share))
      expect(sideOf()).toBe(side)
      const panel = panelBox()
      if (side === 'top') expect(panel.top).toBeLessThan(40)
      else expect(panel.bottom).toBeGreaterThan(window.innerHeight - 40)
      const word = litBoxes()
      expect(word.length).toBeGreaterThan(0)
      for (const box of word) expect(meets(box, panel)).toBe(false)
    },
  )

  it.each([
    [0.2, 'bottom'],
    [0.8, 'top'],
  ])(
    'the selection bar on a word %f of the way down opens the panel at the %s',
    async (share, side) => {
      const { host, text } = await openFlow(flow, PHONE)
      const word = await lookUpSelection(host, text, pointAt(text, share))
      expect(sideOf()).toBe(side)
      for (const box of word) expect(meets(box, panelBox())).toBe(false)
    },
  )

  it('holds a long answer under its share of the screen', async () => {
    const { text } = await openFlow(flow, PHONE, LONG)
    await tap(...pointAt(text, 0.2))
    const { height } = panelBox()
    expect(height).toBeLessThanOrEqual(window.innerHeight * CAP + 1)
    expect(height).toBeGreaterThan(window.innerHeight * CAP - 4)
  })

  it('stops short of a word with less room beside it than that share', async () => {
    const { host, text } = await openFlow(flow, PHONE, LONG)
    await showBars(host)
    await tap(...pointAt(text, 0.5))
    const panel = panelBox()
    expect(panel.height).toBeLessThan(window.innerHeight * CAP)
    const word = litBoxes()
    expect(word.length).toBeGreaterThan(0)
    for (const box of word) {
      const clear =
        sideOf() === 'top' ? box.top - panel.bottom : panel.top - box.bottom
      expect(clear).toBeGreaterThanOrEqual(GAP - 0.5)
    }
  })

  it('scrolls a long answer inside itself, under its head, and leaves the book', async () => {
    const { text } = await openFlow(flow, PHONE, LONG)
    await tap(...pointAt(text, 0.2))
    const panel = dialog() as HTMLElement
    const head = panel.querySelector('.reader-panel-head') as HTMLElement
    const box = panel.getBoundingClientRect()
    const headTop = head.getBoundingClientRect().top
    const book = scrolled(text)
    const [x] = middle(box)
    await swipeBetween([x, box.bottom - 30], [x, box.top + 70])
    await wait(400)
    expect(panel.scrollTop).toBeGreaterThan(0)
    expect(head.getBoundingClientRect().top).toBeCloseTo(headTop, 0)
    expect(
      document.elementFromPoint(
        ...middle(buttonOf('Close').getBoundingClientRect()),
      ),
    ).toBe(buttonOf('Close'))
    expect(scrolled(text)).toEqual(book)
    // On past the end of the answer, where a scroll would pass to the book.
    act(() => {
      panel.scrollTop = panel.scrollHeight
    })
    await wait(100)
    await swipeBetween([x, box.bottom - 30], [x, box.top + 70])
    await wait(400)
    expect(scrolled(text)).toEqual(book)
  })

  it.each(['a tap', 'the selection bar'])(
    'leaves the place as it was when %s opens it, answers and closes',
    async (opener) => {
      const { host, text, dictionary } = await openFlow(flow, PHONE, LONG)
      let answer = () => {}
      dictionary.gate = new Promise((done) => {
        answer = done
      })
      const before = scrolled(text)
      // Where the panel opens until it is seated.
      const point = pointAt(text, 0.12)
      if (opener === 'a tap') await tap(...point)
      else await lookUpSelection(host, text, point)
      expect(dialog()).not.toBeNull()
      expect(scrolled(text)).toEqual(before)
      answer()
      await wait(200)
      expect(scrolled(text)).toEqual(before)
      act(() => buttonOf('Close').click())
      await wait(200)
      expect(dialog()).toBeNull()
      expect(scrolled(text)).toEqual(before)
    },
  )

  it('keeps its side and its edge as a late answer arrives', async () => {
    const { text, dictionary } = await openFlow(flow, PHONE, LONG)
    let answer = () => {}
    dictionary.gate = new Promise((done) => {
      answer = done
    })
    await tap(...pointAt(text, 0.2))
    const asking = panelBox()
    expect(sideOf()).toBe('bottom')
    answer()
    await wait(250)
    expect(sideOf()).toBe('bottom')
    expect(panelBox().height).toBeGreaterThan(asking.height)
    expect(panelBox().bottom).toBeCloseTo(asking.bottom, 0)
  })

  it('chooses again for the next word', async () => {
    const { text } = await openFlow(flow, PHONE)
    await tap(...pointAt(text, 0.35))
    expect(sideOf()).toBe('bottom')
    await tap(...pointAt(text, 0.65))
    expect(sideOf()).toBe('top')
    const word = litBoxes()
    expect(word.length).toBeGreaterThan(0)
    for (const box of word) expect(meets(box, panelBox())).toBe(false)
  })

  it.each([
    [0.2, 'bottom'],
    [0.8, 'top'],
  ])(
    'stays clear of the bars, for a word %f of the way down',
    async (share, side) => {
      const { host, text } = await openFlow(flow, PHONE, LONG)
      await showBars(host)
      await tap(...pointAt(text, share))
      expect(sideOf()).toBe(side)
      const panel = panelBox()
      for (const bar of host.querySelectorAll(
        '.reader-top-bar, .reader-bottom-bar',
      )) {
        expect(meets(bar.getBoundingClientRect(), panel)).toBe(false)
      }
    },
  )

  it.each([
    ['pinning bar', 'book-pinning', {}],
    [
      'compared columns’ bar',
      'book-columns',
      { position: 'absolute', top: '0' },
    ],
  ])('stays clear of the %s', async (_, name, place) => {
    const { host, text } = await openFlow(flow, PHONE, LONG)
    // Both belong to compared columns, where a tap looks nothing up, so the
    // bar is drawn here with its own class and stylesheet.
    const bar = document.createElement('div')
    bar.className = name
    bar.textContent = 'Pick a paragraph on the left to pin.'
    Object.assign(bar.style, place)
    host.querySelector('.book-reader')?.append(bar)
    await tap(...pointAt(text, 0.8))
    expect(sideOf()).toBe('top')
    expect(meets(bar.getBoundingClientRect(), panelBox())).toBe(false)
  })

  it.each([
    [0.2, 'bottom'],
    [0.8, 'top'],
  ])(
    'moves clear of the bars shown over it, for a word %f of the way down',
    async (share, side) => {
      const { host, text } = await openFlow(flow, PHONE, LONG)
      await tap(...pointAt(text, share))
      expect(sideOf()).toBe(side)
      await showBars(host)
      expect(sideOf()).toBe(side)
      const panel = panelBox()
      for (const bar of host.querySelectorAll(
        '.reader-top-bar, .reader-bottom-bar',
      )) {
        expect(meets(bar.getBoundingClientRect(), panel)).toBe(false)
      }
    },
  )

  it('shows a second word waiting for its answer, with nothing of the first to save', async () => {
    const { text, dictionary } = await openFlow(flow, PHONE)
    await tap(...pointAt(text, 0.35))
    expect(buttonOf('Save word')).toBeDefined()
    let answer = () => {}
    dictionary.gate = new Promise((done) => {
      answer = done
    })
    await tap(...pointAt(text, 0.65))
    expect(dialog()?.textContent).toContain('Looking it up…')
    expect(buttonOf('Save word')).toBeUndefined()
    answer()
    await wait(200)
    expect(dialog()?.textContent).not.toContain('Looking it up…')
    expect(buttonOf('Save word')).toBeDefined()
  })

  it('takes the focus, and a real tap on Close shuts it, on either side', async () => {
    const { text } = await openFlow(flow, PHONE)
    for (const share of [0.2, 0.8]) {
      await tap(...pointAt(text, share))
      expect(dialog()?.contains(document.activeElement)).toBe(true)
      await tap(...middle(buttonOf('Close').getBoundingClientRect()))
      expect(dialog()).toBeNull()
      await wait(100)
    }
  })

  it('saves the word for a real tap on Save word', async () => {
    const { host, text } = await openFlow(flow, PHONE)
    await tap(...pointAt(text, 0.8))
    expect(sideOf()).toBe('top')
    await tap(...middle(buttonOf('Save word').getBoundingClientRect()))
    expect(dialog()).toBeNull()
    expect(host.querySelector('[data-note]')).not.toBeNull()
  })
})

describe.each(SCROLLED)('at phone size, $name', (flow) => {
  it('leaves the panel where it opens for a selection scrolled off the screen', async () => {
    const { host, text } = await openFlow(flow, PHONE)
    await lookUpSelection(host, text, pointAt(text, 0.5), () => {
      text.scrollBy(
        flow.vertical ? { left: -3 * innerWidth } : { top: 3 * innerHeight },
      )
    })
    expect(dialog()).not.toBeNull()
    expect(sideOf()).toBeUndefined()
  })

  it('does not move the panel as the reader scrolls the book on', async () => {
    const { text } = await openFlow(flow, PHONE)
    await tap(...pointAt(text, 0.4))
    const was = panelBox()
    const book = scrolled(text)
    // Far enough to carry the word past the middle of the screen, or off it.
    await (flow.vertical
      ? swipeBetween([30, 300], [360, 300])
      : swipeBetween([195, 300], [195, 480]))
    await wait(400)
    expect(scrolled(text)).not.toEqual(book)
    expect(sideOf()).toBe('bottom')
    expect(panelBox().top).toBe(was.top)
    expect(panelBox().height).toBe(was.height)
  })
})

describe('a word its answer lengthens past the middle of the screen', () => {
  const answered = async (dictionary: Dictionary, length: number) => {
    let answer = () => {}
    dictionary.gate = new Promise((done) => {
      answer = done
    })
    dictionary.answer = { ...SHORT, length }
    return answer
  }

  it.each(VERTICAL)(
    'keeps the side its first character chose, $name',
    async (flow) => {
      const { text, dictionary } = await openFlow(flow, PHONE)
      const answer = await answered(dictionary, 14)
      await tap(...pointAt(text, 0.42))
      expect(sideOf()).toBe('bottom')
      answer()
      await wait(250)
      const word = litBoxes()
      const lowest = Math.max(...word.map((box) => box.bottom))
      // The word now has more room above it, and the side stays regardless.
      expect(lowest).toBeGreaterThan(window.innerHeight * 0.6)
      expect(sideOf()).toBe('bottom')
      for (const box of word) expect(meets(box, panelBox())).toBe(false)
    },
  )

  it('keeps the side its first character chose, running on to the next line', async () => {
    const { host, text, dictionary } = await openFlow(FLOWS[0], PHONE)
    const spot = offsetAtPoint(text, ...pointAt(text, 0.5)) as {
      block: number
      at: number
    }
    const last = lastOnLine(host, spot.block, spot.at, false)
    const pitch =
      charBox(host, spot.block, last + 1).top -
      charBox(host, spot.block, last).top
    // Just above the middle, so the line below it tips the room the other way.
    const box = charBox(host, spot.block, last)
    const by = box.top + box.height / 2 - (innerHeight / 2 - pitch / 4)
    act(() => {
      text.scrollBy({ top: by })
    })
    await wait(150)
    const answer = await answered(dictionary, 6)
    await tap(...middle(charBox(host, spot.block, last - 2)))
    expect(sideOf()).toBe('bottom')
    answer()
    await wait(250)
    expect(litBoxes().length).toBe(2)
    expect(sideOf()).toBe('bottom')
  })
})

describe('an answer that leaves its word no room on the chosen side', () => {
  it.each(VERTICAL)('moves the panel across once, $name', async (flow) => {
    const { host, text, dictionary } = await openFlow(flow, [844, 390])
    // Across from the panel, which vertical text docks at the left.
    const point = pointAt(text, 0.36, 0.4)
    const spot = offsetAtPoint(text, ...point) as { block: number; at: number }
    const last = lastOnLine(host, spot.block, spot.at, true)
    let answer = () => {}
    dictionary.gate = new Promise((done) => {
      answer = done
    })
    dictionary.answer = {
      ...SHORT,
      length: Math.min(16, last - spot.at + 1),
    }
    await tap(...point)
    expect(sideOf()).toBe('bottom')
    answer()
    await wait(250)
    expect(sideOf()).toBe('top')
    const word = litBoxes()
    expect(word.length).toBeGreaterThan(0)
    for (const box of word) expect(meets(box, panelBox())).toBe(false)
    await wait(400)
    expect(sideOf()).toBe('top')
  })
})

describe('a word with a piece at each end of the screen', () => {
  it.each(VERTICAL)('leaves Close to be pressed, $name', async (flow) => {
    const { host, text, dictionary } = await openFlow(flow, [360, 640])
    const point = pointAt(text, 0.5)
    const spot = offsetAtPoint(text, ...point) as { block: number; at: number }
    const start = lastOnLine(host, spot.block, spot.at, true) - 3
    dictionary.answer = { ...SHORT, length: 14 }
    await tap(...middle(charBox(host, spot.block, start)))
    const pieces = litBoxes()
    expect(Math.min(...pieces.map((box) => box.top))).toBeLessThan(160)
    expect(Math.max(...pieces.map((box) => box.bottom))).toBeGreaterThan(480)
    const panel = panelBox()
    const close = buttonOf('Close').getBoundingClientRect()
    expect(close.top).toBeGreaterThanOrEqual(panel.top)
    expect(close.bottom).toBeLessThanOrEqual(panel.bottom)
    expect(close.top).toBeGreaterThanOrEqual(0)
    expect(close.bottom).toBeLessThanOrEqual(window.innerHeight)
    await tap(...middle(close))
    expect(dialog()).toBeNull()
  })

  it.each(VERTICAL)(
    'moves nothing, since no scroll along the text can clear it, $name',
    async (flow) => {
      const { host, text, dictionary } = await openFlow(flow, [360, 640])
      const point = pointAt(text, 0.5)
      const spot = offsetAtPoint(text, ...point) as {
        block: number
        at: number
      }
      const start = lastOnLine(host, spot.block, spot.at, true) - 3
      dictionary.answer = { ...SHORT, length: 14 }
      const before = scrolled(text)
      await tap(...middle(charBox(host, spot.block, start)))
      const panel = panelBox()
      expect(litBoxes().some((box) => meets(box, panel))).toBe(true)
      expect(scrolled(text)).toEqual(before)
    },
  )
})

describe('a word beside the panel, where the screen is wider than it', () => {
  it.each([
    { flow: FLOWS[0], along: 0.3 },
    { flow: FLOWS[2], along: 0.7 },
  ])(
    'takes no limit on the height from the word, $flow.name',
    async ({ flow, along }) => {
      const { text } = await openFlow(flow, [844, 390], LONG)
      await tap(...pointAt(text, 0.5, along))
      const panel = panelBox()
      const word = litBoxes()
      expect(word.length).toBeGreaterThan(0)
      for (const box of word) {
        expect(box.right <= panel.left || box.left >= panel.right).toBe(true)
      }
      expect(dialog()?.style.getPropertyValue('--lookup-room')).toBe('')
      expect(panel.height).toBeGreaterThan(window.innerHeight * CAP - 4)
    },
  )
})

describe('turning the phone with the panel open', () => {
  it.each([
    { flow: FLOWS[0], share: 0.35 },
    { flow: FLOWS[2], share: 0.35 },
    { flow: FLOWS[3], share: 0.2 },
  ])(
    'chooses the side again for the new screen, $flow.name',
    async ({ flow, share }) => {
      const { text } = await openFlow(flow, PHONE)
      await tap(...pointAt(text, share))
      expect(sideOf()).toBe('bottom')
      await page.viewport(390, 500)
      await wait(500)
      const word = litBoxes()
      expect(word.length).toBeGreaterThan(0)
      expect(sideOf()).toBe('top')
      for (const box of word) expect(meets(box, panelBox())).toBe(false)
    },
  )

  it.each(SCROLLED)(
    'keeps its side when the word has been scrolled away, $name',
    async (flow) => {
      const { text } = await openFlow(flow, PHONE)
      await tap(...pointAt(text, 0.3))
      expect(sideOf()).toBe('bottom')
      act(() => {
        text.scrollBy(
          flow.vertical ? { left: -3 * innerWidth } : { top: 3 * innerHeight },
        )
      })
      await wait(200)
      expect(litBoxes()).toEqual([])
      await page.viewport(390, 700)
      await wait(500)
      expect(sideOf()).toBe('bottom')
      expect(dialog()?.style.getPropertyValue('--lookup-room')).toBe('')
    },
  )
})

describe.each(FLOWS)('at the sidebar layout, $name', (flow) => {
  it('docks the panel where it always opens, for a word anywhere', async () => {
    const { text } = await openFlow(flow, SIDEBAR)
    const tops: number[] = []
    for (const share of [0.2, 0.8]) {
      await tap(...pointAt(text, share))
      const panel = dialog() as HTMLElement
      expect(panel.dataset.side).toBeUndefined()
      expect(panel.style.getPropertyValue('--lookup-room')).toBe('')
      tops.push(panelBox().top)
      act(() => buttonOf('Close').click())
      await wait(400)
    }
    expect(tops[0]).toBe(tops[1])
  })
})

describe.each([
  ['phone', PHONE],
  ['sidebar', SIDEBAR],
] as const)('bringing a word in at the %s layout', (_, size) => {
  /** A word on screen broken over two lines, nearest the middle of the screen. */
  function brokenWord(host: HTMLElement, text: HTMLElement) {
    const screen = text.getBoundingClientRect()
    const centre = middle(screen)
    let nearest: { word: Range; pieces: DOMRect[]; off: number } | undefined
    for (const held of host.querySelectorAll<HTMLElement>('[data-block]')) {
      const drawn = held.getBoundingClientRect()
      if (!meets(drawn, screen)) continue
      const n = Number(held.dataset.block)
      for (let at = 0; at + 1 < CATS.length; at++) {
        const word = rangeOf(host, n, at, 2)
        const pieces = [...word.getClientRects()]
        if (pieces.length !== 2) continue
        // Well inside the screen across the lines, for a scroll to have room.
        const [across, along] =
          text.dataset.vertical === undefined ? [4, 40] : [40, 4]
        const inside = pieces.every(
          (box) =>
            box.left >= screen.left + across &&
            box.right <= screen.right - across &&
            box.top >= screen.top + along &&
            box.bottom <= screen.bottom - along,
        )
        if (!inside) continue
        const [x, y] = middle(pieces[0])
        const off = Math.abs(x - centre[0]) + Math.abs(y - centre[1])
        if (!nearest || off < nearest.off) nearest = { word, pieces, off }
      }
    }
    if (!nearest)
      throw new Error('no word on the screen is broken over two lines')
    return nearest
  }

  it.each(SCROLLED)(
    'leaves alone a broken word no cover lies over, $name',
    async (flow) => {
      const { host, text } = await openFlow(flow, size)
      const { word, pieces } = brokenWord(host, text)
      const [first, second] = pieces
      // Level with one piece and across from the other, over neither.
      const cover = flow.vertical
        ? new DOMRect(second.left, first.top, second.width, first.height)
        : new DOMRect(first.left, second.top, first.width, second.height)
      for (const piece of pieces) expect(meets(piece, cover)).toBe(false)
      const before = scrolled(text)
      bringIntoSight(text, word, [cover])
      await wait(100)
      expect(scrolled(text)).toEqual(before)
    },
  )

  it.each(SCROLLED)(
    'moves a broken word a cover lies over, $name',
    async (flow) => {
      const { host, text } = await openFlow(flow, size)
      const { word, pieces } = brokenWord(host, text)
      const [first] = pieces
      const cover = new DOMRect(
        first.left - 2,
        first.top - 2,
        first.width + 4,
        first.height + 4,
      )
      const before = scrolled(text)
      bringIntoSight(text, word, [cover])
      await wait(100)
      expect(scrolled(text)).not.toEqual(before)
      for (const piece of word.getClientRects()) {
        expect(meets(piece, cover)).toBe(false)
      }
    },
  )
})

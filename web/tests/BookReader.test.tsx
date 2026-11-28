import { act } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { lookUpAt } from '../src/api'
import { bookImageUrl, CARDS_WAIT_MS, type CharacterCard } from '../src/book'
import { selectionIn } from '../src/bookNotes'
import { BookReader, Sentences } from '../src/BookReader'
import { GateContext, type Gated } from '../src/gate'
import { lookupModule } from '../src/modules/lookup'
import type { Note } from '../src/notes'
import { deferred, flush, land } from './support/async'
import { ADA, MARA, NED, naming } from './support/characters'
import './support/dialog'
import { typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { fakeStorage } from './support/storage'

vi.mock('../src/Account', () => ({
  Account: ({ visible }: { visible: boolean }) =>
    visible ? <div data-testid="account" /> : null,
}))

const book = {
  title: 'The Book',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
        { text: '“Who?” he asked.', heading: false, sentences: [[0, 16]] },
        { text: 'Nobody.', heading: false, sentences: [[0, 7]] },
        { text: 'CHAPTER TWO', heading: true, sentences: [[0, 11]] },
        { text: 'The End', heading: true, sentences: [[0, 7]] },
        { text: 'Done.', heading: false, sentences: [[0, 5]] },
      ],
    },
    ko: {
      label: 'Korean',
      language: 'ko',
      blocks: [
        { text: '1장', heading: true, sentences: [[0, 2]] },
        { text: '“누구?”', heading: false, sentences: [[0, 5]] },
        { text: '그가 물었다.', heading: false, sentences: [[0, 7]] },
        { text: '아무도.', heading: false, sentences: [[0, 4]] },
      ],
    },
  },
  alignments: [
    {
      to: 'ko',
      links: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
        [2, 3, 3, 4],
        [3, 6, 4, 4],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
        [2, 3, 3, 4],
        [3, 6, 4, 4],
      ],
      pins: [],
    },
  ],
}

/** An administrator, signed in from this browser, who may pin. */
const administrator: Gated = {
  who: {
    user_id: 1,
    admin: true,
    can_download: true,
    can_stream: true,
    can_stamp: false,
  },
  signOut: () => {},
}

/**
 * The server: `sent` as the book (a 500 for `null`), `saved` as the stored
 * position, and `notes` as the notes, each write of which it keeps and
 * answers with a fresh tag. `puts` are the positions written, `written` the
 * notes documents and `pinned` the pins.
 */
function server(
  saved?: { edition: string; slot: number },
  sent: unknown = book,
  notes: Note[] = [],
) {
  const puts: unknown[] = []
  const written: { notes: Note[] }[] = []
  const pinned: unknown[] = []
  let held = notes
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/api/works/w/book') {
      return sent === null
        ? new Response('gone', { status: 500 })
        : Response.json(sent)
    }
    if (url === '/api/works/w/book/pins') {
      pinned.push(JSON.parse(String(init?.body)))
      return new Response(null, { status: 204 })
    }
    // The library's answer for the work: filed in no series.
    if (url === '/api/library/works/w') return Response.json({ series: null })
    if (url === '/api/works/w/notes') {
      if (init?.method === 'PUT') {
        const doc = JSON.parse(String(init.body))
        written.push(doc)
        held = doc.notes
      }
      return Response.json(
        { work: 'w', notes: held },
        { headers: { etag: `"n${written.length}"` } },
      )
    }
    if (init?.method === 'PUT') {
      puts.push(JSON.parse(String(init.body)))
      return new Response(null, { status: 204 })
    }
    if (!saved) return new Response(null, { status: 204 })
    return Response.json({ positions: [{ ...saved, updated: 'now' }] })
  })
  return { fetch, puts, written, pinned }
}

/** `scrollIntoView`'s targets, by `data-block`. */
function scrolledTo(): string[] {
  const calls = vi.mocked(Element.prototype.scrollIntoView).mock.contexts
  return calls.map((el) => (el as HTMLElement).dataset.block ?? '')
}

/** Blocks before `first` are scrolled past the top of the text. */
function scrolledPast(first: number) {
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: Element) {
      const block = (this as HTMLElement).dataset?.block
      const bottom = block === undefined ? 0 : Number(block) < first ? -10 : 100
      return { top: 0, bottom } as DOMRect
    },
  )
}

async function opened(
  saved?: { edition: string; slot: number },
  sent: unknown = book,
  notes: Note[] = [],
  admin = false,
) {
  const { fetch, puts, written, pinned } = server(saved, sent, notes)
  vi.stubGlobal('fetch', fetch)
  Element.prototype.scrollIntoView = vi.fn()
  const onLeave = vi.fn()
  const reader = <BookReader work="w" onLeave={onLeave} />
  const host = mount(
    admin ? <GateContext value={administrator}>{reader}</GateContext> : reader,
  )
  await land()
  return { host, onLeave, puts, written, pinned, fetch }
}

function press(key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    cancelable: true,
    ...init,
  })
  act(() => {
    window.dispatchEvent(event)
  })
  return event
}

let clock = 0

/** When the next touch starts: each far enough from the last not to be a double tap. */
function later(): number {
  clock += 10_000
  return clock
}

/**
 * A press and release on `on`, the text by default, `travel` pixels down and
 * `sideways` across, `held` milliseconds long and starting at `at`, by the
 * kind of pointer `by` names where it names one.
 */
function touch(
  host: HTMLElement,
  {
    on,
    travel = 0,
    sideways = 0,
    held = 0,
    at = later(),
    x = 100,
    y = 100,
    by,
  }: {
    on?: Element | null
    travel?: number
    sideways?: number
    x?: number
    y?: number
    held?: number
    at?: number
    by?: 'mouse' | 'touch'
  } = {},
  /** Whether a tap's wait, `TAP_MS`, runs out before the next touch. */
  waited = true,
) {
  const target = on ?? host.querySelector('.book-text')
  const pointer = (type: string, x: number, y: number, time: number) => {
    const event = new PointerEvent(type, {
      clientX: x,
      clientY: y,
      bubbles: true,
      pointerType: by,
    })
    Object.defineProperty(event, 'timeStamp', { value: time })
    return event
  }
  const faked = vi.isFakeTimers()
  if (!faked) vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  act(() => {
    target?.dispatchEvent(pointer('pointerdown', x, y, at))
    target?.dispatchEvent(
      pointer('pointerup', x + sideways, y + travel, at + held),
    )
  })
  if (waited) act(() => vi.advanceTimersByTime(200))
  if (!faked) vi.useRealTimers()
}

const tap = (host: HTMLElement, travel = 0) => touch(host, { travel })

/** A double tap on block `block`, its taps 150 ms apart, starting at `at`. */
function doubleTap(host: HTMLElement, block: number, at = later()) {
  const on = host.querySelector(`.book-text [data-block="${block}"]`)
  touch(host, { on, at })
  touch(host, { on, at: at + 150 })
}

/**
 * A paged text 600 px wide, starting at the left edge, each block `i` on page
 * `pages[i]`, or running from one page to another, `[first, last]`, and each
 * figure, in the order drawn, on page `figures[i]`, or page 0; turned by
 * `scrollTo`, which scrolls as a browser would. When not paged, the blocks
 * before `above` are scrolled past. Answers the page shown.
 */
function paging(
  pages: (number | [number, number])[],
  above = 0,
  figures: number[] = [],
) {
  let left = 0
  const isText = (el: Element) => el.classList.contains('book-text')
  const span = (el: Element) => {
    const figure = [...document.querySelectorAll('[data-figure]')].indexOf(el)
    const on =
      figure >= 0
        ? (figures[figure] ?? 0)
        : (pages[Number((el as HTMLElement).dataset.block)] ?? 0)
    return typeof on === 'number' ? [on, on] : on
  }
  /** One line's box on page `page`, as the page shown places it. */
  const line = (page: number, bottom: number) => {
    const at = page * 600 - left + 24
    return { left: at, right: at + 552, top: 0, bottom } as DOMRect
  }
  const last = Math.max(
    ...pages.map((on) => (typeof on === 'number' ? on : on[1])),
    ...figures,
  )
  vi.spyOn(Element.prototype, 'clientWidth', 'get').mockImplementation(
    function (this: Element) {
      return isText(this) ? 600 : 0
    },
  )
  vi.spyOn(Element.prototype, 'scrollLeft', 'get').mockImplementation(
    () => left,
  )
  vi.spyOn(Element.prototype, 'scrollWidth', 'get').mockImplementation(
    function (this: Element) {
      return isText(this) ? (last + 1) * 600 : 0
    },
  )
  Element.prototype.scrollTo = vi.fn(function (
    this: Element,
    to?: ScrollToOptions | number,
  ) {
    const was = left
    left = typeof to === 'object' ? (to.left ?? left) : left
    if (left !== was) this.dispatchEvent(new Event('scroll'))
  }) as Element['scrollTo']
  vi.spyOn(Element.prototype, 'getClientRects').mockImplementation(function (
    this: Element,
  ) {
    const [first, end] = span(this)
    return [line(first, 100), line(end, 100)] as unknown as DOMRectList
  })
  // The reading area's box as the text's, a page that fills its window, so a
  // tap's thirds are the same of either.
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: Element) {
      if (isText(this) || this.classList.contains('book-area')) {
        return {
          left: 0,
          right: 600,
          width: 600,
          top: 0,
          bottom: 800,
        } as DOMRect
      }
      const block = Number((this as HTMLElement).dataset.block)
      const [first, end] = span(this)
      const bottom = block < above ? -10 : 100
      return {
        ...line(first, bottom),
        right: line(end, bottom).right,
      } as DOMRect
    },
  )
  return () => left / 600
}

const paged = (mode = 'scroll') =>
  localStorage.setItem(
    'taiyaku.prefs.book-layout',
    JSON.stringify({ mode, paged: true }),
  )

/** A note, `over` the parts a test cares about. */
const note = (over: Partial<Note>): Note => ({
  id: 'n',
  edition: 'en',
  slot: 0,
  body: '',
  color: 'amber',
  created: '2026-11-15T10:00:00.000Z',
  updated: '2026-11-15T10:00:00.000Z',
  ...over,
})

/** A highlight, id `h`, on `edition`'s block `slot` from `start` up to `end`. */
const highlightOn = (
  edition: string,
  slot: number,
  start: number,
  end: number,
) => note({ id: 'h', edition, slot, text: { start, end } })

/** Where character `offset` of block `block`'s own words is, past the asides set among them. */
function pointIn(
  host: HTMLElement,
  block: number,
  offset: number,
): [Node, number] {
  const words = host.querySelector(
    `[data-block="${block}"] [data-text]`,
  ) as Node
  const walker = document.createTreeWalker(words, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node.parentElement?.closest('[data-aside]')
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT,
  })
  let left = offset
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const length = node.textContent?.length ?? 0
    if (left <= length) return [node, left]
    left -= length
  }
  throw new Error(`block ${block} has no character ${offset}`)
}

/** Select block `block`'s own words from `start` up to `end`, across the spans its sentences are drawn in. */
function select(host: HTMLElement, block: number, start: number, end: number) {
  const range = document.createRange()
  range.setStart(...pointIn(host, block, start))
  range.setEnd(...pointIn(host, block, end))
  window.getSelection()?.removeAllRanges()
  window.getSelection()?.addRange(range)
}

/** Let the notes' write, a moment after the last change, land. */
async function saved() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 750))
  })
  await land()
}

/** The peek's text, and the language it is set in. */
/** `en`'s second block is two sentences, each of which `ko` gives a block. */
const sentenced = {
  title: 'Sentences',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
        {
          text: 'Who asked. Nobody knew.',
          heading: false,
          sentences: [
            [0, 10],
            [11, 23],
          ],
        },
      ],
    },
    ko: {
      label: 'Korean',
      language: 'ko',
      blocks: [
        { text: '1장', heading: true, sentences: [[0, 2]] },
        { text: '누가 물었다.', heading: false, sentences: [[0, 7]] },
        { text: '아무도 몰랐다.', heading: false, sentences: [[0, 8]] },
      ],
    },
  },
  alignments: [
    {
      to: 'ko',
      pins: [],
      links: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 2],
        [2, 3, 2, 3],
      ],
    },
  ],
}

/** `sentenced` with a French edition too, its second block two sentences as English's is. */
const three = {
  ...sentenced,
  editions: {
    ...sentenced.editions,
    fr: {
      label: 'French',
      language: 'fr',
      blocks: [
        { text: 'UN', heading: true, sentences: [[0, 2]] },
        {
          text: 'Qui demanda. Personne.',
          heading: false,
          sentences: [
            [0, 12],
            [13, 22],
          ],
        },
      ],
    },
  },
  alignments: [
    ...sentenced.alignments,
    {
      to: 'fr',
      pins: [],
      links: [
        [0, 1, 0, 1],
        [1, 2, 1, 2],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 2],
        [2, 3, 2, 3],
      ],
    },
  ],
}

/** `book` with English a short heading and a paragraph of 100 characters. */
const headed = {
  ...book,
  editions: {
    ...book.editions,
    en: {
      ...book.editions.en,
      blocks: [
        { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
        { text: 'x'.repeat(100), heading: false, sentences: [[0, 100]] },
      ],
    },
  },
}

const peeked = (host: HTMLElement) => {
  const peek = host.querySelector<HTMLElement>('.book-peek')
  return (
    peek && [
      peek.closest('[data-block]')?.getAttribute('data-block'),
      peek.lang,
      peek.textContent,
    ]
  )
}

/** The blocks lit. */
const lit = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>('.book-text [data-lit]')].map(
    (b) => b.dataset.block,
  )

/** The readout row's text, without its Contents button. */
const readout = (host: HTMLElement) =>
  host.querySelector('.reader-readout span')?.textContent

/** The edition drawn, told by its first block. */
const edition = (host: HTMLElement) =>
  host.querySelector('.book-text [data-block="0"]')?.textContent

/**
 * The top bar's layout button of these words. It throws on a miss, so a test
 * that switches by a name the bar does not show fails there, rather than
 * skipping the switch and passing on what follows.
 */
function layoutButton(host: HTMLElement, name: string): HTMLElement {
  const button = [
    ...host.querySelectorAll<HTMLElement>('[aria-label="Layout"] button'),
  ].find((b) => b.textContent === name)
  if (!button) throw new Error(`no layout button named "${name}"`)
  return button
}

/**
 * A speech engine that speaks nothing: what it was asked to say, each
 * utterance's `start` and `end` fired when a test says, and the voices it has.
 */
function voices(listed: { lang: string; name: string }[] = []) {
  const spoken: SpeechSynthesisUtterance[] = []
  const cancel = vi.fn()
  vi.stubGlobal('speechSynthesis', {
    getVoices: () => listed,
    speak: (u: SpeechSynthesisUtterance) => spoken.push(u),
    cancel,
  })
  vi.stubGlobal(
    'SpeechSynthesisUtterance',
    class {
      lang = ''
      rate = 1
      voice: unknown = null
      onstart?: () => void
      onend?: () => void
      onerror?: () => void
      readonly text: string
      constructor(text: string) {
        this.text = text
      }
    },
  )
  const starts = () =>
    act(() =>
      spoken.at(-1)?.onstart?.(new Event('start') as SpeechSynthesisEvent),
    )
  const ends = () =>
    act(() => spoken.at(-1)?.onend?.(new Event('end') as SpeechSynthesisEvent))
  return { spoken, cancel, starts, ends }
}

const litSentences = (host: HTMLElement) =>
  [...host.querySelectorAll('[data-sentence][data-lit]')].map(
    (el) => el.textContent,
  )

beforeEach(() => {
  vi.stubGlobal('localStorage', fakeStorage())
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('BookReader', () => {
  it('opens on the hub at its start, a heading drawn as one, the bars hidden', async () => {
    const { host } = await opened()
    expect(edition(host)).toBe('CHAPTER ONE')
    expect(host.querySelector('.book-text h2')?.textContent).toBe('CHAPTER ONE')
    expect(host.querySelectorAll('[data-block]')).toHaveLength(6)
    expect(scrolledTo()).toEqual(['0'])
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(document.title).toContain('The Book')
  })

  it('resumes the saved edition at its saved block', async () => {
    const { host } = await opened({ edition: 'ko', slot: 2 })
    expect(edition(host)).toBe('1장')
    expect(scrolledTo()).toEqual(['2'])
  })

  it('shows the bars on a tap: the title, the edition, and where the reader is', async () => {
    const { host } = await opened()
    tap(host)
    expect(host.querySelector('.reader-title-work')?.textContent).toBe(
      'The Book',
    )
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'English',
    )
    expect(readout(host)).toBe(
      '0% · <1m read · CHAPTER ONE · 0% of the chapter',
    )
    const layouts = host.querySelectorAll('[aria-label="Layout"] button')
    expect(
      [...layouts].map((b) => [b.textContent, b.getAttribute('aria-pressed')]),
    ).toEqual([
      ['One edition', 'true'],
      ['Compare editions', 'false'],
      ['Each sentence over its translation', 'false'],
      ['Page turning', 'false'],
    ])
    tap(host)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('takes a press that travels as a scroll, not a tap', async () => {
    const { host } = await opened()
    tap(host, 40)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('takes a press that moves as far as the page reader allows on each axis as a tap', async () => {
    const { host } = await opened()
    touch(host, { travel: 10, sideways: 10 })
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    touch(host, { travel: 11 })
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    touch(host, { sideways: 11 })
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  /** The keyboard brought the bars up, so they are its to take away, focus in them or not (O419). */
  it('shows the bars on Tab, and hides them on Escape', async () => {
    const { host, onLeave } = await opened()
    press('Tab')
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    press('Escape')
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(onLeave).not.toHaveBeenCalled()
  })

  /** A tap takes the bars from the keyboard: hidden by one and brought back by another, they are the tap's. */
  it('leaves the reader on Escape through bars a tap brought back after Tab showed them (O419)', async () => {
    const { host, onLeave } = await opened()
    press('Tab')
    tap(host)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    tap(host)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    press('Escape')
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('leaves the reader on Escape through bars a tap brought up (O419)', async () => {
    const { host, onLeave } = await opened()
    tap(host)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    press('Escape')
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('shows its ? its name and its key as hover text, as the comic reader’s does', async () => {
    const { host } = await opened()
    tap(host)
    expect(
      [...host.querySelectorAll('.reader-help')].map((b) => [
        b.getAttribute('aria-label'),
        b.getAttribute('title'),
      ]),
    ).toEqual([['Help', 'Help (?)']])
  })

  it('opens settings on s and help on ?, and Escape closes the panel before the bars', async () => {
    const { host } = await opened()
    tap(host)
    press('s')
    expect(host.querySelector('[aria-label="Settings"]')).not.toBeNull()
    press('?')
    const help = host.querySelector('[aria-label="Keyboard help"]')
    expect(help?.textContent).toContain(
      'switch to the next edition, at the same place',
    )
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
    press('Escape')
    expect(host.querySelector('[aria-label="Keyboard help"]')).toBeNull()
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('opens and closes settings from the bar, where the text is set and reset', async () => {
    const { host } = await opened()
    tap(host)
    const button = (text: string) =>
      [...host.querySelectorAll('button')].find((b) => b.textContent === text)
    act(() => button('Settings')?.click())
    const settings = host.querySelector('[aria-label="Settings"]')
    expect(settings).not.toBeNull()
    const size = [...(settings?.querySelectorAll('label') ?? [])].find(
      (label) => label.textContent?.includes('text size'),
    )
    const box = size?.querySelector('input') as HTMLInputElement
    await typeInto(box, '24')
    const style = () => host.querySelector<HTMLElement>('.book-text')?.style
    expect(style()?.getPropertyValue('--book-text-size')).toBe('24px')
    const text = [...(settings?.querySelectorAll('section') ?? [])].find((s) =>
      s.textContent?.startsWith('Text'),
    )
    act(() => text?.querySelector('button')?.click())
    expect(style()?.getPropertyValue('--book-text-size')).toBe('18px')
    act(() => button('Account…')?.click())
    expect(host.querySelector('[data-testid="account"]')).not.toBeNull()
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
    press('s')
    act(() => button('Close')?.click())
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
  })

  // O431: by the "Back from reading" value the route read as the book
  // opened; a book mounted with none is given the series page's.
  it.each([
    [{ putsSeriesUnder: true }, 'back to the series, else where you came from'],
    [{ putsSeriesUnder: false }, 'back where you came from, else the library'],
    [{}, 'back to the series, else where you came from'],
  ])(
    'names L in help by where the way back goes, opened with %o (O431)',
    async (props, words) => {
      vi.stubGlobal('fetch', server().fetch)
      Element.prototype.scrollIntoView = vi.fn()
      const host = mount(<BookReader work="w" onLeave={vi.fn()} {...props} />)
      await land()
      press('?')
      const row = [
        ...host.querySelectorAll('[aria-label="Keyboard help"] dl > div'),
      ].find((one) => one.querySelector('kbd')?.textContent === 'L')
      expect(row?.querySelector('dd')?.textContent).toBe(words)
    },
  )

  it('closes help from its own button', async () => {
    const { host } = await opened()
    press('?')
    const close = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    )
    act(() => close?.click())
    expect(host.querySelector('[aria-label="Keyboard help"]')).toBeNull()
  })

  it('opens the account panel on A', async () => {
    const { host } = await opened()
    press('A')
    expect(host.querySelector('[data-testid="account"]')).not.toBeNull()
  })

  it('sets the text as the book-text settings say', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ textSize: 24, lineSpacing: 2, columnWidth: 30 }),
    )
    const { host } = await opened()
    const style = host.querySelector<HTMLElement>('.book-text')?.style
    expect(style?.getPropertyValue('--book-text-size')).toBe('24px')
    expect(style?.getPropertyValue('--book-line-spacing')).toBe('2')
    expect(style?.getPropertyValue('--book-column-width')).toBe('720px')
  })

  it('sets the typeface, the page, justification and indents, and the language', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({
        typeface: 'sans',
        page: 'paper',
        justify: false,
        indent: true,
      }),
    )
    const { host } = await opened()
    const text = host.querySelector<HTMLElement>('.book-text')
    expect(text?.lang).toBe('en')
    expect(text?.dataset).toMatchObject({ typeface: 'sans', page: 'paper' })
    expect(text?.dataset.justify).toBeUndefined()
    expect(text?.dataset.indent).toBe('true')
    press(' ')
    expect(host.querySelector<HTMLElement>('.book-text')?.lang).toBe('ko')
  })

  it('justifies and spaces paragraphs by default', async () => {
    const { host } = await opened()
    const text = host.querySelector<HTMLElement>('.book-text')
    expect(text?.dataset).toMatchObject({
      typeface: 'serif',
      page: 'dark',
      justify: 'true',
      indent: 'true',
    })
  })

  it('lists the contents on t, a run of headings as one entry, and goes to one, staying open', async () => {
    const { host, puts } = await opened()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    press('t')
    const panel = host.querySelector('[aria-label="Contents"]')
    const entries = [...(panel?.querySelectorAll('li button') ?? [])]
    expect(entries.map((b) => b.textContent)).toEqual([
      'CHAPTER ONE',
      'CHAPTER TWO · The End',
    ])
    expect(entries.map((b) => b.getAttribute('aria-current'))).toEqual([
      'true',
      'false',
    ])
    act(() => (entries[1] as HTMLElement | null)?.click())
    expect(scrolledTo().at(-1)).toBe('3')
    const current = host.querySelector(
      '[aria-label="Contents"] [aria-current="true"]',
    )
    expect(current?.textContent).toBe('CHAPTER TWO · The End')
    await act(async () => {
      vi.advanceTimersByTime(1000)
    })
    vi.useRealTimers()
    await land()
    expect(puts).toEqual([{ edition: 'en', slot: 3 }])
    press('t')
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
    press('Tab')
    expect(readout(host)).toBe('60% · <1m read · The End · 0% of the chapter')
  })

  it('closes the open panel on a tap on the text, leaving the bars as they were', async () => {
    const { host } = await opened()
    press('t')
    tap(host, 40)
    expect(host.querySelector('[aria-label="Contents"]')).not.toBeNull()
    tap(host)
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('says an edition with no headings has no contents', async () => {
    const plain = {
      label: 'Plain',
      language: null,
      blocks: [{ text: 'x', heading: false, sentences: [[0, 1]] }],
    }
    const { host } = await opened(undefined, {
      ...book,
      hub: 'p',
      editions: { p: plain },
      alignments: [],
    })
    press('t')
    expect(
      host.querySelector('[aria-label="Contents"]')?.textContent,
    ).toContain('no headings')
    expect(host.querySelector<HTMLElement>('.book-text')?.lang).toBe('')
    press('Tab')
    expect(readout(host)).toBe('0% · <1m read')
  })

  it('opens and closes the contents from the bar', async () => {
    const { host } = await opened()
    tap(host)
    const button = (text: string) =>
      [...host.querySelectorAll('button')].find((b) => b.textContent === text)
    act(() => button('Contents')?.click())
    expect(host.querySelector('[aria-label="Contents"]')).not.toBeNull()
    expect(button('Contents')?.getAttribute('aria-expanded')).toBe('true')
    act(() => button('Close')?.click())
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
    act(() => button('Contents')?.click())
    act(() => button('Contents')?.click())
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
  })

  it('marks where each chapter starts on the progress line', async () => {
    const { host } = await opened()
    tap(host)
    const ticks = [
      ...host.querySelectorAll<HTMLElement>('.book-progress-tick'),
    ].map((tick) => tick.style.left)
    expect(ticks).toEqual(['0%', `${(34 / 57) * 100}%`])
  })

  it('lights the blocks a swap lands on, then puts the light out', async () => {
    const { host } = await opened()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    scrolledPast(1)
    press(' ')
    expect(lit(host)).toEqual(['1', '2'])
    act(() => vi.advanceTimersByTime(1499))
    expect(lit(host)).toEqual(['1', '2'])
    act(() => vi.advanceTimersByTime(1))
    expect(lit(host)).toEqual([])
    vi.useRealTimers()
  })

  it('lights nothing where the swap lands on no counterpart', async () => {
    const { host } = await opened({ edition: 'en', slot: 4 })
    scrolledPast(4)
    press(' ')
    expect(lit(host)).toEqual([])
  })

  it('fades the light as motion does, and not at all with motion off', async () => {
    const fade = (host: HTMLElement) =>
      host
        .querySelector<HTMLElement>('.book-text')
        ?.style.getPropertyValue('--book-lit-fade')
    expect(fade((await opened()).host)).toBe('600ms')
    localStorage.setItem(
      'taiyaku.prefs.motion',
      JSON.stringify({ animate: false, ms: 120 }),
    )
    expect(fade((await opened()).host)).toBe('0ms')
  })

  it('peeks on p at the next edition under the block at the top, until p again', async () => {
    const { host } = await opened()
    scrolledPast(1)
    press('p')
    expect(peeked(host)).toEqual(['1', 'ko', '“누구?”그가 물었다.'])
    expect(host.querySelector('.book-peek')?.getAttribute('aria-label')).toBe(
      'Korean',
    )
    press('p')
    expect(peeked(host)).toBeNull()
  })

  it('says when the next edition has nothing for the block peeked at', async () => {
    const { host } = await opened()
    scrolledPast(4)
    press('p')
    expect(peeked(host)).toEqual(['4', 'ko', 'Korean has nothing here.'])
  })

  it('peeks on a double tap at the block tapped, the bars as they were, and a double tap there again closes it', async () => {
    const { host } = await opened()
    doubleTap(host, 0)
    expect(peeked(host)).toEqual(['0', 'ko', '1장'])
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    doubleTap(host, 0)
    expect(peeked(host)).toBeNull()
  })

  it('closes a peek on a second double tap straight after, though the third click selected the paragraph', async () => {
    const { host } = await opened()
    const block = host.querySelector('.book-text [data-block="1"]') as Element
    doubleTap(host, 1, 50_000)
    expect(peeked(host)?.[0]).toBe('1')
    window.getSelection()?.selectAllChildren(block)
    doubleTap(host, 1, 50_300)
    expect(peeked(host)).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('waits 200 ms before a tap acts, and a second tap in that time stops it', async () => {
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]')
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    touch(host, { on, at: 60_000 }, false)
    act(() => vi.advanceTimersByTime(199))
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    touch(host, { on, at: 60_199 }, false)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    act(() => vi.advanceTimersByTime(1000))
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(peeked(host)?.[0]).toBe('1')
    touch(host, { on, at: 70_000 }, false)
    act(() => vi.advanceTimersByTime(200))
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    vi.useRealTimers()
  })

  it('takes two taps further apart than a double tap as two taps', async () => {
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]')
    touch(host, { on, at: 80_000 })
    touch(host, { on, at: 80_300 })
    expect(peeked(host)).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('takes a third quick tap as a tap of its own', async () => {
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]')
    for (const at of [90_000, 90_100, 90_200]) touch(host, { on, at })
    expect(peeked(host)?.[0]).toBe('1')
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('peeks on a double tap at a sentence at the next edition’s sentence for it alone, lit until it closes', async () => {
    const { host } = await opened(undefined, sentenced)
    const second = host.querySelector('[data-block="1"] [data-sentence="2"]')
    touch(host, { on: second, at: 60_000 })
    touch(host, { on: second, at: 60_150 })
    expect(peeked(host)).toEqual(['1', 'ko', '아무도 몰랐다.'])
    expect(second?.hasAttribute('data-lit')).toBe(true)
    expect(
      host.querySelector('[data-sentence="1"]')?.hasAttribute('data-lit'),
    ).toBe(false)
    press('Escape')
    expect(peeked(host)).toBeNull()
    expect(second?.hasAttribute('data-lit')).toBe(false)
  })

  it('sets a sentence’s peek straight after it, the rest of the paragraph after the peek', async () => {
    const { host } = await opened(undefined, sentenced)
    const first = host.querySelector('[data-block="1"] [data-sentence="1"]')
    touch(host, { on: first, at: 62_000 })
    touch(host, { on: first, at: 62_150 })
    const peek = host.querySelector('.book-peek')
    expect(peek?.textContent).toBe('누가 물었다.')
    expect(peek?.previousElementSibling).toBe(first)
    expect(peek?.nextElementSibling?.getAttribute('data-sentence')).toBe('2')
    expect(host.querySelectorAll('.book-peek')).toHaveLength(1)
  })

  it('counts a highlight made with a peek open in the block’s own words alone', async () => {
    const { host, written } = await opened(undefined, sentenced)
    const first = host.querySelector('[data-block="1"] [data-sentence="1"]')
    touch(host, { on: first, at: 64_000 })
    touch(host, { on: first, at: 64_150 })
    expect(host.querySelector('.book-peek')).not.toBeNull()
    select(host, 1, 11, 17)
    touch(host, { travel: 40, at: 65_000 })
    const bar = host.querySelector('[role="toolbar"][aria-label="Selection"]')
    act(() =>
      (
        bar?.querySelector('[aria-label="rose"]') as HTMLElement | null
      )?.click(),
    )
    await saved()
    expect(written.at(-1)?.notes).toMatchObject([
      { edition: 'en', slot: 1, text: { start: 11, end: 17 } },
    ])
  })

  it('sets each sentence’s translation straight after it, interlinear', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'interlinear' }),
    )
    const { host } = await opened(undefined, sentenced)
    const lines = [...host.querySelectorAll<HTMLElement>('.book-interlinear')]
    expect(
      lines.map((l) => [
        l.previousElementSibling?.getAttribute('data-sentence'),
        l.textContent,
        l.lang,
      ]),
    ).toEqual([
      ['0', '1장', 'ko'],
      ['1', '누가 물었다.', 'ko'],
      ['2', '아무도 몰랐다.', 'ko'],
    ])
    press('Tab')
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'English · Korean',
    )
  })

  it('shows a translation two sentences share once, after the second', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'interlinear' }),
    )
    const joined = {
      ...sentenced,
      alignments: [
        {
          ...sentenced.alignments[0],
          sentences: [
            [0, 1, 0, 1],
            [1, 3, 1, 3],
          ],
        },
      ],
    }
    const { host } = await opened(undefined, joined)
    const lines = [...host.querySelectorAll<HTMLElement>('.book-interlinear')]
    expect(lines.map((l) => [l.dataset.interlinear, l.textContent])).toEqual([
      ['0', '1장'],
      ['2', '누가 물었다. 아무도 몰랐다.'],
    ])
  })

  it('hides each translation until tapped, a tap there leaving the bars alone', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'interlinear', hideTranslation: true }),
    )
    const { host } = await opened(undefined, sentenced)
    const line = () =>
      host.querySelector<HTMLElement>('[data-interlinear="2"]') as HTMLElement
    expect(line().hasAttribute('data-hidden')).toBe(true)
    touch(host, { on: line(), at: 90_000 })
    expect(line().hasAttribute('data-hidden')).toBe(false)
    expect(
      host.querySelector('[data-interlinear="1"]')?.hasAttribute('data-hidden'),
    ).toBe(true)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    touch(host, { on: line(), at: 91_000 })
    expect(line().hasAttribute('data-hidden')).toBe(true)
    // A second tap straight after one that revealed a line peeks; the bars
    // it never showed stay as they were.
    touch(host, { on: line(), at: 92_000 })
    touch(host, { on: line(), at: 92_250 })
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('reads aloud on r from the sentence at the top, lighting each as it is read, until r again', async () => {
    const speech = voices([{ lang: 'en-GB', name: 'Daniel' }])
    const { host } = await opened(undefined, sentenced)
    scrolledPast(1)
    press('r')
    expect(speech.spoken.map((u) => [u.text, u.lang, u.rate])).toEqual([
      ['Who asked.', 'en', 1],
    ])
    speech.starts()
    expect(litSentences(host)).toEqual(['Who asked.'])
    speech.ends()
    speech.starts()
    expect(litSentences(host)).toEqual(['Nobody knew.'])
    press('r')
    expect(speech.cancel).toHaveBeenCalledTimes(1)
    expect(litSentences(host)).toEqual([])
  })

  it('moves nothing while reading aloud, compared or not, every sentence read being on the screen', async () => {
    for (const mode of ['scroll', 'compare']) {
      localStorage.setItem(
        'taiyaku.prefs.book-layout',
        JSON.stringify({ mode }),
      )
      const speech = voices([{ lang: 'en-GB', name: 'Daniel' }])
      const { host } = await opened(undefined, sentenced)
      // The whole book on one screen, every character of it measured there.
      vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
        function (this: Element) {
          return {
            top: 0,
            bottom: this.classList.contains('book-text') ? 800 : 100,
            left: 0,
            right: 600,
          } as DOMRect
        },
      )
      Object.defineProperty(Range.prototype, 'getClientRects', {
        configurable: true,
        value: () => [{ top: 50, bottom: 70, left: 10, right: 20 }],
      })
      onTestFinished(() => {
        Reflect.deleteProperty(Range.prototype, 'getClientRects')
      })
      const text = host.querySelector('.book-text') as HTMLElement
      text.scrollTo = vi.fn() as HTMLElement['scrollTo']
      vi.mocked(Element.prototype.scrollIntoView).mockClear()
      press('r')
      speech.starts()
      for (let i = 0; i < 2; i++) {
        speech.ends()
        speech.starts()
      }
      expect(speech.spoken.map((u) => u.text)).toEqual([
        'CHAPTER ONE',
        'Who asked.',
        'Nobody knew.',
      ])
      expect(vi.mocked(text.scrollTo)).not.toHaveBeenCalled()
      expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled()
      press('r')
      rerender(<div />)
    }
  })

  it('moves the text, compared, to each sentence read aloud that is off the screen', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const speech = voices([{ lang: 'en-GB', name: 'Daniel' }])
    const { host } = await opened(undefined, sentenced)
    // Every character measured below the screen, which ends at 800.
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        return {
          top: 0,
          bottom: this.classList.contains('book-text') ? 800 : 100,
          left: 0,
          right: 600,
        } as DOMRect
      },
    )
    Object.defineProperty(Range.prototype, 'getClientRects', {
      configurable: true,
      value: () => [{ top: 900, bottom: 920, left: 10, right: 20 }],
    })
    onTestFinished(() => {
      Reflect.deleteProperty(Range.prototype, 'getClientRects')
    })
    const text = host.querySelector('.book-text') as HTMLElement
    text.scrollTo = vi.fn() as HTMLElement['scrollTo']
    vi.mocked(Element.prototype.scrollIntoView).mockClear()
    press('r')
    speech.starts()
    // A block's first sentence is placed by the row holding the block.
    expect(speech.spoken.at(-1)?.text).toBe('CHAPTER ONE')
    expect(scrolledTo()).toEqual(['0'])
    speech.ends()
    speech.starts()
    expect(speech.spoken.at(-1)?.text).toBe('Who asked.')
    expect(scrolledTo()).toEqual(['0', '1'])
    // A later one, by its first character, measured at 900.
    expect(vi.mocked(text.scrollTo)).not.toHaveBeenCalled()
    speech.ends()
    speech.starts()
    expect(speech.spoken.at(-1)?.text).toBe('Nobody knew.')
    expect(vi.mocked(text.scrollTo).mock.calls).toEqual([[{ top: 900 }]])
    press('r')
  })

  it('stops reading aloud on Escape before anything else Escape closes', async () => {
    const speech = voices()
    const { host } = await opened(undefined, sentenced)
    press('Tab')
    press('r')
    speech.starts()
    press('Escape')
    expect(speech.cancel).toHaveBeenCalledTimes(1)
    expect(litSentences(host)).toEqual([])
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('stops reading aloud on a swap, and on leaving the book', async () => {
    const speech = voices()
    await opened(undefined, sentenced)
    press('r')
    press(' ')
    expect(speech.cancel).toHaveBeenCalledTimes(1)
    press('r')
    rerender(<div />)
    expect(speech.cancel).toHaveBeenCalledTimes(2)
  })

  it('says a device has no voice for the book’s language rather than reading it in another', async () => {
    const speech = voices([{ lang: 'ja-JP', name: 'Kyoko' }])
    const { host } = await opened(undefined, sentenced)
    press('r')
    expect(speech.spoken).toEqual([])
    expect(host.querySelector('.book-notice')?.textContent).toBe(
      'This device has no voice for en.',
    )
  })

  it('says when the device could not read the book aloud', async () => {
    const speech = voices()
    const { host } = await opened(undefined, sentenced)
    press('r')
    const failed = new Event('error') as SpeechSynthesisErrorEvent
    Object.defineProperty(failed, 'error', { value: 'synthesis-failed' })
    act(() => speech.spoken[0]?.onerror?.(failed))
    expect(host.querySelector('.book-notice')?.textContent).toBe(
      'This device could not read en aloud.',
    )
    expect(litSentences(host)).toEqual([])
  })

  it('says a browser with no speech at all has no voice, and Escape still closes a panel', async () => {
    vi.stubGlobal('speechSynthesis', undefined)
    const { host } = await opened(undefined, sentenced)
    const notice = () => host.querySelector('.book-notice')?.textContent
    press('r')
    expect(notice()).toBe('This device has no voice for en.')
    press('s')
    expect(host.querySelector('[aria-label="Settings"]')).not.toBeNull()
    press('Escape')
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
    press('r')
    expect(notice()).toBe('This device has no voice for en.')
  })

  it('keeps no stop for a reading that ended before it began, so Escape and r carry on', async () => {
    voices()
    const spoken: SpeechSynthesisUtterance[] = []
    const failed = new Event('error') as SpeechSynthesisErrorEvent
    Object.defineProperty(failed, 'error', { value: 'synthesis-failed' })
    vi.stubGlobal('speechSynthesis', {
      getVoices: () => [],
      speak: (u: SpeechSynthesisUtterance) => {
        spoken.push(u)
        u.onerror?.(failed)
      },
      cancel: vi.fn(),
    })
    const { host } = await opened(undefined, sentenced)
    press('r')
    expect(spoken).toHaveLength(1)
    expect(host.querySelector('.book-notice')?.textContent).toBe(
      'This device could not read en aloud.',
    )
    press('s')
    press('Escape')
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
    press('r')
    expect(spoken).toHaveLength(2)
  })

  it('reads at the pace set, kept between half and twice the voice’s own', async () => {
    localStorage.setItem(
      'taiyaku.prefs.read-aloud',
      JSON.stringify({ rate: 5 }),
    )
    const speech = voices()
    await opened(undefined, sentenced)
    press('r')
    expect(speech.spoken[0]?.rate).toBe(2)
  })

  it('sets a verse line’s printed number in the margin, and reads out the line at the top', async () => {
    const lines = [
      'Arms and the man I sing,',
      'who first from Troy,',
      'came to Italy.',
    ]
    const verse = {
      ...sentenced,
      editions: {
        ...sentenced.editions,
        en: {
          ...sentenced.editions.en,
          blocks: [
            { text: 'BOOK ONE', heading: true, sentences: [[0, 8]] },
            ...lines.map((text, i) => ({
              text,
              heading: false,
              sentences: [[0, text.length]],
              line: i === 0 ? 5 : null,
            })),
          ],
        },
      },
    }
    const { host } = await opened(undefined, verse)
    expect(
      [...host.querySelectorAll<HTMLElement>('[data-line]')].map(
        (p) => p.dataset.line,
      ),
    ).toEqual(['5'])
    scrolledPast(2)
    act(() => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
    })
    press('Tab')
    expect(readout(host)).toContain('· line 6 ·')
  })

  it('peeks on p at every sentence of the block at the top, as before', async () => {
    const { host } = await opened(undefined, sentenced)
    scrolledPast(1)
    press('p')
    expect(peeked(host)).toEqual(['1', 'ko', '누가 물었다.아무도 몰랐다.'])
  })

  it('lights, compared, a sentence double-tapped and what the other side has for it, until Escape', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, sentenced)
    const lit = () =>
      [...host.querySelectorAll('[data-sentence][data-lit]')].map(
        (el) => el.textContent,
      )
    const second = host.querySelector(
      '[data-pick="left:1"] [data-sentence="2"]',
    )
    touch(host, { on: second, at: 70_000 })
    touch(host, { on: second, at: 70_150 })
    expect(lit()).toEqual(['Nobody knew.', '아무도 몰랐다.'])
    const right = host.querySelector(
      '[data-pick="right:1"] [data-sentence="1"]',
    )
    touch(host, { on: right, at: 71_000 })
    touch(host, { on: right, at: 71_150 })
    expect(lit()).toEqual(['Who asked.', '누가 물었다.'])
    press('Escape')
    expect(lit()).toEqual([])
  })

  it('peeks on a double tap with a panel open, closing the panel and leaving the bars', async () => {
    const { host } = await opened()
    press('t')
    doubleTap(host, 1)
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(peeked(host)?.[0]).toBe('1')
  })

  it('takes back the word a double-click selects', async () => {
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]') as Element
    touch(host, { on, at: 70_000 })
    window.getSelection()?.selectAllChildren(on)
    touch(host, { on, at: 70_150 })
    expect(window.getSelection()?.isCollapsed).toBe(true)
    expect(peeked(host)?.[0]).toBe('1')
  })

  it('takes a long press or a sideways drag as selecting, not a tap', async () => {
    const { host } = await opened()
    touch(host, { held: 600 })
    touch(host, { sideways: 40 })
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    touch(host, { held: 400 })
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('peeks at nothing on p compared, which draws no peek, so Escape closes the panel, and lets go of a peek as compare starts', async () => {
    const { host } = await opened()
    press('p')
    expect(host.querySelector('.book-peek')).not.toBeNull()
    press('Tab')
    act(() => layoutButton(host, 'Compare editions').click())
    press('s')
    expect(host.querySelector('[aria-label="Settings"]')).not.toBeNull()
    press('p')
    press('Escape')
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
    act(() => layoutButton(host, 'One edition').click())
    expect(host.querySelector('.book-peek')).toBeNull()
  })

  it('closes a peek on Escape before the panel, and on a swap', async () => {
    const { host } = await opened()
    scrolledPast(1)
    press('s')
    press('p')
    press('Escape')
    expect(peeked(host)).toBeNull()
    expect(host.querySelector('[aria-label="Settings"]')).not.toBeNull()
    press('p')
    expect(peeked(host)?.[0]).toBe('1')
    press(' ')
    expect(edition(host)).toBe('1장')
    expect(peeked(host)).toBeNull()
  })

  it('compares the edition read with the next, a row to each passage, each set in its language', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened()
    const rowsDrawn = [...host.querySelectorAll<HTMLElement>('.book-row')]
    expect(
      rowsDrawn.map((row) => [
        row.dataset.block,
        ...[...row.children].map((cell) => cell.textContent),
      ]),
    ).toEqual([
      ['0', 'CHAPTER ONE', '1장'],
      ['1', '“Who?” he asked.', '“누구?”그가 물었다.'],
      ['2', 'Nobody.', '아무도.'],
      ['3', 'CHAPTER TWOThe EndDone.', ''],
    ])
    expect(
      [...rowsDrawn[0].children].map((cell) => (cell as HTMLElement).lang),
    ).toEqual(['en', 'ko'])
    expect(rowsDrawn[0].querySelector('h2')?.textContent).toBe('CHAPTER ONE')
    tap(host)
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'English · Korean',
    )
    press(' ')
    expect(host.querySelector('.book-row')?.textContent).toBe('1장CHAPTER ONE')
  })

  it('switches layout from the bar, keeping the block at the top', async () => {
    const { host } = await opened()
    tap(host)
    scrolledPast(2)
    act(() => layoutButton(host, 'Compare editions').click())
    expect(host.querySelectorAll('.book-row')).toHaveLength(4)
    expect(scrolledTo().at(-1)).toBe('2')
    expect(localStorage.getItem('taiyaku.prefs.book-layout')).toContain(
      'compare',
    )
    act(() => layoutButton(host, 'One edition').click())
    expect(host.querySelectorAll('.book-row')).toHaveLength(0)
    expect(edition(host)).toBe('CHAPTER ONE')
    expect(scrolledTo().at(-1)).toBe('2')
  })

  it('reads the block at the top off the row it is in, and goes back to a block inside a row', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host, puts } = await opened({ edition: 'ko', slot: 2 })
    expect(
      [...host.querySelectorAll<HTMLElement>('.book-row')].map(
        (row) => row.dataset.block,
      ),
    ).toEqual(['0', '1', '3', '4'])
    expect(scrolledTo().at(-1)).toBe('1')
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    scrolledPast(3)
    await act(async () => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
      vi.advanceTimersByTime(1000)
    })
    vi.useRealTimers()
    await land()
    expect(puts).toEqual([{ edition: 'ko', slot: 3 }])
  })

  it('keeps the block at the top when a setting redraws the text', async () => {
    const { host } = await opened()
    press('s')
    scrolledPast(3)
    const size = [...host.querySelectorAll('label')].find((label) =>
      label.textContent?.includes('text size'),
    )
    await typeInto(size?.querySelector('input') as HTMLInputElement, '24')
    expect(scrolledTo().at(-1)).toBe('3')
  })

  it('offers a book of one edition only the paged switch, and scrolls it though compare was chosen', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const plain = {
      label: 'Plain',
      language: null,
      blocks: [{ text: 'x', heading: false, sentences: [[0, 1]] }],
    }
    const { host } = await opened(undefined, {
      ...book,
      hub: 'p',
      editions: { p: plain },
      alignments: [],
    })
    expect(host.querySelectorAll('.book-row')).toHaveLength(0)
    tap(host)
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'Plain',
    )
    expect(
      [...host.querySelectorAll('[aria-label="Layout"] button')].map(
        (b) => b.textContent,
      ),
    ).toEqual(['Page turning'])
  })

  it('lays a paged book out in pages, and opens on the page its saved block is on', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened({ edition: 'en', slot: 3 })
    expect(host.querySelector<HTMLElement>('.book-text')?.dataset.paged).toBe(
      'true',
    )
    expect(page()).toBe(1)
  })

  it('opens on the page a saved block starts on, though it runs onto the next', async () => {
    paged()
    const page = paging([0, 0, [1, 2], 3, 3, 3])
    await opened({ edition: 'en', slot: 2 })
    expect(page()).toBe(1)
  })

  it('turns a page on the arrows and Page Up and Down, saving the block it opens on', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host, puts } = await opened()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    expect(press('ArrowRight').defaultPrevented).toBe(true)
    expect(page()).toBe(1)
    press('PageDown')
    expect(page()).toBe(2)
    press('ArrowLeft')
    press('PageUp')
    press('PageDown')
    expect(page()).toBe(1)
    await act(async () => {
      vi.advanceTimersByTime(1000)
    })
    vi.useRealTimers()
    await land()
    expect(puts).toEqual([{ edition: 'en', slot: 2 }])
    touch(host, { x: 300 })
    expect(readout(host)).toBe(
      'page 2 of 3 · <1m read · CHAPTER ONE · page 2 of 2',
    )
  })

  it('turns a page on a tap on either outer third, and shows the bars on one between', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    touch(host, { x: 500 })
    expect(page()).toBe(1)
    touch(host, { x: 100 })
    expect(page()).toBe(0)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    touch(host, { x: 300 })
    expect(page()).toBe(0)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('turns a paged book’s page at once on a tap on an outer third, not waiting for a second', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    touch(host, { x: 500 }, false)
    expect(page()).toBe(1)
    touch(host, { x: 100 }, false)
    expect(page()).toBe(0)
  })

  it('turns two pages on a double tap on a paged book’s outer third, and peeks on one in its middle third', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]')
    touch(host, { on, x: 500, at: 95_000 }, false)
    touch(host, { on, x: 500, at: 95_150 })
    expect(page()).toBe(2)
    expect(peeked(host)).toBeNull()
    touch(host, { on, x: 300, at: 96_000 }, false)
    touch(host, { on, x: 300, at: 96_150 })
    expect(page()).toBe(2)
    expect(peeked(host)?.[0]).toBe('1')
  })

  it('takes the click of a tap that turns a paged book’s page, so it presses nothing the new page draws under the finger', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened({ edition: 'en', slot: 2 }, linked)
    expect(page()).toBe(1)
    touch(host, { x: 100 }, false)
    expect(page()).toBe(0)
    // A touch screen's click comes after the turn, on the web link the page
    // turned to has drawn where the finger was.
    const click = new MouseEvent('click', {
      bubbles: true,
      cancelable: true,
      detail: 1,
    })
    act(() => {
      linkIn(host, 0)?.dispatchEvent(click)
    })
    expect(click.defaultPrevented).toBe(true)
  })

  it('takes back the word a double-click on a paged book’s outer third selects, turning two pages', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]') as Element
    touch(host, { on, x: 500, at: 97_000 }, false)
    window.getSelection()?.selectAllChildren(on)
    touch(host, { on, x: 500, at: 97_150 })
    expect(page()).toBe(2)
    expect(window.getSelection()?.isCollapsed).toBe(true)
  })

  it('lights, compared and paged, the sentence a double tap on an outer third is on, turning nothing', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare', paged: true }),
    )
    const page = paging([0, 0, 0])
    const { host } = await opened(undefined, sentenced)
    const lit = () =>
      [...host.querySelectorAll('[data-sentence][data-lit]')].map(
        (el) => el.textContent,
      )
    const second = host.querySelector(
      '[data-pick="left:1"] [data-sentence="2"]',
    )
    touch(host, { on: second, x: 500, at: 70_000 }, false)
    touch(host, { on: second, x: 500, at: 70_150 })
    expect(page()).toBe(0)
    expect(lit()).toEqual(['Nobody knew.', '아무도 몰랐다.'])
  })

  it('turns a compared, paged book’s page when a tap’s wait on an outer third runs out, and a second tap then lights its sentence, the bars left down', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare', paged: true }),
    )
    const page = paging([0, 0, 0])
    const { host } = await opened(undefined, sentenced)
    const lit = () =>
      [...host.querySelectorAll('[data-sentence][data-lit]')].map(
        (el) => el.textContent,
      )
    const second = () =>
      host.querySelector('[data-pick="left:1"] [data-sentence="2"]')
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    touch(host, { on: second(), x: 500, at: 75_000 }, false)
    expect(page()).toBe(0)
    act(() => vi.advanceTimersByTime(200))
    expect(page()).toBe(1)
    touch(host, { on: second(), x: 500, at: 75_250 }, false)
    act(() => vi.advanceTimersByTime(1000))
    expect(page()).toBe(1)
    expect(lit()).toEqual(['Nobody knew.', '아무도 몰랐다.'])
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    vi.useRealTimers()
  })

  it('waits, paged, on a tap on an outer third with a panel open, closing the panel and turning nothing', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    press('t')
    expect(host.querySelector('[aria-label="Contents"]')).not.toBeNull()
    touch(host, { x: 500 })
    expect(page()).toBe(0)
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
  })

  it('opens a note, paged, on a tap on its mark in an outer third, turning nothing', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, book, [highlightOn('en', 1, 0, 3)])
    touch(host, { on: host.querySelector('mark'), x: 500 })
    expect(page()).toBe(0)
    expect(
      host.querySelector('[role="dialog"][aria-label="Note"]'),
    ).not.toBeNull()
  })

  it('shows a hidden translation, paged, on a tap on it in an outer third, turning nothing', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({
        mode: 'interlinear',
        hideTranslation: true,
        paged: true,
      }),
    )
    const page = paging([0, 0, 0])
    const { host } = await opened(undefined, sentenced)
    const line = () =>
      host.querySelector('[data-interlinear="2"]') as HTMLElement
    expect(line().hasAttribute('data-hidden')).toBe(true)
    touch(host, { on: line(), x: 500 })
    expect(page()).toBe(0)
    expect(line().hasAttribute('data-hidden')).toBe(false)
  })

  it('goes back to the block at the top when the window is resized', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    await opened()
    press('ArrowRight')
    expect(page()).toBe(1)
    act(() => {
      ;(
        Element.prototype.scrollTo as unknown as { mockClear(): void }
      ).mockClear()
      window.dispatchEvent(new Event('resize'))
    })
    expect(Element.prototype.scrollTo).toHaveBeenCalledWith({ left: 600 })
  })

  it('pages on the switch in the bar, and back, keeping the block at the top', async () => {
    const page = paging([0, 0, 1, 1, 2, 2], 3)
    const { host } = await opened({ edition: 'en', slot: 3 })
    touch(host, { x: 300 })
    const switched = () =>
      [...host.querySelectorAll('[aria-label="Layout"] button')].find(
        (b) => b.textContent === 'Page turning',
      ) as HTMLElement
    expect(switched().getAttribute('aria-pressed')).toBe('false')
    expect(readout(host)).toMatch(/^\d+% · /)
    act(() => switched().click())
    expect(readout(host)).toMatch(/^page 2 of 3 · /)
    expect(host.querySelector<HTMLElement>('.book-text')?.dataset.paged).toBe(
      'true',
    )
    expect(switched().getAttribute('aria-pressed')).toBe('true')
    expect(page()).toBe(1)
    expect(localStorage.getItem('taiyaku.prefs.book-layout')).toBe(
      JSON.stringify({ mode: 'scroll', paged: true, hideTranslation: false }),
    )
    act(() => switched().click())
    expect(
      host.querySelector<HTMLElement>('.book-text')?.dataset.paged,
    ).toBeUndefined()
  })

  it('pages two editions compared, a page turning both', async () => {
    paged('compare')
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const text = host.querySelector<HTMLElement>('.book-text')
    expect(text?.dataset).toMatchObject({ paged: 'true', compared: 'true' })
    expect(host.querySelectorAll('.book-row')).toHaveLength(4)
    press('ArrowRight')
    expect(page()).toBe(1)
  })

  it('counts pages in the bar when paged, in the book and in its chapter', async () => {
    paged()
    // Chapter one's last block runs from page 2 onto page 3; chapter two
    // starts on page 4.
    paging([0, 0, [1, 2], 3, 3, 3])
    const { host } = await opened()
    touch(host, { x: 300 })
    expect(readout(host)).toBe(
      'page 1 of 4 · <1m read · CHAPTER ONE · page 1 of 3',
    )
    press('ArrowRight')
    expect(readout(host)).toBe(
      'page 2 of 4 · <1m read · CHAPTER ONE · page 2 of 3',
    )
    press('ArrowRight')
    expect(readout(host)).toBe(
      'page 3 of 4 · <1m read · CHAPTER ONE · page 3 of 3',
    )
    press('ArrowRight')
    expect(readout(host)).toBe('page 4 of 4 · <1m read · The End · page 1 of 1')
  })

  it('sets the text larger on = and +, smaller on -, and back on 0, keeping the block at the top', async () => {
    const { host } = await opened()
    const size = () =>
      host
        .querySelector<HTMLElement>('.book-text')
        ?.style.getPropertyValue('--book-text-size')
    scrolledPast(3)
    press('=')
    expect(size()).toBe('20px')
    press('+')
    expect(size()).toBe('22px')
    press('-')
    expect(size()).toBe('20px')
    expect(scrolledTo().at(-1)).toBe('3')
    press('0')
    expect(size()).toBe('18px')
  })

  /** Animation frames run only when `frame` is called, and none cancelled before then. */
  function frames() {
    const waiting = new Map<number, FrameRequestCallback>()
    // From 1, as a browser's are: the reader keeps 0 for no frame awaited.
    let made = 0
    vi.stubGlobal('requestAnimationFrame', (run: FrameRequestCallback) => {
      waiting.set(++made, run)
      return made
    })
    vi.stubGlobal('cancelAnimationFrame', (frame: number) => {
      waiting.delete(frame)
    })
    return () =>
      act(() => {
        const due = [...waiting.values()]
        waiting.clear()
        for (const run of due) run(0)
      })
  }

  /** A notch's worth or part of one, `deltaY` pixels, turned on `text`. */
  function wheelOn(text: HTMLElement, deltaY: number) {
    const event = new WheelEvent('wheel', {
      deltaY,
      deltaMode: 0,
      cancelable: true,
    })
    act(() => {
      text.dispatchEvent(event)
    })
    return event
  }

  it('sets a paged book’s text larger on the wheel up and smaller down, a step a notch, adding up a smooth wheel', async () => {
    const frame = frames()
    paged()
    paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    const size = () => text.style.getPropertyValue('--book-text-size')
    const wheel = (deltaY: number) => wheelOn(text, deltaY)
    expect(wheel(-100).defaultPrevented).toBe(true)
    expect(size()).toBe('20px')
    // The frame after a step ends the wait, so the part notch below is taken
    // up at once: turned while a frame is awaited, it would wait for the notch
    // that completes it, and a part notch that steps alone would go unseen.
    frame()
    wheel(-60)
    expect(size()).toBe('20px')
    wheel(-40)
    expect(size()).toBe('22px')
    frame()
    wheel(200)
    expect(size()).toBe('18px')
    // A part notch left over after a step is kept for the notches after it.
    frame()
    wheel(150)
    expect(size()).toBe('16px')
    frame()
    wheel(50)
    expect(size()).toBe('14px')
  })

  it('steps a paged book’s text at once on a lone notch, and the notches turned before the next frame together when it comes', async () => {
    const frame = frames()
    paged()
    paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    const size = () => text.style.getPropertyValue('--book-text-size')
    wheelOn(text, -100)
    expect(size()).toBe('20px')
    wheelOn(text, -100)
    wheelOn(text, -100)
    wheelOn(text, -100)
    expect(size()).toBe('20px')
    frame()
    expect(size()).toBe('26px')
    // A frame with no notch waiting ends the wait: the next notch steps at once.
    frame()
    wheelOn(text, 100)
    expect(size()).toBe('24px')
  })

  it('steps no notch still waiting for its frame, and takes no wheel, once the book is no longer paged', async () => {
    const frame = frames()
    paged()
    paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    touch(host, { x: 300 })
    const text = () => host.querySelector('.book-text') as HTMLElement
    const size = () => text().style.getPropertyValue('--book-text-size')
    wheelOn(text(), -100)
    wheelOn(text(), -100)
    expect(size()).toBe('20px')
    const switched = [
      ...host.querySelectorAll('[aria-label="Layout"] button'),
    ].find((b) => b.textContent === 'Page turning') as HTMLElement
    act(() => switched.click())
    expect(text().dataset.paged).toBeUndefined()
    frame()
    expect(size()).toBe('20px')
    // Scrolled, the wheel scrolls the text: the paged wheel's listener has gone.
    expect(wheelOn(text(), -100).defaultPrevented).toBe(false)
  })

  it('leaves a compared, paged reader on its page when + is pressed at the largest size', async () => {
    paged('compare')
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ textSize: 40 }),
    )
    // The second row, block 1 beside ko 1-2, runs from page 0 to page 2.
    const page = paging([0, [0, 2], 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    press('ArrowRight')
    expect(page()).toBe(1)
    press('+')
    expect(text.style.getPropertyValue('--book-text-size')).toBe('40px')
    expect(page()).toBe(1)
  })

  it('leaves a compared, paged reader on its page when - is pressed at the smallest size', async () => {
    paged('compare')
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ textSize: 12 }),
    )
    const page = paging([0, [0, 2], 2, 2])
    await opened()
    press('ArrowRight')
    expect(page()).toBe(1)
    press('-')
    expect(page()).toBe(1)
  })

  it('leaves a compared, paged reader on its page when 0 is pressed at the usual size', async () => {
    paged('compare')
    const page = paging([0, [0, 2], 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    press('ArrowRight')
    expect(page()).toBe(1)
    press('0')
    expect(text.style.getPropertyValue('--book-text-size')).toBe('18px')
    expect(page()).toBe(1)
  })

  it('leaves a compared, paged reader on its page when the wheel turns up at the largest size', async () => {
    paged('compare')
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ textSize: 40 }),
    )
    const page = paging([0, [0, 2], 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    press('ArrowRight')
    expect(page()).toBe(1)
    wheelOn(text, -100)
    expect(text.style.getPropertyValue('--book-text-size')).toBe('40px')
    expect(page()).toBe(1)
  })

  it('leaves a compared, paged reader on its page when the wheel turns down at the smallest size', async () => {
    paged('compare')
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ textSize: 12 }),
    )
    const page = paging([0, [0, 2], 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    press('ArrowRight')
    expect(page()).toBe(1)
    wheelOn(text, 100)
    expect(text.style.getPropertyValue('--book-text-size')).toBe('12px')
    expect(page()).toBe(1)
  })

  it('leaves the wheel to scroll a book that is not paged', async () => {
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    const event = new WheelEvent('wheel', { deltaY: -100, cancelable: true })
    act(() => {
      text.dispatchEvent(event)
    })
    expect(event.defaultPrevented).toBe(false)
    expect(text.style.getPropertyValue('--book-text-size')).toBe('18px')
  })

  it('moves a screen less a line on the arrows when the book scrolls', async () => {
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(500)
    text.scrollBy = vi.fn()
    press('ArrowRight')
    press('PageUp')
    expect(vi.mocked(text.scrollBy).mock.calls).toEqual([
      [{ top: 450 }],
      [{ top: -450 }],
    ])
  })

  it('writes a note on the paragraph at the top on n, kept a moment after the last word', async () => {
    const { host, written } = await opened()
    scrolledPast(1)
    press('n')
    const box = host.querySelector<HTMLTextAreaElement>(
      '[role="dialog"][aria-label="Note"] textarea',
    )
    expect(box).not.toBeNull()
    act(() => box?.focus())
    await typeInto(box as HTMLTextAreaElement, 'who asks?')
    act(() => box?.blur())
    await saved()
    expect(written.at(-1)?.notes).toMatchObject([
      { edition: 'en', slot: 1, body: 'who asks?', color: 'amber' },
    ])
    expect(written.at(-1)?.notes[0]).not.toHaveProperty('text')
    expect(
      host
        .querySelector('[data-block="1"] .book-note-mark')
        ?.getAttribute('title'),
    ).toBe('who asks?')
  })

  it('offers a selection’s words as a highlight, and marks them in its colour', async () => {
    const { host, written } = await opened()
    select(host, 1, 1, 6)
    touch(host, { travel: 40 })
    const bar = host.querySelector('[role="toolbar"][aria-label="Selection"]')
    act(() =>
      (
        bar?.querySelector('[aria-label="rose"]') as HTMLElement | null
      )?.click(),
    )
    expect(host.querySelector('[role="toolbar"]')).toBeNull()
    expect(host.querySelector('[data-block="1"] mark')?.textContent).toBe(
      'Who?”',
    )
    await saved()
    expect(written.at(-1)?.notes).toMatchObject([
      { edition: 'en', slot: 1, text: { start: 1, end: 6 }, color: 'rose' },
    ])
  })

  it('lets go of a selection on Escape, its words and its bar both', async () => {
    const { host } = await opened()
    select(host, 1, 1, 6)
    touch(host, { travel: 40 })
    expect(host.querySelector('[role="toolbar"]')).not.toBeNull()
    press('Escape')
    expect(host.querySelector('[role="toolbar"]')).toBeNull()
    expect(window.getSelection()?.isCollapsed).toBe(true)
  })

  it('writes a note on a selection from the bar, and keeps a selection to its first paragraph', async () => {
    const { host, written } = await opened()
    const from = host.querySelector(
      '[data-block="1"] [data-sentence]',
    )?.firstChild
    const to = host.querySelector(
      '[data-block="2"] [data-sentence]',
    )?.firstChild
    const range = document.createRange()
    range.setStart(from as Node, 7)
    range.setEnd(to as Node, 3)
    window.getSelection()?.removeAllRanges()
    window.getSelection()?.addRange(range)
    touch(host, { travel: 40 })
    const bar = host.querySelector('[role="toolbar"]')
    const note = [...(bar?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent === 'Note',
    )
    act(() => note?.click())
    expect(
      host.querySelector('[role="dialog"][aria-label="Note"] .book-note-quote')
        ?.textContent,
    ).toBe('he asked.')
    await saved()
    expect(written.at(-1)?.notes[0].text).toEqual({ start: 7, end: 16 })
  })

  it('opens a note from its highlight or its mark, edits its colour, and deletes it', async () => {
    const { host, written } = await opened(undefined, book, [
      highlightOn('en', 1, 0, 3),
      note({ id: 'p', edition: 'en', slot: 2, body: 'gone' }),
    ])
    touch(host, { on: host.querySelector('mark') })
    const dialog = () =>
      host.querySelector('[role="dialog"][aria-label="Note"]')
    expect(dialog()?.querySelector('.book-note-quote')?.textContent).toBe('“Wh')
    act(() =>
      (
        dialog()?.querySelector('[aria-label="sky"]') as HTMLElement | null
      )?.click(),
    )
    await saved()
    expect(written.at(-1)?.notes[0].color).toBe('sky')
    touch(host, { on: host.querySelector('[data-block="2"] .book-note-mark') })
    expect(dialog()?.querySelector('textarea')?.value).toBe('gone')
    const remove = [...(dialog()?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent === 'Delete',
    )
    act(() => remove?.click())
    expect(dialog()).toBeNull()
    await saved()
    expect(written.at(-1)?.notes.map((n) => n.id)).toEqual(['h'])
  })

  it('marks another edition’s note on the paragraph a swap from it lands on, not as a highlight', async () => {
    const { host } = await opened(undefined, book, [highlightOn('ko', 2, 0, 2)])
    expect(host.querySelector('mark')).toBeNull()
    expect(
      host.querySelector('[data-block="1"] .book-note-mark'),
    ).not.toBeNull()
  })

  it('lists every note on a, and goes to one in its own edition', async () => {
    const { host } = await opened(undefined, book, [
      highlightOn('ko', 0, 0, 1),
      note({ id: 'p', edition: 'en', slot: 2, body: 'end' }),
    ])
    press('a')
    const list = host.querySelector('[role="dialog"][aria-label="Notes"]')
    const entries = [...(list?.querySelectorAll('li button') ?? [])]
    expect(entries.map((b) => b.textContent)).toEqual(['Nobody.end', '1'])
    act(() => (entries[1] as HTMLElement | null)?.click())
    expect(edition(host)).toBe('1장')
    expect(scrolledTo().at(-1)).toBe('0')
    expect(
      host.querySelector('[role="dialog"][aria-label="Note"]'),
    ).not.toBeNull()
  })

  it('says there are no notes yet in an empty list', async () => {
    const { host } = await opened()
    press('a')
    expect(
      host.querySelector('[role="dialog"][aria-label="Notes"]')?.textContent,
    ).toContain('No notes yet')
  })

  it('undoes the last note edit on z, and redoes it on Z', async () => {
    const { host, written } = await opened()
    press('n')
    press('Escape')
    expect(host.querySelectorAll('.book-note-mark')).toHaveLength(1)
    press('z')
    expect(host.querySelectorAll('.book-note-mark')).toHaveLength(0)
    press('Z')
    expect(host.querySelectorAll('.book-note-mark')).toHaveLength(1)
    await saved()
    expect(written.at(-1)?.notes).toHaveLength(1)
  })

  it('says a note cannot be kept where the notes came with no tag to write against', async () => {
    const { fetch } = server()
    const answer = fetch.getMockImplementation()
    fetch.mockImplementation(async (url: string, init?: RequestInit) =>
      url === '/api/works/w/notes'
        ? Response.json({ work: 'w', notes: [] })
        : (answer?.(url, init) as Promise<Response>),
    )
    vi.stubGlobal('fetch', fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
    await land()
    press('n')
    await saved()
    expect(host.querySelector('.book-notice')?.textContent).toContain(
      'not being saved',
    )
    expect(
      fetch.mock.calls.some(
        ([url, init]) => url === '/api/works/w/notes' && init?.method === 'PUT',
      ),
    ).toBe(false)
  })

  /** The reader on `sentenced`, its server answering `/api/dictionary/` with `dictionary`. */
  async function withDictionary(
    dictionary: (lang: string, word: string) => Response,
  ) {
    const { fetch, written } = server(undefined, sentenced)
    const answer = fetch.getMockImplementation()
    const asked: string[] = []
    fetch.mockImplementation((url: string, init?: RequestInit) => {
      const found = url.match(/^\/api\/dictionary\/([^/]+)\/(.+)$/)
      if (!found) return answer?.(url, init) as Promise<Response>
      const [lang, word] = [found[1], decodeURIComponent(found[2])]
      asked.push(`${lang}:${word}`)
      return Promise.resolve(dictionary(lang, word))
    })
    vi.stubGlobal('fetch', fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
    await land()
    return { host, written, asked }
  }

  const lookUp = async (host: HTMLElement) => {
    select(host, 1, 0, 3)
    touch(host, { travel: 40 })
    const bar = host.querySelector('[role="toolbar"][aria-label="Selection"]')
    const button = [...(bar?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent === 'Look up',
    )
    await act(async () => {
      button?.click()
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
  }

  it('looks a selected word up in its language’s dictionary, and keeps its meaning on it', async () => {
    const { host, written, asked } = await withDictionary(() =>
      Response.json([
        { word: 'who', pos: 'pron', glosses: ['what person'], via: null },
      ]),
    )
    await lookUp(host)
    expect(asked).toEqual(['en:Who'])
    const panel = host.querySelector('[role="dialog"][aria-label="Look up"]')
    expect(panel?.querySelector('.reader-panel-title')?.textContent).toBe('Who')
    expect(panel?.querySelector('ol')?.textContent).toBe('what person')
    const keep = [...(panel?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent === 'Save word',
    )
    act(() => keep?.click())
    expect(host.querySelector('[aria-label="Look up"]')).toBeNull()
    await saved()
    expect(written.at(-1)?.notes).toMatchObject([
      {
        edition: 'en',
        slot: 1,
        text: { start: 0, end: 3 },
        body: 'who [pron]: what person',
      },
    ])
  })

  it('says when the server has no dictionary for the book’s language', async () => {
    const { host } = await withDictionary(
      () => new Response('no dictionary', { status: 404 }),
    )
    await lookUp(host)
    expect(host.querySelector('[aria-label="Look up"] p')?.textContent).toBe(
      'This server has no dictionary for en.',
    )
  })

  describe('the look-up mode', () => {
    /** One Japanese edition, its second block two sentences. */
    const japanese = {
      title: '猫',
      hub: 'ja',
      editions: {
        ja: {
          label: '日本語',
          language: 'ja',
          blocks: [
            { text: '一', heading: true, sentences: [[0, 1]] },
            {
              text: '吾輩は猫である。名前はまだ無い。',
              heading: false,
              sentences: [
                [0, 8],
                [8, 16],
              ],
            },
          ],
        },
      },
      alignments: [],
    }

    /** `japanese` with `text` as its second block, its edition tagged `language`. */
    const japaneseOf = (text: string, language: string | null = 'ja') => ({
      ...japanese,
      editions: {
        ja: {
          ...japanese.editions.ja,
          language,
          blocks: [
            japanese.editions.ja.blocks[0],
            { text, heading: false, sentences: [[0, text.length]] },
          ],
        },
      },
    })

    /** `japanese` with an English edition to swap to, block for block. */
    const bilingual = {
      ...japanese,
      editions: {
        ...japanese.editions,
        en: {
          label: 'English',
          language: 'en',
          blocks: [
            { text: 'One', heading: true, sentences: [[0, 3]] },
            {
              text: 'I am a cat. No name yet.',
              heading: false,
              sentences: [
                [0, 11],
                [12, 24],
              ],
            },
          ],
        },
      },
      alignments: [
        {
          to: 'en',
          pins: [],
          links: [
            [0, 1, 0, 1],
            [1, 2, 1, 2],
          ],
          sentences: [
            [0, 1, 0, 1],
            [1, 2, 1, 2],
            [2, 3, 2, 3],
          ],
        },
      ],
    }

    const on = () =>
      localStorage.setItem('taiyaku.prefs.lookup', JSON.stringify({ on: true }))

    afterEach(() => {
      Reflect.deleteProperty(document, 'caretPositionFromPoint')
    })

    /**
     * Has the browser's caret-from-point answer character `at` of block
     * `block`'s own words, wherever the press is.
     */
    function caretOn(host: HTMLElement, block: number, at: number) {
      const [offsetNode, offset] = pointIn(host, block, at)
      Object.defineProperty(document, 'caretPositionFromPoint', {
        configurable: true,
        value: () => ({ offsetNode, offset }),
      })
    }

    /**
     * Lays block `block`'s own words out on one line as Chromium answers for
     * them: each code point 20 px wide from x = 0, the line from y = 90 to 110.
     * The caret at a point is at the nearer edge of the character under it, in
     * that character's own text node, so the far half of a node's last
     * character puts it at the end of that node. Answered by `api`, the call
     * a browser has.
     */
    function layOutLine(
      host: HTMLElement,
      block: number,
      api:
        | 'caretPositionFromPoint'
        | 'caretRangeFromPoint' = 'caretPositionFromPoint',
    ) {
      const words = host.querySelector(
        `[data-block="${block}"] [data-text]`,
      ) as Node
      const walker = document.createTreeWalker(words, NodeFilter.SHOW_TEXT, {
        acceptNode: (node) =>
          node.parentElement?.closest('[data-aside]')
            ? NodeFilter.FILTER_REJECT
            : NodeFilter.FILTER_ACCEPT,
      })
      const nodes: Text[] = []
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        nodes.push(node as Text)
      }
      const whole = nodes.map((node) => node.data).join('')
      const points = [...whole]
      const startOf = (node: Text) =>
        nodes
          .slice(0, nodes.indexOf(node))
          .reduce((n, word) => n + word.length, 0)
      const drawnAt = (at: number) => [...whole.slice(0, at)].length * 20
      const caret = (x: number) => {
        const index = Math.max(
          0,
          Math.min(points.length - 1, Math.floor(x / 20)),
        )
        const from = points.slice(0, index).join('').length
        const at = x - index * 20 < 10 ? from : from + points[index].length
        const node = nodes.find(
          (word) => from < startOf(word) + word.length,
        ) as Text
        return { node, offset: at - startOf(node) }
      }
      Object.defineProperty(document, api, {
        configurable: true,
        value: (x: number) => {
          const { node, offset } = caret(x)
          return api === 'caretPositionFromPoint'
            ? { offsetNode: node, offset }
            : { startContainer: node, startOffset: offset }
        },
      })
      Object.defineProperty(Range.prototype, 'getClientRects', {
        configurable: true,
        value(this: Range) {
          if (!nodes.includes(this.startContainer as Text)) return []
          const from = startOf(this.startContainer as Text) + this.startOffset
          const to = startOf(this.endContainer as Text) + this.endOffset
          return [
            { left: drawnAt(from), right: drawnAt(to), top: 90, bottom: 110 },
          ]
        },
      })
      onTestFinished(() => {
        Reflect.deleteProperty(document, api)
        Reflect.deleteProperty(Range.prototype, 'getClientRects')
      })
    }

    /**
     * The reader on `sent`, its server answering a word's look-up and a
     * text's longest word with `dictionary`, and `asked` each, as
     * `lang:word` or `lang:at:text`.
     */
    async function reader(
      sent: unknown,
      dictionary: (asked: string) => Response | Promise<Response> = () =>
        Response.json([]),
      notes: Note[] = [],
    ) {
      const { fetch, written } = server(undefined, sent, notes)
      const answer = fetch.getMockImplementation()
      const asked: string[] = []
      fetch.mockImplementation((url: string, init?: RequestInit) => {
        const found = url.match(/^\/api\/dictionary\/([^/]+)\/(at\/)?(.+)$/)
        if (!found) return answer?.(url, init) as Promise<Response>
        const what = `${found[1]}:${found[2] ? 'at:' : ''}${decodeURIComponent(found[3])}`
        asked.push(what)
        return Promise.resolve(dictionary(what))
      })
      vi.stubGlobal('fetch', fetch)
      Element.prototype.scrollIntoView = vi.fn()
      const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
      await land()
      return { host, written, asked, fetch }
    }

    /** A tap on block `block`'s own words at character `at`, and its answer let land. */
    async function tapOn(
      host: HTMLElement,
      block: number,
      at: number,
      x = 100,
    ) {
      caretOn(host, block, at)
      touch(host, {
        on: host.querySelector(`[data-block="${block}"] [data-sentence]`),
        x,
      })
      await land()
    }

    const panel = () =>
      document.querySelector('[role="dialog"][aria-label="Look up"]')
    const bars = (host: HTMLElement) =>
      host.querySelector('.reader-top-bar') !== null
    const selectedText = () => window.getSelection()?.toString()
    const who = { word: 'who', pos: 'pron', glosses: ['a person'], via: null }
    const lookUpTitle = () =>
      panel()?.querySelector('.reader-panel-title')?.textContent
    const saveWord = () =>
      [...(panel()?.querySelectorAll('button') ?? [])].find(
        (b) => b.textContent === 'Save word',
      )

    /** Block `block`'s characters `start` up to `end` selected by a drag, and looked up from the bar. */
    async function lookUpFromBar(
      host: HTMLElement,
      block: number,
      start: number,
      end: number,
    ) {
      select(host, block, start, end)
      touch(host, { travel: 40 })
      const button = [
        ...host.querySelectorAll(
          '[role="toolbar"][aria-label="Selection"] button',
        ),
      ].find((b) => b.textContent === 'Look up') as HTMLElement
      act(() => button.click())
      await land()
    }

    it('leaves a tap to show the bars when the mode is off, looking nothing up', async () => {
      const { host, asked } = await reader(sentenced)
      await tapOn(host, 1, 13)
      expect(bars(host)).toBe(true)
      expect(asked).toEqual([])
      expect(panel()).toBeNull()
    })

    it('turns on and off on l and from its button in the bars, remembered', async () => {
      const { host } = await reader(sentenced)
      press('l')
      press('Tab')
      const button = () =>
        [...host.querySelectorAll('button')].find(
          (b) => b.textContent === 'Look up words',
        )
      expect(button()?.getAttribute('aria-pressed')).toBe('true')
      expect(
        JSON.parse(localStorage.getItem('taiyaku.prefs.lookup') ?? '{}'),
      ).toEqual({ on: true, includeEarlierBooks: true })
      act(() => button()?.click())
      expect(button()?.getAttribute('aria-pressed')).toBe('false')
      act(() => button()?.click())
      const again = mount(<BookReader work="w" onLeave={vi.fn()} />)
      await land()
      press('Tab')
      const kept = [...again.querySelectorAll('button')].find(
        (b) => b.textContent === 'Look up words',
      )
      expect(kept?.getAttribute('aria-pressed')).toBe('true')
    })

    it('looks up the word tapped, selected, the bars left as they were', async () => {
      on()
      const { host, asked } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await tapOn(host, 1, 13)
      expect(asked).toEqual(['en:Nobody'])
      expect(panel()?.querySelector('.reader-panel-title')?.textContent).toBe(
        'Nobody',
      )
      expect(selectedText()).toBe('Nobody')
      expect(bars(host)).toBe(false)
    })

    it('offers Save word only once the notes are read, keeping a word saved then and another device’s notes', async () => {
      on()
      const { fetch, written } = server(undefined, sentenced, [
        note({ id: 'theirs', slot: 0, body: 'theirs' }),
      ])
      const answer = fetch.getMockImplementation()
      let read = () => {}
      const notesRead = new Promise<void>((resolve) => {
        read = resolve
      })
      fetch.mockImplementation(async (url: string, init?: RequestInit) => {
        if (url === '/api/works/w/notes' && init?.method !== 'PUT') {
          await notesRead
        }
        if (url.startsWith('/api/dictionary/')) return Response.json([who])
        return answer?.(url, init) as Promise<Response>
      })
      vi.stubGlobal('fetch', fetch)
      Element.prototype.scrollIntoView = vi.fn()
      const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
      await land()
      await tapOn(host, 1, 2)
      expect(lookUpTitle()).toBe('Who')
      expect(saveWord()?.hasAttribute('disabled')).toBe(true)
      read()
      await land()
      expect(saveWord()?.hasAttribute('disabled')).toBe(false)
      act(() => saveWord()?.click())
      await saved()
      expect(written.at(-1)?.notes).toMatchObject([
        { id: 'theirs' },
        { edition: 'en', slot: 1, text: { start: 0, end: 3 } },
      ])
    })

    it('looks up a Japanese text’s longest word from the character tapped, and saves that word', async () => {
      on()
      const { host, asked, written } = await reader(japanese, () =>
        Response.json({ length: 5, entries: [who] }),
      )
      await tapOn(host, 1, 3)
      expect(asked).toEqual(['ja:at:猫である。名前はまだ無い。'])
      expect(selectedText()).toBe('猫である。')
      expect(panel()?.querySelector('.reader-panel-title')?.textContent).toBe(
        '猫である。',
      )
      const save = [...(panel()?.querySelectorAll('button') ?? [])].find(
        (b) => b.textContent === 'Save word',
      )
      act(() => save?.click())
      await saved()
      expect(written.at(-1)?.notes).toMatchObject([
        { edition: 'ja', slot: 1, text: { start: 3, end: 8 } },
      ])
    })

    it('says so when the server has no Japanese dictionary', async () => {
      on()
      const { host } = await reader(
        japanese,
        () => new Response('none', { status: 404 }),
      )
      await tapOn(host, 1, 3)
      expect(panel()?.querySelector('p')?.textContent).toBe(
        'This server has no dictionary for ja.',
      )
      expect(window.getSelection()?.isCollapsed).toBe(true)
    })

    it('turns the page on a tap in a paged text’s outer thirds, looking up only from the middle', async () => {
      on()
      paged()
      const page = paging([0, 0, 1])
      const { host, asked } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await tapOn(host, 1, 13, 500)
      expect(page()).toBe(1)
      await tapOn(host, 1, 13, 100)
      expect(page()).toBe(0)
      expect(asked).toEqual([])
      expect(panel()).toBeNull()
      await tapOn(host, 1, 13, 300)
      expect(page()).toBe(0)
      expect(asked).toEqual(['en:Nobody'])
      expect(panel()).not.toBeNull()
    })

    it('looks a word up from any third of a scrolled text', async () => {
      on()
      const { host, asked } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await tapOn(host, 1, 13, 500)
      expect(asked).toEqual(['en:Nobody'])
    })

    it('saves a word on the edition it was looked up in, after a swap to another', async () => {
      on()
      const { host, written } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await tapOn(host, 1, 13)
      press(' ')
      await land()
      const save = [...(panel()?.querySelectorAll('button') ?? [])].find(
        (b) => b.textContent === 'Save word',
      )
      act(() => save?.click())
      await saved()
      expect(written.at(-1)?.notes).toMatchObject([
        { edition: 'en', slot: 1, text: { start: 11, end: 17 } },
      ])
    })

    it('takes a tap on a space as a tap, looking nothing up', async () => {
      on()
      const { host, asked } = await reader(sentenced)
      await tapOn(host, 1, 10)
      expect(bars(host)).toBe(true)
      expect(asked).toEqual([])
    })

    it('takes a tap on Japanese punctuation as a tap, looking nothing up', async () => {
      on()
      const { host, asked } = await reader(japanese)
      await tapOn(host, 1, 7)
      expect(bars(host)).toBe(true)
      expect(asked).toEqual([])
    })

    it('shows the last word tapped, though the first one’s answer comes last', async () => {
      on()
      const first = deferred<Response>()
      const { host } = await reader(japanese, (asked) =>
        asked.startsWith('ja:at:猫')
          ? first.promise
          : Response.json({ length: 2, entries: [who] }),
      )
      await tapOn(host, 1, 3)
      await tapOn(host, 1, 8)
      await flush(() =>
        first.resolve(Response.json({ length: 5, entries: [] })),
      )
      await land()
      expect(panel()?.querySelector('.reader-panel-title')?.textContent).toBe(
        '名前',
      )
      expect(selectedText()).toBe('名前')
    })

    it('shows the last English word tapped, though the first one’s answer comes last', async () => {
      on()
      const first = deferred<Response>()
      const { host } = await reader(sentenced, (asked) =>
        asked === 'en:Who' ? first.promise : Response.json([who]),
      )
      await tapOn(host, 1, 1)
      await tapOn(host, 1, 13)
      await flush(() => first.resolve(Response.json([])))
      await land()
      expect(panel()?.querySelector('.reader-panel-title')?.textContent).toBe(
        'Nobody',
      )
      expect(panel()?.querySelector('ol')?.textContent).toBe('a person')
      expect(selectedText()).toBe('Nobody')
    })

    it('looks up a highlighted word, and opens a note from its mark', async () => {
      on()
      const { host, asked } = await reader(sentenced, () => Response.json([]), [
        highlightOn('en', 1, 0, 3),
        note({ id: 'p', edition: 'en', slot: 1, body: 'a mark' }),
      ])
      caretOn(host, 1, 1)
      touch(host, { on: host.querySelector('[data-block="1"] mark') })
      await land()
      expect(asked).toEqual(['en:Who'])
      expect(panel()).not.toBeNull()
      touch(host, { on: host.querySelector('.book-note-mark') })
      expect(
        host.querySelector('[role="dialog"][aria-label="Note"]'),
      ).not.toBeNull()
      expect(asked).toEqual(['en:Who'])
    })

    it('leaves a selection made since alone when the look-up closes', async () => {
      on()
      const { host } = await reader(sentenced, () => Response.json([who]))
      await tapOn(host, 1, 13)
      expect(selectedText()).toBe('Nobody')
      select(host, 1, 0, 3)
      const close = [...(panel()?.querySelectorAll('button') ?? [])].find(
        (b) => b.textContent === 'Close',
      )
      act(() => close?.click())
      expect(panel()).toBeNull()
      expect(selectedText()).toBe('Who')
    })

    it('lets go of its own selection when the look-up closes', async () => {
      on()
      const { host } = await reader(sentenced, () => Response.json([who]))
      await tapOn(host, 1, 13)
      expect(selectedText()).toBe('Nobody')
      press('Escape')
      expect(panel()).toBeNull()
      expect(window.getSelection()?.isCollapsed).toBe(true)
    })

    it('closes the look-up and its selection on a double tap, and peeks', async () => {
      on()
      const { host } = await reader(sentenced, () => Response.json([who]))
      caretOn(host, 1, 13)
      const at = later()
      const sentence = host.querySelector('[data-block="1"] [data-sentence]')
      touch(host, { on: sentence, at })
      await land()
      expect(panel()).not.toBeNull()
      touch(host, { on: sentence, at: at + 150 })
      await land()
      expect(panel()).toBeNull()
      expect(window.getSelection()?.isCollapsed).toBe(true)
      expect(peeked(host)?.[0]).toBe('1')
      expect(bars(host)).toBe(false)
    })

    it('still selects on a drag or a long press, closing an open look-up', async () => {
      on()
      const { host, asked } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await tapOn(host, 1, 13)
      expect(asked).toEqual(['en:Nobody'])
      select(host, 1, 0, 3)
      touch(host, { travel: 40 })
      expect(
        host.querySelector('[role="toolbar"][aria-label="Selection"]'),
      ).not.toBeNull()
      expect(panel()).toBeNull()
      expect(selectedText()).toBe('Who')
      select(host, 1, 11, 17)
      touch(host, { held: 600 })
      expect(
        host.querySelector('[role="toolbar"][aria-label="Selection"]'),
      ).not.toBeNull()
      expect(asked).toEqual(['en:Nobody'])
    })

    it('reveals a hidden interlinear line on a tap, and takes one on a peek as a tap', async () => {
      on()
      localStorage.setItem(
        'taiyaku.prefs.book-layout',
        JSON.stringify({ mode: 'interlinear', hideTranslation: true }),
      )
      const { host, asked } = await reader(sentenced)
      const line = () =>
        host.querySelector<HTMLElement>('[data-interlinear="2"]') as HTMLElement
      const caret = line().firstChild as Node
      Object.defineProperty(document, 'caretPositionFromPoint', {
        configurable: true,
        value: () => ({ offsetNode: caret, offset: 1 }),
      })
      touch(host, { on: line() })
      await land()
      expect(line().hasAttribute('data-hidden')).toBe(false)
      expect(bars(host)).toBe(false)
      expect(asked).toEqual([])
    })

    it('takes a tap on a peek as a tap, looking nothing up', async () => {
      on()
      const { host, asked } = await reader(sentenced)
      press('p')
      const peek = host.querySelector('.book-peek') as HTMLElement
      const caret = peek.querySelector('span')?.firstChild ?? peek
      Object.defineProperty(document, 'caretPositionFromPoint', {
        configurable: true,
        value: () => ({ offsetNode: caret, offset: 1 }),
      })
      touch(host, { on: peek })
      await land()
      expect(bars(host)).toBe(true)
      expect(asked).toEqual([])
    })

    it('takes a tap on a character’s far half as one on that character, and one on the next character’s near half as one on that', async () => {
      on()
      const { host, asked } = await reader(japanese, () =>
        Response.json({ length: 1, entries: [who] }),
      )
      layOutLine(host, 1)
      const sentence = host.querySelector('[data-block="1"] [data-sentence]')
      // 猫 is drawn from 60 to 80, で from 80 to 100.
      touch(host, { on: sentence, x: 75 })
      await land()
      touch(host, { on: sentence, x: 85 })
      await land()
      expect(asked).toEqual([
        'ja:at:猫である。名前はまだ無い。',
        'ja:at:である。名前はまだ無い。',
      ])
    })

    it('takes a tap on a glossed kanji’s far half, which puts the caret at the end of its own node, as one on the kanji, and one on the next character’s near half as one on that', async () => {
      on()
      const glossed = {
        ...japanese,
        editions: {
          ja: {
            ...japanese.editions.ja,
            blocks: [
              japanese.editions.ja.blocks[0],
              {
                ...japanese.editions.ja.blocks[1],
                marks: [{ kind: 'ruby', start: 3, end: 4, reading: 'ねこ' }],
              },
            ],
          },
        },
      }
      const { host, asked } = await reader(glossed, () =>
        Response.json({ length: 1, entries: [who] }),
      )
      layOutLine(host, 1)
      const ruby = host.querySelector('[data-block="1"] ruby')
      touch(host, { on: ruby, x: 75 })
      await land()
      touch(host, { on: ruby, x: 85 })
      await land()
      expect(asked).toEqual([
        'ja:at:猫である。名前はまだ無い。',
        'ja:at:である。名前はまだ無い。',
      ])
    })

    it('takes a tap on the far half of a kanji outside the BMP as one on the whole kanji', async () => {
      on()
      const paired = {
        ...japanese,
        editions: {
          ja: {
            ...japanese.editions.ja,
            blocks: [
              japanese.editions.ja.blocks[0],
              {
                text: '\u{20BB7}野家の牛丼。',
                heading: false,
                sentences: [[0, 8]],
              },
            ],
          },
        },
      }
      const { host, asked } = await reader(paired, () =>
        Response.json({ length: 1, entries: [who] }),
      )
      layOutLine(host, 1)
      const sentence = host.querySelector('[data-block="1"] [data-sentence]')
      touch(host, { on: sentence, x: 15 })
      await land()
      touch(host, { on: sentence, x: 25 })
      await land()
      expect(asked).toEqual([
        'ja:at:\u{20BB7}野家の牛丼。',
        'ja:at:野家の牛丼。',
      ])
    })

    it('finds the character by the older caret call where a browser has only that', async () => {
      on()
      const { host, asked } = await reader(japanese, () =>
        Response.json({ length: 1, entries: [who] }),
      )
      layOutLine(host, 1, 'caretRangeFromPoint')
      touch(host, {
        on: host.querySelector('[data-block="1"] [data-sentence]'),
        x: 85,
      })
      await land()
      expect(asked).toEqual(['ja:at:である。名前はまだ無い。'])
    })

    it.each([
      ['off the line, across it', { x: 75, y: 130 }],
      ['past the end of the line', { x: 200, y: 100 }],
    ])(
      'leaves a tap on no character, %s, to show the bars, looking nothing up',
      async (_where, point) => {
        on()
        // Ending on a kanji, which a tap taken for the last character would ask for.
        const ending = {
          ...japanese,
          editions: {
            ja: {
              ...japanese.editions.ja,
              blocks: [
                japanese.editions.ja.blocks[0],
                { text: '吾輩は猫', heading: false, sentences: [[0, 4]] },
              ],
            },
          },
        }
        const { host, asked } = await reader(ending)
        layOutLine(host, 1)
        touch(host, {
          on: host.querySelector('[data-block="1"] [data-sentence]'),
          ...point,
        })
        await land()
        expect(asked).toEqual([])
        expect(panel()).toBeNull()
        expect(bars(host)).toBe(true)
      },
    )

    it('keeps the light on the word a hold after a tap selects again, for the selection bar', async () => {
      on()
      const { host } = await reader(sentenced, () => Response.json([who]))
      await tapOn(host, 1, 13)
      expect(selectedText()).toBe('Nobody')
      touch(host, { held: 600 })
      expect(
        host.querySelector('[role="toolbar"][aria-label="Selection"]'),
      ).not.toBeNull()
      expect(panel()).toBeNull()
      expect(selectedText()).toBe('Nobody')
    })

    it('counts a tap on a ruby’s reading as one on the start of its base', async () => {
      on()
      const ruby = {
        ...japanese,
        editions: {
          ja: {
            ...japanese.editions.ja,
            blocks: [
              japanese.editions.ja.blocks[0],
              {
                ...japanese.editions.ja.blocks[1],
                marks: [{ kind: 'ruby', start: 3, end: 4, reading: 'ねこ' }],
              },
            ],
          },
        },
      }
      const { host, asked } = await reader(ruby, () =>
        Response.json({ length: 1, entries: [who] }),
      )
      const rt = host.querySelector('[data-block="1"] rt') as HTMLElement
      Object.defineProperty(document, 'caretPositionFromPoint', {
        configurable: true,
        value: () => ({ offsetNode: rt.firstChild, offset: 1 }),
      })
      touch(host, { on: rt })
      await land()
      expect(asked).toEqual(['ja:at:猫である。名前はまだ無い。'])
      expect(selectedText()).toBe('猫')
    })

    it('lights nothing in the edition swapped to when the answer lands there, and saves on the edition asked in', async () => {
      on()
      const answer = deferred<Response>()
      const { host, written } = await reader(bilingual, () => answer.promise)
      await tapOn(host, 1, 3)
      press(' ')
      await land()
      expect(
        host.querySelector('[data-block="1"] [data-text]')?.textContent,
      ).toBe('I am a cat. No name yet.')
      await flush(() =>
        answer.resolve(Response.json({ length: 3, entries: [who] })),
      )
      await land()
      expect(lookUpTitle()).toBe('猫であ')
      expect(selectedText()).toBe('')
      act(() => saveWord()?.click())
      await saved()
      expect(written.at(-1)?.notes).toMatchObject([
        { edition: 'ja', slot: 1, text: { start: 3, end: 6 } },
      ])
    })

    it('reads a stored mode that is not true as off, and turns it on at the first press', async () => {
      localStorage.setItem(
        'taiyaku.prefs.lookup',
        JSON.stringify({ on: 'yes' }),
      )
      const { host, asked } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await tapOn(host, 1, 13)
      expect(asked).toEqual([])
      expect(panel()).toBeNull()
      const button = [...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Look up words',
      ) as HTMLElement
      expect(button.getAttribute('aria-pressed')).toBe('false')
      act(() => button.click())
      expect(button.getAttribute('aria-pressed')).toBe('true')
    })

    it('asks only the sixteen code points from the character tapped of a block that runs on past them', async () => {
      on()
      const { host, asked } = await reader(
        japaneseOf(
          '吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。',
        ),
        () => Response.json({ length: 1, entries: [who] }),
      )
      await tapOn(host, 1, 3)
      expect(asked).toEqual(['ja:at:猫である。名前はまだ無い。どこで'])
    })

    it('selects nothing when the answer lands after the look-up closed', async () => {
      on()
      const answer = deferred<Response>()
      const { host } = await reader(japanese, () => answer.promise)
      await tapOn(host, 1, 3)
      press('Escape')
      expect(panel()).toBeNull()
      await flush(() =>
        answer.resolve(Response.json({ length: 5, entries: [who] })),
      )
      await land()
      expect(panel()).toBeNull()
      expect(selectedText()).toBe('')
    })

    it('keeps a later tap’s entries when an earlier tap’s look-up fails last', async () => {
      on()
      const first = deferred<Response>()
      const { host } = await reader(japanese, (asked) =>
        asked.startsWith('ja:at:猫')
          ? first.promise
          : Response.json({ length: 2, entries: [who] }),
      )
      await tapOn(host, 1, 3)
      await tapOn(host, 1, 8)
      await flush(() => first.resolve(new Response('gone', { status: 500 })))
      await land()
      expect(lookUpTitle()).toBe('名前')
      expect(panel()?.querySelector('ol')?.textContent).toBe('a person')
    })

    it('says the look-up failed when the server refuses it, whatever the refusal holds', async () => {
      on()
      const { host } = await reader(japanese, () =>
        Response.json({ length: 1, entries: [who] }, { status: 500 }),
      )
      await tapOn(host, 1, 3)
      expect(panel()?.querySelector('p')?.textContent).toContain(': 500 ')
      expect(panel()?.querySelector('ol')).toBeNull()
    })

    it('escapes the text in the address it asks', async () => {
      on()
      const { host, fetch } = await reader(japaneseOf('何だ?本当か'), () =>
        Response.json({ length: 1, entries: [who] }),
      )
      await tapOn(host, 1, 0)
      expect(
        fetch.mock.calls
          .map(([url]) => url)
          .filter((url) => url.startsWith('/api/dictionary/')),
      ).toEqual([
        '/api/dictionary/ja/at/%E4%BD%95%E3%81%A0%3F%E6%9C%AC%E5%BD%93%E3%81%8B',
      ])
    })

    it('escapes the language in the address of a longest-word search', async () => {
      const { fetch } = await reader(japanese, () =>
        Response.json({ length: 1, entries: [who] }),
      )
      await lookUpAt('ja/JP', '何')
      expect(fetch.mock.calls.at(-1)?.[0]).toBe(
        '/api/dictionary/ja%2FJP/at/%E4%BD%95',
      )
    })

    it('escapes an edition’s language in the address of a word it asks', async () => {
      on()
      const { host, fetch } = await reader(japaneseOf('cat', 'ja/JP'))
      await tapOn(host, 1, 0)
      expect(fetch.mock.calls.at(-1)?.[0]).toBe('/api/dictionary/ja%2FJP/cat')
    })

    it('titles the look-up with the character tapped until an answer names a word, and after one that names none', async () => {
      on()
      const answer = deferred<Response>()
      const { host } = await reader(japanese, () => answer.promise)
      await tapOn(host, 1, 3)
      expect(lookUpTitle()).toBe('猫')
      expect(panel()?.querySelector('p')?.textContent).toBe('Looking it up…')
      await flush(() =>
        answer.resolve(Response.json({ length: 0, entries: [] })),
      )
      await land()
      expect(lookUpTitle()).toBe('猫')
      expect(selectedText()).toBe('')
      expect(panel()?.querySelector('p')?.textContent).toBe(
        'Not in the dictionary.',
      )
    })

    it('drops a tap’s answer once the look-up has moved to a selection from the bar', async () => {
      on()
      const answer = deferred<Response>()
      const { host } = await reader(japanese, (asked) =>
        asked.startsWith('ja:at:') ? answer.promise : Response.json([who]),
      )
      await tapOn(host, 1, 3)
      // With the mode off, the drag below leaves the look-up open.
      press('l')
      await lookUpFromBar(host, 1, 8, 10)
      expect(lookUpTitle()).toBe('名前')
      await flush(() =>
        answer.resolve(Response.json({ length: 5, entries: [] })),
      )
      await land()
      expect(lookUpTitle()).toBe('名前')
      expect(panel()?.querySelector('ol')?.textContent).toBe('a person')
    })

    it('drops a Japanese tap’s answer once a word is tapped in the edition swapped to', async () => {
      on()
      const answer = deferred<Response>()
      const { host } = await reader(bilingual, (asked) =>
        asked.startsWith('ja:at:') ? answer.promise : Response.json([who]),
      )
      await tapOn(host, 1, 3)
      press(' ')
      await land()
      await tapOn(host, 1, 7)
      expect(lookUpTitle()).toBe('cat')
      await flush(() =>
        answer.resolve(Response.json({ length: 5, entries: [] })),
      )
      await land()
      expect(lookUpTitle()).toBe('cat')
      expect(selectedText()).toBe('cat')
    })

    it('leaves a look-up opened from the selection bar open through a new hold, with the mode off', async () => {
      const { host } = await reader(sentenced, () => Response.json([who]))
      await lookUpFromBar(host, 1, 0, 3)
      expect(panel()).not.toBeNull()
      select(host, 1, 11, 17)
      touch(host, { held: 600 })
      expect(panel()).not.toBeNull()
    })

    describe('the route a tap takes', () => {
      const KANA = '吾輩は猫である。名前はまだ無い。'
      const HANZI = '我是一只猫。名字还没有。'
      const JAPANESE: [string, string | null][] = [
        ['ja', 'ja'],
        ['ja-JP', 'ja-JP'],
        ['jpn', 'jpn'],
        ['no tag, its text with kana', null],
      ]

      it.each(JAPANESE)(
        'asks the Japanese dictionary for the longest word from the character tapped: %s',
        async (_, language) => {
          on()
          const { host, asked } = await reader(japaneseOf(KANA, language), () =>
            Response.json({ length: 2, entries: [who] }),
          )
          await tapOn(host, 1, 3)
          expect(asked).toEqual(['ja:at:猫である。名前はまだ無い。'])
          expect(lookUpTitle()).toBe('猫で')
        },
      )

      it.each(JAPANESE)(
        'names the Japanese dictionary when the server has none: %s',
        async (_, language) => {
          on()
          const { host } = await reader(
            japaneseOf(KANA, language),
            () => new Response('none', { status: 404 }),
          )
          await tapOn(host, 1, 3)
          expect(panel()?.querySelector('p')?.textContent).toBe(
            'This server has no dictionary for ja.',
          )
        },
      )

      it('saves the longest word of an edition tagged jpn on that edition', async () => {
        on()
        const { host, written } = await reader(japaneseOf(KANA, 'jpn'), () =>
          Response.json({ length: 2, entries: [who] }),
        )
        await tapOn(host, 1, 3)
        act(() => saveWord()?.click())
        await saved()
        expect(written.at(-1)?.notes).toMatchObject([
          { edition: 'ja', slot: 1, text: { start: 3, end: 5 } },
        ])
      })

      it.each([
        ['jav', KANA, '吾輩は猫である'],
        ['zh', HANZI, '我是一只猫'],
      ])(
        'looks up the run of letters tapped in its own language: %s',
        async (language, text, run) => {
          on()
          const { host, asked } = await reader(japaneseOf(text, language), () =>
            Response.json([who]),
          )
          await tapOn(host, 1, 3)
          expect(asked).toEqual([`${language}:${run}`])
          expect(lookUpTitle()).toBe(run)
        },
      )

      it('asks nothing for an untagged edition all in kanji, which names no language', async () => {
        on()
        const { host, asked } = await reader(japaneseOf(HANZI, null))
        await tapOn(host, 1, 3)
        expect(asked).toEqual([])
        expect(panel()?.querySelector('p')?.textContent).toBe(
          'This server has no dictionary for a book that names no language.',
        )
      })
    })

    it('leaves the mode off for an l typed into the search box, and turns it on for one outside it', async () => {
      const { host } = await reader(japanese)
      const mode = () => localStorage.getItem('taiyaku.prefs.lookup')
      press('/')
      const box = host.querySelector('input[type="search"]') as HTMLInputElement
      act(() => {
        box.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'l',
            bubbles: true,
            cancelable: true,
          }),
        )
      })
      expect(JSON.parse(mode() ?? '{}').on).not.toBe(true)
      press('Escape')
      press('l')
      expect(JSON.parse(mode() ?? '{}')).toEqual({
        on: true,
        includeEarlierBooks: true,
      })
    })

    it('lists the keys of finding and looking up once each in Help', async () => {
      const { host } = await reader(japanese)
      press('?')
      const keys = [
        ...host.querySelectorAll('[aria-label="Keyboard help"] kbd'),
      ].map((key) => key.textContent)
      for (const key of ['/', ']', '[', 'l']) {
        expect(keys.filter((listed) => listed === key)).toEqual([key])
      }
    })

    describe('in a Korean book', () => {
      /** The second block's text: 10년 at 3, 4번지에 at 7, 11월이 at 12, 2000 at 17. */
      const KOREAN = '그는 10년 4번지에 11월이 2000 살았다.'

      it.each([
        ['the digit of 10년', 3, '10년'],
        ['the letter of 10년', 5, '10년'],
        ['the digit of 4번지에', 7, '4번지에'],
        ['the particle of 4번지에', 10, '4번지에'],
        ['the digit of 11월이', 12, '11월이'],
        ['the particle of 11월이', 15, '11월이'],
      ])(
        'looks up and lights the whole run from %s',
        async (_name, at, run) => {
          on()
          const { host, asked } = await reader(japaneseOf(KOREAN, 'ko'))
          await tapOn(host, 1, at)
          expect(asked).toEqual([`ko:${run}`])
          expect(lookUpTitle()).toBe(run)
          expect(selectedText()).toBe(run)
        },
      )

      it.each(['ko', 'KO', 'ko-KR', 'ko_KR'])(
        'takes digits in for an edition tagged %s',
        async (tag) => {
          on()
          const { host, asked } = await reader(japaneseOf(KOREAN, tag))
          await tapOn(host, 1, 3)
          expect(asked).toEqual([`${tag}:10년`])
        },
      )

      it('takes a tap on a bare number as a tap, looking nothing up', async () => {
        on()
        const { host, asked } = await reader(japaneseOf(KOREAN, 'ko'))
        await tapOn(host, 1, 18)
        expect(asked).toEqual([])
        expect(bars(host)).toBe(true)
      })

      it.each([
        ['Spanish', 'es'],
        ['English', 'en'],
        ['a language whose tag only begins as Korean’s does', 'kok'],
      ])('leaves digits out of a word in %s', async (_name, tag) => {
        on()
        const { host, asked } = await reader(japaneseOf('Pasó 10años.', tag))
        await tapOn(host, 1, 5)
        expect(asked).toEqual([])
        expect(bars(host)).toBe(true)
        await tapOn(host, 1, 8)
        expect(asked).toEqual([`${tag}:años`])
        expect(selectedText()).toBe('años')
      })
    })

    it('saves a word looked up from the selection bar on its own edition after a swap, with the mode off', async () => {
      const { host, written } = await reader(sentenced, () =>
        Response.json([who]),
      )
      await lookUpFromBar(host, 1, 0, 3)
      press(' ')
      await land()
      act(() => saveWord()?.click())
      await saved()
      expect(written.at(-1)?.notes).toMatchObject([
        { edition: 'en', slot: 1, text: { start: 0, end: 3 } },
      ])
    })

    describe('the characters a look-up asks for', () => {
      const para = (text: string) => ({
        text,
        heading: false,
        sentences: [[0, text.length]],
      })
      /** Its second word, `Ned`, starts at 5 and runs on 183 units, the 128th a letter. */
      const RUN = `Then Ned Reed kept the lamp${' and kept it burning'.repeat(8)}.`
      /** Its second word, `Ned`, starts at 5, the 128th unit from there the first half of 😀. */
      const PAIR = `Then ${'Ned Reed waited'.padEnd(127, '.')}😀 and left.`
      /** `ko` the hub and `en` beside it, block for block, `en` alone with character data. */
      const cast = {
        title: 'The Lamp',
        hub: 'ko',
        editions: {
          ko: {
            label: 'Korean',
            language: 'ko',
            blocks: [
              { text: '하나', heading: true, sentences: [[0, 2]] },
              para('에이다가 등불을 켰다.'),
              para('그때 네드 리드가 등불을 지켰다.'),
              para('에이다는 네드 리드를 보았다.'),
              para('그는 기다리다 떠났다.'),
              para('마라 벨이 돌아왔다.'),
              para('느-느-네드 리드가 들어왔다.'),
            ],
          },
          en: {
            label: 'English',
            language: 'en',
            characters: true,
            blocks: [
              { text: 'One', heading: true, sentences: [[0, 3]] },
              para('Ada lit the lamp.'),
              para(RUN),
              para('Ada saw Ned Reed go.'),
              para(PAIR),
              para('Mara Vell came back.'),
              para('N-N-Ned Reed came in.'),
            ],
          },
        },
        alignments: [
          {
            to: 'en',
            pins: [],
            links: [0, 1, 2, 3, 4, 5, 6].map((b) => [b, b + 1, b, b + 1]),
            sentences: [0, 1, 2, 3, 4, 5, 6].map((b) => [b, b + 1, b, b + 1]),
          },
        ],
      }
      const inEnglish = { edition: 'en', slot: 0 }
      /** The address a tap in `edition`'s block `block`, the words `before` it, asks the characters route at. */
      const asksFor = (
        edition: string,
        block: number,
        text: string,
        earlier: 0 | 1,
        before: string,
      ) =>
        `/api/works/w/${edition}/book/characters?block=${block}&text=${encodeURIComponent(text)}&earlier=${earlier}&before=${encodeURIComponent(before)}`
      /** `RUN` from `Ned` on, cut to 128 units. */
      const fromNed = RUN.slice(5, 5 + 128)

      /**
       * The reader on `sent`, at `saved` where given, its server answering a
       * word with `answers.dictionary` or else `who`, a longest word with two
       * characters, and the characters route with `answers.characters` for
       * the text asked or else a book haiyaku holds that names no one.
       * `asked` lists the dictionary's requests as `reader`'s does, and
       * `characters` the characters route's addresses.
       */
      async function withCharacters(
        sent: unknown,
        saved?: { edition: string; slot: number },
        answers: {
          characters?: (text: string) => Response | Promise<Response>
          dictionary?: () => Response | Promise<Response>
        } = {},
      ) {
        const { fetch } = server(saved, sent)
        const answer = fetch.getMockImplementation()
        const asked: string[] = []
        fetch.mockImplementation((url: string, init?: RequestInit) => {
          if (url.includes('/book/characters?')) {
            const text = new URL(url, 'http://reader').searchParams.get('text')
            return Promise.resolve(
              answers.characters?.(text ?? '') ?? Response.json(naming()),
            )
          }
          const found = url.match(/^\/api\/dictionary\/([^/]+)\/(at\/)?(.+)$/)
          if (!found) return answer?.(url, init) as Promise<Response>
          asked.push(
            `${found[1]}:${found[2] ? 'at:' : ''}${decodeURIComponent(found[3])}`,
          )
          return Promise.resolve(
            found[2]
              ? Response.json({ length: 2, entries: [who] })
              : (answers.dictionary?.() ?? Response.json([who])),
          )
        })
        vi.stubGlobal('fetch', fetch)
        Element.prototype.scrollIntoView = vi.fn()
        const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
        await land()
        const characters = () =>
          fetch.mock.calls
            .map(([url]) => url)
            .filter((url) => url.includes('/book/characters'))
        return { host, asked, characters }
      }

      it('asks for the characters a tapped word names, from the word on, with the words before it and the earlier books', async () => {
        on()
        const { host, characters } = await withCharacters(cast, inEnglish)
        await tapOn(host, 2, 6)
        expect(characters()).toEqual([asksFor('en', 2, fromNed, 1, 'Then ')])
      })

      it('sends no words before a tap on a block’s first word', async () => {
        on()
        const { host, characters } = await withCharacters(cast, inEnglish)
        await tapOn(host, 1, 1)
        expect(characters()).toEqual([
          '/api/works/w/en/book/characters?block=1&text=Ada%20lit%20the%20lamp.&earlier=1',
        ])
      })

      it('sends the words before a tap far into a block from a word’s start, 64 units at most', async () => {
        on()
        const { host, characters } = await withCharacters(cast, inEnglish)
        const last = RUN.lastIndexOf('burning')
        await tapOn(host, 2, last + 1)
        expect(characters()).toEqual([
          asksFor(
            'en',
            2,
            'burning.',
            1,
            `it burning${' and kept it burning'.repeat(2)} and kept it `,
          ),
        ])
      })

      it('cuts the text at 128 units without splitting a character outside the BMP', async () => {
        on()
        const { host, characters } = await withCharacters(cast, inEnglish)
        await tapOn(host, 4, 6)
        expect(characters()).toEqual([
          asksFor('en', 4, PAIR.slice(5, 5 + 127), 1, 'Then '),
        ])
      })

      it('asks with the selection bar’s text and the block it starts in, and no words before it', async () => {
        const { host, characters } = await withCharacters(cast, inEnglish)
        await lookUpFromBar(host, 3, 8, 16)
        expect(characters()).toEqual([
          '/api/works/w/en/book/characters?block=3&text=Ned%20Reed&earlier=1',
        ])
      })

      it('asks nothing of a book edition without character data', async () => {
        on()
        const { host, asked, characters } = await withCharacters(cast)
        await tapOn(host, 2, 4)
        expect(asked).toEqual(['ko:네드'])
        expect(characters()).toEqual([])
      })

      it('draws neither the cards nor the widened light of a look-up before it in a book edition without character data', async () => {
        on()
        const { host, characters } = await withCharacters(cast, inEnglish, {
          characters: () => Response.json({ ...naming(NED), length: 8 }),
        })
        await tapOn(host, 2, 6)
        expect(cardsDrawn()).toEqual(['Ned Reed'])
        expect(selectedText()).toBe('Ned Reed')
        press(' ')
        await land()
        expect(edition(host)).toBe('하나')
        await tapOn(host, 2, 4)
        expect(lookUpTitle()).toBe('네드')
        expect(cardsDrawn()).toEqual([])
        expect(selectedText()).toBe('네드')
        expect(characters()).toHaveLength(1)
      })

      it('asks once for a Japanese tap, after its longest word too', async () => {
        on()
        const { host, characters } = await withCharacters({
          ...japanese,
          editions: { ja: { ...japanese.editions.ja, characters: true } },
        })
        await tapOn(host, 1, 3)
        expect(lookUpTitle()).toBe('猫で')
        expect(characters()).toEqual([
          asksFor('ja', 1, '猫である。名前はまだ無い。', 1, '吾輩は'),
        ])
      })

      it('includes the earlier books by default and asks without them once the setting is off', async () => {
        expect(lookupModule.defaults.includeEarlierBooks).toBe(true)
        localStorage.setItem(
          'taiyaku.prefs.lookup',
          JSON.stringify({ on: true, includeEarlierBooks: false }),
        )
        const off = await withCharacters(cast, inEnglish)
        await tapOn(off.host, 2, 6)
        expect(off.characters()).toEqual([
          asksFor('en', 2, fromNed, 0, 'Then '),
        ])
        // A slice stored by a build without the setting.
        localStorage.setItem(
          'taiyaku.prefs.lookup',
          JSON.stringify({ on: true }),
        )
        const before = await withCharacters(cast, inEnglish)
        await tapOn(before.host, 2, 6)
        expect(before.characters()).toEqual([
          asksFor('en', 2, fromNed, 1, 'Then '),
        ])
      })

      it('asks without the earlier books on the next tap once the setting is turned off in the session', async () => {
        on()
        const { host, characters } = await withCharacters(cast, inEnglish)
        await tapOn(host, 2, 6)
        expect(characters()).toEqual([asksFor('en', 2, fromNed, 1, 'Then ')])
        press('s')
        const box = [...host.querySelectorAll('[aria-label="Settings"] label')]
          .find((label) => label.textContent === 'include earlier books')
          ?.querySelector<HTMLInputElement>('input[type="checkbox"]')
        expect(box?.checked).toBe(true)
        act(() => box?.click())
        press('Escape')
        await tapOn(host, 2, 6)
        expect(characters()).toEqual([
          asksFor('en', 2, fromNed, 1, 'Then '),
          asksFor('en', 2, fromNed, 0, 'Then '),
        ])
      })

      /** The cards the look-up draws, by their headings. */
      const cardsDrawn = () =>
        [...(panel()?.querySelectorAll('.book-lookup-character h3') ?? [])].map(
          (heading) => heading.textContent,
        )
      const entriesDrawn = () => [
        ...(panel()?.querySelectorAll('.book-lookup-entry') ?? []),
      ]
      /** The clock the hold's wait runs on, run by hand from here. */
      const stopClock = () =>
        vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
      const elapse = async (ms: number) => {
        await act(async () => {
          vi.advanceTimersByTime(ms)
        })
      }

      it('holds the first draw until the characters are in', async () => {
        on()
        const characters = deferred<Response>()
        const { host } = await withCharacters(cast, inEnglish, {
          characters: () => characters.promise,
        })
        stopClock()
        await tapOn(host, 2, 6)
        await elapse(CARDS_WAIT_MS - 1)
        expect(entriesDrawn()).toEqual([])
        await flush(() => characters.resolve(Response.json(naming(NED))))
        await land()
        expect(cardsDrawn()).toEqual(['Ned Reed'])
        const [entry] = entriesDrawn()
        if (!entry) throw new Error('no entry drawn')
        const card = panel()?.querySelector('.book-lookup-character')
        expect(
          card &&
            card.compareDocumentPosition(entry) &
              Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy()
      })

      it('holds the first draw until the dictionary is in too when the characters come first', async () => {
        on()
        const dictionary = deferred<Response>()
        const { host } = await withCharacters(cast, inEnglish, {
          characters: () => Response.json(naming(NED)),
          dictionary: () => dictionary.promise,
        })
        stopClock()
        await tapOn(host, 2, 6)
        expect(cardsDrawn()).toEqual([])
        await flush(() => dictionary.resolve(Response.json([who])))
        await land()
        expect(cardsDrawn()).toEqual(['Ned Reed'])
        expect(entriesDrawn()).toHaveLength(1)
      })

      it('draws the dictionary when the characters miss the wait, and a late answer then moves nothing', async () => {
        on()
        const characters = deferred<Response>()
        const { host } = await withCharacters(cast, inEnglish, {
          characters: () => characters.promise,
        })
        stopClock()
        await tapOn(host, 2, 6)
        await elapse(CARDS_WAIT_MS - 1)
        expect(entriesDrawn()).toEqual([])
        await elapse(1)
        const [entry] = entriesDrawn()
        expect(entry).toBeDefined()
        await flush(() => characters.resolve(Response.json(naming(NED))))
        await land()
        expect(cardsDrawn()).toEqual([])
        expect(entriesDrawn()[0]).toBe(entry)
      })

      it('draws the dictionary alone when the characters fail, and logs the failure', async () => {
        on()
        const { host } = await withCharacters(cast, inEnglish, {
          characters: () => new Response('the store broke', { status: 500 }),
        })
        const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
        stopClock()
        await tapOn(host, 2, 6)
        expect(entriesDrawn()).toHaveLength(1)
        expect(cardsDrawn()).toEqual([])
        expect(document.body.textContent).not.toContain('the store broke')
        expect(warn).toHaveBeenCalledOnce()
        expect(String(warn.mock.calls[0]?.[1])).toContain('500 the store broke')
      })

      it('a later tap’s panel never draws an earlier tap’s cards', async () => {
        on()
        const ned = deferred<Response>()
        const { host } = await withCharacters(cast, inEnglish, {
          characters: (text) =>
            text.startsWith('Ned') ? ned.promise : Response.json(naming(MARA)),
        })
        stopClock()
        await tapOn(host, 2, 6)
        await tapOn(host, 5, 1)
        expect(cardsDrawn()).toEqual(['Mara Vell'])
        await flush(() => ned.resolve(Response.json(naming(NED))))
        await land()
        expect(cardsDrawn()).toEqual(['Mara Vell'])
      })

      it('a book without character data draws the dictionary at once', async () => {
        on()
        const { host } = await withCharacters(cast)
        stopClock()
        await tapOn(host, 2, 4)
        expect(entriesDrawn()).toHaveLength(1)
      })

      describe('the light and the marks while a card shows', () => {
        /** The characters route naming `cards` by a name `name.length` units long from `name.start`. */
        const answered = (
          cards: CharacterCard[],
          name: { start?: number; length?: number } = {},
        ) => Response.json({ ...naming(...cards), ...name })
        /**
         * `cast`, its Korean edition writing the name in block 3 in the
         * letters the card has it in, and holding character data where
         * `flagged`.
         */
        const lettered = (flagged: boolean) => ({
          ...cast,
          editions: {
            ...cast.editions,
            ko: {
              ...cast.editions.ko,
              characters: flagged,
              blocks: cast.editions.ko.blocks.map((block, i) =>
                i === 3 ? para('에이다는 Ned Reed 씨를 보았다.') : block,
              ),
            },
          },
        })
        const bySurname = (card: CharacterCard): CharacterCard => ({
          ...card,
          matched: { text: card.name.text, part: 'surname' },
        })

        /**
         * A browser's custom highlights, which jsdom has none of. `marked`
         * is what is set under a name, each range as its block and the words
         * it covers, `gone:` before one whose text is no longer drawn.
         */
        function highlights() {
          const marks = new Map<string, { ranges: Range[] }>()
          vi.stubGlobal('CSS', { highlights: marks })
          vi.stubGlobal(
            'Highlight',
            class {
              readonly ranges: Range[]
              constructor(...ranges: Range[]) {
                this.ranges = ranges
              }
            },
          )
          const marked = (name: string) =>
            (marks.get(name)?.ranges ?? []).map((range) => {
              const at = range.startContainer
              const block =
                at.parentElement?.closest<HTMLElement>('[data-block]')
              return `${at.isConnected ? '' : 'gone:'}${block?.dataset.block}:${range}`
            })
          return { marks, marked }
        }

        /**
         * The text 800 px tall and each block 500 px of it in turn, scrolled
         * `top` px down: at 0 the blocks within a screen of it are 0 to 3.
         * Answers what scrolls it, as a browser would.
         */
        function laidOut(host: HTMLElement) {
          let top = 0
          const box = (y: number, height: number, width: number) =>
            ({
              left: 0,
              right: width,
              width,
              top: y,
              bottom: y + height,
              height,
            }) as DOMRect
          vi.spyOn(
            Element.prototype,
            'getBoundingClientRect',
          ).mockImplementation(function (this: Element) {
            if (this.classList.contains('book-text')) return box(0, 800, 600)
            const block = (this as HTMLElement).dataset?.block
            if (block !== undefined) {
              return box(Number(block) * 500 - top, 500, 600)
            }
            // A figure takes no room, at the top of the block it is set before.
            const next = (this.nextElementSibling as HTMLElement | null)
              ?.dataset?.block
            return next === undefined
              ? box(0, 0, 0)
              : box(Number(next) * 500 - top, 0, 600)
          })
          return (to: number) => {
            top = to
            act(() => {
              host
                .querySelector('.book-text')
                ?.dispatchEvent(new Event('scroll'))
            })
          }
        }

        /** The frames asked for and not yet drawn, run when a test says. */
        function frames() {
          const waiting = new Map<number, FrameRequestCallback>()
          let asked = 0
          vi.spyOn(window, 'requestAnimationFrame').mockImplementation(
            (run) => {
              waiting.set(++asked, run)
              return asked
            },
          )
          vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((id) => {
            waiting.delete(id)
          })
          return {
            asked: () => asked,
            run: () =>
              act(() => {
                const due = [...waiting.values()]
                waiting.clear()
                for (const draw of due) draw(0)
              }),
          }
        }

        it('lights the word tapped until the cards say how far the name runs, then the whole name', async () => {
          on()
          const { marked } = highlights()
          const characters = deferred<Response>()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => characters.promise,
          })
          stopClock()
          await tapOn(host, 2, 6)
          expect(selectedText()).toBe('Ned')
          expect(marked('book-lookup-word')).toEqual(['2:Ned'])
          await flush(() => characters.resolve(answered([NED], { length: 8 })))
          await land()
          expect(selectedText()).toBe('Ned Reed')
          expect(marked('book-lookup-word')).toEqual(['2:Ned Reed'])
        })

        it('lights back over the words before the tap where the name begins there', async () => {
          on()
          const { marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { start: -4, length: 8 }),
          })
          await tapOn(host, 3, 13)
          expect(lookUpTitle()).toBe('Reed')
          expect(selectedText()).toBe('Ned Reed')
          expect(marked('book-lookup-word')).toEqual(['3:Ned Reed'])
        })

        it('lights a name that begins past a stutter with the word tapped, all of both', async () => {
          on()
          const { marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { start: 4, length: 8 }),
          })
          await tapOn(host, 6, 5)
          expect(lookUpTitle()).toBe('N-N-Ned')
          expect(selectedText()).toBe('N-N-Ned Reed')
          expect(marked('book-lookup-word')).toEqual(['6:N-N-Ned Reed'])
        })

        it('never lights less than the word tapped, nor from before its block', async () => {
          on()
          const names = [{ length: 2 }, { start: -99, length: 8 }]
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], names.shift()),
          })
          await tapOn(host, 2, 6)
          expect(selectedText()).toBe('Ned')
          press('Escape')
          await tapOn(host, 2, 6)
          expect(selectedText()).toBe('Then Ned')
        })

        it('lights the word alone where the answer names no one, or comes after the wait', async () => {
          on()
          const late = deferred<Response>()
          const answers = [answered([], { length: 8 }), late.promise]
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answers.shift() as Response | Promise<Response>,
          })
          await tapOn(host, 2, 6)
          expect(selectedText()).toBe('Ned')
          press('Escape')
          stopClock()
          await tapOn(host, 2, 6)
          await elapse(CARDS_WAIT_MS)
          await flush(() => late.resolve(answered([NED], { length: 8 })))
          await land()
          expect(entriesDrawn()).toHaveLength(1)
          expect(selectedText()).toBe('Ned')
        })

        it('keeps the name marked as a highlight of its own while the look-up is open, whatever is selected, and lets it go as it closes', async () => {
          on()
          const { marks, marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { length: 8 }),
          })
          await tapOn(host, 2, 6)
          select(host, 1, 0, 3)
          await land()
          expect(marked('book-lookup-word')).toEqual(['2:Ned Reed'])
          expect(selectedText()).toBe('Ada')
          press('Escape')
          expect(panel()).toBeNull()
          expect(marks.has('book-lookup-word')).toBe(false)
        })

        it('lights nothing when the dictionary answers a hold after the look-up has closed', async () => {
          on()
          const { marks } = highlights()
          const dictionary = deferred<Response>()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { length: 8 }),
            dictionary: () => dictionary.promise,
          })
          stopClock()
          await tapOn(host, 2, 6)
          expect(selectedText()).toBe('Ned')
          press('Escape')
          expect(panel()).toBeNull()
          await flush(() => dictionary.resolve(Response.json([who])))
          await land()
          expect(selectedText()).toBe('')
          expect(marks.has('book-lookup-word')).toBe(false)
        })

        it('marks the other places near the screen that name the card’s character, and not the name lit', async () => {
          on()
          const { marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { length: 8 }),
          })
          laidOut(host)
          await tapOn(host, 2, 6)
          expect(cardsDrawn()).toEqual(['Ned Reed'])
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
        })

        it('passes over a figure set between two blocks near the screen', async () => {
          on()
          const { marked } = highlights()
          const figured = {
            ...cast,
            editions: {
              ...cast.editions,
              en: {
                ...cast.editions.en,
                figures: [{ before: 3, image: 1, alt: 'a lamp' }],
              },
            },
          }
          const { host } = await withCharacters(figured, inEnglish, {
            characters: () => answered([NED], { length: 8 }),
          })
          laidOut(host)
          await tapOn(host, 2, 6)
          expect(
            host.querySelector('.book-text > [data-block="2"]')
              ?.nextElementSibling?.tagName,
          ).toBe('FIGURE')
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
        })

        it('marks what of a name the light on the word leaves', async () => {
          on()
          const { marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED]),
          })
          laidOut(host)
          await tapOn(host, 3, 13)
          expect(marked('book-lookup-word')).toEqual(['3:Reed'])
          expect(marked('book-mention')).toEqual(['2:Ned Reed', '3:Ned'])
        })

        it('marks the mentions for a look-up from the selection bar, which lights no word of its own', async () => {
          const { marks, marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED]),
          })
          laidOut(host)
          await lookUpFromBar(host, 3, 8, 16)
          expect(cardsDrawn()).toEqual(['Ned Reed'])
          expect(marked('book-mention')).toEqual(['2:Ned Reed', '3:Ned Reed'])
          expect(marks.has('book-lookup-word')).toBe(false)
        })

        it('marks nothing in an edition swapped to after a look-up from the selection bar, though that edition’s own look-up marks the name there', async () => {
          const { marks, marked } = highlights()
          const { host } = await withCharacters(lettered(true), inEnglish, {
            characters: () => answered([NED]),
          })
          laidOut(host)
          await lookUpFromBar(host, 3, 8, 16)
          expect(marked('book-mention')).toEqual(['2:Ned Reed', '3:Ned Reed'])
          press(' ')
          await land()
          expect([...marks.keys()]).toEqual([])
          await lookUpFromBar(host, 3, 5, 13)
          expect(lookUpTitle()).toBe('Ned Reed')
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
        })

        it('marks again as a scroll brings another block near, once a frame, and not for a scroll that brings none', async () => {
          on()
          const { marks, marked } = highlights()
          const frame = frames()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { length: 8 }),
          })
          const scroll = laidOut(host)
          await tapOn(host, 2, 6)
          const first = marks.get('book-mention')
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
          const before = frame.asked()
          scroll(20)
          scroll(50)
          expect(frame.asked()).toBe(before + 1)
          frame.run()
          expect(marks.get('book-mention')).toBe(first)
          scroll(500)
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
          frame.run()
          expect(marked('book-mention')).toEqual(['3:Ned Reed', '4:Ned Reed'])
        })

        it('clears the marks as the look-up closes, and a frame still waiting marks nothing', async () => {
          on()
          const { marks, marked } = highlights()
          const frame = frames()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([NED], { length: 8 }),
          })
          const scroll = laidOut(host)
          await tapOn(host, 2, 6)
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
          scroll(500)
          press('Escape')
          expect(marks.has('book-mention')).toBe(false)
          frame.run()
          expect(marks.has('book-mention')).toBe(false)
        })

        it('clears the marks on a new tap that names no one, and moves the word’s own mark', async () => {
          on()
          const { marks, marked } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: (text) =>
              text.startsWith('Ned')
                ? answered([NED], { length: 8 })
                : answered([]),
          })
          laidOut(host)
          await tapOn(host, 2, 6)
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
          await tapOn(host, 1, 1)
          expect(lookUpTitle()).toBe('Ada')
          expect(marks.has('book-mention')).toBe(false)
          expect(marked('book-lookup-word')).toEqual(['1:Ada'])
        })

        it('marks nothing where a tap on a surname names several', async () => {
          on()
          const { marks } = highlights()
          const { host } = await withCharacters(cast, inEnglish, {
            characters: () => answered([bySurname(NED), bySurname(ADA)]),
          })
          laidOut(host)
          await tapOn(host, 3, 13)
          expect(cardsDrawn()).toEqual(['Ned Reed', 'Ada Reed'])
          expect(marks.has('book-mention')).toBe(false)
        })

        it('marks nothing in an edition swapped to, and marks the text drawn anew on the way back', async () => {
          on()
          const { marks, marked } = highlights()
          const { host } = await withCharacters(lettered(false), inEnglish, {
            characters: () => answered([NED], { length: 8 }),
          })
          laidOut(host)
          await tapOn(host, 2, 6)
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
          press(' ')
          await land()
          expect(edition(host)).toBe('하나')
          expect(
            host.querySelector('[data-block="3"] [data-text]')?.textContent,
          ).toContain('Ned Reed')
          expect([...marks.keys()]).toEqual([])
          press(' ')
          await land()
          expect(edition(host)).toBe('One')
          expect(marked('book-mention')).toEqual(['3:Ned Reed'])
          expect(marked('book-lookup-word')).toEqual(['2:Ned Reed'])
        })
      })
    })
  })

  it('hands over every highlight as cards for Anki from the notes list', async () => {
    const made: Blob[] = []
    vi.stubGlobal('URL', {
      ...URL,
      createObjectURL: (blob: Blob) => {
        made.push(blob)
        return 'blob:words'
      },
      revokeObjectURL: vi.fn(),
    })
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(() => {})
    const { host } = await opened(undefined, sentenced, [
      note({ id: 'w', slot: 1, text: { start: 0, end: 3 }, body: 'who' }),
    ])
    press('a')
    const exporting = [
      ...host.querySelectorAll('[aria-label="Notes"] button'),
    ].find((b) => b.textContent === 'Export words') as HTMLElement
    act(() => exporting.click())
    expect(click).toHaveBeenCalledTimes(1)
    expect(await made[0]?.text()).toBe('Who,who<br><i>Who asked.</i>')
  })

  it('writes no note before the notes have been read, which a write would overwrite', async () => {
    const { fetch } = server()
    const answer = fetch.getMockImplementation()
    fetch.mockImplementation((url: string, init?: RequestInit) =>
      url === '/api/works/w/notes'
        ? new Promise<Response>(() => {})
        : (answer?.(url, init) as Promise<Response>),
    )
    vi.stubGlobal('fetch', fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
    await land()
    press('n')
    await saved()
    expect(fetch.mock.calls.some(([, init]) => init?.method === 'PUT')).toBe(
      false,
    )
    expect(host.querySelector('.book-notice')).toBeNull()
    expect(host.querySelector('.book-note-mark')).toBeNull()
    select(host, 1, 1, 6)
    touch(host, { travel: 40 })
    expect(host.querySelector('[role="toolbar"]')).toBeNull()
  })

  it('says why a write of the notes was refused, and keeps them', async () => {
    const { fetch } = server()
    const answer = fetch.getMockImplementation()
    fetch.mockImplementation(async (url: string, init?: RequestInit) =>
      url === '/api/works/w/notes' && init?.method === 'PUT'
        ? new Response('no room', { status: 507 })
        : (answer?.(url, init) as Promise<Response>),
    )
    vi.stubGlobal('fetch', fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
    await land()
    press('n')
    await saved()
    expect(host.querySelector('.book-notice')?.textContent).toContain('no room')
    expect(host.querySelectorAll('.book-note-mark')).toHaveLength(1)
  })

  it('says when the notes could not be read', async () => {
    const { fetch } = server()
    const answer = fetch.getMockImplementation()
    fetch.mockImplementation(async (url: string, init?: RequestInit) =>
      url === '/api/works/w/notes'
        ? new Response('down', { status: 500 })
        : (answer?.(url, init) as Promise<Response>),
    )
    vi.stubGlobal('fetch', fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
    await land()
    expect(host.querySelector('.book-notice')?.textContent).toContain(
      'could not be read',
    )
  })

  it('offers nothing for a selection that holds none of its first paragraph’s words', async () => {
    const { host } = await opened()
    const from = host.querySelector(
      '[data-block="1"] [data-sentence]',
    )?.firstChild
    const to = host.querySelector(
      '[data-block="2"] [data-sentence]',
    )?.firstChild
    const range = document.createRange()
    range.setStart(from as Node, (from?.textContent ?? '').length)
    range.setEnd(to as Node, 3)
    window.getSelection()?.removeAllRanges()
    window.getSelection()?.addRange(range)
    touch(host, { travel: 40 })
    expect(host.querySelector('[role="toolbar"]')).toBeNull()
  })

  it('keeps a reader’s notes on top of another device’s, and shows both', async () => {
    const { fetch, written } = server()
    let refused = false
    const answer = fetch.getMockImplementation()
    fetch.mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/notes' && init?.method === 'PUT' && !refused) {
        refused = true
        return new Response('these notes changed since you read them', {
          status: 412,
        })
      }
      if (url === '/api/works/w/notes' && !init?.method && refused) {
        return Response.json(
          { work: 'w', notes: [note({ id: 'elsewhere', slot: 2 })] },
          { headers: { etag: '"n9"' } },
        )
      }
      return answer?.(url, init) as Promise<Response>
    })
    vi.stubGlobal('fetch', fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(<BookReader work="w" onLeave={vi.fn()} />)
    await land()
    press('n')
    await saved()
    expect(written.at(-1)?.notes.map((n) => n.id)).toContain('elsewhere')
    expect(host.querySelectorAll('.book-note-mark')).toHaveLength(2)
    expect(host.querySelector('.book-notice')?.textContent).toContain(
      'kept on top',
    )
  })

  it('opens a note on a slow double tap on its highlight, leaving the bars down', async () => {
    const { host } = await opened(undefined, book, [highlightOn('en', 1, 0, 3)])
    const on = host.querySelector('mark')
    touch(host, { on, at: 40_000 })
    touch(host, { on, at: 40_250 })
    expect(
      host.querySelector('[role="dialog"][aria-label="Note"]'),
    ).not.toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('keeps the words typed when the note is closed from its button', async () => {
    const { host, written } = await opened()
    press('n')
    const dialog = host.querySelector('[role="dialog"][aria-label="Note"]')
    const box = dialog?.querySelector('textarea') as HTMLTextAreaElement
    await typeInto(box, 'kept')
    const close = [...(dialog?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent === 'Close',
    )
    act(() => close?.click())
    await saved()
    expect(written.at(-1)?.notes[0].body).toBe('kept')
  })

  it('reads out how long has been read, and after a minute how long the chapter will take', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-11-15T10:00:00Z'))
    const { host } = await opened()
    press('Tab')
    expect(readout(host)).toBe(
      '0% · <1m read · CHAPTER ONE · 0% of the chapter',
    )
    vi.setSystemTime(new Date('2026-11-15T10:02:00Z'))
    scrolledPast(1)
    act(() => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
    })
    // 11 characters read in two minutes, and 23 left in the chapter.
    expect(readout(host)).toBe(
      '19% · 2m read · ~4m left in the chapter · CHAPTER ONE · 32% of the chapter',
    )
    vi.useRealTimers()
  })

  it('starts the pace afresh on swapping to another edition', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-11-15T10:00:00Z'))
    const { host } = await opened()
    press('Tab')
    vi.setSystemTime(new Date('2026-11-15T10:02:00Z'))
    scrolledPast(1)
    act(() => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
    })
    expect(readout(host)).toContain('left in the chapter')
    press(' ')
    // The other edition's text runs at its own rate, so the one read so far
    // says nothing of it.
    expect(readout(host)).not.toContain('left in the chapter')
    vi.useRealTimers()
  })

  it('does not count a jump as reading', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-11-15T10:00:00Z'))
    const long = {
      ...book,
      editions: {
        ...book.editions,
        en: {
          ...book.editions.en,
          blocks: [
            { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
            { text: 'x'.repeat(4000), heading: false, sentences: [[0, 4000]] },
            { text: 'Nobody.', heading: false, sentences: [[0, 7]] },
          ],
        },
      },
    }
    const { host } = await opened(undefined, long)
    press('Tab')
    vi.setSystemTime(new Date('2026-11-15T10:02:00Z'))
    scrolledPast(2)
    act(() => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
    })
    expect(readout(host)).toMatch(/^\d+% · 2m read · CHAPTER ONE/)
    vi.useRealTimers()
  })

  it('goes where the progress line is pressed and dragged to', async () => {
    const { host } = await opened()
    touch(host, { x: 300 })
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const line = slider?.querySelector<HTMLElement>('.reader-progress')
    if (!slider || !line) throw new Error('no progress line')
    line.getBoundingClientRect = () =>
      ({ left: 100, width: 200, top: 0, bottom: 4 }) as DOMRect
    let captured = false
    slider.setPointerCapture = () => {
      captured = true
    }
    slider.hasPointerCapture = () => captured
    act(() => {
      slider.dispatchEvent(
        new PointerEvent('pointerdown', { clientX: 200, bubbles: true }),
      )
    })
    // Half way through the 57 characters is in block 2.
    expect(scrolledTo().at(-1)).toBe('2')
    act(() => {
      slider.dispatchEvent(
        new PointerEvent('pointermove', { clientX: 300, bubbles: true }),
      )
    })
    expect(scrolledTo().at(-1)).toBe('5')
    captured = false
    act(() => {
      slider.dispatchEvent(
        new PointerEvent('pointermove', { clientX: 100, bubbles: true }),
      )
    })
    expect(scrolledTo().at(-1)).toBe('5')
  })

  it('moves along the progress line on its keys, which turn no page', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    touch(host, { x: 300 })
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const key = (key: string) => {
      const event = new KeyboardEvent('keydown', {
        key,
        bubbles: true,
        cancelable: true,
      })
      act(() => {
        slider?.dispatchEvent(event)
      })
      return event
    }
    expect(key('End').defaultPrevented).toBe(true)
    expect(page()).toBe(2)
    key('Home')
    expect(page()).toBe(0)
    key('ArrowRight')
    expect(page()).toBe(0)
    expect(key('x').defaultPrevented).toBe(false)
  })

  it('moves a tenth of the book on the progress line’s Page Down, and a hundredth on its arrow', async () => {
    const long = {
      ...book,
      editions: {
        ...book.editions,
        en: {
          ...book.editions.en,
          blocks: [
            { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
            { text: 'x'.repeat(100), heading: false, sentences: [[0, 100]] },
          ],
        },
      },
    }
    const { host } = await opened(undefined, long)
    press('Tab')
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const key = (key: string) =>
      act(() => {
        slider?.dispatchEvent(
          new KeyboardEvent('keydown', {
            key,
            bubbles: true,
            cancelable: true,
          }),
        )
      })
    const at = () => Number(slider?.getAttribute('aria-valuenow'))
    key('ArrowRight')
    expect(scrolledTo().at(-1)).toBe('0')
    key('Home')
    key('PageDown')
    expect(scrolledTo().at(-1)).toBe('1')
    const tenth = at()
    key('Home')
    // Each press inside the heading leaves the reader at its start, and the
    // next goes on from where the last asked, not from the heading again.
    for (let i = 0; i < 10; i++) key('ArrowRight')
    expect(at()).toBeGreaterThanOrEqual(tenth)
  })

  it('starts the progress line’s steps afresh in another edition, and once it has lost focus', async () => {
    const long = {
      ...book,
      editions: {
        ...book.editions,
        en: {
          ...book.editions.en,
          blocks: [
            { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
            { text: 'x'.repeat(100), heading: false, sentences: [[0, 100]] },
          ],
        },
      },
    }
    const { host } = await opened(undefined, long)
    scrolledPast(0)
    press('Tab')
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const presses = (n: number) => {
      for (let i = 0; i < n; i++) {
        act(() => {
          slider?.dispatchEvent(
            new KeyboardEvent('keydown', {
              key: 'ArrowRight',
              bubbles: true,
              cancelable: true,
            }),
          )
        })
      }
    }
    const at = () => slider?.getAttribute('aria-valuenow')
    // Three hundredths into English's heading, a tenth of the book.
    presses(3)
    expect(at()).toBe('0')
    press(' ')
    // Korean's heading is a ninth of its 18 characters: nine hundredths in
    // stay in it, where twelve would not.
    presses(9)
    expect(at()).toBe('0')
    act(() => {
      slider?.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    })
    presses(3)
    expect(at()).toBe('0')
  })

  it('never takes the reader back on the progress line’s forward arrow, from a later page of a paragraph', async () => {
    paged()
    // The long paragraph runs from page 0 to page 2.
    const page = paging([0, [0, 2]])
    const long = {
      ...book,
      editions: {
        ...book.editions,
        en: {
          ...book.editions.en,
          blocks: [
            { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
            { text: 'x'.repeat(100), heading: false, sentences: [[0, 100]] },
          ],
        },
      },
    }
    const { host } = await opened(undefined, long)
    touch(host, { x: 300 })
    press('ArrowRight')
    press('ArrowRight')
    expect(page()).toBe(2)
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    act(() => {
      slider?.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'ArrowRight',
          bubbles: true,
          cancelable: true,
        }),
      )
    })
    expect(page()).toBe(2)
  })

  it('never takes the reader back on the progress line’s forward steps, compared, from a later page of a row', async () => {
    paged('compare')
    // Read in Korean, the row of its blocks 1 and 2 runs from page 0 to 2.
    const page = paging([0, [0, 2], 2, 2, 2])
    const { host } = await opened({ edition: 'ko', slot: 0 })
    touch(host, { x: 300 })
    press('ArrowRight')
    press('ArrowRight')
    expect(page()).toBe(2)
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    // Three tenths on, the third lands on Korean block 2, inside the row.
    for (let i = 0; i < 3; i++) {
      act(() => {
        slider?.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'PageDown',
            bubbles: true,
            cancelable: true,
          }),
        )
      })
      expect(page()).toBe(2)
    }
  })

  it('takes the reader on, compared, on a progress line step that lands past the row at the top', async () => {
    paged('compare')
    // Read in Korean, the row of its blocks 1 and 2 runs from page 0 to 2,
    // and the row of its block 3 is on page 3.
    const page = paging([0, [0, 2], 2, 3, 3])
    const { host } = await opened({ edition: 'ko', slot: 0 })
    touch(host, { x: 300 })
    press('ArrowRight')
    press('ArrowRight')
    expect(page()).toBe(2)
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const step = () =>
      act(() => {
        slider?.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'PageDown',
            bubbles: true,
            cancelable: true,
          }),
        )
      })
    // Six tenths on is still Korean block 2, inside the row; seven is its
    // block 3.
    for (let i = 0; i < 6; i++) {
      step()
      expect(page()).toBe(2)
    }
    step()
    expect(page()).toBe(3)
  })

  it('takes the reader back on the progress line’s back arrow, from a later page of a paragraph, to the block before', async () => {
    paged()
    // The long paragraph runs from page 0 to page 2.
    const page = paging([0, [0, 2]])
    const { host } = await opened(undefined, headed)
    touch(host, { x: 300 })
    press('ArrowRight')
    press('ArrowRight')
    expect(page()).toBe(2)
    act(() => {
      host.querySelector('[role="slider"]')?.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'ArrowLeft',
          bubbles: true,
          cancelable: true,
        }),
      )
    })
    expect(page()).toBe(0)
  })

  it('starts the progress line’s steps afresh after a press on the line', async () => {
    const { host } = await opened(undefined, headed)
    // The line a hundred pixels long, and every block on the screen.
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      () => ({ top: 0, bottom: 100, left: 0, width: 100 }) as DOMRect,
    )
    press('Tab')
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const at = () => slider?.getAttribute('aria-valuenow')
    const presses = (n: number) => {
      for (let i = 0; i < n; i++) {
        act(() => {
          slider?.dispatchEvent(
            new KeyboardEvent('keydown', {
              key: 'ArrowRight',
              bubbles: true,
              cancelable: true,
            }),
          )
        })
      }
    }
    // Nine hundredths into the heading, which runs to 11 of 111 characters.
    presses(9)
    expect(at()).toBe('0')
    // A press five hundredths along, in the heading too.
    act(() => {
      slider?.dispatchEvent(
        new PointerEvent('pointerdown', { clientX: 5, bubbles: true }),
      )
    })
    expect(at()).toBe('0')
    // Gone on from the press, not from the nine hundredths asked before it.
    presses(1)
    expect(at()).toBe('0')
  })

  it('starts the progress line’s steps afresh once the reader has moved by other means', async () => {
    const longer = {
      ...headed,
      editions: {
        ...headed.editions,
        en: {
          ...headed.editions.en,
          blocks: [
            ...headed.editions.en.blocks,
            { text: 'y'.repeat(100), heading: false, sentences: [[0, 100]] },
          ],
        },
      },
    }
    const { host } = await opened(undefined, longer)
    const scrolled = (first: number) => {
      scrolledPast(first)
      act(() => {
        host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
      })
    }
    scrolled(1)
    press('Tab')
    const slider = host.querySelector<HTMLElement>('[role="slider"]')
    const at = () => slider?.getAttribute('aria-valuenow')
    const presses = (n: number) => {
      for (let i = 0; i < n; i++) {
        act(() => {
          slider?.dispatchEvent(
            new KeyboardEvent('keydown', {
              key: 'ArrowRight',
              bubbles: true,
              cancelable: true,
            }),
          )
        })
      }
    }
    // The paragraph after the heading runs from 11 to 111 of 211 characters:
    // five hundredths on stay in it.
    expect(at()).toBe('5')
    presses(5)
    expect(at()).toBe('5')
    // Scrolled back to the heading, a hundredth on is in the heading.
    scrolled(0)
    expect(at()).toBe('0')
    presses(1)
    expect(at()).toBe('0')
  })

  it('pins, compared, a paragraph on the left to its match on the right, for an administrator', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host, pinned, fetch } = await opened(undefined, book, [], true)
    press('c')
    const banner = () => host.querySelector('.book-pinning')?.textContent
    expect(banner()).toContain('Pick a paragraph on the left')
    touch(host, { on: host.querySelector('[data-pick="left:1"]') })
    expect(banner()).toContain('Now pick its match on the right')
    touch(host, { on: host.querySelector('[data-pick="right:2"]') })
    await land()
    expect(pinned).toEqual([
      { edition: 'ko', hub: 'en', hub_block: 1, block: 2 },
    ])
    expect(banner()).toBeUndefined()
    expect(
      fetch.mock.calls.filter(([url]) => url === '/api/works/w/book'),
    ).toHaveLength(2)
  })

  it('leaves the mousedown of a tap that picks a paragraph to the browser', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, book, [], true)
    press('c')
    const pick = host.querySelector('[data-pick="left:1"]') as Element
    touch(host, { on: pick })
    expect(host.querySelector('.book-pinning')?.textContent).toContain(
      'Now pick its match on the right',
    )
    expect(
      pick.dispatchEvent(
        new MouseEvent('mousedown', { bubbles: true, cancelable: true }),
      ),
    ).toBe(true)
  })

  it('waits for a paragraph on the left before a match on the right', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host, pinned } = await opened(undefined, book, [], true)
    press('c')
    touch(host, { on: host.querySelector('[data-pick="right:2"]') })
    await land()
    expect(pinned).toEqual([])
    expect(host.querySelector('.book-pinning')?.textContent).toContain(
      'on the left',
    )
  })

  it('unpins a pinned paragraph, and cancels on Escape', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const held = {
      ...book,
      alignments: [{ ...book.alignments[0], pins: [[1, 1]] }],
    }
    const { host, pinned } = await opened(undefined, held, [], true)
    expect(
      host.querySelector('.book-row[data-pinned]')?.getAttribute('data-block'),
    ).toBe('1')
    press('c')
    touch(host, { on: host.querySelector('[data-pick="left:2"]') })
    expect(host.querySelector('.book-pinning button')?.textContent).toBe(
      'Cancel',
    )
    touch(host, { on: host.querySelector('[data-pick="left:1"]') })
    const unpin = [
      ...host.querySelectorAll<HTMLElement>('.book-pinning button'),
    ].find((b) => b.textContent === 'Unpin')
    act(() => unpin?.click())
    await land()
    expect(pinned).toEqual([
      { edition: 'ko', hub: 'en', hub_block: 1, block: null },
    ])
    press('c')
    press('Escape')
    expect(host.querySelector('.book-pinning')).toBeNull()
  })

  it('pins to the edition the second column shows, though another comes next', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    localStorage.setItem('taiyaku.book-columns.w', '["ko"]')
    const { host, pinned } = await opened(undefined, three, [], true)
    press('c')
    touch(host, { on: host.querySelector('[data-pick="left:1"]') })
    touch(host, { on: host.querySelector('[data-pick="right:2"]') })
    await land()
    expect(pinned).toEqual([
      { edition: 'ko', hub: 'en', hub_block: 1, block: 2 },
    ])
  })

  it('offers to unpin, and unpins, only what the second column’s edition has pinned', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    localStorage.setItem('taiyaku.book-columns.w', '["ko"]')
    const held = {
      ...three,
      alignments: [
        { ...three.alignments[0], pins: [[1, 1]] },
        { ...three.alignments[1], pins: [[0, 0]] },
      ],
    }
    const { host, pinned } = await opened(undefined, held, [], true)
    expect(
      [...host.querySelectorAll<HTMLElement>('.book-row[data-pinned]')].map(
        (row) => row.dataset.block,
      ),
    ).toEqual(['1'])
    const buttons = () =>
      [...host.querySelectorAll<HTMLElement>('.book-pinning button')].map(
        (b) => b.textContent,
      )
    press('c')
    touch(host, { on: host.querySelector('[data-pick="left:0"]') })
    expect(buttons()).toEqual(['Cancel'])
    touch(host, { on: host.querySelector('[data-pick="left:1"]') })
    expect(buttons()).toEqual(['Unpin', 'Cancel'])
    const unpin = [
      ...host.querySelectorAll<HTMLElement>('.book-pinning button'),
    ].find((b) => b.textContent === 'Unpin')
    act(() => unpin?.click())
    await land()
    expect(pinned).toEqual([
      { edition: 'ko', hub: 'en', hub_block: 1, block: null },
    ])
  })

  it('pins nothing for a reader who is not an administrator', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened()
    press('c')
    expect(host.querySelector('.book-pinning')).toBeNull()
  })

  it('pins nothing outside compare, and stops pinning when compare is left', async () => {
    const { host } = await opened(undefined, book, [], true)
    press('c')
    expect(host.querySelector('.book-pinning')).toBeNull()
    press('Tab')
    act(() => layoutButton(host, 'Compare editions').click())
    press('c')
    expect(host.querySelector('.book-pinning')).not.toBeNull()
    act(() => layoutButton(host, 'One edition').click())
    expect(host.querySelector('.book-pinning')).toBeNull()
  })

  it('compares a third edition added from the columns’ bar, kept for the work', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, three)
    const cells = () =>
      [...host.querySelectorAll<HTMLElement>('.book-row')].map((row) =>
        [...row.children].map((cell) => cell.textContent),
      )
    // The next edition in order, French, is the second column until chosen.
    expect(cells()[1]).toEqual([
      'Who asked. Nobody knew.',
      'Qui demanda. Personne.',
    ])
    const add = host.querySelector<HTMLElement>('[aria-label="Add a column"]')
    act(() => add?.click())
    expect(cells()[1]).toEqual([
      'Who asked. Nobody knew.',
      'Qui demanda. Personne.',
      '누가 물었다.아무도 몰랐다.',
    ])
    expect(localStorage.getItem('taiyaku.book-columns.w')).toBe('["fr","ko"]')
    expect(host.querySelector('[aria-label="Add a column"]')).toBeNull()
    press('Tab')
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'English · French · Korean',
    )
    const remove = host.querySelector<HTMLElement>(
      '[aria-label="Remove French"]',
    )
    act(() => remove?.click())
    expect(cells()[1]).toEqual([
      'Who asked. Nobody knew.',
      '누가 물었다.아무도 몰랐다.',
    ])
    expect(host.querySelector('[aria-label="Remove Korean"]')).toBeNull()
  })

  it('changes a column’s edition from its choice in the bar', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, three)
    const choice = host.querySelector<HTMLSelectElement>(
      '[aria-label="Column 2"]',
    ) as HTMLSelectElement
    expect([...choice.options].map((o) => o.textContent)).toEqual([
      'French',
      'Korean',
    ])
    act(() => {
      choice.value = 'ko'
      choice.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(
      host.querySelector('.book-row:nth-child(2) [data-column="ko"]')
        ?.textContent,
    ).toBe('누가 물었다.아무도 몰랐다.')
    expect(host.querySelector('[data-column="fr"]')).toBeNull()
  })

  it('lights, across three columns, what each has for a sentence double-tapped in any', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    localStorage.setItem('taiyaku.book-columns.w', '["ko","fr"]')
    const { host } = await opened(undefined, three)
    const french = host.querySelector('[data-column="fr"] [data-sentence="2"]')
    touch(host, { on: french, at: 95_000 })
    touch(host, { on: french, at: 95_150 })
    expect(litSentences(host)).toEqual([
      'Nobody knew.',
      '아무도 몰랐다.',
      'Personne.',
    ])
  })

  it('pins nothing with three columns, a pin joining two', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    localStorage.setItem('taiyaku.book-columns.w', '["ko","fr"]')
    const { host } = await opened(undefined, three, [], true)
    press('c')
    expect(host.querySelector('.book-pinning')).toBeNull()
  })

  it('puts the edition swapped to first, the one left taking its column', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    localStorage.setItem('taiyaku.book-columns.w', '["ko","fr"]')
    const { host } = await opened(undefined, three)
    press(' ')
    expect(
      [
        ...host.querySelectorAll<HTMLElement>(
          '.book-row:first-child > [data-column]',
        ),
      ].map((c) => c.dataset.column),
    ).toEqual(['ko', 'en', 'fr'])
  })

  /**
   * Read in Korean beside English and scrolled to the last row, which holds
   * English's blocks 3 to 5 beside none of Korean's 4, so is named Korean
   * block 4, past Korean's end.
   */
  async function atTheLastRow() {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const opening = await opened({ edition: 'ko', slot: 0 })
    expect(
      [...opening.host.querySelectorAll<HTMLElement>('.book-row')].map(
        (row) => row.dataset.block,
      ),
    ).toEqual(['0', '1', '3', '4'])
    scrolledPast(4)
    act(() => {
      opening.host
        .querySelector('.book-text')
        ?.dispatchEvent(new Event('scroll'))
    })
    return opening
  }

  it('swaps, compared, from a last row of only English to the English it shows, not the book’s start', async () => {
    await atTheLastRow()
    press(' ')
    expect(scrolledTo().at(-1)).toBe('3')
  })

  it('swaps, compared, from the second of two last rows of only English to the English it shows', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    // English's blocks 3 and 4 to 5 are two rows, each beside none of
    // Korean's 4, so both are named Korean block 4.
    const split = {
      ...book,
      alignments: [
        {
          ...book.alignments[0],
          links: [
            [0, 1, 0, 1],
            [1, 2, 1, 3],
            [2, 3, 3, 4],
            [3, 4, 4, 4],
            [4, 6, 4, 4],
          ],
        },
      ],
    }
    const { host } = await opened({ edition: 'ko', slot: 0 }, split)
    const rows = [...host.querySelectorAll<HTMLElement>('.book-row')]
    expect(rows.map((row) => row.dataset.block)).toEqual([
      '0',
      '1',
      '3',
      '4',
      '4',
    ])
    // Every row but the last scrolled past the top.
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        const index = rows.indexOf(this as HTMLElement)
        if (index < 0) return { top: 0, bottom: 0 } as DOMRect
        return { top: 0, bottom: index < 4 ? -10 : 100 } as DOMRect
      },
    )
    act(() => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
    })
    press(' ')
    expect(scrolledTo().at(-1)).toBe('4')
  })

  it('lights, swapping compared from a last row of only English, the English blocks the row shows', async () => {
    const { host } = await atTheLastRow()
    press(' ')
    // Compared, no block's light is drawn: it is seen once compare is left
    // before it fades.
    press('Tab')
    act(() =>
      [...host.querySelectorAll<HTMLElement>('[aria-label="Layout"] button')]
        .find((b) => b.textContent === 'One edition')
        ?.click(),
    )
    expect(lit(host)).toEqual(['3', '4', '5'])
  })

  it('writes a note, compared, from a last row of only English on Korean’s last block', async () => {
    const { written } = await atTheLastRow()
    press('n')
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 750))
    })
    await land()
    expect(written.at(-1)?.notes.map((n) => [n.edition, n.slot])).toEqual([
      ['ko', 3],
    ])
  })

  it('reads aloud, compared, from a last row of only English from Korean’s last block', async () => {
    const speech = voices()
    await atTheLastRow()
    press('r')
    expect(speech.spoken.map((u) => [u.text, u.lang])).toEqual([
      ['아무도.', 'ko'],
    ])
  })

  it('puts the edition swapped to first and the one left beside it, with no columns ever chosen', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, three)
    const shown = () =>
      [
        ...host.querySelectorAll<HTMLElement>(
          '.book-row:first-child > [data-column]',
        ),
      ].map((c) => c.dataset.column)
    const [read, beside] = shown()
    expect(shown()).toHaveLength(2)
    press(' ')
    expect(shown()).toEqual([beside, read])
    press(' ')
    expect(shown()).toEqual([read, beside])
  })

  it('keeps the edition after the one read as the column beside it, with no columns ever chosen, after a swap made reading alone', async () => {
    const { host } = await opened(undefined, three)
    press(' ')
    press('Tab')
    act(() =>
      [...host.querySelectorAll<HTMLElement>('[aria-label="Layout"] button')]
        .find((b) => b.textContent === 'Compare editions')
        ?.click(),
    )
    // French is read, swapped to from English, and Korean is the edition
    // after it.
    expect(
      [
        ...host.querySelectorAll<HTMLElement>(
          '.book-row:first-child > [data-column]',
        ),
      ].map((c) => c.dataset.column),
    ).toEqual(['fr', 'ko'])
  })

  it('lets go of a lit pair of sentences when compare is left', async () => {
    const { host } = await opened(undefined, sentenced)
    press('Tab')
    act(() => layoutButton(host, 'Compare editions').click())
    const second = host.querySelector(
      '[data-pick="left:1"] [data-sentence="2"]',
    )
    touch(host, { on: second, at: 80_000 })
    touch(host, { on: second, at: 80_150 })
    expect(host.querySelectorAll('[data-sentence][data-lit]')).toHaveLength(2)
    act(() => layoutButton(host, 'One edition').click())
    act(() => layoutButton(host, 'Compare editions').click())
    expect(host.querySelectorAll('[data-sentence][data-lit]')).toHaveLength(0)
  })

  it('leaves Space to a focused bar button rather than swapping', async () => {
    const { host } = await opened()
    press('Tab')
    const button = host.querySelector<HTMLElement>('.reader-top-bar button')
    expect(button?.closest('[role="dialog"]')).toBeNull()
    button?.focus()
    const space = new KeyboardEvent('keydown', {
      key: ' ',
      bubbles: true,
      cancelable: true,
    })
    act(() => {
      button?.dispatchEvent(space)
    })
    expect(space.defaultPrevented).toBe(false)
    expect(edition(host)).toBe('CHAPTER ONE')
  })

  it('leaves Space and Enter to a focused button, and a panel’s keys to the panel', async () => {
    const { host } = await opened()
    press('t')
    const entry = host.querySelector<HTMLElement>(
      '[aria-label="Contents"] li button',
    )
    entry?.focus()
    const space = new KeyboardEvent('keydown', {
      key: ' ',
      bubbles: true,
      cancelable: true,
    })
    act(() => {
      entry?.dispatchEvent(space)
    })
    expect(space.defaultPrevented).toBe(false)
    expect(edition(host)).toBe('CHAPTER ONE')
    act(() => {
      entry?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 's', bubbles: true }),
      )
    })
    expect(host.querySelector('[aria-label="Settings"]')).toBeNull()
    act(() => {
      entry?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
      )
    })
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
  })

  it('opens a saved block past a shorter book’s end on its last block', async () => {
    const { host } = await opened({ edition: 'ko', slot: 40 })
    expect(scrolledTo()).toEqual(['3'])
    press('Tab')
    expect(readout(host)).toMatch(/^\d+% · /)
  })

  it('swaps to nothing on Space with one edition', async () => {
    const plain = {
      label: 'Plain',
      language: null,
      blocks: [{ text: 'x', heading: false, sentences: [[0, 1]] }],
    }
    const { host, puts } = await opened(undefined, {
      ...book,
      hub: 'p',
      editions: { p: plain },
      alignments: [],
    })
    press(' ')
    expect(host.querySelector('[data-lit]')).toBeNull()
    await land()
    expect(puts).toEqual([])
  })

  it('closes a note’s panel when an undo takes the note away', async () => {
    const { host } = await opened()
    press('n')
    expect(
      host.querySelector('[role="dialog"][aria-label="Note"]'),
    ).not.toBeNull()
    press('z')
    expect(host.querySelector('[role="dialog"][aria-label="Note"]')).toBeNull()
    tap(host)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('shows an open note’s words as an undo leaves them', async () => {
    const { host } = await opened(undefined, book, [
      note({ id: 'p', slot: 1, body: 'first' }),
    ])
    touch(host, { on: host.querySelector('[data-block="1"] .book-note-mark') })
    const box = () =>
      host.querySelector<HTMLTextAreaElement>('[aria-label="Note"] textarea')
    act(() => box()?.focus())
    await typeInto(box() as HTMLTextAreaElement, 'second')
    act(() => box()?.blur())
    expect(box()?.value).toBe('second')
    expect(
      host
        .querySelector('[data-block="1"] .book-note-mark')
        ?.getAttribute('title'),
    ).toBe('second')
    press('z')
    expect(
      host
        .querySelector('[data-block="1"] .book-note-mark')
        ?.getAttribute('title'),
    ).toBe('first')
    expect(box()?.value).toBe('first')
  })

  it('swaps on Space to the block that shows the same text, and back', async () => {
    const { host } = await opened()
    scrolledPast(2)
    press(' ')
    expect(edition(host)).toBe('1장')
    expect(scrolledTo().at(-1)).toBe('3')
    scrolledPast(3)
    press(' ')
    expect(edition(host)).toBe('CHAPTER ONE')
    expect(scrolledTo().at(-1)).toBe('2')
  })

  it('goes to the nth edition on its digit', async () => {
    const { host } = await opened()
    scrolledPast(1)
    press('2')
    expect(edition(host)).toBe('1장')
    expect(scrolledTo().at(-1)).toBe('1')
    press('9')
    expect(edition(host)).toBe('1장')
  })

  it('leaves keys typed into a field to the field', async () => {
    const { host } = await opened()
    const field = document.createElement('input')
    host.append(field)
    act(() => {
      field.dispatchEvent(
        new KeyboardEvent('keydown', { key: ' ', bubbles: true }),
      )
    })
    expect(edition(host)).toBe('CHAPTER ONE')
  })

  it('leaves Space with a modifier to the browser', async () => {
    const { host } = await opened()
    press(' ', { ctrlKey: true })
    expect(edition(host)).toBe('CHAPTER ONE')
  })

  it('saves the place a swap lands on once the reader has stopped', async () => {
    const { puts } = await opened()
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    scrolledPast(2)
    press(' ')
    expect(puts).toEqual([])
    await act(async () => {
      vi.advanceTimersByTime(1000)
    })
    vi.useRealTimers()
    await land()
    expect(puts).toEqual([{ edition: 'ko', slot: 3 }])
  })

  it('saves the block at the top once scrolling has stopped, and reads it out', async () => {
    const { host, puts } = await opened()
    tap(host)
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    scrolledPast(1)
    await act(async () => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
      vi.advanceTimersByTime(1000)
    })
    vi.useRealTimers()
    await land()
    expect(puts).toEqual([{ edition: 'en', slot: 1 }])
    expect(readout(host)).toBe(
      '19% · <1m read · CHAPTER ONE · 32% of the chapter',
    )
  })

  it('says a work with no book edition has none, and leaves from there', async () => {
    const { host, onLeave } = await opened(undefined, {
      title: 'T',
      hub: null,
      editions: {},
      alignments: [],
    })
    expect(host.querySelector('.book-failed')?.textContent).toContain(
      'no book edition',
    )
    host.querySelector('button')?.click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('says a book that would not load could not be opened', async () => {
    const { host, onLeave } = await opened(undefined, null)
    expect(host.querySelector('.book-failed')?.textContent).toContain(
      'could not be opened',
    )
    host.querySelector('button')?.click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('leaves on L', async () => {
    const { onLeave } = await opened()
    press('L')
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it.each([
    [undefined, 'Back to the library'],
    [{ word: 'Series', phrase: 'the series' }, 'Back to the series'],
    [{ word: 'Back', phrase: '' }, 'Back'],
  ])(
    'names the way back from a book that would not load for where it goes: %o',
    async (leaveTo, name) => {
      vi.stubGlobal('fetch', server(undefined, null).fetch)
      const onLeave = vi.fn()
      const host = mount(
        <BookReader work="w" onLeave={onLeave} leaveTo={leaveTo} />,
      )
      await land()
      expect(
        [...host.querySelectorAll('button')].map((b) => b.textContent),
      ).toEqual([name])
      host.querySelector('button')?.click()
      expect(onLeave).toHaveBeenCalledOnce()
    },
  )

  it('names the top bar’s way out by the word it is handed', async () => {
    vi.stubGlobal('fetch', server().fetch)
    Element.prototype.scrollIntoView = vi.fn()
    const host = mount(
      <BookReader
        work="w"
        onLeave={vi.fn()}
        leaveTo={{ word: 'Series', phrase: 'the series' }}
      />,
    )
    await land()
    tap(host)
    expect(host.querySelector('.reader-back')?.textContent).toBe('Series')
  })
})

/**
 * O431: the book hands its route the series the library's answer for the
 * work names, as the comic reader does, and from the answer alone: the
 * series the top bar keeps starts as `null` before the answer lands, which
 * would say "in no series" before anything is known.
 */
describe('the series the library’s answer names — O431', () => {
  const SEVEN = { id: 7, title: 'Seven' }

  /** The book, the library's answer for the work being `answer`. */
  async function reading(answer: Promise<Response>) {
    const { fetch } = server()
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string, init?: RequestInit) =>
        url === '/api/library/works/w' ? answer : fetch(url, init),
      ),
    )
    Element.prototype.scrollIntoView = vi.fn()
    const onSeries = vi.fn()
    mount(<BookReader work="w" onLeave={vi.fn()} onSeries={onSeries} />)
    await land()
    return onSeries
  }

  it('reports nothing while the answer is out, then its series, once', async () => {
    const answer = deferred<Response>()
    const onSeries = await reading(answer.promise)
    const out = [...onSeries.mock.calls]
    await act(async () => {
      answer.resolve(Response.json({ series: SEVEN }))
    })
    await land()
    expect([out, onSeries.mock.calls]).toEqual([[], [[SEVEN]]])
  })

  it('reports null for a work the answer files in no series', async () => {
    const onSeries = await reading(
      Promise.resolve(Response.json({ series: null })),
    )
    expect(onSeries.mock.calls).toEqual([[null]])
  })

  it('reports nothing when the answer fails', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    const onSeries = await reading(
      Promise.resolve(new Response('gone', { status: 500 })),
    )
    expect([onSeries.mock.calls, logged.mock.calls.length > 0]).toEqual([
      [],
      true,
    ])
  })
})

/** One edition, `en`, its one block `text` drawn with `marks`. */
const marked = (text: string, marks: unknown[]) => ({
  title: 'Marked',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        {
          text,
          heading: false,
          sentences: [[0, text.length]],
          ...(marks.length > 0 && { marks }),
        },
      ],
    },
  },
  alignments: [],
})

/** Block 0's own words, where its marks are drawn. */
const words = (host: HTMLElement) =>
  host.querySelector('[data-block="0"] [data-text]') as HTMLElement

describe('a block’s marks', () => {
  it('draws italic and bold as <i> and <b>, and a ruby with its reading set aside', async () => {
    const { host } = await opened(
      undefined,
      marked('坊ちゃん big bold', [
        { kind: 'ruby', start: 0, end: 1, reading: 'ぼっ' },
        { kind: 'italic', start: 5, end: 8 },
        { kind: 'bold', start: 9, end: 13 },
      ]),
    )
    const own = words(host)
    expect(own.querySelector('i')?.textContent).toBe('big')
    expect(own.querySelector('b')?.textContent).toBe('bold')
    expect(own.querySelector('ruby')?.outerHTML).toBe(
      '<ruby>坊<rt data-aside="true">ぼっ</rt></ruby>',
    )
  })

  it('counts a selection in the base text alone, as in the block without marks', async () => {
    const text = '坊ちゃんの話'
    const plain = await opened(undefined, marked(text, []))
    select(plain.host, 0, 1, 4)
    const unmarked = selectionIn(plain.host.querySelector('.book-text'))
    expect(unmarked?.range).toEqual({ start: 1, end: 4 })
    const { host } = await opened(
      undefined,
      marked(text, [
        { kind: 'ruby', start: 0, end: 1, reading: 'ぼっ' },
        { kind: 'italic', start: 1, end: 3 },
        { kind: 'link', start: 3, end: 5, block: 0, note: false },
      ]),
    )
    // Selected on the drawn text nodes themselves, not through `select`,
    // whose walker skips `[data-aside]` as `selectionIn`'s does: a reading
    // counted by both would still agree.
    const nodes: Text[] = []
    const walker = document.createTreeWalker(words(host), NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      nodes.push(node as Text)
    }
    expect(nodes.map((node) => node.data)).toEqual([
      '坊',
      'ぼっ',
      'ちゃ',
      'んの',
      '話',
    ])
    const range = document.createRange()
    range.setStart(nodes[2], 0)
    range.setEnd(nodes[3], 1)
    window.getSelection()?.removeAllRanges()
    window.getSelection()?.addRange(range)
    expect(window.getSelection()?.toString()).toBe('ちゃん')
    expect(selectionIn(host.querySelector('.book-text'))?.range).toEqual(
      unmarked?.range,
    )
  })

  it('draws no reading with furigana off, the base text as it is', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ furigana: false }),
    )
    const { host } = await opened(
      undefined,
      marked('坊ちゃん', [{ kind: 'ruby', start: 0, end: 1, reading: 'ぼっ' }]),
    )
    expect(words(host).querySelector('rt')).toBeNull()
    expect(words(host).textContent).toBe('坊ちゃん')
  })

  // The book's `<a><sup><i>` comes as three marks over one run, drawn in
  // `Runs`' one order whatever the book's was.
  it('nests a link, a superscript and an italic, the text whole', async () => {
    const { host } = await opened(
      undefined,
      marked('see 12 here', [
        { kind: 'italic', start: 4, end: 6 },
        { kind: 'sup', start: 4, end: 6 },
        { kind: 'link', start: 4, end: 6, block: 0, note: true },
      ]),
    )
    const own = words(host)
    const link = own.querySelector('a')
    expect(link?.dataset).toMatchObject({ linkBlock: '0', linkNote: 'true' })
    expect(link?.querySelector(':scope > i > sup')?.textContent).toBe('12')
    expect(own.textContent).toBe('see 12 here')
  })

  it('opens a web link in a new tab, even one read as a note', async () => {
    const { host } = await opened(
      undefined,
      marked('a site', [
        { kind: 'link', start: 2, end: 6, url: 'https://x.org/', note: true },
      ]),
    )
    const link = words(host).querySelector('a')
    expect(link?.getAttribute('href')).toBe('https://x.org/')
    expect(link?.target).toBe('_blank')
    expect(link?.rel).toBe('noopener noreferrer')
    expect(link?.dataset).toMatchObject({ linkUrl: 'https://x.org/' })
    expect(link?.dataset.linkNote).toBeUndefined()
  })

  it('draws an inline image from the book’s images, with no text of its own', async () => {
    const { host } = await opened(
      undefined,
      marked('ab', [
        { kind: 'image', start: 1, end: 1, image: 3, alt: 'a map' },
      ]),
    )
    const own = words(host)
    const image = own.querySelector('img')
    expect(image?.getAttribute('src')).toBe(bookImageUrl('w', 'en', 3))
    expect(image?.getAttribute('src')).toBe('/api/works/w/en/book/images/3')
    expect(image?.alt).toBe('a map')
    expect(own.textContent).toBe('ab')
  })

  it('opens an inline image tapped full size, as a figure opens', async () => {
    const { host } = await opened(
      undefined,
      marked('ab', [
        { kind: 'image', start: 1, end: 1, image: 3, alt: 'a map' },
      ]),
    )
    touch(host, { on: words(host).querySelector('.book-inline-image') })
    const full = host.querySelector<HTMLDialogElement>(
      'dialog.book-figure-full',
    )
    expect(full?.open).toBe(true)
    expect(full?.querySelector('img')?.getAttribute('src')).toBe(
      bookImageUrl('w', 'en', 3),
    )
    expect(full?.querySelector('img')?.alt).toBe('a map')
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })
})

/** `en` read and `ja` the next edition, whose one block is drawn with `marks`. */
const twoEditions = (marks: unknown[]) => ({
  title: 'Two',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [{ text: 'Hello.', heading: false, sentences: [[0, 6]] }],
    },
    ja: {
      label: 'Japanese',
      language: 'ja',
      blocks: [
        {
          text: '坊ちゃん big',
          heading: false,
          sentences: [[0, 8]],
          marks,
        },
      ],
    },
  },
  alignments: [
    {
      to: 'ja',
      links: [[0, 1, 0, 1]],
      sentences: [[0, 1, 0, 1]],
      pins: [],
    },
  ],
})

const japaneseMarks = [
  { kind: 'ruby', start: 0, end: 1, reading: 'ぼっ' },
  { kind: 'italic', start: 5, end: 8 },
]

describe('another edition’s marks beside the one read', () => {
  it('draws the ruby in the interlinear line under a sentence', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'interlinear' }),
    )
    const { host } = await opened(undefined, twoEditions(japaneseMarks))
    const line = host.querySelector('.book-interlinear') as HTMLElement
    expect(line.querySelector('ruby')?.outerHTML).toBe(
      '<ruby>坊<rt data-aside="true">ぼっ</rt></ruby>',
    )
    expect(line.querySelector('i')?.textContent).toBe('big')
  })

  it('draws the italic in the peek of a block', async () => {
    const { host } = await opened(undefined, twoEditions(japaneseMarks))
    scrolledPast(0)
    press('p')
    expect(host.querySelector('.book-peek i')?.textContent).toBe('big')
  })

  it('draws marks in a compare column', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, twoEditions(japaneseMarks))
    const cell = host.querySelectorAll('.book-row')[0].children[1]
    expect(cell.querySelector('i')?.textContent).toBe('big')
    expect(cell.querySelector('ruby')?.textContent).toContain('坊')
  })
})

describe('vertical Japanese', () => {
  /** The reader opened on `edition` of `twoEditions`, with `vertical` and `layout` as the prefs. */
  async function verticalOn(
    edition: string,
    vertical: boolean,
    layout: object = {},
  ) {
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ vertical }),
    )
    localStorage.setItem('taiyaku.prefs.book-layout', JSON.stringify(layout))
    // jsdom has no `scrollTo`, which placing a paged text calls.
    Element.prototype.scrollTo = vi.fn()
    const { host } = await opened(
      { edition, slot: 0 },
      twoEditions(japaneseMarks),
    )
    return host.querySelector<HTMLElement>('.book-text')?.dataset.vertical
  }

  it('sets a Japanese edition vertically with the setting on, scrolled or paged', async () => {
    expect(await verticalOn('ja', true)).toBe('true')
    expect(await verticalOn('ja', true, { paged: true })).toBe('true')
  })

  it('leaves the text horizontal for an edition not Japanese, or with the setting off', async () => {
    expect(await verticalOn('en', true)).toBeUndefined()
    expect(await verticalOn('ja', false)).toBeUndefined()
  })

  it('leaves the text horizontal compared or interlinear', async () => {
    expect(await verticalOn('ja', true, { mode: 'compare' })).toBeUndefined()
    expect(
      await verticalOn('ja', true, { mode: 'interlinear' }),
    ).toBeUndefined()
  })

  it('is off by default, and turned on in settings', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-text',
      JSON.stringify({ justify: true }),
    )
    const { host } = await opened(
      { edition: 'ja', slot: 0 },
      twoEditions(japaneseMarks),
    )
    const text = () => host.querySelector<HTMLElement>('.book-text')
    expect(text()?.dataset.vertical).toBeUndefined()
    press('s')
    const settings = host.querySelector('[aria-label="Settings"]')
    const row = (name: string) =>
      [...(settings?.querySelectorAll('label') ?? [])]
        .find((label) => label.textContent === name)
        ?.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(row('justify')?.checked).toBe(true)
    const box = row('vertical')
    expect(box?.checked).toBe(false)
    act(() => box?.click())
    expect(text()?.dataset.vertical).toBe('true')
  })

  /** The vertical reader, `layout` its layout, its text a screen 500 px wide that records `scrollBy`. */
  async function verticalText(layout: object = {}) {
    await verticalOn('ja', true, layout)
    const text = document.querySelector('.book-text') as HTMLElement
    vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(500)
    text.scrollBy = vi.fn()
    return text
  }

  it('reads ← as on and → as back, Page Down and Up as ever', async () => {
    const text = await verticalText()
    press('ArrowLeft')
    press('ArrowRight')
    press('PageDown')
    press('PageUp')
    expect(vi.mocked(text.scrollBy).mock.calls).toEqual([
      [{ left: -450 }],
      [{ left: 450 }],
      [{ left: -450 }],
      [{ left: 450 }],
    ])
  })

  it('scrolls on along the text, leftward, on the wheel turned down, and back turned up', async () => {
    const text = await verticalText()
    const wheel = (init: WheelEventInit) => {
      const event = new WheelEvent('wheel', { cancelable: true, ...init })
      act(() => {
        text.dispatchEvent(event)
      })
      return event
    }
    expect(wheel({ deltaY: 100 }).defaultPrevented).toBe(true)
    wheel({ deltaY: -40 })
    wheel({ deltaY: 1, deltaMode: WheelEvent.DOM_DELTA_PAGE })
    expect(vi.mocked(text.scrollBy).mock.calls).toEqual([
      [{ left: -100 }],
      [{ left: 40 }],
      [{ left: -500 }],
    ])
    // Sideways, and a pinch, are the browser's.
    expect(wheel({ deltaX: 100 }).defaultPrevented).toBe(false)
    expect(wheel({ deltaY: 100, ctrlKey: true }).defaultPrevented).toBe(false)
    expect(text.scrollBy).toHaveBeenCalledTimes(3)
  })

  it('sizes a vertical paged book’s text on the wheel, as a horizontal one’s', async () => {
    const text = await verticalText({ paged: true })
    const event = new WheelEvent('wheel', { deltaY: -100, cancelable: true })
    act(() => {
      text.dispatchEvent(event)
    })
    expect(event.defaultPrevented).toBe(true)
    expect(text.style.getPropertyValue('--book-text-size')).toBe('20px')
    expect(text.scrollBy).not.toHaveBeenCalled()
  })
})

describe('a sentence peek of several sentences', () => {
  it('is one span, the sentences joined by a space', async () => {
    const sent = twoEditions([])
    sent.editions.ja.blocks[0] = {
      text: 'One.Two.',
      heading: false,
      sentences: [
        [0, 4],
        [4, 8],
      ],
      marks: [],
    } as never
    sent.alignments[0].sentences = [[0, 1, 0, 2]]
    const { host } = await opened(undefined, sent)
    const first = host.querySelector('[data-block="0"] [data-sentence="0"]')
    touch(host, { on: first, at: 60_000 })
    touch(host, { on: first, at: 60_150 })
    const spans = host.querySelectorAll('.book-peek > span')
    expect([...spans].map((x) => x.textContent)).toEqual(['One. Two.'])
  })
})

/** `book`, with an image before its second block and one after its last in each edition. */
const figured = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      figures: [
        { before: 1, image: 7, alt: 'a map' },
        { before: 6, image: 8, alt: 'the end' },
      ],
    },
    ko: {
      ...book.editions.ko,
      figures: [{ before: 3, image: 2, alt: '지도' }],
    },
  },
}

/**
 * `book` with a cover before its first block, a page of its own, a title
 * graphic after the cover, as an image-only title page holding more than one
 * image sends it, and a map after the first block.
 */
const covered = {
  ...book,
  editions: {
    ...book.editions,
    en: {
      ...book.editions.en,
      figures: [
        { before: 0, image: 3, alt: 'the cover', page: true },
        { before: 0, image: 4, alt: 'the title' },
        { before: 1, image: 7, alt: 'a map' },
      ],
    },
  },
}

/**
 * `book` with a plate set before chapter two's heading, `page` saying
 * whether it is sent as a page of its own; one that is not can still take a
 * page, as a picture a page high does.
 */
const plated = (page: boolean) => ({
  ...book,
  editions: {
    ...book.editions,
    en: {
      ...book.editions.en,
      figures: [{ before: 3, image: 5, alt: 'the plate', page }],
    },
  },
})

/** `covered` with only its cover, set in a web link. */
const linkedCover = {
  ...covered,
  editions: {
    ...covered.editions,
    en: {
      ...covered.editions.en,
      figures: [
        {
          before: 0,
          image: 3,
          alt: 'the cover',
          page: true,
          link: { url: 'https://y.org/', note: false },
        },
      ],
    },
  },
}

/**
 * `image` drawn from a picture `natural` px across and down into `box` on
 * the screen, as a browser draws it: by default `paging`'s whole page.
 */
function drawnInto(
  image: Element,
  [w, h]: [number, number],
  box = { left: 0, top: 0, width: 600, height: 800 },
) {
  Object.defineProperties(image, {
    naturalWidth: { value: w, configurable: true },
    naturalHeight: { value: h, configurable: true },
    getBoundingClientRect: {
      configurable: true,
      value: () => ({
        ...box,
        right: box.left + box.width,
        bottom: box.top + box.height,
      }),
    },
  })
}

/** The click the browser sends on `on` after a tap: `taken` where the tap cancelled it. */
function clickAfter(on: Element): 'taken' | 'followed' {
  const click = new MouseEvent('click', {
    bubbles: true,
    cancelable: true,
    detail: 1,
  })
  act(() => {
    on.dispatchEvent(click)
  })
  return click.defaultPrevented ? 'taken' : 'followed'
}

/** The images of `within`, by their source. */
const sources = (within: Element) =>
  [...within.querySelectorAll('img')].map((img) => img.getAttribute('src'))

describe('a book’s images between blocks', () => {
  it('draws a figure before the block it names, and after the last block', async () => {
    const { host } = await opened(undefined, figured)
    const text = host.querySelector('.book-text') as HTMLElement
    const drawn = [...text.children].map((child) =>
      child.tagName === 'FIGURE'
        ? child.querySelector('img')?.getAttribute('src')
        : (child as HTMLElement).dataset.block,
    )
    expect(drawn).toEqual([
      '0',
      bookImageUrl('w', 'en', 7),
      '1',
      '2',
      '3',
      '4',
      '5',
      bookImageUrl('w', 'en', 8),
    ])
    const figure = text.querySelector('figure') as HTMLElement
    expect(figure.className).toBe('book-figure')
    expect(figure.dataset.block).toBeUndefined()
    expect(figure.dataset.line).toBeUndefined()
    expect(figure.querySelector('img')?.alt).toBe('a map')
  })

  it('takes the block after a figure at the top as the block at the top', async () => {
    const { host } = await opened(undefined, figured)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        const block = (this as HTMLElement).dataset?.block
        return { top: 0, bottom: block === '0' ? -10 : 100 } as DOMRect
      },
    )
    press('p')
    expect(peeked(host)?.[0]).toBe('1')
  })

  it('takes the last block as the block at the top where only the figures after it are left', async () => {
    const { host } = await opened(undefined, figured)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        const block = (this as HTMLElement).dataset?.block
        return { top: 0, bottom: block === undefined ? 100 : -10 } as DOMRect
      },
    )
    press('p')
    expect(peeked(host)?.[0]).toBe('5')
  })

  it('draws a figure, compared, only in its own edition’s column', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, figured)
    const column = (key: string) =>
      [...host.querySelectorAll(`[data-column="${key}"]`)].flatMap(sources)
    expect(column('en')).toEqual([
      bookImageUrl('w', 'en', 7),
      bookImageUrl('w', 'en', 8),
    ])
    expect(column('ko')).toEqual([bookImageUrl('w', 'ko', 2)])
    const rows = [...host.querySelectorAll('.book-row')]
    expect(rows.map((row) => sources(row).length)).toEqual([0, 1, 1, 1])
    const enOfRow = (row: number) =>
      rows[row].querySelector('[data-column="en"]')
    expect(enOfRow(1)?.firstElementChild?.tagName).toBe('FIGURE')
    expect(enOfRow(3)?.lastElementChild?.tagName).toBe('FIGURE')
    expect(
      rows[2].querySelector('[data-column="ko"]')?.firstElementChild?.tagName,
    ).toBe('FIGURE')
  })

  it('opens a figure tapped full size, the bars left as they were, until Escape', async () => {
    const { host } = await opened(undefined, figured)
    touch(host, { on: host.querySelector('.book-figure img') })
    const full = host.querySelector<HTMLDialogElement>(
      'dialog.book-figure-full',
    )
    expect(full?.open).toBe(true)
    expect(full?.querySelector('img')?.getAttribute('src')).toBe(
      bookImageUrl('w', 'en', 7),
    )
    expect(full?.querySelector('img')?.alt).toBe('a map')
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    press('Escape')
    expect(host.querySelector('dialog')).toBeNull()
  })

  it.each([
    ['unset', undefined, 'true'],
    ['on', true, 'true'],
    ['off', false, undefined],
  ])(
    'lets a figure opened full size grow past its own size as the comic reader’s upscale-to-fit setting says, %s',
    async (_, upscaleToFit, upscale) => {
      if (upscaleToFit !== undefined) {
        localStorage.setItem(
          'taiyaku.prefs.fit',
          JSON.stringify({ upscaleToFit }),
        )
      }
      const { host } = await opened(undefined, figured)
      touch(host, { on: host.querySelector('.book-figure img') })
      expect(
        host.querySelector<HTMLElement>('dialog.book-figure-full')?.dataset
          .upscale,
      ).toBe(upscale)
    },
  )

  it('marks a figure sent as a page of its own as a page image, and no other', async () => {
    // jsdom has no `scrollTo`, which opening the book at its cover calls.
    Element.prototype.scrollTo = vi.fn()
    const { host } = await opened(undefined, covered)
    const [cover, title, map] =
      host.querySelectorAll<HTMLElement>('.book-figure')
    expect(cover.querySelector('img')?.alt).toBe('the cover')
    expect(cover.dataset.pageImage).toBe('true')
    expect(title.dataset.pageImage).toBeUndefined()
    expect(map.dataset.pageImage).toBeUndefined()
  })

  it.each([
    ['unset', undefined, 'true'],
    ['on', true, 'true'],
    ['off', false, undefined],
  ])(
    'lets a page image grow past its own size as the comic reader’s upscale-to-fit setting says, %s',
    async (_, upscaleToFit, upscale) => {
      if (upscaleToFit !== undefined) {
        localStorage.setItem(
          'taiyaku.prefs.fit',
          JSON.stringify({ upscaleToFit }),
        )
      }
      Element.prototype.scrollTo = vi.fn()
      const { host } = await opened(undefined, covered)
      expect(
        host.querySelector<HTMLElement>('.book-text')?.dataset.upscale,
      ).toBe(upscale)
    },
  )

  it('turns a paged book’s page on a tap on a page image’s outer thirds, and shows or hides the bars from its middle, opening nothing', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, covered)
    const cover = host.querySelector('[data-page-image] img') as Element
    // Its picture the whole page, so every tap here is on it.
    drawnInto(cover, [600, 800])
    const bars = () => host.querySelector('.reader-top-bar') !== null
    const full = () => host.querySelector('dialog.book-figure-full')
    touch(host, { on: cover, x: 500 })
    expect([page(), bars(), full()]).toEqual([1, false, null])
    touch(host, { on: cover, x: 100 })
    expect([page(), bars(), full()]).toEqual([0, false, null])
    touch(host, { on: cover, x: 300 })
    expect([page(), bars(), full()]).toEqual([0, true, null])
    touch(host, { on: cover, x: 300 })
    expect([page(), bars(), full()]).toEqual([0, false, null])
  })

  it('takes the click of a tap that turns the page from a page image set in a web link, a mouse’s too, and leaves one on its middle to the link', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, linkedCover)
    const cover = host.querySelector('[data-page-image] img') as Element
    expect(cover.closest('a')?.target).toBe('_blank')
    drawnInto(cover, [600, 800])
    const clicked = () => clickAfter(cover)
    touch(host, { on: cover, x: 500, by: 'mouse' })
    expect(clicked()).toBe('taken')
    expect(page()).toBe(1)
    touch(host, { on: cover, x: 100, by: 'touch' })
    expect(clicked()).toBe('taken')
    expect(page()).toBe(0)
    touch(host, { on: cover, x: 300, by: 'mouse' })
    expect(clicked()).toBe('followed')
    expect(page()).toBe(0)
    expect(host.querySelector('dialog.book-figure-full')).toBeNull()
  })

  it('shows or hides the bars from a tap on a paged page image’s middle third, on its picture or beside it, and opens nothing', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, covered)
    const cover = host.querySelector('[data-page-image] img') as Element
    // A wide picture, drawn across the page from 300 px down to 500.
    drawnInto(cover, [600, 200])
    const bars = () => host.querySelector('.reader-top-bar') !== null
    const full = () => host.querySelector('dialog.book-figure-full')
    // Above it, on the figure, as where the image is drawn its own size.
    touch(host, { on: cover.closest('figure'), x: 300, y: 100 })
    expect([bars(), full(), page()]).toEqual([true, null, 0])
    touch(host, { on: cover, x: 300, y: 400 })
    expect([bars(), full(), page()]).toEqual([false, null, 0])
    // Below it, on the image, as where the image is drawn the page's size.
    touch(host, { on: cover, x: 300, y: 700 })
    expect([bars(), full(), page()]).toEqual([true, null, 0])
    touch(host, { on: cover, x: 300, y: 400 })
    expect([bars(), full(), page()]).toEqual([false, null, 0])
  })

  it('follows the link from a tap on the picture of a paged page image set in a web link, and shows or hides the bars from one beside a tall picture', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, linkedCover)
    const cover = host.querySelector('[data-page-image] img') as Element
    // A tall picture, drawn down the page from 250 px across to 350.
    drawnInto(cover, [100, 800])
    const bars = () => host.querySelector('.reader-top-bar') !== null
    touch(host, { on: cover, x: 210, y: 400, by: 'mouse' })
    expect([clickAfter(cover), bars(), page()]).toEqual(['taken', true, 0])
    touch(host, { on: cover, x: 390, y: 400, by: 'touch' })
    expect([clickAfter(cover), bars(), page()]).toEqual(['taken', false, 0])
    touch(host, { on: cover, x: 300, y: 400, by: 'mouse' })
    expect([clickAfter(cover), bars(), page()]).toEqual(['followed', false, 0])
    expect(host.querySelector('dialog.book-figure-full')).toBeNull()
  })

  it('opens a page image full size from a tap on it in a scrolled book, drawn or not, on its picture or beside it', async () => {
    Element.prototype.scrollTo = vi.fn()
    const { host } = await opened(undefined, covered)
    const cover = host.querySelector('[data-page-image] img') as Element
    const shown = () =>
      host.querySelector('dialog.book-figure-full img')?.getAttribute('alt')
    touch(host, { on: cover, x: 300 })
    expect(shown()).toBe('the cover')
    press('Escape')
    expect(host.querySelector('dialog')).toBeNull()
    // A wide picture, drawn across the page from 300 px down to 500, tapped
    // above it, where paged the tap would be the page's, and then on it.
    drawnInto(cover, [600, 200])
    touch(host, { on: cover, x: 300, y: 100 })
    expect(shown()).toBe('the cover')
    press('Escape')
    touch(host, { on: cover, x: 300, y: 400 })
    expect(shown()).toBe('the cover')
  })

  it.each([
    ['', covered],
    [', set in a web link', linkedCover],
  ])(
    'shows or hides the bars from a tap on the middle of a paged page image not drawn yet%s',
    async (_, sent) => {
      paged()
      paging([0, 0, 1, 1, 2, 2])
      const { host } = await opened(undefined, sent)
      const cover = host.querySelector('[data-page-image] img') as Element
      touch(host, { on: cover, x: 300, by: 'mouse' })
      expect([
        clickAfter(cover),
        host.querySelector('.reader-top-bar') !== null,
        host.querySelector('dialog.book-figure-full'),
      ]).toEqual(['taken', true, null])
    },
  )

  it('takes the click of a tap beside the picture of a paged page image set in a web link, a mouse’s too, and shows or hides the bars, and leaves one on the picture to the link', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, linkedCover)
    const cover = host.querySelector('[data-page-image] img') as Element
    const link = cover.closest('a') as Element
    drawnInto(cover, [600, 200])
    const bars = () => host.querySelector('.reader-top-bar') !== null
    touch(host, { on: cover, x: 300, y: 100, by: 'mouse' })
    expect([clickAfter(cover), bars(), page()]).toEqual(['taken', true, 0])
    touch(host, { on: link, x: 300, y: 700, by: 'touch' })
    expect([clickAfter(link), bars(), page()]).toEqual(['taken', false, 0])
    touch(host, { on: cover, x: 300, y: 400, by: 'mouse' })
    expect([clickAfter(cover), bars(), page()]).toEqual(['followed', false, 0])
  })

  it('opens a page image full size from a tap on its outer thirds when compared, where it is drawn as any figure', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare', paged: true }),
    )
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened(undefined, covered)
    expect(
      host.querySelector('.book-text')?.hasAttribute('data-compared'),
    ).toBe(true)
    touch(host, { on: host.querySelector('[data-page-image] img'), x: 500 })
    expect(page()).toBe(0)
    expect(
      host.querySelector('dialog.book-figure-full img')?.getAttribute('alt'),
    ).toBe('the cover')
  })

  it('opens a paged book afresh on the page of the cover set before its first block, not on the title’s after it', async () => {
    paged()
    const page = paging([2, 2, 3, 3, 4, 4], 0, [0, 1, 2])
    await opened(undefined, covered)
    expect(page()).toBe(0)
  })

  it('reads page 1 of the first chapter on the cover a paged book opens on afresh, its first block a heading, not page 0', async () => {
    paged()
    // The cover alone on page 0; the title and chapter one from page 1 to
    // page 2, where chapter two starts.
    const page = paging([1, 1, 2, 2, 3, 3], 0, [0, 1, 1])
    const { host } = await opened(undefined, covered)
    expect(page()).toBe(0)
    touch(host, { x: 300 })
    expect(readout(host)).toBe(
      'page 1 of 4 · <1m read · CHAPTER ONE · page 1 of 3',
    )
  })

  it('opens a scrolled book afresh at the top of the cover set before its first block, not at the title after it or the block', async () => {
    Element.prototype.scrollTo = vi.fn()
    // The title drawn below the cover; anything else measured as jsdom does.
    const measured = Element.prototype.getBoundingClientRect
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        const alt = this.querySelector(':scope > img')?.getAttribute('alt')
        return alt === 'the title'
          ? ({ top: 300, bottom: 400 } as DOMRect)
          : measured.call(this)
      },
    )
    await opened(undefined, covered)
    expect(scrolledTo()).toEqual([])
    expect(Element.prototype.scrollTo).toHaveBeenCalledWith({ top: 0 })
  })

  it.each([
    ['a page image', true],
    ['a figure not sent as a page', false],
  ])(
    'goes from the contents to the page of a chapter’s heading, not to %s set before it',
    async (_, page) => {
      paged()
      // Chapter one on pages 0 and 1, the plate alone on page 2, and chapter
      // two's heading on page 3. The plate's page is the chapter's first
      // (`pagesOf`), so the heading's reads page 2 of the chapter.
      const shown = paging([0, 0, 1, 3, 3, 4], 0, [2])
      const { host } = await opened(undefined, plated(page))
      press('t')
      const entries = host.querySelectorAll<HTMLElement>(
        '[aria-label="Contents"] li button',
      )
      expect(entries[1]?.textContent).toBe('CHAPTER TWO · The End')
      act(() => entries[1]?.click())
      expect(shown()).toBe(3)
      press('t')
      press('Tab')
      expect(readout(host)).toBe(
        'page 4 of 5 · <1m read · The End · page 2 of 3',
      )
    },
  )

  it('leaves the bars as they were on a figure tapped just after a tap on the text', async () => {
    const { host } = await opened(undefined, figured)
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    const at = later()
    touch(host, { at }, false)
    touch(host, { on: host.querySelector('.book-figure img'), at: at + 100 })
    act(() => vi.advanceTimersByTime(1000))
    vi.useRealTimers()
    expect(host.querySelector('dialog.book-figure-full')).not.toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('closes the full-size figure on a tap beside it, not on the image', async () => {
    const { host } = await opened(undefined, figured)
    touch(host, { on: host.querySelector('.book-figure img') })
    const full = () => host.querySelector('dialog.book-figure-full')
    const image = full()?.querySelector('img') ?? null
    // Drawn, its picture filling its box, so every tap on it is on the picture.
    drawnInto(image as Element, [600, 800])
    touch(host, { on: image })
    act(() => image?.click())
    expect(full()).not.toBeNull()
    touch(host, { on: full() })
    act(() => (full() as HTMLElement).click())
    expect(full()).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('keeps the full-size figure open when the tap that opened it clicks on it, as a touch screen’s does', async () => {
    const { host } = await opened(undefined, figured)
    touch(host, { on: host.querySelector('.book-figure img') })
    const full = () => host.querySelector('dialog.book-figure-full')
    act(() => (full() as HTMLElement).click())
    expect(full()).not.toBeNull()
  })

  it('goes back to the reader’s place, paged, when an image before it loads, and not for one after', async () => {
    paged()
    paging([0, 1, 2, 3, 4, 5])
    const { host } = await opened({ edition: 'en', slot: 4 }, figured)
    const scrolls = vi.mocked(Element.prototype.scrollTo)
    scrolls.mockClear()
    const [before, after] = host.querySelectorAll('.book-figure img')
    act(() => {
      after.dispatchEvent(new Event('load'))
    })
    expect(scrolls).not.toHaveBeenCalled()
    act(() => {
      before.dispatchEvent(new Event('load'))
    })
    expect(scrolls.mock.calls).toEqual([[{ left: 4 * 600 }]])
  })

  it('leaves a scrolled reader to the browser when an image loads', async () => {
    const { host } = await opened({ edition: 'en', slot: 4 }, figured)
    vi.mocked(Element.prototype.scrollIntoView).mockClear()
    act(() => {
      host.querySelector('.book-figure img')?.dispatchEvent(new Event('load'))
    })
    expect(scrolledTo()).toEqual([])
  })
})

/**
 * `book`, its blocks linked: block 0 to a web page, block 1 to a footnote in
 * block 5, block 2 straight on to block 3, and the Korean block 1 to a
 * footnote in its own block 3.
 */
const linked = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      blocks: book.editions.en.blocks.map((block, i) => {
        const marks = [
          [
            {
              kind: 'link',
              start: 8,
              end: 11,
              url: 'https://x.org/',
              note: false,
            },
          ],
          [{ kind: 'link', start: 1, end: 4, block: 5, note: true }],
          [{ kind: 'link', start: 0, end: 6, block: 3, note: false }],
          [],
          [],
          [{ kind: 'italic', start: 0, end: 4 }],
        ][i]
        return marks.length > 0 ? { ...block, marks } : block
      }),
    },
    ko: {
      ...book.editions.ko,
      blocks: book.editions.ko.blocks.map((block, i) =>
        i === 1
          ? {
              ...block,
              marks: [{ kind: 'link', start: 1, end: 3, block: 3, note: true }],
            }
          : block,
      ),
    },
  },
}

/** `book` with English's block 1 linking to its block 5. */
const linkingOn = {
  ...book,
  editions: {
    ...book.editions,
    en: {
      ...book.editions.en,
      blocks: book.editions.en.blocks.map((block, i) =>
        i === 1
          ? {
              ...block,
              marks: [
                { kind: 'link', start: 1, end: 5, block: 5, note: false },
              ],
            }
          : block,
      ),
    },
  },
}

/** The link drawn in block `block` of the text read. */
const linkIn = (host: HTMLElement, block: number) =>
  host.querySelector<HTMLElement>(`.book-text [data-block="${block}"] a`)

const footnote = (host: HTMLElement) =>
  host.querySelector<HTMLDialogElement>('dialog.book-footnote')

const backButton = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>('.reader-bottom-bar button')].find(
    (b) => b.textContent === 'Back',
  )

describe('a book’s links', () => {
  it('opens a footnote reference’s note in a popup on a tap, its marks drawn, the bars left as they were', async () => {
    const { host } = await opened(undefined, linked)
    touch(host, { on: linkIn(host, 1) })
    const popup = footnote(host)
    expect(popup?.open).toBe(true)
    expect(popup?.querySelector('.book-footnote-text')?.textContent).toBe(
      'Done.',
    )
    expect(popup?.querySelector('i')?.textContent).toBe('Done')
    expect(popup?.closest('.book-text')).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('opens the note once on a double tap on its reference, and peeks nothing', async () => {
    const { host } = await opened(undefined, linked)
    const on = linkIn(host, 1)
    const at = later()
    touch(host, { on, at })
    touch(host, { on, at: at + 150 })
    expect(footnote(host)?.open).toBe(true)
    expect(host.querySelectorAll('dialog.book-footnote')).toHaveLength(1)
    expect(host.querySelector('.book-peek')).toBeNull()
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('closes the popup on Escape, and nothing else with it', async () => {
    const { host } = await opened(undefined, linked)
    tap(host)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    touch(host, { on: linkIn(host, 1) })
    press('Escape')
    expect(footnote(host)).toBeNull()
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
  })

  it('closes the popup on a tap beside it', async () => {
    const { host } = await opened(undefined, linked)
    touch(host, { on: linkIn(host, 1) })
    expect(footnote(host)?.open).toBe(true)
    touch(host, { on: footnote(host) })
    act(() => footnote(host)?.click())
    expect(footnote(host)).toBeNull()
  })

  it('keeps the popup open when the tap that opened it clicks on it, as a touch screen’s does', async () => {
    const { host } = await opened(undefined, linked)
    touch(host, { on: linkIn(host, 1) })
    act(() => footnote(host)?.click())
    expect(footnote(host)?.open).toBe(true)
  })

  it('goes to the note from the popup, and Back returns to where the reader was, then goes', async () => {
    const { host } = await opened(undefined, linked)
    scrolledPast(1)
    touch(host, { on: linkIn(host, 1) })
    const go = [...(footnote(host)?.querySelectorAll('button') ?? [])].find(
      (b) => b.textContent === 'Go to note',
    )
    act(() => go?.click())
    expect(footnote(host)).toBeNull()
    expect(scrolledTo().at(-1)).toBe('5')
    expect(backButton(host)).toBeDefined()
    act(() => backButton(host)?.click())
    expect(scrolledTo().at(-1)).toBe('1')
    expect(backButton(host)).toBeUndefined()
  })

  it('jumps straight on a tap on any other in-book link, with the same way back', async () => {
    const { host } = await opened(undefined, linked)
    scrolledPast(2)
    touch(host, { on: linkIn(host, 2) })
    expect(footnote(host)).toBeNull()
    expect(scrolledTo().at(-1)).toBe('3')
    act(() => backButton(host)?.click())
    expect(scrolledTo().at(-1)).toBe('2')
    expect(backButton(host)).toBeUndefined()
  })

  it('returns, compared and paged, to the page the link was followed from, not the first its row is on', async () => {
    paged('compare')
    // English's block 1, in a row that runs from page 0 to page 2, links to
    // block 5 on the last page.
    const page = paging([0, [0, 2], 2, 2])
    const { host } = await opened(undefined, linkingOn)
    press('ArrowRight')
    expect(page()).toBe(1)
    touch(host, { on: host.querySelector('[data-pick="left:1"] a') })
    expect(page()).toBe(2)
    act(() => backButton(host)?.click())
    expect(page()).toBe(1)
  })

  it('returns to the block a link was followed from, compared and paged, once paging is left before Back', async () => {
    paged('compare')
    const page = paging([0, [0, 2], 2, 2])
    const { host } = await opened(undefined, linkingOn)
    press('ArrowRight')
    expect(page()).toBe(1)
    touch(host, { on: host.querySelector('[data-pick="left:1"] a') })
    expect(page()).toBe(2)
    act(() => layoutButton(host, 'Page turning').click())
    vi.mocked(Element.prototype.scrollIntoView).mockClear()
    act(() => backButton(host)?.click())
    // Placed by the row holding block 1, not scrolled a third of the way into
    // it, which is how far into the row the page the link was on began.
    expect(scrolledTo()).toEqual(['1'])
  })

  it('returns to the block a link was followed from, scrolled, though compare is chosen before Back', async () => {
    const { host } = await opened(undefined, linked)
    // The screen starts a quarter into the first line of block 0.
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      () => ({ top: 0, bottom: 100, left: 0, right: 600 }) as DOMRect,
    )
    Object.defineProperty(Range.prototype, 'getClientRects', {
      configurable: true,
      value: () => [{ top: -5, bottom: 15, left: 0, right: 10 }],
    })
    onTestFinished(() => {
      Reflect.deleteProperty(Range.prototype, 'getClientRects')
    })
    host.querySelector<HTMLElement>('.book-text')!.scrollTo = vi.fn()
    touch(host, { on: linkIn(host, 2) })
    expect(scrolledTo().at(-1)).toBe('3')
    act(() => layoutButton(host, 'Compare editions').click())
    act(() => backButton(host)?.click())
    // Placed by the row holding block 0, not by a fraction of a line's
    // height taken into a row's.
    expect(scrolledTo().at(-1)).toBe('0')
  })

  it('keeps the bars and Back up on a link tapped just after a tap on the text', async () => {
    const { host } = await opened(undefined, linked)
    scrolledPast(2)
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    const at = later()
    touch(host, { at }, false)
    touch(host, { on: linkIn(host, 2), at: at + 100 })
    act(() => vi.advanceTimersByTime(1000))
    vi.useRealTimers()
    expect(scrolledTo().at(-1)).toBe('3')
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    expect(backButton(host)).toBeDefined()
  })

  it('closes an open panel on a jump, so nothing covers Back', async () => {
    const { host } = await opened(undefined, linked)
    press('t')
    expect(host.querySelector('[aria-label="Contents"]')).not.toBeNull()
    touch(host, { on: linkIn(host, 2) })
    expect(scrolledTo().at(-1)).toBe('3')
    expect(host.querySelector('[aria-label="Contents"]')).toBeNull()
    expect(backButton(host)).toBeDefined()
  })

  it('leaves a web link to the browser: no popup, no jump, the bars as they were', async () => {
    const { host } = await opened(undefined, linked)
    const link = host.querySelector<HTMLAnchorElement>(
      '.book-text [data-block="0"] a',
    )
    expect(link?.getAttribute('href')).toBe('https://x.org/')
    expect(link?.target).toBe('_blank')
    expect(link?.rel).toBe('noopener noreferrer')
    const before = scrolledTo().length
    const click = new MouseEvent('click', { bubbles: true, cancelable: true })
    touch(host, { on: link })
    act(() => {
      link?.dispatchEvent(click)
    })
    expect(click.defaultPrevented).toBe(false)
    expect(footnote(host)).toBeNull()
    expect(scrolledTo()).toHaveLength(before)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
  })

  it('selects a link’s words for a highlight on a press past the slop, or held', async () => {
    const { host } = await opened(undefined, linked)
    const bar = () =>
      host.querySelector('[role="toolbar"][aria-label="Selection"]')
    select(host, 1, 1, 4)
    touch(host, { on: linkIn(host, 1), travel: 40 })
    expect(bar()).not.toBeNull()
    expect(footnote(host)).toBeNull()
    select(host, 1, 1, 4)
    touch(host, { on: linkIn(host, 1), held: 600 })
    expect(bar()).not.toBeNull()
    expect(footnote(host)).toBeNull()
  })

  it('opens a link’s note, compared, from its own column’s edition', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const { host } = await opened(undefined, linked)
    touch(host, { on: host.querySelector('[data-column="ko"] a') })
    expect(
      footnote(host)?.querySelector('.book-footnote-text')?.textContent,
    ).toBe('아무도.')
    expect(
      footnote(host)
        ?.querySelector('.book-footnote-card')
        ?.getAttribute('lang'),
    ).toBe('ko')
  })

  it('makes an in-book link one a keyboard reaches and follows on Enter, with no address of its own', async () => {
    const { host } = await opened(undefined, linked)
    const link = linkIn(host, 1) as HTMLElement
    expect(link.getAttribute('href')).toBeNull()
    expect(link.getAttribute('role')).toBe('link')
    expect(link.tabIndex).toBe(0)
    act(() => {
      link.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    expect(footnote(host)?.open).toBe(true)
  })

  it('keeps a note’s card on the screen when the window turns from tall to short under it', async () => {
    vi.stubGlobal('innerHeight', 800)
    const { host } = await opened(undefined, linked)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        return (
          this.matches('a[data-link]')
            ? { left: 40, right: 60, top: 600, bottom: 620 }
            : { left: 0, right: 0, top: 0, bottom: 100 }
        ) as DOMRect
      },
    )
    touch(host, { on: linkIn(host, 1) })
    const card = () =>
      footnote(host)?.querySelector<HTMLElement>('.book-footnote-card')
    expect(card()?.style.bottom).toBe('204px')
    vi.stubGlobal('innerHeight', 400)
    act(() => {
      window.dispatchEvent(new Event('resize'))
    })
    const style = card()?.style as CSSStyleDeclaration
    expect(style.maxHeight).toBe('180px')
    const extent = style.top
      ? [Number.parseFloat(style.top), Number.parseFloat(style.top) + 180]
      : style.bottom
        ? [
            400 - Number.parseFloat(style.bottom) - 180,
            400 - Number.parseFloat(style.bottom),
          ]
        : [110, 290]
    expect(extent[0]).toBeGreaterThanOrEqual(0)
    expect(extent[1]).toBeLessThanOrEqual(400)
    vi.unstubAllGlobals()
  })
})

/**
 * `book`, its images set in links: block 0's to a web page, block 1's calling
 * the footnote in block 5, block 2's straight on to block 3; a figure before
 * block 4 going to block 3 too, and one after the last to a web page.
 */
const imagesLinked = {
  ...book,
  editions: {
    ...book.editions,
    en: {
      ...book.editions.en,
      blocks: book.editions.en.blocks.map((block, i) => {
        const link = [
          { url: 'https://x.org/', note: false },
          { block: 5, note: true },
          { block: 3, note: false },
        ][i]
        return link
          ? {
              ...block,
              marks: [
                { kind: 'image', start: 1, end: 1, image: 3, alt: 'go', link },
              ],
            }
          : block
      }),
      figures: [
        { before: 4, image: 7, alt: 'on', link: { block: 3, note: false } },
        {
          before: 6,
          image: 8,
          alt: 'out',
          link: { url: 'https://y.org/', note: false },
        },
      ],
    },
  },
}

/** The image drawn in block `block` of the text read. */
const imageIn = (host: HTMLElement, block: number) =>
  host.querySelector<HTMLElement>(`.book-text [data-block="${block}"] img`)

const fullFigure = (host: HTMLElement) =>
  host.querySelector('dialog.book-figure-full')

describe('a book’s images set in links', () => {
  it('jumps on a tap on an inline image set in an in-book link, with Back, and opens no figure', async () => {
    const { host } = await opened(undefined, imagesLinked)
    scrolledPast(2)
    touch(host, { on: imageIn(host, 2) })
    expect(fullFigure(host)).toBeNull()
    expect(scrolledTo().at(-1)).toBe('3')
    act(() => backButton(host)?.click())
    expect(scrolledTo().at(-1)).toBe('2')
  })

  it('opens the note an inline image calls as its reference', async () => {
    const { host } = await opened(undefined, imagesLinked)
    touch(host, { on: imageIn(host, 1) })
    expect(fullFigure(host)).toBeNull()
    expect(footnote(host)?.open).toBe(true)
    expect(
      footnote(host)?.querySelector('.book-footnote-text')?.textContent,
    ).toBe('Done.')
  })

  it('leaves an inline image linked to the web to the browser, in a new tab', async () => {
    const { host } = await opened(undefined, imagesLinked)
    const link = imageIn(host, 0)?.closest('a')
    expect(link?.getAttribute('href')).toBe('https://x.org/')
    expect(link?.target).toBe('_blank')
    const before = scrolledTo().length
    touch(host, { on: imageIn(host, 0) })
    expect(fullFigure(host)).toBeNull()
    expect(footnote(host)).toBeNull()
    expect(scrolledTo()).toHaveLength(before)
  })

  it('opens the note a figure calls as its reference mid-screen when the figure is taller than the screen', async () => {
    const { host } = await opened(undefined, {
      ...imagesLinked,
      editions: {
        ...imagesLinked.editions,
        en: {
          ...imagesLinked.editions.en,
          figures: [
            { before: 4, image: 7, alt: 'on', link: { block: 5, note: true } },
          ],
        },
      },
    })
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: Element) {
        return (
          this.closest('figure')
            ? { left: 0, right: 300, top: -100, bottom: innerHeight + 100 }
            : { left: 0, right: 0, top: 0, bottom: 0 }
        ) as DOMRect
      },
    )
    touch(host, { on: host.querySelector('.book-figure img') })
    expect(fullFigure(host)).toBeNull()
    expect(footnote(host)?.open).toBe(true)
    const card = footnote(host)?.querySelector<HTMLElement>(
      '.book-footnote-card',
    )
    expect(card?.textContent).toContain('Done.')
    expect(card?.dataset.placed).toBeUndefined()
    expect([card?.style.top, card?.style.bottom]).toEqual(['', ''])
  })

  it('names a link holding only an image with no alt text for where it goes', async () => {
    const sent = marked('abc', [
      {
        kind: 'image',
        start: 0,
        end: 0,
        image: 1,
        alt: '',
        link: { block: 0, note: true },
      },
      {
        kind: 'image',
        start: 1,
        end: 1,
        image: 2,
        alt: '',
        link: { url: 'https://www.x.org/about/us', note: false },
      },
      {
        kind: 'image',
        start: 2,
        end: 2,
        image: 3,
        alt: '',
        link: { block: 0, note: false },
      },
      {
        kind: 'image',
        start: 3,
        end: 3,
        image: 4,
        alt: 'a map',
        link: { block: 0, note: false },
      },
      {
        kind: 'image',
        start: 3,
        end: 3,
        image: 6,
        alt: '',
        link: { url: 'mailto:a@x.org', note: false },
      },
    ])
    const { host } = await opened(undefined, {
      ...sent,
      editions: {
        en: {
          ...sent.editions.en,
          figures: [
            { before: 1, image: 5, alt: '', link: { block: 0, note: false } },
          ],
        },
      },
    })
    const names = [...host.querySelectorAll('.book-text a')].map((a) => [
      a.querySelector('img')?.getAttribute('src'),
      a.getAttribute('aria-label'),
    ])
    expect(names).toEqual([
      [bookImageUrl('w', 'en', 1), 'Note'],
      [bookImageUrl('w', 'en', 2), 'x.org'],
      [bookImageUrl('w', 'en', 3), 'Linked passage'],
      [bookImageUrl('w', 'en', 4), null],
      [bookImageUrl('w', 'en', 6), 'mailto:a@x.org'],
      [bookImageUrl('w', 'en', 5), 'Linked passage'],
    ])
  })

  it('follows a figure set in a link, and leaves one linked to the web to the browser', async () => {
    const { host } = await opened(undefined, imagesLinked)
    scrolledPast(1)
    const [onward, out] = host.querySelectorAll('.book-figure img')
    expect(onward.getAttribute('alt')).toBe('on')
    expect(out.closest('a')?.getAttribute('href')).toBe('https://y.org/')
    expect(out.closest('a')?.target).toBe('_blank')
    const before = scrolledTo().length
    touch(host, { on: out })
    expect(fullFigure(host)).toBeNull()
    expect(scrolledTo()).toHaveLength(before)
    touch(host, { on: onward })
    expect(fullFigure(host)).toBeNull()
    expect(scrolledTo().at(-1)).toBe('3')
    expect(backButton(host)).toBeDefined()
  })
})

describe('a search match lit in a block', () => {
  it('draws .book-found around exactly the match, leaving selection offsets alone', () => {
    const block = {
      text: '坊ちゃんの話',
      sentences: [[0, 6]] as [number, number][],
      marks: [{ kind: 'italic', start: 1, end: 3 }],
    }
    const draw = (found?: [number, number]) =>
      mount(
        <div data-block="0">
          <span data-text>
            <Sentences
              block={block as never}
              first={0}
              highlights={[]}
              work="w"
              edition="en"
              furigana
              found={found}
            />
          </span>
        </div>,
      )
    const host = draw([1, 4])
    const lit = [...host.querySelectorAll('.book-found')]
    expect(lit.map((el) => el.textContent).join('')).toBe('ちゃん')
    expect(host.querySelector('[data-text]')?.textContent).toBe('坊ちゃんの話')
    // Characters `from` to `to` selected on whichever text nodes the
    // drawing split the words into, as `selectionIn` counts them.
    const selected = (on: HTMLElement, from: number, to: number) => {
      const nodes: Text[] = []
      const walker = document.createTreeWalker(
        on.querySelector('[data-text]') as Node,
        NodeFilter.SHOW_TEXT,
      )
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        nodes.push(node as Text)
      }
      const point = (at: number, end: boolean): [Text, number] => {
        let left = at
        for (const node of nodes) {
          if (end ? left <= node.length : left < node.length)
            return [node, left]
          left -= node.length
        }
        throw new Error(`no character ${at}`)
      }
      const range = document.createRange()
      range.setStart(...point(from, false))
      range.setEnd(...point(to, true))
      window.getSelection()?.removeAllRanges()
      window.getSelection()?.addRange(range)
      const got = selectionIn(on)?.range
      window.getSelection()?.removeAllRanges()
      return got
    }
    const plain = draw()
    // Inside the match, the match whole, from inside it on past it, from
    // before it into it, across it, after it, and before it.
    for (const [from, to] of [
      [2, 3],
      [1, 4],
      [2, 5],
      [0, 2],
      [0, 6],
      [4, 6],
      [0, 1],
    ]) {
      const want = { start: from, end: to }
      expect(selected(host, from, to), `lit, ${from} to ${to}`).toEqual(want)
      expect(selected(plain, from, to), `plain, ${from} to ${to}`).toEqual(want)
    }
  })
})

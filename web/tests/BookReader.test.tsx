import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { bookImageUrl } from '../src/book'
import { selectionIn } from '../src/bookNotes'
import { BookReader } from '../src/BookReader'
import { GateContext, type Gated } from '../src/gate'
import type { Note } from '../src/notes'
import { land } from './support/async'
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
 * `sideways` across, `held` milliseconds long and starting at `at`.
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
  }: {
    on?: Element | null
    travel?: number
    sideways?: number
    x?: number
    held?: number
    at?: number
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
    })
    Object.defineProperty(event, 'timeStamp', { value: time })
    return event
  }
  const faked = vi.isFakeTimers()
  if (!faked) vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  act(() => {
    target?.dispatchEvent(pointer('pointerdown', x, 100, at))
    target?.dispatchEvent(
      pointer('pointerup', x + sideways, 100 + travel, at + held),
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
 * `pages[i]`, or running from one page to another, `[first, last]`; turned by
 * `scrollTo`, which scrolls as a browser would. When not paged, the blocks
 * before `above` are scrolled past. Answers the page shown.
 */
function paging(pages: (number | [number, number])[], above = 0) {
  let left = 0
  const isText = (el: Element) => el.classList.contains('book-text')
  const span = (block: number) => {
    const on = pages[block] ?? 0
    return typeof on === 'number' ? [on, on] : on
  }
  /** One line's box on page `page`, as the page shown places it. */
  const line = (page: number, bottom: number) => {
    const at = page * 600 - left + 24
    return { left: at, right: at + 552, top: 0, bottom } as DOMRect
  }
  const last = Math.max(...pages.map((_, i) => span(i)[1]))
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
    const [first, end] = span(Number((this as HTMLElement).dataset.block))
    return [line(first, 100), line(end, 100)] as unknown as DOMRectList
  })
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: Element) {
      if (isText(this)) {
        return {
          left: 0,
          right: 600,
          width: 600,
          top: 0,
          bottom: 800,
        } as DOMRect
      }
      const block = Number((this as HTMLElement).dataset.block)
      const [first, end] = span(block)
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

/** Select block `block`'s own words from `start` up to `end`, across the spans its sentences are drawn in. */
function select(host: HTMLElement, block: number, start: number, end: number) {
  const words = host.querySelector(
    `[data-block="${block}"] [data-text]`,
  ) as Node
  const at = (offset: number): [Node, number] => {
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
  const range = document.createRange()
  range.setStart(...at(start))
  range.setEnd(...at(end))
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
      ['scroll', 'true'],
      ['compare', 'false'],
      ['interlinear', 'false'],
      ['paged', 'false'],
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

  it('shows the bars on Tab, and hides them on Escape', async () => {
    const { host } = await opened()
    press('Tab')
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    press('Escape')
    expect(host.querySelector('.reader-top-bar')).toBeNull()
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
    const layout = (name: string) =>
      [...host.querySelectorAll('[aria-label="Layout"] button')].find(
        (b) => b.textContent === name,
      ) as HTMLElement
    act(() => layout('compare').click())
    expect(host.querySelectorAll('.book-row')).toHaveLength(4)
    expect(scrolledTo().at(-1)).toBe('2')
    expect(localStorage.getItem('taiyaku.prefs.book-layout')).toContain(
      'compare',
    )
    act(() => layout('scroll').click())
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
    ).toEqual(['paged'])
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

  it('peeks on a double tap on a paged book’s outer third, turning nothing', async () => {
    paged()
    const page = paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const on = host.querySelector('.book-text [data-block="1"]')
    touch(host, { on, x: 500, at: 95_000 }, false)
    touch(host, { on, x: 500, at: 95_150 })
    expect(page()).toBe(0)
    expect(peeked(host)?.[0]).toBe('1')
    // A slower one: the page has turned when the second tap lands, and the
    // bars, which the first tap never touched, stay down.
    touch(host, { on, x: 500, at: 96_000 })
    expect(page()).toBe(1)
    touch(host, { on, x: 500, at: 96_250 })
    expect(host.querySelector('.reader-top-bar')).toBeNull()
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
        (b) => b.textContent === 'paged',
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

  it('sets a paged book’s text larger on the wheel up and smaller down, a step a notch, adding up a smooth wheel', async () => {
    paged()
    paging([0, 0, 1, 1, 2, 2])
    const { host } = await opened()
    const text = host.querySelector('.book-text') as HTMLElement
    const size = () => text.style.getPropertyValue('--book-text-size')
    const wheel = (deltaY: number) => {
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
    expect(wheel(-100).defaultPrevented).toBe(true)
    expect(size()).toBe('20px')
    wheel(-60)
    expect(size()).toBe('20px')
    wheel(-40)
    expect(size()).toBe('22px')
    wheel(200)
    expect(size()).toBe('18px')
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
    fetch.mockImplementation(async (url: string) =>
      url === '/api/works/w/notes'
        ? new Response('down', { status: 500 })
        : url === '/api/works/w/book'
          ? Response.json(book)
          : new Response(null, { status: 204 }),
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
    key('ArrowRight')
    expect(scrolledTo().at(-1)).toBe('0')
    key('Home')
    key('PageDown')
    expect(scrolledTo().at(-1)).toBe('1')
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
    const layout = (name: string) =>
      [
        ...host.querySelectorAll<HTMLElement>('[aria-label="Layout"] button'),
      ].find((b) => b.textContent === name)
    act(() => layout('compare')?.click())
    press('c')
    expect(host.querySelector('.book-pinning')).not.toBeNull()
    act(() => layout('scroll')?.click())
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

  it('lets go of a lit pair of sentences when compare is left', async () => {
    const { host } = await opened(undefined, sentenced)
    press('Tab')
    const layout = (name: string) =>
      [
        ...host.querySelectorAll<HTMLElement>('[aria-label="Layout"] button'),
      ].find((b) => b.textContent === name)
    act(() => layout('compare')?.click())
    const second = host.querySelector(
      '[data-pick="left:1"] [data-sentence="2"]',
    )
    touch(host, { on: second, at: 80_000 })
    touch(host, { on: second, at: 80_150 })
    expect(host.querySelectorAll('[data-sentence][data-lit]')).toHaveLength(2)
    act(() => layout('scroll')?.click())
    act(() => layout('compare')?.click())
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

import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { show } from '../src/bookPages'
import { context } from '../src/bookSearch'
import { BookReader } from '../src/BookReader'
import { land } from './support/async'
import './support/dialog'
import { typeInto } from './support/input'
import { mount } from './support/mount'
import { fakeStorage } from './support/storage'

// Wrapped rather than replaced, so a pick places the text as it would, and
// the test can still say where it was asked to go.
vi.mock('../src/bookPages', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/bookPages')>()
  return { ...real, show: vi.fn(real.show) }
})

vi.mock('../src/bookSearch', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/bookSearch')>()
  return { ...real, context: vi.fn(real.context) }
})

vi.mock('../src/Account', () => ({
  Account: () => null,
}))

const book = {
  title: 'Harry',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
        {
          text: 'Harry woke. Harry rose.',
          heading: false,
          sentences: [
            [0, 11],
            [12, 23],
          ],
        },
        { text: 'Nobody called.', heading: false, sentences: [[0, 14]] },
        { text: 'CHAPTER TWO', heading: true, sentences: [[0, 11]] },
        { text: 'Then harry left.', heading: false, sentences: [[0, 16]] },
      ],
    },
    fr: {
      label: 'French',
      language: 'fr',
      blocks: [
        { text: 'CHAPITRE UN', heading: true, sentences: [[0, 11]] },
        { text: 'Harry dormait.', heading: false, sentences: [[0, 14]] },
        { text: 'CHAPITRE DEUX', heading: true, sentences: [[0, 13]] },
        { text: 'Harry partit.', heading: false, sentences: [[0, 13]] },
      ],
    },
  },
  alignments: [
    {
      to: 'fr',
      pins: [],
      links: [
        [0, 1, 0, 1],
        [1, 3, 1, 2],
        [3, 4, 2, 3],
        [4, 5, 3, 4],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 4, 1, 2],
        [4, 5, 2, 3],
        [5, 6, 3, 4],
      ],
    },
  ],
}

/** One edition of `n` blocks, each `harry` once. */
const many = (n: number) => ({
  title: 'Many',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: n }, (_, i) => ({
        text: `harry ${i}`,
        heading: false,
        sentences: [[0, 7]],
      })),
    },
  },
  alignments: [],
})

/** One edition of ten blocks, `harry` in the even ones and `ron` in the odd. */
const alternating = {
  title: 'Alternating',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 10 }, (_, i) => {
        const text = `${i % 2 ? 'ron' : 'harry'} ${i}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
    },
  },
  alignments: [],
}

function server(sent: unknown) {
  return vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/api/works/w/book') return Response.json(sent)
    if (url === '/api/works/w/notes') {
      return Response.json(
        { work: 'w', notes: [] },
        { headers: { etag: '"n0"' } },
      )
    }
    if (init?.method === 'PUT') return new Response(null, { status: 204 })
    return new Response(null, { status: 204 })
  })
}

async function opened(sent: unknown = book) {
  vi.stubGlobal('fetch', server(sent))
  Element.prototype.scrollIntoView = vi.fn()
  const host = mount(<BookReader work="w" onLeave={() => {}} />)
  await land()
  return host
}

function press(key: string, init: KeyboardEventInit = {}, on?: Element) {
  const event = new KeyboardEvent('keydown', {
    key,
    cancelable: true,
    bubbles: true,
    ...init,
  })
  act(() => {
    ;(on ?? window).dispatchEvent(event)
  })
  return event
}

const panel = (host: HTMLElement) =>
  host.querySelector<HTMLElement>('[role="dialog"][aria-label="Search"]')
const box = (host: HTMLElement) =>
  panel(host)?.querySelector<HTMLInputElement>('input[type="search"]') ?? null
const rows = (host: HTMLElement) => [
  ...(panel(host)?.querySelectorAll<HTMLButtonElement>(
    '.book-contents button',
  ) ?? []),
]
const count = (host: HTMLElement) =>
  panel(host)?.querySelector('.book-search-count')?.textContent

/** Type `query` into the box and wait out the debounce. */
async function search(host: HTMLElement, query: string) {
  await typeInto(box(host) as HTMLInputElement, query)
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 150))
  })
}

/** Where the search light is: each `.book-found`'s block and text. */
const lit = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>('.book-text .book-found')].map(
    (el) => [
      el.closest<HTMLElement>('[data-block], [data-pick]')?.dataset.block ??
        el.closest<HTMLElement>('[data-pick]')?.dataset.pick,
      el.textContent,
    ],
  )

/** Where the text was last placed: its block and character. */
const placed = () => vi.mocked(show).mock.calls.at(-1)?.slice(1, 3)

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

/** The notes documents written, each as sent. */
function notesWritten() {
  return vi
    .mocked(fetch)
    .mock.calls.filter(
      ([url, init]) => url === '/api/works/w/notes' && init?.method === 'PUT',
    )
    .map(([, init]) => JSON.parse(String(init?.body)))
}

function tap(host: HTMLElement) {
  const target = host.querySelector('.book-text') as Element
  const pointer = (type: string, time: number) => {
    const event = new PointerEvent(type, {
      clientX: 300,
      clientY: 100,
      bubbles: true,
    })
    Object.defineProperty(event, 'timeStamp', { value: time })
    return event
  }
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  act(() => {
    target.dispatchEvent(pointer('pointerdown', 50_000))
    target.dispatchEvent(pointer('pointerup', 50_000))
  })
  act(() => vi.advanceTimersByTime(200))
  vi.useRealTimers()
}

beforeEach(() => {
  vi.stubGlobal('localStorage', fakeStorage())
  vi.mocked(show).mockClear()
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('searching a book', () => {
  it('opens on / with focus in the box, and lists matches under their chapters', async () => {
    const host = await opened()
    const slash = press('/')
    expect(slash.defaultPrevented).toBe(true)
    expect(panel(host)).not.toBeNull()
    expect(document.activeElement).toBe(box(host))
    await search(host, 'harry')
    const chapters = [
      ...(panel(host)?.querySelectorAll('.book-search-chapter') ?? []),
    ].map((h) => h.textContent)
    expect(chapters).toEqual(['CHAPTER ONE', 'CHAPTER TWO'])
    expect(
      rows(host).map((r) => r.querySelector('strong')?.textContent),
    ).toEqual(['Harry', 'Harry', 'harry'])
    expect(rows(host)[1].textContent).toBe('Harry woke. Harry rose.')
    expect(rows(host)[2].firstChild?.textContent).toBe('Then ')
    expect(count(host)).toBe('3 matches')
  })

  it('counts past 500 as 500+, drawing only 500', async () => {
    const host = await opened(many(600))
    press('/')
    await search(host, 'harry')
    expect(count(host)).toBe('500+ matches')
    expect(rows(host)).toHaveLength(500)
  })

  it('steps past the 500th listed to every match on Enter and Shift+Enter', async () => {
    const host = await opened(many(600))
    press('/')
    await search(host, 'harry')
    expect(rows(host)).toHaveLength(500)
    press('Enter', { shiftKey: true }, box(host) as Element)
    expect(placed()).toEqual([599, 0])
    act(() => rows(host)[499].click())
    expect(placed()).toEqual([499, 0])
    press('/')
    press('Enter', {}, box(host) as Element)
    expect(placed()).toEqual([500, 0])
  })

  it('steps past the 500th listed to every match on ] and [', async () => {
    const host = await opened(many(600))
    press('/')
    await search(host, 'harry')
    act(() => rows(host)[499].click())
    press(']')
    expect(placed()).toEqual([500, 0])
    press('/')
    act(() => rows(host)[0].click())
    press('[')
    expect(placed()).toEqual([599, 0])
  })

  it('keeps the rows it listed while the reader scrolls under the panel', async () => {
    const host = await opened(many(600))
    press('/')
    await search(host, 'harry')
    expect(rows(host)).toHaveLength(500)
    vi.mocked(context).mockClear()
    act(() => {
      host.querySelector('.book-text')?.dispatchEvent(new Event('scroll'))
    })
    expect(vi.mocked(context)).not.toHaveBeenCalled()
  })

  it('says one match as one', async () => {
    const host = await opened()
    press('/')
    await search(host, 'nobody')
    expect(count(host)).toBe('1 match')
  })

  it('lists nothing and counts nothing for a query too short to search', async () => {
    const host = await opened()
    press('/')
    await search(host, 'h')
    expect(rows(host)).toHaveLength(0)
    expect(count(host)).toBeUndefined()
  })

  it('goes to a match pressed, lit, and closes', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    act(() => rows(host)[1].click())
    expect(placed()).toEqual([1, 12])
    expect(lit(host)).toEqual([['1', 'Harry']])
    expect(panel(host)).toBeNull()
  })

  it('steps on Enter and back on Shift+Enter in the box, open, then on ] and [', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    press('Enter', {}, box(host) as Element)
    expect(placed()).toEqual([1, 0])
    press('Enter', {}, box(host) as Element)
    expect(placed()).toEqual([1, 12])
    expect(lit(host)).toEqual([['1', 'Harry']])
    press('Enter', { shiftKey: true }, box(host) as Element)
    expect(placed()).toEqual([1, 0])
    expect(panel(host)).not.toBeNull()
    press('Escape', {}, box(host) as Element)
    expect(panel(host)).toBeNull()
    press(']')
    expect(placed()).toEqual([1, 12])
    press(']')
    expect(placed()).toEqual([4, 5])
    expect(lit(host)).toEqual([['4', 'harry']])
    press('[')
    expect(placed()).toEqual([1, 12])
  })

  it('steps on ] and [ typed with AltGr, but not on Ctrl or Alt alone', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    press('Escape', {}, box(host) as Element)
    const altGr = { ctrlKey: true, altKey: true, modifierAltGraph: true }
    press(']', altGr)
    expect(placed()).toEqual([1, 0])
    press(']', altGr)
    expect(placed()).toEqual([1, 12])
    press('[', { altKey: true, modifierAltGraph: true })
    expect(placed()).toEqual([1, 0])
    const before = vi.mocked(show).mock.calls.length
    press(']', { ctrlKey: true })
    press(']', { altKey: true })
    expect(vi.mocked(show).mock.calls.length).toBe(before)
  })

  it('keeps / ] and [ for the box while typing in it', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    const before = vi.mocked(show).mock.calls.length
    const typed = press(']', {}, box(host) as Element)
    press('[', {}, box(host) as Element)
    press('/', {}, box(host) as Element)
    expect(typed.defaultPrevented).toBe(false)
    expect(vi.mocked(show).mock.calls.length).toBe(before)
    expect(panel(host)).not.toBeNull()
  })

  it('searches every edition with the switch, grouped by edition, and swaps to one picked', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    const all = panel(host)?.querySelector<HTMLInputElement>(
      'input[type="checkbox"]',
    )
    act(() => all?.click())
    const groups = [
      ...(panel(host)?.querySelectorAll('.book-search-edition') ?? []),
    ].map((h) => h.textContent)
    expect(groups).toEqual(['English', 'French'])
    const chapters = [
      ...(panel(host)?.querySelectorAll('.book-search-chapter') ?? []),
    ].map((h) => h.textContent)
    expect(chapters).toEqual([
      'CHAPTER ONE',
      'CHAPTER TWO',
      'CHAPITRE UN',
      'CHAPITRE DEUX',
    ])
    expect(count(host)).toBe('5 matches')
    act(() => rows(host)[4].click())
    expect(host.querySelector<HTMLElement>('.book-text')?.lang).toBe('fr')
    expect(placed()).toEqual([3, 0])
    expect(lit(host)).toEqual([['3', 'Harry']])
  })

  it('keeps the light on Escape, and puts it out on the next tap', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    press('Enter', {}, box(host) as Element)
    act(() => panel(host)?.focus())
    press('Escape')
    expect(panel(host)).toBeNull()
    expect(lit(host)).toEqual([['1', 'Harry']])
    tap(host)
    expect(lit(host)).toEqual([])
  })

  it('steps on after a tap from the match the tap put the light out on', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    press('Enter', {}, box(host) as Element)
    press('Escape', {}, box(host) as Element)
    tap(host)
    expect(lit(host)).toEqual([])
    press(']')
    expect(placed()).toEqual([1, 12])
    expect(lit(host)).toEqual([['1', 'Harry']])
  })

  it('steps from the reader’s place on Enter and Shift+Enter once the query changes', async () => {
    const host = await opened(alternating)
    press('/')
    await search(host, 'ron')
    act(() => rows(host)[2].click())
    expect(placed()).toEqual([5, 0])
    press('/')
    await search(host, 'harry')
    press('Enter', {}, box(host) as Element)
    expect(placed()).toEqual([6, 0])
    await search(host, 'harr')
    press('Enter', { shiftKey: true }, box(host) as Element)
    expect(placed()).toEqual([4, 0])
  })

  it('steps from the reader’s place on [ and ] once the edition changes', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    act(() => rows(host)[2].click())
    expect(placed()).toEqual([4, 5])
    scrolledPast(4)
    press(' ')
    expect(host.querySelector<HTMLElement>('.book-text')?.lang).toBe('fr')
    expect(placed()).toEqual([3, undefined])
    press('[')
    expect(placed()).toEqual([1, 0])
    press(']')
    expect(placed()).toEqual([3, 0])
    scrolledPast(3)
    press(' ')
    expect(host.querySelector<HTMLElement>('.book-text')?.lang).toBe('en')
    press(']')
    expect(placed()).toEqual([4, 5])
  })

  it('searches the first column compared, and lights the match there', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const host = await opened()
    press('/')
    await search(host, 'harry')
    expect(count(host)).toBe('3 matches')
    act(() => rows(host)[2].click())
    const found = host.querySelector('.book-found')
    expect(found?.closest('[data-column]')?.getAttribute('data-column')).toBe(
      'en',
    )
    expect(found?.textContent).toBe('harry')
  })

  it('searches the main line, interlinear', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'interlinear' }),
    )
    const host = await opened()
    press('/')
    await search(host, 'harry')
    expect(count(host)).toBe('3 matches')
    act(() => rows(host)[0].click())
    expect(
      host.querySelector('.book-found')?.closest('.book-interlinear'),
    ).toBeNull()
  })

  it('opens from the bars, and lists its keys in the help', async () => {
    const host = await opened()
    press('Tab')
    const button = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Search',
    )
    act(() => button?.click())
    expect(panel(host)).not.toBeNull()
    expect(button?.getAttribute('aria-expanded')).toBe('true')
    act(() => panel(host)?.focus())
    press('?')
    const help = host.querySelector('[aria-label="Keyboard help"]')
    expect(help?.textContent).toContain('search the book')
    expect(help?.textContent).toContain('the next match')
  })

  it('still writes a new note on n while a search stands, and saves no light with it', async () => {
    const host = await opened()
    press('/')
    await search(host, 'harry')
    press('Enter', {}, box(host) as Element)
    press('Escape', {}, box(host) as Element)
    expect(lit(host)).toEqual([['1', 'Harry']])
    press('n')
    expect(
      host.querySelector('[role="dialog"][aria-label="Note"]'),
    ).not.toBeNull()
    expect(placed()).toEqual([1, 0])
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 750))
    })
    await land()
    const notes = notesWritten().at(-1)?.notes
    expect(notes).toHaveLength(1)
    expect(notes[0]).toMatchObject({ edition: 'en', body: '' })
    expect(notes[0]).not.toHaveProperty('text')
  })
})

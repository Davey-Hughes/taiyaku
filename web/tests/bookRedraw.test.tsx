import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { firstSentences } from '../src/book'
import * as bookNotes from '../src/bookNotes'
import { BookReader } from '../src/BookReader'
import { land } from './support/async'
import './support/dialog'
import { mount } from './support/mount'
import { fakeStorage } from './support/storage'

// How many of a book's blocks the reader draws again for one thing a reader
// does. An edition runs to thousands of blocks, and drawing them all again is
// what a size step, a page turn or a sentence read aloud must not cost, so
// each test counts the blocks whose words are cut into runs (`pieces`, once a
// block) rather than timing anything.
vi.mock('../src/bookNotes', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/bookNotes')>()
  return { ...real, pieces: vi.fn(real.pieces) }
})
// Compared, each row draws a few blocks of every edition, and numbering a
// whole edition's sentences for each row would cost the rows times the
// blocks, so the numbering (`firstSentences`) is counted too.
vi.mock('../src/book', async (importOriginal) => {
  const real = await importOriginal<typeof import('../src/book')>()
  return { ...real, firstSentences: vi.fn(real.firstSentences) }
})
vi.mock('../src/Account', () => ({ Account: () => null }))

const BLOCKS = 30

/** A book of `BLOCKS` blocks, each two sentences but every tenth a heading. */
const book = {
  title: 'Long',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: BLOCKS }, (_, i) =>
        i % 10 === 0
          ? {
              text: `CHAPTER ${i / 10 + 1}`,
              heading: true,
              sentences: [[0, 9]],
            }
          : {
              text: 'Who asked. Nobody knew.',
              heading: false,
              sentences: [
                [0, 10],
                [11, 23],
              ],
            },
      ),
    },
  },
  alignments: [],
}

/**
 * `book` with a German edition set under it sentence for sentence, the
 * translation of block 15 alone starting "Offen".
 */
const translated = {
  ...book,
  editions: {
    ...book.editions,
    de: {
      label: 'German',
      language: 'de',
      blocks: book.editions.en.blocks.map((block, i) =>
        block.heading
          ? block
          : {
              ...block,
              text:
                i === 15
                  ? 'Offen ist. Offen, hier.'
                  : 'Wer fragt. Keiner sagt.',
            },
      ),
    },
  },
  alignments: [
    {
      to: 'de',
      pins: [],
      links: Array.from({ length: BLOCKS }, (_, i) => [i, i + 1, i, i + 1]),
      sentences: book.editions.en.blocks
        .flatMap((block) => block.sentences)
        .map((_, s) => [s, s + 1, s, s + 1]),
    },
  ],
}

function server(sent: unknown = book) {
  return vi.fn(async (url: string, init?: RequestInit) => {
    if (url === '/api/works/w/book') return Response.json(sent)
    if (url === '/api/library/works/w') return Response.json({ series: null })
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

function press(key: string) {
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key, cancelable: true }),
    )
  })
}

/** How many blocks have had their words drawn since the last count. */
function drawn(): number {
  const pieces = vi.mocked(bookNotes.pieces)
  const count = pieces.mock.calls.length
  pieces.mockClear()
  return count
}

/**
 * A double tap on `on` at `at` ms, its taps 150 ms apart, and as long after
 * it as a tap waits to act.
 */
function doubleTap(on: Element, at: number) {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  for (const time of [at, at + 150]) {
    const pointer = (type: string) => {
      const event = new PointerEvent(type, {
        clientX: 100,
        clientY: 100,
        bubbles: true,
      })
      Object.defineProperty(event, 'timeStamp', { value: time })
      return event
    }
    act(() => {
      on.dispatchEvent(pointer('pointerdown'))
      on.dispatchEvent(pointer('pointerup'))
    })
    act(() => vi.advanceTimersByTime(time === at ? 150 : 1000))
  }
  vi.useRealTimers()
}

/** A tap on `on`, and as long after it as a tap waits to act. */
function tap(on: Element) {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  const pointer = (type: string) =>
    new PointerEvent(type, { clientX: 100, clientY: 100, bubbles: true })
  act(() => {
    on.dispatchEvent(pointer('pointerdown'))
    on.dispatchEvent(pointer('pointerup'))
  })
  act(() => vi.advanceTimersByTime(1000))
  vi.useRealTimers()
}

function voices() {
  const spoken: SpeechSynthesisUtterance[] = []
  vi.stubGlobal('speechSynthesis', {
    getVoices: () => [],
    speak: (u: SpeechSynthesisUtterance) => spoken.push(u),
    cancel: vi.fn(),
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
  return {
    starts: () =>
      act(() =>
        spoken.at(-1)?.onstart?.(new Event('start') as SpeechSynthesisEvent),
      ),
    ends: () =>
      act(() =>
        spoken.at(-1)?.onend?.(new Event('end') as SpeechSynthesisEvent),
      ),
  }
}

beforeEach(() => {
  vi.stubGlobal('localStorage', fakeStorage())
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('the blocks the reader draws again', () => {
  it('draws every block when the book opens', async () => {
    // THE CONTROL: the count sees the blocks the reader draws.
    await opened()
    expect(drawn()).toBeGreaterThanOrEqual(BLOCKS)
  })

  it('draws none again for a text size step', async () => {
    const host = await opened()
    const size = () =>
      host
        .querySelector<HTMLElement>('.book-text')
        ?.style.getPropertyValue('--book-text-size')
    expect(size()).toBe('18px')
    drawn()
    press('=')
    expect(size()).toBe('20px')
    expect(drawn()).toBe(0)
  })

  it('draws none again for a text size step, compared', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const host = await opened(translated)
    const size = () =>
      host
        .querySelector<HTMLElement>('.book-text')
        ?.style.getPropertyValue('--book-text-size')
    expect(
      host.querySelectorAll('.book-text[data-compared] .book-row'),
    ).toHaveLength(BLOCKS)
    // Both columns' blocks, the count's control.
    expect(drawn()).toBeGreaterThanOrEqual(2 * BLOCKS)
    press('=')
    expect(size()).toBe('20px')
    expect(drawn()).toBe(0)
  })

  it('numbers a compared edition’s sentences once each time it draws the rows, not once a row', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'compare' }),
    )
    const numbered = vi.mocked(firstSentences)
    numbered.mockClear()
    drawn()
    const host = await opened(translated)
    expect(
      host.querySelectorAll('.book-text[data-compared] .book-row'),
    ).toHaveLength(BLOCKS)
    // The German edition, which is compared but not read, so only the rows
    // number its sentences; its block 15 alone starts "Offen", drawn once
    // each time the rows are.
    const german = numbered.mock.calls.filter(([blocks]) =>
      blocks.some((block) => block.text.startsWith('Offen')),
    )
    const drawings = vi
      .mocked(bookNotes.pieces)
      .mock.calls.filter(([text]) => text.startsWith('Offen'))
    expect(drawings.length).toBeGreaterThan(0)
    expect(german).toHaveLength(drawings.length)
  })

  it('draws none again when a panel opens beside the text', async () => {
    const host = await opened()
    press('Tab')
    drawn()
    const contents = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Contents',
    )
    act(() => contents?.click())
    expect(contents?.getAttribute('aria-expanded')).toBe('true')
    expect(drawn()).toBe(0)
  })

  it('draws again only the block holding the sentence read aloud', async () => {
    const speech = voices()
    const host = await opened()
    press('r')
    speech.starts()
    speech.ends()
    drawn()
    speech.starts()
    expect(host.querySelectorAll('[data-sentence][data-lit]')).toHaveLength(1)
    expect(drawn()).toBeLessThanOrEqual(2)
  })

  it('draws again only the blocks a peek leaves and goes to', async () => {
    const host = await opened(translated)
    const sentence = (block: number) =>
      host.querySelector(`[data-block="${block}"] [data-sentence]`) as Element
    const peekedIn = () =>
      host
        .querySelector('.book-peek')
        ?.closest('[data-block]')
        ?.getAttribute('data-block')
    doubleTap(sentence(3), 60_000)
    expect(peekedIn()).toBe('3')
    drawn()
    doubleTap(sentence(5), 120_000)
    expect(peekedIn()).toBe('5')
    // The blocks of the edition read, not the peek's own words, which are
    // the German edition's.
    const english = new Set(book.editions.en.blocks.map((block) => block.text))
    const blocks = vi
      .mocked(bookNotes.pieces)
      .mock.calls.filter(([text]) => english.has(text))
    expect(blocks.length).toBeGreaterThan(0)
    expect(blocks.length).toBeLessThanOrEqual(2)
  })

  it('leaves a block whose hidden translation is tapped open undrawn as reading aloud moves on elsewhere', async () => {
    localStorage.setItem(
      'taiyaku.prefs.book-layout',
      JSON.stringify({ mode: 'interlinear', hideTranslation: true }),
    )
    const speech = voices()
    const host = await opened(translated)
    // Block 15's first sentence, after two headings of one and thirteen
    // blocks of two.
    const line = () =>
      host.querySelector('[data-interlinear="28"]') as HTMLElement
    expect(line().textContent).toBe('Offen ist.')
    tap(line())
    expect(line().hasAttribute('data-hidden')).toBe(false)
    press('r')
    speech.starts()
    speech.ends()
    drawn()
    speech.starts()
    expect(host.querySelectorAll('[data-sentence][data-lit]')).toHaveLength(1)
    const pieces = vi.mocked(bookNotes.pieces).mock.calls
    expect(pieces.length).toBeGreaterThan(0)
    expect(pieces.filter(([text]) => text.startsWith('Offen'))).toEqual([])
  })
})

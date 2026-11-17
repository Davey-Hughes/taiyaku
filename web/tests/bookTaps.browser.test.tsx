import { act } from 'react'
import { afterAll, describe, expect, it, onTestFinished, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
import { BookReader } from '../src/BookReader'
import { topOf } from '../src/bookPages'
import '../src/index.css'
import { CLICK_WAIT_MS } from '../src/tapClick'
import { mount } from './support/mount'
import { mouse } from './support/mouse'
import { tapAt } from './support/touch'

vi.mock('../src/Account', () => ({ Account: () => null }))

// Images the browser draws at a size of their own: a small one, so the
// full-size figure leaves room beside it, and `PLATE`, as wide as the column
// and taller than the screen.
vi.mock('../src/book', async (original) => ({
  ...(await original<typeof import('../src/book')>()),
  bookImageUrl: (_work: string, _edition: string, image: number) =>
    image === PLATE ? svg(2000, 20000) : svg(120, 80),
}))

const PLATE = 9

function svg(width: number, height: number): string {
  return `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="${width}" height="${height}"/></svg>`,
  )}`
}

/**
 * What a finger's tap on the book's text does, which jsdom cannot answer:
 * Chromium sends a touch's `click` after its `pointerup`, to whatever is
 * drawn there by then.
 */

const WORDS = Array.from(
  { length: 40 },
  (_, i) => `word${i} the quick brown fox.`,
).join(' ')

/** Six blocks, a footnote reference in the first to the last, and a figure before the second. */
const book = {
  title: 'Linked',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 6 }, (_, i) => {
        const text = i === 5 ? 'Done.' : `Block ${i} opens. ${WORDS}`
        return {
          text,
          heading: false,
          sentences: [[0, text.length]],
          ...(i === 0
            ? {
                marks: [
                  { kind: 'link', start: 6, end: 7, block: 5, note: true },
                ],
              }
            : {}),
        }
      }),
      figures: [{ before: 1, image: 0, alt: 'a map' }],
    },
  },
  alignments: [],
}

/** Twelve blocks, and before the second a plate set in a link to the seventh. */
const plated = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      blocks: Array.from({ length: 12 }, (_, i) => {
        const text = `Block ${i} opens. ${WORDS}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
      figures: [
        {
          before: 1,
          image: PLATE,
          alt: 'a plate',
          link: { block: 6, note: false },
        },
      ],
    },
  },
}

/** The book reader over the whole screen, as the app's route draws it, its images loaded. */
async function reader(
  drawn: object = book,
  onLeave = () => {},
): Promise<HTMLElement> {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(drawn)
      if (url === '/api/works/w/notes') {
        return Response.json(
          { work: 'w', notes: [] },
          { headers: { etag: '"n"' } },
        )
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [] })
    }),
  )
  const host = mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={onLeave} />
    </div>,
  )
  const image = () =>
    host.querySelector<HTMLImageElement>('.book-figure img') ?? undefined
  for (let i = 0; i < 100 && !image()?.complete; i++) {
    await act(() => new Promise((done) => setTimeout(done, 10)))
  }
  return host
}

/** Long enough for a tap's `click` to arrive and what it sets to be drawn. */
async function settled() {
  for (let i = 0; i < 5; i++) {
    await act(() => new Promise((done) => setTimeout(done, 20)))
  }
}

/**
 * Every element a `click` is sent to from here on in the test, `aimed`, and
 * every one it reaches, `reached`: the first heard on the window before
 * anything the reader adds there, the second on the document after it.
 */
function clicks(): { aimed: EventTarget[]; reached: EventTarget[] } {
  const aimed: EventTarget[] = []
  const reached: EventTarget[] = []
  const onAimed = (event: Event) => {
    if (event.target) aimed.push(event.target)
  }
  const onReached = (event: Event) => {
    if (event.target) reached.push(event.target)
  }
  window.addEventListener('click', onAimed, true)
  document.addEventListener('click', onReached, true)
  onTestFinished(() => {
    window.removeEventListener('click', onAimed, true)
    document.removeEventListener('click', onReached, true)
  })
  return { aimed, reached }
}

const middleOf = (el: Element) => {
  const box = el.getBoundingClientRect()
  return { x: box.left + box.width / 2, y: box.top + box.height / 2 }
}

const backIn = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>('.reader-bottom-bar button')].find(
    (b) => b.textContent === 'Back',
  )

/**
 * Where Back and the top bar's Library button are drawn: the bars and Back
 * brought up by a jump from the keyboard, measured, and taken down again,
 * the reader back at its first block with the bars hidden.
 */
async function barsMeasured(host: HTMLElement) {
  const link = host.querySelector('.book-figure a') as HTMLElement
  act(() => {
    link.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
    )
  })
  await settled()
  const back = middleOf(backIn(host) as Element)
  const library = middleOf(host.querySelector('.reader-back') as Element)
  act(() => backIn(host)?.click())
  act(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
  })
  await settled()
  expect(host.querySelector('.reader-bottom-bar')).toBeNull()
  return { back, library }
}

/** The next touch's end prevented, so Chromium sends that tap no `click`. */
function noClickFromNextTap() {
  const prevent = (event: Event) => event.preventDefault()
  window.addEventListener('touchend', prevent, {
    capture: true,
    once: true,
    passive: false,
  })
  onTestFinished(() => window.removeEventListener('touchend', prevent, true))
}

describe('a finger’s tap on the book’s text', () => {
  afterAll(() => vi.unstubAllGlobals())

  it('keeps open the note its tap on a reference opened, and a tap beside the note closes it', async () => {
    const host = await reader()
    const note = () =>
      host.querySelector<HTMLDialogElement>('dialog.book-footnote')
    const { aimed, reached } = clicks()
    const reference = middleOf(
      host.querySelector('[data-block="0"] a') as Element,
    )
    await tapAt(reference.x, reference.y)
    await settled()
    expect(note()?.open).toBe(true)
    // THE STATE THIS TEST NAMES: the tap's click was sent to the note's dialog.
    expect(aimed).toEqual([note()])
    expect(reached).toEqual([])

    const beside = { x: 20, y: window.innerHeight - 20 }
    expect(document.elementFromPoint(beside.x, beside.y)).toBe(note())
    await tapAt(beside.x, beside.y)
    await settled()
    expect(note()).toBeNull()
  })

  it('keeps open the image its tap on a figure opened, and a tap beside the image closes it', async () => {
    const host = await reader()
    const full = () =>
      host.querySelector<HTMLDialogElement>('dialog.book-figure-full')
    const { aimed, reached } = clicks()
    const figure = middleOf(host.querySelector('.book-figure img') as Element)
    await tapAt(figure.x, figure.y)
    await settled()
    expect(full()?.open).toBe(true)
    // THE STATE THIS TEST NAMES: the tap's click was sent to the dialog
    // itself, beside the image drawn full size.
    expect(aimed).toEqual([full()])
    expect(reached).toEqual([])

    const beside = { x: 20, y: 20 }
    expect(document.elementFromPoint(beside.x, beside.y)).toBe(full())
    await tapAt(beside.x, beside.y)
    await settled()
    expect(full()).toBeNull()
  })

  it('keeps the jump its tap on a linked image made, though Back is drawn under the finger', async () => {
    const host = await reader(plated)
    const text = host.querySelector('.book-text') as HTMLElement
    const { back } = await barsMeasured(host)
    expect(
      document.elementFromPoint(back.x, back.y)?.closest('.book-figure a'),
    ).not.toBeNull()
    const { aimed, reached } = clicks()
    await tapAt(back.x, back.y)
    await settled()
    expect(backIn(host)).toBeDefined()
    expect(topOf(text).block).toBe(6)
    // THE STATE THIS TEST NAMES: the tap's click was sent to Back.
    expect(aimed).toEqual([backIn(host)])
    expect(reached).toEqual([])
  })

  it('keeps the reader open when its tap on a linked image draws the top bar’s Library button under the finger', async () => {
    const onLeave = vi.fn()
    const host = await reader(plated, onLeave)
    const text = host.querySelector('.book-text') as HTMLElement
    const { library } = await barsMeasured(host)
    const plate = host.querySelector('.book-figure img') as HTMLElement
    await act(async () => {
      text.scrollBy({ top: plate.getBoundingClientRect().top + 100 })
    })
    await settled()
    expect(
      document
        .elementFromPoint(library.x, library.y)
        ?.closest('.book-figure a'),
    ).not.toBeNull()
    const { aimed } = clicks()
    await tapAt(library.x, library.y)
    await settled()
    expect(onLeave).not.toHaveBeenCalled()
    expect(backIn(host)).toBeDefined()
    // THE STATE THIS TEST NAMES: the tap's click was sent to Library.
    expect(aimed).toEqual([host.querySelector('.reader-back')])
  })

  it('sends the click of a double tap’s second tap, which can redraw the bars, nowhere', async () => {
    const host = await reader()
    const words = host.querySelector('[data-block="1"] [data-sentence]')
    // Its first line, on the screen where the sentence runs on below it.
    const line = (words as Element).getClientRects()[0]
    const at = { x: line.left + line.width / 2, y: line.top + line.height / 2 }
    const { aimed, reached } = clicks()
    await tapAt(at.x, at.y)
    await tapAt(at.x, at.y)
    await settled()
    await settled()
    await settled()
    // Taken for a double tap: the first tap's wait to bring the bars up
    // stopped.
    expect(host.querySelector('.reader-bottom-bar')).toBeNull()
    // THE STATE THIS TEST NAMES: a click sent for each tap.
    expect(aimed).toHaveLength(2)
    expect(reached).toEqual([aimed[0]])
  })

  it('leaves a click from the keyboard to Back while a tap’s click is still awaited', async () => {
    const host = await reader(plated)
    const text = host.querySelector('.book-text') as HTMLElement
    const { back } = await barsMeasured(host)
    const { aimed } = clicks()
    noClickFromNextTap()
    await tapAt(back.x, back.y)
    await settled()
    // THE STATE THIS TEST NAMES: the jump made, and no click sent for it.
    expect(aimed).toEqual([])
    expect(topOf(text).block).toBe(6)
    backIn(host)?.focus()
    await userEvent.keyboard('{Enter}')
    await settled()
    expect(backIn(host)).toBeUndefined()
    expect(topOf(text).block).toBe(0)
  })

  it('leaves the click of the next press to Back while a tap’s click is still awaited', async () => {
    const host = await reader(plated)
    const text = host.querySelector('.book-text') as HTMLElement
    const { back } = await barsMeasured(host)
    const { aimed } = clicks()
    noClickFromNextTap()
    await tapAt(back.x, back.y)
    await settled()
    // THE STATE THIS TEST NAMES: the jump made, and no click sent for it.
    expect(aimed).toEqual([])
    expect(topOf(text).block).toBe(6)
    await act(async () => {
      await mouse.moveTo(back.x, back.y)
      await mouse.press()
      await mouse.release()
    })
    await settled()
    expect(backIn(host)).toBeUndefined()
    expect(topOf(text).block).toBe(0)
  })

  it('stops waiting for a tap’s click that never came', async () => {
    const host = await reader(plated)
    const text = host.querySelector('.book-text') as HTMLElement
    const { back } = await barsMeasured(host)
    noClickFromNextTap()
    await tapAt(back.x, back.y)
    await settled()
    expect(topOf(text).block).toBe(6)
    await act(
      () => new Promise((done) => setTimeout(done, CLICK_WAIT_MS + 100)),
    )
    // A click as a pointer's would carry, with no press before it that
    // would end the wait sooner.
    act(() => {
      backIn(host)?.dispatchEvent(
        new MouseEvent('click', { bubbles: true, detail: 1 }),
      )
    })
    await settled()
    expect(backIn(host)).toBeUndefined()
    expect(topOf(text).block).toBe(0)
  })

  it('leaves a mouse’s click on a link to the page', async () => {
    const host = await reader(plated)
    const text = host.querySelector('.book-text') as HTMLElement
    const box = (
      host.querySelector('.book-figure img') as Element
    ).getBoundingClientRect()
    const plate = {
      x: box.left + box.width / 2,
      y: (box.top + window.innerHeight) / 2,
    }
    const { reached } = clicks()
    await act(async () => {
      await mouse.moveTo(plate.x, plate.y)
      await mouse.press()
      await mouse.release()
    })
    await settled()
    expect(topOf(text).block).toBe(6)
    expect(reached).toEqual([host.querySelector('.book-figure img')])
  })
})

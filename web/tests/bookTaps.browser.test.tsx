import { act } from 'react'
import { afterAll, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { BookReader } from '../src/BookReader'
import { topOf } from '../src/bookPages'
import '../src/index.css'
import { paintedBox } from '../src/notes'
import { CLICK_WAIT_MS } from '../src/tapClick'
import { mount } from './support/mount'
import { mouse, pen } from './support/mouse'
import { pointerRulesAs } from './support/pointer'
import { ringOf } from './support/ring'
import { tapAt } from './support/touch'

vi.mock('../src/Account', () => ({ Account: () => null }))

// Images the browser draws at a size of their own: a small one, so the
// full-size figure leaves room beside it, `PLATE`, as wide as the column
// and taller than the screen, `COVER`, a wide picture smaller than a page,
// so a page drawing it leaves room above and below it, and `TALL`, shaped
// as a printed book's cover.
vi.mock('../src/book', async (original) => ({
  ...(await original<typeof import('../src/book')>()),
  bookImageUrl: (_work: string, _edition: string, image: number) =>
    image === PLATE
      ? svg(2000, 20000)
      : image === COVER
        ? svg(200, 60)
        : image === TALL
          ? svg(683, 1024)
          : svg(120, 80),
}))

const PLATE = 9
const COVER = 8
const TALL = 7

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

/** Whether each `mousedown` from here on in the test was prevented, heard after all else on its way. */
function mouseDowns(): boolean[] {
  const prevented: boolean[] = []
  const heard = (event: Event) => prevented.push(event.defaultPrevented)
  window.addEventListener('mousedown', heard)
  onTestFinished(() => window.removeEventListener('mousedown', heard))
  return prevented
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
    // Its own size, so the room beside it is the dialog's.
    localStorage.setItem(
      'taiyaku.prefs.fit',
      JSON.stringify({ upscaleToFit: false }),
    )
    onTestFinished(() => localStorage.removeItem('taiyaku.prefs.fit'))
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

  it('leaves the mousedown of a tap on a reference, on a figure and of a double tap to the browser', async () => {
    const host = await reader()
    const prevented = mouseDowns()
    const reference = middleOf(
      host.querySelector('[data-block="0"] a') as Element,
    )
    await tapAt(reference.x, reference.y)
    await settled()
    expect(prevented).toEqual([false])
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
      )
    })
    await settled()

    prevented.length = 0
    const figure = middleOf(host.querySelector('.book-figure img') as Element)
    await tapAt(figure.x, figure.y)
    await settled()
    expect(prevented).toEqual([false])
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
      )
    })
    await settled()

    prevented.length = 0
    const words = host.querySelector('[data-block="1"] [data-sentence]')
    const line = (words as Element).getClientRects()[0]
    const at = { x: line.left + line.width / 2, y: line.top + line.height / 2 }
    const first = Date.now() / 1000
    await tapAt(at.x, at.y, first)
    await tapAt(at.x, at.y, first + 0.15)
    await settled()
    expect(prevented).toEqual([false, false])
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

/** `book` with a cover before its first block, a page of its own. */
const covered = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      figures: [{ before: 0, image: COVER, alt: 'the cover', page: true }],
    },
  },
}

/** `covered` with its cover set in a web link. */
const linkedCovered = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      figures: [
        {
          before: 0,
          image: COVER,
          alt: 'the cover',
          page: true,
          link: { url: 'https://y.org/', note: false },
        },
      ],
    },
  },
}

/** `covered` with a printed book's cover, `TALL`, taller than it is wide. */
const tallCovered = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      figures: [{ before: 0, image: TALL, alt: 'the cover', page: true }],
    },
  },
}

/** A paged layout, and upscale-to-fit as `upscaleToFit` says, for this case alone. */
function pagedWith(upscaleToFit: boolean) {
  localStorage.setItem(
    'taiyaku.prefs.book-layout',
    JSON.stringify({ mode: 'scroll', paged: true }),
  )
  localStorage.setItem('taiyaku.prefs.fit', JSON.stringify({ upscaleToFit }))
  onTestFinished(() => {
    localStorage.removeItem('taiyaku.prefs.book-layout')
    localStorage.removeItem('taiyaku.prefs.fit')
  })
}

/**
 * The page image on `host`'s page: its figure and image, the page it fills,
 * the picture its image draws, contained in it, and the page's middle third
 * across, which a tap in neither turns back nor on.
 */
function coverOf(host: HTMLElement) {
  const text = host.querySelector('.book-text') as HTMLElement
  const figure = host.querySelector('[data-page-image]') as HTMLElement
  const image = figure.querySelector('img') as HTMLImageElement
  const box = image.getBoundingClientRect()
  const art = paintedBox(
    { w: image.naturalWidth, h: image.naturalHeight },
    { w: box.width, h: box.height },
  )
  const across = text.getBoundingClientRect()
  return {
    figure,
    image,
    sheet: figure.getBoundingClientRect(),
    picture: new DOMRect(box.left + art.x, box.top + art.y, art.w, art.h),
    middle: {
      left: across.left + across.width / 3,
      right: across.left + (across.width * 2) / 3,
    },
  }
}

/**
 * Every element whose `click` from here on in the test reaches the document
 * unprevented, and so would follow a web link it is set in. Each is prevented
 * there, after the reader has heard it, so no link is opened.
 */
function linksFollowed(): EventTarget[] {
  const followed: EventTarget[] = []
  const hear = (event: Event) => {
    if (!event.defaultPrevented && event.target) followed.push(event.target)
    event.preventDefault()
  }
  document.addEventListener('click', hear)
  onTestFinished(() => document.removeEventListener('click', hear))
  return followed
}

/**
 * A finger's tap on a paged book's cover, a page image: the page already
 * sizes it as the comic reader sizes a page, so a tap opens nothing and is the
 * page's, its middle third showing or hiding the bars, on the picture or on
 * the room the page leaves around it. Set in a link, a tap on its picture in
 * the middle third follows the link. With upscale-to-fit off the room is the
 * figure's, or the link's, around an image drawn its own size; on, it is
 * inside the image, drawn the page's size with its picture contained in it.
 */
describe('a finger’s tap on a paged book’s cover', () => {
  afterAll(() => vi.unstubAllGlobals())

  it.each([
    ['off', false],
    ['on', true],
  ] as const)(
    'with upscale-to-fit %s, shows the bars from a tap in its middle third beside its picture, hides them from one on the picture, and opens nothing',
    async (_, upscaleToFit) => {
      pagedWith(upscaleToFit)
      const host = await reader(covered)
      const { figure, image, sheet, picture, middle } = coverOf(host)
      const x = (middle.left + middle.right) / 2
      const beside = { x, y: (sheet.top + picture.top) / 2 }
      const on = { x, y: picture.top + picture.height / 2 }
      const barsUp = () => host.querySelector('.reader-top-bar') !== null
      const full = () => host.querySelector('dialog.book-figure-full')
      // THE STATE THIS TEST NAMES: a tap above the picture, in the page's
      // middle third, lands on the cover, on its image where upscaled, and
      // one on the picture's middle on its image.
      expect([
        document.elementFromPoint(beside.x, beside.y) ===
          (upscaleToFit ? image : figure),
        beside.y < picture.top - 10,
        document.elementFromPoint(on.x, on.y) === image,
        barsUp(),
      ]).toEqual([true, true, true, false])
      const first = Date.now() / 1000
      await tapAt(beside.x, beside.y, first)
      await until(() => barsUp() || full() !== null)
      await settled()
      expect([barsUp(), full()]).toEqual([true, null])
      expect(document.elementFromPoint(on.x, on.y)).toBe(image)
      await tapAt(on.x, on.y, first + 1)
      await until(() => !barsUp() || full() !== null)
      await settled()
      expect([barsUp(), full()]).toEqual([false, null])
    },
  )

  it.each([
    ['off', false],
    ['on', true],
  ] as const)(
    'with upscale-to-fit %s, set in a web link, shows the bars from a tap in its middle third beside its picture, and follows the link from one on the picture',
    async (_, upscaleToFit) => {
      pagedWith(upscaleToFit)
      const host = await reader(linkedCovered)
      const { image, sheet, picture, middle } = coverOf(host)
      const x = (middle.left + middle.right) / 2
      const beside = { x, y: (sheet.top + picture.top) / 2 }
      const on = { x, y: picture.top + picture.height / 2 }
      const barsUp = () => host.querySelector('.reader-top-bar') !== null
      const full = () => host.querySelector('dialog.book-figure-full')
      const followed = linksFollowed()
      // THE STATE THIS TEST NAMES: a tap above the picture, in the page's
      // middle third, lands in the link, on its image where upscaled, and
      // one on the picture's middle on its image.
      expect([
        document.elementFromPoint(beside.x, beside.y) ===
          (upscaleToFit ? image : image.closest('a')),
        beside.y < picture.top - 10,
        document.elementFromPoint(on.x, on.y) === image,
        barsUp(),
      ]).toEqual([true, true, true, false])
      const first = Date.now() / 1000
      await tapAt(beside.x, beside.y, first)
      await until(() => barsUp())
      await settled()
      expect([barsUp(), followed, full()]).toEqual([true, [], null])
      await tapAt(on.x, on.y, first + 1)
      await settled()
      await settled()
      expect([barsUp(), followed, full()]).toEqual([true, [image], null])
    },
  )

  it('at 1280×800, shows the bars from a tap in the middle of a page-high cover, all picture across its page’s middle third, and hides them from a second', async () => {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(1280, 800)
    pagedWith(true)
    const host = await reader(tallCovered)
    const { image, sheet, picture, middle } = coverOf(host)
    const at = {
      x: (middle.left + middle.right) / 2,
      y: picture.top + picture.height / 2,
    }
    const barsUp = () => host.querySelector('.reader-top-bar') !== null
    const full = () => host.querySelector('dialog.book-figure-full')
    // THE STATE THIS TEST NAMES: the picture runs the page's height and
    // across its middle third, so no room beside it is left there.
    expect([
      picture.top,
      picture.bottom,
      picture.left < middle.left,
      picture.right > middle.right,
      document.elementFromPoint(at.x, at.y) === image,
    ]).toEqual([
      expect.closeTo(sheet.top, 0),
      expect.closeTo(sheet.bottom, 0),
      true,
      true,
      true,
    ])
    const first = Date.now() / 1000
    await tapAt(at.x, at.y, first)
    await until(() => barsUp() || full() !== null)
    await settled()
    expect([barsUp(), full()]).toEqual([true, null])
    expect(document.elementFromPoint(at.x, at.y)).toBe(image)
    await tapAt(at.x, at.y, first + 1)
    await until(() => !barsUp() || full() !== null)
    await settled()
    expect([barsUp(), full()]).toEqual([false, null])
  })
})

/**
 * A mouse over a paged book's cover set in a web link: the link's pointer
 * only where a click follows the link, on its picture in the page's middle
 * third, and no pointer on the room beside the picture or in an outer third.
 */
describe('a mouse over a paged book’s cover set in a link', () => {
  afterAll(() => vi.unstubAllGlobals())

  it.each([
    ['off', false],
    ['on', true],
  ] as const)(
    'with upscale-to-fit %s, shows the link’s pointer on its picture in the page’s middle third only',
    async (_, upscaleToFit) => {
      pagedWith(upscaleToFit)
      const host = await reader(linkedCovered)
      const { sheet, picture, middle } = coverOf(host)
      const x = (middle.left + middle.right) / 2
      const y = picture.top + picture.height / 2
      const onPicture = { x, y }
      const beside = { x, y: (sheet.top + picture.top) / 2 }
      const outer = { x: picture.left + 2, y }
      // THE STATE THIS TEST NAMES: the picture reaches into the page's left
      // third, and there is room above it.
      expect([outer.x < middle.left, beside.y < picture.top - 10]).toEqual([
        true,
        true,
      ])
      const cursors: string[] = []
      for (const at of [onPicture, beside, outer, onPicture]) {
        await mouse.moveTo(at.x, at.y)
        const under = document.elementFromPoint(at.x, at.y) as Element
        cursors.push(getComputedStyle(under).cursor)
      }
      expect(cursors).toEqual(['pointer', 'default', 'default', 'pointer'])
    },
  )
})

/**
 * The keys after a tap on the room beside the picture of a paged book's
 * cover set in a web link, in the page's middle third: the tap is the page's,
 * so it leaves the link without focus, and Space and Enter after it are the
 * reader's, where a focused link would take them both.
 */
describe('the keys after a tap beside the picture of a paged book’s cover set in a link', () => {
  afterAll(() => vi.unstubAllGlobals())

  /** `linkedCovered` with a second edition, for Space to swap to. */
  const twice = {
    ...linkedCovered,
    editions: {
      ...linkedCovered.editions,
      fr: { ...linkedCovered.editions.en, label: 'French', language: 'fr' },
    },
  }

  it.each([
    ['finger', 'off', false],
    ['finger', 'on', true],
    ['mouse', 'off', false],
    ['mouse', 'on', true],
    ['pen', 'off', false],
    ['pen', 'on', true],
  ] as const)(
    'after a %s’s, with upscale-to-fit %s, shows the bars and leaves the link unfocused, so Space swaps editions and Enter follows nothing',
    async (by, _, upscaleToFit) => {
      pagedWith(upscaleToFit)
      const host = await reader(twice)
      const text = host.querySelector('.book-text') as HTMLElement
      const { image, sheet, picture, middle } = coverOf(host)
      const at = {
        x: (middle.left + middle.right) / 2,
        y: (sheet.top + picture.top) / 2,
      }
      // Real keys reach the frame once something in it has had focus.
      const stop = document.createElement('button')
      document.body.append(stop)
      stop.focus()
      stop.remove()
      const followed = linksFollowed()
      // THE STATE THIS TEST NAMES: the tap lands in the link, above its
      // picture, with focus on the body.
      expect([
        image.closest('a')?.contains(document.elementFromPoint(at.x, at.y)),
        at.y < picture.top - 10,
      ]).toEqual([true, true])
      expect(document.activeElement).toBe(document.body)
      if (by === 'finger') await tapAt(at.x, at.y)
      else {
        const pointer = by === 'mouse' ? mouse : pen
        await act(async () => {
          await pointer.moveTo(at.x, at.y)
          await pointer.press()
          await pointer.release()
        })
      }
      await until(() => host.querySelector('.reader-top-bar') !== null)
      await settled()
      expect(host.querySelector('.reader-top-bar')).not.toBeNull()
      expect(document.activeElement).toBe(document.body)
      await act(async () => {
        await userEvent.keyboard(' ')
      })
      await settled()
      expect(text.lang).toBe('fr')
      await act(async () => {
        await userEvent.keyboard('{Enter}')
      })
      await settled()
      expect(followed).toEqual([])
    },
  )
})

/** `book` with its small figure set before its first block, where a book opened afresh starts. */
const mapped = {
  ...book,
  editions: {
    en: {
      ...book.editions.en,
      figures: [{ before: 0, image: 0, alt: 'a map' }],
    },
  },
}

/** Where `image` draws its picture, contained in its own box and centred in it. */
function pictureOf(image: HTMLImageElement): DOMRect {
  const box = image.getBoundingClientRect()
  const art = paintedBox(
    { w: image.naturalWidth, h: image.naturalHeight },
    { w: box.width, h: box.height },
  )
  return new DOMRect(box.left + art.x, box.top + art.y, art.w, art.h)
}

/**
 * A figure opened full size: no larger than the screen lets it be drawn, and
 * larger than its own size only while the comic reader's upscale-to-fit is
 * on, so a small picture opens no smaller than a page drew it.
 */
describe('a figure opened full size', () => {
  afterAll(() => vi.unstubAllGlobals())

  it.each([
    ['off', 'scrolled', false, false],
    ['on', 'scrolled', true, false],
    ['off', 'paged', false, true],
    ['on', 'paged', true, true],
  ] as const)(
    'with upscale-to-fit %s, %s, draws a small picture its own size, or while on as large as the screen lets it be, never smaller than the page drew it',
    async (_, __, upscaleToFit, paged) => {
      localStorage.setItem(
        'taiyaku.prefs.book-layout',
        JSON.stringify({ mode: 'scroll', paged }),
      )
      localStorage.setItem(
        'taiyaku.prefs.fit',
        JSON.stringify({ upscaleToFit }),
      )
      onTestFinished(() => {
        localStorage.removeItem('taiyaku.prefs.book-layout')
        localStorage.removeItem('taiyaku.prefs.fit')
      })
      const host = await reader(mapped)
      const figure = host.querySelector('.book-figure img') as HTMLImageElement
      const onPage = figure.getBoundingClientRect()
      // THE STATE THIS TEST NAMES: the page draws the 120×80 picture its own
      // size, as a figure, not a page image.
      expect([
        onPage.width,
        onPage.height,
        figure.closest('[data-page-image]'),
      ]).toEqual([120, 80, null])
      const at = middleOf(figure)
      await tapAt(at.x, at.y)
      const full = () =>
        host.querySelector<HTMLDialogElement>('dialog.book-figure-full')
      await until(() => full()?.open === true)
      const image = full()?.querySelector('img') as HTMLImageElement
      await until(() => image.complete && image.naturalWidth > 0)
      const dialog = full() as HTMLDialogElement
      const style = getComputedStyle(dialog)
      const room = {
        w:
          dialog.clientWidth -
          Number.parseFloat(style.paddingLeft) -
          Number.parseFloat(style.paddingRight),
        h:
          dialog.clientHeight -
          Number.parseFloat(style.paddingTop) -
          Number.parseFloat(style.paddingBottom),
      }
      const scale = upscaleToFit ? Math.min(room.w / 120, room.h / 80) : 1
      const drawn = pictureOf(image)
      expect([drawn.width, drawn.height]).toEqual([
        expect.closeTo(120 * scale, 0),
        expect.closeTo(80 * scale, 0),
      ])
      expect(scale > 1).toBe(upscaleToFit)
      expect(drawn.width).toBeGreaterThanOrEqual(onPage.width - 0.5)
      expect(drawn.height).toBeGreaterThanOrEqual(onPage.height - 0.5)
    },
  )

  it('with upscale-to-fit on, keeps open the image its tap opened though the tap’s click lands on the room beside its picture, and closes it from a tap there, not on the picture', async () => {
    localStorage.setItem(
      'taiyaku.prefs.fit',
      JSON.stringify({ upscaleToFit: true }),
    )
    onTestFinished(() => localStorage.removeItem('taiyaku.prefs.fit'))
    const host = await reader(mapped)
    const full = () =>
      host.querySelector<HTMLDialogElement>('dialog.book-figure-full')
    const { aimed } = clicks()
    const at = middleOf(host.querySelector('.book-figure img') as Element)
    await tapAt(at.x, at.y)
    await settled()
    const image = full()?.querySelector('img') as HTMLImageElement
    await until(() => image.complete && image.naturalWidth > 0)
    const picture = pictureOf(image)
    // THE STATE THIS TEST NAMES: the tap's click was sent to the image drawn
    // full size, above its picture, on the room the image leaves there.
    expect([full()?.open, aimed, at.y < picture.top - 10]).toEqual([
      true,
      [image],
      true,
    ])

    const on = { x: picture.left + picture.width / 2, y: picture.top + 10 }
    await tapAt(on.x, on.y)
    await settled()
    expect(full()?.open).toBe(true)

    expect(document.elementFromPoint(at.x, at.y)).toBe(image)
    await tapAt(at.x, at.y)
    await settled()
    expect(full()).toBeNull()
  })
})

/** Real time, a frame at a time, until `check` holds; throws if it never does. */
async function until(check: () => boolean, ms = 3000) {
  const deadline = performance.now() + ms
  while (!check()) {
    if (performance.now() > deadline) throw new Error('condition never held')
    await act(() => new Promise((done) => setTimeout(done, 20)))
  }
}

/**
 * `Escape` in the book closes what is open, a footnote first, then takes the
 * bars away while the keyboard holds them (focus a key put in them, or bars a
 * key brought up with no pointer pressed since), and otherwise leaves the
 * reader by its way back (O419). Bars a tap or the mouse brought up are not
 * the keyboard's until a key moves focus into them. Each case counts the
 * reader's way out; `back.browser.test.tsx` follows it.
 */
describe('Escape in the book, with real keys (O419)', () => {
  afterAll(() => vi.unstubAllGlobals())

  /** A real key, press or click, inside `act`. Nothing here navigates: the way out is counted. */
  const real = async (input: () => Promise<unknown>) => {
    await act(async () => {
      await input()
    })
  }

  const barsUp = (host: HTMLElement) =>
    host.querySelector('.reader-top-bar') !== null

  /** Whether focus is in either bar. */
  const focusInBars = () =>
    document.activeElement?.closest('.reader-top-bar, .reader-bottom-bar') !=
    null

  /** The footnote reference in block 0; focused, it sends real keys to the frame. */
  const reference = (host: HTMLElement) =>
    host.querySelector('[data-block="0"] a') as HTMLElement

  it('closes a footnote at the first Escape, and leaves at the second', async () => {
    const onLeave = vi.fn()
    const host = await reader(book, onLeave)
    const note = () =>
      host.querySelector<HTMLDialogElement>('dialog.book-footnote')
    await real(() => userEvent.click(reference(host)))
    await until(() => note()?.open === true)
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    const first = [note(), onLeave.mock.calls.length]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([first, onLeave.mock.calls.length]).toEqual([[null, 0], 1])
  })

  it('leaves at one Escape through bars a tap brought up', async () => {
    const onLeave = vi.fn()
    const host = await reader(book, onLeave)
    await real(() =>
      userEvent.click(
        host.querySelector('[data-block="2"] [data-sentence]') as HTMLElement,
      ),
    )
    await until(() => barsUp(host))
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([focusInBars(), onLeave.mock.calls.length]).toEqual([false, 1])
  })

  /** Bars a tap brought up, which Tab then moves focus into from the text. */
  it('takes the bars away at the first Escape while keyboard focus is in them, and leaves at the second', async () => {
    const onLeave = vi.fn()
    const host = await reader(book, onLeave)
    await real(() =>
      userEvent.click(
        host.querySelector('[data-block="5"] [data-sentence]') as HTMLElement,
      ),
    )
    await until(() => barsUp(host))
    for (let i = 0; i < 12 && !focusInBars(); i++)
      await real(() => userEvent.tab())
    const into = [focusInBars(), barsUp(host)]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    const first = [barsUp(host), onLeave.mock.calls.length]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([into, first, onLeave.mock.calls.length]).toEqual([
      [true, true],
      [false, 0],
      1,
    ])
  })

  /** The keyboard brought the bars up, so its Escape takes them away though focus is not in them yet. */
  it('hides the bars at an Escape straight after the Tab that showed them, and leaves at the next', async () => {
    const onLeave = vi.fn()
    const host = await reader(book, onLeave)
    reference(host).focus()
    await real(() => userEvent.tab())
    await until(() => barsUp(host))
    const shown = [focusInBars(), document.activeElement === reference(host)]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    const first = [barsUp(host), onLeave.mock.calls.length]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([shown, first, onLeave.mock.calls.length]).toEqual([
      [false, true],
      [false, 0],
      1,
    ])
  })

  /**
   * Help opened by a click on bars a tap brought up: closing it at an
   * `Escape` hands focus back to Help, so the bars are then the keyboard's,
   * and one `Escape` a step: Help, the bars, the book.
   */
  it('closes Help a click opened at the first Escape with focus back on Help, takes the bars away at the second, and leaves at the third', async () => {
    const onLeave = vi.fn()
    const host = await reader(book, onLeave)
    const helpOpen = () =>
      host.querySelector('[role="dialog"][aria-label="Keyboard help"]') !== null
    await real(() =>
      userEvent.click(
        host.querySelector('[data-block="2"] [data-sentence]') as HTMLElement,
      ),
    )
    await until(() => barsUp(host))
    const help = host.querySelector('.reader-help') as HTMLElement
    await real(() => userEvent.click(help))
    await until(helpOpen)
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    const first = [
      helpOpen(),
      document.activeElement === help,
      barsUp(host),
      onLeave.mock.calls.length,
    ]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    const second = [focusInBars(), barsUp(host), onLeave.mock.calls.length]
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([first, second, onLeave.mock.calls.length]).toEqual([
      [false, true, true, 0],
      [false, false, 0],
      1,
    ])
  })

  /** A jump the mouse made brings the bars up with Back, and they are the mouse's. */
  it('leaves at one Escape through bars a mouse click on a link brought up', async () => {
    const onLeave = vi.fn()
    const host = await reader(plated, onLeave)
    const box = (
      host.querySelector('.book-figure img') as Element
    ).getBoundingClientRect()
    await act(async () => {
      await mouse.moveTo(
        box.left + box.width / 2,
        (box.top + window.innerHeight) / 2,
      )
      await mouse.press()
      await mouse.release()
    })
    await until(() => barsUp(host) && backIn(host) !== undefined)
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([barsUp(host), onLeave.mock.calls.length]).toEqual([true, 1])
  })

  it('closes the search at an Escape typed in its box, and stays', async () => {
    const onLeave = vi.fn()
    const host = await reader(book, onLeave)
    const box = () =>
      host.querySelector<HTMLInputElement>('.docked-panel input[type="search"]')
    reference(host).focus()
    await real(() => userEvent.keyboard('/'))
    await until(() => box() !== null && document.activeElement === box())
    await real(() => userEvent.keyboard('opens'))
    await real(() => userEvent.keyboard('{Escape}'))
    await settled()
    expect([box(), onLeave.mock.calls.length]).toEqual([null, 0])
  })
})

/**
 * The book's bottom bar ends in Help while the help module is on (O425): a
 * click opens it as `?` does, and Help gives focus back on closing (O161).
 */
describe('Help in the book’s bottom bar, with real clicks (O425)', () => {
  afterAll(() => vi.unstubAllGlobals())

  /** A real key, press or click, inside `act`. */
  const real = async (input: () => Promise<unknown>) => {
    await act(async () => {
      await input()
    })
  }

  const barsUp = (host: HTMLElement) =>
    host.querySelector('.reader-bottom-bar') !== null

  const helpOpen = (host: HTMLElement) =>
    host.querySelector('[role="dialog"][aria-label="Keyboard help"]') !== null

  /** Every button named Help, wherever it is. */
  const helps = (host: HTMLElement) =>
    [...host.querySelectorAll('button')].filter(
      (b) => b.getAttribute('aria-label') === 'Help',
    )

  /** A tap on the text, which brings the bars up. */
  async function tapForBars(host: HTMLElement) {
    await real(() =>
      userEvent.click(
        host.querySelector('[data-block="2"] [data-sentence]') as HTMLElement,
      ),
    )
    await until(() => barsUp(host))
  }

  it('draws one Help while the bars are up; a click opens Help, and Escape closes it onto Help', async () => {
    const host = await reader(book)
    const down = helps(host).length
    await tapForBars(host)
    expect([down, helps(host).length]).toEqual([0, 1])
    const [help] = helps(host)
    expect(help.closest('.reader-bottom-bar')).not.toBeNull()
    await real(() => userEvent.click(help))
    await until(() => helpOpen(host))
    const opened = help.getAttribute('aria-expanded')
    await real(() => userEvent.keyboard('{Escape}'))
    await until(() => !helpOpen(host))
    expect([
      opened,
      help.getAttribute('aria-expanded'),
      document.activeElement === help,
      barsUp(host),
    ]).toEqual(['true', 'false', true, true])
  })

  /** The bars sit on the window's bottom edge, which cuts anything drawn past it. */
  it('draws Help’s focus ring whole inside the window', async () => {
    const host = await reader(book)
    ;(host.querySelector('[data-block="0"] a') as HTMLElement).focus()
    for (let i = 0; i < 30 && helps(host)[0] !== document.activeElement; i++) {
      await real(() => userEvent.tab())
    }
    const [help] = helps(host)
    // The fixture tells: real keys brought focus to Help.
    expect(document.activeElement).toBe(help)
    const ring = ringOf(help)
    expect({
      left: ring.left >= 0,
      top: ring.top >= 0,
      right: ring.right <= innerWidth,
      bottom: ring.bottom <= innerHeight,
    }).toEqual({ left: true, top: true, right: true, bottom: true })
  })

  it.each([
    ['mouse', true],
    ['touch laptop', true],
    ['tablet with trackpad', true],
    ['phone', false],
  ] as const)('draws Help under a %s’s pointers: %s', async (device, drawn) => {
    onTestFinished(pointerRulesAs(device))
    const host = await reader(book)
    await tapForBars(host)
    expect(helps(host).map((help) => help.checkVisibility())).toEqual([drawn])
  })

  it('draws no Help once Keyboard help is switched off in Settings', async () => {
    onTestFinished(() => localStorage.removeItem('taiyaku.prefs.help'))
    const host = await reader(book)
    await tapForBars(host)
    const on = helps(host).length
    const gear = [...host.querySelectorAll('.reader-top-bar button')].find(
      (b) => b.textContent === 'Settings',
    ) as HTMLElement
    await real(() => userEvent.click(gear))
    const settings = () =>
      host.querySelector('[role="dialog"][aria-label="Settings"]')
    await until(() => settings() !== null)
    const section = [
      ...(settings() as Element).querySelectorAll('.reader-section'),
    ].find((s) => s.querySelector('h3')?.textContent === 'Keyboard help')
    const enabled = section?.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement
    await real(() => userEvent.click(enabled))
    await until(() => !enabled.checked)
    await real(() => userEvent.keyboard('{Escape}'))
    await until(() => settings() === null)
    expect([on, barsUp(host), helps(host).length]).toEqual([1, true, 0])
  })
})

/**
 * The book's readout row at a phone's width with Back drawn, its fullest
 * (O425): each button keeps to one line inside the row, under a mouse's rules
 * and a finger's, and the readout text alone gives way. The finger's fullest
 * row is a tablet's with a trackpad, which draws Help as well; a touch screen
 * alone draws none. Where the buttons are wider than the window even so, the
 * row scrolls sideways: each, scrolled into view, is whole inside the window
 * and takes a press at its middle.
 */
describe('the book’s readout row at a phone’s width, with Back drawn (O425)', () => {
  afterAll(() => vi.unstubAllGlobals())

  const NAMES = ['Contents', 'Search', 'Look up words', 'Back', 'Help']

  /** The row's buttons a stylesheet draws: Help is not, under a touch screen alone. */
  const buttonsOf = (row: HTMLElement) =>
    [...row.querySelectorAll<HTMLElement>('button')].filter((button) =>
      button.checkVisibility(),
    )

  /** The book at `width`×`height` with Back drawn by a jump from the keyboard; its readout row. */
  async function rowWithBack(width: number, height: number) {
    await page.viewport(width, height)
    const host = await reader(plated)
    const link = host.querySelector('.book-figure a') as HTMLElement
    act(() => {
      link.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    await settled()
    await document.fonts.ready
    return host.querySelector('.reader-readout') as HTMLElement
  }

  /**
   * Each button's name, whether it lies within what the row shows top to
   * bottom (its box, less any scrollbar), and whether it lies inside the
   * window side to side.
   */
  function measured(row: HTMLElement, button: HTMLElement) {
    const at = row.getBoundingClientRect()
    const box = button.getBoundingClientRect()
    const shownBottom = at.top + row.clientTop + row.clientHeight
    return [
      button.getAttribute('aria-label') ?? button.textContent,
      box.top >= at.top && box.bottom <= shownBottom,
      box.left >= 0 && box.right <= window.innerWidth,
    ]
  }

  for (const device of ['mouse', 'tablet with trackpad', 'phone'] as const) {
    it.each([
      [390, 844],
      [375, 667],
    ])(
      `keeps every button on one line inside the row at %i×%i, ${device}`,
      async (width, height) => {
        onTestFinished(pointerRulesAs(device))
        const row = await rowWithBack(width, height)
        const fits = buttonsOf(row).map((button) => measured(row, button))
        expect(fits).toEqual(
          NAMES.filter((name) => device !== 'phone' || name !== 'Help').map(
            (name) => [name, true, true],
          ),
        )
      },
    )
  }

  for (const device of ['mouse', 'tablet with trackpad'] as const) {
    it(`scrolls the row to each button, whole and inside the window, at 320×568, ${device}`, async () => {
      onTestFinished(pointerRulesAs(device))
      const row = await rowWithBack(320, 568)
      const buttons = buttonsOf(row)
      // THE STATE THIS TEST NAMES: the buttons alone are wider than the
      // window, so they cannot all be in it at once.
      const widths = buttons.reduce(
        (sum, button) => sum + button.getBoundingClientRect().width,
        0,
      )
      expect(widths).toBeGreaterThan(window.innerWidth)
      const fits = buttons.map((button) => {
        button.scrollIntoView({ block: 'nearest', inline: 'nearest' })
        const box = button.getBoundingClientRect()
        const pressed = document.elementFromPoint(
          box.left + box.width / 2,
          box.top + box.height / 2,
        )
        return [...measured(row, button), pressed === button]
      })
      expect(fits).toEqual(NAMES.map((name) => [name, true, true, true]))
    })
  }

  /**
   * Where the row fits, from the first button's start to the readout's end,
   * centred between the row's start and Help, less the row's 4 px gap, or
   * the row's end where Help is switched off or not drawn.
   */
  it.each([
    ['on', 'Help', 'mouse'],
    ['off', undefined, 'mouse'],
    ['on, undrawn under a touch screen alone', undefined, 'phone'],
  ] as const)(
    'centres its buttons and readout where they fit, at 1280×800, with Help %s',
    async (help, last, device) => {
      onTestFinished(pointerRulesAs(device))
      if (help === 'off') {
        localStorage.setItem(
          'taiyaku.prefs.help',
          JSON.stringify({ enabled: false }),
        )
        onTestFinished(() => localStorage.removeItem('taiyaku.prefs.help'))
      }
      const row = await rowWithBack(1280, 800)
      const items = [...row.children].filter((item) => item.checkVisibility())
      const end = items.at(-1) as HTMLElement
      expect(end.getAttribute('aria-label') ?? undefined).toBe(last)
      const group = items.filter((item) => !item.matches('.reader-help'))
      const from = (group[0] as HTMLElement).getBoundingClientRect().left
      const to = (group.at(-1) as HTMLElement).getBoundingClientRect().right
      const at = row.getBoundingClientRect()
      // Help, where it is on, stands at the row's end.
      if (last) {
        expect(
          Math.abs(end.getBoundingClientRect().right - (at.right - 12)),
        ).toBeLessThanOrEqual(1)
      }
      const roomEnd = last
        ? end.getBoundingClientRect().left - 4
        : at.right - 12
      const room = (at.left + 12 + roomEnd) / 2
      expect(Math.abs((from + to) / 2 - room)).toBeLessThanOrEqual(1)
      // THE STATE THIS TEST NAMES: room to spare, so centring has something
      // to do.
      expect(from).toBeGreaterThan(at.left + 12 + 100)
    },
  )
})

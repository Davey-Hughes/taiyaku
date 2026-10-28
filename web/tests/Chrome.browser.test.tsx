import { act } from 'react'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
// Required, for the reason `Filmstrip.browser.test.tsx` gives at its own
// import: the reader's custom properties, the line's height and the mark's
// colour all reach a page through this.
import '../src/index.css'
import { Chrome, type ChromeProps } from '../src/Chrome'
import type { Group } from '../src/pairing'
import type { Position } from '../src/resolver'
import { mount, rerender } from './support/mount'
import { mouse } from './support/mouse'

/**
 * O216: the mark on the progress line, with a real strip over a real line.
 *
 * `Chrome.test.tsx` holds what `Chrome` writes on the mark, a `max()` and a
 * `calc()` that jsdom leaves unresolved, and `Filmstrip.test.tsx` when the
 * strip tells and what it tells from numbers given by hand. Neither can say
 * where the mark lands. Here the rail's numbers are the browser's, the strip
 * tells them itself, and what is read is the mark's own box against the
 * line's and the tiles'.
 *
 * Nothing answers `/api` in this project, so the thumbnails the strip asks
 * for never load and every tile stands at its floor: the same footing
 * `Filmstrip.browser.test.tsx` describes. The one case about a thumbnail
 * loading makes its own.
 *
 * **Both reading directions, every case that measures a box.** The rail is
 * `row-reverse` for a work read right to left, its `scrollLeft` runs
 * negative, and the line is mirrored: each of the three is a way to be right
 * in one direction and wrong in the other.
 */

/** A volume of `pages` single pages, one tile each. */
function volume(pages: number): { slots: Position[]; groups: Group[] } {
  const slots: Position[] = Array.from({ length: pages }, (_, i) => ({
    page: i,
  }))
  return { slots, groups: slots.map((_, i) => ({ slots: [i] })) }
}

// Lengths of volume, for the things the rail's length decides. Each case
// that leans on one asserts it of the rail it measures, so a change to the
// tiles' size that moved a volume out of its kind fails there.
/** Long enough that an eighth of the share on show is narrower than the floor. */
const LONG = volume(120)
/** Long enough to overflow the rail, and short enough that the eighth is the wider. */
const SHORT = volume(24)
/** Short enough that the rail holds all of it. */
const FITS = volume(3)
/** A little longer than the rail is wide, in the wide window its case sets: the travel is a small part of the length. */
const BARELY = volume(21)

/** The floor and the fraction `Chrome.tsx` draws the mark by, as the spec gives them. */
const FLOOR_PX = 32
const SHARE = 1 / 8

function props(overrides: Partial<ChromeProps> = {}): ChromeProps {
  return {
    visible: true,
    motion: { animate: false, ms: 0 },
    readouts: {
      position: true,
      time: false,
      chapter: false,
      health: false,
      notes: false,
    },
    position: 'slot 1',
    chapter: '',
    progress: 0.25,
    editionLine: 'Raw',
    mode: 'single',
    onMode: () => {},
    settingsOpen: false,
    onSettings: () => {},
    onLeave: () => {},
    reading: 'ltr',
    work: 'w',
    edition: 'e',
    ...LONG,
    currentIndex: 0,
    onNavigate: () => {},
    ...overrides,
  }
}

/**
 * The two bars in a reader that fills the window. `.reader` carries the
 * custom properties, and a fixed box is what the bottom bar's `position:
 * absolute` docks against, as `App`'s root is in the app.
 *
 * Its own function so that a `rerender` can hand React the tree `inReader`
 * mounted, which keeps the bars and their strip the same instances across it.
 */
function reader(overrides: Partial<ChromeProps>) {
  return (
    <div className="reader" style={{ position: 'fixed', inset: 0 }}>
      <Chrome {...props(overrides)} />
    </div>
  )
}

function inReader(overrides: Partial<ChromeProps>) {
  return mount(reader(overrides))
}

const railOf = (host: HTMLElement) =>
  host.querySelector('.reader-strip-rail') as HTMLElement
const lineOf = (host: HTMLElement) =>
  host.querySelector('.reader-progress') as HTMLElement
/** The mark's element, which is there whenever the bars are, drawn or hidden. */
const markOf = (host: HTMLElement) =>
  host.querySelector('[data-testid="strip-shown"]') as HTMLElement

/** The mark's box. Throws where the mark is hidden, so a case cannot measure nothing. */
function markBox(host: HTMLElement): DOMRect {
  const mark = markOf(host)
  if (mark.hidden) throw new Error('the mark is hidden')
  return mark.getBoundingClientRect()
}

const centre = (box: DOMRect) => box.left + box.width / 2

const frame = () =>
  new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))

/**
 * Lets the observer on the rail make its first report. A `ResizeObserver`
 * reports once of its own accord, in the frame after it is told what to
 * watch, and the strip reads the rail when it does. Two frames, since the
 * report comes after the first frame's own callbacks have run.
 *
 * A case about something that happens after the strip has mounted waits for
 * this first, so that the report is behind it and what happens is told by
 * whatever is there to tell it, or by nothing.
 */
async function firstReportPast() {
  await act(async () => {
    await frame()
    await frame()
  })
}

/**
 * Waits for the rail to be at rest, which is two frames running in which
 * nothing but this asked for a frame. A glide of the strip asks for one in
 * every frame it runs, and the rail's scroll listener for one after every
 * scroll. Each frame is in an `act` of its own, so a render one asks for is
 * made before the next. Gives up after ten seconds, which fails the case.
 */
async function untilAtRest() {
  const asked = vi.spyOn(window, 'requestAnimationFrame')
  onTestFinished(() => asked.mockRestore())
  const deadline = performance.now() + 10_000
  let quiet = 0
  while (quiet < 2) {
    if (performance.now() > deadline) {
      throw new Error('the rail still asks for frames after ten seconds')
    }
    const before = asked.mock.calls.length
    await act(async () => {
      await frame()
    })
    quiet = asked.mock.calls.length - before > 1 ? 0 : quiet + 1
  }
}

/** How far the rail can be scrolled. */
const travel = (rail: HTMLElement) => rail.scrollWidth - rail.clientWidth

/**
 * The width the spec gives the mark for the rail as it stands: an eighth of
 * the share of its length on show, as a share of the line, and never under
 * the floor.
 */
function widthDue(rail: HTMLElement, line: HTMLElement): number {
  const share = rail.clientWidth / rail.scrollWidth
  return Math.max(FLOOR_PX, share * SHARE * line.getBoundingClientRect().width)
}

/**
 * Writes the rail's `scrollLeft`, then waits for the `scroll` the browser
 * fires for it and for the frame the rail's listener takes after that. The
 * listener here is added after the rail's own and the frame asked for after
 * the rail's, so the rail's runs first both times; the `act` held open
 * around them is what the state set in that frame lands inside.
 *
 * `to` has to be somewhere the rail is not, or no `scroll` comes.
 */
async function scrollRail(rail: HTMLElement, to: number) {
  expect(Math.round(rail.scrollLeft)).not.toBe(Math.round(to))
  await act(async () => {
    const scrolled = new Promise((resolve) =>
      rail.addEventListener('scroll', resolve, { once: true }),
    )
    rail.scrollLeft = to
    await scrolled
    await frame()
  })
}

/**
 * Which side of an earlier tile a later one stands on: `1` for the right,
 * `-1` for the left. Asked of two tiles' boxes rather than of the reading
 * direction, so no case here decides for itself what `rtl` means.
 */
function laterSide(rail: HTMLElement): 1 | -1 {
  const [first, second] = [...rail.querySelectorAll('button')].map((b) =>
    centre(b.getBoundingClientRect()),
  )
  if (first === undefined || second === undefined || first === second) {
    throw new Error('two tiles side by side are needed to tell the sides apart')
  }
  return second > first ? 1 : -1
}

/** The rail's far end, as the browser signs it for this rail. */
const farEnd = (rail: HTMLElement) => laterSide(rail) * travel(rail)

describe('the mark on the progress line, laid out', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`stands flush with the line's start while the rail stands at its own, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host).getBoundingClientRect()

      // THE STATE THIS TEST NAMES: a rail that overflows, at its start.
      expect(travel(rail)).toBeGreaterThan(0)
      expect(rail.scrollLeft).toBe(0)

      // The line starts on the side the first tile stands on: the side later
      // tiles are not.
      const mark = markBox(host)
      if (laterSide(rail) === 1) {
        expect(mark.left).toBeCloseTo(line.left, 0)
      } else {
        expect(mark.right).toBeCloseTo(line.right, 0)
      }
      // Inside the line whichever end it stands at.
      expect(mark.left).toBeGreaterThanOrEqual(line.left - 0.5)
      expect(mark.right).toBeLessThanOrEqual(line.right + 0.5)
    })

    it(`stands flush with the line's end once the rail is scrolled to its own, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()

      await scrollRail(rail, farEnd(rail))

      // THE STATE THIS TEST NAMES: the rail has gone as far as it goes.
      expect(Math.abs(rail.scrollLeft)).toBeCloseTo(travel(rail), 0)

      const line = lineOf(host).getBoundingClientRect()
      const mark = markBox(host)
      if (later === 1) {
        expect(mark.right).toBeCloseTo(line.right, 0)
      } else {
        expect(mark.left).toBeCloseTo(line.left, 0)
      }
      expect(mark.left).toBeGreaterThanOrEqual(line.left - 0.5)
      expect(mark.right).toBeLessThanOrEqual(line.right + 0.5)
    })

    // A volume a little longer than the strip is wide. The rail's travel is
    // a small part of its length, so a pixel of scroll is next to nothing
    // against the length and a visible step along the line, which is as long
    // as the rail is wide. Taken to its end a pixel at a time, the mark has
    // to stand where the rail does after every one of them.
    it(`follows a rail that barely overflows to its end a pixel at a time, ${reading}`, async () => {
      await page.viewport(2400, 900)
      const host = inReader({ ...BARELY, reading, currentIndex: 0 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()

      // THE STATE THIS TEST NAMES: a rail that overflows by a small part of
      // its width, with pixels enough of travel to step through.
      expect(travel(rail)).toBeGreaterThan(8)
      expect(travel(rail)).toBeLessThan(rail.clientWidth / 10)

      /** How far the mark stands from the line's start, and the room it has to move in. */
      const stand = () => {
        const line = lineOf(host).getBoundingClientRect()
        const mark = markBox(host)
        return {
          fromStart:
            later === 1 ? mark.left - line.left : line.right - mark.right,
          room: line.width - mark.width,
        }
      }
      const end = farEnd(rail)
      await scrollRail(rail, end - later * 4)
      for (const short of [3, 2, 1, 0]) {
        await scrollRail(rail, end - later * short)
        const { fromStart, room } = stand()
        const along = Math.abs(rail.scrollLeft) / travel(rail)
        expect
          .soft(fromStart, `${short} px short of the rail's end`)
          .toBeCloseTo(along * room, 0)
      }

      // At the rail's end, flush with the line's.
      expect(Math.abs(rail.scrollLeft)).toBe(travel(rail))
      const { fromStart, room } = stand()
      expect(fromStart).toBeCloseTo(room, 0)
    })

    // The line's own box, and the side its fill grows from. The line is
    // turned round for a work read right to left, and turned about its own
    // middle: it lies where it lay, across the bar, and the fill starts at
    // the end the volume starts at.
    it(`draws the line across the bar and fills it from the end the volume starts at, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0, progress: 0.25 })
      const rail = railOf(host)
      const bar = (
        host.querySelector('.reader-bottom-bar') as HTMLElement
      ).getBoundingClientRect()
      const lineElement = lineOf(host)
      const line = lineElement.getBoundingClientRect()
      const margins = getComputedStyle(lineElement)
      const inset = Number.parseFloat(margins.marginLeft)
      expect(inset).toBeGreaterThan(0)
      expect(Number.parseFloat(margins.marginRight)).toBe(inset)

      expect(line.left).toBeCloseTo(bar.left + inset, 0)
      expect(line.right).toBeCloseTo(bar.right - inset, 0)

      const fill = (
        host.querySelector('.reader-progress-fill') as HTMLElement
      ).getBoundingClientRect()
      expect(fill.width).toBeCloseTo(line.width / 4, 0)
      if (laterSide(rail) === 1) {
        expect(fill.left).toBeCloseTo(line.left, 0)
      } else {
        expect(fill.right).toBeCloseTo(line.right, 0)
      }
    })

    // The bars are hidden and shown with the rail where it stood: the strip
    // that comes back centres on the same tile, so its rail's numbers are
    // the ones the last strip told. The mark that comes back with it is a
    // new element, and has to be drawn as the old one was.
    it(`is drawn again as it was when the bars come back with the rail where it stood, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      await firstReportPast()
      const first = markOf(host)
      const before = markBox(host)
      const stood = railOf(host).scrollLeft
      expect(stood).not.toBe(0)

      rerender(reader({ reading, currentIndex: 60, visible: false }))
      expect(host.querySelector('.reader-bottom-bar')).toBeNull()
      rerender(reader({ reading, currentIndex: 60 }))

      // THE STATE THIS TEST NAMES: a new mark, over a rail that stands where
      // the last one stood.
      expect(markOf(host)).not.toBe(first)
      expect(railOf(host).scrollLeft).toBe(stood)
      const after = markBox(host)
      expect(after.width).toBeCloseTo(before.width, 0)
      expect(after.width).toBeGreaterThan(0)
      expect(after.left).toBeCloseTo(before.left, 0)
    })

    // The rail is moved toward later pages, so its tiles slide toward the
    // side the earlier ones are on, and the mark has to go the other way:
    // toward the side the later pages lie on, as a scrollbar's thumb does.
    //
    // The step is short of what rebuilds the strip's window, so nothing
    // renders, and the observer's first report is let past first: what moves
    // the mark here is the scroll's own telling.
    it(`moves toward the side the later pages lie on as the rail is scrolled to them, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()
      const tile = () =>
        centre(
          (
            rail.querySelector('[data-group="60"]') as HTMLElement
          ).getBoundingClientRect(),
        )

      // THE STATE THIS TEST NAMES: the rail stands in the middle of its
      // travel, with room to go on.
      const step = 300
      expect(Math.abs(rail.scrollLeft)).toBeGreaterThan(step)
      expect(travel(rail) - Math.abs(rail.scrollLeft)).toBeGreaterThan(step)
      const markBefore = markBox(host)
      const tileBefore = tile()
      const window = () =>
        [...rail.querySelectorAll('button')].map((b) => b.dataset.group)
      const windowBefore = window()

      await scrollRail(rail, rail.scrollLeft + later * step)

      // The tiles went toward the earlier side, by the step, and they are the
      // tiles that were there: the window was not rebuilt.
      expect(tile() - tileBefore).toBeCloseTo(-later * step, 0)
      expect(window()).toEqual(windowBefore)
      // The mark went toward the later side: as far along the room its width
      // leaves it on the line as the step is along the rail's travel.
      const line = lineOf(host).getBoundingClientRect()
      const markAfter = markBox(host)
      const moved = centre(markAfter) - centre(markBefore)
      expect.soft(moved * later).toBeGreaterThan(0)
      expect
        .soft(moved)
        .toBeCloseTo(
          later * (step / travel(rail)) * (line.width - markAfter.width),
          0,
        )
    })

    it(`is as wide as the floor on a long volume, and one pixel proud of the line, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host).getBoundingClientRect()

      // THE STATE THIS TEST NAMES: an eighth of the share on show is narrower
      // than the floor, so the floor is what decides.
      const eighth = (rail.clientWidth / rail.scrollWidth) * SHARE * line.width
      expect(eighth).toBeGreaterThan(0)
      expect(eighth).toBeLessThan(FLOOR_PX - 1)

      const mark = markBox(host)
      expect(mark.width).toBeCloseTo(FLOOR_PX, 0)
      expect(line.height).toBe(2)
      expect([mark.top, mark.bottom]).toEqual([line.top - 1, line.bottom + 1])
    })

    // The case above reads the mark's box, and a box is the same whether or
    // not what holds it clips it: a line with `overflow: hidden` leaves the
    // box a pixel proud and draws none of either row. What is under a point
    // is what is drawn there, so this asks what is under a point in each of
    // the two rows.
    //
    // A row is asked for by its own whole number. Between two rows the
    // browser's answer leans upward, and a point half a pixel above a box
    // was read as inside it.
    it(`is drawn in the row above the line and the row below it, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const line = lineOf(host).getBoundingClientRect()
      const mark = markOf(host)
      const x = centre(markBox(host))
      const under = (row: number) => document.elementFromPoint(x, row)
      // THE STATE THIS TEST NAMES: the line lies on whole rows, two of them.
      expect(Number.isInteger(line.top)).toBe(true)
      expect(line.height).toBe(2)

      expect(under(line.top - 1)).toBe(mark)
      expect(under(line.bottom)).toBe(mark)
      // On the line's own two rows as well.
      expect(under(line.top)).toBe(mark)
      expect(under(line.top + 1)).toBe(mark)
      // THE CONTROL: a row further out either way is not the mark's.
      expect(under(line.top - 2)).not.toBe(mark)
      expect(under(line.bottom + 1)).not.toBe(mark)
    })

    // A drag of the strip writes the rail's `scrollLeft` as the wheel does,
    // and the mark has to go with it. A real mouse, held: the mark is read
    // with the button still down.
    it(`follows a real drag of the strip, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()
      /** How far the mark stands from the line's start, and the room it has to move in. */
      const stand = () => {
        const line = lineOf(host).getBoundingClientRect()
        const mark = markBox(host)
        return {
          fromStart:
            later === 1 ? mark.left - line.left : line.right - mark.right,
          room: line.width - mark.width,
        }
      }
      const before = stand()
      const stood = rail.scrollLeft
      const tile = (
        rail.querySelector('[data-group="62"]') as HTMLElement
      ).getBoundingClientRect()
      const x = Math.round(centre(tile))
      const y = Math.round(tile.top + tile.height / 2)
      // Toward the middle of the window, so the pointer stays inside it.
      const by = x > window.innerWidth / 2 ? -600 : 600

      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, by)
        // The `scroll` the last write fires, and the frame the rail's
        // listener takes after it.
        await frame()
        await frame()
      })

      // THE STATE THIS TEST NAMES: the rail has gone with the pointer, and
      // far enough that a mark left where it was would be seen to be.
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(598)
      const { fromStart, room } = stand()
      const along = Math.abs(rail.scrollLeft) / travel(rail)
      expect(Math.abs(along * room - before.fromStart)).toBeGreaterThan(20)

      expect(fromStart).toBeCloseTo(along * room, 0)

      await act(async () => {
        await mouse.release()
        await frame()
      })
    })

    // A drag let go while the pointer is moving carries the rail on, with
    // the reader's animation on as it is here, and the mark has to go on
    // with it. A real mouse again: the mark is read once the rail has come
    // to rest, well on from where the mouse let it go.
    //
    // Each input carries a time the case gives it, 15 px and 15 ms a move,
    // so the pointer is going at 1 px a millisecond however long this
    // machine takes between two of them: `Filmstrip.browser.test.tsx` has
    // the same flick and why.
    it(`follows a glide of the strip to where it comes to rest, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({
        reading,
        currentIndex: 60,
        motion: { animate: true, ms: 80 },
      })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()
      /** How far the mark stands from the line's start, and the room it has to move in. */
      const stand = () => {
        const line = lineOf(host).getBoundingClientRect()
        const mark = markBox(host)
        return {
          fromStart:
            later === 1 ? mark.left - line.left : line.right - mark.right,
          room: line.width - mark.width,
        }
      }
      const tile = (
        rail.querySelector('[data-group="62"]') as HTMLElement
      ).getBoundingClientRect()
      const x = Math.round(centre(tile))
      const y = Math.round(tile.top + tile.height / 2)
      let letGoAt = Number.NaN
      const onUp = () => {
        letGoAt = rail.scrollLeft
      }
      window.addEventListener('pointerup', onUp, true)
      onTestFinished(() => window.removeEventListener('pointerup', onUp, true))

      await act(async () => {
        await mouse.moveTo(x, y)
        mouse.clock(0)
        await mouse.press()
        for (let move = 1; move <= 8; move++) {
          mouse.clock(15 * move)
          await mouse.moveTo(x + 15 * move, y)
        }
        await mouse.release()
        mouse.clock()
      })
      const atTheRelease = stand()
      await untilAtRest()

      // THE STATE THIS TEST NAMES: the rail has gone on since it was let go
      // by the 325 px a glide from that speed carries it, far enough that a
      // mark left where it was then would be seen to be.
      const on = Math.abs(rail.scrollLeft - letGoAt)
      expect(on).toBeGreaterThanOrEqual(325 - 2)
      expect(on).toBeLessThanOrEqual(325 + 2)
      const { fromStart, room } = stand()
      const along = Math.abs(rail.scrollLeft) / travel(rail)
      expect(Math.abs(along * room - atTheRelease.fromStart)).toBeGreaterThan(8)

      expect(fromStart).toBeCloseTo(along * room, 0)
    })

    it(`is as wide as an eighth of the share on show on a short volume, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...SHORT, reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)

      // THE STATE THIS TEST NAMES: the rail overflows, and an eighth of the
      // share on show is wider than the floor, so the eighth is what decides.
      expect(travel(rail)).toBeGreaterThan(0)
      const due = widthDue(rail, line)
      expect(due).toBeGreaterThan(FLOOR_PX + 1)

      expect(markBox(host).width).toBeCloseTo(due, 0)
    })

    // The same bars and the same strip throughout: a longer volume is handed
    // to them and then the short one again, as a change of edition does.
    it(`hides the mark while the strip shows the whole volume, and draws it once it does not, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...FITS, reading, currentIndex: 0 })
      const rail = railOf(host)
      const mark = markOf(host)

      // THE STATE THIS TEST NAMES: tiles are drawn, and none is out of sight.
      expect(rail.querySelectorAll('button')).toHaveLength(3)
      expect(travel(rail)).toBe(0)
      // Hidden, and by that not drawn: the stylesheet's rule for the mark
      // sets nothing that would show a hidden element.
      expect(mark.hidden).toBe(true)
      expect(getComputedStyle(mark).display).toBe('none')

      rerender(reader({ ...LONG, reading, currentIndex: 0 }))
      expect(railOf(host)).toBe(rail)
      expect(markOf(host)).toBe(mark)
      expect(travel(rail)).toBeGreaterThan(0)
      expect(mark.hidden).toBe(false)
      // Drawn where a strip mounted on the long volume draws it: at the
      // line's start, as wide as the floor.
      const line = lineOf(host).getBoundingClientRect()
      const box = markBox(host)
      expect(box.width).toBeCloseTo(FLOOR_PX, 0)
      if (laterSide(rail) === 1) {
        expect(box.left).toBeCloseTo(line.left, 0)
      } else {
        expect(box.right).toBeCloseTo(line.right, 0)
      }

      rerender(reader({ ...FITS, reading, currentIndex: 0 }))
      expect(travel(rail)).toBe(0)
      expect(mark.hidden).toBe(true)
      expect(getComputedStyle(mark).display).toBe('none')
    })

    // The thumbnails the strip asks for never load here, so the late one is
    // made: once the strip has drawn and told, each image it rendered is
    // handed a real picture, square where a tile's floor is a page's upright
    // shape. That is what a slow server does. The tiles take the pictures'
    // width, the rail grows, and nothing scrolls and nothing renders.
    it(`is as wide as the rail's numbers say after thumbnails load late, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...SHORT, reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)
      await firstReportPast()

      const before = rail.scrollWidth
      const stale = markBox(host).width
      expect(stale).toBeCloseTo(widthDue(rail, line), 0)

      const canvas = document.createElement('canvas')
      canvas.width = 160
      canvas.height = 160
      const picture = canvas.toDataURL('image/png')
      const images = [...rail.querySelectorAll('img')]
      expect(images.length).toBeGreaterThan(0)
      await act(async () => {
        const loaded = images.map(
          (img) =>
            new Promise((resolve) =>
              img.addEventListener('load', resolve, { once: true }),
            ),
        )
        for (const img of images) {
          // Every one the window rendered, the ones out of sight too.
          img.loading = 'eager'
          img.src = picture
        }
        await Promise.all(loaded)
        // The frame the strip takes after a load. Asked for after the strip's.
        await frame()
      })

      // THE STATE THIS TEST NAMES: the rail is longer than it was, still
      // where it stood, and by enough that a mark left as it was would show.
      expect(rail.scrollWidth).toBeGreaterThan(before)
      expect(rail.scrollLeft).toBe(0)
      const due = widthDue(rail, line)
      expect(stale - due).toBeGreaterThan(5)

      expect(markBox(host).width).toBeCloseTo(due, 0)
    })

    it(`is as wide as the rail's numbers say after the window is made narrower, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...SHORT, reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)
      await firstReportPast()
      const wide = rail.clientWidth
      const shareBefore = rail.clientWidth / rail.scrollWidth
      expect(markBox(host).width).toBeCloseTo(widthDue(rail, line), 0)

      // Waited for by what it does and not by time: first the rail is laid
      // out narrower, then two frames, since the observer is called in the
      // frame that lays the narrower window out, after that frame's own
      // callbacks have run.
      await act(async () => {
        await page.viewport(1000, 900)
        while (rail.clientWidth >= wide) await frame()
        await frame()
        await frame()
      })

      // THE STATE THIS TEST NAMES: a narrower rail over the same tiles, still
      // overflowing, showing a share of them a stale mark would misstate.
      expect(rail.clientWidth).toBeLessThan(wide)
      expect(travel(rail)).toBeGreaterThan(0)
      expect(shareBefore - rail.clientWidth / rail.scrollWidth).toBeGreaterThan(
        0.05,
      )
      const due = widthDue(rail, line)
      expect(due).toBeGreaterThan(FLOOR_PX + 1)

      expect(markBox(host).width).toBeCloseTo(due, 0)
    })
  }

  // The class and the token meeting: `index.css.test.ts` holds the token's
  // value and `classNames.test.ts` that the class has a rule.
  it('is drawn in the reader’s mark colour, which lets the line through', async () => {
    await page.viewport(1440, 900)
    const host = inReader({ currentIndex: 0 })
    const mark = markOf(host)
    expect(mark.hidden).toBe(false)
    expect(getComputedStyle(mark).backgroundColor).toBe(
      'rgba(236, 228, 212, 0.5)',
    )
  })
})

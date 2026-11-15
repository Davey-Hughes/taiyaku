/**
 * What the browser files about the reader's bars share (O216, O286): the
 * volumes they are handed, the bars mounted in a reader that fills the
 * window, the measures of the rail, the progress line and its mark, and a
 * flick of the strip with a real mouse.
 *
 * Nothing answers `/api` in the browser project, so the thumbnails the strip
 * asks for never load and every tile stands at its floor.
 */
import { act } from 'react'
import { expect, onTestFinished, vi } from 'vitest'
import { Chrome, type ChromeProps } from '../../src/Chrome'
import type { Group } from '../../src/pairing'
import type { Position } from '../../src/resolver'
import { mount } from './mount'
import { mouse } from './mouse'

/** A volume of `pages` single pages, one tile each. */
export function volume(pages: number): { slots: Position[]; groups: Group[] } {
  const slots: Position[] = Array.from({ length: pages }, (_, i) => ({
    page: i,
  }))
  return { slots, groups: slots.map((_, i) => ({ slots: [i] })) }
}

// Lengths of volume, for the things the rail's length decides. Each case
// that leans on one asserts it of the rail it measures, so a change to the
// tiles' size that moved a volume out of its kind fails there.
/** Long enough that the share on show is narrower than `MARK_MIN_PX` at 1440 px, so that bound decides the mark's width. */
export const VERY_LONG = volume(400)
/** Long enough that the share on show lies between the bounds at 1440 px and at 1000, so neither bound decides. */
export const LONG = volume(120)
/** Long enough to overflow the rail, and short enough that the share on show is wider than `MARK_MAX_PX`, so that bound decides. */
export const SHORT = volume(24)
/** Short enough that the rail holds all of it: no mark is drawn. */
export const FITS = volume(3)
/** A little longer than the rail is wide, in the wide window its case sets: the travel is a small part of the length. */
export const BARELY = volume(21)

/** The bounds `Chrome.tsx` holds the mark's width between, as the spec gives them. */
export const MARK_MIN_PX = 64
export const MARK_MAX_PX = 160

export function props(overrides: Partial<ChromeProps> = {}): ChromeProps {
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
export function reader(overrides: Partial<ChromeProps>) {
  return (
    <div className="reader" style={{ position: 'fixed', inset: 0 }}>
      <Chrome {...props(overrides)} />
    </div>
  )
}

export function inReader(overrides: Partial<ChromeProps>) {
  return mount(reader(overrides))
}

export const railOf = (host: HTMLElement) =>
  host.querySelector('.reader-strip-rail') as HTMLElement
export const lineOf = (host: HTMLElement) =>
  host.querySelector('.reader-progress') as HTMLElement
/** The mark's element, which is there whenever the bars are, drawn or hidden. */
export const markOf = (host: HTMLElement) =>
  host.querySelector('[data-testid="strip-shown"]') as HTMLElement

/** The mark's box. Throws where the mark is hidden, so a case cannot measure nothing. */
export function markBox(host: HTMLElement): DOMRect {
  const mark = markOf(host)
  if (mark.hidden) throw new Error('the mark is hidden')
  return mark.getBoundingClientRect()
}

export const centre = (box: DOMRect) => box.left + box.width / 2

export const frame = () =>
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
export async function firstReportPast() {
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
export async function untilAtRest() {
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
export const travel = (rail: HTMLElement) => rail.scrollWidth - rail.clientWidth

/** The share of the rail's length on show, as a width on the line, before either bound. */
export const onShowPx = (rail: HTMLElement, line: HTMLElement) =>
  (rail.clientWidth / rail.scrollWidth) * line.getBoundingClientRect().width

/**
 * The width the spec gives the mark for the rail as it stands: the share of
 * its length on show, as a share of the line, held between `MARK_MIN_PX` and
 * `MARK_MAX_PX`.
 */
export function widthDue(rail: HTMLElement, line: HTMLElement): number {
  return Math.min(Math.max(MARK_MIN_PX, onShowPx(rail, line)), MARK_MAX_PX)
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
export async function scrollRail(rail: HTMLElement, to: number) {
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
 * Thumbnails that arrive late, as from a slow server: every image the rail
 * rendered, the ones out of sight too, is handed a real picture, square
 * where a tile's floor is a page's upright shape. The tiles take the
 * pictures' width and the rail grows; the loading itself scrolls nothing
 * and renders nothing. Waits for every load and for the frame the strip
 * takes after one, which is asked for after the strip's.
 */
export async function loadThumbnailsLate(rail: HTMLElement) {
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
      img.loading = 'eager'
      img.src = picture
    }
    await Promise.all(loaded)
    await frame()
  })
}

/**
 * Which side of an earlier tile a later one stands on: `1` for the right,
 * `-1` for the left. Asked of two tiles' boxes rather than of the reading
 * direction, so no case here decides for itself what `rtl` means.
 */
export function laterSide(rail: HTMLElement): 1 | -1 {
  const [first, second] = [...rail.querySelectorAll('button')].map((b) =>
    centre(b.getBoundingClientRect()),
  )
  if (first === undefined || second === undefined || first === second) {
    throw new Error('two tiles side by side are needed to tell the sides apart')
  }
  return second > first ? 1 : -1
}

/** The rail's far end, as the browser signs it for this rail. */
export const farEnd = (rail: HTMLElement) => laterSide(rail) * travel(rail)

/** The band over the progress line that a mouse takes hold of it by (O286). */
export const gripOf = (host: HTMLElement) =>
  host.querySelector('.reader-progress-grip') as HTMLElement

/** A whole-pixel `y` on the progress line, the lower of its two rows. */
export function lineRow(host: HTMLElement): number {
  const line = lineOf(host).getBoundingClientRect()
  return Math.floor(line.top + line.height / 2)
}

/**
 * The window `x` that lies `d` px from the line's start as the reader sees
 * it: its left edge, or its right where the line is mirrored. Which edge is
 * asked of the tiles, as `laterSide` asks it.
 */
export function atAlong(host: HTMLElement, d: number): number {
  const line = lineOf(host).getBoundingClientRect()
  return laterSide(railOf(host)) === 1 ? line.left + d : line.right - d
}

/** The mark's drawn start, measured from the line's start as `atAlong` measures, and its width. */
export function markFrom(host: HTMLElement): { start: number; width: number } {
  const line = lineOf(host).getBoundingClientRect()
  const mark = markBox(host)
  return {
    start:
      laterSide(railOf(host)) === 1
        ? mark.left - line.left
        : line.right - mark.right,
    width: mark.width,
  }
}

/**
 * The middle of tile `group`'s box, to the whole pixel, where a press on it
 * lands. Throws where that tile is not rendered.
 */
export function tileMiddle(
  rail: HTMLElement,
  group: number,
): { x: number; y: number } {
  const tile = rail.querySelector<HTMLElement>(`[data-group="${group}"]`)
  if (!tile) throw new Error(`tile ${group} is not rendered`)
  const box = tile.getBoundingClientRect()
  return {
    x: Math.round(centre(box)),
    y: Math.round(box.top + box.height / 2),
  }
}

/**
 * A flick from (`x`, `y`) toward `side`, `1` for the right and `-1` for the
 * left: 15 px and 15 ms a move for eight moves, let go with the last, which
 * is a pointer going at 1 px a millisecond however long this machine takes
 * between two of them (`mouse.clock`). `Filmstrip.browser.test.tsx` has the
 * same flick and why.
 */
export async function flick(x: number, y: number, side: 1 | -1) {
  await act(async () => {
    await mouse.moveTo(x, y)
    mouse.clock(0)
    await mouse.press()
    for (let move = 1; move <= 8; move++) {
      mouse.clock(15 * move)
      await mouse.moveTo(x + side * 15 * move, y)
    }
    await mouse.release()
    mouse.clock()
  })
}

/** Where the rail stood at each `pointerup` since this was called, the last one last. */
export function rememberLetGo(rail: HTMLElement): number[] {
  const at: number[] = []
  const onUp = () => at.push(rail.scrollLeft)
  window.addEventListener('pointerup', onUp, true)
  onTestFinished(() => window.removeEventListener('pointerup', onUp, true))
  return at
}

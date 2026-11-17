import { type RefObject, useLayoutEffect, useRef } from 'react'
import {
  frameTime,
  mayReseat,
  NEAR_PX,
  RESEAT_MS,
  reseatEased,
  rowLaid,
  turnSnaps,
  uprightNow,
} from './reseatTiming.ts'

/**
 * A `px` length as a number, and `NaN` for any other text: a custom
 * property the stylesheet has not registered as a length computes to the
 * text it was written as (a `rem`, a `clamp()`), which no column is counted
 * by.
 */
export function pxOf(value: string): number {
  const text = value.trim()
  return text.endsWith('px') ? Number.parseFloat(text) : Number.NaN
}

/**
 * How many columns `repeat(auto-fill, …)` lays across `width`, `gap` apart:
 * as many as fit `track` wide, and one at the least, which the fill
 * layout's `min(track, 100%)` narrows to `width` where `track` is wider.
 * Both layouts count their columns this way, so both helpers below share it.
 */
export function columnsIn(width: number, track: number, gap: number): number {
  return Math.max(1, Math.floor((width + gap) / (track + gap)))
}

/**
 * The gap the grid lays its columns apart by in the exact layout: the
 * columns are `track` wide and `justify-content` spreads the spare room into
 * the gaps. One column has no gap to spread into, and keeps the least.
 */
export function spreadGap(width: number, track: number, gap: number): number {
  const n = columnsIn(width, track, gap)
  return n > 1 ? (width - n * track) / (n - 1) : gap
}

/**
 * A column's width in the fill layout (`--tile-fill`), where the grid's
 * `minmax(min(track, 100%), 1fr)` columns share the row at the least gap:
 * a lone column is the whole `width`, also where that is under `track`.
 */
export function filledColumn(
  width: number,
  track: number,
  gap: number,
): number {
  const n = columnsIn(width, track, gap)
  return (width - (n - 1) * gap) / n
}

const GAP = '--continue-gap'
const COLUMN = '--continue-col'

/**
 * Where a list was last scrolled to, for the size it was laid at and the
 * number of cards it held: at its end, or the card at its start edge as
 * steps from the first card, the fraction how far past that card's edge.
 * `left`, `step` and `travel` are the row as last seen.
 */
type Place = {
  ul: HTMLUListElement
  size: number
  left: number
  step: number
  travel: number
  count: number
  atEnd: boolean
  first: number
}

/**
 * The column and gap the row was last laid at, and what it was laid for:
 * how many of the grid's columns, in which layout, the view upright or not.
 */
type Laid = {
  ul: HTMLUListElement
  column: number
  gap: number
  columns: number
  fill: boolean
  upright: boolean | undefined
}

/**
 * A reseat under way: the column and gap drawn as it began, and when; no
 * time yet where it was begun between frames and none has drawn it.
 */
type Slide = { column: number; gap: number; start: number | undefined }

/**
 * How much taller than wide a card's cover is: `aspect-ratio: 2 / 3` on
 * `.continue img` and its placeholder. The cover heads its card, its column
 * wide, so its foot follows from the row's box with no cover read.
 */
export const COVER_TALL = 3 / 2

/**
 * Whether a row drawn at `box`, its covers' foot `foot` px down the view,
 * slides with the covers: on screen or within `NEAR_PX`, where a cover would
 * move, and in the fill layout only where its covers are not wholly above
 * the view. A filled card's height follows its column. With its covers above
 * the view the browser anchors the page below them, so a slide would have it
 * correct the scroll each frame rather than once; with a cover in view it
 * anchors to that, and a row laid at once would push the grid under covers
 * just starting their glide.
 */
const slidesFrom = (box: DOMRect, fill: boolean, foot: number) =>
  (fill ? foot > 0 : box.bottom > -NEAR_PX) &&
  box.top < window.innerHeight + NEAR_PX

/**
 * Where a row seen `left` along its `travel`, its cards `step` apart,
 * stands. A row that fits is never at its end: once it overflows again it
 * would be sent to its end.
 */
function seenAt(left: number, step: number, travel: number) {
  return {
    atEnd: travel > 1 && left >= travel - 1,
    first: step > 0 ? left / step : 0,
  }
}

/**
 * Lays the Continue row's list from outside React, so a resize or a scroll
 * renders nothing, and before paint, so no frame shows it unlaid: each card
 * over a grid column, also once the row overflows, by the exact layout's
 * spread gap or the fill layout's column, or at the least gap where the
 * tokens are not px lengths. `data-more-before` and `data-more-after` mark a
 * side with cards past it, at most once a frame. A cover-size change or a
 * resize keeps the row's place, at its end or the same card at its start
 * edge as far as the row reaches; a change in the number of cards drops it.
 */
export function useContinueRow(
  list: RefObject<HTMLUListElement | null>,
  drawn: boolean,
  coverSize: number,
  count: number,
): void {
  const place = useRef<Place | null>(null)
  // Kept across this effect's runs, so a cover-size change mid-reseat goes
  // on from where the row is drawn.
  const laidAs = useRef<Laid | null>(null)
  const slide = useRef<Slide | null>(null)
  // `coverSize` and `count` re-run this: a size change measures with the
  // new tokens in its commit rather than waiting for the observer, and a
  // card more or fewer moves the `scrollWidth` the scroll marks read,
  // which the observer does not see.
  useLayoutEffect(() => {
    const ul = list.current
    if (!drawn || ul === null) return
    // Where there is no observer, nothing lays the row again at a new
    // width, so the row as it stands is all a mark can read.
    const relaid = typeof ResizeObserver !== 'undefined'
    // The list's width the row was last laid for.
    let laidAt = 0
    // `framed` where this runs in a frame (its animation frame callbacks or
    // its resize observers), not between frames as the effect's commit does.
    const measure = (framed: boolean) => {
      const style = getComputedStyle(ul)
      // The tokens, not the list's own track and gap, which this sets.
      const track = pxOf(style.getPropertyValue('--tile-min'))
      const gap = pxOf(style.getPropertyValue('--tile-gap'))
      const box = ul.getBoundingClientRect()
      laidAt = box.width
      const width =
        laidAt -
        Number.parseFloat(style.paddingLeft) -
        Number.parseFloat(style.paddingRight) -
        Number.parseFloat(style.borderLeftWidth) -
        Number.parseFloat(style.borderRightWidth)
      // A gap read as zero would count more columns than the grid lays.
      if (!(track > 0 && gap >= 0 && width > 0)) {
        ul.style.removeProperty(GAP)
        ul.style.removeProperty(COLUMN)
        laidAs.current = null
        slide.current = null
        return
      }
      const fill = Number(style.getPropertyValue('--tile-fill').trim()) === 1
      // Read after the box, whose read lays the page out at a new size
      // first, so this is that size's orientation.
      const upright = uprightNow()
      const columns = columnsIn(width, track, gap)
      const target = fill
        ? { column: filledColumn(width, track, gap), gap }
        : { column: track, gap: spreadGap(width, track, gap) }
      const time = frameTime()
      const last = laidAs.current?.ul === ul ? laidAs.current : null
      // Where the grid's covers move (`reseat.ts`), the cards over them do
      // too: the row moves from the column and gap it is drawn at, on the
      // covers' clock and curve, towards the live target, so it ends on
      // the grid's columns wherever the window has gone meanwhile. Across
      // the phone layout's edge, where `--tile-fill` turns, and at a
      // phone's turn while `TURN_RESEATS` says so, the covers snap, and so
      // does the row; and off screen, or filled with its covers above the
      // view (`slidesFrom`).
      if (
        last !== null &&
        (last.fill !== fill || turnSnaps(last.upright, upright))
      ) {
        slide.current = null
      } else if (last !== null && last.columns !== columns) {
        const foot =
          box.top +
          Number.parseFloat(style.borderTopWidth) +
          Number.parseFloat(style.paddingTop) +
          last.column * COVER_TALL
        slide.current =
          mayReseat() && slidesFrom(box, fill, foot)
            ? {
                column: last.column,
                gap: last.gap,
                start: framed ? time : undefined,
              }
            : null
      }
      let drawn = target
      const under = slide.current
      if (under !== null) {
        // A size change is measured in its commit, between frames, where the
        // timeline still reads the last frame's time, while the covers'
        // reseats start in the frame that lays them: a slide stamped in the
        // commit would run a frame ahead of them. It starts at the first
        // frame that draws it, and stands where it began until then.
        if (under.start === undefined && framed) under.start = time
        const progress =
          under.start === undefined ? 0 : (time - under.start) / RESEAT_MS
        if (progress >= 1) {
          slide.current = null
        } else {
          const along = reseatEased(Math.max(0, progress))
          drawn = {
            column: under.column + (target.column - under.column) * along,
            gap: under.gap + (target.gap - under.gap) * along,
          }
        }
      }
      // Mid-reseat the row is laid by both. At rest each layout removes the
      // other's property, so that nothing set for it is left in force once
      // the layout turns.
      if (slide.current !== null) {
        ul.style.setProperty(COLUMN, `${drawn.column}px`)
        ul.style.setProperty(GAP, `${drawn.gap}px`)
      } else if (fill) {
        ul.style.setProperty(COLUMN, `${drawn.column}px`)
        ul.style.removeProperty(GAP)
      } else {
        ul.style.setProperty(GAP, `${drawn.gap}px`)
        ul.style.removeProperty(COLUMN)
      }
      laidAs.current = { ul, ...drawn, columns, fill, upright }
    }
    const stepOf = () => {
      const [one, two] = ul.children
      return two === undefined
        ? 0
        : two.getBoundingClientRect().left - one.getBoundingClientRect().left
    }
    const mark = () => {
      const left = ul.scrollLeft
      const end = ul.scrollWidth - ul.clientWidth
      ul.toggleAttribute('data-more-before', left > 1)
      ul.toggleAttribute('data-more-after', left < end - 1)
      const was = place.current
      const step = stepOf()
      // At a width the row is not yet laid for, it reads with the old gap
      // or column, clamped where that row now ends sooner; the observer
      // lays it and puts the place back before this frame is painted.
      const unlaid = relaid && ul.getBoundingClientRect().width !== laidAt
      // Kept until the row is scrolled, its cards move or their number
      // changes: where a size change or a resize put it reads back
      // rounded, or clamped where the row now fits, so working it out
      // again would drift over a drag's steps or lose the place a drag
      // back would return to. A card more or fewer moves no pitch, and
      // moves `scrollLeft` only where it clamps a row near its end, so
      // the count is what says the place is stale.
      if (
        was?.ul === ul &&
        was.count === count &&
        (unlaid || (was.left === left && was.step === step))
      ) {
        return
      }
      place.current = {
        ul,
        size: coverSize,
        left,
        step,
        travel: end,
        count,
        ...seenAt(left, step, end),
      }
    }
    // Puts the row back where `was` stood, at the pitch it is laid at now,
    // and keeps the place as laid now, so the mark after this keeps it too.
    const restore = (was: Place) => {
      ul.scrollLeft = was.atEnd
        ? ul.scrollWidth - ul.clientWidth
        : was.first * stepOf()
      place.current = {
        ...was,
        size: coverSize,
        left: ul.scrollLeft,
        step: stepOf(),
        travel: ul.scrollWidth - ul.clientWidth,
        count,
      }
    }
    // A reseat is laid frame by frame rather than animated: the row is a
    // scroller whose place `mark` and `restore` read from its layout, which
    // a transform on the cards would leave behind (and stretch the scroll
    // range with), so its column and gap themselves move, and each frame
    // puts the place back as a resize does.
    let sliding = 0
    // Lays the row and keeps its place, as a resize does, and says so where
    // its column, and with it its height, changed. Run only in a frame.
    const relay = () => {
      const column = laidAs.current?.column
      const kept = place.current
      measure(true)
      if (kept?.ul === ul) restore(kept)
      mark()
      if (laidAs.current?.column !== column) rowLaid()
    }
    const lay = () => {
      // A scroll since the last frame is the place to keep: its own mark
      // runs after this one in the frame.
      mark()
      relay()
      slideOn()
    }
    const slideOn = () => {
      if (slide.current === null || sliding !== 0) return
      sliding = requestAnimationFrame(() => {
        sliding = 0
        if (slide.current === null) return
        lay()
      })
    }
    let frame = 0
    const scrolled = () => {
      if (frame !== 0) return
      frame = requestAnimationFrame(() => {
        frame = 0
        mark()
      })
    }
    const laid = () =>
      `${ul.style.getPropertyValue(GAP)} ${ul.style.getPropertyValue(COLUMN)}`
    let again = 0
    const resized = (_: unknown, observer: ResizeObserver) => {
      const was = laid()
      // The last mark's place, not the row's own: the new width has laid
      // the row with the old gap or column, which may have clamped it.
      relay()
      slideOn()
      if (laid() === was) return
      // A column set here changes the cards' height, a resize the observer
      // cannot report in this frame, which it says with an error event.
      // Watched again next frame, its first report carries the size then.
      observer.unobserve(ul)
      again = requestAnimationFrame(() => {
        again = 0
        observer.observe(ul)
      })
    }
    // The last mark's place, not the row's own: measuring lays the row
    // with the new size and the old gap, which may clamp it.
    const was = place.current
    measure(false)
    // A place kept at another count stood among other cards, carried
    // through any put-back since, so a size change in the same commit as
    // the count keeps the place as last seen instead.
    if (was !== null && was.ul === ul && was.size !== coverSize) {
      restore(
        was.count === count
          ? was
          : { ...was, ...seenAt(was.left, was.step, was.travel) },
      )
    }
    mark()
    slideOn()
    ul.addEventListener('scroll', scrolled, { passive: true })
    // The list's width changes where the layout turns, since the sidebar's
    // column comes and goes there, so this is also what lays the row for
    // the other layout.
    const observer = relaid ? new ResizeObserver(resized) : undefined
    observer?.observe(ul)
    return () => {
      ul.removeEventListener('scroll', scrolled)
      cancelAnimationFrame(frame)
      cancelAnimationFrame(again)
      cancelAnimationFrame(sliding)
      observer?.disconnect()
    }
  }, [list, drawn, coverSize, count])
}

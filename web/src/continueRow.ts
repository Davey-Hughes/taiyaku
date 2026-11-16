import { type RefObject, useLayoutEffect, useRef } from 'react'

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
 * How many columns `repeat(auto-fill, …)` lays across `width`, each at
 * least `track` wide and `gap` apart: one at the least. Both layouts count
 * their columns this way, so both helpers below share it.
 */
function columnsIn(width: number, track: number, gap: number): number {
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
 * `minmax(track, 1fr)` columns share the row at the least gap.
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
 * Where a list was last scrolled to, for the size it was laid at: at its
 * end, or the card at its start edge as steps from the first card, the
 * fraction how far past that card's edge.
 */
type Place = {
  ul: HTMLUListElement
  size: number
  left: number
  step: number
  atEnd: boolean
  first: number
}

/**
 * Lays the Continue row's list from outside React, so a resize or a scroll
 * renders nothing, and before paint, so no frame shows it unlaid. A card
 * sits over a grid column, also once the row overflows, by the exact
 * layout's spread gap or the fill layout's column; where the tokens are not
 * px lengths it sets neither, and the row keeps the least gap.
 * `data-more-before` and `data-more-after` mark a side with cards past it,
 * at most once a frame. A cover-size change keeps the row's place: at its
 * end, or the same card at its start edge as far as the row reaches.
 */
export function useContinueRow(
  list: RefObject<HTMLUListElement | null>,
  drawn: boolean,
  coverSize: number,
  count: number,
): void {
  const place = useRef<Place | null>(null)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `coverSize` and `count` are triggers — on a size change the commit measures with the new tokens rather than waiting for the observer, and a card more or fewer moves the `scrollWidth` the scroll marks read, which the observer does not see.
  useLayoutEffect(() => {
    const ul = list.current
    if (!drawn || ul === null) return
    const measure = () => {
      const style = getComputedStyle(ul)
      // The tokens, not the list's own track and gap, which this sets.
      const track = pxOf(style.getPropertyValue('--tile-min'))
      const gap = pxOf(style.getPropertyValue('--tile-gap'))
      const width =
        ul.getBoundingClientRect().width -
        Number.parseFloat(style.paddingLeft) -
        Number.parseFloat(style.paddingRight) -
        Number.parseFloat(style.borderLeftWidth) -
        Number.parseFloat(style.borderRightWidth)
      // A gap read as zero would count more columns than the grid lays.
      if (!(track > 0 && gap >= 0 && width > 0)) {
        ul.style.removeProperty(GAP)
        ul.style.removeProperty(COLUMN)
        return
      }
      // Each layout removes the other's property, so that nothing set
      // for it is left in force once the layout turns.
      if (Number(style.getPropertyValue('--tile-fill').trim()) === 1) {
        ul.style.setProperty(COLUMN, `${filledColumn(width, track, gap)}px`)
        ul.style.removeProperty(GAP)
      } else {
        ul.style.setProperty(GAP, `${spreadGap(width, track, gap)}px`)
        ul.style.removeProperty(COLUMN)
      }
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
      // Kept until the row is scrolled or its cards move: where a size
      // change put it reads back rounded, or clamped where the row now
      // fits, so working it out again would drift over a drag's steps or
      // lose the place a drag back would return to.
      if (was?.ul === ul && was.left === left && was.step === step) return
      place.current = {
        ul,
        size: coverSize,
        left,
        step,
        atEnd: end > 1 && left >= end - 1,
        first: step > 0 ? left / step : 0,
      }
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
      measure()
      mark()
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
    measure()
    if (was !== null && was.ul === ul && was.size !== coverSize) {
      ul.scrollLeft = was.atEnd
        ? ul.scrollWidth - ul.clientWidth
        : was.first * stepOf()
      place.current = {
        ...was,
        size: coverSize,
        left: ul.scrollLeft,
        step: stepOf(),
      }
    }
    mark()
    ul.addEventListener('scroll', scrolled, { passive: true })
    // The list's width changes where the layout turns, since the sidebar's
    // column comes and goes there, so this is also what lays the row for
    // the other layout.
    const observer =
      typeof ResizeObserver === 'undefined'
        ? undefined
        : new ResizeObserver(resized)
    observer?.observe(ul)
    return () => {
      ul.removeEventListener('scroll', scrolled)
      cancelAnimationFrame(frame)
      cancelAnimationFrame(again)
      observer?.disconnect()
    }
  }, [list, drawn, coverSize, count])
}

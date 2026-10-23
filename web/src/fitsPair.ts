/**
 * Whether a list and the page opened from it fit side by side: spec §3.
 *
 * **Measured against a probe, not asked of a container query.** The
 * breakpoint is the list's least column, the gap and the page's measure,
 * added up from their tokens, and a container query cannot read `var()`.
 * The probe is an element sized from those tokens, so its width is the
 * breakpoint, in whatever the tokens and the root font size make of it now.
 *
 * **Narrow until measured.** A probe that measures nothing (not laid out
 * yet, or jsdom, which lays nothing out) never fits, and neither does a
 * browser without `ResizeObserver`, which could not say when to measure
 * again. Narrow is the layout that draws everything: the page alone, with
 * its way back.
 */
import { type RefObject, useLayoutEffect, useState } from 'react'

/**
 * True while `split`'s width inside its border is at least `probe`'s. Read
 * before the first paint, so a wide window never draws a narrow frame, and
 * again whenever either element is resized: the window, the sidebar, or the
 * root font size the tokens are in.
 */
export function useFitsPair(
  split: RefObject<HTMLElement | null>,
  probe: RefObject<HTMLElement | null>,
): boolean {
  const [fits, setFits] = useState(false)
  useLayoutEffect(() => {
    const room = split.current
    const need = probe.current
    if (
      room === null ||
      need === null ||
      typeof ResizeObserver === 'undefined'
    ) {
      return
    }
    const measure = () => {
      const width = need.getBoundingClientRect().width
      setFits(width > 0 && room.clientWidth >= width)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(room)
    observer.observe(need)
    return () => observer.disconnect()
  }, [split, probe])
  return fits
}

/**
 * Which items of a wrapping row end a line, for the separators drawn after
 * them: a two-line row's cells (`admin.css`, `settings.css`) and a
 * directory's facts under its title. Each separator is drawn after the item
 * it follows, so no line starts with one; an item that ends a line is marked
 * `data-ends-line`, and its separator is hidden there, so no line ends with
 * one either. Only a browser knows where a line breaks, so it is measured.
 *
 * **Marking cannot move a break.** A hidden separator keeps its box
 * (`visibility`), so the line it ends is as wide as before and nothing
 * re-wraps under the mark.
 *
 * Measured after every draw of the caller, whenever the element holding the
 * items is resized (a narrower window, a list beside an opened page), and
 * once the fonts arrive.
 */
import { useCallback, useEffect, useLayoutEffect, useState } from 'react'

/** A Configuration table's cells after the name's, which a two-line row wraps. */
export const ROW_CELLS = 'tbody td:not(.lead)'

/**
 * A ref for the element holding the items, and the marking of each item
 * `items` finds under it: an item ends a line where it is the last of its
 * siblings that `items` matches, or where the next of them starts below it.
 */
export function useLineEnds<E extends HTMLElement>(
  items: string,
): (element: E | null) => void {
  const [scope, setScope] = useState<E | null>(null)

  const mark = useCallback(() => {
    if (scope === null) return
    for (const item of scope.querySelectorAll<HTMLElement>(items)) {
      const at = item.getBoundingClientRect()
      // Not laid out (hidden, or a document with no layout): there is no
      // line to end, and a mark would be a guess.
      if (at.height === 0) continue
      let next = item.nextElementSibling
      while (next !== null && !next.matches(items)) {
        next = next.nextElementSibling
      }
      const ends =
        next === null || next.getBoundingClientRect().top >= at.bottom - 0.5
      item.toggleAttribute('data-ends-line', ends)
    }
  }, [scope, items])

  // No dependency list: every draw can change what the items say, and so
  // where their lines break.
  useLayoutEffect(() => {
    mark()
  })

  useEffect(() => {
    if (scope === null) return
    let live = true
    document.fonts?.ready.then(() => {
      if (live) mark()
    })
    if (typeof ResizeObserver === 'undefined') {
      return () => {
        live = false
      }
    }
    const observer = new ResizeObserver(mark)
    observer.observe(scope)
    return () => {
      live = false
      observer.disconnect()
    }
  }, [scope, mark])

  return setScope
}

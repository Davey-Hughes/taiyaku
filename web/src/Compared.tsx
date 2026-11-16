import type { ReactNode } from 'react'
import type { Direction } from './modules/direction'
import { type FitPrefs, seamStyle } from './modules/fit'

/**
 * The page reader's compare layout: the page read beside another edition's
 * for the same place, set as a spread's two pages are, meeting at the seam.
 * The page read takes the cell its reading order puts first, so a
 * right-to-left work reads it on the right.
 */
export function Compared({
  reading,
  fit,
  children,
  companion,
}: {
  reading: Direction
  fit: FitPrefs
  children: ReactNode
  companion: ReactNode
}) {
  const [left, right] =
    reading === 'rtl' ? [companion, children] : [children, companion]
  return (
    <div className="compared">
      <div style={seamStyle(fit, 'first')}>{left}</div>
      <div style={seamStyle(fit, 'second')}>{right}</div>
    </div>
  )
}

/**
 * The bottom bar's pieces, shared by the page reader's `Chrome` and the book
 * reader: the band itself, the progress line and the readout row.
 */
import type { CSSProperties } from 'react'

// The bottom bar (spec 2026-09-17 §8.4): one full-width band docked to the
// reader's bottom edge — the strip, the progress line and the readout row,
// padded by the safe areas.
//
// `position: absolute` against `App.tsx`'s root, which carries `position:
// relative` for exactly this, rather than a flex child of the page-area
// column: an absolutely positioned box is removed from that flow entirely, so
// mounting or unmounting it cannot change the page area's size — the one
// property this component exists to guarantee. `zIndex: 3` stands it over the
// page's own layers (`Annotations`' overlay is at 2); the docked panels share
// the figure and dock clear of it (`--panel-bottom`, `styles/reader.css`).
export const bottomBarStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  bottom: 0,
  zIndex: 3,
  paddingLeft: 'var(--safe-left)',
  paddingRight: 'var(--safe-right)',
  paddingBottom: 'var(--safe-bottom)',
}

// How far through the edition, as a line across the bar: `readoutLines`'
// `progress`, the fraction the readout's percentage prints — the two agree
// except where the slot count has gone backwards, which `progress` clamps at 1
// and `positionLine` does not (see `ReadoutLines.progress`).
export const progressStyle: CSSProperties = {
  height: 'var(--progress-height)',
  margin: '0 var(--space-3)',
  // So the mark is placed against the line and not against the bar: the
  // percentages in its `left` and its width are of the line's own width.
  position: 'relative',
}

// The readout row, the bar's last line. Its fixed `height` holds whatever its
// text does; `readoutTextStyle`'s `nowrap` is what keeps a second line from
// spilling out of it. `readoutRow` (`modules/readouts.ts`) is the other half
// of this: a fixed segment order, so which readouts are enabled or have
// something to say right now changes the row's *length*, never the order
// pieces appear in.
export const readoutStyle: CSSProperties = {
  height: 'var(--readout-height)',
  display: 'flex',
  alignItems: 'center',
  gap: 4,
  padding: '0 12px',
}

// The row's text. The truncation sits on this span, not the row, because
// `text-overflow` does nothing on a flex container (O141). `nowrap` keeps the
// row one line high, since wrapped text grows past a fixed `height`, and what
// runs wider than the bar ends in "…". `overflow: 'hidden'` also makes the
// span a scroll container, which as a flex item has no automatic minimum
// width, so it shrinks to the row's padding. Unstretched, it is as wide as its
// text, so short text stays centred.
export const readoutTextStyle: CSSProperties = {
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
}

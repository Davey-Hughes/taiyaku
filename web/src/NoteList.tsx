import type { CSSProperties } from 'react'
import { isPoint, type NoteHere } from './notes'
import { noteColour } from './palette'

/**
 * The notes on the page being read, as a list beside it or under it.
 *
 * **Why a second place to read a note at all.** A note's words used to live
 * only inside the region they marked, which fails twice: a small region holds
 * a line or two before clipping, and a *point* has no area to hold anything.
 * The list is where the words always fit, and the number on each row is the
 * number on its marker, so reading one here tells you which mark on the page
 * it belongs to.
 *
 * Numbered by position in `here`, which is the order the markers themselves
 * are drawn in — so the list reads down the page, and the same note keeps the
 * same number as long as the page does.
 *
 * Placement is `notes.ts`'s `listPlacement`: beside the page where the window
 * is wide enough for the margin to hold it, under the page where it is not,
 * and whichever a reader asks for in settings. This component only lays out
 * the rows; where it is mounted is `App.tsx`'s business, which is why there is
 * no positioning in these styles beyond filling what it is given.
 */
const sideStyle: CSSProperties = {
  position: 'absolute',
  top: 8,
  right: 8,
  // Height from what it holds, not from the page. Pinning `bottom` as well as
  // `top` stretched the panel down the whole margin however few notes were in
  // it, so two rows sat at the top of a column of empty background — a box
  // that looked like it had lost its contents. `maxHeight` keeps a long list
  // scrolling inside the page rather than past it.
  maxHeight: 'calc(100% - 16px)',
  width: 240,
  overflowY: 'auto',
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding: 10,
  borderRadius: 8,
  background: 'rgba(24, 24, 24, 0.9)',
  color: 'rgba(255, 255, 255, 0.92)',
  fontSize: '0.85em',
  zIndex: 3,
}

const belowStyle: CSSProperties = {
  flex: '0 0 auto',
  maxHeight: '28%',
  overflowY: 'auto',
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding: '8px 12px',
  background: 'rgba(24, 24, 24, 0.9)',
  color: 'rgba(255, 255, 255, 0.92)',
  fontSize: '0.85em',
}

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  gap: 8,
  padding: 0,
  border: 'none',
  background: 'none',
  color: 'inherit',
  font: 'inherit',
  textAlign: 'left',
  cursor: 'pointer',
}

const numberStyle = (colour: string): CSSProperties => ({
  flex: '0 0 auto',
  minWidth: 18,
  height: 18,
  borderRadius: 3,
  background: colour,
  color: 'rgba(20, 20, 20, 0.9)',
  fontSize: 11,
  fontWeight: 600,
  lineHeight: '18px',
  textAlign: 'center',
})

export interface NoteListProps {
  /** The notes on this slot, in marker order — `notes.ts`'s `notesHere`. */
  here: NoteHere[]
  placement: 'side' | 'below'
  onOpen: (id: string) => void
}

export function NoteList({ here, placement, onOpen }: NoteListProps) {
  if (here.length === 0) return null
  return (
    <div
      style={placement === 'side' ? sideStyle : belowStyle}
      aria-label="Notes on this page"
      role="list"
    >
      {here.map((h, index) => (
        <button
          key={h.note.id}
          type="button"
          role="listitem"
          style={rowStyle}
          onClick={() => onOpen(h.note.id)}
        >
          <span style={numberStyle(noteColour(h.note.color).tab)}>
            {index + 1}
          </span>
          <span>
            {/* Where a travelled note came from, and what kind of mark it is —
                both said in the row rather than left to be inferred from a
                marker the reader may not have found yet. */}
            {h.travelled && (
              <em style={{ opacity: 0.7 }}>on {h.note.edition} · </em>
            )}
            {isPoint(h.note.region) && (
              <em style={{ opacity: 0.7 }}>point · </em>
            )}
            {h.note.body || <em style={{ opacity: 0.7 }}>empty</em>}
          </span>
        </button>
      ))}
    </div>
  )
}

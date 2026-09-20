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
 * and whichever a reader asks for in settings. `App.tsx` decides only which of
 * the two `here` mounts as — beside the page (`sideStyle`) or under it
 * (`belowStyle`) — not where either one sits: beside the page, this component
 * docks itself the way a panel does, between the bars (`sideStyle`'s own
 * `top`/`maxHeight`, below); under the page, it fills what it is given.
 */
const sideStyle: CSSProperties = {
  position: 'absolute',
  // Where a docked panel stands (spec 2026-09-17 §8.5): below the top bar's
  // line and above the bottom bar's, whether or not the bars are shown —
  // `--panel-top` and `--panel-bottom` in `styles/reader.css`.
  top: 'var(--panel-top)',
  right: 'calc(var(--space-4) + var(--safe-right))',
  // Height from what it holds, not from the page. Pinning `bottom` as well as
  // `top` stretched the panel down the whole margin however few notes were in
  // it, so two rows sat at the top of a column of empty background — a box
  // that looked like it had lost its contents. `maxHeight` keeps a long list
  // scrolling between the bars rather than under them, floored at 8rem the
  // same way `.docked-panel` is in `styles/reader.css`, for a window too
  // short to fit both bars with anything left between them.
  maxHeight: 'max(8rem, calc(100% - var(--panel-top) - var(--panel-bottom)))',
  width: 240,
  overflowY: 'auto',
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding: 10,
  zIndex: 3,
}

// Padded by the safe areas, since it runs to the reader's bottom edge.
const belowStyle: CSSProperties = {
  flex: '0 0 auto',
  maxHeight: '28%',
  overflowY: 'auto',
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding:
    '8px calc(12px + var(--safe-right)) calc(8px + var(--safe-bottom)) calc(12px + var(--safe-left))',
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
  color: 'var(--art-ink)',
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
  /**
   * Whether `Chrome`'s bars are shown — O112, answered in spec 2026-09-17
   * §8.1. Under the bottom bar the list below the page is `visibility:
   * hidden`: still mounted and still taking its space, so the page area keeps
   * its size on `Tab`, which `Chrome` exists to guarantee. The list beside the
   * page docks clear of both bars and ignores this.
   */
  covered?: boolean
}

export function NoteList({ here, placement, onOpen, covered }: NoteListProps) {
  if (here.length === 0) return null
  return (
    <div
      className={`note-list note-list-${placement}`}
      style={
        placement === 'side'
          ? sideStyle
          : { ...belowStyle, visibility: covered ? 'hidden' : undefined }
      }
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
              <em className="reader-muted">on {h.note.edition} · </em>
            )}
            {isPoint(h.note.region) && (
              <em className="reader-muted">point · </em>
            )}
            {h.note.body || <em className="reader-muted">empty</em>}
          </span>
        </button>
      ))}
    </div>
  )
}

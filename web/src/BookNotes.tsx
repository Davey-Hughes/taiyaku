/**
 * The book reader's notes: the panel a note is written in, the list of every
 * note (`a`), and the bar a selection offers its highlights from.
 */
import {
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { type Book, editionOrder } from './book'
import { excerpt } from './bookNotes'
import { usePanelFocus } from './focus'
import type { Note } from './notes'
import { DEFAULT_COLOUR } from './palette'
import { Swatches } from './Swatches'

/**
 * One note, its words written as they are typed and kept on leaving the box,
 * so a step of the undo history is a thought rather than a letter.
 */
export function NoteEditor({
  book,
  note,
  onChange,
  onDelete,
  onClose,
}: {
  book: Book
  note: Note
  onChange: (next: Note) => void
  onDelete: () => void
  onClose: () => void
}) {
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, true)
  const [draft, setDraft] = useState(note.body)
  const kept = useRef({ note, draft, onChange })
  kept.current = { note, draft, onChange }
  useEffect(() => () => keepDraft(kept), [])
  const marked = excerpt(book, note)
  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label="Note"
      ref={panelRef}
      tabIndex={-1}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Note</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {marked && <blockquote className="book-note-quote">{marked}</blockquote>}
      <textarea
        className="reader-field"
        aria-label="Note"
        rows={4}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => keepDraft(kept)}
      />
      <Swatches
        chosen={note.color ?? DEFAULT_COLOUR}
        onPick={(color) =>
          onChange({ ...note, color, updated: new Date().toISOString() })
        }
      />
      <button type="button" className="reader-button" onClick={onDelete}>
        Delete
      </button>
    </div>
  )
}

/**
 * The words typed into a note's box, written to it where they differ. The
 * written note is kept as the one in hand: leaving the box remounts the
 * editor, and its unmount would otherwise write the same words as a second
 * undo step.
 */
function keepDraft(
  kept: RefObject<{
    note: Note
    draft: string
    onChange: (next: Note) => void
  }>,
) {
  const { note, draft, onChange } = kept.current
  if (draft === note.body) return
  const next = { ...note, body: draft, updated: new Date().toISOString() }
  kept.current.note = next
  onChange(next)
}

/** Every note on the book, in the order the book runs, each a way to it. */
export function NoteList({
  book,
  notes,
  onOpen,
  onClose,
  onExport,
}: {
  book: Book
  notes: Note[]
  onOpen: (note: Note) => void
  onClose: () => void
  /** Hand the viewer every highlight as cards for Anki. */
  onExport: () => void
}) {
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, true)
  const order = editionOrder(book)
  const sorted = notes.toSorted(
    (a, b) =>
      order.indexOf(a.edition) - order.indexOf(b.edition) ||
      a.slot - b.slot ||
      (a.text?.start ?? 0) - (b.text?.start ?? 0),
  )
  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label="Notes"
      ref={panelRef}
      tabIndex={-1}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Notes</strong>
        <button type="button" className="reader-button" onClick={onExport}>
          Export words
        </button>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {sorted.length === 0 ? (
        <p>
          No notes yet. Select some words to highlight them, or press n for a
          note on the paragraph at the top.
        </p>
      ) : (
        <ol className="book-contents">
          {sorted.map((note) => (
            <li key={note.id}>
              <button type="button" onClick={() => onOpen(note)}>
                <span className="book-note-quote">{excerpt(book, note)}</span>
                {note.body && <span>{note.body}</span>}
              </button>
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}

/** How far the bar keeps from the selection and from the window's edges. */
const BAR_GAP = 8

/**
 * The highlights a selection can become, and a note on it, beside the
 * selection: under it, or vertical, left of it, a line there being the
 * column's whole height. On the other side where it would leave the window,
 * and held inside the window either way.
 */
export function SelectionBar({
  near,
  vertical,
  onHighlight,
  onNote,
  onLookUp,
}: {
  near: { left: number; top: number; right: number; bottom: number }
  vertical: boolean
  onHighlight: (colour: string) => void
  onNote: () => void
  onLookUp: () => void
}) {
  const bar = useRef<HTMLDivElement>(null)
  const [at, setAt] = useState<{ left: number; top: number }>()
  useLayoutEffect(() => {
    const el = bar.current
    if (!el) return
    const { width, height } = el.getBoundingClientRect()
    const wide = window.innerWidth
    const high = window.innerHeight
    let left = near.left
    let top = near.bottom + BAR_GAP
    if (vertical) {
      left = near.left - BAR_GAP - width
      top = near.top
      if (left < BAR_GAP) left = near.right + BAR_GAP
    } else if (top + height > high - BAR_GAP) {
      top = near.top - BAR_GAP - height
    }
    const within = (v: number, room: number) =>
      Math.max(BAR_GAP, Math.min(v, room - BAR_GAP))
    setAt({ left: within(left, wide - width), top: within(top, high - height) })
  }, [near, vertical])
  return (
    <div
      ref={bar}
      className="book-selection-bar"
      role="toolbar"
      aria-label="Selection"
      // Measured before it is shown, so it is never drawn where it would not fit.
      style={at ?? { left: 0, top: 0, visibility: 'hidden' }}
    >
      <Swatches label="Colour to mark in" onPick={onHighlight} />
      <button type="button" className="reader-button" onClick={onNote}>
        Note
      </button>
      <button type="button" className="reader-button" onClick={onLookUp}>
        Look up
      </button>
    </div>
  )
}

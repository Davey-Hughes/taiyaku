/**
 * The book reader's notes: the panel a note is written in, the list of every
 * note (`a`), and the bar a selection offers its highlights from.
 */
import { type RefObject, useEffect, useRef, useState } from 'react'
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

/** The highlights a selection can become, and a note on it, beside the selection. */
export function SelectionBar({
  at,
  onHighlight,
  onNote,
  onLookUp,
}: {
  at: { x: number; y: number }
  onHighlight: (colour: string) => void
  onNote: () => void
  onLookUp: () => void
}) {
  return (
    <div
      className="book-selection-bar"
      role="toolbar"
      aria-label="Selection"
      style={{ left: at.x, top: at.y }}
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

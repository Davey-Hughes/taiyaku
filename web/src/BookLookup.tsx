/**
 * The book reader's dictionary: a selected word looked up in this server's
 * dictionary for the book's language, each sense set out, and the meaning
 * kept on the word as a note to learn it by.
 */
import { useRef } from 'react'
import type { DictionaryEntry } from './api'
import { usePanelFocus } from './focus'

/** Where a lookup stands. */
export type Looked =
  | { kind: 'looking' }
  | { kind: 'none'; lang: string }
  | { kind: 'found'; entries: DictionaryEntry[] }
  | { kind: 'failed'; why: string }

/** A word's entries, each to be kept on the word with "Save word". */
export function LookupPanel({
  word,
  looked,
  tapsPass = false,
  onSave,
  onClose,
}: {
  word: string
  looked: Looked
  /** Whether a tap goes through the panel to the text under it. */
  tapsPass?: boolean
  onSave: (entry: DictionaryEntry) => void
  onClose: () => void
}) {
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, true)
  return (
    <div
      className="docked-panel book-lookup-panel"
      role="dialog"
      aria-label="Look up"
      ref={panelRef}
      tabIndex={-1}
      style={tapsPass ? { pointerEvents: 'none' } : undefined}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">{word}</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {looked.kind === 'looking' && <p>Looking it up…</p>}
      {looked.kind === 'none' && (
        <p>This server has no dictionary for {looked.lang}.</p>
      )}
      {looked.kind === 'failed' && <p>{looked.why}</p>}
      {looked.kind === 'found' && looked.entries.length === 0 && (
        <p>Not in the dictionary.</p>
      )}
      {looked.kind === 'found' &&
        looked.entries.map((entry, i) => (
          <section key={i} className="book-lookup-entry">
            <p>
              <strong>{entry.word}</strong> {entry.pos && <em>{entry.pos}</em>}
              {entry.via && entry.via !== entry.word && (
                <> · from {entry.via}</>
              )}
            </p>
            <ol>
              {entry.glosses.map((gloss, j) => (
                <li key={j}>{gloss}</li>
              ))}
            </ol>
            <button
              type="button"
              className="reader-button"
              onClick={() => onSave(entry)}
            >
              Save word
            </button>
          </section>
        ))}
    </div>
  )
}

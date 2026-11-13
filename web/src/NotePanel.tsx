import { useEffect, useRef, type CSSProperties } from 'react'
import { usePanelFocus } from './focus'
import {
  exportName,
  notesJson,
  type Note,
  type NotesDoc,
  type PanelMode,
  type ReviewEntry,
} from './notes'
import { COLOUR_NAMES, DEFAULT_COLOUR, noteColour } from './palette'

// The panel is `.docked-panel` in `styles/reader.css`, shared with `Help` and
// `CreditsPanel` (spec 2026-09-17 §8.5).

const rowStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 2,
  padding: '6px 0',
  border: 'none',
  borderTop: '1px solid var(--reader-hair)',
  background: 'none',
  color: 'inherit',
  font: 'inherit',
  textAlign: 'left',
  cursor: 'pointer',
}

// `PanelMode` — what the panel is showing, or nothing at all — is declared in
// `notes.ts` and imported above. Declared here, it would make a module that
// must not import React take its own state's type from a `.tsx`. Imported
// rather than re-exported because `useComponentExportOnlyModules` refuses an
// `export type { … }` specifier from a file that exports a component, the same
// rule `tiles.ts` exists for.

export interface NotePanelProps {
  mode: PanelMode | undefined
  /** The note being written or edited, while `mode` is `'edit'`. */
  draft: Note | undefined
  /** Every note on the work, with where each lands here — `notes.ts`'s `reviewList`. */
  entries: ReviewEntry[]
  /** What export writes out: the server's document, less what a read adds. */
  doc: NotesDoc
  work: string
  /** The edition being read, which is what "reachable from here" is relative to. */
  viewing: string
  /** Whether a write is on the wire. Every exit is disabled for the duration. */
  busy: boolean
  notice: string | undefined
  onBody: (body: string) => void
  onColour: (colour: string) => void
  onDelete: (id: string) => void
  onClose: () => void
  /** Whether there is anywhere to step back to, or forward to — see `history.ts`. */
  canUndo: boolean
  canRedo: boolean
  onUndo: () => void
  onRedo: () => void
  /** Take the reader to a note's landing slot in the edition being read. */
  onGo: (entry: ReviewEntry) => void
}

/**
 * The two surfaces a note needs that the page itself cannot hold: writing one,
 * and finding every one.
 *
 * One component rather than two because they share the same box, the same
 * busy-and-notice handling and the same close, and a reader moves between them
 * — saving a note and then looking at the list is one gesture apart.
 *
 * **An unreachable note is listed, marked, and its row is disabled rather than
 * missing.** It is the case the design names explicitly: a note whose slot has
 * no counterpart in the edition being read does not disappear, because a note
 * that vanished on a swap is indistinguishable from one that was never saved.
 * Disabled rather than clickable-and-inert, so the reason is visible before
 * the click rather than discovered by nothing happening after it.
 *
 * Export is a download of the server's document less each note's `at` and
 * `stale` (`notesJson`), from an object URL revoked at once. A note is a
 * reader's own writing, and getting it out must not require this tool.
 */
export function NotePanel({
  mode,
  draft,
  entries,
  doc,
  work,
  viewing,
  busy,
  notice,
  onBody,
  onColour,
  onDelete,
  onClose,
  canUndo,
  canRedo,
  onUndo,
  onRedo,
  onGo,
}: NotePanelProps) {
  const bodyRef = useRef<HTMLTextAreaElement>(null)

  // Before the textarea effect below, which moves focus: this records what had
  // focus when the panel opened, so closing can hand it back (`focus.ts`).
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, mode !== undefined)

  // Focus the editor whenever it opens on a note, and put the caret at the end.
  //
  // **`autoFocus` alone is not enough.** It applies when React first inserts
  // the element, so the first `n` of a session would focus the textarea, and
  // every `n` after a save would leave focus on `<body>` — so a reader writing
  // a second note would be typing into the page, where `s` opens the settings
  // panel and a digit jumps edition.
  //
  // Keyed on the note's id rather than on `mode`, so reopening a *different*
  // note refocuses while an ordinary re-render (typing a character, picking a
  // colour) does not fight the caret.
  //
  // The caret goes to the end because reopening a note to add to it is the
  // common case; landing at the start means a reader's first keystroke lands
  // in front of what they wrote last time.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `draft?.id` and `busy` are triggers, not closure reads — the body reaches the textarea through a ref. `draft?.id` in particular is what the comment above specifies: reopening a *different* note must refocus. Removing them would break that.
  useEffect(() => {
    if (mode !== 'edit') return
    const el = bodyRef.current
    if (!el) return
    el.focus()
    el.setSelectionRange(el.value.length, el.value.length)
    // `busy` is in the deps for the failed-write case: a save that is refused
    // leaves the editor open with the reader's text still in it, and focus on
    // whatever they clicked — so the next thing they type goes to the page
    // instead of the note. A successful save closes the editor, so this never
    // fights a reader who is done.
  }, [mode, draft?.id, busy])

  if (!mode) return null

  function download() {
    const url = URL.createObjectURL(
      new Blob([notesJson(doc)], { type: 'application/json' }),
    )
    const link = document.createElement('a')
    link.href = url
    link.download = exportName(work)
    link.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label={mode === 'edit' ? 'Note' : 'All notes'}
      ref={panelRef}
      tabIndex={-1}
    >
      {mode === 'edit' && draft && (
        <>
          {/* Where the note is anchored, stated before it is written: a note
              is anchored to a slot of one edition, and a reader who swapped a
              moment ago should not have to remember which. */}
          <p className="reader-meta" style={{ margin: 0 }}>
            {draft.edition} slot {draft.slot} ·{' '}
            {draft.region ? 'region' : 'whole page'}
          </p>
          {draft.stale && (
            <p className="reader-muted" style={{ margin: 0 }}>
              The page this note was written on is no longer in {draft.edition},
              so it is shown where it was stored. Marking a region moves it onto
              the page shown.
            </p>
          )}
          <textarea
            ref={bodyRef}
            value={draft.body}
            onChange={(e) => onBody(e.target.value)}
            rows={5}
            aria-label="Note body"
            placeholder="Drag on the page to mark a region."
            className="reader-field"
            style={{ width: '100%', resize: 'vertical', font: 'inherit' }}
          />
          {/*
            The colours, as swatches rather than a dropdown: the choice *is*
            the appearance, so a list of names would make a reader translate
            twice. The current one is ringed rather than ticked, because a tick
            drawn on a swatch has to be one colour or the other and would
            vanish into half of them.
          */}
          <div
            style={{ display: 'flex', gap: 6 }}
            role="group"
            aria-label="Note colour"
          >
            {COLOUR_NAMES.map((name) => {
              const chosen = (draft.color ?? DEFAULT_COLOUR) === name
              return (
                <button
                  key={name}
                  type="button"
                  className="reader-swatch"
                  disabled={busy}
                  onClick={() => onColour(name)}
                  aria-label={name}
                  aria-pressed={chosen}
                  title={name}
                  style={{
                    width: 22,
                    height: 22,
                    padding: 0,
                    borderRadius: '50%',
                    border: 'none',
                    background: noteColour(name).tab,
                    cursor: 'pointer',
                  }}
                />
              )
            })}
          </div>
          {notice && <p style={{ margin: 0 }}>{notice}</p>}
          {/*
            No Save and no Done: changes reach the server on their own, and
            being finished is not an act a reader performs.

            **There is a Close (O137).** `Escape`, or a click on the background
            around the page, closes this. That holds for a keyboard and a mouse
            and fails on a phone, where both are missing: a tap on the page
            while the editor is open is region drawing's gesture, so Delete
            would be the only way out of a note (spec 2026-09-17 §9.3). Close is
            the third exit, and the only one a finger can always reach.

            Every exit is disabled while the write is on the wire, the same rule
            the correction banner follows — Delete especially, since it is the
            control that most looks like it undoes the save it is racing, and
            Close, since a close racing a write is the same ordering.
          */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <button
              type="button"
              className="reader-button"
              disabled={busy}
              onClick={() => onDelete(draft.id)}
            >
              Delete
            </button>
            <span className="reader-meta" style={{ marginLeft: 'auto' }}>
              {busy ? 'saving…' : 'saved'}
            </span>
            <button
              type="button"
              className="reader-button"
              disabled={busy}
              onClick={onClose}
            >
              Close
            </button>
          </div>
        </>
      )}
      {mode === 'review' && (
        <>
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
            }}
          >
            <strong className="reader-panel-title">
              {entries.length} note{entries.length === 1 ? '' : 's'}
            </strong>
            <span style={{ display: 'flex', gap: 8 }}>
              {/* Undo and redo live here as well as on their keys, because
                  this is the screen a reader opens when they want to know what
                  has happened to their notes. */}
              <button
                type="button"
                className="reader-button"
                disabled={!canUndo || busy}
                onClick={onUndo}
                title="Undo (z)"
              >
                ↶
              </button>
              <button
                type="button"
                className="reader-button"
                disabled={!canRedo || busy}
                onClick={onRedo}
                title="Redo (Z)"
              >
                ↷
              </button>
              <button
                type="button"
                className="reader-button"
                disabled={entries.length === 0}
                onClick={download}
              >
                Export
              </button>
              <button type="button" className="reader-button" onClick={onClose}>
                Close
              </button>
            </span>
          </div>
          {notice && <p style={{ margin: 0 }}>{notice}</p>}
          {entries.length === 0 && (
            <p className="reader-muted" style={{ margin: 0 }}>
              Nothing written yet. <em>n</em> starts a note on the page you are
              reading.
            </p>
          )}
          {entries.map((entry) => (
            <button
              key={entry.note.id}
              type="button"
              style={rowStyle}
              onClick={() => onGo(entry)}
              disabled={entry.here === undefined}
            >
              <span className="reader-meta">
                {/* The same colour the page draws it in, so a note found here
                    is recognisable there without reading a word. */}
                <span
                  aria-hidden
                  style={{
                    display: 'inline-block',
                    width: 8,
                    height: 8,
                    borderRadius: '50%',
                    marginRight: 6,
                    background: noteColour(entry.note.color).tab,
                  }}
                />
                {entry.note.edition} slot {entry.note.slot}
                {entry.here === undefined
                  ? ` · not reachable from ${viewing}`
                  : entry.note.edition === viewing
                    ? ''
                    : ` · ${viewing} slot ${entry.here}`}
                {entry.note.stale && ' · no longer on its page'}
              </span>
              <span>{entry.note.body || <em>empty</em>}</span>
            </button>
          ))}
        </>
      )}
    </div>
  )
}

import { useEffect, useMemo, useRef, useState } from 'react'
import { getNotes, putNotes } from './api'
import { debounce } from './debounce'
import {
  current,
  emptyHistory,
  type History,
  record,
  redo,
  undo,
} from './history'
import type { Note, NotesDoc } from './notes'
import { saveNotes, writeGate } from './notesWrite'

/**
 * How long after the last change the notes are written. `App.tsx`'s
 * `NOTES_DEBOUNCE_MS`, picked for the same reason: a note is prose, and a
 * reader pausing mid-sentence should not spend a round trip on it.
 */
const SAVE_AFTER_MS = 700

export interface BookNotes {
  notes: Note[]
  /** Whether the notes have been read, before which a change would be written over them. */
  ready: boolean
  /** Replace the notes, a step the history can undo. */
  change: (next: Note[]) => void
  undo: () => void
  redo: () => void
  /** What the last write said, where it did not simply land. */
  notice?: string
}

/**
 * A work's notes for the book reader: read once, changed as a whole document
 * kept in an undo history, and written a moment after the last change, kept
 * on top of another device's where the two met.
 */
export function useBookNotes(work: string): BookNotes {
  const [history, setHistory] = useState<History<Note[]>>(() =>
    emptyHistory([]),
  )
  const [notice, setNotice] = useState<string>()
  const [ready, setReady] = useState(false)
  const held = useRef<{ doc?: NotesDoc; etag?: string }>({})
  const latest = useRef<Note[]>([])
  latest.current = current(history)

  useEffect(() => {
    let live = true
    getNotes(work)
      .then(({ doc, etag }) => {
        if (!live) return
        held.current = { doc, etag }
        setHistory(emptyHistory(doc.notes))
        setReady(true)
      })
      .catch((why: unknown) => {
        if (live) setNotice(`These notes could not be read: ${String(why)}`)
      })
    return () => {
      live = false
    }
  }, [work])

  const save = useMemo(
    () =>
      debounce(async () => {
        const gate = writeGate(held.current.doc, held.current.etag)
        if (gate.kind === 'wait') return
        if (gate.kind === 'refuse') {
          setNotice(gate.notice)
          return
        }
        const outcome = await saveNotes(
          work,
          gate.etag,
          gate.doc,
          latest.current,
          {
            put: putNotes,
            get: getNotes,
          },
        )
        if (outcome.kind === 'refused') {
          setNotice(outcome.notice)
          return
        }
        held.current = { doc: outcome.doc, etag: outcome.etag }
        if (outcome.kind === 'merged') {
          // The history starts again at the merge: an undo past it would
          // write the other device's notes away.
          setHistory(emptyHistory(outcome.doc.notes))
          setNotice(outcome.notice)
        } else {
          setNotice(undefined)
        }
      }, SAVE_AFTER_MS),
    [work],
  )
  useEffect(() => () => save.flush(), [save])

  return useMemo(
    () => ({
      notes: current(history),
      ready,
      change: (next: Note[]) => {
        setHistory((h) => record(h, next, Object.is))
        save()
      },
      undo: () => {
        setHistory(undo)
        save()
      },
      redo: () => {
        setHistory(redo)
        save()
      },
      notice,
    }),
    [history, ready, notice, save],
  )
}

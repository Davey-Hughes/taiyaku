/**
 * Writing the notes document, and what to do when somebody else wrote first.
 *
 * A file of its own rather than another function in `notes.ts`, for the reason
 * the extraction map gives for `loadSession`: everything in `notes.ts` is a
 * pure synchronous rule over `Note` and `Region`, and this is the one part of
 * the note editor that takes ports and is `async`. Putting the only `await` in
 * that file would make it a module about two different things.
 */
import type { FetchedNotes } from './api'
import { mergeOver, type Note, type NotesDoc } from './notes'
import { messageOf } from './message'

/**
 * `api.ts`'s `putNotes` and `getNotes`, injected.
 *
 * The no-value-import-of-`resolver.ts` rule does not apply here — `api.ts`
 * value-imports nothing from it — but they are ports anyway, because half of
 * what has to be pinned about this function is *which arguments a write
 * received*, and there is no other way to ask. A test's `put` pushes
 * `{ work, etag, doc }` onto an array and resolves or rejects; that is the
 * whole seam.
 */
export interface NotesPorts {
  put(work: string, etag: string, doc: NotesDoc): Promise<FetchedNotes>
  get(work: string): Promise<FetchedNotes>
}

/**
 * Whether a document a component is holding can be written at all.
 *
 * Generic over the document type — `truthWrite.ts`'s `saveTruth` reuses this
 * exact function rather than a second copy of the same three-way rule, since
 * a labelling session is the same shape as a notes edit: a document, a
 * validator that may or may not still be current, and nothing to write
 * before either has loaded.
 */
export type WriteGate<T = NotesDoc> =
  /**
   * There is a document and a validator. Both are handed back rather than
   * left for the caller to re-narrow: `saveNotes` below needs the pair, and a
   * caller that had to prove again what this already proved would be free to
   * prove it differently.
   */
  | { kind: 'write'; etag: string; doc: T }
  /**
   * Nothing has been loaded yet. Not a refusal and not a failure — there is
   * no document to write and nobody has asked for one to be written, so this
   * is the one silent arm.
   */
  | { kind: 'wait' }
  /**
   * There is a document, and no validator to write it against. `notice` is
   * what the reader is shown, the same field `SaveOutcome`'s `refused` uses
   * and for the same reason.
   */
  | { kind: 'refuse'; notice: string }

/**
 * The sentence a reader sees when their notes have stopped reaching the
 * server. It names the consequence first, because otherwise a reader sees the
 * editor, the notes, the undo history and no sign at all that any of it is
 * still being saved.
 */
const NO_VALIDATOR =
  'These notes are not being saved: this device has no current copy of them ' +
  'to write against. Reload the page.'

/**
 * The guard `App.tsx`'s `writeNotes` opens with, as a rule rather than an
 * `if` inside a component.
 *
 * Here rather than inline for the reason `saveNotes` below takes ports: what
 * this decides is what a reader sees when their notes stop reaching the
 * server, and a decision that can only be reached by mounting a component and
 * driving a debounce is a decision nothing tests.
 *
 * **A missing validator refuses out loud; only a missing document is silent.**
 * One branch for both — `if (!notesDoc || notesEtag === undefined) return` —
 * would make the defect invisible. Two things reach "document, no validator",
 * and neither is exotic: a load whose notes read failed (`startup.ts`'s
 * `Opened.notesEtag`, absent by design so that a failed read can never become
 * an unconditional write), and a write whose response carried no `ETag` —
 * `api.ts`'s `putNotes` reports that as `etag: undefined` and `App.tsx` stores
 * it. Either one would kill every later write for the rest of the session
 * while the editor went on accepting keystrokes, and nothing anywhere would
 * say so.
 *
 * The principle is §4.1a's, taken from the 412 path rather than invented here:
 * **refuse rather than write without a validator, and tell the reader.**
 *
 * `notice` defaults to `NO_VALIDATOR`, the notes wording, so a call site that
 * names only `doc` and `etag` gets it. A caller writing a different kind of
 * document (`truthWrite.ts`'s `saveTruth`) passes its own sentence, so a
 * reader who cannot save their labels is not told "these notes are not being
 * saved".
 */
export function writeGate<T>(
  doc: T | undefined,
  etag: string | undefined,
  notice: string = NO_VALIDATOR,
): WriteGate<T> {
  if (!doc) return { kind: 'wait' }
  if (etag === undefined) return { kind: 'refuse', notice }
  return { kind: 'write', etag, doc }
}

export type SaveOutcome =
  /** The write landed on the first attempt. */
  | { kind: 'written'; doc: NotesDoc; etag: string | undefined }
  /**
   * A stale validator, re-read, merged, and **written** — this variant is
   * produced only where the second write itself succeeded, which is what makes
   * the false notice unrepresentable rather than merely avoided. `doc` and
   * `etag` are the second write's, not the re-read's.
   */
  | {
      kind: 'merged'
      doc: NotesDoc
      etag: string | undefined
      /** "These notes changed on another device; yours were kept on top." */
      notice: string
    }
  /**
   * Nothing landed. `notice` is what the reader is shown; `error` carries a
   * failed re-read for the caller to log.
   */
  | { kind: 'refused'; notice: string; error?: unknown }

/**
 * The server's own wording for a stale validator (`routes.rs`'s `put_notes`),
 * matched by substring like the correction path's — `api.ts` only promises the
 * status is somewhere in the thrown message.
 */
const STALE = 'changed since you read them'

const KEPT_ON_TOP =
  'These notes changed on another device; yours were kept on top.'

/**
 * Write a notes document, and on a stale validator re-read, lay `notes` back
 * over what arrived by id, and write that. Retried once and once only.
 *
 * **On a stale validator it re-reads and re-applies rather than asking the
 * reader to try again.** Writes are automatic, so there is nobody to ask
 * "somebody else wrote; press Save again": the honest thing is to take the
 * other device's document, lay this reader's own notes back over it by id, and
 * write that. Notes belonging to *other* ids survive untouched, which is the
 * part that matters; two devices editing the same note is last-write-wins.
 *
 * Retried once and once only. A second failure is not a race, it is something
 * wrong, and a loop would hammer the server with a reader watching nothing
 * happen.
 *
 * **The retry writes against the validator the re-read returned.** Re-sending
 * the validator that had just been refused would always 412, so the merge
 * would never land.
 *
 * `notesWrite.test.ts` has the cases that hold it shut.
 */
export async function saveNotes(
  work: string,
  etag: string,
  doc: NotesDoc,
  notes: Note[],
  ports: NotesPorts,
): Promise<SaveOutcome> {
  try {
    const fresh = await ports.put(work, etag, { work: doc.work, notes })
    return { kind: 'written', doc: fresh.doc, etag: fresh.etag }
  } catch (err) {
    const message = messageOf(err)
    if (!message.includes(STALE)) return { kind: 'refused', notice: message }
    let fresh: FetchedNotes
    try {
      fresh = await ports.get(work)
    } catch (rereadErr) {
      // The re-read's own failure is logged by the caller and the reader is
      // shown the original refusal.
      return { kind: 'refused', notice: message, error: rereadErr }
    }
    // A server old enough to answer this route without an `ETag` leaves
    // nothing to write conditionally against, and there is no unconditional
    // write anywhere in this reader. Refusing is what the load path already
    // does when the notes read fails; inventing a write here would be the one
    // place that guessed.
    if (fresh.etag === undefined) return { kind: 'refused', notice: message }
    try {
      const landed = await ports.put(work, fresh.etag, {
        work: doc.work,
        notes: mergeOver(fresh.doc.notes, notes),
      })
      return {
        kind: 'merged',
        doc: landed.doc,
        etag: landed.etag,
        notice: KEPT_ON_TOP,
      }
    } catch (retryErr) {
      // The second refusal is what the reader is shown, not the first: it is
      // the one that describes what actually stopped the write. Nothing was
      // written, so nothing is claimed to have been.
      return { kind: 'refused', notice: messageOf(retryErr) }
    }
  }
}

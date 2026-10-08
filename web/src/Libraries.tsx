/**
 * The libraries screen: Configuration → Libraries.
 *
 * **The list owns which libraries there are; each card owns what it is
 * doing**, as `Users` and `UserRow` split it.
 *
 * **A library is made with its folders.** There is no empty library: a grant
 * names a library and resolves to its folders, and one with none would read
 * as a grant on everything. So the form collects a name and one or more
 * folders and sends them together, and the server makes all of it or none.
 *
 * **Two failure slots.** `reload` is every card's `onChanged`, so one slot
 * would let a card's rename erase the form's refusal, which is the one
 * message naming which folder was refused and why. The refusal is drawn in
 * the form, beside the button that was pressed; the list's failure is the
 * screen's.
 *
 * **The probe from an add is kept, by the folder's path.** The watch probe
 * runs when a folder is added and at no other time, so a folder added in
 * this session has a watch state and one read from the list does not.
 *
 * **A removal's re-derive report, or a move's, is kept here, not in the card
 * or the folder's row.** What was removed or moved is not drawn where it was
 * once the list reloads, and the report is the one place a lost position or
 * an unwritable document is said. A removal's is named for the library, or
 * for the folder where the library stays, and a move's for the folder. It is
 * replaced by the next removal or move, and cleared when a library is made.
 *
 * **Reads carry a sequence number.** Several are routinely in flight, and
 * they need not answer in order.
 */
import { useCallback, useEffect, useId, useRef, useState } from 'react'
import type { Added, Derived, Probe, RemovalAnswer } from './config'
import { FolderPicker } from './FolderPicker'
import {
  createLibrary,
  type Library,
  listLibraries,
  type MoveAnswer,
  type Moved,
} from './libraries'
import { LibraryCard } from './LibraryCard'
import { MoveCounts } from './MoveCounts'
import { sentenceOf } from './message'
import { Refusal } from './Refusal'
import { reveal } from './reveal'
import { DeriveSection } from './ScanReport'

export function Libraries() {
  const [libraries, setLibraries] = useState<Library[] | undefined>()
  /**
   * Why the list could not be read: an object for each failed read, so that
   * one repeating a standing sentence is a new failure and is brought into
   * view again where `reveal` says it is to be.
   */
  const [failure, setFailure] = useState<
    { sentence: string; reveal: boolean } | undefined
  >()
  const failureEl = useRef<HTMLParagraphElement>(null)
  /**
   * Why the last library was not made. Cleared by the next attempt and by any
   * change to what the form holds, since the sentence is about that.
   */
  const [createFailure, setCreateFailure] = useState<string | undefined>()
  const [name, setName] = useState('')
  const [folders, setFolders] = useState<string[]>([])
  const [creating, setCreating] = useState(false)
  /** The text in the make form's path box, which is not a folder until added. */
  const [pending, setPending] = useState('')
  const heldWhy = useId()
  /**
   * "Make the library" is held while no folder is on the list, and a path in
   * the box looks like one that is (O209). Said only then: a held button
   * with an empty box has nothing in it to explain.
   */
  const heldByPath = folders.length === 0 && pending.trim() !== ''
  /**
   * What the re-derive after the last removal, or the derive after the last
   * move, found, and whose it was.
   */
  const [removal, setRemoval] = useState<
    | {
        name: string
        derive: Derived
        /** A move's: what it did, and where from. Absent for a removal's. */
        move?: { moved: Moved; from?: string; newLibrary: boolean }
      }
    | undefined
  >()
  const [probes, setProbes] = useState<Record<string, Probe>>({})
  /** A ref as well as `creating`: two submits before a render both read false. */
  const inFlight = useRef(false)
  const latest = useRef(0)

  /** `quiet`: no press started this read, so its failure is not scrolled to. */
  const reload = useCallback((quiet = false) => {
    const seq = ++latest.current
    listLibraries()
      .then((body) => {
        if (seq !== latest.current) return
        setLibraries(body.libraries)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure({ sentence: sentenceOf(err), reveal: !quiet })
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const remember = useCallback((added: Added[]) => {
    setProbes((held) => {
      const next = { ...held }
      for (const one of added) {
        next[one.root.path] = {
          watching: one.watching,
          watch_reason: one.watch_reason,
        }
      }
      return next
    })
  }, [])

  /**
   * A library is gone. It is dropped here as well as by the reload: if the
   * reload fails the card would otherwise stay, with live buttons, for a
   * library that is gone.
   */
  const dropLibrary = useCallback(
    (id: number, name: string, answer: RemovalAnswer) => {
      setRemoval({ name, derive: answer.derive })
      setLibraries((held) => held?.filter((one) => one.id !== id))
    },
    [],
  )

  const removed = useCallback(
    (id: number, name: string, answer: RemovalAnswer) => {
      dropLibrary(id, name, answer)
      reload()
    },
    [dropLibrary, reload],
  )

  /** What is drawn, for the decisions a removal makes from it. */
  const drawn = useRef<Library[] | undefined>(undefined)
  drawn.current = libraries

  /**
   * A folder was removed from a library. Its re-derive's report is kept here
   * for the reason a library's is: the row that asked is unmounted by the
   * reload. The folder is dropped from what is drawn at once, for the reason
   * a library is, and a folder that was the last of those drawn takes the
   * library with it: the report is then named for the library, which is what
   * went. The row that asked reads the list itself once this returns.
   */
  const folderRemoved = useCallback(
    (id: number, path: string, answer: RemovalAnswer) => {
      const library = drawn.current?.find((one) => one.id === id)
      const left = library?.folders.filter((one) => one.path !== path)
      if (library !== undefined && left?.length === 0) {
        dropLibrary(id, library.name, answer)
        return
      }
      setRemoval({ name: path, derive: answer.derive })
      setLibraries((held) =>
        held?.map((one) =>
          one.id === id
            ? { ...one, folders: one.folders.filter((f) => f.path !== path) }
            : one,
        ),
      )
    },
    [dropLibrary],
  )

  /**
   * A folder was moved to another library. Its derive's report is kept here
   * for the reason a removal's is: the row that asked is unmounted by the
   * reload, which the row asks for itself once this returns. It replaces a
   * removal's, being drawn in its place, and is named for the folder and the
   * library it went to.
   */
  const folderMoved = useCallback(
    (id: number, path: string, answer: MoveAnswer) => {
      setRemoval({
        name: path,
        derive: answer.derive,
        move: {
          moved: answer.moved,
          from: drawn.current?.find((one) => one.id === id)?.name,
          // A target the screen does not draw is one the move made.
          newLibrary: !drawn.current?.some(
            (one) => one.id === answer.moved.library_id,
          ),
        },
      })
      // Dropped from what is drawn at once, for the reason a removed folder
      // is: if the reload fails, a folder that is gone from its library
      // would stay, with live buttons. A library the move emptied went with
      // it. The folder is not drawn in its new library: the reload does that.
      setLibraries((held) =>
        answer.moved.library_removed !== null
          ? held?.filter((one) => one.id !== id)
          : held?.map((one) =>
              one.id === id
                ? {
                    ...one,
                    folders: one.folders.filter((f) => f.path !== path),
                  }
                : one,
            ),
      )
    },
    [],
  )

  const report = useRef<HTMLDivElement>(null)
  const reportLabel = useId()
  // What was removed was somewhere down the page and the report is at the top,
  // so the view would end with nothing said and focus on a button that no
  // longer exists. Focusing the report has its name read. Focus alone scrolls
  // only as far as the nearest edge, which for a report taller than the
  // window is its foot, so its head is put at the top of the view, clear of
  // the phone bar, as well, when it is not already in view. Taken once per
  // report: the list reading again, or a library being made, leaves focus
  // where it is.
  useEffect(() => {
    const el = report.current
    if (removal === undefined || el === null) return
    el.focus({ preventScroll: true })
    reveal(el)
  }, [removal])
  // The list's failure is drawn above everything, and a person acting further
  // down the page is not looking there. A failed read is brought into view,
  // which does nothing where it already is, except the read at the end of any
  // scan, including one the person started: the page would move from under
  // whoever is typing by then. Focus stays where it is.
  useEffect(() => {
    const el = failureEl.current
    if (failure?.reveal && el !== null) reveal(el)
  }, [failure])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || folders.length === 0) return
    if (inFlight.current) return
    inFlight.current = true
    setCreating(true)
    setCreateFailure(undefined)
    createLibrary(name, folders)
      .then((made) => {
        inFlight.current = false
        setCreating(false)
        setName('')
        setRemoval(undefined)
        setFolders([])
        remember(made.added)
        // Drawn from the create's own answer, as a removed library is
        // dropped at once: if the read that follows fails, the list still
        // holds what was just made. It goes last. The server orders by name
        // in its own collation, which the browser's `localeCompare` is not,
        // and the read that follows puts it where the server does.
        setLibraries((held) =>
          held === undefined
            ? held
            : [
                ...held.filter((one) => one.id !== made.library.id),
                made.library,
              ],
        )
        reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setCreating(false)
        setCreateFailure(sentenceOf(err))
      })
  }

  return (
    <section className="libraries">
      <h2>Libraries</h2>
      {failure !== undefined && (
        <p role="alert" ref={failureEl}>
          {libraries === undefined
            ? failure.sentence
            : `The libraries list could not be read again (${failure.sentence}), so what is drawn may be out of date.`}
        </p>
      )}
      {removal !== undefined && (
        <div
          ref={report}
          className="removal-report scan-report"
          tabIndex={-1}
          role="group"
          aria-labelledby={reportLabel}
        >
          <p id={reportLabel}>
            {removal.move === undefined
              ? `What re-deriving after removing ${removal.name} found:`
              : `Moved ${removal.name}${
                  removal.move.from === undefined
                    ? ''
                    : ` from ${removal.move.from}`
                } to ${removal.move.moved.library}.`}
          </p>
          {removal.move !== undefined && (
            <>
              <MoveCounts
                moved={removal.move.moved}
                past
                newLibrary={removal.move.newLibrary}
              />
              <p>What deriving after the move found:</p>
            </>
          )}
          <DeriveSection derive={removal.derive} />
        </div>
      )}

      {libraries !== undefined &&
        (libraries.length === 0 ? (
          <p className="empty">
            No libraries yet. Make one below: a name, and the folder or folders
            its files are in. The server checks that it can read each folder and
            that none sits inside another.
          </p>
        ) : (
          <ul className="library-list">
            {libraries.map((library) => (
              <LibraryCard
                // By id: a library's id is never given to a later library.
                key={library.id}
                library={library}
                others={libraries
                  .filter((other) => other.id !== library.id)
                  .map(({ id, name }) => ({ id, name }))}
                probes={probes}
                onAdded={remember}
                onChanged={reload}
                onRemoved={(name, answer) => removed(library.id, name, answer)}
                onFolderRemoved={(path, answer) =>
                  folderRemoved(library.id, path, answer)
                }
                onFolderMoved={(path, answer) =>
                  folderMoved(library.id, path, answer)
                }
              />
            ))}
          </ul>
        ))}

      <form onSubmit={submit} className="add-library">
        <h3>Make a library</h3>
        {/* O175: the `aria-label` repeats the label's words. */}
        <label className="field">
          <span>Name</span>
          <input
            type="text"
            name="library-name"
            aria-label="Name"
            value={name}
            onChange={(e) => {
              setCreateFailure(undefined)
              setName(e.target.value)
            }}
          />
        </label>
        {folders.length > 0 && (
          <ul className="chosen-folders">
            {folders.map((folder) => (
              <li key={folder}>
                <code>{folder}</code>
                <button
                  type="button"
                  name="unchoose"
                  aria-label={`Take ${folder} off the list`}
                  onClick={() => {
                    setCreateFailure(undefined)
                    setFolders((held) => held.filter((f) => f !== folder))
                  }}
                >
                  Take off
                </button>
              </li>
            ))}
          </ul>
        )}
        <FolderPicker
          action="Add to the list"
          onPath={setPending}
          onPick={(path) => {
            setCreateFailure(undefined)
            setFolders((held) => (held.includes(path) ? held : [...held, path]))
          }}
        />
        <button
          type="submit"
          disabled={creating || name === '' || folders.length === 0}
          aria-describedby={heldByPath ? heldWhy : undefined}
        >
          {creating ? 'Checking…' : 'Make the library'}
        </button>
        {heldByPath && (
          <p className="why" id={heldWhy}>
            The folder in the box is not on the list yet: “Add to the list” puts
            it there.
          </p>
        )}
        {createFailure !== undefined && <Refusal>{createFailure}</Refusal>}
        <p className="why">
          Folders in one library are derived together: a chapter release in one
          folder joins its volume in another as a second edition.
        </p>
      </form>
    </section>
  )
}

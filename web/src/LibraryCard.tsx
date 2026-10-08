/**
 * One library: its name, what it holds, a row per folder, and what is done
 * to it whole.
 *
 * **The card owns the library's writes and its folder rows own theirs.** A
 * rename, an added folder, a scan of the whole library and its removal are
 * sent from here. A folder's settings, its own scan and its removal are
 * `RootRow`'s, unchanged.
 *
 * **A scan of the library is watched twice, and that is the design.** The
 * server holds each folder's slot in its scan registry while the library's
 * scan runs, so each row's own status route answers for it. The card bumps
 * `adopt`, each row polls and draws its own report, and the card polls the
 * library's status to know whether the library is scanning.
 *
 * **It asks the library's status once on mount**, for `RootRow`'s reason: a
 * scan started before this card was drawn is still running.
 *
 * **A 409 from Scan library is proof a scan is in flight**, of the library or
 * of one folder, so the card draws the sentence and polls. The sentence goes
 * when that scan ends.
 *
 * **Scans and removals of a library and its folders exclude each other.** The
 * card is busy while it is scanning, starting a scan or knows of one, while
 * its removal dialog is open or its removal is on its way, and while any
 * folder row says it is scanning, starting a scan, or has a removal or a move
 * dialog open. Each row is told whether something *else* is busy, and holds
 * its Scan folder, its Move… and its Remove…, and closes an open removal or
 * move dialog; the card holds its own Scan library and Remove library…
 * while anything is busy, and its removal's confirm while a scan of the
 * library is known or a folder is busy. A held control is a disabled one and
 * sends nothing. A scan holds neither Rename… nor Add folder nor a folder's
 * settings; the removal of the library holds Rename… and Add folder.
 */
import { useEffect, useRef, useState } from 'react'
import {
  type Added,
  type Probe,
  type RemovalAnswer,
  SCAN_POLL_MS,
  statusOf,
} from './config'
import { FolderPicker } from './FolderPicker'
import {
  addFolder,
  type Library,
  libraryScanStatus,
  type MoveAnswer,
  renameLibrary,
  startLibraryScan,
} from './libraries'
import { messageOf, sentenceOf } from './message'
import { Refusal } from './Refusal'
import type { NamedLibrary } from './MoveRoot'
import { RemoveLibrary } from './RemoveLibrary'
import { RootRow } from './RootRow'

export interface LibraryCardProps {
  library: Library
  /** Every other library, for a folder's Move…. */
  others: NamedLibrary[]
  /** Watch probes of folders added in this session, by path. */
  probes: Record<string, Probe>
  /** A folder was added here: its probe is the screen's to keep. */
  onAdded: (added: Added[]) => void
  /**
   * The list reloads on a rename, an added folder or a finished scan. `quiet`
   * is a scan that ended by itself: no press started that read.
   */
  onChanged: (quiet?: boolean) => void
  /**
   * The library is gone. Its re-derive's report is the screen's to keep, since
   * this card is not drawn after the list reloads: a lost position or an
   * unwritable document reaches nobody otherwise.
   */
  onRemoved: (name: string, answer: RemovalAnswer) => void
  /**
   * A folder of this library is gone: its path, and what its re-derive found.
   * Handed up for the reason `onRemoved` is, since the row that asked is not
   * drawn once the list reloads.
   */
  onFolderRemoved: (path: string, answer: RemovalAnswer) => void
  /**
   * A folder of this library is now in another: its path, and what the move
   * did and what deriving after it found. Handed up for the same reason.
   */
  onFolderMoved: (path: string, answer: MoveAnswer) => void
}

/** How many consecutive failed polls a running scan survives, as `RootRow`'s. */
const POLL_MISSES_ALLOWED = 3

/** `n` of a thing, in the number the count is. Zero takes the plural. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export function LibraryCard({
  library,
  others,
  probes,
  onAdded,
  onChanged,
  onRemoved,
  onFolderRemoved,
  onFolderMoved,
}: LibraryCardProps) {
  /** Why the card's last scan could not start, or its poll failed. */
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * The sentence of a 409: a scan is in flight. A slot of its own, because it
   * is true only until that scan ends, and is cleared then.
   */
  const [conflict, setConflict] = useState<string | undefined>()
  /** Why the last folder was not added. Drawn beside the picker it answers. */
  const [addFailure, setAddFailure] = useState<string | undefined>()
  /** Why the last rename was refused. Drawn in the rename form. */
  const [renameFailure, setRenameFailure] = useState<string | undefined>()
  /**
   * The rows that are scanning, starting a scan, or have a removal or a move
   * dialog open, by path: the key the rows are drawn by, which outlasts a
   * change of id.
   */
  const [busyRows, setBusyRows] = useState<ReadonlySet<string>>(new Set())
  const [renaming, setRenaming] = useState(false)
  const [name, setName] = useState(library.name)
  const [saving, setSaving] = useState(false)
  const [removing, setRemoving] = useState(false)
  const [running, setRunning] = useState(false)
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | null>(null)
  /** Bumped when this card starts a scan. See `RootRow`'s `adopt`. */
  const [adopt, setAdopt] = useState(0)
  /**
   * A ref as well as the flags: two presses before a render both read false.
   * One for the scan's start and one for the card's writes, so Save pressed
   * while a scan is starting is not dropped.
   */
  const scanning = useRef(false)
  const writing = useRef(false)
  const misses = useRef(0)
  // Held in a ref for the reason `RootRow` holds its own: the poll's interval
  // must not restart each time the parent hands over a fresh closure.
  const changed = useRef(onChanged)
  changed.current = onChanged

  useEffect(() => {
    let cancelled = false
    libraryScanStatus(library.id)
      .then((status) => {
        if (cancelled || !status.running) return
        setElapsed(status.elapsed_secs)
        setRunning(true)
      })
      .catch(() => {
        // Silent, as `RootRow`'s mount read is: pressing Scan folder is what
        // surfaces a real fault.
      })
    return () => {
      cancelled = true
    }
  }, [library.id])

  useEffect(() => {
    if (!running) return
    let cancelled = false
    const timer = setInterval(() => {
      libraryScanStatus(library.id)
        .then((status) => {
          if (cancelled) return
          misses.current = 0
          setElapsed(status.elapsed_secs)
          if (status.running) return
          setRunning(false)
          setConflict(undefined)
          changed.current(true)
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses.current += 1
          if (misses.current < POLL_MISSES_ALLOWED) return
          setRunning(false)
          // The 409's sentence is true while the scan it named is watched,
          // and nothing watches it now.
          setConflict(undefined)
          setFailure(sentenceOf(err))
        })
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [running, library.id])

  const scan = () => {
    if (scanning.current) return
    scanning.current = true
    setStarting(true)
    setFailure(undefined)
    setConflict(undefined)
    misses.current = 0
    startLibraryScan(library.id)
      .then(() => {
        scanning.current = false
        setStarting(false)
        setRunning(true)
        setAdopt((n) => n + 1)
      })
      .catch((err: unknown) => {
        scanning.current = false
        setStarting(false)
        if (statusOf(messageOf(err)) === 409) {
          setConflict(sentenceOf(err))
          setRunning(true)
        } else {
          setFailure(sentenceOf(err))
        }
      })
  }

  const rename = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || writing.current) return
    writing.current = true
    setSaving(true)
    setRenameFailure(undefined)
    renameLibrary(library.id, name)
      .then(() => {
        writing.current = false
        setSaving(false)
        setRenaming(false)
        onChanged()
      })
      .catch((err: unknown) => {
        writing.current = false
        setSaving(false)
        setRenameFailure(sentenceOf(err))
      })
  }

  /** Rejects on a refusal, which is what keeps the picker's typed path. */
  const add = (path: string) => {
    setAddFailure(undefined)
    return addFolder(library.id, path).then(
      (added) => {
        onAdded([added])
        onChanged()
      },
      (err: unknown) => {
        setAddFailure(sentenceOf(err))
        throw err
      },
    )
  }

  const rowsBusy = busyRows.size > 0
  const own = running || starting || removing
  /**
   * Whether anything is going on in this library: its own scan or removal,
   * or a folder scanning or with a removal or a move dialog open. Scan
   * library and Remove library… are held while it is.
   */
  const busy = own || rowsBusy
  /** Whether something other than the folder at `path` is going on. */
  const othersBusy = (path: string) =>
    own || [...busyRows].some((key) => key !== path)

  /** A row says whether it is busy; the card keeps the set of those that are. */
  const reportRow = (path: string, on: boolean) =>
    setBusyRows((held) => {
      if (held.has(path) === on) return held
      const next = new Set(held)
      if (on) next.add(path)
      else next.delete(path)
      return next
    })

  return (
    <li className="library-card">
      <h3>{library.name}</h3>
      <p className="holds">
        {count(library.works, 'work', 'works')} in{' '}
        {count(library.folders.length, 'folder', 'folders')}
      </p>
      {library.works === 0 && library.books > 0 && (
        <p className="no-readable">
          No readable files yet: its scans found{' '}
          {count(library.books, 'EPUB or PDF file', 'EPUB or PDF files')}, and
          reading those arrives with the books work. A rescan is all this
          library will need then.
        </p>
      )}
      {failure !== undefined && <p role="alert">{failure}</p>}
      {conflict !== undefined && <p role="alert">{conflict}</p>}

      <div className="actions">
        <button
          type="button"
          name="scan-library"
          onClick={scan}
          disabled={busy}
        >
          Scan library
        </button>
        <button
          type="button"
          name="rename-library"
          onClick={() => {
            setName(library.name)
            setRenaming(true)
          }}
          disabled={renaming || removing}
        >
          Rename…
        </button>
        <button
          type="button"
          name="remove-library"
          onClick={() => setRemoving(true)}
          disabled={busy}
        >
          Remove library…
        </button>
      </div>
      {running && (
        <p className="scanning">
          A scan is running in this library…
          {elapsed === null ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}

      {renaming && (
        <form className="rename-library" onSubmit={rename}>
          {/* O175: the `aria-label` repeats the label's words. */}
          <label className="field">
            <span>Name</span>
            <input
              type="text"
              name="library-name"
              aria-label="Name"
              value={name}
              onChange={(e) => {
                setRenameFailure(undefined)
                setName(e.target.value)
              }}
            />
          </label>
          <button type="submit" disabled={saving || name === '' || removing}>
            {saving ? 'Saving…' : 'Save'}
          </button>
          <button
            type="button"
            onClick={() => {
              setRenameFailure(undefined)
              setRenaming(false)
            }}
            disabled={saving}
          >
            Cancel
          </button>
          {renameFailure !== undefined && <Refusal>{renameFailure}</Refusal>}
        </form>
      )}

      {removing && (
        <RemoveLibrary
          library={library}
          blocked={running || rowsBusy}
          onCancel={() => setRemoving(false)}
          onRemoved={(answer) => {
            setRemoving(false)
            onRemoved(library.name, answer)
          }}
        />
      )}

      <ul className="root-list">
        {library.folders.map((root) => (
          <RootRow
            // By path, not id: a freed `root.id` is given to the next folder
            // added, and a path cannot collide, since no two folders overlap.
            key={root.path}
            root={root}
            probe={probes[root.path]}
            adopt={adopt}
            lastOf={library.folders.length === 1 ? library.name : undefined}
            held={othersBusy(root.path)}
            moveTo={{
              from: { id: library.id, name: library.name },
              targets: others,
            }}
            onBusy={(on) => reportRow(root.path, on)}
            onChanged={onChanged}
            onRemoved={(answer) => onFolderRemoved(root.path, answer)}
            onMoved={(answer) => onFolderMoved(root.path, answer)}
          />
        ))}
      </ul>

      <div className="add-root">
        <FolderPicker
          action="Add folder"
          onPick={add}
          disabled={removing}
          onEdit={() => setAddFailure(undefined)}
        />
        {addFailure !== undefined && <Refusal>{addFailure}</Refusal>}
      </div>
    </li>
  )
}

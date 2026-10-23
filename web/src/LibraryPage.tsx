/**
 * One library's page: its name, what it holds, what is done to it whole, and
 * a table of its directories, each of which has a page of its own.
 *
 * **The page owns the library's writes and a directory's page owns the
 * directory's.** A rename, an added directory, a scan of the whole library and
 * its removal are sent from here. A directory's settings, its own scan, its
 * move and its removal are `RootRow`'s, on `DirPage`.
 *
 * **It asks the library's status once on mount**, for `RootRow`'s reason: a
 * scan started before this page was drawn is still running. The status says
 * whether any directory of the library is scanning, so a directory's own scan,
 * started on its page, holds this page's Scan library and Remove… as
 * a scan of the library does. The directories it names as running say
 * "Scanning…" in the table.
 *
 * **A 409 from Scan library is proof a scan is in flight**, of the library or
 * of one directory, so the page draws the sentence and polls. The sentence goes
 * when that scan ends.
 *
 * **The reports of a scan this page watched are drawn here when it ends.**
 * A report is held by its directory's own status, which this page does not
 * otherwise read, so once the library's says nothing is running the status
 * of each directory the page saw the scan hold is read, and whatever report
 * each holds is drawn under its path. Only those: any other directory's status
 * holds a report from another scan, if any, and a directory that joined the
 * library while the scan ran is not in it.
 *
 * **Scans and removals exclude each other.** Scan library and Remove…
 * are held while a scan is known, starting, or the removal dialog is open;
 * the removal's confirm is held while a scan is known. A scan holds neither
 * Rename… nor Add directory…; the removal holds both, and the Directories table
 * draws its paths without links while it is open: a directory's page offers
 * Scan directory, Move… and Remove… on a directory the removal is taking.
 */
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { NavLink } from 'react-router'
import {
  type Added,
  humanInterval,
  lastScan,
  POLL_MISSES_ALLOWED,
  type RemovalAnswer,
  SCAN_POLL_MS,
  type ScanOutcome,
  scanStatus,
  statusOf,
} from './config'
import { dirPath, LIBRARIES, libraryNameClash } from './configAddress'
import { DirPicker } from './DirPicker'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import {
  addDir,
  type Library,
  type LibraryScanStatus,
  libraryScanStatus,
  renameLibrary,
  startLibraryScan,
} from './libraries'
import { messageOf, sentenceOf } from './message'
import { NameClash } from './NameClash'
import { PathWithBreaks } from './PathWithBreaks'
import { Refusal } from './Refusal'
import { bringIntoView } from './reveal'
import { RemoveLibrary } from './RemoveLibrary'
import { ScanReport } from './ScanReport'
import { PageHeader, RemovalSection, Row, Section } from './settingsLayout'
import { WaitWords } from './WaitWords'
import { ROW_CELLS, useLineEnds } from './lineEnds'

export interface LibraryPageProps {
  library: Library
  /**
   * Every library the list holds, this one among them, so that a rename to
   * a name another holds is refused as it is typed; `undefined` while the
   * list is unread, when only the server refuses it.
   */
  libraries: readonly Pick<Library, 'id' | 'name'>[] | undefined
  /**
   * What the screen says above the page: a list that could not be read, the
   * report of a directory that left this library, or an answer that came once
   * the page it was asked from had gone.
   */
  notices?: ReactNode
  /**
   * The person was sent here by an action whose button went with the page
   * it was on, so the heading takes focus.
   */
  arrived?: boolean
  /** A directory was added here: its probe is the screen's to keep. */
  onAdded: (added: Added[]) => void
  /**
   * The list reloads on a rename, an added directory or a finished scan.
   * `quiet` is a scan that ended by itself: no press started that read.
   */
  onChanged: (quiet?: boolean) => void
  /**
   * The library was renamed to `name`, the server's answer, called just
   * before `onChanged`: its address is its name, which the screen follows.
   */
  onRenamed?: (name: string) => void
  /**
   * The library is gone. Its re-derive's report is the screen's to keep, since
   * this page is not drawn once the library is gone.
   */
  onRemoved: (name: string, answer: RemovalAnswer) => void
  /**
   * A write sent from this page was refused after the page had gone, with
   * the sentence that says so and names the library or the directory.
   */
  onLateRefusal?: (sentence: string) => void
  /**
   * A directory's link was followed, with its address: the directory's page
   * takes focus, since the link that had it may be hidden beside it.
   */
  onOpen?: (to: string) => void
}

/** The directories a library's status names as scanning, by id. */
function runningIds(status: LibraryScanStatus): ReadonlySet<number> {
  return new Set(
    status.dirs.filter((one) => one.running).map((one) => one.root_id),
  )
}

/** `n` of a thing, in the number the count is. Zero takes the plural. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

/** Save's words while a rename is out, and the width Save keeps for them. */
const SAVING = 'Saving…'

export function LibraryPage({
  library,
  libraries,
  notices,
  arrived = false,
  onAdded,
  onChanged,
  onRenamed,
  onRemoved,
  onLateRefusal,
  onOpen,
}: LibraryPageProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  /** Which of a directory's cells end a line where its row is drawn in two. */
  const cellEnds = useLineEnds<HTMLTableElement>(ROW_CELLS)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  const drawn = useDrawn()

  /** Why the page's last scan could not start, or its poll failed. */
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * The sentence of a 409: a scan is in flight. A slot of its own, because it
   * is true only until that scan ends, and is cleared then.
   */
  const [conflict, setConflict] = useState<string | undefined>()
  /** Why the last directory was not added. Drawn beside the picker it answers. */
  const [addFailure, setAddFailure] = useState<string | undefined>()
  /** Why the last rename was refused. Laid over what is under the header. */
  const [renameFailure, setRenameFailure] = useState<string | undefined>()
  const [renaming, setRenaming] = useState(false)
  /**
   * The name a save set, drawn as the title until the list read after the
   * save says it: the title takes focus as the editor closes, and is read
   * out then. `from` is the name the list said, so the saved one is drawn
   * only while the list still says that.
   */
  const [renamed, setRenamed] = useState<{ from: string; to: string }>()
  // biome-ignore lint/correctness/useExhaustiveDependencies: `library.name` is the trigger: the list has been read again, and its name is the one to draw.
  useEffect(() => {
    setRenamed(undefined)
  }, [library.name])
  const title = renamed?.from === library.name ? renamed.to : library.name
  const [adding, setAdding] = useState(false)
  const [name, setName] = useState(library.name)
  const [saving, setSaving] = useState(false)
  /** `saving`, once it has gone on long enough to be drawn. */
  const savingDrawn = useDrawnWait(saving)
  /**
   * Why the server would refuse the name typed, as the list judges it. Save
   * is held at once while there is one, since it is no wait: the server
   * still judges every save, against a list another admin may have changed.
   */
  const clash = libraryNameClash(name, libraries ?? [], library.id)
  /**
   * The clash as it is said: not while a refusal in the same words is
   * drawn, which an edit takes away, so that one sentence is never said
   * twice. Beside a refusal in other words it is said, since it is what
   * holds Save.
   */
  const saidClash = renameFailure === clash ? undefined : clash
  const clashNote = useId()
  const [removing, setRemoving] = useState(false)
  const [running, setRunning] = useState(false)
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | null>(null)
  /** The directories the library's status last named as scanning, by id. */
  const [scanning, setScanning] = useState<ReadonlySet<number>>(new Set())
  /** Every directory drawn as scanning since the scan this page watches began. */
  const covered = useRef<Set<number>>(new Set())
  useEffect(() => {
    for (const id of scanning) covered.current.add(id)
  }, [scanning])
  /** Bumped when a scan this page watched ends. */
  const [ended, setEnded] = useState(0)
  /** What each directory's status held when the scan this page watched ended. */
  const [reports, setReports] = useState<
    { path: string; outcome: ScanOutcome }[] | undefined
  >()
  /**
   * A ref as well as the flags: two presses before a render both read false.
   * One for the scan's start and one for the page's writes, so Save pressed
   * while a scan is starting is not dropped.
   */
  const startingRef = useRef(false)
  const writing = useRef(false)
  const misses = useRef(0)
  // Held in refs for the reason `RootRow` holds its own: the poll's interval
  // must not restart each time the parent hands over a fresh closure, nor
  // each time the list reads again.
  const changed = useRef(onChanged)
  changed.current = onChanged
  const dirs = useRef(library.dirs)
  dirs.current = library.dirs

  useEffect(() => {
    let cancelled = false
    libraryScanStatus(library.id)
      .then((status) => {
        if (cancelled || !status.running) return
        setElapsed(status.elapsed_secs)
        setScanning(runningIds(status))
        setRunning(true)
      })
      .catch(() => {
        // Silent, as `RootRow`'s mount read is: pressing Scan library is what
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
          setScanning(runningIds(status))
          if (status.running) return
          setRunning(false)
          setConflict(undefined)
          changed.current(true)
          setEnded((n) => n + 1)
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses.current += 1
          if (misses.current < POLL_MISSES_ALLOWED) return
          setRunning(false)
          setScanning(new Set())
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

  // A read that fails draws no report for that directory: the table still says
  // when it was last scanned.
  useEffect(() => {
    if (ended === 0) return
    let cancelled = false
    const scanned = covered.current
    Promise.all(
      dirs.current
        .filter((root) => scanned.has(root.id))
        .map((root) =>
          scanStatus(root.id).then(
            (one) =>
              one.report === null
                ? undefined
                : { path: root.path, outcome: one.report },
            () => undefined,
          ),
        ),
    ).then((found) => {
      if (cancelled) return
      const held = found.filter((one) => one !== undefined)
      setReports(held.length === 0 ? undefined : held)
    })
    return () => {
      cancelled = true
    }
  }, [ended])

  const scan = () => {
    if (startingRef.current) return
    startingRef.current = true
    setStarting(true)
    setFailure(undefined)
    setConflict(undefined)
    setReports(undefined)
    covered.current = new Set()
    misses.current = 0
    startLibraryScan(library.id)
      .then(() => {
        startingRef.current = false
        setStarting(false)
        setScanning(new Set(library.dirs.map((root) => root.id)))
        setRunning(true)
      })
      .catch((err: unknown) => {
        startingRef.current = false
        setStarting(false)
        if (!drawn.current) {
          onLateRefusal?.(
            `A scan of ${library.name} did not start (${sentenceOf(err)}).`,
          )
          return
        }
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
    if (name === '' || clash !== undefined || writing.current) return
    writing.current = true
    setSaving(true)
    setRenameFailure(undefined)
    renameLibrary(library.id, name)
      .then((answer) => {
        writing.current = false
        setSaving(false)
        setRenamed({ from: library.name, to: answer.name })
        closedBy.current = 'save'
        setRenaming(false)
        onRenamed?.(answer.name)
        onChanged()
      })
      .catch((err: unknown) => {
        writing.current = false
        setSaving(false)
        const sentence = sentenceOf(err)
        if (drawn.current) setRenameFailure(sentence)
        else
          onLateRefusal?.(
            `${library.name} was not renamed to ${name} (${sentence}).`,
          )
      })
  }

  /** Rejects on a refusal, which is what keeps the picker's typed path. */
  const add = (path: string) => {
    setAddFailure(undefined)
    return addDir(library.id, path).then(
      (added) => {
        setAdding(false)
        onAdded([added])
        onChanged()
      },
      (err: unknown) => {
        const sentence = sentenceOf(err)
        if (drawn.current) setAddFailure(sentence)
        else
          onLateRefusal?.(
            `${path} was not added to ${library.name} (${sentence}).`,
          )
        throw err
      },
    )
  }

  const renameForm = useId()
  const nameBox = useRef<HTMLInputElement>(null)
  const renameButton = useRef<HTMLButtonElement>(null)
  const saveButton = useRef<HTMLButtonElement>(null)
  const cancelRenameButton = useRef<HTMLButtonElement>(null)
  useFocusLeavesHeld(clash !== undefined || savingDrawn, saveButton, nameBox)
  // The wait holds Cancel too, which a person may have tabbed to.
  useFocusLeavesHeld(savingDrawn, cancelRenameButton, nameBox)
  /** How the editor was closed, which says where focus goes next. */
  const closedBy = useRef<'save' | 'cancel' | undefined>(undefined)
  // The button pressed is hidden by what it opens or closes, so focus
  // follows it. Into the box, with the name selected so that typing replaces
  // it. Back to Rename…, shown again in the slot Cancel leaves, after Cancel
  // or Escape.
  // To the title, which reads the name out, after a save, and after a Cancel
  // whose Rename… the removal holds. Each is brought out from under the
  // phone bar: `focus` scrolls only what is outside the window, and the bar
  // covers its top.
  useEffect(() => {
    const show = (el: HTMLElement | null) => {
      el?.focus({ preventScroll: true })
      bringIntoView(el)
    }
    if (renaming) {
      show(nameBox.current)
      nameBox.current?.select()
      return
    }
    const by = closedBy.current
    closedBy.current = undefined
    if (by === undefined) return
    const back = renameButton.current
    if (by === 'cancel' && back !== null && !back.disabled) back.focus()
    else show(heading.current)
  }, [renaming])
  // Held from the moment a save is sent, before Cancel is drawn held, so
  // that nothing closes the editor under a save on its way.
  const closeRename = () => {
    if (writing.current) return
    setRenameFailure(undefined)
    closedBy.current = 'cancel'
    setRenaming(false)
  }
  /** Escape from the box, Save or Cancel closes the editor, as Cancel does. */
  const closeOnEscape = (e: React.KeyboardEvent) => {
    // An Escape that closes an input method's candidates is that method's,
    // not the editor's.
    if (e.key !== 'Escape' || e.nativeEvent.isComposing) return
    e.preventDefault()
    closeRename()
  }

  const addRoot = useRef<HTMLDivElement>(null)
  const addingWas = useRef(adding)
  // The button pressed is replaced by what it opens or closes, so focus
  // follows it: into the path box when the picker opens, and back to
  // Add directory… when it closes.
  useEffect(() => {
    if (addingWas.current === adding) return
    addingWas.current = adding
    addRoot.current
      ?.querySelector<HTMLElement>(
        adding ? 'input[name="path"]' : 'button[name="add-dir"]',
      )
      ?.focus()
  }, [adding])

  const removeButton = useRef<HTMLButtonElement>(null)
  /**
   * Set when the removal's confirm is cancelled: focus goes back to the
   * Remove… that opened it, not to the document, where the confirm's Cancel
   * leaves it as it goes. It takes focus whether or not a scan still holds
   * it, as a held `HeldButton` does.
   */
  const backToRemove = useRef(false)
  useEffect(() => {
    if (removing || !backToRemove.current) return
    backToRemove.current = false
    removeButton.current?.focus()
  }, [removing])

  return (
    <div className="library-page">
      <PageHeader
        back={{ to: LIBRARIES, label: 'Libraries' }}
        title={title}
        titleRef={heading}
        titleEditor={
          renaming ? (
            <form
              className="rename-library page-title-editor"
              id={renameForm}
              onSubmit={rename}
            >
              {/* O175: no label is drawn in the title's place, so the
                  box's name is said here. */}
              <input
                ref={nameBox}
                type="text"
                className="page-title-box"
                name="library-name"
                aria-label="Name"
                aria-invalid={saidClash === undefined ? undefined : true}
                aria-describedby={
                  saidClash === undefined ? undefined : clashNote
                }
                value={name}
                onChange={(e) => {
                  setRenameFailure(undefined)
                  setName(e.target.value)
                }}
                onKeyDown={closeOnEscape}
              />
            </form>
          ) : undefined
        }
        description={
          <span className="holds">
            {count(library.works, 'work', 'works')} in{' '}
            {count(library.dirs.length, 'directory', 'directories')}
          </span>
        }
        editorAnswer={
          <>
            {renameFailure !== undefined && <Refusal>{renameFailure}</Refusal>}
            <NameClash id={clashNote} sentence={saidClash} />
          </>
        }
        actions={
          <>
            <button
              ref={renameButton}
              type="button"
              name="rename-library"
              onClick={() => {
                setName(title)
                setRenaming(true)
              }}
              disabled={removing}
            >
              Rename…
            </button>
            {/* Scan library and Remove… are held by a scan and by a press of
                their own, either of which can come while the button has
                focus, so each is a `HeldButton` and keeps it. A press of the
                other holds it by `disabled`: focus is on the button pressed
                then, and a button that cannot take focus as its hold comes
                on stays out of the tab order. */}
            <HeldButton
              type="button"
              name="scan-library"
              onClick={scan}
              held={running || starting}
              disabled={removing}
            >
              Scan library
            </HeldButton>
          </>
        }
        editorActions={
          <>
            {/* Kept as wide as the wider of its words, so that drawing its
                wait moves nothing beside it. */}
            <button
              ref={saveButton}
              type="submit"
              form={renameForm}
              name="save-rename"
              className="keeps-wait-width"
              disabled={
                savingDrawn || name === '' || clash !== undefined || removing
              }
              onKeyDown={closeOnEscape}
            >
              <WaitWords drawn={savingDrawn} words="Save" wait={SAVING} />
            </button>
            <button
              ref={cancelRenameButton}
              type="button"
              name="cancel-rename"
              onClick={closeRename}
              onKeyDown={closeOnEscape}
              disabled={savingDrawn}
            >
              Cancel
            </button>
          </>
        }
      />
      {library.works === 0 && library.books > 0 && (
        <p className="no-readable">
          No readable files yet: its scans found{' '}
          {count(library.books, 'EPUB or PDF file', 'EPUB or PDF files')}, and
          reading those arrives with the books work. A rescan is all this
          library will need then.
        </p>
      )}
      {running && (
        <p className="scanning">
          A scan is running in this library…
          {elapsed === null ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}
      {notices}
      {failure !== undefined && <p role="alert">{failure}</p>}
      {conflict !== undefined && <p role="alert">{conflict}</p>}

      <Section title="Directories">
        <table ref={cellEnds} className="config-table dir-table">
          <thead>
            <tr>
              <th scope="col">Path</th>
              <th scope="col">Scan every</th>
              <th scope="col">Last scan</th>
            </tr>
          </thead>
          <tbody>
            {library.dirs.map((root) => {
              const to = dirPath(library.name, root.id)
              return (
                // By path, not id: a freed `root.id` is given to the next
                // directory added, and a path cannot collide, since no two
                // directories overlap.
                <tr key={root.path}>
                  <td className="lead">
                    {removing ? (
                      <code>
                        <PathWithBreaks path={root.path} />
                      </code>
                    ) : (
                      // A NavLink, so the row whose directory is open beside
                      // this page says so: `aria-current="page"`.
                      <NavLink
                        className="row-link"
                        to={to}
                        onClick={() => onOpen?.(to)}
                      >
                        <code>
                          <PathWithBreaks path={root.path} />
                        </code>
                      </NavLink>
                    )}
                  </td>
                  <td>
                    <span className="cell-label">Scan every: </span>
                    {root.scan_interval_secs === null
                      ? 'not set'
                      : humanInterval(root.scan_interval_secs)}
                  </td>
                  <td>
                    <span className="cell-label">Last scan: </span>
                    {scanning.has(root.id) ? 'Scanning…' : lastScan(root)}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
        <div className="add-root" ref={addRoot}>
          {adding ? (
            <DirPicker
              action="Add directory"
              onPick={add}
              disabled={removing}
              onEdit={() => setAddFailure(undefined)}
            >
              <button
                type="button"
                name="cancel-add"
                onClick={() => {
                  setAddFailure(undefined)
                  setAdding(false)
                }}
              >
                Cancel
              </button>
            </DirPicker>
          ) : (
            <button
              type="button"
              name="add-dir"
              onClick={() => setAdding(true)}
              disabled={removing}
            >
              Add directory…
            </button>
          )}
          {addFailure !== undefined && <Refusal>{addFailure}</Refusal>}
        </div>
      </Section>

      {reports !== undefined && (
        <Section title="What the scan found">
          {reports.map((one) => (
            <div key={one.path} className="dir-report">
              <h4>
                <code>{one.path}</code>
              </h4>
              <ScanReport outcome={one.outcome} level={5} />
            </div>
          ))}
        </Section>
      )}

      <RemovalSection title="Removal">
        <Row
          label="Remove this library"
          help="Asks first, with what it would take."
        >
          {/* Held as Scan library is, and for the same reasons. */}
          <HeldButton
            ref={removeButton}
            type="button"
            name="remove-library"
            onClick={() => setRemoving(true)}
            held={running || removing}
            disabled={starting}
          >
            Remove…
          </HeldButton>
        </Row>
        {removing && (
          <RemoveLibrary
            library={library}
            blocked={running}
            onCancel={() => {
              backToRemove.current = true
              setRemoving(false)
            }}
            onRemoved={(answer) => {
              setRemoving(false)
              onRemoved(library.name, answer)
            }}
            onLateRefusal={onLateRefusal}
          />
        )}
      </RemovalSection>
    </div>
  )
}

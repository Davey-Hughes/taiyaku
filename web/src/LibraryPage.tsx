/**
 * One library's page: its name, what it holds, what is done to it whole, and
 * a table of its directories, each of which has a page of its own.
 *
 * **The page owns the library's writes and a directory's page owns the
 * directory's.** A rename, an added directory, a scan of the whole library and
 * its removal are sent from here. A directory's settings, its own scan, its
 * move and its removal are `RootRow`'s, on `DirPage`.
 *
 * **Its body waits for the library's scan status** (`pageReads.ts`'s
 * `libraryScan`), for `RootRow`'s reason: a scan started before this page
 * was drawn is still running, and the first body of a first visit says so
 * (spec §4.5). A return draws its body at once from the status kept, and
 * reads it again (below), where that status is not stale; where it is, the
 * return waits for its read as a first visit does. A press of Scan library
 * answered once the page had gone leaves it stale, and so does a directory's
 * own scan, which the directory's page marks (`DirPage`). While the body
 * waits, and while the screen reads the list that names the library, the
 * page draws its header, named as its address spells it until the list names
 * the library and as the list does then, and "Loading…" in its body's place
 * once the wait has held. That header is the one the drawn page keeps, so
 * whatever has focus in it keeps it as the body comes. A status that cannot
 * be read, with none kept, is drawn as no scan running, as `RootRow`'s is:
 * pressing Scan library is what surfaces a real fault; a failed re-read
 * keeps the status kept. The status says whether any directory a scan of the
 * library walks is scanning: its own, and each directory inside them that it
 * shows, which the list names (`LibraryDir`'s `nested`). So a directory's
 * own scan, in a status this page reads, holds its Scan library and Remove…
 * as a scan of the library does, and the directories it names as running say
 * "Scanning…" in the table. A scan running only in directories inside them,
 * which the table does not list, is said a line for each library whose
 * request the status names (`scans`), with the topmost folders it walks
 * here: it is that library's scan, not this one's, which holds its own
 * directories too. Scan library is held all the same, since the server
 * refuses it while any directory it walks is being scanned. A page drawn
 * beside the directory's is not told of that scan and reads nothing for it,
 * so it keeps both free: told, it would take the scan up and draw its report
 * a second time beside the directory's own (spec §9's call 9).
 *
 * **A running scan is polled through the store, one read at a time.** The
 * next read is sent a poll's interval after the last has answered: the store
 * keeps only the newest request's answer, so polls that overlapped could each
 * be overtaken and leave the kept status behind. The status read again on a
 * return lands whole, in one step: a scan it finds is taken up as one found
 * on arrival is, and one the kept status said was running is drawn ended or
 * further on, as it says. An end that this visit's first read of the status
 * finds draws no report, reads no list and marks no Library view: that scan
 * ended before this visit, and `RootRow`'s mount read adopts only a running
 * scan. A scan the page started itself is followed by its poll alone.
 *
 * **A 409 from Scan library is proof a scan is in flight**, of the library or
 * of one directory, so the page draws the sentence and polls. The sentence goes
 * when that scan ends.
 *
 * **The reports of a scan this page watched are drawn here when it ends.**
 * A report is held by its directory's own status, which this page does not
 * otherwise read, so once the library's says nothing is running the status
 * of each directory the page saw the scan hold is read, and whatever report
 * each holds is drawn under its path, a directory inside one of the
 * library's among them. Only those: any other directory's status
 * holds a report from another scan, if any, and a directory that joined the
 * library while the scan ran is not in it.
 *
 * **Scans and removals exclude each other.** Scan library and Remove… are
 * held while a scan is known, starting, or the removal dialog is open; the
 * removal's confirm is held while a scan is known. Remove… and the confirm
 * are held too while the status names a removal waiting for a scan around or
 * inside one of the library's directories (`held_removals`), which the
 * server refuses the removal for; the status is read again each poll's
 * interval until it names none. A scan holds neither Rename… nor Add
 * directory…; the removal holds both, and the Directories table draws its
 * paths without links while it is open: a directory's page offers Scan
 * directory, Move… and Remove… on a directory the removal is taking.
 */
import {
  Fragment,
  type ReactNode,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import { NavLink, useLocation } from 'react-router'
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
import { dirPath, LIBRARIES, libraryNameClash, nameAt } from './configAddress'
import { DirPicker } from './DirPicker'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import {
  addDir,
  folderIn,
  type Library,
  type LibraryScanStatus,
  type NestedDir,
  renameLibrary,
  startLibraryScan,
} from './libraries'
import { messageOf, sentenceOf } from './message'
import { NameClash } from './NameClash'
import { invalidateLibraryViews, libraryScan, ROOTS } from './pageReads'
import { PathWithBreaks } from './PathWithBreaks'
import { Refusal } from './Refusal'
import { entryOf, invalidate, readNow, readSet } from './reads'
import { bringIntoView } from './reveal'
import { RemoveLibrary } from './RemoveLibrary'
import { ScanReport } from './ScanReport'
import {
  PageHeader,
  PageLoading,
  RemovalSection,
  Row,
  Section,
} from './settingsLayout'
import { usePage } from './usePage'
import { WaitWords } from './WaitWords'
import { ROW_CELLS, useLineEnds } from './lineEnds'

export interface LibraryPageProps {
  /**
   * `undefined` while the screen reads the list that names the library, or
   * could not read it.
   */
  library: Library | undefined
  /**
   * The list's wait has held long enough to draw "Loading…" for it: drawn
   * while `library` is `undefined`. The wait for the scan status is the
   * page's own.
   */
  loadingDrawn?: boolean
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
   * Library `id` was renamed to `name`, the server's answer, called just
   * before `onChanged`: its address is its name, which the screen follows.
   * The page names its library, since the screen draws the page before the
   * list it finds the library in has been read.
   */
  onRenamed?: (id: number, name: string) => void
  /**
   * Library `id`, called `name`, is gone. Its re-derive's report is the
   * screen's to keep, since this page is not drawn once the library is gone.
   */
  onRemoved: (id: number, name: string, answer: RemovalAnswer) => void
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

/**
 * A scan the page watches, numbered from 1, and what it learnt of it from:
 * `kept`, the status an earlier visit kept, which a return draws its body
 * from at once, until a read of this visit finds the scan still running;
 * `status`, a status this visit read; or `press`, its own press of Scan
 * library (the start, or a 409).
 */
interface Watch {
  readonly n: number
  readonly from: 'kept' | 'status' | 'press'
}

/**
 * The directories a scan of `library` walks, as the list names them: its own,
 * and each directory inside them that it shows. The server decides which it
 * shows; this only gathers what it said.
 */
function walkedBy(library: Library): { id: number; path: string }[] {
  return [...library.dirs, ...shownInside(library)].map(({ id, path }) => ({
    id,
    path,
  }))
}

/**
 * The directories inside `library`'s own that it shows, each with the path of
 * the one it sits in, `outer`.
 */
function shownInside(library: Library): (NestedDir & { outer: string })[] {
  return library.dirs.flatMap((root) =>
    root.nested
      .filter((one) => one.shown)
      .map((one) => ({ ...one, outer: root.path })),
  )
}

/**
 * One line's worth for each of `scans` that walks directories inside
 * `library`'s own: the library that asked for it, falling back to the
 * directory's own where none is named, and the folders it walks here, the
 * topmost only, each once, so two of its directories of one name inside two
 * of this library's are said once.
 */
function scansInside(
  library: Library,
  scans: LibraryScanStatus['scans'],
): { key: string; by: string; folders: string[] }[] {
  const inside = shownInside(library)
  return scans.flatMap((scan) => {
    const walked = inside.filter((one) => scan.root_ids.includes(one.id))
    const topmost = walked.filter(
      (one) => !walked.some((other) => one.path.startsWith(`${other.path}/`)),
    )
    if (topmost.length === 0) return []
    const by = scan.library ?? topmost[0].library
    const folders = [
      ...new Set(topmost.map((one) => folderIn(one.path, one.outer))),
    ]
    return [{ key: `${by}\u0000${folders.join('\u0000')}`, by, folders }]
  })
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
  loadingDrawn = false,
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
  /**
   * The name the address spells, which heads the page until the list names
   * the library.
   */
  const named = nameAt(useLocation().pathname, LIBRARIES)?.name

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
  }, [library?.name])
  const [adding, setAdding] = useState(false)
  /**
   * What the name box holds once Rename… has opened it, and the library's
   * own name before that: the page is drawn before the list names it.
   */
  const [typed, setName] = useState<string | undefined>()
  const name = typed ?? library?.name ?? ''
  const [saving, setSaving] = useState(false)
  /** `saving`, once it has gone on long enough to be drawn. */
  const savingDrawn = useDrawnWait(saving)
  /**
   * Why the server would refuse the name typed, as the list judges it. Save
   * is held at once while there is one, since it is no wait: the server
   * still judges every save, against a list another admin may have changed.
   */
  const clash = libraryNameClash(name, libraries ?? [], library?.id)
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
  /** The scans the library's status last named, by the library that asked. */
  const [scans, setScans] = useState<LibraryScanStatus['scans']>([])
  /**
   * Whether the status last read held the library's removal for a scan
   * around or inside one of its directories (`held_removals`), which the
   * server refuses the removal for.
   */
  const [removalHeld, setRemovalHeld] = useState(false)
  /** Bumped as each read made for `removalHeld` settles, to ask again. */
  const [heldReads, setHeldReads] = useState(0)
  /** Consecutive failed reads made for `removalHeld`. */
  const heldMisses = useRef(0)
  /** Every directory drawn as scanning since the scan this page watches began. */
  const covered = useRef<Set<number>>(new Set())
  useEffect(() => {
    for (const id of scanning) covered.current.add(id)
  }, [scanning])
  /**
   * The scan the page watches. While it came from a status, kept or read,
   * each newer status is taken whole; one the page started is followed by
   * its poll alone.
   */
  const [watch, setWatch] = useState<Watch>({ n: 0, from: 'status' })
  /**
   * The number of the watched scan that last ended. A number rather than a
   * count, so that a scan the store and the poll both say has ended ends
   * once.
   */
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
  /** The poll's reads sent by this mount, for a read the poll overtook. */
  const pollsSent = useRef(0)
  // Held in refs for the reason `RootRow` holds its own: the poll's interval
  // must not restart each time the parent hands over a fresh closure, nor
  // each time the list reads again.
  const changed = useRef(onChanged)
  changed.current = onChanged
  const watchNow = useRef(watch)
  watchNow.current = watch
  const walked = useRef(library === undefined ? [] : walkedBy(library))
  walked.current = library === undefined ? [] : walkedBy(library)

  const libraryId = library?.id
  // Its address is known only once the list has named the library (§4.5),
  // and the body waits for it.
  const {
    ready,
    answers: [status],
    loadingDrawn: statusWaitDrawn,
  } = usePage<[LibraryScanStatus]>([
    libraryId === undefined ? undefined : libraryScan(libraryId),
  ])
  /**
   * The page waited for this visit's read of its scan status, so the status
   * its body is first drawn from is one this visit read, not one an earlier
   * visit kept.
   */
  const [waited, setWaited] = useState(false)
  if (library !== undefined && !ready && !waited) setWaited(true)
  /**
   * Ends `ending` once, whichever of the store and the poll says so first.
   * A scan taken up from a kept status and found over by this visit's first
   * read of it ended before this visit: it is drawn as not running, with no
   * report and no read of the list, as `RootRow`'s mount read adopts only a
   * running scan.
   */
  const endWatch = (ending: Watch) => {
    setRunning(false)
    setConflict(undefined)
    if (ending.from !== 'kept') setEnded(ending.n)
  }
  /**
   * The scan status the page last took. The one its body is first drawn
   * from says whether a scan is running, and so does each read after it:
   * the read a return sends, and each of the poll's, which comes here
   * through the store as well as to the poll. A scan it finds is watched;
   * one watched from a status is drawn as each newer status says, ended or
   * further on, in the render that status lands. An answer in the words of
   * the status taken keeps the answer the store holds, the same object, so
   * it comes here not at all.
   */
  const [taken, setTaken] = useState<LibraryScanStatus | undefined>()
  if (ready && status !== taken) {
    setTaken(status)
    setRemovalHeld((status?.held_removals.length ?? 0) > 0)
    if (running && watch.from !== 'press') {
      if (status !== undefined) {
        setElapsed(status.elapsed_secs)
        setScanning(runningIds(status))
        setScans(status.scans)
        if (!status.running) endWatch(watch)
        else if (watch.from === 'kept') setWatch({ ...watch, from: 'status' })
      }
    } else if (status?.running === true && !running) {
      setElapsed(status.elapsed_secs)
      setScanning(runningIds(status))
      setScans(status.scans)
      setRunning(true)
      setWatch({
        n: watch.n + 1,
        // The first status of a return that did not wait is the kept one.
        from: !waited && taken === undefined ? 'kept' : 'status',
      })
    }
  }

  /** The watched scan's number while it is one taken up from a kept status. */
  const keptWatch = watch.from === 'kept' ? watch.n : undefined
  // This visit saw a scan taken up from a kept status running once its read
  // of the status has settled saying so. An answer in the kept status's own
  // words keeps the answer the store holds, the same object, so `taken`
  // cannot see it; the read is joined here instead. It is out, as
  // `usePage`'s effect sent it in the commit that took the scan up, so
  // joining it sends nothing. A read that failed saw nothing, and so did
  // one the store dropped for a newer request: while the watch is kept only
  // the poll sends one, so a poll sent since the join means the entry is
  // not this read's answer. One that found the scan over ends it through
  // `taken` with no report, which a move made here before that render
  // would turn into a report.
  useEffect(() => {
    if (keptWatch === undefined || libraryId === undefined) return
    const scan = libraryScan(libraryId)
    const pollsBefore = pollsSent.current
    let gone = false
    void readSet([scan]).then(() => {
      const entry = entryOf(scan)
      if (
        gone ||
        pollsSent.current !== pollsBefore ||
        entry?.failure !== undefined ||
        entry?.answer?.running !== true
      ) {
        return
      }
      setWatch((was) =>
        was.n === keptWatch && was.from === 'kept'
          ? { ...was, from: 'status' }
          : was,
      )
    })
    return () => {
      gone = true
    }
  }, [keptWatch, libraryId])

  // biome-ignore lint/correctness/useExhaustiveDependencies: `endWatch` is made anew each render and calls only state setters, which never change.
  useEffect(() => {
    if (!running || libraryId === undefined) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    // Called as the watch starts and once the last read has answered, never
    // on a clock of its own.
    const next = () => {
      timer = setTimeout(() => {
        pollsSent.current += 1
        readNow(libraryScan(libraryId))
          .then((now) => {
            if (cancelled) return
            misses.current = 0
            setElapsed(now.elapsed_secs)
            setScanning(runningIds(now))
            setScans(now.scans)
            setRemovalHeld(now.held_removals.length > 0)
            if (now.running) {
              setWatch((was) =>
                was.from === 'kept' ? { ...was, from: 'status' } : was,
              )
              next()
              return
            }
            endWatch(watchNow.current)
          })
          .catch((err: unknown) => {
            if (cancelled) return
            misses.current += 1
            if (misses.current < POLL_MISSES_ALLOWED) {
              next()
              return
            }
            setRunning(false)
            setScanning(new Set())
            setScans([])
            // The 409's sentence is true while the scan it named is watched,
            // and nothing watches it now.
            setConflict(undefined)
            setFailure(sentenceOf(err))
          })
      }, SCAN_POLL_MS)
    }
    next()
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [running, libraryId])

  // A scan around or inside one of the library's directories holds its
  // removal and is no scan this page watches, so while it holds and none of
  // the library's runs, the status is read again each poll's interval. The
  // read lands in the store, which hands it to `taken`; the count asks again
  // whether or not that answer changed anything. Failed reads give the hold
  // up as the main poll does, and say so.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `heldReads` is what asks again once a read has settled.
  useEffect(() => {
    if (!removalHeld || running || libraryId === undefined) return
    let cancelled = false
    const timer = setTimeout(() => {
      readNow(libraryScan(libraryId)).then(
        () => {
          if (cancelled) return
          heldMisses.current = 0
          setHeldReads((n) => n + 1)
        },
        (err: unknown) => {
          if (cancelled) return
          heldMisses.current += 1
          if (heldMisses.current < POLL_MISSES_ALLOWED) {
            setHeldReads((n) => n + 1)
            return
          }
          heldMisses.current = 0
          setRemovalHeld(false)
          setFailure(sentenceOf(err))
        },
      )
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [removalHeld, running, libraryId, heldReads])

  // The list is read again for what the scan changed, quietly: no press
  // started that read. A read that fails draws no report for that directory:
  // the table still says when it was last scanned. What the scan found
  // changes what the Library views draw, so they are marked too.
  useEffect(() => {
    if (ended === 0) return
    changed.current(true)
    invalidateLibraryViews()
    let cancelled = false
    const scanned = covered.current
    Promise.all(
      walked.current
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

  if (library === undefined || !ready) {
    return (
      <div className="library-page">
        {/* Keyed, as in the drawn page, so that each is the same element
            there whatever the drawn page puts between them. */}
        <PageHeader
          key="header"
          back={{ to: LIBRARIES, label: 'Libraries' }}
          title={library?.name ?? named}
          titleRef={heading}
        />
        <Fragment key="notices">{notices}</Fragment>
        <PageLoading
          drawn={library === undefined ? loadingDrawn : statusWaitDrawn}
        />
      </div>
    )
  }

  const title = renamed?.from === library.name ? renamed.to : library.name
  /**
   * While none of the library's own directories is scanning, a line for each
   * library whose scan walks directories inside them, which the table does
   * not list: that library's name and the folders it walks here, the topmost
   * only, each once.
   */
  const scanLines = library.dirs.some((root) => scanning.has(root.id))
    ? []
    : scansInside(library, scans)

  const scan = () => {
    if (startingRef.current) return
    startingRef.current = true
    setStarting(true)
    setFailure(undefined)
    setConflict(undefined)
    setReports(undefined)
    covered.current = new Set()
    misses.current = 0
    // A start, or a 409, which is proof of a scan, leaves the status the
    // store keeps saying none is running until it is read again, and a
    // return before the first poll would draw that.
    const keptStatusStale = () => void invalidate(libraryScan(library.id))
    startLibraryScan(library.id)
      .then((started) => {
        keptStatusStale()
        startingRef.current = false
        setStarting(false)
        // The server's own list: the page's can predate a pattern edit or a
        // nested add, and a directory it names that the scan did not walk
        // would have an earlier report drawn as this scan's.
        setScanning(new Set(started.root_ids))
        setScans([])
        setRunning(true)
        setWatch((was) => ({ n: was.n + 1, from: 'press' }))
      })
      .catch((err: unknown) => {
        startingRef.current = false
        setStarting(false)
        const conflict = statusOf(messageOf(err)) === 409
        if (conflict) keptStatusStale()
        if (!drawn.current) {
          onLateRefusal?.(
            `A scan of ${library.name} did not start (${sentenceOf(err)}).`,
          )
          return
        }
        if (conflict) {
          setConflict(sentenceOf(err))
          setRunning(true)
          setWatch((was) => ({ n: was.n + 1, from: 'press' }))
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
        onRenamed?.(library.id, answer.name)
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
        // A new root, which Rules draws.
        void invalidate(ROOTS)
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

  return (
    <div className="library-page">
      <PageHeader
        key="header"
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
      {running && scanLines.length === 0 && (
        <p className="scanning">
          A scan is running in this library…
          {elapsed === null ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}
      {running &&
        scanLines.map((line) => (
          <p key={line.key} className="scanning">
            {line.by} is scanning{' '}
            {line.folders.map((folder, i) => (
              <Fragment key={folder}>
                {i === 0 ? '' : i === line.folders.length - 1 ? ' and ' : ', '}
                <code>
                  <PathWithBreaks path={folder} />
                </code>
              </Fragment>
            ))}
            , which {library.name} shows.
          </p>
        ))}
      <Fragment key="notices">{notices}</Fragment>
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
            held={running || removalHeld || removing}
            disabled={starting}
          >
            Remove…
          </HeldButton>
        </Row>
        {removing && (
          <RemoveLibrary
            library={library}
            blocked={running || removalHeld}
            onCancel={() => {
              backToRemove.current = true
              setRemoving(false)
            }}
            onRemoved={(answer) => {
              setRemoving(false)
              onRemoved(library.id, library.name, answer)
            }}
            onLateRefusal={onLateRefusal}
          />
        )}
      </RemovalSection>
    </div>
  )
}

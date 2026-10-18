/**
 * The libraries screen: Configuration → Libraries, and the pages beneath it.
 *
 * **Four pages over one list.** The table of libraries, the form that makes
 * one, a library's page and a folder's page each read the list this screen
 * holds and pick what they draw from it by id; there is no route that answers
 * one library. The list is read when the screen mounts, again on each move
 * between its pages, and after every write.
 *
 * **Two failure slots.** The list's failure is the screen's and is drawn on
 * whichever page is showing; a refusal is drawn in the form that was sent,
 * while that form is drawn.
 *
 * **The probe from an add is kept, by the folder's path.** The watch probe
 * runs when a folder is added and at no other time, so a folder added in
 * this session has a watch state and one read from the list does not.
 *
 * **A removal's re-derive report, or a move's, is kept here, and drawn one
 * level up.** What was removed or moved has no page to draw it on, so the
 * report belongs to the page above it: a library's removal to the table, a
 * folder's to its library's page, or to the table where the library went with
 * its last folder. The screen goes up to that page when the answer comes,
 * if the person is still where they asked, and the report takes focus there:
 * the button that had it went with the page. The report is the one place a
 * lost position or an unwritable document is said.
 *
 * **An answer that comes once the person has gone to another page of the
 * screen is said on the page they are on**, and leaves focus where it is,
 * since nothing they were doing there went with it: a removal's or a move's
 * report, or the refusal of any write whose page or form went before its
 * answer came. A report said so lands in a polite live region drawn with the
 * page, since nothing takes focus to have it read. Whatever is said goes when
 * the person leaves the page it is drawn on. Once another tab has replaced the screen it is said
 * nowhere.
 *
 * **Reads carry a sequence number.** Several are routinely in flight, and
 * they need not answer in order.
 *
 * **A read of the list that is still the latest when it answers asks the
 * shell to read the status again**, whether it lands or fails, so the
 * sidebar's libraries follow every write: a removal whose re-read of the list
 * fails has still removed the library. An older read, which the sequence
 * guard drops, asks nothing.
 *
 * **What a write takes away is dropped from what is drawn at once**, and in
 * the same transition as the move to the page above it, so the page that
 * asked is never drawn over a library or folder that is gone.
 */
import {
  type ReactNode,
  startTransition,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import {
  Link,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
} from 'react-router'
import type { Added, Derived, Probe, RemovalAnswer } from './config'
import { FolderPage } from './FolderPage'
import {
  type Created,
  type Library,
  listLibraries,
  type MoveAnswer,
  type Moved,
} from './libraries'
import { LibraryPage } from './LibraryPage'
import { LibraryTable } from './LibraryTable'
import { MoveCounts } from './MoveCounts'
import { sentenceOf } from './message'
import { NewLibrary } from './NewLibrary'
import { reveal } from './reveal'
import { DeriveSection } from './ScanReport'
import { useStatusRefresh } from './shell'

const TABLE = '/config/libraries'
const libraryPath = (id: number) => `${TABLE}/${id}`

/** What a removal or a move found, whose it was, and the page it is drawn on. */
interface Report {
  at: string
  /** Taken when the answer itself moved the person to the report's page. */
  focus?: boolean
  name: string
  derive: Derived
  /** A move's: what it did, and where from. Absent for a removal's. */
  move?: { moved: Moved; from?: string; newLibrary: boolean }
}

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
  const [report, setReport] = useState<Report | undefined>()
  /**
   * The refusals whose form went before they came, each with the page it is
   * drawn on, and a number to tell two in the same words apart.
   */
  const [late, setLate] = useState<
    { at: string; sentence: string; n: number }[]
  >([])
  const lateCount = useRef(0)
  /**
   * The page a make or the make form's Cancel sent the person to. Its
   * heading takes focus once, since the button that had it went with the
   * form.
   */
  const [arrival, setArrival] = useState<string | undefined>()
  const [probes, setProbes] = useState<Record<string, Probe>>({})
  const latest = useRef(0)
  const { pathname } = useLocation()
  const navigate = useNavigate()

  const refreshStatus = useStatusRefresh()

  /** `quiet`: no press started this read, so its failure is not scrolled to. */
  const reload = useCallback(
    (quiet = false) => {
      const seq = ++latest.current
      listLibraries()
        .then((body) => {
          if (seq !== latest.current) return
          setLibraries(body.libraries)
          refreshStatus()
          setFailure(undefined)
        })
        .catch((err: unknown) => {
          if (seq !== latest.current) return
          setFailure({ sentence: sentenceOf(err), reveal: !quiet })
          refreshStatus()
        })
    },
    [refreshStatus],
  )

  // biome-ignore lint/correctness/useExhaustiveDependencies: `pathname` is the trigger: each page reads the list afresh.
  useEffect(() => {
    reload()
  }, [reload, pathname])

  /** The page drawn now, for an answer that arrives after a page change. */
  const here = useRef(pathname)
  here.current = pathname
  /** False once another Configuration tab has replaced this screen. */
  const mounted = useRef(false)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  /**
   * True while a removal's or a move's answer is being taken. `RootRow` calls
   * `onChanged` in the same call as `onRemoved` or `onMoved`, and the answer
   * has already arranged the one read that follows the write, so that
   * `onChanged` reads nothing. It is cleared when the current task's
   * microtasks run, before any later `onChanged` can come.
   */
  const answering = useRef(false)

  const folderChanged = useCallback(
    (quiet?: boolean) => {
      if (!answering.current) reload(quiet)
    },
    [reload],
  )

  /**
   * Takes the answer to a removal or a move started on the page `from`: its
   * report, and what the write took out of the list, at once. The person is
   * sent to the report's page, `next.at`, only while on `from`, by path: the
   * folder is gone, so a folder page left and opened again goes up too. An
   * answer that comes after they went elsewhere leaves them where they are,
   * with the report drawn on that page. The list is read once either way: by
   * the change of page, or here.
   */
  const answered = useCallback(
    (from: string, next: Report, drop: (held: Library[]) => Library[]) => {
      answering.current = true
      queueMicrotask(() => {
        answering.current = false
      })
      const sent = mounted.current && here.current === from
      startTransition(() => {
        setReport(
          sent
            ? { ...next, focus: true }
            : { ...next, at: here.current, focus: false },
        )
        setLibraries((held) => held && drop(held))
        if (sent) navigate(next.at)
      })
      if (!sent) reload(true)
    },
    [navigate, reload],
  )

  /**
   * A refusal whose form is gone, said on the page the person is on. Once
   * another tab has replaced the screen there is no page to say it on.
   */
  const say = useCallback((sentence: string) => {
    const n = ++lateCount.current
    setLate((held) => [...held, { at: here.current, sentence, n }])
  }, [])

  // What is said belongs to the page it was drawn on, and so does an
  // arrival: a page opened again later is opened by the person.
  useEffect(() => {
    setReport((held) =>
      held !== undefined && held.at !== pathname ? undefined : held,
    )
    setLate((held) =>
      held.every((one) => one.at === pathname)
        ? held
        : held.filter((one) => one.at === pathname),
    )
    setArrival((held) => (held === pathname ? held : undefined))
  }, [pathname])

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

  /** What is drawn, for the decisions a removal makes from it. */
  const drawn = useRef<Library[] | undefined>(undefined)
  drawn.current = libraries

  /**
   * A library was made. The person is taken to its page only while the form
   * that asked is still drawn; an answer that comes after they left it, even
   * for a fresh form at the same address, leaves them where they are. The
   * list is read once either way, by the change of page or here, so the table
   * and the sidebar show it.
   */
  const made = useCallback(
    (created: Created, sent: boolean) => {
      remember(created.added)
      startTransition(() => {
        // Drawn from the create's own answer: if the read that follows
        // fails, the list still holds what was just made. It goes last; the
        // read puts it where the server's own collation does.
        setLibraries((held) =>
          held === undefined
            ? [created.library]
            : [
                ...held.filter((one) => one.id !== created.library.id),
                created.library,
              ],
        )
        if (sent) {
          setReport(undefined)
          setArrival(libraryPath(created.library.id))
          navigate(libraryPath(created.library.id))
        }
      })
      if (!sent) reload(true)
    },
    [remember, navigate, reload],
  )

  const cancel = useCallback(() => {
    startTransition(() => {
      setArrival(TABLE)
      navigate(TABLE)
    })
  }, [navigate])

  const libraryRemoved = useCallback(
    (from: string, id: number, name: string, answer: RemovalAnswer) => {
      answered(from, { at: TABLE, name, derive: answer.derive }, (held) =>
        held.filter((one) => one.id !== id),
      )
    },
    [answered],
  )

  /**
   * A folder was removed from a library. A folder that was the last of its
   * library's took the library with it: the report is then named for the
   * library, which is what went, and drawn on the table.
   */
  const folderRemoved = useCallback(
    (from: string, id: number, path: string, answer: RemovalAnswer) => {
      const library = drawn.current?.find((one) => one.id === id)
      const left = library?.folders.filter((one) => one.path !== path)
      if (library !== undefined && left?.length === 0) {
        libraryRemoved(from, id, library.name, answer)
        return
      }
      answered(
        from,
        { at: libraryPath(id), name: path, derive: answer.derive },
        (held) =>
          held.map((one) =>
            one.id === id
              ? { ...one, folders: one.folders.filter((f) => f.path !== path) }
              : one,
          ),
      )
    },
    [libraryRemoved, answered],
  )

  /**
   * A folder was moved to another library. The report goes to the library it
   * left, or to the table where the move emptied and removed that library. It
   * is named for the folder and the library it went to.
   */
  const folderMoved = useCallback(
    (from: string, id: number, path: string, answer: MoveAnswer) => {
      const gone = answer.moved.library_removed !== null
      answered(
        from,
        {
          at: gone ? TABLE : libraryPath(id),
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
        },
        // The folder is not drawn in its new library: the reload does that.
        (held) =>
          gone
            ? held.filter((one) => one.id !== id)
            : held.map((one) =>
                one.id === id
                  ? {
                      ...one,
                      folders: one.folders.filter((f) => f.path !== path),
                    }
                  : one,
              ),
      )
    },
    [answered],
  )

  const reportEl = useRef<HTMLDivElement>(null)
  const reportLabel = useId()
  const shown = report?.at === pathname ? report : undefined
  const lateShown = late.filter((one) => one.at === pathname)
  // A report the answer moved the person to replaces the page they were
  // acting on, and focus was on a button that no longer exists. Focusing the
  // report has its name read. Focus alone scrolls only as far as the nearest
  // edge, which for a report taller than the window is its foot, so its head
  // is put at the top of the view, clear of the phone bar, as well, when it
  // is not already in view. Taken once per report: the list reading again
  // leaves focus where it is. A report drawn on a page the person went to
  // themselves is neither focused nor scrolled to: they are doing something
  // else there.
  useEffect(() => {
    const el = reportEl.current
    if (shown?.focus !== true || el === null) return
    el.focus({ preventScroll: true })
    reveal(el)
  }, [shown])
  // The list's failure is drawn at the head of the page, and a person acting
  // further down it is not looking there. A failed read is brought into view,
  // which does nothing where it already is, except the read at the end of any
  // scan: the page would move from under whoever is typing by then. Focus
  // stays where it is.
  useEffect(() => {
    const el = failureEl.current
    if (failure?.reveal && el !== null) reveal(el)
  }, [failure])

  const reportNode = shown !== undefined && (
    <div
      ref={reportEl}
      className="removal-report scan-report"
      tabIndex={-1}
      role="group"
      aria-labelledby={reportLabel}
    >
      <p id={reportLabel}>
        {shown.move === undefined
          ? `What re-deriving after removing ${shown.name} found:`
          : `Moved ${shown.name}${
              shown.move.from === undefined ? '' : ` from ${shown.move.from}`
            } to ${shown.move.moved.library}.`}
      </p>
      {shown.move !== undefined && (
        <>
          <MoveCounts
            moved={shown.move.moved}
            past
            newLibrary={shown.move.newLibrary}
          />
          <p>What deriving after the move found:</p>
        </>
      )}
      <DeriveSection derive={shown.derive} />
    </div>
  )

  const notices: ReactNode = (
    <>
      {failure !== undefined && (
        <p role="alert" ref={failureEl}>
          {libraries === undefined
            ? failure.sentence
            : `The libraries list could not be read again (${failure.sentence}), so what is drawn may be out of date.`}
        </p>
      )}
      {lateShown.map((one) => (
        <p key={one.n} role="alert">
          {one.sentence}
        </p>
      ))}
      {shown?.focus === true && reportNode}
      {/* A report that takes focus is read as it does, and is kept out of
          this region so it is not read twice. One that does not is read only
          by landing in a live region, and a region drawn together with what
          it says is often not read at all, so this one is drawn with the
          page, empty, before any answer can come. */}
      <div aria-live="polite">{shown?.focus !== true && reportNode}</div>
    </>
  )

  return (
    <section className="libraries">
      <Routes>
        <Route
          index
          element={
            <LibraryTable
              libraries={libraries}
              notices={notices}
              arrived={arrival === pathname}
            />
          }
        />
        <Route
          path="new"
          element={
            <NewLibrary
              notices={notices}
              onMade={made}
              onLateRefusal={say}
              onCancel={cancel}
            />
          }
        />
        <Route
          path=":id"
          element={
            <LibraryAt libraries={libraries} notices={notices}>
              {(library) => (
                <LibraryPage
                  // By id, so another library's page is a page of its own.
                  key={library.id}
                  library={library}
                  notices={notices}
                  arrived={arrival === pathname}
                  onAdded={remember}
                  onChanged={reload}
                  onLateRefusal={say}
                  onRemoved={(name, answer) =>
                    libraryRemoved(pathname, library.id, name, answer)
                  }
                />
              )}
            </LibraryAt>
          }
        />
        <Route
          path=":id/folders/:fid"
          element={
            <LibraryAt libraries={libraries} notices={notices}>
              {(library) => (
                <FolderAt library={library} notices={notices}>
                  {(root) => (
                    <FolderPage
                      key={root.path}
                      library={library}
                      root={root}
                      probe={probes[root.path]}
                      others={(libraries ?? [])
                        .filter((other) => other.id !== library.id)
                        .map(({ id, name }) => ({ id, name }))}
                      notices={notices}
                      onChanged={folderChanged}
                      onLateRefusal={say}
                      onRemoved={(answer) =>
                        folderRemoved(pathname, library.id, root.path, answer)
                      }
                      onMoved={(answer) =>
                        folderMoved(pathname, library.id, root.path, answer)
                      }
                    />
                  )}
                </FolderAt>
              )}
            </LibraryAt>
          }
        />
        <Route path="*" element={<Navigate to={TABLE} replace />} />
      </Routes>
    </section>
  )
}

/**
 * The library the path names, or a line saying there is none. Nothing is
 * drawn while the list has not been read: the failure, if it failed, is.
 */
function LibraryAt({
  libraries,
  notices,
  children,
}: {
  libraries: Library[] | undefined
  notices: ReactNode
  children: (library: Library) => ReactNode
}) {
  const { id } = useParams()
  if (libraries === undefined) return notices
  const library = libraries.find((one) => String(one.id) === id)
  if (library === undefined) {
    return (
      <div className="library-page">
        <p className="back">
          <Link to={TABLE}>‹ Libraries</Link>
        </p>
        {notices}
        <Missing key={id}>No such library.</Missing>
      </div>
    )
  }
  return children(library)
}

/** The folder of `library` the path names, or a line saying there is none. */
function FolderAt({
  library,
  notices,
  children,
}: {
  library: Library
  notices: ReactNode
  children: (root: Library['folders'][number]) => ReactNode
}) {
  const { fid } = useParams()
  const root = library.folders.find((one) => String(one.id) === fid)
  if (root === undefined) {
    return (
      <div className="folder-page">
        <p className="back">
          <Link to={libraryPath(library.id)}>‹ {library.name}</Link>
        </p>
        {notices}
        <Missing key={fid}>No such folder in this library.</Missing>
      </div>
    )
  }
  return children(root)
}

/**
 * The line that says the page asked for is not there. It takes focus when it
 * is drawn: whatever had focus went with the page it was on, or the person
 * came by an address. Its caller keys it by the id asked for, so a second
 * unknown id is a line drawn anew and takes focus again.
 */
function Missing({ children }: { children: string }) {
  const line = useRef<HTMLParagraphElement>(null)
  useEffect(() => {
    line.current?.focus()
  }, [])
  return (
    <p className="missing" ref={line} tabIndex={-1}>
      {children}
    </p>
  )
}

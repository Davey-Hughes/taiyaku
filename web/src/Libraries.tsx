/**
 * The libraries screen: Configuration → Libraries, and the pages beneath it.
 *
 * **Four pages over one list.** The table of libraries, the form that makes
 * one, a library's page and a directory's page each read the list this screen
 * holds and pick what they draw from it: a library by the name its address
 * spells, as `configAddress.ts` finds it, and a directory by its number. There
 * is no route that answers one library. The list is read when the screen
 * mounts, again on each move between its pages, and after every write.
 *
 * **Two failure slots.** The list's failure is the screen's and is drawn on
 * whichever page is showing; a refusal is drawn in the form that was sent,
 * while that form is drawn.
 *
 * **The probe from an add is kept, by the directory's path.** The watch probe
 * runs when a directory is added and at no other time, so a directory added in
 * this session has a watch state and one read from the list does not.
 *
 * **A removal's re-derive report, or a move's, is kept here, and drawn one
 * level up.** What was removed or moved has no page to draw it on, so the
 * report belongs to the page above it: a library's removal to the table, a
 * directory's to its library's page, or to the table where the library went
 * with its last directory. The screen goes up to that page when the answer
 * comes, if the person is still where they asked, and the report takes focus
 * there: the button that had it went with the page. The report is the one place
 * a lost position or an unwritable document is said.
 *
 * **An answer that comes once the person has gone to another page of the
 * screen is said on the page they are on**, as `configPages.tsx` says for
 * both screens: a removal's or a move's report, or the refusal of any write
 * whose page or form went before its answer came.
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
 * asked is never drawn over a library or directory that is gone.
 */
import {
  type ReactNode,
  type RefObject,
  startTransition,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useMatch,
  useNavigate,
  useParams,
} from 'react-router'
import type { Added, Derived, Probe, RemovalAnswer } from './config'
import {
  byName,
  LIBRARIES,
  libraryPath,
  NEW_LIBRARY,
  nameAt,
} from './configAddress'
import { useLateAnswers } from './configPages'
import { DirPage } from './DirPage'
import {
  type Created,
  type Library,
  listLibraries,
  type MoveAnswer,
  type Moved,
} from './libraries'
import { LibraryPage } from './LibraryPage'
import { LibraryTable } from './LibraryTable'
import { Missing } from './Missing'
import { MoveCounts } from './MoveCounts'
import { sentenceOf } from './message'
import { NewLibrary } from './NewLibrary'
import { reveal } from './reveal'
import { DeriveSection } from './ScanReport'
import { PageHeader } from './settingsLayout'
import { useStatusRefresh } from './shell'
import { SideBySide } from './SideBySide'

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

/**
 * Marks the current task as taking a write's answer (`answering`), until its
 * microtasks have run.
 */
function takeAnswer(answering: RefObject<boolean>) {
  answering.current = true
  queueMicrotask(() => {
    answering.current = false
  })
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

  const reportEl = useRef<HTMLDivElement>(null)
  const {
    place,
    shown,
    setLine: setReport,
    say,
    follow,
    here,
    arrived,
    setArrival,
    cancel,
    drawNotices,
  } = useLateAnswers<Report>(LIBRARIES, reportEl)

  /**
   * True while a removal's, a move's or a rename's answer is being taken.
   * `RootRow` calls `onChanged` in the same call as `onRemoved` or `onMoved`,
   * and `LibraryPage` in the same call as `onRenamed`, and the answer has
   * already arranged the one read that follows the write, so that
   * `onChanged` reads nothing. It is cleared when the current task's
   * microtasks run, before any later `onChanged` can come (`takeAnswer`).
   */
  const answering = useRef(false)

  const changed = useCallback(
    (quiet?: boolean) => {
      if (!answering.current) reload(quiet)
    },
    [reload],
  )

  /**
   * Takes the answer to a removal or a move started on the page `from`: its
   * report, and what the write took out of the list, at once. The person is
   * sent to the report's page, `next.at`, only while on `from`, by path: the
   * directory is gone, so a directory page left and opened again goes up too.
   * An answer that comes after they went elsewhere leaves them where they are,
   * with the report drawn on that page. The list is read once either way: by
   * the change of page, or here.
   */
  const answered = useCallback(
    (from: string, next: Report, drop: (held: Library[]) => Library[]) => {
      takeAnswer(answering)
      const { sent, line } = place(from, next)
      startTransition(() => {
        setReport(line)
        setLibraries((held) => held && drop(held))
        if (sent) navigate(line.at)
      })
      if (!sent) reload(true)
    },
    [place, setReport, navigate, reload],
  )

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
          setArrival(libraryPath(created.library.name))
          navigate(libraryPath(created.library.name))
        }
      })
      if (!sent) reload(true)
    },
    [remember, setReport, setArrival, navigate, reload],
  )

  const libraryRemoved = useCallback(
    (from: string, id: number, name: string, answer: RemovalAnswer) => {
      answered(from, { at: LIBRARIES, name, derive: answer.derive }, (held) =>
        held.filter((one) => one.id !== id),
      )
    },
    [answered],
  )

  /**
   * A directory was removed from a library. A directory that was the last of
   * its library's took the library with it: the report is then named for the
   * library, which is what went, and drawn on the table. So is the report of
   * one whose library the list no longer holds, which has no page.
   */
  const dirRemoved = useCallback(
    (from: string, id: number, path: string, answer: RemovalAnswer) => {
      const library = drawn.current?.find((one) => one.id === id)
      const left = library?.dirs.filter((one) => one.path !== path)
      if (library !== undefined && left?.length === 0) {
        libraryRemoved(from, id, library.name, answer)
        return
      }
      answered(
        from,
        {
          at: library === undefined ? LIBRARIES : libraryPath(library.name),
          name: path,
          derive: answer.derive,
        },
        (held) =>
          held.map((one) =>
            one.id === id
              ? { ...one, dirs: one.dirs.filter((f) => f.path !== path) }
              : one,
          ),
      )
    },
    [libraryRemoved, answered],
  )

  /**
   * A directory was moved to another library. The report goes to the library it
   * left, or to the table where the move emptied and removed that library. It
   * is named for the directory and the library it went to.
   */
  const dirMoved = useCallback(
    (from: string, id: number, path: string, answer: MoveAnswer) => {
      const gone = answer.moved.library_removed !== null
      const left = drawn.current?.find((one) => one.id === id)
      answered(
        from,
        {
          at: gone || left === undefined ? LIBRARIES : libraryPath(left.name),
          name: path,
          derive: answer.derive,
          move: {
            moved: answer.moved,
            from: left?.name,
            // A target the screen does not draw is one the move made.
            newLibrary: !drawn.current?.some(
              (one) => one.id === answer.moved.library_id,
            ),
          },
        },
        // The directory is not drawn in its new library: the reload does that.
        (held) =>
          gone
            ? held.filter((one) => one.id !== id)
            : held.map((one) =>
                one.id === id
                  ? {
                      ...one,
                      dirs: one.dirs.filter((f) => f.path !== path),
                    }
                  : one,
              ),
      )
    },
    [answered],
  )

  /**
   * A library was renamed. While the address is still this library's, its
   * page follows the name: the address is replaced, not pushed, with an
   * open directory's part kept, so Back does not return to an address that
   * now names nothing. What the page says goes with it (`follow`), and so
   * does an answer still out from it. The list takes the new name in the
   * same transition, so the page is never drawn at an address the list
   * cannot find, and a read sent before the rename is dropped, since it
   * would say the old name. The list is read once: by the change of page, or
   * here where there is none.
   */
  const libraryRenamed = useCallback(
    (id: number, name: string) => {
      takeAnswer(answering)
      latest.current++
      const page = here.current
      const at = nameAt(page, LIBRARIES)
      const held = drawn.current
      const named =
        at !== undefined &&
        held !== undefined &&
        byName(held, at.name)?.id === id
      const to = libraryPath(name)
      const moves = named && `${to}${at.rest}` !== page
      startTransition(() => {
        setLibraries((list) =>
          list?.map((one) => (one.id === id ? { ...one, name } : one)),
        )
        if (moves) {
          // From the library's own page as the address spells it.
          follow(page.slice(0, page.length - at.rest.length), to)
          navigate(`${to}${at.rest}`, { replace: true })
        }
      })
      if (!moves) reload()
    },
    [here, follow, navigate, reload],
  )

  const reportLabel = useId()
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

  const notices = drawNotices(
    failure !== undefined && (
      <p role="alert" ref={failureEl}>
        {libraries === undefined
          ? failure.sentence
          : `The libraries list could not be read again (${failure.sentence}), so what is drawn may be out of date.`}
      </p>
    ),
    reportNode,
  )

  /**
   * Whether the address goes beyond a library's own: a directory's page,
   * the end of this screen's tree, which `SideBySide` draws beside its
   * library's page where the width allows and alone where it does not. A
   * library is not the end of its tree, so its page opens on its own.
   */
  const dirOpen = (useMatch(`${LIBRARIES}/:name/*`)?.params['*'] ?? '') !== ''
  /**
   * The make page, whose address `Config` routes to this screen outside the
   * table's, so that the libraries' names are the only segments beneath it.
   */
  const making = useMatch(NEW_LIBRARY) !== null

  if (making) {
    return (
      <section className="libraries">
        <NewLibrary
          libraries={libraries}
          notices={notices}
          onMade={made}
          onLateRefusal={say}
          onCancel={cancel}
        />
      </section>
    )
  }

  return (
    <section className="libraries">
      <Routes>
        <Route
          index
          element={
            <LibraryTable
              libraries={libraries}
              notices={notices}
              arrived={arrived}
            />
          }
        />
        {/* One route for a library and its directories, so the library's page
            stays mounted while a directory opens beside it or another directory
            replaces that one. */}
        <Route
          path=":name/*"
          element={
            <LibraryAt libraries={libraries} notices={notices}>
              {(library) => (
                <SideBySide
                  list={
                    <LibraryPage
                      // By id, so another library's page is a page of its own.
                      key={library.id}
                      library={library}
                      libraries={libraries}
                      // The screen says each thing once: on the directory's
                      // page where one is open.
                      notices={dirOpen ? undefined : notices}
                      arrived={arrived && !dirOpen}
                      onOpen={setArrival}
                      onAdded={remember}
                      onChanged={changed}
                      onRenamed={(name) => libraryRenamed(library.id, name)}
                      onLateRefusal={say}
                      onRemoved={(name, answer) =>
                        libraryRemoved(pathname, library.id, name, answer)
                      }
                    />
                  }
                  page={
                    dirOpen ? (
                      <Routes>
                        <Route
                          path="dirs/:did"
                          element={
                            <DirAt library={library} notices={notices}>
                              {(root) => (
                                <DirPage
                                  key={root.path}
                                  library={library}
                                  root={root}
                                  probe={probes[root.path]}
                                  others={(libraries ?? [])
                                    .filter((other) => other.id !== library.id)
                                    .map(({ id, name }) => ({ id, name }))}
                                  notices={notices}
                                  arrived={arrived}
                                  onChanged={changed}
                                  onLateRefusal={say}
                                  onRemoved={(answer) =>
                                    dirRemoved(
                                      pathname,
                                      library.id,
                                      root.path,
                                      answer,
                                    )
                                  }
                                  onMoved={(answer) =>
                                    dirMoved(
                                      pathname,
                                      library.id,
                                      root.path,
                                      answer,
                                    )
                                  }
                                />
                              )}
                            </DirAt>
                          }
                        />
                        <Route
                          path="*"
                          element={<Navigate to={LIBRARIES} replace />}
                        />
                      </Routes>
                    ) : null
                  }
                />
              )}
            </LibraryAt>
          }
        />
        <Route path="*" element={<Navigate to={LIBRARIES} replace />} />
      </Routes>
    </section>
  )
}

/**
 * The library whose name the path spells, or a page saying there is none,
 * whose line takes focus. Nothing is drawn while the list has not been read:
 * the failure, if it failed, is.
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
  const at = nameAt(useLocation().pathname, LIBRARIES)
  if (libraries === undefined) return notices
  const library = at === undefined ? undefined : byName(libraries, at.name)
  if (library === undefined) {
    return (
      <div className="settings-page">
        <PageHeader
          back={{ to: LIBRARIES, label: 'Libraries' }}
          title="No such library"
        />
        {notices}
        <Missing key={at?.name}>
          There is no library at this address: it may have been removed.
        </Missing>
      </div>
    )
  }
  return children(library)
}

/**
 * The directory of `library` the path names, or a page saying there is none,
 * whose line takes focus.
 */
function DirAt({
  library,
  notices,
  children,
}: {
  library: Library
  notices: ReactNode
  children: (root: Library['dirs'][number]) => ReactNode
}) {
  const { did } = useParams()
  const root = library.dirs.find((one) => String(one.id) === did)
  if (root === undefined) {
    return (
      <div className="settings-page">
        <PageHeader
          back={{ to: libraryPath(library.name), label: library.name }}
          title="No such directory"
        />
        {notices}
        <Missing key={did}>
          This library has no directory at this address: it may have been
          removed, or moved to another library.
        </Missing>
      </div>
    )
  }
  return children(root)
}

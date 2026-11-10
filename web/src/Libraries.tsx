/**
 * The libraries screen: Configuration → Libraries, and the pages beneath it.
 *
 * **Four pages over one list.** The table of libraries, the form that makes
 * one, a library's page and a directory's page each read the list this screen
 * holds and pick what they draw from it: a library by the name its address
 * spells, as `configAddress.ts` finds it, and a directory by its number. There
 * is no route that answers one library. The list is the read store's
 * (`pageReads.ts`'s `LIBRARIES`): read when the screen mounts, again on each
 * move between its pages but the one a rename makes, and after every write.
 * Until it has been read, the table and a library's page draw their header
 * and, after a while, "Loading…" in their body's place, a directory's
 * address draws its library's page, and the make page draws its form at
 * once. A library's page names itself from its address, and waits for its
 * scan status too once the list has named the library (`LibraryPage`).
 *
 * **Two failure slots.** The list's failure is the screen's and is drawn on
 * whichever page is showing; a refusal is drawn in the form that was sent,
 * while that form is drawn.
 *
 * **The probe from an add is kept, by the directory's path.** The watch probe
 * runs when a directory is added and at no other time, so a directory added in
 * this session has a watch state and one read from the list does not.
 *
 * **A removal's re-derive report, or what a move did, is kept here, and drawn
 * one level up.** What was removed or moved has no page to draw it on, so the
 * report belongs to the page above it: a library's removal to the table, a
 * directory's to its library's page, or to the table where the library went
 * with its last directory. The screen goes up to that page when the answer
 * comes, if the person is still where they asked, and the report takes focus
 * there: the button that had it went with the page. A removal's report is the
 * one place a lost position or an unwritable document is said; a move derives
 * nothing, so its report says who gained and lost sight of the directory.
 *
 * **An answer that comes once the person has gone to another page of the
 * screen is said on the page they are on**, as `configPages.tsx` says for
 * both screens: a removal's or a move's report, or the refusal of any write
 * whose page or form went before its answer came.
 *
 * **The sidebar's status is read with the list** (`LIBRARY_STATUS`), so the
 * sidebar's libraries follow the screen's (O200). The screen reads it as it
 * arrives, from any tab, beside `usePage`'s read of the list; each read after
 * that invalidates the two together, so they land in one step and follow
 * every write: a removal whose re-read of the list fails has still removed
 * the library. The invalidation is the store's, not the screen's, so an
 * answer that comes once another tab has replaced the screen still moves the
 * list a return to it draws (O222).
 *
 * **Every directory write marks the roots stale** (`ROOTS`), which Rules
 * draws, so Rules arriving afterwards waits for them rather than drawing
 * what the app knows is out of date. A make or a removal or move does it
 * here; an add does it in `LibraryPage`, and a directory's settings in
 * `RootRow`. A removal marks what the schema's cascades change with it as
 * well: the rules scoped to its directories, which they delete, the
 * accepted proposals that wrote those rules, and, for a library, the
 * accounts' grants. A change of page marks none of these, so a visit that wrote
 * nothing leaves Rules to draw at once.
 *
 * **A removal or a move marks the Library views stale**
 * (`invalidateLibraryViews`), so a Library view arriving afterwards waits
 * for its own read: a removal takes works out of them, and a move takes
 * works from one library's views to another's. A make adds directories
 * that hold no works until they are scanned, and a rename changes a name no
 * view's answer carries, so neither marks them. A change
 * of page marks none of them, which is why each write marks them and
 * `reread` does not.
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
import { useLateAnswers, usePageKey } from './configPages'
import { DirPage } from './DirPage'
import {
  type Created,
  type Library,
  libraryRemovedSentence,
  type MoveAnswer,
  type Moved,
  type UnlinkAnswer,
  type Unlinked,
  withLibraryMade,
  withLibraryRenamed,
  withMovedDir,
  withoutDir,
  withoutLibrary,
} from './libraries'
import { LibraryPage } from './LibraryPage'
import { LibraryTable } from './LibraryTable'
import { Missing } from './Missing'
import { MoveCounts } from './MoveCounts'
import { sentenceOf } from './message'
import { NewLibrary } from './NewLibrary'
import {
  ACCEPTED_PROPOSALS,
  invalidateLibraryViews,
  LIBRARIES as LIST,
  LIBRARY_STATUS,
  ROOTS,
  RULES,
  SPLIT_EDITIONS,
  USERS as USER_LIST,
} from './pageReads'
import { entryOf, invalidate, readSet } from './reads'
import { reveal } from './reveal'
import { DeriveSection } from './ScanReport'
import { PageHeader } from './settingsLayout'
import { SideBySide } from './SideBySide'
import { usePage } from './usePage'

/**
 * What a removal found, or what a move or an unlink did, whose it was, and
 * the page it is drawn on.
 */
interface Report {
  at: string
  /** Taken when the answer itself moved the person to the report's page. */
  focus?: boolean
  name: string
  /** A removal's: what re-deriving after it found. A move derives nothing. */
  derive?: Derived
  /** A move's: what it did, and where from. Absent for a removal's. */
  move?: { moved: Moved; from?: string }
  /**
   * An unlink's: what it did, and the library the directory left, where the
   * list still held it. An unlink derives nothing.
   */
  unlinked?: { unlinked: Unlinked; from?: string }
}

/**
 * What an unlink's report says: the directory, the library it left, and that
 * library, where it went with it, as the rehearsal said it would.
 */
function unlinkedLine(
  path: string,
  { unlinked, from }: NonNullable<Report['unlinked']>,
): string {
  const removed = libraryRemovedSentence(unlinked, true)
  const line = `Removed ${path} from ${from ?? 'its library'}.`
  return removed === undefined ? line : `${line} ${removed}`
}

/** How a directory left a library, with the answer that says what that did. */
type DirLeft =
  | { how: 'removed'; answer: RemovalAnswer }
  | { how: 'unlinked'; answer: UnlinkAnswer }

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
  const {
    ready,
    answers: [list],
    // Why the list could not be read, once this visit has read it: no
    // failure kept from an earlier visit is handed over.
    failures: [failure],
    loadingDrawn,
  } = usePage<[{ libraries: Library[] }]>([LIST])
  /** The store's list, once the page may draw it. */
  const listed = ready ? list?.libraries : undefined
  const { pathname } = useLocation()
  const navigate = useNavigate()
  /**
   * The address a rename's answer is moving the person to, from the answer
   * until the effect below sees the move drawn. The rename reads the list at
   * once, and its answer may land before the move is drawn: drawn at the
   * address that still spells the old name, it would name no library there.
   */
  const renamedTo = useRef<string | undefined>(undefined)
  /**
   * What is drawn: the store's list, with what a write's answer took away,
   * renamed or made laid over it until the store holds another list. A read
   * that fails leaves the store's list as it was, so what the write changed
   * stays drawn.
   */
  const [libraries, setLibraries] = useState(listed)
  const [drawnFrom, setDrawnFrom] = useState(listed)
  if (
    listed !== drawnFrom &&
    (renamedTo.current === undefined || renamedTo.current === pathname)
  ) {
    setDrawnFrom(listed)
    setLibraries(listed)
  }
  const failureEl = useRef<HTMLParagraphElement>(null)
  const [probes, setProbes] = useState<Record<string, Probe>>({})

  /**
   * Whether the failure of the newest read is brought into view: not for a
   * re-read `reread(true)` sent, which no press started (`quiet`).
   */
  const revealFailure = useRef(true)
  /** Reads the list and the sidebar's status again, as one set. */
  const reread = useCallback((quiet = false) => {
    revealFailure.current = !quiet
    void invalidate(LIST, LIBRARY_STATUS)
  }, [])

  // The status beside the arrival's read of the list, which is `usePage`'s.
  // Not in that page's set, which would hold the screen's body until the
  // sidebar's answer came. A read out for it already is joined; when both
  // are drawn at once the shell's own joins this one, since a component's
  // effects run before its parent's.
  useEffect(() => {
    void readSet([LIBRARY_STATUS])
  }, [])

  /** The page the effect below last saw. */
  const pageSeen = useRef(pathname)
  // Each page reads the list afresh, but for the one a rename moved the
  // person to, which the rename read it for. The first page's read is
  // `usePage`'s.
  useEffect(() => {
    if (pageSeen.current === pathname) return
    pageSeen.current = pathname
    const read = renamedTo.current === pathname
    renamedTo.current = undefined
    if (!read) reread()
  }, [pathname, reread])

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
      if (!answering.current) reread(quiet)
    },
    [reread],
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
      if (!sent) reread(true)
    },
    [place, setReport, navigate, reread],
  )

  const remember = useCallback((added: Added[]) => {
    setProbes((held) => {
      const next = { ...held }
      for (const one of added) {
        // A link ran no probe: what one found as another library added the
        // directory is still what it found of the same directory.
        if (one.watching === null) continue
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
   * A write answered on library `id`'s page, or a press of its Scan library,
   * takes away the report drawn there, or on the page of a directory beside
   * it: the report says what was held before that write, which an add of the
   * same directory contradicts (O255). A report on another library's pages
   * is theirs. Says whether the report held focus.
   */
  const wroteOn = useCallback(
    (id: number): boolean => {
      const at = nameAt(here.current, LIBRARIES)
      const held = drawn.current
      const own =
        at !== undefined &&
        held !== undefined &&
        byName(held, at.name)?.id === id
      if (!own) return false
      const focused =
        reportEl.current?.contains(document.activeElement) === true
      setReport(undefined)
      return focused
    },
    [here, setReport],
  )

  /**
   * Counts the adds whose answer took away a report that held focus, which
   * the library's page then puts on Add directory… (O255).
   */
  const [addFocus, setAddFocus] = useState(0)

  /**
   * A directory New library linked as it made the library it goes to, for
   * that library's page to say, once.
   */
  const [madeLink, setMadeLink] = useState<
    { libraryId: number; path: string; also_in: string[] } | undefined
  >()
  const madeLinkTaken = useCallback(() => setMadeLink(undefined), [])

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
      const link = created.added.find((one) => one.also_in.length > 0)
      // Its directories are new roots, which Rules draws.
      void invalidate(ROOTS)
      startTransition(() => {
        // Drawn from the create's own answer: if the read that follows
        // fails, the list still holds what was just made. It goes last; the
        // read puts it where the server's own collation does.
        setLibraries((held) =>
          held === undefined
            ? [created.library]
            : withLibraryMade(held, created.library),
        )
        if (sent) {
          setReport(undefined)
          setMadeLink(
            link === undefined
              ? undefined
              : {
                  libraryId: created.library.id,
                  path: link.root.path,
                  also_in: link.also_in,
                },
          )
          setArrival(libraryPath(created.library.name))
          navigate(libraryPath(created.library.name))
        }
      })
      if (!sent) reread(true)
    },
    [remember, setReport, setArrival, navigate, reread],
  )

  const libraryRemoved = useCallback(
    (from: string, id: number, name: string, answer: RemovalAnswer) => {
      // The removal deleted the library's directories and, by the schema's
      // cascades, the rules scoped to them, the rule each accepted proposal
      // that wrote one names, and every account's grant of the library.
      void invalidate(ROOTS, RULES, ACCEPTED_PROPOSALS, USER_LIST)
      invalidateLibraryViews()
      answered(from, { at: LIBRARIES, name, derive: answer.derive }, (held) =>
        withoutLibrary(held, id),
      )
    },
    [answered],
  )

  /**
   * A directory left library `id`: unlinked from it alone, or removed from
   * every library holding it (`left.how`). Each library the answer names as
   * emptied went with its last directory. Where `id` is one of them, the
   * report is drawn on the table, and a removal's is named for the library,
   * which is what went. So is the report of one whose library the list no
   * longer holds, which has no page. An unlink's report says which library
   * the directory left (`unlinkedLine`).
   */
  const dirRemoved = useCallback(
    (from: string, id: number, path: string, left: DirLeft) => {
      const { how } = left
      const emptied =
        left.how === 'removed'
          ? left.answer.removed.libraries_removed
          : left.answer.unlinked.library_removed === null
            ? []
            : [left.answer.unlinked.library_removed]
      const library = drawn.current?.find((one) => one.id === id)
      const gone = library !== undefined && emptied.includes(library.name)
      const at =
        gone || library === undefined ? LIBRARIES : libraryPath(library.name)
      // A removal deleted the directory and, by the schema's cascades, the
      // rules scoped to it and the rule each accepted proposal that wrote
      // one names; an unlink deletes neither. An emptied library went with
      // every account's grant of it.
      void invalidate(
        ROOTS,
        ...(how === 'removed' ? [RULES, ACCEPTED_PROPOSALS] : []),
        ...(emptied.length > 0 ? [USER_LIST] : []),
      )
      invalidateLibraryViews()
      answered(
        from,
        left.how === 'unlinked'
          ? {
              at,
              name: path,
              unlinked: { unlinked: left.answer.unlinked, from: library?.name },
            }
          : {
              at,
              name: gone ? library.name : path,
              derive: left.answer.derive,
            },
        (held) => withoutDir(held, id, path, how, emptied),
      )
    },
    [answered],
  )

  /**
   * A directory was moved to another library. The report goes to the library it
   * left, or to the table where the move emptied and removed that library. It
   * is named for the directory and the library it went to. Every other
   * library holding it names that library in place of the one it left at
   * once (`withMovedDir`); the read that follows draws it in that library.
   */
  const dirMoved = useCallback(
    (from: string, id: number, path: string, answer: MoveAnswer) => {
      const gone = answer.moved.library_removed !== null
      // Every directory write marks the roots stale, whatever fields their
      // list carries: which it changed is the server's to know. A move that
      // emptied its library removed it, and every account's grant of it.
      void invalidate(ROOTS, ...(gone ? [USER_LIST] : []))
      invalidateLibraryViews()
      const left = drawn.current?.find((one) => one.id === id)
      answered(
        from,
        {
          at: gone || left === undefined ? LIBRARIES : libraryPath(left.name),
          name: path,
          move: { moved: answer.moved, from: left?.name },
        },
        (held) => withMovedDir(held, id, path, answer.moved),
      )
    },
    [answered],
  )

  /**
   * A library was renamed. While the address is still this library's, its
   * page follows the name: the address is replaced, not pushed, with an
   * open directory's part kept, so Back does not return to an address that
   * names nothing. What the page says and an answer still out from it go
   * with it (`follow`), but for its report, taken away (`wroteOn`). The list
   * takes the new name in the same transition, so the page is never drawn
   * at an address the list cannot find, and is read here, at once, to
   * overtake a read sent before the rename, which would say the old name;
   * the change of page this makes reads nothing more.
   */
  const libraryRenamed = useCallback(
    (id: number, name: string) => {
      takeAnswer(answering)
      const page = here.current
      const at = nameAt(page, LIBRARIES)
      const held = drawn.current
      const named =
        at !== undefined &&
        held !== undefined &&
        byName(held, at.name)?.id === id
      const to = libraryPath(name)
      const moves = named && `${to}${at.rest}` !== page
      if (moves) renamedTo.current = `${to}${at.rest}`
      reread()
      // Each account's grants name the libraries it is granted, and each
      // split edition the libraries holding it, so Users and Split editions
      // keep the old name until they read them again.
      void invalidate(USER_LIST, SPLIT_EDITIONS)
      startTransition(() => {
        wroteOn(id)
        setLibraries((list) => list && withLibraryRenamed(list, id, name))
        if (moves) {
          // From the library's own page as the address spells it.
          follow(page.slice(0, page.length - at.rest.length), to)
          navigate(`${to}${at.rest}`, { replace: true })
        }
      })
    },
    [here, follow, navigate, reread, wroteOn],
  )

  const reportLabel = useId()
  // The list's failure is drawn at the head of the page, and a person acting
  // further down it is not looking there. A failed read is brought into view,
  // which does nothing where it already is, except the read at the end of any
  // scan: the page would move from under whoever is typing by then. Focus
  // stays where it is. Each failed read lands as a new entry in the store,
  // so one repeating a standing sentence is brought into view again; a mark
  // of stale is a new entry too, and is not a read's answer. Run again when
  // the failure is handed over, which may be a render after its entry came.
  const entry = entryOf(LIST)
  useEffect(() => {
    const el = failureEl.current
    if (failure === undefined || entry?.stale !== false || el === null) return
    if (revealFailure.current) reveal(el)
  }, [entry, failure])

  const reportNode = shown !== undefined && (
    <div
      ref={reportEl}
      className="removal-report scan-report"
      tabIndex={-1}
      role="group"
      aria-labelledby={reportLabel}
    >
      <p id={reportLabel}>
        {shown.move !== undefined
          ? `Moved ${shown.name}${
              shown.move.from === undefined ? '' : ` from ${shown.move.from}`
            } to ${shown.move.moved.library}.`
          : shown.unlinked !== undefined
            ? unlinkedLine(shown.name, shown.unlinked)
            : `What re-deriving after removing ${shown.name} found:`}
      </p>
      {shown.move !== undefined && <MoveCounts moved={shown.move.moved} past />}
      {shown.derive !== undefined && <DeriveSection derive={shown.derive} />}
    </div>
  )

  const notices = drawNotices(
    failure !== undefined && (
      <p role="alert" ref={failureEl}>
        {libraries === undefined
          ? sentenceOf(failure)
          : `The libraries list could not be read again (${sentenceOf(failure)}), so what is drawn may be out of date.`}
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
              loadingDrawn={loadingDrawn}
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
              {(library, key) => (
                <SideBySide
                  list={
                    <LibraryPage
                      // A page of its own for each library (`usePageKey`).
                      key={key}
                      library={library}
                      loadingDrawn={loadingDrawn}
                      libraries={libraries}
                      // The screen says each thing once: on the directory's
                      // page where one is drawn, which needs its library.
                      notices={
                        dirOpen && library !== undefined ? undefined : notices
                      }
                      arrived={arrived && !dirOpen}
                      linkedAtMaking={
                        madeLink !== undefined &&
                        madeLink.libraryId === library?.id
                          ? madeLink
                          : undefined
                      }
                      onLinkedAtMakingTaken={madeLinkTaken}
                      onOpen={setArrival}
                      onAdded={(added) => {
                        remember(added)
                        if (library !== undefined && wroteOn(library.id)) {
                          setAddFocus((n) => n + 1)
                        }
                      }}
                      addFocus={addFocus}
                      onScan={() => {
                        if (library !== undefined) wroteOn(library.id)
                      }}
                      onChanged={changed}
                      onRenamed={libraryRenamed}
                      onLateRefusal={say}
                      onRemoved={(id, name, answer) =>
                        libraryRemoved(pathname, id, name, answer)
                      }
                    />
                  }
                  page={
                    dirOpen && library !== undefined ? (
                      <Routes>
                        <Route
                          path="dirs/:did"
                          element={
                            <DirAt library={library} notices={notices}>
                              {(root) => (
                                <DirPage
                                  // One directory on two libraries' pages is
                                  // two pages, so the key holds the library;
                                  // it holds the path for the reason the
                                  // library's table keys its rows by it.
                                  key={`${library.id}:${root.path}`}
                                  library={library}
                                  root={root}
                                  probe={probes[root.path]}
                                  // A library already holding the directory
                                  // is no place to move it (decision 25).
                                  others={(libraries ?? [])
                                    .filter(
                                      (other) =>
                                        other.id !== library.id &&
                                        !root.libraries.some(
                                          (one) => one.id === other.id,
                                        ),
                                    )
                                    .map(({ id, name }) => ({ id, name }))}
                                  notices={notices}
                                  arrived={arrived}
                                  onOpen={setArrival}
                                  onChanged={changed}
                                  onLateRefusal={say}
                                  onRemoved={(answer) =>
                                    dirRemoved(
                                      pathname,
                                      library.id,
                                      root.path,
                                      { how: 'removed', answer },
                                    )
                                  }
                                  onUnlinked={(answer) =>
                                    dirRemoved(
                                      pathname,
                                      library.id,
                                      root.path,
                                      { how: 'unlinked', answer },
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
 * whose line takes focus. `children` draws the library's page from the
 * moment the screen is drawn, with the library `undefined` while the list is
 * read or could not be, so that the header drawn then is the one kept
 * (`usePageKey`).
 */
function LibraryAt({
  libraries,
  notices,
  children,
}: {
  libraries: Library[] | undefined
  notices: ReactNode
  children: (library: Library | undefined, key: number) => ReactNode
}) {
  const at = nameAt(useLocation().pathname, LIBRARIES)
  const library =
    libraries === undefined || at === undefined
      ? undefined
      : byName(libraries, at.name)
  const key = usePageKey(library?.id)
  if (libraries === undefined) return children(undefined, key)
  if (library === undefined) {
    return (
      <div className="settings-page">
        <PageHeader
          back={{ to: LIBRARIES, label: 'Libraries' }}
          title="No such library"
        />
        {notices}
        <Missing key={at?.name}>
          There is no library at this address: it may have been removed, or the
          libraries list on this page is out of date; reload the page to read it
          again.
        </Missing>
      </div>
    )
  }
  return children(library, key)
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
          removed, moved to another library, or removed from this library, or
          the libraries list on this page is out of date; reload the page to
          read it again.
        </Missing>
      </div>
    )
  }
  return children(root)
}

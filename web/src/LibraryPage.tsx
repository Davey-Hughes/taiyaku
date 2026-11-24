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
 * `libraryScan`), which says what holds its removal. A return draws its body
 * at once from the status kept, and reads it again, where that status is not
 * stale; where it is, the return waits for its read as a first visit does.
 * While the body waits, and while the screen reads the list that names the
 * library, the page draws its header, named as its address spells it until
 * the list names the library and as the list does then, and "Loading…" in its
 * body's place once the wait has held. That header is the one the drawn page
 * keeps, so whatever has focus in it keeps it as the body comes. A status
 * that cannot be read, with none kept, is drawn as nothing holding it.
 *
 * **A scan is drawn from the scan feed** (`scanFeed.ts`), whoever started
 * it: each job the feed says holds one of the library's directories, or one
 * it shows inside them. Scan library reads Cancel while one does, and the
 * directories each job walks say "Scanning…" in the table. Each job's line
 * is its step and count, and names the other libraries it holds. Once none
 * holds the library, a directory a cancel paused says so, and since when.
 * Its own press, or a 409, which is proof of a scan in flight, asks the feed
 * at once, and Scan library is held until that answer lands: the button goes
 * from Scan library to Cancel with no free press between.
 *
 * **The reports of a scan are drawn here when it ends**: when a job holding
 * the library leaves the feed, or the answer its press asked for holds none,
 * since the scan ended before any poll saw it. The status of each directory
 * the job held, of those a scan of the library walks, is read, and whatever
 * report each holds is drawn under its path. Only those: any other
 * directory's status holds a report from another scan, if any. Nor, for a
 * job the feed showed, the one whose page is open beside this one: that
 * page's row saw the job too and draws its report, which side by side would
 * be said twice. Of a scan no poll saw, the row draws nothing. What a scan
 * changed in the list and the Library views is read again by the feed.
 *
 * **Scans and removals exclude each other.** Remove… is held while a scan
 * is known or starting, and Scan library while one is starting or the
 * removal dialog is open; the removal's confirm is held while a scan is
 * known. Remove… and the confirm are held too while the status names a
 * removal waiting for a scan around or inside one of the library's
 * directories (`held_removals`), or a derive holding the lease
 * (`derive_running`, O51), which the server refuses the removal for. While
 * the feed shows no scan, the status is read again each poll's interval until
 * it names none; a hold a scan explains is read again as the feed marks the
 * status stale at the scan's end. It is read once, too, at a refusal of the
 * removal for a hold, which the page may not be drawing. A scan holds none
 * of Rename…, Add directory… and Match editions by page content; the
 * removal holds all three, and the Directories table draws its paths without
 * links while it is open: a directory's page offers Scan directory, Move…
 * and Remove… on a directory the removal is taking.
 *
 * **The passes are drawn apart from the scans**, the fingerprint pass's and
 * the match pass's, each a line and a Cancel of its own after theirs while
 * it covers the library (`PassLine`): neither holds anything a scan or a
 * removal waits on. Match editions by page content says whether they cover
 * the library, and is drawn from the server's answer, never the press, so a
 * refused switch leaves it as it was.
 */
import {
  Fragment,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { NavLink, useLocation, useMatch } from 'react-router'
import {
  type Added,
  keptSchedule,
  lastScan,
  POLL_MISSES_ALLOWED,
  type RemovalAnswer,
  SCAN_POLL_MS,
  type ScanOutcome,
  scanEvery,
  scanStatus,
  statusOf,
} from './config'
import { dirPath, LIBRARIES, libraryNameClash, nameAt } from './configAddress'
import { JOB_WAITS } from './deriveHold'
import { DirPicker } from './DirPicker'
import { focusUnder } from './focusUnder'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import {
  addDir,
  type Library,
  type LibraryDir,
  type LibraryScanStatus,
  type NestedDir,
  namesOf,
  type RemovalHold,
  removalHold,
  renameLibrary,
  SCAN_OFF_WORDS,
  setMatchByContent,
  startLibraryScan,
} from './libraries'
import { linkedPlace } from './linkedPlace'
import { messageOf, sentenceOf } from './message'
import { NameClash } from './NameClash'
import { libraryScan, ROOTS } from './pageReads'
import { PassLine } from './PassLine'
import { PathWithBreaks } from './PathWithBreaks'
import { Refusal } from './Refusal'
import { entryOf, invalidate, readNow } from './reads'
import { bringIntoView } from './reveal'
import { RemoveLibrary } from './RemoveLibrary'
import {
  askScanFeed,
  cancelScan,
  coversToo,
  fingerprintJob,
  jobsHolding,
  matchJob,
  scanJobs,
  stepLine,
  useScanFeed,
} from './scanFeed'
import { ScanReport } from './ScanReport'
import {
  PageHeader,
  PageLoading,
  RemovalSection,
  Row,
  Section,
} from './settingsLayout'
import { usePage } from './usePage'
import { inUtc } from './utc'
import { WaitWords } from './WaitWords'
import { ROW_CELLS, useLineEnds } from './lineEnds'
import { count } from './count'

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
  /**
   * A directory New library linked as it made this library, which other
   * libraries held, by name: the page says so as an add's link is said.
   */
  linkedAtMaking?: { path: string; also_in: string[]; schedule?: string }
  /** The page has taken `linkedAtMaking`, which is not said a second time. */
  onLinkedAtMakingTaken?: () => void
  /** A directory was added here: its probe is the screen's to keep. */
  onAdded: (added: Added[]) => void
  /**
   * Counts the adds whose answer took the screen's report on this page, or
   * on a directory's page beside it, away while it held focus, which may be
   * an add sent from an earlier drawing of the page. Each puts focus on Add
   * directory… (O255).
   */
  addFocus?: number
  /**
   * Scan library was pressed, and a scan asked for: what the screen said on
   * this page, or on a directory's page beside it, was of the library before
   * it (O255).
   */
  onScan?: () => void
  /**
   * The list reloads on a rename or an added directory; the scan feed reads
   * it again when a scan ends.
   */
  onChanged: () => void
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
 * How `dir` overlaps another library's directory, as its row's Overlaps cell
 * says it, without naming the library (spec decision 28): Shared, where
 * another library holds it too; Inside, where it sits in another library's
 * directory; Around, where another library's directory sits in it. A
 * library's own directories are never the same or nested, so every one the
 * server names is another library's. A directory inside that this library
 * does not show counts too: it overlaps all the same.
 */
function overlapsOf(dir: LibraryDir): string[] {
  return [
    ...(dir.libraries.length > 1 ? ['Shared'] : []),
    ...(dir.inside.length > 0 ? ['Inside'] : []),
    ...(dir.nested.length > 0 ? ['Around'] : []),
  ]
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

const px = (length: string) => Number.parseFloat(length) || 0

/**
 * The room the add form takes in `root` as it is drawn, the picker without
 * its directory browser, or Add directory… alone: what a link's line is
 * drawn in, so that its arrival and its going move nothing below it. The
 * browser's room goes at the add, as after an add that links nothing
 * (O524). A content height, as `height` takes one.
 */
function roomOf(root: HTMLElement | null): number {
  if (root === null) return 0
  const style = getComputedStyle(root)
  // The browser's host is a line of the row to itself, drawn only while the
  // browser in it draws something (`admin.css`), with the row's gap above
  // it. Heights unrounded, so that a room taken with the browser open is
  // the room taken with it shut.
  const host = root.querySelector(':scope > .dir-browser-host')
  const browser =
    host === null || getComputedStyle(host).display === 'none'
      ? 0
      : host.getBoundingClientRect().height + px(style.rowGap)
  return Math.max(
    0,
    root.getBoundingClientRect().height -
      px(style.borderTopWidth) -
      px(style.borderBottomWidth) -
      px(style.paddingTop) -
      px(style.paddingBottom) -
      browser,
  )
}

/**
 * Lays a link's line where `linkedPlace` says, read from the page as it is
 * drawn: as `top`, and padding a `margin-bottom` gives back, so it keeps
 * its place in the flow. Its own place is taken off first, so that what is
 * read is where the flow lays it now.
 */
function placeLinked(line: HTMLElement | null, room: HTMLElement | null) {
  if (line === null || room === null) return
  for (const side of [
    'top',
    'padding-top',
    'padding-bottom',
    'margin-bottom',
  ]) {
    line.style.removeProperty(side)
  }
  if (!line.hasChildNodes()) return
  const roomStyle = getComputedStyle(room)
  const box = room.closest('.settings-box') ?? room
  const drawn = (parts: Iterable<Element>) =>
    Array.from(parts, (part) => part.getBoundingClientRect())
  const place = linkedPlace({
    room: room.getBoundingClientRect(),
    padTop: px(roomStyle.paddingTop),
    padBottom: px(roomStyle.paddingBottom),
    line: line.getBoundingClientRect(),
    tableRows: drawn(box.querySelectorAll('tr')),
    roomRows: drawn(
      [...room.children].filter(
        (part) =>
          part.compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING,
      ),
    ),
  })
  if (place === undefined) return
  line.style.top = `${place.top}px`
  if (place.grows === 0) return
  const style = getComputedStyle(line)
  line.style.paddingTop = `${px(style.paddingTop) + place.grows / 2}px`
  line.style.paddingBottom = `${px(style.paddingBottom) + place.grows / 2}px`
  line.style.marginBottom = `${-place.grows}px`
}

/** Whether `line` says a link over the control with focus (`focusUnder`). */
const overFocus = (line: HTMLElement | null) =>
  line?.hasChildNodes() === true && focusUnder(line)

/**
 * What the line says of a link: the directory, who else holds it, and the
 * shared schedule it kept where the add asked for another (`keptSchedule`).
 */
function LinkedWords({
  path,
  also_in,
  schedule,
}: {
  path: string
  also_in: string[]
  schedule?: string
}) {
  return (
    <>
      <code>
        <PathWithBreaks path={path} />
      </code>{' '}
      is also a directory of {namesOf(also_in)}. It keeps the settings it has
      there
      {schedule !== undefined && `, and its scan schedule stays ${schedule}`}.
    </>
  )
}

/** A link's line taken away by a press, its room kept. */
const quieted = <T extends object>(link: T | undefined) =>
  link === undefined ? undefined : { ...link, quiet: true }

/**
 * Why a library's status holds its removal. Every read the page takes a hold
 * from comes here, so that none of them forgets a reason.
 */
const removalHoldIn = (status: LibraryScanStatus | undefined): RemovalHold =>
  status === undefined
    ? undefined
    : removalHold(status.held_removals.length > 0, status.derive_running)

/**
 * A time as a Last scan cell says it after each end a scan can have but ok, in
 * figures no time the server writes draws wider: it writes
 * `YYYY-MM-DD HH:MM:SS`, and eights are the UI face's widest figures. A scan
 * fails or is refused; one the server runs reads `waiting` while it waits for
 * the job lock or the lease (O51), `deriving` until its derive ends,
 * `interrupted` where a restart found either (O186), and `cancelled` where
 * Cancel stopped it. An ok time, their opening words, is narrower than any
 * of them.
 */
const WIDEST_ENDS = [
  'failed',
  'refused',
  'deriving',
  'interrupted',
  'waiting',
  'cancelled',
].map((status) =>
  lastScan({ last_scan_at: '8888-88-88 88:88:88', last_scan_status: status }),
)

/**
 * The words a directory's Last scan cell holds unseen while it scans: what it
 * said, and the widest words any scan's end can give it. The layout takes the
 * widest in whatever face draws them, so the column widens as the page first
 * draws the row scanning, and the end of no scan it draws as scanning moves
 * it (O253).
 * A scan it never draws, one the CLI runs, or one that starts and ends
 * between two of the feed's polls, still moves the column when the list is
 * next read.
 */
const heldWhileScanning = (dir: LibraryDir): string[] => [
  lastScan(dir),
  ...WIDEST_ENDS,
]

/** Save's words while a rename is out, and the width Save keeps for them. */
const SAVING = 'Saving…'

/** The switch's label, which names it too in a refusal said once its page has gone. */
const MATCH_WORDS = 'Match editions by page content'

export function LibraryPage({
  library,
  loadingDrawn = false,
  libraries,
  notices,
  arrived = false,
  linkedAtMaking,
  onLinkedAtMakingTaken,
  onAdded,
  addFocus,
  onScan,
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
  /** The id, as the router reads it, of the directory open beside this page. */
  const beside = useMatch(`${LIBRARIES}/:name/dirs/:did`)?.params.did

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
  /**
   * The switch's send is out. A ref as well, as `writing` is: two presses
   * before a render both read false.
   */
  const [switching, setSwitching] = useState(false)
  const switchingRef = useRef(false)
  /** Why the last switch was refused. Drawn under its box. */
  const [switchFailure, setSwitchFailure] = useState<string | undefined>()
  /**
   * The switch an answer set, drawn until the list read after it says so, as
   * `renamed` is: `from` is what the list said as it was sent.
   */
  const [switched, setSwitched] = useState<{ from: boolean; to: boolean }>()
  // biome-ignore lint/correctness/useExhaustiveDependencies: `library.match_by_content` is the trigger: the list has been read again, and its switch is the one to draw.
  useEffect(() => {
    setSwitched(undefined)
  }, [library?.match_by_content])
  const [adding, setAdding] = useState(false)
  /**
   * Don’t scan yet (schedule Off), unticked as the picker opens so that no
   * directory is added Off for a tick left from the add before (O275), and
   * kept through a refusal, as the typed path is.
   */
  const [scanOff, setScanOff] = useState(false)
  /** Where the picker draws its browser: below Don’t scan yet (O316). */
  const [browserHost, setBrowserHost] = useState<HTMLDivElement | null>(null)
  /**
   * The directory the last add, or the making of the library, linked, which
   * other libraries held; the room the line saying so is drawn in, until it
   * is measured; and whether the list has held the directory since, after
   * which the list no longer holding it means the link went.
   */
  const [linked, setLinked] = useState<
    | {
        path: string
        also_in: string[]
        schedule?: string
        room?: number
        held: boolean
        /** Taken away by a press, its room still kept. */
        quiet?: boolean
      }
    | undefined
  >()
  // The line goes with the link: a removal or a move of the directory from
  // a page drawn beside this one leaves this page mounted.
  if (linked !== undefined && library !== undefined) {
    const holds = library.dirs.some((dir) => dir.path === linked.path)
    if (holds && !linked.held) setLinked({ ...linked, held: true })
    else if (!holds && linked.held) setLinked(undefined)
  }
  // Taken once it is handed over, after the page's first paint, so the
  // line fills a live region already drawn empty, which is what a screen
  // reader reads out.
  useEffect(() => {
    if (linkedAtMaking === undefined) return
    setLinked({ ...linkedAtMaking, held: false })
    onLinkedAtMakingTaken?.()
  }, [linkedAtMaking, onLinkedAtMakingTaken])
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
  /**
   * Scan library was pressed, and the start, or the feed's answer asked for
   * after it, is still out.
   */
  const [starting, setStarting] = useState(false)
  /** Cancel was pressed, and its sends, or the feed's answer after them, are out. */
  const [cancelSent, setCancelSent] = useState(false)
  const feed = useScanFeed()
  const libraryId = library?.id
  /** Each job holding one of the directories a scan of the library walks. */
  const jobs = libraryId === undefined ? [] : jobsHolding(feed, libraryId)
  const running = jobs.length > 0
  /**
   * An answer since Scan library was last pressed showed a job of the
   * library: its end is the feed's to see, with the row beside it.
   */
  const sawJob = useRef(false)
  if (running) sawJob.current = true
  /** The directories the jobs holding the library walk, by id. */
  const scanning = new Set(jobs.flatMap((job) => job.dirs.map((one) => one.id)))
  /** Cancel was pressed for every job holding the library, and none has stopped. */
  const cancelling = running && jobs.every((job) => job.cancelling)
  /** Why the status last read held the library's removal (`removalHoldIn`). */
  const [removalHeld, setRemovalHeld] = useState<RemovalHold>()
  /** Bumped as each read made for `removalHeld` settles, to ask again. */
  const [heldReads, setHeldReads] = useState(0)
  /** Consecutive failed reads made for `removalHeld`. */
  const heldMisses = useRef(0)
  /**
   * The words each directory's Last scan cell has held while it scanned, by
   * path, since the page was entered. They stay unseen beside what it says
   * after, so that the column keeps the width it took as the row was first
   * drawn scanning until the page is left, and the end of no scan the page
   * draws moves it (O253; `heldWhileScanning` says which it does not draw).
   */
  const heldWords = useRef(new Map<string, Set<string>>())
  // After every render: `scanning` is made anew by each.
  useEffect(() => {
    for (const dir of library?.dirs ?? []) {
      if (!scanning.has(dir.id)) continue
      const held = heldWords.current.get(dir.path) ?? new Set<string>()
      for (const words of heldWhileScanning(dir)) held.add(words)
      heldWords.current.set(dir.path, held)
    }
  })
  /**
   * The scans that have ended since the page was drawn, counted, the
   * directories the last of them held, by id, and the one of those whose
   * report this page leaves to the row beside it.
   */
  const [ended, setEnded] = useState<{
    n: number
    dirs: ReadonlySet<number>
    skip?: string
  }>({ n: 0, dirs: new Set() })
  /** What each directory's status held when the scan the page drew ended. */
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
  // Held in a ref so that the reports read at a scan's end are of the
  // directories the list names then, without the end waiting on a render.
  const walked = useRef(library === undefined ? [] : walkedBy(library))
  walked.current = library === undefined ? [] : walkedBy(library)

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
   * Ends a scan that held `dirs`: the 409's sentence was true while it ran,
   * and the reports of those dirs but `skip` are read.
   */
  const endScan = (dirs: number[], skip?: string) => {
    setConflict(undefined)
    setEnded((was) => ({ n: was.n + 1, dirs: new Set(dirs), skip }))
  }
  /**
   * The status the page last took, which says what holds its removal: the
   * one its body is first drawn from, and each read after it.
   */
  const [taken, setTaken] = useState<LibraryScanStatus | undefined>()
  if (ready && status !== taken) {
    setTaken(status)
    setRemovalHeld(removalHoldIn(status))
  }
  /**
   * The feed's answer the page last compared with the one before it, and
   * the library it compared them for: a job holding the library in that
   * answer and in none since has ended, in the render the answer lands.
   */
  const [compared, setCompared] = useState({ feed, libraryId })
  if (compared.feed !== feed || compared.libraryId !== libraryId) {
    setCompared({ feed, libraryId })
    if (libraryId !== undefined && compared.libraryId === libraryId) {
      const now = new Set(jobs.map((job) => job.id))
      const gone = jobsHolding(compared.feed, libraryId).filter(
        (job) => !now.has(job.id),
      )
      // The row of a directory open beside this page saw the job too, and
      // draws its report as the job leaves the feed.
      if (gone.length > 0) {
        endScan(
          gone.flatMap((job) => job.dirs.map((one) => one.id)),
          beside,
        )
      }
    }
  }

  /**
   * The feed shows a scan, which reads the status again as it ends. A pass
   * holds no removal and may run for hours, so it stops no poll.
   */
  const feedShowsJob = scanJobs(feed).length > 0
  // A scan around or inside one of the library's directories, or a derive,
  // holds its removal and is no scan of the library's, so while it holds and
  // the feed shows no scan to read the status again at its end, the status is
  // read again each poll's interval. The read lands in the store, which hands
  // it to `taken`; the count asks again whether or not that answer changed
  // anything. Failed reads give the hold up, and say so.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `heldReads` is what asks again once a read has settled.
  useEffect(() => {
    if (removalHeld === undefined || feedShowsJob || libraryId === undefined)
      return
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
          setRemovalHeld(undefined)
          setFailure(sentenceOf(err))
        },
      )
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [removalHeld, feedShowsJob, libraryId, heldReads])

  // The report of each directory the ended scan held, of those a scan of the
  // library walks, is read: a directory that joined the library while it ran
  // was not in it. A read that fails draws no report for that directory: the
  // table still says when it was last scanned.
  useEffect(() => {
    if (ended.n === 0) return
    let cancelled = false
    Promise.all(
      walked.current
        .filter(
          (root) => ended.dirs.has(root.id) && String(root.id) !== ended.skip,
        )
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
  const linkedLine = useRef<HTMLParagraphElement>(null)
  // A link said on arrival keeps the room the add form takes as the page is
  // first drawn, measured once it is: Add directory… and the line's words,
  // drawn unseen there until the line says them.
  useEffect(() => {
    if (linked === undefined || linked.room !== undefined) return
    if (addRoot.current === null) return
    setLinked({ ...linked, room: roomOf(addRoot.current) })
  })
  /**
   * Whether the line lies over the control with focus, such as Add
   * directory… under it at a phone's width: the ring is drawn on the line
   * (`settings.css`), as a callout's is.
   */
  const [lineOverFocus, setLineOverFocus] = useState(false)
  // After every render, since any of them may change the line's words or
  // the rows above it.
  useLayoutEffect(() => {
    placeLinked(linkedLine.current, addRoot.current)
    setLineOverFocus(overFocus(linkedLine.current))
  })
  const pageDrawn = library !== undefined && ready
  // A change of the box's size, such as a phone turning, reflows the line's
  // words with no render of the page, and focus moves with none either.
  useEffect(() => {
    const box = addRoot.current?.closest('.settings-box')
    if (!pageDrawn || box == null || typeof ResizeObserver === 'undefined')
      return
    const resized = new ResizeObserver(() => {
      placeLinked(linkedLine.current, addRoot.current)
      setLineOverFocus(overFocus(linkedLine.current))
    })
    resized.observe(box)
    const focusMoved = () => setLineOverFocus(overFocus(linkedLine.current))
    document.addEventListener('focusin', focusMoved)
    document.addEventListener('focusout', focusMoved)
    return () => {
      resized.disconnect()
      document.removeEventListener('focusin', focusMoved)
      document.removeEventListener('focusout', focusMoved)
    }
  }, [pageDrawn])
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
  const addFocusWas = useRef(addFocus)
  // The report that held focus is gone, so focus goes where an add's answer
  // on this page puts it rather than to the document. A page drawn after
  // that add takes nothing from it.
  useEffect(() => {
    if (addFocusWas.current === addFocus) return
    addFocusWas.current = addFocus
    addRoot.current
      ?.querySelector<HTMLElement>('button[name="add-dir"]')
      ?.focus()
  }, [addFocus])

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
  const matchesByContent =
    switched?.from === library.match_by_content
      ? switched.to
      : library.match_by_content
  const pass = fingerprintJob(feed, library.id)
  const matching = matchJob(feed, library.id)
  /**
   * The room a link was said in, once it is known and while the picker is
   * shut: kept after a press takes the line away, so that its going moves
   * nothing either, until the picker takes the room back.
   */
  const kept =
    linked?.room === undefined || adding
      ? undefined
      : { ...linked, room: linked.room }
  /** The link said, while no press has taken it away. */
  const said = kept?.quiet === true ? undefined : kept
  /**
   * A link said on arrival, while its room is still to be measured: its
   * words are drawn unseen, so the room is in the page from its first paint,
   * and the line, a live region drawn empty, says them a commit later.
   */
  const unsaid = adding
    ? undefined
    : linked === undefined
      ? linkedAtMaking
      : linked.room === undefined
        ? linked
        : undefined
  /**
   * When a cancel paused the first of the library's directories it paused,
   * while one is paused: the earliest, since the store writes a time that
   * sorts as a string.
   */
  const pausedSince = library.dirs
    .flatMap((dir) => (dir.paused_at === null ? [] : [dir.paused_at]))
    .sort()[0]

  /**
   * Scan library is held until the feed's answer asked for after the start,
   * or after a 409, lands: the button goes from Scan library to Cancel with
   * no free press between. An answer asked for after the start that holds no
   * job of the library's means the scan has already ended, and ends it here
   * where no poll saw it (`sawJob`); a 409 names no directories.
   */
  const followScan = (dirs: number[]) =>
    askScanFeed().then((answer) => {
      startingRef.current = false
      setStarting(false)
      if (
        answer !== undefined &&
        jobsHolding(answer, library.id).length === 0 &&
        !sawJob.current
      )
        endScan(dirs)
    })

  const scan = () => {
    if (startingRef.current) return
    startingRef.current = true
    sawJob.current = false
    setStarting(true)
    setFailure(undefined)
    setConflict(undefined)
    setReports(undefined)
    onScan?.()
    startLibraryScan(library.id).then(
      // The server's own list: the page's can predate a pattern edit or a
      // nested add, and a directory it names that the scan did not walk
      // would have an earlier report drawn as this scan's.
      (started) => followScan(started.root_ids),
      (err: unknown) => {
        const conflict = statusOf(messageOf(err)) === 409
        if (drawn.current && conflict) {
          setConflict(sentenceOf(err))
          void followScan([])
          return
        }
        startingRef.current = false
        setStarting(false)
        if (drawn.current) setFailure(sentenceOf(err))
        else
          onLateRefusal?.(
            `A scan of ${library.name} did not start (${sentenceOf(err)}).`,
          )
      },
    )
  }

  /**
   * Cancels each job holding the library, with no dialog: a cancel costs at
   * most one directory's unsaved walk, and Scan resumes the rest (spec §7).
   * Held until the feed's answer after the sends lands.
   */
  const cancel = () => {
    setCancelSent(true)
    setFailure(undefined)
    Promise.all(jobs.map((job) => cancelScan(job.id)))
      .then(
        () => askScanFeed(),
        (err: unknown) => {
          if (drawn.current) setFailure(sentenceOf(err))
        },
      )
      .then(() => setCancelSent(false))
  }

  /**
   * The removal's dialog was refused for a hold (a 409), proof of something
   * holding that the page may not draw (O252): the status taken may not name
   * it, or the page gave it up after failed reads. The status is read again
   * through the store, so `taken` draws an answer in new words as it draws a
   * return's read: a removal held around one of the library's directories
   * holds Remove… and is polled until it ends, and a scan of the library is
   * taken up. Silent if it fails: the dialog has said what was refused.
   */
  const heldRefusal = () => {
    const scan = libraryScan(library.id)
    void readNow(scan).then(
      () => {
        // An answer in the words the store holds keeps its object, so
        // `taken` never sees it, and a hold the page gave up would stay
        // given up. So the hold is drawn here too, from the newest answer
        // the store holds: not from this read's, which a newer read may have
        // overtaken, and which would then hold Remove… over a hold that read
        // saw end.
        const kept = entryOf(scan)?.answer
        if (kept !== undefined) setRemovalHeld(removalHoldIn(kept))
      },
      () => undefined,
    )
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

  /**
   * Held while its send is out, the box keeps what the server holds until it
   * answers, so a refusal has nothing to put back.
   */
  const switchMatching = (on: boolean) => {
    if (switchingRef.current) return
    switchingRef.current = true
    setSwitching(true)
    setSwitchFailure(undefined)
    const from = library.match_by_content
    setMatchByContent(library.id, on)
      .then((answer) => {
        switchingRef.current = false
        setSwitching(false)
        setSwitched({ from, to: answer.match_by_content })
        onChanged()
      })
      .catch((err: unknown) => {
        switchingRef.current = false
        setSwitching(false)
        const sentence = sentenceOf(err)
        if (drawn.current) setSwitchFailure(sentence)
        else
          onLateRefusal?.(
            `${MATCH_WORDS} for ${library.name} was not saved (${sentence}).`,
          )
      })
  }

  /** Rejects on a refusal, which is what keeps the picker's typed path. */
  const add = (path: string) => {
    setAddFailure(undefined)
    return addDir(library.id, path, scanOff).then(
      (added) => {
        setLinked(
          added.also_in.length === 0
            ? undefined
            : {
                path: added.root.path,
                also_in: added.also_in,
                schedule: keptSchedule(added),
                room: roomOf(addRoot.current),
                held: false,
              },
        )
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
    // A line saying a link lies over what is below it and takes a press
    // there, so a press of any action on the page takes it away, as a
    // press on it does: it never holds a control beneath it for long.
    <div
      className="library-page"
      onClickCapture={(e) => {
        if (
          said !== undefined &&
          (e.target as Element).closest('button, a, summary, label') !== null
        )
          setLinked(quieted)
      }}
    >
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
            {/* Scan library, which reads Cancel while a scan holds the
                library, and Remove… are held by a scan and by a press of
                their own, either of which can come while the button has
                focus, so each is a `HeldButton` and keeps it. A press of the
                other holds it by `disabled`: focus is on the button pressed
                then, and a button that cannot take focus as its hold comes
                on stays out of the tab order. */}
            <HeldButton
              type="button"
              name="scan-library"
              onClick={running ? cancel : scan}
              held={starting || cancelSent || cancelling}
              disabled={removing}
            >
              {running ? 'Cancel' : 'Scan library'}
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
          {count(
            library.books,
            'PDF or fixed-layout EPUB file',
            'PDF or fixed-layout EPUB files',
          )}
          , and reading those arrives with the books work. A rescan is all this
          library will need then.
        </p>
      )}
      {jobs.map((job) => {
        const covers = coversToo(job, library.id)
        return (
          <Fragment key={job.id}>
            <p className="scanning">
              {job.cancelling ? 'Cancelling…' : stepLine(job)}
            </p>
            {covers !== undefined && <p className="scanning">{covers}</p>}
          </Fragment>
        )
      })}
      {pass !== undefined && <PassLine key={pass.id} job={pass} />}
      {matching !== undefined && <PassLine key={matching.id} job={matching} />}
      {!running && pausedSince !== undefined && (
        <p className="paused">
          Paused until Scan is pressed, since {inUtc(pausedSince)}
        </p>
      )}
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
              <th scope="col" className="overlaps">
                Overlaps
              </th>
            </tr>
          </thead>
          <tbody>
            {library.dirs.map((root) => {
              const to = dirPath(library.name, root.id)
              const overlaps = overlapsOf(root)
              const said = scanning.has(root.id) ? 'Scanning…' : lastScan(root)
              const held = new Set([
                ...(heldWords.current.get(root.path) ?? []),
                ...(scanning.has(root.id) ? heldWhileScanning(root) : []),
              ])
              held.delete(said)
              return (
                // By path, not id: a freed `root.id` is given to the next
                // directory added, and a path cannot collide, since a
                // library's own directories are never the same or nested.
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
                    {scanEvery(root)}
                  </td>
                  <td>
                    <span className="cell-label">Last scan: </span>
                    {said}
                    {/* What the cell has said as it scanned, and the widest
                        words a scan's end can give it, unseen and unread,
                        keep the column as wide as it was made when the row
                        was first drawn scanning, so the paths beside it keep
                        their lines as a scan drawn here ends and nothing
                        below them moves. */}
                    {[...held].map((words) => (
                      <span
                        key={words}
                        className="held-words"
                        aria-hidden="true"
                      >
                        {words}
                      </span>
                    ))}
                  </td>
                  {/* Empty where it overlaps nothing, label and all, so a
                      two-line row has no item there (`lineEnds.ts`). */}
                  <td className="overlaps">
                    {overlaps.length > 0 && (
                      <>
                        <span className="cell-label">Overlaps: </span>
                        {overlaps.join(', ')}
                      </>
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
        {/* Kept at the room measured as the link was said, whatever the
            window does after, so the line moves nothing below it: where its
            words need more, it lies up over what is above it in its box
            (`linkedPlace`). */}
        <div
          className="add-root"
          ref={addRoot}
          data-linked={kept === undefined ? undefined : ''}
          style={kept === undefined ? undefined : { height: `${kept.room}px` }}
        >
          {adding ? (
            <DirPicker
              action="Add directory"
              onPick={add}
              disabled={removing}
              browserHost={browserHost}
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
              onClick={() => {
                setLinked(undefined)
                setScanOff(false)
                setAdding(true)
              }}
              disabled={removing}
            >
              Add directory…
            </button>
          )}
          {/* No help line: the add's room is measured for the link's line
              (`roomOf`), and New library's help says what Off does. O175:
              the `aria-label` is the label's words as written. */}
          {adding && (
            <label className="scan-off">
              <input
                type="checkbox"
                name="scan-off"
                aria-label={SCAN_OFF_WORDS}
                checked={scanOff}
                onChange={(e) => {
                  setAddFailure(undefined)
                  setScanOff(e.target.checked)
                }}
              />
              {SCAN_OFF_WORDS}
            </label>
          )}
          {/* Drawn empty while nothing is said, picker open or not, so that
              a screen reader reads it as it fills. */}
          {/* biome-ignore lint/a11y/useKeyWithClickEvents: it takes no focus, so no key can reach it; the press it takes is a pointer's, which would otherwise reach the control beneath, and it goes at the click, not at the pointer's press, so a tap's click lands on it too. */}
          <p
            className="linked"
            ref={linkedLine}
            data-over-focus={lineOverFocus ? '' : undefined}
            role="status"
            onClick={() => setLinked(quieted)}
          >
            {said !== undefined && (
              <LinkedWords
                path={said.path}
                also_in={said.also_in}
                schedule={said.schedule}
              />
            )}
          </p>
          {unsaid !== undefined && (
            <p className="linked" data-room="" aria-hidden="true">
              <LinkedWords
                path={unsaid.path}
                also_in={unsaid.also_in}
                schedule={unsaid.schedule}
              />
            </p>
          )}
          {addFailure !== undefined && <Refusal>{addFailure}</Refusal>}
          {/* The directory browser, last in the add's row (O316): below
              Don’t scan yet and below a refusal, which answers the buttons,
              so a long listing pushes neither out of sight. */}
          <div className="dir-browser-host" ref={setBrowserHost} />
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

      <Section title="Matching">
        <Row
          check
          label={MATCH_WORDS}
          help="Fingerprints this library's pages in the background, and joins volumes whose pages match as editions of one work."
        >
          {(ids) => (
            // Its send holds it by `aria-disabled`, as an account's boxes
            // are held (O159): `disabled` greys a ticked box and takes its
            // focus. The removal holds it by `disabled`, as it holds
            // Rename…: focus is on Remove… as that hold comes on.
            <input
              type="checkbox"
              id={ids.control}
              name="match-by-content"
              aria-describedby={ids.help}
              aria-disabled={switching}
              disabled={removing}
              checked={matchesByContent}
              onChange={(e) => switchMatching(e.target.checked)}
            />
          )}
        </Row>
        {switchFailure !== undefined && <Refusal>{switchFailure}</Refusal>}
      </Section>

      <RemovalSection title="Removal">
        <Row
          label="Remove this library"
          help={
            // Why a scan it does not walk holds it, which no scan line says
            // (O250); its own scan's lines say theirs.
            running || removalHeld === undefined
              ? 'Asks first, with what it would take.'
              : removalHeld === 'around'
                ? `Waits for a scan of a directory around or inside one of ${library.name}'s directories.`
                : JOB_WAITS
          }
          action
        >
          {(ids) => (
            // Held as Scan library is, and for the same reasons.
            <HeldButton
              ref={removeButton}
              type="button"
              name="remove-library"
              onClick={() => setRemoving(true)}
              held={running || removalHeld !== undefined || removing}
              disabled={starting}
              aria-describedby={ids.help}
            >
              Remove…
            </HeldButton>
          )}
        </Row>
        {removing && (
          <RemoveLibrary
            library={library}
            blocked={running || removalHeld !== undefined}
            onCancel={() => {
              backToRemove.current = true
              setRemoving(false)
            }}
            onRemoved={(answer) => {
              setRemoving(false)
              onRemoved(library.id, library.name, answer)
            }}
            // The dialog draws the refusal itself.
            onRefused={(_, status) => {
              if (status === 409) heldRefusal()
            }}
            onLateRefusal={onLateRefusal}
          />
        )}
      </RemovalSection>
    </div>
  )
}

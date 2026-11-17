/**
 * One directory's page, under its library's: a way back to the library, and
 * the directory's own settings, scan, move and removal, which are `RootRow`'s.
 *
 * **The page holds only what the server refuses.** **A scan of another of
 * its library's directories holds Move… and Remove from this library**: the
 * server refuses a move while a scan of either library is reserved, and that
 * removal while a scan of this one is, and takes the rest, since a scan asked
 * of this directory waits for the other to finish and Remove… does not wait.
 * **A scan of a directory around or inside this one, of any library, holds
 * Remove…**: the server refuses a removal while one is reserved, and takes
 * the scan. **Anything holding the server's job lock holds Move… and Remove
 * from this library** (the serve crate's `LibraryJob` names what takes it):
 * neither waits for the lock, and each is refused while it is held. **A
 * derive holding the store's derive lease, in this server or another
 * process, holds Remove…** (O51): the server refuses a removal while it
 * does. The scan feed (`scanFeed.ts`) shows the first, and the library's
 * status says the second as `held_removals`, the third as `moves_held` and
 * the fourth as `derive_running`. The status is read once on mount, again as
 * the feed comes to show no job, and while the feed shows none and any of
 * the three holds, each poll until none does: a job holds the job lock, and
 * may hold the rest, until it ends. The removal's dry run and its DELETE
 * take the lease themselves, so a read that overlaps either draws no hold
 * from the derive it finds, and a later read that overlaps neither does
 * (`OwnRequests`): drawn from it, the hold would close the dialog whose dry
 * run it found. A failed poll keeps the hold until `POLL_MISSES_ALLOWED`
 * have failed in a row. A scan of this directory, whoever started it, is
 * `RootRow`'s own to draw from the feed.
 *
 * **A scan of this directory marks its library's scan status stale in the
 * store**, as the row reports it was asked for, started or was answered
 * 409, or an action was refused for a hold, and tells no reader of it (spec
 * §9's call 9). A library's page drawn later waits for its own read rather
 * than draw a status from before the scan.
 *
 * **What the directories around and inside this one govern is the server's
 * to say** (`LibraryDir`), drawn under the directory's facts. Whether a
 * library shows a directory's books turns on patterns only the server
 * compiles, so the page draws what it is sent and works none of it out.
 */
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react'
import {
  POLL_MISSES_ALLOWED,
  type Probe,
  type RemovalAnswer,
  SCAN_POLL_MS,
} from './config'
import { dirPath, libraryPath } from './configAddress'
import {
  folderIn,
  type Library,
  type LibraryDir,
  libraryScanStatus,
  type MoveAnswer,
  namesOf,
  type RemovalHold,
  removalHold,
  type UnlinkAnswer,
} from './libraries'
import { type OwnRequests, useOwnRequests } from './deriveHold'
import { sentenceOf } from './message'
import { libraryScan } from './pageReads'
import { markStale } from './reads'
import { PathWithBreaks } from './PathWithBreaks'
import { RootRow } from './RootRow'
import { useScanFeed } from './scanFeed'

export interface DirPageProps {
  library: Library
  root: LibraryDir
  /** Only a directory added in this session has one. */
  probe?: Probe
  /** Every library not holding the directory, for Move…. */
  others: { id: number; name: string }[]
  /**
   * What the screen says above the page: a list that could not be read, or
   * an answer that came once the page it was asked from had gone.
   */
  notices?: ReactNode
  onChanged: () => void
  onRemoved: (answer: RemovalAnswer) => void
  /** The directory left this library alone, which another keeps. */
  onUnlinked: (answer: UnlinkAnswer) => void
  onMoved: (answer: MoveAnswer) => void
  /** A write sent from this page was refused once it had gone. See `RootRow`'s. */
  onLateRefusal?: (sentence: string) => void
  /** The person opened this from the library's Directories table: the heading takes focus. */
  arrived?: boolean
  /** Another library's page of this directory was opened from this one. See `RootRow`'s. */
  onOpen?: (to: string) => void
}

/**
 * Why `rootId`'s removal is held, by a scan around or inside it or by a
 * derive (`removal`), whether the server refuses its move (`move`): for a
 * scan of another of `library`'s directories, which the feed shows, or for
 * the job lock, and why that could no longer be told once a hold was given
 * up. The sentence holds until the library or the directory changes, which
 * asks afresh, or `forget` is called.
 */
function useOtherScan(
  library: Library,
  rootId: number,
  requests: OwnRequests,
): {
  removal: RemovalHold
  move: boolean
  failure?: string
  forget: () => void
  askAgain: () => void
} {
  const feed = useScanFeed()
  const own = new Set(library.dirs.map((one) => one.id))
  // A directory inside one of the library's that it shows holds this one
  // only through `held_removals`.
  const sibling = (feed?.jobs ?? []).some((job) =>
    job.dirs.some((one) => one.id !== rootId && own.has(one.id)),
  )
  const feedShowsJob = (feed?.jobs ?? []).length > 0
  // A ref, so the read below sees the feed's latest answer without being
  // sent again, or dropped, as a job starts.
  const showsJob = useRef(feedShowsJob)
  showsJob.current = feedShowsJob
  /** Bumped by `askAgain`, which runs the read below afresh. */
  const [asked, setAsked] = useState(0)
  const [removal, setRemoval] = useState<RemovalHold>()
  const [movesHeld, setMovesHeld] = useState(false)
  const [failure, setFailure] = useState<string | undefined>()
  /** Stops the read's poll, or reads now and polls on: the feed's to call. */
  const poll = useRef<{ stop: () => void; now: () => void }>(undefined)
  const libraryId = library.id
  // biome-ignore lint/correctness/useExhaustiveDependencies: `asked` is a trigger, not a closure read: `askAgain` bumps it to read the status afresh.
  useEffect(() => {
    // The sentence is about the hold it gave up, not about this one.
    setFailure(undefined)
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    /** Bumped by each read, so only the newest one's answer is drawn. */
    let sent = 0
    /** Whether the last answer held anything. */
    let holding = false
    /** Whether the last answer that could place a derive found one. */
    let deriving = false
    /** Consecutive failed polls. */
    let misses = 0
    const ask = () => {
      sent += 1
      const id = sent
      const mark = requests.mark()
      libraryScanStatus(libraryId)
        .then((status) => {
          if (cancelled || id !== sent) return
          misses = 0
          // The derive may be the page's own removal (`useDeriveHold`'s doc),
          // so it is read again a poll later and the last one stands. The
          // rest of the answer cannot be the removal, and is drawn now.
          const unsure = status.derive_running && requests.overlapped(mark)
          if (!unsure) deriving = status.derive_running
          const removal = removalHold(
            status.held_removals.includes(rootId),
            deriving,
          )
          holding = removal !== undefined || status.moves_held
          setRemoval(removal)
          setMovesHeld(status.moves_held)
          if ((holding || unsure) && !showsJob.current)
            timer = setTimeout(ask, SCAN_POLL_MS)
        })
        .catch((err: unknown) => {
          if (cancelled || id !== sent) return
          // A blip in a poll must not free the buttons while what holds them
          // still does, so the hold outlasts a miss.
          misses += 1
          if (holding && misses < POLL_MISSES_ALLOWED) {
            if (!showsJob.current) timer = setTimeout(ask, SCAN_POLL_MS)
            return
          }
          // A first read that fails held nothing and says nothing. A hold
          // given up is said, as `LibraryPage`'s is: it frees the buttons
          // while the server may still refuse what they send.
          if (holding) setFailure(sentenceOf(err))
          holding = false
          deriving = false
          setRemoval(undefined)
          setMovesHeld(false)
        })
    }
    poll.current = {
      stop: () => clearTimeout(timer),
      now: () => {
        clearTimeout(timer)
        ask()
      },
    }
    ask()
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [libraryId, rootId, asked, requests])
  /** Whether the feed showed a job when the effect below last ran. */
  const sawJob = useRef(feedShowsJob)
  // While the feed shows a job the status is not polled: the job holds the
  // job lock, and may hold the rest, until it ends, and its end is when the
  // status is read again.
  useEffect(() => {
    if (feedShowsJob) poll.current?.stop()
    else if (sawJob.current) poll.current?.now()
    sawJob.current = feedShowsJob
  }, [feedShowsJob])
  // Stable, so `RootRow`'s Scan directory handler is not made afresh each draw.
  const forget = useCallback(() => setFailure(undefined), [])
  // Asks afresh for a refusal that says something holds which no read here
  // has seen, so that the hold is drawn and polled until it ends.
  const askAgain = useCallback(() => setAsked((n) => n + 1), [])
  return { removal, move: sibling || movesHeld, failure, forget, askAgain }
}

/**
 * What `root`'s page says of the directories around and inside it, as the
 * server sends them (`LibraryDir`): a line for each directory it sits in,
 * outermost first, then the directories inside it, folded under their count
 * so that a directory holding many stays one line until it is opened.
 * Nothing where there are none. `library` is the name of `root`'s library.
 * Each other directory is said with every library holding it.
 */
function Nesting({ root, library }: { root: LibraryDir; library: string }) {
  if (root.inside.length === 0 && root.nested.length === 0) return null
  const n = root.nested.length
  return (
    <div className="nesting">
      {root.inside.map((one) => {
        const names = namesOf(one.libraries)
        const its = one.libraries.length === 1 ? 'its' : 'their'
        return (
          <p key={one.path}>
            {one.shows
              ? `Also shown in ${names}`
              : one.kept_out_by === 'dot_name'
                ? `Kept out of ${names}: ${its} scans skip names starting with a dot`
                : `Kept out of ${names} by ${its} exclusions`}{' '}
            (inside{' '}
            <code>
              <PathWithBreaks path={one.path} />
            </code>
            )
          </p>
        )
      })}
      {n > 0 && (
        <details>
          <summary>
            {n === 1
              ? "1 folder inside uses its own library's rules"
              : `${n} folders inside use their own library's rules`}
          </summary>
          <ul>
            {root.nested.map((one) => (
              <li key={one.path}>
                <code>
                  <PathWithBreaks path={folderIn(one.path, root.path)} />
                </code>
                <span>{namesOf(one.libraries)}</span>
                <span>
                  {one.shown
                    ? `also shown in ${library}`
                    : one.kept_out_by === 'dot_name'
                      ? 'kept out: names starting with a dot are skipped'
                      : "kept out by this directory's exclusions"}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )
}

export function DirPage({
  library,
  root,
  probe,
  others,
  notices,
  onChanged,
  onRemoved,
  onUnlinked,
  onMoved,
  onLateRefusal,
  arrived,
  onOpen,
}: DirPageProps) {
  /** The removal's dry run and its DELETE, each of which takes the lease. */
  const requests = useOwnRequests()
  const {
    removal: removalHeld,
    move: moveHeld,
    failure,
    forget,
    askAgain,
  } = useOtherScan(library, root.id, requests)
  return (
    <div className="dir-page">
      <RootRow
        // By library and path, for the reasons the screen keys this page so.
        key={`${library.id}:${root.path}`}
        back={{
          to: libraryPath(library.name),
          label: library.name,
          closes: dirPath(library.name, root.id),
        }}
        notices={
          <>
            {notices}
            {failure !== undefined && <p role="alert">{failure}</p>}
          </>
        }
        root={root}
        nesting={<Nesting root={root} library={library.name} />}
        arrived={arrived}
        probe={probe}
        library={{ id: library.id, name: library.name }}
        libraries={root.libraries}
        removalHeld={removalHeld}
        ownRequests={requests}
        moveHeld={moveHeld}
        // A scan of this directory is a fresh start, and the sentence of a
        // hold given up says nothing of it.
        onScan={forget}
        onScanMoved={() => markStale(libraryScan(library.id))}
        onHeldRefusal={askAgain}
        moveTo={others}
        onChanged={onChanged}
        onRemoved={onRemoved}
        onUnlinked={onUnlinked}
        onMoved={onMoved}
        onLateRefusal={onLateRefusal}
        onOpen={onOpen}
      />
    </div>
  )
}

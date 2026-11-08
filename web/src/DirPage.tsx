/**
 * One directory's page, under its library's: a way back to the library, and
 * the directory's own settings, scan, move and removal, which are `RootRow`'s.
 *
 * **The page holds only what the server refuses.** **A scan of another of
 * its library's directories holds Move… alone**: the server refuses a move
 * while a scan of either library is reserved, and takes the rest, since a
 * scan asked of this directory waits for the other to finish and a removal
 * does not wait. **A scan of a directory around or inside this one, of any
 * library, holds Remove…**: the server refuses a removal while one is
 * reserved, and takes the scan. **Anything holding the server's job lock
 * holds Move…**, a running scan of any library, a rule save, a stamp or
 * another move: a move does not wait for the lock and is refused while it is
 * held. The library's status says the first in `dirs`, the second as
 * `held_removals` and the third as `moves_held`, so it is read once on mount
 * and, while any holds, again each poll until none does.
 * A failed poll keeps the hold until `POLL_MISSES_ALLOWED` have failed in a
 * row, as `RootRow`'s poll keeps watching its scan. A scan of this
 * directory alone is `RootRow`'s own to see. One the library's status names
 * beside a sibling's, as a scan of the whole library is, holds all three from
 * that read: the library's scan holds each directory's slot, and the server
 * refuses all three while this one's is reserved. Once a poll finds that nothing
 * holds the page any more after something did, the Library views are marked
 * stale, as `RootRow`'s and `LibraryPage`'s polls mark them: what a scan
 * found changes what they draw. A first read that finds nothing holding, and
 * a hold given up on failed polls, saw no scan end and mark nothing.
 *
 * **A scan of this directory marks its library's scan status stale in the
 * store**, as the row reports it was asked for, started, was found running,
 * ended or was left running, and tells no reader of it (spec §9's call 9).
 * A library's page drawn later waits for its own read rather than draw a
 * status from before the scan. One drawn beside this page keeps what it
 * drew: told, it would read the status, take the scan up, and draw its
 * report a second time beside the row's own.
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
  type UnlinkAnswer,
} from './libraries'
import { sentenceOf } from './message'
import { invalidateLibraryViews, libraryScan } from './pageReads'
import { markStale } from './reads'
import { PathWithBreaks } from './PathWithBreaks'
import { RootRow } from './RootRow'

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
  onChanged: (quiet?: boolean) => void
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
 * Whether the library's status names `rootId` scanning beside another of
 * `library`'s directories, as a scan of the library does (`self`), whether a
 * scan around or inside `rootId` holds its removal (`removal`), whether the
 * server refuses its move (`move`): for the job lock, or for a scan of
 * another of `library`'s directories, and why that could no longer be told
 * once a hold was given up. The sentence holds until the library or the
 * directory changes, which asks afresh, or `forget` is called.
 */
function useOtherScan(
  library: Library,
  rootId: number,
): {
  self: boolean
  removal: boolean
  move: boolean
  failure?: string
  forget: () => void
  askAgain: () => void
} {
  /** Bumped by `askAgain`, which runs the read below afresh. */
  const [asked, setAsked] = useState(0)
  const [mine, setMine] = useState(false)
  const [removal, setRemoval] = useState(false)
  const [move, setMove] = useState(false)
  const [failure, setFailure] = useState<string | undefined>()
  // A ref, so a list read again does not restart the poll.
  const own = useRef<ReadonlySet<number>>(new Set())
  own.current = new Set(library.dirs.map((one) => one.id))
  const libraryId = library.id
  // biome-ignore lint/correctness/useExhaustiveDependencies: `asked` is a trigger, not a closure read: `askAgain` bumps it to read the status afresh.
  useEffect(() => {
    // The sentence is about the hold it gave up, not about this one.
    setFailure(undefined)
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    /** Whether the last answer held anything. */
    let holding = false
    /** Consecutive failed polls, counted as `RootRow` counts its own. */
    let misses = 0
    const ask = () => {
      libraryScanStatus(libraryId)
        .then((status) => {
          if (cancelled) return
          misses = 0
          const wasHolding = holding
          // A directory inside one of the library's that it shows is named
          // too, and holds this one only through `held_removals`.
          const sibling = status.dirs.some(
            (one) =>
              one.running &&
              one.root_id !== rootId &&
              own.current.has(one.root_id),
          )
          // Named beside a sibling, this directory's own scan holds all three
          // before its own status says so, and the sibling keeps the poll
          // going until it ends. Named alone, it is `RootRow`'s own to see.
          const self =
            sibling &&
            status.dirs.some((one) => one.running && one.root_id === rootId)
          const around = status.held_removals.includes(rootId)
          holding = sibling || around || status.moves_held
          setMine(self)
          setRemoval(around)
          setMove(sibling || status.moves_held)
          if (holding) timer = setTimeout(ask, SCAN_POLL_MS)
          // The scan this page saw running is over, and what it found
          // changes what the Library views draw.
          else if (wasHolding) invalidateLibraryViews()
        })
        .catch((err: unknown) => {
          if (cancelled) return
          // A blip in a poll must not free the buttons while the other
          // directory still scans, so the hold outlasts a miss as `RootRow`'s
          // poll does.
          misses += 1
          if (holding && misses < POLL_MISSES_ALLOWED) {
            timer = setTimeout(ask, SCAN_POLL_MS)
            return
          }
          // A first read that fails held nothing and says nothing, as
          // `RootRow`'s mount read does. A hold given up is said, as
          // `RootRow`'s and `LibraryPage`'s polls say theirs: it frees the
          // buttons while the server may still refuse what they send.
          if (holding) setFailure(sentenceOf(err))
          holding = false
          setMine(false)
          setRemoval(false)
          setMove(false)
        })
    }
    ask()
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [libraryId, rootId, asked])
  // Stable, so `RootRow`'s Scan directory handler is not made afresh each draw.
  const forget = useCallback(() => setFailure(undefined), [])
  // Asks afresh for a refusal that says something holds which no read here
  // has seen, so that the hold is drawn and polled until it ends.
  const askAgain = useCallback(() => setAsked((n) => n + 1), [])
  return { self: mine, removal, move, failure, forget, askAgain }
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
  const {
    self: held,
    removal: removalHeld,
    move: moveHeld,
    failure,
    forget,
    askAgain,
  } = useOtherScan(library, root.id)
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
        held={held}
        removalHeld={removalHeld}
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

/**
 * One directory's page, under its library's: a way back to the library, and
 * the directory's own settings, scan, move and removal, which are `RootRow`'s.
 *
 * **Another directory's scan holds this one's Scan directory, Move… and
 * Remove…**, because of the three the server refuses only the move for it. It
 * refuses a scan or a removal of a directory only while that directory itself
 * is being scanned; a scan asked of a second directory waits for the first to
 * finish, and a removal does not wait. The library's status names every
 * directory that is scanning, so it is read once on mount and, while another
 * directory is named, again each poll until none is. A failed poll keeps the
 * hold until `POLL_MISSES_ALLOWED` have failed in a row, as `RootRow`'s poll
 * keeps watching its scan. A scan of this directory, or of the whole library,
 * is `RootRow`'s own to see: the library's scan holds each directory's slot.
 */
import { type ReactNode, useCallback, useEffect, useState } from 'react'
import { Link } from 'react-router'
import {
  POLL_MISSES_ALLOWED,
  type Probe,
  type RemovalAnswer,
  type Root,
  SCAN_POLL_MS,
} from './config'
import { type Library, libraryScanStatus, type MoveAnswer } from './libraries'
import { sentenceOf } from './message'
import { RootRow } from './RootRow'

export interface DirPageProps {
  library: Library
  root: Root
  /** Only a directory added in this session has one. */
  probe?: Probe
  /** Every other library, for Move…. */
  others: { id: number; name: string }[]
  /**
   * What the screen says above the page: a list that could not be read, or
   * an answer that came once the page it was asked from had gone.
   */
  notices?: ReactNode
  onChanged: (quiet?: boolean) => void
  onRemoved: (answer: RemovalAnswer) => void
  onMoved: (answer: MoveAnswer) => void
  /** A write sent from this page was refused once it had gone. See `RootRow`'s. */
  onLateRefusal?: (sentence: string) => void
}

/**
 * Whether a directory of `libraryId` other than `rootId` is scanning, and why
 * that could no longer be told once a hold was given up. The sentence holds
 * until the library or the directory changes, which asks afresh, or `forget`
 * is called.
 */
function useOtherScan(
  libraryId: number,
  rootId: number,
): { other: boolean; failure?: string; forget: () => void } {
  const [other, setOther] = useState(false)
  const [failure, setFailure] = useState<string | undefined>()
  useEffect(() => {
    // The sentence is about the hold it gave up, not about this one.
    setFailure(undefined)
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    /** Whether the last answer named another directory scanning. */
    let holding = false
    /** Consecutive failed polls, counted as `RootRow` counts its own. */
    let misses = 0
    const ask = () => {
      libraryScanStatus(libraryId)
        .then((status) => {
          if (cancelled) return
          misses = 0
          holding = status.dirs.some(
            (one) => one.running && one.root_id !== rootId,
          )
          setOther(holding)
          if (holding) timer = setTimeout(ask, SCAN_POLL_MS)
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
          // buttons while the other directory may still be scanning, and the
          // server refuses neither this directory's scan nor its removal for
          // that. Only Move… is still refused.
          if (holding) setFailure(sentenceOf(err))
          holding = false
          setOther(false)
        })
    }
    ask()
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [libraryId, rootId])
  // Stable, so `RootRow`'s Scan directory handler is not made afresh each draw.
  const forget = useCallback(() => setFailure(undefined), [])
  return { other, failure, forget }
}

export function DirPage({
  library,
  root,
  probe,
  others,
  notices,
  onChanged,
  onRemoved,
  onMoved,
  onLateRefusal,
}: DirPageProps) {
  const { other: held, failure, forget } = useOtherScan(library.id, root.id)
  return (
    <div className="dir-page">
      <p className="back">
        <Link to={`/config/libraries/${library.id}`}>‹ {library.name}</Link>
      </p>
      {notices}
      {failure !== undefined && <p role="alert">{failure}</p>}
      <RootRow
        // By path, for the reason the library's table keys its rows so.
        key={root.path}
        root={root}
        probe={probe}
        lastOf={library.dirs.length === 1 ? library.name : undefined}
        held={held}
        // A scan of this directory is a fresh start, and the sentence of a
        // hold given up says nothing of it.
        onScan={forget}
        moveTo={{
          from: { id: library.id, name: library.name },
          targets: others,
        }}
        onChanged={onChanged}
        onRemoved={onRemoved}
        onMoved={onMoved}
        onLateRefusal={onLateRefusal}
      />
    </div>
  )
}

/**
 * Whether a derive or the server's job lock holds now, for the pages that
 * read no library's scan status: the Rules page and a work's Write
 * ComicInfo.xml (O51).
 *
 * **A page holds only what the server refuses.** A derive holds the store's
 * derive lease, in this server or another process, and the server refuses a
 * preview of the rules, a save of them and a stamp while it does; the job
 * lock refuses a save and a stamp as well. So the status is read once the
 * page can send one of those, and while either holds it is read again each
 * `SCAN_POLL_MS` until neither does, as `DirPage`'s holds are. A failed read
 * keeps the hold until `POLL_MISSES_ALLOWED` have failed in a row, and a
 * hold given up is said: it frees what the server may still refuse.
 *
 * **A read cannot tell the page's own request from another derive.** The
 * server's `derive_running` is true for every holder of the lease in its
 * process, so a read sent while one of the page's own requests that take
 * the lease is out may be describing that request (`OwnRequests`). Such a
 * read that finds a derive changes nothing, and the status is read again a
 * poll later: a hold drawn from it would drop or close the very request it
 * describes. The request's own answer settles it, since its lease excluded
 * every other derive while it held: a 200 is current, and a 409 reads again.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { POLL_MISSES_ALLOWED, SCAN_POLL_MS } from './config'
import { deriveStatus } from './libraries'
import { sentenceOf } from './message'

/**
 * What a hold for a derive or the job lock says, beside what it holds. It
 * names no holder, as the server's refusal names none (O245).
 */
export const JOB_WAITS = 'Waits for a scan or another library job to finish.'

/** A page's own requests that take the derive lease, counted while out. */
export interface OwnRequests {
  /** `request`, counted as the page's own until it settles. */
  track: <T>(request: Promise<T>) => Promise<T>
  /** Taken as a status read is sent, for `overlapped`. */
  mark: () => number
  /**
   * Whether one of the page's own requests was out at any time since
   * `mark` was taken, so that the read sent then may describe it.
   */
  overlapped: (mark: number) => boolean
}

export function ownRequests(): OwnRequests {
  let out = 0
  let sent = 0
  return {
    track: (request) => {
      out += 1
      sent += 1
      return request.finally(() => {
        out -= 1
      })
    },
    // A request out at the mark overlaps the read for as long as it is out,
    // and one sent since does, whether or not it is still out. The lease is
    // exclusive, so while the page's own request is out the derive a read
    // finds is that request, or one its 409 will report: drawn as a hold, it
    // would close or drop that request. One that never settles keeps reads
    // discounted while it is out, and a press then meets the server's 409.
    mark: () => (out > 0 ? -1 : sent),
    overlapped: (mark) => mark < 0 || sent !== mark,
  }
}

/** One `OwnRequests` for the page's lifetime. */
export function useOwnRequests(): OwnRequests {
  const [own] = useState(ownRequests)
  return own
}

export interface DeriveHold {
  /** A derive holds the lease, as the last read said. */
  derive: boolean
  /** The job lock is held, as the last read said. */
  job: boolean
  /**
   * Bumped as each read made while a hold or a refusal stood is taken, and
   * as a hold is given up, so that what a page owes to a hold's end, or to a
   * refusal's read, is sent then. A read discounted for the page's own
   * request, or finding nothing with nothing held before it, changes nothing
   * a page draws, so it bumps nothing.
   */
  reads: number
  /** Why a hold was given up after failed reads. */
  failure?: string
  /**
   * A refusal (a 409) says something holds which no read here has seen:
   * the status is read now, its read kept through failures as a hold's is,
   * so that the hold is drawn and polled until it ends.
   */
  askAgain: () => void
}

/**
 * Reads nothing while `active` is false, and holds nothing then. `own` is
 * the page's own requests that take the lease, whose reads are discounted.
 */
export function useDeriveHold(active: boolean, own?: OwnRequests): DeriveHold {
  /** Bumped by `askAgain`, which runs the read below afresh. */
  const [asked, setAsked] = useState(0)
  const [derive, setDerive] = useState(false)
  const [job, setJob] = useState(false)
  const [reads, setReads] = useState(0)
  const [failure, setFailure] = useState<string | undefined>()
  /** Set by `askAgain`: the read it starts is one made while holding. */
  const refused = useRef(false)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `asked` is a trigger, not a closure read: `askAgain` bumps it to read the status afresh.
  useEffect(() => {
    // The sentence is about the hold it gave up, not about this one.
    setFailure(undefined)
    if (!active) {
      setDerive(false)
      setJob(false)
      return
    }
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    /** Whether the last answer held anything, or a refusal said so. */
    let holding = refused.current
    refused.current = false
    let misses = 0
    const ask = () => {
      const mark = own?.mark()
      deriveStatus()
        .then((status) => {
          if (cancelled) return
          misses = 0
          // The derive may be the page's own request (this file's doc).
          if (
            status.derive_running &&
            mark !== undefined &&
            own?.overlapped(mark)
          ) {
            timer = setTimeout(ask, SCAN_POLL_MS)
            return
          }
          const held = holding
          holding = status.derive_running || status.job_held
          setDerive(status.derive_running)
          setJob(status.job_held)
          if (held) setReads((n) => n + 1)
          if (holding) timer = setTimeout(ask, SCAN_POLL_MS)
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses += 1
          if (holding && misses < POLL_MISSES_ALLOWED) {
            timer = setTimeout(ask, SCAN_POLL_MS)
            return
          }
          // A first read that fails held nothing and says nothing.
          if (holding) {
            setFailure(sentenceOf(err))
            setReads((n) => n + 1)
          }
          holding = false
          setDerive(false)
          setJob(false)
        })
    }
    ask()
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [active, asked, own])
  const askAgain = useCallback(() => {
    refused.current = true
    setAsked((n) => n + 1)
  }, [])
  return { derive, job, reads, failure, askAgain }
}

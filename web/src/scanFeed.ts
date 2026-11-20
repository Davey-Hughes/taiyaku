/**
 * `GET /api/library/scans`, polled once per tab for every page that shows a
 * scan, and `POST /api/library/scans/{job}/cancel` (spec
 * 2026-11-15-first-scan-progress-and-cancel §7, "One feed client per tab").
 *
 * **One poll, whoever draws it.** The first `useScanFeed` starts the poll
 * and the last to go stops it, so the pages a tab draws from it at once, a
 * library's page and a directory's open beside it say, send one request,
 * not one each. It asks every `SCAN_POLL_MS` while any job holds a library
 * or a pass runs, the fingerprint pass or the match pass, which hold none but
 * whose counts move as they read; every `FEED_IDLE_MS` otherwise, as for a
 * reader while only a pass runs, since a reader is sent no jobs; and never
 * while the tab is hidden.
 * Shown again, it asks at once.
 *
 * **The feed says when to read again.** When a scan leaves it, or `ended`
 * moves, the reads a scan changes are read again: the library status, the
 * libraries and directories, and the Library views. `ended` is what catches
 * a scan that started and ended between two polls, or while the tab was
 * hidden, which no poll ever listed. A first answer has nothing to compare,
 * so it reads again only where a poll was missed before it (`missed`).
 */
import { useSyncExternalStore } from 'react'
import { getJson, sendJson } from './api'
import { humanBytes } from './bytes'
import { expectBody, SCAN_POLL_MS } from './config'
import { namesOf } from './libraries'
import {
  invalidateLibraryViews,
  LIBRARIES,
  LIBRARY_STATUS,
  ROOTS,
} from './pageReads'
import { invalidatePrefix } from './reads'

/**
 * The step a job is in: a scan's, in the order a scan passes through them,
 * then the fingerprint pass's one, then the match pass's one.
 */
export type ScanStep =
  | 'waiting'
  | 'listing'
  | 'resolving'
  | 'hashing'
  | 'writing'
  | 'finishing'
  | 'fingerprinting'
  | 'matching'

/** One running job, as an administrator is sent it. */
export interface FeedJob {
  id: number
  /**
   * A scan, or a pass: the fingerprint pass or the match pass, which hold no
   * directory, are never in the feed's `libraries`, and do not move `ended`
   * by ending: nothing a scan holds back waits on them.
   */
  kind: 'scan' | 'fingerprint' | 'match'
  step: ScanStep
  /** The step's count so far: `null` while waiting and finishing. */
  done: number | null
  /**
   * What `done` counts towards, `null` while it is not known: listing never
   * knows it, and hashing knows it once every folder is sized. A pass's is
   * the units queued so far, across every listing of the pass, and a unit
   * queued twice counts twice; `null` until the first is queued.
   */
  of: number | null
  /** The directory being listed or read, of the job's; else `null`. */
  dir: { at: number; of: number } | null
  dirs: { id: number; path: string }[]
  libraries: { id: number; name: string }[]
  elapsed_secs: number
  /** Cancel was pressed and the job has not stopped yet. */
  cancelling: boolean
}

/** What the feed answers. */
export interface ScanFeed {
  /**
   * Moves each time a scan job ends, and each time the match pass derives,
   * which joins volumes; its value means nothing else. A pass's end leaves
   * it, since ending changes nothing the pages read.
   */
  ended: number
  /**
   * Each library a running job holds, with the step of the lowest job id
   * holding it: for an administrator every one, for anyone else those it
   * is granted.
   */
  libraries: { id: number; step: ScanStep }[]
  /** Every running job, sent to an administrator only. */
  jobs?: FeedJob[]
}

/** How the server answers a cancel: asked, or how the job had ended. */
export interface CancelAnswer {
  job: number
  cancelling?: true
  ended?: string
  step?: ScanStep
}

/**
 * How often the feed is asked while no job holds a library and no pass is
 * listed, as for a reader while a pass runs, and after a failed read.
 */
export const FEED_IDLE_MS = 5000

const FEED = '/api/library/scans'

const listeners = new Set<() => void>()
/** Told of each scan's end, before the reads it changed are sent again. */
const scanEndListeners = new Set<() => void>()
/**
 * The last answer, kept while nothing draws it: a page that replaces another
 * in one commit draws what the other drew, not nothing until its first poll
 * lands.
 */
let latest: ScanFeed | undefined
let timer: ReturnType<typeof setTimeout> | undefined
/**
 * A poll was dropped for a hidden tab, or a read failed, before any answer
 * landed. The pages drew their reads meanwhile, and a scan may have ended
 * since, which the first answer cannot tell: without this, a page mounted in
 * a hidden tab keeps reads from before the scan's end until it is left.
 */
let missed = false
/** Bumped by every ask and by stopping, so only the newest ask's answer lands. */
let asked = 0
/** The ask `askScanFeed` sent, while it is the newest and has not landed. */
let askedNow: number | undefined
/** Each `askScanFeed` call waiting for the newest ask to land. */
const waiting: ((answer: ScanFeed | undefined) => void)[] = []

/** Hands each waiting caller `answer`, `undefined` where none landed. */
function settle(answer: ScanFeed | undefined): void {
  askedNow = undefined
  for (const resolve of waiting.splice(0)) resolve(answer)
}

function ask(): void {
  clearTimeout(timer)
  timer = undefined
  // Asked again when the tab is shown, so a poll due while hidden is dropped
  // rather than put off.
  if (document.visibilityState === 'hidden') {
    if (latest === undefined) missed = true
    return
  }
  asked += 1
  const id = asked
  getJson<ScanFeed>(FEED).then(
    (answer) => {
      if (id !== asked) return
      if (latest === undefined ? missed : scanEnded(latest, answer))
        readScanAgain()
      latest = answer
      for (const listener of listeners) listener()
      settle(answer)
      timer = setTimeout(
        ask,
        answer.libraries.length > 0 ||
          fingerprintJob(answer) !== undefined ||
          matchJob(answer) !== undefined
          ? SCAN_POLL_MS
          : FEED_IDLE_MS,
      )
    },
    () => {
      // The last answer is kept: one failed read says nothing of the scan.
      if (id !== asked) return
      if (latest === undefined) missed = true
      settle(undefined)
      timer = setTimeout(ask, FEED_IDLE_MS)
    },
  )
}

/**
 * Whether a scan ended between `before` and `now`. A library is compared as
 * well as a job, since a reader is sent no jobs, and either can leave an
 * answer whose `ended` was read before the job ended. A pass's end is no
 * scan's: it changes none of the reads `readScanAgain` sends.
 */
function scanEnded(before: ScanFeed, now: ScanFeed): boolean {
  if (now.ended !== before.ended) return true
  const jobs = new Set(scanJobs(now).map((job) => job.id))
  const libraries = new Set(now.libraries.map((library) => library.id))
  return (
    scanJobs(before).some((job) => !jobs.has(job.id)) ||
    before.libraries.some((library) => !libraries.has(library.id))
  )
}

/**
 * The reads a scan changes. One `invalidatePrefix` call is one set, so a page
 * drawn from several of the first call's reads moves once. The Library views
 * are marked in sets of their own, so a page drawn from them and from these
 * may move more than once. `LIBRARIES`' address is also the prefix of each
 * library's own reads.
 */
function readScanAgain(): void {
  for (const listener of scanEndListeners) listener()
  invalidatePrefix(LIBRARY_STATUS.address, LIBRARIES.address, ROOTS.address)
  invalidateLibraryViews()
}

function onVisibilityChange(): void {
  if (document.visibilityState === 'visible') ask()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (listeners.size === 1) {
    document.addEventListener('visibilitychange', onVisibilityChange)
    ask()
  }
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) stop()
  }
}

function stop(): void {
  clearTimeout(timer)
  timer = undefined
  asked += 1
  document.removeEventListener('visibilitychange', onVisibilityChange)
  settle(undefined)
}

/**
 * The feed's last answer, `undefined` before the first; drawn again on each
 * answer. While any component draws it, the tab polls.
 */
export function useScanFeed(): ScanFeed | undefined {
  return useSyncExternalStore(subscribe, () => latest)
}

/**
 * Asks the feed now, for a page that has just started or cancelled a scan,
 * and hands back the answer once it lands: `undefined` where it failed, or
 * where nothing was asked because nothing draws the feed or the tab is
 * hidden, so a caller is never held for good. An ask this sent and still
 * out is shared; a poll still out is not, since it may have been sent before
 * the caller's write was answered, and is overtaken.
 */
export function askScanFeed(): Promise<ScanFeed | undefined> {
  if (listeners.size === 0 || document.visibilityState === 'hidden') {
    return Promise.resolve(undefined)
  }
  return new Promise((resolve) => {
    waiting.push(resolve)
    if (askedNow === asked) return
    ask()
    askedNow = asked
  })
}

/**
 * Calls `listener` at each scan's end the feed sees, just before it sends the
 * reads that scan changed: a screen tells those reads, which no press of its
 * own started, from its own. Hands back what stops it.
 */
export function onScanEnd(listener: () => void): () => void {
  scanEndListeners.add(listener)
  return () => {
    scanEndListeners.delete(listener)
  }
}

/** Every running scan job: every job but the two passes. */
export function scanJobs(feed: ScanFeed | undefined): FeedJob[] {
  return (feed?.jobs ?? []).filter((job) => job.kind === 'scan')
}

/**
 * The pass of `kind`, if one runs; given `libraryId`, only while the pass
 * covers that library.
 */
function passJob(
  feed: ScanFeed | undefined,
  kind: 'fingerprint' | 'match',
  libraryId: number | undefined,
): FeedJob | undefined {
  return feed?.jobs?.find(
    (job) =>
      job.kind === kind &&
      (libraryId === undefined ||
        job.libraries.some((library) => library.id === libraryId)),
  )
}

/**
 * The fingerprint pass, if one runs; given `libraryId`, only while the pass
 * covers that library.
 */
export function fingerprintJob(
  feed: ScanFeed | undefined,
  libraryId?: number,
): FeedJob | undefined {
  return passJob(feed, 'fingerprint', libraryId)
}

/**
 * The match pass, if one runs; given `libraryId`, only while the pass covers
 * that library.
 */
export function matchJob(
  feed: ScanFeed | undefined,
  libraryId?: number,
): FeedJob | undefined {
  return passJob(feed, 'match', libraryId)
}

/**
 * Every scan job holding a directory of library `libraryId`. A pass covering
 * it holds none, so a page offers Scan library beside it.
 */
export function jobsHolding(
  feed: ScanFeed | undefined,
  libraryId: number,
): FeedJob[] {
  return scanJobs(feed).filter((job) =>
    job.libraries.some((library) => library.id === libraryId),
  )
}

/** The scan job holding directory `rootId`, if one does. */
export function jobHoldingDir(
  feed: ScanFeed | undefined,
  rootId: number,
): FeedJob | undefined {
  return scanJobs(feed).find((job) => job.dirs.some((dir) => dir.id === rootId))
}

/**
 * What a page of library `libraryId` says under `job`'s step: the other
 * libraries it covers, `undefined` where there are none. Cancel stops the
 * whole job, so a press on this page stops their scans too.
 */
export function coversToo(job: FeedJob, libraryId: number): string | undefined {
  const others = job.libraries
    .filter((one) => one.id !== libraryId)
    .map((one) => one.name)
  return others.length === 0
    ? undefined
    : `This scan covers ${namesOf(others)} too.`
}

/**
 * The step of a job holding library `libraryId`: what a reader, who is sent
 * no jobs, is shown.
 */
export function libraryStep(
  feed: ScanFeed | undefined,
  libraryId: number,
): ScanStep | undefined {
  return feed?.libraries.find((library) => library.id === libraryId)?.step
}

const STEP_WORDS: Record<ScanStep, string> = {
  waiting: 'Waiting for another job',
  listing: 'Listing',
  resolving: 'Reading files',
  hashing: 'Hashing',
  writing: 'Writing works',
  finishing: 'Finishing',
  fingerprinting: 'Fingerprinting pages',
  matching: 'Matching volumes',
}

/** A step by its word alone, as a reader is shown it and `stepLine` opens. */
export function stepWord(step: ScanStep): string {
  return STEP_WORDS[step]
}

const figure = (n: number) => n.toLocaleString('en')

/** `done of of`, or `done` alone while the total is not known. */
function outOf(done: number, of: number | null, unit: (n: number) => string) {
  return of === null ? unit(done) : `${unit(done)} of ${unit(of)}`
}

/**
 * A job's step and its count, as an administrator is shown it: "Reading
 * files · 120 of 2,097 · directory 2 of 5".
 */
export function stepLine(job: FeedJob): string {
  const word = stepWord(job.step)
  const done = job.done ?? 0
  const onDir = (line: string) =>
    job.dir === null
      ? line
      : `${line} · directory ${job.dir.at} of ${job.dir.of}`
  switch (job.step) {
    case 'listing':
      return onDir(
        `${word} · ${figure(done)} ${done === 1 ? 'file' : 'files'} found`,
      )
    case 'resolving':
      return onDir(`${word} · ${outOf(done, job.of, figure)}`)
    case 'hashing':
      // Folders are sized before the first byte is hashed.
      return job.of === null
        ? `${word} · sizing files`
        : `${word} · ${outOf(done, job.of, humanBytes)}`
    case 'writing':
    // The step's word names what its figures count.
    case 'matching':
      return `${word} · ${outOf(done, job.of, figure)}`
    case 'fingerprinting': {
      // The step's word names pages and its figures count files, so the noun
      // says which, agreeing with the figure it follows.
      const files = (job.of ?? done) === 1 ? 'file' : 'files'
      return `${word} · ${outOf(done, job.of, figure)} ${files}`
    }
    default:
      return word
  }
}

/**
 * Asks job `job` to stop. A running job answers `cancelling` however often
 * it is asked; one that has ended answers how it ended.
 */
export const cancelScan = (job: number): Promise<CancelAnswer> =>
  expectBody(sendJson<CancelAnswer>(`/api/library/scans/${job}/cancel`, 'POST'))

/**
 * Signing in or out, and every test's end: stops the poll and forgets the
 * last answer, which was the account's that read it.
 */
export function resetScanFeed(): void {
  listeners.clear()
  stop()
  latest = undefined
  missed = false
}

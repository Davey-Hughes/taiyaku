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
 * moves, the reads a scan changes that a page draws are read again, as one
 * set: the library status, the libraries and directories, the Library views
 * and the split editions. `ended` is what catches a scan that started and
 * ended between two polls, or while the tab was hidden, which no poll ever
 * listed. A first answer has nothing to compare, so it reads again only where
 * a poll was missed before it (`missed`). Where `written` moves in an answer
 * that ends no scan, a running scan's early pass has written volumes, and
 * only the Library views and the split editions are read again, so a page
 * draws them while the scan runs (O314); that answer is not held for them,
 * as it takes no scan off a page.
 *
 * **A scan's end is drawn once those reads have landed (O321).** The answer
 * that ends a scan is held, and every page draws the answer before it, with
 * the scan, until the set is applied: drawn at once, it would take the scan
 * off a page still drawing what it read before the scan, a "Never" or a "Not
 * read yet" between the scan and what it left. A read that never answers
 * holds it no longer than `SCAN_END_HOLD_MS`, and the next poll is timed from
 * the answer's publication.
 *
 * **A kept answer is marked stale once polling has stayed stopped for
 * `SCAN_POLL_MS` (O451),** until the next answer is published. A polling page
 * asks again that often while a scan runs, so an answer kept longer may show
 * a scan that has since ended. It is still the base the next answer is
 * compared with, so an end is still held for its reads, and what the admin
 * pages draw at once; only words drawn from it may be out of date, so the
 * reader's scan line reads `useScanFeedStale` and keeps its room without them.
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
  libraryViewReads,
  ROOTS,
} from './pageReads'
import { invalidate, readsUnder } from './reads'

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
  /**
   * The volumes the scan's early passes have written so far while it hashes;
   * `null` until the first early pass reports, and always for the
   * fingerprint and match passes, which run none.
   */
  ready: number | null
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
   * Moves each time a running scan's early pass writes volumes; its value
   * means nothing else. Compared only for a change: an answer without it reads as
   * unmoved.
   */
  written?: number
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

/**
 * The longest an answer that ends a scan is held for the reads it sent. The
 * idle poll's interval: a scan that starts and ends between two idle polls is
 * already seen that late, so a read that never answers delays the end by no
 * more than the feed's own slack. Only a read silent this long meets the
 * bound, held by a stalled network, since the Library views' wait for
 * progress saves stops at `OPENING_SAVE_WAIT_MS`; a shorter bound would take
 * the scan off a page before a slow re-read that does answer has landed.
 */
export const SCAN_END_HOLD_MS = FEED_IDLE_MS

const FEED = '/api/library/scans'

const listeners = new Set<() => void>()
/** Told of each scan's end, before the reads it changed are sent again. */
const scanEndListeners = new Set<() => void>()
/**
 * The last answer, kept while nothing draws it: a page that replaces another
 * in one commit draws what the other drew, not nothing until its first poll
 * lands. Kept when `stale` too: it is still the base a return's first answer
 * is compared with, so an end is still held for its reads, and the admin
 * pages' answer; only the words drawn from it may be out of date.
 */
let latest: ScanFeed | undefined
/**
 * Set once polling has stayed stopped for `SCAN_POLL_MS`, and cleared by the
 * next answer published. A polling page asks again every `SCAN_POLL_MS` while
 * a scan runs, so a kept answer older than that may show a scan that has since
 * ended. A gap shorter than that leaves it unset: the commit that swaps one
 * consumer for another, and a library switch's loading or a pending search
 * that answers within it.
 */
let stale = false
/**
 * Sets `stale` when it fires. A subscribe cancels it, which alone keeps the
 * mark off while a page draws the feed.
 */
let staleTimer: ReturnType<typeof setTimeout> | undefined
/**
 * The next poll, or, while an answer that ends a scan is held, the end of
 * its hold: an ask or a stop clears either, as either overtakes the held
 * answer.
 */
let timer: ReturnType<typeof setTimeout> | undefined
/**
 * A poll was dropped for a hidden tab, or a read failed, before any answer
 * landed. The pages drew their reads meanwhile, and a scan may have ended
 * since, which the first answer cannot tell: without this, a page mounted in
 * a hidden tab keeps reads from before the scan's end until it is left.
 */
let missed = false
/**
 * Bumped by every ask and by stopping, so only the newest ask's answer lands,
 * and only an answer no ask has overtaken while it was held is published.
 */
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
      if (latest === undefined ? missed : scanEnded(latest, answer)) {
        publishOnceRead(id, answer)
      } else {
        if (latest !== undefined && worksArrived(latest, answer))
          void invalidateLibraryViews()
        publish(answer)
      }
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
 * Draws `answer` on every page, hands it to each waiting `askScanFeed`, and
 * times the next poll from now.
 */
function publish(answer: ScanFeed): void {
  latest = answer
  // Before the listeners are told, so a view reading the answer and the mark
  // draws one state in one render.
  stale = false
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
}

/**
 * Sends the reads changed by the scan whose end `answer` shows, and
 * publishes `answer` once they are applied or `SCAN_END_HOLD_MS` has passed,
 * whichever is first, unless an ask or a stop has overtaken ask `id` by then:
 * a newer answer is compared with the one still published, so it ends the
 * scan again and is held for reads of its own.
 */
function publishOnceRead(id: number, answer: ScanFeed): void {
  let holding = true
  const release = () => {
    if (!holding || id !== asked) return
    holding = false
    clearTimeout(timer)
    publish(answer)
  }
  void readScanAgain().then(release)
  timer = setTimeout(release, SCAN_END_HOLD_MS)
}

/**
 * Tells each scan-end listener, then sends the reads a scan changes that a
 * page draws as one set, so a page drawn from several of them moves once
 * where no read overtakes one: the library status, the libraries, with each
 * library's own reads under `LIBRARIES`' address, the directories, the
 * Library views and the split editions. Resolves as `invalidate` does, a
 * failed read included, so a read that overtakes one it sent holds the end
 * until it lands too; never rejects.
 */
function readScanAgain(): Promise<void> {
  for (const listener of scanEndListeners) listener()
  return invalidate(
    ...readsUnder(LIBRARY_STATUS.address, LIBRARIES.address, ROOTS.address),
    ...libraryViewReads(),
  )
}

function onVisibilityChange(): void {
  if (document.visibilityState === 'visible') ask()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (listeners.size === 1) {
    // The mark, where it is set, stays until an answer is published.
    clearTimeout(staleTimer)
    staleTimer = undefined
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
  // Cleared first: a second stop with no subscribe between, as a reset after
  // the last unmount, would orphan the first timer, which no subscribe can
  // then cancel.
  clearTimeout(staleTimer)
  staleTimer =
    latest === undefined
      ? undefined
      : setTimeout(() => {
          stale = true
        }, SCAN_POLL_MS)
}

/**
 * The feed's last answer published, `undefined` before the first; drawn
 * again on each one published. While any component draws it, the tab polls.
 */
export function useScanFeed(): ScanFeed | undefined {
  return useSyncExternalStore(subscribe, () => latest)
}

/**
 * Whether the answer `useScanFeed` hands back is marked stale: kept from before
 * polling stopped at least `SCAN_POLL_MS` ago, with no answer since. For a
 * view that draws the kept answer's words, which may then name a scan that
 * has ended.
 */
export function useScanFeedStale(): boolean {
  return useSyncExternalStore(subscribe, () => stale)
}

/**
 * Asks the feed now, for a page that has just started or cancelled a scan,
 * and hands back the answer once it is published, which for one that ends a
 * scan is once the reads it sent have landed or `SCAN_END_HOLD_MS` has
 * passed: `undefined` where it failed, or where nothing was asked because
 * nothing draws the feed or the tab is hidden, so a caller is never held for
 * good. An ask this sent and still out is shared; a poll still out is not,
 * since it may have been sent before the caller's write was answered, and is
 * overtaken.
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

/**
 * Whether a running scan's early pass wrote volumes between `before` and
 * `now`, so the Library views are read again and show them while the scan
 * runs. Asked only of an answer that ends no scan, as one that does reads the
 * views with everything else. An answer that carries the marker after one
 * that did not reads as moved: volumes may have been written between them.
 */
function worksArrived(before: ScanFeed, now: ScanFeed): boolean {
  return now.written !== undefined && now.written !== before.written
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

/** " · 41 volumes ready", or nothing before a pass has written a volume. */
function volumesReady(ready: number | null): string {
  if (ready === null || ready <= 0) return ''
  return ` · ${figure(ready)} ${ready === 1 ? 'volume' : 'volumes'} ready`
}

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
        : `${word} · ${outOf(done, job.of, humanBytes)}${volumesReady(job.ready)}`
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
 * last answer, which was the account's that read it, and its mark.
 */
export function resetScanFeed(): void {
  listeners.clear()
  stop()
  clearTimeout(staleTimer)
  staleTimer = undefined
  stale = false
  latest = undefined
  missed = false
}

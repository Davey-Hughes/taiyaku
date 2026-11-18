import { afterEach, vi } from 'vitest'
import type { FeedJob, ScanFeed } from '../../src/scanFeed'
import { deferred } from './async'

// Put back after each test, so a test that stubbed the feed never hands its
// `fetch` to the next.
afterEach(() => {
  vi.unstubAllGlobals()
})

/**
 * A running job as an administrator is sent it: job 7, reading files in
 * directory 1 of library 7, `libraries.ts`'s Manga, unless `over` names
 * otherwise.
 */
export const feedJob = (over: Partial<FeedJob> = {}): FeedJob => ({
  id: 7,
  kind: 'scan',
  step: 'resolving',
  done: 120,
  of: 2097,
  dir: { at: 1, of: 1 },
  dirs: [{ id: 1, path: '/media/Manga/Volumes' }],
  libraries: [{ id: 7, name: 'Manga' }],
  elapsed_secs: 3.5,
  cancelling: false,
  ...over,
})

/**
 * A fingerprint pass as an administrator is sent it: job 12, a count of files
 * across the store, covering library 7, `libraries.ts`'s Manga, unless
 * `over` names otherwise. It holds no directory, and a feed never holds its
 * libraries.
 */
export const fingerprintPass = (over: Partial<FeedJob> = {}): FeedJob => ({
  id: 12,
  kind: 'fingerprint',
  step: 'fingerprinting',
  done: 120,
  of: 2097,
  dir: null,
  dirs: [],
  libraries: [{ id: 7, name: 'Manga' }],
  elapsed_secs: 12.5,
  cancelling: false,
  ...over,
})

/** What a poll is answered with: a feed, or a status to fail with. */
export type FeedAnswer = ScanFeed | number

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })

/**
 * Stubs `fetch` for `GET /api/library/scans`, answered with `first` until
 * `answer` sets another, and for `POST /api/library/scans/{job}/cancel`: a
 * job the feed holds is marked cancelling and answers 202, one an answer held
 * before answers 200 with how it ended, and any other 404, as the server
 * answers a job it holds no slot for. Every other request goes to the
 * `fetch` in place before, so a test stubs the routes its page reads first.
 */
export function serveFeed(first: FeedAnswer) {
  let now = first
  /** Every job an answer has held, which the server still knows once ended. */
  const known = new Set<number>()
  const know = (answer: FeedAnswer) => {
    if (typeof answer !== 'number')
      for (const job of answer.jobs ?? []) known.add(job.id)
  }
  know(first)
  let polls = 0
  let held: Promise<void> | undefined
  const cancels: number[] = []
  const before = globalThis.fetch
  const answerNow = () =>
    typeof now === 'number'
      ? new Response('refused', { status: now })
      : json(now)
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url === '/api/library/scans') {
        polls += 1
        const wait = held
        held = undefined
        return wait === undefined
          ? Promise.resolve(answerNow())
          : wait.then(answerNow)
      }
      const cancel = /^\/api\/library\/scans\/(\d+)\/cancel$/.exec(url)
      if (cancel !== null && init?.method === 'POST') {
        const job = Number(cancel[1])
        cancels.push(job)
        if (
          typeof now === 'number' ||
          !(now.jobs ?? []).some((one) => one.id === job)
        ) {
          return Promise.resolve(
            known.has(job)
              ? json({ job, ended: 'finished' })
              : json({ error: `there is no scan job ${job}` }, 404),
          )
        }
        now = {
          ...now,
          jobs: (now.jobs ?? []).map((one) =>
            one.id === job ? { ...one, cancelling: true } : one,
          ),
        }
        return Promise.resolve(json({ job, cancelling: true }, 202))
      }
      return before(input, init)
    }),
  )
  return {
    /** Answers every poll from now on with `next`. */
    answer(next: FeedAnswer) {
      know(next)
      now = next
    },
    /**
     * Holds the next poll's answer open until the returned function is
     * called; it then answers with what `answer` last set.
     */
    hold(): () => void {
      const release = deferred<void>()
      held = release.promise
      return () => release.resolve()
    },
    /** How many polls have been sent. */
    polls: () => polls,
    /** The jobs a cancel was sent for, in order. */
    cancels,
  }
}

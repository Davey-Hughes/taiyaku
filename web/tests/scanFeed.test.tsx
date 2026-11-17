import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SCAN_POLL_MS } from '../src/config'
import {
  gridRead,
  LIBRARIES,
  LIBRARY_STATUS,
  libraryScan,
  ROOTS,
  RULES,
} from '../src/pageReads'
import { entryOf, readSet } from '../src/reads'
import {
  askScanFeed,
  cancelScan,
  FEED_IDLE_MS,
  type FeedJob,
  jobHoldingDir,
  jobsHolding,
  libraryStep,
  onScanEnd,
  type ScanFeed,
  type ScanStep,
  stepLine,
  stepWord,
  useScanFeed,
} from '../src/scanFeed'
import { usePage } from '../src/usePage'
import { mount, rerender } from './support/mount'
import { address } from './support/reads'
import { feedJob, serveFeed } from './support/scanFeed'

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
})

afterEach(() => {
  vi.useRealTimers()
  // jsdom's own getter, on the prototype, answers again.
  Reflect.deleteProperty(document, 'visibilityState')
})

/** Lets `ms` of the clock run inside `act`, and the polls sent meanwhile land. */
async function wait(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
}

/** The tab turns `state`, as the browser tells a page. */
async function turn(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', {
    value: state,
    configurable: true,
  })
  await act(async () => {
    document.dispatchEvent(new Event('visibilitychange'))
  })
}

const idle = (ended: number): ScanFeed => ({ ended, libraries: [] })

/** The feed while `job` runs, each library it holds at its step. */
const running = (ended: number, job: FeedJob = feedJob()): ScanFeed => ({
  ended,
  libraries: job.libraries.map(({ id }) => ({ id, step: job.step })),
  jobs: [job],
})

/**
 * The feed's consumers. Made here because `useComponentExportOnlyModules`
 * refuses an unexported component at module scope, and `noExportsInTest`
 * refuses the export.
 */
function consumers() {
  /** Draws the feed's `ended`, or `none` before any answer. */
  function Ended() {
    const feed = useScanFeed()
    return <p className="ended">{feed === undefined ? 'none' : feed.ended}</p>
  }

  function Consumers({ n }: { n: number }) {
    return Array.from({ length: n }, (_, i) => <Ended key={i} />)
  }

  /** A page drawn from the library status, as the sidebar is. */
  function StatusReader() {
    const page = usePage([LIBRARY_STATUS])
    return <p className="status">{page.ready ? 'read' : 'reading'}</p>
  }

  return { Ended, Consumers, StatusReader }
}

const { Ended, Consumers, StatusReader } = consumers()

const drawn = (host: Element) =>
  [...host.querySelectorAll('.ended')].map((p) => p.textContent)

/**
 * Stubs the library status's route, under the feed's stub, and counts its
 * requests.
 */
function serveStatus() {
  const sent = vi.fn((input: RequestInfo | URL) =>
    String(input) === '/api/library/status'
      ? Promise.resolve(
          new Response(JSON.stringify({ libraries: [] }), {
            headers: { 'content-type': 'application/json' },
          }),
        )
      : Promise.reject(new Error(`unexpected ${String(input)}`)),
  )
  vi.stubGlobal('fetch', sent)
  return () => sent.mock.calls.length
}

describe('the scan feed', () => {
  it('one poll serves every consumer', async () => {
    const feed = serveFeed(idle(1))
    const host = mount(<Consumers n={3} />)
    await wait(0)
    const atMount = feed.polls()
    feed.answer(idle(2))
    await wait(FEED_IDLE_MS)
    expect([atMount, feed.polls()]).toEqual([1, 2])
    expect(drawn(host)).toEqual(['2', '2', '2'])
  })

  it('asks every second while a job runs and every five seconds while none does', async () => {
    const feed = serveFeed(running(1))
    mount(<Ended />)
    const polls: number[] = []
    await wait(0)
    polls.push(feed.polls())
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    feed.answer(idle(2))
    // The idle answer lands at this poll, and the next waits five seconds.
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    await wait(FEED_IDLE_MS - SCAN_POLL_MS)
    polls.push(feed.polls())
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    expect(polls).toEqual([1, 2, 3, 3, 4])
  })

  it('sends nothing while the tab is hidden and asks at once when it is shown', async () => {
    const feed = serveFeed(running(1))
    mount(<Ended />)
    await wait(0)
    await turn('hidden')
    const before = feed.polls()
    await wait(20000)
    const whileHidden = feed.polls() - before
    await turn('visible')
    const onShown = feed.polls() - before - whileHidden
    await wait(0)
    expect([whileHidden, onShown]).toEqual([0, 1])
  })

  it('reads the library status again when a job leaves the feed', async () => {
    const statusReads = serveStatus()
    const feed = serveFeed(running(1, feedJob({ id: 7 })))
    mount(
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    await wait(0)
    const reads = [statusReads()]
    // Job 7 ends between the server's reading `ended` and listing the jobs,
    // while job 8 holds the same library: only the job's id tells.
    feed.answer(running(1, feedJob({ id: 8 })))
    await wait(SCAN_POLL_MS)
    reads.push(statusReads())
    await wait(SCAN_POLL_MS)
    reads.push(statusReads())
    expect(reads).toEqual([1, 2, 2])
  })

  it('reads again when ended moves though no job was ever seen', async () => {
    const statusReads = serveStatus()
    const feed = serveFeed(idle(3))
    mount(
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    await wait(0)
    const reads = [statusReads()]
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    reads.push(statusReads())
    await wait(FEED_IDLE_MS)
    reads.push(statusReads())
    expect(reads).toEqual([1, 2, 2])
  })

  it('reads again when a library leaves the feed before ended moves', async () => {
    const statusReads = serveStatus()
    // A reader's form: libraries and their steps, no jobs.
    const feed = serveFeed({
      ended: 3,
      libraries: [{ id: 7, step: 'hashing' }],
    })
    mount(
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    await wait(0)
    const reads = [statusReads()]
    feed.answer(idle(3))
    await wait(SCAN_POLL_MS)
    reads.push(statusReads())
    await wait(FEED_IDLE_MS)
    reads.push(statusReads())
    expect(reads).toEqual([1, 2, 2])
  })

  it('reads again on the first answer of a tab that mounted hidden, as a job may have ended unseen', async () => {
    const statusReads = serveStatus()
    const feed = serveFeed(running(1))
    await turn('hidden')
    mount(
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    await wait(0)
    const atMount = [statusReads(), feed.polls()]
    feed.answer(idle(2))
    await wait(60000)
    await turn('visible')
    await wait(0)
    expect([atMount, statusReads(), feed.polls()]).toEqual([[1, 0], 2, 1])
  })

  it('reads again on the first answer after a failed first read', async () => {
    const statusReads = serveStatus()
    const feed = serveFeed(500)
    mount(
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    await wait(0)
    const afterFailure = statusReads()
    feed.answer(idle(2))
    await wait(FEED_IDLE_MS)
    const afterAnswer = statusReads()
    await wait(FEED_IDLE_MS)
    expect([afterFailure, afterAnswer, statusReads()]).toEqual([1, 2, 2])
  })

  it('marks every library, directory and Library view read stale when a scan ends, and no other', async () => {
    const kept = [
      LIBRARY_STATUS,
      LIBRARIES,
      libraryScan(7),
      ROOTS,
      gridRead(7),
      RULES,
    ].map((one) => address<object>(one.address))
    const reading = readSet(kept.map((one) => one.read))
    for (const one of kept) one.requests[0].resolve({})
    await reading
    const feed = serveFeed(idle(3))
    mount(<Ended />)
    await wait(0)
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    expect(kept.map((one) => entryOf(one.read)?.stale)).toEqual([
      true,
      true,
      true,
      true,
      true,
      false,
    ])
  })

  it('tells each scan-end listener before it reads again, until stopped', async () => {
    const kept = address<object>(LIBRARIES.address)
    const reading = readSet([kept.read])
    kept.requests[0].resolve({})
    await reading
    const feed = serveFeed(idle(3))
    const stale: (boolean | undefined)[] = []
    const stop = onScanEnd(() => stale.push(entryOf(kept.read)?.stale))
    mount(<Ended />)
    await wait(0)
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    stop()
    feed.answer(idle(5))
    await wait(FEED_IDLE_MS)
    expect(stale).toEqual([false])
  })

  it('stops asking once the last consumer unmounts', async () => {
    const feed = serveFeed(idle(1))
    mount(<Consumers n={2} />)
    await wait(0)
    rerender(<Consumers n={1} />)
    await wait(FEED_IDLE_MS)
    const withOne = feed.polls()
    rerender(<Consumers n={0} />)
    await wait(4 * FEED_IDLE_MS)
    expect([withOne, feed.polls()]).toEqual([2, 2])
  })

  it('asks nothing when the tab is shown again after the last consumer has gone', async () => {
    const feed = serveFeed(idle(1))
    mount(<Consumers n={1} />)
    await wait(0)
    rerender(<Consumers n={0} />)
    await turn('hidden')
    await turn('visible')
    await wait(4 * FEED_IDLE_MS)
    expect(feed.polls()).toBe(1)
  })

  it('a hide and show inside one interval asks once, and the next poll waits a whole interval from then', async () => {
    const feed = serveFeed(idle(1))
    mount(<Ended />)
    await wait(0)
    await wait(SCAN_POLL_MS)
    await turn('hidden')
    await turn('visible')
    await wait(0)
    const onShown = feed.polls()
    // Where the poll due before the hide was left, it would be sent here.
    await wait(FEED_IDLE_MS - SCAN_POLL_MS)
    const atOldDue = feed.polls()
    await wait(SCAN_POLL_MS)
    expect([onShown, atOldDue, feed.polls()]).toEqual([2, 2, 3])
  })

  it.each([
    ['an answer', idle(2)],
    ['a failure', 500],
  ])(
    'starts no poll from %s that lands after the last consumer has gone',
    async (_, last) => {
      const feed = serveFeed(idle(1))
      mount(<Consumers n={1} />)
      await wait(0)
      feed.answer(last)
      const release = feed.hold()
      await wait(FEED_IDLE_MS)
      rerender(<Consumers n={0} />)
      release()
      await wait(4 * FEED_IDLE_MS)
      expect(feed.polls()).toBe(2)
    },
  )

  it('draws the last answer for a consumer that mounts after the last has gone, while it asks again', async () => {
    const feed = serveFeed(idle(1))
    const host = mount(<Consumers n={1} />)
    await wait(0)
    rerender(<Consumers n={0} />)
    feed.answer(idle(2))
    const release = feed.hold()
    rerender(<Consumers n={1} />)
    const atMount = drawn(host)
    release()
    await wait(0)
    expect([atMount, drawn(host), feed.polls()]).toEqual([['1'], ['2'], 2])
  })

  it('keeps the last answer through a failed read and asks again five seconds on', async () => {
    const feed = serveFeed(running(1))
    const host = mount(<Ended />)
    await wait(0)
    feed.answer(500)
    await wait(SCAN_POLL_MS)
    const failed = [feed.polls(), drawn(host)]
    feed.answer(running(2))
    await wait(FEED_IDLE_MS - SCAN_POLL_MS)
    const notYet = feed.polls()
    await wait(SCAN_POLL_MS)
    expect([failed, notYet, feed.polls(), drawn(host)]).toEqual([
      [2, ['1']],
      2,
      3,
      ['2'],
    ])
  })
})

describe('askScanFeed', () => {
  it('asks at once and hands back the answer once it lands', async () => {
    const feed = serveFeed(idle(1))
    const host = mount(<Ended />)
    await wait(0)
    feed.answer(running(2))
    let answer: ScanFeed | undefined
    await act(async () => {
      const asked = askScanFeed()
      expect(feed.polls()).toBe(2)
      answer = await asked
    })
    expect(answer).toEqual(running(2))
    expect(drawn(host)).toEqual(['2'])
  })

  it('shares an ask it sent, and overtakes a poll that was out before it', async () => {
    const feed = serveFeed(idle(1))
    mount(<Ended />)
    await wait(0)
    // The poll out now may have left before the caller's write was answered.
    const release = feed.hold()
    await wait(FEED_IDLE_MS)
    feed.answer(running(2))
    const answers: (ScanFeed | undefined)[] = []
    await act(async () => {
      const both = Promise.all([askScanFeed(), askScanFeed()])
      release()
      answers.push(...(await both))
    })
    expect(feed.polls()).toBe(3)
    expect(answers).toEqual([running(2), running(2)])
  })

  it('hands back nothing for a failed read, and asks nothing while nothing draws the feed', async () => {
    const feed = serveFeed(500)
    expect(await askScanFeed()).toBeUndefined()
    expect(feed.polls()).toBe(0)
    mount(<Ended />)
    await wait(0)
    let answer: ScanFeed | string | undefined = 'still out'
    void askScanFeed().then((landed) => {
      answer = landed
    })
    await wait(0)
    expect([feed.polls(), answer]).toEqual([2, undefined])
  })
})

describe('cancelScan', () => {
  it('posts to the job’s cancel address and hands back the answer', async () => {
    const feed = serveFeed(running(1, feedJob({ id: 7 })))
    await expect(cancelScan(7)).resolves.toEqual({ job: 7, cancelling: true })
    expect(feed.cancels).toEqual([7])
  })
})

describe('the jobs a page shows', () => {
  const volumes = feedJob({ id: 7 })
  const shared = feedJob({
    id: 8,
    step: 'hashing',
    dirs: [{ id: 2, path: '/media/Manga/Chapters' }],
    libraries: [
      { id: 7, name: 'Manga' },
      { id: 9, name: 'Comics' },
    ],
  })
  const admin: ScanFeed = {
    ended: 0,
    libraries: [
      { id: 7, step: 'resolving' },
      { id: 9, step: 'hashing' },
    ],
    jobs: [volumes, shared],
  }

  it('finds each job holding a library', () => {
    expect(
      [7, 9, 3].map((library) =>
        jobsHolding(admin, library).map((job) => job.id),
      ),
    ).toEqual([[7, 8], [8], []])
  })

  it('finds the job holding a directory', () => {
    expect([1, 2, 5].map((dir) => jobHoldingDir(admin, dir)?.id)).toEqual([
      7,
      8,
      undefined,
    ])
  })

  it('reads a library’s step from the libraries, which a reader is sent too', () => {
    const reader: ScanFeed = {
      ended: 0,
      libraries: [{ id: 9, step: 'hashing' }],
    }
    expect([libraryStep(reader, 9), libraryStep(reader, 7)]).toEqual([
      'hashing',
      undefined,
    ])
    expect(jobsHolding(reader, 9)).toEqual([])
  })

  it('finds nothing before the first answer', () => {
    expect([
      jobsHolding(undefined, 7),
      jobHoldingDir(undefined, 1),
      libraryStep(undefined, 7),
    ]).toEqual([[], undefined, undefined])
  })
})

describe('stepLine', () => {
  it('words each step with its count, and the directory while listing and reading files', () => {
    expect(
      [
        feedJob({ step: 'waiting', done: null, of: null, dir: null }),
        feedJob({
          step: 'listing',
          done: 340,
          of: null,
          dir: { at: 2, of: 5 },
        }),
        feedJob({ step: 'listing', done: 1, of: null, dir: { at: 1, of: 5 } }),
        feedJob({ step: 'listing', done: 0, of: null, dir: null }),
        feedJob({
          step: 'resolving',
          done: 120,
          of: 2097,
          dir: { at: 2, of: 5 },
        }),
        feedJob({
          step: 'resolving',
          done: 120,
          of: null,
          dir: { at: 2, of: 5 },
        }),
        feedJob({ step: 'hashing', done: 1048576, of: 5368709120, dir: null }),
        feedJob({ step: 'hashing', done: 0, of: null, dir: null }),
        feedJob({ step: 'writing', done: 40, of: 138, dir: null }),
        feedJob({ step: 'finishing', done: null, of: null, dir: null }),
      ].map(stepLine),
    ).toEqual([
      'Waiting for another job',
      'Listing · 340 files found · directory 2 of 5',
      'Listing · 1 file found · directory 1 of 5',
      'Listing · 0 files found',
      'Reading files · 120 of 2,097 · directory 2 of 5',
      'Reading files · 120 · directory 2 of 5',
      'Hashing · 1.0 MiB of 5.0 GiB',
      'Hashing · sizing files',
      'Writing works · 40 of 138',
      'Finishing',
    ])
  })

  it('names each step by its word alone', () => {
    const steps: ScanStep[] = [
      'waiting',
      'listing',
      'resolving',
      'hashing',
      'writing',
      'finishing',
    ]
    expect(steps.map(stepWord)).toEqual([
      'Waiting for another job',
      'Listing',
      'Reading files',
      'Hashing',
      'Writing works',
      'Finishing',
    ])
  })
})

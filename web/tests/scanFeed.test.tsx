import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SCAN_POLL_MS } from '../src/config'
import type { LibraryStatus } from '../src/library'
import {
  gridRead,
  LIBRARIES,
  LIBRARY_STATUS,
  libraryScan,
  ROOTS,
  RULES,
  SPLIT_EDITIONS,
} from '../src/pageReads'
import { entryOf, invalidate, readSet } from '../src/reads'
import {
  askScanFeed,
  cancelScan,
  FEED_IDLE_MS,
  type FeedJob,
  fingerprintJob,
  jobHoldingDir,
  jobsHolding,
  libraryStep,
  matchJob,
  onScanEnd,
  type ScanFeed,
  type ScanStep,
  scanJobs,
  stepLine,
  stepWord,
  useScanFeed,
} from '../src/scanFeed'
import { usePage } from '../src/usePage'
import { deferred } from './support/async'
import { mount, rerender } from './support/mount'
import { address } from './support/reads'
import {
  type FeedAnswer,
  feedJob,
  fingerprintPass,
  matchPass,
  serveFeed,
} from './support/scanFeed'
import { holdTheClock, watchDrawn } from './support/wait'

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

  /**
   * A page drawn from the library status and library 7's grid, as a
   * library's page is: whether its first library has been scanned, and how
   * many series the grid holds.
   */
  function StatusAndGrid() {
    const page = usePage([LIBRARY_STATUS, gridRead(7)])
    const [status, grid] = page.answers
    return (
      <>
        <p className="scanned">{String(status?.libraries[0]?.scanned)}</p>
        <p className="series">{String(grid?.series.length)}</p>
      </>
    )
  }

  /** Draws the step of the feed's first scan job, or `none`. */
  function Step() {
    const job = scanJobs(useScanFeed())[0]
    return <p className="step">{job === undefined ? 'none' : job.step}</p>
  }

  return { Ended, Consumers, StatusReader, StatusAndGrid, Step }
}

const { Ended, Consumers, StatusReader, StatusAndGrid, Step } = consumers()

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

const STATUS = LIBRARY_STATUS.address
const GRID = gridRead(7).address

/** Library 7, Manga, as the status lists it, scanned or not yet. */
const mangaStatus = (scanned: boolean): LibraryStatus => ({
  libraries: [{ id: 7, name: 'Manga', scanned }],
  restricted: false,
})

/** Library 7's grid holding `count` series, with ids unlike their places. */
const gridOf = (count: number) => ({
  series: Array.from({ length: count }, (_, i) => ({ id: 31 + i })),
  unfiled: [],
})

/**
 * Stubs each of `paths`, under the feed's stub, with every request held open
 * until the test answers it: `answer` and `fail` settle the route's `n`th
 * request, counting from 0, and `sent` counts its requests.
 */
function holdRoutes(...paths: string[]) {
  const requests = new Map(
    paths.map((path) => [path, [] as ReturnType<typeof deferred<Response>>[]]),
  )
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL) => {
      const held = requests.get(String(input))
      if (held === undefined) {
        return Promise.reject(new Error(`unexpected ${String(input)}`))
      }
      const one = deferred<Response>()
      held.push(one)
      return one.promise
    }),
  )
  const request = (path: string, n: number) => {
    const one = requests.get(path)?.[n]
    if (one === undefined) throw new Error(`no request ${n} to ${path}`)
    return one
  }
  return {
    answer(path: string, n: number, body: unknown) {
      request(path, n).resolve(
        new Response(JSON.stringify(body), {
          headers: { 'content-type': 'application/json' },
        }),
      )
    },
    fail(path: string, n: number, status: number) {
      request(path, n).resolve(new Response('refused', { status }))
    },
    sent: (path: string) => requests.get(path)?.length ?? 0,
  }
}

/**
 * Mounts `ui` under a feed answering `first`, with every request to the
 * library status held, and lands the feed's first answer and the status's
 * first read, Manga not yet scanned.
 */
async function mountWithStatus(first: ScanFeed, ui: React.ReactNode) {
  const routes = holdRoutes(STATUS)
  const feed = serveFeed(first)
  const host = mount(ui)
  await wait(0)
  routes.answer(STATUS, 0, mangaStatus(false))
  await wait(0)
  return { routes, feed, host }
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

  it('asks every second while only a fingerprint pass runs, though it holds no library, so its count moves as it reads', async () => {
    const feed = serveFeed({
      ended: 1,
      libraries: [],
      jobs: [fingerprintPass()],
    })
    mount(<Ended />)
    const polls: number[] = []
    await wait(0)
    polls.push(feed.polls())
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    feed.answer(idle(1))
    // The idle answer lands at this poll, and the next waits `FEED_IDLE_MS`.
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    await wait(FEED_IDLE_MS - SCAN_POLL_MS)
    polls.push(feed.polls())
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    expect(polls).toEqual([1, 2, 3, 3, 4])
  })

  it('asks every second while only the match pass runs, though it holds no library, so its count moves as it matches', async () => {
    const feed = serveFeed({ ended: 1, libraries: [], jobs: [matchPass()] })
    mount(<Ended />)
    const polls: number[] = []
    await wait(0)
    polls.push(feed.polls())
    await wait(SCAN_POLL_MS)
    polls.push(feed.polls())
    feed.answer(idle(1))
    // The idle answer lands at this poll, and the next waits `FEED_IDLE_MS`.
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

  it.each<{
    missed: string
    first: FeedAnswer
    hidden: boolean
    reach: () => Promise<void>
  }>([
    {
      missed: 'a tab that mounted hidden',
      first: running(1),
      hidden: true,
      reach: () => turn('visible'),
    },
    {
      missed: 'a failed first read',
      first: 500,
      hidden: false,
      reach: () => wait(FEED_IDLE_MS),
    },
  ])(
    'draws the first answer after $missed only once the status read it sent again has landed',
    async ({ first, hidden, reach }) => {
      const routes = holdRoutes(STATUS)
      const feed = serveFeed(first)
      if (hidden) await turn('hidden')
      const host = mount(
        <>
          <StatusReader />
          <Ended />
        </>,
      )
      await wait(0)
      routes.answer(STATUS, 0, mangaStatus(false))
      await wait(0)
      feed.answer(idle(2))
      await reach()
      await wait(0)
      const whileOut = [drawn(host), routes.sent(STATUS)]
      routes.answer(STATUS, 1, mangaStatus(true))
      await wait(0)
      expect([whileOut, drawn(host)]).toEqual([[['none'], 2], ['2']])
    },
  )

  it('marks every library, directory and Library view read stale when a scan ends, and no other', async () => {
    const kept = [
      LIBRARY_STATUS,
      LIBRARIES,
      libraryScan(7),
      ROOTS,
      gridRead(7),
      SPLIT_EDITIONS,
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
      true,
      false,
    ])
  })

  it('publishes the answer that ended a scan only once the reads it sent have landed', async () => {
    const { routes, feed, host } = await mountWithStatus(
      idle(3),
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    const whileOut = [drawn(host), routes.sent(STATUS)]
    routes.answer(STATUS, 1, mangaStatus(true))
    await wait(0)
    expect([whileOut, drawn(host)]).toEqual([[['3'], 2], ['4']])
  })

  it.each([
    ['the status', 'the grid'],
    ['the grid', 'the status'],
  ] as const)(
    "sends a scan's re-reads as one set: a page drawn from the status and the grid moves once, %s answering first",
    async (first, second) => {
      const routes = holdRoutes(STATUS, GRID)
      const feed = serveFeed(idle(3))
      const host = mount(
        <>
          <StatusAndGrid />
          <Ended />
        </>,
      )
      await wait(0)
      routes.answer(STATUS, 0, mangaStatus(false))
      routes.answer(GRID, 0, gridOf(0))
      await wait(0)
      const states = watchDrawn(() =>
        [...host.querySelectorAll('.scanned, .series')]
          .map((p) => p.textContent)
          .join(' '),
      )
      feed.answer(idle(4))
      await wait(FEED_IDLE_MS)
      const after = { 'the status': mangaStatus(true), 'the grid': gridOf(3) }
      const path = { 'the status': STATUS, 'the grid': GRID }
      for (const one of [first, second]) {
        routes.answer(path[one], 1, after[one])
        await wait(0)
      }
      states.stop()
      expect([...states.seen]).toEqual(['false 0', 'true 3'])
    },
  )

  it('draws a scan’s end only once a read of the status sent while the end is held has landed, never over the status from before the scan', async () => {
    const routes = holdRoutes(STATUS, GRID)
    const feed = serveFeed(idle(3))
    const host = mount(
      <>
        <StatusAndGrid />
        <Ended />
      </>,
    )
    await wait(0)
    routes.answer(STATUS, 0, mangaStatus(false))
    routes.answer(GRID, 0, gridOf(0))
    await wait(0)
    const states = watchDrawn(() =>
      [...host.querySelectorAll('.scanned, .series, .ended')]
        .map((p) => p.textContent)
        .join(' '),
    )
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    void invalidate(LIBRARY_STATUS)
    routes.answer(STATUS, 1, mangaStatus(true))
    routes.answer(GRID, 1, gridOf(3))
    await wait(0)
    routes.answer(STATUS, 2, mangaStatus(true))
    await wait(0)
    states.stop()
    // The set lands without the status read the newer one overtook, so the
    // grid moves first; the end waits for the newer read.
    expect([...states.seen]).toEqual([
      'false 0 3',
      'false 3 3',
      'true 3 3',
      'true 3 4',
    ])
  })

  it('publishes at the cap when a re-read never answers, and polls on', async () => {
    // Only the test moves the clock, so 1 ms before the cap is 1 ms before it.
    holdTheClock()
    const { feed, host } = await mountWithStatus(
      idle(3),
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    const landed = feed.polls()
    // The pinned cap, not the constant, so that a longer one fails.
    await wait(4999)
    const before = [drawn(host), feed.polls()]
    await wait(1)
    const at = drawn(host)
    await wait(FEED_IDLE_MS)
    expect([landed, before, at, feed.polls()]).toEqual([
      2,
      [['3'], 2],
      ['4'],
      3,
    ])
  })

  it('times the next poll from the cap, though the re-read lands after it', async () => {
    // Only the test moves the clock, so a poll due at a time is sent at it.
    holdTheClock()
    const { routes, feed, host } = await mountWithStatus(
      idle(3),
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    await wait(5000)
    const atCap = [drawn(host), feed.polls()]
    // The re-read lands 2000 ms after the cap published: the poll that
    // publication timed stays due one idle interval after it.
    await wait(2000)
    routes.answer(STATUS, 1, mangaStatus(true))
    await wait(FEED_IDLE_MS - 2000)
    expect([atCap, feed.polls()]).toEqual([[['4'], 2], 3])
  })

  it('publishes at once when a re-read fails', async () => {
    const { routes, feed, host } = await mountWithStatus(
      idle(3),
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    const whileOut = drawn(host)
    routes.fail(STATUS, 1, 500)
    await wait(SCAN_POLL_MS)
    expect([whileOut, drawn(host)]).toEqual([['3'], ['4']])
  })

  it('publishes an answer that ends no scan as it lands, while a read is out', async () => {
    const routes = holdRoutes(STATUS, GRID)
    const feed = serveFeed(running(3, feedJob({ id: 7, step: 'resolving' })))
    const host = mount(
      <>
        <StatusAndGrid />
        <Step />
      </>,
    )
    await wait(0)
    routes.answer(STATUS, 0, mangaStatus(false))
    routes.answer(GRID, 0, gridOf(0))
    await wait(0)
    // A re-read of a drawn view is out, as one sent for a step's writes
    // would be; an answer that ends no scan does not wait for it.
    void invalidate(gridRead(7))
    feed.answer(running(3, feedJob({ id: 7, step: 'hashing' })))
    const step = () => host.querySelector('.step')?.textContent
    const before = step()
    await wait(SCAN_POLL_MS)
    expect([before, step(), routes.sent(STATUS), routes.sent(GRID)]).toEqual([
      'resolving',
      'hashing',
      1,
      2,
    ])
  })

  it('an ask sent while an answer is held overtakes it', async () => {
    const { routes, feed, host } = await mountWithStatus(
      idle(1),
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    const ended = watchDrawn(() => drawn(host).join(' '))
    feed.answer(idle(2))
    await wait(FEED_IDLE_MS)
    // Ended 2 is held for the status read it sent; the ask sent now is held
    // too, so the first's read lands while the second is out.
    feed.answer(idle(3))
    const release = feed.hold()
    void askScanFeed()
    routes.answer(STATUS, 1, mangaStatus(true))
    await wait(0)
    release()
    await wait(0)
    routes.answer(STATUS, 2, mangaStatus(true))
    await wait(0)
    ended.stop()
    expect([...ended.seen]).toEqual(['1', '3'])
  })

  it('an askScanFeed made while its own ask is held settles at publication', async () => {
    const { routes, feed } = await mountWithStatus(
      idle(3),
      <>
        <StatusReader />
        <Ended />
      </>,
    )
    feed.answer(idle(4))
    const answers: (ScanFeed | undefined)[] = []
    void askScanFeed().then((answer) => answers.push(answer))
    await wait(0)
    const whileOut = [...answers]
    // Made while the held ask is its own, so it joins it.
    void askScanFeed().then((answer) => answers.push(answer))
    await wait(0)
    routes.answer(STATUS, 1, mangaStatus(true))
    await wait(0)
    expect([whileOut, answers, feed.polls()]).toEqual([
      [],
      [idle(4), idle(4)],
      2,
    ])
  })

  it('stopping while an answer is held publishes nothing and times no poll', async () => {
    const { routes, feed, host } = await mountWithStatus(
      idle(3),
      <>
        <StatusReader />
        <Consumers n={1} />
      </>,
    )
    feed.answer(idle(4))
    await wait(FEED_IDLE_MS)
    rerender(
      <>
        <StatusReader />
        <Consumers n={0} />
      </>,
    )
    routes.answer(STATUS, 1, mangaStatus(true))
    await wait(2 * FEED_IDLE_MS)
    const polls = feed.polls()
    // A consumer mounted now draws the answer last published while its own
    // ask is out.
    feed.hold()
    rerender(
      <>
        <StatusReader />
        <Consumers n={1} />
      </>,
    )
    expect([polls, drawn(host)]).toEqual([2, ['3']])
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

  it('reads nothing again and tells no scan-end listener when a fingerprint pass leaves the feed, and does both when a scan leaves', async () => {
    const kept = address<object>(LIBRARIES.address)
    const reading = readSet([kept.read])
    kept.requests[0].resolve({})
    await reading
    // A pass is never in the libraries the feed holds, and its end leaves
    // `ended` where it was.
    const feed = serveFeed({
      ended: 3,
      libraries: [],
      jobs: [fingerprintPass()],
    })
    const told: (boolean | undefined)[] = []
    const stop = onScanEnd(() => told.push(entryOf(kept.read)?.stale))
    mount(<Ended />)
    await wait(0)
    feed.answer(idle(3))
    await wait(FEED_IDLE_MS)
    const passEnded = { told: [...told], stale: entryOf(kept.read)?.stale }
    // Scan 7 ends while scan 8 holds the same library, `ended` not yet
    // moved: only the scan's id tells.
    feed.answer(running(3, feedJob({ id: 7 })))
    await wait(FEED_IDLE_MS)
    feed.answer(running(3, feedJob({ id: 8 })))
    await wait(SCAN_POLL_MS)
    const scanEnded = { told: [...told], stale: entryOf(kept.read)?.stale }
    stop()
    expect({ passEnded, scanEnded }).toEqual({
      passEnded: { told: [], stale: false },
      scanEnded: { told: [false], stale: true },
    })
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

  it('finds no job holding a library that only a fingerprint pass covers', () => {
    const passing: ScanFeed = {
      ended: 0,
      libraries: [],
      jobs: [fingerprintPass({ libraries: [{ id: 1, name: 'Manga' }] })],
    }
    expect(jobsHolding(passing, 1)).toEqual([])
  })

  it('tells the scans from the fingerprint pass, which it finds for a library only while the pass covers it', () => {
    // The scan comes first and covers library 1 too, so neither the order
    // nor the library alone finds the pass.
    const scan = feedJob({ id: 7, libraries: [{ id: 1, name: 'Manga' }] })
    const pass = fingerprintPass({ libraries: [{ id: 1, name: 'Manga' }] })
    const both: ScanFeed = {
      ended: 0,
      libraries: [{ id: 1, step: 'resolving' }],
      jobs: [scan, pass],
    }
    expect([
      scanJobs(both).map((job) => job.id),
      jobsHolding(both, 1).map((job) => job.id),
      fingerprintJob(both)?.id,
      fingerprintJob(both, 1)?.id,
      fingerprintJob(both, 2),
      fingerprintJob({ ...both, jobs: [scan] }),
    ]).toEqual([[7], [7], 12, 12, undefined, undefined])
  })

  it('tells the match pass from the scans and the fingerprint pass, and finds it for a library only while the pass covers it', () => {
    // The fingerprint pass comes first and covers library 2 as well, so
    // neither the kind alone nor the library alone finds the match pass.
    const scan = feedJob({ id: 7, libraries: [{ id: 1, name: 'Manga' }] })
    const fingerprinting = fingerprintPass({
      id: 13,
      libraries: [
        { id: 1, name: 'Manga' },
        { id: 2, name: 'Comics' },
      ],
    })
    const all: ScanFeed = {
      ended: 0,
      libraries: [{ id: 1, step: 'resolving' }],
      jobs: [scan, fingerprinting, matchPass()],
    }
    expect([
      scanJobs(all).map((job) => job.id),
      jobsHolding(all, 1).map((job) => job.id),
      matchJob(all)?.id,
      matchJob(all, 1)?.id,
      matchJob(all, 2),
      fingerprintJob(all, 2)?.id,
      matchJob({ ...all, jobs: [scan, fingerprinting] }),
      matchJob(undefined),
    ]).toEqual([[7], [7], 12, 12, undefined, 13, undefined, undefined])
  })

  it('finds nothing before the first answer', () => {
    expect([
      jobsHolding(undefined, 7),
      jobHoldingDir(undefined, 1),
      libraryStep(undefined, 7),
      scanJobs(undefined),
      fingerprintJob(undefined),
    ]).toEqual([[], undefined, undefined, [], undefined])
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

  it('words a fingerprint pass’s count in files, the noun agreeing with the figure before it', () => {
    expect(
      [
        fingerprintPass({ done: 120, of: 2097 }),
        fingerprintPass({ done: 1, of: 1 }),
        fingerprintPass({ done: 0, of: 1 }),
        fingerprintPass({ done: 120, of: null }),
        fingerprintPass({ done: 1, of: null }),
      ].map(stepLine),
    ).toEqual([
      'Fingerprinting pages · 120 of 2,097 files',
      'Fingerprinting pages · 1 of 1 file',
      'Fingerprinting pages · 0 of 1 file',
      'Fingerprinting pages · 120 files',
      'Fingerprinting pages · 1 file',
    ])
  })

  it('words the match pass’s count as it stands, with no noun after it', () => {
    expect([matchPass(), matchPass({ of: null })].map(stepLine)).toEqual([
      'Matching volumes · 3 of 12',
      'Matching volumes · 3',
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
      'fingerprinting',
      'matching',
    ]
    expect(steps.map(stepWord)).toEqual([
      'Waiting for another job',
      'Listing',
      'Reading files',
      'Hashing',
      'Writing works',
      'Finishing',
      'Fingerprinting pages',
      'Matching volumes',
    ])
  })
})

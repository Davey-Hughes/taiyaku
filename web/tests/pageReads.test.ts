/**
 * The Library view's reads: the address each names, the client call it
 * makes through the module a test mocks, its wait for the progress saves,
 * and the prefixes that cover them.
 */
import { afterEach, describe, expect, it, type Mock, vi } from 'vitest'
import { progressSaved, putProgress } from '../src/api'
import * as library from '../src/library'
import {
  gridRead,
  homeRead,
  invalidateLibraryViews,
  LIBRARY_VIEWS,
  searchRead,
  seriesRead,
} from '../src/pageReads'
import {
  emptyReads,
  entryOf,
  type Read,
  readSet,
  subscribe,
} from '../src/reads'
import { deferred } from './support/async'
import { address, keptSplitEditions } from './support/reads'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  grid: vi.fn(),
  home: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
}))

const subscriptions: (() => void)[] = []
/**
 * The progress saves a test holds: answered after it, so one a failing test
 * left out is not one the next test's reads wait for.
 */
const held: ReturnType<typeof deferred<Response>>[] = []

afterEach(async () => {
  for (const off of subscriptions.splice(0)) off()
  for (const save of held.splice(0)) {
    save.resolve(new Response(null, { status: 204 }))
  }
  await progressSaved()
  emptyReads()
  vi.unstubAllGlobals()
  vi.resetAllMocks()
})

/** Lets every request already settled run its handlers. */
const landed = () => new Promise((resolve) => setTimeout(resolve, 0))

/** The client functions the four reads call, mocked above. */
const clients = {
  grid: library.grid,
  home: library.home,
  oneSeries: library.oneSeries,
  search: library.search,
}

type Client = keyof typeof clients

/** The mocked client function, untyped: the table mixes four signatures. */
const mocked = (client: Client) =>
  clients[client] as unknown as Mock<(...args: unknown[]) => Promise<unknown>>

/** Each factory, the client function it calls, and that call's arguments. */
const factories: readonly {
  name: string
  read: () => Read<unknown>
  address: string
  client: Client
  args: readonly unknown[]
}[] = [
  {
    name: 'gridRead(2)',
    read: () => gridRead(2),
    address: '/api/library/series?library=2',
    client: 'grid',
    args: [2],
  },
  {
    name: 'gridRead(undefined)',
    read: () => gridRead(undefined),
    address: '/api/library/series',
    client: 'grid',
    args: [undefined],
  },
  {
    name: 'homeRead(2)',
    read: () => homeRead(2),
    address: '/api/library/home?library=2',
    client: 'home',
    args: [2],
  },
  {
    name: 'homeRead(undefined)',
    read: () => homeRead(undefined),
    address: '/api/library/home',
    client: 'home',
    args: [undefined],
  },
  {
    name: 'seriesRead(17, 2)',
    read: () => seriesRead(17, 2),
    address: '/api/library/series/17?library=2',
    client: 'oneSeries',
    args: [17, 2],
  },
  {
    name: 'seriesRead(17)',
    read: () => seriesRead(17),
    address: '/api/library/series/17',
    client: 'oneSeries',
    args: [17, undefined],
  },
  {
    name: "searchRead('a b&c', 2)",
    read: () => searchRead('a b&c', 2),
    address: '/api/library/search?q=a%20b%26c&library=2',
    client: 'search',
    args: ['a b&c', 2],
  },
  {
    name: "searchRead('a', undefined)",
    read: () => searchRead('a', undefined),
    address: '/api/library/search?q=a',
    client: 'search',
    args: ['a', undefined],
  },
]

describe('the Library view’s reads', () => {
  it.each(factories)(
    '$name names the address its client function sends',
    async (f) => {
      const actual =
        await vi.importActual<typeof import('../src/library')>('../src/library')
      const sent: string[] = []
      vi.stubGlobal(
        'fetch',
        vi.fn((url: string) => {
          sent.push(url)
          return Promise.resolve(new Response('{}', { status: 200 }))
        }),
      )
      const real = {
        grid: actual.grid,
        home: actual.home,
        oneSeries: actual.oneSeries,
        search: actual.search,
      }[f.client] as (...args: unknown[]) => Promise<unknown>
      await real(...f.args)
      expect(sent).toEqual([f.address])
      expect(f.read().address).toBe(f.address)
    },
  )

  it.each(factories)(
    '$name calls its client function only once a progress save sent before it has settled',
    async (f) => {
      vi.stubGlobal(
        'fetch',
        vi.fn(() => {
          const answer = deferred<Response>()
          held.push(answer)
          return answer.promise
        }),
      )
      const sent = putProgress('w', { edition: 'raw', slot: 1 })
      const answer = { from: f.name }
      mocked(f.client).mockResolvedValue(answer)

      const got = f.read().fetch()
      await landed()
      expect(mocked(f.client)).not.toHaveBeenCalled()

      held[0].resolve(new Response(null, { status: 204 }))
      await sent
      await expect(got).resolves.toBe(answer)
      expect(mocked(f.client)).toHaveBeenCalledOnce()
      expect(mocked(f.client)).toHaveBeenCalledWith(...f.args)
    },
  )

  it('keeps a series opened from a library apart from the same series of every library', async () => {
    vi.mocked(library.oneSeries).mockImplementation((_id, narrowed) =>
      Promise.resolve({ from: narrowed ?? 'every' } as unknown as Awaited<
        ReturnType<typeof library.oneSeries>
      >),
    )
    await readSet([seriesRead(17, 2), seriesRead(17)])
    expect(entryOf(seriesRead(17, 2))?.answer).toEqual({ from: 2 })
    expect(entryOf(seriesRead(17))?.answer).toEqual({ from: 'every' })
  })

  it('names the three views’ prefixes', () => {
    expect(LIBRARY_VIEWS).toEqual([
      '/api/library/series',
      '/api/library/home',
      '/api/library/search',
    ])
  })

  it('re-reads every drawn factory address on invalidateLibraryViews, and no other address', async () => {
    for (const f of factories) {
      mocked(f.client).mockImplementation((...args) =>
        Promise.resolve({ args }),
      )
    }
    const others = ['/api/library/status', '/api/library/libraries'].map(
      (path) => address<string>(path),
    )
    const reads = [
      ...factories.map((f) => f.read()),
      ...others.map((o) => o.read),
    ]
    for (const r of reads) {
      subscriptions.push(subscribe(r.address, () => undefined))
    }
    const first = readSet(reads)
    for (const o of others) o.requests[0].resolve(o.read.address)
    await first
    /** The calls of `f`'s client function with `f`'s arguments. */
    const callsOf = (f: (typeof factories)[number]) =>
      mocked(f.client).mock.calls.filter(
        (args) => JSON.stringify(args) === JSON.stringify(f.args),
      ).length
    for (const f of factories) {
      expect(entryOf(f.read())?.answer, f.name).toEqual({ args: f.args })
      expect(callsOf(f), f.name).toBe(1)
    }

    invalidateLibraryViews()
    await landed()
    for (const f of factories) expect(callsOf(f), f.name).toBe(2)
    for (const o of others) {
      expect(o.fetch, o.read.address).toHaveBeenCalledTimes(1)
      expect(entryOf(o.read)?.stale, o.read.address).toBe(false)
    }
  })

  it('marks the split editions kept stale on invalidateLibraryViews, sending nothing while nothing draws them', async () => {
    const split = await keptSplitEditions()
    expect(split.stale()).toBe(false)
    invalidateLibraryViews()
    await landed()
    expect(split.stale()).toBe(true)
    expect(split.sent()).toBe(1)
  })
})

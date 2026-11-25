/**
 * The pages a scan changes read again when it ends, whoever started it: told
 * by the scan feed (O273). Stubbed at `fetch`, not by mocking a module, so
 * that each case runs the feed's poll and the pages' reads as a tab does.
 */
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SCAN_POLL_MS } from '../src/config'
import type { Library } from '../src/libraries'
import { Libraries } from '../src/Libraries'
import { Library as LibraryView } from '../src/Library'
import type { Grid, LibraryStatus, SeriesCard } from '../src/library'
import { JOINING_DOT } from '../src/joiningDot'
import { FEED_IDLE_MS, type ScanFeed } from '../src/scanFeed'
import { inUtc } from '../src/utc'
import { dir, idle, manga } from './support/libraries'
import { mount } from './support/mount'
import { feedJob, serveFeed } from './support/scanFeed'
import { cellValue } from './support/table'
import { StatusContext } from '../src/shell'
import { deferred } from './support/async'
import { StatusFromStore } from './support/statusFromStore'
import { watchDrawn } from './support/wait'

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })

/**
 * Stubs the reads the Libraries screen sends for a library's page, the list
 * answered with `list()` as each read is answered, and Scan library's start
 * of library 7, which reserves the directories `list()` gives it.
 */
function serveScreen(list: () => Library[]) {
  let held: Promise<void> | undefined
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url === '/api/library/libraries') {
        const wait = held
        held = undefined
        const answer = () => json({ libraries: list() })
        return wait === undefined
          ? Promise.resolve(answer())
          : wait.then(answer)
      }
      if (url === '/api/library/libraries/7/scan' && init?.method === 'POST') {
        const reserved = list()
          .filter((one) => one.id === 7)
          .flatMap((one) => one.dirs.map((d) => d.id))
        return Promise.resolve(
          json({ library_id: 7, status_url: url, root_ids: reserved }, 202),
        )
      }
      if (url === '/api/library/libraries/7/scan')
        return Promise.resolve(json(idle(7)))
      if (url === '/api/library/status')
        return Promise.resolve(json({ libraries: [], restricted: false }))
      return Promise.reject(new Error(`unexpected ${url}`))
    }),
  )
  return {
    /**
     * Holds the next read of the list open until the returned function is
     * called; it then answers with what `list()` says then.
     */
    hold(): () => void {
      const release = deferred<void>()
      held = release.promise
      return () => release.resolve()
    },
  }
}

/** Lets `ms` of the clock run, and the reads sent meanwhile land. */
const clock = (ms: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })

/** What each row of the library page's Directories table says under Last scan. */
const lastScans = (host: HTMLElement) =>
  [...host.querySelectorAll('.library-page .dir-table tbody tr')].map((row) =>
    cellValue(row.querySelectorAll('td')[2]),
  )

/** What a directory's page says of its last scan and its next. */
const dirFacts = (host: HTMLElement) =>
  [...host.querySelectorAll('.dir-page .regime > div')]
    .filter((fact) =>
      ['last scan', 'next scan'].includes(
        fact.querySelector('dt')?.textContent ?? '',
      ),
    )
    .map((fact) => fact.querySelector('dd')?.textContent)

const idleFeed = (ended: number): ScanFeed => ({ ended, libraries: [] })

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('a scan another page or the scheduler started, as it ends', () => {
  it('a library page reads its list again when a scan it did not start ends (O273)', async () => {
    let scanned: string | null = null
    serveScreen(() => [
      manga({
        dirs: [
          {
            ...dir(1, '/media/Manga/Volumes'),
            last_scan_at: scanned,
            last_scan_status: scanned === null ? null : 'ok',
          },
        ],
      }),
    ])
    const feed = serveFeed(idleFeed(0))
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/config/libraries/Manga']}>
          <Routes>
            <Route path="/config/libraries/*" element={<Libraries />} />
          </Routes>
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await clock(0)
    expect(lastScans(host)).toEqual(['Never'])

    feed.answer({
      ended: 0,
      libraries: [{ id: 7, step: 'resolving' }],
      jobs: [feedJob()],
    })
    await clock(FEED_IDLE_MS)
    scanned = '2026-11-16 03:40:00'
    feed.answer(idleFeed(1))
    await clock(SCAN_POLL_MS)
    expect(lastScans(host)).toEqual([inUtc('2026-11-16 03:40:00')])
  })

  it('a directory page reads its facts again when a scan it did not start ends (O273)', async () => {
    let scanned = false
    serveScreen(() => [
      manga({
        dirs: [
          {
            ...dir(1, '/media/Manga/Volumes'),
            ...(scanned && {
              last_scan_at: '2026-11-16 03:40:00',
              last_scan_status: 'ok',
              next_scan_at: '2026-11-17 03:40:00',
            }),
          },
        ],
      }),
    ])
    const feed = serveFeed(idleFeed(0))
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/config/libraries/Manga/dirs/1']}>
          <Routes>
            <Route path="/config/libraries/*" element={<Libraries />} />
          </Routes>
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await clock(0)
    expect(dirFacts(host)).toEqual(['never', 'due'])

    feed.answer({
      ended: 0,
      libraries: [{ id: 7, step: 'resolving' }],
      jobs: [feedJob()],
    })
    await clock(FEED_IDLE_MS)
    scanned = true
    feed.answer(idleFeed(1))
    await clock(SCAN_POLL_MS)
    expect(dirFacts(host)).toEqual([
      `${inUtc('2026-11-16 03:40:00')}${JOINING_DOT}ok`,
      inUtc('2026-11-17 03:40:00'),
    ])
  })

  it('the reader’s view reads again when the scan ends', async () => {
    let grid: Grid = { series: [], unfiled: [] }
    vi.stubGlobal(
      'fetch',
      vi.fn((input: RequestInfo | URL) => {
        const url = String(input)
        if (url === '/api/library/series?library=1')
          return Promise.resolve(json(grid))
        if (url === '/api/library/home?library=1')
          return Promise.resolve(json({ continuing: [], added: [], read: [] }))
        return Promise.reject(new Error(`unexpected ${url}`))
      }),
    )
    // A reader's feed: the step alone, and no job.
    const feed = serveFeed({
      ended: 0,
      libraries: [{ id: 1, step: 'hashing' }],
    })
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/?library=1']}>
          <StatusContext.Provider
            value={{
              libraries: [{ id: 1, name: 'Manga', scanned: false }],
              restricted: true,
            }}
          >
            <LibraryView />
          </StatusContext.Provider>
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await clock(0)
    expect(host.querySelector('.empty h2')?.textContent).toBe('Scanning')

    grid = {
      series: [
        {
          id: 3,
          title: 'A Series',
          works: 1,
          editions: 1,
          aligned: 0,
          added: 3,
          unread: 1,
          cover: null,
        },
      ],
      unfiled: [],
    }
    feed.answer(idleFeed(1))
    await clock(SCAN_POLL_MS)
    expect(
      [...host.querySelectorAll('a[href^="/series/"]')].map((a) =>
        a.getAttribute('href'),
      ),
    ).toEqual(['/series/3?library=1'])
    expect(host.querySelector('.empty')).toBeNull()
  })
})

/**
 * Each case draws a job, lands its end's answer while the list's re-read is
 * held, reads the page, then lets the re-read land. What is drawn on the way
 * is watched from the job on: `seen` keeps each state once, so a frame of
 * the reads from before the scan drawn again at its end would be lost among
 * the states drawn before the job.
 */
describe('as a scan ends, a page draws no frame of the reads from before it (O259, O321)', () => {
  const VOLUMES = '/media/Manga/Volumes'
  const CHAPTERS = '/media/Manga/Chapters'
  /** What the job's step says, on a page and in the table. */
  const STEP = 'Reading files · 120 of 2,097 · directory 2 of 2'

  /** A scan of both of Manga's directories, directories 4 and 9, on the second. */
  const scanningBoth: ScanFeed = {
    ended: 0,
    libraries: [{ id: 7, step: 'resolving' }],
    jobs: [
      feedJob({
        dir: { at: 2, of: 2 },
        dirs: [
          { id: 4, path: VOLUMES },
          { id: 9, path: CHAPTERS },
        ],
      }),
    ],
  }

  /** The Libraries screen at `address`, once its first reads have landed. */
  async function mountAt(address: string): Promise<HTMLElement> {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={[address]}>
          <Routes>
            <Route path="/config/libraries/*" element={<Libraries />} />
          </Routes>
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await clock(0)
    return host
  }

  /**
   * Ends the scan the feed shows with the list's re-read the end sends held,
   * and resolves once the end's answer has landed, with what lets that read
   * land. The hold outlasts three polls' time before it does, which is under
   * `SCAN_END_HOLD_MS` by more than a slow run's clock drifts: a shorter cap,
   * publishing the end without its reads, would draw the old reads there.
   */
  async function endWithListHeld(
    screen: ReturnType<typeof serveScreen>,
    feed: ReturnType<typeof serveFeed>,
  ): Promise<() => Promise<void>> {
    const release = screen.hold()
    feed.answer(idleFeed(1))
    await clock(SCAN_POLL_MS)
    return async () => {
      await clock(3 * SCAN_POLL_MS)
      release()
      await clock(0)
    }
  }

  /** A library page's step lines, and its paused line or `null`. */
  const libraryLines = (host: HTMLElement) => [
    [...host.querySelectorAll('.library-page > p.scanning')].map(
      (line) => line.textContent,
    ),
    host.querySelector('.library-page > p.paused')?.textContent ?? null,
  ]

  /** A directory page's step line, its facts and its paused line, each or `null`. */
  const dirLines = (host: HTMLElement) => [
    host.querySelector('.dir-page > p.scanning')?.textContent ?? null,
    dirFacts(host),
    host.querySelector('.dir-page > p.paused')?.textContent ?? null,
  ]

  /** The Libraries table's rows, each its name, its Works and its Last scan. */
  const tableRows = (host: HTMLElement) =>
    [...host.querySelectorAll('.library-table tbody tr')].map((row) => {
      const cells = row.querySelectorAll('td')
      return [cellValue(cells[0]), cellValue(cells[2]), cellValue(cells[3])]
    })

  const pausedLine = (at: string) =>
    `Paused until Scan is pressed, since ${inUtc(at)}`

  it('a library page says Scanning… under each directory’s Last scan until the list read as a first scan completes lands, then each its own time', async () => {
    let scanned = false
    const screen = serveScreen(() => [
      manga({
        dirs: [
          {
            ...dir(4, VOLUMES),
            ...(scanned && {
              last_scan_at: '2026-11-16 03:40:00',
              last_scan_status: 'ok',
            }),
          },
          {
            ...dir(9, CHAPTERS),
            ...(scanned && {
              last_scan_at: '2026-11-16 04:12:00',
              last_scan_status: 'ok',
            }),
          },
        ],
      }),
    ])
    const feed = serveFeed(idleFeed(0))
    const host = await mountAt('/config/libraries/Manga')
    expect(lastScans(host)).toEqual(['Never', 'Never'])
    feed.answer(scanningBoth)
    await clock(FEED_IDLE_MS)
    expect(lastScans(host)).toEqual(['Scanning…', 'Scanning…'])
    const cells = watchDrawn(() => JSON.stringify(lastScans(host)))

    scanned = true
    const listLands = await endWithListHeld(screen, feed)
    expect(lastScans(host)).toEqual(['Scanning…', 'Scanning…'])
    await listLands()
    cells.stop()
    expect(lastScans(host)).toEqual([
      inUtc('2026-11-16 03:40:00'),
      inUtc('2026-11-16 04:12:00'),
    ])
    const states = [...cells.seen].map((state) => JSON.parse(state) as string[])
    expect(states.filter((one) => one.includes('Never'))).toEqual([])
  })

  it('a library page whose Scan library was pressed over a pause never draws the pause again as that scan completes', async () => {
    const pausedAt = '2026-11-15 21:05:00'
    let scanned = false
    const screen = serveScreen(() => [
      manga({
        dirs: [
          {
            ...dir(4, VOLUMES),
            last_scan_at: scanned
              ? '2026-11-16 03:40:00'
              : '2026-11-14 22:00:00',
            last_scan_status: 'ok',
          },
          {
            ...dir(9, CHAPTERS),
            ...(scanned
              ? { last_scan_at: '2026-11-16 04:12:00', last_scan_status: 'ok' }
              : { last_scan_status: 'cancelled', paused_at: pausedAt }),
          },
        ],
      }),
    ])
    const feed = serveFeed(idleFeed(0))
    const host = await mountAt('/config/libraries/Manga')
    expect(libraryLines(host)).toEqual([[], pausedLine(pausedAt)])

    // The press reads no list, so the page's list still holds the pause; the
    // feed's job is what takes its line away.
    feed.answer(scanningBoth)
    const scanLibrary = host.querySelector<HTMLButtonElement>(
      '.library-page button[name="scan-library"]',
    )
    if (scanLibrary === null) throw new Error('no Scan library')
    await act(async () => {
      scanLibrary.click()
    })
    await clock(0)
    expect(libraryLines(host)).toEqual([[STEP], null])
    const lines = watchDrawn(() => JSON.stringify(libraryLines(host)))

    scanned = true
    const listLands = await endWithListHeld(screen, feed)
    expect(libraryLines(host)).toEqual([[STEP], null])
    await listLands()
    lines.stop()
    expect(libraryLines(host)).toEqual([[], null])
    expect([...lines.seen]).toEqual([
      JSON.stringify([[STEP], null]),
      JSON.stringify([[], null]),
    ])
  })

  it('a library page goes from a job’s step to the pause a cancel made elsewhere in one step', async () => {
    const pausedAt = '2026-11-16 03:52:00'
    let cancelled = false
    const screen = serveScreen(() => [
      manga({
        dirs: [dir(4, VOLUMES), dir(9, CHAPTERS)].map((one) =>
          cancelled
            ? { ...one, last_scan_status: 'cancelled', paused_at: pausedAt }
            : one,
        ),
      }),
    ])
    const feed = serveFeed(scanningBoth)
    const host = await mountAt('/config/libraries/Manga')
    expect(libraryLines(host)).toEqual([[STEP], null])
    const lines = watchDrawn(() => JSON.stringify(libraryLines(host)))

    cancelled = true
    const listLands = await endWithListHeld(screen, feed)
    expect(libraryLines(host)).toEqual([[STEP], null])
    await listLands()
    lines.stop()
    expect(libraryLines(host)).toEqual([[], pausedLine(pausedAt)])
    expect([...lines.seen]).toEqual([
      JSON.stringify([[STEP], null]),
      JSON.stringify([[], pausedLine(pausedAt)]),
    ])
  })

  it.each([
    {
      end: 'is cancelled',
      after: {
        last_scan_at: null,
        last_scan_status: 'cancelled',
        next_scan_at: null,
        paused_at: '2026-11-16 03:52:00',
      },
      facts: ['never'],
      paused: pausedLine('2026-11-16 03:52:00'),
    },
    {
      end: 'completes',
      after: {
        last_scan_at: '2026-11-16 03:58:00',
        last_scan_status: 'ok',
        next_scan_at: '2026-11-17 03:58:00',
      },
      facts: [
        `${inUtc('2026-11-16 03:58:00')}${JOINING_DOT}ok`,
        inUtc('2026-11-17 03:58:00'),
      ],
      paused: null,
    },
  ])(
    'a directory page whose scan $end draws what that scan left as the scan leaves the page',
    async ({ after, facts, paused }) => {
      let ended = false
      const screen = serveScreen(() => [
        manga({
          dirs: [
            {
              ...dir(4, VOLUMES),
              ...(ended
                ? after
                : {
                    last_scan_at: '2026-11-16 03:47:00',
                    last_scan_status: 'deriving',
                    next_scan_at: '2026-11-17 03:47:00',
                  }),
            },
            dir(9, CHAPTERS),
          ],
        }),
      ])
      const feed = serveFeed(scanningBoth)
      const host = await mountAt('/config/libraries/Manga/dirs/4')
      const running = [STEP, ['scanning…', inUtc('2026-11-17 03:47:00')], null]
      expect(dirLines(host)).toEqual(running)
      const lines = watchDrawn(() => JSON.stringify(dirLines(host)))

      ended = true
      const listLands = await endWithListHeld(screen, feed)
      expect(dirLines(host)).toEqual(running)
      await listLands()
      lines.stop()
      expect(dirLines(host)).toEqual([null, facts, paused])
      const states = [...lines.seen].map(
        (state) => JSON.parse(state) as ReturnType<typeof dirLines>,
      )
      expect(states.filter(([step]) => step === null)).toEqual([
        [null, facts, paused],
      ])
    },
  )

  it.each([
    {
      end: 'completes',
      dirsAfter: [
        { last_scan_at: '2026-11-16 03:40:00', last_scan_status: 'ok' },
        { last_scan_at: '2026-11-16 04:12:00', last_scan_status: 'ok' },
      ],
      lastScan: inUtc('2026-11-16 04:12:00'),
    },
    {
      end: 'is cancelled after writing works',
      dirsAfter: [
        { last_scan_status: 'cancelled', paused_at: '2026-11-16 03:52:00' },
        { last_scan_status: 'cancelled', paused_at: '2026-11-16 03:52:00' },
      ],
      lastScan: 'Never',
    },
  ])(
    'the Libraries table keeps a library’s row Scanning until the list read as its first scan $end lands, and the other library’s row as it was',
    async ({ dirsAfter, lastScan }) => {
      let ended = false
      const comics: Library = {
        id: 12,
        name: 'Comics',
        works: 5,
        books: 0,
        match_by_content: false,
        dirs: [
          {
            ...dir(6, '/media/Comics', [{ id: 12, name: 'Comics' }]),
            last_scan_at: '2026-11-10 08:00:00',
            last_scan_status: 'ok',
          },
        ],
      }
      const screen = serveScreen(() => [
        comics,
        manga({
          works: ended ? 3 : 0,
          dirs: [dir(4, VOLUMES), dir(9, CHAPTERS)].map((one, i) =>
            ended ? { ...one, ...dirsAfter[i] } : one,
          ),
        }),
      ])
      const feed = serveFeed(scanningBoth)
      const host = await mountAt('/config/libraries')
      const comicsRow = ['Comics', '5 works', inUtc('2026-11-10 08:00:00')]
      const scanningRow = ['Manga', '0 works', `Scanning${JOINING_DOT}${STEP}`]
      const mangaRow = (rows: string[][]) =>
        rows.find(([name]) => name === 'Manga')
      expect(mangaRow(tableRows(host))).toEqual(scanningRow)
      const rows = watchDrawn(() => JSON.stringify(tableRows(host)))

      ended = true
      const listLands = await endWithListHeld(screen, feed)
      expect(mangaRow(tableRows(host))).toEqual(scanningRow)
      await listLands()
      rows.stop()
      expect(mangaRow(tableRows(host))).toEqual(['Manga', '3 works', lastScan])
      const states = [...rows.seen].map(
        (state) => JSON.parse(state) as string[][],
      )
      // The words a row drawn from the list before the scan says, which no
      // state may draw once Scanning has gone: Never is the cancel's own.
      const before = ['0 works', ...(lastScan === 'Never' ? [] : ['Never'])]
      expect(
        states
          .map(mangaRow)
          .filter(
            (row) =>
              row !== undefined &&
              !row[2].startsWith('Scanning') &&
              (before.includes(row[1]) || before.includes(row[2])),
          ),
      ).toEqual([])
      expect(
        states.map((one) => one.find(([name]) => name === 'Comics')),
      ).toEqual(states.map(() => comicsRow))
    },
  )
})

/**
 * The reader's empty view as a first scan of library 7 ends, under the status
 * read through the store as the shell reads it, so that the end's set reads it
 * again with the grid. Each case lands the end's answer with the status's and
 * the grid's re-reads held, and lets the last of them land three polls' time
 * later, under `SCAN_END_HOLD_MS` by the margin the cases above keep: a
 * shorter cap would draw the reads from before the scan there. What is drawn
 * is watched from the scan on.
 */
describe('as a first scan ends, the reader’s empty view draws no frame of the reads from before it (O321)', () => {
  const STATUS = '/api/library/status'
  const GRID = '/api/library/series?library=7'
  const HOME = '/api/library/home?library=7'
  /** What the job's step says under Scanning. */
  const STEP = 'Reading files · 120 of 2,097 · directory 1 of 1'

  /** Comics, library 12, read before, and Manga, library 7, read or not. */
  const statusWith = (scanned: boolean): LibraryStatus => ({
    libraries: [
      { id: 12, name: 'Comics', scanned: true },
      { id: 7, name: 'Manga', scanned },
    ],
    restricted: false,
  })

  const seriesCard = (id: number, title: string): SeriesCard => ({
    id,
    title,
    works: 1,
    editions: 1,
    aligned: 0,
    added: id,
    unread: 1,
    cover: null,
  })

  const noSeries: Grid = { series: [], unfiled: [] }

  /** A scan of Manga, library 7, as an administrator is sent it. */
  const scanningManga: ScanFeed = {
    ended: 0,
    libraries: [{ id: 7, step: 'resolving' }],
    jobs: [feedJob()],
  }

  /**
   * Stubs the status and library 7's grid and home lists, the status and
   * the grid answered with what `now()` says as each read is answered.
   */
  function serveView(now: () => { status: LibraryStatus; grid: Grid }) {
    const held = new Map<string, Promise<void>>()
    vi.stubGlobal(
      'fetch',
      vi.fn((input: RequestInfo | URL) => {
        const url = String(input)
        if (![STATUS, GRID, HOME].includes(url))
          return Promise.reject(new Error(`unexpected ${url}`))
        const answer = () =>
          url === STATUS
            ? json(now().status)
            : url === GRID
              ? json(now().grid)
              : json({ continuing: [], added: [], read: [] })
        const wait = held.get(url)
        held.delete(url)
        return wait === undefined
          ? Promise.resolve(answer())
          : wait.then(answer)
      }),
    )
    return {
      /**
       * Holds the next read of `url` open until the returned function is
       * called; it then answers with what `now()` says then.
       */
      hold(url: string): () => void {
        const release = deferred<void>()
        held.set(url, release.promise)
        return () => release.resolve()
      },
    }
  }

  afterEach(() => {
    // jsdom's own getter, on the prototype, answers again.
    Reflect.deleteProperty(document, 'visibilityState')
  })

  /** The tab turns `state`, as the browser tells a page. */
  const turn = (state: DocumentVisibilityState) =>
    act(async () => {
      Object.defineProperty(document, 'visibilityState', {
        value: state,
        configurable: true,
      })
      document.dispatchEvent(new Event('visibilitychange'))
    })

  /** Library 7's page, once its first reads and the feed's first answer have landed. */
  async function mountView(): Promise<HTMLElement> {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/?library=7']}>
          <StatusFromStore>
            <LibraryView />
          </StatusFromStore>
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await clock(0)
    return host
  }

  /** What the empty state says, its heading and each line under it. */
  const said = (host: HTMLElement) => {
    const empty = host.querySelector('.empty')
    return [
      empty?.querySelector('h2')?.textContent,
      ...[...(empty?.querySelectorAll('p') ?? [])].map((p) => p.textContent),
    ]
  }

  /** The view's empty state's heading, or `null`, and each series it links to. */
  const viewDrawn = (host: HTMLElement) => [
    host.querySelector('.empty h2')?.textContent ?? null,
    [...host.querySelectorAll('a[href^="/series/"]')].map((a) =>
      a.getAttribute('href'),
    ),
  ]

  /**
   * Ends the scan with the status's and the grid's re-reads held, lets
   * `first` land two polls' time after the end's answer and `last` a poll
   * after that, and resolves once both have landed.
   */
  async function endWithReadsHeld(
    view: ReturnType<typeof serveView>,
    feed: ReturnType<typeof serveFeed>,
    first: string,
    last: string,
  ) {
    const releaseFirst = view.hold(first)
    const releaseLast = view.hold(last)
    feed.answer(idleFeed(1))
    await clock(SCAN_POLL_MS)
    await clock(2 * SCAN_POLL_MS)
    releaseFirst()
    await clock(SCAN_POLL_MS)
    releaseLast()
    await clock(0)
  }

  const works = [null, ['/series/3?library=7', '/series/5?library=7']]

  /** Library 7's grid once the scan has written the two series `works` links to. */
  const withWorks: Grid = {
    series: [seriesCard(3, 'A Series'), seriesCard(5, 'Another Series')],
    unfiled: [],
  }

  it.each([
    { first: STATUS, last: GRID, lands: 'the status’s re-read first' },
    { first: GRID, last: STATUS, lands: 'the grid’s re-read first' },
  ])(
    'a first scan that completes with works goes from Scanning to the works, with $lands',
    async ({ first, last }) => {
      let ended = false
      const view = serveView(() => ({
        status: statusWith(ended),
        grid: ended ? withWorks : noSeries,
      }))
      const feed = serveFeed(scanningManga)
      const host = await mountView()
      expect(said(host)).toEqual(['Scanning', STEP])
      const drawn = watchDrawn(() => JSON.stringify(viewDrawn(host)))

      ended = true
      await endWithReadsHeld(view, feed, first, last)
      drawn.stop()
      expect(viewDrawn(host)).toEqual(works)
      expect([...drawn.seen].map((state) => JSON.parse(state))).toEqual([
        ['Scanning', []],
        works,
      ])
    },
  )

  it.each([
    { first: STATUS, last: GRID, lands: 'the status’s re-read first' },
    { first: GRID, last: STATUS, lands: 'the grid’s re-read first' },
  ])(
    'a first scan that completes with nothing written goes from Scanning to Nothing to read yet, with $lands',
    async ({ first, last }) => {
      let ended = false
      const view = serveView(() => ({
        status: statusWith(ended),
        grid: noSeries,
      }))
      const feed = serveFeed(scanningManga)
      const host = await mountView()
      expect(said(host)).toEqual(['Scanning', STEP])
      const drawn = watchDrawn(() => JSON.stringify(viewDrawn(host)))

      ended = true
      await endWithReadsHeld(view, feed, first, last)
      drawn.stop()
      expect([...drawn.seen].map((state) => JSON.parse(state))).toEqual([
        ['Scanning', []],
        ['Nothing to read yet', []],
      ])
    },
  )

  // The feed's first answer in a tab opened hidden follows a poll it
  // dropped, so it is held for its re-reads as an end seen by a poll is: the
  // view waits for that answer, and drawn at once it would end the wait over
  // the reads from before the scan.
  it('a view opened in a hidden tab while the first scan runs goes from its wait to the works once shown, never Not read yet', async () => {
    let ended = false
    const view = serveView(() => ({
      status: statusWith(ended),
      grid: ended ? withWorks : noSeries,
    }))
    const feed = serveFeed(scanningManga)
    Object.defineProperty(document, 'visibilityState', {
      value: 'hidden',
      configurable: true,
    })
    const host = await mountView()
    const drawn = watchDrawn(() => JSON.stringify(viewDrawn(host)))

    ended = true
    feed.answer(idleFeed(1))
    const releaseStatus = view.hold(STATUS)
    const releaseGrid = view.hold(GRID)
    await turn('visible')
    await clock(2 * SCAN_POLL_MS)
    releaseStatus()
    await clock(SCAN_POLL_MS)
    releaseGrid()
    await clock(0)
    drawn.stop()
    expect(viewDrawn(host)).toEqual(works)
    expect([...drawn.seen].map((state) => JSON.parse(state))).toEqual([
      [null, []],
      works,
    ])
  })

  /**
   * Cancels a first scan of Manga, which leaves its status unread and its
   * grid `grid`, and expects the view to go from Scanning to `after` alone.
   */
  async function cancelledLeaves(grid: Grid, after: unknown[]) {
    let ended = false
    const view = serveView(() => ({
      status: statusWith(false),
      grid: ended ? grid : noSeries,
    }))
    const feed = serveFeed(scanningManga)
    const host = await mountView()
    expect(said(host)).toEqual(['Scanning', STEP])
    const drawn = watchDrawn(() => JSON.stringify(viewDrawn(host)))

    ended = true
    await endWithReadsHeld(view, feed, STATUS, GRID)
    drawn.stop()
    expect(viewDrawn(host)).toEqual(after)
    expect([...drawn.seen].map((state) => JSON.parse(state))).toEqual([
      ['Scanning', []],
      after,
    ])
  }

  it('a first scan cancelled after writing works goes from Scanning to its works, never Not read yet', async () => {
    await cancelledLeaves(
      { series: [seriesCard(3, 'A Series')], unfiled: [] },
      [null, ['/series/3?library=7']],
    )
  })

  // A control: a cancel that wrote nothing ends on Not read yet, so the watch
  // is seen to record that heading where the view draws it.
  it('a first scan cancelled with nothing written goes from Scanning to Not read yet', async () => {
    await cancelledLeaves(noSeries, ['Not read yet', []])
  })
})

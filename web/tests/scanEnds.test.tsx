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
import type { Grid } from '../src/library'
import { JOINING_DOT } from '../src/joiningDot'
import { FEED_IDLE_MS, type ScanFeed } from '../src/scanFeed'
import { inUtc } from '../src/utc'
import { dir, idle, manga } from './support/libraries'
import { mount } from './support/mount'
import { feedJob, serveFeed } from './support/scanFeed'
import { cellValue } from './support/table'
import { StatusContext } from '../src/shell'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    headers: { 'content-type': 'application/json' },
  })

/**
 * Stubs the reads the Libraries screen sends for a library's page, the list
 * answered with `list()` as each read is sent.
 */
function serveScreen(list: () => Library[]) {
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/library/libraries')
        return Promise.resolve(json({ libraries: list() }))
      if (url === '/api/library/libraries/7/scan')
        return Promise.resolve(json(idle(7)))
      if (url === '/api/library/status')
        return Promise.resolve(json({ libraries: [], restricted: false }))
      return Promise.reject(new Error(`unexpected ${url}`))
    }),
  )
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

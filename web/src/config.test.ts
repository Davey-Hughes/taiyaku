import { afterEach, describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from './api'
import {
  addRoot,
  humanInterval,
  listRoots,
  patchRoot,
  regimeLine,
  removalCost,
  removeRoot,
  type Root,
  scanFailed,
  scanStatus,
  startScan,
  TABS,
} from './config'

const root = (over: Partial<Root> = {}): Root => ({
  id: 1,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: '2026-09-04T10:00:00Z',
  last_scan_status: 'ok',
  ...over,
})

describe('TABS', () => {
  it('leads with the tab the index redirect lands on', () => {
    // `Config.tsx` sends `/config` to `TABS[0].path`. A tab list whose first
    // entry is not a real route is a redirect into nothing.
    expect(TABS.length).toBeGreaterThan(0)
    expect(TABS[0].path).toBe('roots')
  })

  it('gives every tab a distinct path', () => {
    const paths = TABS.map((t) => t.path)
    expect(new Set(paths).size).toBe(paths.length)
  })
})

describe('humanInterval', () => {
  it('reads a day as hours, not 86400 seconds', () => {
    expect(humanInterval(86400)).toBe('24h')
  })

  it('reads a quarter hour as minutes', () => {
    expect(humanInterval(900)).toBe('15m')
  })

  it('keeps seconds below a minute', () => {
    expect(humanInterval(30)).toBe('30s')
  })

  it('rounds down rather than inventing precision', () => {
    // 90 minutes is 1h, not "1.5h": the interval is a coarse operational
    // fact and a fractional hour reads as a measurement it is not.
    expect(humanInterval(5400)).toBe('1h')
  })
})

describe('regimeLine', () => {
  it('says the watch state is unknown when no probe has run', () => {
    // `GET /api/library/roots` does not carry the probe — only `POST` does.
    // Spec §3.1: the honest gap beats a number nobody measured.
    expect(regimeLine(root())).toContain('watch state unknown')
  })

  it('reports a probe that succeeded', () => {
    expect(
      regimeLine(root(), { watching: true, watch_reason: null }),
    ).toContain('watching')
  })

  it('names why a probe failed', () => {
    expect(
      regimeLine(root(), {
        watching: false,
        watch_reason: 'inotify limit reached',
      }),
    ).toContain('inotify limit reached')
  })

  it('says a root has never been scanned rather than drawing a null', () => {
    const line = regimeLine(
      root({ last_scan_at: null, last_scan_status: null }),
    )
    expect(line).toContain('never scanned')
    expect(line).not.toContain('null')
  })

  it('carries the last scan status when there is one', () => {
    expect(regimeLine(root({ last_scan_status: 'refused' }))).toContain(
      'refused',
    )
  })

  it('says so when no interval is set', () => {
    expect(regimeLine(root({ scan_interval_secs: null }))).toContain(
      'no scan interval',
    )
  })
})

describe('scanFailed', () => {
  it('tells a failure from a finished report', () => {
    expect(scanFailed({ error: 'the scan panicked' })).toBe(true)
  })
})

/**
 * A fresh `Response` per call, not one shared instance — `Response.json()`
 * consumes its body stream, so a second read of the same instance throws
 * "Body is unusable" rather than exercising the second assertion.
 * `library.test.ts` carries the same note for the same reason.
 */
const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the roots client', () => {
  it('asks for the roots list', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json({ roots: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await listRoots()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/roots')
  })

  it('posts the path, and nothing else, to add a root', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ root: {}, watching: true }))
    vi.stubGlobal('fetch', fetchMock)
    await addRoot('/srv/manga')
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/roots')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    expect(init.body).toBe(JSON.stringify({ path: '/srv/manga' }))
  })

  it('patches with only the columns it was handed', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await patchRoot(3, { edition_key: null })
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/roots/3')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('PATCH')
    expect(init.body).toBe(JSON.stringify({ edition_key: null }))
  })

  it('spells dry_run exactly, because the server refuses any other key', async () => {
    // `deny_unknown_fields`: before it existed, `?dryrun=true` — one key off —
    // answered 200 and ran the real, committed removal. A typo here is a
    // deleted library, so the exact string is pinned rather than assumed.
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        json({ removed: {}, derive: {}, dry_run: true }),
      )
    vi.stubGlobal('fetch', fetchMock)
    await removalCost(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/roots/3?dry_run=true',
    )
  })

  it('removes without the dry-run key at all', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        json({ removed: {}, derive: {}, dry_run: false }),
      )
    vi.stubGlobal('fetch', fetchMock)
    await removeRoot(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/roots/3')
    expect(fetchMock.mock.calls[0]?.[0]).not.toContain('dry_run')
  })

  it('starts and polls a scan at the same path, by different methods', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        json({ root_id: 3, status_url: '/api/library/roots/3/scan' }),
      )
    vi.stubGlobal('fetch', fetchMock)
    await startScan(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/roots/3/scan')
    // Bound first, rather than `(…?.[1] as RequestInit).method` inline:
    // `noUnsafeOptionalChaining` refuses a non-optional access hanging off an
    // optional chain, since the chain short-circuiting to `undefined` makes it
    // a TypeError. `pnpm format` does not see this; `biome check` does.
    const started = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(started.method).toBe('POST')

    await scanStatus(3)
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/roots/3/scan')
    expect(fetchMock.mock.calls[1]?.[1]).toBeUndefined()
  })

  it('refuses to pass off an empty 204 as a body', async () => {
    // `expectBody`'s whole job. A route that answered 204 where this client
    // expects a body would otherwise hand `undefined` to a screen that then
    // reads `.removed.works` off it.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 204 })),
    )
    await expect(removalCost(3)).rejects.toThrow(/empty 204/)
  })

  it('routes a 401 through the one place that notices it', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 401 })),
    )
    await expect(listRoots()).rejects.toBeInstanceOf(Unauthenticated)
  })

  it("carries the server's own sentence out of a refusal, not just the code", async () => {
    // Spec §3.2: the screen renders the message that names *which* of the
    // three validations failed. A thrower that dropped the body would throw
    // that away at the client boundary.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('/srv/a/b sits inside the existing root /srv/a', {
          status: 400,
        }),
      ),
    )
    await expect(addRoot('/srv/a/b')).rejects.toThrow(/sits inside/)
  })
})

import { afterEach, describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from './api'
import {
  humanInterval,
  listRoots,
  patchRoot,
  regimeFacts,
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
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: '2026-09-04T10:00:00Z',
  last_scan_status: 'ok',
  ...over,
})

describe('TABS', () => {
  it('leads with the tab the index redirect lands on', () => {
    // `Config.tsx` sends `/config` to `TABS[0].path`. A tab list whose first
    // entry is not a real route is a redirect into nothing. PR B prepends the
    // rule editor, which is what makes the flagship the landing screen — the
    // build design's §2 records that as the one intended move of this list.
    expect(TABS.length).toBeGreaterThan(0)
    expect(TABS[0].path).toBe('rules')
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

describe('regimeFacts', () => {
  // Ten candidate cases (six migrated from the old `regimeLine` sentence
  // test, four written fresh for the labelled-list shape) collapsed to five
  // here, and Task 4's review added a sixth for the one arm none of the ten
  // had ever reached — a failed probe with no reason. Six: a case survives only if it exercises a behaviour none of the
  // others do. "carries the last scan status when there is one" and "keeps
  // the outcome with the date" both just assert the status gets appended
  // after the date — one case, not two — and the full-array case below
  // subsumes "no probe", "never scanned" and "no interval set" on its own,
  // since each is one arm of the same three-entry result.

  it('names each fact under its own label, in order watch / scan every / last scan', () => {
    // A folder read from a list carries no probe (only a route that adds a
    // folder runs one) and this root has never been scanned and has no
    // interval set — every arm at its fallback, and the order the caller's
    // `<dl>` renders in. Spec §3.1: the honest gap beats a number nobody
    // measured.
    const bare = root({
      scan_interval_secs: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    expect(regimeFacts(bare)).toEqual([
      { label: 'watch', value: 'unknown — probed when a folder is added' },
      { label: 'scan every', value: 'not set' },
      { label: 'last scan', value: 'never' },
    ])
  })

  it('says a probe failed even when the server gave no reason', () => {
    // The one arm neither `regimeLine`'s six cases nor this block's first
    // five ever reached: `watching: false` with a null reason. Found by
    // Task 4's review, and older than Task 4 — the sentence version had the
    // same hole, so a typo here would have shipped either way.
    expect(
      regimeFacts(root(), { watching: false, watch_reason: null })[0],
    ).toEqual({ label: 'watch', value: 'probe failed' })
  })

  it('reports a probe that succeeded', () => {
    expect(
      regimeFacts(root(), { watching: true, watch_reason: null })[0],
    ).toEqual({ label: 'watch', value: 'watching' })
  })

  it('reports a failed watch probe with its reason', () => {
    expect(
      regimeFacts(root(), {
        watching: false,
        watch_reason: 'inotify limit',
      })[0],
    ).toEqual({ label: 'watch', value: 'probe failed: inotify limit' })
  })

  it('keeps the outcome with the date rather than beside the other facts', () => {
    const scanned = root({
      last_scan_at: '2026-09-17 08:33:49',
      last_scan_status: 'ok',
    })
    expect(regimeFacts(scanned)[2]).toEqual({
      label: 'last scan',
      value: '2026-09-17 08:33:49 · ok',
    })
  })

  it('states an unrecorded outcome rather than omitting it', () => {
    const scanned = root({
      last_scan_at: '2026-09-17 08:33:49',
      last_scan_status: null,
    })
    expect(regimeFacts(scanned)[2].value).toBe(
      '2026-09-17 08:33:49 · outcome unrecorded',
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
    // The screen renders the server's message, which names why a write was
    // refused. A thrower that dropped the body would throw that away at the
    // client boundary. Driven through `patchRoot`; the other writes send through
    // the same `sendJson`.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('/srv/a/b sits inside the existing root /srv/a', {
          status: 400,
        }),
      ),
    )
    await expect(patchRoot(3, { edition_key: null })).rejects.toThrow(
      /sits inside/,
    )
  })
})

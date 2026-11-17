import { afterEach, describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from '../src/api'
import {
  humanInterval,
  keptSchedule,
  lastScan,
  listRoots,
  patchRoot,
  regimeFacts,
  removalCost,
  removeRoot,
  type Root,
  type ScanOutcome,
  scanCancelled,
  scanFailed,
  scanStatus,
  startScan,
  TABS,
} from '../src/config'

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
  next_scan_at: null,
  paused_at: null,
  ...over,
})

describe('TABS', () => {
  it('leads with the tab the index redirect lands on', () => {
    // `Config.tsx` sends `/config` to `TABS[0].path`, so the first entry is
    // the landing screen: Libraries, where a new server starts (O219 §1.4).
    expect(TABS.length).toBeGreaterThan(0)
    expect(TABS[0].path).toBe('libraries')
  })

  it('orders the tabs Libraries, Users, Rules, Backup — O219 §1.4 — then Split editions', () => {
    expect(TABS.map((t) => [t.path, t.label])).toEqual([
      ['libraries', 'Libraries'],
      ['users', 'Users'],
      ['rules', 'Rules'],
      ['backup', 'Backup'],
      ['split-editions', 'Split editions'],
    ])
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

  it('reads an interval that is not whole hours in minutes, as the Scan schedule counts it, rounded down', () => {
    expect(humanInterval(5400)).toBe('90m')
    expect(humanInterval(9000)).toBe('150m')
    expect(humanInterval(3660)).toBe('61m')
    expect(humanInterval(330)).toBe('5m')
  })
})

describe('regimeFacts', () => {
  // A case is here only if it exercises a behaviour none of the others do.
  // The full-array case below subsumes "no probe", "never scanned",
  // "automatic" and "due" on its own, since each is one arm of the same
  // four-entry result.

  it('names each fact under its own label, in order watch / scan every / last scan / next scan', () => {
    // A directory read from a list carries no probe (only a route that adds a
    // directory runs one), and this root has never been scanned and keeps the
    // automatic schedule — every arm at its fallback, and the order the
    // caller's `<dl>` renders in. Spec §3.1: the honest gap beats a number
    // nobody measured.
    const bare = root({
      scan_interval_secs: null,
      last_scan_at: null,
      last_scan_status: null,
      next_scan_at: null,
    })
    expect(regimeFacts(bare)).toEqual([
      { label: 'watch', value: 'unknown — probed when a directory is added' },
      { label: 'scan every', value: 'automatic' },
      { label: 'last scan', value: 'never' },
      { label: 'next scan', value: 'due' },
    ])
  })

  it('says off for both the schedule and the next scan of a directory set to Off', () => {
    // Scanned, so the next scan's null is the Off's and not a first scan's:
    // only the interval tells the two apart.
    const off = root({
      scan_interval_secs: 0,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status: 'ok',
      next_scan_at: null,
    })
    expect(regimeFacts(off).slice(1)).toEqual([
      { label: 'scan every', value: 'off' },
      { label: 'last scan', value: '2026-11-10 12:00:00 UTC\u00a0·\u00a0ok' },
      { label: 'next scan', value: 'off' },
    ])
  })

  it('says off for an interval below zero, as the scheduler reads one, and that a directory never scanned waits for a press', () => {
    const below = root({
      scan_interval_secs: -1,
      last_scan_at: null,
      last_scan_status: null,
      next_scan_at: null,
    })
    expect(regimeFacts(below).slice(1)).toEqual([
      { label: 'scan every', value: 'off' },
      { label: 'last scan', value: 'never\u00a0·\u00a0until Scan is pressed' },
      { label: 'next scan', value: 'off' },
    ])
  })

  it('says a directory set to Off and never scanned whose pressed scan waits for the job lock never, as Scan was pressed', () => {
    const waiting = root({
      scan_interval_secs: 0,
      last_scan_at: null,
      last_scan_status: 'waiting',
      next_scan_at: null,
    })
    expect(regimeFacts(waiting).slice(1)).toEqual([
      { label: 'scan every', value: 'off' },
      { label: 'last scan', value: 'never' },
      { label: 'next scan', value: 'off' },
    ])
  })

  it('gives an interval in its unit and the next scan as a time in UTC', () => {
    const hourly = root({
      scan_interval_secs: 3600,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status: 'ok',
      next_scan_at: '2026-11-10 13:00:00',
    })
    expect(regimeFacts(hourly).slice(1)).toEqual([
      { label: 'scan every', value: '1h' },
      { label: 'last scan', value: '2026-11-10 12:00:00 UTC\u00a0·\u00a0ok' },
      { label: 'next scan', value: '2026-11-10 13:00:00 UTC' },
    ])
  })

  it('gives an interval of hours and minutes in minutes, as it was set, with the next scan that far on', () => {
    const ninety = root({
      scan_interval_secs: 5400,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status: 'ok',
      next_scan_at: '2026-11-10 13:30:00',
    })
    expect(regimeFacts(ninety).slice(1)).toEqual([
      { label: 'scan every', value: '90m' },
      { label: 'last scan', value: '2026-11-10 12:00:00 UTC · ok' },
      { label: 'next scan', value: '2026-11-10 13:30:00 UTC' },
    ])
    expect(regimeFacts(root({ scan_interval_secs: 9000 }))[1]?.value).toBe(
      '150m',
    )
  })

  it('draws an interval under the minimum as stored, and the next scan as the server gives it', () => {
    // Only a store written before the minimum holds such an interval. The
    // scheduler runs it at the minimum, so the server's next scan is five
    // minutes on, where the page's own sum would say one.
    const quick = root({
      scan_interval_secs: 60,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status: 'ok',
      next_scan_at: '2026-11-10 12:05:00',
    })
    expect(regimeFacts(quick).slice(1)).toEqual([
      { label: 'scan every', value: '1m' },
      { label: 'last scan', value: '2026-11-10 12:00:00 UTC · ok' },
      { label: 'next scan', value: '2026-11-10 12:05:00 UTC' },
    ])
  })

  it('draws no next scan for a paused directory, scanned or not, whatever the server sends', () => {
    // The server's `next_scan_at` keeps its meaning while paused: a scanned
    // directory's is a time, and a never-scanned one's is null, read `due`.
    const pausedEvery = root({
      scan_interval_secs: 3600,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status: 'cancelled',
      next_scan_at: '2026-11-10 13:00:00',
      paused_at: '2026-11-16 03:33:10',
    })
    const pausedNever = root({
      scan_interval_secs: null,
      last_scan_at: null,
      last_scan_status: 'cancelled',
      next_scan_at: null,
      paused_at: '2026-11-16 03:33:10',
    })
    const labels = (one: Root) => regimeFacts(one).map(({ label }) => label)
    expect([labels(pausedEvery), labels(pausedNever)]).toEqual([
      ['watch', 'scan every', 'last scan'],
      ['watch', 'scan every', 'last scan'],
    ])
    expect(regimeFacts({ ...pausedEvery, paused_at: null })[3]).toEqual({
      label: 'next scan',
      value: '2026-11-10 13:00:00 UTC',
    })
  })

  it('says a probe failed even when the server gave no reason', () => {
    // `watching: false` with a null reason is the arm no other case here
    // reaches, so a typo in this arm would ship unseen.
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
      value: '2026-09-17 08:33:49 UTC\u00a0·\u00a0ok',
    })
  })

  it('states an unrecorded outcome rather than omitting it', () => {
    const scanned = root({
      last_scan_at: '2026-09-17 08:33:49',
      last_scan_status: null,
    })
    expect(regimeFacts(scanned)[2].value).toBe(
      '2026-09-17 08:33:49 UTC\u00a0·\u00a0outcome unrecorded',
    )
  })
})

describe('lastScan', () => {
  it('draws a derive cut by a restart, one still running, and one waiting for another derive, after the time as it draws a failure', () => {
    const at = (last_scan_status: string) =>
      lastScan({ last_scan_at: '2026-11-10 12:00:00', last_scan_status })
    expect(['failed', 'interrupted', 'deriving', 'waiting'].map(at)).toEqual([
      '2026-11-10 12:00:00 UTC\u00a0·\u00a0failed',
      '2026-11-10 12:00:00 UTC\u00a0·\u00a0interrupted',
      '2026-11-10 12:00:00 UTC\u00a0·\u00a0deriving',
      '2026-11-10 12:00:00 UTC\u00a0·\u00a0waiting',
    ])
  })

  it('says a directory set to Off and never scanned waits for a press, and any other never-scanned one Never', () => {
    const never = { last_scan_at: null, last_scan_status: null }
    expect(lastScan({ ...never, scan_interval_secs: 0 })).toBe(
      'Never\u00a0·\u00a0until Scan is pressed',
    )
    expect(lastScan({ ...never, scan_interval_secs: null })).toBe('Never')
    expect(lastScan({ ...never, scan_interval_secs: 86400 })).toBe('Never')
    expect(lastScan(never)).toBe('Never')
  })

  it('says Never of a directory set to Off and never scanned whose pressed scan waits for the job lock, as Scan was pressed', () => {
    expect(
      lastScan({
        last_scan_at: null,
        last_scan_status: 'waiting',
        scan_interval_secs: 0,
      }),
    ).toBe('Never')
  })
})

describe('keptSchedule', () => {
  const kept = (scan_interval_secs: number | null, schedule_kept = true) =>
    keptSchedule({ schedule_kept, root: root({ scan_interval_secs }) })

  it('says nothing of an add that kept no schedule, whatever the directory’s', () => {
    expect([null, 0, 3600].map((secs) => kept(secs, false))).toEqual([
      undefined,
      undefined,
      undefined,
    ])
  })

  it('names the schedule a link kept as the line says it', () => {
    expect([null, 0, -1, 3600, 5400].map((secs) => kept(secs))).toEqual([
      'automatic',
      'off',
      'off',
      'every 1h',
      'every 90m',
    ])
  })
})

describe('scanFailed', () => {
  it('tells a failure from a finished report', () => {
    expect(scanFailed({ error: 'the scan panicked' })).toBe(true)
  })
})

describe('scanCancelled', () => {
  // A cancel leaves a refused walk's sentence in place, so a job's reports
  // hold both shapes side by side.
  it('tells a cancelled directory from a refused one, and each guard only its own', () => {
    const reports: ScanOutcome[] = [
      { cancelled: { step: 'hashing' } },
      { error: 'the directory is not readable' },
    ]
    expect(
      reports.map((report) => [scanCancelled(report), scanFailed(report)]),
    ).toEqual([
      [true, false],
      [false, true],
    ])
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
    // `deny_unknown_fields`: without it, `?dryrun=true` — one key off —
    // would answer 200 and run the real, committed removal. A typo here is a
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

  it('names the library whose page asks for a scan as a query, and none where none is given', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        json({ root_id: 3, status_url: '/api/library/roots/3/scan' }),
      )
    vi.stubGlobal('fetch', fetchMock)
    await startScan(3, 7)
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/roots/3/scan?library=7',
    )
    const started = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect([started.method, started.body]).toEqual(['POST', undefined])
    await startScan(3)
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/roots/3/scan')
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

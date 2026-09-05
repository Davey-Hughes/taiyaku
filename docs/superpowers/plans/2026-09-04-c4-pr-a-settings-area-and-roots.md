# C4 PR A — the settings area, roots and scan control

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the settings area at `/config` and its first screen — roots, their six editable columns, the three validations surfaced, removal with counts, and the scan trigger with its live poll and full report.

**Architecture:** A nested route area (`Config.tsx`) under the existing `Shell`, whose tab list lives in `config.ts` so the sub-nav and the index redirect read one list. `config.ts` is the typed client for `/api/library/roots*` and the scan routes, plus the pure regime-line helpers. `Roots.tsx` owns the list and the add form; `RootRow.tsx` owns one root's card, its edit and its scan poll; `RemoveRoot.tsx` owns the destructive dialog; `ScanReport.tsx` renders both server reports whole.

**Tech Stack:** React 19, `react-router` 8, TypeScript 7, Vitest 4 (`node` for `.test.ts`, `jsdom` for `.test.tsx`), Biome. No new runtime dependency.

**Spec:** `docs/superpowers/specs/2026-09-04-c4-screens-design.md` §2, §3, §7. Product design: `2026-08-14-library-configuration-design.md` §2 and §3.

## Global Constraints

- **No client-side admin guard.** Every `/config/*` route is reachable by URL; every screen renders the server's own 403 sentence. `getJson`/`sendJson` already throw it as the message. Spec §2.
- **Never call `fetch` directly.** Everything goes through `api.ts`'s `getJson`/`sendJson`, which is the one place a 401 is routed. Spec §2; `api.ts`'s own `whenSignedOut` doc.
- **Every report field is drawn, always** — including a collection that is empty this time. An empty `Vec` renders as `0`, never as a section that vanishes. Spec §3.4; C4 §3.
- **Zero is drawn, never omitted**, in the removal dialog as well as the report. Spec §3.3.
- **Screen tests are `.test.tsx` (jsdom); module tests are `.test.ts` (node).** `vite.config.ts` routes them by extension. Spec §7.
- **No new runtime dependency.** `package.json`'s `dependencies` is unchanged by this PR.
- **Doc comments carry the argument**, in the style of every neighbouring file: what this beat and what it cost, not what the code does.
- **Never add AI/Claude attribution** to any file, comment, commit or PR body.
- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`** — never with `TAIYAKU_CORPUS` set, and never piped through `tail`, which masks the exit status. Quote the `all green` line as evidence.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `web/src/config.ts` | `TABS`, the roots/scan types, the client, and the pure regime-line helpers |
| `web/src/ScanReport.tsx` | Both server reports rendered whole, did-not-do first |
| `web/src/RemoveRoot.tsx` | The removal dialog: the dry run, the counts, the confirm |
| `web/src/RootRow.tsx` | One root's card: the regime line, the six fields, edit, scan trigger and poll |
| `web/src/Roots.tsx` | The list, the add form, the reload |
| `web/src/Config.tsx` | The area shell: the sub-nav and `<Outlet/>` |
| `web/src/Routes.tsx` | *(modify)* the nested `/config` route |
| `web/src/Sidebar.tsx` | *(modify)* the admin-only Configuration link |
| `web/src/index.css` | *(modify)* the `.config` block |

---

### Task 1: `config.ts` — the client, the tabs, and the regime line

**Files:**
- Create: `web/src/config.ts`
- Test: `web/src/config.test.ts`

**Interfaces:**
- Consumes: `getJson`, `sendJson` from `web/src/api.ts`.
- Produces: `Tab`, `TABS: Tab[]`; `Root`, `Probe`, `Added`, `RootPatch`, `Orphan`, `Declined`, `Walk`, `Derived`, `FinishedScan`, `FailedScan`, `ScanOutcome`, `ScanStatus`, `Removal`, `RemovalAnswer`; `scanFailed(o: ScanOutcome): o is FailedScan`; `listRoots(): Promise<{roots: Root[]}>`, `addRoot(path: string): Promise<Added>`, `patchRoot(id: number, patch: RootPatch): Promise<void|undefined>`, `removalCost(id: number): Promise<RemovalAnswer>`, `removeRoot(id: number): Promise<RemovalAnswer>`, `startScan(id: number): Promise<{root_id: number; status_url: string}>`, `scanStatus(id: number): Promise<ScanStatus>`; `humanInterval(secs: number): string`, `regimeLine(root: Root, probe?: Probe): string`, `SCAN_POLL_MS: number`.

- [ ] **Step 1: Write the failing test**

Create `web/src/config.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import {
  humanInterval,
  regimeLine,
  type Root,
  scanFailed,
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run --project node src/config.test.ts`
Expected: FAIL — `Failed to resolve import "./config"`.

- [ ] **Step 3: Write `config.ts`**

Create `web/src/config.ts`:

```ts
/**
 * The `/api/library/roots*` and scan-control API, and the settings area's own
 * tab list — spec C4 §2 and §3, over the routes that shipped in #105.
 *
 * **Separate from `library.ts`** for the reason that file gives for being
 * separate from `api.ts`: the two share a session and nothing else. Every
 * route here is admin-gated and all but two of them write; nothing in browse
 * does either.
 *
 * **`TABS` lives here rather than in `Config.tsx`** so that the sub-nav which
 * draws it and the index redirect which picks a landing screen read one list.
 * Two lists is a tab with no route behind it, or a redirect into nothing —
 * and neither shows up until somebody clicks.
 *
 * **The regime line is a pure function over a `Root`**, for the reason
 * `library.ts` keeps its chip and sort helpers rather than inlining them: the
 * part worth testing is the sentence, not the element it lands in.
 */
import { getJson, sendJson } from './api'

/** One tab of the settings area. */
export interface Tab {
  /** The path segment under `/config`. */
  path: string
  label: string
}

/**
 * The settings area's tabs, in spec C4's own section order.
 *
 * **Read twice** — by `Config.tsx`'s sub-nav, and by its index redirect, which
 * sends a bare `/config` to `TABS[0]`. So the landing screen is whichever tab
 * leads this list, and PR B prepending the rule editor is what makes the
 * flagship the landing screen rather than a separate decision somewhere else.
 */
export const TABS: Tab[] = [{ path: 'roots', label: 'Roots' }]

/** One `root` row, as `GET /api/library/roots` sends it. */
export interface Root {
  id: number
  path: string
  edition_key: string | null
  default_kind: string | null
  default_language: string | null
  scan_interval_secs: number | null
  /** Whether `<root>/.taiyaku/` is kept — migration 012, off by default. */
  keeps_documents: boolean
  /** `null` until this root has been scanned once. */
  last_scan_at: string | null
  /** `ok`, `refused`, or a failure — `null` alongside `last_scan_at`. */
  last_scan_status: string | null
}

/**
 * What `watch::probe` found — an *experiment*, per spec B §8, not a lookup.
 *
 * **Only `POST /api/library/roots` carries this.** The list route does not,
 * and re-probing every root on every page load would turn a page load into N
 * filesystem experiments. Spec §3.1: a stated gap beats an invented number.
 */
export interface Probe {
  watching: boolean
  watch_reason: string | null
}

/** `POST /api/library/roots`'s answer: the row it minted and the probe it ran. */
export interface Added extends Probe {
  root: Root
  interval_secs: number
}

/**
 * A root's editable columns.
 *
 * **Absent and `null` mean different things**, which is why every field is
 * optional *and* nullable: the server reads an absent key as "leave this
 * column alone" and an explicit `null` as "clear it". `config_api`'s
 * `patch_nullable_text` is the other end of this.
 */
export interface RootPatch {
  edition_key?: string | null
  default_kind?: string | null
  default_language?: string | null
  exclude_globs?: string | null
  scan_interval_secs?: number | null
  keeps_documents?: boolean
}

/** One `DeriveReport::orphan_documents` entry. */
export interface Orphan {
  path: string
  title: string | null
  absorbed_into: { slug: string; title: string } | null
}

/** One `DeriveReport::declined_documents` entry. */
export interface Declined {
  path: string
  title: string | null
}

/** `scan::ScanReport` — what the walk found. */
export interface Walk {
  root: string
  dirs: number
  files: number
  added: number
  updated: number
  gone: number
  /** Spec B §8.1's refusal, or `null` where the scan was applied. */
  refused: string | null
  unreadable_dirs: string[]
}

/**
 * `derive::DeriveReport` — what derivation made of the walk.
 *
 * **All twenty-nine fields, because the server sends all twenty-nine.** The
 * route destructures both reports with no `..` precisely so a field cannot go
 * missing on the way out; a client type that omitted one would undo that at
 * the last step. C4 §3's discipline is that a scan names what it did not do.
 */
export interface Derived {
  series: number
  works: number
  editions: number
  members: number
  unattached_chapter_dirs: number
  unreadable: number
  suppressed: number
  rekeyed: number
  kept_editions: number
  emptied_works: number
  unfolded_works: number
  unfoldable: string[]
  unreadable_hand: string[]
  repaired_documents: number
  unwritable_documents: string[]
  orphan_documents: Orphan[]
  declined_documents: Declined[]
  stale_documents: string[]
  moved_positions: number
  lost_positions: number
  moved_notes: number
  moved_truth_pages: number
  moved_truth_collisions: number
  discarded_rows: number
  series_holding_no_work: number
  merged: number
  unrecognised_kinds: number
  raised: number
  root_removed_mid_derive: number
}

/** A scan that ran to the end. */
export interface FinishedScan {
  scan: Walk
  derive: Derived
  /** The server's own note about `/api/works` being a startup snapshot. */
  note: string
}

/** A scan that failed or panicked — `run_scan`'s other shape. */
export interface FailedScan {
  error: string
}

export type ScanOutcome = FinishedScan | FailedScan

/**
 * Which of the two shapes `report` holds.
 *
 * A type guard rather than a field: the server sends two different objects
 * under one key and neither carries a discriminant, so the narrowing has to
 * be structural. Written once here so no screen invents a second test for it.
 */
export function scanFailed(outcome: ScanOutcome): outcome is FailedScan {
  return 'error' in outcome
}

/** `GET /api/library/roots/{id}/scan`. */
export interface ScanStatus {
  root_id: number
  running: boolean
  /** Present only while the registry holds an entry for this root. */
  elapsed_secs?: number
  /** `null` while running — `ScanProgress` has nothing to report until done. */
  report: ScanOutcome | null
  last_scan_at: string | null
  last_scan_status: string | null
}

/** What removing a root costs. */
export interface Removal {
  works: number
  positions: number
  notes: number
}

/**
 * `DELETE /api/library/roots/{id}`'s answer, for a dry run and a real removal
 * alike — the same body either way, with `dry_run` saying which it was.
 */
export interface RemovalAnswer {
  removed: Removal
  derive: Derived
  dry_run: boolean
}

// --- the client -----------------------------------------------------------

/**
 * `sendJson` resolves `T | undefined` because a 204 has nothing to parse, and
 * `patchRoot` below is this module's one route that answers one. Narrowed
 * here rather than by overloading `sendJson`, for the reason `curation.ts`'s
 * own copy of this helper gives: teaching the shared plumbing that a body
 * argument implies a non-empty reply would be a lie told to every caller.
 */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

export const listRoots = () => getJson<{ roots: Root[] }>('/api/library/roots')

/**
 * Add a root, and get back the three validations' verdict.
 *
 * A refusal throws with the server's own sentence, which names *which* of the
 * three failed — the path does not exist, the server cannot read it, or it
 * overlaps an existing root in one direction or the other. Spec §3.2: the
 * screen renders that message and adds nothing, because a client-side guess
 * at readability is a guess about a process it is not.
 */
export const addRoot = (path: string): Promise<Added> =>
  expectBody(sendJson<Added>('/api/library/roots', 'POST', { path }))

/** Change only the columns `patch` names. Answers 204. */
export const patchRoot = (id: number, patch: RootPatch) =>
  sendJson<void>(`/api/library/roots/${id}`, 'PATCH', patch)

/**
 * What removing root `id` would cost, without removing anything.
 *
 * **`?dry_run=true` spelled exactly.** The server refuses an unknown query
 * key rather than dropping it — `deny_unknown_fields`, added after a
 * whole-branch review found `?dryrun=true` answering 200 and running the
 * real, committed removal.
 */
export const removalCost = (id: number): Promise<RemovalAnswer> =>
  expectBody(
    sendJson<RemovalAnswer>(`/api/library/roots/${id}?dry_run=true`, 'DELETE'),
  )

/** The real thing. C4 §2 calls this the most destructive action in the product. */
export const removeRoot = (id: number): Promise<RemovalAnswer> =>
  expectBody(sendJson<RemovalAnswer>(`/api/library/roots/${id}`, 'DELETE'))

/**
 * Start a scan. Answers 202 and the status URL; a scan already running
 * against this root answers 409 naming the one in flight.
 */
export const startScan = (
  id: number,
): Promise<{ root_id: number; status_url: string }> =>
  expectBody(
    sendJson<{ root_id: number; status_url: string }>(
      `/api/library/roots/${id}/scan`,
      'POST',
    ),
  )

export const scanStatus = (id: number) =>
  getJson<ScanStatus>(`/api/library/roots/${id}/scan`)

/**
 * How often `RootRow` asks a running scan how it is doing.
 *
 * **One second, and it is a guess.** The route reads an in-memory registry
 * entry plus one indexed row, so a second is generous rather than tight; the
 * walk it is watching is measured in tens of seconds at best. Nothing has
 * measured what a full scan of a real root costs — spec §10, and spec B
 * carries the same gap — so this is chosen against the cost of asking rather
 * than against the thing being asked about.
 */
export const SCAN_POLL_MS = 1000

// --- pure helpers ---------------------------------------------------------

/**
 * A scan interval in the coarsest unit that does not lie about it.
 *
 * Rounds *down* rather than showing a fraction: 90 minutes reads `1h`, not
 * `1.5h`. The interval is an operational fact somebody set, and a fractional
 * hour reads as a measurement it is not.
 */
export function humanInterval(secs: number): string {
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h`
  if (secs >= 60) return `${Math.floor(secs / 60)}m`
  return `${secs}s`
}

/**
 * C4 §3's regime line, assembled from what this client actually knows.
 *
 * The spec's own example reads
 * `watching 156 directories · scanning every 24h · last scan 2h ago, 0 changes`.
 * Two of those figures are not available here: the directory count and the
 * change count both belong to a scan report, not to a `root` row, so this
 * draws the three facts the row does carry and the report draws the rest.
 *
 * **`probe` is optional and its absence is stated, not defaulted.** Only
 * `POST /api/library/roots` runs the probe, so a root loaded from the list has
 * no watch state at all — and "watching" would be a claim, while "not
 * watching" would be a different one. Spec §3.1.
 */
export function regimeLine(root: Root, probe?: Probe): string {
  const watch =
    probe === undefined
      ? 'watch state unknown — probed when a root is added'
      : probe.watching
        ? 'watching'
        : probe.watch_reason === null
          ? 'watch probe failed'
          : `watch probe failed: ${probe.watch_reason}`

  const interval =
    root.scan_interval_secs === null
      ? 'no scan interval set'
      : `scanning every ${humanInterval(root.scan_interval_secs)}`

  const last =
    root.last_scan_at === null
      ? 'never scanned'
      : `last scan ${root.last_scan_at} · ${root.last_scan_status ?? 'outcome unrecorded'}`

  return `${watch} · ${interval} · ${last}`
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && pnpm vitest run --project node src/config.test.ts`
Expected: PASS — 12 tests.

- [ ] **Step 5: Typecheck and lint**

Run: `cd web && pnpm typecheck && pnpm lint`
Expected: both clean.

- [ ] **Step 6: Commit**

```bash
git add web/src/config.ts web/src/config.test.ts
git commit -m "config.ts: the roots and scan client, the tab list, and the regime line"
```

---

### Task 2: `ScanReport.tsx` — both reports, whole, confession first

**Files:**
- Create: `web/src/ScanReport.tsx`
- Test: `web/src/ScanReport.test.tsx`

**Interfaces:**
- Consumes: `Derived`, `FinishedScan`, `ScanOutcome`, `Walk`, `scanFailed` from `./config`.
- Produces: `<ScanReport outcome={ScanOutcome} />`.

- [ ] **Step 1: Write the failing test**

Create `web/src/ScanReport.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import type { Derived, FinishedScan, Walk } from './config'
import { ScanReport } from './ScanReport'
import { mount } from './testing/mount'

const walk = (over: Partial<Walk> = {}): Walk => ({
  root: '/srv/manga',
  dirs: 156,
  files: 1858,
  added: 3,
  updated: 0,
  gone: 0,
  refused: null,
  unreadable_dirs: [],
  ...over,
})

const derived = (over: Partial<Derived> = {}): Derived => ({
  series: 130,
  works: 1858,
  editions: 1883,
  members: 1883,
  unattached_chapter_dirs: 0,
  unreadable: 0,
  suppressed: 0,
  rekeyed: 0,
  kept_editions: 0,
  emptied_works: 0,
  unfolded_works: 0,
  unfoldable: [],
  unreadable_hand: [],
  repaired_documents: 0,
  unwritable_documents: [],
  orphan_documents: [],
  declined_documents: [],
  stale_documents: [],
  moved_positions: 0,
  lost_positions: 0,
  moved_notes: 0,
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
  series_holding_no_work: 0,
  merged: 0,
  unrecognised_kinds: 0,
  raised: 0,
  root_removed_mid_derive: 0,
  ...over,
})

const finished = (over: Partial<FinishedScan> = {}): FinishedScan => ({
  scan: walk(),
  derive: derived(),
  note: "this process's own /api/works list is a startup snapshot",
  ...over,
})

describe('ScanReport', () => {
  it('draws an empty unreadable list as zero rather than omitting it', () => {
    // C4 §3's whole discipline. "Nothing unreadable this time" and "this
    // field does not exist in this version" must not look the same, and a
    // missing row is how they stop looking different.
    const host = mount(<ScanReport outcome={finished()} />)
    const row = host.querySelector('[data-field="unreadable"]')
    expect(row).not.toBeNull()
    expect(row?.textContent).toContain('0')
  })

  it('draws every field of both reports', () => {
    // Thirty-seven fields: eight from the walk, twenty-nine from the derive.
    // Counted rather than spot-checked, because the failure this guards is a
    // field quietly dropped on the way out — which is exactly what happened
    // once already on the server side.
    const host = mount(<ScanReport outcome={finished()} />)
    expect(host.querySelectorAll('[data-field]')).toHaveLength(37)
  })

  it('puts what the scan did not do before what it did', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    const fields = [...host.querySelectorAll('[data-field]')].map((el) =>
      el.getAttribute('data-field'),
    )
    expect(fields.indexOf('unreadable')).toBeLessThan(fields.indexOf('works'))
    expect(fields.indexOf('suppressed')).toBeLessThan(fields.indexOf('series'))
  })

  it('names an unreadable format rather than a count alone', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          scan: walk({ unreadable_dirs: ['/srv/manga/broken'] }),
        })}
      />,
    )
    expect(host.textContent).toContain('/srv/manga/broken')
  })

  it("draws spec B's refusal as a refusal, not as a count", () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          scan: walk({
            refused: 'the root held 1858 entries last scan and 0 now',
          }),
        })}
      />,
    )
    const refusal = host.querySelector('[role="alert"]')
    expect(refusal?.textContent).toContain('0 now')
  })

  it('names the work an orphaned document was absorbed into', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            orphan_documents: [
              {
                path: '/srv/manga/.taiyaku/x.library.json',
                title: 'Monster',
                absorbed_into: { slug: 'monster-01', title: 'Monster v1' },
              },
            ],
          }),
        })}
      />,
    )
    expect(host.textContent).toContain('Monster v1')
  })

  it('draws the note about /api/works being a startup snapshot', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    expect(host.textContent).toContain('startup snapshot')
  })

  it('draws a failed scan as the failure it is', () => {
    const host = mount(<ScanReport outcome={{ error: 'the scan panicked' }} />)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'the scan panicked',
    )
    expect(host.querySelectorAll('[data-field]')).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run --project jsdom src/ScanReport.test.tsx`
Expected: FAIL — `Failed to resolve import "./ScanReport"`.

- [ ] **Step 3: Write `ScanReport.tsx`**

Create `web/src/ScanReport.tsx`:

```tsx
/**
 * A scan's two reports, rendered whole — spec C4 §3.
 *
 * **Two sections, because the server already sends two objects.** The walk
 * and the derivation are two different passes over the library, and keeping
 * them apart is what lets this draw "what the walk found" and "what
 * derivation made of it" rather than making a reader work out which of
 * thirty-seven keys came from which pass. `finished_scan_json`'s own doc
 * comment makes the same argument from the other side.
 *
 * **Every field is drawn, including the collections that are empty this
 * time.** The route destructures both structs with no `..` so a field cannot
 * go missing on the way out; undoing that at the last step by hiding an
 * empty row would make "nothing unreadable this time" and "this field does
 * not exist in this version of the API" look identical — exactly the
 * omission C4 §3 writes rules against.
 *
 * **The confession comes first.** A scan that quietly ignored 40 EPUBs and
 * said "done" is the failure this project keeps writing rules against, and
 * burying it under the achievement is the readable version of the same
 * thing. So the fields naming what did *not* happen lead each section and
 * the counts of what did follow them.
 *
 * `data-field` on every row is what the test counts. A count rather than a
 * list of names: the failure being guarded is a field silently dropped, and
 * a test that enumerates the fields it expects has the same hole it is
 * checking for.
 */
import type { Declined, Derived, Orphan, ScanOutcome, Walk } from './config'
import { scanFailed } from './config'

/** One count, with the key the test counts on. */
function Count({ field, label, n }: { field: string; label: string; n: number }) {
  return (
    <div className="count" data-field={field}>
      <dt>{label}</dt>
      <dd>{n}</dd>
    </div>
  )
}

/**
 * One list of paths, drawn as its length and then its members.
 *
 * The length is drawn even when it is zero, which is the whole point: an
 * empty list is a fact this report is making, not a section to skip.
 */
function Paths({
  field,
  label,
  paths,
}: {
  field: string
  label: string
  paths: string[]
}) {
  return (
    <div className="paths" data-field={field}>
      <dt>{label}</dt>
      <dd>
        {paths.length}
        {paths.length > 0 && (
          <ul>
            {paths.map((path) => (
              <li key={path}>
                <code>{path}</code>
              </li>
            ))}
          </ul>
        )}
      </dd>
    </div>
  )
}

/** The orphaned documents, each naming the work that absorbed its own. */
function Orphans({ orphans }: { orphans: Orphan[] }) {
  return (
    <div className="paths" data-field="orphan_documents">
      <dt>Orphaned documents</dt>
      <dd>
        {orphans.length}
        {orphans.length > 0 && (
          <ul>
            {orphans.map((orphan) => (
              <li key={orphan.path}>
                <code>{orphan.path}</code>
                {orphan.title !== null && <> — {orphan.title}</>}
                {orphan.absorbed_into !== null && (
                  <> · absorbed into {orphan.absorbed_into.title}</>
                )}
              </li>
            ))}
          </ul>
        )}
      </dd>
    </div>
  )
}

/** The documents a root no longer keeps — declined, not orphaned. */
function Declines({ declined }: { declined: Declined[] }) {
  return (
    <div className="paths" data-field="declined_documents">
      <dt>Documents this root no longer keeps</dt>
      <dd>
        {declined.length}
        {declined.length > 0 && (
          <ul>
            {declined.map((one) => (
              <li key={one.path}>
                <code>{one.path}</code>
                {one.title !== null && <> — {one.title}</>}
              </li>
            ))}
          </ul>
        )}
      </dd>
    </div>
  )
}

function WalkSection({ walk }: { walk: Walk }) {
  return (
    <section className="walk">
      <h3>What the walk found</h3>
      {walk.refused !== null && (
        <p role="alert" className="refused">
          This scan was refused and nothing was applied: {walk.refused}
        </p>
      )}
      <dl>
        <Paths
          field="unreadable_dirs"
          label="Directories it could not read"
          paths={walk.unreadable_dirs}
        />
        <Count field="dirs" label="Directories" n={walk.dirs} />
        <Count field="files" label="Files" n={walk.files} />
        <Count field="added" label="Added" n={walk.added} />
        <Count field="updated" label="Updated" n={walk.updated} />
        <Count field="gone" label="Gone" n={walk.gone} />
        <div className="count" data-field="root">
          <dt>Root</dt>
          <dd>
            <code>{walk.root}</code>
          </dd>
        </div>
        <div className="count" data-field="refused">
          <dt>Refused</dt>
          <dd>{walk.refused === null ? 'no' : 'yes'}</dd>
        </div>
      </dl>
    </section>
  )
}

function DeriveSection({ derive }: { derive: Derived }) {
  return (
    <section className="derived">
      <h3>What derivation made of it</h3>
      <dl>
        {/* What it did not do, first. */}
        <Count field="unreadable" label="Unreadable" n={derive.unreadable} />
        <Count field="suppressed" label="Suppressed" n={derive.suppressed} />
        <Count
          field="unrecognised_kinds"
          label="Unrecognised kinds"
          n={derive.unrecognised_kinds}
        />
        <Count
          field="unattached_chapter_dirs"
          label="Unattached chapter directories"
          n={derive.unattached_chapter_dirs}
        />
        <Count
          field="lost_positions"
          label="Reading positions lost"
          n={derive.lost_positions}
        />
        <Count
          field="discarded_rows"
          label="Rows discarded"
          n={derive.discarded_rows}
        />
        <Count
          field="series_holding_no_work"
          label="Series holding no work"
          n={derive.series_holding_no_work}
        />
        <Count
          field="root_removed_mid_derive"
          label="Roots removed mid-derive"
          n={derive.root_removed_mid_derive}
        />
        <Count
          field="moved_truth_collisions"
          label="Truth-page collisions"
          n={derive.moved_truth_collisions}
        />
        <Paths
          field="unfoldable"
          label="Could not fold"
          paths={derive.unfoldable}
        />
        <Paths
          field="unreadable_hand"
          label="Unreadable documents"
          paths={derive.unreadable_hand}
        />
        <Paths
          field="unwritable_documents"
          label="Documents it could not write"
          paths={derive.unwritable_documents}
        />
        <Paths
          field="stale_documents"
          label="Stale documents"
          paths={derive.stale_documents}
        />
        <Orphans orphans={derive.orphan_documents} />
        <Declines declined={derive.declined_documents} />

        {/* Then what it did. */}
        <Count field="series" label="Series" n={derive.series} />
        <Count field="works" label="Works" n={derive.works} />
        <Count field="editions" label="Editions" n={derive.editions} />
        <Count field="members" label="Members" n={derive.members} />
        <Count field="merged" label="Merged" n={derive.merged} />
        <Count field="rekeyed" label="Rekeyed" n={derive.rekeyed} />
        <Count
          field="kept_editions"
          label="Editions kept"
          n={derive.kept_editions}
        />
        <Count
          field="emptied_works"
          label="Works emptied"
          n={derive.emptied_works}
        />
        <Count
          field="unfolded_works"
          label="Works unfolded"
          n={derive.unfolded_works}
        />
        <Count
          field="repaired_documents"
          label="Documents repaired"
          n={derive.repaired_documents}
        />
        <Count
          field="moved_positions"
          label="Reading positions moved"
          n={derive.moved_positions}
        />
        <Count field="moved_notes" label="Notes moved" n={derive.moved_notes} />
        <Count
          field="moved_truth_pages"
          label="Truth pages moved"
          n={derive.moved_truth_pages}
        />
        <Count field="raised" label="Proposals raised" n={derive.raised} />
      </dl>
    </section>
  )
}

export function ScanReport({ outcome }: { outcome: ScanOutcome }) {
  if (scanFailed(outcome)) {
    return (
      <div className="scan-report">
        <p role="alert">{outcome.error}</p>
      </div>
    )
  }
  return (
    <div className="scan-report">
      <WalkSection walk={outcome.scan} />
      <DeriveSection derive={outcome.derive} />
      <p className="note">{outcome.note}</p>
    </div>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && pnpm vitest run --project jsdom src/ScanReport.test.tsx`
Expected: PASS — 8 tests. If the field count assertion fails, the number in the test is the truth to fix in the component, not the other way round: count the `data-field` attributes above and make sure all 8 walk fields and all 29 derive fields are present.

- [ ] **Step 5: Commit**

```bash
git add web/src/ScanReport.tsx web/src/ScanReport.test.tsx
git commit -m "ScanReport: both reports whole, what it did not do first"
```

---

### Task 3: `RemoveRoot.tsx` — the counts before the act

**Files:**
- Create: `web/src/RemoveRoot.tsx`
- Test: `web/src/RemoveRoot.test.tsx`

**Interfaces:**
- Consumes: `Root`, `RemovalAnswer`, `removalCost`, `removeRoot` from `./config`.
- Produces: `<RemoveRoot root={Root} onCancel={() => void} onRemoved={() => void} />`.

- [ ] **Step 1: Write the failing test**

Create `web/src/RemoveRoot.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { RemoveRoot } from './RemoveRoot'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
}))

const root: config.Root = {
  id: 3,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
}

const answer = (
  over: Partial<config.Removal> = {},
): config.RemovalAnswer => ({
  removed: { works: 412, positions: 3, notes: 27, ...over },
  derive: {} as config.Derived,
  dry_run: true,
})

async function settle(onRemoved = vi.fn()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RemoveRoot root={root} onCancel={vi.fn()} onRemoved={onRemoved} />,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

function press(host: HTMLElement, label: string) {
  const button = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.includes(label),
  )
  if (!button) throw new Error(`no button matching ${label}`)
  return button
}

beforeEach(() => {
  vi.mocked(config.removalCost).mockResolvedValue(answer())
  vi.mocked(config.removeRoot).mockResolvedValue({
    ...answer(),
    dry_run: false,
  })
})

describe('RemoveRoot', () => {
  it('states all three counts before it acts', async () => {
    // The whole sentence, not three loose digits: `toContain('3')` is
    // satisfied by any string holding a 3 anywhere, including the root's own
    // id in the aria-label, so it would pass against a dialog that never
    // drew the positions count at all.
    expect((await settle()).textContent).toContain(
      'This removes 412 works, 3 in-progress positions and 27 notes.',
    )
  })

  it('draws a zero rather than saying nothing about it', async () => {
    // Spec §3.3: "This removes 0 works" is a different sentence from a dialog
    // that mentions no counts, and the second reads as "counts unavailable".
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ works: 0, positions: 0, notes: 0 }),
    )
    const host = await settle()
    expect(host.textContent).toMatch(/0 works/)
  })

  it('sends no real DELETE until the confirm is pressed', async () => {
    await settle()
    expect(config.removalCost).toHaveBeenCalledWith(3)
    expect(config.removeRoot).not.toHaveBeenCalled()
  })

  it('removes when confirmed, and tells its caller', async () => {
    const onRemoved = vi.fn()
    const host = await settle(onRemoved)
    await act(async () => {
      press(host, 'Remove').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(config.removeRoot).toHaveBeenCalledWith(3)
    expect(onRemoved).toHaveBeenCalled()
  })

  it('draws the server sentence when the dry run is refused', async () => {
    vi.mocked(config.removalCost).mockRejectedValue(
      new Error('/api/library/roots/3: 403 an administrator may do this'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'an administrator may do this',
    )
  })

  it('does not offer a confirm while the counts are unknown', async () => {
    vi.mocked(config.removalCost).mockRejectedValue(new Error('500'))
    const host = await settle()
    expect(
      [...host.querySelectorAll('button')].some((b) =>
        b.textContent?.includes('Remove'),
      ),
    ).toBe(false)
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run --project jsdom src/RemoveRoot.test.tsx`
Expected: FAIL — `Failed to resolve import "./RemoveRoot"`.

- [ ] **Step 3: Write `RemoveRoot.tsx`**

Create `web/src/RemoveRoot.tsx`:

```tsx
/**
 * Removing a root, and what it costs — spec C4 §2.
 *
 * **The most destructive action in the product, and it does not look like
 * one.** Works derive from `scan_entry` + `rule`; dropping a root drops its
 * entries, and progress, notes and read history all hang off works. So this
 * states its counts before it acts, and the real `DELETE` is sent from here
 * and nowhere else.
 *
 * **The dry run is fetched when this mounts, not when the card renders.** A
 * count per root per page load is a derivation-shaped question asked about
 * roots nobody is removing — `removal_cost` runs the same apply-read-rollback
 * machinery the rule preview does, measured at a median of 281 ms on a real
 * library.
 *
 * **The confirm does not exist until the counts do.** A button that removes a
 * root while the sentence above it says nothing is the failure this whole
 * component exists to prevent, and "the dry run failed" is exactly when a
 * person is most likely to press on regardless.
 *
 * **Zero is drawn.** "This removes 0 works, 0 positions and 0 notes" is a
 * different sentence from a dialog that mentions no counts, and the second
 * reads as "counts unavailable" to whoever is about to press the button.
 */
import { useEffect, useState } from 'react'
import { type RemovalAnswer, removalCost, removeRoot, type Root } from './config'

export interface RemoveRootProps {
  root: Root
  onCancel: () => void
  /** Called once the root is actually gone, so the list can reload. */
  onRemoved: () => void
}

export function RemoveRoot({ root, onCancel, onRemoved }: RemoveRootProps) {
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)

  useEffect(() => {
    let cancelled = false
    removalCost(root.id)
      .then((answer) => {
        if (cancelled) return
        setCost(answer)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [root.id])

  const confirm = () => {
    setRemoving(true)
    removeRoot(root.id)
      .then(() => onRemoved())
      .catch((err: unknown) => {
        setRemoving(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <div className="remove-root" role="group" aria-label={`Remove ${root.path}`}>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {cost === undefined ? (
        failure === undefined && <p>Working out what this would remove…</p>
      ) : (
        <>
          <p className="counts">
            This removes {cost.removed.works} works,{' '}
            {cost.removed.positions} in-progress positions and{' '}
            {cost.removed.notes} notes.
          </p>
          <p className="irreversible">
            Nothing here can be undone from this screen. The files on disk are
            untouched; what goes is everything this library derived from them,
            and everything anybody recorded against it.
          </p>
          <button type="button" onClick={confirm} disabled={removing}>
            {removing ? 'Removing…' : `Remove ${root.path}`}
          </button>
        </>
      )}
      <button type="button" onClick={onCancel} disabled={removing}>
        Cancel
      </button>
    </div>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && pnpm vitest run --project jsdom src/RemoveRoot.test.tsx`
Expected: PASS — 6 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/RemoveRoot.tsx web/src/RemoveRoot.test.tsx
git commit -m "RemoveRoot: the counts come first, and the confirm does not exist without them"
```

---

### Task 4: `RootRow.tsx` — one root's card, its fields, and its scan

**Files:**
- Create: `web/src/RootRow.tsx`
- Test: `web/src/RootRow.test.tsx`

**Interfaces:**
- Consumes: `Root`, `RootPatch`, `ScanStatus`, `SCAN_POLL_MS`, `patchRoot`, `regimeLine`, `scanStatus`, `startScan` from `./config`; `<ScanReport>`; `<RemoveRoot>`.
- Produces: `<RootRow root={Root} probe={Probe | undefined} onChanged={() => void} />`.

- [ ] **Step 1: Write the failing test**

Create `web/src/RootRow.test.tsx`:

```tsx
import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { RootRow } from './RootRow'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))

const root = (over: Partial<config.Root> = {}): config.Root => ({
  id: 3,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  ...over,
})

function press(host: HTMLElement, label: string) {
  const button = [...host.querySelectorAll('button')].find((b) =>
    b.textContent?.includes(label),
  )
  if (!button) throw new Error(`no button matching ${label}`)
  return button
}

async function settle(over: Partial<config.Root> = {}, onChanged = vi.fn()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RootRow root={root(over)} probe={undefined} onChanged={onChanged} />,
    )
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  vi.mocked(config.patchRoot).mockResolvedValue(undefined)
  vi.mocked(config.startScan).mockResolvedValue({
    root_id: 3,
    status_url: '/api/library/roots/3/scan',
  })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('RootRow', () => {
  it('says a root has never been scanned rather than drawing a null', async () => {
    const host = await settle()
    expect(host.textContent).toContain('never scanned')
    expect(host.textContent).not.toContain('null')
  })

  it('says the watch state is unknown for a root it did not just add', async () => {
    const host = await settle()
    expect(host.textContent).toContain('watch state unknown')
  })

  it('patches only the field that changed', async () => {
    const host = await settle()
    const box = host.querySelector(
      'input[name="default_language"]',
    ) as HTMLInputElement
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'ja')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new Event('blur', { bubbles: true }))
      await Promise.resolve()
    })
    expect(config.patchRoot).toHaveBeenCalledWith(3, { default_language: 'ja' })
  })

  it('clears a field to null rather than to an empty string', async () => {
    // `patch_nullable_text` reads an absent key as "leave it alone" and an
    // explicit null as "clear it". An empty string is neither, and would set
    // the column to '' — a value nothing in the store ever means.
    const host = await settle()
    const box = host.querySelector(
      'input[name="edition_key"]',
    ) as HTMLInputElement
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, '')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new Event('blur', { bubbles: true }))
      await Promise.resolve()
    })
    expect(config.patchRoot).toHaveBeenCalledWith(3, { edition_key: null })
  })

  it('draws the 409 naming the scan already running', async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 a scan of root 3 (/srv/manga) is already running',
      ),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already running',
    )
  })

  it('polls while a scan runs and stops when it finishes', async () => {
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: true,
        elapsed_secs: 1.2,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValue({
        root_id: 3,
        running: false,
        elapsed_secs: 4.5,
        report: { error: 'the scan panicked' },
        last_scan_at: '2026-09-04T12:00:00Z',
        last_scan_status: 'failed',
      })

    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Scanning')

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('the scan panicked')

    const calls = vi.mocked(config.scanStatus).mock.calls.length
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS * 3)
      await Promise.resolve()
    })
    expect(vi.mocked(config.scanStatus).mock.calls.length).toBe(calls)
  })

  it('reloads the list once a scan finishes', async () => {
    const onChanged = vi.fn()
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      elapsed_secs: 4.5,
      report: { error: 'nope' },
      last_scan_at: '2026-09-04T12:00:00Z',
      last_scan_status: 'failed',
    })
    const host = await settle({}, onChanged)
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(onChanged).toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run --project jsdom src/RootRow.test.tsx`
Expected: FAIL — `Failed to resolve import "./RootRow"`.

- [ ] **Step 3: Write `RootRow.tsx`**

Create `web/src/RootRow.tsx`:

```tsx
/**
 * One root: what it is, what it defaults to, and what its last scan did —
 * spec C4 §2 and §3.
 *
 * **The card owns its own scan poll**, the way `ProposalRow` owns its own
 * evidence fetch: a scan belongs to the root being scanned rather than to the
 * page, and hoisting the poll to the list would make a page with six roots
 * poll six times whether or not any of them is scanning.
 *
 * **A field is patched on blur, and only the field that changed.** An
 * absent key means "leave this column alone" to `patch_nullable_text`, so
 * sending the whole row on every edit would rewrite five columns to answer a
 * question about one. **An emptied box clears the column to `null`, not to
 * `''`** — the schema's nullable columns mean "nothing set", and an empty
 * string is a value nothing in the store ever means.
 *
 * **`exclude_globs` is edited here and does nothing.** O92: the column ships
 * in migration 1, `scan::update_root` patches it, and `scan::scan_root` never
 * reads it, so nothing is ever excluded from a scan. It is drawn because C4
 * §2 lists it among a root's fields and because leaving it out would hide the
 * gap rather than record it; the label says so, in as many words, rather than
 * letting somebody set it and wonder.
 *
 * **Live progress is elapsed seconds and nothing else.** `ScanProgress`
 * carries a start `Instant` and a done flag and no per-file counter; drawing a
 * bar over a number nobody computes would be an invented figure on the screen
 * whose own discipline is that a report names what it did not do.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  patchRoot,
  type Probe,
  regimeLine,
  type Root,
  type RootPatch,
  SCAN_POLL_MS,
  type ScanOutcome,
  scanStatus,
  startScan,
} from './config'
import { RemoveRoot } from './RemoveRoot'
import { ScanReport } from './ScanReport'

export interface RootRowProps {
  root: Root
  /** Only a root added in this session has one. See `config.ts`'s `Probe`. */
  probe?: Probe
  /** The list reloads on a save, a removal, or a finished scan. */
  onChanged: () => void
}

/**
 * The four text columns this card edits.
 *
 * **Each carries its own `patch` builder rather than a `keyof RootPatch` and a
 * computed key.** `save({ [field.name]: value })` types as
 * `{ [x: string]: string | null }`, which is not a `RootPatch` — TypeScript
 * widens a computed key even when the key's type is a union of literals — so
 * the alternative was a cast, and a cast here would silently accept a field
 * name `RootPatch` does not have. Four three-line builders cost less than one
 * `as` that turns a typo into a 400.
 */
interface TextField {
  name: string
  label: string
  from: (root: Root) => string
  patch: (value: string | null) => RootPatch
}

const TEXT_FIELDS: TextField[] = [
  {
    name: 'edition_key',
    label: 'Edition key',
    from: (r) => r.edition_key ?? '',
    patch: (edition_key) => ({ edition_key }),
  },
  {
    name: 'default_kind',
    label: 'Default kind',
    from: (r) => r.default_kind ?? '',
    patch: (default_kind) => ({ default_kind }),
  },
  {
    name: 'default_language',
    label: 'Default language',
    from: (r) => r.default_language ?? '',
    patch: (default_language) => ({ default_language }),
  },
  {
    name: 'exclude_globs',
    label: 'Exclude globs (recorded, not yet applied — O92)',
    // Not on `Root`: the list route does not send it. Editing it writes; there
    // is nothing to read back until the column is read by anything at all.
    from: () => '',
    patch: (exclude_globs) => ({ exclude_globs }),
  },
]

export function RootRow({ root, probe, onChanged }: RootRowProps) {
  const [failure, setFailure] = useState<string | undefined>()
  const [confirming, setConfirming] = useState(false)
  const [running, setRunning] = useState(false)
  const [elapsed, setElapsed] = useState<number | undefined>()
  const [report, setReport] = useState<ScanOutcome | null>(null)

  // `onChanged` is called from inside the poll's interval callback, so it is
  // held in a ref rather than named as a dependency: a parent that hands over
  // a fresh closure each render would otherwise tear down and restart the
  // interval on every one of its own renders, which is a poll that never
  // fires at its own cadence.
  const changed = useRef(onChanged)
  changed.current = onChanged

  const save = (patch: RootPatch) => {
    patchRoot(root.id, patch)
      .then(() => {
        setFailure(undefined)
        onChanged()
      })
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  const begin = useCallback(() => {
    setFailure(undefined)
    setReport(null)
    startScan(root.id)
      .then(() => {
        setRunning(true)
      })
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }, [root.id])

  useEffect(() => {
    if (!running) return
    let cancelled = false
    const timer = setInterval(() => {
      scanStatus(root.id)
        .then((status) => {
          if (cancelled) return
          setElapsed(status.elapsed_secs)
          if (status.running) return
          setRunning(false)
          setReport(status.report)
          changed.current()
        })
        .catch((err: unknown) => {
          if (cancelled) return
          setRunning(false)
          setFailure(err instanceof Error ? err.message : String(err))
        })
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [running, root.id])

  return (
    <li className="root-row">
      <h3>
        <code>{root.path}</code>
      </h3>
      <p className="regime">{regimeLine(root, probe)}</p>
      {failure !== undefined && <p role="alert">{failure}</p>}

      <div className="fields">
        {TEXT_FIELDS.map((field) => (
          <label key={field.name}>
            <span>{field.label}</span>
            <input
              type="text"
              name={field.name}
              defaultValue={field.from(root)}
              onBlur={(e) => {
                const next = e.target.value
                const before = field.from(root)
                if (next === before) return
                // Empty clears the column. See this file's own doc comment.
                save(field.patch(next === '' ? null : next))
              }}
            />
          </label>
        ))}
        <label>
          <span>Scan interval (seconds)</span>
          <input
            type="number"
            name="scan_interval_secs"
            defaultValue={root.scan_interval_secs ?? ''}
            onBlur={(e) => {
              const raw = e.target.value
              const next = raw === '' ? null : Number(raw)
              if (next === root.scan_interval_secs) return
              if (next !== null && Number.isNaN(next)) return
              save({ scan_interval_secs: next })
            }}
          />
        </label>
        <label>
          <span>Keeps documents under .taiyaku/</span>
          <input
            type="checkbox"
            name="keeps_documents"
            defaultChecked={root.keeps_documents}
            onChange={(e) => save({ keeps_documents: e.target.checked })}
          />
        </label>
      </div>

      <div className="actions">
        <button type="button" onClick={begin} disabled={running}>
          Scan now
        </button>
        <button
          type="button"
          onClick={() => setConfirming(true)}
          disabled={running || confirming}
        >
          Remove…
        </button>
      </div>

      {running && (
        <p className="scanning">
          Scanning…{elapsed === undefined ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}
      {report !== null && <ScanReport outcome={report} />}
      {confirming && (
        <RemoveRoot
          root={root}
          onCancel={() => setConfirming(false)}
          onRemoved={() => {
            setConfirming(false)
            onChanged()
          }}
        />
      )}
    </li>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && pnpm vitest run --project jsdom src/RootRow.test.tsx`
Expected: PASS — 7 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/RootRow.tsx web/src/RootRow.test.tsx
git commit -m "RootRow: the regime line, the fields, and a scan that polls itself"
```

---

### Task 5: `Roots.tsx` — the list and the add form

**Files:**
- Create: `web/src/Roots.tsx`
- Test: `web/src/Roots.test.tsx`

**Interfaces:**
- Consumes: `Added`, `Root`, `addRoot`, `humanInterval`, `listRoots` from `./config`; `<RootRow>`.
- Produces: `<Roots />`, a screen taking no props — mounted by `Config.tsx`'s route.

- [ ] **Step 1: Write the failing test**

Create `web/src/Roots.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { Roots } from './Roots'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  addRoot: vi.fn(),
  listRoots: vi.fn(),
  patchRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))

const root = (over: Partial<config.Root> = {}): config.Root => ({
  id: 3,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  ...over,
})

async function settle() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Roots />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

async function addPath(host: HTMLElement, path: string) {
  const box = host.querySelector('input[name="path"]') as HTMLInputElement
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, path)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
  const form = host.querySelector('form') as HTMLFormElement
  await act(async () => {
    form.dispatchEvent(
      new Event('submit', { bubbles: true, cancelable: true }),
    )
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [root()] })
})

describe('Roots', () => {
  it('lists the roots the server sent', async () => {
    const host = await settle()
    expect(host.textContent).toContain('/srv/manga')
  })

  it('draws the server refusal for a nested root, in both directions', async () => {
    // Spec §3.2: the screen renders the server's own sentence and adds
    // nothing. A client that guessed at nesting would be a second
    // implementation of a check whose whole point is that it runs as the
    // server process.
    vi.mocked(config.addRoot).mockRejectedValue(
      new Error(
        '/api/library/roots: 400 /srv/manga/sub sits inside the existing root /srv/manga',
      ),
    )
    const host = await settle()
    await addPath(host, '/srv/manga/sub')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'sits inside the existing root',
    )
  })

  it('draws the refusal for a path the server cannot read', async () => {
    vi.mocked(config.addRoot).mockRejectedValue(
      new Error('/api/library/roots: 400 /mnt/remote cannot be read'),
    )
    const host = await settle()
    await addPath(host, '/mnt/remote')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'cannot be read',
    )
  })

  it('shows the watch probe for a root it just added', async () => {
    // The third validation, and the only one that produces something on a
    // *successful* add. C4 §2 wants the regime known before the first scan.
    const added: config.Added = {
      root: root({ id: 9, path: '/mnt/remote/manga' }),
      watching: false,
      watch_reason: 'inotify limit reached',
      interval_secs: 900,
    }
    vi.mocked(config.addRoot).mockResolvedValue(added)
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [root(), added.root],
    })
    const host = await settle()
    await addPath(host, '/mnt/remote/manga')
    expect(host.textContent).toContain('inotify limit reached')
    expect(host.textContent).toContain('15m')
  })

  it('reloads the list after an add', async () => {
    vi.mocked(config.addRoot).mockResolvedValue({
      root: root({ id: 9 }),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    const host = await settle()
    const before = vi.mocked(config.listRoots).mock.calls.length
    await addPath(host, '/srv/other')
    expect(vi.mocked(config.listRoots).mock.calls.length).toBeGreaterThan(
      before,
    )
  })

  it('draws the 403 sentence rather than a bare status', async () => {
    vi.mocked(config.listRoots).mockRejectedValue(
      new Error(
        '/api/library/roots: 403 configuring the library is an administrator’s',
      ),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'an administrator',
    )
  })

  it('says a library with no roots has none, rather than drawing nothing', async () => {
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
    const host = await settle()
    expect(host.textContent).toContain('No roots')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run --project jsdom src/Roots.test.tsx`
Expected: FAIL — `Failed to resolve import "./Roots"`.

- [ ] **Step 3: Write `Roots.tsx`**

Create `web/src/Roots.tsx`:

```tsx
/**
 * The roots screen — spec C4 §2 and §3.
 *
 * **The list owns which roots there are; each card owns what it is doing.**
 * `Proposals`/`ProposalRow`'s split and its stated reason: the evidence a row
 * fetches belongs to the thing being judged, and here that is the dry-run
 * counts and the scan poll.
 *
 * **The probe from an add is kept, keyed by root id.** `watch::probe` runs on
 * `POST` and nowhere else, so a root added in this session has a watch state
 * and one loaded from the list does not — see `config.ts`'s `Probe`. Holding
 * the answer here rather than dropping it is the difference between showing
 * the regime C4 §2 asks for "before the first scan" and showing it never.
 *
 * **No client-side path validation.** All three of C4 §2's checks run in the
 * server process, and the second one — is this path readable *by the server*
 * — is a question a browser cannot ask at all. The refusal's own sentence
 * names which check failed; this screen draws it.
 */
import { useCallback, useEffect, useState } from 'react'
import {
  type Added,
  addRoot,
  humanInterval,
  listRoots,
  type Probe,
  type Root,
} from './config'
import { RootRow } from './RootRow'

export function Roots() {
  const [roots, setRoots] = useState<Root[] | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [path, setPath] = useState('')
  const [adding, setAdding] = useState(false)
  const [probes, setProbes] = useState<Record<number, Probe>>({})
  const [justAdded, setJustAdded] = useState<Added | undefined>()

  const reload = useCallback(() => {
    listRoots()
      .then((body) => {
        setRoots(body.roots)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (path === '') return
    setAdding(true)
    addRoot(path)
      .then((added) => {
        setAdding(false)
        setPath('')
        setFailure(undefined)
        setJustAdded(added)
        setProbes((held) => ({
          ...held,
          [added.root.id]: {
            watching: added.watching,
            watch_reason: added.watch_reason,
          },
        }))
        reload()
      })
      .catch((err: unknown) => {
        setAdding(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <section className="roots">
      <h2>Roots</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}

      {justAdded !== undefined && (
        <p className="probe">
          {justAdded.root.path}:{' '}
          {justAdded.watching
            ? 'the watcher is running'
            : `the watch probe failed${
                justAdded.watch_reason === null
                  ? ''
                  : `: ${justAdded.watch_reason}`
              }`}
          , so it will be scanned every{' '}
          {humanInterval(justAdded.interval_secs)}.
        </p>
      )}

      {roots !== undefined &&
        (roots.length === 0 ? (
          <p className="empty">
            No roots yet. Add the directory your library lives in below; the
            server checks that it can read it and that it does not overlap one
            you already have.
          </p>
        ) : (
          <ul className="root-list">
            {roots.map((root) => (
              <RootRow
                key={root.id}
                root={root}
                probe={probes[root.id]}
                onChanged={reload}
              />
            ))}
          </ul>
        ))}

      <form onSubmit={submit} className="add-root">
        <label>
          <span>Add a root</span>
          <input
            type="text"
            name="path"
            value={path}
            placeholder="/srv/manga"
            onChange={(e) => setPath(e.target.value)}
          />
        </label>
        <button type="submit" disabled={adding || path === ''}>
          {adding ? 'Checking…' : 'Add'}
        </button>
      </form>
    </section>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && pnpm vitest run --project jsdom src/Roots.test.tsx`
Expected: PASS — 7 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/Roots.tsx web/src/Roots.test.tsx
git commit -m "Roots: the list, the add form, and the probe an add hands back"
```

---

### Task 6: `Config.tsx`, the route, and the sidebar link

**Files:**
- Create: `web/src/Config.tsx`
- Test: `web/src/Config.test.tsx`
- Modify: `web/src/Routes.tsx`
- Modify: `web/src/Sidebar.tsx`
- Modify: `web/src/Sidebar.test.tsx`

**Interfaces:**
- Consumes: `TABS` from `./config`; `Outlet`, `NavLink`, `Navigate` from `react-router`.
- Produces: `<Config />`, mounted at `/config` with an index redirect to `TABS[0].path` and one child route per tab.

- [ ] **Step 1: Write the failing test**

Create `web/src/Config.test.tsx`:

```tsx
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { Config } from './Config'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
}))

async function at(entry: string) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <Routes>
          <Route path="/config/*" element={<Config />} />
        </Routes>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

beforeEach(() => {
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
})

describe('Config', () => {
  it('draws one sub-nav link per tab', async () => {
    const host = await at('/config/roots')
    const links = host.querySelectorAll('.config-nav a')
    expect(links).toHaveLength(config.TABS.length)
  })

  it('sends a bare /config to the leading tab', async () => {
    // The index redirect reads `TABS[0]`, so this cannot drift from the
    // sub-nav — one list, read twice. Spec §2.
    const host = await at('/config')
    expect(host.textContent).toContain('Roots')
    expect(host.querySelector('.roots')).not.toBeNull()
  })

  it('marks the current tab', async () => {
    const host = await at('/config/roots')
    expect(host.querySelector('.config-nav a[aria-current="page"]')).not.toBeNull()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run --project jsdom src/Config.test.tsx`
Expected: FAIL — `Failed to resolve import "./Config"`.

- [ ] **Step 3: Write `Config.tsx`**

Create `web/src/Config.tsx`:

```tsx
/**
 * The settings area — spec C4, and the build design's §2.
 *
 * **One sidebar entry and a sub-nav, not four sidebar entries.** C4's opening
 * line settles the shape: configuration is "touched rarely and from a settings
 * area", so the always-visible chrome grows by one. Four entries would also
 * have put a second "Roots" in a sidebar that already has one meaning
 * something else — a browse filter, not an administrative screen.
 *
 * **No admin guard here, deliberately.** `Routes.tsx` puts none on
 * `/proposals` either, and `api.ts`'s `getJson` was changed during C3
 * precisely so a non-admin who types that URL is shown *the server's own
 * sentence* rather than a bare status. A `<RequireAdmin>` wrapper would put a
 * second opinion about who may administer into the client, months after O93
 * closed by making `session::administering_or_forbidden` the only function in
 * `taiyaku-serve` that holds one. A client that draws a screen it should not
 * is cosmetic; a client that *decides* is the shape O93 was.
 *
 * **The routes are declared here rather than in `Routes.tsx`.** `TABS` is the
 * list, and a tab whose route lived in another file could exist in the sub-nav
 * with nothing behind it — which is exactly the drift keeping the list in one
 * place is for.
 */
import { Navigate, NavLink, Route, Routes } from 'react-router'
import { TABS } from './config'
import { Roots } from './Roots'

/** Which component draws each tab. Keyed by `Tab.path`. */
const SCREENS: Record<string, React.ReactNode> = {
  roots: <Roots />,
}

export function Config() {
  return (
    <div className="config">
      <nav className="config-nav" aria-label="Configuration">
        <ul>
          {TABS.map((tab) => (
            <li key={tab.path}>
              <NavLink to={`/config/${tab.path}`}>{tab.label}</NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <Routes>
        <Route
          index
          element={<Navigate to={`/config/${TABS[0].path}`} replace />}
        />
        {TABS.map((tab) => (
          <Route key={tab.path} path={tab.path} element={SCREENS[tab.path]} />
        ))}
      </Routes>
    </div>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && pnpm vitest run --project jsdom src/Config.test.tsx`
Expected: PASS — 3 tests.

- [ ] **Step 5: Wire the route into `Routes.tsx`**

In `web/src/Routes.tsx`, add the import beside the others:

```tsx
import { Config } from './Config'
```

and add this route immediately before the `/read/:work` route:

```tsx
      {/*
        A splat, because `Config` declares its own child routes from `TABS` —
        see that file's own doc comment for why the list and the routes it
        implies live together. Every other route here is a leaf.
      */}
      <Route
        path="/config/*"
        element={
          <Shell>
            <Config />
          </Shell>
        }
      />
```

- [ ] **Step 6: Add the sidebar link**

In `web/src/Sidebar.tsx`, inside `<ul className="sidebar-primary">`, after the Proposals entry:

```tsx
        {admin && (
          <li>
            <Link to="/config">Configuration</Link>
          </li>
        )}
```

And extend this file's doc comment with a paragraph:

```
 * **`admin` now gates two entries, and the second is spec C4's.** The
 * settings area is admin-only for the same reason the proposals inbox is, and
 * takes the same prop rather than a second one: what an account may *see* is
 * the server's to enforce — every `/api/library/{rules,roots,users,backups}`
 * route answers a reader 403 — and this chooses only what to draw. A
 * non-admin who types `/config` reaches it and is shown the server's own
 * sentence, which is exactly what `/proposals` already does.
```

- [ ] **Step 7: Add the sidebar test**

Append to `web/src/Sidebar.test.tsx`:

```tsx
it('draws the configuration link for an admin and not for a reader', () => {
  const withAdmin = mount(
    <MemoryRouter>
      <Sidebar roots={[]} showRoots={false} admin />
    </MemoryRouter>,
  )
  expect(withAdmin.textContent).toContain('Configuration')

  const withoutAdmin = mount(
    <MemoryRouter>
      <Sidebar roots={[]} showRoots={false} />
    </MemoryRouter>,
  )
  expect(withoutAdmin.textContent).not.toContain('Configuration')
})
```

Check the existing imports at the top of that file first; add `MemoryRouter` from `react-router` and `mount` from `./testing/mount` only if they are not already there.

- [ ] **Step 8: Run every jsdom test**

Run: `cd web && pnpm vitest run --project jsdom`
Expected: PASS, including the pre-existing `Sidebar.test.tsx` and `Shell.test.tsx` suites.

- [ ] **Step 9: Commit**

```bash
git add web/src/Config.tsx web/src/Config.test.tsx web/src/Routes.tsx web/src/Sidebar.tsx web/src/Sidebar.test.tsx
git commit -m "the settings area: /config, its sub-nav, and one sidebar entry"
```

---

### Task 7: styling, the docs, and the gate

**Files:**
- Modify: `web/src/index.css`
- Modify: `docs/status.md`
- Modify: `README.md`
- Modify: `web/vite.config.ts` (thresholds, only if they move)
- Modify: `scripts/coverage-floor.mjs` (floors and file count)

- [ ] **Step 1: Style the area**

Append to `web/src/index.css`, following the `.proposal-detail` block's own conventions — system colours throughout, no hardcoded hex (`Login.tsx` shipped a `#111` on this app's front door and `index.css.test.ts` exists because of it):

```css
/* Spec C4's settings area. System colours, per this file's own rule. */
.config-nav ul {
  display: flex;
  gap: 1rem;
  list-style: none;
  margin: 0 0 1rem;
  padding: 0 0 0.5rem;
  border-bottom: 1px solid color-mix(in srgb, currentColor 25%, transparent);
}

.config-nav a[aria-current='page'] {
  font-weight: 600;
  text-decoration: underline;
}

.roots .root-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.root-row {
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  border-radius: 4px;
  margin-bottom: 1rem;
  padding: 1rem;
}

.root-row .regime {
  opacity: 0.8;
}

.root-row .fields {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  gap: 0.5rem;
}

.root-row .fields label {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.remove-root {
  border: 2px solid color-mix(in srgb, currentColor 50%, transparent);
  border-radius: 4px;
  margin-top: 1rem;
  padding: 1rem;
}

.remove-root .counts {
  font-weight: 600;
}

.scan-report dl {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
  gap: 0.25rem 1rem;
  margin: 0;
}

.scan-report .count,
.scan-report .paths {
  display: flex;
  gap: 0.5rem;
}

.scan-report dt {
  opacity: 0.8;
}

.scan-report dd {
  margin: 0;
}

.scan-report .note {
  font-size: 0.9em;
  opacity: 0.7;
}
```

- [ ] **Step 2: Run the whole web suite and read the coverage numbers**

Run: `cd web && pnpm test:coverage`
Expected: PASS. Read `web/coverage/coverage-summary.json`, not the text table — the table omits every file at 100% on all four metrics.

- [ ] **Step 3: Update the coverage floors**

Run: `node scripts/coverage-floor.mjs` and read what it reports. Update the per-file floors and the minimum file count in `scripts/coverage-floor.mjs` to the measured values, and the aggregate thresholds in `web/vite.config.ts` only if a measured figure has fallen below one. Keep each comment's `// measured N, was M when set` form.

**The number to move when a threshold fires is the coverage, not the bar** — that rule is written into `vite.config.ts` and holds here.

- [ ] **Step 4: Update `docs/status.md`**

Five edits, and the "open items ritual" sweep on the way in and the way out:

```sh
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -nE '[0-9]{3,4} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

1. The test count, **at both of its sites** — "Five crates, N tests" and "Nine of the N are standing checks" two lines later. The JUnit report from the gate is the number.
2. The `web/` row's prose: configuring a library is no longer CLI-only.
3. `taiyaku-serve`'s route prose is **unchanged** by this PR — no route is added here. PR B is what changes it.
4. **Open one item**: `Account.tsx` mounts from inside the reader, so the users screen's "change your own password there" (PR C) and any other pointer to the account panel names a screen no URL reaches. Opening one item means five edits of its own: the total count word at the head of the section, the "leaving N" sentence, a table row, a prose bullet, and the "What remains" line — whose count word **wraps across a line break**, so grep the sentence opener and read the next line.
5. Nothing closes. O92 is *touched* by this PR — `exclude_globs` is now editable from a screen and still read by nothing — so its prose bullet gains a sentence saying the editor reached it, and its checkbox stays `[ ]`.

- [ ] **Step 5: Update `README.md`**

Configuring a library stops being a CLI-only activity: roots can be added, edited, removed and scanned from `/config/roots`. Find the section that says otherwise and correct it in place.

- [ ] **Step 6: Run the gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: exits 0 and prints its `all green` line. **Do not pipe this through `tail` or append `; echo`** — either masks the exit status. Quote the `all green` line verbatim as the evidence that it passed.

- [ ] **Step 7: Verify against the real app**

Start the server against a scratch copy of the real library and drive the screen:

```bash
cp -a /home/davey/**temp**/manga-test /tmp/claude-1000/-home-davey-projects-taiyaku/538ea892-d20d-45ca-972a-c9bb1a5ce7c0/scratchpad/roots-check
```

Then run `taiyaku serve` against a library file under the scratchpad, `pnpm dev` in `web/`, and check five things by hand:

1. `/config` redirects to `/config/roots`.
2. A root with no `last_scan_at` reads "never scanned", not `null`.
3. Adding a nested root shows the server's own sentence.
4. **A real scan runs and its report renders** — this is the first scan this screen has ever driven, and **its wall-clock time is a measurement worth recording in `docs/notes.md`**: spec §10 names it as the gap both spec B and the server half carry, and it is what C4 §3's interval defaults should be chosen from.
5. The removal dialog's counts against a root that actually holds works.

**Isolate the writes and prove it afterwards** — the scratch copy above is the isolation; `git status` in the real library's own tree, and a diff of the scratch directory, are the proof.

- [ ] **Step 8: Commit**

```bash
git add web/src/index.css docs/status.md README.md scripts/coverage-floor.mjs web/vite.config.ts docs/notes.md
git commit -m "styling, the coverage floors, and what this PR invalidates in the docs"
```

- [ ] **Step 9: Open the PR**

Use the **gitea** MCP, not `gh` — `gh` is authenticated against the wrong host. **Never hard-wrap the PR body**: the body *is* the squash commit message, and the remote is squash-only, so the branch becomes exactly one commit on main.

---

## Self-Review

**Spec coverage:**

| Spec § | Task |
| --- | --- |
| §2 the settings area, `TABS`, no admin guard | 1 (TABS), 6 (area, route, sidebar) |
| §3.1 the regime line and the probe gap | 1 (`regimeLine`), 4 (drawn), 5 (probe kept) |
| §3.2 the three validations surfaced | 5 |
| §3.3 removal states its counts, zero drawn | 3 |
| §3.4 the scan, the poll, the report's discipline | 1 (`SCAN_POLL_MS`), 2 (the report), 4 (trigger and poll) |
| §3.5 the file list | all |
| §7 the tests | every task |
| §9 what this invalidates | 7 |
| §10 the unmeasured full scan | 7 step 7 |

**Type consistency:** `Root`, `Probe`, `Added`, `RootPatch`, `Removal`, `RemovalAnswer`, `Walk`, `Derived`, `Orphan`, `Declined`, `FinishedScan`, `FailedScan`, `ScanOutcome`, `ScanStatus` are defined once in Task 1 and imported by name everywhere after. `onChanged` is the reload callback in both `RootRow` and `Roots`; `onRemoved`/`onCancel` are `RemoveRoot`'s alone.

**Known gap, deliberate:** `exclude_globs` is not on `Root` because `GET /api/library/roots` does not send it — `root_json` omits it. Task 4 draws an empty box for it and the label says the column is recorded and not yet applied (O92). It is write-only from this screen until either the list route carries it or O92 closes.

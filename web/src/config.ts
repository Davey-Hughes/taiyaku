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
export const TABS: Tab[] = [
  { path: 'rules', label: 'Rules' },
  { path: 'roots', label: 'Roots' },
]

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
 * measured what a full scan of a real root costs — the build design's §10, and
 * spec B carries the same gap — so this is chosen against the cost of asking
 * rather than against the thing being asked about.
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
 * draws the three facts the row does carry and `ScanReport` draws the rest.
 *
 * **`probe` is optional and its absence is stated, not defaulted.** Only
 * `POST /api/library/roots` runs the probe, so a root loaded from the list has
 * no watch state at all — and "watching" would be a claim, while "not
 * watching" would be a different one.
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

/**
 * The HTTP status inside one of this module's thrown messages, if it carries
 * one.
 *
 * `api.ts` throws `` `${path}: ${status} ${body}` ``, and a caller that needs
 * to act on the status — the roots screen treats a 409 from `startScan` as
 * proof a scan is running — was matching `message.includes('409')`, which is
 * satisfied by a 409 *anywhere*: removing root 409 from another tab produced
 * a 404 whose body reads `there is no root 409`, and the screen entered a
 * fake running state on the strength of the id. Anchored to the start, where
 * the path (which never contains a space) is followed by `: ` and the status.
 * Whole-branch review pass 4, 2026-09-04.
 */
export function statusOf(message: string): number | undefined {
  const found = /^\S+: (\d{3})\b/.exec(message)
  return found === null ? undefined : Number(found[1])
}

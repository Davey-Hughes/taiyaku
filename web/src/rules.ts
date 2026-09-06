/**
 * The `/api/library/rules*` API and the rule editor's pure helpers — spec C4
 * §1 and §6, over the routes that shipped in #105 and PR B's own `PUT`.
 *
 * **Separate from `config.ts`** for the reason `config.ts` gives for being
 * separate from `library.ts`: the two share a session and nothing else. The
 * one thing they do share is `Root`, which this module reads to convert a
 * root id to the path an export document has to carry instead.
 *
 * **The `root_path` ↔ `root_id` conversion is one pair of functions used in
 * both directions**, so an export and an import cannot disagree about the
 * mapping — the build design's §5.6. `POST /api/library/rules/import` takes a
 * bare array of documents that name a root by *path*, because a document
 * carried between libraries cannot mean anything by a bare number.
 *
 * **Nothing here validates a pattern.** A regex that compiles in JS is not the
 * regex the store compiles, and the server owns the answer — the build
 * design's §8. What a bad pattern produces is a `complaints` entry from the
 * save, which is the only route that reports one.
 */
import { getJson, sendJson } from './api'
import type { Derived, Root } from './config'

/** The four values `rule.kind` holds — `rules.rs`'s own `FromStr`. */
export const RULE_KINDS = [
  'prefix',
  'regex',
  'builtin-volume',
  'builtin-chapter',
] as const

/** Which of a `rel_path`'s three derived strings a rule is tested against. */
export const MATCH_ONS = ['path', 'name', 'parent'] as const

/**
 * Where the editor says a preview is getting expensive.
 *
 * **Forty, and it is a guess.** The overrides term is quadratic in the number
 * of proposed rules — measured in a debug build on a one-work fixture: 10
 * extra rules 47 ms, 40 → 401 ms, 80 → 1.34 s, 160 → 5.01 s. This is anchored
 * to that curve's knee, not to a measurement of this screen. The preview route
 * itself refuses past 200.
 */
export const PREVIEW_EXPENSIVE_ABOVE = 40

/** One `rule` row, as `GET /api/library/rules` sends it. */
export interface Rule {
  /** `0` is the store's own sentinel for a rule the editor has not saved yet. */
  id: number
  ordinal: number
  enabled: boolean
  /** `null` scopes the rule to every root. */
  root_id: number | null
  kind: string
  pattern: string
  match_on: string
  set_series: string | null
  set_edition_key: string | null
  set_kind: string | null
  set_language: string | null
  note: string | null
}

/** What a proposed list would change — the Impact tab. */
export interface Impact {
  series_before: number
  series_after: number
  series_merged: number
  works_rehomed: number
  works_orphaned: number
  /**
   * Proposals a derivation would put back in front of someone. **Not
   * `proposals_resolved`, which is not reportable** — a derivation reaches one
   * proposal writer and it only ever raises or reopens. The build design's §5.5.
   */
  proposals_reopened: number
  proposals_raised: number
}

/** Where one member sits. */
export interface Placement {
  series: string
  work: string
  edition: string
}

/**
 * Which rule decided one field — **a position in the request's own `rules`
 * array, never a rule id.** The store holds this boundary deliberately: an
 * unsaved rule carries `id: 0`, which is indistinguishable from a second
 * unsaved rule and from the root-fallback sentinel. Render the rule at
 * `index`, never `rules[index].id`.
 */
export type Decided = { kind: 'rule'; index: number } | { kind: 'root_default' }

/** One `scan_entry` the selected rule's scope reaches. */
export interface Row {
  rel_path: string
  root_id: number
  before: Placement | null
  after: Placement | null
  /** `false` is a near-miss, which is the point of the table — not an absence. */
  matched: boolean
  decided_by: Record<string, Decided>
}

/**
 * One field where a later enabled rule replaces an earlier one's different
 * answer.
 *
 * **An observation, not a complaint — O95.** This was `Conflict` until
 * 2026-09-05, and the word made the editor offer a reorder as the fix: on the
 * shipped seeded defaults, rule 4 overriding rule 1's series is what folds a
 * colour edition into its main series, and the reorder is the one thing that
 * breaks it. The store finds exactly what it always found; only the noun and
 * what the screen does with it changed.
 *
 * `first` and `wins` are positions in the request's `rules` array, the same
 * boundary `Decided` holds. `others` counts the disagreements `first` does not
 * name, so a chain of three or more says so rather than implying it is the
 * whole story.
 *
 * **`first_value` and `wins_value` describe `rel_path` alone**, not all
 * `examples` of it — which earlier rules ran, and so what they left behind, is
 * a per-file fact. They are what makes the row a judgement someone can
 * actually make: naming the two rules says *that* they disagreed, and only the
 * values say what about. `null` is a value here — a rule clearing what an
 * earlier one set — and a number for `volume` or `chapter`.
 */
export interface Override {
  field: string
  rel_path: string
  first: number
  wins: number
  first_value: string | number | null
  wins_value: string | number | null
  others: number
  examples: number
}

/** `POST /api/library/rules/preview`'s answer. */
export interface Preview {
  impact: Impact
  rows: Row[]
  /** Whether more rows existed than the route's 500-row cap. Drawn, not swallowed. */
  rows_truncated: boolean
  overrides: Override[]
}

/**
 * `PUT /api/library/rules`'s answer.
 *
 * **`derive`, whole — whole-branch review, I5/config_api's own
 * `derive_report_json`.** `impact` describes the database's own structure
 * and nothing past it; a save that could not write a hand document (a
 * read-only root, `hand::save`'s own `persisted: false` path) used to answer
 * with a rehome count and silence about it. Carried as the same `Derived`
 * shape `ScanReport.tsx` already renders, not copied field by field, so this
 * type cannot drift from that one the moment a field is added to either.
 */
export interface Saved {
  impact: Impact
  rules: Rule[]
  reopened_proposals: number[]
  complaints: string[]
  derive: Derived
}

/**
 * One rule as the export document carries it — **`root_path`, not `root_id`,
 * and no `id` at all.**
 */
export interface RuleDoc {
  ordinal: number
  enabled: boolean
  root_path: string | null
  kind: string
  pattern: string
  match_on: string
  set_edition_key: string | null
  set_kind: string | null
  set_language: string | null
  set_series: string | null
  note: string | null
}

// --- the client -----------------------------------------------------------

/** `config.ts`'s own helper, for the reason its doc comment gives. */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

export const listRules = () =>
  getJson<{ rules: Rule[]; complaints: string[] }>('/api/library/rules')

/**
 * What the whole proposed list would do, plus which pairs of it disagree.
 *
 * **`selected` is omitted rather than sent as null when nothing is open.**
 * The route reads both the same way, but `preview_with_rows` does no
 * row-building work at all for `None` — no `scan_entry` walk, no second
 * `Engine` — and a key present is a key somebody later reads as intent.
 */
export const previewRules = (rules: Rule[], selected?: number) =>
  expectBody(
    sendJson<Preview>('/api/library/rules/preview', 'POST', {
      rules,
      ...(selected === undefined ? {} : { selected }),
    }),
  )

/**
 * Save: replace the rule set with `rules` **and derive over the library**.
 *
 * The only route in this product that applies a rule. Not `importRules`, which
 * mints fresh ids by design and so reopens every accepted proposal whose rule
 * it replaces — correct for a foreign document, wrong for a save.
 */
export const saveRules = (rules: Rule[]) =>
  expectBody(sendJson<Saved>('/api/library/rules', 'PUT', { rules }))

/**
 * Replace the whole set from an export document. **Destructive**: fresh ids,
 * so every accepted proposal whose rule this replaces is reopened, and the
 * answer names them.
 */
export const importRules = (doc: string) =>
  expectBody(
    sendJson<{
      rules: Rule[]
      complaints: string[]
      reopened_proposals: number[]
    }>('/api/library/rules/import', 'POST', JSON.parse(doc) as unknown),
  )

// --- the document converter, used in both directions ----------------------

/** Rules as the export document carries them. */
export function toDocument(rules: Rule[], roots: Root[]): RuleDoc[] {
  const pathOf = new Map(roots.map((r) => [r.id, r.path]))
  return rules.map((rule) => {
    if (rule.root_id !== null && !pathOf.has(rule.root_id))
      throw new Error(
        `rule ${rule.ordinal} names root ${rule.root_id}, which this library does not have`,
      )
    return {
      ordinal: rule.ordinal,
      enabled: rule.enabled,
      root_path:
        rule.root_id === null ? null : (pathOf.get(rule.root_id) ?? null),
      kind: rule.kind,
      pattern: rule.pattern,
      match_on: rule.match_on,
      set_edition_key: rule.set_edition_key,
      set_kind: rule.set_kind,
      set_language: rule.set_language,
      set_series: rule.set_series,
      note: rule.note,
    }
  })
}

/**
 * An export document read back against this library's roots.
 *
 * **Every rule comes back `id: 0`.** A document carries none of this library's
 * ids, and inventing one is exactly what would make a save claim to keep a row
 * it has never seen.
 */
export function fromDocument(doc: RuleDoc[], roots: Root[]): Rule[] {
  const idOf = new Map(roots.map((r) => [r.path, r.id]))
  return doc.map((entry) => {
    if (entry.root_path !== null && !idOf.has(entry.root_path))
      throw new Error(
        `this document names the root ${entry.root_path}, which this library does not have`,
      )
    return {
      id: 0,
      ordinal: entry.ordinal,
      enabled: entry.enabled,
      root_id:
        entry.root_path === null ? null : (idOf.get(entry.root_path) ?? null),
      kind: entry.kind,
      pattern: entry.pattern,
      match_on: entry.match_on,
      set_series: entry.set_series,
      set_edition_key: entry.set_edition_key,
      set_kind: entry.set_kind,
      set_language: entry.set_language,
      note: entry.note,
    }
  })
}

// --- pure helpers ---------------------------------------------------------

/** `list` with the entry at `from` moved to `to`. */
export function moved<T>(list: T[], from: number, to: number): T[] {
  const out = [...list]
  const [entry] = out.splice(from, 1)
  out.splice(to, 0, entry)
  return out
}

/**
 * Ordinals set to list position, one-based.
 *
 * **A drag that changed the array and not the ordinals would preview exactly
 * as it did before it.** Layering is positional — `Engine::compile` orders by
 * `ordinal` then id — so the array's order means nothing to the server until
 * it is written into the column the server reads.
 */
export function renumber(rules: Rule[]): Rule[] {
  return rules.map((rule, index) => ({ ...rule, ordinal: index + 1 }))
}

/** A rule the editor has added and not saved. */
export function blankRule(ordinal: number): Rule {
  return {
    id: 0,
    ordinal,
    enabled: true,
    root_id: null,
    kind: 'regex',
    pattern: '',
    match_on: 'path',
    set_series: null,
    set_edition_key: null,
    set_kind: null,
    set_language: null,
    note: null,
  }
}

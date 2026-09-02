/**
 * The `/api/library/{works,series,editions,members}/{id}` PATCH API —
 * spec C3 §5. Knows HTTP; knows nothing about a screen.
 *
 * Separate from `library.ts` and `curation.ts` for the reason those two are
 * separate from each other: `library.ts` is C1's read layer over `browse`,
 * `curation.ts` is C3 §7's proposals inbox, and this is C3 §5's direct edits
 * to a work, series, edition or member's own document. All three share
 * `api.ts`'s `getJson`/`sendJson`, so the 401 rule and the write error shape
 * stay one rule across every module that talks to the server, rather than
 * three that can drift.
 *
 * # The wire contract, as the server actually answers — not as planned
 *
 * Every route answers the re-projected `LibraryDoc` — what the store now
 * holds, not what the caller sent — merged with two more fields:
 *
 * - **`persisted` is always a plain boolean.** `false` means the row was
 *   written and is safe from the next scan, but the `<stem>.library.json`
 *   sidecar did not reach disk: a read-only root, or a work with no on-disk
 *   address at all (every work imported from a sidecar directly, rather than
 *   scanned under a root).
 * - **`why` is present only when non-empty, and is never `[]`.** It is
 *   absent on the *ordinary* path — every write that both updated the row
 *   and reached disk — and present as a non-empty array of reasons only when
 *   at least one document failed to land. `patch_series` and `patch_work`'s
 *   own `series` field can fan out to more than one work's document, so more
 *   than one reason can occur in a single answer; a single-document route
 *   answers at most one. **A type or a caller that reads `why` as always an
 *   array — `body.why.length` unguarded — is wrong on the common case**,
 *   which is exactly why [`PatchResult.why`] is optional rather than
 *   defaulted, and why [`reasons`] exists below rather than leaving every
 *   caller to write its own `?? []`.
 *
 * A non-2xx response is never one of the two shapes above. **A hard failure
 * — 409 (a refusal `librarydoc::of`/`apply` or `hand::save` raised), 404 (a
 * slug or id naming nothing this route can edit), 403 (a non-admin) — is a
 * bare string body, not JSON.** `sendJson` already reads the body as text
 * rather than parsing it on any non-`ok` response, which is what keeps a
 * plain-text 409 from being handed to `JSON.parse` and thrown as a confusing
 * syntax error instead of the server's own sentence — see `api.ts`'s own
 * doc comment on `sendJson` for why every write in this codebase throws that
 * way.
 *
 * # Disagreements with Task 8's own brief
 *
 * The brief (and the plan it was copied from) predates the server's shipped
 * shape and sketches `Promise<LibraryDoc & { persisted: boolean }>` with no
 * `why` at all, and its illustrative tests never exercise a `why` or a 409.
 * Both are wrong in the same direction: `why` is not always present, so it is
 * modelled here as optional (`why?: string[]`) rather than a bare
 * `LibraryDoc & { persisted: boolean }` with `why` unmentioned, or worse,
 * defaulted to `[]` — either would tell a caller a lie about the ordinary
 * case (no writes ever land as `why: []`; see `worst_case` and
 * `merge_worst_case` in `crates/taiyaku-serve/src/metadata.rs`, which build
 * this fact into the response body directly). The two tests the brief did
 * not carry — an absent `why` and a bare-string 409 — are added below,
 * since a client that only ever saw an array `why` or a JSON error body in
 * its own tests is a client that breaks on first contact with the real
 * server.
 */
import { sendJson } from './api'
import type { Direction } from './modules/direction'

/** A closed set — `series_title.kind` in `librarydoc.rs`. */
export type TitleKind = 'romaji' | 'native' | 'english' | 'alt'

/** One series title: 「五等分の花嫁」, one of several for one series. */
export interface SeriesTitle {
  title: string
  kind: TitleKind
  /** The title a reader is shown by default. Absent, not `false`, when unset — the server skips a false `primary` rather than sending it. */
  primary?: boolean
}

/**
 * A series grouping: its names and its reading direction.
 *
 * Both fields are absent, not present-and-empty, on a work with no series
 * information at all — `librarydoc.rs`'s `SeriesInfo::is_empty` skips the
 * whole `series` key from the document rather than sending `{}`.
 */
export interface SeriesInfo {
  titles?: SeriesTitle[]
  reading?: Direction
}

/** A closed set — `edition.kind` in `librarydoc.rs`. */
export type EditionKind = 'raw' | 'official' | 'fan'

/** A closed set — `credit.role` in `librarydoc.rs`. */
export type CreditRole =
  | 'author'
  | 'artist'
  | 'translator'
  | 'group'
  | 'publisher'

/** One person credited on one edition. */
export interface Credit {
  role: CreditRole
  name: string
}

/** Hand-set metadata for one edition, keyed by the sidecar's own edition key on [`LibraryDoc.editions`]. */
export interface EditionMeta {
  label?: string
  language?: string
  kind?: EditionKind
  credits?: Credit[]
}

/**
 * A closed set — `member.chapter_source` in `librarydoc.rs`. Never sent by
 * this client: [`MemberEdit`] has no such field, since only the server may
 * choose `hand` for a hand-set chapter — see this module's own note on
 * [`patchMember`].
 */
export type ChapterSource = 'comicinfo' | 'filename' | 'hand'

/** One member's chapter number, addressed by content hash rather than path. */
export interface MemberMeta {
  sha256: string
  chapter?: number
  chapter_source?: ChapterSource
}

/**
 * A whole `<stem>.library.json`, as every route here answers it — the
 * document the store now holds, re-projected, not an echo of what was sent.
 *
 * Every field below `updated` is absent rather than present-and-empty on a
 * work nobody has hand-edited: `librarydoc.rs`'s own `skip_serializing_if`
 * on each one omits the key rather than sending `{}`, `[]` or `null`. A type
 * of `series: SeriesInfo` (never absent) or `tags: string[]` (never absent)
 * would be a type lying about a document that has not been touched.
 */
export interface LibraryDoc {
  taiyaku_library: number
  updated: string
  series?: SeriesInfo
  volume?: number
  title?: string
  editions?: Record<string, EditionMeta>
  /** Keyed by `sha256`, never by path — a path is not stable across a rescan. */
  members?: MemberMeta[]
  tags?: string[]
  /** Provider name (`anilist`, `mal`, `mangaupdates`, …) to that provider's id. */
  external?: Record<string, string>
}

/**
 * What every one of this module's four routes answers: the document as
 * re-projected, plus whether it reached disk.
 *
 * See this module's own doc comment for why `why` is optional rather than a
 * defaulted array — it is present, and non-empty, only when `persisted` is
 * `false` for a reason the server can name.
 */
export interface PatchResult extends LibraryDoc {
  persisted: boolean
  why?: string[]
  /**
   * Present, and `false`, only when the work's root keeps no documents at
   * all — absent means the ordinary case, the same rule `why` follows.
   *
   * The editor needs no branch for it: a root that keeps no documents answers
   * `persisted: true`, and the "not durable" note already keys off
   * `persisted`. It is here so a future screen can say *this library keeps its
   * corrections in the database only* without re-deriving that from a silence.
   */
  keeps_documents?: boolean
}

/**
 * `why`, defaulted to empty — the one line a caller would otherwise have to
 * repeat at every call site rather than write once here.
 *
 * Beside the four routes rather than in a component, the same reasoning
 * `curation.ts`'s own pure helpers give: one field of a server answer turned
 * into what a screen needs, worth testing on its own and worth keeping in
 * one place rather than three.
 */
export const reasons = (result: Pick<PatchResult, 'why'>): string[] =>
  result.why ?? []

/**
 * `sendJson` resolves `T | undefined` because two of its own routes answer a
 * 204 with nothing to parse — see `curation.ts`'s own `expectBody`, which
 * this mirrors. None of this module's four routes ever do: every branch in
 * `crates/taiyaku-serve/src/metadata.rs` answers either a document (200) or
 * a bare-string refusal caught before this ever runs (see `sendJson`'s own
 * throw on a non-`ok` response). Narrowing here is what lets every function
 * below return `Promise<PatchResult>` rather than `Promise<PatchResult |
 * undefined>`, without teaching `sendJson` itself a rule that would be a lie
 * for its other callers.
 */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

/**
 * `PATCH /api/library/works/{slug}` fields — volume, title, series
 * membership. Every field absent means "leave it alone", the module-wide
 * contract every one of this file's edit types holds to; only send the keys
 * actually being changed.
 */
export interface WorkEdit {
  title?: string
  volume?: number
  series?: SeriesInfo
}

export const patchWork = (
  slug: string,
  fields: WorkEdit,
): Promise<PatchResult> =>
  expectBody(
    sendJson<PatchResult>(
      `/api/library/works/${encodeURIComponent(slug)}`,
      'PATCH',
      fields,
    ),
  )

/**
 * `PATCH /api/library/series/{id}` fields — titles and their kind, reading
 * direction, tags, external ids. Absent means "leave it alone", same as
 * every edit type in this file.
 */
export interface SeriesEdit {
  titles?: SeriesTitle[]
  reading?: Direction
  tags?: string[]
  external?: Record<string, string>
}

export const patchSeries = (
  id: number,
  fields: SeriesEdit,
): Promise<PatchResult> =>
  expectBody(
    sendJson<PatchResult>(`/api/library/series/${id}`, 'PATCH', fields),
  )

/**
 * `PATCH /api/library/editions/{id}` fields — label, language, kind,
 * credits. Absent means "leave it alone", same as every edit type in this
 * file.
 */
export interface EditionEdit {
  label?: string
  language?: string
  kind?: EditionKind
  credits?: Credit[]
}

export const patchEdition = (
  id: number,
  fields: EditionEdit,
): Promise<PatchResult> =>
  expectBody(
    sendJson<PatchResult>(`/api/library/editions/${id}`, 'PATCH', fields),
  )

/**
 * `PATCH /api/library/members/{id}` fields — a hand-set chapter number.
 *
 * **No `chapter_source` field, deliberately, and never add one.** The
 * server forces `chapter_source: hand` on any write that sets `chapter` —
 * `crates/taiyaku-serve/src/metadata.rs`'s own doc comment on `MemberEdit`
 * explains why: a hand-set number is a different kind of claim from a
 * parsed one, and a client that could send `chapter_source: "comicinfo"`
 * alongside a number a person typed could launder the one into the other.
 * `chapter` absent leaves the member's existing chapter and source alone;
 * `patch_member` had a defect where an absent `chapter` erased both, which
 * is exactly the failure "send only what changed" exists to prevent.
 */
export interface MemberEdit {
  chapter?: number
}

export const patchMember = (
  id: number,
  fields: MemberEdit,
): Promise<PatchResult> =>
  expectBody(
    sendJson<PatchResult>(`/api/library/members/${id}`, 'PATCH', fields),
  )

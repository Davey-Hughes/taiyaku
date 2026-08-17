/**
 * The `taiyaku serve` API. Knows HTTP; knows nothing about reading.
 *
 * Page and thumbnail are URLs rather than fetches: both go straight into an
 * `<img src>`, so the browser's own cache and decoding handle them, and
 * fetching them into memory first would defeat both.
 */
import type { NotesDoc } from './notes'
import type { TruthDoc } from './truth'

const seg = (s: string) => encodeURIComponent(s)

/**
 * `index` is a *page* index into the server's flattened page sequence — not
 * a *slot* index from the reading model (`Resolver.slotAt`, `ReadingState`).
 * A joined spread is one page but two slots, so the two only coincide up to
 * the first spread. A caller holding a slot must convert first, via
 * `Resolver.slotAt(edition, slot).page`. Same goes for `thumbUrl` below.
 */
/**
 * `longEdge` asks the server to fit the page inside that many pixels — see
 * `pageSize.ts` for how the reader chooses one and why. Omitted means the
 * member's own bytes at full size, which is what zoom asks for and what every
 * caller wanting the original keeps getting.
 *
 * The parameter is part of the URL rather than a header, so the browser's own
 * cache keys on it: two sizes of one page are two cache entries, which is what
 * lets a zoom-in and a zoom-out both be instant on a page already fetched at
 * both sizes.
 */
export const pageUrl = (
  work: string,
  edition: string,
  index: number,
  longEdge?: number,
) => {
  const path = `/api/works/${seg(work)}/${seg(edition)}/pages/${index}`
  return longEdge === undefined ? path : `${path}?w=${longEdge}`
}

export const thumbUrl = (work: string, edition: string, index: number) =>
  `${pageUrl(work, edition, index)}/thumb`

/**
 * One entry of `GET /api/works` — enough to identify a work, group it under
 * a series and show its cover, without shipping the sidecar itself.
 *
 * **`series` and `volume` come from the server's library rows, not from the
 * sidecar** — there is no field for either on the sidecar document, because a
 * series grouping and a volume number are spec B's scanner's job, not the
 * aligner's. Both are `null` today rather than absent: nothing populates
 * either column yet, and the server answers them honestly as "not yet known"
 * rather than leaving the key out.
 *
 * `slug` is the permalink and can outlive `title`: `taiyaku-library`'s
 * `import::update_work` updates a re-imported work's title but leaves its
 * slug alone on purpose, so a bookmark survives a rename. A stale slug
 * beside a fresh title is therefore expected, not a bug.
 */
export interface WorkSummary {
  slug: string
  title: string
  series: string | null
  volume: number | null
  editions: string[]
  cover: string
}

export async function listWorks(): Promise<WorkSummary[]> {
  const res = await fetch('/api/works')
  if (!res.ok)
    throw new Error(`listing works: ${res.status} ${await res.text()}`)
  return res.json()
}

export interface FetchedSidecar {
  json: string
  /** Absent only from a server older than the correction route. */
  etag?: string
}

/**
 * The sidecar, and the validator a correction has to be written against.
 *
 * Separate from `getSidecar` rather than replacing it: a caller that only
 * wants to read the document should not have to know a correction path exists,
 * and the validator is meaningless to it. `getSidecar` below delegates here
 * rather than repeating the fetch, so there is one implementation of "load
 * the sidecar" and not two that have to be kept in step.
 */
export async function getSidecarWithEtag(
  work: string,
): Promise<FetchedSidecar> {
  const res = await fetch(`/api/works/${seg(work)}`)
  if (!res.ok)
    throw new Error(`loading ${work}: ${res.status} ${await res.text()}`)
  return { json: await res.text(), etag: res.headers.get('etag') ?? undefined }
}

/** The sidecar, verbatim, as text — `wasmResolver` parses it, not JSON.parse. */
export async function getSidecar(work: string): Promise<string> {
  return (await getSidecarWithEtag(work)).json
}

export interface StoredProgress {
  edition: string
  slot: number
}

/** `undefined` where nothing is recorded — the server answers 204, not 404. */
export async function getProgress(
  work: string,
): Promise<StoredProgress | undefined> {
  const res = await fetch(`/api/works/${seg(work)}/progress`)
  if (res.status === 204) return undefined
  if (!res.ok)
    throw new Error(`reading progress: ${res.status} ${await res.text()}`)
  return res.json()
}

/**
 * `keepalive: true` is what lets this survive the page going away —
 * `App.tsx`'s `visibilitychange`/`pagehide` handlers flush a pending write
 * exactly when the page might not run another line of JS, and a plain
 * `fetch` there is liable to be cancelled by the browser before the request
 * leaves. `navigator.sendBeacon` is the usual tool for that moment, but it
 * always sends a `POST`, and this route is registered as `GET`/`PUT` only
 * (`taiyaku-serve`'s `routes.rs`) — a `sendBeacon` call here would 405
 * against a route that was never taught to accept it, so `fetch` with
 * `keepalive: true` is the one that actually works. The body here is a few
 * bytes of JSON, well under the keepalive request's ~64 KiB total-body
 * budget, so there is no reason to reserve this flag for only the unload
 * path — every write gets the same resilience.
 */
export async function putProgress(
  work: string,
  progress: StoredProgress,
): Promise<void> {
  const res = await fetch(`/api/works/${seg(work)}/progress`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(progress),
    keepalive: true,
  })
  if (!res.ok)
    throw new Error(`writing progress: ${res.status} ${await res.text()}`)
}

/** The notes document and the validator a write has to be conditional on. */
export interface FetchedNotes {
  doc: NotesDoc
  /** Absent only from a server older than the notes route. */
  etag?: string
}

/**
 * Every note on a work.
 *
 * A work nobody has annotated answers with an empty document rather than a
 * 204, unlike `getProgress` above — writing notes requires `If-Match`, and a
 * client that had never seen a document would have no validator to send for
 * its first write. So there is no "nothing recorded" case to handle here:
 * there is always a document and always a tag.
 */
export async function getNotes(work: string): Promise<FetchedNotes> {
  const res = await fetch(`/api/works/${seg(work)}/notes`)
  if (!res.ok)
    throw new Error(`reading notes: ${res.status} ${await res.text()}`)
  return {
    doc: (await res.json()) as NotesDoc,
    etag: res.headers.get('etag') ?? undefined,
  }
}

/**
 * Replace a work's notes, conditionally.
 *
 * `If-Match` is required by the server rather than optional here, the same as
 * `patchAlignment` below: two devices annotating one work is the same shape as
 * two correcting it, and last-write-wins would discard a note with nothing
 * anywhere saying so. The response body is the stored document, so a caller
 * refreshes from this rather than making a second request that could already
 * be out of date again.
 *
 * No `keepalive`, unlike `putProgress`: a note is a deliberate act the reader
 * is watching the result of, not a background write that has to survive the
 * tab going away.
 *
 * **The returned `etag` is `undefined` where the response carried no `ETag`,
 * and that is not a shrug.** `routes.rs`'s `put_notes` always sends one on a
 * success, so reaching it means a proxy stripped the header or the server is
 * older than this route — and either way the caller is now holding a document
 * it has no validator for. `notesWrite.ts`'s `writeGate` is what that answer
 * runs into: the next write is refused and the reader is told, rather than
 * every write from here on being a silent no-op, which is what it used to be.
 */
export async function putNotes(
  work: string,
  etag: string,
  doc: NotesDoc,
): Promise<FetchedNotes> {
  const res = await fetch(`/api/works/${seg(work)}/notes`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', 'if-match': etag },
    body: JSON.stringify(doc),
  })
  if (!res.ok)
    throw new Error(`writing notes: ${res.status} ${await res.text()}`)
  return {
    doc: (await res.json()) as NotesDoc,
    etag: res.headers.get('etag') ?? undefined,
  }
}

export interface CorrectionRequest {
  from: string
  to: string
  /** A slot of `from`, in span notation — see `correction.ts`'s `spanOf`. */
  a: string
  /** A slot of `to`, or `null` to unpair `a`. */
  b: string | null
  note?: string
}

/**
 * Write one hand correction, and get the document it produced.
 *
 * `If-Match` is required by the server, not optional here: two devices
 * correcting one work is the same shape as two writing progress, and last
 * write wins would discard the other one's correction with nothing anywhere
 * saying so. The response body is the updated sidecar, so a caller refreshes
 * its resolver from this rather than making a second request that could
 * already be out of date again.
 *
 * No `keepalive` here, unlike `putProgress`: a correction is a deliberate act
 * a reader is watching the result of, not a background write that has to
 * survive the tab going away.
 *
 * The thrown message carries `res.status` ahead of the body, like every other
 * failure in this file — a caller telling a stale-validator `412` apart from
 * a malformed-body `400` or a missing-`If-Match` `428` needs the status, not
 * just the server's prose. The body text still comes along too: on a `412`
 * it names the fresh `ETag`, which is what a caller needs to retry.
 */
export async function patchAlignment(
  work: string,
  etag: string,
  correction: CorrectionRequest,
): Promise<FetchedSidecar> {
  const res = await fetch(`/api/works/${seg(work)}/alignments`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json', 'if-match': etag },
    body: JSON.stringify(correction),
  })
  if (!res.ok)
    throw new Error(`correcting ${work}: ${res.status} ${await res.text()}`)
  return { json: await res.text(), etag: res.headers.get('etag') ?? undefined }
}

export interface FetchedTruth {
  doc: TruthDoc
  /** Absent only from a server older than the truth route, or a proxy that stripped it. */
  etag?: string
}

/** Every labelled box on a work. */
export async function getTruth(work: string): Promise<FetchedTruth> {
  const res = await fetch(`/api/works/${seg(work)}/truth`)
  if (!res.ok)
    throw new Error(`reading truth: ${res.status} ${await res.text()}`)
  return {
    doc: (await res.json()) as TruthDoc,
    etag: res.headers.get('etag') ?? undefined,
  }
}

/**
 * Replace a work's truth document, conditionally.
 *
 * `If-Match` is required by the server rather than optional here, as for
 * `putNotes`. The stakes are higher than notes: a labelling session is hours
 * of human work that exists nowhere else, and last-write-wins would discard
 * it silently.
 */
export async function putTruth(
  work: string,
  etag: string,
  doc: TruthDoc,
): Promise<FetchedTruth> {
  const res = await fetch(`/api/works/${seg(work)}/truth`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', 'if-match': etag },
    body: JSON.stringify(doc),
  })
  if (!res.ok)
    throw new Error(`writing truth: ${res.status} ${await res.text()}`)
  return {
    doc: (await res.json()) as TruthDoc,
    etag: res.headers.get('etag') ?? undefined,
  }
}

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
 * A 401 from any of this module's calls.
 *
 * **A state change rather than an error to show.** Being signed out is not
 * something a reader can act on by reading a message; it is something `App`
 * routes on. Raised as its own type from one place — `request` below — because
 * eight separate branches is eight chances for a ninth call site to forget, and
 * the ninth is exactly the one that would ship signed-out readers a stack trace
 * where a login screen belongs.
 */
export class Unauthenticated extends Error {
  constructor() {
    super('not signed in')
    this.name = 'Unauthenticated'
  }
}

/**
 * Called once, from anywhere, the first time a 401 arrives.
 *
 * **Routing a 401 cannot be left to each call site.** It was, and the writes all
 * ended in `.catch(console.error)` — so a session revoked from another browser
 * left the reader turning pages while every position and note write was
 * discarded into a console nobody has open, with no login screen and nothing on
 * screen wrong. Only the two load-time effects routed it, which are exactly the
 * calls that happen when a session is *least* likely to have just died.
 *
 * A module-level hook rather than a parameter threaded through sixteen
 * signatures: the answer is the same for every call, and a signature a caller
 * can decline to pass is how this happened the first time. Found by the
 * whole-branch review, 2026-08-18.
 */
let onUnauthenticated: (() => void) | undefined

export function whenSignedOut(handler: () => void): void {
  onUnauthenticated = handler
}

/**
 * `fetch`, with the one status this module answers structurally.
 *
 * Every call below goes through this rather than calling `fetch` directly.
 * `pageUrl` and `thumbUrl` are the deliberate exceptions and do not: they
 * return URLs for an `<img src>`, so the browser fetches them and a 401 there
 * shows as a broken image rather than reaching any code here — which is why
 * `App` asks `me()` on load instead of waiting to be told by a page that failed
 * to draw.
 */
const request = async (input: string, init?: RequestInit) => {
  // Forwarded without an explicit `undefined` when there is no init, so the
  // call `fetch` sees is byte-for-byte the one it saw before this wrapper
  // existed. That keeps every pre-existing test in `api.test.ts` passing
  // untouched, which is the evidence that this change did nothing to these
  // calls except notice a 401.
  const res = init === undefined ? await fetch(input) : await fetch(input, init)
  if (res.status === 401) {
    // Fired before the throw, so a caller that swallows the rejection — every
    // write does, deliberately, since a failed position write is not something
    // to show a reader — still hands the app the one fact it must act on.
    onUnauthenticated?.()
    throw new Unauthenticated()
  }
  return res
}

/** Who the server resolved this browser to, or `Unauthenticated`. */
export interface Me {
  user_id: number
  admin: boolean
  can_download: boolean
  can_stream: boolean
}

export async function me(): Promise<Me> {
  const res = await request('/api/me')
  if (!res.ok) throw new Error(`reading the account: ${res.status}`)
  return (await res.json()) as Me
}

/**
 * Exchange a name and password for a session cookie.
 *
 * The cookie is `HttpOnly`, so nothing here ever sees it — the browser holds it
 * and sends it, and this function's whole output is whether it worked. That is
 * the point of `HttpOnly`: a script that cannot read the session cannot leak it.
 *
 * **Not routed through `request`**, because a 401 here is the ordinary answer to
 * a wrong password rather than a signed-out state to route on.
 */
export async function login(name: string, password: string): Promise<void> {
  const res = await fetch('/api/session', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name, password }),
  })
  if (res.status === 401)
    throw new Error('That name and password did not match.')
  if (res.status === 429)
    throw new Error('Too many attempts from here. Try again shortly.')
  if (res.status === 403)
    throw new Error(
      'The server refused this request\u2019s origin. If it sits behind a proxy, that proxy has to pass the Origin header through.',
    )
  // A 500 here means the password was accepted and the session could not be
  // recorded — see the server's `Attempt::Failed`. Distinct from the 401 above
  // so a reader is not told their correct password is wrong.
  if (res.status === 500)
    throw new Error(
      'The password was right, but the server could not record the session. Try again in a moment.',
    )
  if (!res.ok) throw new Error(`signing in: ${res.status} ${await res.text()}`)
}

/** One API key as the account screen lists it — never its secret. */
export interface ApiKey {
  id: number
  name: string
  created: string
  last_used: string | null
}

export async function listKeys(): Promise<ApiKey[]> {
  const res = await request('/api/keys')
  if (!res.ok) throw new Error(`listing keys: ${res.status}`)
  return (await res.json()) as ApiKey[]
}

/**
 * Create an API key and return its secret.
 *
 * **The only time the secret exists outside the browser's memory.** The server
 * stores its sha256 and nothing else, so a caller that drops this string cannot
 * ask for it again — which the account screen has to say plainly rather than
 * leave a reader to discover.
 */
export async function createKey(name: string): Promise<string> {
  const res = await request('/api/keys', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`creating a key: ${res.status}`)
  return ((await res.json()) as { secret: string }).secret
}

export async function revokeKey(id: number): Promise<void> {
  const res = await request(`/api/keys/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`revoking key ${id}: ${res.status}`)
}

/**
 * Change this account's password.
 *
 * Every *other* session of this account ends server-side; this one does not, so
 * a reader who changes their password is not signed out of the browser they
 * changed it from.
 */
export async function changePassword(
  current: string,
  next: string,
): Promise<void> {
  const res = await request('/api/password', {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ current, next }),
  })
  if (res.status === 403)
    throw new Error('That current password did not match.')
  if (!res.ok) throw new Error(`changing the password: ${res.status}`)
}

/** One open session as the account screen lists it. */
export interface SessionRow {
  id: string
  created: string
  last_seen: string
  user_agent: string | null
  ip: string | null
  /** Whether this is the browser doing the asking. */
  current: boolean
}

export async function listSessions(): Promise<SessionRow[]> {
  const res = await request('/api/sessions')
  if (!res.ok) throw new Error(`listing sessions: ${res.status}`)
  return (await res.json()) as SessionRow[]
}

export async function revokeSession(id: string): Promise<void> {
  const res = await request(`/api/sessions/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  })
  if (!res.ok) throw new Error(`revoking a session: ${res.status}`)
}

/** End this session. Answers 204 whether or not there was one. */
export async function logout(): Promise<void> {
  const res = await fetch('/api/session', { method: 'DELETE' })
  if (!res.ok) throw new Error(`signing out: ${res.status}`)
}

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
  const res = await request('/api/works')
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
  const res = await request(`/api/works/${seg(work)}`)
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
  const res = await request(`/api/works/${seg(work)}/progress`)
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
  const res = await request(`/api/works/${seg(work)}/progress`, {
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
  const res = await request(`/api/works/${seg(work)}/notes`)
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
  const res = await request(`/api/works/${seg(work)}/notes`, {
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
  const res = await request(`/api/works/${seg(work)}/alignments`, {
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
  const res = await request(`/api/works/${seg(work)}/truth`)
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
  const res = await request(`/api/works/${seg(work)}/truth`, {
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

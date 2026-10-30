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
 * something a reader can act on by reading a message; it is something
 * `Gate.tsx` routes on. Raised as its own type from one place — `request`
 * below — because a branch in every call is one more chance for the next call
 * site to forget, and that one is exactly the one that would ship signed-out
 * readers a stack trace where a login screen belongs.
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
 * **Routing a 401 cannot be left to each call site.** A write that ends in
 * `.catch(console.error)` would, once a session was revoked from another
 * browser, leave the reader turning pages while every position and note write
 * is discarded into a console nobody has open, with no login screen and
 * nothing on screen wrong. Routing it only from the two load-time effects
 * would catch exactly the calls that happen when a session is *least* likely
 * to have just died.
 *
 * A module-level hook rather than a parameter threaded through each call's
 * signature: the answer is the same for every call, and a signature a caller
 * can decline to pass is how a 401 goes unrouted.
 *
 * **One slot, so exactly one component may register.** A second registration
 * replaces the first with nothing said, which is why this belongs to
 * `Gate.tsx` — the one component above the router, mounted for the whole page
 * load — and not to a screen: `main.tsx` renders the router, `/` is the
 * library, and `App` mounts on `/read/:work` alone, so a signed-out visitor on
 * `/` would reach a screen where this hook was never registered at all.
 */
let onUnauthenticated: (() => void) | undefined

export function whenSignedOut(handler: () => void): void {
  onUnauthenticated = handler
}

/**
 * `fetch`, with the one status this module answers structurally.
 *
 * Every call below goes through this rather than calling `fetch` directly,
 * except `login` (its doc says why) and `logout`. `pageUrl` and `thumbUrl`
 * do not fetch: they return URLs for an `<img src>`, so the browser fetches
 * them and a 401 there shows as a broken image rather than reaching any code
 * here — which is why `Gate.tsx` asks `me()` on load instead of waiting to be
 * told by a page that failed to draw.
 */
const request = async (input: string, init?: RequestInit) => {
  // Forwarded without an explicit `undefined` when there is no init, so the
  // call `fetch` sees is byte-for-byte a direct `fetch(input)`.
  const res = init === undefined ? await fetch(input) : await fetch(input, init)
  if (res.status === 401) {
    // Fired before the throw, so a caller that swallows the rejection — a
    // progress write does, deliberately, since a failed position write is not
    // something to show a reader — still hands the app the one fact it must
    // act on.
    onUnauthenticated?.()
    throw new Unauthenticated()
  }
  return res
}

/**
 * A guarded `GET` returning JSON.
 *
 * Exported for `library.ts`, which is a second module of calls against the
 * same server and the same session. A second `fetch` wrapper there would be a
 * second place for the 401 rule to be forgotten, which is the exact failure
 * `whenSignedOut` above exists to have fixed once.
 *
 * **A non-2xx throws the body along with the status, the same as `sendJson`
 * below and for the same reason.** The read routes it serves are where leaving
 * the body out would cost the most: `listProposals` and `proposalDetail` both
 * come through here, `Routes.tsx` puts no admin guard on `/proposals`, and so
 * a reader who types that URL would be shown `"/api/library/proposals: 403"`
 * while the server's own sentence — the proposals inbox is an administrator's
 * — would be read off the wire and dropped.
 */
export async function getJson<T>(path: string): Promise<T> {
  const res = await request(path)
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
  return (await res.json()) as T
}

/**
 * A guarded write returning JSON, or `undefined` for a 204.
 *
 * Exported for the same reason `getJson` is: `curation.ts` is a third
 * module of calls against the same server and the same session, and a
 * second `fetch` wrapper there would be a second place for the 401 rule to
 * be forgotten.
 *
 * A non-2xx throws the path, the status and the body together — the shape
 * `patchAlignment` and `putNotes` throw, and for the same reason
 * `patchAlignment`'s own doc gives: the status is what tells a caller a
 * stale-validator refusal apart from one it cannot fix by retrying. The body
 * matters here specially: the proposals API refuses an accept by *naming the
 * series it would have moved*, and a caller shown only the status would
 * throw away the whole point of the refusal.
 */
export async function sendJson<T>(
  path: string,
  method: 'POST' | 'PUT' | 'DELETE' | 'PATCH',
  body?: unknown,
): Promise<T | undefined> {
  const res = await request(path, {
    method,
    ...(body === undefined
      ? {}
      : {
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(body),
        }),
  })
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
  if (res.status === 204) return undefined
  return (await res.json()) as T
}

/** Who the server resolved this browser to, or `Unauthenticated`. */
export interface Me {
  user_id: number
  admin: boolean
  can_download: boolean
  can_stream: boolean
  /**
   * **Not implied by `admin`.** Every other capability here reads
   * `admin || column` in `Principal::from_account`; this one reads the column
   * alone — the one documented exception to O86, because it decides whether an
   * account may rewrite files taiyaku does not own. A screen that treated an
   * administrator as holding it would offer a control the server answers 403.
   */
  can_stamp: boolean
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
  if (!res.ok) throw new Error(`/api/keys: ${res.status} ${await res.text()}`)
  return ((await res.json()) as { secret: string }).secret
}

export async function revokeKey(id: number): Promise<void> {
  const path = `/api/keys/${id}`
  const res = await request(path, { method: 'DELETE' })
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
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
  if (!res.ok)
    throw new Error(`/api/password: ${res.status} ${await res.text()}`)
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
  const path = `/api/sessions/${encodeURIComponent(id)}`
  const res = await request(path, { method: 'DELETE' })
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
}

/** End this session. Answers 204 whether or not there was one. */
export async function logout(): Promise<void> {
  const res = await fetch('/api/session', { method: 'DELETE' })
  if (!res.ok)
    throw new Error(`/api/session: ${res.status} ${await res.text()}`)
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
 * caller wanting the original gets.
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

/**
 * A filmstrip tile for one page, at `longEdge` if one is given — O134.
 *
 * **The query goes after `/thumb`, and cannot come from `pageUrl`.** That
 * helper appends its own `?w=` to the page path, which this route hangs
 * `/thumb` off: reusing it would build `…/pages/3?w=320/thumb`, a path no
 * route matches. The two sizes are deliberately different parameters of
 * different routes — a page's `w` is what the reader draws, a tile's is one
 * of `tileSize.ts`'s three rungs.
 *
 * Without one it is the 160 px tile — what `CreditsPanel.tsx` asks for (its
 * thumbnails are 56 px tall, so even a 3× screen wants only 168 and a rung
 * there would fetch 320 to fix nothing), and what an account that may not
 * stream is given whatever it asks for.
 *
 * A library cover does **not** come through here at all: the server sends its
 * address on the same tile route, `?v=` included, and `library.ts`'s
 * `coverSources` adds a cover-size `w` to it. Two callers of one route, sized
 * by two different ladders.
 */
export const thumbUrl = (
  work: string,
  edition: string,
  index: number,
  longEdge?: number,
) => {
  const path = `${pageUrl(work, edition, index)}/thumb`
  return longEdge === undefined ? path : `${path}?w=${longEdge}`
}

/**
 * One entry of `GET /api/works` — enough to identify a work, group it under
 * a series and show its cover, without shipping the sidecar itself.
 *
 * **`series` and `volume` come from the server's library rows, not from the
 * sidecar** — there is no field for either on the sidecar document, because
 * a series grouping and a volume number are spec B's scanner's job, not
 * the aligner's.
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

/**
 * A 404 from the sidecar read — O179.
 *
 * **A type, so that the one screen with words of its own for this can ask
 * what was thrown rather than read it.** `ReaderFailed.tsx` says "The
 * server found no work named …" for a `/read/<slug>` the server answers 404,
 * and `App`'s load effect tells that failure from every other with
 * `instanceof`, as it tells `Unauthenticated`.
 *
 * **The message is the one `getSidecarWithEtag` builds for every non-2xx**: it
 * hands that string here for a 404, so `messageOf` reads from this what it
 * reads from a plain `Error`. `Labeller.tsx`'s notice is that string,
 * through `getSidecar`.
 *
 * `work` is the slug the caller asked for, as it was given and not as the URL
 * escapes it. The response's body is in the message and nowhere else.
 */
export class WorkNotFound extends Error {
  readonly work: string

  constructor(work: string, message: string) {
    super(message)
    this.name = 'WorkNotFound'
    this.work = work
  }
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
 *
 * A non-2xx that reaches here — a 401 does not, `request` throws it as
 * `Unauthenticated` — throws `loading`, the slug, the status and the body in
 * one message; a 404 throws that message as a `WorkNotFound`.
 */
export async function getSidecarWithEtag(
  work: string,
): Promise<FetchedSidecar> {
  const res = await request(`/api/works/${seg(work)}`)
  if (!res.ok) {
    const message = `loading ${work}: ${res.status} ${await res.text()}`
    throw res.status === 404
      ? new WorkNotFound(work, message)
      : new Error(message)
  }
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

/** One position per edition, as the server stores them. */
interface ServerPosition extends StoredProgress {
  updated: string
}

/**
 * `undefined` where nothing is recorded — the server answers 204, not 404.
 *
 * The server answers **every** edition's position, newest first. The reader
 * resumes into the head: it opens one edition at a time, and the most recently
 * updated is the one the reader was last in. The rest are not discarded
 * server-side — switching edition and coming back finds the place that
 * edition held.
 */
export async function getProgress(
  work: string,
): Promise<StoredProgress | undefined> {
  const res = await request(`/api/works/${seg(work)}/progress`)
  if (res.status === 204) return undefined
  if (!res.ok)
    throw new Error(`reading progress: ${res.status} ${await res.text()}`)
  const body = (await res.json()) as { positions?: ServerPosition[] }
  const newest = body.positions?.[0]
  return newest ? { edition: newest.edition, slot: newest.slot } : undefined
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
 *
 * Each save is held in `savesOut` until it settles, so `progressSaved` can
 * wait for it.
 */
export function putProgress(
  work: string,
  progress: StoredProgress,
): Promise<void> {
  const save = sendProgress(work, progress)
  savesOut.add(save)
  const settled = () => {
    savesOut.delete(save)
  }
  void save.then(settled, settled)
  return save
}

async function sendProgress(
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

/** The progress saves sent and not yet settled. */
const savesOut = new Set<Promise<void>>()

/**
 * Settles once every progress save sent before the call has settled; never
 * rejects. The Library view's reads wait on it, so a read after reading
 * answers with the position just saved rather than the one before it; a
 * failed save has nothing more to wait for, and a save sent after the call
 * is not this read's to wait for.
 */
export async function progressSaved(): Promise<void> {
  await Promise.allSettled([...savesOut])
}

/** How many progress saves are out: exported for its tests. */
export function progressSavesOut(): number {
  return savesOut.size
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
 * every write from here on being a silent no-op.
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
 * The thrown message carries `res.status` ahead of the body, as most writes
 * in this file do — a caller telling a stale-validator `412` apart from a
 * malformed-body `400` or a missing-`If-Match` `428` needs the status, not
 * just the server's prose. The body text still comes along too: on a `412`
 * it names the fresh `ETag`, which is what a caller needs to retry.
 * (`listKeys` and `listSessions` throw on status alone: the account screen
 * draws the last good list over a failed one and says nothing of it.)
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

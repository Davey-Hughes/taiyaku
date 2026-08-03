/**
 * The `taiyaku serve` API. Knows HTTP; knows nothing about reading.
 *
 * Page and thumbnail are URLs rather than fetches: both go straight into an
 * `<img src>`, so the browser's own cache and decoding handle them, and
 * fetching them into memory first would defeat both.
 */
const seg = (s: string) => encodeURIComponent(s)

/**
 * `index` is a *page* index into the server's flattened page sequence — not
 * a *slot* index from the reading model (`Resolver.slotAt`, `ReadingState`).
 * A joined spread is one page but two slots, so the two only coincide up to
 * the first spread. A caller holding a slot must convert first, via
 * `Resolver.slotAt(edition, slot).page`. Same goes for `thumbUrl` below.
 */
export const pageUrl = (work: string, edition: string, index: number) =>
  `/api/works/${seg(work)}/${seg(edition)}/pages/${index}`

export const thumbUrl = (work: string, edition: string, index: number) =>
  `${pageUrl(work, edition, index)}/thumb`

export async function listWorks(): Promise<string[]> {
  const res = await fetch('/api/works')
  if (!res.ok) throw new Error(`listing works: ${res.status} ${await res.text()}`)
  return res.json()
}

/** The sidecar, verbatim, as text — `wasmResolver` parses it, not JSON.parse. */
export async function getSidecar(work: string): Promise<string> {
  const res = await fetch(`/api/works/${seg(work)}`)
  if (!res.ok) throw new Error(`loading ${work}: ${res.status} ${await res.text()}`)
  return res.text()
}

export interface StoredProgress {
  edition: string
  slot: number
}

/** `undefined` where nothing is recorded — the server answers 204, not 404. */
export async function getProgress(work: string): Promise<StoredProgress | undefined> {
  const res = await fetch(`/api/works/${seg(work)}/progress`)
  if (res.status === 204) return undefined
  if (!res.ok) throw new Error(`reading progress: ${res.status} ${await res.text()}`)
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
export async function putProgress(work: string, progress: StoredProgress): Promise<void> {
  const res = await fetch(`/api/works/${seg(work)}/progress`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(progress),
    keepalive: true,
  })
  if (!res.ok) throw new Error(`writing progress: ${res.status} ${await res.text()}`)
}

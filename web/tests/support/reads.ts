import { vi } from 'vitest'
import type { Grid } from '../../src/library'
import { gridRead } from '../../src/pageReads'
import { entryOf, read, readSet } from '../../src/reads'
import { deferred } from './async'

/**
 * A `Read` whose client call the test answers: each call is a new request,
 * held open in `requests` until the test settles it, so the order answers
 * land in is the test's to choose.
 */
export function address<T>(path: string) {
  const requests: ReturnType<typeof deferred<T>>[] = []
  const fetch = vi.fn(() => {
    const request = deferred<T>()
    requests.push(request)
    return request.promise
  })
  return { read: read(path, fetch), fetch, requests }
}

/**
 * The grid of `library` read and kept with nothing drawing it, as a Library
 * view left behind leaves it, under `gridRead`'s address. `stale()` says
 * whether a write has marked it since: a stale entry is not drawn on
 * arrival, so the next visit waits for a request of its own. The mark sends
 * nothing, since nothing draws the address: `sent()` counts the requests,
 * the one that kept it included.
 */
export async function keptGrid(library?: number) {
  const grid = address<Grid>(gridRead(library).address)
  const kept = readSet([grid.read])
  grid.requests[0].resolve({ series: [], unfiled: [] })
  await kept
  return {
    stale: () => entryOf(grid.read)?.stale,
    sent: () => grid.fetch.mock.calls.length,
  }
}

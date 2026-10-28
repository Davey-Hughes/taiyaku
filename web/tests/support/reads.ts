import { vi } from 'vitest'
import { read } from '../../src/reads'
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

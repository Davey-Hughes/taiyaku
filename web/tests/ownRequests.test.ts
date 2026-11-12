import { describe, expect, it } from 'vitest'
import { ownRequests } from '../src/deriveHold'
import { deferred } from './support/async'

/** A request sent through `own`, held open until it is settled. */
const sentThrough = (own: ReturnType<typeof ownRequests>) => {
  const request = deferred<string>()
  const tracked = own.track(request.promise)
  // Settles as the page's own handlers do, failure included.
  const done = tracked.then(
    () => undefined,
    () => undefined,
  )
  return { request, tracked, done }
}

describe('ownRequests', () => {
  it('overlaps a read sent while a request is out, even once it has settled', async () => {
    const own = ownRequests()
    const { request, done } = sentThrough(own)
    const mark = own.mark()
    expect(own.overlapped(mark)).toBe(true)
    request.resolve('answer')
    await done
    expect(own.overlapped(mark)).toBe(true)
  })

  it('overlaps a read sent before a request that was sent and settled while the read was out', async () => {
    const own = ownRequests()
    const mark = own.mark()
    const { request, done } = sentThrough(own)
    request.resolve('answer')
    await done
    expect(own.overlapped(mark)).toBe(true)
  })

  it('overlaps nothing for a read sent once every request has settled, answered or refused', async () => {
    const own = ownRequests()
    const answered = sentThrough(own)
    const refused = sentThrough(own)
    answered.request.resolve('answer')
    refused.request.reject(new Error('409'))
    await answered.done
    await refused.done
    const mark = own.mark()
    expect(own.overlapped(mark)).toBe(false)
  })

  it('hands back the request’s own answer and its own failure', async () => {
    const own = ownRequests()
    const answered = sentThrough(own)
    answered.request.resolve('answer')
    expect(await answered.tracked).toBe('answer')
    const refused = sentThrough(own)
    const why = new Error('409')
    refused.request.reject(why)
    await expect(refused.tracked).rejects.toBe(why)
  })
})

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { about, forgetAbout, shortCommit } from './about'
import { Unauthenticated, whenSignedOut } from './api'
import { ABOUT as ANSWER } from './testing/about'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

beforeEach(() => {
  forgetAbout()
})

afterEach(() => {
  vi.unstubAllGlobals()
  whenSignedOut(() => undefined)
})

describe('about', () => {
  it('asks the route once, and answers a later call without a request', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json(ANSWER))
    vi.stubGlobal('fetch', fetchMock)
    expect(await about()).toEqual(ANSWER)
    expect(await about()).toEqual(ANSWER)
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/about')
  })

  it('sends one request for two callers asking before it answers', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json(ANSWER))
    vi.stubGlobal('fetch', fetchMock)
    const [first, second] = await Promise.all([about(), about()])
    expect(first).toEqual(ANSWER)
    expect(second).toEqual(ANSWER)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('asks again after a failure, and answers with the second request', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(() => new Response('down', { status: 503 }))
      .mockImplementation(() => json(ANSWER))
    vi.stubGlobal('fetch', fetchMock)
    await expect(about()).rejects.toThrow('/api/about: 503 down')
    expect(await about()).toEqual(ANSWER)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('hands a 401 to the signed-out hook, as every guarded call does', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 401 })),
    )
    const signedOut = vi.fn()
    whenSignedOut(signedOut)
    await expect(about()).rejects.toBeInstanceOf(Unauthenticated)
    expect(signedOut).toHaveBeenCalledTimes(1)
  })
})

describe('shortCommit', () => {
  it('is the first seven characters of the hash', () => {
    expect(shortCommit(ANSWER.commit ?? '')).toBe('5ee5159')
  })
})

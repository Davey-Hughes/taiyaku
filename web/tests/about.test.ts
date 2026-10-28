import { afterEach, describe, expect, it, vi } from 'vitest'
import { about, shortCommit } from '../src/about'
import { Unauthenticated, whenSignedOut } from '../src/api'
import { ABOUT as ANSWER } from './support/about'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
  whenSignedOut(() => undefined)
})

describe('about', () => {
  // The read store shares a request and keeps the answer (`ABOUT` in
  // `pageReads.ts`); an answer held here would come back to its re-reads.
  it('sends a request to the route for every call, and holds no answer', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(() => json(ANSWER))
      .mockImplementation(() => json({ ...ANSWER, commit: null }))
    vi.stubGlobal('fetch', fetchMock)
    expect(await about()).toEqual(ANSWER)
    expect(await about()).toEqual({ ...ANSWER, commit: null })
    expect(fetchMock.mock.calls.map(([address]) => address)).toEqual([
      '/api/about',
      '/api/about',
    ])
  })

  it('fails in the route’s words, and answers the next call', async () => {
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

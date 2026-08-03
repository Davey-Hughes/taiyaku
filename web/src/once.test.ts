import { describe, expect, it } from 'vitest'
import { once } from './once'

describe('once', () => {
  it('runs the wrapped function exactly once for two callers already in flight', async () => {
    // Same shape as two concurrent `wasmResolver` calls racing `init()`
    // under React StrictMode's double-invoked effect: both callers ask
    // before either has had a chance to resolve.
    let calls = 0
    const fn = () => new Promise<number>((resolve) => setTimeout(() => resolve(++calls), 0))
    const ensure = once(fn)

    const a = ensure()
    const b = ensure()

    expect(await a).toBe(1)
    expect(await b).toBe(1)
    expect(calls).toBe(1)
  })

  it('keeps returning the first settled result for calls made afterwards', async () => {
    let calls = 0
    const fn = async () => ++calls
    const ensure = once(fn)

    await ensure()
    const result = await ensure()

    expect(result).toBe(1)
    expect(calls).toBe(1)
  })

  it('propagates a rejection to the caller', async () => {
    let calls = 0
    const fn = async () => {
      calls++
      throw new Error('boom')
    }
    const ensure = once(fn)

    await expect(ensure()).rejects.toThrow('boom')
    expect(calls).toBe(1)
  })

  it('retries on the next call after a rejection, and can succeed', async () => {
    let calls = 0
    const fn = async () => {
      calls++
      if (calls === 1) throw new Error('transient')
      return calls
    }
    const ensure = once(fn)

    await expect(ensure()).rejects.toThrow('transient')
    expect(calls).toBe(1)

    const result = await ensure()

    expect(result).toBe(2)
    expect(calls).toBe(2)
  })
})

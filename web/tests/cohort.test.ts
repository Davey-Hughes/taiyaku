import { describe, expect, it } from 'vitest'
import { createCohort, noCohort } from '../src/cohort'

/** A controllable promise — the same helper `decode.test.ts` uses over a fake image's `decode()`. */
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (err: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/**
 * A `Defer` the test fires by hand, so membership closes exactly where the
 * test says rather than on the microtask queue's own schedule. `createCohort`'s
 * default is `queueMicrotask`; nothing here depends on which, which is the
 * reason it is a parameter.
 */
function manualDefer() {
  const queued: (() => void)[] = []
  return {
    defer: (fn: () => void) => {
      queued.push(fn)
    },
    flush: () => {
      const now = queued.splice(0)
      for (const fn of now) fn()
    },
  }
}

describe('createCohort', () => {
  it('starts every job immediately, without waiting for the cohort to close', () => {
    const { defer } = manualDefer()
    const cohort = createCohort(defer)
    const started: string[] = []
    void cohort.run(() => {
      started.push('a')
      return Promise.resolve()
    })
    void cohort.run(() => {
      started.push('b')
      return Promise.resolve()
    })
    expect(started).toEqual(['a', 'b'])
  })

  it('holds the first job that settles until the second has settled too', async () => {
    const { defer, flush } = manualDefer()
    const cohort = createCohort(defer)
    const fast = deferred<void>()
    const slow = deferred<void>()
    const released: string[] = []

    const a = cohort.run(() => fast.promise).then(() => released.push('a'))
    const b = cohort.run(() => slow.promise).then(() => released.push('b'))
    flush()

    fast.resolve()
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()
    expect(released).toEqual([])

    slow.resolve()
    await Promise.all([a, b])
    expect(released).toEqual(['a', 'b'])
  })

  it('releases a member whose partner never joined, so a lone job is not stuck waiting for one', async () => {
    const { defer, flush } = manualDefer()
    const cohort = createCohort(defer)
    const only = deferred<void>()
    let released = false
    const p = cohort
      .run(() => only.promise)
      .then(() => {
        released = true
      })
    flush()
    only.resolve()
    await p
    expect(released).toBe(true)
  })

  it('does not hold a later job behind an earlier cohort that is still in flight', async () => {
    const { defer, flush } = manualDefer()
    const cohort = createCohort(defer)
    const stuck = deferred<void>()
    void cohort.run(() => stuck.promise)
    flush()

    // A fresh tick: this job belongs to its own cohort and must not inherit
    // the previous one's unsettled member.
    const later = deferred<void>()
    let released = false
    const p = cohort
      .run(() => later.promise)
      .then(() => {
        released = true
      })
    flush()
    later.resolve()
    await p
    expect(released).toBe(true)
  })

  it('releases the cohort even when one member rejects, rather than stranding the others', async () => {
    const { defer, flush } = manualDefer()
    const cohort = createCohort(defer)
    const bad = deferred<void>()
    const good = deferred<void>()
    const failing = cohort.run(() => bad.promise)
    let released = false
    const ok = cohort
      .run(() => good.promise)
      .then(() => {
        released = true
      })
    flush()

    bad.reject(new Error('undecodable'))
    good.resolve()
    await ok
    expect(released).toBe(true)
    await expect(failing).rejects.toThrow('undecodable')
  })

  it('turns a job that throws synchronously into a rejection, so the cohort still closes', async () => {
    const { defer, flush } = manualDefer()
    const cohort = createCohort(defer)
    const partner = deferred<void>()
    const thrown = cohort.run(() => {
      throw new Error('sync')
    })
    let released = false
    const ok = cohort
      .run(() => partner.promise)
      .then(() => {
        released = true
      })
    flush()
    partner.resolve()
    await ok
    expect(released).toBe(true)
    await expect(thrown).rejects.toThrow('sync')
  })

  it('resolves to its job’s own value', async () => {
    const { defer, flush } = manualDefer()
    const cohort = createCohort(defer)
    const p = cohort.run(() => Promise.resolve('page.jpg'))
    flush()
    await expect(p).resolves.toBe('page.jpg')
  })
})

describe('noCohort', () => {
  it('runs a job with no barrier at all', async () => {
    const started: string[] = []
    const slow = deferred<void>()
    void noCohort.run(() => {
      started.push('slow')
      return slow.promise
    })
    await expect(
      noCohort.run(() => {
        started.push('fast')
        return Promise.resolve('done')
      }),
    ).resolves.toBe('done')
    expect(started).toEqual(['slow', 'fast'])
  })
})

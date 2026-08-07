import { describe, expect, it } from 'vitest'
import {
  createDecodeQueue,
  decodeTo,
  presentWhenDecoded,
  type Decodable,
} from './decode'

/** A controllable promise, the same shape `motion.test.ts`'s stubbed rAF pair
 * gives a test over timing — here over a fake image's `decode()` instead. */
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (err: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/** A fake image whose `decode()` is a promise the test controls — no DOM, no jsdom. */
function fakeImage(decode: () => Promise<void>): Decodable {
  return { src: '', decode }
}

describe('decodeTo', () => {
  it('points the image at src before decoding it', () => {
    const image = fakeImage(() => Promise.resolve())
    void decodeTo(image, 'a.jpg')
    expect(image.src).toBe('a.jpg')
  })

  it('does not present a source until its decode has resolved', async () => {
    const d = deferred<void>()
    const image = fakeImage(() => d.promise)
    let settled = false
    const p = decodeTo(image, 'a.jpg').then(() => {
      settled = true
    })
    await Promise.resolve()
    await Promise.resolve()
    expect(settled).toBe(false)
    d.resolve()
    await p
    expect(settled).toBe(true)
  })

  it('still settles when decode rejects, rather than hanging on an undecodable source forever', async () => {
    const d = deferred<void>()
    const image = fakeImage(() => d.promise)
    const p = decodeTo(image, 'broken.jpg')
    d.reject(new Error('nope'))
    await expect(p).resolves.toBeUndefined()
  })
})

describe('createDecodeQueue', () => {
  it('starts jobs immediately up to the cap', () => {
    const queue = createDecodeQueue(2)
    const started: number[] = []
    const pending = [deferred<void>(), deferred<void>(), deferred<void>()]
    pending.forEach((d, i) => {
      void queue.run(() => {
        started.push(i)
        return d.promise
      })
    })
    expect(started).toEqual([0, 1])
  })

  it('starts the next queued job only once a running one settles', async () => {
    const queue = createDecodeQueue(1)
    const started: number[] = []
    const d0 = deferred<void>()
    const d1 = deferred<void>()
    void queue.run(() => {
      started.push(0)
      return d0.promise
    })
    void queue.run(() => {
      started.push(1)
      return d1.promise
    })
    expect(started).toEqual([0])
    d0.resolve()
    await Promise.resolve()
    await Promise.resolve()
    expect(started).toEqual([0, 1])
  })

  it('frees its slot on a rejection too, so one bad decode does not stall the queue', async () => {
    const queue = createDecodeQueue(1)
    const started: number[] = []
    const d0 = deferred<void>()
    const d1 = deferred<void>()
    const first = queue.run(() => {
      started.push(0)
      return d0.promise
    })
    void queue.run(() => {
      started.push(1)
      return d1.promise
    })
    d0.reject(new Error('boom'))
    await first.catch(() => {})
    await Promise.resolve()
    expect(started).toEqual([0, 1])
  })

  it('never exceeds the cap even as a burst of eight queues up behind it', () => {
    const queue = createDecodeQueue(3)
    let active = 0
    let peak = 0
    for (let i = 0; i < 8; i += 1) {
      void queue.run(() => {
        active += 1
        peak = Math.max(peak, active)
        return new Promise<void>(() => {
          // Never settles — this test only checks how many start, not how
          // the queue drains, so nothing here needs to resolve.
        })
      })
    }
    expect(peak).toBeLessThanOrEqual(3)
    expect(active).toBe(3)
  })

  it('clamps a zero concurrency to 1 rather than stalling every job forever', async () => {
    const queue = createDecodeQueue(0)
    let ran = false
    await queue.run(() => {
      ran = true
      return Promise.resolve()
    })
    expect(ran).toBe(true)
  })

  it('clamps a negative concurrency the same way', async () => {
    const queue = createDecodeQueue(-3)
    let ran = false
    await queue.run(() => {
      ran = true
      return Promise.resolve()
    })
    expect(ran).toBe(true)
  })

  it('clamps a non-finite concurrency (NaN from a hand-edited pref) to 1 rather than capping nothing', () => {
    const queue = createDecodeQueue(Number.NaN)
    const started: number[] = []
    for (let i = 0; i < 3; i += 1) {
      void queue.run(() => {
        started.push(i)
        return new Promise<void>(() => {
          // Never settles — only the immediate start count matters here.
        })
      })
    }
    expect(started).toEqual([0])
  })

  it('frees its concurrency slot even when a job throws synchronously instead of returning a rejected promise', async () => {
    const queue = createDecodeQueue(1)
    const first = queue.run<void>(() => {
      throw new Error('boom')
    })
    await first.catch(() => {})
    let ranSecond = false
    await queue.run(() => {
      ranSecond = true
      return Promise.resolve()
    })
    expect(ranSecond).toBe(true)
  })

  it('two independently created queues never contend for the same concurrency slot — a job stuck in one leaves the other free', async () => {
    // Pins the fix for the priority inversion a review found: `App.tsx` now
    // gives `PageView`'s swap decode its own queue, separate from the
    // prefetch window's, so a stale prefetch job holding its one queue's
    // slot can never delay the live page turn's decode in the other.
    const stale = createDecodeQueue(1)
    const live = createDecodeQueue(1)
    void stale.run(
      () =>
        new Promise<void>(() => {
          // Never settles — stands in for a prefetch job superseded mid-flight,
          // still holding its queue's one slot with nothing left to free it.
        }),
    )
    let ran = false
    await live.run(() => {
      ran = true
      return Promise.resolve()
    })
    expect(ran).toBe(true)
  })
})

describe('presentWhenDecoded', () => {
  it('does not present a source until its decode has resolved', async () => {
    const d = deferred<void>()
    const queue = createDecodeQueue(1)
    let shown = 'old.jpg'
    const p = presentWhenDecoded(
      () => fakeImage(() => d.promise),
      queue,
      'new.jpg',
      () => false,
      (src) => {
        shown = src
      },
    )
    await Promise.resolve()
    await Promise.resolve()
    expect(shown).toBe('old.jpg')
    d.resolve()
    await p
    expect(shown).toBe('new.jpg')
  })

  it('discards a decode that a later request has already superseded', async () => {
    const d = deferred<void>()
    const queue = createDecodeQueue(1)
    let shown = 'old.jpg'
    const p = presentWhenDecoded(
      () => fakeImage(() => d.promise),
      queue,
      'stale.jpg',
      () => true,
      (src) => {
        shown = src
      },
    )
    d.resolve()
    await p
    expect(shown).toBe('old.jpg')
  })

  it('creates a fresh image per call, so two overlapping requests never fight over one src', async () => {
    const made: Decodable[] = []
    const queue = createDecodeQueue(2)
    const d1 = deferred<void>()
    const d2 = deferred<void>()
    const makeImage = () => {
      const image = fakeImage(() =>
        made.length === 1 ? d1.promise : d2.promise,
      )
      made.push(image)
      return image
    }
    void presentWhenDecoded(
      makeImage,
      queue,
      'a.jpg',
      () => false,
      () => {},
    )
    void presentWhenDecoded(
      makeImage,
      queue,
      'b.jpg',
      () => false,
      () => {},
    )
    expect(made).toHaveLength(2)
    expect(made[0]?.src).toBe('a.jpg')
    expect(made[1]?.src).toBe('b.jpg')
  })
})

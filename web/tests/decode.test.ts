import { describe, expect, it } from 'vitest'
import { createCohort } from '../src/cohort'
import {
  createDecodeQueue,
  decodeTo,
  presentWhenDecoded,
  unboundedDecodeQueue,
  type Decodable,
} from '../src/decode'

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
    // Pins the separation: `App.tsx` gives `PageView`'s swap decode its own
    // queue, separate from the prefetch window's, so a stale prefetch job
    // holding its one queue's slot can never delay the live page turn's
    // decode in the other.
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

  // **Ordering lives in the caller.** `PageView` drops anything older than
  // what it is already drawing, so this function presents whatever it decoded
  // and the reader sees pages go by during a flick instead of one frozen
  // spread. What it must never do is present a job it *skipped*, which the
  // tests below pin.
  it('presents a decode a later request superseded, leaving ordering to the caller', async () => {
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
    expect(shown).toBe('stale.jpg')
  })

  // The flick case. A job that has gone stale by the time the queue reaches it
  // must cost nothing — not the decode, and not the fetch that assigning `src`
  // would trigger.
  it('never touches a superseded source that newer work is waiting behind, so a passed-over page costs no fetch and no decode', async () => {
    const queue = createDecodeQueue(1)
    const first = deferred<void>()
    const built: string[] = []
    const makeImage = (label: string) => () => {
      built.push(label)
      return fakeImage(() =>
        label === 'held' ? first.promise : Promise.resolve(),
      )
    }

    // Occupies the queue's single slot.
    void presentWhenDecoded(
      makeImage('held'),
      queue,
      'held.jpg',
      () => false,
      () => {},
    )
    // Queued behind it and superseded before it runs — with a third job
    // waiting behind *it*, which is what makes drawing it pointless.
    const skipped = presentWhenDecoded(
      makeImage('passed-over'),
      queue,
      'passed-over.jpg',
      () => true,
      () => {},
    )
    void presentWhenDecoded(
      makeImage('newest'),
      queue,
      'newest.jpg',
      () => false,
      () => {},
    )

    expect(built).toEqual(['held'])
    first.resolve()
    await skipped
    // `passed-over` never built an image, so it never assigned a `src` and
    // never asked the network for anything. `newest` did.
    expect(built).toEqual(['held', 'newest'])
  })

  // The other half, and the one that makes a flick visible: a superseded frame
  // with nothing waiting behind it is still the freshest picture that exists,
  // so it is decoded and shown.
  it('decodes and presents a superseded source when nothing newer is waiting', async () => {
    const queue = createDecodeQueue(1)
    const shown: string[] = []
    await presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      queue,
      'passed-over.jpg',
      () => true,
      (src) => shown.push(src),
    )
    expect(shown).toEqual(['passed-over.jpg'])
  })

  it('settles a skipped job rather than leaving its caller waiting forever, and shows nothing for it', async () => {
    const queue = createDecodeQueue(1)
    const held = deferred<void>()
    void queue.run(() => held.promise)
    let presented = false
    const skipped = presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      queue,
      'gone.jpg',
      () => true,
      () => {
        presented = true
      },
    )
    // Newer work behind it, so it is skipped rather than drawn.
    void queue.run(() => Promise.resolve())
    held.resolve()
    await expect(skipped).resolves.toBeUndefined()
    // A skipped job pointed no image at anything, so presenting its `src` would
    // show an undecoded source.
    expect(presented).toBe(false)
  })

  // **Both halves of a turn take the same path.** A skipped member and a
  // decoded member in one cohort would let a pair show two generations at
  // once, so `decideOnce` makes the skip/decode call once per cohort. The
  // test that follows pins that directly.
  it('gives every member of a cohort the same skip decision, so a pair cannot split', async () => {
    const cohort = createCohort()
    const shown: string[] = []
    const busy = createDecodeQueue(1)
    const holding = deferred<void>()
    void busy.run(() => holding.promise)

    // Both halves of one turn, both superseded, with newer work queued behind.
    const left = presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      busy,
      'left.jpg',
      () => true,
      (src) => shown.push(src),
      cohort,
    )
    const right = presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      busy,
      'right.jpg',
      () => true,
      (src) => shown.push(src),
      cohort,
    )
    void busy.run(() => Promise.resolve())
    holding.resolve()

    await Promise.all([left, right])
    // Both skipped, or both drawn — never one of each.
    expect(shown.length === 0 || shown.length === 2).toBe(true)
  })

  it('decides once even when the two members would answer differently on their own', async () => {
    const cohort = createCohort()
    const shown: string[] = []
    const busy = createDecodeQueue(1)
    const holding = deferred<void>()
    void busy.run(() => holding.promise)

    // One half superseded, the other not — the split that would tear.
    const stale = presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      busy,
      'stale.jpg',
      () => true,
      (src) => shown.push(src),
      cohort,
    )
    const fresh = presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      busy,
      'fresh.jpg',
      () => false,
      (src) => shown.push(src),
      cohort,
    )
    void busy.run(() => Promise.resolve())
    holding.resolve()

    await Promise.all([stale, fresh])
    expect(shown.length === 0 || shown.length === 2).toBe(true)
  })

  it('still decodes a job that is current when the queue reaches it', async () => {
    const queue = createDecodeQueue(1)
    const shown: string[] = []
    await presentWhenDecoded(
      () => fakeImage(() => Promise.resolve()),
      queue,
      'wanted.jpg',
      () => false,
      (src) => shown.push(src),
    )
    expect(shown).toEqual(['wanted.jpg'])
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

  // `two` mode's pair, at the seam `PageView` actually uses: each half would
  // otherwise present the moment its own decode settled, and the halves would
  // not arrive together. See the `cohort.ts` entry in
  // `docs/superpowers/notes/2026-10-29-code-history-web-src.md`.
  it('holds a decoded page until the page beside it has decoded too, when both share a cohort', async () => {
    const cohort = createCohort()
    const fast = deferred<void>()
    const slow = deferred<void>()
    const shown: string[] = []

    const left = presentWhenDecoded(
      () => fakeImage(() => fast.promise),
      unboundedDecodeQueue,
      'left.jpg',
      () => false,
      (src) => shown.push(src),
      cohort,
    )
    const right = presentWhenDecoded(
      () => fakeImage(() => slow.promise),
      unboundedDecodeQueue,
      'right.jpg',
      () => false,
      (src) => shown.push(src),
      cohort,
    )

    fast.resolve()
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()
    expect(shown).toEqual([])

    slow.resolve()
    await Promise.all([left, right])
    expect(shown).toEqual(['left.jpg', 'right.jpg'])
  })

  it('still presents a lone page in a cohort, so single mode pays no barrier', async () => {
    const cohort = createCohort()
    const only = deferred<void>()
    const shown: string[] = []
    const p = presentWhenDecoded(
      () => fakeImage(() => only.promise),
      unboundedDecodeQueue,
      'only.jpg',
      () => false,
      (src) => shown.push(src),
      cohort,
    )
    only.resolve()
    await p
    expect(shown).toEqual(['only.jpg'])
  })

  // **Presenting both halves, superseded or not, is deliberate, and it is
  // the cost of showing pages go by.** A superseded half is drawn when
  // nothing newer is queued, so a pair whose two cells make that decision
  // differently can briefly show two generations at once. In a real turn both
  // cells change together, join one cohort and decode together, so they move
  // in lockstep; this is reachable only when one cell's job is skipped and
  // its partner's is not. See `docs/notes.md`.
  it('presents both halves of a cohort once it releases, superseded or not', async () => {
    const cohort = createCohort()
    const fast = deferred<void>()
    const slow = deferred<void>()
    const shown: string[] = []
    let staleLeft = false

    const left = presentWhenDecoded(
      () => fakeImage(() => fast.promise),
      unboundedDecodeQueue,
      'left.jpg',
      () => staleLeft,
      (src) => shown.push(src),
      cohort,
    )
    const right = presentWhenDecoded(
      () => fakeImage(() => slow.promise),
      unboundedDecodeQueue,
      'right.jpg',
      () => false,
      (src) => shown.push(src),
      cohort,
    )

    fast.resolve()
    staleLeft = true
    slow.resolve()
    await Promise.all([left, right])
    expect(shown).toEqual(['left.jpg', 'right.jpg'])
  })
})

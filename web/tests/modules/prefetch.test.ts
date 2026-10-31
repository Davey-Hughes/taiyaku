import { describe, expect, it } from 'vitest'
import type { Display } from '../../src/reading'
import {
  groupWindow,
  prefetchModule,
  prefetchTargets,
  superseded,
  window,
} from '../../src/modules/prefetch'

describe('window', () => {
  it('takes N ahead and M behind', () => {
    expect(window(10, 100, 2, 1)).toEqual([9, 11, 12])
  })

  it('clamps at the start of a volume', () => {
    expect(window(0, 100, 2, 2)).toEqual([1, 2])
  })

  it('clamps at the end', () => {
    expect(window(99, 100, 2, 1)).toEqual([98])
  })

  it('never includes the current slot, which is already on screen', () => {
    expect(window(5, 100, 3, 3)).not.toContain(5)
  })

  it('is empty when both depths are zero, so the module can be switched off', () => {
    expect(window(5, 100, 0, 0)).toEqual([])
  })

  it('orders nearest-first, so a cancelled flick has fetched the useful ones', () => {
    expect(window(10, 100, 3, 0)).toEqual([11, 12, 13])
  })

  it('interleaves ahead and behind by distance — both slots one away before either slot two away', () => {
    expect(window(10, 100, 2, 2)).toEqual([9, 11, 8, 12])
  })

  it('is empty on a single-slot volume, with nothing either side to fetch', () => {
    expect(window(0, 1, 3, 3)).toEqual([])
  })
})

describe('prefetchModule', () => {
  // One group each way, not two: reaching further is slower, because a decoded
  // page is ~26 MB and a wide window evicts the pages the reader is arriving
  // at. The table is the `prefetch.ts` entry in
  // `docs/superpowers/notes/2026-10-29-code-history-web-src.md`.
  it('defaults to enabled, one group either side, and a capped decode concurrency', () => {
    expect(prefetchModule.defaults).toEqual({
      enabled: true,
      ahead: 1,
      behind: 1,
      maxConcurrent: 3,
    })
  })

  it('claims no keys — tuned in settings, not pressed mid-read', () => {
    expect(prefetchModule.keys ?? []).toEqual([])
  })
})

describe('groupWindow', () => {
  /** `two` mode's grouping over eight ordinary slots. */
  const paired = [
    { slots: [0, 1] },
    { slots: [2, 3] },
    { slots: [4, 5] },
    { slots: [6, 7] },
  ]

  it('reaches whole groups, so the pair a turn lands on is never half-fetched', () => {
    // From the group holding slots 2 and 3. `window`'s slot-space answer
    // here is [1, 3, 4] — slot 5 missing, which is the second half of the
    // very next group.
    expect(groupWindow(paired, 2, 2, 1)).toEqual([0, 1, 4, 5, 6, 7])
  })

  it('leaves out the group the reader is standing in, both slots of it', () => {
    expect(groupWindow(paired, 2, 2, 1)).not.toContain(2)
    expect(groupWindow(paired, 2, 2, 1)).not.toContain(3)
  })

  it('answers the same as the slot window when every group is one slot', () => {
    const single = [0, 1, 2, 3, 4, 5].map((s) => ({ slots: [s] }))
    expect(groupWindow(single, 2, 2, 1)).toEqual(window(2, 6, 2, 1))
  })

  it('takes the nearer group first, behind before ahead at each tier', () => {
    expect(groupWindow(paired, 4, 2, 2)).toEqual([2, 3, 6, 7, 0, 1])
  })

  it('clamps at both ends rather than naming a group that is not there', () => {
    expect(groupWindow(paired, 0, 2, 1)).toEqual([2, 3, 4, 5])
    expect(groupWindow(paired, 6, 2, 1)).toEqual([4, 5])
  })

  it('reaches past a solo group rather than stopping at it', () => {
    // What the aligned pairing produces around a joined spread: one group of
    // one, with ordinary pairs either side.
    const mixed = [
      { slots: [0, 1] },
      { slots: [2] },
      { slots: [3, 4] },
      { slots: [5, 6] },
    ]
    expect(groupWindow(mixed, 2, 2, 1)).toEqual([0, 1, 3, 4, 5, 6])
  })

  it('names a slot once even when neighbouring groups share it', () => {
    const overlapping = [{ slots: [0, 1] }, { slots: [2, 3] }, { slots: [3] }]
    const got = groupWindow(overlapping, 0, 2, 1)
    expect(got).toEqual([...new Set(got)])
  })

  it('prefetches nothing for a slot no group holds, rather than guessing one', () => {
    expect(groupWindow(paired, 99, 2, 1)).toEqual([])
  })

  it('prefetches nothing when both depths are zero', () => {
    expect(groupWindow(paired, 2, 0, 0)).toEqual([])
  })
})

describe('prefetchTargets', () => {
  const paired = [{ slots: [0, 1] }, { slots: [2, 3] }, { slots: [4, 5] }]
  const prefs = { ...prefetchModule.defaults }
  const at: Display = { kind: 'at', edition: 'en', slot: 2, via: 'identity' }
  // A page index that is not the slot index, so nothing below can pass by
  // reading one for the other: slot n is page n + 10.
  const slotAt = (_edition: string, index: number) => ({ page: index + 10 })

  it('names each neighbouring slot with the page its URL is built from', () => {
    expect(prefetchTargets(paired, at, prefs, 'two', slotAt)).toEqual([
      { slot: 0, page: 10 },
      { slot: 1, page: 11 },
      { slot: 4, page: 14 },
      { slot: 5, page: 15 },
    ])
  })

  it('asks slotAt for the edition being looked at, not the authority', () => {
    const seen: string[] = []
    prefetchTargets(paired, at, prefs, 'two', (edition, index) => {
      seen.push(edition)
      return { page: index }
    })
    expect(new Set(seen)).toEqual(new Set(['en']))
  })

  // Every group is already mounted in scroll mode, so a window around the
  // current slot is not a meaningful subset to warm ahead of the rest.
  it('warms nothing in scroll mode', () => {
    expect(prefetchTargets(paired, at, prefs, 'scroll', slotAt)).toEqual([])
  })

  it('warms nothing while the module is off', () => {
    expect(
      prefetchTargets(paired, at, { ...prefs, enabled: false }, 'two', slotAt),
    ).toEqual([])
  })

  // A gap has nothing paired to prefetch around.
  it('warms nothing in a gap', () => {
    const gap: Display = {
      kind: 'gap',
      edition: 'raw',
      from: { edition: 'en', slot: 2 },
    }
    expect(prefetchTargets(paired, gap, prefs, 'two', slotAt)).toEqual([])
  })

  // No fallback to the slot-space `window`, which counts the wrong unit in
  // `two` mode — an unassigned grouping warms nothing at all.
  it('warms nothing before a grouping has been assigned', () => {
    expect(prefetchTargets(undefined, at, prefs, 'two', slotAt)).toEqual([])
  })

  // A prefetch miss is cheaper than a fetch of the wrong page.
  it('skips a slot the resolver cannot place rather than guessing at it', () => {
    const patchy = (_edition: string, index: number) =>
      index === 1 ? undefined : { page: index + 10 }
    expect(prefetchTargets(paired, at, prefs, 'two', patchy)).toEqual([
      { slot: 0, page: 10 },
      { slot: 4, page: 14 },
      { slot: 5, page: 15 },
    ])
  })

  // The depths are the module's, and `groupWindow`'s nearest-group-first
  // interleaving is what decides the order.
  it('honours ahead and behind, nearest group first', () => {
    const single = [0, 1, 2, 3, 4, 5].map((slot) => ({ slots: [slot] }))
    const here: Display = {
      kind: 'at',
      edition: 'en',
      slot: 3,
      via: 'identity',
    }
    expect(
      prefetchTargets(
        single,
        here,
        { ...prefs, ahead: 2, behind: 1 },
        'single',
        slotAt,
      ).map((t) => t.slot),
    ).toEqual([2, 4, 5])
  })
})

describe('superseded', () => {
  const queuedFor = { edition: 'en', slot: 7 }
  const atSame: Display = {
    kind: 'at',
    edition: 'en',
    slot: 7,
    via: 'identity',
  }

  it('is not superseded while the reader is still where the window was built', () => {
    expect(superseded(queuedFor, atSame)).toBe(false)
  })

  // Strict equality on the slot, not proximity: the position the reader moved
  // *to* has queued its own window, so nothing is lost by dropping this one.
  it('is superseded by a move of a single slot', () => {
    expect(superseded(queuedFor, { ...atSame, slot: 8 })).toBe(true)
  })

  it('is superseded by a swap to another edition at the same slot', () => {
    expect(superseded(queuedFor, { ...atSame, edition: 'raw' })).toBe(true)
  })

  it('is superseded in a gap', () => {
    expect(
      superseded(queuedFor, {
        kind: 'gap',
        edition: 'raw',
        from: { edition: 'en', slot: 7 },
      }),
    ).toBe(true)
  })

  // What stops a queue draining into a reader that has gone.
  it('is superseded with no session to ask', () => {
    expect(superseded(queuedFor, undefined)).toBe(true)
  })

  // `via` is how the position was reached, not where it is — a window queued
  // before a swap resolved onto the same slot is still the right window.
  it('does not read anything but the edition and the slot', () => {
    expect(superseded(queuedFor, { ...atSame, via: 'direct' })).toBe(false)
  })
})

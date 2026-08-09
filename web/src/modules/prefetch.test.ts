import { describe, expect, it } from 'vitest'
import { groupWindow, prefetchModule, window } from './prefetch'

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
  // One group each way, not two — these count groups since `groupWindow`, and
  // reaching further measured *slower*: a decoded page is ~26 MB, so a wide
  // window evicts the pages the reader is arriving at. See the module's own
  // doc for the table.
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
    // The shipped defaults, from the group holding slots 2 and 3. `window`'s
    // slot-space answer here is [1, 3, 4] — slot 5 missing, which is the
    // second half of the very next group.
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

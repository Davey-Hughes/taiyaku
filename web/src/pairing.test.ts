import { describe, expect, it } from 'vitest'
import { groups } from './pairing'
import { bSlots, whole } from './fixtures'

describe('single', () => {
  it('is one slot per group', () => {
    expect(groups(whole(3), 'single', 'rtl')).toEqual([
      { slots: [0] }, { slots: [1] }, { slots: [2] },
    ])
  })

  it('shows a joined spread as one wide tile, with the slots either side their own groups', () => {
    // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page.
    expect(groups(bSlots, 'single', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2, 3], spread: true },
      { slots: [4] },
      { slots: [5] },
      { slots: [6] },
      { slots: [7] },
      { slots: [8] },
    ])
  })
})

describe('two-page', () => {
  it('pairs slots (0,1), (2,3), …', () => {
    expect(groups(whole(4), 'two', 'rtl')).toEqual([{ slots: [0, 1] }, { slots: [2, 3] }])
  })

  it('leaves a trailing odd slot alone rather than pairing it with nothing', () => {
    expect(groups(whole(3), 'two', 'rtl')).toEqual([{ slots: [0, 1] }, { slots: [2] }])
  })

  it('shows a joined spread as one wide tile, not as two paired halves', () => {
    // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page.
    // The spread is its own group and it restarts pairing after itself,
    // which is why slot 4 opens a new pair rather than joining slot 3.
    expect(groups(bSlots, 'two', 'rtl')).toEqual([
      { slots: [0, 1] },
      { slots: [2, 3], spread: true },
      { slots: [4, 5] },
      { slots: [6, 7] },
      { slots: [8] },
    ])
  })

  it('does not split a spread that starts mid-pair', () => {
    // 0, 1R, 1L, 2 — the spread begins at slot 1, the second half of the
    // first would-be pair.
    const odd = [
      { page: 0 },
      { page: 1, half: 'R' as const }, { page: 1, half: 'L' as const },
      { page: 2 },
    ]
    expect(groups(odd, 'two', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1, 2], spread: true },
      { slots: [3] },
    ])
  })
})

describe('scroll', () => {
  it('is one slot per group, like single — the difference is layout, not grouping', () => {
    expect(groups(whole(3), 'scroll', 'rtl')).toEqual([
      { slots: [0] }, { slots: [1] }, { slots: [2] },
    ])
  })

  it('shows a joined spread as one wide tile here too, not as two separate scroll rows', () => {
    // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page.
    expect(groups(bSlots, 'scroll', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2, 3], spread: true },
      { slots: [4] },
      { slots: [5] },
      { slots: [6] },
      { slots: [7] },
      { slots: [8] },
    ])
  })
})

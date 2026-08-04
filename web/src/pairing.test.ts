import { describe, expect, it } from 'vitest'
import { cellsFor, groups } from './pairing'
import { bSlots, odd, whole } from './fixtures'

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
    // odd: 0, 1R, 1L, 2 — the spread begins at slot 1, the second half of
    // the first would-be pair.
    expect(groups(odd, 'two', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1, 2], spread: true },
      { slots: [3] },
    ])
  })
})

describe('two-page with a hand-inserted break', () => {
  it('restarts pairing at the break', () => {
    expect(groups(whole(6), 'two', 'rtl', [3])).toEqual([
      { slots: [0, 1] },
      { slots: [2] },
      { slots: [3, 4] },
      { slots: [5] },
    ])
  })

  it('ignores a break in single and scroll modes, where there is no parity', () => {
    expect(groups(whole(4), 'single', 'rtl', [2])).toEqual(
      groups(whole(4), 'single', 'rtl', []),
    )
  })

  it('composes with a spread that starts mid-pair — the spread still forces its own restart, and a break elsewhere forces its own', () => {
    // odd extended by a trailing plain page: 0, 1R, 1L, 2, 3 — same
    // mid-pair spread as 'does not split a spread that starts mid-pair',
    // with a slot afterward for a break to act on. Without a break, slots 3
    // and 4 pair normally: [{0}, {1,2,spread}, {3,4}]. Break 4 leaves the
    // spread's own restart untouched (nextIsSpreadStart still puts slot 0
    // alone) while separately forcing slot 3 alone too (nextIsBreak) — the
    // two mechanisms firing at different positions within one call, which
    // is the composition nothing in the suite exercised before.
    const ext = [...odd, { page: 3 }]
    expect(groups(ext, 'two', 'rtl', [4])).toEqual([
      { slots: [0] },
      { slots: [1, 2], spread: true },
      { slots: [3] },
      { slots: [4] },
    ])
  })

  it('does not split a joined spread even when a break falls inside it, and still honours a break elsewhere', () => {
    // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page.
    // With no breaks this pairs (0,1), spread(2,3), (4,5), (6,7), 8 alone.
    // Break 3 falls on the spread's own second slot and is absorbed — the
    // spread still wins. Break 5 lands on a slot the walk actually
    // revisits and forces slot 4 alone, which cascades: (5,6) then (7,8)
    // pair up instead of (4,5), (6,7), 8 alone.
    //
    // (The review's suggested [3, 6] was tried first: 6 turns out to land
    // on a position the walk never revisits as a "current" index once the
    // spread has put it back on even parity, so it has no effect at all —
    // groups(bSlots, 'two', 'rtl', [3, 6]) equals the no-breaks case
    // exactly, which would pass even if breaks were entirely unwired. 5 is
    // used instead so the test actually proves the mechanism does
    // something.)
    expect(groups(bSlots, 'two', 'rtl', [3, 5])).toEqual([
      { slots: [0, 1] },
      { slots: [2, 3], spread: true },
      { slots: [4] },
      { slots: [5, 6] },
      { slots: [7, 8] },
    ])
  })
})

describe('cellsFor', () => {
  it('splits an ordinary pair into two equal-width cells, ltr in slot order', () => {
    expect(cellsFor({ slots: [2, 3] }, 'ltr')).toEqual({ wide: false, first: 2, second: 3 })
  })

  it('reverses an ordinary pair for rtl', () => {
    expect(cellsFor({ slots: [2, 3] }, 'rtl')).toEqual({ wide: false, first: 3, second: 2 })
  })

  it('puts a joined spread wide in the first cell, with its real second half mirrored into the second', () => {
    expect(cellsFor({ slots: [4, 5], spread: true }, 'ltr')).toEqual({ wide: true, first: 4, second: 5 })
  })

  it('does not reorder a spread for rtl — it has no left/right to reorder', () => {
    expect(cellsFor({ slots: [4, 5], spread: true }, 'rtl')).toEqual({ wide: true, first: 4, second: 5 })
  })

  it('mirrors a solo slot (no pairing partner) into both cells, wide', () => {
    expect(cellsFor({ slots: [7] }, 'ltr')).toEqual({ wide: true, first: 7, second: 7 })
  })

  it('mirrors a solo slot the same way regardless of reading direction', () => {
    expect(cellsFor({ slots: [7] }, 'rtl')).toEqual({ wide: true, first: 7, second: 7 })
  })

  it('keeps the same two positions stable across a shape change at a spread boundary', () => {
    // The exact turn that used to remount: leaving an ordinary pair for the
    // spread that interrupts it. `first` carries through as the same slot
    // family (the position nearer the start of the group), and both calls
    // return `wide` alongside a fully-populated `second` — nothing here
    // ever omits a cell, which is what let `App.tsx` stop switching element
    // types at this position.
    const pair = cellsFor({ slots: [2, 3] }, 'ltr')
    const spread = cellsFor({ slots: [4, 5], spread: true }, 'ltr')
    expect(pair).toEqual({ wide: false, first: 2, second: 3 })
    expect(spread).toEqual({ wide: true, first: 4, second: 5 })
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

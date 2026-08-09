import { describe, expect, it } from 'vitest'
import {
  cellOf,
  cellsFor,
  correctionTiles,
  type Group,
  groups,
  stepToGroup,
} from './pairing'
import { bSlots, odd, whole } from './fixtures'

/** Slots 0..n-1, with `spreads` naming the pages that are joined. */
function slotsWith(pages: number, spreads: number[]) {
  const out: { page: number; half?: 'L' | 'R' }[] = []
  for (let page = 0; page < pages; page++) {
    if (spreads.includes(page)) {
      out.push({ page, half: 'R' }, { page, half: 'L' })
    } else {
      out.push({ page })
    }
  }
  return out
}

describe('single', () => {
  it('is one slot per group', () => {
    expect(groups(whole(3), 'single', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2] },
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
    expect(groups(whole(4), 'two', 'rtl')).toEqual([
      { slots: [0, 1] },
      { slots: [2, 3] },
    ])
  })

  it('leaves a trailing odd slot alone rather than pairing it with nothing', () => {
    expect(groups(whole(3), 'two', 'rtl')).toEqual([
      { slots: [0, 1] },
      { slots: [2] },
    ])
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
    expect(cellsFor({ slots: [2, 3] }, 'ltr')).toEqual({
      wide: false,
      alone: false,
      first: 2,
      second: 3,
    })
  })

  it('reverses an ordinary pair for rtl', () => {
    expect(cellsFor({ slots: [2, 3] }, 'rtl')).toEqual({
      wide: false,
      alone: false,
      first: 3,
      second: 2,
    })
  })

  it('puts a joined spread wide in the first cell, with its real second half mirrored into the second', () => {
    expect(cellsFor({ slots: [4, 5], spread: true }, 'ltr')).toEqual({
      wide: true,
      alone: true,
      first: 4,
      second: 5,
    })
  })

  it('does not reorder a spread for rtl — it has no left/right to reorder', () => {
    expect(cellsFor({ slots: [4, 5], spread: true }, 'rtl')).toEqual({
      wide: true,
      alone: true,
      first: 4,
      second: 5,
    })
  })

  // **Contract changed with the low-speed flicker fix.** This used to expect
  // `wide: true` — an unpartnered page drawn across the whole group, exactly as
  // a joined spread is. It is not a spread: it is an ordinary page that happens
  // to have no partner, and drawing it full width collapsed the layout and
  // expanded it again on consecutive turns wherever the aligned pairing left
  // one. It keeps its own cell now; `alone` is what tells `App.tsx` the other
  // one holds nothing but a mirror.
  it('mirrors a solo slot (no pairing partner) into both cells, but is not wide', () => {
    expect(cellsFor({ slots: [7] }, 'ltr')).toEqual({
      wide: false,
      alone: true,
      first: 7,
      second: 7,
    })
  })

  it('mirrors a solo slot the same way regardless of reading direction', () => {
    expect(cellsFor({ slots: [7] }, 'rtl')).toEqual({
      wide: false,
      alone: true,
      first: 7,
      second: 7,
    })
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
    expect(pair).toEqual({ wide: false, alone: false, first: 2, second: 3 })
    expect(spread).toEqual({ wide: true, alone: true, first: 4, second: 5 })
  })

  // The property the split exists for: crossing an unpartnered page must not
  // change how wide the drawn page is, because that is what the reader sees as
  // the layout jumping.
  it('draws an unpartnered page at the same width as a paired one, so crossing it changes no geometry', () => {
    expect(cellsFor({ slots: [7] }, 'ltr').wide).toBe(
      cellsFor({ slots: [2, 3] }, 'ltr').wide,
    )
    // A real spread is the one thing that does change it.
    expect(cellsFor({ slots: [4, 5], spread: true }, 'ltr').wide).toBe(true)
  })
})

describe('cellOf', () => {
  it('names the cell an ordinary pair puts a slot in, ltr', () => {
    expect(cellOf({ slots: [2, 3] }, 'ltr', 2)).toBe('first')
    expect(cellOf({ slots: [2, 3] }, 'ltr', 3)).toBe('second')
  })

  // The half the swap would land on if reading direction were ignored is the
  // *other* one, which is the whole reason this reads `cellsFor` rather than
  // comparing against `group.slots[0]`.
  it('reverses with rtl, because the cells do', () => {
    expect(cellOf({ slots: [2, 3] }, 'rtl', 2)).toBe('second')
    expect(cellOf({ slots: [2, 3] }, 'rtl', 3)).toBe('first')
  })

  it('is `only` for a joined spread, either half and either direction', () => {
    const spread: Group = { slots: [4, 5], spread: true }
    expect(cellOf(spread, 'ltr', 4)).toBe('only')
    expect(cellOf(spread, 'ltr', 5)).toBe('only')
    expect(cellOf(spread, 'rtl', 4)).toBe('only')
  })

  // **Contract changed, and the old expectation was a real geometry bug.**
  // This asserted `'only'` for an unpartnered page. `viewport.ts`'s `cellBox`
  // reads `'only'` as *the page fills the group* and returns `WHOLE`; since an
  // unpartnered page keeps its own half-width cell, that mapped a zoom rect
  // against twice the width the page actually covers. `'only'` is the joined
  // spread alone.
  it('is `first` for an unpartnered page, which occupies one cell rather than the group', () => {
    expect(cellOf({ slots: [7] }, 'rtl', 7)).toBe('first')
    expect(cellOf({ slots: [7] }, 'ltr', 7)).toBe('first')
  })

  it('is still `only` for a joined spread, which does fill the group', () => {
    expect(cellOf({ slots: [4, 5], spread: true }, 'rtl', 4)).toBe('only')
  })
})

describe('correctionTiles', () => {
  it('is one group per slot, ignoring pairing entirely', () => {
    expect(correctionTiles(whole(3))).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2] },
    ])
  })

  // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page, joined
  // into a single `spread: true` group by `groups`. `correctionTiles` is the
  // fix for I4: a reader correcting has to be able to name either half on
  // its own, so this keeps them apart rather than folding them into one
  // unaddressable two-slot tile.
  it('does not join a spread into one tile — each half is its own group', () => {
    expect(correctionTiles(bSlots)).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2] },
      { slots: [3] },
      { slots: [4] },
      { slots: [5] },
      { slots: [6] },
      { slots: [7] },
      { slots: [8] },
    ])
  })

  it('does not pair slots even though two-page mode would', () => {
    // Contrast with groups(whole(4), 'two', 'rtl'), which pairs (0,1), (2,3).
    expect(correctionTiles(whole(4))).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2] },
      { slots: [3] },
    ])
  })
})

describe('scroll', () => {
  it('is one slot per group, like single — the difference is layout, not grouping', () => {
    expect(groups(whole(3), 'scroll', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1] },
      { slots: [2] },
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

describe('stepToGroup', () => {
  const rtl = 'rtl' as const

  /**
   * The defect this exists for, reported from a manual pass: a joined spread
   * is two slots drawn as one image, so stepping by slot showed the same
   * picture twice and the reader pressed the arrow again wondering why
   * nothing moved.
   */
  it('crosses a joined spread in one press', () => {
    const slots = slotsWith(6, [2])
    const g = groups(slots, 'single', rtl)
    // Slot 2 is the spread's R half, slot 3 its L half, slot 4 is page 3.
    expect(stepToGroup(g, 2, 1)).toBe(4)
  })

  /** And back the same way, rather than landing inside the spread. */
  it('crosses it backwards in one press too', () => {
    const slots = slotsWith(6, [2])
    const g = groups(slots, 'single', rtl)
    expect(stepToGroup(g, 4, -1)).toBe(2)
  })

  /**
   * Landing on a group's *first* slot is what makes a swap predictable: both
   * halves of a spread draw the same image but resolve differently, so a
   * reader who cannot see which half they are on should always be on the one
   * the group names.
   */
  it('lands on the first slot of the group it moves to', () => {
    const slots = slotsWith(6, [2])
    const g = groups(slots, 'single', rtl)
    // From inside the spread's second half, forward is still the next group.
    expect(stepToGroup(g, 3, 1)).toBe(4)
  })

  /** Ordinary pages still move one at a time. */
  it('moves one page at a time where nothing is joined', () => {
    const slots = slotsWith(5, [])
    const g = groups(slots, 'single', rtl)
    expect(stepToGroup(g, 1, 1)).toBe(2)
    expect(stepToGroup(g, 3, -1)).toBe(2)
  })

  /**
   * In `two` mode a group is a pair, so one press turns both pages — stepping
   * by slot advanced the pair by one and redrew half of what was already on
   * screen.
   */
  it('turns a whole pair in two-page mode', () => {
    const slots = slotsWith(6, [])
    const g = groups(slots, 'two', rtl)
    expect(stepToGroup(g, 0, 1)).toBe(2)
    expect(stepToGroup(g, 2, 1)).toBe(4)
  })

  /** Both ends stop rather than wrapping or running off. */
  it('stops at both ends', () => {
    const slots = slotsWith(4, [])
    const g = groups(slots, 'single', rtl)
    expect(stepToGroup(g, 0, -1)).toBe(0)
    expect(stepToGroup(g, 3, 1)).toBe(3)
  })

  /**
   * O28. The old clamp kept the reader in the group they were already in and
   * the first-slot landing then moved them to its start — backward, on a
   * forward press. None of these presses change the picture, since both slots
   * are inside one drawn group; what moved was the slot a swap answers from.
   *
   * `two` mode needs no spread for this, because a pair is a group too.
   */
  it('leaves the slot alone going forward off the end', () => {
    const pairs = groups(slotsWith(6, []), 'two', rtl)
    // [[0,1],[2,3],[4,5]] — slot 5 is the last pair's second page.
    expect(stepToGroup(pairs, 5, 1)).toBe(5)

    for (const mode of ['single', 'two', 'scroll'] as const) {
      // Six pages, the last one a joined spread: slot 6 is its second half.
      const g = groups(slotsWith(6, [5]), mode, rtl)
      expect(stepToGroup(g, 6, 1)).toBe(6)
    }
  })

  /**
   * The mirror of it, at the start. O28's entry recorded that backward here
   * "already returns the slot unchanged", which held only from a group's
   * *first* slot — from the second it answered 0, invisibly, in the direction
   * the reader had pressed.
   */
  it('leaves the slot alone going backward off the start', () => {
    const pairs = groups(slotsWith(6, []), 'two', rtl)
    expect(stepToGroup(pairs, 1, -1)).toBe(1)

    for (const mode of ['single', 'two', 'scroll'] as const) {
      // Six pages, the first one a joined spread: slot 1 is its second half.
      const g = groups(slotsWith(6, [0]), mode, rtl)
      expect(stepToGroup(g, 1, -1)).toBe(1)
    }
  })

  /**
   * And the rule is confined to the boundary: away from it, a second slot
   * still moves to the neighbouring group's first slot in both directions.
   * Without this the fix could be spelled "a second slot never moves".
   */
  it('still moves off a second slot in the middle of the volume', () => {
    // 0, 1, 2R, 2L, 3, 4, 5 — slot 3 is the spread's second half.
    const g = groups(slotsWith(6, [2]), 'single', rtl)
    expect(stepToGroup(g, 3, 1)).toBe(4)
    expect(stepToGroup(g, 3, -1)).toBe(1)
  })

  /** A slot no group claims cannot move — better than guessing at one. */
  it('stays put when the slot belongs to no group', () => {
    const slots = slotsWith(3, [])
    const g = groups(slots, 'single', rtl)
    expect(stepToGroup(g, 99, 1)).toBe(99)
  })
})

describe('groups, with slots set aside', () => {
  it('emits no group for a skipped slot', () => {
    const g = groups(slotsWith(6, []), 'single', 'rtl', [], new Set([3]))
    expect(g.map((x) => x.slots)).toEqual([[0], [1], [2], [4], [5]])
  })

  it('drops a joined spread whole when both halves are skipped', () => {
    // Slots 2 and 3 are page 2's two halves — one credits card, one tile.
    const g = groups(slotsWith(5, [2]), 'single', 'rtl', [], new Set([2, 3]))
    expect(g.map((x) => x.slots)).toEqual([[0], [1], [4], [5]])
  })

  it('keeps a joined spread whose halves disagree', () => {
    // `creditSlotsIn` filters by page and both halves share one, so this
    // cannot arise from the credits module. `groups` is handed a slot set and
    // cannot check where it came from, so it keeps the image rather than
    // drawing half of one.
    const g = groups(slotsWith(5, [2]), 'single', 'rtl', [], new Set([2]))
    expect(g.map((x) => x.slots)).toEqual([[0], [1], [2, 3], [4], [5]])
  })

  it('pairs around a skipped slot rather than stranding its neighbour', () => {
    // **The reason this lives in `groups` rather than in a filter over its
    // result.** Unskipped, `two` mode pairs (0,1) (2,3) (4,5). With slot 3
    // set aside, a filter could only drop a pair whose *both* slots were
    // skipped — none here — so `skip` would do nothing at all. Pairing past
    // it instead gives slot 2 a real partner and leaves no tile holding a
    // page the reader asked to set aside.
    const g = groups(slotsWith(6, []), 'two', 'rtl', [], new Set([3]))
    expect(g.map((x) => x.slots)).toEqual([[0, 1], [2, 4], [5]])
  })

  it('measured: on the primary corpus this is the difference between working and not', () => {
    // `en-fan`'s four credit pages each sit at a chapter's end and so pair
    // with an ordinary page. Driven in a browser against a written sidecar,
    // filtering finished groups gave 122 tiles with `skip` on and 122 with it
    // off. This is that shape in miniature: every skipped slot is the second
    // of its would-be pair.
    const none = groups(slotsWith(8, []), 'two', 'rtl')
    const set = groups(slotsWith(8, []), 'two', 'rtl', [], new Set([1, 5]))
    expect(none.length).toBe(4)
    expect(set.length).toBeLessThan(none.length)
    expect(set.flatMap((x) => x.slots)).not.toContain(1)
    expect(set.flatMap((x) => x.slots)).not.toContain(5)
  })

  it('honours a break at the partner, not at the slot after this one', () => {
    // The break is a slot number, so a skipped slot in between must not move
    // which slot it refers to. Slot 1 skipped, break on 2: slot 0 finds 2 as
    // its partner and the break says 2 opens a fresh group instead.
    const g = groups(slotsWith(5, []), 'two', 'rtl', [2], new Set([1]))
    expect(g.map((x) => x.slots)).toEqual([[0], [2, 3], [4]])
  })

  it('leaves every caller that skips nothing exactly as it was', () => {
    // `skip` switched off must cost nothing and change nothing —
    // `module.ts`'s contract that a module which is off contributes nothing,
    // and the property M5b's manual pass checks for.
    const slots = slotsWith(7, [3])
    for (const mode of ['single', 'two', 'scroll'] as const) {
      expect(groups(slots, mode, 'rtl', [], new Set())).toEqual(
        groups(slots, mode, 'rtl'),
      )
    }
  })
})

describe('groups, the cases the whole-branch review found', () => {
  it('keeps a joined spread whole in two mode when only one half is skipped', () => {
    // The partner search can land on a spread's *second* half when its first
    // is skipped. Pairing with it put half a wide image in an ordinary cell
    // and dropped the other half — while `single` mode, given the same input,
    // kept the spread. The two modes must agree.
    const slots = slotsWith(3, [1])
    const two = groups(slots, 'two', 'rtl', [], new Set([1]))
    const single = groups(slots, 'single', 'rtl', [], new Set([1]))
    expect(two.map((x) => x.slots)).toEqual([[0], [1, 2], [3]])
    expect(two.find((x) => x.spread)?.slots).toEqual([1, 2])
    expect(single.find((x) => x.spread)?.slots).toEqual([1, 2])
  })

  it('honours a break that falls inside the skipped run', () => {
    // A break is a slot a reader marked by hand. Testing only `partner`
    // swallowed one sitting between `index` and `partner`, so switching
    // `skip` on silently changed the parity of the rest of the volume for a
    // reader who had fixed it themselves.
    const slots = slotsWith(5, [])
    expect(
      groups(slots, 'two', 'rtl', [1], new Set([1])).map((x) => x.slots),
    ).toEqual([[0], [2, 3], [4]])
    // Distinguishable from the same call without the break, which is the
    // whole point — these were byte-identical before.
    expect(
      groups(slots, 'two', 'rtl', [], new Set([1])).map((x) => x.slots),
    ).toEqual([
      [0, 2],
      [3, 4],
    ])
  })
})

describe('stepToGroup, across a group that straddles a skipped slot', () => {
  // `[2, 4]` is one tile with slot 3 set aside between its halves — a shape
  // that could not exist before `groups` took a `skipped` set, and that breaks
  // any test written against a group's first or last slot alone.
  //
  // **These pin a backstop, not a reachable state.** `skippedApartFrom` exempts
  // the page the reader is standing on, so the app never asks `stepToGroup`
  // about a slot in no group. That invariant lives in `modules/credits.ts`,
  // one module away, which is exactly why the fallback is kept and why it is
  // tested here rather than left to be exercised by accident.

  it('steps onto the straddling tile rather than over it', () => {
    const g = groups(slotsWith(6, []), 'two', 'rtl', [], new Set([3]))
    expect(g.map((x) => x.slots)).toEqual([[0, 1], [2, 4], [5]])
    // Both directions land on it: the skipped slot sits inside its span.
    expect(stepToGroup(g, 3, 1)).toBe(2)
    expect(stepToGroup(g, 3, -1)).toBe(2)
  })

  it('answers both directions with the tile the reader is inside', () => {
    // The two directions failed differently here, and the comment this
    // replaces claimed both were stuck. Only `←` was: it returned 1 and did
    // nothing. `→` returned 4 — it moved, but past slot 2, which is drawn as
    // half of the tile `[0, 2]`. Both now answer 0, that tile's first slot,
    // so nothing drawn is stepped over in either direction. The forward answer
    // is therefore a reversal (4 → 0), not only an unsticking.
    const g = groups(slotsWith(6, []), 'two', 'rtl', [], new Set([1, 3]))
    expect(g.map((x) => x.slots)).toEqual([
      [0, 2],
      [4, 5],
    ])
    expect(stepToGroup(g, 1, -1)).toBe(0)
    expect(stepToGroup(g, 1, 1)).toBe(0)
  })
})

describe('stepToGroup, standing on a slot no group holds', () => {
  // Reachable three ways once `skip` filters groups: taking a credit page
  // from the panel, clicking its filmstrip tile before switching `skip` on,
  // and switching `skip` on while already standing there. Before the credits
  // panel this could not happen, and the function simply refused to move.

  it('steps forward to the nearest surviving group', () => {
    const g = groups(slotsWith(6, []), 'single', 'rtl', [], new Set([3]))
    expect(stepToGroup(g, 3, 1)).toBe(4)
  })

  it('steps backward to the nearest surviving group', () => {
    const g = groups(slotsWith(6, []), 'single', 'rtl', [], new Set([3]))
    expect(stepToGroup(g, 3, -1)).toBe(2)
  })

  it('stays put when nothing survives ahead', () => {
    // The boundary answer the reader already gives at the first and last
    // slot: an arrow that has nowhere to go does nothing, rather than
    // jumping to the far end of the volume.
    const g = groups(slotsWith(6, []), 'single', 'rtl', [], new Set([3, 4, 5]))
    expect(stepToGroup(g, 3, 1)).toBe(3)
  })

  it('crosses a run of skipped slots in one press', () => {
    const g = groups(slotsWith(6, []), 'single', 'rtl', [], new Set([1, 2, 3]))
    expect(stepToGroup(g, 0, 1)).toBe(4)
  })
})

describe('skipping is presentational', () => {
  // Moved here from `modules/credits.test.ts` when `nextReadable` was retired:
  // the claim is now a property of the grouping, not of a slot walker.
  // The whole content of the numbering decision. If someone later renumbers
  // for readability, this goes red — which is the point.
  it('changes which slot an arrow lands on, never what a slot is called', () => {
    const slots = slotsWith(6, [])

    const walk = (from: Group[]) => {
      const visited: number[] = []
      let at = 0
      for (let i = 0; i < 3; i++) {
        at = stepToGroup(from, at, 1)
        visited.push(at)
      }
      return visited
    }

    expect(walk(groups(slots, 'single', 'rtl', [], new Set([2, 3])))).toEqual([
      1, 4, 5,
    ])
    expect(walk(groups(slots, 'single', 'rtl'))).toEqual([1, 2, 3])

    // Same slot table in both worlds — only which entries are reachable
    // differs. `slots.length` is what the position readout divides by.
    expect(slots.length).toBe(6)
  })
})

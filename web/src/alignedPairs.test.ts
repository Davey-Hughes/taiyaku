import { describe, expect, it } from 'vitest'
import {
  alignedBreaks,
  alignedBreaksCached,
  emptyBreaksCache,
  forcedBreaks,
  pairingBreaks,
} from './alignedPairs'
import { fakeResolver, whole } from './fixtures'
import { groups } from './pairing'
import type { Position } from './resolver'

/** `a|b|i` -> the slot of `b` that slot `i` of `a` lands on. */
type Pairs = Record<string, number>

/** Both directions of one correspondence, since a resolver is asked either way. */
function both(a: string, b: string, map: Record<number, number>): Pairs {
  const out: Pairs = {}
  for (const [from, to] of Object.entries(map)) {
    out[`${a}|${b}|${from}`] = to
    out[`${b}|${a}|${to}`] = Number(from)
  }
  return out
}

/** What `two` mode would draw for `edition`, given a set of breaks. */
function drawn(slots: Position[], breaks: number[]): number[][] {
  return groups(slots, 'two', 'rtl', breaks).map((g) => g.slots)
}

describe('alignedBreaks', () => {
  it('derives nothing for the hub itself, whose parity is the reference', () => {
    const resolver = fakeResolver({ hub: whole(6), b: whole(6) }, {})
    expect(alignedBreaks(resolver, 'hub', 'hub', 'rtl')).toEqual([])
  })

  // Asserted on what gets drawn, not on the list: the derivation names where
  // every hub group opens, so an edition already in phase is handed positions
  // its pairing restarts at anyway. Those are inert — a break tells the group
  // before it to stop short, and a position that already starts a group has
  // nothing to stop short of — and the property worth pinning is that the
  // grouping is unchanged, not that the list is empty.
  it('leaves the grouping alone when the two editions already pair in phase', () => {
    const bSlots = whole(6)
    const resolver = fakeResolver(
      { hub: whole(6), b: bSlots },
      both('hub', 'b', { 0: 0, 1: 1, 2: 2, 3: 3, 4: 4, 5: 5 }),
    )
    const breaks = alignedBreaks(resolver, 'hub', 'b', 'rtl')
    expect(drawn(bSlots, breaks)).toEqual(drawn(bSlots, []))
  })

  // The reported defect, at its smallest: `b` carries one page the hub does
  // not, so from there on the hub's pairs and `b`'s are one slot out of phase.
  it('restarts the pairing where an extra page in the other edition put it out of phase', () => {
    const hubSlots = whole(6)
    const bSlots = whole(7)
    const resolver = fakeResolver(
      { hub: hubSlots, b: bSlots },
      // `b` slot 2 is an insert with no hub counterpart.
      both('hub', 'b', { 0: 0, 1: 1, 2: 3, 3: 4, 4: 5, 5: 6 }),
    )

    expect(drawn(bSlots, [])).toEqual([[0, 1], [2, 3], [4, 5], [6]])

    const breaks = alignedBreaks(resolver, 'hub', 'b', 'rtl')
    expect(breaks).toEqual([3, 5])
    // The hub draws [0,1] [2,3] [4,5]; `b` now draws their counterparts —
    // [0,1] [3,4] [5,6] — with the unpaired insert alone between them.
    expect(drawn(bSlots, breaks)).toEqual([[0, 1], [2], [3, 4], [5, 6]])
  })

  it('puts each pair of the hub and its counterpart on screen together, which is what a swap answers with', () => {
    const hubSlots = whole(6)
    const bSlots = whole(7)
    const map = { 0: 0, 1: 1, 2: 3, 3: 4, 4: 5, 5: 6 }
    const resolver = fakeResolver(
      { hub: hubSlots, b: bSlots },
      both('hub', 'b', map),
    )
    const breaks = alignedBreaks(resolver, 'hub', 'b', 'rtl')
    const there = drawn(bSlots, breaks)

    for (const group of drawn(hubSlots, [])) {
      const want = group.map((s) => map[s as keyof typeof map])
      const got = there.find((g) => g.includes(want[0] as number))
      expect(got).toEqual(want)
    }
  })

  it('keeps a group whose first slot is unpaired in phase, from the slot its second lands on', () => {
    // Hub slot 2 has no counterpart; slot 3 does. The pair (2,3) still has to
    // open a group in `b`, or everything after it drifts — so the break comes
    // from slot 3's landing, which is the group's only resolvable member.
    const hubSlots = whole(6)
    const bSlots = whole(5)
    const resolver = fakeResolver(
      { hub: hubSlots, b: bSlots },
      both('hub', 'b', { 0: 0, 1: 1, 3: 2, 4: 3, 5: 4 }),
    )
    const breaks = alignedBreaks(resolver, 'hub', 'b', 'rtl')
    expect(breaks).toEqual([2, 3])
    // The hub draws [0,1] [2,3] [4,5]; `b` holds counterparts for one slot of
    // the middle pair and both of the others, and draws exactly those.
    expect(drawn(bSlots, breaks)).toEqual([[0, 1], [2], [3, 4]])
  })

  it('leaves a joined spread whole, since a spread cannot be halved by a break', () => {
    const hubSlots = whole(4)
    // `b` page 1 is a joined spread: two slots, one image.
    const bSlots: Position[] = [
      { page: 0 },
      { page: 1, half: 'R' },
      { page: 1, half: 'L' },
      { page: 2 },
    ]
    const resolver = fakeResolver(
      { hub: hubSlots, b: bSlots },
      both('hub', 'b', { 0: 0, 1: 1, 2: 2, 3: 3 }),
    )
    const breaks = alignedBreaks(resolver, 'hub', 'b', 'rtl')
    // Whatever the derivation says, the spread stays one group.
    expect(drawn(bSlots, breaks)).toContainEqual([1, 2])
  })

  it('derives nothing rather than a partial parity when the hub cannot be walked', () => {
    const resolver = fakeResolver({ b: whole(4) }, {})
    expect(alignedBreaks(resolver, 'missing', 'b', 'rtl')).toEqual([])
  })

  it('never names slot 0, where pairing already restarts', () => {
    const resolver = fakeResolver(
      { hub: whole(4), b: whole(4) },
      both('hub', 'b', { 0: 0, 1: 1, 2: 2, 3: 3 }),
    )
    expect(alignedBreaks(resolver, 'hub', 'b', 'rtl')).not.toContain(0)
  })

  it('reports one break once when two hub groups land on the same slot', () => {
    // A shorter edition: hub slots 2 and 3 both resolve into `b` slot 2.
    const resolver = fakeResolver(
      { hub: whole(5), b: whole(3) },
      {
        'hub|b|0': 0,
        'hub|b|1': 1,
        'hub|b|2': 2,
        'hub|b|3': 2,
        'b|hub|0': 0,
        'b|hub|1': 1,
        'b|hub|2': 2,
      },
    )
    const breaks = alignedBreaks(resolver, 'hub', 'b', 'rtl')
    expect(breaks).toEqual([...new Set(breaks)].sort((a, b) => a - b))
  })
})

describe('forcedBreaks', () => {
  it('finds nothing in an edition with no joined spread', () => {
    expect(forcedBreaks(whole(6))).toEqual([])
  })

  it('names both ends of a joined spread — where the pair before it stops and the one after starts', () => {
    const slots: Position[] = [
      { page: 0 },
      { page: 1, half: 'R' },
      { page: 1, half: 'L' },
      { page: 2 },
      { page: 3 },
    ]
    expect(forcedBreaks(slots)).toEqual([1, 3])
  })

  it('names a spread that ends the volume without inventing a slot past it', () => {
    const slots: Position[] = [
      { page: 0 },
      { page: 1, half: 'R' },
      { page: 1, half: 'L' },
    ]
    expect(forcedBreaks(slots)).toEqual([1])
  })

  it('names each of two spreads that sit back to back', () => {
    // The `en-mono` shape that broke the first version of this module: pages 1
    // and 2 both joined, which the hub splits into four ordinary pages.
    const slots: Position[] = [
      { page: 0 },
      { page: 1, half: 'R' },
      { page: 1, half: 'L' },
      { page: 2, half: 'R' },
      { page: 2, half: 'L' },
      { page: 3 },
    ]
    expect(forcedBreaks(slots)).toEqual([1, 3, 5])
  })
})

describe('alignedBreaks, across a spread the hub splits', () => {
  // `b` joins pages 1 and 2; the hub carries all four as ordinary pages. The
  // hub's own parity would pair (0,1) (2,3) (4,5), which cuts both of `b`'s
  // spreads in half — so the boundaries `b` forces have to reach the hub.
  const hubSlots = whole(6)
  const bSlots: Position[] = [
    { page: 0 },
    { page: 1, half: 'R' },
    { page: 1, half: 'L' },
    { page: 2, half: 'R' },
    { page: 2, half: 'L' },
    { page: 3 },
  ]
  const map = { 0: 0, 1: 1, 2: 2, 3: 3, 4: 4, 5: 5 }
  const resolver = fakeResolver(
    { hub: hubSlots, b: bSlots },
    both('hub', 'b', map),
  )

  it('moves the hub’s own pairing onto the boundaries the other edition forces', () => {
    const breaks = alignedBreaks(resolver, 'hub', 'hub', 'rtl')
    expect(breaks).toEqual([1, 3, 5])
    expect(drawn(hubSlots, breaks)).toEqual([[0], [1, 2], [3, 4], [5]])
  })

  it('draws the counterpart of every spread as one pair on the hub side', () => {
    const hubDrawn = drawn(
      hubSlots,
      alignedBreaks(resolver, 'hub', 'hub', 'rtl'),
    )
    const bDrawn = drawn(bSlots, alignedBreaks(resolver, 'hub', 'b', 'rtl'))
    for (const group of bDrawn) {
      const want = group.map((s) => map[s as keyof typeof map])
      const got = hubDrawn.find((g) => g.includes(want[0] as number))
      expect(got).toEqual(want)
    }
  })
})

describe('alignedBreaksCached', () => {
  /** Counts `slotAt` calls, which is what the walk costs and so what the cache is for. */
  function counting(base: ReturnType<typeof fakeResolver>) {
    let calls = 0
    return {
      resolver: {
        ...base,
        slotAt: (edition: string, index: number) => {
          calls += 1
          return base.slotAt(edition, index)
        },
      },
      calls: () => calls,
    }
  }

  const build = () =>
    counting(
      fakeResolver(
        { hub: whole(6), b: whole(7), c: whole(6) },
        {
          ...both('hub', 'b', { 0: 0, 1: 1, 2: 3, 3: 4, 4: 5, 5: 6 }),
          ...both('hub', 'c', { 0: 0, 1: 1, 2: 2, 3: 3, 4: 4, 5: 5 }),
        },
      ),
    )

  it('walks the hub once for repeated calls with the same inputs', () => {
    const { resolver, calls } = build()
    const cache = emptyBreaksCache()
    const first = alignedBreaksCached(cache, resolver, 'hub', 'b', 'rtl')
    const after = calls()
    const second = alignedBreaksCached(cache, resolver, 'hub', 'b', 'rtl')
    expect(calls()).toBe(after)
    expect(second).toBe(first)
  })

  it('walks again when the edition changes, rather than answering with the last one’s breaks', () => {
    const { resolver, calls } = build()
    const cache = emptyBreaksCache()
    const forB = alignedBreaksCached(cache, resolver, 'hub', 'b', 'rtl')
    const after = calls()
    const forC = alignedBreaksCached(cache, resolver, 'hub', 'c', 'rtl')
    expect(calls()).toBeGreaterThan(after)
    expect(forC).not.toEqual(forB)
  })

  // A correction or a re-read replaces the resolver while every name stays the
  // same. Keying on names alone would keep serving the old alignment's pairing.
  it('walks again when the resolver is replaced under unchanged names', () => {
    const cache = emptyBreaksCache()
    const before = build()
    alignedBreaksCached(cache, before.resolver, 'hub', 'b', 'rtl')

    const corrected = counting(
      fakeResolver(
        { hub: whole(6), b: whole(7) },
        both('hub', 'b', { 0: 0, 1: 1, 2: 2, 3: 3, 4: 4, 5: 5 }),
      ),
    )
    const after = alignedBreaksCached(
      cache,
      corrected.resolver,
      'hub',
      'b',
      'rtl',
    )
    expect(corrected.calls()).toBeGreaterThan(0)
    expect(after).toEqual(alignedBreaks(corrected.resolver, 'hub', 'b', 'rtl'))
  })
})

describe('pairingBreaks', () => {
  it('keeps a reader’s own break the alignment did not derive', () => {
    expect(pairingBreaks([7], [3, 5])).toEqual([3, 5, 7])
  })

  it('returns the derived list untouched when the reader has placed none', () => {
    const derived = [3, 5]
    expect(pairingBreaks([], derived)).toBe(derived)
  })

  it('returns the reader’s list untouched when nothing was derived', () => {
    const own = [3]
    expect(pairingBreaks(own, [])).toBe(own)
  })

  it('does not repeat a position both lists name', () => {
    expect(pairingBreaks([3, 9], [3, 5])).toEqual([3, 5, 9])
  })
})

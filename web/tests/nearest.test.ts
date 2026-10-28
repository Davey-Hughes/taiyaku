import { describe, expect, it } from 'vitest'
import { fakeResolver, whole, bSlots } from './support/fixtures'
import { nearestPaired } from '../src/nearest'

const r = fakeResolver(
  { a: whole(8), b: bSlots },
  { 'a|b|1': 0, 'a|b|2': 1, 'a|b|3': 2 },
)

describe('nearestPaired', () => {
  it('offers the paired slots either side of a gap, already resolved', () => {
    // `a` slot 4 is unpaired; 3 is the nearest paired before it and there is
    // nothing paired after it. `b`'s slot 2 is `{ page: 2, half: 'R' }` — the
    // resolved page and the slot holding it both travel with the offer, so a
    // caller has neither to resolve again.
    expect(
      nearestPaired({ authority: 'a', slot: 4, viewing: 'b' }, r, 8),
    ).toEqual({ before: { slot: 3, page: 2, at: 2 } })
  })

  it('offers only what exists at the start of a volume', () => {
    expect(
      nearestPaired({ authority: 'a', slot: 0, viewing: 'b' }, r, 8),
    ).toEqual({ after: { slot: 1, page: 0, at: 0 } })
  })

  it('offers nothing where the whole edition is unpaired', () => {
    const none = fakeResolver({ a: whole(3), b: whole(3) }, {})
    expect(
      nearestPaired({ authority: 'a', slot: 1, viewing: 'b' }, none, 3),
    ).toEqual({})
  })

  it('resolves each offer once, not once for the scan and again for the caller', () => {
    // The double-resolve this fix removes lived in `App.tsx` (untestable
    // here — see its own no-resolver-in-tests comment), not in
    // `nearestPaired` itself. What this pins is the half `nearestPaired`
    // owns: the scan calls `resolver.resolve` once per candidate slot it
    // actually looks at, and the found offer already carries a resolved
    // `page` — so a caller has nothing left to resolve.
    let calls = 0
    const counting: typeof r = {
      ...r,
      resolve: (from, to, at) => {
        calls++
        return r.resolve(from, to, at)
      },
    }
    const found = nearestPaired(
      { authority: 'a', slot: 4, viewing: 'b' },
      counting,
      8,
    )
    // Slot 4 is the gap; the scan checks slot 3 (before, found immediately)
    // and slots 5, 6, 7 (after, all past the end of `a`'s 8 slots — none
    // resolve, so the scan exhausts `within` without an `after`).
    expect(calls).toBe(4)
    expect(found).toEqual({ before: { slot: 3, page: 2, at: 2 } })
  })

  // O40. The scan passes over a slot the `credits` module has set aside, the
  // same way `pairing.ts`'s partner search already does, rather than offering
  // a page the reader asked to keep out of the reading flow. Both halves of
  // `b`'s joined spread are in the set because `creditSlotsIn` matches by
  // page, which is what a real skipped set looks like here.
  it('passes over a skipped slot and offers the next one that resolves', () => {
    expect(
      nearestPaired(
        { authority: 'a', slot: 4, viewing: 'b' },
        r,
        8,
        new Set([2, 3]),
      ),
    ).toEqual({ before: { slot: 2, page: 1, at: 1 } })
  })

  it('offers nothing on that side where every candidate is skipped', () => {
    expect(
      nearestPaired(
        { authority: 'a', slot: 4, viewing: 'b' },
        r,
        8,
        new Set([0, 1, 2, 3]),
      ),
    ).toEqual({})
  })

  it('reports `at` in the viewed edition, not in the authority', () => {
    // `a` slot 3 pairs to `b` slot 2. The two numbers differ, so a
    // confusion between the spaces would be visible here and nowhere in the
    // tests above, where `slot` and `at` happen to be close together.
    // `slot` is what `takeOffered` navigates by (authority-space); `at` is
    // what the filmstrip centres on (a tile of `b`).
    const found = nearestPaired({ authority: 'a', slot: 4, viewing: 'b' }, r, 8)
    expect(found.before?.slot).toBe(3)
    expect(found.before?.at).toBe(2)
    expect(r.slotAt('b', found.before!.at)).toEqual({ page: 2, half: 'R' })
  })
})

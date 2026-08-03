import { describe, expect, it } from 'vitest'
import { fakeResolver, whole, bSlots } from './fixtures'
import { display, navigate, restore, swap, type ReadingState } from './reading'

// `a` slots 1,2,3 pair with `b` slots 0,1,2. `a` slot 0 and 4..7 pair with
// nothing, so the gap case is in range.
const r = fakeResolver(
  { a: whole(8), b: bSlots },
  { 'a|b|1': 0, 'a|b|2': 1, 'a|b|3': 2, 'b|a|0': 1, 'b|a|1': 2, 'b|a|2': 3 },
)

const at = (slot: number): ReadingState => ({ authority: 'a', slot, viewing: 'a' })

describe('display', () => {
  it('shows the authority slot itself when viewing the authority', () => {
    expect(display(at(2), r)).toEqual({ kind: 'at', edition: 'a', slot: 2, via: 'identity' })
  })

  it('resolves into the viewed edition when they differ', () => {
    expect(display(swap(at(2), 'b'), r)).toEqual({
      kind: 'at', edition: 'b', slot: 1, via: 'direct',
    })
  })

  it('reports a gap rather than moving to the nearest slot', () => {
    expect(display(swap(at(0), 'b'), r)).toEqual({
      kind: 'gap', edition: 'b', from: { edition: 'a', slot: 0 },
    })
  })

  it('lands a slot on one half of a joined spread, not on the whole page', () => {
    expect(display(swap(at(3), 'b'), r)).toEqual({
      kind: 'at', edition: 'b', slot: 2, via: 'direct',
    })
  })
})

describe('swap', () => {
  it('never moves authority, so a swap cannot drift', () => {
    const start = at(2)
    const there = swap(start, 'b')
    expect(there.authority).toBe('a')
    expect(there.slot).toBe(2)
    expect(there.viewing).toBe('b')
  })

  it('returns exactly where it started, over every paired slot', () => {
    for (const slot of [1, 2, 3]) {
      expect(swap(swap(at(slot), 'b'), 'a')).toEqual(at(slot))
    }
  })

  it('returns exactly where it started even from a gap', () => {
    expect(swap(swap(at(0), 'b'), 'a')).toEqual(at(0))
  })
})

describe('navigate', () => {
  it('moves authority to the edition being viewed', () => {
    const there = navigate(swap(at(2), 'b'), 1)
    expect(there).toEqual({ authority: 'b', slot: 1, viewing: 'b' })
  })
})

describe('restore', () => {
  it('starts at slot 0 of the hub when nothing is stored', () => {
    expect(restore(undefined, 'a', r)).toEqual({ authority: 'a', slot: 0, viewing: 'a' })
  })

  it('starts at the stored position, viewing the edition it was stored against', () => {
    expect(restore({ edition: 'b', slot: 5 }, 'a', r)).toEqual({
      authority: 'b', slot: 5, viewing: 'b',
    })
  })

  it('trusts a stored slot on the hub itself rather than resetting to 0', () => {
    // The hub is only a fallback for "nothing recorded yet" — a stored
    // position, even on the hub, must not be forced back to slot 0.
    expect(restore({ edition: 'a', slot: 7 }, 'a', r)).toEqual({
      authority: 'a', slot: 7, viewing: 'a',
    })
  })

  it('falls back to the start of the edition when the stored slot is past its end', () => {
    // `a` has 8 slots (indices 0..7, per `whole(8)` above) — a sidecar
    // re-solved after this was written could easily have shrunk it. Landing
    // at slot 0 is the honest move: it stays on the edition the reader was
    // actually reading, rather than guessing a slot near wherever the new
    // boundary happens to fall — guessing "last slot" would just be a
    // different silent wrong page.
    expect(restore({ edition: 'a', slot: 8 }, 'a', r)).toEqual({
      authority: 'a', slot: 0, viewing: 'a',
    })
  })

  it('falls back to the hub when the stored edition no longer exists', () => {
    // Distinct from an out-of-range slot: there is no slot space at all to
    // place the reader in an edition the sidecar no longer defines (renamed
    // or dropped), so this is exactly the "nothing recorded" case.
    expect(restore({ edition: 'gone', slot: 3 }, 'a', r)).toEqual({
      authority: 'a', slot: 0, viewing: 'a',
    })
  })
})

// Nearly free and nearly tautological as written: `swap` only sets `viewing`,
// so this cannot fail while `swap` stays one line. It earns its place anyway,
// as the regression guard for the day someone "optimises" `swap` to resolve
// eagerly and write position — exactly the drift `notes.md`'s swap gotcha
// warns about.
describe('the round-trip seam', () => {
  it('A to B to A returns to the starting slot, for every slot of every ordered pair', () => {
    for (const from of r.editions()) {
      for (const to of r.editions()) {
        const count = r.slotCount(from)!
        for (let slot = 0; slot < count; slot++) {
          const start: ReadingState = { authority: from, slot, viewing: from }
          const back = swap(swap(start, to), from)
          expect(back, `${from} slot ${slot} via ${to}`).toEqual(start)
        }
      }
    }
  })
})

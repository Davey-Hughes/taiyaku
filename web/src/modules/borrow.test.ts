import { describe, expect, it } from 'vitest'
import { borrowModule, borrowNote, lenderFor, withBorrow } from './borrow'
import { fakeResolver, whole } from '../fixtures'
import { display, type ReadingState } from '../reading'

// Four editions of four pages each, `a` the hub and therefore first in the
// order `App.tsx` passes. Slot 2 is the interesting one: `b` has no
// counterpart for it, so a reader viewing `b` from authority `a` is in a gap
// there. `c` has no counterpart either; `d` does. That shape is what lets one
// fixture exercise the order and the skip rather than two.
const slots = { a: whole(4), b: whole(4), c: whole(4), d: whole(4) }
const order = ['a', 'b', 'c', 'd']

// Every slot of `a` pairs into `b` except slot 2. `d` pairs slot 2; `c` never
// does.
const pairs = { 'a|b|0': 0, 'a|b|1': 1, 'a|b|3': 3, 'a|d|2': 2 }

const resolver = fakeResolver(slots, pairs)
const inGap: ReadingState = { authority: 'a', slot: 2, viewing: 'b' }
const on = { enabled: true }

describe('lenderFor', () => {
  it('prefers an edition that is neither viewed nor the authority', () => {
    expect(lenderFor(inGap, resolver, order)).toEqual({ edition: 'd', slot: 2 })
  })

  it('skips a candidate that does not resolve, in the order given', () => {
    // `c` comes before `d` and has no pairing for slot 2, so a rule that
    // returned the first *candidate* rather than the first that *resolves*
    // would answer `c` here.
    expect(lenderFor(inGap, resolver, order)?.edition).not.toBe('c')
  })

  it('lends nothing when no third edition resolves, rather than the authority', () => {
    // The authority's page is already one `Space` away — `applyIntent`'s
    // toggle goes to `state.authority` whenever `viewing !== authority`, which
    // is true of every gap. Lending it drew a page the reader had just left,
    // without arrows, annotations, zoom or prefetch, in place of the `Gap`
    // view that names the missing edition and offers both escapes.
    // 228 of 276 corpus borrows were this. See the 2026-08-13 design, §2.
    const twoEditions = fakeResolver({ a: whole(4), b: whole(4) }, pairs)
    expect(lenderFor(inGap, twoEditions, ['a', 'b'])).toBeUndefined()
  })

  it('never lends the edition being viewed', () => {
    // `b` resolves to itself trivially, so a rule that forgot to exclude the
    // viewed edition would answer `b` — and draw the page that is missing.
    const fromD: ReadingState = { authority: 'a', slot: 2, viewing: 'd' }
    expect(lenderFor(fromD, resolver, order)?.edition).not.toBe('d')
  })

  it('has nothing to lend when the authority slot does not exist', () => {
    const offEnd: ReadingState = { authority: 'a', slot: 99, viewing: 'b' }
    expect(lenderFor(offEnd, resolver, order)).toBeUndefined()
  })
})

describe('withBorrow', () => {
  const gap = display(inGap, resolver)

  it('maps a gap to a borrowed page, keeping `edition` the viewed one', () => {
    expect(withBorrow(gap, inGap, resolver, order, on)).toEqual({
      kind: 'borrowed',
      edition: 'b',
      from: { edition: 'a', slot: 2 },
      lender: { edition: 'd', slot: 2 },
    })
  })

  it('leaves a gap alone when the module is switched off', () => {
    expect(withBorrow(gap, inGap, resolver, order, { enabled: false })).toBe(
      gap,
    )
  })

  it('leaves a gap alone when nothing can lend', () => {
    const offEnd: ReadingState = { authority: 'a', slot: 99, viewing: 'b' }
    const shown = display(offEnd, resolver)
    expect(withBorrow(shown, offEnd, resolver, order, on)).toBe(shown)
  })

  it('passes the gap straight through when nothing can lend, by identity', () => {
    const twoEditions = fakeResolver({ a: whole(4), b: whole(4) }, pairs)
    const shown = display(inGap, twoEditions)
    expect(withBorrow(shown, inGap, twoEditions, ['a', 'b'], on)).toBe(shown)
  })

  it('leaves a drawn page alone', () => {
    const at: ReadingState = { authority: 'a', slot: 0, viewing: 'b' }
    const shown = display(at, resolver)
    expect(shown.kind).toBe('at')
    expect(withBorrow(shown, at, resolver, order, on)).toBe(shown)
  })
})

describe('borrowModule', () => {
  it('starts enabled and claims no keys', () => {
    expect(borrowModule.defaults).toEqual({ enabled: true })
    expect(borrowModule.keys).toBeUndefined()
  })
})

describe('borrowNote', () => {
  it('names a two-edition work’s own situation, true of most of the corpus', () => {
    expect(borrowNote(2)).toBe(
      'This work has two editions, so there is no third edition to borrow a page from.',
    )
  })

  it('says nothing once a third edition exists for lenderFor to try', () => {
    expect(borrowNote(3)).toBeUndefined()
    expect(borrowNote(4)).toBeUndefined()
  })

  it('still answers, worded correctly, for edition counts the corpus does not have', () => {
    expect(borrowNote(1)).toBe(
      'This work has one edition, so there is no third edition to borrow a page from.',
    )
    expect(borrowNote(0)).toBe(
      'This work has no editions, so there is no third edition to borrow a page from.',
    )
  })
})

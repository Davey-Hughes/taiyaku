import { describe, expect, it } from 'vitest'
import { pairingBreaks } from '../../src/alignedPairs'
import {
  breaksFor,
  realign,
  regroupModule,
  toggleBreak,
  withBreaks,
  type RegroupPrefs,
} from '../../src/modules/regroup'
import { groups, type Mode } from '../../src/pairing'
import type { Position } from '../../src/resolver'

describe('toggleBreak', () => {
  it('inserts a break where there was none', () => {
    expect(toggleBreak([], 4)).toEqual([4])
  })

  it('removes one that is already there', () => {
    expect(toggleBreak([2, 4], 4)).toEqual([2])
  })

  it('keeps the list sorted, so two readers reach the same layout', () => {
    expect(toggleBreak([4], 2)).toEqual([2, 4])
  })

  it('refuses a break at slot 0, which would restart pairing where it already starts', () => {
    expect(toggleBreak([], 0)).toEqual([])
  })
})

describe('regroupModule', () => {
  it('defaults to no breaks per volume, then edition', () => {
    expect(regroupModule.defaults.breaks).toEqual({})
  })

  it('claims one key', () => {
    expect(regroupModule.keys?.map((k) => k.key)).toEqual(['b'])
  })
})

/** Twelve single pages, no spreads. */
const FLAT: Position[] = Array.from({ length: 12 }, (_, page) => ({ page }))

/**
 * Twelve slots whose sixth and seventh are one joined spread, so pairing in
 * `two` mode runs `(0,1), (2,3), 4, [5,6], (7,8), (9,10), 11`.
 */
const SPREAD_AT_5: Position[] = [
  ...FLAT.slice(0, 5),
  { page: 5, half: 'L' },
  { page: 5, half: 'R' },
  ...FLAT.slice(5, 10).map(({ page }) => ({ page: page + 1 })),
]

/** `App`'s two-page layout for a list of the reader's own breaks. */
const layoutOf =
  (slots: Position[], derived: number[] = [], skipped = new Set<number>()) =>
  (own: number[]) =>
    groups(slots, 'two', 'ltr', pairingBreaks(own, derived), skipped)

const press = (own: number[], slot: number, mode: Mode = 'two') =>
  realign(layoutOf(FLAT), own, slot, mode)

describe('realign', () => {
  it('on the first slot of a pair, breaks at its second, so the first stands alone', () => {
    expect(press([], 4)).toEqual([5])
  })

  it('from the page that now stands alone, a second press undoes the first', () => {
    expect(press([5], 4)).toEqual([])
  })

  it('on the second slot of a pair, breaks at that slot too', () => {
    expect(press([], 5)).toEqual([5])
  })

  it("on the volume's first pair, breaks at its second slot rather than refusing slot 0", () => {
    expect(press([], 0)).toEqual([1])
  })

  it('on the last page standing alone, changes nothing', () => {
    // `[5]`: `(0,1), (2,3), 4, (5,6), (7,8), (9,10), 11`.
    expect(press([5], 11)).toBeUndefined()
  })

  it('on a lone page whose next slot opens a spread, changes nothing', () => {
    expect(realign(layoutOf(SPREAD_AT_5), [], 4, 'two')).toBeUndefined()
  })

  it('on a joined spread, changes nothing', () => {
    expect(realign(layoutOf(SPREAD_AT_5), [], 5, 'two')).toBeUndefined()
    expect(realign(layoutOf(SPREAD_AT_5), [], 6, 'two')).toBeUndefined()
  })

  it('on a lone page whose next slot is a derived break, changes nothing', () => {
    // A derived break is never overridden: the reader's own break there is
    // absorbed by the one the alignment already holds.
    expect(realign(layoutOf(FLAT, [5]), [], 4, 'two')).toBeUndefined()
  })

  it('on a lone page whose next slot is set aside, still undoes the first press', () => {
    // Slot 5 skipped: `4` pairs with `6`, so the first press breaks at 6, and
    // the slot after the lone 4 that opens a group is 6, not 5.
    const skipped = new Set([5])
    expect(realign(layoutOf(FLAT, [], skipped), [], 4, 'two')).toEqual([6])
    expect(realign(layoutOf(FLAT, [], skipped), [6], 4, 'two')).toEqual([])
  })

  it('in single mode, breaks at the slot on screen', () => {
    expect(press([], 5, 'single')).toEqual([5])
  })

  it('in single mode, on a slot that already opens a pair, changes nothing', () => {
    expect(press([], 4, 'single')).toBeUndefined()
  })

  it('in single mode, at slot 0, changes nothing', () => {
    expect(press([], 0, 'single')).toBeUndefined()
  })

  it('in scroll mode, breaks at the slot on screen as single does', () => {
    expect(press([], 5, 'scroll')).toEqual([5])
  })
})

describe('breaksFor', () => {
  it("reads one volume's edition's list", () => {
    const prefs: RegroupPrefs = { breaks: { 'vol-01': { raw: [5] } } }
    expect(breaksFor(prefs, 'vol-01', 'raw')).toEqual([5])
  })

  it("does not read another volume's list under the same edition key", () => {
    const prefs: RegroupPrefs = { breaks: { 'vol-02': { raw: [5] } } }
    expect(breaksFor(prefs, 'vol-01', 'raw')).toEqual([])
  })

  it('reads a slice keyed by edition alone as no breaks', () => {
    const prefs = { breaks: { raw: [5] } } as unknown as RegroupPrefs
    expect(breaksFor(prefs, 'vol-01', 'raw')).toEqual([])
  })
})

describe('withBreaks', () => {
  it("sets one volume's edition's list, leaving its other editions and every other volume intact", () => {
    const prefs: RegroupPrefs = {
      breaks: { 'vol-01': { en: [2] }, 'vol-02': { raw: [3] } },
    }
    expect(withBreaks(prefs, 'vol-01', 'raw', [5])).toEqual({
      breaks: { 'vol-01': { en: [2], raw: [5] }, 'vol-02': { raw: [3] } },
    })
  })

  it('does not mutate the prefs object passed in', () => {
    const prefs: RegroupPrefs = { breaks: { 'vol-01': { en: [2] } } }
    withBreaks(prefs, 'vol-01', 'raw', [5])
    expect(prefs).toEqual({ breaks: { 'vol-01': { en: [2] } } })
  })
})

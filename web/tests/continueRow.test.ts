import { describe, expect, it } from 'vitest'
import { filledColumn, pxOf, spreadGap } from '../src/continueRow'

/**
 * The arithmetic the Continue row is laid by (spec O288 item 2): the
 * grid's columns counted as `repeat(auto-fill, …)` counts them, and the gap
 * or the column the grid ends up with in either layout. The row itself is
 * drawn only in a browser (`Library.continue.browser.test.tsx`).
 */

describe('pxOf', () => {
  it('reads a px length, with any whitespace around it', () => {
    expect([pxOf('16px'), pxOf(' 9.6px ')]).toEqual([16, 9.6])
  })

  it('reads nothing from a length in other units, an expression or nothing', () => {
    expect([pxOf('1rem'), pxOf('clamp(0.6rem, 1px, 1rem)'), pxOf('')]).toEqual([
      Number.NaN,
      Number.NaN,
      Number.NaN,
    ])
  })
})

describe('spreadGap', () => {
  it('spreads the room the columns leave into the gaps between them', () => {
    // Six 144 px columns across 1008 px; two across a phone's 358 px.
    expect([spreadGap(1008, 144, 16), spreadGap(358, 144, 16)]).toEqual([
      28.8, 70,
    ])
  })

  it('counts the columns down, where the count lies between two', () => {
    // 883 / 160 is 5.52: five columns, not six.
    expect(spreadGap(867, 144, 16)).toBe(36.75)
  })

  it('keeps the least gap with one column, however much room it leaves', () => {
    expect([spreadGap(358, 240, 16), spreadGap(144, 144, 16)]).toEqual([16, 16])
  })
})

describe('filledColumn', () => {
  it('shares the row among the columns that fit, less the gaps between them', () => {
    expect([filledColumn(358, 144, 16), filledColumn(867, 144, 16)]).toEqual([
      171, 160.6,
    ])
  })

  it('is the whole row with one column', () => {
    expect(filledColumn(358, 240, 16)).toBe(358)
  })

  it('counts at a fractional least gap', () => {
    // 104 px covers' least gap is 32/3 px at a 16 px root: three columns.
    expect(filledColumn(358, 104, 32 / 3)).toBeCloseTo(112.22, 2)
  })
})

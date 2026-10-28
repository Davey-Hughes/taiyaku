import { describe, expect, it } from 'vitest'
import {
  directionModule,
  effectiveDirection,
  flipped,
  setOverride,
  stepFor,
  type DirectionPrefs,
} from '../../src/modules/direction'

describe('effectiveDirection', () => {
  it('uses the sidecar when there is no override', () => {
    expect(effectiveDirection('rtl', undefined)).toBe('rtl')
    expect(effectiveDirection('ltr', undefined)).toBe('ltr')
  })

  it('lets an override win', () => {
    expect(effectiveDirection('rtl', 'ltr')).toBe('ltr')
  })

  it('treats an override equal to the sidecar as a no-op, not an error', () => {
    expect(effectiveDirection('rtl', 'rtl')).toBe('rtl')
  })
})

describe('directionModule', () => {
  it('defaults to no override, so the sidecar governs until asked otherwise', () => {
    expect(directionModule.defaults.override).toEqual({})
  })

  it('claims exactly one key', () => {
    expect(directionModule.keys?.map((k) => k.key)).toEqual(['d'])
  })
})

describe('setOverride', () => {
  it('sets the override for the given edition', () => {
    const prefs: DirectionPrefs = { override: {} }
    expect(setOverride(prefs, 'en-mono', 'ltr')).toEqual({
      override: { 'en-mono': 'ltr' },
    })
  })

  it("leaves another edition's override intact — App.tsx used to inline this merge, untested", () => {
    const prefs: DirectionPrefs = { override: { 'en-fan': 'rtl' } }
    expect(setOverride(prefs, 'en-mono', 'ltr')).toEqual({
      override: { 'en-fan': 'rtl', 'en-mono': 'ltr' },
    })
  })

  it('does not mutate the prefs object passed in', () => {
    const prefs: DirectionPrefs = { override: { 'en-fan': 'rtl' } }
    setOverride(prefs, 'en-mono', 'ltr')
    expect(prefs).toEqual({ override: { 'en-fan': 'rtl' } })
  })
})

describe('stepFor', () => {
  it('leaves a left-to-right work alone, so nothing changes without an override', () => {
    expect(stepFor(1, 'ltr')).toBe(1)
    expect(stepFor(-1, 'ltr')).toBe(-1)
  })

  // The whole point: in a right-to-left work the next page is drawn to the
  // left, so the left arrow is the one that advances.
  it('makes the left arrow advance in a right-to-left work', () => {
    expect(stepFor(-1, 'rtl')).toBe(1)
    expect(stepFor(1, 'rtl')).toBe(-1)
  })

  it('is its own inverse, so a reader who flips direction twice is where they started', () => {
    for (const reading of ['ltr', 'rtl'] as const)
      for (const by of [1, -1] as const)
        expect(stepFor(stepFor(by, reading), reading)).toBe(by)
  })
})

describe('flipped', () => {
  /**
   * **The first press of `d` is always visible.** Flipping from the *override*
   * — `undefined` for an edition nobody has touched — would have to guess a
   * starting point, and on a right-to-left work guessing `ltr` makes the first
   * press set `rtl` and appear to do nothing at all.
   */
  it('flips against the direction in effect, not against the override', () => {
    const fresh = directionModule.defaults
    expect(flipped('rtl', fresh, 'raw').override.raw).toBe('ltr')
    expect(flipped('ltr', fresh, 'raw').override.raw).toBe('rtl')
  })

  it('flips an edition that already has an override', () => {
    const set = { override: { raw: 'ltr' as const } }
    expect(flipped('rtl', set, 'raw').override.raw).toBe('rtl')
  })

  it('is its own inverse, so two presses are where the reader started', () => {
    const once = flipped('rtl', directionModule.defaults, 'raw')
    expect(flipped('rtl', once, 'raw').override.raw).toBe('rtl')
    expect(
      effectiveDirection('rtl', flipped('rtl', once, 'raw').override.raw),
    ).toBe('rtl')
  })

  /** Overrides are per edition, and `setOverride`'s whole reason for existing. */
  it('leaves every other edition’s override alone', () => {
    const set = { override: { raw: 'ltr' as const, jp: 'ltr' as const } }
    expect(flipped('rtl', set, 'raw').override).toEqual({
      raw: 'rtl',
      jp: 'ltr',
    })
  })
})

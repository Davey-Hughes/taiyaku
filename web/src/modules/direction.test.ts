import { describe, expect, it } from 'vitest'
import {
  directionModule,
  effectiveDirection,
  setOverride,
  type DirectionPrefs,
} from './direction'

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

import { describe, expect, it } from 'vitest'
import {
  regroupModule,
  toggleBreak,
  toggleEditionBreak,
  type RegroupPrefs,
} from './regroup'

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
  it('defaults to no breaks per edition', () => {
    expect(regroupModule.defaults.breaks).toEqual({})
  })

  it('claims one key', () => {
    expect(regroupModule.keys?.map((k) => k.key)).toEqual(['b'])
  })
})

describe('toggleEditionBreak', () => {
  it('sets the break list for the given edition', () => {
    const prefs: RegroupPrefs = { breaks: {} }
    expect(toggleEditionBreak(prefs, 'en-mono', 4)).toEqual({
      breaks: { 'en-mono': [4] },
    })
  })

  it("leaves another edition's breaks intact — App.tsx used to inline this merge, untested", () => {
    const prefs: RegroupPrefs = { breaks: { 'en-fan': [2] } }
    expect(toggleEditionBreak(prefs, 'en-mono', 4)).toEqual({
      breaks: { 'en-fan': [2], 'en-mono': [4] },
    })
  })

  it('toggles off a break already set for that edition, without touching another', () => {
    const prefs: RegroupPrefs = { breaks: { 'en-fan': [2], 'en-mono': [4] } }
    expect(toggleEditionBreak(prefs, 'en-mono', 4)).toEqual({
      breaks: { 'en-fan': [2], 'en-mono': [] },
    })
  })

  it('does not mutate the prefs object passed in', () => {
    const prefs: RegroupPrefs = { breaks: { 'en-fan': [2] } }
    toggleEditionBreak(prefs, 'en-mono', 4)
    expect(prefs).toEqual({ breaks: { 'en-fan': [2] } })
  })
})

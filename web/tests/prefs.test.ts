import { afterEach, describe, expect, it, vi } from 'vitest'
import { NAMESPACE, loadPrefs, resetPrefs, savePrefs } from '../src/prefs'
import { fakeStorage } from './support/storage'

/** A storage that always throws, simulating a full or unavailable store. */
function throwingStorage(): Storage {
  const fail = () => {
    throw new Error('quota exceeded')
  }
  return {
    get length() {
      return 0
    },
    clear: fail,
    getItem: () => null,
    key: () => null,
    removeItem: fail,
    setItem: fail,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('prefs', () => {
  it('returns the defaults when nothing is stored', () => {
    vi.stubGlobal('localStorage', fakeStorage())
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: true,
      ms: 120,
    })
  })

  it('round-trips a saved slice', () => {
    vi.stubGlobal('localStorage', fakeStorage())
    savePrefs('motion', { animate: false, ms: 0 })
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: false,
      ms: 0,
    })
  })

  it('fills in a key the stored slice predates, rather than dropping it', () => {
    const s = fakeStorage()
    s.setItem('taiyaku.prefs.motion', JSON.stringify({ animate: false }))
    vi.stubGlobal('localStorage', s)
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: false,
      ms: 120,
    })
  })

  it('falls back to the defaults on unparseable storage rather than throwing', () => {
    const s = fakeStorage()
    s.setItem('taiyaku.prefs.motion', 'not json')
    vi.stubGlobal('localStorage', s)
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: true,
      ms: 120,
    })
  })

  it('reset removes the slice so the defaults apply again', () => {
    vi.stubGlobal('localStorage', fakeStorage())
    savePrefs('motion', { animate: false, ms: 0 })
    resetPrefs('motion')
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: true,
      ms: 120,
    })
  })

  it('falls back to the defaults when the stored value parses but is a string, rather than spreading its character indices', () => {
    const s = fakeStorage()
    s.setItem('taiyaku.prefs.motion', JSON.stringify('hello'))
    vi.stubGlobal('localStorage', s)
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: true,
      ms: 120,
    })
  })

  it('falls back to the defaults when the stored value parses but is an array', () => {
    const s = fakeStorage()
    s.setItem('taiyaku.prefs.motion', JSON.stringify(['nope']))
    vi.stubGlobal('localStorage', s)
    expect(loadPrefs('motion', { animate: true, ms: 120 })).toEqual({
      animate: true,
      ms: 120,
    })
  })

  it('warns naming the preference id when saving fails, rather than failing silently', () => {
    vi.stubGlobal('localStorage', throwingStorage())
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    expect(() => savePrefs('motion', { animate: false, ms: 0 })).not.toThrow()
    expect(warn).toHaveBeenCalledTimes(1)
    expect(warn.mock.calls[0]?.[0]).toContain('motion')
    warn.mockRestore()
  })

  it('warns naming the preference id when reset fails, rather than failing silently', () => {
    vi.stubGlobal('localStorage', throwingStorage())
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    expect(() => resetPrefs('motion')).not.toThrow()
    expect(warn).toHaveBeenCalledTimes(1)
    expect(warn.mock.calls[0]?.[0]).toContain('motion')
    warn.mockRestore()
  })
})

describe('the storage namespace', () => {
  it('prefixes every key, so nothing this reader writes is unqualified', () => {
    expect(NAMESPACE).toBe('taiyaku.prefs')
  })

  // Two segments, not one: a later kind of stored value gets a sibling rather
  // than being mixed in with preferences.
  it('leaves room for a sibling that is not a preference', () => {
    const [root, kind] = NAMESPACE.split('.')
    expect(root).toBe('taiyaku')
    expect(kind).toBe('prefs')
  })
})

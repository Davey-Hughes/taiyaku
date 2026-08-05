import { describe, expect, it } from 'vitest'
import { fieldKind, fieldsOf, humanize, nestedCount, nestedSummary, withFieldValue } from './settingsPanel'

describe('fieldKind', () => {
  it('reads a boolean default as a checkbox field', () => {
    expect(fieldKind(true)).toBe('boolean')
  })

  it('reads a number default as a numeric field', () => {
    expect(fieldKind(120)).toBe('number')
  })

  it('reads a string default as a text field', () => {
    expect(fieldKind('dark')).toBe('string')
  })

  it('reads a nested record as an object field, not a broken primitive control', () => {
    expect(fieldKind({ 'en-mono': 'ltr' })).toBe('object')
  })

  it('falls back to object for anything that is not a boolean, number or string', () => {
    expect(fieldKind(null)).toBe('object')
    expect(fieldKind([1, 2, 3])).toBe('object')
    expect(fieldKind(undefined)).toBe('object')
  })
})

describe('fieldsOf', () => {
  it('lists one field per key, in declaration order', () => {
    expect(fieldsOf({ animate: true, ms: 120 })).toEqual([
      { key: 'animate', kind: 'boolean' },
      { key: 'ms', kind: 'number' },
    ])
  })

  it('marks a per-edition record as an object field rather than guessing a primitive control for it', () => {
    expect(fieldsOf({ override: {} })).toEqual([{ key: 'override', kind: 'object' }])
  })

  it('is empty for a module with nothing to tune, so a generated section renders no controls', () => {
    expect(fieldsOf({})).toEqual([])
  })
})

describe('withFieldValue', () => {
  it('flips a boolean field', () => {
    expect(withFieldValue({ animate: true, ms: 120 }, 'animate', false)).toEqual({
      animate: false,
      ms: 120,
    })
  })

  it('coerces a numeric field from the string an input element hands back', () => {
    expect(withFieldValue({ animate: true, ms: 120 }, 'ms', '80')).toEqual({ animate: true, ms: 80 })
  })

  it('accepts a number directly, without needing a string round-trip', () => {
    expect(withFieldValue({ ms: 120 }, 'ms', 80)).toEqual({ ms: 80 })
  })

  it('refuses an unparseable number rather than storing NaN', () => {
    expect(withFieldValue({ ms: 120 }, 'ms', 'abc')).toEqual({ ms: 120 })
  })

  it('passes a string field through unchanged in shape', () => {
    expect(withFieldValue({ theme: 'dark' }, 'theme', 'light')).toEqual({ theme: 'light' })
  })

  it('leaves an object-valued field alone — it is reset-only, never patched a key at a time here', () => {
    const prefs = { override: { 'en-mono': 'ltr' } }
    expect(withFieldValue(prefs, 'override', 'nope')).toEqual({ override: { 'en-mono': 'ltr' } })
  })

  it('ignores a key the prefs slice does not have, rather than adding an untyped one', () => {
    expect(withFieldValue({ animate: true }, 'nope', 5)).toEqual({ animate: true })
  })

  it('does not mutate the prefs object passed in', () => {
    const prefs = { animate: true, ms: 120 }
    withFieldValue(prefs, 'ms', 80)
    expect(prefs).toEqual({ animate: true, ms: 120 })
  })
})

describe('nestedCount', () => {
  it('counts the entries of a per-edition record', () => {
    expect(nestedCount({ 'en-mono': 'ltr', 'en-fan': 'rtl' })).toBe(2)
  })

  it('is zero for an empty record', () => {
    expect(nestedCount({})).toBe(0)
  })

  it('is zero for a non-object, rather than throwing', () => {
    expect(nestedCount(undefined)).toBe(0)
  })
})

describe('nestedSummary', () => {
  it('says none set at zero', () => {
    expect(nestedSummary(0)).toBe('none set')
  })

  it('counts one entry', () => {
    expect(nestedSummary(1)).toBe('1 set')
  })

  it('counts several entries', () => {
    expect(nestedSummary(3)).toBe('3 set')
  })
})

describe('humanize', () => {
  it('splits a camelCase key into lowercase words', () => {
    expect(humanize('maxConcurrent')).toBe('max concurrent')
  })

  it('leaves a single-word key alone but for case', () => {
    expect(humanize('ms')).toBe('ms')
    expect(humanize('animate')).toBe('animate')
  })
})


describe('fieldsOf with declared options', () => {
  it('infers a select for a string the module declared values for', () => {
    const fields = fieldsOf({ placement: 'auto', label: 'free text' }, { placement: ['auto', 'side'] })
    expect(fields).toEqual([
      { key: 'placement', kind: 'select', options: ['auto', 'side'] },
      { key: 'label', kind: 'string' },
    ])
  })

  // Options against a boolean or a number would be a module contradicting its
  // own default's type; the value decides the kind, which is what keeps this
  // panel from having to know which module it is looking at.
  it('ignores options declared against something that is not a string', () => {
    expect(fieldsOf({ on: true, depth: 2 }, { on: ['yes'], depth: ['1'] })).toEqual([
      { key: 'on', kind: 'boolean' },
      { key: 'depth', kind: 'number' },
    ])
  })

  it('leaves a string with an empty option list as free text', () => {
    expect(fieldsOf({ label: 'x' }, { label: [] })).toEqual([{ key: 'label', kind: 'string' }])
  })
})

import { describe, expect, it } from 'vitest'
import { AXES, KINDS, keysOf, LABEL_KEYS } from './labelKeys'
import type { LabelBox } from './labelling'

const box = (over: Partial<LabelBox> = {}): LabelBox => ({
  id: 'a',
  x: 0.1,
  y: 0.1,
  w: 0.2,
  h: 0.2,
  kind: 'dialogue',
  axis: 'v',
  ignore: false,
  ...over,
})

describe('keysOf', () => {
  it('collapses a contiguous run', () => {
    expect(keysOf({ keys: ['1', '2', '3', '4', '5', '6'], label: '' })).toBe(
      '1-6',
    )
  })

  it('writes unrelated keys out in full rather than as a range', () => {
    // `v-n` reads as a span from `v` to `n`: backwards, four keys wide, and
    // containing neither `h` nor anything a labeller could press. It reached
    // the screen because the rule was length alone.
    expect(keysOf({ keys: ['v', 'h', 'n'], label: '' })).toBe('v h n')
  })

  it('leaves one and two key rows alone', () => {
    expect(keysOf({ keys: ['Tab'], label: '' })).toBe('Tab')
    expect(keysOf({ keys: ['a', 'b'], label: '' })).toBe('a b')
  })

  it('does not collapse multi-character keys that happen to number three', () => {
    expect(keysOf({ keys: ['Home', 'End', 'Delete'], label: '' })).toBe(
      'Home End Delete',
    )
  })
})

describe('LABEL_KEYS', () => {
  it('answers a distinct key in exactly one row', () => {
    // Two rows claiming one key would make the handler's `find` decide by
    // table order, silently — and the legend would list the key twice with
    // two different descriptions, which is the drift this table exists to
    // rule out.
    const all = LABEL_KEYS.flatMap((b) => b.keys)
    expect(new Set(all).size).toBe(all.length)
  })

  it('sets each kind from its own digit, in KINDS order', () => {
    const kinds = LABEL_KEYS.find((b) => b.keys.includes('1'))
    for (const [i, kind] of KINDS.entries()) {
      const after = kinds?.apply?.([box()], 'a', String(i + 1))
      expect(after?.[0].kind, `digit ${i + 1}`).toBe(kind)
    }
  })

  it('sets each axis from its own key, null included', () => {
    const axes = LABEL_KEYS.find((b) => b.keys.includes('v'))
    for (const { key, axis } of AXES) {
      const after = axes?.apply?.([box({ axis: 'h' })], 'a', key)
      expect(after?.[0].axis, `key ${key}`).toBe(axis)
    }
  })

  it('describes every row it carries', () => {
    for (const binding of LABEL_KEYS) {
      expect(binding.label.length, binding.keys.join()).toBeGreaterThan(0)
      expect(binding.keys.length).toBeGreaterThan(0)
    }
  })
})

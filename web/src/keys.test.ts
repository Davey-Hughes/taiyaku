import { describe, expect, it } from 'vitest'
import { intentOf, isInteractiveTarget } from './keys'

const editions = ['raw', 'en-mono', 'en-colour']
const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  ({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods }) as KeyboardEvent

describe('intentOf', () => {
  it('maps space to the toggle', () => {
    expect(intentOf(press(' '), editions)).toEqual({ kind: 'toggle' })
  })

  it('maps a digit to the edition at that position, one-based', () => {
    expect(intentOf(press('1'), editions)).toEqual({ kind: 'view', edition: 'raw' })
    expect(intentOf(press('3'), editions)).toEqual({ kind: 'view', edition: 'en-colour' })
  })

  it('ignores a digit past the end rather than wrapping', () => {
    expect(intentOf(press('4'), editions)).toBeUndefined()
    expect(intentOf(press('0'), editions)).toBeUndefined()
  })

  it('maps the arrows to navigation', () => {
    expect(intentOf(press('ArrowRight'), editions)).toEqual({ kind: 'step', by: 1 })
    expect(intentOf(press('ArrowLeft'), editions)).toEqual({ kind: 'step', by: -1 })
  })

  it('maps tab to the chrome toggle', () => {
    expect(intentOf(press('Tab'), editions)).toEqual({ kind: 'chrome' })
  })

  it('yields every key to a modifier, so browser shortcuts still work', () => {
    expect(intentOf(press('1', { metaKey: true }), editions)).toBeUndefined()
    expect(intentOf(press(' ', { ctrlKey: true }), editions)).toBeUndefined()
  })
})

// A fake `EventTarget` — this file runs under Vitest's `node` environment, no
// `HTMLElement` global, so `isInteractiveTarget` is written to duck-type
// rather than `instanceof`, and this constructs exactly the shape it reads.
const target = (tagName: string, tabindex?: string) =>
  ({
    tagName,
    hasAttribute: (name: string) => name === 'tabindex' && tabindex !== undefined,
  }) as unknown as EventTarget

describe('isInteractiveTarget', () => {
  it('is true for a button, a link, and an input', () => {
    expect(isInteractiveTarget(target('BUTTON'))).toBe(true)
    expect(isInteractiveTarget(target('A'))).toBe(true)
    expect(isInteractiveTarget(target('INPUT'))).toBe(true)
  })

  it('is true for anything carrying tabindex, whatever its tag', () => {
    expect(isInteractiveTarget(target('DIV', '0'))).toBe(true)
    expect(isInteractiveTarget(target('DIV', '-1'))).toBe(true)
  })

  it('is false for an ordinary element with no tabindex', () => {
    expect(isInteractiveTarget(target('DIV'))).toBe(false)
    expect(isInteractiveTarget(target('IMG'))).toBe(false)
  })

  it('is false for no target at all', () => {
    expect(isInteractiveTarget(null)).toBe(false)
  })
})

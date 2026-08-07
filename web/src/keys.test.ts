import { describe, expect, it } from 'vitest'
import { intentOf, isInteractiveTarget } from './keys'
import type { ReaderModule } from './module'

const editions = ['raw', 'en-mono', 'en-colour']
const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  ({
    key,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    ...mods,
  }) as KeyboardEvent

const mod = (id: string, keys: string[]): ReaderModule<{ on: boolean }> => ({
  id,
  label: id,
  defaults: { on: true },
  keys: keys.map((key) => ({ key, intent: `${id}:${key}` })),
})

describe('intentOf', () => {
  it('maps space to the toggle', () => {
    expect(intentOf(press(' '), editions)).toEqual({ kind: 'toggle' })
  })

  it('maps a module-declared key to its declared intent', () => {
    const modules = [mod('direction', ['d'])]
    expect(intentOf(press('d'), editions, modules)).toEqual({
      kind: 'module',
      intent: 'direction:d',
    })
  })

  it('is silent on a key no module claims', () => {
    const modules = [mod('direction', ['d'])]
    expect(intentOf(press('z'), editions, modules)).toBeUndefined()
  })

  it('yields a module key to a modifier, same as every built-in', () => {
    const modules = [mod('direction', ['d'])]
    expect(
      intentOf(press('d', { ctrlKey: true }), editions, modules),
    ).toBeUndefined()
  })

  it('lets a built-in win over a module claiming the same key', () => {
    // Not a collision `buildRegistry` would catch — a module cannot declare
    // Space, an arrow, Tab or a digit in the first place, since those are the
    // reader's own reserved keys, not the registry's to arbitrate.
    const modules = [mod('spacebar-module', [' '])]
    expect(intentOf(press(' '), editions, modules)).toEqual({ kind: 'toggle' })
  })

  it('defaults to no modules, so existing callers need no change', () => {
    expect(intentOf(press('d'), editions)).toBeUndefined()
  })

  it('maps a digit to the edition at that position, one-based', () => {
    expect(intentOf(press('1'), editions)).toEqual({
      kind: 'view',
      edition: 'raw',
    })
    expect(intentOf(press('3'), editions)).toEqual({
      kind: 'view',
      edition: 'en-colour',
    })
  })

  it('ignores a digit past the end rather than wrapping', () => {
    expect(intentOf(press('4'), editions)).toBeUndefined()
    expect(intentOf(press('0'), editions)).toBeUndefined()
  })

  it('maps the arrows to navigation', () => {
    expect(intentOf(press('ArrowRight'), editions)).toEqual({
      kind: 'step',
      by: 1,
    })
    expect(intentOf(press('ArrowLeft'), editions)).toEqual({
      kind: 'step',
      by: -1,
    })
  })

  it('maps tab to the chrome toggle', () => {
    expect(intentOf(press('Tab'), editions)).toEqual({ kind: 'chrome' })
  })

  // Only the modifier guard: `intentOf` takes no event target, so standing
  // down for a focused control is not something this function can be asked
  // about — that guard lives in `App.tsx`'s keydown handler beside every
  // other intent's. Named for what it actually checks.
  it('c asks to correct, and yields to a modifier', () => {
    expect(intentOf(press('c'), ['raw', 'en'], [])).toEqual({ kind: 'correct' })
    expect(
      intentOf(press('c', { ctrlKey: true }), ['raw', 'en'], []),
    ).toBeUndefined()
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
    hasAttribute: (name: string) =>
      name === 'tabindex' && tabindex !== undefined,
  }) as unknown as EventTarget

describe('isInteractiveTarget', () => {
  it('is true for a button, a link, and an input', () => {
    expect(isInteractiveTarget(target('BUTTON'))).toBe(true)
    expect(isInteractiveTarget(target('A'))).toBe(true)
    expect(isInteractiveTarget(target('INPUT'))).toBe(true)
  })

  // A reader typing a note is the case this missed until M5c put the first
  // textarea in the reader: every module key fired mid-word, so typing `s`
  // opened the settings panel, `n` started another note, Space swapped
  // editions and a digit jumped to one. Reported from a real browser, where
  // the tests that "covered" the editor had set the value with `fill()` and
  // so never sent a keystroke through this at all.
  it('is true for a textarea — a reader typing a note is not pressing reader keys', () => {
    expect(isInteractiveTarget(target('TEXTAREA'))).toBe(true)
  })

  // The other way a page can be typed into. Nothing in this reader uses one
  // today; it is here because the rule is "is the target taking text", not "is
  // it one of three tag names", and the next surface that takes text should
  // not have to rediscover this bug.
  it('is true for a contenteditable element, whatever its tag', () => {
    const editable = {
      tagName: 'DIV',
      isContentEditable: true,
      hasAttribute: () => false,
    }
    expect(isInteractiveTarget(editable as unknown as EventTarget)).toBe(true)
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

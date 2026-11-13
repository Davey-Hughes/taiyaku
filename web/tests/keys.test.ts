import { describe, expect, it } from 'vitest'
import {
  controlKind,
  inPanel,
  intentOf,
  isInteractivePress,
  READER_KEYS,
  type ReaderBinding,
  withoutCorrecting,
} from '../src/keys'
import type { ReaderModule } from '../src/module'

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

  // The half of that the test above cannot see, since it passes no modules: a
  // digit past the end **stops**, it does not go on to offer the key around.
  // `4` is the reader's whether or not a fourth edition exists, so a module
  // claiming it stays silent on a three-edition work — otherwise the same
  // press would mean different things at different edition counts.
  //
  // `0` is the contrast, and deliberate: it is claimed by nobody here, which
  // is what leaves it free for `modules/zoom.ts` to take.
  it('stops on a digit past the end rather than offering it to a module', () => {
    const modules = [mod('greedy', ['4', '0'])]
    expect(intentOf(press('4'), editions, modules)).toBeUndefined()
    expect(intentOf(press('0'), editions, modules)).toEqual({
      kind: 'module',
      intent: 'greedy:0',
    })
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
  // about — that guard is `keyplan.ts`'s `controlKeeps`, in `keyPlan`. Named
  // for what it actually checks.
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

  it('L asks to go back to the library', () => {
    expect(intentOf(press('L'), editions)).toEqual({ kind: 'library' })
  })
})

describe('READER_KEYS', () => {
  it('describes every built-in the reader answers before modules', () => {
    const claimed = READER_KEYS.flatMap((b) => b.keys)
    for (const key of [' ', 'ArrowRight', 'ArrowLeft', 'Tab', 'c'])
      expect(claimed).toContain(key)
  })

  it('covers the digits as one row rather than nine', () => {
    const digits = READER_KEYS.filter((b) => b.keys.includes('1'))
    expect(digits).toHaveLength(1)
    expect(digits[0].keys).toEqual([
      '1',
      '2',
      '3',
      '4',
      '5',
      '6',
      '7',
      '8',
      '9',
    ])
  })

  // `0` staying unclaimed is what makes it safe for zoom to take — see
  // `modules/zoom.ts`, which says so in its own doc.
  it('leaves 0 unclaimed', () => {
    expect(READER_KEYS.flatMap((b) => b.keys)).not.toContain('0')
  })

  it('gives every row a label for the help to show', () => {
    for (const b of READER_KEYS) {
      expect(b.label.length).toBeGreaterThan(0)
      expect(b.keys.length).toBeGreaterThan(0)
    }
  })

  it('the library key is in the one table the help screen is generated from', () => {
    const row = READER_KEYS.find((b) => b.keys.includes('L'))
    expect(row?.label, 'a label, or Help cannot describe it').toBeTruthy()
  })

  // O58: the help of an account that may not correct leaves out `c`'s row and
  // nothing else, in the order the reader answers the keys.
  it('withoutCorrecting drops the correcting row alone and keeps the rest in order', () => {
    const kept = withoutCorrecting(READER_KEYS)
    const labels = READER_KEYS.map((b) => b.label)
    expect(labels).toContain('correct this pairing')
    expect(kept.map((b) => b.label)).toEqual(
      labels.filter((label) => label !== 'correct this pairing'),
    )
  })

  // Rebound, the correcting row has neither `c` nor its label, and `c` steps:
  // the row goes by what it asks for, and `c`'s new row stays.
  it('withoutCorrecting finds a rebound correcting row by its intent', () => {
    const steps: ReaderBinding = {
      keys: ['c'],
      label: 'next page',
      resolve: () => ({ kind: 'step', by: 1 }),
    }
    const corrects: ReaderBinding = {
      keys: ['x'],
      label: 'fix it',
      resolve: () => ({ kind: 'correct' }),
    }
    expect(withoutCorrecting([steps, corrects])).toEqual([steps])
  })

  // The property the ordering carries: a module cannot take a reader key,
  // which `intentOf`'s own comment calls "not a priority choice so much as
  // those keys not being a module's to claim".
  it('answers a built-in even when a module claims the same key', () => {
    const greedy = mod('greedy', [' ', 'c', '1'])
    expect(intentOf(press(' '), editions, [greedy])).toEqual({ kind: 'toggle' })
    expect(intentOf(press('c'), editions, [greedy])).toEqual({
      kind: 'correct',
    })
    expect(intentOf(press('1'), editions, [greedy])).toEqual({
      kind: 'view',
      edition: editions[0],
    })
  })
})

// A fake `EventTarget` — this file runs under Vitest's `node` environment, no
// `HTMLElement` global, so `controlKind` is written to duck-type rather than
// `instanceof`, and this constructs exactly the shape it reads.
const target = (tagName: string, extra: Record<string, unknown> = {}) =>
  ({ tagName, ...extra }) as unknown as EventTarget

describe('controlKind', () => {
  it('text entry keeps every key: a text input, a number, a textarea, a select', () => {
    expect(controlKind(target('INPUT', { type: 'text' }))).toBe('text')
    expect(controlKind(target('INPUT', { type: 'number' }))).toBe('text')
    expect(controlKind(target('INPUT', { type: 'password' }))).toBe('text')
    expect(controlKind(target('TEXTAREA'))).toBe('text')
  })

  // O166: several of Settings' fields are selects, and a key pressed on one
  // would reach the reader — `s` would close the panel under it, `→` would
  // turn the page.
  it('is text for a select', () => {
    expect(controlKind(target('SELECT'))).toBe('text')
  })

  // A reader typing a note would otherwise fire every module key mid-word. The
  // rule is "is the target taking text", so a contenteditable counts whatever
  // its tag.
  it('is text for a contenteditable element, whatever its tag', () => {
    expect(controlKind(target('DIV', { isContentEditable: true }))).toBe('text')
  })

  it('a button, a link and a checkbox keep only the keys that press them', () => {
    expect(controlKind(target('BUTTON'))).toBe('button')
    expect(controlKind(target('A'))).toBe('button')
    expect(controlKind(target('INPUT', { type: 'checkbox' }))).toBe('button')
    expect(controlKind(target('INPUT', { type: 'button' }))).toBe('button')
  })

  // The page area and every panel carry `tabindex` (spec §3.1, §3.3), so a
  // rule that counted it would stand every reader key down on all of them.
  it('keeps nothing for the page area or a panel, tabindex or not', () => {
    expect(controlKind(target('DIV', { tabIndex: -1 }))).toBe('none')
    expect(controlKind(target('DIV'))).toBe('none')
    expect(controlKind(target('IMG'))).toBe('none')
  })

  it('keeps nothing for no target at all', () => {
    expect(controlKind(null)).toBe('none')
  })
})

describe('isInteractivePress', () => {
  // This file runs with no DOM, so these fakes stand in for elements: the
  // function asks `closest` one question, and what is testable here is that
  // it asks it and trusts the answer. Whether the *selector* names the right
  // controls is DOM behaviour, checked in the browser, and that pass is
  // recorded in `docs/status.md`.
  const el = (match: unknown) => ({ closest: (_sel: string) => match })

  it('is false when nothing in the ancestry is a control', () => {
    expect(isInteractivePress(el(null) as unknown as EventTarget)).toBe(false)
  })

  it('is true when `closest` finds one', () => {
    expect(
      isInteractivePress(el({ tagName: 'BUTTON' }) as unknown as EventTarget),
    ).toBe(true)
  })

  it('asks about every kind of control', () => {
    let asked = ''
    const spy = {
      closest: (sel: string) => {
        asked = sel
        return null
      },
    }
    isInteractivePress(spy as unknown as EventTarget)
    for (const control of [
      'button',
      'a',
      'input',
      'textarea',
      'select',
      'tabindex',
      'contenteditable',
    ])
      expect(asked).toContain(control)
  })

  // The difference between the two rules, stated as a test: a press lands on
  // the text, and only the ancestor-aware one sees the button holding it.
  it('parts company with the keydown rule on a node inside a control', () => {
    const inner = { tagName: 'SPAN', closest: () => ({ tagName: 'BUTTON' }) }
    expect(isInteractivePress(inner as unknown as EventTarget)).toBe(true)
    expect(controlKind(inner as unknown as EventTarget)).toBe('none')
  })

  it('is false for a null target and for one that cannot be asked', () => {
    expect(isInteractivePress(null)).toBe(false)
    expect(isInteractivePress({} as EventTarget)).toBe(false)
  })
})

describe('inPanel', () => {
  it('is true inside a dialog, false outside one and for no target', () => {
    const inside = {
      closest: (s: string) => (s === '[role="dialog"]' ? {} : null),
    }
    const outside = { closest: () => null }
    expect(inPanel(inside as unknown as EventTarget)).toBe(true)
    expect(inPanel(outside as unknown as EventTarget)).toBe(false)
    expect(inPanel(null)).toBe(false)
  })
})

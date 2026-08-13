import { describe, expect, it } from 'vitest'
import { READER_KEYS } from '../keys'
import type { KeyBinding, ReaderModule } from '../module'
import {
  enabledModules,
  helpGroups,
  helpModule,
  keyLabel,
  keysLabel,
} from './help'
import { registry } from './registry'

const mod = (
  id: string,
  keys: KeyBinding[],
): ReaderModule<{ on: boolean }> => ({
  id,
  label: id,
  defaults: { on: true },
  keys,
})

describe('helpModule', () => {
  it('is registered', () => {
    expect(registry.ids).toContain('help')
  })

  it('opens on both ? and h', () => {
    expect((helpModule.keys ?? []).map((k) => k.key).sort()).toEqual(['?', 'h'])
  })

  it('gives both keys the same intent, so either opens the one panel', () => {
    const intents = new Set((helpModule.keys ?? []).map((k) => k.intent))
    expect(intents.size).toBe(1)
  })

  it('claims nothing the reader answers first', () => {
    expect(READER_KEYS.flatMap((b) => b.keys)).not.toContain('h')
    expect(READER_KEYS.flatMap((b) => b.keys)).not.toContain('?')
  })

  it('claims nothing another module already has', () => {
    const mine = new Set((helpModule.keys ?? []).map((k) => k.key))
    const others = registry
      .all()
      .filter((m) => m.id !== 'help')
      .flatMap((m) => m.keys ?? [])
      .map((k) => k.key)
    for (const key of others) expect(mine.has(key)).toBe(false)
  })

  it('is on by default — a help nobody can find helps nobody', () => {
    expect(helpModule.defaults.enabled).toBe(true)
  })
})

describe('keyLabel', () => {
  // The three keys in the two real tables that do not render as themselves.
  it('names the space bar, which would otherwise be an empty cell', () => {
    expect(keyLabel(' ')).toBe('Space')
  })

  it('draws the arrows as arrows', () => {
    expect(keyLabel('ArrowRight')).toBe('→')
    expect(keyLabel('ArrowLeft')).toBe('←')
  })

  it('covers the two arrows nothing declares yet', () => {
    expect(keyLabel('ArrowUp')).toBe('↑')
    expect(keyLabel('ArrowDown')).toBe('↓')
  })

  it('passes through a DOM name that already reads as a word', () => {
    expect(keyLabel('Tab')).toBe('Tab')
    expect(keyLabel('Escape')).toBe('Escape')
  })

  it('passes through every printable key the tables declare', () => {
    for (const key of ['c', 'd', 'n', 'z', 'Z', 'C', '=', '-', '0', '?', 'h']) {
      expect(keyLabel(key)).toBe(key)
    }
  })
})

describe('keysLabel', () => {
  it('collapses a consecutive run to a range', () => {
    expect(keysLabel(['1', '2', '3', '4', '5', '6', '7', '8', '9'])).toBe('1–9')
  })

  it('uses an en dash, because - is itself a key in this list', () => {
    expect(keysLabel(['1', '2', '3'])).not.toContain('-')
  })

  it('leaves two keys as two, a range being no shorter than the pair', () => {
    expect(keysLabel(['a', 'b'])).toBe('a / b')
  })

  it('joins keys that are not a run', () => {
    expect(keysLabel(['?', 'h'])).toBe('? / h')
  })

  it('does not treat multi-character names as a run', () => {
    expect(keysLabel(['ArrowLeft', 'ArrowRight'])).toBe('← / →')
  })

  it('leaves a single key alone', () => {
    expect(keysLabel([' '])).toBe('Space')
  })
})

describe('helpGroups', () => {
  it('leads with the reader table, one row per binding', () => {
    const groups = helpGroups([])
    expect(groups).toHaveLength(1)
    expect(groups[0].heading).toBe('Reading')
    expect(groups[0].rows).toHaveLength(READER_KEYS.length)
  })

  it('carries every reader binding through by its own label', () => {
    const said = helpGroups([])[0].rows.map((r) => r.description)
    for (const binding of READER_KEYS) expect(said).toContain(binding.label)
  })

  it('shows the digit row once, as a range', () => {
    const rows = helpGroups([])[0].rows
    const digits = rows.find((r) => r.description === 'jump to the nth edition')
    expect(digits?.keys).toBe('1–9')
  })

  it('takes a module heading from the module, not from a written list', () => {
    const groups = helpGroups([mod('made-up', [{ key: 'q', intent: 'q:it' }])])
    expect(groups[1]).toEqual({
      heading: 'made-up',
      rows: [{ keys: 'q', description: 'q:it' }],
    })
  })

  it('skips a module that declares no keys', () => {
    const keyless: ReaderModule<{ on: boolean }> = {
      id: 'keyless',
      label: 'Keyless',
      defaults: { on: true },
    }
    expect(helpGroups([keyless, mod('k', [])])).toHaveLength(1)
  })

  it('folds two keys with one intent into one row', () => {
    const groups = helpGroups([helpModule])
    expect(groups[1].rows).toEqual([
      { keys: '? / h', description: 'show this list of keys' },
    ])
  })

  it('says the binding label rather than the intent it dispatches on', () => {
    const groups = helpGroups([
      mod('m', [{ key: 'q', intent: 'm:verb', label: 'do the thing' }]),
    ])
    expect(groups[1].rows).toEqual([{ keys: 'q', description: 'do the thing' }])
  })

  it('takes the first label where two keys share one intent', () => {
    const groups = helpGroups([
      mod('m', [
        { key: 'q', intent: 'm:verb', label: 'the one row' },
        { key: 'w', intent: 'm:verb', label: 'the row that loses' },
      ]),
    ])
    expect(groups[1].rows).toEqual([
      { keys: 'q / w', description: 'the one row' },
    ])
  })

  it('keeps the modules in the order it was given them', () => {
    const groups = helpGroups([
      mod('second', [{ key: 'x', intent: 'a' }]),
      mod('third', [{ key: 'y', intent: 'b' }]),
    ])
    expect(groups.map((g) => g.heading)).toEqual(['Reading', 'second', 'third'])
  })

  it('accounts for every intent the registry declares', () => {
    const groups = helpGroups(registry.all())
    const said = groups.flatMap((g) => g.rows).map((r) => r.description)
    for (const m of registry.all()) {
      for (const binding of m.keys ?? []) {
        expect(said).toContain(binding.label ?? binding.intent)
      }
    }
  })

  // The fallback is the branch no registered module reaches, so it is checked
  // against a made-up one rather than left to a future binding to discover.
  it('falls back to the intent for a binding with no label', () => {
    for (const m of registry.all()) {
      for (const binding of m.keys ?? []) expect(binding.label).toBeDefined()
    }
    const groups = helpGroups([mod('m', [{ key: 'q', intent: 'm:verb' }])])
    expect(groups[1].rows).toEqual([{ keys: 'q', description: 'm:verb' }])
  })

  it('accounts for every module that declares a key', () => {
    const headings = helpGroups(registry.all()).map((g) => g.heading)
    for (const m of registry.all()) {
      if ((m.keys ?? []).length > 0) expect(headings).toContain(m.label)
    }
  })
})

describe('enabledModules', () => {
  const switchable: ReaderModule<{ enabled: boolean }> = {
    id: 'switchable',
    label: 'Switchable',
    defaults: { enabled: true },
    keys: [{ key: 'q', intent: 'switchable:go', label: 'go' }],
  }
  const always = mod('always', [
    { key: 'w', intent: 'always:go', label: 'go anyway' },
  ])

  it('drops a module a reader has switched off', () => {
    expect(
      enabledModules([switchable, always], { switchable: { enabled: false } }),
    ).toEqual([always])
  })

  it('keeps one that is switched on', () => {
    expect(
      enabledModules([switchable, always], { switchable: { enabled: true } }),
    ).toEqual([switchable, always])
  })

  // The decision this function is here to record: `enabled` is a convention
  // some modules have, and a module without one is on. Treating an absent flag
  // as off would empty the overlay of every key that actually works.
  it('keeps a module with no enabled preference, whatever else is false', () => {
    expect(enabledModules([always], { always: { on: false } })).toEqual([
      always,
    ])
  })

  it('reads the defaults where the map has no slice for a module yet', () => {
    const offByDefault: ReaderModule<{ enabled: boolean }> = {
      ...switchable,
      id: 'off-by-default',
      defaults: { enabled: false },
    }
    expect(enabledModules([switchable, offByDefault], {})).toEqual([switchable])
  })

  it('passes the whole registry through, since every module ships on', () => {
    const defaults = Object.fromEntries(
      registry.all().map((m) => [m.id, m.defaults as object]),
    )
    expect(enabledModules(registry.all(), defaults)).toEqual(registry.all())
  })
})

describe('helpGroups and the reading direction', () => {
  const reading = (dir: 'ltr' | 'rtl') =>
    Object.fromEntries(
      helpGroups([], dir)[0].rows.map((r) => [r.keys, r.description]),
    )

  it('says the right arrow advances in a left-to-right work', () => {
    const rows = reading('ltr')
    expect(rows['→']).toBe(
      'next page — or the nearest one forward, out of a gap or a borrowed page',
    )
    expect(rows['←']).toBe(
      'previous page — or the nearest one back, out of a gap or a borrowed page',
    )
  })

  // The bug this exists for: in a right-to-left work the next page is drawn to
  // the left, so a help screen saying "→ next page" is telling the reader to
  // press the key that goes back.
  it('swaps them in a right-to-left work', () => {
    const rows = reading('rtl')
    expect(rows['←']).toBe(
      'next page — or the nearest one forward, out of a gap or a borrowed page',
    )
    expect(rows['→']).toBe(
      'previous page — or the nearest one back, out of a gap or a borrowed page',
    )
  })

  it('leaves every non-arrow row alone in both directions', () => {
    const ltr = reading('ltr')
    const rtl = reading('rtl')
    for (const key of Object.keys(ltr))
      if (key !== '→' && key !== '←') expect(rtl[key]).toBe(ltr[key])
  })

  it('defaults to left-to-right, so an unthreaded caller reads as before', () => {
    expect(helpGroups([])[0].rows).toEqual(helpGroups([], 'ltr')[0].rows)
  })
})

describe('helpGroups pairs step rows by meaning, not by position', () => {
  // The case the first implementation got wrong: with three step-like rows it
  // reversed the list instead of pairing them, so one row kept its own label
  // and another took a stranger's.
  const table = [
    {
      keys: ['ArrowRight'],
      label: 'next page',
      resolve: () => ({ kind: 'step' as const, by: 1 as const }),
    },
    {
      keys: ['ArrowLeft'],
      label: 'previous page',
      resolve: () => ({ kind: 'step' as const, by: -1 as const }),
    },
    { keys: ['x'], label: 'something else', resolve: () => undefined },
  ]

  it('swaps only the two that mean forward and back', () => {
    const rows = Object.fromEntries(
      helpGroups([], 'rtl', table)
        .flatMap((g) => g.rows)
        .map((r) => [r.keys, r.description]),
    )
    expect(rows['←']).toBe('next page')
    expect(rows['→']).toBe('previous page')
    expect(rows.x).toBe('something else')
  })

  it('leaves a row alone when nothing else means the opposite', () => {
    const lonely = [table[0], table[2]]
    const rows = Object.fromEntries(
      helpGroups([], 'rtl', lonely)
        .flatMap((g) => g.rows)
        .map((r) => [r.keys, r.description]),
    )
    expect(rows['→']).toBe('next page')
  })
})

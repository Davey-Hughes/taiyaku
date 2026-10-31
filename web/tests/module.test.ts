import { describe, expect, it } from 'vitest'
import {
  buildRegistry,
  changeSlice,
  collisions,
  duplicateIds,
  fieldChanged,
  initialPrefs,
  persistPrefs,
  sliceFor,
  sliceReset,
  withSlice,
  type ReaderModule,
} from '../src/module'
import { withFieldValue } from '../src/settingsPanel'

const mod = (
  id: string,
  keys: string[] = [],
): ReaderModule<{ on: boolean }> => ({
  id,
  label: id,
  defaults: { on: true },
  keys: keys.map((key) => ({ key, intent: `${id}:${key}` })),
})

describe('collisions', () => {
  it('finds two enabled modules claiming one key', () => {
    expect(collisions([mod('a', ['g']), mod('b', ['g'])])).toEqual([
      { key: 'g', modules: ['a', 'b'] },
    ])
  })

  it('is silent when every key is claimed once', () => {
    expect(collisions([mod('a', ['g']), mod('b', ['h'])])).toEqual([])
  })

  it("ignores a key claimed twice by one module, which is that module's business", () => {
    expect(collisions([mod('a', ['g', 'g'])])).toEqual([])
  })

  it('names all three modules when three claim one key, not just the first two', () => {
    expect(
      collisions([mod('a', ['g']), mod('b', ['g']), mod('c', ['g'])]),
    ).toEqual([{ key: 'g', modules: ['a', 'b', 'c'] }])
  })
})

describe('duplicateIds', () => {
  it('finds two modules sharing one id', () => {
    expect(duplicateIds([mod('a'), mod('a')])).toEqual(['a'])
  })

  it('is silent when every id is unique', () => {
    expect(duplicateIds([mod('a'), mod('b')])).toEqual([])
  })
})

describe('buildRegistry', () => {
  it('throws on a collision rather than letting two modules fight at runtime', () => {
    expect(() => buildRegistry([mod('a', ['g']), mod('b', ['g'])])).toThrow(
      /\bg\b/,
    )
  })

  it('names every clashing module in the thrown message when three claim one key', () => {
    let message = ''
    try {
      buildRegistry([mod('a', ['g']), mod('b', ['g']), mod('c', ['g'])])
    } catch (err) {
      message = (err as Error).message
    }
    expect(message).toContain('a')
    expect(message).toContain('b')
    expect(message).toContain('c')
  })

  it('throws on a duplicate id rather than letting the later entry silently shadow the earlier one', () => {
    expect(() => buildRegistry([mod('a'), mod('a')])).toThrow(/\ba\b/)
  })

  it('exposes modules by id and preserves declaration order', () => {
    const r = buildRegistry([mod('b'), mod('a')])
    expect(r.ids).toEqual(['b', 'a'])
    expect(r.get('a')?.label).toBe('a')
  })

  it('all() returns every registered module, for a settings panel to enumerate', () => {
    const r = buildRegistry([mod('b'), mod('a')])
    expect(r.all().map((m) => m.id)).toEqual(['b', 'a'])
  })
})

describe('sliceFor', () => {
  it("reads a module's own slice back out of a map keyed by id", () => {
    const a = mod('a')
    const map = { a: { on: false } }
    expect(sliceFor(map, a)).toEqual({ on: false })
  })

  it("falls back to the module's defaults when the map has no entry for it", () => {
    const a = mod('a')
    expect(sliceFor({}, a)).toBe(a.defaults)
  })

  it("reads only its own module's entry, ignoring the rest of the map", () => {
    const a = mod('a')
    const b = mod('b')
    const map = { a: { on: false }, b: { on: true } }
    expect(sliceFor(map, a)).toEqual({ on: false })
    expect(sliceFor(map, b)).toEqual({ on: true })
  })
})

describe('the generic per-module preference map App.tsx builds', () => {
  interface ExtraPrefs {
    on: boolean
    n: number
  }
  const extra: ReaderModule<ExtraPrefs> = {
    id: 'extra',
    label: 'Extra',
    defaults: { on: true, n: 5 },
  }

  it('a module known only to the registry is read, changed and reset through sliceFor/withFieldValue alone — nothing here names "extra"', () => {
    const built = buildRegistry([mod('a'), extra])
    // App.tsx's initial state: one slice per registered module, keyed by id —
    // `loadPrefs` in the real component, a plain copy of `defaults` here since
    // this test has no `localStorage` to merge against.
    let map: Record<string, object> = Object.fromEntries(
      built.all().map((m) => [m.id, { ...(m.defaults as object) }]),
    )

    // `Settings.tsx`'s onChange path — `handleSettingChange`'s shape.
    map = { ...map, [extra.id]: withFieldValue(sliceFor(map, extra), 'n', '9') }
    expect(sliceFor(map, extra)).toEqual({ on: true, n: 9 })

    // `Settings.tsx`'s onReset path — `handleSettingReset`'s shape.
    map = { ...map, [extra.id]: { ...extra.defaults } }
    expect(sliceFor(map, extra)).toEqual({ on: true, n: 5 })
  })
})

/**
 * The preferences map, as six rules rather than six hand-written spreads.
 *
 * A hand-written merge would drop every other edition's saved value, which
 * `direction.ts`'s `setOverride` exists to prevent.
 */

const two = (): ReaderModule<{ on: boolean; size: number }> => ({
  id: 'two',
  label: 'Two',
  defaults: { on: true, size: 3 },
})

const nested = (): ReaderModule<{ override: Record<string, string> }> => ({
  id: 'nested',
  label: 'Nested',
  defaults: { override: {} },
})

describe('withSlice', () => {
  it('replaces one module and leaves the rest', () => {
    const map = { two: { on: false, size: 1 }, other: { x: 1 } }
    const next = withSlice(map, two(), { on: true, size: 9 })
    expect(next).toEqual({ two: { on: true, size: 9 }, other: { x: 1 } })
  })

  /**
   * A `setPrefsMap` updater must stay pure — React may invoke it more than
   * once, e.g. under StrictMode — so the map it is handed cannot be written
   * into. Asserted on identity, which is the only way to see the difference.
   */
  it('never writes into the map it was given', () => {
    const map = { two: { on: false, size: 1 } }
    const next = withSlice(map, two(), { on: true, size: 9 })
    expect(next).not.toBe(map)
    expect(map.two).toEqual({ on: false, size: 1 })
  })
})

describe('changeSlice', () => {
  it('applies one rule to one module’s current slice', () => {
    const map = { two: { on: false, size: 1 }, other: { x: 1 } }
    const next = changeSlice(map, two(), (prefs) => ({
      ...prefs,
      on: !prefs.on,
    }))
    expect(next.two).toEqual({ on: true, size: 1 })
    expect(next.other).toEqual({ x: 1 })
  })

  /** A module registered but never written falls back to its own defaults, the same fallback `sliceFor` applies at every read. */
  it('changes a module with no entry yet, from its defaults', () => {
    const next = changeSlice({}, two(), (prefs) => ({ ...prefs, size: 7 }))
    expect(next.two).toEqual({ on: true, size: 7 })
  })

  /**
   * **The defect this shape exists to prevent.** A rule that returns only the
   * field it changed drops the rest of the slice — which is exactly what
   * `direction.ts`'s `setOverride` exists to prevent, and what makes the
   * *rule* rather than the merge the thing to get right.
   */
  it('keeps whatever the rule keeps, and nothing more', () => {
    const dropped = changeSlice({}, two(), () => ({ on: false }) as never)
    expect(dropped.two).toEqual({ on: false })
  })
})

describe('initialPrefs', () => {
  it('loads every registered module’s slice, keyed by id', () => {
    const loaded: string[] = []
    const map = initialPrefs([two(), nested()], (id, defaults) => {
      loaded.push(id)
      return { ...defaults, loaded: true }
    })
    expect(loaded).toEqual(['two', 'nested'])
    expect(map.two).toEqual({ on: true, size: 3, loaded: true })
    expect(map.nested).toEqual({ override: {}, loaded: true })
  })

  it('is empty for an empty registry', () => {
    expect(initialPrefs([], () => ({}))).toEqual({})
  })
})

describe('persistPrefs', () => {
  it('saves every module’s slice, not only the one that moved', () => {
    const saved: [string, object][] = []
    persistPrefs([two(), nested()], { two: { on: false, size: 1 } }, (id, p) =>
      saved.push([id, p]),
    )
    expect(saved).toEqual([
      ['two', { on: false, size: 1 }],
      // A module with no entry yet saves its own defaults rather than
      // `undefined`, which would put a `null` in storage for the next load.
      ['nested', { override: {} }],
    ])
  })
})

describe('fieldChanged and sliceReset', () => {
  const registry = buildRegistry([
    two() as ReaderModule<unknown>,
    nested() as ReaderModule<unknown>,
  ])

  it('coerces the control’s raw value through withFieldValue', () => {
    const next = fieldChanged({}, registry, 'two', 'size', '12')
    expect(next.two).toEqual({ on: true, size: 12 })
  })

  it('refuses a key the module does not declare', () => {
    const next = fieldChanged({}, registry, 'two', 'nope', 1)
    expect(next.two).toEqual({ on: true, size: 3 })
  })

  /** A stale panel must not write a slice no module will ever read back. */
  it('leaves the map alone for an id the registry does not know', () => {
    const map = { two: { on: false, size: 1 } }
    expect(fieldChanged(map, registry, 'ghost', 'on', true)).toBe(map)
    expect(sliceReset(map, registry, 'ghost')).toBe(map)
  })

  it('resets one module to its defaults, leaving the rest', () => {
    const map = {
      two: { on: false, size: 99 },
      nested: { override: { a: 'b' } },
    }
    const next = sliceReset(map, registry, 'two')
    expect(next.two).toEqual({ on: true, size: 3 })
    expect(next.nested).toEqual({ override: { a: 'b' } })
  })

  /**
   * A *copy* of `defaults`, never the module's own literal — a reset that
   * handed the literal to state would let the next `fieldChanged` spread from
   * an object the module itself is still holding.
   */
  it('resets to a copy, not to the module’s own defaults object', () => {
    const mod = two()
    const single = buildRegistry([mod as ReaderModule<unknown>])
    const next = sliceReset({}, single, 'two')
    expect(next.two).toEqual(mod.defaults)
    expect(next.two).not.toBe(mod.defaults)
  })
})

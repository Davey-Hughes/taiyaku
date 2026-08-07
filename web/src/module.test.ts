import { describe, expect, it } from 'vitest'
import {
  buildRegistry,
  collisions,
  duplicateIds,
  sliceFor,
  type ReaderModule,
} from './module'
import { withFieldValue } from './settingsPanel'

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

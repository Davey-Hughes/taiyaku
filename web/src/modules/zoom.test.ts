import { describe, expect, it } from 'vitest'
import { registry } from './registry'
import { zoomModule } from './zoom'

describe('zoomModule', () => {
  it('is registered', () => {
    // `Registry.ids` is a property, not a method — see `module.ts`'s own
    // interface and `annotations.test.ts`'s `expect(registry.ids).toContain`.
    expect(registry.ids).toContain('zoom')
  })

  it("claims three keys, none of them another module's", () => {
    const mine = new Set((zoomModule.keys ?? []).map((k) => k.key))
    expect(mine).toEqual(new Set(['=', '-', '0']))
    const others = registry
      .all()
      .filter((m) => m.id !== 'zoom')
      .flatMap((m) => m.keys ?? [])
      .map((k) => k.key)
    for (const key of others) expect(mine.has(key)).toBe(false)
  })

  // `intentOf` answers ' ', arrows, Tab, 'c' and 1-9 before it reaches any
  // module, so a module claiming one of those would declare a binding that
  // never fires rather than collide loudly.
  it('claims nothing intentOf answers before modules', () => {
    const reserved = new Set([
      ' ',
      'ArrowRight',
      'ArrowLeft',
      'Tab',
      'c',
      ...'123456789',
    ])
    for (const { key } of zoomModule.keys ?? [])
      expect(reserved.has(key)).toBe(false)
  })

  it('offers onSwap as a choice rather than free text', () => {
    expect(zoomModule.options?.onSwap).toEqual(['content', 'reset'])
  })

  it('starts enabled, at fit-whole magnification', () => {
    expect(zoomModule.defaults.enabled).toBe(true)
    expect(zoomModule.defaults.onSwap).toBe('content')
    expect(zoomModule.defaults.maxScale).toBeGreaterThan(1)
    expect(zoomModule.defaults.step).toBeGreaterThan(1)
  })
})

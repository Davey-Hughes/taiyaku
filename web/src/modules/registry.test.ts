import { describe, expect, it } from 'vitest'
import { registry } from './registry'

describe('registry', () => {
  it('registers all nine modules this build ships, in the order they are declared', () => {
    expect(registry.ids).toEqual([
      'direction',
      'motion',
      'regroup',
      'readouts',
      'prefetch',
      'annotations',
      'settings',
      'credits',
      'zoom',
    ])
  })

  it('every module has a non-empty label, for the settings panel section heading', () => {
    for (const mod of registry.all()) {
      expect(mod.label.length).toBeGreaterThan(0)
    }
  })
})

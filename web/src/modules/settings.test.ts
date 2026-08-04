import { describe, expect, it } from 'vitest'
import { settingsModule } from './settings'

describe('settingsModule', () => {
  it('defaults to an empty slice — nothing to tune, only a key to claim', () => {
    expect(settingsModule.defaults).toEqual({})
  })

  it('claims exactly one key', () => {
    expect(settingsModule.keys?.map((k) => k.key)).toEqual(['s'])
  })
})

import { describe, expect, it } from 'vitest'
import { browseModule, browseRegistry } from '../src/browseModule'
import { fieldsOf } from '../src/settingsPanel'

describe('the browse module', () => {
  it('declares §3’s preset A as its defaults', () => {
    expect(browseModule.defaults).toEqual({
      layout: 'grid',
      editionSelector: true,
      panel: 'button',
      density: 'comfortable',
      continueStrip: true,
    })
  })

  it('generates a control for every preference, and a choice where there are options', () => {
    const fields = fieldsOf(browseModule.defaults, browseModule.options)
    expect(fields.map((f) => [f.key, f.kind])).toEqual([
      ['layout', 'select'],
      ['editionSelector', 'boolean'],
      ['panel', 'select'],
      ['density', 'select'],
      ['continueStrip', 'boolean'],
    ])
  })

  it('offers exactly the values §3’s table names', () => {
    const fields = fieldsOf(browseModule.defaults, browseModule.options)
    const options = (key: string) => fields.find((f) => f.key === key)?.options
    expect(options('layout')).toEqual(['grid', 'table'])
    expect(options('panel')).toEqual(['off', 'button', 'on-select'])
    expect(options('density')).toEqual(['compact', 'comfortable'])
  })

  it('is a registry of its own, holding exactly one module', () => {
    expect(browseRegistry.ids).toEqual(['browse'])
    expect(browseRegistry.get('browse')).toBe(browseModule)
  })

  it('claims no key, so it can never collide with a reader’s', () => {
    expect(browseModule.keys ?? []).toEqual([])
  })
})

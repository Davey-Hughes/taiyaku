import { describe, expect, it } from 'vitest'
import {
  type BrowsePrefs,
  browseModule,
  browseRegistry,
  coverPositionOf,
  coverSizeAt,
  coverSizeOf,
  coverSizeScale,
  putsSeriesUnder,
} from '../src/browseModule'
import { fieldsOf } from '../src/settingsPanel'

describe('the browse module', () => {
  it('declares §3’s preset A as its defaults', () => {
    expect(browseModule.defaults).toEqual({
      layout: 'grid',
      editionSelector: true,
      panel: 'button',
      coverSize: 144,
      continueStrip: true,
      alwaysShowAligned: false,
      backFromReading: 'seriesPage',
    })
  })

  it('generates a control for every preference, a choice where there are options and a slider where there is a scale', () => {
    const fields = fieldsOf(
      browseModule.defaults,
      browseModule.options,
      browseModule.sliders,
    )
    expect(fields.map((f) => [f.key, f.kind])).toEqual([
      ['layout', 'select'],
      ['editionSelector', 'boolean'],
      ['panel', 'select'],
      ['coverSize', 'slider'],
      ['continueStrip', 'boolean'],
      ['alwaysShowAligned', 'boolean'],
      ['backFromReading', 'select'],
    ])
  })

  it('declares the cover size a slider on its own scale, 144 at the middle', () => {
    const field = fieldsOf(
      browseModule.defaults,
      browseModule.options,
      browseModule.sliders,
    ).find((f) => f.key === 'coverSize')
    expect(field?.initial).toBe(144)
    expect(field?.scale).toBe(coverSizeScale)
    expect(coverSizeScale.valueAt(coverSizeScale.last / 2)).toBe(144)
    expect(coverSizeScale.unit).toBe('px')
  })

  it('offers exactly the values §3’s table names', () => {
    const fields = fieldsOf(browseModule.defaults, browseModule.options)
    const options = (key: string) => fields.find((f) => f.key === key)?.options
    expect(options('layout')).toEqual(['grid', 'table'])
    expect(options('panel')).toEqual(['off', 'button', 'on-select'])
    expect(options('backFromReading')).toEqual([
      'seriesPage',
      'whereYouCameFrom',
    ])
    expect(Object.keys(browseModule.options ?? {})).toEqual([
      'layout',
      'panel',
      'backFromReading',
    ])
  })

  it('is a registry of its own, holding exactly one module', () => {
    expect(browseRegistry.ids).toEqual(['browse'])
    expect(browseRegistry.get('browse')).toBe(browseModule)
  })

  it('claims no key, so it can never collide with a reader’s', () => {
    expect(browseModule.keys ?? []).toEqual([])
  })
})

describe('the cover-size scale (O285)', () => {
  it('runs 96 to 144 in 1 px and 144 to 240 in 2 px, 144 at the middle', () => {
    expect([0, 47, 48, 49, 96].map(coverSizeAt)).toEqual([
      96, 143, 144, 146, 240,
    ])
    const positions = Array.from({ length: 97 }, (_, p) => p)
    expect(positions.map((p) => coverPositionOf(coverSizeAt(p)))).toEqual(
      positions,
    )
  })

  it('draws a size off the scale at the nearest position', () => {
    expect(coverPositionOf(104)).toBe(8)
    // Cast, as a provider other than the shell can hand in any value.
    const drawn = (coverSize: unknown) =>
      coverSizeOf({ ...browseModule.defaults, coverSize: coverSize as number })
    expect([151, 153, 239, 50, 1000, -5].map(drawn)).toEqual([
      152, 154, 240, 96, 240, 96,
    ])
    expect(
      [Number.NaN, Number.POSITIVE_INFINITY, '144', '200'].map(drawn),
    ).toEqual([144, 144, 144, 144])
  })
})

describe('the way back from a reader (O431)', () => {
  it('puts the series page under a reader for any value but where you came from', () => {
    // Cast, as `loadBrowsePrefs` lets any stored value through.
    const under = (backFromReading: unknown) =>
      putsSeriesUnder({
        ...browseModule.defaults,
        backFromReading: backFromReading as BrowsePrefs['backFromReading'],
      })
    expect(
      ['whereYouCameFrom', 'seriesPage', 'home', '', 1].map(under),
    ).toEqual([false, true, true, true, true])
    const { backFromReading: _dropped, ...missing } = browseModule.defaults
    expect(putsSeriesUnder(missing as BrowsePrefs)).toBe(true)
  })
})

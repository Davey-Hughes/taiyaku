import { describe, expect, it } from 'vitest'
import { layoutModule } from './layout'
import { registry } from './registry'

describe('layoutModule', () => {
  // Registration is the whole mechanism: it is what makes the mode load, save
  // and reach the settings panel. Without this entry the reader forgets the
  // mode on every reload, which is the defect this module exists to close.
  it('is registered', () => {
    expect(registry.ids).toContain('layout')
  })

  it('starts on single page, which is what the reader did before it persisted', () => {
    expect(layoutModule.defaults.mode).toBe('single')
  })

  it('offers exactly the three modes the page area can draw', () => {
    expect(layoutModule.options?.mode).toEqual(['single', 'two', 'scroll'])
  })

  // A string preference with declared options renders as a select; without the
  // options it would be a free-text box, and a typo would put the reader in a
  // mode that does not exist.
  it('declares its options, so the generated control is a choice', () => {
    expect(layoutModule.options?.mode).toContain(layoutModule.defaults.mode)
  })

  it('claims no keys — the mode has buttons in the top bar, shown with the strip', () => {
    expect(layoutModule.keys ?? []).toEqual([])
  })
})

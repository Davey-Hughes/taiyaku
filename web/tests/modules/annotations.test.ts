import { describe, expect, it } from 'vitest'
import { collisions } from '../../src/module'
import {
  annotationsModule,
  noteCountLine,
  toggleReveal,
} from '../../src/modules/annotations'
import { registry } from '../../src/modules/registry'

describe('annotationsModule', () => {
  it('claims a key for each of its gestures', () => {
    expect(annotationsModule.keys?.map((k) => k.key)).toEqual([
      'n',
      'r',
      'a',
      'z',
      'Z',
    ])
  })

  // The failure declared keybindings exist to prevent, checked against the
  // real registry rather than a hand-built list — a module added later that
  // claims `n` must fail here, not fight `annotations` silently at runtime.
  it('collides with nothing else registered', () => {
    expect(collisions(registry.all())).toEqual([])
  })

  it('is registered, so its preferences are wired and the panel shows it', () => {
    expect(registry.ids).toContain('annotations')
  })

  // Every default is a primitive, which is what makes `Settings.tsx` generate
  // a working control for each — a nested record renders read-only (see
  // `settingsPanel.ts`'s `fieldKind`), which is right for a per-edition map
  // and wrong for a plain toggle.
  it('declares only primitives, so every setting is editable in the panel', () => {
    for (const value of Object.values(annotationsModule.defaults)) {
      expect(['boolean', 'string']).toContain(typeof value)
    }
  })

  // The contract's first declared options. A string with no options renders as
  // a text box, which for a placement is a control that invites a typo and
  // answers with nothing.
  it('declares the values its placement setting may take', () => {
    expect(annotationsModule.options?.notesList).toEqual([
      'auto',
      'side',
      'below',
    ])
  })

  // Travel is the whole feature, so it is on. Revealing is off: the margin
  // tabs already say where the notes are without drawing on the page, so a
  // reader who wants the artwork gets it until they ask otherwise.
  it('defaults to travelling notes, and to drawing nothing over the page', () => {
    expect(annotationsModule.defaults).toEqual({
      markers: true,
      travel: true,
      reveal: false,
      notesList: 'auto',
    })
  })
})

describe('noteCountLine', () => {
  it('says nothing where the work has no notes at all', () => {
    expect(noteCountLine(0, 0)).toBeUndefined()
  })

  it('counts what is on this page against the work', () => {
    expect(noteCountLine(2, 7)).toBe('2 notes here of 7')
  })

  it('says none here rather than a zero that reads like a bug', () => {
    expect(noteCountLine(0, 7)).toBe('no notes here of 7')
  })

  it('does not pluralise one', () => {
    expect(noteCountLine(1, 1)).toBe('1 note here of 1')
  })
})

describe('toggleReveal', () => {
  it('turns the regions on and off again', () => {
    const off = annotationsModule.defaults
    const on = toggleReveal(off)
    expect(on.reveal).toBe(true)
    expect(toggleReveal(on).reveal).toBe(false)
  })

  /**
   * The reason this is a function rather than a spread at the call site: a
   * hand-written one is where the field beside it gets dropped, which is the
   * defect `direction.ts`'s `setOverride` was extracted over.
   */
  it('leaves every other preference alone', () => {
    const next = toggleReveal({
      markers: false,
      travel: true,
      reveal: false,
      notesList: 'below',
    })
    expect(next).toEqual({
      markers: false,
      travel: true,
      reveal: true,
      notesList: 'below',
    })
  })
})

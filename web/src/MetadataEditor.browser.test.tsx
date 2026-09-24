import { describe, expect, it } from 'vitest'
// Required, the same reason every other file in this project gives: the
// rules under test live in `admin.css`, which reaches the app through
// `index.css`. Without this the boxes below measure browser defaults.
import './index.css'
import { mount } from './testing/mount'

/**
 * I1 of the whole-branch review. `MetadataEditor.tsx:744` renders
 * `<label className="field title">` on the Series page, ②'s shipped screen,
 * and no `.metadata-editor` rule exists anywhere. Before ⑤a hoisted `.field`
 * out of `.proposal-detail`, a bare `.field` selector never reached it;
 * after the hoist it did, restyling a screen nobody decided to change.
 *
 * Built from the class list by hand rather than by mounting `MetadataEditor`
 * itself — the defect is entirely about which stylesheet rule a class list
 * reaches, not about the component's own behaviour, and the controller's own
 * measurement compared exactly this pair: a hoisted `.field` beside an
 * untouched control in the same `.library`.
 */
describe('the Series page’s title field, after the Rules screen hoisted .field', () => {
  it('is not restyled by a class the Series page never opted into', () => {
    const host = mount(
      <div className="library">
        <label className="field title">
          <span>Title</span>
          <input />
        </label>
        <label className="untouched">
          <span>Untouched</span>
          <input />
        </label>
      </div>,
    )
    const label = host.querySelector('.field.title')
    const hoisted = host.querySelector('.field.title input')
    const untouched = host.querySelector('.untouched input')
    if (!label || !hoisted || !untouched) {
      throw new Error('the fixture did not render')
    }

    // Measured by the controller: uppercase, 11.2px, `display: grid` and a
    // 0.672px letter-spacing on the label; `--bg` rather than `--field`, 4px
    // rather than `--radius-control`, and 12.8px rather than 16px on the
    // input. Compared against the untouched control rather than against
    // hard-coded numbers, so this still holds if a token's value changes.
    expect(getComputedStyle(label).textTransform).toBe('none')
    expect(getComputedStyle(hoisted).backgroundColor).toBe(
      getComputedStyle(untouched).backgroundColor,
    )
    expect(getComputedStyle(hoisted).borderRadius).toBe(
      getComputedStyle(untouched).borderRadius,
    )
    expect(getComputedStyle(hoisted).fontSize).toBe(
      getComputedStyle(untouched).fontSize,
    )
  })
})

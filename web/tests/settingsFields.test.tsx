import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { FieldControl } from '../src/settingsFields'
import { mount } from './support/mount'

const noStyle = {}

describe('FieldControl', () => {
  it('renders a checkbox for a boolean and reports the new value', () => {
    const onChange = vi.fn()
    const host = mount(
      <FieldControl
        field={{ key: 'continueStrip', kind: 'boolean' }}
        value={true}
        onChange={onChange}
        rowStyle={noStyle}
      />,
    )
    const box = host.querySelector('input[type="checkbox"]') as HTMLInputElement
    expect(box.checked).toBe(true)
    act(() => {
      box.click()
    })
    expect(onChange).toHaveBeenCalledWith(false)
  })

  it('renders a select for a string with options, humanised', () => {
    const host = mount(
      <FieldControl
        field={{
          key: 'panel',
          kind: 'select',
          options: ['off', 'on-select', 'onSelect'],
        }}
        value={'off'}
        onChange={() => {}}
        rowStyle={noStyle}
      />,
    )
    // `humanize` splits camelCase and lowercases; it does not touch a
    // hyphen. `'on-select'` alone would prove nothing — it is a no-op for
    // `humanize` (`humanize('on-select') === 'on-select'`), so a render that
    // never called `humanize` on its options at all would pass this
    // assertion just as well. `'onSelect'` is the discriminating case:
    // `humanize('onSelect') === 'on select'`, which only a render that
    // actually humanizes its options can produce.
    expect(
      [...host.querySelectorAll('option')].map((o) => o.textContent),
    ).toEqual(['off', 'on-select', 'on select'])
  })

  it('summarises an object rather than inventing a control for it', () => {
    const host = mount(
      <FieldControl
        field={{ key: 'breaks', kind: 'object' }}
        value={{ a: 1, b: 2 }}
        onChange={() => {}}
        rowStyle={noStyle}
      />,
    )
    expect(host.textContent).toContain('2')
    expect(host.querySelector('input')).toBeNull()
  })

  it('takes its row styling from its caller', () => {
    // Deliberately not `'6px 0'` — that is `Settings.tsx`'s own module-scope
    // `rowStyle` value, which used to be what this component closed over. A
    // buggy move that left a same-valued constant behind in `settingsFields.tsx`
    // and never wired the prop through would still satisfy an assertion phrased
    // against `'6px 0'`, so this uses a value that appears nowhere else, which
    // only the prop path can produce.
    const host = mount(
      <FieldControl
        field={{ key: 'x', kind: 'boolean' }}
        value={false}
        onChange={() => {}}
        rowStyle={{ padding: '3px 9px' }}
      />,
    )
    expect((host.firstElementChild as HTMLElement).style.padding).toBe(
      '3px 9px',
    )
  })
})

import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ReaderModule } from './module'
import { Settings } from './Settings'
import { mount } from './testing/mount'

/**
 * `Settings` is generated purely from `settingsPanel.ts`'s inference, which
 * already has its own test file — `fieldKind`, `fieldsOf`, `humanize`,
 * `nestedCount` and `nestedSummary` are pinned there, branch by branch. What
 * is only pinned here is what the *component* does with that inference: does
 * it render a control per field kind, does it wire a control's change event
 * back to `onChange(id, key, raw)` uncoerced (coercion is `withFieldValue`'s
 * job, also tested elsewhere and not re-tested here), does a fieldless
 * module get no section, and does Reset call `onReset(id)`.
 */

interface ModAPrefs {
  enabled: boolean
  count: number
  name: string
  choice: string
  nested: Record<string, number>
}

const modA: ReaderModule<ModAPrefs> = {
  id: 'modA',
  label: 'Module A',
  defaults: {
    enabled: true,
    count: 5,
    name: 'hello',
    choice: 'x',
    nested: { a: 1, b: 2 },
  },
  options: { choice: ['x', 'y'] },
}

// `modules/settings.ts`'s own `settingsModule` is exactly this shape — the
// case `Settings`'s doc calls out by name as rendering no section at all.
const modEmpty: ReaderModule<Record<string, never>> = {
  id: 'empty',
  label: 'Empty Module',
  defaults: {},
}

const modules: ReaderModule<unknown>[] = [modA, modEmpty]

// A minimal, dependency-free stand-in for `@testing-library/react`'s
// `fireEvent`, matching how a real browser actually dispatches these for
// each control kind: `input` for a text/number box, `change` for a select, a
// real `.click()` for a checkbox (which jsdom's activation behaviour turns
// into a checked toggle plus its own `input`/`change` pair). Setting `.value`
// directly rather than through the element's own property would be silently
// swallowed by React's change-tracking on a controlled input, so this goes
// through the prototype's own setter instead.
function fireInput(el: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    'value',
  )?.set
  setter?.call(el, value)
  el.dispatchEvent(new Event('input', { bubbles: true }))
}

function fireSelect(el: HTMLSelectElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLSelectElement.prototype,
    'value',
  )?.set
  setter?.call(el, value)
  el.dispatchEvent(new Event('change', { bubbles: true }))
}

describe('Settings', () => {
  it('renders nothing while not visible', () => {
    const host = mount(
      <Settings
        visible={false}
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    expect(host.children.length).toBe(0)
  })

  it('renders a dialog with one section per module that has fields, skipping one that has none', () => {
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    const dialog = host.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-label')).toBe('Settings')
    // `empty`'s defaults is `{}` — `fieldsOf` reports no fields, so
    // `Settings` renders no section for it at all, not an empty one.
    const sections = host.querySelectorAll('section')
    expect(sections.length).toBe(1)
    expect(sections[0]?.querySelector('h3')?.textContent).toBe('Module A')
  })

  it('renders each field kind as the control fieldKind infers, seeded from defaults when prefs has no slice yet', () => {
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    const checkbox = host.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement
    const number = host.querySelector(
      'input[type="number"]',
    ) as HTMLInputElement
    const text = host.querySelector('input[type="text"]') as HTMLInputElement
    const select = host.querySelector('select') as HTMLSelectElement

    expect(checkbox.checked).toBe(true)
    expect(number.value).toBe('5')
    expect(text.value).toBe('hello')
    expect(select.value).toBe('x')
    expect([...select.options].map((o) => o.value)).toEqual(['x', 'y'])

    // The `'object'`-kind field: no editable control, just the reset-only
    // entry count `nestedSummary`/`nestedCount` compute.
    expect(host.textContent).toContain('2 set')
  })

  it('seeds every control from prefs when a slice already exists, not from defaults', () => {
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{
          modA: {
            enabled: false,
            count: 9,
            name: 'bye',
            choice: 'y',
            nested: {},
          },
        }}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    const checkbox = host.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement
    const number = host.querySelector(
      'input[type="number"]',
    ) as HTMLInputElement
    const text = host.querySelector('input[type="text"]') as HTMLInputElement
    const select = host.querySelector('select') as HTMLSelectElement

    expect(checkbox.checked).toBe(false)
    expect(number.value).toBe('9')
    expect(text.value).toBe('bye')
    expect(select.value).toBe('y')
    // `nestedCount({})` is 0 — `nestedSummary` words that as "none set"
    // rather than "0 set".
    expect(host.textContent).toContain('none set')
  })

  it('blanks a number, select or text control rather than showing a stale-typed value — a slice saved by an older build, say, where that key held a different type', () => {
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{
          modA: {
            enabled: true,
            count: undefined,
            name: undefined,
            choice: undefined,
            nested: {},
          },
        }}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    const number = host.querySelector(
      'input[type="number"]',
    ) as HTMLInputElement
    const text = host.querySelector('input[type="text"]') as HTMLInputElement
    const select = host.querySelector('select') as HTMLSelectElement

    expect(number.value).toBe('')
    expect(text.value).toBe('')
    // The component still passes `''` as the select's `value` (the same
    // fallback the number and text controls get) — but a native `<select>`
    // with no `<option value="">` of its own falls back to its first
    // option rather than showing a blank choice, which is why this reads
    // `'x'` and not `''`.
    expect(select.value).toBe('x')
  })

  it('reports a checkbox change as the boolean the change event carries', () => {
    const onChange = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        onChange={onChange}
        onReset={vi.fn()}
      />,
    )
    const checkbox = host.querySelector(
      'input[type="checkbox"]',
    ) as HTMLInputElement
    act(() => checkbox.click())
    expect(onChange).toHaveBeenCalledWith('modA', 'enabled', false)
  })

  it('reports a number field change as the raw string, uncoerced — coercion is withFieldValue’s job', () => {
    const onChange = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        onChange={onChange}
        onReset={vi.fn()}
      />,
    )
    const number = host.querySelector(
      'input[type="number"]',
    ) as HTMLInputElement
    act(() => fireInput(number, '42'))
    expect(onChange).toHaveBeenCalledWith('modA', 'count', '42')
  })

  it('reports a text field change as the typed string', () => {
    const onChange = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        onChange={onChange}
        onReset={vi.fn()}
      />,
    )
    const text = host.querySelector('input[type="text"]') as HTMLInputElement
    act(() => fireInput(text, 'world'))
    expect(onChange).toHaveBeenCalledWith('modA', 'name', 'world')
  })

  it('reports a select change as the chosen option’s value', () => {
    const onChange = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        onChange={onChange}
        onReset={vi.fn()}
      />,
    )
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => fireSelect(select, 'y'))
    expect(onChange).toHaveBeenCalledWith('modA', 'choice', 'y')
  })

  it('resets one module’s whole slice on its own Reset button, naming that module’s id', () => {
    const onReset = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        onChange={vi.fn()}
        onReset={onReset}
      />,
    )
    const resetButton = host.querySelector('button') as HTMLButtonElement
    expect(resetButton.textContent).toBe('Reset')
    act(() => resetButton.click())
    expect(onReset).toHaveBeenCalledWith('modA')
  })

  it('renders a note below a module’s fields when notes has an entry for its id', () => {
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        notes={{ modA: 'This module does nothing on this particular work.' }}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    const section = host.querySelector('section')
    // `<p>` is the note's own element — nothing else `Settings` renders inside
    // a section uses one, so finding it is enough to show it landed inside
    // modA's own section rather than floating outside it.
    const note = section?.querySelector('p')
    expect(note?.textContent).toBe(
      'This module does nothing on this particular work.',
    )
  })

  it('renders no note for a module notes has no entry for — unchanged from before notes existed', () => {
    const host = mount(
      <Settings
        visible
        modules={[modA]}
        prefs={{}}
        notes={{ someOtherModule: 'Not modA’s note.' }}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    )
    const section = host.querySelector('section')
    expect(section?.querySelector('p')).toBeNull()
  })
})

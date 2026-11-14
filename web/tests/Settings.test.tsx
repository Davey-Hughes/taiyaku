import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ReaderModule } from '../src/module'
import { Settings } from '../src/Settings'
import { mount } from './support/mount'

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

/**
 * jsdom defines no `elementFromPoint` at all: calling it throws
 * `document.elementFromPoint is not a function`, and `vi.spyOn` on it throws
 * `"The property 'elementFromPoint' is not defined on the object"`.
 * `Settings`'s release guard reads it on every `pointerup`, so every test
 * below that fires one needs an answer — and a single answer for the whole
 * file cannot be right, since "released over the scrim" and "released over
 * the panel" are the two things most of these tests exist to tell apart.
 * Stubbed per test, to whatever a real hit test would answer for that test's
 * own press, and restored after.
 */
function stubHitTest(at: (x: number, y: number) => Element | null): () => void {
  const before = document.elementFromPoint
  document.elementFromPoint = at as typeof document.elementFromPoint
  return () => {
    document.elementFromPoint = before
  }
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
      />,
    )
    expect(host.children.length).toBe(0)
  })

  it('closes on its own Close button, since ⚙ opens it by pointer', () => {
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
        onAccount={vi.fn()}
      />,
    )
    const close = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    )
    close?.click()
    expect(onClose).toHaveBeenCalledOnce()
  })

  // O229: a touch reader's only way into the account panel from a book. Close
  // stays at the row's end, where the account panel has its own.
  it('draws Account… in its head row before Close, and hands a press to onAccount alone', () => {
    const onAccount = vi.fn()
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
        onAccount={onAccount}
      />,
    )
    const head = host.querySelector('.reader-panel-head') as HTMLElement
    const buttons = [...head.querySelectorAll('button')]
    expect(buttons.map((b) => b.textContent)).toEqual(['Account…', 'Close'])
    const account = buttons.find((b) => b.textContent === 'Account…')
    ;(account as HTMLButtonElement).click()
    expect(onAccount).toHaveBeenCalledOnce()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('draws as the reader’s dialog, its look in the stylesheet', () => {
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={vi.fn()}
        onAccount={vi.fn()}
      />,
    )
    const dialog = host.querySelector('[role="dialog"]') as HTMLElement
    expect(dialog.className).toBe('reader-dialog')
    expect(dialog.getAttribute('style')).toBeNull()
    expect(dialog.firstElementChild?.className).toBe('reader-dialog-panel')
  })

  it('renders a dialog with one section per module that has fields, skipping one that has none', () => {
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
      />,
    )
    const resetButton = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Reset',
    ) as HTMLButtonElement
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
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

  it('closes when the press goes down and comes up on the scrim', () => {
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
        onAccount={vi.fn()}
      />,
    )
    const scrim = host.querySelector('.reader-dialog') as HTMLElement
    // Released over the scrim, same as it went down — a real hit test at
    // release would answer the scrim.
    const restore = stubHitTest(() => scrim)
    try {
      act(() => {
        scrim.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
        scrim.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }))
        scrim.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      })
      expect(onClose).toHaveBeenCalledTimes(1)
    } finally {
      restore()
    }
  })

  it('does not close when the press began inside the panel', () => {
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
        onAccount={vi.fn()}
      />,
    )
    const scrim = host.querySelector('.reader-dialog') as HTMLElement
    const panel = host.querySelector('.reader-dialog-panel') as HTMLElement
    // A drag that starts on a control and releases over the scrim: `click`'s
    // target is the scrim, and this must not be read as looking away. A real
    // hit test at release would answer the scrim too — a mouse does not
    // retarget.
    const restore = stubHitTest(() => scrim)
    try {
      act(() => {
        panel.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
        scrim.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }))
        scrim.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      })
      expect(onClose).not.toHaveBeenCalled()
    } finally {
      restore()
    }
  })

  it('does not close when the press began on the scrim and released inside the panel', () => {
    // `click`'s own target is this element either way a mixed press goes,
    // since a DOM `click` fires on the nearest common ancestor of the down and
    // up targets — so this is the one direction the two tests above cannot
    // tell apart from the correctly refused one, and the one that would close
    // the dialog under a reader who pressed the dim background and swept onto
    // the panel.
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
        onAccount={vi.fn()}
      />,
    )
    const scrim = host.querySelector('.reader-dialog') as HTMLElement
    const panel = host.querySelector('.reader-dialog-panel') as HTMLElement
    // Released over the panel: a real hit test at release would answer the
    // panel here too, the same as `pointerup`'s own `target` — a mouse does
    // not retarget the way the touch case below does.
    const restore = stubHitTest(() => panel)
    try {
      act(() => {
        scrim.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
        panel.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }))
        scrim.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      })
      expect(onClose).not.toHaveBeenCalled()
    } finally {
      restore()
    }
  })

  it('does not close when a touch release retargets to the scrim over the panel', () => {
    // This is not the mixed-direction case above — that one is a mouse's
    // stream, where a drag that starts on a control really does release with
    // `pointerup`'s own `target` on the scrim. A *touch* pointer never produces
    // that stream at all: it gets implicit pointer capture on `pointerdown`, so
    // the browser dispatches every later event for that pointer — `pointerup`
    // and `click` alike — at the capture element regardless of where the finger
    // actually lifts. For a press down on the scrim and up over the panel,
    // `pointerup`'s *target* is still the scrim; only its `clientX`/`clientY`
    // say the finger moved. Here a sweep from x=16 to x=24, less than half the
    // scrim's own ~20px margin (`docs/notes.md`, "Style pass ④") and so a
    // realistic slip, crosses onto the panel: `pointerdown`, `pointerup` and
    // `click` all target the scrim, with the release's client coordinates
    // over the panel.
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
        onAccount={vi.fn()}
      />,
    )
    const scrim = host.querySelector('.reader-dialog') as HTMLElement
    const panel = host.querySelector('.reader-dialog-panel') as HTMLElement
    // A real hit test: short of the scrim's ~20px margin is the scrim, at or
    // past it is the panel.
    const restore = stubHitTest((x) => (x < 20 ? scrim : panel))
    try {
      act(() => {
        scrim.dispatchEvent(
          new PointerEvent('pointerdown', { bubbles: true, clientX: 16 }),
        )
        // Capture retargets this to the scrim even though the finger is now
        // over the panel — the stream a touch browser actually produces, not
        // a mouse's.
        scrim.dispatchEvent(
          new PointerEvent('pointerup', { bubbles: true, clientX: 24 }),
        )
        scrim.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      })
      expect(onClose).not.toHaveBeenCalled()
    } finally {
      restore()
    }
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
        onClose={vi.fn()}
        onAccount={vi.fn()}
      />,
    )
    const section = host.querySelector('section')
    expect(section?.querySelector('p')).toBeNull()
  })
})

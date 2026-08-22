import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { browseModule } from './browseModule'
import { BrowseSettings } from './BrowseSettings'
import { mount } from './testing/mount'

describe('BrowseSettings', () => {
  it('renders one control per preference, generated rather than named', () => {
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={() => {}}
        onReset={() => {}}
      />,
    )
    expect(host.querySelectorAll('label')).toHaveLength(6)
    expect([...host.querySelectorAll('select')]).toHaveLength(3)
  })

  it('reports a changed choice as the value, not the event', () => {
    const onChange = vi.fn()
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={onChange}
        onReset={() => {}}
      />,
    )
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'table'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(onChange).toHaveBeenCalledWith('layout', 'table')
  })

  it('offers a reset', () => {
    const onReset = vi.fn()
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={() => {}}
        onReset={onReset}
      />,
    )
    act(() => {
      ;(host.querySelector('button') as HTMLButtonElement).click()
    })
    expect(onReset).toHaveBeenCalledTimes(1)
  })

  /**
   * **What the count-six test above does not pin.** Spec C1 §4's central
   * claim is that a `browse` module needs no settings UI of its own, because
   * `settingsPanel.ts`'s `fieldKind` derives each control from a
   * preference's runtime type. The test above asserts `BrowseSettings`
   * renders exactly six controls — but a hand-written, non-generated
   * component with six literal controls satisfies that assertion exactly as
   * well as the generated one does. Counting six of anything is not evidence
   * that they were generated.
   *
   * What is evidence: adding a seventh key to `browseModule.defaults` and
   * watching a seventh control appear with **no edit to `BrowseSettings.tsx`
   * at all**. Only a component that actually reads `defaults` at render time
   * — rather than one written against today's six keys by hand — can produce
   * that seventh control.
   *
   * `vi.doMock` + a fresh dynamic import, rather than the file-level
   * `vi.mock` the tests above would then have to share: only *this* test's
   * copy of `browseModule` should gain the extra key. `vi.mock` is hoisted
   * and evaluated once for the whole file, which would leave no way to give
   * the tests above their real, six-key module and this one an augmented
   * seven-key module side by side. `vi.resetModules()` clears the registry
   * so the dynamic imports below re-evaluate against the mock instead of
   * reusing the six-key instance the top-level `import` statements already
   * cached before any test ran.
   */
  it('adds a control when a preference is added, with no edit to BrowseSettings.tsx', async () => {
    vi.resetModules()
    vi.doMock('./browseModule', async (importOriginal) => {
      const actual = await importOriginal<typeof import('./browseModule')>()
      return {
        ...actual,
        browseModule: {
          ...actual.browseModule,
          // `number`, so `fieldKind` gives it a numeric input — one of the
          // kinds `fieldKind` handles, same as `settingsPanel.test.ts` pins.
          defaults: { ...actual.browseModule.defaults, pageSize: 24 },
        },
      }
    })

    try {
      const { BrowseSettings: SevenKeyBrowseSettings } = await import(
        './BrowseSettings'
      )
      const { browseModule: sevenKeyModule } = await import('./browseModule')

      const host = mount(
        <SevenKeyBrowseSettings
          prefs={sevenKeyModule.defaults}
          onChange={() => {}}
          onReset={() => {}}
        />,
      )
      const labels = [...host.querySelectorAll('label')]
      expect(labels).toHaveLength(7)
      // The seventh control is a numeric input — `pageSize`'s own type,
      // inferred rather than declared.
      expect(host.querySelectorAll('input[type="number"]')).toHaveLength(1)
      expect(labels.some((l) => l.textContent?.trim() === 'page size')).toBe(
        true,
      )
    } finally {
      vi.doUnmock('./browseModule')
      vi.resetModules()
    }
  })
})

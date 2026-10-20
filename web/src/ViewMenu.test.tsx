import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import {
  type BrowseControls,
  BrowseControlsContext,
  type BrowsePrefs,
  browseModule,
} from './browseModule'
import { mount } from './testing/mount'
import { ViewMenu } from './ViewMenu'

/** Every browse preference, in the module's own order. */
const EVERY = Object.keys(browseModule.defaults) as (keyof BrowsePrefs)[]

/**
 * A View button over `keys`, with a shell's controls stood in for: the
 * button draws nothing where no shell sits above it.
 */
function at(
  keys: readonly (keyof BrowsePrefs)[],
  controls: BrowseControls = { change: () => {}, reset: () => {} },
) {
  return mount(
    <MemoryRouter>
      <BrowseControlsContext.Provider value={controls}>
        <ViewMenu keys={keys} />
      </BrowseControlsContext.Provider>
    </MemoryRouter>,
  )
}

function viewButton(host: HTMLElement): HTMLButtonElement {
  const button = host.querySelector<HTMLButtonElement>('button.view-button')
  if (!button) throw new Error('no View button')
  return button
}

/** The panel View controls, found by the id its button names. */
function panelOf(host: HTMLElement): HTMLElement {
  const id = viewButton(host).getAttribute('aria-controls')
  const panel = id ? document.getElementById(id) : null
  if (!panel || !host.contains(panel)) throw new Error('View controls nothing')
  return panel
}

/** Opens View and hands back its panel, failing if it did not open. */
function openView(host: HTMLElement): HTMLElement {
  const panel = panelOf(host)
  expect(panel.hidden).toBe(true)
  act(() => {
    viewButton(host).click()
  })
  expect(viewButton(host).getAttribute('aria-expanded')).toBe('true')
  expect(panel.hidden).toBe(false)
  return panel
}

describe('ViewMenu', () => {
  it('renders one control per preference it is given, generated rather than named', () => {
    const panel = openView(at(EVERY))
    expect(panel.querySelectorAll('label')).toHaveLength(5)
    expect([...panel.querySelectorAll('select')]).toHaveLength(3)
  })

  it('reports a changed choice as the value, not the event', () => {
    const change = vi.fn()
    const panel = openView(at(EVERY, { change, reset: () => {} }))
    const select = panel.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'table'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(change).toHaveBeenCalledWith('layout', 'table')
  })

  it('offers a reset', () => {
    const reset = vi.fn()
    const panel = openView(at(EVERY, { change: () => {}, reset }))
    act(() => {
      ;(panel.querySelector('button') as HTMLButtonElement).click()
    })
    expect(reset).toHaveBeenCalledTimes(1)
  })

  /**
   * **What the count test above does not pin.** Spec C1 §4's central claim
   * is that a `browse` module needs no settings UI of its own, because
   * `settingsPanel.ts`'s `fieldKind` derives each control from a
   * preference's runtime type. A hand-written component with five literal
   * controls satisfies the count exactly as well as the generated one does.
   *
   * What is evidence: adding a key to `browseModule.defaults` and watching
   * one more control appear with **no edit to `ViewMenu.tsx` at all**. Only
   * a component that reads `defaults` at render time can produce it.
   *
   * `vi.doMock` + a fresh dynamic import, rather than a file-level
   * `vi.mock`: only *this* test's copy of `browseModule` should gain the
   * key. `vi.mock` is hoisted and evaluated once for the whole file.
   * `vi.resetModules()` clears the registry so the dynamic imports below
   * re-evaluate against the mock instead of reusing the instance the
   * top-level imports cached.
   */
  it('adds a control when a preference is added, with no edit to ViewMenu.tsx', async () => {
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
      const { ViewMenu: AugmentedViewMenu } = await import('./ViewMenu')
      // The context object the augmented menu reads is the fresh module's,
      // not the one imported at the top of this file.
      const fresh = await import('./browseModule')
      const host = mount(
        <MemoryRouter>
          <fresh.BrowseControlsContext.Provider
            value={{ change: () => {}, reset: () => {} }}
          >
            <AugmentedViewMenu
              keys={[...EVERY, 'pageSize' as keyof BrowsePrefs]}
            />
          </fresh.BrowseControlsContext.Provider>
        </MemoryRouter>,
      )
      const panel = openView(host)
      const labels = [...panel.querySelectorAll('label')]
      expect(labels).toHaveLength(6)
      // The added control is a numeric input — `pageSize`'s own type,
      // inferred rather than declared.
      expect(panel.querySelectorAll('input[type="number"]')).toHaveLength(1)
      expect(labels.some((l) => l.textContent?.trim() === 'Page size')).toBe(
        true,
      )
    } finally {
      vi.doUnmock('./browseModule')
      vi.resetModules()
    }
  })

  it('draws its rows from the stylesheet, with no inline style', () => {
    const panel = openView(at(EVERY))
    const styled = [...panel.querySelectorAll('label')].filter((label) =>
      label.hasAttribute('style'),
    )
    expect(styled).toEqual([])
  })

  it('is a disclosure: a button naming the panel that follows it, and a panel that is no dialog', () => {
    // Spec §2: the panel follows §1's disclosure rules. It holds form
    // controls and no popup is owed, so `aria-haspopup` would announce one
    // that never comes, and a `dialog` would promise focus moving into it.
    const host = at(EVERY)
    const button = viewButton(host)
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(button.hasAttribute('aria-haspopup')).toBe(false)
    const panel = openView(host)
    expect(panel.hasAttribute('role')).toBe(false)
    expect(panel.hasAttribute('aria-label')).toBe(false)
    // After the button in the document, so Tab from the button enters it.
    expect(
      button.compareDocumentPosition(panel) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()
  })
})

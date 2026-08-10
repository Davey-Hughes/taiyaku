import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach } from 'vitest'

// One root and one host per test, torn down after each. Extracted from
// `PageView.browser.test.tsx`, which had this inline, so the `jsdom` and
// `browser` projects mount identically. Deliberately not
// `@testing-library/react`: this repository's rule is that a dev-only
// dependency needs a stated reason, and a second mounting API earns nothing
// over the lines already working here.
let root: Root | undefined
let host: HTMLElement | undefined

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
})

/** Mount a component and hand back its host element. */
export function mount(ui: React.ReactNode): HTMLElement {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  act(() => root?.render(ui))
  return host
}

/**
 * Render new UI onto the current mount's own root, rather than creating a
 * fresh one — for a test that needs the *same* component instance to see a
 * prop change, the way a real page turn changes `PageView`'s `src` under an
 * instance React keeps mounted across it. A second `mount()` call would
 * remount a fresh instance instead, which is a different thing to test.
 *
 * Throws rather than no-opping when nothing is mounted. The optional-chained
 * form this replaced would silently do nothing, and a test whose `rerender`
 * did nothing still passes every assertion about what was on screen *before*
 * it — so the failure mode is a green test that checked nothing.
 */
export function rerender(ui: React.ReactNode): void {
  if (!root) {
    throw new Error('rerender() called before mount() — nothing is mounted')
  }
  act(() => root?.render(ui))
}

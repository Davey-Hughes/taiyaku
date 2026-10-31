import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach } from 'vitest'

// A root and a host per mount, every one torn down after each test. Shared,
// so the `jsdom` and `browser` projects mount identically. Deliberately not
// `@testing-library/react`: this repository's rule is that a dev-only
// dependency needs a stated reason, and a second mounting API earns nothing
// over the lines already working here.
//
// Every mount, not only the last: a test that mounts twice would otherwise
// leave the first drawn after it ends, still subscribed to the read store
// (`reads.ts`), whose emptying after the test (`browser-setup.ts`) would then
// draw it again outside `act`.
let mounted: { root: Root; host: HTMLElement }[] = []
/** The newest mount's root, which `rerender` draws onto. */
let root: Root | undefined

afterEach(() => {
  act(() => {
    for (const one of mounted) one.root.unmount()
  })
  for (const one of mounted) one.host.remove()
  mounted = []
  root = undefined
})

/** Mount a component and hand back its host element. */
export function mount(ui: React.ReactNode): HTMLElement {
  const host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  mounted.push({ root, host })
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
 * Throws rather than no-opping when nothing is mounted. An optional-chained
 * call would silently do nothing, and a test whose `rerender` did nothing
 * still passes every assertion about what was on screen *before* it — so the
 * failure mode is a green test that checked nothing.
 */
export function rerender(ui: React.ReactNode): void {
  if (!root) {
    throw new Error('rerender() called before mount() — nothing is mounted')
  }
  act(() => root?.render(ui))
}

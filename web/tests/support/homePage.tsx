// Shared by the browser tests that draw the home page inside the shell. Each
// file keeps its own `vi.mock` of `src/library`, since a mock is hoisted per
// file, answers its reads and waits for its own tiles.
import { act, Profiler, type ReactNode } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { onTestFinished } from 'vitest'
import { page } from 'vitest/browser'
import type { Me } from '../../src/api'
import { browseModule } from '../../src/browseModule'
import { GateContext } from '../../src/gate'
import { Library } from '../../src/Library'
import { NAMESPACE } from '../../src/prefs'
import { Shell } from '../../src/Shell'
import { mount } from './mount'

/** Where the shell keeps the browse preferences, the cover size among them. */
export const BROWSE_KEY = `${NAMESPACE}.${browseModule.id}`

/** The browse preferences as the shell last stored them. */
export function storedBrowse(): Record<string, unknown> {
  return JSON.parse(localStorage.getItem(BROWSE_KEY) ?? '{}')
}

/**
 * The home page at `entry` (`/` unless given) inside the shell, in a `width`
 * by `height` window (844 unless given) until the test ends. `coverSize` and
 * the other browse preferences in `browse`, where given, are stored in the
 * real `localStorage` before the shell reads them; the shell stores its
 * preferences as it mounts and on every change, so the entry is removed
 * when the test ends either way. `onCommit`, where given, is called for
 * each commit inside `Library`, from a `Profiler` around it; `who` is the
 * account signed in, where given.
 */
export async function homePage({
  width,
  height = 844,
  coverSize,
  browse = {},
  entry = '/',
  onCommit,
  who,
  routes,
}: {
  width: number
  height?: number
  coverSize?: number
  browse?: Record<string, unknown>
  entry?: string
  onCommit?: () => void
  who?: Me
  /** Routes beside the home page's, such as the reader's. */
  routes?: ReactNode
}): Promise<HTMLElement> {
  onTestFinished(() => localStorage.removeItem(BROWSE_KEY))
  const stored = coverSize === undefined ? browse : { ...browse, coverSize }
  if (Object.keys(stored).length === 0) localStorage.removeItem(BROWSE_KEY)
  else localStorage.setItem(BROWSE_KEY, JSON.stringify(stored))
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, height)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <GateContext
        value={who === undefined ? undefined : { who, signOut: () => {} }}
      >
        <MemoryRouter initialEntries={[entry]}>
          <Routes>
            <Route
              path="/"
              element={
                <Shell>
                  {onCommit === undefined ? (
                    <Library />
                  ) : (
                    <Profiler id="library" onRender={onCommit}>
                      <Library />
                    </Profiler>
                  )}
                </Shell>
              }
            />
            {routes}
          </Routes>
        </MemoryRouter>
      </GateContext>,
    )
    // The height chain `base.css` gives `#root`, which `mount()`'s host is
    // not: without it `.library`'s `height: 100%` acts as `auto`, and the
    // desktop layout would grow with its content as it never does in the app.
    host.style.height = '100%'
    await Promise.resolve()
    await Promise.resolve()
  })
  // The search bar debounces its request by 250 ms (`searchHits.ts`); wait
  // it out with real timers.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
  })
  if (host === undefined) throw new Error('nothing mounted')
  return host
}

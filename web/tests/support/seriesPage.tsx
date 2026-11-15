// Shared by the browser tests that draw the Series page's grid inside the
// shell's page area. Each file keeps its own `vi.mock`s of `src/library`
// and waits for its own tiles.
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { onTestFinished } from 'vitest'
import { page } from 'vitest/browser'
import type { Me } from '../../src/api'
import {
  BrowseControlsContext,
  type BrowsePrefs,
  BrowsePrefsContext,
  browseModule,
} from '../../src/browseModule'
import { GateContext } from '../../src/gate'
import { Series } from '../../src/Series'
import { mount } from './mount'
import { mouse } from './mouse'

/**
 * The Series page at `/series/7` for `who`, in `.library > main` as the
 * shell draws it, in a `width` by `height` window (800 unless given) until
 * the test ends; `compact` is the shell's `density` class, and `panel` and
 * `layout` are laid over the default preferences. With View's controls,
 * doing nothing, since `ViewMenu` draws nothing without them, and the
 * sidebar's empty column, which a desktop `.library` grid puts first.
 * Answers the host once mounted: the series' read is the caller's mock, and
 * so is waiting for it.
 */
export async function seriesPage({
  width,
  height = 800,
  panel,
  layout = browseModule.defaults.layout,
  compact = false,
  who,
}: {
  width: number
  height?: number
  panel: BrowsePrefs['panel']
  layout?: BrowsePrefs['layout']
  compact?: boolean
  who: Me
}): Promise<HTMLElement> {
  // Off every tile before the page is drawn: a tile drawn under a resting
  // pointer is entered, and in `on-select` mode that opens its panel.
  await mouse.moveTo(0, 0)
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, height)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <BrowsePrefsContext.Provider
        value={{ ...browseModule.defaults, panel, layout }}
      >
        <BrowseControlsContext.Provider
          value={{ change: () => {}, reset: () => {} }}
        >
          <GateContext.Provider value={{ who, signOut: () => {} }}>
            <div className={compact ? 'library compact' : 'library'}>
              <div className="sidebar-column" />
              <main>
                <MemoryRouter initialEntries={['/series/7']}>
                  <Routes>
                    <Route path="/series/:id" element={<Series />} />
                  </Routes>
                </MemoryRouter>
              </main>
            </div>
          </GateContext.Provider>
        </BrowseControlsContext.Provider>
      </BrowsePrefsContext.Provider>,
    )
  })
  if (host === undefined) throw new Error('nothing mounted')
  return host
}

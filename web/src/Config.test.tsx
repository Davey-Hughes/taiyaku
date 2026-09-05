import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { Config } from './Config'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
}))

async function at(entry: string) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <Routes>
          <Route path="/config/*" element={<Config />} />
        </Routes>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
})

describe('Config', () => {
  it('draws one sub-nav link per tab', async () => {
    const host = await at('/config/roots')
    expect(host.querySelectorAll('.config-nav a')).toHaveLength(
      config.TABS.length,
    )
  })

  it('sends a bare /config to the leading tab', async () => {
    // The index redirect reads `TABS[0]`, so it cannot drift from the sub-nav
    // — one list, read twice. The build design's §2.
    const host = await at('/config')
    expect(host.textContent).toContain('Roots')
    expect(host.querySelector('.roots')).not.toBeNull()
  })

  it('marks the current tab', async () => {
    const host = await at('/config/roots')
    expect(
      host.querySelector('.config-nav a[aria-current="page"]'),
    ).not.toBeNull()
  })

  it('gives every tab a screen to draw', async () => {
    // A tab in `TABS` with no entry in `SCREENS` renders an empty route: a
    // link that navigates to nothing. Asserted rather than trusted, because
    // the two lists are what PR B and PR C each extend.
    for (const tab of config.TABS) {
      const host = await at(`/config/${tab.path}`)
      expect(host.querySelector('.config > :not(.config-nav)')).not.toBeNull()
    }
  })

  it('redirects a sub-path that names no tab', async () => {
    // **Review finding 10.** `Routes.tsx` matches `/config/*` first, so its
    // own catch-all never fires for a bad sub-path — `/config/rules` today
    // drew the sub-nav over a blank pane with nothing said. PR B adding that
    // tab is exactly when somebody types it.
    const host = await at('/config/nonsense')
    expect(host.querySelector('.roots')).not.toBeNull()
  })
})

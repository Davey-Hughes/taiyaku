import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { Config } from './Config'
import * as backupApi from './backup'
import * as curation from './curation'
import * as librariesApi from './libraries'
import * as rulesApi from './rules'
import * as usersApi from './users'
import { mount } from './testing/mount'

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  listLibraries: vi.fn(),
}))
vi.mock('./rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
}))
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  listUsers: vi.fn(),
}))
vi.mock('./backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./backup')>()),
  listBackups: vi.fn(),
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
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(rulesApi.listRules).mockResolvedValue({ rules: [], complaints: [] })
  vi.mocked(rulesApi.previewRules).mockResolvedValue({
    impact: {
      series_before: 0,
      series_after: 0,
      series_merged: 0,
      works_rehomed: 0,
      works_orphaned: 0,
      proposals_reopened: 0,
      proposals_raised: 0,
    },
    rows: [],
    rows_truncated: false,
    overrides: [],
  })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

describe('Config', () => {
  it('names its tab after the tab a bare /config lands on — O173', async () => {
    document.title = 'stale'
    await at('/config')
    expect(document.title).toBe(`${config.TABS[0].label} — taiyaku`)
  })

  it('draws one sub-nav link per tab', async () => {
    const host = await at('/config/libraries')
    expect(host.querySelectorAll('.config-nav a')).toHaveLength(
      config.TABS.length,
    )
  })

  it('sends a bare /config to the leading tab', async () => {
    // The index redirect reads `TABS[0]`, so it cannot drift from the sub-nav
    // — one list, read twice. The build design's §2. PR B prepends the rule
    // editor, which is what makes it the landing screen.
    const host = await at('/config')
    expect(host.textContent).toContain('Rules')
    expect(host.querySelector('.rule-editor')).not.toBeNull()
  })

  it('marks the current tab', async () => {
    const host = await at('/config/libraries')
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
      // `:not(h1)`: the hidden heading is not a screen (O163).
      expect(
        host.querySelector('.config > :not(.config-nav):not(h1)'),
      ).not.toBeNull()
    }
  })

  it('sends the old roots tab to the libraries screen', async () => {
    const host = await at('/config/roots')
    expect(host.querySelector('.libraries')).not.toBeNull()
    expect(document.title).toBe('Libraries — taiyaku')
  })

  it('redirects a sub-path that names no tab', async () => {
    // **Review finding 10.** `Routes.tsx` matches `/config/*` first, so its
    // own catch-all never fires for a bad sub-path — `/config/rules` today
    // drew the sub-nav over a blank pane with nothing said. PR B adding that
    // tab is exactly when somebody types it.
    const host = await at('/config/nonsense')
    expect(host.querySelector('.rule-editor')).not.toBeNull()
  })

  it('opens every tab with one h1, Configuration, above the nav — O163', async () => {
    for (const tab of config.TABS) {
      const host = await at(`/config/${tab.path}`)
      const h1s = [...host.querySelectorAll('h1')]
      expect(
        h1s.map((h) => [h.textContent, h.className]),
        tab.path,
      ).toEqual([['Configuration', 'visually-hidden']])
      expect(host.querySelector('.config')?.firstElementChild, tab.path).toBe(
        h1s[0],
      )
    }
  })

  it('names its tab after the configuration tab it shows — O173', async () => {
    for (const tab of config.TABS) {
      document.title = 'stale'
      await at(`/config/${tab.path}`)
      expect(document.title, tab.path).toBe(`${tab.label} — taiyaku`)
    }
  })
})

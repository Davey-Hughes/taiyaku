import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from '../src/config'
import { Config } from '../src/Config'
import * as backupApi from '../src/backup'
import * as curation from '../src/curation'
import * as librariesApi from '../src/libraries'
import * as rulesApi from '../src/rules'
import * as splitApi from '../src/splitEditions'
import * as usersApi from '../src/users'
import { deferred, flush } from './support/async'
import { press, submit, typeInto } from './support/input'
import { mount } from './support/mount'

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  createLibrary: vi.fn(),
  listLibraries: vi.fn(),
}))
vi.mock('../src/rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))
vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
}))
vi.mock('../src/splitEditions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/splitEditions')>()),
  listSplitEditions: vi.fn(),
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
  vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: [] })
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

  it('sends a bare /config to the leading tab, Libraries — O219 §1.4', async () => {
    // The index redirect reads `TABS[0]`, so it cannot drift from the sub-nav
    // — one list, read twice.
    const host = await at('/config')
    expect(host.querySelector('.library-table')).not.toBeNull()
    expect(
      host.querySelector('.config-nav a[aria-current="page"]')?.textContent,
    ).toBe('Libraries')
  })

  it('draws the tabs in the order Libraries, Users, Rules, Backup — O219 §1.4 — then Split editions', async () => {
    const host = await at('/config/libraries')
    expect(
      [...host.querySelectorAll('.config-nav a')].map((a) => [
        a.textContent,
        a.getAttribute('href'),
      ]),
    ).toEqual([
      ['Libraries', '/config/libraries'],
      ['Users', '/config/users'],
      ['Rules', '/config/rules'],
      ['Backup', '/config/backup'],
      ['Split editions', '/config/split-editions'],
    ])
  })

  it('marks the current tab', async () => {
    const host = await at('/config/libraries')
    expect(
      host.querySelector('.config-nav a[aria-current="page"]'),
    ).not.toBeNull()
  })

  it('gives every tab a screen to draw', async () => {
    // A tab in `TABS` with no entry in `SCREENS` renders an empty route: a
    // link that navigates to nothing. Asserted rather than trusted.
    for (const tab of config.TABS) {
      const host = await at(`/config/${tab.path}`)
      // `:not(h1)`: the hidden heading is not a screen (O163).
      expect(
        host.querySelector('.config > :not(.config-nav):not(h1)'),
      ).not.toBeNull()
    }
  })

  it.each([
    ['/config/new-account', '.new-account form.add-user'],
    ['/config/users/davey', '.account-page .page-title'],
  ])('draws the users tab’s page at %s', async (entry, drawn) => {
    // Only a tab with `pages` is routed with the paths beneath its own, and
    // only a tab with `make` with its make page outside them; any other path
    // goes back to the leading tab.
    document.title = 'stale'
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        {
          id: 1,
          name: 'davey',
          role: 'admin',
          can_download: true,
          can_stream: true,
          can_stamp: false,
          can_log_in: true,
          sessions: [],
          keys: [],
          every_library: true,
          libraries: [],
        },
      ],
    })
    const host = await at(entry)
    expect(host.querySelector(`.users ${drawn}`)).not.toBeNull()
    expect(
      host.querySelector('.config-nav a[aria-current="page"]')?.textContent,
    ).toBe('Users')
    expect(document.title).toBe('Users — taiyaku')
  })

  it('draws the make-a-library page at /config/new-library, under the Libraries tab', async () => {
    document.title = 'stale'
    const host = await at('/config/new-library')
    expect(
      host.querySelector('.libraries .new-library form.add-library'),
    ).not.toBeNull()
    expect(
      host.querySelector('.config-nav a[aria-current="page"]')?.textContent,
    ).toBe('Libraries')
    expect(document.title).toBe('Libraries — taiyaku')
  })

  it('keeps the libraries screen between the make page and the table, so a make refused after the person left is said on the table', async () => {
    const made = deferred<never>()
    vi.mocked(librariesApi.createLibrary).mockReturnValue(made.promise)
    const host = await at('/config/new-library')
    const form = host.querySelector('form.add-library') as HTMLFormElement
    await typeInto(
      form.querySelector('input[name="library-name"]') as HTMLInputElement,
      'Comics',
    )
    await typeInto(
      form.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Comics',
    )
    await submit(form)
    await press(host.querySelector('.new-library .back a') as Element)
    await flush()
    expect(host.querySelector('.library-table')).not.toBeNull()
    await flush(() =>
      made.reject(
        new Error(
          '/api/library/libraries: 500 {"error":"the store is locked"}',
        ),
      ),
    )
    expect(
      [...host.querySelectorAll('.library-table [role="alert"]')].map(
        (alert) => alert.textContent,
      ),
    ).toEqual(['The library Comics was not made (the store is locked).'])
  })

  it('sends the old roots tab to the libraries screen', async () => {
    const host = await at('/config/roots')
    expect(host.querySelector('.libraries .library-table')).not.toBeNull()
    expect(document.title).toBe('Libraries — taiyaku')
  })

  it('redirects a sub-path that names no tab', async () => {
    // `Routes.tsx` matches `/config/*` first, so its own catch-all never fires
    // for a bad sub-path, which would draw the sub-nav over a blank pane with
    // nothing said.
    const host = await at('/config/nonsense')
    expect(host.querySelector('.library-table')).not.toBeNull()
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

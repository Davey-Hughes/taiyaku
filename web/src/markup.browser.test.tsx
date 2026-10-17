import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import { Account } from './Account'
import * as config from './config'
import * as curation from './curation'
import './index.css'
import { Login } from './Login'
import { Proposals } from './Proposals'
import { RootRow } from './RootRow'
import { RuleList } from './RuleList'
import type { Rule } from './rules'
import { Sidebar } from './Sidebar'
import { mount } from './testing/mount'
import { UserRow } from './UserRow'
import type * as usersApi from './users'

/**
 * The accessibility pass's §4, as a browser computes it. jsdom here has no
 * name computation (`testing/mount.ts` says why there is no testing-library),
 * so the jsdom tests beside each component pin the markup a name is computed
 * from, and this file pins the names themselves — with
 * `toHaveAccessibleName`, `toHaveAccessibleDescription` and
 * `getByRole(…, { name })`.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  scanStatus: vi.fn(),
}))

vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))

const root = (over: Partial<config.Root> = {}): config.Root => ({
  id: 3,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  ...over,
})

const user = (over: Partial<usersApi.User> = {}): usersApi.User => ({
  id: 1,
  name: 'davey',
  role: 'admin',
  can_download: true,
  can_stream: true,
  can_stamp: false,
  can_log_in: true,
  sessions: [],
  keys: [],
  libraries: [],
  ...over,
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('names — §4.3 and §4.4', () => {
  it('names the Proposals link with what its count counts — O164', async () => {
    mount(
      <MemoryRouter>
        <Sidebar libraries={[]} admin={true} openProposals={17} />
      </MemoryRouter>,
    )
    await expect
      .element(
        page.getByRole('link', { name: 'Proposals, 17 open', exact: true }),
      )
      .toBeInTheDocument()
  })

  it('names Exclude globs by its label and describes it by its hint — O165', async () => {
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    await act(async () => {
      mount(<RootRow root={root()} probe={undefined} onChanged={() => {}} />)
      await Promise.resolve()
    })
    await expect
      .element(
        page.getByRole('textbox', {
          name: 'Exclude globs (one gitignore pattern per line)',
          exact: true,
        }),
      )
      .toHaveAccessibleDescription(
        'A trailing slash or bare name excludes the directory and everything ' +
          "under it; a '/**' suffix excludes only what's inside it.",
      )
  })

  it('groups the three permission boxes under one name — O154', async () => {
    await act(async () => {
      mount(
        <MemoryRouter>
          <UserRow
            user={user({ libraries: [{ id: 70, name: 'Manga' }] })}
            users={[user()]}
            libraries={[
              {
                id: 70,
                name: 'Manga',
                works: 0,
                books: 0,
                folders: [root()],
              },
            ]}
            onChanged={() => {}}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
    })
    // The grant box is drawn, so a group that swallowed it would show here.
    expect(
      page.getByRole('checkbox', { name: 'Manga' }).elements(),
    ).toHaveLength(1)
    const boxes = page
      .getByRole('group', { name: 'Permissions', exact: true })
      .getByRole('checkbox')
    expect(boxes.elements().map((box) => box.getAttribute('name'))).toEqual([
      'can_download',
      'can_stream',
      'can_stamp',
    ])
  })
})

describe('headings — §4.2', () => {
  it('names the inbox’s h1 with the screen it heads — O163', async () => {
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: { 'edition-of': 16, 'attach-chapters': 1 },
    })
    await act(async () => {
      mount(<Proposals />)
      await Promise.resolve()
      await Promise.resolve()
    })
    await expect
      .element(page.getByRole('heading', { level: 1 }))
      .toHaveAccessibleName('Proposals, 17 open')
  })

  /** A sibling `h2` in the same place: the size `base.css` gives one there. */
  const sizeOfAnH2Beside = (heading: Element): string => {
    const probe = document.createElement('h2')
    heading.parentElement?.append(probe)
    const size = getComputedStyle(probe).fontSize
    probe.remove()
    return size
  }

  it('keeps the sign-in heading at the h2 size it was drawn at — O163', () => {
    const host = mount(<Login onSignedIn={() => {}} />)
    const h1 = host.querySelector('h1')
    if (!h1) throw new Error('no h1')
    expect(getComputedStyle(h1).fontSize).toBe(sizeOfAnH2Beside(h1))
  })

  it('keeps the account page’s title at the h2 size it was drawn at — O163', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() =>
        Promise.resolve(
          new Response('[]', {
            status: 200,
            headers: { 'content-type': 'application/json' },
          }),
        ),
      ),
    )
    let host: HTMLElement | undefined
    await act(async () => {
      // Inside `.library`, where `base.css` sizes headings — as `Shell` puts it.
      host = mount(
        <div className="library">
          <Account placement="page" onSignedOut={() => {}} />
        </div>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    const h1 = host?.querySelector('.account > h1')
    if (!h1) throw new Error('no h1')
    expect(getComputedStyle(h1).fontSize).toBe(sizeOfAnH2Beside(h1))
  })
})

describe('the selected rule — §4.1', () => {
  it('draws the selected row’s mark from the button’s aria-current — O156', () => {
    const rule = (over: Partial<Rule> = {}): Rule => ({
      id: 1,
      ordinal: 1,
      enabled: true,
      root_id: null,
      kind: 'regex',
      pattern: 'Manga/.*',
      match_on: 'path',
      set_series: null,
      set_edition_key: 'raw',
      set_kind: null,
      set_language: null,
      note: null,
      ...over,
    })
    const host = mount(
      <RuleList
        rules={[rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })]}
        selected={1}
        proposals={[]}
        overrideCount={0}
        frozen={false}
        onSelect={() => {}}
        onToggle={() => {}}
        onReorder={() => {}}
        onAdd={() => {}}
        onDelete={() => {}}
      />,
    )
    const shadows = [...host.querySelectorAll('.rule-row')].map(
      (row) => getComputedStyle(row).boxShadow,
    )
    expect(shadows).toHaveLength(2)
    expect(shadows[0]).toBe('none')
    expect(shadows[1]).not.toBe('none')
  })
})

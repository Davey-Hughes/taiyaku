import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as librariesApi from '../src/libraries'
import { deferred } from './support/async'
import { click, linkIn, openedAt } from './support/typedAddress'
import * as usersApi from '../src/users'

/**
 * An account's address is its name, escaped where an address needs it: held
 * here through the router the app runs, `BrowserRouter`, at the frame's real
 * address, as `Libraries.address.browser.test.tsx` holds a library's.
 */

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  listLibraries: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))

/**
 * One name for each thing an address has to escape, with the path the
 * browser holds for its page, written out rather than computed so that a
 * wrong escape cannot agree with itself. `2` is a name, and also the id of
 * `sam/kim`, which an address by id would open instead. `Kim` and `kim` are
 * two accounts, as names are unique only as written.
 */
const NAMES: [name: string, segment: string][] = [
  ['Sam Jones', 'Sam%20Jones'],
  ['sam/kim', 'sam%2Fkim'],
  ['Who?', 'Who%3F'],
  ['No. #1', 'No.%20%231'],
  ['100%', '100%25'],
  ['Text holding %2F', 'Text%20holding%20%252F'],
  ['Tom & Jerry', 'Tom%20%26%20Jerry'],
  ['Émile', '%C3%89mile'],
  ['2', '2'],
  ['Kim', 'Kim'],
  ['kim', 'kim'],
]

/** Each name's account, numbered from 1 in that order. */
const USERS: usersApi.User[] = NAMES.map(([name], i) => ({
  id: i + 1,
  name,
  role: 'user',
  can_download: true,
  can_stream: true,
  can_stamp: false,
  can_log_in: true,
  wrong_passwords: 0,
  wrong_password_alarm: false,
  sessions: [],
  keys: [],
  every_library: true,
  libraries: [],
}))

/** Signed in as none of them, so no title carries "this is you". */
const who = {
  user_id: 100,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: USERS })
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
})

const table = (host: HTMLElement) =>
  host.querySelector('.split-list .user-table') as Element

/** The names of the table's rows marked as the open one. */
const marked = (host: HTMLElement) =>
  [...host.querySelectorAll('.user-table .row-link[aria-current="page"]')].map(
    (a) => a.textContent,
  )

/** The Configuration tab marked as the current one. */
const currentTab = (host: HTMLElement) =>
  host.querySelector('.config-nav a[aria-current="page"]')?.textContent

describe('an account’s address, through the app’s own router', () => {
  it.each(NAMES)(
    'opens %s’s page beside the table from its row, marks that row alone, and goes back to it',
    async (name, segment) => {
      const host = await openedAt('/config/users', who, 1280)

      await click(linkIn(table(host), name))
      expect(location.pathname).toBe(`/config/users/${segment}`)
      expect(
        host.querySelector('.split-detail .account-page .page-title')
          ?.textContent,
      ).toBe(name)
      expect(marked(host)).toEqual([name])

      await click(
        linkIn(host.querySelector('.account-page .back') as Element, '‹ Users'),
      )
      expect(location.pathname).toBe('/config/users')
      expect(host.querySelector('.account-page')).toBeNull()
      expect(document.activeElement).toBe(linkIn(table(host), name))
    },
  )

  it.each(NAMES)(
    'answers the address of %s’s page alone at a phone’s width, as a reload or a typed address reaches it',
    async (name, segment) => {
      const host = await openedAt(`/config/users/${segment}`, who, 390)
      expect(location.pathname).toBe(`/config/users/${segment}`)
      expect(host.querySelector('.account-page .page-title')?.textContent).toBe(
        name,
      )
      expect(host.querySelector('.user-table')).toBeNull()
      expect(
        linkIn(
          host.querySelector('.account-page .back') as Element,
          '‹ Users',
        ).getAttribute('href'),
      ).toBe('/config/users')
    },
  )

  // The row's link spells the address one way; a typed address may spell
  // the same page another, in another case or with a character the link
  // escapes left bare, and is kept in the bar as typed.
  it.each([
    ['Sam Jones', 'sam%20jones'],
    ['Tom & Jerry', 'Tom%20&%20Jerry'],
    ['sam/kim', 'sam%2fkim'],
  ])(
    'gives focus back to %s’s row when the back link closes its page at an address typed as %s, at a phone’s width',
    async (name, typed) => {
      const host = await openedAt(`/config/users/${typed}`, who, 390)
      expect(location.pathname).toBe(`/config/users/${typed}`)
      expect(host.querySelector('.account-page .page-title')?.textContent).toBe(
        name,
      )

      await click(
        linkIn(host.querySelector('.account-page .back') as Element, '‹ Users'),
      )
      expect(location.pathname).toBe('/config/users')
      expect(host.querySelector('.account-page')).toBeNull()
      expect(document.activeElement).toBe(linkIn(table(host), name))
    },
  )

  it('opens New account beside the table at /config/new-account, under the Users tab, and goes back to its link', async () => {
    const host = await openedAt('/config/users', who, 1280)
    await click(linkIn(table(host), 'New account'))
    expect(location.pathname).toBe('/config/new-account')
    expect(host.querySelector('.split-detail .new-account form')).not.toBeNull()
    expect(table(host)).not.toBeNull()
    expect(currentTab(host)).toBe('Users')

    await click(
      linkIn(host.querySelector('.new-account .back') as Element, '‹ Users'),
    )
    expect(location.pathname).toBe('/config/users')
    expect(host.querySelector('.new-account')).toBeNull()
    expect(document.activeElement).toBe(linkIn(table(host), 'New account'))
  })

  it('gives focus back to the New account link when the back link closes the make page at an address typed as /config/New-Account, at a phone’s width', async () => {
    const host = await openedAt('/config/New-Account', who, 390)
    expect(location.pathname).toBe('/config/New-Account')
    expect(host.querySelector('.new-account form')).not.toBeNull()

    await click(
      linkIn(host.querySelector('.new-account .back') as Element, '‹ Users'),
    )
    expect(location.pathname).toBe('/config/users')
    expect(host.querySelector('.new-account')).toBeNull()
    expect(document.activeElement).toBe(linkIn(table(host), 'New account'))
  })

  it('answers /config/new-account alone at a phone’s width, under the Users tab', async () => {
    const host = await openedAt('/config/new-account', who, 390)
    expect(host.querySelector('.new-account form')).not.toBeNull()
    expect(host.querySelector('.user-table')).toBeNull()
    expect(currentTab(host)).toBe('Users')
  })
})

describe('an account’s page while its lists are read, through the app’s own router', () => {
  it('heads the page with its address as typed, keeps focus on ‹ Users taken while the libraries list is read, and takes the account’s own name once it lands', async () => {
    const libraries = deferred<{ libraries: librariesApi.Library[] }>()
    vi.mocked(librariesApi.listLibraries).mockReturnValue(libraries.promise)
    const host = await openedAt('/config/users/sam%20jones', who, 390)
    const title = () =>
      host.querySelector('.account-page .page-title')?.textContent
    const back = () =>
      linkIn(host.querySelector('.account-page .back') as Element, '‹ Users')
    expect(title()).toBe('sam jones')
    expect(host.querySelector('.account-page .settings-section')).toBeNull()
    const waiting = back()
    act(() => waiting.focus())
    expect(document.activeElement).toBe(waiting)

    await act(async () => {
      libraries.resolve({ libraries: [] })
      await new Promise((r) => setTimeout(r, 120))
    })
    expect(title()).toBe('Sam Jones')
    expect(host.querySelector('.account-page .settings-section')).not.toBeNull()
    expect(back()).toBe(waiting)
    expect(document.activeElement).toBe(waiting)
  })
})

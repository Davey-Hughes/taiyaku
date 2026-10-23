import { act } from 'react'
import {
  MemoryRouter,
  type NavigateFunction,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { useFitsPair } from './fitsPair'
import { GateContext } from './gate'
import * as librariesApi from './libraries'
import { deferred, flush } from './testing/async'
import { press } from './testing/input'
import { mount } from './testing/mount'
import { expectRowsWired, helpOf, row, section } from './testing/settingsRows'
import { cellValue } from './testing/table'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './testing/wait'
import * as usersApi from './users'
import { Users } from './Users'

// Whether the table and an open page fit side by side is the width's, which
// jsdom does not lay out. Narrow by default, as the real hook answers in
// jsdom, which has no `ResizeObserver`; a test that draws the pair says so.
vi.mock('./fitsPair', () => ({ useFitsPair: vi.fn(() => false) }))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  listLibraries: vi.fn(),
}))
vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  addUser: vi.fn(),
  listUsers: vi.fn(),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

const TABLE = '/config/users'
/** The make page, outside the names' space beneath the table. */
const NEW = '/config/new-account'

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
  every_library: true,
  libraries: [],
  ...over,
})

const who: Me = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

let steer: NavigateFunction | undefined
let where = ''

/** The path the screen is at. */
const here = () => where

/**
 * Draws the screen at `entry`, under the route `Config` gives it, signed in as
 * `signedInAs`, and lets the reads it starts answer. Another Configuration
 * tab's route sits beside it, so a test can leave the screen for one.
 */
async function at(entry = TABLE, signedInAs: Me = who) {
  // Hands the router to the test, which is outside it. Declared here for the
  // reason `Library.test.tsx`'s `locationProbe` gives:
  // `useComponentExportOnlyModules` refuses an unexported component at module
  // scope.
  function Steering() {
    steer = useNavigate()
    where = useLocation().pathname
    return null
  }
  let host: HTMLElement | undefined
  // One element for both routes, as `Config` gives it, so the screen stays
  // mounted between the make page and the pages beneath the table.
  const screen = <Users />
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <GateContext value={{ who: signedInAs, signOut: () => undefined }}>
          <Routes>
            <Route path={`${TABLE}/*`} element={screen} />
            <Route path={NEW} element={screen} />
            <Route
              path="/config/libraries"
              element={<p className="other-tab" />}
            />
          </Routes>
        </GateContext>
        <Steering />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  await flush()
  if (!host) throw new Error('nothing mounted')
  return host
}

/** Moves to `to` as a typed address or a link elsewhere on the app would. */
async function go(to: string) {
  await flush(() => steer?.(to))
  await flush()
}

/** The link whose text is `text`. */
const link = (host: HTMLElement, text: string) => {
  const found = [...host.querySelectorAll('a')].find(
    (a) => a.textContent === text,
  )
  if (found === undefined) throw new Error(`no link "${text}"`)
  return found
}

/** Follows the link whose text is `text`, and lets the page it opens read. */
async function follow(host: HTMLElement, text: string) {
  await press(link(host, text))
  await flush()
}

/** The button on the page whose text is `text`. */
const button = (host: HTMLElement, text: string) => {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  )
  if (found === undefined) throw new Error(`no button "${text}"`)
  return found
}

/** The table's rows, in the order drawn. */
const rows = (host: HTMLElement) => [
  ...host.querySelectorAll('.user-table tbody tr'),
]

/** The names of the table's accounts, in the order drawn. */
const names = (host: HTMLElement) =>
  rows(host).map((tr) => tr.querySelector('.row-link')?.textContent)

/** The title of the account page that is drawn. */
const heading = (host: HTMLElement) =>
  host.querySelector('.account-page .page-title')?.textContent

/** The account page's count of sessions and keys, its section's line. */
const tally = (host: HTMLElement) =>
  section(host, 'Sessions & keys').querySelector(
    '.settings-section-description',
  )?.textContent

/** The account page's Libraries section, which holds the grant editor. */
const grants = (host: HTMLElement) => section(host, 'Libraries')

/** The titles of the sections drawn, in order. */
const sectionTitles = (host: HTMLElement) =>
  [...host.querySelectorAll('.settings-section-title')].map(
    (title) => title.textContent,
  )

const session: usersApi.User['sessions'][number] = {
  id: 'aaa',
  created: '2026-09-01 10:00:00',
  last_seen: '2026-09-06 09:00:00',
  user_agent: 'Firefox',
  ip: '10.0.0.2',
  current: false,
}

async function fill(host: HTMLElement, name: string, value: string) {
  const box = host.querySelector(`input[name="${name}"]`) as HTMLInputElement
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, value)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

/** The make page's Add. */
const addButton = (host: HTMLElement) =>
  host.querySelector('form.add-user button[type="submit"]') as HTMLButtonElement

/** Why the name typed would be refused, said as it is typed. */
const clashNote = (host: HTMLElement) =>
  host.querySelector<HTMLElement>('form.add-user > [role="status"]')

/** Whether the name box is marked invalid, and what describes it. */
const boxMarks = (host: HTMLElement) => {
  const box = host.querySelector('input[name="new-name"]') as HTMLInputElement
  return [
    box.getAttribute('aria-invalid'),
    box.getAttribute('aria-describedby'),
  ]
}

async function submitAdd(host: HTMLElement) {
  const form = host.querySelector('form.add-user') as HTMLFormElement
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** What the page for a name the list does not hold says beneath its title. */
const NO_SUCH = 'There is no account at this address: it may have been removed.'

const unreadLibraries = () =>
  new Error('/api/library/libraries: 500 {"error":"the store is unavailable"}')

beforeEach(() => {
  // A reset, not a clear: a clear keeps the queued `mockReturnValueOnce`
  // values and the implementations a test set, so a read one test left
  // unconsumed would answer the next test's mount.
  vi.resetAllMocks()
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user()] })
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
})

describe('Users, the table', () => {
  it('lists the accounts the server sent', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await at()
    expect(names(host)).toEqual(['davey', 'sam'])
  })

  it('marks the row of the account this browser is signed in as', async () => {
    // Not the first row, so a mark drawn by position is seen.
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user({ id: 5, name: 'sam', role: 'user' }), user()],
    })
    const host = await at()
    const [sam, davey] = rows(host)
    expect(davey?.textContent).toContain('this is you')
    expect(sam?.textContent).not.toContain('this is you')
  })

  it('marks the row of an account nobody can sign in as', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', can_log_in: false })],
    })
    const host = await at()
    const [davey, sam] = rows(host)
    expect(davey?.textContent).not.toContain('locked')
    expect(sam?.querySelector('td.lead')?.textContent).toBe(
      'sam\u00a0·\u00a0locked',
    )
  })

  it('draws a table of Name, Role, Libraries and Last seen, each row a link to its account’s page through its name', async () => {
    // sam's id is not its position, and the signed-in account is not first,
    // so a link or a mark drawn by position is seen.
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({
          id: 5,
          name: 'sam',
          role: 'user',
          every_library: false,
          libraries: [
            { id: 70, name: 'Manga' },
            { id: 80, name: 'Comics' },
          ],
        }),
        user({
          sessions: [
            {
              id: 'aaa',
              created: '2026-09-01 10:00:00',
              last_seen: '2026-09-06 09:00:00',
              user_agent: 'Firefox',
              ip: '10.0.0.2',
              current: true,
            },
          ],
        }),
      ],
    })
    const host = await at()
    const table = host.querySelector('.user-table table') as HTMLElement
    expect(
      [...table.querySelectorAll('thead th')].map((th) => [
        th.textContent,
        th.getAttribute('scope'),
      ]),
    ).toEqual([
      ['Name', 'col'],
      ['Role', 'col'],
      ['Libraries', 'col'],
      ['Last seen', 'col'],
    ])
    expect(
      rows(host).map((tr) => [...tr.querySelectorAll('td')].map(cellValue)),
    ).toEqual([
      ['sam', 'user', 'Manga, Comics', '—'],
      [
        'davey\u00a0·\u00a0this is you',
        'admin',
        'Every library',
        '2026-09-06 09:00:00 UTC',
      ],
    ])
    // The row's one link is its name, and nothing else in the row is one.
    for (const tr of rows(host)) {
      expect(tr.querySelectorAll('a')).toHaveLength(1)
    }
    expect(link(host, 'sam').getAttribute('href')).toBe('/config/users/sam')
    expect(link(host, 'davey').getAttribute('href')).toBe('/config/users/davey')

    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(heading(host)).toBe('sam')
  })

  it('draws the table as the whole of an untitled box, under the header Users whose one action is New account', async () => {
    const host = await at()
    const screen = host.querySelector('.user-table') as HTMLElement
    const header = screen.querySelector(':scope > .page-header')
    expect(header?.querySelector('.page-title')?.textContent).toBe('Users')
    expect(header?.querySelector('.page-description')?.textContent).toBe(
      'Who can sign in, what each account may do, and what it sees.',
    )
    expect(
      [...(header?.querySelectorAll('.page-actions a') ?? [])].map((a) => [
        a.textContent,
        a.getAttribute('href'),
      ]),
    ).toEqual([['New account', NEW]])
    const sections = screen.querySelectorAll('section.settings-section')
    expect(sections).toHaveLength(1)
    expect(sections[0].querySelector('.settings-section-head')).toBeNull()
    const box = sections[0].querySelector(':scope > .settings-box')
    expect([...(box?.children ?? [])].map((child) => child.tagName)).toEqual([
      'TABLE',
    ])
  })

  it('draws the server sentence, not the path and status, for a list that was refused', async () => {
    vi.mocked(usersApi.listUsers).mockRejectedValue(
      new Error(
        '/api/library/users: 403 {"error":"an API key is not an administrator"}',
      ),
    )
    const host = await at()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'an API key is not an administrator',
    )
  })

  it('draws the reason the list could not be read, and no table', async () => {
    // The server's plain-text 403, as an account demoted while on the page
    // is answered.
    vi.mocked(usersApi.listUsers).mockRejectedValue(
      new Error("/api/library/users: 403 this route is an administrator's"),
    )
    const host = await at()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      "this route is an administrator's",
    )
    expect(host.querySelector('.user-table table')).toBeNull()
  })
})

describe('Users, the pages', () => {
  it('reads the list again on each change of page', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await at()
    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
    await follow(host, '‹ Users')
    expect(here()).toBe(TABLE)
    expect(usersApi.listUsers).toHaveBeenCalledTimes(3)
    await follow(host, 'New account')
    expect(here()).toBe(NEW)
    expect(usersApi.listUsers).toHaveBeenCalledTimes(4)
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    expect(usersApi.listUsers).toHaveBeenCalledTimes(5)
  })

  it('says there is no such account, with the way back to the table, for a name the list does not hold', async () => {
    const host = await at('/config/users/kim')
    expect(heading(host)).toBeUndefined()
    const page = host
      .querySelector('.missing')
      ?.closest<HTMLElement>('.settings-page') as HTMLElement
    expect(page.querySelector('.page-title')?.textContent).toBe(
      'No such account',
    )
    expect(page.querySelector('.missing')?.textContent).toBe(NO_SUCH)
    expect(link(page, '‹ Users').getAttribute('href')).toBe(TABLE)
  })

  it('goes back to the table once an account is removed, and the table says so', async () => {
    const sam = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [user(), sam] })
      .mockResolvedValue({ users: [user()] })
    const removal = {
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 0,
      },
    }
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      ...removal,
      dry_run: true,
    })
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      ...removal,
      dry_run: false,
    })
    const host = await at('/config/users/sam')
    await press(button(host, 'Remove…'))
    await press(button(host, 'Remove sam'))
    await flush()
    expect(usersApi.removeUser).toHaveBeenCalledWith(2)
    expect(here()).toBe(TABLE)
    expect(host.querySelector('.user-table > p.gone-line')?.textContent).toBe(
      'Removed sam.',
    )
    expect(names(host)).toEqual(['davey'])
  })

  it('drops the removed account from the table at once, while the reads that follow are held', async () => {
    const sam = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [user(), sam] })
      .mockReturnValue(new Promise(() => undefined))
    const removal = {
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 0,
      },
    }
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      ...removal,
      dry_run: true,
    })
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      ...removal,
      dry_run: false,
    })
    const host = await at('/config/users/sam')
    await press(button(host, 'Remove…'))
    await press(button(host, 'Remove sam'))
    await flush()
    expect(here()).toBe(TABLE)
    expect(usersApi.listUsers).toHaveBeenCalledTimes(3)
    expect(host.querySelector('.user-table > p.gone-line')?.textContent).toBe(
      'Removed sam.',
    )
    expect(names(host)).toEqual(['davey'])
  })
})

describe('Users, making one', () => {
  it('posts the name, password and role, then reloads', async () => {
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledWith('sam', 'hunter2', 'user')
    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
  })

  it('goes to the page of the account it made', async () => {
    const sam = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [user()] })
      .mockResolvedValue({ users: [user(), sam] })
    vi.mocked(usersApi.addUser).mockResolvedValue(sam)
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await flush()
    expect(here()).toBe('/config/users/sam')
    expect(heading(host)).toBe('sam')
  })

  it.each([
    { read: 'is held', after: () => new Promise<never>(() => undefined) },
    {
      read: 'fails',
      after: () => Promise.reject(new Error('/api/library/users: 503 gone')),
    },
  ])(
    'draws the made account’s page from the add’s own answer while the read that follows $read',
    async ({ after }) => {
      const sam = user({ id: 2, name: 'sam', role: 'user' })
      vi.mocked(usersApi.listUsers)
        .mockResolvedValueOnce({ users: [user()] })
        .mockImplementation(after)
      vi.mocked(usersApi.addUser).mockResolvedValue(sam)
      const host = await at(NEW)
      await fill(host, 'new-name', 'sam')
      await fill(host, 'new-password', 'hunter2')
      await submitAdd(host)
      await flush()
      expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
      expect(here()).toBe('/config/users/sam')
      expect(host.querySelector('.missing')).toBeNull()
      expect(heading(host)).toBe('sam')
    },
  )

  it('draws the server sentence when a name is already taken', async () => {
    // The list this page holds does not know `sam`, as when another admin
    // has just made it: `user.name` is `UNIQUE`, and the route's 400 and its
    // own sentence are what the page draws.
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 400 {"error":"an account is already called `sam`"}',
      ),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'is already called',
    )
  })

  it('keeps that refusal when a background reload lands', async () => {
    // The list's failure and the add's refusal are two slots: a read of the
    // list that succeeds clears the first, and must leave the second, the one
    // sentence naming why the account was not made.
    const read = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 400 {"error":"an account is already called `sam`"}',
      ),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'is already called',
    )

    await flush(() => read.resolve({ users: [user()] }))

    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'is already called',
    )
  })

  it('says a taken name once when the list read after a refused add learns of it, and says the clash once an edit takes the refusal away', async () => {
    const read = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 400 {"error":"an account is already called `sam`"}',
      ),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await flush(() =>
      read.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    const said = () =>
      [
        ...host.querySelectorAll(
          'form.add-user > [role="alert"], form.add-user > [role="status"]',
        ),
      ]
        .filter((one) => one.textContent !== '')
        .map((one) => [one.getAttribute('role'), one.textContent])
    expect(said()).toEqual([['alert', 'an account is already called `sam`']])
    expect(boxMarks(host)).toEqual([null, null])

    // Editing the name takes the refusal away, as the page's other forms do.
    await fill(host, 'new-name', 'sam2')
    expect(said()).toEqual([])
    await fill(host, 'new-name', 'sam')
    expect(said()).toEqual([['status', 'an account is already called `sam`']])
    expect(boxMarks(host)).toEqual(['true', clashNote(host)?.id])
  })

  it('draws the server sentence, not the path and status, for a refused add', async () => {
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 400 {"error":"an account is already called `sam`"}',
      ),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'an account is already called `sam`',
    )
  })

  it('draws, under the form and as it is typed, why an account cannot be called . or .., holds Add and sends nothing', async () => {
    // The server refuses both names, since no address can hold either; the
    // page says so in the server's sentence before Add.
    const why = 'An address cannot hold “.” or “..” as a name.'
    const host = await at(NEW)
    await fill(host, 'new-password', 'hunter2')
    for (const name of ['.', '..']) {
      await fill(host, 'new-name', name)
      expect(
        [...host.querySelectorAll('[role="alert"], [role="status"]')]
          .filter((a) => a.textContent !== '')
          .map((a) => [
            a.parentElement?.matches('form.add-user'),
            a.textContent,
          ]),
        name,
      ).toEqual([[true, why]])
      expect(addButton(host).disabled).toBe(true)
    }
    await submitAdd(host)
    expect(usersApi.addUser).not.toHaveBeenCalled()
  })

  it('says as it is typed that an account holds the name exactly as written, tied to the box, and holds Add at once until the name is free', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'Sam' })],
    })
    const host = await at(NEW)
    await fill(host, 'new-password', 'hunter2')
    const note = clashNote(host) as HTMLElement
    // Drawn, empty, before it has anything to say, so that its words are
    // read out as they come.
    expect(note.textContent).toBe('')
    // Another case is another account.
    await fill(host, 'new-name', 'sam')
    expect(note.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])
    expect(addButton(host).disabled).toBe(false)

    await fill(host, 'new-name', 'Sam')
    expect(clashNote(host)).toBe(note)
    expect(note.textContent).toBe('an account is already called `Sam`')
    expect(boxMarks(host)).toEqual(['true', note.id])
    // At once: no wait is drawn for it.
    expect(addButton(host).disabled).toBe(true)
    expect(addButton(host).textContent).toBe('Add')

    await fill(host, 'new-name', 'Sam 2')
    expect(clashNote(host)).toBe(note)
    expect(note.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])
    expect(addButton(host).disabled).toBe(false)
  })

  it('moves focus from Add to the name box when the list read holds it', async () => {
    const read = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    act(() => addButton(host).focus())
    expect(document.activeElement).toBe(addButton(host))
    await flush(() =>
      read.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    expect(addButton(host).disabled).toBe(true)
    expect(document.activeElement).toBe(
      host.querySelector('input[name="new-name"]'),
    )
  })

  it('leaves focus in the password box when the list read holds Add', async () => {
    const read = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    const password = host.querySelector(
      'input[name="new-password"]',
    ) as HTMLInputElement
    act(() => password.focus())
    await flush(() =>
      read.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    expect(addButton(host).disabled).toBe(true)
    expect(document.activeElement).toBe(password)
  })

  it('says the clash a list read brings beside an add refusal in other words, and holds Add with it', async () => {
    const read = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 500 {"error":"adding sam: database is locked"}',
      ),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)

    await flush(() =>
      read.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    expect(
      [
        ...(host
          .querySelector('form.add-user')
          ?.querySelectorAll('[role="alert"], [role="status"]') ?? []),
      ]
        .filter((one) => one.textContent !== '')
        .map((one) => [one.getAttribute('role'), one.textContent]),
    ).toEqual([
      ['alert', 'adding sam: database is locked'],
      ['status', 'an account is already called `sam`'],
    ])
    expect(boxMarks(host)).toEqual(['true', clashNote(host)?.id])
    expect(addButton(host).disabled).toBe(true)
  })

  it('sends nothing on Enter in the form while the name clashes', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'Sam' })],
    })
    const host = await at(NEW)
    await fill(host, 'new-name', 'Sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(usersApi.addUser).not.toHaveBeenCalled()
    expect(host.querySelector('form.add-user')).not.toBeNull()
  })

  describe('its wait, drawn only once it has lasted', () => {
    const add = (host: HTMLElement) =>
      host.querySelector(
        'form.add-user button[type="submit"]',
      ) as HTMLButtonElement
    const cancel = (host: HTMLElement) =>
      host.querySelector(
        'form.add-user button[name="cancel"]',
      ) as HTMLButtonElement
    /** Add's words, and whether it and Cancel are drawn held. */
    const state = (host: HTMLElement) => [
      add(host).textContent,
      add(host).disabled,
      cancel(host).disabled,
    ]
    const ready = ['Add', false, false]
    async function filled() {
      const host = await at(NEW)
      await fill(host, 'new-name', 'sam')
      await fill(host, 'new-password', 'hunter2')
      return host
    }

    it('draws no wait and no held button for an add refused inside the wait’s delay', async () => {
      holdTheClock()
      const sent = deferred<usersApi.User>()
      vi.mocked(usersApi.addUser).mockReturnValue(sent.promise)
      const host = await filled()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await submitAdd(host)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() =>
        sent.reject(
          new Error(
            '/api/library/users: 400 {"error":"an account is already called `sam`"}',
          ),
        ),
      )
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('[role="alert"]')?.textContent).toContain(
        'is already called',
      )
      expect([...drawn.seen]).toEqual([JSON.stringify(ready)])
    })

    it('draws Creating… and holds Add and Cancel from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(usersApi.addUser).mockReturnValue(new Promise(() => {}))
      const host = await filled()
      await submitAdd(host)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(ready)
      pass(1)
      expect(state(host)).toEqual(['Creating…', true, true])
    })

    it('holds a second add and Cancel from the send, before either is drawn held', async () => {
      holdTheClock()
      vi.mocked(usersApi.addUser).mockReturnValue(new Promise(() => {}))
      const host = await filled()
      await submitAdd(host)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(ready)
      await submitAdd(host)
      expect(usersApi.addUser).toHaveBeenCalledTimes(1)
      await press(cancel(host))
      expect(here()).toBe(NEW)
    })

    it('moves focus from Add to the name box once its wait is drawn, and keeps it there through a refusal', async () => {
      holdTheClock()
      const sent = deferred<usersApi.User>()
      vi.mocked(usersApi.addUser).mockReturnValue(sent.promise)
      const host = await filled()
      const nameBox = host.querySelector('input[name="new-name"]')
      act(() => add(host).focus())
      await submitAdd(host)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(document.activeElement).toBe(add(host))
      pass(1)
      expect(state(host)).toEqual(['Creating…', true, true])
      expect(document.activeElement).toBe(nameBox)

      await flush(() =>
        sent.reject(
          new Error(
            '/api/library/users: 400 {"error":"an account is already called `sam`"}',
          ),
        ),
      )
      expect(
        host.querySelector('form.add-user [role="alert"]')?.textContent,
      ).toBe('an account is already called `sam`')
      expect(document.activeElement).toBe(nameBox)
    })

    it('moves focus from Cancel to the name box once the add’s wait holds it', async () => {
      holdTheClock()
      vi.mocked(usersApi.addUser).mockReturnValue(new Promise(() => {}))
      const host = await filled()
      await submitAdd(host)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      // Tabbed to inside the wait's delay.
      act(() => cancel(host).focus())
      expect(document.activeElement).toBe(cancel(host))
      pass(1)
      expect(state(host)).toEqual(['Creating…', true, true])
      expect(document.activeElement).toBe(
        host.querySelector('input[name="new-name"]'),
      )
    })

    it('puts focus on the made account’s heading after an add whose wait was drawn, as after one answered at once', async () => {
      holdTheClock()
      const sent = deferred<usersApi.User>()
      vi.mocked(usersApi.addUser).mockReturnValue(sent.promise)
      const sam = user({ id: 2, name: 'sam', role: 'user' })
      const host = await filled()
      vi.mocked(usersApi.listUsers).mockResolvedValue({
        users: [user(), sam],
      })
      act(() => add(host).focus())
      await submitAdd(host)
      pass(WAIT_DRAWN_AFTER_MS)
      expect(document.activeElement).toBe(
        host.querySelector('input[name="new-name"]'),
      )

      await flush(() => sent.resolve(sam))
      await flush()
      const heading = host.querySelector('.account-page .page-title')
      expect(here()).toBe('/config/users/sam')
      expect(heading?.textContent).toBe('sam')
      expect(document.activeElement).toBe(heading)
    })
  })

  it('sends one POST for two submits dispatched before the first answers', async () => {
    // A ref rather than `disabled={adding}`: React batches state, so both
    // handlers read `adding === false`. Reachable by holding Enter in the name
    // box, and the second request trips `user.name UNIQUE` — a 400 rendered
    // over the account that was just created.
    vi.mocked(usersApi.addUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledTimes(1)
  })

  it('creates an administrator when the add form says so', async () => {
    // The role the form sends is the one it was set to, not the default the
    // server would apply — `004_auth.sql` defaults to `user` precisely because
    // an omitted column minted an administrator.
    vi.mocked(usersApi.addUser).mockResolvedValue(
      user({ id: 2, name: 'sam', role: 'admin' }),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    const select = host.querySelector(
      'select[name="new-role"]',
    ) as HTMLSelectElement
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set
      setter?.call(select, 'admin')
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledWith('sam', 'hunter2', 'admin')
  })

  it("explains the add form in the administrator's words, not the repository's", async () => {
    // O160: this help named a spec section and a migration file, neither of
    // which the person reading the screen can open. The wording is asserted
    // on each element that says it rather than the page, which holds other
    // words. The backtick and `<code>` checks hold #112's finding, that this
    // help once drew markdown backticks as themselves (`docs/notes.md`,
    // "What the pixels found that the tests did not").
    const host = await at(NEW)
    const page = host.querySelector('.new-account') as HTMLElement
    const role = row(page, 'Role')
    const said = [
      page.querySelector('.page-header .page-description'),
      helpOf(role),
    ]
    expect(said.map((one) => one?.textContent)).toEqual([
      'There is no sign-up: an administrator makes accounts.',
      'A new account gets the user role unless this is set to admin.',
    ])
    for (const one of said) {
      expect(one?.textContent).not.toMatch(/C4|§|004_auth|\.sql|`/)
      expect(one?.querySelector('code')).toBeNull()
    }
    // `user` and `admin` are the help's words because they are the form's:
    // the two options of the select its row holds.
    expect(
      [...role.querySelectorAll('select[name="new-role"] option')]
        .map((option) => option.textContent)
        .sort(),
    ).toEqual(['admin', 'user'])
  })

  it('draws the make form as one box of Name, Password and Role, with Add and Cancel at its foot, and a refused add under them', async () => {
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 500 {"error":"adding sam: database is locked"}',
      ),
    )
    const host = await at(NEW)
    const page = host.querySelector('.new-account') as HTMLElement
    expect(page.querySelector('.page-header .page-title')?.textContent).toBe(
      'New account',
    )
    expect(link(page, '‹ Users').getAttribute('href')).toBe(TABLE)
    const boxes = page.querySelectorAll('.settings-box')
    expect(boxes).toHaveLength(1)
    const form = boxes[0].querySelector(':scope > form.add-user') as Element
    const parts = () =>
      [...form.children].map((child) =>
        child.matches('.settings-row')
          ? child.querySelector('.settings-row-label')?.textContent
          : child.matches('[role="status"]')
            ? { says: child.textContent }
            : child.matches('[role="alert"]')
              ? { refuses: child.textContent }
              : [...child.querySelectorAll('button')].map((b) => b.textContent),
      )
    // Last, why the name typed would be refused, empty until it is.
    expect(parts()).toEqual([
      'Name',
      'Password',
      'Role',
      ['Add', 'Cancel'],
      { says: '' },
    ])
    expect(page.querySelector('.actions')).toBeNull()
    // A refused add is answered under the buttons that sent it, in the box.
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(parts()).toEqual([
      'Name',
      'Password',
      'Role',
      ['Add', 'Cancel'],
      { refuses: 'adding sam: database is locked' },
      { says: '' },
    ])
  })

  it('wires each of the make form’s rows to its control, and its help to the control it explains', async () => {
    const host = await at(NEW)
    expectRowsWired(host.querySelector('.new-account') as HTMLElement)
  })

  it.each([
    { empty: 'name', filled: 'new-password', value: 'hunter2' },
    { empty: 'password', filled: 'new-name', value: 'sam' },
  ])(
    'sends nothing when the $empty box is empty',
    async ({ filled, value }) => {
      const host = await at(NEW)
      await fill(host, filled, value)
      await submitAdd(host)
      expect(usersApi.addUser).not.toHaveBeenCalled()
    },
  )

  // `reload` runs on each change of page and after every write the server
  // carries out, so two reads of the list are routinely in flight at once —
  // and the server need not answer them in order. Without the sequence check
  // the *older* answer wins, and the screen settles on the list as it was
  // before the write that triggered the newer read.
  it('keeps the newer list when an older read answers after it', async () => {
    const first = deferred<{ users: usersApi.User[] }>()
    const second = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))

    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    // The make page's read and the account page's are both in flight.
    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)

    // The newer read holds a session the add's own answer does not, so the
    // page drawing it shows that this read landed.
    await flush(() =>
      second.resolve({
        users: [user(), user({ id: 2, name: 'sam', sessions: [session] })],
      }),
    )
    expect(tally(host)).toBe('1 session\u00a0·\u00a00 API keys')

    // The read the make page started finally answers, with the list as it
    // was before `sam` existed. It is the stale one and must not be drawn.
    await flush(() => first.resolve({ users: [user()] }))
    expect(heading(host)).toBe('sam')
    expect(host.querySelector('.missing')).toBeNull()
    expect(tally(host)).toBe('1 session\u00a0·\u00a00 API keys')
  })

  it('keeps the newer list when an older read *fails* after it', async () => {
    // The catch needs its own guard for the same reason and does not get one
    // for free: a stale rejection would put a red alert over a list that was
    // read successfully a moment later.
    const first = deferred<{ users: usersApi.User[] }>()
    const second = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))

    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    // The make page's read and the account page's are both in flight.
    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)

    await flush(() =>
      second.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    await flush(() => first.reject(new Error('/api/library/users: 503 gone')))

    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(heading(host)).toBe('sam')
  })

  // The libraries list is read on each change of page too, and its reads
  // need not answer in order either.
  it('keeps the newer libraries list when an older read answers after it', async () => {
    const manga = { id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }
    const comics = { id: 80, name: 'Comics', works: 0, books: 0, dirs: [] }
    const first = deferred<{ libraries: librariesApi.Library[] }>()
    vi.mocked(librariesApi.listLibraries)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue({ libraries: [manga, comics] })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const host = await at()
    await follow(host, 'sam')
    expect(librariesApi.listLibraries).toHaveBeenCalledTimes(2)
    expect(
      grants(host).querySelector('input[name="library-80"]'),
    ).not.toBeNull()

    // The table's read finally answers, with the list as it was before
    // Comics was made. It is the stale one and must not be drawn.
    await flush(() => first.resolve({ libraries: [manga] }))
    expect(
      grants(host).querySelector('input[name="library-80"]'),
    ).not.toBeNull()
  })

  it('keeps the newer libraries list when an older read *fails* after it', async () => {
    const manga = { id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }
    const first = deferred<{ libraries: librariesApi.Library[] }>()
    vi.mocked(librariesApi.listLibraries)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue({ libraries: [manga] })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const host = await at()
    await follow(host, 'sam')
    expect(librariesApi.listLibraries).toHaveBeenCalledTimes(2)

    await flush(() => first.reject(unreadLibraries()))
    expect(host.textContent).not.toContain('libraries list could not be read')
    expect(
      grants(host).querySelector('input[name="library-70"]'),
    ).not.toBeNull()
  })

  it('gives each add-form control its label’s words as its aria-label — O175', async () => {
    const host = await at(NEW)
    // The form's own three: `AccountPage.test.tsx` holds the account page's.
    // Named, because a loop over nothing passes.
    const labelled = [
      ...host.querySelectorAll<HTMLLabelElement>(
        'form.add-user label.settings-row-label',
      ),
    ].map((label) => ({ written: label.textContent, control: label.control }))
    expect(labelled.map(({ written }) => written)).toEqual([
      'Name',
      'Password',
      'Role',
    ])
    for (const { written, control } of labelled) {
      expect(control?.getAttribute('aria-label'), written ?? '').toBe(written)
    }
  })
})

describe('Users, an account’s page', () => {
  it('offers the page the libraries, and names its radios for its own account', async () => {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({
          id: 2,
          name: 'sam',
          role: 'user',
          every_library: false,
          libraries: [{ id: 70, name: 'Manga' }],
        }),
      ],
    })
    const radios = (host: HTMLElement) =>
      [
        ...grants(host).querySelectorAll<HTMLInputElement>(
          'input[type="radio"]',
        ),
      ].map((radio) => radio.name)
    const host = await at('/config/users/sam')
    const granted = grants(host).querySelector<HTMLInputElement>(
      'input[name="library-70"]',
    )
    expect(granted?.checked).toBe(true)
    // One account's radios are drawn at a time, so a shared name would break
    // nothing now; the name is the account's so that the group is that
    // account's whatever page draws it.
    expect(radios(host)).toEqual(['sees-2', 'sees-2'])

    await go('/config/users/davey')
    expect(heading(host)).toContain('davey')
    expect(radios(host)).toEqual(['sees-1', 'sees-1'])
  })

  it.each([
    [
      'sam',
      'sam would see every library, including any added later, and the ticks below would be cleared.',
    ],
    ['kim', 'kim would see every library, including any added later.'],
  ])(
    'asks before lifting account %s’s limit, in its own sentence',
    async (name, said) => {
      // kim is limited to no library, and lifting that is asked as well.
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          { id: 70, name: 'Manga', works: 0, books: 0, dirs: [] },
          { id: 80, name: 'Comics', works: 0, books: 0, dirs: [] },
        ],
      })
      vi.mocked(usersApi.listUsers).mockResolvedValue({
        users: [
          user({
            id: 2,
            name: 'sam',
            role: 'user',
            every_library: false,
            libraries: [{ id: 70, name: 'Manga' }],
          }),
          user({
            id: 3,
            name: 'kim',
            role: 'user',
            every_library: false,
          }),
        ],
      })
      const host = await at(`/config/users/${name}`)
      await act(async () => {
        grants(host)
          .querySelector<HTMLInputElement>('input[value="every"]')
          ?.click()
        await Promise.resolve()
      })
      expect(usersApi.patchUser).not.toHaveBeenCalled()
      const sentence = host
        .querySelector('.lift-limit')
        ?.getAttribute('aria-describedby')
      expect(sentence && document.getElementById(sentence)?.textContent).toBe(
        said,
      )
    },
  )

  it('says so when the libraries list could not be read, rather than drawing an empty one', async () => {
    // The grant editor draws "There are no libraries yet" for an empty
    // array, and a failed read would make that sentence a lie about the
    // server, told on the control that decides what an account sees.
    vi.mocked(librariesApi.listLibraries).mockRejectedValue(unreadLibraries())
    const host = await at('/config/users/davey')
    // The server's sentence, in the whole alert, not the path and status.
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'The libraries list could not be read (the store is unavailable), so the grant editor below shows nothing rather than what there is.',
    )
    // And the page says the same thing where the editor would be, rather than
    // claiming there are no libraries — an empty editor and an unread one
    // look identical, and only one of them is true.
    expect(grants(host).textContent).toContain('has not been read')
    expect(grants(host).textContent).not.toContain('no libraries yet')
  })

  it('reads the libraries list again on a change of page, and drops the alert once a read lands', async () => {
    vi.mocked(librariesApi.listLibraries)
      .mockRejectedValueOnce(unreadLibraries())
      .mockResolvedValue({
        libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }],
      })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const host = await at('/config/users/sam')
    expect(host.textContent).toContain('libraries list could not be read')
    expect(grants(host).querySelector('input[name="library-70"]')).toBeNull()

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(host.textContent).not.toContain('libraries list could not be read')
    expect(
      grants(host).querySelector('input[name="library-70"]'),
    ).not.toBeNull()
  })

  it('draws the grant editor unread again when a read fails after one that landed', async () => {
    // The alert says the editor shows nothing, so it must not go on drawing
    // the list the earlier read brought.
    vi.mocked(librariesApi.listLibraries)
      .mockRejectedValueOnce(unreadLibraries())
      .mockResolvedValueOnce({
        libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }],
      })
      .mockResolvedValueOnce({
        libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }],
      })
      .mockRejectedValue(unreadLibraries())
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const boxes = (host: HTMLElement) =>
      [
        ...grants(host).querySelectorAll<HTMLInputElement>(
          'input[type="checkbox"]',
        ),
      ].map((box) => box.name)
    const host = await at('/config/users/sam')
    expect(host.textContent).toContain('libraries list could not be read')
    expect(boxes(host)).toEqual([])

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(host.textContent).not.toContain('libraries list could not be read')
    expect(boxes(host)).toEqual(['library-70'])

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(host.textContent).toContain('libraries list could not be read')
    expect(boxes(host)).toEqual([])
    expect(grants(host).querySelector('.unknown')?.textContent).toContain(
      'has not been read',
    )
  })

  it('offers a library made since the screen mounted, once the page changes', async () => {
    const manga = { id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }
    const comics = { id: 80, name: 'Comics', works: 0, books: 0, dirs: [] }
    vi.mocked(librariesApi.listLibraries)
      .mockResolvedValueOnce({ libraries: [manga] })
      .mockResolvedValue({ libraries: [manga, comics] })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const host = await at('/config/users/sam')
    expect(
      grants(host).querySelector('input[name="library-70"]'),
    ).not.toBeNull()
    expect(grants(host).querySelector('input[name="library-80"]')).toBeNull()

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(
      grants(host).querySelector('input[name="library-80"]'),
    ).not.toBeNull()
  })

  it.each([
    ['the table', TABLE],
    ['the make page', NEW],
    ['the line for an unknown account', '/config/users/kim'],
  ])(
    'says nothing of the libraries list on %s, which draws no grant editor',
    async (_, page) => {
      // The alert speaks of "the grant editor below", which only an account's
      // page draws.
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(unreadLibraries())
      const host = await at(page)
      expect(sectionTitles(host)).not.toContain('Libraries')
      expect(host.textContent).not.toContain('libraries list could not be read')
    },
  )
})

describe('Users, an answer that comes after the person has moved on', () => {
  const davey = user()
  const sam = user({ id: 2, name: 'sam', role: 'user' })
  const kim = user({ id: 3, name: 'kim', role: 'user' })
  const removal = {
    removed: {
      name: 'sam',
      positions: 0,
      notes: 0,
      sessions: 0,
      keys: 0,
      grants: 0,
    },
  }

  /**
   * Starts making sam on the make page, and holds the answer until `answer`
   * lets it go. `answer` counts the reads of the list from then on.
   */
  async function making() {
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, kim] })
    const add = deferred<usersApi.User>()
    vi.mocked(usersApi.addUser).mockReturnValue(add.promise)
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledTimes(1)
    const answer = async () => {
      vi.mocked(usersApi.listUsers).mockResolvedValue({
        users: [davey, kim, sam],
      })
      vi.mocked(usersApi.listUsers).mockClear()
      await flush(() => add.resolve(sam))
      await flush()
    }
    return { host, answer }
  }

  /** As `making`, for sam's removal from sam's page. */
  async function removing() {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [davey, sam, kim],
    })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      ...removal,
      dry_run: true,
    })
    const remove = deferred<usersApi.RemovalAnswer>()
    vi.mocked(usersApi.removeUser).mockReturnValue(remove.promise)
    const host = await at('/config/users/sam')
    await press(button(host, 'Remove…'))
    await press(button(host, 'Remove sam'))
    expect(usersApi.removeUser).toHaveBeenCalledTimes(1)
    const answer = async () => {
      vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, kim] })
      vi.mocked(usersApi.listUsers).mockClear()
      await flush(() => remove.resolve({ ...removal, dry_run: false }))
      await flush()
    }
    return { host, answer }
  }

  it.each([
    {
      where: 'on another account’s page',
      leave: (_host: HTMLElement) => go('/config/users/kim'),
      to: '/config/users/kim',
    },
    {
      where: 'on the table',
      leave: (host: HTMLElement) => follow(host, '‹ Users'),
      to: TABLE,
    },
    {
      where: 'on another Configuration tab',
      leave: (_host: HTMLElement) => go('/config/libraries'),
      to: '/config/libraries',
    },
  ])(
    'leaves the person $where when a make answers there, and reads the list once',
    async ({ leave, to }) => {
      const { host, answer } = await making()
      await leave(host)
      await answer()
      expect(here()).toBe(to)
      // The one read is `added`'s own, for a make whose form has gone. On
      // another tab it is sent though nothing draws its answer.
      expect(usersApi.listUsers).toHaveBeenCalledTimes(1)
      // What was made is in the list the page draws once that read lands.
      if (to === TABLE) expect(names(host)).toEqual(['davey', 'kim', 'sam'])
      if (to === '/config/users/kim') expect(heading(host)).toBe('kim')
    },
  )

  it('leaves a fresh make form where it is when the make sent from an earlier one answers, and reads the list once', async () => {
    const { host, answer } = await making()
    await follow(host, '‹ Users')
    await follow(host, 'New account')
    await fill(host, 'new-name', 'lee, half typed')
    expect(here()).toBe(NEW)
    await answer()
    expect(here()).toBe(NEW)
    expect(
      host.querySelector<HTMLInputElement>('input[name="new-name"]')?.value,
    ).toBe('lee, half typed')
    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)
  })

  it('leaves the person on another account’s page when a removal answers there, says so there without taking focus, and drops it on the next change of page', async () => {
    const { host, answer } = await removing()
    await go('/config/users/kim')
    const before = host.querySelector('select[name="role"]') as HTMLElement
    act(() => before.focus())
    // Nothing takes focus to have it read, so it lands in a live region,
    // which is drawn with the page, empty, before the answer comes: a region
    // drawn together with what it says is often not read.
    const region = host.querySelector('.account-page > [aria-live="polite"]')
    const emptyBefore = region?.childNodes.length
    await answer()
    expect(here()).toBe('/config/users/kim')
    expect(heading(host)).toBe('kim')
    expect(region).not.toBeNull()
    expect(emptyBefore).toBe(0)
    expect(host.querySelector('.account-page > [aria-live="polite"]')).toBe(
      region,
    )
    expect(region?.textContent).toBe('Removed sam.')
    expect(document.activeElement).toBe(before)
    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)

    await go('/config/users/davey')
    expect(host.textContent).not.toContain('Removed sam.')
    await follow(host, '‹ Users')
    expect(host.textContent).not.toContain('Removed sam.')
    // Not drawn again on the page it was drawn on, opened again.
    await go('/config/users/kim')
    expect(heading(host)).toBe('kim')
    expect(host.textContent).not.toContain('Removed sam.')
  })

  it('leaves the person on the table when a removal answers there, says so there without taking focus, and reads the list once', async () => {
    const { host, answer } = await removing()
    await follow(host, '‹ Users')
    const before = link(host, 'New account')
    act(() => before.focus())
    await answer()
    expect(here()).toBe(TABLE)
    expect(
      host.querySelector('.user-table > [aria-live="polite"]')?.textContent,
    ).toBe('Removed sam.')
    expect(document.activeElement).toBe(before)
    expect(names(host)).toEqual(['davey', 'kim'])
    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)
  })

  it('leaves the person on another Configuration tab when a removal answers there, and says it nowhere', async () => {
    const { host, answer } = await removing()
    await go('/config/libraries')
    await answer()
    expect(here()).toBe('/config/libraries')
    expect(host.querySelector('.other-tab')).not.toBeNull()
    await go(TABLE)
    expect(names(host)).toEqual(['davey', 'kim'])
    expect(host.textContent).not.toContain('Removed sam.')
  })

  it('goes to the table from the account’s page left and opened again before its removal answered, and says so there', async () => {
    const { host, answer } = await removing()
    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    await answer()
    expect(here()).toBe(TABLE)
    expect(host.querySelector('.user-table > p.gone-line')?.textContent).toBe(
      'Removed sam.',
    )
    expect(names(host)).toEqual(['davey', 'kim'])
  })
})

describe('Users, a write refused once its page has gone', () => {
  const SAM = '/config/users/sam'
  const locked = () =>
    new Error('/api/library/users: 500 {"error":"the store is locked"}')

  /** Chooses `value` in `select`, as a person would. */
  async function choose(select: HTMLSelectElement, value: string) {
    await act(async () => {
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(select, value)
      select.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
    })
  }

  const writes = [
    {
      what: 'a make',
      entry: NEW,
      held: () => vi.mocked(usersApi.addUser),
      start: async (host: HTMLElement) => {
        await fill(host, 'new-name', 'lee')
        await fill(host, 'new-password', 'hunter2')
        await submitAdd(host)
      },
      said: 'The account lee was not made (the store is locked).',
    },
    {
      what: 'a role change',
      entry: SAM,
      held: () => vi.mocked(usersApi.patchUser),
      start: (host: HTMLElement) =>
        choose(
          host.querySelector('select[name="role"]') as HTMLSelectElement,
          'admin',
        ),
      said: 'The role for sam was not changed to admin (the store is locked).',
    },
    {
      what: 'a grant',
      entry: SAM,
      held: () => vi.mocked(usersApi.patchUser),
      start: (host: HTMLElement) =>
        press(host.querySelector('input[name="library-70"]') as Element),
      said: 'Libraries for sam were not saved (the store is locked).',
    },
    {
      what: 'Only these',
      entry: SAM,
      over: { every_library: true },
      held: () => vi.mocked(usersApi.patchUser),
      start: (host: HTMLElement) =>
        press(host.querySelector('input[value="only"]') as Element),
      said: 'Libraries for sam were not saved (the store is locked).',
    },
    {
      what: 'a lifted limit',
      entry: SAM,
      held: () => vi.mocked(usersApi.patchUser),
      start: async (host: HTMLElement) => {
        await press(host.querySelector('input[value="every"]') as Element)
        await press(button(host, 'See every library'))
      },
      said: 'Libraries for sam were not saved (the store is locked).',
    },
    {
      what: 'a permission',
      entry: SAM,
      held: () => vi.mocked(usersApi.patchUser),
      start: (host: HTMLElement) =>
        press(host.querySelector('input[name="can_download"]') as Element),
      said: 'Can download for sam was not saved (the store is locked).',
    },
    {
      what: 'a session’s revocation',
      entry: SAM,
      over: { sessions: [session] },
      held: () => vi.mocked(usersApi.revokeUserSession),
      start: (host: HTMLElement) =>
        press(
          host.querySelector('button[name="revoke-session-aaa"]') as Element,
        ),
      said: 'A session for sam was not revoked (the store is locked).',
    },
    {
      what: 'a key’s revocation',
      entry: SAM,
      over: {
        keys: [
          {
            id: 5,
            name: 'laptop',
            created: '2026-09-01 10:00:00',
            last_used: null,
          },
        ],
      },
      held: () => vi.mocked(usersApi.revokeUserKey),
      start: (host: HTMLElement) =>
        press(host.querySelector('button[name="revoke-key-5"]') as Element),
      said: 'The API key laptop for sam was not revoked (the store is locked).',
    },
    {
      what: 'a password',
      entry: SAM,
      held: () => vi.mocked(usersApi.patchUser),
      start: async (host: HTMLElement) => {
        await fill(host, 'password', 'hunter3')
        await act(async () => {
          host
            .querySelector('form.reset')
            ?.dispatchEvent(
              new Event('submit', { bubbles: true, cancelable: true }),
            )
          await Promise.resolve()
        })
      },
      said: 'A new password for sam was not set (the store is locked).',
    },
    {
      what: 'a removal',
      entry: SAM,
      held: () => vi.mocked(usersApi.removeUser),
      start: async (host: HTMLElement) => {
        await press(button(host, 'Remove…'))
        await press(button(host, 'Remove sam'))
      },
      said: 'sam was not removed (the store is locked).',
    },
  ]

  /** Draws `entry`, sends the write, and holds its answer. */
  async function sent({ entry, over, held, start }: (typeof writes)[number]) {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, dirs: [] }],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({
          id: 2,
          name: 'sam',
          role: 'user',
          every_library: false,
          ...over,
        }),
        user({ id: 3, name: 'kim', role: 'user' }),
      ],
    })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 0,
      },
      dry_run: true,
    })
    const write = deferred<never>()
    held().mockReturnValue(write.promise)
    const host = await at(entry)
    await start(host)
    expect(held()).toHaveBeenCalledTimes(1)
    return { host, write }
  }

  const places = [
    {
      place: 'another account’s page',
      leave: (_host: HTMLElement) => go('/config/users/kim'),
      page: '.account-page',
    },
    {
      place: 'the table',
      leave: (host: HTMLElement) => follow(host, '‹ Users'),
      page: '.user-table',
    },
  ]

  it.each(
    writes.flatMap((write) => places.map((place) => ({ ...write, ...place }))),
  )(
    'says $what refused once its page has gone on $place, without taking focus, and drops it on the next change of page',
    async (each) => {
      const { host, write } = await sent(each)
      await each.leave(host)
      const before = host.querySelector(`${each.page} a`) as HTMLElement
      act(() => before.focus())
      await flush(() => write.reject(locked()))
      await flush()
      expect(
        [...host.querySelectorAll(`${each.page} > [role="alert"]`)].map(
          (p) => p.textContent,
        ),
      ).toEqual([each.said])
      expect(document.activeElement).toBe(before)

      const drawnAt = here()
      await go('/config/users/davey')
      expect(host.textContent).not.toContain('the store is locked')
      await go(drawnAt)
      expect(host.textContent).not.toContain('the store is locked')
    },
  )

  it.each(writes)(
    'says $what refused once another tab has replaced the screen nowhere, and leaves the person there',
    async (each) => {
      const { host, write } = await sent(each)
      await go('/config/libraries')
      await flush(() => write.reject(locked()))
      await flush()
      expect(here()).toBe('/config/libraries')
      await go(TABLE)
      expect(host.textContent).not.toContain('the store is locked')
    },
  )
})

describe('Users, where focus goes', () => {
  const davey = user()
  const sam = user({ id: 2, name: 'sam', role: 'user' })
  const removal = {
    removed: {
      name: 'sam',
      positions: 0,
      notes: 0,
      sessions: 0,
      keys: 0,
      grants: 0,
    },
  }
  const accountHeading = (host: HTMLElement) =>
    host.querySelector('.account-page .page-title')
  const tableHeading = (host: HTMLElement) =>
    host.querySelector('.user-table .page-title')

  async function makeSam(host: HTMLElement) {
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await flush()
  }

  // Not by its row's link, which gives the heading focus as opening any row
  // does: what is held is that the make's arrival is spent once.
  it.each([
    {
      by: 'its address',
      back: (_host: HTMLElement) => go('/config/users/sam'),
    },
    {
      by: 'Back',
      back: async (_host: HTMLElement) => {
        await flush(() => steer?.(-1))
        await flush()
      },
    },
  ])(
    'puts focus on the made account’s heading, and not again when the person comes back to it by $by',
    async ({ back }) => {
      vi.mocked(usersApi.listUsers)
        .mockResolvedValueOnce({ users: [davey] })
        .mockResolvedValue({ users: [davey, sam] })
      vi.mocked(usersApi.addUser).mockResolvedValue(sam)
      const host = await at(NEW)
      await makeSam(host)
      expect(here()).toBe('/config/users/sam')
      expect(accountHeading(host)?.textContent).toBe('sam')
      expect(document.activeElement).toBe(accountHeading(host))

      await follow(host, '‹ Users')
      await back(host)
      expect(here()).toBe('/config/users/sam')
      expect(document.activeElement).not.toBe(accountHeading(host))
    },
  )

  it('leaves the made account’s heading alone when the make answered after the person left its form', async () => {
    const add = deferred<usersApi.User>()
    vi.mocked(usersApi.addUser).mockReturnValue(add.promise)
    const host = await at(NEW)
    await makeSam(host)
    await follow(host, '‹ Users')
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, sam] })
    await flush(() => add.resolve(sam))
    await flush()
    // By its address: its row's link gives the heading focus whatever the
    // make did.
    await go('/config/users/sam')
    expect(here()).toBe('/config/users/sam')
    expect(accountHeading(host)?.textContent).toBe('sam')
    expect(document.activeElement).not.toBe(accountHeading(host))
  })

  it('puts focus on the line that says the account was removed, leaves it where the person puts it when the list that follows answers, and drops the line on the next change of page', async () => {
    const after = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [davey, sam] })
      // The page's own read after the write, then the table's.
      .mockResolvedValueOnce({ users: [davey] })
      .mockReturnValueOnce(after.promise)
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      ...removal,
      dry_run: true,
    })
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      ...removal,
      dry_run: false,
    })
    const host = await at('/config/users/sam')
    await press(button(host, 'Remove…'))
    await press(button(host, 'Remove sam'))
    await flush()
    expect(here()).toBe(TABLE)
    // The mount's read, the page's own after the removal (`settled`), and the
    // table's on the change of page: the last is the one held.
    expect(usersApi.listUsers).toHaveBeenCalledTimes(3)
    const line = host.querySelector('.user-table > p.gone-line')
    expect(line?.textContent).toBe('Removed sam.')
    expect(document.activeElement).toBe(line)

    const elsewhere = link(host, 'New account')
    act(() => elsewhere.focus())
    await flush(() => after.resolve({ users: [davey] }))
    expect(document.activeElement).toBe(elsewhere)

    // The table opened again is opened by the person: no line, and focus
    // goes back to the link of the page they closed, not to a line.
    await follow(host, 'New account')
    await follow(host, '‹ Users')
    expect(here()).toBe(TABLE)
    expect(host.querySelector('.user-table > p.gone-line')).toBeNull()
    expect(document.activeElement).toBe(link(host, 'New account'))
  })

  it('puts focus on the line that says there is no such account', async () => {
    const host = await at('/config/users/kim')
    const missing = host.querySelector('.missing')
    expect(missing?.textContent).toBe(NO_SUCH)
    expect(document.activeElement).toBe(missing)
  })

  it('puts focus on the no-such-account line again when a second address naming nothing follows the first', async () => {
    const host = await at('/config/users/kim')
    expect(document.activeElement).toBe(host.querySelector('.missing'))
    act(() => (document.activeElement as HTMLElement).blur())
    await go('/config/users/lee')
    expect(here()).toBe('/config/users/lee')
    const missing = host.querySelector('.missing')
    expect(missing).not.toBeNull()
    expect(document.activeElement).toBe(missing)
  })

  it('puts focus on the line that says there is no such account when a read finds the one drawn gone', async () => {
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [davey, sam] })
      .mockResolvedValue({ users: [davey] })
    vi.mocked(usersApi.patchUser).mockResolvedValue({
      ...sam,
      can_download: false,
    })
    const host = await at('/config/users/sam')
    expect(host.querySelector('.missing')).toBeNull()
    await press(host.querySelector('input[name="can_download"]') as Element)
    await flush()
    expect(here()).toBe('/config/users/sam')
    const missing = host.querySelector('.missing')
    expect(missing?.textContent).toBe(NO_SUCH)
    expect(document.activeElement).toBe(missing)
  })

  it('puts focus on the table’s heading when the make form is cancelled', async () => {
    const host = await at(NEW)
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    const heading = tableHeading(host)
    expect(heading?.textContent).toBe('Users')
    expect(document.activeElement).toBe(heading)
  })

  it('puts focus on the heading of a page its row’s link opens, back on that row when its back link closes it, and leaves it where it falls on Back', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, sam] })
    const host = await at()
    const row = link(host, 'sam')
    act(() => row.focus())
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(accountHeading(host)?.textContent).toBe('sam')
    expect(document.activeElement).toBe(accountHeading(host))

    const back = link(host, '‹ Users')
    act(() => back.focus())
    await follow(host, '‹ Users')
    expect(here()).toBe(TABLE)
    expect(document.activeElement).toBe(link(host, 'sam'))

    await flush(() => steer?.(-1))
    await flush()
    expect(here()).toBe('/config/users/sam')
    expect(document.activeElement).toBe(document.body)
  })

  it('gives focus to the made account’s row on the table its back link goes to', async () => {
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [davey] })
      .mockResolvedValue({ users: [davey, sam] })
    vi.mocked(usersApi.addUser).mockResolvedValue(sam)
    const host = await at(NEW)
    await makeSam(host)
    expect(document.activeElement).toBe(accountHeading(host))
    const back = link(host, '‹ Users')
    act(() => back.focus())
    await follow(host, '‹ Users')
    expect(here()).toBe(TABLE)
    expect(document.activeElement).toBe(link(host, 'sam'))
  })

  it('puts focus on the heading of an account’s page opened from the table Cancel went to', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, sam] })
    const host = await at(NEW)
    await press(button(host, 'Cancel'))
    await flush()
    expect(document.activeElement).toBe(tableHeading(host))
    const row = link(host, 'sam')
    act(() => row.focus())
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/sam')
    expect(accountHeading(host)?.textContent).toBe('sam')
    expect(document.activeElement).toBe(accountHeading(host))
  })

  it('leaves focus on the box the person pressed on the made account’s page when its write answers', async () => {
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [davey] })
      .mockResolvedValue({ users: [davey, sam] })
    vi.mocked(usersApi.addUser).mockResolvedValue(sam)
    vi.mocked(usersApi.patchUser).mockResolvedValue({
      ...sam,
      can_download: false,
    })
    const host = await at(NEW)
    await makeSam(host)
    expect(document.activeElement).toBe(accountHeading(host))
    const box = host.querySelector(
      'input[name="can_download"]',
    ) as HTMLInputElement
    act(() => box.focus())
    await press(box)
    await flush()
    // The write's answer and the read after it have both landed.
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(box.checked).toBe(false)
    expect(document.activeElement).toBe(box)
  })

  it('leaves focus where the person put it on the table Cancel went to when the table’s read lands', async () => {
    const held = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [davey] })
      .mockReturnValueOnce(held.promise)
    const host = await at(NEW)
    await press(button(host, 'Cancel'))
    await flush()
    expect(document.activeElement).toBe(tableHeading(host))
    const elsewhere = link(host, 'New account')
    act(() => elsewhere.focus())
    await flush(() => held.resolve({ users: [davey, sam] }))
    expect(names(host)).toEqual(['davey', 'sam'])
    expect(document.activeElement).toBe(elsewhere)
  })
})

describe('Users, side by side', () => {
  it('draws an account alone where the two do not fit, with no table', async () => {
    vi.mocked(useFitsPair).mockReturnValue(false)
    const host = await at(`${TABLE}/davey`)
    expect(host.querySelector('.account-page .page-title')?.textContent).toBe(
      'davey — this is you',
    )
    expect(host.querySelector('.user-table')).toBeNull()
  })

  it('draws New account alone where the two do not fit, with no table', async () => {
    vi.mocked(useFitsPair).mockReturnValue(false)
    const host = await at(NEW)
    expect(host.querySelector('.new-account .page-title')?.textContent).toBe(
      'New account',
    )
    expect(host.querySelector('.user-table')).toBeNull()
  })

  it('draws an account beside the table where the two fit, its row marked as the open one', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await at(`${TABLE}/sam`)
    expect(
      host.querySelector('.split-detail .account-page .page-title')
        ?.textContent,
    ).toBe('sam')
    expect(names(host)).toEqual(['davey', 'sam'])
    expect(
      rows(host).map((tr) =>
        tr.querySelector('.row-link')?.getAttribute('aria-current'),
      ),
    ).toEqual([null, 'page'])
  })

  it('draws New account beside the table where the two fit', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    const host = await at(NEW)
    expect(
      host.querySelector('.split-detail .new-account .page-title')?.textContent,
    ).toBe('New account')
    expect(names(host)).toEqual(['davey'])
  })

  // The list's failure stands for every notice: `Users.tsx` hands the one
  // set to whichever of the two is open, the page before the table.
  it.each([
    { page: 'New account', entry: NEW, drawnIn: '.split-detail' },
    {
      page: 'an account’s address',
      entry: `${TABLE}/davey`,
      drawnIn: '.split-detail',
    },
    { page: 'no page', entry: TABLE, drawnIn: '.split-list' },
  ])(
    'says the list could not be read once, on $page, where the two fit',
    async ({ entry, drawnIn }) => {
      vi.mocked(useFitsPair).mockReturnValue(true)
      vi.mocked(usersApi.listUsers).mockRejectedValue(
        new Error('/api/library/users: 503 {"error":"the store is locked"}'),
      )
      const host = await at(entry)
      const alerts = [...host.querySelectorAll('[role="alert"]')]
      expect(alerts.map((alert) => alert.textContent)).toEqual([
        'the store is locked',
      ])
      expect(alerts[0].closest(drawnIn)).not.toBeNull()
      // The table's header is drawn in every case, so one alert is not one
      // for want of a table to draw a second in.
      expect(host.querySelector('.user-table')).not.toBeNull()
    },
  )
})

describe('Users, addressed by name', () => {
  /** Whether each row of the table is marked as the open one, in order. */
  const marks = (host: HTMLElement) =>
    rows(host).map((tr) =>
      tr.querySelector('.row-link')?.getAttribute('aria-current'),
    )

  it('opens an account at its name, and one named as a number at that name rather than at an id', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: '3', role: 'user' }),
        user({ id: 3, name: 'kim', role: 'user' }),
      ],
    })
    const host = await at(`${TABLE}/3`)
    expect(heading(host)).toBe('3')
    await go(`${TABLE}/kim`)
    expect(heading(host)).toBe('kim')
  })

  it('opens an account at its name in another ASCII case where no other name folds to it, and each of two names differing only in case at its own', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'Sam', role: 'user' }),
        user({ id: 3, name: 'sam', role: 'user' }),
        user({ id: 4, name: 'kim', role: 'user' }),
      ],
    })
    const host = await at(`${TABLE}/KIM`)
    expect(heading(host)).toBe('kim')
    await go(`${TABLE}/Sam`)
    expect(heading(host)).toBe('Sam')
    await go(`${TABLE}/sam`)
    expect(heading(host)).toBe('sam')
  })

  it('says there is no such account at an address whose case two names fold to', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'Sam', role: 'user' }),
        user({ id: 3, name: 'sam', role: 'user' }),
      ],
    })
    const host = await at(`${TABLE}/SAM`)
    expect(heading(host)).toBeUndefined()
    expect(host.querySelector('.missing')?.textContent).toBe(NO_SUCH)
  })

  it('reads a name holding the text %2F as written, not as the name with a slash', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'a/b', role: 'user' }),
        user({ id: 3, name: 'a%2Fb', role: 'user' }),
      ],
    })
    const host = await at(`${TABLE}/a%252Fb`)
    expect(heading(host)).toBe('a%2Fb')
    await go(`${TABLE}/a%2Fb`)
    expect(heading(host)).toBe('a/b')
  })

  it('links each account at its name, escaped where an address needs it', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'Sam Jones', role: 'user' }),
        user({ id: 3, name: 'Tom & Jerry/Extras?', role: 'user' }),
      ],
    })
    const host = await at()
    expect(
      rows(host).map((tr) => {
        const a = tr.querySelector('.row-link')
        return [a?.textContent, a?.getAttribute('href')]
      }),
    ).toEqual([
      ['davey', `${TABLE}/davey`],
      ['Sam Jones', `${TABLE}/Sam%20Jones`],
      ['Tom & Jerry/Extras?', `${TABLE}/Tom%20%26%20Jerry%2FExtras%3F`],
    ])
    await follow(host, 'Tom & Jerry/Extras?')
    expect(here()).toBe(`${TABLE}/Tom%20%26%20Jerry%2FExtras%3F`)
    expect(heading(host)).toBe('Tom & Jerry/Extras?')
  })

  it('marks the row of the account the address opens, and no other, where two names differ only in case and where the address’s case is another', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'Sam', role: 'user' }),
        user({ id: 3, name: 'sam', role: 'user' }),
        user({ id: 4, name: 'kim', role: 'user' }),
      ],
    })
    const host = await at(`${TABLE}/sam`)
    expect(heading(host)).toBe('sam')
    expect(marks(host)).toEqual([null, null, 'page', null])
    await go(`${TABLE}/Sam`)
    expect(marks(host)).toEqual([null, 'page', null, null])
    await go(`${TABLE}/KIM`)
    expect(heading(host)).toBe('kim')
    expect(marks(host)).toEqual([null, null, null, 'page'])
  })

  it('draws no page for a name while the list is being read, and the account’s once it is', async () => {
    const list = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValue(list.promise)
    const host = await at(`${TABLE}/sam`)
    expect(host.querySelector('.missing')).toBeNull()
    expect(host.querySelector('.account-page')).toBeNull()
    await flush(() =>
      list.resolve({
        users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
      }),
    )
    expect(heading(host)).toBe('sam')
  })

  it('draws the make page at /config/new-account, and the account named “new” at its own address', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'new', role: 'user' })],
    })
    const host = await at('/config/new-account')
    expect(host.querySelector('.new-account form.add-user')).not.toBeNull()
    expect(heading(host)).toBeUndefined()
    await go(`${TABLE}/new`)
    expect(host.querySelector('form.add-user')).toBeNull()
    expect(heading(host)).toBe('new')
  })

  it('opens an account it has just made at the address of its name, and focuses its title', async () => {
    const made = user({ id: 2, name: 'Sam Jones', role: 'user' })
    vi.mocked(usersApi.listUsers)
      .mockResolvedValueOnce({ users: [user()] })
      .mockResolvedValue({ users: [user(), made] })
    vi.mocked(usersApi.addUser).mockResolvedValue(made)
    const host = await at('/config/new-account')
    await fill(host, 'new-name', 'Sam Jones')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await flush()
    expect(here()).toBe(`${TABLE}/Sam%20Jones`)
    expect(heading(host)).toBe('Sam Jones')
    expect(document.activeElement).toBe(
      host.querySelector('.account-page .page-title'),
    )
  })
})

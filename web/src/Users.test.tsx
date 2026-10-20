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
import { GateContext } from './gate'
import * as librariesApi from './libraries'
import { deferred, flush } from './testing/async'
import { fields } from './testing/fields'
import { press } from './testing/input'
import { mount } from './testing/mount'
import { cellValue } from './testing/table'
import * as usersApi from './users'
import { Users } from './Users'

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
const NEW = '/config/users/new'

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
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <GateContext value={{ who: signedInAs, signOut: () => undefined }}>
          <Routes>
            <Route path={`${TABLE}/*`} element={<Users />} />
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

/** The heading of the account page that is drawn. */
const heading = (host: HTMLElement) =>
  host.querySelector('.user-row > h2')?.textContent

/** The account page's count of sessions and keys. */
const tally = (host: HTMLElement) =>
  host.querySelector('.user-row .tally')?.textContent

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

async function submitAdd(host: HTMLElement) {
  const form = host.querySelector('form.add-user') as HTMLFormElement
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

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
    expect(sam?.querySelector('td.lead')?.textContent).toBe('sam · locked')
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
      ['davey · this is you', 'admin', 'Every library', '2026-09-06 09:00:00'],
    ])
    // The row's one link is its name, and nothing else in the row is one.
    for (const tr of rows(host)) {
      expect(tr.querySelectorAll('a')).toHaveLength(1)
    }
    expect(link(host, 'sam').getAttribute('href')).toBe('/config/users/5')
    expect(link(host, 'davey').getAttribute('href')).toBe('/config/users/1')

    await follow(host, 'sam')
    expect(here()).toBe('/config/users/5')
    expect(heading(host)).toBe('sam')
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
    vi.mocked(usersApi.listUsers).mockRejectedValue(
      new Error('/api/library/users: 403 an API key is not an administrator'),
    )
    const host = await at()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'an API key is not an administrator',
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
    expect(here()).toBe('/config/users/2')
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

  it('says there is no such account, with the way back to the table, for an id the list does not hold', async () => {
    const host = await at('/config/users/9')
    expect(heading(host)).toBeUndefined()
    expect(host.querySelector('.missing')?.textContent).toBe('No such account.')
    expect(link(host, '‹ Users').getAttribute('href')).toBe(TABLE)
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
    const host = await at('/config/users/2')
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
    const host = await at('/config/users/2')
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
    expect(here()).toBe('/config/users/2')
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
      expect(here()).toBe('/config/users/2')
      expect(host.querySelector('.missing')).toBeNull()
      expect(heading(host)).toBe('sam')
    },
  )

  it('draws the server sentence when a name is already taken', async () => {
    // C4 §4: no client-side uniqueness check. `user.name` is `UNIQUE` and the
    // route answers 400 with its own sentence; a second opinion here would be
    // a race with a store this browser does not own.
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error('/api/library/users: 400 an account named sam already exists'),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already exists',
    )
  })

  it('keeps that refusal when a background reload lands', async () => {
    // The list's failure and the add's refusal are two slots: a read of the
    // list that succeeds clears the first, and must leave the second, the one
    // sentence naming why the account was not made.
    const read = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error('/api/library/users: 400 an account named sam already exists'),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already exists',
    )

    await flush(() => read.resolve({ users: [user()] }))

    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already exists',
    )
  })

  it('draws the server sentence, not the path and status, for a refused add', async () => {
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error(
        '/api/library/users: 400 {"error":"an account named sam already exists"}',
      ),
    )
    const host = await at(NEW)
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'an account named sam already exists',
    )
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
    // O160: this paragraph named a spec section and a migration file, neither
    // of which the person reading the screen can open. The wording is asserted
    // on the paragraph itself rather than the page, which holds other words.
    // The backtick and `<code>` checks hold #112's finding, that this
    // paragraph once drew markdown backticks as themselves (`docs/notes.md`,
    // "What the pixels found that the tests did not").
    const host = await at(NEW)
    const why = host.querySelector('form.add-user > p.why')
    expect(why).not.toBeNull()
    const text = why?.textContent ?? ''
    expect(text).toContain('an administrator makes accounts')
    expect(text).toContain('the user role unless Role is set to admin')
    expect(text).not.toMatch(/C4|§|004_auth|\.sql|`/)
    expect(why?.querySelector('code')).toBeNull()
    // "Role", `user` and `admin` are the paragraph's words because they are
    // the form's: the select's label and its two options.
    const select = host.querySelector('form.add-user select[name="new-role"]')
    expect(select?.closest('label')?.querySelector('span')?.textContent).toBe(
      'Role',
    )
    expect(
      [...(select?.querySelectorAll('option') ?? [])]
        .map((option) => option.textContent)
        .sort(),
    ).toEqual(['admin', 'user'])
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
    expect(tally(host)).toBe('1 session · 0 API keys')

    // The read the make page started finally answers, with the list as it
    // was before `sam` existed. It is the stale one and must not be drawn.
    await flush(() => first.resolve({ users: [user()] }))
    expect(heading(host)).toBe('sam')
    expect(host.querySelector('.missing')).toBeNull()
    expect(tally(host)).toBe('1 session · 0 API keys')
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
    const manga = { id: 70, name: 'Manga', works: 0, books: 0, folders: [] }
    const comics = { id: 80, name: 'Comics', works: 0, books: 0, folders: [] }
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
      host.querySelector('.grants input[name="library-80"]'),
    ).not.toBeNull()

    // The table's read finally answers, with the list as it was before
    // Comics was made. It is the stale one and must not be drawn.
    await flush(() => first.resolve({ libraries: [manga] }))
    expect(
      host.querySelector('.grants input[name="library-80"]'),
    ).not.toBeNull()
  })

  it('keeps the newer libraries list when an older read *fails* after it', async () => {
    const manga = { id: 70, name: 'Manga', works: 0, books: 0, folders: [] }
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
      host.querySelector('.grants input[name="library-70"]'),
    ).not.toBeNull()
  })

  it('gives each add-form control its label’s words as its aria-label — O175', async () => {
    const host = await at(NEW)
    // The form's own three: `AccountPage.test.tsx` holds the account page's.
    // Counted, because a loop over nothing passes.
    const labelled = fields(host.querySelectorAll('form.add-user label.field'))
    expect(labelled).toHaveLength(3)
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })
})

describe('Users, an account’s page', () => {
  it('offers the page the libraries, and names its radios for its own account', async () => {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, folders: [] }],
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
        ...host.querySelectorAll<HTMLInputElement>(
          '.grants input[type="radio"]',
        ),
      ].map((radio) => radio.name)
    const host = await at('/config/users/2')
    const granted = host.querySelector<HTMLInputElement>(
      '.grants input[name="library-70"]',
    )
    expect(granted?.checked).toBe(true)
    // One account's radios are drawn at a time, so a shared name would break
    // nothing now; the name is the account's so that the group is that
    // account's whatever page draws it.
    expect(radios(host)).toEqual(['sees-2', 'sees-2'])

    await go('/config/users/1')
    expect(heading(host)).toContain('davey')
    expect(radios(host)).toEqual(['sees-1', 'sees-1'])
  })

  it.each([
    [
      2,
      'sam would see every library, including any added later, and the ticks below would be cleared.',
    ],
    [3, 'kim would see every library, including any added later.'],
  ])(
    'asks before lifting account %i’s limit, in its own sentence',
    async (id, said) => {
      // kim is limited to no library, and lifting that is asked as well.
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          { id: 70, name: 'Manga', works: 0, books: 0, folders: [] },
          { id: 80, name: 'Comics', works: 0, books: 0, folders: [] },
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
      const host = await at(`/config/users/${id}`)
      await act(async () => {
        host
          .querySelector<HTMLInputElement>('.grants input[value="every"]')
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
    const host = await at('/config/users/1')
    // The server's sentence, in the whole alert, not the path and status.
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'The libraries list could not be read (the store is unavailable), so the grant editor below shows nothing rather than what there is.',
    )
    // And the page says the same thing where the editor would be, rather than
    // claiming there are no libraries — an empty editor and an unread one
    // look identical, and only one of them is true.
    expect(host.querySelector('.grants')?.textContent).toContain(
      'has not been read',
    )
    expect(host.querySelector('.grants')?.textContent).not.toContain(
      'no libraries yet',
    )
  })

  it('reads the libraries list again on a change of page, and drops the alert once a read lands', async () => {
    vi.mocked(librariesApi.listLibraries)
      .mockRejectedValueOnce(unreadLibraries())
      .mockResolvedValue({
        libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, folders: [] }],
      })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const host = await at('/config/users/2')
    expect(host.textContent).toContain('libraries list could not be read')
    expect(host.querySelector('.grants input[name="library-70"]')).toBeNull()

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/2')
    expect(host.textContent).not.toContain('libraries list could not be read')
    expect(
      host.querySelector('.grants input[name="library-70"]'),
    ).not.toBeNull()
  })

  it('draws the grant editor unread again when a read fails after one that landed', async () => {
    // The alert says the editor shows nothing, so it must not go on drawing
    // the list the earlier read brought.
    vi.mocked(librariesApi.listLibraries)
      .mockRejectedValueOnce(unreadLibraries())
      .mockResolvedValueOnce({
        libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, folders: [] }],
      })
      .mockResolvedValueOnce({
        libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, folders: [] }],
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
        ...host.querySelectorAll<HTMLInputElement>(
          '.grants input[type="checkbox"]',
        ),
      ].map((box) => box.name)
    const host = await at('/config/users/2')
    expect(host.textContent).toContain('libraries list could not be read')
    expect(boxes(host)).toEqual([])

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(host.textContent).not.toContain('libraries list could not be read')
    expect(boxes(host)).toEqual(['library-70'])

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/2')
    expect(host.textContent).toContain('libraries list could not be read')
    expect(boxes(host)).toEqual([])
    expect(host.querySelector('.grants .unknown')?.textContent).toContain(
      'has not been read',
    )
  })

  it('offers a library made since the screen mounted, once the page changes', async () => {
    const manga = { id: 70, name: 'Manga', works: 0, books: 0, folders: [] }
    const comics = { id: 80, name: 'Comics', works: 0, books: 0, folders: [] }
    vi.mocked(librariesApi.listLibraries)
      .mockResolvedValueOnce({ libraries: [manga] })
      .mockResolvedValue({ libraries: [manga, comics] })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user(),
        user({ id: 2, name: 'sam', role: 'user', every_library: false }),
      ],
    })
    const host = await at('/config/users/2')
    expect(
      host.querySelector('.grants input[name="library-70"]'),
    ).not.toBeNull()
    expect(host.querySelector('.grants input[name="library-80"]')).toBeNull()

    await follow(host, '‹ Users')
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/2')
    expect(
      host.querySelector('.grants input[name="library-80"]'),
    ).not.toBeNull()
  })

  it.each([
    ['the table', TABLE],
    ['the make page', NEW],
    ['the line for an unknown account', '/config/users/9'],
  ])(
    'says nothing of the libraries list on %s, which draws no grant editor',
    async (_, page) => {
      // The alert speaks of "the grant editor below", which only an account's
      // page draws.
      vi.mocked(librariesApi.listLibraries).mockRejectedValue(unreadLibraries())
      const host = await at(page)
      expect(host.querySelector('.grants')).toBeNull()
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
    const host = await at('/config/users/2')
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
      leave: (_host: HTMLElement) => go('/config/users/3'),
      to: '/config/users/3',
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
      if (to === '/config/users/3') expect(heading(host)).toBe('kim')
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
    await go('/config/users/3')
    const before = host.querySelector('select[name="role"]') as HTMLElement
    act(() => before.focus())
    // Nothing takes focus to have it read, so it lands in a live region,
    // which is drawn with the page, empty, before the answer comes: a region
    // drawn together with what it says is often not read.
    const region = host.querySelector('.user-row > [aria-live="polite"]')
    const emptyBefore = region?.childNodes.length
    await answer()
    expect(here()).toBe('/config/users/3')
    expect(heading(host)).toBe('kim')
    expect(region).not.toBeNull()
    expect(emptyBefore).toBe(0)
    expect(host.querySelector('.user-row > [aria-live="polite"]')).toBe(region)
    expect(region?.textContent).toBe('Removed sam.')
    expect(document.activeElement).toBe(before)
    expect(usersApi.listUsers).toHaveBeenCalledTimes(1)

    await go('/config/users/1')
    expect(host.textContent).not.toContain('Removed sam.')
    await follow(host, '‹ Users')
    expect(host.textContent).not.toContain('Removed sam.')
    // Not drawn again on the page it was drawn on, opened again.
    await go('/config/users/3')
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
    expect(here()).toBe('/config/users/2')
    await answer()
    expect(here()).toBe(TABLE)
    expect(host.querySelector('.user-table > p.gone-line')?.textContent).toBe(
      'Removed sam.',
    )
    expect(names(host)).toEqual(['davey', 'kim'])
  })
})

describe('Users, a write refused once its page has gone', () => {
  const SAM = '/config/users/2'
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
      said: 'May download whole editions for sam was not saved (the store is locked).',
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
      libraries: [{ id: 70, name: 'Manga', works: 0, books: 0, folders: [] }],
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
      leave: (_host: HTMLElement) => go('/config/users/3'),
      page: '.user-row',
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
      await go('/config/users/1')
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
    host.querySelector('.user-row > h2')

  async function makeSam(host: HTMLElement) {
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await flush()
  }

  it.each([
    {
      by: 'its row’s link',
      back: (host: HTMLElement) => follow(host, 'sam'),
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
      expect(here()).toBe('/config/users/2')
      expect(accountHeading(host)?.textContent).toBe('sam')
      expect(document.activeElement).toBe(accountHeading(host))

      await follow(host, '‹ Users')
      await back(host)
      expect(here()).toBe('/config/users/2')
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
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/2')
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
    const host = await at('/config/users/2')
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

    // The table opened again is opened by the person: no line, and nothing
    // takes focus.
    await follow(host, 'New account')
    await follow(host, '‹ Users')
    expect(here()).toBe(TABLE)
    expect(host.querySelector('.user-table > p.gone-line')).toBeNull()
    expect(document.activeElement).toBe(document.body)
  })

  it('puts focus on the line that says there is no such account', async () => {
    const host = await at('/config/users/9')
    const missing = host.querySelector('.missing')
    expect(missing?.textContent).toBe('No such account.')
    expect(document.activeElement).toBe(missing)
  })

  it('puts focus on the no-such-account line again when a second unknown id follows the first', async () => {
    const host = await at('/config/users/9')
    expect(document.activeElement).toBe(host.querySelector('.missing'))
    act(() => (document.activeElement as HTMLElement).blur())
    await go('/config/users/8')
    expect(here()).toBe('/config/users/8')
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
    const host = await at('/config/users/2')
    expect(host.querySelector('.missing')).toBeNull()
    await press(host.querySelector('input[name="can_download"]') as Element)
    await flush()
    expect(here()).toBe('/config/users/2')
    const missing = host.querySelector('.missing')
    expect(missing?.textContent).toBe('No such account.')
    expect(document.activeElement).toBe(missing)
  })

  it('puts focus on the table’s heading when the make form is cancelled', async () => {
    const host = await at(NEW)
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    const heading = host.querySelector('.user-table > h2')
    expect(heading?.textContent).toBe('Users')
    expect(document.activeElement).toBe(heading)
  })

  it('leaves focus where it falls on a page the person opened: by a row’s link, by the back link, and by Back', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, sam] })
    const host = await at()
    const row = link(host, 'sam')
    act(() => row.focus())
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/2')
    expect(document.activeElement).toBe(document.body)

    const back = link(host, '‹ Users')
    act(() => back.focus())
    await follow(host, '‹ Users')
    expect(here()).toBe(TABLE)
    expect(document.activeElement).toBe(document.body)

    await flush(() => steer?.(-1))
    await flush()
    expect(here()).toBe('/config/users/2')
    expect(document.activeElement).toBe(document.body)
  })

  it('leaves focus alone on the table the person goes back to from the made account’s page', async () => {
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
    expect(document.activeElement).toBe(document.body)
  })

  it('leaves focus alone on an account’s page opened from the table Cancel went to', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [davey, sam] })
    const host = await at(NEW)
    await press(button(host, 'Cancel'))
    await flush()
    expect(document.activeElement).toBe(host.querySelector('.user-table > h2'))
    const row = link(host, 'sam')
    act(() => row.focus())
    await follow(host, 'sam')
    expect(here()).toBe('/config/users/2')
    expect(document.activeElement).toBe(document.body)
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
    expect(document.activeElement).toBe(host.querySelector('.user-table > h2'))
    const elsewhere = link(host, 'New account')
    act(() => elsewhere.focus())
    await flush(() => held.resolve({ users: [davey, sam] }))
    expect(names(host)).toEqual(['davey', 'sam'])
    expect(document.activeElement).toBe(elsewhere)
  })
})

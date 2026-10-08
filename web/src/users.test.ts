import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  addUser,
  listUsers,
  onlyAdminWhoCanLogIn,
  patchUser,
  removalCost,
  removeUser,
  revokeUserKey,
  revokeUserSession,
  ROLES,
  type User,
} from './users'

const user = (over: Partial<User> = {}): User => ({
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

/**
 * A fresh `Response` per call, not one shared instance — `Response.json()`
 * consumes its body stream, so a second read of the same instance throws
 * "Body is unusable". `config.test.ts` and `library.test.ts` carry the same
 * note for the same reason.
 */
const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ROLES', () => {
  it('spells the two values the schema CHECK accepts', () => {
    // `004_auth.sql`'s `CHECK (role IN ('admin', 'user'))`. A third value here
    // is a 400 the add form would offer in a dropdown.
    expect([...ROLES]).toEqual(['user', 'admin'])
  })
})

describe('onlyAdminWhoCanLogIn', () => {
  it('is true for the sole administrator', () => {
    const sole = user({ id: 1, role: 'admin' })
    expect(
      onlyAdminWhoCanLogIn([sole, user({ id: 2, role: 'user' })], sole),
    ).toBe(true)
  })

  it('is false beside a second administrator who can log in', () => {
    const first = user({ id: 1, role: 'admin' })
    const second = user({ id: 2, name: 'sam', role: 'admin' })
    expect(onlyAdminWhoCanLogIn([first, second], first)).toBe(false)
  })

  it('is true beside a second administrator who cannot log in', () => {
    // The store asks `another_admin_can_log_in` — an account still holding
    // `credential::LOCKED` has the role and no way in, so it cannot be the
    // administrator this library is left with. A client that counted admins
    // would enable Remove on the one account that must not be removed, which
    // is the one direction §6.1 forbids: the client may never permit what the
    // server would refuse.
    const live = user({ id: 1, role: 'admin' })
    const locked = user({
      id: 2,
      name: 'sam',
      role: 'admin',
      can_log_in: false,
    })
    expect(onlyAdminWhoCanLogIn([live, locked], live)).toBe(true)
  })

  it('is false for a reader, even the only account in the library', () => {
    const reader = user({ id: 1, role: 'user' })
    expect(onlyAdminWhoCanLogIn([reader], reader)).toBe(false)
  })
})

describe('the users client', () => {
  it('asks for the users list', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json({ users: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await listUsers()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users')
  })

  it('posts the name, password and role, and unwraps the account', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ user: user({ id: 7, name: 'sam' }) }))
    vi.stubGlobal('fetch', fetchMock)
    const made = await addUser('sam', 'hunter2', 'user')
    expect(made.name).toBe('sam')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    expect(init.body).toBe(
      JSON.stringify({ name: 'sam', password: 'hunter2', role: 'user' }),
    )
  })

  it('patches with only the keys it was handed', async () => {
    // An absent key leaves the column alone and `libraries: []` grants nothing —
    // which spec A2 defines as unrestricted. Sending a whole row would rewrite
    // five columns to answer a question about one.
    const fetchMock = vi.fn().mockImplementation(() => json({ user: user() }))
    vi.stubGlobal('fetch', fetchMock)
    await patchUser(3, { can_download: false })
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users/3')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('PATCH')
    expect(init.body).toBe(JSON.stringify({ can_download: false }))
  })

  it('passes can_stamp through to the PATCH body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(json({ user: user() }))
    vi.stubGlobal('fetch', fetchMock)

    await patchUser(3, { can_stamp: true })

    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('PATCH')
    expect(init.body).toBe(JSON.stringify({ can_stamp: true }))
  })

  it('spells dry_run exactly, because the server refuses any other key', async () => {
    // `deny_unknown_fields` on `DeleteUserQuery`: `?dryrun=true` — one key off
    // — answered 200 and ran the real, committed removal on the roots route
    // before that attribute existed. This route is the second-most destructive
    // in the product, so the exact string is pinned rather than assumed.
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ removed: {}, dry_run: true }))
    vi.stubGlobal('fetch', fetchMock)
    await removalCost(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/users/3?dry_run=true',
    )
  })

  it('removes without the dry-run key at all', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ removed: {}, dry_run: false }))
    vi.stubGlobal('fetch', fetchMock)
    await removeUser(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users/3')
    expect(fetchMock.mock.calls[0]?.[0]).not.toContain('dry_run')
  })

  it('revokes a session under the account that holds it', async () => {
    // `account::revoke_session` is scoped to the account in SQL, and the path
    // is what carries the scope: an id from one account's listing must not end
    // another account's session.
    const fetchMock = vi
      .fn()
      .mockImplementation(() => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await revokeUserSession(4, 'a/b')
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/users/4/sessions/a%2Fb',
    )
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('DELETE')
  })

  it('revokes a key under the account that holds it', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await revokeUserKey(4, 9)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users/4/keys/9')
  })
})

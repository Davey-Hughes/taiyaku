import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { GateContext } from './gate'
import * as librariesApi from './libraries'
import { deferred, flush } from './testing/async'
import { fields } from './testing/fields'
import { mount } from './testing/mount'
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

const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

async function settle(signedInAs = who) {
  let host: HTMLElement | undefined
  await act(async () => {
    // A router because a card for the signed-in account links to `/account`
    // (O94), and that card is drawn whenever `signedInAs` is in the list.
    host = mount(
      <MemoryRouter>
        <GateContext value={{ who: signedInAs, signOut: () => undefined }}>
          <Users />
        </GateContext>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
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

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user()] })
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
})

describe('Users', () => {
  it('lists the accounts the server sent', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await settle()
    expect(host.querySelectorAll('li.user-row')).toHaveLength(2)
  })

  it('marks the card of the account this browser is signed in as', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await settle()
    const cards = [...host.querySelectorAll('li.user-row')]
    expect(cards[0]?.textContent).toContain('this is you')
    expect(cards[1]?.textContent).not.toContain('this is you')
  })

  it('posts the name, password and role, then reloads', async () => {
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledWith('sam', 'hunter2', 'user')
    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
  })

  it('draws the server sentence when a name is already taken', async () => {
    // C4 §4: no client-side uniqueness check. `user.name` is `UNIQUE` and the
    // route answers 400 with its own sentence; a second opinion here would be
    // a race with a store this browser does not own.
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error('/api/library/users: 400 an account named sam already exists'),
    )
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already exists',
    )
  })

  it('keeps that refusal when a background reload lands', async () => {
    // **PR A's review pass 2, one screen over.** One failure slot served both
    // the list and the add form, and `reload` — every row's `onChanged` —
    // cleared it, so a revocation in any card silently erased the one message
    // naming why an account could not be created.
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({
          sessions: [
            {
              id: 'aaa',
              created: '2026-09-01T10:00:00Z',
              last_seen: '2026-09-06T09:00:00Z',
              user_agent: 'Firefox',
              ip: '10.0.0.2',
              current: true,
            },
          ],
        }),
      ],
    })
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error('/api/library/users: 400 an account named sam already exists'),
    )
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)

    const revoke = host.querySelector(
      'button[name="revoke-session-aaa"]',
    ) as HTMLButtonElement
    await act(async () => {
      revoke.click()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
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
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'an account named sam already exists',
    )
  })

  it('draws the server sentence, not the path and status, for a list that was refused', async () => {
    vi.mocked(usersApi.listUsers).mockRejectedValue(
      new Error(
        '/api/library/users: 403 {"error":"an API key is not an administrator"}',
      ),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'an API key is not an administrator',
    )
  })

  it('offers each card the libraries, and names its radios for its own account', async () => {
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
    const host = await settle()
    const cards = [...host.querySelectorAll('li.user-row')]
    const granted = cards[1]?.querySelector<HTMLInputElement>(
      'input[name="library-70"]',
    )
    expect(granted?.checked).toBe(true)
    // One radio group per card: a shared name would make the two cards one.
    const radios = host.querySelectorAll<HTMLInputElement>(
      '.grants input[type="radio"]',
    )
    expect([...radios].map((radio) => radio.name)).toEqual([
      'sees-1',
      'sees-1',
      'sees-2',
      'sees-2',
    ])
  })

  it('gives each card its own question, each described by its own sentence', async () => {
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
    const host = await settle()
    const [sam, kim] = [...host.querySelectorAll('li.user-row')]
    const every = (card: Element | undefined) =>
      card?.querySelector<HTMLInputElement>('.grants input[value="every"]')
    await act(async () => {
      every(sam)?.click()
      every(kim)?.click()
      await Promise.resolve()
    })
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    const sentence = (card: Element | undefined) => {
      const id = card
        ?.querySelector('.lift-limit')
        ?.getAttribute('aria-describedby')
      return id ? document.getElementById(id) : null
    }
    expect(sentence(sam)?.textContent).toBe(
      'sam would see every library, including any added later, and the ticks below would be cleared.',
    )
    expect(sentence(kim)?.textContent).toBe(
      'kim would see every library, including any added later.',
    )
    expect(sam?.contains(sentence(sam) ?? null)).toBe(true)
    expect(kim?.contains(sentence(kim) ?? null)).toBe(true)
  })

  it('sends one POST for two submits dispatched before the first answers', async () => {
    // A ref rather than `disabled={adding}`: React batches state, so both
    // handlers read `adding === false`. Reachable by holding Enter in the name
    // box, and the second request trips `user.name UNIQUE` — a 400 rendered
    // over the account that was just created.
    vi.mocked(usersApi.addUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledTimes(1)
  })

  it('draws the reason the list could not be read', async () => {
    vi.mocked(usersApi.listUsers).mockRejectedValue(
      new Error('/api/library/users: 403 an API key is not an administrator'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'an API key is not an administrator',
    )
    expect(host.querySelector('li.user-row')).toBeNull()
  })

  it('creates an administrator when the add form says so', async () => {
    // The role the form sends is the one it was set to, not the default the
    // server would apply — `004_auth.sql` defaults to `user` precisely because
    // an omitted column minted an administrator.
    vi.mocked(usersApi.addUser).mockResolvedValue(
      user({ id: 2, name: 'sam', role: 'admin' }),
    )
    const host = await settle()
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
    // on the paragraph itself rather than the page, which also carries the
    // cards.
    // The backtick and `<code>` checks hold #112's finding, that this
    // paragraph once drew markdown backticks as themselves (`docs/notes.md`,
    // "What the pixels found that the tests did not").
    const host = await settle()
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

  it('sends nothing when either box is empty', async () => {
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await submitAdd(host)
    expect(usersApi.addUser).not.toHaveBeenCalled()
  })

  it('says so when the libraries list could not be read, rather than drawing an empty one', async () => {
    // The grant editor draws "There are no libraries yet" for an empty
    // array, and a failed read would make that sentence a lie about the
    // server, told on the control that decides what an account sees.
    vi.mocked(librariesApi.listLibraries).mockRejectedValue(
      new Error(
        '/api/library/libraries: 500 {"error":"the store is unavailable"}',
      ),
    )
    const host = await settle()
    // The server's sentence, in the whole alert, not the path and status.
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'The libraries list could not be read (the store is unavailable), so the grant editors below show nothing rather than what there is.',
    )
    // And the card says the same thing where the editor would be, rather than
    // claiming there are no libraries — an empty editor and an unread one
    // look identical, and only one of them is true.
    expect(host.querySelector('.grants')?.textContent).toContain(
      'has not been read',
    )
    expect(host.querySelector('.grants')?.textContent).not.toContain(
      'no libraries yet',
    )
  })
  // **The two `seq !== latest.current` guards, which no test had reached.**
  // `reload` is handed to every card as `onChanged` and is also called after
  // an add, so two list reads are routinely in flight at once — and the
  // server need not answer them in order. Without the sequence check the
  // *older* answer wins, and the screen settles on the list as it was before
  // the write that triggered the newer read.
  it('keeps the newer list when an older read answers after it', async () => {
    const first = deferred<{ users: usersApi.User[] }>()
    const second = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))

    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)

    await flush(() =>
      second.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    expect(host.querySelectorAll('li.user-row')).toHaveLength(2)

    // The mount read finally answers, with the list as it was before `sam`
    // existed. It is the stale one and must not be drawn.
    await flush(() => first.resolve({ users: [user()] }))
    expect(host.querySelectorAll('li.user-row')).toHaveLength(2)
  })

  it('keeps the newer list when an older read *fails* after it', async () => {
    // The catch needs its own guard for the same reason and does not get one
    // for free: a stale rejection would put a red alert over a list that was
    // read successfully a moment later, and the failure slot is the same one
    // the add form writes to.
    const first = deferred<{ users: usersApi.User[] }>()
    const second = deferred<{ users: usersApi.User[] }>()
    vi.mocked(usersApi.listUsers)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))

    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)

    await flush(() =>
      second.resolve({ users: [user(), user({ id: 2, name: 'sam' })] }),
    )
    await flush(() => first.reject(new Error('/api/library/users: 503 gone')))

    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelectorAll('li.user-row')).toHaveLength(2)
  })

  it('gives each add-form control its label’s words as its aria-label — O175', async () => {
    const host = await settle()
    // The form's own three, not the card above it: `UserRow.test.tsx` holds
    // the card's. Counted, because a loop over nothing passes.
    const labelled = fields(host.querySelectorAll('form.add-user label.field'))
    expect(labelled).toHaveLength(3)
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })
})

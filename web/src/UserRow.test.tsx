import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, type Mock, vi } from 'vitest'
import type { Root } from './config'
import { fields } from './testing/fields'
import { mount, rerender } from './testing/mount'
import { UserRow } from './UserRow'
import * as usersApi from './users'

vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
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
  roots: [],
  ...over,
})

const root = (over: Partial<Root> = {}): Root => ({
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

const sam = () => user({ id: 2, name: 'sam', role: 'admin' })

interface Mounted {
  host: HTMLElement
  changed: Mock
}

async function show(
  who: usersApi.User,
  everyone: usersApi.User[],
  over: {
    roots?: Root[] | null
    actingAs?: number
    onChanged?: Mock
  } = {},
): Promise<Mounted> {
  const changed = over.onChanged ?? vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    // A router because the self-reset refusal links to `/account` (O94).
    host = mount(
      <MemoryRouter>
        <UserRow
          user={who}
          users={everyone}
          // `null` asks for the unread state; absent means one ordinary root.
          roots={over.roots === null ? undefined : (over.roots ?? [root()])}
          actingAs={over.actingAs}
          onChanged={changed}
        />
      </MemoryRouter>,
    )
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, changed }
}

const control = <T extends Element>(host: HTMLElement, selector: string): T => {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

async function choose(host: HTMLElement, selector: string, value: string) {
  const el = control<HTMLSelectElement>(host, selector)
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set
    setter?.call(el, value)
    el.dispatchEvent(new Event('change', { bubbles: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

async function click(el: Element) {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  // **Answers for the account it was asked about.** A flat `user()` here is
  // davey, so since `applied` shipped every patch on sam's card turned it into
  // davey's — the name, the role, and with it the last-administrator sentence
  // — and every assertion was a whole-card `toContain` with no name in it, so
  // they all passed anyway. Third review pass; the repo's own rule is to
  // assert per element rather than per page.
  vi.mocked(usersApi.patchUser).mockImplementation((id, patch) =>
    Promise.resolve(
      user({
        id,
        name: id === 1 ? 'davey' : 'sam',
        role: id === 1 ? 'admin' : 'user',
        ...(patch.roots === undefined
          ? {}
          : { roots: patch.roots.map((r) => ({ id: r, path: `/srv/${r}` })) }),
      }),
    ),
  )
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
  vi.mocked(usersApi.revokeUserKey).mockResolvedValue(undefined)
})

describe('the two invariants on the last administrator', () => {
  it('disables both the role control and Remove, and says why', async () => {
    // §6.1: one invariant reached through two routes. A screen that disabled
    // only Remove leaves the role dropdown open on the identical accident.
    const sole = user()
    const { host } = await show(sole, [
      sole,
      user({ id: 2, name: 'reader', role: 'user' }),
    ])
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(true)
    expect(
      control<HTMLButtonElement>(host, 'button[name="remove"]').disabled,
    ).toBe(true)
    expect(host.textContent).toContain('only administrator who can log in')
  })

  it('leaves both live beside a second administrator who can log in', async () => {
    // The negative case, so the assertions above cannot pass against a card
    // that disables everything.
    const first = user()
    const { host } = await show(first, [first, sam()], { actingAs: 99 })
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(false)
    expect(
      control<HTMLButtonElement>(host, 'button[name="remove"]').disabled,
    ).toBe(false)
  })

  it('still disables the role control beside an administrator who cannot log in', async () => {
    const live = user()
    const locked = user({
      id: 2,
      name: 'sam',
      role: 'admin',
      can_log_in: false,
    })
    const { host } = await show(live, [live, locked], { actingAs: 99 })
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(true)
  })
})

describe('the actor rule', () => {
  it('will not let an administrator demote themselves, even beside another', async () => {
    // Fires about the actor rather than about the population — `acting_as` is
    // what carries it in the store, and this card is where a person meets it.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 1 })
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(true)
    expect(host.textContent).toContain('another administrator has to do it')
  })

  it('will not let an administrator reset their own password, and links to where to', async () => {
    // The fourth refusal, and the one whose advice names another screen. Until
    // O94 that screen had no URL, and this said "open any work and press A";
    // it links to `/account` now. Asserted on the refusal's own paragraph and
    // on the link as an element, so a sentence that merely spells out
    // `/account` does not pass for a way a person can follow.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 1 })
    expect(
      control<HTMLInputElement>(host, 'input[name="password"]').disabled,
    ).toBe(true)
    const why = control(host, 'form.reset .why')
    expect(why.querySelector('code')?.textContent).toBe('/api/password')
    expect(why.querySelector('a')?.getAttribute('href')).toBe('/account')
    expect(why.textContent).not.toContain('press A')
  })

  it("resets somebody else's password and says what that ends", async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host, changed } = await show(other, [me, other], { actingAs: 1 })
    const box = control<HTMLInputElement>(host, 'input[name="password"]')
    expect(box.disabled).toBe(false)
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'hunter2')
      box.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await click(control(host, 'button[name="reset"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
    expect(changed).toHaveBeenCalled()
    expect(host.textContent).toContain('API keys are untouched')
    // Per element: the card is still sam's after the answer it applied.
    expect(control(host, 'h3').textContent).toBe('sam')
  })
})

describe('the fields', () => {
  it('retires the password notice on the next write', async () => {
    // O96's `heldNotice` shape: a notice about one write standing over every
    // later one. The mutation that follows it is what retires it.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const box = control<HTMLInputElement>(host, 'input[name="password"]')
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'hunter2')
      box.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await click(control(host, 'button[name="reset"]'))
    expect(control(host, '.done').textContent).toBe(
      'Password set. sam is signed out everywhere.',
    )

    await click(control(host, 'input[name="can_download"]'))
    expect(host.textContent).not.toContain('is signed out everywhere')
  })

  it('patches only the capability column that changed', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'input[name="can_download"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { can_download: false })
  })

  it('sends the whole grant list when a root is ticked, because there is no per-root route', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      roots: [root({ id: 3 }), root({ id: 4, path: '/srv/raw' })],
    })
    await click(control(host, 'input[name="root-4"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { roots: [4] })
  })

  it('holds every control until the reload it triggered has landed', async () => {
    // **The window between a 200 and the list coming back.** `user` is a
    // prop, so inside it the card still draws its pre-patch grants with the
    // boxes live — and `toggleGrant` computes a *whole list* from those, so a
    // second tick there sends the new root alone and silently revokes the
    // first. Held until `onChanged` settles.
    let release: (() => void) | undefined
    const changed = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve
        }),
    )
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      roots: [root({ id: 3 }), root({ id: 4, path: '/srv/raw' })],
      onChanged: changed,
    })

    await click(control(host, 'input[name="root-3"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { roots: [3] })

    // **Held, but not greyed — O159.** Every checkbox on the card says it is
    // held to a screen reader and keeps its fill. `disabled` took the fill
    // away, so one tick flashed every box on the card.
    const boxes = [
      ...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ]
    expect(boxes.map((box) => box.name)).toEqual([
      'can_download',
      'can_stream',
      'can_stamp',
      'root-3',
      'root-4',
    ])
    for (const box of boxes) {
      expect(box.disabled, box.name).toBe(false)
      expect(box.getAttribute('aria-disabled'), box.name).toBe('true')
    }
    // The rest of the card keeps `disabled`. Its greying is not what O159
    // was about.
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(true)

    // **The lock is `begin`'s guard, and nothing stands in front of it
    // now.** A tick lands on a live box, reaches `save`, and `begin`
    // refuses it. This case alone cannot say the guard is the ref: by now
    // the card has rendered `saving`, so a guard reading that would pass
    // here too. The double-press DELETE cases below are what prove the ref.
    const second = control<HTMLInputElement>(host, 'input[name="root-4"]')
    const stream = control<HTMLInputElement>(host, 'input[name="can_stream"]')
    await act(async () => {
      second.click()
      stream.click()
      await Promise.resolve()
    })
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    // And each box shows what the card holds, not the refused tick.
    expect(second.checked).toBe(false)
    expect(stream.checked).toBe(true)

    await act(async () => {
      release?.()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(
      control<HTMLInputElement>(host, 'input[name="root-4"]').getAttribute(
        'aria-disabled',
      ),
    ).toBe('false')
  })

  it('computes the next grant list from the answer when the reload brings nothing', async () => {
    // **The hold's own hole, found by the second review pass.** `settled`
    // waits for the reload — but a reload that fails, or one superseded by
    // another card's, resolves without fresh props, so the card came back
    // live over pre-write values and the next tick computed a whole grant
    // list from them, silently revoking the first.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.patchUser).mockResolvedValue(
      user({
        id: 2,
        name: 'sam',
        role: 'user',
        roots: [{ id: 3, path: '/srv/manga' }],
      }),
    )
    // The parent answers, and hands back the same props it already had.
    const changed = vi.fn(() => Promise.resolve())
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      roots: [root({ id: 3 }), root({ id: 4, path: '/srv/raw' })],
      onChanged: changed,
    })

    await click(control(host, 'input[name="root-3"]'))
    expect(usersApi.patchUser).toHaveBeenNthCalledWith(1, 2, { roots: [3] })
    // Drawn from the answer, not from the props that never moved.
    expect(
      control<HTMLInputElement>(host, 'input[name="root-3"]').checked,
    ).toBe(true)

    await click(control(host, 'input[name="root-4"]'))
    expect(usersApi.patchUser).toHaveBeenNthCalledWith(2, 2, { roots: [3, 4] })
  })

  it('says the roots list has not been read, and names what is granted', async () => {
    // `roots ?? []` drew "this library has no roots yet" while the read was
    // in flight and for ever after one that failed — a positive false claim
    // about the library, on the control whose empty state means unrestricted.
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      roots: [{ id: 3, path: '/srv/manga' }],
    })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      roots: null,
    })
    const grants = control(host, '.grants').textContent ?? ''
    expect(grants).toContain('has not been read')
    expect(grants).toContain('/srv/manga')
    expect(grants).not.toContain('no roots yet')
    expect(host.querySelector('.grant-list')).toBeNull()
  })

  it('says that nothing ticked means unrestricted', async () => {
    // Counter-intuitive and load-bearing: an empty grant is every root, not
    // none. Spec A2, O54 — every account in every existing library holds one.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 99 })
    expect(host.textContent).toContain('unrestricted')
  })

  it('patches the role when the dropdown is live', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host, changed } = await show(other, [me, other], { actingAs: 1 })
    await choose(host, 'select[name="role"]', 'admin')
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { role: 'admin' })
    expect(changed).toHaveBeenCalled()
  })

  it('patches the streaming column on its own', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'input[name="can_stream"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { can_stream: false })
  })

  it('sends the grant list without the root that was unticked', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      roots: [
        { id: 3, path: '/srv/manga' },
        { id: 4, path: '/srv/raw' },
      ],
    })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      roots: [root({ id: 3 }), root({ id: 4, path: '/srv/raw' })],
    })
    await click(control(host, 'input[name="root-3"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { roots: [4] })
  })

  it('offers can_stamp as a third capability, and sends it on its own', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user', can_stamp: false })
    const { host } = await show(other, [me, other])

    // `control` throws when the selector matches nothing, so its return is
    // never null and a `not.toBeNull()` here could not fail. The claim worth
    // making is the state of the box it found.
    const box = control<HTMLInputElement>(host, 'input[name="can_stamp"]')
    expect(box.checked).toBe(false)

    await click(box)

    // On its own: a save that also resent `can_stream` would write a column
    // nobody touched, and an absent key is what the route reads as "leave
    // this alone".
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { can_stamp: true })
  })

  it('draws can_stamp unticked for an administrator who does not hold it', async () => {
    // **The one capability `role` does not dominate** — the documented O86
    // exception, because it decides whether an account may rewrite archives
    // taiyaku does not own rather than widening what it may read. A card that
    // drew this ticked because the account is an admin would state a
    // permission the server answers 403 to.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'admin', can_stamp: false })
    const { host } = await show(other, [me, other])

    expect(
      control<HTMLInputElement>(host, 'input[name="can_stamp"]').checked,
    ).toBe(false)
  })

  it('says the library has no roots rather than drawing an empty editor', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1, roots: [] })
    expect(host.textContent).toContain('This library has no roots yet')
  })

  it('draws the server sentence when a patch is refused', async () => {
    // The other half of §6.1: what the client could not tell in advance
    // arrives as a 409 with a sentence, and the card renders that.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'admin' })
    vi.mocked(usersApi.patchUser).mockRejectedValue(
      new Error(
        '/api/library/users/2: 409 demoting `sam` would leave this library with no administrator who can log in',
      ),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'input[name="can_download"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no administrator who can log in',
    )
  })

  it('names an account nobody can sign in as', async () => {
    const locked = user({ id: 2, name: 'sam', role: 'user', can_log_in: false })
    const { host } = await show(locked, [user(), locked], { actingAs: 1 })
    expect(host.textContent).toContain('Nobody can sign in as this account')
  })

  it('keeps the role explanation out of the field grid', async () => {
    // The same last-administrator scenario the invariant tests above use,
    // which is known to render `roleWhy` — the sentence this test checks the
    // placement of.
    const sole = user()
    const { host } = await show(sole, [
      sole,
      user({ id: 2, name: 'reader', role: 'user' }),
    ])
    const fields = host.querySelector('.user-row .fields')
    expect(fields).not.toBeNull()
    // `p.why` is a sentence, not a field: it must not be a child of the grid.
    expect(fields?.querySelector('p.why')).toBeNull()
    // It still renders — it moved, it did not go.
    expect(host.querySelector('.user-row p.why')).not.toBeNull()
  })

  it('groups the three permission checkboxes in a row of their own', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const checks = host.querySelector('.user-row .checks')
    expect(checks).not.toBeNull()
    expect(checks?.querySelectorAll('input[type="checkbox"]')).toHaveLength(3)
    // And none of them is left behind in the grid.
    expect(
      host.querySelectorAll('.user-row .fields input[type="checkbox"]'),
    ).toHaveLength(0)
  })

  it('gives the role label the field pattern rather than inline flow', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const role = host.querySelector('select[name="role"]')?.closest('label')
    expect(role?.className).toContain('field')
  })
})

describe('sessions and keys', () => {
  it('marks the browser doing the asking and revokes under this account', async () => {
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01T10:00:00Z',
          last_seen: '2026-09-06T09:00:00Z',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: true,
        },
        {
          id: 'bbb',
          created: '2026-08-01T10:00:00Z',
          last_seen: '2026-08-02T10:00:00Z',
          user_agent: null,
          ip: null,
          current: false,
        },
      ],
      keys: [
        {
          id: 9,
          name: 'the tablet',
          created: '2026-08-01T10:00:00Z',
          last_used: null,
        },
      ],
    })
    const { host, changed } = await show(other, [user(), other], {
      actingAs: 1,
    })

    // Asserted per element, not by `toContain` over the whole card: a
    // whole-page match cannot see which session carries the marker.
    const rows = [...host.querySelectorAll('.session-list > li')]
    expect(rows).toHaveLength(2)
    expect(rows[0]?.textContent).toContain('this browser')
    expect(rows[1]?.textContent).not.toContain('this browser')
    expect(rows[1]?.textContent).toContain('no user-agent recorded')

    await click(control(host, 'button[name="revoke-session-bbb"]'))
    expect(usersApi.revokeUserSession).toHaveBeenCalledWith(2, 'bbb')
    expect(changed).toHaveBeenCalled()

    // Read before the revocation: the row goes locally the moment the DELETE
    // answers, so this is the only moment it is there to read.
    expect(control(host, '.key-list').textContent).toContain('never used')
    await click(control(host, 'button[name="revoke-key-9"]'))
    expect(usersApi.revokeUserKey).toHaveBeenCalledWith(2, 9)
    expect(host.querySelector('.key-list')).toBeNull()
  })
})

describe('a revocation that fails', () => {
  it('draws the server sentence and reloads nothing', async () => {
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01T10:00:00Z',
          last_seen: '2026-09-06T09:00:00Z',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
      ],
      keys: [
        {
          id: 9,
          name: 'the tablet',
          created: '2026-08-01T10:00:00Z',
          last_used: '2026-09-05T08:00:00Z',
        },
      ],
    })
    vi.mocked(usersApi.revokeUserSession).mockRejectedValue(
      new Error(
        '/api/library/users/2/sessions/aaa: 404 no such session under that account',
      ),
    )
    const { host, changed } = await show(other, [user(), other], {
      actingAs: 1,
    })
    // The key's own row says when it was last used, rather than only the
    // never-used case the other test covers.
    expect(host.querySelector('.key-list')?.textContent).toContain(
      'last used 2026-09-05T08:00:00Z',
    )
    await click(control(host, 'button[name="revoke-session-aaa"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no such session',
    )
    expect(changed).not.toHaveBeenCalled()
  })
})

describe('a revocation whose reload brings nothing back', () => {
  it('drops the row it revoked, so no second press can 404 over it', async () => {
    // `settled` waits for the list read, and that read can fail or be
    // superseded without bringing fresh props — so the revoked session stayed
    // listed with a live Revoke, and its next press answered 404 for a session
    // already gone. Third review pass, inside the second's fix.
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01T10:00:00Z',
          last_seen: '2026-09-06T09:00:00Z',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
        {
          id: 'bbb',
          created: '2026-08-01T10:00:00Z',
          last_seen: '2026-08-02T10:00:00Z',
          user_agent: 'an old iPad',
          ip: null,
          current: false,
        },
      ],
    })
    // The parent answers and hands back the props it already had.
    const changed = vi.fn(() => Promise.resolve())
    const { host } = await show(other, [user(), other], {
      actingAs: 1,
      onChanged: changed,
    })
    await click(control(host, 'button[name="revoke-session-aaa"]'))

    expect(host.querySelector('button[name="revoke-session-aaa"]')).toBeNull()
    expect(
      host.querySelector('button[name="revoke-session-bbb"]'),
    ).not.toBeNull()
    expect([...host.querySelectorAll('.session-list > li')]).toHaveLength(1)
  })
})

describe('a revocation answering after a fresher list', () => {
  it('drops its row from what the card holds now, not from the click-time copy', async () => {
    // A list read landing during the `DELETE` brings a newer row; writing back
    // the snapshot the click was made from would drop whatever it brought —
    // a session opened since, or a role another administrator changed. Fourth
    // review pass.
    const session = (id: string, agent: string) => ({
      id,
      created: '2026-09-01T10:00:00Z',
      last_seen: '2026-09-06T09:00:00Z',
      user_agent: agent,
      ip: null,
      current: false,
    })
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [session('aaa', 'Firefox')],
    })
    let settleRevoke: (() => void) | undefined
    vi.mocked(usersApi.revokeUserSession).mockImplementation(
      () =>
        new Promise((resolve) => {
          settleRevoke = () => resolve(undefined)
        }),
    )
    const { host } = await show(other, [user(), other], { actingAs: 1 })
    await click(control(host, 'button[name="revoke-session-aaa"]'))

    // A newer list lands while the DELETE is still in flight.
    const fresher = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [session('aaa', 'Firefox'), session('ccc', 'a phone')],
    })
    await act(async () => {
      rerender(
        <MemoryRouter>
          <UserRow
            user={fresher}
            users={[user(), fresher]}
            roots={[root()]}
            actingAs={1}
            onChanged={vi.fn()}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
    })

    await act(async () => {
      settleRevoke?.()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('button[name="revoke-session-aaa"]')).toBeNull()
    expect(
      host.querySelector('button[name="revoke-session-ccc"]'),
    ).not.toBeNull()
  })
})

describe('a double-clicked revocation', () => {
  it('sends one DELETE, so no 404 lands over a revocation that worked', async () => {
    // The second `DELETE` finds no row, and `revoke_session` answers 404 for a
    // session that is not there — the same answer as for one that never
    // existed — so an unguarded card reported a failure for the thing it had
    // just done.
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01T10:00:00Z',
          last_seen: '2026-09-06T09:00:00Z',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
      ],
    })
    vi.mocked(usersApi.revokeUserSession).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [user(), other], { actingAs: 1 })
    const button = control<HTMLButtonElement>(
      host,
      'button[name="revoke-session-aaa"]',
    )
    await act(async () => {
      button.click()
      button.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(usersApi.revokeUserSession).toHaveBeenCalledTimes(1)
  })
})

describe('removal', () => {
  it('keeps a failed dry run visible when another write clears the card', async () => {
    // `begin()` clears the shared failure slot on every write, and the dialog
    // draws "working out…" while there is neither a cost nor a reason — so a
    // capability toggle after a failed dry run put the dialog back to
    // "working out" for ever, with nothing in flight. Third review pass.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockRejectedValue(
      new Error('/api/library/users/2?dry_run=true: 500 counting failed'),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    expect(host.textContent).toContain('counting failed')

    await click(control(host, 'input[name="can_stream"]'))
    expect(host.textContent).toContain('counting failed')
    expect(host.textContent).not.toContain('Working out what this would remove')
  })

  it('drops the tombstone when an account of that name exists again', async () => {
    // `Users` keys cards by name, so re-creating a removed name reuses this
    // card instance — and a tombstone that outlived its account drew the new
    // one as removed, with no controls, until a page reload. The recycled-id
    // hazard `Users.tsx` describes, reached through the name. Fourth pass.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const cost = {
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 0,
      },
      dry_run: true,
    }
    vi.mocked(usersApi.removalCost).mockResolvedValue(cost)
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      ...cost,
      dry_run: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'button[name="confirm-remove"]'))
    expect(host.querySelector('.gone')).not.toBeNull()

    // The add form made another `sam`; the list read brings it back.
    const remade = user({ id: 7, name: 'sam', role: 'user' })
    await act(async () => {
      rerender(
        <MemoryRouter>
          <UserRow
            user={remade}
            users={[me, remade]}
            roots={[root()]}
            actingAs={1}
            onChanged={vi.fn()}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
    })
    expect(host.querySelector('.gone')).toBeNull()
    expect(host.querySelector('select[name="role"]')).not.toBeNull()
  })

  it('keeps the newest dry run when a slower one answers after it', async () => {
    // Remove…, Cancel, Remove… again: two are in flight and nothing orders
    // them, so a slow first answer landing after a failed second drew five
    // counts and a live *Remove sam* under an alert saying the count could not
    // be taken. Fourth review pass.
    let settleFirst: ((answer: usersApi.RemovalAnswer) => void) | undefined
    vi.mocked(usersApi.removalCost)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            settleFirst = resolve
          }),
      )
      .mockRejectedValueOnce(
        new Error('/api/library/users/2?dry_run=true: 500 counting failed'),
      )
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'button[name="cancel-remove"]'))
    await click(control(host, 'button[name="remove"]'))
    expect(host.textContent).toContain('counting failed')

    await act(async () => {
      settleFirst?.({
        removed: {
          name: 'sam',
          positions: 9,
          notes: 9,
          sessions: 9,
          keys: 9,
          grants: 9,
        },
        dry_run: true,
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('button[name="confirm-remove"]')).toBeNull()
    expect(host.textContent).not.toContain('9 in-progress positions')
    expect(host.textContent).toContain('counting failed')
  })

  it('deadens the confirm while another write is in flight', async () => {
    // Every other control on the card is held while a write is in flight;
    // this one did not, so a press during a live patch was swallowed by the
    // ref guard with nothing on screen saying why — an enabled destructive
    // button that does nothing. Third review pass.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
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
    vi.mocked(usersApi.patchUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'input[name="can_stream"]'))
    expect(
      control<HTMLButtonElement>(host, 'button[name="confirm-remove"]')
        .disabled,
    ).toBe(true)
  })

  it('draws a removed account as gone rather than as an editable card', async () => {
    // The account is deleted and the reload that would drop the card failed.
    // Closing the dialog alone put the plain Remove… back over an id that
    // answers "no account with id 2", with every other control live beside it.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
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
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 0,
      },
      dry_run: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'button[name="confirm-remove"]'))

    expect(host.querySelector('.gone')?.textContent).toContain('Removed')
    expect(host.querySelector('button[name="remove"]')).toBeNull()
    expect(host.querySelector('select[name="role"]')).toBeNull()
    expect(host.querySelector('input[name="password"]')).toBeNull()
  })

  it('states all five counts and the name, and sends no DELETE until confirmed', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 4,
        notes: 1,
        sessions: 2,
        keys: 0,
        grants: 3,
      },
      dry_run: true,
    })
    // The real removal is mocked too: without it the confirm below calls
    // `.then` on `undefined` and the test passes while throwing.
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 4,
        notes: 1,
        sessions: 2,
        keys: 0,
        grants: 3,
      },
      dry_run: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))

    const counts = control(host, '.counts').textContent ?? ''
    expect(counts).toContain('sam')
    expect(counts).toContain('4 in-progress positions')
    expect(counts).toContain('1 note')
    expect(counts).toContain('2 open sessions')
    expect(counts).toContain('0 API keys')
    expect(counts).toContain('3 root grants')
    expect(usersApi.removeUser).not.toHaveBeenCalled()

    await click(control(host, 'button[name="confirm-remove"]'))
    expect(usersApi.removeUser).toHaveBeenCalledWith(2)
  })

  it('closes without sending anything when the dialog is cancelled', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 1,
        keys: 1,
        grants: 0,
      },
      dry_run: true,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    // The singular forms, on the one dialog where "1 open sessions" would be
    // read by somebody about to delete a person.
    expect(control(host, '.counts').textContent).toContain('1 open session,')
    await click(control(host, 'button[name="cancel-remove"]'))
    expect(host.querySelector('.remove-user')).toBeNull()
    expect(usersApi.removeUser).not.toHaveBeenCalled()
  })

  it('draws the refusal when the commit is refused, and stays open', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
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
    vi.mocked(usersApi.removeUser).mockRejectedValue(
      new Error(
        '/api/library/users/2: 409 removing `sam` would leave this library with no administrator who can log in',
      ),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'button[name="confirm-remove"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no administrator who can log in',
    )
    expect(host.querySelector('button[name="confirm-remove"]')).not.toBeNull()
  })

  it('sends one DELETE for two presses before the first answers', async () => {
    // A ref rather than `disabled={removing}`: React batches state, so two
    // presses dispatched before the next render both read `false`. This is the
    // button where an unguarded second dispatch is least worth leaving to
    // luck.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
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
    vi.mocked(usersApi.removeUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    const confirm = control<HTMLButtonElement>(
      host,
      'button[name="confirm-remove"]',
    )
    // **Both presses inside one `act`, before React re-renders.** That is the
    // window the ref guards and `disabled={removing}` does not: a second press
    // *after* a render lands on a disabled button and would pass against a
    // card with no guard at all. Checked by hand-applying that defect.
    await act(async () => {
      confirm.click()
      confirm.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(usersApi.removeUser).toHaveBeenCalledTimes(1)
  })

  it('marks the confirm button and not Cancel, before the cost has loaded', async () => {
    // The whole reason this is a class and not `:first-of-type`: until the
    // cost resolves, Cancel is the only button in the block. Both removal
    // blocks render their confirm button only after the cost resolves, so
    // `:first-of-type` would paint Cancel in danger red on the loading state.
    let land: ((answer: usersApi.RemovalAnswer) => void) | undefined
    vi.mocked(usersApi.removalCost).mockReturnValue(
      new Promise<usersApi.RemovalAnswer>((resolve) => {
        land = resolve
      }),
    )
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(control(host, 'button[name="remove"]'))

    const block = control(host, '.remove-user')
    expect(block.querySelectorAll('button')).toHaveLength(1)
    expect(block.querySelector('.confirm')).toBeNull()

    await act(async () => {
      land?.({
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
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(block.querySelector('.confirm')?.getAttribute('name')).toBe(
      'confirm-remove',
    )
    expect(
      control<HTMLButtonElement>(
        host,
        'button[name="cancel-remove"]',
      ).classList.contains('confirm'),
    ).toBe(false)
  })

  it('closes its own dialog rather than trusting the card to be unmounted', async () => {
    // If the `DELETE` works and the reload that follows it fails, the account
    // is gone and the card is still on screen — with a live *Remove sam* over
    // it, whose next press answers "no account with id 2". Second review pass.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
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
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 0,
      },
      dry_run: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'button[name="confirm-remove"]'))
    expect(host.querySelector('.remove-user')).toBeNull()
    expect(host.querySelector('button[name="confirm-remove"]')).toBeNull()
  })

  it('offers no confirm button while the counts are unknown', async () => {
    // The one failure this dialog exists to prevent: a button that removes an
    // account while the sentence above it says nothing. "The dry run failed"
    // is exactly when a person presses on regardless.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockRejectedValue(
      new Error('/api/library/users/2?dry_run=true: 500 counting failed'),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    expect(host.querySelector('button[name="confirm-remove"]')).toBeNull()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'counting failed',
    )
  })
})

describe('the permission boxes — O154', () => {
  it('are one group named Permissions, holding exactly the three', async () => {
    const { host } = await show(sam(), [user(), sam()])
    const group = host.querySelector('[role="group"][aria-label="Permissions"]')
    if (!group) throw new Error('no Permissions group')
    // By name, so a grant box wandering in, or a permission wandering out,
    // fails here rather than passing a count.
    expect(
      [...group.querySelectorAll('input[type="checkbox"]')].map((box) =>
        box.getAttribute('name'),
      ),
    ).toEqual(['can_download', 'can_stream', 'can_stamp'])
  })
})

describe('the field labels — O175', () => {
  it('gives each control under a field label that label’s words as its aria-label', async () => {
    const { host } = await show(sam(), [user(), sam()])
    const labelled = fields(host.querySelectorAll('.user-row label.field'))
    // Role and the new password. Counted, because a loop over nothing passes.
    expect(labelled).toHaveLength(2)
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })
})

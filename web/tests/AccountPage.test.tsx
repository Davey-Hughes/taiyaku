import { act } from 'react'
import { MemoryRouter } from 'react-router'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  type Mock,
  vi,
} from 'vitest'
import { Account } from '../src/Account'
import { AccountPage } from '../src/AccountPage'
import type { Library } from '../src/libraries'
import { ABOUT } from './support/about'
import { deferred, flush } from './support/async'
import { holdOf } from './support/held'
import { submit, typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { expectRowsWired, helpOf, row, section } from './support/settingsRows'
import {
  holdTheClock,
  pass,
  sentenceDrawn,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'
import * as usersApi from '../src/users'

vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
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

const lib = (id: number, name: string, paths: string[]): Library => ({
  id,
  name,
  works: 0,
  books: 0,
  dirs: paths.map((path, i) => ({
    id: id * 10 + i,
    path,
    edition_key: null,
    default_kind: null,
    default_language: null,
    exclude_globs: null,
    scan_interval_secs: null,
    keeps_documents: false,
    last_scan_at: null,
    last_scan_status: null,
    next_scan_at: null,
    libraries: [{ id, name }],
    nested: [],
    inside: [],
  })),
})

const MANGA = lib(70, 'Manga', [
  '/media/Manga/Volumes',
  '/media/Manga/Chapters',
])
const COMICS = lib(80, 'Comics', ['/media/Comics'])

const sam = () => user({ id: 2, name: 'sam', role: 'admin' })

interface Mounted {
  host: HTMLElement
  changed: Mock
}

async function show(
  who: usersApi.User,
  everyone: usersApi.User[],
  over: {
    libraries?: Library[] | null
    actingAs?: number
    onChanged?: Mock
    onLateRefusal?: Mock
  } = {},
): Promise<Mounted> {
  const changed = over.onChanged ?? vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    // A router because the self-reset refusal links to `/account` (O94).
    host = mount(
      <MemoryRouter>
        <AccountPage
          user={who}
          users={everyone}
          // `null` asks for the unread state; absent means the two libraries.
          libraries={
            over.libraries === null
              ? undefined
              : (over.libraries ?? [MANGA, COMICS])
          }
          actingAs={over.actingAs}
          onChanged={changed}
          onLateRefusal={over.onLateRefusal}
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

/** The password form's Confirm password, typed into wherever a set is sent. */
const confirmBox = (host: HTMLElement) =>
  control<HTMLInputElement>(host, 'input[name="confirm-password"]')

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

/** The Libraries section, which holds the grants. */
const grants = (host: HTMLElement) => section(host, 'Libraries')

const radio = (host: HTMLElement, value: 'every' | 'only') =>
  control<HTMLInputElement>(
    grants(host),
    `input[type="radio"][value="${value}"]`,
  )
/** The sentence naming grants whose library the list does not hold. */
const unlisted = (host: HTMLElement) =>
  grants(host).querySelector('[data-field="unlisted-grants"]')
/** The sentence saying a limited account holds no grant at all. */
const seesNone = (host: HTMLElement) =>
  grants(host).querySelector('[data-field="no-library"]')
const tick = (host: HTMLElement, id: number) =>
  control<HTMLInputElement>(host, `input[name="library-${id}"]`)

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
  // davey, and the page draws a patch's answer (`applied`), so a patch on
  // sam's page answered with it would turn the page into davey's: the name,
  // the role, and with it the last-administrator sentence.
  vi.mocked(usersApi.patchUser).mockImplementation((id, patch) =>
    Promise.resolve(
      user({
        id,
        name: id === 1 ? 'davey' : 'sam',
        role: id === 1 ? 'admin' : 'user',
        // The server's two answers to a grant write: lifting deletes the
        // rows, and a list, empty or not, is a limit.
        ...(patch.every_library === true
          ? { every_library: true, libraries: [] }
          : {}),
        ...(patch.libraries === undefined
          ? {}
          : {
              every_library: false,
              libraries: patch.libraries.map((l) => ({
                id: l,
                name: `library ${l}`,
              })),
            }),
      }),
    ),
  )
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
  vi.mocked(usersApi.revokeUserKey).mockResolvedValue(undefined)
  // `clearAllMocks` keeps an implementation, so a dry run one case made fail
  // would fail the next case's too.
  vi.mocked(usersApi.removalCost).mockReset()
})

describe('the page before its account is known', () => {
  /**
   * The page at an address spelling `SAM`, as the users screen draws it
   * while its lists are read (`user` absent) and once they have found the
   * account.
   */
  const at = (over: { user?: usersApi.User; loadingDrawn?: boolean }) => (
    <MemoryRouter initialEntries={['/config/users/SAM']}>
      <AccountPage
        user={over.user}
        users={over.user === undefined ? undefined : [user(), over.user]}
        libraries={[MANGA, COMICS]}
        loadingDrawn={over.loadingDrawn}
        actingAs={1}
        notices={<p className="screen-notice">From the screen.</p>}
        onChanged={vi.fn()}
      />
    </MemoryRouter>
  )
  const title = (host: HTMLElement) =>
    host.querySelector('.account-page > .page-header .page-title')?.textContent
  const sections = (host: HTMLElement) =>
    host.querySelectorAll('.account-page section.settings-section').length

  it('heads itself with the name its address spells, draws the screen’s notices, and Loading… only once its wait is drawn, with no body', () => {
    const host = mount(at({}))
    expect(title(host)).toBe('SAM')
    expect(
      host.querySelector('.account-page > .screen-notice')?.textContent,
    ).toBe('From the screen.')
    expect(host.querySelector('.account-page > p[role="status"]')).toBeNull()
    expect(sections(host)).toBe(0)

    rerender(at({ loadingDrawn: true }))
    expect(
      host.querySelector('.account-page > p[role="status"]')?.textContent,
    ).toBe('Loading…')
    expect(sections(host)).toBe(0)
  })

  it('keeps its header, its notices and focus on ‹ Users when the account it waited for is drawn, and heads itself with the account’s own name', () => {
    const host = mount(at({ loadingDrawn: true }))
    const back = [...host.querySelectorAll('a')].find(
      (a) => a.textContent === '‹ Users',
    ) as HTMLAnchorElement
    const notice = host.querySelector('.screen-notice')
    act(() => back.focus())

    rerender(at({ user: user({ id: 2, name: 'sam', role: 'user' }) }))
    expect(title(host)).toBe('sam')
    expect(sections(host)).toBeGreaterThan(0)
    expect(host.querySelector('.account-page > p[role="status"]')).toBeNull()
    expect(
      [...host.querySelectorAll('a')].find((a) => a.textContent === '‹ Users'),
    ).toBe(back)
    expect(document.activeElement).toBe(back)
    expect(host.querySelector('.screen-notice')).toBe(notice)
  })
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
    // One reason in two sentences, each beside the control it holds.
    expect(helpOf(row(host, 'Role'))?.textContent).toBe(
      'davey is the only administrator who can log in, so the role cannot change. Give another account the admin role first.',
    )
    expect(helpOf(row(host, 'Remove this account'))?.textContent).toBe(
      'davey is the only administrator who can log in, so it cannot be removed. Give another account the admin role first.',
    )
  })

  it('leaves both live beside a second administrator who can log in', async () => {
    // The negative case, so the assertions above cannot pass against a page
    // that disables everything.
    const first = user()
    const { host } = await show(first, [first, sam()], { actingAs: 99 })
    expect(
      [
        control<HTMLSelectElement>(host, 'select[name="role"]'),
        control<HTMLButtonElement>(host, 'button[name="remove"]'),
      ].map(holdOf),
    ).toEqual(['free', 'free'])
    expect(helpOf(row(host, 'Role'))).toBeNull()
    expect(helpOf(row(host, 'Remove this account'))?.textContent).toBe(
      'Asks first, with what it would take.',
    )
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
    // what carries it in the store, and this page is where a person meets it.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 1 })
    expect(
      control<HTMLSelectElement>(host, 'select[name="role"]').disabled,
    ).toBe(true)
    expect(helpOf(row(host, 'Role'))?.textContent).toBe(
      'You cannot take away your own admin role; another administrator has to.',
    )
  })

  it('will not let an administrator reset their own password, and links to where to', async () => {
    // The fourth refusal, and the one whose advice names another screen: it
    // links to `/account` (O94). Asserted on the refusal's own paragraph and
    // on the link as an element, so a sentence that merely spells out
    // `/account` does not pass for a way a person can follow.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 1 })
    expect(
      ['input[name="password"]', 'input[name="confirm-password"]'].map(
        (box) => control<HTMLInputElement>(host, box).disabled,
      ),
    ).toEqual([true, true])
    const why = helpOf(row(host, 'New password'))
    if (!why) throw new Error('no help beside New password')
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
    await typeInto(confirmBox(host), 'hunter2')
    await click(control(host, 'button[name="reset"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
    expect(changed).toHaveBeenCalled()
    expect(helpOf(row(host, 'New password'))?.textContent).toBe(
      'Ends every session sam has open. API keys are untouched: they are separate credentials, revoked on their own.',
    )
    // Per element: the page is still sam's after the answer it applied.
    expect(control(host, 'h2').textContent).toBe('sam')
  })
})

describe('the fields', () => {
  it('puts focus in the password’s box when a set pressed on Set password works and empties it', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const box = control<HTMLInputElement>(host, 'input[name="password"]')
    const set = control<HTMLButtonElement>(host, 'button[name="reset"]')
    await typeInto(box, 'hunter2')
    await typeInto(confirmBox(host), 'hunter2')
    act(() => set.focus())
    await click(set)
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
    expect(control(host, '.done').textContent).toBe(
      'Password set. sam is signed out everywhere.',
    )
    expect([box.value, holdOf(set)]).toEqual(['', 'disabled'])
    expect(document.activeElement).toBe(box)
  })

  it('says a password was set in a polite live region drawn with the form, empty until then', async () => {
    // A region drawn together with what it says is often not read at all.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const region = host.querySelector('form.reset [aria-live="polite"]')
    expect(region?.textContent).toBe('')
    await typeInto(
      control<HTMLInputElement>(host, 'input[name="password"]'),
      'hunter2',
    )
    await typeInto(confirmBox(host), 'hunter2')
    await click(control(host, 'button[name="reset"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
    expect(host.querySelector('form.reset [aria-live="polite"]')).toBe(region)
    expect(region?.textContent).toBe(
      'Password set. sam is signed out everywhere.',
    )
  })

  it('leaves focus where it is when a set that empties the box came from elsewhere than Set password', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const box = control<HTMLInputElement>(host, 'input[name="password"]')
    const role = control<HTMLSelectElement>(host, 'select[name="role"]')
    await typeInto(box, 'hunter2')
    await typeInto(confirmBox(host), 'hunter2')
    act(() => role.focus())
    await submit(control(host, 'form.reset'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
    expect(box.value).toBe('')
    expect(document.activeElement).toBe(role)
  })

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
    await typeInto(confirmBox(host), 'hunter2')
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

  it('sends the whole list when a library is ticked', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    vi.mocked(usersApi.patchUser).mockResolvedValue({
      ...other,
      libraries: [
        { id: 70, name: 'Manga' },
        { id: 80, name: 'Comics' },
      ],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [70, 80] })
  })

  it('holds every control until the reload it triggered has landed', async () => {
    // **The window between a 200 and the list coming back.** `user` is a
    // prop, so inside it the page still draws its pre-patch grants with the
    // boxes live — and `toggleGrant` computes a *whole list* from those, so a
    // second tick there sends the new library alone and silently revokes the
    // first. Held until `onChanged` settles.
    holdTheClock()
    let release: (() => void) | undefined
    const changed = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve
        }),
    )
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      libraries: [MANGA, COMICS, lib(90, 'Raws', ['/media/Raws'])],
      onChanged: changed,
    })

    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [70, 80] })

    // **Held, but not greyed — O159.** Every checkbox on the page says it is
    // held to a screen reader and keeps its fill. `disabled` would take the
    // fill away, so one tick would flash every box on the page.
    const boxes = [
      ...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ]
    expect(boxes.map((box) => box.name)).toEqual([
      'can_download',
      'can_stream',
      'can_stamp',
      'library-70',
      'library-80',
      'library-90',
    ])
    for (const box of boxes) {
      expect(box.disabled, box.name).toBe(false)
      expect(box.getAttribute('aria-disabled'), box.name).toBe('true')
    }
    // The rest of the page is drawn held once the write has gone on long
    // enough. Its greying is not what O159 was about.
    pass(WAIT_DRAWN_AFTER_MS)
    expect(holdOf(control(host, 'select[name="role"]'))).toBe('held')

    // **The lock is `begin`'s guard, and nothing stands in front of it.** A
    // tick lands on a live box, reaches `save`, and `begin` refuses it. This
    // case alone cannot say the guard is the ref: by now the page has rendered
    // `saving`, so a guard reading that would pass here too. The double-press
    // DELETE cases below are what prove the ref.
    const second = tick(host, 90)
    const stream = control<HTMLInputElement>(host, 'input[name="can_stream"]')
    await act(async () => {
      second.click()
      stream.click()
      await Promise.resolve()
    })
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    // And each box shows what the page holds, not the refused tick.
    expect(second.checked).toBe(false)
    expect(stream.checked).toBe(true)

    // The radios are pressed mid-save too, and each is marked held. That the
    // guards in front of them are what keep them still is held by the cases
    // below that press them within one `act`; here `begin` would refuse a
    // write anyway.
    await act(async () => {
      radio(host, 'every').click()
      radio(host, 'only').click()
      await Promise.resolve()
    })
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(radio(host, 'only').checked).toBe(true)
    expect(radio(host, 'every').checked).toBe(false)
    expect(radio(host, 'every').getAttribute('aria-disabled')).toBe('true')
    expect(radio(host, 'only').getAttribute('aria-disabled')).toBe('true')

    await act(async () => {
      release?.()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(tick(host, 90).getAttribute('aria-disabled')).toBe('false')
    expect(radio(host, 'every').getAttribute('aria-disabled')).toBe('false')
  })

  it('computes the next grant list from the answer when the reload brings nothing', async () => {
    // `settled` waits for the reload, but a reload that fails, or one
    // superseded by a later one, resolves without fresh props. A page that
    // came back live over the values from before the write would compute
    // the next tick's whole grant list from them, silently revoking the
    // first.
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    vi.mocked(usersApi.patchUser).mockResolvedValue(
      user({
        id: 2,
        name: 'sam',
        role: 'user',
        every_library: false,
        libraries: [
          { id: 70, name: 'Manga' },
          { id: 80, name: 'Comics' },
        ],
      }),
    )
    // The parent answers, and hands back the same props it already had.
    const changed = vi.fn(() => Promise.resolve())
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      onChanged: changed,
    })

    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenNthCalledWith(1, 2, {
      libraries: [70, 80],
    })
    // Drawn from the answer, not from the props that never moved.
    expect(tick(host, 80).checked).toBe(true)

    await click(tick(host, 70))
    expect(usersApi.patchUser).toHaveBeenNthCalledWith(2, 2, {
      libraries: [80],
    })
  })

  it('says the libraries list has not been read, and names what is granted', async () => {
    // An unread list must not be drawn as an empty one: "there are no
    // libraries yet" would be a positive false claim about the server.
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      libraries: null,
    })
    const said = grants(host).textContent ?? ''
    expect(said).toContain('has not been read')
    expect(said).toContain('Manga')
    expect(said).not.toContain('no libraries yet')
    expect(grants(host).querySelector('input')).toBeNull()
  })

  // With the list unread this sentence is the only place the page says what
  // an account without a grant sees, and the two such accounts see opposite
  // things.
  it.each([
    [true, 'This account sees every library.'],
    [false, 'This account sees no library.'],
  ])(
    'says what an unread list leaves an account with no grant seeing, every_library %s',
    async (every, sees) => {
      const me = user()
      const other = user({
        id: 2,
        name: 'sam',
        role: 'user',
        every_library: every,
      })
      const { host } = await show(other, [me, other], {
        actingAs: 1,
        libraries: null,
      })
      expect(control(grants(host), 'p.unknown').textContent).toBe(
        `The libraries list has not been read, so there is nothing to offer here. ${sees}`,
      )
    },
  )

  it('draws an account that sees every library as Every library, with no boxes', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(control(grants(host), 'legend').textContent).toBe(
      'Libraries this account may see',
    )
    expect(radio(host, 'every').checked).toBe(true)
    expect(radio(host, 'only').checked).toBe(false)
    expect(host.querySelector('.grant-list')).toBeNull()
    expect(seesNone(host)).toBeNull()
  })

  it('draws an account limited to no library as Only these, every box clear, and says it sees none', async () => {
    // The radio follows `every_library`, never the list's length: the two
    // accounts hold the same empty list and see opposite things.
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(radio(host, 'only').checked).toBe(true)
    expect(radio(host, 'every').checked).toBe(false)
    expect(tick(host, 70).checked).toBe(false)
    expect(tick(host, 80).checked).toBe(false)
    expect(seesNone(host)?.textContent).toBe('This account sees no library.')
  })

  // An administrator sees every library whatever its rows say, so it is
  // never the account left with none — read, unread or empty list alike.
  it.each([
    ['read', [MANGA, COMICS]],
    ['unread', null],
    ['empty', []],
  ] as const)(
    'never says an administrator limited to no library sees none, with the list %s',
    async (_, libraries) => {
      const me = user()
      const admin = user({
        id: 2,
        name: 'sam',
        role: 'admin',
        every_library: false,
      })
      const { host } = await show(admin, [me, admin], {
        actingAs: 1,
        libraries: libraries === null ? null : [...libraries],
      })
      expect(grants(host).textContent).not.toContain('sees no library')
    },
  )

  it('tells a no-library account from an every-library one on a server with no library', async () => {
    // Removing the server's only library leaves the accounts limited to it
    // seeing nothing, while an every-library account will see the next one.
    const me = user()
    const none = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
    })
    const limited = await show(none, [me, none], {
      actingAs: 1,
      libraries: [],
    })
    expect(control(grants(limited.host), '.empty').textContent).toBe(
      'There are no libraries yet.',
    )
    expect(seesNone(limited.host)?.textContent).toBe(
      'This account sees no library.',
    )

    const every = user({ id: 3, name: 'kim', role: 'user' })
    const open = await show(every, [me, every], { actingAs: 1, libraries: [] })
    expect(seesNone(open.host)).toBeNull()
  })

  it('words its two choices, and names them for this account alone', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(radio(host, 'every').closest('label')?.textContent).toBe(
      'Every library, including any added later',
    )
    expect(radio(host, 'only').closest('label')?.textContent).toBe('Only these')
    // The account's id keeps the group's name unique. One account is drawn
    // to a page, so no other group there shares it.
    expect(radio(host, 'every').name).toBe('sees-2')
    expect(radio(host, 'only').name).toBe('sees-2')
  })

  it('draws a granted account as Only these, a box per library with its directories', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(radio(host, 'only').checked).toBe(true)
    expect(tick(host, 70).checked).toBe(true)
    expect(tick(host, 80).checked).toBe(false)
    const manga = tick(host, 70).closest('li') as HTMLElement
    expect(
      [...(tick(host, 70).labels ?? [])].map((l) => l.textContent),
    ).toEqual(['Manga'])
    expect(
      [...manga.querySelectorAll('.grant-dirs code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    const comics = tick(host, 80).closest('li') as HTMLElement
    expect(
      [...(tick(host, 80).labels ?? [])].map((l) => l.textContent),
    ).toEqual(['Comics'])
    expect(
      [...comics.querySelectorAll('.grant-dirs code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/Comics'])
  })

  // O193: a library of two directories is one box, and unticking it revokes it.
  it('revokes a library of two directories by its one box', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [
        { id: 70, name: 'Manga' },
        { id: 80, name: 'Comics' },
      ],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(tick(host, 70))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [80] })
  })

  it('clears the last tick, and the account then sees no library', async () => {
    // Clearing it narrows the account to nothing, which is the safe
    // direction: it widens nothing, so no tick is held back.
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(tick(host, 70).getAttribute('aria-disabled')).toBe('false')
    // Described by its library's directories alone: no reason holds it.
    const dirs = helpOf(tick(host, 70).closest('.settings-row') as HTMLElement)
    expect(dirs?.id).toBeTruthy()
    expect(tick(host, 70).getAttribute('aria-describedby')).toBe(dirs?.id)
    expect(
      [...(dirs?.querySelectorAll('code') ?? [])].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    expect(seesNone(host)).toBeNull()

    await click(tick(host, 70))

    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [] })
    expect(tick(host, 70).checked).toBe(false)
    expect(radio(host, 'only').checked).toBe(true)
    expect(seesNone(host)?.textContent).toBe('This account sees no library.')
  })

  it('takes the no-library sentence away when a library is ticked', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(seesNone(host)).not.toBeNull()

    await click(tick(host, 80))

    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [80] })
    expect(tick(host, 80).checked).toBe(true)
    expect(seesNone(host)).toBeNull()
  })

  it('names a grant whose library the list does not hold, and says it is the only one, without saying the account sees none', async () => {
    const me = user()
    const ghost = { id: 99, name: 'Gone' }
    const alone = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [ghost],
    })
    const { host } = await show(alone, [me, alone], { actingAs: 1 })
    // No box is drawn for it, so it is named where it can be read.
    expect(host.querySelector('input[name="library-99"]')).toBeNull()
    expect(unlisted(host)?.textContent).toBe(
      'This account is granted Gone, which the list does not hold.',
    )
    // Nothing is ticked, yet the account sees Gone: the sentence follows the
    // grants, not the boxes drawn.
    expect(seesNone(host)).toBeNull()
    expect(radio(host, 'only').checked).toBe(true)
  })

  it('names only the grants the list does not hold, beside the ones it does', async () => {
    const me = user()
    const mixed = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [
        { id: 70, name: 'Manga' },
        { id: 98, name: 'Gone' },
        { id: 99, name: 'Lost' },
      ],
    })
    const { host } = await show(mixed, [me, mixed], { actingAs: 1 })
    expect(unlisted(host)?.textContent).toBe(
      'Also granted, though the list does not hold them: Gone and Lost.',
    )
    expect(tick(host, 70).checked).toBe(true)
  })

  it.each([
    [['Gone', 'Lost'], 'Gone and Lost'],
    [['Gone', 'Lost', 'Found'], 'Gone, Lost and Found'],
  ])(
    'joins several unlisted grants, %j, with "and", so that no pair reads as one name',
    async (names, joined) => {
      const me = user()
      const sam = user({
        id: 2,
        name: 'sam',
        role: 'user',
        every_library: false,
        libraries: names.map((name, i) => ({ id: 90 + i, name })),
      })
      const { host } = await show(sam, [me, sam], { actingAs: 1 })
      expect(unlisted(host)?.textContent).toBe(
        `This account is granted ${joined}, which the list does not hold.`,
      )
    },
  )

  it('names a grant in a list that was read and holds no library', async () => {
    const me = user()
    const alone = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 99, name: 'Gone' }],
    })
    const { host } = await show(alone, [me, alone], {
      actingAs: 1,
      libraries: [],
    })
    expect(control(grants(host), '.empty').textContent).toBe(
      'There are no libraries yet.',
    )
    expect(unlisted(host)?.textContent).toBe(
      'This account is granted Gone, which the list does not hold.',
    )
  })

  it('draws no line about unlisted grants for an account whose grants are all listed', async () => {
    const me = user()
    const held = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(held, [me, held], { actingAs: 1 })
    expect(unlisted(host)).toBeNull()
  })

  it('matches a grant to a library by id, whatever either is called', async () => {
    const me = user()
    const renamed = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga, before it was renamed' }],
    })
    const { host } = await show(renamed, [me, renamed], { actingAs: 1 })
    expect(unlisted(host)).toBeNull()
    expect(tick(host, 70).checked).toBe(true)
  })

  it('keeps a grant the list does not hold when another library is ticked', async () => {
    const me = user()
    const mixed = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [
        { id: 99, name: 'Gone' },
        { id: 70, name: 'Manga' },
      ],
    })
    const { host } = await show(mixed, [me, mixed], { actingAs: 1 })
    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, {
      libraries: [99, 70, 80],
    })
  })

  it('asks before Every library lifts a limit, and lifts it on the answer', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(radio(host, 'every'))
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(radio(host, 'only').checked).toBe(true)
    const asking = control(grants(host), '.lift-limit')
    expect(asking.getAttribute('role')).toBe('group')
    expect(asking.querySelector('p')?.textContent).toBe(
      'sam would see every library, including any added later, and the ticks below would be cleared.',
    )

    await click(control(host, 'button[name="lift-limit"]'))
    // An empty list is a limit to no library, so lifting says so in a key of
    // its own.
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { every_library: true })
    expect(radio(host, 'every').checked).toBe(true)
    expect(host.querySelector('.grant-list')).toBeNull()
    expect(host.querySelector('.lift-limit')).toBeNull()
    expect(document.activeElement).toBe(radio(host, 'every'))
  })

  it('moves focus to Keep the limit when the question opens, and describes the group by its sentence', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(radio(host, 'every'))

    expect(document.activeElement).toBe(
      control(host, 'button[name="keep-limit"]'),
    )
    const asking = control(grants(host), '.lift-limit')
    expect(asking.getAttribute('aria-label')).toBe('Lift the limit')
    const described = asking.getAttribute('aria-describedby')
    expect(described).toBeTruthy()
    const sentence = described ? document.getElementById(described) : null
    expect(sentence?.textContent).toBe(
      'sam would see every library, including any added later, and the ticks below would be cleared.',
    )
  })

  it('takes the question away when Only these is chosen again, and sends nothing', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(radio(host, 'every'))
    expect(host.querySelector('.lift-limit')).not.toBeNull()

    await click(radio(host, 'only'))
    expect(host.querySelector('.lift-limit')).toBeNull()
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(radio(host, 'only').checked).toBe(true)
  })

  it('retires the question when the account is lifted elsewhere, and does not bring it back', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(radio(host, 'every'))
    expect(host.querySelector('.lift-limit')).not.toBeNull()

    const again = (who: usersApi.User) =>
      act(async () => {
        rerender(
          <MemoryRouter>
            <AccountPage
              user={who}
              users={[me, who]}
              libraries={[MANGA, COMICS]}
              actingAs={1}
              onChanged={vi.fn()}
            />
          </MemoryRouter>,
        )
        await Promise.resolve()
      })
    // Its grants revoked elsewhere: still limited, so lifting is still a
    // change, and the question stands.
    await again({ ...other, libraries: [] })
    expect(host.querySelector('.lift-limit')).not.toBeNull()
    // Lifted elsewhere: nothing is left to ask.
    await again({ ...other, every_library: true, libraries: [] })
    expect(host.querySelector('.lift-limit')).toBeNull()
    // A limit again: the question was answered by the account, not re-asked.
    await again({
      ...other,
      every_library: false,
      libraries: [{ id: 80, name: 'Comics' }],
    })
    expect(host.querySelector('.lift-limit')).toBeNull()
  })

  it('asks before lifting an account that sees no library, and names no ticks', async () => {
    // A limit to nothing is still a limit: lifting it widens the account to
    // every library, the one widening that is always asked.
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(radio(host, 'every'))
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(radio(host, 'only').checked).toBe(true)
    expect(control(grants(host), '.lift-limit p').textContent).toBe(
      'sam would see every library, including any added later.',
    )

    await click(control(host, 'button[name="lift-limit"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { every_library: true })
    expect(radio(host, 'every').checked).toBe(true)
    expect(seesNone(host)).toBeNull()
  })

  it('holds both answers while a write is in flight, from the send, and draws them held once it has lasted', async () => {
    holdTheClock()
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    vi.mocked(usersApi.patchUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(radio(host, 'every'))
    await click(control(host, 'input[name="can_stream"]'))
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    pass(WAIT_DRAWN_AFTER_MS - 1)
    const answers = ['lift-limit', 'keep-limit'].map((name) =>
      control<HTMLButtonElement>(host, `button[name="${name}"]`),
    )
    expect(answers.map(holdOf)).toEqual(['free', 'free'])
    for (const answer of answers) await click(answer)
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.lift-limit')).not.toBeNull()
    pass(1)
    expect(answers.map(holdOf)).toEqual(['held', 'held'])
    for (const answer of answers) await click(answer)
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.lift-limit')).not.toBeNull()
  })

  it('keeps the limit when the answer is no, and puts focus back on Only these', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(radio(host, 'every'))
    await click(control(host, 'button[name="keep-limit"]'))

    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(host.querySelector('.lift-limit')).toBeNull()
    expect(radio(host, 'only').checked).toBe(true)
    expect(tick(host, 70).checked).toBe(true)
    expect(document.activeElement).toBe(radio(host, 'only'))
  })

  it('saves Only these at once as no library', async () => {
    // Narrowing to nothing is the safe direction, and every tick after it
    // saves at once as well, so there is no unsaved choice to keep.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(radio(host, 'only'))
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [] })
    expect(radio(host, 'only').checked).toBe(true)
    expect(host.querySelector('.grant-list')).not.toBeNull()
    expect(seesNone(host)?.textContent).toBe('This account sees no library.')

    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledTimes(2)
    expect(usersApi.patchUser).toHaveBeenLastCalledWith(2, { libraries: [80] })
  })

  it('puts an account back to Every library after Only these and a tick', async () => {
    // A reload that brings nothing, so the page runs on the answers it kept.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const changed = vi.fn(() => Promise.resolve())
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      onChanged: changed,
    })

    await click(radio(host, 'only'))
    await click(tick(host, 80))
    await click(radio(host, 'every'))
    await click(control(host, 'button[name="lift-limit"]'))

    expect(usersApi.patchUser).toHaveBeenNthCalledWith(1, 2, {
      libraries: [],
    })
    expect(usersApi.patchUser).toHaveBeenNthCalledWith(2, 2, {
      libraries: [80],
    })
    expect(usersApi.patchUser).toHaveBeenNthCalledWith(3, 2, {
      every_library: true,
    })
    expect(radio(host, 'every').checked).toBe(true)
    expect(radio(host, 'only').checked).toBe(false)
    expect(host.querySelector('.grant-list')).toBeNull()
    expect(seesNone(host)).toBeNull()
  })

  it('keeps drawing Only these when Every library is pressed while a tick is being saved', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
    })
    vi.mocked(usersApi.patchUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)

    await click(radio(host, 'every'))

    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(radio(host, 'only').checked).toBe(true)
    expect(radio(host, 'every').checked).toBe(false)
    expect(host.querySelector('.grant-list')).not.toBeNull()
    expect(host.querySelector('.lift-limit')).toBeNull()
  })

  it('does not switch to Only these while another write is being saved', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.patchUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'input[name="can_stream"]'))
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)

    await click(radio(host, 'only'))

    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(radio(host, 'every').checked).toBe(true)
    expect(radio(host, 'only').checked).toBe(false)
    expect(host.querySelector('.grant-list')).toBeNull()
  })

  /**
   * **Inside one `act`, nothing has rendered between two clicks**, so a handler
   * reading the `saving` state still sees false after a write has begun. Only
   * a handler that reads the ref sees the write. A reset click and a radio
   * click in one `act` is how the two are told apart: by the time the other
   * tests press the radio, the page has rendered `saving`.
   */
  async function resetThen(host: HTMLElement, second: Element) {
    await typeInto(
      control<HTMLInputElement>(host, 'input[name="password"]'),
      'hunter2',
    )
    await typeInto(confirmBox(host), 'hunter2')
    await act(async () => {
      control<HTMLElement>(host, 'button[name="reset"]').click()
      ;(second as HTMLElement).click()
      await Promise.resolve()
    })
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
  }

  it('does not switch to Only these in the same act as a write that has just begun', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.patchUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await resetThen(host, radio(host, 'only'))
    expect(radio(host, 'every').checked).toBe(true)
    expect(host.querySelector('.grant-list')).toBeNull()
  })

  it('does not ask to lift the limit in the same act as a write that has just begun', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      every_library: false,
    })
    vi.mocked(usersApi.patchUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(host.querySelector('.grant-list')).not.toBeNull()
    await resetThen(host, radio(host, 'every'))
    expect(radio(host, 'only').checked).toBe(true)
    expect(host.querySelector('.grant-list')).not.toBeNull()
    expect(host.querySelector('.lift-limit')).toBeNull()
  })

  it('says an administrator sees everything whatever is chosen', async () => {
    const me = user()
    const admin = user({
      id: 2,
      name: 'sam',
      role: 'admin',
      every_library: false,
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(admin, [me, admin], { actingAs: 1 })
    // Beside the choice it qualifies, as its help.
    expect(helpOf(row(host, 'Sees'))?.textContent).toBe(
      'An administrator sees every library whatever is chosen here; the choice applies if the role becomes user.',
    )
    const reader = user({ id: 3, name: 'kim', role: 'user' })
    const other = await show(reader, [me, reader], { actingAs: 1 })
    expect(helpOf(row(other.host, 'Sees'))).toBeNull()
    expect(grants(other.host).textContent).not.toContain(
      'An administrator sees every library',
    )
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
    // taiyaku does not own rather than widening what it may read. A page that
    // drew this ticked because the account is an admin would state a
    // permission the server answers 403 to.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'admin', can_stamp: false })
    const { host } = await show(other, [me, other])

    expect(
      control<HTMLInputElement>(host, 'input[name="can_stamp"]').checked,
    ).toBe(false)
  })

  it('says there are no libraries rather than drawing an empty editor', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      libraries: [],
    })
    expect(grants(host).textContent).toContain('There are no libraries yet')
    expect(grants(host).querySelector('input')).toBeNull()
  })

  it('draws the server sentence when a patch is refused', async () => {
    // The other half of §6.1: what the client could not tell in advance
    // arrives as a 409 with a sentence, and the page renders that.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'admin' })
    vi.mocked(usersApi.patchUser).mockRejectedValue(
      new Error(
        '/api/library/users/2: 409 {"error":"demoting `sam` would leave this library with no administrator who can log in"}',
      ),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'input[name="can_download"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'demoting `sam` would leave this library with no administrator who can log in',
    )
  })

  it('names an account nobody can sign in as', async () => {
    const locked = user({ id: 2, name: 'sam', role: 'user', can_log_in: false })
    const { host } = await show(locked, [user(), locked], { actingAs: 1 })
    // Under the header, above every section.
    const notice = control(host, '.page-header').nextElementSibling
    expect(notice?.className).toBe('locked')
    expect(notice?.textContent).toBe(
      'Nobody can sign in as this account: it holds no usable password. Setting one below gives it a way in.',
    )
  })

  it('keeps the role explanation beside its label, out of the control’s box', async () => {
    // The same last-administrator scenario the invariant tests above use,
    // which is known to render `roleWhy` — the sentence this test checks the
    // placement of.
    const sole = user()
    const { host } = await show(sole, [
      sole,
      user({ id: 2, name: 'reader', role: 'user' }),
    ])
    const role = row(host, 'Role')
    // A sentence, not a control: it sits under the label, and the control's
    // box holds the dropdown alone.
    const why = helpOf(role)
    expect(why?.textContent).toContain('only administrator who can log in')
    expect(why?.parentElement).toBe(
      role.querySelector(':scope > .settings-row-text'),
    )
    const box = role.querySelector(':scope > .settings-row-control')
    expect([...(box?.children ?? [])].map((el) => el.tagName)).toEqual([
      'SELECT',
    ])
  })

  it('gives each permission checkbox a row of its own, with its words and its help', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const rows = ['can_download', 'can_stream', 'can_stamp'].map((name) => {
      const r = control(host, `input[name="${name}"]`).closest('.settings-row')
      if (!r) throw new Error(`no row holds ${name}`)
      return r
    })
    expect(
      rows.map((r) => [
        r.querySelector(':scope > .settings-row-text > .settings-row-label')
          ?.textContent,
        helpOf(r)?.textContent,
        r.querySelectorAll('input').length,
        r.classList.contains('settings-row-check'),
      ]),
    ).toEqual([
      ['Can download', 'Save whole editions as files.', 1, true],
      ['Can stream', 'Read pages, in the browser or an OPDS app.', 1, true],
      ['Can stamp', 'Write ComicInfo.xml into archives.', 1, true],
    ])
    // And none of them shares the role's row.
    expect(
      row(host, 'Role').querySelectorAll('input[type="checkbox"]'),
    ).toHaveLength(0)
  })

  it('gives the role its row’s label, tied by for, in a row that stacks when narrow', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const select = control<HTMLSelectElement>(host, 'select[name="role"]')
    const labels = [...(select.labels ?? [])]
    expect(labels.map((l) => [l.className, l.textContent])).toEqual([
      ['settings-row-label', 'Role'],
    ])
    // Not wrapped: the label sits in the row's text, apart from the box.
    expect(labels[0]?.contains(select)).toBe(false)
    // A check row keeps its control beside its words at every width; this
    // one puts the label above the dropdown when the box is narrow.
    expect(select.closest('.settings-row')?.classList).not.toContain(
      'settings-row-check',
    )
  })
})

/**
 * Confirm password (the user's rulings of 2026-10-25): Set password is held
 * while New password and Confirm password differ, and a callout laid under
 * Confirm password's row, over Set password's at the form's foot (the
 * user's rulings of 2026-10-26), says so whenever the confirm is neither
 * empty nor the start of the new one, wherever focus is, and after a press
 * of the held button.
 */
describe('an account’s Confirm password', () => {
  /** Sam's page, seen by another administrator, and its password form. */
  async function samsPassword() {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    return {
      host,
      form: control<HTMLFormElement>(host, 'form.reset'),
      box: control<HTMLInputElement>(host, 'input[name="password"]'),
      confirm: confirmBox(host),
      set: control<HTMLButtonElement>(host, 'button[name="reset"]'),
    }
  }

  /**
   * The status region in the callout drawn straight after Confirm
   * password's row.
   */
  const lineAfter = (confirm: HTMLInputElement) => {
    const callout = confirm.closest('.settings-row')?.nextElementSibling
    expect(callout?.className).toBe('callout')
    const region = callout?.querySelector(':scope > [role="status"]')
    if (!(region instanceof HTMLElement)) throw new Error('no status region')
    return region
  }

  it('holds Set password, by aria-disabled, while New password and Confirm password differ, and frees it when they match', async () => {
    const { box, confirm, set } = await samsPassword()
    await typeInto(box, 'hunter22')
    expect(holdOf(set)).toBe('held')
    await typeInto(confirm, 'hunter2')
    expect(holdOf(set)).toBe('held')
    await typeInto(confirm, 'hunter22')
    expect(holdOf(set)).toBe('free')
    await typeInto(confirm, 'hunter222')
    expect(holdOf(set)).toBe('held')
    await typeInto(box, 'hunter222')
    expect(holdOf(set)).toBe('free')
  })

  it('says the two do not match for a press of Set password while they differ, the confirm empty or the start of the password, until a box is edited, and sends nothing', async () => {
    const { form, box, confirm, set } = await samsPassword()
    const region = lineAfter(confirm)
    await typeInto(box, 'hunter22')
    expect(region.textContent).toBe('')
    await click(set)
    expect([
      region.textContent,
      confirm.getAttribute('aria-invalid'),
      confirm.getAttribute('aria-describedby'),
    ]).toEqual(['The two passwords do not match.', 'true', region.id])

    await typeInto(confirm, 'hunter')
    expect(region.textContent).toBe('')
    await click(set)
    expect(region.textContent).toBe('The two passwords do not match.')
    await typeInto(confirm, 'hunter2')
    expect(region.textContent).toBe('')

    // A submit that reaches the form another way says it too.
    await submit(form)
    expect(region.textContent).toBe('The two passwords do not match.')
    expect(usersApi.patchUser).not.toHaveBeenCalled()
  })

  it('says the two do not match in a callout under Confirm password’s row, before Set password’s row and where Password set is said, in a status region drawn empty before it, for a typo and a longer confirm and not for the start of the password', async () => {
    const { box, confirm } = await samsPassword()
    const region = lineAfter(confirm)
    const setRow = region.parentElement?.nextElementSibling
    expect([
      setRow?.querySelector(':scope > button[name="reset"]') !== null,
      setRow?.nextElementSibling?.matches('.done'),
    ]).toEqual([true, true])
    expect(region.textContent).toBe('')
    await typeInto(box, 'hunter22')
    act(() => confirm.focus())
    const said = async (typed: string) => {
      await typeInto(confirm, typed)
      expect(lineAfter(confirm)).toBe(region)
      return region.textContent
    }
    expect(await said('hun')).toBe('')
    expect(await said('hunx')).toBe('The two passwords do not match.')
    expect(await said('hunter22')).toBe('')
    expect(await said('hunter222')).toBe('The two passwords do not match.')
    expect(await said('')).toBe('')
  })

  it('says the two do not match while they differ, wherever focus is', async () => {
    const { host, box, confirm } = await samsPassword()
    const region = lineAfter(confirm)
    const role = control<HTMLSelectElement>(host, 'select[name="role"]')
    // Filled while focus is in neither, as a password manager fills.
    await typeInto(box, 'hunter22')
    await typeInto(confirm, 'hunted')
    expect(region.textContent).toBe('The two passwords do not match.')
    for (const focused of [confirm, box, role]) {
      act(() => focused.focus())
      expect(region.textContent, focused.name).toBe(
        'The two passwords do not match.',
      )
    }
    await typeInto(confirm, 'hunter')
    expect(region.textContent).toBe('')
  })

  it('names the line as Confirm password’s description, and marks the box invalid, only while the line is said', async () => {
    const { box, confirm } = await samsPassword()
    const region = lineAfter(confirm)
    act(() => confirm.focus())
    const marks = () => [
      confirm.getAttribute('aria-invalid'),
      confirm.getAttribute('aria-describedby'),
    ]
    await typeInto(box, 'hunter22')
    await typeInto(confirm, 'hunter')
    expect(marks()).toEqual([null, null])
    await typeInto(confirm, 'hunted')
    expect(region.id).not.toBe('')
    expect(marks()).toEqual(['true', region.id])
    await typeInto(confirm, 'hunter22')
    expect(marks()).toEqual([null, null])
  })

  it('sends nothing for a submit while the two differ, and sends the new password once they match', async () => {
    const { form, box, confirm } = await samsPassword()
    await typeInto(box, 'hunter22')
    await typeInto(confirm, 'hunter2')
    await submit(form)
    await typeInto(confirm, 'hunter23')
    await submit(form)
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    await typeInto(confirm, 'hunter22')
    await submit(form)
    expect(vi.mocked(usersApi.patchUser).mock.calls).toEqual([
      [2, { password: 'hunter22' }],
    ])
  })

  it('keeps focus on Set password when a change to the boxes holds it', async () => {
    const { box, confirm, set } = await samsPassword()
    await typeInto(box, 'hunter22')
    await typeInto(confirm, 'hunter22')
    act(() => set.focus())
    expect(document.activeElement).toBe(set)
    // A box filled while Set password has focus, as a password manager may
    // fill one.
    await typeInto(confirm, 'hunter2')
    expect(holdOf(set)).toBe('held')
    expect(document.activeElement).toBe(set)
  })

  it('empties New password and Confirm password once a set works', async () => {
    const { box, confirm, set } = await samsPassword()
    await typeInto(box, 'hunter22')
    await typeInto(confirm, 'hunter22')
    await click(set)
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect([box.value, confirm.value]).toEqual(['', ''])
  })
})

describe('sessions and keys', () => {
  it('marks the browser doing the asking and revokes under this account', async () => {
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      // In the server's order, the last seen first. The newest session is
      // not the one last seen, so a header that gave the newest session's
      // time would say 2026-09-02.
      sessions: [
        {
          id: 'aaa',
          created: '2026-08-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: true,
        },
        {
          id: 'bbb',
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-02 10:00:00',
          user_agent: null,
          ip: null,
          current: false,
        },
      ],
      keys: [
        {
          id: 9,
          name: 'the tablet',
          created: '2026-08-01 10:00:00',
          last_used: null,
        },
      ],
    })
    const { host, changed } = await show(other, [user(), other], {
      actingAs: 1,
    })

    // The tally is the section's line, and the last time any session was
    // seen is the header's.
    expect(
      control(section(host, 'Sessions & keys'), '.settings-section-description')
        .textContent,
    ).toBe('2 sessions\u00a0·\u00a01 API key')
    expect(control(host, '.page-description').textContent).toBe(
      'User\u00a0·\u00a0last seen 2026-09-06 09:00:00 UTC',
    )
    // Each time the server wrote says it is UTC, as the Backups list says.
    expect(
      [...host.querySelectorAll('.session-list .when, .key-list .when')].map(
        (when) => when.textContent,
      ),
    ).toEqual([
      'started 2026-08-01 10:00:00 UTC\u00a0·\u00a0last seen 2026-09-06 09:00:00 UTC',
      'started 2026-09-01 10:00:00 UTC\u00a0·\u00a0last seen 2026-09-02 10:00:00 UTC',
      'API key\u00a0·\u00a0made 2026-08-01 10:00:00 UTC\u00a0·\u00a0never used',
    ])

    // Asserted per element, not by `toContain` over the whole page: a
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
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
      ],
      keys: [
        {
          id: 9,
          name: 'the tablet',
          created: '2026-08-01 10:00:00',
          last_used: '2026-09-05 08:00:00',
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
      'last used 2026-09-05 08:00:00 UTC',
    )
    await click(control(host, 'button[name="revoke-session-aaa"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no such session',
    )
    expect(changed).not.toHaveBeenCalled()
  })
})

/**
 * A proxy's own page for an upstream that did not answer, as the thrown
 * message carries it after `path: status`.
 */
const PROXY_PAGE =
  '502 <html>\r\n<head><title>502 Bad Gateway</title></head>\r\n<body>\r\n<center><h1>502 Bad Gateway</h1></center>\r\n</body>\r\n</html>\r\n'

describe('a write answered with no sentence', () => {
  const other = () =>
    user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
          user_agent: 'Firefox',
          ip: null,
          current: false,
        },
      ],
    })

  it('says a revocation answered with a page of markup by what was not done and the status, and never the page', async () => {
    vi.mocked(usersApi.revokeUserSession).mockRejectedValue(
      new Error(`/api/library/users/2/sessions/aaa: ${PROXY_PAGE}`),
    )
    const { host } = await show(other(), [user(), other()], { actingAs: 1 })
    await click(control(host, 'button[name="revoke-session-aaa"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'A session for sam was not revoked: the server answered 502',
    )
  })

  it('says a dry run of a removal answered with a page of markup by what was not counted and the status, and never the page', async () => {
    vi.mocked(usersApi.removalCost).mockRejectedValue(
      new Error(`/api/library/users/2?dry_run=true: ${PROXY_PAGE}`),
    )
    const { host } = await show(other(), [user(), other()], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    expect(
      control(host, '.remove-user').querySelector('[role="alert"]')
        ?.textContent,
    ).toBe(
      'What removing sam would take could not be counted: the server answered 502',
    )
  })

  it('says a write answered with a page of markup once its page has gone by what was not done and the status, and never the page', async () => {
    let refuse: ((err: Error) => void) | undefined
    vi.mocked(usersApi.patchUser).mockImplementation(
      () =>
        new Promise((_, reject) => {
          refuse = reject
        }),
    )
    const late = vi.fn()
    const { host } = await show(other(), [user(), other()], {
      actingAs: 1,
      onLateRefusal: late,
    })
    await click(control(host, 'input[name="can_stream"]'))
    rerender(<p>another page</p>)
    await act(async () => {
      refuse?.(new Error(`/api/library/users/2: ${PROXY_PAGE}`))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(late.mock.calls).toEqual([
      ['Can stream for sam was not saved (the server answered 502).'],
    ])
  })
})

describe('a revocation whose reload brings nothing back', () => {
  it('drops the row it revoked, so no second press can 404 over it', async () => {
    // `settled` waits for the list read, and that read can fail or be
    // superseded without bringing fresh props. A revoked session left listed
    // would keep a live Revoke, whose next press answers 404 for a session
    // already gone.
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
        {
          id: 'bbb',
          created: '2026-08-01 10:00:00',
          last_seen: '2026-08-02 10:00:00',
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
  it('drops its row from what the page holds now, not from the click-time copy', async () => {
    // A list read landing during the `DELETE` brings a newer row; writing back
    // the snapshot the click was made from would drop whatever it brought:
    // a session opened since, or a role another administrator changed.
    const session = (id: string, agent: string) => ({
      id,
      created: '2026-09-01 10:00:00',
      last_seen: '2026-09-06 09:00:00',
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
          <AccountPage
            user={fresher}
            users={[user(), fresher]}
            libraries={[MANGA]}
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

/**
 * Where focus goes when a Revoke's row goes with focus on it, which would
 * drop it to the page: to the nearest row left in the same list, the next
 * one, else the one before; with no row to go to, to the heading of
 * Sessions & keys. Compared with `toBe`, since `toEqual` reads two Revokes
 * of the same markup as equal.
 */
describe('focus after a Revoke', () => {
  const session = (id: string, agent: string) => ({
    id,
    created: '2026-09-01 10:00:00',
    last_seen: '2026-09-06 09:00:00',
    user_agent: agent,
    ip: null,
    current: false,
  })
  const key = (id: number, name: string) => ({
    id,
    name,
    created: '2026-08-01 10:00:00',
    last_used: null,
  })
  const other = () =>
    user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        session('aaa', 'Firefox'),
        session('bbb', 'Safari'),
        session('ccc', 'Chrome'),
      ],
      keys: [key(9, 'the tablet'), key(10, 'the phone')],
    })

  /** Focus the Revoke named `name`, press it, and let what follows land. */
  const pressRevoke = async (host: HTMLElement, name: string) => {
    const button = control<HTMLButtonElement>(host, `button[name="${name}"]`)
    act(() => button.focus())
    await act(async () => {
      button.click()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
  }

  const revokeNamed = (host: HTMLElement, name: string) =>
    control<HTMLButtonElement>(host, `button[name="${name}"]`)

  const sectionHeading = (host: HTMLElement) =>
    control<HTMLElement>(
      section(host, 'Sessions & keys'),
      '.settings-section-title',
    )

  it('gives focus to the next session’s Revoke, else the one before’s, and with none left to the heading of Sessions & keys', async () => {
    const { host } = await show(other(), [user(), other()], { actingAs: 1 })

    await pressRevoke(host, 'revoke-session-bbb')
    expect(document.activeElement).toBe(revokeNamed(host, 'revoke-session-ccc'))
    await pressRevoke(host, 'revoke-session-ccc')
    expect(document.activeElement).toBe(revokeNamed(host, 'revoke-session-aaa'))
    await pressRevoke(host, 'revoke-session-aaa')
    expect(document.activeElement).toBe(sectionHeading(host))
    expect(sectionHeading(host).getAttribute('tabindex')).toBe('-1')
    expect(vi.mocked(usersApi.revokeUserSession).mock.calls).toEqual([
      [2, 'bbb'],
      [2, 'ccc'],
      [2, 'aaa'],
    ])
  })

  it('gives focus to the next key’s Revoke, and with none left to the heading of Sessions & keys', async () => {
    const { host } = await show(other(), [user(), other()], { actingAs: 1 })

    await pressRevoke(host, 'revoke-key-9')
    expect(document.activeElement).toBe(revokeNamed(host, 'revoke-key-10'))
    await pressRevoke(host, 'revoke-key-10')
    expect(document.activeElement).toBe(sectionHeading(host))
    expect(vi.mocked(usersApi.revokeUserKey).mock.calls).toEqual([
      [2, 9],
      [2, 10],
    ])
  })

  it('leaves focus where it was put when a Revoke’s row goes once focus has left it', async () => {
    const answer = deferred<undefined>()
    vi.mocked(usersApi.revokeUserSession).mockReturnValue(answer.promise)
    const { host } = await show(other(), [user(), other()], { actingAs: 1 })
    const button = revokeNamed(host, 'revoke-session-bbb')
    act(() => button.focus())
    await click(button)
    const elsewhere = control<HTMLInputElement>(host, 'input[name="password"]')
    act(() => elsewhere.focus())
    await act(async () => {
      answer.resolve(undefined)
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    expect(host.querySelector('button[name="revoke-session-bbb"]')).toBeNull()
    expect(document.activeElement).toBe(elsewhere)
  })
})

describe('a double-clicked revocation', () => {
  it('sends one DELETE, so no 404 lands over a revocation that worked', async () => {
    // The second `DELETE` finds no row, and `revoke_session` answers 404 for a
    // session that is not there — the same answer as for one that never
    // existed — so an unguarded page would report a failure for the thing it
    // had just done.
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
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
  it('keeps Remove… drawn, held, while the confirm it opened is open, and a press of it asks nothing more', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockReturnValue(new Promise(() => {}))
    const { host } = await show(other, [me, other], { actingAs: 1 })
    const remove = control<HTMLButtonElement>(host, 'button[name="remove"]')
    await click(remove)
    expect(host.querySelector('.remove-user')).not.toBeNull()
    expect(host.querySelector('button[name="remove"]')).toBe(remove)
    expect(holdOf(remove)).toBe('held')
    await click(remove)
    expect(usersApi.removalCost).toHaveBeenCalledTimes(1)
  })

  it('hands focus back to Remove… when its confirm is cancelled', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockReturnValue(new Promise(() => {}))
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    const cancel = control<HTMLButtonElement>(
      host,
      'button[name="cancel-remove"]',
    )
    act(() => cancel.focus())
    await click(cancel)
    expect(host.querySelector('button[name="cancel-remove"]')).toBeNull()
    expect(document.activeElement).toBe(control(host, 'button[name="remove"]'))
  })

  it('keeps a failed dry run visible when another write clears the page', async () => {
    // `begin()` clears the shared failure slot on every write, and the dialog
    // draws "working out…" while there is neither a cost nor a reason, so a
    // capability toggle after a failed dry run would put the dialog back to
    // "working out" for ever, with nothing in flight.
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
    // `Users` keys the page by name, so where the page outlives a removal,
    // re-creating the removed name reuses this instance — and a tombstone
    // that outlived its account would draw the new one as removed, with no
    // controls. The recycled-id hazard `Users.tsx` describes, reached
    // through the name.
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
          <AccountPage
            user={remade}
            users={[me, remade]}
            libraries={[MANGA]}
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
    // them, so a slow first answer landing after a failed second would draw
    // its counts and a live *Remove sam* under an alert saying the count
    // could not be taken.
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

  it('deadens the confirm while another write is in flight, from the send, and draws it held once the write has lasted', async () => {
    // One write at a time: from the send, `inFlight` turns a press of the
    // confirm down, as it does every other control's, and the confirm is
    // drawn held once the write has lasted `WAIT_DRAWN_AFTER_MS`, as the
    // page's other buttons are, so a write answered at once draws no hold.
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
    holdTheClock()
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    await click(control(host, 'input[name="can_stream"]'))
    const confirm = control<HTMLButtonElement>(
      host,
      'button[name="confirm-remove"]',
    )
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(holdOf(confirm)).toBe('free')
    await click(confirm)
    expect(usersApi.removeUser).not.toHaveBeenCalled()
    pass(1)
    expect(holdOf(confirm)).toBe('held')
    await click(confirm)
    expect(usersApi.removeUser).not.toHaveBeenCalled()
  })

  it('draws a removed account as gone rather than as an editable page', async () => {
    // The account is deleted and nothing takes the page away: no `onRemoved`
    // here. Closing the dialog alone would put the plain Remove… back over an
    // id that answers "no account with id 2", with every other control live
    // beside it.
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
    expect(counts).toContain('3 library grants')
    expect(usersApi.removeUser).not.toHaveBeenCalled()

    await click(control(host, 'button[name="confirm-remove"]'))
    expect(usersApi.removeUser).toHaveBeenCalledWith(2)
  })

  it('draws the server sentence when the dry run is refused', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockRejectedValue(
      new Error(
        '/api/library/users/2?dry_run=true: 409 {"error":"removing `sam` would leave this library with no administrator who can log in"}',
      ),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'removing `sam` would leave this library with no administrator who can log in',
    )
  })

  it('says one library grant in the singular', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 1,
      },
      dry_run: true,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    expect(control(host, '.counts').textContent).toContain(
      'and 1 library grant.',
    )
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
    // page with no guard at all.
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
    // cost resolves, Cancel is the only button in the block. The removal
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

  it('opens no dialog over an account re-created under a removed one’s name: the dialog went with the body the tombstone replaced', async () => {
    // Nothing takes the page away here (no `onRemoved`). The removal's
    // answer draws the tombstone in the body's place, and the body's open
    // dialog goes with it. An account re-created under the name drops the
    // tombstone and is drawn with a body of its own, so no dialog comes back
    // over it, working out a removal nothing asked for, with no *Remove…*
    // button.
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
    expect(host.querySelector('.gone')).not.toBeNull()

    const remade = user({ id: 7, name: 'sam', role: 'user' })
    await act(async () => {
      rerender(
        <MemoryRouter>
          <AccountPage
            user={remade}
            users={[me, remade]}
            libraries={[MANGA]}
            actingAs={1}
            onChanged={vi.fn()}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
    })
    expect(host.querySelector('.gone')).toBeNull()
    expect(host.querySelector('.remove-user')).toBeNull()
    expect(host.querySelector('button[name="confirm-remove"]')).toBeNull()
    expect(host.querySelector('button[name="remove"]')).not.toBeNull()
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

describe('the waits, drawn only once each has lasted', () => {
  const other = () =>
    user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
      ],
      keys: [
        {
          id: 9,
          name: 'the tablet',
          created: '2026-08-01 10:00:00',
          last_used: null,
        },
      ],
    })
  const cost: usersApi.RemovalAnswer = {
    removed: {
      name: 'sam',
      positions: 0,
      notes: 0,
      sessions: 1,
      keys: 1,
      grants: 0,
    },
    dry_run: true,
  }
  /** How each control that a write draws held once it has lasted is held. */
  const held = (host: HTMLElement) =>
    [
      'select[name="role"]',
      'input[name="password"]',
      'input[name="confirm-password"]',
      'button[name="revoke-session-aaa"]',
      'button[name="revoke-key-9"]',
      'button[name="remove"]',
    ].map((selector) => holdOf(control(host, selector)))
  const none = ['free', 'free', 'free', 'free', 'free', 'free']

  describe('a write on the page', () => {
    it('draws no control held for a write answered inside the wait’s delay', async () => {
      holdTheClock()
      const answer = deferred<usersApi.User>()
      vi.mocked(usersApi.patchUser).mockReturnValue(answer.promise)
      const { host } = await show(other(), [user(), other()], { actingAs: 1 })
      const drawn = watchDrawn(() => JSON.stringify(held(host)))
      await click(control(host, 'input[name="can_stream"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() => answer.resolve({ ...other(), can_stream: false }))
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(
        control<HTMLInputElement>(host, 'input[name="can_stream"]').checked,
      ).toBe(false)
      expect([...drawn.seen]).toEqual([JSON.stringify(none)])
    })

    it('draws every control it holds as held from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(usersApi.patchUser).mockReturnValue(new Promise(() => {}))
      const { host } = await show(other(), [user(), other()], { actingAs: 1 })
      await click(control(host, 'input[name="can_stream"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(held(host)).toEqual(none)
      pass(1)
      expect(held(host)).toEqual([
        'held',
        'held',
        'held',
        'held',
        'held',
        'held',
      ])
    })

    // The password's form is submitted on its own as well as through a
    // press of Set password: a submit that reaches it other than through
    // the held button finds its write refused too.
    it.each<[string, (host: HTMLElement) => Promise<void>]>([
      [
        'a choice of Role',
        (host) => choose(host, 'select[name="role"]', 'admin'),
      ],
      [
        'typing in New password',
        (host) =>
          typeInto(
            control<HTMLInputElement>(host, 'input[name="password"]'),
            'hunter22',
          ),
      ],
      [
        'typing in Confirm password',
        (host) => typeInto(confirmBox(host), 'hunter22'),
      ],
      [
        'a press of Set password',
        (host) => click(control(host, 'button[name="reset"]')),
      ],
      [
        'a submit of the password’s form',
        (host) => submit(control(host, 'form.reset')),
      ],
      [
        'a press of a session’s Revoke',
        (host) => click(control(host, 'button[name="revoke-session-aaa"]')),
      ],
      [
        'a press of a key’s Revoke',
        (host) => click(control(host, 'button[name="revoke-key-9"]')),
      ],
      [
        'a press of Remove…',
        (host) => click(control(host, 'button[name="remove"]')),
      ],
    ])('turns down %s once the write is drawn holding it', async (_, touch) => {
      holdTheClock()
      vi.mocked(usersApi.patchUser).mockReturnValue(new Promise(() => {}))
      vi.mocked(usersApi.removalCost).mockResolvedValue(cost)
      const { host } = await show(other(), [user(), other()], { actingAs: 1 })
      const password = control<HTMLInputElement>(host, 'input[name="password"]')
      await typeInto(password, 'hunter2')
      await typeInto(confirmBox(host), 'hunter2')
      await click(control(host, 'input[name="can_stream"]'))
      pass(WAIT_DRAWN_AFTER_MS)
      expect([
        ...held(host),
        holdOf(control(host, 'button[name="reset"]')),
      ]).toEqual(['held', 'held', 'held', 'held', 'held', 'held', 'held'])

      await touch(host)
      expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
      expect(
        control<HTMLSelectElement>(host, 'select[name="role"]').value,
      ).toBe('user')
      expect([password.value, confirmBox(host).value]).toEqual([
        'hunter2',
        'hunter2',
      ])
      expect(usersApi.revokeUserSession).not.toHaveBeenCalled()
      expect(usersApi.revokeUserKey).not.toHaveBeenCalled()
      expect(usersApi.removalCost).not.toHaveBeenCalled()
      expect(host.querySelector('.remove-user')).toBeNull()
    })

    it('holds every control from the send, before any is drawn held', async () => {
      holdTheClock()
      vi.mocked(usersApi.patchUser).mockReturnValue(new Promise(() => {}))
      vi.mocked(usersApi.removalCost).mockResolvedValue(cost)
      const { host } = await show(other(), [user(), other()], { actingAs: 1 })
      await click(control(host, 'input[name="can_stream"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(held(host)).toEqual(none)

      await choose(host, 'select[name="role"]', 'admin')
      expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
      const password = control<HTMLInputElement>(host, 'input[name="password"]')
      await typeInto(password, 'hunter2')
      await typeInto(confirmBox(host), 'hunter2')
      expect([password.value, confirmBox(host).value]).toEqual(['', ''])
      await click(control(host, 'button[name="revoke-session-aaa"]'))
      await click(control(host, 'button[name="revoke-key-9"]'))
      expect(usersApi.revokeUserSession).not.toHaveBeenCalled()
      expect(usersApi.revokeUserKey).not.toHaveBeenCalled()
      await click(control(host, 'button[name="remove"]'))
      expect(usersApi.removalCost).not.toHaveBeenCalled()
      expect(host.querySelector('.remove-user')).toBeNull()
    })
  })

  describe('a removal', () => {
    const WORKING = 'Working out what this would remove…'
    /** Remove's words, and how it and Cancel are held. */
    const state = (host: HTMLElement) => {
      const confirm = control<HTMLButtonElement>(
        host,
        'button[name="confirm-remove"]',
      )
      const cancel = control<HTMLButtonElement>(
        host,
        'button[name="cancel-remove"]',
      )
      return [confirm.textContent, holdOf(confirm), holdOf(cancel)]
    }
    async function confirming() {
      vi.mocked(usersApi.removalCost).mockResolvedValue(cost)
      const shown = await show(other(), [user(), other()], { actingAs: 1 })
      await click(control(shown.host, 'button[name="remove"]'))
      return shown.host
    }

    it('keeps the dry run’s sentence unseen for counts answered inside the wait’s delay', async () => {
      holdTheClock()
      const counted = deferred<usersApi.RemovalAnswer>()
      vi.mocked(usersApi.removalCost).mockReturnValue(counted.promise)
      const { host } = await show(other(), [user(), other()], { actingAs: 1 })
      const drawn = watchDrawn(() => sentenceDrawn(host, WORKING))
      await click(control(host, 'button[name="remove"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() => counted.resolve(cost))
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('.remove-user .counts')).not.toBeNull()
      expect([...drawn.seen]).toEqual(['none', 'unseen'])
    })

    it('draws the dry run’s sentence from the wait’s delay, in the room it kept', async () => {
      holdTheClock()
      vi.mocked(usersApi.removalCost).mockReturnValue(new Promise(() => {}))
      const { host } = await show(other(), [user(), other()], { actingAs: 1 })
      await click(control(host, 'button[name="remove"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(sentenceDrawn(host, WORKING)).toBe('unseen')
      pass(1)
      expect(sentenceDrawn(host, WORKING)).toBe('drawn')
    })

    it('draws no wait and no held button for a removal refused inside the wait’s delay', async () => {
      holdTheClock()
      const sent = deferred<usersApi.RemovalAnswer>()
      vi.mocked(usersApi.removeUser).mockReturnValue(sent.promise)
      const host = await confirming()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await click(control(host, 'button[name="confirm-remove"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() =>
        sent.reject(new Error('/api/library/users/2: 500 the disk is full')),
      )
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('[role="alert"]')?.textContent).toContain(
        'the disk is full',
      )
      expect([...drawn.seen]).toEqual([
        JSON.stringify(['Remove sam', 'free', 'free']),
      ])
    })

    it('draws Removing… and holds Remove and Cancel from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(usersApi.removeUser).mockReturnValue(new Promise(() => {}))
      const host = await confirming()
      await click(control(host, 'button[name="confirm-remove"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Remove sam', 'free', 'free'])
      pass(1)
      expect(state(host)).toEqual(['Removing…', 'held', 'held'])
      await click(control(host, 'button[name="confirm-remove"]'))
      expect(usersApi.removeUser).toHaveBeenCalledTimes(1)
      await click(control(host, 'button[name="cancel-remove"]'))
      expect(host.querySelector('.remove-user')).not.toBeNull()
    })

    it('holds Remove and Cancel from the send, before either is drawn held', async () => {
      holdTheClock()
      vi.mocked(usersApi.removeUser).mockReturnValue(new Promise(() => {}))
      const host = await confirming()
      await click(control(host, 'button[name="confirm-remove"]'))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Remove sam', 'free', 'free'])
      await click(control(host, 'button[name="confirm-remove"]'))
      expect(usersApi.removeUser).toHaveBeenCalledTimes(1)
      await click(control(host, 'button[name="cancel-remove"]'))
      expect(host.querySelector('.remove-user')).not.toBeNull()
    })
  })
})

describe('a permission refused once its page has gone', () => {
  // The sentence is said on whatever page the person is on, where nothing
  // else names the setting, so it names it by the words its row shows.
  it.each([
    ['can_download', 'Can download'],
    ['can_stream', 'Can stream'],
    ['can_stamp', 'Can stamp'],
  ])('names %s by its row’s label, %s', async (name, label) => {
    let refuse: ((err: Error) => void) | undefined
    vi.mocked(usersApi.patchUser).mockImplementation(
      () =>
        new Promise((_, reject) => {
          refuse = reject
        }),
    )
    const late = vi.fn()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [user(), other], {
      actingAs: 1,
      onLateRefusal: late,
    })
    const box = control<HTMLInputElement>(host, `input[name="${name}"]`)
    expect([...(box.labels ?? [])].map((l) => l.textContent)).toEqual([label])
    await click(box)
    // Gone before the answer comes: an `act` of its own, since one nested
    // in the next would not unmount the page until that one ended.
    rerender(<p>another page</p>)
    await act(async () => {
      refuse?.(
        new Error('/api/library/users/2: 500 {"error":"the store is locked"}'),
      )
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(late.mock.calls).toEqual([
      [`${label} for sam was not saved (the store is locked).`],
    ])
  })
})

describe('the rows', () => {
  // Each case names the rows it reaches, so the check is never over fewer
  // rows than it says: the grant list's only while a limit stands, and the
  // choice's help only for an administrator.
  it.each([
    ['an account that sees every library', 'user', true, 0],
    ['an account limited to one library', 'user', false, 2],
    ['an administrator limited to no library', 'admin', false, 2],
  ] as const)(
    'ties each row’s label and help to its control, for %s',
    async (_, role, every, ticks) => {
      const who = user({
        id: 2,
        name: 'sam',
        role,
        every_library: every,
        libraries: role === 'user' && !every ? [{ id: 70, name: 'Manga' }] : [],
      })
      const { host } = await show(who, [user(), who], { actingAs: 1 })
      const sees = row(host, 'Sees')
      expect(sees.querySelectorAll('input[type="radio"]')).toHaveLength(2)
      expect(helpOf(sees) !== null).toBe(role === 'admin')
      expect(
        grants(host).querySelectorAll('input[type="checkbox"]'),
      ).toHaveLength(ticks)
      expectRowsWired(host)
    },
  )
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
  it('gives each control that carries an aria-label its row label’s words as that label', async () => {
    const { host } = await show(sam(), [user(), sam()])
    const labelled = [
      ...host.querySelectorAll<HTMLLabelElement>('label.settings-row-label'),
    ]
      .map((label) => ({ written: label.textContent, control: label.control }))
      .filter(({ control }) => control?.hasAttribute('aria-label'))
    // Role and the new password, twice. Named, because a loop over nothing
    // passes.
    expect(labelled.map(({ written }) => written)).toEqual([
      'Role',
      'New password',
      'Confirm password',
    ])
    for (const { written, control } of labelled) {
      expect(control?.getAttribute('aria-label'), written ?? '').toBe(written)
    }
  })
})

/**
 * The caller's own keys and sessions are also the lists `/account` draws. A
 * revocation on one's own account marks the list it changed stale, so
 * `/account` arriving afterwards waits for it rather than drawing the
 * revoked row with a live Revoke; one on another account changes neither.
 */
describe('a revocation, and the lists /account draws', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  const json = (body: unknown) =>
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  const PHONE = {
    id: 7,
    name: 'phone',
    created: '2026-08-01 10:00:00',
    last_used: null,
  }
  const session = (id: string, agent: string, current: boolean) => ({
    id,
    created: '2026-08-01 10:00:00',
    last_seen: '2026-09-02 10:00:00',
    user_agent: agent,
    ip: null,
    current,
  })
  const FIREFOX = session('aa', 'Firefox', true)
  const SAFARI = session('bb', 'Safari', false)

  /**
   * The caller's lists as the server holds them, less what a revocation of
   * the caller's own took; `hold` keeps every later read of either list
   * until the case lets them answer.
   */
  function callerServer() {
    const gone = new Set<number | string>()
    vi.mocked(usersApi.revokeUserKey).mockImplementation(async (id, kid) => {
      if (id === 1) gone.add(kid)
    })
    vi.mocked(usersApi.revokeUserSession).mockImplementation(
      async (id, sid) => {
        if (id === 1) gone.add(sid)
      },
    )
    const answer = (input: string) => {
      if (input === '/api/keys')
        return json([PHONE].filter((key) => !gone.has(key.id)))
      if (input === '/api/sessions')
        return json([FIREFOX, SAFARI].filter((one) => !gone.has(one.id)))
      if (input === '/api/about') return json(ABOUT)
      return new Response(null, { status: 404 })
    }
    const held: (() => void)[] = []
    let holding = false
    const fetch = vi.fn((input: string) =>
      holding && input !== '/api/about'
        ? new Promise<Response>((resolve) => {
            held.push(() => resolve(answer(input)))
          })
        : Promise.resolve(answer(input)),
    )
    return {
      fetch,
      hold: () => {
        holding = true
      },
      letAnswer: () => {
        for (const one of held.splice(0)) one()
      },
    }
  }

  /** The label of each row of `/account` that ends in its Revoke. */
  const revocable = (host: HTMLElement) =>
    [...host.querySelectorAll('.settings-row')]
      .filter(
        (one) =>
          one.querySelector(':scope > .settings-row-control > button')
            ?.textContent === 'Revoke',
      )
      .map((one) => one.querySelector('.settings-row-label')?.textContent)

  /** `/account` drawn, and its reads let answer. */
  async function accountDrawn() {
    const host = mount(<Account placement="page" onSignedOut={() => {}} />)
    await flush()
    return host
  }

  /**
   * `/account` drawn and left, so the store keeps what it read; then the
   * account's page of `whose`, where `button` is pressed; then `/account`
   * again with its lists' reads held. Which rows it draws on arrival, its
   * wait line once the wait has held, and which rows it draws once the
   * reads land.
   */
  async function accountAfter(whose: usersApi.User, button: string) {
    const answers = callerServer()
    vi.stubGlobal('fetch', answers.fetch)
    const before = await accountDrawn()
    expect(revocable(before)).toEqual(['phone', 'Firefox (this one)', 'Safari'])
    rerender(<div />)
    const { host } = await show(whose, [user(), sam(), whose], {
      actingAs: 1,
    })
    await click(control(host, button))
    rerender(<div />)
    holdTheClock()
    answers.hold()
    const again = mount(<Account placement="page" onSignedOut={() => {}} />)
    await act(async () => {
      await Promise.resolve()
    })
    const onArrival = revocable(again)
    pass(WAIT_DRAWN_AFTER_MS)
    const wait =
      again.querySelector('section.account > p[role="status"]')?.textContent ??
      null
    await flush(() => answers.letAnswer())
    return { onArrival, wait, landed: revocable(again) }
  }

  const own = () =>
    user({
      keys: [PHONE],
      sessions: [FIREFOX, SAFARI],
    })
  const theirs = () =>
    user({
      id: 3,
      name: 'kim',
      role: 'user',
      keys: [{ ...PHONE, id: 9 }],
      sessions: [session('cc', 'Safari', false)],
    })

  it.each([
    ['a key', 'button[name="revoke-key-7"]', ['Firefox (this one)', 'Safari']],
    [
      'a session',
      'button[name="revoke-session-bb"]',
      ['phone', 'Firefox (this one)'],
    ],
  ] as const)(
    'makes /account wait for the list on a return after %s of one’s own is revoked on one’s account’s page',
    async (_, button, left) => {
      expect(await accountAfter(own(), button)).toEqual({
        onArrival: [],
        wait: 'Loading…',
        landed: left,
      })
    },
  )

  it.each([
    ['a key', 'button[name="revoke-key-9"]'],
    ['a session', 'button[name="revoke-session-cc"]'],
  ] as const)(
    'draws /account at once on a return after %s is revoked on another account’s page',
    async (_, button) => {
      const all = ['phone', 'Firefox (this one)', 'Safari']
      expect(await accountAfter(theirs(), button)).toEqual({
        onArrival: all,
        wait: null,
        landed: all,
      })
    },
  )
})

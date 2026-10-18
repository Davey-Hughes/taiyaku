import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, type Mock, vi } from 'vitest'
import type { Library } from './libraries'
import { fields } from './testing/fields'
import { typeInto } from './testing/input'
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
  every_library: true,
  libraries: [],
  ...over,
})

const lib = (id: number, name: string, paths: string[]): Library => ({
  id,
  name,
  works: 0,
  books: 0,
  folders: paths.map((path, i) => ({
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
          // `null` asks for the unread state; absent means the two libraries.
          libraries={
            over.libraries === null
              ? undefined
              : (over.libraries ?? [MANGA, COMICS])
          }
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

const radio = (host: HTMLElement, value: 'every' | 'only') =>
  control<HTMLInputElement>(
    host,
    `.grants input[type="radio"][value="${value}"]`,
  )
/** The sentence naming grants whose library the list does not hold. */
const unlisted = (host: HTMLElement) =>
  host.querySelector('.grants [data-field="unlisted-grants"]')
/** The sentence saying a limited account holds no grant at all. */
const seesNone = (host: HTMLElement) =>
  host.querySelector('.grants [data-field="no-library"]')
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
    // prop, so inside it the card still draws its pre-patch grants with the
    // boxes live — and `toggleGrant` computes a *whole list* from those, so a
    // second tick there sends the new library alone and silently revokes the
    // first. Held until `onChanged` settles.
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
      'library-70',
      'library-80',
      'library-90',
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
    const second = tick(host, 90)
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
    // **The hold's own hole, found by the second review pass.** `settled`
    // waits for the reload — but a reload that fails, or one superseded by
    // another card's, resolves without fresh props, so the card came back
    // live over pre-write values and the next tick computed a whole grant
    // list from them, silently revoking the first.
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
    const grants = control(host, '.grants').textContent ?? ''
    expect(grants).toContain('has not been read')
    expect(grants).toContain('Manga')
    expect(grants).not.toContain('no libraries yet')
    expect(host.querySelector('.grants input')).toBeNull()
  })

  // With the list unread this sentence is the only place the card says what
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
      expect(control(host, '.grants p.unknown').textContent).toBe(
        `The libraries list has not been read, so there is nothing to offer here. ${sees}`,
      )
    },
  )

  it('draws an account that sees every library as Every library, with no boxes', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(control(host, '.grants legend').textContent).toBe(
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
      expect(control(host, '.grants').textContent).not.toContain(
        'sees no library',
      )
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
    expect(control(limited.host, '.grants .empty').textContent).toBe(
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
    // Two cards sharing one name would be one radio group to a browser.
    expect(radio(host, 'every').name).toBe('sees-2')
    expect(radio(host, 'only').name).toBe('sees-2')
  })

  it('draws a granted account as Only these, a box per library with its folders', async () => {
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
    expect(manga.querySelector('label span')?.textContent).toBe('Manga')
    expect(
      [...manga.querySelectorAll('.grant-folders code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    const comics = tick(host, 80).closest('li') as HTMLElement
    expect(comics.querySelector('label span')?.textContent).toBe('Comics')
    expect(
      [...comics.querySelectorAll('.grant-folders code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/Comics'])
  })

  // O193: a library of two folders is one box, and unticking it revokes it.
  it('revokes a library of two folders by its one box', async () => {
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
    expect(tick(host, 70).hasAttribute('aria-describedby')).toBe(false)
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
    expect(control(host, '.grants .empty').textContent).toBe(
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
    const asking = control(host, '.grants .lift-limit')
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
    const asking = control(host, '.grants .lift-limit')
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
            <UserRow
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
    expect(control(host, '.grants .lift-limit p').textContent).toBe(
      'sam would see every library, including any added later.',
    )

    await click(control(host, 'button[name="lift-limit"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { every_library: true })
    expect(radio(host, 'every').checked).toBe(true)
    expect(seesNone(host)).toBeNull()
  })

  it('holds both answers while a write is in flight', async () => {
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
    expect(
      control<HTMLButtonElement>(host, 'button[name="lift-limit"]').disabled,
    ).toBe(true)
    expect(
      control<HTMLButtonElement>(host, 'button[name="keep-limit"]').disabled,
    ).toBe(true)
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
    // A reload that brings nothing, so the card runs on the answers it kept.
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
   * tests press the radio, the card has rendered `saving`.
   */
  async function resetThen(host: HTMLElement, second: Element) {
    await typeInto(
      control<HTMLInputElement>(host, 'input[name="password"]'),
      'hunter2',
    )
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
    expect(control(host, '.grants').textContent).toContain(
      'An administrator sees every library',
    )
    const reader = user({ id: 3, name: 'kim', role: 'user' })
    const other = await show(reader, [me, reader], { actingAs: 1 })
    expect(control(other.host, '.grants').textContent).not.toContain(
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

  it('says there are no libraries rather than drawing an empty editor', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      libraries: [],
    })
    expect(control(host, '.grants').textContent).toContain(
      'There are no libraries yet',
    )
    expect(host.querySelector('.grants input')).toBeNull()
  })

  it('draws the server sentence when a patch is refused', async () => {
    // The other half of §6.1: what the client could not tell in advance
    // arrives as a 409 with a sentence, and the card renders that.
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

import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
import { AccountPage } from '../src/AccountPage'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import type { Library } from '../src/libraries'
import { mount } from './support/mount'
import { row } from './support/settingsRows'
import * as usersApi from '../src/users'

/**
 * The account page under real keys. A radio group chooses on an arrow key as it
 * does on a click, so a key pressed on "Only these" would reach "Every library"
 * and lift the account's limit unasked (O195).
 */

vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  patchUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

const dir = (
  id: number,
  path: string,
  library: { id: number; name: string },
): Library['dirs'][number] => ({
  id,
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
  libraries: [library],
  nested: [],
  inside: [],
})

const MANGA: Library = {
  id: 70,
  name: 'Manga',
  works: 0,
  books: 0,
  dirs: [dir(700, '/media/Manga', { id: 70, name: 'Manga' })],
}
const COMICS: Library = {
  id: 80,
  name: 'Comics',
  works: 0,
  books: 0,
  dirs: [dir(800, '/media/Comics', { id: 80, name: 'Comics' })],
}

const sam: usersApi.User = {
  id: 2,
  name: 'sam',
  role: 'user',
  can_download: true,
  can_stream: true,
  can_stamp: false,
  can_log_in: true,
  wrong_passwords: 0,
  wrong_password_alarm: false,
  sessions: [],
  keys: [],
  every_library: false,
  libraries: [{ id: 70, name: 'Manga' }],
}

async function shown(who: usersApi.User = sam): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter>
        <AccountPage
          user={who}
          users={[who]}
          libraries={[MANGA, COMICS]}
          actingAs={1}
          onChanged={vi.fn()}
        />
      </MemoryRouter>,
    )
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const radio = (host: HTMLElement, value: 'every' | 'only') => {
  const found = row(host, 'Sees').querySelector<HTMLInputElement>(
    `input[type="radio"][value="${value}"]`,
  )
  if (!found) throw new Error(`no ${value} radio`)
  return found
}

const click = (el: Element) =>
  act(async () => {
    await userEvent.click(el)
  })
const keys = (text: string) =>
  act(async () => {
    await userEvent.keyboard(text)
  })

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(usersApi.patchUser).mockResolvedValue({
    ...sam,
    every_library: true,
    libraries: [],
  })
  vi.mocked(usersApi.revokeUserKey).mockResolvedValue(undefined)
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
})

describe('the account page’s choice of libraries, as the browser names it', () => {
  // Each radio keeps its own label, so the row's word and the help reach a
  // screen reader only through the group around them.
  it('names the radio group by its row’s label and describes it by the administrator’s help', async () => {
    const host = await shown({ ...sam, role: 'admin' })
    const group = row(host, 'Sees').querySelector('[role="radiogroup"]')
    if (!group) throw new Error('no radiogroup in the Sees row')
    expect(group).toHaveAccessibleName('Sees')
    expect(group).toHaveAccessibleDescription(
      'An administrator sees every library whatever is chosen here; the choice applies if the role becomes user.',
    )
    expect(radio(host, 'every')).toHaveAccessibleName(
      'Every library, including any added later',
    )
    expect(radio(host, 'only')).toHaveAccessibleName('Only these')
  })
})

describe('the account page under real keys', () => {
  for (const key of ['ArrowUp', 'ArrowLeft', 'ArrowDown', 'ArrowRight']) {
    it(`asks before ${key} on the radios lifts a limit`, async () => {
      const host = await shown()
      const only = radio(host, 'only')
      await click(only) // focuses it; it is already chosen, so nothing changes
      await keys(`{${key}}`)

      expect(usersApi.patchUser).not.toHaveBeenCalled()
      expect(host.querySelector('.lift-limit')).not.toBeNull()
      expect(only.checked).toBe(true)
    })
  }

  it('asks before an arrow lifts an account limited to no library', async () => {
    const host = await shown({ ...sam, libraries: [] })
    const only = radio(host, 'only')
    await click(only)
    await keys('{ArrowUp}')

    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(host.querySelector('.lift-limit')).not.toBeNull()
    expect(only.checked).toBe(true)
  })

  it('lifts the limit from the keyboard: an arrow, Shift+Tab, Enter', async () => {
    const host = await shown()
    await click(radio(host, 'only'))
    await keys('{ArrowUp}')
    // The question takes focus onto the answer that changes nothing, so the
    // account is not left on a radio that reads as unchosen.
    expect(document.activeElement?.getAttribute('name')).toBe('keep-limit')
    await keys('{Shift>}{Tab}{/Shift}')
    expect(document.activeElement?.getAttribute('name')).toBe('lift-limit')
    expect(document.activeElement?.textContent).toBe('See every library')
    await keys('{Enter}')
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { every_library: true })
    expect(host.querySelector('.lift-limit')).toBeNull()
    expect(radio(host, 'every').checked).toBe(true)
    expect(document.activeElement).toBe(radio(host, 'every'))
  })

  it('takes the question away when Only these is chosen again by a key', async () => {
    const host = await shown()
    await click(radio(host, 'only'))
    await keys('{ArrowUp}')
    expect(host.querySelector('.lift-limit')).not.toBeNull()
    // Back to the radio the arrow left, then an arrow to the one that stands.
    await keys('{Shift>}{Tab}{/Shift}{Shift>}{Tab}{/Shift}')
    expect(document.activeElement).toBe(radio(host, 'only'))
    await keys('{ArrowUp}')
    await keys('{ArrowDown}')
    expect(host.querySelector('.lift-limit')).toBeNull()
    expect(usersApi.patchUser).not.toHaveBeenCalled()
  })
})

/**
 * Chromium moves the focus of a control taken out of the page to the
 * body, which jsdom only stands in for. Focus is compared with `toBe`.
 */
describe('the account page’s focus after a Revoke, under real keys', () => {
  it('gives focus, as Enter on each Revoke takes its row, to the next row’s Revoke, and with none left in its list to the heading of Sessions & keys, from which Tab reaches the keys’ first', async () => {
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
    const host = await shown({
      ...sam,
      sessions: [session('aaa', 'Firefox'), session('bbb', 'Safari')],
      keys: [key(9, 'the tablet'), key(10, 'the phone')],
    })
    const named = (name: string) => {
      const found = host.querySelector<HTMLButtonElement>(
        `button[name="${name}"]`,
      )
      if (!found) throw new Error(`no ${name}`)
      return found
    }
    const heading = [...host.querySelectorAll('h2, h3')].find(
      (each) => each.textContent === 'Sessions & keys',
    )
    /** Enter on the focused control, and what it sends lands. */
    const enter = () =>
      act(async () => {
        await userEvent.keyboard('{Enter}')
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })

    // A real click first, so that the keys reach the page, not the frame.
    const password = host.querySelector('input[name="password"]')
    if (!password) throw new Error('no password box')
    await click(password)
    act(() => named('revoke-session-aaa').focus())
    await enter()
    expect(document.activeElement).toBe(named('revoke-session-bbb'))
    await enter()
    expect(document.activeElement).toBe(heading)
    await keys('{Tab}')
    expect(document.activeElement).toBe(named('revoke-key-9'))
    await enter()
    expect(document.activeElement).toBe(named('revoke-key-10'))
    await enter()
    expect(document.activeElement).toBe(heading)
    expect(vi.mocked(usersApi.revokeUserSession).mock.calls).toEqual([
      [2, 'aaa'],
      [2, 'bbb'],
    ])
    expect(vi.mocked(usersApi.revokeUserKey).mock.calls).toEqual([
      [2, 9],
      [2, 10],
    ])
  })
})

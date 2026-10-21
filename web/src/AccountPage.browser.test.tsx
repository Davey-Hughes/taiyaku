import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
import { AccountPage } from './AccountPage'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import './index.css'
import type { Library } from './libraries'
import { mount } from './testing/mount'
import * as usersApi from './users'

/**
 * The account page under real keys. A radio group chooses on an arrow key as it
 * does on a click, so a key pressed on "Only these" reached "Every library"
 * and, until O195, lifted the account's limit unasked.
 */

vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  patchUser: vi.fn(),
}))

const dir = (id: number, path: string): Library['dirs'][number] => ({
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
})

const MANGA: Library = {
  id: 70,
  name: 'Manga',
  works: 0,
  books: 0,
  dirs: [dir(700, '/media/Manga')],
}
const COMICS: Library = {
  id: 80,
  name: 'Comics',
  works: 0,
  books: 0,
  dirs: [dir(800, '/media/Comics')],
}

const sam: usersApi.User = {
  id: 2,
  name: 'sam',
  role: 'user',
  can_download: true,
  can_stream: true,
  can_stamp: false,
  can_log_in: true,
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
  const found = host.querySelector<HTMLInputElement>(
    `.grants input[type="radio"][value="${value}"]`,
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

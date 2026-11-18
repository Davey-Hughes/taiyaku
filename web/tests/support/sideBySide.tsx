/**
 * What `SideBySide.browser.test.tsx` and `SideBySide.scrollbars.browser.test.tsx`
 * share: the fixture both pairs are held with, Configuration mounted inside
 * the real `Shell`, and the measures a breakpoint is read in. The two files
 * run in different launches of Chromium, so neither can hold the other's
 * cases.
 *
 * **Each caller mocks the modules `answerPairs` answers for**: `library.ts`'s
 * `status`, `curation.ts`'s `listProposals`, `config.ts`'s `scanStatus` and
 * `listRoots`, `libraries.ts`' `libraryScanStatus` and `listLibraries`, and
 * `users.ts`' `listUsers`. `vi.mock` is hoisted in the file that calls it, so
 * it cannot be called from here.
 */
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { vi } from 'vitest'
import { page } from 'vitest/browser'
import { Config } from '../../src/Config'
import * as config from '../../src/config'
import * as curation from '../../src/curation'
import { GateContext } from '../../src/gate'
import * as librariesApi from '../../src/libraries'
import * as library from '../../src/library'
import { Shell } from '../../src/Shell'
import * as usersApi from '../../src/users'
import { mount } from './mount'

/** `count` accounts, none an administrator, named `reader-1` onwards. */
export const accounts = (count: number): usersApi.User[] =>
  Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    name: `reader-${i + 1}`,
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

/** A directory of Manga's, by its id. */
export const dir = (id: number): librariesApi.LibraryDir => ({
  id,
  path: `/mnt/user/media 1/Manga/Shelf ${id}`,
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: '2026-10-05 21:14:09',
  last_scan_status: 'ok',
  next_scan_at: null,
  paused_at: null,
  libraries: [{ id: 1, name: 'Manga' }],
  nested: [],
  inside: [],
})

/** Manga, library 1, holding `count` directories, ids 1 onwards. */
export const manga = (count: number): librariesApi.Library => ({
  id: 1,
  name: 'Manga',
  works: 1858,
  books: 0,
  match_by_content: true,
  dirs: Array.from({ length: count }, (_, i) => dir(i + 1)),
})

/** Signed in as none of the accounts, so no title carries "this is you". */
export const who = {
  user_id: 100,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

/**
 * What the shell and both pairs read on mount: `users` for the Users
 * table, `libraries` for the Libraries table, and no scan running.
 */
export function answerPairs(
  users: usersApi.User[],
  libraries: librariesApi.Library[],
) {
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Manga', scanned: true }],
    restricted: false,
  })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  } as never)
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
  // No directory scanning: `useOtherScan` asks once and not again, so the
  // number of reads is the number of pages that make one.
  vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({
      library_id: id,
      held_removals: [],
      moves_held: false,
      derive_running: false,
      scans: [],
      running: false,
      elapsed_secs: null,
      dirs: [],
    }),
  )
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
}

/** Wait `ms` inside `act`, so what lands meanwhile is drawn. */
export const settle = (ms = 120) =>
  act(async () => {
    await new Promise((r) => setTimeout(r, ms))
  })

/**
 * The window at `width` × `height` under what is mounted, and the wait for
 * what the layout makes of it, both inside one `act`: `useFitsPair` sets its
 * state when the new width is measured, a frame or more after the window
 * changes.
 */
export async function resize(width: number, height: number) {
  await act(async () => {
    await page.viewport(width, height)
    await new Promise((r) => setTimeout(r, 120))
  })
}

/**
 * Configuration at `entry`, inside the shell, at a window of `width` ×
 * `height`. The window is set before anything is mounted, so nothing
 * measures it until the mount does.
 */
export async function configAt(
  width: number,
  height: number,
  entry: string,
): Promise<HTMLElement> {
  await page.viewport(width, height)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <GateContext value={{ who, signOut: () => undefined }}>
          <Routes>
            <Route
              path="/config/*"
              element={
                <Shell>
                  <Config />
                </Shell>
              }
            />
          </Routes>
        </GateContext>
      </MemoryRouter>,
    )
    host.style.height = '100%'
  })
  if (!host) throw new Error('nothing mounted')
  await settle()
  await document.fonts.ready
  return host
}

export const need = <T extends Element = HTMLElement>(
  scope: ParentNode,
  selector: string,
): T => {
  const found = scope.querySelector<T>(selector)
  if (found === null) throw new Error(`nothing matches ${selector}`)
  return found
}

/** The used width of a CSS length, as the document's root resolves it. */
export function px(length: string): number {
  const probe = document.createElement('div')
  probe.style.width = length
  document.body.append(probe)
  const width = probe.getBoundingClientRect().width
  probe.remove()
  return width
}

/**
 * The split's width a pair needs, from the spec's terms rather than from the
 * probe `useFitsPair` measures, so a probe sized wrong cannot agree with
 * itself: the list's least column, the gap and the page's measure.
 */
export const needed = () => px('calc(20rem + var(--space-6) + 40rem)')

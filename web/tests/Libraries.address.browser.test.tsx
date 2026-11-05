import { beforeEach, describe, expect, it, vi } from 'vitest'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as config from '../src/config'
import * as librariesApi from '../src/libraries'
import { byRole } from './support/located'
import { click, linkIn, openedAt } from './support/typedAddress'

/**
 * A library's address is its name, escaped where an address needs it: held
 * here through the router the app runs, `BrowserRouter`, at the frame's real
 * address, which is where the browser's own reading of a path, its escapes
 * and its decoding, can differ from the router's.
 */

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  scanStatus: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
}))

/**
 * One name for each thing an address has to escape, with the path the
 * browser holds for its page, written out rather than computed so that a
 * wrong escape cannot agree with itself. `2` is a name, and also the id of
 * `Manga/Chapters`, which an address by id would open instead.
 */
const NAMES: [name: string, segment: string][] = [
  ['Light novels', 'Light%20novels'],
  ['Manga/Chapters', 'Manga%2FChapters'],
  ['What?', 'What%3F'],
  ['No. #1', 'No.%20%231'],
  ['100%', '100%25'],
  ['Text holding %2F', 'Text%20holding%20%252F'],
  ['Tom & Jerry', 'Tom%20%26%20Jerry'],
  ['Émile', '%C3%89mile'],
  ['2', '2'],
]

/** Each name's library, of one directory, numbered from 100. */
const LIBRARIES: librariesApi.Library[] = NAMES.map(([name], i) => ({
  id: i + 1,
  name,
  works: 0,
  books: 0,
  dirs: [
    {
      id: 100 + i,
      path: `/srv/shelf ${i + 1}`,
      edition_key: 'en',
      default_kind: 'manga',
      default_language: 'en',
      exclude_globs: null,
      scan_interval_secs: null,
      keeps_documents: false,
      last_scan_at: null,
      last_scan_status: null,
      nested: [],
      inside: [],
    },
  ],
}))

const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({
    libraries: LIBRARIES,
  })
  vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({
      library_id: id,
      held_removals: [],
      moves_held: false,
      scans: [],
      running: false,
      elapsed_secs: null,
      dirs: [],
    }),
  )
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
})

const titleOf = (host: HTMLElement, page: string) =>
  host.querySelector(`.${page} .page-title`)?.textContent

describe('a library’s address, through the app’s own router', () => {
  it.each(NAMES)(
    'opens %s’s page from its row, its directory’s page from its own, and goes back to that row',
    async (name, segment) => {
      const i = NAMES.findIndex(([one]) => one === name)
      const path = `/srv/shelf ${i + 1}`
      const host = await openedAt('/config/libraries', who)

      await click(linkIn(host.querySelector('.library-table') as Element, name))
      expect(location.pathname).toBe(`/config/libraries/${segment}`)
      expect(titleOf(host, 'library-page')).toBe(name)

      await click(
        byRole(
          linkIn(host.querySelector('.dir-table') as Element, path),
          'link',
        ),
      )
      expect(location.pathname).toBe(
        `/config/libraries/${segment}/dirs/${100 + i}`,
      )
      expect(titleOf(host, 'dir-page')).toBe(path)

      await click(
        linkIn(host.querySelector('.dir-page .back') as Element, `‹ ${name}`),
      )
      expect(location.pathname).toBe(`/config/libraries/${segment}`)
      expect(host.querySelector('.dir-page')).toBeNull()
      expect(document.activeElement).toBe(
        linkIn(host.querySelector('.dir-table') as Element, path),
      )

      await click(
        linkIn(
          host.querySelector('.library-page .back') as Element,
          '‹ Libraries',
        ),
      )
      expect(location.pathname).toBe('/config/libraries')
      expect(
        linkIn(host.querySelector('.library-table') as Element, name),
      ).toBeDefined()
    },
  )

  it.each(NAMES)(
    'answers the address of %s’s directory as a reload or a typed address reaches it',
    async (name, segment) => {
      const i = NAMES.findIndex(([one]) => one === name)
      const host = await openedAt(
        `/config/libraries/${segment}/dirs/${100 + i}`,
        who,
      )
      expect(location.pathname).toBe(
        `/config/libraries/${segment}/dirs/${100 + i}`,
      )
      expect(titleOf(host, 'dir-page')).toBe(`/srv/shelf ${i + 1}`)
      expect(
        linkIn(
          host.querySelector('.dir-page .back') as Element,
          `‹ ${name}`,
        ).getAttribute('href'),
      ).toBe(`/config/libraries/${segment}`)
    },
  )

  // The row's link spells the address one way; a typed address may spell
  // the same page another, in another case or with a character the link
  // escapes left bare, and is kept in the bar as typed.
  it.each([
    ['Light novels', 'light%20novels', 0],
    ['Tom & Jerry', 'Tom%20&%20Jerry', 6],
    ['Manga/Chapters', 'manga%2fchapters', 1],
  ])(
    'gives focus back to the row of %s’s directory when the back link closes its page at an address typed as %s',
    async (name, typed, i) => {
      const address = `/config/libraries/${typed}/dirs/${100 + i}`
      const path = `/srv/shelf ${i + 1}`
      const host = await openedAt(address, who)
      expect(location.pathname).toBe(address)
      expect(titleOf(host, 'dir-page')).toBe(path)

      await click(
        linkIn(host.querySelector('.dir-page .back') as Element, `‹ ${name}`),
      )
      expect(host.querySelector('.dir-page')).toBeNull()
      expect(titleOf(host, 'library-page')).toBe(name)
      expect(document.activeElement).toBe(
        linkIn(host.querySelector('.dir-table') as Element, path),
      )
    },
  )
})

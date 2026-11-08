import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as config from '../src/config'
import * as librariesApi from '../src/libraries'
import { deferred } from './support/async'
import { holdOf } from './support/held'
import { counted, dir, idle, scanningIn } from './support/libraries'
import { buttonSaying, byRole } from './support/located'
import { click, linkIn, openedAt } from './support/typedAddress'

/**
 * A library's address is its name, escaped where an address needs it: held
 * here through the router the app runs, `BrowserRouter`, at the frame's real
 * address, which is where the browser's own reading of a path, its escapes
 * and its decoding, can differ from the router's.
 */

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  removalCost: vi.fn(),
  scanStatus: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  unlinkCost: vi.fn(),
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
      libraries: [{ id: i + 1, name }],
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

/**
 * One directory in two libraries is two pages, Manga's and Kids', at one
 * root id: what is open on one, and what holds it, is not the other's.
 */
describe('one directory in two libraries, through the app’s own router', () => {
  const MANGA = { id: 1, name: 'Manga' }
  const KIDS = { id: 2, name: 'Kids' }
  const shared = () => dir(300, '/srv/shared', [MANGA, KIDS])
  const libraries: librariesApi.Library[] = [
    {
      ...MANGA,
      works: 0,
      books: 0,
      dirs: [shared(), dir(301, '/srv/manga', [MANGA])],
    },
    { ...KIDS, works: 0, books: 0, dirs: [shared()] },
  ]

  it('draws Kids’ page of a directory afresh after Manga’s: the removal opened on Manga’s is gone, and Manga’s scan holds nothing there', async () => {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries })
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    // A scan of Manga's other directory holds Manga's Move…; Kids' status
    // is not answered until the end.
    const kids = deferred<librariesApi.LibraryScanStatus>()
    vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
      id === MANGA.id
        ? Promise.resolve(scanningIn(libraries[0], 301))
        : kids.promise,
    )
    const host = await openedAt('/config/libraries/Manga/dirs/300', who)
    const page = () => host.querySelector('.dir-page') as HTMLElement
    expect(holdOf(buttonSaying(page(), 'Move…'))).toBe('held')
    await click(buttonSaying(page(), 'Remove…'))
    expect(page().querySelector('.remove-root')).not.toBeNull()

    await act(async () => {
      history.pushState(null, '', '/config/libraries/Kids/dirs/300')
      window.dispatchEvent(new PopStateEvent('popstate'))
      await new Promise((resolve) => setTimeout(resolve, 120))
    })
    expect(location.pathname).toBe('/config/libraries/Kids/dirs/300')
    expect(titleOf(host, 'dir-page')).toBe('/srv/shared')
    expect(page().querySelector('.back a')?.textContent).toBe('‹ Kids')
    expect(page().querySelector('.remove-root')).toBeNull()
    expect(holdOf(buttonSaying(page(), 'Move…'))).toBe('free')

    await act(async () => {
      kids.resolve(idle(KIDS.id))
      await new Promise((resolve) => setTimeout(resolve, 120))
    })
    expect(holdOf(buttonSaying(page(), 'Move…'))).toBe('free')
  })

  it('goes to Kids’ page of the directory by a real click on Kids in its Also in line, made outside act with Manga’s Remove from Manga open: that is gone, and the line names Manga', async () => {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries })
    vi.mocked(librariesApi.unlinkCost).mockResolvedValue({
      unlinked: {
        loses_sight: [],
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      dry_run: true,
    })
    const host = await openedAt('/config/libraries/Manga/dirs/300', who)
    const page = () => host.querySelector('.dir-page') as HTMLElement
    await click(buttonSaying(page(), 'Remove from Manga…'))
    expect(page().querySelector('.unlink-root .confirm')).not.toBeNull()

    const kids = linkIn(page().querySelector('.also-in') as Element, 'Kids')
    // As the app runs: `BrowserRouter` commits the change of page in a
    // transition, which `act` would hold until it ended.
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      await userEvent.click(kids)
      const until = performance.now() + 2000
      while (
        page()?.querySelector('.back a')?.textContent !== '‹ Kids' &&
        performance.now() < until
      ) {
        await new Promise((resolve) => setTimeout(resolve, 20))
      }
      await new Promise((resolve) => setTimeout(resolve, 120))
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(location.pathname).toBe('/config/libraries/Kids/dirs/300')
    expect(page().querySelector('.back a')?.textContent).toBe('‹ Kids')
    expect(page().querySelector('.unlink-root')).toBeNull()
    expect(page().querySelector('.also-in')?.textContent).toBe(
      'Also in Manga, which shares these settings and scans.',
    )
    expect(buttonSaying(page(), 'Remove from Kids…')).toBeDefined()
  })

  it('puts focus on the heading of Kids’ page of the directory that a real Enter on Kids in its Also in line opens, pressed outside act', async () => {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries })
    const host = await openedAt('/config/libraries/Manga/dirs/300', who)
    const page = () => host.querySelector('.dir-page') as HTMLElement
    const kids = linkIn(page().querySelector('.also-in') as Element, 'Kids')
    // Focused inside the frame, so that the key goes to the page and not to
    // the frame around it.
    await act(async () => kids.focus())
    expect(document.activeElement).toBe(kids)
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      await userEvent.keyboard('{Enter}')
      const until = performance.now() + 2000
      while (
        page()?.querySelector('.back a')?.textContent !== '‹ Kids' &&
        performance.now() < until
      ) {
        await new Promise((resolve) => setTimeout(resolve, 20))
      }
      await new Promise((resolve) => setTimeout(resolve, 120))
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(location.pathname).toBe('/config/libraries/Kids/dirs/300')
    expect(page().querySelector('.back a')?.textContent).toBe('‹ Kids')
    expect(document.activeElement).toBe(page().querySelector('.page-title'))
  })
})

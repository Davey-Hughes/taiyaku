import { act } from 'react'
import {
  MemoryRouter,
  type NavigateFunction,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { emptyDerive } from './testing/derive'
import { Libraries } from './Libraries'
import * as api from './libraries'
import { deferred, flush } from './testing/async'
import { press, submit, typeInto } from './testing/input'
import { fields } from './testing/fields'
import { dir, idle, scanningIn } from './testing/libraries'
import { mount } from './testing/mount'
import { cellValue } from './testing/table'
import { StatusRefreshContext } from './shell'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addDir: vi.fn(),
  createLibrary: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))

const TABLE = '/config/libraries'

/** A library whose directories are numbered `id * 10`, `id * 10 + 1`, …. */
const library = (id: number, name: string, paths: string[]): api.Library => ({
  id,
  name,
  works: 0,
  books: 0,
  dirs: paths.map((p, i) => dir(id * 10 + i, p)),
})

/** Two libraries: Manga, of two directories, and Comics, of one. */
const twoAndOne = () => [
  library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
  library(2, 'Comics', ['/media/Comics']),
]

/** The shell's status read, which the sidebar's libraries are drawn from. */
const refreshed = vi.fn()

let steer: NavigateFunction | undefined
let where = ''

/** The path the screen is at. */
const here = () => where

/**
 * Draws the screen at `entry`, under the route `Config` gives it, and lets
 * the first read of the list, and the reads of the page it draws, answer.
 */
async function at(entry = TABLE) {
  // Hands the router to the test, which is outside it. Declared here for the
  // reason `Library.test.tsx`'s `locationProbe` gives:
  // `useComponentExportOnlyModules` refuses an unexported component at module
  // scope.
  function Steering() {
    steer = useNavigate()
    where = useLocation().pathname
    return null
  }
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <StatusRefreshContext.Provider value={refreshed}>
        <MemoryRouter initialEntries={[entry]}>
          <Routes>
            <Route path={`${TABLE}/*`} element={<Libraries />} />
            <Route path="/config/users" element={<p className="other-tab" />} />
          </Routes>
          <Steering />
        </MemoryRouter>
      </StatusRefreshContext.Provider>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  await flush()
  if (!host) throw new Error('nothing mounted')
  return host
}

/** Moves to `to` as a typed address or a link elsewhere on the app would. */
async function go(to: string) {
  await flush(() => steer?.(to))
  await flush()
}

/**
 * Lets reads answer until `ready` holds, or a bounded number of turns pass:
 * an add answers, then asks for the list, then draws it, which is more turns
 * than one settle gives. The assertion after it still fails if it never does.
 */
async function until(ready: () => boolean) {
  for (let turn = 0; turn < 10 && !ready(); turn++) await flush()
}

/** The link whose text is `text`. */
const link = (host: HTMLElement, text: string) => {
  const found = [...host.querySelectorAll('a')].find(
    (a) => a.textContent === text,
  )
  if (found === undefined) throw new Error(`no link "${text}"`)
  return found
}

/** Follows the link whose text is `text`, and lets the page it opens read. */
async function follow(host: HTMLElement, text: string) {
  await press(link(host, text))
  await flush()
}

/** The names of the table's libraries, in the order drawn. */
const names = (host: HTMLElement) =>
  [...host.querySelectorAll('.library-table tbody tr .row-link')].map(
    (a) => a.textContent,
  )

/** The paths in the library page's Directories table, in the order drawn. */
const paths = (host: HTMLElement) =>
  [...host.querySelectorAll('.dir-table tbody code')].map((c) => c.textContent)

/** The facts the directory page states about its watch, scans and last scan. */
const regime = (host: HTMLElement) =>
  [...host.querySelectorAll('.root-row .regime dd')].map((d) => d.textContent)

const form = (host: HTMLElement) =>
  host.querySelector('form.add-library') as HTMLFormElement
const nameBox = (host: HTMLElement) =>
  form(host).querySelector('input[name="library-name"]') as HTMLInputElement
const pathBox = (host: HTMLElement) =>
  form(host).querySelector('input[name="path"]') as HTMLInputElement
const makeButton = (host: HTMLElement) =>
  form(host).querySelector('button[type="submit"]') as HTMLButtonElement

/** Fills the make page's form with `name` and `path` and sends it. */
async function makeOne(host: HTMLElement, name: string, path: string) {
  await typeInto(nameBox(host), name)
  await typeInto(pathBox(host), path)
  await submit(form(host))
  await flush()
}

/** The button on the page whose text is `text`. */
const button = (host: HTMLElement, text: string) => {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  )
  if (found === undefined) throw new Error(`no button "${text}"`)
  return found
}

async function choose(select: HTMLSelectElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set?.call(select, value)
    select.dispatchEvent(new Event('change', { bubbles: true }))
  })
}

/**
 * Moves the directory whose page is drawn to the library `to`, or to a new one
 * called Extras, through the rehearsal and the confirm.
 */
async function moveDrawnDir(host: HTMLElement, to: string) {
  await press(button(host, 'Move…'))
  await choose(
    host.querySelector('select[name="move-target"]') as HTMLSelectElement,
    to,
  )
  if (to === 'new') {
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      'Extras',
    )
  }
  await press(host.querySelector('button[name="rehearse"]') as Element)
  await press(host.querySelector('.move-root .confirm') as Element)
  await flush()
}

/** Removes the directory whose page is drawn, through its dialog. */
async function removeDrawnDir(host: HTMLElement) {
  await press(button(host, 'Remove…'))
  await press(host.querySelector('.remove-root .confirm') as Element)
  await flush()
}

/** Removes the library whose page is drawn, through its dialog. */
async function removeDrawnLibrary(host: HTMLElement) {
  await press(host.querySelector('button[name="remove-library"]') as Element)
  await press(host.querySelector('.remove-library .confirm') as Element)
  await flush()
}

const locked = () =>
  new Error('/api/library/libraries: 500 {"error":"the store is locked"}')

const removal = (lost = 0, dry_run = false): config.RemovalAnswer => ({
  removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
  derive: { ...emptyDerive, lost_positions: lost },
  dry_run,
})

/** A move to Comics that removed nothing, with `moved` over it. */
const movedAnswer = (
  moved: Partial<api.Moved> = {},
  dry_run = false,
): api.MoveAnswer => ({
  moved: {
    library_id: 2,
    library: 'Comics',
    works_kept: 1,
    works_joined: 0,
    works_split: 0,
    series_joined: 0,
    positions: 0,
    notes: 0,
    alignments_dropped: 0,
    library_removed: null,
    grants_removed: 0,
    left_with_no_library: [],
    ...moved,
  },
  derive: emptyDerive,
  dry_run,
})

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({
      library_id: id,
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
  vi.mocked(api.libraryRemovalCost).mockResolvedValue(removal(0, true))
  vi.mocked(api.removeLibrary).mockResolvedValue(removal(4))
  vi.mocked(config.removalCost).mockResolvedValue(removal(0, true))
  vi.mocked(config.removeRoot).mockResolvedValue(removal(4))
})

describe('Libraries, the table', () => {
  it('heads itself, and says so when there is no library yet', async () => {
    const host = await at()
    expect(host.querySelector('.library-table > h2')?.textContent).toBe(
      'Libraries',
    )
    expect(
      host.querySelector('.library-table > .empty')?.textContent,
    ).toContain('No libraries yet')
    expect(host.querySelector('.library-table table')).toBeNull()
  })

  it('draws a row per library, in the order sent, and offers a directory the other libraries to move to', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(2, 'Comics', ['/media/Comics']),
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
      ],
    })
    const host = await at()
    expect(names(host)).toEqual(['Comics', 'Manga'])
    expect(host.querySelector('.empty')).toBeNull()

    await go('/config/libraries/1/dirs/10')
    await press(button(host, 'Move…'))
    const options = [
      ...(host.querySelector('select[name="move-target"]') as HTMLSelectElement)
        .options,
    ].map((o) => o.textContent)
    expect(options).toContain('Comics')
    expect(options).not.toContain('Manga')
  })

  it('draws a table of Name, Directories, Works and Last scan, each row a link to its library’s page through its name', async () => {
    const manga = library(1, 'Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/C',
    ])
    manga.works = 1
    const comics = library(2, 'Comics', ['/media/Comics'])
    comics.works = 12
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [comics, manga],
    })
    const host = await at()
    const table = host.querySelector('.library-table table') as HTMLElement
    expect(
      [...table.querySelectorAll('thead th')].map((th) => [
        th.textContent,
        th.getAttribute('scope'),
      ]),
    ).toEqual([
      ['Name', 'col'],
      ['Directories', 'col'],
      ['Works', 'col'],
      ['Last scan', 'col'],
    ])
    const rows = [...table.querySelectorAll('tbody tr')].map((tr) =>
      [...tr.querySelectorAll('td')].map(cellValue),
    )
    expect(rows).toEqual([
      ['Comics', '1 directory', '12 works', 'Never'],
      ['Manga', '2 directories', '1 work', 'Never'],
    ])
    // The row's one link is its name, and nothing else in the row is one.
    for (const tr of table.querySelectorAll('tbody tr')) {
      expect(tr.querySelectorAll('a')).toHaveLength(1)
    }
    expect(link(host, 'Manga').getAttribute('href')).toBe('/config/libraries/1')

    await follow(host, 'Manga')
    expect(here()).toBe('/config/libraries/1')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
  })

  it('says when each library was last scanned: its latest directory’s scan, with the outcome when that was not ok, or Never', async () => {
    const scanned = (
      id: number,
      at: string | null,
      status: string | null,
    ): config.Root => ({
      ...dir(id, `/media/${id}`),
      last_scan_at: at,
      last_scan_status: status,
    })
    const latest: api.Library = {
      ...library(1, 'Latest', []),
      dirs: [
        scanned(10, '2026-10-01 10:00:00', 'ok'),
        scanned(11, null, null),
        scanned(12, '2026-10-03 09:00:00', 'ok'),
        scanned(13, '2026-10-02 23:59:59', 'failed: disk gone'),
      ],
    }
    const failed: api.Library = {
      ...library(2, 'Failed', []),
      dirs: [scanned(20, '2026-10-04 08:00:00', 'failed: disk gone')],
    }
    const unrecorded: api.Library = {
      ...library(3, 'Unrecorded', []),
      dirs: [scanned(30, '2026-10-05 07:00:00', null)],
    }
    const never: api.Library = {
      ...library(4, 'Never', []),
      dirs: [scanned(40, null, null), scanned(41, null, null)],
    }
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [latest, failed, unrecorded, never],
    })
    const host = await at()
    const lastScans = [
      ...host.querySelectorAll('.library-table tbody tr td:last-child'),
    ].map(cellValue)
    expect(lastScans).toEqual([
      '2026-10-03 09:00:00',
      '2026-10-04 08:00:00 · failed: disk gone',
      '2026-10-05 07:00:00 · outcome unrecorded',
      'Never',
    ])
  })

  it('draws the alert, and neither a table nor the no-library line, when the list cannot be read', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    const host = await at()
    expect(
      host.querySelector('.library-table > [role="alert"]')?.textContent,
    ).toBe('the store is locked')
    expect(host.querySelector('.library-table table')).toBeNull()
    expect(host.querySelector('.library-table .empty')).toBeNull()
  })

  it('says why when the list cannot be read', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 the store is locked'),
    )
    const host = await at()
    expect(
      host.querySelector('.library-table > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })

  it('draws the list’s failure as the server’s sentence', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    const host = await at()
    expect(
      host.querySelector('.library-table > [role="alert"]')?.textContent,
    ).toBe('the store is locked')
  })
})

describe('Libraries, the pages', () => {
  it('reads the list again on each change of page', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    const host = await at()
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
    await follow(host, 'Manga')
    expect(api.listLibraries).toHaveBeenCalledTimes(2)
    await follow(host, '/media/Manga')
    expect(here()).toBe('/config/libraries/1/dirs/10')
    expect(api.listLibraries).toHaveBeenCalledTimes(3)
    await follow(host, '‹ Manga')
    expect(api.listLibraries).toHaveBeenCalledTimes(4)
    await follow(host, '‹ Libraries')
    expect(api.listLibraries).toHaveBeenCalledTimes(5)
    await follow(host, 'New library')
    expect(here()).toBe('/config/libraries/new')
    expect(api.listLibraries).toHaveBeenCalledTimes(6)
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    expect(api.listLibraries).toHaveBeenCalledTimes(7)
  })

  it('says there is no such library, with the way back to the table, for an id the list does not hold', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    const host = await at('/config/libraries/9')
    expect(host.querySelector('.library-page > h2')).toBeNull()
    expect(host.querySelector('.missing')?.textContent).toBe('No such library.')
    expect(link(host, '‹ Libraries').getAttribute('href')).toBe(TABLE)
  })

  it('says there is no such directory, with the way back to its library, for a directory id the library does not hold', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    // Directory 20 is Comics', not Manga's.
    const host = await at('/config/libraries/1/dirs/20')
    expect(host.querySelector('.root-row')).toBeNull()
    expect(host.querySelector('.missing')?.textContent).toBe(
      'No such directory in this library.',
    )
    expect(link(host, '‹ Manga').getAttribute('href')).toBe(
      '/config/libraries/1',
    )
  })

  it.each([
    { what: 'a library', entry: '/config/libraries/9' },
    { what: 'a directory', entry: '/config/libraries/1/dirs/20' },
  ])(
    'puts focus on the line that says there is no such $what',
    async ({ entry }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({
        libraries: [library(1, 'Manga', ['/media/Manga'])],
      })
      const host = await at(entry)
      const missing = host.querySelector('.missing')
      expect(missing).not.toBeNull()
      expect(document.activeElement).toBe(missing)
    },
  )

  it.each([
    {
      what: 'library',
      first: '/config/libraries/77',
      second: '/config/libraries/78',
    },
    {
      what: 'directory',
      first: '/config/libraries/1/dirs/77',
      second: '/config/libraries/1/dirs/78',
    },
  ])(
    'puts focus on the no-such-$what line again when a second unknown id follows the first',
    async ({ first, second }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({
        libraries: [library(1, 'Manga', ['/media/Manga'])],
      })
      const host = await at(first)
      expect(document.activeElement).toBe(host.querySelector('.missing'))
      act(() => (document.activeElement as HTMLElement).blur())
      await go(second)
      expect(here()).toBe(second)
      const missing = host.querySelector('.missing')
      expect(missing).not.toBeNull()
      expect(document.activeElement).toBe(missing)
    },
  )

  it('puts focus on the line that says there is no such library when a read finds the one drawn gone', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    const host = await at('/config/libraries/1')
    expect(host.querySelector('.missing')).toBeNull()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
    await follow(host, '/media/Manga')
    const missing = host.querySelector('.missing')
    expect(missing?.textContent).toBe('No such library.')
    expect(document.activeElement).toBe(missing)
  })

  it.each([
    {
      what: 'a library’s removal goes to the table',
      from: '/config/libraries/1',
      to: TABLE,
      page: '.library-table',
      act: removeDrawnLibrary,
    },
    {
      what: 'a directory’s removal goes to its library’s page',
      from: '/config/libraries/1/dirs/11',
      to: '/config/libraries/1',
      page: '.library-page',
      act: removeDrawnDir,
    },
    {
      what: 'the removal of a library’s last directory goes to the table',
      from: '/config/libraries/2/dirs/20',
      to: TABLE,
      page: '.library-table',
      act: removeDrawnDir,
    },
    {
      what: 'a directory’s move goes to the page of the library it left',
      from: '/config/libraries/1/dirs/11',
      to: '/config/libraries/1',
      page: '.library-page',
      act: (host: HTMLElement) => moveDrawnDir(host, '2'),
      moved: {},
    },
    {
      what: 'a move that took its library goes to the table',
      from: '/config/libraries/2/dirs/20',
      to: TABLE,
      page: '.library-table',
      act: (host: HTMLElement) => moveDrawnDir(host, '1'),
      moved: { library_id: 1, library: 'Manga', library_removed: 'Comics' },
    },
  ])(
    '$what, with its report drawn and focused there, and reads the list once',
    async ({ from, to, page, act: run, moved }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
      if (moved !== undefined) {
        vi.mocked(api.moveCost).mockResolvedValue(movedAnswer(moved, true))
        vi.mocked(api.moveDir).mockResolvedValue(movedAnswer(moved))
      }
      const host = await at(from)
      vi.mocked(api.listLibraries).mockClear()
      await run(host)
      expect(here()).toBe(to)
      const report = host.querySelector(`${page} > .removal-report`)
      expect(report).not.toBeNull()
      expect(document.activeElement).toBe(report)
      // The read the change of page makes is the one that follows the write.
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
    },
  )

  it.each([
    {
      what: 'removal',
      act: removeDrawnDir,
      said: 'What re-deriving after removing /media/Manga/Chapters found:',
    },
    {
      what: 'move',
      act: (host: HTMLElement) => moveDrawnDir(host, '2'),
      said: 'Moved /media/Manga/Chapters from Manga to Comics.',
    },
  ])(
    'names the directory by its path in the report its $what draws a page up',
    async ({ act: run, said }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
      vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
      vi.mocked(api.moveDir).mockResolvedValue(movedAnswer())
      const host = await at('/config/libraries/1/dirs/11')
      await run(host)
      expect(
        host.querySelector('.library-page > .removal-report > p')?.textContent,
      ).toBe(said)
    },
  )

  it.each([
    {
      by: 'the browser’s Back',
      leave: async (_host: HTMLElement) => {
        await flush(() => steer?.(-1))
        await flush()
      },
      to: '/config/libraries/1',
    },
    {
      by: 'a link',
      leave: (host: HTMLElement) => follow(host, '‹ Manga'),
      to: '/config/libraries/1',
    },
    {
      by: 'another tab',
      leave: (_host: HTMLElement) => go('/config/users'),
      to: '/config/users',
    },
  ])(
    'saves the directory settings typed and not yet left when the directory’s page goes by $by',
    async ({ leave, to }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({
        libraries: [library(1, 'Manga', ['/media/Manga'])],
      })
      vi.mocked(config.patchRoot).mockResolvedValue(undefined)
      const host = await at('/config/libraries/1')
      await follow(host, '/media/Manga')
      await typeInto(
        host.querySelector(
          'input[name="default_language"]',
        ) as HTMLInputElement,
        'ja',
      )
      await typeInto(
        host.querySelector(
          'input[name="scan_interval_secs"]',
        ) as HTMLInputElement,
        '600',
      )
      expect(config.patchRoot).not.toHaveBeenCalled()

      await leave(host)
      expect(here()).toBe(to)
      expect(host.querySelector('.root-row')).toBeNull()
      expect(config.patchRoot).toHaveBeenCalledTimes(2)
      expect(config.patchRoot).toHaveBeenCalledWith(10, {
        default_language: 'ja',
      })
      expect(config.patchRoot).toHaveBeenCalledWith(10, {
        scan_interval_secs: 600,
      })
    },
  )

  it('drops a rename typed and not saved when the library’s page is left, and sends nothing', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    const host = await at('/config/libraries/1')
    await press(button(host, 'Rename…'))
    await typeInto(
      host.querySelector(
        'form.rename-library input[name="library-name"]',
      ) as HTMLInputElement,
      'Comics',
    )
    await follow(host, '‹ Libraries')
    expect(here()).toBe(TABLE)
    await follow(host, 'Manga')
    expect(api.renameLibrary).not.toHaveBeenCalled()
    expect(host.querySelector('form.rename-library')).toBeNull()
  })

  it('says a directory setting refused as its page went on the page the person went to, without taking focus, and drops it on the next change of page', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    vi.mocked(config.patchRoot).mockRejectedValue(
      new Error('/api/library/roots/10: 500 {"error":"the store is locked"}'),
    )
    const host = await at('/config/libraries/1/dirs/10')
    await typeInto(
      host.querySelector('input[name="default_language"]') as HTMLInputElement,
      'ja',
    )
    await follow(host, '‹ Manga')
    expect(here()).toBe('/config/libraries/1')
    expect(config.patchRoot).toHaveBeenCalledTimes(1)
    const said = host.querySelector('.library-page > [role="alert"]')
    expect(said?.textContent).toBe(
      'Default language for /media/Manga was not saved (the store is locked).',
    )
    expect(document.activeElement).not.toBe(said)

    await follow(host, '‹ Libraries')
    expect(host.querySelector('[role="alert"]')).toBeNull()
    // Nor drawn again on the page it was said on.
    await follow(host, 'Manga')
    expect(here()).toBe('/config/libraries/1')
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('still reads the list after a save on a directory’s page that follows a directory’s removal', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    const host = await at('/config/libraries/1/dirs/11')
    await removeDrawnDir(host)
    await follow(host, '/media/Manga/Volumes')
    vi.mocked(api.listLibraries).mockClear()
    await press(host.querySelector('input[name="keeps_documents"]') as Element)
    await flush()
    expect(config.patchRoot).toHaveBeenCalledWith(10, { keeps_documents: true })
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
  })
})

describe('Libraries, an answer that comes after the person has moved on', () => {
  /**
   * Starts the removal or the move of Manga's second directory from its page,
   * and holds the answer until `answer` lets it go. `answer` counts the
   * reads of the list from then on.
   */
  async function started(kind: 'removal' | 'move') {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const removed = deferred<config.RemovalAnswer>()
    const moved = deferred<api.MoveAnswer>()
    vi.mocked(config.removeRoot).mockReturnValue(removed.promise)
    vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
    vi.mocked(api.moveDir).mockReturnValue(moved.promise)
    const host = await at('/config/libraries/1/dirs/11')
    if (kind === 'removal') {
      await press(button(host, 'Remove…'))
      await press(host.querySelector('.remove-root .confirm') as Element)
      expect(config.removeRoot).toHaveBeenCalledTimes(1)
    } else {
      await press(button(host, 'Move…'))
      await choose(
        host.querySelector('select[name="move-target"]') as HTMLSelectElement,
        '2',
      )
      await press(host.querySelector('button[name="rehearse"]') as Element)
      await press(host.querySelector('.move-root .confirm') as Element)
      expect(api.moveDir).toHaveBeenCalledTimes(1)
    }
    const answer = async () => {
      vi.mocked(api.listLibraries).mockClear()
      refreshed.mockClear()
      await flush(() =>
        kind === 'removal'
          ? removed.resolve(removal(4))
          : moved.resolve(movedAnswer()),
      )
      await flush()
    }
    return { host, answer }
  }

  const said = {
    removal: 'What re-deriving after removing /media/Manga/Chapters found:',
    move: 'Moved /media/Manga/Chapters from Manga to Comics.',
  }

  it.each(['removal', 'move'] as const)(
    'draws a directory’s %s report on the library page the person already went up to, leaves focus where they put it, reads the list once, and holds back no later read',
    async (kind) => {
      const { host, answer } = await started(kind)
      await follow(host, '‹ Manga')
      expect(here()).toBe('/config/libraries/1')
      const before = host.querySelector(
        'button[name="scan-library"]',
      ) as HTMLButtonElement
      act(() => before.focus())
      await answer()

      expect(here()).toBe('/config/libraries/1')
      const report = host.querySelector(
        '.library-page > [aria-live="polite"] > .removal-report',
      )
      expect(report?.querySelector(':scope > p')?.textContent).toBe(said[kind])
      expect(document.activeElement).toBe(before)
      expect(api.listLibraries).toHaveBeenCalledTimes(1)

      // A save on a directory's page after it still reads the list.
      vi.mocked(config.patchRoot).mockResolvedValue(undefined)
      await follow(host, '/media/Manga/Volumes')
      vi.mocked(api.listLibraries).mockClear()
      await press(
        host.querySelector('input[name="keeps_documents"]') as Element,
      )
      await flush()
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
    },
  )

  it.each(['removal', 'move'] as const)(
    'leaves the person on the table when a directory’s %s answers there, draws its report there without taking focus, and reads the list once',
    async (kind) => {
      const { host, answer } = await started(kind)
      await follow(host, '‹ Manga')
      await follow(host, '‹ Libraries')
      expect(here()).toBe(TABLE)
      const before = link(host, 'New library')
      act(() => before.focus())
      await answer()
      expect(here()).toBe(TABLE)
      const report = host.querySelector(
        '.library-table > [aria-live="polite"] > .removal-report',
      )
      expect(report?.querySelector(':scope > p')?.textContent).toBe(said[kind])
      expect(document.activeElement).toBe(before)
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
    },
  )

  it.each(['removal', 'move'] as const)(
    'leaves the person on another library’s page when a directory’s %s answers there, draws its report there without taking focus, and drops it on the next change of page',
    async (kind) => {
      const { host, answer } = await started(kind)
      await go('/config/libraries/2')
      const before = host.querySelector(
        'button[name="scan-library"]',
      ) as HTMLButtonElement
      act(() => before.focus())
      // Nothing takes focus to have it read, so it lands in a live region,
      // which is drawn with the page, empty, before the answer comes: a
      // region drawn together with what it says is often not read.
      const region = host.querySelector('.library-page > [aria-live="polite"]')
      expect(region).not.toBeNull()
      expect(region?.childNodes).toHaveLength(0)
      await answer()
      expect(here()).toBe('/config/libraries/2')
      expect(host.querySelector('.library-page > h2')?.textContent).toBe(
        'Comics',
      )
      expect(host.querySelector('.library-page > [aria-live="polite"]')).toBe(
        region,
      )
      const report = region?.querySelector(':scope > .removal-report')
      expect(report?.querySelector(':scope > p')?.textContent).toBe(said[kind])
      expect(document.activeElement).toBe(before)
      // Still the group its opening paragraph names.
      expect(report?.getAttribute('role')).toBe('group')
      expect(
        document.getElementById(report?.getAttribute('aria-labelledby') ?? ''),
      ).toBe(report?.querySelector(':scope > p'))
      expect(api.listLibraries).toHaveBeenCalledTimes(1)

      // Not drawn again on the page it would have gone to.
      await go('/config/libraries/1')
      expect(host.querySelector('.library-page > h2')?.textContent).toBe(
        'Manga',
      )
      expect(host.querySelector('.removal-report')).toBeNull()
    },
  )

  it.each(['removal', 'move'] as const)(
    'leaves focus, and what is typed, in the rename box of the page the person went up to when a directory’s %s answers there',
    async (kind) => {
      const { host, answer } = await started(kind)
      await follow(host, '‹ Manga')
      await press(button(host, 'Rename…'))
      const box = host.querySelector(
        'form.rename-library input[name="library-name"]',
      ) as HTMLInputElement
      act(() => box.focus())
      await typeInto(box, 'Manga, all of it')
      await answer()
      expect(
        host.querySelector(
          '.library-page > [aria-live="polite"] > .removal-report',
        ),
      ).not.toBeNull()
      expect(document.activeElement).toBe(box)
      expect(box.value).toBe('Manga, all of it')
    },
  )

  it.each(['removal', 'move'] as const)(
    'leaves the person on another Configuration tab when a directory’s %s answers there, and reads the list once',
    async (kind) => {
      const { host, answer } = await started(kind)
      await go('/config/users')
      expect(host.querySelector('.other-tab')).not.toBeNull()
      await answer()
      expect(here()).toBe('/config/users')
      expect(host.querySelector('.other-tab')).not.toBeNull()
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
      // The sidebar's libraries follow the write from any page.
      expect(refreshed).toHaveBeenCalledTimes(1)
    },
  )

  it.each(['removal', 'move'] as const)(
    'goes up from a directory page the person left and opened again before its %s answered, with the report, and reads the list once',
    async (kind) => {
      const { host, answer } = await started(kind)
      await follow(host, '‹ Manga')
      await follow(host, '/media/Manga/Chapters')
      expect(here()).toBe('/config/libraries/1/dirs/11')
      await answer()
      expect(here()).toBe('/config/libraries/1')
      const report = host.querySelector('.library-page > .removal-report')
      expect(report?.querySelector(':scope > p')?.textContent).toBe(said[kind])
      expect(document.activeElement).toBe(report)
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
    },
  )
})

describe('Libraries, a write refused once its page has gone', () => {
  const MANGA = '/config/libraries/1'
  const CHAPTERS = '/config/libraries/1/dirs/11'
  const box = (host: HTMLElement, name: string) =>
    host.querySelector(`[name="${name}"]`) as HTMLInputElement

  it.each([
    {
      what: 'a rename',
      entry: MANGA,
      held: () => vi.mocked(api.renameLibrary),
      start: async (host: HTMLElement) => {
        await press(button(host, 'Rename…'))
        await typeInto(
          host.querySelector(
            'form.rename-library input[name="library-name"]',
          ) as HTMLInputElement,
          'Comix',
        )
        await submit(host.querySelector('form.rename-library') as Element)
      },
      said: 'Manga was not renamed to Comix (the store is locked).',
    },
    {
      what: 'an added directory',
      entry: MANGA,
      held: () => vi.mocked(api.addDir),
      start: async (host: HTMLElement) => {
        await press(box(host, 'add-dir'))
        await typeInto(box(host, 'path'), '/media/Extras')
        await press(box(host, 'pick'))
      },
      said: '/media/Extras was not added to Manga (the store is locked).',
    },
    {
      what: 'a scan of the library',
      entry: MANGA,
      held: () => vi.mocked(api.startLibraryScan),
      start: (host: HTMLElement) => press(box(host, 'scan-library')),
      said: 'A scan of Manga did not start (the store is locked).',
    },
    {
      what: 'the library’s removal',
      entry: MANGA,
      held: () => vi.mocked(api.removeLibrary),
      start: async (host: HTMLElement) => {
        await press(box(host, 'remove-library'))
        await press(host.querySelector('.remove-library .confirm') as Element)
      },
      said: 'Manga was not removed (the store is locked).',
    },
    {
      what: 'a directory setting left by its box',
      entry: CHAPTERS,
      held: () => vi.mocked(config.patchRoot),
      start: async (host: HTMLElement) => {
        await typeInto(box(host, 'default_language'), 'ja')
        await flush(() =>
          box(host, 'default_language').dispatchEvent(
            new FocusEvent('focusout', { bubbles: true }),
          ),
        )
      },
      said: 'Default language for /media/Manga/Chapters was not saved (the store is locked).',
    },
    {
      what: 'the directory’s checkbox',
      entry: CHAPTERS,
      held: () => vi.mocked(config.patchRoot),
      start: (host: HTMLElement) => press(box(host, 'keeps_documents')),
      said: 'Keeps documents under .taiyaku/ for /media/Manga/Chapters was not saved (the store is locked).',
    },
    {
      what: 'a scan of the directory',
      entry: CHAPTERS,
      held: () => vi.mocked(config.startScan),
      start: (host: HTMLElement) => press(button(host, 'Scan directory')),
      said: 'A scan of /media/Manga/Chapters did not start (the store is locked).',
    },
    {
      what: 'the directory’s removal',
      entry: CHAPTERS,
      held: () => vi.mocked(config.removeRoot),
      start: removeDrawnDir,
      said: '/media/Manga/Chapters was not removed (the store is locked).',
    },
    {
      what: 'the directory’s move',
      entry: CHAPTERS,
      held: () => vi.mocked(api.moveDir),
      start: (host: HTMLElement) => moveDrawnDir(host, '2'),
      said: '/media/Manga/Chapters was not moved from Manga (the store is locked).',
    },
  ])(
    'says $what refused once its page has gone on the page the person went to, without taking focus',
    async ({ entry, held, start, said }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
      vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
      const write = deferred<never>()
      held().mockReturnValue(write.promise)
      const host = await at(entry)
      await start(host)
      expect(held()).toHaveBeenCalledTimes(1)
      await follow(host, entry === MANGA ? '‹ Libraries' : '‹ Manga')
      const page = entry === MANGA ? '.library-table' : '.library-page'
      expect(host.querySelector(page)).not.toBeNull()
      const before = host.querySelector(`${page} a`) as HTMLAnchorElement
      act(() => before.focus())
      await flush(() =>
        write.reject(
          new Error('/api/library: 500 {"error":"the store is locked"}'),
        ),
      )
      await flush()
      expect(
        [...host.querySelectorAll(`${page} > [role="alert"]`)].map(
          (p) => p.textContent,
        ),
      ).toEqual([said])
      expect(document.activeElement).toBe(before)
    },
  )
})

describe('Libraries, making one', () => {
  it.each([
    {
      where: 'still on the make page',
      leave: async (_host: HTMLElement) => {},
      to: '/config/libraries/1',
    },
    {
      where: 'on the table',
      leave: (host: HTMLElement) => follow(host, '‹ Libraries'),
      to: TABLE,
    },
    {
      where: 'on another library’s page',
      leave: (_host: HTMLElement) => go('/config/libraries/2'),
      to: '/config/libraries/2',
    },
    {
      where: 'on another Configuration tab',
      leave: (_host: HTMLElement) => go('/config/users'),
      to: '/config/users',
    },
  ])(
    'leaves the person at $to when a make answers while they are $where, and reads the list once',
    async ({ leave, to }) => {
      const comics = library(2, 'Comics', ['/media/Comics'])
      const made = library(1, 'Manga', ['/media/Manga'])
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [comics] })
      const create = deferred<api.Created>()
      vi.mocked(api.createLibrary).mockReturnValue(create.promise)
      const host = await at('/config/libraries/new')
      await makeOne(host, 'Manga', '/media/Manga')
      await leave(host)

      vi.mocked(api.listLibraries).mockResolvedValue({
        libraries: [comics, made],
      })
      vi.mocked(api.listLibraries).mockClear()
      refreshed.mockClear()
      await flush(() => create.resolve({ library: made, added: [] }))
      await flush()
      expect(here()).toBe(to)
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
      // The sidebar's libraries follow the make from any page.
      expect(refreshed).toHaveBeenCalledTimes(1)
      if (to === TABLE) expect(names(host)).toEqual(['Comics', 'Manga'])
      if (to === '/config/libraries/1') {
        expect(host.querySelector('.library-page > h2')?.textContent).toBe(
          'Manga',
        )
      }
    },
  )

  it('leaves a fresh make form where it is when the make sent from an earlier one answers, and reads the list once', async () => {
    const create = deferred<api.Created>()
    vi.mocked(api.createLibrary).mockReturnValue(create.promise)
    const host = await at('/config/libraries/new')
    await makeOne(host, 'First', '/media/first')
    await follow(host, '‹ Libraries')
    await follow(host, 'New library')
    await typeInto(nameBox(host), 'Second, half typed')
    expect(here()).toBe('/config/libraries/new')

    vi.mocked(api.listLibraries).mockClear()
    await flush(() =>
      create.resolve({
        library: library(3, 'First', ['/media/first']),
        added: [],
      }),
    )
    await flush()
    expect(here()).toBe('/config/libraries/new')
    expect(nameBox(host).value).toBe('Second, half typed')
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
  })

  it('makes a library from a name and one directory, goes to its page, and keeps the directory’s probe for the directory’s page', async () => {
    const made = library(1, 'Manga', ['/media/Manga/Volumes'])
    vi.mocked(api.createLibrary).mockResolvedValue({
      library: made,
      added: [
        {
          root: made.dirs[0],
          watching: false,
          watch_reason: 'no events',
          interval_secs: 3600,
        },
      ],
    })
    const host = await at('/config/libraries/new')
    expect(makeButton(host).disabled).toBe(true)
    await typeInto(nameBox(host), 'Manga')
    expect(makeButton(host).disabled).toBe(true)
    await typeInto(pathBox(host), '/media/Manga/Volumes')
    expect(makeButton(host).disabled).toBe(false)

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await submit(form(host))
    await flush()

    expect(api.createLibrary).toHaveBeenCalledWith('Manga', [
      '/media/Manga/Volumes',
    ])
    expect(here()).toBe('/config/libraries/1')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
    await follow(host, '/media/Manga/Volumes')
    expect(regime(host)).toContain('probe failed: no events')
  })

  it('puts focus on the made library’s heading, and only once', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(here()).toBe('/config/libraries/1')
    const heading = host.querySelector('.library-page > h2') as HTMLElement
    expect(heading.textContent).toBe('Manga')
    expect(document.activeElement).toBe(heading)

    // Back to the page by a link later: the heading is not taken again.
    await follow(host, '‹ Libraries')
    await follow(host, 'Manga')
    expect(document.activeElement).not.toBe(
      host.querySelector('.library-page > h2'),
    )
  })

  it('puts focus on the table’s heading when the make form is cancelled', async () => {
    const host = await at('/config/libraries/new')
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    const heading = host.querySelector('.library-table > h2')
    expect(heading?.textContent).toBe('Libraries')
    expect(document.activeElement).toBe(heading)
  })

  it('leaves focus alone on the table the person goes back to from the made library’s page', async () => {
    // The arrival belongs to the page it sent the person to, so the table
    // opened from there is opened by the person.
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(document.activeElement).toBe(
      host.querySelector('.library-page > h2'),
    )
    const back = link(host, '‹ Libraries')
    act(() => back.focus())
    await follow(host, '‹ Libraries')
    expect(here()).toBe(TABLE)
    expect(document.activeElement).toBe(document.body)
  })

  it('leaves the made library’s heading alone when the make answered after the person left its form', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    const create = deferred<api.Created>()
    vi.mocked(api.createLibrary).mockReturnValue(create.promise)
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '/media/Manga')
    await follow(host, '‹ Libraries')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await flush(() => create.resolve({ library: made, added: [] }))
    await flush()
    await follow(host, 'Manga')
    expect(here()).toBe('/config/libraries/1')
    expect(document.activeElement).not.toBe(
      host.querySelector('.library-page > h2'),
    )
  })

  it.each([
    {
      where: 'the table',
      leave: (host: HTMLElement) => follow(host, '‹ Libraries'),
      page: '.library-table',
    },
    {
      where: 'a fresh make form',
      leave: async (host: HTMLElement) => {
        await follow(host, '‹ Libraries')
        await follow(host, 'New library')
      },
      page: '.new-library',
    },
  ])(
    'says a make refused after its form was left on $where, without taking focus, and drops it on the next change of page',
    async ({ leave, page }) => {
      vi.mocked(api.listLibraries).mockResolvedValue({
        libraries: [library(2, 'Comics', ['/media/Comics'])],
      })
      const create = deferred<api.Created>()
      vi.mocked(api.createLibrary).mockReturnValue(create.promise)
      const host = await at('/config/libraries/new')
      await makeOne(host, 'Manga', '/media/typo')
      await leave(host)
      const before = host.querySelector(`${page} a`) as HTMLAnchorElement
      act(() => before.focus())
      await flush(() =>
        create.reject(
          new Error(
            '/api/library/libraries: 400 {"error":"/media/typo: No such file or directory (os error 2)"}',
          ),
        ),
      )
      await flush()
      const said = host.querySelector(`${page} > [role="alert"]`)
      expect(said?.textContent).toBe(
        'The library Manga was not made (/media/typo: No such file or directory (os error 2)).',
      )
      expect(document.activeElement).toBe(before)

      await go('/config/libraries/2')
      expect(host.querySelector('.library-page > h2')?.textContent).toBe(
        'Comics',
      )
      expect(host.querySelector('[role="alert"]')).toBeNull()
      // Nor drawn again on the table.
      await go(TABLE)
      expect(host.querySelector('[role="alert"]')).toBeNull()
    },
  )

  it('sends the whole path when Enter follows the last keystroke before the form has drawn it', async () => {
    const made = library(1, 'Trial', ['/media/Trial-a'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    // By its label, not its name: the form reads the box by the name the
    // picker gives it, and a box renamed under it must fail here.
    const box = form(host).querySelector(
      'input[aria-label="Directory path"]',
    ) as HTMLInputElement
    await typeInto(nameBox(host), 'Trial')
    await typeInto(box, '/media/Trial-')
    // One turn: the box draws its keystroke at once, and the form learns of
    // it a render later, after the submit.
    await flush(() => {
      Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set?.call(box, '/media/Trial-a')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      form(host).dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
    })
    expect(api.createLibrary).toHaveBeenCalledTimes(1)
    expect(api.createLibrary).toHaveBeenCalledWith('Trial', ['/media/Trial-a'])
  })

  it('sends nothing, and keeps the form as it is, when the form holds no box by the picker’s name', async () => {
    const host = await at('/config/libraries/new')
    await typeInto(nameBox(host), 'Manga')
    const box = pathBox(host)
    await typeInto(box, '/media/Manga')
    box.removeAttribute('name')
    await submit(form(host))
    expect(api.createLibrary).not.toHaveBeenCalled()
    expect(here()).toBe('/config/libraries/new')
    expect(nameBox(host).value).toBe('Manga')
    expect(box.value).toBe('/media/Manga')
  })

  it('sends the directory’s path without the white space around it', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '  /media/Manga \t')
    expect(api.createLibrary).toHaveBeenCalledWith('Manga', ['/media/Manga'])
  })

  it('holds Make the library, and says nothing of why, while the name or the directory box is empty', async () => {
    // The box's path is the directory sent, so there is no path waiting to be
    // added and nothing to explain: what is missing is an empty box in view.
    const host = await at('/config/libraries/new')
    const said = () => form(host).querySelectorAll('.why').length
    const quiet = said()
    const heldSilently = () => {
      expect(makeButton(host).disabled).toBe(true)
      expect(makeButton(host).hasAttribute('aria-describedby')).toBe(false)
      expect(said()).toBe(quiet)
    }
    heldSilently()
    await typeInto(pathBox(host), '/media/Manga')
    heldSilently()
    await typeInto(nameBox(host), 'Manga')
    expect(makeButton(host).disabled).toBe(false)
    await typeInto(pathBox(host), '   ')
    heldSilently()
    await typeInto(pathBox(host), '')
    heldSilently()
  })

  it("draws a refusal's sentence and keeps what was entered", async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries: 400 {"error":"/media/typo: No such file or directory (os error 2)"}',
      ),
    )
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '/media/typo')

    expect(form(host).querySelector('[role="alert"]')?.textContent).toBe(
      '/media/typo: No such file or directory (os error 2)',
    )
    // Beside the button that was pressed, and not the list's slot.
    expect(host.querySelector('.new-library > [role="alert"]')).toBeNull()
    expect(nameBox(host).value).toBe('Manga')
    expect(pathBox(host).value).toBe('/media/typo')
    expect(here()).toBe('/config/libraries/new')
  })

  it('draws the newest list when two reads answer out of order', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')

    await flush(() => first.resolve({ libraries: [] }))
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(host.querySelector('.missing')).toBeNull()
  })

  it('keeps a create refusal when the page’s own read of the list answers after it', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries: 400 {"error":"a library is already called `Comics`"}',
      ),
    )
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Comics', '/media/x')
    expect(form(host).querySelector('[role="alert"]')?.textContent).toContain(
      'already called',
    )

    await flush(() =>
      first.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    expect(form(host).querySelector('[role="alert"]')?.textContent).toContain(
      'already called',
    )
    expect(nameBox(host).value).toBe('Comics')
    expect(pathBox(host).value).toBe('/media/x')
  })

  it('sends one create for two submits', async () => {
    vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
    const host = await at('/config/libraries/new')
    await typeInto(nameBox(host), 'Manga')
    await typeInto(pathBox(host), '/media/Manga')
    await submit(form(host))
    await submit(form(host))
    expect(api.createLibrary).toHaveBeenCalledTimes(1)
  })

  it('clears the last refusal when the next attempt is made, and draws the form empty when it is next opened, once it works', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries: 400 {"error":"refused"}'),
      )
      .mockResolvedValueOnce({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '/media/Manga')
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await submit(form(host))
    await flush()
    expect(here()).toBe('/config/libraries/1')
    expect(host.querySelector('[role="alert"]')).toBeNull()

    await follow(host, '‹ Libraries')
    await follow(host, 'New library')
    expect(nameBox(host).value).toBe('')
    expect(pathBox(host).value).toBe('')
    expect(form(host).querySelector('[role="alert"]')).toBeNull()
  })

  it('clears a create refusal when the name or the directory’s path is edited', async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"refused"}'),
    )
    const host = await at('/config/libraries/new')
    const refused = () => form(host).querySelector('[role="alert"]')
    await makeOne(host, 'Manga', '/media/a')
    expect(refused()).not.toBeNull()
    await typeInto(nameBox(host), 'Manga 2')
    expect(refused()).toBeNull()

    await submit(form(host))
    expect(refused()).not.toBeNull()
    await typeInto(pathBox(host), '/media/b')
    expect(refused()).toBeNull()
  })

  it('draws the create refusal after the row that holds Make the library', async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"refused"}'),
    )
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '/media/a')
    expect(
      makeButton(host).parentElement?.nextElementSibling?.getAttribute('role'),
    ).toBe('alert')
  })

  it('holds no create refusal while the next attempt is in flight', async () => {
    vi.mocked(api.createLibrary)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries: 400 {"error":"refused"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '/media/Manga')
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).toBeNull()
  })

  it('keeps the list’s failure when the make form is edited', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    const host = await at('/config/libraries/new')
    expect(
      host.querySelector('.new-library > [role="alert"]')?.textContent,
    ).toBe('the store is locked')
    await typeInto(nameBox(host), 'M')
    await typeInto(pathBox(host), '/media/x')
    await typeInto(pathBox(host), '')
    expect(
      host.querySelector('.new-library > [role="alert"]')?.textContent,
    ).toBe('the store is locked')
  })

  it('takes the list’s failure away when a later read succeeds', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    expect(host.querySelector('.new-library > [role="alert"]')).not.toBeNull()

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
  })

  it('draws no failure from a read that a newer one has superseded', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    await flush(() =>
      first.reject(
        new Error('/api/library/libraries: 500 {"error":"too late"}'),
      ),
    )
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
  })

  it('draws the library that was just made at once, when the list that follows cannot be read', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await makeOne(host, 'Manga', '/media/Manga')
    expect(here()).toBe('/config/libraries/1')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toBe(
      'The libraries list could not be read again (the store is locked), so what is drawn may be out of date.',
    )

    // The table, whose read fails too, draws it from what the make answered.
    await follow(host, '‹ Libraries')
    expect(names(host)).toEqual(['Manga'])
    expect(host.querySelector('.empty')).toBeNull()
  })

  it('draws a made library after those already drawn until the next read orders it', async () => {
    const comics = library(2, 'Comics', ['/media/Comics'])
    const zeta = library(3, 'Zeta', ['/media/Zeta'])
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [comics, zeta],
    })
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at('/config/libraries/new')
    const slow = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(slow.promise)
    await makeOne(host, 'Manga', '/media/Manga')
    await follow(host, '‹ Libraries')
    expect(names(host)).toEqual(['Comics', 'Zeta', 'Manga'])

    // The server orders by name, and the read that follows says where it goes.
    await flush(() => slow.resolve({ libraries: [comics, made, zeta] }))
    expect(names(host)).toEqual(['Comics', 'Manga', 'Zeta'])
  })

  it('gives each box of the make-a-library form its label’s words as its aria-label — O175', async () => {
    const host = await at('/config/libraries/new')
    const labelled = fields(form(host).querySelectorAll('label.field'))
    expect(labelled.map((f) => f.written)).toEqual(['Name', 'Directory path'])
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })
})

describe('Libraries, a probe', () => {
  it('does not hand a directory the probe of a removed one that shared its id', async () => {
    const first = library(1, 'Manga', ['/srv/first'])
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: first,
      added: [
        {
          root: first.dirs[0],
          watching: false,
          watch_reason: 'inotify limit reached',
          interval_secs: 900,
        },
      ],
    })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
    const host = await at('/config/libraries/new')
    await makeOne(host, 'Manga', '/srv/first')
    await follow(host, '/srv/first')
    expect(host.textContent).toContain('inotify limit reached')

    // The directory's id, freed and given to another directory by the store.
    const second = {
      ...first,
      dirs: [dir(first.dirs[0].id, '/srv/second')],
    }
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: second,
      added: [],
    })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [second] })
    await go('/config/libraries/new')
    await makeOne(host, 'Manga', '/srv/second')
    await follow(host, '/srv/second')
    expect(here()).toBe('/config/libraries/1/dirs/10')
    expect(host.textContent).toContain('/srv/second')
    expect(host.textContent).not.toContain('inotify limit reached')
  })

  it('does not reuse a directory’s page across two directories that shared an id', async () => {
    const edition = (host: HTMLElement) =>
      (host.querySelector('input[name="edition_key"]') as HTMLInputElement)
        .value
    const first = library(1, 'Manga', ['/srv/first'])
    first.dirs[0].edition_key = 'first'
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
    const host = await at('/config/libraries/1/dirs/10')
    expect(edition(host)).toBe('first')

    // A save on this page reads the list again, and the directory at this id is
    // now another one.
    const second = library(1, 'Manga', ['/srv/second'])
    second.dirs[0].edition_key = 'second'
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [second] })
    await press(host.querySelector('input[name="keeps_documents"]') as Element)
    await flush()
    expect(host.querySelector('.root-row h2')?.textContent).toBe('/srv/second')
    expect(edition(host)).toBe('second')
  })

  it('draws a directory’s page opened from a sibling’s free of the hold the sibling’s page had, until its own read of the library answers', async () => {
    const libraries = twoAndOne()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
    // The first read finds the second directory scanning; every later one is
    // held open, so only the page's own state can free its buttons.
    const later = deferred<api.LibraryScanStatus>()
    let asked = 0
    vi.mocked(api.libraryScanStatus).mockImplementation(() =>
      asked++ === 0
        ? Promise.resolve(scanningIn(libraries[0], 11))
        : later.promise,
    )
    const held = (host: HTMLElement) =>
      ['Scan directory', 'Move…', 'Remove…'].map(
        (text) => button(host, text).disabled,
      )
    const host = await at('/config/libraries/1/dirs/10')
    expect(held(host)).toEqual([true, true, true])

    await go('/config/libraries/1/dirs/11')
    expect(host.querySelector('.root-row h2')?.textContent).toBe(
      '/media/Manga/Chapters',
    )
    expect(held(host)).toEqual([false, false, false])
    await flush(() => later.resolve(idle(1)))
    expect(held(host)).toEqual([false, false, false])
  })

  it('keeps a watch probe through a failed read of the list, for the read that follows', async () => {
    // The probe runs when a directory is added and never again, so it is the
    // only chance to say what the watch found; a list that could not be read
    // after the add must not take it with it.
    const first = library(1, 'Manga', ['/srv/first'])
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: first,
      added: [
        {
          root: first.dirs[0],
          watching: false,
          watch_reason: 'inotify limit reached',
          interval_secs: 900,
        },
      ],
    })
    const host = await at('/config/libraries/new')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await makeOne(host, 'Manga', '/srv/first')
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toContain('the store is locked')

    // A later read answers, as the one on opening the directory's page does.
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
    await follow(host, '/srv/first')
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(regime(host)).toContain('probe failed: inotify limit reached')
  })

  it('keeps the watch probe of a directory added to a library, for its page', async () => {
    const manga = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [manga] })
    const extra = dir(9, '/media/Manga/Extras')
    vi.mocked(api.addDir).mockResolvedValue({
      root: extra,
      watching: false,
      watch_reason: 'inotify limit reached',
      interval_secs: 900,
    })
    const host = await at('/config/libraries/1')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [{ ...manga, dirs: [...manga.dirs, extra] }],
    })
    await press(host.querySelector('button[name="add-dir"]') as Element)
    const picker = host.querySelector(
      '.library-page > .add-root',
    ) as HTMLElement
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    await until(() => paths(host).length === 2)
    expect(paths(host)).toEqual(['/media/Manga', '/media/Manga/Extras'])
    await follow(host, '/media/Manga/Extras')
    expect(here()).toBe('/config/libraries/1/dirs/9')
    expect(regime(host)).toContain('probe failed: inotify limit reached')
  })
})

describe('Libraries, a library’s removal', () => {
  function twoLibraries() {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(2, 'Comics', ['/media/Comics']),
        library(1, 'Manga', ['/media/Manga']),
      ],
    })
  }

  it('draws what the re-derive after a removal found on the table, and drops it once the table is left, so the table opened again draws none', async () => {
    const manga = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [manga] })
    const host = await at('/config/libraries/1')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
    await removeDrawnLibrary(host)

    expect(here()).toBe(TABLE)
    expect(names(host)).toEqual([])
    const report = host.querySelector('.library-table > .removal-report')
    expect(report?.querySelector('p')?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
    expect(report?.querySelector('.derived')).not.toBeNull()
    expect(
      report?.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('4')

    await follow(host, 'New library')
    expect(host.querySelector('.removal-report')).toBeNull()
    await follow(host, '‹ Libraries')
    expect(here()).toBe(TABLE)
    expect(host.querySelector('.removal-report')).toBeNull()
    expect(document.activeElement?.closest('.removal-report')).toBeNull()

    await follow(host, 'New library')
    const made = library(2, 'Comics', ['/media/Comics'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Comics', '/media/Comics')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Comics')
    expect(host.querySelector('.removal-report')).toBeNull()
  })

  it('moves focus to the removal report, which is named, when one is drawn', async () => {
    twoLibraries()
    const host = await at('/config/libraries/1')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    await removeDrawnLibrary(host)
    const report = host.querySelector('.library-table > .removal-report')
    expect(report).not.toBeNull()
    expect(document.activeElement).toBe(report)
    expect(report?.getAttribute('tabindex')).toBe('-1')
    // A group, which is what makes its name one: an `aria-label` on a plain
    // `div` is not announced. Named by its opening paragraph, which it
    // would otherwise repeat when read on.
    expect(report?.getAttribute('role')).toBe('group')
    // Read as it takes focus, and so outside the live region, which would
    // have it read twice. The region is there, empty.
    expect(report?.closest('[aria-live]')).toBeNull()
    const region = host.querySelector('.library-table > [aria-live="polite"]')
    expect(region).not.toBeNull()
    expect(region?.childNodes).toHaveLength(0)
    expect(report?.hasAttribute('aria-label')).toBe(false)
    const named = document.getElementById(
      report?.getAttribute('aria-labelledby') ?? '',
    )
    expect(named).toBe(report?.querySelector(':scope > p'))
    expect(named?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
  })

  it('drops a removed library at once when the list that follows cannot be read', async () => {
    twoLibraries()
    const host = await at('/config/libraries/1')
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 the store is locked'),
    )
    await removeDrawnLibrary(host)
    expect(names(host)).toEqual(['Comics'])
    expect(
      host.querySelector('.library-table > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })

  it('drops the first of two libraries at once too, by id and not by position', async () => {
    twoLibraries()
    const host = await at('/config/libraries/2')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await removeDrawnLibrary(host)
    expect(names(host)).toEqual(['Manga'])
  })

  it('takes focus once for a report, and leaves it where the person puts it when the list that follows answers', async () => {
    twoLibraries()
    const host = await at('/config/libraries/1')
    const slow = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(slow.promise)
    await removeDrawnLibrary(host)
    expect(document.activeElement).toBe(host.querySelector('.removal-report'))

    const next = link(host, 'New library')
    act(() => next.focus())
    await flush(() =>
      slow.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    expect(names(host)).toEqual(['Comics'])
    expect(host.querySelector('.removal-report')).not.toBeNull()
    expect(document.activeElement).toBe(next)
  })

  it('leaves focus where it is when a read of the list fails after a report took it', async () => {
    twoLibraries()
    const host = await at('/config/libraries/1')
    const slow = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(slow.promise)
    await removeDrawnLibrary(host)
    expect(document.activeElement).toBe(host.querySelector('.removal-report'))

    const next = link(host, 'New library')
    act(() => next.focus())
    await flush(() => slow.reject(locked()))
    expect(
      host.querySelector('.library-table > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    expect(document.activeElement).toBe(next)
  })

  it('holds only the library that is scanning, and none of the others', async () => {
    const comics = library(2, 'Comics', ['/media/Comics'])
    const manga = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [comics, manga],
    })
    vi.mocked(api.startLibraryScan).mockResolvedValue({
      library_id: 2,
      status_url: '/api/library/libraries/2/scan',
    })
    const host = await at('/config/libraries/2')
    vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
      Promise.resolve({
        library_id: id,
        running: id === 2,
        elapsed_secs: 1,
        dirs: [],
      }),
    )
    const held = () =>
      [...host.querySelectorAll('.library-page button')]
        .filter((b) => (b as HTMLButtonElement).disabled)
        .map((b) => b.textContent)
    await press(host.querySelector('button[name="scan-library"]') as Element)

    // Held: the library's own scan and removal. Its directories' buttons are on
    // their own pages.
    expect(held()).toEqual(['Scan library', 'Remove library…'])
    expect(host.querySelector('.library-page .scanning')).not.toBeNull()

    // Straight from one library's page to the other's, as a typed address
    // goes, so the page is not unmounted between them. Manga's own read of its
    // status is held open: what is drawn until it answers is a page of its
    // own, not Comics' carried over.
    vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
      id === 1
        ? new Promise(() => {})
        : Promise.resolve({
            library_id: id,
            running: true,
            elapsed_secs: 1,
            dirs: [],
          }),
    )
    await go('/config/libraries/1')
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(held()).toEqual([])
    expect(host.querySelector('.library-page .scanning')).toBeNull()
  })
})

describe('Libraries, a directory’s move', () => {
  it('keeps what deriving after the move found, named for the directory and the library, once the list has reloaded', async () => {
    const answer = (dry_run: boolean): api.MoveAnswer => ({
      moved: {
        library_id: 2,
        library: 'Comics',
        works_kept: 1,
        works_joined: 0,
        works_split: 0,
        series_joined: 2,
        // The rehearsal's counts differ from the move's: the kept report
        // can only have the move's.
        positions: dry_run ? 1 : 3,
        notes: 0,
        alignments_dropped: 0,
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      derive: { ...emptyDerive, lost_positions: dry_run ? 2 : 6 },
      dry_run,
    })
    vi.mocked(api.moveCost).mockResolvedValue(answer(true))
    vi.mocked(api.moveDir).mockResolvedValue(answer(false))
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await at('/config/libraries/1/dirs/11')
    const after = [
      library(1, 'Manga', ['/media/Manga/Volumes']),
      library(2, 'Comics', ['/media/Comics', '/media/Manga/Chapters']),
    ]
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: after })
    await moveDrawnDir(host, '2')

    expect(here()).toBe('/config/libraries/1')
    const report = host.querySelector(
      '.library-page > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'Moved /media/Manga/Chapters from Manga to Comics.',
    )
    // What the move did leads, in the past tense, before what deriving found.
    const did = report.querySelector('.move-counts')?.textContent ?? ''
    expect(did).toContain(
      '1 work kept its address, 0 works joined a work Comics already held, as another edition, and 0 works were split between the two libraries.',
    )
    expect(did).toContain('3 in-progress positions and 0 notes went with them.')
    expect(did).toContain('2 series already in Comics gained volumes.')
    expect(
      [...report.querySelectorAll(':scope > p')].map((p) => p.textContent),
    ).toEqual([
      'Moved /media/Manga/Chapters from Manga to Comics.',
      'What deriving after the move found:',
    ])
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('6')
    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
    await follow(host, '‹ Libraries')
    await follow(host, 'Comics')
    expect(paths(host)).toEqual(['/media/Comics', '/media/Manga/Chapters'])
  })

  /** Moves Manga's second directory to `to` and answers the report kept. */
  async function keptReport(
    to: '2' | 'new',
    moved: Partial<api.Moved>,
  ): Promise<HTMLElement> {
    vi.mocked(api.moveCost).mockResolvedValue(movedAnswer(moved, true))
    vi.mocked(api.moveDir).mockResolvedValue(movedAnswer(moved))
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await at('/config/libraries/1/dirs/11')
    await moveDrawnDir(host, to)
    return host.querySelector('.removal-report') as HTMLElement
  }

  it('leaves the join out of the kept report where the target was made by the move', async () => {
    const report = await keptReport('new', {
      library: 'Extras',
      library_id: 9,
      works_kept: 2,
    })
    expect(report.querySelector('.move-counts')?.textContent).toContain(
      '2 works kept their address and 0 works were split between the two libraries.',
    )
    expect(report.textContent).not.toContain('joined')
  })

  it('says what the move dropped and removed, in the past tense', async () => {
    const report = await keptReport('2', {
      alignments_dropped: 2,
      library_removed: 'Manga',
      grants_removed: 3,
    })
    const did = report.querySelector('.move-counts')?.textContent ?? ''
    expect(did).toContain(
      '2 alignments between editions the move separated were dropped.',
    )
    expect(did).toContain(
      'Manga had no other directory, so it was removed, with 3 grants.',
    )
  })

  it('drops a moved directory from its library at once when the list that follows cannot be read', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
    vi.mocked(api.moveDir).mockResolvedValue(movedAnswer())
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await at('/config/libraries/1/dirs/11')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await moveDrawnDir(host, '2')
    // Gone from Manga; not drawn in Comics, which the reload would do.
    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
    expect(
      host.querySelector('.library-page > .removal-report > p')?.textContent,
    ).toBe('Moved /media/Manga/Chapters from Manga to Comics.')
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    await follow(host, '‹ Libraries')
    await follow(host, 'Comics')
    expect(paths(host)).toEqual(['/media/Comics'])
  })

  it('drops the library a move emptied at once when the list that follows cannot be read', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      movedAnswer({ library_removed: 'Manga' }, true),
    )
    vi.mocked(api.moveDir).mockResolvedValue(
      movedAnswer({ library_removed: 'Manga' }),
    )
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await at('/config/libraries/1/dirs/10')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await moveDrawnDir(host, '2')
    expect(names(host)).toEqual(['Comics'])
  })
})

describe('Libraries, a directory’s removal', () => {
  beforeEach(() => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(config.removeRoot).mockResolvedValue({
      removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
      derive: { ...emptyDerive, lost_positions: 4 },
      dry_run: false,
    })
  })

  const two = () =>
    library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters'])

  it('keeps what a directory’s re-derive found, named for the directory, once the list has reloaded', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two()] })
    const host = await at('/config/libraries/1/dirs/11')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga/Volumes'])],
    })
    await removeDrawnDir(host)

    const report = host.querySelector(
      '.library-page > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'What re-deriving after removing /media/Manga/Chapters found:',
    )
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('4')
    expect(document.activeElement).toBe(report)
    // Its headings sit one level beneath the page's own.
    expect(
      [...report.querySelectorAll('.report-heading')].map((h) => h.tagName),
    ).toEqual(['H3'])
    // The library stays, with the directory that is left.
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
  })

  it('drops a directory’s report once its library’s page is left, so the page opened again draws none', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two()] })
    const host = await at('/config/libraries/1/dirs/11')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga/Volumes'])],
    })
    await removeDrawnDir(host)
    expect(host.querySelector('.library-page > .removal-report')).not.toBeNull()

    await follow(host, '/media/Manga/Volumes')
    expect(host.querySelector('.removal-report')).toBeNull()
    await follow(host, '‹ Manga')
    expect(here()).toBe('/config/libraries/1')
    expect(host.querySelector('.removal-report')).toBeNull()
    expect(document.activeElement?.closest('.removal-report')).toBeNull()
  })

  it('drops a removed directory at once when the list that follows cannot be read', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two()] })
    const host = await at('/config/libraries/1/dirs/11')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await removeDrawnDir(host)

    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
    expect(host.querySelector('.holds')?.textContent).toBe(
      '0 works in 1 directory',
    )
    expect(
      host.querySelector('.library-page > .removal-report > p')?.textContent,
    ).toBe('What re-deriving after removing /media/Manga/Chapters found:')
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    // What is left is live, and is now the library's last directory.
    await follow(host, '/media/Manga/Volumes')
    const remove = button(host, 'Remove…')
    expect(remove.disabled).toBe(false)
    await press(remove)
    expect(host.querySelector('.remove-root .last-dir')?.textContent).toContain(
      'the only directory of the library Manga',
    )
  })

  it('names the library, not the directory, when both directories of a library are removed before the list answers', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two()] })
    vi.mocked(config.removeRoot)
      .mockResolvedValueOnce({
        removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
        derive: { ...emptyDerive, lost_positions: 4 },
        dry_run: false,
      })
      .mockResolvedValueOnce({
        removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
        derive: { ...emptyDerive, lost_positions: 7 },
        dry_run: false,
      })
    const host = await at('/config/libraries/1/dirs/11')
    const slow = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(slow.promise)
    await removeDrawnDir(host)
    expect(
      host.querySelector('.library-page > .removal-report > p')?.textContent,
    ).toBe('What re-deriving after removing /media/Manga/Chapters found:')

    await follow(host, '/media/Manga/Volumes')
    await removeDrawnDir(host)
    expect(here()).toBe(TABLE)
    const report = host.querySelector(
      '.library-table > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('7')
    expect(names(host)).toEqual([])

    await flush(() => slow.resolve({ libraries: [] }))
    expect(names(host)).toEqual([])
  })

  it('drops a library at once when its last directory went and the list cannot be read, and reads the list once', async () => {
    const one = library(1, 'Manga', ['/media/Manga'])
    const comics = library(2, 'Comics', ['/media/Comics'])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [comics, one] })
    const host = await at('/config/libraries/1/dirs/10')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    vi.mocked(api.listLibraries).mockClear()
    await removeDrawnDir(host)

    expect(names(host)).toEqual(['Comics'])
    expect(
      host.querySelector('.library-table > .removal-report > p')?.textContent,
    ).toBe('What re-deriving after removing Manga found:')
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
  })

  it('names the library when its last directory went and the library with it', async () => {
    const one = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [one, library(2, 'Comics', ['/media/Comics'])],
    })
    const host = await at('/config/libraries/1/dirs/10')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    await removeDrawnDir(host)

    const report = host.querySelector(
      '.library-table > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('4')
    expect(document.activeElement).toBe(report)
    expect(names(host)).toEqual(['Comics'])
  })
})

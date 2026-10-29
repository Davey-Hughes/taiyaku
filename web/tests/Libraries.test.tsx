import { act } from 'react'
import {
  MemoryRouter,
  type NavigateFunction,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from '../src/api'
import * as config from '../src/config'
import * as curation from '../src/curation'
import { useFitsPair } from '../src/fitsPair'
import { GateContext } from '../src/gate'
import { emptyDerive } from './support/derive'
import { Libraries } from '../src/Libraries'
import * as api from '../src/libraries'
import * as status from '../src/library'
import {
  ACCEPTED_PROPOSALS,
  LIBRARY_STATUS,
  ROOTS,
  RULES,
  USERS as USER_LIST,
} from '../src/pageReads'
import { RuleEditor } from '../src/RuleEditor'
import * as rulesApi from '../src/rules'
import { deferred, flush } from './support/async'
import './support/dialog'
import { holdOf } from './support/held'
import { press, submit, typeInto } from './support/input'
import { changesIn } from './support/liveRegion'
import { dir, idle, scanningIn, tick } from './support/libraries'
import { mount, rerender } from './support/mount'
import { keptGrid } from './support/reads'
import { expectRowsWired, helpOf, row, section } from './support/settingsRows'
import { cellValue } from './support/table'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'
import { usePage } from '../src/usePage'
import * as usersApi from '../src/users'
import { Users } from '../src/Users'

// Whether the list and an open page fit side by side is the width's, which
// jsdom does not lay out. Narrow by default, as the real hook answers in
// jsdom, which has no `ResizeObserver`; a test that draws the pair says so.
vi.mock('../src/fitsPair', () => ({ useFitsPair: vi.fn(() => false) }))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  listRoots: vi.fn(),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
// Rules, which draws the roots a directory write changes.
vi.mock('../src/rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))
vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
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
/** The make page, outside the names' space. */
const NEW_LIBRARY = '/config/new-library'

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
  /**
   * Draws from the status as the shell's sidebar does, beside every tab, so
   * an invalidation of it is read at once (`status.status`'s calls).
   */
  function Sidebar() {
    usePage([LIBRARY_STATUS])
    return null
  }
  let host: HTMLElement | undefined
  // One element for both routes, as `Config` gives it, so the screen stays
  // mounted between the make page and the pages beneath the table.
  const screen = <Libraries />
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <Routes>
          <Route path={`${TABLE}/*`} element={screen} />
          <Route path={NEW_LIBRARY} element={screen} />
          <Route path="/config/users" element={<p className="other-tab" />} />
          <Route path="/config/rules" element={<RuleEditor />} />
        </Routes>
        <Steering />
        <Sidebar />
      </MemoryRouter>,
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

/** The table's heading, its header's title. */
const tableHeading = (host: HTMLElement) =>
  host.querySelector<HTMLElement>('.library-table .page-title')

/** The heading of the library page that is drawn. */
const libraryHeading = (host: HTMLElement) =>
  host.querySelector<HTMLElement>('.library-page .page-title')

/** The title of the library page that is drawn. */
const libraryTitle = (host: HTMLElement) => libraryHeading(host)?.textContent

/** The title of the directory page that is drawn: the directory's path. */
const dirTitle = (host: HTMLElement) =>
  host.querySelector('.dir-page .page-title')?.textContent

/** The paths in the library page's Directories table, in the order drawn. */
const paths = (host: HTMLElement) =>
  [...host.querySelectorAll('.dir-table tbody code')].map((c) => c.textContent)

/** The facts the directory page states about its watch, scans and last scan. */
const regime = (host: HTMLElement) =>
  [...host.querySelectorAll('.dir-page .regime dd')].map((d) => d.textContent)

const form = (host: HTMLElement) =>
  host.querySelector('form.add-library') as HTMLFormElement
const nameBox = (host: HTMLElement) =>
  form(host).querySelector('input[name="library-name"]') as HTMLInputElement
const pathBox = (host: HTMLElement) =>
  form(host).querySelector('input[name="path"]') as HTMLInputElement
const makeButton = (host: HTMLElement) =>
  form(host).querySelector('button[type="submit"]') as HTMLButtonElement
/** Why the name typed would be refused, said as it is typed. */
const clashNote = (host: HTMLElement) =>
  form(host).querySelector<HTMLElement>(':scope > [role="status"]')
/** Whether the name box is marked invalid, and what describes it. */
const boxMarks = (host: HTMLElement) => [
  nameBox(host).getAttribute('aria-invalid'),
  nameBox(host).getAttribute('aria-describedby'),
]

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

/** What the page for a library name the list does not hold says. */
const NO_SUCH_LIBRARY =
  'There is no library at this address: it may have been removed.'

/** What the page for a directory id its library does not hold says. */
const NO_SUCH_DIR =
  'This library has no directory at this address: it may have been removed, or moved to another library.'

/** The page drawn for an address that names nothing: its header and line. */
const missingPage = (host: HTMLElement) =>
  host
    .querySelector('.missing')
    ?.closest<HTMLElement>('.settings-page') as HTMLElement

const locked = () =>
  new Error('/api/library/libraries: 500 {"error":"the store is locked"}')

/** Directory `id`'s own status: its scan running, or ended with a report. */
const ownStatus = (id: number, running: boolean): config.ScanStatus => ({
  root_id: id,
  running,
  elapsed_secs: running ? 3 : undefined,
  report: running ? null : { error: 'the scan panicked' },
  last_scan_at: running ? null : '2026-10-27 12:00:00',
  last_scan_status: running ? null : 'failed',
})

/**
 * What a library's page draws of a scan: Scan library's hold, the first
 * directory's Last scan cell, whether the running line is drawn, and how
 * many reports. `null` for what is not drawn, as while the page waits.
 */
const scanState = (host: HTMLElement) => {
  const scanButton = host.querySelector(
    '.library-page button[name="scan-library"]',
  )
  const firstCell = host
    .querySelector('.library-page .dir-table tbody tr')
    ?.querySelectorAll('td')[2]
  return JSON.stringify([
    scanButton === null ? null : holdOf(scanButton),
    firstCell === undefined ? null : cellValue(firstCell),
    host.querySelector('.library-page > .scanning') !== null,
    host.querySelectorAll('.library-page .scan-report').length,
  ])
}

/** A library's page waiting for its scan status, or not drawn. */
const NO_SCAN_DRAWN = JSON.stringify([null, null, false, 0])

/** Scan directory pressed on the directory's page drawn, and its start answered. */
async function scanDrawnDir(host: HTMLElement, id: number) {
  vi.mocked(config.startScan).mockResolvedValue({
    root_id: id,
    status_url: `/api/library/roots/${id}/scan`,
  })
  await press(button(host, 'Scan directory'))
  await flush()
}

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
  vi.mocked(status.status).mockResolvedValue({
    libraries: [],
    restricted: false,
  })
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
  it('heads itself, and says so in its box when there is no library yet', async () => {
    const host = await at()
    expect(tableHeading(host)?.textContent).toBe('Libraries')
    const box = host.querySelector('.library-table .settings-box')
    expect([...(box?.children ?? [])].map((child) => child.className)).toEqual([
      'empty',
    ])
    expect(box?.querySelector('.empty')?.textContent).toBe(
      'No libraries yet. A library is made with one directory, and more can be added on its page.',
    )
    expect(host.querySelector('.library-table table')).toBeNull()
  })

  it('draws the table as the whole of an untitled box, under the header Libraries whose one action is New library', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at()
    const screen = host.querySelector('.library-table') as HTMLElement
    const header = screen.querySelector(':scope > .page-header')
    expect(header?.querySelector('.page-title')?.textContent).toBe('Libraries')
    expect(header?.querySelector('.page-description')?.textContent).toBe(
      'Each library is a name and the directories its files are in.',
    )
    expect(
      [...(header?.querySelectorAll('.page-actions a') ?? [])].map((a) => [
        a.textContent,
        a.getAttribute('href'),
      ]),
    ).toEqual([['New library', NEW_LIBRARY]])
    // The table's own page has no way up: it is the top of the screen.
    expect(header?.querySelector('.back')).toBeNull()
    const sections = screen.querySelectorAll('section.settings-section')
    expect(sections).toHaveLength(1)
    expect(sections[0].querySelector('.settings-section-head')).toBeNull()
    const box = sections[0].querySelector(':scope > .settings-box')
    expect([...(box?.children ?? [])].map((child) => child.tagName)).toEqual([
      'TABLE',
    ])
    expect(screen.querySelector('p.new-item')).toBeNull()
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

    await go('/config/libraries/Manga/dirs/10')
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
    expect(link(host, 'Manga').getAttribute('href')).toBe(
      '/config/libraries/Manga',
    )

    await follow(host, 'Manga')
    expect(here()).toBe('/config/libraries/Manga')
    expect(libraryTitle(host)).toBe('Manga')
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
      '2026-10-03 09:00:00 UTC',
      '2026-10-04 08:00:00 UTC\u00a0·\u00a0failed: disk gone',
      '2026-10-05 07:00:00 UTC\u00a0·\u00a0outcome unrecorded',
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

describe('Libraries, a first visit and a return', () => {
  /** The "Loading…" line drawn in the body's place of the page `page`. */
  const waitLine = (host: HTMLElement, page: string) =>
    host.querySelector(`.${page} > p[role="status"]`)

  /** The table as drawn: whether it is, its rows, and its wait line. */
  const tableState = (host: HTMLElement) =>
    JSON.stringify([
      host.querySelector('.library-table') !== null,
      names(host),
      waitLine(host, 'library-table')?.textContent ?? null,
    ])

  it('heads the table at once on a first visit, draws Loading… in its body’s place from 200 ms, and the table in one step once the list lands', async () => {
    holdTheClock()
    const list = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(list.promise)
    const host = await at()
    expect(tableHeading(host)?.textContent).toBe('Libraries')
    expect(host.querySelector('.library-table .settings-box')).toBeNull()
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(waitLine(host, 'library-table')).toBeNull()
    pass(1)
    expect(waitLine(host, 'library-table')?.textContent).toBe('Loading…')

    const drawn = watchDrawn(() => tableState(host))
    await flush(() => list.resolve({ libraries: twoAndOne() }))
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify([true, [], 'Loading…']),
      JSON.stringify([true, ['Manga', 'Comics'], null]),
    ])
  })

  it('draws the table at once on a return from another tab, reads the list again, and draws its answer in one step with no wait', async () => {
    holdTheClock()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at()
    await go('/config/users')
    const again = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockClear()
    vi.mocked(api.listLibraries).mockReturnValue(again.promise)

    const drawn = watchDrawn(() => tableState(host))
    await go(TABLE)
    expect(names(host)).toEqual(['Manga', 'Comics'])
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
    pass(WAIT_DRAWN_AFTER_MS)
    await flush(() =>
      again.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify([false, [], null]),
      JSON.stringify([true, ['Manga', 'Comics'], null]),
      JSON.stringify([true, ['Comics'], null]),
    ])
  })

  /**
   * O200: the sidebar lists the libraries the status names, so the screen
   * reads it as it arrives, beside the list, whichever tab the person came
   * from; the shell stays drawn between Configuration's tabs. A read already
   * out for it is joined; when both are drawn at once, the sidebar's own
   * joins the screen's, whose effect runs first.
   */
  it('reads the sidebar’s status as it arrives from another tab, once, beside one read of the list', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at()
    expect(status.status).toHaveBeenCalledTimes(1)
    await go('/config/users')
    vi.mocked(status.status).mockClear()
    vi.mocked(api.listLibraries).mockClear()

    await go(TABLE)
    expect(names(host)).toEqual(['Manga', 'Comics'])
    expect(status.status).toHaveBeenCalledTimes(1)
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
  })

  it('heads a library’s page with the name its address spells while the list is read, and draws Loading… in its body’s place from 200 ms', async () => {
    holdTheClock()
    const list = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(list.promise)
    const host = await at(`${TABLE}/Light%20novels`)
    expect(libraryTitle(host)).toBe('Light novels')
    expect(
      link(host, '‹ Libraries').closest('.library-page > .page-header'),
    ).not.toBeNull()
    expect(host.querySelector('.missing')).toBeNull()
    expect(paths(host)).toEqual([])
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(waitLine(host, 'library-page')).toBeNull()
    pass(1)
    expect(waitLine(host, 'library-page')?.textContent).toBe('Loading…')

    await flush(() =>
      list.resolve({
        libraries: [library(4, 'Light novels', ['/media/Novels'])],
      }),
    )
    expect(libraryTitle(host)).toBe('Light novels')
    expect(paths(host)).toEqual(['/media/Novels'])
    expect(waitLine(host, 'library-page')).toBeNull()
  })

  /** A library's page as drawn: its paths, its running scan, its wait line. */
  const libraryState = (host: HTMLElement) =>
    JSON.stringify([
      paths(host),
      host.querySelector('.library-page > .scanning')?.textContent ?? null,
      waitLine(host, 'library-page')?.textContent ?? null,
    ])

  it('keeps Loading… in a library’s page’s body’s place while its scan status is read once the list has found it, and draws its body with the running scan in one step', async () => {
    holdTheClock()
    const list = deferred<{ libraries: api.Library[] }>()
    const scan = deferred<api.LibraryScanStatus>()
    vi.mocked(api.listLibraries).mockReturnValue(list.promise)
    vi.mocked(api.libraryScanStatus).mockReturnValue(scan.promise)
    const host = await at(`${TABLE}/Manga`)
    pass(WAIT_DRAWN_AFTER_MS)
    expect(waitLine(host, 'library-page')?.textContent).toBe('Loading…')

    const libraries = twoAndOne()
    await flush(() => list.resolve({ libraries }))
    expect(libraryTitle(host)).toBe('Manga')
    expect(libraryState(host)).toBe(JSON.stringify([[], null, 'Loading…']))
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    expect(api.libraryScanStatus).toHaveBeenCalledWith(1)

    const drawn = watchDrawn(() => libraryState(host))
    await flush(() => scan.resolve(scanningIn(libraries[0], 11)))
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify([[], null, 'Loading…']),
      JSON.stringify([
        ['/media/Manga/Volumes', '/media/Manga/Chapters'],
        'A scan is running in this library… 3s',
        null,
      ]),
    ])
  })

  /**
   * §9's call 9, where the two do not fit: the library's page is not drawn
   * while the directory's is, and what it kept is from before the
   * directory's own scan, so a return waits for its read.
   */
  describe('a return to a library’s page after its directory’s own scan', () => {
    it('waits for the status on a return from a directory’s page whose scan started, where the status kept said none was running, then draws the scan running', async () => {
      const libraries = twoAndOne()
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
      const host = await at(`${TABLE}/Manga`)
      expect(scanState(host)).toBe(JSON.stringify(['free', 'Never', false, 0]))
      await follow(host, '/media/Manga/Volumes')
      await scanDrawnDir(host, 10)
      expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()

      const read = deferred<api.LibraryScanStatus>()
      vi.mocked(api.libraryScanStatus).mockReturnValue(read.promise)
      const drawn = watchDrawn(() => scanState(host))
      await follow(host, '‹ Manga')
      expect(libraryTitle(host)).toBe('Manga')
      expect(scanState(host)).toBe(NO_SCAN_DRAWN)
      await flush(() => read.resolve(scanningIn(libraries[0], 10)))
      drawn.stop()
      expect([...drawn.seen]).toEqual([
        NO_SCAN_DRAWN,
        JSON.stringify(['held', 'Scanning…', true, 0]),
      ])
    })

    it('waits for the status on a return made while its directory’s scan was still being started, then draws the scan running', async () => {
      const libraries = twoAndOne()
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
      const host = await at(`${TABLE}/Manga`)
      expect(scanState(host)).toBe(JSON.stringify(['free', 'Never', false, 0]))
      await follow(host, '/media/Manga/Volumes')
      const start = deferred<{ root_id: number; status_url: string }>()
      vi.mocked(config.startScan).mockReturnValue(start.promise)
      await press(button(host, 'Scan directory'))
      expect(config.startScan).toHaveBeenCalledTimes(1)

      const read = deferred<api.LibraryScanStatus>()
      vi.mocked(api.libraryScanStatus).mockReturnValue(read.promise)
      const drawn = watchDrawn(() => scanState(host))
      await follow(host, '‹ Manga')
      expect(libraryTitle(host)).toBe('Manga')
      expect(scanState(host)).toBe(NO_SCAN_DRAWN)
      await flush(() => read.resolve(scanningIn(libraries[0], 10)))
      await flush(() =>
        start.resolve({
          root_id: 10,
          status_url: '/api/library/roots/10/scan',
        }),
      )
      drawn.stop()
      expect([...drawn.seen]).toEqual([
        NO_SCAN_DRAWN,
        JSON.stringify(['held', 'Scanning…', true, 0]),
      ])
    })

    it('waits for the status on a return from a directory’s page that saw its scan end, where the status kept said it was running, and draws no scan and no report', async () => {
      vi.useFakeTimers({
        toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'],
      })
      const libraries = twoAndOne()
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
      vi.mocked(api.libraryScanStatus).mockResolvedValue(
        scanningIn(libraries[0], 10),
      )
      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve(ownStatus(id, id === 10)),
      )
      const host = await at(`${TABLE}/Manga`)
      expect(scanState(host)).toBe(
        JSON.stringify(['held', 'Scanning…', true, 0]),
      )
      await follow(host, '/media/Manga/Volumes')
      expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()

      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve(ownStatus(id, false)),
      )
      vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(1))
      await tick()
      expect(host.querySelector('.dir-page > .scanning')).toBeNull()
      expect(host.querySelectorAll('.dir-page .scan-report')).toHaveLength(1)

      const read = deferred<api.LibraryScanStatus>()
      vi.mocked(api.libraryScanStatus).mockReturnValue(read.promise)
      const drawn = watchDrawn(() => scanState(host))
      await follow(host, '‹ Manga')
      expect(libraryTitle(host)).toBe('Manga')
      expect(scanState(host)).toBe(NO_SCAN_DRAWN)
      await flush(() => read.resolve(idle(1)))
      await flush()
      drawn.stop()
      expect([...drawn.seen]).toEqual([
        NO_SCAN_DRAWN,
        JSON.stringify(['free', 'Never', false, 0]),
      ])
    })

    it('waits for the status on a return after its directory’s page was left for another screen while its scan ran, and the scan then ended', async () => {
      const libraries = twoAndOne()
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
      const host = await at(`${TABLE}/Manga`)
      await follow(host, '/media/Manga/Volumes')
      await scanDrawnDir(host, 10)
      expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()
      await go('/config/users')
      expect(host.querySelector('.other-tab')).not.toBeNull()

      // The scan ends with nothing drawn to see it.
      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve(ownStatus(id, false)),
      )
      const read = deferred<api.LibraryScanStatus>()
      vi.mocked(api.libraryScanStatus).mockReturnValue(read.promise)
      await go(`${TABLE}/Manga`)
      expect(libraryTitle(host)).toBe('Manga')
      expect(scanState(host)).toBe(NO_SCAN_DRAWN)
      await flush(() => read.resolve(idle(1)))
      expect(scanState(host)).toBe(JSON.stringify(['free', 'Never', false, 0]))
    })

    it('waits for the status on a return from a directory’s page whose first read took up a scan the status kept did not have, then draws the scan running', async () => {
      const libraries = twoAndOne()
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
      const host = await at(`${TABLE}/Manga`)
      expect(scanState(host)).toBe(JSON.stringify(['free', 'Never', false, 0]))
      // The directory's scan began elsewhere: another tab, or the command
      // line.
      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve(ownStatus(id, id === 10)),
      )
      vi.mocked(api.libraryScanStatus).mockResolvedValue(
        scanningIn(libraries[0], 10),
      )
      await follow(host, '/media/Manga/Volumes')
      expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()

      const read = deferred<api.LibraryScanStatus>()
      vi.mocked(api.libraryScanStatus).mockReturnValue(read.promise)
      const drawn = watchDrawn(() => scanState(host))
      await follow(host, '‹ Manga')
      expect(libraryTitle(host)).toBe('Manga')
      expect(scanState(host)).toBe(NO_SCAN_DRAWN)
      await flush(() => read.resolve(scanningIn(libraries[0], 10)))
      drawn.stop()
      expect([...drawn.seen]).toEqual([
        NO_SCAN_DRAWN,
        JSON.stringify(['held', 'Scanning…', true, 0]),
      ])
    })
  })

  it('keeps focus on a library’s page’s ‹ Libraries, taken while its reads are out, once they land: the header and the notices drawn while waiting are the page’s own', async () => {
    const list = deferred<{ libraries: api.Library[] }>()
    const scan = deferred<api.LibraryScanStatus>()
    vi.mocked(api.listLibraries).mockReturnValue(list.promise)
    vi.mocked(api.libraryScanStatus).mockReturnValue(scan.promise)
    const host = await at(`${TABLE}/Manga`)
    const back = link(host, '‹ Libraries')
    act(() => back.focus())
    expect(document.activeElement).toBe(back)
    // The screen's live region, drawn empty before any answer can come, is
    // the drawn page's too.
    const region = () =>
      host.querySelector('.library-page > [aria-live="polite"]')
    const waiting = region()
    expect(waiting).not.toBeNull()

    await flush(() => list.resolve({ libraries: twoAndOne() }))
    expect(document.activeElement).toBe(back)
    await flush(() => scan.resolve(idle(1)))
    expect(paths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    expect(link(host, '‹ Libraries')).toBe(back)
    expect(document.activeElement).toBe(back)
    expect(region()).toBe(waiting)
  })

  it('names a library’s waiting page as the list does once the list has found it, not as its address spells it', async () => {
    const list = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(list.promise)
    vi.mocked(api.libraryScanStatus).mockReturnValue(new Promise(() => {}))
    const host = await at(`${TABLE}/manga`)
    expect(libraryTitle(host)).toBe('manga')

    await flush(() => list.resolve({ libraries: twoAndOne() }))
    expect(paths(host)).toEqual([])
    expect(libraryTitle(host)).toBe('Manga')
  })

  it('draws the list’s failure under a library’s page’s header, and no Loading… beside it, and reads no scan status, when the list cannot be read', async () => {
    holdTheClock()
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    const host = await at(`${TABLE}/Manga`)
    expect(libraryTitle(host)).toBe('Manga')
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toBe('the store is locked')
    pass(WAIT_DRAWN_AFTER_MS)
    expect(waitLine(host, 'library-page')).toBeNull()
    expect(paths(host)).toEqual([])
    expect(api.libraryScanStatus).not.toHaveBeenCalled()
  })

  /**
   * O222: a write sent from the screen, whose answer lands once Users has
   * replaced the screen. The screen drawn on a return is a new one, and the
   * answer marks the list stale in the store: read at once if the return has
   * drawn it, and by the return otherwise. So whichever comes first, the
   * return or the answer, the list drawn is the one the write left.
   */
  it.each([
    { order: 'the return, then the answer', answerFirst: false },
    { order: 'the answer, then the return', answerFirst: true },
  ])(
    'draws the list as a write sent from the screen left it, with no change of page, after another tab replaced the screen and $order — O222',
    async ({ answerFirst }) => {
      let held = ['/media/Manga/Volumes', '/media/Manga/Chapters']
      vi.mocked(api.listLibraries).mockImplementation(async () => ({
        libraries: [
          library(1, 'Manga', held),
          library(2, 'Comics', ['/media/Comics']),
        ],
      }))
      const removed = deferred<config.RemovalAnswer>()
      vi.mocked(config.removeRoot).mockReturnValue(removed.promise)
      const host = await at(`${TABLE}/Manga/dirs/11`)
      await press(button(host, 'Remove…'))
      await press(host.querySelector('.remove-root .confirm') as Element)
      expect(config.removeRoot).toHaveBeenCalledTimes(1)
      await go('/config/users')
      const answer = () =>
        flush(() => {
          held = ['/media/Manga/Volumes']
          removed.resolve(removal(4))
        })
      const both = ['/media/Manga/Volumes', '/media/Manga/Chapters']
      if (answerFirst) await answer()
      const drawn = watchDrawn(() => JSON.stringify(paths(host)))
      await go(`${TABLE}/Manga`)
      if (!answerFirst) {
        expect(paths(host)).toEqual(both)
        await answer()
      }
      await flush()
      drawn.stop()
      expect(here()).toBe(`${TABLE}/Manga`)
      expect(paths(host)).toEqual(['/media/Manga/Volumes'])
      // The list the answer made stale is never drawn on the return.
      if (answerFirst) {
        expect([...drawn.seen]).not.toContain(JSON.stringify(both))
      }
    },
  )
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
    expect(here()).toBe('/config/libraries/Manga/dirs/10')
    expect(api.listLibraries).toHaveBeenCalledTimes(3)
    await follow(host, '‹ Manga')
    expect(api.listLibraries).toHaveBeenCalledTimes(4)
    await follow(host, '‹ Libraries')
    expect(api.listLibraries).toHaveBeenCalledTimes(5)
    await follow(host, 'New library')
    expect(here()).toBe(NEW_LIBRARY)
    expect(api.listLibraries).toHaveBeenCalledTimes(6)
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    expect(api.listLibraries).toHaveBeenCalledTimes(7)
  })

  it('says there is no such library, with the way back to the table, for a name the list does not hold', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    const host = await at('/config/libraries/Novels')
    expect(host.querySelector('.library-page')).toBeNull()
    const page = missingPage(host)
    expect(page.querySelector('.page-title')?.textContent).toBe(
      'No such library',
    )
    expect(page.querySelector('.missing')?.textContent).toBe(NO_SUCH_LIBRARY)
    expect(link(page, '‹ Libraries').getAttribute('href')).toBe(TABLE)
  })

  it('says there is no such directory, with the way back to its library, for a directory id the library does not hold', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    // Directory 20 is Comics', not Manga's.
    const host = await at('/config/libraries/Manga/dirs/20')
    expect(host.querySelector('.dir-page')).toBeNull()
    const page = missingPage(host)
    expect(page.querySelector('.page-title')?.textContent).toBe(
      'No such directory',
    )
    expect(page.querySelector('.missing')?.textContent).toBe(NO_SUCH_DIR)
    expect(link(page, '‹ Manga').getAttribute('href')).toBe(
      '/config/libraries/Manga',
    )
  })

  it.each([
    { what: 'a library', entry: '/config/libraries/Novels' },
    { what: 'a directory', entry: '/config/libraries/Manga/dirs/20' },
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
      first: '/config/libraries/Novels',
      second: '/config/libraries/Westerns',
    },
    {
      what: 'directory',
      first: '/config/libraries/Manga/dirs/77',
      second: '/config/libraries/Manga/dirs/78',
    },
  ])(
    'puts focus on the no-such-$what line again when a second address naming nothing follows the first',
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
    const host = await at('/config/libraries/Manga')
    expect(host.querySelector('.missing')).toBeNull()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
    await follow(host, '/media/Manga')
    const missing = host.querySelector('.missing')
    expect(missing?.textContent).toBe(NO_SUCH_LIBRARY)
    expect(document.activeElement).toBe(missing)
  })

  it.each([
    {
      what: 'a library’s removal goes to the table',
      from: '/config/libraries/Manga',
      to: TABLE,
      page: '.library-table',
      act: removeDrawnLibrary,
    },
    {
      what: 'a directory’s removal goes to its library’s page',
      from: '/config/libraries/Manga/dirs/11',
      to: '/config/libraries/Manga',
      page: '.library-page',
      act: removeDrawnDir,
    },
    {
      what: 'the removal of a library’s last directory goes to the table',
      from: '/config/libraries/Comics/dirs/20',
      to: TABLE,
      page: '.library-table',
      act: removeDrawnDir,
    },
    {
      what: 'a directory’s move goes to the page of the library it left',
      from: '/config/libraries/Manga/dirs/11',
      to: '/config/libraries/Manga',
      page: '.library-page',
      act: (host: HTMLElement) => moveDrawnDir(host, '2'),
      moved: {},
    },
    {
      what: 'a move that took its library goes to the table',
      from: '/config/libraries/Comics/dirs/20',
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
      const host = await at('/config/libraries/Manga/dirs/11')
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
      to: '/config/libraries/Manga',
    },
    {
      by: 'a link',
      leave: (host: HTMLElement) => follow(host, '‹ Manga'),
      to: '/config/libraries/Manga',
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
      const host = await at('/config/libraries/Manga')
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
      expect(host.querySelector('.dir-page')).toBeNull()
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
    const host = await at('/config/libraries/Manga')
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
    const host = await at('/config/libraries/Manga/dirs/10')
    await typeInto(
      host.querySelector('input[name="default_language"]') as HTMLInputElement,
      'ja',
    )
    await follow(host, '‹ Manga')
    expect(here()).toBe('/config/libraries/Manga')
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
    expect(here()).toBe('/config/libraries/Manga')
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('still reads the list after a save on a directory’s page that follows a directory’s removal', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    const host = await at('/config/libraries/Manga/dirs/11')
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
    const host = await at('/config/libraries/Manga/dirs/11')
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
      vi.mocked(status.status).mockClear()
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
      expect(here()).toBe('/config/libraries/Manga')
      const before = host.querySelector(
        'button[name="scan-library"]',
      ) as HTMLButtonElement
      act(() => before.focus())
      await answer()

      expect(here()).toBe('/config/libraries/Manga')
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
      await go('/config/libraries/Comics')
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
      expect(here()).toBe('/config/libraries/Comics')
      expect(libraryTitle(host)).toBe('Comics')
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
      await go('/config/libraries/Manga')
      expect(libraryTitle(host)).toBe('Manga')
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
    'leaves the person on another Configuration tab when a directory’s %s answers there, reads the sidebar’s status once, and the list on the return',
    async (kind) => {
      const { host, answer } = await started(kind)
      await go('/config/users')
      expect(host.querySelector('.other-tab')).not.toBeNull()
      await answer()
      expect(here()).toBe('/config/users')
      expect(host.querySelector('.other-tab')).not.toBeNull()
      // The sidebar's libraries follow the write from any page; the list,
      // which nothing draws here, is read once something draws it.
      expect(status.status).toHaveBeenCalledTimes(1)
      expect(api.listLibraries).not.toHaveBeenCalled()
      await go(TABLE)
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
    },
  )

  it.each(['removal', 'move'] as const)(
    'goes up from a directory page the person left and opened again before its %s answered, with the report, and reads the list once',
    async (kind) => {
      const { host, answer } = await started(kind)
      await follow(host, '‹ Manga')
      await follow(host, '/media/Manga/Chapters')
      expect(here()).toBe('/config/libraries/Manga/dirs/11')
      await answer()
      expect(here()).toBe('/config/libraries/Manga')
      const report = host.querySelector('.library-page > .removal-report')
      expect(report?.querySelector(':scope > p')?.textContent).toBe(said[kind])
      expect(document.activeElement).toBe(report)
      expect(api.listLibraries).toHaveBeenCalledTimes(1)
    },
  )
})

describe('Libraries, a write refused once its page has gone', () => {
  const MANGA = '/config/libraries/Manga'
  const CHAPTERS = '/config/libraries/Manga/dirs/11'
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
      to: '/config/libraries/Manga',
    },
    {
      where: 'on the table',
      leave: (host: HTMLElement) => follow(host, '‹ Libraries'),
      to: TABLE,
    },
    {
      where: 'on another library’s page',
      leave: (_host: HTMLElement) => go('/config/libraries/Comics'),
      to: '/config/libraries/Comics',
    },
    {
      where: 'on another Configuration tab',
      leave: (_host: HTMLElement) => go('/config/users'),
      to: '/config/users',
    },
  ])(
    'leaves the person at $to when a make answers while they are $where, and reads the sidebar’s status once and the list once where it is drawn',
    async ({ leave, to }) => {
      const comics = library(2, 'Comics', ['/media/Comics'])
      const made = library(1, 'Manga', ['/media/Manga'])
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [comics] })
      const create = deferred<api.Created>()
      vi.mocked(api.createLibrary).mockReturnValue(create.promise)
      const host = await at(NEW_LIBRARY)
      await makeOne(host, 'Manga', '/media/Manga')
      await leave(host)

      vi.mocked(api.listLibraries).mockResolvedValue({
        libraries: [comics, made],
      })
      vi.mocked(api.listLibraries).mockClear()
      vi.mocked(status.status).mockClear()
      await flush(() => create.resolve({ library: made, added: [] }))
      await flush()
      expect(here()).toBe(to)
      // The sidebar's libraries follow the make from any page. The list is
      // read where a page of the screen draws it, and on another tab once
      // the person is back.
      expect(status.status).toHaveBeenCalledTimes(1)
      expect(api.listLibraries).toHaveBeenCalledTimes(
        to === '/config/users' ? 0 : 1,
      )
      if (to === TABLE) expect(names(host)).toEqual(['Comics', 'Manga'])
      if (to === '/config/libraries/Manga') {
        expect(libraryTitle(host)).toBe('Manga')
      }
    },
  )

  it('leaves a fresh make form where it is when the make sent from an earlier one answers, and reads the list once', async () => {
    const create = deferred<api.Created>()
    vi.mocked(api.createLibrary).mockReturnValue(create.promise)
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'First', '/media/first')
    await follow(host, '‹ Libraries')
    await follow(host, 'New library')
    await typeInto(nameBox(host), 'Second, half typed')
    expect(here()).toBe(NEW_LIBRARY)

    vi.mocked(api.listLibraries).mockClear()
    await flush(() =>
      create.resolve({
        library: library(3, 'First', ['/media/first']),
        added: [],
      }),
    )
    await flush()
    expect(here()).toBe(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
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
    expect(here()).toBe('/config/libraries/Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
    await follow(host, '/media/Manga/Volumes')
    expect(regime(host)).toContain('probe failed: no events')
  })

  it('puts focus on the made library’s heading, and only once', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(here()).toBe('/config/libraries/Manga')
    const heading = libraryHeading(host) as HTMLElement
    expect(heading.textContent).toBe('Manga')
    expect(document.activeElement).toBe(heading)

    // Back to the page by a link later: the heading is not taken again.
    await follow(host, '‹ Libraries')
    await follow(host, 'Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(document.activeElement).not.toBe(libraryHeading(host))
  })

  it('puts focus on the table’s heading when the make form is cancelled', async () => {
    const host = await at(NEW_LIBRARY)
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
    const heading = tableHeading(host)
    expect(heading?.textContent).toBe('Libraries')
    expect(document.activeElement).toBe(heading)
  })

  it('leaves focus alone on the table the person goes back to from the made library’s page', async () => {
    // The arrival belongs to the page it sent the person to, so the table
    // opened from there is opened by the person.
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(document.activeElement).toBe(libraryHeading(host))
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
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Manga', '/media/Manga')
    await follow(host, '‹ Libraries')
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await flush(() => create.resolve({ library: made, added: [] }))
    await flush()
    await follow(host, 'Manga')
    expect(here()).toBe('/config/libraries/Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(document.activeElement).not.toBe(libraryHeading(host))
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
      const host = await at(NEW_LIBRARY)
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

      await go('/config/libraries/Comics')
      expect(libraryTitle(host)).toBe('Comics')
      expect(host.querySelector('[role="alert"]')).toBeNull()
      // Nor drawn again on the table.
      await go(TABLE)
      expect(host.querySelector('[role="alert"]')).toBeNull()
    },
  )

  it('sends the whole path when Enter follows the last keystroke before the form has drawn it', async () => {
    const made = library(1, 'Trial', ['/media/Trial-a'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
    await typeInto(nameBox(host), 'Manga')
    const box = pathBox(host)
    await typeInto(box, '/media/Manga')
    box.removeAttribute('name')
    await submit(form(host))
    expect(api.createLibrary).not.toHaveBeenCalled()
    expect(here()).toBe(NEW_LIBRARY)
    expect(nameBox(host).value).toBe('Manga')
    expect(box.value).toBe('/media/Manga')
  })

  describe('its wait, drawn only once it has lasted', () => {
    const cancel = (host: HTMLElement) =>
      form(host).querySelector('button[name="cancel"]') as HTMLButtonElement
    /** Make the library's words, and whether it and Cancel are drawn held. */
    const state = (host: HTMLElement) => [
      makeButton(host).textContent,
      makeButton(host).disabled,
      cancel(host).disabled,
    ]
    const ready = ['Make the library', false, false]
    async function filled() {
      const host = await at(NEW_LIBRARY)
      await typeInto(nameBox(host), 'Manga')
      await typeInto(pathBox(host), '/media/Manga')
      return host
    }

    it('draws no wait and no held button for a make refused inside the wait’s delay', async () => {
      holdTheClock()
      const create = deferred<api.Created>()
      vi.mocked(api.createLibrary).mockReturnValue(create.promise)
      const host = await filled()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await submit(form(host))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() =>
        create.reject(
          new Error(
            '/api/library/libraries: 400 {"error":"/media/Manga: No such file or directory (os error 2)"}',
          ),
        ),
      )
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(form(host).querySelector('[role="alert"]')?.textContent).toBe(
        '/media/Manga: No such file or directory (os error 2)',
      )
      expect([...drawn.seen]).toEqual([JSON.stringify(ready)])
    })

    it('draws Checking… and holds Make the library and Cancel from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
      const host = await filled()
      await submit(form(host))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(ready)
      pass(1)
      expect(state(host)).toEqual(['Checking…', true, true])
    })

    it('holds a second make and Cancel from the send, before either is drawn held', async () => {
      holdTheClock()
      vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
      const host = await filled()
      await submit(form(host))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(ready)
      await submit(form(host))
      expect(api.createLibrary).toHaveBeenCalledTimes(1)
      await press(cancel(host))
      expect(here()).toBe(NEW_LIBRARY)
    })

    it('moves focus from Make the library to the name box once its wait is drawn, and keeps it there through a refusal', async () => {
      holdTheClock()
      const create = deferred<api.Created>()
      vi.mocked(api.createLibrary).mockReturnValue(create.promise)
      const host = await filled()
      act(() => makeButton(host).focus())
      await submit(form(host))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(document.activeElement).toBe(makeButton(host))
      pass(1)
      expect(state(host)).toEqual(['Checking…', true, true])
      expect(document.activeElement).toBe(nameBox(host))

      await flush(() =>
        create.reject(
          new Error(
            '/api/library/libraries: 400 {"error":"/media/Manga: No such file or directory (os error 2)"}',
          ),
        ),
      )
      expect(form(host).querySelector('[role="alert"]')?.textContent).toBe(
        '/media/Manga: No such file or directory (os error 2)',
      )
      expect(document.activeElement).toBe(nameBox(host))
    })

    it('moves focus from Cancel to the name box once the make’s wait holds it', async () => {
      holdTheClock()
      vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
      const host = await filled()
      await submit(form(host))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      // Tabbed to inside the wait's delay.
      act(() => cancel(host).focus())
      expect(document.activeElement).toBe(cancel(host))
      pass(1)
      expect(state(host)).toEqual(['Checking…', true, true])
      expect(document.activeElement).toBe(nameBox(host))
    })

    it('puts focus on the made library’s heading after a make whose wait was drawn, as after one answered at once', async () => {
      holdTheClock()
      const create = deferred<api.Created>()
      vi.mocked(api.createLibrary).mockReturnValue(create.promise)
      const made = library(1, 'Manga', ['/media/Manga'])
      const host = await filled()
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
      act(() => makeButton(host).focus())
      await submit(form(host))
      pass(WAIT_DRAWN_AFTER_MS)
      expect(document.activeElement).toBe(nameBox(host))

      await flush(() => create.resolve({ library: made, added: [] }))
      await until(() => libraryHeading(host) !== null)
      expect(here()).toBe('/config/libraries/Manga')
      expect(libraryTitle(host)).toBe('Manga')
      expect(document.activeElement).toBe(libraryHeading(host))
    })
  })

  it('sends the directory’s path without the white space around it', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Manga', '  /media/Manga \t')
    expect(api.createLibrary).toHaveBeenCalledWith('Manga', ['/media/Manga'])
  })

  it('holds Make the library, and says nothing of why, while the name or the directory box is empty', async () => {
    // The box's path is the directory sent, so there is no path waiting to be
    // added and nothing to explain: what is missing is an empty box in view.
    const host = await at(NEW_LIBRARY)
    const said = () =>
      form(host).querySelectorAll('.why, .settings-row-help, [role="alert"]')
        .length
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
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Manga', '/media/typo')

    expect(form(host).querySelector('[role="alert"]')?.textContent).toBe(
      '/media/typo: No such file or directory (os error 2)',
    )
    // Beside the button that was pressed, and not the list's slot.
    expect(host.querySelector('.new-library > [role="alert"]')).toBeNull()
    expect(nameBox(host).value).toBe('Manga')
    expect(pathBox(host).value).toBe('/media/typo')
    expect(here()).toBe(NEW_LIBRARY)
  })

  it('draws, under the form and as it is typed, why a library cannot be called . or .., holds the make and sends nothing', async () => {
    // The server refuses both names once trimmed, since no address can hold
    // either; the page says so in the server's sentence before the make.
    const why = 'An address cannot hold “.” or “..” as a name.'
    const host = await at(NEW_LIBRARY)
    await typeInto(pathBox(host), '/media/Manga')
    for (const name of ['.', ' .. ']) {
      await typeInto(nameBox(host), name)
      expect(
        [...host.querySelectorAll('[role="alert"], [role="status"]')]
          .filter((a) => a.textContent !== '')
          .map((a) => [
            a.parentElement?.matches('form.add-library'),
            a.textContent,
          ]),
        JSON.stringify(name),
      ).toEqual([[true, why]])
      expect(makeButton(host).disabled).toBe(true)
    }
    await submit(form(host))
    expect(api.createLibrary).not.toHaveBeenCalled()
  })

  it('says as it is typed that a library holds the name in another case, tied to the box, and holds the make at once until the name is free', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    const host = await at(NEW_LIBRARY)
    await typeInto(pathBox(host), '/media/x')
    await typeInto(nameBox(host), 'Manga')
    const note = clashNote(host) as HTMLElement
    // Drawn, empty, before it has anything to say, so that its words are
    // read out as they come.
    expect(note.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])
    expect(makeButton(host).disabled).toBe(false)

    await typeInto(nameBox(host), 'COMICS')
    expect(clashNote(host)).toBe(note)
    expect(note.textContent).toBe(
      'a library is already called `Comics`; add the directory to it instead',
    )
    expect(boxMarks(host)).toEqual(['true', note.id])
    // At once: no wait is drawn for it.
    expect(makeButton(host).disabled).toBe(true)
    expect(makeButton(host).textContent).toBe('Make the library')

    // A keystroke that keeps the clash changes nothing that is read out.
    const changes = changesIn(note)
    await typeInto(nameBox(host), 'comics')
    expect(changes()).toBe(0)

    await typeInto(nameBox(host), 'Comics 2')
    expect(clashNote(host)).toBe(note)
    expect(note.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])
    expect(makeButton(host).disabled).toBe(false)
  })

  it('moves focus from Make the library to the name box when the list read holds it', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const host = await at(NEW_LIBRARY)
    await typeInto(nameBox(host), 'Comics')
    await typeInto(pathBox(host), '/media/x')
    act(() => makeButton(host).focus())
    expect(document.activeElement).toBe(makeButton(host))
    await flush(() =>
      first.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    expect(makeButton(host).disabled).toBe(true)
    expect(document.activeElement).toBe(nameBox(host))
  })

  it('leaves focus in the path box when the list read holds Make the library', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const host = await at(NEW_LIBRARY)
    await typeInto(nameBox(host), 'Comics')
    await typeInto(pathBox(host), '/media/x')
    act(() => pathBox(host).focus())
    await flush(() =>
      first.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    expect(makeButton(host).disabled).toBe(true)
    expect(document.activeElement).toBe(pathBox(host))
  })

  it('sends nothing on Enter in the form while the name clashes', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'comics', '/media/x')
    expect(api.createLibrary).not.toHaveBeenCalled()
    expect(here()).toBe(NEW_LIBRARY)
  })

  it('draws the newest list when two reads answer out of order', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(libraryTitle(host)).toBe('Manga')

    await flush(() => first.resolve({ libraries: [] }))
    expect(libraryTitle(host)).toBe('Manga')
    expect(host.querySelector('.missing')).toBeNull()
  })

  it('keeps a create refusal when the page’s own read of the list answers after it', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries: 400 {"error":"a library is already called `Comics`; add the directory to it instead"}',
      ),
    )
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Comics', '/media/x')
    expect(form(host).querySelector('[role="alert"]')?.textContent).toContain(
      'already called',
    )

    await flush(() =>
      first.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    // Said once: the list now knows the name too, and its clash is the
    // refusal's own sentence, which waits until an edit takes the refusal
    // away.
    expect(
      [...form(host).querySelectorAll('[role="alert"], [role="status"]')]
        .filter((one) => one.textContent !== '')
        .map((one) => [one.getAttribute('role'), one.textContent]),
    ).toEqual([
      [
        'alert',
        'a library is already called `Comics`; add the directory to it instead',
      ],
    ])
    expect(nameBox(host).value).toBe('Comics')
    expect(pathBox(host).value).toBe('/media/x')
  })

  it('says the clash a list read brings beside a refusal in other words, and holds Make the library with it', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries: 400 {"error":"/media/x: No such file or directory (os error 2)"}',
      ),
    )
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Comics', '/media/x')

    await flush(() =>
      first.resolve({ libraries: [library(2, 'Comics', ['/media/Comics'])] }),
    )
    expect(
      [...form(host).querySelectorAll('[role="alert"], [role="status"]')]
        .filter((one) => one.textContent !== '')
        .map((one) => [one.getAttribute('role'), one.textContent]),
    ).toEqual([
      ['alert', '/media/x: No such file or directory (os error 2)'],
      [
        'status',
        'a library is already called `Comics`; add the directory to it instead',
      ],
    ])
    expect(boxMarks(host)).toEqual(['true', clashNote(host)?.id])
    expect(makeButton(host).disabled).toBe(true)
  })

  it('sends one create for two submits', async () => {
    vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
    const host = await at(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Manga', '/media/Manga')
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await submit(form(host))
    await flush()
    expect(here()).toBe('/config/libraries/Manga')
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
    const host = await at(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Manga', '/media/Manga')
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).toBeNull()
  })

  it('keeps the list’s failure when the make form is edited', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    const host = await at(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
    expect(host.querySelector('.new-library > [role="alert"]')).not.toBeNull()

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
  })

  it('draws no failure from a read that a newer one has superseded', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, 'Manga', '/media/Manga')
    await flush(() =>
      first.reject(
        new Error('/api/library/libraries: 500 {"error":"too late"}'),
      ),
    )
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
    expect(libraryTitle(host)).toBe('Manga')
  })

  it('draws the library that was just made at once, when the list that follows cannot be read', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await makeOne(host, 'Manga', '/media/Manga')
    expect(here()).toBe('/config/libraries/Manga')
    expect(libraryTitle(host)).toBe('Manga')
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
    const host = await at(NEW_LIBRARY)
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
    const host = await at(NEW_LIBRARY)
    // Named, because a loop over nothing passes.
    const labelled = [
      ...form(host).querySelectorAll<HTMLLabelElement>(
        'label.settings-row-label',
      ),
    ].map((label) => ({ written: label.textContent, control: label.control }))
    expect(labelled.map(({ written }) => written)).toEqual([
      'Name',
      'Directory path',
    ])
    for (const { written, control } of labelled) {
      expect(control?.getAttribute('aria-label'), written ?? '').toBe(written)
    }
  })

  it('draws the make form as one box of Name and Directory path, with Make the library and Cancel at its foot', async () => {
    const host = await at(NEW_LIBRARY)
    const page = host.querySelector('.new-library') as HTMLElement
    expect(page.querySelector('.page-header .page-title')?.textContent).toBe(
      'New library',
    )
    expect(link(page, '‹ Libraries').getAttribute('href')).toBe(TABLE)
    const boxes = page.querySelectorAll('.settings-box')
    expect(boxes).toHaveLength(1)
    const made = boxes[0].querySelector(':scope > form.add-library') as Element
    const parts = [...made.children].map((child) =>
      child.matches('.settings-row')
        ? child.querySelector('.settings-row-label')?.textContent
        : child.matches('[role="status"]')
          ? { says: child.textContent }
          : [...child.querySelectorAll('button')].map((b) => b.textContent),
    )
    // Last, why the name typed would be refused, empty until it is.
    expect(parts).toEqual([
      'Name',
      'Directory path',
      ['Make the library', 'Cancel'],
      { says: '' },
    ])
    // Browse… is the directory row's, with the box it writes a path into.
    expect(
      row(made, 'Directory path').querySelector('button[name="browse"]'),
    ).not.toBeNull()
    expect(page.querySelector('.actions')).toBeNull()
  })

  it('wires each of the make form’s rows to its control, and its help to the control it explains', async () => {
    const host = await at(NEW_LIBRARY)
    expectRowsWired(host.querySelector('.new-library') as HTMLElement)
  })

  it('says each fact the make page’s help says beside what it explains', async () => {
    const host = await at(NEW_LIBRARY)
    const page = host.querySelector('.new-library') as HTMLElement
    expect(
      page.querySelector('.page-header .page-description')?.textContent,
    ).toBe(
      'A library starts with one directory. More can be added on its page: the directories of one library are derived together, so a chapter release in one joins its volume in another as a second edition.',
    )
    expect(helpOf(row(page, 'Name'))).toBeNull()
    expect(helpOf(row(page, 'Directory path'))?.textContent).toBe(
      'The server checks that it can read the directory and that it overlaps no other directory.',
    )
    expect(page.querySelector('.why')).toBeNull()
  })
})

describe('Libraries, a probe', () => {
  it('does not hand a directory the probe of a removed one that shared its id', async () => {
    const first = library(1, 'Manga', ['/srv/first'])
    // The list holds each library once it is made.
    vi.mocked(api.createLibrary).mockImplementationOnce(async () => {
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
      return {
        library: first,
        added: [
          {
            root: first.dirs[0],
            watching: false,
            watch_reason: 'inotify limit reached',
            interval_secs: 900,
          },
        ],
      }
    })
    const host = await at(NEW_LIBRARY)
    await makeOne(host, 'Manga', '/srv/first')
    await follow(host, '/srv/first')
    expect(host.textContent).toContain('inotify limit reached')

    // The directory's id, freed and given to another directory by the store.
    const second = {
      ...first,
      dirs: [dir(first.dirs[0].id, '/srv/second')],
    }
    vi.mocked(api.createLibrary).mockImplementationOnce(async () => {
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [second] })
      return { library: second, added: [] }
    })
    // The first is gone, as the freed id says.
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
    await go(NEW_LIBRARY)
    await makeOne(host, 'Manga', '/srv/second')
    await follow(host, '/srv/second')
    expect(here()).toBe('/config/libraries/Manga/dirs/10')
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
    const host = await at('/config/libraries/Manga/dirs/10')
    expect(edition(host)).toBe('first')

    // A save on this page reads the list again, and the directory at this id is
    // now another one.
    const second = library(1, 'Manga', ['/srv/second'])
    second.dirs[0].edition_key = 'second'
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [second] })
    await press(host.querySelector('input[name="keeps_documents"]') as Element)
    await flush()
    expect(dirTitle(host)).toBe('/srv/second')
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
      ['Scan directory', 'Move…', 'Remove…'].map((text) =>
        holdOf(button(host, text)),
      )
    const host = await at('/config/libraries/Manga/dirs/10')
    expect(held(host)).toEqual(['held', 'held', 'held'])

    await go('/config/libraries/Manga/dirs/11')
    expect(dirTitle(host)).toBe('/media/Manga/Chapters')
    expect(held(host)).toEqual(['free', 'free', 'free'])
    await flush(() => later.resolve(idle(1)))
    expect(held(host)).toEqual(['free', 'free', 'free'])
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
    const host = await at(NEW_LIBRARY)
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
    const host = await at('/config/libraries/Manga')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [{ ...manga, dirs: [...manga.dirs, extra] }],
    })
    await press(host.querySelector('button[name="add-dir"]') as Element)
    const picker = section(host, 'Directories').querySelector(
      ':scope > .settings-box > .add-root',
    ) as HTMLElement
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    await until(() => paths(host).length === 2)
    expect(paths(host)).toEqual(['/media/Manga', '/media/Manga/Extras'])
    await follow(host, '/media/Manga/Extras')
    expect(here()).toBe('/config/libraries/Manga/dirs/9')
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
    const host = await at('/config/libraries/Manga')
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
    // The list holds the library once it is made.
    vi.mocked(api.createLibrary).mockImplementation(async () => {
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
      return { library: made, added: [] }
    })
    await makeOne(host, 'Comics', '/media/Comics')
    expect(libraryTitle(host)).toBe('Comics')
    expect(host.querySelector('.removal-report')).toBeNull()
  })

  it('moves focus to the removal report, which is named, when one is drawn', async () => {
    twoLibraries()
    const host = await at('/config/libraries/Manga')
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
    const host = await at('/config/libraries/Manga')
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
    const host = await at('/config/libraries/Comics')
    vi.mocked(api.listLibraries).mockRejectedValue(locked())
    await removeDrawnLibrary(host)
    expect(names(host)).toEqual(['Manga'])
  })

  it('takes focus once for a report, and leaves it where the person puts it when the list that follows answers', async () => {
    twoLibraries()
    const host = await at('/config/libraries/Manga')
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
    const host = await at('/config/libraries/Manga')
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
    const host = await at('/config/libraries/Comics')
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
        .filter((b) => holdOf(b) !== 'free')
        .map((b) => `${b.textContent} ${holdOf(b)}`)
    await press(host.querySelector('button[name="scan-library"]') as Element)

    // Held: the library's own scan and removal. Its directories' buttons are on
    // their own pages.
    expect(held()).toEqual(['Scan library held', 'Remove… held'])
    expect(host.querySelector('.library-page .scanning')).not.toBeNull()

    // Straight from one library's page to the other's, as a typed address
    // goes, so the screen is not unmounted between them. Manga's own read of
    // its status is held open: what is drawn until it answers is a page of
    // its own, not Comics' carried over, and so is the body it draws once
    // that read says nothing is running there.
    const mangaStatus = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
      id === 1
        ? mangaStatus.promise
        : Promise.resolve({
            library_id: id,
            running: true,
            elapsed_secs: 1,
            dirs: [],
          }),
    )
    await go('/config/libraries/Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(held()).toEqual([])
    expect(host.querySelector('.library-page .scanning')).toBeNull()

    await flush(() => mangaStatus.resolve(idle(1)))
    expect(paths(host)).toEqual(['/media/Manga'])
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
    const host = await at('/config/libraries/Manga/dirs/11')
    const after = [
      library(1, 'Manga', ['/media/Manga/Volumes']),
      library(2, 'Comics', ['/media/Comics', '/media/Manga/Chapters']),
    ]
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: after })
    await moveDrawnDir(host, '2')

    expect(here()).toBe('/config/libraries/Manga')
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
    const host = await at('/config/libraries/Manga/dirs/11')
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
    const host = await at('/config/libraries/Manga/dirs/11')
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
    const host = await at('/config/libraries/Manga/dirs/10')
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
    const host = await at('/config/libraries/Manga/dirs/11')
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
    expect(libraryTitle(host)).toBe('Manga')
    expect(paths(host)).toEqual(['/media/Manga/Volumes'])
  })

  it('drops a directory’s report once its library’s page is left, so the page opened again draws none', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two()] })
    const host = await at('/config/libraries/Manga/dirs/11')
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga/Volumes'])],
    })
    await removeDrawnDir(host)
    expect(host.querySelector('.library-page > .removal-report')).not.toBeNull()

    await follow(host, '/media/Manga/Volumes')
    expect(host.querySelector('.removal-report')).toBeNull()
    await follow(host, '‹ Manga')
    expect(here()).toBe('/config/libraries/Manga')
    expect(host.querySelector('.removal-report')).toBeNull()
    expect(document.activeElement?.closest('.removal-report')).toBeNull()
  })

  it('drops a removed directory at once when the list that follows cannot be read', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two()] })
    const host = await at('/config/libraries/Manga/dirs/11')
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
    expect(holdOf(remove)).toBe('free')
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
    const host = await at('/config/libraries/Manga/dirs/11')
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
    const host = await at('/config/libraries/Manga/dirs/10')
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
    const host = await at('/config/libraries/Manga/dirs/10')
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

describe('Libraries, side by side', () => {
  afterEach(() => {
    vi.mocked(useFitsPair).mockReset()
  })

  it('draws a directory alone where the two do not fit, with no library page to read for', async () => {
    vi.mocked(useFitsPair).mockReturnValue(false)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at(`${TABLE}/Manga/dirs/10`)
    expect(host.querySelector('.dir-page .page-title')?.textContent).toBe(
      '/media/Manga/Volumes',
    )
    expect(host.querySelector('.dir-table')).toBeNull()
    expect(host.querySelector('.library-page')).toBeNull()
    // The directory's page asks once whether another directory is
    // scanning. The library's page, not drawn, asks nothing.
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  it('draws the library page beside the directory where they fit, its row marked as the open one', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at(`${TABLE}/Manga/dirs/10`)
    expect(host.querySelector('.dir-page .page-title')?.textContent).toBe(
      '/media/Manga/Volumes',
    )
    expect(paths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    const rows = [...host.querySelectorAll('.dir-table tbody .row-link')]
    expect(rows.map((a) => a.getAttribute('aria-current'))).toEqual([
      'page',
      null,
    ])
  })

  // §9's call 9: the directory's page marks the library's status stale and
  // tells no reader, so the library's page beside it draws what it drew
  // before, where a re-read would take the scan up and draw its report a
  // second time beside the directory's own.
  it('leaves the library’s page drawn beside its directory’s as it was through the directory’s own scan and its end: it reads no status, holds nothing, and draws no report of its own', async () => {
    vi.useFakeTimers({
      toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'],
    })
    vi.mocked(useFitsPair).mockReturnValue(true)
    const libraries = twoAndOne()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
    const host = await at(`${TABLE}/Manga/dirs/10`)
    expect(dirTitle(host)).toBe('/media/Manga/Volumes')
    expect(libraryTitle(host)).toBe('Manga')
    const libraryHeld = () =>
      JSON.stringify(
        ['scan-library', 'remove-library'].map((name) =>
          holdOf(
            host.querySelector(
              `.library-page button[name="${name}"]`,
            ) as Element,
          ),
        ),
      )
    const held = watchDrawn(libraryHeld)
    vi.mocked(api.libraryScanStatus).mockClear()
    // A read of the library's status would find the directory scanning.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(libraries[0], 10),
    )
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve(ownStatus(id, id === 10)),
    )
    await scanDrawnDir(host, 10)
    expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()
    await tick()
    expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()

    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve(ownStatus(id, false)),
    )
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(1))
    await tick()
    await tick()
    await flush()
    held.stop()
    expect(host.querySelector('.dir-page > .scanning')).toBeNull()
    expect(host.querySelectorAll('.scan-report')).toHaveLength(1)
    expect(host.querySelector('.dir-page .scan-report')).not.toBeNull()
    expect([...held.seen]).toEqual([JSON.stringify(['free', 'free'])])
    expect(api.libraryScanStatus).not.toHaveBeenCalled()
  })

  // The library's read is out as the directory's mount read takes up the
  // scan, and answers as the server did before the scan began: the page
  // drawing it is answered, and a page arriving once it has gone waits.
  it('makes the library’s page wait on its next arrival when its read, out as the directory’s page beside it took up a scan, answered from before the scan', async () => {
    vi.useFakeTimers({
      toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'],
    })
    vi.mocked(useFitsPair).mockReturnValue(true)
    const libraries = twoAndOne()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries })
    const before: ReturnType<typeof deferred<api.LibraryScanStatus>>[] = []
    vi.mocked(api.libraryScanStatus).mockImplementation(() => {
      const read = deferred<api.LibraryScanStatus>()
      before.push(read)
      return read.promise
    })
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve(ownStatus(id, id === 10)),
    )
    const host = await at(`${TABLE}/Manga/dirs/10`)
    expect(host.querySelector('.dir-page > .scanning')).not.toBeNull()
    await flush(() => {
      for (const read of before) read.resolve(idle(1))
    })
    expect(scanState(host)).toBe(JSON.stringify(['free', 'Never', false, 0]))

    // The directory's page gives up after a run of failed polls, so nothing
    // it does marks the status again.
    vi.mocked(config.scanStatus).mockRejectedValue(
      new Error('/api/library/roots/10/scan: 500 the store is locked'),
    )
    for (let poll = 0; poll < config.POLL_MISSES_ALLOWED; poll++) await tick()
    expect(host.querySelector('.dir-page > .scanning')).toBeNull()

    await go('/config/users')
    const read = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValue(read.promise)
    await go(`${TABLE}/Manga`)
    expect(libraryTitle(host)).toBe('Manga')
    expect(scanState(host)).toBe(NO_SCAN_DRAWN)
    await flush(() => read.resolve(scanningIn(libraries[0], 10)))
    expect(scanState(host)).toBe(JSON.stringify(['held', 'Scanning…', true, 0]))
  })

  it('opens a library’s page on its own where a page would fit beside the table, as a library is not the end of its tree', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at()
    await follow(host, 'Manga')
    expect(here()).toBe('/config/libraries/Manga')
    expect(libraryTitle(host)).toBe('Manga')
    expect(host.querySelector('.library-table')).toBeNull()
    expect(host.querySelector('.split-pair')).toBeNull()
  })

  it('draws New library on its own where a page would fit beside the table', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at(NEW_LIBRARY)
    expect(host.querySelector('.new-library .page-title')?.textContent).toBe(
      'New library',
    )
    expect(host.querySelector('.library-table')).toBeNull()
  })

  // The list's failure stands for every notice: `Libraries.tsx` hands the
  // one set to the page open beside a library's, and to the library's page
  // while none is.
  it.each([
    {
      page: 'a directory’s page beside its library’s',
      from: `${TABLE}/Manga`,
      entry: `${TABLE}/Manga/dirs/10`,
      drawnIn: '.split-detail .dir-page',
    },
    {
      page: 'the page for a directory its library does not hold',
      from: `${TABLE}/Manga`,
      entry: `${TABLE}/Manga/dirs/99`,
      drawnIn: '.split-detail .settings-page',
    },
    {
      page: 'a library’s page with no directory open',
      from: `${TABLE}/Manga/dirs/10`,
      entry: `${TABLE}/Manga`,
      drawnIn: '.split-list .library-page',
    },
  ])(
    'says the list could not be read again once, on $page, where the two fit',
    async ({ from, entry, drawnIn }) => {
      vi.mocked(useFitsPair).mockReturnValue(true)
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
      const host = await at(from)
      vi.mocked(api.listLibraries).mockRejectedValue(locked())
      await go(entry)
      expect(here()).toBe(entry)
      const alerts = [...host.querySelectorAll('[role="alert"]')]
      expect(alerts.map((alert) => alert.textContent)).toEqual([
        'The libraries list could not be read again (the store is locked), so what is drawn may be out of date.',
      ])
      expect(alerts[0].closest(drawnIn)).not.toBeNull()
      // And one live region, so no notice can be said on the other page.
      expect(host.querySelectorAll('[aria-live="polite"]')).toHaveLength(1)
      // The library's page is drawn in every case, so one alert is not one
      // for want of a page to draw a second in.
      expect(libraryTitle(host)).toBe('Manga')
    },
  )

  it.each([
    { layout: 'beside its library’s page', fits: true },
    { layout: 'alone', fits: false },
  ])(
    'puts focus on the heading of a directory’s page its row opens $layout, and back on that row when its back link closes it',
    async ({ fits }) => {
      vi.mocked(useFitsPair).mockReturnValue(fits)
      vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
      const host = await at(`${TABLE}/Manga`)
      const opening = link(host, '/media/Manga/Chapters')
      act(() => opening.focus())
      await follow(host, '/media/Manga/Chapters')
      expect(here()).toBe(`${TABLE}/Manga/dirs/11`)
      expect(dirTitle(host)).toBe('/media/Manga/Chapters')
      expect(document.activeElement).toBe(
        host.querySelector('.dir-page .page-title'),
      )

      await follow(host, '‹ Manga')
      expect(here()).toBe(`${TABLE}/Manga`)
      expect(document.activeElement).toBe(link(host, '/media/Manga/Chapters'))
    },
  )
})

describe('Libraries, addressed by name', () => {
  afterEach(() => {
    vi.mocked(useFitsPair).mockReset()
  })

  /** Renames the library whose page is drawn to `name`, by its Save. */
  async function renameTo(host: HTMLElement, name: string) {
    await press(button(host, 'Rename…'))
    const box = host.querySelector(
      'form.rename-library input[name="library-name"]',
    ) as HTMLInputElement
    await typeInto(box, name)
    await submit(host.querySelector('form.rename-library') as Element)
    await flush()
  }

  /** A list of Manga alone that says the name a rename gave it once it has. */
  function mangaRenamedBy(to: string) {
    let renamed = false
    vi.mocked(api.listLibraries).mockImplementation(async () => ({
      libraries: [library(1, renamed ? to : 'Manga', ['/media/Manga'])],
    }))
    vi.mocked(api.renameLibrary).mockImplementation(async () => {
      renamed = true
      return { id: 1, name: to }
    })
  }

  it('opens a library at its name, and one named as a number at that name rather than at an id', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, '2', ['/media/Two']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await at(`${TABLE}/2`)
    expect(libraryTitle(host)).toBe('2')
    await go(`${TABLE}/Comics`)
    expect(libraryTitle(host)).toBe('Comics')
  })

  it('opens a library at its name in another ASCII case, as the store finds names', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const host = await at(`${TABLE}/mANGA`)
    expect(libraryTitle(host)).toBe('Manga')
  })

  it('reads a name holding the text %2F as written, not as the name with a slash', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'a/b', ['/media/Slash']),
        library(2, 'a%2Fb', ['/media/Text']),
      ],
    })
    const host = await at(`${TABLE}/a%252Fb`)
    expect(libraryTitle(host)).toBe('a%2Fb')
    await go(`${TABLE}/a%2Fb`)
    expect(libraryTitle(host)).toBe('a/b')
  })

  it('links each library and its directories at its name, escaped where an address needs it', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Light novels', ['/media/Light novels']),
        library(2, 'Tom & Jerry/Extras?', ['/media/Tom']),
      ],
    })
    const host = await at()
    expect(
      [...host.querySelectorAll('.library-table tbody .row-link')].map((a) => [
        a.textContent,
        a.getAttribute('href'),
      ]),
    ).toEqual([
      ['Light novels', `${TABLE}/Light%20novels`],
      ['Tom & Jerry/Extras?', `${TABLE}/Tom%20%26%20Jerry%2FExtras%3F`],
    ])
    await follow(host, 'Tom & Jerry/Extras?')
    expect(here()).toBe(`${TABLE}/Tom%20%26%20Jerry%2FExtras%3F`)
    expect(libraryTitle(host)).toBe('Tom & Jerry/Extras?')
    expect(link(host, '/media/Tom').getAttribute('href')).toBe(
      `${TABLE}/Tom%20%26%20Jerry%2FExtras%3F/dirs/20`,
    )
    await follow(host, '/media/Tom')
    expect(dirTitle(host)).toBe('/media/Tom')
    expect(link(host, '‹ Tom & Jerry/Extras?').getAttribute('href')).toBe(
      `${TABLE}/Tom%20%26%20Jerry%2FExtras%3F`,
    )
  })

  it('replaces the address with the new name as a rename lands, so Back goes past the old one, which then names nothing', async () => {
    mangaRenamedBy('Manga 2')
    const host = await at()
    await follow(host, 'Manga')
    expect(here()).toBe(`${TABLE}/Manga`)
    await renameTo(host, 'Manga 2')
    expect(here()).toBe(`${TABLE}/Manga%202`)
    expect(libraryTitle(host)).toBe('Manga 2')
    // The title takes focus as the editor closes, as it does wherever the
    // page is: the page is the same page at its new address.
    expect(document.activeElement).toBe(libraryHeading(host))
    await flush(() => steer?.(-1))
    await flush()
    expect(here()).toBe(TABLE)
    await go(`${TABLE}/Manga`)
    expect(missingPage(host).querySelector('.page-title')?.textContent).toBe(
      'No such library',
    )
  })

  it('keeps an open directory’s part of the address as a rename lands', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    mangaRenamedBy('Manga 2')
    const host = await at(`${TABLE}/Manga/dirs/10`)
    await renameTo(host, 'Manga 2')
    expect(here()).toBe(`${TABLE}/Manga%202/dirs/10`)
    expect(libraryTitle(host)).toBe('Manga 2')
    expect(dirTitle(host)).toBe('/media/Manga')
  })

  it('draws the renamed library’s page at its new address while the list is read again', async () => {
    const reading = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockResolvedValueOnce({
      libraries: [library(1, 'Manga', ['/media/Manga'])],
    })
    vi.mocked(api.listLibraries).mockReturnValue(reading.promise)
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 1, name: 'Manga 2' })
    const host = await at(`${TABLE}/Manga`)
    await renameTo(host, 'Manga 2')
    expect(here()).toBe(`${TABLE}/Manga%202`)
    expect(host.querySelector('.missing')).toBeNull()
    expect(libraryTitle(host)).toBe('Manga 2')
    expect(document.activeElement).toBe(libraryHeading(host))
  })

  it('drops a read of the list sent before a rename landed, which says the old name', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    const before = deferred<{ libraries: api.Library[] }>()
    const after = deferred<{ libraries: api.Library[] }>()
    const answer = deferred<{ id: number; name: string }>()
    vi.mocked(api.listLibraries)
      .mockResolvedValueOnce({
        libraries: [library(1, 'Manga', ['/media/Manga'])],
      })
      .mockReturnValueOnce(before.promise)
      .mockReturnValue(after.promise)
    vi.mocked(api.renameLibrary).mockReturnValue(answer.promise)
    const host = await at(`${TABLE}/Manga`)
    // Opening the directory beside the page sends a read, held here.
    await follow(host, '/media/Manga')
    await renameTo(host, 'Manga 2')
    // The old read lands just after the rename, before the page moves.
    await flush(() => {
      answer.resolve({ id: 1, name: 'Manga 2' })
      before.resolve({ libraries: [library(1, 'Manga', ['/media/Manga'])] })
    })
    expect(here()).toBe(`${TABLE}/Manga%202/dirs/10`)
    expect(host.querySelector('.missing')).toBeNull()
    expect(libraryTitle(host)).toBe('Manga 2')
  })

  it('reads the list once after a rename that moves the address', async () => {
    mangaRenamedBy('Manga 2')
    const host = await at(`${TABLE}/Manga`)
    const before = vi.mocked(api.listLibraries).mock.calls.length
    await renameTo(host, 'Manga 2')
    await flush()
    expect(here()).toBe(`${TABLE}/Manga%202`)
    expect(api.listLibraries).toHaveBeenCalledTimes(before + 1)
  })

  it('does not move a person who went to another library’s page before the rename landed', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    const answer = deferred<{ id: number; name: string }>()
    vi.mocked(api.renameLibrary).mockReturnValue(answer.promise)
    const host = await at(`${TABLE}/Manga`)
    await renameTo(host, 'Manga 2')
    await go(`${TABLE}/Comics`)
    await flush(() => answer.resolve({ id: 1, name: 'Manga 2' }))
    await flush()
    expect(here()).toBe(`${TABLE}/Comics`)
    expect(libraryTitle(host)).toBe('Comics')
  })

  /**
   * Manga, of two directories, and Comics. A rename of Manga to `to` and a
   * removal of Chapters each change what the list says from then on; the
   * removal answers once `removed` lets it go.
   */
  function mangaRenamedAndRemoved(to: string) {
    let name = 'Manga'
    let paths = ['/media/Manga/Volumes', '/media/Manga/Chapters']
    vi.mocked(api.listLibraries).mockImplementation(async () => ({
      libraries: [
        library(1, name, paths),
        library(2, 'Comics', ['/media/Comics']),
      ],
    }))
    vi.mocked(api.renameLibrary).mockImplementation(async () => {
      name = to
      return { id: 1, name: to }
    })
    const removed = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockImplementation(async () => {
      const answer = await removed.promise
      paths = paths.filter((one) => one !== '/media/Manga/Chapters')
      return answer
    })
    return removed
  }

  const CHAPTERS_REMOVED =
    'What re-deriving after removing /media/Manga/Chapters found:'

  it('keeps a directory’s removal report on its library’s page through a rename of the library, which takes focus no more', async () => {
    const removed = mangaRenamedAndRemoved('Shelves')
    const host = await at(`${TABLE}/Manga/dirs/11`)
    await removeDrawnDir(host)
    await flush(() => removed.resolve(removal(4)))
    await flush()
    expect(here()).toBe(`${TABLE}/Manga`)
    const report = host.querySelector('.library-page > .removal-report')
    expect(report?.querySelector(':scope > p')?.textContent).toBe(
      CHAPTERS_REMOVED,
    )
    expect(document.activeElement).toBe(report)

    await renameTo(host, 'Shelves')
    expect(here()).toBe(`${TABLE}/Shelves`)
    expect(libraryTitle(host)).toBe('Shelves')
    const kept = host.querySelector('.library-page > .removal-report')
    expect(kept?.querySelector(':scope > p')?.textContent).toBe(
      CHAPTERS_REMOVED,
    )
    // The editor's close puts focus on the title; the report took focus once,
    // when its answer moved the person to it.
    expect(document.activeElement).toBe(libraryHeading(host))
  })

  it('takes a directory’s removal answered after its library’s rename landed beside it to the renamed library’s page, with its report', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    const removed = mangaRenamedAndRemoved('Shelves')
    const host = await at(`${TABLE}/Manga/dirs/11`)
    const dirPage = host.querySelector('.dir-page') as HTMLElement
    await press(button(dirPage, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(config.removeRoot).toHaveBeenCalledTimes(1)
    await renameTo(host, 'Shelves')
    expect(here()).toBe(`${TABLE}/Shelves/dirs/11`)

    await flush(() => removed.resolve(removal(4)))
    await flush()
    expect(here()).toBe(`${TABLE}/Shelves`)
    expect(host.querySelector('.missing')).toBeNull()
    const report = host.querySelector('.library-page > .removal-report')
    expect(report?.querySelector(':scope > p')?.textContent).toBe(
      CHAPTERS_REMOVED,
    )
    expect(document.activeElement).toBe(report)
  })

  it('takes a directory’s removal answered in the same turn as its library’s rename, before the rename is drawn, to the renamed library’s page with its report', async () => {
    vi.mocked(useFitsPair).mockReturnValue(true)
    let name = 'Manga'
    let paths = ['/media/Manga/Volumes', '/media/Manga/Chapters']
    vi.mocked(api.listLibraries).mockImplementation(async () => ({
      libraries: [
        library(1, name, paths),
        library(2, 'Comics', ['/media/Comics']),
      ],
    }))
    const renamed = deferred<{ id: number; name: string }>()
    vi.mocked(api.renameLibrary).mockReturnValue(renamed.promise)
    const removed = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockReturnValue(removed.promise)
    const host = await at(`${TABLE}/Manga/dirs/11`)
    const dirPage = host.querySelector('.dir-page') as HTMLElement
    await press(button(dirPage, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(config.removeRoot).toHaveBeenCalledTimes(1)
    await press(button(host, 'Rename…'))
    await typeInto(
      host.querySelector(
        'form.rename-library input[name="library-name"]',
      ) as HTMLInputElement,
      'Shelves',
    )
    await submit(host.querySelector('form.rename-library') as Element)
    expect(api.renameLibrary).toHaveBeenCalledTimes(1)
    expect(here()).toBe(`${TABLE}/Manga/dirs/11`)

    // Both answers are taken inside one `act`, the rename's first, so the
    // removal's is taken while what is drawn still names Manga.
    await act(async () => {
      name = 'Shelves'
      paths = ['/media/Manga/Volumes']
      renamed.resolve({ id: 1, name: 'Shelves' })
      removed.resolve(removal(4))
      for (let i = 0; i < 20; i++) await Promise.resolve()
    })
    await flush()
    await flush()
    expect(here()).toBe(`${TABLE}/Shelves`)
    expect(host.querySelector('.missing')).toBeNull()
    const report = host.querySelector('.library-page > .removal-report')
    expect(report?.querySelector(':scope > p')?.textContent).toBe(
      CHAPTERS_REMOVED,
    )
  })

  it('keeps a refusal said on a library’s page through a rename of the library', async () => {
    mangaRenamedAndRemoved('Shelves')
    const write = deferred<never>()
    vi.mocked(config.removeRoot).mockReturnValue(write.promise)
    const host = await at(`${TABLE}/Manga/dirs/11`)
    await removeDrawnDir(host)
    await follow(host, '‹ Manga')
    await flush(() =>
      write.reject(
        new Error('/api/library: 500 {"error":"the store is locked"}'),
      ),
    )
    await flush()
    const said = '/media/Manga/Chapters was not removed (the store is locked).'
    const alerts = () =>
      [...host.querySelectorAll('.library-page > [role="alert"]')].map(
        (p) => p.textContent,
      )
    expect(alerts()).toEqual([said])

    await renameTo(host, 'Shelves')
    expect(here()).toBe(`${TABLE}/Shelves`)
    expect(alerts()).toEqual([said])
  })

  it('draws the make page at /config/new-library, and the library named “new” at its own address', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'new', ['/media/New'])],
    })
    const host = await at(NEW_LIBRARY)
    expect(form(host)).not.toBeNull()
    expect(host.querySelector('.library-table')).toBeNull()
    await go(`${TABLE}/new`)
    expect(form(host)).toBeNull()
    expect(libraryTitle(host)).toBe('new')
  })

  it('links New library to /config/new-library, whose back link and Cancel go to the table', async () => {
    const host = await at()
    expect(link(host, 'New library').getAttribute('href')).toBe(NEW_LIBRARY)
    await follow(host, 'New library')
    expect(here()).toBe(NEW_LIBRARY)
    expect(link(host, '‹ Libraries').getAttribute('href')).toBe(TABLE)
    await follow(host, '‹ Libraries')
    expect(here()).toBe(TABLE)
    await follow(host, 'New library')
    await press(button(host, 'Cancel'))
    await flush()
    expect(here()).toBe(TABLE)
  })

  it('opens a library it has just made at the name the server gave it', async () => {
    // The server trims a name, so the name typed and the name made differ.
    const made = library(7, 'Light novels', ['/media/Light novels'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await at(NEW_LIBRARY)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await makeOne(host, ' Light novels ', '/media/Light novels')
    expect(api.createLibrary).toHaveBeenCalledWith(' Light novels ', [
      '/media/Light novels',
    ])
    expect(here()).toBe(`${TABLE}/Light%20novels`)
    expect(libraryTitle(host)).toBe('Light novels')
    expect(document.activeElement).toBe(libraryHeading(host))
  })
})

/**
 * Rules draws the roots, which a directory write can change. A write here
 * marks them stale, so Rules arriving afterwards waits for them rather than
 * drawing roots the app knows are out of date; a visit that wrote nothing
 * leaves Rules to draw at once.
 */
describe('Libraries, a directory write and the roots Rules draws', () => {
  beforeEach(() => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [],
      complaints: [],
    })
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: {},
    })
    vi.mocked(rulesApi.previewRules).mockResolvedValue({
      impact: {
        series_before: 0,
        series_after: 0,
        series_merged: 0,
        works_rehomed: 0,
        works_orphaned: 0,
        proposals_reopened: 0,
        proposals_raised: 0,
      },
      rows: [],
      rows_truncated: false,
      overrides: [],
    })
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    vi.mocked(config.removalCost).mockResolvedValue(removal(0, true))
    vi.mocked(config.removeRoot).mockResolvedValue(removal())
  })

  /** Whether Rules' body is drawn: its rule count is part of it. */
  const rulesDrawn = (host: HTMLElement) =>
    host.querySelector('.rule-editor .rule-count') !== null

  /** Rules drawn, so the store keeps what it read; then `to`. */
  async function fromRules(to: string) {
    const host = await at('/config/rules')
    expect(rulesDrawn(host)).toBe(true)
    await go(to)
    return host
  }

  /**
   * Back to Rules with the roots' read held: whether its body is drawn on
   * arrival, its wait line once the wait has held, the roots' reads sent,
   * and whether the body is drawn once they land.
   */
  async function backToRules(host: HTMLElement) {
    holdTheClock()
    const roots = deferred<{ roots: config.Root[] }>()
    vi.mocked(config.listRoots).mockClear()
    vi.mocked(config.listRoots).mockReturnValue(roots.promise)
    await go('/config/rules')
    const onArrival = rulesDrawn(host)
    pass(WAIT_DRAWN_AFTER_MS)
    const wait =
      host.querySelector('.rule-editor > p[role="status"]')?.textContent ?? null
    const reads = vi.mocked(config.listRoots).mock.calls.length
    await flush(() => roots.resolve({ roots: [] }))
    return { onArrival, wait, reads, landed: rulesDrawn(host) }
  }

  it('draws Rules at once on a return after a visit that wrote nothing, moving between pages', async () => {
    const host = await fromRules('/config/libraries/Manga/dirs/10')
    await follow(host, '‹ Manga')
    await follow(host, '‹ Libraries')
    expect(await backToRules(host)).toEqual({
      onArrival: true,
      wait: null,
      reads: 1,
      landed: true,
    })
  })

  it.each([
    [
      'a directory’s setting is saved as its box is left',
      '/config/libraries/Manga/dirs/10',
      async (host: HTMLElement) => {
        const box = host.querySelector(
          'input[name="default_language"]',
        ) as HTMLInputElement
        await typeInto(box, 'ja')
        // React's `onBlur` listens for `focusout`.
        await flush(() =>
          box.dispatchEvent(new FocusEvent('focusout', { bubbles: true })),
        )
        return vi.mocked(config.patchRoot)
      },
    ],
    [
      'a directory’s setting typed is sent as its page is left',
      '/config/libraries/Manga/dirs/10',
      async (host: HTMLElement) => {
        await typeInto(
          host.querySelector(
            'input[name="default_language"]',
          ) as HTMLInputElement,
          'ja',
        )
        await follow(host, '‹ Manga')
        return vi.mocked(config.patchRoot)
      },
    ],
    [
      'a directory’s documents are kept',
      '/config/libraries/Manga/dirs/10',
      async (host: HTMLElement) => {
        await press(
          host.querySelector('input[name="keeps_documents"]') as Element,
        )
        await flush()
        return vi.mocked(config.patchRoot)
      },
    ],
    [
      'a directory is added',
      '/config/libraries/Manga',
      async (host: HTMLElement) => {
        vi.mocked(api.addDir).mockResolvedValue({
          root: dir(9, '/media/Manga/Extras'),
          watching: true,
          watch_reason: null,
          interval_secs: 900,
        })
        await press(host.querySelector('button[name="add-dir"]') as Element)
        const picker = section(host, 'Directories').querySelector(
          ':scope > .settings-box > .add-root',
        ) as HTMLElement
        await typeInto(
          picker.querySelector('input[name="path"]') as HTMLInputElement,
          '/media/Manga/Extras',
        )
        await press(picker.querySelector('button[name="pick"]') as Element)
        await flush()
        return vi.mocked(api.addDir)
      },
    ],
    [
      'a directory is removed',
      '/config/libraries/Manga/dirs/11',
      async (host: HTMLElement) => {
        await removeDrawnDir(host)
        return vi.mocked(config.removeRoot)
      },
    ],
    [
      'a library is made',
      NEW_LIBRARY,
      async (host: HTMLElement) => {
        const made = library(3, 'Extras', ['/media/Extras'])
        vi.mocked(api.createLibrary).mockResolvedValue({
          library: made,
          added: [
            {
              root: made.dirs[0],
              watching: true,
              watch_reason: null,
              interval_secs: 900,
            },
          ],
        })
        await makeOne(host, 'Extras', '/media/Extras')
        return vi.mocked(api.createLibrary)
      },
    ],
    [
      'a directory is moved',
      '/config/libraries/Manga/dirs/11',
      async (host: HTMLElement) => {
        vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
        vi.mocked(api.moveDir).mockResolvedValue(movedAnswer())
        await moveDrawnDir(host, '2')
        return vi.mocked(api.moveDir)
      },
    ],
    [
      'a library is removed',
      '/config/libraries/Comics',
      async (host: HTMLElement) => {
        await removeDrawnLibrary(host)
        return vi.mocked(api.removeLibrary)
      },
    ],
  ] as const)(
    'makes Rules wait for the roots on a return after %s',
    async (_, to, write) => {
      const host = await fromRules(to)
      const sent = await write(host)
      expect(sent).toHaveBeenCalledTimes(1)
      expect(await backToRules(host)).toEqual({
        onArrival: false,
        wait: 'Loading…',
        reads: 1,
        landed: true,
      })
    },
  )
})

/**
 * A removal deletes more than its directories: the schema's cascades take
 * the rules scoped to them, the accepted proposals' rules with those, and a
 * library's grants. A page drawing any of them when the answer lands reads
 * it again at once; a move marks only the roots, and the grants where it
 * emptied its library.
 */
describe('Libraries, what a removal or a move makes out of date', () => {
  beforeEach(() => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [],
      complaints: [],
    })
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: {},
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
    vi.mocked(config.removalCost).mockResolvedValue(removal(0, true))
    vi.mocked(config.removeRoot).mockResolvedValue(removal())
  })

  /** A page drawing every read a removal or a move can make out of date. */
  function Drawing() {
    usePage([ROOTS, RULES, ACCEPTED_PROPOSALS, USER_LIST])
    return null
  }

  /** How many times each was read since the counts were cleared. */
  const readAgain = () => ({
    roots: vi.mocked(config.listRoots).mock.calls.length,
    rules: vi.mocked(rulesApi.listRules).mock.calls.length,
    proposals: vi.mocked(curation.listProposals).mock.calls.length,
    users: vi.mocked(usersApi.listUsers).mock.calls.length,
  })

  it.each([
    [
      'a directory is removed',
      '/config/libraries/Manga/dirs/11',
      async (host: HTMLElement) => {
        await removeDrawnDir(host)
        return vi.mocked(config.removeRoot)
      },
      { roots: 1, rules: 1, proposals: 1, users: 0 },
    ],
    [
      'a library’s last directory is removed, which removes the library',
      '/config/libraries/Comics/dirs/20',
      async (host: HTMLElement) => {
        await removeDrawnDir(host)
        return vi.mocked(config.removeRoot)
      },
      { roots: 1, rules: 1, proposals: 1, users: 1 },
    ],
    [
      'a library is removed',
      '/config/libraries/Comics',
      async (host: HTMLElement) => {
        await removeDrawnLibrary(host)
        return vi.mocked(api.removeLibrary)
      },
      { roots: 1, rules: 1, proposals: 1, users: 1 },
    ],
    [
      'a directory is moved',
      '/config/libraries/Manga/dirs/11',
      async (host: HTMLElement) => {
        vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
        vi.mocked(api.moveDir).mockResolvedValue(movedAnswer())
        await moveDrawnDir(host, '2')
        return vi.mocked(api.moveDir)
      },
      { roots: 1, rules: 0, proposals: 0, users: 0 },
    ],
    [
      'a directory is moved out of a library it empties, which removes the library',
      '/config/libraries/Comics/dirs/20',
      async (host: HTMLElement) => {
        const emptied = { library_id: 1, library: 'Manga' }
        vi.mocked(api.moveCost).mockResolvedValue(
          movedAnswer({ ...emptied, library_removed: 'Comics' }, true),
        )
        vi.mocked(api.moveDir).mockResolvedValue(
          movedAnswer({ ...emptied, library_removed: 'Comics' }),
        )
        await moveDrawnDir(host, '1')
        return vi.mocked(api.moveDir)
      },
      { roots: 1, rules: 0, proposals: 0, users: 1 },
    ],
  ] as const)(
    'has a page drawing what it changed read it again, once %s',
    async (_, to, write, reads) => {
      mount(<Drawing />)
      const host = await at(to)
      vi.mocked(config.listRoots).mockClear()
      vi.mocked(rulesApi.listRules).mockClear()
      vi.mocked(curation.listProposals).mockClear()
      vi.mocked(usersApi.listUsers).mockClear()
      const sent = await write(host)
      expect(sent).toHaveBeenCalledTimes(1)
      await flush()
      expect(readAgain()).toEqual(reads)
    },
  )
})

/**
 * A removal takes works out of the Library views, and a move takes works
 * from one library's views to another's. Each marks them stale once it is
 * answered, so a Library view arriving afterwards waits for its own read; a
 * refused write and a change of page change nothing, and mark nothing.
 */
describe('Libraries, a write and the Library views', () => {
  beforeEach(() => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  })

  it.each([
    [
      'a library is removed',
      '/config/libraries/Comics',
      async (host: HTMLElement) => {
        await removeDrawnLibrary(host)
        return vi.mocked(api.removeLibrary)
      },
    ],
    [
      'a directory is removed',
      '/config/libraries/Manga/dirs/11',
      async (host: HTMLElement) => {
        await removeDrawnDir(host)
        return vi.mocked(config.removeRoot)
      },
    ],
    [
      'a library’s last directory is removed, which removes the library',
      '/config/libraries/Comics/dirs/20',
      async (host: HTMLElement) => {
        await removeDrawnDir(host)
        return vi.mocked(config.removeRoot)
      },
    ],
    [
      'a directory is moved',
      '/config/libraries/Manga/dirs/11',
      async (host: HTMLElement) => {
        vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
        vi.mocked(api.moveDir).mockResolvedValue(movedAnswer())
        await moveDrawnDir(host, '2')
        return vi.mocked(api.moveDir)
      },
    ],
    [
      'a directory is moved out of a library it empties, which removes the library',
      '/config/libraries/Comics/dirs/20',
      async (host: HTMLElement) => {
        const emptied = {
          library_id: 1,
          library: 'Manga',
          library_removed: 'Comics',
        }
        vi.mocked(api.moveCost).mockResolvedValue(movedAnswer(emptied, true))
        vi.mocked(api.moveDir).mockResolvedValue(movedAnswer(emptied))
        await moveDrawnDir(host, '1')
        return vi.mocked(api.moveDir)
      },
    ],
  ] as const)('marks them stale once %s', async (_, to, write) => {
    const grid = await keptGrid()
    const host = await at(to)
    expect(grid.stale()).toBe(false)
    const sent = await write(host)
    expect(sent).toHaveBeenCalledTimes(1)
    expect(grid.stale()).toBe(true)
    expect(grid.sent()).toBe(1)
  })

  it('marks none of them stale when a move is refused', async () => {
    const grid = await keptGrid()
    const host = await at('/config/libraries/Manga/dirs/11')
    vi.mocked(api.moveCost).mockResolvedValue(movedAnswer({}, true))
    vi.mocked(api.moveDir).mockRejectedValue(
      new Error(
        '/api/library/roots/11/move: 409 {"error":"`Comics` already holds volumes of /media/Manga/Chapters"}',
      ),
    )
    await moveDrawnDir(host, '2')
    expect(api.moveDir).toHaveBeenCalledTimes(1)
    expect(
      host.querySelector('.move-root [role="alert"]')?.textContent,
    ).toContain('already holds')
    expect(grid.stale()).toBe(false)
  })

  it('marks none of them stale for a visit that wrote nothing, moving between pages', async () => {
    const grid = await keptGrid()
    const host = await at('/config/libraries/Manga/dirs/10')
    await follow(host, '‹ Manga')
    await follow(host, '‹ Libraries')
    await follow(host, 'Comics')
    expect(here()).toBe('/config/libraries/Comics')
    expect(libraryTitle(host)).toBe('Comics')
    expect(grid.stale()).toBe(false)
  })
})

/**
 * Each account's grants name the libraries it is granted, so a rename
 * changes the users list Users draws. The rename marks that list stale, so
 * Users arriving afterwards waits for it rather than drawing the old name.
 */
describe('Libraries, a rename and the users list Users draws', () => {
  const ADMIN: Me = {
    user_id: 1,
    admin: true,
    can_download: true,
    can_stream: true,
    can_stamp: false,
  }

  /** An account limited to the library whose name the list says now. */
  const grantedTo = (name: string): usersApi.User => ({
    id: 2,
    name: 'sam',
    role: 'user',
    can_download: false,
    can_stream: false,
    can_stamp: false,
    can_log_in: true,
    sessions: [],
    keys: [],
    every_library: false,
    libraries: [{ id: 1, name }],
  })

  beforeEach(() => {
    let renamed = false
    vi.mocked(api.listLibraries).mockImplementation(async () => ({
      libraries: [library(1, renamed ? 'Manga 2' : 'Manga', ['/media/Manga'])],
    }))
    vi.mocked(api.renameLibrary).mockImplementation(async () => {
      renamed = true
      return { id: 1, name: 'Manga 2' }
    })
    vi.mocked(usersApi.listUsers).mockImplementation(async () => ({
      users: [grantedTo(renamed ? 'Manga 2' : 'Manga')],
    }))
  })

  /** Users drawn in a root of its own, as Configuration's other tab. */
  async function usersDrawn() {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/config/users']}>
          <GateContext value={{ who: ADMIN, signOut: () => undefined }}>
            <Routes>
              <Route path="/config/users/*" element={<Users />} />
            </Routes>
          </GateContext>
        </MemoryRouter>,
      )
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    return host
  }

  /** What the table's Libraries column says, row by row. */
  const sees = (host: HTMLElement) =>
    [...host.querySelectorAll('.user-table tbody td.wraps')].map(cellValue)

  /** Users drawn, so the store keeps what it read; then left. */
  async function usersLeft() {
    const host = await usersDrawn()
    await flush()
    expect(sees(host)).toEqual(['Manga'])
    rerender(<div />)
  }

  /**
   * Back to Users with the users list's read held: what its Libraries
   * column says on arrival, its wait line once the wait has held, the reads
   * of the list sent, and what the column says once the read lands.
   */
  async function backToUsers() {
    holdTheClock()
    const read = deferred<{ users: usersApi.User[] }>()
    const answer = await usersApi.listUsers()
    vi.mocked(usersApi.listUsers).mockClear()
    vi.mocked(usersApi.listUsers).mockReturnValueOnce(read.promise)
    const host = await usersDrawn()
    const onArrival = sees(host)
    pass(WAIT_DRAWN_AFTER_MS)
    const wait =
      host.querySelector('.user-table > p[role="status"]')?.textContent ?? null
    const reads = vi.mocked(usersApi.listUsers).mock.calls.length
    await flush(() => read.resolve(answer))
    return { onArrival, wait, reads, landed: sees(host) }
  }

  it('makes Users wait for the users list on a return after a library is renamed', async () => {
    await usersLeft()
    const host = await at(`${TABLE}/Manga`)
    await press(button(host, 'Rename…'))
    await typeInto(
      host.querySelector(
        'form.rename-library input[name="library-name"]',
      ) as HTMLInputElement,
      'Manga 2',
    )
    await submit(host.querySelector('form.rename-library') as Element)
    await flush()
    expect(api.renameLibrary).toHaveBeenCalledTimes(1)
    expect(await backToUsers()).toEqual({
      onArrival: [],
      wait: 'Loading…',
      reads: 1,
      landed: ['Manga 2'],
    })
  })

  it('draws Users at once on a return after a visit to a library’s page that renamed nothing', async () => {
    await usersLeft()
    await at(`${TABLE}/Manga`)
    expect(api.renameLibrary).not.toHaveBeenCalled()
    expect(await backToUsers()).toEqual({
      onArrival: ['Manga'],
      wait: null,
      reads: 1,
      landed: ['Manga'],
    })
  })
})

/**
 * A Rules save derives the library again, which can fold works or make
 * them, so it changes the works Libraries counts for each library. The save
 * marks the list stale, so Libraries arriving afterwards waits for it.
 */
describe('Rules, a save and the libraries list Libraries draws', () => {
  const IMPACT = {
    series_before: 0,
    series_after: 0,
    series_merged: 0,
    works_rehomed: 0,
    works_orphaned: 0,
    proposals_reopened: 0,
    proposals_raised: 0,
  }

  beforeEach(() => {
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: twoAndOne() })
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [],
      complaints: [],
    })
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: {},
    })
    vi.mocked(rulesApi.previewRules).mockResolvedValue({
      impact: IMPACT,
      rows: [],
      rows_truncated: false,
      overrides: [],
    })
    vi.mocked(rulesApi.saveRules).mockResolvedValue({
      impact: IMPACT,
      rules: [],
      reopened_proposals: [],
      complaints: [],
      derive: emptyDerive,
    })
  })

  /** What the table's Works column says, row by row. */
  const works = (host: HTMLElement) =>
    [...host.querySelectorAll('.library-table tbody tr')].map((tr) =>
      cellValue(tr.querySelectorAll('td')[2]),
    )

  /**
   * The table drawn, so the store keeps the list; then Rules, where `write`
   * is done; then the table again with the list's read held. What the
   * table draws on arrival, its wait line once the wait has held, the reads
   * of the list sent, and what it draws once the read lands.
   */
  async function tableAfter(write: (host: HTMLElement) => Promise<void>) {
    const host = await at(TABLE)
    expect(names(host)).toEqual(['Manga', 'Comics'])
    await go('/config/rules')
    expect(host.querySelector('.rule-editor .rule-count')).not.toBeNull()
    await write(host)
    holdTheClock()
    const read = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockClear()
    vi.mocked(api.listLibraries).mockReturnValueOnce(read.promise)
    await go(TABLE)
    const onArrival = names(host)
    pass(WAIT_DRAWN_AFTER_MS)
    const wait =
      host.querySelector('.library-table > p[role="status"]')?.textContent ??
      null
    const reads = vi.mocked(api.listLibraries).mock.calls.length
    const [manga, comics] = twoAndOne()
    await flush(() =>
      read.resolve({ libraries: [{ ...manga, works: 5 }, comics] }),
    )
    return { onArrival, wait, reads, landed: works(host) }
  }

  it('makes Libraries wait for the list on a return after the rules are saved', async () => {
    expect(
      await tableAfter(async (host) => {
        await press(host.querySelector('button.save') as Element)
        await flush()
        expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
      }),
    ).toEqual({
      onArrival: [],
      wait: 'Loading…',
      reads: 1,
      landed: ['5 works', '0 works'],
    })
  })

  it('draws Libraries at once on a return after a visit to Rules that saved nothing', async () => {
    expect(
      await tableAfter(async () => {
        expect(rulesApi.saveRules).not.toHaveBeenCalled()
      }),
    ).toEqual({
      onArrival: ['Manga', 'Comics'],
      wait: null,
      reads: 1,
      landed: ['5 works', '0 works'],
    })
  })
})

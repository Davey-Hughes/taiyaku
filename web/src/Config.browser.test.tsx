import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `PreviewTabs.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app through
// `index.css`. Without this the boxes below are unstyled and the assertions
// measure browser defaults.
import './index.css'
import { Config } from './Config'
import * as config from './config'
import type { Derived, FinishedScan, Unreadable, Walk } from './config'
import { GateContext } from './gate'
import { Backup } from './Backup'
import * as librariesApi from './libraries'
import * as backupApi from './backup'
import { FolderPicker } from './FolderPicker'
import { Users } from './Users'
import * as usersApi from './users'
import { DeriveSection, ScanReport } from './ScanReport'
import { deferred } from './testing/async'
import { press, submit, typeInto } from './testing/input'
import { mount } from './testing/mount'

/**
 * The browser pass over all four Configuration screens — Task 7 of style
 * pass ⑤b. Nothing Tasks 1–6 did is measurable in jsdom: a measured width, a
 * computed colour and a grid's row count all need a real layout engine — the
 * gap ⑤a's own scope-`<select>` defect shipped through (`admin.css`'s
 * `.root-row .field select` comment records it: found by the browser pass,
 * against a 35-character real root, on a screen `RuleForm.browser.test.tsx`'s
 * short fixture root had already passed). This file turns the one-off
 * Chromium checks each of Tasks 1–6 did by hand into standing assertions.
 *
 * **These three screens take no props and fetch their own data**, so this
 * file stubs `./config`, `./libraries`, `./users` and `./backup` exactly as
 * `Libraries.test.tsx`, `Users.test.tsx` and `Backup.test.tsx` do — copying each mock surface from
 * its matching jsdom test rather than guessing which functions a mount
 * actually calls. `mount` keeps ONE module-level host, torn down in
 * `afterEach` — one mount per `it()`, never two, or the second leaks into
 * `document.body` underneath the first.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addFolder: vi.fn(),
  createLibrary: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listFolders: vi.fn(),
  listLibraries: vi.fn(),
  moveCost: vi.fn(),
  removeLibrary: vi.fn(),
}))

vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  addUser: vi.fn(),
  listUsers: vi.fn(),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

vi.mock('./backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./backup')>()),
  listBackups: vi.fn(),
  takeBackup: vi.fn(),
}))

// --- fixtures, copied from each screen's own jsdom test ---------------------

const root = (over: Partial<config.Root> = {}): config.Root => ({
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

/**
 * A real path from this library, not a short fixture one. ⑤a's C1 hid behind
 * a ten-character `/srv/manga`: the scope `<select>`'s intrinsic width only
 * spilled past its label — and out through four `overflow-x: visible`
 * ancestors onto the document — against a 35-character real root, and the
 * fixture-rooted test that should have caught it passed. `RuleForm.
 * browser.test.tsx`'s own comment on `admin.css`'s scope-select rule records
 * exactly this.
 */
const longPathRoot = root({ path: '/home/davey/temp/manga-test/Volumes' })

/** A library of one folder, as the grant editor lists it. */
const libraryOf = (folder: config.Root): librariesApi.Library => ({
  id: 70,
  name: 'Manga',
  works: 0,
  books: 0,
  folders: [folder],
})

/** One library holding `folders`, as the list route sends it. */
const holding = (...folders: config.Root[]) => ({
  libraries: [{ id: 1, name: 'Manga', works: 0, books: 0, folders }],
})

/** A library name with sixty characters and nowhere to break. */
const UNBROKEN_NAME = 'Omnibus_Editions'.repeat(4).slice(0, 60)

/** A root whose path has a longer run between hyphens and spaces than a page at 390 is wide. */
const UNBROKEN_ROOT = root({
  path: '/home/davey/projects/taiyaku/target/a_scratch_library/Volumes',
})

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

const backupEntry = (
  over: Partial<backupApi.Backup> = {},
): backupApi.Backup => ({
  name: 'library-20260906T143502Z.db',
  bytes: 1024 * 1024 * 12,
  taken: '20260906T143502Z',
  ...over,
})

// --- the scan report's own fixture, copied from `ScanReport.test.tsx` -------

const walk = (over: Partial<Walk> = {}): Walk => ({
  root: '/srv/manga',
  dirs: 156,
  files: 1858,
  added: 3,
  updated: 0,
  gone: 0,
  refused: null,
  unreadable_dirs: [],
  excluded_dirs: [],
  excluded_files: 0,
  bad_exclude_patterns: [],
  forgotten_by_exclusion: 0,
  ...over,
})

const derived = (over: Partial<Derived> = {}): Derived => ({
  series: 130,
  works: 1858,
  editions: 1883,
  members: 1883,
  unattached_chapter_dirs: 0,
  unreadable: {
    total: 0,
    refused_format: 0,
    by_format: {},
    uncounted: [],
    vanished: [],
    no_pages: 0,
  },
  suppressed: 0,
  rekeyed: 0,
  kept_editions: 0,
  emptied_works: 0,
  unfolded_works: 0,
  unfoldable: [],
  unreadable_hand: [],
  repaired_documents: 0,
  unwritable_documents: [],
  orphan_documents: [],
  declined_documents: [],
  stale_documents: [],
  moved_positions: 0,
  lost_positions: 0,
  moved_notes: 0,
  dropped_notes: 0,
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
  uncarried_alignments: 0,
  series_holding_no_work: 0,
  merged: 0,
  unrecognised_kinds: 0,
  raised: 0,
  root_removed_mid_derive: 0,
  ...over,
})

const finished = (over: Partial<FinishedScan> = {}): FinishedScan => ({
  scan: walk(),
  derive: derived(),
  ...over,
})

const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

// --- the Libraries screen's four pages ---------------------------------------

/** The table of libraries. */
const TABLE = '/config/libraries'
/** The page that makes a library. */
const NEW_LIBRARY = `${TABLE}/new`
/** The page of `holding(…)`'s library. */
const LIBRARY = `${TABLE}/1`
/** The page of `root()`'s folder, in `holding(…)`'s library. */
const FOLDER = `${LIBRARY}/folders/3`

/**
 * Configuration at `entry`, its tabs above the page: each page is a route of
 * its own, and `.config`'s rules draw the back link and New library.
 */
function routed(entry: string) {
  return (
    <MemoryRouter initialEntries={[entry]}>
      <GateContext value={{ who, signOut: () => undefined }}>
        <Routes>
          <Route path="/config/*" element={<Config />} />
        </Routes>
      </GateContext>
    </MemoryRouter>
  )
}

/** Let what a page reads on mount, and what that read draws, land. */
async function settled(ui: React.ReactNode) {
  return await act(async () => {
    const mounted = mount(ui)
    await new Promise((resolve) => setTimeout(resolve, 20))
    return mounted
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  // Every `RootRow` asks the status route once on mount (`RootRow.tsx`'s own
  // doc comment) — without an answer the mock resolves `undefined` and the
  // `.then` inside that effect throws before anything renders.
  vi.mocked(config.scanStatus).mockResolvedValue({
    root_id: 3,
    running: false,
    report: null,
    last_scan_at: null,
    last_scan_status: null,
  })
  vi.mocked(librariesApi.libraryScanStatus).mockResolvedValue({
    library_id: 1,
    running: false,
    elapsed_secs: null,
    folders: [],
  })
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

/**
 * `<Libraries>`/`<Users>`/`<Backup>` mount standalone in production too — they
 * are routed inside `Shell`'s `<main>`, which is where `.library > main`'s
 * own CSS (`shell.css`) applies. Reproducing that ancestor here, the same
 * way `AcceptForm.browser.test.tsx` and `Proposals.browser.test.tsx` do
 * ("inside the sidebar-plus-main shell it lives in"), is what lets `main`
 * be checked at all: a bare, unstyled `<main>` carries none of the rules
 * that make the real one's overflow behave as it does, and the empty
 * result would be checking nothing.
 */
function inShell(children: React.ReactNode) {
  return (
    <div className="library" style={{ height: '844px' }}>
      <div className="sidebar-column" />
      <main>{children}</main>
    </div>
  )
}

/** Both, not just the document — see `inShell`'s own comment. */
function assertNoSidewaysScroll(host: HTMLElement) {
  const doc = document.documentElement
  const main = host.querySelector('main')
  expect(main).not.toBeNull()
  expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
  expect(main!.scrollWidth).toBeLessThanOrEqual(main!.clientWidth)
}

/** Configuration at `entry`, in the shell, once what it reads has landed. */
const opened = (entry: string) => settled(inShell(routed(entry)))

describe('the Libraries screen, with real keys and a real pointer', () => {
  it('makes a library when Enter is pressed in the make page’s path box straight after typing', async () => {
    await page.viewport(1280, 844)
    vi.mocked(librariesApi.createLibrary).mockResolvedValueOnce({
      library: holding(UNBROKEN_ROOT).libraries[0],
      added: [],
    })
    const host = await opened(NEW_LIBRARY)
    const name = host.querySelector(
      'input[name="library-name"]',
    ) as HTMLInputElement
    const box = host.querySelector('input[name="path"]') as HTMLInputElement
    // Focus inside the page first: a real key pressed before anything in
    // the test's frame has focus goes to the frame.
    await act(async () => name.focus())
    await userEvent.keyboard('Manga')
    await act(async () => box.focus())
    await userEvent.keyboard('/srv/manga{Enter}')
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50))
    })
    expect(librariesApi.createLibrary).toHaveBeenCalledTimes(1)
    expect(librariesApi.createLibrary).toHaveBeenCalledWith('Manga', [
      '/srv/manga',
    ])
  })

  it.each([390, 1280])(
    'lands a press anywhere on a row of the table on that row’s link, at %ipx',
    async (width) => {
      await page.viewport(width, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          {
            id: 1,
            name: 'Manga',
            works: 340,
            books: 0,
            folders: [root(), root({ id: 4, path: '/srv/chapters' })],
          },
          {
            id: 2,
            name: 'Comics',
            works: 1,
            books: 0,
            folders: [root({ id: 5, path: '/srv/comics' })],
          },
        ],
      })
      const host = await opened(TABLE)
      await document.fonts.ready
      const rows = [...host.querySelectorAll('.library-table tbody tr')]
      expect(rows).toHaveLength(2)
      rows.forEach((row, i) => {
        const [, ...values] = [...row.querySelectorAll('td')]
        expect(values).toHaveLength(3)
        for (const cell of values) {
          const at = cell.getBoundingClientRect()
          const hit = document.elementFromPoint(
            at.left + at.width / 2,
            at.top + at.height / 2,
          )
          expect(
            hit?.closest('a')?.getAttribute('href'),
            cell.textContent,
          ).toBe(`/config/libraries/${i + 1}`)
        }
      })
    },
  )
})

describe('the Libraries pages at a phone’s width', () => {
  /**
   * One library whose name and folder have nowhere to break, its folder's
   * last scan failed: the longest words each of the four pages draws.
   */
  const UNBROKEN_LIBRARY = {
    libraries: [
      {
        id: 1,
        name: UNBROKEN_NAME,
        works: 12,
        books: 0,
        folders: [
          {
            ...UNBROKEN_ROOT,
            last_scan_at: '2026-10-01 10:00:00',
            last_scan_status: 'failed',
          },
        ],
      },
    ],
  }

  it.each([
    [TABLE, '.library-table .config-table'],
    [NEW_LIBRARY, '.new-library form.add-library'],
    [LIBRARY, '.library-page .folder-table'],
    [FOLDER, '.folder-page .root-row'],
  ])(
    'does not scroll sideways on %s, whose name and path have nowhere to break',
    async (entry, drawn) => {
      await page.viewport(390, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(UNBROKEN_LIBRARY)
      const host = await opened(entry)
      await document.fonts.ready
      // The page asked for, and not the table an unknown address goes to.
      expect(host.querySelector(drawn), drawn).not.toBeNull()
      assertNoSidewaysScroll(host)
    },
  )

  /** Two libraries: one whose folders hold a failed scan and none, one never scanned. */
  const SCANNED = {
    libraries: [
      {
        id: 1,
        name: 'Manga',
        works: 340,
        books: 0,
        folders: [
          root(),
          root({
            id: 4,
            path: '/srv/chapters',
            scan_interval_secs: null,
            last_scan_at: '2026-10-01 10:00:00',
            last_scan_status: 'failed',
          }),
        ],
      },
      {
        id: 2,
        name: 'Comics',
        works: 1,
        books: 0,
        folders: [root({ id: 5, path: '/srv/comics' })],
      },
    ],
  }

  it.each([
    [
      TABLE,
      '.library-table .config-table',
      [
        [
          'Manga',
          '2 folders',
          '340 works',
          'Last scan: 2026-10-01 10:00:00 · failed',
        ],
        ['Comics', '1 folder', '1 work', 'Last scan: Never'],
      ],
      'Comics',
    ],
    [
      LIBRARY,
      '.library-page .folder-table',
      [
        ['/srv/manga', 'Scan every: 24h', 'Last scan: Never'],
        [
          '/srv/chapters',
          'Scan every: not set',
          'Last scan: 2026-10-01 10:00:00 · failed',
        ],
      ],
      '/srv/manga',
    ],
  ])(
    'stacks each row of the table on %s into a block of two lines, each value after its label',
    async (entry, selector, rows, short) => {
      await page.viewport(390, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(SCANNED)
      const host = await opened(entry)
      await document.fonts.ready
      const table = host.querySelector(selector) as HTMLElement
      // What is drawn: `innerText` leaves out what the stylesheet hides.
      expect(
        [...table.querySelectorAll('tbody tr')].map((tr) =>
          [...tr.querySelectorAll('td')].map((td) => td.innerText),
        ),
      ).toEqual(rows)
      // The headings are not drawn at this width, so a label says the
      // heading's own words.
      expect(
        (table.querySelector('thead') as HTMLElement).getBoundingClientRect()
          .width,
      ).toBeLessThanOrEqual(1)
      const headings = [...table.querySelectorAll('thead th')].map(
        (th) => th.textContent,
      )
      for (const tr of table.querySelectorAll('tbody tr')) {
        const [lead, ...values] = [...tr.querySelectorAll('td')]
        values.forEach((td, i) => {
          const label = td.querySelector('.cell-label')
          if (label !== null)
            expect(label.textContent).toBe(`${headings[i + 1]}: `)
        })
        // A block: the name on its own line, and its values beneath it.
        expect(getComputedStyle(tr).display).not.toBe('table-row')
        const foot = lead.getBoundingClientRect().bottom
        for (const td of values) {
          expect(td.getBoundingClientRect().top).toBeGreaterThanOrEqual(
            foot - 0.5,
          )
        }
        // Two lines: values short enough to share the line beneath the
        // name do share it. A long one may wrap; this row's are short.
        if (lead.innerText === short) {
          const top = values[0].getBoundingClientRect().top
          for (const td of values) {
            expect(td.getBoundingClientRect().top, td.innerText).toBeCloseTo(
              top,
              0,
            )
          }
        }
      }
      expect(
        [...table.querySelectorAll<HTMLElement>('td.lead')].map(
          (td) => td.innerText,
        ),
      ).toContain(short)
    },
  )

  it.each([
    [390, 'shut'],
    [390, 'open'],
    [1280, 'shut'],
    [1280, 'open'],
  ])(
    'puts the add-folder picker’s Cancel on the line of its buttons, at %ipx with the browser %s',
    async (width, browser) => {
      await page.viewport(width, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(
        holding(longPathRoot),
      )
      vi.mocked(librariesApi.listFolders).mockResolvedValue({
        path: '/srv',
        parent: '/',
        folders: [{ name: 'manga', path: '/srv/manga' }],
      })
      const host = await opened(LIBRARY)
      await press(host.querySelector('button[name="add-folder"]') as Element)
      const add = host.querySelector('.library-page > .add-root') as HTMLElement
      if (browser === 'open')
        await press(add.querySelector('button[name="browse"]') as Element)
      await document.fonts.ready
      const [pick, browse, cancel] = ['pick', 'browse', 'cancel-add'].map(
        (name) =>
          (
            add.querySelector(`button[name="${name}"]`) as HTMLElement
          ).getBoundingClientRect(),
      )
      expect(browse.top).toBeCloseTo(pick.top, 0)
      expect(cancel.top).toBeCloseTo(browse.top, 0)
      expect(cancel.left).toBeGreaterThan(browse.right)
      // The browser, when open, is beneath the whole line of buttons.
      const shown = add.querySelector('.folder-browser')
      expect(shown !== null).toBe(browser === 'open')
      if (shown !== null) {
        expect(shown.getBoundingClientRect().top).toBeGreaterThanOrEqual(
          cancel.bottom,
        )
      }
      assertNoSidewaysScroll(host)
    },
  )

  it('draws a refused Add folder on a line of its own beneath the buttons', async () => {
    // A refusal short enough to sit beside Cancel, were it let: at 390 the
    // buttons' line has room for it.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    vi.mocked(librariesApi.addFolder).mockRejectedValue(
      new Error('/api/library/libraries/1/folders: 400 {"error":"refused"}'),
    )
    const host = await opened(LIBRARY)
    await press(host.querySelector('button[name="add-folder"]') as Element)
    const add = host.querySelector('.library-page > .add-root') as HTMLElement
    await typeInto(
      add.querySelector('input[name="path"]') as HTMLInputElement,
      '/srv/extra',
    )
    await press(add.querySelector('button[name="pick"]') as Element)
    await document.fonts.ready
    const alert = add.querySelector('[role="alert"]') as HTMLElement
    expect(alert.textContent).toBe('refused')
    const lowest = Math.max(
      ...[...add.querySelectorAll('button')].map(
        (b) => b.getBoundingClientRect().bottom,
      ),
    )
    expect(alert.getBoundingClientRect().top).toBeGreaterThanOrEqual(lowest)
    expect(alert.getBoundingClientRect().left).toBeCloseTo(
      add.getBoundingClientRect().left,
      0,
    )
  })
})

describe('the Configuration screens at 390px', () => {
  it('does not scroll sideways on a library’s page, and wraps the rename form inside the page', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    const host = await opened(LIBRARY)
    assertNoSidewaysScroll(host)
    // The library's three actions share a row that must wrap: at 320px they
    // ask for more than is left them in every face. The fallback faces are
    // forced so that the result does not depend on whether the web faces have
    // loaded.
    withFallbackFonts()
    await page.viewport(320, 844)
    await press(host.querySelector('button[name="rename-library"]') as Element)
    assertNoSidewaysScroll(host)
    for (const selector of ['.library-page > .actions', '.rename-library']) {
      const row = host.querySelector(selector) as HTMLElement
      const edge = row.getBoundingClientRect().right + 0.5
      expect(
        [...row.children]
          .filter((el) => el.getBoundingClientRect().right > edge)
          .map((el) => `${el.tagName.toLowerCase()}.${el.className}`),
        selector,
      ).toEqual([])
      // Each keeps its words on one line: a row that does not wrap shrinks
      // its buttons until their labels break.
      for (const button of row.querySelectorAll('button')) {
        expect(button.getBoundingClientRect().height, selector).toBeLessThan(44)
      }
    }
  })

  it('keeps a folder’s own path inside its page when the path has nowhere to break', async () => {
    // `longPathRoot` above breaks at its hyphen and fits. A path with a
    // longer run than the page is wide takes the page's heading past its
    // edge, and the document with it, unless the heading breaks it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    const host = await opened(FOLDER)
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
    // The path is the page's heading and takes no margin above it.
    const heading = host.querySelector('.root-row > h2') as HTMLElement
    expect(heading.textContent).toBe(UNBROKEN_ROOT.path)
    expect(getComputedStyle(heading).marginTop).toBe('0px')
  })

  it('keeps the removal’s confirm button inside the page for the same path', async () => {
    // Another place the folder's page draws its path: `RemoveRoot`'s confirm
    // button says "Remove " and then the whole of it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived(),
      dry_run: true,
    })
    const host = await opened(FOLDER)
    await act(async () => {
      const open = [...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Remove…',
      )
      open?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await document.fonts.ready
    const confirm = host.querySelector('.remove-root .confirm')
    expect(confirm?.textContent).toBe(`Remove ${UNBROKEN_ROOT.path}`)
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  /**
   * The move dialog on a long path and a long library name (the same O180
   * defect): a folder's page in one library, a second library whose name has
   * nowhere to break, Move… pressed, the second chosen and the rehearsal
   * asked for.
   */
  async function rehearsedMove(
    asked: Promise<librariesApi.MoveAnswer> | Error,
  ) {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        {
          id: 1,
          name: 'Manga',
          works: 0,
          books: 0,
          folders: [UNBROKEN_ROOT],
        },
        {
          id: 2,
          name: UNBROKEN_NAME,
          works: 0,
          books: 0,
          folders: [root({ id: 4, path: '/srv/other' })],
        },
      ],
    })
    if (asked instanceof Error) {
      vi.mocked(librariesApi.moveCost).mockRejectedValue(asked)
    } else {
      vi.mocked(librariesApi.moveCost).mockResolvedValue(await asked)
    }
    const host = await opened(FOLDER)
    const folder = host.querySelector('.folder-page') as HTMLElement
    await act(async () => {
      const open = [...folder.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      )
      open?.click()
      await Promise.resolve()
    })
    await act(async () => {
      const box = folder.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '2')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(folder.querySelector('button[name="rehearse"]') as Element)
    await document.fonts.ready
    return { host, folder }
  }

  it('keeps a move’s refusal and the list of volumes it names inside the page', async () => {
    const volumes = Array.from({ length: 20 }, (_, i) =>
      `${String(i).padStart(2, '0')}_`.padEnd(80, 'v'),
    )
    const { host, folder } = await rehearsedMove(
      new Error(
        `/api/library/roots/3/move?dry_run=true: 409 ${JSON.stringify({
          error: `${UNBROKEN_NAME} already holds volumes of ${UNBROKEN_ROOT.path}`,
          volumes,
        })}`,
      ),
    )
    const list = folder.querySelector('.move-volumes') as HTMLDetailsElement
    list.open = true
    await document.fonts.ready
    expect(list.querySelectorAll('li')).toHaveLength(20)
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  it('keeps a move’s confirm button inside the page for the same path and name', async () => {
    const { host, folder } = await rehearsedMove(
      Promise.resolve({
        moved: {
          library_id: 2,
          library: UNBROKEN_NAME,
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
        },
        derive: derived(),
        dry_run: true,
      }),
    )
    const confirm = folder.querySelector('.move-root .confirm')
    expect(confirm?.textContent).toBe(
      `Move ${UNBROKEN_ROOT.path} to ${UNBROKEN_NAME}`,
    )
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  it('draws a move’s confirm as the action: --accent on its border and text, and heavier', async () => {
    const { folder } = await rehearsedMove(
      Promise.resolve({
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
        },
        derive: derived(),
        dry_run: true,
      }),
    )
    const confirm = folder.querySelector('.move-root .confirm') as HTMLElement
    const rehearse = folder.querySelector(
      'button[name="rehearse"]',
    ) as HTMLElement
    // The token as the browser resolves it, whichever scheme is in force.
    const probe = document.createElement('span')
    probe.style.color = 'var(--accent)'
    folder.append(probe)
    const accent = getComputedStyle(probe).color
    probe.remove()
    const style = getComputedStyle(confirm)
    expect(style.borderTopColor).toBe(accent)
    expect(style.color).toBe(accent)
    expect(style.fontWeight).toBe('600')
    expect(getComputedStyle(rehearse).borderTopColor).not.toBe(accent)
  })

  /** Makes a library through the make page's form: a name and its folder, submitted. */
  async function makeLibrary(host: HTMLElement, name: string, path: string) {
    const form = host.querySelector('form.add-library') as HTMLFormElement
    await typeInto(
      form.querySelector('input[name="library-name"]') as HTMLInputElement,
      name,
    )
    await typeInto(
      form.querySelector('input[name="path"]') as HTMLInputElement,
      path,
    )
    await submit(form)
  }

  it('keeps the make page’s own sentence about a refused library inside the window', async () => {
    // The sentence the make page says, naming a path as plain text: the
    // server's refusal of a library. It is not an element wider than its
    // box — the words run out of a paragraph that stays its own width — so
    // only the document can see it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.createLibrary).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries: 400 {"error":"${UNBROKEN_ROOT.path}: cannot be read: Permission denied (os error 13)"}`,
      ),
    )
    const host = await opened(NEW_LIBRARY)
    await makeLibrary(host, 'Manga', UNBROKEN_ROOT.path)
    await document.fonts.ready
    const alerts = [...host.querySelectorAll('form.add-library [role="alert"]')]
    expect(alerts.map((el) => el.textContent)).toEqual([
      `${UNBROKEN_ROOT.path}: cannot be read: Permission denied (os error 13)`,
    ])
    assertNoSidewaysScroll(host)
    expect(alerts[0].getBoundingClientRect().right).toBeLessThanOrEqual(
      document.documentElement.clientWidth,
    )
  })

  /**
   * A folder's page around `children`, with the ancestors the app draws it
   * in: Configuration, the Libraries screen, and the folder's
   * `section.root-row`, inside the shell.
   */
  function onAFolderPage(children: React.ReactNode) {
    return inShell(
      <div className="config">
        <section className="libraries">
          <div className="folder-page">
            <section className="root-row">{children}</section>
          </div>
        </section>
      </div>,
    )
  }

  /**
   * A scan report where a folder's page draws it, at the level `RootRow`
   * gives it, holding the `unreadable` block and the walk given.
   *
   * **Nothing around it is padded**, so the report has the width of `main`'s
   * content, and a rule's defect shows only where the window is narrow
   * enough: each case's viewport is one at which the defect it names,
   * put back by hand, fails it.
   */
  async function reportOnAFolderPage(
    unreadable: Unreadable,
    scan: Partial<Walk> = {},
  ) {
    return await settled(
      onAFolderPage(
        <ScanReport
          outcome={finished({
            scan: walk(scan),
            derive: derived({ unreadable }),
          })}
          level={3}
        />,
      ),
    )
  }

  it("keeps the scan report's list of causes inside its row", async () => {
    // **This is what holds `.scan-report > section > dl`** (O177). As
    // `.scan-report dl`, that rule reaches `UnreadableCauses`'s own nested
    // `<dl>` and makes it a grid of one 16rem column whatever it holds,
    // which is wider than the row a 360px window leaves it. A wider window
    // has room for the column, so this case is at 360.
    //
    // **The assertion is on the list itself**: a list that ends in `main`'s
    // padding scrolls nothing.
    //
    // **No cause in this fixture names a path, on purpose**: a path is the
    // other thing that has pushed this list past its row (O178, held by the
    // cases below), and here the rule is the only thing that can.
    await page.viewport(360, 844)
    const host = await reportOnAFolderPage({
      total: 26,
      refused_format: 15,
      by_format: { '7z': 3, rar: 12 },
      uncounted: [],
      vanished: [],
      no_pages: 11,
    })
    await document.fonts.ready
    const row = host.querySelector('.unreadable-causes') as HTMLElement
    const causes = host.querySelector(
      '.unreadable-causes > dd > dl',
    ) as HTMLElement
    expect(row).not.toBeNull()
    expect(causes).not.toBeNull()
    expect(causes.getBoundingClientRect().right).toBeLessThanOrEqual(
      row.getBoundingClientRect().right + 0.5,
    )
  })

  it('does not scroll sideways with a populated scan report', async () => {
    // **What it holds is that a report naming paths stays inside the
    // folder's page** (O178): a path does not break at a slash, and without
    // the `overflow-wrap` of `.scan-report .paths li, .derived .paths li` the
    // causes that name one end past the page in a 320px window. It asks the page and not only the
    // document, which a list ending in `main`'s padding does not scroll.
    //
    // Fixture copied from `ScanReport.test.tsx`'s "says why each unreadable
    // entry was skipped, cause by cause".
    await page.viewport(320, 844)
    const host = await reportOnAFolderPage({
      total: 29,
      refused_format: 15,
      by_format: { '7z': 3, rar: 12 },
      uncounted: ['/srv/manga/Bisque Doll/Bisque Doll.cbz'],
      vanished: ['/srv/manga/Gone - v01', '/srv/manga/Gone - v02'],
      no_pages: 11,
    })
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
    // On a folder's page a report's headings step off what is above them and
    // sit near what they head.
    const headings = [...host.querySelectorAll('.report-heading')]
    expect(headings).toHaveLength(2)
    for (const heading of headings) {
      expect(getComputedStyle(heading).marginTop).toBe('12px')
      expect(getComputedStyle(heading).marginBottom).toBe('8px')
    }
  })

  /** The one file the real library's scan left uncounted: longer than any row here. */
  const LONG_PATH =
    '/home/davey/temp/manga-test/Volumes/Bisque Doll de Chu ♡/Bisque Doll de Chu ♡.cbz'

  /** A path with nowhere to break: no space and no hyphen, and wider than the window. */
  const UNBROKEN_PATH = `/srv/manga/${'a_directory_with_no_break_in_it_'.repeat(2)}/v01.cbz`

  /**
   * Every element in the folder's page whose box ends past the page's edge,
   * by tag and class — so a failure names what stuck out. The document
   * cannot stand in for this: the page sits inside `main`'s padding, and a
   * list that ends in that padding scrolls nothing.
   *
   * **Boxes, and so not words.** A path in a `<code>` is seen, because the
   * `<code>`'s box is the path's. Plain text that runs out of a paragraph is
   * not: the paragraph keeps its width and nothing here is past the page.
   * Only the document sees that, which is why the cases about a sentence ask
   * `assertNoSidewaysScroll`.
   */
  function pastItsPage(host: HTMLElement): string[] {
    const folderPage = host.querySelector('.folder-page') as HTMLElement
    const edge = folderPage.getBoundingClientRect().right + 0.5
    return [...folderPage.querySelectorAll('*')]
      .filter((el) => el.getBoundingClientRect().right > edge)
      .map((el) => `${el.tagName.toLowerCase()}.${el.className}`)
  }

  /** A row's label and its value, as boxes. */
  function rowOf(host: HTMLElement, field: string) {
    const row = host.querySelector(`[data-field="${field}"]`) as HTMLElement
    expect(row, field).not.toBeNull()
    return {
      label: (
        row.querySelector(':scope > dt') as HTMLElement
      ).getBoundingClientRect(),
      value: (
        row.querySelector(':scope > dd') as HTMLElement
      ).getBoundingClientRect(),
    }
  }

  /**
   * The stylesheet's own fallbacks in place of the two web faces, which is
   * what a reader has until the font files arrive and for good if they never
   * do. Wider than the faces: it is what took the populated case past the
   * viewport's edge before it waited for `document.fonts.ready` (O177).
   */
  function withFallbackFonts() {
    const root = document.documentElement
    root.style.setProperty('--font-ui', 'system-ui, sans-serif')
    root.style.setProperty('--font-title', 'serif')
    onTestFinished(() => {
      root.style.removeProperty('--font-ui')
      root.style.removeProperty('--font-title')
    })
  }

  it('keeps a path longer than its row inside the folder’s page, in the faces and in their fallbacks', async () => {
    await page.viewport(390, 844)
    const host = await reportOnAFolderPage(
      {
        total: 14,
        refused_format: 12,
        by_format: { rar: 12 },
        uncounted: [LONG_PATH],
        vanished: [UNBROKEN_PATH],
        no_pages: 0,
      },
      // A walk row as well as the causes: the causes sit in `DeriveSection`,
      // which `.derived .paths li` reaches on its own, and what the walk
      // found is only `.scan-report .paths li`'s.
      { unreadable_dirs: [UNBROKEN_PATH] },
    )
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])

    // The control that the fallback took: a label is wider in it.
    const label = host.querySelector('.unreadable-causes > dt') as HTMLElement
    const inTheFaces = label.getBoundingClientRect().width
    withFallbackFonts()
    expect(label.getBoundingClientRect().width).toBeGreaterThan(inTheFaces)
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  it('drops a list under its label when it does not fit beside it, and only then', async () => {
    // At 375 the causes' column is narrow enough to take one label onto two
    // lines, which the last check below needs.
    await page.viewport(375, 844)
    const host = await reportOnAFolderPage(
      {
        total: 13,
        refused_format: 12,
        by_format: { rar: 12 },
        uncounted: [LONG_PATH],
        vanished: [],
        no_pages: 0,
      },
      { excluded_dirs: ['/srv/a'] },
    )
    await document.fonts.ready

    // The long path: under its label, and from the label's own left edge.
    const long = rowOf(host, 'unreadable.uncounted')
    expect(long.value.top).toBeGreaterThanOrEqual(long.label.bottom)
    expect(Math.abs(long.value.left - long.label.left)).toBeLessThanOrEqual(0.5)

    // A list short enough to sit beside its label stays there.
    const short = rowOf(host, 'excluded_dirs')
    expect(short.value.left).toBeGreaterThanOrEqual(short.label.right)
    expect(short.value.top).toBeLessThan(short.label.bottom)

    // And a row with no list is not this rule's: its `0` stays beside a
    // label that takes two lines of the causes' narrow column. The first
    // line is what makes this about anything: a label on one line — 24px —
    // would have room for its `0` whether the row wrapped or not.
    const none = rowOf(host, 'unreadable.vanished')
    expect(none.label.height).toBeGreaterThan(40)
    expect(none.value.left).toBeGreaterThanOrEqual(none.label.right)
    expect(none.value.top).toBeLessThan(none.label.bottom)
  })

  /** A derivation that found nothing unreadable, for the cases about what the walk found. */
  const NOTHING_UNREADABLE = derived().unreadable

  it('keeps a list that drops close under its label when the row beside it is taller', async () => {
    // At 1280 the report's top-level lists are two columns, and a row is as
    // tall as the taller of its pair. A `.paths` row is a flex container
    // stretched to that height; once its list wraps onto a line of its own,
    // the two lines share the spare height between them unless told not to,
    // and the list ends up far below the label it belongs to.
    await page.viewport(1280, 900)
    const host = await reportOnAFolderPage(NOTHING_UNREADABLE, {
      unreadable_dirs: ['/srv/manga/some_directory_or_other/lost+found'],
      excluded_dirs: Array.from(
        { length: 12 },
        (_, i) => `/srv/manga/series ${i}/.thumbnails`,
      ),
    })
    await document.fonts.ready
    const one = rowOf(host, 'unreadable_dirs')
    const many = rowOf(host, 'excluded_dirs')
    // What the case needs to be about anything: the two share a grid row,
    // the first one's list has dropped, and the second's is many lines tall.
    expect(one.label.top).toBe(many.label.top)
    expect(one.value.top).toBeGreaterThanOrEqual(one.label.bottom)
    expect(many.value.height).toBeGreaterThan(200)
    // The list starts one line of label and the row's gap below the label's
    // top. Measured from the label's top and not its bottom: the label's own
    // box stretches down its line, so its bottom edge follows the list.
    expect(one.value.top - one.label.top).toBeLessThanOrEqual(40)
  })

  it('keeps a refused scan’s sentence inside the window', async () => {
    // `scan.rs` opens its refusal with the root's own path, and
    // `WalkSection` draws it as plain text in a paragraph on the folder's
    // page.
    await page.viewport(390, 844)
    const host = await reportOnAFolderPage(NOTHING_UNREADABLE, {
      refused: `${UNBROKEN_ROOT.path} scanned empty while the library holds 12 entries for it — refusing to apply.`,
    })
    await document.fonts.ready
    expect(host.querySelector('.refused')?.textContent).toContain(
      UNBROKEN_ROOT.path,
    )
    assertNoSidewaysScroll(host)
  })

  it('keeps a failed watch probe’s reason inside the folder’s page', async () => {
    // The folder page's `watch` fact is the probe's own sentence once its
    // folder has just been added, and `watch.rs` puts the path in it. The
    // library is made, and its folder's page opened from the library's.
    await page.viewport(390, 844)
    const reason = `watching ${UNBROKEN_ROOT.path}: No space left on device (os error 28)`
    vi.mocked(librariesApi.listLibraries)
      .mockResolvedValueOnce({ libraries: [] })
      .mockResolvedValue(holding(UNBROKEN_ROOT))
    vi.mocked(librariesApi.createLibrary).mockResolvedValueOnce({
      library: holding(UNBROKEN_ROOT).libraries[0],
      added: [
        {
          root: UNBROKEN_ROOT,
          watching: false,
          watch_reason: reason,
          interval_secs: 3600,
        },
      ],
    })
    const host = await opened(NEW_LIBRARY)
    await makeLibrary(host, 'Manga', UNBROKEN_ROOT.path)
    await press(host.querySelector('.folder-table .row-link') as Element)
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.folder-page .regime dd')].map(
        (el) => el.textContent,
      ),
    ).toContain(`probe failed: ${reason}`)
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  it('keeps a scan that ran and failed inside the window', async () => {
    // `ScanReport` draws a failed scan as one paragraph, and the server's
    // sentence for it names the root's path.
    await page.viewport(390, 844)
    const why = `${UNBROKEN_ROOT.path} was scanned, but deriving its structure afterward failed: database is locked`
    const host = await settled(
      onAFolderPage(<ScanReport outcome={{ error: why }} level={3} />),
    )
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.scan-report > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([why])
    assertNoSidewaysScroll(host)
  })

  it('keeps a turned-down “Scan folder”’s sentence inside the window', async () => {
    // The folder page's own alert, for a "Scan folder" the server turned
    // down with a sentence that names the root's path — as its 409 for a
    // scan already running does.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    const why = `/api/library/roots/3/scan: 500 ${UNBROKEN_ROOT.path} could not be scanned`
    vi.mocked(config.startScan).mockRejectedValueOnce(new Error(why))
    const host = await opened(FOLDER)
    await act(async () => {
      const scan = [
        ...host.querySelectorAll<HTMLButtonElement>('.root-row button'),
      ].find((b) => b.textContent === 'Scan folder')
      scan?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.root-row > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([`${UNBROKEN_ROOT.path} could not be scanned`])
    assertNoSidewaysScroll(host)
  })

  it('keeps a long path inside the folder’s page in a removal’s rehearsal, outside any scan report', async () => {
    // What removing the folder would make of the library is drawn in the
    // removal's own `<details>`, which no `.scan-report` holds.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived({
        unreadable: {
          total: 1,
          refused_format: 0,
          by_format: {},
          uncounted: [UNBROKEN_PATH],
          vanished: [],
          no_pages: 0,
        },
      }),
      dry_run: true,
    })
    const host = await opened(FOLDER)
    await press(
      [...host.querySelectorAll('.root-row button')].find(
        (b) => b.textContent === 'Remove…',
      ) as Element,
    )
    const rehearsal = host.querySelector(
      '.remove-root .removal-derive',
    ) as HTMLDetailsElement
    rehearsal.open = true
    await document.fonts.ready
    expect(rehearsal.closest('.scan-report')).toBeNull()
    expect(rehearsal.textContent).toContain(UNBROKEN_PATH)
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  /** The page of a library of one folder, named `name`, in the shell. */
  async function libraryNamed(name: string, folder: config.Root) {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ ...holding(folder).libraries[0], name }],
    })
    return await opened(LIBRARY)
  }

  it('keeps a library whose name has nowhere to break inside the window', async () => {
    // A library is named by a person, and a name need not hold a space. It is
    // drawn as its page's heading and, below, in the form that renames it.
    await page.viewport(390, 844)
    const host = await libraryNamed(UNBROKEN_NAME, UNBROKEN_ROOT)
    await document.fonts.ready
    const heading = host.querySelector('.library-page > h2') as HTMLElement
    expect(heading.textContent).toBe(UNBROKEN_NAME)
    assertNoSidewaysScroll(host)
    expect(heading.scrollWidth).toBeLessThanOrEqual(heading.clientWidth)
    await press(host.querySelector('button[name="rename-library"]') as Element)
    assertNoSidewaysScroll(host)
  })

  it('keeps the folder browser inside the page', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listFolders).mockResolvedValue({
      path: UNBROKEN_ROOT.path,
      parent: '/',
      folders: [
        {
          name: 'x'.repeat(80),
          path: `${UNBROKEN_ROOT.path}/${'x'.repeat(80)}`,
        },
      ],
    })
    const host = await libraryNamed('Manga', UNBROKEN_ROOT)
    await press(host.querySelector('button[name="add-folder"]') as Element)
    await press(
      host.querySelector(
        '.library-page > .add-root button[name="browse"]',
      ) as Element,
    )
    await document.fonts.ready
    const browser = host.querySelector('.folder-browser') as HTMLElement
    const entry = host.querySelector('.folder-list button') as HTMLElement
    expect(entry.textContent).toBe('x'.repeat(80))
    assertNoSidewaysScroll(host)
    expect(entry.getBoundingClientRect().right).toBeLessThanOrEqual(
      browser.getBoundingClientRect().right,
    )
    const where = host.querySelector('.folder-browser .where') as HTMLElement
    expect(where.getBoundingClientRect().right).toBeLessThanOrEqual(
      browser.getBoundingClientRect().right,
    )
    // The box has a line of its own and the two buttons share the one beneath
    // it: neither is left alone on a line (`flex-basis` of the box).
    const buttons = ['pick', 'browse'].map((name) =>
      (
        host.querySelector(
          `.library-page > .add-root button[name="${name}"]`,
        ) as HTMLElement
      ).getBoundingClientRect(),
    )
    expect(buttons[1].top).toBe(buttons[0].top)
  })

  it('keeps the folder browser’s own path inside it, whatever paragraph rule is above it', async () => {
    // Under `.libraries` the screen's paragraph rule breaks the path as well,
    // so the library page's case cannot tell whether `.folder-browser .where`
    // does. Mounted bare, only that rule is left to.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listFolders).mockResolvedValue({
      path: UNBROKEN_ROOT.path,
      parent: '/',
      folders: [],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<FolderPicker action="Add folder" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    await document.fonts.ready
    const where = host.querySelector('.folder-browser .where') as HTMLElement
    expect(where.textContent).toBe(UNBROKEN_ROOT.path)
    const box = host.querySelector('.folder-browser') as HTMLElement
    assertNoSidewaysScroll(host)
    expect(where.getBoundingClientRect().right).toBeLessThanOrEqual(
      box.getBoundingClientRect().right,
    )
  })

  it('keeps Up inside the folder browser when the folder it names has nowhere to break', async () => {
    // Mounted bare for the reason the case above is: no rule above the
    // browser is left to break the name for it.
    await page.viewport(390, 844)
    const outer = 'a_directory_with_no_break_in_it_'.repeat(3)
    vi.mocked(librariesApi.listFolders).mockResolvedValue({
      path: `/srv/manga/${outer}/Trial-a`,
      parent: `/srv/manga/${outer}`,
      folders: [],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<FolderPicker action="Add folder" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    await document.fonts.ready
    await expect
      .element(
        page.getByRole('button', { name: `Up to ${outer}`, exact: true }),
      )
      .toBeVisible()
    const up = host.querySelector('button[name="up"]') as HTMLElement
    const box = host.querySelector('.folder-browser') as HTMLElement
    assertNoSidewaysScroll(host)
    expect(up.getBoundingClientRect().right).toBeLessThanOrEqual(
      box.getBoundingClientRect().right,
    )
  })

  it('holds the folder list still while the next listing is read after a refusal', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listFolders).mockResolvedValue({
      path: '/srv/manga',
      parent: null,
      folders: [{ name: 'extra', path: '/srv/manga/extra' }],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<FolderPicker action="Add folder" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    vi.mocked(librariesApi.listFolders).mockRejectedValueOnce(
      new Error('/x: 403 {"error":"/srv/manga/extra is not readable"}'),
    )
    await press(host.querySelector('.folder-list button') as Element)
    await document.fonts.ready
    expect(host.querySelector('.folder-browser [role="alert"]')).not.toBeNull()
    const list = () =>
      (
        host.querySelector('.folder-list') as HTMLElement
      ).getBoundingClientRect().top
    const before = list()
    const held = deferred<librariesApi.FolderListing>()
    vi.mocked(librariesApi.listFolders).mockReturnValueOnce(held.promise)
    await press(host.querySelector('.folder-list button') as Element)
    expect(list()).toBe(before)
    await act(async () =>
      held.resolve({
        path: '/srv/manga/extra',
        parent: '/srv/manga',
        folders: [],
      }),
    )
  })

  it('holds the folder list still while the next listing is read, slow or not', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listFolders).mockResolvedValue({
      path: '/srv/manga',
      parent: null,
      folders: [
        { name: 'extra', path: '/srv/manga/extra' },
        { name: 'Volumes', path: '/srv/manga/Volumes' },
      ],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<FolderPicker action="Add folder" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    await document.fonts.ready
    const list = () =>
      (
        host.querySelector('.folder-list') as HTMLElement
      ).getBoundingClientRect().top
    const before = list()
    const held = deferred<librariesApi.FolderListing>()
    vi.mocked(librariesApi.listFolders).mockReturnValueOnce(held.promise)
    await press(host.querySelector('.folder-list button') as Element)
    expect(list()).toBe(before)

    await act(
      () =>
        new Promise((done) =>
          setTimeout(done, librariesApi.READING_DELAY_MS + 50),
        ),
    )
    expect(host.querySelector('.folder-browser')?.textContent).toContain(
      'Reading folders…',
    )
    expect(list()).toBe(before)
    await act(async () =>
      held.resolve({
        path: '/srv/manga/extra',
        parent: '/srv/manga',
        folders: [],
      }),
    )
  })

  it('keeps the library’s removal inside the window', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived(),
      dry_run: true,
    })
    const host = await libraryNamed(UNBROKEN_NAME, UNBROKEN_ROOT)
    await press(host.querySelector('button[name="remove-library"]') as Element)
    await document.fonts.ready
    const confirm = host.querySelector(
      '.remove-library .confirm',
    ) as HTMLElement
    expect(confirm.textContent).toBe(`Remove ${UNBROKEN_NAME}`)
    assertNoSidewaysScroll(host)
    const box = host.querySelector('.remove-library') as HTMLElement
    expect(confirm.getBoundingClientRect().right).toBeLessThanOrEqual(
      box.getBoundingClientRect().right,
    )
    // The dialog is followed by the folders it would remove, and is clear
    // of them.
    const next = box.nextElementSibling as HTMLElement
    expect(next.textContent).toBe('Folders')
    expect(
      next.getBoundingClientRect().top - box.getBoundingClientRect().bottom,
    ).toBeGreaterThanOrEqual(16)
  })

  it('keeps what a removal’s re-derive found inside the window', async () => {
    // The table draws the report, above the list, once the library that was
    // removed is no longer there to hold it.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived({
        unreadable: {
          total: 1,
          refused_format: 0,
          by_format: {},
          uncounted: [UNBROKEN_PATH],
          vanished: [],
          no_pages: 0,
        },
      }),
      dry_run: false,
    })
    const host = await libraryNamed(UNBROKEN_NAME, UNBROKEN_ROOT)
    await press(host.querySelector('button[name="remove-library"]') as Element)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    await press(host.querySelector('.remove-library .confirm') as Element)
    await document.fonts.ready
    const report = host.querySelector(
      '.library-table > .removal-report',
    ) as HTMLElement
    expect(report.querySelector('p')?.textContent).toBe(
      `What re-deriving after removing ${UNBROKEN_NAME} found:`,
    )
    expect(report.textContent).toContain(UNBROKEN_PATH)
    // Drawn as a box, so a long report reads as one notice above the list.
    expect(getComputedStyle(report).borderTopWidth).toBe('1px')
    assertNoSidewaysScroll(host)
    const edge = report.getBoundingClientRect().right + 0.5
    expect(
      [...report.querySelectorAll('*')]
        .filter((el) => el.getBoundingClientRect().right > edge)
        .map((el) => `${el.tagName.toLowerCase()}.${el.className}`),
    ).toEqual([])
  })

  it('brings the head of a removal’s report into view, from the foot of a long page', async () => {
    // The library's page is long and was scrolled to its foot; the report is
    // drawn at the head of the table, which is the page that follows. Focus
    // alone scrolls a report taller than the window only as far as its foot,
    // so the head, whose opening sentence says whose it is, has to be put in
    // view as well.
    await page.viewport(390, 844)
    const comics = {
      id: 2,
      name: 'Comics',
      works: 0,
      books: 0,
      folders: [root({ id: 5, path: '/srv/comics' })],
    }
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        holding(
          ...Array.from({ length: 30 }, (_, i) =>
            root({ id: 10 + i, path: `/srv/manga/${i}` }),
          ),
        ).libraries[0],
        comics,
      ],
    })
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived({ lost_positions: 4 }),
      dry_run: false,
    })
    const host = await opened(LIBRARY)
    await document.fonts.ready
    await press(host.querySelector('button[name="remove-library"]') as Element)
    // The document scrolls here, not `main`: `assertNoSidewaysScroll` reads
    // both for that reason.
    const doc = document.documentElement
    expect(doc.scrollHeight).toBeGreaterThan(doc.clientHeight * 1.5)
    window.scrollTo(0, doc.scrollHeight)
    const from = window.scrollY
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [comics],
    })
    await press(host.querySelector('.remove-library .confirm') as Element)
    const report = host.querySelector(
      '.library-table > .removal-report',
    ) as HTMLElement
    expect(document.activeElement).toBe(report)
    // The report is taller than the window, and its head is what is in view.
    expect(report.getBoundingClientRect().height).toBeGreaterThan(
      doc.clientHeight,
    )
    const head = (
      report.querySelector('p') as HTMLElement
    ).getBoundingClientRect()
    expect(head.top).toBeGreaterThanOrEqual(0)
    expect(head.bottom).toBeLessThanOrEqual(doc.clientHeight)
    // And it would not be, had the view been left where the library's page
    // had it: what makes this case about the head being brought into view.
    const left = Math.min(from, doc.scrollHeight - doc.clientHeight)
    expect(head.bottom + window.scrollY).toBeLessThan(left)
  })

  it('does not scroll sideways on the users screen', async () => {
    await page.viewport(390, 844)
    // The grant editor draws a library's folders, so the path that must wrap
    // is the one that is on screen.
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [libraryOf(UNBROKEN_ROOT)],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({ every_library: false, libraries: [{ id: 70, name: 'Manga' }] }),
      ],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(
          <MemoryRouter>
            <GateContext value={{ who, signOut: () => undefined }}>
              <Users />
            </GateContext>
          </MemoryRouter>,
        ),
      )
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    assertNoSidewaysScroll(host)
  })

  it.each([
    ['read', true],
    ['unread', false],
  ])(
    'keeps a granted library with a name that has nowhere to break inside the window, its list %s',
    async (_state, read) => {
      // The grant editor draws a library's name beside its tick, and in the
      // sentence that stands in for the list when the list was not read.
      await page.viewport(390, 844)
      if (read) {
        vi.mocked(librariesApi.listLibraries).mockResolvedValue({
          libraries: [
            {
              ...libraryOf(UNBROKEN_ROOT),
              name: UNBROKEN_NAME,
            },
          ],
        })
      } else {
        vi.mocked(librariesApi.listLibraries).mockRejectedValue(
          new Error('/api/library/libraries: 500 the store is locked'),
        )
      }
      vi.mocked(usersApi.listUsers).mockResolvedValue({
        users: [
          user({
            role: 'user',
            every_library: false,
            libraries: [{ id: 70, name: UNBROKEN_NAME }],
          }),
        ],
      })
      const host = await act(async () => {
        const mounted = mount(
          inShell(
            <MemoryRouter>
              <GateContext value={{ who, signOut: () => undefined }}>
                <Users />
              </GateContext>
            </MemoryRouter>,
          ),
        )
        await Promise.resolve()
        await Promise.resolve()
        return mounted
      })
      await document.fonts.ready
      expect(host.querySelector('.grants')?.textContent).toContain(
        UNBROKEN_NAME,
      )
      assertNoSidewaysScroll(host)
      const group = host.querySelector('.grants') as HTMLElement
      expect(group.scrollWidth).toBeLessThanOrEqual(group.clientWidth)
    },
  )

  it('does not scroll sideways on the backup screen', async () => {
    await page.viewport(390, 844)
    vi.mocked(backupApi.listBackups).mockResolvedValue({
      backups: [
        backupEntry({
          name: 'library-20260906T143502Z.db',
        }),
      ],
    })
    const host = await act(async () => {
      const mounted = mount(inShell(<Backup />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    assertNoSidewaysScroll(host)
  })
})

describe('the Configuration screens at desktop width', () => {
  it('keeps every control inside the card that holds it', async () => {
    await page.viewport(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [libraryOf(longPathRoot)],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({ every_library: false, libraries: [{ id: 70, name: 'Manga' }] }),
      ],
    })
    const host = await act(async () => {
      const mounted = mount(
        <MemoryRouter>
          <GateContext value={{ who, signOut: () => undefined }}>
            <Users />
          </GateContext>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    const card = host.querySelector('.user-row') as HTMLElement
    expect(card).not.toBeNull()
    const cardBox = card.getBoundingClientRect()
    // **The card's own *border box* is not 640px, and must not be asserted
    // as one.** There is no global `border-box` reset in this app (the
    // Ground Truth this task started from names it explicitly), so
    // `max-width: var(--config-measure)` constrains `.user-row`'s *content*
    // box — the border box measures content(640) plus its own padding and
    // border on top, 674px here (640 + 2×16px `--space-4` + 2×1px border).
    // Measured first as `cardBox.width <= 640`, which failed at 674 — the
    // wrong axis, not a CSS defect; fixed to compare against the content
    // box a control actually sits inside, which is what the controls below
    // are checked against.
    const cardStyle = getComputedStyle(card)
    const cardContentWidth =
      cardBox.width -
      Number.parseFloat(cardStyle.paddingLeft) -
      Number.parseFloat(cardStyle.paddingRight) -
      Number.parseFloat(cardStyle.borderLeftWidth) -
      Number.parseFloat(cardStyle.borderRightWidth)
    // The measure itself: this is what removing `.user-row`'s `max-width`
    // actually breaks — see this file's own mutation notes.
    expect(cardContentWidth).toBeLessThanOrEqual(40 * 16)
    const controls = card.querySelectorAll('input, select, textarea')
    expect(controls.length).toBeGreaterThan(0)
    for (const control of controls) {
      // `.field`, `.checks` and the role `<select>` all stretch to fill
      // whatever content box the card gives them — Task 2's whole point is
      // that only the card carries a figure, and no control carries one of
      // its own (`admin-config.css.test.ts`'s "sets the measure once"
      // pins the CSS side of that same rule).
      expect(control.getBoundingClientRect().width).toBeLessThanOrEqual(
        cardContentWidth + 0.5,
      )
    }
  })

  it('keeps the add-user form inside its own measure', async () => {
    await page.viewport(1280, 900)
    const host = await act(async () => {
      const mounted = mount(
        <MemoryRouter>
          <GateContext value={{ who, signOut: () => undefined }}>
            <Users />
          </GateContext>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    const form = host.querySelector('.add-user') as HTMLElement
    expect(form).not.toBeNull()
    const formBox = form.getBoundingClientRect()
    // Same content-box arithmetic as `.user-row`'s assertion above, and the
    // same reason: no global `border-box` reset, so the border box is the
    // wrong axis to check.
    const formStyle = getComputedStyle(form)
    const formContentWidth =
      formBox.width -
      Number.parseFloat(formStyle.paddingLeft) -
      Number.parseFloat(formStyle.paddingRight) -
      Number.parseFloat(formStyle.borderLeftWidth) -
      Number.parseFloat(formStyle.borderRightWidth)
    // `.add-user` shares admin.css's "one measure for the settings area"
    // selector list with `.user-row`, so the assertion above — which only
    // ever queries `.user-row` — passes whether or not `.add-user` is still
    // in that list. This is the separate guard the shared selector needs.
    expect(formContentWidth).toBeLessThanOrEqual(40 * 16)
  })

  it('keeps the backup list inside its own measure', async () => {
    await page.viewport(1280, 900)
    vi.mocked(backupApi.listBackups).mockResolvedValue({
      backups: [backupEntry()],
    })
    const host = await act(async () => {
      const mounted = mount(<Backup />)
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    const list = host.querySelector('.backup .backup-list') as HTMLElement
    expect(list).not.toBeNull()
    // A list with no rows can collapse to zero width and pass no matter what
    // `max-width` says, so this fixture (`backupEntry()`, above) must render
    // at least one — proving the width below is actually load-bearing.
    expect(list.querySelectorAll('li').length).toBeGreaterThan(0)
    const listBox = list.getBoundingClientRect()
    const listStyle = getComputedStyle(list)
    const listContentWidth =
      listBox.width -
      Number.parseFloat(listStyle.paddingLeft) -
      Number.parseFloat(listStyle.paddingRight) -
      Number.parseFloat(listStyle.borderLeftWidth) -
      Number.parseFloat(listStyle.borderRightWidth)
    // `.backup .backup-list` is its own rule, not shared with any other
    // selector — the one screen `--config-measure` otherwise never reached,
    // per admin.css's own comment on the rule — and nothing else in this
    // file watches it.
    expect(listContentWidth).toBeLessThanOrEqual(40 * 16)
  })

  it('puts each root fact on its own row beside its label', async () => {
    await page.viewport(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    const host = await settled(routed(FOLDER))
    const rows = [...host.querySelectorAll('.regime > div')]
    expect(rows).toHaveLength(3)
    const tops = rows.map(
      (row) => row.querySelector('dt')!.getBoundingClientRect().top,
    )
    // Three distinct rows: `display: contents` on `.regime > div` is what
    // makes the grid place the dt/dd pairs directly, one per row — drop it
    // and the three `<div>` wrappers become the grid items instead, each one
    // occupying a single cell rather than contributing a dt and a dd to two
    // columns, and the three collapse onto one line.
    expect(new Set(tops).size).toBe(3)
    for (const row of rows) {
      const dt = row.querySelector('dt')!.getBoundingClientRect()
      const dd = row.querySelector('dd')!.getBoundingClientRect()
      expect(dd.left).toBeGreaterThan(dt.right - 1)
    }
  })
})

describe('the libraries screen at desktop width', () => {
  /** Two libraries, the second holding only entries nothing reads yet. */
  const TWO = {
    libraries: [
      {
        id: 1,
        name: 'Manga',
        works: 5,
        books: 0,
        folders: [longPathRoot],
      },
      {
        id: 2,
        name: 'Light novels',
        works: 0,
        books: 3,
        folders: [root({ id: 4, path: '/srv/novels' })],
      },
    ],
  }

  async function shown(ui: React.ReactNode) {
    await page.viewport(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(TWO)
    const host = await settled(ui)
    await document.fonts.ready
    return host
  }

  const box = (el: Element) => el.getBoundingClientRect()

  it('sets a library’s page on the measure, says what it holds quietly, and lets its picker’s box take the line’s room', async () => {
    // The second library's page, which says both what it holds and that
    // none of it is readable yet.
    const host = await shown(routed(`${TABLE}/2`))
    const libraryPage = host.querySelector('.library-page') as HTMLElement
    expect(box(libraryPage).width).toBeLessThanOrEqual(40 * 16 + 0.5)

    // What a library holds is quiet: the colour and size of a folder's own
    // labels, `--muted` at `--text-meta`, as the browser resolves them.
    const probe = document.createElement('span')
    probe.style.color = 'var(--muted)'
    probe.style.fontSize = 'var(--text-meta)'
    libraryPage.append(probe)
    const { color, fontSize } = getComputedStyle(probe)
    probe.remove()
    for (const selector of ['.holds', '.no-readable']) {
      const sentence = getComputedStyle(
        host.querySelector(`.library-page > ${selector}`) as Element,
      )
      expect(sentence.color, selector).toBe(color)
      expect(sentence.fontSize, selector).toBe(fontSize)
    }

    // The picker's line runs to its own edge, Cancel last: the box takes the
    // room its buttons and Cancel leave.
    await press(host.querySelector('button[name="add-folder"]') as Element)
    const add = host.querySelector('.library-page > .add-root') as HTMLElement
    const cancel = add.querySelector('button[name="cancel-add"]') as HTMLElement
    expect(box(cancel).right).toBeCloseTo(box(add).right, 0)
  })

  it('sets the make page’s form on the measure, its button at its own width', async () => {
    const host = await shown(routed(NEW_LIBRARY))
    const form = host.querySelector('form.add-library') as HTMLElement
    expect(box(form).width).toBeLessThanOrEqual(40 * 16 + 0.5)
    // Its action keeps its own width and does not stretch across.
    const make = form.querySelector('button[type="submit"]') as HTMLElement
    expect(box(make).width).toBeLessThan(box(form).width / 2)
  })

  it('draws the two boxes of a folder’s first row at one height', async () => {
    // A neighbour's textarea, and its hint, made the row taller than a box
    // needs; a box in a grid stretches to its row unless told not to.
    const host = await shown(routed(FOLDER))
    const edition = host.querySelector('input[name="edition_key"]') as Element
    const language = host.querySelector(
      'input[name="default_language"]',
    ) as Element
    expect(box(language).height).toBe(box(edition).height)
  })

  it('lights a Folders row under the pointer only while the row is a link', async () => {
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 5, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived(),
      dry_run: true,
    })
    const host = await shown(routed(LIBRARY))
    const heading = host.querySelector('.library-page > h2') as HTMLElement
    const row = () => host.querySelector('.folder-table tbody tr') as Element
    const pointAt = (el: Element) =>
      act(async () => {
        await userEvent.hover(el)
      })
    const background = () => getComputedStyle(row()).backgroundColor
    await pointAt(heading)
    const unlit = background()
    await pointAt(row())
    expect(row().querySelector('a.row-link')).not.toBeNull()
    expect(background()).not.toBe(unlit)

    // The removal open, the row's link is withdrawn and the row goes nowhere.
    await press(host.querySelector('button[name="remove-library"]') as Element)
    expect(row().querySelector('a')).toBeNull()
    await pointAt(heading)
    await pointAt(row())
    expect(background()).toBe(unlit)
  })

  it('puts the removal’s report on the measure, as a grid', async () => {
    const host = await shown(routed(LIBRARY))
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 5, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 5, positions: 0, notes: 0, left_with_no_library: [] },
      derive: derived({ lost_positions: 4 }),
      dry_run: false,
    })
    await press(host.querySelector('button[name="remove-library"]') as Element)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [TWO.libraries[1]],
    })
    await press(host.querySelector('.remove-library .confirm') as Element)
    const report = host.querySelector(
      '.library-table > .removal-report',
    ) as Element
    // The report ends where the table beneath it does.
    expect(box(report).right).toBeCloseTo(
      box(host.querySelector('.library-table .config-table') as Element).right,
      0,
    )
    // Its lists are the two-column grid a folder's scan report has, so a
    // report of this many rows is not a page tall.
    const list = report.querySelector('.derived > dl') as Element
    expect(getComputedStyle(list).display).toBe('grid')
    expect(
      getComputedStyle(list).gridTemplateColumns.split(' ').length,
    ).toBeGreaterThan(1)
    // A heading of the report is its own size, not the page's heading.
    const heading = report.querySelector('.report-heading') as Element
    expect(getComputedStyle(heading).marginTop).toBe('20px')
    expect(Number.parseFloat(getComputedStyle(heading).fontSize)).toBeLessThan(
      Number.parseFloat(
        getComputedStyle(host.querySelector('.library-table > h2') as Element)
          .fontSize,
      ),
    )
  })

  it('draws a report’s heading alike at every level its caller may pass', async () => {
    // Styled by class, not by tag: a heading is the same label whether a
    // page holds the report under its h2 or the library page holds it under
    // a folder's h4. The browser's own heading faces differ from level to
    // level, so only the class can make these equal.
    await page.viewport(1280, 900)
    const host = await act(async () => {
      const mounted = mount(
        <div className="library">
          <main>
            <section className="libraries">
              <h2>Libraries</h2>
              {([3, 4, 5, 6] as const).map((level) => (
                <DeriveSection key={level} derive={derived()} level={level} />
              ))}
            </section>
          </main>
        </div>,
      )
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    const headings = [...host.querySelectorAll('.report-heading')]
    expect(headings.map((h) => h.tagName)).toEqual(['H3', 'H4', 'H5', 'H6'])
    const reference = getComputedStyle(headings[0])
    expect(reference.fontWeight).toBe('600')
    expect(reference.lineHeight).toBe('20px')
    // The title face, as the screen's own heading has it.
    expect(reference.fontFamily).toBe(
      getComputedStyle(host.querySelector('h2') as Element).fontFamily,
    )
    for (const heading of headings) {
      const style = getComputedStyle(heading)
      expect(style.fontFamily, heading.tagName).toBe(reference.fontFamily)
      expect(style.fontWeight, heading.tagName).toBe(reference.fontWeight)
      expect(style.lineHeight, heading.tagName).toBe(reference.lineHeight)
      expect(style.fontSize, heading.tagName).toBe(reference.fontSize)
    }
  })

  it('puts the folder browser on a line of its own, and its folders full width', async () => {
    vi.mocked(librariesApi.listFolders).mockResolvedValue({
      path: '/a',
      parent: '/',
      folders: [{ name: 'b', path: '/a/b' }],
    })
    const host = await shown(
      <FolderPicker action="Add folder" onPick={() => undefined} />,
    )
    await press(host.querySelector('button[name="browse"]') as Element)
    const browser = host.querySelector('.folder-browser') as Element
    expect(box(browser).width).toBeCloseTo(
      box(host.querySelector('.folder-picker') as Element).width,
      0,
    )
    // A folder's button takes the whole row of the list it is in.
    const entry = host.querySelector('.folder-list button') as Element
    expect(box(entry).width).toBeCloseTo(
      box(host.querySelector('.folder-list') as Element).width,
      0,
    )
  })

  it('draws the grants of an account as one group, with the ticks under their radio', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({
          role: 'user',
          every_library: false,
          libraries: [{ id: 1, name: 'Manga' }],
        }),
      ],
    })
    const host = await shown(
      <MemoryRouter>
        <GateContext value={{ who, signOut: () => undefined }}>
          <Users />
        </GateContext>
      </MemoryRouter>,
    )
    const group = host.querySelector('.user-row .grants') as HTMLElement
    expect(getComputedStyle(group).borderTopWidth).toBe('1px')
    expect(getComputedStyle(group).borderTopStyle).toBe('solid')
    expect(getComputedStyle(group).borderTopColor).toBe(
      getComputedStyle(host.querySelector('.user-row') as Element)
        .borderTopColor,
    )
    // A box, then a gap, then its words: for both radios and each tick.
    for (const label of group.querySelectorAll(
      ':scope > label, .grant-list label',
    )) {
      expect(getComputedStyle(label).display).toBe('flex')
      expect(getComputedStyle(label).columnGap).toBe('8px')
    }
    const radio = group.querySelector('input[type="radio"]') as Element
    const tick = group.querySelector('.grant-list input') as Element
    expect(box(tick).left).toBeGreaterThan(box(radio).left + 16)

    // The group stands a step below the boxes above it, and its legend is a
    // quiet label on the border: smaller than the words beneath it and not
    // in their colour.
    expect(getComputedStyle(group).marginTop).toBe('12px')
    const legend = group.querySelector(':scope > legend') as HTMLElement
    const words = group.querySelector(':scope > label') as HTMLElement
    expect(getComputedStyle(legend).fontSize).toBe('12px')
    expect(getComputedStyle(legend).paddingLeft).toBe('4px')
    expect(getComputedStyle(legend).color).not.toBe(
      getComputedStyle(words).color,
    )
  })
})

describe('the scan report', () => {
  it('draws a zero more quietly than a value', async () => {
    await page.viewport(1280, 900)
    // Not `walk.updated`/`walk.added` — `[data-zero] dd` and
    // `.count:not([data-zero]) dd` select by document order, not by field
    // name, and `WalkSection` renders `Paths` before any `Count`. The first
    // `[data-zero]` element in the whole report is the `unreadable_dirs`
    // `Paths` row (`finished()`'s default `unreadable_dirs: []`), and the
    // first unmarked `.count` is `dirs` (156), the first `Count` whose value
    // is non-zero. The rule under test is generic across `.count` and
    // `.paths`, so either pairing pins it; this is the pairing the queries
    // actually reach.
    const host = await act(async () =>
      mount(<ScanReport outcome={finished()} />),
    )
    const zero = host.querySelector('[data-zero] dd') as HTMLElement
    const value = host.querySelector(
      '.count:not([data-zero]) dd',
    ) as HTMLElement
    expect(zero).not.toBeNull()
    expect(value).not.toBeNull()
    expect(getComputedStyle(zero).color).not.toBe(getComputedStyle(value).color)
    // Pinned to the actual tokens, not merely "different" — `index.css.test.ts`
    // holds `--muted`/`--text` to their own contrast floors; this is what
    // says the scan report actually reaches them.
    expect(getComputedStyle(zero).color).toBe('rgb(107, 97, 81)')
    expect(getComputedStyle(value).color).toBe('rgb(35, 30, 23)')
  })
})

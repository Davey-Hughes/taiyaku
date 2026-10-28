import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { type Locator, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as backupApi from '../src/backup'
import * as config from '../src/config'
import {
  accountPath,
  dirPath,
  LIBRARIES,
  libraryPath,
  NEW_ACCOUNT,
  NEW_LIBRARY,
  USERS,
} from '../src/configAddress'
import * as librariesApi from '../src/libraries'
import { settleUntil } from './support/async'
import { buttonSaying, byRole } from './support/located'
import { emptyDerive } from './support/derive'
import {
  accounts,
  answerPairs,
  configAt,
  dir,
  manga,
  need,
  needed,
  px,
  resize,
  settle,
  who,
} from './support/sideBySide'
import type * as usersApi from '../src/users'

/**
 * Side by side (spec §3) in a real layout engine: the width decides whether
 * the end of a tree opens beside its list or alone, and only a browser
 * measures a width. Both pairs are held, the Users table beside an account
 * and a library's page beside a directory's, inside the real `Shell`, so the
 * sidebar and `main`'s padding take from the window what they take in the
 * app.
 */

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  scanStatus: vi.fn(),
  listRoots: vi.fn(),
  removalCost: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  libraryRemovalCost: vi.fn(),
  removeLibrary: vi.fn(),
  moveCost: vi.fn(),
}))
vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))
// The build's label at the sidebar's foot asks for the build; answered here,
// so the request never leaves the test.
vi.mock('../src/about', async (importOriginal) => {
  const { ABOUT } = await import('./support/about')
  return {
    ...(await importOriginal<typeof import('../src/about')>()),
    about: vi.fn().mockResolvedValue(ABOUT),
  }
})

/**
 * Accounts enough that the table is taller than the window at every size
 * used here, so the list beside a page has a scroll of its own to keep.
 */
const ACCOUNTS = accounts(30)
const LAST_ACCOUNT = ACCOUNTS[ACCOUNTS.length - 1]

/** Directories enough that the library's page is taller than the window. */
const MANGA = manga(30)
const LAST_DIR = MANGA.dirs[MANGA.dirs.length - 1]

beforeEach(() => {
  vi.clearAllMocks()
  answerPairs(ACCOUNTS, [MANGA])
  vi.mocked(backupApi.listBackups).mockResolvedValue({
    backups: [
      {
        name: 'library-20260906T143502Z.db',
        bytes: 1024 * 1024 * 12,
        taken: '20260906T143502Z',
      },
    ],
  })
})

/**
 * Each pair: what opens and what it opens from, the list's address, the root
 * class of the list and of the page opened from it, the address of each
 * row's page and of the last row's, and what the list's table is followed by
 * in its box.
 */
const PAIRS = [
  {
    opens: 'an account',
    from: 'the Users table',
    listAt: USERS,
    list: 'user-table',
    page: 'account-page',
    rowAt: (n: number) => accountPath(ACCOUNTS[n - 1].name),
    last: accountPath(LAST_ACCOUNT.name),
    // The table is the last thing in its box.
    foot: 'its box’s inner edge',
    after: null,
  },
  {
    opens: 'a directory',
    from: 'its library’s page',
    listAt: libraryPath(MANGA.name),
    list: 'library-page',
    page: 'dir-page',
    rowAt: (n: number) => dirPath(MANGA.name, MANGA.dirs[n - 1].id),
    last: dirPath(MANGA.name, LAST_DIR.id),
    // Add directory follows the table in its box.
    foot: 'Add directory',
    after: 'add-root',
  },
] as const

/**
 * The widths each layout is held at. The breakpoint is the split's own
 * width, so where the window puts it depends on the shell around it; each
 * case near it checks it is within 100px of it, on the side it names, so a
 * breakpoint 100px off in either direction draws the wrong layout in one of
 * them.
 */
const JUST_WIDE = 1300
const WIDE = 1920
const JUST_NARROW = 1220
const PHONE = 390

/** A real mouse, inside `act` so that React sees what it starts. */
const click = async (el: Element | Locator) => {
  await act(async () => {
    await userEvent.click(el)
  })
  await settle()
}
/** A real mouse on a row's link (`byRole` says why not on the element). */
const clickRow = (link: Element) => click(byRole(link, 'link'))
/** Real keys, the same way. */
const keys = async (text: string) => {
  await act(async () => {
    await userEvent.keyboard(text)
  })
  await settle()
}

/** Configuration at `entry`, inside the shell, at a window of `width`. */
const shell = (width: number, entry: string) => configAt(width, 900, entry)

/** The split's width beyond what a pair needs; negative where it is short. */
function overBreakpoint(host: HTMLElement): number {
  const split = host.querySelector('.split') as HTMLElement
  return split.clientWidth - needed()
}

describe('a wide window draws the end of the tree beside its list', () => {
  for (const { opens, from, list, page: opened, rowAt } of PAIRS) {
    it.each([JUST_WIDE, WIDE])(
      `draws ${opens} beside ${from} at %ipx: its row marked, one --space-6 after the list, the list 20rem to 40rem`,
      async (width) => {
        const entry = rowAt(3)
        const host = await shell(width, entry)
        const over = overBreakpoint(host)
        expect(over).toBeGreaterThanOrEqual(0)
        if (width === JUST_WIDE) expect(over).toBeLessThan(100)

        const column = need(host, '.split-pair > .split-list')
        const listEl = need(column, `:scope > .${list}`)
        const pageEl = need(host, `.split-pair > .split-detail > .${opened}`)
        expect(
          [...listEl.querySelectorAll('.row-link[aria-current="page"]')].map(
            (a) => a.getAttribute('href'),
          ),
        ).toEqual([entry])

        const gap =
          pageEl.getBoundingClientRect().left -
          listEl.getBoundingClientRect().right
        expect(gap).toBeCloseTo(px('var(--space-6)'), 0)

        const listWidth = column.getBoundingClientRect().width
        expect(listWidth).toBeGreaterThanOrEqual(px('20rem') - 0.5)
        expect(listWidth).toBeLessThanOrEqual(px('40rem') + 0.5)
      },
    )
  }
})

describe('a narrow window draws the end of the tree alone', () => {
  for (const { opens, from, list, page: opened, rowAt } of PAIRS) {
    it.each([JUST_NARROW, PHONE])(
      `draws ${opens} alone at %ipx, with ${from} not in the document`,
      async (width) => {
        const host = await shell(width, rowAt(3))
        if (width === JUST_NARROW) {
          const over = overBreakpoint(host)
          expect(over).toBeLessThan(0)
          expect(over).toBeGreaterThan(-100)
        }
        need(host, `.split-detail > .${opened}`)
        expect(host.querySelector('.split-pair')).toBeNull()
        expect(host.querySelector('.split-list')).toBeNull()
        expect(host.querySelector(`.${list}`)).toBeNull()
        if (opened === 'dir-page') {
          // The directory's page asks once whether another of its library's
          // directories is scanning; a library page mounted out of sight
          // would ask for its own scans too.
          expect(librariesApi.libraryScanStatus).toHaveBeenCalledTimes(1)
          expect(librariesApi.libraryScanStatus).toHaveBeenCalledWith(MANGA.id)
        }
      },
    )
  }
})

describe('focus goes to the opened page and back to its row', () => {
  for (const { opens, from, listAt, list, page: opened, rowAt } of PAIRS) {
    it.each([
      ['beside it', WIDE],
      ['alone', JUST_NARROW],
    ] as const)(
      `opens ${opens} from ${from}, drawn %s, by a click onto its heading, and its back link returns focus to the row`,
      async (layout, width) => {
        const host = await shell(width, listAt)
        // Inside the frame first: a key pressed before anything here has
        // focus goes to the frame around the test.
        const listTitle = need(host, `.${list} .page-title`)
        await act(async () => listTitle.focus())
        expect(document.activeElement).toBe(listTitle)

        const to = rowAt(3)
        await clickRow(need(host, `.${list} .row-link[href="${to}"]`))
        expect(host.querySelector('.split-pair') !== null).toBe(
          layout === 'beside it',
        )
        expect(document.activeElement).toBe(
          need(host, `.${opened} .page-title`),
        )

        // The back link is the header's first stop, just before the title.
        await keys('{Shift>}{Tab}{/Shift}')
        const back = need(host, `.${opened} .back a`)
        expect(document.activeElement).toBe(back)
        await keys('{Enter}')
        expect(host.querySelector(`.${opened}`)).toBeNull()
        expect(document.activeElement).toBe(
          need(host, `.${list} .row-link[href="${to}"]`),
        )
      },
    )
  }
})

describe('the list beside a page survives a switch of rows', () => {
  for (const { opens, from, list, rowAt } of PAIRS) {
    it(`keeps ${from}, its node and its scroll, when ${opens} beside it gives way to another`, async () => {
      const host = await shell(WIDE, rowAt(1))
      const column = need(host, '.split-pair > .split-list')
      const listEl = need(column, `:scope > .${list}`) as HTMLElement & {
        kept?: true
      }
      listEl.kept = true
      // The list scrolls on its own beside the page. Halfway, so that a list
      // scrolled to bring the open row to its head, or drawn afresh at its
      // own head, would move.
      column.scrollTop = (column.scrollHeight - column.clientHeight) / 2
      const scrolled = column.scrollTop
      expect(scrolled).toBeGreaterThan(0)

      // The row across the middle of what the window shows of the list: in
      // view, so nothing need scroll to reach it, and at neither end.
      const view = column.getBoundingClientRect()
      const middle = (view.top + Math.min(view.bottom, window.innerHeight)) / 2
      const target = [
        ...listEl.querySelectorAll<HTMLAnchorElement>('.row-link'),
      ].find((link) => {
        const row = (link.closest('tr') as HTMLElement).getBoundingClientRect()
        return row.top <= middle && row.bottom >= middle
      })
      if (target === undefined) throw new Error('no row across the middle')
      const to = target.getAttribute('href')
      expect(to).not.toBe(rowAt(1))

      await clickRow(target)
      expect(
        need(host, '.row-link[aria-current="page"]').getAttribute('href'),
      ).toBe(to)
      const after = need(host, '.split-pair > .split-list')
      expect(
        (need(after, `:scope > .${list}`) as HTMLElement & { kept?: true })
          .kept,
      ).toBe(true)
      expect(after.scrollTop).toBeCloseTo(scrolled, 0)
    })
  }
})

/**
 * A table in a box is inset `--space-4` from it on all four sides, and a lit
 * row is lit inside that inset, so the last row ends one `--space-4` above
 * what ends the inset: the box's inner edge where the table is the last
 * thing in its box, and the next thing in the box otherwise. Held where the
 * table draws as a table and where its rows draw in two lines, which makes
 * it a block: beside an open page, and at a phone's width, alone.
 */
describe('the last row sits one --space-4 inside its box', () => {
  for (const { from, listAt, list, last, foot, after } of PAIRS) {
    it.each([
      [JUST_WIDE, 'beside a page', 'block'],
      [WIDE, 'beside a page', 'table'],
      [PHONE, 'alone', 'block'],
    ] as const)(
      `ends the last row of ${from} one --space-4 above ${foot} at %ipx, %s, drawn as a %s`,
      async (width, layout, display) => {
        const beside = layout === 'beside a page'
        const host = await shell(width, beside ? last : listAt)
        const listEl = need(
          host,
          `.split${beside ? '.split-pair' : ''} > .split-list > .${list}`,
        )
        const link = need(listEl, `.row-link[href="${last}"]`)
        // Lit where it is open beside the list.
        expect(link.getAttribute('aria-current')).toBe(beside ? 'page' : null)
        const row = link.closest('tr') as HTMLElement
        expect(row.parentElement?.lastElementChild).toBe(row)
        const table = row.closest('table') as HTMLElement
        expect(getComputedStyle(table).display).toBe(display)
        const box = table.parentElement as HTMLElement
        expect(box.classList.contains('settings-box')).toBe(true)
        const next = table.nextElementSibling
        expect(next?.className ?? null).toBe(after)
        const end =
          next === null
            ? box.getBoundingClientRect().bottom -
              Number.parseFloat(getComputedStyle(box).borderBottomWidth)
            : next.getBoundingClientRect().top
        expect(end - row.getBoundingClientRect().bottom).toBeCloseTo(
          px('var(--space-4)'),
          0,
        )
      },
    )
  }
})

/** Three libraries, so that the Libraries table's first and last rows differ. */
const SHELVES: librariesApi.Library[] = [
  MANGA,
  { id: 2, name: 'Comics', works: 412, books: 0, dirs: [dir(41)] },
  { id: 3, name: 'Light novels', works: 96, books: 0, dirs: [dir(42)] },
]

/**
 * The same inset on all four sides: the first row drawn starts one
 * `--space-4` below the box's inner edge, the last ends one above it, and the
 * table is one from each side. Where the rows draw in two lines the head row
 * is hidden, so the first drawn is the body's first. Held for the Users
 * table, beside a page and alone, and for the Libraries table, which is only
 * ever alone.
 */
describe('a table in its box is inset one --space-4 on all four sides', () => {
  it.each([
    ['the Users table', 'beside a page', JUST_WIDE, 'block'],
    ['the Users table', 'beside a page', WIDE, 'table'],
    ['the Users table', 'alone', PHONE, 'block'],
    ['the Libraries table', 'alone', PHONE, 'block'],
    ['the Libraries table', 'alone', WIDE, 'table'],
  ] as const)(
    'insets %s, %s, at %ipx, drawn as a %s',
    async (table, layout, width, display) => {
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: SHELVES,
      })
      const users = table === 'the Users table'
      const entry = users
        ? layout === 'beside a page'
          ? accountPath(ACCOUNTS[2].name)
          : USERS
        : LIBRARIES
      const host = await shell(width, entry)
      expect(host.querySelector('.split-pair') !== null).toBe(
        layout === 'beside a page',
      )
      const tableEl = need(
        host,
        `.${users ? 'user-table' : 'library-table'} .config-table`,
      )
      expect(getComputedStyle(tableEl).display).toBe(display)
      const box = tableEl.parentElement as HTMLElement
      expect(box.classList.contains('settings-box')).toBe(true)
      expect(tableEl.nextElementSibling).toBeNull()

      const rows = [...tableEl.querySelectorAll('tr')].filter(
        (tr) =>
          getComputedStyle(tr.parentElement as HTMLElement).position !==
          'absolute',
      )
      // The head row where it is drawn, and a row per account or library.
      expect(rows.length).toBe(
        (display === 'table' ? 1 : 0) + (users ? ACCOUNTS : SHELVES).length,
      )
      const first = rows[0].getBoundingClientRect()
      const last = rows[rows.length - 1].getBoundingClientRect()
      const at = tableEl.getBoundingClientRect()
      const edge = box.getBoundingClientRect()
      const border = getComputedStyle(box)
      const inner = {
        top: edge.top + Number.parseFloat(border.borderTopWidth),
        bottom: edge.bottom - Number.parseFloat(border.borderBottomWidth),
        left: edge.left + Number.parseFloat(border.borderLeftWidth),
        right: edge.right - Number.parseFloat(border.borderRightWidth),
      }
      const inset = px('var(--space-4)')
      expect({
        top: Math.round(first.top - inner.top),
        bottom: Math.round(inner.bottom - last.bottom),
        left: Math.round(at.left - inner.left),
        right: Math.round(inner.right - at.right),
      }).toEqual({ top: inset, bottom: inset, left: inset, right: inset })
    },
  )
})

/** The longest label a derive's report draws: ruling 2's. */
const LONG_LABEL = 'Members whose directory was removed mid-derive'

/** What a removal would take, and what re-deriving after it would find. */
const REMOVAL: config.RemovalAnswer = {
  removed: { works: 1, positions: 0, notes: 0, left_with_no_library: [] },
  derive: emptyDerive,
  dry_run: true,
}

/** What moving a directory of Manga to Comics would do. */
const MOVED: librariesApi.Moved = {
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
}

/** A scan of the directory at `path` that ended, as the status route says. */
const scanned = (path: string): config.FinishedScan => ({
  scan: {
    root: path,
    dirs: 1,
    files: 1,
    added: 1,
    updated: 0,
    gone: 0,
    refused: null,
    unreadable_dirs: [],
    excluded_dirs: [],
    excluded_files: 0,
    bad_exclude_patterns: [],
    forgotten_by_exclusion: 0,
  },
  derive: emptyDerive,
})

/** Opens the disclosure in `scope` whose summary says `words`, by a click. */
async function disclose(scope: ParentNode, words: string) {
  const summary = [...scope.querySelectorAll('summary')].find(
    (one) => one.textContent?.trim() === words,
  )
  if (summary === undefined) throw new Error(`no disclosure says ${words}`)
  await click(summary)
  expect((summary.parentElement as HTMLDetailsElement).open).toBe(true)
}

/**
 * Every page this PR draws, at a phone's width, and the states of them that
 * draw `LONG_LABEL`: a directory's and a library's scan report, each
 * removal's and a move's rehearsal with what re-deriving would find opened,
 * and the report the Libraries table draws after a removal. `holds` is the
 * page's own root, so a case that drew some other page fails.
 */
const AT_A_PHONE: Array<{
  page: string
  holds: string
  long: boolean
  open: () => Promise<HTMLElement>
}> = [
  {
    page: 'the Users table',
    holds: '.user-table',
    long: false,
    open: () => shell(PHONE, USERS),
  },
  {
    page: 'an account',
    holds: '.account-page',
    long: false,
    open: () => shell(PHONE, accountPath(ACCOUNTS[2].name)),
  },
  {
    page: 'New account',
    holds: '.new-account',
    long: false,
    open: () => shell(PHONE, NEW_ACCOUNT),
  },
  {
    page: 'the Libraries table',
    holds: '.library-table',
    long: false,
    open: () => shell(PHONE, LIBRARIES),
  },
  {
    page: 'New library',
    holds: '.new-library',
    long: false,
    open: () => shell(PHONE, NEW_LIBRARY),
  },
  {
    page: 'a library',
    holds: '.library-page',
    long: false,
    open: () => shell(PHONE, libraryPath(MANGA.name)),
  },
  {
    page: 'a directory',
    holds: '.dir-page',
    long: false,
    open: () => shell(PHONE, dirPath(MANGA.name, 3)),
  },
  {
    page: 'Backup',
    holds: '.backup',
    long: false,
    open: () => shell(PHONE, '/config/backup'),
  },
  {
    page: 'a directory with its scan report',
    holds: '.dir-page',
    long: true,
    open: async () => {
      // Running when the page is drawn, so the page takes it up, and the
      // poll after it ends reads its report.
      let scanning = true
      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve(
          scanning
            ? {
                root_id: id,
                running: true,
                elapsed_secs: 1,
                report: null,
                last_scan_at: null,
                last_scan_status: null,
              }
            : {
                root_id: id,
                running: false,
                report: scanned(dir(id).path),
                last_scan_at: null,
                last_scan_status: null,
              },
        ),
      )
      const host = await shell(PHONE, dirPath(MANGA.name, 3))
      scanning = false
      await settleUntil(
        'the directory’s report',
        () => host.querySelector('.dir-page .scan-report') !== null,
        3 * config.SCAN_POLL_MS,
      )
      return host
    },
  },
  {
    page: 'a library with what its scan found',
    holds: '.library-page',
    long: true,
    open: async () => {
      // A scan of two of Manga's directories is running when the page is
      // drawn, so the page takes it up, and the poll after it ends reads
      // each directory's report.
      let scanning = true
      vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
        Promise.resolve({
          library_id: id,
          running: scanning,
          elapsed_secs: scanning ? 1 : null,
          dirs: [1, 2].map((root_id) => ({ root_id, running: scanning })),
        }),
      )
      vi.mocked(config.scanStatus).mockImplementation((id) =>
        Promise.resolve({
          root_id: id,
          running: false,
          report: scanned(dir(id).path),
          last_scan_at: null,
          last_scan_status: null,
        }),
      )
      const host = await shell(PHONE, libraryPath(MANGA.name))
      scanning = false
      await settleUntil(
        'both directories’ reports',
        () => host.querySelectorAll('.library-page .dir-report').length === 2,
        3 * config.SCAN_POLL_MS,
      )
      return host
    },
  },
  {
    page: 'a directory’s removal, what re-deriving would find open',
    holds: '.dir-page',
    long: true,
    open: async () => {
      vi.mocked(config.removalCost).mockResolvedValue(REMOVAL)
      const host = await shell(PHONE, dirPath(MANGA.name, 3))
      await click(buttonSaying(need(host, '.dir-page'), 'Remove…'))
      await disclose(
        need(host, '.remove-root'),
        'What re-deriving without this directory would find',
      )
      return host
    },
  },
  {
    page: 'a library’s removal, what re-deriving would find open',
    holds: '.library-page',
    long: true,
    open: async () => {
      vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue(REMOVAL)
      const host = await shell(PHONE, libraryPath(MANGA.name))
      await click(need(host, 'button[name="remove-library"]'))
      await disclose(
        need(host, '.remove-library'),
        'What re-deriving without this library would find',
      )
      return host
    },
  },
  {
    page: 'a directory’s move, what deriving after it would find open',
    holds: '.dir-page',
    long: true,
    open: async () => {
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: SHELVES,
      })
      vi.mocked(librariesApi.moveCost).mockResolvedValue({
        moved: MOVED,
        derive: emptyDerive,
        dry_run: true,
      })
      const host = await shell(PHONE, dirPath(MANGA.name, 3))
      await click(buttonSaying(need(host, '.dir-page'), 'Move…'))
      await act(async () => {
        await userEvent.selectOptions(
          need(host, 'select[name="move-target"]'),
          String(SHELVES[1].id),
        )
      })
      await click(need(host, 'button[name="rehearse"]'))
      await disclose(
        need(host, '.dir-page'),
        'What deriving after the move would find',
      )
      return host
    },
  },
  {
    page: 'the Libraries table with a removal’s report',
    holds: '.library-table',
    long: true,
    open: async () => {
      vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue(REMOVAL)
      vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
        ...REMOVAL,
        dry_run: false,
      })
      const host = await shell(PHONE, libraryPath(MANGA.name))
      await click(need(host, 'button[name="remove-library"]'))
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: SHELVES.slice(1),
      })
      await click(need(host, '.remove-library .confirm'))
      need(host, '.library-table > .removal-report')
      return host
    },
  },
]

/**
 * At a phone's width, in each theme, neither the document nor `main`
 * scrolls sideways (`Config.browser.test.tsx`'s `assertNoSidewaysScroll`
 * gives why both). Ink draws the pages' text at 500 where Paper draws it at
 * 400, from another file of the face whose words take other widths, so each
 * case first asserts the weight its theme gives. Where a state draws
 * `LONG_LABEL`, each label is drawn, and inside its own box.
 */
describe('no Configuration page scrolls sideways at a phone’s width, in Paper or in Ink', () => {
  for (const [theme, name] of [
    ['paper', 'Paper'],
    ['ink', 'Ink'],
  ] as const) {
    for (const { page: drawn, holds, long, open } of AT_A_PHONE) {
      it(`keeps ${drawn} inside the window in ${name}`, async () => {
        document.documentElement.setAttribute('data-theme', theme)
        onTestFinished(() =>
          document.documentElement.removeAttribute('data-theme'),
        )
        const host = await open()
        await document.fonts.ready
        expect([
          window.innerWidth,
          document.documentElement.clientWidth,
        ]).toEqual([PHONE, PHONE])
        need(host, `main ${holds}`)
        expect(getComputedStyle(need(host, '.library')).fontWeight).toBe(
          theme === 'ink' ? '500' : '400',
        )
        if (long) {
          const labels = [...host.querySelectorAll('.derived dt')].filter(
            (dt) => dt.textContent === LONG_LABEL,
          )
          expect(labels.length).toBeGreaterThan(0)
          for (const label of labels) {
            expect(label.getClientRects().length).toBeGreaterThan(0)
            expect(label.scrollWidth).toBeLessThanOrEqual(label.clientWidth)
          }
        }
        const doc = document.documentElement
        const main = need(host, 'main')
        expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
        expect(main.scrollWidth).toBeLessThanOrEqual(main.clientWidth)
      })
    }
  }
})

/**
 * The separators between a two-line row's cells, and between a directory's
 * facts under its title, where the row or the facts take more than one line.
 * A separator is a "·" drawn before or after an item; each place a line
 * starts or ends with one, or two items share a line with no separator or
 * two between them, is named.
 */
describe('no line of a two-line row or of a directory’s facts starts or ends with a separator', () => {
  /** Whether `el`'s `pseudo` draws a "·" that can be seen. */
  const dot = (el: Element, pseudo: '::before' | '::after') => {
    const style = getComputedStyle(el, pseudo)
    return style.content.includes('·') && style.visibility === 'visible'
  }

  /**
   * Each misplaced separator among `items`, one row's or one list's, in
   * order, and how many of them start a line after the first.
   */
  function separators(items: HTMLElement[]) {
    const strays: string[] = []
    let wraps = 0
    items.forEach((item, i) => {
      const at = item.getBoundingClientRect()
      const before = items[i - 1]?.getBoundingClientRect()
      const after = items[i + 1]?.getBoundingClientRect()
      const startsLine = before === undefined || at.top >= before.bottom - 0.5
      const endsLine = after === undefined || after.top >= at.bottom - 0.5
      if (i > 0 && startsLine) wraps += 1
      const name = JSON.stringify(item.textContent)
      if (startsLine && dot(item, '::before')) {
        strays.push(`${name} starts a line with a separator`)
      }
      if (endsLine && dot(item, '::after')) {
        strays.push(`${name} ends a line with a separator`)
      }
      if (!endsLine && dot(item, '::after') === dot(items[i + 1], '::before')) {
        strays.push(`${name} and the next share a line with no one separator`)
      }
    })
    return { strays, wraps }
  }

  /** Every group `groups` finds, each held by `separators`. */
  const held = (host: HTMLElement, groups: string, items: string) => {
    const found = [...host.querySelectorAll<HTMLElement>(groups)].map((group) =>
      separators([...group.querySelectorAll<HTMLElement>(items)]),
    )
    expect(found.length).toBeGreaterThan(0)
    return {
      strays: found.flatMap(({ strays }) => strays),
      wraps: found.reduce((sum, { wraps }) => sum + wraps, 0),
    }
  }

  /** Administrators who see every library, each seen at a time of day. */
  const SEEN: usersApi.User[] = accounts(3).map((user) => ({
    ...user,
    role: 'admin',
    sessions: [
      {
        id: `s${user.id}`,
        created: '2026-10-20 08:00:00',
        last_seen: '2026-10-22 02:02:19',
        user_agent: null,
        ip: null,
        current: false,
      },
    ],
  }))
  const ROW_CELLS = 'td:not(.lead)'

  for (const { name, width, entry, groups, items } of [
    {
      name: 'the Users table beside an account',
      width: 1366,
      entry: accountPath(SEEN[1].name),
      groups: '.split-list .user-table tbody tr',
      items: ROW_CELLS,
    },
    {
      name: 'the Users table',
      width: PHONE,
      entry: USERS,
      groups: '.user-table tbody tr',
      items: ROW_CELLS,
    },
    {
      name: 'the Libraries table',
      width: PHONE,
      entry: LIBRARIES,
      groups: '.library-table tbody tr',
      items: ROW_CELLS,
    },
    {
      name: 'a library’s Directories',
      width: PHONE,
      entry: libraryPath(MANGA.name),
      groups: '.library-page .dir-table tbody tr',
      items: ROW_CELLS,
    },
    {
      name: 'a directory’s facts',
      width: WIDE,
      entry: dirPath(MANGA.name, 1),
      groups: '.dir-page .page-description .regime',
      items: ':scope > div',
    },
    {
      name: 'a directory’s facts',
      width: PHONE,
      entry: dirPath(MANGA.name, 1),
      groups: '.dir-page .page-description .regime',
      items: ':scope > div',
    },
  ]) {
    it(`keeps each separator in ${name} between two items on one line, at ${width}`, async () => {
      answerPairs(SEEN, [MANGA])
      const host = await shell(width, entry)
      const { strays, wraps } = held(host, groups, items)
      // Some line is wrapped, or there is nothing here to hold.
      expect(wraps).toBeGreaterThan(0)
      expect(strays).toEqual([])
    })
  }
})

/**
 * The dots written in these screens' text, as against those the stylesheet
 * draws between items: " · this is you" after an account's name, a last
 * scan's time and outcome, a session's two facts. Each that starts or ends a
 * line, wherever the window's width puts the line's end, is named with the
 * words around it.
 */
describe('no line of these screens starts or ends with a dot written in its text', () => {
  /**
   * Each dot in the text under `root` that starts or ends a line, and how
   * many dots there are. Lines are read from each character's box: a dot
   * starts a line where the drawn character before it is on a line above,
   * and ends one where the drawn character after it is on a line below.
   */
  function textDots(root: Element) {
    const drawn: { char: string; top: number; bottom: number }[] = []
    const range = document.createRange()
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
    for (
      let node = walker.nextNode();
      node !== null;
      node = walker.nextNode()
    ) {
      const text = node.textContent ?? ''
      for (let at = 0; at < text.length; at += 1) {
        if (/\s/.test(text[at])) continue
        range.setStart(node, at)
        range.setEnd(node, at + 1)
        const box = range.getClientRects()[0]
        if (box === undefined || box.height === 0) continue
        drawn.push({ char: text[at], top: box.top, bottom: box.bottom })
      }
    }
    const oneLine = (a: (typeof drawn)[number], b: (typeof drawn)[number]) =>
      a.top < b.bottom - 1 && b.top < a.bottom - 1
    const strays: string[] = []
    let dots = 0
    drawn.forEach((char, i) => {
      if (char.char !== '·') return
      dots += 1
      const words = drawn
        .slice(Math.max(0, i - 10), i + 11)
        .map((c) => c.char)
        .join('')
      if (i === 0 || !oneLine(drawn[i - 1], char)) {
        strays.push(`starts a line: ${words}`)
      }
      if (i === drawn.length - 1 || !oneLine(char, drawn[i + 1])) {
        strays.push(`ends a line: ${words}`)
      }
    })
    return { strays, dots }
  }

  /**
   * The signed-in account and a locked one, each named long enough to take
   * more than a line at a phone's width, both with a session; and Manga's
   * directories, one whose last scan's outcome went unrecorded.
   */
  const LONG = (id: number, name: string): usersApi.User => ({
    ...accounts(1)[0],
    id,
    name,
    sessions: [
      {
        id: `s${id}`,
        created: '2026-10-20 08:00:00',
        last_seen: '2026-10-22 02:02:19',
        user_agent: 'Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Firefox/131.0',
        ip: '100.64.0.12',
        current: false,
      },
    ],
  })
  const ME = LONG(who.user_id, 'reader-whose-name-runs-on-past-a-phone')
  const LOCKED = {
    ...LONG(2, 'another-reader-whose-name-runs-on-past-it'),
    can_log_in: false,
  }
  const SHELF: librariesApi.Library = {
    ...MANGA,
    dirs: [dir(1), { ...dir(2), last_scan_status: null }],
  }

  for (const { name, entry, root } of [
    { name: 'the Users table', entry: USERS, root: '.user-table tbody' },
    {
      name: 'an account’s page',
      entry: accountPath(ME.name),
      root: '.account-page',
    },
    {
      name: 'a library’s Directories',
      entry: libraryPath(SHELF.name),
      root: '.library-page .dir-table tbody',
    },
  ]) {
    it(`keeps each dot in the text of ${name} on a line with the words either side of it, at every width from 320 to 872`, async () => {
      answerPairs([ME, LOCKED], [SHELF])
      const host = await shell(PHONE, entry)
      const strays = new Set<string>()
      let dots = 0
      for (let width = 320; width <= 872; width += 12) {
        await resize(width, 900)
        const roots = host.querySelectorAll(root)
        expect(roots.length).toBeGreaterThan(0)
        for (const el of roots) {
          const found = textDots(el)
          dots += found.dots
          for (const stray of found.strays) strays.add(`${width}: ${stray}`)
        }
      }
      expect(dots).toBeGreaterThan(0)
      expect([...strays]).toEqual([])
    })
  }
})

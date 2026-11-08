import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { cdp, type Locator, page, userEvent } from 'vitest/browser'
// **Required**, the same reason `PreviewTabs.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app through
// `index.css`. Without this the boxes below are unstyled and the assertions
// measure browser defaults.
import '../src/index.css'
import { Config } from '../src/Config'
import * as config from '../src/config'
import type { Derived, FinishedScan, Unreadable, Walk } from '../src/config'
import { GateContext } from '../src/gate'
import { Backup } from '../src/Backup'
import * as librariesApi from '../src/libraries'
import * as backupApi from '../src/backup'
import { DirPicker } from '../src/DirPicker'
import * as usersApi from '../src/users'
import { DeriveSection, ScanReport } from '../src/ScanReport'
import { Row, Section } from '../src/settingsLayout'
import { deferred, flush } from './support/async'
import { holdOf } from './support/held'
import {
  type Channels,
  channels,
  contrast,
  tokenColour,
} from './support/colour'
import {
  changesOf,
  expectCallout,
  expectLaidUnderConfirm,
  expectPressesTaken,
  expectRoomUnseen,
  expectRowDrawnAt,
  layoutOf,
  liesOver,
  linesOf,
  middleWhereOver,
  pressAt,
  pressOn,
  roomKept,
} from './support/callout'
import { expectDisclosure } from './support/disclosure'
import { press, submit, typeInto } from './support/input'
import { boxOf, expectKept } from './support/kept'
import { buttonSaying, byRole } from './support/located'
import { mount, rerender } from './support/mount'
import {
  areaOf,
  coarseRulesOn,
  fingerArea,
  near,
  overlapOf,
  pressArea,
} from './support/pointer'
import { rootFontAt } from './support/rootFont'
import { tapAt } from './support/touch'
import { holdTheClock, pass, WAIT_DRAWN_AFTER_MS } from './support/wait'

/**
 * The browser pass over the Configuration screens. A measured width, a
 * computed colour and a grid's row count all need a real layout engine, so
 * none of them is measurable in jsdom.
 *
 * **These screens take no props and fetch their own data**, so this
 * file stubs `../src/config`, `../src/libraries`, `../src/users` and
 * `../src/backup` exactly as `Libraries.test.tsx`, `Users.test.tsx` and
 * `Backup.test.tsx` do — copying each mock surface from its matching jsdom
 * test rather than guessing which functions a mount actually calls. `mount`
 * keeps ONE module-level host, torn down in `afterEach` — one mount per
 * `it()`, never two, or the second leaks into `document.body` underneath the
 * first.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  addDir: vi.fn(),
  createLibrary: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listDirs: vi.fn(),
  listLibraries: vi.fn(),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
  removeLibrary: vi.fn(),
  unlinkCost: vi.fn(),
}))

vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  addUser: vi.fn(),
  listUsers: vi.fn(),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
  takeBackup: vi.fn(),
}))

// --- fixtures, copied from each screen's own jsdom test ---------------------

const root = (
  over: Partial<librariesApi.LibraryDir> = {},
): librariesApi.LibraryDir => ({
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
  libraries: [{ id: 1, name: 'Manga' }],
  nested: [],
  inside: [],
  ...over,
})

/**
 * A real path from this library, not a short fixture one. The scope
 * `<select>`'s intrinsic width would spill past its label — and out through
 * four `overflow-x: visible` ancestors onto the document — only against a
 * long real root like this 35-character one, not a ten-character
 * `/srv/manga`. `RuleForm.browser.test.tsx`'s own comment on `admin.css`'s
 * scope-select rule records exactly this.
 */
const longPathRoot = root({ path: '/home/davey/temp/manga-test/Volumes' })

/** A library of one directory, as the grant editor lists it. */
const libraryOf = (dir: librariesApi.LibraryDir): librariesApi.Library => ({
  id: 70,
  name: 'Manga',
  works: 0,
  books: 0,
  dirs: [{ ...dir, libraries: [{ id: 70, name: 'Manga' }] }],
})

/** Comics, library 2, as its directories name it. */
const COMICS = [{ id: 2, name: 'Comics' }]

/** One library holding `dirs`, as the list route sends it. */
const holding = (...dirs: librariesApi.LibraryDir[]) => ({
  libraries: [{ id: 1, name: 'Manga', works: 0, books: 0, dirs }],
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
  taken_over: 0,
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
/** The page that makes a library, outside the names' space. */
const NEW_LIBRARY = '/config/new-library'
/** The page of `holding(…)`'s library, at its name. */
const LIBRARY = `${TABLE}/Manga`
/** The page of `root()`'s directory, in `holding(…)`'s library. */
const DIR = `${LIBRARY}/dirs/3`

// --- the Users screen's three pages ------------------------------------------

/** The table of accounts. */
const USERS = '/config/users'
/** The page that makes an account, outside the names' space. */
const NEW_ACCOUNT = '/config/new-account'
/** The page of `user()`'s account, the one signed in, at its name. */
const ACCOUNT = `${USERS}/davey`

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
    held_removals: [],
    moves_held: false,
    scans: [],
    running: false,
    elapsed_secs: null,
    dirs: [],
  })
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
  // `clearAllMocks` keeps an implementation, so a removal one case held, or
  // a write it answered, would answer the next case's too.
  vi.mocked(usersApi.removeUser).mockReset()
  vi.mocked(usersApi.patchUser).mockReset()
  vi.mocked(usersApi.removalCost).mockReset()
  vi.mocked(config.patchRoot).mockReset()
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

/**
 * Presses `button`, whose write the test holds unanswered, and waits on the
 * browser's clock past the time a wait takes to be drawn, so that it is
 * drawn at its wait's `words`. Each of `kept`, the button first, keeps the
 * box it had: the button keeps the width of the words it waits with.
 */
async function drawnWaiting(
  button: HTMLElement,
  words: string,
  kept: HTMLElement[] = [button],
) {
  const names = kept.map((el) => el.textContent ?? '')
  const before = kept.map(boxOf)
  await act(async () => {
    button.click()
    await new Promise((r) => setTimeout(r, WAIT_DRAWN_AFTER_MS + 100))
  })
  expect(button.textContent).toBe(words)
  names.forEach((name, i) => {
    expectKept(name, before[i], boxOf(kept[i]))
  })
}

/** Configuration at `entry`, in the shell, once what it reads has landed. */
const opened = (entry: string) => settled(inShell(routed(entry)))

/**
 * The window at `width` × `height` for this test, and back at the size it
 * had once the test is done, so the cases after it start where they would
 * have without it.
 */
async function sized(width: number, height: number) {
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, height)
}

/** A spacing token's length in px, as the document defines it. */
function token(name: string): number {
  const px = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(name),
  )
  expect(px, name).toBeGreaterThan(0)
  return px
}

/**
 * A removal's confirm `box` laid out with its confirm and Cancel side by
 * side, `--space-2` apart, or, where the two do not fit on one line, on two
 * that far apart; then, where the removal has one, the disclosure of what
 * re-deriving would find, starting a `--space-5` below the lower of the
 * two; and in the document in that order, so Tab goes the way the eye does.
 */
function expectRemovalFoot(box: HTMLElement, lines: 1 | 2) {
  const confirm = box.querySelector('.confirm') as HTMLElement
  const cancel = buttonSaying(box, 'Cancel')
  const derive = box.querySelector<HTMLElement>(':scope > .removal-derive')
  const names = ['the confirm', 'Cancel', 'the disclosure']
  const drawn = [confirm, cancel, ...(derive === null ? [] : [derive])]
  drawn.slice(1).forEach((el, at) => {
    expect(
      drawn[at].compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING,
      `${names[at + 1]} after ${names[at]}`,
    ).toBeTruthy()
  })
  const gap = token('--space-2')
  const yes = confirm.getBoundingClientRect()
  const no = cancel.getBoundingClientRect()
  // Under the sentences they act on, where those start, at every width.
  const counts = (
    box.querySelector('.counts') as HTMLElement
  ).getBoundingClientRect()
  expect
    .soft(Math.abs(yes.left - counts.left), 'the confirm at the start')
    .toBeLessThanOrEqual(0.5)
  if (lines === 1) {
    expect.soft(Math.abs(no.top - yes.top), 'one line').toBeLessThanOrEqual(0.5)
    expect
      .soft(no.left - yes.right, 'the gap between')
      .toBeGreaterThanOrEqual(gap - 0.5)
  } else {
    expect
      .soft(no.top - yes.bottom, 'the gap between lines')
      .toBeGreaterThanOrEqual(gap - 0.5)
    // And on two only because the confirm's words, on one line, leave
    // Cancel no room beside it.
    const room = (box.querySelector('.settings-actions') as HTMLElement)
      .clientWidth
    confirm.style.width = 'max-content'
    const asked = confirm.getBoundingClientRect().width + gap + no.width
    confirm.style.width = ''
    expect.soft(asked, 'too wide for one line').toBeGreaterThan(room)
  }
  if (derive !== null) {
    expect
      .soft(
        derive.getBoundingClientRect().top - Math.max(yes.bottom, no.bottom),
        'the gap above the disclosure',
      )
      .toBeGreaterThanOrEqual(token('--space-5') - 0.5)
  }
  // The boxes a press can land on: no two share any of their area,
  // and each one's press area (`pressArea`) is its own box.
  const pressed = [
    confirm,
    cancel,
    ...(derive === null ? [] : [derive.querySelector('summary') as Element]),
  ]
  pressed.at(-1)?.scrollIntoView({ block: 'center' })
  const boxes = pressed.map((el) => areaOf(el.getBoundingClientRect()))
  boxes.forEach((area, at) => {
    boxes.slice(at + 1).forEach((other, past) => {
      expect
        .soft(
          overlapOf(area, other),
          `${names[at]} over ${names[at + 1 + past]}`,
        )
        .toBe(0)
    })
    expect
      .soft(pressArea(pressed[at]), `a press on ${names[at]}`)
      .toEqual(near(area))
  })
}

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
            dirs: [root(), root({ id: 4, path: '/srv/chapters' })],
          },
          {
            id: 2,
            name: 'Comics',
            works: 1,
            books: 0,
            dirs: [root({ id: 5, path: '/srv/comics', libraries: COMICS })],
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
          ).toBe(`/config/libraries/${['Manga', 'Comics'][i]}`)
        }
      })
    },
  )
})

/**
 * `table` at a phone's width: `rows` as drawn, each row a block of its name
 * and then its values, every value's label its column heading's words, and
 * the row whose name is `short` on two lines.
 */
function expectStacked(table: HTMLElement, rows: string[][], short: string) {
  // What is drawn: `innerText` leaves out what the stylesheet hides.
  expect(
    [...table.querySelectorAll('tbody tr')].map((tr) =>
      [...tr.querySelectorAll('td')].map((td) => td.innerText),
    ),
  ).toEqual(rows)
  // The headings are not drawn at this width, so a label says the
  // heading's own words.
  expect(
    (table.querySelector('thead') as HTMLElement).getBoundingClientRect().width,
  ).toBeLessThanOrEqual(1)
  const headings = [...table.querySelectorAll('thead th')].map(
    (th) => th.textContent,
  )
  for (const tr of table.querySelectorAll('tbody tr')) {
    const [lead, ...values] = [...tr.querySelectorAll('td')]
    values.forEach((td, i) => {
      const label = td.querySelector('.cell-label')
      if (label !== null) expect(label.textContent).toBe(`${headings[i + 1]}: `)
    })
    // A block: the name on its own line, and its values beneath it.
    expect(getComputedStyle(tr).display).not.toBe('table-row')
    const foot = lead.getBoundingClientRect().bottom
    for (const td of values) {
      expect(td.getBoundingClientRect().top).toBeGreaterThanOrEqual(foot - 0.5)
    }
    // Two lines: values short enough to share the line beneath the name do
    // share it. A long one may wrap; this row's are short.
    if (lead.innerText === short) {
      const top = values[0].getBoundingClientRect().top
      for (const td of values) {
        expect(td.getBoundingClientRect().top, td.innerText).toBeCloseTo(top, 0)
      }
    }
  }
  expect(
    [...table.querySelectorAll<HTMLElement>('td.lead')].map(
      (td) => td.innerText,
    ),
  ).toContain(short)
}

describe('a library’s Directories table, with its Overlaps column', () => {
  /** A real path's length: the column must leave a path like it its room. */
  const LONG = '/mnt/user/media/Manga/Kobayashi-san Chi no Meidoragon - Lucoa'
  const around = (path: string): librariesApi.AroundDir => ({
    path,
    libraries: ['Everything'],
    shows: true,
    kept_out_by: null,
  })
  /**
   * Manga's directories: the long path, which overlaps in all three ways and
   * whose last scan failed, so its row has the longest words; one inside
   * another library's directory; and one that overlaps nothing.
   */
  const OVERLAPPING = holding(
    root({
      path: LONG,
      last_scan_at: '2026-10-01 10:00:00',
      last_scan_status: 'failed',
      libraries: [{ id: 1, name: 'Manga' }, ...COMICS],
      inside: [around('/mnt/user/media')],
      nested: [
        {
          id: 9,
          path: `${LONG}/Extras`,
          libraries: ['Comics'],
          shown: true,
          kept_out_by: null,
        },
      ],
    }),
    root({
      id: 4,
      path: '/mnt/user/media/Manga/Chapters',
      inside: [around('/mnt/user/media')],
    }),
    root({ id: 5, path: '/srv/comics', scan_interval_secs: null }),
  )

  /** The library page at `width`, its Directories table and that table's box. */
  async function drawn(width: number) {
    await sized(width, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(OVERLAPPING)
    const host = await opened(LIBRARY)
    await document.fonts.ready
    const table = host.querySelector('.library-page .dir-table') as HTMLElement
    return { host, table, box: table.closest('.settings-box') as HTMLElement }
  }

  /** Each of `table`'s cells, heading or not, drawn inside `box`. */
  function expectInside(table: HTMLElement, box: HTMLElement) {
    const edge = box.getBoundingClientRect()
    for (const cell of table.querySelectorAll<HTMLElement>('th, td')) {
      const at = cell.getBoundingClientRect()
      if (at.width === 0) continue
      expect(at.left, cell.textContent).toBeGreaterThanOrEqual(edge.left)
      expect(at.right, cell.textContent).toBeLessThanOrEqual(edge.right)
    }
  }

  /** The width of `text`'s first drawing in `cell`, and how many lines it takes. */
  function wordIn(cell: Element, text: string) {
    const node = [...cell.childNodes].find(
      (one) =>
        one.nodeType === Node.TEXT_NODE && one.textContent?.includes(text),
    ) as Text
    const range = document.createRange()
    const at = (node.textContent ?? '').indexOf(text)
    range.setStart(node, at)
    range.setEnd(node, at + text.length)
    const boxes = [...range.getClientRects()].filter((one) => one.width > 0)
    return {
      width: range.getBoundingClientRect().width,
      lines: new Set(boxes.map((one) => Math.round(one.top))).size,
    }
  }

  it('gives Overlaps no more room than its heading or its widest word, which it never breaks, and keeps every cell inside the box at 1280', async () => {
    const { host, table, box } = await drawn(1280)
    const heading = table.querySelector('thead th.overlaps') as HTMLElement
    expect(heading.textContent).toBe('Overlaps')
    const cells = [...table.querySelectorAll('tbody tr')].map(
      (row) => row.querySelectorAll('td')[3],
    )
    expect(cells.map((cell) => (cell as HTMLElement).innerText)).toEqual([
      'Shared, Inside, Around',
      'Inside',
      '',
    ])
    const words = ['Shared,', 'Inside,', 'Around'].map((word) =>
      wordIn(cells[0], word),
    )
    for (const word of words) expect(word.lines).toBe(1)
    const style = getComputedStyle(heading)
    const room =
      heading.getBoundingClientRect().width -
      Number.parseFloat(style.paddingLeft) -
      Number.parseFloat(style.paddingRight)
    const widest = Math.max(
      wordIn(heading, 'Overlaps').width,
      ...words.map((word) => word.width),
    )
    expect(room).toBeLessThanOrEqual(widest + 1)
    expectInside(table, box)
    assertNoSidewaysScroll(host)
  })

  it('says Overlaps after its label at 390, draws nothing for a directory that overlaps nothing, and keeps every cell inside the box', async () => {
    const { host, table, box } = await drawn(390)
    expectStacked(
      table,
      [
        [
          LONG,
          'Scan every: 24h',
          'Last scan: 2026-10-01 10:00:00 UTC\u00a0·\u00a0failed',
          'Overlaps: Shared, Inside, Around',
        ],
        [
          '/mnt/user/media/Manga/Chapters',
          'Scan every: 24h',
          'Last scan: Never',
          'Overlaps: Inside',
        ],
        ['/srv/comics', 'Scan every: not set', 'Last scan: Never', ''],
      ],
      '/srv/comics',
    )
    // The row that overlaps nothing ends its line at Last scan: no
    // separator is drawn towards the empty cell after it.
    const blank = table.querySelectorAll('tbody tr')[2]
    const last = blank.querySelectorAll('td')[2]
    expect(last.innerText).toBe('Last scan: Never')
    expect(getComputedStyle(last, '::after').visibility).toBe('hidden')
    expectInside(table, box)
    assertNoSidewaysScroll(host)
  })

  it.each([
    [600, 'two lines'],
    [656, 'two lines'],
    [657, 'a table'],
  ])(
    'draws its rows at %i in %s: four columns are a table once their box is 36rem wide',
    async (width, drawnAs) => {
      const { table, box } = await drawn(width)
      const style = getComputedStyle(box)
      const room =
        box.clientWidth -
        Number.parseFloat(style.paddingLeft) -
        Number.parseFloat(style.paddingRight)
      if (drawnAs === 'a table') expect(room).toBeGreaterThanOrEqual(576)
      else expect(room).toBeLessThan(576)
      const row = table.querySelector('tbody tr') as HTMLElement
      const label = row
        .querySelectorAll('td')[3]
        .querySelector('.cell-label') as HTMLElement
      if (drawnAs === 'a table') {
        expect(getComputedStyle(row).display).toBe('table-row')
        expect(getComputedStyle(label).display).toBe('none')
      } else {
        expect(getComputedStyle(row).display).toBe('flex')
        expect(getComputedStyle(label).display).toBe('inline')
        expect(
          (table.querySelector('thead') as HTMLElement).getBoundingClientRect()
            .width,
        ).toBeLessThanOrEqual(1)
      }
    },
  )
})

describe('the Libraries pages at a phone’s width', () => {
  /** The page of `UNBROKEN_LIBRARY`'s library, at its name. */
  const UNBROKEN_PAGE = `${TABLE}/${UNBROKEN_NAME}`
  /**
   * One library whose name and directory have nowhere to break, its directory's
   * last scan failed: the longest words each of the four pages draws.
   */
  const UNBROKEN_LIBRARY = {
    libraries: [
      {
        id: 1,
        name: UNBROKEN_NAME,
        works: 12,
        books: 0,
        dirs: [
          {
            ...UNBROKEN_ROOT,
            last_scan_at: '2026-10-01 10:00:00',
            last_scan_status: 'failed',
            libraries: [{ id: 1, name: UNBROKEN_NAME }],
          },
        ],
      },
    ],
  }

  it.each([
    [TABLE, '.library-table .config-table'],
    [NEW_LIBRARY, '.new-library form.add-library'],
    [UNBROKEN_PAGE, '.library-page .dir-table'],
    [`${UNBROKEN_PAGE}/dirs/3`, '.dir-page .settings-section'],
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

  /** Two libraries: one whose directories hold a failed scan and none, one never scanned. */
  const SCANNED = {
    libraries: [
      {
        id: 1,
        name: 'Manga',
        works: 340,
        books: 0,
        dirs: [
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
        dirs: [root({ id: 5, path: '/srv/comics', libraries: COMICS })],
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
          '2 directories',
          '340 works',
          'Last scan: 2026-10-01 10:00:00 UTC\u00a0·\u00a0failed',
        ],
        ['Comics', '1 directory', '1 work', 'Last scan: Never'],
      ],
      'Comics',
    ],
    [
      LIBRARY,
      '.library-page .dir-table',
      [
        ['/srv/manga', 'Scan every: 24h', 'Last scan: Never', ''],
        [
          '/srv/chapters',
          'Scan every: not set',
          'Last scan: 2026-10-01 10:00:00 UTC\u00a0·\u00a0failed',
          '',
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
      expectStacked(host.querySelector(selector) as HTMLElement, rows, short)
    },
  )

  it.each([
    [390, 'shut'],
    [390, 'open'],
    [1280, 'shut'],
    [1280, 'open'],
  ])(
    'puts the add-directory picker’s Cancel on the line of its buttons, at %ipx with the browser %s',
    async (width, browser) => {
      await page.viewport(width, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(
        holding(longPathRoot),
      )
      vi.mocked(librariesApi.listDirs).mockResolvedValue({
        path: '/srv',
        parent: '/',
        dirs: [{ name: 'manga', path: '/srv/manga' }],
      })
      const host = await opened(LIBRARY)
      await press(host.querySelector('button[name="add-dir"]') as Element)
      const add = host.querySelector('.library-page .add-root') as HTMLElement
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
      const shown = add.querySelector('.dir-browser')
      expect(shown !== null).toBe(browser === 'open')
      if (shown !== null) {
        expect(shown.getBoundingClientRect().top).toBeGreaterThanOrEqual(
          cancel.bottom,
        )
      }
      assertNoSidewaysScroll(host)
    },
  )

  it.each([
    // [the window, the line it leaves, where the buttons go, the page open]
    // At 390 the three just fit on the line under the path box, in any
    // grouping of them, so this case holds only that the gutters leave them
    // that room.
    [390, 309, 'under the path box', LIBRARY],
    // The refused Add directory's case below.
    [520, 439, 'under the path box', LIBRARY],
    [531, 450, 'under the path box', LIBRARY],
    [633, 552, 'beside the path box', LIBRARY],
    // A desk's width: the page at its measure.
    [1280, 606, 'beside the path box', LIBRARY],
    // The library's page as the list, a directory's page beside it: at
    // 1280 the list's column is at its least, narrower than the three.
    [1280, 287, 'stacked under the path box', DIR],
    [1920, 606, 'beside the path box', DIR],
  ] as const)(
    'keeps Cancel beside Browse…, and Add directory on their line where the three fit, at %ipx, whose %ipx line has them %s',
    async (width, line, where, entry) => {
      await sized(width, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(
        holding(longPathRoot),
      )
      const host = await opened(entry)
      await press(host.querySelector('button[name="add-dir"]') as Element)
      await document.fonts.ready
      const add = host.querySelector('.library-page .add-root') as HTMLElement
      const at = add.getBoundingClientRect()
      // The line measured, so a change of gutter fails here rather than
      // moving every case to some other width unseen.
      expect(host.querySelector('.split-pair') !== null).toBe(entry === DIR)
      expect(at.width).toBeCloseTo(line, 0)
      const field = (
        add.querySelector('label.field') as HTMLElement
      ).getBoundingClientRect()
      const [pick, browse, cancel] = ['pick', 'browse', 'cancel-add'].map(
        (name) =>
          (
            add.querySelector(`button[name="${name}"]`) as HTMLElement
          ).getBoundingClientRect(),
      )
      // Cancel is never left alone: it is on Browse…'s line, after it.
      expect(cancel.top).toBeCloseTo(browse.top, 0)
      expect(cancel.left).toBeGreaterThan(browse.right)
      if (where === 'stacked under the path box') {
        // Too narrow for the three: the path box, then Add directory, then
        // Browse… and Cancel together, each line at the line's start (Add
        // directory's place is held below, with the other cases under the
        // path box).
        expect(browse.top).toBeGreaterThanOrEqual(pick.bottom)
        expect(browse.left).toBeCloseTo(at.left, 0)
        expect(pick.width + browse.width + cancel.width + 16).toBeGreaterThan(
          at.width,
        )
      } else {
        // The three on one line, in their order.
        expect(browse.top).toBeCloseTo(pick.top, 0)
        expect(browse.left).toBeGreaterThan(pick.right)
      }
      if (where === 'beside the path box') {
        // The path box gives up width before the buttons wrap, and keeps a
        // usable width while it does.
        expect(pick.left).toBeGreaterThan(field.right)
        expect(pick.bottom).toBeCloseTo(field.bottom, 0)
        expect(cancel.right).toBeCloseTo(at.right, 0)
        expect(field.width).toBeGreaterThanOrEqual(12 * 16)
      } else {
        // The buttons go under the path box, which takes the line.
        expect(pick.top).toBeGreaterThanOrEqual(field.bottom)
        expect(pick.left).toBeCloseTo(at.left, 0)
        expect(field.right).toBeCloseTo(at.right, 0)
      }
      assertNoSidewaysScroll(host)
    },
  )

  it('draws a refused Add directory on a line of its own beneath the buttons', async () => {
    // A refusal short enough to sit beside Cancel, were it let: at 520 the
    // box has its own line and the buttons' line beneath it has room for the
    // refusal's words, which is checked before what it is there for.
    await page.viewport(520, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    vi.mocked(librariesApi.addDir).mockRejectedValue(
      new Error('/api/library/libraries/1/dirs: 400 {"error":"refused"}'),
    )
    const host = await opened(LIBRARY)
    await press(host.querySelector('button[name="add-dir"]') as Element)
    const add = host.querySelector('.library-page .add-root') as HTMLElement
    await typeInto(
      add.querySelector('input[name="path"]') as HTMLInputElement,
      '/srv/extra',
    )
    await press(add.querySelector('button[name="pick"]') as Element)
    await document.fonts.ready
    const alert = add.querySelector('[role="alert"]') as HTMLElement
    expect(alert.textContent).toBe('refused')
    const words = document.createRange()
    words.selectNodeContents(alert)
    const cancel = (
      add.querySelector('button[name="cancel-add"]') as HTMLElement
    ).getBoundingClientRect()
    const pick = (
      add.querySelector('button[name="pick"]') as HTMLElement
    ).getBoundingClientRect()
    expect(cancel.top).toBeCloseTo(pick.top, 0)
    expect(
      add.getBoundingClientRect().right - cancel.right - 8,
    ).toBeGreaterThan(words.getBoundingClientRect().width)
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

describe('the Users pages at a phone’s width', () => {
  /**
   * An account whose name has nowhere to break, limited to the one library,
   * whose name and directory have nowhere to break either: the longest words
   * the table and the account's page draw. The make page draws none of them,
   * and is held as it is drawn.
   */
  const UNBROKEN_ACCOUNT = user({
    id: 2,
    name: UNBROKEN_NAME,
    role: 'user',
    every_library: false,
    libraries: [{ id: 70, name: UNBROKEN_NAME }],
  })
  /** Its page, at its name. */
  const UNBROKEN_ACCOUNT_PAGE = `${USERS}/${UNBROKEN_NAME}`

  it.each([
    [USERS, '.user-table .config-table'],
    [NEW_ACCOUNT, '.new-account form.add-user'],
    [UNBROKEN_ACCOUNT_PAGE, '.account-page .grant-list'],
  ])(
    'does not scroll sideways on %s, whose names and path have nowhere to break',
    async (entry, drawn) => {
      await sized(390, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [{ ...libraryOf(UNBROKEN_ROOT), name: UNBROKEN_NAME }],
      })
      vi.mocked(usersApi.listUsers).mockResolvedValue({
        users: [user(), UNBROKEN_ACCOUNT],
      })
      const host = await opened(entry)
      await document.fonts.ready
      // The page asked for, and not the table an unknown address goes to.
      expect(host.querySelector(drawn), drawn).not.toBeNull()
      assertNoSidewaysScroll(host)
    },
  )

  it('keeps the removal’s confirm button inside the page for the same name', async () => {
    await sized(390, 844)
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), UNBROKEN_ACCOUNT],
    })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: UNBROKEN_NAME,
        positions: 0,
        notes: 0,
        sessions: 0,
        keys: 0,
        grants: 1,
      },
      dry_run: true,
    })
    vi.mocked(usersApi.removeUser).mockReturnValue(new Promise(() => {}))
    const host = await opened(UNBROKEN_ACCOUNT_PAGE)
    await press(host.querySelector('button[name="remove"]') as Element)
    await document.fonts.ready
    const confirm = host.querySelector('.remove-user .confirm') as HTMLElement
    expect(confirm.textContent).toBe(`Remove ${UNBROKEN_NAME}`)
    assertNoSidewaysScroll(host)
    // Too wide for Cancel beside it, so Cancel takes the next line.
    expectRemovalFoot(confirm.closest('.remove-user') as HTMLElement, 2)
    // And at its wait's words, in the box it had.
    await drawnWaiting(confirm, 'Removing…', [
      confirm,
      host.querySelector('button[name="cancel-remove"]') as HTMLElement,
    ])
    assertNoSidewaysScroll(host)
  })

  it('stacks each row of the table into a block of two lines, each value after its label', async () => {
    await sized(390, 844)
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({
          sessions: [
            {
              id: 'aaa',
              created: '2026-09-01 10:00:00',
              last_seen: '2026-10-01 10:00:00',
              user_agent: 'Firefox',
              ip: '10.0.0.2',
              current: true,
            },
          ],
        }),
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
        user({
          id: 3,
          name: 'kim',
          role: 'user',
          can_log_in: false,
          every_library: false,
        }),
      ],
    })
    const host = await opened(USERS)
    await document.fonts.ready
    expectStacked(
      host.querySelector('.user-table .config-table') as HTMLElement,
      [
        [
          'davey\u00a0·\u00a0this is you',
          'Role: admin',
          'Libraries: Every library',
          'Last seen: 2026-10-01 10:00:00 UTC',
        ],
        ['sam', 'Role: user', 'Libraries: Manga, Comics', 'Last seen: —'],
        [
          'kim\u00a0·\u00a0locked',
          'Role: user',
          'Libraries: None',
          'Last seen: —',
        ],
      ],
      'kim\u00a0·\u00a0locked',
    )
  })
})

describe('the Configuration screens at 390px', () => {
  it('does not scroll sideways on a library’s page, and keeps both sets of the header’s buttons whole inside it while the name is edited', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    const host = await opened(LIBRARY)
    assertNoSidewaysScroll(host)
    // Rename… · Scan library and Save · Cancel share one slot as wide as the
    // wider set, so both are measured, the unseen one too: at 320px a slot
    // that does not wrap asks for more than the header has in every face.
    // The fallback faces are forced so that the result does not depend on
    // whether the web faces have loaded; the web faces' case is
    // `Libraries.shell.browser.test.tsx`'s at 320.
    withFallbackFonts()
    await page.viewport(320, 844)
    await press(host.querySelector('button[name="rename-library"]') as Element)
    assertNoSidewaysScroll(host)
    const header = host.querySelector(
      '.library-page > .page-header',
    ) as HTMLElement
    const edge = header.getBoundingClientRect().right + 0.5
    const sets = [...header.querySelectorAll('.page-actions-set')]
    expect(sets).toHaveLength(2)
    for (const set of sets) {
      const buttons = [...set.querySelectorAll('button')]
      const words = buttons.map((b) => b.textContent).join(' · ')
      expect(buttons).toHaveLength(2)
      expect(
        buttons
          .filter((b) => b.getBoundingClientRect().right > edge)
          .map((b) => b.textContent),
        words,
      ).toEqual([])
      // Each keeps its words on one line. A set's two buttons fit at 320
      // unwrapped, so what this guards is the header's own line: actions
      // held beside the title rather than wrapped under it are shrunk until
      // their labels break.
      for (const button of buttons) {
        expect(button.getBoundingClientRect().height, words).toBeLessThan(44)
      }
    }
  })

  it('keeps a directory’s own path inside its page when the path has nowhere to break', async () => {
    // `longPathRoot` above breaks at its hyphen and fits. A path with a
    // longer run than the page is wide takes the page's heading past its
    // edge, and the document with it, unless the heading breaks it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    const host = await opened(DIR)
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
    // The path is the page's heading and takes no margin above it.
    const heading = host.querySelector('.dir-page .page-title') as HTMLElement
    expect(heading.textContent).toBe(UNBROKEN_ROOT.path)
    expect(getComputedStyle(heading).marginTop).toBe('0px')
  })

  it('keeps the room of a dry run’s sentence while it is unseen, so that Cancel does not move when it is drawn', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    vi.mocked(config.removalCost).mockReturnValue(new Promise(() => {}))
    const host = await opened(DIR)
    // The wait's timer on a clock the test moves, so that the sentence is
    // read unseen and then drawn with no race against the test's own time.
    holdTheClock()
    await press(buttonSaying(host, 'Remove…'))
    const removal = host.querySelector('.remove-root') as HTMLElement
    const sentence = [...removal.querySelectorAll('p')].find(
      (p) => p.textContent === 'Working out what this would remove…',
    ) as HTMLElement
    const cancel = buttonSaying(removal, 'Cancel')
    const unseenAt = cancel.getBoundingClientRect().top
    // Laid out, so taking its room, and unseen.
    expect(sentence.getBoundingClientRect().height).toBeGreaterThan(0)
    expect(getComputedStyle(sentence).visibility).toBe('hidden')
    pass(WAIT_DRAWN_AFTER_MS)
    expect(getComputedStyle(sentence).visibility).toBe('visible')
    expect(
      Math.abs(cancel.getBoundingClientRect().top - unseenAt),
    ).toBeLessThanOrEqual(0.5)
    expect(sentence.getBoundingClientRect().bottom).toBeLessThanOrEqual(
      cancel.getBoundingClientRect().top,
    )
  })

  it('keeps the removal’s confirm button inside the page for the same path', async () => {
    // Another place the directory's page draws its path: `RemoveRoot`'s confirm
    // button says "Remove " and then the whole of it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    const host = await opened(DIR)
    await act(async () => {
      const open = [...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Remove…',
      )
      open?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await document.fonts.ready
    const confirm = host.querySelector('.remove-root .confirm') as HTMLElement
    expect(confirm.textContent).toBe(`Remove ${UNBROKEN_ROOT.path}`)
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
    // Too wide for Cancel beside it, so Cancel takes the next line.
    expectRemovalFoot(confirm.closest('.remove-root') as HTMLElement, 2)
    // And at its wait's words, in the box it had.
    vi.mocked(config.removeRoot).mockReturnValue(new Promise(() => {}))
    const removal = host.querySelector('.remove-root') as HTMLElement
    await drawnWaiting(confirm, 'Removing…', [
      confirm,
      buttonSaying(removal, 'Cancel'),
    ])
    assertNoSidewaysScroll(host)
    expect(pastItsPage(host)).toEqual([])
  })

  it('draws the removal’s wait in the middle of the confirm the path makes tall', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    const host = await opened(DIR)
    await press(buttonSaying(host, 'Remove…'))
    await document.fonts.ready
    const confirm = host.querySelector('.remove-root .confirm') as HTMLElement
    vi.mocked(config.removeRoot).mockReturnValue(new Promise(() => {}))
    await drawnWaiting(confirm, 'Removing…')
    // The words' own box: the element they are drawn in fills the
    // button's cell.
    const words = document.createRange()
    words.selectNodeContents(
      confirm.querySelector(':scope > span:not([aria-hidden])') as Element,
    )
    const said = words.getBoundingClientRect()
    const button = confirm.getBoundingClientRect()
    // Tall: the unseen words, the path's, run to several lines.
    expect(button.height).toBeGreaterThan(2 * said.height)
    const middle = (from: number, to: number) => (from + to) / 2
    expect(
      Math.abs(
        middle(said.top, said.bottom) - middle(button.top, button.bottom),
      ),
    ).toBeLessThanOrEqual(1)
    expect(
      Math.abs(
        middle(said.left, said.right) - middle(button.left, button.right),
      ),
    ).toBeLessThanOrEqual(1)
  })

  /**
   * The move dialog on a long path and a long library name (the same O180
   * defect): a directory's page in one library, a second library whose name has
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
          dirs: [UNBROKEN_ROOT],
        },
        {
          id: 2,
          name: UNBROKEN_NAME,
          works: 0,
          books: 0,
          dirs: [
            root({
              id: 4,
              path: '/srv/other',
              libraries: [{ id: 2, name: UNBROKEN_NAME }],
            }),
          ],
        },
      ],
    })
    if (asked instanceof Error) {
      vi.mocked(librariesApi.moveCost).mockRejectedValue(asked)
    } else {
      vi.mocked(librariesApi.moveCost).mockResolvedValue(await asked)
    }
    const host = await opened(DIR)
    const dir = host.querySelector('.dir-page') as HTMLElement
    await act(async () => {
      const open = [...dir.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      )
      open?.click()
      await Promise.resolve()
    })
    await act(async () => {
      const box = dir.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '2')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(dir.querySelector('button[name="rehearse"]') as Element)
    await document.fonts.ready
    return { host, dir }
  }

  it('keeps a move’s refusal inside its dialog', async () => {
    const why = `a scan of ${UNBROKEN_ROOT.path} is running; move the directory once it has finished`
    const { host, dir } = await rehearsedMove(
      new Error(
        `/api/library/roots/3/move?dry_run=true: 409 ${JSON.stringify({
          error: why,
          volumes: [],
        })}`,
      ),
    )
    expect(dir.querySelector('.move-root [role="alert"]')?.textContent).toBe(
      why,
    )
    assertNoSidewaysScroll(host)
    expect(pastItsDialog(dir)).toEqual([])
  })

  it('keeps a move’s dialog inside the page for the same path and name, and its confirm and Cancel in their places through the move’s wait', async () => {
    const { host, dir } = await rehearsedMove(
      Promise.resolve({
        moved: {
          library_id: 2,
          library: UNBROKEN_NAME,
          gains_sight: [],
          loses_sight: [],
          library_removed: null,
          grants_removed: 0,
          left_with_no_library: [],
        },
        dry_run: true,
      }),
    )
    const confirm = dir.querySelector('.move-root .confirm') as HTMLElement
    expect(confirm.textContent).toBe('Move')
    assertNoSidewaysScroll(host)
    expect(pastItsDialog(dir)).toEqual([])
    // And at its wait's words, in the box it had, Cancel beside it unmoved.
    vi.mocked(librariesApi.moveDir).mockReturnValue(new Promise(() => {}))
    await drawnWaiting(confirm, 'Moving…', [
      confirm,
      buttonSaying(dir.querySelector('.move-root') as Element, 'Cancel'),
    ])
    assertNoSidewaysScroll(host)
    expect(pastItsDialog(dir)).toEqual([])
  })

  it('draws a move’s confirm as the action: --accent on its border and text, and heavier', async () => {
    const { dir } = await rehearsedMove(
      Promise.resolve({
        moved: {
          library_id: 2,
          library: 'Comics',
          gains_sight: [],
          loses_sight: [],
          library_removed: null,
          grants_removed: 0,
          left_with_no_library: [],
        },
        dry_run: true,
      }),
    )
    const confirm = dir.querySelector('.move-root .confirm') as HTMLElement
    const rehearse = dir.querySelector('button[name="rehearse"]') as HTMLElement
    // The token as the browser resolves it, whichever scheme is in force.
    const probe = document.createElement('span')
    probe.style.color = 'var(--accent)'
    dir.append(probe)
    const accent = getComputedStyle(probe).color
    probe.remove()
    const style = getComputedStyle(confirm)
    expect(style.borderTopColor).toBe(accent)
    expect(style.color).toBe(accent)
    expect(style.fontWeight).toBe('600')
    expect(getComputedStyle(rehearse).borderTopColor).not.toBe(accent)
  })

  /** Makes a library through the make page's form: a name and its directory, submitted. */
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
   * A scan report on a directory's page, with the ancestors the app draws it
   * in: Configuration, the Libraries screen, the directory's page and its
   * Scan report box, inside the shell.
   */
  function onADirPage(children: React.ReactNode) {
    return inShell(
      <div className="config">
        <section className="libraries">
          <div className="dir-page">
            <Section title="Scan report">{children}</Section>
          </div>
        </section>
      </div>,
    )
  }

  /**
   * A scan report where a directory's page draws it, at the level `RootRow`
   * gives it, holding the `unreadable` block and the walk given.
   *
   * **Only its box is around it**, so the report has the width of the box's
   * inside, and a rule's defect shows only where the window is narrow
   * enough: each case's viewport is one at which the defect it names,
   * put back by hand, fails it.
   */
  async function reportOnADirPage(
    unreadable: Unreadable,
    scan: Partial<Walk> = {},
  ) {
    return await settled(
      onADirPage(
        <ScanReport
          outcome={finished({
            scan: walk(scan),
            derive: derived({ unreadable }),
          })}
          level={4}
        />,
      ),
    )
  }

  it("keeps the scan report's list of causes inside its row", async () => {
    // **This is what holds `.scan-report > section > dl`** (O177). As
    // `.scan-report dl`, that rule would reach `UnreadableCauses`'s own nested
    // `<dl>` and make it a grid of one 16rem column whatever it holds, which
    // is wider than the row a 360px window leaves it. A wider window has room
    // for the column, so this case is at 360.
    //
    // **The assertion is on the list itself**: a list that ends in `main`'s
    // padding scrolls nothing.
    //
    // **No cause in this fixture names a path, on purpose**: a path is the
    // other thing that can push this list past its row (O178, held by the
    // cases below), and here the rule is the only thing that can.
    await page.viewport(360, 844)
    const host = await reportOnADirPage({
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
    // directory's page** (O178): a path does not break at a slash, and without
    // the `overflow-wrap` of `.scan-report .paths li, .derived .paths li` the
    // causes that name one end past the page in a 320px window. It asks the page and not only the
    // document, which a list ending in `main`'s padding does not scroll.
    //
    // Fixture copied from `ScanReport.test.tsx`'s "says why each unreadable
    // entry was skipped, cause by cause".
    await page.viewport(320, 844)
    const host = await reportOnADirPage({
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
    // In its box, a report's first heading starts at the box's edge and the
    // second steps off what is above it; each sits near what it heads.
    const headings = [...host.querySelectorAll('.report-heading')]
    expect(headings.map((h) => getComputedStyle(h).marginTop)).toEqual([
      '0px',
      '12px',
    ])
    expect(headings.map((h) => getComputedStyle(h).marginBottom)).toEqual([
      '8px',
      '8px',
    ])
  })

  /** The one file the real library's scan left uncounted: longer than any row here. */
  const LONG_PATH =
    '/home/davey/temp/manga-test/Volumes/Bisque Doll de Chu ♡/Bisque Doll de Chu ♡.cbz'

  /** A path with nowhere to break: no space and no hyphen, and wider than the window. */
  const UNBROKEN_PATH = `/srv/manga/${'a_directory_with_no_break_in_it_'.repeat(2)}/v01.cbz`

  /**
   * Every element in the directory's page whose box ends past the page's edge,
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
    const dirPage = host.querySelector('.dir-page') as HTMLElement
    const edge = dirPage.getBoundingClientRect().right + 0.5
    return [...dirPage.querySelectorAll('*')]
      .filter((el) => el.getBoundingClientRect().right > edge)
      .map((el) => `${el.tagName.toLowerCase()}.${el.className}`)
  }

  /**
   * The same, for the move's dialog on the directory's page and its own
   * edge: it is drawn over the page, so the page's edge does not hold it.
   * It is held open, and from scrolling sideways inside itself, first.
   */
  function pastItsDialog(dirPage: HTMLElement): string[] {
    const dialog = dirPage.querySelector('dialog') as HTMLDialogElement
    expect(dialog?.open).toBe(true)
    expect(dialog.scrollWidth).toBeLessThanOrEqual(dialog.clientWidth)
    const edge = dialog.getBoundingClientRect().right + 0.5
    return [...dialog.querySelectorAll('*')]
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
   * do. Wider than the faces (O177).
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

  it('keeps a path longer than its row inside the directory’s page, in the faces and in their fallbacks', async () => {
    await page.viewport(390, 844)
    const host = await reportOnADirPage(
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
    const host = await reportOnADirPage(
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
    const host = await reportOnADirPage(NOTHING_UNREADABLE, {
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
    // `WalkSection` draws it as plain text in a paragraph on the directory's
    // page.
    await page.viewport(390, 844)
    const host = await reportOnADirPage(NOTHING_UNREADABLE, {
      refused: `${UNBROKEN_ROOT.path} scanned empty while the library holds 12 entries for it — refusing to apply.`,
    })
    await document.fonts.ready
    expect(host.querySelector('.refused')?.textContent).toContain(
      UNBROKEN_ROOT.path,
    )
    assertNoSidewaysScroll(host)
  })

  it('keeps a failed watch probe’s reason inside the directory’s page', async () => {
    // The directory page's `watch` fact is the probe's own sentence once its
    // directory has just been added, and `watch.rs` puts the path in it. The
    // library is made, and its directory's page opened from the library's.
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
          also_in: [],
        },
      ],
    })
    const host = await opened(NEW_LIBRARY)
    await makeLibrary(host, 'Manga', UNBROKEN_ROOT.path)
    await press(host.querySelector('.dir-table .row-link') as Element)
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.dir-page .regime dd')].map(
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
      onADirPage(<ScanReport outcome={{ error: why }} level={4} />),
    )
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.scan-report > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([why])
    assertNoSidewaysScroll(host)
  })

  it('keeps a turned-down “Scan directory”’s sentence inside the window', async () => {
    // The directory page's own alert, for a "Scan directory" the server turned
    // down with a sentence that names the root's path — as its 409 for a
    // scan already running does.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    const why = `/api/library/roots/3/scan: 500 ${UNBROKEN_ROOT.path} could not be scanned`
    vi.mocked(config.startScan).mockRejectedValueOnce(new Error(why))
    const host = await opened(DIR)
    await act(async () => {
      const scan = [
        ...host.querySelectorAll<HTMLButtonElement>('.dir-page button'),
      ].find((b) => b.textContent === 'Scan directory')
      scan?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.dir-page > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([`${UNBROKEN_ROOT.path} could not be scanned`])
    assertNoSidewaysScroll(host)
  })

  it('keeps a long path inside the directory’s page in a removal’s rehearsal, outside any scan report', async () => {
    // What removing the directory would make of the library is drawn in the
    // removal's own `<details>`, which no `.scan-report` holds.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
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
    const host = await opened(DIR)
    await press(
      [...host.querySelectorAll('.dir-page button')].find(
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

  /** The page of a library of one directory, named `name`, in the shell. */
  async function libraryNamed(name: string, dir: librariesApi.LibraryDir) {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ ...holding(dir).libraries[0], name }],
    })
    return await opened(`${TABLE}/${encodeURIComponent(name)}`)
  }

  it('keeps a library whose name has nowhere to break inside the window', async () => {
    // A library is named by a person, and a name need not hold a space. It is
    // drawn as its page's heading and, once Rename… is pressed, in the box
    // laid over that heading.
    await page.viewport(390, 844)
    const host = await libraryNamed(UNBROKEN_NAME, UNBROKEN_ROOT)
    await document.fonts.ready
    const heading = host.querySelector(
      '.library-page .page-title',
    ) as HTMLElement
    expect(heading.textContent).toBe(UNBROKEN_NAME)
    assertNoSidewaysScroll(host)
    expect(heading.scrollWidth).toBeLessThanOrEqual(heading.clientWidth)
    // The heading breaks the name over lines of its own here, and keeps the
    // header that tall while the box stands over its first line, so the
    // Directories beneath do not move as the editor opens.
    const directories = () =>
      (
        [...host.querySelectorAll('.settings-section-title')].find(
          (h) => h.textContent === 'Directories',
        ) as Element
      ).getBoundingClientRect().top
    const before = directories()
    await press(host.querySelector('button[name="rename-library"]') as Element)
    expect(document.activeElement?.getAttribute('name')).toBe('library-name')
    assertNoSidewaysScroll(host)
    expect(directories()).toBeCloseTo(before, 0)
  })

  it('keeps the directory browser inside the page', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: UNBROKEN_ROOT.path,
      parent: '/',
      dirs: [
        {
          name: 'x'.repeat(80),
          path: `${UNBROKEN_ROOT.path}/${'x'.repeat(80)}`,
        },
      ],
    })
    const host = await libraryNamed('Manga', UNBROKEN_ROOT)
    await press(host.querySelector('button[name="add-dir"]') as Element)
    await press(
      host.querySelector(
        '.library-page .add-root button[name="browse"]',
      ) as Element,
    )
    await document.fonts.ready
    const browser = host.querySelector('.dir-browser') as HTMLElement
    const entry = host.querySelector('.dir-list button') as HTMLElement
    expect(entry.textContent).toBe('x'.repeat(80))
    assertNoSidewaysScroll(host)
    expect(entry.getBoundingClientRect().right).toBeLessThanOrEqual(
      browser.getBoundingClientRect().right,
    )
    const where = host.querySelector('.dir-browser .where') as HTMLElement
    expect(where.getBoundingClientRect().right).toBeLessThanOrEqual(
      browser.getBoundingClientRect().right,
    )
    // The box has a line of its own and the two buttons share the one beneath
    // it: neither is left alone on a line (`flex-basis` of the box).
    const buttons = ['pick', 'browse'].map((name) =>
      (
        host.querySelector(
          `.library-page .add-root button[name="${name}"]`,
        ) as HTMLElement
      ).getBoundingClientRect(),
    )
    expect(buttons[1].top).toBe(buttons[0].top)
  })

  it('keeps the directory browser’s own path inside it, whatever paragraph rule is above it', async () => {
    // Under `.libraries` the screen's paragraph rule breaks the path as well,
    // so the library page's case cannot tell whether `.dir-browser .where`
    // does. Mounted bare, only that rule is left to.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: UNBROKEN_ROOT.path,
      parent: '/',
      dirs: [],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<DirPicker action="Add directory" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    await document.fonts.ready
    const where = host.querySelector('.dir-browser .where') as HTMLElement
    expect(where.textContent).toBe(UNBROKEN_ROOT.path)
    const box = host.querySelector('.dir-browser') as HTMLElement
    assertNoSidewaysScroll(host)
    expect(where.getBoundingClientRect().right).toBeLessThanOrEqual(
      box.getBoundingClientRect().right,
    )
  })

  it('keeps Up inside the directory browser when the directory it names has nowhere to break', async () => {
    // Mounted bare for the reason the case above is: no rule above the
    // browser is left to break the name for it.
    await page.viewport(390, 844)
    const outer = 'a_directory_with_no_break_in_it_'.repeat(3)
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: `/srv/manga/${outer}/Trial-a`,
      parent: `/srv/manga/${outer}`,
      dirs: [],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<DirPicker action="Add directory" onPick={() => undefined} />),
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
    const box = host.querySelector('.dir-browser') as HTMLElement
    assertNoSidewaysScroll(host)
    expect(up.getBoundingClientRect().right).toBeLessThanOrEqual(
      box.getBoundingClientRect().right,
    )
  })

  it('holds the directory list still while the next listing is read after a refusal', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: '/srv/manga',
      parent: null,
      dirs: [{ name: 'extra', path: '/srv/manga/extra' }],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<DirPicker action="Add directory" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    vi.mocked(librariesApi.listDirs).mockRejectedValueOnce(
      new Error('/x: 403 {"error":"/srv/manga/extra is not readable"}'),
    )
    await press(host.querySelector('.dir-list button') as Element)
    await document.fonts.ready
    expect(host.querySelector('.dir-browser [role="alert"]')).not.toBeNull()
    const list = () =>
      (host.querySelector('.dir-list') as HTMLElement).getBoundingClientRect()
        .top
    const before = list()
    const held = deferred<librariesApi.DirListing>()
    vi.mocked(librariesApi.listDirs).mockReturnValueOnce(held.promise)
    await press(host.querySelector('.dir-list button') as Element)
    expect(list()).toBe(before)
    await act(async () =>
      held.resolve({
        path: '/srv/manga/extra',
        parent: '/srv/manga',
        dirs: [],
      }),
    )
  })

  it('holds the directory list still while the next listing is read, slow or not', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: '/srv/manga',
      parent: null,
      dirs: [
        { name: 'extra', path: '/srv/manga/extra' },
        { name: 'Volumes', path: '/srv/manga/Volumes' },
      ],
    })
    const host = await act(async () => {
      const mounted = mount(
        inShell(<DirPicker action="Add directory" onPick={() => undefined} />),
      )
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button[name="browse"]') as Element)
    await document.fonts.ready
    const list = () =>
      (host.querySelector('.dir-list') as HTMLElement).getBoundingClientRect()
        .top
    const before = list()
    const held = deferred<librariesApi.DirListing>()
    vi.mocked(librariesApi.listDirs).mockReturnValueOnce(held.promise)
    await press(host.querySelector('.dir-list button') as Element)
    expect(list()).toBe(before)

    await act(
      () =>
        new Promise((done) =>
          setTimeout(done, librariesApi.READING_DELAY_MS + 50),
        ),
    )
    expect(host.querySelector('.dir-browser')?.textContent).toContain(
      'Reading directories…',
    )
    expect(list()).toBe(before)
    await act(async () =>
      held.resolve({
        path: '/srv/manga/extra',
        parent: '/srv/manga',
        dirs: [],
      }),
    )
  })

  it('keeps the library’s removal inside the window', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    const host = await libraryNamed(UNBROKEN_NAME, UNBROKEN_ROOT)
    await press(host.querySelector('button[name="remove-library"]') as Element)
    await document.fonts.ready
    const confirm = host.querySelector(
      '.remove-library .confirm',
    ) as HTMLElement
    const box = host.querySelector('.remove-library') as HTMLElement
    const inside = () => {
      assertNoSidewaysScroll(host)
      expect(confirm.getBoundingClientRect().right).toBeLessThanOrEqual(
        box.getBoundingClientRect().right,
      )
    }
    expect(confirm.textContent).toBe(`Remove ${UNBROKEN_NAME}`)
    inside()
    // Too wide for Cancel beside it, so Cancel takes the next line.
    expectRemovalFoot(box, 2)
    // And at its wait's words, in the box it had.
    vi.mocked(librariesApi.removeLibrary).mockReturnValue(new Promise(() => {}))
    await drawnWaiting(confirm, 'Removing…', [
      confirm,
      buttonSaying(box, 'Cancel'),
    ])
    inside()
    // The confirm is drawn inside the page's removal box, beneath its
    // Remove…, and inside the box's edge as well as the window's.
    const removal = box.closest('.settings-removal .settings-box') as Element
    expect(removal).not.toBeNull()
    const remove = host.querySelector('button[name="remove-library"]')
    expect(remove?.closest('.settings-box')).toBe(removal)
    expect(box.getBoundingClientRect().top).toBeGreaterThanOrEqual(
      (remove as Element).getBoundingClientRect().bottom,
    )
    expect(box.getBoundingClientRect().right).toBeLessThanOrEqual(
      removal.getBoundingClientRect().right,
    )
  })

  it('keeps what a removal’s re-derive found inside the window', async () => {
    // The table draws the report, above the list, once the library that was
    // removed is no longer there to hold it.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
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
      dirs: [root({ id: 5, path: '/srv/comics', libraries: COMICS })],
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
      removed: {
        works: 0,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: {
        works: 0,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
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
    await sized(390, 844)
    // The grant editor draws a library's directories, so the path that must
    // wrap is the one that is on screen.
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [libraryOf(UNBROKEN_ROOT)],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({ every_library: false, libraries: [{ id: 70, name: 'Manga' }] }),
      ],
    })
    const host = await opened(ACCOUNT)
    await document.fonts.ready
    // The account's page, whose grant editor is what draws the path.
    expect(
      host.querySelector('.account-page .grant-dirs code')?.textContent,
    ).toBe(UNBROKEN_ROOT.path)
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
      await sized(390, 844)
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
      const host = await opened(ACCOUNT)
      await document.fonts.ready
      // The Libraries section's one group: the radios, the ticks, and the
      // sentence that stands in for them.
      const group = host.querySelector('.account-page fieldset') as HTMLElement
      expect(group?.textContent).toContain(UNBROKEN_NAME)
      assertNoSidewaysScroll(host)
      expect(group.scrollWidth).toBeLessThanOrEqual(group.clientWidth)
    },
  )

  for (const width of [390, 1280]) {
    it(`draws Backing up… in the box Back up now had, at ${width}`, async () => {
      await sized(width, 844)
      vi.mocked(backupApi.takeBackup).mockReturnValue(new Promise(() => {}))
      const host = await act(async () => {
        const mounted = mount(inShell(<Backup />))
        await Promise.resolve()
        await Promise.resolve()
        return mounted
      })
      const take = host.querySelector('button[name="take"]') as HTMLElement
      expect(take.textContent).toBe('Back up now')
      await drawnWaiting(take, 'Backing up…')
      assertNoSidewaysScroll(host)
    })
  }

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
  it('keeps every control inside the account page that holds it', async () => {
    await sized(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [libraryOf(longPathRoot)],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({ every_library: false, libraries: [{ id: 70, name: 'Manga' }] }),
      ],
    })
    const host = await settled(routed(ACCOUNT))
    const account = host.querySelector('.account-page') as HTMLElement
    expect(account).not.toBeNull()
    // The content box, not the border box: there is no global `border-box`
    // reset, so `max-width` caps the content, and the content box is what a
    // control sits inside.
    const accountStyle = getComputedStyle(account)
    const contentWidth =
      account.getBoundingClientRect().width -
      Number.parseFloat(accountStyle.paddingLeft) -
      Number.parseFloat(accountStyle.paddingRight) -
      Number.parseFloat(accountStyle.borderLeftWidth) -
      Number.parseFloat(accountStyle.borderRightWidth)
    // The measure itself: `.settings-page` sets it, and beside the table the
    // split's page column holds it as well, so either one alone keeps this.
    expect(contentWidth).toBeLessThanOrEqual(40 * 16)
    const controls = account.querySelectorAll('input, select, textarea')
    expect(controls.length).toBeGreaterThan(0)
    for (const control of controls) {
      // A row's control takes its own width (`settings.css`), capped at its
      // column, so the box it is in is what holds it, and the page's
      // measure holds the box.
      const name = control.getAttribute('name')
      const held = control.closest('.settings-box') as Element
      expect(held, name ?? '').not.toBeNull()
      expect(
        control.getBoundingClientRect().right,
        name ?? '',
      ).toBeLessThanOrEqual(held.getBoundingClientRect().right + 0.5)
      expect(control.getBoundingClientRect().width).toBeLessThanOrEqual(
        contentWidth + 0.5,
      )
    }
  })

  it('keeps the add-user form inside its own measure, and Add and Cancel where they were while it waits', async () => {
    await sized(1280, 900)
    const host = await settled(routed(NEW_ACCOUNT))
    const form = host.querySelector('.new-account .add-user') as HTMLElement
    expect(form).not.toBeNull()
    // Content box, for the reason the account page's case gives.
    const formStyle = getComputedStyle(form)
    const formContentWidth =
      form.getBoundingClientRect().width -
      Number.parseFloat(formStyle.paddingLeft) -
      Number.parseFloat(formStyle.paddingRight) -
      Number.parseFloat(formStyle.borderLeftWidth) -
      Number.parseFloat(formStyle.borderRightWidth)
    expect(formContentWidth).toBeLessThanOrEqual(40 * 16)

    vi.mocked(usersApi.addUser).mockReturnValue(new Promise(() => {}))
    await typeInto(
      form.querySelector('input[name="new-name"]') as HTMLInputElement,
      'sam',
    )
    await typeInto(
      form.querySelector('input[name="new-password"]') as HTMLInputElement,
      'hunter2',
    )
    await typeInto(
      form.querySelector('input[name="confirm-password"]') as HTMLInputElement,
      'hunter2',
    )
    const add = form.querySelector('button[type="submit"]') as HTMLElement
    expect(add.textContent).toBe('Add')
    await drawnWaiting(add, 'Creating…', [
      add,
      form.querySelector('button[name="cancel"]') as HTMLElement,
    ])
  })

  describe('New account’s Confirm password', () => {
    const keys = (text: string) =>
      act(async () => {
        await userEvent.keyboard(text)
      })

    /**
     * The make page, at a desktop's width unless asked, alone or `inShell`
     * as it is drawn in the app, and its parts.
     */
    async function makeForm(width = 1280, height = 900, shell = false) {
      await sized(width, height)
      const host = shell
        ? await opened(NEW_ACCOUNT)
        : await settled(routed(NEW_ACCOUNT))
      const form = host.querySelector('.new-account .add-user') as HTMLElement
      const box = (name: string) =>
        form.querySelector(`input[name="${name}"]`) as HTMLInputElement
      return {
        name: box('new-name'),
        password: box('new-password'),
        confirm: box('confirm-password'),
        confirmRow: box('confirm-password').closest(
          '.settings-row',
        ) as HTMLElement,
        role: form.querySelector('select[name="new-role"]') as HTMLElement,
        roleLabel: form.querySelector(
          '.settings-row:has(select[name="new-role"]) label',
        ) as HTMLElement,
        roleHelp: form.querySelector(
          '.settings-row:has(select[name="new-role"]) .settings-row-help',
        ) as HTMLElement,
        add: form.querySelector('button[type="submit"]') as HTMLButtonElement,
        cancel: form.querySelector(
          'button[name="cancel"]',
        ) as HTMLButtonElement,
        form,
        // The status region in the callout drawn after Confirm password's
        // row, laid over Role's select.
        line: form.querySelector(
          ':scope > .callout > [role="status"]',
        ) as HTMLElement,
      }
    }

    it('says the two passwords differ in a callout laid over Role’s select, moving nothing, and holds Add; Enter sends nothing then', async () => {
      vi.mocked(usersApi.addUser).mockReturnValue(new Promise(() => {}))
      const { name, password, confirm, confirmRow, role, add, form, line } =
        await makeForm()
      const callout = line.parentElement as HTMLElement
      expect([
        callout.getBoundingClientRect().height,
        getComputedStyle(line).borderTopStyle,
      ]).toEqual([0, 'none'])

      // A real click gives the frame focus, so the keys reach the page.
      await act(async () => {
        await userEvent.click(name)
      })
      const before = layoutOf(form, callout)
      // Enter while Confirm password is empty, then while it holds the
      // start of the password, presses the held Add, which says so.
      await keys('sam{Tab}hunter22{Enter}')
      expect(line.textContent).toBe('The two passwords do not match.')
      await keys('{Tab}hunter')
      expect(line.textContent).toBe('')
      await keys('{Enter}')
      expect(line.textContent).toBe('The two passwords do not match.')
      await keys('{Backspace}d{Enter}')
      expect(document.activeElement).toBe(confirm)
      expect(line.textContent).toBe('The two passwords do not match.')
      expect(holdOf(add)).toBe('held')
      expect(layoutOf(form, callout)).toEqual(before)
      await expectCallout(line, confirmRow, { over: role })
      expect(usersApi.addUser).not.toHaveBeenCalled()
      // Back to Password and on to Confirm password, the words untouched.
      expect(
        await changesOf(line, () =>
          userEvent.keyboard('{Shift>}{Tab}{/Shift}'),
        ),
      ).toBe(0)
      expect(document.activeElement).toBe(password)
      await keys('{Tab}{End}')

      await keys('{Backspace}{Backspace}er22{Enter}')
      expect(line.textContent).toBe('')
      expect(vi.mocked(usersApi.addUser).mock.calls).toEqual([
        ['sam', 'hunter22', 'user'],
      ])
    })

    for (const [width, height] of [
      [1920, 1080],
      [600, 900],
      [390, 844],
    ] as const) {
      it(`lays the line saying the two passwords differ over Role’s select alone, whole, its words on one line, leaving Role’s label and help, Add and Cancel in view, at ${width}px`, async () => {
        const {
          name,
          confirmRow,
          role,
          roleLabel,
          roleHelp,
          add,
          cancel,
          line,
        } = await makeForm(width, height)
        await act(async () => {
          await userEvent.click(name)
        })
        await keys('sam{Tab}hunter22{Tab}hunted')
        expect(line.textContent).toBe('The two passwords do not match.')
        await expectCallout(line, confirmRow, { over: role })
        expect({
          role: liesOver(line, role),
          label: liesOver(line, roleLabel),
          help: liesOver(line, roleHelp),
          add: liesOver(line, add),
          cancel: liesOver(line, cancel),
        }).toEqual({
          role: true,
          label: false,
          help: false,
          add: false,
          cancel: false,
        })
        const words = document.createRange()
        words.selectNodeContents(line)
        expect(words.getClientRects(), 'its words’ lines').toHaveLength(1)
      })
    }

    for (const [width, rootFont] of [
      [320, ''],
      [390, '20px'],
    ] as const) {
      it(`lays the line saying the two passwords differ over Role’s select and no box, so a press on Confirm password reaches it, moving nothing, at ${width}px with the root’s font at ${rootFont || 'the browser’s own'}`, async () => {
        rootFontAt(rootFont)
        const { name, confirm, confirmRow, role, form, line } = await makeForm(
          width,
          900,
          true,
        )
        const callout = line.parentElement as HTMLElement
        await act(async () => {
          await userEvent.click(name)
        })
        const before = layoutOf(form, callout)
        await keys('sam{Tab}hunter22{Tab}hunted')
        expect(line.textContent).toBe('The two passwords do not match.')
        expect(layoutOf(form, callout), 'what the line moved').toEqual(before)
        act(() => name.focus())
        await expectLaidUnderConfirm(line, confirm, confirmRow, { over: role })
      })
    }

    it('takes a press on the line over Role’s select, so that Role opens nothing and takes no focus, and Tab still reaches it', async () => {
      const { name, confirm, role, line } = await makeForm()
      await act(async () => {
        await userEvent.click(name)
      })
      await keys('sam{Tab}hunter22{Tab}hunted')
      expect(line.textContent).toBe('The two passwords do not match.')
      await expectPressesTaken(line, role)

      await act(async () => {
        await userEvent.click(confirm)
      })
      await keys('{Tab}')
      expect(document.activeElement).toBe(role)
      expect(line.textContent).toBe('The two passwords do not match.')
    })

    it('keeps focus on Add when a fill of Confirm password holds it', async () => {
      const { name, confirm, add } = await makeForm()
      await act(async () => {
        await userEvent.click(name)
      })
      await keys('sam{Tab}hunter22{Tab}hunter22')
      act(() => add.focus())
      expect(document.activeElement).toBe(add)
      // As a password manager fills a box, with focus left where it was.
      await typeInto(confirm, 'hunter2')
      expect(holdOf(add)).toBe('held')
      expect(document.activeElement).toBe(add)
    })
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
    const list = host.querySelector('.backup .backup-rows') as HTMLElement
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
    // Backup's page takes no measure of its own: the list keeps to it only
    // through its section's (`settings.css`), and nothing else in this file
    // watches that on this screen.
    expect(listContentWidth).toBeLessThanOrEqual(40 * 16)
  })

  it('draws a backup’s file name as the tables draw a name, and marks it under the pointer and the keyboard’s focus', async () => {
    await page.viewport(1280, 900)
    vi.mocked(backupApi.listBackups).mockResolvedValue({
      backups: [backupEntry()],
    })
    const host = await opened('/config/backup')
    const link = host.querySelector('.backup-rows a') as HTMLElement
    expect(link.textContent).toBe('library-20260906T143502Z.db')
    // A table's name, drawn by its class alone, beside the backup's.
    const name = document.createElement('a')
    name.className = 'row-link'
    name.href = '#'
    name.textContent = 'sam'
    link.closest('li')?.append(name)
    const look = (el: Element) => {
      const style = getComputedStyle(el)
      return [style.fontWeight, style.textDecorationLine, style.color]
    }
    const table = look(name)
    name.remove()
    const title = host.querySelector('.page-title') as HTMLElement
    await act(async () => {
      await userEvent.hover(title)
    })
    expect(look(link)).toEqual(table)

    const line = () => getComputedStyle(link).textDecorationLine
    await act(async () => {
      await userEvent.hover(link)
    })
    expect(line(), 'under the pointer').toBe('underline')
    await act(async () => {
      await userEvent.hover(title)
    })
    expect(line(), 'the pointer gone').toBe('none')

    // Back up now is the control before it.
    const backUp = buttonSaying(host, 'Back up now')
    backUp.focus()
    await act(async () => {
      await userEvent.tab()
    })
    expect(document.activeElement).toBe(link)
    expect(line(), 'focused from the keyboard').toBe('underline')
    expect(getComputedStyle(link).outlineStyle).toBe('solid')
  })

  it('puts each of a directory’s facts on the header’s line, beside its label', async () => {
    await page.viewport(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    const host = await settled(routed(DIR))
    const facts = [
      ...host.querySelectorAll('.dir-page .page-description .regime > div'),
    ]
    expect(facts.map((fact) => fact.querySelector('dt')?.textContent)).toEqual([
      'watch',
      'scan every',
      'last scan',
    ])
    // Each `<div>` lays its dt and dd out on one line, the value after its
    // label: as blocks, each fact would stand on two lines, its value under
    // its label.
    for (const fact of facts) {
      const dt = (fact.querySelector('dt') as Element).getBoundingClientRect()
      const dd = (fact.querySelector('dd') as Element).getBoundingClientRect()
      const label = fact.querySelector('dt')?.textContent ?? ''
      expect(dd.top, label).toBeCloseTo(dt.top, 0)
      expect(dd.left, label).toBeGreaterThan(dt.right - 1)
    }
  })

  it('puts the line of the directory it sits in and the fold of those inside it under the facts, a step apart and from their start; the fold opens to aligned rows, by a real click and by Enter, and keeps the page from scrolling sideways at a phone’s width', async () => {
    const between: librariesApi.LibraryDir = {
      ...longPathRoot,
      inside: [
        {
          path: '/home/davey/temp/manga-test',
          libraries: ['Everything'],
          shows: true,
          kept_out_by: null,
        },
      ],
      nested: [
        {
          id: 9,
          path: '/home/davey/temp/manga-test/Volumes/Kids in a long directory name',
          libraries: ['Kids'],
          shown: false,
          kept_out_by: 'exclusion',
        },
        {
          id: 10,
          path: '/home/davey/temp/manga-test/Volumes/old',
          libraries: ['Archive'],
          shown: true,
          kept_out_by: null,
        },
      ],
    }
    for (const [width, height] of [
      [1280, 900],
      [390, 844],
    ]) {
      await page.viewport(width, height)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(between))
      const host = await opened(DIR)
      const facts = (
        host.querySelector('.dir-page .page-description > .regime') as Element
      ).getBoundingClientRect()
      const nesting = host.querySelector(
        '.dir-page .page-description > .nesting',
      ) as HTMLElement
      const parts = [...nesting.children].map((part) =>
        part.getBoundingClientRect(),
      )
      expect(
        [...nesting.children].map((part) => part.tagName),
        `${width}`,
      ).toEqual(['P', 'DETAILS'])
      // A step under what is above it, each starting where the facts do.
      let above = facts.bottom
      for (const part of parts) {
        expect(part.top - above, `${width}`).toBeCloseTo(4, 0)
        expect(part.left, `${width}`).toBeCloseTo(facts.left, 0)
        above = part.bottom
      }

      const summary = nesting.querySelector('details > summary') as HTMLElement
      await expectDisclosure(summary, `the folders inside, at ${width}`)
      // Each column starts where the row above's does.
      const rows = [...nesting.querySelectorAll('details li')].map((row) =>
        [...row.children].map((cell) => cell.getBoundingClientRect().left),
      )
      expect(rows, `${width}`).toHaveLength(2)
      for (const [column, left] of rows[0].entries()) {
        expect(rows[1][column], `${width}, column ${column}`).toBeCloseTo(
          left,
          0,
        )
      }
      assertNoSidewaysScroll(host)

      // The click left focus on the summary, and Enter folds it again.
      expect(document.activeElement, `${width}`).toBe(summary)
      await act(async () => {
        await userEvent.keyboard('{Enter}')
      })
      expect((summary.parentElement as HTMLDetailsElement).open).toBe(false)
      act(() => rerender(<div />))
    }
  })
  it('draws the library page’s scan lines alike, muted at the UI size, its own scan’s and another library’s, and keeps both within the page at a phone’s width', async () => {
    const outer: librariesApi.LibraryDir = {
      ...longPathRoot,
      nested: [
        {
          id: 9,
          path: '/home/davey/temp/manga-test/Volumes/Kids in a long directory name/weekly',
          libraries: ['Kids'],
          shown: true,
          kept_out_by: null,
        },
      ],
    }
    const status = (own: boolean): librariesApi.LibraryScanStatus => ({
      library_id: 1,
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 3, running: own },
        { root_id: 9, running: true },
      ],
      held_removals: own ? [] : [3],
      moves_held: false,
      scans: own
        ? [{ library: 'Manga', root_ids: [3, 9] }]
        : [{ library: 'Kids', root_ids: [9] }],
    })
    for (const own of [false, true]) {
      for (const [width, height] of [
        [1280, 900],
        [390, 844],
      ]) {
        const at = `${own ? 'own' : 'Kids'}, ${width}`
        await page.viewport(width, height)
        vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(outer))
        vi.mocked(librariesApi.libraryScanStatus).mockResolvedValue(status(own))
        const host = await opened(LIBRARY)
        const line = host.querySelector(
          '.library-page > .scanning',
        ) as HTMLElement
        expect(line?.textContent, at).toMatch(
          own
            ? /^A scan is running in this library…/
            : /^Kids is scanning Kids in a long directory name\/weekly\/, which Manga shows\.$/,
        )
        const muted = getComputedStyle(
          host.querySelector('.library-page .page-description') as Element,
        )
        const drawn = getComputedStyle(line)
        expect(drawn.color, at).toBe(muted.color)
        expect(drawn.fontSize, at).toBe(muted.fontSize)
        const main = (
          host.querySelector('main') as HTMLElement
        ).getBoundingClientRect()
        expect(line.getBoundingClientRect().right, at).toBeLessThanOrEqual(
          main.right,
        )
        assertNoSidewaysScroll(host)
        act(() => rerender(<div />))
      }
    }
  })
})

describe('a library page’s lines for a directory another library holds too', () => {
  it('draws another library’s scan of a directory it holds too as its other scan lines are, muted at the UI size, and keeps it within the page at a phone’s width', async () => {
    const shared = root({
      ...longPathRoot,
      libraries: [
        { id: 1, name: 'Manga' },
        { id: 2, name: 'Kids' },
      ],
    })
    for (const [width, height] of [
      [1280, 900],
      [390, 844],
    ]) {
      await page.viewport(width, height)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(shared))
      vi.mocked(librariesApi.libraryScanStatus).mockResolvedValue({
        library_id: 1,
        running: true,
        elapsed_secs: 3,
        dirs: [{ root_id: 3, running: true }],
        held_removals: [],
        moves_held: false,
        scans: [{ library: 'Kids', root_ids: [3] }],
      })
      const host = await opened(LIBRARY)
      const line = host.querySelector(
        '.library-page > .scanning',
      ) as HTMLElement
      expect(line?.textContent, `${width}`).toBe(
        'Kids is scanning /home/davey/temp/manga-test/Volumes, which Manga holds too.',
      )
      const muted = getComputedStyle(
        host.querySelector('.library-page .page-description') as Element,
      )
      const drawn = getComputedStyle(line)
      expect(drawn.color, `${width}`).toBe(muted.color)
      expect(drawn.fontSize, `${width}`).toBe(muted.fontSize)
      const main = (
        host.querySelector('main') as HTMLElement
      ).getBoundingClientRect()
      expect(
        line.getBoundingClientRect().right,
        `${width}`,
      ).toBeLessThanOrEqual(main.right)
      assertNoSidewaysScroll(host)
      act(() => rerender(<div />))
    }
  })

  /**
   * Manga's page at `width`, a link of `path` from `also_in` answered as
   * the picker, its browser open if `browse`, is pressed: `open` is how far
   * below the top of the add form what is below it was with the picker
   * open, and `reach` says it now.
   */
  async function linkedAt(
    width: number,
    height: number,
    path: string,
    also_in: string[],
    browse = false,
  ) {
    await page.viewport(width, height)
    const linked = root({
      id: 4,
      path,
      libraries: [
        { id: 1, name: 'Manga' },
        { id: 2, name: also_in[0] },
      ],
    })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: null,
      parent: null,
      dirs: [
        { name: 'manga', path: '/srv/manga' },
        { name: 'comics', path: '/srv/comics' },
      ],
    })
    const sent = deferred<config.Added>()
    vi.mocked(librariesApi.addDir).mockReturnValue(sent.promise)
    const host = await opened(LIBRARY)
    const addRoot = () =>
      host.querySelector('.library-page .add-root') as HTMLElement
    await pressOn(addRoot().querySelector('button[name="add-dir"]'))
    if (browse) {
      await pressOn(addRoot().querySelector('button[name="browse"]'))
      expect(addRoot().querySelector('.dir-browser .dir-list')).not.toBeNull()
    }
    await typeInto(
      addRoot().querySelector('input[name="path"]') as HTMLInputElement,
      path,
    )
    await pressOn(addRoot().querySelector('button[name="pick"]'))
    await document.fonts.ready
    const removal = () =>
      host.querySelector('.library-page .settings-removal') as HTMLElement
    /** From the top of the add form to the top of what is below it. */
    const reach = () =>
      removal().getBoundingClientRect().top -
      addRoot().getBoundingClientRect().top
    const open = reach()
    expect(addRoot().querySelector('.dir-picker')).not.toBeNull()
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot, linked),
    )
    await act(async () => {
      sent.resolve({
        root: linked,
        watching: null,
        watch_reason: null,
        interval_secs: null,
        also_in,
      })
      await new Promise((resolve) => setTimeout(resolve, 120))
    })
    expect(addRoot().querySelector('.dir-picker')).toBeNull()
    const line = addRoot().querySelector('.linked') as HTMLElement
    return { host, addRoot, line, open, reach }
  }

  /** The live server's own path, and one of seventy characters. */
  const PATHS = [
    '/home/davey/temp/manga-test/Kids',
    '/home/davey/temp/manga-test/Collections/Shonen Jump/Weekly/Extras/Kids',
  ]
  const SIZES = [
    [1280, 900],
    [390, 844],
  ]
  const EACH = SIZES.flatMap(([width, height]) =>
    PATHS.map((path) => [width, height, path] as const),
  )

  /**
   * Where a link's line lies in its box: `foot`, how far its foot is above
   * the foot of the box's content, which is its last row's, inside that
   * row's own padding; `clear`, how far above the Removal section's heading;
   * `covers`, the box's rows and Add directory… that it lies over, and
   * `halves`, those it covers only in part; and `onTop`, whether it is what
   * is drawn at its top and at its foot.
   */
  function linePlace(host: HTMLElement, line: HTMLElement) {
    const box = line.closest('.settings-box') as HTMLElement
    const last = box.lastElementChild as HTMLElement
    const contentFoot =
      last.getBoundingClientRect().bottom -
      Number.parseFloat(getComputedStyle(last).paddingBottom)
    const heading = host.querySelector(
      '.library-page .settings-removal .settings-section-title',
    ) as HTMLElement
    const at = line.getBoundingClientRect()
    const under = [
      ...box.querySelectorAll<HTMLElement>('tr, .add-root > button'),
    ].filter(
      (row) => row.getBoundingClientRect().height > 0 && liesOver(line, row),
    )
    const halves = under
      .filter((row) => {
        const r = row.getBoundingClientRect()
        return !(r.top >= at.top - 0.5 && r.bottom <= at.bottom + 0.5)
      })
      .map((row) => row.textContent)
    const drawnAt = (y: number) => {
      const hit = document.elementFromPoint((at.left + at.right) / 2, y)
      return hit !== null && line.contains(hit)
    }
    return {
      foot: contentFoot - at.bottom,
      clear: heading.getBoundingClientRect().top - at.bottom,
      covers: under.map((row) => row.textContent),
      halves,
      onTop: drawnAt(at.top + 4) && drawnAt(at.bottom - 4),
    }
  }

  it.each([
    [1280, 900],
    [390, 844],
  ])(
    'says a link with Add directory…, in the room the picker took, and moves nothing below it, at %ipx',
    async (width, height) => {
      const { host, addRoot, line, open, reach } = await linkedAt(
        width,
        height,
        '/srv/manga/Kids',
        ['Kids'],
      )
      expect(line.textContent).toBe(
        '/srv/manga/Kids is also a directory of Kids. It keeps the settings it has there.',
      )
      expect(reach()).toBeCloseTo(open, 0)
      const room = addRoot().getBoundingClientRect()
      const at = line.getBoundingClientRect()
      expect(at.top).toBeGreaterThanOrEqual(room.top - 0.5)
      expect(at.bottom).toBeLessThanOrEqual(room.bottom + 0.5)
      assertNoSidewaysScroll(host)
    },
  )

  it.each(EACH)(
    'says a link of two libraries at %ipx×%ipx, of %s, inside its box, moving nothing below it: where its words need more than the picker’s room, up over what is above it, each row whole',
    async (width, height, path) => {
      const { host, line, open, reach } = await linkedAt(width, height, path, [
        'Kids',
        'Comics',
      ])
      expect(line.textContent).toBe(
        `${path} is also a directory of Kids and Comics. It keeps the settings it has there.`,
      )
      expect(reach()).toBeCloseTo(open, 0)
      const place = linePlace(host, line)
      expect(place.foot).toBeGreaterThanOrEqual(-0.5)
      expect(place.clear).toBeGreaterThan(0)
      expect(place.halves).toEqual([])
      expect(place.onTop).toBe(true)
      assertNoSidewaysScroll(host)
    },
  )

  it.each(['mouse', 'tap'] as const)(
    'takes a link’s line away at a real %s where it lies over a control of its box, pressing nothing beneath, and the next press there reaches the control beneath, at 390px',
    async (input) => {
      const { line } = await linkedAt(
        390,
        844,
        '/home/davey/temp/manga-test/Kids',
        ['Kids', 'Comics'],
      )
      const box = line.closest('.settings-box') as HTMLElement
      const beneath = [
        ...box.querySelectorAll<HTMLElement>('button, a, input, select'),
      ].find((control) => liesOver(line, control))
      expect(beneath, 'a control of the box under the line').toBeDefined()
      const control = beneath as HTMLElement
      let pressed = 0
      control.addEventListener('click', () => {
        pressed += 1
      })
      const { x, y } = middleWhereOver(line, control)
      // The press lands on the line, over the control, and not on the
      // control itself.
      const first = document.elementFromPoint(x, y)
      expect(first !== null && line.contains(first)).toBe(true)
      if (input === 'tap') await tapAt(x, y)
      else await pressAt(x, y)
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 120))
      })
      expect(line.textContent).toBe('')
      expect(pressed).toBe(0)
      const next = document.elementFromPoint(x, y)
      expect(next !== null && control.contains(next)).toBe(true)
      await pressAt(x, y)
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 120))
      })
      expect(pressed).toBe(1)
    },
  )

  it('keeps the room it measured as the link was said when a phone turns to portrait, moving nothing below it', async () => {
    const { host, addRoot, line, open, reach } = await linkedAt(
      844,
      390,
      '/home/davey/temp/manga-test/Kids',
      ['Kids', 'Comics'],
    )
    const kept = addRoot().getBoundingClientRect().height
    expect(reach()).toBeCloseTo(open, 0)
    await page.viewport(390, 844)
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 120))
    })
    expect(addRoot().getBoundingClientRect().height).toBeCloseTo(kept, 0)
    expect(reach()).toBeCloseTo(open, 0)
    // Its words reflowed with no render of the page, and it is laid inside
    // its box again.
    const place = linePlace(host, line)
    expect(place.foot).toBeGreaterThanOrEqual(-0.5)
    expect(place.halves).toEqual([])
    expect(place.onTop).toBe(true)
  })

  it('keeps the whole room of a picker whose directory browser was open as the link was said, moving nothing below it', async () => {
    const { line, open, reach } = await linkedAt(
      1280,
      900,
      '/srv/manga/Kids',
      ['Kids'],
      true,
    )
    expect(line.textContent).toContain('is also a directory of Kids')
    expect(reach()).toBeCloseTo(open, 0)
  })

  /**
   * The page of Extras, made at `width` over `path`, which Kids and Comics
   * hold, as the app draws it: the make is sent outside `act`, which would
   * draw only the last of its changes. `seen` is how far below the add form
   * the Removal section was, and what the line said, at each change to the
   * page from the make on.
   */
  async function madeAt(width: number, height: number, path: string) {
    await page.viewport(width, height)
    const made: librariesApi.Library = {
      ...holding(root({ id: 4, path })).libraries[0],
      id: 5,
      name: 'Extras',
    }
    vi.mocked(librariesApi.createLibrary).mockResolvedValueOnce({
      library: made,
      added: [
        {
          root: made.dirs[0],
          watching: null,
          watch_reason: null,
          interval_secs: null,
          also_in: ['Kids', 'Comics'],
        },
      ],
    })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    const host = await opened(NEW_LIBRARY)
    await typeInto(
      host.querySelector('input[name="library-name"]') as HTMLInputElement,
      'Extras',
    )
    await typeInto(
      host.querySelector('input[name="path"]') as HTMLInputElement,
      path,
    )
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [...holding(longPathRoot).libraries, made],
    })
    await document.fonts.ready
    const reach = () => {
      const addRoot = host.querySelector('.library-page .add-root')
      const removal = host.querySelector('.library-page .settings-removal')
      return addRoot === null || removal === null
        ? undefined
        : removal.getBoundingClientRect().top -
            addRoot.getBoundingClientRect().top
    }
    const seen: { reach: number; said: string }[] = []
    const watcher = new MutationObserver(() => {
      const now = reach()
      if (now === undefined) return
      seen.push({
        reach: now,
        said:
          host.querySelector('.library-page .add-root > .linked')
            ?.textContent ?? '',
      })
    })
    watcher.observe(host, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
    })
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      ;(host.querySelector('form.add-library') as Element).dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      const until = performance.now() + 3000
      while (
        (host.querySelector('.library-page .add-root > .linked')?.textContent ??
          '') === '' &&
        performance.now() < until
      ) {
        await new Promise((resolve) => setTimeout(resolve, 20))
      }
      await new Promise((resolve) => setTimeout(resolve, 120))
    } finally {
      watcher.disconnect()
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    const line = host.querySelector(
      '.library-page .add-root > .linked',
    ) as HTMLElement
    return { host, line, seen, reach: () => reach() ?? Number.NaN }
  }

  it.each(EACH)(
    'says a link New library made on the made library’s page at %ipx×%ipx, of %s, in a room the page has from its first paint, inside its box, moving nothing below it as the line says it',
    async (width, height, path) => {
      const { host, line, seen } = await madeAt(width, height, path)
      expect(line.textContent).toBe(
        `${path} is also a directory of Kids and Comics. It keeps the settings it has there.`,
      )
      // At every change the page saw, the Removal section was where it is.
      expect(seen.length).toBeGreaterThan(0)
      for (const one of seen) {
        expect(one.reach, `${one.said === '' ? 'empty' : 'said'}`).toBeCloseTo(
          seen[0].reach,
          0,
        )
      }
      const place = linePlace(host, line)
      // In the room the page had from its first paint, over nothing.
      expect(place.covers).toEqual([])
      expect(place.foot).toBeGreaterThanOrEqual(-0.5)
      expect(place.clear).toBeGreaterThan(0)
      expect(place.halves).toEqual([])
      expect(place.onTop).toBe(true)
      assertNoSidewaysScroll(host)
    },
  )

  it.each([
    [390, 1280],
    [1280, 390],
  ])(
    'keeps the made library’s page’s line inside its box, clear of the Removal heading, and the room it had, as the window goes from %ipx to %ipx and back',
    async (from, to) => {
      const sizes = { 390: [390, 844], 1280: [1280, 900] } as const
      const [width, height] = sizes[from as 390 | 1280]
      const { host, line, reach } = await madeAt(width, height, PATHS[1])
      const kept = reach()
      for (const [step, [w, h]] of [
        sizes[to as 390 | 1280],
        sizes[from as 390 | 1280],
      ].entries()) {
        await page.viewport(w, h)
        await act(async () => {
          await new Promise((resolve) => setTimeout(resolve, 120))
        })
        const place = linePlace(host, line)
        const at = `step ${step}, ${w}px`
        expect(reach(), at).toBeCloseTo(kept, 0)
        expect(place.foot, at).toBeGreaterThanOrEqual(-0.5)
        expect(place.clear, at).toBeGreaterThan(0)
        expect(place.halves, at).toEqual([])
        expect(place.onTop, at).toBe(true)
      }
    },
  )

  it('lays a link’s line too tall for the room under Add directory… over the rows above that room instead, each whole, the button in view, at 390px, moving nothing below it', async () => {
    const { host, addRoot, line, open, reach } = await linkedAt(
      390,
      844,
      PATHS[1],
      ['Kids', 'Comics', 'Teens', 'Light novels'],
    )
    const room = addRoot()
    const roomAt = room.getBoundingClientRect()
    const padding = getComputedStyle(room)
    const roomHeight =
      roomAt.height -
      Number.parseFloat(padding.paddingTop) -
      Number.parseFloat(padding.paddingBottom)
    const at = line.getBoundingClientRect()
    // Its words need more than the room.
    expect(at.height).toBeGreaterThan(roomHeight)
    const add = room.querySelector('button[name="add-dir"]') as HTMLElement
    expect(liesOver(line, add)).toBe(false)
    const middle = add.getBoundingClientRect()
    expect(
      document.elementFromPoint(
        (middle.left + middle.right) / 2,
        (middle.top + middle.bottom) / 2,
      ),
    ).toBe(add)
    expect(at.bottom).toBeLessThanOrEqual(roomAt.top + 0.5)
    const rows = [
      ...host.querySelectorAll<HTMLElement>('.library-page .dir-table tr'),
    ].filter((row) => row.getBoundingClientRect().height > 0)
    expect(at.top).toBeGreaterThanOrEqual(
      Math.min(...rows.map((row) => row.getBoundingClientRect().top)) - 0.5,
    )
    const place = linePlace(host, line)
    expect(place.covers.length).toBeGreaterThan(0)
    expect(place.halves).toEqual([])
    expect(place.clear).toBeGreaterThan(0)
    expect(place.onTop).toBe(true)
    expect(reach()).toBeCloseTo(open, 0)
  })

  it('keeps a link’s line too tall for its room beside Add directory… at 1280px, laid up from the room’s foot over no row of the table', async () => {
    const { host, addRoot, line, open, reach } = await linkedAt(
      1280,
      900,
      PATHS[1],
      ['Kids', 'Comics', 'Teens', 'Light novels'],
    )
    const add = addRoot().querySelector('button[name="add-dir"]') as HTMLElement
    expect(liesOver(line, add)).toBe(false)
    const place = linePlace(host, line)
    expect(place.covers).toEqual([])
    expect(place.foot).toBeGreaterThanOrEqual(-0.5)
    expect(place.clear).toBeGreaterThan(0)
    expect(place.onTop).toBe(true)
    expect(reach()).toBeCloseTo(open, 0)
  })

  it('lays a link’s line too tall for the rows above its room up from the room’s foot over Add directory… instead, inside its box, at 390px', async () => {
    const holders = Array.from(
      { length: 16 },
      (_, i) => `Collection ${String.fromCharCode(65 + i)}`,
    )
    const { host, addRoot, line, open, reach } = await linkedAt(
      390,
      844,
      PATHS[1],
      holders,
    )
    const add = addRoot().querySelector('button[name="add-dir"]') as HTMLElement
    const rows = [
      ...host.querySelectorAll<HTMLElement>('.library-page .dir-table tr'),
    ].filter((row) => row.getBoundingClientRect().height > 0)
    const first = Math.min(
      ...rows.map((row) => row.getBoundingClientRect().top),
    )
    const at = line.getBoundingClientRect()
    // More words than the rows above the room have room for.
    expect(at.height).toBeGreaterThan(
      addRoot().getBoundingClientRect().top - first,
    )
    expect(at.top).toBeGreaterThanOrEqual(first - 0.5)
    expect(liesOver(line, add)).toBe(true)
    const place = linePlace(host, line)
    expect(place.foot).toBeGreaterThanOrEqual(-0.5)
    expect(place.clear).toBeGreaterThan(0)
    expect(place.halves).toEqual([])
    expect(place.onTop).toBe(true)
    expect(reach()).toBeCloseTo(open, 0)
  })

  it('takes the focus ring off a link’s line as focus leaves Add directory… beneath it: by a real Tab onward, and by a real click on nothing that takes focus, at 390px', async () => {
    const { host, addRoot, line } = await linkedAt(390, 844, PATHS[0], [
      'Kids',
      'Comics',
    ])
    const add = addRoot().querySelector('button[name="add-dir"]') as HTMLElement
    const rows = host.querySelectorAll<HTMLElement>(
      '.library-page .dir-table .row-link',
    )
    await act(async () => rows[rows.length - 1].focus())
    const ringed = () => ({
      marked: line.hasAttribute('data-over-focus'),
      outline: getComputedStyle(line).outlineStyle,
    })
    await act(async () => {
      await userEvent.tab()
    })
    expect(document.activeElement).toBe(add)
    expect(ringed()).toEqual({ marked: true, outline: 'solid' })

    await act(async () => {
      await userEvent.tab()
    })
    expect(document.activeElement).not.toBe(add)
    expect(ringed()).toEqual({ marked: false, outline: 'none' })

    await act(async () => {
      await userEvent.tab({ shift: true })
    })
    expect(document.activeElement).toBe(add)
    expect(ringed()).toEqual({ marked: true, outline: 'solid' })

    const nothing = host.querySelector(
      '.library-page .page-description',
    ) as HTMLElement
    await act(async () => {
      await userEvent.click(nothing)
    })
    expect(document.activeElement).toBe(document.body)
    expect(ringed()).toEqual({ marked: false, outline: 'none' })
  })

  it('draws the focus ring on a link’s line where a real Tab reaches Add directory… beneath it at 390px, and a real Enter there still opens the picker', async () => {
    const { host, addRoot, line } = await linkedAt(390, 844, PATHS[0], [
      'Kids',
      'Comics',
    ])
    const add = addRoot().querySelector('button[name="add-dir"]') as HTMLElement
    expect(liesOver(line, add)).toBe(true)
    const rows = host.querySelectorAll<HTMLElement>(
      '.library-page .dir-table .row-link',
    )
    const before = rows[rows.length - 1]
    expect(liesOver(line, before)).toBe(false)
    // Focused inside the frame, so that the keys go to the page.
    await act(async () => before.focus())
    expect(line.hasAttribute('data-over-focus')).toBe(false)
    expect(getComputedStyle(line).outlineStyle).toBe('none')

    await act(async () => {
      await userEvent.tab()
    })
    expect(document.activeElement).toBe(add)
    expect(line.hasAttribute('data-over-focus')).toBe(true)
    const ring = getComputedStyle(line)
    expect(ring.outlineStyle).toBe('solid')
    expect(ring.outlineWidth).toBe('2px')
    expect(getComputedStyle(add).outlineStyle).toBe('none')

    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(addRoot().querySelector('.dir-picker')).not.toBeNull()
    expect(document.activeElement).toBe(
      addRoot().querySelector('input[name="path"]'),
    )
  })
})

describe('the Users pages’ own lines', () => {
  const sam = user({ id: 2, name: 'sam', role: 'user' })
  const removal = {
    removed: {
      name: 'sam',
      positions: 0,
      notes: 0,
      sessions: 0,
      keys: 0,
      grants: 0,
    },
  }

  it('puts the role’s hint a step under the Role label, beside the box it explains', async () => {
    await sized(1280, 900)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user()] })
    const host = await opened(ACCOUNT)
    const role = host.querySelector('select[name="role"]') as HTMLElement
    const row = role.closest('.settings-row') as HTMLElement
    const label = row.querySelector('.settings-row-label') as HTMLElement
    const hint = row.querySelector('.settings-row-help') as HTMLElement
    expect(label.textContent).toBe('Role')
    expect(hint?.textContent).toContain('cannot take away your own admin role')
    // The row's own step, not a paragraph's margins.
    expect(getComputedStyle(hint).marginTop).toBe('4px')
    expect(getComputedStyle(hint).marginBottom).toBe('0px')
    const [at, under, box] = [hint, label, role].map((el) =>
      el.getBoundingClientRect(),
    )
    expect(at.top).toBeCloseTo(under.bottom + 4, 0)
    expect(at.right).toBeLessThanOrEqual(box.left)
  })

  it('says an account was removed quietly, on the table it goes back to', async () => {
    await sized(1280, 900)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      ...removal,
      dry_run: true,
    })
    vi.mocked(usersApi.removeUser).mockResolvedValue({
      ...removal,
      dry_run: false,
    })
    const host = await opened(`${USERS}/sam`)
    await press(host.querySelector('button[name="remove"]') as Element)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user()] })
    await press(host.querySelector('.remove-user .confirm') as Element)
    await flush()
    const line = host.querySelector('.user-table > p.gone-line') as HTMLElement
    expect(line?.textContent).toBe('Removed sam.')
    expect(getComputedStyle(line).color).toBe(
      tokenColour(line.parentElement as Element, '--muted'),
    )
  })

  it('says a password was set in the page’s own colour', async () => {
    await sized(1280, 900)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    vi.mocked(usersApi.patchUser).mockResolvedValue(sam)
    const host = await opened(`${USERS}/sam`)
    await typeInto(
      host.querySelector('input[name="password"]') as HTMLInputElement,
      'hunter22',
    )
    await typeInto(
      host.querySelector('input[name="confirm-password"]') as HTMLInputElement,
      'hunter22',
    )
    await submit(host.querySelector('form.reset') as Element)
    await flush()
    const line = host.querySelector('.reset .done') as HTMLElement
    expect(line?.textContent).toBe(
      'Password set. sam is signed out everywhere.',
    )
    expect(getComputedStyle(line).color).toBe(
      tokenColour(line.parentElement as Element, '--text'),
    )
  })

  it('draws the password’s live region with no room and no line until a set works, and then as a line of its form', async () => {
    await sized(1280, 900)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    vi.mocked(usersApi.patchUser).mockResolvedValue(sam)
    const host = await opened(`${USERS}/sam`)
    const region = host.querySelector(
      '.reset > [aria-live="polite"]',
    ) as HTMLElement
    const empty = getComputedStyle(region)
    expect([
      region.getBoundingClientRect().height,
      empty.borderTopStyle,
    ]).toEqual([0, 'none'])

    await typeInto(
      host.querySelector('input[name="password"]') as HTMLInputElement,
      'hunter22',
    )
    await typeInto(
      host.querySelector('input[name="confirm-password"]') as HTMLInputElement,
      'hunter22',
    )
    await submit(host.querySelector('form.reset') as Element)
    await flush()
    expect(region.textContent).toBe(
      'Password set. sam is signed out everywhere.',
    )
    const drawn = getComputedStyle(region)
    expect([drawn.borderTopWidth, drawn.borderTopStyle]).toEqual([
      '1px',
      'solid',
    ])
    expect(drawn.borderTopColor).toBe(
      tokenColour(region.parentElement as Element, '--line'),
    )
    expect(parseFloat(drawn.paddingTop)).toBeGreaterThan(0)
    expect(drawn.paddingBottom).toBe(drawn.paddingTop)
  })

  for (const [width, height] of [
    [1920, 1080],
    [1280, 900],
    [390, 844],
  ] as const) {
    it(`draws the callout saying the two passwords differ under Confirm password’s row, over Set password’s, covering Set password whole and moving nothing, at ${width}px`, async () => {
      await sized(width, height)
      vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
      const host = await opened(`${USERS}/sam`)
      const form = host.querySelector('form.reset') as HTMLElement
      const parts = ['password', 'confirm-password'].map(
        (name) =>
          form.querySelector(`input[name="${name}"]`) as HTMLInputElement,
      )
      const set = form.querySelector('button[name="reset"]') as HTMLElement
      const line = form.querySelector(
        ':scope > .callout > [role="status"]',
      ) as HTMLElement
      const callout = line.parentElement as HTMLElement
      expect([
        callout.getBoundingClientRect().height,
        getComputedStyle(line).borderTopStyle,
      ]).toEqual([0, 'none'])
      const page = host.querySelector('.account-page') as HTMLElement
      await typeInto(parts[0], 'hunter22')
      // In the middle of the view, so the callout under it is in view too,
      // and focused first: neither is a change of layout.
      parts[1].scrollIntoView({ block: 'center' })
      act(() => parts[1].focus())
      const before = layoutOf(page, callout)

      await typeInto(parts[1], 'hunted')
      expect(line.textContent).toBe('The two passwords do not match.')
      expect(layoutOf(page, callout)).toEqual(before)
      await expectCallout(line, parts[1].closest('.settings-row') as Element)
      expect(liesOver(line, set)).toBe(true)
    })
  }

  /**
   * The line lies under Confirm password's row and over no box, its words
   * on one line or two (the user's ruling of 2026-10-26): where they take
   * two, Set password's row keeps their room whether the line is said or
   * not, so nothing moves when it comes. Where they take one, that row
   * keeps no room beyond Set password's.
   */
  for (const [width, rootFont] of [
    [320, ''],
    [390, '20px'],
    [390, ''],
    [1920, ''],
  ] as const) {
    it(`lays the line saying the two passwords differ under Confirm password’s row and over no box, so a press on Confirm password reaches it, moving nothing, at ${width}px with the root’s font at ${rootFont || 'the browser’s own'}`, async () => {
      rootFontAt(rootFont)
      await sized(width, 900)
      vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
      const host = await opened(`${USERS}/sam`)
      const form = host.querySelector('form.reset') as HTMLElement
      const [password, confirm] = ['password', 'confirm-password'].map(
        (name) =>
          form.querySelector(`input[name="${name}"]`) as HTMLInputElement,
      )
      const set = form.querySelector('button[name="reset"]') as HTMLElement
      const line = form.querySelector(
        ':scope > .callout > [role="status"]',
      ) as HTMLElement
      const callout = line.parentElement as HTMLElement
      const accountPage = host.querySelector('.account-page') as HTMLElement
      await typeInto(password, 'hunter22')
      // In the middle of the view, so the line under it is in view too, and
      // focused first: neither is a change of layout.
      confirm.scrollIntoView({ block: 'center' })
      act(() => confirm.focus())
      const before = layoutOf(accountPage, callout)
      await typeInto(confirm, 'hunted')
      expect(line.textContent).toBe('The two passwords do not match.')
      expect(layoutOf(accountPage, callout), 'what the line moved').toEqual(
        before,
      )
      act(() => password.focus())
      await expectLaidUnderConfirm(
        line,
        confirm,
        confirm.closest('.settings-row') as Element,
      )
      expectRoomUnseen(set, line)
      if (linesOf(line) === 1) {
        expect(roomKept(set), 'room kept beside Set password').toBeCloseTo(0, 1)
      }
    })
  }

  /**
   * At a desktop width the line takes one line, so Set password's row
   * keeps no room for it, and the form is drawn as a row keeping none
   * draws it (`expectRowDrawnAt`): Set password at its row's end, where a
   * form's buttons stand (`.settings-actions`), under the two boxes.
   */
  for (const width of [1920, 1280]) {
    it(`draws Set password at its row’s end under the two boxes, its row keeping no room for the line saying the two passwords differ, at ${width}px`, async () => {
      await sized(width, 900)
      vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
      const host = await opened(`${USERS}/sam`)
      await expectRowDrawnAt(
        host.querySelector('form.reset button[name="reset"]') as HTMLElement,
        {
          row: [0, 171, 606, 59],
          button: [483, 184, 123, 34],
          boxes: [
            [350, 42, 256, 31],
            [350, 128, 256, 31],
          ],
        },
      )
    })
  }

  it('takes a press on the line saying the two passwords differ, so that Set password beneath it is neither pressed nor focused, and Tab still reaches it', async () => {
    await sized(1280, 900)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    vi.mocked(usersApi.patchUser).mockResolvedValue(sam)
    const host = await opened(`${USERS}/sam`)
    const form = host.querySelector('form.reset') as HTMLElement
    const box = (name: string) =>
      form.querySelector(`input[name="${name}"]`) as HTMLInputElement
    const [password, confirm] = [box('password'), box('confirm-password')]
    const set = form.querySelector('button[name="reset"]') as HTMLElement
    const line = form.querySelector(
      ':scope > .callout > [role="status"]',
    ) as HTMLElement
    await typeInto(password, 'hunter22')
    await typeInto(confirm, 'hunted')
    confirm.scrollIntoView({ block: 'center' })
    expect(line.textContent).toBe('The two passwords do not match.')
    await expectPressesTaken(line, set)
    expect(usersApi.patchUser).not.toHaveBeenCalled()

    await act(async () => {
      await userEvent.click(confirm)
      await userEvent.keyboard('{Tab}')
    })
    expect(document.activeElement).toBe(set)
    expect(line.textContent).toBe('The two passwords do not match.')
  })

  it('says a removal that answers on another account’s page there, leaving focus and the view where they were', async () => {
    await sized(390, 844)
    // Kim's page is longer than the window, so there is a view to move.
    const kim = user({
      id: 3,
      name: 'kim',
      role: 'user',
      sessions: Array.from({ length: 30 }, (_, i) => ({
        id: `s${i}`,
        created: '2026-09-01 10:00:00',
        last_seen: '2026-09-06 09:00:00',
        user_agent: 'Firefox',
        ip: '10.0.0.2',
        current: false,
      })),
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), sam, kim],
    })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      ...removal,
      dry_run: true,
    })
    const remove = deferred<usersApi.RemovalAnswer>()
    vi.mocked(usersApi.removeUser).mockReturnValue(remove.promise)
    const host = await opened(`${USERS}/sam`)
    await press(host.querySelector('button[name="remove"]') as Element)
    await press(host.querySelector('.remove-user .confirm') as Element)
    // Away by the page's own links, while the removal has not answered.
    await press(host.querySelector('.account-page .back a') as Element)
    await press(
      [...host.querySelectorAll('.user-table .row-link')].find(
        (a) => a.textContent === 'kim',
      ) as Element,
    )
    await flush()
    await document.fonts.ready
    const doc = document.documentElement
    expect(doc.scrollHeight).toBeGreaterThan(doc.clientHeight * 1.5)
    // The person is at the foot of the page, on its last Revoke.
    const revoke = [
      ...host.querySelectorAll<HTMLElement>('.session-list button'),
    ].at(-1) as HTMLElement
    revoke.focus()
    expect(window.scrollY).toBeGreaterThan(0)
    const from = revoke.getBoundingClientRect().top

    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), kim] })
    await flush(() => remove.resolve({ ...removal, dry_run: false }))
    await flush()
    const line = host.querySelector('.account-page p.gone-line') as HTMLElement
    expect(line?.textContent).toBe('Removed sam.')
    expect(document.activeElement).toBe(revoke)
    // Where the person is looking has not moved. The line is drawn above
    // the view, and the browser's scroll anchoring moves the scroll by its
    // height to keep it so: the scroll offset is not what is held.
    expect(revoke.getBoundingClientRect().top).toBeCloseTo(from, 0)
    // Announced instead, from the region drawn with the page.
    expect(
      line.parentElement?.matches('.account-page > [aria-live="polite"]'),
    ).toBe(true)
    // Quiet wherever it is drawn, as on the table.
    expect(getComputedStyle(line).color).toBe(
      tokenColour(line.parentElement as Element, '--muted'),
    )
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
        dirs: [longPathRoot],
      },
      {
        id: 2,
        name: 'Light novels',
        works: 0,
        books: 3,
        dirs: [
          root({
            id: 4,
            path: '/srv/novels',
            libraries: [{ id: 2, name: 'Light novels' }],
          }),
        ],
      },
    ],
  }

  async function shown(ui: React.ReactNode) {
    await sized(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(TWO)
    const host = await settled(ui)
    await document.fonts.ready
    return host
  }

  const box = (el: Element) => el.getBoundingClientRect()

  it('sets a library’s page on the measure, and says what it holds quietly', async () => {
    // The second library's page, which says both what it holds and that
    // none of it is readable yet. In the shell, as it is always drawn. Where
    // its picker's line puts Cancel is the phone-width matrix's (the 1280
    // case on a library's page).
    const host = await shown(inShell(routed(`${TABLE}/Light%20novels`)))
    const libraryPage = host.querySelector('.library-page') as HTMLElement
    expect(box(libraryPage).width).toBeLessThanOrEqual(40 * 16 + 0.5)

    // What a library holds is quiet: the colour and size of a directory's own
    // labels, `--muted` at `--text-meta`, as the browser resolves them.
    const probe = document.createElement('span')
    probe.style.color = 'var(--muted)'
    probe.style.fontSize = 'var(--text-meta)'
    libraryPage.append(probe)
    const { color, fontSize } = getComputedStyle(probe)
    probe.remove()
    for (const selector of ['.holds', '.no-readable']) {
      const sentence = getComputedStyle(
        host.querySelector(`.library-page ${selector}`) as Element,
      )
      expect(sentence.color, selector).toBe(color)
      expect(sentence.fontSize, selector).toBe(fontSize)
    }
  })

  it('sets the make page’s form on the measure, its button at its own width, at both its words', async () => {
    const host = await shown(routed(NEW_LIBRARY))
    const form = host.querySelector('form.add-library') as HTMLElement
    const make = form.querySelector('button[type="submit"]') as HTMLElement
    const measured = () => {
      expect(box(form).width).toBeLessThanOrEqual(40 * 16 + 0.5)
      // Its action keeps its own width and does not stretch across.
      expect(box(make).width).toBeLessThan(box(form).width / 2)
    }
    measured()
    vi.mocked(librariesApi.createLibrary).mockReturnValue(new Promise(() => {}))
    await typeInto(
      form.querySelector('input[name="library-name"]') as HTMLInputElement,
      // No library holds it, so the make is not held as it is typed.
      'Westerns',
    )
    await typeInto(
      form.querySelector('input[name="path"]') as HTMLInputElement,
      '/srv/manga',
    )
    expect(make.textContent).toBe('Make the library')
    await drawnWaiting(make, 'Checking…', [
      make,
      form.querySelector('button[name="cancel"]') as HTMLElement,
    ])
    measured()
  })

  it('draws the two boxes of a directory’s first row at one height', async () => {
    // A box in a grid stretches to its row unless told not to.
    const host = await shown(routed(DIR))
    const edition = host.querySelector('input[name="edition_key"]') as Element
    const language = host.querySelector(
      'input[name="default_language"]',
    ) as Element
    expect(box(language).height).toBe(box(edition).height)
  })

  it('lights a Directories row under the pointer only while the row is a link', async () => {
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: {
        works: 5,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    const host = await shown(routed(LIBRARY))
    const heading = host.querySelector(
      '.library-page .page-title',
    ) as HTMLElement
    const row = () => host.querySelector('.dir-table tbody tr') as Element
    const pointAt = (el: Element | Locator) =>
      act(async () => {
        await userEvent.hover(el)
      })
    const rowLead = byRole(row().querySelector('td.lead') as Element, 'cell')
    const background = () => getComputedStyle(row()).backgroundColor
    await pointAt(heading)
    const unlit = background()
    await pointAt(rowLead)
    expect(row().querySelector('a.row-link')).not.toBeNull()
    expect(background()).not.toBe(unlit)

    // The removal open, the row's link is withdrawn and the row goes nowhere.
    await press(host.querySelector('button[name="remove-library"]') as Element)
    expect(row().querySelector('a')).toBeNull()
    await pointAt(heading)
    await pointAt(rowLead)
    expect(background()).toBe(unlit)
  })

  it('puts the removal’s report on the measure, as a grid', async () => {
    const host = await shown(routed(LIBRARY))
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: {
        works: 5,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: {
        works: 5,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
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
    // The report ends where the box of the table beneath it does.
    expect(box(report).right).toBeCloseTo(
      box(host.querySelector('.library-table .settings-box') as Element).right,
      0,
    )
    // Its lists are the two-column grid a directory's scan report has, so a
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
        getComputedStyle(
          host.querySelector('.library-table .page-title') as Element,
        ).fontSize,
      ),
    )
  })

  it('draws a report’s heading alike at every level its caller may pass', async () => {
    // Styled by class, not by tag: a heading is the same label whether a
    // page holds the report under its h2 or the library page holds it under
    // a directory's h4. The browser's own heading faces differ from level to
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

  it('puts the directory browser on a line of its own, and its directories full width', async () => {
    vi.mocked(librariesApi.listDirs).mockResolvedValue({
      path: '/a',
      parent: '/',
      dirs: [{ name: 'b', path: '/a/b' }],
    })
    const host = await shown(
      <DirPicker action="Add directory" onPick={() => undefined} />,
    )
    await press(host.querySelector('button[name="browse"]') as Element)
    const browser = host.querySelector('.dir-browser') as Element
    expect(box(browser).width).toBeCloseTo(
      box(host.querySelector('.dir-picker') as Element).width,
      0,
    )
    // A directory's button takes the whole row of the list it is in.
    const entry = host.querySelector('.dir-list button') as Element
    expect(box(entry).width).toBeCloseTo(
      box(host.querySelector('.dir-list') as Element).width,
      0,
    )
  })

  it('draws the grants of an account as one group of its section’s box, the ticks a row each beneath the Sees row', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({
          role: 'user',
          every_library: false,
          libraries: [{ id: 1, name: 'Manga' }],
        }),
      ],
    })
    const host = await shown(routed(ACCOUNT))
    const group = host.querySelector('.account-page fieldset') as HTMLElement
    const held = group.parentElement as HTMLElement
    expect(held.matches('.settings-box')).toBe(true)
    // Its legend names it for a screen reader alone: the section's heading
    // is its name on screen, so the legend is clipped to nothing there.
    const legend = group.querySelector(':scope > legend') as HTMLElement
    expect(legend.textContent).toBe('Libraries this account may see')
    expect(getComputedStyle(legend).clipPath).toBe('inset(50%)')

    // The group draws no edge, inset or margin of its own, so its rows line
    // up with the box's, as a fieldset's own does not.
    const heldStyle = getComputedStyle(held)
    const start =
      box(held).left +
      Number.parseFloat(heldStyle.borderLeftWidth) +
      Number.parseFloat(heldStyle.paddingLeft)
    const end =
      box(held).right -
      Number.parseFloat(heldStyle.borderRightWidth) -
      Number.parseFloat(heldStyle.paddingRight)
    const sees = group.querySelector(':scope > .settings-row') as HTMLElement
    expect(sees.textContent).toContain('Sees')
    expect(box(sees).left).toBeCloseTo(start, 0)
    expect(box(sees).right).toBeCloseTo(end, 0)

    // The ticks are beneath the Sees row, split from it by the hairline the
    // box's rows are split by, as the browser resolves it. A border left to
    // `currentColor` is the text's colour and fails here.
    const list = group.querySelector('.grant-list') as HTMLElement
    expect(box(list).top).toBeGreaterThanOrEqual(box(sees).bottom)
    const listStyle = getComputedStyle(list)
    expect(listStyle.borderTopWidth).toBe('1px')
    expect(listStyle.borderTopStyle).toBe('solid')
    const probe = document.createElement('div')
    probe.style.borderTop = '1px solid var(--line)'
    group.append(probe)
    const line = getComputedStyle(probe).borderTopColor
    probe.remove()
    expect(listStyle.borderTopColor).toBe(line)
    expect(line).not.toBe(listStyle.color)

    // A radio is a box, then a gap, then its words.
    const choices = [...group.querySelectorAll('.settings-choices label')]
    expect(choices).toHaveLength(2)
    for (const label of choices) {
      expect(getComputedStyle(label).display).toBe('flex')
      expect(getComputedStyle(label).columnGap).toBe('8px')
    }
    // A tick is at its row's end, beside its library's name.
    const rows = [...list.querySelectorAll('.settings-row')]
    expect(rows).toHaveLength(2)
    for (const row of rows) {
      const name = box(row.querySelector('.settings-row-label') as Element)
      const input = row.querySelector('input[type="checkbox"]') as Element
      const tick = box(input)
      expect(tick.left).toBeGreaterThan(name.right)
      // The box's own margin is the browser's, outside the row's control.
      expect(
        tick.right + Number.parseFloat(getComputedStyle(input).marginRight),
      ).toBeCloseTo(end, 0)
      expect(tick.top).toBeLessThan(name.bottom)
      expect(tick.bottom).toBeGreaterThan(name.top)
    }
  })
})

/**
 * The directory picker's path box and Browse… are one line's controls, so
 * they are drawn at one height and, side by side, from one top: on the make
 * page, where the box is the row's control, and in Add directory…, where it
 * is under its own label. Where the buttons have gone under the box, the
 * heights are still one.
 */
describe('the directory picker’s line', () => {
  it.each([
    [NEW_LIBRARY, 1920, true],
    [NEW_LIBRARY, 1024, true],
    [NEW_LIBRARY, 390, true],
    [LIBRARY, 1920, true],
    [LIBRARY, 1024, true],
    [LIBRARY, 390, false],
  ] as const)(
    'on %s at %i px draws Browse… at the path box’s height, beside it: %s',
    async (entry, width, beside) => {
      await sized(width, 900)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(root()))
      const host = await opened(entry)
      if (entry === LIBRARY) {
        await press(host.querySelector('button[name="add-dir"]') as Element)
      }
      await document.fonts.ready
      const picker = host.querySelector('.dir-picker') as HTMLElement
      const [path, browse] = [
        picker.querySelector('input[name="path"]') as Element,
        picker.querySelector('button[name="browse"]') as Element,
      ].map((el) => el.getBoundingClientRect())
      expect(browse.top < path.bottom, 'beside the box').toBe(beside)
      expect
        .soft(Math.abs(browse.height - path.height), 'heights apart')
        .toBeLessThanOrEqual(0.5)
      if (beside) {
        expect
          .soft(Math.abs(browse.top - path.top), 'tops apart')
          .toBeLessThanOrEqual(0.5)
      }
    },
  )
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

/**
 * **The way up from a page beneath a table is a finger's 44 px under a
 * coarse pointer** (spec 2026-09-17 §7.1), on a new library's page, a
 * library's, a directory's, a new account's and an account's, and its target
 * stays clear of the tabs' above it, which O157 grows to 44 the same way.
 * Each target, measured by hit testing along its middle row and
 * column (`pressArea`), is its box grown about its middle to 44 px
 * (`fingerArea`), so nothing is drawn over any of it on those lines; and
 * the way up's grown box shares nothing with any tab's, corners included.
 * The fine case shows it is the coarse rules that reach 44.
 */
describe('the way up from a page beneath a table', () => {
  // Coarse first: the fine case after it is what shows the coarse rules
  // were switched back.
  it.each([
    [NEW_LIBRARY, 390, 'coarse', '‹ Libraries'],
    [LIBRARY, 390, 'coarse', '‹ Libraries'],
    [DIR, 390, 'coarse', '‹ Manga'],
    [NEW_ACCOUNT, 390, 'coarse', '‹ Users'],
    [ACCOUNT, 390, 'coarse', '‹ Users'],
    [LIBRARY, 1280, 'coarse', '‹ Libraries'],
    [ACCOUNT, 1280, 'coarse', '‹ Users'],
    [LIBRARY, 390, 'fine', '‹ Libraries'],
  ] as const)(
    'on %s at %i px with a %s pointer',
    async (entry, width, pointer, words) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      await sized(width, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(root()))
      vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user()] })
      const host = await opened(entry)
      await document.fonts.ready
      const backs = [...host.querySelectorAll('.config .back a')]
      expect(backs.map((link) => link.textContent)).toEqual([words])
      const back = backs[0] as Element
      const tabs = [...host.querySelectorAll('.config-nav a')]
      expect(tabs).toHaveLength(5)
      if (pointer === 'fine') {
        expect(pressArea(back).height).toBeLessThan(44)
        return
      }
      expect.soft(pressArea(back), words).toEqual(near(fingerArea(back)))
      for (const tab of tabs) {
        const name = tab.textContent ?? ''
        expect.soft(pressArea(tab), name).toEqual(near(fingerArea(tab)))
        expect
          .soft(overlapOf(fingerArea(back), fingerArea(tab)), `with ${name}`)
          .toBe(0)
      }
    },
  )
})

/**
 * **A checkbox row's first line is a finger's 44 px tall under a coarse
 * pointer, and all of it is the target** (spec 2026-09-17 §7.1): an
 * account's three permissions, each library it is limited to, and a
 * directory's Keeps documents under .taiyaku/, every check row these pages
 * draw. The label takes the line, the gap and the box's column included,
 * and the box is centred on it. Hit testing along the label's middle row
 * and column (`pressArea`) finds it from its start to the box, and a press
 * a pixel inside the top and the bottom of the 44 px, under the words and
 * above and below the box, one just above the box and one in the gap, each
 * sends one write. Per row, since one row's help can be taller than
 * another's.
 */
describe('a checkbox row under a coarse pointer', () => {
  const sam = user({
    id: 2,
    name: 'sam',
    role: 'user',
    every_library: false,
    libraries: [{ id: 1, name: 'Manga' }],
  })

  it.each([
    [
      `${USERS}/sam`,
      ['Can download', 'Can stream', 'Can stamp', 'Manga'],
      usersApi.patchUser,
    ],
    [DIR, ['Keeps documents under .taiyaku/'], config.patchRoot],
  ] as const)('on %s at 390 px', async (entry, words, write) => {
    onTestFinished(coarseRulesOn())
    await sized(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(root()))
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    vi.mocked(usersApi.patchUser).mockResolvedValue(sam)
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    const host = await opened(entry)
    await document.fonts.ready
    const rows = [...host.querySelectorAll<HTMLElement>('.settings-row-check')]
    expect(rows.map((row) => row.querySelector('label')?.textContent)).toEqual(
      words,
    )
    for (const row of rows) {
      row.scrollIntoView({ block: 'center' })
      const label = row.querySelector('label.settings-row-label') as Element
      const name = label.textContent ?? ''
      const box = row.closest('.settings-box') as HTMLElement
      const [at, tick, control, within] = [
        label,
        row.querySelector('input[type="checkbox"]') as Element,
        row.querySelector('.settings-row-control') as Element,
        box,
      ].map((el) => el.getBoundingClientRect())
      const middle = at.top + at.height / 2
      const tickMiddle = tick.top + tick.height / 2
      const gap = Number.parseFloat(getComputedStyle(row).columnGap)
      /** The middle of the words' column, before the gap. */
      const wordsX = (at.left + control.left - gap) / 2
      expect
        .soft(at.height, `${name}: the label's height`)
        .toBeGreaterThanOrEqual(44)
      expect
        .soft(tickMiddle - middle, `${name}: the box's centre`)
        .toBeCloseTo(0, 0)
      const area = pressArea(label)
      expect
        .soft([area.top, area.bottom, area.left], `${name}: a press lands`)
        .toEqual([
          expect.closeTo(at.top, 1),
          expect.closeTo(at.bottom, 1),
          expect.closeTo(at.left, 1),
        ])
      // And up to the box, and no further: over the box a press lands on the
      // box, not on the label its column lies over. Hit testing answers for
      // a pixel, so an edge is found up to a pixel either side.
      expect
        .soft(
          Math.abs(tick.left - area.right),
          `${name}: a press lands up to the box`,
        )
        .toBeLessThan(1)
      expect
        .soft(
          document.elementFromPoint(tick.left + tick.width / 2, tickMiddle),
          `${name}: a press on the box`,
        )
        .toBe(row.querySelector('input[type="checkbox"]'))
      // Pressed through the box the row is drawn in, so that a press that
      // misses the label is a press on what lies around it, and not refused.
      // A press's position is from the box's padding edge, inside its border.
      for (const [where, x, y] of [
        ['at the top, under the words', wordsX, middle - 21],
        ['at the bottom, under the words', wordsX, middle + 21],
        ['in the gap', control.left - gap / 2, middle],
        ['above the box', tick.left + tick.width / 2, tickMiddle - 21],
        ['below the box', tick.left + tick.width / 2, tickMiddle + 21],
        // Inside the box's own column, whose element wraps the box closely.
        ['just above the box', tick.left + tick.width / 2, tick.top - 2],
      ] as const) {
        const sent = vi.mocked(write).mock.calls.length
        await act(async () => {
          await userEvent.click(box, {
            position: {
              x: x - within.left - box.clientLeft,
              y: y - within.top - box.clientTop,
            },
          })
          await new Promise((resolve) => setTimeout(resolve, 20))
        })
        expect
          .soft(
            vi.mocked(write).mock.calls.length - sent,
            `${name}: a press ${where}`,
          )
          .toBe(1)
      }
    }
  })

  it('keeps the space where a label’s words meet its markup', async () => {
    onTestFinished(coarseRulesOn())
    await sized(390, 844)
    const host = await act(async () =>
      mount(
        <div className="config">
          <Section title="Scanning">
            <Row
              check
              label={
                <>
                  Keeps <code>.taiyaku/</code>
                </>
              }
            >
              {(ids) => <input type="checkbox" id={ids.control} />}
            </Row>
          </Section>
        </div>,
      ),
    )
    await document.fonts.ready
    const label = host.querySelector('label.settings-row-label') as Element
    expect(label.textContent).toBe('Keeps .taiyaku/')
    const text = document.createTreeWalker(label, NodeFilter.SHOW_TEXT)
    const keeps = text.nextNode() as Text
    expect(keeps.data).toBe('Keeps ')
    /** Where the text node's first `end` characters end. */
    const endOf = (end: number) => {
      const range = document.createRange()
      range.setStart(keeps, 0)
      range.setEnd(keeps, end)
      return range.getBoundingClientRect()
    }
    const word = endOf('Keeps'.length)
    const space = endOf('Keeps '.length)
    const code = (
      label.querySelector('code') as Element
    ).getBoundingClientRect()
    // The space is drawn, and the code follows it on the same line.
    expect(space.right - word.right).toBeGreaterThan(2)
    expect(code.left).toBeCloseTo(space.right, 0)
    expect(code.top).toBeLessThan(word.bottom)
  })
})

/**
 * An account limited to two libraries: Manga, whose name is one line and
 * whose three directories make its help several, and one whose name takes
 * two lines at a phone's width. The box of each check row is beside its
 * label's words, and the gap from a name to its first directory is one
 * whatever the name's length.
 */
describe('a library’s grant', () => {
  const LONG_NAME =
    'Collected chapters and side stories from the weekly anthology'
  const GRANTS = {
    libraries: [
      {
        id: 1,
        name: 'Manga',
        works: 0,
        books: 0,
        dirs: [1, 2, 3].map((id) =>
          root({ id, path: `/srv/media/manga/shelf-${id}/Volumes` }),
        ),
      },
      {
        id: 2,
        name: LONG_NAME,
        works: 0,
        books: 0,
        dirs: [
          root({
            id: 4,
            path: '/srv/media/anthology/Chapters',
            libraries: [{ id: 2, name: LONG_NAME }],
          }),
        ],
      },
    ],
  }
  const sam = user({
    id: 2,
    name: 'sam',
    role: 'user',
    every_library: false,
    libraries: [{ id: 1, name: 'Manga' }],
  })
  const grantsAt = async (width: number) => {
    await sized(width, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(GRANTS)
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    const host = await opened(`${USERS}/sam`)
    await document.fonts.ready
    return host
  }
  /** Each line a check row's label's words are drawn on, top to bottom. */
  const linesOf = (row: Element) => {
    const words = document.createRange()
    words.selectNodeContents(
      row.querySelector('label.settings-row-label') as Element,
    )
    return [...words.getClientRects()]
  }

  it.each([1920, 390])(
    'puts each box beside its label’s first line with a mouse, however tall its help, at %i px',
    async (width) => {
      const host = await grantsAt(width)
      const rows = [...host.querySelectorAll('.settings-row-check')]
      const names = rows.map((row) => row.querySelector('label')?.textContent)
      expect(names).toEqual([
        'Can download',
        'Can stream',
        'Can stamp',
        'Manga',
        LONG_NAME,
      ])
      // Manga's help is taller than its name, so the middle of the whole
      // item is below the name's line.
      const manga = rows[3] as Element
      const help = (
        manga.querySelector('.settings-row-help') as Element
      ).getBoundingClientRect()
      expect(help.height).toBeGreaterThan(2 * linesOf(manga)[0].height)
      for (const [i, row] of rows.entries()) {
        const first = linesOf(row)[0]
        const tick = (
          row.querySelector('input[type="checkbox"]') as Element
        ).getBoundingClientRect()
        const centre = tick.top + tick.height / 2
        expect
          .soft([centre >= first.top, centre <= first.bottom], `${names[i]}`)
          .toEqual([true, true])
      }
    },
  )

  it('keeps the gap from a name to its first directory under a coarse pointer, one line or two', async () => {
    onTestFinished(coarseRulesOn())
    const host = await grantsAt(390)
    const rows = [...host.querySelectorAll('.grant-list .settings-row-check')]
    const lines = rows.map((row) => linesOf(row))
    const tops = (rects: DOMRect[]) => new Set(rects.map((r) => r.top)).size
    expect(lines.map(tops)).toEqual([1, 2])
    const gaps = rows.map((row, i) => {
      const path = (
        row.querySelector('.grant-dirs code') as Element
      ).getBoundingClientRect()
      const last = lines[i].at(-1) as DOMRect
      return path.top - last.bottom
    })
    expect(Math.abs(gaps[0] - gaps[1]), `gaps ${gaps}`).toBeLessThanOrEqual(1)
    // The band a finger aims along is kept.
    for (const row of rows) {
      const label = row.querySelector('label.settings-row-label') as Element
      expect(label.getBoundingClientRect().height).toBeGreaterThanOrEqual(44)
    }
  })
})

// --- colours as drawn --------------------------------------------------------

/** `el` as drawn: the pixels of its screenshot, read back through a canvas. */
const drawn = async (el: Element) => {
  const png = await page.screenshot({ element: el, save: false })
  const image = new Image()
  image.src = `data:image/png;base64,${png}`
  await image.decode()
  const canvas = document.createElement('canvas')
  canvas.width = image.naturalWidth
  canvas.height = image.naturalHeight
  const context = canvas.getContext('2d')
  if (context === null) throw new Error('no canvas to read the pixels from')
  context.drawImage(image, 0, 0)
  return context.getImageData(0, 0, canvas.width, canvas.height)
}
const pixelAt = (image: ImageData, x: number, y: number): Channels => {
  const at = (y * image.width + x) * 4
  const [r, g, b] = image.data.slice(at, at + 3)
  return { r, g, b, a: 1 }
}
/** How far apart two colours are: their channels' differences, summed. */
const apart = (one: Channels, other: Channels) =>
  Math.abs(one.r - other.r) +
  Math.abs(one.g - other.g) +
  Math.abs(one.b - other.b)

/**
 * An unchecked checkbox's and radio's edge is the edge of a control, at 3:1
 * or more against the ground it is drawn on (WCAG 1.4.11), in Ink as in
 * Paper; a checked one is filled, or dotted, with the accent. Read from the
 * computed style, the edge as a drawn border: a box with no border of its
 * own is the browser's, whose edge no style reaches.
 */
describe('a checkbox and a radio', () => {
  /** The background of the nearest box under `el` that paints one. */
  const groundOf = (el: Element) => {
    for (let at = el.parentElement; at; at = at.parentElement) {
      const ground = channels(getComputedStyle(at).backgroundColor)
      if (ground.a === 1) return ground
    }
    throw new Error('nothing under it paints a ground')
  }

  for (const [theme, name] of [
    ['ink', 'Ink'],
    ['paper', 'Paper'],
  ] as const) {
    it(`draws an unchecked one’s edge at 3:1 or more against its ground, and a checked one in the accent, in ${name}`, async () => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      await sized(1280, 900)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          { id: 1, name: 'Manga', works: 0, books: 0, dirs: [root()] },
          {
            id: 2,
            name: 'Comics',
            works: 0,
            books: 0,
            dirs: [root({ id: 4, path: '/srv/comics', libraries: COMICS })],
          },
        ],
      })
      const sam = user({
        id: 2,
        name: 'sam',
        role: 'user',
        every_library: false,
        libraries: [{ id: 1, name: 'Manga' }],
      })
      vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
      const host = await opened(`${USERS}/sam`)
      const all = [
        ...host.querySelectorAll<HTMLInputElement>(
          'input[type="checkbox"], input[type="radio"]',
        ),
      ]
      const kinds = (checked: boolean) =>
        all.filter((el) => el.checked === checked).map((el) => el.type)
      // Can stamp and Comics; Every library. Can download, Can stream and
      // Manga; Only these.
      expect(kinds(false)).toEqual(['checkbox', 'radio', 'checkbox'])
      expect(kinds(true)).toEqual(['checkbox', 'checkbox', 'radio', 'checkbox'])
      const accent = tokenColour(host, '--accent')
      for (const box of all) {
        const style = getComputedStyle(box)
        const what = `${box.name || box.value} ${box.checked ? 'checked' : 'unchecked'}`
        expect
          .soft([style.borderTopStyle, style.borderTopWidth], what)
          .toEqual(['solid', '1px'])
        if (!box.checked) {
          const edge = channels(style.borderTopColor)
          expect
            .soft(contrast(edge, groundOf(box)), what)
            .toBeGreaterThanOrEqual(3)
        } else if (box.type === 'checkbox') {
          expect.soft(style.backgroundColor, what).toBe(accent)
          // The tick, in the colour drawn on the accent.
          const tick = getComputedStyle(box, '::before')
          expect
            .soft(tick.backgroundColor, what)
            .toBe(tokenColour(host, '--on-accent'))
        } else {
          expect.soft(style.borderTopColor, what).toBe(accent)
          expect
            .soft(getComputedStyle(box, '::before').backgroundColor, what)
            .toBe(accent)
        }
      }
    })
  }

  it('draws a held one with a dashed edge, and a held checked one on the page’s ground with its tick or dot in the accent, unlike a live one', async () => {
    const host = await act(async () =>
      mount(
        <div className="library">
          <input type="checkbox" disabled />
          <input type="radio" disabled />
          <input type="checkbox" />
          <input type="checkbox" defaultChecked disabled />
          <input type="checkbox" defaultChecked />
          <input type="radio" defaultChecked disabled />
          <input type="radio" defaultChecked />
        </div>,
      ),
    )
    const [box, radio, live, heldBox, liveBox, heldRadio, liveRadio] = [
      ...host.querySelectorAll('input'),
    ]
    for (const held of [box, radio, heldBox, heldRadio]) {
      expect.soft(getComputedStyle(held).borderTopStyle).toBe('dashed')
    }
    for (const one of [live, liveBox, liveRadio]) {
      expect.soft(getComputedStyle(one).borderTopStyle).toBe('solid')
    }
    const accent = tokenColour(host, '--accent')
    for (const [held, one] of [
      [heldBox, liveBox],
      [heldRadio, liveRadio],
    ] as const) {
      const style = getComputedStyle(held)
      expect
        .soft(style.backgroundColor, held.type)
        .toBe(tokenColour(host, '--bg'))
      expect.soft(style.borderTopColor, held.type).toBe(accent)
      expect
        .soft(getComputedStyle(held, '::before').backgroundColor, held.type)
        .toBe(accent)
      // The fills differ. For the checkbox the fill is the cue: a dashed
      // edge in the accent on a box filled with the accent is not seen. For
      // the radio the dashed ring is: its dot leaves the ring in sight, and
      // its held ground and its live one are too near to be told apart.
      expect
        .soft(style.backgroundColor, held.type)
        .not.toBe(getComputedStyle(one).backgroundColor)
    }
  })

  // Forced colours are set over the DevTools session, as a system set to
  // them has the browser draw, and cleared after: the setting is the page's,
  // not this frame's, and would outlive the file.
  it('leaves it to the browser under forced colours, with no tick or dot of its own drawn over the browser’s', async () => {
    const force = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'forced-colors', value }],
      })
    onTestFinished(async () => {
      await force('')
    })
    const host = await act(async () =>
      mount(
        // Four times over, so that a screenshot has pixels enough to count.
        <div className="library" style={{ zoom: 4 }}>
          <input type="checkbox" />
          <input type="checkbox" defaultChecked />
          <input type="radio" />
          <input type="radio" defaultChecked />
        </div>,
      ),
    )
    const [box, checkedBox, radio, checkedRadio] = [
      ...host.querySelectorAll('input'),
    ]
    expect(getComputedStyle(checkedBox).appearance).toBe('none')
    await force('active')
    // THE STATE THIS TEST NAMES: this page is told the system forces colours.
    expect(matchMedia('(forced-colors: active)').matches).toBe(true)
    const probe = document.createElement('div')
    probe.style.color = 'Canvas'
    host.append(probe)
    const ground = channels(getComputedStyle(probe).color)
    probe.remove()
    /** The share of `el`'s box drawn in anything but the ground. */
    const inked = async (el: Element) => {
      const image = await drawn(el)
      let count = 0
      for (let y = 0; y < image.height; y++) {
        for (let x = 0; x < image.width; x++) {
          if (apart(pixelAt(image, x, y), ground) > 192) count++
        }
      }
      return count / (image.width * image.height)
    }
    for (const [off, on] of [
      [box, checkedBox],
      [radio, checkedRadio],
    ] as const) {
      for (const one of [off, on]) {
        expect.soft(getComputedStyle(one).appearance, one.type).toBe('auto')
      }
      expect
        .soft(getComputedStyle(on, '::before').content, on.type)
        .toBe('none')
      // The browser's own control shows its state: checked, its fill or its
      // dot inks a share of the box that unchecked is the ground.
      const [unchecked, checked] = [await inked(off), await inked(on)]
      expect
        .soft(
          checked - unchecked,
          `${on.type}: ${checked} against ${unchecked}`,
        )
        .toBeGreaterThan(0.15)
    }
  })

  /**
   * The browser's own radio is dotted three fifths of it across, at any
   * size, so the dot stands off the ring by the same share of the box at the
   * coarse 24 px as at the fine 13 px. Measured along each one's middle row:
   * the run of the dot's colour through its middle, as a share of the box,
   * the run ending where a pixel is nearer the gap's colour than the dot's.
   */
  for (const pointer of ['fine', 'coarse'] as const) {
    it(`dots a checked radio across the share of it the browser dots its own, with a ${pointer} pointer`, async () => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      const host = await act(async () =>
        mount(
          // Eight times over, so that a pixel of the screenshot is a small
          // share of the box. The browser's own is outside `.library`, which
          // the drawing is scoped to, and inside `.config`, which the coarse
          // size is.
          <div className="config" style={{ zoom: 8 }}>
            <div className="library">
              <input type="radio" defaultChecked />
            </div>
            <input type="radio" defaultChecked />
          </div>,
        ),
      )
      const [ours, browsers] = [...host.querySelectorAll('input')]
      const size = pointer === 'coarse' ? '24px' : '13px'
      expect(getComputedStyle(ours).inlineSize).toBe(size)
      expect(getComputedStyle(browsers).inlineSize).toBe(size)
      expect(getComputedStyle(browsers).appearance).toBe('auto')
      const dotted = async (radio: Element) => {
        const image = await drawn(radio)
        const y = Math.floor(image.height / 2)
        const middle = Math.floor(image.width / 2)
        const dot = pixelAt(image, middle, y)
        const row = Array.from({ length: image.width }, (_, x) =>
          apart(pixelAt(image, x, y), dot),
        )
        // The gap is the colour on the row farthest from the dot's.
        const half = Math.max(...row) / 2
        let left = middle
        while (left > 0 && row[left - 1] < half) left--
        let right = middle
        while (right < image.width - 1 && row[right + 1] < half) right++
        return (right - left + 1) / image.width
      }
      const [share, theirs] = [await dotted(ours), await dotted(browsers)]
      expect(
        Math.abs(share - theirs),
        `${share} of it across, the browser's ${theirs}`,
      ).toBeLessThan(0.03)
    })
  }
})

describe('a library’s rename box', () => {
  // Forced colours over the DevTools session, cleared after, as `a checkbox
  // and a radio` sets them.
  it('marks its focus under forced colours, where the line under it is drawn in one colour whether it has focus or not', async () => {
    const force = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'forced-colors', value }],
      })
    onTestFinished(async () => {
      await force('')
    })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(root()))
    const host = await opened(LIBRARY)
    await press(host.querySelector('button[name="rename-library"]') as Element)
    const box = host.querySelector('.page-title-box') as HTMLInputElement
    const save = host.querySelector('button[name="save-rename"]') as Element
    await force('active')
    // THE STATE THIS TEST NAMES: this page is told the system forces colours.
    expect(matchMedia('(forced-colors: active)').matches).toBe(true)
    const probe = document.createElement('div')
    probe.style.color = 'Canvas'
    host.append(probe)
    const ground = channels(getComputedStyle(probe).color)
    probe.remove()
    /**
     * The share of the box's last two columns drawn in anything but the
     * ground, above the line under it: past the end of the name, where only
     * a mark of focus draws.
     */
    const markedAtItsEnd = async () => {
      const image = await drawn(box)
      const scale = image.width / box.getBoundingClientRect().width
      const columns = Math.round(2 * scale)
      const rows = image.height - Math.round(2 * scale)
      let count = 0
      for (let y = 0; y < rows; y++) {
        for (let x = image.width - columns; x < image.width; x++) {
          if (apart(pixelAt(image, x, y), ground) > 192) count++
        }
      }
      return count / (rows * columns)
    }
    act(() => (save as HTMLElement).focus())
    expect(await markedAtItsEnd()).toBeLessThan(0.1)
    act(() => box.focus())
    expect(document.activeElement).toBe(box)
    expect(await markedAtItsEnd()).toBeGreaterThan(0.9)
  })
})

/**
 * Every `<summary>` the settings pages draw, each pressed with a real mouse:
 * the cursor over it and its marker, closed and open
 * (`support/disclosure.ts`). The Rules page's is held in
 * `RuleEditor.browser.test.tsx`, which can save a rule.
 */
describe('the settings pages’ disclosures', () => {
  const summaryOf = (host: HTMLElement, details: string) =>
    host.querySelector(`${details} > summary`) as HTMLElement

  it('draws How backups work as something to press, closed and open', async () => {
    await sized(1280, 800)
    const host = await opened('/config/backup')
    await expectDisclosure(
      summaryOf(host, '.settings-details'),
      'How backups work',
    )
  })

  it('draws what re-deriving without a directory would find as something to press, closed and open', async () => {
    await sized(1280, 800)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(root()))
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    const host = await opened(DIR)
    await press(
      [...host.querySelectorAll('.dir-page button')].find(
        (b) => b.textContent === 'Remove…',
      ) as Element,
    )
    await expectDisclosure(
      summaryOf(host, '.remove-root .removal-derive'),
      'the directory removal’s derive',
    )
  })

  it('draws what re-deriving without a library would find as something to press, closed and open', async () => {
    await sized(1280, 800)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(root()))
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 0,
        notes: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    const host = await opened(LIBRARY)
    await press(host.querySelector('button[name="remove-library"]') as Element)
    await expectDisclosure(
      summaryOf(host, '.remove-library .removal-derive'),
      'the library removal’s derive',
    )
  })
})

describe('a removal’s confirm, its button and Cancel', () => {
  const COST = {
    removed: {
      works: 1896,
      positions: 0,
      notes: 0,
      left_with_no_library: [],
      libraries_removed: [],
      kept: [],
    },
    derive: derived(),
    dry_run: true as const,
  }

  it.each([1280, 390])('lays out a library’s at %i', async (width) => {
    await sized(width, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        {
          id: 1,
          name: 'Volumes',
          works: 1896,
          books: 0,
          dirs: [
            root({ libraries: [{ id: 1, name: 'Volumes' }] }),
            root({
              id: 4,
              path: '/srv/volumes',
              libraries: [{ id: 1, name: 'Volumes' }],
            }),
          ],
        },
      ],
    })
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue(COST)
    const host = await opened(`${TABLE}/Volumes`)
    await press(host.querySelector('button[name="remove-library"]') as Element)
    await document.fonts.ready
    const box = host.querySelector('.remove-library') as HTMLElement
    expect(box.querySelector('.confirm')?.textContent).toBe('Remove Volumes')
    expectRemovalFoot(box, 1)
  })

  // The path with nowhere to break is too wide at 1280 for Cancel beside
  // the confirm too; at 390 it is `keeps the removal’s confirm button
  // inside the page for the same path`'s.
  it.each([
    [1280, '/srv/manga', 1],
    [390, '/srv/manga', 1],
    [1280, UNBROKEN_ROOT.path, 2],
  ] as const)(
    'lays out a directory’s at %i, its path %s, on %i line(s)',
    async (width, path, lines) => {
      await sized(width, 844)
      vi.mocked(librariesApi.listLibraries).mockResolvedValue(
        holding(root({ path })),
      )
      vi.mocked(config.removalCost).mockResolvedValue(COST)
      const host = await opened(DIR)
      await press(
        buttonSaying(host.querySelector('.dir-page') as Element, 'Remove…'),
      )
      await document.fonts.ready
      const box = host.querySelector('.remove-root') as HTMLElement
      expect(box.querySelector('.confirm')?.textContent).toBe(`Remove ${path}`)
      expectRemovalFoot(box, lines)
    },
  )

  it.each([1280, 390])(
    'lays out a directory’s removal from one library of two at %i',
    async (width) => {
      await sized(width, 844)
      const both = [
        { id: 1, name: 'Manga' },
        { id: 2, name: 'Kids' },
      ]
      vi.mocked(librariesApi.listLibraries).mockResolvedValue({
        libraries: [
          {
            id: 1,
            name: 'Manga',
            works: 0,
            books: 0,
            dirs: [root({ libraries: both })],
          },
          {
            id: 2,
            name: 'Kids',
            works: 0,
            books: 0,
            dirs: [root({ libraries: both })],
          },
        ],
      })
      vi.mocked(librariesApi.unlinkCost).mockResolvedValue({
        unlinked: {
          loses_sight: ['alice'],
          library_removed: null,
          grants_removed: 0,
          left_with_no_library: [],
        },
        dry_run: true,
      })
      const host = await opened(DIR)
      await press(
        buttonSaying(
          host.querySelector('.dir-page') as Element,
          'Remove from Manga…',
        ),
      )
      await document.fonts.ready
      const box = host.querySelector('.unlink-root') as HTMLElement
      expect(box.querySelector('.confirm')?.textContent).toBe(
        'Remove from Manga',
      )
      expectRemovalFoot(box, 1)
    },
  )

  it.each([1280, 390])('lays out an account’s at %i', async (width) => {
    await sized(width, 844)
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
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
    const host = await opened(`${USERS}/sam`)
    await press(host.querySelector('button[name="remove"]') as Element)
    await document.fonts.ready
    const box = host.querySelector('.remove-user') as HTMLElement
    expect(box.querySelector('.confirm')?.textContent).toBe('Remove sam')
    expectRemovalFoot(box, 1)
  })
})

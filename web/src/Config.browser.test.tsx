import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
// **Required**, the same reason `PreviewTabs.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app through
// `index.css`. Without this the boxes below are unstyled and the assertions
// measure browser defaults.
import './index.css'
import * as config from './config'
import type { Derived, FinishedScan, Walk } from './config'
import { GateContext } from './gate'
import { Backup } from './Backup'
import * as backupApi from './backup'
import { Roots } from './Roots'
import { Users } from './Users'
import * as usersApi from './users'
import { ScanReport } from './ScanReport'
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
 * file stubs `./config`, `./users` and `./backup` exactly as `Roots.test.tsx`,
 * `Users.test.tsx` and `Backup.test.tsx` do — copying each mock surface from
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
  addRoot: vi.fn(),
  listRoots: vi.fn(),
  patchRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
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
  roots: [],
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
  note: "this process's own /api/works list is a startup snapshot",
  ...over,
})

const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
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
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

describe('the Configuration screens at 390px', () => {
  /**
   * `<Roots>`/`<Users>`/`<Backup>` mount standalone in production too — they
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

  /** Both, not just the document — see this describe block's own comment. */
  function assertNoSidewaysScroll(host: HTMLElement) {
    const doc = document.documentElement
    const main = host.querySelector('main')
    expect(main).not.toBeNull()
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
    expect(main!.scrollWidth).toBeLessThanOrEqual(main!.clientWidth)
  }

  it('does not scroll sideways on the roots screen', async () => {
    await page.viewport(390, 844)
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [longPathRoot] })
    const host = await act(async () => {
      const mounted = mount(inShell(<Roots />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    assertNoSidewaysScroll(host)
  })

  it('does not scroll sideways with a populated scan report', async () => {
    // C1 of the whole-branch review over ⑤b: the roots case above mocks
    // `scanStatus` with `report: null`, so nothing here ever laid out a scan
    // report at 390px — the width `.scan-report dl`'s descendant selector
    // reached `UnreadableCauses`'s own nested `<dl>` and gave it a 256px
    // floor. The `unreadable` block has to be populated with causes
    // (`by_format`/`uncounted`/`vanished`), or that nested `<dl>` never
    // renders enough to exercise the defect. Fixture copied from
    // `ScanReport.test.tsx`'s "says why each unreadable entry was skipped,
    // cause by cause". Wrapped in a bare `.root-row` `<li>`, the way
    // `RootRow.tsx` actually mounts a `<ScanReport>`: that card's own
    // `var(--space-4)` padding is what narrows `.unreadable-causes` enough
    // for the nested `<dl>`'s 256px floor to actually overflow — a bare
    // `<ScanReport>` straight inside `<main>` has 358px to give it, which is
    // wide enough that the same defect never reaches `main` or the document.
    await page.viewport(390, 844)
    const host = await act(async () => {
      const mounted = mount(
        inShell(
          <section className="roots">
            <ul className="root-list">
              <li className="root-row">
                <ScanReport
                  outcome={finished({
                    derive: derived({
                      unreadable: {
                        total: 29,
                        refused_format: 15,
                        by_format: { '7z': 3, rar: 12 },
                        uncounted: ['/srv/manga/Bisque Doll/Bisque Doll.cbz'],
                        vanished: [
                          '/srv/manga/Gone - v01',
                          '/srv/manga/Gone - v02',
                        ],
                        no_pages: 11,
                      },
                    }),
                  })}
                />
              </li>
            </ul>
          </section>,
        ),
      )
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    // Measured once the fonts are in. In the fallback font this report is
    // wider than the screen, so a run that measured before they had loaded
    // failed: whenever the case ran alone, and now and then inside a whole
    // run. **With the fonts in, this case reads the same with the rule's
    // defect put back as without** — read O177 before trusting it to hold
    // that rule.
    await document.fonts.ready
    assertNoSidewaysScroll(host)
  })

  it('does not scroll sideways on the users screen', async () => {
    await page.viewport(390, 844)
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [longPathRoot] })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user()],
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
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [longPathRoot] })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user({ roots: [] })],
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
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [longPathRoot] })
    const host = await act(async () => {
      const mounted = mount(<Roots />)
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
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

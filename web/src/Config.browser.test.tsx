import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
// **Required**, the same reason `PreviewTabs.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app through
// `index.css`. Without this the boxes below are unstyled and the assertions
// measure browser defaults.
import './index.css'
import * as config from './config'
import type { Derived, FinishedScan, Unreadable, Walk } from './config'
import { GateContext } from './gate'
import { Backup } from './Backup'
import * as backupApi from './backup'
import { Roots } from './Roots'
import { Users } from './Users'
import * as usersApi from './users'
import { DeriveSection, ScanReport } from './ScanReport'
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
  removalCost: vi.fn(),
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

/** A root whose path has a longer run between hyphens and spaces than a card at 390 is wide. */
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

  it('keeps a root’s own path inside its card when the path has nowhere to break', async () => {
    // `longPathRoot` above breaks at its hyphen and fits. A root whose path
    // has a longer run than the card is wide used to take the card's heading
    // out of the card, and the document with it — found on a scratch library
    // under `target/`, whose path is that long (O180).
    await page.viewport(390, 844)
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [UNBROKEN_ROOT] })
    const host = await act(async () => {
      const mounted = mount(inShell(<Roots />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
  })

  it('keeps the removal’s confirm button inside the card for the same path', async () => {
    // Another place this screen draws a root's path: `RemoveRoot`'s confirm
    // button says "Remove " and then the whole of it (O180).
    await page.viewport(390, 844)
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [UNBROKEN_ROOT] })
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0 },
      derive: derived(),
      dry_run: true,
    })
    const host = await act(async () => {
      const mounted = mount(inShell(<Roots />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
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
    expect(pastItsCard(host)).toEqual([])
  })

  /** Types a path into the add form and submits it, as `Roots.test.tsx` does. */
  async function addPath(host: HTMLElement, path: string) {
    const box = host.querySelector('input[name="path"]') as HTMLInputElement
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, path)
      box.dispatchEvent(new Event('input', { bubbles: true }))
    })
    const form = host.querySelector('form.add-root') as HTMLFormElement
    await act(async () => {
      form.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
  }

  it('keeps the screen’s own sentences about a root inside the window', async () => {
    // The two sentences this screen says outside any card, each naming a
    // path as plain text: what the watch probe found for a root just added,
    // and the server's refusal of one. Neither is an element wider than its
    // box — the words run out of a paragraph that stays its own width — so
    // only the document can see them (O180).
    await page.viewport(390, 844)
    // An empty list first and the new root after, as the server would answer
    // either side of the add: `Roots` drops the probe's sentence when the
    // list it reloads does not hold the root it is about.
    vi.mocked(config.listRoots)
      .mockResolvedValueOnce({ roots: [] })
      .mockResolvedValue({ roots: [UNBROKEN_ROOT] })
    vi.mocked(config.addRoot).mockResolvedValueOnce({
      root: UNBROKEN_ROOT,
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    const host = await act(async () => {
      const mounted = mount(inShell(<Roots />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await addPath(host, UNBROKEN_ROOT.path)
    await document.fonts.ready
    expect(host.querySelector('p.probe')?.textContent).toContain(
      UNBROKEN_ROOT.path,
    )
    assertNoSidewaysScroll(host)

    vi.mocked(config.addRoot).mockRejectedValueOnce(
      new Error(`/api/library/roots: 400 ${UNBROKEN_ROOT.path} cannot be read`),
    )
    await addPath(host, UNBROKEN_ROOT.path)
    expect(
      [...host.querySelectorAll('.roots > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([`/api/library/roots: 400 ${UNBROKEN_ROOT.path} cannot be read`])
    // The probe's sentence is gone by now — a new submit clears it — so
    // what the document measures here is the refusal alone.
    expect(host.querySelector('p.probe')).toBeNull()
    assertNoSidewaysScroll(host)
  })

  /**
   * A scan report in a root's card, the way `RootRow.tsx` mounts one: a bare
   * `.root-row` `<li>` in the list, holding the `unreadable` block these two
   * cases are about. The roots case above mocks `scanStatus` with
   * `report: null`, so it lays out no report at all.
   *
   * **The card is what makes the report narrow enough to matter.** Its
   * padding and border leave `.unreadable-causes` a 324px row at 390.
   * Mounted bare inside `<main>` that row is 358px, which has room for the
   * 256px floor `admin.css`'s comment on `.scan-report > section > dl`
   * describes, and that rule's defect shows nowhere — measured with the
   * defect put back, for O177.
   */
  async function reportInACard(
    unreadable: Unreadable,
    scan: Partial<Walk> = {},
  ) {
    return await act(async () => {
      const mounted = mount(
        inShell(
          <section className="roots">
            <ul className="root-list">
              <li className="root-row">
                <ScanReport
                  outcome={finished({
                    scan: walk(scan),
                    derive: derived({ unreadable }),
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
  }

  it("keeps the scan report's list of causes inside its row", async () => {
    // **This is what holds `.scan-report > section > dl`**, C1 of the
    // whole-branch review over ⑤b. Put back to `.scan-report dl`, that rule
    // reaches `UnreadableCauses`'s own nested `<dl>` and makes it a grid of
    // one 256px column whatever it holds. Measured here with the defect put
    // back and the fonts in: the list ends 19px past the row it sits in and
    // 2px past the card's border, and the `<code>` holding `rar` with it.
    //
    // **The document does not scroll for it** — the card sits 16px inside
    // `main` — so `assertNoSidewaysScroll` reads 390 of 390 with the defect
    // and without, which is why the case below never held this rule (O177).
    // The assertion is on the list itself.
    //
    // **No cause in this fixture names a path, on purpose**: a path is the
    // other thing that has pushed this list past its row (O178, held by the
    // cases below), and here the rule is the only thing that can.
    await page.viewport(390, 844)
    const host = await reportInACard({
      total: 26,
      refused_format: 15,
      by_format: { '7z': 3, rar: 12 },
      uncounted: [],
      vanished: [],
      no_pages: 11,
    })
    // So that the figures above are the ones a failure prints. The verdict
    // does not wait on it: in the fallback font the list ends 28px past its
    // row with the defect, and on it without.
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
    // **This was written on ⑤b to hold `.scan-report > section > dl` and
    // never did**: with that rule's defect put back the document reads 390
    // of 390 here, as it does without (O177). The case above holds the rule.
    //
    // **What it holds is that a report naming paths stays inside its card**
    // (O178). Until the two rules under `.scan-report .paths` in `admin.css`,
    // the causes that name a path made the list wider than its row: a path
    // does not break at a slash, and a label and its list shared one line
    // whatever the list held. With the fonts in, the list ended 14px past
    // the card's border and under 2px short of the viewport's edge, so the
    // document did not scroll and this case, which asked only the document,
    // passed; in the fallback font it ended 12px past the edge, which is
    // what this case failed on, run alone and now and then inside a whole
    // run, until it waited for the fonts. It asks the card now.
    //
    // Fixture copied from `ScanReport.test.tsx`'s "says why each unreadable
    // entry was skipped, cause by cause".
    await page.viewport(390, 844)
    const host = await reportInACard({
      total: 29,
      refused_format: 15,
      by_format: { '7z': 3, rar: 12 },
      uncounted: ['/srv/manga/Bisque Doll/Bisque Doll.cbz'],
      vanished: ['/srv/manga/Gone - v01', '/srv/manga/Gone - v02'],
      no_pages: 11,
    })
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
  })

  /** The one file the real library's scan left uncounted: longer than any row here. */
  const LONG_PATH =
    '/home/davey/temp/manga-test/Volumes/Bisque Doll de Chu ♡/Bisque Doll de Chu ♡.cbz'

  /** A path with nowhere to break: no space and no hyphen, and wider than the window. */
  const UNBROKEN_PATH = `/srv/manga/${'a_directory_with_no_break_in_it_'.repeat(2)}/v01.cbz`

  /**
   * Every element in the card whose box ends past the card's border, by tag
   * and class — so a failure names what stuck out. The document cannot stand
   * in for this: the card sits 16px inside `main`, and a list 14px past the
   * card's border scrolls nothing (O177, O178).
   *
   * **Boxes, and so not words.** A path in a `<code>` is seen, because the
   * `<code>`'s box is the path's. Plain text that runs out of a paragraph is
   * not: the paragraph keeps its width and nothing here is past the card.
   * Only the document sees that, which is why the cases about a sentence ask
   * `assertNoSidewaysScroll`.
   */
  function pastItsCard(host: HTMLElement): string[] {
    const card = host.querySelector('.root-row') as HTMLElement
    const edge = card.getBoundingClientRect().right + 0.5
    return [...card.querySelectorAll('*')]
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

  it('keeps a path longer than its row inside the card, in the faces and in their fallbacks', async () => {
    await page.viewport(390, 844)
    const host = await reportInACard(
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
    expect(pastItsCard(host)).toEqual([])

    // The control that the fallback took: a label is wider in it. This
    // half is here for the state O177's case used to fail in.
    const label = host.querySelector('.unreadable-causes > dt') as HTMLElement
    const inTheFaces = label.getBoundingClientRect().width
    withFallbackFonts()
    expect(label.getBoundingClientRect().width).toBeGreaterThan(inTheFaces)
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
  })

  it('drops a list under its label when it does not fit beside it, and only then', async () => {
    await page.viewport(390, 844)
    const host = await reportInACard(
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
    const host = await reportInACard(NOTHING_UNREADABLE, {
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
    // `WalkSection` draws it as plain text in a paragraph inside the card.
    await page.viewport(390, 844)
    const host = await reportInACard(NOTHING_UNREADABLE, {
      refused: `${UNBROKEN_ROOT.path} scanned empty while the library holds 12 entries for it — refusing to apply.`,
    })
    await document.fonts.ready
    expect(host.querySelector('.refused')?.textContent).toContain(
      UNBROKEN_ROOT.path,
    )
    assertNoSidewaysScroll(host)
  })

  it('keeps a failed watch probe’s reason inside the card', async () => {
    // The card's `watch` fact is the probe's own sentence once a root has
    // just been added, and `watch.rs` puts the root's path in it.
    await page.viewport(390, 844)
    vi.mocked(config.listRoots)
      .mockResolvedValueOnce({ roots: [] })
      .mockResolvedValue({ roots: [UNBROKEN_ROOT] })
    const reason = `watching ${UNBROKEN_ROOT.path}: No space left on device (os error 28)`
    vi.mocked(config.addRoot).mockResolvedValueOnce({
      root: UNBROKEN_ROOT,
      watching: false,
      watch_reason: reason,
      interval_secs: 86400,
    })
    const host = await act(async () => {
      const mounted = mount(inShell(<Roots />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await addPath(host, UNBROKEN_ROOT.path)
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.root-row .regime dd')].map(
        (el) => el.textContent,
      ),
    ).toContain(`probe failed: ${reason}`)
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
  })

  it('keeps a scan that ran and failed inside the window', async () => {
    // `ScanReport` draws a failed scan as one paragraph, and the server's
    // sentence for it names the root's path.
    await page.viewport(390, 844)
    const why = `root 3 (${UNBROKEN_ROOT.path}) scanned, but deriving its structure afterward failed: database is locked`
    const host = await act(async () => {
      const mounted = mount(
        inShell(
          <section className="roots">
            <ul className="root-list">
              <li className="root-row">
                <ScanReport outcome={{ error: why }} />
              </li>
            </ul>
          </section>,
        ),
      )
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.scan-report > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([why])
    assertNoSidewaysScroll(host)
  })

  it('keeps a turned-down “Scan now”’s sentence inside the window', async () => {
    // The card's own alert, for a "Scan now" the server turned down with a
    // sentence that names the root's path — as its 409 for a scan already
    // running does.
    await page.viewport(390, 844)
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [UNBROKEN_ROOT] })
    const why = `/api/library/roots/3/scan: 500 ${UNBROKEN_ROOT.path} could not be scanned`
    vi.mocked(config.startScan).mockRejectedValueOnce(new Error(why))
    const host = await act(async () => {
      const mounted = mount(inShell(<Roots />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await act(async () => {
      const scan = [...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Scan now',
      )
      scan?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await document.fonts.ready
    expect(
      [...host.querySelectorAll('.root-row > [role="alert"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([why])
    assertNoSidewaysScroll(host)
  })

  it('keeps a long path inside the card in a removal report, outside any scan report', async () => {
    await page.viewport(390, 844)
    const host = await act(async () => {
      const mounted = mount(
        inShell(
          <section className="roots">
            <ul className="root-list">
              <li className="root-row">
                <div className="removal-report">
                  <DeriveSection
                    derive={derived({
                      unreadable: {
                        total: 1,
                        refused_format: 0,
                        by_format: {},
                        uncounted: [UNBROKEN_PATH],
                        vanished: [],
                        no_pages: 0,
                      },
                    })}
                  />
                </div>
              </li>
            </ul>
          </section>,
        ),
      )
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
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

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
import * as librariesApi from './libraries'
import * as backupApi from './backup'
import { FolderPicker } from './FolderPicker'
import { Libraries } from './Libraries'
import { Users } from './Users'
import * as usersApi from './users'
import { DeriveSection, ScanReport } from './ScanReport'
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

describe('the Configuration screens at 390px', () => {
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

  /** Both, not just the document — see this describe block's own comment. */
  function assertNoSidewaysScroll(host: HTMLElement) {
    const doc = document.documentElement
    const main = host.querySelector('main')
    expect(main).not.toBeNull()
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
    expect(main!.scrollWidth).toBeLessThanOrEqual(main!.clientWidth)
  }

  it('does not scroll sideways on the libraries screen, and wraps the rename form inside its card', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(longPathRoot),
    )
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    assertNoSidewaysScroll(host)
    // The library's three actions share a row that must wrap: at 320px they
    // ask for more than is left them in every face. The fallback faces are
    // forced so that the result does not depend on whether the web faces have
    // loaded.
    withFallbackFonts()
    await page.viewport(320, 844)
    await press(host.querySelector('button[name="rename-library"]') as Element)
    assertNoSidewaysScroll(host)
    for (const selector of ['.library-card > .actions', '.rename-library']) {
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

  it('keeps a root’s own path inside its card when the path has nowhere to break', async () => {
    // `longPathRoot` above breaks at its hyphen and fits. A root whose path
    // has a longer run than the card is wide used to take the card's heading
    // out of the card, and the document with it — found on a scratch library
    // under `target/`, whose path is that long (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
    // The path is the card's heading and takes no margin of its own.
    const heading = host.querySelector('.root-row > h4') as HTMLElement
    expect(getComputedStyle(heading).marginTop).toBe('0px')
  })

  it('keeps the removal’s confirm button inside the card for the same path', async () => {
    // Another place this screen draws a root's path: `RemoveRoot`'s confirm
    // button says "Remove " and then the whole of it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0 },
      derive: derived(),
      dry_run: true,
    })
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
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

  /**
   * The move dialog on a long path and a long library name (the same O180
   * defect): a folder in one library, a second library whose name has nowhere
   * to break, Move… pressed, the second chosen and the rehearsal asked for.
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
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    const card = host.querySelector('.library-card') as HTMLElement
    await act(async () => {
      const open = [...card.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      )
      open?.click()
      await Promise.resolve()
    })
    await act(async () => {
      const box = card.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '2')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(card.querySelector('button[name="rehearse"]') as Element)
    await document.fonts.ready
    return { host, card }
  }

  it('keeps a move’s refusal and the list of volumes it names inside the card', async () => {
    const volumes = Array.from({ length: 20 }, (_, i) =>
      `${String(i).padStart(2, '0')}_`.padEnd(80, 'v'),
    )
    const { host, card } = await rehearsedMove(
      new Error(
        `/api/library/roots/3/move?dry_run=true: 409 ${JSON.stringify({
          error: `${UNBROKEN_NAME} already holds volumes of ${UNBROKEN_ROOT.path}`,
          volumes,
        })}`,
      ),
    )
    const list = card.querySelector('.move-volumes') as HTMLDetailsElement
    list.open = true
    await document.fonts.ready
    expect(list.querySelectorAll('li')).toHaveLength(20)
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
  })

  it('keeps a move’s confirm button inside the card for the same path and name', async () => {
    const { host, card } = await rehearsedMove(
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
        },
        derive: derived(),
        dry_run: true,
      }),
    )
    const confirm = card.querySelector('.move-root .confirm')
    expect(confirm?.textContent).toBe(
      `Move ${UNBROKEN_ROOT.path} to ${UNBROKEN_NAME}`,
    )
    assertNoSidewaysScroll(host)
    expect(pastItsCard(host)).toEqual([])
  })

  it('draws a move’s confirm as the action: --accent on its border and text, and heavier', async () => {
    const { card } = await rehearsedMove(
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
        },
        derive: derived(),
        dry_run: true,
      }),
    )
    const confirm = card.querySelector('.move-root .confirm') as HTMLElement
    const rehearse = card.querySelector(
      'button[name="rehearse"]',
    ) as HTMLElement
    // The token as the browser resolves it, whichever scheme is in force.
    const probe = document.createElement('span')
    probe.style.color = 'var(--accent)'
    card.append(probe)
    const accent = getComputedStyle(probe).color
    probe.remove()
    const style = getComputedStyle(confirm)
    expect(style.borderTopColor).toBe(accent)
    expect(style.color).toBe(accent)
    expect(style.fontWeight).toBe('600')
    expect(getComputedStyle(rehearse).borderTopColor).not.toBe(accent)
  })

  /** Makes a library through the form: a name, one folder chosen, submitted. */
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
    await press(form.querySelector('button[name="pick"]') as Element)
    await submit(form)
  }

  it('keeps the screen’s own sentence about a refused library inside the window', async () => {
    // The one sentence this screen says outside any card, naming a path as
    // plain text: the server's refusal of a library. It is not an element
    // wider than its box — the words run out of a paragraph that stays its
    // own width — so only the document can see it (O180).
    await page.viewport(390, 844)
    vi.mocked(librariesApi.createLibrary).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries: 400 {"error":"${UNBROKEN_ROOT.path}: cannot be read: Permission denied (os error 13)"}`,
      ),
    )
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
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
    // The folder that was refused stays on the list, its path breaking and its
    // button whole on the row's far edge.
    const row = host.querySelector('.chosen-folders li') as HTMLElement
    const button = row.querySelector('button') as HTMLElement
    expect(button.getBoundingClientRect().height).toBeLessThan(44)
    expect(button.getBoundingClientRect().right).toBeCloseTo(
      row.getBoundingClientRect().right,
      0,
    )
  })

  /**
   * A scan report in a root's card, the way `RootRow.tsx` mounts one: a bare
   * `.root-row` `<li>` in the list, holding the `unreadable` block these two
   * cases are about. The libraries case above mocks `scanStatus` with
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
          <section className="libraries">
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
    // Inside a folder's card a report's headings step off what is above them
    // and sit near what they head.
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
    // The card's `watch` fact is the probe's own sentence once a folder has
    // just been added, and `watch.rs` puts the path in it.
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
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await makeLibrary(host, 'Manga', UNBROKEN_ROOT.path)
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
    const why = `${UNBROKEN_ROOT.path} was scanned, but deriving its structure afterward failed: database is locked`
    const host = await act(async () => {
      const mounted = mount(
        inShell(
          <section className="libraries">
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

  it('keeps a turned-down “Scan folder”’s sentence inside the window', async () => {
    // The card's own alert, for a "Scan folder" the server turned down with a
    // sentence that names the root's path — as its 409 for a scan already
    // running does.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(
      holding(UNBROKEN_ROOT),
    )
    const why = `/api/library/roots/3/scan: 500 ${UNBROKEN_ROOT.path} could not be scanned`
    vi.mocked(config.startScan).mockRejectedValueOnce(new Error(why))
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await act(async () => {
      // The folder's own button: the library's, above it, is another route's.
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

  it('keeps a long path inside the card in a removal report, outside any scan report', async () => {
    await page.viewport(390, 844)
    const host = await act(async () => {
      const mounted = mount(
        inShell(
          <section className="libraries">
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

  /** The library of one folder, named `name`, mounted in the shell. */
  async function libraryNamed(name: string, folder: config.Root) {
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [{ ...holding(folder).libraries[0], name }],
    })
    return await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
  }

  it('keeps a library whose name has nowhere to break inside the window', async () => {
    // A library is named by a person, and a name need not hold a space. It is
    // drawn as the card's heading and, below, in the form that renames it.
    await page.viewport(390, 844)
    const host = await libraryNamed(UNBROKEN_NAME, UNBROKEN_ROOT)
    await document.fonts.ready
    const heading = host.querySelector('.library-card > h3') as HTMLElement
    expect(heading.textContent).toBe(UNBROKEN_NAME)
    assertNoSidewaysScroll(host)
    expect(heading.scrollWidth).toBeLessThanOrEqual(heading.clientWidth)
    await press(host.querySelector('button[name="rename-library"]') as Element)
    assertNoSidewaysScroll(host)
  })

  it('keeps a chosen folder’s button whole, on the row’s far edge', async () => {
    // The path gives way and the button does not: a word broken over two
    // lines, and a width that differs from one row to the next, read as two
    // controls.
    await page.viewport(390, 844)
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    const form = host.querySelector('form.add-library') as HTMLFormElement
    for (const path of [
      '/srv/a',
      `/srv/${'a_directory_with_no_break_'.repeat(6)}`,
    ]) {
      await typeInto(
        form.querySelector('input[name="path"]') as HTMLInputElement,
        path,
      )
      await press(form.querySelector('button[name="pick"]') as Element)
    }
    await document.fonts.ready
    const rows = [...form.querySelectorAll('.chosen-folders li')]
    expect(rows).toHaveLength(2)
    const widths = rows.map((row) => {
      const button = row.querySelector('button') as HTMLElement
      const at = button.getBoundingClientRect()
      expect(at.height).toBeLessThan(44)
      expect(at.right).toBeCloseTo(row.getBoundingClientRect().right, 0)
      return at.width
    })
    expect(widths[1]).toBeCloseTo(widths[0], 0)
    assertNoSidewaysScroll(host)
  })

  it('keeps the folder browser inside the card', async () => {
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
    await press(
      host.querySelector(
        '.library-card > .add-root button[name="browse"]',
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
          `.library-card > .add-root button[name="${name}"]`,
        ) as HTMLElement
      ).getBoundingClientRect(),
    )
    expect(buttons[1].top).toBe(buttons[0].top)
  })

  it('keeps the folder browser’s own path inside it, whatever paragraph rule is above it', async () => {
    // Under `.libraries` the screen's paragraph rule breaks the path as well,
    // so the card's case cannot tell whether `.folder-browser .where` does.
    // Mounted bare, only that rule is left to.
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

  it('keeps the library’s removal inside the window', async () => {
    await page.viewport(390, 844)
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0 },
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
    // The dialog is followed by the card it would remove, and is clear of it.
    const next = host.querySelector('.library-card > .root-list') as HTMLElement
    expect(
      next.getBoundingClientRect().top - box.getBoundingClientRect().bottom,
    ).toBeGreaterThanOrEqual(16)
  })

  it('keeps what a removal’s re-derive found inside the window', async () => {
    // The screen draws the report itself, above the list, once the library
    // that was removed is no longer there to hold it.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0 },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 1, positions: 0, notes: 0 },
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
      '.libraries > .removal-report',
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
    // The removed card is lower down than the report, which is above the
    // list. Focus alone scrolls a report taller than the window only as far
    // as its foot, so the head, whose opening sentence says whose it is,
    // has to be put in view as well.
    await page.viewport(390, 844)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        {
          ...holding(longPathRoot, root({ id: 4, path: '/srv/b' }))
            .libraries[0],
        },
        {
          id: 2,
          name: 'Comics',
          works: 0,
          books: 0,
          folders: [root({ id: 5, path: '/srv/comics' })],
        },
      ],
    })
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: derived({ lost_positions: 4 }),
      dry_run: false,
    })
    const host = await act(async () => {
      const mounted = mount(inShell(<Libraries />))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    // The document scrolls here, not `main`: `assertNoSidewaysScroll` reads
    // both for that reason.
    const doc = document.documentElement
    expect(doc.scrollHeight).toBeGreaterThan(doc.clientHeight * 1.5)
    window.scrollTo(0, doc.scrollHeight)
    const remove = host.querySelectorAll('button[name="remove-library"]')[1]
    await press(remove)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [holding(longPathRoot).libraries[0]],
    })
    await press(host.querySelector('.remove-library .confirm') as Element)
    const report = host.querySelector(
      '.libraries > .removal-report',
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
  })

  it('does not scroll sideways on the users screen', async () => {
    await page.viewport(390, 844)
    // The grant editor draws a library's folders, so the path that must wrap
    // is the one that is on screen.
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [libraryOf(UNBROKEN_ROOT)],
    })
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user({ libraries: [{ id: 70, name: 'Manga' }] })],
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
      users: [user({ libraries: [{ id: 70, name: 'Manga' }] })],
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
    const host = await act(async () => {
      const mounted = mount(<Libraries />)
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

  async function shown(ui: React.ReactNode = <Libraries />) {
    await page.viewport(1280, 900)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue(TWO)
    const host = await act(async () => {
      const mounted = mount(ui)
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    return host
  }

  const box = (el: Element) => el.getBoundingClientRect()

  it('sets each library on the measure, parts it from the next by a rule, and boxes the form', async () => {
    const host = await shown()
    const [first, second] = [...host.querySelectorAll('.library-card')]
    expect(box(first).width).toBeLessThanOrEqual(40 * 16 + 0.5)
    expect(getComputedStyle(first).borderTopWidth).toBe('0px')
    expect(getComputedStyle(second).borderTopWidth).toBe('1px')
    expect(getComputedStyle(second).paddingTop).toBe('16px')

    // What a library holds is quiet: the colour and size of the folder
    // card's own labels.
    const quiet = getComputedStyle(host.querySelector('.regime dt') as Element)
    for (const selector of ['.holds', '.no-readable']) {
      const sentence = getComputedStyle(
        host.querySelector(`.library-card ${selector}`) as Element,
      )
      expect(sentence.color, selector).toBe(quiet.color)
      expect(sentence.fontSize, selector).toBe(quiet.fontSize)
    }

    // The form is boxed, as a folder's card is: a library is not.
    const form = host.querySelector('form.add-library') as HTMLElement
    expect(getComputedStyle(form).borderTopWidth).toBe('1px')
    expect(getComputedStyle(form).paddingTop).toBe('16px')
    // Its one action keeps its own width and does not stretch across.
    const submit = form.querySelector('button[type="submit"]') as HTMLElement
    expect(box(submit).width).toBeLessThan(box(form).width / 2)

    // The picker under a library's folders ends on the card's right edge: the
    // box takes the room the two buttons leave.
    const add = host.querySelector('.library-card > .add-root') as HTMLElement
    const browse = add.querySelector('button[name="browse"]') as HTMLElement
    expect(box(browse).right).toBeCloseTo(box(add).right, 0)
  })

  it('ends the make-a-library form where a library’s card ends', async () => {
    // The form is boxed and a card is not, so a form that adds its border and
    // padding to the measure ends past the card above it.
    const host = await shown()
    const form = host.querySelector('form.add-library') as HTMLElement
    const card = host.querySelector('.library-card') as HTMLElement
    expect(box(form).right).toBeCloseTo(box(card).right, 0)
    expect(box(form).width).toBeLessThanOrEqual(40 * 16 + 0.5)
  })

  it('draws the two boxes of a folder’s first row at one height', async () => {
    // A neighbour's textarea, and its hint, made the row taller than a box
    // needs; a box in a grid stretches to its row unless told not to.
    const host = await shown()
    const edition = host.querySelector('input[name="edition_key"]') as Element
    const language = host.querySelector(
      'input[name="default_language"]',
    ) as Element
    expect(box(language).height).toBe(box(edition).height)
  })

  it('puts the removal’s report on the measure, as a grid', async () => {
    const host = await shown()
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 5, positions: 0, notes: 0 },
      derive: derived(),
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 5, positions: 0, notes: 0 },
      derive: derived({ lost_positions: 4 }),
      dry_run: false,
    })
    await press(host.querySelector('button[name="remove-library"]') as Element)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [TWO.libraries[1]],
    })
    await press(host.querySelector('.remove-library .confirm') as Element)
    const report = host.querySelector('.libraries > .removal-report') as Element
    // The report ends where a library's card does.
    expect(box(report).right).toBeCloseTo(
      box(host.querySelector('.library-card') as Element).right,
      0,
    )
    // Its lists are the two-column grid a folder's scan report has, so a
    // report of this many rows is not a page tall.
    const list = report.querySelector('.derived > dl') as Element
    expect(getComputedStyle(list).display).toBe('grid')
    expect(
      getComputedStyle(list).gridTemplateColumns.split(' ').length,
    ).toBeGreaterThan(1)
    // A heading of the report is its own size, not a library's name.
    const heading = report.querySelector('.report-heading') as Element
    expect(getComputedStyle(heading).marginTop).toBe('20px')
    expect(Number.parseFloat(getComputedStyle(heading).fontSize)).toBeLessThan(
      Number.parseFloat(
        getComputedStyle(host.querySelector('.library-card > h3') as Element)
          .fontSize,
      ),
    )
  })

  it('draws a report’s heading alike at every level its caller may pass', async () => {
    // Styled by class, not by tag: a heading is the same label whether the
    // screen holds the report under an h2 or a folder's card holds it under
    // an h4. The browser's own heading faces differ from level to level, so
    // only the class can make these equal.
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
      users: [user({ role: 'user', libraries: [{ id: 1, name: 'Manga' }] })],
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

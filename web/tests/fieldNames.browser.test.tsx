/// <reference types="@vitest/browser-playwright" />
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { cdp, page } from 'vitest/browser'
import { About } from '../src/About'
import { Account } from '../src/Account'
import * as api from '../src/api'
import * as backupApi from '../src/backup'
import { Config } from '../src/Config'
import * as config from '../src/config'
import * as curation from '../src/curation'
import { GateContext } from '../src/gate'
import * as librariesApi from '../src/libraries'
import { Licence } from '../src/Licence'
// **Required.** The capitals this file is about are `admin.css`'s, which
// reaches the app through `index.css`; without it every `.field` label is
// drawn as written and each `.field` case's first assertion fails.
import '../src/index.css'
import { ProposalRow } from '../src/ProposalRow'
import { RootRow } from '../src/RootRow'
import { Libraries } from '../src/Libraries'
import { RuleForm } from '../src/RuleForm'
import * as rulesApi from '../src/rules'
import * as splitApi from '../src/splitEditions'
import type { Rule } from '../src/rules'
import { answerAbout } from './support/aboutPage'
import { emptyDerive } from './support/derive'
import { fields } from './support/fields'
import { press } from './support/input'
import { licenceTextsLanded } from './support/licence'
import { mount } from './support/mount'
import * as usersApi from '../src/users'
import { Users } from '../src/Users'

/**
 * O175: what Chromium's own accessibility tree names the controls under a
 * `.field` label, read over the DevTools protocol.
 *
 * `markup.browser.test.tsx` pins names as Playwright computes them.
 * `admin.css` draws the `.field` labels mounted here as upper-case
 * micro-labels through `text-transform`; Chromium's tree would name a control
 * in those capitals — "EDITION KEY", "NAME" — where Playwright's computation
 * gives the words as written. Each control carries its label's words in an
 * `aria-label`, and this file holds what the tree makes of that.
 *
 * **A `.field` case first asserts that its labels are drawn in capitals.** A
 * label drawn as written names its control as written with no `aria-label`
 * at all, so without that the names below could pass having tested nothing.
 *
 * **A settings row's label is drawn as written**: `settings.css` gives
 * `.settings-row-label` no `text-transform`. So a row case holds what O175
 * is for rather than the capitals: it first reads every row's label, tied to
 * its control by `for`, and then holds each control's name in Chromium's
 * tree to a list written out: those words in their own case or, where its
 * `aria-label` says more, those words and then the rest. An `aria-label` in
 * other words or another case fails it, and so does a sheet drawing the
 * label in capitals over a control left without its `aria-label`, which
 * Chromium's tree then names in those capitals.
 *
 * One `it()` per form or card: `support/mount.ts` holds one root, for the
 * reason `Filmstrip.browser.test.tsx` gives.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

vi.mock('../src/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api')>()),
  listKeys: vi.fn(),
  listSessions: vi.fn(),
}))

vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
}))

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  listRoots: vi.fn(),
  scanStatus: vi.fn(),
}))

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  removeLibrary: vi.fn(),
}))

vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
  proposalDetail: vi.fn(),
  resolution: vi.fn(),
}))

vi.mock('../src/rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
}))

vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))

vi.mock('../src/splitEditions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/splitEditions')>()),
  listSplitEditions: vi.fn(),
}))

const root: librariesApi.LibraryDir = {
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
}

/** The one library the Libraries screen's cases list, holding `root`. */
const MANGA: librariesApi.Library = {
  id: 1,
  name: 'Manga',
  works: 0,
  books: 0,
  dirs: [root],
}

/**
 * The Libraries screen at `entry`, as Configuration routes it, without
 * Configuration's own heading and tabs: what is read is the page's.
 */
const librariesAt = (entry: string) => {
  // One element for both routes, as Configuration gives it.
  const screen = <Libraries />
  return (
    <MemoryRouter initialEntries={[entry]}>
      <Routes>
        <Route path="/config/libraries/*" element={screen} />
        <Route path="/config/new-library" element={screen} />
      </Routes>
    </MemoryRouter>
  )
}

/** The Users screen at `entry`, as Configuration routes it. */
const usersAt = (entry: string) => {
  // One element for both routes, as Configuration gives it.
  const screen = <Users />
  return (
    <MemoryRouter initialEntries={[entry]}>
      <Routes>
        <Route path="/config/users/*" element={screen} />
        <Route path="/config/new-account" element={screen} />
      </Routes>
    </MemoryRouter>
  )
}

/** The account signed in: `user()`'s, an administrator. */
const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

/**
 * Configuration at `entry`, its hidden h1 and its tabs above the page, so
 * that a page's outline is read under the heading it sits beneath.
 */
const configAt = (entry: string) => (
  <MemoryRouter initialEntries={[entry]}>
    <GateContext value={{ who, signOut: () => undefined }}>
      <Routes>
        <Route path="/config/*" element={<Config />} />
      </Routes>
    </GateContext>
  </MemoryRouter>
)

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

const rule: Rule = {
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga \\(RAW\\)/.*',
  match_on: 'path',
  set_series: null,
  set_edition_key: 'raw',
  set_kind: null,
  set_language: null,
  note: null,
}

/** One end of a proposal, with no volume scanned so the panel asks for no page. */
const side = (series_id: number, title: string): curation.Side => ({
  series_id,
  title,
  directories: [`/srv/manga/Publisher/${title}/`],
  volume_count: 14,
  volumes: [1, 2, 3],
  root_name: 'Publisher',
  first_volume: null,
})

const proposal: curation.Proposal = {
  id: 2,
  kind: 'edition-of',
  subject_id: 3,
  target_id: 4,
  score: 2,
  reason: 'the suffix matches the edition vocabulary',
  state: 'open',
  created_rule_id: null,
}

const detail: curation.Detail = {
  proposal,
  subject: side(3, 'Go-Toubun no Hanayome - Full Color Edition'),
  target: side(4, 'Go-Toubun no Hanayome'),
  reasons: [{ text: 'both hold 14 volume(s)', sign: '+' }],
  rule: {
    root_id: 1,
    kind: 'prefix',
    pattern: 'Publisher/Go-Toubun no Hanayome - Full Color Edition/',
    match_on: 'path',
    set_series: 'Go-Toubun no Hanayome',
    set_edition_key: 'full-color-edition',
    set_kind: 'official',
    set_language: null,
    note: 'accepted from proposal 2',
  },
}

/** Mount, and let what the screen reads on mount land. */
async function shown(ui: React.ReactNode): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(ui)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/**
 * Each `.field` label's words as written, and the `text-transform` they are
 * drawn with — the guard every `.field` case opens on.
 */
const drawn = (host: HTMLElement): Array<[string, string]> =>
  fields(host.querySelectorAll('label.field')).map(({ written, words }) => [
    written,
    getComputedStyle(words).textTransform,
  ])

/**
 * Each settings row's label words as written, read through the `for` that
 * ties it to its control: the guard every row case opens on, which proves
 * the names below are read against every row.
 */
const rowWords = (host: HTMLElement): string[] =>
  fields(host.querySelectorAll('label.settings-row-label')).map(
    ({ written }) => written,
  )

/** The roles Chromium's tree gives a text box, a `<select>` and a number box. */
const CONTROLS = ['textbox', 'combobox', 'spinbutton']

/** `CONTROLS` and a checkbox: what a settings row's label can name. */
const ROW_CONTROLS = [...CONTROLS, 'checkbox']

/**
 * `[role, name]` for every node of one of `roles` in this file's frame, as
 * Chromium's accessibility tree has them and in that tree's order.
 *
 * **This file's frame, found by its address.** The session belongs to the
 * page Vitest runs its test files in, each in an `<iframe>` of its own, so
 * the frame is picked out rather than taken to be the only one.
 *
 * **Walked from the root rather than read off in the order sent**, which is
 * level by level and not the order on screen.
 */
async function named(roles: string[]): Promise<Array<[string, string]>> {
  return (await treeOf(roles)).map(({ role, name }) => [role, name])
}

/** One node of Chromium's tree: its role, its name and, for a heading, its level. */
interface Node {
  role: string
  name: string
  level: number | undefined
}

async function treeOf(roles: string[]): Promise<Node[]> {
  const session = cdp()
  await session.send('Accessibility.enable')
  try {
    const { frameTree } = await session.send('Page.getFrameTree')
    const mine = (frameTree.childFrames ?? []).filter(
      ({ frame }) => frame.url === location.href,
    )
    if (mine.length !== 1) {
      throw new Error(`${mine.length} frames are at ${location.href}`)
    }
    const { nodes } = await session.send('Accessibility.getFullAXTree', {
      frameId: mine[0].frame.id,
    })
    const byId = new Map(nodes.map((node) => [node.nodeId, node]))
    const found: Node[] = []
    const walk = (id: string) => {
      const node = byId.get(id)
      if (!node) return
      const role = String(node.role?.value)
      if (!node.ignored && roles.includes(role)) {
        const level = node.properties?.find((p) => p.name === 'level')
        found.push({
          role,
          name: String(node.name?.value ?? ''),
          level: level === undefined ? undefined : Number(level.value.value),
        })
      }
      for (const child of node.childIds ?? []) walk(child)
    }
    for (const node of nodes) {
      if (node.parentId === undefined) walk(node.nodeId)
    }
    return found
  } finally {
    await session.send('Accessibility.disable')
  }
}

/** Every text box, `<select>` and number box, by `named`. */
const controls = () => named(CONTROLS)

/** Every control a settings row's label can name, by `named`. */
const rowControls = () => named(ROW_CONTROLS)

/**
 * An expanded `ProposalRow` for `detail`, with its accept form drawn: the row
 * is what renders `.proposal-detail`, and the form is drawn inside it.
 */
async function expandedProposal(): Promise<HTMLElement> {
  vi.mocked(curation.proposalDetail).mockResolvedValue(detail)
  vi.mocked(curation.resolution).mockResolvedValue({
    also_resolves: [],
    collateral: [],
    targets_reached: [],
    entries_matched: 14,
    subject_reached: true,
    works_moved: 14,
  })
  const host = await shown(
    <ProposalRow
      proposal={{
        ...proposal,
        subject_title: 'Go-Toubun no Hanayome - Full Color Edition',
        target_title: 'Go-Toubun no Hanayome',
      }}
      expanded={true}
      onToggle={() => {}}
      onChanged={() => {}}
    />,
  )
  // Past the form's own debounce, so its sweep lands inside an `act`.
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 400)
    })
  })
  return host
}

describe('the name Chromium gives a control under a field label — O175', () => {
  it('names the add-account form’s controls as written', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    const host = await shown(usersAt('/config/new-account'))
    expect(rowWords(host)).toEqual([
      'Name',
      'Password',
      'Confirm password',
      'Role',
    ])
    expect(await rowControls()).toEqual([
      ['textbox', 'Name'],
      ['textbox', 'Password'],
      ['textbox', 'Confirm password'],
      ['combobox', 'Role'],
    ])
  })

  it('names an account page’s controls as written', async () => {
    const sam = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user(), sam] })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    const host = await shown(usersAt('/config/users/sam'))
    expect(rowWords(host)).toEqual([
      'Role',
      'Can download',
      'Can stream',
      'Can stamp',
      'New password',
      'Confirm password',
    ])
    expect(await rowControls()).toEqual([
      ['combobox', 'Role'],
      ['checkbox', 'Can download'],
      ['checkbox', 'Can stream'],
      ['checkbox', 'Can stamp'],
      ['textbox', 'New password'],
      ['textbox', 'Confirm password'],
    ])
  })

  it('names the make-a-library form’s boxes as written', async () => {
    // The make page, whose form's two boxes are the only ones on it.
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    const host = await shown(librariesAt('/config/new-library'))
    expect(rowWords(host)).toEqual(['Name', 'Directory path'])
    expect(await rowControls()).toEqual([
      ['textbox', 'Name'],
      ['textbox', 'Directory path'],
    ])
  })

  it('names a directory page’s controls as written', async () => {
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    const host = await shown(
      <RootRow root={root} probe={undefined} onChanged={() => {}} />,
    )
    expect(rowWords(host)).toEqual([
      'Scan interval',
      'Exclude globs',
      'Keeps documents under .taiyaku/',
      'Edition key',
      'Default kind',
      'Default language',
    ])
    expect(await rowControls()).toEqual([
      // Two `aria-label`s say more than their row's label, the unit and the
      // syntax, after its words.
      ['spinbutton', 'Scan interval (seconds)'],
      ['textbox', 'Exclude globs (one gitignore pattern per line)'],
      ['checkbox', 'Keeps documents under .taiyaku/'],
      ['textbox', 'Edition key'],
      ['textbox', 'Default kind'],
      ['textbox', 'Default language'],
    ])
  })

  it('names the accept form’s controls as written', async () => {
    const host = await expandedProposal()
    expect(drawn(host)).toEqual([
      ['Pattern', 'uppercase'],
      ['Match as', 'uppercase'],
      ['Edition key', 'uppercase'],
      ['Edition kind', 'uppercase'],
      ['Language', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['textbox', 'Pattern'],
      ['combobox', 'Match as'],
      ['textbox', 'Edition key'],
      ['combobox', 'Edition kind'],
      ['textbox', 'Language'],
    ])
  })

  it('names the rule form’s controls as written', async () => {
    const host = await shown(
      <RuleForm rule={rule} roots={[root]} onCommit={() => {}} />,
    )
    expect(drawn(host)).toEqual([
      ['kind', 'uppercase'],
      ['match on', 'uppercase'],
      ['pattern', 'uppercase'],
      ['series', 'uppercase'],
      ['edition', 'uppercase'],
      ['kind', 'uppercase'],
      ['language', 'uppercase'],
      ['note', 'uppercase'],
      ['scope', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['combobox', 'kind'],
      ['combobox', 'match on'],
      ['textbox', 'pattern'],
      ['textbox', 'series'],
      ['textbox', 'edition'],
      ['textbox', 'kind'],
      ['textbox', 'language'],
      ['textbox', 'note'],
      ['combobox', 'scope'],
    ])
  })

  it('names the account page’s boxes as written', async () => {
    vi.mocked(api.listKeys).mockResolvedValue([])
    vi.mocked(api.listSessions).mockResolvedValue([])
    answerAbout()
    const host = await shown(
      <Account placement="page" onSignedOut={() => {}} />,
    )
    // `/api/about`'s answer, which draws the catalog's address's row, lands
    // inside `act`.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
    expect(rowWords(host)).toEqual([
      'Current password',
      'New password',
      'Confirm password',
      'New key',
    ])
    expect(await rowControls()).toEqual([
      ['textbox', 'Current password'],
      ['textbox', 'New password'],
      ['textbox', 'Confirm password'],
      // The key's name box, named by its label and not its placeholder.
      ['textbox', 'New key'],
    ])
  })
})

/**
 * O182: the same capitals on two headings and a group. `admin.css` draws a
 * proposal's two `.lab` headings and the rule form's `sets` legend through
 * `text-transform: uppercase` as well, and Chromium's tree would name them in
 * those capitals. Each carries its words in an `aria-label`.
 *
 * Each case first asserts that the words are drawn in capitals, for the
 * reason the field cases above do.
 */
describe('the name Chromium gives a heading or a group drawn in capitals — O182', () => {
  it('names a proposal’s two headings as written', async () => {
    const host = await expandedProposal()
    expect(
      [...host.querySelectorAll('h2.lab')].map((heading) => [
        heading.textContent,
        getComputedStyle(heading).textTransform,
      ]),
    ).toEqual([
      ['Why this was proposed', 'uppercase'],
      ['The rule this writes', 'uppercase'],
    ])
    expect(await named(['heading'])).toEqual([
      // The two sides' titles, which are drawn as written.
      ['heading', 'Go-Toubun no Hanayome - Full Color Edition'],
      ['heading', 'Go-Toubun no Hanayome'],
      ['heading', 'Why this was proposed'],
      ['heading', 'The rule this writes'],
    ])
  })

  it('names the rule form’s sets group as written', async () => {
    const host = await shown(
      <RuleForm rule={rule} roots={[root]} onCommit={() => {}} />,
    )
    const legend = host.querySelector('.rule-sets > legend')
    if (!legend) throw new Error('the rule form drew no sets legend')
    expect([
      legend.textContent,
      getComputedStyle(legend).textTransform,
    ]).toEqual(['sets', 'uppercase'])
    expect(await named(['group'])).toEqual([['group', 'sets']])
  })
})

/** What every Configuration page reads on mount, answered with nothing running. */
function answerPages() {
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
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({
    libraries: [MANGA],
  })
  vi.mocked(usersApi.listUsers).mockResolvedValue({
    users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
  })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
  vi.mocked(splitApi.listSplitEditions).mockResolvedValue({
    editions: [
      {
        work: 'foo',
        title: 'Foo',
        edition: 'raw',
        pages: 5,
        members: [
          {
            path: 'Foo 01.cbz',
            pages: 2,
            directories: [{ root: 3, path: '/srv/manga', libraries: [1] }],
          },
          { path: 'Foo 02.cbz', pages: 3, directories: [] },
        ],
        libraries: [{ id: 1, name: 'Manga', members: 1, pages: 2 }],
      },
    ],
  })
  vi.mocked(rulesApi.listRules).mockResolvedValue({
    rules: [rule],
    complaints: [],
  })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [root] })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
  vi.mocked(rulesApi.previewRules).mockResolvedValue({
    impact: RULES_IMPACT,
    rows: [],
    rows_truncated: false,
    overrides: [],
  })
  vi.mocked(rulesApi.saveRules).mockResolvedValue({
    impact: RULES_IMPACT,
    rules: [rule],
    reopened_proposals: [],
    complaints: [],
    derive: emptyDerive,
  })
}

/** What the Rules screen's preview and save say the rule list does. */
const RULES_IMPACT: rulesApi.Impact = {
  series_before: 2,
  series_after: 1,
  series_merged: 1,
  works_rehomed: 3,
  works_orphaned: 0,
  proposals_reopened: 0,
  proposals_raised: 0,
}

/**
 * Each page of the Configuration screens heads itself with one h2 — the
 * table's name, the make page's, the library's or the account's name, or the
 * directory's path — beneath Configuration's hidden h1, and its sections a
 * level below. Where a window is wide enough for a page to open beside its
 * list, the two are two pages, so the outline holds two h2s, the list's
 * first. What the tree makes of them is held beside the capitals cases
 * because the same sheets draw those headings.
 */
describe('the headings of the Configuration pages, in Chromium’s own tree', () => {
  it.each([
    ['/config/libraries', [['Libraries', 2]]],
    ['/config/new-library', [['New library', 2]]],
    [
      '/config/libraries/Manga',
      [
        ['Manga', 2],
        ['Directories', 3],
        ['Removal', 3],
      ],
    ],
    [
      '/config/libraries/Manga/dirs/3',
      [
        [root.path, 2],
        ['How it is scanned', 3],
        ['How it is read', 3],
        ['Removal', 3],
      ],
    ],
    ['/config/users', [['Users', 2]]],
    ['/config/new-account', [['New account', 2]]],
    [
      '/config/users/sam',
      [
        ['sam', 2],
        ['Role & permissions', 3],
        ['Libraries', 3],
        ['Password', 3],
        ['Sessions & keys', 3],
        ['Removal', 3],
      ],
    ],
    [
      '/config/backup',
      [
        ['Backup', 2],
        ['Backups', 3],
      ],
    ],
    [
      '/config/rules',
      [
        ['Rules', 2],
        ['Preview', 3],
      ],
    ],
    [
      '/config/split-editions',
      [
        ['Split editions', 2],
        ['Foo\u00a0·\u00a0raw', 3],
      ],
    ],
  ])(
    'reads %s as one page: its own heading, and its sections beneath it',
    async (entry, page) => {
      answerPages()
      const headings = [['Configuration', 1], ...page]
      const host = await shown(configAt(entry))
      expect(
        [...host.querySelectorAll('h1, h2, h3, h4, h5, h6')].map(
          (h) => h.tagName,
        ),
      ).toEqual(headings.map(([, level]) => `H${level}`))
      expect(await treeOf(['heading'])).toEqual(
        headings.map(([name, level]) => ({ role: 'heading', name, level })),
      )
    },
  )

  // Wide enough for a page to open beside its list (spec §3): the list's
  // page and the opened one are two pages, each with its own h2, the list's
  // first as the document draws it.
  it.each([
    [
      '/config/libraries/Manga/dirs/3',
      [
        ['Manga', 2],
        ['Directories', 3],
        ['Removal', 3],
        [root.path, 2],
        ['How it is scanned', 3],
        ['How it is read', 3],
        ['Removal', 3],
      ],
    ],
    [
      '/config/users/sam',
      [
        ['Users', 2],
        ['sam', 2],
        ['Role & permissions', 3],
        ['Libraries', 3],
        ['Password', 3],
        ['Sessions & keys', 3],
        ['Removal', 3],
      ],
    ],
  ])(
    'reads %s at a wide window as its list and its page side by side, each heading its own sections',
    async (entry, pair) => {
      // The window this file's other cases are read at, given back after.
      const was = [window.innerWidth, window.innerHeight] as const
      onTestFinished(() => page.viewport(...was))
      await page.viewport(1920, 900)
      answerPages()
      const headings = [['Configuration', 1], ...pair]
      const host = await shown(configAt(entry))
      // `useFitsPair`'s observer reports once the next frames are drawn,
      // and sets its state again then: waited for inside `act`, so that
      // update lands where an `act` is waiting for it.
      await act(async () => {
        await new Promise((resolve) => requestAnimationFrame(resolve))
        await new Promise((resolve) => requestAnimationFrame(resolve))
      })
      expect(host.querySelector('.split-pair')).not.toBeNull()
      expect(
        [...host.querySelectorAll('h1, h2, h3, h4, h5, h6')].map(
          (h) => h.tagName,
        ),
      ).toEqual(headings.map(([, level]) => `H${level}`))
      expect(await treeOf(['heading'])).toEqual(
        headings.map(([name, level]) => ({ role: 'heading', name, level })),
      )
    },
  )

  it('reads a directory’s scan report one level beneath its section’s heading', async () => {
    // A report's two headings are named by their words and sit under the
    // heading of whatever draws them: on a directory's page, its Scan report
    // section.
    const finishedScan = {
      scan: {
        root: root.path,
        dirs: 1,
        files: 1,
        added: 0,
        updated: 0,
        gone: 0,
        refused: null,
        unreadable_dirs: [],
        excluded_dirs: [],
        excluded_files: 0,
        bad_exclude_patterns: [],
        forgotten_by_exclusion: 0,
        taken_over: 0,
      },
      derive: emptyDerive,
    }
    // Running when the directory's page is drawn, so it adopts the scan and
    // polls; the poll then finds it finished, with its report.
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: true,
        elapsed_secs: 1,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValue({
        root_id: 3,
        running: false,
        report: finishedScan,
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
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [MANGA],
    })
    await shown(librariesAt('/config/libraries/Manga/dirs/3'))
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, config.SCAN_POLL_MS + 400)
      })
    })
    expect(await treeOf(['heading'])).toEqual([
      { role: 'heading', name: root.path, level: 2 },
      { role: 'heading', name: 'How it is scanned', level: 3 },
      { role: 'heading', name: 'How it is read', level: 3 },
      { role: 'heading', name: 'Scan report', level: 3 },
      { role: 'heading', name: 'What the walk found', level: 4 },
      {
        role: 'heading',
        name: 'What derivation made of every library',
        level: 4,
      },
      { role: 'heading', name: 'Removal', level: 3 },
    ])
  })

  it('reads a removal’s report one level beneath the table’s heading', async () => {
    vi.mocked(librariesApi.libraryScanStatus).mockResolvedValue({
      library_id: 1,
      held_removals: [],
      moves_held: false,
      scans: [],
      running: false,
      elapsed_secs: null,
      dirs: [],
    })
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [MANGA],
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
      derive: emptyDerive,
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
      derive: emptyDerive,
      dry_run: false,
    })
    // Removed from its page, whose report is drawn on the table.
    const host = await shown(librariesAt('/config/libraries/Manga'))
    await press(host.querySelector('button[name="remove-library"]') as Element)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(await treeOf(['heading'])).toEqual([
      { role: 'heading', name: 'Libraries', level: 2 },
      {
        role: 'heading',
        name: 'What derivation made of every library',
        level: 3,
      },
    ])
  })

  it('reads a save’s re-derive report beside the preview, beneath Rules', async () => {
    // The report is drawn after the preview's box, not in it, so it is not
    // read as one of the preview's parts.
    answerPages()
    const host = await shown(configAt('/config/rules'))
    await press(host.querySelector('button.save') as Element)
    const details = host.querySelector(
      'details.save-derive',
    ) as HTMLDetailsElement
    // Opened: what a closed disclosure holds is not in the tree.
    await act(async () => {
      details.open = true
    })
    expect(await treeOf(['heading'])).toEqual([
      { role: 'heading', name: 'Configuration', level: 1 },
      { role: 'heading', name: 'Rules', level: 2 },
      { role: 'heading', name: 'Preview', level: 3 },
      {
        role: 'heading',
        name: 'What derivation made of every library',
        level: 3,
      },
    ])
  })
})

/**
 * `/account`, About and the licence page are pages of their own, not
 * Configuration's, so each heads itself with a visible h1 and its sections
 * one level below.
 */
describe('the headings of /account, About and the licence page, in Chromium’s own tree', () => {
  it.each([
    [
      '/account',
      () => {
        vi.mocked(api.listKeys).mockResolvedValue([])
        vi.mocked(api.listSessions).mockResolvedValue([])
        answerAbout()
        return <Account placement="page" onSignedOut={() => {}} />
      },
      // The two lists, and `/api/about`'s answer, which brings the
      // catalog's address under API keys.
      () =>
        act(async () => {
          await new Promise((resolve) => setTimeout(resolve, 0))
        }),
      [
        ['Account', 1],
        ['Password', 2],
        ['API keys', 2],
        ['Signed in', 2],
      ],
    ],
    [
      '/about',
      () => {
        answerAbout()
        return <About />
      },
      // `/api/about`'s answer, which brings every section but the last.
      () =>
        act(async () => {
          await new Promise((resolve) => setTimeout(resolve, 0))
        }),
      [
        ['About', 1],
        ['This build', 2],
        ['Formats', 2],
        ['Licence', 2],
      ],
    ],
    [
      '/about/licence',
      () => <Licence />,
      licenceTextsLanded,
      [
        ['Licence', 1],
        ['taiyaku', 2],
        ['Shippori Mincho B1', 2],
        ['Zen Kaku Gothic New', 2],
        ['taiyaku marks', 2],
      ],
    ],
  ] as const)(
    'reads %s as its own heading, and its sections beneath it',
    async (_, draw, land, headings) => {
      const host = await shown(<MemoryRouter>{draw()}</MemoryRouter>)
      await land()
      expect(
        [...host.querySelectorAll('h1, h2, h3, h4, h5, h6')].map(
          (h) => h.tagName,
        ),
      ).toEqual(headings.map(([, level]) => `H${level}`))
      expect(await treeOf(['heading'])).toEqual(
        headings.map(([name, level]) => ({ role: 'heading', name, level })),
      )
    },
  )
})

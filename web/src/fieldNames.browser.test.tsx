/// <reference types="@vitest/browser-playwright" />
import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import { cdp } from 'vitest/browser'
import { Account } from './Account'
import * as api from './api'
import * as config from './config'
import * as curation from './curation'
import * as librariesApi from './libraries'
// **Required.** The capitals this file is about are `admin.css`'s, which
// reaches the app through `index.css`; without it every label is drawn as
// written and each case's first assertion fails.
import './index.css'
import { ProposalRow } from './ProposalRow'
import { RootRow } from './RootRow'
import { Libraries } from './Libraries'
import { RuleForm } from './RuleForm'
import type { Rule } from './rules'
import { emptyDerive } from './testing/derive'
import { fields } from './testing/fields'
import { press } from './testing/input'
import { mount } from './testing/mount'
import { UserRow } from './UserRow'
import * as usersApi from './users'
import { Users } from './Users'

/**
 * O175: what Chromium's own accessibility tree names the controls under a
 * `.field` label, read over the DevTools protocol.
 *
 * `markup.browser.test.tsx` pins names as Playwright computes them, and here
 * the two disagreed. `admin.css` draws the `.field` labels mounted here as
 * upper-case micro-labels through `text-transform`; Chromium's tree named
 * the control in those capitals — "EDITION KEY", "NAME" — where Playwright's
 * computation gave the words as written. Each control now carries its
 * label's words in an `aria-label`, and this file holds what the tree makes
 * of that.
 *
 * **Each case first asserts that its labels are drawn in capitals.** A label
 * drawn as written names its control as written with no `aria-label` at all,
 * so without that the names below could pass having tested nothing.
 *
 * One `it()` per form or card: `testing/mount.ts` holds one root, for the
 * reason `Filmstrip.browser.test.tsx` gives.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

vi.mock('./api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./api')>()),
  listKeys: vi.fn(),
  listSessions: vi.fn(),
}))

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  scanStatus: vi.fn(),
}))

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  removeLibrary: vi.fn(),
}))

vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  proposalDetail: vi.fn(),
  resolution: vi.fn(),
}))

vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  listUsers: vi.fn(),
}))

const root: config.Root = {
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
}

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
 * drawn with — the guard every case opens on.
 */
const drawn = (host: HTMLElement): Array<[string, string]> =>
  fields(host.querySelectorAll('label.field')).map(({ written, words }) => [
    written,
    getComputedStyle(words).textTransform,
  ])

/** The roles Chromium's tree gives a text box, a `<select>` and a number box. */
const CONTROLS = ['textbox', 'combobox', 'spinbutton']

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
    // No accounts, so the form's controls are the only ones on the screen.
    vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    const host = await shown(<Users />)
    expect(drawn(host)).toEqual([
      ['Name', 'uppercase'],
      ['Password', 'uppercase'],
      ['Role', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['textbox', 'Name'],
      ['textbox', 'Password'],
      ['combobox', 'Role'],
    ])
  })

  it('names an account card’s controls as written', async () => {
    const sam = user({ id: 2, name: 'sam', role: 'user' })
    const host = await shown(
      // A router because the card can link to `/account` (O94).
      <MemoryRouter>
        <UserRow
          user={sam}
          users={[user(), sam]}
          libraries={[]}
          onChanged={() => {}}
        />
      </MemoryRouter>,
    )
    expect(drawn(host)).toEqual([
      ['Role', 'uppercase'],
      ['Set a new password', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['combobox', 'Role'],
      ['textbox', 'Set a new password'],
    ])
  })

  it('names the make-a-library form’s boxes as written', async () => {
    // No libraries, so the form's two boxes are the only ones on the screen.
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    const host = await shown(<Libraries />)
    expect(drawn(host)).toEqual([
      ['Name', 'uppercase'],
      ['Folder path', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['textbox', 'Name'],
      ['textbox', 'Folder path'],
    ])
  })

  it('names a root card’s controls as written', async () => {
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
    expect(drawn(host)).toEqual([
      ['Edition key', 'uppercase'],
      ['Default kind', 'uppercase'],
      ['Default language', 'uppercase'],
      ['Exclude globs (one gitignore pattern per line)', 'uppercase'],
      ['Scan interval (seconds)', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['textbox', 'Edition key'],
      ['textbox', 'Default kind'],
      ['textbox', 'Default language'],
      ['textbox', 'Exclude globs (one gitignore pattern per line)'],
      ['spinbutton', 'Scan interval (seconds)'],
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

  it('names the account page’s password boxes as written', async () => {
    vi.mocked(api.listKeys).mockResolvedValue([])
    vi.mocked(api.listSessions).mockResolvedValue([])
    const host = await shown(
      <Account placement="page" onSignedOut={() => {}} />,
    )
    expect(drawn(host)).toEqual([
      ['Current', 'uppercase'],
      ['New', 'uppercase'],
    ])
    expect(await controls()).toEqual([
      ['textbox', 'Current'],
      ['textbox', 'New'],
      // The key-name box: no `.field` wraps it, and its placeholder names it.
      ['textbox', 'Which device is this for?'],
    ])
  })
})

/**
 * O182: the same capitals on two headings and a group. `admin.css` draws a
 * proposal's two `.lab` headings and the rule form's `sets` legend through
 * `text-transform: uppercase` as well, and Chromium's tree named them in
 * those capitals. Each now carries its words in an `aria-label`.
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

/**
 * A library's card has two headings: its name, and each folder's path a level
 * below. What the tree makes of them is held beside the capitals cases
 * because the make-a-library form's own heading is drawn by the same sheet.
 */
describe('the headings of the libraries screen, in Chromium’s own tree', () => {
  it('reads the screen, the library, its folder and the form in order', async () => {
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
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        { id: 1, name: 'Manga', works: 0, books: 0, folders: [root] },
      ],
    })
    const host = await shown(<Libraries />)
    expect(
      [...host.querySelectorAll('h1, h2, h3, h4, h5, h6')].map(
        (h) => h.tagName,
      ),
    ).toEqual(['H2', 'H3', 'H4', 'H3'])
    const tree = await named(['heading'])
    expect(tree).toEqual([
      ['heading', 'Libraries'],
      ['heading', 'Manga'],
      ['heading', root.path],
      ['heading', 'Make a library'],
    ])
  })

  it('reads a folder’s scan report one level beneath the folder’s path', async () => {
    // A report's two headings are named by their words and sit under the
    // heading of whatever draws them.
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
      },
      derive: emptyDerive,
    }
    // Running when the card is drawn, so it adopts the scan and polls; the
    // poll then finds it finished, with its report.
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
      running: false,
      elapsed_secs: null,
      folders: [],
    })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        { id: 1, name: 'Manga', works: 0, books: 0, folders: [root] },
      ],
    })
    await shown(<Libraries />)
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, config.SCAN_POLL_MS + 400)
      })
    })
    expect(await treeOf(['heading'])).toEqual([
      { role: 'heading', name: 'Libraries', level: 2 },
      { role: 'heading', name: 'Manga', level: 3 },
      { role: 'heading', name: root.path, level: 4 },
      { role: 'heading', name: 'What the walk found', level: 5 },
      { role: 'heading', name: 'What derivation made of it', level: 5 },
      { role: 'heading', name: 'Make a library', level: 3 },
    ])
  })

  it('reads a removal’s report one level beneath the screen', async () => {
    vi.mocked(librariesApi.libraryScanStatus).mockResolvedValue({
      library_id: 1,
      running: false,
      elapsed_secs: null,
      folders: [],
    })
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({
      libraries: [
        { id: 1, name: 'Manga', works: 0, books: 0, folders: [root] },
      ],
    })
    vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(librariesApi.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: emptyDerive,
      dry_run: false,
    })
    const host = await shown(<Libraries />)
    await press(host.querySelector('button[name="remove-library"]') as Element)
    vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(await treeOf(['heading'])).toEqual([
      { role: 'heading', name: 'Libraries', level: 2 },
      { role: 'heading', name: 'What derivation made of it', level: 3 },
      { role: 'heading', name: 'Make a library', level: 3 },
    ])
  })
})

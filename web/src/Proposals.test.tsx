import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as curation from './curation'
import { Proposals } from './Proposals'
import { InboxRefreshContext } from './shell'
import { mount } from './testing/mount'

// Following `Series.test.tsx`'s `vi.mock` shape. `screen`/`userEvent` are not
// used here — this repo has no `@testing-library/*` dependency at all
// (`testing/mount.ts`'s own doc comment says why: a second mounting API earns
// nothing over the `querySelector`/`act` lines already working here), so the
// interactions below drive real DOM elements and events the way every other
// component test in this tree does.
//
// **Every route this screen can reach, not only the one it calls itself.**
// `listProposals` was the only mock, which left the rows' `proposalDetail`
// and `dismissProposal` as the *real* functions calling the real global
// `fetch`: jsdom inherits Node's undici, and `/api/library/proposals/1` is a
// relative URL undici rejects with `TypeError: Failed to parse URL` before a
// socket opens. So it did not flake — but the test named "shows that row its
// own evidence" was asserting against a failed load, and the day anything
// binds `fetch` to jsdom's window it becomes a real request out of a unit
// test. A screen's mock has to cover what the components it mounts call.
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
  proposalDetail: vi.fn(),
  dismissProposal: vi.fn(),
  reopenProposal: vi.fn(),
}))

const proposal = (
  over: Partial<curation.ListedProposal> = {},
): curation.ListedProposal => ({
  id: 1,
  kind: 'edition-of',
  subject_id: 10,
  target_id: 20,
  subject_title: 'Go-Toubun no Hanayome - Full Color Edition',
  target_title: 'Go-Toubun no Hanayome',
  score: 2,
  reason: 'suffix "- Full Color Edition" matches the vocabulary',
  state: 'open',
  created_rule_id: null,
  ...over,
})

/** A side with nothing in it: this file's tests are about the list, not the panel. */
const side = (title: string): curation.Side => ({
  series_id: 1,
  title,
  directories: [],
  volume_count: 0,
  volumes: [],
  root_name: null,
  first_volume: null,
})

/**
 * The evidence a row's own request answers with, built from the row.
 *
 * The panel is `ProposalRow.test.tsx`'s subject; what this file needs is an
 * answer that renders *something* per row, so a test about which row is open
 * is not silently a test about a failed load.
 */
const evidenceFor = (p: curation.ListedProposal): curation.Detail => ({
  proposal: p,
  subject: side(p.subject_title),
  target: p.target_title === null ? null : side(p.target_title),
  reasons: [],
  rule: null,
})

/** Mount and let the initial fetch settle — two flushes, as `Series.test.tsx` explains. */
async function settle(): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Proposals />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const click = async (el: Element) => {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** The one control that reveals weak proposals, by class rather than by label. */
function weakBox(host: HTMLElement): HTMLInputElement {
  const box = host.querySelector<HTMLInputElement>(
    '.weak-check input[type="checkbox"]',
  )
  if (!box) throw new Error('no weak-proposals control rendered')
  return box
}

/**
 * The filter object the most recent `listProposals` call carried.
 *
 * Reads `.mock.calls` directly rather than going through
 * `toHaveBeenLastCalledWith`, so the object can be handed to `toEqual`
 * *whole*. Reading it whole is the point: the kind tabs and the score
 * threshold are two independent filters, and a tab that also moved the
 * threshold — or a toggle that also cleared the kind — passes any assertion
 * that inspects the one field the click was about. Only comparing every
 * field the screen sends can tell those apart.
 */
function lastCall(): curation.ProposalsFilter {
  const calls = vi.mocked(curation.listProposals).mock.calls
  const last = calls.at(-1)
  if (last === undefined || last[0] === undefined) {
    throw new Error('listProposals was never called')
  }
  return last[0]
}

describe('Proposals', () => {
  // Several tests here read `.mock.calls` directly (`lastCall()`), and a call
  // left over from a previous test would let a broken implementation pass by
  // finding someone else's correct call instead of making its own.
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(curation.proposalDetail).mockResolvedValue(
      evidenceFor(proposal()),
    )
    vi.mocked(curation.dismissProposal).mockResolvedValue(undefined)
    vi.mocked(curation.reopenProposal).mockResolvedValue(undefined)
  })

  it('gives every row its own proposal, not the first one three times', async () => {
    // Three rows differing in all three fields a line draws. A fixture whose
    // rows agreed could not tell "each row reads its own proposal" apart from
    // "every row reads `proposals[0]`", and neither of those apart from "the
    // line renders nothing at all" — three mutations that survived a version
    // of this test which counted `<li>`s and read only their class.
    //
    // Scores 2, 1 and 0 straddle `strength`'s own boundary (>= 2), so the
    // strength column is not constant either.
    //
    // **The titles are what a collapsed row says it is**, and the three rows
    // carry three different pairs of them. The third row has no target at
    // all, which is the `attach-chapters` shape: its arrow line must be
    // absent rather than reading `↳ an edition of null`.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [
        proposal({
          id: 1,
          kind: 'edition-of',
          subject_title: 'Go-Toubun no Hanayome - Full Color Edition',
          target_title: 'Go-Toubun no Hanayome',
          score: 2,
        }),
        proposal({
          id: 2,
          kind: 'same-series',
          subject_title: 'Alt Title For GTNH',
          target_title: 'Kobayashi-san Chi no Meidoragon',
          score: 1,
        }),
        proposal({
          id: 3,
          kind: 'align',
          subject_title: 'Sousou no Frieren - Fan Translated',
          target_title: null,
          score: 0,
        }),
      ],
      counts: { 'edition-of': 1, 'same-series': 1, align: 1 },
    })

    const host = await settle()

    expect(host.textContent).toContain('3 open')
    const rows = [...host.querySelectorAll('li')]
    expect(rows).toHaveLength(3)
    const column = (selector: string) =>
      rows.map((li) => li.querySelector(selector)?.textContent)
    expect(column('.kind')).toEqual(['Edition Of', 'Same Series', 'Align'])
    expect(column('.subject')).toEqual([
      'Go-Toubun no Hanayome - Full Color Edition',
      'Alt Title For GTNH',
      'Sousou no Frieren - Fan Translated',
    ])
    // The whole line, relation word included: the phrase is a claim about
    // the two series and differs by kind, so a row that printed one fixed
    // word — or the subject's title twice — reads identically to this one
    // against a fixture of a single kind.
    expect(column('.arrow')).toEqual([
      '↳ an edition of Go-Toubun no Hanayome',
      '↳ the same series as Kobayashi-san Chi no Meidoragon',
      undefined,
    ])
    expect(column('.score')).toEqual(['strong', 'maybe', 'weak'])
    expect(rows.map((li) => li.className)).toEqual(['strong', 'maybe', 'weak'])
  })

  it('asks for the open state at the default score threshold on the very first request', async () => {
    // Distinct from "keeps a weak proposal reachable" below: a mutation that
    // always sent `min_score: 0` regardless of `showWeak` would still pass
    // that test's post-click assertion, since 0 is what it checks for either
    // way. Only the *first* call, taken before any click, tells "defaults to
    // 1" apart from "always 0". `state: 'open'` is the default the state
    // control starts from (O72) — dropping it entirely would ask the server
    // for every state, not just open ones, and only the first call, before
    // any control is touched, can see which state the screen lands on.
    //
    // The whole object, including `kind: undefined`: an inbox that opened
    // pre-filtered to one kind is a real defect and reads identically to a
    // correct one through `objectContaining`.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: {},
    })

    await settle()

    expect(lastCall()).toEqual({
      state: 'open',
      kind: undefined,
      min_score: 1,
    })
  })

  it('filters by kind through the tabs, each reading its own kind rather than a fixed one', async () => {
    // Two kinds with different counts (29 and 6, not equal) — a tab reading
    // the wrong key, or every tab reading the same key, would be caught by
    // checking both labels below rather than only the one that gets clicked.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [proposal({ id: 1 })],
      counts: { 'edition-of': 29, 'attach-chapters': 6 },
    })
    const host = await settle()

    // The header counts the *inbox*, not the rows currently listed. This
    // fixture is the only one where the two differ — 35 against a list of
    // one — so it is the only one that can tell them apart, and with a kind
    // tab selected they differ at runtime on every screen.
    expect(host.querySelector('h1')?.textContent).toBe('35 open')

    // The kind row is one-of-N and wears the tab look; the state row is the
    // coarser filter and keeps pills; the threshold is neither and sits
    // outside both (O150, spec §12.3). jsdom computes no styles, so what a
    // test here can hold is that the three are three different elements —
    // `admin-proposals.css.test.ts` holds that they are drawn differently.
    expect(host.querySelector('.proposal-tabs')).not.toBeNull()
    // `.weak-toggle` is gone from the app, so this line cannot go red today.
    // It is kept as a name guard: it is the only thing here that would catch
    // the old pill being reintroduced under its old name, which the `input`
    // line below cannot, a `<button>` not being an `<input>`.
    expect(host.querySelector('.proposal-tabs .weak-toggle')).toBeNull()
    expect(host.querySelector('.proposal-tabs input')).toBeNull()
    expect(
      host.querySelector('.weak-check input[type="checkbox"]'),
    ).not.toBeNull()

    const tabs = [...host.querySelectorAll('.proposal-tabs button.tab')]
    const editionTab = tabs.find((b) =>
      /Edition Of 29/.test(b.textContent ?? ''),
    )
    const chaptersTab = tabs.find((b) => /Chapters 6/.test(b.textContent ?? ''))
    const allTab = tabs.find((b) => /^All/.test(b.textContent ?? ''))
    if (!editionTab) throw new Error('no "Edition Of 29" tab rendered')
    if (!chaptersTab) throw new Error('no "Chapters 6" tab rendered')
    if (!allTab) throw new Error('no "All" tab rendered')
    // The exact text, not just a substring match: catches the total being
    // computed some way other than a sum over every kind's own count (a
    // count of keys, or the largest single count, would both still contain
    // "All" and both disagree with 35 here).
    expect(allTab.textContent).toBe('All 35')

    // The whole filter object rather than `.kind` alone: a tab that also
    // dropped the threshold to 0 on its way past is exactly the kind of
    // coupling a one-field assertion cannot see.
    await click(chaptersTab)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'attach-chapters',
      min_score: 1,
    })
    expect(chaptersTab.getAttribute('aria-pressed')).toBe('true')
    expect(editionTab.getAttribute('aria-pressed')).toBe('false')
    expect(allTab.getAttribute('aria-pressed')).toBe('false')

    // Clicking a *different* tab afterwards must change the argument, not
    // just append to it — a fixture that only ever clicked one kind could
    // not tell "reads this tab's kind" apart from "always sends this one
    // kind".
    await click(editionTab)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'edition-of',
      min_score: 1,
    })
    expect(editionTab.getAttribute('aria-pressed')).toBe('true')
    expect(chaptersTab.getAttribute('aria-pressed')).toBe('false')
    expect(allTab.getAttribute('aria-pressed')).toBe('false')

    // "All" clears the filter rather than sticking to the last kind clicked.
    await click(allTab)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: undefined,
      min_score: 1,
    })
    expect(allTab.getAttribute('aria-pressed')).toBe('true')
    expect(editionTab.getAttribute('aria-pressed')).toBe('false')
  })

  it('keeps a weak proposal reachable behind the threshold, without disturbing the kind filter', async () => {
    // C3 §4: the pattern-level control is a score threshold, not a
    // suppression list — "a dismissal you disagree with later is a thing you
    // have to be able to find", and so is a proposal the threshold is
    // currently hiding.
    //
    // Non-empty `counts`, unlike the version of this test that shipped: with
    // `counts: {}` no kind tab renders at all, so a threshold toggle that
    // also cleared the kind had nothing to clear and the coupling was
    // unobservable. A kind is selected first here for exactly that reason.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: { 'edition-of': 29, 'attach-chapters': 6 },
    })
    const host = await settle()

    const editionTab = [
      ...host.querySelectorAll('.proposal-tabs button.tab'),
    ].find((b) => /Edition Of 29/.test(b.textContent ?? ''))
    if (!editionTab) throw new Error('no "Edition Of 29" tab rendered')
    await click(editionTab)

    const weak = weakBox(host)
    expect(weak.checked).toBe(false)
    // The label is stable now — asserted once, because a label that changed
    // under a state also drawn was the defect O150's answer removed.
    expect(weak.closest('label')?.textContent?.trim()).toBe(
      'Show weak proposals',
    )

    await click(weak)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'edition-of',
      min_score: 0,
    })
    expect(weak.checked).toBe(true)

    // A real toggle, not a one-way switch a fixture that clicked only once
    // could not tell apart from this: pressing it again must go back to the
    // default threshold — and must still not touch the kind.
    await click(weak)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'edition-of',
      min_score: 1,
    })
    expect(weak.checked).toBe(false)
  })

  it('expands one row at a time, and shows that row its own evidence', async () => {
    // Each row's evidence answers from its own id, so "the second row's own
    // panel" is a thing this fixture can be wrong about — and the panel it
    // reads below is a real one. It used to be a *failed* one: `proposalDetail`
    // was unmocked here, reached the global `fetch` and rejected, so the only
    // `<p>` the assertion could have been reading was `.full-reason` above
    // the alert. See this file's `vi.mock` for the whole of that.
    const rowsIn = [
      proposal({ id: 1, subject_title: 'first subject' }),
      proposal({ id: 2, subject_title: 'second subject' }),
    ]
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: rowsIn,
      counts: { 'edition-of': 2 },
    })
    vi.mocked(curation.proposalDetail).mockImplementation((id) => {
      const found = rowsIn.find((p) => p.id === id)
      if (!found) throw new Error(`no fixture proposal ${id}`)
      return Promise.resolve(evidenceFor(found))
    })
    const host = await settle()

    const rows = [...host.querySelectorAll('li')]
    expect(rows).toHaveLength(2)
    // The disclosure control *inside* the collapsed line, not the line
    // itself. The line is a plain element precisely so Accept and Dismiss
    // can sit in it beside this button — an interactive descendant of a
    // `<button>` is invalid HTML.
    const buttons = rows.map((li) => li.querySelector('.row button.disclosure'))
    const [firstButton, secondButton] = buttons
    if (!firstButton || !secondButton) {
      throw new Error('row disclosure control missing')
    }
    // By class, not by `<p>` position: the panel's paragraph order is
    // `ProposalRow`'s business and a test here that depended on it would
    // fail for a change that had nothing to do with which row is open.
    const evidence = (i: number) =>
      rows[i]?.querySelector('.proposal-detail .side-title')?.textContent

    await click(firstButton)
    expect(evidence(0)).toBe('first subject')
    expect(rows[1]?.querySelector('.proposal-detail')).toBeNull()
    expect(firstButton.getAttribute('aria-expanded')).toBe('true')
    expect(secondButton.getAttribute('aria-expanded')).toBe('false')

    // Opening the second row must close the first — "one row expanded at a
    // time", not "every row opened stays open" — and the panel it opens must
    // be the second row's own, not the first row's shown twice.
    await click(secondButton)
    expect(rows[0]?.querySelector('.proposal-detail')).toBeNull()
    expect(evidence(1)).toBe('second subject')
    expect(firstButton.getAttribute('aria-expanded')).toBe('false')
    expect(secondButton.getAttribute('aria-expanded')).toBe('true')

    // And pressing the open row's own button again closes it.
    await click(secondButton)
    expect(rows[1]?.querySelector('.proposal-detail')).toBeNull()
    expect(secondButton.getAttribute('aria-expanded')).toBe('false')
  })

  it('re-reads the inbox when a row dismisses its own proposal', async () => {
    // **The seam between the row and the screen, which nothing joined.**
    // `ProposalRow.test.tsx` proves a row calls `onChanged` after a
    // dismissal; the test below proves `reload` re-fetches. Neither sees the
    // wiring between them, and replacing `onChanged={reload}` in
    // `Proposals.tsx` with `() => {}` left all 41 of this branch's targeted
    // tests green — while leaving the list and every tab count stale after
    // every dismissal, which is the exact defect the reload counter exists
    // to prevent.
    //
    // It has to live in this file: `Proposals` is what hands a row its
    // `onChanged`, so this is the only place both halves are on screen at
    // once. The counts move as well as the rows — a mutation that re-fetched
    // but only wrote `setProposals` would leave the header reading 2.
    vi.mocked(curation.listProposals)
      .mockResolvedValueOnce({
        proposals: [proposal({ id: 1 }), proposal({ id: 2 })],
        counts: { 'edition-of': 2 },
      })
      .mockResolvedValue({
        proposals: [proposal({ id: 2 })],
        counts: { 'edition-of': 1 },
      })

    const host = await settle()
    expect(host.querySelectorAll('li')).toHaveLength(2)
    expect(host.querySelector('h1')?.textContent).toBe('2 open')

    const dismiss = host.querySelector('li button.dismiss')
    if (!dismiss) throw new Error('no dismiss control rendered')
    await click(dismiss)

    // The row dismissed its own proposal, and the screen asked again.
    expect(curation.dismissProposal).toHaveBeenCalledWith(1)
    expect(curation.listProposals).toHaveBeenCalledTimes(2)
    expect(host.querySelectorAll('li')).toHaveLength(1)
    expect(host.querySelector('h1')?.textContent).toBe('1 open')
  })

  it('tells the chrome to re-count when a write moves the inbox', async () => {
    // **The sidebar's badge and this screen's header are the same number, on
    // screen together.** `Shell` reads the count once when it mounts; a
    // dismissal here re-reads the list but cannot reach that fetch, so
    // without this the badge sits at 2 beside a header reading 1 until a
    // navigation. `Shell.test.tsx` pins that the chrome *has* a count;
    // nothing there can pin that a write on this screen moves it.
    //
    // Driven through a real dismissal rather than by calling `reload`
    // directly: `reload` is also `Retry`'s handler, and a version that
    // refreshed on Retry alone — the case where the count did not move —
    // would pass a test that only pressed Retry.
    vi.mocked(curation.listProposals)
      .mockResolvedValueOnce({
        proposals: [proposal({ id: 1 }), proposal({ id: 2 })],
        counts: { 'edition-of': 2 },
      })
      .mockResolvedValue({
        proposals: [proposal({ id: 2 })],
        counts: { 'edition-of': 1 },
      })

    const recounts: number[] = []
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <InboxRefreshContext.Provider value={() => recounts.push(1)}>
          <Proposals />
        </InboxRefreshContext.Provider>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')

    expect(recounts).toHaveLength(0)

    const dismiss = host.querySelector('li button.dismiss')
    if (!dismiss) throw new Error('no dismiss control rendered')
    await click(dismiss)

    expect(recounts).toHaveLength(1)
    expect(host.querySelector('h1')?.textContent).toBe('1 open')
  })

  it('re-asks on demand, and clears the failure once an answer arrives', async () => {
    // The reload seam. Accepting or dismissing a proposal changes both the
    // list and the counts, and an effect keyed on the filters alone would
    // show neither until a filter was touched — so the effect carries a
    // reload counter and `Retry` is its first caller.
    //
    // Two failures before the success, and two clicks: one click cannot tell
    // a counter that increments from one that is assigned a constant, since
    // both move the state the first time and only the second click asks the
    // difference. The failure text changes between them so the second
    // request is visible rather than merely assumed.
    vi.mocked(curation.listProposals)
      .mockRejectedValueOnce(new Error('/api/library/proposals: 500'))
      .mockRejectedValueOnce(new Error('/api/library/proposals: 503'))
      .mockResolvedValue({
        proposals: [proposal({ id: 1 })],
        counts: { 'edition-of': 1 },
      })

    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/proposals: 500',
    )

    const retry = host.querySelector('button.retry')
    if (!retry) throw new Error('no retry control rendered')
    await click(retry)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/proposals: 503',
    )

    // The banner must clear on success rather than sitting above a list that
    // loaded correctly.
    await click(retry)
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.textContent).toContain('1 open')
    expect(host.querySelectorAll('li')).toHaveLength(1)
  })

  it('drops the tab for a kind whose count has fallen to zero', async () => {
    // `setCounts` replaces; it does not merge. The two read identically on a
    // first response, which is all every other fixture here asks for — and a
    // merge leaves a tab counting proposals that are no longer there, since
    // a kind with nothing open is *absent* from the server's `counts` map
    // rather than present with a zero.
    vi.mocked(curation.listProposals)
      .mockResolvedValueOnce({
        proposals: [],
        counts: { 'edition-of': 2, 'attach-chapters': 1 },
      })
      .mockResolvedValue({ proposals: [], counts: { 'edition-of': 2 } })

    const host = await settle()
    const labels = () =>
      [...host.querySelectorAll('.proposal-tabs button.tab')].map(
        (b) => b.textContent,
      )
    expect(labels()).toEqual(['All 3', 'Edition Of 2', 'Attach Chapters 1'])

    // Any filter change re-asks; the threshold toggle is the one control
    // that is always on screen.
    await click(weakBox(host))
    expect(labels()).toEqual(['All 2', 'Edition Of 2'])
  })

  it('discards a stale response that lands after a newer filter change', async () => {
    // `Library.tsx`, `Shell.tsx`, `Series.tsx` and `Search.tsx` all guard
    // their own fetch effects with a `cancelled` flag for exactly this: a
    // request that is no longer the current one must not overwrite what a
    // later one already set. Every other test here awaits two microtask
    // flushes between mount and the next assertion, which is enough time for
    // vitest's default mocked-promise resolution to land in order — so none
    // of them can tell "discards the stale one" apart from "the stale one
    // just never arrived late enough to matter". This test controls both
    // promises by hand and resolves them out of order on purpose.
    let resolveFirst: ((inbox: curation.Inbox) => void) | undefined
    let resolveSecond: ((inbox: curation.Inbox) => void) | undefined
    vi.mocked(curation.listProposals)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveSecond = resolve
          }),
      )

    const host = await settle()

    // Nothing has answered yet, and this is the only test that looks at the
    // screen in that state: an initial `proposals` or `counts` seeded with
    // anything but empty would draw a row nobody asked for, and every other
    // fixture here overwrites the seed before it is ever read.
    expect(host.querySelectorAll('li')).toHaveLength(0)
    expect(host.querySelector('h1')?.textContent).toBe('0 open')

    const weakButton = weakBox(host)
    // Triggers the second, current request — the effect's dependency
    // (`showWeak`) changed, which cancels the first and starts this one.
    await act(async () => {
      weakButton.click()
      await Promise.resolve()
    })
    if (!resolveSecond) throw new Error('the second request never started')

    // The current request settles first...
    await act(async () => {
      resolveSecond?.({
        proposals: [proposal({ id: 2 })],
        counts: { 'edition-of': 1 },
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('1 open')

    // ...then the stale one, from before the filter changed, lands late.
    // Without the cancelled guard this overwrites the correct, current state
    // with an answer to a question the screen is no longer asking.
    await act(async () => {
      resolveFirst?.({
        proposals: [proposal({ id: 1 }), proposal({ id: 99 })],
        counts: { 'edition-of': 2 },
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('1 open')
    expect(host.querySelectorAll('li')).toHaveLength(1)
  })

  it('discards a stale rejection that lands after a newer request already succeeded', async () => {
    // The `catch` side of the same guard the resolve-path test above pins.
    // `.then` and `.catch` each carry their own `if (cancelled) return`, and
    // mutating only the `.then` one still passed every other test here —
    // this is the one that would have let that survive.
    let rejectFirst: ((err: unknown) => void) | undefined
    let resolveSecond: ((inbox: curation.Inbox) => void) | undefined
    vi.mocked(curation.listProposals)
      .mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            rejectFirst = reject
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveSecond = resolve
          }),
      )

    const host = await settle()

    const weakButton = weakBox(host)
    await act(async () => {
      weakButton.click()
      await Promise.resolve()
    })
    if (!resolveSecond) throw new Error('the second request never started')

    await act(async () => {
      resolveSecond?.({
        proposals: [proposal({ id: 2 })],
        counts: { 'edition-of': 1 },
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()

    // The stale request's rejection lands after the current one already
    // succeeded. Without the `.catch` side of the cancelled guard, this
    // paints a failure banner over a screen that just loaded correctly.
    await act(async () => {
      rejectFirst?.(new Error('/api/library/proposals: 500'))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.textContent).toContain('1 open')
  })

  it('says so when the inbox cannot be read', async () => {
    // The exact message, not a substring of it. Its sibling below asserts
    // `toBe('offline')`; a `toContain('500')` here would leave the pair
    // asymmetric, and `String(err)` — which renders this same rejection as
    // `Error: /api/library/proposals: 500` — satisfies it just as well as
    // `err.message` does.
    vi.mocked(curation.listProposals).mockRejectedValue(
      new Error('/api/library/proposals: 500'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/proposals: 500',
    )
  })

  /**
   * `Shell.test.tsx`'s "stringifies a rejection that is not an Error" pins
   * the same fallback there; every other failure fixture in this file
   * rejects with a real `Error`, so `String(err)` has never run here either.
   */
  it('stringifies a rejection that is not an Error, rather than losing it', async () => {
    vi.mocked(curation.listProposals).mockRejectedValue('offline')
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })

  it('falls back to the raw kind string for a kind the tab labels do not name', async () => {
    // `kindLabel`'s `?? kind` fallback: every fixture above uses one of the
    // four known kinds, so an unlisted one has never reached it.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [proposal({ id: 1, kind: 'future-kind' })],
      counts: { 'future-kind': 1 },
    })
    const host = await settle()
    expect(host.textContent).toContain('future-kind')
  })

  it('draws no empty state until the first answer has landed', async () => {
    // The whole empty-state branch rests on telling "nothing came back" from
    // "nothing has come back *yet*", which an `[]`/`{}` initial state cannot
    // do — it reads identically to an inbox that answered with nothing in
    // it, so the message below would flash on every mount before the first
    // response. `Library.tsx`'s `gridData` and `Search.tsx`'s `hits` hold
    // the same `undefined` for the same reason.
    let finish: (inbox: curation.Inbox) => void = () => {}
    vi.mocked(curation.listProposals).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )

    const host = await settle()
    expect(host.querySelector('section.empty')).toBe(null)

    await act(async () => {
      finish({ proposals: [], counts: {} })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('section.empty')).not.toBe(null)
  })

  it('tells a cleared inbox apart from one whose weak proposals are hidden', async () => {
    // Two situations, and the person can act on exactly one of each: "there
    // is nothing to answer" and "there may be something and the threshold is
    // holding it back". `EmptyState`'s own doc gives the rule — `no-roots`
    // and `not-scanned` are split on it too — and collapsing the second into
    // the first tells somebody their work is finished while the control that
    // would show them otherwise sits unpressed a few pixels above the
    // message.
    //
    // Asserted on both directions of the toggle rather than on one: a screen
    // that printed either message unconditionally passes half of this.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [],
      counts: {},
    })

    const host = await settle()
    const empty = () => host.querySelector('section.empty')

    expect(host.querySelector('h1')?.textContent).toBe('0 open')
    expect(host.querySelectorAll('li')).toHaveLength(0)
    expect(empty()?.textContent).toContain('above the score threshold')
    expect(empty()?.textContent).toContain('Show weak proposals')
    expect(empty()?.textContent).not.toContain('taiyaku library scan')

    // With the threshold already dropped there is nothing left for that
    // control to reveal, so the message stops naming it.
    await click(weakBox(host))
    expect(empty()?.textContent).not.toContain('above the score threshold')
    expect(empty()?.textContent).toContain('taiyaku library scan')
  })

  it('keeps the tab for a selected kind answered down to nothing, and says where the rest are', async () => {
    // **The filter can empty its own list, and the tab it was set from is
    // absent from the next response.** `counts` comes from a SQL `GROUP BY`,
    // so a kind with nothing open is missing rather than zero: answering the
    // last `attach-chapters` proposal took that tab out of the row
    // *underneath the person's own selection*. The list went blank, no tab
    // read pressed, the header still said "2 open", and nothing said what
    // had happened or how to get back.
    //
    // A stateful stub rather than a queue of canned answers, because the
    // recovery is half of the defect: a fixture answering the same empty
    // list to *every* filter cannot tell "All brings the others back" from
    // "nothing comes back at all".
    let chapters = 1
    vi.mocked(curation.listProposals).mockImplementation(
      async (filter = {}) => {
        const rows = [
          ...(chapters > 0
            ? [proposal({ id: 1, kind: 'attach-chapters' })]
            : []),
          proposal({ id: 2, kind: 'edition-of' }),
          proposal({ id: 3, kind: 'edition-of' }),
        ].filter((p) => filter.kind === undefined || p.kind === filter.kind)
        const counts: Record<string, number> = { 'edition-of': 2 }
        if (chapters > 0) counts['attach-chapters'] = chapters
        return { proposals: rows, counts }
      },
    )
    vi.mocked(curation.dismissProposal).mockImplementation(async () => {
      chapters = 0
    })

    const host = await settle()
    const tabs = () => [
      ...host.querySelectorAll<HTMLElement>('.proposal-tabs button.tab'),
    ]
    const labels = () => tabs().map((b) => b.textContent)
    const pressed = () =>
      tabs()
        .filter((b) => b.getAttribute('aria-pressed') === 'true')
        .map((b) => b.textContent)
    const tab = (which: RegExp): HTMLElement => {
      const found = tabs().find((b) => which.test(b.textContent ?? ''))
      if (!found) throw new Error(`no tab matching ${which}`)
      return found
    }

    expect(labels()).toEqual(['All 3', 'Edition Of 2', 'Attach Chapters 1'])

    await click(tab(/^Attach Chapters/))
    expect(host.querySelectorAll('li')).toHaveLength(1)
    // Selecting a kind the server *is* still reporting adds nothing to the
    // row: the tab kept below is the one `counts` has stopped carrying, and
    // a version that appended unconditionally would draw this kind twice,
    // once at its real count and once at zero.
    expect(labels()).toEqual(['All 3', 'Edition Of 2', 'Attach Chapters 1'])

    const dismiss = host.querySelector('li button.dismiss')
    if (!dismiss) throw new Error('no dismiss control rendered')
    await click(dismiss)

    // The tab stays, reading zero, and is still the one that reads pressed:
    // a filter in force with no control holding `aria-pressed` is a screen
    // keeping its own state somewhere nobody can see it.
    expect(labels()).toEqual(['All 2', 'Edition Of 2', 'Attach Chapters 0'])
    expect(pressed()).toEqual(['Attach Chapters 0'])
    expect(host.querySelectorAll('li')).toHaveLength(0)

    // And the list says what happened — naming the kind, and naming the
    // control that undoes it — rather than leaving a header reading "2 open"
    // over nothing at all.
    expect(host.querySelector('h1')?.textContent).toBe('2 open')
    expect(host.querySelector('section.empty')?.textContent).toContain(
      'No Attach Chapters proposal is waiting on a decision.',
    )
    expect(host.querySelector('section.empty')?.textContent).toContain(
      'All shows the ones that are.',
    )

    // The recovery it names works, and takes the zero tab with it.
    await click(tab(/^All/))
    expect(host.querySelectorAll('li')).toHaveLength(2)
    expect(labels()).toEqual(['All 2', 'Edition Of 2'])
    expect(host.querySelector('section.empty')).toBe(null)
  })
  /** The state control's buttons, in order. */
  const stateButtons = (host: HTMLElement) => [
    ...host.querySelectorAll<HTMLElement>('.proposal-states button.state'),
  ]
  const stateButton = (host: HTMLElement, label: string): HTMLElement => {
    const found = stateButtons(host).find((b) => b.textContent === label)
    if (!found) throw new Error(`no state control labelled ${label}`)
    return found
  }

  it('asks for each state through its own control, keeping the kind and the threshold (O72)', async () => {
    // Spec C3 §4: accepted and dismissed proposals "stay listed under their
    // own filters". Every control is pressed, and the whole filter object is
    // compared each time, so a control sending a fixed state, or one that
    // reset the kind or the threshold on the way, fails here rather than
    // reading like a filter that works.
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [proposal({ id: 1, kind: 'edition-of' })],
      counts: { 'edition-of': 1 },
    })
    const host = await settle()

    expect(stateButtons(host).map((b) => b.textContent)).toEqual([
      'Open',
      'Accepted',
      'Dismissed',
    ])
    const pressed = () =>
      stateButtons(host)
        .filter((b) => b.getAttribute('aria-pressed') === 'true')
        .map((b) => b.textContent)
    expect(pressed()).toEqual(['Open'])

    // A kind and the weak toggle set first, so both have something to lose.
    const editionTab = [
      ...host.querySelectorAll<HTMLElement>('.proposal-tabs button.tab'),
    ].find((b) => b.textContent?.startsWith('Edition Of'))
    if (!editionTab) throw new Error('no Edition Of tab')
    await click(editionTab)
    await click(weakBox(host))

    await click(stateButton(host, 'Dismissed'))
    expect(lastCall()).toEqual({
      state: 'dismissed',
      kind: 'edition-of',
      min_score: 0,
    })
    expect(pressed()).toEqual(['Dismissed'])

    await click(stateButton(host, 'Accepted'))
    expect(lastCall()).toEqual({
      state: 'accepted',
      kind: 'edition-of',
      min_score: 0,
    })
    expect(pressed()).toEqual(['Accepted'])

    await click(stateButton(host, 'Open'))
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'edition-of',
      min_score: 0,
    })
    expect(pressed()).toEqual(['Open'])
  })

  it('heads a decided list with its own state, and says that state is empty in its own words', async () => {
    // `counts` is counted within the state the request carried, so the
    // header over a dismissed list is a count of dismissals: "2 open" there
    // would be a false number. And an empty decided list is not a cleared
    // inbox — see `EmptyState`'s `decided-*` kinds.
    vi.mocked(curation.listProposals).mockImplementation(
      async (filter = {}): Promise<curation.Inbox> =>
        filter.state === 'dismissed'
          ? {
              proposals: [
                proposal({ id: 4, state: 'dismissed' }),
                proposal({ id: 5, state: 'dismissed' }),
              ],
              counts: { 'edition-of': 2 },
            }
          : filter.state === 'accepted'
            ? { proposals: [], counts: {} }
            : { proposals: [proposal({ id: 1 })], counts: { 'edition-of': 1 } },
    )
    const host = await settle()
    expect(host.querySelector('h1')?.textContent).toBe('1 open')

    await click(stateButton(host, 'Dismissed'))
    expect(host.querySelector('h1')?.textContent).toBe('2 dismissed')
    expect(host.querySelectorAll('li')).toHaveLength(2)

    await click(stateButton(host, 'Accepted'))
    expect(host.querySelector('h1')?.textContent).toBe('0 accepted')
    const empty = host.querySelector('section.empty')
    expect(empty?.textContent).toContain(
      'Nothing accepted is above the score threshold.',
    )
    expect(empty?.textContent).not.toContain('Nothing to answer')

    await click(weakBox(host))
    expect(host.querySelector('section.empty')?.textContent).toContain(
      'No proposal in this library has been accepted.',
    )
  })

  it('names a kind with no dismissals as that kind, while other kinds hold some', async () => {
    // `decided-kind-none` rather than `kind-answered`: "waiting on a
    // decision" is false of a dismissed list, and a screen that picked its
    // empty state without reading `state` would print it.
    vi.mocked(curation.listProposals).mockImplementation(
      async (filter = {}) => {
        const rows =
          filter.state === 'dismissed'
            ? [proposal({ id: 4, kind: 'edition-of', state: 'dismissed' })]
            : [
                proposal({ id: 1, kind: 'edition-of' }),
                proposal({ id: 2, kind: 'attach-chapters' }),
              ]
        const counts: Record<string, number> = {}
        for (const r of rows) counts[r.kind] = (counts[r.kind] ?? 0) + 1
        return {
          proposals: rows.filter(
            (p) => filter.kind === undefined || p.kind === filter.kind,
          ),
          counts,
        }
      },
    )
    const host = await settle()
    const chapters = [
      ...host.querySelectorAll<HTMLElement>('.proposal-tabs button.tab'),
    ].find((b) => b.textContent?.startsWith('Attach Chapters'))
    if (!chapters) throw new Error('no Attach Chapters tab')
    await click(chapters)
    await click(stateButton(host, 'Dismissed'))

    const empty = host.querySelector('section.empty')
    expect(empty?.textContent).toContain(
      'No Attach Chapters proposal has been dismissed.',
    )
    expect(empty?.textContent).not.toContain('waiting on a decision')
  })

  it('re-reads the dismissed list, still dismissed, and re-counts the chrome, when a row reopens its proposal', async () => {
    // The Reopen half of the seam `re-reads the inbox when a row dismisses
    // its own proposal` pins for Dismiss. A reload that fell back to the
    // open state would show the reopened proposal where the person is not
    // looking and hide the rest of the dismissals they were reading — so the
    // second request's state is asserted, not only that there was one.
    let dismissed = [4, 5]
    vi.mocked(curation.listProposals).mockImplementation(
      async (filter = {}): Promise<curation.Inbox> =>
        filter.state === 'dismissed'
          ? {
              proposals: dismissed.map((id) =>
                proposal({ id, state: 'dismissed' }),
              ),
              counts:
                dismissed.length === 0
                  ? {}
                  : { 'edition-of': dismissed.length },
            }
          : { proposals: [], counts: {} },
    )
    vi.mocked(curation.reopenProposal).mockImplementation(async (id) => {
      dismissed = dismissed.filter((d) => d !== id)
    })

    const recounts: number[] = []
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <InboxRefreshContext.Provider value={() => recounts.push(1)}>
          <Proposals />
        </InboxRefreshContext.Provider>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')

    await click(stateButton(host, 'Dismissed'))
    expect(host.querySelector('h1')?.textContent).toBe('2 dismissed')
    const reopen = host.querySelector('li button.reopen')
    if (!reopen) throw new Error('no reopen control rendered')
    const calls = vi.mocked(curation.listProposals).mock.calls.length

    await click(reopen)

    expect(curation.reopenProposal).toHaveBeenCalledWith(4)
    expect(vi.mocked(curation.listProposals).mock.calls.length).toBe(calls + 1)
    expect(lastCall().state).toBe('dismissed')
    expect(host.querySelector('h1')?.textContent).toBe('1 dismissed')
    expect(recounts).toHaveLength(1)
  })
  it('heads the list with the state it was fetched for, while the next list is on its way', async () => {
    // Held, not resolved at once: the header, the counts and the empty
    // state all describe one answer, and the window between pressing a
    // state and its answer landing is where reading the pressed control
    // instead drew "2 dismissed" over two open rows. Both directions, so the
    // empty state is watched in that window too — it chose its words from
    // the pressed control and said "Nothing to answer" over an empty
    // dismissed list.
    const held: Array<(inbox: curation.Inbox) => void> = []
    let first = true
    vi.mocked(curation.listProposals).mockImplementation(() => {
      if (first) {
        first = false
        return Promise.resolve({
          proposals: [proposal({ id: 1 }), proposal({ id: 2 })],
          counts: { 'edition-of': 2 },
        })
      }
      return new Promise<curation.Inbox>((resolve) => {
        held.push(resolve)
      })
    })
    const host = await settle()
    const header = () => host.querySelector('h1')?.textContent
    const empty = () => host.querySelector('section.empty')?.textContent
    const pressed = () =>
      stateButtons(host)
        .filter((b) => b.getAttribute('aria-pressed') === 'true')
        .map((b) => b.textContent)
    expect(header()).toBe('2 open')

    await click(stateButton(host, 'Dismissed'))
    expect(pressed()).toEqual(['Dismissed'])
    expect(lastCall().state).toBe('dismissed')
    expect(header()).toBe('2 open')
    expect(host.querySelectorAll('li')).toHaveLength(2)

    await act(async () => {
      held[0]?.({ proposals: [], counts: {} })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(header()).toBe('0 dismissed')
    expect(empty()).toContain('Nothing dismissed')

    await click(stateButton(host, 'Open'))
    expect(pressed()).toEqual(['Open'])
    expect(header()).toBe('0 dismissed')
    expect(empty()).toContain('Nothing dismissed')
    expect(empty()).not.toContain('Nothing to answer')

    await act(async () => {
      held[1]?.({
        proposals: [proposal({ id: 1 }), proposal({ id: 2 })],
        counts: { 'edition-of': 2 },
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(header()).toBe('2 open')
    expect(host.querySelector('section.empty')).toBeNull()
  })
})

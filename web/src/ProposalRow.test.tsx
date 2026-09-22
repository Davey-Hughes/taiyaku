import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as curation from './curation'
import { ProposalRow } from './ProposalRow'
import { requestedTileEdge } from './tileSize'
import { mount, rerender } from './testing/mount'

// `Proposals.test.tsx`'s `vi.mock` shape, for the reason its own comment
// gives: `screen`/`userEvent` are not dependencies of this repository, so the
// assertions below drive real DOM elements through `querySelector`.
//
// `resolution` and `acceptProposal` join them for one test: `re-reads the
// inbox when the rule in its panel is accepted`, the only one here that runs
// fake timers and advances past the form's debounce. Every other test in this
// file mounts the same form and reaches neither, because they run real timers
// and the sweep is never sent — measured by taking both out of this mock and
// the `resolution` default out of `beforeEach`, dropping that test and the two
// beside it, and making `globalThis.fetch` throw: the remaining 30 all passed.
// So these two are what one fake-timer test needs, not a wall against thirty
// live requests. What the form *does* with the answer is `AcceptForm.test.tsx`'s
// subject.
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  proposalDetail: vi.fn(),
  dismissProposal: vi.fn(),
  reopenProposal: vi.fn(),
  unacceptProposal: vi.fn(),
  resolution: vi.fn(),
  acceptProposal: vi.fn(),
}))

/**
 * The two titles are the ones `subject()`/`target()` below carry, because the
 * collapsed line and the panel are drawing the same two series from two
 * different payloads — the list's row and the detail's sides — and a fixture
 * where they disagreed would hide a row that read the panel's copy by
 * accident.
 */
const proposal = (
  over: Partial<curation.ListedProposal> = {},
): curation.ListedProposal => ({
  id: 42,
  kind: 'edition-of',
  subject_id: 10,
  target_id: 20,
  subject_title: 'Monster - The Perfect Edition - Colored',
  target_title: 'Monster - The Perfect Edition',
  score: 2,
  reason: 'suffix "Colored" matches the edition vocabulary',
  state: 'open',
  created_rule_id: null,
  ...over,
})

/**
 * **Every field differs from the other side's**, and that is the whole design
 * of these fixtures rather than an incidental choice. The subject and the
 * target are drawn by one component called twice, so a panel that read
 * `detail.subject` for both — or `interior[0]` twice, or the cover index for
 * an interior — renders identically to the correct one against a fixture
 * whose two sides agree. Slug, edition key, page count, formats, interior
 * indices, title, directory, volume count, volume list and root are therefore
 * all different between the two, and none of the numbers repeats.
 */
const subjectVolume = (
  over: Partial<curation.FirstVolume> = {},
): curation.FirstVolume => ({
  work_slug: 'monster-colored-v01',
  edition_key: 'en-color',
  page_count: 412,
  formats: ['zip'],
  interior: [41, 205],
  ...over,
})

const targetVolume = (
  over: Partial<curation.FirstVolume> = {},
): curation.FirstVolume => ({
  work_slug: 'monster-v01',
  edition_key: 'raw',
  page_count: 388,
  formats: ['dir', 'zip'],
  interior: [38, 190],
  ...over,
})

const subject = (over: Partial<curation.Side> = {}): curation.Side => ({
  series_id: 10,
  title: 'Monster - The Perfect Edition - Colored',
  directories: ['/srv/manga/Manga/Monster - The Perfect Edition - Colored/'],
  volume_count: 9,
  volumes: [1, 2, 3],
  root_name: 'Manga',
  first_volume: subjectVolume(),
  ...over,
})

const target = (over: Partial<curation.Side> = {}): curation.Side => ({
  series_id: 20,
  title: 'Monster - The Perfect Edition',
  directories: ['/srv/manga/Library/Monster - The Perfect Edition/'],
  volume_count: 7,
  volumes: [1, 2.5],
  root_name: 'Library',
  first_volume: targetVolume(),
  ...over,
})

const rule = (over: Partial<curation.Rule> = {}): curation.Rule => ({
  root_id: null,
  kind: 'prefix',
  pattern: 'Monster - The Perfect Edition - Colored/',
  match_on: 'path',
  set_series: 'Monster - The Perfect Edition',
  set_edition_key: 'en-color',
  set_kind: 'official',
  set_language: null,
  note: '',
  ...over,
})

const detail = (over: Partial<curation.Detail> = {}): curation.Detail => ({
  proposal: proposal(),
  subject: subject(),
  target: target(),
  reasons: [
    { text: 'suffix "Colored" matches the edition vocabulary', sign: '+' },
    { text: '9 vs 7 volumes', sign: '-' },
    { text: 'target is itself an edition of Monster', sign: '?' },
  ],
  rule: rule(),
  ...over,
})

const noop = () => {}

const row = (expanded: boolean, p: curation.ListedProposal = proposal()) => (
  <ul>
    <ProposalRow
      proposal={p}
      expanded={expanded}
      onToggle={noop}
      onChanged={noop}
    />
  </ul>
)

/** Mount and let the detail request settle — two flushes, as `Series.test.tsx` explains. */
async function settle(ui: React.ReactNode): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(ui)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const open = (
  over: Partial<curation.Detail> = {},
  p?: curation.ListedProposal,
) => {
  vi.mocked(curation.proposalDetail).mockResolvedValue(detail(over))
  return settle(row(true, p))
}

const click = async (el: Element) => {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

const sides = (host: HTMLElement) => [...host.querySelectorAll('.side')]

const sideAt = (host: HTMLElement, i: number): Element => {
  const side = sides(host)[i]
  if (!side) throw new Error(`no side panel at index ${i}`)
  return side
}

const texts = (root: Element | HTMLElement, selector: string) =>
  [...root.querySelectorAll(selector)].map((el) => el.textContent)

const attrs = (host: HTMLElement, selector: string, name: string) =>
  [...host.querySelectorAll(selector)].map((el) => el.getAttribute(name))

/** The accept form's Pattern box — the one control that shows which rule the form mounted against. */
const patternBox = (host: HTMLElement): HTMLInputElement => {
  const box = host.querySelector('.accept-form .field.pattern input')
  if (!box) throw new Error('no pattern box on the accept form')
  return box as HTMLInputElement
}

/** Each fact as `"{term} {value}"`, so a term drawn beside the wrong value shows. */
const facts = (side: Element) =>
  [...side.querySelectorAll('.fact')].map(
    (f) =>
      `${f.querySelector('dt')?.textContent} ${f.querySelector('dd')?.textContent}`,
  )

describe('ProposalRow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(curation.proposalDetail).mockResolvedValue(detail())
    vi.mocked(curation.dismissProposal).mockResolvedValue(undefined)
    vi.mocked(curation.reopenProposal).mockResolvedValue(undefined)
    vi.mocked(curation.resolution).mockResolvedValue({
      also_resolves: [],
      collateral: [],
      targets_reached: [],
      entries_matched: 15,
      subject_reached: true,
      works_moved: 14,
    })
  })

  it('draws its collapsed line from its own proposal, and asks the server for nothing', async () => {
    // Every field away from the fixture's default, so a line drawn from a
    // constant — or from the wrong field — cannot pass: a different kind,
    // two different titles, and a score on the other side of `strength`'s
    // boundary.
    //
    // The two titles differ from each other *and* from the panel fixture's,
    // which is what tells "the line reads the row it was handed" apart from
    // "the line reads whatever the last panel loaded".
    const host = await settle(
      row(
        false,
        proposal({
          kind: 'same-series',
          subject_title: 'Kanojo mo Kanojo',
          target_title: 'Girlfriend, Girlfriend',
          reason: 'both hold 9 volumes',
          score: 1,
        }),
      ),
    )

    expect(host.querySelector('.kind')?.textContent).toBe('Same Series')
    expect(host.querySelector('.subject')?.textContent).toBe('Kanojo mo Kanojo')
    expect(host.querySelector('.arrow')?.textContent).toBe(
      '↳ the same series as Girlfriend, Girlfriend',
    )
    expect(host.querySelector('.score')?.textContent).toBe('weak')

    // The `reason` sentence is the panel's, not the line's — it opens with
    // the subject's own title in quotes, so beside `.subject` it would be
    // the same words twice.
    expect(host.textContent).not.toContain('both hold 9 volumes')
    expect(host.querySelector('li')?.className).toBe('weak')
    expect(
      host.querySelector('button.disclosure')?.getAttribute('aria-expanded'),
    ).toBe('false')

    // The panel is what costs a request, so a collapsed row must not make
    // one — an inbox of 37 rows would otherwise ask for 37 panels to show
    // none of them.
    expect(host.querySelector('.proposal-detail')).toBeNull()
    expect(curation.proposalDetail).not.toHaveBeenCalled()
  })

  it('asks for its own evidence, once, when it is opened', async () => {
    // The id from the proposal it was handed, not a fixed one: every other
    // fixture here uses 42.
    await open({}, proposal({ id: 7 }))

    expect(curation.proposalDetail).toHaveBeenCalledTimes(1)
    expect(curation.proposalDetail).toHaveBeenCalledWith(7)
  })

  it('toggles from the disclosure control and not from the controls beside it', async () => {
    const toggles: number[] = []
    vi.mocked(curation.proposalDetail).mockResolvedValue(detail())
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal()}
          expanded={false}
          onToggle={() => toggles.push(1)}
          onChanged={noop}
        />
      </ul>,
    )

    const disclosure = host.querySelector('button.disclosure')
    const dismiss = host.querySelector('button.dismiss')
    if (!disclosure || !dismiss) throw new Error('the row is missing a control')

    await click(disclosure)
    expect(toggles).toHaveLength(1)

    // Dismiss sits inside the same line; wiring it to the disclosure's
    // handler — or nesting it inside that button — would open the row
    // instead of deciding it.
    await click(dismiss)
    expect(toggles).toHaveLength(1)
  })

  it('shows the whole reason in the panel, which the line above it ellipsises', async () => {
    const host = await open(
      {},
      proposal({ reason: 'a sentence too long for one line of the list' }),
    )

    expect(
      host.querySelector('.proposal-detail .full-reason')?.textContent,
    ).toBe('a sentence too long for one line of the list')
  })

  it('draws a cover and two interior pages per side, each from its own edition', async () => {
    // C3 §2's first decision. Six tiles: the cover is page 0 and the two
    // interiors are the store's own indices, and the two sides name
    // different works, different editions and different pages — so a panel
    // reading one side for both, or a fixed page index, comes out wrong
    // rather than merely equal.
    const host = await open()

    // The box is fixed by `styles/admin.css`'s `.proposal-detail .pages img`
    // — 4.5rem wide at `aspect-ratio: 2 / 3`, so 108 px tall — which is why
    // the rung below is derived from that constant rather than written out as
    // `?w=160`: jsdom reports `devicePixelRatio` 1, so this is the smallest
    // rung, and the arithmetic that matters on a 3× screen is
    // `tileSize.test.ts`'s.
    const rung = requestedTileEdge(108, window.devicePixelRatio)
    expect(attrs(host, '.pages img', 'src')).toEqual([
      `/api/works/monster-colored-v01/en-color/pages/0/thumb?w=${rung}`,
      `/api/works/monster-colored-v01/en-color/pages/41/thumb?w=${rung}`,
      `/api/works/monster-colored-v01/en-color/pages/205/thumb?w=${rung}`,
      `/api/works/monster-v01/raw/pages/0/thumb?w=${rung}`,
      `/api/works/monster-v01/raw/pages/38/thumb?w=${rung}`,
      `/api/works/monster-v01/raw/pages/190/thumb?w=${rung}`,
    ])

    // The tile is a thumbnail linking to the page itself: judging panel
    // layout is what the interiors are for, and a thumbnail is where that
    // starts. `thumbUrl` above and `pageUrl` here are different routes, and
    // a link pointing at the thumbnail would be the smaller image twice.
    expect(attrs(host, '.pages a.page', 'href')).toEqual([
      '/api/works/monster-colored-v01/en-color/pages/0',
      '/api/works/monster-colored-v01/en-color/pages/41',
      '/api/works/monster-colored-v01/en-color/pages/205',
      '/api/works/monster-v01/raw/pages/0',
      '/api/works/monster-v01/raw/pages/38',
      '/api/works/monster-v01/raw/pages/190',
    ])

    // Opened deliberately and one row at a time, so the tiles are cheap —
    // but a tile is still six images per row on a list that scrolls, and the
    // link leaves the panel standing rather than navigating away from the
    // comparison it exists to be.
    expect(attrs(host, '.pages img', 'loading')).toEqual(
      Array.from({ length: 6 }, () => 'lazy'),
    )
    const tile = host.querySelector('.pages a.page')
    expect(tile?.getAttribute('target')).toBe('_blank')
    expect(tile?.getAttribute('rel')).toBe('noreferrer')

    // Each image says which side it belongs to and which page it is; two
    // panels of unnamed thumbnails are indistinguishable to anyone not
    // looking at them.
    expect(attrs(host, '.pages img', 'alt')).toEqual([
      'subject volume 1 cover',
      'subject volume 1 page 42',
      'subject volume 1 page 206',
      'target volume 1 cover',
      'target volume 1 page 39',
      'target volume 1 page 191',
    ])
  })

  it('draws however many interior pages the store sent, which is sometimes fewer and sometimes none', async () => {
    // C3 §8's last line: a member of three pages yields fewer interiors, a
    // one-page member none, and neither may render a broken tile. Two
    // different shortfalls in one fixture, so "always two" and "always one"
    // both fail.
    const host = await open({
      subject: subject({ first_volume: subjectVolume({ interior: [] }) }),
      target: target({ first_volume: targetVolume({ interior: [7] }) }),
    })

    expect(attrs(host, '.pages img', 'alt')).toEqual([
      'subject volume 1 cover',
      'target volume 1 cover',
      'target volume 1 page 8',
    ])
  })

  it('degrades rather than breaking when a side has no volume scanned yet', async () => {
    // A series holding only chapter-organised directories has no `work` row
    // and therefore no first volume. The panel still has to say what it
    // knows — the volume count and the root — rather than drawing three
    // broken tiles or nothing at all.
    const host = await open({ subject: subject({ first_volume: null }) })

    expect(sideAt(host, 0).querySelectorAll('img')).toHaveLength(0)
    expect(sideAt(host, 0).querySelector('.no-volume')?.textContent).toBe(
      'No volume has been scanned for this side yet.',
    )
    // The two facts that come from the missing volume are absent; the two
    // that do not are still drawn.
    expect(facts(sideAt(host, 0))).toEqual(['volumes 9', 'root Manga'])

    // The other side is untouched — a missing volume on one side must not
    // blank the side being compared against it.
    expect(sideAt(host, 1).querySelectorAll('img')).toHaveLength(3)
    expect(sideAt(host, 1).querySelector('.no-volume')).toBeNull()
    expect(facts(sideAt(host, 1))).toEqual([
      'volumes 7',
      'v01 pages 388',
      'format dir · zip',
      'root Library',
    ])
  })

  it('names both sides and states which is which', async () => {
    const host = await open()

    expect(texts(host, '.side .side-role')).toEqual([
      'Subject',
      'Proposed target',
    ])
    expect(texts(host, '.side .side-title')).toEqual([
      'Monster - The Perfect Edition - Colored',
      'Monster - The Perfect Edition',
    ])
  })

  it('draws one side only for a kind that names no target', async () => {
    // `attach-chapters` has no second series, and `Detail.target` is null
    // for it. A panel that assumed two sides would throw on the field it
    // read off `null`.
    //
    // The collapsed line is the same question one payload earlier: the row's
    // own `target_title` is null there, and an arrow line drawn regardless
    // reads `↳ related to null` — which is why the assertion below is for the
    // element's absence rather than for its text.
    const host = await open(
      {
        proposal: proposal({ kind: 'attach-chapters', target_id: null }),
        target: null,
      },
      proposal({
        kind: 'attach-chapters',
        target_id: null,
        subject_title: 'Kaguya-sama - Fan Translated',
        target_title: null,
      }),
    )

    expect(sides(host)).toHaveLength(1)
    expect(sideAt(host, 0).querySelector('.side-role')?.textContent).toBe(
      'Subject',
    )
    expect(host.querySelector('.subject')?.textContent).toBe(
      'Kaguya-sama - Fan Translated',
    )
    expect(host.querySelector('.arrow')).toBeNull()
  })

  it('shows the one path on disk, and the first plus a count where a side spans several', async () => {
    // `Side.directories` is a list and is rendered as one. Both branches in
    // one fixture, on different sides, so "always the first" and "always the
    // count" each fail — and the path is shown verbatim in a `<code>`, which
    // is C3 §2's own wording.
    const host = await open({
      target: target({
        directories: [
          '/srv/manga/Library/Monster - The Perfect Edition/',
          '/srv/manga/Publisher/Monster - The Perfect Edition/',
          '/srv/manga/Scans/Monster - The Perfect Edition/',
        ],
      }),
    })

    expect(texts(sideAt(host, 0), '.directories code')).toEqual([
      '/srv/manga/Manga/Monster - The Perfect Edition - Colored/',
    ])
    expect(sideAt(host, 0).querySelector('.more')).toBeNull()

    // The first path, and a count of the rest — not all three joined, and
    // not a count of all three.
    expect(texts(sideAt(host, 1), '.directories code')).toEqual([
      '/srv/manga/Library/Monster - The Perfect Edition/',
    ])
    expect(sideAt(host, 1).querySelector('.more')?.textContent).toBe('+2 more')
    expect(sideAt(host, 1).textContent).not.toContain('/srv/manga/Scans/')
  })

  it('says so when nothing on disk resolves to a side at all', async () => {
    // The empty list, which is not only "never scanned": a rename or an
    // edited rule leaves a series with a real volume count and a real first
    // volume while nothing resolves to it any more — so the side keeps
    // drawing everything else.
    const host = await open({ subject: subject({ directories: [] }) })

    expect(sideAt(host, 0).querySelector('.directories')).toBeNull()
    // The whole sentence. Its tail — "it has not been scanned, or a rename or
    // a rule moved it" — is the half that says an empty list is *not* only
    // "never scanned", which is `Side.directories`' own documented contract
    // and the thing a `toContain` on the first clause left free to be deleted.
    expect(sideAt(host, 0).querySelector('.no-directory')?.textContent).toBe(
      'Nothing on disk resolves to this series today — it has not been ' +
        'scanned, or a rename or a rule moved it.',
    )
    expect(sideAt(host, 0).querySelectorAll('img')).toHaveLength(3)
    expect(facts(sideAt(host, 0))).toEqual([
      'volumes 9',
      'v01 pages 412',
      'format zip',
      'root Manga',
    ])

    // The other side still has its path.
    expect(texts(sideAt(host, 1), '.directories code')).toEqual([
      '/srv/manga/Library/Monster - The Perfect Edition/',
    ])
  })

  it('lists every format the edition holds, and says so when it has none', async () => {
    // `formats` is edition-wide and a list: an edition can hold a directory
    // and a `.cbz` at once, so naming one of them would describe the other
    // wrongly.
    const host = await open({
      subject: subject({
        first_volume: subjectVolume({ formats: ['cbz', 'dir', 'zip'] }),
      }),
      target: target({ first_volume: targetVolume({ formats: [] }) }),
    })

    expect(facts(sideAt(host, 0))[2]).toBe('format cbz · dir · zip')
    expect(facts(sideAt(host, 1))[2]).toBe('format —')
  })

  it('says a side has no root rather than drawing an empty cell', async () => {
    // `root_name` is null where a side's entries sit under several roots or
    // under none.
    const host = await open({ subject: subject({ root_name: null }) })

    expect(facts(sideAt(host, 0))[3]).toBe('root —')
    expect(facts(sideAt(host, 1))[3]).toBe('root Library')
  })

  it('lists each side own volume numbers, fractions included', async () => {
    // C3 §2 asks for the list so a mismatch is visible rather than
    // summarised — 9 works against a list of three is exactly that, and the
    // two sides disagree on both the count and the numbers.
    const host = await open()

    expect(texts(sideAt(host, 0), '.volume-list .vol')).toEqual(['1', '2', '3'])
    expect(texts(sideAt(host, 1), '.volume-list .vol')).toEqual(['1', '2.5'])
  })

  it('draws a repeated volume number twice rather than collapsing it', async () => {
    // The store's query has no `DISTINCT`, so a series holding two works
    // numbered 1 sends `[1, 1, 2]` — which is exactly the mismatch this list
    // exists to make visible.
    //
    // `console.error` is watched because rendering is not where keying by the
    // number goes wrong: React draws both chips and *reports* the collision,
    // and its own message for it says the second child will be discarded. A
    // test that only read the three chips would pass on the broken key.
    //
    // Filtered rather than asserted empty: every mount in this project logs
    // React's "not configured to support act(...)" line, which is nothing to
    // do with this and would make the assertion fail for a reason it is not
    // about.
    const complaints = vi.spyOn(console, 'error').mockImplementation(() => {})
    const host = await open({ subject: subject({ volumes: [1, 1, 2] }) })

    expect(texts(sideAt(host, 0), '.volume-list .vol')).toEqual(['1', '1', '2'])
    expect(
      complaints.mock.calls.filter((call) =>
        call.some((arg) => typeof arg === 'string' && /same key/i.test(arg)),
      ),
    ).toEqual([])
    complaints.mockRestore()
  })

  it('lists each scored signal on its own line with its sign', async () => {
    // Three signs, all different: a list that dropped the sign, or drew one
    // sign for every line, reads the same as this one against a fixture
    // where every reason is a `+`.
    const host = await open()

    const lines = [...host.querySelectorAll('.reasons li')].map(
      (li) =>
        `${li.querySelector('.sign')?.textContent} ${li.querySelector('.text')?.textContent}`,
    )
    expect(lines).toEqual([
      '+ suffix "Colored" matches the edition vocabulary',
      '- 9 vs 7 volumes',
      '? target is itself an edition of Monster',
    ])

    // The heading over them, which nothing held: the list is a list of
    // *signals behind a score*, and unlabelled above a panel of paths and
    // page thumbnails it reads as a description of the artwork.
    expect(host.querySelector('h2.lab')?.textContent).toBe(
      'Why this was proposed',
    )
  })

  it('says plainly that nothing has compared the artwork, and offers a control that admits it has not run', async () => {
    // C3 §2's second decision. Every signal above is about names and counts;
    // an absent number that looked like a low score would be worse than a
    // control that says it has not run.
    const host = await open()

    const measured = host.querySelector('.measured')
    expect(measured?.textContent).toContain('Nothing here compared the artwork')
    expect(measured?.textContent).toContain('duplicates --against')
    // **The clause that makes the sentence true rather than merely
    // impressive.** `duplicates --against` exists and answers this question;
    // the whole claim is that it has *not been run* here. Deleting those six
    // words leaves a paragraph that reads as if the comparison had happened,
    // and every assertion above it passes.
    expect(measured?.textContent).toContain('and it has not been run')

    const compare = measured?.querySelector<HTMLButtonElement>('button.compare')
    expect(compare?.textContent).toBe('Compare v01')
    expect(compare?.disabled).toBe(true)

    // And the line under the control says what would have to change, in
    // full: inverted to "Already available: the fingerprinting code lives in
    // `taiyaku-cli`" it contradicts the disabled control beside it, and
    // nothing failed.
    expect(measured?.querySelector('.not-yet')?.textContent).toBe(
      'Not wired up yet: the fingerprinting code has to leave taiyaku-cli ' +
        'first.',
    )
  })

  it('says an open proposal has no rule it could be accepted as', async () => {
    const host = await open({ rule: null })

    expect(host.querySelector('.no-rule')?.textContent).toBe(
      'There is no rule this proposal could be accepted as, as it stands.',
    )
  })

  it('does not guess at why, on the one state where guessing was wrong', async () => {
    // **The sentence used to enumerate, and the enumeration was drawn over a
    // panel that contradicted it.** `evidence` hands `offered_rule` the same
    // `paths_of(&subject_title)` it hands `side`, so an empty `directories`
    // is not merely *consistent with* `rule: null` — it causes it:
    // `common_directory_pattern` refuses on `dirs.is_empty()`. Every open
    // proposal in this state therefore drew "Nothing on disk resolves to this
    // series today — it has not been scanned" and, four inches below, "its
    // subject spans more than one directory".
    //
    // It also read as exhaustive, joined by "or", while naming three of at
    // least six: `offered_rule` also refuses a subject sitting under one
    // directory name in *two roots*, a suffix that slugifies to nothing, and
    // a proposal that is not open. This is the pairing that would have caught
    // it, and it is the fixture that was missing.
    const host = await open({
      subject: subject({ directories: [] }),
      rule: null,
    })

    expect(sideAt(host, 0).querySelector('.no-directory')?.textContent).toBe(
      'Nothing on disk resolves to this series today — it has not been ' +
        'scanned, or a rename or a rule moved it.',
    )
    const noRule = host.querySelector('.no-rule')?.textContent
    expect(noRule).toBe(
      'There is no rule this proposal could be accepted as, as it stands.',
    )
    expect(noRule).not.toContain('directory')
  })

  it('does not tell an already-decided proposal it cannot be accepted', async () => {
    // `evidence` returns `rule: null` for *every* refusal and checks the
    // state before anything else — so an accepted proposal reads null here
    // too. The state that decides this is the one the detail carries, not
    // the one the list handed the row, which may predate the accept: this
    // fixture makes the two disagree on purpose.
    const host = await open(
      {
        proposal: proposal({ state: 'accepted', created_rule_id: 3 }),
        rule: null,
      },
      proposal({ state: 'open' }),
    )

    expect(host.querySelector('.no-rule')).toBeNull()
  })

  it('says nothing about rules when there is a rule to accept', async () => {
    const host = await open()

    expect(host.querySelector('.no-rule')).toBeNull()
  })

  it('offers the accept form for the rule the detail carries', async () => {
    // The seam, and which of the panel's two rule-shaped payloads reaches
    // it: the form is handed `detail.rule`, so its pattern box holds that
    // rule's pattern rather than a re-derivation of it. `AcceptForm.test.tsx`
    // owns what the form then does.
    const host = await open()

    expect(
      host.querySelector<HTMLInputElement>('.accept-form .field.pattern input')
        ?.value,
    ).toBe('Monster - The Perfect Edition - Colored/')
  })

  it('draws no accept form where there is no rule to write', async () => {
    // The other side of `.no-rule`. A form defaulted from a null rule has no
    // pattern, no key and nothing to preview; the panel says so in one line
    // instead.
    const host = await open({ rule: null })

    expect(host.querySelector('.accept-form')).toBeNull()
    expect(host.querySelector('.no-rule')).not.toBeNull()
  })

  it('re-reads the inbox when the rule in its panel is accepted', async () => {
    // `onChanged`'s other caller. Dismissing already asks for it and an
    // accept moves the same two things — the row leaves the open list and
    // its kind's tab count drops — so a form wired to nothing would leave an
    // accepted proposal sitting in the inbox until a filter was touched.
    //
    // Fake timers only here: the form sweeps the library on a debounce
    // before its Accept control becomes available, and nothing else in this
    // file waits for that.
    vi.useFakeTimers()
    try {
      const changes: number[] = []
      vi.mocked(curation.acceptProposal).mockResolvedValue({ rule_id: 91 })
      const host = await settle(
        <ul>
          <ProposalRow
            proposal={proposal()}
            expanded={true}
            onToggle={noop}
            onChanged={() => changes.push(1)}
          />
        </ul>,
      )
      await act(async () => {
        vi.advanceTimersByTime(300)
        await Promise.resolve()
        await Promise.resolve()
      })

      // The target's title, not the subject's — the two differ by a suffix,
      // and the sentence names the series being accepted *into*.
      expect(
        host.querySelector('.accept-form .consequence')?.textContent,
      ).toContain('to series Monster - The Perfect Edition, from')
      const control = host.querySelector<HTMLButtonElement>(
        '.accept-form button.accept',
      )
      if (!control) throw new Error('no accept control rendered')
      expect(control.disabled).toBe(false)

      await click(control)

      // The id off the detail, and the rule off the detail: 42 is this
      // fixture's proposal and nothing else here is numbered.
      expect(curation.acceptProposal).toHaveBeenCalledWith(42, rule())
      expect(changes).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('says the evidence is on its way while it is', async () => {
    vi.mocked(curation.proposalDetail).mockImplementation(
      () => new Promise(() => {}),
    )

    const host = await settle(row(true))

    expect(host.querySelector('.loading')?.textContent).toBe(
      'Reading the evidence…',
    )
    expect(host.querySelector('.sides')).toBeNull()
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('says why the evidence could not be read, instead of an empty panel', async () => {
    vi.mocked(curation.proposalDetail).mockRejectedValue(
      new Error('/api/library/proposals/42: 500'),
    )

    const host = await settle(row(true))

    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/proposals/42: 500',
    )
    expect(host.querySelector('.sides')).toBeNull()
    expect(host.querySelector('.loading')).toBeNull()
  })

  it('stringifies a rejection that is not an Error, rather than losing it', async () => {
    // `Proposals.test.tsx` and `Shell.test.tsx` pin the same fallback for the
    // same reason: every other failure fixture rejects with a real `Error`,
    // so `String(err)` has never run.
    vi.mocked(curation.proposalDetail).mockRejectedValue('offline')

    const host = await settle(row(true))

    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })

  it('clears a failure once an answer arrives, rather than leaving the alert above it', async () => {
    // Closing and reopening the row is the retry this panel has, and a
    // failure left in place would sit above evidence that loaded correctly —
    // the same defect `Proposals.tsx`'s banner clears on success.
    vi.mocked(curation.proposalDetail)
      .mockRejectedValueOnce(new Error('/api/library/proposals/42: 500'))
      .mockResolvedValue(detail())

    const host = await settle(row(true))
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/proposals/42: 500',
    )

    await act(async () => {
      rerender(row(false))
      await Promise.resolve()
    })
    await act(async () => {
      rerender(row(true))
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelectorAll('.side')).toHaveLength(2)
  })

  it('discards a stale answer that lands after the row was reopened', async () => {
    // The same guard `Proposals.tsx` carries, for the same reason: closing
    // and reopening a row starts a second request, and the first one landing
    // late would paint the panel with an answer nobody is waiting for. The
    // two answers differ in the title they draw, so "the second won" and
    // "the first won" are distinguishable.
    let resolveFirst: ((d: curation.Detail) => void) | undefined
    let resolveSecond: ((d: curation.Detail) => void) | undefined
    vi.mocked(curation.proposalDetail)
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

    const host = await settle(row(true))
    await act(async () => {
      rerender(row(false))
      await Promise.resolve()
    })
    await act(async () => {
      rerender(row(true))
      await Promise.resolve()
    })
    if (!resolveSecond) throw new Error('the second request never started')

    await act(async () => {
      resolveSecond?.(
        detail({ subject: subject({ title: 'the current answer' }) }),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.side-title')?.textContent).toBe(
      'the current answer',
    )

    await act(async () => {
      resolveFirst?.(
        detail({ subject: subject({ title: 'the stale answer' }) }),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.side-title')?.textContent).toBe(
      'the current answer',
    )
  })

  it('mounts the accept form against the rule the panel is showing, not the one the last expand showed', async () => {
    // **The defect this catches is a rule spliced from two answers.**
    // `AcceptForm` seeds all five of its controls with `useState`, which runs
    // once per mount, while its `candidate` spreads the *current* `rule`
    // prop over them. So a form instance kept across two different offered
    // rules sends `pattern`, `kind` and the three `set_*` fields from the
    // rule it mounted with and `root_id`, `match_on`, `set_series` and `note`
    // from the one now underneath it — and `set_series` and `note` have no
    // control on the form, so the half that changed is invisible on screen.
    // `offered_rule` re-derives per request, off live `scan_entry` rows and
    // the target's current title, so another accept, a background scan or a
    // rename between two expands is enough to make the two rules differ.
    //
    // **`discards a stale answer that lands after the row was reopened`
    // above walks this same path and cannot see it**: its first request never
    // resolves, so `detail` is still `undefined` at the collapse and there is
    // nothing retained to mount a form against. The first detail has to
    // *land* for the retention to exist, which is what this fixture does.
    //
    // The two assertions are one pair on purpose: the panel drawing the new
    // subject's title while the pattern box still holds the old rule's
    // pattern is the splice itself, made visible. Asserting the pattern alone
    // would not tell "the form kept the old rule" apart from "the second
    // request never arrived".
    vi.mocked(curation.proposalDetail)
      .mockResolvedValueOnce(detail())
      .mockResolvedValueOnce(
        detail({
          subject: subject({ title: 'the re-derived subject' }),
          rule: rule({ pattern: 'Monster - The Perfect Edition -/' }),
        }),
      )

    const host = await settle(row(true))
    expect(host.querySelector('.side-title')?.textContent).toBe(
      'Monster - The Perfect Edition - Colored',
    )
    expect(patternBox(host).value).toBe(
      'Monster - The Perfect Edition - Colored/',
    )

    await act(async () => {
      rerender(row(false))
      await Promise.resolve()
    })
    await act(async () => {
      rerender(row(true))
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(host.querySelector('.side-title')?.textContent).toBe(
      'the re-derived subject',
    )
    expect(patternBox(host).value).toBe('Monster - The Perfect Edition -/')
  })

  it('discards a stale rejection that lands after the row already has its evidence', async () => {
    // The `catch` side of the same guard: `.then` and `.catch` each carry
    // their own `cancelled` check, and mutating one leaves the other test
    // above green.
    let rejectFirst: ((err: unknown) => void) | undefined
    let resolveSecond: ((d: curation.Detail) => void) | undefined
    vi.mocked(curation.proposalDetail)
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

    const host = await settle(row(true))
    await act(async () => {
      rerender(row(false))
      await Promise.resolve()
    })
    await act(async () => {
      rerender(row(true))
      await Promise.resolve()
    })
    if (!resolveSecond) throw new Error('the second request never started')

    await act(async () => {
      resolveSecond?.(detail())
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()

    await act(async () => {
      rejectFirst?.(new Error('/api/library/proposals/42: 500'))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('.sides')).not.toBeNull()
  })

  it('dismisses its own proposal and asks the screen to re-read the inbox', async () => {
    // C3 §4: dismissing writes no rule and needs no form — doing nothing
    // already leaves the directory its own series, so a dismissal only
    // confirms that. Both the list and the tab counts move, which is what
    // `onChanged` is for.
    const changes: number[] = []
    let resolveDismiss: (() => void) | undefined
    vi.mocked(curation.dismissProposal).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveDismiss = () => {
            resolve(undefined)
          }
        }),
    )
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal({ id: 7 })}
          expanded={false}
          onToggle={noop}
          onChanged={() => changes.push(1)}
        />
      </ul>,
    )

    const dismiss = host.querySelector<HTMLButtonElement>('button.dismiss')
    if (!dismiss) throw new Error('no dismiss control rendered')
    // The word on it, not only the class the tests find it by: a control
    // nobody can read is one nobody presses.
    expect(dismiss.textContent).toBe('Dismiss')
    expect(dismiss.disabled).toBe(false)

    await click(dismiss)
    expect(curation.dismissProposal).toHaveBeenCalledWith(7)
    // In flight: a second press would dismiss the same proposal twice.
    expect(dismiss.disabled).toBe(true)
    expect(changes).toHaveLength(0)

    await act(async () => {
      resolveDismiss?.()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(changes).toHaveLength(1)
    expect(dismiss.disabled).toBe(false)
  })

  it('says why a dismissal failed, and lets it be tried again', async () => {
    const changes: number[] = []
    vi.mocked(curation.dismissProposal)
      .mockRejectedValueOnce(
        new Error('/api/library/proposals/42/dismiss: 403'),
      )
      .mockResolvedValue(undefined)
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal()}
          expanded={false}
          onToggle={noop}
          onChanged={() => changes.push(1)}
        />
      </ul>,
    )

    const dismiss = host.querySelector<HTMLButtonElement>('button.dismiss')
    if (!dismiss) throw new Error('no dismiss control rendered')

    await click(dismiss)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '/api/library/proposals/42/dismiss: 403',
    )
    // A failed write must not tell the screen to re-read a list that did not
    // change, and must leave the control usable.
    expect(changes).toHaveLength(0)
    expect(dismiss.disabled).toBe(false)

    await click(dismiss)
    expect(curation.dismissProposal).toHaveBeenCalledTimes(2)
    expect(changes).toHaveLength(1)
    // The failure belonged to the attempt that failed, not to the row.
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('stringifies a dismissal rejection that is not an Error', async () => {
    vi.mocked(curation.dismissProposal).mockRejectedValue('offline')
    const host = await settle(row(false))

    const dismiss = host.querySelector('button.dismiss')
    if (!dismiss) throw new Error('no dismiss control rendered')
    await click(dismiss)

    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })
  /** The line's own write controls, beside the disclosure, by their words. */
  const lineControls = (host: HTMLElement) =>
    [...host.querySelectorAll<HTMLButtonElement>('.row > button')]
      .filter((b) => !b.classList.contains('disclosure'))
      .map((b) => `${b.className}:${b.textContent}`)

  it('offers each state exactly its own decision on the line, and an accepted row none (O72)', async () => {
    // Asserted as the whole list per state rather than "has a Reopen": a
    // row that drew Dismiss beside Reopen, or Reopen on an accepted row —
    // an Undo that O74 says would not undo — passes any check for presence.
    // Three rows in one mount, so a control keyed to the first row's state
    // rather than each row's own is visible too.
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal({ id: 1, state: 'open' })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
        <ProposalRow
          proposal={proposal({ id: 2, state: 'dismissed' })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
        <ProposalRow
          proposal={proposal({ id: 3, state: 'accepted', created_rule_id: 9 })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
      </ul>,
    )

    const rows = [...host.querySelectorAll<HTMLElement>('li')]
    expect(rows).toHaveLength(3)
    expect(rows.map(lineControls)).toEqual([
      ['dismiss:Dismiss'],
      ['reopen:Reopen'],
      [],
    ])
  })

  it('reopens its own dismissed proposal and asks the screen to re-read the inbox', async () => {
    const changes: number[] = []
    let resolveReopen: (() => void) | undefined
    vi.mocked(curation.reopenProposal).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveReopen = () => {
            resolve(undefined)
          }
        }),
    )
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal({ id: 7, state: 'dismissed' })}
          expanded={false}
          onToggle={noop}
          onChanged={() => changes.push(1)}
        />
      </ul>,
    )

    const reopen = host.querySelector<HTMLButtonElement>('button.reopen')
    if (!reopen) throw new Error('no reopen control rendered')
    expect(reopen.disabled).toBe(false)

    await click(reopen)
    // Its own write, to its own proposal — and not the dismissal it undoes.
    expect(curation.reopenProposal).toHaveBeenCalledWith(7)
    expect(curation.dismissProposal).not.toHaveBeenCalled()
    expect(reopen.disabled).toBe(true)
    expect(changes).toHaveLength(0)

    await act(async () => {
      resolveReopen?.()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(changes).toHaveLength(1)
    expect(reopen.disabled).toBe(false)
  })

  it('says why a reopen failed, and re-reads nothing', async () => {
    const changes: number[] = []
    vi.mocked(curation.reopenProposal).mockRejectedValueOnce(
      new Error('proposal 42 is accepted, not dismissed'),
    )
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal({ state: 'dismissed' })}
          expanded={false}
          onToggle={noop}
          onChanged={() => changes.push(1)}
        />
      </ul>,
    )
    const reopen = host.querySelector<HTMLButtonElement>('button.reopen')
    if (!reopen) throw new Error('no reopen control rendered')

    await click(reopen)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'proposal 42 is accepted, not dismissed',
    )
    expect(changes).toHaveLength(0)
    expect(reopen.disabled).toBe(false)
  })

  it('sends one write for two presses in one frame, before disabled can take effect', async () => {
    // Both presses inside one `act()`: that is the window between the first
    // press and the re-render that disables the control, and the only one a
    // ref guard is for. Pressing, awaiting and pressing again tests
    // `disabled` instead. Both of the line's writes share the guard, so both
    // are driven.
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal({ id: 5, state: 'open' })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
        <ProposalRow
          proposal={proposal({ id: 6, state: 'dismissed' })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
      </ul>,
    )
    const dismiss = host.querySelector<HTMLButtonElement>('button.dismiss')
    const reopen = host.querySelector<HTMLButtonElement>('button.reopen')
    if (!dismiss || !reopen) throw new Error('a line control is missing')

    await act(async () => {
      dismiss.click()
      dismiss.click()
      reopen.click()
      reopen.click()
      await Promise.resolve()
    })

    expect(vi.mocked(curation.dismissProposal).mock.calls).toEqual([[5]])
    expect(vi.mocked(curation.reopenProposal).mock.calls).toEqual([[6]])
  })
  it('offers a decided row no accept, dismiss or un-accept control, collapsed or opened (O72)', async () => {
    // **Every button in the row, not the line's direct children.** The
    // per-state test above reads `.row > button`, so an Undo drawn inside a
    // `<span>` on the line, or in the opened panel, passed it — and O74 is
    // why no Undo may exist: un-accepting leaves what the rule derived
    // standing. The panel is loaded, with `rule: null` as the server sends
    // for any proposal not open, so what it draws is on screen. Every
    // enabled control is then pressed, so a control that reached a write by
    // some other name is caught by the call rather than by its label.
    const expected = {
      accepted: {
        collapsed: ['disclosure'],
        opened: ['disclosure', 'compare:Compare v01'],
      },
      dismissed: {
        collapsed: ['disclosure', 'reopen:Reopen'],
        opened: ['disclosure', 'reopen:Reopen', 'compare:Compare v01'],
      },
    } as const
    for (const state of ['accepted', 'dismissed'] as const) {
      for (const expanded of [false, true]) {
        vi.clearAllMocks()
        const p = proposal({
          id: 31,
          state,
          created_rule_id: state === 'accepted' ? 9 : null,
        })
        vi.mocked(curation.proposalDetail).mockResolvedValue(
          detail({ proposal: p, rule: null }),
        )
        const host = await settle(row(expanded, p))
        if (expanded) {
          expect(host.querySelector('.sides')).not.toBeNull()
        }

        const buttons = [
          ...host.querySelectorAll<HTMLButtonElement>(
            'li button, li [role="button"]',
          ),
        ]
        expect(
          buttons.map((b) =>
            b.classList.contains('disclosure')
              ? 'disclosure'
              : `${b.className}:${b.textContent}`,
          ),
        ).toEqual(expected[state][expanded ? 'opened' : 'collapsed'])

        for (const b of buttons) {
          if (b.classList.contains('disclosure') || b.disabled) continue
          await click(b)
        }
        expect(curation.unacceptProposal).not.toHaveBeenCalled()
        expect(curation.acceptProposal).not.toHaveBeenCalled()
        expect(curation.dismissProposal).not.toHaveBeenCalled()
      }
    }
  })

  it('takes a second press after a write succeeds, while the row is still on screen', async () => {
    // A successful write asks the screen to reload, and a reload that fails
    // leaves this very row mounted — so the line has to answer again. Its
    // `disabled` comes from `writing`, which is released on success whether
    // or not the ref is, so only a second press can see a ref left held:
    // the row would look ready and send nothing. Both of the line's writes,
    // though they share one runner, so a split runner stays covered.
    const host = await settle(
      <ul>
        <ProposalRow
          proposal={proposal({ id: 5, state: 'open' })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
        <ProposalRow
          proposal={proposal({ id: 6, state: 'dismissed' })}
          expanded={false}
          onToggle={noop}
          onChanged={noop}
        />
      </ul>,
    )
    const dismiss = host.querySelector<HTMLButtonElement>('button.dismiss')
    const reopen = host.querySelector<HTMLButtonElement>('button.reopen')
    if (!dismiss || !reopen) throw new Error('a line control is missing')

    await click(dismiss)
    expect(dismiss.disabled).toBe(false)
    await click(dismiss)
    await click(reopen)
    expect(reopen.disabled).toBe(false)
    await click(reopen)

    expect(vi.mocked(curation.dismissProposal).mock.calls).toEqual([[5], [5]])
    expect(vi.mocked(curation.reopenProposal).mock.calls).toEqual([[6], [6]])
  })
})

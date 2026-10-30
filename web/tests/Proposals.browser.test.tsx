import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import * as curation from '../src/curation'
import '../src/index.css'
import { Proposals } from '../src/Proposals'
import { mount } from './support/mount'

/**
 * The collapsed inbox row, laid out — which is the one thing about it neither
 * of the other two projects can see.
 *
 * **Why this file exists, and what the pass before it got wrong.** C3 §3
 * chose a scannable list over a one-at-a-time triage loop for a single
 * reason: with ~37 proposals mostly of one kind, a person should be able to
 * *see* that 29 of 37 are the same question and answer it once. That is a
 * claim about what a row shows at a given width, so it is a claim only a
 * layout engine can check — `jsdom` returns zeroes from
 * `getBoundingClientRect`, and every assertion in `Proposals.test.tsx` and
 * `ProposalRow.test.tsx` would pass just as happily on a row ellipsised to
 * ten characters.
 *
 * A browser pass had already looked at this screen and found it fine. It was
 * driven with **invented `reason` strings the server never emits**, at one
 * width, so it flattered the wide case and never rendered the narrow one. The
 * fixtures below are the store's own sentences, in the shapes
 * `raise_edition_of` and `raise_attach_chapters` write them — the two
 * `format!` calls were printed out of `proposals/tests.rs`' own fixtures, not
 * written from memory — and both widths are here because the defect lived at
 * exactly one of them.
 *
 * **460 and 1100 are not arbitrary.** 460px since the style pass's ②: below
 * 900px the sidebar leaves the flow, and 460 − 2 × 16 gives `main` the 428px
 * that 700 − 224 − 48 gave it beside the 14rem sidebar — the same list width
 * the row reflows at either way. At 1100px `.library` is still the fixed
 * 14rem sidebar plus `main`'s 1.5rem padding, so the list is 828px there —
 * either side of the 32rem container query that reflows the row.
 * The `@container inbox` comment in `styles/admin.css` says why it is 32rem.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium), so
 * nothing here is a gate. It is the measurement, kept runnable, so the next
 * person to change this row can re-take it instead of re-inventing it.
 */

vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
  proposalDetail: vi.fn(),
  dismissProposal: vi.fn(),
  reopenProposal: vi.fn(),
}))

/** The store's own wording. */
const ROWS: curation.ListedProposal[] = [
  {
    id: 1,
    kind: 'edition-of',
    subject_id: 1,
    target_id: 2,
    subject_title: 'Monster - The Perfect Edition - Colored',
    target_title: 'Monster - The Perfect Edition',
    score: 2,
    reason:
      '"Monster - The Perfect Edition - Colored" may be the "Colored" edition of "Monster - The Perfect Edition" — the suffix matches the edition vocabulary; both hold 9 volume(s)',
    state: 'open',
    created_rule_id: null,
  },
  {
    id: 2,
    kind: 'edition-of',
    subject_id: 3,
    target_id: 4,
    subject_title: 'Go-Toubun no Hanayome - Full Color Edition',
    target_title: 'Go-Toubun no Hanayome',
    score: 2,
    reason:
      '"Go-Toubun no Hanayome - Full Color Edition" may be the "Full Color Edition" edition of "Go-Toubun no Hanayome" — the suffix matches the edition vocabulary; both hold 14 volume(s)',
    state: 'open',
    created_rule_id: null,
  },
  {
    // **The long one, and it is here to be cut.** Every other fixture fits
    // the column at 1100px, so nothing else in this file can tell "the title
    // is ellipsised inside its track" apart from "the track grew to fit it
    // and the list is now wider than `main`" — which is the failure
    // `minmax(0, 1fr)` and `min-width: 0` exist to prevent, and which both
    // survived their own deletion until this row existed. Its shape is the
    // mockup's own second row plus an edition suffix.
    id: 3,
    kind: 'edition-of',
    subject_id: 5,
    target_id: 6,
    subject_title:
      'Kobayashi-san Chi no Meidoragon - Kanna no Nichijo - Full Color Edition',
    target_title: 'Kobayashi-san Chi no Meidoragon - Kanna no Nichijo',
    score: 2,
    reason:
      '"Kobayashi-san Chi no Meidoragon - Kanna no Nichijo - Full Color Edition" may be the "Full Color Edition" edition of "Kobayashi-san Chi no Meidoragon - Kanna no Nichijo" — the suffix matches the edition vocabulary; both hold 11 volume(s)',
    state: 'open',
    created_rule_id: null,
  },
  {
    id: 4,
    kind: 'attach-chapters',
    subject_id: 7,
    target_id: null,
    subject_title: 'Kaguya-sama - Fan Translated',
    target_title: null,
    score: 3,
    reason:
      '"Kaguya-sama - Fan Translated" holds one directory organised by chapter, not volume — only alignment can say which volume it belongs to, e.g. `taiyaku align raw-v01.cbz -- en-c001.cbz en-c002.cbz …`',
    state: 'open',
    created_rule_id: null,
  },
]

/** The inbox at one window width, inside the sidebar-plus-main shell it lives in. */
async function inboxAt(width: number): Promise<HTMLElement> {
  await page.viewport(width, 640)
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: ROWS,
    counts: { 'edition-of': 29, 'same-series': 2, 'attach-chapters': 6 },
  })
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <div className="library" style={{ height: '640px' }}>
        <div className="sidebar-column" />
        <main>
          <Proposals />
        </main>
      </div>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  // A track sized by its text measures narrower before the web fonts arrive —
  // 3.3px for the reflowed row at 460px, 6.2px for the title column at 1100px —
  // so every measurement in this file is taken once they have.
  await document.fonts.ready
  if (!host) throw new Error('nothing mounted')
  return host
}

const rows = (host: HTMLElement) => [...host.querySelectorAll('li')]

const boxOf = (row: Element, selector: string): number =>
  row.querySelector(selector)?.getBoundingClientRect().width ?? 0

/** How much of an element's text the box actually shows. */
const cut = (row: Element, selector: string): boolean => {
  const el = row.querySelector(selector)
  if (!el) throw new Error(`no ${selector} in this row`)
  return el.scrollWidth > Math.ceil(el.getBoundingClientRect().width)
}

/**
 * A title too long for its column is *ellipsised inside it*, not laid out at
 * its natural width with the list grown around it.
 *
 * Both halves, because either alone passes on the broken layout: a box that
 * grew to fit its text is not `cut` (nothing overflows it), and a box that is
 * cut could still be sitting wider than the column it is supposed to be in.
 */
function ellipsisedInsideItsColumn(row: Element, selector: string) {
  expect(cut(row, selector)).toBe(true)
  expect(boxOf(row, selector)).toBeLessThanOrEqual(boxOf(row, '.what'))
}

describe('the collapsed inbox row, in a browser', () => {
  it('gives each row its two titles at 1100px, uncut', async () => {
    const host = await inboxAt(1100)
    const listed = rows(host)
    expect(listed).toHaveLength(4)

    // Measured: the list is 828px and the title column 506.6px of it, with the
    // web fonts loaded.
    const first = listed[0]
    if (!first) throw new Error('no first row')
    expect(boxOf(first, '.what')).toBeGreaterThan(400)
    expect(cut(first, '.subject')).toBe(false)
    expect(cut(first, '.arrow')).toBe(false)
    expect(first.querySelector('.subject')?.textContent).toBe(
      'Monster - The Perfect Edition - Colored',
    )
    expect(first.querySelector('.arrow')?.textContent).toBe(
      '↳ an edition of Monster - The Perfect Edition',
    )

    // The two `edition-of` rows are the pair C3 §3 is about: same kind, same
    // strength, same shape of sentence. What tells them apart is the titles,
    // and at this width both are readable in full.
    const second = listed[1]
    if (!second) throw new Error('no second row')
    expect(cut(second, '.subject')).toBe(false)
    expect(second.querySelector('.subject')?.textContent).toBe(
      'Go-Toubun no Hanayome - Full Color Edition',
    )

    // And a title past the column's width is cut to fit rather than widening
    // the list under it.
    const long = listed[2]
    if (!long) throw new Error('no long-titled row')
    ellipsisedInsideItsColumn(long, '.subject')
    expect(
      host.querySelector('.proposal-list')?.scrollWidth ?? 0,
    ).toBeLessThanOrEqual(host.querySelector('main')?.clientWidth ?? 0)
  })

  it('reflows at 460px rather than cutting the title to ten characters', async () => {
    const host = await inboxAt(460)
    const first = rows(host)[0]
    if (!first) throw new Error('no first row')

    // **The measurement the review was about.** The row drew the `reason`
    // sentence in a flex column that measured 100px here — ten characters,
    // `"Monster - `, identical on every `edition-of` row in the inbox. The
    // container query gives the titles the whole row instead: measured
    // 326.6px once the web fonts have loaded — at 460px on this branch, as
    // at 700px before it — and the subject and its target both fit.
    expect(boxOf(first, '.what')).toBeGreaterThan(300)
    expect(cut(first, '.subject')).toBe(false)
    expect(cut(first, '.arrow')).toBe(false)
    expect(first.querySelector('.subject')?.textContent).toBe(
      'Monster - The Perfect Edition - Colored',
    )
    expect(first.querySelector('.arrow')?.textContent).toBe(
      '↳ an edition of Monster - The Perfect Edition',
    )

    // The reflow is a *reflow*: the kind and the strength pill keep the first
    // line, so the two countable fields still read straight down the list.
    const kindBox = first.querySelector('.kind')?.getBoundingClientRect()
    const whatBox = first.querySelector('.what')?.getBoundingClientRect()
    if (!kindBox || !whatBox) throw new Error('the row is missing a column')
    expect(whatBox.top).toBeGreaterThan(kindBox.top)

    // A title the reflowed row still cannot hold is cut inside it, both
    // lines, rather than pushing the list wider than `main`.
    const long = rows(host)[2]
    if (!long) throw new Error('no long-titled row')
    ellipsisedInsideItsColumn(long, '.subject')
    ellipsisedInsideItsColumn(long, '.arrow')
    expect(
      host.querySelector('.proposal-list')?.scrollWidth ?? 0,
    ).toBeLessThanOrEqual(host.querySelector('main')?.clientWidth ?? 0)
  })

  it('leaves the arrow line off a proposal that names one series', async () => {
    // `attach-chapters`. A row that drew it anyway reads `↳ related to null`,
    // and jsdom cannot tell that from a row one line shorter.
    const host = await inboxAt(460)
    const chapters = rows(host)[3]
    if (!chapters) throw new Error('no attach-chapters row')

    expect(chapters.querySelector('.arrow')).toBeNull()
    expect(chapters.querySelector('.subject')?.textContent).toBe(
      'Kaguya-sama - Fan Translated',
    )
    const withTarget = rows(host)[0]?.getBoundingClientRect().height ?? 0
    expect(chapters.getBoundingClientRect().height).toBeLessThan(withTarget)
  })
})

import { act } from 'react'
import { beforeEach, describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
// **Required**, the same reason `Filmstrip.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app
// through `index.css`. Without this the boxes below are unstyled and the
// assertions measure browser defaults.
import './index.css'
import { PreviewTabs } from './PreviewTabs'
import type { Preview, Row, Rule } from './rules'
import { mount } from './testing/mount'

/**
 * What jsdom cannot answer: whether the table scrolls instead of the page,
 * and whether the pinned column is actually opaque. Every assertion here is
 * a measured box or a computed style.
 */

/**
 * Real paths, because the widths under test are their widths. `FIELDS` in
 * `PreviewTabs.tsx` is a seven-element constant, so Provenance is 8 columns
 * whatever the data holds; what the rows have to supply is `rel_path`
 * strings as long as a real library's.
 *
 * **This fixture's own before-state**, measured against the real 390 x 844
 * viewport the `beforeEach` below sets (`page.viewport()`, not
 * `documentElement.style.width`, which never touches `clientWidth`):
 * `document.documentElement.clientWidth` measured 390 — no scrollbar
 * deducted from it in this run — and with both `.table-scroll` wrappers
 * temporarily removed from `PreviewTabs.tsx`, `document.documentElement.
 * scrollWidth` reached 1409px on Matched files and 1581px on Provenance:
 * 1019px and 1191px of real document overflow, restored afterwards.
 *
 * **The app's own numbers, against a real library, are different — and are
 * the app's, not this fixture's.** `admin.css`'s own comment on
 * `.table-scroll` records `.provenance` rendering 524px and `.matched`
 * 393px there against a 375px client width, 165px and 34px of overflow.
 * Two different pages, two different measurements; neither stands in for
 * the other.
 *
 * **Longer than the shortest strings that name this scenario**, and with an
 * underscored release-group filename rather than a spaced one, on purpose:
 * a `rel_path` broken only at spaces wraps onto several lines rather than
 * forcing the table wider, and the first attempt at this fixture, using
 * shorter space-broken names, produced zero document overflow with no CSS
 * at all — a green fixture that proved nothing, the exact trap this file's
 * own review called out.
 */
const PATHS = [
  'Death Note (Colored Council)/Death_Note_-_v01_(Colored_Council)_[SomeVeryLongScanlationGroupNameThatKeepsGoingAndGoing].cbz',
  'Death Note (Colored Council)/Death_Note_-_v02_(Colored_Council)_[SomeVeryLongScanlationGroupNameThatKeepsGoingAndGoing].cbz',
  'Jigokuraku - Side Story - Forest of Misfortune/Jigokuraku_Side_Story_Forest_of_Misfortune_(2023)_[SomeVeryLongScanlationGroupNameThatKeepsGoingAndGoing].cbz',
]

const rows: Row[] = PATHS.map((rel_path, i) => ({
  rel_path,
  root_id: 1,
  before: { series: 'Death Note', work: `w${i}`, edition: 'en' },
  after: { series: 'Death Note', work: `w${i}`, edition: 'en' },
  matched: i !== 2,
  decided_by: {
    series: { kind: 'rule', index: 0 },
    volume: { kind: 'rule', index: 1 },
  },
}))

const fixture = {
  preview: {
    impact: {
      series_before: 134,
      series_after: 134,
      series_merged: 0,
      works_rehomed: 0,
      works_orphaned: 0,
      proposals_reopened: 0,
      proposals_raised: 0,
    },
    rows,
    rows_truncated: false,
    overrides: [],
  } satisfies Preview,
  rules: [] as Rule[],
  selected: 0,
  busy: false,
  frozen: false,
  onReorder: () => {},
  onSelectRule: () => {},
}

const settle = () =>
  new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)))

/**
 * `act()`-wrapped because a bare `button.click()` fires a state update
 * outside it, which `browser-setup.ts` exists to make React report. Discrete
 * events flush synchronously regardless, so the bare form worked — but it is
 * the one call site in this file that did not match the repo's own
 * convention (`Shell.browser.test.tsx`), and the gap was 14 act warnings on
 * a single run of this file.
 */
async function open(host: HTMLElement, label: string) {
  const button = [
    ...host.querySelectorAll<HTMLButtonElement>('.preview-tabs button'),
  ].find((b) => b.textContent?.startsWith(label))
  if (!button) throw new Error(`no tab named ${label}`)
  await act(async () => {
    button.click()
  })
}

// A real viewport, not a `documentElement.style.width` that never touches
// `clientWidth` (per CSSOM, the root element's `clientWidth` is the
// viewport's). 390 x 844 is a phone's; `PATHS`' own comment has the
// measured client width this actually produces.
beforeEach(async () => {
  await page.viewport(390, 844)
})

describe('the preview at phone width', () => {
  it('never moves the page sideways, on any of the four tabs', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    // **All four, not just the two with tables.** Only one tab's content is
    // ever in the DOM, so a single-tab assertion passes over whichever table
    // it did not open — and before this branch both of them overflowed. (The
    // sizes are in `PATHS`' comment above, kept apart there because this
    // fixture's numbers and the real app's are different measurements.)
    for (const tab of ['Impact', 'Matched files', 'Provenance', 'Overrides']) {
      await open(host, tab)
      await settle()
      const doc = document.documentElement
      expect(doc.scrollWidth, tab).toBeLessThanOrEqual(doc.clientWidth)
    }
  })

  it('scrolls the eight-column table inside its own box instead', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    // Provenance is file plus one column per metadata field, eight in all.
    // Matched files scrolls too — see the case below; an earlier version of
    // this comment claimed it fitted a phone, which was true of this
    // fixture's ten-character series names and never of the real library's.
    {
      const tab = 'Provenance'
      await open(host, tab)
      await settle()
      const box = host.querySelector('.table-scroll') as HTMLElement
      expect(box, tab).not.toBeNull()
      expect(box.scrollWidth, tab).toBeGreaterThan(box.clientWidth)
      // **Not just wider content than box — an actual scroll container.**
      // `scrollWidth > clientWidth` alone survives `overflow-x: visible`
      // unchanged: `max-width: 100%` still caps the box's own layout width
      // regardless of `overflow-x`, so the box stays narrower than its
      // content either way, and that comparison cannot tell "clipped and
      // scrollable" from "not clipped, painted over the page instead" —
      // which is exactly the failure `overflow-x: visible` is. A non-scroll
      // container ignores a `scrollLeft` write and stays at 0; a real one
      // moves. Reproduced by hand: with `overflow-x: visible` this line was
      // the only one of the file's tests that still passed, because
      // nothing had actually scrolled for the assertions below it to
      // disagree about.
      box.scrollLeft = box.scrollWidth - box.clientWidth
      expect(box.scrollLeft, tab).toBeGreaterThan(0)
    }
  })

  it('scrolls the four-column table in its own box too', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    await open(host, 'Matched files')
    await settle()
    const box = host.querySelector('.table-scroll') as HTMLElement
    expect(box).not.toBeNull()
    // **This asserted the opposite until the browser pass, and the opposite
    // was only ever true of this fixture.** It read "fits a phone outright,
    // so the reader never has to scroll sideways to read a row" — true here
    // because `before`/`after` hold `'Death Note'`, ten characters. Measured
    // against the library these rules were built on, where series titles run
    // to 77 and both those columns keep `nowrap`, Matched files is **971px
    // in a 343px box before any floor is applied at all**. It has never fit.
    // The scroll box is what it needs, and what it has.
    expect(box.scrollWidth).toBeGreaterThan(box.clientWidth)
    box.scrollLeft = box.scrollWidth - box.clientWidth
    expect(box.scrollLeft).toBeGreaterThan(0)
  })

  it('keeps the file column on screen, and opaque, through the scroll', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    await open(host, 'Provenance')
    await settle()
    const box = host.querySelector('.table-scroll') as HTMLElement
    const firstCell = box.querySelector('tbody td:first-child') as HTMLElement
    const before = firstCell.getBoundingClientRect().left
    box.scrollLeft = box.scrollWidth - box.clientWidth
    // Without this the comparison below is vacuous: a box that ignored the
    // write has not moved, so of course the cell has not either.
    expect(box.scrollLeft).toBeGreaterThan(0)
    await settle()
    expect(firstCell.getBoundingClientRect().left).toBeCloseTo(before, 0)
    // Position alone is not enough: a transparent sticky cell sits in
    // exactly the right place while the columns it covers read straight
    // through it.
    const background = getComputedStyle(firstCell).backgroundColor
    expect(background).not.toBe('rgba(0, 0, 0, 0)')
    expect(background).not.toBe('transparent')
  })

  it('shows the whole path in the pinned cell rather than cutting its end', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    await open(host, 'Provenance')
    await settle()
    const box = host.querySelector('.table-scroll') as HTMLElement
    expect(box).not.toBeNull()
    const cells = [...box.querySelectorAll<HTMLElement>('tbody td:first-child')]
    // The cap that makes `position: sticky` work in Chromium would, if it
    // clipped, hide the end of the path — which is the part that tells these
    // rows apart. Nothing overflowing its own box is what says it wraps
    // instead: a clipped cell has scrollWidth past clientWidth, a wrapped
    // one does not.
    for (const cell of cells) {
      expect(cell.scrollWidth, cell.textContent ?? '').toBeLessThanOrEqual(
        cell.clientWidth + 1,
      )
    }
    // Wrapping is what makes the whole path visible, and a wrapped cell is
    // taller than an unwrapped one. Under `nowrap` + ellipsis every cell is
    // exactly one line high, so this is the assertion that can tell the two
    // apart — `textContent` cannot, being untouched by any of it.
    const lineHeight = Number.parseFloat(getComputedStyle(cells[0]).lineHeight)
    expect(cells.some((cell) => cell.clientHeight > lineHeight * 1.5)).toBe(
      true,
    )
  })
})

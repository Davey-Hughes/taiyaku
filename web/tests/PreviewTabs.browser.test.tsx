import { act } from 'react'
import { beforeEach, describe, expect, it } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Filmstrip.browser.test.tsx` gives: every
// rule under test lives in `styles/admin.css`, which reaches the app
// through `index.css`. Without this the boxes below are unstyled and the
// assertions measure browser defaults.
import '../src/index.css'
import { PreviewTabs } from '../src/PreviewTabs'
import type { Preview, Row, Rule } from '../src/rules'
import { mount } from './support/mount'

/**
 * What jsdom cannot answer: whether the table scrolls instead of the page,
 * and whether the pinned column is actually opaque. Every assertion here is
 * a measured box or a computed style.
 */

/**
 * Real paths, because the widths under test are their widths. Provenance draws
 * `file` plus a column per field some row decided (O149), so these rows decide
 * all seven to keep it the eight-column case; what else they have to supply is
 * `rel_path` strings as long as a real library's.
 *
 * **Longer than the shortest strings that name this scenario**, and with an
 * underscored release-group filename rather than a spaced one, on purpose: a
 * `rel_path` broken only at spaces wraps onto several lines rather than forcing
 * the table wider.
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
  // All seven, because Provenance draws a column only for a field some row
  // decided (O149), and this file's case is the widest table there is.
  decided_by: {
    series: { kind: 'rule', index: 0 },
    edition_key: { kind: 'root_default' },
    kind: { kind: 'root_default' },
    language: { kind: 'root_default' },
    volume: { kind: 'rule', index: 1 },
    chapter: { kind: 'rule', index: 1 },
    title: { kind: 'rule', index: 0 },
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
 * `act()`-wrapped because a bare `button.click()` fires a state update outside
 * it, which `browser-setup.ts` exists to make React report. Discrete events
 * flush synchronously regardless, but a bare click does not match the repo's
 * own convention (`Shell.browser.test.tsx`).
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
// viewport's). 390 x 844 is a phone's.
beforeEach(async () => {
  await page.viewport(390, 844)
})

describe('the preview at phone width', () => {
  it('never moves the page sideways, on any of the four tabs', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    // **All four, not just the two with tables.** Only one tab's content is
    // ever in the DOM, so a single-tab assertion passes over whichever table it
    // did not open.
    for (const tab of ['Impact', 'Matched files', 'Provenance', 'Overrides']) {
      await open(host, tab)
      await settle()
      const doc = document.documentElement
      expect(doc.scrollWidth, tab).toBeLessThanOrEqual(doc.clientWidth)
    }
  })

  it('scrolls the eight-column table inside its own box instead', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    // Provenance is file plus one column per metadata field some row decided —
    // eight here only because this fixture decides all seven. Matched files
    // scrolls too — see the case below.
    {
      const tab = 'Provenance'
      await open(host, tab)
      await settle()
      // The premise, pinned: an edit to the fixture that stops deciding a
      // field makes this a narrower table under the same name.
      expect(
        host.querySelectorAll('table.provenance thead th'),
        tab,
      ).toHaveLength(8)
      const box = host.querySelector('.table-scroll') as HTMLElement
      expect(box, tab).not.toBeNull()
      expect(box.scrollWidth, tab).toBeGreaterThan(box.clientWidth)
      // **Not just wider content than box — an actual scroll container.**
      // `scrollWidth > clientWidth` alone survives `overflow-x: visible`
      // unchanged: `max-width: 100%` still caps the box's own layout width
      // regardless of `overflow-x`, so the box stays narrower than its content
      // either way, and that comparison cannot tell "clipped and scrollable"
      // from "not clipped, painted over the page instead" — which is exactly
      // the failure `overflow-x: visible` is. A non-scroll container ignores a
      // `scrollLeft` write and stays at 0; a real one moves.
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
    // Both the `before` and `after` columns keep `nowrap`, so with a real
    // library's series titles this table does not fit a phone. The scroll box
    // is what it needs, and what it has.
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
    // A pinned cell that clipped would hide the end of the path — which is
    // the part that tells these rows apart. Nothing overflowing its own box
    // is what says it wraps instead: a clipped cell has `scrollWidth` past
    // `clientWidth`, a wrapped one does not.
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

describe('the tabs, by keyboard and by name — O156', () => {
  it('walks the tabs with real arrow keys, and Tab leaves the tablist', async () => {
    const host = mount(<PreviewTabs {...fixture} />)
    const tabs = () => [...host.querySelectorAll<HTMLElement>('[role="tab"]')]
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => {
      tabs()[0].focus()
    })
    const press = async (key: string) => {
      await act(async () => {
        await userEvent.keyboard(key)
      })
      return document.activeElement?.textContent
    }
    expect(await press('{ArrowRight}')).toBe('Matched files')
    expect(await press('{End}')).toBe('Overrides')
    expect(await press('{ArrowRight}')).toBe('Impact')
    expect(await press('{ArrowLeft}')).toBe('Overrides')
    expect(await press('{Home}')).toBe('Impact')
    // One stop for the whole list: Tab goes past the other tabs.
    await act(async () => {
      await userEvent.tab()
    })
    expect(tabs()).not.toContain(document.activeElement)
  })

  it('names the list, the selected tab and the panel it shows', async () => {
    mount(<PreviewTabs {...fixture} />)
    await expect
      .element(page.getByRole('tablist', { name: 'Preview', exact: true }))
      .toBeInTheDocument()
    await expect
      .element(page.getByRole('tab', { selected: true }))
      .toHaveAccessibleName('Impact')
    await expect
      .element(page.getByRole('tabpanel', { name: 'Impact', exact: true }))
      .toBeInTheDocument()
  })
})

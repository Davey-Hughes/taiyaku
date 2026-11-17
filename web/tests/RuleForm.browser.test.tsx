import { describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
// **Required**, the same reason `PreviewTabs.browser.test.tsx` gives: the box
// width under test is `admin.css`'s, which reaches the app through
// `index.css`. Without this the assertion below measures browser defaults.
import '../src/index.css'
import type { Root } from '../src/config'
import { RuleForm } from '../src/RuleForm'
import type { Rule } from '../src/rules'
import { mount } from './support/mount'

/**
 * What jsdom cannot answer: it returns zeroes from
 * `getBoundingClientRect`, so a pattern field sharing a third of the row
 * passes a jsdom assertion just as happily as one on a row of its own. Only
 * a measured box tells them apart, which is why this file exists.
 *
 * A bare `.field.pattern` declaring `flex` would tie `.rule-form .field` at
 * (0,2,0), and source order would give the win to `.rule-form .field`: the
 * pattern box would be a third of the row instead of the whole.
 * `.rule-form .field.pattern` is raised above the tie.
 *
 * **Selectors, not line numbers.** A line cite into `admin.css` goes stale as
 * the file changes, so the rules are named.
 */

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

const roots: Root[] = [
  {
    id: 1,
    // **A real root path, because that is what sizes the `scope` select.** A
    // `<select>`'s intrinsic width is its widest option, so this string is
    // the fixture's whole point: `/srv/manga`, ten characters, fits a phone,
    // and the 35 characters below do not.
    path: '/home/davey/temp/manga-test/Volumes',
    edition_key: 'en',
    default_kind: 'manga',
    default_language: 'en',
    exclude_globs: null,
    scan_interval_secs: 86400,
    keeps_documents: false,
    last_scan_at: null,
    last_scan_status: null,
    next_scan_at: null,
    paused_at: null,
  },
]

describe('the rule form, in a browser', () => {
  it('gives the pattern field a box as wide as the form, not a third of it', async () => {
    const host = mount(
      <RuleForm rule={rule} roots={roots} onCommit={() => {}} />,
    )
    const form = host.querySelector('.rule-form')
    const pattern = host.querySelector('.field.pattern')
    if (!form || !pattern) throw new Error('the rule form did not render')
    const formWidth = form.getBoundingClientRect().width
    const patternWidth = pattern.getBoundingClientRect().width
    expect(patternWidth).toBeGreaterThan(formWidth * 0.9)
  })

  it('keeps the sets fieldset inside the form rather than 26px past it', async () => {
    const host = mount(
      <RuleForm rule={rule} roots={roots} onCommit={() => {}} />,
    )
    // The pane's width, because that is what the form has to fit inside:
    // `mount()` appends to `document.body`, where nothing bounds the form.
    // 777px stands in for that pane's width at a 1440px window.
    host.style.width = '777px'
    await new Promise((r) => requestAnimationFrame(() => r(undefined)))
    const form = host.querySelector('.rule-form')
    const sets = host.querySelector('.rule-sets')
    if (!form || !sets) throw new Error('the rule form did not render')

    // **`flex-basis` sizes the content box, and this project sets no global
    // `border-box` reset.** `flex: 1 0 100%` asks for the form's full width as
    // content, so without `box-sizing: border-box` the fieldset's own
    // `var(--space-3)` padding and 1px border would sit on top of it, past the
    // form's width.
    //
    // **Asserted on this element rather than on anything's `scrollWidth`.**
    // The document reads no overflow at any width, because the element that
    // scrolls is `main`, above boxes that are all `overflow-x: visible`. A
    // blanket "nothing spills" assertion fails on the bare fixture for
    // reasons the real screen does not have.
    expect(sets.getBoundingClientRect().width).toBeLessThanOrEqual(
      form.getBoundingClientRect().width,
    )
  })

  it('shrinks the scope select to its label at phone width', async () => {
    await page.viewport(390, 844)
    const host = mount(
      <RuleForm rule={rule} roots={roots} onCommit={() => {}} />,
    )
    host.style.width = '343px'
    await new Promise((r) => requestAnimationFrame(() => r(undefined)))
    const scope = host.querySelector<HTMLSelectElement>(
      'select[name="root_id"]',
    )
    const label = scope?.closest('.field')
    if (!scope || !label) throw new Error('the scope field did not render')
    // A grid item will not go below its content's min-content width without
    // `min-width: 0`, and a select's min-content is its widest option — the
    // root path above. Without that declaration this select would be wider
    // than its label, and the overflow would reach the document.
    expect(scope.getBoundingClientRect().width).toBeLessThanOrEqual(
      label.getBoundingClientRect().width + 1,
    )
  })
})

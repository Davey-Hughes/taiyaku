import { describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
// **Required**, the same reason `PreviewTabs.browser.test.tsx` gives: the box
// width under test is `admin.css`'s, which reaches the app through
// `index.css`. Without this the assertion below measures browser defaults.
import './index.css'
import type { Root } from './config'
import { RuleForm } from './RuleForm'
import type { Rule } from './rules'
import { mount } from './testing/mount'

/**
 * What jsdom cannot answer: it returns zeroes from
 * `getBoundingClientRect`, so a pattern field sharing a third of the row
 * passes a jsdom assertion just as happily as one on a row of its own. Only
 * a measured box tells them apart, which is why this file exists.
 *
 * The whole-branch review's C1. `.field.pattern` and `.rule-form .field`
 * both declare `flex`, they tied at (0,2,0), and source order gave it to
 * `.rule-form .field` — so the pattern box rendered 342px of the form's
 * 1051 at 1440px instead of the whole row. The fix raised
 * `.rule-form .field.pattern` above the tie.
 *
 * **Selectors, not line numbers.** The first version of this comment cited
 * `admin.css:505` and `:884`, which pointed at a prose comment and at
 * `.rule-row .select`'s body by the time the commit landed — and quoted a
 * selector that same commit had deleted. Every other comment in this tree
 * names the rule instead, for exactly that reason.
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
    // the fixture's whole point. It was `/srv/manga` — ten characters, which
    // fits a phone — and the browser pass then found the select rendering
    // 299px inside its 166px label against the library these rules were
    // built on, whose root is the 35 characters below, putting 118px of
    // sideways scroll on the document at 390.
    path: '/home/davey/temp/manga-test/Volumes',
    edition_key: 'en',
    default_kind: 'manga',
    default_language: 'en',
    exclude_globs: null,
    scan_interval_secs: 86400,
    keeps_documents: false,
    last_scan_at: null,
    last_scan_status: null,
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
    // Measured by the controller at 1440px on `/config/rules`: 342px of the
    // form's 1051px, a third of the row rather than the whole of it.
    expect(patternWidth).toBeGreaterThan(formWidth * 0.9)
  })

  it('keeps the sets fieldset inside the form rather than 26px past it', async () => {
    const host = mount(
      <RuleForm rule={rule} roots={roots} onCommit={() => {}} />,
    )
    // The pane's width, because that is what the form has to fit inside.
    // `mount()` appends to `document.body`, where the form is bounded by
    // nothing. 777px is what `.editor-right` measured at a 1440px window
    // when this was found.
    host.style.width = '777px'
    await new Promise((r) => requestAnimationFrame(() => r(undefined)))
    const form = host.querySelector('.rule-form')
    const sets = host.querySelector('.rule-sets')
    if (!form || !sets) throw new Error('the rule form did not render')

    // **`flex-basis` sizes the content box, and this project sets no global
    // `border-box` reset.** `flex: 1 0 100%` asked for the form's full width
    // as content; the fieldset's own `var(--space-3)` padding and 1px border
    // then sat on top, so it rendered 803px inside a 777px form — 12+12 and
    // 1+1 exactly. `box-sizing: border-box` is what holds it.
    //
    // **Asserted on this element rather than on anything's `scrollWidth`.**
    // The document's reads no overflow at any width, because the element
    // that grew a scrollbar for those 26px was `main`, four ancestors up
    // through boxes that are all `overflow-x: visible`. A blanket
    // "nothing spills" assertion was tried here and fails on the bare
    // fixture for reasons the real screen does not have.
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
    // root path above. Without that declaration this select rendered 299px
    // inside a 166px label, and the overflow reached the document.
    expect(scope.getBoundingClientRect().width).toBeLessThanOrEqual(
      label.getBoundingClientRect().width + 1,
    )
  })
})

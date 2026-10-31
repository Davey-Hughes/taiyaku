import { describe, expect, it } from 'vitest'
import adminSource from '../../src/styles/admin.css?raw'
import settingsSource from '../../src/styles/settings.css?raw'
import { rules, selectors, withoutComments } from '../support/css'

/**
 * The `/account` page read from its stylesheet. Only facts that are *defects*
 * if they regress live here, the rule `admin-rules.css.test.ts` and
 * `admin-proposals.css.test.ts` state for their own screens; how the screen
 * looks is the browser pass's job (spec 2026-09-17 §5 item 2).
 *
 * **In the `node` project, which is where every stylesheet-text assertion in
 * this repository lives**, for a mechanical reason rather than a tidy one.
 * Vitest's default `css: false` stubs a `?raw` CSS import to the empty string;
 * only the `node` project carries the `css.include` line that turns that off.
 */

/** Every rule in `admin.css` whose selector list satisfies `wanted`. */
function rulesWithSelector(
  wanted: (selector: string) => boolean,
): Array<{ prelude: string; body: string }> {
  // `selectors()` rather than `split(',')`: the shared block's lists are
  // plain today, but the coarse companion sits beside rules that use
  // `:where(a, b)`, where a naive split cuts a selector in half.
  return rules(withoutComments(adminSource)).filter(({ prelude }) =>
    selectors(prelude).some(wanted),
  )
}

/** The `.account` root as a class of its own, not `.account-page`'s prefix. */
const ACCOUNT = /\.account(?![\w-])/

/**
 * **Not vacuous, stated first.** Every count below is a filter over this
 * text, and an empty stylesheet makes each of them zero — the state this
 * file lives in the `node` project to avoid. A pin on the text itself is
 * what separates "the rule is gone" from "the file never arrived".
 */
it('reads admin.css at all', () => {
  // `'.account .secret {'` and not `'.account .secret'`: the brace makes it
  // a rule, and one only this page draws with.
  expect(adminSource).toContain('.account .secret {')
})

/**
 * **The page's boxes are settings rows' controls**, so `settings.css` sizes
 * them, with its coarse-pointer floor beside the size it lifts. An entry
 * for this page left in the shared `.field` block's coupled lists — the
 * reset, the 16px floor, the 44px floor — would be a second size rule for
 * a page box at another specificity, the shape that produced O151, and an
 * entry left in one list and not the other is O152's. So none is left,
 * neither the labels' entries nor the key box's.
 */
describe('the account page leaves the shared field block whole', () => {
  it('names the page’s labels in none of the field block’s lists', () => {
    expect(
      rulesWithSelector((s) => ACCOUNT.test(s) && /\.field(?![\w-])/.test(s)),
    ).toEqual([])
    // Nor a rule of the page's own beneath the field block: two rules at
    // different specificities setting a size on the same box is O151's shape.
    expect(withoutComments(adminSource)).not.toMatch(/^\.account input\b/m)
  })

  it('names the key box in none of the control lists', () => {
    expect(rulesWithSelector((s) => /\.account-row(?![\w-])/.test(s))).toEqual(
      [],
    )
  })
})

describe('the account page’s h1 — O163', () => {
  it('stays at the h2 size it was drawn at before it was an h1', () => {
    // The title is `PageHeader`'s, and `settings.css` holds it at
    // `--text-h2`, where `base.css` sizes an `h1` at `--text-h1` inside
    // `.library`. No rule of this page's sizes it as well.
    const bodies = rules(settingsSource)
      .filter(({ prelude }) =>
        selectors(prelude).includes('.page-header .page-title'),
      )
      .map(({ body }) => body)
    expect(bodies).toHaveLength(1)
    expect(bodies[0]).toMatch(/(?:^|;)\s*font-size:\s*var\(--text-h2\)/)
    expect(rulesWithSelector((s) => /\.account\s*>\s*h1\b/.test(s))).toEqual([])
  })
})

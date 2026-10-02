import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import { rules, selectors, withoutComments } from '../testing/css'

/**
 * The `/account` page read from its stylesheet — ⑤c Task 6, and the same rule
 * `admin-rules.css.test.ts` and `admin-proposals.css.test.ts` state for their
 * own screens: only facts that are *defects* if they regress live here, and
 * how the screen looks is the browser pass's job (spec 2026-09-17 §5 item 2).
 *
 * **In the `node` project, which is where every stylesheet-text assertion in
 * this repository lives**, for a mechanical reason rather than a tidy one.
 * Vitest's default `css: false` stubs a `?raw` CSS import to the empty
 * string, and only the `node` project carries the `css.include` line that
 * turns that off. These assertions were written inside `Account.test.tsx`
 * first, where `admin.css?raw` arrives as `''`: probed under `jsdom` on
 * 2026-09-24, that import measured **length 0** there against 13384
 * characters for the `./Account.tsx?raw` read beside it — so a `.tsx` source
 * read is unaffected by the stub and stays in that file, while everything
 * below moved here rather than being propped up by widening a shared config
 * for one test.
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

/**
 * **Not vacuous, stated first.** Every count below is a filter over this
 * text, and an empty stylesheet makes each of them zero — the state this
 * file lives in the `node` project to avoid. A pin on the text itself is
 * what separates "the rule is gone" from "the file never arrived".
 */
it('reads admin.css at all', () => {
  // `'.account-card {'` and not `'.account-card'`: the bare name is also a
  // prefix of every other class that starts with it, so a renamed rule left
  // this green. The brace makes it a rule.
  expect(adminSource).toContain('.account-card {')
})

/**
 * **The 16 px floor's coupled lists agree** — O152's shape. `base.css.test.ts`
 * pairs an exposed rule with its coarse companion by *exact selector text*,
 * so a control subject added to the reset and not to the floor beneath it is
 * a size with no companion. That much `base.css.test.ts` does catch, proven
 * by hand on 2026-09-24: removing `.account .account-row input` from the coarse list
 * alone turns it red naming that selector. What it cannot see is a subject
 * missing from the *reset* — there is then no exposed rule for it at all —
 * which is why the membership is counted here too.
 *
 * **Three lists for a labelled field, two for a bare control.** `.account
 * .field` is the `<label>`, and it takes the block's base rule; as `.account
 * .field input` it takes the control reset and the coarse floor under it.
 * The key-name box has no label to wrap it — only a placeholder, and whether
 * it should have a real one is the accessibility pass's question, not this
 * file's — so `.account .account-row input` belongs to the two control lists and to
 * neither the label rule nor anything else.
 *
 * It was measured into those two rather than reasoned into them. Before
 * them the box drew **166px wide at 16px with `1px 2px` of padding**, beside
 * password boxes at 14px and `4px 8px`: `base.css`'s bare-control defaults,
 * because ⑤c's `.account .field input` cannot reach an input that no
 * `.field` wraps. `Account.browser.test.tsx` measures what those lists are
 * worth on screen; this file is what stops one of them being edited without
 * the other.
 */
describe('the account page joins the shared field block whole', () => {
  it('puts the field label in all three coupled lists and nowhere else', () => {
    const found = rulesWithSelector((s) => s.startsWith('.account .field'))
    expect(found).toHaveLength(3)
    // The pair ⑤c deleted rather than left beneath the new one: two rules at
    // different specificities both setting a size on the same element is the
    // shape that produced O151.
    expect(withoutComments(adminSource)).not.toMatch(/^\.account input\b/m)
  })

  it('puts the unlabelled key-name box in both of the control lists', () => {
    // The two coupled lists are the rules carrying *both* subjects. An
    // intersection rather than "two rules somewhere": a second standalone
    // copy of the reset, or the box named twice under the floor, would
    // satisfy a bare count and neither would pair.
    const coupled = rulesWithSelector(
      (s) => s === '.account .account-row input',
    ).filter(({ prelude }) =>
      selectors(prelude).some((s) => s.startsWith('.account .field ')),
    )
    expect(coupled).toHaveLength(2)
    // One of them is the floor and the other is not — the pairing itself.
    expect(
      coupled.map(({ body }) =>
        /font-size:\s*max\(\s*16px/.test(body) ? 'floor' : 'reset',
      ),
    ).toEqual(['reset', 'floor'])
  })

  it('gives that box the row’s remaining width, which no shared list can', () => {
    // `.account-row` is a flex row shared with the Create button, so the
    // reset's `min-width: 0` leaves the box at its intrinsic width and the
    // rest of the row a hole — 399px of one, measured. The fill is a rule of
    // its own because the shared lists reach five other screens, none of
    // which puts a control in a flex row beside a button.
    const fill = rulesWithSelector(
      (s) => s === '.account .account-row input',
    ).filter(({ body }) => /(?:^|;)\s*flex:\s*1\s*;/.test(body))
    expect(fill).toHaveLength(1)
    // And it is that rule alone: the shared reset must not grow a `flex`,
    // which would reach the Configuration screens' fields as well.
    expect(selectors(fill[0].prelude)).toEqual(['.account .account-row input'])
  })
})

describe('the account page’s h1 — O163', () => {
  it('stays at the h2 size it was drawn at before it was an h1', () => {
    // `base.css` sizes an `h1` at `--text-h1` inside `.library`; this rule
    // holds the page title where it was as an `h2`.
    const bodies = rules(adminSource)
      .filter(({ prelude }) => selectors(prelude).includes('.account > h1'))
      .map(({ body }) => body)
    expect(bodies).toHaveLength(1)
    expect(bodies[0]).toMatch(/(?:^|;)\s*font-size:\s*var\(--text-h2\)/)
  })
})

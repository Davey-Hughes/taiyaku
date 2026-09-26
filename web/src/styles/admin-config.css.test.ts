import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import { rules, selectors, withoutComments } from '../testing/css'

/**
 * ⑤b empties `admin.css` of the two pre-token idioms the style pass replaced
 * everywhere else: a border mixed from `currentColor`, and `opacity` standing
 * in for a quiet colour. Both are invisible to `colourLiterals.test.ts`,
 * which only fails on a literal — `color-mix(in srgb, currentColor 25%,
 * transparent)` names no colour at all, and `opacity: 0.8` names none either.
 *
 * Asserted here rather than left to review, because "the sweep is complete"
 * is a claim that decays the moment somebody adds a rule.
 */
describe('admin.css holds no pre-token idiom', () => {
  it('uses opacity for no quiet text', () => {
    // A disabled control is the one exemption the spec's §4 allows, and
    // `admin.css` has none today. If one is added, name it here rather than
    // widening the regex.
    const found = rules(adminSource)
      .filter(({ body }) => /(?:^|;)\s*opacity:/.test(body))
      .map(({ prelude }) => prelude)
    expect(found).toEqual([])
  })

  // The companion case: Task 6 replaced the last two `color-mix(in srgb,
  // currentColor …)` borders — `.remove-root` and `.remove-user` — with
  // `--danger`, closing the sweep Task 3 left these two out of.
  it('mixes no border from currentColor', () => {
    const found = [
      ...withoutComments(adminSource).matchAll(
        /color-mix\(in srgb, currentColor[^)]*\)/g,
      ),
    ].map((m) => m[0])
    expect(found).toEqual([])
  })
})

/**
 * I2 of the whole-branch review over ⑤b: nothing asserted that `--danger`
 * actually reaches the two removal blocks. Deleting `box-shadow: inset 3px 0
 * 0 var(--danger)` from `.remove-root, .remove-user`, or either
 * `var(--danger)` declaration from `.remove-root .confirm, .remove-user
 * .confirm`, left the whole suite green — it was the only one of ⑤b's five
 * decisions with no test. Static rather than measured, like the sibling
 * describe block below: this is cheaper than a browser assertion and the
 * token's own contrast is already covered by `index.css.test.ts`.
 */
describe('admin.css marks the two removal blocks with --danger', () => {
  it('puts the token on the strip and on the confirm button', () => {
    const strip = rules(adminSource).find(
      ({ prelude }) => prelude === '.remove-root, .remove-user',
    )
    expect(strip, '.remove-root, .remove-user must exist').toBeTruthy()
    expect(strip?.body).toMatch(
      /(?:^|;)\s*box-shadow:\s*inset 3px 0 0 var\(--danger\)/,
    )

    const confirm = rules(adminSource).find(
      ({ prelude }) =>
        prelude === '.remove-root .confirm, .remove-user .confirm',
    )
    expect(
      confirm,
      '.remove-root .confirm, .remove-user .confirm must exist',
    ).toBeTruthy()
    expect(confirm?.body).toMatch(/(?:^|;)\s*border-color:\s*var\(--danger\)/)
    expect(confirm?.body).toMatch(/;\s*color:\s*var\(--danger\)/)
  })
})

/**
 * Task 2's own rule: one `--config-measure` on the card, not a width on each
 * control inside it. Static rather than measured — the browser pass
 * (`Config.browser.test.tsx`) is what proves the measure actually reaches
 * 640px in a real layout; this is what stops a later edit from adding a
 * second, competing width beside it.
 */
describe('admin.css sets the settings-area measure once', () => {
  it('sets the measure once rather than per control', () => {
    const widths = rules(adminSource)
      .filter(({ prelude }) =>
        /\.(root-row|user-row|add-user|add-root)\b/.test(prelude),
      )
      .filter(({ body }) => /(?:^|;)\s*width:/.test(body))
      .map(({ prelude }) => prelude)
    // A `width` on a control inside these cards is the drift §11.4 refuses:
    // a figure per control is a figure to re-measure per review round.
    expect(widths).toEqual([])
  })
})

/**
 * O153: every `.why` on `/config/users` is one kind of sentence, an
 * explanation beneath the control it is about, so one rule quiets them all.
 * It read `.user-row .why` until 2026-09-25, which reached the role hint and
 * the two inside `.reset` and missed the add-account form's, the one `.why`
 * outside a card. Measured in Chromium on 2026-09-24: that one computed
 * 16px in `--text` beside two at 12px in `--muted`, louder than the form's
 * own labels.
 */
describe('admin.css quiets every .why on the users screen', () => {
  it('reaches .users .why rather than only the ones inside a card', () => {
    // Found by its body rather than its prelude, because the prelude is
    // what this case is about.
    const quiet = rules(adminSource).filter(
      ({ body }) =>
        /(?:^|;)\s*color:\s*var\(--muted\)/.test(body) &&
        /(?:^|;)\s*font-size:\s*var\(--text-meta\)/.test(body),
    )
    expect(
      quiet.filter(({ prelude }) => selectors(prelude).includes('.users .why')),
    ).toHaveLength(1)
    // And the narrow one is gone rather than left beside it, where it would
    // be a second rule for the same three paragraphs to drift apart in.
    expect(
      quiet.some(({ prelude }) =>
        selectors(prelude).includes('.user-row .why'),
      ),
    ).toBe(false)
  })
})

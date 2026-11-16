import { describe, expect, it } from 'vitest'
import adminSource from '../../src/styles/admin.css?raw'
import baseSource from '../../src/styles/base.css?raw'
import indexSource from '../../src/index.css?raw'
import { rules, selectors, subject, withoutComments } from '../support/css'

/**
 * `base.css` restyles elements only inside `.library` and `.login`, plus the
 * document-wide rules the page cannot work without. A bare `button` rule would
 * reach the reader, whose inline-styled chrome sets some properties and leaves
 * the rest to the browser, so a border or a padding set here would move the
 * reader's geometry (spec 2026-09-17 §5). A prelude can hold a comma-separated
 * selector list — `.a, .b { … }` — and each selector in it is scoped, or not,
 * independently of the others, so the list is split on its top-level commas
 * before the scope check runs.
 *
 * **It guards `base.css` and nothing else.** Element defaults live there by
 * the spec's §4 table, and a bare element rule written in another partial is
 * not this file's to find. And a comma inside an attribute value
 * (`[x="a,b"]`) is taken for a separator and split wrongly: the first half
 * keeps its `:where(.library, .login) ` prefix and passes, and only the
 * second (`b"]`) comes out as an unscoped fragment — so the check still
 * fails closed rather than letting a selector through.
 */
const SCOPE = ':where(.library, .login) '

/** Every prelude `rules()` finds in `css`, an at-rule's own excepted. */
function preludes(css: string): string[] {
  return rules(css)
    .map(({ prelude }) => prelude)
    .filter((prelude) => !prelude.startsWith('@'))
}

const DOCUMENT_WIDE = [
  ':root',
  'html, body, #root',
  'body',
  '::selection',
].flatMap(selectors)

/**
 * `.library` or `.login` itself, under a theme read from `data-theme` on the
 * root: Ink's weight rule (spec 2026-10-18 §3). It sets `font-weight` on the
 * scope element so that everything inside inherits it. Written under
 * `SCOPE`, it would set the weight on every element inside instead, at the
 * same zero specificity as the headings' 600, which it would override by
 * coming later, and as an author rule over every `<b>`'s and `<strong>`'s
 * bolder. So the guard allows the scope element and nothing else under a
 * theme: not the root alone, which the reader inherits from, and not an
 * element below it.
 */
const THEMED_SCOPE =
  /^:root(?:\[data-theme="[a-z]+"\]|:not\(\[data-theme="[a-z]+"\]\)) :where\(\.library, \.login\)$/

/**
 * Every selector this CSS text styles that is neither one of the
 * document-wide selectors nor scoped to `.library`/`.login`.
 */
function strays(css: string): string[] {
  return preludes(css)
    .flatMap(selectors)
    .filter(
      (selector) =>
        !DOCUMENT_WIDE.includes(selector) &&
        !selector.startsWith(SCOPE) &&
        !THEMED_SCOPE.test(selector),
    )
}

describe('base.css', () => {
  it('reads its rules at all', () => {
    expect(preludes(baseSource)).toContain('body')
  })

  it('styles no element outside the screens the style pass has reached', () => {
    expect(strays(baseSource)).toEqual([])
  })

  // The two rules tie at no specificity, so the later one wins; a ring rule
  // moved below this one would ring every landing point.
  it('draws no ring where focus only lands, in one rule after the ring it ties', () => {
    const all = rules(baseSource)
    const ring = all.findIndex(
      ({ prelude }) =>
        prelude === ':where(.library, .login) :where(:focus-visible)',
    )
    const none = all.filter(({ body }) =>
      /(?:^|;)\s*outline:\s*none\s*(?:;|$)/.test(body),
    )
    expect(none.map(({ prelude }) => prelude)).toEqual([
      ':where(.library, .login) :where( .page-title:not(.licence *), .missing, .gone-line, .removal-report, .dir-browser .where ):where(:focus)',
    ])
    expect(ring).toBeGreaterThanOrEqual(0)
    expect(all.indexOf(none[0])).toBeGreaterThan(ring)
  })

  it('catches a bare selector hiding in an otherwise-scoped selector list', () => {
    expect(
      strays(':where(.library, .login) a, button { color: red; }'),
    ).toEqual(['button'])
    expect(
      strays(':where(.library, .login) :where(h1, h2, h3) { margin: 0; }'),
    ).toEqual([])
  })

  it('lets a theme on the root qualify the scope element itself, and nothing else', () => {
    expect(
      strays(`
        :root[data-theme="ink"] :where(.library, .login) { font-weight: 500; }
        :root:not([data-theme="paper"]) :where(.library, .login) { font-weight: 500; }
      `),
    ).toEqual([])
    expect(
      strays(`
        :root[data-theme="ink"] { font-weight: 500; }
        :root[data-theme="ink"] button { font-weight: 500; }
        :root[data-theme="ink"] .reader { font-weight: 500; }
      `),
    ).toEqual([
      ':root[data-theme="ink"]',
      ':root[data-theme="ink"] button',
      ':root[data-theme="ink"] .reader',
    ])
  })
})

/** A subject that *is* one of the three controls, however qualified. */
const CONTROL_SUBJECT = /^(?:input|select|textarea)(?![\w-])/

/**
 * Every selector in `selector`'s place that styles exactly one control,
 * with its wrapper preserved.
 *
 * A wrapped subject — `.panel :where(input, select)` — is narrowed to one
 * control at a time *inside the same wrapper*, never unwrapped: `:where()`
 * contributes zero and `:is()` contributes its most specific argument, so
 * `:where(input)` and `:where(input, select)` have identical specificity and
 * the "two rules sharing one selector always tie" guarantee below survives
 * the expansion intact. Unwrapping to a bare `input` would not.
 *
 * **A list that is not all controls is dropped**, because `:is()` takes its
 * most specific argument: narrowing `:is(input, .foo)` from (0,1,0) to (0,0,1)
 * would silently change what it has to beat, and keeping it whole would make
 * `base.css`'s own `:where(button, input, select, textarea)` read as exposed
 * with no exact-text companion — red against code that is safe. **Dropped is
 * not the same as strict**: such a rule never enters `exposed` at all, so
 * nothing can go red for it. See the limit stated below.
 *
 * `:not()` and `:has()` are not expanded and must not be: `.foo:not(input)`
 * styles `.foo`, not an input.
 */
function expandControlSubjects(selector: string): string[] {
  const subj = subject(selector)
  const prefix = selector.slice(0, selector.length - subj.length)
  if (CONTROL_SUBJECT.test(subj)) return [selector]
  const wrapped = /^(:where|:is)\((.*)\)([^)]*)$/.exec(subj)
  if (!wrapped) return []
  const parts = selectors(wrapped[2])
  if (!parts.every((part) => CONTROL_SUBJECT.test(part))) return []
  return parts.map((part) => `${prefix}${wrapped[1]}(${part})${wrapped[3]}`)
}

/**
 * Every rule `css` holds that gives an `input`, `select` or `textarea` its
 * own `font-size`, paired with the selector that got it. Kept as pairs
 * rather than flattened, because the value a selector was given is exactly
 * what decides whether it needs a companion below.
 *
 * **Qualified subjects count**: comparing `subject(selector)` to the three
 * names for equality would miss `input[type="text"]`, `input:focus` and
 * `select:disabled` — and `.rule-form .field input[type="text"]` at (0,3,1)
 * beats both `base.css`'s floor and the companion below it.
 *
 * **What it still cannot see**, stated rather than papered over, because an
 * understated limit is worse than none — a reader trusts it and writes the
 * defect:
 *
 * - **An attribute-only subject**, `.field [type="text"]`, (0,1,1).
 * - **A wrapped list mixing controls with anything else.** `base.css`'s
 *   `:where(.library, .login) :where(button, input, select, textarea)
 *   { font: inherit }` is one, and carries the very shorthand this gate was
 *   widened to read. It is dropped rather than checked. That rule is safe
 *   today only because the separate floor beneath it — `:where(input,
 *   select, textarea)` inside `@media (pointer: coarse)`, a different
 *   selector text — sets `font-size` as a longhand and so wins over the
 *   shorthand; nothing here knows that, and a *new* mixed rule written
 *   without such a neighbour would pass unseen. O152's second half.
 * - **A second rule with a selector text that is already floored.** The pairing
 *   is by text across the whole concatenated cascade, not per rule, so once
 *   *any* rule with selector X has a companion, *every* rule with selector X
 *   counts as covered — including one written after that companion, which beats
 *   it on source order at equal specificity. The narrow guard below (`places
 *   admin.css's companion after the rule it corrects`) checks that ordering for
 *   one selector by hand; the general case needs each rule's position, which
 *   `rules()` does not return.
 *
 * `reader.css`'s `.reader-dialog-panel :where(input, select) { font: inherit }`
 * is the rule behind this gate reading the `font` shorthand and a wrapped
 * subject (O151): it sits under a panel at `--text-ui`, which draws the
 * reader's Settings dialog.
 */
function fontSizedControls(
  css: string,
): Array<{ selector: string; value: string }> {
  return rules(css).flatMap(({ prelude, body }) => {
    const sized = body.match(/(?:^|;)\s*font-size:\s*([^;]+)/)
    const shorthand = body.match(/(?:^|;)\s*font:\s*([^;]+)/)
    // `font-size` wins when a rule carries both: it comes after the
    // shorthand in every rule in this tree, and the shorthand's own size is
    // what it would override.
    const value = sized
      ? sized[1].trim()
      : shorthand
        ? `font: ${shorthand[1].trim()}`
        : null
    if (value === null) return []
    return selectors(prelude)
      .flatMap(expandControlSubjects)
      .map((selector) => ({ selector, value }))
  })
}

/**
 * Splits `css` into the text inside every top-level
 * `@media (pointer: coarse) { … }` block and everything outside one.
 * `rules()` already discards which `@media`, if any, an inner rule came
 * from — right for a scope check, wrong for a floor check, which needs
 * exactly that fact. Brace-counted by hand because the inner rule's own
 * `{`/`}` would end a naive non-greedy match at the wrong `}`.
 */
function splitByCoarsePointer(css: string): { coarse: string; rest: string } {
  const text = withoutComments(css)
  const open = /@media\s*\(pointer:\s*coarse\)\s*\{/g
  const coarse: string[] = []
  const rest: string[] = []
  let cursor = 0
  let match: RegExpExecArray | null
  // biome-ignore lint/suspicious/noAssignInExpressions: RegExp.exec's own idiom
  while ((match = open.exec(text))) {
    rest.push(text.slice(cursor, match.index))
    let depth = 1
    let i = open.lastIndex
    const start = i
    while (depth > 0 && i < text.length) {
      if (text[i] === '{') depth++
      else if (text[i] === '}') depth--
      i++
    }
    coarse.push(text.slice(start, i - 1))
    cursor = i
    open.lastIndex = i
  }
  rest.push(text.slice(cursor))
  return { coarse: coarse.join('\n'), rest: rest.join('\n') }
}

/**
 * Whether `value` is provably at least 16px without running a browser.
 * `:root`'s own `font: 16px / 1.5 …` above makes `rem` a fixed multiple, so a
 * literal `px` or `rem` can be resolved; a `var()`, `calc()`, `%` or `em`
 * cannot, and is treated the same as a literal under 16px. `max(16px, …)` is
 * the floor's own idiom and is trusted rather than parsed.
 */
function needsAFloor(value: string): boolean {
  // A `font` shorthand is never proven. Its size sits between optional
  // style/variant/weight/stretch keywords and a mandatory family, and `font:
  // inherit` resolves against an ancestor this gate has no model of (O151).
  // Unprovable is the same as under the line here.
  if (value.startsWith('font: ')) return !/max\(\s*16px/.test(value)
  if (/^max\(\s*16px/.test(value)) return false
  const rem = value.match(/^(-?\d*\.?\d+)rem$/)
  if (rem) return Number.parseFloat(rem[1]) * 16 < 16
  const px = value.match(/^(-?\d*\.?\d+)px$/)
  if (px) return Number.parseFloat(px[1]) < 16
  return true
}

/**
 * Every stylesheet `index.css` imports, concatenated in the order it
 * imports them — read from `index.css` itself rather than hand-kept, and in
 * that order rather than any other, because `index.css`'s own top comment
 * says a specificity tie is broken by exactly this order. `import.meta.glob`
 * is `colourLiterals.test.ts`'s own way of reading every file under `src/`
 * rather than a hand-kept list that goes stale the day a stylesheet is
 * added and left off it.
 */
const STYLESHEETS = import.meta.glob<string>('./*.css', {
  base: '../../src/styles',
  query: '?raw',
  import: 'default',
  eager: true,
})

const CASCADE_ORDER = [
  ...indexSource.matchAll(/@import\s+"\.\/styles\/([\w-]+\.css)"/g),
].map((m) => `./${m[1]}`)

describe('the 16px iOS zoom floor, against every stylesheet index.css imports', () => {
  /**
   * `base.css`'s floor above is written at `:where()`, zero specificity, so any
   * rule anywhere under `src/styles/` that names `input`, `select` or
   * `textarea` — even with a single class — outranks it regardless of whether
   * the media query matches: specificity, not the media query, decides the tie.
   *
   * This is not a scope check like `strays()` above — the defect is about
   * cascade position *between two rules*, not about one selector reaching
   * somewhere it should not — so it reads the whole cascade `index.css`
   * actually builds and pairs every font-size this cannot prove already
   * clears 16px with an identically-selectored rule inside a
   * `@media (pointer: coarse)` block. The same selector text is what makes
   * this reliable without computing specificity by hand: two rules sharing
   * one selector always tie, so a companion at the same text is provably at
   * the rule it has to beat, not merely at some higher specificity.
   */
  it('reads index.css’s own import order rather than assuming one', () => {
    // Pinned to the file as it stands, the way `colourLiterals.test.ts`
    // pins `SOURCES['./styles/tokens.css']` — proof the regex above matched
    // something real, not an empty array that would make every test below
    // it pass over nothing.
    expect(CASCADE_ORDER).toEqual([
      './base.css',
      './tokens.css',
      './annotations.css',
      './shell.css',
      './library.css',
      './admin.css',
      './settings.css',
      './reader.css',
      './book.css',
    ])
    for (const path of CASCADE_ORDER) {
      expect(STYLESHEETS[path], path).toBeTruthy()
    }
  })

  it('gives every input/select/textarea an unproven font-size a same-selector coarse-pointer companion', () => {
    const cascade = CASCADE_ORDER.map((path) => STYLESHEETS[path]).join('\n')
    const { coarse, rest } = splitByCoarsePointer(cascade)
    const exposed = fontSizedControls(rest).filter(({ value }) =>
      needsAFloor(value),
    )
    // Not vacuous: `.field input`/`.field select` are exposed today, and are
    // the reason this test exists.
    expect(exposed.length).toBeGreaterThan(0)
    // **Filtered by value, not just matched by selector.** Collecting every
    // coarse-block selector regardless of what it declares would let the
    // companion's own `max(16px, …)` be changed to a bare `var(--text-ui)` with
    // this test still green: the defect it exists to prevent, reached by
    // changing one token.
    const floored = new Set(
      fontSizedControls(coarse)
        .filter(({ value }) => !needsAFloor(value))
        .map((c) => c.selector),
    )
    const uncovered = exposed
      .filter(({ selector }) => !floored.has(selector))
      .map(({ selector }) => selector)
    expect(uncovered).toEqual([])
  })

  it('places admin.css’s companion after the rule it corrects, so an equal-specificity tie favours it', () => {
    // Equal specificity is not enough on its own — the *later* of two tied
    // rules wins, so a companion pasted above the rule it is meant to
    // override would still lose to it on every pointer.
    const fieldRule = adminSource.indexOf('.field input')
    const companion = adminSource.indexOf('@media (pointer: coarse)')
    expect(fieldRule).toBeGreaterThan(-1)
    expect(companion).toBeGreaterThan(fieldRule)
  })

  it('sees a control whose size comes from the font shorthand', () => {
    // `font: inherit` sets font-size, so the gate has to read the shorthand as
    // well as `font-size:`. This is half of O151: the reader's Settings panel
    // hands `--text-ui` to every control through exactly this declaration.
    const exposed = fontSizedControls('.panel input { font: inherit; }')
    expect(exposed).toEqual([
      { selector: '.panel input', value: 'font: inherit' },
    ])
    expect(needsAFloor('font: inherit')).toBe(true)
  })

  it('sees a subject wrapped in :where() or :is()', () => {
    // The other half of O151. `:where()` carries no specificity, which is no
    // defence at all: the floor is (0,0,0), so one class beats it.
    expect(
      fontSizedControls('.panel :where(input, select) { font-size: 14px; }'),
    ).toEqual([
      { selector: '.panel :where(input)', value: '14px' },
      { selector: '.panel :where(select)', value: '14px' },
    ])
  })

  it('pairs a companion that covers more controls than the rule it corrects', () => {
    // The reader panel's companion names `textarea` although the panel draws
    // none, so its selector text differs from the exposed rule by one name.
    // Expanding both is what makes the superset pair, which exact-text matching
    // would not do.
    const exposed = fontSizedControls(
      '.p :where(input, select) { font: inherit; }',
    )
    // `every()` on an empty array is vacuously true, and the whole claim
    // below is about what expansion produced — so pin the count first.
    expect(exposed).toHaveLength(2)
    const floored = new Set(
      fontSizedControls(
        '.p :where(input, select, textarea) { font-size: max(16px, 1rem); }',
      ).map((c) => c.selector),
    )
    expect(exposed.every(({ selector }) => floored.has(selector))).toBe(true)
  })

  it('does not expand a wrapped list that is not all controls', () => {
    // `:is(input, .foo)` takes its most specific argument's specificity, so
    // narrowing it to `:is(input)` would change what it beats. It is dropped
    // instead — which is what the assertion below says, and is NOT the same
    // as strict: a dropped rule never enters `exposed`, so nothing can go red
    // for it. The limit is stated in `fontSizedControls`'s own comment.
    expect(
      fontSizedControls('.p :is(input, .foo) { font-size: 14px; }'),
    ).toEqual([])
  })
})

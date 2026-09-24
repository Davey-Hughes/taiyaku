import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import baseSource from './base.css?raw'
import indexSource from '../index.css?raw'
import { rules, selectors, subject, withoutComments } from '../testing/css'

/**
 * `base.css` restyles elements only inside the screens the style pass has
 * reached — `.library` and `.login` — plus the document-wide rules the page
 * cannot work without. A bare `button` rule would reach the reader, whose
 * inline-styled chrome sets some properties and leaves the rest to the
 * browser, so a border or a padding set here would move geometry that the
 * pass's reader PR redraws on its own terms (spec 2026-09-17 §5). A prelude
 * can hold a comma-separated selector list — `.a, .b { … }` — and each
 * selector in it is scoped, or not, independently of the others, so the
 * list is split on its top-level commas before the scope check runs.
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
 * Every selector this CSS text styles that is neither one of the
 * document-wide selectors nor scoped to `.library`/`.login`.
 */
function strays(css: string): string[] {
  return preludes(css)
    .flatMap(selectors)
    .filter(
      (selector) =>
        !DOCUMENT_WIDE.includes(selector) && !selector.startsWith(SCOPE),
    )
}

describe('base.css', () => {
  it('reads its rules at all', () => {
    expect(preludes(baseSource)).toContain('body')
  })

  it('styles no element outside the screens the style pass has reached', () => {
    expect(strays(baseSource)).toEqual([])
  })

  it('catches a bare selector hiding in an otherwise-scoped selector list', () => {
    expect(
      strays(':where(.library, .login) a, button { color: red; }'),
    ).toEqual(['button'])
    expect(
      strays(':where(.library, .login) :where(h1, h2, h3) { margin: 0; }'),
    ).toEqual([])
  })
})

/** A subject that *is* one of the three controls, however qualified. */
const CONTROL_SUBJECT = /^(?:input|select|textarea)(?![\w-])/

/**
 * Every rule `css` holds that gives an `input`, `select` or `textarea` its
 * own `font-size`, paired with the selector that got it. Kept as pairs
 * rather than flattened, because the value a selector was given is exactly
 * what decides whether it needs a companion below.
 *
 * **Qualified subjects count**, and the first version of this did not see
 * them: it compared `subject(selector)` to the three names for equality, so
 * `input[type="text"]`, `input:focus` and `select:disabled` were all
 * invisible — and `.rule-form .field input[type="text"]` at (0,3,1) beats
 * both `base.css`'s floor and the companion below it. Found by the
 * re-review of the round that added this file.
 *
 * **What it still cannot see**, stated rather than papered over, because an
 * understated limit is worse than none — a reader trusts it and writes the
 * defect:
 *
 * - **The `font` shorthand.** The match is on `font-size:` alone, so
 *   `font: inherit` and `font: 0.8rem/1.4 …` both pass unseen.
 * - **A subject wrapped in `:where()`, `:is()`, `:not()` or `:has()`.**
 *   `:is()` and friends *do* carry specificity, so `.foo :is(input)` is
 *   (0,1,1) and beats the floor outright. And `:where()` needing ancestors
 *   is no defence: **the floor is (0,0,0), not merely low**, so one class is
 *   enough — and a bare `:where(input)` in any sheet imported after
 *   `base.css` ties at zero and wins on order.
 * - **An attribute-only subject**, `.field [type="text"]`, (0,1,1).
 *
 * Both of the first two are live in this tree, together, in one rule:
 * `reader.css`'s `.reader-dialog-panel :where(input, select) { font:
 * inherit }` under a panel at `--text-ui`, which is what draws the reader's
 * Settings dialog — its `type="number"` boxes render 14px on a coarse
 * pointer. That is C2's failure exactly, and this gate cannot see it twice
 * over. It is **not** this branch's to fix: the reader is outside
 * `.library`/`.login`, so `base.css`'s floor never reached it and the
 * exposure is older than ⑤a. It is filed as its own open item.
 */
function fontSizedControls(
  css: string,
): Array<{ selector: string; value: string }> {
  return rules(css).flatMap(({ prelude, body }) => {
    const declared = body.match(/(?:^|;)\s*font-size:\s*([^;]+)/)
    if (!declared) return []
    const value = declared[1].trim()
    return selectors(prelude)
      .filter((selector) => CONTROL_SUBJECT.test(subject(selector)))
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
 * cannot, and is treated the same as a literal that measured under the
 * line — the whole reason C2 held even though `--text-ui` (14px) is not the
 * `0.8rem` (12.8px) that shipped. `max(16px, …)` is the floor's own idiom
 * and is trusted rather than parsed.
 */
function needsAFloor(value: string): boolean {
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
  query: '?raw',
  import: 'default',
  eager: true,
})

const CASCADE_ORDER = [
  ...indexSource.matchAll(/@import\s+"\.\/styles\/([\w-]+\.css)"/g),
].map((m) => `./${m[1]}`)

describe('the 16px iOS zoom floor, against every stylesheet index.css imports', () => {
  /**
   * C2 of the whole-branch review. `base.css`'s floor above is written at
   * `:where()`, zero specificity, so any rule anywhere under `src/styles/`
   * that names `input`, `select` or `textarea` — even with a single
   * class — outranks it regardless of whether the media query matches.
   * `admin.css`'s `.field input`/`.field select` did exactly that, at a
   * literal `0.8rem` (12.8px), and stayed at 12.8px on a real coarse
   * pointer because specificity, not the media query, decided the tie.
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
      './reader.css',
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
    // **Filtered by value, not just matched by selector.** The first version
    // of this collected every coarse-block selector regardless of what it
    // declared, so changing the companion's own `max(16px, …)` back to a
    // bare `var(--text-ui)` put all nine controls under the floor again and
    // this test stayed green — the defect it exists to prevent, reachable in
    // one token. Found by the re-review of the round that added it.
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
})

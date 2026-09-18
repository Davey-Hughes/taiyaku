/**
 * Shared parsing for the tests that read a stylesheet's own text rather than
 * render it — `../index.css.test.ts`, `../styles/base.css.test.ts` and
 * `../theme.test.ts`. Comments are stripped and each rule is split into its
 * prelude and its body; a caller that needs individual selectors or what one
 * of them styles builds on `selectors()` and `subject()` below.
 */

/** Strips `/* ... *\/` comments, so one cannot hide a rule from `rules()`. */
export function withoutComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, '')
}

/**
 * Every rule in `css`: its prelude (whitespace collapsed to single spaces and
 * trimmed) and its body, the raw text between its braces. The prelude cannot
 * contain `;`, so a top-level `@import url(…);` is never swallowed into the
 * next rule's prelude; the body cannot contain `{` or `}`, so an at-rule that
 * wraps other rules — `@media`, `@supports` — never itself matches as one
 * rule, and only the rules nested inside it do. An at-rule with no nested
 * block, such as `@font-face`, matches like any other rule, prelude and all.
 */
export function rules(css: string): Array<{ prelude: string; body: string }> {
  return [...withoutComments(css).matchAll(/([^{};]+)\{([^{}]*)\}/g)].map(
    (m) => ({ prelude: m[1].replace(/\s+/g, ' ').trim(), body: m[2] }),
  )
}

/**
 * Splits a prelude's comma-separated selector list into its individual
 * selectors. A comma inside `:is(...)`, `:where(...)` or `:not(...)` opens a
 * nested selector list of its own and is not a top-level separator, so only
 * commas at nesting depth zero split the list. A comma inside an attribute
 * value (`[x="a,b"]`) is split wrongly — see each caller's own doc comment
 * for what that costs it there.
 */
export function selectors(prelude: string): string[] {
  const list: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < prelude.length; i++) {
    const char = prelude[i]
    if (char === '(') depth++
    else if (char === ')') depth--
    else if (char === ',' && depth === 0) {
      list.push(prelude.slice(start, i).trim())
      start = i + 1
    }
  }
  list.push(prelude.slice(start).trim())
  return list
}

/** A selector's last compound: what it styles, after its last combinator. */
export function subject(selector: string): string {
  let depth = 0
  let start = 0
  for (let i = 0; i < selector.length; i++) {
    const char = selector[i]
    if (char === '(') depth++
    else if (char === ')') depth--
    else if (depth === 0 && /[\s>+~]/.test(char)) start = i + 1
  }
  return selector.slice(start)
}

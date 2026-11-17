/**
 * Shared parsing for the tests that read a stylesheet's own text rather than
 * render it. Its callers are `../index.css.test.ts`, `../theme.test.ts`,
 * `../classNames.test.ts`, `../builtStylesheet.test.ts`, and `../styles/`'s
 * `base.css.test.ts`, `admin-rules.css.test.ts`, `admin-config.css.test.ts`,
 * `admin-proposals.css.test.ts`, `admin-account.css.test.ts`,
 * `library.css.test.ts`, `phoneLayout.css.test.ts`, `reader.css.test.ts`,
 * `settings.css.test.ts` and `tokens.css.test.ts`.
 *
 * **Every one of them is a `.ts` in the `node` project, and that is a
 * constraint rather than a coincidence.** Vitest's default `css: false`
 * stubs a `?raw` CSS import to the empty string, and only that project
 * carries the `css.include` line turning it off — so the same import that
 * hands a caller here the whole stylesheet hands a `.test.tsx` beside it
 * `''`, and every `rules()` count over it silently becomes zero. The rule is
 * about the **jsdom** project, not about the extension:
 * `../AcceptForm.browser.test.tsx` imports `../src/styles/tokens.css?raw`
 * into a `.tsx` and is fine, because the `browser` project does not stub CSS
 * either. What must not happen is a stylesheet assertion in the
 * project that does.
 *
 * Comments are stripped and each rule is split into its prelude and its
 * body; a caller that needs individual selectors or what one of them styles
 * builds on `selectors()` and `subject()` below.
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

/**
 * Each rule of `css` whose prelude is `prelude`: its body, and the at-rules,
 * such as `@media (width < 900px)`, that it sits inside, joined by a space,
 * or `''` for a rule at the top level, which is what `rules()` drops.
 * Brace-counted by hand, as `../styles/base.css.test.ts`'s
 * `splitByCoarsePointer` is, since an inner rule's own braces end a
 * non-greedy match at the wrong `}`.
 */
export function atRulesOf(
  css: string,
  prelude: string,
): Array<{ at: string; body: string }> {
  const text = withoutComments(css)
  const open: {
    head: string
    from: number
    found?: { at: string; body: string }
  }[] = []
  const found: { at: string; body: string }[] = []
  let start = 0
  for (let i = 0; i < text.length; i++) {
    const char = text[i]
    if (char === '{') {
      const head = text.slice(start, i).replace(/\s+/g, ' ').trim()
      const block: (typeof open)[number] = { head, from: i + 1 }
      if (head === prelude) {
        block.found = {
          at: open
            .map((one) => one.head)
            .filter((h) => h.startsWith('@'))
            .join(' '),
          body: '',
        }
        found.push(block.found)
      }
      open.push(block)
      start = i + 1
    } else if (char === '}') {
      const block = open.pop()
      if (block?.found !== undefined) {
        block.found.body = text.slice(block.from, i)
      }
      start = i + 1
    } else if (char === ';') {
      start = i + 1
    }
  }
  return found
}

/**
 * Each value `body` declares for `property`, in order, spaces collapsed:
 * `body` is one rule's, as `atRulesOf` hands it.
 */
export function valuesOf(body: string, property: string): string[] {
  return [
    ...body.matchAll(
      new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]*?)\\s*(?=;|$)`, 'g'),
    ),
  ].map((m) => m[1].replace(/\s+/g, ' '))
}

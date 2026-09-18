import { describe, expect, it } from 'vitest'
import baseSource from './base.css?raw'
import { rules, selectors } from '../testing/css'

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

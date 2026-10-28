import { describe, expect, it } from 'vitest'

/**
 * **Colour lives in `styles/tokens.css`**, spec 2026-09-17 §4 rule 2.
 *
 * Every non-test `.ts`, `.tsx` and `.css` file under `src/` is read as text
 * and counted for hex colours, `rgb()`/`rgba()`/`hsl()`/`hsla()` and CSS
 * system-colour keywords — outside comments, and for a keyword in TypeScript
 * only inside a string literal, since `Field` is also a type name there.
 *
 * **Two files are exempt**, both because their colour is data rather than
 * theme: `styles/tokens.css`, where colour is defined, and `palette.ts`,
 * whose note colours are drawn *on* artwork and so follow no theme.
 *
 * **The allowance only shrinks, and it is now empty.** It started at what the
 * tree held when the tokens arrived, and `toEqual` makes a file over its entry
 * fail and a file *under* its entry fail too. ⑤c was the style pass's last PR
 * and emptied it: `Account.tsx`'s ten went when its inline `style=` props
 * became class names, and `styles/annotations.css`'s two became
 * `--box-edge-bright` and `--box-hatch`.
 *
 * **Those last two were nearly exempted instead, on a reason that was
 * wrong**, and the correction is worth keeping because it is the kind of
 * mistake this file exists to make expensive. The argument was that both
 * values are deliberately theme-independent, and that moving them into
 * `tokens.css` would put values that must *not* follow the theme into the
 * file whose whole job is following it. `tokens.css` has carried a **Fixed**
 * block since the box palette was written, for exactly that case, and
 * `annotations.css` already drew four of its six colours from it. The
 * supporting quotation was also attributed to `palette.ts`, which does not
 * contain it — it is `annotations.css`'s own comment, about itself.
 *
 * Tokenising turned out to fix something as well: the hatch's
 * `rgba(74, 36, 0, 0.5)` was `--box-line` at half alpha, hand-copied two
 * lines above a `var(--box-fill)` in the same declaration. Changing
 * `--box-line` would have moved the outline and left the hatch behind.
 *
 * **Adding an entry back is a decision, not a fix.** A colour that needs to
 * exist outside `tokens.css` needs the exempt paragraph above to gain a third
 * bullet saying why — and "it must not follow the theme" is not that reason,
 * because `tokens.css` already holds such values.
 *
 * **What this cannot see:** a colour named in words (`black`), a colour built
 * at runtime, or a CSS id selector made only of hex digits (`#add`), which it
 * would count. Nor a regular expression literal in TypeScript, which is
 * scanned as code: one holding `//` or `/*` — `/\//` ends in two slashes —
 * reads to the counter as a comment opening, and it under-counts whatever
 * that comment appears to swallow. None of these exists here today.
 *
 * **And it is file-granular where it exempts.** Everything in an exempt file
 * is unseen, not just the values the exemption was for. That is why ⑤c
 * declined to add a third.
 */
const SOURCES = import.meta.glob<string>(
  ['./**/*.{ts,tsx,css}', '!./**/*.test.{ts,tsx}'],
  { base: '../src', query: '?raw', import: 'default', eager: true },
)

const EXEMPT = new Set(['./styles/tokens.css', './palette.ts'])

const ALLOWED: Record<string, number> = {}

const HEX = /(?<![\w&])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b/g
const COLOUR_FUNCTION = /\b(?:rgba?|hsla?)\(/g
const SYSTEM_COLOUR =
  /\b(?:Canvas|CanvasText|ButtonFace|ButtonText|ButtonBorder|GrayText|Highlight|HighlightText|AccentColor|AccentColorText|LinkText|VisitedText|Field|FieldText|Mark|MarkText|SelectedItem|SelectedItemText)\b/g

/**
 * Comments and string literals in one left-to-right pass, so a string holding
 * `/*` — `Routes.tsx`'s `"/config/*"` — is read as a string rather than as a
 * comment that swallows the code after it.
 */
const TS_TOKEN =
  /\/\*[\s\S]*?\*\/|\/\/[^\n]*|'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"|`(?:[^`\\]|\\.)*`/g

function colourCount(path: string, source: string): number {
  const count = (text: string, re: RegExp) => text.match(re)?.length ?? 0
  if (path.endsWith('.css')) {
    const code = source.replace(/\/\*[\s\S]*?\*\//g, ' ')
    return (
      count(code, HEX) +
      count(code, COLOUR_FUNCTION) +
      count(code, SYSTEM_COLOUR)
    )
  }
  const strings: string[] = []
  const code = source.replace(TS_TOKEN, (token) => {
    if (token.startsWith('/')) return ' '
    strings.push(token)
    return token
  })
  return (
    count(code, HEX) +
    count(code, COLOUR_FUNCTION) +
    count(strings.join('\n'), SYSTEM_COLOUR)
  )
}

describe('colour literals', () => {
  it('reads the tree it claims to', () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(100)
    expect(SOURCES['./styles/tokens.css']).toContain('light-dark(')
    expect(SOURCES['./App.tsx']).toContain('export default')
  })

  it('reads every stylesheet as its text, not as the empty string', () => {
    // A `.css` file the `node` project does not process (`vite.config.ts`,
    // `css.include`) arrives here as `''` rather than failing, and would be
    // counted as holding no colour.
    const css = Object.entries(SOURCES).filter(([path]) =>
      path.endsWith('.css'),
    )
    expect(css.length).toBeGreaterThan(5)
    expect(
      css.filter(([, source]) => source.trim() === '').map(([path]) => path),
    ).toEqual([])
  })

  it('counts literals and keywords, and nothing in a comment', () => {
    expect(
      colourCount(
        'probe.css',
        '/* ButtonFace #fff */ a { color: #fff; background: rgba(0, 0, 0, 0.5); border: 1px solid ButtonBorder }',
      ),
    ).toBe(3)
    expect(
      colourCount(
        'probe.ts',
        "// #fff\nconst a = { color: 'Canvas', b: '#abc', c: '/config/*' }\ntype Field = 1\nconst d = 'rgba(1, 2, 3, 0.5)'",
      ),
    ).toBe(3)
  })

  it('holds every file outside the tokens to exactly its allowance', () => {
    const actual: Record<string, number> = {}
    for (const [path, source] of Object.entries(SOURCES)) {
      if (EXEMPT.has(path)) continue
      const n = colourCount(path, source)
      if (n > 0) actual[path] = n
    }
    expect(actual).toEqual(ALLOWED)
  })
})

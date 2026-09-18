import { describe, expect, it } from 'vitest'

/**
 * **Colour lives in `styles/tokens.css`**, spec 2026-09-17 §4 rule 2.
 *
 * Every non-test file under `src/` is read as text and counted for hex
 * colours, `rgb()`/`rgba()`/`hsl()`/`hsla()` and CSS system-colour keywords —
 * outside comments, and for a keyword in TypeScript only inside a string
 * literal, since `Field` is also a type name there. Two files are exempt:
 * `styles/tokens.css`, where colours are defined, and `palette.ts`, whose
 * note colours are drawn on artwork and so follow no theme.
 *
 * **The allowance only shrinks.** It started at what the tree held when the
 * tokens arrived, and `toEqual` makes a file over its entry fail and a file
 * *under* its entry fail too — so a PR that removes literals has to lower the
 * entry, and the list cannot go stale. The style pass's last PR leaves it
 * empty.
 *
 * **What this cannot see:** a colour named in words (`black`), a colour built
 * at runtime, or a CSS id selector made only of hex digits (`#add`), which it
 * would count. Nor a regular expression literal in TypeScript, which is
 * scanned as code: one holding `//` or `/*` — `/\//` ends in two slashes —
 * reads to the counter as a comment opening, and it under-counts whatever
 * that comment appears to swallow. None of these exists here today.
 */
const SOURCES = import.meta.glob<string>(
  ['./**/*.{ts,tsx,css}', '!./**/*.test.{ts,tsx}', '!./testing/**'],
  { query: '?raw', import: 'default', eager: true },
)

const EXEMPT = new Set(['./styles/tokens.css', './palette.ts'])

const ALLOWED: Record<string, number> = {
  './Account.tsx': 10,
  './Annotations.tsx': 13,
  './Borrowed.tsx': 5,
  './Chrome.tsx': 11,
  './CorrectionBanner.tsx': 2,
  './CreditsPanel.tsx': 5,
  './Filmstrip.tsx': 4,
  './Help.tsx': 4,
  './NoteList.tsx': 5,
  './NotePanel.tsx': 6,
  './Settings.tsx': 5,
  './styles/annotations.css': 2,
}

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

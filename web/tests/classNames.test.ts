import { describe, expect, it } from 'vitest'
import { rules } from './support/css'

/**
 * **A class a component writes is named by some rule's selector**, O158.
 *
 * The non-test `.tsx` files under `src/` are read as text for the classes
 * they write, and the rules in `index.css` and `styles/*.css` — through
 * `support/css.ts`'s `rules()` — for the classes their selectors name. A
 * class written and named by no selector fails here, with the file that
 * writes it.
 *
 * **What it reads.** A `className="…"`, word by word. Inside a
 * `className={…}`, each word of a quoted string — except one side of an
 * `===` or `!==`, which is what `RuleForm.tsx`'s `'pattern'` is in
 * `name === 'pattern'` — and each word of a template literal that does not
 * touch a `${…}`. A site that spells nothing names its classes instead, and
 * `NAMED` says where each name's classes come from: a `const` in the same
 * file whose strings are read as classes, or `null` where nothing is.
 * **`NAMED` is held exactly**, so a new name fails here until it is listed.
 *
 * **What counts as a rule** is a class anywhere in a selector — its subject,
 * an ancestor, inside `:not()`. That is weaker than "draws the element"; what
 * it catches is a class no selector holds, which a rename that skipped the
 * stylesheet leaves behind.
 *
 * **What this cannot see.** A `className={…}` that draws on a table, `look`
 * or `classes`, is read through the table, and a ternary over strings is
 * read in full; two shapes hold a class this does not read:
 *
 * - **A class composed at runtime**: `NoteList.tsx`'s
 *   `` `note-list-${placement}` ``. Its two values have rules,
 *   `reader.css`'s `.note-list-side` and `.note-list-below`.
 * - **A class a function returns**: `ProposalRow.tsx`'s `strength(…)`, from
 *   `curation.ts`. Only `strong` of its three values has a rule — `admin.css`
 *   leaves `maybe` and `weak` plain on purpose — and renaming any of the
 *   three passes here.
 *
 * **The link from a name to its table is this file's word.** `look` is
 * `LOOKS[…]` in `Account.tsx`; what is checked is that `LOOKS` is found and
 * holds strings, not that `look` is drawn from it.
 *
 * Four more shapes would go unseen:
 *
 * - **A name outside a template literal, beside a string** — `cond ? 'a' :
 *   look.x`, `look.x ?? 'a'`, `fn('a')` — reads the string and not the
 *   name. A name inside a template's `${…}` is read, as `Account.tsx`'s
 *   `` `secret ${look.secret ?? ''}` `` is.
 * - **An object's keys as classes**, as `clsx` takes them: in
 *   `clsx('a', { b: on })` the key `b` is not read, and nor is `clsx` taken
 *   for a name.
 * - **A class applied without a `className` attribute** — through
 *   `classList`, `setAttribute('class', …)`, `.className =`, or a
 *   `className` property handed to a spread, `createElement` or
 *   `cloneElement` — in a `.ts` or a `.tsx`.
 * - **The tokeniser's limits**, which it shares with
 *   `colourLiterals.test.ts`: a `//` or `/*` in JSX text reads as a comment
 *   and hides what follows it, and a regular-expression literal holding one,
 *   or a quote or a backtick, is misread.
 *
 * **The exemptions are by class, not by site**, and they are held both ways:
 * an exempt class that gains a rule, or stops being written, fails until its
 * entry goes. It does not ask the reverse — a rule no component writes.
 */
const SOURCES = import.meta.glob<string>(['./**/*.tsx', '!./**/*.test.tsx'], {
  base: '../src',
  query: '?raw',
  import: 'default',
  eager: true,
})

const STYLES = import.meta.glob<string>(['./index.css', './styles/*.css'], {
  base: '../src',
  query: '?raw',
  import: 'default',
  eager: true,
})

/** Read only for the selectors `HOOKS` says they hold. */
const TESTS = import.meta.glob<string>('./**/*.test.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
})

/**
 * What a `className={…}` names instead of spelling, by file, and the `const`
 * in that file whose strings are its classes — `null` where none is read.
 */
const NAMED: Record<string, Record<string, string | null>> = {
  './Account.tsx': { look: 'LOOKS' },
  // A prop: each caller's class is read where the caller writes it.
  './CatalogAddress.tsx': { className: null },
  // `note-list-${placement}`: composed, so there is no table to read.
  './NoteList.tsx': { placement: null },
  // Defined in `curation.ts`, which this does not read.
  './ProposalRow.tsx': { strength: null },
  './Shell.tsx': { classes: 'classes' },
}

/**
 * Classes no selector names that a test selects by, under the test file that
 * does. Each is checked against that file's strings, so an entry outlives its
 * selector only by failing.
 *
 * **Listed, not discovered.** Exempting whatever some test selects by would
 * exempt the very rename this file exists for, whenever the rename reached a
 * test that selects by the class.
 */
const HOOKS: Record<string, string[]> = {
  './AcceptForm.test.tsx': [
    'edition-key',
    'edition-kind',
    'language',
    'match-kind',
  ],
  // The account page's "Password set." is drawn as the page is, and its
  // lists of grants, sessions and keys as the settings groups they are, so
  // no rule names them.
  './AccountPage.test.tsx': ['done', 'grant-list', 'key-list', 'session-list'],
  './App.browser.test.tsx': ['reader-turn'],
  './App.test.tsx': ['page-area'],
  // The account page's password form, which this file submits; drawn as
  // the page is, so no rule names it.
  './Config.browser.test.tsx': ['reset'],
  './Labeller.test.tsx': ['region-overlay'],
  // `rename-library` is the form holding the name's box in the title's
  // place; the box is drawn by its own class, and the form by none.
  './LibraryPage.test.tsx': ['dir-table', 'rename-library'],
  './MetadataEditor.test.tsx': [
    'archive',
    'asking',
    'consequences',
    'containers',
    'formats',
    'key-held',
    'key-moved',
    'key-moved-alone',
    'metadata-editor',
    'nothing',
    'rewrote',
    'sidecar',
    'skipped',
    'stamp-comicinfo',
    'stamp-state',
    'stamped',
    'total',
    'unopened',
    'unrecognised',
    'writing',
  ],
  './PreviewTabs.test.tsx': [
    'edit-winner',
    'others',
    'provenance',
    'reorder',
    'truncated',
  ],
  './ProposalRow.test.tsx': ['page'],
  './Proposals.test.tsx': ['tab', 'weak-check'],
  './RuleEditor.test.tsx': ['held-edit', 'refused', 'save', 'saved'],
  './RuleList.test.tsx': [
    'add-rule',
    'confirm-delete',
    'delete',
    'override-badge',
  ],
}

/**
 * Classes no selector names and nothing selects by, on an element something
 * else draws — and what that is.
 */
const ELSEWHERE: Record<string, string> = {
  labeller: 'its own `style` prop',
  'labeller-controls': 'its own `style` prop',
  'labeller-legend-row': "`.labeller-legend`'s rules, as that element's child",
  matched: "admin.css's `.preview` table rules; a comment beside them names it",
  'save-derive': "admin.css's `summary` rule, whose comment names it",
  size: "Backup's, `.settings-row-help`'s rules, as part of a row's help; `MetadataEditor`'s, nothing but the archive's row it sits in",
  when: "`.settings-row-help`'s rules, as part of a row's help",
}

/**
 * Classes no selector names, no test or source selects by, and no stylesheet
 * spells as `.name` even in a comment: names nothing reads. Whether each
 * should gain a rule or lose its name is not decided here. A rule on the
 * element's tag or an ancestor may still draw it; this test does not look.
 */
const UNREAD = [
  'archives',
  'complaint',
  'impact',
  'labeller-readout',
  'options',
  'overrides',
  'overrides-lead',
  'path',
  'plan-failure',
  'preview-body',
  'save-mismatch',
  'scan-notice',
  'series-count',
  'stamp-failure',
  'unwritten',
  'verdict',
  'walk',
  'you',
]

const EXEMPT = new Set([
  ...Object.values(HOOKS).flat(),
  ...Object.keys(ELSEWHERE),
  ...UNREAD,
])

/**
 * Comments and string literals in one left-to-right pass — the same pattern
 * as `colourLiterals.test.ts`'s, for the same reason: `Routes.tsx`'s
 * `"/config/*"` is a string, not a comment opening.
 */
const TS_TOKEN =
  /\/\*[\s\S]*?\*\/|\/\/[^\n]*|'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"|`(?:[^`\\]|\\.)*`/g

/** The string-literal half of `TS_TOKEN`. */
const LITERAL = /'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"|`(?:[^`\\]|\\.)*`/g

/** A literal, so that a bracket inside one is skipped, or a bracket. */
const LITERAL_OR_BRACKET = new RegExp(`${LITERAL.source}|[{}[\\]()]`, 'g')

/** A class in a selector. No ident starts with a digit, so `.5em` is none. */
const CLASS = /\.(-?[_a-zA-Z][\w-]*)/g

function uncommented(source: string): string {
  return source.replace(TS_TOKEN, (token) =>
    token.startsWith('/') ? ' ' : token,
  )
}

/** From `start` to the bracket closing the one opened just before it. */
function bracketed(code: string, start: number): string {
  let depth = 1
  for (const m of code.slice(start).matchAll(LITERAL_OR_BRACKET)) {
    if (/^[{[(]$/.test(m[0])) depth++
    else if (/^[}\])]$/.test(m[0]) && --depth === 0)
      return code.slice(start, start + m.index)
  }
  throw new Error(`no bracket closes the one before offset ${start}`)
}

const words = (text: string) => text.split(/\s+/).filter(Boolean)

/** `look` in `look.note`, `strength` in `strength(proposal.score)`. */
const leading = (expression: string) =>
  /[A-Za-z_$][\w$]*/.exec(expression)?.[0] ?? expression.trim()

/**
 * The classes `expression`'s strings and template literals spell, and the
 * names its templates' `${…}` hold.
 */
function spelled(expression: string): { classes: string[]; names: string[] } {
  const classes: string[] = []
  const names: string[] = []
  for (const m of expression.matchAll(LITERAL)) {
    const before = expression.slice(0, m.index)
    const after = expression.slice(m.index + m[0].length)
    if (/[=!]==?\s*$/.test(before) || /^\s*[=!]==?/.test(after)) continue
    const text = m[0].slice(1, -1)
    if (!m[0].startsWith('`')) {
      classes.push(...words(text))
      continue
    }
    const marked = text.replace(/\$\{([^}]*)\}/g, (_, inner: string) => {
      names.push(leading(inner))
      return '\0'
    })
    classes.push(...words(marked).filter((word) => !word.includes('\0')))
  }
  return { classes, names }
}

/** The classes `source` writes, and the names it writes classes through. */
function written(source: string): { classes: string[]; names: string[] } {
  const code = uncommented(source)
  const classes: string[] = []
  const names: string[] = []
  for (const m of code.matchAll(/\bclassName\s*=\s*(?:"([^"]*)"|\{)/g)) {
    if (m[1] !== undefined) {
      classes.push(...words(m[1]))
      continue
    }
    const expression = bracketed(code, m.index + m[0].length)
    const read = spelled(expression)
    classes.push(...read.classes)
    names.push(...read.names)
    if (read.classes.length + read.names.length === 0)
      names.push(leading(expression))
  }
  return { classes, names }
}

/** What `const name = {…}` or `[…]` in `source` spells, if it is there. */
function table(source: string, name: string): string[] | undefined {
  const code = uncommented(source)
  const m = new RegExp(`\\bconst ${name}\\b[^=]*=\\s*[[{]`).exec(code)
  if (m === null) return undefined
  return spelled(bracketed(code, m.index + m[0].length)).classes
}

const DRAWN = new Set(
  Object.values(STYLES).flatMap((css) =>
    rules(css).flatMap(({ prelude }) =>
      [...prelude.matchAll(CLASS)].map((m) => m[1]),
    ),
  ),
)

function scan() {
  /** Each class written, and the files writing it. */
  const classes = new Map<string, Set<string>>()
  const names: Record<string, string[]> = {}
  const unreadTables: string[] = []
  const add = (name: string, file: string) => {
    classes.set(name, (classes.get(name) ?? new Set()).add(file))
  }
  for (const [file, source] of Object.entries(SOURCES)) {
    const read = written(source)
    for (const name of read.classes) add(name, file)
    if (read.names.length > 0) names[file] = [...new Set(read.names)].sort()
    for (const from of Object.values(NAMED[file] ?? {})) {
      if (from === null) continue
      const spelt = table(source, from) ?? []
      if (spelt.length === 0) unreadTables.push(`${from} in ${file}`)
      for (const name of spelt) add(name, file)
    }
  }
  return { classes, names, unreadTables }
}

const SCAN = scan()

describe('class names', () => {
  it('reads the tree it claims to', () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(40)
    expect(SOURCES['./Account.tsx']).toContain('const LOOKS')
    // As `colourLiterals.test.ts` says of its own: a stylesheet the `node`
    // project does not process arrives as `''` and would name no class.
    expect(Object.keys(STYLES).length).toBeGreaterThan(5)
    expect(
      Object.entries(STYLES)
        .filter(([, css]) => css.trim() === '')
        .map(([path]) => path),
    ).toEqual([])
    expect(DRAWN.has('reader-failure')).toBe(true)
    expect([...(SCAN.classes.get('reader-failure') ?? [])]).toEqual([
      './Account.tsx',
    ])
  })

  it('reads strings, ternaries and whole template words, not comments', () => {
    // Were `"/config/*"` read as a comment opening, it would run to the `*/`
    // two lines down and swallow `a b`.
    expect(
      written(
        [
          '<Route path="/config/*" />',
          '<p className="a b">',
          '{/* <p className="block-comment"> */}',
          '// <p className="line-comment">',
          "<p className={kind === 'compared' ? 'c' : undefined}>",
          `<p className={\`d e-\${f}\`}>`,
          '<p className={look.g}>',
        ].join('\n'),
      ),
    ).toEqual({ classes: ['a', 'b', 'c', 'd'], names: ['f', 'look'] })
    expect(
      table(
        [
          "const T: Record<'x', string> = {",
          "  // a: 'line-comment',",
          "  h: 'h i',",
          "  j: kind === 'compared' ? 'j' : '',",
          '}',
        ].join('\n'),
        'T',
      ),
    ).toEqual(['h', 'i', 'j'])
  })

  it('writes through the names it lists, and finds each table it names', () => {
    expect(SCAN.names).toEqual(
      Object.fromEntries(
        Object.entries(NAMED).map(([file, names]) => [
          file,
          Object.keys(names).sort(),
        ]),
      ),
    )
    expect(SCAN.unreadTables).toEqual([])
  })

  it('fails on a class it reads that no selector names', () => {
    const missing = [...SCAN.classes]
      .filter(([name]) => !DRAWN.has(name) && !EXEMPT.has(name))
      .map(([name, files]) => `${name} in ${[...files].sort().join(', ')}`)
      .sort()
    expect(missing).toEqual([])
  })

  it('exempts only classes still written and still in no selector', () => {
    expect(
      [...EXEMPT]
        .filter((name) => DRAWN.has(name) || !SCAN.classes.has(name))
        .sort(),
    ).toEqual([])
  })

  it('holds each hook to a test that still selects by it', () => {
    const lost: string[] = []
    for (const [file, hooks] of Object.entries(HOOKS)) {
      const strings = (uncommented(TESTS[file] ?? '').match(LITERAL) ?? []).map(
        (literal) => literal.replace(/\$\{[^}]*\}/g, ''),
      )
      for (const hook of hooks) {
        const selector = new RegExp(`\\.${hook}(?![\\w-])`)
        if (!strings.some((literal) => selector.test(literal)))
          lost.push(`${hook} in ${file}`)
      }
    }
    expect(lost).toEqual([])
  })
})

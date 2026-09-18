# Style pass ① — the foundation: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the web UI its ink-and-paper tokens, fonts and theme switch, restyle every control on the browse screens and the login screen through them, and put the guards in place that keep colour in one file — without moving any screen's layout.

**Architecture:** `web/src/index.css` becomes six `@import`ed partials under `web/src/styles/`. Colours live only in `styles/tokens.css`, each written once as `light-dark(<paper>, <ink>)`; `color-scheme` picks the half, from the OS or from `data-theme` on `<html>`. The 79 CSS system-colour keywords are replaced one-to-one by tokens, `styles/base.css` styles elements inside `.library` and `.login` only, and a theme preference (`theme.ts`) is applied before first paint by an inline script in `index.html` and changed from a control in the sidebar.

**Tech Stack:** React 19, Vite 8, Vitest 4 (projects `node`, `jsdom`, `browser`), Biome 2.5.7, pnpm 11, `@fontsource` 5.3.0.

**Spec:** [`docs/superpowers/specs/2026-09-17-style-pass-design.md`](../specs/2026-09-17-style-pass-design.md) — this plan is its PR ①.

**Branch:** `style-1-foundation`, which already carries the spec (`6c6a217`).

## Decided while planning — the spec is amended to match in Task 8

These came out of reading the code the plan touches. Each is recorded in the
spec by Task 8 so the spec stays true.

1. **Zen Kaku Gothic New has no tabular figures.** Its Latin subset
   (`@fontsource/zen-kaku-gothic-new@5.3.0`, `latin-400`) carries only the
   `ccmp`, `frac`, `vert` and `vrt2` GSUB features and its digits advance 383
   to 509 units, read with fontTools. Tables therefore use `--font-figures`: a
   `taiyaku figures` face that maps U+0030–0039 to a local tabular sans
   (`Helvetica Neue`, `Roboto`, `Noto Sans`, `DejaVu Sans`, `Arial`) and falls
   back to `--font-ui` for everything else.
2. **`base.css` styles controls only inside `.library` and `.login`.** The
   reader's chrome is inline-styled and leaves some properties to the browser,
   so a document-wide `button` rule would change a filmstrip tile's border or
   a panel button's padding under geometry PR ③ redraws. Every scoped rule is
   `:where(.library, .login) :where(…)`, zero specificity, so every existing
   class rule still wins.
   **The four document-wide rules do reach the reader and the labeller**, and
   that is accepted rather than overlooked: `:root`'s `font`, so what in the
   reader opts in with `font: inherit` — its Library button and its readout —
   renders in Zen Kaku Gothic New from ① and its intrinsic width moves;
   `body`'s `background` and `color`; and `::selection`. The user decided this
   on 2026-09-17, after Task 4's review found it, on the grounds that ③
   redraws that chrome anyway, and the browser project passed 28 of 28 before
   and after (its reader cases measure live boxes rather than pinning widths).
   So "no change to the reader" binds reader *files*, not everything about how
   the reader looks. *This first predicted the layout buttons would change
   font too; Task 8 measured that they do not.* A bare `<button>` does not
   inherit `:root`'s font, because the UA stylesheet sets one on `button`: the
   layout button `single` computed `font-family: Arial` in Chromium on Linux,
   which is that platform's UA default, not a promise of Arial elsewhere.
3. **`viewport-fit=cover` moves to PR ③.** Without the safe-area padding ③
   designs, it would put the library under a phone's notch in landscape.
4. **The contrast test's trust case changes shape.** The shipped defects
   (D2's 3.81:1, the Accept's 1.35:1) were measured against Chromium's system
   colours, which no rule uses after this PR. The token model instead
   reproduces four ratios the spec's §2 computed outside the test.
5. **The system-colour keywords are replaced in this PR, all 79.** Spec §4
   rule 5 recomputes D2, D3 and the Accept against tokens, which is only true
   if those rules name tokens. The mapping is spec §2's, one to one.
6. **The colour-literal guard counts system-colour keywords in `.ts`/`.tsx`
   only inside string literals**, and strips comments with a scanner that
   knows strings, because `Routes.tsx` holds the string `"/config/*"` and
   `RootRow.tsx` `'/**'`, either of which a naive `/* … */` strip would treat
   as a comment opening.
7. **The browser project runs in Tasks 2, 4 and 6, not only in Task 8.**
   Four of its five files import `index.css`, and two assert layout at 700 px
   that a font change can move. Its baseline on this branch before any task,
   `vitest run --project browser`: 5 files, 28 tests, all passed. *Added by
   the pre-flight review, 2026-09-17.*
8. **`AcceptForm.browser.test.tsx`'s palette check moves to the tokens in
   Task 2.** It re-measured Chromium's system colours so that
   `index.css.test.ts`'s model stayed honest; once that model reads tokens,
   the old check would keep passing while guarding nothing. Its replacement
   asks Chromium to resolve every `light-dark()` pair under each
   `color-scheme` and compares the pixels with the pair the node model
   parses. *Added by the pre-flight review, 2026-09-17.*

## Global Constraints

- Colour values are exactly spec §2's: paper `bg #f3eee3`, `surface #e9e2d2`, `field #fbf8f2`, `line #d6ccb9`, `line-strong #897d68`, `text #231e17`, `muted #6b6151`, `accent #b0361d`, `on-accent #fbf8f2`, `danger #9e1b32`, `success #3f6b2f`, `warning #7d5500`; ink `bg #15130f`, `surface #211e18`, `field #1b1914`, `line #35312a`, `line-strong #746c60`, `text #ece4d4`, `muted #a69b87`, `accent #e8715a`, `on-accent #15130f`, `danger #f28b9a`, `success #95c283`, `warning #e2b55c`.
- Fonts: Shippori Mincho B1 600 and 700; Zen Kaku Gothic New 400, 500 and 700; from `@fontsource`, never a CDN.
- The theme is stored at `localStorage` key `taiyaku.prefs.theme` as `{"choice": "system" | "paper" | "ink"}`, default `system`.
- No colour literal (hex, `rgb()`, `rgba()`, `hsl()`, `hsla()`) or CSS system-colour keyword may be added to any non-test file under `web/src/` except `styles/tokens.css` and `palette.ts`.
- Inline `style={{}}` objects keep every geometry property they have today — **except the login screen**, the one screen ① restyles: spec §2's corners (0 on panels, 2 px on controls) and the chosen direction's "hairline rules instead of boxes" replace its 12 px radius, its drop shadow and its four-sided border with top and bottom rules. *Scoped 2026-09-17, after Task 7's review read the unqualified constraint as forbidding that.*
- No change to the reader (`App.tsx`, `Chrome.tsx`, `Filmstrip.tsx`, panels) beyond comment text.
- Component tests use `mount()` from `web/src/testing/mount.ts` and `act()`; `@testing-library/react` is not a dependency.
- `pnpm -C web test` runs the `node` project only. Run a `.test.tsx` with `--project jsdom`. Under Claude Code pass `--reporter=default` when reading test output.
- `tsc --noEmit -p .` checks nothing here; typecheck with `pnpm -C web run typecheck`.
- Scratch output goes under `target/style-pass/` (gitignored), never `/tmp` or `$HOME`.
- Commit at the end of every task. Commit messages carry no attribution lines.

---

### Task 1: Split `index.css` into partials, changing nothing that ships

**Files:**
- Create: `web/src/styles/base.css`, `web/src/styles/tokens.css`, `web/src/styles/annotations.css`, `web/src/styles/shell.css`, `web/src/styles/library.css`, `web/src/styles/admin.css`
- Modify: `web/src/index.css` (whole file), `web/src/index.css.test.ts:1-2`, `web/src/index.css.test.ts:84-91`, `web/vite.config.ts:55-64`

**Interfaces:**
- Produces: the six partials, each holding exactly the rules named in spec §4's table; `index.css` as six `@import`s in the order base, tokens, annotations, shell, library, admin; `vite.config.ts`'s node project processing CSS for `src/index.css` and `src/styles/*.css`.

- [ ] **Step 1: Build the current stylesheet as the baseline**

Run from the repository root:

```bash
pnpm -C web exec vite build --outDir ../target/style-pass/dist-before --emptyOutDir
```

Expected: the build ends with `✓ built in …`, and `target/style-pass/dist-before/assets/` holds at least one `.css` file.

- [ ] **Step 2: Confirm the five group boundaries are blank lines**

```bash
wc -l < web/src/index.css
sed -n '25p;47p;295p;398p;765p' web/src/index.css | command grep -c '^$'
```

Expected: `1693`, then `5`. If either differs, stop: `index.css` changed since this plan was written and the ranges below are wrong.

- [ ] **Step 3: Move the rules**

```bash
mkdir web/src/styles
sed -n '1,24p' web/src/index.css > web/src/styles/base.css
sed -n '26,46p' web/src/index.css > web/src/styles/tokens.css
sed -n '48,294p' web/src/index.css > web/src/styles/annotations.css
sed -n '296,397p' web/src/index.css > web/src/styles/shell.css
sed -n '399,764p' web/src/index.css > web/src/styles/library.css
sed -n '766,1693p' web/src/index.css > web/src/styles/admin.css
```

Check nothing was lost or duplicated:

```bash
command grep -v '^$' web/src/index.css > target/style-pass/index-before.txt
cat web/src/styles/base.css web/src/styles/tokens.css web/src/styles/annotations.css web/src/styles/shell.css web/src/styles/library.css web/src/styles/admin.css | command grep -v '^$' > target/style-pass/index-joined.txt
cmp target/style-pass/index-before.txt target/style-pass/index-joined.txt && echo identical
```

Expected: `identical`. (`mkdir -p target/style-pass` first if Step 1 did not create it.)

- [ ] **Step 4: Replace `index.css` with the imports**

Write `web/src/index.css`:

```css
/* The stylesheet, by area — spec 2026-09-17 §4. Vite inlines every import at
 * build time. The order is the order the rules had when this was one file,
 * which matters wherever two rules tie on specificity. */
@import "./styles/base.css";
@import "./styles/tokens.css";
@import "./styles/annotations.css";
@import "./styles/shell.css";
@import "./styles/library.css";
@import "./styles/admin.css";
```

- [ ] **Step 5: Run the contrast test to see it lose its rules**

```bash
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: FAIL. Every `describe` errors with `no rule found for … in index.css`, because `index.css?raw` is now six import lines.

- [ ] **Step 6: Point the test at the partials**

In `web/src/index.css.test.ts`, replace line 2:

```ts
import cssSource from './index.css?raw'
```

with:

```ts
import adminSource from './styles/admin.css?raw'
import librarySource from './styles/library.css?raw'

/** D2 and D3 live in `library.css`, the disabled Accept in `admin.css`. */
const cssSource = [librarySource, adminSource].join('\n')
```

In the same file, in `ruleBody`, replace:

```ts
  if (!m) throw new Error(`no rule found for ${selector} in index.css`)
```

with:

```ts
  if (!m) throw new Error(`no rule found for ${selector} in styles/`)
```

In `web/vite.config.ts`, replace lines 55–64:

```ts
          // Vitest's default `css: false` stubs every CSS import — `?raw`
          // included — to an empty string, which is fine for the rest of
          // this project (no test here reads CSS text) but breaks
          // `index.css.test.ts`'s `import cssSource from './index.css?raw'`,
          // the WCAG-contrast regression test for the browser pass's D2 and
          // D3: `Login.theme.test.ts` already reads `.tsx` source the same
          // way to catch a hardcoded colour, and CSS needs the identical
          // treatment to catch a wrong opacity. Scoped to `index.css` alone,
          // not `css: true`, so no other test pays for real CSS processing.
          css: { include: [/index\.css/] },
```

with:

```ts
          // Vitest's default `css: false` stubs every CSS import — `?raw`
          // included — to an empty string. The tests that read stylesheet
          // text rather than render it (jsdom resolves neither `light-dark()`
          // nor a composited `opacity`) need the real text, so CSS is
          // processed for `index.css` and the partials in `styles/` and
          // nothing else — not `css: true`, so no other test pays for real
          // CSS processing.
          css: { include: [/src\/(index\.css|styles\/[\w-]+\.css)/] },
```

- [ ] **Step 7: Run the contrast test again**

```bash
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: PASS, 10 tests.

- [ ] **Step 8: Prove the shipped CSS did not change**

```bash
pnpm -C web exec vite build --outDir ../target/style-pass/dist-split --emptyOutDir
diff -r target/style-pass/dist-before target/style-pass/dist-split && echo "dist identical"
```

Expected: `dist identical`. Vite drops comments and inlines the imports, so the same rules in the same order produce the same bytes and the same content-hashed file names. Any difference is a rule that moved out of order: stop and compare the two CSS files.

- [ ] **Step 9: Lint and typecheck**

```bash
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: both exit 0. If Biome reports formatting in `index.css`, run `pnpm -C web exec biome format --write src/index.css` and re-run the lint.

- [ ] **Step 10: Commit**

```bash
git add web/src/index.css web/src/styles web/src/index.css.test.ts web/vite.config.ts
git commit -m "Style pass 1: split index.css into partials, byte-identical build"
```

---

### Task 2: Colour tokens, and every system colour replaced by one

**Files:**
- Modify: `web/src/styles/tokens.css` (prepend), `web/src/styles/base.css:1-6` and `:22-24`, `web/src/styles/{annotations,shell,library,admin}.css` (keyword substitution and seven comments), `web/src/index.css.test.ts` (whole file), `web/src/AcceptForm.browser.test.tsx:1-8`, `:36-39`, `:232-234`, `:329-330`, `:360-396`, `web/src/Shell.tsx:10-11`, `web/src/Chrome.tsx:183-186`, `web/src/Account.tsx:107-109`

**Interfaces:**
- Consumes: Task 1's partials.
- Produces: custom properties `--bg`, `--surface`, `--field`, `--line`, `--line-strong`, `--text`, `--muted`, `--accent`, `--on-accent`, `--danger`, `--success`, `--warning` on `:root`, each `light-dark(<paper>, <ink>)`; `:root[data-theme="paper"]` pins `color-scheme: light`, `:root[data-theme="ink"]` pins `dark`. No CSS system-colour keyword remains outside a comment in `styles/`.

- [ ] **Step 1: Write the token-driven contrast test**

Replace the whole of `web/src/index.css.test.ts` with:

```ts
import { describe, expect, it } from 'vitest'
import adminSource from './styles/admin.css?raw'
import librarySource from './styles/library.css?raw'
import shellSource from './styles/shell.css?raw'
import tokensSource from './styles/tokens.css?raw'

/**
 * WCAG AA contrast, computed from the stylesheets' own text.
 *
 * **jsdom cannot answer this by rendering** — it resolves neither
 * `light-dark()` nor a composited `opacity` into pixels — so this reads the
 * `light-dark(<paper>, <ink>)` pairs in `styles/tokens.css` and the rules in
 * `styles/`, and recomputes the relative-luminance arithmetic a browser
 * applies.
 *
 * **Why the model is trusted.** Until 2026-09-17 these rules used CSS system
 * colours, and this file modelled the values a live Chromium reported for
 * them; that model reproduced the browser pass's own figures (D2's 3.81:1 in
 * dark, the disabled Accept's 1.35:1), which is what showed it was the real
 * one. No rule names a system colour now, so the token model earns the same
 * trust by reproducing four ratios the style pass's spec computed outside this
 * file (`docs/superpowers/specs/2026-09-17-style-pass-design.md`, §2).
 *
 * Three regression checks predate the tokens and are kept, recomputed against
 * them: D2 and D3 from the 2026-08-21 browser pass, and the disabled Accept.
 * The fixes are described above `.alignment-panel .paired` and
 * `.volumes li.without .cover-box` in `styles/library.css`, and above
 * `.proposal-detail .accept:disabled` in `styles/admin.css`.
 */

type Rgb = readonly [number, number, number]
const SCHEMES = ['paper', 'ink'] as const
type Scheme = (typeof SCHEMES)[number]

const cssSource = [shellSource, librarySource, adminSource].join('\n')

function hex(value: string): Rgb {
  const n = Number.parseInt(value.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

/** Every `--name: light-dark(#paper, #ink)` in `tokens.css`. */
const TOKENS = new Map<string, Record<Scheme, Rgb>>(
  [
    ...tokensSource.matchAll(
      /--([a-z-]+):\s*light-dark\(\s*(#[0-9a-fA-F]{6})\s*,\s*(#[0-9a-fA-F]{6})\s*\)/g,
    ),
  ].map((m): [string, Record<Scheme, Rgb>] => [
    m[1],
    { paper: hex(m[2]), ink: hex(m[3]) },
  ]),
)

function token(name: string, scheme: Scheme): Rgb {
  const pair = TOKENS.get(name)
  if (!pair) throw new Error(`tokens.css has no light-dark() pair for --${name}`)
  return pair[scheme]
}

/** A declared `var(--name)`, as the colour that token paints in `scheme`. */
function resolve(value: string, scheme: Scheme): Rgb {
  const m = value.match(/^var\(--([a-z-]+)\)$/)
  if (!m) throw new Error(`not a token reference: ${value}`)
  return token(m[1], scheme)
}

function channelLum(c: number): number {
  const s = c / 255
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}

function relLum([r, g, b]: Rgb): number {
  return (
    0.2126 * channelLum(r) + 0.7152 * channelLum(g) + 0.0722 * channelLum(b)
  )
}

/** WCAG contrast ratio between two sRGB colours. */
function contrast(a: Rgb, b: Rgb): number {
  const la = relLum(a)
  const lb = relLum(b)
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
}

/**
 * `opacity` composites the *whole element group* against what is behind it —
 * not the text colour alone blended toward the background it sits on. A group
 * rendered at `alpha` shows, wherever a layer painted,
 * `alpha * layer + (1 - alpha) * backdrop`; `alpha = 1` is the unchanged
 * colour, which is what lets one helper cover the dimmed and undimmed cases.
 */
function composite(fg: Rgb, backdrop: Rgb, alpha: number): Rgb {
  return [
    alpha * fg[0] + (1 - alpha) * backdrop[0],
    alpha * fg[1] + (1 - alpha) * backdrop[1],
    alpha * fg[2] + (1 - alpha) * backdrop[2],
  ]
}

function ruleBody(selector: string): string {
  const re = new RegExp(
    `${selector.replace(/[.[\]]/g, '\\$&')}\\s*\\{([^}]*)\\}`,
  )
  const m = cssSource.match(re)
  if (!m) throw new Error(`no rule found for ${selector} in styles/`)
  return m[1]
}

function declared(body: string, property: string): string | undefined {
  const m = body.match(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`))
  return m?.[1].trim()
}

function required(body: string, property: string): string {
  const value = declared(body, property)
  if (value === undefined) throw new Error(`rule sets no ${property}: ${body}`)
  return value
}

function opacityOf(body: string): number {
  const m = body.match(/opacity:\s*([\d.]+)/)
  if (!m) throw new Error(`rule sets no opacity: ${body}`)
  return Number(m[1])
}

describe('the token model', () => {
  it('reproduces four ratios the spec computed outside this file', () => {
    expect(contrast(token('text', 'paper'), token('bg', 'paper'))).toBeCloseTo(
      14.3,
      2,
    )
    expect(
      contrast(token('muted', 'paper'), token('surface', 'paper')),
    ).toBeCloseTo(4.71, 2)
    expect(
      contrast(token('line-strong', 'ink'), token('bg', 'ink')),
    ).toBeCloseTo(3.58, 2)
    expect(contrast(token('accent', 'ink'), token('bg', 'ink'))).toBeCloseTo(
      6.13,
      2,
    )
  })
})

const GROUNDS = ['bg', 'surface', 'field'] as const
const TEXT_TOKENS = [
  'text',
  'muted',
  'accent',
  'danger',
  'success',
  'warning',
] as const

describe("spec §2's floors hold in both themes", () => {
  for (const scheme of SCHEMES) {
    it(`every text colour clears 4.5:1 and every control border 3:1 in ${scheme}`, () => {
      const failures: string[] = []
      for (const ground of GROUNDS) {
        for (const text of TEXT_TOKENS) {
          const ratio = contrast(token(text, scheme), token(ground, scheme))
          if (ratio < 4.5) {
            failures.push(`--${text} on --${ground}: ${ratio.toFixed(2)}`)
          }
        }
        const border = contrast(
          token('line-strong', scheme),
          token(ground, scheme),
        )
        if (border < 3) {
          failures.push(`--line-strong on --${ground}: ${border.toFixed(2)}`)
        }
      }
      const onAccent = contrast(
        token('on-accent', scheme),
        token('accent', scheme),
      )
      if (onAccent < 4.5) {
        failures.push(`--on-accent on --accent: ${onAccent.toFixed(2)}`)
      }
      expect(failures).toEqual([])
    })
  }
})

describe('D2: `.alignment-panel .paired` clears WCAG AA in both themes', () => {
  // `.paired` is `178 of 180`, the number the panel exists to show, faded by
  // `opacity` over the panel's own ground. One opacity lands at different
  // ratios on different grounds, so it has to clear 4.5:1 in both themes.
  const panel = ruleBody('.alignment-panel')
  const opacity = opacityOf(ruleBody('.alignment-panel .paired'))

  for (const scheme of SCHEMES) {
    it(`clears 4.5:1 in ${scheme}`, () => {
      const ground = resolve(required(panel, 'background'), scheme)
      const text = resolve(required(panel, 'color'), scheme)
      expect(
        contrast(composite(text, ground, opacity), ground),
      ).toBeGreaterThanOrEqual(4.5)
    })
  }
})

describe('D3: a dimmed volume tile fades its cover, not its label or its button', () => {
  // The defect: `.volumes li.without { opacity: 0.5 }` dimmed the whole `<li>`
  // — cover, title and the `i` button together — though the tile is not
  // disabled. `groupOpacity` is what that bare rule would contribute; with the
  // rule gone (the fix) there is nothing left to dim them and it is 1.
  function groupOpacity(): number {
    const m = cssSource.match(/\.volumes li\.without\s*\{([^}]*)\}/)
    return m ? opacityOf(m[1]) : 1
  }

  const library = ruleBody('.library')
  const button = ruleBody('.volumes .panel-open')

  it('no longer dims the whole tile as one group', () => {
    expect(cssSource).not.toMatch(/\.volumes li\.without\s*\{/)
  })

  it('dims the cover box instead, and still dims it (not merely relocated to a no-op)', () => {
    const opacity = opacityOf(ruleBody('.volumes li.without .cover-box'))
    expect(opacity).toBeLessThan(1)
  })

  for (const scheme of SCHEMES) {
    it(`leaves the title at 4.5:1 or better in ${scheme}`, () => {
      const alpha = groupOpacity()
      const ground = resolve(required(library, 'background'), scheme)
      const text = resolve(required(library, 'color'), scheme)
      expect(
        contrast(composite(text, ground, alpha), ground),
      ).toBeGreaterThanOrEqual(4.5)
    })

    it(`leaves the \`i\` button at 4.5:1 or better in ${scheme}`, () => {
      const alpha = groupOpacity()
      const ground = resolve(required(library, 'background'), scheme)
      const text = composite(
        resolve(required(button, 'color'), scheme),
        ground,
        alpha,
      )
      const face = composite(
        resolve(required(button, 'background'), scheme),
        ground,
        alpha,
      )
      expect(contrast(text, face)).toBeGreaterThanOrEqual(4.5)
    })
  }
})

/**
 * **The disabled Accept** — the one place on the proposals panel where "WCAG
 * exempts an inactive component" was the wrong call, because the form sits
 * disabled for the whole of every edit whose answer is a refusal. It shipped at
 * `GrayText` over `ButtonFace`, 1.35:1 in dark Chromium. The fix keeps the word
 * at full contrast and says "unavailable" with the ground and the border
 * instead, so this guards both halves.
 */
describe('the disabled Accept is legible in both themes', () => {
  const enabled = ruleBody('.proposal-detail .accept')
  const disabled = ruleBody('.proposal-detail .accept:disabled')

  /** What the disabled control actually paints: its own rule, or the base. */
  function effective(property: string): string {
    const value = declared(disabled, property) ?? declared(enabled, property)
    if (value === undefined) {
      throw new Error(`neither .accept nor .accept:disabled sets ${property}`)
    }
    return value
  }

  for (const scheme of SCHEMES) {
    it(`clears 4.5:1 in ${scheme}`, () => {
      expect(
        contrast(
          resolve(effective('color'), scheme),
          resolve(effective('background'), scheme),
        ),
      ).toBeGreaterThanOrEqual(4.5)
    })
  }

  it('is still not drawn as the live control', () => {
    expect(declared(disabled, 'background')).toBeDefined()
    expect(declared(disabled, 'background')).not.toBe(
      declared(enabled, 'background'),
    )
    expect(declared(disabled, 'border-style')).toBe('dashed')
    expect(declared(enabled, 'border')).toContain('solid')
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: FAIL. The token-model and floors cases fail with `tokens.css has no light-dark() pair for --text`; D2, D3's four scheme cases and the Accept's two fail with `not a token reference: ButtonFace` or `CanvasText`. The two D3 structure cases and "is still not drawn as the live control" pass.

- [ ] **Step 3: Define the tokens**

Prepend to `web/src/styles/tokens.css`, above the box-palette comment that is its first line today, followed by one blank line:

```css
/* Design tokens — every colour the themed screens use (spec 2026-09-17 §2,
 * `docs/superpowers/specs/2026-09-17-style-pass-design.md`).
 *
 * Each colour is written once, as `light-dark(<paper>, <ink>)`, and
 * `color-scheme` picks the half: the OS's preference by default, or the
 * reader's choice through `data-theme` on `<html>` (`theme.ts`). Every text
 * colour clears 4.5:1 on `--bg`, `--surface` and `--field` in its theme, and
 * `--line-strong` clears 3:1 on all three; `index.css.test.ts` recomputes both
 * from this file, so a value changed here is checked where it is changed.
 *
 * **Reading older comments in `styles/`.** Until 2026-09-17 these rules used
 * CSS system colours, and a comment written before then names the keyword its
 * rule used. Each became the token beside it:
 *
 *   Canvas → --bg                 CanvasText → --text
 *   ButtonFace → --surface        ButtonBorder → --line-strong
 *   GrayText → --muted            Highlight, LinkText → --accent
 *   HighlightText → --on-accent
 *
 * A contrast figure quoted in such a comment was measured against the system
 * colour, not the token. */
:root {
  color-scheme: light dark;

  --bg: light-dark(#f3eee3, #15130f);
  --surface: light-dark(#e9e2d2, #211e18);
  --field: light-dark(#fbf8f2, #1b1914);
  --line: light-dark(#d6ccb9, #35312a);
  --line-strong: light-dark(#897d68, #746c60);
  --text: light-dark(#231e17, #ece4d4);
  --muted: light-dark(#6b6151, #a69b87);
  --accent: light-dark(#b0361d, #e8715a);
  --on-accent: light-dark(#fbf8f2, #15130f);
  --danger: light-dark(#9e1b32, #f28b9a);
  --success: light-dark(#3f6b2f, #95c283);
  --warning: light-dark(#7d5500, #e2b55c);
}

:root[data-theme="paper"] {
  color-scheme: light;
}

:root[data-theme="ink"] {
  color-scheme: dark;
}
```

In `web/src/styles/base.css`, replace lines 1–6:

```css
:root {
  color-scheme: light dark;
  font:
    16px / 1.5 system-ui,
    sans-serif;
}
```

with:

```css
:root {
  font:
    16px / 1.5 system-ui,
    sans-serif;
}
```

and, in the same file, replace:

```css
body {
  margin: 0;
}
```

with:

```css
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
}
```

(`color-scheme` now lives in `tokens.css`, beside the `data-theme` rules that override it. The body takes `--text` here rather than in Task 4 because `AcceptForm.browser.test.tsx` compares the proposals panel's text colour with the body's: `.library` says `var(--text)` from this task on, and a body left at the browser's `CanvasText` would no longer match.)

- [ ] **Step 4: Replace every system-colour keyword outside comments**

Run from the repository root:

```bash
python3 - web/src/styles/annotations.css web/src/styles/shell.css web/src/styles/library.css web/src/styles/admin.css <<'EOF'
import pathlib, re, sys

# Spec §2's mapping. Longer names first, so `HighlightText` and `CanvasText`
# are never read as `Highlight` and `Canvas`.
MAP = [
    ('ButtonBorder', 'var(--line-strong)'),
    ('CanvasText', 'var(--text)'),
    ('ButtonFace', 'var(--surface)'),
    ('HighlightText', 'var(--on-accent)'),
    ('Highlight', 'var(--accent)'),
    ('Canvas', 'var(--bg)'),
    ('LinkText', 'var(--accent)'),
    ('GrayText', 'var(--muted)'),
]
total = 0
for name in sys.argv[1:]:
    path = pathlib.Path(name)
    # Odd indices are comments, kept as written.
    parts = re.split(r'(/\*.*?\*/)', path.read_text(), flags=re.S)
    for i in range(0, len(parts), 2):
        for word, token in MAP:
            parts[i], n = re.subn(rf'\b{word}\b', token, parts[i])
            total += n
    path.write_text(''.join(parts))
print(f'{total} replacements')
EOF
```

Expected: `79 replacements`. Any other number means the partials differ from what this plan read: stop.

- [ ] **Step 5: Correct the comments that now say the wrong thing**

Each edit below is an exact replacement.

`web/src/styles/shell.css` — replace:

```
 * `index.css` has no general-purpose design variables to reuse here — the
 * only custom properties above are the `--box-*` palette, and that palette is
 * documented as deliberately fixed against the artwork rather than following
 * `color-scheme`. This screen is UI chrome, not artwork, so it uses CSS
 * system colours instead (`Canvas`, `CanvasText`, `ButtonFace`,
 * `ButtonBorder`, `LinkText`, `Highlight`), which follow `color-scheme: light
 * dark` from `:root` the way a hardcoded palette would not.
```

with:

```
 * This screen is UI chrome, not artwork, so it draws from the theme's tokens
 * in `tokens.css` rather than the `--box-*` palette, which is fixed against
 * the artwork on purpose. The tokens follow `color-scheme` — the OS's, or the
 * reader's choice through `data-theme` — the way a hardcoded palette would
 * not. (Until 2026-09-17 this block used CSS system colours; `tokens.css`
 * maps each keyword to its token.)
```

`web/src/styles/library.css` — replace ` * or fail. System colours, not `--box-*`: this is chrome, not artwork. */` with ` * or fail. Tokens, not `--box-*`: this is chrome, not artwork. */`, and replace `/* §3's preset B. System colours, like every other rule in this block. */` with `/* §3's preset B. Tokens, like every other rule in this block. */`.

`web/src/styles/admin.css` — replace ` * System colours, like everything else below `.library`. */` with ` * Tokens, like everything else below `.library`. */`.

`web/src/styles/admin.css` — replace:

```
 * **System colours and no `opacity`.** Everything below `.library` is drawn
 * from `Canvas`/`CanvasText`/`ButtonFace`/`ButtonBorder` so it follows the
 * reader's colour scheme, and this panel adds no dimming: `index.css.test.ts`
```

with:

```
 * **Tokens and no `opacity`.** Everything below `.library` is drawn from
 * `--bg`/`--text`/`--surface`/`--line-strong` so it follows the reader's
 * colour scheme, and this panel adds no dimming: `index.css.test.ts`
```

and, a few lines below it, replace:

```
 * is `GrayText` on a disabled control, which is what that colour is for and
 * which WCAG exempts as an inactive component — the sentence beside such a
```

with:

```
 * is `--muted` (`GrayText` until 2026-09-17) on a disabled control, which is
 * what that colour is for and which WCAG exempts as an inactive component —
 * the sentence beside such a
```

`web/src/styles/admin.css` — replace `/* Spec C4's settings area. System colours throughout, per this file's own` with `/* Spec C4's settings area. Tokens throughout, per `tokens.css`'s own`; replace `/* Spec C4 §4's users screen and §5's backup screen. System colours, as the` with `/* Spec C4 §4's users screen and §5's backup screen. Tokens, as the`; and replace:

```
   overlay, drawn here inside `.library`'s `<main>`. System colours, per this
   file's rule. The panel's palette is fixed dark to sit over a page, and on a
   light `Canvas` it would draw the "Password changed" notice in pale green on
   white, so `Account` hands this placement no colour at all. */
```

with:

```
   overlay, drawn here inside `.library`'s `<main>`. Tokens, per
   `tokens.css`. The panel's palette is fixed dark to sit over a page, and on
   a paper `--bg` it would draw the "Password changed" notice in pale green on
   paper, so `Account` hands this placement no colour at all. */
```

`web/src/Shell.tsx` — replace:

```
 * System colours throughout — see `index.css`'s `.library` block, and
 * `Login.tsx`, which shipped a hardcoded `#111` on this app's front door.
```

with:

```
 * The theme's tokens throughout — see `styles/tokens.css`, and `Login.tsx`,
 * which once shipped a hardcoded `#111` on this app's front door.
```

`web/src/Chrome.tsx` — replace:

```
// screen's `Canvas`/`ButtonFace` system colours (`index.css`'s `.chips
// button`), which follow OS light/dark and would put a light-mode button on
```

with:

```
// screen's `--bg`/`--surface` tokens (`styles/library.css`'s `.chips
// button`), which follow the theme and would put a paper button on
```

`web/src/Account.tsx` — replace:

```
 * which follows the viewer's colour scheme, and on a light `Canvas` the
 * panel's `#a8ffb4` notice is pale green on white. `index.css`'s `.account`
 * block draws the page in system colours instead. The layout-only inline
```

with:

```
 * which follows the viewer's colour scheme, and on a paper `--bg` the
 * panel's `#a8ffb4` notice is pale green on paper. `styles/admin.css`'s
 * `.account` block draws the page in the theme's tokens instead. The
 * layout-only inline
```

- [ ] **Step 6: Run the contrast test**

```bash
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: PASS, 14 tests.

- [ ] **Step 7: Watch each check fail against its own defect**

Apply each mutation alone, run the command from Step 6, confirm the named failure, and undo it before the next.

1. `styles/tokens.css`: paper `--muted` `#6b6151` → `#8a806f`. Expected: "every text colour clears 4.5:1 … in paper" fails listing `--muted on --surface`, and the token-model case fails.
2. `styles/library.css`: in `.alignment-panel .paired`, `opacity: 0.9` → `opacity: 0.3`. Expected: both D2 cases fail.
3. `styles/admin.css`: in `.proposal-detail .accept:disabled`, `background: var(--bg)` → `background: var(--accent)`. Expected: both "clears 4.5:1" Accept cases fail.
4. `index.css.test.ts`: in `TOKENS`, `{ paper: hex(m[2]), ink: hex(m[3]) }` → `{ paper: hex(m[3]), ink: hex(m[2]) }`. Expected: the token-model case fails.

Then confirm every mutation is undone:

```bash
git diff --stat
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: the diff lists only the files this task changes, and the test passes with 14.

- [ ] **Step 8: Confirm no system colour survives outside a comment**

```bash
python3 - web/src/styles/*.css <<'EOF'
import re, sys
kw = re.compile(r'\b(Canvas|CanvasText|ButtonFace|ButtonText|ButtonBorder|GrayText|Highlight|HighlightText|LinkText|Field|FieldText)\b')
for name in sys.argv[1:]:
    code = re.sub(r'/\*.*?\*/', '', open(name).read(), flags=re.S)
    print(name, len(kw.findall(code)))
EOF
```

Expected: `0` for all six files.

- [ ] **Step 9: Point the browser project's palette check at the tokens**

In `web/src/AcceptForm.browser.test.tsx`, after `import './index.css'` (line 5), add:

```tsx
import tokensSource from './styles/tokens.css?raw'
```

Replace the doc-comment paragraph:

```
 * **Both colour schemes, set through `color-scheme` on the root.** Everything
 * below `.library` is drawn from CSS system colours, which resolve against
 * that property — so overriding it is how a real Chromium is asked for the
 * dark palette without a second browser instance.
```

with:

```
 * **Both colour schemes, set through `color-scheme` on the root.** Everything
 * below `.library` is drawn from the tokens in `styles/tokens.css`, each a
 * `light-dark()` pair that resolves against that property — so overriding it
 * is how a real Chromium is asked for the ink palette without a second browser
 * instance.
```

Replace:

```
    // the button, at the same full-contrast `CanvasText` as everything else
```

with:

```
    // the button, at the same full-contrast `--text` as everything else
```

Replace:

```
      // `CanvasText` resolves to real pixels here and in `jsdom` it does not
      // resolve at all, which is why this lives in this project.
```

with:

```
      // `var(--text)` resolves to real pixels here and in `jsdom` it does not
      // resolve at all, which is why this lives in this project.
```

Replace the whole of the file's last case — the one named *paints the system colours `index.css.test.ts` computes its ratios from*, lines 360–396 — up to and including its closing `  })`, just above the file's final `})`, with:

```tsx
  it('paints the tokens `index.css.test.ts` computes its ratios from', async () => {
    // That file cannot render, so it parses `tokens.css`'s `light-dark()` pairs
    // and recomputes WCAG's math from them, assuming paper under `light` and
    // ink under `dark`. This is the check that a real Chromium agrees: every
    // pair, resolved in each scheme, paints the half the model took. A later
    // rule that redefined a token, or a pair written the wrong way round,
    // fails here while the model goes on passing.
    const pairs = [
      ...tokensSource.matchAll(
        /--([a-z-]+):\s*light-dark\(\s*(#[0-9a-fA-F]{6})\s*,\s*(#[0-9a-fA-F]{6})\s*\)/g,
      ),
    ]
    // Exactly spec §2's twelve, so an unread or empty file fails here rather
    // than passing a loop that ran no iterations.
    expect(pairs).toHaveLength(12)
    const rgb = (hex: string) => {
      const n = Number.parseInt(hex.slice(1), 16)
      return `rgb(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255})`
    }

    for (const scheme of ['light', 'dark'] as const) {
      document.documentElement.style.colorScheme = scheme
      const probe = document.createElement('span')
      document.body.append(probe)
      try {
        for (const [, name, paper, ink] of pairs) {
          probe.style.color = `var(--${name})`
          expect(`${scheme} --${name}: ${getComputedStyle(probe).color}`).toBe(
            `${scheme} --${name}: ${rgb(scheme === 'light' ? paper : ink)}`,
          )
        }
      } finally {
        probe.remove()
      }
    }
  })
```

- [ ] **Step 10: Run the browser project, and watch the new check fail**

```bash
pnpm -C web exec vitest run --project browser --reporter=default
```

Expected: 5 files, 28 tests, all passed — the same count as the baseline, since one case was replaced by one.

Then apply each alone, run the same command, confirm, undo:

1. Append to `web/src/styles/admin.css` (imported last): `:root { --muted: #ff0000; }`. Expected: "paints the tokens …" FAILS naming `--muted`, in both schemes; the node project's contrast test would still pass, which is what this check adds.
2. `web/src/styles/tokens.css`: `--warning: light-dark(#7d5500, #e2b55c);` → `--warning: #7d5500;`. Expected: FAILS on `toHaveLength(12)` with 11.

If Chromium is not installed (`pnpm -C web exec playwright install chromium` fixes that), say so in the report rather than skipping the step.

- [ ] **Step 11: Lint, typecheck, and the whole node project**

```bash
pnpm -C web run lint
pnpm -C web run typecheck
pnpm -C web exec vitest run --project node
```

Expected: all three exit 0.

- [ ] **Step 12: Commit**

```bash
git add web/src/styles web/src/index.css.test.ts web/src/AcceptForm.browser.test.tsx web/src/Shell.tsx web/src/Chrome.tsx web/src/Account.tsx
git commit -m "Style pass 1: colour tokens replace every system colour"
```

---

### Task 3: A guard that keeps colour in `tokens.css`

**Files:**
- Create: `web/src/colourLiterals.test.ts`

**Interfaces:**
- Consumes: Task 2's partials (no system colour outside comments; `annotations.css` still holds two artwork literals).
- Produces: `ALLOWED`, a per-file allowance later tasks and PRs lower and delete entries from. Its starting value is below.

- [ ] **Step 1: Write the guard with an empty allowance**

Create `web/src/colourLiterals.test.ts`:

```ts
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
 * would count. None exists here today.
 */
const SOURCES = import.meta.glob<string>(
  ['./**/*.{ts,tsx,css}', '!./**/*.test.{ts,tsx}', '!./testing/**'],
  { query: '?raw', import: 'default', eager: true },
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
      count(code, HEX) + count(code, COLOUR_FUNCTION) + count(code, SYSTEM_COLOUR)
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
```

- [ ] **Step 2: Run it and read the counts it finds**

```bash
pnpm -C web exec vitest run --project node src/colourLiterals.test.ts --reporter=default
```

Expected: the first two cases PASS; the third FAILS, and its diff lists exactly these thirteen entries:

```
./Account.tsx 10          ./Help.tsx 4
./Annotations.tsx 13      ./Login.tsx 9
./Borrowed.tsx 5          ./NoteList.tsx 5
./Chrome.tsx 11           ./NotePanel.tsx 6
./CorrectionBanner.tsx 2  ./Settings.tsx 5
./CreditsPanel.tsx 5      ./styles/annotations.css 2
./Filmstrip.tsx 4
```

If the diff shows any other file or number, stop and report it: either the tree changed or the counter differs from the one these figures came from.

- [ ] **Step 3: Set the allowance**

Replace `const ALLOWED: Record<string, number> = {}` with:

```ts
const ALLOWED: Record<string, number> = {
  './Account.tsx': 10,
  './Annotations.tsx': 13,
  './Borrowed.tsx': 5,
  './Chrome.tsx': 11,
  './CorrectionBanner.tsx': 2,
  './CreditsPanel.tsx': 5,
  './Filmstrip.tsx': 4,
  './Help.tsx': 4,
  './Login.tsx': 9,
  './NoteList.tsx': 5,
  './NotePanel.tsx': 6,
  './Settings.tsx': 5,
  './styles/annotations.css': 2,
}
```

- [ ] **Step 4: Run it**

```bash
pnpm -C web exec vitest run --project node src/colourLiterals.test.ts --reporter=default
```

Expected: PASS, 3 tests.

- [ ] **Step 5: Watch it fail in both directions, and not on a comment**

Apply each alone, run Step 4's command, confirm, undo:

1. `web/src/Borrowed.tsx`: add a last line `export const probe = '#ff0000'`. Expected: FAIL, the diff shows `./Borrowed.tsx` at 6 against 5.
2. `colourLiterals.test.ts`: `'./Chrome.tsx': 11` → `12`. Expected: FAIL, the diff shows 11 against 12.
3. `web/src/Borrowed.tsx`: add a last line `// #123456 rgba(0, 0, 0, 1) 'Canvas'`. Expected: PASS — a comment counts for nothing.

Then:

```bash
git diff --stat
```

Expected: only `web/src/colourLiterals.test.ts`, untracked.

- [ ] **Step 6: Lint and typecheck**

```bash
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: both exit 0.

- [ ] **Step 7: Commit**

```bash
git add web/src/colourLiterals.test.ts
git commit -m "Style pass 1: a guard that keeps colour in tokens.css"
```

---

### Task 4: Fonts, type and spacing tokens, and element defaults

**Files:**
- Modify: `web/package.json`, `web/pnpm-lock.yaml` (through `pnpm add`), `web/src/styles/tokens.css`, `web/src/styles/base.css` (whole file)
- Create: `web/src/styles/base.css.test.ts`

**Interfaces:**
- Consumes: Task 2's colour tokens.
- Produces: `--font-title`, `--font-ui`, `--font-figures`, `--text-h1` … `--text-meta`, `--space-1` … `--space-7`, `--radius-control`; element defaults under `:where(.library, .login)`. Task 6 and Task 7 rely on `.library` and `.login` receiving them.

- [ ] **Step 1: Add the fonts**

```bash
pnpm -C web add @fontsource/shippori-mincho-b1@^5.3.0 @fontsource/zen-kaku-gothic-new@^5.3.0
```

Expected: both appear under `dependencies` in `web/package.json` (they ship in the bundle), and `pnpm-lock.yaml` changes.

- [ ] **Step 2: Write the scope test**

Create `web/src/styles/base.css.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import baseSource from './base.css?raw'

/**
 * `base.css` restyles elements only inside the screens the style pass has
 * reached — `.library` and `.login` — plus the document-wide rules the page
 * cannot work without. A bare `button` rule would reach the reader, whose
 * inline-styled chrome sets some properties and leaves the rest to the
 * browser, so a border or a padding set here would move geometry that the
 * pass's reader PR redraws on its own terms (spec 2026-09-17 §5).
 */
const DOCUMENT_WIDE = [':root', 'html, body, #root', 'body', '::selection']
const SCOPE = ':where(.library, .login) '

function preludes(css: string): string[] {
  const code = css.replace(/\/\*[\s\S]*?\*\//g, '')
  return [...code.matchAll(/([^{}]+)\{/g)]
    .map((m) => m[1].replace(/\s+/g, ' ').trim())
    .filter((prelude) => !prelude.startsWith('@'))
}

describe('base.css', () => {
  it('reads its rules at all', () => {
    expect(preludes(baseSource)).toContain('body')
  })

  it('styles no element outside the screens the style pass has reached', () => {
    const stray = preludes(baseSource).filter(
      (prelude) =>
        !DOCUMENT_WIDE.includes(prelude) && !prelude.startsWith(SCOPE),
    )
    expect(stray).toEqual([])
  })
})
```

- [ ] **Step 3: Run it against today's `base.css`**

```bash
pnpm -C web exec vitest run --project node src/styles/base.css.test.ts --reporter=default
```

Expected: PASS, 2 tests — today's `base.css` holds only `:root`, the height chain and `body`, all document-wide. The step that proves the scope test can fail is Step 7.

- [ ] **Step 4: Add the font imports, the figures face, and type and spacing tokens**

In `web/src/styles/tokens.css`, directly after the header comment Task 2 added and before its `:root {`, insert:

```css
@import "@fontsource/shippori-mincho-b1/600.css";
@import "@fontsource/shippori-mincho-b1/700.css";
@import "@fontsource/zen-kaku-gothic-new/400.css";
@import "@fontsource/zen-kaku-gothic-new/500.css";
@import "@fontsource/zen-kaku-gothic-new/700.css";

/* Tabular digits for tables. Zen Kaku Gothic New's digits are proportional —
 * its Latin subset advances them 383 to 509 units and has no `tnum` feature,
 * read with fontTools on 2026-09-17 — so a column of page counts would jitter.
 * This face covers the ten digits only, from whichever tabular system sans the
 * device has; every other character falls through to `--font-ui`. */
@font-face {
  font-family: "taiyaku figures";
  src:
    local("Helvetica Neue"), local("Roboto"), local("Noto Sans"),
    local("DejaVu Sans"), local("Arial");
  unicode-range: U+0030-0039;
}
```

In the same file, inside the first `:root` block, after `--warning: light-dark(#7d5500, #e2b55c);`, add:

```css

  --font-title: "Shippori Mincho B1", serif;
  --font-ui: "Zen Kaku Gothic New", system-ui, sans-serif;
  --font-figures: "taiyaku figures", var(--font-ui);

  --text-h1: 2rem;
  --text-h2: 1.5rem;
  --text-h3: 1.25rem;
  --text-body: 1rem;
  --text-ui: 0.875rem;
  --text-meta: 0.75rem;

  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;
  --space-7: 48px;

  --radius-control: 2px;
```

- [ ] **Step 5: Write the element defaults**

Replace the whole of `web/src/styles/base.css` with:

```css
/* Element defaults — spec 2026-09-17 §2 and §5 ①. */
:root {
  font: 16px / 1.5 var(--font-ui);
}

/*
 * `PageView`'s `height: 100%` chain (and the `<img>`'s `max-height: 100%`)
 * only constrains anything if every ancestor up to the viewport has a
 * definite height. Without this, `html`/`body`/`#root` are auto-height —
 * sized to their content, i.e. to the image — so `max-height: 100%` resolves
 * against an undefined height and is ignored: a tall page overflows the
 * viewport instead of being downscaled to fit it.
 */
html,
body,
#root {
  height: 100%;
}

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
}

::selection {
  background: var(--accent);
  color: var(--on-accent);
}

/* **Everything below is scoped to the screens the style pass has reached**,
 * not the whole document. The reader draws its chrome over artwork with
 * inline styles that set some properties and leave the rest to the browser,
 * so a document-wide `button` rule would change a filmstrip tile's border or
 * a panel button's padding under geometry that the reader's own PR redraws.
 * `:where()` on both halves keeps every rule here at zero specificity, so
 * each class rule in `shell.css`, `library.css` and `admin.css` still wins
 * exactly as it did. `base.css.test.ts` holds the scope. */
:where(.library, .login) :where(h1, h2, h3) {
  font-family: var(--font-title);
  font-weight: 600;
  line-height: 1.25;
}

:where(.library, .login) :where(h1) {
  font-size: var(--text-h1);
}

:where(.library, .login) :where(h2) {
  font-size: var(--text-h2);
}

:where(.library, .login) :where(h3) {
  font-size: var(--text-h3);
}

:where(.library, .login) :where(a) {
  color: var(--accent);
  text-underline-offset: 3px;
}

:where(.library, .login) :where(button, input, select, textarea) {
  font: inherit;
  color: inherit;
}

:where(.library, .login)
  :where(
    button,
    select,
    textarea,
    input:not([type="checkbox"], [type="radio"], [type="range"])
  ) {
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
}

:where(.library, .login) :where(button, select) {
  background: var(--surface);
}

:where(.library, .login) :where(input, textarea) {
  background: var(--field);
}

:where(.library, .login) :where(button) {
  padding: var(--space-1) var(--space-3);
  cursor: pointer;
}

:where(.library, .login) :where(button:disabled) {
  cursor: default;
}

:where(.library, .login)
  :where(
    input[type="checkbox"],
    input[type="radio"],
    input[type="range"],
    progress
  ) {
  accent-color: var(--accent);
}

:where(.library, .login) :where(table) {
  border-collapse: collapse;
  font-family: var(--font-figures);
  font-variant-numeric: tabular-nums;
}

:where(.library, .login) :where(th) {
  text-align: left;
  font-weight: 500;
  color: var(--muted);
}

:where(.library, .login) :where(:focus-visible) {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
```

Then let Biome settle the formatting of both files:

```bash
pnpm -C web exec biome format --write src/styles/base.css src/styles/tokens.css
```

- [ ] **Step 6: Run the scope test, the contrast test and the guard**

```bash
pnpm -C web exec vitest run --project node src/styles/base.css.test.ts src/index.css.test.ts src/colourLiterals.test.ts --reporter=default
```

Expected: PASS — 2, 14 and 3 tests. (If Biome wrapped a selector across lines, the scope test still passes: it collapses whitespace before comparing.)

- [ ] **Step 7: Watch the scope test fail**

Append to `web/src/styles/base.css`:

```css
button {
  cursor: pointer;
}
```

Run Step 6's command. Expected: "styles no element outside the screens the style pass has reached" FAILS with `stray` equal to `["button"]`. Remove the rule and run again: PASS.

- [ ] **Step 8: Build, to prove the font imports resolve**

```bash
pnpm -C web exec vite build --outDir ../target/style-pass/dist-task4 --emptyOutDir
ls target/style-pass/dist-task4/assets | command grep -c 'woff2$'
```

Expected: the build succeeds, and the count is greater than 0.

- [ ] **Step 9: Lint, typecheck, and the node project**

```bash
pnpm -C web run lint
pnpm -C web run typecheck
pnpm -C web exec vitest run --project node
pnpm -C web install --frozen-lockfile --lockfile-only
pnpm -C web exec vitest run --project browser --reporter=default
```

Expected: all five exit 0 (the fourth is `check-all.sh`'s lockfile check), and the browser project reports 28 passed.

**The browser project is the first place the fonts meet a layout assertion.** `AcceptForm.browser.test.tsx` asserts which fields share a row at 700 and 1100 px, and `Proposals.browser.test.tsx` how a row reflows at 700 px, and both now render in Zen Kaku Gothic New. If one of them fails, do not edit the assertion and do not tune the CSS to fit it: record the assertion, the widths the failure message gives, and the same test's result with the `@import "@fontsource/…"` lines temporarily removed, then report `DONE_WITH_CONCERNS` with that evidence and leave the decision to the controller.

- [ ] **Step 10: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/styles
git commit -m "Style pass 1: fonts, type and spacing tokens, scoped element defaults"
```

---

### Task 5: The theme preference, applied before first paint

**Files:**
- Create: `web/src/theme.ts`, `web/src/theme.test.ts`
- Modify: `web/index.html`, `web/src/main.tsx`, `web/src/label.tsx`, `web/src/prefs.ts:21-25`

**Interfaces:**
- Consumes: `loadPrefs`, `savePrefs`, `NAMESPACE` from `web/src/prefs.ts`; `fakeStorage` from `web/src/testing/storage.ts`.
- Produces:
  - `export type ThemeChoice = 'system' | 'paper' | 'ink'`
  - `export const THEME_CHOICES: readonly ThemeChoice[]` — `['system', 'paper', 'ink']`
  - `export const THEME_PREF_ID = 'theme'`
  - `export function loadTheme(): ThemeChoice`
  - `export function saveTheme(choice: ThemeChoice): void`
  - `export function applyTheme(choice: ThemeChoice, root: Pick<Element, 'setAttribute' | 'removeAttribute'>): void`

- [ ] **Step 1: Write the tests**

Create `web/src/theme.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import indexHtml from '../index.html?raw'
import { NAMESPACE } from './prefs'
import tokensCss from './styles/tokens.css?raw'
import { fakeStorage } from './testing/storage'
import { applyTheme, loadTheme, saveTheme, THEME_PREF_ID } from './theme'

/** An element that records only what `applyTheme` writes. */
function fakeRoot() {
  const attributes = new Map<string, string>()
  return {
    attributes,
    setAttribute: (name: string, value: string) => {
      attributes.set(name, value)
    },
    removeAttribute: (name: string) => {
      attributes.delete(name)
    },
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('theme', () => {
  it('follows the OS until a reader chooses', () => {
    vi.stubGlobal('localStorage', fakeStorage())
    expect(loadTheme()).toBe('system')
  })

  it('remembers a choice under taiyaku.prefs.theme', () => {
    const storage = fakeStorage()
    vi.stubGlobal('localStorage', storage)
    saveTheme('ink')
    expect(storage.getItem(`${NAMESPACE}.${THEME_PREF_ID}`)).toBe(
      '{"choice":"ink"}',
    )
    expect(loadTheme()).toBe('ink')
  })

  it('reads a stored choice it does not know as the OS default', () => {
    const storage = fakeStorage()
    storage.setItem('taiyaku.prefs.theme', '{"choice":"sepia"}')
    vi.stubGlobal('localStorage', storage)
    expect(loadTheme()).toBe('system')
  })

  it('pins paper and ink as data-theme, and leaves system to the OS', () => {
    const root = fakeRoot()
    applyTheme('ink', root)
    expect(root.attributes.get('data-theme')).toBe('ink')
    applyTheme('paper', root)
    expect(root.attributes.get('data-theme')).toBe('paper')
    applyTheme('system', root)
    expect(root.attributes.has('data-theme')).toBe(false)
  })
})

describe("index.html's pre-paint script", () => {
  const script = indexHtml.match(/<script>([\s\S]*?)<\/script>/)?.[1]

  const STORED: Array<[string, string | null]> = [
    ['nothing', null],
    ['paper', '{"choice":"paper"}'],
    ['ink', '{"choice":"ink"}'],
    ['system', '{"choice":"system"}'],
    ['a choice it does not know', '{"choice":"sepia"}'],
    ['a value that is not JSON', '{not json'],
    ['JSON that is not an object', '"ink"'],
  ]

  it.each(STORED)(
    'agrees with loadTheme and applyTheme when %s is stored',
    (_, stored) => {
      if (script === undefined) throw new Error('index.html has no <script>')
      const storage = fakeStorage()
      if (stored !== null) storage.setItem('taiyaku.prefs.theme', stored)

      const painted = fakeRoot()
      new Function('localStorage', 'document', script)(storage, {
        documentElement: painted,
      })

      vi.stubGlobal('localStorage', storage)
      const applied = fakeRoot()
      applyTheme(loadTheme(), applied)

      expect(painted.attributes).toEqual(applied.attributes)
    },
  )

  it('comes before the app module, so it runs before first paint', () => {
    const inline = indexHtml.indexOf('<script>')
    expect(inline).toBeGreaterThan(-1)
    expect(inline).toBeLessThan(indexHtml.indexOf('<script type="module"'))
  })
})

describe("index.html's theme-color", () => {
  it("is each theme's --bg", () => {
    const bg = tokensCss.match(
      /--bg:\s*light-dark\((#[0-9a-f]{6}),\s*(#[0-9a-f]{6})\)/,
    )
    const meta = (scheme: string) =>
      indexHtml.match(
        new RegExp(
          `<meta name="theme-color" content="(#[0-9a-f]{6})" media="\\(prefers-color-scheme: ${scheme}\\)"`,
        ),
      )?.[1]
    expect(bg).not.toBeNull()
    expect(meta('light')).toBe(bg?.[1])
    expect(meta('dark')).toBe(bg?.[2])
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project node src/theme.test.ts --reporter=default
```

Expected: FAIL — the file cannot resolve `./theme`.

- [ ] **Step 3: Write `theme.ts`**

Create `web/src/theme.ts`:

```ts
/**
 * The reader's theme: follow the OS, or pin paper or ink (spec 2026-09-17 §3).
 *
 * Stored per device through `prefs.ts`, like every preference about the person
 * rather than the work, and applied as `data-theme` on `<html>`, which
 * `styles/tokens.css` turns into `color-scheme` — and every colour token is a
 * `light-dark()` pair that `color-scheme` picks a half of.
 *
 * **`index.html` reads the same key before first paint**, in an inline script
 * that cannot import this module. `theme.test.ts` runs that script against
 * `loadTheme` and `applyTheme` for every stored shape, so the two cannot drift
 * apart in silence.
 */
import { loadPrefs, savePrefs } from './prefs'

export type ThemeChoice = 'system' | 'paper' | 'ink'

/** In the order the control offers them. */
export const THEME_CHOICES: readonly ThemeChoice[] = ['system', 'paper', 'ink']

/** The `prefs.ts` id, and so the `localStorage` key `taiyaku.prefs.theme`. */
export const THEME_PREF_ID = 'theme'

function isThemeChoice(value: unknown): value is ThemeChoice {
  return THEME_CHOICES.includes(value as ThemeChoice)
}

export function loadTheme(): ThemeChoice {
  const { choice } = loadPrefs<{ choice: unknown }>(THEME_PREF_ID, {
    choice: 'system',
  })
  return isThemeChoice(choice) ? choice : 'system'
}

export function saveTheme(choice: ThemeChoice): void {
  savePrefs(THEME_PREF_ID, { choice })
}

/** `system` removes the attribute rather than writing one, so the OS decides. */
export function applyTheme(
  choice: ThemeChoice,
  root: Pick<Element, 'setAttribute' | 'removeAttribute'>,
): void {
  if (choice === 'system') {
    root.removeAttribute('data-theme')
  } else {
    root.setAttribute('data-theme', choice)
  }
}
```

- [ ] **Step 4: Add the pre-paint script and the theme colours to `index.html`**

Replace the whole of `web/index.html` with:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="theme-color" content="#f3eee3" media="(prefers-color-scheme: light)" />
    <meta name="theme-color" content="#15130f" media="(prefers-color-scheme: dark)" />
    <title>taiyaku reader</title>
    <!--
      The stored theme, before first paint (spec 2026-09-17 §3): a reader who
      picked Ink on a light-mode OS would otherwise see paper while the modules
      load. It mirrors theme.ts's loadTheme and applyTheme, and theme.test.ts
      runs it against them. Being inline, it is the one thing here a strict
      Content-Security-Policy would stop; main.tsx applies the theme again, so
      what that would cost is the flash, not the theme. The theme-color pair
      above follows the OS, not this choice, and theme.test.ts holds each to
      its theme's --bg.
    -->
    <script>
      try {
        var choice = JSON.parse(localStorage.getItem('taiyaku.prefs.theme')).choice
        if (choice === 'paper' || choice === 'ink') {
          document.documentElement.setAttribute('data-theme', choice)
        }
      } catch (_) {}
    </script>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 5: Run the tests**

```bash
pnpm -C web exec vitest run --project node src/theme.test.ts --reporter=default
```

Expected: PASS, 13 tests (4 + 7 + 1 + 1).

- [ ] **Step 6: Watch them fail against their own defects**

Apply each alone, run Step 5's command, confirm, undo:

1. `web/index.html`: `'taiyaku.prefs.theme'` → `'taiyaku.prefs.themes'`. Expected: the `paper` and `ink` agreement cases FAIL.
2. `web/src/theme.ts`: in `applyTheme`, `root.removeAttribute('data-theme')` → `root.setAttribute('data-theme', choice)`. Expected: "pins paper and ink …" and the `nothing`, `system` and three unknown-value agreement cases FAIL.
3. `web/index.html`: swap `#f3eee3` and `#15130f` between the two `theme-color` tags. Expected: "is each theme's --bg" FAILS.

- [ ] **Step 7: Apply the theme from the entry points too**

In `web/src/main.tsx`, replace:

```tsx
import './index.css'
import { Gate } from './Gate.tsx'
import { AppRoutes } from './Routes.tsx'
```

with:

```tsx
import './index.css'
import { Gate } from './Gate.tsx'
import { AppRoutes } from './Routes.tsx'
import { applyTheme, loadTheme } from './theme.ts'

// Again here, after `index.html`'s pre-paint script has done it: a
// Content-Security-Policy that refuses inline scripts would stop that one and
// not this, and a flash of the OS's theme is a better failure than the wrong
// theme for the whole session.
applyTheme(loadTheme(), document.documentElement)
```

In `web/src/label.tsx`, replace:

```tsx
import './index.css'
import { Labeller } from './Labeller.tsx'
```

with:

```tsx
import './index.css'
import { Labeller } from './Labeller.tsx'
import { applyTheme, loadTheme } from './theme.ts'
```

and, directly above `createRoot(document.getElementById('root')!).render(`, add:

```tsx
// The labeller shares the tokens but has no pre-paint script of its own, so it
// takes the stored theme here, before anything renders.
applyTheme(loadTheme(), document.documentElement)

```

In `web/src/prefs.ts`, replace:

```ts
 * **One namespace, one writer.** This module is the only thing in `web/src`
 * that touches `localStorage` at all — checked rather than assumed, and worth
 * keeping true: a value stored anywhere else would be the first one whose key
 * nobody thought about, on an origin this reader may share with whatever else
 * is served from it.
```

with:

```ts
 * **One namespace, one writer.** This module is the only thing in `web/src`
 * that touches `localStorage` at all — checked rather than assumed, and worth
 * keeping true: a value stored anywhere else would be the first one whose key
 * nobody thought about, on an origin this reader may share with whatever else
 * is served from it. One *reader* sits outside `web/src`: `index.html`'s
 * pre-paint script reads `taiyaku.prefs.theme`, which `theme.ts` writes
 * through this module and `theme.test.ts` holds the script to.
```

- [ ] **Step 8: Run the node and jsdom projects, lint and typecheck**

```bash
pnpm -C web exec vitest run --project node
pnpm -C web exec vitest run --project jsdom src/main.test.tsx
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: all exit 0. `main.test.tsx` imports `main.tsx`, so it now runs `applyTheme` in jsdom.

- [ ] **Step 9: Commit**

```bash
git add web/index.html web/src/theme.ts web/src/theme.test.ts web/src/main.tsx web/src/label.tsx web/src/prefs.ts
git commit -m "Style pass 1: the theme preference, applied before first paint"
```

---

### Task 6: The System / Paper / Ink control in the sidebar

**Files:**
- Create: `web/src/ThemeControl.tsx`, `web/src/ThemeControl.test.tsx`
- Modify: `web/src/Shell.tsx` (import and render), `web/src/Shell.test.tsx` (one case), `web/src/styles/shell.css` (append)

**Interfaces:**
- Consumes: `THEME_CHOICES`, `ThemeChoice`, `loadTheme`, `saveTheme`, `applyTheme` from Task 5.
- Produces: `export function ThemeControl()`, a component with no props, rendering `<fieldset class="theme-control">` with a `legend.sidebar-heading` "Theme" and three `label > input[type=radio][name=theme]`, labelled System, Paper, Ink. Shell renders it as the last child of `.sidebar-column`.

- [ ] **Step 1: Write the component tests**

Create `web/src/ThemeControl.test.tsx`:

```tsx
import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from './testing/mount'
import { fakeStorage } from './testing/storage'
import { ThemeControl } from './ThemeControl'

function radio(host: HTMLElement, label: string): HTMLInputElement {
  const row = [...host.querySelectorAll('.theme-control label')].find(
    (l) => l.textContent?.trim() === label,
  )
  const input = row?.querySelector('input[type="radio"]')
  if (!(input instanceof HTMLInputElement)) {
    throw new Error(`no radio labelled "${label}"`)
  }
  return input
}

describe('ThemeControl', () => {
  let storage: Storage

  beforeEach(() => {
    storage = fakeStorage()
    vi.stubGlobal('localStorage', storage)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    document.documentElement.removeAttribute('data-theme')
  })

  it('offers System, Paper and Ink, with System chosen until a reader picks', () => {
    const host = mount(<ThemeControl />)
    expect(
      [...host.querySelectorAll('.theme-control label')].map((l) =>
        l.textContent?.trim(),
      ),
    ).toEqual(['System', 'Paper', 'Ink'])
    expect(radio(host, 'System').checked).toBe(true)
  })

  it('starts from the stored choice', () => {
    storage.setItem('taiyaku.prefs.theme', '{"choice":"paper"}')
    const host = mount(<ThemeControl />)
    expect(radio(host, 'Paper').checked).toBe(true)
  })

  it('applies and remembers a choice the moment it is made', () => {
    const host = mount(<ThemeControl />)
    act(() => {
      radio(host, 'Ink').click()
    })
    expect(document.documentElement.getAttribute('data-theme')).toBe('ink')
    expect(storage.getItem('taiyaku.prefs.theme')).toBe('{"choice":"ink"}')
    expect(radio(host, 'Ink').checked).toBe(true)
  })

  it('hands the choice back to the OS when System is picked again', () => {
    storage.setItem('taiyaku.prefs.theme', '{"choice":"ink"}')
    document.documentElement.setAttribute('data-theme', 'ink')
    const host = mount(<ThemeControl />)
    act(() => {
      radio(host, 'System').click()
    })
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
    expect(storage.getItem('taiyaku.prefs.theme')).toBe('{"choice":"system"}')
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom src/ThemeControl.test.tsx --reporter=default
```

Expected: FAIL — `./ThemeControl` cannot be resolved.

- [ ] **Step 3: Write the component**

Create `web/src/ThemeControl.tsx`:

```tsx
/**
 * The theme switch at the foot of the sidebar: System, Paper or Ink
 * (spec 2026-09-17 §3).
 *
 * **A choice takes effect the moment it is made** — stored, then applied to
 * `<html>` — with nothing to confirm, because a theme is cheap to change back
 * and a preview that needed a Save would only be a second place for the two to
 * disagree. The pre-paint script in `index.html` applies the stored choice on
 * the next load, before this component exists.
 *
 * Native radios in a `fieldset`, so the group is one tab stop with arrow keys
 * between its three choices and a screen reader names it by its legend;
 * `shell.css` draws them as one segmented row.
 */
import { useState } from 'react'
import {
  applyTheme,
  loadTheme,
  saveTheme,
  THEME_CHOICES,
  type ThemeChoice,
} from './theme'

const LABELS: Record<ThemeChoice, string> = {
  system: 'System',
  paper: 'Paper',
  ink: 'Ink',
}

export function ThemeControl() {
  const [choice, setChoice] = useState<ThemeChoice>(loadTheme)

  const choose = (next: ThemeChoice) => {
    setChoice(next)
    saveTheme(next)
    applyTheme(next, document.documentElement)
  }

  return (
    <fieldset className="theme-control">
      <legend className="sidebar-heading">Theme</legend>
      <div className="segments">
        {THEME_CHOICES.map((option) => (
          <label key={option}>
            <input
              type="radio"
              name="theme"
              value={option}
              checked={choice === option}
              onChange={() => {
                choose(option)
              }}
            />
            {LABELS[option]}
          </label>
        ))}
      </div>
    </fieldset>
  )
}
```

- [ ] **Step 4: Run the component tests**

```bash
pnpm -C web exec vitest run --project jsdom src/ThemeControl.test.tsx --reporter=default
```

Expected: PASS, 4 tests.

- [ ] **Step 5: Watch them fail against their own defects**

Apply each alone, run Step 4's command, confirm, undo:

1. Delete the `saveTheme(next)` line. Expected: "applies and remembers …" and "hands the choice back …" FAIL on the stored value.
2. Delete the `applyTheme(next, document.documentElement)` line. Expected: the same two FAIL on `data-theme`.
3. `useState<ThemeChoice>(loadTheme)` → `useState<ThemeChoice>('system')`. Expected: "starts from the stored choice" FAILS.

- [ ] **Step 6: Put it in the sidebar**

In `web/src/Shell.test.tsx`, directly after the `it('fetches the status once and offers it to what it renders', …)` case, add:

```tsx
  it('offers the theme switch at the foot of the sidebar', async () => {
    vi.stubGlobal('localStorage', fakeStorage())
    const host = await settle(<Shell>{null}</Shell>)
    const column = host.querySelector('.sidebar-column')
    expect(column?.lastElementChild?.classList.contains('theme-control')).toBe(
      true,
    )
    expect(
      column?.querySelectorAll('.theme-control input[type="radio"]'),
    ).toHaveLength(3)
  })
```

Run it:

```bash
pnpm -C web exec vitest run --project jsdom src/Shell.test.tsx --reporter=default
```

Expected: the new case FAILS (`lastElementChild` is `.browse-settings`); every other case passes.

In `web/src/Shell.tsx`, add the import after `import { Sidebar } from './Sidebar'`:

```tsx
import { ThemeControl } from './ThemeControl'
```

and replace:

```tsx
              <BrowseSettings prefs={prefs} onChange={change} onReset={reset} />
            </div>
```

with:

```tsx
              <BrowseSettings prefs={prefs} onChange={change} onReset={reset} />
              <ThemeControl />
            </div>
```

Run the Shell tests again. Expected: PASS, every case.

- [ ] **Step 7: Draw it as a segmented row**

Append to `web/src/styles/shell.css`:

```css

/* The theme switch (`ThemeControl.tsx`): three native radios drawn as one
 * segmented row. The inputs stay in the layout at zero opacity over their
 * labels, so a click, a tap and the arrow keys all reach the real control;
 * `:has()` carries the checked and focused states up to the label. */
.theme-control {
  min-inline-size: 0;
  margin: 0;
  border: 0;
  padding: 0 1rem 1rem;
}

.theme-control legend {
  padding: 0;
}

.theme-control .segments {
  display: flex;
  border: 1px solid var(--line-strong);
}

.theme-control label {
  position: relative;
  flex: 1;
  padding: 0.25rem 0;
  font-size: var(--text-ui);
  text-align: center;
  color: var(--muted);
  cursor: pointer;
}

.theme-control label + label {
  border-left: 1px solid var(--line-strong);
}

.theme-control input {
  position: absolute;
  inset: 0;
  margin: 0;
  opacity: 0;
  cursor: pointer;
}

.theme-control label:has(input:checked) {
  background: var(--text);
  color: var(--bg);
}

.theme-control label:has(input:focus-visible) {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
```

- [ ] **Step 8: The whole jsdom and node projects, lint, typecheck**

```bash
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web run lint
pnpm -C web run typecheck
pnpm -C web exec vitest run --project browser --reporter=default
```

Expected: all exit 0, and the browser project reports 28 passed. `BrowseSettings.test.tsx` still counts six labels, because the control is Shell's sibling of `BrowseSettings`, not a child of it.

- [ ] **Step 9: Commit**

```bash
git add web/src/ThemeControl.tsx web/src/ThemeControl.test.tsx web/src/Shell.tsx web/src/Shell.test.tsx web/src/styles/shell.css
git commit -m "Style pass 1: the System / Paper / Ink control in the sidebar"
```

---

### Task 7: The login screen takes the tokens

**Files:**
- Modify: `web/src/Login.tsx:19-84` and its JSX, `web/src/Login.test.tsx` (one case), `web/src/colourLiterals.test.ts` (remove one entry)
- Delete: `web/src/Login.theme.test.ts`

**Interfaces:**
- Consumes: the colour and type tokens (Tasks 2 and 4), `base.css`'s `:where(.library, .login)` scope (Task 4), and the guard's `ALLOWED` (Task 3).
- Produces: `Login`'s overlay carries `className="login"`.

- [ ] **Step 1: Write the failing test**

In `web/src/Login.test.tsx`, directly after the `it('sends what was typed and reports success upward', …)` case, add:

```tsx
  it('carries the class base.css scopes the theme’s controls to', () => {
    const host = mount(<Login onSignedIn={noop} />)
    expect(host.firstElementChild?.classList.contains('login')).toBe(true)
  })
```

```bash
pnpm -C web exec vitest run --project jsdom src/Login.test.tsx --reporter=default
```

Expected: the new case FAILS; the rest pass.

- [ ] **Step 2: Restyle**

In `web/src/Login.tsx`, replace everything from the comment beginning `// This screen used to be reachable only from inside the reader, which is` down to and including the closing `}` of `errorStyle` with:

```tsx
// The sign-in screen takes the theme like every screen inside `.library`
// (spec 2026-09-17 §2): paper or ink by the reader's choice, the title face
// for the wordmark, `--danger` for a refusal. Its controls are drawn by
// `styles/base.css`, whose rules reach this screen through the `login` class
// on the overlay, so what stays inline here is layout — and the accent on the
// one button that submits.
//
// History worth keeping: this screen shipped a hardcoded dark palette (`#111`,
// `#ffb4a8`) when it only ever appeared over the reader, and handed a
// light-preference reader a dark screen once it fronted the library.
// `colourLiterals.test.ts` now refuses any colour in this file at all.
const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'var(--bg)',
  zIndex: 30,
}

const panelStyle: CSSProperties = {
  width: 'min(360px, 90vw)',
  padding: '32px 28px',
  borderTop: '1px solid var(--line)',
  borderBottom: '1px solid var(--line)',
}

const wordmarkStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 10,
  margin: 0,
}

/** 対訳, *taiyaku*, set vertically in the accent like a seal. */
const sealStyle: CSSProperties = {
  writingMode: 'vertical-rl',
  padding: '4px 3px 3px',
  background: 'var(--accent)',
  color: 'var(--on-accent)',
  fontSize: 'var(--text-meta)',
  letterSpacing: '0.1em',
  lineHeight: 1,
}

const fieldStyle: CSSProperties = {
  display: 'block',
  boxSizing: 'border-box',
  width: '100%',
  marginTop: 6,
  padding: '8px 10px',
}

const labelStyle: CSSProperties = {
  display: 'block',
  marginTop: 16,
  fontSize: 'var(--text-ui)',
}

const errorStyle: CSSProperties = {
  marginTop: 16,
  color: 'var(--danger)',
  fontSize: 'var(--text-ui)',
}

const submitStyle: CSSProperties = {
  ...fieldStyle,
  marginTop: 24,
  borderColor: 'var(--accent)',
  background: 'var(--accent)',
  color: 'var(--on-accent)',
}
```

In the same file's JSX, replace:

```tsx
    <div style={overlayStyle}>
      <form style={panelStyle} onSubmit={submit}>
        <h2 style={{ margin: 0, fontSize: '1.1em' }}>taiyaku</h2>
```

with:

```tsx
    <div className="login" style={overlayStyle}>
      <form style={panelStyle} onSubmit={submit}>
        <h2 style={wordmarkStyle}>
          <span style={sealStyle} aria-hidden="true">
            対訳
          </span>
          taiyaku
        </h2>
```

and replace:

```tsx
          style={{ ...fieldStyle, marginTop: 18, cursor: 'pointer' }}
```

with:

```tsx
          style={submitStyle}
```

- [ ] **Step 3: Retire the narrow guard and lower the allowance**

```bash
git rm web/src/Login.theme.test.ts
```

In `web/src/colourLiterals.test.ts`, delete the line `  './Login.tsx': 9,`.

- [ ] **Step 4: Run the Login tests and the guard**

```bash
pnpm -C web exec vitest run --project jsdom src/Login.test.tsx --reporter=default
pnpm -C web exec vitest run --project node src/colourLiterals.test.ts --reporter=default
```

Expected: both PASS. The guard now holds `Login.tsx` to zero, which is stronger than the test it replaces: that one refused only a hex literal outside one `light-dark()` pair.

- [ ] **Step 5: Watch both fail**

Apply each alone, run the matching command from Step 4, confirm, undo:

1. `Login.tsx`: remove `className="login" ` from the overlay. Expected: "carries the class …" FAILS.
2. `Login.tsx`: in `errorStyle`, `color: 'var(--danger)'` → `color: '#b00020'`. Expected: the guard FAILS, listing `./Login.tsx: 1`.

- [ ] **Step 6: The jsdom and node projects, lint, typecheck**

```bash
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: all exit 0.

- [ ] **Step 7: Commit**

```bash
git add web/src/Login.tsx web/src/Login.test.tsx web/src/colourLiterals.test.ts
git commit -m "Style pass 1: the login screen takes the tokens"
```

---

### Task 8: Measure it in a browser, record it, and run the gate

**Files:**
- Modify: `docs/notes.md` (append a section), `docs/status.md` (test counts; the runtime dependency sentence), `docs/superpowers/specs/2026-09-17-style-pass-design.md` (five amendments)

**Interfaces:**
- Consumes: every earlier task.
- Produces: the figures `notes.md` records, and a green gate.

- [ ] **Step 1: Build the shipped bundle and compare it with Task 1's baseline**

```bash
pnpm -C web exec vite build --outDir ../target/style-pass/dist-after --emptyOutDir > target/style-pass/build-after.log 2>&1; echo "exit $?"
cat target/style-pass/build-after.log
for d in dist-before dist-after; do
  echo "== $d"
  du -sb target/style-pass/$d
  for ext in js css woff2 woff; do
    printf '%s files: %s, bytes: %s\n' "$ext" \
      "$(find target/style-pass/$d -name "*.$ext" | wc -l)" \
      "$(find target/style-pass/$d -name "*.$ext" -exec cat {} + | wc -c)"
  done
done
```

Expected: `exit 0`. Record both blocks for `notes.md`: the JS and CSS sizes before and after, and the number and total bytes of font files the build now ships.

- [ ] **Step 2: Serve a scratch copy of the real library**

This branch changes nothing under `crates/` (`git diff --stat main -- crates` prints nothing), so `target/debug/taiyaku` built at `dbd47a3` is current; rebuild it with `cargo build -p taiyaku-cli` only if that diff is not empty. Skip what is already running (`ss -ltn | command grep -E ':(8491|5291) '`). Otherwise:

```bash
mkdir -p target/style-pass/lib target/style-pass/cache
target/debug/taiyaku library backup --out target/style-pass/lib --db /home/davey/temp/manga-test/library.db
mv target/style-pass/lib/library-*.db target/style-pass/lib/library.db
printf 'stylepass\nstylepass\n' | target/debug/taiyaku library user password local --db target/style-pass/lib/library.db
```

Then start two servers with the Bash tool's `run_in_background`, not `&`:

```bash
exec target/debug/taiyaku serve --addr 127.0.0.1:8491 --db target/style-pass/lib/library.db --cache target/style-pass/cache --insecure-cookies
```

```bash
TAIYAKU_SERVE=http://127.0.0.1:8491 exec web/node_modules/.bin/vite web --host 127.0.0.1 --port 5291 --strictPort
```

The backup reads the real `library.db` and writes only under `target/`; every write the browser pass makes lands in the copy.

- [ ] **Step 3: Drive every browse screen in both themes**

With Playwright, sign in at `http://127.0.0.1:5291/` as `local` / `stylepass`. At 1440×900 and again at 390×844, in Paper and in Ink (chosen with the sidebar control), screenshot `/`, `/series/6`, `/search`, `/proposals`, `/config/roots` and `/account` to `.playwright-mcp/style-1-<screen>-<theme>-<width>.png`, and read every screenshot. Record for `notes.md`: anything unreadable, clipped, or still drawn in a browser-default grey. Layout problems at 390 px are expected — the sidebar still takes 14rem until PR ② — and are recorded as seen, not fixed.

- [ ] **Step 4: Measure that the tokens resolve and the choice persists**

On `/`, for each theme choice, evaluate:

```js
() => ({
  theme: document.documentElement.getAttribute('data-theme'),
  scheme: getComputedStyle(document.documentElement).colorScheme,
  bg: getComputedStyle(document.querySelector('.library')).backgroundColor,
  text: getComputedStyle(document.querySelector('.library')).color,
})
```

Expected: Paper gives `bg` `rgb(243, 238, 227)` and `text` `rgb(35, 30, 23)`; Ink gives `rgb(21, 19, 15)` and `rgb(236, 228, 212)`. Choose Ink, reload the page, evaluate again: `theme` is `ink` and `bg` is the ink value. Choose System, then with `browser_run_code_unsafe` run `await page.emulateMedia({ colorScheme: 'dark' })` and evaluate: `theme` is `null`, `bg` is the ink value; emulate `light`: the paper value. Record all of it.

- [ ] **Step 5: Measure the figures face and the fonts a page downloads**

On `/`, after the page has settled, evaluate:

```js
async () => {
  await document.fonts.ready
  const main = document.querySelector('.library main')
  const width = (inTable, text) => {
    const host = document.createElement(inTable ? 'table' : 'div')
    const span = document.createElement('span')
    span.textContent = text
    if (inTable) host.insertRow().insertCell().append(span)
    else host.append(span)
    main.append(host)
    const w = span.getBoundingClientRect().width
    host.remove()
    return w
  }
  return {
    table: [width(true, '1111111111'), width(true, '8888888888')],
    outside: [width(false, '1111111111'), width(false, '8888888888')],
    fonts: performance
      .getEntriesByType('resource')
      .filter((e) => /\.woff2?(\?|$)/.test(e.name))
      .map((e) => [e.name.split('/').pop(), e.encodedBodySize]),
  }
}
```

Expected: the two `table` widths are equal and the two `outside` widths differ — the second pair is the control that shows Zen Kaku Gothic New's proportional digits were measured, not assumed. Record both pairs, and every font file with its size.

- [ ] **Step 6: Check the reader and the login screen**

Open `/read/dandadan---v01--2022--cbz` in Paper and in Ink at 1440×900, press `Tab` to show the chrome, and screenshot. Evaluate on the layout button:

```js
() => getComputedStyle(document.querySelector('button[aria-pressed]')).borderRadius
```

Expected: not `2px` — `base.css` does not reach the reader. Then sign out from `/account`, and at 390×844 screenshot the login screen in both themes (set `localStorage['taiyaku.prefs.theme']` to `{"choice":"ink"}` and reload for Ink). Record what the screenshots show.

Run the browser test project, which covers O65's docked button among others:

```bash
pnpm -C web test:browser
```

Expected: every test passes. Record the count.

- [ ] **Step 7: Count the tests**

```bash
pnpm -C web exec vitest run --project node --reporter=default 2>&1 | command grep -E '^\s+Tests '
pnpm -C web exec vitest run --project jsdom --reporter=default 2>&1 | command grep -E '^\s+Tests '
```

Record both figures. Expected: `node` 1,159 + 4 (index.css.test.ts) + 3 (colourLiterals) + 2 (base.css) + 13 (theme) − 1 (Login.theme) = **1,180**; `jsdom` 977 + 4 (ThemeControl) + 1 (Shell) + 1 (Login) = **983**. If a run disagrees, the run is right: write its figure and find the difference before going on.

- [ ] **Step 8: Write it down**

Append to `docs/notes.md` a section headed `## Style pass ①: the foundation — <today's date>`, in this file's usual style (what was measured, how, the figures), holding:

1. Task 1's split check: `dist identical` from `diff -r`.
2. Step 1's bundle figures before and after, and the font files the build ships.
3. Step 5's digit widths (both pairs) and the fonts the library page downloaded.
4. Step 4's computed colours for Paper, Ink, and System under emulated dark and light, and the reload check.
5. Step 6's reader border-radius and the browser project's count.
6. Step 3's and Step 6's observations, including what is left for PRs ② to ⑤.
7. The fontTools reading that decided the figures face: Zen Kaku Gothic New `latin-400` GSUB features `ccmp`, `frac`, `vert`, `vrt2`; digit advances 0: 504, 1: 383, 2: 445, 3: 484, 4: 493, 5: 468, 6: 493, 7: 412, 8: 509, 9: 475.

In `docs/status.md`:

1. In `Six crates, 2227 tests, plus 2,136 in \`web/\`'s TypeScript — 1,159 in the \`node\`` and the line after it, replace `2,136`, `1,159` and `977` with Step 7's total, `node` and `jsdom` figures.
2. After `alone cost +37.59 kB raw / +13.16 kB gzip measured in isolation when it landed` and the `(Task 7),` on the line after it, insert: ` and the style pass's first PR added \`@fontsource/shippori-mincho-b1\` and \`@fontsource/zen-kaku-gothic-new\`, which are fonts and CSS rather than code: <Step 1's before and after JS and CSS sizes, and the font files and bytes the build ships>,` — with the measured figures in place of the angle brackets.

In `docs/superpowers/specs/2026-09-17-style-pass-design.md` — whose prose is wrapped at 80 columns, so each phrase quoted below may break across a line; match it across the break, and re-wrap what you write:

1. §2, replace the paragraph beginning `**Not yet known**, and checked in PR ① rather than assumed:` with a paragraph recording the fontTools reading, the `taiyaku figures` face and `--font-figures`, Step 5's digit widths, and that dense tables were read in Step 3 (and what was seen).
2. §2 **Type**, replace `What the whole set adds to \`web/dist\` was not measured; PR ① measures it.` with Step 1's figures.
3. §3 item 6, replace it with: `**\`index.html\`** gains two \`<meta name="theme-color">\` tags, one per \`prefers-color-scheme\`, so Safari's toolbar matches the ground — the OS's ground, not a pinned choice. \`viewport-fit=cover\` moved to PR ③: without the safe-area padding ③ designs, it would put the library under a phone's notch in landscape.`
4. §4 rule 5, replace the two sentences beginning `The Accept check has a case that reproduces the` and ending `that trust yet.` with: `None of the three can reproduce its shipped defect any more — each was measured against a system colour no rule uses now — so the token model earns trust instead by reproducing four ratios §2 computed outside the test.`
5. §5 ①, after `Every browser-default control on every screen takes the tokens, and the login screen is restyled.` add: `Every one inside \`.library\` and \`.login\`, that is: \`base.css\` scopes its element rules there with \`:where()\`, because the reader's inline-styled chrome leaves properties to the browser that a document-wide rule would change under ③. And all 79 system-colour keywords become tokens in ①, one to one, since rule 5 recomputes D2, D3 and the Accept against tokens. Four rules stay document-wide and so reach the reader and the labeller — \`:root\`'s font, \`body\`'s background and colour, and \`::selection\` — so the reader's buttons and readout take the UI font in ① and shift width a little; accepted 2026-09-17 because ③ redraws that chrome.`

- [ ] **Step 9: Run the gate**

Run with the Bash tool's `run_in_background`:

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh > target/style-pass/gate.log 2>&1; echo "exit $?"
```

When it finishes, read the end of `target/style-pass/gate.log`. Expected: `exit 0` and an `all green` line. Quote that line exactly in the PR body. A failure is read in full and fixed, never summarised.

- [ ] **Step 10: Commit**

```bash
git add docs/notes.md docs/status.md docs/superpowers/specs/2026-09-17-style-pass-design.md
git commit -m "Style pass 1: measured in the browser, recorded, gate green"
```

---

## After the tasks

Not tasks in this plan, but required before the PR opens (spec §5, "Every PR, before it is opened"):

1. A whole-branch review of `main..style-1-foundation`, then a review of each fix round until one finds nothing.
2. Every hand mutation above re-run by whoever dispatched the tasks, control run first.
3. The pre-merge claim check over the PR body, `docs/status.md`, `docs/notes.md` and the spec's amendments.
4. The PR through the Gitea MCP, never `gh`. The body is the squash commit message, is not hard-wrapped, and carries no attribution.

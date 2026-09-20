# Style pass ③ — the reader: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redraw the reader in ink on a black ground — a top bar and a bottom bar that come and go together, panels that dock between them, safe areas on a phone — and answer O112, without changing what the reader does beyond the four things spec §8 names.

**Architecture:** A new `styles/reader.css` holds the reader's look and the custom properties its geometry reads (the strip's height at the 900 px breakpoint, the safe areas, where panels dock). `color-scheme: dark` on the `.reader` root makes every `light-dark()` token resolve to its Ink half, so components draw from the ordinary tokens. A new `TopBar` component joins the strip inside `Chrome`, which becomes two bars; `App.tsx` loses its always-on layout row and fetches the work's title from the library. The panels move from four copies of one inline style object to classes.

**Tech Stack:** React 19, React Router 8, Vite 8, Vitest 4 (projects `node`, `jsdom`, `browser`), Biome 2.5.7, pnpm 11.

**Spec:** [`docs/superpowers/specs/2026-09-17-style-pass-design.md`](../specs/2026-09-17-style-pass-design.md) — §5 ③ and §8. This plan is its PR ③.

**Branch:** `style-pass-3-reader`, off `dbb68e4`, which carries §8 (`8fc236c`).

## Decided while planning

These came out of reading and probing the code the plan touches, on this
branch, before any task.

1. **`light-dark()` tokens resolve per element.** A page holding the token
   rules, one paragraph outside a `color-scheme: dark` box and one inside,
   under Paper pinned and System with the OS emulated light and dark: the
   inside paragraph computed Ink's text, `rgb(236, 228, 212)`, in all four,
   in Chromium 151 and Firefox 153 (spec §8). So `.reader` needs no palette
   of its own.
2. **jsdom keeps `var()` in inline styles verbatim.** A probe set
   `borderColor: 'var(--accent)'`, `bottom: 'calc(var(--reader-bottom) + 8px)'`
   and `background: 'var(--reader-panel)'` through React and read back
   exactly those strings from `element.style`; a `padding` shorthand holding
   `var()` reads back whole too, with its longhands `''`. So geometry that depends on
   the viewport stays inline, reading custom properties that
   `styles/reader.css` sets per breakpoint, and a jsdom test pins the
   expression while the browser project measures the pixels.
3. **Biome accepts what `reader.css` needs** (probe): `env(safe-area-inset-top,
   0px)`, `html:has(.reader)`, `rgb(21 19 15 / 0.92)`, `@media (width <
   900px)` redefining a custom property, and `clip-path: inset(50%)`.
4. **Today's layout row is 37 px.** Measured in the browser project on
   `dbb68e4`: the row 37 px tall and the page area 863 px at 1440×900, 987
   at 768×1024. Spec §8 now quotes it.
5. **Tests find the reader's buttons by `textContent`** — `App.test.tsx`'s
   and `App.browser.test.tsx`'s `button()`, `Chrome.test.tsx`'s `Library`.
   So the top bar's icon buttons carry their word as visually hidden text:
   `single`, `two`, `scroll`, `Settings`, and `Library` beside a chevron.
6. **`App.test.tsx`'s `pageArea` helper is `children[1]` of the root**, which
   stops being the page area when the layout row leaves. The page area gets
   the class `page-area` and the helper finds it by that.
7. **`App` draws nothing until its load answers**, by design (`App.test.tsx`,
   "draws nothing until the load answers"), so its `.reader` root is not
   there to paint the ground black during a load. `Routes.tsx`'s `Reader`
   wraps it in `div.reader-route`, and the ground rule matches either.
8. **Only `s` closes Settings.** `keyPlan`'s `Escape` branch has no arm for
   it, and it has no Close button. `Settings.test.tsx` mounts it twelve
   times, each needing the new `onClose`.
9. **`Annotations`' overlay stands at `zIndex: 2`**, so the two bars take
   `zIndex: 3`, the docked panels' figure; the panels dock clear of the bars
   by construction.
10. **The reader's title comes from `library.ts`'s existing `work(slug)`**,
    the client for `/api/library/works/{slug}`, and `App.test.tsx` and
    `App.browser.test.tsx` mock `./library` for it the way they mock `./api`.

## Global Constraints

- Colours come from `styles/tokens.css` only. No colour literal (hex, `rgb()`, `rgba()`, `hsl()`, `hsla()`) or CSS system-colour keyword may be added to any non-test file under `web/src/` except `styles/tokens.css` and `palette.ts`; `colourLiterals.test.ts` enforces it, and a file whose count falls must have its entry lowered — or removed at zero — in the same task.
- The reader is dark in both themes: `color-scheme: dark` on `.reader`, `#000` behind the page (`--reader-ground`), panels `--reader-panel` (Ink's `bg` at 92%) with a 1 px `--reader-hair` (Ink's `text` at 14%) and no shadow, square corners on panels and bars.
- The breakpoint is 900 px: `@media (width < 900px)`. Strip tiles are 160 px at and above it, 96 px below.
- Spacing is `--space-1` to `--space-7` (4, 8, 12, 16, 24, 32, 48 px); sizes are `--text-h1`, `--text-h2`, `--text-h3`, `--text-body`, `--text-ui`, `--text-meta`; buttons and inputs take `--radius-control` (2 px).
- Touch targets are 44 px under `(pointer: coarse)`.
- What the reader does changes in exactly these ways (spec §8): the layout control and `Library` move into a top bar shown with the strip; ⚙ opens Settings and Settings gains a Close button; the reader fetches `/api/library/works/{slug}` once for its title; the note list under the page is hidden while the chrome is shown (O112). The library's screens change only by safe-area padding.
- Geometry that varies with the viewport is a custom property in `styles/reader.css`, read inline with `var()`. Other geometry stays inline where a test pins it (§4 rule 3). Visual properties — colour, border, type — move to classes in `reader.css` (§4 rule 4).
- `index.css.test.ts`'s `ruleBody` returns the *first* rule in source order whose selector matches exactly. Keep `shell.css`'s main `.library` rule first and keep its `background` and `color`.
- Component tests use `mount()` from `web/src/testing/mount.ts` and `act()`; `@testing-library/react` is not a dependency.
- `pnpm -C web test` runs the `node` project only. Run a `.test.tsx` with `--project jsdom` and a `.browser.test.tsx` with `--project browser`. Under Claude Code pass `--reporter=default` when reading test output.
- Typecheck with `pnpm -C web run typecheck`, and lint with `pnpm -C web run lint`, which is `biome check` and includes formatting: if it reports formatting, run `pnpm -C web exec biome format --write <the files>` and lint again.
- Never `cd`: the shell's `cd` is zoxide. Use `pnpm -C web …` and paths from the repository root.
- Every new test is watched failing with its defect put back by hand, after a control run shows it green, and passing again once restored. Each task names its mutations.
- Scratch output goes under `target/style-pass/three/` (gitignored), never `/tmp` or `$HOME`.
- Commit at the end of every task, with a message starting `Style pass 3:` and no attribution lines.

---

### Task 1: The frame — the ground, the reader's tokens, the safe areas

**Files:**
- Create: `web/src/styles/reader.css`, `web/src/styles/reader.css.test.ts`
- Modify: `web/src/styles/tokens.css`, `web/src/index.css`, `web/index.html`, `web/src/styles/shell.css`, `web/src/Routes.tsx`, `web/src/App.tsx`, `web/src/index.css.test.ts`, `web/src/Routes.test.tsx`, `web/src/App.test.tsx`, `web/src/App.browser.test.tsx`

**Interfaces:**
- Produces, in `tokens.css`: `--reader-ground`, `--reader-panel`, `--reader-hair`.
- Produces, on `.reader` in `reader.css`: `--reader-top-bar` (52px), `--strip-tile` (160px; 96px below 900 px), `--strip-height`, `--progress-height` (2px), `--readout-height` (32px), `--reader-bottom-bar`, `--safe-top`, `--safe-right`, `--safe-bottom`, `--safe-left`, `--panel-top`, `--panel-bottom`.
- Produces, in `App.tsx`'s markup: the root carries `className="reader"`, the page area `className="page-area"`. In `Routes.tsx`: `div.reader-route` around `<App>`.

- [ ] **Step 1: Write the failing node tests**

Create `web/src/styles/reader.css.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import indexHtml from '../../index.html?raw'
import { rules, selectors, withoutComments } from '../testing/css'
import readerSource from './reader.css?raw'
import shellSource from './shell.css?raw'

/**
 * The reader's frame, read from its stylesheet (spec 2026-09-17 §8.2). jsdom
 * renders none of this, and the browser project measures what it draws
 * (`App.browser.test.tsx`); what is pinned here is what the rules declare.
 */

/** The bodies of every rule whose prelude is exactly `prelude`. */
const bodies = (css: string, prelude: string) =>
  rules(css)
    .filter((rule) => rule.prelude === prelude)
    .map((rule) => rule.body)

/**
 * Every value `property` is given in those rules, with all whitespace taken
 * out, so the check does not depend on how Biome wraps a long `calc()`.
 */
function declared(css: string, prelude: string, property: string): string[] {
  return bodies(css, prelude).flatMap((body) =>
    [
      ...body.matchAll(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`, 'g')),
    ].map((m) => m[1].replace(/\s+/g, '')),
  )
}

describe('the reader root', () => {
  it('is dark in both themes, on the black ground', () => {
    expect(declared(readerSource, '.reader', 'color-scheme')).toEqual(['dark'])
    expect(declared(readerSource, '.reader', 'background')).toEqual([
      'var(--reader-ground)',
    ])
    expect(declared(readerSource, '.reader', 'color')).toEqual(['var(--text)'])
  })

  it('blacks out the document behind it, from the route box as well as the root', () => {
    const ground = rules(readerSource).find((rule) =>
      rule.body.includes('var(--reader-ground)') && rule.prelude !== '.reader',
    )
    expect(ground && selectors(ground.prelude)).toEqual([
      ':root:has(.reader-route, .reader)',
      ':root:has(.reader-route, .reader) body',
    ])
  })

  it('stands the strip’s tiles at 160 px, and 96 below the breakpoint', () => {
    expect(declared(readerSource, '.reader', '--strip-tile')).toEqual([
      '160px',
      '96px',
    ])
    expect(withoutComments(readerSource)).toMatch(
      /@media \(width < 900px\)\s*\{\s*\.reader\s*\{\s*--strip-tile:\s*96px;\s*\}\s*\}/,
    )
  })

  it('docks panels between the bars from the safe areas and the bars’ heights', () => {
    expect(declared(readerSource, '.reader', '--panel-top')).toEqual([
      'calc(var(--safe-top)+var(--reader-top-bar)+var(--space-4))',
    ])
    expect(declared(readerSource, '.reader', '--panel-bottom')).toEqual([
      'calc(var(--safe-bottom)+var(--reader-bottom-bar)+var(--space-4))',
    ])
    expect(declared(readerSource, '.reader', '--safe-top')).toEqual([
      'env(safe-area-inset-top,0px)',
    ])
  })
})

describe('the safe areas', () => {
  it('are there to pad: index.html lets the page run under the notch', () => {
    expect(indexHtml).toMatch(
      /<meta name="viewport" content="width=device-width, initial-scale=1\.0, viewport-fit=cover" \/>/,
    )
  })

  it('pad the library’s frame, its phone bar and its drawer', () => {
    /** Every rule for `prelude`, joined, with whitespace taken out. */
    const padding = (prelude: string) =>
      bodies(shellSource, prelude).join(';').replace(/\s+/g, '')
    expect(padding('.library')).toContain('env(safe-area-inset-left,0px)')
    expect(padding('.library')).toContain('env(safe-area-inset-right,0px)')
    expect(padding('.library')).toContain('env(safe-area-inset-bottom,0px)')
    expect(padding('.phone-bar')).toContain('env(safe-area-inset-top,0px)')
    expect(padding('.sidebar-column')).toContain('env(safe-area-inset-left,0px)')
    expect(padding('.menu-close')).toContain('env(safe-area-inset-top,0px)')
  })
})
```

In `web/src/index.css.test.ts`, directly after the `describe("spec §2's floors hold in both themes", …)` block closes, add:

```ts
/** `--name: rgb(r g b / a)` in `tokens.css`, as its channels and its alpha. */
function readerToken(name: string): { rgb: Rgb; alpha: number } {
  const m = tokensSource.match(
    new RegExp(`--${name}:\\s*rgb\\((\\d+) (\\d+) (\\d+) / ([\\d.]+)\\)`),
  )
  if (!m) throw new Error(`tokens.css has no rgb() value for --${name}`)
  return {
    rgb: [Number(m[1]), Number(m[2]), Number(m[3])],
    alpha: Number(m[4]),
  }
}

const BLACK: Rgb = [0, 0, 0]
const WHITE: Rgb = [255, 255, 255]

describe("the reader's tokens (spec §8.2)", () => {
  it("are Ink's --bg and --text at §2's opacities, over a black ground", () => {
    expect(tokensSource).toMatch(/--reader-ground:\s*#000;/)
    expect(readerToken('reader-panel')).toEqual({
      rgb: token('bg', 'ink'),
      alpha: 0.92,
    })
    expect(readerToken('reader-hair')).toEqual({
      rgb: token('text', 'ink'),
      alpha: 0.14,
    })
  })

  it('reproduce two ratios the spec computed outside this file', () => {
    const { rgb, alpha } = readerToken('reader-panel')
    expect(
      contrast(token('text', 'ink'), composite(rgb, BLACK, alpha)),
    ).toBeCloseTo(14.86, 2)
    expect(
      contrast(token('muted', 'ink'), composite(rgb, WHITE, alpha)),
    ).toBeCloseTo(5.52, 2)
  })

  for (const [under, ground] of [
    ['black', BLACK],
    ['white artwork', WHITE],
  ] as const) {
    it(`keep every Ink text colour above 4.5:1 on a panel over ${under}`, () => {
      const { rgb, alpha } = readerToken('reader-panel')
      const panel = composite(rgb, ground, alpha)
      const failures = TEXT_TOKENS.map(
        (text) => [text, contrast(token(text, 'ink'), panel)] as const,
      )
        .filter(([, ratio]) => ratio < 4.5)
        .map(([text, ratio]) => `--${text}: ${ratio.toFixed(2)}`)
      expect(failures).toEqual([])
    })
  }
})
```

- [ ] **Step 2: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project node --reporter=default src/styles/reader.css.test.ts src/index.css.test.ts
```

Expected: FAIL — `reader.css.test.ts` cannot resolve `./reader.css?raw`, and `index.css.test.ts` reports `tokens.css has no rgb() value for --reader-panel`.

- [ ] **Step 3: Add the reader's tokens**

In `web/src/styles/tokens.css`, append after the box palette's `:root { … }` block, at the end of the file:

```css

/* The reader's own three (spec 2026-09-17 §2 and §8.2). Fixed, like the box
 * palette and unlike the pairs above: the reader is dark in both themes. Each
 * is an Ink value from those pairs — `--bg` #15130f and `--text` #ece4d4 — at
 * the opacity §2 gives it, and `index.css.test.ts` holds them to the pairs. */
:root {
  --reader-ground: #000;
  --reader-panel: rgb(21 19 15 / 0.92);
  --reader-hair: rgb(236 228 212 / 0.14);
}
```

- [ ] **Step 4: Create `reader.css` and import it**

Create `web/src/styles/reader.css`:

```css
/* The reader — spec 2026-09-17 §8.
 *
 * Dark in both themes: `color-scheme: dark` on `.reader` resolves every
 * `light-dark()` token to its Ink half inside the reader — measured in
 * Chromium 151 and Firefox 153 (§8) — so the bars and panels draw from the
 * ordinary tokens and nothing here keeps a second palette. `--reader-ground`,
 * `--reader-panel` and `--reader-hair` (`tokens.css`) are the three values §2
 * gives the reader alone.
 *
 * Geometry stays inline in the components, where their tests pin it (§4 rule
 * 3). What varies with the viewport — the strip's height, the safe areas,
 * where a panel docks — is a custom property here, which those inline styles
 * read with `var()`. */

/* The route's own box, so the ground is black from the first frame: `App`
 * draws nothing until its load answers, and its `.reader` root is not there
 * until then. */
.reader-route {
  height: 100%;
}

:root:has(.reader-route, .reader),
:root:has(.reader-route, .reader) body {
  background: var(--reader-ground);
}

.reader {
  --reader-top-bar: 52px;
  /* The strip's tiles. 160 px is `cover.rs`'s `THUMB_LONG_EDGE`, a tile's
   * natural height, which a manual pass asked for; below the breakpoint 96,
   * so a phone shows about five pages and the two bars cover about 27% of a
   * 390×844 screen rather than 35% (§8.1). */
  --strip-tile: 160px;
  /* The rail's 6 px padding and a tile's 2 px border, top and bottom
   * (`Filmstrip.tsx`'s `railStyle` and `tileStyle`). */
  --strip-height: calc(var(--strip-tile) + 16px);
  --progress-height: 2px;
  --readout-height: 32px;
  /* The bottom bar above the safe area: its 1 px rule, the strip, the
   * progress line and the readout row. */
  --reader-bottom-bar: calc(
    1px + var(--strip-height) + var(--progress-height) + var(--readout-height)
  );
  --safe-top: env(safe-area-inset-top, 0px);
  --safe-right: env(safe-area-inset-right, 0px);
  --safe-bottom: env(safe-area-inset-bottom, 0px);
  --safe-left: env(safe-area-inset-left, 0px);
  /* Where a panel docks (§8.1): below the top bar's line and above the bottom
   * bar's, whether or not the chrome is shown, so a panel never moves. */
  --panel-top: calc(var(--safe-top) + var(--reader-top-bar) + var(--space-4));
  --panel-bottom: calc(
    var(--safe-bottom) + var(--reader-bottom-bar) + var(--space-4)
  );
  color-scheme: dark;
  background: var(--reader-ground);
  color: var(--text);
  font-family: var(--font-ui);
}

@media (width < 900px) {
  .reader {
    --strip-tile: 96px;
  }
}
```

In `web/src/index.css`, add after the `@import "./styles/admin.css";` line:

```css
@import "./styles/reader.css";
```

- [ ] **Step 5: `viewport-fit=cover`, and the library's frame padded for it**

In `web/index.html`, replace

```html
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
```

with

```html
    <meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover" />
```

In `web/src/styles/shell.css`, directly after the `.library > main { … }` rule (the one with `min-width: 0`), add:

```css

/* The safe areas (spec 2026-09-17 §8.2). `index.html`'s `viewport-fit=cover`
 * lets the page run under a phone's notch, rounded corners and home
 * indicator, so the frame pads itself by the insets the browser reports —
 * zero on a screen without them. After the main `.library` rule, which
 * `index.css.test.ts`'s `ruleBody` reads first. */
.library {
  box-sizing: border-box;
  padding: 0 env(safe-area-inset-right, 0px) env(safe-area-inset-bottom, 0px)
    env(safe-area-inset-left, 0px);
}
```

Inside the `@media (width < 900px)` block, in the `.phone-bar` rule, replace

```css
    height: 52px;
    padding: 0 var(--space-2);
```

with

```css
    box-sizing: border-box;
    height: calc(52px + env(safe-area-inset-top, 0px));
    padding: env(safe-area-inset-top, 0px) var(--space-2) 0;
```

In the same block's `.menu-close` rule, replace `    top: var(--space-2);` with

```css
    top: calc(var(--space-2) + env(safe-area-inset-top, 0px));
```

and in the same block's `.sidebar-column` rule, after `    width: min(20rem, 85vw);`, add

```css
    box-sizing: border-box;
    padding: env(safe-area-inset-top, 0px) 0 env(safe-area-inset-bottom, 0px)
      env(safe-area-inset-left, 0px);
```

- [ ] **Step 6: Run the node tests to verify they pass**

```bash
pnpm -C web exec vitest run --project node --reporter=default src/styles/reader.css.test.ts src/index.css.test.ts src/theme.test.ts src/colourLiterals.test.ts
```

Expected: PASS. `theme.test.ts` still finds exactly one `color-scheme` among `tokens.css`'s `:root` rules, since the new block declares none.

- [ ] **Step 7: Write the failing component tests**

In `web/src/Routes.test.tsx`, directly after the `it('opens the reader on the work in the URL', …)` test, add:

```tsx
  it('puts the reader inside its route box, which blacks out the page before the reader draws', () => {
    const host = at('/read/gotoubun-v01')
    expect(
      host.querySelector('.reader-route > [data-testid="reader"]'),
    ).not.toBeNull()
  })
```

In `web/src/App.test.tsx`, replace the `pageArea` helper and its doc comment

```ts
/**
 * The page area — the second child of the reader's root column, and what
 * `pageAreaRef` points at. The wheel listener and the swap's measurements are
 * both on this element rather than on the page inside it.
 */
const pageArea = (host: HTMLElement) =>
  host.firstElementChild?.children[1] as HTMLElement
```

with

```ts
/**
 * The page area — `.page-area`, and what `pageAreaRef` points at. The wheel
 * listener and the swap's measurements are both on this element rather than
 * on the page inside it. Found by its class: it was the root's second child
 * until the layout row left for the top bar (style pass ③).
 */
const pageArea = (host: HTMLElement) =>
  host.querySelector('.page-area') as HTMLElement
```

and directly after the `it('draws nothing until the load answers, then the restored page', …)` test, add:

```tsx
  it('marks its root as the reader, and the page area inside it', async () => {
    const host = await open()
    const root = host.firstElementChild as HTMLElement
    expect(root.classList.contains('reader')).toBe(true)
    expect(pageArea(host)?.parentElement).toBe(root)
  })
```

In `web/src/App.browser.test.tsx`, in `readerWithChrome`, replace

```ts
  await until(() => button('scroll') !== undefined)
```

with

```ts
  await until(() => document.querySelector('.reader') !== null)
```

and at the end of the file add:

```tsx
describe('the reader in Ink whatever the theme, in a browser', () => {
  /**
   * The mechanism spec §8 rests on, in the app rather than in a probe: the
   * tokens are `light-dark()` pairs, and `color-scheme: dark` on `.reader`
   * picks their Ink half inside it even with Paper pinned on `<html>`. The
   * body is the control — outside the reader, Paper's text.
   */
  it('draws the reader in Ink on black with Paper pinned', async () => {
    document.documentElement.setAttribute('data-theme', 'paper')
    try {
      await page.viewport(1440, 900)
      mount(
        <div style={{ position: 'fixed', inset: 0 }}>
          <App work={WORK} onLeave={() => {}} />
        </div>,
      )
      await until(() => document.querySelector('.reader') !== null)
      const reader = getComputedStyle(
        document.querySelector('.reader') as HTMLElement,
      )
      expect(reader.color).toBe('rgb(236, 228, 212)')
      expect(reader.backgroundColor).toBe('rgb(0, 0, 0)')
      expect(getComputedStyle(document.body).backgroundColor).toBe(
        'rgb(0, 0, 0)',
      )
      const outside = document.createElement('p')
      document.body.append(outside)
      expect(getComputedStyle(outside).color).toBe('rgb(35, 30, 23)')
      outside.remove()
    } finally {
      document.documentElement.removeAttribute('data-theme')
    }
  })
})
```

- [ ] **Step 8: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/Routes.test.tsx src/App.test.tsx
pnpm -C web exec vitest run --project browser --reporter=default src/App.browser.test.tsx
```

Expected: FAIL — the route box and `.reader` class are absent; every `App.test.tsx` test that uses `pageArea` fails on a `null` page area; the browser test times out in `until` ("condition never held").

- [ ] **Step 9: Mark the root, the page area and the route**

In `web/src/App.tsx`, in the final `return (`, replace

```tsx
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        position: 'relative',
      }}
    >
```

with

```tsx
    <div
      className="reader"
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        position: 'relative',
      }}
    >
```

and on the page area — the `<div` whose props include `ref={pageAreaRef}` — add `className="page-area"` as its first prop, directly after `<div`.

In `web/src/Routes.tsx`, replace

```tsx
  if (!work) return <Navigate to="/" replace />
  return <App key={work} work={work} onLeave={() => void navigate('/')} />
```

with

```tsx
  if (!work) return <Navigate to="/" replace />
  // `.reader-route` blacks out the page from the first frame (spec 2026-09-17
  // §8.2): `App` draws nothing until its load answers, so its own `.reader`
  // root is not there for `styles/reader.css`'s ground rule until then.
  return (
    <div className="reader-route">
      <App key={work} work={work} onLeave={() => void navigate('/')} />
    </div>
  )
```

- [ ] **Step 10: Run the tests to verify they pass**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/Routes.test.tsx src/App.test.tsx
pnpm -C web exec vitest run --project browser --reporter=default src/App.browser.test.tsx
```

Expected: PASS, every test in all three files.

- [ ] **Step 11: Watch the new tests fail with their defects put back**

One at a time, each followed by the command that runs it, then restored and re-run green:

1. Delete `  color-scheme: dark;` from `.reader` in `reader.css` → `reader.css.test.ts` "is dark in both themes" fails, and the browser test's `reader.color` reads `rgb(35, 30, 23)`.
2. Delete `, viewport-fit=cover` from `index.html` → "are there to pad" fails.
3. Replace the ground rule's prelude with `.reader` alone → "blacks out the document" fails, and the browser test's `body` background is not black.
4. Change `96px` to `160px` in the `@media` block → "stands the strip's tiles" fails.
5. Change `--reader-panel`'s alpha to `0.9` → "are Ink's --bg and --text" fails.
6. In `Routes.tsx`, return `<App …/>` without the wrapper → "puts the reader inside its route box" fails.

- [ ] **Step 12: Typecheck, lint, commit**

```bash
pnpm -C web run typecheck
pnpm -C web run lint
git add web/index.html web/src/index.css web/src/styles/tokens.css web/src/styles/reader.css web/src/styles/reader.css.test.ts web/src/styles/shell.css web/src/index.css.test.ts web/src/Routes.tsx web/src/Routes.test.tsx web/src/App.tsx web/src/App.test.tsx web/src/App.browser.test.tsx
git commit -m "Style pass 3: the reader's black ground, its tokens and viewport properties, and the safe areas"
```

---

### Task 2: What the bars say — progress, and the title lines

**Files:**
- Create: `web/src/readerTitle.ts`, `web/src/readerTitle.test.ts`
- Modify: `web/src/modules/readouts.ts`, `web/src/modules/readouts.test.ts`

**Interfaces:**
- Produces: `ReadoutLines.progress: number`, `Math.min((slot + 1) / total, 1)` from `readoutLines`.
- Produces: `export interface TitleLines { title?: string; editionLine: string }` and `export function titleLines(detail: WorkDetail | undefined, edition: string): TitleLines` in `readerTitle.ts`.

- [ ] **Step 1: Write the failing tests**

In `web/src/modules/readouts.test.ts`, inside `describe('readoutLines', () => {`, directly after the `it('never estimates against a negative number of slots left', …)` test, add:

```ts
  /**
   * The bottom bar's progress line (spec 2026-09-17 §8.4) is the same
   * `(slot + 1) / total` the position line prints as its percentage, so the
   * two cannot disagree.
   */
  it('reports how far through the edition, as the fraction the percentage shows', () => {
    const half = lines(at('a', 3))
    expect(half.position).toContain('50%')
    expect(half.progress).toBe(0.5)
  })

  it('never reports past the end, for a slot count that has gone backwards', () => {
    const backwards = readoutLines(
      at('a', 20),
      { ...fakeResolver({ a: whole(30) }, {}), slotCount: () => 3 },
      [],
      'a',
      { here: 0, total: 0 },
      CLOCK,
    )
    expect(backwards.progress).toBe(1)
  })
```

and in the same describe's `it('answers every line the row lays out', …)`, add `'progress',` as the last entry of the expected key list, after `'notes',`.

Create `web/src/readerTitle.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import type { WorkDetail } from './library'
import { titleLines } from './readerTitle'

const detail = (over: Partial<WorkDetail> = {}): WorkDetail => ({
  id: 1,
  slug: 'dandadan-v01',
  title: 'Dandadan',
  volume: 1,
  aligned: true,
  pages: 207,
  editions: [
    { key: 'raw', label: 'Raw, monochrome', members: 1, pages: 207, missing: 0 },
    { key: 'en', label: 'English official', members: 1, pages: 200, missing: 0 },
  ],
  opens: 'raw',
  alignments: [],
  ...over,
})

describe('titleLines', () => {
  it('names the edition by its key alone until the library answers', () => {
    expect(titleLines(undefined, 'raw')).toEqual({ editionLine: 'raw' })
  })

  it('gives the work’s title, and the volume with the label of the edition on screen', () => {
    expect(titleLines(detail(), 'en')).toEqual({
      title: 'Dandadan',
      editionLine: 'Vol. 1 · English official',
    })
  })

  it('gives the label alone for a work with no volume', () => {
    expect(titleLines(detail({ volume: null }), 'raw').editionLine).toBe(
      'Raw, monochrome',
    )
  })

  it('keeps a fractional volume as it is', () => {
    expect(titleLines(detail({ volume: 10.5 }), 'raw').editionLine).toBe(
      'Vol. 10.5 · Raw, monochrome',
    )
  })

  it('falls back to the key for an edition the library did not list', () => {
    expect(titleLines(detail(), 'en-fan').editionLine).toBe('Vol. 1 · en-fan')
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project node --reporter=default src/modules/readouts.test.ts src/readerTitle.test.ts
```

Expected: FAIL — `progress` is `undefined`, the key list lacks it, and `./readerTitle` does not resolve.

- [ ] **Step 3: Implement**

In `web/src/modules/readouts.ts`, in `export interface ReadoutLines {`, after `  notes: string | undefined`, add:

```ts
  /**
   * How far through the edition, 0 to 1 — not a line of text, but the bottom
   * bar's progress line (spec 2026-09-17 §8.4), and computed here from the
   * same `(slot + 1) / total` the position line turns into its percentage so
   * the two cannot disagree. Clamped at 1 for a slot count that has gone
   * backwards, the case `time`'s `Math.max` guards.
   */
  progress: number
```

and in `readoutLines`'s returned object, after the `notes: noteCountLine(notes.here, notes.total),` line, add:

```ts
    progress: Math.min((from.slot + 1) / total, 1),
```

Create `web/src/readerTitle.ts`:

```ts
import type { WorkDetail } from './library'

/**
 * What the reader's top bar says (spec 2026-09-17 §8.3): the work's title and,
 * under it, the edition on screen — `Vol. N · <label>`, or the label alone
 * for a work with no volume.
 *
 * From the library's answer to `/api/library/works/{slug}` rather than the
 * sidecar, because that is where a metadata edit lands (`title_hand`,
 * `label_hand`); the sidecar is the file on disk. Until the library answers,
 * or if it never does, the edition's key is all there is to say, and the
 * title is absent rather than guessed.
 */
export interface TitleLines {
  title?: string
  editionLine: string
}

export function titleLines(
  detail: WorkDetail | undefined,
  edition: string,
): TitleLines {
  if (!detail) return { editionLine: edition }
  const label =
    detail.editions.find((e) => e.key === edition)?.label ?? edition
  return {
    title: detail.title,
    editionLine:
      detail.volume === null ? label : `Vol. ${detail.volume} · ${label}`,
  }
}
```

- [ ] **Step 4: Run them to verify they pass**

```bash
pnpm -C web exec vitest run --project node --reporter=default src/modules/readouts.test.ts src/readerTitle.test.ts
```

Expected: PASS.

- [ ] **Step 5: Watch them fail with their defects put back**

1. `progress: (from.slot + 1) / total,` (no clamp) → "never reports past the end" fails at `7`.
2. `progress: Math.min(from.slot / total, 1),` → "reports how far through" fails at `0.375`.
3. In `titleLines`, return `{ title: detail.title, editionLine: label }` → "gives the work's title, and the volume…" fails.
4. In `titleLines`, `?? ''` in place of `?? edition` → "falls back to the key" fails.

- [ ] **Step 6: Typecheck, lint, commit**

```bash
pnpm -C web run typecheck
pnpm -C web run lint
git add web/src/modules/readouts.ts web/src/modules/readouts.test.ts web/src/readerTitle.ts web/src/readerTitle.test.ts
git commit -m "Style pass 3: the progress fraction and the top bar's title lines"
```

---

### Task 3: The top bar

**Files:**
- Create: `web/src/TopBar.tsx`, `web/src/TopBar.test.tsx`
- Modify: `web/src/styles/reader.css`

**Interfaces:**
- Consumes: `Mode` from `web/src/pairing.ts`; `--reader-top-bar` and the `--safe-*` properties from Task 1.
- Produces: `export interface TopBarProps { title?: string; editionLine: string; mode: Mode; onMode: (mode: Mode) => void; settingsOpen: boolean; onSettings: () => void; onLeave: () => void; style?: CSSProperties }` and `export function TopBar(props: TopBarProps)`, rendering `div.reader-top-bar` holding `button.reader-back` (text `Library`), `div.reader-title` (`span.reader-title-work` only when `title` is set, then `span.reader-title-edition`), `div.reader-layout[role=group][aria-label=Layout]` with three `aria-pressed` buttons whose text is `single`, `two`, `scroll`, and `button.reader-icon-button[aria-expanded]` whose text is `Settings`. Every icon is an `<svg aria-hidden="true">`.

- [ ] **Step 1: Write the failing tests**

Create `web/src/TopBar.test.tsx`:

```tsx
import { describe, expect, it, vi } from 'vitest'
import { mount, rerender } from './testing/mount'
import { TopBar, type TopBarProps } from './TopBar'

const props = (over: Partial<TopBarProps> = {}): TopBarProps => ({
  title: 'Dandadan',
  editionLine: 'Vol. 1 · Raw, monochrome',
  mode: 'single',
  onMode: vi.fn(),
  settingsOpen: false,
  onSettings: vi.fn(),
  onLeave: vi.fn(),
  ...over,
})

/** A button by its text — the word an icon button carries, visually hidden. */
const button = (host: ParentNode, text: string) =>
  [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

describe('TopBar', () => {
  it('leaves for the library', () => {
    const onLeave = vi.fn()
    const host = mount(<TopBar {...props({ onLeave })} />)
    button(host, 'Library').click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('names the work and the edition on screen', () => {
    const host = mount(<TopBar {...props()} />)
    expect(host.querySelector('.reader-title-work')?.textContent).toBe(
      'Dandadan',
    )
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'Vol. 1 · Raw, monochrome',
    )
  })

  it('shows the edition line alone until the title arrives', () => {
    const host = mount(
      <TopBar {...props({ title: undefined, editionLine: 'raw' })} />,
    )
    expect(host.querySelector('.reader-title-work')).toBeNull()
    expect(host.querySelector('.reader-title')?.textContent).toBe('raw')
  })

  it('marks the layout on screen pressed, and the other two not', () => {
    const host = mount(<TopBar {...props({ mode: 'two' })} />)
    expect(
      ['single', 'two', 'scroll'].map((m) =>
        button(host, m).getAttribute('aria-pressed'),
      ),
    ).toEqual(['false', 'true', 'false'])
  })

  it('asks for the layout a button names', () => {
    const onMode = vi.fn()
    const host = mount(<TopBar {...props({ onMode })} />)
    button(host, 'scroll').click()
    expect(onMode).toHaveBeenCalledWith('scroll')
  })

  it('opens settings, and says whether they are open', () => {
    const onSettings = vi.fn()
    const host = mount(<TopBar {...props({ onSettings })} />)
    expect(button(host, 'Settings').getAttribute('aria-expanded')).toBe(
      'false',
    )
    button(host, 'Settings').click()
    expect(onSettings).toHaveBeenCalledOnce()
    rerender(<TopBar {...props({ onSettings, settingsOpen: true })} />)
    expect(button(host, 'Settings').getAttribute('aria-expanded')).toBe('true')
  })

  it('docks at the reader’s top edge, its height padded by the top safe area, over the page', () => {
    const host = mount(<TopBar {...props()} />)
    const bar = host.firstElementChild as HTMLElement
    expect(bar.classList.contains('reader-top-bar')).toBe(true)
    expect(bar.style.position).toBe('absolute')
    expect(bar.style.top).toBe('0px')
    expect(bar.style.height).toBe(
      'calc(var(--reader-top-bar) + var(--safe-top))',
    )
    expect(Number(bar.style.zIndex)).toBe(3)
  })

  it('takes the fade it is handed', () => {
    const host = mount(<TopBar {...props({ style: { opacity: 0 } })} />)
    expect((host.firstElementChild as HTMLElement).style.opacity).toBe('0')
  })

  it('keeps its five icons out of the accessibility tree', () => {
    const host = mount(<TopBar {...props()} />)
    const icons = [...host.querySelectorAll('svg')]
    expect(icons).toHaveLength(5)
    for (const icon of icons) expect(icon.getAttribute('aria-hidden')).toBe('true')
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/TopBar.test.tsx
```

Expected: FAIL — `./TopBar` does not resolve.

- [ ] **Step 3: Implement `TopBar`**

Create `web/src/TopBar.tsx`:

```tsx
import type { CSSProperties, ReactNode } from 'react'
import type { Mode } from './pairing'

/** The three layouts, in the order the control draws them. */
const MODES: readonly Mode[] = ['single', 'two', 'scroll']

/**
 * Each layout drawn rather than named (spec 2026-09-17 §8.3): at 390 px the
 * three words would leave the title about 70 px. The word stays in the button
 * as visually hidden text, so its accessible name — and the `textContent` the
 * reader's tests find it by — is still `single`, `two` or `scroll`.
 */
const ICONS: Record<Mode, ReactNode> = {
  single: <rect x="7" y="4" width="10" height="16" />,
  two: (
    <>
      <rect x="3" y="5" width="8" height="14" />
      <rect x="13" y="5" width="8" height="14" />
    </>
  ),
  scroll: (
    <>
      <rect x="7" y="2" width="10" height="9" />
      <rect x="7" y="13" width="10" height="9" />
    </>
  ),
}

// Docked to the reader's top edge like the bottom bar to its bottom, and for
// the same reason: absolutely positioned against `App.tsx`'s root, so showing
// it never resizes the page area. The height is the bar's 52 px plus the top
// safe area, which the padding keeps clear. `zIndex: 3` stands it over the
// page's own layers — `Annotations`' overlay is at 2.
const barStyle: CSSProperties = {
  position: 'absolute',
  top: 0,
  left: 0,
  right: 0,
  zIndex: 3,
  boxSizing: 'border-box',
  height: 'calc(var(--reader-top-bar) + var(--safe-top))',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--space-2)',
  padding:
    'var(--safe-top) calc(var(--space-2) + var(--safe-right)) 0 calc(var(--space-3) + var(--safe-left))',
}

export interface TopBarProps {
  /** The work's title, from the library — absent until it answers, or if it never does. */
  title?: string
  /** `Vol. N · <label>` for the edition on screen, or its key alone — `readerTitle.ts`. */
  editionLine: string
  mode: Mode
  onMode: (mode: Mode) => void
  /** Whether the settings panel is open, which ⚙ toggles. */
  settingsOpen: boolean
  onSettings: () => void
  onLeave: () => void
  /** `Chrome`'s fade, applied to the bar as one piece. */
  style?: CSSProperties
}

/**
 * The reader's top bar (spec 2026-09-17 §8.3): the way back to the library,
 * what is being read, the layout, and settings. `Chrome` shows it with the
 * bottom bar and takes both away together, and draws neither while
 * correcting, when `CorrectionBanner` has the top edge.
 *
 * It holds no state: the layout is `App.tsx`'s `layout` preference, the same
 * slice the settings panel's Layout section writes, and ⚙ flips the same flag
 * the `s` key does.
 */
export function TopBar({
  title,
  editionLine,
  mode,
  onMode,
  settingsOpen,
  onSettings,
  onLeave,
  style,
}: TopBarProps) {
  return (
    <div className="reader-top-bar" style={{ ...barStyle, ...style }}>
      <button type="button" className="reader-back" onClick={onLeave}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M15 5l-7 7 7 7" />
        </svg>
        Library
      </button>
      <div className="reader-title">
        {title !== undefined && (
          <span className="reader-title-work">{title}</span>
        )}
        <span className="reader-title-edition">{editionLine}</span>
      </div>
      <div className="reader-layout" role="group" aria-label="Layout">
        {MODES.map((m) => (
          <button
            key={m}
            type="button"
            aria-pressed={m === mode}
            onClick={() => onMode(m)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              {ICONS[m]}
            </svg>
            <span className="visually-hidden">{m}</span>
          </button>
        ))}
      </div>
      <button
        type="button"
        className="reader-icon-button"
        aria-expanded={settingsOpen}
        onClick={onSettings}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1" />
        </svg>
        <span className="visually-hidden">Settings</span>
      </button>
    </div>
  )
}
```

If `pnpm -C web run lint` refuses `role="group"` on the `div` (`useSemanticElements`), use a `<fieldset className="reader-layout" aria-label="Layout">` instead and add `margin: 0; min-width: 0; padding: 0;` to `.reader-layout` in Step 4; the test finds the buttons by text either way.

- [ ] **Step 4: Style it**

Append to `web/src/styles/reader.css`:

```css

/* Visually hidden, still read out, and still the button's text: the words
 * behind the top bar's icons (`TopBar.tsx`). */
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

/* The top bar (spec 2026-09-17 §8.3). Its geometry is `TopBar.tsx`'s. */
.reader-top-bar {
  border-bottom: 1px solid var(--reader-hair);
  background: var(--reader-panel);
  color: var(--text);
}

.reader-top-bar svg {
  width: 20px;
  height: 20px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.reader-back {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  min-height: 36px;
  padding: 0 var(--space-2) 0 0;
  border: 0;
  background: none;
  color: var(--muted);
  font: inherit;
  font-size: var(--text-ui);
  white-space: nowrap;
  cursor: pointer;
}

.reader-back:hover {
  color: var(--text);
}

.reader-title {
  flex: 1;
  min-width: 0;
  line-height: 1.2;
}

.reader-title-work,
.reader-title-edition {
  display: block;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.reader-title-work {
  font-family: var(--font-title);
  font-size: var(--text-body);
  font-weight: 600;
}

.reader-title-edition {
  color: var(--muted);
  font-size: var(--text-meta);
}

.reader-layout {
  display: inline-flex;
  border: 1px solid var(--reader-hair);
  border-radius: var(--radius-control);
}

.reader-layout button,
.reader-icon-button {
  display: inline-grid;
  place-items: center;
  padding: 0;
  border: 0;
  background: none;
  cursor: pointer;
}

.reader-layout button {
  min-width: 36px;
  min-height: 32px;
  color: var(--muted);
}

.reader-layout button[aria-pressed="true"] {
  background: var(--text);
  color: var(--bg);
}

.reader-icon-button {
  min-width: 40px;
  min-height: 40px;
  color: var(--text);
}

.reader :focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

/* 44 px for a finger (spec 2026-09-17 §8.3). The bar is 52 px, so they fit. */
@media (pointer: coarse) {
  .reader-back,
  .reader-layout button,
  .reader-icon-button {
    min-width: 44px;
    min-height: 44px;
  }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/TopBar.test.tsx
pnpm -C web exec vitest run --project node --reporter=default src/colourLiterals.test.ts
```

Expected: PASS.

- [ ] **Step 6: Watch them fail with their defects put back**

1. `aria-pressed={m !== mode}` → "marks the layout on screen pressed" fails.
2. `onClick={() => onMode(mode)}` → "asks for the layout a button names" fails with `'single'`.
3. Drop `aria-expanded={settingsOpen}` → "opens settings, and says whether they are open" fails.
4. Render `<span className="reader-title-work">{title}</span>` unconditionally → "shows the edition line alone" fails.
5. Drop `...style` from the bar → "takes the fade it is handed" fails.

- [ ] **Step 7: Typecheck, lint, commit**

```bash
pnpm -C web run typecheck
pnpm -C web run lint
git add web/src/TopBar.tsx web/src/TopBar.test.tsx web/src/styles/reader.css
git commit -m "Style pass 3: the reader's top bar, as a component"
```

---

### Task 4: The bars in the reader

`Chrome` becomes the top bar and one bottom bar; `App.tsx` loses its layout row and feeds the bars; the strip's rings take tokens.

**Files:**
- Modify: `web/src/Chrome.tsx` (rewritten), `web/src/Chrome.test.tsx` (rewritten), `web/src/Filmstrip.tsx`, `web/src/Filmstrip.test.tsx`, `web/src/App.tsx`, `web/src/App.test.tsx`, `web/src/App.browser.test.tsx`, `web/src/modules/layout.ts` (a comment), `web/src/styles/reader.css`, `web/src/colourLiterals.test.ts`

**Interfaces:**
- Consumes: `TopBar` and `TopBarProps` (Task 3); `titleLines` and `ReadoutLines.progress` (Task 2); `--strip-height`, `--progress-height`, `--readout-height`, `--reader-bottom-bar`, `--safe-*` (Task 1).
- Produces: `ChromeProps` gains `progress: number`, `title?: string`, `editionLine: string`, `mode: Mode`, `onMode: (mode: Mode) => void`, `settingsOpen: boolean`, `onSettings: () => void`. `Chrome` renders `TopBar` (not while `correcting`) and `div.reader-bottom-bar` whose three children are the strip (`style.height` `var(--strip-height)`), `div.reader-progress[aria-hidden]` holding `div.reader-progress-fill`, and `div.reader-readout[role=status]`.

- [ ] **Step 1: Rewrite `Chrome.test.tsx` for the two bars**

Replace the whole of `web/src/Chrome.test.tsx` with:

```tsx
import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Chrome, type ChromeProps } from './Chrome'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { mount, rerender } from './testing/mount'

/**
 * `Chrome` is the two bars — `TopBar.tsx` (its own test) owns the top bar's
 * controls, `Filmstrip.tsx` (its own test) owns tile layout,
 * `modules/motion.ts`'s `scheduleFade` (pinned in `motion.test.ts`) owns the
 * frame-scheduling arithmetic, and `modules/readouts.ts`'s `readoutRow`
 * (pinned in `readouts.test.ts`) owns the join. What is only pinned here is
 * what `Chrome` itself does: the `visible` unmount gate, where each bar docks
 * and what goes in it, the top bar standing down while correcting, wiring the
 * fade through a real `useEffect` across a hide/show cycle, the hover-preview
 * state it owns, and passing the right, gated fields to `readoutRow`.
 */

// jsdom has no layout engine and implements no `scrollIntoView` —
// `Filmstrip.tsx`'s own centring effect calls it on mount. Stubbed only so
// that effect does not throw; what it centres on is `Filmstrip`'s test's
// concern.
Element.prototype.scrollIntoView ??= () => {}

const slots: Position[] = [{ page: 0 }, { page: 1, half: 'L' }, { page: 2 }]

const groups: Group[] = [{ slots: [0] }, { slots: [1] }, { slots: [2] }]

const baseProps: ChromeProps = {
  visible: true,
  motion: { animate: false, ms: 0 },
  readouts: {
    position: true,
    time: true,
    chapter: true,
    health: true,
    notes: true,
  },
  position: 'slot 1 / 3',
  time: '2m read',
  chapter: 'c001',
  health: 'sound pair',
  notes: '3 notes',
  progress: 0.25,
  title: 'Volume One',
  editionLine: 'Vol. 1 · Raw',
  mode: 'single',
  onMode: vi.fn(),
  settingsOpen: false,
  onSettings: vi.fn(),
  reading: 'ltr',
  work: 'work1',
  edition: 'ed1',
  slots,
  groups,
  currentIndex: 0,
  onNavigate: vi.fn(),
  onLeave: vi.fn(),
}

const buttonNamed = (host: ParentNode, text: string) =>
  [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement | undefined

const bottomBar = (host: HTMLElement) =>
  host.querySelector('.reader-bottom-bar') as HTMLElement

describe('Chrome', () => {
  it('renders nothing while hidden — an unmount, not merely invisible chrome', () => {
    const host = mount(<Chrome {...baseProps} visible={false} />)
    expect(host.children.length).toBe(0)
  })

  it('renders both bars while visible', () => {
    const host = mount(<Chrome {...baseProps} />)
    // One button per group's tile, plus the top bar's five: Library, the
    // three layouts and Settings.
    expect(host.querySelectorAll('button').length).toBe(groups.length + 5)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    expect(host.querySelector('[role="status"]')).not.toBeNull()
  })

  it('gives the reader a way back to the library', () => {
    const onLeave = vi.fn()
    const host = mount(<Chrome {...baseProps} onLeave={onLeave} />)
    buttonNamed(host, 'Library')?.click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  /**
   * Spec 2026-09-17 §8.3, reversing O65's dock above the strip: the way back
   * is in the top bar now, which is inside this component, so it keeps the
   * fade and stands down while correcting as O65's button did.
   * `App.browser.test.tsx` measures that nothing covers it.
   */
  it('puts the way back in the top bar, on the reader’s top edge', () => {
    const host = mount(<Chrome {...baseProps} />)
    const bar = host.querySelector('.reader-top-bar') as HTMLElement
    expect(bar.style.top).toBe('0px')
    expect(bar.contains(buttonNamed(host, 'Library') as Node)).toBe(true)
  })

  it('hands the top bar its title, edition line, layout and settings', () => {
    const onMode = vi.fn()
    const onSettings = vi.fn()
    const host = mount(
      <Chrome
        {...baseProps}
        mode="two"
        settingsOpen
        onMode={onMode}
        onSettings={onSettings}
      />,
    )
    expect(host.querySelector('.reader-title')?.textContent).toBe(
      'Volume OneVol. 1 · Raw',
    )
    expect(buttonNamed(host, 'two')?.getAttribute('aria-pressed')).toBe('true')
    expect(buttonNamed(host, 'Settings')?.getAttribute('aria-expanded')).toBe(
      'true',
    )
    buttonNamed(host, 'scroll')?.click()
    expect(onMode).toHaveBeenCalledWith('scroll')
    buttonNamed(host, 'Settings')?.click()
    expect(onSettings).toHaveBeenCalledOnce()
  })

  it('draws no top bar while correcting, when the banner has the top edge', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(buttonNamed(host, 'Library')).toBeUndefined()
    expect(bottomBar(host)).not.toBeNull()
  })

  it('docks one bottom bar to the reader’s bottom edge, padded by the safe areas', () => {
    const host = mount(<Chrome {...baseProps} />)
    const bar = bottomBar(host)
    expect(bar.style.position).toBe('absolute')
    expect([bar.style.left, bar.style.right, bar.style.bottom]).toEqual([
      '0px',
      '0px',
      '0px',
    ])
    expect(Number(bar.style.zIndex)).toBe(3)
    expect(bar.style.paddingBottom).toBe('var(--safe-bottom)')
    expect(bar.style.paddingLeft).toBe('var(--safe-left)')
    expect(bar.style.paddingRight).toBe('var(--safe-right)')
  })

  it('stacks the strip, the progress line and the readout row inside it, in that order', () => {
    const host = mount(<Chrome {...baseProps} />)
    const [strip, progress, readout] = [
      ...bottomBar(host).children,
    ] as HTMLElement[]
    expect(strip.style.height).toBe('var(--strip-height)')
    expect(strip.querySelectorAll('button')).toHaveLength(groups.length)
    expect(progress.classList.contains('reader-progress')).toBe(true)
    expect(progress.getAttribute('aria-hidden')).toBe('true')
    expect(progress.style.height).toBe('var(--progress-height)')
    expect(readout.getAttribute('role')).toBe('status')
    expect(readout.style.height).toBe('var(--readout-height)')
  })

  it('fills the progress line to the fraction it is handed', () => {
    const host = mount(<Chrome {...baseProps} progress={0.25} />)
    const fill = host.querySelector('.reader-progress-fill') as HTMLElement
    expect(fill.style.width).toBe('25%')
    rerender(<Chrome {...baseProps} progress={1} />)
    expect(fill.style.width).toBe('100%')
  })

  it('joins the readout row in position/health/time/chapter/notes order', () => {
    const host = mount(<Chrome {...baseProps} />)
    expect(host.querySelector('[role="status"]')?.textContent).toBe(
      'slot 1 / 3 · sound pair · 2m read · c001 · 3 notes',
    )
  })

  it('drops every readout switched off in prefs, leaving nothing to join', () => {
    const host = mount(
      <Chrome
        {...baseProps}
        readouts={{
          position: false,
          time: false,
          chapter: false,
          health: false,
          notes: false,
        }}
      />,
    )
    expect(host.querySelector('[role="status"]')?.textContent).toBe('')
  })

  it('shows no hover preview before any tile is focused, even while correcting', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    expect(host.querySelector('img[alt=""]')).toBeNull()
  })

  it('reveals the hovered tile’s page as a preview while correcting, labelled with its half', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    const tiles = host.querySelectorAll('button')

    // Tile 0 is slot 0, a plain page with no half.
    act(() => tiles[0]?.focus())
    let img = host.querySelector('img[alt=""]') as HTMLImageElement
    expect(img).not.toBeNull()
    expect(img.getAttribute('src')).toBe('/api/works/work1/ed1/pages/0')
    expect(img.nextElementSibling?.textContent).toBe('page 1')

    // Tile 1 is slot 1, one half ('L') of a joined spread.
    act(() => tiles[1]?.focus())
    img = host.querySelector('img[alt=""]') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('/api/works/work1/ed1/pages/1')
    expect(img.nextElementSibling?.textContent).toBe('page 2 L')

    // Blurring the tile clears the preview rather than leaving it stuck.
    act(() => tiles[1]?.blur())
    expect(host.querySelector('img[alt=""]')).toBeNull()
  })

  it('stands the hover preview just above the bottom bar', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    act(() => host.querySelectorAll('button')[0]?.focus())
    const preview = host.querySelector('img[alt=""]')
      ?.parentElement as HTMLElement
    expect(preview.style.bottom).toBe(
      'calc(var(--safe-bottom) + var(--reader-bottom-bar) + 8px)',
    )
  })

  describe('the fade, driven through a real useEffect across a hide/show cycle', () => {
    // A stubbed requestAnimationFrame/cancelAnimationFrame pair, the same
    // shape `motion.test.ts` uses for `scheduleFade` itself: frames are
    // recorded rather than auto-fired, so a test controls exactly when a
    // scheduled frame lands.
    let pending: Map<number, FrameRequestCallback>
    let nextId: number

    beforeEach(() => {
      pending = new Map()
      nextId = 0
      vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        nextId += 1
        pending.set(nextId, cb)
        return nextId
      })
      vi.stubGlobal('cancelAnimationFrame', (id: number) => {
        pending.delete(id)
      })
    })

    afterEach(() => {
      vi.unstubAllGlobals()
    })

    const flush = () => {
      const callbacks = [...pending.values()]
      pending.clear()
      for (const cb of callbacks) cb(0)
    }

    /** Both bars' opacity — they fade as one. */
    const opacities = (host: HTMLElement) => [
      (host.querySelector('.reader-top-bar') as HTMLElement).style.opacity,
      bottomBar(host).style.opacity,
    ]

    it('starts fully shown, with no opacity declared, when animation is off', () => {
      const host = mount(<Chrome {...baseProps} />)
      expect(opacities(host)).toEqual(['', ''])
      expect(pending.size).toBe(0)
    })

    it('paints both bars at opacity 0 on first show, then flips them to 1 once the scheduled frame lands', () => {
      const host = mount(
        <Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />,
      )
      expect(opacities(host)).toEqual(['0', '0'])
      expect(pending.size).toBe(1)

      act(() => flush())
      expect(opacities(host)).toEqual(['1', '1'])
    })

    it('resets shown on hide and repeats the same one-frame-late fade on the next show — no flash', () => {
      const host = mount(
        <Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />,
      )
      act(() => flush())
      expect(opacities(host)).toEqual(['1', '1'])

      // Hide: the component instance survives (only its return value goes to
      // null), and the effect's own `!visible` branch is what resets `shown`
      // rather than it surviving stale from before.
      rerender(
        <Chrome
          {...baseProps}
          motion={{ animate: true, ms: 80 }}
          visible={false}
        />,
      )
      expect(host.children.length).toBe(0)

      // Show again, on the same instance: if `shown` had survived the hide
      // unreset, this would paint straight at opacity 1 — a flash of chrome
      // already fully visible with nothing having animated it there.
      rerender(<Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />)
      expect(opacities(host)).toEqual(['0', '0'])

      act(() => flush())
      expect(opacities(host)).toEqual(['1', '1'])
    })
  })
})
```

- [ ] **Step 2: Update `Filmstrip.test.tsx`'s two rings**

In `web/src/Filmstrip.test.tsx`, replace

```ts
    expect(btns[1].style.borderColor).toBe('rgb(255, 255, 255)')
```

with

```ts
    expect(btns[1].style.borderColor).toBe('var(--accent)')
```

and replace

```ts
    expect(btns[2].style.borderColor).toBe('rgb(242, 177, 52)')
```

with

```ts
    expect(btns[2].style.borderColor).toBe('var(--warning)')
```

- [ ] **Step 3: Write the failing `App.test.tsx` tests**

In `web/src/App.test.tsx`:

1. In the `ports` object passed to `vi.hoisted`, after `  wasmResolver: vi.fn(),`, add:

```ts
  // `library.ts`'s `work()`: the top bar's title and the editions' labels
  // (spec 2026-09-17 §8.3), fetched once beside the load.
  workDetail: vi.fn(),
```

2. Directly after the `vi.mock('./api', …)` call, add:

```ts
vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  work: ports.workDetail,
}))
```

3. Directly after `const emptyNotes = (): NotesDoc => ({ work: WORK, notes: [] })`, add:

```ts
/** `/api/library/works/{slug}` for `WORK`, as far as the top bar reads it. */
const libraryDetail = (): WorkDetail => ({
  id: 1,
  slug: WORK,
  title: 'Volume One',
  volume: 1,
  aligned: true,
  pages: 6,
  editions: [
    { key: 'raw', label: 'Raw, monochrome', members: 1, pages: 6, missing: 0 },
    { key: 'en', label: 'English official', members: 1, pages: 6, missing: 0 },
  ],
  opens: 'raw',
  alignments: [],
})
```

and, directly after the line `import { fakeResolver, whole } from './fixtures'` at the top of the file, add:

```ts
import { Unauthenticated } from './api'
import type { WorkDetail } from './library'
```

`./api` is mocked with `importOriginal` spread in, so this `Unauthenticated` is the real class — the one `App.tsx` tests with `instanceof`.

4. In the top-level `beforeEach`, after `  ports.wasmResolver.mockImplementation(async () => resolver)`, add:

```ts
  ports.workDetail.mockResolvedValue(libraryDetail())
```

5. In `it('the mode buttons change what the page area draws', …)`, replace

```ts
    const host = await open()
    const two = [...host.querySelectorAll('button')].find(
```

with

```ts
    const host = await open()
    // In the top bar since style pass ③, which `Tab` brings in with the strip.
    press('Tab')
    const two = [...host.querySelectorAll('button')].find(
```

6. In `it('a reader key stands down while focus is in a control', …)`, which takes the page's first button — the `single` button above the page until now, and nothing at all once it leaves — replace

```ts
    const host = await open()
    const button = host.querySelector('button')
```

with

```ts
    const host = await open()
    // The reader draws no button until `Tab` shows the bars (style pass ③).
    press('Tab')
    const button = host.querySelector('button')
```

7. Directly after `it("the chrome's Library button calls the onLeave prop", …)` — inside the same `describe` — add:

```tsx
  it('shows the layout control only with the chrome, not above the page', async () => {
    const host = await open()
    expect(button(host, 'two')).toBeUndefined()
    press('Tab')
    expect(button(host, 'two')).toBeDefined()
  })

  it('names the work and the edition on screen in the top bar, and follows the edition', async () => {
    const host = await open()
    press('Tab')
    expect(host.querySelector('.reader-title-work')?.textContent).toBe(
      'Volume One',
    )
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'Vol. 1 · Raw, monochrome',
    )
    press('2')
    await settle()
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'Vol. 1 · English official',
    )
    expect(ports.workDetail).toHaveBeenCalledOnce()
    expect(ports.workDetail).toHaveBeenCalledWith(WORK)
  })

  it('names the edition by its key when the library does not answer, and says why in the console', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    ports.workDetail.mockRejectedValue(new Error('no server'))
    const host = await open()
    press('Tab')
    expect(host.querySelector('.reader-title')?.textContent).toBe('raw')
    expect(logged).toHaveBeenCalledWith(expect.any(Error))
    logged.mockRestore()
  })

  it('says nothing in the console when the title fetch finds the session gone', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    ports.workDetail.mockRejectedValue(new Unauthenticated())
    const host = await open()
    press('Tab')
    expect(host.querySelector('.reader-title')?.textContent).toBe('raw')
    expect(logged).not.toHaveBeenCalled()
    logged.mockRestore()
  })

  it('⚙ opens and closes the settings panel', async () => {
    const host = await open()
    press('Tab')
    click(button(host, 'Settings'))
    expect(dialog(host, 'Settings')).not.toBeNull()
    click(button(host, 'Settings'))
    expect(dialog(host, 'Settings')).toBeNull()
  })

  it('fills the progress line to where the reader stands', async () => {
    const host = await open()
    press('Tab')
    // Stored progress is `raw` slot 2 of six: (2 + 1) / 6.
    expect(
      (host.querySelector('.reader-progress-fill') as HTMLElement).style.width,
    ).toBe('50%')
  })
```

- [ ] **Step 4: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/Chrome.test.tsx src/Filmstrip.test.tsx src/App.test.tsx
```

Expected: FAIL — `Chrome.test.tsx` on the missing `.reader-top-bar` and `.reader-bottom-bar` (and `tsc` would refuse its new props); `Filmstrip.test.tsx`'s two rings; `App.test.tsx`'s new tests, and "shows the layout control only with the chrome" finding `two` before `Tab`.

- [ ] **Step 5: Rewrite `Chrome.tsx`**

Replace the whole of `web/src/Chrome.tsx` with:

```tsx
import { useEffect, useState, type CSSProperties } from 'react'
import { pageUrl } from './api'
import { Filmstrip, type FilmstripProps } from './Filmstrip'
import {
  fadeStyle,
  isAnimating,
  scheduleFade,
  type MotionPrefs,
} from './modules/motion'
import { readoutRow, type ReadoutsPrefs } from './modules/readouts'
import type { Mode } from './pairing'
import { TopBar } from './TopBar'

/// How large the hover preview may stand, as a fraction of the viewport.
///
/// Sized in `vh` rather than pixels because this is the one surface where a
/// reader is comparing two drawings rather than recognising one, so it should
/// take whatever the screen can give it. It draws the *page* (`pageUrl`), not
/// the tile — at 160px a thumbnail cannot answer "is this the same drawing",
/// which is the whole question a correction asks.
///
/// One request per hovered tile, and only while correcting. That is not the
/// cold-strip burst O9 and O11 discuss: a reader hovers a handful of
/// candidates, one at a time, at human speed.
const previewMaxHeight = '55vh'

// The bottom bar (spec 2026-09-17 §8.4): one full-width band docked to the
// reader's bottom edge — the strip, the progress line and the readout row,
// padded by the safe areas. It replaced a strip floating 16 px in from each
// side over a readout band of its own.
//
// `position: absolute` against `App.tsx`'s root, which carries `position:
// relative` for exactly this, rather than a flex child of the page-area
// column: an absolutely positioned box is removed from that flow entirely, so
// mounting or unmounting it cannot change the page area's size — the one
// property this component exists to guarantee. `zIndex: 3` stands it over the
// page's own layers (`Annotations`' overlay is at 2); the docked panels share
// the figure and dock clear of it (`--panel-bottom`, `styles/reader.css`).
const bottomBarStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  bottom: 0,
  zIndex: 3,
  paddingLeft: 'var(--safe-left)',
  paddingRight: 'var(--safe-right)',
  paddingBottom: 'var(--safe-bottom)',
}

// The strip: its tiles at the breakpoint's size plus the rail's padding and
// the tiles' borders. `--strip-tile` in `styles/reader.css` has the two sizes
// and why each — 160 px, the thumbnail's own height, from 900 px up, and 96 on
// a phone — and a height that depends on the viewport is a custom property
// because an inline style cannot hold a media query.
const stripStyle: CSSProperties = { height: 'var(--strip-height)' }

// How far through the edition, as a line across the bar: `readoutLines`'
// `progress`, the fraction the readout's percentage prints, so the two cannot
// disagree.
const progressStyle: CSSProperties = {
  height: 'var(--progress-height)',
  margin: '0 var(--space-3)',
}

// The hover preview: a page-sized look at the tile under the pointer, while
// correcting. Centred horizontally and standing 8 px clear of the bottom bar,
// so a reader's eye travels the shortest distance between the candidate and
// the page they are comparing it against. Before style pass ③ this band also
// held the `Library` button (O65), which is why that button stood down while
// correcting; it is in the top bar now.
const previewStyle: CSSProperties = {
  position: 'absolute',
  left: 'var(--space-4)',
  right: 'var(--space-4)',
  bottom: 'calc(var(--safe-bottom) + var(--reader-bottom-bar) + 8px)',
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  gap: 4,
  pointerEvents: 'none',
}

const previewImgStyle: CSSProperties = {
  maxHeight: previewMaxHeight,
  maxWidth: '100%',
  width: 'auto',
  objectFit: 'contain',
}

// The readout row, the bar's last line.
//
// `height` alone is not enough to keep the row from reflowing: a row that
// wraps to two lines grows past a fixed `height` rather than being
// constrained by it. `whiteSpace: 'nowrap'` forbids the wrap outright, and
// `overflow: 'hidden'`/`textOverflow: 'ellipsis'` are what happens instead if
// the joined text ever runs wider than the bar. `readoutRow`
// (`modules/readouts.ts`) is the other half of this: a fixed segment order, so
// which readouts are enabled or have something to say right now changes the
// row's *length*, never the order pieces appear in.
const readoutStyle: CSSProperties = {
  height: 'var(--readout-height)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 4,
  padding: '0 12px',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
}

export interface ChromeProps extends FilmstripProps {
  /** Whether the two bars are shown at all. */
  visible: boolean
  /** The `motion` module's current prefs — drives the fade this component plays on showing. */
  motion: MotionPrefs
  /** Which of the readout lines are switched on — see `modules/readouts.ts`. */
  readouts: ReadoutsPrefs
  /**
   * The readout lines' text, already computed by `App.tsx` from
   * `modules/readouts.ts`'s pure functions — `Chrome` only lays them out, it
   * does not derive any of them itself. `position` and `chapter` are always
   * strings (`positionLine`/`chapterReadout` never answer "nothing to say");
   * `time` is a string too, once a session exists, but `health` is commonly
   * `undefined` — a sound pair with a counterpart says nothing, by design.
   *
   * Each is display-ready already: a percentage, a 1-based slot, a chapter
   * label — none of it is a value that could leak into storage, a URL or the
   * server the way a raw zero-based slot could.
   */
  position: string
  time?: string
  chapter: string
  health?: string
  /** The note count — derived by `modules/annotations.ts`, absent where the work has none. */
  notes?: string
  /** How far through the edition, 0 to 1 — `readoutLines`' `progress`. */
  progress: number
  /** The work's title, for the top bar; absent until the library answers — `readerTitle.ts`. */
  title?: string
  /** The top bar's second line: `Vol. N · <label>`, or the edition's key — `readerTitle.ts`. */
  editionLine: string
  /** The layout on screen, and the top bar's way to change it. */
  mode: Mode
  onMode: (mode: Mode) => void
  /** Whether the settings panel is open; ⚙ toggles it. */
  settingsOpen: boolean
  onSettings: () => void
  /** Back to the library grid — spec C1 §6. */
  onLeave: () => void
}

/**
 * The reader's two bars — the top bar (`TopBar.tsx`) and the bottom bar of
 * strip, progress line and readout row — brought in and taken away together
 * by `Tab` (`keys.ts`'s `{ kind: 'chrome' }`), never separately (spec
 * 2026-09-17 §8.1). Returns `null` while hidden, rather than e.g.
 * `visibility: hidden`, so the hidden state really is "no chrome at all" —
 * edge-to-edge reading with nothing on screen but the page — and not merely
 * invisible chrome still doing layout work.
 *
 * That unmount is also why only *showing* the chrome fades and hiding it
 * does not: an element that has already unmounted has nothing left on
 * screen to animate out. The unmount is also what keeps a reader who never
 * presses `Tab` from ever fetching a single filmstrip thumbnail — not
 * `Filmstrip`'s `loading="lazy"` on those thumbnails, which does the same
 * job for a strip that's already on screen (see `Filmstrip.tsx`) but was
 * measured directly, on this project, to not defer a thumbnail's request at
 * all in the environment tested (docs/status.md, "the tiles'
 * `loading=\"lazy\"` does not defer thumbnail requests"). So keeping
 * `Chrome` mounted and fading its opacity to zero instead would trade the
 * unmount's real protection for `loading="lazy"`'s measured-absent one.
 * If this is revisited, what would settle it is a network trace comparing
 * today's `Tab`-press burst against a mounted-with-`opacity: 0` prototype.
 *
 * **While correcting, the top bar is not drawn**: `CorrectionBanner` takes
 * the top edge (§8.3). That keeps O65's rule that the way back to the
 * library stands down during a correction — a transient, task-focused state
 * with nothing for "back to the library" to offer — while `L` (`keys.ts`)
 * still leaves: a pending correction has not been written to anything, so
 * leaving loses nothing.
 *
 * The readout row shows whichever of position, alignment health, reading
 * time, chapter and notes `readouts` has switched on, joined by `readoutRow`
 * in that fixed order — see `readoutRow`'s own doc for why.
 */
export function Chrome({
  visible,
  motion,
  readouts,
  position,
  time,
  chapter,
  health,
  notes,
  progress,
  title,
  editionLine,
  mode,
  onMode,
  settingsOpen,
  onSettings,
  onLeave,
  ...filmstrip
}: ChromeProps) {
  const animating = isAnimating(motion)

  // Which tile the pointer is over, while correcting. Held here rather than
  // in `Filmstrip` because the preview is drawn outside the bottom bar — see
  // the preview's JSX below.
  const [preview, setPreview] = useState<{ page: number; half?: 'L' | 'R' }>()

  // Whether the fade has settled on fully shown. Starts at `!animating` so a
  // reader with animation off, or the duration zeroed, never sees even a
  // single-frame flash at `opacity: 0` — `isAnimating` treats both of those
  // as no animation at all, and this honours that rather than reintroducing
  // a flash of its own.
  const [shown, setShown] = useState(!animating)

  // `Chrome` is always mounted from `App.tsx`'s side — only this function's
  // own `return null` below changes, not whether the `<Chrome>` element
  // itself is torn down — so `shown` survives a hide/show cycle unless reset
  // here. `scheduleFade` (`modules/motion.ts`) is the branch: hidden, or not
  // animating, it settles `shown` immediately; visible and animating, it
  // schedules the frame that flips `shown` to `true` a moment after the
  // render below first paints at `opacity: 0`, which is what gives the
  // already-declared `transition` something to animate. Its returned cleanup
  // is what makes a rapid re-toggle safe. Extracted to `motion.ts` so it can
  // be tested directly against a stubbed `requestAnimationFrame` — see
  // `motion.test.ts`.
  useEffect(
    () => scheduleFade(visible, animating, setShown),
    [visible, animating],
  )

  if (!visible) return null

  const fade: CSSProperties = fadeStyle(motion, shown)
  const row = readoutRow({
    position: readouts.position ? position : undefined,
    time: readouts.time ? time : undefined,
    chapter: readouts.chapter ? chapter : undefined,
    health: readouts.health ? health : undefined,
    notes: readouts.notes ? notes : undefined,
  })
  return (
    <>
      {/*
        The hover preview, a sibling of the bars rather than inside the
        bottom one, so the bar's own box does not clip it. `pointerEvents:
        'none'`, so moving toward it never takes the hover off the tile that
        summoned it. Only while correcting — `onPreview` is only wired then
        (see `Filmstrip`), but the condition is written out anyway.
      */}
      {filmstrip.correcting && preview && (
        <div style={{ ...previewStyle, ...fade }}>
          <img
            className="reader-preview-img"
            src={pageUrl(filmstrip.work, filmstrip.edition, preview.page)}
            alt=""
            style={previewImgStyle}
          />
          <span className="reader-preview-label">
            {preview.half
              ? `page ${preview.page + 1} ${preview.half}`
              : `page ${preview.page + 1}`}
          </span>
        </div>
      )}
      {!filmstrip.correcting && (
        <TopBar
          title={title}
          editionLine={editionLine}
          mode={mode}
          onMode={onMode}
          settingsOpen={settingsOpen}
          onSettings={onSettings}
          onLeave={onLeave}
          style={fade}
        />
      )}
      <div className="reader-bottom-bar" style={{ ...bottomBarStyle, ...fade }}>
        <div style={stripStyle}>
          {/* `visible` is always true here — `Chrome` returns `null` above
              otherwise — but passed through explicitly anyway, since
              `Filmstrip`'s own scroll-into-view effect is keyed on it; see
              `Filmstrip.tsx`'s doc for why. */}
          <Filmstrip {...filmstrip} visible={visible} onPreview={setPreview} />
        </div>
        <div className="reader-progress" style={progressStyle} aria-hidden="true">
          <div
            className="reader-progress-fill"
            style={{ height: '100%', width: `${progress * 100}%` }}
          />
        </div>
        <div className="reader-readout" style={readoutStyle} role="status">
          {row}
        </div>
      </div>
    </>
  )
}
```

- [ ] **Step 6: The strip's rings and badge take tokens**

In `web/src/Filmstrip.tsx`:

1. In `currentTileStyle`, replace `  borderColor: '#fff',` with `  borderColor: 'var(--accent)',`.
2. In `resolvedTileStyle`, replace `  borderColor: '#f2b134',` with `  borderColor: 'var(--warning)',`.
3. In `halfBadgeStyle`, delete the two lines `  background: 'rgba(0, 0, 0, 0.72)',` and `  color: '#fff',`, and in the comment above it replace the line

```ts
// high-contrast because it sits on artwork of unknown brightness.
```

with

```ts
// high-contrast because it sits on artwork of unknown brightness: the
// reader's panel and text, from `.reader-half-badge` in `styles/reader.css`.
```
4. On the badge's `<span style={halfBadgeStyle}>`, add `className="reader-half-badge"` before `style`.

- [ ] **Step 7: Style the bottom bar**

Append to `web/src/styles/reader.css`:

```css

/* The bottom bar (spec 2026-09-17 §8.4). Its geometry is `Chrome.tsx`'s. */
.reader-bottom-bar {
  border-top: 1px solid var(--reader-hair);
  background: var(--reader-panel);
  color: var(--text);
}

.reader-progress {
  background: var(--line);
}

.reader-progress-fill {
  background: var(--accent);
}

.reader-readout {
  color: var(--muted);
  font-size: var(--text-meta);
}

/* The strip's R/L mark on half a spread, drawn over the thumbnail. */
.reader-half-badge {
  background: var(--reader-panel);
  color: var(--text);
}

.reader-preview-img {
  background: var(--reader-panel);
}

.reader-preview-label {
  padding: 2px var(--space-2);
  background: var(--reader-panel);
  color: var(--text);
  font-size: var(--text-meta);
}
```

- [ ] **Step 8: Wire the bars into `App.tsx`**

In `web/src/App.tsx`:

1. Beside the other imports, add:

```ts
import { work as workDetail, type WorkDetail } from './library'
import { titleLines } from './readerTitle'
```

2. Directly after `  const [helpVisible, setHelpVisible] = useState(false)`, add:

```ts
  // The top bar's title and edition labels (spec 2026-09-17 §8.3): the
  // library's, with metadata edits applied, which the sidecar's `work.title`
  // and `editions.*.label` are not. Asked once beside the load and never
  // waited on: until it answers, or if it fails, the bar names the edition by
  // its key (`readerTitle.ts`). No stale-answer guard: `Routes.tsx` keys this
  // component by `work`, so one instance only ever asks about one work.
  const [detail, setDetail] = useState<WorkDetail>()
  useEffect(() => {
    workDetail(work).then(setDetail, (err: unknown) => {
      if (!(err instanceof Unauthenticated)) console.error(err)
    })
  }, [work])
```

3. Delete the layout row — the whole element below, which sits between the root's opening tag and the page area:

```tsx
      <div
        style={{
          flex: '0 0 auto',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: 8,
        }}
      >
        {(['single', 'two', 'scroll'] as const).map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => setMode(m)}
            aria-pressed={m === mode}
          >
            {m}
          </button>
        ))}
      </div>
```

and in the comment directly above the root `<div`, replace `so it can overlay the toolbar and the page area below` with `so it can overlay the page area`.

4. Directly after the line `  const stripAnchor = gapMoves && (gapMoves.before?.at ?? gapMoves.after?.at)`, just above the final `return`, add:

```ts
  // What the top bar says — `readerTitle.ts`.
  const heading = titleLines(detail, shown.edition)
```

5. In the `<Chrome` element, after `        notes={readouts.notes}`, add:

```tsx
        progress={readouts.progress}
        title={heading.title}
        editionLine={heading.editionLine}
        mode={mode}
        onMode={setMode}
        settingsOpen={settingsVisible}
        onSettings={() => setSettingsVisible((open) => !open)}
```

6. Anywhere else in `App.tsx`, a comment that says the layout buttons sit "above the page" or "in the toolbar": `command grep -n 'toolbar\|above the page' web/src/App.tsx` and correct each to say they are in the top bar, which `Tab` shows. Do the same in `web/src/modules/layout.ts`, whose doc says "The mode also has buttons of its own above the page".

- [ ] **Step 9: `App.browser.test.tsx` mocks the title too**

In `web/src/App.browser.test.tsx`, in the `ports` object add `  workDetail: vi.fn(),`; directly after its `vi.mock('./api', …)` call add

```ts
vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  work: ports.workDetail,
}))
```

and in its `beforeEach`, after the `ports.putProgress.mockResolvedValue(undefined)` line, add

```ts
  ports.workDetail.mockResolvedValue({
    id: 1,
    slug: WORK,
    title: 'Volume One',
    volume: 1,
    aligned: true,
    pages: PAGES,
    editions: [
      { key: 'raw', label: 'Raw', members: 1, pages: PAGES, missing: 0 },
      { key: 'en', label: 'English', members: 1, pages: PAGES, missing: 0 },
    ],
    opens: 'raw',
    alignments: [],
  })
```

- [ ] **Step 10: The allowlist loses `Chrome.tsx` and `Filmstrip.tsx`**

In `web/src/colourLiterals.test.ts`, delete the lines `  './Chrome.tsx': 11,` and `  './Filmstrip.tsx': 4,` from `ALLOWED`.

- [ ] **Step 11: Run the tests to verify they pass**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/Chrome.test.tsx src/Filmstrip.test.tsx src/TopBar.test.tsx src/App.test.tsx
pnpm -C web exec vitest run --project node --reporter=default src/colourLiterals.test.ts
pnpm -C web exec vitest run --project browser --reporter=default src/App.browser.test.tsx
```

Expected: PASS, all. The two O65 browser tests still pass unchanged: after `Tab`, `single`, `two`, `scroll` and `Library` are the top bar's, each reachable at its centre and overlapping nothing.

- [ ] **Step 12: Watch them fail with their defects put back**

1. In `Chrome.tsx`, render `<TopBar …/>` without the `!filmstrip.correcting &&` guard → "draws no top bar while correcting" fails.
2. `stripStyle` as `{ height: 176 }` → "stacks the strip, the progress line and the readout row" fails.
3. The fill's width as `` `${progress}%` `` → "fills the progress line" fails in `Chrome.test.tsx` and in `App.test.tsx`.
4. `style={{ ...bottomBarStyle }}` without `...fade` → the fade tests fail on the bottom bar.
5. `previewStyle.bottom` back to a number (`248`) → "stands the hover preview just above the bottom bar" fails.
6. In `Filmstrip.tsx`, `borderColor: '#fff'` back in `currentTileStyle` → `Filmstrip.test.tsx`'s current-tile test fails, and `colourLiterals.test.ts` fails on `./Filmstrip.tsx: 1`.
7. In `App.tsx`, `titleLines(undefined, shown.edition)` → "names the work and the edition on screen" fails.
8. In `App.tsx`'s effect, `console.error(err)` without the `instanceof` guard → "says nothing in the console when the title fetch finds the session gone" fails.

- [ ] **Step 13: Typecheck, lint, commit**

```bash
pnpm -C web run typecheck
pnpm -C web run lint
git add web/src/Chrome.tsx web/src/Chrome.test.tsx web/src/Filmstrip.tsx web/src/Filmstrip.test.tsx web/src/App.tsx web/src/App.test.tsx web/src/App.browser.test.tsx web/src/styles/reader.css web/src/colourLiterals.test.ts web/src/modules/layout.ts
git commit -m "Style pass 3: the top bar and one bottom bar in the reader; the layout row leaves the page"
```

---

### Task 5: The panels — Settings, Help, Credits and the notes panel

Four copies of one inline style object become one class, docked between the bars; Settings becomes a dialog in the same look, with a Close button.

**Files:**
- Modify: `web/src/Settings.tsx`, `web/src/Settings.test.tsx`, `web/src/Help.tsx`, `web/src/Help.test.tsx`, `web/src/CreditsPanel.tsx`, `web/src/CreditsPanel.test.tsx`, `web/src/NotePanel.tsx`, `web/src/NotePanel.test.tsx`, `web/src/App.tsx`, `web/src/App.test.tsx`, `web/src/styles/reader.css`, `web/src/styles/reader.css.test.ts`, `web/src/colourLiterals.test.ts`

**Interfaces:**
- Consumes: `--panel-top`, `--panel-bottom`, `--safe-*` (Task 1).
- Produces: `SettingsProps.onClose: () => void`. Classes in `reader.css`: `docked-panel` (Help, Credits, the notes panel), `reader-dialog` and `reader-dialog-panel` (Settings), `reader-panel-head`, `reader-panel-title`, `reader-panel-heading`, `reader-section`, `reader-section-head`, `reader-section-title`, `reader-muted`, `reader-meta`, `reader-note`, `reader-button`, `reader-field`, `reader-swatch`, `reader-thumb`. Task 6 uses `reader-muted` and `reader-button`.

- [ ] **Step 1: Write the failing tests**

1. Give every `Settings` mount in `web/src/Settings.test.tsx` an `onClose`:

```bash
perl -pi -e 's/^(\s*)onReset=(.*)$/$1onReset=$2\n$1onClose={vi.fn()}/' web/src/Settings.test.tsx
command grep -c 'onClose={vi.fn()}' web/src/Settings.test.tsx
```

Expected: `12`. Then, inside `describe('Settings', () => {`, after the `it('renders nothing while not visible', …)` test, add:

```tsx
  it('closes on its own Close button, since ⚙ opens it by pointer', () => {
    const onClose = vi.fn()
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={onClose}
      />,
    )
    const close = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    )
    close?.click()
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('draws as the reader’s dialog, its look in the stylesheet', () => {
    const host = mount(
      <Settings
        visible
        modules={modules}
        prefs={{}}
        onChange={vi.fn()}
        onReset={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    const dialog = host.querySelector('[role="dialog"]') as HTMLElement
    expect(dialog.className).toBe('reader-dialog')
    expect(dialog.getAttribute('style')).toBeNull()
    expect(dialog.firstElementChild?.className).toBe('reader-dialog-panel')
  })
```

2. In `web/src/Help.test.tsx`, inside `describe('Help', () => {`, add:

```tsx
  it('docks as a panel between the bars, its look in the stylesheet', () => {
    const host = mount(
      <Help visible={true} modules={[]} reading="ltr" onClose={noop} />,
    )
    const panel = host.querySelector('[role="dialog"]') as HTMLElement
    expect(panel.className).toBe('docked-panel')
    expect(panel.getAttribute('style')).toBeNull()
  })
```

3. In `web/src/CreditsPanel.test.tsx`, inside `describe('CreditsPanel', () => {`, add:

```tsx
  it('docks as a panel between the bars, its look in the stylesheet', () => {
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="raw"
        pages={[3]}
        skipping={false}
        onGo={noop}
        onClose={noop}
      />,
    )
    const panel = host.querySelector('[role="dialog"]') as HTMLElement
    expect(panel.className).toBe('docked-panel')
    expect(panel.getAttribute('style')).toBeNull()
    expect(host.querySelector('img')?.className).toBe('reader-thumb')
  })
```

4. In `web/src/NotePanel.test.tsx`, inside `describe('NotePanel, editing', () => {`, add:

```tsx
  it('docks as a panel between the bars, its look in the stylesheet', () => {
    const host = mount(<NotePanel {...baseProps()} />)
    const panel = host.querySelector('[role="dialog"]') as HTMLElement
    expect(panel.className).toBe('docked-panel')
    expect(panel.getAttribute('style')).toBeNull()
  })

  it('rings the chosen swatch from the stylesheet, by aria-pressed', () => {
    const host = mount(
      <NotePanel {...baseProps({ draft: note({ color: 'rose' }) })} />,
    )
    for (const swatch of host.querySelectorAll<HTMLButtonElement>(
      '[role="group"] button',
    )) {
      expect(swatch.classList.contains('reader-swatch')).toBe(true)
      expect(swatch.style.boxShadow).toBe('')
    }
  })
```

5. Append to `web/src/styles/reader.css.test.ts`:

```ts
describe('the panels (spec §8.5)', () => {
  it('dock between the bars, whether or not the bars are shown', () => {
    expect(declared(readerSource, '.docked-panel', 'top')).toEqual([
      'var(--panel-top)',
    ])
    expect(declared(readerSource, '.docked-panel', 'max-height')).toEqual([
      'max(8rem,calc(100%-var(--panel-top)-var(--panel-bottom)))',
    ])
  })

  for (const panel of ['.docked-panel', '.reader-dialog-panel']) {
    it(`draw ${panel} on the reader's panel, with a hairline, square and unshadowed`, () => {
      expect(declared(readerSource, panel, 'background')).toEqual([
        'var(--reader-panel)',
      ])
      expect(declared(readerSource, panel, 'border')).toEqual([
        '1pxsolidvar(--reader-hair)',
      ])
      expect(declared(readerSource, panel, 'border-radius')).toEqual([])
      expect(declared(readerSource, panel, 'box-shadow')).toEqual([])
    })
  }

  it('ring the chosen note colour in the panel, then the text', () => {
    expect(
      declared(readerSource, '.reader-swatch[aria-pressed="true"]', 'box-shadow'),
    ).toEqual(['0002pxvar(--bg),0004pxvar(--text)'])
  })
})
```

6. In `web/src/App.test.tsx`, directly after the `it('⚙ opens and closes the settings panel', …)` test from Task 4, add:

```tsx
  it("Settings' Close button closes the panel", async () => {
    const host = await open()
    press('s')
    expect(dialog(host, 'Settings')).not.toBeNull()
    click(button(host, 'Close'))
    expect(dialog(host, 'Settings')).toBeNull()
  })
```

- [ ] **Step 2: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/Settings.test.tsx src/Help.test.tsx src/CreditsPanel.test.tsx src/NotePanel.test.tsx src/App.test.tsx
pnpm -C web exec vitest run --project node --reporter=default src/styles/reader.css.test.ts
```

Expected: FAIL — no Close button, inline `style` on each panel, no `reader-thumb`, swatches carrying an inline `boxShadow`, and no `.docked-panel` rules. `pnpm -C web run typecheck` would also refuse `onClose` on `Settings`.

- [ ] **Step 3: Settings — a dialog in the panels' look, with Close**

In `web/src/Settings.tsx`:

1. Replace everything from `const overlayStyle: CSSProperties = {` through the end of the `noteStyle` object (the six style objects `overlayStyle`, `panelStyle`, `sectionStyle`, `sectionHeaderStyle`, `rowStyle`, `noteStyle`) with:

```tsx
// A field's row, handed to `FieldControl`, which takes its layout as an
// object because the library's own settings pass theirs (`settingsFields.tsx`).
// Colour and type are `.reader-dialog-panel`'s, in `styles/reader.css`, like
// the rest of this panel's look (spec 2026-09-17 §8.5).
const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '6px 0',
}
```

2. In `SettingsProps`, after `  onReset: (id: string) => void`, add:

```ts
  /**
   * Close the panel. Its own button because ⚙ opens it by pointer (spec
   * 2026-09-17 §8.3), and `s` — the only other way out — is a key.
   */
  onClose: () => void
```

3. Replace the `Settings` function with:

```tsx
export function Settings({
  visible,
  modules,
  prefs,
  notes,
  onChange,
  onReset,
  onClose,
}: SettingsProps) {
  if (!visible) return null
  return (
    <div className="reader-dialog" role="dialog" aria-label="Settings">
      <div className="reader-dialog-panel">
        <div className="reader-panel-head">
          <h2 className="reader-panel-title">Settings</h2>
          <button type="button" className="reader-button" onClick={onClose}>
            Close
          </button>
        </div>
        {modules.map((mod) => {
          const fields = fieldsOf(mod.defaults as object, mod.options)
          if (fields.length === 0) return null
          const modulePrefs = (prefs[mod.id] ?? mod.defaults) as Record<
            string,
            unknown
          >
          const note = notes?.[mod.id]
          return (
            <section key={mod.id} className="reader-section">
              <div className="reader-section-head">
                <h3 className="reader-section-title">{mod.label}</h3>
                <button
                  type="button"
                  className="reader-button"
                  onClick={() => onReset(mod.id)}
                >
                  Reset
                </button>
              </div>
              {fields.map((field) => (
                <FieldControl
                  key={field.key}
                  field={field}
                  value={modulePrefs[field.key]}
                  onChange={(raw) => onChange(mod.id, field.key, raw)}
                  rowStyle={rowStyle}
                />
              ))}
              {note && <p className="reader-meta reader-note">{note}</p>}
            </section>
          )
        })}
      </div>
    </div>
  )
}
```

In `web/src/App.tsx`, in the `<Settings` element, after `        onReset={handleSettingReset}`, add:

```tsx
        onClose={() => setSettingsVisible(false)}
```

- [ ] **Step 4: Help**

In `web/src/Help.tsx`:

1. Replace everything from the comment `` // The shell `CreditsPanel` uses, which is the shell `NotePanel` uses: three `` through the end of the `rowStyle` object (the comment, `panelStyle`, `groupHeadingStyle` and `rowStyle`) with:

```tsx
// The panel is `.docked-panel` in `styles/reader.css`, where `CreditsPanel`
// and the notes panel dock too: one class for what was three copies of one
// inline object, which this comment used to count (spec 2026-09-17 §8.5).

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  gap: 10,
  padding: '4px 0',
  borderTop: '1px solid var(--reader-hair)',
}
```

2. In `Help`'s JSX, replace

```tsx
    <div style={panelStyle} role="dialog" aria-label="Keyboard help">
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
        <strong style={{ flex: 1 }}>Keys</strong>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
      {helpGroups(modules, reading).map((group) => (
        <section key={group.heading}>
          <h3 style={groupHeadingStyle}>{group.heading}</h3>
```

with

```tsx
    <div className="docked-panel" role="dialog" aria-label="Keyboard help">
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Keys</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {helpGroups(modules, reading).map((group) => (
        <section key={group.heading}>
          <h3 className="reader-panel-heading">{group.heading}</h3>
```

- [ ] **Step 5: Credits**

In `web/src/CreditsPanel.tsx`:

1. Replace everything from the comment `` // Same shell as `NotePanel`'s, deliberately: panels that open over the page `` through the end of the `panelStyle` object with:

```tsx
// The panel is `.docked-panel` in `styles/reader.css`, shared with `Help` and
// the notes panel (spec 2026-09-17 §8.5). This comment used to record that
// the reason for copying it had expired at four copies; it is one class now.
```

2. In `rowStyle`, replace `  borderTop: '1px solid rgba(255, 255, 255, 0.12)',` with `  borderTop: '1px solid var(--reader-hair)',`.
3. In `thumbStyle`, delete `  background: 'rgba(0, 0, 0, 0.35)',`.
4. In `CreditsPanel`'s JSX: `style={panelStyle}` on the dialog becomes `className="docked-panel"`; the header row `<div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>` becomes `<div className="reader-panel-head">`; `<strong style={{ flex: 1 }}>` becomes `<strong className="reader-panel-title">`; the Close `<button type="button" onClick={onClose}>` gains `className="reader-button"`; both `<p style={{ margin: 0, opacity: 0.8 }}>` become `<p className="reader-muted" style={{ margin: 0 }}>`; and the `<img` gains `className="reader-thumb"` before `style={thumbStyle}`.

- [ ] **Step 6: The notes panel**

In `web/src/NotePanel.tsx`:

1. Replace the `panelStyle` object (from `const panelStyle: CSSProperties = {` to its closing `}`) with:

```tsx
// The panel is `.docked-panel` in `styles/reader.css`, shared with `Help` and
// `CreditsPanel` (spec 2026-09-17 §8.5).
```

2. In `rowStyle`, replace `  borderTop: '1px solid rgba(255, 255, 255, 0.12)',` with `  borderTop: '1px solid var(--reader-hair)',`.
3. In the JSX:
   - `style={panelStyle}` on the dialog becomes `className="docked-panel"`.
   - `<p style={{ margin: 0, opacity: 0.8, fontSize: '0.85em' }}>` (the anchor line) becomes `<p className="reader-meta" style={{ margin: 0 }}>`.
   - The `<textarea` gains `className="reader-field"` before `style=`.
   - Each swatch `<button` gains `className="reader-swatch"`, and its style object loses the whole `boxShadow:` entry — the three lines from `boxShadow: chosen` to `: 'none',`. `chosen` stays: `aria-pressed` still reads it.
   - Both `{notice && <p style={{ margin: 0, opacity: 0.85 }}>{notice}</p>}` become `{notice && <p style={{ margin: 0 }}>{notice}</p>}`.
   - The Delete, ↶, ↷, Export and Close `<button type="button"` each gain `className="reader-button"`.
   - `<span style={{ marginLeft: 'auto', opacity: 0.65, fontSize: '0.8em' }}>` becomes `<span className="reader-meta" style={{ marginLeft: 'auto' }}>`.
   - The review header's `<strong>` becomes `<strong className="reader-panel-title">`.
   - `<p style={{ margin: 0, opacity: 0.8 }}>` (Nothing written yet) becomes `<p className="reader-muted" style={{ margin: 0 }}>`.
   - `<span style={{ opacity: 0.75, fontSize: '0.85em' }}>` (an entry's edition and slot) becomes `<span className="reader-meta">`.

`import { useEffect, useRef, type CSSProperties } from 'react'` stays: `rowStyle` still uses the type.

- [ ] **Step 7: Style the panels**

Append to `web/src/styles/reader.css`:

```css

/* The docked panels — Help, Credits and the notes panel (spec 2026-09-17
 * §8.5): one class for what was four copies of one inline object. 16 px below
 * the top bar's line and above the bottom bar's, whether or not the bars are
 * shown, so a panel never moves; a long one scrolls inside itself, and a
 * window too short for the bars still gets 8rem. */
.docked-panel {
  position: absolute;
  top: var(--panel-top);
  right: calc(var(--space-4) + var(--safe-right));
  z-index: 3;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  width: 384px;
  max-width: calc(
    100% - 2 * var(--space-4) - var(--safe-left) - var(--safe-right)
  );
  max-height: max(8rem, calc(100% - var(--panel-top) - var(--panel-bottom)));
  overflow-y: auto;
  padding: var(--space-3);
  border: 1px solid var(--reader-hair);
  background: var(--reader-panel);
  color: var(--text);
  font-size: var(--text-ui);
}

/* Settings: a dialog over a scrim, in the panels' look. */
.reader-dialog {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--scrim);
}

.reader-dialog-panel {
  box-sizing: border-box;
  width: min(480px, 90vw);
  max-height: 80vh;
  overflow-y: auto;
  padding: var(--space-5);
  border: 1px solid var(--reader-hair);
  background: var(--reader-panel);
  color: var(--text);
  font-size: var(--text-ui);
}

.reader-dialog-panel :where(input, select) {
  font: inherit;
  accent-color: var(--accent);
}

.reader-panel-head {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.reader-panel-title {
  flex: 1;
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--text-h3);
  font-weight: 600;
}

.reader-panel-heading {
  margin: var(--space-2) 0 0;
  color: var(--muted);
  font-family: var(--font-title);
  font-size: var(--text-ui);
  font-weight: 600;
}

.reader-section {
  margin-top: var(--space-4);
  padding-top: var(--space-4);
  border-top: 1px solid var(--reader-hair);
}

.reader-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.reader-section-title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--text-body);
  font-weight: 600;
}

.reader-muted {
  color: var(--muted);
}

.reader-meta {
  color: var(--muted);
  font-size: var(--text-meta);
}

.reader-note {
  margin: var(--space-1) 0 0;
}

/* A panel's own button — Close, Reset, Export, Delete, a gap's offers — which
 * until style pass ③ was the browser's own: `base.css`'s element rules reach
 * only `.library` and `.login`. */
.reader-button {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.reader-button:disabled {
  color: var(--muted);
  cursor: default;
}

.reader-field {
  box-sizing: border-box;
  padding: var(--space-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
  background: var(--field);
  color: var(--text);
  font: inherit;
}

/* A note colour's swatch, the chosen one ringed: the panel, then the text. */
.reader-swatch[aria-pressed="true"] {
  box-shadow:
    0 0 0 2px var(--bg),
    0 0 0 4px var(--text);
}

/* Behind a credit page's thumbnail, the letterbox it is drawn into. */
.reader-thumb {
  background: var(--reader-ground);
}

@media (pointer: coarse) {
  .reader-button {
    min-height: 44px;
  }
}
```

- [ ] **Step 8: The allowlist loses the four panels**

In `web/src/colourLiterals.test.ts`, delete from `ALLOWED` the lines `  './CreditsPanel.tsx': 5,`, `  './Help.tsx': 4,`, `  './NotePanel.tsx': 6,` and `  './Settings.tsx': 5,`.

- [ ] **Step 9: Run the tests to verify they pass**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/Settings.test.tsx src/Help.test.tsx src/CreditsPanel.test.tsx src/NotePanel.test.tsx src/App.test.tsx
pnpm -C web exec vitest run --project node --reporter=default src/styles/reader.css.test.ts src/colourLiterals.test.ts
```

Expected: PASS. If `colourLiterals.test.ts` reports a count above zero for one of the four files, a literal was missed: `command grep -n "rgba\?(\|#[0-9a-fA-F]\{3,8\}\b" web/src/<file>` finds it, and it moves to a class or a token rather than back onto the allowlist.

- [ ] **Step 10: Watch them fail with their defects put back**

1. In `Settings.tsx`, the Close button's `onClick={undefined}` → `Settings.test.tsx`'s "closes on its own Close button" and `App.test.tsx`'s "Settings' Close button closes the panel" fail.
2. In `Help.tsx`, `style={{ top: 16 }}` back on the dialog beside its class → Help's "docks as a panel" fails.
3. In `NotePanel.tsx`, the swatch's `boxShadow: chosen ? '0 0 0 2px var(--bg)' : 'none'` put back → "rings the chosen swatch from the stylesheet" fails.
4. In `reader.css`, `.docked-panel`'s `top: 16px` → reader.css.test's "dock between the bars" fails.
5. In `reader.css`, `border-radius: 8px;` added to `.reader-dialog-panel` → its "square and unshadowed" case fails.

- [ ] **Step 11: Typecheck, lint, commit**

```bash
pnpm -C web run typecheck
pnpm -C web run lint
git add web/src/Settings.tsx web/src/Settings.test.tsx web/src/Help.tsx web/src/Help.test.tsx web/src/CreditsPanel.tsx web/src/CreditsPanel.test.tsx web/src/NotePanel.tsx web/src/NotePanel.test.tsx web/src/App.tsx web/src/App.test.tsx web/src/styles/reader.css web/src/styles/reader.css.test.ts web/src/colourLiterals.test.ts
git commit -m "Style pass 3: the reader's panels dock between the bars in ink; Settings gains Close"
```

---

### Task 6: The note list (O112), the banner, gaps, borrowed pages and the notes on the page

**Files:**
- Modify: `web/src/NoteList.tsx`, `web/src/NoteList.test.tsx`, `web/src/CorrectionBanner.tsx`, `web/src/CorrectionBanner.test.tsx`, `web/src/Gap.tsx`, `web/src/GapOffers.tsx`, `web/src/Borrowed.tsx`, `web/src/Borrowed.test.tsx`, `web/src/Annotations.tsx`, `web/src/Annotations.test.tsx`, `web/src/App.tsx`, `web/src/App.test.tsx`, `web/src/styles/tokens.css`, `web/src/styles/reader.css`, `web/src/colourLiterals.test.ts`

**Interfaces:**
- Consumes: `reader-muted`, `reader-button` (Task 5); `--panel-top`, `--panel-bottom`, `--safe-*` (Task 1).
- Produces: `NoteListProps.covered?: boolean`. Tokens `--art-ink`, `--art-caption`, `--art-caption-text`, `--art-edge`, `--art-edge-soft`, `--art-edge-faint`, `--art-drawing` in `tokens.css`.

- [ ] **Step 1: Write the failing tests**

1. In `web/src/NoteList.test.tsx`, inside `describe('NoteList', () => {`, add:

```tsx
  it('docks beside the page where a panel docks, between the bars', () => {
    const host = mount(
      <NoteList here={[here()]} placement="side" onOpen={() => {}} />,
    )
    const list = host.firstElementChild as HTMLElement
    expect(list.style.top).toBe('var(--panel-top)')
    expect(list.style.maxHeight).toBe(
      'calc(100% - var(--panel-top) - var(--panel-bottom))',
    )
    expect(list.className).toBe('note-list note-list-side')
  })

  /**
   * O112, answered in spec 2026-09-17 §8.1: under the bottom bar the list
   * steps aside — hidden, but mounted and still taking its space, so the page
   * area does not change size on `Tab`.
   */
  it('steps aside under the bars while they are shown, keeping its space', () => {
    const host = mount(
      <NoteList here={[here()]} placement="below" onOpen={() => {}} covered />,
    )
    const list = host.firstElementChild as HTMLElement
    expect(list.style.visibility).toBe('hidden')
    expect(list.style.flex).toBe('0 0 auto')
    expect(host.querySelector('[role="listitem"]')).not.toBeNull()
  })

  it('shows under the page while the bars are away', () => {
    const host = mount(
      <NoteList here={[here()]} placement="below" onOpen={() => {}} />,
    )
    expect((host.firstElementChild as HTMLElement).style.visibility).toBe('')
  })

  it('never steps aside beside the page, which docks clear of the bars', () => {
    const host = mount(
      <NoteList here={[here()]} placement="side" onOpen={() => {}} covered />,
    )
    expect((host.firstElementChild as HTMLElement).style.visibility).toBe('')
  })

  it('pads the list under the page by the safe areas', () => {
    const host = mount(
      <NoteList here={[here()]} placement="below" onOpen={() => {}} />,
    )
    const list = host.firstElementChild as HTMLElement
    expect(list.style.padding).toBe(
      '8px calc(12px + var(--safe-right)) calc(8px + var(--safe-bottom)) calc(12px + var(--safe-left))',
    )
    expect(list.className).toBe('note-list note-list-below')
  })

  it('draws a number chip’s digit in the artwork’s ink', () => {
    const host = mount(
      <NoteList here={[here()]} placement="side" onOpen={() => {}} />,
    )
    const chip = host.querySelector('button > span') as HTMLElement
    expect(chip.style.color).toBe('var(--art-ink)')
  })
```

2. In `web/src/CorrectionBanner.test.tsx`, append:

```tsx
describe('CorrectionBanner, where it stands', () => {
  /** Spec 2026-09-17 §8.3: the top bar is not drawn while correcting, and this takes its place. */
  it('takes the top bar’s place: the reader’s top edge, full width, over the page', () => {
    const host = mount(<CorrectionBanner {...props()} />)
    const banner = host.firstElementChild as HTMLElement
    expect(banner.className).toBe('reader-banner')
    expect([banner.style.top, banner.style.left, banner.style.right]).toEqual([
      '0px',
      '0px',
      '0px',
    ])
    expect(Number(banner.style.zIndex)).toBe(3)
    expect(banner.style.padding).toContain('var(--safe-top)')
  })

  it('gives its buttons the reader’s button look', () => {
    const host = mount(<CorrectionBanner {...props({ pending: picked })} />)
    for (const b of buttons(host)) expect(b.className).toBe('reader-button')
  })
})
```

3. In `web/src/Borrowed.test.tsx`, inside `describe('Borrowed', () => {`, add:

```tsx
  it('marks the borrowed page in the warning colour, its look in the stylesheet', () => {
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button') as HTMLButtonElement
    expect(mark.className).toBe('borrowed-mark')
    expect(mark.style.background).toBe('')
    expect((host.firstElementChild as HTMLElement).style.boxShadow).toBe(
      'inset 4px 0 0 0 var(--warning)',
    )
  })
```

4. In `web/src/Annotations.test.tsx`, replace both occurrences of

```ts
      expect(pending.style.background).toBe('rgba(0, 0, 0, 0.42)')
```

with

```ts
      expect(pending.style.background).toBe('var(--art-drawing)')
```

5. In `web/src/App.test.tsx`, in the `describe` that holds `it('r reveals the regions, and the list of notes on the page with them', …)`, add after that test:

```tsx
  it('O112: the note list under the page steps aside while the chrome is up, and comes back', async () => {
    localStorage.setItem(
      'taiyaku.prefs.annotations',
      JSON.stringify({
        markers: true,
        travel: true,
        reveal: true,
        notesList: 'below',
      }),
    )
    ports.getNotes.mockResolvedValue({
      doc: {
        work: WORK,
        notes: [
          {
            id: 'note-1',
            edition: 'raw',
            slot: 2,
            body: 'a note already here',
            created: '2026-01-01T00:00:00.000Z',
            updated: '2026-01-01T00:00:00.000Z',
          },
        ],
      },
      etag: 'n1',
    })
    const host = await open()
    const list = () =>
      host.querySelector('[aria-label="Notes on this page"]') as HTMLElement
    expect(list().style.visibility).toBe('')
    press('Tab')
    expect(list().style.visibility).toBe('hidden')
    press('Tab')
    expect(list().style.visibility).toBe('')
  })
```

- [ ] **Step 2: Run them to verify they fail**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/NoteList.test.tsx src/CorrectionBanner.test.tsx src/Borrowed.test.tsx src/Annotations.test.tsx src/App.test.tsx
```

Expected: FAIL on every new assertion and on the two `var(--art-drawing)` lines; `tsc` would also refuse `covered` on `NoteList`.

- [ ] **Step 3: The artwork's tokens**

Append to `web/src/styles/tokens.css`:

```css

/* What the notes overlay draws on artwork (spec 2026-09-17 §8.5): fixed, for
 * the box palette's reason — the artwork does not follow the theme — and at
 * exactly the values `Annotations.tsx` and `NoteList.tsx` drew before they
 * became tokens. */
:root {
  /* A note number's digit on its coloured tab or chip. */
  --art-ink: rgba(20, 20, 20, 0.9);
  /* Behind a note's words inside its region, and the words. */
  --art-caption: rgba(16, 16, 16, 0.82);
  --art-caption-text: rgba(255, 255, 255, 0.95);
  /* The dark lines under a region's outline, so it holds on white paper. */
  --art-edge: rgba(0, 0, 0, 0.85);
  --art-edge-soft: rgba(0, 0, 0, 0.7);
  --art-edge-faint: rgba(0, 0, 0, 0.5);
  /* A region's fill while the pointer is still drawing it. */
  --art-drawing: rgba(0, 0, 0, 0.42);
}
```

- [ ] **Step 4: The note list**

In `web/src/NoteList.tsx`:

1. Replace the `sideStyle` object with:

```tsx
const sideStyle: CSSProperties = {
  position: 'absolute',
  // Where a docked panel stands (spec 2026-09-17 §8.5): below the top bar's
  // line and above the bottom bar's, whether or not the bars are shown —
  // `--panel-top` and `--panel-bottom` in `styles/reader.css`.
  top: 'var(--panel-top)',
  right: 'calc(var(--space-4) + var(--safe-right))',
  // Height from what it holds, not from the page. Pinning `bottom` as well as
  // `top` stretched the panel down the whole margin however few notes were in
  // it, so two rows sat at the top of a column of empty background — a box
  // that looked like it had lost its contents. `maxHeight` keeps a long list
  // scrolling between the bars rather than under them.
  maxHeight: 'calc(100% - var(--panel-top) - var(--panel-bottom))',
  width: 240,
  overflowY: 'auto',
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding: 10,
  zIndex: 3,
}
```

2. Replace the `belowStyle` object with:

```tsx
// Padded by the safe areas, since it runs to the reader's bottom edge.
const belowStyle: CSSProperties = {
  flex: '0 0 auto',
  maxHeight: '28%',
  overflowY: 'auto',
  display: 'flex',
  flexDirection: 'column',
  gap: 6,
  padding:
    '8px calc(12px + var(--safe-right)) calc(8px + var(--safe-bottom)) calc(12px + var(--safe-left))',
}
```

3. In `numberStyle`, replace `  color: 'rgba(20, 20, 20, 0.9)',` with `  color: 'var(--art-ink)',`.
4. In `NoteListProps`, after `  placement: 'side' | 'below'`, add:

```ts
  /**
   * Whether `Chrome`'s bars are shown — O112, answered in spec 2026-09-17
   * §8.1. Under the bottom bar the list below the page is `visibility:
   * hidden`: still mounted and still taking its space, so the page area keeps
   * its size on `Tab`, which `Chrome` exists to guarantee. The list beside the
   * page docks clear of both bars and ignores this.
   */
  covered?: boolean
```

5. Replace the function's signature and root element

```tsx
export function NoteList({ here, placement, onOpen }: NoteListProps) {
  if (here.length === 0) return null
  return (
    <div
      style={placement === 'side' ? sideStyle : belowStyle}
```

with

```tsx
export function NoteList({ here, placement, onOpen, covered }: NoteListProps) {
  if (here.length === 0) return null
  return (
    <div
      className={`note-list note-list-${placement}`}
      style={
        placement === 'side'
          ? sideStyle
          : { ...belowStyle, visibility: covered ? 'hidden' : undefined }
      }
```

6. Each of the three `<em style={{ opacity: 0.7 }}>` becomes `<em className="reader-muted">`.

In `web/src/App.tsx`, replace

```tsx
        <NoteList here={here} placement="below" onOpen={openNote} />
```

with

```tsx
        <NoteList
          here={here}
          placement="below"
          onOpen={openNote}
          covered={chromeVisible}
        />
```

- [ ] **Step 5: The correction banner, in the top bar's place**

In `web/src/CorrectionBanner.tsx`, replace the root element's opening

```tsx
    <div
      role="status"
      style={{
        position: 'absolute',
        top: 16,
        left: 16,
        right: 16,
        zIndex: 1,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: 8,
        padding: '10px 16px',
        borderRadius: 8,
        background: 'rgba(24, 24, 24, 0.92)',
        color: 'rgba(255, 255, 255, 0.92)',
        textAlign: 'center',
      }}
    >
```

with

```tsx
    // In the top bar's place (spec 2026-09-17 §8.3): `Chrome` draws no top
    // bar while correcting, so this runs the reader's full width from its top
    // edge, padded by the safe areas, and stands at the bars' `zIndex: 3`.
    <div
      role="status"
      className="reader-banner"
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        zIndex: 3,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: 8,
        padding:
          'calc(var(--safe-top) + 10px) calc(16px + var(--safe-right)) 10px calc(16px + var(--safe-left))',
        textAlign: 'center',
      }}
    >
```

Then `{notice && <p style={{ margin: 0, opacity: 0.85 }}>{notice}</p>}` becomes `{notice && <p style={{ margin: 0 }}>{notice}</p>}`, and the Confirm and Cancel `<button type="button"` each gain `className="reader-button"`.

- [ ] **Step 6: Gaps**

In `web/src/Gap.tsx`, `<p style={{ fontSize: '1.1em', margin: 0 }}>` becomes `<p className="reader-gap-title">`, and `<p style={{ margin: 0, opacity: 0.8 }}>{sentence[verdict]}</p>` becomes `<p className="reader-muted" style={{ margin: 0 }}>{sentence[verdict]}</p>`.

In `web/src/GapOffers.tsx`, `<p style={{ margin: 0, opacity: 0.8 }}>` becomes `<p className="reader-muted" style={{ margin: 0 }}>`, and both `<button type="button" onClick={() => onTake(…)}>` gain `className="reader-button"`.

- [ ] **Step 7: Borrowed pages**

In `web/src/Borrowed.tsx`:

1. In `MARK_STYLE`, replace the three lines

```ts
  fontSize: '0.85em',
  cursor: 'pointer',
  background: 'rgba(180, 140, 60, 0.9)',
  color: '#1a1a1a',
```

with

```ts
  fontSize: 'var(--text-meta)',
  cursor: 'pointer',
```

and in its doc comment, replace the line

```ts
 * is also what stops one of them being restyled alone.
```

with

```ts
 * is also what stops one of them being restyled alone. Its colours are
 * `.borrowed-mark`'s, in `styles/reader.css`: `--warning` with Ink's ground on
 * it (spec 2026-09-17 §8.5).
```

2. Both mark buttons — `style={MARK_STYLE}` — gain `className="borrowed-mark"` before `style`.
3. Replace `        boxShadow: 'inset 4px 0 0 0 rgba(180, 140, 60, 0.9)',` with `        boxShadow: 'inset 4px 0 0 0 var(--warning)',`.
4. In the caption block's style, delete `          background: 'rgba(0, 0, 0, 0.65)',` and `          color: '#f2f2f2',`, and give that `<div` `className="borrowed-caption"` before `style`.
5. In the comment above the caption block, the line ``text (0.85em over the 1.5 line-height it inherits from `index.css`'s`` becomes ``text (`--text-meta`, 0.75rem, over the 1.5 line-height it inherits from `index.css`'s``, keeping its indentation — the mark is shorter now, so 32 still clears it; re-wrap the comment if Biome asks.

- [ ] **Step 8: The notes on the page**

In `web/src/Annotations.tsx`:

1. In `tabStyle`, replace `  color: travelled ? colour : 'rgba(20, 20, 20, 0.9)',` with `  color: travelled ? colour : 'var(--art-ink)',`.
2. In `bodyStyle`, replace `  background: 'rgba(16, 16, 16, 0.82)',` and `  color: 'rgba(255, 255, 255, 0.95)',` with `  background: 'var(--art-caption)',` and `  color: 'var(--art-caption-text)',`.
3. In `pinStyle`, replace `  boxShadow: '0 0 0 1px rgba(0, 0, 0, 0.7), inset 0 0 0 1px rgba(0, 0, 0, 0.5)',` with `  boxShadow: '0 0 0 1px var(--art-edge-soft), inset 0 0 0 1px var(--art-edge-faint)',`.
4. In `pendingStyle`, both `background: drawing ? 'rgba(0, 0, 0, 0.42)' : colour.tint,` become `background: drawing ? 'var(--art-drawing)' : colour.tint,`; `boxShadow: '0 0 0 1px rgba(0, 0, 0, 0.85)',` becomes `boxShadow: '0 0 0 1px var(--art-edge)',`; and the two-line `boxShadow:` value `'0 0 0 1px rgba(0, 0, 0, 0.85), inset 0 0 0 1px rgba(0, 0, 0, 0.85)',` becomes `'0 0 0 1px var(--art-edge), inset 0 0 0 1px var(--art-edge)',`.
5. In `popoverStyle`, delete `  borderRadius: 6,`, `  background: 'rgba(16, 16, 16, 0.94)',`, `  color: 'rgba(255, 255, 255, 0.95)',` and `  boxShadow: '0 6px 18px rgba(0, 0, 0, 0.5)',`; add to its doc comment the sentence ``Its look is `.reader-popover`'s, the panels' (spec 2026-09-17 §8.5).``; and on the element that uses it, `<span style={popoverStyle(offset, tabSide)}>` becomes `<span className="reader-popover" style={popoverStyle(offset, tabSide)}>`, with the `<em style={{ opacity: 0.75 }}>` inside it becoming `<em className="reader-muted">`.

- [ ] **Step 9: Style them**

Append to `web/src/styles/reader.css`:

```css

/* The note list beside the page or under it (`NoteList.tsx`). */
.note-list {
  background: var(--reader-panel);
  color: var(--text);
  font-size: var(--text-ui);
}

.note-list-side {
  border: 1px solid var(--reader-hair);
}

.note-list-below {
  border-top: 1px solid var(--reader-hair);
}

/* The correction banner, in the top bar's place (`CorrectionBanner.tsx`). */
.reader-banner {
  border-bottom: 1px solid var(--reader-hair);
  background: var(--reader-panel);
  color: var(--text);
}

/* A gap's sentence (`Gap.tsx`), on the black. */
.reader-gap-title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--text-h3);
  font-weight: 600;
}

/* A page borrowed from another edition (`Borrowed.tsx`): the mark in the
 * warning colour with Ink's ground on it, 9.72:1, and its caption. */
.borrowed-mark {
  background: var(--warning);
  color: var(--bg);
}

.borrowed-caption {
  background: var(--reader-panel);
  color: var(--text);
}

/* A note's words beside its tab, on the page (`Annotations.tsx`). */
.reader-popover {
  border: 1px solid var(--reader-hair);
  background: var(--reader-panel);
  color: var(--text);
}
```

- [ ] **Step 10: The allowlist**

In `web/src/colourLiterals.test.ts`, delete from `ALLOWED` the lines for `./Annotations.tsx`, `./Borrowed.tsx`, `./CorrectionBanner.tsx` and `./NoteList.tsx`. What remains is exactly:

```ts
const ALLOWED: Record<string, number> = {
  './Account.tsx': 10,
  './styles/annotations.css': 2,
}
```

`Account.tsx` is ⑤'s screen; `styles/annotations.css`'s two are the labeller's region boxes (spec §8.5).

- [ ] **Step 11: Run the tests to verify they pass**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default src/NoteList.test.tsx src/CorrectionBanner.test.tsx src/Borrowed.test.tsx src/Annotations.test.tsx src/Gap.test.tsx src/App.test.tsx
pnpm -C web exec vitest run --project node --reporter=default src/colourLiterals.test.ts src/index.css.test.ts src/styles/reader.css.test.ts
```

Expected: PASS.

- [ ] **Step 12: Watch them fail with their defects put back**

1. In `NoteList.tsx`, `visibility: undefined` in place of `covered ? 'hidden' : undefined` → "steps aside under the bars" and `App.test.tsx`'s O112 test fail.
2. Apply `visibility` to the side list too (`style={{ ...(placement === 'side' ? sideStyle : belowStyle), visibility: covered ? 'hidden' : undefined }}`) → "never steps aside beside the page" fails.
3. Unmount instead of hiding (`if (covered && placement === 'below') return null`) → "keeping its space" fails on `null`.
4. `CorrectionBanner`'s `zIndex: 1` → "takes the top bar's place" fails.
5. `Annotations.tsx`'s drawing fill back to `'rgba(0, 0, 0, 0.42)'` → both `Annotations.test.tsx` lines fail, and `colourLiterals.test.ts` reports `./Annotations.tsx`.

- [ ] **Step 13: Typecheck, lint, commit**

```bash
pnpm -C web run typecheck
pnpm -C web run lint
git add web/src/NoteList.tsx web/src/NoteList.test.tsx web/src/CorrectionBanner.tsx web/src/CorrectionBanner.test.tsx web/src/Gap.tsx web/src/GapOffers.tsx web/src/Borrowed.tsx web/src/Borrowed.test.tsx web/src/Annotations.tsx web/src/Annotations.test.tsx web/src/App.tsx web/src/App.test.tsx web/src/styles/tokens.css web/src/styles/reader.css web/src/colourLiterals.test.ts
git commit -m "Style pass 3: O112 — the note list steps aside under the bars; the banner, gaps, borrowed pages and notes in ink"
```

---

### Task 7: The geometry, measured in the browser project

jsdom pins what each style declares; this measures what Chromium draws from it. The gate runs this project; CI does not.

**Files:**
- Modify: `web/src/App.browser.test.tsx`

**Interfaces:**
- Consumes: everything from Tasks 1–6.

- [ ] **Step 1: Replace the O65 block with the two bars' measurements**

In `web/src/App.browser.test.tsx`:

1. Replace the file's header comment (the `/** … */` above `const ports`) with:

```tsx
/**
 * The reader's two bars and what docks between them, laid out in a real
 * browser (spec 2026-09-17 §8) — the only place these can be observed.
 * `Chrome.test.tsx`, `TopBar.test.tsx` and `NoteList.test.tsx` pin what each
 * style declares, in jsdom, which CI runs; jsdom has no layout, so a box
 * cannot be told from another there, and `var()` stays a string. This asks
 * Chromium where each box actually lands, at a desktop, a portrait tablet and
 * a phone.
 *
 * O65 was first measured here: with the chrome open, `Library` sat over the
 * `single` and `two` buttons. Both are in the top bar now (§8.3), and the
 * first two cases ask the same question of it. O112 is the last block.
 *
 * The mocks are `App.test.tsx`'s, for the reasons that file's header gives:
 * `vi.mock('./resolver')` is what lets `App.tsx` load at all without the
 * generated wasm package. Only the network calls are replaced in `./api` and
 * `./library`, so the `<img src>`s are the real ones. Unless a server is
 * listening behind vite's dev proxy they fail there, and the proxy prints
 * `ECONNREFUSED` to stderr.
 *
 * CI does not run this project (`vite.config.ts`: no Chromium); the gate
 * does.
 */
```

2. Replace everything from the `/**` doc comment above `async function readerWithChrome(` to the end of the file with:

```tsx
/**
 * The reader at `width`×`height`, loaded, the chrome hidden. The wrapper
 * fills the viewport the way `#root` does under `index.css`, so `App`'s
 * `height: 100%` column lays out at the viewport's size.
 */
async function reader(width: number, height: number) {
  await page.viewport(width, height)
  mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <App work={WORK} onLeave={() => {}} />
    </div>,
  )
  await until(() => document.querySelector('.reader') !== null)
}

function press(key: string) {
  act(() => {
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key, bubbles: true }),
    )
  })
}

/** The same, with the chrome shown. */
async function readerWithChrome(width: number, height: number) {
  await reader(width, height)
  press('Tab')
  await until(() => button('Library') !== undefined)
}

const box = (selector: string) =>
  (document.querySelector(selector) as HTMLElement).getBoundingClientRect()

// 1440×900 is the browser pass's desktop; 768×1024 a portrait tablet's,
// below the breakpoint; 390×844 a phone's.
const SIZES: [number, number][] = [
  [1440, 900],
  [768, 1024],
  [390, 844],
]

/** The strip's height at a width: 160 px tiles from 900 px up, 96 below, plus 16. */
const stripAt = (width: number) => (width >= 900 ? 176 : 112)

describe('the reader’s two bars, in a browser', () => {
  it.each(SIZES)(
    'at %i×%i, each top-bar control is what a click at its centre reaches',
    async (width, height) => {
      await readerWithChrome(width, height)
      for (const name of ['Library', 'single', 'two', 'scroll', 'Settings']) {
        const control = button(name)
        expect(hitAtCentreOf(control), name).toBe(control)
      }
    },
  )

  it.each(SIZES)(
    'at %i×%i, the Library button overlaps no other control',
    async (width, height) => {
      await readerWithChrome(width, height)
      const leave = button('Library')
      const rect = leave.getBoundingClientRect()
      const covered = [...document.querySelectorAll('button')]
        .filter((b) => b !== leave)
        .filter((b) => overlaps(rect, b.getBoundingClientRect()))
        .map(
          (b) =>
            b.textContent ||
            b.getAttribute('aria-label') ||
            b.querySelector('img')?.getAttribute('alt') ||
            b.outerHTML.slice(0, 80),
        )
      expect(covered).toEqual([])
    },
  )

  it.each(SIZES)(
    'at %i×%i, the bars meet the reader’s edges',
    async (width, height) => {
      await readerWithChrome(width, height)
      const top = box('.reader-top-bar')
      const bottom = box('.reader-bottom-bar')
      expect([top.top, top.left, top.right, top.height]).toEqual([
        0,
        0,
        width,
        52,
      ])
      expect([bottom.bottom, bottom.left, bottom.right]).toEqual([
        height,
        0,
        width,
      ])
      // Its 1 px rule, the strip, the 2 px progress line and the 32 px readout.
      expect(bottom.height).toBe(1 + stripAt(width) + 2 + 32)
    },
  )

  it.each(SIZES)(
    'at %i×%i, the strip stands 176 px from 900 px up and 112 below',
    async (width, height) => {
      await readerWithChrome(width, height)
      const strip = (document.querySelector('.reader-bottom-bar') as HTMLElement)
        .firstElementChild as HTMLElement
      expect(strip.getBoundingClientRect().height).toBe(stripAt(width))
    },
  )

  it.each(SIZES)(
    'at %i×%i, the page area is the reader’s whole height, whether or not the bars are shown',
    async (width, height) => {
      await reader(width, height)
      const hidden = box('.page-area')
      expect([hidden.top, hidden.height, hidden.width]).toEqual([
        0,
        height,
        width,
      ])
      press('Tab')
      await until(() => button('Library') !== undefined)
      const shown = box('.page-area')
      expect([shown.top, shown.height, shown.width]).toEqual([
        0,
        height,
        width,
      ])
    },
  )

  it.each(SIZES)(
    'at %i×%i, the keyboard help docks 16 px inside both bars and the right edge',
    async (width, height) => {
      await readerWithChrome(width, height)
      press('h')
      await until(
        () => document.querySelector('[aria-label="Keyboard help"]') !== null,
      )
      const help = box('[aria-label="Keyboard help"]')
      const top = box('.reader-top-bar')
      const bottom = box('.reader-bottom-bar')
      expect(help.top).toBe(top.bottom + 16)
      expect(help.bottom).toBeLessThanOrEqual(bottom.top - 16)
      expect(help.right).toBe(width - 16)
    },
  )
})

describe('O112: the note list under the page, in a browser', () => {
  beforeEach(() => {
    localStorage.setItem(
      'taiyaku.prefs.annotations',
      JSON.stringify({
        markers: true,
        travel: true,
        reveal: true,
        notesList: 'below',
      }),
    )
    // Four notes on the slot on screen: O112's count, at which no row took a
    // click at its centre with the chrome open.
    ports.getNotes.mockResolvedValue({
      doc: {
        work: WORK,
        notes: [1, 2, 3, 4].map((n) => ({
          id: `note-${n}`,
          edition: 'raw',
          slot: 2,
          body: `note ${n}`,
          created: '2026-01-01T00:00:00.000Z',
          updated: '2026-01-01T00:00:00.000Z',
        })),
      },
      etag: 'n1',
    })
  })

  const rows = () =>
    [
      ...document.querySelectorAll(
        '[aria-label="Notes on this page"] [role="listitem"]',
      ),
    ] as HTMLButtonElement[]

  it.each(SIZES)(
    'at %i×%i, every row takes a click at its centre while the bars are away',
    async (width, height) => {
      await reader(width, height)
      await until(() => rows().length === 4)
      for (const row of rows()) expect(hitAtCentreOf(row)).toBe(row)
    },
  )

  it.each(SIZES)(
    'at %i×%i, the list steps aside under the bars, and no row takes a click meant for them',
    async (width, height) => {
      await reader(width, height)
      await until(() => rows().length === 4)
      press('Tab')
      await until(() => button('Library') !== undefined)
      const list = document.querySelector(
        '[aria-label="Notes on this page"]',
      ) as HTMLElement
      expect(getComputedStyle(list).visibility).toBe('hidden')
      for (const row of rows()) expect(hitAtCentreOf(row)).not.toBe(row)
    },
  )
})

describe('the reader in Ink whatever the theme, in a browser', () => {
  /**
   * The mechanism spec §8 rests on, in the app rather than in a probe: the
   * tokens are `light-dark()` pairs, and `color-scheme: dark` on `.reader`
   * picks their Ink half inside it even with Paper pinned on `<html>`. The
   * body is the control — outside the reader, Paper's text.
   */
  it('draws the reader in Ink on black with Paper pinned', async () => {
    document.documentElement.setAttribute('data-theme', 'paper')
    try {
      await reader(1440, 900)
      const root = getComputedStyle(
        document.querySelector('.reader') as HTMLElement,
      )
      expect(root.color).toBe('rgb(236, 228, 212)')
      expect(root.backgroundColor).toBe('rgb(0, 0, 0)')
      expect(getComputedStyle(document.body).backgroundColor).toBe(
        'rgb(0, 0, 0)',
      )
      press('Tab')
      await until(() => button('Library') !== undefined)
      expect(
        getComputedStyle(document.querySelector('.reader-top-bar') as HTMLElement)
          .color,
      ).toBe('rgb(236, 228, 212)')
      const outside = document.createElement('p')
      document.body.append(outside)
      expect(getComputedStyle(outside).color).toBe('rgb(35, 30, 23)')
      outside.remove()
    } finally {
      document.documentElement.removeAttribute('data-theme')
    }
  })
})
```

This replaces Task 1's `describe('the reader in Ink whatever the theme, in a browser', …)` with the version above, which adds the top bar's colour; there must be one such block in the file, not two. The helpers `until`, `button`, `hitAtCentreOf` and `overlaps` above the replaced region stay as they are.

- [ ] **Step 2: Run them**

```bash
pnpm -C web exec vitest run --project browser --reporter=default src/App.browser.test.tsx
```

Expected: PASS, 25 tests (8 cases at three sizes, and the theme check). A size that fails is read before anything is changed: `box()`'s figures are Chromium's, and a sub-pixel value (`51.99`) is a rounding question to settle by reading the rect, not by loosening `toEqual` blind.

- [ ] **Step 3: Watch them fail with their defects put back**

1. In `reader.css`, `.docked-panel`'s `top: 16px` → "the keyboard help docks" fails at all three sizes.
2. In `reader.css`'s `@media` block, `--strip-tile: 160px` → the strip and bars-meet-edges cases fail at 768 and 390.
3. In `TopBar.tsx`, `top: 8` in `barStyle` → "the bars meet the reader's edges" fails.
4. In `Chrome.tsx`, `position: 'relative'` in `bottomBarStyle` → "the page area is the reader's whole height" fails once the bars are shown.
5. In `NoteList.tsx`, `visibility: undefined` → O112's "steps aside" case fails on `visible`.
6. In `reader.css`, delete `color-scheme: dark;` from `.reader` → the theme check fails on `rgb(35, 30, 23)`.

- [ ] **Step 4: Commit**

```bash
pnpm -C web run lint
git add web/src/App.browser.test.tsx
git commit -m "Style pass 3: the two bars, the panels and O112 measured in the browser project"
```

---

### Task 8: Measure it in a browser and on the phone, record it, and run the gate

Steps 3 to 6 need the running app, and Step 6 needs the user holding their iPhone. **The controller runs this task, not a subagent.**

**Files:**
- Modify: `docs/notes.md` (append a section), `docs/status.md` (O112 closed; O134 and O135 opened; counts; O65's row marked), `docs/superpowers/specs/2026-09-17-style-pass-design.md` (anything the measurements contradict)

**Interfaces:**
- Consumes: every earlier task.
- Produces: the figures `notes.md` records, O112 closed and O134 and O135 open in `status.md`, and a green gate.

- [ ] **Step 1: Serve the scratch copy of the real library**

This branch changes nothing under `crates/` (`git diff --stat main -- crates` prints nothing). Build `serve` at this tree, which is quick if `target/debug/taiyaku` is current:

```bash
cargo build --bin taiyaku
```

The scratch copy from ① is at `target/style-pass/lib/library.db`, with `local` / `stylepass`. Skip whatever is already listening (`ss -ltn | command grep -E ':(8491|5291) '`); otherwise start each with the Bash tool's `run_in_background`:

```bash
exec target/debug/taiyaku serve --addr 127.0.0.1:8491 --db target/style-pass/lib/library.db --cache target/style-pass/cache --insecure-cookies --log-requests target/style-pass/three/requests.log
```

```bash
TAIYAKU_SERVE=http://127.0.0.1:8491 exec web/node_modules/.bin/vite web --host 127.0.0.1 --port 5291 --strictPort
```

If `serve` was already running without `--log-requests target/style-pass/three/requests.log`, stop it by PID (`ss -ltnp 'sport = :8491'`) and start it as above: Step 6 reads that log.

- [ ] **Step 2: Pick a work, and give it notes**

With Playwright at 1440×900, sign in at `http://127.0.0.1:5291/` as `local` / `stylepass`, then find a work with two editions or more:

```js
async () => (await (await fetch('/api/works')).json())
  .filter((w) => w.editions.length >= 2)
  .slice(0, 3)
  .map((w) => ({ slug: w.slug, title: w.title, editions: w.editions }))
```

Record the slug chosen. Open `/read/<slug>`, turn two pages with `ArrowLeft` (or `ArrowRight` for a left-to-right edition), and write two notes on that page: press `n`, type `first note`, press `Escape`; press `n`, type `second note`, press `Escape`. Open Settings with `s`, set the notes list to `below` in the Annotations section, close it with its Close button, and press `r` so the notes show. These writes land in the scratch copy only.

- [ ] **Step 3: The browser pass, the whole matrix**

At 390×844 and at 1440×900, in Paper and in Ink (set `localStorage['taiyaku.prefs.theme']` to `{"choice":"paper"}` or `{"choice":"ink"}` and reload), on the chosen work, screenshot each state to `target/style-pass/three/pass/<state>-<theme>-<width>.png`:

1. `reading` — the chrome hidden, `single`.
2. `chrome` — `Tab`, `single`.
3. `two` — the chrome shown, `two` pressed in the top bar.
4. `scroll` — the chrome shown, `scroll` pressed; press `single` again afterwards.
5. `settings` — ⚙ pressed.
6. `help` — the chrome shown, `h` pressed.
7. `notes` — the chrome hidden, the notes under the page.
8. `notes-chrome` — the same with `Tab`: the list steps aside (O112).
9. `correcting` — `c` pressed; at 1440 with the pointer over a tile, so the hover preview shows.
10. `borrowed` — a borrowed page with the chrome shown, which the whole-branch review found the top bar covering before the mark was docked below the bar's line. Reach one by pressing a digit for an edition that lacks the slot on screen, or take the strip to a slot only one edition has; if this work has no such slot, say so and skip it.
11. `scroll-jump` — scroll mode, the chrome shown, after clicking a strip tile: the page it lands on must start below the top bar, not under it. Then, still in scroll mode, press `Tab` while parked at a page's top: the page area's `scroll-padding-top` moves the *next* scroll target, and `useScrollToCurrent` is not keyed on the chrome, so the bar covers that page's top edge until the next move. Record what it looks like and whether it reads as a defect worth its own item.

That is 44 screenshots. Read every one, and record what each shows and anything unreadable, clipped, still drawn in a browser-default grey or white, or overlapping. Then compare each state's Paper and Ink screenshots, which should match:

```bash
for f in target/style-pass/three/pass/*-paper-*.png; do printf '%s ' "$(basename "$f")"; compare -metric AE "$f" "${f/-paper-/-ink-}" null: 2>&1; echo; done
```

Record each count. A state whose pair differs by more than a few hundred pixels is read side by side and the difference named: something in the reader is following the theme.

- [ ] **Step 4: Measure what the screenshots cannot say**

At 390×844 and at 1440×900, in Paper, with the chrome shown, evaluate:

```js
() => {
  const r = (s) => {
    const b = document.querySelector(s)?.getBoundingClientRect()
    return b && [b.top, b.bottom, b.left, b.right, b.height].map((n) => Math.round(n * 10) / 10)
  }
  return {
    top: r('.reader-top-bar'),
    bottom: r('.reader-bottom-bar'),
    strip: r('.reader-bottom-bar > div'),
    page: r('.page-area'),
    sideways: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    barText: getComputedStyle(document.querySelector('.reader-top-bar')).color,
    title: getComputedStyle(document.querySelector('.reader-title-work')).fontFamily,
    ground: getComputedStyle(document.body).backgroundColor,
  }
}
```

Expected: `top` `[0, 52, 0, W, 52]`; `bottom` ending at the viewport's height, `211` tall at 1440 and `147` at 390; `strip` `176` and `112` tall; `page` from `0` to the viewport's height; `sideways` `0`; `barText` `rgb(236, 228, 212)`; `title` beginning `"Shippori Mincho B1"`; `ground` `rgb(0, 0, 0)`. Hide the chrome and evaluate `r('.page-area')` again: the same. Record every figure.

At 390×844, on the library's `/`, `/series/<a series id>` and `/search?q=a`, evaluate `document.documentElement.scrollWidth - document.documentElement.clientWidth`. Expected: `0` each, as after ② — the safe-area padding is zero here.

- [ ] **Step 5: Count the tests**

```bash
pnpm -C web exec vitest run --project node --reporter=default 2>&1 | command grep -E '^\s+Tests '
pnpm -C web exec vitest run --project jsdom --reporter=default 2>&1 | command grep -E '^\s+Tests '
pnpm -C web exec vitest run --project browser --reporter=default 2>&1 | command grep -E '^\s+Tests '
```

Expected: `node` 1,204 + 10 (Task 1) + 7 (Task 2) + 4 (Task 5) = **1,225**; `jsdom` 1,000 + 2 (Task 1) + 9 (Task 3) + 9 (Task 4: six in `App.test.tsx`, three more in `Chrome.test.tsx` than it had) + 7 (Task 5) + 10 (Task 6) = **1,037**; `browser` 37 + 1 (Task 1) − 4 (O65's four) + 24 (Task 7) = **58**. If a run disagrees, the run is right: write its figure and find the difference before going on.

- [ ] **Step 6: The iPhone, over the tailnet**

Expose vite to the tailnet only (not Funnel):

```bash
tailscale serve --bg --https=443 http://127.0.0.1:5291
```

Stop vite by PID and start it again, with the Bash tool's `run_in_background`, allowing the tailnet name:

```bash
__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS=davey-endeavor.walrus-census.ts.net TAIYAKU_SERVE=http://127.0.0.1:8491 exec web/node_modules/.bin/vite web --host 127.0.0.1 --port 5291 --strictPort
```

Confirm from this machine first: `curl -s -o /dev/null -w '%{http_code}\n' https://davey-endeavor.walrus-census.ts.net/` prints `200`. If it prints `403` with vite's "Blocked request" text, the variable did not take: add `allowedHosts: ['davey-endeavor.walrus-census.ts.net']` to `server` in `web/vite.config.ts` for the pass only, and remove it before committing.

Ask the user to open `https://davey-endeavor.walrus-census.ts.net/` on their iPhone and sign in as `local` / `stylepass`. If sign-in answers "this request's Origin does not match this server", restart `serve` with `--trusted-proxy 127.0.0.1/32` added, and ask again.

Then ask the user, one at a time, and record each answer:

1. On the library, held upright: does the phone bar sit clear of the status bar, and the last row of covers clear of the home indicator?
2. Held sideways: does everything on the library stay clear of the notch and the rounded corners — the phone bar, the covers, and the drawer when ☰ opens it?
3. Opening the chosen work from the library: is there a flash of paper before the reader draws, or is it black from the first moment?
4. In the reader, held upright and sideways: is the ground black edge to edge, including behind the status bar and under Safari's toolbar? What colour is Safari's toolbar over the reader, and over the library?
5. Does any part of the page, or anything drawn over it, sit under the notch or the home indicator where it shouldn't?

A phone cannot open the chrome until ④, so the bars' own safe areas, their 44 px targets and Safari's resolution of the tokens inside `.reader` are ④'s device pass (spec §8.6); nothing here asks about them.

Read the user-agent of the iPhone's requests before believing any answer:

```bash
command grep -o '"user_agent":"[^"]*iPhone[^"]*"' target/style-pass/three/requests.log | sort | uniq -c
```

Expected: at least one line naming `iPhone` and `Safari`. If the log carries no user-agent field, read it with `command grep -m3 iPhone target/style-pass/three/requests.log` and record what it does carry. Afterwards:

```bash
tailscale serve --https=443 off
tailscale serve status
```

Expected: `No serve config`. Restore vite to Step 1's command.

- [ ] **Step 7: Write it down**

Append to `docs/notes.md` a section headed `## Style pass ③: the reader — <today's date>`, in this file's usual style (what was measured, how, the figures), holding:

1. The planning probes: `light-dark()` resolved per element in Chromium 151 and Firefox 153, under Paper pinned and System with the OS emulated light and dark; jsdom keeping `var()` and `calc(var())` verbatim in inline styles; Biome accepting `env()`, `:root:has()`, the space-separated `rgb()`, a custom property redefined in `@media (width < 900px)` and `clip-path: inset(50%)`; and today's layout row measured at 37 px, the page area at 863 px, at 1440×900 (987 at 768×1024).
2. Step 4's figures, each size and each state.
3. Step 3's observations, state by state and theme by theme, with each Paper/Ink pair's `compare` count.
4. Step 6's answers, each with the user-agent it was read against.

In `docs/status.md`:

1. **The test counts.** In ``Six crates, 2231 tests, plus 2,204 in `web/`'s TypeScript — 1,204 in the `node` `` and the line after it, replace `2,204`, `1,204`, `1,000` and `thirty-seven` with Step 5's total of node and jsdom, its `node` figure, its `jsdom` figure and its `browser` figure in words. `2231` does not move: nothing under `crates/` changed.
2. **O112 closes.** In its row, replace the opening

```markdown
| O112 | [ ] | **With the chrome open,
```

with

```markdown
| O112 | [x] | **Closed <today's date>, on the style pass's ③ branch, once the user had chosen among its options with the new bars drawn (spec 2026-09-17 §8.1): while the chrome is shown, the note list under the page is `visibility: hidden` — still mounted and still taking its space, so the page area does not change on `Tab` — and comes back when the chrome goes. Measured in the browser project at 1440×900, 768×1024 and 390×844 with four notes on the slot on screen: with the chrome hidden every row takes a click at its centre, and with it shown the list's computed visibility is `hidden` (`App.browser.test.tsx`, "O112"). The band above the strip where O65 docked `Library` is empty, since the button moved into the top bar (§8.3). The cost the user accepted: no note can be read or opened while the strip is up; the readout's note count still says how many there are.** **With the chrome open,
```

and in the same row's site column replace

```markdown
`Chrome.tsx`'s `stripStyle` and `readoutStyle`
```

with

```markdown
`Chrome.tsx`'s bottom bar, `NoteList.tsx`'s `covered`
```

3. **O65's row** keeps its prose as written. At the end of its description cell — immediately before the ` | ` that starts its site column, whose text begins `` `Chrome.tsx`'s `leaveButtonStyle` `` — add:

```markdown
 *(Since style pass ③ the button is in the reader's top bar, which comes and goes with the strip — spec 2026-09-17 §8.3 — so `leaveButtonStyle` and `aboveStrip` no longer exist.)*
```

4. **O134 and O135 open.** After the `| O133 |` row, add these two rows, each one line:

```markdown
| O134 | [ ] | **The reader's page strip is soft on a phone.** Its tiles are the 160 px thumbnail (`cover.rs`'s `THUMB_LONG_EDGE`), drawn 96 px tall below 900 px since style pass ③ (spec 2026-09-17 §8.4): stretched 1.8× on a 3× screen, against 3× at 160 — the blur O133 found on the library's covers, which were the same thumbnail, and fixed there by sizing covers per screen. **What would fix it**: the server sending a larger tile to a dense screen, as O133 does for covers, which multiplies the per-edition tile cache O11 records. **What would decide it**: whether the user minds the strip's softness on the iPhone, which ④'s device pass — the first to show the strip on a phone — can ask | `cover.rs`'s `THUMB_LONG_EDGE`, `Filmstrip.tsx`'s tiles, `api.ts`'s `thumbUrl` | the style pass's ③ design, 2026-09-19 |
| O135 | [ ] | **For the accessibility pass: the keyboard does not reach the reader's top bar, and Settings closes on neither `Escape` nor `s` once ⚙ has opened it.** `keyPlan` answers `Tab` with `toggle-chrome` and leaves its focus move to the browser, so one `Tab` shows the bars and the next hides them. **Measured in Chromium at 1440×900 with real keys, on this branch and on `main` alike**: the first `Tab` focuses the strip tile after the current one, not any top-bar control, so *no* top-bar control is reachable by `Tab`; `Space` then activates that tile and turns a page rather than swapping editions; a second `Tab` returns focus to `<body>`. Every top-bar control has another keyboard route — `L`, `s`, and Layout inside Settings — so nothing is out of reach. Settings, a dialog with a Close button since style pass ③, has no `Escape` arm in `keyPlan`'s `Escape` branch, where Help and the notes panel do; and after ⚙ opens it the click leaves focus on ⚙, so `keyPlan`'s interactive-target guard stands `s` down too — Close, or a click outside, is the way out. Left to the accessibility pass the user put after ⑤ (spec 2026-09-17 §5) | `keyplan.ts`'s `keyPlan` (`Tab`'s `toggle-chrome`, the `Escape` branch, the interactive-target guard), `TopBar.tsx`, `Settings.tsx` | the style pass's ③ design and its whole-branch review, 2026-09-19 |
```

5. **The counts.** `One hundred and thirty-three, each found by` becomes `One hundred and thirty-five, each found by`. `**Eighty-six are now closed` becomes `**Eighty-seven are now closed`, and in that sentence's list insert `O112, ` between `O111, ` and `O113`. `— leaving forty-seven.**` becomes `— leaving forty-eight.**`. The italic parenthetical after `leaving forty-seven.**` ends `…and a download-only account had been served every cover size, and took it to forty-seven.)*`; immediately before that closing `)*`, insert ` O112 closed on <today's date> on the style pass's ③ branch, once the user had chosen among its options with the new bars drawn, and took it to forty-six. O134 and O135 opened on that branch, from its design, and took it to forty-eight.` In `What remains:`, `the forty-seven open items` becomes `the forty-eight open items`; the count after it of those found by a whole-branch review moves only if O112 was one of them — read the sentence, and lower it by one if O112 is named or counted there.
6. **The sweep**, which must agree with itself:

```bash
command grep -c '^| O[0-9]' docs/status.md
command grep -c '^| O[0-9]* | \[x\]' docs/status.md
command grep -n -A1 '^What remains:' docs/status.md
```

Expected: `135`, `87`, and a `What remains` line saying forty-eight. Each new row has exactly six `|` outside its backticks.

7. **The bare O-number.** `command grep -rn 'O112' docs web/src` and read every hit: a present-tense sentence elsewhere saying O112 is open is corrected or marked (`*O112 has since closed — see its entry.*`); historical narrative is kept as written.

8. **The `web/` row in "What is built"** (around line 335) still describes the reader's chrome as "a floating chrome". Correct it to the two bars, in that row's own register.

9. **What the whole-branch review left for the spec** — in `docs/superpowers/specs/2026-09-17-style-pass-design.md` §8: the safe-area padding went on `.library` rather than on `<main>` as §8.2 says (harmless at and above 900 px, and it moves the sidebar off a landscape phone's notch; below 900 px the sticky phone bar is inset with it, so its hairline stops short of both edges — say which the device pass saw); the borrowed page's mark now docks below the top bar's line, by the panels' rule in §8.1, which §8.5 should record with the reason (the top bar covered it, and it is the pointer's only way into the offers); and §8.3's `Tab` paragraph, which is marked *reasoned, not measured*, is replaced by O135's measurement.

In `docs/superpowers/specs/2026-09-17-style-pass-design.md` — prose wrapped at 80 columns, so match each phrase across line breaks and re-wrap what you write: anything Steps 3, 4 or 6 contradicted in §8, corrected in place and said to be measured; and in §8.6's last bullet, the two items filed there named by their numbers, O134 and O135.

- [ ] **Step 8: Run the gate**

Run with the Bash tool's `run_in_background`:

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh > target/style-pass/three/gate.log 2>&1; echo "exit $?"
```

When it finishes, read the end of `target/style-pass/three/gate.log`. Expected: `exit 0` and an `all green` line; quote that line exactly in the PR body. A failure is read in full and fixed, never summarised.

- [ ] **Step 9: Commit**

```bash
git add docs/notes.md docs/status.md docs/superpowers/specs/2026-09-17-style-pass-design.md
git commit -m "Style pass 3: measured in the browser and on the phone, recorded; O112 closed, O134 and O135 filed; gate green"
```

---

## After the tasks

Not tasks in this plan, but required before the PR opens (spec §5, "Every PR, before it is opened"):

1. A whole-branch review of `main..style-pass-3-reader`, then a review of each fix round until one finds nothing.
2. Every hand mutation above re-run by whoever dispatched the tasks, control run first.
3. The pre-merge claim check over the PR body, `docs/status.md`, `docs/notes.md` and the spec.
4. The PR through the Gitea MCP, never `gh`. The body is the squash commit message, is not hard-wrapped, and carries no attribution.

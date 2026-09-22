# O140 — the chrome's build cost — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make opening the reader's chrome cheap — 274 ms today — by windowing the filmstrip rail so it builds ~20 tiles instead of 237, then mounting the chrome once and freezing it while hidden.

**Architecture:** Two independent mechanisms, in two commits. First, `Filmstrip` gains a nominal per-tile width (`tileUnits`) so tile positions are derivable without loading any image, which lets it render a window of tiles between two `calc()`-sized spacers. Second, `Chrome` stops returning `null` after its first show: it hides its subtree with `display: none` and re-renders the *same React element object* while hidden, so React skips reconciling it.

**Tech Stack:** React 19, TypeScript, Vite, Vitest (three projects: `node`, `jsdom`, `browser`/Playwright-Chromium), Biome.

## Global Constraints

- **Branch:** `o140-chrome-build-cost`, already created, spec already committed (`b556e9e`).
- **Spec:** `docs/superpowers/specs/2026-09-20-o140-chrome-build-cost-design.md`. Every figure quoted below comes from its §1 and §2.
- **No new dependencies.** This repo's rule is that a dependency needs a stated reason; nothing here earns one. The freeze is hand-rolled, not `react-freeze`.
- **Pure logic does not live in `.tsx`.** `biome.jsonc` enables `style/useComponentExportOnlyModules` by name — a pure function exported from a component file defeats Fast Refresh. New pure functions go in `web/src/tiles.ts`.
- **Longhand `border*` properties only** in `Filmstrip.tsx`'s style objects. React warns when an element's style toggles between a shorthand and a longhand of the same property across renders, which is exactly what happens as a tile gains and loses `currentTileStyle`.
- **No AI attribution** in any commit message, comment, or doc.
- **The gate** is `scripts/check-all.sh`. Quote its final `all green` line when reporting; piping it through `tail` masks a failure.
- **Vitest reporter:** under Claude Code, vitest selects its `agent` reporter, which hides passing tests' console output. Use `--reporter=default` when a run's console output matters.

---

## File Structure

| File | Responsibility | Task |
| --- | --- | --- |
| `web/src/tiles.ts` | Pure tile arithmetic. Gains `tileUnits`, `spacerWidth`, `stripWindow`, `focusAtFraction`. | 1, 3, 4 |
| `web/src/tiles.test.ts` | Unit tests for the above, in the `node` project. | 1, 3, 4 |
| `web/src/styles/reader.css` | Gains `--strip-unit`, `--strip-gap`, `--strip-tile-border`. | 2 |
| `web/src/Filmstrip.tsx` | Explicit tile widths; renders a window plus two spacers; extends the window on scroll. | 2, 3, 4 |
| `web/src/Filmstrip.test.tsx` | jsdom assertions on style attributes and which tiles render. | 2, 3, 4 |
| `web/src/Filmstrip.browser.test.tsx` | **New.** Real layout: windowed `scrollWidth` equals unwindowed. | 3 |
| `web/src/Chrome.tsx` | Mounts once; hides with `display: none`; freezes its subtree while hidden. | 5, 6 |
| `web/src/Chrome.test.tsx` | The inverted mount assertions and the freeze test. | 5, 6 |
| `docs/notes.md`, `docs/status.md` | The re-measurement, and O140's five-part close. | 7 |

---

### Task 1: `tileUnits` — how wide a tile is, without its image

**Files:**
- Modify: `web/src/tiles.ts` (append after `tilePages`)
- Test: `web/src/tiles.test.ts` (append a new `describe`)

**Interfaces:**
- Consumes: `Group` from `./pairing` (already imported by `tiles.ts`).
- Produces: `tileUnits(group: Group): 1 | 2`. Tasks 2, 3 and 4 all use it.

- [ ] **Step 1: Write the failing test**

Append to `web/src/tiles.test.ts`:

```tsx
describe('tileUnits', () => {
  it('is one for a singleton group', () => {
    const slots = whole(3)
    const [group] = groups(slots, 'single', 'rtl')
    expect(tileUnits(group)).toBe(1)
  })

  it('is two for a non-spread pair — two images side by side', () => {
    const slots = whole(4)
    const [pair] = groups(slots, 'two', 'rtl')
    expect(tileUnits(pair)).toBe(2)
  })

  // The case that makes this function necessary rather than derivable.
  // `tilePages` collapses a joined spread to ONE entry, because its two
  // slots share a page — so a width taken from `tilePages(group).length`
  // would call a spread one unit wide. It is one image, but that image is a
  // double-page scan and draws twice as wide as a page.
  it('is two for a joined spread, though tilePages gives it one entry', () => {
    const spread = groups(bSlots, 'single', 'rtl').find((g) => g.spread)!
    expect(tilePages(spread, bSlots)).toHaveLength(1)
    expect(tileUnits(spread)).toBe(2)
  })

  it('is one for a spread half addressed on its own while correcting', () => {
    const [rTile, lTile] = correctionTiles(bSlots).slice(2, 4)
    expect(tileUnits(rTile)).toBe(1)
    expect(tileUnits(lTile)).toBe(1)
  })
})
```

Add `tileUnits` to the existing `import { tilePages } from './tiles'` line.

- [ ] **Step 2: Run it and watch it fail**

```bash
pnpm -C web exec vitest run --project node tiles.test.ts --reporter=default
```

Expected: FAIL — `tileUnits is not a function` (or a TypeScript error from `tsc -b`; either is the failure).

- [ ] **Step 3: Implement**

Append to `web/src/tiles.ts`:

```ts
/**
 * How many nominal page-widths one tile occupies.
 *
 * This is what lets the rail be windowed at all: without it, a tile's width
 * is whatever its thumbnail turns out to draw at, so nothing can say where
 * tile 150 begins without first loading tiles 0 through 149 — and a
 * not-yet-loaded tile lays out at 4 px, which is why a cold strip's tiles
 * sit collapsed until their images arrive.
 *
 * **Deliberately not `tilePages(group, slots).length`.** That would be
 * wrong in exactly one case, and it is the case most likely to be tested
 * last: `tilePages` collapses a joined spread to a single entry, because its
 * two slots share one page. That page is a double-page scan and draws twice
 * as wide as an ordinary page, so a spread is two units while being one
 * image. A `two`-mode pair is two units for the other reason — two images
 * side by side. Both answers are 2; neither follows from the entry count.
 */
export function tileUnits(group: Group): 1 | 2 {
  return group.spread || group.slots.length === 2 ? 2 : 1
}
```

- [ ] **Step 4: Run it and watch it pass**

```bash
pnpm -C web exec vitest run --project node tiles.test.ts --reporter=default
```

Expected: PASS, 4 new tests in the `tileUnits` describe.

- [ ] **Step 5: Commit**

```bash
git add web/src/tiles.ts web/src/tiles.test.ts
git commit -m "O140: tileUnits — a tile's width without its image

The rail cannot be windowed while a tile is as wide as its thumbnail
happens to draw. tileUnits derives it from the group instead: two for a
joined spread (one image, twice as wide) or a two-mode pair (two images),
one otherwise. Not tilePages().length, which calls a spread one unit."
```

---

### Task 2: Give a tile an explicit width

**Files:**
- Modify: `web/src/styles/reader.css:52` (the `--strip-tile` block) and its `@media (width < 900px)` block
- Modify: `web/src/Filmstrip.tsx` — `railStyle`, `tileStyle`, `currentTileStyle`, `resolvedTileStyle`, and the tile `<button>`
- Test: `web/src/Filmstrip.test.tsx`

**Interfaces:**
- Consumes: `tileUnits` from Task 1.
- Produces: the CSS custom properties `--strip-unit`, `--strip-gap`, `--strip-tile-border`, which Task 3's `spacerWidth` writes `calc()` against.

- [x] **Step 1: Measure the drawn tile, at both breakpoints — ALREADY DONE**

**The controller took this measurement on 2026-09-21; the numbers are in
Step 2 below and you do not need to re-take them.** What was measured, on
`akame-ga-kill----v01--2015--cbz` with the chrome open, sampling only tiles
holding a single `<img>`:

| viewport | `--strip-tile` | drawn image | one-unit tile | two-unit tile |
| --- | --- | --- | --- | --- |
| 1440×900 | 160px | 96.97 × 145 | button 100.97 | button 197.33 |
| 390×844 | 96px | 54.16 × 81 | button 58.16 | button 112 |

Rail `column-gap` measured 6px at both. A two-unit tile measured almost
exactly twice a one-unit tile at both sizes, which is `tileUnits`' model
confirmed against the real rail rather than only against its own unit tests.

**Why these are literals and not a `calc()` off `--strip-tile`:** the unit is
0.606 of `--strip-tile` at 1440 and 0.563 at 390, because the drawn image
height is `--strip-tile - 15px` at both — and that 15 px is the classic
scrollbar O138 already identified as a platform setting rather than anything
this code chooses. A `calc()` would therefore be right at one breakpoint and
wrong at the other. It also means the figure shifts by about a pixel on a
platform drawing overlay scrollbars; that stops mattering once the width is
explicit, because the tile is then `--strip-unit` wide regardless and the
image letterboxes inside it.

<details><summary>The original instruction, kept for the record</summary>

The unit must be the width a one-page tile actually draws today, not a
number reasoned out from `--strip-tile`. `--strip-tile` is a *height* (160 px
from 900 px up, 96 below) and the drawn image is shorter than it — measured
at 390×844 the image is 54×81 inside a 58 px button, against a
`--strip-tile` of 96. Do not try to derive one from the other.

Serve the app as the spec's §0 describes, then in the browser:

```js
const rail = document.querySelector('.reader-bottom-bar').firstElementChild.firstElementChild
const img = rail.querySelector('button img')
;({ tile: rail.querySelector('button').getBoundingClientRect().width,
    image: img.getBoundingClientRect().width,
    stripTile: getComputedStyle(document.querySelector('.reader')).getPropertyValue('--strip-tile') })
```

Run it at **1440×900** and at **390×844**, on a volume with no joined spread
near the current slot so the sampled tile is genuinely one unit. Record both
image widths — they are the two `--strip-unit` values Step 2 writes.

</details>

- [ ] **Step 2: Add the three custom properties**

In `web/src/styles/reader.css`, after the `--strip-tile: 160px;` declaration
and its comment:

```css
  /* One nominal page-width in the strip, and the two figures the rail's own
   * geometry is made of. `--strip-unit` is **measured, not derived**: it is
   * the width a one-page tile's image actually draws at this breakpoint
   * (O140, 2026-09-21), which is not `--strip-tile` times an aspect —
   * `--strip-tile` is a height and the image draws shorter than it. A tile
   * is given this width explicitly so the rail can be windowed: `tiles.ts`'s
   * `spacerWidth` sums it over the tiles a spacer stands in for, and nothing
   * has to measure a tile to know where the next one starts. */
  --strip-unit: 97px;
  --strip-gap: 6px;
  --strip-tile-border: 2px;
```

And in the `@media (width < 900px)` block beside `--strip-tile: 96px;`:

```css
    --strip-unit: 54px;
```

**Both figures are measured — 96.97 px at 1440×900 and 54.16 px at 390×844,
rounded to the nearest pixel. Use them exactly; do not re-derive them from
`--strip-tile`, which Step 1 records as giving a different ratio at each
breakpoint.**

- [ ] **Step 3: Write the failing tests**

Append to the `describe('tiles', ...)` block in `web/src/Filmstrip.test.tsx`:

```tsx
  // jsdom has no layout engine, so these assert the style the component
  // *writes*, not a measured box. The measured box is the browser test's
  // job (`Filmstrip.browser.test.tsx`).
  it('gives every tile an explicit width in nominal page-widths', () => {
    const host = mount(<Filmstrip {...props()} />)
    for (const btn of buttons(host)) {
      expect(btn.style.width).toBe('calc(var(--strip-unit) * 1)')
    }
  })

  it('gives a joined spread twice the width of a single page', () => {
    // One spread group, two slots sharing page 2, beside two ordinary pages.
    const spreadGroups: Group[] = [{ slots: [0] }, { slots: [2, 3], spread: true }]
    const host = mount(
      <Filmstrip {...props({ groups: spreadGroups, currentIndex: 0 })} />,
    )
    const [single, spread] = buttons(host)
    expect(single.style.width).toBe('calc(var(--strip-unit) * 1)')
    expect(spread.style.width).toBe('calc(var(--strip-unit) * 2)')
  })
```

- [ ] **Step 4: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx --reporter=default
```

Expected: FAIL — `expected '' to be 'calc(var(--strip-unit) * 1)'`, twice.

- [ ] **Step 5: Implement**

In `web/src/Filmstrip.tsx`, import `tileUnits` alongside `tilePages`:

```ts
import { tilePages, tileUnits } from './tiles'
```

Change `railStyle`'s gap to the custom property, so the spacer arithmetic and
the rail cannot disagree about it:

```ts
const railStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--strip-gap)',
  height: '100%',
  overflowX: 'auto',
  overflowY: 'hidden',
  padding: '6px 8px',
  boxSizing: 'border-box',
}
```

Change `tileStyle`'s border width to the same custom property — keeping the
longhand form the file's own comment requires — and leave `currentTileStyle`
and `resolvedTileStyle` spreading it as they already do:

```ts
const tileStyle: CSSProperties = {
  flex: '0 0 auto',
  display: 'flex',
  height: '100%',
  padding: 0,
  margin: 0,
  borderWidth: 'var(--strip-tile-border)',
  borderStyle: 'solid',
  borderColor: 'transparent',
  borderRadius: 4,
  background: 'none',
  cursor: 'pointer',
}
```

Give the thumbnail a width to letterbox inside, replacing `width: 'auto'`:

```ts
// `width: 100%` with `object-fit: contain`, rather than the `width: auto`
// this replaced. A tile now has a width of its own (`tileUnits`), so the
// image fits inside that box instead of setting it. The visible consequence,
// accepted in the O140 design §3: a page whose aspect is not the nominal
// letterboxes inside its tile rather than widening it. The never-crop rule
// `modules/fit.ts`'s `pageStyle` follows is unchanged — `contain` is what
// keeps it.
const tileImgStyle: CSSProperties = {
  height: '100%',
  width: '100%',
  objectFit: 'contain',
  display: 'block',
  pointerEvents: 'none',
}
```

And in the tile `<button>`, merge the width onto whichever of the three tile
styles applies:

```tsx
            style={{
              ...(isCurrent
                ? currentTileStyle
                : isResolved
                  ? resolvedTileStyle
                  : tileStyle),
              width: `calc(var(--strip-unit) * ${tileUnits(group)})`,
            }}
```

- [ ] **Step 6: Run the tests and watch them pass**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx --reporter=default
```

Expected: PASS, including every pre-existing case in the file.

- [ ] **Step 7: Look at it in a real browser**

Serve the app per the spec's §0 and open the chrome at 390×844 and at
1440×900. Confirm the strip still reads as a strip: tiles evenly spaced, the
current tile's accent border where it was, no tile squashed to nothing. A
screenshot at each size, kept with the measurement notes.

This step is not optional and is not replaceable by the jsdom assertions —
they pin the style string, which is exactly what a wrong `--strip-unit`
would leave looking correct.

- [ ] **Step 8: Commit**

```bash
git add web/src/styles/reader.css web/src/Filmstrip.tsx web/src/Filmstrip.test.tsx
git commit -m "O140: a tile gets an explicit width

--strip-unit is the width a one-page tile's image measures at each
breakpoint, taken from the running app rather than derived from
--strip-tile, which is a height the image draws shorter than. A tile is
now that width times its tileUnits, and the thumbnail letterboxes inside
it instead of setting it. Nothing is windowed yet; this is what makes
windowing possible."
```

---

### Task 2a: The nominal width is a floor, not a fixed size

**Added 2026-09-21, after Task 2 landed and was driven in a real browser.**
Task 2 as written set `width` on the tile and `width: 100%` on the image.
Measured on the running app, that did two things the design did not intend:

1. The button computes `box-sizing: border-box`, so `width: calc(unit * n)`
   is the tile's **outer** width with the 2 px border inside it. Every
   ordinary tile became 97 px where it had been 100.97 px, and every
   thumbnail 93 px where it had been 96.97 px.
2. A page whose thumbnail is genuinely landscape — 160×120 on
   `akame-ga-kill----v01--2015--cbz`, a double-page scan the sidecar never
   marked `spread: true`, so `tileUnits` correctly gives it 1 — went from
   drawing 193×145 to letterboxing at about 93×70. Roughly a quarter of the
   area, on the one kind of tile that is a landmark when a reader is hunting
   a page.

**The user chose, on 2026-09-21, to make the nominal width a floor**: an
unrendered tile occupies exactly its nominal width, so the window and the
spacers still compute, and a loaded image is allowed to exceed it and draw as
it does today.

**Files:** `web/src/Filmstrip.tsx`, `web/src/Filmstrip.test.tsx`.

- [ ] **Step 1: Change the tile from a fixed width to a floor**

In the tile `<button>`, replace the `width` merged onto the tile style with a
`minWidth` that carries the borders too:

```tsx
            style={{
              ...(isCurrent
                ? currentTileStyle
                : isResolved
                  ? resolvedTileStyle
                  : tileStyle),
              minWidth: `calc(var(--strip-unit) * ${tileUnits(group)} + var(--strip-tile-border) * 2)`,
            }}
```

`minWidth`, not `width`, and the borders added explicitly because the button
is `box-sizing: border-box`: a tile that has not rendered its image occupies
exactly this, and one that has is free to be wider.

- [ ] **Step 2: Give the image its width back**

`tileImgStyle` returns to `width: 'auto'`, so a loaded thumbnail sets its
tile's width the way it did before O140:

```ts
// `height` fixed, `width: auto` so aspect ratio is preserved — the same
// never-crop rule `modules/fit.ts`'s `pageStyle` follows for a page, just at
// tile scale. A tile is deliberately *not* on that rule's upscale switch: a
// thumbnail enlarged past its source helps nobody find a page.
//
// The tile's `minWidth` (see the tile `<button>`) is what gives an
// unrendered tile a width without this image: a floor, not a clamp. A page
// that draws wider than its nominal — a double-page scan the sidecar did not
// mark `spread: true`, which `tileUnits` therefore calls one unit — still
// draws at its own width rather than letterboxing inside a box a quarter of
// its size. Measured on the running app: 197.33 px against a nominal 101.
const tileImgStyle: CSSProperties = {
  height: '100%',
  width: 'auto',
  objectFit: 'contain',
  display: 'block',
  pointerEvents: 'none',
}
```

- [ ] **Step 3: Update the two tests Task 2 added**

They assert `btn.style.width`; they must now assert `btn.style.minWidth`, and
the expected string gains the border term:

```tsx
  it('gives every tile a floor width in nominal page-widths', () => {
    const host = mount(<Filmstrip {...props()} />)
    for (const btn of buttons(host)) {
      expect(btn.style.minWidth).toBe(
        'calc(var(--strip-unit) * 1 + var(--strip-tile-border) * 2)',
      )
    }
    // A floor, not a fixed size — a loaded image may draw wider.
    expect(buttons(host)[0].style.width).toBe('')
  })

  it('gives a joined spread twice the floor of a single page', () => {
    const spreadGroups: Group[] = [
      { slots: [0] },
      { slots: [2, 3], spread: true },
    ]
    const host = mount(
      <Filmstrip {...props({ groups: spreadGroups, currentIndex: 0 })} />,
    )
    const [single, spread] = buttons(host)
    expect(single.style.minWidth).toBe(
      'calc(var(--strip-unit) * 1 + var(--strip-tile-border) * 2)',
    )
    expect(spread.style.minWidth).toBe(
      'calc(var(--strip-unit) * 2 + var(--strip-tile-border) * 2)',
    )
  })
```

- [ ] **Step 4: Run the suite**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx --reporter=default
pnpm -C web run typecheck && pnpm -C web run lint
```

Expected: PASS, including every pre-existing case.

- [ ] **Step 5: Commit**

```bash
git add web/src/Filmstrip.tsx web/src/Filmstrip.test.tsx
git commit -m "O140: the nominal tile width is a floor, not a fixed size

Driving the app after the previous commit showed two things. box-sizing
is border-box, so a fixed width made every tile 4px narrower than it had
been; and a landscape page the sidecar never marked as a joined spread
letterboxed to about a quarter of its area, which is the one tile a
reader hunting a page most needs to recognise.

min-width instead, carrying the two borders explicitly. An unrendered
tile occupies exactly its nominal, so the window and the spacers still
compute; a loaded image draws at its own width as it did before. Measured
after: a portrait tile is 101px against 100.97 before, and the wide page
is back to 197.33."
```

**What this does not change:** the spacers still size in nominal widths, and
they only ever cover tiles that have not rendered — which are exactly the
tiles still at their nominal. `spacerWidth`'s arithmetic in Task 3 is
unaffected.

---

### Task 3: Render a window of tiles between two spacers

**Files:**
- Modify: `web/src/tiles.ts` — add `stripWindow` and `spacerWidth`
- Modify: `web/src/tiles.test.ts`
- Modify: `web/src/Filmstrip.tsx`
- Modify: `web/src/Filmstrip.test.tsx`
- Create: `web/src/Filmstrip.browser.test.tsx`

**Interfaces:**
- Consumes: `tileUnits` (Task 1); `--strip-unit`, `--strip-gap`, `--strip-tile-border` (Task 2).
- Produces:
  - `stripWindow(focus: number, count: number, margin: number): { lo: number; hi: number }` — a half-open range, clamped to `[0, count]`.
  - `spacerWidth(units: number, tiles: number): string` — a CSS `calc()`, or `'0px'` for no tiles.
  - The exported constant `STRIP_WINDOW_MARGIN` from `Filmstrip.tsx` is **not** created; the margin is a module-private constant in `Filmstrip.tsx`. Task 4 reads it from there.

- [ ] **Step 1: Write the failing tests for the two pure functions**

Append to `web/src/tiles.test.ts`:

```tsx
describe('stripWindow', () => {
  it('spans a margin either side of the focus', () => {
    expect(stripWindow(50, 237, 12)).toEqual({ lo: 38, hi: 63 })
  })

  it('clamps at the start rather than running negative', () => {
    expect(stripWindow(2, 237, 12)).toEqual({ lo: 0, hi: 15 })
  })

  it('clamps at the end rather than running past the last tile', () => {
    expect(stripWindow(235, 237, 12)).toEqual({ lo: 223, hi: 237 })
  })

  it('is the whole rail when the margin covers it', () => {
    expect(stripWindow(1, 4, 12)).toEqual({ lo: 0, hi: 4 })
  })

  // `currentIndex` is -1 in a gap and while correcting before a counterpart
  // is picked. A window built around -1 must not start below zero.
  it('treats a focus below zero as the first tile', () => {
    expect(stripWindow(-1, 237, 12)).toEqual({ lo: 0, hi: 13 })
  })

  it('is empty for an empty rail', () => {
    expect(stripWindow(0, 0, 12)).toEqual({ lo: 0, hi: 0 })
  })
})

describe('spacerWidth', () => {
  // A spacer replaces N tiles AND the gaps between them, but not the gap
  // either side of itself — the rail still lays one out, so N tiles
  // contribute N-1 internal gaps.
  //
  // The border term is `tiles * 2`, not `units * 2`: every tile has two
  // borders whatever its width. A tile's nominal OUTER width is therefore
  // `units * --strip-unit + 2 * --strip-tile-border`, which measures 101 px
  // at 1440x900 against the 100.97 px a loaded one-page tile occupied before
  // O140 touched anything. Measured on the running app: a 237-tile rail
  // whose tiles are all at their nominal width is
  // `237*101 + 236*6 + 16 = 25369`, and the live rail read 25465 — the 96 px
  // difference being the one tile on that volume whose image is wider than
  // its nominal and is allowed to exceed it (see `tileStyle`'s `minWidth`).
  it('sums the units, the borders and the internal gaps', () => {
    expect(spacerWidth(5, 4)).toBe(
      'calc(var(--strip-unit) * 5 + var(--strip-tile-border) * 8 + var(--strip-gap) * 3)',
    )
  })

  it('has no internal gap for a single tile', () => {
    expect(spacerWidth(2, 1)).toBe(
      'calc(var(--strip-unit) * 2 + var(--strip-tile-border) * 2 + var(--strip-gap) * 0)',
    )
  })

  it('is zero for no tiles, so a rail at either end renders no spacer', () => {
    expect(spacerWidth(0, 0)).toBe('0px')
  })
})
```

Add `spacerWidth, stripWindow` to the `./tiles` import.

- [ ] **Step 2: Run and watch them fail**

```bash
pnpm -C web exec vitest run --project node tiles.test.ts --reporter=default
```

Expected: FAIL — `stripWindow is not a function`.

- [ ] **Step 2a: Correct a comment the previous task falsified**

`web/src/tiles.ts`'s `tileUnits` doc still says "a not-yet-loaded tile lays
out at 4 px, which is why a cold strip's tiles sit collapsed until their
images arrive". Task 2a gave every tile a `minWidth` floor, so an unrendered
tile now occupies its full nominal width and nothing collapses.

Rewrite that sentence so it states why the function exists **without** the
falsified premise: a tile's width is otherwise whatever its thumbnail draws
at, so nothing can say where tile 150 begins without loading tiles 0-149.
Keep the rest of the doc, including the paragraph about why this is not
`tilePages(group, slots).length` — that part is untouched and is the reason
the function is not derivable.

This is the third site of the same mistake on this branch (the other two were
in `Filmstrip.tsx` and `Filmstrip.test.tsx`, fixed in `23729b6`). A grep for
the shape across `web/src` found no others.

- [ ] **Step 3: Implement the two pure functions**

Append to `web/src/tiles.ts`:

```ts
/**
 * The half-open range of tiles the rail renders, `margin` either side of
 * `focus`, clamped to the rail.
 *
 * `focus` below zero is treated as the first tile rather than rejected:
 * `currentIndex` is `-1` wherever the viewed edition has no slot for the
 * reader's position — a gap, or a borrowed page — and while correcting
 * before a counterpart is picked. `Filmstrip` resolves those states to a
 * real tile of its own (the paired or anchor tile) before calling this, but
 * clamping here as well means a caller that does not cannot produce a
 * negative `lo`.
 */
export function stripWindow(
  focus: number,
  count: number,
  margin: number,
): { lo: number; hi: number } {
  if (count <= 0) return { lo: 0, hi: 0 }
  const at = Math.max(0, Math.min(focus, count - 1))
  return {
    lo: Math.max(0, at - margin),
    hi: Math.min(count, at + margin + 1),
  }
}

/**
 * The CSS width of one spacer standing in for `tiles` tiles totalling
 * `units` nominal page-widths.
 *
 * Written as a `calc()` over the same three custom properties the rail and
 * its tiles are laid out with, rather than as a pixel number, so nothing
 * here measures a tile and a breakpoint change moves the spacers with the
 * tiles automatically.
 *
 * **The gap count is `tiles - 1`, not `tiles`.** A spacer replaces N tiles
 * and the N-1 gaps *between* them; the gaps on either side of the spacer
 * itself are the ones the rail still lays out around it, and counting them
 * here would widen the rail by one gap per spacer. The arithmetic that has
 * to hold: for a rail of N tiles split into K before, W rendered and M
 * after, the rendered row is 1 + W + 1 flex items with W + 1 gaps, and
 * `spacerWidth(K) + spacerWidth(M) + (W+1)·gap` equals the unwindowed
 * `Σwidths + (N-1)·gap` exactly when each spacer carries its own tiles'
 * widths plus its own internal gaps. A tile's width here is its nominal
 * outer width, borders included, because the button is
 * `box-sizing: border-box` — and a spacer only ever covers tiles that have
 * not rendered, which are exactly the ones still at their nominal. `Filmstrip.browser.test.tsx` measures
 * that equality rather than trusting this paragraph.
 */
export function spacerWidth(units: number, tiles: number): string {
  if (tiles <= 0) return '0px'
  return `calc(var(--strip-unit) * ${units} + var(--strip-tile-border) * ${tiles * 2} + var(--strip-gap) * ${tiles - 1})`
}
```

- [ ] **Step 4: Run and watch them pass**

```bash
pnpm -C web exec vitest run --project node tiles.test.ts --reporter=default
```

Expected: PASS, 9 new tests.

- [ ] **Step 5: Write the failing component tests**

Append to `web/src/Filmstrip.test.tsx` a new describe. It needs a rail long
enough to be windowed, which the file's four-slot fixture is not:

```tsx
describe('windowing', () => {
  // 120 ordinary pages — long enough that a 12-tile margin leaves most of
  // the rail unrendered.
  const longSlots: Position[] = Array.from({ length: 120 }, (_, i) => ({
    page: i,
  }))
  const longGroups: Group[] = longSlots.map((_, i) => ({ slots: [i] }))
  const longProps = (overrides: Partial<FilmstripProps> = {}) =>
    props({ slots: longSlots, groups: longGroups, ...overrides })

  const spacers = (host: HTMLElement) =>
    Array.from(host.querySelectorAll('[data-testid="strip-spacer"]'))

  it('renders a window of tiles rather than one per page', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    expect(buttons(host).length).toBeLessThan(40)
    expect(buttons(host).length).toBeGreaterThan(0)
  })

  it('always renders the current tile, and marks it', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    const current = host.querySelector('[aria-current="true"]')
    expect(current).not.toBeNull()
    expect(current?.querySelector('img')?.getAttribute('alt')).toBe('page 61')
  })

  it('stands a spacer either side of a window in the middle of the rail', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    expect(spacers(host)).toHaveLength(2)
  })

  it('has no leading spacer when the window starts at the first tile', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    expect(spacers(host)).toHaveLength(1)
  })

  it('has no trailing spacer when the window ends at the last tile', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 119 })} />)
    expect(spacers(host)).toHaveLength(1)
  })

  // The window follows whichever tile the centring chain would centre on —
  // the same `currentRef ?? pairedRef ?? anchorRef` order the component
  // already documents. A window built only around `currentIndex` would
  // leave the tile a correction is about unrendered, and `scrollIntoView`
  // with nothing to scroll to.
  it('is built around the resolved tile while correcting, not the current one', () => {
    const host = mount(
      <Filmstrip
        {...longProps({
          currentIndex: -1,
          correcting: { to: 'ed2', pairedIndex: 90 },
          onPickCounterpart: vi.fn(),
        })}
      />,
    )
    const dashed = Array.from(host.querySelectorAll('button')).find(
      (b) => b.style.borderStyle === 'dashed',
    )
    expect(dashed).toBeDefined()
    expect(dashed?.querySelector('img')?.getAttribute('alt')).toBe('page 91')
  })

  it('is built around the anchor where this edition has no slot at all', () => {
    const host = mount(
      <Filmstrip {...longProps({ currentIndex: -1, anchor: 75 })} />,
    )
    const alts = Array.from(host.querySelectorAll('img')).map((i) =>
      i.getAttribute('alt'),
    )
    expect(alts).toContain('page 76')
  })
})
```

- [ ] **Step 6: Run and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx --reporter=default
```

Expected: FAIL — the first case fails with `expected 120 to be less than 40`.

- [ ] **Step 7: Implement the window in `Filmstrip.tsx`**

Import the two new functions:

```ts
import { spacerWidth, stripWindow, tilePages, tileUnits } from './tiles'
```

Add the margin constant and the spacer style near the other module constants:

```ts
/**
 * How many tiles the rail builds either side of the one a reader is looking
 * at.
 *
 * Twelve rather than "enough to fill the viewport": a phone shows about five
 * tiles and a 1440 desktop about thirteen, so twelve either side covers both
 * with room to flick before the scroll handler has to extend it, while
 * leaving a 237-tile rail building about 25 tiles rather than 237. The
 * figure this trades against is O140's: 237 tiles cost 274 ms to build, 20
 * tiles 38 ms.
 */
const windowMargin = 12

// A spacer stands in for the tiles outside the window, at exactly their
// summed width, so every rendered tile keeps the position it would have had
// and `scrollIntoView` still lands on it. `aria-hidden`, and not a button:
// it is geometry, not a page, and a screen reader walking the strip should
// find only tiles.
const spacerStyle: CSSProperties = { flex: '0 0 auto', height: '100%' }
```

Inside the component, after the refs and before the effect, resolve the tile
the window is built around and derive the range:

```tsx
  // Which tile the window is centred on: the same `current ?? paired ??
  // anchor` order the centring effect below scrolls to, resolved to an index
  // rather than to a ref. Built here rather than inside the map so the two
  // cannot disagree — a window built around a different tile than the one
  // `recenter` scrolls to would scroll to a tile that was never rendered.
  const paired = correcting?.pairedIndex
  const pairedFocus =
    paired === undefined
      ? -1
      : groups.findIndex((g) => g.slots.includes(paired))
  const anchorFocus =
    anchor === undefined || currentIndex !== -1 || correcting
      ? -1
      : groups.findIndex((g) => g.slots.includes(anchor))
  // Named `primaryFocus` rather than `focus` because Task 4 adds a second
  // source — where the reader has scrolled to — and `focus` becomes the
  // choice between them. Naming it now means Task 4 adds a line rather than
  // renaming one.
  const primaryFocus =
    currentIndex >= 0
      ? currentIndex
      : pairedFocus >= 0
        ? pairedFocus
        : anchorFocus
  const { lo, hi } = stripWindow(primaryFocus, groups.length, windowMargin)

  // The summed units on either side, for the two spacers.
  const unitsIn = (from: number, to: number) =>
    groups.slice(from, to).reduce((sum, g) => sum + tileUnits(g), 0)
```

Replace `groups.map((group, index) => {` with a windowed slice, keeping the
absolute index:

```tsx
      {lo > 0 && (
        <div
          data-testid="strip-spacer"
          aria-hidden="true"
          style={{ ...spacerStyle, width: spacerWidth(unitsIn(0, lo), lo) }}
        />
      )}
      {groups.slice(lo, hi).map((group, offset) => {
        const index = lo + offset
```

…and after the closing `})}` of the map, the trailing spacer:

```tsx
      {hi < groups.length && (
        <div
          data-testid="strip-spacer"
          aria-hidden="true"
          style={{
            ...spacerStyle,
            width: spacerWidth(
              unitsIn(hi, groups.length),
              groups.length - hi,
            ),
          }}
        />
      )}
```

Everything inside the map body is unchanged — `isCurrent`, `isResolved`,
`isAnchor`, the ref chain, `aria-current`, the handlers and `tilePages`.

- [ ] **Step 8: Run and watch them pass**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx --reporter=default
```

Expected: PASS, including every pre-existing case.

- [ ] **Step 9: Write the browser test that measures the geometry**

Create `web/src/Filmstrip.browser.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { page } from '@vitest/browser/context'
// **Required.** `--strip-unit` and friends live in `styles/reader.css`, which
// reaches the app through `index.css`. Without this import the custom
// properties do not resolve, every `calc()` that reads them is invalid, and
// every tile collapses — on BOTH sides of any comparison below, so the
// assertions would pass while measuring nothing. Every other
// `*.browser.test.tsx` in this directory imports it for the same reason.
import './index.css'
import { Filmstrip, type FilmstripProps } from './Filmstrip'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { mount } from './testing/mount'

/**
 * What jsdom cannot answer: whether a windowed rail is the same *shape* as an
 * unwindowed one. Every assertion here is a measured box.
 *
 * The invariant is deliberately "the windowed rail is as wide as its tiles'
 * nominal widths say it should be", not "as wide as it was before O140" —
 * giving a tile a width moved that on its own, before anything was windowed.
 *
 * Nothing here loads a thumbnail: `thumbUrl` points at `/api/...`, which no
 * server answers in this environment, so every tile sits at exactly its
 * nominal floor. That is what makes the closed-form arithmetic below exact.
 * It is also why these tests cannot speak to a rail whose images have loaded,
 * where a tile may exceed its floor — the spacers only ever cover tiles that
 * have not rendered, which are the ones still at nominal.
 */
const slots: Position[] = Array.from({ length: 120 }, (_, i) => ({ page: i }))
const groups: Group[] = slots.map((_, i) => ({ slots: [i] }))

function props(overrides: Partial<FilmstripProps> = {}): FilmstripProps {
  return {
    reading: 'ltr',
    work: 'w',
    edition: 'e',
    slots,
    groups,
    currentIndex: 60,
    onNavigate: () => {},
    ...overrides,
  }
}

/** The rail needs the reader's custom properties, which live on `.reader`. */
function inReader(ui: React.ReactNode) {
  return mount(
    <div className="reader" style={{ position: 'fixed', inset: 0 }}>
      <div style={{ height: 'var(--strip-height)' }}>{ui}</div>
    </div>,
  )
}

const railOf = (host: HTMLElement) =>
  host.querySelector('.reader > div > div') as HTMLElement

/** The rail's geometry, read from the stylesheet rather than hardcoded. */
function geometry(rail: HTMLElement) {
  const cs = getComputedStyle(rail)
  const num = (v: string) => Number.parseFloat(v)
  const unit = num(cs.getPropertyValue('--strip-unit'))
  const border = num(cs.getPropertyValue('--strip-tile-border'))
  return {
    unit,
    border,
    gap: num(cs.columnGap),
    padding: num(cs.paddingLeft) + num(cs.paddingRight),
    nominal: unit + 2 * border,
  }
}

describe('a windowed rail has the shape of an unwindowed one', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`is as wide as every tile's nominal width says, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader(<Filmstrip {...props({ reading })} />)
      const rail = railOf(host)
      const g = geometry(rail)

      // THE GUARD. If `index.css` were not imported, or the custom property
      // renamed, `--strip-unit` resolves to nothing, every tile collapses,
      // and every arithmetic assertion below would still agree with itself.
      // This is the line that fails instead.
      expect(g.unit).toBeGreaterThan(0)
      expect(g.nominal).toBeGreaterThan(50)
      expect(rail.querySelector('button')?.getBoundingClientRect().width).toBeCloseTo(
        g.nominal,
        0,
      )

      // 120 one-unit tiles, whatever fraction of them is actually rendered.
      expect(rail.scrollWidth).toBeCloseTo(
        120 * g.nominal + 119 * g.gap + g.padding,
        0,
      )
    })
  }

  it('renders far fewer tiles than the rail holds', async () => {
    await page.viewport(1440, 900)
    const host = inReader(<Filmstrip {...props()} />)
    expect(host.querySelectorAll('button').length).toBeLessThan(40)
    expect(host.querySelectorAll('button').length).toBeGreaterThan(0)
  })

  it('puts the current tile where an unwindowed rail would put it', async () => {
    await page.viewport(1440, 900)
    const host = inReader(<Filmstrip {...props({ currentIndex: 60 })} />)
    const rail = railOf(host)
    const g = geometry(rail)
    const current = host.querySelector('[aria-current="true"]') as HTMLElement
    expect(current).not.toBeNull()

    // Tile 60 starts 60 tiles and 60 gaps in from the rail's content edge.
    const offset =
      current.getBoundingClientRect().left -
      rail.getBoundingClientRect().left +
      rail.scrollLeft
    expect(offset).toBeCloseTo(
      Number.parseFloat(getComputedStyle(rail).paddingLeft) +
        60 * (g.nominal + g.gap),
      0,
    )
  })
})
```

- [ ] **Step 10: Run the browser test**

```bash
pnpm -C web exec vitest run --project browser Filmstrip.browser.test.tsx --reporter=default
```

Expected: PASS, 4 cases. If Chromium is missing:
`pnpm -C web exec playwright install chromium`.

- [ ] **Step 11: Hand-apply the defect and watch a test fail**

The spacer arithmetic is the part of this task that renders identically
whether it is right or wrong — a rail one gap too wide per spacer looks
exactly like a rail that is correct. So apply the defect the tests name and
confirm one of them catches it.

In `tiles.ts`, change `spacerWidth`'s gap count from `${tiles - 1}` to
`${tiles}` and re-run:

```bash
pnpm -C web exec vitest run --project browser Filmstrip.browser.test.tsx --reporter=default
```

Expected: FAIL on `scrolls the same total width` for both `ltr` and `rtl`.
**Restore the `- 1` and confirm green before committing.** If the run passes
with the defect in place, the test is not reaching it and the test is what
needs fixing.

- [ ] **Step 12: Commit**

```bash
git add web/src/tiles.ts web/src/tiles.test.ts web/src/Filmstrip.tsx web/src/Filmstrip.test.tsx web/src/Filmstrip.browser.test.tsx
git commit -m "O140: the rail renders a window, not every tile

stripWindow gives the range around whichever tile the centring chain
would centre on — current, else paired, else anchor, resolved to an index
so the window and the scroll cannot disagree — and two spacers stand over
the rest at exactly their summed width. A 237-tile rail now builds about
25 tiles.

The spacer's gap count is tiles-1, not tiles: it replaces the gaps
between the tiles it covers, not the ones the rail lays out around it.
Applied that off-by-one by hand and watched the browser test fail on it."
```

---

### Task 4: Extend the window as the reader scrolls

**Files:**
- Modify: `web/src/tiles.ts` — add `focusAtFraction`
- Modify: `web/src/tiles.test.ts`
- Modify: `web/src/Filmstrip.tsx`
- Modify: `web/src/Filmstrip.test.tsx`

**Interfaces:**
- Consumes: `stripWindow` (Task 3).
- Produces: `focusAtFraction(units: number[], fraction: number): number`.

- [ ] **Step 1: Write the failing test**

Append to `web/src/tiles.test.ts`:

```tsx
describe('focusAtFraction', () => {
  // Four tiles of one unit each: the rail is four units long, so each tile
  // owns a quarter of it.
  const even = [1, 1, 1, 1]

  it('is the first tile at the start of the rail', () => {
    expect(focusAtFraction(even, 0)).toBe(0)
  })

  it('is the last tile at the end of the rail', () => {
    expect(focusAtFraction(even, 1)).toBe(3)
  })

  it('is the tile whose span contains the fraction', () => {
    expect(focusAtFraction(even, 0.3)).toBe(1)
    expect(focusAtFraction(even, 0.6)).toBe(2)
  })

  // A wide tile owns proportionally more of the rail, which is the whole
  // reason this reads units rather than counting tiles.
  it('gives a two-unit tile twice the span of a one-unit tile', () => {
    const mixed = [1, 2, 1] // spans 0-0.25, 0.25-0.75, 0.75-1
    expect(focusAtFraction(mixed, 0.1)).toBe(0)
    expect(focusAtFraction(mixed, 0.5)).toBe(1)
    expect(focusAtFraction(mixed, 0.9)).toBe(2)
  })

  it('clamps a fraction outside the rail rather than running off it', () => {
    expect(focusAtFraction(even, -0.5)).toBe(0)
    expect(focusAtFraction(even, 1.5)).toBe(3)
  })

  it('is zero for an empty rail', () => {
    expect(focusAtFraction([], 0.5)).toBe(0)
  })
})
```

Add `focusAtFraction` to the `./tiles` import.

- [ ] **Step 2: Run and watch it fail**

```bash
pnpm -C web exec vitest run --project node tiles.test.ts --reporter=default
```

Expected: FAIL — `focusAtFraction is not a function`.

- [ ] **Step 3: Implement**

Append to `web/src/tiles.ts`:

```ts
/**
 * Which tile sits at `fraction` of the way along a rail whose tiles are
 * `units` wide.
 *
 * Takes a fraction rather than a pixel offset deliberately: the rail's
 * pixel geometry lives in CSS (`--strip-unit` and friends), and a caller
 * that had to convert a `scrollLeft` into an index would need those numbers
 * in JavaScript, which is exactly the measurement this design avoids.
 * `scrollLeft / (scrollWidth - clientWidth)` is a fraction the browser
 * computes, and it is the same fraction under `row-reverse` once its sign
 * is taken off — which is why `Filmstrip` passes `Math.abs`.
 *
 * The borders and gaps are not modelled here. They make each tile's true
 * span a little wider than its unit share, so the answer can be off by one
 * tile near a boundary. That is harmless: the caller uses this to choose a
 * window with a twelve-tile margin, not to scroll to anything.
 */
export function focusAtFraction(units: number[], fraction: number): number {
  if (units.length === 0) return 0
  const total = units.reduce((sum, u) => sum + u, 0)
  const target = Math.max(0, Math.min(1, fraction)) * total
  let seen = 0
  for (let i = 0; i < units.length; i++) {
    seen += units[i]
    if (target < seen) return i
  }
  return units.length - 1
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
pnpm -C web exec vitest run --project node tiles.test.ts --reporter=default
```

Expected: PASS, 6 new tests.

- [ ] **Step 5: Write the failing component test**

Append to `describe('windowing', ...)` in `web/src/Filmstrip.test.tsx`:

```tsx
  // jsdom reports every box as zero, so a real scroll cannot be simulated.
  // What can be: that the rail listens, and that a scroll to the far end
  // moves the window there. `scrollWidth`/`clientWidth` are defined on the
  // element directly for this one test.
  it('extends the window to where the reader scrolls', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    const rail = host.firstElementChild as HTMLElement
    expect(
      Array.from(host.querySelectorAll('img')).map((i) => i.alt),
    ).not.toContain('page 100')

    Object.defineProperty(rail, 'scrollWidth', { value: 1200, configurable: true })
    Object.defineProperty(rail, 'clientWidth', { value: 200, configurable: true })
    rail.scrollLeft = 1000 // the far end
    act(() => {
      rail.dispatchEvent(new Event('scroll'))
    })

    expect(
      Array.from(host.querySelectorAll('img')).map((i) => i.alt),
    ).toContain('page 100')
  })
```

- [ ] **Step 6: Run and watch it fail**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx -t "extends the window" --reporter=default
```

Expected: FAIL — `expected [ 'page 1', … ] to contain 'page 100'`.

- [ ] **Step 7: Implement the scroll handler**

In `Filmstrip.tsx`, import `focusAtFraction` and `useState`:

```ts
import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { focusAtFraction, spacerWidth, stripWindow, tilePages, tileUnits } from './tiles'
```

Add the state and the handler, beside the focus derivation from Task 3:

```tsx
  // Where the reader has scrolled to, as a tile index, or `undefined` while
  // they have not scrolled away from the tile the rail centred on.
  //
  // Cleared whenever the centring effect below runs, so a page turn or a new
  // correction always takes the window back to the tile that state is about
  // rather than leaving it wherever a previous flick left it.
  const [scrolledTo, setScrolledTo] = useState<number>()

  const units = groups.map(tileUnits)
  const focus = scrolledTo ?? primaryFocus
```

Task 3 already named its derivation `primaryFocus`, so nothing is renamed
here. Change the `stripWindow` call to read the combined `focus`:

```tsx
  const { lo, hi } = stripWindow(focus, groups.length, windowMargin)
```

Add the scroll listener to the existing effect, and clear `scrolledTo` there:

```tsx
  // biome-ignore lint/correctness/useExhaustiveDependencies: `currentIndex`, `visible`, `correcting?.pairedIndex` and `anchor` are triggers, not closure reads — the body reaches the tile through refs. Removing them, which is what this rule suggests, would centre the rail once and never re-centre it.
  useEffect(() => {
    setScrolledTo(undefined)
    const recenter = () =>
      (
        currentRef.current ??
        pairedRef.current ??
        anchorRef.current
      )?.scrollIntoView({ inline: 'center', block: 'nearest' })
    recenter()
    const rail = railRef.current
    if (!rail) return
    rail.addEventListener('load', recenter, true)
    return () => rail.removeEventListener('load', recenter, true)
  }, [currentIndex, visible, correcting?.pairedIndex, anchor])
```

And a second effect for the scroll, rAF-throttled:

```tsx
  // Extending the window as the reader flicks along the rail. Throttled to
  // one frame: a scroll fires far more often than a window needs rebuilding,
  // and rebuilding it per event would put the very cost this windowing
  // exists to remove back on the reader's finger.
  //
  // The fraction, rather than a pixel offset, is what `focusAtFraction`
  // takes — see its doc. `Math.abs` is what makes `row-reverse` work:
  // scrolling a reversed flex row moves `scrollLeft` negative in this
  // engine, and the distance along the rail is the same either way.
  useEffect(() => {
    const rail = railRef.current
    if (!rail) return
    let frame = 0
    const onScroll = () => {
      if (frame) return
      frame = requestAnimationFrame(() => {
        frame = 0
        const span = rail.scrollWidth - rail.clientWidth
        const fraction = span <= 0 ? 0 : Math.abs(rail.scrollLeft) / span
        setScrolledTo(focusAtFraction(units, fraction))
      })
    }
    rail.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      rail.removeEventListener('scroll', onScroll)
      if (frame) cancelAnimationFrame(frame)
    }
    // `units` is rebuilt every render; depending on it would re-attach the
    // listener every render. Its *content* only changes when `groups` does.
  }, [groups])
```

Note the test dispatches `scroll` synchronously, so the rAF must resolve —
add `vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => { cb(0); return 0 })`
to the windowing describe's `beforeEach`, and restore it in an `afterEach`.

- [ ] **Step 7a: Extend the window on focus, not only on scroll**

**Why this is here.** Task 3's review found that a keyboard or screen-reader
user now reaches 25 tiles rather than 237, and `Tab` past the last rendered
one leaves the rail entirely — there is no next tile to focus, because it was
never built. A scroll handler does not help: a keyboard user never scrolls.

The fix is the same state the scroll handler already sets. Focus is reported
by a `focusin` listener on the rail (it bubbles, unlike `focus`), and a tile
that takes focus names the index the window should centre on — so by the time
the reader tabs to the edge of the window, the next tiles are already built.

Add to the scroll effect, listening on the same rail:

```tsx
    const onFocusIn = (event: FocusEvent) => {
      const tile = (event.target as HTMLElement | null)?.closest('button')
      if (!tile || !rail.contains(tile)) return
      const rendered = [...rail.querySelectorAll('button')]
      const at = rendered.indexOf(tile as HTMLButtonElement)
      if (at >= 0) setScrolledTo(lo + at)
    }
    rail.addEventListener('focusin', onFocusIn)
```

remembering to remove it in the same cleanup, and to add `lo` to the effect's
dependencies (or read it through a ref, if adding it re-attaches the listener
too often — say which you chose and why in the report).

`lo + at` rather than `at`: `rendered` holds only the windowed slice, so its
indices are slice-relative and `lo` is what makes them absolute. That is the
same off-by-`lo` the review checked the tile map for.

- [ ] **Step 7b: Test it**

```tsx
  it('extends the window to a tile that takes focus, for a reader using the keyboard', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    const rail = host.firstElementChild as HTMLElement
    const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
    expect(before).not.toContain('page 30')

    // The last tile the window built takes focus, as it would under `Tab`.
    const tiles = [...rail.querySelectorAll('button')]
    const last = tiles[tiles.length - 1]
    act(() => {
      last.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })

    expect(
      Array.from(host.querySelectorAll('img')).map((i) => i.alt),
    ).toContain('page 30')
  })
```

Run it, watch it fail before Step 7a's listener exists, then pass after.

- [ ] **Step 8: Run and watch it pass**

```bash
pnpm -C web exec vitest run --project jsdom Filmstrip.test.tsx --reporter=default
```

Expected: PASS.

- [ ] **Step 9: Drive it in a real browser**

Serve the app, open the chrome on the 237-page volume at 390×844, and flick
the strip from one end to the other. Confirm: tiles appear as you reach them,
the rail never jumps or resizes under the finger, and the far end is
reachable. Do it in an `rtl` work — the project's own record has an `rtl`
fixture passing a defect that `ltr` caught (O65).

**And the keyboard route, which is the one Step 7a exists for:** put focus on
a tile and hold `Tab`. Focus must keep finding tiles rather than falling out
of the rail after the window's last one.

- [ ] **Step 10: Commit**

```bash
git add web/src/tiles.ts web/src/tiles.test.ts web/src/Filmstrip.tsx web/src/Filmstrip.test.tsx
git commit -m "O140: the window follows the reader along the rail

A scroll sets the window's focus from the fraction of the rail scrolled,
rAF-throttled, so flicking to the far end builds the tiles there. The
fraction rather than a pixel offset keeps the rail's geometry in CSS, and
Math.abs is what makes row-reverse work. The centring effect clears the
scrolled focus, so a page turn takes the window back to the page."
```

---

### Task 5: The chrome mounts once and hides in place

**Files:**
- Modify: `web/src/Chrome.tsx`
- Test: `web/src/Chrome.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks. Independent of 1–4, but sequenced after them so the freeze in Task 6 protects a ~25-tile subtree rather than a 237-tile one.
- Produces: the container `div.reader-chrome`, which Task 6 puts the frozen tree inside.

- [ ] **Step 1: Write the failing tests**

Replace the existing case at `web/src/Chrome.test.tsx:71` and add two:

```tsx
  it('renders nothing before it has ever been shown', () => {
    const host = mount(<Chrome {...baseProps} visible={false} />)
    expect(host.children.length).toBe(0)
  })

  // The protection this preserves, and the only reason the unmount existed:
  // `loading="lazy"` was measured on this project not to defer every
  // thumbnail request (O140 design §1.1), so a chrome mounted before a
  // reader asks for it would fetch about a hundred thumbnails they never
  // asked to see.
  it('requests no thumbnail before it has ever been shown', () => {
    const host = mount(<Chrome {...baseProps} visible={false} />)
    expect(host.querySelectorAll('img')).toHaveLength(0)
  })

  it('keeps its DOM after a hide, rather than unmounting it', () => {
    const host = mount(<Chrome {...baseProps} visible={true} />)
    const bar = bottomBar(host)
    expect(bar).not.toBeNull()

    rerender(<Chrome {...baseProps} visible={false} />)
    expect(bottomBar(host)).toBe(bar) // the same node, not a new one
  })

  it('takes the hidden chrome out of layout, not merely out of sight', () => {
    const host = mount(<Chrome {...baseProps} visible={true} />)
    rerender(<Chrome {...baseProps} visible={false} />)
    const chrome = host.querySelector('.reader-chrome') as HTMLElement
    expect(chrome.style.display).toBe('none')
  })
```

- [ ] **Step 2: Run and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom Chrome.test.tsx --reporter=default
```

Expected: FAIL — `keeps its DOM after a hide` fails with `expected null to be <div>`.

- [ ] **Step 3: Implement**

In `web/src/Chrome.tsx`, replace `if (!visible) return null` with a
mount-once gate, and wrap the returned fragment in a container.

Add beside the other state:

```tsx
  // Whether the chrome has ever been asked for. Until it has, this component
  // renders nothing at all — which is what keeps a reader who never presses
  // `Tab` from fetching a single filmstrip thumbnail. That protection is the
  // whole reason this used to `return null` whenever hidden, and O140's
  // measurement made it *more* important rather than less: `loading="lazy"`
  // was found to defer only some of a long rail's requests (101 of 237 at
  // 390×844), so the mount, not the attribute, is what bounds the burst.
  //
  // A ref rather than state: flipping it must not itself cause a render.
  //
  // Written from an **effect**, not during render. A render-phase ref write
  // runs even for a render React discards, and this one does not need to
  // happen during render anyway: on the first *visible* render the guard
  // below passes on `visible` alone, and the ref only has to be true by the
  // time `visible` goes false again — which is after this effect has run.
  const shownOnce = useRef(false)
  useEffect(() => {
    if (visible) shownOnce.current = true
  }, [visible])
```

Replace the early return:

```tsx
  if (!visible && !shownOnce.current) return null
```

And wrap the returned fragment. The old `return ( <> … </> )` becomes:

```tsx
  return (
    <div
      className="reader-chrome"
      // `display: none` rather than `visibility: hidden`, although the
      // latter re-shows in 4 ms against 15 (O140 design §1): a
      // `visibility: hidden` rail is still laid out, so its several
      // thousand pixels of tiles reflow on every page turn — the cost this
      // whole change exists to remove. 11 ms on a re-show is worth paying to
      // keep the hidden chrome out of layout entirely.
      style={visible ? undefined : { display: 'none' }}
    >
      … the existing fragment's children, unchanged …
    </div>
  )
```

**The container must not disturb the two bars' own positioning.** Both are
`position: absolute` against `App.tsx`'s root, so a plain `div` between them
and that root changes nothing about where they dock — but confirm it in Step
5 rather than assuming it.

- [ ] **Step 4: Run and watch them pass**

```bash
pnpm -C web exec vitest run --project jsdom Chrome.test.tsx --reporter=default
```

Expected: PASS, including every pre-existing case in the file.

- [ ] **Step 5: Run the whole jsdom and browser suites**

```bash
pnpm -C web exec vitest run --project jsdom --reporter=default
pnpm -C web exec vitest run --project browser --reporter=default
```

Expected: PASS. This is the step that finds any test which toggled the chrome
off and then asserted absence — the design's §7 says the sweep for those is
part of the work, not a list it has already completed. Convert each one it
finds to an assertion about hiddenness, and note it in the commit.

- [ ] **Step 6: Drive it in a real browser**

Serve the app and check the two things jsdom cannot see:

1. **The bars still dock where they did** at 1440×900 and 390×844 — the top
   bar at the top edge, the bottom bar at the bottom, panels clear of both.
2. **The fade still plays on showing.** `scheduleFade` schedules a frame so
   the first paint lands at `opacity: 0`, which is also what a `display`
   change needs to animate from, but that is an expectation and not a
   verification until it is watched.

- [ ] **Step 7: Commit**

```bash
git add web/src/Chrome.tsx web/src/Chrome.test.tsx
git commit -m "O140: the chrome is built once and then hidden in place

It still renders nothing until the first time it is shown, so a reader who
never presses Tab still fetches no thumbnail — O140 found loading=lazy
defers only some of a long rail's requests, so the mount is what bounds
that burst. After the first show it stays in the DOM behind display:none,
which unlike visibility:hidden takes the rail out of layout."
```

---

### Task 6: Freeze the chrome while it is hidden

**Files:**
- Modify: `web/src/Chrome.tsx`
- Test: `web/src/Chrome.test.tsx`

**Interfaces:**
- Consumes: the `div.reader-chrome` container from Task 5.
- Produces: nothing other tasks read.

- [ ] **Step 1: Write the failing test**

This is the test the design calls out as the one that must fail when the
freeze is removed. It asserts a *behaviour* — that a hidden chrome does not
take in new readout text — because a frozen subtree and a live one look
identical in the DOM until something changes underneath them.

Append to `web/src/Chrome.test.tsx`:

```tsx
  // Without the freeze, React reconciles all of the chrome's elements on
  // every render behind a hidden bar — measured at 165 ms a page turn
  // against 37 ms (O140 design §2), on the most common interaction in the
  // app. `display: none` does not prevent a re-render; only not building a
  // new element tree does.
  //
  // This asserts the freeze through its one visible consequence: a hidden
  // chrome keeps the readout it was hidden with, and takes the new one on
  // the way back up.
  it('does not re-render behind a hidden bar', () => {
    const host = mount(
      <Chrome {...baseProps} visible={true} position="slot 1 / 3" />,
    )
    expect(host.querySelector('[role="status"]')?.textContent).toContain(
      'slot 1 / 3',
    )

    rerender(<Chrome {...baseProps} visible={false} position="slot 1 / 3" />)
    // A page turn behind the hidden chrome.
    rerender(<Chrome {...baseProps} visible={false} position="slot 2 / 3" />)
    expect(host.querySelector('[role="status"]')?.textContent).toContain(
      'slot 1 / 3',
    )

    // And it catches up the moment it is asked for.
    rerender(<Chrome {...baseProps} visible={true} position="slot 2 / 3" />)
    expect(host.querySelector('[role="status"]')?.textContent).toContain(
      'slot 2 / 3',
    )
  })

  it('re-centres the rail on the page turned to while it was hidden', () => {
    const host = mount(<Chrome {...baseProps} visible={true} currentIndex={0} />)
    rerender(<Chrome {...baseProps} visible={false} currentIndex={0} />)
    rerender(<Chrome {...baseProps} visible={true} currentIndex={2} />)
    expect(host.querySelector('[aria-current="true"]')).toBe(
      host.querySelectorAll('.reader-bottom-bar button')[2],
    )
  })
```

- [ ] **Step 2: Run and watch it fail**

```bash
pnpm -C web exec vitest run --project jsdom Chrome.test.tsx -t "does not re-render" --reporter=default
```

Expected: FAIL — the readout reads `slot 2 / 3` while hidden, because
nothing is freezing it yet.

- [ ] **Step 3: Implement the freeze**

In `Chrome.tsx`, import `ReactElement` and hold the last tree:

```ts
import { useEffect, useRef, useState, type CSSProperties, type ReactElement } from 'react'
```

Add beside `hasShown`:

```tsx
  // The element tree this component last built while it was shown.
  //
  // While hidden it renders *this same object* again rather than a new one.
  // React compares elements by reference and skips reconciling a subtree
  // whose element has not changed, so a page turn behind a hidden chrome
  // costs nothing at all — without this it costs 165 ms against a closed
  // chrome's 37 ms (O140 design §2), because `display: none` stops the
  // painting and not the re-rendering.
  //
  // Written from an **effect**, so nothing is assigned during render — the
  // effect runs after commit, so this only ever holds a tree React actually
  // put on screen.
  //
  // What goes stale while frozen: the props the frozen tree closed over,
  // including `onNavigate` and `onPickCounterpart`. None is reachable — the
  // tree is inside `display: none`, so nothing can be clicked, hovered or
  // focused — and it is rebuilt from live props the moment `visible` goes
  // true again.
  const frozen = useRef<ReactElement>(null)
  useEffect(() => {
    if (live) frozen.current = live
  })
```

Then build the tree only while visible, and render the frozen one otherwise.
**`shownOnce` from Task 5 goes away here**: `frozen.current` is non-null for
exactly the same reason it was true — the chrome has been shown at least
once — so keeping both would be two flags for one fact.

```tsx
  // Everything from `const fade = …` down to the end of the old fragment
  // moves inside this conditional. `live` is a new element every render
  // while visible, which is what makes the chrome update while it is up.
  const live = visible ? buildLive() : undefined

  if (!visible && !frozen.current) return null

  return (
    <div
      className="reader-chrome"
      style={visible ? undefined : { display: 'none' }}
    >
      {visible ? live : frozen.current}
    </div>
  )
```

where `buildLive()` is the existing body, inlined rather than extracted if
that reads better:

```tsx
  const fade: CSSProperties = fadeStyle(motion, shown)
  const row = readoutRow({ … })
  return (
    <>
      … the existing fragment's children, unchanged …
    </>
  )
```

Note that `visible` is still passed to `Filmstrip` inside the frozen tree, so
while hidden it holds the `true` it was frozen with. That is deliberate and
harmless: the rail's centring effect is keyed on `currentIndex` as well, and
Step 1's second test is what pins that re-showing after a page turn still
re-centres.

- [ ] **Step 4: Run and watch them pass**

```bash
pnpm -C web exec vitest run --project jsdom Chrome.test.tsx --reporter=default
```

Expected: PASS.

- [ ] **Step 5: Hand-apply the defect and watch the test fail**

Remove the `visible ?` guard from `live` so the tree is rebuilt every render
(`const live = buildLive()`) and render `{live}` unconditionally — the exact
defect the test names — and re-run:

```bash
pnpm -C web exec vitest run --project jsdom Chrome.test.tsx -t "does not re-render" --reporter=default
```

Expected: FAIL. **Restore both guards and confirm green before committing.** A
freeze that silently thaws looks identical on screen, so a test that passes
with the guard removed is proving nothing.

- [ ] **Step 6: Commit**

```bash
git add web/src/Chrome.tsx web/src/Chrome.test.tsx
git commit -m "O140: a hidden chrome stops re-rendering

display:none stops the painting, not the re-rendering — a mounted chrome
still cost 165 ms on a page turn against a closed one's 37 ms. While
hidden the component now renders the same element object it last built, so
React skips the subtree entirely, and rebuilds from live props the moment
it is shown again. Removed the guard by hand and watched the test fail."
```

---

### Task 7: Re-measure, and close the row

**Files:**
- Modify: `docs/notes.md` (append a section)
- Modify: `docs/status.md` (O140's row and the sweep paragraph)
- Modify: `web/src/Filmstrip.tsx` or `docs/status.md` as the numbers require

**Interfaces:**
- Consumes: everything above.

- [ ] **Step 1: Run the full gate**

```bash
NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 ./scripts/check-all.sh
```

Expected: the final `all green` line. Quote it verbatim when reporting —
piping this through `tail` or appending `; echo` masks a failure.

- [ ] **Step 2: Re-take all five numbers**

Under the spec's §0 conditions exactly — `390x844x1`, 4× CPU throttle, the
237-page `akame-ga-kill----v01--2015--cbz` on the scratch library, warm
thumbnails — measure each of these and record both the before and the after:

| # | Measurement | Before |
| --- | --- | --- |
| 1 | First open of the chrome | 274 ms |
| 2 | A later open | 274 ms |
| 3 | A page turn, chrome closed | 37 ms |
| 4 | A page turn, chrome open | 178 ms |
| 5 | Thumbnail requests on a first open | 101 |

Medians of at least five, with the spread. **For the page-turn figures,
assert that the page actually moved** — the readout lives inside the chrome,
so a probe that reads it measures nothing while the chrome is closed. Read
the page `<img>`'s `src` instead, and report how many of N turns moved.

- [ ] **Step 3: Check the two things that are not numbers**

1. **The fade still plays on showing**, watched in a real browser at both
   sizes.
2. **A reader who never presses `Tab` requests zero thumbnails** — load the
   reader, turn several pages without opening the chrome, and confirm
   `performance.getEntriesByType('resource').filter(e => e.name.includes('/thumb'))`
   is empty.

- [ ] **Step 4: Write the measurements into `docs/notes.md`**

A new section, in the file's own style: what was measured, on what, under
which conditions, what the figures were before and after, and any figure
that moved less than the design predicted. The design's §5 says explicitly
that if the 178 ms open-chrome turn has not fallen as far as the ~50 ms
guess, **that is a new row, not a reason to hold the PR** — open it rather
than quietly restating the guess.

- [ ] **Step 5: Close O140 in `docs/status.md`**

The five-part edit the project's ritual requires: the row's checkbox, the
row's text rewritten to say what closed it and on what evidence, the
open/closed counts, the sweep paragraph's running narrative, and any other
row whose text this contradicts.

**Two corrections belong in this edit and are not optional:**

1. The `loading="lazy"` claim under "What M5b did not reach" — it says the
   attribute "does not defer thumbnail requests at all in the environment
   tested". Measured at 390×844 on a 237-tile rail, 101 of 237 were
   requested. Rewrite it to state which rail length and viewport each figure
   belongs to, per design §1.1.
2. O134's row says the strip's tiles are "stretched 1.8× on a 3× screen".
   That is about `devicePixelRatio` 3 and this profile ran at 1, so it is
   untouched — **do not change it**, and do not cite this profile as
   evidence either way. Noted here only so the next reader does not have to
   re-derive that it is out of scope.

- [ ] **Step 6: Check every factual claim before saying it is ready**

The project's rule, and it has caught a wrong sentence on most PRs it has
been applied to: read the row, the notes section and the PR body as prose
and check each factual claim against what was actually run. Do this as its
own pass, separately from writing them. Prefer deleting a figure to
correcting it a fourth time.

- [ ] **Step 7: Commit and open the PR**

```bash
git add docs/notes.md docs/status.md
git commit -m "O140: measured after, and closed

<the five numbers, before and after>"
```

Open the PR against `main` through the **gitea** MCP tools — `gh` on this
machine is authenticated against the wrong host. Do not hard-wrap the PR
body.

---

## Self-Review

**Spec coverage.** §1 and §2 are the measurements, carried into Task 7's
re-measurement table. §3's `tileUnits`, explicit widths, spacers, window and
scroll handler are Tasks 1–4. §4's mount-once, `display: none` and freeze are
Tasks 5–6, with the fade left as a browser check in both. §5's "what this
does not fix" is carried into Task 7 Step 4 as an instruction to open a row
rather than restate a guess. §6's seven re-measurements are Task 7 Steps 2
and 3. §7's test list is distributed across the tasks that introduce each
behaviour, with the `tileUnits` unit tests in Task 1 and the sweep for
toggle-then-assert-absence tests in Task 5 Step 5. §8's build order is the
task order.

**Placeholder scan.** One deliberate blank: Task 2 Step 2's `--strip-unit`
value at the desktop breakpoint, which Step 1 measures and Step 2 says to
write in. That is a measurement, not a placeholder — inventing the number
here is exactly what the project's "measured, not recalled" rule forbids.
The 54 px phone figure is already measured and carried through.

**Type consistency.** `tileUnits(group: Group): 1 | 2`, `stripWindow(focus,
count, margin): { lo, hi }`, `spacerWidth(units, tiles): string` and
`focusAtFraction(units: number[], fraction): number` are used with those
exact signatures in every task that consumes them. `primaryFocus` is named
that way in Task 3, where it is introduced, so that Task 4 adds the scrolled
focus beside it instead of renaming anything — a first draft of this plan had
the rename, which is the shape of bug this check exists to catch.

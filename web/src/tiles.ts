import type { Group } from './pairing'
import type { Position } from './resolver'

/** One tile's worth of pages, in reading order within the tile. */
export interface Tile {
  /** The slot each entry navigates to, in `groups`' order. */
  slot: number
  page: number
  /**
   * Which half of a joined spread's shared page this entry is, when it is
   * one. Only ever set for a genuine one-slot group — never for the
   * two-slot `spread: true` group `groups()` itself produces, which stands
   * for *both* halves as a single wide tile and has no one half to name.
   *
   * This is what lets two singleton tiles that share a page — a joined
   * spread's `R` and `L` slots, addressed individually via `pairing.ts`'s
   * `correctionTiles` while correcting — read as anything other than two
   * identical, unlabelled thumbnails: `Filmstrip` folds this into each
   * tile's `alt` text, the same `page N half` convention `App.tsx`'s `altAt`
   * uses for the page view itself.
   */
  half?: 'L' | 'R'
}

/**
 * The pages a tile shows for one `groups()` entry.
 *
 * A joined spread is two slots but **one page** — `pairing.ts`'s `groups`
 * already guarantees `here.page === next.page` for a `spread: true` group,
 * so rendering one `<img>` per slot would fetch the same thumbnail twice.
 * This collapses it to the single entry the tile actually shows, and is
 * exactly the seam a slot/page mixup would show up on: `slots` is a
 * *slot*-indexed table, `page` the number `thumbUrl` needs, and the two
 * only coincide up to the edition's first joined spread.
 *
 * A non-spread pair (`two` mode) keeps both slots — each is its own page —
 * and a singleton group is the one-slot case either way.
 *
 * Kept in its own module, separate from `Filmstrip.tsx`'s component, rather
 * than exported alongside it: a pure function sharing a file with a
 * component is exactly what defeats React Fast Refresh (biome's
 * `style/useComponentExportOnlyModules` flags this — enabled by name in
 * `biome.jsonc`, since it is not in the recommended set), and this codebase already
 * keeps pure logic and components apart everywhere else (`pairing.ts`,
 * `reading.ts`, `nearest.ts` beside `App.tsx`, `Chrome.tsx`, `Gap.tsx`).
 * `tilePages` is also directly unit-tested (`tiles.test.ts`), which a plain
 * module suits better than a component file.
 */
export function tilePages(group: Group, slots: Position[]): Tile[] {
  if (group.spread) {
    const [slot] = group.slots
    return [{ slot, page: slots[slot].page }]
  }
  if (group.slots.length === 1) {
    const [slot] = group.slots
    const { page, half } = slots[slot]
    return [{ slot, page, half }]
  }
  return group.slots.map((slot) => ({ slot, page: slots[slot].page }))
}

/**
 * How many nominal page-widths one tile occupies.
 *
 * This is what lets the rail be windowed at all: past its `minWidth` floor, a
 * tile's width is otherwise whatever its thumbnail turns out to draw at, so
 * nothing can say where tile 150 begins without first loading tiles 0
 * through 149.
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

/**
 * The `minWidth` floor `Filmstrip.tsx` gives a tile of `units`, and the one
 * definition both the tile style and the runtime `--strip-unit` measurement
 * below read — rather than each holding its own copy of the `calc()`
 * string, which would let the two silently drift apart the next time either
 * one changed. The measurement effect has to recognise a *one-unit* tile
 * (as opposed to a two-unit spread or pair, whose floor is double) by
 * matching a rendered tile's `style.minWidth` back against `tileMinWidth(1)`
 * — see `Filmstrip.tsx`'s mount effect.
 */
export function tileMinWidth(units: 1 | 2): string {
  return `calc(var(--strip-unit) * ${units} + var(--strip-tile-border) * 2)`
}

/**
 * A loaded tile's measured outer width, rescaled to what a **one-unit**
 * tile of the same per-unit width would have measured at — what
 * `medianTileWidth`'s sample is built from now that it draws from every
 * loaded tile, not only one-unit ones (fix-round review, finding A).
 *
 * `tileMinWidth`'s own `calc()`, read backwards: a tile's outer width is
 * `unit * units + border * 2`, and the border term is the same two edges
 * whatever `units` is — so it has to come out *before* dividing by `units`,
 * or a two-unit tile's own border would be halved along with its content.
 * Adding the border back on afterwards is what keeps the result an *outer*
 * width — the same kind of number `medianTileWidth`'s sample, and
 * `refinedStripUnit`'s `outerWidth` parameter that consumes it, already
 * meant, rather than a bare per-unit price that would need both of those to
 * change what they expect. For a one-unit tile this is the identity:
 * `(outerWidth - border*2) / 1 + border*2` is `outerWidth` again, so this
 * changes nothing for the samples that already worked.
 *
 * **Why every loaded tile needs to contribute at all.** In `two` mode every
 * group but a break singleton is two slots (`pairing.ts`), so a 33-tile
 * window can hold as few as zero loaded one-unit tiles — under the old
 * one-unit-only filter, `medianTileWidth` could never reach a sample there,
 * which left `--strip-unit` unrefined and `Filmstrip.tsx`'s `load` listener
 * attached for the component's whole life (see that effect's own comment).
 * A two-unit tile answers the same question a one-unit tile does — what one
 * unit measures on this platform, right now — once its own border and unit
 * count are divided back out, so `two` mode gets a sample from its own
 * tiles rather than needing a one-unit tile to happen to be in the window
 * at all.
 */
export function normalizedTileWidth(
  outerWidth: number,
  border: number,
  units: 1 | 2,
): number {
  return (outerWidth - border * 2) / units + border * 2
}

/**
 * The corrected `--strip-unit`, derived from a real tile's measured outer
 * width — or `undefined` when the current value is already close enough
 * that writing one would be noise.
 *
 * `outerWidth` is expected to be a loaded tile's `getBoundingClientRect()
 * .width` — or, since C2 (whole-branch review), `medianTileWidth`'s median
 * of several loaded tiles' — a border-box width either way, because the
 * tile `<button>` is `box-sizing: border-box` (`Filmstrip.tsx`'s
 * `tileStyle`). Subtracting both borders back out is what recovers the same
 * `--strip-unit` the CSS floor and the spacers already build on top of —
 * see `reader.css`'s own comment on `--strip-unit` for why that figure is a
 * content width, not the tile's full outer width.
 *
 * **Why a runtime measurement at all, rather than a better constant.**
 * Because the width a tile draws at is not the same in every context, and
 * the difference is not this code's to predict. Measured at 390×844 on the
 * same machine and the same browser: under desktop emulation the rail
 * subtracts 15 px of its own height (`offsetHeight` 112, `clientHeight` 97),
 * the image draws 81 tall and a loaded one-unit tile 58.16 wide; under
 * mobile emulation it subtracts **nothing**, the image draws 96 tall and the
 * tile 68.19 — against a nominal of 58 either way. The spacers stay sized to
 * the constant, so on the second of those a loaded tile sits 10.19 px over
 * nominal, the mismatch grows and shrinks as tiles load and the window
 * re-anchors, and it shows up as scroll-position drift.
 *
 * **What subtracts those 15 px is not settled here, and this doc does not
 * pretend otherwise.** A classic scrollbar is the obvious candidate and 15 px
 * is its usual thickness — but O138 measured this rail at 1440×900 in this
 * same engine but in *headless* Chromium, not the driven, emulated one used
 * here, and found the subtraction **0 px**, and recorded that neither
 * this machine's Chromium nor the user's own desktop shows a classic
 * scrollbar. That row left the hook this one is hanging on: *"If the 15 px
 * returns, that unexplained difference is where to start."* It has returned
 * and it is still unexplained.
 *
 * The fix does not depend on the answer, which is why it did not wait for
 * one: whatever takes the height, a hardcoded number cannot be right in both
 * conditions, and measuring a tile that has actually drawn is correct without
 * knowing why the contexts differ.
 *
 * **The `tolerance` (0.5 px default) is what lets `Filmstrip`'s effect call
 * this at most once per mount and still converge.** Setting `--strip-unit`
 * changes every tile's width on the next paint, which could in principle
 * re-trigger a measurement — but `Filmstrip` never measures a second time
 * once its loaded sample first reaches its minimum size (a ref flag blocks
 * it — see `medianTileWidth`, which is what that sample is reduced through
 * before it ever reaches here), so there is nothing left to re-trigger
 * regardless of this function's answer. The tolerance instead absorbs
 * sub-pixel rounding: a measured value within half a pixel of the CSS
 * default means the constant already matches this context — which is what
 * desktop emulation at `dpr` 1 gives, 54.16 against 54 — and not a
 * difference worth a style write. **It is one case and not necessarily the
 * usual one** — mobile emulation at the same size gives 68.19 against the
 * same 58 nominal, where this function does write. What happens at other
 * sizes is not claimed here: the only width measured at 1440×900 is
 * `Filmstrip.browser.test.tsx`'s, which finds a tile within half a pixel of
 * nominal — so the constant appears right there, and an earlier draft of
 * this comment asserting the opposite was inferring widths from O138's
 * *height* readings, which is not something they can support.
 */
export function refinedStripUnit(
  outerWidth: number,
  border: number,
  currentUnit: number,
  tolerance = 0.5,
): number | undefined {
  const unit = outerWidth - border * 2
  return Math.abs(unit - currentUnit) > tolerance ? unit : undefined
}

/**
 * The median of `widths` — every loaded tile currently rendered, each
 * already rescaled by `normalizedTileWidth` to the outer width a one-unit
 * tile of the same per-unit width would have measured at — or `undefined`
 * when there are fewer than `minSample` of them to trust.
 *
 * **Why a median, not the first qualifying tile (C2, whole-branch
 * review).** A double-page scan the sidecar never marked `spread: true` is a
 * one-slot group, so `tileUnits` correctly calls it one unit and it carries
 * the one-unit `minWidth` floor — while drawing at roughly twice the nominal
 * width, the same way a genuine `spread: true` tile does. `Filmstrip`'s
 * measurement effect cannot tell that tile apart from an ordinary one by its
 * `minWidth` alone, because there is nothing to tell apart: both are
 * one-unit by the sidecar's own (wrong, in this case) account. Accepting
 * whichever tile happened to load first — the previous rule — corrupted
 * `--strip-unit`, and with it every tile floor and every spacer, to about
 * double the true value whenever that tile won the race. This is not a
 * hypothetical: `docs/notes.md` records six such pages on
 * `akame-ga-kill----v01--2015--cbz`, and a stubbed double-width tile
 * reproduced `--strip-unit` taking that tile's width instead of the true
 * ~64 — `tiles.test.ts` and `Filmstrip.test.tsx` carry the exact figures,
 * which is where to read them rather than from a number repeated here. A median survives
 * this **when the outliers are a minority of the sample**, whatever their
 * own width is: they sort to one end and cannot reach the middle. That is a
 * weaker guarantee than "rare in the edition", and deliberately stated as
 * the weaker one — the sample is only the tiles loaded around where the
 * reader is, and `docs/notes.md` records the six unmarked scans on
 * `akame-ga-kill…v01` as a *run*, so a chrome opened inside that run is
 * exactly the case an appeal to edition-wide rarity would wave away. With
 * three of five samples against it the median gives the outlier outright.
 * What bounds the damage then is that every tile contributes, so the sample
 * is as wide as the window rather than as narrow as the run.
 *
 * **Why every loaded tile, not only one-unit ones (fix-round review,
 * finding A).** The measurement used to filter its sample down to one-unit
 * tiles before ever reaching this function. In `two` mode every group but a
 * break singleton is two slots, so a 33-tile window could hold as few as
 * zero qualifying tiles, `minSample` could then never be reached, and the
 * `load` listener reading this never detached — it kept re-scanning the
 * rail and forcing a layout read on every thumbnail that loaded, for the
 * life of the mount, on the branch whose whole purpose was removing that
 * per-load cost. `normalizedTileWidth` is what lets a two-unit tile answer
 * the same question a one-unit tile does, so `widths` here now draws from
 * the whole rendered window rather than a subset of it that can be empty.
 *
 * **Why a minimum sample at all.** The same asymmetry runs the other way on
 * a small sample: with one loaded tile, the "median" of one value is that
 * value, and if it happens to be the one unmarked double-page scan in the
 * window, nothing in the arithmetic would say so. `minSample` (3 by
 * default, lowered from 5 now that every loaded tile contributes rather
 * than only one-unit ones) is what an early render — the first frame or
 * two, before most of the window's thumbnails have decoded — cannot yet
 * satisfy, so this declines to answer rather than let one tile of either
 * kind decide the measurement. Reaching this floor is not the only way
 * `Filmstrip`'s `load` listener stops, though: a rail with fewer tiles than
 * `minSample` altogether — a work with fewer than `minSample` groups, say —
 * can never reach it, which is why that listener also detaches once every
 * tile it could ever measure has loaded, whether or not this function ever
 * answered. See that effect's own comment for why neither condition alone
 * is enough.
 */
export function medianTileWidth(
  widths: number[],
  minSample = 3,
): number | undefined {
  if (widths.length < minSample) return undefined
  const sorted = [...widths].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid]
}

/**
 * The summed nominal units of `groups.slice(from, to)` — what one of
 * `Filmstrip`'s two spacers has to cover for the tiles it stands in for.
 *
 * **Not `to - from` (a tile count).** A joined spread is two units in one
 * group (see `tileUnits`), so a spacer sized by counting groups rather than
 * summing `tileUnits` across them comes up one unit short per spread outside
 * the window. `Filmstrip.test.tsx` does build `spread: true` fixtures, in two
 * of its tile-floor tests — but each is only 1–2 groups, small enough that
 * the rail's 33-tile window renders the whole thing: `lo` stays 0 and `hi`
 * stays `groups.length`, so neither spacer's `lo > 0` / `hi < groups.length`
 * guard ever fires and this function is never reached with a spread group in
 * range. That file's *windowing* fixtures, the ones that do produce a
 * non-empty range on both sides, are all singleton groups with no spread —
 * and so is `Filmstrip.browser.test.tsx`'s own 120-tile fixture. `tiles.test.ts`
 * below is what actually exercises a `spread: true` group inside a summed
 * range.
 *
 * Lives here rather than as a closure inside `Filmstrip.tsx` (where it
 * started, as `unitsIn`) for the same reason every other pure function in
 * this module does: a component file can only export components
 * (`style/useComponentExportOnlyModules` in `biome.jsonc`), and a closure
 * that never left `Filmstrip.tsx` had no test of its own — the summation was
 * only ever exercised through fixtures where it agreed with a tile count.
 */
export function unitsBetween(
  groups: Group[],
  from: number,
  to: number,
): number {
  return groups.slice(from, to).reduce((sum, g) => sum + tileUnits(g), 0)
}

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
 * window with a sixteen-tile margin, not to scroll to anything.
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

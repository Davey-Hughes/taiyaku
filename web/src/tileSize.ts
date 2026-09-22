/**
 * How large a filmstrip tile to ask the server for — O134.
 *
 * The strip's tiles were one size, `cover.rs`'s `THUMB_LONG_EDGE` of 160 px,
 * whatever screen drew them: 1.8× upscaled on the 3× phone the row was opened
 * from, where the tile is drawn 96 CSS px tall. The server now makes a tile at
 * any rung of `TILE_LADDER` (`cover.rs`'s `TILE_LONG_EDGES` — the same three),
 * and this is the client half: which rung a given screen should ask for.
 *
 * **The same shape as `pageSize.ts`, with two rules deliberately inverted.**
 * Read that module first; the differences are marked below and each one is
 * about a tile being small and numerous where a page is large and single.
 */

/**
 * The long edges the server will make a tile at.
 *
 * Three rungs cover every case: a tile is drawn 96 or 160 CSS px tall and a
 * screen is 1×, 2× or 3×, so the pixels wanted are 96, 160, 192, 288, 320 or
 * 480. **Held identically in `cover.rs`'s `TILE_LONG_EDGES`**; a rung added
 * here alone is a `w` the server does not recognise and answers with the
 * 160 px tile, which is soft rather than broken and so invisible without a
 * test that decodes the response.
 */
export const TILE_LADDER: readonly number[] = [160, 320, 480]

/**
 * The viewport width below which the strip's tile is the shorter one, and the
 * two heights it takes — `styles/reader.css`'s `--strip-tile`.
 *
 * **Restated here rather than read from the stylesheet**, because the answer
 * is needed during the strip's first render: reading it back from
 * `getComputedStyle` needs the rail to be in the document, which is one commit
 * too late — every tile would fetch at 160 and then again at its real rung.
 * `styles/reader.css.test.ts` pins the stylesheet against these two constants
 * so the restatement cannot drift, which is the same trade `pageSize.ts` does
 * not have to make because a page's box is measured rather than declared.
 */
export const STRIP_BREAKPOINT_PX = 900
export const STRIP_TILE_PX = { wide: 160, narrow: 96 } as const

/**
 * The rung to ask for, for a tile drawn `tileHeightCss` CSS pixels tall on a
 * display of `dpr`.
 *
 * Rounding **up**, never down, exactly as `requestedLongEdge` does: the cost
 * of a rung too far is bytes, and the cost of a rung too short is the blur
 * this row exists to remove.
 *
 * **Above the top rung it clamps rather than answering `undefined`.** A page
 * has an original to fall back to and a tile does not, so a 4× display's 640
 * is served the 480 that exists.
 *
 * **Any input that is not a positive finite number falls to the *smallest*
 * rung** — the opposite direction from `requestedLongEdge`, which falls to the
 * original. A wrong size is invisible either way, so each falls toward its own
 * cheap failure: a soft page is unreadable, where a soft tile is a thumbnail
 * nobody reads, and there are 237 of them.
 */
export function requestedTileEdge(tileHeightCss: number, dpr: number): number {
  const usable = (value: number) => Number.isFinite(value) && value > 0
  if (!usable(tileHeightCss) || !usable(dpr)) return TILE_LADDER[0]
  const wanted = tileHeightCss * dpr
  return (
    TILE_LADDER.find((rung) => rung >= wanted) ??
    TILE_LADDER[TILE_LADDER.length - 1]
  )
}

/** Just enough of a `Window` to choose a rung, so a test can hand over a fake. */
export interface TileView {
  matchMedia?: (query: string) => { matches: boolean }
  devicePixelRatio?: number
}

/**
 * The rung the filmstrip should ask for in `view`.
 *
 * **`matchMedia` rather than the drawn height**, for the reason
 * `STRIP_TILE_PX` gives: this is called during the first render, before there
 * is an element to measure. Optional because jsdom does not implement
 * `matchMedia` at all — probed on 2026-09-21: it is `undefined`, not a stub
 * that answers `false` — so every component test in that project takes the
 * missing branch, and it has to mean "the wide tile" rather than throw.
 */
export function stripTileEdge(view: TileView | undefined): number {
  const query = view?.matchMedia?.(`(width < ${STRIP_BREAKPOINT_PX}px)`)
  const height =
    query?.matches === true ? STRIP_TILE_PX.narrow : STRIP_TILE_PX.wide
  return requestedTileEdge(height, view?.devicePixelRatio ?? 1)
}

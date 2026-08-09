/**
 * How large a page to ask the server for.
 *
 * **The measurement that put this here.** `gotoubun-v01`'s `en-mono` pages are
 * 2147×3056 — 6.56 megapixels — and in a 1385×512 window the reader draws them
 * into a box of 693×475. Twenty times the pixels that reach the screen. What
 * that costs, measured in the browser with the bytes already cached so the
 * timing is decode alone:
 *
 * | fetched at | pixels | decode | decoded bitmap |
 * | ---------- | -----: | -----: | -------------: |
 * | 2147 (as released) | 6.56 MP | 74.5 ms | 26.2 MB |
 * | 1600 | 3.64 MP | 14.9 ms | 14.6 MB |
 * | 1024 | 1.49 MP |  7.3 ms |  6.0 MB |
 * | 700  | 0.70 MP |  4.3 ms |  2.8 MB |
 *
 * Both columns matter. The decode is what a page turn waits on — `decode.ts`
 * holds the outgoing page until the incoming one can be painted, and
 * `cohort.ts` makes a spread wait for the slower of its halves. The bitmap is
 * what the browser evicts under memory pressure, which is what made an
 * occasional turn cost hundreds of milliseconds instead of single digits; at
 * 2.8 MB there is far less worth evicting. `raw-mono`, at 685×1024, has never
 * felt slow, and this is why.
 *
 * **A ladder rather than the exact size**, because the server caches one file
 * per (member, page, long edge) and a reader dragging a window edge would
 * otherwise mint a cache entry per pixel of width. Six rungs cover phone
 * through 4K; the cost of rounding up is at most one rung's worth of decode.
 */
export const SIZE_LADDER: readonly number[] = [512, 768, 1024, 1536, 2048, 3072]

/**
 * The long edge to request for a page drawn into a box whose long edge is
 * `boxLongEdge` CSS pixels, on a display of `devicePixelRatio`, magnified by
 * `scale`.
 *
 * `undefined` means **ask for the original**, and it is the answer whenever the
 * reader needs more pixels than the ladder offers. That is what makes zoom
 * correct rather than soft: L1 magnifies up to 8×, and a page fetched for
 * fit-whole would be blurry exactly where zoom is the point. Rather than a
 * separate "is the reader zoomed" flag, magnification simply multiplies the
 * pixels wanted, and once that exceeds the top rung the full-resolution page is
 * the only thing that satisfies it — one rule covering fit-whole, a nudge above
 * 1×, and 8×.
 *
 * `undefined` is also the answer for any input that is not a positive finite
 * number. A wrong size here is invisible — the page still draws, just soft —
 * so the failure has to fall toward the original bytes rather than toward a
 * guess. `window.innerHeight` inside a hidden iframe, a `devicePixelRatio` of 0
 * from a headless context, a `scale` derived from a zero-width rect: all of
 * them land on full resolution rather than on a 512px page nobody can read.
 *
 * Rounding **up** to a rung, never down: a page drawn at 1025 CSS pixels asked
 * for at 1024 is visibly soft, where asking at 1536 costs about half a
 * frame of decode.
 */
export function requestedLongEdge(
  boxLongEdge: number,
  devicePixelRatio: number,
  scale: number,
): number | undefined {
  const usable = (value: number) => Number.isFinite(value) && value > 0
  if (!usable(boxLongEdge) || !usable(devicePixelRatio) || !usable(scale)) {
    return undefined
  }
  const wanted = boxLongEdge * devicePixelRatio * scale
  return SIZE_LADDER.find((rung) => rung >= wanted)
}

/**
 * The long edge of the box one page is drawn into, from the window and the
 * layout mode.
 *
 * `two` mode halves the width, because each of its cells holds one page; every
 * other mode gives a page the whole area. Deliberately an over-estimate — it
 * ignores the chrome, the seam padding and any letterboxing, so the number is
 * never smaller than what is really drawn. Erring high costs at most one rung;
 * erring low is a soft page.
 */
export function pageBoxLongEdge(
  windowWidth: number,
  windowHeight: number,
  mode: 'single' | 'two' | 'scroll',
): number {
  const width = mode === 'two' ? windowWidth / 2 : windowWidth
  return Math.max(width, windowHeight)
}

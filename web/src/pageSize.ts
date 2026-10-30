/**
 * How large a page to ask the server for.
 *
 * A page is fetched near the size it is drawn at, not at the original: a page
 * fetched at full size can carry far more pixels than reach the screen. Both
 * decode time and decoded bitmap size matter. The decode is what a page turn
 * waits on — `decode.ts` holds the outgoing page until the incoming one can be
 * painted, and `cohort.ts` makes a spread wait for the slower of its halves.
 * The bitmap is what the browser evicts under memory pressure, which is what
 * makes an occasional turn slow; a small bitmap leaves far less worth evicting.
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
 * for at 1024 is visibly soft, where asking at 1536 costs a little more decode.
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

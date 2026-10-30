import type { ArtworkHug } from './modules/fit'
import type { Cell, Drawn, Frame, MapKind } from './viewport'

/**
 * Carrying the view across a swap: the two-phase machine, without the DOM.
 *
 * The two halves of a swap are available at two different moments and neither
 * can wait for the other. The outgoing page is on screen when the key is
 * pressed and gone after the re-render; the incoming one does not exist yet,
 * because `PageView`'s decode gate deliberately holds the old image until the
 * new one is ready to paint. So a swap is recorded on the keypress and applied
 * one decode later, and everything in between is a decision about which of
 * those two states the reader is in.
 *
 * Not folded into `viewport.ts`: that file is what a rect *means*. This is
 * *when to ask it*. Not folded into `modules/zoom.ts` either: it takes no
 * `ZoomPrefs` beyond the `MapKind` that module already derives, and it is the
 * one part of the geometry that is a state machine rather than arithmetic.
 *
 * A wrong answer here displaces the view by half a group and carries the
 * displacement forward through every later swap, until the reader presses `0`.
 */

/**
 * As much of an `<img>` as measuring needs, so no DOM is required to test it.
 *
 * `NodeListOf<HTMLImageElement>` satisfies this structurally, which is what
 * lets the caller pass `host.querySelectorAll('img')` straight in.
 */
export interface Measurable {
  src: string
  naturalWidth: number
  naturalHeight: number
}

/**
 * The natural size of the page `url` names, from the images on screen now.
 *
 * Matched by URL suffix rather than by threading slot identity down the tree,
 * the same rule `draftImage` and `PageAnnotations` already match by — `pageAt`
 * builds exactly this URL, and `two` mode draws two pages of which only one is
 * the slot being asked about.
 *
 * `undefined` covers three states that are one state for a caller: the page is
 * not on screen, or it is but has not decoded (the decode gate holds the
 * *outgoing* page's `src` on the element until the incoming one is ready, so a
 * swap pressed twice quickly genuinely has no frame to measure), or it decoded
 * to nothing. All three mean the transform cannot be computed, which `mapView`
 * and the design's §9 both answer by leaving the view alone.
 */
export function frameOf(
  images: Iterable<Measurable>,
  url: string,
): Frame | undefined {
  for (const image of images) {
    if (!image.src.endsWith(url)) continue
    if (image.naturalWidth <= 0 || image.naturalHeight <= 0) return undefined
    return { w: image.naturalWidth, h: image.naturalHeight }
  }
  return undefined
}

/**
 * What the page area is drawing — the page holding the slot the reader is
 * standing on, which of the group's cells draws it, and which half of it the
 * slot actually names.
 *
 * `half` is `Drawn`'s and it is the slot's rather than the image's: a joined
 * spread is one image drawn for two slots, so the URL and the frame are the
 * same on both and only this tells `mapView` which of the two the reader is
 * on. Absent for every ordinary slot.
 */
export interface DrawnPage {
  url: string
  cell: Cell
  half?: 'L' | 'R'
  hug: ArtworkHug
}

/** A swap whose incoming page has not been measured yet. */
export interface CarriedSwap {
  from: Drawn
  /** The edition being **left** — what tells a no-op digit from a real swap. */
  edition: string
}

/**
 * Measure the outgoing page, at the last moment it is still on screen.
 *
 * `undefined` means this swap carries nothing, which the reader sees as the
 * view left where it was — the design's §9 answer, reached the same way for a
 * swap onto a gap and for one pressed faster than a page decodes.
 *
 * `hug` is carried too: the artwork is pulled to the seam in two-page mode, and
 * a swap that measured it as centred would ask the wrong question —
 * see `Drawn.hug`.
 */
export function carrySwap(
  drawn: DrawnPage | undefined,
  frame: Frame | undefined,
  leaving: string | undefined,
  enabled: boolean,
): CarriedSwap | undefined {
  if (!enabled || !drawn || !leaving || !frame) return undefined
  return {
    from: { frame, cell: drawn.cell, half: drawn.half, hug: drawn.hug },
    edition: leaving,
  }
}

/**
 * What a carried swap should do, asked once when it is recorded and again on
 * every `load` the page area hears until one of the answers is final.
 *
 * **Four kinds, not three.** `apply`/`wait`/`drop` conflates two pairs of
 * distinct cases, and one of those conflations introduces a defect — see
 * `done` below, which is the one that does.
 */
export type SwapStep =
  | {
      kind: 'apply'
      /** Hand these to `mapView` inside `setView`'s updater. */
      from: Drawn
      to: Drawn
      areaAspect: number
      mapKind: MapKind
    }
  /**
   * Keep the `load` listener attached; do **not** clear the carry.
   *
   * `why` because the two waits are not the same wait, and only one of them
   * ends by itself:
   *
   * - `'undecoded'` — the incoming image is on screen but has not decoded, so
   *   `frameOf` finds nothing to measure. A `load` event is exactly what ends
   *   this, which is why the listener is there.
   * - `'no-box'` — the page area has no layout. **No `load` will end this**,
   *   because nothing about the image is what is missing; it ends on the next
   *   render that gives the area a box, and the effect re-runs on the reader's
   *   next move. It is a wait rather than a drop because a zero box is an
   *   element mid-teardown — the same guard the wheel handler states in full,
   *   where dividing by it puts `NaN` into the rect and the page vanishes with
   *   no error.
   */
  | { kind: 'wait'; why: 'undecoded' | 'no-box' }
  /**
   * Nothing to land on, or a swap that changed nothing. **Clear the carry.**
   *
   * Three ways in: no page area to measure at all (`box` absent), nothing
   * drawn to carry the view onto (`to` absent — a mode switch to scroll, or
   * the reader moved onto a gap), and `viewing === carried.edition`, which is
   * a digit pressed for the edition already on screen. `swap` builds a fresh
   * `ReadingState` either way, so that comparison is the only thing that tells
   * a no-op swap from a real one.
   */
  | { kind: 'drop' }
  /**
   * Finished, and the carry belongs to somebody else — **do not clear it.**
   *
   * A later swap replaced this one while it waited, and that newer swap has
   * its own effect with its own listener. The difference from `drop` is
   * load-bearing: clearing would throw away the newer swap's carry and leave
   * the view where it was, which is the displacement this module exists to
   * prevent — and it would do it only under a swap pressed faster than a page
   * decodes, which is the hardest state in this reader to reach by hand.
   */
  | { kind: 'done' }

/**
 * `stillCurrent` is "the carry this call is about is still the one held" read
 * at the call site. It is a parameter rather than a check the caller makes
 * before calling, so the "finished but do not clear" rule is inside the tested
 * module rather than in `App`.
 *
 * `to`, `frame` and `box` are read fresh on every attempt rather than captured
 * when the swap was recorded. `to` in particular: a switch between single and
 * two-page mode while the incoming page decodes would otherwise leave a `cell`
 * measured under the other mode, and `cell` is which half of the group the
 * page occupies, so a stale one lands the view about half a group out.
 */
export function swapStep(
  carried: CarriedSwap,
  stillCurrent: boolean,
  to: DrawnPage | undefined,
  frame: Frame | undefined,
  box: { width: number; height: number } | undefined,
  viewing: string | undefined,
  mapKind: MapKind,
): SwapStep {
  if (!stillCurrent) return { kind: 'done' }
  if (!box || !to || viewing === carried.edition) return { kind: 'drop' }
  if (!frame) return { kind: 'wait', why: 'undecoded' }
  if (box.width <= 0 || box.height <= 0) return { kind: 'wait', why: 'no-box' }
  return {
    kind: 'apply',
    from: carried.from,
    to: { frame, cell: to.cell, half: to.half, hug: to.hug },
    areaAspect: box.width / box.height,
    mapKind,
  }
}

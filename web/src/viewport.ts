import type { ArtworkHug } from './modules/fit'

/**
 * What part of the drawn group the reader is looking at.
 *
 * Fractions of the group, not pixels and not fractions of a page: `WHOLE` is
 * fit-whole, which is what the reader did before zoom existed, and every
 * other rect is a window into it.
 *
 * A rect rather than a scale plus a centre. A swap has to preserve what was
 * *visible*, and a clamp has to bound it; both of those are statements about
 * a rectangle, and deriving one from a scale and a point at every use is the
 * same arithmetic written in three places.
 *
 * **`w === h` holds for every rect held as this state**, and it is why
 * `Viewport` (`Viewport.tsx`) can draw with a single `scale(1/rect.w,
 * 1/rect.h)`. The moment the two differ that scale stops being uniform — the
 * page stretches on whichever axis has the smaller fraction — and nothing
 * here would notice: `w` and `h` are still finite, positive numbers, every
 * function below still returns cleanly, and no test in `viewport.test.ts`
 * inspects a CSS transform to catch it.
 *
 * It can hold because group space *is* the page-area box (`paintedIn`, below,
 * already says as much): `Viewport`'s wrapper is `100%` of
 * its parent, so `WHOLE` — and anything derived from it while it keeps this
 * shape — fills exactly the box whose aspect `fitRect` takes as
 * `viewportAspect`. A rect whose real proportions (`rect.w * groupAspect /
 * rect.h`, `fitRect`'s own term for it) equal that box's aspect is, by that
 * equality, a rect with the same fraction on both axes.
 *
 * `fitRect` called with `groupAspect === viewportAspect` is what produces
 * that, as a consequence of its existing contract rather than a case of its
 * own: on every path but its own early return (see `fitRect`'s doc — an
 * invalid aspect or a degenerate rect), it returns a rect whose real aspect
 * equals `viewportAspect` (`viewport.test.ts` fixes `groupAspect = 1`,
 * `viewportAspect = 2` on `{x:0.25,y:0.25,w:0.5,h:0.5}` and gets back
 * `{w:1,h:0.5}`, unequal, because the two aspects passed in there are
 * unequal). Pass the same value for both and `rect.w === rect.h` is what
 * that contract reduces to. `zoomAt`, `panBy` and `clampRect` all preserve it
 * afterward: `panBy` never touches `w` or `h`; `clampRect` clamps each axis
 * by the same rule against the same starting size, so two equal sizes clamp
 * to two equal results; `zoomAt` divides `w` and `h` by the same `by` and
 * clamps both to the same floor and the same `1` before its own call into
 * `clampRect`, so two equal inputs are still equal going in.
 *
 * `into`, `outOf`, and `mapRect`'s `'pillarbox'` case are the three that may
 * not preserve it, because each can scale the two axes by different factors —
 * a cell is half the group's width and all of its height; a pillarbox
 * rescales one axis and leaves the other alone. **May, not must**, and the
 * distinction is reachable rather than pedantic: `into`/`outOf` given a square
 * `inner` scale both axes alike and come back unchanged, which is exactly what
 * `cellBox('only')` hands them — so `imageToGroup(rect, 'only')` preserves the
 * invariant while `imageToGroup(rect, 'first')` does not. `into`'s own doc
 * already hedges this correctly; an earlier draft of this paragraph said the
 * three "deliberately do not preserve it", which overclaimed against the very
 * warning it was written to generalise.
 *
 * Each of the three says, at its own return, that its result is not fit to
 * hold as this state and needs `fitRect` first. This paragraph is the general
 * rule those are instances of, not a fourth restatement of it — and the rule
 * is that a result which *may* break the invariant has to be re-fitted, since
 * a caller cannot tell from the type which case it got.
 */
export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

/**
 * Fit-whole: the entire group, which is how the reader drew before zoom.
 *
 * Frozen. This is the identity constant every caller shares, not a template
 * each one gets its own copy of, and nothing in this module mutates a rect
 * in place anyway — so a caller that wrote to it by mistake would otherwise
 * corrupt it for every other caller at once, silently, and `toEqual(WHOLE)`
 * in a test would not catch that.
 */
export const WHOLE: Rect = Object.freeze({ x: 0, y: 0, w: 1, h: 1 })

/**
 * A rect brought inside the group it addresses.
 *
 * **Sliding before shrinking is the rule**, and the order matters. A rect that
 * merely overhangs is a pan that went too far, and the reader's magnification
 * should survive it — so it slides back and keeps its size. A rect genuinely
 * larger than the group cannot slide anywhere, and is reduced to the group's
 * own extent on that axis.
 *
 * **What the second case is actually for**, since this comment got it wrong
 * once: it is `zoomAt` reaching full size. `zoomAt` caps each axis at `1` and
 * *then* solves an origin from the anchor, which at full size can be any
 * number at all — and at full size there is exactly one position the axis can
 * occupy. Reducing to `{start: 0, size: 1}` is what puts it there. Nothing
 * else in this module produces a size above `1`.
 *
 * It said "a group is two slots wide or one — a gap draws one, an odd tail
 * draws one — so a rect carried across a page turn from a pair onto a
 * singleton addresses space that does not exist" until 2026-08-07, and that
 * is false. It described an earlier model in which group space was the *drawn
 * content*, whose width really did change with the slot count. Group space is
 * the **page-area box**, which is the same box in every group — so a rect
 * carried across a turn is already inside it, and `App.tsx`'s clamp on a turn
 * is defensive rather than load-bearing, as that call site now says. The
 * correction outlived the comment because a wrong justification for correct
 * code has nothing to fail against.
 *
 * Each axis is still clamped independently. Not for the reason above — the
 * two axes of the page-area box change together, on a window resize — but
 * because `into`, `outOf` and `mapRect` all scale the axes by different
 * factors, so a rect arriving here can overhang on one axis alone, and
 * clamping them together would move a reader vertically for a horizontal
 * reason.
 */
export function clampRect(rect: Rect): Rect {
  const axis = (start: number, size: number) => {
    if (size >= 1) return { start: 0, size: 1 }
    return { start: Math.min(Math.max(start, 0), 1 - size), size }
  }
  const h = axis(rect.x, rect.w)
  const v = axis(rect.y, rect.h)
  return { x: h.start, y: v.start, w: h.size, h: v.size }
}

/**
 * A rect grown to the viewport's shape, about its own centre.
 *
 * "The content, fitted" — the spec's §2. The mapped rect is generally not the
 * viewport's shape, because editions differ in aspect (measured: Kaguya's raw
 * is 0.703 against the English 0.667, `docs/notes.md`), and something has to
 * give. This grows, never shrinks, so a swap shows a little extra rather than
 * hiding a sliver that was visible a moment ago. Hiding it is the failure that
 * makes the feature untrustworthy; showing extra is harmless, and it is the
 * same conservative direction the composition loss is described as taking.
 *
 * Both aspects are width over height. `groupAspect` converts the rect's
 * fractions into real proportions — a rect half as wide and half as tall as a
 * 2:1 group is itself 2:1, not square.
 *
 * A non-finite or non-positive aspect means something has not been laid out or
 * decoded yet, and the honest answer is to leave the rect alone rather than
 * multiply by a zero.
 */
export function fitRect(
  rect: Rect,
  viewportAspect: number,
  groupAspect: number,
): Rect {
  if (
    !Number.isFinite(viewportAspect) ||
    !Number.isFinite(groupAspect) ||
    viewportAspect <= 0 ||
    groupAspect <= 0 ||
    rect.w <= 0 ||
    rect.h <= 0
  )
    return rect
  const real = (rect.w * groupAspect) / rect.h
  const grow = (start: number, size: number, by: number) => ({
    start: start - (size * (by - 1)) / 2,
    size: size * by,
  })
  if (real < viewportAspect) {
    const { start, size } = grow(rect.x, rect.w, viewportAspect / real)
    return { x: start, y: rect.y, w: size, h: rect.h }
  }
  const { start, size } = grow(rect.y, rect.h, real / viewportAspect)
  return { x: rect.x, y: start, w: rect.w, h: size }
}

/**
 * The rect after a zoom anchored at a point.
 *
 * `at` is in group fractions — where the pointer is over the *group*, not over
 * the viewport — and the anchor holds for as long as the zoomed rect fits
 * inside the group. Zooming about the rect's centre instead is the version
 * that feels broken: a reader aims at a balloon, and the balloon slides away.
 *
 * `by` above 1 zooms in. Bounded at both ends: never past `maxScale`, and
 * never wider than the group, because a rect larger than what it addresses is
 * not a view of anything. The two bounds behave differently once the anchor
 * is solved against them. The floor stops the zoom cleanly: the size stops
 * shrinking, and the anchor is solved against that floored size the same as
 * any other — the new rect is no larger than the one it replaces, and the
 * anchor sat inside that one, so the new rect fits without sliding. The
 * ceiling is different in kind: reaching the whole group's extent on an axis
 * leaves exactly one position on that axis, `0`, so the clamp below discards
 * the solved origin outright rather than sliding it — there is nowhere else
 * to put it. Measured by hand:
 * `zoomAt({x:0.1,y:0.1,w:0.9,h:0.9}, {x:0.15,y:0.15}, 0.5, 8)` hits the
 * ceiling on both axes (`0.9 / 0.5 = 1.8`, capped to `1`) and returns `WHOLE`
 * — the anchor sat at fraction ~0.056 of the old rect on both axes and lands
 * at 0.15 of the new one.
 *
 * Away from the ceiling the anchor still holds unless the zoomed rect would
 * overhang the group, which a zoom-out near the group's edge can do without
 * reaching full size. Clamping on the way out handles that without a second
 * rule for it: it is what puts an overhanging rect back inside, and where the
 * clamp has to slide the rect, the anchor gives way with it — the
 * alternative is showing background that is not part of the page. Measured
 * by hand: `zoomAt({x:0.7,y:0.7,w:0.3,h:0.3}, {x:0.85,y:0.85}, 0.5, 8)`
 * returns `{x:0.4,y:0.4,w:0.6,h:0.6}` — the anchor sat at fraction 0.5 of the
 * old rect on both axes and sits at fraction 0.75 of the new one, a visible
 * slide.
 *
 * **Both worked examples above sit flush against the same corner of the
 * group**, and saying so is the point: where the rect is placed is not what
 * separates them. Only whether the zoom reached full size is. An earlier
 * draft of this comment called one an edge case and the other a corner case,
 * which named a difference the two do not have.
 */
export function zoomAt(
  rect: Rect,
  at: { x: number; y: number },
  by: number,
  maxScale: number,
): Rect {
  // A gesture that cannot be interpreted should leave the view alone rather
  // than manufacture a rect from it: `by` outside `(0, Infinity)` has no
  // sensible zoom direction, and a non-finite or non-positive `rect.w`/`h`
  // divides by zero into NaN. A NaN rect reaches `scale(NaN)` downstream with
  // no error anywhere — the page just vanishes — so this is caught here
  // rather than left to fail silently further down.
  if (
    !Number.isFinite(by) ||
    by <= 0 ||
    !Number.isFinite(rect.w) ||
    !Number.isFinite(rect.h) ||
    rect.w <= 0 ||
    rect.h <= 0
  )
    return rect
  const floor = 1 / Math.max(maxScale, 1)
  const w = Math.min(Math.max(rect.w / by, floor), 1)
  const h = Math.min(Math.max(rect.h / by, floor), 1)
  // Where the anchor sat within the old rect has to be where it sits in the
  // new one; solving for the new origin is what keeps it under the pointer.
  const fx = rect.w > 0 ? (at.x - rect.x) / rect.w : 0.5
  const fy = rect.h > 0 ? (at.y - rect.y) / rect.h : 0.5
  return clampRect({ x: at.x - fx * w, y: at.y - fy * h, w, h })
}

/**
 * `WheelEvent.deltaMode`'s three values, named — `DOM_DELTA_PIXEL`,
 * `DOM_DELTA_LINE`, `DOM_DELTA_PAGE`. Not guesses; these are fixed by the
 * event interface. `wheelBy` still reads anything *else* as pixels rather than
 * switching exhaustively — see its default arm for why.
 */
const PIXELS = 0
const LINES = 1
const PAGES = 2

/**
 * How large a `deltaY` one notch of each kind is.
 *
 * The first three are what browsers conventionally emit for one detent of a
 * mouse wheel: 100 pixels in Chromium, 3 lines in Firefox, 1 page where the
 * browser has been configured to scroll a page at a time. **Convention, not a
 * measurement taken here** — which is the weaker claim of the two: a device
 * emitting some other per-detent size zooms faster or slower than one notch
 * per detent, and finding that out needs a wheel under a reader's hand.
 *
 * `pinch` is a guess with nothing behind it at all. A trackpad pinch reaches a
 * `wheel` listener as `ctrlKey` with pixel deltas an order of magnitude
 * smaller than a scroll's, so reading it at 100 px to the notch would make a
 * full-hand pinch worth a fraction of one notch — as useless as the defect
 * this function replaced, in the other direction. 10 is one order of
 * magnitude, chosen because that is the size of the discrepancy and for no
 * better reason. The browser pass is where it gets a number.
 */
const PER_NOTCH = { pixels: 100, lines: 3, pages: 1, pinch: 10 }

/**
 * The most notches one event may be worth. Also guesses.
 *
 * `MAX_NOTCHES` keeps one pathological event — an accelerated flick, or a
 * browser this has not been tried against — from crossing the whole zoom range
 * by itself, which is the same failure this function exists to fix. 4 notches
 * is 1.15⁴ ≈ 1.75× at the default `step`: a large single jump, and short of
 * the range.
 *
 * `MAX_PINCH_NOTCHES` is 1 and does a second job besides bounding a pinch. A
 * *mouse* wheel with ctrl held arrives indistinguishable from a pinch except
 * by its size (±100 px), and at the pinch rate that reads as ten notches.
 * Capping at one notch makes ctrl+wheel from a mouse do exactly what the same
 * wheel does without ctrl — the only defensible answer while the two gestures
 * cannot be told apart, and the reason the pinch rate is safe to guess at all.
 */
const MAX_NOTCHES = 4
const MAX_PINCH_NOTCHES = 1

/**
 * How much one wheel event asks the view to zoom, as `zoomAt`'s `by`.
 *
 * `step` stays "one notch" — the same number `=` and `-` apply per press — and
 * this raises it to however many notches the event carries. A mouse detent is
 * one notch and comes out as `step` exactly, so `zoom.ts`'s claim for the
 * preference ("1.15 is about five wheel notches to double") still holds; a
 * trackpad, which emits tens of small events per gesture, comes out as tens of
 * small factors that multiply to about the same thing over the same movement.
 *
 * **The alternative it beats is `deltaY < 0 ? step : 1 / step`**, which is what
 * this replaced: one full notch per event whatever its size. That discards the
 * magnitude, and the cost is arithmetic rather than taste — at the defaults
 * `step` 1.15 and `maxScale` 8, `1.15 ** 15 ≈ 8.14`, so any gesture emitting
 * fifteen events pins the view at maximum zoom. That a trackpad emits tens of
 * events per gesture is the claim behind calling fifteen a low bar, and it is
 * received rather than counted here.
 *
 * **A raw `deltaY` is not comparable across `deltaMode`s**, so it is
 * normalised to notches first. One detent is `100` in pixel mode and `3` in
 * line mode; dividing both by one constant would make the same gesture zoom
 * about thirty times faster in Firefox than in Chrome, which is the same bug
 * as the one above with a different trigger.
 *
 * Here rather than in a module of its own, and here rather than inline in
 * `App.tsx`'s listener: its only output is `zoomAt`'s `by` and its only input
 * besides the event is `step`, so it has nothing to say that is not about the
 * zoom directly above it — and `App.tsx` value-imports the resolver and is
 * unreachable from Vitest, so a rule left inline there is covered by typecheck
 * alone.
 *
 * Takes the fields it reads rather than a `WheelEvent`, so this module stays
 * free of DOM types and a test can hand it a plain object.
 */
export function wheelBy(
  wheel: { deltaY: number; deltaMode: number; ctrlKey: boolean },
  step: number,
): number {
  // A gesture that cannot be interpreted leaves the view alone. Returned as
  // `1` rather than as `NaN`: `zoomAt` declines a non-finite `by` anyway, but
  // a factor of exactly one says "no zoom was asked for" here, where the
  // decision is actually made, instead of deferring it one call.
  if (!Number.isFinite(wheel.deltaY) || wheel.deltaY === 0) return 1
  if (!Number.isFinite(step) || step <= 0) return 1
  const pinch = wheel.ctrlKey && wheel.deltaMode === PIXELS
  const per = pinch
    ? PER_NOTCH.pinch
    : wheel.deltaMode === LINES
      ? PER_NOTCH.lines
      : wheel.deltaMode === PAGES
        ? PER_NOTCH.pages
        : // Pixels, and anything a future `deltaMode` adds: an unknown mode
          // read as pixels zooms too slowly, where reading it as lines or
          // pages would zoom about thirty or a hundred times too fast.
          PER_NOTCH.pixels
  const notches = Math.min(
    Math.abs(wheel.deltaY) / per,
    pinch ? MAX_PINCH_NOTCHES : MAX_NOTCHES,
  )
  // Up the page is `deltaY < 0` in every mode, and up is zoom in.
  return step ** (wheel.deltaY < 0 ? notches : -notches)
}

/**
 * The rect after a pan, in group fractions.
 *
 * Clamped, so a drag that runs past the edge stops at it rather than sailing
 * off and leaving the reader looking at background. At fit-whole there is
 * nowhere to go and the rect comes back unchanged, which is what makes a drag
 * on an unzoomed page a no-op instead of a jitter.
 */
export function panBy(rect: Rect, dx: number, dy: number): Rect {
  return clampRect({ ...rect, x: rect.x + dx, y: rect.y + dy })
}

/**
 * Which of a group's drawing positions an image occupies.
 *
 * `'only'` is every case where one image fills the group: a joined spread and
 * a solo slot with no partner — the two cases `pairing.ts`'s `cellsFor`
 * reports as `wide: true` — plus `single` mode, which never calls `cellsFor`
 * at all (`App.tsx`'s `renderGroup` branches around it for every mode but
 * `two`) and so never had a second cell to begin with. `'first'` and
 * `'second'` are `two` mode's ordinary pair, in the order `cellsFor` puts
 * them on screen, which is already reading-direction-corrected. Nothing here
 * needs to know about `rtl` for that reason.
 */
export type Cell = 'only' | 'first' | 'second'

/**
 * The box a cell occupies within its group, in group fractions.
 *
 * `'only'` is the whole group; `'first'` and `'second'` are the two
 * equal-width flex cells `two` mode splits it into — see `Cell`'s doc for
 * which real cases map to which name. `imageToGroup` and `groupToImage`
 * below are `into` and `outOf` applied to this box.
 */
function cellBox(cell: Cell): Rect {
  if (cell === 'first') return { x: 0, y: 0, w: 0.5, h: 1 }
  if (cell === 'second') return { x: 0.5, y: 0, w: 0.5, h: 1 }
  return WHOLE
}

/**
 * A rect on one page image, expressed against the group that draws it.
 *
 * Two-page mode gives each cell half the group's width — `flex: 1` on two
 * cells of a full-width flex row — so an image rect's horizontal extent halves
 * and shifts by the cell's own offset. The vertical axis is untouched: cells
 * split width, not height.
 *
 * This is the conversion a swap goes through, and it is the reason zoom state
 * is held in group space rather than image space. The reader's viewport can
 * straddle both cells of a pair; an image-space rect cannot say that.
 *
 * Literally `into` with `cell`'s own box (`cellBox`, above) — kept as its own
 * name because the call sites read better for it than a `cellBox` lookup
 * would.
 */
export function imageToGroup(rect: Rect, cell: Cell): Rect {
  return into(rect, cellBox(cell))
}

/**
 * The inverse of `imageToGroup`, for the same layout — `outOf` with the same
 * `cellBox`.
 */
export function groupToImage(rect: Rect, cell: Cell): Rect {
  return outOf(rect, cellBox(cell))
}

/**
 * Where the page is painted inside the box that holds it, in that box's own
 * fractions.
 *
 * The fractional twin of `notes.ts`'s `paintedBox`, which answers the same
 * question in pixels for the annotation overlay. Kept separate rather than
 * shared because that one takes DOM measurements and this module has no DOM;
 * they are the same rule stated in the two units each caller already has.
 *
 * This is what stops the swap being wrong by exactly the letterbox. Group
 * space is the page-area box, but `PageView` drew fit-whole until the `fit` module shipped, so a portrait
 * page in a landscape viewport is inset with background down both sides. A
 * swap maps between *page* frames, so the inset comes out on the way in and
 * goes back on the way out.
 *
 * An unmeasured aspect — zero, negative or `NaN`, which is what an image
 * reports before it decodes — gives the whole box rather than a rect built
 * from a division by zero.
 *
 * `w` and `h` are almost never equal — a letterbox is exactly the case where
 * one axis shrinks and the other doesn't — so, like `into`/`outOf` below,
 * this result is not fit to hold as viewport state on its own; it feeds the
 * swap's arithmetic, not the zoom rect itself.
 */
export function paintedIn(
  imageAspect: number,
  boxAspect: number,
  hug: ArtworkHug = 'centre',
): Rect {
  if (
    !Number.isFinite(imageAspect) ||
    !Number.isFinite(boxAspect) ||
    imageAspect <= 0 ||
    boxAspect <= 0
  )
    return WHOLE
  const w = Math.min(1, imageAspect / boxAspect)
  const h = Math.min(1, boxAspect / imageAspect)
  const x = hug === 'left-edge' ? 0 : hug === 'right-edge' ? 1 - w : (1 - w) / 2
  return { x, y: (1 - h) / 2, w, h }
}

/**
 * A rect stated in an inner box's fractions, restated in the outer box's.
 *
 * One arithmetic for every nesting this module has: a page inside the page
 * area, a cell inside a two-page group. `imageToGroup` above is this with the
 * cell's own box, and is kept as its own name because the call sites read
 * better for it.
 *
 * The output's `w` and `h` need not be equal, even when `rect`'s are — `inner`
 * need not be square — so this is not fit to hold as viewport state on its
 * own, which requires `w === h`. Run it through `fitRect` first.
 */
export function into(rect: Rect, inner: Rect): Rect {
  return {
    x: inner.x + rect.x * inner.w,
    y: inner.y + rect.y * inner.h,
    w: rect.w * inner.w,
    h: rect.h * inner.h,
  }
}

/**
 * The inverse of `into`.
 *
 * A collapsed inner box has no fractions to divide by — an image mid-teardown
 * measures that way — so the rect comes back untouched rather than as `NaN`,
 * which `Viewport` would turn into `scale(NaN)` and a page that vanishes with
 * no error anywhere.
 *
 * `groupToImage` above is this with the cell's own box, for the same reason
 * `into`'s doc gives for `imageToGroup`.
 *
 * Same caution as `into`: the result's `w` and `h` need not match, so this is
 * not fit to hold as viewport state on its own. Run it through `fitRect`
 * first.
 */
export function outOf(rect: Rect, inner: Rect): Rect {
  if (inner.w <= 0 || inner.h <= 0) return rect
  return {
    x: (rect.x - inner.x) / inner.w,
    y: (rect.y - inner.y) / inner.h,
    w: rect.w / inner.w,
    h: rect.h / inner.h,
  }
}

/** One drawn image's natural size, which is all either candidate needs. */
export interface Frame {
  w: number
  h: number
}

/**
 * How a rect on one edition's page becomes a rect on another's.
 *
 * `'reset'` is the baseline the others were measured against, and would have
 * been the fallback had none of them beaten it: throw the view away and show
 * the whole page. A transform that does not beat discarding the zoom is not
 * worth having. Every candidate did — each landed inside 5% of a page against
 * reset's half-page bound — and `mapRect` below carries the measurement that
 * picked between them, the ones it beat, and what each cost.
 */
export type MapKind = 'identity' | 'pillarbox' | 'reset'

/**
 * The same content, named against the other edition's page.
 *
 * **`'identity'`** assumes the same fractions name the same content in both
 * editions, so whatever framing difference exists between them is ignored
 * rather than corrected.
 *
 * **`'pillarbox'`** assumes the shared artwork is as tall as the taller page
 * says it is, and that the wider-aspect page carries its extra as margin at
 * left and right, split evenly between the two edges.
 *
 * **A measurement put it here, not an argument.** `docs/notes.md`, "Which
 * derived transform maps a rect between editions", is the write-up and the
 * authority for every figure below. 288 slots were sampled from twelve corpus
 * volumes across four series — `raw-mono` against `en-mono`, the one edition
 * pair all twelve carry — and for each, a grid search over per-axis scale and
 * offset found the best rect-to-rect map, scored with the solver's own
 * `similarity` rather than with a metric invented for the question. That best
 * map's vertical scale is the identity map to within the search's own final
 * resolution, and its horizontal scale tracks `aFrom / aTo` in twelve volumes
 * of twelve, sign included. Residuals below are how far a candidate puts a
 * point that the best map puts somewhere else, in fractions of the target
 * page, over the 284 slots whose search settled; median then p90, across then
 * down.
 *
 * **Both candidates the design named lost, and this rule was not one of them.**
 * What it beat and what each cost, the winner's own figures first:
 *
 * - `'pillarbox'` leaves 0.0195 / 0.0276 across and 0.0041 / 0.0116 down. It
 *   beats `'identity'` on the horizontal in 10 volumes of 12; the two it does
 *   not are the two whose aspects nearly match already, where there is almost
 *   nothing for either rule to correct.
 * - `'identity'` costs 0.0296 / 0.0405 across for the same 0.0041 / 0.0116
 *   down — exactly right on the vertical, uncorrected on the horizontal.
 * - `'letterbox'` is what this function shipped as until 2026-08-08, and is
 *   renamed rather than kept beside `'pillarbox'` because the measurement found
 *   it wrong and not merely beaten. It took the **max** of the two aspects as
 *   the content's, which is this rule with its axes swapped: it never rescaled
 *   the horizontal, so it inherited `'identity'`'s horizontal residual whole,
 *   and then added a vertical one four times `'identity'`'s at the median
 *   (0.0162 against 0.0041). Correcting nothing on the axis that needed it and
 *   adding error to the axis that did not is worse than doing nothing.
 * - `'reset'` is a bound rather than a residual: the reader sees the whole page
 *   and has to re-find the balloon, so up to half of one. Unmeasured, and it
 *   does not need measuring while every candidate above is inside 5% of a page.
 *
 * **The horizontal finding is the one that could have been an artefact**, and
 * which experiments exclude it is the difference between a result and a number.
 * `fingerprint` resizes every crop to one fixed grid, so a search can buy
 * similarity by squaring two renderings up rather than by locating artwork, and
 * `sx/sy ≈ aFrom/aTo` is exactly what that would look like. Two runs exclude it
 * between them, and neither would do it alone. The same search against a
 * resample of the source page — where the aspect difference is fully present
 * and the true answer is known to be identity — returns `sx/sy = 1.0000` in all
 * twelve volumes, so the search does not chase aspect. And the same search run
 * `en-mono → raw-mono` over the same slots inverts, in twelve volumes of
 * twelve: the two directions' horizontal scales multiply to a median 0.9941,
 * against the 1.0842 they would give if the scale belonged to the metric rather
 * than to the pages.
 *
 * A frame that is non-finite, zero or negative has not decoded yet — a swap
 * can be pressed before the incoming page is ready, and `NaN` is what an
 * image reports before it decodes (see `paintedIn`) — and the honest answer
 * is to leave the rect alone rather than map it through a division that only
 * produces more `NaN`.
 *
 * Like `into`/`outOf`, `'pillarbox'` may return a rect whose `w` and `h`
 * differ even when the input's matched — but not because `from` and `to`
 * carry their margin on different axes. `content` is defined as the *narrower*
 * of the two frames' aspects, so it is no wider than either frame, `ch` is
 * identically 1 for both, and `y`/`h` come out exactly as they went in; the
 * true reason is the plainer one `paintedIn` gives: a pillarbox is exactly the
 * case where one axis is rescaled and the other is not, and here that axis is
 * always the horizontal one. Rescaled, not shrunk — mapping *out* of the frame
 * carrying the margin grows it, which is what the `widens` test asserts. Not
 * fit to hold as viewport state on its own, which requires `w === h`. Run it
 * through `fitRect` first.
 *
 * "Identically 1" is a claim about the arithmetic, and it rests on the guard
 * below: `content` is the smaller of the two aspects, so `aspect / content` is
 * never under 1 for either frame and `Math.min` takes the 1.
 * `naturalWidth`/`naturalHeight` are bounded integers, but the *derived* aspect
 * of a hand-constructed frame need not be finite. `{w: 1e308, h: 1e-10}`
 * overflows to `Infinity` and then hides behind a healthy partner, because
 * `content` takes the smaller of the two — leaving the overflowing frame's own
 * `cw` at zero, which `axis` divides by when that frame is the source and
 * collapses the rect to nothing when it is the target. `{w: 1e-310, h: 1e308}`
 * underflows to zero and becomes `content` itself, making `cw` a `0 / 0` on
 * either side. Each ends in a rect no caller can use, past a guard that checked
 * only the dimensions, so both aspects are checked rather than the comment
 * being softened to describe a hole. Which of the two hides is the half that
 * inverted with the axis: under `max` it was the underflowing frame.
 */
export function mapRect(
  rect: Rect,
  from: Frame,
  to: Frame,
  kind: MapKind,
): Rect {
  if (kind === 'reset') return WHOLE
  if (
    !Number.isFinite(from.w) ||
    !Number.isFinite(from.h) ||
    !Number.isFinite(to.w) ||
    !Number.isFinite(to.h) ||
    from.w <= 0 ||
    from.h <= 0 ||
    to.w <= 0 ||
    to.h <= 0
  )
    // The caller's own object, deliberately, where the `identity` branch below
    // returns a copy. That asymmetry is load-bearing rather than untidy: this
    // is reached from `setView((rect) => mapView(…))`, and React bails out of
    // the re-render on `Object.is`. Returning a fresh but equal rect for a
    // swap pressed before the incoming page decoded would repaint the reader
    // for a gesture that changed nothing. Safe because nothing in this module
    // mutates a rect in place — the property `WHOLE`'s own doc relies on too.
    return rect
  if (kind === 'identity') return { ...rect }

  // Both frames hold the same content at one aspect, centred in each. The
  // content's aspect is the *narrower* of the two, which is what the corpus
  // measured: the shared artwork is as tall as the taller page says it is, and
  // the wider-aspect page carries its extra as margin at left and right. The
  // spec's rule said the opposite — margin on the narrower slot, at top and
  // bottom — and it is the one token below that this function had wrong.
  //
  // Mapping through that shared box — out of the source's fractions, into the
  // content's, out into the target's — is what makes this an **involution**:
  // mapping A to B and back returns the rect it started from, to floating
  // point. A formula that instead fit the source's own aspect into the target
  // is not, and a swap followed by a swap back would walk the view a little
  // further off the page every time.
  // The dimensions being finite does not make either aspect finite, and it is
  // the *frames'* aspects that have to be checked rather than the `content`
  // taken from them: `content` is the smaller of the two, so an overflowing
  // frame hides behind a healthy one and leaves its own `cw` at zero for `axis`
  // to divide by. See this function's doc for both constructions.
  const aFrom = from.w / from.h
  const aTo = to.w / to.h
  if (
    !Number.isFinite(aFrom) ||
    !Number.isFinite(aTo) ||
    aFrom <= 0 ||
    aTo <= 0
  )
    return rect
  const content = Math.min(aFrom, aTo)
  const box = (frame: Frame) => {
    const aspect = frame.w / frame.h
    return {
      cw: Math.min(1, content / aspect),
      ch: Math.min(1, aspect / content),
    }
  }
  const a = box(from)
  const b = box(to)
  const axis = (start: number, size: number, ca: number, cb: number) => ({
    start: (1 - cb) / 2 + ((start - (1 - ca) / 2) * cb) / ca,
    size: (size * cb) / ca,
  })
  const h = axis(rect.x, rect.w, a.cw, b.cw)
  const v = axis(rect.y, rect.h, a.ch, b.ch)
  return { x: h.start, y: v.start, w: h.size, h: v.size }
}

/**
 * One image on screen: its own pixels, which cell of the group draws it, and
 * which part of it the reader's slot addresses.
 *
 * Exactly what `mapView` needs about each side of a swap and nothing more.
 * `frame` is the `<img>`'s `naturalWidth`/`naturalHeight` — every aspect below
 * is derived from it — and `cell` is which drawing position it occupies, which
 * `Cell`'s own doc enumerates. A page has no identity here beyond these three
 * facts: nothing in this module knows what edition or slot it came from, and
 * nothing needs to.
 */
export interface Drawn {
  frame: Frame
  cell: Cell
  /**
   * Which edge this page's artwork is pulled to inside its element.
   *
   * **`mapView` was the fourth consumer of that fact and the one that did not
   * ask**, which is why this field exists. `notes.ts`'s `paintedBox` learned
   * the hug when the seam shipped, and `paintedIn` — which that function's own
   * doc calls its fractional twin — did not, so a swap while zoomed in
   * two-page mode measured both pages as centred while both were hugged to
   * the seam. Single and scroll mode were unaffected: their hug is `centre`,
   * where the two answers coincide, which is exactly why nothing caught it.
   *
   * Optional, defaulting to `centre`, so every caller drawing one page into
   * one box is unchanged.
   */
  hug?: ArtworkHug
  /**
   * Which half of `frame` the slot on screen addresses, when `frame` is a
   * joined spread. `undefined` for an ordinary slot, which is every slot
   * outside a spread.
   *
   * **Not folded into `cell`, and not derivable from `frame`.** `cell` is a box
   * in *group* fractions and this is a box in *image* fractions, with the
   * fit-whole letterbox between them — `halfBox` below says what that costs if
   * the two are confused. And a wide `frame` is not evidence of a spread: a
   * single landscape page has the same shape and addresses all of itself. Only
   * the slot table knows, which is why this is passed in.
   *
   * Independent of `cell` in the arithmetic: `mapView` applies each in its own
   * space, so both being set is not a case. `pairing.ts` draws a joined spread
   * as one wide image in every mode, so what actually reaches here today is a
   * `half` beside `'only'` — but nothing below depends on that.
   */
  half?: 'L' | 'R'
}

/** A frame's width over its height. `NaN` for a frame that has not decoded, which every consumer below already guards. */
const aspectOf = (frame: Frame) => frame.w / frame.h

/** Whether a frame is a real measurement — the same test `mapRect` applies to each of its own two, spelled once for `mapView` to apply to both at the top. */
const decoded = (frame: Frame) =>
  Number.isFinite(frame.w) &&
  Number.isFinite(frame.h) &&
  frame.w > 0 &&
  frame.h > 0

/**
 * The real proportions of one cell's box, given the group's.
 *
 * `fitRect`'s own "real aspect" term (`rect.w * groupAspect / rect.h`) applied
 * to `cellBox`, and it is needed because a page is drawn fit-whole inside its
 * **cell**, not inside the group: `paintedIn` therefore has to be given the
 * cell's aspect, which in `two` mode is the group's halved. For `'only'` the
 * cell *is* the group and this returns `groupAspect` unchanged, which is why a
 * single-page swap needs no special case.
 */
function cellAspect(cell: Cell, groupAspect: number): number {
  const box = cellBox(cell)
  return (box.w * groupAspect) / box.h
}

/**
 * The box one half of a joined spread occupies inside the image that draws
 * both, in that image's own fractions.
 *
 * **`cellBox`'s twin, one space in**, and the distance between the two is why
 * this is a second function rather than more `Cell` values. A cell is a box in
 * group fractions; a half is a box in image fractions; `paintedIn`'s letterbox
 * sits between them.
 *
 * **What confusing the two would cost, worked rather than asserted, because
 * both boxes are centred and the cheap reading is that being centred makes it
 * cancel.** The *fold* would land in the same place. That is the whole of what
 * being centred buys, and nothing else survives it. The corpus's 1.4-wide
 * spread paints at `{x: 0.15, w: 0.7}` of a 2:1 page area, so its left half
 * runs from 0.15 to 0.5 of the cell where a `cellBox`-shaped half would run
 * from 0 to 0.5: the same fold, an outer edge sitting 0.15 of the box out in
 * the background, and every extent 30% short, since the rect would be divided
 * by 0.5 instead of by 0.35. The error is zero at the fold and grows to 0.3 of
 * the half at its outer edge — the worst shape a defect of this kind can have,
 * because a reader zoomed near the fold sees almost nothing wrong.
 *
 * **Left and right are the image's, not the reader's.** `Position.half` is in
 * image coordinates (`crates/taiyaku-core/src/slot.rs`: "Sides are in image
 * coordinates, not reading order", and `docs/notes.md` gives the reason —
 * "that is what a reader has to crop"). Nothing between an image fraction and
 * the screen mirrors: reading direction decides which *slot* is drawn in which
 * cell (`cellsFor`), never which way round an image is painted. So `'L'` is
 * the left half of the pixels and `rtl` does not enter here.
 *
 * **Exactly half, because the corpus measured exactly half.** `docs/notes.md`,
 * "What this means for the schema": every one of the 28 spreads in the corpus
 * is exactly twice the width of its neighbours. That is the same measurement
 * that made `Half` a left/right flag instead of a stored geometry, and the
 * alternative it beat is recorded there — "part *i* of *n*", which a three-page
 * gatefold would need and which no corpus volume has yet asked for. If one
 * ever does, this function and `Drawn.half`'s type are where it lands.
 *
 * `undefined` is `WHOLE`, and that is what makes a slot with no half behave
 * *exactly* as it did before this existed rather than merely closely: `into`
 * and `outOf` against `WHOLE` reduce to `x + f * 1` and `(x - 0) / 1`, which
 * are identities in floating point and not approximations of one.
 */
function halfBox(half: 'L' | 'R' | undefined): Rect {
  if (half === 'L') return { x: 0, y: 0, w: 0.5, h: 1 }
  if (half === 'R') return { x: 0.5, y: 0, w: 0.5, h: 1 }
  return WHOLE
}

/**
 * The natural size of what one slot addresses: the whole image, or half of it.
 *
 * What `mapRect` has to be handed for a slot on half a joined spread, and the
 * reason is `mapRect`'s own contract rather than tidiness — it reconciles two
 * *aspects*, and a joined spread's is twice its half's.
 *
 * **This argument and `halfBox` fix two different halves of one defect**, and
 * they are worth telling apart because each alone still misses. Handed the
 * whole spread's frame, `'pillarbox'` takes the doubling for margin the wider
 * side carries at its edges and rescales by two about the half's own centre:
 * with `halfBox` already applied the reader lands on the *right* half at twice
 * the wrong magnification, everything but the exact centre displaced. Without
 * `halfBox` as well, the same reading maps the spread's central band — the
 * fold — onto the other edition's whole page, which is what `docs/status.md`
 * §5 recorded as the shipped behaviour. Reachable on the primary corpus:
 * `en-mono` joins Go-Toubun v01's page 4 where the raw prints two pages.
 *
 * Derived from `halfBox` rather than written out as `frame.w / 2`, so the
 * frame `mapRect` reconciles and the box the rect is taken out of cannot drift
 * apart — a gatefold would edit `halfBox` and this would follow. For no half
 * both factors are `1`, which is exact.
 */
const halfFrame = (frame: Frame, half: 'L' | 'R' | undefined): Frame => {
  const box = halfBox(half)
  return { w: frame.w * box.w, h: frame.h * box.h }
}

/**
 * The view rect after a swap: the same content, named against the page the
 * other edition draws there.
 *
 * The whole of the spec's §2 in one composition, and each step is one of the
 * functions above:
 *
 * 1. `groupToImage`, `outOf` the outgoing page's painted box, then `outOf` its
 *    own half — group fractions become fractions of the *page*, and then of
 *    the part of it the reader's slot addresses. All three steps are needed and
 *    they are three different insets in three different spaces: which cell of a
 *    two-page group the image occupies, the letterbox `PageView`'s fit-whole
 *    leaves around the image inside that cell, and which half of the image a
 *    joined spread's slot names. Skipping the second is what would make a swap
 *    wrong by exactly that letterbox — `paintedIn`'s own doc says so. Skipping
 *    the third is what put a reader on the fold of a joined spread instead of
 *    on the half they were reading, which `halfFrame`'s doc works through.
 * 2. `mapRect` between the two *slots'* frames — `halfFrame` of each side, the
 *    whole image where there is no half — which is the transform proper.
 * 3. `into` the incoming page's half, then its painted box, then
 *    `imageToGroup` — the inverse of step 1 against the page now on screen.
 * 4. `fitRect` against the page area's aspect **for both arguments**, then
 *    `clampRect`. Group space is the page-area box, so passing that aspect
 *    twice is what restores `Rect`'s `w === h` invariant, which steps 1–3 are
 *    all entitled to break (`into`, `outOf` and `mapRect` each say so at their
 *    own return). It is also the "grown to the viewport's shape" half of §2 —
 *    a swap shows a little extra rather than hiding a sliver.
 *
 * **The half's place in that order is forced, not chosen**, at both of its
 * neighbours. It has to come *after* the painted box on the way in and before
 * it on the way out, because a half is half of the **image** and the painted
 * box is where image fractions begin — put it first and it would be half of
 * the cell, which `halfBox`'s doc costs out (same fold, outer edge 0.15 of the
 * box into the background, every extent 30% short). And it has to come
 * *before* `mapRect` rather than after, because `mapRect` reconciles the two
 * shapes it is handed: taking the half out first is what makes the shape it is
 * handed the half's, which is `halfFrame`'s whole argument.
 *
 * **A slot with no half comes through exactly as it did before halves
 * existed**, not approximately: `halfBox(undefined)` is `WHOLE`, and `into`
 * and `outOf` against `WHOLE` are floating-point identities rather than
 * near-identities — see `halfBox`'s own doc. That is why the two new steps are
 * unconditional here instead of guarded by an `if`, and why the existing
 * `mapView` tests hold unchanged.
 *
 * `'reset'` short-circuits to `WHOLE` rather than falling through the
 * composition. Throwing the view away means fit-whole of the *group*, which is
 * what every mode drew before zoom existed; running `mapRect`'s own `WHOLE` —
 * which is in image fractions — through steps 3 and 4 instead would land on
 * the incoming page's own box, and in `two` mode that is half the group, i.e.
 * a swap that "resets" to a 2× zoom.
 *
 * A page area that has not been laid out has no aspect to fit against, and
 * `fitRect` would return the rect unfitted — a rect whose `w` and `h` may
 * differ, which `Viewport` draws as a stretched page. So this leaves the rect
 * alone instead, the same answer `mapRect` gives for a frame that has not
 * decoded, and for the same reason: the transform genuinely cannot be computed
 * yet.
 *
 * An undecoded frame is checked **here** as well, rather than left to
 * `mapRect`'s own guard. `mapRect` declining leaves the rect alone in *image*
 * fractions, and steps 3 and 4 then run anyway — so a swap pressed before the
 * incoming page decoded would come back not with the rect it started from but
 * with that rect re-read against a page whose painted box is unknown. Leaving
 * the whole composition alone is the answer §9 of the design asks for; the
 * inner guard stays because `mapRect` is called from elsewhere.
 */
export function mapView(
  rect: Rect,
  from: Drawn,
  to: Drawn,
  areaAspect: number,
  kind: MapKind,
): Rect {
  if (kind === 'reset') return WHOLE
  if (!Number.isFinite(areaAspect) || areaAspect <= 0) return rect
  if (!decoded(from.frame) || !decoded(to.frame)) return rect
  const onFrom = outOf(
    outOf(
      groupToImage(rect, from.cell),
      paintedIn(
        aspectOf(from.frame),
        cellAspect(from.cell, areaAspect),
        from.hug,
      ),
    ),
    halfBox(from.half),
  )
  const mapped = mapRect(
    onFrom,
    halfFrame(from.frame, from.half),
    halfFrame(to.frame, to.half),
    kind,
  )
  const onTo = imageToGroup(
    into(
      into(mapped, halfBox(to.half)),
      paintedIn(aspectOf(to.frame), cellAspect(to.cell, areaAspect), to.hug),
    ),
    to.cell,
  )
  return clampRect(fitRect(onTo, areaAspect, areaAspect))
}

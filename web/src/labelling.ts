import { cryptoRandom, ulid } from './ulid'

/** What a labelled box holds. Mirrors `taiyaku-core`'s `Kind` exactly. */
export type Kind =
  | 'dialogue'
  | 'narration'
  | 'sfx'
  | 'logo'
  | 'credits'
  | 'other'

/**
 * A reading direction, or `null` where the question does not apply.
 *
 * A radial sound effect has no reading direction, and forcing a labeller to
 * pick one records a coin toss as data.
 */
export type BoxAxis = 'v' | 'h' | null

export interface LabelBox {
  id: string
  /** Page fractions, matching `TruthRegion`. */
  x: number
  y: number
  w: number
  h: number
  kind: Kind
  axis: BoxAxis
  ignore: boolean
}

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export interface Point {
  x: number
  y: number
}

const clampFraction = (v: number) => Math.min(Math.max(v, 0), 1)

/**
 * One axis of a dragged rectangle, normalised and clamped to the page.
 *
 * `near` is the smaller of the two ends and `extent` the distance between
 * them. Both edges are brought back onto the page: the near edge by clamping
 * it, the far edge by capping what is left of the extent at the page's own
 * remaining width.
 *
 * **Written here rather than calling `notes.ts`'s `regionFromDrag`**, which is
 * the reader's equivalent and does clamp. That one takes the drag's two
 * *points* and recomputes the extent as `max - min`, and a difference of two
 * fractions does not round back to the number that was dragged: routing an
 * on-page box through it turns a stored `w` of `0.3` into
 * `0.30000000000000004`. This form touches the extent only when the drag
 * actually left the page, so a box drawn wholly on it is stored exactly as
 * drawn — across 1,500 boxes that is the difference between a clamp and a
 * quiet rewrite of every label.
 */
function onPage(
  near: number,
  extent: number,
): { near: number; extent: number } {
  const clamped = clampFraction(near)
  return {
    near: clamped,
    // `extent - (clamped - near)` is what is left after the near edge moved,
    // and `1 - clamped` is all the page has left to give.
    extent: Math.min(extent - (clamped - near), 1 - clamped),
  }
}

/**
 * A dragged rectangle as it would be stored: dragged in any direction, with
 * positive extents and both edges on the page. `undefined` for a drag that
 * leaves nothing — one wholly in the letterbox, or one with no extent at all.
 *
 * Its own function so that the live preview drawn *during* a drag is the same
 * arithmetic rather than a second copy of it. A preview computed separately
 * would be a picture of a box the labeller is not about to get: each case
 * below is one where a naive `to − from` and the stored box disagree.
 *
 * **Normalised**, because a drag that runs up and to the left gives negative
 * extents, and a box with a negative width fails every containment test
 * downstream while looking fine in the JSON.
 *
 * **Clamped to the page**, which normalising alone does not do and the
 * labeller once left undone. `PageView` letterboxes the page, so a drag that
 * starts or ends beside the image — natural when boxing a caption that bleeds
 * to the page edge — reads as a fraction outside `0..1`, and an unclamped box
 * stores area that is not on the page at all. `score.rs`'s
 * `classify_single_cover` divides by exactly that area (`inter / (t.w *
 * t.h)`), so an inflated truth box dilutes its own coverage: a genuine 1:1
 * demotes to `Fragment` or `Missed`, and its prediction becomes `spurious`.
 * Both terms of the promotion rule move, in the punishing direction, and
 * nothing anywhere prints a word about it. `pointInSlot`'s own doc already
 * named the rule this restores — "a point in the letterbox reads as outside
 * `0..1`, and `regionFromDrag` is what clamps a whole drag once both ends are
 * known" — and the labeller called nothing of the sort.
 *
 * A drag with no area at all is refused rather than stored: a click that
 * missed, or one that lay wholly in the letterbox and clamped down to nothing.
 */
export function normaliseRect(rect: Rect): Rect | undefined {
  const { near: x, extent: w } = onPage(
    Math.min(rect.x, rect.x + rect.w),
    Math.abs(rect.w),
  )
  const { near: y, extent: h } = onPage(
    Math.min(rect.y, rect.y + rect.h),
    Math.abs(rect.h),
  )
  // `<= 0` rather than `=== 0`: a drag wholly in the letterbox clamps to a
  // negative extent, not a zero one, and a negative width is the very thing
  // the normalisation above exists to keep out of the document.
  if (w <= 0 || h <= 0) return undefined
  return { x, y, w, h }
}

/**
 * A fresh box from a dragged rectangle, or `undefined` when nothing survives
 * normalisation — a drag wholly in the letterbox, or one with no extent.
 *
 * Returns the box rather than an array with it appended, because the caller
 * needs its `id`. A box is selected the moment it is drawn, so the kind and
 * axis keys land on it without a click or a `Tab` first: over ~900 boxes that
 * is one deliberate action per box rather than two, and the two-step version
 * has a failure mode the one-step version cannot have — tagging the box you
 * drew *before* this one.
 */
export interface BoxDefaults {
  kind: Kind
  axis: BoxAxis
}

/**
 * What a box starts as when nothing has been drawn yet.
 *
 * `ignore` is deliberately not among these and must not become one: it excuses
 * a box from scoring, so carrying it forward would silently drop boxes out of
 * the measurement for as long as nobody noticed. Kind and axis are guesses a
 * labeller corrects; `ignore` is a judgement about one box.
 */
export const FIRST_BOX: BoxDefaults = { kind: 'dialogue', axis: 'v' }

export function newBox(
  rect: Rect,
  defaults: BoxDefaults = FIRST_BOX,
): LabelBox | undefined {
  const on = normaliseRect(rect)
  if (!on) return undefined
  return {
    id: ulid(Date.now(), cryptoRandom),
    ...on,
    kind: defaults.kind,
    axis: defaults.axis,
    ignore: false,
  }
}

const edit = (
  boxes: readonly LabelBox[],
  id: string,
  change: (box: LabelBox) => LabelBox,
): LabelBox[] => boxes.map((box) => (box.id === id ? change(box) : box))

export const setKind = (
  boxes: readonly LabelBox[],
  id: string,
  kind: Kind,
): LabelBox[] => edit(boxes, id, (box) => ({ ...box, kind }))

export const setAxis = (
  boxes: readonly LabelBox[],
  id: string,
  axis: BoxAxis,
): LabelBox[] => edit(boxes, id, (box) => ({ ...box, axis }))

export const toggleIgnore = (
  boxes: readonly LabelBox[],
  id: string,
): LabelBox[] => edit(boxes, id, (box) => ({ ...box, ignore: !box.ignore }))

export const removeBox = (boxes: readonly LabelBox[], id: string): LabelBox[] =>
  boxes.filter((box) => box.id !== id)

const holds = (box: LabelBox, at: Point): boolean =>
  at.x >= box.x &&
  at.x <= box.x + box.w &&
  at.y >= box.y &&
  at.y <= box.y + box.h

/**
 * Every box containing the point, topmost first.
 *
 * Later boxes draw on top of earlier ones, so the array is reversed: what a
 * reader sees on top is what a click should reach first.
 */
export const boxesAt = (boxes: readonly LabelBox[], at: Point): LabelBox[] =>
  boxes.filter((box) => holds(box, at)).reverse()

/**
 * The next box down the stack under the cursor, wrapping at the bottom.
 *
 * Sound effects routinely overlap balloons and panel borders, so a click that
 * always takes the topmost box makes the one underneath unreachable. This is
 * what replaces a sidebar list: the boxes stay on the image and Tab walks
 * through the ones that overlap.
 *
 * A selection that is not under the cursor starts the walk over rather than
 * answering `undefined` — the cursor has moved to a different stack, and the
 * previous selection says nothing about where in this one to begin.
 */
export function cycleAt(
  boxes: readonly LabelBox[],
  at: Point,
  selected: string | undefined,
): string | undefined {
  const under = boxesAt(boxes, at)
  if (under.length === 0) return undefined
  const index = under.findIndex((box) => box.id === selected)
  if (index === -1) return under[0].id
  return under[(index + 1) % under.length].id
}

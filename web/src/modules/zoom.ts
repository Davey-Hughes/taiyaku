import type { ReaderModule } from '../module'
import {
  panBy,
  wheelBy,
  WHOLE,
  zoomAt,
  type MapKind,
  type Rect,
} from '../viewport'

export const IN_INTENT = 'zoom:in'
export const OUT_INTENT = 'zoom:out'
export const WHOLE_INTENT = 'zoom:whole'

/**
 * Zoom and pan, and what a swap does with them.
 *
 * **Declared ahead of its own experiment, and the experiment has since run.**
 * `onSwap: 'content'` means "map through whichever derived transform wins" —
 * `viewport.ts`'s `mapRect` — and this module landed before the corpus
 * measurement scheduled as Task 8 of
 * `docs/superpowers/plans/2026-08-07-l1-geometry.md` had said which that was.
 * `'content'` was the right default even then, on the design's own promotion
 * rule (`docs/superpowers/specs/2026-08-07-l1-geometry-design.md` §4):
 * `'reset'` is what ships if no candidate beats throwing the view away, not a
 * default to assume ahead of a result.
 *
 * The result, `docs/notes.md`'s "Which derived transform maps a rect between
 * editions", left this line alone and changed the one it points at: neither
 * candidate the design named won, `mapRect`'s `'pillarbox'` did, and every one
 * of the three beat throwing the view away. So `'content'` is now a
 * measurement rather than a bet, and `mapRect`'s doc carries the figures. A
 * reader who disagrees with the default can still say so.
 *
 * **`maxScale` and `step` are guesses, not measurements.** 8x on a
 * 1200px-tall page puts a single line of dialogue across the viewport, which
 * is past what anyone has asked for and short of a scale where the source
 * pixels stop meaning anything; 1.15 is about five wheel notches to double
 * (1.15⁵ ≈ 2.011). Both are worth revisiting against a reader rather than
 * against a number.
 *
 * `0` is safe to claim and cannot become an edition jump later: `keys.ts`'s
 * `READER_KEYS` answers `1`..`9` and deliberately leaves `0` alone, and says
 * so where it is declared, so a tenth edition would need a different key
 * regardless of what this module does. *This cited `intentOf`'s `/^[1-9]$/`
 * until 2026-08-08, when that test became a table row.*
 */
export interface ZoomPrefs {
  enabled: boolean
  maxScale: number
  step: number
  onSwap: 'content' | 'reset'
}

export const zoomModule: ReaderModule<ZoomPrefs> = {
  id: 'zoom',
  label: 'Zoom',
  defaults: {
    enabled: true,
    maxScale: 8,
    step: 1.15,
    onSwap: 'content',
  },
  options: { onSwap: ['content', 'reset'] },
  keys: [
    { key: '=', intent: IN_INTENT, label: 'zoom in' },
    { key: '-', intent: OUT_INTENT, label: 'zoom out' },
    { key: '0', intent: WHOLE_INTENT, label: 'fit the whole page' },
  ],
}

/**
 * Which of `viewport.ts`'s derived transforms `onSwap: 'content'` maps
 * through.
 *
 * **A result, and it said this line was wrong.** It read `'letterbox'` until
 * 2026-08-08, on the guess that a letterbox "is centred by construction" — the
 * rule that took Tongari v09 from 98 runs to 7 — and that framing differences
 * between packagings arise the same way. The corpus says the reasoning was
 * sound about letterboxing and wrong about these editions: the shared artwork
 * is as tall as the taller page, and the wider one carries margin at its
 * sides, not its top and bottom.
 *
 * `docs/notes.md`, "Which derived transform maps a rect between editions", is
 * the measurement — 288 slots over twelve corpus volumes, `raw-mono` against
 * `en-mono` — and `mapRect`'s own doc carries the figures beside the
 * arithmetic. What `'pillarbox'` beat, in order: `'letterbox'`, which corrects
 * nothing on the axis that needs it and adds four times the error on the axis
 * that does not, so it loses to doing nothing at all; `'identity'`, which is
 * exactly right on the vertical and uncorrected on the horizontal, where
 * `'pillarbox'` cuts the median residual from 0.0296 of page width to 0.0195;
 * and `'reset'`, a half-page bound rather than a residual. None of the three
 * is what `mapView` corrects most visibly — that is the fit-whole letterbox
 * *inside* the page area, which `paintedIn` removes whichever one is chosen.
 *
 * Still the seam rather than the decision: had no candidate beaten throwing
 * the view away, the design's §4 promotion rule ships reset-on-swap, which is
 * a change to `defaults.onSwap` above and not to this constant.
 */
const CONTENT_MAP: MapKind = 'pillarbox'

/** A rect's own centre — where a keyboard zoom anchors, having no pointer to anchor on. */
export function centreOf(rect: Rect): { x: number; y: number } {
  return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 }
}

/**
 * A box, as much of a `DOMRect` as any of this needs.
 *
 * Declared rather than taking `DOMRect`, so nothing below reads a DOM type and
 * a test hands in an object literal. The four fields are what a pointer
 * anchor needs; `panStep` takes only the two sizes, because a pan is a delta
 * and never asks where the box is.
 */
export interface Box {
  left: number
  top: number
  width: number
  height: number
}

/**
 * Where a client point falls over the group, in the rect's own fractions.
 *
 * `undefined` for a zero-sized box — an element mid-teardown, not a viewport.
 * Dividing by it puts `NaN` into the anchor, which `zoomAt` propagates into
 * the rect, `Viewport` turns into `scale(NaN)`, and the reader sees the page
 * vanish with no error anywhere. This is the boundary where DOM numbers enter
 * the arithmetic, so it is the place to stop them rather than hardening every
 * pure function downstream against a caller that should not exist.
 */
export function anchorIn(
  rect: Rect,
  box: Box,
  at: { x: number; y: number },
): { x: number; y: number } | undefined {
  if (box.width <= 0 || box.height <= 0) return undefined
  return {
    x: rect.x + ((at.x - box.left) / box.width) * rect.w,
    y: rect.y + ((at.y - box.top) / box.height) * rect.h,
  }
}

/**
 * Whether the rect can be moved at all right now: the module is on **and**
 * something is drawn to move it over.
 *
 * One predicate, because it is asked in four places that must agree — the
 * three zoom keys and the pan press. `drawn` rather than `mode !== 'scroll'`
 * for the reason the press's own guard gives: what the page area is drawing is
 * already absent in scroll mode *and* in a gap, where `Gap` renders inside the
 * page area and would otherwise pan a rect nobody is drawing.
 *
 * This is `notes.ts`'s `pressTakesPan`'s `zoomable` field, and the two must
 * stay one rule: the *decision* to pan is part of what a press on the page
 * means, which is that file's subject, and the *arithmetic* of a pan frame is
 * `panStep` below. Sharing this predicate is what keeps them from drifting.
 */
export function zoomable(prefs: ZoomPrefs, drawn: boolean): boolean {
  return prefs.enabled && drawn
}

/**
 * `=`, `-`, `0`. The rect back unchanged where the module is off.
 *
 * A keyboard zoom has no pointer to anchor on, so it anchors on the rect's own
 * centre — the one case where `zoomAt`'s doc's "zooming about the centre feels
 * broken" does not apply, because there is no point the reader is aiming at
 * for it to slide away from.
 *
 * The `enabled` check is **not** made redundant by `zoomable` above, and is
 * named here so nobody deletes it as dead: `zoomable` asks a second question
 * as well (is anything drawn), so it is a different predicate, and this is the
 * one that keeps a switched-off module from writing a zoomed rect whatever a
 * caller forgot to ask first.
 *
 * `'whole'` hands back `WHOLE` itself rather than a copy of it, so a `0` press
 * at fit-whole is `Object.is`-equal to the state already held and React bails
 * out of the re-render — the same saving `panStep` below makes deliberately.
 */
export function zoomStep(
  rect: Rect,
  which: 'in' | 'out' | 'whole',
  prefs: ZoomPrefs,
): Rect {
  if (!prefs.enabled) return rect
  if (which === 'whole') return WHOLE
  return zoomAt(
    rect,
    centreOf(rect),
    which === 'in' ? prefs.step : 1 / prefs.step,
    prefs.maxScale,
  )
}

/**
 * A wheel notch or a trackpad pinch, anchored where the pointer is.
 *
 * `undefined` where the box is unusable, which the caller reads as "leave the
 * view alone" — see `anchorIn` for what dividing by a zero box costs.
 *
 * Proportional to the delta and normalised across `deltaMode` first;
 * `wheelBy`'s doc has that rule and names every constant it guesses at. What
 * this replaced was `deltaY < 0 ? step : 1 / step`, a full step per event
 * whatever its size, which at the defaults let a single trackpad flick pin the
 * view at `maxScale`.
 */
export function wheelStep(
  rect: Rect,
  wheel: { deltaY: number; deltaMode: number; ctrlKey: boolean },
  at: { x: number; y: number },
  box: Box,
  prefs: ZoomPrefs,
): Rect | undefined {
  const anchor = anchorIn(rect, box, at)
  if (!anchor) return undefined
  return zoomAt(rect, anchor, wheelBy(wheel, prefs.step), prefs.maxScale)
}

/**
 * One pan frame: the rect after the pointer moved from `from` to `to`.
 *
 * A drag moves the page under the viewport, so the rect moves the other way —
 * dragging right shows what was to the left. The step is scaled by
 * `rect.w`/`rect.h` because the rect is what one page area's width is
 * showing: at 4x, a pointer crossing the box has crossed a quarter of the
 * group.
 *
 * **The *same* rect back where the pan had nowhere to go**, so React's own
 * `Object.is` bail-out applies. `panBy` says a drag at fit-whole is "a no-op
 * instead of a jitter", and it is the *rect* that is one: without this the
 * no-op still allocates a fresh, equal rect on every pointer move, and every
 * one of those re-renders the reader and re-runs `PageAnnotations`'
 * measurement. Fit-whole is what a reader who never zooms is in the whole
 * time, so that is the common case rather than an edge of one. Only `x` and
 * `y` are compared because `panBy` is the only writer here and it never
 * touches the sizes.
 */
export function panStep(
  rect: Rect,
  box: { width: number; height: number },
  from: { x: number; y: number },
  to: { x: number; y: number },
): Rect {
  const next = panBy(
    rect,
    (-(to.x - from.x) / box.width) * rect.w,
    (-(to.y - from.y) / box.height) * rect.h,
  )
  return next.x === rect.x && next.y === rect.y ? rect : next
}

/** Which derived transform a swap maps the view through. */
export function swapMapKind(prefs: ZoomPrefs): MapKind {
  return prefs.onSwap === 'reset' ? 'reset' : CONTENT_MAP
}

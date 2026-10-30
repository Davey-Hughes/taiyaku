/**
 * What a *finished* press on the reader's page area means.
 *
 * The rule lives here rather than in `App.tsx` for the reason `notes.ts`'s
 * `gestureFor` gives for the same split: this is where it can be tested.
 * `App.tsx` reads the DOM for it — the page area's box, where the press went
 * down and came up, which panels are open — and carries out the answer.
 *
 * Named for gestures rather than for touch because a mouse answers these too
 * (spec 2026-09-17 §9.1): a click is a press that moved nowhere, and the
 * browser project drives every rule below without emulating a finger.
 *
 * **What this does not decide** is anything a press does while it is still
 * down. Panning is `pressTakesPan`'s; drawing a region, moving one and
 * finishing a note are `gestureFor`'s; the wheel and the trackpad pinch it
 * carries are `App.tsx`'s own listener's. A press one of them has already
 * spent arrives here marked `consumed`.
 */

/** Where and when a press was, in the page area's own pixels and in milliseconds. */
export interface Press {
  /** Pixels from the page area's left edge, not the viewport's. */
  x: number
  /** Pixels from the page area's top edge. */
  y: number
  /** `event.timeStamp`, which jsdom and a real browser both fill. */
  t: number
}

/** Everything the rule reads. `App.tsx` supplies all of it from the DOM and from state. */
export interface PressReading {
  down: Press
  up: Press
  /** The page area's own width. Thirds are of this, not of a page cell. */
  width: number
  /** The press began on a control — `keys.ts`'s ancestor-aware `isInteractivePress`. */
  onControl: boolean
  /** Something already acted on this press while it was down. */
  consumed: boolean
  /** The note editor is open: `panelMode === 'edit'`. */
  editing: boolean
  /** Help, Settings, credits or the note review panel is open. */
  panelOpen: boolean
  /** Scroll mode, where scrolling is the turn. */
  scrolling: boolean
  /** The view is the whole group — `viewport.ts`'s `isWhole`, by value. */
  atWhole: boolean
}

/** What the press turned out to mean. `by` is *physical*, before `stepFor`. */
export type PageGesture =
  | { kind: 'none' }
  | { kind: 'turn'; by: 1 | -1 }
  | { kind: 'toggle-chrome' }
  | { kind: 'close-panels' }

/**
 * How far a press may travel and still be a tap, in CSS pixels.
 *
 * These four numbers are **chosen, not measured** (spec §9.2). A thumb on a
 * phone is what says whether 10 px is too tight to hold still for and whether
 * 40 px is far enough to be deliberate; a figure that changes is changed here
 * and in §9.2 together.
 */
export const TAP_SLOP = 10

/** How long a press may last and still be a tap, in milliseconds. */
export const TAP_HOLD = 500

/** How far a drag must travel across the page to turn it, in CSS pixels. */
export const SWIPE_MIN = 40

/** How much more across than down a swipe must be. */
export const SWIPE_RATIO = 2

const NONE: PageGesture = { kind: 'none' }

export function gestureOfPress(press: PressReading): PageGesture {
  // A press that began on a control was never this rule's, and one already
  // spent must not act twice: `gestureFor`'s `close` fires on `pointerdown`,
  // so without this the release of the tap that shut the note editor would
  // fall through below and turn a page as well.
  if (press.onControl || press.consumed) return NONE

  const dx = press.up.x - press.down.x
  const dy = press.up.y - press.down.y
  const tapped =
    Math.abs(dx) <= TAP_SLOP &&
    Math.abs(dy) <= TAP_SLOP &&
    press.up.t - press.down.t <= TAP_HOLD
  if (!tapped) return swipe(press, dx, dy)

  // Region drawing's gesture, which §5 ④ forbids taking. The editor's own
  // exits are `Escape`, its Close button, and `gestureFor`'s letterbox rule.
  if (press.editing) return NONE
  // One tap returns to the page whatever is over it, and turns nothing — so
  // there is no precedence between panels to invent.
  if (press.panelOpen) return { kind: 'close-panels' }
  // Scrolling is already the turn in scroll mode, so the whole page area is
  // the middle third there.
  if (press.scrolling) return { kind: 'toggle-chrome' }
  // The pan's zero-box guard, for the same reason: a box measured mid-teardown
  // divides the thirds into nothing. `NaN` fails this comparison too.
  if (!(press.width > 0)) return NONE

  const third = press.width / 3
  // Where the press went *down* decides, not where it came up: a tap has
  // barely moved, and the finger's landing is what the reader aimed.
  if (press.down.x < third) return { kind: 'turn', by: -1 }
  if (press.down.x >= press.width - third) return { kind: 'turn', by: 1 }
  return { kind: 'toggle-chrome' }
}

/**
 * A drag that was not a tap.
 *
 * Only at fit-whole, where `panStep` hands the same rect back and `panBy`'s
 * own doc calls the drag "a no-op instead of a jitter" — so the swipe is
 * carved out of a press the pan already discards rather than taken from it.
 * Zoomed in, every drag stays the pan's.
 *
 * No time limit: a slow drag at fit-whole moved nothing either, and reading it
 * as a turn costs nothing.
 *
 * **Each guard below refuses a different thing, and the order is why.** Scroll
 * mode's vertical drag never reaches the axis test: `press.scrolling` refuses
 * it on the first line. A slip that barely moves across the page — a finger
 * lifting from the page onto a bar, say — is refused by `SWIPE_MIN`, before
 * the axis test too. What the axis test is actually for is a drag that has
 * already travelled `SWIPE_MIN` or more across and is **less than twice as
 * wide as it is tall** — `SWIPE_RATIO` is 2, so a 50×30 drag is refused
 * although it is wider than it is tall: the cut is at about 26.6° from the
 * horizontal, not at 45°.
 */
function swipe(press: PressReading, dx: number, dy: number): PageGesture {
  if (press.editing || press.panelOpen || press.scrolling || !press.atWhole)
    return NONE
  if (Math.abs(dx) < SWIPE_MIN) return NONE
  if (Math.abs(dx) < SWIPE_RATIO * Math.abs(dy)) return NONE
  // Dragging leftwards brings in what is on the right: the physical `→`.
  return { kind: 'turn', by: dx < 0 ? 1 : -1 }
}

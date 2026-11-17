import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type RefObject,
} from 'react'
import { thumbUrl } from './api'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { stepFor, type Direction } from './modules/direction'
import { stripTileEdge } from './tileSize'
import {
  focusAtFraction,
  type GlideSample,
  glideSpeed,
  glideTravel,
  medianTileWidth,
  normalizedTileWidth,
  refinedStripUnit,
  releaseSpeed,
  shownShare,
  spacerWidth,
  STRIP_DRAG_SLOP,
  STRIP_GLIDE_MIN_SPEED,
  STRIP_GLIDE_MS,
  STRIP_GLIDE_SAMPLE_MS,
  stripWheel,
  stripWindow,
  tileMinWidth,
  tilePages,
  tileUnits,
  unitsBetween,
  type RailShare,
} from './tiles'

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

// Longhand `border*` properties throughout, deliberately, rather than the
// `border` shorthand: React warns when an element's style toggles between a
// shorthand and a longhand override of the same property across renders
// (exactly what happens here as a tile gains and loses `currentTileStyle`),
// so both styles stick to the same longhand set.
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

const currentTileStyle: CSSProperties = {
  ...tileStyle,
  borderColor: 'var(--accent)',
}

// The `R`/`L` mark on one half of a split spread. Bottom-left rather than
// centred, so it never covers the part of a panel a reader is comparing, and
// high-contrast because it sits on artwork of unknown brightness: the
// reader's panel and text, from `.reader-half-badge` in `styles/reader.css`.
const halfBadgeStyle: CSSProperties = {
  position: 'absolute',
  bottom: 2,
  left: 2,
  padding: '0 4px',
  borderRadius: 3,
  fontSize: 10,
  lineHeight: '14px',
  fontWeight: 600,
  pointerEvents: 'none',
}

const resolvedTileStyle: CSSProperties = {
  ...tileStyle,
  // Dashed, not solid: the solid border already means "where you are", and a
  // reader choosing a counterpart has to tell "where the alignment says this
  // lands" apart from it at a glance. Same longhand-only rule as above.
  borderColor: 'var(--warning)',
  borderStyle: 'dashed',
}

/**
 * How many tiles the rail builds either side of the anchor the window is
 * centred on.
 *
 * The scroll handler below re-anchors once the *rendered* tiles run out
 * against the visible rail, read straight from the DOM (see that handler's
 * own doc). `windowMargin` has to be wide enough that the rendered tiles reach
 * past whatever the viewport shows, because the rebuild that check triggers
 * takes a frame to land while the reader keeps scrolling.
 *
 * 16 rather than some smaller number: `focusAtFraction` returns the tile at
 * the rail's *leading* edge for the current scroll fraction, and the
 * viewport extends about a dozen tiles forward from there at the widest
 * supported width (1440 px, over a tile's nominal outer width and the gap
 * beside it: `--strip-unit`, two borders and `--strip-gap`). 16 leaves a few
 * tiles of slack over the reach the viewport has, kept as a round number
 * rather than shaved to the exact minimum.
 *
 * A larger window per rebuild is paid for by far fewer rebuilds than
 * rebuilding on every scroll frame would cost.
 */
const windowMargin = 16

// A spacer stands in for the tiles outside the window, at exactly their
// summed width, so every rendered tile keeps the position it would have had
// and `scrollIntoView` still lands on it. `aria-hidden`, and not a button:
// it is geometry, not a page, and a screen reader walking the strip should
// find only tiles.
const spacerStyle: CSSProperties = { flex: '0 0 auto', height: '100%' }

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
// its size.
const tileImgStyle: CSSProperties = {
  height: '100%',
  width: 'auto',
  objectFit: 'contain',
  display: 'block',
  pointerEvents: 'none',
}

/**
 * What a tile shows, for the hover preview.
 *
 * While correcting, `correctionTiles` gives every group exactly one slot, so
 * this is that slot's page and half. `tilePages` is what already maps a group
 * onto its page indices, and reusing it is what keeps the preview naming the
 * same page the tile is drawing rather than a second derivation that could
 * disagree.
 */
function previewOf(
  group: Group,
  slots: Position[],
): { page: number; half?: 'L' | 'R' } | undefined {
  const [first] = tilePages(group, slots)
  return first && { page: first.page, half: first.half }
}

/**
 * What the progress line asks of the rail while a mouse drags it as a
 * scrollbar (O286).
 */
export interface StripScroller {
  /**
   * Stands the rail `along` its travel, 0 at the start of the volume and 1 at
   * its end, reckoned from the rail's widths as they are now. Stops a glide
   * first: its next frame would write the rail back to where it had it going.
   */
  scrollAlong: (along: number) => void
}

export interface FilmstripProps {
  /**
   * Which way the rail runs.
   *
   * A right-to-left work is read right-to-left, and a strip that ran the other
   * way would put page 1 on the left while the pages beside it put page 1 on
   * the right. A reader hunting a page would have to read the strip backwards
   * from the reader.
   *
   * `row-reverse` rather than reversing the array, so a tile's index means the
   * same in both directions — `stepToGroup`, the current-tile border and the
   * correction picker all address tiles by index, and reversing the data would
   * make every one of them wrong in a way no type could catch.
   */
  reading: Direction
  work: string
  /** The edition being viewed — tiles show its pages, not the authority's. */
  edition: string
  /** Every slot of `edition`, in order — the same table the page renderer resolves from. */
  slots: Position[]
  /**
   * `groups(slots, mode, reading)` while reading — reused, not re-derived, so
   * the tiles pair exactly as the page view does. While `correcting` is set,
   * this is `pairing.ts`'s `correctionTiles(slots)` instead: one tile per
   * slot regardless of `mode`, so any slot (including one half of a joined
   * spread) can be picked on its own — see `correcting`'s own doc below.
   */
  groups: Group[]
  /** Index into `groups` of the group the reader is currently looking at. */
  currentIndex: number
  /**
   * A slot of `edition` to centre the rail on when nothing is current.
   *
   * Set whenever the viewed edition has no slot for the reader's position at
   * all — a gap, or the borrowed page `modules/borrow.ts` draws in place
   * of one — where there is no current tile to centre on, and the nearest slot
   * that does resolve is the closest thing to "where the reader is". With that
   * module at its default the borrowed case is the ordinary one and the gap
   * the rare one, so reading this as "only in a gap" would describe the
   * exception. `App.tsx` passes `nearestPaired`'s `before?.at ?? after?.at`,
   * which is the same page `Gap`'s own `←` button offers.
   *
   * Centring only. The anchor tile gets **no border**: the solid one means
   * "where you are" and the dashed one means "where the alignment says this
   * lands", and in either state neither is true — the position lands nowhere
   * in this edition, which is what made it a gap and what a borrowed page has
   * not changed. Marking it either way would put a claim on screen in the one
   * state that exists to say the claim cannot be made.
   */
  anchor?: number
  /** Real navigation — calls through to `reading.ts`'s `navigate`, never `swap`. */
  onNavigate: (slot: number) => void
  /**
   * Whether `Chrome` currently shows this strip at all. Optional because
   * `Chrome` unmounts this component entirely while hidden (`if (!visible)
   * return null`) rather than hiding it in place — so the mount that follows
   * already re-runs the effect below on its own. Threaded through anyway and
   * included in that effect's dependencies, so the behaviour still holds if
   * `Chrome` ever switches to hiding this in place (e.g. for a transition)
   * instead of unmounting it.
   */
  visible?: boolean
  /**
   * Set while the reader is choosing a counterpart. The strip then shows
   * `to`'s tiles rather than the edition being read, with the slot the
   * alignment currently resolves to marked — which is the thing being
   * disagreed with, so it has to be visible to disagree with it.
   *
   * `pairedIndex` is a *slot* number within `to` (`Resolver.indexOf`'s own
   * unit), not a `groups` array index — this component is what maps it onto
   * a tile, via `group.slots.includes(pairedIndex)`, the same way it already
   * maps `slots`/`tilePages` in slot space. `App.tsx` supplies `edition`,
   * `slots` and `groups` themselves for `to` while this is set; nothing here
   * fetches a second edition's data on its own.
   *
   * `groups` itself is always `correctionTiles`' one-slot-per-tile shape
   * while this is set (see that prop's own doc) — never a two-slot spread or
   * `two`-mode pair. That is what makes `group.slots.includes(pairedIndex)`
   * exact rather than merely "the tile containing it": with a one-slot
   * `group`, "contains" and "is" are the same test, so the dashed marker
   * below always names the one slot the alignment actually resolves to, and
   * `onPickCounterpart(group.slots[0])` always asserts that same slot back.
   */
  correcting?: { to: string; pairedIndex?: number }
  /** A tile click while `correcting` — the assertion, not navigation. */
  onPickCounterpart?: (slot: number) => void
  /**
   * The tile under the pointer, while `correcting` — `undefined` when the
   * pointer leaves one.
   *
   * Reported upward rather than previewed here because the preview has to
   * escape this strip's own `overflow: hidden`; `Chrome` renders it as a
   * sibling. A tile is drawn at most 160px tall (`tileSize.ts`'s
   * `STRIP_TILE_PX`), which is enough to recognise a page and not enough to
   * decide whether two drawings are the same drawing — which is exactly what
   * a correction asks.
   */
  onPreview?: (tile: { page: number; half?: 'L' | 'R' } | undefined) => void
  /**
   * Which stretch of the volume the rail is showing (O216): told once the
   * strip has mounted, and after that whenever it has changed. `Chrome`
   * draws it on the progress line, since the rail draws no scrollbar of its
   * own.
   */
  onShown?: (shown: RailShare) => void
  /**
   * Whether a drag let go while the pointer is moving carries the rail on
   * (O216). True where it is not given. `Chrome` passes the reader's own
   * animation setting. Whether the system asks for reduced motion is not
   * the caller's to say: the strip asks that itself, at each release.
   */
  glides?: boolean
  /**
   * Filled with this strip's `StripScroller` while it is mounted, and
   * emptied when it goes. `Chrome` drags the rail from the progress line
   * through it.
   */
  scroller?: RefObject<StripScroller | null>
}

/**
 * One tile per group, laid out as `groups` paired them: while reading, a
 * pair reads as a pair and a joined spread as one wide tile; while
 * `correcting`, every group is exactly one slot (`pairing.ts`'s
 * `correctionTiles` — see the `groups` and `correcting` props' own docs), so
 * every tile addresses one slot and nothing more. The current group is
 * marked with `aria-current` and a solid border; the group
 * `correcting.pairedIndex` names gets a dashed one instead — see
 * `resolvedTileStyle`. Regroup (dragging a tile to fix a pairing) is absent
 * — a later milestone's job.
 *
 * The rail is `overflowX: auto` with one tile per group, so the current one
 * can be off-screen when the chrome opens — and, since its thumbnail is
 * lazy-loaded, not even requested. `currentRef` is attached to whichever tile
 * is current and centred into view whenever that changes or the strip becomes
 * visible. `pairedRef` plays the same role for the dashed tile, and
 * `anchorRef` for the neighbourhood of a position this edition has no slot
 * for. At most one of the three is ever attached at a time — `App.tsx` sets
 * `anchor` only in a gap or on the borrowed page that stands in for one,
 * where `currentIndex` is `-1` and nothing is correcting, and `isAnchor`
 * below re-tests that rather than assuming it — so centring on "whichever is
 * attached" is exactly the tile a reader needs on screen for the state they
 * are in: the one to disagree with while correcting, and the nearest page
 * that exists while standing where this edition has none.
 */
export function Filmstrip({
  reading,
  work,
  edition,
  slots,
  groups,
  currentIndex,
  anchor,
  onNavigate,
  visible,
  correcting,
  onPickCounterpart,
  onPreview,
  onShown,
  glides = true,
  scroller,
}: FilmstripProps) {
  const currentRef = useRef<HTMLButtonElement>(null)
  const pairedRef = useRef<HTMLButtonElement>(null)
  // Third and last in the chain below. At most one of the three is ever
  // attached, and `isAnchor` further down is what enforces that here rather
  // than trusting `App.tsx` to: an anchor is honoured only while
  // `currentIndex` is `-1` and nothing is `correcting`, so a caller that set
  // `anchor` alongside a real position would lose the anchor rather than
  // attach two refs in one render. Centring on "whichever is attached"
  // therefore always names the one tile a reader needs on screen for the
  // state they are in.
  const anchorRef = useRef<HTMLButtonElement>(null)
  const railRef = useRef<HTMLDivElement>(null)

  // Set by the centring effect when a position change lands with focus on
  // the strip; the effect after it moves focus to the tile now standing for
  // that position (spec 2026-09-26 §3.2). A ref and a second effect, rather
  // than a `focus()` inside the first, because the new tile may not be
  // rendered yet — the same reason `pendingRecentre` retries.
  const followFocus = useRef(false)

  // O134. The rung every tile in this strip asks the server for, read once
  // at mount.
  //
  // **Lazy state rather than an effect**, so the very first render already
  // carries it: setting it afterwards would mean every visible tile fetched at
  // 160 px and then again at its real rung, which is the whole cost this row
  // is trying to remove doubled.
  //
  // **Once per mount, and not on resize.** `devicePixelRatio` changes when a
  // window moves between monitors and the layout changes when one is dragged
  // into or out of the phone layout; re-reading either would refetch every
  // tile in the window to sharpen a strip nobody is looking at mid-drag.
  //
  // A mount is not a reading session: `Chrome` returns `null` while hidden, so
  // this strip unmounts on every chrome hide and reads the rung again on the
  // next show. (The mount-once candidate is O143, which the user declined.)
  // That costs nothing: the rung is a pure function of the viewport and the
  // density, so a re-read on an unchanged screen yields the same URLs and the
  // browser cache answers them. What it does mean is that a reader who hides
  // the chrome, drags the window out of the phone layout and shows it again
  // *does* get the right rung — the stale case this comment guards against is
  // narrower than it looks, and is only ever within one showing of the bars.
  const [tileEdge] = useState(() =>
    stripTileEdge(typeof window === 'undefined' ? undefined : window),
  )

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
  // Named `primaryFocus` rather than `focus` because `focus` below is the
  // choice between this and where the reader has scrolled to.
  const primaryFocus =
    currentIndex >= 0
      ? currentIndex
      : pairedFocus >= 0
        ? pairedFocus
        : anchorFocus

  // Where the reader has scrolled to, as a tile index, or `undefined` while
  // they have not scrolled away from the tile the rail centred on.
  //
  // Cleared whenever the centring effect below runs, so a page turn or a new
  // correction always takes the window back to the tile that state is about
  // rather than leaving it wherever a previous flick left it.
  const [scrolledTo, setScrolledTo] = useState<number>()

  const units = groups.map(tileUnits)
  const focus = scrolledTo ?? primaryFocus
  const { lo, hi } = stripWindow(focus, groups.length, windowMargin)

  // Set whenever the tile to centre on may have moved out of the *rendered*
  // window — cleared once `recenter` (below) actually finds a ref to scroll
  // to. See the two effects that follow for why a single synchronous
  // attempt is not enough.
  const pendingRecentre = useRef(false)

  // Stops a glide that a let-go drag has left carrying the rail on. The drag's
  // effect further down puts its own in here.
  const stopGlide = useRef(() => {})

  // True while that effect hands focus back to a tile of the rail, for the
  // `focusin` listener to read: see `giveFocusBack`.
  const handingFocusBack = useRef(false)

  // `currentRef ?? pairedRef ?? anchorRef`, scrolled into view if one of
  // them is attached. Returns whether it found one, which is what lets the
  // effects below tell "nothing to scroll to yet" apart from "already
  // done". A glide under way is stopped first: its next frame would write
  // the rail back to where the glide had it going.
  const recenter = () => {
    const target = currentRef.current ?? pairedRef.current ?? anchorRef.current
    if (!target) return false
    stopGlide.current()
    target.scrollIntoView({ inline: 'center', block: 'nearest' })
    return true
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: `currentIndex`, `visible`, `correcting?.pairedIndex` and `anchor` are triggers, not closure reads — the body reaches the tile through refs. Removing them, which is what this rule suggests, would centre the rail once and never re-centre it. `scrolledTo` is read below (finding D) but deliberately left out too: it changes on nearly every scroll frame and every `Tab` press, and this effect exists to run only on an actual position change, not on those — the same reasoning `unitsRef` further down already follows for `units`.
  useEffect(() => {
    // Read before anything below can move focus or rebuild the window: the
    // tile that had focus is still in this render's DOM.
    followFocus.current =
      railRef.current?.contains(document.activeElement) ?? false
    // Read before `setScrolledTo` below schedules clearing it — reading
    // `scrolledTo` again after that call would only ever see `undefined`,
    // which is exactly the render this effect runs on, not the one before it.
    // This is what tells "the window this render started with was already the
    // settled one" apart from "a flick had left it stale", which the success
    // of `recenter()` just below cannot tell apart on its own — see that
    // call's own comment.
    const resetPending = scrolledTo !== undefined
    setScrolledTo(undefined)
    pendingRecentre.current = true
    // Usually succeeds immediately: the ordinary case is a window that
    // already contains the new focus, because nothing has scrolled the rail
    // away from it since the last render. Keeping this attempt here, rather
    // than only ever relying on the flush effect below, is what makes the
    // ordinary case cost nothing extra.
    //
    // When the reader *had* flicked the rail elsewhere, this attempt runs
    // against the *stale* window — the one built from the `scrolledTo` this
    // same call is resetting, not yet the one rebuilt around the new focus,
    // because `setScrolledTo` only schedules that rebuild rather than
    // applying it inline — so `currentRef`/`pairedRef`/`anchorRef` can all be
    // null here even though the tile they should point at is about to exist.
    // A page turn after a flick would then rebuild the window around the new
    // current tile a moment later with nothing having called `scrollIntoView`
    // on it, and the rail would sit scrolled to wherever the flick left it
    // with nothing rendered there — a blank strip. Leaving the flag set when
    // this attempt fails is what lets the second effect below retry once that
    // rebuild has actually landed.
    //
    // Leaving the flag set when this attempt *succeeds* matters too,
    // whenever `resetPending` is true. `recenter()` can find a ref and scroll
    // to it against the stale window — the new current tile can simply
    // already have been inside it — but the reset this same call scheduled has
    // not rebuilt the window yet, so the tile has only landed to the accuracy
    // of the stale window's spacer arithmetic, not the rebuilt one's.
    // Clearing the flag here would stop the retry effect below from ever
    // correcting that. In the ordinary case `resetPending` is already false
    // (nothing had scrolled the rail away since the last render), so this
    // changes nothing there — the flag clears on this first success.
    if (recenter() && !resetPending) pendingRecentre.current = false
    // A tile's `minWidth` floor gives it its full nominal width before its
    // image has decoded, so a tile's on-screen position does not depend on
    // load order — the attempt above is enough on its own, and nothing needs
    // to re-run once thumbnails start arriving.
    //
    // No capture-phase `load` listener on the rail re-runs `recenter` when a
    // thumbnail loads (`load` does not bubble, so catching a descendant's
    // would take the capture phase). Scrolling the rail renders new tiles,
    // whose images then load, so such a listener would fire continuously while
    // the reader is mid-flick and keep dragging the rail back to the current
    // tile, which would make the rail impossible to make progress on. It would
    // only catch a tile that grows past its floor once its image decodes — a
    // double-page scan the sidecar never marked `spread: true` — and that
    // growth is small enough on the common case that going without the
    // listener trades away only the rare, cosmetic case of an unmarked spread
    // for a rail that can be scrolled at all.
  }, [currentIndex, visible, correcting?.pairedIndex, anchor])

  // The follow the effect above asks for. No dependency list, so it retries on
  // the render that rebuilds the window if the tile was not there yet. While
  // correcting neither ref is attached, so a pick moves nothing.
  useEffect(() => {
    if (!followFocus.current) return
    const target = currentRef.current ?? anchorRef.current
    if (!target) return
    followFocus.current = false
    if (document.activeElement !== target) target.focus({ preventScroll: true })
  })

  // The retry the comment above promises: no dependency array, so this runs
  // after *every* render — including the one that resets `scrolledTo` to
  // `undefined` and rebuilds the window around `primaryFocus`, which is
  // exactly the render the effect above cannot see the result of (it has
  // already run by the time that reset commits). Guarded by
  // `pendingRecentre` rather than calling `recenter` unconditionally on
  // every render: ordinary scrolling also re-renders this component (via
  // `setScrolledTo` in the windowing effect further down), and re-centring
  // on every one of those renders would drag the rail out from under the
  // reader's finger mid-flick — the failure a `load` listener re-centring
  // would cause, from a different trigger. Only the effect above
  // ever sets the flag, and a scroll changes none of its dependencies — so a
  // flick cannot arm this, which is the property that matters. What can:
  // a page turn, a new correction, a new anchor, and also the mount itself
  // and the chrome being shown, since `visible` is among those dependencies.
  useEffect(() => {
    if (!pendingRecentre.current) return
    if (recenter()) pendingRecentre.current = false
  })

  // Refines `--strip-unit` from real tiles' drawn widths, once, the first
  // time enough qualifying tiles have actually loaded (`medianTileWidth`'s
  // own minimum sample — see `tiles.ts`). See `reader.css`'s own comment on
  // `--strip-unit` for what the constant is: a starting estimate for the
  // first paint and for any tile that has not drawn yet, refined here once
  // real geometry exists.
  //
  // A ref, not state: writing `--strip-unit` is a direct DOM mutation onto
  // the rail element, the same way `scrollIntoView` above already is, and
  // triggering a re-render over it would cost a frame for nothing this
  // component itself reads back — every consumer of the value (the floor,
  // the spacers, every tile's own `minWidth`) reads it straight from CSS.
  const unitMeasuredRef = useRef(false)
  useEffect(() => {
    const rail = railRef.current
    if (!rail) return

    // Scans the *currently rendered* tiles for every one that has actually
    // drawn (`naturalWidth > 0`; an unloaded image is still sitting at its
    // `minWidth` floor, which is the CSS estimate this exists to correct,
    // not a measurement of anything real), and takes the *median* of their
    // widths rather than the first one found.
    //
    // A double-page scan the sidecar never marked `spread: true` is itself a
    // one-slot group, so `tileUnits` correctly calls it one unit and it
    // carries the same one-unit `minWidth` floor as an ordinary tile — while
    // drawing at roughly twice the nominal width. Nothing here can tell that
    // tile apart from an ordinary one by `minWidth` alone, because there is
    // nothing to tell apart: both are genuinely one-unit by the sidecar's own
    // account. Taking whichever tile happened to load first would accept that
    // tile's width outright whenever it won the race, corrupting
    // `--strip-unit` (and with it every tile floor and every spacer) to about
    // double the true value. `medianTileWidth` (`tiles.ts`) is what a handful
    // of such outliers among an edition's ordinary pages cannot move.
    //
    // Every loaded tile contributes to the sample, not only one-unit ones —
    // `normalizedTileWidth` (`tiles.ts`) rescales a two-unit tile's own outer
    // width back to what a one-unit tile of the same per-unit width would
    // have measured, so a `two`-mode pair or a genuine `spread: true` tile
    // joins the same sample a one-unit tile does rather than being excluded
    // from it. `two` mode gives every group but a break singleton two slots
    // (`pairing.ts`), so a 33-tile window could hold as few as zero one-unit
    // tiles under a one-unit-only filter — `medianTileWidth` could then never
    // reach its minimum sample, which would leave `--strip-unit` unrefined
    // and this effect's `load` listener below attached for the component's
    // whole life. A tile's unit count is read back from its own `minWidth` by
    // an equality check against `tileMinWidth(1)` or `tileMinWidth(2)` rather
    // than a second parser for the `calc()` string itself.
    //
    // Marks `unitMeasuredRef` only once `medianTileWidth` has enough loaded
    // tiles to answer at all (its own minimum sample, not merely "whether a
    // write happened"), so this keeps scanning as more thumbnails load until
    // it has a trustworthy sample — see `refinedStripUnit`'s own doc for why
    // that is what keeps the measurement from oscillating: setting
    // `--strip-unit` changes every tile's width on the next paint, which
    // could in principle re-trigger a measurement, but there is nothing left
    // to re-trigger once this ref is set.
    const attempt = () => {
      if (unitMeasuredRef.current) return
      const cs = getComputedStyle(rail)
      const border = Number.parseFloat(
        cs.getPropertyValue('--strip-tile-border'),
      )
      const widths = [...rail.querySelectorAll('button')].flatMap((btn) => {
        const naturalWidth = btn.querySelector('img')?.naturalWidth ?? 0
        if (naturalWidth <= 0) return []
        const tileUnitCount =
          btn.style.minWidth === tileMinWidth(1)
            ? 1
            : btn.style.minWidth === tileMinWidth(2)
              ? 2
              : undefined
        if (tileUnitCount === undefined) return []
        return [
          normalizedTileWidth(
            btn.getBoundingClientRect().width,
            border,
            tileUnitCount,
          ),
        ]
      })
      const median = medianTileWidth(widths)
      if (median === undefined) return
      const current = Number.parseFloat(cs.getPropertyValue('--strip-unit'))
      const unit = refinedStripUnit(median, border, current)
      if (unit !== undefined) {
        rail.style.setProperty('--strip-unit', `${unit}px`)
      }
      unitMeasuredRef.current = true
    }

    // The *other* way this has to terminate. `attempt` only ever sets
    // `unitMeasuredRef` once `medianTileWidth` reaches its minimum sample —
    // but a rail with fewer tiles than that (a work with fewer than
    // `minSample` groups, say) can never reach it, and relying on the ref
    // alone would leave the `load` listener below attached for the
    // component's whole life, re-running `querySelectorAll` and a forced
    // `getBoundingClientRect` read on every future thumbnail load — exactly
    // the per-load cost windowing exists to remove. Once every tile
    // *currently* rendered has loaded, no further `load` event on this window
    // can add anything `attempt` has not already seen, so there is nothing
    // left to wait for. Neither condition is enough on its own: the ref alone
    // never fires below the minimum sample, and this check alone — with no
    // ref to short-circuit it — would wait for every one of a 33-tile
    // window's images before ever measuring, which is the early-exit the ref
    // exists to provide in the ordinary case.
    const doneForNow = () =>
      unitMeasuredRef.current ||
      [...rail.querySelectorAll('button')].every(
        (btn) => (btn.querySelector('img')?.naturalWidth ?? 0) > 0,
      )

    // A cached image can already report `naturalWidth` before any `load`
    // fires at all, so the first attempt happens synchronously rather than
    // waiting on an event that may never come.
    attempt()
    if (doneForNow()) {
      unitMeasuredRef.current = true
      return
    }

    // Capture phase: `load` does not bubble, so a descendant `<img>`'s load
    // has to be caught on the way down. A listener that re-ran
    // `scrollIntoView` on every descendant load would fight a mid-flick
    // scroll (see the centring effect's comment); this one only ever calls
    // `attempt`, which writes a CSS custom property at most once and never
    // scrolls, and it detaches itself once either termination condition above
    // is met — so it fires at most as many times as it takes to get there, not
    // once per tile for the component's whole life, and cannot cause that.
    // Listening on the rail rather than on one specific tile's image is
    // deliberate too: lazy-loaded images resolve out of DOM order, so no fixed
    // set of tiles' images can be counted on to be the ones that load.
    const onLoad = (event: Event) => {
      if (!(event.target instanceof HTMLImageElement)) return
      attempt()
      if (doneForNow()) {
        unitMeasuredRef.current = true
        rail.removeEventListener('load', onLoad, true)
      }
    }
    rail.addEventListener('load', onLoad, true)
    return () => rail.removeEventListener('load', onLoad, true)
  }, [])

  // Kept current after every render rather than listed as a dependency of
  // the effect below: `units` (`groups.map(tileUnits)`) is a new array on
  // every render regardless of whether the tiles it describes changed —
  // `App.tsx` computes `grouped` straight in its render body with no
  // `useMemo`, so `groups` itself is a new reference on every `App`
  // re-render, including one triggered by nothing more than the
  // reading-time readout ticking. Depending on it directly would tear the
  // scroll listener down and reattach it on nearly every one of those
  // re-renders — the same per-event cost the rAF throttle below exists to
  // avoid. `onScroll` only needs whichever `units` was current as of the
  // frame it handles, so a ref updated after every render gives it that
  // without resubscribing.
  //
  // `onFocusIn` below reads the tile's own `data-group` (see the tile
  // `<button>`'s own attribute) and not a `lo` ref kept the same way, added
  // to whichever position the focused tile held among the rendered buttons: a
  // ref synced by an effect with no dependency list is only as current as the
  // *previous* commit's effects by the time an *earlier* effect in the *same*
  // commit fires a synchronous DOM event — which is exactly what the
  // focus-follow effect above can do, calling `.focus()` on a tile within the
  // very commit that first renders it. Read there, the `lo` ref would still
  // hold the prior window's value, so the rebuild it triggered would land on
  // the wrong absolute tile and unmount the one focus-follow had just
  // focused. The attribute is written by the same render that put the tile in
  // the DOM, so there is nothing for it to lag behind.
  const unitsRef = useRef(units)
  useEffect(() => {
    unitsRef.current = units
  })

  // What the rail shows of its whole length, told to `onShown` when it has
  // changed. Three things change it, and each has its own caller:
  //
  // - a scroll, told from the frame the scroll listener below already takes;
  // - a render, told by the effect here, which runs after every one. A render
  //   can change the rail's length under a `scrollLeft` that stays where it
  //   was: other groups, or a window rebuilt so that a spacer at its nominal
  //   width stands where loaded tiles stood wider;
  // - neither of those, told by the effect after this one.
  //
  // Compared with what was last told, so a caller that found nothing moved
  // tells nothing, and `Chrome` is not made to write the mark again on a
  // render that was only the reading-time readout ticking. Compared exactly,
  // with no allowance for a small difference: the mark is placed by how far
  // the rail is along its travel, and on a volume a little longer than the
  // strip is wide the travel is a small part of the rail's length, so a
  // movement that is next to nothing against the length is a real part of
  // the way along the line.
  const onShownRef = useRef(onShown)
  const toldShown = useRef<RailShare | undefined>(undefined)
  const tellShown = useRef(() => {
    const rail = railRef.current
    if (!rail) return
    const shown = shownShare(rail)
    const told = toldShown.current
    if (told && told.start === shown.start && told.end === shown.end) return
    toldShown.current = shown
    onShownRef.current?.(shown)
  })
  useEffect(() => {
    onShownRef.current = onShown
    tellShown.current()
  })

  // What changes the rail's share with no scroll and no render.
  //
  // A thumbnail that loads gives its tile its real width, and the rail grows
  // by the difference. `load` does not bubble, so it is heard in the capture
  // phase, as the listener that refines `--strip-unit` hears it. That one
  // lets go once its measuring is done; this one stays for as long as the
  // strip is mounted, since every window the reader scrolls to loads
  // thumbnails of its own. Answered in the next frame and not in the event:
  // a window's thumbnails arrive together, and each load leaves the rail to
  // be laid out again before it can be read, so reading it at every one
  // would do that work once for each image where a frame does it once.
  //
  // The rail's own width follows the window's. The observer is called after
  // layout, when reading the rail costs nothing, so it tells there and then
  // and takes no frame.
  useEffect(() => {
    const rail = railRef.current
    if (!rail) return
    let frame = 0
    const onLoad = () => {
      if (frame) return
      frame = requestAnimationFrame(() => {
        frame = 0
        tellShown.current()
      })
    }
    rail.addEventListener('load', onLoad, true)
    const observer = new ResizeObserver(() => tellShown.current())
    observer.observe(rail)
    return () => {
      rail.removeEventListener('load', onLoad, true)
      observer.disconnect()
      if (frame) cancelAnimationFrame(frame)
    }
  }, [])

  // Extending the window as the reader flicks along the rail. Throttled to
  // one frame: a scroll fires far more often than a window needs rebuilding,
  // and rebuilding it per event would put the very cost this windowing
  // exists to remove back on the reader's finger.
  //
  // The fraction, rather than a pixel offset, is what `focusAtFraction`
  // takes — see its doc. `Math.abs` is what makes `row-reverse` work:
  // scrolling a reversed flex row moves `scrollLeft` negative in this
  // engine, and the distance along the rail is the same either way.
  //
  // Read where the rail's own spacers sit against the visible rail, from the
  // DOM, and rebuild once one of them is within a quarter of the rail's width
  // of it — early enough that the window is rebuilt while there is still
  // something drawn where the reader is about to look (O140). This is right
  // regardless of viewport width, unlike a tile-count constant that has to be
  // re-derived — and can be wrong — for every width the rail is shown at.
  //
  // Reading the first and last *rendered tile's* box instead, and checking
  // both against both ends of the rail, does not work: the rail is
  // `row-reverse` for a right-to-left work, so DOM order and visual order are
  // opposite there, and the last tile in the DOM sits at the *left* of the
  // screen, not the right. Checking both ends of the DOM order against both
  // ends of the rail does not cancel that out — it means one of the two
  // checks is true on nearly every frame, in whichever direction the reading
  // direction did not intend. A spacer reaching the visible strip *is* the
  // blank gap this guard exists to prevent, and testing for that directly is
  // symmetric under either flex direction with no arithmetic about reading
  // direction needed at all.
  //
  // Attached once per mount (empty deps) rather than re-attached whenever
  // `groups` changes: `groups` is a new array on every `App` re-render (see
  // `unitsRef` above), so keying off it would tear both listeners down and
  // reattach them on essentially every render. `units` is read through
  // `unitsRef`, so nothing render-scoped is captured and one mount's
  // listeners stay correct for the component's whole lifetime. `Filmstrip`
  // itself is unmounted and remounted whenever the chrome is toggled
  // (`Chrome` returns `null` rather than hiding it in place — see the
  // `visible` prop's doc), which is what makes per-mount the right
  // granularity rather than per-app-lifetime.
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
        const rb = rail.getBoundingClientRect()
        // Re-anchor before a spacer reaches the visible strip, not after: a
        // guard that covers the distance scrolled while the new window renders,
        // which depends on scroll speed and render time rather than viewport
        // width alone, so a purely proportional guard under-provides on narrow
        // screens where render time is longest. The guard is at least a quarter
        // of the rail's width, but no less than 200 px: under 4× CPU throttle a
        // 97 px guard (390/4) leaves a spacer briefly visible on a 390 px
        // screen. Asked of the spacers rather than the first/last rendered
        // tile — see this effect's own doc above for why a tile-ends test
        // silently inverts under `row-reverse` and a spacer-ends test does not.
        const guard = Math.max(rb.width / 4, 200)
        let nearAGap = false
        for (const spacer of rail.querySelectorAll(
          '[data-testid="strip-spacer"]',
        )) {
          const b = spacer.getBoundingClientRect()
          if (b.right > rb.left - guard && b.left < rb.right + guard) {
            nearAGap = true
            break
          }
        }
        if (nearAGap) {
          setScrolledTo(focusAtFraction(unitsRef.current, fraction))
        }
        tellShown.current()
      })
    }
    rail.addEventListener('scroll', onScroll, { passive: true })
    // A keyboard or screen-reader user reaches the rail by `Tab`, never by
    // scrolling it, so the window has to extend on focus too — otherwise,
    // while correcting (when every tile is a stop), `Tab` past the last
    // rendered tile finds nothing to focus and leaves the rail entirely.
    // `focusin` bubbles (plain `focus` does not), so one listener on the rail
    // catches every tile taking focus.
    //
    // Not focus the drag's effect hands back to a tile, which is no reader
    // reaching that tile. The rail may have been scrolled or carried on a
    // long way since the tile had focus, and a window moved to the tile then
    // ends short of what the rail shows: a spacer on show where tiles should
    // be, and after a press that stopped a glide no scroll follows to move
    // the window again.
    //
    // Any other focus that comes to a tile of the rail stops a glide. The
    // browser scrolls a tile reached by `Tab` into view, and a glide's next
    // frame would write the rail back to where the glide had it going.
    const onFocusIn = (event: FocusEvent) => {
      if (handingFocusBack.current) return
      const tile = (event.target as HTMLElement | null)?.closest('button')
      if (!tile || !rail.contains(tile)) return
      stopGlide.current()
      // The tile's own `data-group` (see the tile `<button>`'s own
      // attribute), not a position among the rendered buttons plus a
      // separately-synced `lo` — see the comment above `unitsRef` for why
      // that ref could still be one commit behind when this fires. The
      // attribute is written by the same render that put the tile in the
      // DOM, so it can never disagree with what is actually on screen.
      const group = tile.dataset.group
      if (group !== undefined) setScrolledTo(Number(group))
    }
    rail.addEventListener('focusin', onFocusIn)
    return () => {
      rail.removeEventListener('scroll', onScroll)
      rail.removeEventListener('focusin', onFocusIn)
      if (frame) cancelAnimationFrame(frame)
    }
  }, [])

  // The wheel, since the rail draws no scrollbar for a mouse to drag (O138).
  // Subscribed by hand for the reason `App.tsx`'s own wheel effect gives:
  // React's `wheel` listener is passive, and this one has to
  // `preventDefault()`.
  //
  // An effect of its own rather than a third listener in the one above. That
  // one is attached once per mount and reads everything through refs;
  // `reading` is the one render-scoped value this needs, and it changes on
  // `d` and never on a page turn.
  //
  // The write to `scrollLeft` fires the browser's own `scroll`, so the window
  // follows a wheel the way it follows a flick.
  useEffect(() => {
    const rail = railRef.current
    if (!rail) return
    const later = stepFor(1, reading)
    const onWheel = (event: WheelEvent) => {
      const px = stripWheel(event)
      if (px === undefined) return
      // Before the write, and whether or not the rail can move: a wheel the
      // rail takes is never the page's too, at either end of the rail.
      event.preventDefault()
      rail.scrollLeft += later * px
    }
    rail.addEventListener('wheel', onWheel, { passive: false })
    return () => rail.removeEventListener('wheel', onWheel)
  }, [reading])

  // A change of reading direction stops a glide. The glide writes
  // `scrollLeft`, and the rail turned round measures it from its other end: a
  // reversed row's runs negative. The glide's next frame would send the rail
  // to a place reckoned from the end it no longer starts at.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `reading` is the trigger, not a closure read: the body is what runs when it changes.
  useEffect(() => {
    stopGlide.current()
  }, [reading])

  // Read at each release by the drag's effect below, which is subscribed once
  // and so cannot take it from a render.
  const glidesRef = useRef(glides)
  useEffect(() => {
    glidesRef.current = glides
  })

  // A mouse drags the rail (O216). A press that travels `STRIP_DRAG_SLOP`
  // along the rail becomes a drag. From then on the rail stands where the
  // press found it, less the pointer's travel since. The pointer is captured,
  // so the rail keeps following it outside the strip, and the click the
  // release makes is swallowed, so a drag never turns the page. A press that
  // travels less is a click on the tile under it.
  //
  // A mouse's primary button only. A finger scrolls the rail as the browser
  // scrolls any box that overflows, and a pen is the browser's to answer too:
  // neither is taken here.
  //
  // A drag let go while the pointer is still moving carries the rail on: a
  // glide. `releaseSpeed` reads the pointer's speed from where it has been
  // since the press, and `glideTravel` says how far on from where it was let
  // go the rail has gone since, a frame at a time, slowing until it comes to
  // rest or to an end of the rail. There is none after a `pointercancel`,
  // which is no release, none where `glides` is false, and none where the
  // system asks for reduced motion.
  //
  // A glide stops at any press on the rail, a finger's and a pen's as well
  // as a mouse's, so the browser never scrolls the rail for a finger while a
  // glide is still writing to it; at a wheel over the rail; at the strip's
  // own scroll to a tile; where a tile of the rail takes focus, other than
  // focus the rail hands back; at a change of reading direction; and when
  // the strip is taken away.
  //
  // A mouse's press that stops a glide still carrying the rail at
  // `STRIP_GLIDE_MIN_SPEED` or more is no click on the tile it lands on, as
  // a drag is none: its click is swallowed and the focus it moved is handed
  // back. A press that stops a slower one is an ordinary press, and a click
  // where it goes nowhere. Either can still become a drag. For a finger's
  // tap or a pen's that stops a glide, nothing is swallowed and no focus is
  // handed back.
  //
  // Subscribed by hand, as the wheel above is. What a press has done so far,
  // and a glide, are kept in this effect's own variables and in no state, so
  // the moves and the release of a drag and the frames of a glide render
  // nothing here. What does render is the press: a mouse focuses the tile it
  // presses, and the `focusin` listener above moves the window to a tile that
  // takes focus. Focus handed back to a tile moves no window.
  //
  // The click is stopped at the rail on its way down to the tile, so it never
  // comes back up to the root, which is where React hears a click before it
  // calls a tile's `onClick`.
  useEffect(() => {
    const rail = railRef.current
    if (!rail) return
    let held:
      | {
          pointer: number
          x: number
          scrollLeft: number
          dragging: boolean
          focused: Element | null
          /** Whether this press stopped a glide that was still carrying the rail: see `onDown`. */
          stopped: boolean
        }
      | undefined
    let swallowClick = false
    // Where the pointer has been since a mouse's primary button was last
    // pressed on the rail: the press itself, then each move of that press.
    // A drag's speed is read from it at the release. It begins at the press
    // and not where the press becomes a drag, or a press that creeps to
    // `STRIP_DRAG_SLOP` and is let go would be read from its last move or
    // two alone, as going fast.
    let trail: GlideSample[] = []
    // The glide under way, where one is: the frame it has asked for, the
    // speed it began at, which is the pointer's as it was let go, and when
    // it began.
    let glide: { frame: number; speed: number; began: number } | undefined
    const endGlide = () => {
      if (glide) cancelAnimationFrame(glide.frame)
      glide = undefined
    }
    stopGlide.current = endGlide
    const startGlide = (speed: number) => {
      const from = rail.scrollLeft
      const began = performance.now()
      const whole = Math.abs(speed * STRIP_GLIDE_MS)
      const step = (now: number) => {
        // The content goes on the way the pointer was going, as it followed
        // the pointer in the drag, so `scrollLeft` goes the other way.
        const carried = glideTravel(speed, now - began)
        const want = from - carried
        rail.scrollLeft = want
        // Within half a pixel of where the glide comes to rest; or the rail
        // would not go where it was sent, which is an end of the rail.
        if (
          whole - Math.abs(carried) < 0.5 ||
          Math.abs(rail.scrollLeft - want) > 1
        ) {
          glide = undefined
          return
        }
        glide = { frame: requestAnimationFrame(step), speed, began }
      }
      glide = { frame: requestAnimationFrame(step), speed, began }
    }
    const onDown = (event: PointerEvent) => {
      // Before the pointer and the button are asked about: any press on the
      // rail stops a glide.
      //
      // The press counts as having stopped one only where the glide was
      // still carrying the rail at `STRIP_GLIDE_MIN_SPEED` or more. A glide
      // runs on until it is all but at rest, which is long after the strip
      // has stopped to the eye, and a press on a tile then is the reader
      // choosing that tile.
      const stopped =
        glide !== undefined &&
        Math.abs(glideSpeed(glide.speed, performance.now() - glide.began)) >=
          STRIP_GLIDE_MIN_SPEED
      endGlide()
      if (event.pointerType !== 'mouse' || event.button !== 0) return
      trail = [{ at: event.timeStamp, x: event.clientX }]
      held = {
        stopped,
        pointer: event.pointerId,
        x: event.clientX,
        scrollLeft: rail.scrollLeft,
        dragging: false,
        // What has focus as the press begins. The browser moves focus to the
        // tile that was pressed only after this event.
        focused: document.activeElement,
      }
    }
    // A mouse focuses the tile it presses, and a focused tile is pressed
    // again by Space and by Enter, which would turn the page to it. That is
    // right after a click and wrong after a drag, which is not to turn the
    // page at all. So when a press becomes a drag, focus that the press has
    // moved onto a tile of this rail is handed back to whatever had it as the
    // press began, and where that takes none, as the document's body does
    // not, the tile is left without it. Focus the press did not move, or
    // moved to nothing of the rail's, as a press between two tiles does, is
    // left where it is.
    //
    // `handingFocusBack` is up for as long as `focus()` runs, which is when
    // the `focusin` it sends is heard: the strip's window stays where it is.
    const giveFocusBack = (to: Element | null) => {
      const taken = document.activeElement
      if (taken === to) return
      if (!(taken instanceof HTMLElement) || !rail.contains(taken)) return
      if (to instanceof HTMLElement) {
        handingFocusBack.current = true
        to.focus({ preventScroll: true })
        handingFocusBack.current = false
      }
      if (document.activeElement === taken) taken.blur()
    }
    const onMove = (event: PointerEvent) => {
      if (!held || held.pointer !== event.pointerId) return
      // A move with the primary button up ends the press as a release does.
      // A mouse whose primary button comes up while another is held sends a
      // move, its click after it, and no release until the last button is up.
      // And a press whose release was heard nowhere ends here: left standing,
      // it would have the rail follow a mouse that nobody is holding.
      if ((event.buttons & 1) === 0) {
        onEnd(event)
        return
      }
      // Every move of the press, those before it is a drag too. The loop
      // bounds how many a long press keeps, and decides nothing:
      // `releaseSpeed` reads the last `STRIP_GLIDE_SAMPLE_MS` of whatever
      // trail it is handed.
      trail.push({ at: event.timeStamp, x: event.clientX })
      while (
        trail.length > 1 &&
        event.timeStamp - trail[0].at > STRIP_GLIDE_SAMPLE_MS
      ) {
        trail.shift()
      }
      const travelled = event.clientX - held.x
      if (!held.dragging) {
        if (Math.abs(travelled) < STRIP_DRAG_SLOP) return
        held.dragging = true
        rail.setPointerCapture(event.pointerId)
        rail.style.cursor = 'grabbing'
        giveFocusBack(held.focused)
      }
      // The rail's content goes where the pointer goes, whichever way the
      // rail runs: a reversed row's `scrollLeft` runs negative, and it too
      // grows as the content moves left.
      rail.scrollLeft = held.scrollLeft - travelled
    }
    const onEnd = (event: PointerEvent) => {
      if (!held || held.pointer !== event.pointerId) return
      const { dragging, stopped, focused } = held
      held = undefined
      // A press that stopped a glide still carrying the rail, and went
      // nowhere, is no click on the tile it landed on, which the glide
      // carried under it. Its click is swallowed and the focus it moved is
      // handed back, as a drag's are.
      if (!dragging && !stopped) return
      if (!dragging) giveFocusBack(focused)
      rail.style.cursor = ''
      if (rail.hasPointerCapture(event.pointerId)) {
        rail.releasePointerCapture(event.pointerId)
      }
      // The click the release makes comes straight after it, before a timer
      // set here lands. After a drag it comes to the rail and not to the tile
      // under the pointer, since the pointer was captured; after a press that
      // stopped a glide it comes to the tile, past the rail on its way down.
      // It is swallowed wherever on the rail it lands. Where no click
      // follows, as after a `pointercancel`, the timer is what keeps the flag
      // from swallowing the reader's next one.
      swallowClick = true
      setTimeout(() => {
        swallowClick = false
      }, 0)
      // Only a drag carries the rail on: a press that stopped a glide and
      // was let go short of a drag has moves of its own in the trail, and
      // they are not a flick. And only a `pointerup`: a `pointercancel` is no
      // release, and a move with the button up can come long after the
      // button did.
      const speed =
        dragging &&
        event.type === 'pointerup' &&
        glidesRef.current &&
        !window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
          ? releaseSpeed(trail, event.timeStamp)
          : 0
      if (speed !== 0) startGlide(speed)
    }
    const onWheelStop = () => endGlide()
    const onClick = (event: MouseEvent) => {
      if (!swallowClick) return
      swallowClick = false
      event.preventDefault()
      event.stopPropagation()
    }
    // A drag of the browser's own cancels the pointer, and the rail stops
    // following. A press on a thumbnail starts none: the image takes no
    // pointer events (`tileImgStyle`), so the press is its tile's. A press on
    // a tile that is part of a selection does, in Chromium.
    const onDragStart = (event: DragEvent) => event.preventDefault()
    rail.addEventListener('pointerdown', onDown)
    rail.addEventListener('pointermove', onMove)
    rail.addEventListener('click', onClick, true)
    rail.addEventListener('dragstart', onDragStart)
    rail.addEventListener('wheel', onWheelStop, { passive: true })
    // The end of a press is listened for on the window, and not on the rail.
    // Before a press is a drag nothing has captured the pointer, so it can
    // leave the rail with the button down and be let go out there, where the
    // rail would not hear it: the press would stand, and another press made
    // out there and carried back in with the button down would be taken for
    // its moves. The pointer's leaving the rail ends nothing, since a hand
    // that slips off the strip's edge and back is still pressing.
    //
    // Heard on the way down, before anything nearer to where it lands can
    // stop it. A release on the rail itself passes the window first too, so
    // the rail needs no listener of its own for it.
    window.addEventListener('pointerup', onEnd, true)
    window.addEventListener('pointercancel', onEnd, true)
    return () => {
      rail.removeEventListener('pointerdown', onDown)
      rail.removeEventListener('pointermove', onMove)
      rail.removeEventListener('click', onClick, true)
      rail.removeEventListener('dragstart', onDragStart)
      window.removeEventListener('pointerup', onEnd, true)
      window.removeEventListener('pointercancel', onEnd, true)
      rail.removeEventListener('wheel', onWheelStop)
      endGlide()
    }
  }, [])

  // The progress line's way to drag the rail (O286). The travel is read at
  // each call and not kept: thumbnails that load and a window rebuilt near a
  // spacer change the rail's length while the line is being dragged. The
  // write fires the rail's own `scroll`, so the window and the mark follow
  // it as they follow the wheel.
  useEffect(() => {
    if (!scroller) return
    const later = stepFor(1, reading)
    scroller.current = {
      scrollAlong: (along) => {
        const rail = railRef.current
        if (!rail) return
        stopGlide.current()
        rail.scrollLeft =
          later * along * Math.max(rail.scrollWidth - rail.clientWidth, 0)
      },
    }
    return () => {
      scroller.current = null
    }
  }, [scroller, reading])

  return (
    <div
      ref={railRef}
      className="reader-strip-rail"
      style={{
        ...railStyle,
        // Derived from `stepFor` rather than re-deciding what `rtl` means:
        // the rail runs the way a forward step goes, which is the same rule
        // the arrows and the help ask.
        flexDirection: stepFor(1, reading) === 1 ? 'row' : 'row-reverse',
      }}
    >
      {lo > 0 && (
        <div
          data-testid="strip-spacer"
          aria-hidden="true"
          style={{
            ...spacerStyle,
            width: spacerWidth(unitsBetween(groups, 0, lo), lo),
          }}
        />
      )}
      {groups.slice(lo, hi).map((group, offset) => {
        const index = lo + offset
        const isCurrent = index === currentIndex
        const isResolved =
          correcting?.pairedIndex !== undefined &&
          group.slots.includes(correcting.pairedIndex)
        // `currentIndex === -1 && !correcting`, not `!isCurrent &&
        // !isResolved`. Those two are already false by the time the ternary
        // below reaches this branch, so testing them again filters nothing —
        // they would only stop *this* tile being claimed twice, which the
        // ternary's own ordering already makes impossible. What has to be
        // excluded is a *different* tile being current or resolved while this
        // one matches `anchor`: two refs attached in one render, which is the
        // case the doc above says cannot happen. Testing the caller's contract
        // directly is what makes that claim true of this code rather than only
        // of `App.tsx`.
        const isAnchor =
          anchor !== undefined &&
          currentIndex === -1 &&
          !correcting &&
          group.slots.includes(anchor)
        // The strip is one stop in the reader's ring (spec 2026-09-26 §3.2):
        // the tile the reader stands on, or the anchor where they stand on
        // nothing this edition has. While correcting, every tile is a stop —
        // picking one is the correction's whole task, and nothing is current
        // then. VoiceOver swipes to every tile either way.
        const isStop = correcting !== undefined || isCurrent || isAnchor
        return (
          <button
            key={group.slots[0]}
            ref={
              isCurrent
                ? currentRef
                : isResolved
                  ? pairedRef
                  : isAnchor
                    ? anchorRef
                    : undefined
            }
            type="button"
            data-group={index}
            tabIndex={isStop ? 0 : -1}
            style={{
              ...(isCurrent
                ? currentTileStyle
                : isResolved
                  ? resolvedTileStyle
                  : tileStyle),
              minWidth: tileMinWidth(tileUnits(group)),
            }}
            aria-current={isCurrent ? 'true' : undefined}
            onClick={() =>
              correcting && onPickCounterpart
                ? onPickCounterpart(group.slots[0])
                : onNavigate(group.slots[0])
            }
            // Pointer *and* focus, so the preview follows a reader arrowing
            // through tiles with the keyboard as well as one using a mouse.
            // Both are no-ops while reading — a preview of the page you are
            // already looking at would be noise.
            onMouseEnter={() =>
              correcting && onPreview?.(previewOf(group, slots))
            }
            onMouseLeave={() => correcting && onPreview?.(undefined)}
            onFocus={() => correcting && onPreview?.(previewOf(group, slots))}
            onBlur={() => correcting && onPreview?.(undefined)}
          >
            {tilePages(group, slots).map(({ slot, page, half }) => (
              // `position: relative` so the half badge below can sit on the
              // thumbnail. A wrapper rather than a badge beside the image:
              // the two halves of one spread are separate tiles with the
              // same `src`, so the label has to travel with the image it
              // labels, not with the tile.
              <span
                key={slot}
                style={{
                  position: 'relative',
                  display: 'flex',
                  height: '100%',
                }}
              >
                <img
                  src={thumbUrl(work, edition, page, tileEdge)}
                  // 1-based — this is read by a person, and `page` itself
                  // (used for `thumbUrl` above) stays the zero-based value the
                  // server expects, the same boundary convention `App.tsx`'s
                  // `altAt` follows for its own `alt` text. `half` is included
                  // when `tilePages` carries one (a joined spread's slot,
                  // addressed on its own while correcting — see `Tile`'s own
                  // doc): without it, that slot's tile and its other half's
                  // tile would share both the same thumbnail *and* the same
                  // alt text, with nothing telling them apart.
                  alt={half ? `page ${page + 1} ${half}` : `page ${page + 1}`}
                  loading="lazy"
                  style={tileImgStyle}
                />
                {/*
                A joined spread's two correction tiles share one thumbnail —
                `thumbUrl` is page-indexed and the two halves are one page —
                so without a visible mark a sighted reader has only position
                to tell them apart. The `alt` text above already carries the
                half for a screen reader; this is the same fact on screen.

                Only ever rendered while correcting, because that is the only
                time `tilePages` splits a spread into two tiles at all — see
                `pairing.ts`'s `correctionTiles`.
              */}
                {half && (
                  <span className="reader-half-badge" style={halfBadgeStyle}>
                    {half}
                  </span>
                )}
              </span>
            ))}
          </button>
        )
      })}
      {hi < groups.length && (
        <div
          data-testid="strip-spacer"
          aria-hidden="true"
          style={{
            ...spacerStyle,
            width: spacerWidth(
              unitsBetween(groups, hi, groups.length),
              groups.length - hi,
            ),
          }}
        />
      )}
    </div>
  )
}

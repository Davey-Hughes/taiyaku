import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { thumbUrl } from './api'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { stepFor, type Direction } from './modules/direction'
import {
  focusAtFraction,
  medianTileWidth,
  normalizedTileWidth,
  refinedStripUnit,
  spacerWidth,
  stripWindow,
  tileMinWidth,
  tilePages,
  tileUnits,
  unitsBetween,
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
 * The scroll handler below no longer moves the anchor on a count of tiles
 * the focus has drifted — it re-anchors once the *rendered* tiles run out
 * against the visible rail, read straight from the DOM (see that handler's
 * own doc). `windowMargin` still has to be wide enough that the rendered
 * tiles reach past whatever the viewport shows, because the rebuild that
 * check triggers takes a frame to land while the reader keeps scrolling.
 *
 * 16 rather than some smaller number: `focusAtFraction` returns the tile at
 * the rail's *leading* edge for the current scroll fraction, and the
 * viewport extends roughly 13 tiles forward from there at the widest
 * supported width (1440 px, a tile's nominal outer width around 101 px at
 * that breakpoint — `Filmstrip.browser.test.tsx` derives the nominal from
 * computed CSS and checks a rendered tile's width against it, though the
 * test itself only asserts that figure is greater than 50, not that it is
 * exactly 101) — not ~7 either side of the focus, which an earlier version
 * of this margin assumed in order to justify a fixed tile-count drift before
 * re-anchoring. That assumption was wrong, and cost real frames: measured at
 * 1440×900 during a fast scroll, a spacer stood inside the visible strip on
 * 7 frames, the worst one a 321 px gap in a 1440 px rail. 16 is 3 tiles of
 * slack over the 13-tile reach the viewport actually has, kept as a round
 * number rather than shaved to the exact minimum.
 *
 * A 237-tile rail with `windowMargin` at 12 built about 25 tiles per change
 * (O140: 237 tiles cost 274 ms, 20 tiles 38 ms); at 16 a change builds about
 * 33 — a larger window per rebuild, paid for by far fewer rebuilds than
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
// its size. Measured on the running app: 197.33 px against a nominal 101.
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

export interface FilmstripProps {
  /**
   * Which way the rail runs.
   *
   * A right-to-left work is read right-to-left, and a strip that always ran
   * the other way put page 1 on the left while the pages beside it put page 1
   * on the right — measured on `gotoubun-v01`, whose raw is `rtl`: tile order
   * was page 1 at x 26 through page 10 at x 939, unchanged when the direction
   * was flipped. A reader hunting a page had to read the strip backwards from
   * the reader.
   *
   * `row-reverse` rather than reversing the array, so a tile's index still
   * means what it meant — `stepToGroup`, the current-tile border and the
   * correction picker all address tiles by index, and reversing the data would
   * have made every one of them wrong in a way no type could catch.
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
   * all — a gap, or the borrowed page `modules/borrow.ts` now draws in place
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
   * sibling. A thumbnail is 160px on its long edge, which is enough to
   * recognise a page and not enough to decide whether two drawings are the
   * same drawing — which is exactly what a correction asks.
   */
  onPreview?: (tile: { page: number; half?: 'L' | 'R' } | undefined) => void
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
 * The rail is `overflowX: auto` with one tile per group — 187 of them for
 * `en-mono` — and nothing used to keep the current one on screen. Opening the
 * chrome at slot 150 showed tiles 0 through about 15, with the `aria-current`
 * tile off-screen and, since its
 * thumbnail is lazy-loaded, not even requested. `currentRef` is attached to
 * whichever tile is current and centred into view whenever that changes or
 * the strip becomes visible. `pairedRef` plays the same role for the dashed
 * tile, and `anchorRef` for the neighbourhood of a position this edition has
 * no slot for. At most one of the three is ever attached at a time — `App.tsx`
 * sets `anchor` only in a gap or on the borrowed page that now stands in for
 * one, where `currentIndex` is `-1` and nothing is correcting, and `isAnchor`
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
  // Named `primaryFocus` rather than `focus` because Task 4 adds a second
  // source — where the reader has scrolled to — and `focus` becomes the
  // choice between them. Naming it now means Task 4 adds a line rather than
  // renaming one.
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
  // attempt is not enough (C1, whole-branch review).
  const pendingRecentre = useRef(false)

  // `currentRef ?? pairedRef ?? anchorRef`, scrolled into view if one of
  // them is attached. Returns whether it found one, which is what lets the
  // effects below tell "nothing to scroll to yet" apart from "already
  // done".
  const recenter = () => {
    const target = currentRef.current ?? pairedRef.current ?? anchorRef.current
    if (!target) return false
    target.scrollIntoView({ inline: 'center', block: 'nearest' })
    return true
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: `currentIndex`, `visible`, `correcting?.pairedIndex` and `anchor` are triggers, not closure reads — the body reaches the tile through refs. Removing them, which is what this rule suggests, would centre the rail once and never re-centre it. `scrolledTo` is read below (finding D) but deliberately left out too: it changes on nearly every scroll frame and every `Tab` press, and this effect exists to run only on an actual position change, not on those — the same reasoning `loRef`/`unitsRef` further down already follow for `lo` and `units`.
  useEffect(() => {
    // fix-round review, finding D: read before `setScrolledTo` below
    // schedules clearing it — reading `scrolledTo` again after that call
    // would only ever see `undefined`, which is exactly the render this
    // effect runs on, not the one before it. This is what tells "the window
    // this render started with was already the settled one" apart from "a
    // flick had left it stale", which the success of `recenter()` just below
    // cannot tell apart on its own — see that call's own comment.
    const resetPending = scrolledTo !== undefined
    setScrolledTo(undefined)
    pendingRecentre.current = true
    // Usually succeeds immediately: the ordinary case is a window that
    // already contains the new focus, because nothing has scrolled the rail
    // away from it since the last render. Keeping this attempt here, rather
    // than only ever relying on the flush effect below, is what makes the
    // ordinary case cost nothing extra.
    //
    // C1 (whole-branch review): when the reader *had* flicked the rail
    // elsewhere, this attempt runs against the *stale* window — the one
    // built from the `scrolledTo` this same call is resetting, not yet the
    // one rebuilt around the new focus, because `setScrolledTo` only
    // schedules that rebuild rather than applying it inline — so
    // `currentRef`/`pairedRef`/`anchorRef` can all be null here even though
    // the tile they should point at is about to exist. A page turn after a
    // flick reproduced exactly this: the window correctly rebuilt around
    // the new current tile a moment later, but nothing had ever called
    // `scrollIntoView` on it, so the rail sat scrolled to wherever the
    // flick left it with nothing rendered there — a blank strip. Leaving the
    // flag set when this attempt fails is what lets the second effect below
    // retry once that rebuild has actually landed.
    //
    // fix-round review, finding D: leaving the flag set when this attempt
    // *succeeds* matters too, whenever `resetPending` is true. `recenter()`
    // can find a ref and scroll to it against the stale window — the new
    // current tile can simply already have been inside it — but the reset
    // this same call scheduled has not rebuilt the window yet, so the tile
    // has only landed to the accuracy of the stale window's spacer
    // arithmetic, not the rebuilt one's. Clearing the flag here would stop
    // the retry effect below from ever correcting that. In the ordinary
    // case `resetPending` is already false (nothing had scrolled the rail
    // away since the last render), so this changes nothing there — the flag
    // still clears on this first success exactly as before.
    if (recenter() && !resetPending) pendingRecentre.current = false
    // A tile's `minWidth` floor (Task 2a, 2026-09-21) gives it its full
    // nominal width before its image has decoded, so a tile's on-screen
    // position no longer depends on load order — the attempt above is
    // enough on its own, and nothing needs to re-run once thumbnails start
    // arriving.
    //
    // This effect used to also attach a capture-phase `load` listener on the
    // rail (`load` does not bubble, so a descendant thumbnail's load had to
    // be caught on the way down) and re-run `recenter` whenever one fired,
    // to catch a tile that grows past its floor once its image decodes — a
    // double-page scan the sidecar never marked `spread: true`. Windowing
    // is what turned that listener into the bug this fixes: scrolling the
    // rail renders new tiles, whose images then load, so the listener fired
    // continuously while the reader was mid-flick and kept dragging the
    // rail back to the current tile — reported from a real iPhone as a
    // flick that swipes and then returns to the current page within about a
    // second, which made the rail impossible to make progress on. The
    // floor's growth is small enough on the common case — a loaded portrait
    // tile measured 58.16 px against a 58 px floor at 390x844 — that losing
    // the listener trades away only the rare, cosmetic case of an unmarked
    // spread (58 px to 112 px) for a rail that can be scrolled at all.
  }, [currentIndex, visible, correcting?.pairedIndex, anchor])

  // The retry the comment above promises: no dependency array, so this runs
  // after *every* render — including the one that resets `scrolledTo` to
  // `undefined` and rebuilds the window around `primaryFocus`, which is
  // exactly the render the effect above cannot see the result of (it has
  // already run by the time that reset commits). Guarded by
  // `pendingRecentre` rather than calling `recenter` unconditionally on
  // every render: ordinary scrolling also re-renders this component (via
  // `setScrolledTo` in the windowing effect further down), and re-centring
  // on every one of those renders would drag the rail out from under the
  // reader's finger mid-flick — the exact regression the removed `load`
  // listener above caused, from a different trigger. Only the effect above
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
  // own minimum sample — see `tiles.ts`). See `tiles.ts`'s
  // `refinedStripUnit` for why a runtime measurement replaces a hardcoded
  // constant at all — in short, a tile measured 58.16 px wide under desktop
  // emulation and 68.19 under mobile, on one machine in one browser, and no
  // constant can be right in both. **Why the contexts differ is not
  // established**; O138 is the row that would have to answer it and does not
  // (it measured this rail at 0 px subtraction at 1440×900), so do not cite
  // it here as though it did,
  // and `reader.css`'s own comment on `--strip-unit` for what the constant
  // is now: a starting estimate for the first paint and for any tile that
  // has not drawn yet, refined here once real geometry exists.
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
    // C2 (whole-branch review): a double-page scan the sidecar never marked
    // `spread: true` is itself a one-slot group, so `tileUnits` correctly
    // calls it one unit and it carries the same one-unit `minWidth` floor as
    // an ordinary tile — while drawing at roughly twice the nominal width.
    // Nothing here can tell that tile apart from an ordinary one by
    // `minWidth` alone, because there is nothing to tell apart: both are
    // genuinely one-unit by the sidecar's own account. Taking whichever tile
    // happened to load first — the previous rule — accepted that tile's
    // width outright whenever it won the race, corrupting `--strip-unit`
    // (and with it every tile floor and every spacer) to about double the
    // true value. `medianTileWidth` (`tiles.ts`) is what a handful of such
    // outliers among an edition's ordinary pages cannot move.
    //
    // fix-round review, finding A: every loaded tile contributes to the
    // sample now, not only one-unit ones — `normalizedTileWidth` (`tiles.ts`)
    // rescales a two-unit tile's own outer width back to what a one-unit
    // tile of the same per-unit width would have measured, so a `two`-mode
    // pair or a genuine `spread: true` tile joins the same sample a one-unit
    // tile does rather than being excluded from it. `two` mode gives every
    // group but a break singleton two slots (`pairing.ts`), so a 33-tile
    // window could hold as few as zero one-unit tiles under the old filter —
    // `medianTileWidth` could then never reach its minimum sample, which
    // left `--strip-unit` unrefined and this effect's `load` listener below
    // attached for the component's whole life. A tile's unit count is read
    // back from its own `minWidth` by the same equality check the old
    // one-unit-only filter already made against `tileMinWidth(1)` — extended
    // to also recognise `tileMinWidth(2)` — rather than a second parser for
    // the `calc()` string itself.
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

    // fix-round review, finding A: the *other* way this has to terminate.
    // `attempt` only ever sets `unitMeasuredRef` once `medianTileWidth`
    // reaches its minimum sample — but a rail with fewer tiles than that (a
    // work with fewer than `minSample` groups, say) can never reach it, and
    // relying on the ref alone would leave the `load` listener below
    // attached for the component's whole life, re-running `querySelectorAll`
    // and a forced `getBoundingClientRect` read on every future thumbnail
    // load — exactly the per-load cost windowing exists to remove. Once
    // every tile *currently* rendered has loaded, no further `load` event on
    // this window can add anything `attempt` has not already seen, so there
    // is nothing left to wait for. Neither condition is enough on its own:
    // the ref alone never fires below the minimum sample, and this check
    // alone — with no ref to short-circuit it — would wait for every one of
    // a 33-tile window's images before ever measuring, which is the
    // early-exit the ref exists to provide in the ordinary case.
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
    // has to be caught on the way down — the same reason the centring
    // effect's own, now-removed load listener above used capture (see that
    // effect's comment). That listener re-ran `scrollIntoView` on every
    // descendant load and fought a mid-flick scroll; this one only ever
    // calls `attempt`, which writes a CSS custom property at most once and
    // never scrolls, and it detaches itself once either termination
    // condition above is met — so it fires at most as many times as it
    // takes to get there, not once per tile for the component's whole life,
    // and cannot reproduce that bug. Listening on the rail rather than on
    // one specific tile's image is deliberate too: lazy-loaded images
    // resolve out of DOM order, so no fixed set of tiles' images can be
    // counted on to be the ones that load.
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
  // the effect below: `lo` moves with almost every window extension that
  // effect itself causes (a scroll frame or a `Tab` press both call
  // `setScrolledTo`, which shifts `lo` on the next render), so depending on
  // it would tear the listeners down and reattach them on nearly every
  // scroll frame and every keypress — the same per-event cost the rAF
  // throttle below exists to avoid. `onFocusIn` only needs the value current
  // as of the event it handles, and a ref updated after every render gives
  // it that without resubscribing anything.
  //
  // `unitsRef` rides along in the same effect for the same reason: `units`
  // (`groups.map(tileUnits)`) is a new array on every render regardless of
  // whether the tiles it describes changed — `App.tsx` computes `grouped`
  // straight in its render body with no `useMemo`, so `groups` itself is a
  // new reference on every `App` re-render, including one triggered by
  // nothing more than the reading-time readout ticking. `onScroll` only
  // needs whichever `units` was current as of the frame it handles, so a
  // ref updated after every render gives it that without resubscribing.
  const loRef = useRef(lo)
  const unitsRef = useRef(units)
  useEffect(() => {
    loRef.current = lo
    unitsRef.current = units
  })

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
  // The rAF throttle above cuts a scroll's *event* rate, but a 1.2 s flick
  // still produced 65 window rebuilds across 181 throttled frames measured
  // this way (O140), because the focus itself moves by roughly a tile on
  // nearly every frame of a fast flick — a rebuild for every tile of drift
  // is expensive on its own. A first fix held the anchor still until the
  // focus had drifted a fixed tile count from it, sized so that count plus
  // the viewport's own reach stayed inside `windowMargin`. That reasoning
  // was wrong: it assumed the viewport straddles the focus tile roughly
  // evenly, when `focusAtFraction` actually returns the tile at the rail's
  // *leading* edge for the scroll fraction, and the viewport extends forward
  // from there — about 13 tiles at 1440 px, not ~7 either side. Measured at
  // 1440×900 during a fast scroll, that mistake showed up as a spacer inside
  // the visible strip on 7 frames, the worst one a 321 px gap in a 1440 px
  // rail.
  //
  // What replaces it: read where the rail's own spacers sit against the
  // visible rail, from the DOM, and rebuild once one of them is within a
  // quarter of the rail's width of it — early enough that the window is
  // rebuilt while there is still something drawn where the reader is about
  // to look. This is right regardless of viewport width, unlike a
  // tile-count constant that has to be re-derived — and can be wrong — for
  // every width the rail is shown at.
  //
  // A first attempt at this read the first and last *rendered tile's* box
  // instead, and checked both against both ends of the rail, reasoning that
  // checking both made it agnostic to reading direction. It did not: the
  // rail is `row-reverse` for a right-to-left work, so DOM order and visual
  // order are opposite there, and the last tile in the DOM sits at the
  // *left* of the screen, not the right. Checking both ends of the DOM
  // order against both ends of the rail does not cancel that out — it means
  // one of the two checks is true on nearly every frame, in whichever
  // direction the reading direction did not intend. Measured driving a
  // 1.2 s flick at 390×844 under 4× CPU throttle: 71 re-anchors across 182
  // frames, worse than the 65 the drift-based rule above produced and worse
  // than the 24 the spacer rule below achieves. A spacer reaching the
  // visible strip *is* the blank gap this guard exists to prevent, and
  // testing for that directly is symmetric under either flex direction with
  // no arithmetic about reading direction needed at all.
  //
  // Attached once per mount (empty deps) rather than re-attached whenever
  // `groups` changes: `groups` is a new array on every `App` re-render (see
  // `unitsRef` above), so keying off it tore both listeners down and
  // reattached them on essentially every render already — exactly the
  // per-render churn this branch exists to remove, not the "only when tiles
  // actually changed" the old comment here claimed. `units` and `lo` are
  // both read through refs, so nothing render-scoped is captured and one
  // mount's listeners stay correct for the component's whole lifetime.
  // `Filmstrip` itself is unmounted and remounted whenever the chrome is
  // toggled (`Chrome` returns `null` rather than hiding it in place — see
  // the `visible` prop's doc), which is what makes per-mount the right
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
        // of the rail's width, but no less than 200 px — measured at 390×844
        // under 4× CPU throttle a 97 px guard (390/4) leaves a spacer briefly
        // visible for 3 frames, up to 18 px, while 1440×900 sees none with its
        // 360 px proportional guard. Asked of the spacers rather than the
        // first/last rendered tile — see this effect's own doc above for why a
        // tile-ends test silently inverts under `row-reverse` and a spacer-ends
        // test does not.
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
      })
    }
    rail.addEventListener('scroll', onScroll, { passive: true })
    // A keyboard or screen-reader user reaches the rail by `Tab`, never by
    // scrolling it, so the window has to extend on focus too — otherwise
    // `Tab` past the last rendered tile finds nothing to focus and leaves
    // the rail entirely. `focusin` bubbles (plain `focus` does not), so one
    // listener on the rail catches every tile taking focus.
    const onFocusIn = (event: FocusEvent) => {
      const tile = (event.target as HTMLElement | null)?.closest('button')
      if (!tile || !rail.contains(tile)) return
      // `rendered` is the windowed slice, so its indices are slice-relative;
      // `lo` is what makes `at` absolute again — the same off-by-`lo` the
      // tile map itself corrects with `lo + offset`.
      const rendered = [...rail.querySelectorAll('button')]
      const at = rendered.indexOf(tile as HTMLButtonElement)
      if (at >= 0) setScrolledTo(loRef.current + at)
    }
    rail.addEventListener('focusin', onFocusIn)
    return () => {
      rail.removeEventListener('scroll', onScroll)
      rail.removeEventListener('focusin', onFocusIn)
      if (frame) cancelAnimationFrame(frame)
    }
  }, [])

  return (
    <div
      ref={railRef}
      style={{
        ...railStyle,
        // Derived from `stepFor` rather than re-deciding what `rtl` means:
        // the rail runs the way a forward step goes, which is the same rule
        // the arrows and the help ask. This read `reading === 'rtl'` until a
        // review pointed out that the section claiming three consumers of one
        // rule had two, and the third was this line.
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
                  src={thumbUrl(work, edition, page)}
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
                to tell them apart. Position is exactly what misleads here:
                tiles lay out in DOM order while slots are in reading order,
                so under `rtl` the *left* tile is the `R` half. The `alt` text
                above already carries this for a screen reader; this is the
                same fact on screen.

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

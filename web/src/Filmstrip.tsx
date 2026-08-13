import { useEffect, useRef, type CSSProperties } from 'react'
import { thumbUrl } from './api'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { stepFor, type Direction } from './modules/direction'
import { tilePages } from './tiles'

const railStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 6,
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
  borderWidth: 2,
  borderStyle: 'solid',
  borderColor: 'transparent',
  borderRadius: 4,
  background: 'none',
  cursor: 'pointer',
}

const currentTileStyle: CSSProperties = {
  ...tileStyle,
  borderColor: '#fff',
}

// The `R`/`L` mark on one half of a split spread. Bottom-left rather than
// centred, so it never covers the part of a panel a reader is comparing, and
// high-contrast because it sits on artwork of unknown brightness.
const halfBadgeStyle: CSSProperties = {
  position: 'absolute',
  bottom: 2,
  left: 2,
  padding: '0 4px',
  borderRadius: 3,
  background: 'rgba(0, 0, 0, 0.72)',
  color: '#fff',
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
  borderColor: '#f2b134',
  borderStyle: 'dashed',
}

// `height` fixed, `width: auto` so aspect ratio is preserved — the same
// never-crop rule `modules/fit.ts`'s `pageStyle` follows for a page, just at
// tile scale. A tile is deliberately *not* on that rule's upscale switch: a
// thumbnail enlarged past its source helps nobody find a page.
// No `max-width` clamp here: unlike a full page, a tile's width is meant to
// vary with its content (a spread reads about twice as wide as a single
// page), so nothing should force it back down to a square-ish box.
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: `currentIndex`, `visible`, `correcting?.pairedIndex` and `anchor` are triggers, not closure reads — the body reaches the tile through refs. Removing them, which is what this rule suggests, would centre the rail once and never re-centre it.
  useEffect(() => {
    const recenter = () =>
      (
        currentRef.current ??
        pairedRef.current ??
        anchorRef.current
      )?.scrollIntoView({ inline: 'center', block: 'nearest' })
    recenter()
    // A tile has no explicit width or height, so a not-yet-loaded thumbnail
    // (`loading="lazy"`) lays out at close to zero width until it decodes —
    // measured directly: the call above, made the instant this effect runs,
    // centres on wherever *that* collapsed layout puts the current tile,
    // which is not where it ends up once the tiles before it have their
    // real images and push it rightward. `load` does not bubble, so this
    // listens in the capture phase on the rail to catch every descendant
    // thumbnail's load and re-run the same call — each one that resolves
    // before the current tile shifts its on-screen position, and re-issuing
    // the call keeps it centred as that settles, rather than leaving it
    // stranded at the first, wrong guess.
    const rail = railRef.current
    if (!rail) return
    rail.addEventListener('load', recenter, true)
    return () => rail.removeEventListener('load', recenter, true)
  }, [currentIndex, visible, correcting?.pairedIndex, anchor])

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
      {groups.map((group, index) => {
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
            style={
              isCurrent
                ? currentTileStyle
                : isResolved
                  ? resolvedTileStyle
                  : tileStyle
            }
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
                {half && <span style={halfBadgeStyle}>{half}</span>}
              </span>
            ))}
          </button>
        )
      })}
    </div>
  )
}

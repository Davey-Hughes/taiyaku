import { useEffect, useRef, type CSSProperties } from 'react'
import { thumbUrl } from './api'
import type { Group } from './pairing'
import type { Position } from './resolver'
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
// never-crop rule `PageView.tsx`'s `fitWhole` follows, just at tile scale.
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
function previewOf(group: Group, slots: Position[]): { page: number; half?: 'L' | 'R' } | undefined {
  const [first] = tilePages(group, slots)
  return first && { page: first.page, half: first.half }
}

export interface FilmstripProps {
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
 * tile — `currentIndex` never matches anything while `correcting` is set (see
 * `App.tsx`), so at most one of the two refs is ever attached at a time, and
 * centring on "whichever is attached" is exactly the tile a reader needs on
 * screen to disagree with it.
 */
export function Filmstrip({
  work,
  edition,
  slots,
  groups,
  currentIndex,
  onNavigate,
  visible,
  correcting,
  onPickCounterpart,
  onPreview,
}: FilmstripProps) {
  const currentRef = useRef<HTMLButtonElement>(null)
  const pairedRef = useRef<HTMLButtonElement>(null)
  const railRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const recenter = () =>
      (currentRef.current ?? pairedRef.current)?.scrollIntoView({ inline: 'center', block: 'nearest' })
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
  }, [currentIndex, visible, correcting?.pairedIndex])

  return (
    <div ref={railRef} style={railStyle}>
      {groups.map((group, index) => {
        const isCurrent = index === currentIndex
        const isResolved = correcting?.pairedIndex !== undefined && group.slots.includes(correcting.pairedIndex)
        return (
          <button
            key={group.slots[0]}
            ref={isCurrent ? currentRef : isResolved ? pairedRef : undefined}
            type="button"
            style={isCurrent ? currentTileStyle : isResolved ? resolvedTileStyle : tileStyle}
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
            onMouseEnter={() => correcting && onPreview?.(previewOf(group, slots))}
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
              <span key={slot} style={{ position: 'relative', display: 'flex', height: '100%' }}>
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

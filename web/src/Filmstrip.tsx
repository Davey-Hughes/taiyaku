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

export interface FilmstripProps {
  work: string
  /** The edition being viewed — tiles show its pages, not the authority's. */
  edition: string
  /** Every slot of `edition`, in order — the same table the page renderer resolves from. */
  slots: Position[]
  /** `groups(slots, mode, reading)` — reused, not re-derived, so the tiles pair exactly as the page view does. */
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
}

/**
 * One tile per group, laid out as `groups` paired them: a pair reads as a
 * pair, a joined spread as one wide tile. The current group is marked with
 * `aria-current` and a border. Regroup (dragging a tile to fix a pairing)
 * and correct (flagging a mismatch) are absent — later milestones' jobs.
 *
 * The rail is `overflowX: auto` with one tile per group — 187 of them for
 * `en-mono` — and nothing used to keep the current one on screen. Opening the
 * chrome at slot 150 showed tiles 0 through about 15, with the `aria-current`
 * tile off-screen and, since its
 * thumbnail is lazy-loaded, not even requested. `currentRef` is attached to
 * whichever tile is current and centred into view whenever that changes or
 * the strip becomes visible.
 */
export function Filmstrip({ work, edition, slots, groups, currentIndex, onNavigate, visible }: FilmstripProps) {
  const currentRef = useRef<HTMLButtonElement>(null)
  const railRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const recenter = () => currentRef.current?.scrollIntoView({ inline: 'center', block: 'nearest' })
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
  }, [currentIndex, visible])

  return (
    <div ref={railRef} style={railStyle}>
      {groups.map((group, index) => {
        const isCurrent = index === currentIndex
        return (
          <button
            key={group.slots[0]}
            ref={isCurrent ? currentRef : undefined}
            type="button"
            style={isCurrent ? currentTileStyle : tileStyle}
            aria-current={isCurrent ? 'true' : undefined}
            onClick={() => onNavigate(group.slots[0])}
          >
            {tilePages(group, slots).map(({ slot, page }) => (
              <img
                key={slot}
                src={thumbUrl(work, edition, page)}
                // 1-based, like `Chrome`'s `displaySlot` — this is read by a
                // person, and `page` itself (used for `thumbUrl` above)
                // stays the zero-based value the server expects.
                alt={`page ${page + 1}`}
                loading="lazy"
                style={tileImgStyle}
              />
            ))}
          </button>
        )
      })}
    </div>
  )
}

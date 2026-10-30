import type { LabelBox } from './labelling'

export interface RegionOverlayProps {
  boxes: readonly LabelBox[]
  selectedId?: string
  onHover?: (id: string | undefined) => void
  onSelect?: (id: string) => void
}

/**
 * Boxes drawn over a page image, in page fractions.
 *
 * **Two consumers, which is why this is its own component.** The labeller
 * creates and edits through it; the reader will later select through it for
 * the peek. Building it once is the reason the labeller is a second Vite
 * entry rather than a mode inside `App.tsx`.
 *
 * Percentages rather than pixels, so the overlay follows the page under zoom
 * and pan without recomputing anything — the same property that lets
 * `Annotations.tsx` position markers over artwork.
 *
 * Kind and ignore ride as data attributes rather than colour alone: a colour
 * difference is invisible to a test and to a reader who cannot distinguish
 * them.
 */
export function RegionOverlay({
  boxes,
  selectedId,
  onHover,
  onSelect,
}: RegionOverlayProps) {
  return (
    <div className="region-overlay">
      {boxes.map((box) => (
        // No interactive role or key handler on purpose: tab-cycling through
        // overlapping boxes is owned by the container (Labeller's cycleAt,
        // labelling.ts), one level up, which already knows which box is
        // current — a role and tabIndex here would put every box in the tab
        // order and fight it.
        // biome-ignore lint/a11y/noStaticElementInteractions: see above.
        // biome-ignore lint/a11y/useKeyWithClickEvents: see above.
        <div
          key={box.id}
          data-testid={`region-${box.id}`}
          data-kind={box.kind}
          // `null` is a real axis — a radial sound effect has no reading
          // direction — so it gets a value of its own rather than an absent
          // attribute, which would be indistinguishable from a box predating
          // the field. `styles/annotations.css` draws all three.
          data-axis={box.axis ?? 'none'}
          data-ignore={String(box.ignore)}
          data-selected={String(box.id === selectedId)}
          className="region-box"
          style={{
            position: 'absolute',
            left: `${box.x * 100}%`,
            top: `${box.y * 100}%`,
            width: `${box.w * 100}%`,
            height: `${box.h * 100}%`,
          }}
          onMouseEnter={() => onHover?.(box.id)}
          onMouseLeave={() => onHover?.(undefined)}
          onClick={() => onSelect?.(box.id)}
        />
      ))}
    </div>
  )
}

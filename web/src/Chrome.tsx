import type { CSSProperties } from 'react'
import { Filmstrip, type FilmstripProps } from './Filmstrip'

const readoutHeight = 32
const gapAboveReadout = 8
const stripHeight = 76
const inset = 16

// `position: absolute` against the nearest positioned ancestor — `App.tsx`'s
// root, which carries `position: relative` for exactly this — rather than a
// flex child of the toolbar/page-area column those sit in. An absolutely
// positioned box is removed from that flow entirely, so mounting or
// unmounting it cannot change the page area's size: the one property this
// component exists to guarantee. `overflow: hidden` here (not on the rail)
// is what clips the tiles to the rounded corners without also clipping the
// box-shadow, which paints from the border box outward and is unaffected by
// an element's own `overflow`.
const stripStyle: CSSProperties = {
  position: 'absolute',
  left: inset,
  right: inset,
  bottom: readoutHeight + gapAboveReadout,
  height: stripHeight,
  borderRadius: 12,
  overflow: 'hidden',
  background: 'rgba(24, 24, 24, 0.92)',
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
}

// The readout row docks to the bottom edge instead of floating — the strip
// above it is the one that is lifted off.
const readoutStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  bottom: 0,
  height: readoutHeight,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  fontSize: '0.85em',
  color: 'rgba(255, 255, 255, 0.85)',
  background: 'rgba(0, 0, 0, 0.6)',
}

export interface ChromeProps extends FilmstripProps {
  /** Whether the strip and the readout row are shown at all. */
  visible: boolean
  /**
   * 1-based position of the current slot, for the readout row only.
   *
   * Named `displaySlot` rather than `slot` so the divergence from every
   * zero-based slot elsewhere (`ReadingState.slot`, `Resolver.slotAt`, stored
   * progress, the server's range check) is visible at the call site, not only
   * in this comment — `App.tsx`'s `putProgress` wiring is the first code path
   * that could leak a display convention into a wire format, and a prop named
   * the same as the zero-based value it is not is the easiest way to do that
   * by accident.
   */
  displaySlot: number
  /** Total slots in the edition being viewed. */
  total: number
}

/**
 * The strip and the readout row — brought in and taken away together by
 * `Tab` (`keys.ts`'s `{ kind: 'chrome' }`), never separately. Returns `null`
 * while hidden, rather than e.g. `visibility: hidden`, so the hidden state
 * really is "no chrome at all" — the design's edge-to-edge reading with no
 * persistent chrome — and not merely invisible chrome still doing layout
 * work.
 *
 * The readout row carries only `slot N / M` here. Reading time, chapter and
 * alignment health are the `readouts` module, M5b — not built here.
 */
export function Chrome({ visible, displaySlot, total, ...filmstrip }: ChromeProps) {
  if (!visible) return null
  return (
    <>
      <div style={stripStyle}>
        {/* `visible` is always true here — `Chrome` returns `null` above
            otherwise — but passed through explicitly anyway, since
            `Filmstrip`'s own scroll-into-view effect is keyed on it; see
            `Filmstrip.tsx`'s doc for why. */}
        <Filmstrip {...filmstrip} visible={visible} />
      </div>
      <div style={readoutStyle} role="status">
        slot {displaySlot} / {total}
      </div>
    </>
  )
}

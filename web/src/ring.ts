/**
 * Where `Tab` goes next in the reader — spec 2026-09-26 §3.1.
 *
 * The ring is the page, then the bars' stops, then whatever the page itself
 * offers (a gap's moves, note tabs, the note list), then the page again;
 * `Shift+Tab` walks it backwards. It is a ring rather than a trap inside the
 * bars, so that pressing `Tab` on and on comes back to reading rather than
 * circling the bars.
 *
 * Pure, over counts, so it can be tested without a DOM. `focus.ts` reads the
 * counts off the page and moves focus; `App.tsx` puts the bars up or down to
 * match the answer.
 */
export type RingRegion = 'page' | 'bars' | 'controls'

/** A stop: a region, and a position in it. `-1` is the region's last stop. */
export interface RingStop {
  region: RingRegion
  index: number
}

/**
 * How many stops each region holds. `bars` is `undefined` while the bars are
 * not mounted — `Chrome` returns `null` while hidden, so their count is not
 * known until they are up.
 */
export interface RingSizes {
  bars: number | undefined
  controls: number
}

export const PAGE_STOP: RingStop = { region: 'page', index: 0 }

export function ringStep(
  at: RingStop,
  sizes: RingSizes,
  backwards: boolean,
): RingStop {
  const hasBars = sizes.bars !== 0
  const hasControls = sizes.controls > 0
  const firstBar: RingStop = { region: 'bars', index: 0 }
  const lastBar: RingStop = {
    region: 'bars',
    index: sizes.bars === undefined ? -1 : sizes.bars - 1,
  }
  const firstControl: RingStop = { region: 'controls', index: 0 }
  const lastControl: RingStop = {
    region: 'controls',
    index: sizes.controls - 1,
  }
  if (!backwards) {
    switch (at.region) {
      case 'page':
        return hasBars ? firstBar : hasControls ? firstControl : PAGE_STOP
      case 'bars':
        if (sizes.bars !== undefined && at.index + 1 < sizes.bars) {
          return { region: 'bars', index: at.index + 1 }
        }
        return hasControls ? firstControl : PAGE_STOP
      case 'controls':
        return at.index + 1 < sizes.controls
          ? { region: 'controls', index: at.index + 1 }
          : PAGE_STOP
    }
  }
  switch (at.region) {
    case 'page':
      return hasControls ? lastControl : hasBars ? lastBar : PAGE_STOP
    case 'controls':
      if (at.index > 0) return { region: 'controls', index: at.index - 1 }
      return hasBars ? lastBar : PAGE_STOP
    case 'bars':
      return at.index > 0 ? { region: 'bars', index: at.index - 1 } : PAGE_STOP
  }
}

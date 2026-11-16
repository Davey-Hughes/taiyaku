import type { SliderScale } from '../../src/settingsPanel'

/**
 * A declared slider that is not the cover size's: positions 0 to 10 at
 * `50 + 10p` ms, so a default of 100 is its middle and 60 is not.
 */
export const MS_SCALE: SliderScale = {
  last: 10,
  valueAt: (position) => 50 + 10 * position,
  positionOf: (value) =>
    Math.min(10, Math.max(0, Math.round((value - 50) / 10))),
  unit: 'ms',
  spokenUnit: 'milliseconds',
}

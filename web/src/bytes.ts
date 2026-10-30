/**
 * Sizes, printed for a person deciding whether something will fit.
 *
 * **Its own module rather than `backup.ts`'s**, since the confirmation screen
 * prints a total size too and importing the backup client into a curation
 * screen would couple two unrelated features through a few pure lines. A
 * second formatter is one that drifts.
 */
const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB']

/**
 * A size in the coarsest binary unit that does not lie about it.
 *
 * **KiB rather than kB**, because the divisor is 1024: a label saying kB over
 * a 1024-divisor is a figure that disagrees with `ls -l`. Bytes are drawn
 * whole; everything above takes one decimal, which is the precision an
 * operator deciding whether a file will fit actually reads.
 */
export function humanBytes(bytes: number): string {
  let size = bytes
  let unit = 0
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024
    unit += 1
  }
  return unit === 0 ? `${size} B` : `${size.toFixed(1)} ${UNITS[unit]}`
}

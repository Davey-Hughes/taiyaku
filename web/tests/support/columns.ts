/**
 * The columns a cover list (`.grid`, `.volumes`) is laid in, read from what
 * the browser drew: its content box's edges and width, its least gap (the
 * computed `column-gap`, the size's `--tile-gap`), and how many covers of
 * `size` px fit across at that gap, which is how many columns `library.css`
 * lays in either layout.
 */
export function columnsOf(list: Element, size: number) {
  const box = list.getBoundingClientRect()
  const style = getComputedStyle(list)
  const left =
    box.left + parseFloat(style.borderLeftWidth) + parseFloat(style.paddingLeft)
  const right =
    box.right -
    parseFloat(style.borderRightWidth) -
    parseFloat(style.paddingRight)
  const width = right - left
  const gap = parseFloat(style.columnGap)
  const n = Math.max(1, Math.floor((width + gap) / (size + gap)))
  return { left, right, width, gap, n }
}

/** `tiles`' boxes in rows, top to bottom: a grid row's items share a top. */
export function rowsOf(tiles: Element[]): DOMRect[][] {
  const rows = new Map<number, DOMRect[]>()
  for (const tile of tiles) {
    const box = tile.getBoundingClientRect()
    rows.set(box.top, [...(rows.get(box.top) ?? []), box])
  }
  return [...rows.entries()].sort(([a], [b]) => a - b).map(([, row]) => row)
}

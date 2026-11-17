/**
 * Where an open volume's panel lies on the Series page (O283): under its
 * tile, over the rows below, two tiles and the grid's gap wide, and never
 * past either edge of the page area. Read from the layout, since only the
 * grid knows which column a tile fell in and how wide the columns came out.
 */

/**
 * The panel's left edge and width: two tiles and the gap between them wide,
 * or the area's width where that is less; from the tile's left edge, or
 * back from its right edge where rightward would pass the area's end, which
 * only a row's last column needs; never left of the area, nor past its end.
 */
export function panelSpan(
  tile: { left: number; right: number; width: number },
  gap: number,
  area: { start: number; end: number },
): { left: number; width: number } {
  const wide = Math.min(2 * tile.width + gap, area.end - area.start)
  // A pixel of slack: the gap is read as the stylesheet writes it but laid
  // out in the layout's own units, so the second-to-last column's two tiles
  // and gap can sum a fraction past the end, where the last column's pass
  // it by a tile. The width is then held to the area, which that fraction
  // would pass, and the left edge to the area's start, which a panel the
  // area's width, laid back from a tile ending a fraction short of the
  // area's end, would pass.
  const left =
    tile.left + wide <= area.end + 1
      ? tile.left
      : Math.max(area.start, tile.right - wide)
  return { left, width: Math.min(wide, area.end - left) }
}

/**
 * The gap a grid `width` px wide lays its columns apart by, from its
 * resolved `grid-template-columns` (the columns' own widths, with any line
 * names): the width less the columns, over the gaps between them. Beside
 * the sidebar that is the gap the stylesheet writes and the spare room
 * `justify-content` spreads into the gaps (O285's exact-size covers). One
 * column has no gap between columns to read.
 */
export function laidGapOf(
  trackList: string,
  width: number,
): number | undefined {
  const tracks = trackList
    .split(' ')
    .map((track) => Number.parseFloat(track))
    .filter((track) => Number.isFinite(track))
  if (tracks.length < 2) return undefined
  const sum = tracks.reduce((total, track) => total + track, 0)
  return (width - sum) / (tracks.length - 1)
}

/** The grid's laid gap, or the gap its stylesheet writes where it has none. */
function laidGap(grid: HTMLElement, width: number): number {
  const style = getComputedStyle(grid)
  return (
    laidGapOf(style.gridTemplateColumns, width) ??
    Number.parseFloat(style.columnGap)
  )
}

/**
 * Lays `panel`, absolutely placed in `tile`, from the tile's grid and the
 * page area around it. Only a volume's tile on a page has columns to lie
 * across, so elsewhere it leaves the panel as its stylesheet draws it.
 */
export function layPanel(panel: HTMLElement, tile: HTMLElement): void {
  const grid = tile.parentElement
  if (grid === null || tile.closest('.library > main') === null) return
  const box = tile.getBoundingClientRect()
  // The grid fills the page area's content box, and its own box is laid out
  // to the fraction of a pixel where the area's `clientWidth` is rounded to
  // a whole one, which would end the area short of its last column.
  const area = grid.getBoundingClientRect()
  const gap = laidGap(grid, area.width)
  const { left, width } = panelSpan(box, gap, {
    start: area.left,
    end: area.right,
  })
  panel.style.left = `${left - box.left}px`
  panel.style.width = `${width}px`
}

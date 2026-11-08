/**
 * A rectangle as it is drawn, as `getBoundingClientRect` reads one. A
 * `DOMRect` is one and so is a plain object, so where a link's line goes
 * can be worked out, and tested, with no page laid out.
 */
export type Drawn = {
  readonly top: number
  readonly bottom: number
  readonly left: number
  readonly right: number
  readonly height: number
}

/** What `linkedPlace` reads of a library page's Directories box as drawn. */
export type LinkedLayout = {
  /** The add form's room, which the line is drawn in. */
  readonly room: Drawn
  /** The room's padding at its top and at its foot. */
  readonly padTop: number
  readonly padBottom: number
  /** The line as it is laid in the flow, with no place of its own. */
  readonly line: Drawn
  /** Every row of the box's table. */
  readonly tableRows: readonly Drawn[]
  /** What the room draws before the line: Add directory…, or the picker. */
  readonly roomRows: readonly Drawn[]
}

/**
 * Where a link's line is laid: `top`, from where the flow lays it, as a
 * `top` moves a box drawn `position: relative`; and `grows`, what it takes
 * beyond its words' height, 0 where that is half a pixel or less: layout rounds
 * to fractions of a pixel, and a line that fits must not be read as growing.
 */
export type LinkedPlace = { readonly top: number; readonly grows: number }

/**
 * Where a link's line whose words need more than its room is laid: over
 * its box's rows, each it reaches whole, words centred, as a callout (the
 * rulings of 2026-10-26). Under Add directory… and too tall for the room,
 * it lies over the rows above the room, the button in view, where they have
 * room for it. Else it is laid up from the room's content foot, over the
 * button where it is under it, as a refused Change password covers its
 * button. Nothing where its words fit as it is laid.
 */
export function linkedPlace(layout: LinkedLayout): LinkedPlace | undefined {
  const { room, line } = layout
  const foot = room.bottom - layout.padBottom
  if (line.bottom <= foot + 0.5) return undefined
  // A row drawn with no height, or one the line does not lie across, is
  // not under it.
  const across = (rows: readonly Drawn[]) =>
    rows.filter(
      (row) => row.height > 0 && row.right > line.left && row.left < line.right,
    )
  const tableRows = across(layout.tableRows)
  const roomRows = across(layout.roomRows)
  const fits = line.height <= foot - room.top - layout.padTop + 0.5
  const overAbove = wholeUp(tableRows, room.top, line.height)
  const contentTop = Math.min(...tableRows.map((row) => row.top))
  const [top, bottom] =
    !fits && roomRows.length > 0 && overAbove >= contentTop - 0.5
      ? [overAbove, room.top]
      : [wholeUp([...tableRows, ...roomRows], foot, line.height), foot]
  const grows = bottom - top - line.height
  return { top: top - line.top, grows: grows <= 0.5 ? 0 : grows }
}

/**
 * The top of what `needs` takes up from `bottom`, moved up to the top of
 * each of `rows` it reaches into, so that it covers each whole.
 */
function wholeUp(rows: readonly Drawn[], bottom: number, needs: number) {
  let top = bottom - needs
  for (const row of [...rows].sort((a, b) => b.bottom - a.bottom)) {
    if (row.top >= bottom - 0.5) continue
    if (row.bottom <= top + 0.5) break
    top = Math.min(top, row.top)
  }
  return top
}

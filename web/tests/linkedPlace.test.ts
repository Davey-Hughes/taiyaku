import { describe, expect, it } from 'vitest'
import { type Drawn, type LinkedLayout, linkedPlace } from '../src/linkedPlace'

/** A box drawn from `top` to `bottom`, across the line's width by default. */
const drawn = (top: number, bottom: number, left = 16, right = 374): Drawn => ({
  top,
  bottom,
  left,
  right,
  height: bottom - top,
})

/**
 * A Directories box at a phone's width: the table's head and two
 * directories' rows (0–40, 40–100, 100–160), then the add form's room with
 * Add directory… alone in it (160–200), the line laid in the flow under the
 * button, `height` tall. Each case changes what it names.
 */
const phone = (height: number, change: Partial<LinkedLayout> = {}) =>
  ({
    room: drawn(160, 200),
    padTop: 0,
    padBottom: 0,
    line: drawn(200, 200 + height),
    tableRows: [drawn(0, 40), drawn(40, 100), drawn(100, 160)],
    roomRows: [drawn(160, 200, 16, 150)],
    ...change,
  }) satisfies LinkedLayout

describe('linkedPlace', () => {
  it('gives no place to a line whose words fit in the room as it is laid', () => {
    // Beside Add directory…, as at a desktop's width, inside the room.
    expect(
      linkedPlace(phone(40, { line: drawn(160, 200, 160, 374) })),
    ).toBeUndefined()
  })

  it('lays a line that fits the room but not where it is laid up from the room’s foot, over the button, covering the room whole', () => {
    // The picker's room kept (160–240), the line under Add directory….
    expect(
      linkedPlace(phone(60, { room: drawn(160, 240) })),
      'drawn 160–240: moved up 40, 20 more than its words',
    ).toEqual({ top: -40, grows: 20 })
  })

  it('lays a line too tall for the room under Add directory… over the rows above the room, each whole, its foot at the room’s top, leaving the button in view', () => {
    // 100 tall: 60–160 reaches into the row at 40–100, so it covers that
    // row from its top.
    expect(
      linkedPlace(phone(100)),
      'drawn 40–160: moved up 160, 20 more than its words',
    ).toEqual({ top: -160, grows: 20 })
  })

  it('lays a line too tall for the rows above the room up from the room’s foot over the button instead, inside its box', () => {
    // 180 tall: 160 of rows above the room have no room for it.
    expect(
      linkedPlace(phone(180)),
      'drawn 0–200: from the table’s first row to the room’s foot',
    ).toEqual({ top: -200, grows: 20 })
  })

  it('counts only what the line lies across: beside Add directory…, not under it, a line too tall for the room is laid up from the room’s foot', () => {
    // As at a desktop's width: Add directory… at 16–150, the line at
    // 160–374 beside it, overflowing the room's foot.
    expect(
      linkedPlace(phone(70, { line: drawn(150, 220, 160, 374) })),
      'drawn 100–200: over the last row whole, not over the rows above the room',
    ).toEqual({ top: -50, grows: 30 })
  })

  it('skips a row drawn with no height, which marks no top of the table to lie up to', () => {
    // The table's first drawn row is at 40; an empty one at 0 above it.
    expect(
      linkedPlace(
        phone(130, {
          tableRows: [drawn(0, 0), drawn(40, 100), drawn(100, 160)],
        }),
      ),
      'drawn 40–200: up from the room’s foot, not 30–160, over the table’s top',
    ).toEqual({ top: -160, grows: 30 })
  })

  it('reads the room between its padding: its content’s foot, and its content’s height for what fits it', () => {
    // The room 150–208, its content 160–200; the rows above end at 150.
    const padded = (height: number) =>
      linkedPlace(
        phone(height, {
          room: drawn(150, 208),
          padTop: 10,
          padBottom: 8,
          tableRows: [drawn(0, 40), drawn(40, 90), drawn(90, 150)],
        }),
      )
    expect(
      padded(45),
      '45 does not fit 40: drawn 90–150, over the row above',
    ).toEqual({ top: -110, grows: 15 })
    expect(
      padded(30),
      '30 fits: drawn 160–200, up from the content’s foot',
    ).toEqual({ top: -40, grows: 10 })
  })

  it('takes no more room than its words where it would grow by half a pixel or less', () => {
    // 39.8 tall, up from the foot to the button's top at 160: 0.2 more.
    expect(linkedPlace(phone(39.8))).toEqual({ top: -40, grows: 0 })
  })
})

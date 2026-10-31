import { describe, expect, it } from 'vitest'
import { bSlots, fakeResolver, whole } from './support/fixtures'
import {
  afterDelete,
  anchorFrom,
  anchorNote,
  artOffset,
  changeShown,
  editsToWrite,
  exportName,
  gestureFor,
  grabNote,
  isBlank,
  isPoint,
  listPlacement,
  mergeOver,
  newNote,
  NOTHING_OPEN,
  nothingOpen,
  notesBySlot,
  notesHere,
  moveRegion,
  notesJson,
  openNote,
  paintedBox,
  pointAt,
  pointInSlot,
  pressTakesPan,
  regionContains,
  regionFromDrag,
  regionUnderDrag,
  reviewList,
  sameNotes,
  shownNote,
  spreadTabs,
  tabOffset,
  toggleReview,
  visibleNotes,
  withEdits,
  withNote,
  withoutNote,
  type Draft,
  type Note,
} from '../src/notes'

const note = (
  id: string,
  edition: string,
  slot: number,
  region?: Note['region'],
): Note => ({
  id,
  edition,
  slot,
  region,
  body: `note ${id}`,
  created: '2026-08-05T10:00:00Z',
  updated: '2026-08-05T10:00:00Z',
})

/** `a` has 8 whole slots, `b` has 9 (page 2 is a joined spread). */
const slots = { a: whole(8), b: bSlots }
/** `a` slot 3 pairs with `b` slot 4, and `a` slot 5 pairs with nothing. */
const pairs: Record<string, number> = {
  'a|b|0': 0,
  'a|b|1': 1,
  'a|b|2': 2,
  'a|b|3': 4,
  'a|b|4': 5,
  'a|b|6': 7,
  'a|b|7': 8,
  'b|a|0': 0,
  'b|a|1': 1,
  'b|a|2': 2,
  'b|a|4': 3,
  'b|a|5': 4,
  'b|a|7': 6,
  'b|a|8': 7,
}
const resolver = fakeResolver(slots, pairs)

describe('notesHere', () => {
  it('shows a note written on the edition being read, at its own slot', () => {
    const notes = [note('1', 'a', 3), note('2', 'a', 4)]
    expect(notesHere(notes, 'a', 3, resolver)).toEqual([
      { note: notes[0], travelled: false },
    ])
  })

  it('travels a note written on another edition onto the slot it resolves to', () => {
    const notes = [note('1', 'a', 3)]
    // `a` 3 pairs with `b` 4, so reading `b` at slot 4 shows it, travelled.
    expect(notesHere(notes, 'b', 4, resolver)).toEqual([
      { note: notes[0], travelled: true },
    ])
    expect(notesHere(notes, 'b', 3, resolver)).toEqual([])
  })

  it('does not travel a note whose slot has no counterpart here', () => {
    // `a` 5 pairs with nothing in `b`.
    expect(notesHere([note('1', 'a', 5)], 'b', 5, resolver)).toEqual([])
  })

  it('shows a local and a travelled note together, local first', () => {
    const local = note('local', 'b', 4)
    const far = note('far', 'a', 3)
    const here = notesHere([far, local], 'b', 4, resolver)
    expect(here.map((h) => h.note.id)).toEqual(['local', 'far'])
  })

  // The round trip the design names as a seam test, exhaustive rather than
  // sampled: a note written on any slot of `a` must be visible at exactly the
  // slot of `b` its counterpart names, and nowhere at all where it has none.
  it('round-trips every note-bearing slot of one edition into the other and back', () => {
    for (let slot = 0; slot < slots.a.length; slot++) {
      const written = note(`n${slot}`, 'a', slot)
      const landing = Array.from(
        { length: slots.b.length },
        (_, at) => at,
      ).filter((at) => notesHere([written], 'b', at, resolver).length === 1)
      const counterpart = pairs[`a|b|${slot}`]
      if (counterpart === undefined) {
        expect(
          landing,
          `a ${slot} has no counterpart, so it must land nowhere`,
        ).toEqual([])
      } else {
        expect(landing, `a ${slot} must land on exactly one slot of b`).toEqual(
          [counterpart],
        )
      }
      // Wherever it travels or does not, it is always visible where it was
      // written — a note that could vanish from its own edition would be the
      // worse failure by far.
      expect(notesHere([written], 'a', slot, resolver)).toHaveLength(1)
    }
  })

  it('is empty rather than resolving anything when there are no notes at all', () => {
    expect(notesHere([], 'b', 4, resolver)).toEqual([])
  })
})

describe('reviewList', () => {
  it('gives every note a landing slot here, or says it has none', () => {
    const notes = [
      note('here', 'a', 3),
      note('gone', 'a', 5),
      note('local', 'b', 0),
    ]
    expect(reviewList(notes, 'b', resolver)).toEqual([
      { note: notes[0], here: 4 },
      { note: notes[1], here: undefined },
      { note: notes[2], here: 0 },
    ])
  })

  // Silence is the failure this project is written against: a note that cannot
  // be reached from here must still be in the list, marked, not omitted.
  it('keeps an unreachable note in the list rather than dropping it', () => {
    expect(reviewList([note('gone', 'a', 5)], 'b', resolver)).toHaveLength(1)
  })

  // `notesHere` resolves once per edition, outward from the edition being
  // read; `reviewList` resolves once per note, from each note's own edition.
  // The two directions must agree, or a note would appear on the page and be
  // called unreachable in the list — the drift a second implementation of
  // travel exists to be checked against.
  it('agrees with notesHere on every slot of every edition', () => {
    const notes = Array.from({ length: slots.a.length }, (_, slot) =>
      note(`n${slot}`, 'a', slot),
    )
    for (const viewing of ['a', 'b'] as const) {
      const entries = reviewList(notes, viewing, resolver)
      for (const entry of entries) {
        const landing = Array.from(
          { length: slots[viewing].length },
          (_, at) => at,
        ).filter((at) =>
          notesHere([entry.note], viewing, at, resolver).some(
            (h) => h.note.id === entry.note.id,
          ),
        )
        expect(landing, `${entry.note.id} viewed from ${viewing}`).toEqual(
          entry.here === undefined ? [] : [entry.here],
        )
      }
    }
  })
})

describe('notesBySlot', () => {
  // Scroll mode mounts every group at once, so asking "which notes are on this
  // slot" per rendered slot would be one scan per page, each resolving.
  // Inverting it costs one resolve per *note*, of which there are a handful,
  // whatever the volume's length.
  it('groups every note under the slot it lands on here', () => {
    const notes = [
      note('a3', 'a', 3),
      note('a4', 'a', 4),
      note('local', 'b', 4),
    ]
    const map = notesBySlot(notes, 'b', resolver)
    // `a` 3 → `b` 4, `a` 4 → `b` 5, and the local note is already on `b` 4.
    expect(map.get(4)?.map((h) => h.note.id)).toEqual(['local', 'a3'])
    expect(map.get(5)?.map((h) => h.note.id)).toEqual(['a4'])
  })

  it('leaves out a note that lands nowhere here rather than bucketing it at 0', () => {
    expect([
      ...notesBySlot([note('gone', 'a', 5)], 'b', resolver).keys(),
    ]).toEqual([])
  })

  // The third implementation of "where does this note go" has to agree with
  // the other two, or scroll mode would draw a tab on a slot single mode does
  // not — the same drift the `notesHere`/`reviewList` agreement test exists
  // to catch, extended to the one that renders a whole volume at once.
  it('agrees with notesHere on every slot of every edition', () => {
    const notes = [
      ...Array.from({ length: slots.a.length }, (_, slot) =>
        note(`a${slot}`, 'a', slot),
      ),
      ...Array.from({ length: slots.b.length }, (_, slot) =>
        note(`b${slot}`, 'b', slot),
      ),
    ]
    for (const viewing of ['a', 'b'] as const) {
      const map = notesBySlot(notes, viewing, resolver)
      for (let slot = 0; slot < slots[viewing].length; slot++) {
        const direct = notesHere(notes, viewing, slot, resolver)
        expect(
          (map.get(slot) ?? [])
            .map((h) => `${h.note.id}:${h.travelled}`)
            .sort(),
          `${viewing} slot ${slot}`,
        ).toEqual(direct.map((h) => `${h.note.id}:${h.travelled}`).sort())
      }
    }
  })
})

describe('tabOffset', () => {
  it('puts a whole-slot note at the middle of the margin', () => {
    expect(tabOffset(note('1', 'a', 0))).toBe(0.5)
  })

  it('puts a region note at the middle of its own region', () => {
    expect(
      tabOffset(note('1', 'a', 0, { x: 0.1, y: 0.2, w: 0.3, h: 0.4 })),
    ).toBeCloseTo(0.4)
  })
})

describe('regionFromDrag', () => {
  it('normalises a drag in any direction into a positive rectangle', () => {
    const forward = regionFromDrag({ x: 0.2, y: 0.3 }, { x: 0.5, y: 0.7 })
    const backward = regionFromDrag({ x: 0.5, y: 0.7 }, { x: 0.2, y: 0.3 })
    expect(forward.x).toBeCloseTo(0.2)
    expect(forward.y).toBeCloseTo(0.3)
    expect(forward.w).toBeCloseTo(0.3)
    expect(forward.h).toBeCloseTo(0.4)
    expect(backward).toEqual(forward)
  })

  it('clamps to the slot, so a drag off the edge stays a legal region', () => {
    expect(regionFromDrag({ x: -0.5, y: 0.5 }, { x: 1.5, y: 1.5 })).toEqual({
      x: 0,
      y: 0.5,
      w: 1,
      h: 0.5,
    })
  })
})

describe('paintedBox', () => {
  // `PageView` draws a page with `object-fit: contain`, so the painted image
  // is inset within its element whenever the aspects differ. A region measured
  // against the element rather than the painted image would be offset by the
  // letterbox — the whole page would drift, and every tint with it.
  it('centres a tall page inside a wide box, leaving side letterbox', () => {
    expect(paintedBox({ w: 100, h: 200 }, { w: 400, h: 200 })).toEqual({
      x: 150,
      y: 0,
      w: 100,
      h: 200,
    })
  })

  it('centres a wide page inside a tall box, leaving letterbox above and below', () => {
    expect(paintedBox({ w: 200, h: 100 }, { w: 200, h: 400 })).toEqual({
      x: 0,
      y: 150,
      w: 200,
      h: 100,
    })
  })

  it('fills exactly when the aspects match, with no letterbox at all', () => {
    expect(paintedBox({ w: 100, h: 200 }, { w: 200, h: 400 })).toEqual({
      x: 0,
      y: 0,
      w: 200,
      h: 400,
    })
  })

  // An image whose size is not known yet — `naturalWidth` is 0 until it
  // decodes — must not divide by zero and hand back NaN fractions that then
  // reach the server as a region.
  it('falls back to the whole box for an image whose size is not known yet', () => {
    expect(paintedBox({ w: 0, h: 0 }, { w: 300, h: 400 })).toEqual({
      x: 0,
      y: 0,
      w: 300,
      h: 400,
    })
  })

  // The four above describe a page sized to its own content, where the element
  // and the artwork differ by a sliver. `fit.ts`'s `upscaleToFit` defaults on
  // and makes the element the whole cell, so the difference is most of the
  // element. The two geometries below are that case.
  const scan = { w: 2147, h: 3056 }
  /** Single mode: the element is the whole page area. */
  const area = { w: 1385, h: 475 }
  /** Two-page mode: each cell is half of it. */
  const cell = { w: 692.5, h: 475 }

  it('insets the artwork inside an element far larger than it, in both axes', () => {
    const art = paintedBox(scan, area)
    expect(art.w).toBeCloseTo(334, 0)
    expect(art.h).toBe(475)
    expect(art.x).toBeCloseTo(526, 0)
    expect(art.y).toBe(0)
  })

  it('insets it above and below when the element is the taller of the two', () => {
    // The other axis of the same case: a wide page upscaled into a square
    // element fills the width and leaves background top and bottom.
    expect(paintedBox({ w: 200, h: 100 }, { w: 400, h: 400 })).toEqual({
      x: 0,
      y: 100,
      w: 400,
      h: 200,
    })
  })

  // `pageStyle`'s `hug` closes the two-page seam with `object-position`, so a
  // pair's two halves are pushed against each other and the leftover is not
  // split evenly.
  it('pushes the artwork to the right edge for the left cell of a pair', () => {
    expect(paintedBox(scan, cell, 'right-edge').x).toBeCloseTo(359, 0)
  })

  it('pushes the artwork to the left edge for the right cell of a pair', () => {
    // 0 within the cell, and the cell's own left edge is at 692.5.
    expect(paintedBox(scan, cell, 'left-edge').x).toBe(0)
  })

  it('splits the leftover evenly when nothing is hugged', () => {
    expect(paintedBox(scan, cell, 'centre').x).toBeCloseTo(179, 0)
  })

  it('never hugs the vertical, whatever the horizontal does', () => {
    // `pageStyle` writes `50%` for the vertical component of all three
    // positions — `0% 50%`, `100% 50%`, `50% 50%` — so a hug can only ever
    // move the artwork sideways.
    for (const hug of ['centre', 'left-edge', 'right-edge'] as const)
      expect(paintedBox(scan, cell, hug).y).toBe(0)
    for (const hug of ['centre', 'left-edge', 'right-edge'] as const)
      expect(paintedBox({ w: 200, h: 100 }, { w: 400, h: 400 }, hug).y).toBe(
        100,
      )
  })

  it('falls back to the whole box for an undecoded image, hug or no hug', () => {
    // The guard comes first: there is no artwork placed yet, so there is no
    // leftover for a hug to push anywhere.
    expect(
      paintedBox({ w: 0, h: 0 }, { w: 300, h: 400 }, 'right-edge'),
    ).toEqual({ x: 0, y: 0, w: 300, h: 400 })
  })

  it('answers exactly as it did before `hug` existed when nothing hugs', () => {
    // Every caller outside a two-page pair passes no hug at all, and the four
    // expectations at the top of this block are what they must keep getting.
    expect(paintedBox({ w: 100, h: 200 }, { w: 400, h: 200 })).toEqual(
      paintedBox({ w: 100, h: 200 }, { w: 400, h: 200 }, 'centre'),
    )
    expect(paintedBox({ w: 100, h: 200 }, { w: 400, h: 200 })).toEqual({
      x: 150,
      y: 0,
      w: 100,
      h: 200,
    })
  })
})

describe('artOffset', () => {
  // `Annotations.tsx`'s `measure` gives this the page area's own rect as
  // `outer`, the `<img>`'s as `inner`, and `paintedBox`'s inset as `art`.
  it('is just the artwork inset when the host and the image share an origin', () => {
    // The case every one of `Annotations.test.tsx`'s fixtures happens to be:
    // `inner.left - outer.left` is 0, so the element-to-element term
    // vanishes and only `art` shows up. Wrong order or wrong sign here would
    // answer exactly the same thing.
    expect(
      artOffset({ left: 10, top: 20 }, { left: 10, top: 20 }, { x: 5, y: 8 }),
    ).toEqual({ left: 5, top: 8 })
  })

  it('folds in the element-to-element offset when the two rects do not coincide', () => {
    // `outer` at (100, 50), `inner` at (90, 70): the image sits 10px left of
    // and 20px below the host it is measured against.
    expect(
      artOffset({ left: 100, top: 50 }, { left: 90, top: 70 }, { x: 5, y: 0 }),
    ).toEqual({ left: -5, top: 20 })
  })

  it('takes the offset as inner minus outer, not outer minus inner', () => {
    // Swapping the two operands — the mutation a matching-origin fixture
    // cannot catch — disagrees with the correct answer as soon as the
    // origins differ.
    const outer = { left: 100, top: 50 }
    const inner = { left: 90, top: 70 }
    const art = { x: 5, y: 0 }
    expect(artOffset(outer, inner, art)).toEqual({ left: -5, top: 20 })
    expect(artOffset(inner, outer, art)).toEqual({ left: 15, top: -20 })
  })
})

describe('pointInSlot', () => {
  it('reads a point inside the painted image as fractions of it', () => {
    const painted = { x: 150, y: 0, w: 100, h: 200 }
    expect(pointInSlot({ x: 200, y: 100 }, painted)).toEqual({ x: 0.5, y: 0.5 })
  })

  it('reports a point in the letterbox as outside, for the drag to clamp', () => {
    const painted = { x: 150, y: 0, w: 100, h: 200 }
    expect(pointInSlot({ x: 0, y: 100 }, painted).x).toBeLessThan(0)
  })

  it('reads the seam edge of a hugged page as the page’s own edge', () => {
    // The left cell of a two-page pair: its artwork is flush against the seam,
    // so the far edge of the cell *is* the far edge of the page. Halving the
    // leftover instead would read that same pixel as past the page's edge.
    const painted = paintedBox(
      { w: 2147, h: 3056 },
      { w: 692.5, h: 475 },
      'right-edge',
    )
    expect(pointInSlot({ x: 692.5, y: 237.5 }, painted).x).toBeCloseTo(1, 5)
  })

  it('does not divide by a zero-sized paint', () => {
    expect(pointInSlot({ x: 5, y: 5 }, { x: 0, y: 0, w: 0, h: 0 })).toEqual({
      x: 0,
      y: 0,
    })
  })
})

describe('spreadTabs', () => {
  // Two whole-page notes on one slot both anchor at 0.5, so their tabs land
  // exactly on top of each other and a reader sees one marker where there
  // are two.
  it('separates tabs that would sit on top of each other', () => {
    const spread = spreadTabs([0.5, 0.5, 0.5], 0.06)
    expect(spread[1] - spread[0]).toBeCloseTo(0.06)
    expect(spread[2] - spread[1]).toBeCloseTo(0.06)
  })

  it('leaves tabs that are already far enough apart exactly where they are', () => {
    expect(spreadTabs([0.1, 0.5, 0.9], 0.06)).toEqual([0.1, 0.5, 0.9])
  })

  // Order in, order out: the caller pairs these back up with its own list, so
  // a sorted return would silently attach each tab to the wrong note.
  it('answers in the order it was given, not in sorted order', () => {
    expect(spreadTabs([0.9, 0.1], 0.06)).toEqual([0.9, 0.1])
  })

  it('nudges the last one up rather than pushing it off the bottom', () => {
    const spread = spreadTabs([0.98, 0.99, 1], 0.06)
    expect(Math.max(...spread)).toBeLessThanOrEqual(1)
    // Still all distinct, which is the whole point.
    expect(new Set(spread).size).toBe(3)
  })
})

describe('regionContains', () => {
  const region = { x: 0.2, y: 0.2, w: 0.4, h: 0.4 }

  it('is true inside and on the edge', () => {
    expect(regionContains(region, { x: 0.4, y: 0.4 })).toBe(true)
    expect(regionContains(region, { x: 0.2, y: 0.2 })).toBe(true)
    expect(regionContains(region, { x: 0.6, y: 0.6 })).toBe(true)
  })

  it('is false outside', () => {
    expect(regionContains(region, { x: 0.19, y: 0.4 })).toBe(false)
    expect(regionContains(region, { x: 0.4, y: 0.61 })).toBe(false)
  })

  // What decides whether a drag moves the region already there or draws a new
  // one, so getting it wrong means a reader who meant to move ends up
  // replacing.
  it('answers false for a region that is not there at all', () => {
    expect(regionContains(undefined, { x: 0.4, y: 0.4 })).toBe(false)
  })
})

describe('moveRegion', () => {
  const region = { x: 0.2, y: 0.2, w: 0.4, h: 0.3 }

  it('translates by the offset given', () => {
    // `toBeCloseTo` rather than `toEqual`: 0.2 + 0.1 is 0.30000000000000004 in
    // binary floating point, and a region is a display fraction — nothing here
    // needs the exact decimal, and asserting it would pin an arithmetic
    // accident rather than the behaviour.
    const moved = moveRegion(region, 0.1, -0.05)
    expect(moved.x).toBeCloseTo(0.3)
    expect(moved.y).toBeCloseTo(0.15)
    expect(moved.w).toBe(0.4)
    expect(moved.h).toBe(0.3)
  })

  // Clamped as a whole rather than per edge: a region dragged off the page
  // stops at the edge keeping its size, where clamping x and y independently
  // would shrink it against the boundary — a reader dragging past the corner
  // would watch their box quietly get smaller.
  it('stops at the edge without changing size', () => {
    expect(moveRegion(region, 5, 5)).toEqual({ x: 0.6, y: 0.7, w: 0.4, h: 0.3 })
    expect(moveRegion(region, -5, -5)).toEqual({ x: 0, y: 0, w: 0.4, h: 0.3 })
  })

  it('leaves a region that fills the slot exactly where it is', () => {
    const whole = { x: 0, y: 0, w: 1, h: 1 }
    expect(moveRegion(whole, 0.3, 0.3)).toEqual(whole)
  })
})

describe('isBlank', () => {
  // Pressing `n` and then going off to nudge somebody else's box leaves a note
  // that was never written in and never marked anything. Saving it puts an
  // empty marker on the page for a keystroke the reader had already moved on
  // from.
  it('is true for a note with neither words nor a region', () => {
    expect(isBlank(note('1', 'a', 0))).toBe(false) // the fixture has a body
    expect(isBlank({ ...note('1', 'a', 0), body: '' })).toBe(true)
    expect(isBlank({ ...note('1', 'a', 0), body: '   ' })).toBe(true)
  })

  // Either one alone is a real note: a marked area with the words still to
  // come, or a note about the whole page.
  it('is false once it has words or a region', () => {
    expect(isBlank({ ...note('1', 'a', 0), body: 'something' })).toBe(false)
    expect(
      isBlank({
        ...note('1', 'a', 0, { x: 0, y: 0, w: 0.5, h: 0.5 }),
        body: '',
      }),
    ).toBe(false)
  })
})

describe('isPoint / pointAt', () => {
  it('is a point when it has no area at all', () => {
    expect(isPoint({ x: 0.3, y: 0.4, w: 0, h: 0 })).toBe(true)
    expect(isPoint({ x: 0.3, y: 0.4, w: 0.1, h: 0 })).toBe(false)
    expect(isPoint(undefined)).toBe(false)
  })

  it('clamps a point to the slot it is on', () => {
    expect(pointAt({ x: 1.4, y: -0.2 })).toEqual({ x: 1, y: 0, w: 0, h: 0 })
  })
})

describe('regionContains with slop', () => {
  // A point has no area, so without slop the only way to grab one would be to
  // hit an infinitely small spot. The caller passes the radius its markers are
  // drawn at, so what can be grabbed is what can be seen.
  it('lets a point be grabbed within the slop', () => {
    const point = { x: 0.5, y: 0.5, w: 0, h: 0 }
    expect(regionContains(point, { x: 0.5, y: 0.5 })).toBe(true)
    expect(regionContains(point, { x: 0.52, y: 0.5 })).toBe(false)
    expect(regionContains(point, { x: 0.52, y: 0.5 }, 0.03)).toBe(true)
    expect(regionContains(point, { x: 0.6, y: 0.5 }, 0.03)).toBe(false)
  })
})

describe('listPlacement', () => {
  it('follows an explicit setting whatever the width', () => {
    expect(listPlacement('side', 400)).toBe('side')
    expect(listPlacement('below', 3000)).toBe('below')
  })

  it('puts the list beside the page where there is room, and under it where there is not', () => {
    expect(listPlacement('auto', 1600)).toBe('side')
    expect(listPlacement('auto', 800)).toBe('below')
  })

  it('treats a setting it does not recognise as auto rather than refusing', () => {
    expect(listPlacement('sideways', 1600)).toBe('side')
    expect(listPlacement('sideways', 800)).toBe('below')
  })
})

describe('withNote / withoutNote', () => {
  it('replaces a note with the same id rather than adding a second', () => {
    const first = note('1', 'a', 0)
    const edited = { ...first, body: 'changed' }
    expect(withNote([first], edited)).toEqual([edited])
  })

  it('appends a note whose id is new', () => {
    expect(withNote([note('1', 'a', 0)], note('2', 'a', 1))).toHaveLength(2)
  })

  it('removes by id and leaves the rest alone', () => {
    const notes = [note('1', 'a', 0), note('2', 'a', 1)]
    expect(withoutNote(notes, '1')).toEqual([notes[1]])
  })
})

describe('gestureFor', () => {
  const box = note('box', 'a', 0, { x: 0.2, y: 0.2, w: 0.3, h: 0.3 })
  const inner = note('inner', 'a', 0, { x: 0.25, y: 0.25, w: 0.1, h: 0.1 })
  const point = note('point', 'a', 0, { x: 0.8, y: 0.8, w: 0, h: 0 })
  const far = note('far', 'b', 0, { x: 0.2, y: 0.2, w: 0.3, h: 0.3 })
  const local = (n: Note) => ({ note: n, travelled: false })
  const press = (over: Partial<Parameters<typeof gestureFor>[0]> = {}) =>
    gestureFor({
      editing: true,
      onAnImage: true,
      onTheDraftsPage: true,
      at: { x: 0.5, y: 0.5 },
      here: [],
      slop: 0.02,
      ...over,
    })

  it('does nothing at all while the editor is closed', () => {
    expect(press({ editing: false, onAnImage: false })).toEqual({
      kind: 'ignore',
    })
  })

  it('finishes the note when the press lands off the page', () => {
    expect(press({ onAnImage: false })).toEqual({ kind: 'close' })
  })

  // Reading "not this one" as "not any" would end a note because a reader
  // touched the facing page of a spread.
  it('ignores a press on another page rather than finishing', () => {
    expect(press({ onTheDraftsPage: false })).toEqual({ kind: 'ignore' })
  })

  it('draws a new region where the press lands on no existing one', () => {
    // Clear of `box`, which spans .2 to .5 in both axes — the default press
    // point of .5,.5 sits on its very corner.
    expect(press({ at: { x: 0.7, y: 0.3 }, here: [local(box)] })).toEqual({
      kind: 'draw',
    })
  })

  it('moves the region the press landed in', () => {
    expect(press({ at: { x: 0.3, y: 0.3 }, here: [local(box)] })).toEqual({
      kind: 'move',
      noteId: 'box',
      from: box.region,
    })
  })

  // Last drawn is on top, so a box inside another is the one you get.
  it('takes the innermost of two overlapping regions', () => {
    expect(
      press({ at: { x: 0.3, y: 0.3 }, here: [local(box), local(inner)] }),
    ).toEqual({
      kind: 'move',
      noteId: 'inner',
      from: inner.region,
    })
  })

  // A point has no area; without slop there would be nothing to take hold of.
  it('grabs a point from within the slop', () => {
    expect(press({ at: { x: 0.81, y: 0.81 }, here: [local(point)] }).kind).toBe(
      'move',
    )
    expect(press({ at: { x: 0.9, y: 0.9 }, here: [local(point)] }).kind).toBe(
      'draw',
    )
  })

  // Its region belongs to another edition's slot; dragging it here would move
  // something that is not on this page.
  it('never grabs a travelled note', () => {
    expect(
      press({ at: { x: 0.3, y: 0.3 }, here: [{ note: far, travelled: true }] }),
    ).toEqual({
      kind: 'draw',
    })
  })
})

describe('editsToWrite', () => {
  const stored = [note('kept', 'a', 0), note('edited', 'a', 1)]

  it('lays edits over the stored notes', () => {
    const changed = { ...stored[1], body: 'changed' }
    expect(editsToWrite(stored, { edited: changed })).toEqual([
      stored[0],
      changed,
    ])
  })

  it('adds a note this session invented once it says something', () => {
    const fresh = note('new', 'a', 2)
    expect(editsToWrite(stored, { new: fresh })).toHaveLength(3)
  })

  // Press `n`, wander off to move another box, and the note `n` made was never
  // written in — an invisible marker for a keystroke already moved on from.
  it('drops a note invented this session and never filled in', () => {
    const blank = { ...note('blank', 'a', 2), body: '', region: undefined }
    expect(editsToWrite(stored, { blank })).toEqual(stored)
  })

  // Emptying an existing note is an edit; deleting one is a button.
  it('keeps an existing note that has been emptied', () => {
    const emptied = { ...stored[0], body: '' }
    expect(editsToWrite(stored, { kept: emptied })).toEqual([
      emptied,
      stored[1],
    ])
  })
})

describe('mergeOver', () => {
  // What a stale-validator retry sends. Automatic saving means there is nobody
  // standing there to be asked to try again, so the write has to decide — and
  // the decision is that the other device's other notes are none of our
  // business.
  it('keeps every note the other device wrote that this reader did not touch', () => {
    const theirs = [note('theirs', 'a', 5)]
    const mine = [note('mine', 'a', 6)]
    expect(
      mergeOver(theirs, mine)
        .map((n) => n.id)
        .sort(),
    ).toEqual(['mine', 'theirs'])
  })

  it("takes this reader's copy of a note both of them edited", () => {
    const theirs = [{ ...note('both', 'a', 5), body: 'theirs' }]
    const mine = [{ ...note('both', 'a', 5), body: 'mine' }]
    expect(mergeOver(theirs, mine)).toEqual(mine)
  })

  it('does not lose a note the other device deleted and this one still holds', () => {
    expect(mergeOver([], [note('mine', 'a', 0)])).toHaveLength(1)
  })
})

describe('export', () => {
  it('names the file after the work, so two exports do not collide', () => {
    expect(exportName('go-toubun-v01')).toBe('go-toubun-v01.notes.json')
  })

  it('exports exactly what the server holds, formatted to be read', () => {
    const doc = { work: 'w', notes: [note('1', 'a', 0)] }
    expect(JSON.parse(notesJson(doc))).toEqual(doc)
    expect(notesJson(doc)).toContain('\n  ')
  })
})

// ---- the editing session ---------------------------------------------------

/** A draft holding `notes`, showing the last of them, with the editor open. */
const editing = (...notes: Note[]): Draft => ({
  panel: 'edit',
  edits: Object.fromEntries(notes.map((n) => [n.id, n])),
  currentId: notes.at(-1)?.id,
})

const here = (...notes: Note[]) =>
  notes.map((note) => ({ note, travelled: false }))

describe('shownNote', () => {
  it('is the note the panel is showing', () => {
    const a = note('a', 'a', 0)
    expect(shownNote(editing(a))).toEqual(a)
  })

  it('is nothing with nothing open, and nothing for an id that went away', () => {
    expect(shownNote(NOTHING_OPEN)).toBeUndefined()
    expect(
      shownNote({ panel: 'edit', edits: {}, currentId: 'gone' }),
    ).toBeUndefined()
  })
})

describe('newNote', () => {
  const at = { edition: 'a', slot: 4 }

  it('opens the editor on a note anchored where the reader is standing', () => {
    const draft = newNote(NOTHING_OPEN, at, [], 'fresh', '2026-08-09T00:00:00Z')
    expect(draft.panel).toBe('edit')
    expect(draft.currentId).toBe('fresh')
    expect(shownNote(draft)).toEqual({
      id: 'fresh',
      edition: 'a',
      slot: 4,
      color: 'amber',
      body: '',
      created: '2026-08-09T00:00:00Z',
      updated: '2026-08-09T00:00:00Z',
    })
  })

  // Always a new note, however many the slot already holds — reopening the one
  // already there would make a second note on a page impossible.
  it('keeps the notes this session already touched', () => {
    const held = note('held', 'a', 4)
    const draft = newNote(editing(held), at, [], 'fresh', 'now')
    expect(Object.keys(draft.edits).sort()).toEqual(['fresh', 'held'])
  })

  // Two notes on one page come out different without a reader choosing.
  it('takes the next colour nobody on this page is using', () => {
    const used = { ...note('used', 'a', 4), color: 'amber' }
    const draft = newNote(NOTHING_OPEN, at, here(used), 'fresh', 'now')
    expect(shownNote(draft)?.color).not.toBe('amber')
  })
})

describe('openNote', () => {
  const stored = [note('stored', 'a', 0)]

  it('brings a stored note in and shows it', () => {
    const draft = openNote(NOTHING_OPEN, 'stored', stored)
    expect(draft.panel).toBe('edit')
    expect(shownNote(draft)).toEqual(stored[0])
  })

  // Clicking back onto a note moved a moment ago shows it where it was left,
  // not where the server still has it.
  it("prefers this session's copy over the stored one", () => {
    const moved = { ...stored[0], body: 'moved' }
    expect(shownNote(openNote(editing(moved), 'stored', stored))).toEqual(moved)
  })

  it('hands the same draft back for an id neither side knows', () => {
    expect(openNote(NOTHING_OPEN, 'nobody', stored)).toBe(NOTHING_OPEN)
  })
})

describe('changeShown', () => {
  it('changes the note on show and leaves the rest alone', () => {
    const other = note('other', 'a', 0)
    const shown = note('shown', 'a', 1)
    const draft = changeShown(editing(other, shown), (n) => ({
      ...n,
      body: 'typed',
    }))
    expect(shownNote(draft)?.body).toBe('typed')
    expect(draft.edits.other).toEqual(other)
  })

  // Identity is how the caller tells a real change from a no-op, and it is the
  // caller that schedules the write.
  it('hands the same draft back when nothing is on show', () => {
    expect(changeShown(NOTHING_OPEN, (n) => n)).toBe(NOTHING_OPEN)
    const stale: Draft = { panel: 'edit', edits: {}, currentId: 'gone' }
    expect(changeShown(stale, (n) => n)).toBe(stale)
  })
})

describe('grabNote', () => {
  const grabbed = note('grabbed', 'a', 0)

  it('brings the grabbed note in and makes it the one on show', () => {
    const draft = grabNote(NOTHING_OPEN, grabbed)
    expect(draft.currentId).toBe('grabbed')
    expect(shownNote(draft)).toEqual(grabbed)
  })

  // Grabbing a box moved earlier continues from where it is now.
  it("keeps this session's copy of a note it has already touched", () => {
    const moved = { ...grabbed, body: 'moved' }
    expect(shownNote(grabNote(editing(moved), grabbed))).toEqual(moved)
  })

  it('leaves the panel alone — a grab happens with the editor already open', () => {
    expect(grabNote({ ...NOTHING_OPEN, panel: 'review' }, grabbed).panel).toBe(
      'review',
    )
  })
})

describe('anchorNote', () => {
  const region = { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }

  // The note a drag ends on is not always the one the panel was showing when
  // it began.
  it('anchors by id rather than by what is on show', () => {
    const dragged = note('dragged', 'a', 0)
    const shown = note('shown', 'a', 1)
    const draft = anchorNote(
      editing(dragged, shown),
      'dragged',
      region,
      'later',
    )
    expect(draft.edits.dragged).toEqual({
      ...dragged,
      region,
      updated: 'later',
    })
    expect(draft.edits.shown).toEqual(shown)
    expect(draft.currentId).toBe('shown')
  })

  it('hands the same draft back for a note this session never had', () => {
    expect(anchorNote(NOTHING_OPEN, 'nobody', region, 'later')).toBe(
      NOTHING_OPEN,
    )
  })
})

describe('toggleReview', () => {
  it('opens the list, and closes it when it is what is open', () => {
    const open = toggleReview(NOTHING_OPEN)
    expect(open.panel).toBe('review')
    expect(toggleReview(open).panel).toBeUndefined()
  })

  // `a` always means "show me the list" rather than sometimes meaning "close".
  it('swaps the editor for the list rather than closing', () => {
    expect(toggleReview({ ...NOTHING_OPEN, panel: 'edit' }).panel).toBe(
      'review',
    )
  })

  it('leaves the edits alone — swapping surfaces is not finishing', () => {
    const held = note('held', 'a', 0)
    expect(toggleReview(editing(held)).edits.held).toEqual(held)
  })
})

describe('nothingOpen / afterDelete', () => {
  it('closes everything and forgets every edit', () => {
    expect(nothingOpen()).toEqual({
      panel: undefined,
      edits: {},
      currentId: undefined,
    })
  })

  // Deleting one note is not abandoning the rest, and the write that carries
  // the deletion carries them too.
  it('forgets the deleted note and keeps the others', () => {
    const kept = note('kept', 'a', 0)
    const gone = note('gone', 'a', 1)
    const draft = afterDelete(editing(kept, gone), 'gone')
    expect(draft.edits).toEqual({ kept })
    expect(draft.panel).toBeUndefined()
    expect(draft.currentId).toBeUndefined()
  })
})

describe('withEdits / visibleNotes', () => {
  const stored = [note('stored', 'a', 0), note('moved', 'a', 1)]
  const moved = { ...stored[1], body: 'moved' }
  const invented = { ...note('invented', 'b', 2), body: '', region: undefined }

  it('draws a nudged box where it was nudged to, not where the server has it', () => {
    expect(withEdits(stored, { moved })).toEqual([stored[0], moved])
  })

  // Unlike `editsToWrite`: a reader who has just pressed `n` needs to see the
  // note they are about to write.
  it('keeps a blank invented note, which a write would drop', () => {
    expect(withEdits(stored, { invented })).toContainEqual(invented)
    expect(editsToWrite(stored, { invented })).not.toContainEqual(invented)
  })

  it('filters to the edition being read when travel is off', () => {
    expect(visibleNotes(stored, { invented }, false, 'a')).toEqual(stored)
    expect(visibleNotes(stored, { invented }, true, 'a')).toHaveLength(3)
  })
})

describe('sameNotes', () => {
  // Reference equality would record a step for a write that changed nothing
  // and leave undo needing two presses to do anything visible.
  it('compares by content, not by reference', () => {
    const a = [note('a', 'a', 0)]
    expect(sameNotes(a, [note('a', 'a', 0)])).toBe(true)
    expect(sameNotes(a, a)).toBe(true)
    expect(sameNotes(a, [{ ...a[0], body: 'other' }])).toBe(false)
  })
})

// ---- the region gesture ----------------------------------------------------

describe('pressTakesPan', () => {
  const press = {
    zoomable: true,
    onControl: false,
    editing: false,
    shift: false,
  }

  it('pans a press on the page with the editor closed', () => {
    expect(pressTakesPan(press)).toBe(true)
  })

  // Nothing drawn to move the rect over, or the module switched off: the same
  // flag the three zoom keys are gated on.
  it('does nothing where there is nothing to pan', () => {
    expect(pressTakesPan({ ...press, zoomable: false })).toBe(false)
  })

  // A sweep to select text in the note list must not pan the manga under it.
  it('hands a press on a control back', () => {
    expect(pressTakesPan({ ...press, onControl: true })).toBe(false)
  })

  it('gives an unmodified press to the region while editing, and a shifted one to the pan', () => {
    expect(pressTakesPan({ ...press, editing: true })).toBe(false)
    expect(pressTakesPan({ ...press, editing: true, shift: true })).toBe(true)
  })
})

describe('regionUnderDrag', () => {
  // Quarters, so the deltas below are exact in binary and the assertions are
  // about the rule rather than about float error.
  const from = { x: 0.5, y: 0.25 }
  const to = { x: 0.25, y: 0.75 }

  it('sweeps a normalised rectangle out, whichever way the drag went', () => {
    expect(regionUnderDrag('draw', from, to)).toEqual(regionFromDrag(from, to))
  })

  // Measured against the region as it was when the drag began, so a slow drag
  // against the page edge cannot accumulate rounding into a drift.
  it('slides a move by the whole offset from where it started', () => {
    const started = { x: 0.5, y: 0.25, w: 0.25, h: 0.25 }
    expect(regionUnderDrag('move', from, to, started)).toEqual({
      x: 0.25,
      y: 0.75,
      w: 0.25,
      h: 0.25,
    })
    expect(regionUnderDrag('move', from, to, started)).toEqual(
      moveRegion(started, -0.25, 0.5),
    )
  })

  it('draws where a move has no region to slide', () => {
    expect(regionUnderDrag('move', from, to)).toEqual(regionFromDrag(from, to))
  })
})

describe('anchorFrom', () => {
  const start = { x: 0.25, y: 0.75 }

  it('plants a point where a draw barely moved', () => {
    const barely = { x: 0.25, y: 0.75, w: 0.001, h: 0.5 }
    expect(anchorFrom('draw', barely, start)).toEqual(pointAt(start))
  })

  it('keeps a rectangle a reader actually swept out', () => {
    const swept = { x: 0.1, y: 0.1, w: 0.3, h: 0.3 }
    expect(anchorFrom('draw', swept, start)).toBe(swept)
  })

  // Clicking once inside a region without moving it must not delete the region
  // it landed on.
  it('never plants a point for a move', () => {
    const unmoved = { x: 0.1, y: 0.1, w: 0, h: 0 }
    expect(anchorFrom('move', unmoved, start)).toBe(unmoved)
  })
})

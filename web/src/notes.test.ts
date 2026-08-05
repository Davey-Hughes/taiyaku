import { describe, expect, it } from 'vitest'
import { bSlots, fakeResolver, whole } from './fixtures'
import {
  editsToWrite,
  exportName,
  gestureFor,
  isBlank,
  isPoint,
  listPlacement,
  mergeOver,
  notesBySlot,
  notesHere,
  moveRegion,
  notesJson,
  paintedBox,
  pointAt,
  pointInSlot,
  regionContains,
  regionFromDrag,
  reviewList,
  spreadTabs,
  tabOffset,
  withNote,
  withoutNote,
  type Note,
} from './notes'

const note = (id: string, edition: string, slot: number, region?: Note['region']): Note => ({
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
    expect(notesHere(notes, 'a', 3, resolver)).toEqual([{ note: notes[0], travelled: false }])
  })

  it('travels a note written on another edition onto the slot it resolves to', () => {
    const notes = [note('1', 'a', 3)]
    // `a` 3 pairs with `b` 4, so reading `b` at slot 4 shows it, travelled.
    expect(notesHere(notes, 'b', 4, resolver)).toEqual([{ note: notes[0], travelled: true }])
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
      const landing = Array.from({ length: slots.b.length }, (_, at) => at).filter(
        (at) => notesHere([written], 'b', at, resolver).length === 1,
      )
      const counterpart = pairs[`a|b|${slot}`]
      if (counterpart === undefined) {
        expect(landing, `a ${slot} has no counterpart, so it must land nowhere`).toEqual([])
      } else {
        expect(landing, `a ${slot} must land on exactly one slot of b`).toEqual([counterpart])
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
    const notes = [note('here', 'a', 3), note('gone', 'a', 5), note('local', 'b', 0)]
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
    const notes = Array.from({ length: slots.a.length }, (_, slot) => note(`n${slot}`, 'a', slot))
    for (const viewing of ['a', 'b'] as const) {
      const entries = reviewList(notes, viewing, resolver)
      for (const entry of entries) {
        const landing = Array.from({ length: slots[viewing].length }, (_, at) => at).filter((at) =>
          notesHere([entry.note], viewing, at, resolver).some((h) => h.note.id === entry.note.id),
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
  // slot" per rendered slot would be one scan per page — 195 on the primary
  // corpus volume, each resolving. Inverting it costs one resolve per *note*,
  // of which there are a handful, whatever the volume's length.
  it('groups every note under the slot it lands on here', () => {
    const notes = [note('a3', 'a', 3), note('a4', 'a', 4), note('local', 'b', 4)]
    const map = notesBySlot(notes, 'b', resolver)
    // `a` 3 → `b` 4, `a` 4 → `b` 5, and the local note is already on `b` 4.
    expect(map.get(4)?.map((h) => h.note.id)).toEqual(['local', 'a3'])
    expect(map.get(5)?.map((h) => h.note.id)).toEqual(['a4'])
  })

  it('leaves out a note that lands nowhere here rather than bucketing it at 0', () => {
    expect([...notesBySlot([note('gone', 'a', 5)], 'b', resolver).keys()]).toEqual([])
  })

  // The third implementation of "where does this note go" has to agree with
  // the other two, or scroll mode would draw a tab on a slot single mode does
  // not — the same drift the `notesHere`/`reviewList` agreement test exists
  // to catch, extended to the one that renders a whole volume at once.
  it('agrees with notesHere on every slot of every edition', () => {
    const notes = [
      ...Array.from({ length: slots.a.length }, (_, slot) => note(`a${slot}`, 'a', slot)),
      ...Array.from({ length: slots.b.length }, (_, slot) => note(`b${slot}`, 'b', slot)),
    ]
    for (const viewing of ['a', 'b'] as const) {
      const map = notesBySlot(notes, viewing, resolver)
      for (let slot = 0; slot < slots[viewing].length; slot++) {
        const direct = notesHere(notes, viewing, slot, resolver)
        expect(
          (map.get(slot) ?? []).map((h) => `${h.note.id}:${h.travelled}`).sort(),
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
    expect(tabOffset(note('1', 'a', 0, { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }))).toBeCloseTo(0.4)
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

  it('does not divide by a zero-sized paint', () => {
    expect(pointInSlot({ x: 5, y: 5 }, { x: 0, y: 0, w: 0, h: 0 })).toEqual({ x: 0, y: 0 })
  })
})

describe('spreadTabs', () => {
  // Two whole-page notes on one slot both anchor at 0.5, so their tabs land
  // exactly on top of each other and a reader sees one marker where there are
  // two — which is how "I can't do multiple annotations per page" looks even
  // once multiple notes are allowed.
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
    expect(isBlank({ ...note('1', 'a', 0, { x: 0, y: 0, w: 0.5, h: 0.5 }), body: '' })).toBe(false)
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
    expect(press({ editing: false, onAnImage: false })).toEqual({ kind: 'ignore' })
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
    expect(press({ at: { x: 0.7, y: 0.3 }, here: [local(box)] })).toEqual({ kind: 'draw' })
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
    expect(press({ at: { x: 0.3, y: 0.3 }, here: [local(box), local(inner)] })).toEqual({
      kind: 'move',
      noteId: 'inner',
      from: inner.region,
    })
  })

  // A point has no area; without slop there would be nothing to take hold of.
  it('grabs a point from within the slop', () => {
    expect(press({ at: { x: 0.81, y: 0.81 }, here: [local(point)] }).kind).toBe('move')
    expect(press({ at: { x: 0.9, y: 0.9 }, here: [local(point)] }).kind).toBe('draw')
  })

  // Its region belongs to another edition's slot; dragging it here would move
  // something that is not on this page.
  it('never grabs a travelled note', () => {
    expect(press({ at: { x: 0.3, y: 0.3 }, here: [{ note: far, travelled: true }] })).toEqual({
      kind: 'draw',
    })
  })
})

describe('editsToWrite', () => {
  const stored = [note('kept', 'a', 0), note('edited', 'a', 1)]

  it('lays edits over the stored notes', () => {
    const changed = { ...stored[1], body: 'changed' }
    expect(editsToWrite(stored, { edited: changed })).toEqual([stored[0], changed])
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
    expect(editsToWrite(stored, { kept: emptied })).toEqual([emptied, stored[1]])
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
    expect(mergeOver(theirs, mine).map((n) => n.id).sort()).toEqual(['mine', 'theirs'])
  })

  it('takes this reader\'s copy of a note both of them edited', () => {
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

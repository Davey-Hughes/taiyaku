import type { ArtworkHug } from './modules/fit'
import { nextColour } from './palette'
import type { Resolver } from './resolver'

/**
 * A rectangle marked on a note's slot, in fractions of the **page** that slot
 * is drawn on — never pixels.
 *
 * *Corrected on 2026-08-08. Every "slot fractions" in this file said slot and
 * meant page, from M5c until L1 needed the two told apart; the code has
 * measured against the drawn page throughout.* `App.tsx`'s `pointOn` is where
 * the numbers are made and says so — it reads the `<img>`'s own
 * `getBoundingClientRect()` through `paintedBox` below — and
 * `Annotations.tsx` positions every marker as a percentage of that same
 * painted box.
 *
 * **The two readings differ on exactly one thing: a joined spread.**
 * `pairing.ts`'s `groups` draws a spread's two slots as one wide image and
 * `tiles.ts` says the group "stands for *both* halves as a single wide tile
 * and has no one half to name", so slots `4L` and `4R` are expressed against
 * the same page and a region anchored to either can name any part of it.
 * Everywhere else a slot is a page and the two coincide, which is why nothing
 * has gone wrong under the old wording.
 *
 * **The consequence belonged to L1, and L1 discharged it on 2026-08-08.**
 * Where one edition joins a spread and the other splits it, getting a reader
 * to the right half is the transform's job — the slot model gets them to the
 * right *page* and stops. `viewport.ts`'s `Drawn` now carries a `half` for
 * exactly that: `mapView` takes the rect out of `halfBox`'s box on the way in
 * and puts it back on the way out, and hands `mapRect` the half's own frame
 * rather than the spread's. Note *regions* still do not travel — that was
 * never L1's scope — but the transform they would travel through no longer has
 * this hole in it.
 *
 * *Until then this paragraph read "L1 did not discharge it" and described the
 * failure: `mapView` was handed each side's drawn image and nothing about
 * halves, and `mapRect` derives everything from the two aspects, so it read
 * the spread's extra width as margin at both edges and mapped its central band
 * onto the single page rather than the half the reader was on. Kept rather
 * than deleted, because it is the sharpest case of what the correction at the
 * top of this comment is about — everywhere else a slot is a page, and a
 * spread is where the difference is actually paid.*
 */
export interface Region {
  x: number
  y: number
  w: number
  h: number
}

/**
 * One note, exactly as `taiyaku-serve`'s `notes.rs` stores it.
 *
 * `region` absent means the whole slot — the degenerate case, not a second
 * kind of note. `created`/`updated` are RFC 3339 strings minted here: the
 * server has no clock in this format, so a note carries the time it was
 * written rather than the time it arrived.
 */
export interface Note {
  id: string
  edition: string
  slot: number
  region?: Region
  /**
   * A colour **name**, not pixels — `palette.ts` is what turns one into
   * colour. Absent means the default, so every note written before colours
   * existed reads as it always did, and a name this build does not know falls
   * back rather than vanishing (see `noteColour`).
   */
  color?: string
  body: string
  created: string
  updated: string
}

export interface NotesDoc {
  work: string
  notes: Note[]
}

/** A note visible where the reader is standing, and whether it came from elsewhere. */
export interface NoteHere {
  note: Note
  /**
   * Written on another edition and resolved onto this slot. Its region does
   * **not** travel, so a travelled note draws a margin tab and never a tint.
   * Drawing the box at an untransformed position would put it over the wrong
   * panel, which is the reader lying about what it knows.
   *
   * *Updated 2026-08-08.* This said "transforming one correctly is L1
   * geometry, a *Later* item". L1 landed, and the transform now exists —
   * `viewport.ts`'s `mapRect`, measured over the corpus. What has not been
   * built is drawing a region through it, which L1's design put out of scope
   * by name so the milestone stayed one shippable thing
   * (`docs/superpowers/specs/2026-08-07-l1-geometry-design.md`, "What L1 is
   * for"). So the behaviour here is unchanged and the reason for it has
   * moved: not "no transform exists" but "nothing draws through the one that
   * does" — which is cheaper than it was, not free, since the transform is
   * shared and the drawing is not.
   */
  travelled: boolean
}

/**
 * Which surface the note panel is showing, or none.
 *
 * Declared here rather than in `NotePanel.tsx`, which imports it: this is the
 * type of the editor's own state, and the `Draft` below is built from it — a
 * module that must not import React cannot take its state's type from a
 * `.tsx`, even type-only, without making the dependency graph read backwards.
 */
export type PanelMode = 'edit' | 'review'

/** One row of the review list: a note, and where it lands here if it does. */
export interface ReviewEntry {
  note: Note
  /** The slot of the edition being read, or `undefined` — unreachable from here. */
  here: number | undefined
}

/**
 * Where each edition's slot space meets the one being read: for every edition
 * that holds notes, the slot of *that* edition which the current slot of the
 * viewed edition corresponds to.
 *
 * **Resolved outward from the viewed edition, once per edition — not once per
 * note.** The two directions agree: measured over the real sidecar and a
 * composed three-edition route, every slot that lands round-trips to exactly
 * where it started — 193 of 193 on Go-Toubun v01 and 28 of 28 composed through
 * the hub, no disagreement either way (see the M5c plan's "What the prototype
 * measured"). Resolving this way holds `from` fixed for the whole scan, which
 * is what `Binding`'s single-pair route memo is built for; resolving per note
 * would alternate pairs and thrash it, the case O14's entry names as its
 * reopening condition.
 */
function counterpartSlots(
  editions: string[],
  viewing: string,
  slot: number,
  resolver: Resolver,
): Map<string, number> {
  const out = new Map<string, number>()
  const at = resolver.slotAt(viewing, slot)
  if (!at) return out
  for (const edition of editions) {
    if (edition === viewing) {
      out.set(edition, slot)
      continue
    }
    const landed = resolver.resolve(viewing, edition, at)
    const index = landed && resolver.indexOf(edition, landed)
    if (index !== undefined) out.set(edition, index)
  }
  return out
}

/**
 * The notes to draw on the slot being read: those written here, and those
 * written on another edition that the alignment puts here.
 *
 * Local notes come first, so the tab order does not shuffle as a reader swaps
 * editions — the same slot in the same edition always draws its own notes in
 * the same place.
 */
export function notesHere(
  notes: Note[],
  viewing: string,
  slot: number,
  resolver: Resolver,
): NoteHere[] {
  if (notes.length === 0) return []
  const editions = [...new Set(notes.map((n) => n.edition))]
  const counterparts = counterpartSlots(editions, viewing, slot, resolver)
  const here = notes.filter((n) => counterparts.get(n.edition) === n.slot)
  return [
    ...here
      .filter((n) => n.edition === viewing)
      .map((note) => ({ note, travelled: false })),
    ...here
      .filter((n) => n.edition !== viewing)
      .map((note) => ({ note, travelled: true })),
  ]
}

/**
 * Every note, in the order they are held, each with the slot it lands on here
 * — or `undefined` where it lands nowhere.
 *
 * A note whose slot has no counterpart in the edition being read **stays in
 * the list, marked**, rather than disappearing: silence is the failure this
 * project is written against, and a note that vanished on a swap would be
 * indistinguishable from one that was never saved.
 *
 * Resolved per note here, not per edition as `notesHere` does — the review
 * list is opened deliberately, once, where `notesHere` runs on every render of
 * every slot, and per-note resolution is the direct expression of the question
 * this answers ("where does *this* note land"). That the two directions agree
 * is not assumed: `notes.test.ts` checks this against `notesHere` on every
 * slot of every edition of the fixture.
 */
export function reviewList(
  notes: Note[],
  viewing: string,
  resolver: Resolver,
): ReviewEntry[] {
  return notes.map((note) => {
    if (note.edition === viewing) return { note, here: note.slot }
    const at = resolver.slotAt(note.edition, note.slot)
    const landed = at && resolver.resolve(note.edition, viewing, at)
    const here = landed ? resolver.indexOf(viewing, landed) : undefined
    return { note, here }
  })
}

/**
 * Every note that lands somewhere in the edition being read, keyed by the slot
 * it lands on.
 *
 * The same question `notesHere` answers, asked once for the whole edition
 * instead of once per slot — which is what continuous-scroll mode needs, since
 * it mounts every group at once and would otherwise scan the note list 195
 * times on the primary corpus volume, resolving on each. This costs one
 * resolve per *note*, of which a work has a handful, whatever its length.
 *
 * Built on `reviewList`, so it is the same walk rather than a third
 * implementation of travel — and `notes.test.ts` pins that it agrees with
 * `notesHere` on every slot of every edition, because a scroll-mode tab
 * appearing on a slot single mode does not is precisely the drift three
 * implementations of one idea invite.
 *
 * Local notes first within a slot, matching `notesHere`'s own order, so the
 * tab order does not change with the reading mode.
 */
export function notesBySlot(
  notes: Note[],
  viewing: string,
  resolver: Resolver,
): Map<number, NoteHere[]> {
  const out = new Map<number, NoteHere[]>()
  for (const { note, here } of reviewList(notes, viewing, resolver)) {
    if (here === undefined) continue
    const at = out.get(here) ?? []
    const entry: NoteHere = { note, travelled: note.edition !== viewing }
    // Local first, travelled appended — the same order `notesHere` returns.
    if (entry.travelled) at.push(entry)
    else at.unshift(entry)
    out.set(here, at)
  }
  return out
}

/**
 * Where a note's margin tab sits, as a fraction of the page's height.
 *
 * The middle of its region, or the middle of the page for a whole-slot note.
 * Fractions in, fraction out — the arithmetic is scale-free, so a tab tracks
 * at any fit scale with nothing to recompute.
 *
 * Page rather than slot, per `Region`'s doc — though this is the one axis
 * where the distinction cannot bite: a joined spread's two slots split its
 * width, not its height, so a spread half is exactly as tall as its page.
 */
export function tabOffset(note: Note): number {
  return note.region ? note.region.y + note.region.h / 2 : 0.5
}

/**
 * Tab positions with overlaps pushed apart, in the order given.
 *
 * Several notes on one slot is the ordinary case — one per panel — and a
 * whole-slot note has no region to derive a position from, so every one of
 * them anchors at the middle. Left alone, two such notes draw one tab on top
 * of another and a reader sees a single marker where there are two, which
 * looks exactly like not being able to make more than one note per page.
 *
 * Only collisions move. A tab whose neighbour is already `minGap` away stays
 * exactly where its region put it, so the position still means something.
 * Where the run reaches the bottom edge the whole group is shifted up rather
 * than allowed to slide off, since a tab past the end of the page is no better
 * than one hidden behind another.
 *
 * Returned in the caller's order, not sorted: the caller pairs these back up
 * with its own list of notes, and a sorted return would attach each tab to the
 * wrong one.
 */
export function spreadTabs(offsets: number[], minGap: number): number[] {
  const order = offsets
    .map((offset, index) => ({ offset, index }))
    .sort((a, b) => a.offset - b.offset)
  let previous: number | undefined
  for (const entry of order) {
    if (previous !== undefined && entry.offset - previous < minGap)
      entry.offset = previous + minGap
    previous = entry.offset
  }
  // A run that ran off the bottom comes back as a group, keeping the gaps it
  // just earned rather than collapsing them again against the edge.
  const overshoot = (previous ?? 0) - 1
  if (overshoot > 0) for (const entry of order) entry.offset -= overshoot
  const out = offsets.slice()
  for (const entry of order)
    out[entry.index] = Math.min(Math.max(entry.offset, 0), 1)
  return out
}

const clampFraction = (v: number) => Math.min(Math.max(v, 0), 1)
const clamp = clampFraction

/**
 * A drag, in page fractions, as a region.
 *
 * Normalised so dragging up-and-left produces the same rectangle as
 * down-and-right, and clamped to the page so a drag that leaves it still names
 * something inside it — the server refuses a region outside `0..1`, and
 * refusing a reader's note because their pointer left the image is not an
 * answer.
 */
export function regionFromDrag(
  a: { x: number; y: number },
  b: { x: number; y: number },
): Region {
  const x = clamp(Math.min(a.x, b.x))
  const y = clamp(Math.min(a.y, b.y))
  return {
    x,
    y,
    w: clamp(Math.max(a.x, b.x)) - x,
    h: clamp(Math.max(a.y, b.y)) - y,
  }
}

/** A rectangle in an element's own pixels. */
export interface Box {
  x: number
  y: number
  w: number
  h: number
}

/**
 * Where the page is actually painted inside its element — the one rule, for
 * every mode and every fit setting.
 *
 * `PageView` draws a page with `object-fit: contain` — aspect preserved, never
 * cropped, the design's "the page fits whole" — so the painted image is inset
 * within its element whenever the two aspects differ, and the inset is
 * background, not page. A region measured against the *element* rather than
 * against this is offset by that inset and scaled against the wrong width —
 * the whole page shifted, and every tint and marker on it with it. That is a
 * wrong frame of reference rather than a rounding error, however small the
 * inset happens to be on any one page.
 *
 * **The element is now routinely far larger than the artwork, in both axes,
 * and until `upscaleToFit` it never was.** *2026-08-08.* This doc used to say
 * "downscaled to fit", and while a page was sized to its own content — `width`
 * and `height` `auto` under both `max-*` at 100%, which is still what
 * `pageStyle` emits with the setting off — the two boxes were the same box.
 * `pageStyle`'s own doc says so: with `auto`, `contain` "has nothing to
 * reconcile, since box and content coincide". This function was an identity
 * for the whole of its life, describing a distinction that had not arrived.
 * `upscaleToFit` sets both to `100%`, so the element is the whole cell and the
 * art is whatever `contain` fits inside it. Measured in a browser on
 * `gotoubun-v01`, single mode at fit-whole, 2026-08-08: a 1385x475 element
 * holding **334x475** of artwork, inset **526px** each side, from a 2147x3056
 * scan. The arithmetic below needed nothing for that — `Math.min` of the two
 * ratios is scale-free and was already right in both directions — but a caller
 * that read the element instead of asking this went from exactly right to
 * 526px out, which is where `Annotations` was until this became its
 * measurement too.
 *
 * **`hug` is the half that did need arithmetic.** `pageStyle` emits
 * `object-position`, and `contain` puts the leftover wherever that says: `0%`
 * against the left edge, `100%` against the right, and the `50%` this used to
 * assume split evenly. The two cells of a real two-page pair hug the seam, so
 * an evenly split inset is wrong there by half the leftover. Measured the same
 * session, two-page mode: this answered 179 for the left cell and the same 179
 * for the right — 179 and 872 across the page area — where the artwork was
 * actually at **359** and **693**. Each 692.5px cell holds 334px of art, so
 * the 359px of leftover was being halved instead of pushed to one side.
 *
 * **Only the horizontal is ever hugged.** `pageStyle` writes `50%` for the
 * vertical component of all three positions — `0% 50%`, `100% 50%`, `50% 50%`
 * — so `y` stays centred whatever `hug` says. That is those declarations
 * restated rather than a second opinion about them.
 *
 * An image whose size is not known yet — `naturalWidth` is 0 until it decodes
 * — falls back to the whole box rather than dividing by zero and handing back
 * `NaN` fractions that would then reach the server as a region. `hug` does
 * nothing in that branch, for the same reason the rest of it does nothing:
 * there is no artwork placed yet, so there is no leftover to push anywhere.
 */
export function paintedBox(
  natural: { w: number; h: number },
  box: { w: number; h: number },
  hug: ArtworkHug = 'centre',
): Box {
  if (natural.w <= 0 || natural.h <= 0)
    return { x: 0, y: 0, w: box.w, h: box.h }
  const scale = Math.min(box.w / natural.w, box.h / natural.h)
  const w = natural.w * scale
  const h = natural.h * scale
  const spare = box.w - w
  return {
    x: hug === 'left-edge' ? 0 : hug === 'right-edge' ? spare : spare / 2,
    y: (box.h - h) / 2,
    w,
    h,
  }
}

/**
 * Where a page's painted artwork sits in the coordinates of the element that
 * hosts the overlay — that host's own top-left, not the `<img>`'s.
 *
 * Two rects go in, not one, because since `upscaleToFit` the `<img>` is
 * routinely not the host: `PageAnnotations` overlays a sibling `<div>` sized
 * to the page area, and the image inside it can sit at a different screen
 * position from that area even though one contains the other. `outer` is the
 * host's own `getBoundingClientRect()`, `inner` is the image's, and `art` is
 * `paintedBox`'s inset of the artwork within `inner`. All three come from
 * live measurement, so a CSS transform — a zoom or a pan — moves every one of
 * them together and this needs nothing extra for that.
 *
 * **With matching origins, every wrong order and sign gives the same
 * answer.** Swap `inner`/`outer`, or subtract the other way, and
 * `inner.left - outer.left` still comes out 0 whenever the two rects start at
 * the same point — which is the ordinary fixture in a test, and even the
 * ordinary page before `upscaleToFit` gave the element and the image the same
 * box. Only a host offset from its image — scrolled, or inset by layout —
 * makes the four ways of combining these disagree, which is exactly why this
 * is its own function with its own tests rather than arithmetic inlined
 * where every caller's rects happen to coincide.
 */
export function artOffset(
  outer: { left: number; top: number },
  inner: { left: number; top: number },
  art: { x: number; y: number },
): { left: number; top: number } {
  return {
    left: inner.left - outer.left + art.x,
    top: inner.top - outer.top + art.y,
  }
}

/**
 * A point in an element's pixels, as fractions of the painted page.
 *
 * The name says slot and the doc says page, which is the whole of `Region`'s
 * correction in one identifier. Kept rather than renamed: it is exported,
 * called from `App.tsx`'s `pointOn` and named by its own tests, and this pass
 * changes comments only.
 *
 * Deliberately **not** clamped: a point in the letterbox reads as outside
 * `0..1`, and `regionFromDrag` is what clamps a whole drag once both ends are
 * known. Clamping here would silently move one end of a drag before the
 * rectangle it belongs to had been formed.
 */
export function pointInSlot(
  point: { x: number; y: number },
  painted: Box,
): { x: number; y: number } {
  if (painted.w <= 0 || painted.h <= 0) return { x: 0, y: 0 }
  return {
    x: (point.x - painted.x) / painted.w,
    y: (point.y - painted.y) / painted.h,
  }
}

/**
 * A region with no area: a reader marking a spot rather than an area.
 *
 * The third of three anchors carried in one field — absent is the whole slot,
 * zero-sized is a point, sized is an area — so nothing downstream needs a
 * second kind of note, and the server validates all three with one rule.
 */
export function isPoint(region: Region | undefined): boolean {
  return region !== undefined && region.w === 0 && region.h === 0
}

/** A point anchor at a place on the slot. */
export function pointAt(at: { x: number; y: number }): Region {
  return { x: clampFraction(at.x), y: clampFraction(at.y), w: 0, h: 0 }
}

/**
 * Whether a point in page fractions falls inside a region.
 *
 * What decides whether a drag on the page *moves* the region already there or
 * draws a new one, so getting it wrong means a reader who meant to nudge a box
 * replaces it instead. Inclusive of the edges: a reader aiming at the boundary
 * of their own rectangle means the rectangle.
 */
export function regionContains(
  region: Region | undefined,
  point: { x: number; y: number },
  slop = 0,
): boolean {
  if (!region) return false
  // `slop` is what makes a **point** grabbable at all: it has no area, so an
  // exact containment test could only ever be satisfied by hitting one
  // infinitely small spot. The caller passes the radius its markers are drawn
  // at, so what a reader can grab is what they can see.
  return (
    point.x >= region.x - slop &&
    point.x <= region.x + region.w + slop &&
    point.y >= region.y - slop &&
    point.y <= region.y + region.h + slop
  )
}

/**
 * A region translated by an offset, kept whole and kept inside the page.
 *
 * **Clamped as a whole rather than per edge.** Clamping `x` and `y`
 * independently against `0..1` would let a region dragged past the corner keep
 * its origin while its far edge ran out of room — the box would quietly shrink
 * as a reader pushed against the boundary, which is the sort of thing that
 * reads as the tool fighting you. Moving the origin within `0 .. 1 - w`
 * instead means it stops at the edge at exactly the size it had.
 *
 * A region that already fills the page has nowhere to go and stays put, which
 * falls out of the same arithmetic rather than needing a case.
 */
export function moveRegion(region: Region, dx: number, dy: number): Region {
  const within = (value: number, size: number) =>
    Math.min(Math.max(value, 0), Math.max(1 - size, 0))
  return {
    ...region,
    x: within(region.x + dx, region.w),
    y: within(region.y + dy, region.h),
  }
}

/** What a press on the page area means. */
export type Gesture =
  | { kind: 'close' }
  | { kind: 'ignore' }
  | { kind: 'move'; noteId: string; from: Region | undefined }
  | { kind: 'draw' }

/**
 * What a press on the page area means while the editor is open.
 *
 * Extracted from `App.tsx` because it is the most-changed rule of this
 * milestone and lived where nothing could test it: grab-any-box, innermost
 * first, background-finishes, and the two guards that keep a press on the
 * facing page of a spread from being read as either.
 *
 * The order matters and is the rule:
 *
 * 1. Not on an image at all — the letterbox around the page — **finishes the
 *    note**. Looking away is how a person stops writing one.
 * 2. On an image, but not the page the editor is anchored to (the facing page
 *    in `two` mode): **nothing**. Reading "not this one" as "not any" would
 *    end a note because a reader touched the other half of a spread.
 * 3. Inside a region already on this page: **move that one**, searched
 *    innermost-first so a box drawn inside another is the one you get, and
 *    with slop so a point — which has no area — can be taken hold of at all.
 * 4. Anywhere else on the page: **draw a new one**.
 *
 * A travelled note is never grabbed: its region belongs to another edition's
 * slot, and dragging it here would move something that is not on this page.
 */
export function gestureFor(press: {
  editing: boolean
  onAnImage: boolean
  onTheDraftsPage: boolean
  at: { x: number; y: number }
  here: NoteHere[]
  slop: number
}): Gesture {
  if (!press.editing) return { kind: 'ignore' }
  if (!press.onAnImage) return { kind: 'close' }
  if (!press.onTheDraftsPage) return { kind: 'ignore' }
  const grabbed = [...press.here]
    .reverse()
    .find(
      (h) =>
        !h.travelled && regionContains(h.note.region, press.at, press.slop),
    )
  if (grabbed)
    return { kind: 'move', noteId: grabbed.note.id, from: grabbed.note.region }
  return { kind: 'draw' }
}

/**
 * How near a marker counts as grabbing it, in page fractions — the space
 * `App.tsx`'s `pointOn` hands back, per `Region` above.
 *
 * A point has no area, so without this the only way to take hold of one would
 * be to hit an infinitely small spot. Roughly the radius `Annotations` draws a
 * pin at, so what a reader can grab is what they can see.
 */
export const GRAB_SLOP = 0.02

/** Where a region drag began: the pixels, the image, and what it means. */
export interface DragStart {
  x: number
  y: number
  el: HTMLImageElement
  /**
   * `draw` sweeps a new rectangle out from where the pointer went down.
   * `move` slides the region that was already under it, keeping its size —
   * `from` is what it looked like when the drag started, so the offset is
   * always measured against that rather than accumulated frame by frame,
   * which would drift.
   */
  intent: 'draw' | 'move'
  from?: Region
  /** Which note is being moved — not always the one the panel was showing when the drag began. */
  noteId?: string
}

/**
 * Whether a press pans the page rather than reaching `gestureFor` above.
 *
 * Asked *before* `gestureFor` rather than as a fifth case inside it. That
 * function is described in its own doc as "the most-changed rule of this
 * milestone and lived where nothing could test it", and rewriting it to carry
 * the pan would spend exactly the testability that comment records.
 *
 * **`zoomable` is one flag, not two**: the module being on *and* something
 * being drawn to move the rect over. It is the same predicate the three zoom
 * keys are gated on, and keeping it one argument is what stops the pan and the
 * keys drifting apart. `drawn` rather than "not scroll mode" because it
 * subsumes it: nothing is drawn in scroll mode *or* in a gap, where `Gap`
 * renders inside the page area — without it, a drag anywhere over a gap,
 * including off one of `Gap`'s own buttons, pans a rect nobody is drawing.
 *
 * **`onControl` excludes rather than requires.** "The editor is closed" is not
 * the same set as "the pointer is over the page", and `zoomable` does not
 * close the difference: the things that share the page area *with* a drawn
 * page are `NoteList` in its `side` placement and the note tabs
 * `PageAnnotations` draws, which take pointer events deliberately so they can
 * be clicked. Neither stops propagation, so without this a sweep to select
 * text in the note list pans the manga underneath it, and so does a drag begun
 * on a tab.
 *
 * *This was "the press is on a page image" until 2026-08-08*, which excluded
 * the same controls by being narrower still. That was the narrowest test true
 * of the page and false of a button — and it also made the letterbox around
 * the page dead to a drag, which a reader trying it said so of. The cost had
 * been judged acceptable on the reasoning that the letterbox shrinks as the
 * zoom grows; what that missed is that a reader reaches for the nearest grey
 * to drag from, and being ignored there reads as the pan being broken rather
 * than as a boundary. Excluding controls also means the next component added
 * inside the page area is included by default and has to opt out, which is the
 * right way round for a background gesture. The caller's `onControl` has to be
 * **ancestor-aware** for that: a press on a note-list entry lands on the
 * *text*, so a target-only test answers `false` for exactly the case the image
 * test was protecting against.
 *
 * `stopPropagation` on every child was the other candidate and lost again: it
 * would put "and this is why the manga does not pan" in `NoteList.tsx` and
 * `Annotations.tsx`, where nobody looking for the pan rule would find it.
 *
 * With the editor open the modifier is what asks for a pan, since an
 * unmodified press there means the region. The one press that leaves here for
 * `gestureFor` rather than panning is the modifier held with the editor open
 * on something that is not a page image, which that function reads as `close`
 * — rule 1 of its own list, not a case invented here. With the editor *closed*
 * nothing is lost by taking the drag first at all, since `gestureFor` answers
 * `ignore` for every press in that state.
 */
export function pressTakesPan(press: {
  zoomable: boolean
  onControl: boolean
  editing: boolean
  shift: boolean
}): boolean {
  return press.zoomable && !press.onControl && (!press.editing || press.shift)
}

/**
 * What the region should be, given where the drag began and where it is now.
 *
 * A move is measured against `started` — the region as it was when the drag
 * began — rather than against the previous frame, so a slow drag against the
 * page edge cannot accumulate rounding into a drift. A draw has no `started`
 * and sweeps out from the origin.
 */
export function regionUnderDrag(
  intent: 'draw' | 'move',
  from: { x: number; y: number },
  to: { x: number; y: number },
  started?: Region,
): Region {
  if (intent === 'move' && started)
    return moveRegion(started, to.x - from.x, to.y - from.y)
  return regionFromDrag(from, to)
}

/**
 * A click rather than a drag, while drawing, plants a point instead.
 *
 * A one-pixel rectangle would be a marker a reader could never find again, and
 * discarding it — which is what this used to do — meant a click on the page
 * did nothing at all and the only way to mark a panel was to draw a box round
 * it. A *move* is exempt: clicking once inside a region without moving it is a
 * no-op that must not delete the region it landed on.
 */
export function anchorFrom(
  intent: 'draw' | 'move',
  region: Region,
  start: { x: number; y: number },
): Region {
  return intent === 'draw' && (region.w < 0.01 || region.h < 0.01)
    ? pointAt(start)
    : region
}

/**
 * The notes a write should send: everything stored, with this session's edits
 * laid over it, minus the ones that were invented here and never filled in.
 *
 * A note this session invented and left with neither words nor a region is
 * dropped — pressing `n` and then going off to move another box leaves one
 * behind, and writing it would put an invisible marker on the page for a
 * keystroke the reader had already moved on from. A note that *existed* and
 * has been emptied is kept, because emptying one is an edit where deleting one
 * is a button.
 */
export function editsToWrite(
  stored: Note[],
  edits: Record<string, Note>,
): Note[] {
  const known = new Set(stored.map((n) => n.id))
  const keep = Object.values(edits).filter(
    (note) => known.has(note.id) || !isBlank(note),
  )
  return keep.reduce(withNote, stored)
}

/**
 * Another device's document with this reader's own notes laid back over it.
 *
 * What a stale-validator retry sends. Notes belonging to ids this reader did
 * not touch survive exactly as the other device wrote them, which is the part
 * that matters; a note both devices edited takes this reader's copy, which is
 * last-write-wins and always was.
 */
export function mergeOver(theirs: Note[], mine: Note[]): Note[] {
  return mine.reduce(withNote, theirs)
}

/**
 * A note that says nothing and marks nothing.
 *
 * Pressing `n` and then going off to move another note's box leaves one of
 * these behind: never typed in, never given a region. Saving it would put an
 * empty marker on the page for a keystroke the reader had already moved on
 * from. Either half alone is a real note — a marked area with the words still
 * to come, or a note about the whole page — so both have to be missing.
 */
export function isBlank(note: Note): boolean {
  return note.body.trim() === '' && note.region === undefined
}

/** Add a note, or replace the one already holding its id. */
export function withNote(notes: Note[], note: Note): Note[] {
  const at = notes.findIndex((n) => n.id === note.id)
  if (at < 0) return [...notes, note]
  return notes.map((n, i) => (i === at ? note : n))
}

/** Drop a note by id. */
export function withoutNote(notes: Note[], id: string): Note[] {
  return notes.filter((n) => n.id !== id)
}

// ---- the editing session ---------------------------------------------------

/**
 * Everything the editor holds that is not the stored document.
 *
 * One record with eight transitions rather than three states written three
 * setters at a time. The argument for it is `nothingOpen` below: closing the
 * editor is the same three-field move made from four different places, and it
 * was written out twice in `App.tsx` before this existed — which is exactly
 * how a fourth exit ends up clearing two of the three.
 *
 * `edits` is a map rather than one draft, because the editor is an **edit
 * mode**: while it is open every region on the page can be grabbed and moved,
 * not only the one belonging to the note being typed into. Grabbing another
 * note's box makes it the one on show, and the note you were on keeps its
 * changes here rather than being discarded for having lost focus — so a write
 * sends everything you touched, and nothing you did quietly goes missing
 * because you reached for a different box.
 */
export interface Draft {
  panel: PanelMode | undefined
  /** Every note this editing session has changed, keyed by id. */
  edits: Record<string, Note>
  currentId: string | undefined
}

/** Nothing open and nothing touched — where every session starts and ends. */
export const NOTHING_OPEN: Draft = {
  panel: undefined,
  edits: {},
  currentId: undefined,
}

/**
 * Escape, a background click, the panel's own close, a delete's aftermath.
 *
 * A function as well as a constant because that is how the other seven read at
 * a call site, and it hands back the shared value rather than a fresh one: a
 * caller setting this when it is already set does not need a re-render.
 */
export function nothingOpen(): Draft {
  return NOTHING_OPEN
}

/**
 * The note the panel is showing.
 *
 * Derived, never stored, so there is one copy of every edit and no way for the
 * shown note and the saved one to disagree.
 */
export function shownNote(draft: Draft): Note | undefined {
  return draft.currentId ? draft.edits[draft.currentId] : undefined
}

/**
 * `n`: always a brand-new note, however many the slot already holds.
 *
 * One note per panel is the ordinary case, and this key used to *reopen* the
 * note already there — which made a second note on a page impossible, since
 * nothing else creates one. The reasoning for reopening was to stop `n`
 * pressed twice leaving duplicates, and it protected against nothing: a draft
 * is not a note until it is written, so a second press only ever replaced an
 * unsaved one. Reopening an existing note is what its margin tab is for.
 *
 * The colour is the next one nobody on **this page** is using, so two notes on
 * one page come out different without a reader choosing — picked from `here`
 * rather than from the whole work, since telling one note from another only
 * matters where they are drawn together.
 *
 * `id` and `now` are arguments rather than minted here: a rule that reaches
 * for a clock and a random source is a rule that cannot be tested for anything
 * except not crashing.
 */
export function newNote(
  draft: Draft,
  at: { edition: string; slot: number },
  here: NoteHere[],
  id: string,
  now: string,
): Draft {
  const fresh: Note = {
    id,
    edition: at.edition,
    slot: at.slot,
    color: nextColour(here.map((h) => h.note.color)),
    body: '',
    created: now,
    updated: now,
  }
  return {
    panel: 'edit',
    edits: { ...draft.edits, [id]: fresh },
    currentId: id,
  }
}

/**
 * A margin tab or a review row: show a note, bringing it into this session.
 *
 * Whatever is already in `edits` for that id wins over the stored copy, so
 * clicking back onto a note you moved a moment ago shows it where you left it
 * rather than where the server still has it. An id neither side knows is not
 * an error and not an empty editor — the same draft comes back, so a caller
 * can tell nothing happened.
 */
export function openNote(draft: Draft, id: string, stored: Note[]): Draft {
  const found = draft.edits[id] ?? stored.find((n) => n.id === id)
  if (!found) return draft
  return {
    panel: 'edit',
    edits: draft.edits[id] ? draft.edits : { ...draft.edits, [id]: found },
    currentId: id,
  }
}

/**
 * Change the note on show, in place. The same `Draft` back when there is none.
 *
 * Identity is how a caller tells a real change from a no-op, which matters
 * because the caller schedules the write: a save scheduled for a change that
 * did not happen is a write of the document as it already is.
 */
export function changeShown(draft: Draft, change: (note: Note) => Note): Draft {
  const id = draft.currentId
  const note = id === undefined ? undefined : draft.edits[id]
  if (id === undefined || !note) return draft
  return { ...draft, edits: { ...draft.edits, [id]: change(note) } }
}

/**
 * A region drag grabbed a note: bring it in and show it.
 *
 * Its own copy in `edits` wins if it has one, so grabbing a box that was moved
 * earlier continues from where it is now rather than from where the server
 * still has it. The panel is untouched — a grab happens while the editor is
 * already open, and this is which note it is open *on*.
 */
export function grabNote(draft: Draft, note: Note): Draft {
  return {
    ...draft,
    edits: draft.edits[note.id]
      ? draft.edits
      : { ...draft.edits, [note.id]: note },
    currentId: note.id,
  }
}

/**
 * Give a note a new anchor — a swept rectangle or a planted point.
 *
 * By id rather than "the one on show", because the note a drag ends on is not
 * always the one the panel was showing when it began. A note this session has
 * not touched is not anchored: the same draft comes back, since the drag had
 * nothing to write onto.
 */
export function anchorNote(
  draft: Draft,
  id: string,
  region: Region,
  now: string,
): Draft {
  const note = draft.edits[id]
  if (!note) return draft
  return {
    ...draft,
    edits: { ...draft.edits, [id]: { ...note, region, updated: now } },
  }
}

/**
 * `a`: show the review list, or close it if that is what is open.
 *
 * **It cannot reach a reader mid-edit, and this comment used to say it did.**
 * This function has a second arm, where `panel: 'edit'` becomes `'review'`,
 * and no keypress gets to it: `keyplan.ts` returns no plan at all for
 * anything but `Escape` while the editor is open, because the editor is a
 * dialog that takes text and a keystroke there means the note. That is the
 * behaviour a reader typing a note wants, so it is the comment that was
 * wrong rather than the guard — the arm is reachable only by a caller that
 * does not exist. The edits are left alone: swapping surfaces is not
 * finishing, and a reader who presses `a` mid-session has not asked to drop
 * what they have touched.
 */
export function toggleReview(draft: Draft): Draft {
  return { ...draft, panel: draft.panel === 'review' ? undefined : 'review' }
}

/**
 * A note was deleted: forget this session's copy of it and put the page back.
 *
 * The other edits stay. Deleting one note is not abandoning the rest, and the
 * write that carries the deletion carries them too.
 */
export function afterDelete(draft: Draft, id: string): Draft {
  const rest = { ...draft.edits }
  delete rest[id]
  return { panel: undefined, edits: rest, currentId: undefined }
}

/**
 * Stored notes with this session's edits over them, plus the ones it invented.
 *
 * So a box that has been nudged draws where it was nudged to, not where the
 * server still has it, and a note that has never been written draws at all.
 *
 * Unlike `editsToWrite`, a blank invented note is **kept**: this is what is
 * drawn, and a reader who has just pressed `n` needs to see the note they are
 * about to write. It is the write that decides an untouched draft is not a
 * note, not the drawing.
 */
export function withEdits(stored: Note[], edits: Record<string, Note>): Note[] {
  return [
    ...stored.map((n) => edits[n.id] ?? n),
    ...Object.values(edits).filter((e) => !stored.some((n) => n.id === e.id)),
  ]
}

/**
 * Those, filtered to one edition when `travel` is off — filtered **before**
 * resolving.
 *
 * Before rather than after, so switching the setting off costs no resolution
 * at all: the module is then genuinely doing nothing, which is the property
 * every module's manual pass checks for.
 */
export function visibleNotes(
  stored: Note[],
  edits: Record<string, Note>,
  travel: boolean,
  viewing: string,
): Note[] {
  const all = withEdits(stored, edits)
  return travel ? all : all.filter((n) => n.edition === viewing)
}

/**
 * Whether two documents are the same, for history's sake.
 *
 * By serialised content rather than by reference: every write produces fresh
 * objects, so reference equality would record a step for a write that changed
 * nothing and leave undo needing two presses to do anything visible.
 */
export function sameNotes(a: Note[], b: Note[]): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

/**
 * Where the list of this page's notes goes: beside the page, or under it.
 *
 * `auto` is the default and answers by width, because the choice is really
 * "is there room beside the page" — a portrait page in a wide window leaves
 * margin the list can live in, and the same page in a narrow one does not, so
 * the list would either crush the page or cover it. Below is the fallback
 * because a column under the page costs height a reader can scroll, where a
 * column beside it costs width a page cannot spare.
 *
 * A setting rather than only a rule: which one is right depends on the screen
 * *and* on the reader, and `auto` guessing wrongly on somebody's monitor is
 * not a reason to make them live with it.
 *
 * An unrecognised setting behaves as `auto` rather than refusing — the same
 * fallback `noteColour` makes, for the same reason.
 */
export function listPlacement(
  setting: string,
  viewportWidth: number,
): 'side' | 'below' {
  if (setting === 'side') return 'side'
  if (setting === 'below') return 'below'
  return viewportWidth >= 1100 ? 'side' : 'below'
}

/**
 * The exported document, formatted to be read by a person as well as parsed.
 *
 * Exactly what the server holds — the same shape, not a rendering — so an
 * export can be put back. A prose rendering would be a second format to keep
 * in step with the first, and this one is already the interchange format.
 */
export function notesJson(doc: NotesDoc): string {
  return JSON.stringify(doc, null, 2)
}

/** What an exported file is called: the work's own id, plus the server's suffix. */
export function exportName(work: string): string {
  return `${work}.notes.json`
}

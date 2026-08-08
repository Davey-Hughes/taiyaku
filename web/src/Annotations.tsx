import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react'
import type { AnnotationsPrefs } from './modules/annotations'
import {
  isPoint,
  spreadTabs,
  tabOffset,
  type NoteHere,
  type Region,
} from './notes'
import { noteColour, type NoteColour } from './palette'

/**
 * Notes on the page: a tab in the margin for each, and — while revealed — a
 * tint over each region.
 *
 * Composited **above** the page rather than inside it, so no other module's
 * transform can move a marker away from the region it names. Nothing here is
 * derived: `here` is `notes.ts`'s `notesHere` and each tab's position is its
 * `tabOffset`, both pure and tested. This file lays them out and decides
 * nothing.
 *
 * `pointerEvents: 'none'` on the layer with `'auto'` on the tabs themselves:
 * the overlay must never swallow a click meant for the page — including the
 * drag that sets a region — and a tab must still be clickable.
 *
 * **A travelled note never draws a tint.** Its region belongs to the slot it
 * was written on, and transforming one between editions is L1 geometry, a
 * *Later* item. Drawing the untransformed box here would put it over the wrong
 * panel, which is the reader lying about what it knows; the tab says the note
 * is here, which is all this build actually knows.
 */
const layerStyle = (box: Box): CSSProperties => ({
  position: 'absolute',
  left: box.left,
  top: box.top,
  width: box.width,
  height: box.height,
  pointerEvents: 'none',
  zIndex: 2,
})

/** Where the drawn page sits inside the page area, in that area's own pixels. */
export interface Box {
  left: number
  top: number
  width: number
  height: number
}

/**
 * A note's tab, in the margin **beside** the page rather than on it.
 *
 * `left: 100%` places it just past the page's right edge, which is where the
 * design puts it — "fit-whole guarantees the side margins the tabs live in on
 * every portrait page", and nothing is drawn over artwork. This layer is
 * anchored to the page itself (regions are fractions of the page, so it has to
 * be), which makes `right: 2` mean two pixels *inside* the artwork rather than
 * two pixels from the edge of the reader: a 10px translucent sliver on the
 * page's own right margin, against 609px of empty background beside it.
 * Reported as not being able to see the markers at all, and fair — it was
 * hiding on the page.
 *
 * The assumption this rests on is the design's own: a portrait page in a
 * landscape viewport leaves margin. Where it does not — a page as wide as the
 * area it is drawn in — the tab would sit outside the page area's own
 * `overflow: hidden` and be clipped. No page in this corpus does that, and the
 * honest fix if one ever does is to measure the margin rather than to move the
 * tab back onto the artwork.
 */
/**
 * How far apart two tabs must sit, as a fraction of the page's height.
 *
 * A tab is 26px tall; on a page drawn around 530px high that is roughly 0.05,
 * so this leaves a hair of daylight between two that would otherwise touch.
 */
const TAB_MIN_GAP = 0.055

const tabStyle = (
  offset: number,
  travelled: boolean,
  colour: string,
): CSSProperties => ({
  position: 'absolute',
  left: '100%',
  marginLeft: 4,
  top: `${offset * 100}%`,
  transform: 'translateY(-50%)',
  pointerEvents: 'auto',
  width: 18,
  height: 22,
  padding: 0,
  border: 'none',
  borderRadius: 3,
  // The tab carries the note's number, which is what ties it to the row in the
  // list beside or below the page — so a reader reading a note there can find
  // what it points at without hunting.
  fontSize: 11,
  lineHeight: '22px',
  textAlign: 'center',
  color: travelled ? colour : 'rgba(20, 20, 20, 0.9)',
  fontWeight: 600,
  // Solid is written here, hollow is written elsewhere — a distinction drawn
  // in *fill* rather than in colour, which is what leaves colour free to mean
  // whatever the reader decided it means. Two notes of different colours on
  // one page still read as travelled or not at a glance.
  background: travelled ? 'transparent' : colour,
  boxShadow: `inset 0 0 0 2px ${colour}`,
  cursor: 'pointer',
})

/**
 * The note's own words, inside the region it marks.
 *
 * **This is a deliberate departure from the design**, which chose margin tabs
 * precisely so that "nothing is ever drawn over artwork" and left the body in
 * the panel. Asked for directly: a box drawn around a panel with no hint of
 * what it says is a marker you have to go and interrogate, and the whole point
 * of anchoring a note to a region is that the two are read together. The
 * design's concern is answered rather than ignored — the text is backed only
 * behind itself, not across the region, so the art inside the box stays
 * visible; it appears only where a region is already being shown (revealed, or
 * being edited), never while merely reading; and it is clipped to its own box
 * rather than spilling across the page.
 *
 * Clipped rather than scrolled or wrapped indefinitely: a region is sized by
 * what a reader drew around, not by how much they wrote, so a long note in a
 * small box shows its first lines and the rest stays in the panel. `title` on
 * the tab and the review list are both still whole.
 */
const bodyStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  bottom: 0,
  margin: 0,
  padding: '2px 5px',
  maxHeight: '100%',
  overflow: 'hidden',
  background: 'rgba(16, 16, 16, 0.82)',
  color: 'rgba(255, 255, 255, 0.95)',
  fontSize: '0.78em',
  lineHeight: 1.25,
  // Long words (a URL, a Japanese run with no spaces) must wrap rather than
  // push the label wider than the region it belongs to.
  overflowWrap: 'anywhere',
}

/**
 * A point anchor, drawn where a box cannot be.
 *
 * A ring rather than a filled dot: a solid blob hides the very detail a reader
 * pointed at, where a ring puts the artwork in the middle of it. Centred on
 * the point by translating half its own size, so it marks the spot rather than
 * hanging below and right of it.
 */
const pinStyle = (region: Region, colour: NoteColour): CSSProperties => ({
  position: 'absolute',
  left: `${region.x * 100}%`,
  top: `${region.y * 100}%`,
  width: 18,
  height: 18,
  marginLeft: -9,
  marginTop: -9,
  borderRadius: '50%',
  border: `2px solid ${colour.edge}`,
  background: colour.tint,
  boxShadow: '0 0 0 1px rgba(0, 0, 0, 0.7), inset 0 0 0 1px rgba(0, 0, 0, 0.5)',
})

const tintStyle = (region: Region, colour: NoteColour): CSSProperties => ({
  position: 'absolute',
  left: `${region.x * 100}%`,
  top: `${region.y * 100}%`,
  width: `${region.w * 100}%`,
  height: `${region.h * 100}%`,
  background: colour.tint,
  outline: `1px solid ${colour.edge}`,
})

/**
 * The region being drawn or edited right now, before it is saved.
 *
 * Drawn **regardless of `reveal`**, and that is deliberate rather than an
 * exception: `reveal` answers "show me every region on this page", a question
 * about notes that already exist. A rectangle a reader is dragging out this
 * second is not a stored region being revealed, it is the direct feedback for
 * the gesture in progress — and without it the drag is invisible, which is
 * exactly how this shipped and was reported.
 *
 * **Darker while the pointer is down, light once it settles**, because the two
 * moments want opposite things. A manga page is mostly white, so a pale wash
 * over it was reported as hard to see while drawing; a dark fill reads at a
 * glance against exactly that background. But the same dark fill sitting on a
 * settled region would obscure the artwork the note is *about*, which is the
 * one thing a marker must not do. So the fill drops away the moment the drag
 * ends and the box stays as a dashed outline.
 *
 * The outline is double — a dark line under a bright one — so the edge holds
 * against both a white page and a black panel border, rather than picking one
 * and disappearing into the other.
 */
const pendingStyle = (
  region: Region,
  drawing: boolean,
  colour: NoteColour,
): CSSProperties =>
  // **A point being edited has to be drawn as a point.** As a rectangle it is
  // zero by zero — an outline around nothing, which is invisible, and a reader
  // cannot take hold of something they cannot see. That is why a point could
  // not be moved: not the grab test, which had slop for exactly this, but that
  // there was nothing on screen to aim at. Same ring the saved ones use, dashed
  // to say it is not written yet.
  isPoint(region)
    ? {
        position: 'absolute',
        left: `${region.x * 100}%`,
        top: `${region.y * 100}%`,
        width: 20,
        height: 20,
        marginLeft: -10,
        marginTop: -10,
        borderRadius: '50%',
        border: `2px dashed ${colour.edge}`,
        background: drawing ? 'rgba(0, 0, 0, 0.42)' : colour.tint,
        boxShadow: '0 0 0 1px rgba(0, 0, 0, 0.85)',
      }
    : {
        position: 'absolute',
        left: `${region.x * 100}%`,
        top: `${region.y * 100}%`,
        width: `${region.w * 100}%`,
        height: `${region.h * 100}%`,
        background: drawing ? 'rgba(0, 0, 0, 0.42)' : colour.tint,
        outline: `2px dashed ${colour.edge}`,
        boxShadow:
          '0 0 0 1px rgba(0, 0, 0, 0.85), inset 0 0 0 1px rgba(0, 0, 0, 0.85)',
      }

/**
 * A note's words beside its tab, while the pointer is on it.
 *
 * Anchored to the tab's own vertical position and drawn to the *left*, into
 * the page rather than off the edge of the reader — a tab sits in the right
 * margin, so a popover to its right would run out of window.
 */
const popoverStyle = (offset: number): CSSProperties => ({
  position: 'absolute',
  // Just inside the page's right edge, beside the tab that summoned it.
  //
  // It used to be placed at `right: calc(100% + 8px)` — outside the layer
  // entirely — and an absolutely positioned box given only a `right` offset
  // takes its width from the space left of that offset, which here was none.
  // So it shrank to the narrowest thing that could hold a character and the
  // text came out one letter per line, read as "completely vertical". A fixed
  // `width` rather than `maxWidth` is the other half: shrink-to-fit is exactly
  // what went wrong, so nothing here is left to it.
  right: 6,
  top: `${offset * 100}%`,
  transform: 'translateY(-50%)',
  width: 240,
  padding: '6px 9px',
  borderRadius: 6,
  background: 'rgba(16, 16, 16, 0.94)',
  color: 'rgba(255, 255, 255, 0.95)',
  fontSize: '0.82em',
  lineHeight: 1.3,
  pointerEvents: 'none',
  overflowWrap: 'anywhere',
  boxShadow: '0 6px 18px rgba(0, 0, 0, 0.5)',
})

export interface AnnotationsProps {
  /**
   * Where the page is drawn inside the page area. Every marker here is
   * positioned as a percentage of *this* box, not of the page area, because a
   * region is fractions of the page (`notes.ts`'s `Region`) and the area
   * around a page is letterbox.
   *
   * `undefined` before the first measurement, and for one frame only — the
   * layer renders nothing rather than falling back to the page area, since
   * that fallback is exactly the wrong-by-a-letterbox placement this prop
   * exists to fix, and a fallback that looks nearly right is worse than a
   * frame of nothing.
   */
  box: Box | undefined
  /** The notes on this slot — `notes.ts`'s `notesHere`, already filtered for travel. */
  here: NoteHere[]
  prefs: AnnotationsPrefs
  /**
   * The region on the draft being edited on this slot: the live rectangle
   * while a drag is in flight, and the settled one after the pointer is
   * released but before Save. Absent when the editor is closed, or open on
   * another slot, or the draft anchors the whole page.
   */
  pending?: Region
  /** Whether the pointer is down on `pending` right now — see `pendingStyle`. */
  drawing?: boolean
  /** The draft's text, shown inside `pending` so a reader sees the note against the region it marks. */
  pendingBody?: string
  /** The draft's colour name, so the rectangle being drawn is already the colour it will be. */
  pendingColour?: string
  onOpen: (id: string) => void
}

export function Annotations({
  box,
  here,
  prefs,
  pending,
  drawing,
  pendingBody,
  pendingColour,
  onOpen,
}: AnnotationsProps) {
  const [hovered, setHovered] = useState<string>()
  if (!box) return null
  // `pending` alone is reason enough to mount: a reader dragging a region on a
  // page with no notes yet, or with markers switched off, still has to see
  // what they are drawing.
  if (!pending && here.length === 0) return null
  if (!pending && !prefs.markers && !prefs.reveal) return null
  return (
    <div style={layerStyle(box)}>
      {pending && (
        <div
          style={pendingStyle(
            pending,
            drawing === true,
            noteColour(pendingColour),
          )}
        >
          {/*
            Not while the pointer is down: a label under a rectangle still
            being swept out covers the very artwork the reader is drawing
            around.

            And never on a **point**, which is 20 pixels across — a label
            inside one is squeezed to a column a character wide, which is
            unreadable and, worse, hides the point itself while it is being
            moved. A point's words live in the list beside the page and in the
            popover on its tab, which is what those surfaces are for.
          */}
          {!drawing && !isPoint(pending) && pendingBody && (
            <p style={bodyStyle}>{pendingBody}</p>
          )}
        </div>
      )}
      {prefs.reveal &&
        here
          .filter((h) => !h.travelled && h.note.region)
          .map((h) =>
            isPoint(h.note.region) ? (
              <div
                key={`pin-${h.note.id}`}
                style={pinStyle(h.note.region!, noteColour(h.note.color))}
              />
            ) : (
              <div
                key={`tint-${h.note.id}`}
                style={tintStyle(h.note.region!, noteColour(h.note.color))}
              >
                {/* A point has no box to hold words, which is the other half of
                    why the list beside the page exists — see `NoteList`. */}
                {h.note.body && <p style={bodyStyle}>{h.note.body}</p>}
              </div>
            ),
          )}
      {prefs.markers &&
        // Positions computed for the whole set at once, so two notes on one
        // slot cannot draw one tab on top of another — see `spreadTabs`. A
        // whole-slot note has no region to derive a position from, so every
        // one of them would otherwise sit at the middle.
        spreadTabs(
          here.map((h) => tabOffset(h.note)),
          TAB_MIN_GAP,
        ).map((offset, index) => {
          const h = here[index]
          const hovering = hovered === h.note.id
          return (
            <span key={h.note.id}>
              {/*
              The note itself, next to its tab, while the pointer is on it.
              Styled rather than the browser's `title`, which appears after a
              delay a reader has usually given up on, cannot be styled to say
              which edition a travelled note came from, and never shows at all
              on a touch screen. `title` is gone rather than kept alongside, so
              there is one answer to hovering a tab and not two arriving a
              second apart.
            */}
              {hovering && h.note.body && (
                <span style={popoverStyle(offset)}>
                  {h.travelled && (
                    <em style={{ opacity: 0.75 }}>on {h.note.edition}: </em>
                  )}
                  {h.note.body}
                </span>
              )}
              <button
                key={h.note.id}
                type="button"
                style={tabStyle(
                  offset,
                  h.travelled,
                  noteColour(h.note.color).tab,
                )}
                onClick={() => onOpen(h.note.id)}
                onPointerEnter={() => setHovered(h.note.id)}
                onPointerLeave={() =>
                  setHovered((current) =>
                    current === h.note.id ? undefined : current,
                  )
                }
                // The body is the title as well as the label: a reader hovering a
                // tab wants the note, not a description of the tab. An empty body
                // — a note saved before anything was typed — says so rather than
                // showing a blank tooltip that reads as a broken one.
                aria-label={
                  h.travelled
                    ? `Note written on ${h.note.edition}: ${h.note.body || 'empty'}`
                    : `Note: ${h.note.body || 'empty'}`
                }
              >
                {index + 1}
              </button>
            </span>
          )
        })}
    </div>
  )
}

/**
 * `Annotations`, measuring the page it sits over.
 *
 * Mounts an `inset: 0` wrapper inside whatever positioned box holds the page,
 * finds the image in that box, and measures it — so every marker is placed as
 * a fraction of the **page**, not of the area around it. That distinction is
 * the whole reason this exists: a region is fractions of a slot, and a
 * portrait page in a landscape viewport leaves letterbox down both sides that
 * is background rather than page. Placing markers against the area drew them
 * offset and over-scaled, reported as corners that do not line up with the
 * pointer.
 *
 * Self-measuring rather than measured by `App.tsx`, because the three reading
 * modes hold their pages in different boxes and continuous scroll holds one
 * per group. A component that finds its own page works in all three; a bespoke
 * effect in `App.tsx` worked in two and left scroll mode with no markers at
 * all.
 *
 * `match` names which image to measure by URL suffix, for the boxes that hold
 * more than one — a two-page spread draws two, and only one of them is the
 * slot being annotated. Falls back to the first image where nothing matches,
 * which is the single-image case stated rather than assumed.
 *
 * A `ResizeObserver` on both boxes rather than a window `resize` listener: an
 * image's box changes for reasons the window never hears about — a new page
 * decoding at a different aspect, the chrome appearing — and a stale
 * measurement means every marker is drawn somewhere the page is not.
 *
 * **Give this a `key` that changes when the tree holding the page is rebuilt**
 * — the reading mode, at least. The observer watches one element, and changing
 * mode replaces it: single mode draws a bare `PageView`, two-page mode wraps
 * two of them in cells, scroll mode mounts one per group. Without a remount
 * the observer goes on watching an image React has already removed, fires once
 * with a zero-sized rect, and never finds its replacement — measured directly,
 * as an overlay collapsed to nothing in the top-left corner after switching
 * single → two.
 */
export function PageAnnotations({
  match,
  version,
  ...rest
}: Omit<AnnotationsProps, 'box'> & { match?: string; version?: unknown }) {
  const anchor = useRef<HTMLDivElement>(null)
  const [box, setBox] = useState<Box>()
  // The measurement the subscribing effect below built, held for the effect
  // after it to re-run without rebuilding anything. See that second effect for
  // why the two are separate at all.
  const remeasure = useRef<(() => void) | undefined>(undefined)

  // Subscribing: which element is watched, and how it is measured. Keyed on
  // `match` alone, so this runs when the page it watches changes and at no
  // other time.
  useLayoutEffect(() => {
    const host = anchor.current?.parentElement
    if (!host) return
    const images = [...host.querySelectorAll('img')]
    const img =
      (match
        ? images.find((candidate) => candidate.src.endsWith(match))
        : undefined) ?? images[0]
    if (!img) {
      setBox(undefined)
      return
    }
    const measure = () => {
      const outer = host.getBoundingClientRect()
      const inner = img.getBoundingClientRect()
      // A zero-sized measurement is not a page, it is an image that has been
      // detached or has not been laid out yet — React removing this element
      // while rebuilding the tree fires the observer with exactly that, and
      // taking it would collapse the overlay to a point in the corner. Keeping
      // the last real box instead is safe: whatever replaced this image will
      // be found by a fresh mount (see `key` at the call sites), and until then
      // the markers stay where the page last was rather than jumping to 0,0.
      if (inner.width === 0 || inner.height === 0) return
      setBox({
        left: inner.left - outer.left,
        top: inner.top - outer.top,
        width: inner.width,
        height: inner.height,
      })
    }
    remeasure.current = measure
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(img)
    observer.observe(host)
    return () => {
      observer.disconnect()
      remeasure.current = undefined
    }
  }, [match])

  // Re-measuring, which is a different event from re-subscribing and used to
  // be the same one.
  //
  // `version` is not read below. It is a trigger: a `ResizeObserver` never
  // fires on a CSS transform, because a transform changes nothing about layout
  // size, so a zoom or a pan would otherwise leave every marker where the page
  // used to be. The arithmetic above needs no change for zoom — both rects it
  // reads come from `getBoundingClientRect()`, which reports post-transform
  // geometry — so this is a staleness fix, not a frame-of-reference one. A
  // stale overlay looks exactly like a wrong transform, which is why it is
  // threaded down rather than left to be noticed.
  //
  // It was a dependency of the effect above until it got its own, which meant
  // the observer was constructed and disconnected once per view change —
  // sixty times a second for the length of a drag, to watch the same two
  // elements it was already watching. The element being observed changes when
  // `match` does; the measurement has to be redone far more often than that.
  //
  // What the split gives up: a `version` change no longer re-resolves *which*
  // `<img>` is being watched, only re-measures the one already found. Nothing
  // needs it to — React reconciles `PageView`'s single image in place, so the
  // element changes only when `match` does or when the `key` at the call site
  // remounts this component, which is what that `key` is documented above to
  // be for.
  //
  // Runs a second time on mount, after the effect above has already measured
  // once. Both land in the same commit's layout phase and React batches the
  // two `setBox` calls into one render, so the cost is one redundant
  // `getBoundingClientRect` pair at mount — cheaper than the first-run flag it
  // would take to skip it.
  //
  // biome-ignore lint/correctness/useExhaustiveDependencies: version is a trigger, not a closure read — see above. Removing it, which is what this rule suggests, would leave every marker where the page was before the zoom.
  useLayoutEffect(() => {
    remeasure.current?.()
  }, [version])

  return (
    <div
      ref={anchor}
      style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}
    >
      <Annotations box={box} {...rest} />
    </div>
  )
}

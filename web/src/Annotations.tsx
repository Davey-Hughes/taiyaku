import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react'
import type { AnnotationsPrefs } from './modules/annotations'
import type { ArtworkHug } from './modules/fit'
import {
  artOffset,
  isPoint,
  paintedBox,
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

/**
 * Where the page's **artwork** sits inside the page area, in that area's own
 * pixels — not where the `<img>` element sits, which since `upscaleToFit` is
 * a much larger box with the artwork somewhere inside it. `PageAnnotations`
 * builds this through `notes.ts`'s `paintedBox`, which is the one rule for
 * that distinction and has the measurements.
 */
export interface Box {
  left: number
  top: number
  width: number
  height: number
}

/**
 * How far apart two tabs must sit, as a fraction of the page's height.
 *
 * A tab is 22px tall; on a page drawn around 530px high that is about
 * 0.04, so this leaves a few pixels of daylight between two that would
 * otherwise touch.
 */
const TAB_MIN_GAP = 0.055

/**
 * A note's tab, in the margin **beside** the page rather than on it — on the
 * side that page actually has a margin on.
 *
 * `'right'` is the default and what every mode but one draws: `left: 100%`
 * with a 4px offset, just past the page's right edge, which is where the
 * design puts it and where nothing is drawn over artwork. `'left'` is the
 * mirror, `right: 100%` with the same offset, and exists for one caller — the
 * first cell of a real two-page pair.
 *
 * **In `two` mode a margin is guaranteed on the outer side of each page and
 * nowhere else.** `contain` leaves the same letterbox on both sides of a page
 * centred in its box, so either side would do and the right is the default.
 * `fit.ts`'s `hug` closes the two-page seam by pushing each half's artwork
 * against it, and pays for that with the whole of one margin per page. Which
 * margin is therefore this parameter. Single mode and scroll mode never hug,
 * which is what the default keeps free.
 *
 * Two alternatives, both rejected:
 *
 * - **Hold a gap open whenever the left page carries a note.** `fit.ts`'s
 *   `gap` already reopens a measured amount of seam, so this needs no side at
 *   all. It buys the margin back at the cost of a seam that opens and closes
 *   as pages turn — a layout that moves for a reason nothing on screen
 *   explains reads as a bug, not as room being made for a marker.
 * - **Let the tabs overlap.** Cheapest of all: no change anywhere. A tab drawn
 *   past the left page's right edge lands on the facing page's artwork, which
 *   asserts a note is on a page it is not on. Being hard to find is a
 *   nuisance; being on the wrong page is the reader lying about what it knows.
 *
 * This layer is anchored to the artwork itself (regions are fractions of the
 * page, so it has to be), which would make `right: 2` mean two pixels *inside*
 * the artwork rather than two pixels from the edge of the reader.
 *
 * The assumption this rests on is the design's own: a portrait page in a
 * landscape viewport leaves margin. Where it does not — a page as wide as the
 * box it is drawn in — the tab sits outside the page area's own `overflow:
 * hidden` and is clipped. **`upscaleToFit` makes every page's `<img>` exactly
 * that wide**, so this layer is anchored to the artwork that `PageAnnotations`
 * below measures rather than to the element, which puts the tab in the real
 * background beside the page.
 */
const tabStyle = (
  offset: number,
  travelled: boolean,
  colour: string,
  side: 'left' | 'right',
): CSSProperties => ({
  position: 'absolute',
  ...(side === 'left'
    ? { right: '100%', marginRight: 4 }
    : { left: '100%', marginLeft: 4 }),
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
  color: travelled ? colour : 'var(--art-ink)',
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
 * the panel. A box drawn around a panel with no hint of what it says is a
 * marker you have to go and interrogate, and the whole point of anchoring a
 * note to a region is that the two are read together. The design's concern is
 * answered rather than ignored — the text is backed only behind itself, not
 * across the region, so the art inside the box stays visible; it appears only
 * where a region is already being shown (revealed, or being edited), never
 * while merely reading; and it is clipped to its own box rather than spilling
 * across the page.
 *
 * Clipped rather than scrolled or wrapped indefinitely: a region is sized by
 * what a reader drew around, not by how much they wrote, so a long note in a
 * small box shows its first lines and the rest stays in the panel. The tab's
 * popover and the review list are both still whole.
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
  background: 'var(--art-caption)',
  color: 'var(--art-caption-text)',
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
  boxShadow:
    '0 0 0 1px var(--art-edge-soft), inset 0 0 0 1px var(--art-edge-faint)',
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
 * the gesture in progress — and without it the drag is invisible.
 *
 * **Darker while the pointer is down, light once it settles**, because the two
 * moments want opposite things. A manga page is mostly white, so a pale wash
 * over it is hard to see while drawing; a dark fill reads at a glance against
 * exactly that background. But the same dark fill sitting on a settled region
 * would obscure the artwork the note is *about*, which is the one thing a
 * marker must not do. So the fill drops away the moment the drag ends and the
 * box stays as a dashed outline.
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
  // cannot take hold of something they cannot see. Same ring the saved ones
  // use, dashed to say it is not written yet.
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
        background: drawing ? 'var(--art-drawing)' : colour.tint,
        boxShadow: '0 0 0 1px var(--art-edge)',
      }
    : {
        position: 'absolute',
        left: `${region.x * 100}%`,
        top: `${region.y * 100}%`,
        width: `${region.w * 100}%`,
        height: `${region.h * 100}%`,
        background: drawing ? 'var(--art-drawing)' : colour.tint,
        outline: `2px dashed ${colour.edge}`,
        boxShadow: '0 0 0 1px var(--art-edge), inset 0 0 0 1px var(--art-edge)',
      }

/**
 * A note's words beside its tab, while the pointer is on it.
 *
 * Anchored to the tab's own vertical position and drawn *into the page*, away
 * from whichever margin the tab is in — the same `side` `tabStyle` takes. A
 * popover outside a right-margin tab runs off the edge of the reader. Outside
 * a left-margin tab it has only whatever the outer letterbox happens to be,
 * with nothing promising it holds this box's fixed 240px. Into the page is the
 * one side that is always as wide as the page.
 *
 * Its look is `.reader-popover`'s, the panels' (spec 2026-09-17 §8.5).
 */
const popoverStyle = (
  offset: number,
  side: 'left' | 'right',
): CSSProperties => ({
  position: 'absolute',
  // Just inside the page's own edge, beside the tab that summoned it.
  //
  // Placed outside the layer, an absolutely positioned box given only a
  // `right` offset takes its width from the space left of that offset, which
  // would be none: it would shrink to the narrowest thing that could hold a
  // character and the text would come out one letter per line. A fixed
  // `width` rather than `maxWidth` is the other half: shrink-to-fit is
  // exactly what would go wrong, so nothing here is left to it.
  ...(side === 'left' ? { left: 6 } : { right: 6 }),
  top: `${offset * 100}%`,
  transform: 'translateY(-50%)',
  width: 240,
  padding: '6px 9px',
  fontSize: '0.82em',
  lineHeight: 1.3,
  pointerEvents: 'none',
  overflowWrap: 'anywhere',
})

export interface AnnotationsProps {
  /**
   * Where the page's artwork is drawn inside the page area. Every marker here
   * is positioned as a percentage of *this* box, not of the page area and not
   * of the `<img>` element either, because a region is fractions of the page
   * (`notes.ts`'s `Region`) and everything around the artwork — inside the
   * element or outside it — is letterbox.
   *
   * `undefined` before the first measurement, which waits for the image to
   * decode — the layer renders nothing rather than falling back to the page
   * area, since that fallback is exactly the wrong-by-a-letterbox placement
   * this prop exists to fix, and a fallback that looks nearly right is worse
   * than a frame of nothing.
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
  /**
   * Which margin the note tabs sit in — see `tabStyle`, which has the whole
   * argument. `'right'` by default, so single mode, scroll mode and every
   * call site that passes nothing draw the right margin; only the first cell
   * of a real two-page pair asks for `'left'`, because the seam has taken its
   * right margin.
   */
  tabSide?: 'left' | 'right'
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
  tabSide = 'right',
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
              on a touch screen. No `title` is kept alongside, so there is one
              answer to hovering a tab and not two arriving a second apart.
            */}
              {hovering && h.note.body && (
                <span
                  className="reader-popover"
                  style={popoverStyle(offset, tabSide)}
                >
                  {h.travelled && (
                    <em className="reader-muted">on {h.note.edition}: </em>
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
                  tabSide,
                )}
                onClick={() => onOpen(h.note.id)}
                onPointerEnter={() => setHovered(h.note.id)}
                onPointerLeave={() =>
                  setHovered((current) =>
                    current === h.note.id ? undefined : current,
                  )
                }
                // The body is the label: a reader who reaches a tab wants
                // the note, not a description of the tab. An empty body — a
                // note saved before anything was typed — says so rather than
                // leaving a blank label that reads as a broken one.
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
 * is background rather than page. Placing markers against the area would draw
 * them offset and over-scaled.
 *
 * **There are two letterboxes, and this holds the artwork's box rather than
 * the element's.** With `upscaleToFit` on by default, the `<img>` fills its
 * cell and `object-fit: contain` draws the page inside it, so the element
 * carries letterbox of its own on top of whatever the cell leaves around the
 * element. Measuring the element is wrong in every mode; `notes.ts`'s
 * `paintedBox` is the rule, applied inside `measure` so a zoom, a pan, a
 * resize and a decode all go back through it.
 *
 * Self-measuring rather than measured by `App.tsx`, because the three reading
 * modes hold their pages in different boxes and continuous scroll holds one
 * per group. A component that finds its own page works in all three.
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
 * with a zero-sized rect, and never finds its replacement, so the overlay
 * collapses to nothing in the top-left corner after switching single → two.
 */
export function PageAnnotations({
  match,
  version,
  hug,
  ...rest
}: Omit<AnnotationsProps, 'box'> & {
  match?: string
  version?: unknown
  /**
   * Which edge of its own element this page's artwork is pulled to —
   * `fit.ts`'s `ArtworkHug`, the same value `App.tsx` hands the `PageView`
   * underneath. Part of *where the artwork is*, so it belongs to the
   * measurement rather than to the drawing: `'centre'` by default, and only a
   * real two-page pair's two cells pass anything else.
   */
  hug?: ArtworkHug
}) {
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
      // **The element's rect is not the page.** `fit.ts`'s `upscaleToFit`
      // (default on) makes the `<img>` fill its whole cell and `object-fit:
      // contain` then draws the artwork somewhere inside it, so a marker at 25%
      // of the page would be drawn off the art it names. `paintedBox` is that
      // arithmetic — the same call `App.tsx`'s `pointOn` measures a drag
      // through, so the overlay and the gesture cannot come to different
      // answers about where the page is.
      //
      // Undecoded, `naturalWidth` is 0 and `paintedBox` answers "the whole
      // element", which is precisely the placement this is here to correct.
      // Stopping short of it rather than taking it: `AnnotationsProps.box`
      // says a fallback that looks nearly right is worse than a frame of
      // nothing, and the `load` listener below is what ends that frame.
      if (img.naturalWidth === 0 || img.naturalHeight === 0) return
      const art = paintedBox(
        { w: img.naturalWidth, h: img.naturalHeight },
        { w: inner.width, h: inner.height },
        hug,
      )
      // `artOffset` folds `outer`/`inner`'s own element-to-element offset
      // together with `art`'s inset — two terms this file does not compose
      // itself, since every fixture that exercises this measurement happens
      // to give the host and the image the same origin and would not notice
      // either term's sign or order coming out wrong. See `notes.ts`.
      const offset = artOffset(outer, inner, art)
      setBox({
        left: offset.left,
        top: offset.top,
        width: art.w,
        height: art.h,
      })
    }
    remeasure.current = measure
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(img)
    observer.observe(host)
    // A decode is the third thing that moves this box, and neither of the
    // other two triggers can see it: the measurement reads `naturalWidth`,
    // which is 0 until the image decodes, while the *element* is `100%` by
    // `100%` under `upscaleToFit` and so changes no layout size for the
    // `ResizeObserver` to fire on. `version` only changes on a zoom or a pan.
    //
    // Two things go wrong without it. The overlay draws nothing for the whole
    // of the first image's decode and never comes back. And on a page turn the
    // box keeps the *previous* page's natural size: `match` changing is what
    // re-runs this effect, and that happens in the commit before `PageView`
    // has swapped the newly decoded `src` in, so the `<img>` measured here is
    // still showing the page being turned away from. Two pages of one scan
    // share an aspect and hide it; a turn onto a joined spread, whose scan is
    // twice as wide and fits the same element quite differently, does not.
    img.addEventListener('load', measure)
    return () => {
      observer.disconnect()
      img.removeEventListener('load', measure)
      remeasure.current = undefined
    }
    // `hug` joins `match` because it is part of *how* the element is measured,
    // not of how often: it changes when the reader turns onto a page that sits
    // in the other cell, or flips the reading direction under a page that
    // stays put — at most once per keystroke, nothing like `version`'s sixty
    // times a second, which is the whole reason that one has an effect of its
    // own below.
  }, [match, hug])

  // Re-measuring, which is a different event from re-subscribing.
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
  // Were `version` a dependency of the effect above, the observer would be
  // constructed and disconnected once per view change — sixty times a second
  // for the length of a drag, to watch the same two elements it was already
  // watching. The element being observed changes when `match` does; the
  // measurement has to be redone far more often than that.
  //
  // What the split gives up: a `version` change does not re-resolve *which*
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

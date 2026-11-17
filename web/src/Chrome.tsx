import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import { pageUrl } from './api'
import {
  bottomBarStyle,
  progressStyle,
  readoutStyle,
  readoutTextStyle,
} from './bars'
import { Filmstrip, type FilmstripProps, type StripScroller } from './Filmstrip'
import {
  fadeStyle,
  isAnimating,
  scheduleFade,
  type MotionPrefs,
} from './modules/motion'
import { readoutRow, type ReadoutsPrefs } from './modules/readouts'
import { stepFor } from './modules/direction'
import type { Layout } from './layouts'
import type { RailShare } from './tiles'
import { TopBar } from './TopBar'

/// How large the hover preview may stand, as a fraction of the viewport.
///
/// Sized in `vh` rather than pixels because this is the one surface where a
/// reader is comparing two drawings rather than recognising one, so it should
/// take whatever the screen can give it. It draws the *page* (`pageUrl`), not
/// the tile — at 160px a thumbnail cannot answer "is this the same drawing",
/// which is the whole question a correction asks.
///
/// One request per tile the pointer comes over, and only while correcting. The
/// pointer comes over a tile by moving onto it, and as well by keeping still
/// while the strip moves under it: the wheel carries tiles under a still
/// pointer, and so does a drag of the strip let go while it is moving, which
/// glides on. Each tile that passes under the pointer that way is a request
/// too.
const previewMaxHeight = '55vh'

// The strip: its tiles at the layout's size plus the rail's padding and the
// tiles' borders. `--strip-tile` in `styles/reader.css` has the two sizes and
// why each — 160 px, the thumbnail's own height, and 96 in the phone layout —
// and a height that depends on the viewport is a custom property because an
// inline style cannot hold a media query.
const stripStyle: CSSProperties = { height: 'var(--strip-height)' }

// The mark on the progress line (O216): where along the volume the strip
// stands. The rail draws no scrollbar, and without this nothing says where
// the strip is once it is scrolled away from the page being read. Proud of
// the line above and below, so it reads as a mark on the line and not as
// part of the fill it may lie over.
//
// As wide as the share of the volume the strip shows, as a browser's
// scrollbar thumb is (O286), between `SHOWN_MARK_MIN` and `SHOWN_MARK_MAX`
// pixels: never too short to take with a mouse, and never so long on a short
// volume that it stops reading as a place on the line.
//
// `markStyle` is all of the mark's style that React is given. Its width, its
// `left` and whether it is hidden are written onto the element by `placeMark`
// in the component, and React leaves alone what it was never handed, so a
// render of `Chrome` does not move the mark.
const SHOWN_MARK_MIN = 64
const SHOWN_MARK_MAX = 160
// Its insets, 1 px proud above and below, are `styles/reader.css`'s, which
// grows them while the line is awake or held (O286).
const markStyle: CSSProperties = {
  position: 'absolute',
}

// Where a mouse can take hold of the progress line (O286): the line, and
// `LINE_GRIP_PX` above and below it, since a 2 px line is too thin to aim
// at. It draws nothing: what is seen there is the line, its fill and the
// mark. Above the line is the rail's own 6 px padding under the tiles, and
// below it the readout row's room above its centred text: 4 px takes
// neither a tile nor any of the row's text. Last of the line's children and
// positioned, so it lies over the mark and takes the press wherever it
// lands. `styles/reader.css` draws it only where some pointer is fine.
const LINE_GRIP_PX = 4
const gripStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  top: -LINE_GRIP_PX,
  bottom: -LINE_GRIP_PX,
}

// The line wakes, drawn thicker so it is easier to take (O286), at any move
// of a mouse over the reader, as an overlay scrollbar does, and sleeps
// `LINE_WAKE_IDLE_MS` after the last move or release. It stays awake while
// the pointer rests within `LINE_KEEP_PX` above or below the line, anywhere
// across the bar, or while a press holds it. A zone and not the grip, so a
// hand that drifts a little off the line does not see it shrink; it takes
// no press, so the tiles and the readout row under it are pressed as
// before.
const LINE_WAKE_IDLE_MS = 1000
const LINE_KEEP_PX = 16

/**
 * Wakes the line (its `data-awake`, which `styles/reader.css` draws thick)
 * at a mouse's moves over the window and puts it to sleep as the comment
 * above says. Returns what takes every listener, frame and timer off again
 * and leaves the line asleep.
 *
 * A move asks for at most one frame, and the attribute is written only when
 * it changes, so the moves themselves render nothing and lay nothing out.
 * One timer at a time: when it comes due early it waits out the rest.
 */
function wakeOnMouse(line: HTMLElement, held: () => boolean): () => void {
  let last = 0
  let x = 0
  let y = 0
  // Whether the pointer has left the window, where no move says where it is.
  let out = true
  let frame = 0
  let timer = 0
  let awake = false
  function set(on: boolean) {
    if (on === awake) return
    awake = on
    if (on) line.dataset.awake = ''
    else delete line.dataset.awake
  }
  function inZone() {
    if (out) return false
    const bar = (
      line.closest('.reader-bottom-bar') ?? line
    ).getBoundingClientRect()
    const box = line.getBoundingClientRect()
    return (
      x >= bar.left &&
      x <= bar.right &&
      y >= box.top - LINE_KEEP_PX &&
      y <= box.bottom + LINE_KEEP_PX
    )
  }
  function arm() {
    if (timer) return
    const idle = performance.now() - last
    timer = window.setTimeout(settle, Math.max(LINE_WAKE_IDLE_MS - idle, 0))
  }
  function settle() {
    timer = 0
    if (performance.now() - last < LINE_WAKE_IDLE_MS) {
      arm()
      return
    }
    // Resting near the line or holding it: awake, and the next move or the
    // release, which is what ends either, arms the idle again.
    set(held() || inZone())
  }
  function onActivity(event: PointerEvent) {
    if (event.pointerType !== 'mouse') return
    last = performance.now()
    x = event.clientX
    y = event.clientY
    out = false
    if (frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      set(true)
      arm()
    })
  }
  function onOut(event: PointerEvent) {
    if (event.pointerType !== 'mouse' || event.relatedTarget !== null) return
    out = true
    arm()
  }
  const listen = { capture: true, passive: true }
  window.addEventListener('pointermove', onActivity, listen)
  window.addEventListener('pointerup', onActivity, listen)
  document.addEventListener('pointerout', onOut, listen)
  return () => {
    window.removeEventListener('pointermove', onActivity, listen)
    window.removeEventListener('pointerup', onActivity, listen)
    document.removeEventListener('pointerout', onOut, listen)
    if (frame) cancelAnimationFrame(frame)
    clearTimeout(timer)
    set(false)
  }
}

/**
 * Ends a hold of the line: no longer drawn as held, and the pointer let go
 * where the line still has it. A line taken off the page has lost it
 * already.
 */
function letGo(held: { pointer: number; line: HTMLElement }) {
  delete held.line.dataset.held
  if (held.line.hasPointerCapture(held.pointer)) {
    held.line.releasePointerCapture(held.pointer)
  }
}

/** The mark's width, as CSS: the share of the line the strip shows, held between the two bounds. */
function markWidth(share: RailShare): string {
  const percent = (share.end - share.start) * 100
  return `clamp(${SHOWN_MARK_MIN}px, ${percent}%, ${SHOWN_MARK_MAX}px)`
}

/**
 * How far along its travel the rail has been scrolled, 0 at the start of the
 * volume and 1 at its end: the mark's place between the two ends of the line.
 *
 * Not `share.start`, which stops short of 1 by the share on show: the mark
 * has to stand at the end of the line when the rail stands at the end of its
 * travel. Held to 1, since the division can come out a hair past it at the
 * end of the travel.
 *
 * Only for a share that is less than the whole. The whole has no travel to
 * be along, and `Chrome` hides the mark for one without asking.
 */
function alongTheVolume(share: RailShare): number {
  return Math.min(share.start / (1 - (share.end - share.start)), 1)
}

// The hover preview: a page-sized look at the tile under the pointer, while
// correcting. Centred horizontally and standing 8 px clear of the bottom bar,
// so a reader's eye travels the shortest distance between the candidate and
// the page they are comparing it against.
//
// `zIndex: 3`, matching the two bars (this file's `bottomBarStyle`, above),
// which stand at 3 to clear `Annotations`' overlay at 2. This preview is drawn
// over the page while correcting, the one surface here that most needs to
// stand clear of everything else on it: at `auto`, note tints, tabs and the
// side list would draw over the page it shows.
const previewStyle: CSSProperties = {
  position: 'absolute',
  left: 'var(--space-4)',
  right: 'var(--space-4)',
  bottom: 'calc(var(--safe-bottom) + var(--reader-bottom-bar) + 8px)',
  zIndex: 3,
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  gap: 4,
  pointerEvents: 'none',
}

const previewImgStyle: CSSProperties = {
  maxHeight: previewMaxHeight,
  maxWidth: '100%',
  width: 'auto',
  objectFit: 'contain',
}

export interface ChromeProps extends FilmstripProps {
  /** Whether the two bars are shown at all. */
  visible: boolean
  /** The `motion` module's current prefs — drives the fade this component plays on showing. */
  motion: MotionPrefs
  /** Which of the readout lines are switched on — see `modules/readouts.ts`. */
  readouts: ReadoutsPrefs
  /**
   * The readout lines' text, already computed by `App.tsx` from
   * `modules/readouts.ts`'s pure functions — `Chrome` only lays them out, it
   * does not derive any of them itself. `position` and `chapter` are always
   * strings (`positionLine`/`chapterReadout` never answer "nothing to say");
   * `time` is a string too, once a session exists, but `health` is commonly
   * `undefined` — a sound pair with a counterpart says nothing, by design.
   *
   * Each is display-ready already: a percentage, a 1-based slot, a chapter
   * label — none of it is a value that could leak into storage, a URL or the
   * server the way a raw zero-based slot could.
   */
  position: string
  time?: string
  chapter: string
  health?: string
  /** The note count — derived by `modules/annotations.ts`, absent where the work has none. */
  notes?: string
  /** How far through the edition, 0 to 1 — `readoutLines`' `progress`. */
  progress: number
  /** The work's title, for the top bar; absent until the library answers — `readerTitle.ts`. */
  title?: string
  /** The top bar's second line: `Vol. N · <label>`, or the edition's key — `readerTitle.ts`. */
  editionLine: string
  /** The layouts the work offers, the one on screen, and the top bar's way to change it. */
  layouts: readonly Layout[]
  mode: Layout
  onMode: (mode: Layout) => void
  /** Whether the settings panel is open; ⚙ toggles it. */
  settingsOpen: boolean
  onSettings: () => void
  /** Back to the library grid — spec C1 §6. */
  onLeave: () => void
}

/**
 * The reader's two bars — the top bar (`TopBar.tsx`) and the bottom bar of
 * strip, progress line and readout row — shown and hidden together, never
 * separately (spec 2026-09-17 §8.1). The ring's `Tab` (spec 2026-09-26 §3.1)
 * and a tap in the middle third do both; `Escape` hides them; a correction
 * shows them, and they stay up after it ends. Returns `null`
 * while hidden, rather than e.g. `visibility: hidden`, so the hidden state
 * really is "no chrome at all" — edge-to-edge reading with nothing on screen
 * but the page — and not merely invisible chrome still doing layout work.
 *
 * That unmount is also why only *showing* the chrome fades and hiding it
 * does not: an element that has already unmounted has nothing left on
 * screen to animate out. The unmount is also what keeps a reader who never
 * presses `Tab` from ever fetching a single filmstrip thumbnail — not
 * `Filmstrip`'s `loading="lazy"` on those thumbnails, which does the same
 * job for a strip that's already on screen (see `Filmstrip.tsx`) but bounds
 * a cold strip's burst only partly (O140). So keeping `Chrome` mounted and
 * fading its opacity to zero instead would trade the unmount's whole
 * protection for `loading="lazy"`'s partial one.
 * If this is revisited, what would settle it is a network trace comparing
 * the `Tab`-press burst against a mounted-with-`opacity: 0` prototype.
 *
 * **While correcting, the top bar is not drawn**: `CorrectionBanner` takes
 * the top edge (§8.3). That keeps O65's rule that the way back to the
 * library stands down during a correction — a transient, task-focused state
 * with nothing for "back to the library" to offer — while `L` (`keys.ts`)
 * still leaves: a pending correction has not been written to anything, so
 * leaving loses nothing.
 *
 * The readout row shows whichever of position, alignment health, reading
 * time, chapter and notes `readouts` has switched on, joined by `readoutRow`
 * in that fixed order — see `readoutRow`'s own doc for why.
 */
export function Chrome({
  visible,
  motion,
  readouts,
  position,
  time,
  chapter,
  health,
  notes,
  progress,
  title,
  editionLine,
  layouts,
  mode,
  onMode,
  settingsOpen,
  onSettings,
  onLeave,
  ...filmstrip
}: ChromeProps) {
  const animating = isAnimating(motion)

  // Which tile the pointer is over, while correcting. Held here rather than
  // in `Filmstrip` because the preview is drawn outside the bottom bar — see
  // the preview's JSX below.
  const [preview, setPreview] = useState<{ page: number; half?: 'L' | 'R' }>()

  // Which stretch of the volume the strip shows goes from `Filmstrip` onto
  // the mark's own element, and not through state. The strip tells on every
  // frame of a scroll that moves its rail, and state here would render this
  // component and the whole strip under it on each of them: the render a
  // frame that the strip's scroll listener is throttled and windowed to stay
  // clear of (`Filmstrip.tsx`, above that listener).
  const markRef = useRef<HTMLDivElement>(null)

  // The progress line as a scrollbar for a mouse (O286). A press on the mark
  // takes hold of it where it was pressed, and a press on the line away from
  // it first centres the mark on the pointer; until the release the mark
  // stays under the pointer, and the rail is scrolled to wherever that puts
  // it. No glide follows a release: a scrollbar does not coast. A finger and
  // a pen are left alone: they swipe the strip itself.
  const scrollerRef = useRef<StripScroller | null>(null)
  // `grab` is how far into the mark it was taken, as a share of the mark's
  // width, so it stays inside the mark if the mark's width changes. `x` is
  // the pointer's last `clientX`, for `placeMark` to stand the rail again by.
  const lineHeld = useRef<
    { pointer: number; grab: number; line: HTMLElement; x: number } | undefined
  >(undefined)

  // The line's geometry, measured from its start as the reader sees it: its
  // left edge, or its right where the line is mirrored for a right-to-left
  // work. The boxes are the drawn ones, so a pointer's `clientX` is measured
  // against what the reader sees under it.
  const lineGeometry = (line: HTMLElement, mark: HTMLElement) => {
    const ltr = stepFor(1, filmstrip.reading) === 1
    const box = line.getBoundingClientRect()
    const markBox = mark.getBoundingClientRect()
    return {
      at: (x: number) => (ltr ? x - box.left : box.right - x),
      markStart: ltr ? markBox.left - box.left : box.right - markBox.right,
      markWidth: markBox.width,
      travel: box.width - markBox.width,
    }
  }

  // How far along its travel the mark stands with the point `grab` of the
  // way into it under `x`, held to the line's two ends. Nothing where there
  // is no mark to drag, or no room on the line to drag it in.
  const alongAt = (line: HTMLElement, x: number, grab: number) => {
    const mark = markRef.current
    if (!mark || mark.hidden) return undefined
    const { at, markWidth, travel } = lineGeometry(line, mark)
    if (travel <= 0) return undefined
    return Math.min(Math.max((at(x) - grab * markWidth) / travel, 0), 1)
  }

  // Stands the mark with the point `grab` of the way into it under `x`, by
  // scrolling the rail to where that puts it.
  const scrollLineTo = (line: HTMLElement, x: number, grab: number) => {
    const along = alongAt(line, x, grab)
    if (along !== undefined) scrollerRef.current?.scrollAlong(along)
  }

  const placeMark = (share: RailShare) => {
    // The mark is rendered beside the strip and in the same commit, so it is
    // there whenever the strip has a rail to tell about.
    const mark = markRef.current as HTMLDivElement
    // Nothing to mark where the strip shows the whole volume, and no place
    // along the line to work out for it.
    const whole = share.end - share.start >= 1
    mark.hidden = whole
    if (whole) return
    const width = markWidth(share)
    mark.style.width = width
    // While the line is held, a share that changed under a still pointer
    // means the rail's length did (thumbnails loaded, the window rebuilt
    // after a jump). The mark is written where the pointer puts it and the
    // rail stood again there: written from the share, it would be painted
    // off the pointer for a frame, and a hold past the end would stop short
    // of it. Settles at once: the rail's scroll then asks for the place it
    // already has, which fires nothing.
    const held = lineHeld.current
    const asked = held && alongAt(held.line, held.x, held.grab)
    mark.style.left = `calc(${asked ?? alongTheVolume(share)} * (100% - ${width}))`
    if (asked !== undefined) scrollerRef.current?.scrollAlong(asked)
  }

  const onLineDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType !== 'mouse' || event.button !== 0) return
    // No text selection begins and focus stays where it was, as the rail's
    // own drag leaves it: Space and the arrows go on doing what they did.
    // Before the mark is asked about, so a press of a line with nothing to
    // drag leaves them too.
    event.preventDefault()
    const mark = markRef.current
    // Nothing to drag where the strip shows the whole volume.
    if (!mark || mark.hidden || !scrollerRef.current) return
    const line = event.currentTarget
    // Keeps the line drawn thick for the whole hold, wherever the pointer
    // goes (`styles/reader.css`). On the element and not through state, so
    // the hold renders nothing.
    line.dataset.held = ''
    const { at, markStart, markWidth } = lineGeometry(line, mark)
    const x = at(event.clientX)
    const onMark = x >= markStart && x <= markStart + markWidth
    const grab = onMark && markWidth > 0 ? (x - markStart) / markWidth : 0.5
    lineHeld.current = {
      pointer: event.pointerId,
      grab,
      line,
      x: event.clientX,
    }
    line.setPointerCapture(event.pointerId)
    // Stops a glide too, whether or not the rail has anywhere to go.
    scrollLineTo(line, event.clientX, grab)
  }

  const onLineMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const held = lineHeld.current
    if (!held || held.pointer !== event.pointerId) return
    // A move with the button up ends the hold, as the rail's drag does: a
    // release heard nowhere would leave the rail following a free mouse.
    if ((event.buttons & 1) === 0) {
      onLineEnd(event)
      return
    }
    held.x = event.clientX
    scrollLineTo(held.line, held.x, held.grab)
  }

  const onLineEnd = (event: ReactPointerEvent<HTMLDivElement>) => {
    const held = lineHeld.current
    if (!held || held.pointer !== event.pointerId) return
    lineHeld.current = undefined
    letGo(held)
  }

  // The line wakes at a mouse's moves while the bars are up: see
  // `wakeOnMouse`. Wherever `styles/reader.css` draws the press band, by the
  // same condition, so the band never takes a press on a line that cannot
  // wake. The bars going away, or the reader, take its listeners with them,
  // and end a hold as a release would: left on, it would keep the next line
  // awake until a press on it, or a move over it with no button down, ended
  // it. It would drag nothing: the line it names is gone, its box all zeros.
  const lineRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const line = lineRef.current
    if (!visible || !line) return
    const sleep = window.matchMedia?.('(any-pointer: fine)').matches
      ? wakeOnMouse(line, () => lineHeld.current !== undefined)
      : undefined
    return () => {
      sleep?.()
      const held = lineHeld.current
      lineHeld.current = undefined
      if (held) letGo(held)
    }
  }, [visible])

  // Every primary `mousedown` on the line, held or not, a pen's and a
  // finger's compatibility one too: its default would select text and move
  // focus, which a press on the line leaves as they were. An engine may
  // send one even after `onLineDown` has prevented the `pointerdown`.
  const onLineMouseDown = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.button === 0) event.preventDefault()
  }

  // Whether the fade has settled on fully shown. Starts at `!animating` so a
  // reader with animation off, or the duration zeroed, never sees even a
  // single-frame flash at `opacity: 0` — `isAnimating` treats both of those
  // as no animation at all, and this honours that rather than reintroducing
  // a flash of its own.
  const [shown, setShown] = useState(!animating)

  // `Chrome` is always mounted from `App.tsx`'s side — only this function's
  // own `return null` below changes, not whether the `<Chrome>` element
  // itself is torn down — so `shown` survives a hide/show cycle unless reset
  // here. `scheduleFade` (`modules/motion.ts`) is the branch: hidden, or not
  // animating, it settles `shown` immediately; visible and animating, it
  // schedules the frame that flips `shown` to `true` a moment after the
  // render below first paints at `opacity: 0`, which is what gives the
  // already-declared `transition` something to animate. Its returned cleanup
  // is what makes a rapid re-toggle safe. It lives in `motion.ts` so it can be
  // tested directly against a stubbed `requestAnimationFrame` — see
  // `motion.test.ts`.
  useEffect(
    () => scheduleFade(visible, animating, setShown),
    [visible, animating],
  )

  if (!visible) return null

  const fade: CSSProperties = fadeStyle(motion, shown)
  const row = readoutRow({
    position: readouts.position ? position : undefined,
    time: readouts.time ? time : undefined,
    chapter: readouts.chapter ? chapter : undefined,
    health: readouts.health ? health : undefined,
    notes: readouts.notes ? notes : undefined,
  })
  return (
    <>
      {/*
        The hover preview, a sibling of the bars rather than inside the
        bottom one, so the bar's own box does not clip it. `pointerEvents:
        'none'`, so moving toward it never takes the hover off the tile that
        summoned it. Only while correcting — `onPreview` is only wired then
        (see `Filmstrip`), but the condition is written out anyway.
      */}
      {filmstrip.correcting && preview && (
        <div style={{ ...previewStyle, ...fade }}>
          <img
            className="reader-preview-img"
            src={pageUrl(filmstrip.work, filmstrip.edition, preview.page)}
            alt=""
            style={previewImgStyle}
          />
          <span className="reader-preview-label">
            {preview.half
              ? `page ${preview.page + 1} ${preview.half}`
              : `page ${preview.page + 1}`}
          </span>
        </div>
      )}
      {!filmstrip.correcting && (
        <TopBar
          title={title}
          editionLine={editionLine}
          layouts={layouts}
          mode={mode}
          onMode={onMode}
          settingsOpen={settingsOpen}
          onSettings={onSettings}
          onLeave={onLeave}
          style={fade}
        />
      )}
      <div
        className="reader-bottom-bar"
        data-ring="bottom"
        style={{ ...bottomBarStyle, ...fade }}
      >
        <div style={stripStyle}>
          {/* `visible` is always true here — `Chrome` returns `null` above
              otherwise — but passed through explicitly anyway, since
              `Filmstrip`'s own scroll-into-view effect is keyed on it; see
              `Filmstrip.tsx`'s doc for why. A let-go drag of the strip
              glides on only where the reader's animation setting is on, as
              this component's own fade plays only then. */}
          <Filmstrip
            {...filmstrip}
            visible={visible}
            onPreview={setPreview}
            onShown={placeMark}
            glides={animating}
            scroller={scrollerRef}
          />
        </div>
        <div
          ref={lineRef}
          className="reader-progress"
          // The line runs the way the strip above it runs, so the mark sits
          // under the tiles it stands for: mirrored whole for a work read
          // right to left, the fill with it.
          style={{
            ...progressStyle,
            transform:
              stepFor(1, filmstrip.reading) === 1 ? undefined : 'scaleX(-1)',
          }}
          aria-hidden="true"
          // With the reader's animation off, the line grows and shrinks
          // without easing (`styles/reader.css`).
          data-still={animating ? undefined : ''}
          onPointerDown={onLineDown}
          onPointerMove={onLineMove}
          onPointerUp={onLineEnd}
          onPointerCancel={onLineEnd}
          onLostPointerCapture={onLineEnd}
          onMouseDown={onLineMouseDown}
        >
          <div
            className="reader-progress-fill"
            // Its height is `styles/reader.css`'s, which thickens it with
            // the line while the line is awake or held.
            style={{ width: `${progress * 100}%` }}
          />
          {/* Always here while the bars are, with no width until the strip
              first tells, which it does on mounting. `placeMark` writes the
              rest of it. */}
          <div
            ref={markRef}
            className="reader-progress-shown"
            data-testid="strip-shown"
            style={markStyle}
          />
          <div className="reader-progress-grip" style={gripStyle} />
        </div>
        {/* Not a live region (spec 2026-09-26 §3.5): `App.tsx`'s own region
            speaks the position once, and this row would also announce every
            change to the reading-time readout. */}
        <div className="reader-readout" style={readoutStyle}>
          <span style={readoutTextStyle}>{row}</span>
        </div>
      </div>
    </>
  )
}

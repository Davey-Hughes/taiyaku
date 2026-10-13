import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { pageUrl } from './api'
import { Filmstrip, type FilmstripProps } from './Filmstrip'
import {
  fadeStyle,
  isAnimating,
  scheduleFade,
  type MotionPrefs,
} from './modules/motion'
import { readoutRow, type ReadoutsPrefs } from './modules/readouts'
import { stepFor } from './modules/direction'
import type { Mode } from './pairing'
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

// The bottom bar (spec 2026-09-17 §8.4): one full-width band docked to the
// reader's bottom edge — the strip, the progress line and the readout row,
// padded by the safe areas. It replaced a strip floating 16 px in from each
// side over a readout band of its own.
//
// `position: absolute` against `App.tsx`'s root, which carries `position:
// relative` for exactly this, rather than a flex child of the page-area
// column: an absolutely positioned box is removed from that flow entirely, so
// mounting or unmounting it cannot change the page area's size — the one
// property this component exists to guarantee. `zIndex: 3` stands it over the
// page's own layers (`Annotations`' overlay is at 2); the docked panels share
// the figure and dock clear of it (`--panel-bottom`, `styles/reader.css`).
const bottomBarStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  bottom: 0,
  zIndex: 3,
  paddingLeft: 'var(--safe-left)',
  paddingRight: 'var(--safe-right)',
  paddingBottom: 'var(--safe-bottom)',
}

// The strip: its tiles at the breakpoint's size plus the rail's padding and
// the tiles' borders. `--strip-tile` in `styles/reader.css` has the two sizes
// and why each — 160 px, the thumbnail's own height, from 900 px up, and 96 on
// a phone — and a height that depends on the viewport is a custom property
// because an inline style cannot hold a media query.
const stripStyle: CSSProperties = { height: 'var(--strip-height)' }

// How far through the edition, as a line across the bar: `readoutLines`'
// `progress`, the fraction the readout's percentage prints — the two agree
// except where the slot count has gone backwards, which `progress` clamps at 1
// and `positionLine` does not (see `ReadoutLines.progress`).
const progressStyle: CSSProperties = {
  height: 'var(--progress-height)',
  margin: '0 var(--space-3)',
  // So the mark is placed against the line and not against the bar: the
  // percentages in its `left` and its width are of the line's own width.
  position: 'relative',
}

// The mark on the progress line (O216): where along the volume the strip
// stands. The rail draws no scrollbar, and without this nothing says where
// the strip is once it is scrolled away from the page being read. Proud of
// the line above and below, so it reads as a mark on the line and not as
// part of the fill it may lie over.
//
// Not as wide as the share of the volume the strip shows: on a wide screen
// that share is a long bar, and the question the mark answers is where, not
// how much. `SHOWN_MARK_SHARE` of that share, so the mark still grows as a
// volume gets shorter, and never narrower than `SHOWN_MARK_FLOOR` pixels, a
// width that can be found at a glance.
//
// `markStyle` is all of the mark's style that React is given. Its width, its
// `left` and whether it is hidden are written onto the element by `placeMark`
// in the component, and React leaves alone what it was never handed, so a
// render of `Chrome` does not move the mark.
const SHOWN_MARK_SHARE = 1 / 8
const SHOWN_MARK_FLOOR = 32
const markStyle: CSSProperties = {
  position: 'absolute',
  top: -1,
  bottom: -1,
}

/** The mark's width, as CSS: a share of the line, or the floor where that is wider. */
function markWidth(share: RailShare): string {
  const percent = (share.end - share.start) * SHOWN_MARK_SHARE * 100
  return `max(${SHOWN_MARK_FLOOR}px, ${percent}%)`
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
// the page they are comparing it against. Before style pass ③ this band also
// held the `Library` button (O65), which is why that button stood down while
// correcting; it is in the top bar now.
//
// `zIndex: 3`, matching the two bars (this file's `bottomBarStyle`, above):
// the bars were raised to 3 to clear `Annotations`' overlay at 2, but this
// preview — drawn over the page while correcting, the one surface here that
// most needs to stand clear of everything else on it — was left at `auto`
// until the whole-branch review found note tints, tabs and the side list
// drawing over the page it shows.
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

// The readout row, the bar's last line.
//
// `height` alone is not enough to keep the row from reflowing: a row that
// wraps to two lines grows past a fixed `height` rather than being
// constrained by it. `whiteSpace: 'nowrap'` forbids the wrap outright, and
// `overflow: 'hidden'`/`textOverflow: 'ellipsis'` are what happens instead if
// the joined text ever runs wider than the bar. `readoutRow`
// (`modules/readouts.ts`) is the other half of this: a fixed segment order, so
// which readouts are enabled or have something to say right now changes the
// row's *length*, never the order pieces appear in.
const readoutStyle: CSSProperties = {
  height: 'var(--readout-height)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 4,
  padding: '0 12px',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
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
  /** The layout on screen, and the top bar's way to change it. */
  mode: Mode
  onMode: (mode: Mode) => void
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
 * job for a strip that's already on screen (see `Filmstrip.tsx`) but was
 * measured directly, on this project, to not defer a thumbnail's request at
 * all in the environment tested (docs/status.md, "the tiles'
 * `loading=\"lazy\"` does not defer thumbnail requests"). So keeping
 * `Chrome` mounted and fading its opacity to zero instead would trade the
 * unmount's real protection for `loading="lazy"`'s measured-absent one.
 * If this is revisited, what would settle it is a network trace comparing
 * today's `Tab`-press burst against a mounted-with-`opacity: 0` prototype.
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
    mark.style.left = `calc(${alongTheVolume(share)} * (100% - ${width}))`
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
  // is what makes a rapid re-toggle safe. Extracted to `motion.ts` so it can
  // be tested directly against a stubbed `requestAnimationFrame` — see
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
          />
        </div>
        <div
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
        >
          <div
            className="reader-progress-fill"
            style={{ height: '100%', width: `${progress * 100}%` }}
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
        </div>
        {/* Not a live region since spec 2026-09-26 §3.5: `App.tsx`'s own
            region speaks the position once, and this row would also have
            announced every change to the reading-time readout. */}
        <div className="reader-readout" style={readoutStyle}>
          {row}
        </div>
      </div>
    </>
  )
}

import { useEffect, useState, type CSSProperties } from 'react'
import { pageUrl } from './api'
import { Filmstrip, type FilmstripProps } from './Filmstrip'
import {
  fadeStyle,
  isAnimating,
  scheduleFade,
  type MotionPrefs,
} from './modules/motion'
import { readoutRow, type ReadoutsPrefs } from './modules/readouts'

const readoutHeight = 32
const gapAboveReadout = 8
const inset = 16

/// How tall the strip stands, reading or correcting.
///
/// Not a guess, and not the 76px this used to be: `cover.rs`'s
/// `THUMB_LONG_EDGE` generates tiles at 160px on the long edge, and a manga
/// page is portrait, so 160px *is* a tile's natural height. At 76 the strip
/// rendered them at 45px — 28% of the resolution already on the wire, paid
/// for and thrown away. This adds the rail's own 6px padding and the tile's
/// 2px border, top and bottom, to show that image at exactly 1:1.
///
/// It was briefly taller only while correcting, on the argument that reading
/// wants a scrubber and correcting wants to compare artwork. A manual pass
/// asked for the larger size in both, which is the better call: recognising
/// a page you have already read is easier at 160px too, and one height means
/// the strip does not resize under a reader entering correction mode.
///
/// Going beyond this means raising `THUMB_LONG_EDGE`, which invalidates every
/// cached tile by construction (the constant is folded into the cache path)
/// and multiplies the 1.12 MiB per edition O11 records. `previewMaxHeight`
/// below is the cheaper answer for the one case that needs more.
const stripHeight = 160 + 6 * 2 + 2 * 2

/// How large the hover preview may stand, as a fraction of the viewport.
///
/// Sized in `vh` rather than pixels because this is the one surface where a
/// reader is comparing two drawings rather than recognising one, so it should
/// take whatever the screen can give it. It draws the *page* (`pageUrl`), not
/// the tile — at 160px a thumbnail cannot answer "is this the same drawing",
/// which is the whole question a correction asks.
///
/// One request per hovered tile, and only while correcting. That is not the
/// cold-strip burst O9 and O11 discuss: a reader hovers a handful of
/// candidates, one at a time, at human speed.
const previewMaxHeight = '55vh'

// `position: absolute` against the nearest positioned ancestor — `App.tsx`'s
// root, which carries `position: relative` for exactly this — rather than a
// flex child of the toolbar/page-area column those sit in. An absolutely
// positioned box is removed from that flow entirely, so mounting or
// unmounting it cannot change the page area's size: the one property this
// component exists to guarantee. `overflow: hidden` here (not on the rail)
// is what clips the tiles to the rounded corners without also clipping the
// box-shadow, which paints from the border box outward and is unaffected by
// an element's own `overflow`.
const stripStyle: CSSProperties = {
  position: 'absolute',
  left: inset,
  right: inset,
  bottom: readoutHeight + gapAboveReadout,
  height: stripHeight,
  borderRadius: 12,
  overflow: 'hidden',
  background: 'rgba(24, 24, 24, 0.92)',
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
}

// The hover preview: a page-sized look at the tile under the pointer, while
// correcting. Centred horizontally and standing on top of the strip, so a
// reader's eye travels the shortest distance between the candidate and the
// page they are comparing it against.
const previewStyle: CSSProperties = {
  position: 'absolute',
  left: inset,
  right: inset,
  bottom: readoutHeight + gapAboveReadout + stripHeight + 8,
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
  borderRadius: 8,
  background: 'rgba(24, 24, 24, 0.92)',
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.55)',
}

const previewLabelStyle: CSSProperties = {
  padding: '2px 8px',
  borderRadius: 10,
  background: 'rgba(24, 24, 24, 0.92)',
  color: 'rgba(255, 255, 255, 0.9)',
  fontSize: '0.8em',
}

// The readout row docks to the bottom edge instead of floating — the strip
// above it is the one that is lifted off.
//
// `height: readoutHeight` alone is not enough to keep the row from
// reflowing: a row that wraps to two lines grows past a fixed `height`
// rather than being constrained by it. `whiteSpace: 'nowrap'` forbids the
// wrap outright, and `overflow: 'hidden'`/`textOverflow: 'ellipsis'` are
// what happens instead if the joined text ever runs wider than the strip —
// truncated with an ellipsis rather than pushed onto a second line that
// would grow the box. `readoutRow` (`modules/readouts.ts`) is the other half
// of this: a fixed segment order, so which readouts are enabled or have
// something to say right now changes the row's *length*, never the order
// pieces appear in.
const readoutStyle: CSSProperties = {
  position: 'absolute',
  left: 0,
  right: 0,
  bottom: 0,
  height: readoutHeight,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  gap: 4,
  padding: '0 12px',
  fontSize: '0.85em',
  color: 'rgba(255, 255, 255, 0.85)',
  background: 'rgba(0, 0, 0, 0.6)',
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
}

// The reader's own way back to the library, docked opposite `CreditsPanel`,
// `Help` and `NotePanel`'s shared shell — all three dock at `top: 16, right:
// 16, zIndex: 3` (see e.g. `NotePanel.tsx`'s `panelStyle`), which used to sit
// directly under this button when any of them was open, and their explicit
// `zIndex: 3` always beat this button's implicit `auto`. `left: inset`
// rather than a higher `zIndex` on this side: stacking above an open panel
// would only trade "hidden behind the panel" for "drawn on top of the
// panel's own content", which is not reachable either, just differently
// broken. `left` is a corner none of the three touches.
//
// It is not, though, a corner `CorrectionBanner` (`CorrectionBanner.tsx`)
// leaves alone: that banner spans `left: 16` to `right: 16` at `top: 16`,
// the full width of the viewport at this same top edge, so `top: inset,
// left: inset` sits inside it, not beside it. No `zIndex` here can fix that
// the way it fixes the three panels — this button and the banner both claim
// real width at the same edge, so any stacking order just decides which one
// gets obscured. What actually clears it is below, in `Chrome` itself: the
// button only renders while `filmstrip.correcting` is unset. A correction is
// a transient, task-focused state with nothing for a "back to the library"
// control to offer — the reader is choosing a counterpart page, not
// browsing — and every other corner is already spoken for, so hiding the
// button is the one move that does not cost another surface its own corner.
// The `L` key stays live throughout regardless — see `Chrome`'s own comment
// on the button below.
//
// Styled to match, rather than left as a bare native `<button>`: this floats
// directly over artwork with nothing behind it, unlike the panels' own
// unstyled `Close`/`Export` buttons, which sit inside a dark box that
// already does the work. Same recipe `stripStyle` above and `panelStyle`
// (`NotePanel.tsx`, `CreditsPanel.tsx`, `Help.tsx`) use for that box —
// `rgba(24, 24, 24, …)` and `rgba(255, 255, 255, …)`, not the library
// screen's `Canvas`/`ButtonFace` system colours (`index.css`'s `.chips
// button`), which follow OS light/dark and would put a light-mode button on
// a reader deliberately dark regardless of theme.
const leaveButtonStyle: CSSProperties = {
  position: 'absolute',
  top: inset,
  left: inset,
  // At the panels' own `zIndex: 3`, so a future change that ever shares
  // their corner again loses no stacking fight the way the implicit `auto`
  // used to. Does nothing for `CorrectionBanner` — see the comment above.
  zIndex: 3,
  border: 'none',
  borderRadius: 8,
  padding: '6px 14px',
  font: 'inherit',
  fontSize: '0.85em',
  cursor: 'pointer',
  background: 'rgba(24, 24, 24, 0.92)',
  color: 'rgba(255, 255, 255, 0.92)',
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
}

export interface ChromeProps extends FilmstripProps {
  /** Whether the strip and the readout row are shown at all. */
  visible: boolean
  /** The `motion` module's current prefs — drives the fade this component plays on showing. */
  motion: MotionPrefs
  /** Which of the four readout lines are switched on — see `modules/readouts.ts`. */
  readouts: ReadoutsPrefs
  /**
   * The four readout lines' text, already computed by `App.tsx` from
   * `modules/readouts.ts`'s pure functions — `Chrome` only lays them out, it
   * does not derive any of them itself. `position` and `chapter` are always
   * strings (`positionLine`/`chapterReadout` never answer "nothing to say");
   * `time` is a string too, once a session exists, but `health` is commonly
   * `undefined` — a sound pair with a counterpart says nothing, by design.
   *
   * Each is display-ready already: a percentage, a 1-based slot, a chapter
   * label — none of it is a value that could leak into storage, a URL or the
   * server the way a raw zero-based slot could, so there is no display-vs-
   * internal distinction left to guard here the way `Chrome`'s old
   * `displaySlot` prop had to.
   */
  position: string
  time?: string
  chapter: string
  health?: string
  /** The note count — derived by `modules/annotations.ts`, absent where the work has none. */
  notes?: string
  /** Back to the library grid — spec C1 §6. */
  onLeave: () => void
}

/**
 * The strip and the readout row — brought in and taken away together by
 * `Tab` (`keys.ts`'s `{ kind: 'chrome' }`), never separately. Returns `null`
 * while hidden, rather than e.g. `visibility: hidden`, so the hidden state
 * really is "no chrome at all" — the design's edge-to-edge reading with no
 * persistent chrome — and not merely invisible chrome still doing layout
 * work.
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
 * unmount's real protection for `loading="lazy"`'s measured-absent one: a
 * `Filmstrip` merely faded to invisible while still laid out on screen would
 * fire the full thumbnail burst on every session, whether or not the reader
 * ever presses `Tab` — a real, ongoing bandwidth cost with, now, no viewport
 * safety net behind it. That is a stronger reason to keep the asymmetry
 * (fade in, snap out) than existed before the measurement, not a weaker one.
 * If this is revisited, what would settle it is a network trace comparing
 * today's `Tab`-press burst against a mounted-with-`opacity: 0` prototype —
 * not taken here, and out of scope for this comment.
 *
 * The readout row shows whichever of position, alignment health, reading
 * time and chapter `readouts` has switched on, joined by `readoutRow` in
 * that fixed order — health sits right after position, ahead of time and
 * chapter, so it is not the first segment a narrow, clipped row drops; see
 * `readoutRow`'s own doc for why, and the row's own style comment below for
 * why order (not presence) is what has to stay fixed at all.
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
  onLeave,
  ...filmstrip
}: ChromeProps) {
  const animating = isAnimating(motion)

  // Which tile the pointer is over, while correcting. Held here rather than
  // in `Filmstrip` because the preview has to render outside the strip's own
  // `overflow: hidden` — see the preview's JSX below.
  const [preview, setPreview] = useState<{ page: number; half?: 'L' | 'R' }>()

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
  // is what makes a rapid re-toggle safe: React runs it before this effect
  // fires again, cancelling a still-pending frame rather than letting it
  // land later against whichever state is current by then. Extracted to
  // `motion.ts` rather than inlined so it can be tested directly against a
  // stubbed `requestAnimationFrame`, with no DOM involved — see
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
        The hover preview, rendered as a *sibling* of the strip rather than
        inside it: `stripStyle` carries `overflow: hidden` to clip tiles to
        its rounded corners, which would clip this too. Sits above the strip
        and is `pointerEvents: 'none'`, so moving toward it never takes the
        hover off the tile that summoned it.

        Only while correcting — `onPreview` is only wired then (see
        `Filmstrip`), so `preview` cannot be set otherwise, but the condition
        is written out anyway rather than relying on that.
      */}
      {filmstrip.correcting && preview && (
        <div style={{ ...previewStyle, ...fade }}>
          <img
            src={pageUrl(filmstrip.work, filmstrip.edition, preview.page)}
            alt=""
            style={previewImgStyle}
          />
          <span style={previewLabelStyle}>
            {preview.half
              ? `page ${preview.page + 1} ${preview.half}`
              : `page ${preview.page + 1}`}
          </span>
        </div>
      )}
      <div style={{ ...stripStyle, ...fade }}>
        {/* `visible` is always true here — `Chrome` returns `null` above
            otherwise — but passed through explicitly anyway, since
            `Filmstrip`'s own scroll-into-view effect is keyed on it; see
            `Filmstrip.tsx`'s doc for why. */}
        <Filmstrip {...filmstrip} visible={visible} onPreview={setPreview} />
      </div>
      <div style={{ ...readoutStyle, ...fade }} role="status">
        {row}
      </div>
      {/* Hidden while correcting rather than restacked against
          `CorrectionBanner` — see `leaveButtonStyle`'s own comment above for
          why no `zIndex` can clear a banner that claims this same edge.
          `onLeave` is still one keystroke away regardless: `L` (`keys.ts`,
          listed on the generated help screen) keeps working during a
          correction. That is deliberate, not an oversight matching this
          button — the key is the documented path, and a reader who presses
          it has explicitly asked to leave. A pending correction has not
          been written to anything yet, so leaving loses nothing; only this
          button, which is a convenience rather than the only way out, needs
          to get out of the banner's way. */}
      {!filmstrip.correcting && (
        <button
          type="button"
          style={{ ...leaveButtonStyle, ...fade }}
          onClick={onLeave}
        >
          Library
        </button>
      )}
    </>
  )
}

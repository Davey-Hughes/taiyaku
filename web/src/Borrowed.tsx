import { type ReactNode, useState } from 'react'
import { GapOffers } from './GapOffers'
import type { GapMove } from './Gap'
import type { Direction } from './modules/direction'
import type { FitPrefs } from './modules/fit'
import { PageView } from './PageView'

export interface BorrowedProps {
  /**
   * Whose page is drawn, and its page number in that edition. `page` is
   * 1-based for display — `App.tsx` converts at the point it builds this
   * prop, the same boundary convention `Gap`'s `page` props follow.
   *
   * `half` mirrors `Position`'s own field and is set for exactly the same
   * reason `App.tsx`'s `altAt` reads it: a joined spread is **two slots
   * sharing one `page`**, so the number alone names the whole spread and a
   * lender slot that is one side of one would be announced as both. It is the
   * `alt` that makes this matter rather than the mark — a sighted reader sees
   * which half is drawn, and a screen reader is told "page 47" for a page that
   * is half of 47. `undefined` for every ordinary slot.
   */
  lender: { edition: string; page: number; half?: 'L' | 'R' }
  src: string
  fit: FitPrefs
  /**
   * The `Gap` view for this slot, revealed when the mark is pressed.
   *
   * An opaque node rather than `Gap`'s own seven props re-declared here: it
   * keeps this component's surface small, keeps its tests free of `Gap`, and
   * leaves `App.tsx` the one place that knows how to build one.
   *
   * Opaque also means this component cannot ask it to dismiss itself, which is
   * why the return control below sits *beside* it rather than being one of
   * `Gap`'s props. `Gap` is a view of a state a reader arrives in, and giving
   * it a dismiss handler would make it a dialog everywhere it is used —
   * including the one place it really is the whole state, with `borrow` off.
   */
  offers: ReactNode
  /** The edition being viewed — the one with no page here. */
  viewing: string
  reading: Direction
  before?: GapMove
  after?: GapMove
  onTake: (slot: number) => void
}

/**
 * The mark's own look, shared by the two buttons that wear it.
 *
 * They are one control seen from two sides — press to see what is missing,
 * press to go back — so a reader should not have to work out that a differently
 * styled button in a different place is the same affordance. Sharing the object
 * is also what stops one of them being restyled alone. Its colours are
 * `.borrowed-mark`'s, in `styles/reader.css`: `--warning` with Ink's ground on
 * it (spec 2026-09-17 §8.5).
 *
 * **Docked below the top bar's line, whether or not the chrome is shown** —
 * the same rule spec §8.1 gives the docked panels: always below the top
 * bar's line, and above the bottom bar's, whether or not the chrome is shown.
 * `top: 0` used to sit the mark directly under the reader's top edge, which
 * the top bar now draws over: measured in Chromium at 390×844 with the chrome
 * shown, the mark occupied 0–26 px, the bar 0–52 px, and `elementFromPoint` at
 * the mark's centre returned the bar, not the mark. The mark is the pointer's
 * *only* way into the offers, and back out (see this file's own doc), so a
 * bar covering it made both unreachable while the bars were up. `left:
 * 'var(--safe-left)'` for the same reason `top` moves: `0` sits under a
 * phone's notch in landscape now that `index.html` carries
 * `viewport-fit=cover`.
 */
const MARK_STYLE = {
  position: 'absolute',
  top: 'calc(var(--safe-top) + var(--reader-top-bar))',
  left: 'var(--safe-left)',
  zIndex: 1,
  border: 'none',
  padding: '4px 10px',
  font: 'inherit',
  fontSize: 'var(--text-meta)',
  cursor: 'pointer',
} as const

/**
 * A page borrowed from another edition, where the viewed edition has none.
 *
 * Drawn solo, in the page area, in every mode. That is not a simplification:
 * `App.tsx` tests this arm before it consults `mode`, and scroll and two-page
 * mode both draw `grouped`, built from the viewed edition's own slots — every
 * one of which exists. There is no column with a hole for this to sit in.
 *
 * **The mark is the control, in both directions.** The one thing a reader
 * might want instead is a real page in the edition they chose, which is exactly
 * what `Gap`'s before/after offers are — so reaching them from the thing that
 * says the page is borrowed puts them where the question arises. A keybinding
 * would be a fifth thing to learn for a screen most readers meet rarely, and
 * `borrow` claims no keys. The revealed side carries its own control back, for
 * the reason stated where it is rendered: looking is not choosing, and nothing
 * else in this state can undo a look.
 *
 * `useState` rather than a prop lifted into `App.tsx`, and nothing here clears
 * it *on a move* (the return control clears it on a press), because on every
 * move a reader can make today this component is *unmounted* rather than
 * re-rendered. Every move that does work — an arrow (`session.ts`'s `step`,
 * since `borrow-lender-and-arrows`'s "An arrow on a gap takes the offered
 * move, on both screens" — before that it was inert here), one of
 * `GapOffers`' own buttons, or a filmstrip tile — takes the offered move
 * through `takeOffered`, which goes through `navigate` and makes the viewed
 * edition the authority, so it always lands on a real page. The toggle goes
 * with the unmount every time.
 *
 * The `key` in `App.tsx` covers the one case that does not: a swap straight
 * from one borrowed position to another, when two editions both lack the slot
 * the reader is standing on. Then this component stays mounted and only its
 * props change, and without the key a mark revealed on the first edition's
 * borrowed page would still be revealed on the second's. `gotoubun-v01` has no
 * such slot, so that path is unexercised rather than dead.
 */
export function Borrowed({
  lender,
  src,
  fit,
  offers,
  viewing,
  reading,
  before,
  after,
  onTake,
}: BorrowedProps) {
  const [showingOffers, setShowingOffers] = useState(false)
  // Exactly `App.tsx`'s `altAt`'s format, and it has to stay exactly that: the
  // two name the same thing — which slot of an edition a drawn image is — and
  // a reader who swaps away from a borrowed spread half onto a real one is
  // comparing these two strings.
  const page = lender.half
    ? `page ${lender.page} ${lender.half}`
    : `page ${lender.page}`
  // **The reveal goes both ways, and the return is a control rather than a
  // key.** A reader who presses the mark to see what they are missing and then
  // wants the borrowed page back had nowhere to go at the time this was built:
  // `Gap` takes no dismiss handler, the arrows were inert on this state
  // (`session.ts`'s `step`, before `borrow-lender-and-arrows`'s "An arrow on a
  // gap takes the offered move, on both screens"), and the only route left was
  // swapping to another edition and back. An arrow reaches this screen now
  // too, but it takes the offered move onto a *different* real page rather
  // than dismissing back to the one borrowed here — this control is still the
  // only way back to it. The design calls the mark "the way back"; without
  // this it was the way back in one direction only.
  //
  // Labelled for a reader who has forgotten how they got here — "the borrowed
  // page" names the thing returned to, where naming the lender ("back to raw
  // page 6") would read as a third offer beside `Gap`'s own two, which are
  // labelled with an edition and a page and go somewhere else entirely.
  //
  // **No arrow glyph on it, deliberately.** `Gap` is the sibling on this
  // screen and its `←`/`→` mean page *order* — flipped for an RTL work, which
  // is the O25-adjacent fix its own comment records. A third arrow next to
  // them meaning "back out of this view" would be the same glyph with a second
  // meaning on the one screen a reader reaches by being lost.
  if (showingOffers)
    return (
      <div style={{ position: 'relative', height: '100%' }}>
        <button
          type="button"
          className="borrowed-mark"
          onClick={() => setShowingOffers(false)}
          style={MARK_STYLE}
        >
          back to the borrowed page
        </button>
        {offers}
      </div>
    )
  return (
    <div
      style={{
        position: 'relative',
        height: '100%',
        // The tinted leading edge. Inset rather than a border on the page
        // itself, so it cannot alter the geometry `fit` computes.
        boxShadow: 'inset 4px 0 0 0 var(--warning)',
      }}
    >
      <button
        type="button"
        className="borrowed-mark"
        onClick={() => setShowingOffers(true)}
        style={MARK_STYLE}
      >
        borrowed from {lender.edition} {page}
      </button>
      <PageView
        src={src}
        alt={`${page}, borrowed from ${lender.edition}`}
        // This component is remounted per position (see the `key` in
        // `App.tsx`), so no mounted instance ever sees its `src` change —
        // which is the only state buffering exists for.
        buffered={false}
        fit={fit}
      />
      {/* The mark alone read as a caption, which the previous design named as
          this feature's real cost. This says the situation in words and puts
          both escapes beside it, the same two the arrows now take.

          **Directly under the mark, top-left, not a full-width bottom bar.**
          A bar pinned across the foot of the page area was tried first and
          measured against real art (`gotoubun-v01`, `en-mono`, hub slot 6, at
          1400×620): at that height it covered roughly the bottom 13% of a
          colour spread half, sitting over the largest text on the page, with
          its white text competing against whatever artwork was behind it. A
          block sized to its own content and stacked below the mark instead
          covers only the one corner every borrowed page already spends on the
          mark, leaving the rest of the artwork alone at any viewport height.

          The `+ 32px` clears MARK_STYLE's own rendered height rather than
          reserving space for it through layout: that button is
          `position: absolute` (so the two marks in this file share one
          object, per MARK_STYLE's own doc) and an absolutely positioned
          sibling never contributes to another element's flow, so nothing
          here can push this block down automatically. 32 is MARK_STYLE's
          text (`--text-meta`, 0.75rem, over the 1.5 line-height it inherits from `index.css`'s
          root) plus its 4px top and bottom padding, rounded up a few px for a
          visible gap — a duplicated number, not a shared one, so a change to
          MARK_STYLE's size needs a matching change here. The rest of the
          expression — `calc(var(--safe-top) + var(--reader-top-bar) + …)` —
          is MARK_STYLE's own `top`, repeated so the caption clears the same
          bar the mark does rather than assuming the mark sits at the
          reader's top edge. No explicit `width`
          for the same reason: unset, an absolutely positioned block shrinks
          to its content instead of stretching to fill its containing block,
          which is what keeps this corner small instead of redrawing the bar
          it replaced one row higher. */}
      <div
        className="borrowed-caption"
        style={{
          position: 'absolute',
          top: 'calc(var(--safe-top) + var(--reader-top-bar) + 32px)',
          left: 'var(--safe-left)',
          zIndex: 1,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'flex-start',
          gap: 8,
          padding: '6px 10px',
        }}
      >
        <p style={{ margin: 0 }}>No {viewing} page here.</p>
        <GapOffers
          reading={reading}
          edition={viewing}
          before={before}
          after={after}
          onTake={onTake}
        />
      </div>
    </div>
  )
}

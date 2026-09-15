import { useEffect, useRef, useState } from 'react'
import { noCohort, type Cohort } from './cohort'
import { createOrdering, type Ordering } from './ordering'
import {
  presentWhenDecoded,
  unboundedDecodeQueue,
  type DecodeQueue,
  type ImageFactory,
} from './decode'
import {
  fitModule,
  pageStyle,
  type ArtworkHug,
  type FitPrefs,
} from './modules/fit'

/**
 * One page, drawn through `fit.ts`'s `pageStyle` — what the CSS values are
 * and why now lives on that function's own doc comment, not here.
 *
 * **Two different mechanisms decide how large a page is drawn, and only one of
 * them is here.** *Corrected on 2026-08-08, when L1 shipped zoom.* This
 * paragraph read "upscaling past 100% is a `fit` module setting in M5b and is
 * off until then", which was already wrong about M5b — it came and went with
 * no `fit` module, deferred by name in
 * `docs/superpowers/plans/2026-08-03-m5b-modules.md` — and zoom has since made
 * it incomplete as well. What each mechanism does:
 *
 * - **This component governs how one page sits in its own box** — the cell
 *   `App.tsx` draws it into, which in `two` mode is half the group. *Corrected
 *   on 2026-08-08, when Task 2 of
 *   `docs/superpowers/plans/2026-08-08-fit-seam-and-help.md` wired this
 *   component to `pageStyle`.* This bullet used to say the box's ceiling was
 *   fixed — "`max-width`/`max-height` at 100% is the ceiling and this
 *   milestone did not raise it" — and that `upscaleToFit` was "still an
 *   unbuilt `fit` module setting". Both are false now: `pageStyle`, in
 *   `fit.ts`, is what draws the image below, and its `upscaleToFit` is the
 *   setting that raises the ceiling.
 * - **`Viewport` scales the group around it.** L1 puts a single CSS transform
 *   on a wrapper above `renderGroup`'s output, so a zoomed page is painted
 *   past its natural size with no declaration here changing — a transform
 *   scales this box along with everything else in the group, and changes
 *   nothing about layout size. That is the route by which a page now exceeds
 *   100%, and `Viewport.tsx`'s own doc is why it sits above `PageView` rather
 *   than resizing the images underneath.
 *
 * Scroll mode has neither: `App.tsx` wraps only the single and two-page branch
 * in `Viewport`, so `pageStyle`'s rule is still the whole story there.
 *
 * The letterbox around the page is background, and since M5c it is where the
 * annotation margin tabs live. **In two-page mode that letterbox is now
 * one-sided.** *Added 2026-08-08, with `hug`.* This paragraph used to say only
 * the first sentence, which was true of a page centred in its box and stopped
 * being true the moment a pair's two halves started hugging the seam: there is
 * no margin left on a page's seam side, so the left page's tabs — drawn at
 * `left: 100%` in `Annotations.tsx` — now land on the facing page's artwork.
 * `docs/superpowers/specs/2026-08-08-fit-seam-and-help-design.md` specifies the
 * fix (the tab moves to the margin *away* from the seam, single and scroll mode
 * unchanged) and it is not built here; this comment is the pointer, not the
 * repair.
 */

/** `() => new Image()` — a detached, off-DOM `HTMLImageElement`. Kept out of `decode.ts` so that module stays free of every DOM reference; this is the one place a real one is ever constructed. */
const newImage: ImageFactory = () => new Image()

export interface PageViewProps {
  src: string
  alt: string
  /**
   * Whether an incoming `src` decodes off-screen and is swapped into view
   * only once ready, or is shown the plain way — set straight onto the one
   * `<img>`, exactly as this component worked before this module existed.
   *
   * `App.tsx` passes `false` in scroll mode. That is not an oversight: scroll
   * mode mounts every group in the edition at once, each `PageView`'s `src`
   * fixed for the component's whole lifetime (it is `loading="lazy"`,
   * below, that bounds *that* burst, not this flag, which only ever matters
   * when a *mounted* component's `src` prop changes under it — single and
   * two-page mode's whole reason for existing, where the same `PageView`
   * instance is reused across a page turn rather than remounted). Decoding
   * every one of scroll mode's images off-screen through the shared
   * `decodeQueue` the moment each mounts would force them all to fetch
   * immediately regardless of `loading="lazy"`'s own (measured, see
   * `docs/status.md`) effect — reintroducing the very burst that attribute
   * was added to bound, just by a different route. See `App.tsx`'s own
   * comment where this is threaded through for the full reasoning.
   */
  buffered: boolean
  /** Caps concurrent decodes across every mounted `PageView` that shares one — `App.tsx` builds a swap queue sized from `prefetchModule`'s `maxConcurrent` and hands it to every instance, so a fast flick's several swap decodes (up to one per mounted `PageView`) draw from one pool rather than each capping itself independently. Deliberately a *different* queue instance from the one `App.tsx`'s prefetch window uses — see its `swapQueue`/`prefetchQueue` doc — so a page turn's own decode can never queue behind prefetch jobs for slots the reader has already moved past. Ignored when `buffered` is false. Defaults to an uncapped queue so this component works standalone. */
  decodeQueue?: DecodeQueue
  /** Builds the off-screen probe image a swap decodes into before presenting — a real, detached `<img>` by default (`newImage`, above); a future test of this component would hand in a fake, the same seam `decode.test.ts` already exercises directly against `presentWhenDecoded`. */
  makeImage?: ImageFactory
  /** Presents this page in the same frame as every other `PageView` whose `src` changed in the same commit — `two` mode's pair, in practice. `App.tsx` builds one instance and hands it to every cell, so the two halves of a spread stop appearing one after the other; see `cohort.ts` for the measurement and for why membership is decided by tick rather than by count. Ignored when `buffered` is false. Defaults to `noCohort`, which is what this component did before the barrier existed. */
  cohort?: Cohort
  /**
   * Called with the source this cell is actually drawing, whenever that
   * changes — including the first render, and including the unbuffered path
   * where it is simply `src`.
   *
   * Exists so `App.tsx` can derive the *geometry* of a two-page group from what
   * is on screen rather than from where the reader has moved to. Those two
   * diverge under a fast flick — state moves per keypress, a cell's picture
   * waits for its decode — and the visible result was the cell widths
   * oscillating between one wide page and two half-width ones three times in
   * 114 ms while the artwork sat still. Reported as flickering.
   */
  onShown?: (src: string) => void
  /**
   * Page geometry. Defaults to `fitModule.defaults` so this component still
   * works standalone — the same reason `decodeQueue` defaults to an uncapped
   * queue — and so a caller that has not been threaded yet draws what the
   * settings panel says rather than a third behaviour of its own.
   */
  fit?: FitPrefs
  /**
   * Which edge of this box the page's artwork hugs — `pageStyle`'s own
   * parameter, threaded through unchanged. Defaults to `'centre'`, so single
   * mode, scroll mode and any standalone use draw exactly what they drew
   * before it existed; only the two cells of a real two-page pair ask for
   * anything else, and only `App.tsx` knows which cell it is drawing. See
   * `ArtworkHug` for why the left cell passes `right-edge`.
   */
  hug?: ArtworkHug
}

/**
 * One page. `buffered` false shows `src` the plain way, unchanged from
 * before this module existed. `buffered` true never shows a source until
 * `decode.ts`'s `presentWhenDecoded` says it is ready to paint, so a page
 * turn — this component's `src` prop changing under an instance React keeps
 * mounted across it, rather than a fresh `<img>` remounting — never lands on
 * a blank or half-painted frame.
 *
 * Only one `<img>` is ever in the returned tree at a time; the second
 * "buffer" the mechanism is named for is `makeImage`'s off-screen probe, alive
 * only for the span of one decode and never inserted into the document. A
 * pair of permanently-mounted, role-swapping `<img>` nodes was the first
 * design tried here and rejected: under a fast flick, a second `src` prop
 * change can arrive before the first swap's decode has settled, and if both
 * targeted the *same* reused buffer node, the second would overwrite the
 * first's `src` mid-decode on a DOM element that might by then already be the
 * visible one — a race that could flash a stale frame onto screen after the
 * reader had already moved past it, which is precisely the bug this
 * component exists to close. A fresh probe per attempt has no such shared
 * mutable node to race over. Which settled decode may then touch the one real
 * `<img>`'s `src` is decided by `ordering.ts`, not by `presentWhenDecoded`'s
 * `stale()` check: since #35 that function presents a superseded decode too,
 * so a flick shows pages on the way, and this component hands it `inOrder`,
 * which draws an attempt only if nothing newer has been drawn (O113).
 */
export function PageView({
  src,
  alt,
  buffered,
  decodeQueue,
  makeImage,
  cohort,
  onShown,
  fit,
  hug,
}: PageViewProps) {
  const [shown, setShown] = useState(src)
  // Read inside `presentWhenDecoded`'s `stale` callback at decode-settle
  // time, not at call time — see that function's own doc for why a captured
  // value would be the wrong check.
  const latestRef = useRef(src)
  // Which of this instance's requests may be drawn — `ordering.ts` holds the
  // rule and its tests. One per instance, created once: two cells must not
  // share a counter, or one cell's progress would drop the other's frames.
  const orderRef = useRef<Ordering>(undefined)
  if (orderRef.current === undefined) orderRef.current = createOrdering()
  const order = orderRef.current

  // `shown` deliberately does not join this array: it changes as a *result*
  // of this effect (via `setShown` below, once a decode settles), and
  // depending on it would tear the in-flight decode's own effect down and
  // re-run this one the moment that happens — pointless, since by then `src ===
  // shown` and the effect would only record that page as drawn a second time.
  // `src` and `buffered` are the only true inputs to "should a new decode
  // start".
  // biome-ignore lint/correctness/useExhaustiveDependencies: shown changes as a result of this effect; adding it would tear the in-flight decode down and restart it the moment it settles. See above.
  useEffect(() => {
    latestRef.current = src
    // The number this `src` occupies among this instance's own `src` changes —
    // the only thing that can order two finished decodes. See `ordering.ts` for
    // why `decode.ts` cannot do it and what goes wrong without it.
    const attempt = order.next()
    // **A `src` that starts no decode takes a number too, and is drawn under
    // it.** Two cases start none: a return to the page already drawn, and the
    // unbuffered path. Both used to return without a number, which left
    // `ordering.ts` nothing newer to drop a decode still in flight against. A
    // reader who turned and turned back before the page they left had decoded
    // got that page drawn over the one they came back to, and kept it until
    // `src` next changed; `=` then `0` inside the zoomed decode left the zoomed
    // rung drawn at fit-whole, and `-` then `=` left the lower rung drawn
    // zoomed. `decode.ts` shows a superseded decode on the promise that it "can
    // never paint over a newer one", and a return is the newest request there
    // is. Measured and tested under O113 in `docs/notes.md`.
    //
    // Unbuffered, this is also what keeps `shown` from going stale —
    // `displayed` below ignores it in this branch (it reads `src` directly),
    // but a caller is free to flip `buffered` back on for an instance React
    // keeps mounted, and that transition must not resume from whatever `shown`
    // last held before it went unbuffered. That includes a buffered decode
    // that settled in the meantime, which the number is what drops.
    if (!buffered || src === shown) {
      order.inOrder(attempt, src, setShown)
      return
    }
    // Joining the cohort happens here, inside the effect body, which is what
    // puts both cells of a pair in the same one: React flushes a commit's
    // effects in one synchronous pass, so the sibling's `run` call lands before
    // `cohort.ts`'s deferred close can fire. Returning above — on `src ===
    // shown`, or unbuffered — deliberately joins nothing: a cell whose page did
    // not change has nothing to present and must not make the cell beside it
    // wait on a decode that will never start. It does take a number first, and
    // a number orders only what this one cell draws.
    void presentWhenDecoded(
      makeImage ?? newImage,
      decodeQueue ?? unboundedDecodeQueue,
      src,
      () => latestRef.current !== src,
      // Monotonic: a decode that finished after a newer one was already drawn
      // is dropped. Everything else is shown, superseded or not, which is what
      // makes a fast flick flip through pages rather than hold one spread and
      // jump.
      (ready) => order.inOrder(attempt, ready, setShown),
      cohort ?? noCohort,
    )
  }, [src, buffered])

  const displayed = buffered ? shown : src
  // Reported after render rather than during it, so a parent setting state in
  // response cannot re-enter this component's render. `displayed` is what the
  // `<img>` below is given, so this is exactly what is on screen.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `onShown` is a fresh closure on every parent render; depending on it would fire this effect every render rather than only when the drawn source changes.
  useEffect(() => {
    onShown?.(displayed)
  }, [displayed])
  // Cheap and derived from a prop plus a constant, so computed on every
  // render rather than memoised — the same call `App.tsx` makes for every
  // `sliceFor` read, per that file's own doc on why.
  // `hug` is passed through rather than defaulted here: `pageStyle`'s own
  // default is `'centre'`, and undefined reaches it as "not given", so there
  // is one place the centred case is written down.
  const style = pageStyle(fit ?? fitModule.defaults, hug)

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        overflow: 'hidden',
      }}
    >
      {/*
        `loading="lazy"` only when unbuffered (scroll mode) — see
        `PageViewProps.buffered`'s doc for why the two must not both apply to
        one mounted image. When `buffered` is true, `src` here is `shown`,
        which `presentWhenDecoded` only ever sets to an already-decoded,
        cache-warm value, so there is nothing left for the browser's own
        loading heuristic to defer.
      */}
      <img
        src={displayed}
        alt={alt}
        loading={buffered ? undefined : 'lazy'}
        style={style}
        draggable={false}
      />
    </div>
  )
}

import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { presentWhenDecoded, unboundedDecodeQueue, type DecodeQueue, type ImageFactory } from './decode'

/**
 * One page, fit whole.
 *
 * `width`/`height: auto` on a replaced element sizes the box to its own
 * intrinsic aspect ratio, clamped by `max-width`/`max-height` at 100% — box
 * size and content size coincide, so there is nothing left for `object-fit`
 * to reconcile: aspect preserved, downscaled to fit, never cropped, never
 * distorted, never force-filled. The container's `align-items: center` (with
 * this image's default `flex-grow: 0`) is what keeps a page smaller than the
 * viewport at its own size instead of stretched to fill — upscaling past
 * 100% is a `fit` module setting in M5b and is off until then.
 *
 * The letterbox around the page is background, and it is where the annotation
 * margin tabs will live in M5c.
 */
const fitWhole: CSSProperties = {
  maxWidth: '100%',
  maxHeight: '100%',
  width: 'auto',
  height: 'auto',
  objectFit: 'contain',
  display: 'block',
}

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
 * mutable node to race over: whichever attempt is current when its decode
 * settles is the only one `presentWhenDecoded`'s `stale()` check lets touch
 * the one real `<img>`'s `src`.
 */
export function PageView({ src, alt, buffered, decodeQueue, makeImage }: PageViewProps) {
  const [shown, setShown] = useState(src)
  // Read inside `presentWhenDecoded`'s `stale` callback at decode-settle
  // time, not at call time — see that function's own doc for why a captured
  // value would be the wrong check.
  const latestRef = useRef(src)

  // `shown` deliberately does not join this array: it changes as a *result*
  // of this effect (via `setShown` below, once a decode settles), and
  // depending on it would tear the in-flight decode's own effect down and
  // re-run this one the moment that happens — pointless, since the `src ===
  // shown` guard already makes the effect a no-op by then. `src` and
  // `buffered` are the only true inputs to "should a new decode start".
  // oxlint-disable react-hooks/exhaustive-deps -- shown changes as a result of this effect; adding it would tear the in-flight decode down and restart it the moment it settles. See above.
  useEffect(() => {
    latestRef.current = src
    if (!buffered) {
      // Keeps `shown` from going stale while unbuffered — `displayed` below
      // ignores it in this branch (it reads `src` directly), but a caller
      // is free to flip `buffered` back on for an instance React keeps
      // mounted, and that transition must not resume from whatever `shown`
      // last held before it went unbuffered.
      setShown(src)
      return
    }
    if (src === shown) return
    void presentWhenDecoded(
      makeImage ?? newImage,
      decodeQueue ?? unboundedDecodeQueue,
      src,
      () => latestRef.current !== src,
      setShown,
    )
  }, [src, buffered])
  // oxlint-enable react-hooks/exhaustive-deps

  const displayed = buffered ? shown : src

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
        style={fitWhole}
        draggable={false}
      />
    </div>
  )
}

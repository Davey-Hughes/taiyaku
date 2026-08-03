import type { CSSProperties } from 'react'

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

export interface PageViewProps {
  src: string
  alt: string
}

export function PageView({ src, alt }: PageViewProps) {
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
        `loading="lazy"` — scroll mode mounts every group at once (below, in
        `App.tsx`), and without this an `en-mono` volume's 187 pages fetch on
        one click, roughly 46 MB. The
        filmstrip's tiles (`Filmstrip.tsx`) already carry the same attribute
        for the same reason, at a smaller scale. Single/two mode mount at
        most a couple of these, all immediately on screen, so the browser
        should load them straight away regardless — confirmed in a real
        browser rather than assumed, since "lazy" images outside the
        viewport at layout time are exactly the case this attribute changes.
      */}
      <img src={src} alt={alt} loading="lazy" style={fitWhole} draggable={false} />
    </div>
  )
}

import { useState } from 'react'
import { coverSources } from './library'

/**
 * A series cover, or a placeholder carrying its title — §8's third empty state.
 *
 * **A cold cover is the common first-run case, not an edge one.** Every scanned
 * series generates its cover on the first request for it, so the first visit to
 * a fresh library is exactly when one can 404 or time out — and a bare `<img>`
 * degrades to an empty box that says nothing and names nothing.
 *
 * The failure is remembered **by `src` rather than as a flag**, so a series
 * whose cover URL changes is retried rather than left broken for the life of
 * the page: `<li key={s.id}>` keeps this component mounted across a re-sort or
 * a chip, and a boolean set once would survive both.
 *
 * `alt=""` on the image and a visible title in the placeholder is not an
 * inconsistency: the `<a>` already carries `.title` as text, so a cover that
 * loads is decorative and an `alt` would have a screen reader say the name
 * twice. The placeholder's text is the *drawing*, standing in for artwork, and
 * `aria-hidden` keeps that from becoming the same repetition.
 *
 * **Three components draw it**: `SeriesGrid` — the library's series tiles and
 * search's hits — a series page's volume tiles, and the continue strip's
 * cards (`Library.tsx`'s `ContinueCard`). One copy, because the
 * failure-by-`src` rule and the empty `alt` are the kind of detail that
 * survives in one file and drifts in two.
 *
 * **`src` is a cover URL that takes `?w=`**, and the image is offered at
 * three sizes, one per screen density (`library.ts`'s `coverSources`, O133):
 * the browser fetches 256 px on a 1× screen and 768 px on a 3× phone. The
 * failure is still remembered by that URL: the browser fetches one of the
 * three, and if it fails the placeholder stands in for the cover, whichever
 * size it was.
 */
export function Cover({ src, title }: { src: string; title: string }) {
  const [failed, setFailed] = useState<string>()
  if (failed === src)
    return (
      <span className="cover-missing" aria-hidden="true">
        {title}
      </span>
    )
  const sources = coverSources(src)
  return (
    <img
      src={sources.src}
      srcSet={sources.srcSet}
      alt=""
      loading="lazy"
      onError={() => {
        setFailed(src)
      }}
    />
  )
}

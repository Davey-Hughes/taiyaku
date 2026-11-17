import { useState } from 'react'
import { coverSizeOf, useBrowsePrefs } from './browseModule'
import { coverSizes, coverSources } from './library'

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
 * **`src` is a cover address the server sent**, and the image is offered at
 * three sizes (`library.ts`'s `coverSources`, O133), of which the browser
 * fetches the one the cover's drawn width (`coverSizes`) and the screen's
 * density need. The failure is still remembered by that address: if the
 * one fetched fails, the placeholder stands in for the cover, whichever
 * size it was. A `null` one is no cover at all, so the placeholder is drawn
 * at once and no image is asked for.
 */
export function Cover({ src, title }: { src: string | null; title: string }) {
  const [failed, setFailed] = useState<string>()
  const size = coverSizeOf(useBrowsePrefs())
  if (src === null || failed === src)
    return (
      <span className="cover-missing" aria-hidden="true">
        {/* An element of its own, so a list can keep the title out of the
            browser's scroll anchoring (`.continue .cover-missing > span`):
            centred, it moves with the placeholder's height, and only an
            element can be excluded, not its text. */}
        <span>{title}</span>
      </span>
    )
  const sources = coverSources(src)
  return (
    <img
      src={sources.src}
      srcSet={sources.srcSet}
      sizes={coverSizes(size)}
      alt=""
      loading="lazy"
      onError={() => {
        setFailed(src)
      }}
    />
  )
}

/**
 * A path, with a line allowed to break after each `/` and each `-` in it.
 * A path may hold no space, and a browser breaks one after a `-` but not
 * after a `/`, so without these breaks a run between two hyphens longer
 * than what is left of the line is broken inside a directory's name by the
 * `overflow-wrap: anywhere` that keeps the path in its box. That is left to
 * a single name longer than a whole line.
 *
 * **Each break is an empty `.path-break`, not a `<wbr>`.** Chromium names a
 * heading holding `<wbr>`s with a space at each, "/ srv/ manga", which is
 * what a screen reader then says; the break's zero-width space is generated
 * by the stylesheet with no alternative text (`settings.css`), so the name,
 * the text and a copy of it are the path as it is.
 */
import { Fragment } from 'react'

export function PathWithBreaks({ path }: { path: string }) {
  return withBreaks(path, /(?<=[/-])/)
}

/**
 * A rule's name, with a line allowed to break after each `::` in it. A
 * seeded rule's note names a Rust item, `taiyaku_read::volume_number`, and a
 * browser breaks between words but never inside that one, so without these
 * breaks the item's whole path is the narrowest the rule list can be drawn.
 * The breaks are drawn as `PathWithBreaks` draws its own.
 */
export function NameWithBreaks({ name }: { name: string }) {
  return withBreaks(name, /(?<=::)/)
}

function withBreaks(text: string, after: RegExp) {
  const parts = text.split(after)
  let from = 0
  return parts.map((part, i) => {
    // Where the part starts in the text: unique, and the same each draw.
    const key = from
    from += part.length
    return (
      <Fragment key={key}>
        {part}
        {i < parts.length - 1 && <span className="path-break" />}
      </Fragment>
    )
  })
}

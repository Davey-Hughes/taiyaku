/**
 * How a run of a book's words is drawn with its marks, wherever the book's
 * text is drawn: the reader's blocks, the compare columns, the peek, the
 * interlinear line and the footnote popup.
 */
import { Fragment, type ReactNode } from 'react'
import { bookImageUrl, imageLinkLabel, type LinkTo, type Mark } from './book'
import type { Segment } from './bookNotes'
import { noteColour } from './palette'

/**
 * Runs of a block's words, each highlight a `<mark>` in its note's colour
 * around the run's marks, nested in one fixed order, outermost first: link,
 * bold, italic, sup, sub, then ruby, so a run cut out of the same marks nests
 * the same way wherever it is cut. A ruby's reading is drawn on the run that
 * opens its base, and only with `furigana`; it is `data-aside`, so a
 * selection and the page's word count pass over it as they do a peek.
 */
export function Runs({
  runs,
  work,
  edition,
  furigana,
}: {
  runs: Segment[]
  work: string
  edition: string
  furigana: boolean
}): ReactNode {
  return runs.map((run, j) => {
    const has = (kind: Mark['kind']) => run.marks.some((m) => m.kind === kind)
    const ruby = run.rubyStart
      ? run.marks.find((m) => m.kind === 'ruby')
      : undefined
    let drawn: ReactNode = run.text
    if (furigana && ruby?.kind === 'ruby') {
      drawn = (
        <ruby>
          {drawn}
          <rt data-aside>{ruby.reading}</rt>
        </ruby>
      )
    }
    if (has('sub')) drawn = <sub>{drawn}</sub>
    if (has('sup')) drawn = <sup>{drawn}</sup>
    if (has('italic')) drawn = <i>{drawn}</i>
    if (has('bold')) drawn = <b>{drawn}</b>
    const link = run.marks.find((m) => m.kind === 'link')
    if (link?.kind === 'link') {
      drawn = (
        <Linked link={link} edition={edition}>
          {drawn}
        </Linked>
      )
    }
    if (run.note) {
      drawn = (
        <mark
          data-note={run.note.id}
          style={{ background: noteColour(run.note.color).tint }}
        >
          {drawn}
        </mark>
      )
    }
    return (
      <Fragment key={j}>
        {run.images?.map((image, k) => {
          if (image.kind !== 'image') return null
          const drawn = (
            <img
              className="book-inline-image"
              src={bookImageUrl(work, edition, image.image)}
              alt={image.alt}
            />
          )
          return (
            <Fragment key={k}>
              {image.link ? (
                <Linked
                  link={image.link}
                  edition={edition}
                  label={imageLinkLabel(image.link, image.alt)}
                >
                  {drawn}
                </Linked>
              ) : (
                drawn
              )}
            </Fragment>
          )
        })}
        {drawn}
      </Fragment>
    )
  })
}

/**
 * `children`, words or an image, in `link`, called `label` where given.
 * Every link is `data-link`, which the reader's tap takes for a link before
 * it can be the first of a double tap, or open an image in it full size. An
 * in-book link names its block of `edition` (`data-link-edition`), since one
 * drawn in a peek or another column is not of the edition read.
 *
 * A web link leaves the book whatever called it, so a note read off one is
 * not one this book can open.
 */
export function Linked({
  link,
  edition,
  label,
  children,
}: {
  link: LinkTo
  edition: string
  label?: string
  children: ReactNode
}): ReactNode {
  return link.url ? (
    <a
      aria-label={label}
      href={link.url}
      target="_blank"
      rel="noopener noreferrer"
      data-link
      data-link-url={link.url}
    >
      {children}
    </a>
  ) : (
    // biome-ignore lint/a11y/useValidAnchor: its destination is a block of this edition, which no URL names, so the reader's own tap or Enter on it goes there.
    <a
      aria-label={label}
      role="link"
      tabIndex={0}
      data-link
      data-link-block={link.block}
      data-link-edition={edition}
      data-link-note={link.note || undefined}
    >
      {children}
    </a>
  )
}

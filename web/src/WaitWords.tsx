/**
 * The words of a button that waits on a write: `words`, or `wait` once the
 * wait is drawn (`useDrawnWait`). The pair not drawn is laid unseen in the
 * same cell, so that a button classed `keeps-wait-width` keeps the box of
 * the wider and taller of the two, whichever is drawn, and nothing beside
 * it or beneath it moves when its words change.
 *
 * The unseen pair is generated from an `aria-hidden` element's attribute
 * (`settings.css`), so the button's name and its text are the drawn words
 * alone.
 */
export function WaitWords({
  drawn,
  words,
  wait,
}: {
  drawn: boolean
  words: string
  wait: string
}) {
  return (
    <>
      <span>{drawn ? wait : words}</span>
      <span aria-hidden="true" data-unseen={drawn ? words : wait} />
    </>
  )
}

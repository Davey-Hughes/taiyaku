/**
 * Why the name being typed would be refused, said as it is typed, in the
 * server's own sentence (`configAddress.ts`), where a refused save of that
 * box is drawn. The box names it as its description and is marked invalid
 * while it speaks, and the button that sends the name is held.
 *
 * **A polite live region drawn for as long as its box is**, empty while the
 * name is fine, so that a screen reader reads the sentence once as it
 * appears, and again only where its words change, never at each keystroke
 * and never cutting in on the typing. A region drawn only once it holds its
 * sentence is not reliably read out. Empty, it takes no room (`settings.css`).
 */
export function NameClash({
  id,
  sentence,
}: {
  id: string
  sentence: string | undefined
}) {
  return (
    <p role="status" id={id} className="name-clash">
      {sentence}
    </p>
  )
}

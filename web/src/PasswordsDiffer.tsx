import type { RefObject } from 'react'
import { Callout } from './Callout'

/** What the line says. */
const PASSWORDS_DIFFER = 'The two passwords do not match.'

/**
 * The line that says a form's two password boxes differ
 * (`useConfirmPassword`), in a callout drawn straight after Confirm
 * password's row and laid over the rows below it, or over the control
 * given as `over` alone (`Callout`): a polite status drawn for as long as
 * its form is, empty while there is nothing to say, so that a screen
 * reader reads it as it fills, and once. Confirm password names it as its
 * description while it speaks.
 */
export function PasswordsDiffer({
  id,
  said,
  over,
}: {
  id: string
  said: boolean
  over?: RefObject<HTMLElement | null>
}) {
  return (
    <Callout over={over}>
      <p role="status" id={id}>
        {said ? PASSWORDS_DIFFER : null}
      </p>
    </Callout>
  )
}

/**
 * The line's words, unseen, in the row under Confirm password, the row the
 * line lies over, which draws them in the cell of its button
 * (`.keeps-differ-room`, `settings.css`): so that row is as tall as the
 * line needs, said or not, wherever its words take more lines than the
 * button's height holds, and the line lies under Confirm password's row and
 * over no box. A screen reader reads them in the line alone, but a query
 * by text finds them here while the line is silent, so a test reads the
 * line by its role or its id, never by its words.
 */
export function DifferRoom() {
  return (
    <span className="differ-room" aria-hidden="true">
      {PASSWORDS_DIFFER}
    </span>
  )
}

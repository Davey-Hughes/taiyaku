import { type ChangeEvent, useId, useState } from 'react'

/**
 * Whether the line saying the two differ speaks of itself: once the confirm
 * is neither empty nor the start of the password, so that a confirm still
 * being typed is not called wrong at each keystroke.
 */
const saysPasswordsDiffer = (password: string, confirm: string) =>
  confirm !== '' && !password.startsWith(confirm)

/**
 * Confirm password, for each form that takes a new password: `/account` and
 * the reader's account panel, an account's page, and New account (the
 * user's rulings of 2026-10-25). The password is typed twice, and the server
 * is sent it once.
 *
 * The button that sends it is held while the two differ, as a `HeldButton`
 * whose `held` takes `differ`, so that it keeps focus and a press reaches
 * it: a press then is turned down and sends nothing, and is handed to
 * `heldPress` (its `onHeldPress`), which the caller's submit guard calls
 * too, for a submit that reaches the form another way. The caller empties
 * the confirm with `setConfirm('')` where it empties the password.
 *
 * The line saying they differ (`PasswordsDiffer`, drawn from `line`) is a
 * callout laid under Confirm password's row, spoken by
 * `saysPasswordsDiffer` wherever focus is (the user's ruling of
 * 2026-10-25), and after a press turned down while the two differ, though
 * the confirm is empty or the start of the password, until either box is
 * edited (the user's ruling of 2026-10-26). It lies over what is beneath
 * it, the held button that sends the password or New account's Role, and
 * takes a press there, so that one on it presses and opens nothing
 * (`Callout`).
 */
export function useConfirmPassword(password: string) {
  const [confirm, setConfirm] = useState('')
  const differ = password !== confirm
  /** The two as a press turned down while they differed found them. */
  const [pressedOn, setPressedOn] = useState<{
    password: string
    confirm: string
  } | null>(null)
  const pressedNow =
    pressedOn !== null &&
    pressedOn.password === password &&
    pressedOn.confirm === confirm
  // An edit of either box ends what the press said, so that the line then
  // speaks by `saysPasswordsDiffer` alone, and not again if the boxes come
  // back to what the press found.
  if (pressedOn !== null && !pressedNow) setPressedOn(null)
  const said = pressedNow || saysPasswordsDiffer(password, confirm)
  const lineId = useId()
  return {
    confirm,
    setConfirm,
    differ,
    /** A press of the held button, or a submit, while the two differ. */
    heldPress: () => {
      if (differ) setPressedOn({ password, confirm })
    },
    /** Spread on Confirm password, after its `type` and `autoComplete`. */
    confirmBox: {
      value: confirm,
      onChange: (e: ChangeEvent<HTMLInputElement>) => {
        setConfirm(e.target.value)
      },
      'aria-invalid': said ? (true as const) : undefined,
      'aria-describedby': said ? lineId : undefined,
    },
    /** `PasswordsDiffer`'s props. */
    line: { id: lineId, said },
  }
}

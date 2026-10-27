import type { ComponentPropsWithRef } from 'react'

export type HeldButtonProps = ComponentPropsWithRef<'button'> & {
  type: 'button' | 'submit'
  /** Nobody may press it now: it is drawn held and a press does nothing. */
  held: boolean
  /**
   * Called for a press turned down while it is held, so that the caller
   * can say why it is held where nothing says so yet.
   */
  onHeldPress?: () => void
}

/**
 * A button that a wait or a hold can take while it has focus, and that keeps
 * focus then. Held, it is `aria-disabled` and its press is turned down here,
 * not `disabled`: Chromium moves focus off a focused control that becomes
 * disabled, to the page.
 * `aria-disabled` keeps it in the tab order too, so the keyboard can reach
 * it and the hold is read out with it.
 *
 * What holds it until it can do something, such as nothing chosen yet or
 * another button's press, is `disabled`, passed through, which keeps it out
 * of the tab order. Where that can come on while it has focus, the caller
 * moves focus to where it is put right (`useFocusLeavesHeld`), as See what
 * this would do sends it to Move to when the library chosen leaves the list.
 *
 * Its held look is `:disabled`'s, drawn from `[aria-disabled="true"]` by the
 * same rules (`base.css`, `settings.css`), and under forced colours in
 * `GrayText`, which `:disabled` has from the browser.
 *
 * The press is turned down with `preventDefault`, so a held submit button
 * submits nothing when it is pressed; a form that Enter in its box can
 * submit guards its own `onSubmit` as well. What keeps a second press from
 * sending before the hold is drawn (a wait is drawn only once it has lasted)
 * is the caller's.
 */
export function HeldButton({
  type,
  held,
  onHeldPress,
  onClick,
  ...button
}: HeldButtonProps) {
  return (
    <button
      {...button}
      type={type}
      aria-disabled={held}
      onClick={(e) => {
        if (held) {
          e.preventDefault()
          onHeldPress?.()
          return
        }
        onClick?.(e)
      }}
    />
  )
}

/**
 * A refusal drawn by the control whose press it answers, beneath it or in a
 * callout laid over the rows around its row, and brought into view when it
 * is drawn (O209): a control at the foot of the window leaves the sentence
 * under it below the fold, and the page does not move by itself. Each
 * refusal slot is cleared when its press is made again, so a second refusal
 * in the same words is drawn anew and brought into view anew.
 */
import { useEffect, useRef } from 'react'
import { bringIntoView } from './reveal'

export function Refusal({
  children,
  onPress,
}: {
  children: string
  /**
   * A press on it, which it takes all the same: it lies over controls, and
   * a press there reaches none of them. Taken at the click: a touch fires
   * `pointerdown` at touchstart, and a refusal gone by then would let the
   * tap's click, sent after the lift, land on the control beneath.
   */
  onPress?: () => void
}) {
  const el = useRef<HTMLParagraphElement>(null)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `children` is the trigger, not a closure read: the body reaches the sentence through the ref, and a refusal drawn anew in new words must be brought into view anew.
  useEffect(() => {
    bringIntoView(el.current)
  }, [children])
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: it takes no focus, so no key can reach it; the press it takes is a pointer's, which would otherwise reach the control beneath.
    <p role="alert" ref={el} onClick={onPress}>
      {children}
    </p>
  )
}

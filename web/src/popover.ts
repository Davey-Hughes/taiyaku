/**
 * A button and the small panel it opens: the sidebar's More and a page's
 * View panel. The disclosure pattern: opening leaves focus on the button,
 * and the panel follows the button in the document, so Tab from the button
 * enters it.
 *
 * **Escape is caught on the way down, at the document**, and stopped there.
 * The phone drawer closes on an Escape it hears at the document on the way
 * up, so a panel open inside the drawer would otherwise close the drawer with
 * it. One Escape closes the innermost thing open, and focus goes back to the
 * button that opened it.
 *
 * Outside the button and its panel, a pointer press or focus arriving closes
 * it, and so does any navigation, a link inside it included: a panel left
 * open over the next screen would be describing the last one.
 */
import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { useLocation } from 'react-router'

/**
 * `Wrap` is the element holding the button and its panel, which is where
 * "outside" begins: More's is a `nav`, View's a `div`.
 */
export function usePopover<Wrap extends HTMLElement = HTMLDivElement>() {
  const [open, setOpen] = useState(false)
  const wrap = useRef<Wrap>(null)
  const button = useRef<HTMLButtonElement>(null)
  const panelId = useId()
  const location = useLocation()

  // Focus on a link inside goes back to the button: the close hides that
  // link, and from a hidden element the next Tab starts again at the top of
  // the document. Inside the phone drawer, the drawer's own close then sends
  // focus on to ☰ (`Shell.tsx`), its effect running after this one.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `location` is a trigger, not a closure read — the body reads nothing from it.
  useEffect(() => {
    if (wrap.current?.contains(document.activeElement)) button.current?.focus()
    setOpen(false)
  }, [location])

  useEffect(() => {
    if (!open) return
    const inside = (target: EventTarget | null) =>
      target instanceof Node && (wrap.current?.contains(target) ?? false)
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      event.stopPropagation()
      setOpen(false)
      button.current?.focus()
    }
    const onPointer = (event: PointerEvent) => {
      if (!inside(event.target)) setOpen(false)
    }
    const onFocus = (event: FocusEvent) => {
      if (!inside(event.target)) setOpen(false)
    }
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('pointerdown', onPointer)
    document.addEventListener('focusin', onFocus)
    return () => {
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('pointerdown', onPointer)
      document.removeEventListener('focusin', onFocus)
    }
  }, [open])

  const toggle = useCallback(() => {
    setOpen((was) => !was)
  }, [])

  return { open, toggle, wrap, button, panelId }
}

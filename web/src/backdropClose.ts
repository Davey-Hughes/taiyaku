import { type MouseEvent, useRef } from 'react'

/**
 * The handlers that close a modal `<dialog>` drawn over the whole screen on a
 * tap beside what it holds: a `click` there, from a press that began there
 * too. Beside is the dialog itself, or only what a `beside` passed in names,
 * so a caller's test names the dialog too. The tap that opened it began on
 * the text beneath, and a touch screen sends that tap's `click` after its
 * `pointerup` has opened the dialog, hit-tested then, so it lands on the
 * dialog itself and, judged by its target alone, would close it at once.
 */
export function useBackdropClose(
  onClose: () => void,
  beside: (event: MouseEvent) => boolean = (event) =>
    event.target === event.currentTarget,
) {
  const pressedOn = useRef(false)
  return {
    onPointerDown: (event: MouseEvent) => {
      pressedOn.current = beside(event)
    },
    onClick: (event: MouseEvent) => {
      if (pressedOn.current && beside(event)) onClose()
    },
  }
}

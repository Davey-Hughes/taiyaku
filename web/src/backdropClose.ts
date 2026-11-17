import { type SyntheticEvent, useRef } from 'react'

/**
 * The handlers that close a modal `<dialog>` drawn over the whole screen on a
 * tap beside what it holds: a `click` on the dialog itself, from a press that
 * began there too. The tap that opened it began on the text beneath, and a
 * touch screen sends that tap's `click` after its `pointerup` has opened the
 * dialog, hit-tested then, so it lands on the dialog itself and, judged by
 * its target alone, would close it at once.
 */
export function useBackdropClose(onClose: () => void) {
  const pressedOn = useRef(false)
  return {
    onPointerDown: (event: SyntheticEvent) => {
      pressedOn.current = event.target === event.currentTarget
    },
    onClick: (event: SyntheticEvent) => {
      if (pressedOn.current && event.target === event.currentTarget) onClose()
    },
  }
}

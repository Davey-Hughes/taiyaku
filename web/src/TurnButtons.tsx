import type { CSSProperties } from 'react'
import { stepFor, type Direction } from './modules/direction'

// Over one outer third of the page area, drawing nothing. `pointer-events:
// none`, so a finger or a mouse reaches the page area and `gestures.ts` as if
// these were not there. VoiceOver's double-tap is *assumed* to press the
// button element itself (spec 2026-09-26 §3.4); that is not confirmed on a
// device (O162).
const turnStyle: CSSProperties = {
  position: 'absolute',
  top: 0,
  bottom: 0,
  width: 'calc(100% / 3)',
  padding: 0,
  border: 0,
  background: 'none',
  pointerEvents: 'none',
}

export interface TurnButtonsProps {
  /** The viewed edition's direction, override included — `App.tsx`'s `effectiveReading`. */
  reading: Direction
  /** A *physical* step, as the arrow keys give one: `readerIntent` mirrors it through `stepFor`. */
  onStep: (by: 1 | -1) => void
}

/**
 * "Previous page" and "Next page" for a screen reader — spec 2026-09-26
 * §3.4, O162. Without these, the page image is the only thing VoiceOver can
 * reach with the bars hidden, and its double-tap lands in the middle third,
 * which shows the bars; nothing on the page can turn it.
 *
 * Out of the ring (`tabIndex={-1}`), because the keyboard has its arrows.
 * Named by what the step does, placed by where a sighted tap does it: in a
 * right-to-left work the left third is therefore named "Next page": the name
 * follows the meaning, the position follows the page.
 */
export function TurnButtons({ reading, onStep }: TurnButtonsProps) {
  const sides = ([-1, 1] as const).map((by) => ({
    by,
    side: by === -1 ? ('left' as const) : ('right' as const),
    next: stepFor(by, reading) === 1,
  }))
  const ordered = sides[0].next ? sides : [sides[1], sides[0]]
  return (
    <>
      {ordered.map(({ by, side, next }) => (
        <button
          key={side}
          type="button"
          className="reader-turn"
          tabIndex={-1}
          style={{ ...turnStyle, [side]: 0 }}
          onClick={() => onStep(by)}
        >
          <span className="visually-hidden">
            {next ? 'Next page' : 'Previous page'}
          </span>
        </button>
      ))}
    </>
  )
}

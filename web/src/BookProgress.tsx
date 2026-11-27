import { useRef } from 'react'
import { progressStyle } from './bars'

/** How far one press of an arrow moves the reader along the book. */
const STEP = 0.01

/**
 * How far through the book, as the page reader's progress line, and a way
 * along it: pressed and dragged, or its arrows and Page Up and Down while it
 * has focus, which keep the keys from turning the page as well.
 */
export function BookProgress({
  progress,
  ticks,
  label,
  edition,
  onSeek,
}: {
  progress: number
  /** Where each chapter starts, as fractions of the book. */
  ticks: number[]
  /** What the reader is at, said for a screen reader. */
  label: string
  /** The edition read, whose keys' steps alone go on from one another. */
  edition: string
  /** Go `fraction` of the way through the book; `by` is the step, when a key asked. */
  onSeek: (fraction: number, by?: number) => void
}) {
  const line = useRef<HTMLDivElement>(null)
  // Where a forward key last asked to go, in which edition, and how far
  // through the book the reader was when it asked. The reader's place is a
  // block's start, so a step inside a long block leaves it where it was, and
  // the next goes on from what was asked rather than asking for the same
  // block again. A step back always leaves the block, so goes from the place.
  const asked = useRef<{ edition: string; from: number; to: number }>(undefined)
  const seekAt = (x: number) => {
    asked.current = undefined
    const box = line.current?.getBoundingClientRect()
    if (box && box.width > 0) onSeek((x - box.left) / box.width)
  }
  const keys: Record<string, number> = {
    ArrowRight: STEP,
    ArrowUp: STEP,
    PageDown: 10 * STEP,
    ArrowLeft: -STEP,
    ArrowDown: -STEP,
    PageUp: -10 * STEP,
  }
  return (
    <div
      className="book-scrubber"
      role="slider"
      tabIndex={0}
      aria-label="Place in the book"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress * 100)}
      aria-valuetext={label}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture?.(event.pointerId)
        seekAt(event.clientX)
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
          seekAt(event.clientX)
        }
      }}
      // Gone elsewhere, as to the contents or Back, the keys start afresh.
      onBlur={() => {
        asked.current = undefined
      }}
      onKeyDown={(event) => {
        const by = keys[event.key]
        const last = asked.current
        const from =
          by !== undefined &&
          by > 0 &&
          last?.edition === edition &&
          last.from === progress
            ? last.to
            : progress
        const to =
          event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? 1
              : by && Math.min(1, Math.max(0, from + by))
        if (to === undefined) return
        event.preventDefault()
        event.stopPropagation()
        asked.current = { edition, from: progress, to }
        onSeek(to, by)
      }}
    >
      <div ref={line} className="reader-progress" style={progressStyle}>
        <div
          className="reader-progress-fill"
          style={{ height: '100%', width: `${progress * 100}%` }}
        />
        {ticks.map((at, i) => (
          <div
            key={i}
            className="book-progress-tick"
            style={{ left: `${at * 100}%` }}
          />
        ))}
      </div>
    </div>
  )
}

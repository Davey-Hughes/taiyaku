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
  onSeek,
}: {
  progress: number
  /** Where each chapter starts, as fractions of the book. */
  ticks: number[]
  /** What the reader is at, said for a screen reader. */
  label: string
  onSeek: (fraction: number) => void
}) {
  const line = useRef<HTMLDivElement>(null)
  const seekAt = (x: number) => {
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
      onKeyDown={(event) => {
        const by = keys[event.key]
        const to =
          event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? 1
              : by && progress + by
        if (to === undefined) return
        event.preventDefault()
        event.stopPropagation()
        onSeek(to)
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

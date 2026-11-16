import { useEffect, useRef } from 'react'
import type { Entry } from './book'
import { usePanelFocus } from './focus'

interface ContentsProps {
  visible: boolean
  entries: Entry[]
  /** The index of the entry being read, marked and scrolled to on opening. */
  current: number
  onGo: (block: number) => void
  onClose: () => void
}

/** A book's contents: every run of headings, the one being read marked. */
export function Contents({
  visible,
  entries,
  current,
  onGo,
  onClose,
}: ContentsProps) {
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, visible)
  useEffect(() => {
    if (visible) {
      panelRef.current
        ?.querySelector('[aria-current="true"]')
        ?.scrollIntoView({ block: 'center' })
    }
  }, [visible])
  if (!visible) return null
  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label="Contents"
      ref={panelRef}
      tabIndex={-1}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Contents</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {entries.length === 0 ? (
        <p>This edition has no headings to list.</p>
      ) : (
        <ol className="book-contents">
          {entries.map((entry, i) => (
            <li key={entry.block}>
              <button
                type="button"
                aria-current={i === current}
                onClick={() => onGo(entry.block)}
              >
                {entry.lines.join(' · ')}
              </button>
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}

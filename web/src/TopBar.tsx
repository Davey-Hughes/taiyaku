import type { CSSProperties, ReactNode } from 'react'
import type { Mode } from './pairing'

/** The three layouts, in the order the control draws them. */
const MODES: readonly Mode[] = ['single', 'two', 'scroll']

/**
 * Each layout drawn rather than named (spec 2026-09-17 §8.3): at 390 px the
 * three words would leave the title about 70 px. The word stays in the button
 * as visually hidden text, so its accessible name — and the `textContent` the
 * reader's tests find it by — is still `single`, `two` or `scroll`.
 */
const ICONS: Record<Mode, ReactNode> = {
  single: <rect x="7" y="4" width="10" height="16" />,
  two: (
    <>
      <rect x="3" y="5" width="8" height="14" />
      <rect x="13" y="5" width="8" height="14" />
    </>
  ),
  scroll: (
    <>
      <rect x="7" y="2" width="10" height="9" />
      <rect x="7" y="13" width="10" height="9" />
    </>
  ),
}

// Docked to the reader's top edge like the bottom bar to its bottom, and for
// the same reason: absolutely positioned against `App.tsx`'s root, so showing
// it never resizes the page area. The height is the bar's 52 px plus the top
// safe area, which the padding keeps clear. `zIndex: 3` stands it over the
// page's own layers — `Annotations`' overlay is at 2.
const barStyle: CSSProperties = {
  position: 'absolute',
  top: 0,
  left: 0,
  right: 0,
  zIndex: 3,
  boxSizing: 'border-box',
  height: 'calc(var(--reader-top-bar) + var(--safe-top))',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--space-2)',
  padding:
    'var(--safe-top) calc(var(--space-2) + var(--safe-right)) 0 calc(var(--space-3) + var(--safe-left))',
}

export interface TopBarProps {
  /** The work's title, from the library — absent until it answers, or if it never does. */
  title?: string
  /** `Vol. N · <label>` for the edition on screen, or its key alone — `readerTitle.ts`. */
  editionLine: string
  mode: Mode
  onMode: (mode: Mode) => void
  /** Whether the settings panel is open, which ⚙ toggles. */
  settingsOpen: boolean
  onSettings: () => void
  onLeave: () => void
  /** `Chrome`'s fade, applied to the bar as one piece. */
  style?: CSSProperties
}

/**
 * The reader's top bar (spec 2026-09-17 §8.3): the way back to the library,
 * what is being read, the layout, and settings. `Chrome` shows it with the
 * bottom bar and takes both away together, and draws neither while
 * correcting, when `CorrectionBanner` has the top edge.
 *
 * It holds no state: the layout is `App.tsx`'s `layout` preference, the same
 * slice the settings panel's Layout section writes, and ⚙ flips the same flag
 * the `s` key does.
 */
export function TopBar({
  title,
  editionLine,
  mode,
  onMode,
  settingsOpen,
  onSettings,
  onLeave,
  style,
}: TopBarProps) {
  return (
    <div
      className="reader-top-bar"
      data-ring="top"
      style={{ ...barStyle, ...style }}
    >
      <button type="button" className="reader-back" onClick={onLeave}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M15 5l-7 7 7 7" />
        </svg>
        Library
      </button>
      <div className="reader-title">
        {title !== undefined && (
          <span className="reader-title-work">{title}</span>
        )}
        <span className="reader-title-edition">{editionLine}</span>
      </div>
      <div className="reader-layout" role="group" aria-label="Layout">
        {MODES.map((m) => (
          <button
            key={m}
            type="button"
            aria-pressed={m === mode}
            onClick={() => onMode(m)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              {ICONS[m]}
            </svg>
            <span className="visually-hidden">{m}</span>
          </button>
        ))}
      </div>
      <button
        type="button"
        className="reader-icon-button"
        aria-expanded={settingsOpen}
        onClick={onSettings}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1" />
        </svg>
        <span className="visually-hidden">Settings</span>
      </button>
    </div>
  )
}

import { useRef, type CSSProperties } from 'react'
import { usePanelFocus } from './focus'
import { READER_KEYS, withoutCorrecting } from './keys'
import type { Direction } from './modules/direction'
import { helpGroups } from './modules/help'
import type { ReaderModule } from './module'

// The panel is `.docked-panel` in `styles/reader.css`, where `CreditsPanel`
// and the notes panel dock too: one class rather than a copy of one inline
// object in each (spec 2026-09-17 §8.5).

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  gap: 10,
  padding: '4px 0',
  borderTop: '1px solid var(--reader-hair)',
}

// Wide enough for `1–9` and `? / h` without the descriptions beside them
// starting at different places down the column. `nowrap` because a key cell
// that wraps mid-range reads as two keys.
const keyStyle: CSSProperties = {
  flex: '0 0 72px',
  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
  whiteSpace: 'nowrap',
}

const descriptionStyle: CSSProperties = {
  margin: 0,
  flex: 1,
}

export interface HelpProps {
  /**
   * Which way the viewed edition reads. The two arrow rows trade descriptions
   * in a right-to-left work, because the physical key that advances is the
   * other one — `helpGroups` derives that from `stepFor` rather than deciding
   * for itself.
   */
  reading: Direction
  visible: boolean
  /**
   * The modules to list. The caller's choice, not this component's: it lists
   * exactly what it is handed, in the order it is handed them. `App.tsx`
   * hands it `enabledModules(registry.all(), prefsMap)` rather than
   * `registry.all()`, so a module switched off in settings is not advertised;
   * that function's doc has the rule and what it does not cover.
   */
  modules: ReaderModule<unknown>[]
  /**
   * The account is an administrator. For anyone else `c`'s row is left out,
   * as `keyplan.ts` leaves the key to the browser (O58).
   */
  mayCorrect: boolean
  onClose: () => void
}

/**
 * What the keys do, taken from the keys themselves.
 *
 * **Nothing here is written down.** Both groups come out of `helpGroups`,
 * which reads `keys.ts`'s `READER_KEYS` and the modules' own `keys`
 * declarations — the same two tables `intentOf` walks to decide what a
 * keypress meant. A hand-written list would be a second copy of a rule, and
 * it would be wrong the first time a binding changed without anyone thinking
 * of this file; `keys.ts` says the same thing where the table is declared,
 * and that table exists in its current shape for this reason.
 *
 * What the reader sees is therefore only as good as the declarations, and
 * the two places where that shows are answered in the declarations rather
 * than here:
 *
 * - The module rows would say each binding's `intent` string, which is
 *   written to be dispatched on rather than read — `zoom:in` reads well
 *   enough, but `annotations:reveal` and `annotations:review` are one letter
 *   apart and name different things. `KeyBinding` has an optional `label`
 *   for this, which widened the module contract deliberately (`module.ts`
 *   records the decision and what it beat), and `helpGroups` prefers it.
 * - A module switched off in settings would still have its keys listed, since
 *   a `ReaderModule` carries no notion of being currently on — that lives in
 *   `App.tsx`'s preference map. So the caller filters: `modules/help.ts`'s
 *   `enabledModules` is what `App.tsx` passes through, and its doc names the
 *   one module whose off switch it cannot see.
 *
 * No state of its own, `null` when not visible, and a close button:
 * `CreditsPanel`'s shape, for the same reason it has that shape. `Escape`
 * closes this panel and `CreditsPanel`, and Settings (spec 2026-09-26 §3.3).
 */
export function Help({
  visible,
  modules,
  reading,
  mayCorrect,
  onClose,
}: HelpProps) {
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, visible)
  if (!visible) return null
  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label="Keyboard help"
      ref={panelRef}
      tabIndex={-1}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Keys</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {helpGroups(
        modules,
        reading,
        mayCorrect ? READER_KEYS : withoutCorrecting(READER_KEYS),
      ).map((group) => (
        <section key={group.heading}>
          <h3 className="reader-panel-heading">{group.heading}</h3>
          <dl style={{ margin: 0 }}>
            {group.rows.map((row) => (
              <div key={row.keys} style={rowStyle}>
                <dt style={keyStyle}>
                  <kbd>{row.keys}</kbd>
                </dt>
                <dd style={descriptionStyle}>{row.description}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </div>
  )
}

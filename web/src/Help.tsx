import type { CSSProperties } from 'react'
import type { Direction } from './modules/direction'
import { helpGroups } from './modules/help'
import type { ReaderModule } from './module'

// The shell `CreditsPanel` uses, which is the shell `NotePanel` uses: three
// panels that open over the page with different keys should not be three
// different-looking objects. Copied rather than shared, which is the same
// call `CreditsPanel` made and recorded — with the caveat that its reason was
// "a shared style module for two callers is indirection without a second
// reader", and this is the third caller. Sharing them is a change to two
// files this task may not touch, so the copy stands and the count is written
// down here for whoever makes that call.
const panelStyle: CSSProperties = {
  position: 'absolute',
  top: 16,
  right: 16,
  width: 360,
  maxWidth: 'calc(100% - 32px)',
  maxHeight: '70vh',
  overflowY: 'auto',
  zIndex: 3,
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 12,
  borderRadius: 8,
  background: 'rgba(24, 24, 24, 0.94)',
  color: 'rgba(255, 255, 255, 0.92)',
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
}

const groupHeadingStyle: CSSProperties = {
  margin: '8px 0 0',
  fontSize: '0.85em',
  textTransform: 'uppercase',
  letterSpacing: '0.06em',
  opacity: 0.7,
}

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  gap: 10,
  padding: '4px 0',
  borderTop: '1px solid rgba(255, 255, 255, 0.12)',
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
  onClose: () => void
}

/**
 * What the keys do, taken from the keys themselves.
 *
 * **Nothing here is written down.** Both groups come out of `helpGroups`,
 * which reads `keys.ts`'s `READER_KEYS` and the modules' own `keys`
 * declarations — the same two tables `intentOf` walks to decide what a
 * keypress meant. A hand-written list would have been a second copy of a
 * rule, and it would have been wrong the first time a binding changed without
 * anyone thinking of this file; `keys.ts` says the same thing where the table
 * is declared, and that table exists in its current shape for this reason.
 * This project has corrected a copy of a rule that drifted from the rule nine
 * times, and the fix each time was to delete the copy.
 *
 * What the reader sees is therefore only as good as the declarations, and
 * both of the two places that showed have since been answered in the
 * declarations rather than here:
 *
 * - The module rows said each binding's `intent` string, which was written to
 *   be dispatched on rather than read — `zoom:in` reads well enough, but
 *   `annotations:reveal` and `annotations:review` are one letter apart and
 *   name different things. `KeyBinding` gained an optional `label` for this,
 *   which widened the module contract deliberately (`module.ts` records the
 *   decision and what it beat), and `helpGroups` prefers it. The prose was
 *   already written; it was sitting on the intent constants' doc comments
 *   where nothing at runtime could reach it.
 * - A module switched off in settings would still have its keys listed, since
 *   a `ReaderModule` carries no notion of being currently on — that lives in
 *   `App.tsx`'s preference map. So the caller filters: `modules/help.ts`'s
 *   `enabledModules` is what `App.tsx` passes through, and its doc names the
 *   one module whose off switch it cannot see.
 *
 * No state of its own, `null` when not visible, and a close button:
 * `CreditsPanel`'s shape, for the same reason it has that shape. It parts
 * from `CreditsPanel` on `Escape`, which does close this and does not close
 * that — see the `Escape` branch in `App.tsx`'s keydown listener, which
 * carries the reason.
 */
export function Help({ visible, modules, reading, onClose }: HelpProps) {
  if (!visible) return null
  return (
    <div style={panelStyle} role="dialog" aria-label="Keyboard help">
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
        <strong style={{ flex: 1 }}>Keys</strong>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
      {helpGroups(modules, reading).map((group) => (
        <section key={group.heading}>
          <h3 style={groupHeadingStyle}>{group.heading}</h3>
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

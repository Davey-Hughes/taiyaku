import type { CSSProperties } from 'react'
import type { ReaderModule } from './module'
import { fieldsOf } from './settingsPanel'
import { FieldControl } from './settingsFields'

// A field's row, handed to `FieldControl`, which takes its layout as an
// object because the library's own settings pass theirs (`settingsFields.tsx`).
// Colour and type are `.reader-dialog-panel`'s, in `styles/reader.css`, like
// the rest of this panel's look (spec 2026-09-17 §8.5).
const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '6px 0',
}

export interface SettingsProps {
  /** Whether the panel is on screen at all — see `modules/settings.ts`'s `s` key. */
  visible: boolean
  /** Every registered module, in registry order (`module.ts`'s `Registry.all()`) — one section per entry, generated rather than named here. */
  modules: ReaderModule<unknown>[]
  /**
   * Each module's current preference slice, keyed by `ReaderModule.id`. A
   * module with no entry here (nothing loaded yet) renders its `defaults`.
   *
   * Typed `object` rather than `Record<string, unknown>`: every module's own
   * prefs type (`DirectionPrefs`, `MotionPrefs`, …) is a plain interface with
   * no index signature, so it structurally satisfies `object` but not
   * `Record<string, unknown>` — TypeScript requires an explicit index
   * signature for the latter, which no module declares and none should have
   * to just to be handed to this panel. `FieldControl`'s own read narrows
   * with a cast at the one place a key is actually looked up by name.
   */
  prefs: Record<string, object>
  /**
   * One sentence about a module's own situation on this work, keyed by
   * `ReaderModule.id`, shown below that module's fields as secondary text.
   *
   * A caller-computed map rather than something this file derives, the same
   * way `App.tsx`'s `<CreditsPanel>` a few lines below its `<Settings>` call
   * is handed `work`/`edition`/`pages` rather than working them out itself:
   * this panel is generated from every module's `defaults` and must stay
   * ignorant of what any one module's fields *mean*, so a module whose toggle
   * can be a no-op for reasons specific to the work being read (`modules/borrow.ts`'s
   * `borrowNote`, today) hands the finished sentence in rather than this file
   * learning that module's id. A module with no entry here renders exactly as
   * it always has — this is additive, not a replacement for any field.
   */
  notes?: Record<string, string>
  /** One field changed for one module — `raw` is whatever the control's own change event handed back; `settingsPanel.ts`'s `withFieldValue` is what a caller coerces it with. */
  onChange: (id: string, key: string, raw: unknown) => void
  /** Reset one module's whole preference slice back to its defaults. */
  onReset: (id: string) => void
  /**
   * Close the panel. Its own button because ⚙ opens it by pointer (spec
   * 2026-09-17 §8.3), and `s` — the only other way out — is a key.
   */
  onClose: () => void
}

/**
 * The settings panel: one section per registered module, one control per
 * key of that module's `defaults`, both generated from `settingsPanel.ts`'s
 * pure inference rather than named per module here. A seventh module needs
 * no edit to this file — only a `defaults` shape for `fieldsOf` to read.
 *
 * A module whose `defaults` has no fields (`modules/settings.ts`'s own
 * `settingsModule`, today) renders no section at all: `fieldsOf` reports an
 * empty list, and a heading over zero controls would be a blank shelf
 * asking to be filled by something nobody built. That's a deliberate
 * consequence of generation, not a special case written for this one
 * module's id.
 */
export function Settings({
  visible,
  modules,
  prefs,
  notes,
  onChange,
  onReset,
  onClose,
}: SettingsProps) {
  if (!visible) return null
  return (
    <div className="reader-dialog" role="dialog" aria-label="Settings">
      <div className="reader-dialog-panel">
        <div className="reader-panel-head">
          <h2 className="reader-panel-title">Settings</h2>
          <button type="button" className="reader-button" onClick={onClose}>
            Close
          </button>
        </div>
        {modules.map((mod) => {
          const fields = fieldsOf(mod.defaults as object, mod.options)
          if (fields.length === 0) return null
          const modulePrefs = (prefs[mod.id] ?? mod.defaults) as Record<
            string,
            unknown
          >
          const note = notes?.[mod.id]
          return (
            <section key={mod.id} className="reader-section">
              <div className="reader-section-head">
                <h3 className="reader-section-title">{mod.label}</h3>
                <button
                  type="button"
                  className="reader-button"
                  onClick={() => onReset(mod.id)}
                >
                  Reset
                </button>
              </div>
              {fields.map((field) => (
                <FieldControl
                  key={field.key}
                  field={field}
                  value={modulePrefs[field.key]}
                  onChange={(raw) => onChange(mod.id, field.key, raw)}
                  rowStyle={rowStyle}
                />
              ))}
              {note && <p className="reader-meta reader-note">{note}</p>}
            </section>
          )
        })}
      </div>
    </div>
  )
}

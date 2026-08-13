import type { CSSProperties } from 'react'
import type { ReaderModule } from './module'
import {
  fieldsOf,
  humanize,
  nestedCount,
  nestedSummary,
  type Field,
} from './settingsPanel'

// A centred modal rather than a docked panel — unlike `Chrome`'s strip and
// readout row, which must never cover the page underneath, the settings
// panel is opened deliberately (`s`, `modules/settings.ts`) and is meant to
// take the reader's attention while it is up, the same way `Gap` owns the
// screen while a slot has no counterpart.
const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'rgba(0, 0, 0, 0.6)',
  zIndex: 20,
}

const panelStyle: CSSProperties = {
  background: 'rgba(24, 24, 24, 0.96)',
  color: 'rgba(255, 255, 255, 0.92)',
  borderRadius: 12,
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
  padding: '20px 24px',
  width: 'min(480px, 90vw)',
  maxHeight: '80vh',
  overflowY: 'auto',
}

const sectionStyle: CSSProperties = {
  marginTop: 16,
  paddingTop: 16,
  borderTop: '1px solid rgba(255, 255, 255, 0.15)',
}

const sectionHeaderStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
}

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '6px 0',
  fontSize: '0.9em',
}

// `opacity: 0.7` is `FieldControl`'s own muted styling for the `'object'`-kind
// summary below — reused here rather than a second shade invented for the
// same purpose, of a module's own note about itself.
const noteStyle: CSSProperties = {
  margin: '4px 0 0',
  fontSize: '0.9em',
  opacity: 0.7,
}

/** One field's control — inferred purely from `field.kind`, which `settingsPanel.ts`'s `fieldKind` already derived from the preference's own runtime type. Nothing here branches on which module or which key this is. */
function FieldControl({
  field,
  value,
  onChange,
}: {
  field: Field
  value: unknown
  onChange: (raw: unknown) => void
}) {
  const label = humanize(field.key)
  if (field.kind === 'boolean') {
    return (
      <label style={rowStyle}>
        <span>{label}</span>
        <input
          type="checkbox"
          checked={value === true}
          onChange={(e) => onChange(e.target.checked)}
        />
      </label>
    )
  }
  if (field.kind === 'number') {
    return (
      <label style={rowStyle}>
        <span>{label}</span>
        <input
          type="number"
          value={typeof value === 'number' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
          style={{ width: 72 }}
        />
      </label>
    )
  }
  if (field.kind === 'select') {
    return (
      <label style={rowStyle}>
        <span>{label}</span>
        <select
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        >
          {(field.options ?? []).map((option) => (
            <option key={option} value={option}>
              {humanize(option)}
            </option>
          ))}
        </select>
      </label>
    )
  }
  if (field.kind === 'string') {
    return (
      <label style={rowStyle}>
        <span>{label}</span>
        <input
          type="text"
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        />
      </label>
    )
  }
  // `'object'` — a nested per-key record such as `direction`'s `override` or
  // `regroup`'s `breaks`, keyed by edition rather than a single settable
  // value. There is no safe primitive control to infer for it (see
  // `fieldKind`'s own doc), so this shows how many entries are set and
  // leaves clearing them to the section's own Reset button, which already
  // clears the whole slice back to `defaults` — a per-edition editor would
  // need to know what "edition" means, which a generated, module-agnostic
  // panel deliberately does not.
  return (
    <div style={rowStyle}>
      <span>{label}</span>
      <span style={{ opacity: 0.7 }}>{nestedSummary(nestedCount(value))}</span>
    </div>
  )
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
}: SettingsProps) {
  if (!visible) return null
  return (
    <div style={overlayStyle} role="dialog" aria-label="Settings">
      <div style={panelStyle}>
        <h2 style={{ marginTop: 0, marginBottom: 0 }}>Settings</h2>
        {modules.map((mod) => {
          const fields = fieldsOf(mod.defaults as object, mod.options)
          if (fields.length === 0) return null
          const modulePrefs = (prefs[mod.id] ?? mod.defaults) as Record<
            string,
            unknown
          >
          const note = notes?.[mod.id]
          return (
            <section key={mod.id} style={sectionStyle}>
              <div style={sectionHeaderStyle}>
                <h3 style={{ margin: 0 }}>{mod.label}</h3>
                <button type="button" onClick={() => onReset(mod.id)}>
                  Reset
                </button>
              </div>
              {fields.map((field) => (
                <FieldControl
                  key={field.key}
                  field={field}
                  value={modulePrefs[field.key]}
                  onChange={(raw) => onChange(mod.id, field.key, raw)}
                />
              ))}
              {note && <p style={noteStyle}>{note}</p>}
            </section>
          )
        })}
      </div>
    </div>
  )
}

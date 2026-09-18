/**
 * The browse screens' settings, in the sidebar — spec C1 §4.
 *
 * **Generated, not written.** One control per key of `browseModule.defaults`,
 * inferred by `settingsPanel.ts` from each value's runtime type. A seventh
 * preference needs no edit here.
 *
 * **Not `Settings.tsx`.** That panel is the reader's: a centred modal on a
 * near-black slab, correct because it draws over artwork. This is sidebar
 * chrome and follows `color-scheme`, so it shares the control (Task 8's
 * `settingsFields.tsx`) and not the chrome. `Login.tsx` is why that
 * distinction is written down rather than assumed. Its rows are drawn by
 * `shell.css`'s `.browse-settings label`, so it hands `FieldControl` no
 * `rowStyle`.
 */
import { type BrowsePrefs, browseModule } from './browseModule'
import { FieldControl } from './settingsFields'
import { fieldsOf } from './settingsPanel'

export interface BrowseSettingsProps {
  prefs: BrowsePrefs
  /**
   * One preference changed, **uncoerced** — `raw` is whatever the control's
   * own change event handed back, which for a number input is a string.
   * `module.ts`'s `fieldChanged` is what coerces it, through
   * `settingsPanel.ts`'s `withFieldValue`. This is `Settings.tsx`'s contract
   * verbatim; see its own `onChange` doc and the test named "reports a number
   * field change as the raw string, uncoerced".
   */
  onChange: (key: string, raw: unknown) => void
  onReset: () => void
}

export function BrowseSettings({
  prefs,
  onChange,
  onReset,
}: BrowseSettingsProps) {
  return (
    <section className="browse-settings">
      <h2 className="sidebar-heading">{browseModule.label}</h2>
      {fieldsOf(browseModule.defaults, browseModule.options).map((field) => (
        <FieldControl
          key={field.key}
          field={field}
          value={prefs[field.key as keyof BrowsePrefs]}
          onChange={(raw) => {
            onChange(field.key, raw)
          }}
        />
      ))}
      <button type="button" onClick={onReset}>
        Reset
      </button>
    </section>
  )
}

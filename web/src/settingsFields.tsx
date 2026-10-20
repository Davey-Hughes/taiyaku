import type { CSSProperties } from 'react'
import {
  humanize,
  nestedCount,
  nestedSummary,
  type Field,
} from './settingsPanel'

/**
 * One field's control — inferred purely from `field.kind`, which
 * `settingsPanel.ts`'s `fieldKind` already derived from the preference's own
 * runtime type. Nothing here branches on which module or which key this is.
 *
 * **Its own file because two panels render it and they disagree about two
 * things.** The reader's `Settings.tsx` is a centred modal over artwork and
 * is dark on purpose; a browse page's View panel (`ViewMenu.tsx`) is UI
 * chrome and follows `color-scheme`. `rowStyle` is the first disagreement:
 * the reader's panel passes its own, and View passes none and draws its rows
 * from `shell.css`. `sentenceCase` is the second. Everything else — the
 * inference §4 calls "generated rather than hoped" — is shared, which is the
 * only way that claim stays true when one of the two changes.
 */
export function FieldControl({
  field,
  value,
  onChange,
  rowStyle,
  sentenceCase = false,
}: {
  field: Field
  value: unknown
  onChange: (raw: unknown) => void
  rowStyle?: CSSProperties
  /**
   * "Edition selector" and "On-select" rather than "edition selector" and
   * "on-select": the library's View panels write their words as sentences.
   * The reader's panel passes nothing and keeps the lowercase it has.
   */
  sentenceCase?: boolean
}) {
  const word = (key: string) => {
    const text = humanize(key)
    return sentenceCase ? text.charAt(0).toUpperCase() + text.slice(1) : text
  }
  const label = word(field.key)
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
              {word(option)}
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

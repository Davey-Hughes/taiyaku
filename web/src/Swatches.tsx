import { COLOUR_NAMES, noteColour } from './palette'

/**
 * The note colours as swatches rather than a dropdown: the choice *is* the
 * appearance, so a list of names would make a reader translate twice. The
 * current one is ringed rather than ticked, because a tick drawn on a swatch
 * has to be one colour or the other and would vanish into half of them.
 */
export function Swatches({
  chosen,
  disabled = false,
  label = 'Note colour',
  onPick,
}: {
  /** The colour shown as chosen, none for a row that only picks. */
  chosen?: string
  disabled?: boolean
  label?: string
  onPick: (name: string) => void
}) {
  return (
    <div style={{ display: 'flex', gap: 6 }} role="group" aria-label={label}>
      {COLOUR_NAMES.map((name) => (
        <button
          key={name}
          type="button"
          className="reader-swatch"
          disabled={disabled}
          onClick={() => onPick(name)}
          aria-label={name}
          aria-pressed={chosen === undefined ? undefined : chosen === name}
          title={name}
          style={{
            width: 22,
            height: 22,
            padding: 0,
            borderRadius: '50%',
            border: 'none',
            background: noteColour(name).tab,
            cursor: 'pointer',
          }}
        />
      ))}
    </div>
  )
}

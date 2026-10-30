/**
 * One rule's fields — spec C4 §1's right-hand pane, top half.
 *
 * **It commits on blur or Enter, never on keystroke.** A preview is
 * expensive and the derivation is the entire cost, so deriving to answer a
 * question about a pattern nobody has finished writing holds the write lock
 * repeatedly for nothing — and a half-typed regex is a rule that matches
 * nothing, so the answer would be wrong as well as expensive. The build
 * design's §5.3, where the alternatives (a debounce, an explicit Preview
 * button) are weighed and refused.
 *
 * **A blur that changed nothing commits nothing.** Tabbing through the form
 * is one gesture to a person and would otherwise be six derivations.
 *
 * **The fields are local state seeded from the prop, keyed on the rule.** The
 * whole point of committing on blur is that the parent does not hold every
 * keystroke; that means this component does, and a `key` on the instance is
 * what makes selecting a different rule redraw rather than keep the last
 * one's half-typed text.
 *
 * **An emptied text field reads as `null`, not `""`.** Every `set_*` column is
 * nullable and the store reads absent-or-null as "this rule sets nothing
 * here"; an empty string is a value, and it would set the field to the empty
 * string for every entry the rule matches.
 *
 * **No pattern validation.** A regex that compiles in JS is not the regex the
 * store compiles — the build design's §8. What a bad pattern produces is a
 * `complaints` entry from the save.
 */
import { useState } from 'react'
import type { Root } from './config'
import { MATCH_ONS, type Rule, RULE_KINDS } from './rules'

/** `""` from a text input is "nothing set", never the empty string. */
const orNull = (value: string): string | null => (value === '' ? null : value)

export function RuleForm({
  rule,
  roots,
  onCommit,
}: {
  rule: Rule
  roots: Root[]
  onCommit: (rule: Rule) => void
}) {
  const [draft, setDraft] = useState<Rule>(rule)

  const commit = () => {
    // A blur or an Enter that changed nothing is not a preview. Compared
    // field-by-field and by value, against the prop, rather than tracked with
    // a dirty flag or `JSON.stringify` — a dirty flag would fire on a field
    // edited back to what it was, and `JSON.stringify` bakes in `Rule`'s
    // current field order, so a future reorder (or a differently-ordered
    // object reaching this component) would silently compare unequal objects
    // as equal and drop a real edit with no test failure.
    const unchanged = (Object.keys(rule) as (keyof Rule)[]).every(
      (k) => draft[k] === rule[k],
    )
    if (unchanged) return
    onCommit(draft)
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      commit()
    }
  }

  // O175: every control here carries its label's words in an `aria-label`,
  // because `.field` draws them in capitals and Chromium's accessibility tree
  // would name the control in those capitals.
  const text = (
    name:
      | 'pattern'
      | 'set_series'
      | 'set_edition_key'
      | 'set_kind'
      | 'set_language'
      | 'note',
    label: string,
  ) => (
    <label className={name === 'pattern' ? 'field pattern' : 'field'}>
      <span>{label}</span>
      <input
        type="text"
        name={name}
        aria-label={label}
        value={draft[name] ?? ''}
        onChange={(e) =>
          setDraft((held) => ({
            ...held,
            [name]:
              name === 'pattern' ? e.target.value : orNull(e.target.value),
          }))
        }
        onBlur={commit}
        onKeyDown={onKeyDown}
      />
    </label>
  )

  return (
    <div className="rule-form">
      <label className="field">
        <span>kind</span>
        <select
          name="kind"
          aria-label="kind"
          value={draft.kind}
          onChange={(e) => {
            const kind = e.target.value
            setDraft((held) => ({ ...held, kind }))
            onCommit({ ...draft, kind })
          }}
        >
          {RULE_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {kind}
            </option>
          ))}
        </select>
      </label>

      <label className="field">
        <span>match on</span>
        <select
          name="match_on"
          aria-label="match on"
          value={draft.match_on}
          onChange={(e) => {
            const match_on = e.target.value
            setDraft((held) => ({ ...held, match_on }))
            onCommit({ ...draft, match_on })
          }}
        >
          {MATCH_ONS.map((on) => (
            <option key={on} value={on}>
              {on}
            </option>
          ))}
        </select>
      </label>

      {text('pattern', 'pattern')}

      {/* O182: the `aria-label` repeats the legend's words because `admin.css`
          draws them in capitals, and Chromium's accessibility tree would name
          the group in those capitals. */}
      <fieldset className="rule-sets" aria-label="sets">
        <legend>sets</legend>
        {text('set_series', 'series')}
        {text('set_edition_key', 'edition')}
        {text('set_kind', 'kind')}
        {text('set_language', 'language')}
      </fieldset>

      {text('note', 'note')}

      <label className="field">
        <span>scope</span>
        <select
          name="root_id"
          aria-label="scope"
          value={draft.root_id === null ? '' : String(draft.root_id)}
          onChange={(e) => {
            const root_id =
              e.target.value === '' ? null : Number(e.target.value)
            setDraft((held) => ({ ...held, root_id }))
            onCommit({ ...draft, root_id })
          }}
        >
          <option value="">every root</option>
          {roots.map((root) => (
            <option key={root.id} value={String(root.id)}>
              {root.path}
            </option>
          ))}
        </select>
      </label>
    </div>
  )
}

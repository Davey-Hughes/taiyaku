/**
 * The ordered rule list — spec C4 §1's left-hand pane.
 *
 * **Everything here addresses a rule by its position, never by its id.** Two
 * rules the editor has added and not saved both carry `id: 0` — the store's
 * own sentinel — so an id cannot tell them apart, and `preview::Decided` and
 * `preview::Override` hold the same boundary in the store for the same
 * reason. `key` is the index too, which is normally the wrong answer in React
 * and is the right one here: the identity of a row in this list *is* its
 * position, and reordering is the operation the list exists to support.
 *
 * **Order is the layering.** `Engine::compile` orders by `ordinal` then id, so
 * a drag that moved the array without renumbering would preview exactly as it
 * did before it — `rules.ts`'s `renumber` is what the screen calls, and this
 * component only reports which index moved where.
 *
 * **Deleting a rule an accepted proposal wrote reopens that proposal**, so it
 * asks first and names the proposal. C4 §1 requires the mark and the
 * reopening; the mark is joined from `GET /api/library/proposals`'s own
 * `created_rule_id`, which needs no server change (the build design's §5.5).
 * A rule no proposal wrote deletes without a dialog — nothing is written until
 * Save, so an ordinary delete is undoable by not saving.
 *
 * **Every gesture here is frozen while a save is in flight — O96.** Not
 * politeness: `RuleEditor::save` replaces the whole list with `answer.rules`
 * when it lands, so a reorder, a toggle, an add or a delete committed while it
 * is out would be applied, previewed, and then silently thrown away. The freeze
 * is also what keeps the positional identity above *true across a save* —
 * `answer.rules` is aligned to the list that was sent, and a list that moved
 * under the request has no correspondence to it at all. `RuleForm` is
 * deliberately **not** frozen; that half is in `RuleEditor.tsx`'s own comment.
 */
import { useEffect, useState } from 'react'
import type { Proposal } from './curation'
import { HeldButton } from './HeldButton'
import { useDrawnWait } from './hooks'
import { NameWithBreaks } from './PathWithBreaks'
import {
  PREVIEW_EXPENSIVE_ABOVE,
  type Rule,
  ruleMechanism,
  ruleName,
} from './rules'

export function RuleList({
  rules,
  selected,
  proposals,
  overrideCount,
  frozen,
  onSelect,
  onToggle,
  onReorder,
  onAdd,
  onDelete,
}: {
  rules: Rule[]
  selected: number | undefined
  proposals: Proposal[]
  overrideCount: number
  /**
   * A write is in flight — a save or an import. Every control here is frozen
   * for its duration, because both writers answer with a list that replaces
   * this one, so a gesture made inside the window is applied, previewed and
   * then overwritten with nothing on screen saying so.
   *
   * Frozen by `aria-disabled` and not `disabled`, so that a control with
   * focus keeps it (`HeldButton`): each handler turns its gesture down from
   * the send, and the buttons are drawn held once the write has lasted.
   *
   * **A write, never `busy`:** a preview in flight is superseded by
   * `runPreview`'s sequence guard and is never lost, so a gesture during one is
   * correct and cheap. Freezing on `busy` would grey the list for as long as a
   * preview takes after every gesture, which is the rhythm the build design's
   * §5.3 costed out and kept.
   */
  frozen: boolean
  onSelect: (index: number) => void
  onToggle: (index: number) => void
  onReorder: (from: number, to: number) => void
  onAdd: () => void
  onDelete: (index: number) => void
}) {
  /** The write, once it has lasted: the buttons' held look. */
  const frozenDrawn = useDrawnWait(frozen)
  /** Which index is awaiting a confirmed delete, and the proposal it would reopen. */
  const [confirming, setConfirming] = useState<
    { index: number; proposal: number } | undefined
  >()

  /**
   * Close the dialog on any list mutation, rather than let it keep holding a
   * stale index.
   *
   * Addressing a rule by its position is right for this list — the doc
   * comment above says why an id cannot do the job — but *holding* a
   * position open across a mutation is not: deleting a different, lower-
   * indexed rule while this dialog is open shifts the array under
   * `confirming.index`, and "Delete and reopen" would then delete whatever
   * rule slid into that slot and reopen a proposal nobody was ever warned
   * about. Closing on every change is simpler than re-deriving where the
   * confirmed rule moved to (or whether it still exists at all — the
   * confirmed rule itself can be the one a different delete just removed),
   * and a person who still wants to delete it sees the same warning again on
   * a second click, against the list as it now stands.
   */
  // biome-ignore lint/correctness/useExhaustiveDependencies: `rules` is a trigger, not a closure read — the body reads nothing from it, only needs to run again each time the array a parent passes is a new one.
  useEffect(() => {
    setConfirming(undefined)
  }, [rules])

  /** The accepted proposal that wrote each rule, by rule id. Never by index. */
  const wroteRule = new Map(
    proposals
      .filter((p) => p.created_rule_id !== null)
      .map((p) => [p.created_rule_id as number, p.id]),
  )

  const askOrDelete = (index: number) => {
    const rule = rules[index]
    const proposal = rule.id === 0 ? undefined : wroteRule.get(rule.id)
    if (proposal === undefined) {
      onDelete(index)
      return
    }
    setConfirming({ index, proposal })
  }

  return (
    <div className="rule-list">
      <ol>
        {rules.map((rule, index) => (
          // The index *is* the key here — see this file's doc comment. No
          // suppression comment: `noArrayIndexKey` is deliberately off in
          // `web/biome.jsonc`, where "off" is the decision rather than an open
          // question, and `App.tsx` already keys a list this way.
          <li
            key={index}
            className="rule-row"
            draggable={!frozen}
            onDragStart={(e) => {
              e.dataTransfer.setData('text/plain', String(index))
              e.dataTransfer.effectAllowed = 'move'
            }}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault()
              // O96, and **both guards are needed**: `draggable={!frozen}`
              // stops a drag *starting* during a write, and this stops the
              // drop of one that was already in progress when the write
              // began — flipping the attribute under a live drag cancels
              // nothing. A reorder committed here is overwritten by
              // `answer.rules` when the write lands, with nothing on screen
              // saying so, which is the whole of O96.
              if (frozen) return
              // **The empty payload is rejected before `Number` sees it, and
              // that is the whole of this line.** `DataTransfer.getData`
              // answers `''` for a key nothing ever set — a file dragged in
              // from the desktop, or any drag that began outside this list —
              // and `Number('')` is `0`, not `NaN`. So a `Number.isNaN` guard
              // alone reads a foreign drop as "row 0 was dragged" and
              // silently reorders the list under someone who never dragged a
              // rule at all. The `from === index` check below only hides it
              // when the drop happens to land on row 0.
              const raw = e.dataTransfer.getData('text/plain')
              if (raw === '') return
              const from = Number(raw)
              if (Number.isNaN(from) || from === index) return
              onReorder(from, index)
            }}
          >
            <span className="ordinal">{index + 1}</span>
            {/* **A label of its own, with no words — O157.** The box has no
                visible label, only its `aria-label`, so under a coarse pointer
                the 24px box would be the whole tap target. `admin.css` grows
                this label to 44×44 and leaves the box drawn at its own size;
                the `aria-label` stays the box's name. The size is all it leaves
                alone: the label takes its 44px of the row's layout, so the
                rule's name has less width beside it and a long one can wrap one
                line more. At a fine pointer the label is a flex box that draws
                the box where it would sit without one. */}
            <label className="enable">
              {/* Held from the send, as `AccountPage.tsx`'s boxes are
                  (O159): `aria-disabled` draws nothing on a box, and the
                  box is controlled, so React puts a refused tick back. */}
              <input
                type="checkbox"
                checked={rule.enabled}
                aria-disabled={frozen}
                aria-label={`rule ${index + 1} enabled`}
                onChange={() => {
                  if (!frozen) onToggle(index)
                }}
              />
            </label>
            <HeldButton
              type="button"
              className="select"
              held={frozenDrawn}
              // O156: the selection is marked on the element that takes
              // focus, this button, rather than on the `<li>` around it.
              aria-current={selected === index ? 'true' : undefined}
              aria-label={`${ruleName(rule)} — ${ruleMechanism(rule)}`}
              onClick={() => {
                if (!frozen) onSelect(index)
              }}
            >
              <span className="rule-name">
                <NameWithBreaks name={ruleName(rule)} />
              </span>
              <span className="mechanism">{ruleMechanism(rule)}</span>
            </HeldButton>
            {!rule.enabled && <span className="off">off</span>}
            {rule.id !== 0 && wroteRule.has(rule.id) && (
              <span
                className="from-proposal"
                title="written by an accepted proposal"
              >
                from a proposal
              </span>
            )}
            <HeldButton
              type="button"
              className="delete"
              held={frozenDrawn}
              aria-label={`delete rule ${index + 1}`}
              onClick={() => {
                if (!frozen) askOrDelete(index)
              }}
            >
              Delete
            </HeldButton>
          </li>
        ))}
      </ol>

      {confirming !== undefined && (
        <div className="confirm" role="alertdialog">
          <p>
            Rule {confirming.index + 1} was written by proposal{' '}
            {confirming.proposal}. Deleting it reopens that proposal — the
            question comes back to the inbox.
          </p>
          <HeldButton
            type="button"
            className="confirm-delete"
            held={frozenDrawn}
            onClick={() => {
              if (frozen) return
              onDelete(confirming.index)
              setConfirming(undefined)
            }}
          >
            Delete and reopen
          </HeldButton>
          <button type="button" onClick={() => setConfirming(undefined)}>
            Keep it
          </button>
        </div>
      )}

      <p className="rule-count">
        {rules.length} rules
        {overrideCount > 0 && (
          <>
            {' · '}
            <span className="override-badge">
              {overrideCount === 1
                ? '1 override'
                : `${overrideCount} overrides`}
            </span>
          </>
        )}
      </p>
      {rules.length > PREVIEW_EXPENSIVE_ABOVE && (
        <p className="cost">
          Past {PREVIEW_EXPENSIVE_ABOVE} rules the preview gets expensive —
          override detection compares every pair, so the cost rises faster than
          the list does.
        </p>
      )}

      <HeldButton
        type="button"
        className="add-rule"
        held={frozenDrawn}
        onClick={() => {
          if (!frozen) onAdd()
        }}
      >
        + Add
      </HeldButton>
    </div>
  )
}

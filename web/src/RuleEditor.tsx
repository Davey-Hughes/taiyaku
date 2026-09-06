/**
 * The rule editor — spec C4 §1, and the build design's §5. The one screen
 * Komga structurally cannot have.
 *
 * **It previews on commit and never while typing.** A preview costs a median
 * 281 ms on a real library (130 series, 1858 works) and 495 ms with a rule
 * appended, with the diff 1.3 ms of it — the derivation is the entire cost.
 * So a preview runs on: a field blur or Enter, an enable toggle, a
 * drag-reorder, an add, a delete, and selecting a different rule. The last is
 * the one that is not obvious: `rows` is scoped to the selected rule, so
 * changing which rule is open is the only way to populate the Matched files
 * and Provenance tabs for it — and it is one gesture, not a stream. A person
 * clicking down a five-rule list pays five derivations; a person typing a
 * regex pays none. The alternatives — an explicit Preview button, a long
 * typing debounce — are weighed and refused in the build design's §5.3.
 *
 * **Nothing is written until Save**, per C4 §1, and the button states what it
 * will do from the live preview's `works_rehomed`. Because the preview is
 * never stale, that number is never a lie.
 *
 * **The save's own impact is compared against the preview it was made from.**
 * Nothing should differ on a single-writer library — but the screen's entire
 * promise is that the preview is true, and the cheapest place to notice it is
 * false is the save that just disproved it. So a difference is drawn rather
 * than swallowed. The compare is field-by-field over `Object.keys`, matching
 * `RuleForm`'s own dirty-check comment rather than the `JSON.stringify` it
 * warns against — `Impact` is server-controlled, but the risk the comment
 * names (a field reorder silently comparing unequal objects as equal) is a
 * property of `JSON.stringify` itself, not of who controls the type.
 *
 * **Save is disabled while a preview is in flight, not only while saving.**
 * The comparison above means something only against a preview that has
 * landed: for as long as `busy` is true, `predicted.current` still holds the
 * *previous* preview's impact. Clicking Save inside that window sends the
 * just-committed `rules` and compares the answer against that stale impact —
 * drawing the "something changed the library" warning for a race the client
 * caused, not the server, on the very six gestures this file's own doc
 * comment names as preview triggers.
 *
 * **No admin guard.** `/config/rules` is reachable by URL and this screen
 * renders the server's own 403 sentence, exactly as `/proposals` does. A
 * client that *decides* who may administer is the shape O93 closed.
 *
 * **A sequence number on the preview, because responses need not arrive in
 * order.** Several commits can be in flight — a blur that starts a preview,
 * then a click on another rule before it lands — and without this an earlier
 * response arriving later would write a stale impact under a Save button that
 * states a number from it. `Roots.tsx` and `Search.tsx` carry the same
 * counter for the same reason.
 *
 * **A successful save re-previews the list it just saved, and Save stays
 * disabled until that lands.** Whole-branch review, I1. `save` does not
 * touch `preview`, so without this the button kept stating the *pre-save*
 * `works_rehomed` after the library had already been re-homed to match it —
 * and a second click sent an already-derived library through `apply` again,
 * where every `Impact` field comes back `0` against a `predicted.current`
 * that still held the first save's number. That mismatch drew "the save
 * differs from the preview it was made from", blaming the server for a race
 * the client caused, one click after the in-flight-preview fix closed the
 * same failure for every other gesture. The mismatch warning itself is kept
 * off `failure` and in its own `saveMismatch` slot for the identical reason
 * `importNotice` is kept off it: the follow-up preview this fix triggers
 * clears `failure` on landing, and a warning stored there would be wiped by
 * the very preview run to produce the number it is standing next to.
 *
 * **A failed preview invalidates `preview` and `predicted.current`, not only
 * `busy`.** Whole-branch review, I2. Without this, a preview that failed —
 * a network blip, a 500, the 400 the store raises for a rule id another
 * admin's edit just deleted — left both at whatever the *last successful*
 * preview computed, for a rule list that is no longer the one on screen. The
 * button stayed enabled, stated that stale number, and a click sent the
 * current (unpreviewed) list to `apply` — which then disagreed with the
 * stale `predicted.current` and drew the same "something changed the
 * library" warning, again blaming the server for a client-side gap.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import type { ChangeEvent } from 'react'
import { listRoots, type Root } from './config'
import { listProposals, type Proposal } from './curation'
import { PreviewTabs } from './PreviewTabs'
import { RuleForm } from './RuleForm'
import { RuleList } from './RuleList'
import { DeriveSection } from './ScanReport'
import {
  blankRule,
  type Impact,
  importRules,
  listRules,
  moved,
  type Preview,
  previewRules,
  renumber,
  type Rule,
  type Saved,
  saveRules,
  toDocument,
} from './rules'

const message = (err: unknown) =>
  err instanceof Error ? err.message : String(err)

/**
 * Field-by-field, matching `RuleForm`'s own dirty-check comment. `Impact` is
 * a flat record of numbers, so `Object.keys` alone is enough to compare it —
 * no nested walk is needed the way a `Rule` compare might.
 */
const impactChanged = (a: Impact, b: Impact) =>
  (Object.keys(a) as (keyof Impact)[]).some((k) => a[k] !== b[k])

/** Only accepted proposals carry a `created_rule_id` — the badge's whole input. */
const acceptedProposals = () =>
  listProposals({ state: 'accepted' }).then((inbox) => inbox.proposals)

export function RuleEditor() {
  const [rules, setRules] = useState<Rule[]>([])
  const [roots, setRoots] = useState<Root[]>([])
  const [proposals, setProposals] = useState<Proposal[]>([])
  const [selected, setSelected] = useState<number | undefined>()
  const [preview, setPreview] = useState<Preview | undefined>()
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * Which proposals the last import reopened, if any. Separate from
   * `failure` for the reason `Roots.tsx`'s `addFailure` is separate from its
   * `failure`, and read that file's own doc comment on the pair: `doImport`
   * fires a follow-up `runPreview` right after landing, and that preview's
   * success path unconditionally clears `failure` — so a notice stored there
   * was wiped by the very call that had just populated it, almost always
   * before a person could read it. Naming which proposals reopened is the
   * whole reason the import route reports them at all; it needs a slot
   * `runPreview` does not touch. Cleared only by the next import.
   */
  const [importNotice, setImportNotice] = useState<string | undefined>()
  /**
   * Set when a landed save's own impact differs from the preview it was made
   * from. Separate from `failure` for the reason this file's own top comment
   * gives on I1: the follow-up preview `save` triggers on landing clears
   * `failure` when it succeeds, and a warning stored there would be wiped by
   * the very preview run to produce a true number for it. Cleared only by
   * the next save.
   */
  const [saveMismatch, setSaveMismatch] = useState<string | undefined>()
  const [saved, setSaved] = useState<Saved | undefined>()
  const [saving, setSaving] = useState(false)
  /** The preview whose impact `saved` is compared against. */
  const predicted = useRef<Preview | undefined>(undefined)
  /** The newest `previewRules` call — see this file's own doc comment. */
  const latest = useRef(0)
  /** Whether a save is already on its way. A ref, for the reason `Roots.tsx`'s is. */
  const savingNow = useRef(false)
  /** The hidden file input the Import button triggers. */
  const fileInput = useRef<HTMLInputElement>(null)

  const runPreview = useCallback((next: Rule[], open: number | undefined) => {
    const seq = ++latest.current
    setBusy(true)
    previewRules(next, open)
      .then((body) => {
        if (seq !== latest.current) return
        setPreview(body)
        predicted.current = body
        setBusy(false)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setBusy(false)
        setFailure(message(err))
        // I2: a failed preview describes nothing — leaving the last
        // *successful* preview in place would keep Save enabled and stating
        // a number computed for a rule list that is no longer the one on
        // screen. `preview === undefined` is what the Save button's own
        // `disabled` already checks, so invalidating both here is enough to
        // disable it without a second condition to keep in sync.
        setPreview(undefined)
        predicted.current = undefined
      })
  }, [])

  useEffect(() => {
    Promise.all([listRules(), listRoots(), acceptedProposals()])
      .then(([listed, rootList, accepted]) => {
        setRules(listed.rules)
        setRoots(rootList.roots)
        setProposals(accepted)
        setFailure(undefined)
        runPreview(listed.rules, undefined)
      })
      .catch((err: unknown) => setFailure(message(err)))
  }, [runPreview])

  /** Every editor gesture lands here: set the rules, then preview once. */
  const commit = (next: Rule[], open: number | undefined) => {
    setRules(next)
    setSelected(open)
    setSaved(undefined)
    runPreview(next, open)
  }

  const save = () => {
    if (savingNow.current) return
    savingNow.current = true
    setSaving(true)
    setFailure(undefined)
    setSaveMismatch(undefined)
    const made = predicted.current
    saveRules(rules)
      .then((answer) => {
        savingNow.current = false
        setSaving(false)
        setSaved(answer)
        setRules(answer.rules)
        if (made !== undefined && impactChanged(made.impact, answer.impact))
          setSaveMismatch(
            'The save differs from the preview it was made from. Something changed the library ' +
              'between the two — the numbers below are what actually happened.',
          )
        // I1: the button's number has to describe *this* saved list, not the
        // one a save ago — so a fresh preview runs for it here, through the
        // same seq-guarded `runPreview` every other gesture uses, and Save
        // stays disabled (`busy`) until it lands. `selected` still names the
        // right rule: a save preserves position, only ids on a freshly
        // inserted row change.
        runPreview(answer.rules, selected)
        // The list just changed under every accepted proposal's
        // `created_rule_id`, so the badges are re-read rather than assumed.
        acceptedProposals()
          .then(setProposals)
          .catch(() => {})
      })
      .catch((err: unknown) => {
        savingNow.current = false
        setSaving(false)
        setFailure(message(err))
      })
  }

  const doImport = (text: string) => {
    // Cleared up front, the same as `Roots.tsx`'s `addFailure` — so a stale
    // notice from a previous import cannot outlive this one, whichever way
    // it resolves.
    setImportNotice(undefined)
    importRules(text)
      .then((answer) => {
        // Every other editor gesture lands in `commit`; this is the shape it
        // already has (set the rules, close/leave the selection, drop the
        // last save, preview once), so it goes through the same funnel
        // rather than repeating it.
        commit(answer.rules, undefined)
        // I3: `POST .../rules/import` writes the rule table and derives
        // nothing — the CLI's own `library rules --import` prints exactly
        // this warning for the identical route (`library.rs`). This screen
        // used to say nothing at all, which left the imported list looking
        // applied the moment it appeared, when in fact the library's
        // structure is still the old rules' until something derives — the
        // exact state this branch exists to make unreachable, reached
        // through the editor's own Import button. The remedy named here is
        // Save, not a scan command: on this screen Save is the thing that
        // derives.
        const reopened =
          answer.reopened_proposals.length > 0
            ? ` It also reopened ${answer.reopened_proposals.length} accepted ` +
              `proposals: ${answer.reopened_proposals.join(', ')} — an imported ` +
              'document carries none of this library’s rule ids, so every rule ' +
              'it replaces is a new row.'
            : ''
        setImportNotice(
          'The imported rules are stored, but the library has not been re-derived ' +
            'from them yet — its structure still reflects the rules from before this ' +
            `import. Save applies them.${reopened}`,
        )
      })
      .catch((err: unknown) => setFailure(message(err)))
  }

  const exportDoc = () => {
    try {
      return JSON.stringify(toDocument(rules, roots), null, 2)
    } catch (err: unknown) {
      // **Guarded, not called unconditionally.** This runs inline in the
      // Export link's `href` on every render, so calling `setFailure` without
      // checking against the current value fires again on the very next
      // render it causes — for as long as a rule names a root this library
      // does not have, which is exactly as long as this catch keeps being
      // reached. React's own render-phase-update budget is not infinite: a
      // bare `setFailure(message(err))` here crashed the whole screen with
      // "Too many re-renders" rather than showing the message, confirmed by
      // a minimal reproduction of the same pattern outside this component.
      const reason = message(err)
      if (reason !== failure) setFailure(reason)
      return ''
    }
  }

  return (
    <section className="rule-editor">
      <h2>Rules</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {saveMismatch !== undefined && (
        <p role="alert" className="save-mismatch">
          {saveMismatch}
        </p>
      )}

      <div className="editor-panes">
        <RuleList
          rules={rules}
          selected={selected}
          proposals={proposals}
          overrideCount={preview?.overrides.length ?? 0}
          onSelect={(index) => {
            // M1: the design runs a preview on selecting a *different*
            // rule — `rows` is scoped to `selected`, and changing which is
            // open is what repopulates it. Re-clicking the one already open
            // changes nothing a preview could answer differently, so
            // committing anyway paid another ~281 ms derivation under the
            // write lock and cleared the "Saved:" line for no reason.
            if (index !== selected) commit(rules, index)
          }}
          onToggle={(index) =>
            commit(
              rules.map((rule, i) =>
                i === index ? { ...rule, enabled: !rule.enabled } : rule,
              ),
              selected,
            )
          }
          onReorder={(from, to) => {
            const next = renumber(moved(rules, from, to))
            // The selected rule travels with the drag rather than staying at
            // an index that now holds a different rule.
            const open =
              selected === undefined
                ? undefined
                : selected === from
                  ? to
                  : selected
            commit(next, open)
          }}
          onAdd={() => {
            const next = renumber([...rules, blankRule(rules.length + 1)])
            commit(next, next.length - 1)
          }}
          onDelete={(index) => {
            const next = renumber(rules.filter((_, i) => i !== index))
            commit(
              next,
              selected === undefined || selected === index
                ? undefined
                : selected > index
                  ? selected - 1
                  : selected,
            )
          }}
        />

        <div className="editor-right">
          {selected !== undefined && rules[selected] !== undefined ? (
            <RuleForm
              // Local draft state lives in the form; a new key is what makes
              // selecting a different rule redraw rather than keep the last
              // one's half-typed text.
              key={`${selected}:${rules[selected].id}`}
              rule={rules[selected]}
              roots={roots}
              onCommit={(edited) =>
                commit(
                  rules.map((rule, i) => (i === selected ? edited : rule)),
                  selected,
                )
              }
            />
          ) : (
            <p className="empty">
              Pick a rule to edit it. The impact below is for the whole list.
            </p>
          )}

          <PreviewTabs
            preview={preview}
            rules={rules}
            selected={selected}
            busy={busy}
            saving={saving}
            onReorder={(from, to) => {
              const next = renumber(moved(rules, from, to))
              commit(next, selected === from ? to : selected)
            }}
            // The same call `RuleList`'s own `onSelect` makes, guard included:
            // "Edit rule 4" from an override row is selecting rule 4, so it
            // costs one derivation, and selecting the rule already open costs
            // none.
            onSelectRule={(index) => {
              if (index !== selected) commit(rules, index)
            }}
          />
        </div>
      </div>

      {saved !== undefined && (
        <p className="saved">
          Saved: {saved.impact.works_rehomed} works re-homed,{' '}
          {saved.impact.series_merged} series merged.
          {saved.reopened_proposals.length > 0 &&
            ` Reopened proposals ${saved.reopened_proposals.join(', ')}.`}
        </p>
      )}
      {saved?.complaints.map((complaint) => (
        <p className="complaint" key={complaint}>
          {complaint}
        </p>
      ))}
      {saved !== undefined && (
        // Context: the save's own re-derive touches `<root>/.taiyaku/`, the
        // one place on disk this route writes, and `Impact` says nothing
        // about it — a save that could not write a hand document used to
        // answer with only a rehome count and silence. `DeriveSection` is
        // the renderer `ScanReport.tsx` already has for this exact struct,
        // reused rather than a second, narrower vocabulary invented here;
        // collapsed by default the same way `RemoveRoot` reuses it, since
        // most saves have nothing in these fields worth a person's first
        // glance.
        <details className="save-derive">
          <summary>What this save's re-derive found</summary>
          <DeriveSection derive={saved.derive} />
        </details>
      )}

      <div className="editor-actions">
        {/*
         * Disabled on `busy` too, not only `saving` — see this file's own
         * top comment. Without it, a click landing inside the ~300-500 ms a
         * preview takes sends `rules` against a `predicted.current` that has
         * not caught up yet, and the mismatch draws a warning that blames
         * the server for a race the client caused.
         */}
        <button
          type="button"
          className="save"
          disabled={saving || busy || preview === undefined}
          onClick={save}
        >
          {saving
            ? 'Saving…'
            : `Save — apply to ${preview?.impact.works_rehomed ?? 0} works`}
        </button>

        <div className="import">
          <button type="button" onClick={() => fileInput.current?.click()}>
            Import
          </button>
          <p>
            Import replaces every rule with the document’s, minting fresh ids,
            which reopens every accepted proposal whose rule it replaces.
          </p>
          {importNotice !== undefined && <p role="alert">{importNotice}</p>}
          <input
            ref={fileInput}
            type="file"
            accept="application/json"
            hidden
            onChange={(e: ChangeEvent<HTMLInputElement>) => {
              const file = e.target.files?.[0]
              e.target.value = ''
              if (file === undefined) return
              file
                .text()
                .then(doImport)
                .catch((err: unknown) => setFailure(message(err)))
            }}
          />
        </div>

        <a
          className="export"
          download="rules.json"
          href={`data:application/json,${encodeURIComponent(exportDoc())}`}
        >
          Export
        </a>
      </div>
    </section>
  )
}

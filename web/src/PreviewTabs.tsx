/**
 * The preview's four tabs — spec C4 §1's right-hand pane, bottom half, and
 * the build design's §5.2.
 *
 * **Four, not three, and the fourth carries a badge drawn from every tab.**
 * C4 §1 names three tabs and describes override detection as a warning; the
 * server half calls it a tab. A warning-that-is-a-tab has the failure mode a
 * warning does not — an override hidden behind a tab nobody clicked — so the
 * count is on the tab strip itself, visible from wherever you are.
 *
 * **The fourth tab states, and does not correct — O95.** On a library holding
 * nothing but the four rules migration 3 seeds, the one row it draws is rule 4
 * reading a series from the filename over rule 1 reading it from the directory
 * — which is **what folds a colour edition into its main series**, and moving
 * rule 4 above rule 1 is the one thing that breaks it. So the row states what
 * happens, draws the two values it happened between, and offers reorder and
 * edit as *options* rather than as fixes: that reads right for a deliberate
 * layering and for a genuine mistake both, and no detector can tell those
 * apart, because the difference is intent.
 *
 * *(O89: only where the directory does not qualify the filename's series. A
 * qualifying directory such as `Death Note (Colored Council)` is its own
 * series, not an override — `docs/status.md`, O89. The one known exception is
 * O128: a user series rule placed between the directory rule and a later
 * capture still draws a row, crediting the capture with the directory's
 * restored value.)*
 *
 * **Matched files and Provenance read one array.** `rows` is scoped to the
 * selected rule and arrives in one request; the two tabs are two readings of it
 * — one down the file list, one across the fields some row decided (O149).
 *
 * **Every rule named here is named by position.** `Decided`, `Override.first`
 * and `Override.wins` are indices into the list the request carried, and the
 * store holds that boundary deliberately: an unsaved rule carries `id: 0`,
 * indistinguishable from a second unsaved rule and from the root-fallback
 * sentinel. "Rule 3" on this screen means the third rule in the editor, which
 * is the rule the person is looking at.
 *
 * **"Proposals resolved" is not drawn, because it is not reportable.** C4 §1
 * lists it; a derivation reaches one proposal writer and it only ever raises
 * or reopens, so the number could only ever be zero. Drawing a structural zero
 * as a figure is the invented number this repository's own discipline refuses.
 *
 * **The row cap is drawn.** A table that quietly stopped at 500 reads as the
 * whole list.
 */
import { type KeyboardEvent, useId, useRef, useState } from 'react'
import { HeldButton } from './HeldButton'
import { useDrawnWait } from './hooks'
import {
  type Field,
  FIELDS,
  type Preview,
  type Rule,
  unsetFields,
  unsetSentence,
} from './rules'

const TABS = ['Impact', 'Matched files', 'Provenance', 'Overrides'] as const
type TabName = (typeof TABS)[number]

const plural = (n: number, one: string, many: string) =>
  `${n} ${n === 1 ? one : many}`

/**
 * One side of an override, as the row draws it.
 *
 * **Two absences told apart, for the same reason `RuleList`'s own label chain
 * and this file's `||` below tell two others apart.** `null` is the field left
 * unset — a real half of a disagreement, since a later rule *clearing* what an
 * earlier one set is exactly what the row exists to show — while `''` is a
 * rule that set it to the empty string. Rendering either as itself puts a
 * blank in the one sentence whose whole job is telling the two values apart.
 */
const shown = (value: string | number | null) => {
  if (value === null) return 'unset'
  if (value === '') return '(empty)'
  return String(value)
}

/**
 * How the row names the rule sitting beside one of its two values.
 *
 * **Three cases, not two, and the third is why this is a function.** A rule's
 * `pattern` is empty in two quite different situations: `blankRule()` leaves
 * it `''` on a rule nobody has finished typing, and migration 3 seeds
 * `builtin-volume` and `builtin-chapter` with `pattern = ''` **by design** —
 * they take no pattern and never will. Calling the second "(no pattern yet)"
 * tells a reader a shipped seeded rule is unfinished, and it is exactly the
 * pairing the numeric rows draw: a hand-written volume regex disagreeing with
 * `builtin-volume` is the likeliest `volume` override there is. So a builtin
 * is named by its kind, which is what identifies it.
 */
const names = (rule: Rule | undefined) => {
  if (rule === undefined) return '(no rule)'
  if (rule.pattern) return rule.pattern
  if (rule.kind.startsWith('builtin-')) return rule.kind
  return '(no pattern yet)'
}

export function PreviewTabs({
  preview,
  rules,
  selected,
  busy,
  frozen,
  onReorder,
  onSelectRule,
}: {
  preview: Preview | undefined
  rules: Rule[]
  selected: number | undefined
  /**
   * A preview is in flight. It holds the empty state back, and the two
   * option buttons; "Previewing…" is the screen's to draw, on the Preview
   * heading's line (`RuleEditor`).
   */
  busy: boolean
  /**
   * A write is in flight — a save or an import. Separate from `busy` because
   * they gate different things: `busy` also holds the empty state back,
   * which a write does not, while both make the two option buttons unsafe —
   * see their own comment below.
   */
  frozen: boolean
  onReorder: (from: number, to: number) => void
  onSelectRule: (index: number) => void
}) {
  const [tab, setTab] = useState<TabName>('Impact')
  /** What holds the two option buttons, from the send: see their comment. */
  const optionsHeld = busy || frozen
  /** The same, once it has lasted: their held look. */
  const optionsHeldDrawn = useDrawnWait(optionsHeld)
  /**
   * O156: tabs, because they switch what the pane below shows — not navigation,
   * since no URL moves. One panel, labelled by whichever tab is selected. The
   * keys are the ARIA guide's for tabs: only the selected tab is a `Tab` stop,
   * and ←, →, Home and End move the selection with the focus, since every panel
   * draws from the preview already held.
   */
  const ids = useId()
  const tabId = (name: TabName) => `${ids}-tab-${TABS.indexOf(name)}`
  const panelId = `${ids}-panel`
  const tabButtons = useRef<(HTMLButtonElement | null)[]>([])
  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    // A modifier means the keystroke is the browser's: Alt+← is Back.
    if (event.altKey || event.ctrlKey || event.metaKey) return
    // From the tab the key was pressed on, which is the selected one unless
    // focus got to another without selecting it.
    const at = tabButtons.current.indexOf(event.currentTarget)
    const to =
      event.key === 'ArrowRight'
        ? (at + 1) % TABS.length
        : event.key === 'ArrowLeft'
          ? (at - 1 + TABS.length) % TABS.length
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? TABS.length - 1
              : undefined
    if (to === undefined) return
    event.preventDefault()
    setTab(TABS[to])
    tabButtons.current[to]?.focus()
  }
  const overrides = preview?.overrides ?? []
  /**
   * O149: the Provenance table draws a column only for a field some row
   * decided, and names the rest in one line above it.
   */
  // Annotated: without it the empty branch types as `never[]`, and
  // `includes` on the union then takes `never`.
  const unset: Field[] = preview === undefined ? [] : unsetFields(preview.rows)
  const drawn = FIELDS.filter((field) => !unset.includes(field))
  /**
   * Drawn on both tabs that list files, so neither stops silently at 500 —
   * and above the table on both, where it is read before the rows it qualifies
   * rather than after them, for the same reason O149's line sits above.
   */
  const capped = (
    <p className="truncated">
      The first 500 files only — more matched than fit here.
    </p>
  )

  return (
    <div className="preview">
      <div className="preview-tabs" role="tablist" aria-label="Preview">
        {TABS.map((name, index) => (
          <button
            key={name}
            ref={(button) => {
              tabButtons.current[index] = button
            }}
            type="button"
            role="tab"
            id={tabId(name)}
            aria-selected={tab === name}
            aria-controls={panelId}
            tabIndex={tab === name ? 0 : -1}
            onClick={() => setTab(name)}
            onKeyDown={onTabKey}
          >
            {name}
            {name === 'Overrides' && overrides.length > 0 && (
              <>
                {' '}
                <span className="badge">{overrides.length}</span>
              </>
            )}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={panelId} aria-labelledby={tabId(tab)}>
        {/*
        **The empty state is suppressed while a preview is in flight**, because
        the two are reachable together and contradict each other when they are.
        `RuleEditor` holds `busy` from its first render until its first preview
        answers, with `preview` still `undefined`, and "No preview yet. Pick a
        rule, or edit one" under "Previewing…" would tell someone to start a
        thing that has already started. So the empty state is drawn only where
        no preview stands and none is out: after a preview has failed.
      */}
        {preview === undefined ? (
          busy ? null : (
            <p className="empty">
              No preview yet. Pick a rule, or edit one — the preview runs when
              you finish a change rather than while you type.
            </p>
          )
        ) : (
          <div className="preview-body">
            {tab === 'Impact' && (
              <ul className="impact">
                <li>
                  {plural(
                    preview.impact.series_merged,
                    'series merged',
                    'series merged',
                  )}
                </li>
                <li>
                  {plural(
                    preview.impact.works_rehomed,
                    'work re-homed',
                    'works re-homed',
                  )}
                </li>
                <li>{preview.impact.works_orphaned} orphaned</li>
                <li>
                  {plural(
                    preview.impact.proposals_reopened,
                    'proposal reopened',
                    'proposals reopened',
                  )}
                </li>
                <li>
                  {plural(
                    preview.impact.proposals_raised,
                    'proposal raised',
                    'proposals raised',
                  )}
                </li>
                <li className="series-count">
                  {preview.impact.series_before} series before ·{' '}
                  {preview.impact.series_after} after
                </li>
              </ul>
            )}

            {tab === 'Matched files' &&
              (preview.rows.length === 0 ? (
                <p className="empty">
                  No files.{' '}
                  {selected === undefined
                    ? 'Select a rule to see what it reaches.'
                    : 'This rule matched nothing and came near nothing.'}
                </p>
              ) : (
                <>
                  {preview.rows_truncated && capped}
                  <div className="table-scroll">
                    <table className="matched">
                      <thead>
                        <tr>
                          <th>file</th>
                          <th>before</th>
                          <th>after</th>
                          <th>match</th>
                        </tr>
                      </thead>
                      <tbody>
                        {preview.rows.map((row) => (
                          <tr key={`${row.root_id}:${row.rel_path}`}>
                            <td>{row.rel_path}</td>
                            <td>{row.before?.series ?? '—'}</td>
                            <td>{row.after?.series ?? '—'}</td>
                            <td>{row.matched ? 'matched' : 'near miss'}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              ))}

            {tab === 'Provenance' &&
              (preview.rows.length === 0 ? (
                <p className="empty">No files to show provenance for.</p>
              ) : (
                <>
                  {preview.rows_truncated && capped}
                  {unset.length > 0 && (
                    <p className="unset">
                      {unsetSentence(
                        unset,
                        preview.rows.length,
                        preview.rows_truncated,
                      )}
                    </p>
                  )}
                  <div className="table-scroll">
                    <table className="provenance">
                      <thead>
                        <tr>
                          <th>file</th>
                          {drawn.map((field) => (
                            <th key={field}>{field}</th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {preview.rows.map((row) => (
                          <tr key={`${row.root_id}:${row.rel_path}`}>
                            <td>{row.rel_path}</td>
                            {drawn.map((field) => {
                              const decided = row.decided_by[field]
                              return (
                                <td key={field}>
                                  {decided === undefined
                                    ? '—'
                                    : decided.kind === 'root_default'
                                      ? 'root default'
                                      : `rule ${decided.index + 1}`}
                                </td>
                              )
                            })}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              ))}

            {tab === 'Overrides' &&
              (overrides.length === 0 ? (
                <p className="empty">
                  No rule replaces another's value. Two rules setting one field
                  to the <em>same</em> value are not an override — the seeded
                  defaults do that on purpose.
                </p>
              ) : (
                <>
                  <p className="overrides-lead">
                    A later rule deciding a field differently is not always a
                    mistake — layering a normalised value over a default is what
                    the seeded rules do. These are the places one rule's value
                    replaces another's.
                  </p>
                  <ul className="overrides">
                    {overrides.map((over, index) => (
                      <li key={`${over.field}:${over.rel_path}:${index}`}>
                        <p className="what">
                          <strong>{over.field}</strong> — rule {over.wins + 1}{' '}
                          overrides rule {over.first + 1} on{' '}
                          {plural(over.examples, 'file', 'files')}.
                        </p>
                        {/*
                        The path and the two values are drawn together because
                        they describe one entry between them: `Override`'s own
                        doc comment in the store carries why the values are
                        this file's rather than all `examples`' of it.
                      */}
                        <p className="where">{over.rel_path}</p>
                        <ul className="values">
                          {/*
                          `||`, not `??` — the same distinction `RuleList`'s own
                          label chain turns on. `blankRule()` leaves `pattern`
                          as `''` rather than `null`, and an empty pattern
                          matches everything, so a freshly added rule is
                          *especially* likely to be one half of an override. A
                          `??` here would leave a blank where the pattern
                          belongs, in the one place whose whole job is saying
                          which rule is which.
                        */}
                          <li>
                            rule {over.first + 1}{' '}
                            <code>{names(rules[over.first])}</code> →{' '}
                            {shown(over.first_value)}
                          </li>
                          <li>
                            rule {over.wins + 1}{' '}
                            <code>{names(rules[over.wins])}</code> →{' '}
                            {shown(over.wins_value)} (wins)
                          </li>
                        </ul>
                        {/*
                        **"in at least one of these files", not "here"** —
                        `others` is a *maximum* across every entry sharing this
                        row's `(field, first, wins)` triple (`entry.others =
                        entry.others.max(others)` in `preview.rs`), so it is a
                        claim about the group and not about the one file named
                        above it. Wording it "2 other rules disagree here.",
                        directly under that path and its two values, which are
                        explicitly that file's own, would be false of the named
                        file whenever a *later* entry is the one with the
                        longer chain.
                      */}
                        {over.others > 0 && (
                          <p className="others">
                            In at least one of these files,{' '}
                            {over.others === 1
                              ? 'one other rule disagrees'
                              : `${over.others} other rules disagree`}{' '}
                            too.
                          </p>
                        )}
                        {/*
                        **"Options:", and two of them** — O95. One button under
                        a sentence in the grammar of a warning is an instruction
                        whatever it says. "Edit rule N" narrows nothing by
                        itself: narrowing means editing a regex, which no button
                        can guess. What it does is put the winning rule in the
                        form, which is `RuleList`'s own `onSelect` — so it costs
                        exactly one derivation, the same as clicking that rule
                        in the list.
                      */}
                        <p className="options">
                          Options:{' '}
                          {/*
                          **Both are held while a preview or a write is in
                          flight**, because this row is drawn from the
                          *previous* preview and its indices need not still
                          describe `rules`. Held by `aria-disabled`, so the
                          one pressed keeps focus through the preview it
                          starts (`HeldButton`), and so each press is turned
                          down here from the send.

                          The preview window: delete the last rule and press
                          either before the preview lands. "Edit rule N" sends
                          an out-of-range `selected` the route refuses
                          (`preview::tests::an_out_of_range_selected_index_is_refused_naming_the_index_and_the_lists_length`),
                          which clears the preview and blames the server for a
                          race the client caused; the reorder is sharper still
                          — `moved()` splices `undefined` in and `renumber`
                          turns it into a phantom `{ordinal: n}` rule.

                          **The write window is the wider one.** `save()` sets
                          `writer.current` (which `frozen` reads), never `busy`,
                          so `busy` alone would leave these two buttons live for
                          the whole of a save. A reorder pressed inside it would
                          be applied, and then silently discarded by the save's
                          own `setRules(answer.rules)`; "Edit rule N" would
                          leave the form on one rule while the Matched-files and
                          Provenance tabs stay scoped to the `selected` captured
                          before the click.
                        */}
                          <HeldButton
                            type="button"
                            className="reorder"
                            held={optionsHeldDrawn}
                            onClick={() => {
                              if (!optionsHeld) onReorder(over.wins, over.first)
                            }}
                          >
                            Move rule {over.wins + 1} above rule{' '}
                            {over.first + 1}
                          </HeldButton>{' '}
                          <HeldButton
                            type="button"
                            className="edit-winner"
                            held={optionsHeldDrawn}
                            onClick={() => {
                              if (!optionsHeld) onSelectRule(over.wins)
                            }}
                          >
                            Edit rule {over.wins + 1}
                          </HeldButton>
                        </p>
                      </li>
                    ))}
                  </ul>
                </>
              ))}
          </div>
        )}
      </div>
    </div>
  )
}

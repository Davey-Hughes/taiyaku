/**
 * The preview's four tabs — spec C4 §1's right-hand pane, bottom half, and
 * the build design's §5.2.
 *
 * **Four, not three, and the fourth carries a badge drawn from every tab.**
 * C4 §1 names three tabs and describes conflict detection as a warning; the
 * server half calls it a tab. A warning-that-is-a-tab has the failure mode a
 * warning does not — a conflict hidden behind a tab nobody clicked — so the
 * count is on the tab strip itself, visible from wherever you are.
 *
 * **Matched files and Provenance read one array.** `rows` is scoped to the
 * selected rule and arrives in one request; the two tabs are two readings of
 * it — one down the file list, one across the seven fields.
 *
 * **Every rule named here is named by position.** `Decided`, `Conflict::first`
 * and `Conflict::wins` are indices into the list the request carried, and the
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
import { useState } from 'react'
import type { Preview, Rule } from './rules'

const TABS = ['Impact', 'Matched files', 'Provenance', 'Conflicts'] as const
type TabName = (typeof TABS)[number]

/** The seven fields, in the order `rules::Field` declares them. */
const FIELDS = [
  'series',
  'edition_key',
  'kind',
  'language',
  'volume',
  'chapter',
  'title',
] as const

const plural = (n: number, one: string, many: string) =>
  `${n} ${n === 1 ? one : many}`

export function PreviewTabs({
  preview,
  rules,
  selected,
  busy,
  onReorder,
}: {
  preview: Preview | undefined
  rules: Rule[]
  selected: number | undefined
  busy: boolean
  onReorder: (from: number, to: number) => void
}) {
  const [tab, setTab] = useState<TabName>('Impact')
  const conflicts = preview?.conflicts ?? []

  return (
    <div className="preview">
      <nav className="preview-tabs" aria-label="Preview">
        {TABS.map((name) => (
          <button
            key={name}
            type="button"
            aria-current={tab === name ? 'true' : undefined}
            onClick={() => setTab(name)}
          >
            {name}
            {name === 'Conflicts' && conflicts.length > 0 && (
              <span className="badge">{conflicts.length}</span>
            )}
          </button>
        ))}
      </nav>

      {busy && <p className="busy">Previewing…</p>}

      {/*
        **The empty state is suppressed while a preview is in flight**, because
        the two are reachable together and contradict each other when they are.
        On the screen's very first load `busy` is true and `preview` is still
        `undefined`, which drew "Previewing…" directly above "No preview yet.
        Pick a rule, or edit one" — telling someone to start a thing that had
        already started. Once a preview has landed the empty state is gone for
        good, so this only ever suppresses the first one.
      */}
      {preview === undefined ? (
        busy ? null : (
          <p className="empty">
            No preview yet. Pick a rule, or edit one — the preview runs when you
            finish a change rather than while you type.
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
                {preview.rows_truncated && (
                  <p className="truncated">
                    The first 500 files only — more matched than fit here.
                  </p>
                )}
              </>
            ))}

          {tab === 'Provenance' &&
            (preview.rows.length === 0 ? (
              <p className="empty">No files to show provenance for.</p>
            ) : (
              <table className="provenance">
                <thead>
                  <tr>
                    <th>file</th>
                    {FIELDS.map((field) => (
                      <th key={field}>{field}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((row) => (
                    <tr key={`${row.root_id}:${row.rel_path}`}>
                      <td>{row.rel_path}</td>
                      {FIELDS.map((field) => {
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
            ))}

          {tab === 'Conflicts' &&
            (conflicts.length === 0 ? (
              <p className="empty">
                No two enabled rules decide a field differently. Two rules
                setting one field to the <em>same</em> value are not a conflict
                — the seeded defaults do that on purpose.
              </p>
            ) : (
              <ul className="conflicts">
                {conflicts.map((conflict, index) => (
                  <li key={`${conflict.field}:${conflict.rel_path}:${index}`}>
                    <p>
                      <strong>{conflict.field}</strong> on {conflict.rel_path}:
                      rule {conflict.first + 1} says{' '}
                      {/*
                        `||`, not `??` — the same distinction `RuleList`'s own
                        label chain turns on. `blankRule()` leaves `pattern`
                        as `''` rather than `null`, and an empty pattern
                        matches everything, so a freshly added rule is
                        *especially* likely to be one half of a conflict. A
                        `??` here rendered "rule 1 says , rule 2 wins" — a
                        blank where the pattern belongs, in the one sentence
                        whose whole job is saying which rule is which.
                      */}
                      {rules[conflict.first]?.pattern || '(no pattern yet)'},
                      rule {conflict.wins + 1} wins
                      {conflict.others > 0 &&
                        ` — and ${conflict.others} others`}
                      . {conflict.examples} file
                      {conflict.examples === 1 ? '' : 's'} like this one.
                    </p>
                    <button
                      type="button"
                      className="reorder"
                      onClick={() => onReorder(conflict.wins, conflict.first)}
                    >
                      Move rule {conflict.wins + 1} above rule{' '}
                      {conflict.first + 1}
                    </button>
                  </li>
                ))}
              </ul>
            ))}
        </div>
      )}
    </div>
  )
}

import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { PreviewTabs } from './PreviewTabs'
import type { Impact, Override, Preview, Row, Rule } from './rules'
import { mount } from './testing/mount'

const impact = (over: Partial<Impact> = {}): Impact => ({
  series_before: 8,
  series_after: 2,
  series_merged: 6,
  works_rehomed: 71,
  works_orphaned: 0,
  proposals_reopened: 3,
  proposals_raised: 1,
  ...over,
})

const row = (over: Partial<Row> = {}): Row => ({
  rel_path: 'Manga (RAW)/Vol 1.cbz',
  root_id: 1,
  before: { series: 'Old', work: 'old-1', edition: 'en' },
  after: { series: 'New', work: 'new-1', edition: 'raw' },
  matched: true,
  decided_by: {
    series: { kind: 'rule', index: 0 },
    edition_key: { kind: 'root_default' },
  },
  ...over,
})

const anOverride = (over: Partial<Override> = {}): Override => ({
  field: 'edition_key',
  rel_path: 'Manga (RAW)/Go-Toubun no Hanayome - Full Color',
  first: 0,
  wins: 1,
  first_value: 'raw',
  wins_value: 'en-color',
  others: 0,
  examples: 3,
  ...over,
})

/**
 * **The ids are deliberately 41 and 58, not 1 and 2.**
 *
 * `Decided`, `Override.first` and `Override.wins` are all *positions* in the
 * list the request carried, never rule ids — the boundary the store holds
 * because an unsaved rule carries `id: 0` and so cannot be told apart from a
 * second unsaved one. The tests below assert this component renders
 * `index + 1`. With ids of 1 and 2 at indices 0 and 1, `id` and `index + 1`
 * are the *same number*, so a component rendering the id would draw exactly
 * what a component rendering the position draws and every assertion would
 * still pass. That was the shipped state until a hand-applied mutation showed
 * the test surviving it.
 */
const rules: Rule[] = [
  {
    id: 41,
    ordinal: 1,
    enabled: true,
    root_id: null,
    kind: 'regex',
    pattern: 'Manga \\(RAW\\)/.*',
    match_on: 'path',
    set_series: null,
    set_edition_key: 'raw',
    set_kind: null,
    set_language: null,
    note: null,
  },
  {
    id: 58,
    ordinal: 2,
    enabled: true,
    root_id: null,
    kind: 'regex',
    pattern: '.*Full Color$',
    match_on: 'path',
    set_series: null,
    set_edition_key: 'en-color',
    set_kind: null,
    set_language: null,
    note: null,
  },
]

const preview = (over: Partial<Preview> = {}): Preview => ({
  impact: impact(),
  rows: [row()],
  rows_truncated: false,
  overrides: [],
  ...over,
})

function render(props: Partial<Parameters<typeof PreviewTabs>[0]> = {}) {
  const onReorder = vi.fn()
  const onSelectRule = vi.fn()
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <PreviewTabs
        preview={preview()}
        rules={rules}
        selected={0}
        busy={false}
        saving={false}
        onReorder={onReorder}
        onSelectRule={onSelectRule}
        {...props}
      />,
    )
  })
  if (!host) throw new Error('nothing mounted')
  const click = (label: string) => {
    act(() => {
      ;[...host!.querySelectorAll<HTMLElement>('.preview-tabs button')]
        .find((b) => b.textContent?.includes(label))
        ?.click()
    })
  }
  return { host, onReorder, onSelectRule, click }
}

describe('PreviewTabs', () => {
  it('draws four tabs', () => {
    const { host } = render()
    const labels = [
      ...host.querySelectorAll<HTMLElement>('.preview-tabs button'),
    ].map((b) => b.textContent ?? '')
    expect(labels[0]).toContain('Impact')
    expect(labels[1]).toContain('Matched')
    expect(labels[2]).toContain('Provenance')
    expect(labels[3]).toContain('Overrides')
  })

  it('badges the override count on the tab, from every tab', () => {
    // C4 §1 describes override detection as a warning; the server half calls
    // it a tab. It is a tab *with a count badge drawn from every tab*, which
    // is what a warning-that-is-a-tab otherwise loses: an override hidden
    // behind a tab nobody clicked.
    const { host } = render({ preview: preview({ overrides: [anOverride()] }) })
    expect(host.querySelector('.preview-tabs .badge')?.textContent).toContain(
      '1',
    )
  })

  it('draws the impact in C4’s own words', () => {
    const { host } = render()
    expect(host.textContent).toContain('6 series merged')
    expect(host.textContent).toContain('71 works re-homed')
    expect(host.textContent).toContain('0 orphaned')
    expect(host.textContent).toContain('3 proposals reopened')
    expect(host.textContent).toContain('1 proposal raised')
  })

  it('never draws “proposals resolved”', () => {
    // A derivation cannot produce it — the build design's §5.5. Drawing it as
    // a zero would be an invented figure.
    const { host } = render()
    expect(host.textContent?.toLowerCase()).not.toContain('resolved')
  })

  it('lists a matched file as before → after', () => {
    const { host, click } = render()
    click('Matched')
    expect(host.textContent).toContain('Manga (RAW)/Vol 1.cbz')
    expect(host.textContent).toContain('Old')
    expect(host.textContent).toContain('New')
  })

  it('marks a near-miss as a near-miss rather than omitting it', () => {
    const { host, click } = render({
      preview: preview({ rows: [row({ matched: false })] }),
    })
    click('Matched')
    expect(host.textContent).toContain('near miss')
  })

  it('says the table was capped, rather than stopping silently', () => {
    const { host, click } = render({
      preview: preview({ rows_truncated: true }),
    })
    click('Matched')
    expect(host.textContent).toContain('500')
  })

  it('names the deciding rule by its position, not its id', () => {
    // `Decided` is an index into the list the request carried. Rendering
    // `rules[index].id` would name a different thing, and for an unsaved rule
    // it would name `0`.
    const { host, click } = render()
    click('Provenance')
    expect(host.textContent).toContain('rule 1')
    expect(host.textContent).toContain('root default')
  })

  it('reads the same rows the matched tab does', () => {
    const { host, click } = render({ preview: preview({ rows: [] }) })
    click('Provenance')
    expect(host.textContent).toContain('No files')
  })

  it('states which rule overrides which, and on how many files', () => {
    const { host, click } = render({
      preview: preview({ overrides: [anOverride()] }),
    })
    click('Overrides')
    expect(host.textContent).toContain('edition_key')
    expect(host.textContent).toContain('Full Color')
    // **The order of the two rule numbers is the whole sentence** — O95. The
    // winner overrides the loser, and a row that read "rule 1 overrides rule
    // 2" would name the correct pair while describing the opposite thing.
    expect(host.textContent).toContain('rule 2 overrides rule 1')
    expect(host.textContent).toContain('3 files')
  })

  it('draws the two values it chose between, each beside its own rule', () => {
    // **The reason the row can be read at all** — O95. Naming the rules says
    // *that* they disagreed; only the values say what about, and the whole
    // judgement the tab now asks for is whether this layering is deliberate.
    //
    // **Asserted per line, because the pairing is the payload and a whole-page
    // `toContain` cannot see it.** The first version of this checked
    // `textContent` for `→ raw` and `→ en-color`, and **passed with the two
    // values transposed** — all 793 jsdom tests did. A transposed row reads
    // "rule 1 `^(?<series>[^/]+)/` → Chainsaw Man / rule 4
    // `^(?<series>.+?)[ _-]+v` → Chainsaw Man - Digital Colored Comics
    // (wins)", so the winner looks like the rule that *keeps* the edition
    // suffix — and a reader then reorders, or edits, against the wrong rule.
    // That is the exact wrong-remedy failure O95 exists to close, reintroduced
    // one level down and invisible to reading. Found by this branch's
    // whole-branch review.
    const { host, click } = render({
      preview: preview({ overrides: [anOverride()] }),
    })
    click('Overrides')
    const lines = [...host.querySelectorAll<HTMLElement>('.values li')].map(
      (line) => line.textContent ?? '',
    )
    expect(lines.length).toBe(2)
    // The loser's line: its own number, its own pattern, its own value — and
    // *not* the winner's value, which is what a transposition puts here.
    expect(lines[0]).toContain('rule 1')
    expect(lines[0]).toContain('Manga')
    expect(lines[0]).toContain('→ raw')
    expect(lines[0]).not.toContain('en-color')
    // The winner's line, and it is the one carrying the mark.
    expect(lines[1]).toContain('rule 2')
    expect(lines[1]).toContain('Full Color')
    expect(lines[1]).toContain('→ en-color')
    expect(lines[1]).toContain('(wins)')
  })

  it('says a value is unset rather than drawing a blank', () => {
    // A later rule *clearing* what an earlier one set is a real half of a
    // disagreement, and `String(null)` in that slot reads as "null" while a
    // bare falsy check reads as nothing at all.
    const { host, click } = render({
      preview: preview({ overrides: [anOverride({ wins_value: null })] }),
    })
    click('Overrides')
    // On the *winner's* line, for the reason the per-line assertions above
    // exist: a whole-page `toContain` cannot tell "the winner cleared this
    // field" from "the loser did", and those are opposite readings of the
    // same row.
    const lines = [...host.querySelectorAll<HTMLElement>('.values li')].map(
      (line) => line.textContent ?? '',
    )
    expect(lines[1]).toContain('unset')
    expect(lines[0]).not.toContain('unset')
  })

  it('offers reorder and editing the winner as two options, not one fix', () => {
    // **O95.** The single button this row used to carry read "Move rule 4
    // above rule 1", and on the shipped seeded defaults pressing it stops a
    // colour edition folding into its main series. Two options under a
    // neutral lead is what reads right for a deliberate layering and for a
    // genuine mistake both.
    const { host, click, onReorder, onSelectRule } = render({
      preview: preview({ overrides: [anOverride()] }),
    })
    click('Overrides')
    expect(host.textContent).toContain('Options:')
    act(() => {
      host.querySelector<HTMLElement>('button.reorder')?.click()
    })
    expect(onReorder).toHaveBeenCalledWith(1, 0)
    act(() => {
      host.querySelector<HTMLElement>('button.edit-winner')?.click()
    })
    // The *winner*, which is the rule to narrow — not the loser and not
    // whichever rule happens to be selected.
    expect(onSelectRule).toHaveBeenCalledWith(1)
  })

  it('names a rule with no pattern yet rather than leaving a blank', () => {
    // **`blankRule()` leaves `pattern` as `''`, and an empty pattern matches
    // everything** — so a freshly added rule is one of the likeliest halves of
    // a real override, and a `??` fallback (which cannot see an empty string)
    // left the pattern slot blank in the one place whose job is telling the
    // two rules apart.
    const blank = { ...rules[0], id: 0, pattern: '' }
    const { host, click } = render({
      rules: [blank, rules[1]],
      preview: preview({ overrides: [anOverride()] }),
    })
    click('Overrides')
    expect(host.textContent).toContain('(no pattern yet)')
    // Not merely that the fallback appears somewhere: that no pattern slot on
    // the row is empty. A `??` renders an empty `<code>` element, which
    // `toContain` cannot see at all.
    const patterns = [...host.querySelectorAll<HTMLElement>('.values code')]
    expect(patterns.length).toBe(2)
    for (const shown of patterns) {
      expect(shown.textContent).not.toBe('')
    }
  })

  it('names a builtin rule by its kind rather than calling it unfinished', () => {
    // **`builtin-volume` and `builtin-chapter` are seeded with `pattern = ''`
    // by design** — migration 3, `003_scanner.sql` — so the `||` fallback that
    // is right for `blankRule()` told a reader a shipped rule was unfinished.
    // It is the likeliest numeric row there is: a hand-written volume regex
    // disagreeing with seeded `builtin-volume` is exactly the chain
    // `preview::tests::the_numeric_fields_report_their_own_values_and_not_each_others`
    // builds. Found by this branch's whole-branch review.
    const builtin = { ...rules[0], kind: 'builtin-volume', pattern: '' }
    const { host, click } = render({
      rules: [builtin, rules[1]],
      preview: preview({ overrides: [anOverride({ field: 'volume' })] }),
    })
    click('Overrides')
    expect(host.textContent).toContain('builtin-volume')
    expect(host.textContent).not.toContain('no pattern yet')
  })

  it.each([
    ['a preview', { busy: true, saving: false }],
    ['a save', { busy: false, saving: true }],
  ])('offers neither option while %s is in flight', (_what, flags) => {
    // The row is drawn from the *previous* preview, so its indices need not
    // still describe `rules` — a delete followed by a click inside either
    // window sends an out-of-range index the route refuses, or splices
    // `undefined` into the list through `moved()`.
    //
    // **Both windows, because they are not the same window.** `save()` sets
    // `saving` and never `busy`, so a version of this test that only checked
    // `busy` passed while a save left both buttons live for seconds on a real
    // library — and a reorder pressed inside that window is applied and then
    // silently discarded by the save's own `setRules`. Found by the second
    // whole-branch review of this branch.
    const { host, click } = render({
      preview: preview({ overrides: [anOverride()] }),
      ...flags,
    })
    click('Overrides')
    expect(
      host.querySelector<HTMLButtonElement>('button.reorder')?.disabled,
    ).toBe(true)
    expect(
      host.querySelector<HTMLButtonElement>('button.edit-winner')?.disabled,
    ).toBe(true)
  })

  it('does not say there is no preview while one is in flight', () => {
    // Both are reachable at once on the screen's first load, and together they
    // contradict: "Previewing…" above "No preview yet. Pick a rule, or edit
    // one" tells someone to start what has already started.
    const { host } = render({ preview: undefined, busy: true })
    expect(host.textContent).toContain('Previewing')
    expect(host.textContent).not.toContain('No preview yet')
  })

  it('says how many other rules disagree, when three or more do', () => {
    const { host, click } = render({
      preview: preview({ overrides: [anOverride({ others: 2 })] }),
    })
    click('Overrides')
    // **The whole sentence, and the element it lives in.** `others` is a
    // maximum across every entry sharing the row's triple, so the copy must
    // not claim it of the one file the row names. Asserting the exact text of
    // `.others` also catches the count being drawn twice: `plural()` prepends
    // the number itself, so routing this through it rendered "2 2 other rules
    // disagree" — which a `toContain('2 other rules disagree')` passes.
    expect(host.querySelector('.others')?.textContent).toBe(
      'In at least one of these files, 2 other rules disagree too.',
    )
  })

  it('says nothing extra in the ordinary two-rule case', () => {
    const { host, click } = render({
      preview: preview({ overrides: [anOverride({ others: 0 })] }),
    })
    click('Overrides')
    expect(host.textContent).not.toContain('disagree')
  })

  it('says it is working rather than drawing a stale answer', () => {
    const { host } = render({ busy: true })
    expect(host.textContent).toContain('Previewing')
  })

  it('says there is nothing yet before the first preview', () => {
    const { host } = render({ preview: undefined })
    expect(host.textContent).toContain('No preview yet')
  })
})

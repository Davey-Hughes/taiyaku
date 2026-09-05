import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { PreviewTabs } from './PreviewTabs'
import type { Conflict, Impact, Preview, Row, Rule } from './rules'
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

const conflict = (over: Partial<Conflict> = {}): Conflict => ({
  field: 'edition_key',
  rel_path: 'Manga (RAW)/Go-Toubun no Hanayome - Full Color',
  first: 0,
  wins: 1,
  others: 0,
  examples: 3,
  ...over,
})

/**
 * **The ids are deliberately 41 and 58, not 1 and 2.**
 *
 * `Decided`, `Conflict.first` and `Conflict.wins` are all *positions* in the
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
  conflicts: [],
  ...over,
})

function render(props: Partial<Parameters<typeof PreviewTabs>[0]> = {}) {
  const onReorder = vi.fn()
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <PreviewTabs
        preview={preview()}
        rules={rules}
        selected={0}
        busy={false}
        onReorder={onReorder}
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
  return { host, onReorder, click }
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
    expect(labels[3]).toContain('Conflicts')
  })

  it('badges the conflict count on the tab, from every tab', () => {
    // C4 §1 describes conflict detection as a warning; the server half calls
    // it a tab. It is a tab *with a count badge drawn from every tab*, which
    // is what a warning-that-is-a-tab otherwise loses: a conflict hidden
    // behind a tab nobody clicked.
    const { host } = render({ preview: preview({ conflicts: [conflict()] }) })
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

  it('names both rules of a conflict and offers reorder', () => {
    const { host, click, onReorder } = render({
      preview: preview({ conflicts: [conflict()] }),
    })
    click('Conflicts')
    expect(host.textContent).toContain('edition_key')
    expect(host.textContent).toContain('Full Color')
    act(() => {
      host.querySelector<HTMLElement>('button.reorder')?.click()
    })
    expect(onReorder).toHaveBeenCalledWith(1, 0)
  })

  it('names a rule with no pattern yet rather than leaving a blank', () => {
    // **`blankRule()` leaves `pattern` as `''`, and an empty pattern matches
    // everything** — so a freshly added rule is one of the likeliest halves of
    // a real conflict, and a `??` fallback (which cannot see an empty string)
    // rendered "rule 1 says , rule 2 wins". The blank sits in the exact
    // sentence whose job is telling the two rules apart.
    const blank = { ...rules[0], id: 0, pattern: '' }
    const { host, click } = render({
      rules: [blank, rules[1]],
      preview: preview({ conflicts: [conflict()] }),
    })
    click('Conflicts')
    expect(host.textContent).toContain('(no pattern yet)')
    expect(host.textContent).not.toContain('says ,')
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
      preview: preview({ conflicts: [conflict({ others: 2 })] }),
    })
    click('Conflicts')
    expect(host.textContent).toContain('2 others')
  })

  it('says nothing extra in the ordinary two-rule case', () => {
    const { host, click } = render({
      preview: preview({ conflicts: [conflict({ others: 0 })] }),
    })
    click('Conflicts')
    expect(host.textContent).not.toContain('others')
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

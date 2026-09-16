import { describe, expect, it } from 'vitest'
import type { Derived, FinishedScan, Walk } from './config'
import { ScanReport } from './ScanReport'
import { mount } from './testing/mount'

const walk = (over: Partial<Walk> = {}): Walk => ({
  root: '/srv/manga',
  dirs: 156,
  files: 1858,
  added: 3,
  updated: 0,
  gone: 0,
  refused: null,
  unreadable_dirs: [],
  excluded_dirs: [],
  excluded_files: 0,
  bad_exclude_patterns: [],
  forgotten_by_exclusion: 0,
  ...over,
})

const derived = (over: Partial<Derived> = {}): Derived => ({
  series: 130,
  works: 1858,
  editions: 1883,
  members: 1883,
  unattached_chapter_dirs: 0,
  unreadable: 0,
  suppressed: 0,
  rekeyed: 0,
  kept_editions: 0,
  emptied_works: 0,
  unfolded_works: 0,
  unfoldable: [],
  unreadable_hand: [],
  repaired_documents: 0,
  unwritable_documents: [],
  orphan_documents: [],
  declined_documents: [],
  stale_documents: [],
  moved_positions: 0,
  lost_positions: 0,
  moved_notes: 0,
  dropped_notes: 0,
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
  uncarried_alignments: 0,
  series_holding_no_work: 0,
  merged: 0,
  unrecognised_kinds: 0,
  raised: 0,
  root_removed_mid_derive: 0,
  ...over,
})

const finished = (over: Partial<FinishedScan> = {}): FinishedScan => ({
  scan: walk(),
  derive: derived(),
  note: "this process's own /api/works list is a startup snapshot",
  ...over,
})

describe('ScanReport', () => {
  it('draws an empty unreadable list as zero rather than omitting it', () => {
    // C4 §3's whole discipline. "Nothing unreadable this time" and "this
    // field does not exist in this version" must not look the same, and a
    // missing row is how they stop looking different.
    const host = mount(<ScanReport outcome={finished()} />)
    const row = host.querySelector('[data-field="unreadable"]')
    expect(row).not.toBeNull()
    expect(row?.textContent).toContain('0')
  })

  it('draws every field of both reports', () => {
    // Forty-three fields: twelve from the walk, thirty-one from the derive.
    // Counted rather than spot-checked, because the failure this guards is a
    // field quietly dropped on the way out — which is exactly what happened
    // once already on the server side.
    const host = mount(<ScanReport outcome={finished()} />)
    expect(host.querySelectorAll('[data-field]')).toHaveLength(43)
  })

  it('draws alignments a fold left behind apart from rows it discarded', () => {
    // O76: a fold carries an alignment now, and the one it cannot carry is
    // not a discarded row. Per element, with a different number in each, so a
    // label paired with the other row's count fails.
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({ discarded_rows: 4, uncarried_alignments: 9 }),
        })}
      />,
    )
    for (const [field, label, n] of [
      ['discarded_rows', 'Rows discarded', '4'],
      ['uncarried_alignments', 'Alignments not carried', '9'],
    ]) {
      const row = host.querySelector(`[data-field="${field}"]`)
      expect(row, `${field} must be drawn`).not.toBeNull()
      expect(row?.querySelector('dt')?.textContent).toBe(label)
      expect(row?.querySelector('dd')?.textContent).toBe(n)
    }
  })

  it('draws identical notes dropped apart from notes moved', () => {
    // O78: a fold drops a note copy the surviving work already holds
    // identically, and that is not a move. Per element, with a different
    // number in each, so a label paired with the other row's count fails.
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({ moved_notes: 3, dropped_notes: 7 }),
        })}
      />,
    )
    for (const [field, label, n] of [
      ['moved_notes', 'Notes moved', '3'],
      ['dropped_notes', 'Identical notes dropped', '7'],
    ]) {
      const row = host.querySelector(`[data-field="${field}"]`)
      expect(row, `${field} must be drawn`).not.toBeNull()
      expect(row?.querySelector('dt')?.textContent).toBe(label)
      expect(row?.querySelector('dd')?.textContent).toBe(n)
    }
  })

  it('puts what the scan did not do before what it did', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    const fields = [...host.querySelectorAll('[data-field]')].map((el) =>
      el.getAttribute('data-field'),
    )
    expect(fields.indexOf('unreadable')).toBeLessThan(fields.indexOf('works'))
    expect(fields.indexOf('suppressed')).toBeLessThan(fields.indexOf('series'))
  })

  it('puts what the walk did not do before what it did', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    const fields = [...host.querySelectorAll('[data-field]')].map((el) =>
      el.getAttribute('data-field'),
    )

    // refused sits between the two groups and is neither, so it is left out
    // of both lists rather than forced onto one side.
    const didNotDo = [
      'unreadable_dirs',
      'excluded_dirs',
      'bad_exclude_patterns',
      'excluded_files',
      'forgotten_by_exclusion',
    ]
    const achievement = ['dirs', 'files', 'added', 'updated', 'gone']

    for (const before of didNotDo) {
      for (const after of achievement) {
        // `indexOf` answers -1 for a field this render dropped entirely, and
        // -1 < anything holds — so a confession row removed outright used to
        // pass this same comparison. Both indices must actually be present
        // before their order means anything.
        expect(
          fields.indexOf(before),
          `${before} must be rendered`,
        ).toBeGreaterThanOrEqual(0)
        expect(
          fields.indexOf(after),
          `${after} must be rendered`,
        ).toBeGreaterThanOrEqual(0)
        expect(
          fields.indexOf(before),
          `${before} must precede ${after}`,
        ).toBeLessThan(fields.indexOf(after))
      }
    }
  })

  it('names an unreadable directory rather than a count alone', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          scan: walk({ unreadable_dirs: ['/srv/manga/broken'] }),
        })}
      />,
    )
    expect(host.textContent).toContain('/srv/manga/broken')
  })

  it("draws spec B's refusal as a refusal, not as a count", () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          scan: walk({
            refused: 'the root held 1858 entries last scan and 0 now',
          }),
        })}
      />,
    )
    const refusal = host.querySelector('[role="alert"]')
    expect(refusal?.textContent).toContain('0 now')
  })

  it('names the work an orphaned document was absorbed into', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            orphan_documents: [
              {
                path: '/srv/manga/.taiyaku/x.library.json',
                title: 'Monster',
                absorbed_into: { slug: 'monster-01', title: 'Monster v1' },
              },
            ],
          }),
        })}
      />,
    )
    expect(host.textContent).toContain('Monster v1')
  })

  it('draws the note about /api/works being a startup snapshot', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    expect(host.textContent).toContain('startup snapshot')
  })

  it('draws a failed scan as the failure it is', () => {
    const host = mount(<ScanReport outcome={{ error: 'the scan panicked' }} />)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'the scan panicked',
    )
    expect(host.querySelectorAll('[data-field]')).toHaveLength(0)
  })

  it('lists a declined document, which is not an orphan', async () => {
    // #104's third category: a root that stopped keeping documents reports
    // the file rather than deleting it, and rather than calling it orphaned.
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            declined_documents: [
              { path: '/srv/manga/.taiyaku/y.library.json', title: 'Pluto' },
            ],
          }),
        })}
      />,
    )
    const row = host.querySelector('[data-field="declined_documents"]')
    expect(row?.textContent).toContain('/srv/manga/.taiyaku/y.library.json')
    expect(row?.textContent).toContain('Pluto')
  })

  it('draws an orphan that nothing absorbed, and one with no title', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            orphan_documents: [
              {
                path: '/srv/manga/.taiyaku/z.library.json',
                title: null,
                absorbed_into: null,
              },
            ],
            declined_documents: [
              { path: '/srv/manga/.taiyaku/w.library.json', title: null },
            ],
          }),
        })}
      />,
    )
    expect(host.textContent).toContain('z.library.json')
    expect(host.textContent).not.toContain('absorbed into')
  })

  it('lists the paths behind every path-valued field, not just their counts', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            unfoldable: ['a-fold'],
            unreadable_hand: ['bad.json'],
            unwritable_documents: ['readonly.json'],
            stale_documents: ['stale.json'],
          }),
        })}
      />,
    )
    for (const path of ['a-fold', 'bad.json', 'readonly.json', 'stale.json']) {
      expect(host.textContent).toContain(path)
    }
  })

  it('draws all four exclusion fields when nothing was excluded', () => {
    const host = mount(<ScanReport outcome={finished()} />)

    // Per element, never a whole-page toContain: a page-level match cannot
    // tell a zero beside the right label from one beside the wrong one.
    for (const [field, label] of [
      ['excluded_dirs', 'Directories excluded'],
      ['excluded_files', 'Files excluded'],
      ['forgotten_by_exclusion', 'Forgotten because excluded'],
      ['bad_exclude_patterns', 'Unusable exclude patterns'],
    ]) {
      const row = host.querySelector(`[data-field="${field}"]`)
      expect(row, `${field} must be drawn`).not.toBeNull()
      expect(row?.querySelector('dt')?.textContent).toBe(label)
      expect(row?.querySelector('dd')?.textContent).toContain('0')
    }
  })

  it('names each excluded directory and each unusable pattern', () => {
    const host = mount(
      <ScanReport
        outcome={finished({
          scan: walk({
            excluded_dirs: ['Naruto/extras', 'Bleach/scans'],
            excluded_files: 12,
            forgotten_by_exclusion: 40,
            bad_exclude_patterns: ['`\\`: dangling'],
          }),
        })}
      />,
    )

    const dirs = host.querySelector('[data-field="excluded_dirs"]')
    expect(dirs?.querySelectorAll('li')).toHaveLength(2)
    expect(dirs?.textContent).toContain('Naruto/extras')
    expect(dirs?.textContent).toContain('Bleach/scans')

    expect(
      host.querySelector('[data-field="excluded_files"] dd')?.textContent,
    ).toBe('12')
    expect(
      host.querySelector('[data-field="forgotten_by_exclusion"] dd')
        ?.textContent,
    ).toBe('40')
    expect(
      host.querySelector('[data-field="bad_exclude_patterns"]')?.textContent,
    ).toContain('dangling')
  })
})

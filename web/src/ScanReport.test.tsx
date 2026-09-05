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
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
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
    // Thirty-seven fields: eight from the walk, twenty-nine from the derive.
    // Counted rather than spot-checked, because the failure this guards is a
    // field quietly dropped on the way out — which is exactly what happened
    // once already on the server side.
    const host = mount(<ScanReport outcome={finished()} />)
    expect(host.querySelectorAll('[data-field]')).toHaveLength(37)
  })

  it('puts what the scan did not do before what it did', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    const fields = [...host.querySelectorAll('[data-field]')].map((el) =>
      el.getAttribute('data-field'),
    )
    expect(fields.indexOf('unreadable')).toBeLessThan(fields.indexOf('works'))
    expect(fields.indexOf('suppressed')).toBeLessThan(fields.indexOf('series'))
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
})

import { describe, expect, it } from 'vitest'
import type { Derived, FinishedScan, Walk } from '../src/config'
import { ScanReport } from '../src/ScanReport'
import { mount } from './support/mount'

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
  taken_over: 0,
  ...over,
})

const derived = (over: Partial<Derived> = {}): Derived => ({
  series: 130,
  works: 1858,
  editions: 1883,
  members: 1883,
  unattached_chapter_dirs: 0,
  unreadable: {
    total: 0,
    refused_format: 0,
    by_format: {},
    uncounted: [],
    vanished: [],
    no_pages: 0,
  },
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
  marked_missing: 0,
  found_again: 0,
  joined: 0,
  followed: 0,
  unwritten_alignments: 0,
  moved_editions: 0,
  unmoved_editions: 0,
  ...over,
})

const finished = (over: Partial<FinishedScan> = {}): FinishedScan => ({
  scan: walk(),
  derive: derived(),
  ...over,
})

describe('ScanReport', () => {
  it('heads the derive block as covering every library', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    expect(
      host.querySelector('.derived h3, .derived h4, .derived h5')?.textContent,
    ).toBe('What derivation made of every library')
  })

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
    // Fifty-four: twelve from the walk, thirty-eight from the derive, and the
    // four causes O90 draws inside `unreadable`. The walk's thirteenth,
    // `taken_over`, is drawn only when it is not zero, which it is not here.
    // Counted rather than spot-checked, because the failure this guards is a
    // field quietly dropped on the way out.
    const host = mount(<ScanReport outcome={finished()} />)
    expect(host.querySelectorAll('[data-field]')).toHaveLength(54)
  })

  it('draws Taken over beside Gone when the walk took rows over, and not at all when it took none', () => {
    const took = mount(
      <ScanReport outcome={finished({ scan: walk({ taken_over: 4 }) })} />,
    )
    const fields = [...took.querySelectorAll('.walk [data-field]')].map((el) =>
      el.getAttribute('data-field'),
    )
    expect(fields[fields.indexOf('gone') + 1]).toBe('taken_over')
    const row = took.querySelector('[data-field="taken_over"]')
    expect(row?.querySelector('dt')?.textContent).toBe('Taken over')
    expect(row?.querySelector('dd')?.textContent).toBe('4')
    expect(took.querySelectorAll('[data-field]')).toHaveLength(55)

    const none = mount(<ScanReport outcome={finished()} />)
    expect(none.querySelector('[data-field="taken_over"]')).toBeNull()
  })

  it('says why each unreadable entry was skipped, cause by cause', () => {
    // O90. Per element, with a different number in every cause, so a label
    // paired with another cause's count, or a path drawn under the other
    // list, fails here where a whole-page `toContain` would pass.
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            unreadable: {
              total: 29,
              refused_format: 15,
              by_format: { '7z': 3, rar: 12 },
              uncounted: ['/srv/manga/Bisque Doll/Bisque Doll.cbz'],
              vanished: ['/srv/manga/Gone - v01', '/srv/manga/Gone - v02'],
              no_pages: 11,
            },
          }),
        })}
      />,
    )
    const row = host.querySelector('[data-field="unreadable"]')
    // The non-zero side of the wrapper's own `data-zero`: `finished()`'s
    // default (used by the broad sweep below) covers `total: 0` only, so
    // this fixture is what proves the wrapper stays unmarked when its total
    // is not zero, not merely that it can be marked.
    expect(row?.hasAttribute('data-zero')).toBe(false)
    expect(row?.querySelector(':scope > dt')?.textContent).toBe('Unreadable')
    expect(row?.querySelector(':scope > dd')?.firstChild?.textContent).toBe(
      '29',
    )

    const formats = row?.querySelector('[data-field="unreadable.by_format"]')
    expect(formats?.querySelector('dt')?.textContent).toBe(
      'In a format taiyaku does not read',
    )
    expect(formats?.querySelector('dd')?.firstChild?.textContent).toBe('15')
    expect(
      [...(formats?.querySelectorAll('li') ?? [])].map((li) => [
        li.querySelector('code')?.textContent,
        li.textContent,
      ]),
    ).toEqual([
      ['7z', '7z 3'],
      ['rar', 'rar 12'],
    ])

    for (const [field, label, paths] of [
      [
        'unreadable.uncounted',
        'Pages could not be counted',
        ['/srv/manga/Bisque Doll/Bisque Doll.cbz'],
      ],
      [
        'unreadable.vanished',
        'Gone before they could be hashed',
        ['/srv/manga/Gone - v01', '/srv/manga/Gone - v02'],
      ],
    ] as const) {
      const cause = row?.querySelector(`[data-field="${field}"]`)
      expect(cause, `${field} must be drawn`).not.toBeNull()
      expect(cause?.querySelector('dt')?.textContent).toBe(label)
      expect(cause?.querySelector('dd')?.firstChild?.textContent).toBe(
        String(paths.length),
      )
      expect(
        [...(cause?.querySelectorAll('li code') ?? [])].map(
          (code) => code.textContent,
        ),
      ).toEqual(paths)
    }

    const noPages = row?.querySelector('[data-field="unreadable.no_pages"]')
    expect(noPages?.querySelector('dt')?.textContent).toBe(
      'Holding no page images',
    )
    expect(noPages?.querySelector('dd')?.textContent).toBe('11')
  })

  it('draws every unreadable cause at zero rather than omitting it', () => {
    const host = mount(<ScanReport outcome={finished()} />)
    for (const field of [
      'unreadable.by_format',
      'unreadable.uncounted',
      'unreadable.vanished',
      'unreadable.no_pages',
    ]) {
      const cause = host.querySelector(`[data-field="${field}"]`)
      expect(cause, `${field} must be drawn`).not.toBeNull()
      expect(cause?.querySelector('dd')?.textContent).toBe('0')
    }
  })

  it('draws alignments a fold left behind apart from rows it discarded', () => {
    // O76: a fold carries an alignment, and the one it cannot carry is not a
    // discarded row. Per element, with a different number in each, so a label
    // paired with the other row's count fails.
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
        // -1 < anything holds — so a confession row removed outright would
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

  it('draws a failed scan as the failure it is', () => {
    const host = mount(<ScanReport outcome={{ error: 'the scan panicked' }} />)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'the scan panicked',
    )
    expect(host.querySelectorAll('[data-field]')).toHaveLength(0)
  })

  it('draws a cancelled scan as the step it stopped in, with no counts and no alert', () => {
    const host = mount(
      <ScanReport outcome={{ cancelled: { step: 'resolving' } }} />,
    )
    expect(
      [...host.querySelectorAll('.scan-report > p')].map((p) => [
        p.textContent,
        p.getAttribute('role'),
      ]),
    ).toEqual([['Cancelled while reading files', null]])
    expect(host.querySelectorAll('[data-field]')).toHaveLength(0)
  })

  it('lists a declined document, which is not an orphan', async () => {
    // A root that stopped keeping documents reports the file rather than
    // deleting it, and rather than calling it orphaned.
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

  it('names what each directory field counts: directories walked, and members whose directory was removed mid-derive', () => {
    // Each label says what its field counts. `dirs` is every directory the
    // walk recorded, and `root_removed_mid_derive` counts members, not
    // directories: `write_roots` adds one for each member whose directory
    // was removed while the derive ran. Each label is read from its own
    // field's row, so a label cannot pass beside another field's count;
    // compared as one object so every wrong label is named at once.
    const host = mount(<ScanReport outcome={finished()} />)
    const fields = [
      'unreadable_dirs',
      'excluded_dirs',
      'dirs',
      'root',
      'root_removed_mid_derive',
    ]
    const labels = Object.fromEntries(
      fields.map((field) => [
        field,
        host.querySelector(`[data-field="${field}"] > dt`)?.textContent,
      ]),
    )

    expect(labels).toEqual({
      unreadable_dirs: 'Directories it could not read',
      excluded_dirs: 'Directories excluded',
      dirs: 'Directories walked',
      root: 'Directory',
      root_removed_mid_derive: 'Members whose directory was removed mid-derive',
    })
  })

  it('counts the works a derive marked missing among what it did not do, and those found again among what it did', () => {
    // O91. Each label is read from its own field's row, so swapping the two
    // labels fails; the zero is marked and the count beside it drawn.
    const host = mount(
      <ScanReport
        outcome={finished({ derive: derived({ found_again: 2 }) })}
      />,
    )
    const rows = ['marked_missing', 'found_again'].map((field) => {
      const row = host.querySelector(`[data-field="${field}"]`)
      return [
        field,
        row?.querySelector('dt')?.textContent,
        row?.querySelector('dd')?.textContent,
        row?.getAttribute('data-zero'),
      ]
    })
    expect(rows).toEqual([
      ['marked_missing', 'Works marked missing', '0', ''],
      ['found_again', 'Missing works found again', '2', null],
    ])
    const fields = [...host.querySelectorAll('.derived [data-field]')].map(
      (el) => el.getAttribute('data-field'),
    )
    // Against a row inside each group, not the boundary: the DOM marks no
    // boundary, so a row first among what it did sits where the last of what
    // it did not do would.
    expect(fields.indexOf('marked_missing')).toBeLessThan(
      fields.indexOf('declined_documents'),
    )
    expect(fields.indexOf('found_again')).toBeGreaterThan(
      fields.indexOf('series'),
    )
  })

  it('draws each count of a derive that joins volumes by content under its own label, on its own side', () => {
    // Each field its own number, so a row drawing another's count, or a
    // label paired with another's row, fails. The sides are read against a
    // row inside each group, as above, since the DOM marks no boundary.
    const host = mount(
      <ScanReport
        outcome={finished({
          derive: derived({
            unwritten_alignments: 3,
            unmoved_editions: 5,
            joined: 7,
            followed: 11,
            moved_editions: 13,
          }),
        })}
      />,
    )
    const rows = [
      'unwritten_alignments',
      'unmoved_editions',
      'joined',
      'followed',
      'moved_editions',
    ].map((field) => {
      const row = host.querySelector(`[data-field="${field}"]`)
      return [
        field,
        row?.querySelector('dt')?.textContent,
        row?.querySelector('dd')?.textContent,
      ]
    })
    expect(rows).toEqual([
      [
        'unwritten_alignments',
        'Alignments not written for volumes joined by content',
        '3',
      ],
      ['unmoved_editions', 'Editions whose move was refused', '5'],
      ['joined', 'Volumes joined by content', '7'],
      ['followed', 'Works moved with their directory', '11'],
      ['moved_editions', 'Editions moved to the work holding their file', '13'],
    ])
    const fields = [...host.querySelectorAll('.derived [data-field]')].map(
      (el) => el.getAttribute('data-field'),
    )
    const notDone = fields.indexOf('declined_documents')
    const done = fields.indexOf('series')
    expect(
      ['unwritten_alignments', 'unmoved_editions'].map(
        (field) => fields.indexOf(field) < notDone,
      ),
    ).toEqual([true, true])
    expect(
      ['joined', 'followed', 'moved_editions'].map(
        (field) => fields.indexOf(field) > done,
      ),
    ).toEqual([true, true, true])
  })

  it('marks a zero count and leaves a non-zero count unmarked', () => {
    // `finished()`'s default walk carries both: `added: 3` and `updated: 0`.
    // A fixture where every count is zero would prove only that the
    // attribute can be written (O92).
    const host = mount(<ScanReport outcome={finished()} />)
    const zero = host.querySelector('[data-field="updated"]')
    const value = host.querySelector('[data-field="added"]')
    expect(zero?.getAttribute('data-zero')).toBe('')
    expect(value?.hasAttribute('data-zero')).toBe(false)
  })

  it('marks an empty path list and leaves a populated one unmarked', () => {
    // `unfoldable` is overridden to one entry; `unreadable_hand` stays at
    // its default `[]`. Both are drawn through the shared `Paths`
    // component, so this exercises the same code the zero rows do.
    const host = mount(
      <ScanReport
        outcome={finished({ derive: derived({ unfoldable: ['a-fold'] }) })}
      />,
    )
    const zero = host.querySelector('[data-field="unreadable_hand"]')
    const value = host.querySelector('[data-field="unfoldable"]')
    expect(zero?.getAttribute('data-zero')).toBe('')
    expect(value?.hasAttribute('data-zero')).toBe(false)
  })

  it('marks the refused row as zero when nothing was refused, and unmarked when it was', () => {
    const notRefused = mount(<ScanReport outcome={finished()} />)
    expect(
      notRefused
        .querySelector('[data-field="refused"]')
        ?.getAttribute('data-zero'),
    ).toBe('')

    const refused = mount(
      <ScanReport
        outcome={finished({
          scan: walk({
            refused: 'the root held 1858 entries last scan and 0 now',
          }),
        })}
      />,
    )
    expect(
      refused
        .querySelector('[data-field="refused"]')
        ?.hasAttribute('data-zero'),
    ).toBe(false)
  })

  it('marks every row whose value is zero, whatever component drew it', () => {
    // Asserted over every `[data-field]` rather than per component, because a
    // row hand-rolled rather than routed through `Count`/`Paths` would keep
    // full strength beside its zero neighbours — the same fact drawn two
    // ways — and no per-component test would be looking at it.
    //
    // The `<dd>`'s *first child node* is read, not its `textContent`. For a
    // wrapper like `unreadable` the `<dd>` also holds a nested `<dl>` of
    // causes, so `textContent` is the count concatenated with that whole
    // subtree — "0In a format taiyaku does not read0…" — and no regex
    // anchored on a word boundary can find the count inside it: `\b` needs a
    // word/non-word transition, and digit-to-letter is word-to-word. Such a
    // regex would fail at every possible value and silently skip the row via
    // `continue`. The first child node is the count alone in every component
    // that draws one, so reading it directly needs no regex.
    const host = mount(<ScanReport outcome={finished()} />)
    const wrong: string[] = []
    for (const row of host.querySelectorAll('[data-field]')) {
      const leading =
        row.querySelector('dd')?.firstChild?.textContent?.trim() ?? ''
      // Not a value row at all (e.g. `root`, whose `<dd>` holds a path) if
      // it is neither a bare number nor `no`/`yes`.
      const isNumber = /^\d+$/.test(leading)
      if (!isNumber && leading !== 'no' && leading !== 'yes') continue
      const isZero = leading === '0' || leading === 'no'
      const marked = row.hasAttribute('data-zero')
      if (isZero !== marked) {
        wrong.push(
          `${row.getAttribute('data-field')}: value ${leading}, marked ${marked}`,
        )
      }
    }
    expect(wrong).toEqual([])
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

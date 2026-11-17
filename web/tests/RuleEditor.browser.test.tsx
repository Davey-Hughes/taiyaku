import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives: the rules
// under test live in `styles/admin.css`, which reaches the app through
// `index.css`.
import '../src/index.css'
import * as backupApi from '../src/backup'
import * as config from '../src/config'
import { Config } from '../src/Config'
import * as curation from '../src/curation'
import * as derive from '../src/libraries'
import { WAIT_DRAWN_AFTER_MS } from '../src/hooks'
import { SIDEBAR_LAYOUT } from '../src/phoneLayout'
import { RuleEditor } from '../src/RuleEditor'
import * as rulesApi from '../src/rules'
import { deferred, settleUntil } from './support/async'
import { pressOn } from './support/callout'
import { tokenColour } from './support/colour'
import { emptyDerive } from './support/derive'
import { expectDisclosure } from './support/disclosure'
import { holdOf } from './support/held'
import { press } from './support/input'
import { lengthOf } from './support/length'
import { mount, rerender } from './support/mount'
import { coarseRulesOn } from './support/pointer'
import { rootFontAt } from './support/rootFont'
import { watchDrawn } from './support/wait'

/**
 * The report a save's re-derive draws on the Rules screen. `ScanReport`'s
 * section headings are styled by class for the Libraries screen, and this
 * screen draws the same report, so what it makes of that class is held here.
 */

vi.mock('../src/rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
  importRules: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  listRoots: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  deriveStatus: vi.fn(),
}))

const rule: rulesApi.Rule = {
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga/.*',
  match_on: 'path',
  set_series: 'Series',
  set_edition_key: null,
  set_kind: null,
  set_language: null,
  note: null,
}

const impact: rulesApi.Impact = {
  series_before: 8,
  series_after: 2,
  series_merged: 6,
  works_rehomed: 71,
  works_orphaned: 0,
  proposals_reopened: 0,
  proposals_raised: 0,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(rulesApi.listRules).mockResolvedValue({
    rules: [rule],
    complaints: [],
  })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
  vi.mocked(derive.deriveStatus).mockResolvedValue({
    derive_running: false,
    job_held: false,
  })
  vi.mocked(rulesApi.previewRules).mockResolvedValue({
    impact,
    rows: [],
    rows_truncated: false,
    overrides: [],
  })
  vi.mocked(rulesApi.saveRules).mockResolvedValue({
    impact,
    rules: [rule],
    reopened_proposals: [],
    complaints: [],
    derive: { ...emptyDerive, lost_positions: 2 },
  })
})

describe('the Rules screen’s save report', () => {
  it.each([390, 1280])(
    'draws the report’s heading at the body size, as a label for what follows it, at %ipx',
    async (width) => {
      await page.viewport(width, 844)
      const host = await act(async () => {
        const mounted = mount(
          <div className="library">
            <main>
              <RuleEditor />
            </main>
          </div>,
        )
        await Promise.resolve()
        await Promise.resolve()
        return mounted
      })
      await press(host.querySelector('button.save') as Element)
      const details = host.querySelector(
        'details.save-derive',
      ) as HTMLDetailsElement
      details.open = true
      await document.fonts.ready

      const heading = details.querySelector('.report-heading') as HTMLElement
      expect(heading.textContent).toBe('What derivation made of every library')
      const style = getComputedStyle(heading)
      // The body's size: never louder than the summary that opened it.
      expect(style.fontSize).toBe(
        getComputedStyle(details.querySelector('summary') as Element).fontSize,
      )
      expect(style.fontSize).toBe('16px')
      expect(style.fontWeight).toBe('600')
      // Its face is the screen's headings', not the body's.
      expect(style.fontFamily).toBe(
        getComputedStyle(
          host.querySelector('.rule-editor .page-title') as Element,
        ).fontFamily,
      )
      // And it stands clear of the summary above it and the list below.
      expect(
        heading.getBoundingClientRect().top -
          (
            details.querySelector('summary') as HTMLElement
          ).getBoundingClientRect().bottom,
      ).toBeGreaterThanOrEqual(16)
    },
  )

  it('draws what a save’s re-derive found as something to press, closed and open', async () => {
    await page.viewport(1280, 800)
    const host = await act(async () => {
      const mounted = mount(
        <div className="library">
          <div className="sidebar-column" />
          <main>
            <div className="config">
              <RuleEditor />
            </div>
          </main>
        </div>,
      )
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await press(host.querySelector('button.save') as Element)
    await expectDisclosure(
      host.querySelector('details.save-derive > summary') as HTMLElement,
      'the save’s re-derive',
    )
  })
})

/**
 * Side by side, each of Rules' boxes is as wide as what it holds,
 * all from where the other Configuration pages start, and the room past
 * them is on the right (the user's ruling of 2026-10-26): the list as wide
 * as its widest rule, up to a cap; the rule's pane as wide as its sentence,
 * or as its form; the preview as wide as the two together, and wider only
 * where a tab holds something wider, up to `main`'s width. Other sentences
 * wrap at the width the boxes set and give them none. `.config` defines no
 * settings measure, and one defined there would narrow the preview under
 * the panes, which the preview's case sees.
 */
describe('the Rules screen’s width', () => {
  /** The rules a new library is seeded with (`003_scanner.sql`). */
  const seeded: rulesApi.Rule[] = (
    [
      [
        'regex',
        '^(?<series>[^/]+)/',
        'path',
        'series from the first directory under the root',
      ],
      [
        'builtin-volume',
        '',
        'name',
        'volume from the name, via taiyaku_read::volume_number',
      ],
      [
        'builtin-chapter',
        '',
        'name',
        'chapter from the name, via taiyaku_read::chapter_number',
      ],
      [
        'regex',
        '^(?<series>.+?)[ _-]+v',
        'name',
        'series from the filename where it repeats it',
      ],
    ] as const
  ).map(([kind, pattern, match_on, note], i) => ({
    ...rule,
    id: i + 1,
    ordinal: i + 1,
    kind,
    pattern,
    match_on,
    set_series: null,
    note,
  }))

  /** Configuration at `entry` in the frame's `main`, as the shell draws it. */
  const framed = (entry: string) => (
    <div className="library" key={entry}>
      <div className="sidebar-column" />
      <main>
        <MemoryRouter initialEntries={[entry]}>
          <Routes>
            <Route path="/config/*" element={<Config />} />
          </Routes>
        </MemoryRouter>
      </main>
    </div>
  )

  /**
   * Rules, the window `width` wide, once what it reads has landed: the
   * fixture's one rule unless `rules` are given.
   */
  async function rulesAt(width: number, rules?: rulesApi.Rule[]) {
    if (rules !== undefined) {
      vi.mocked(rulesApi.listRules).mockResolvedValue({ rules, complaints: [] })
    }
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(width, 900)
    const host = await act(async () => {
      const mounted = mount(framed('/config/rules'))
      await Promise.resolve()
      await Promise.resolve()
      return mounted
    })
    await document.fonts.ready
    return host
  }

  /** Picks rule `index + 1`, as a click on its name does. */
  async function pick(host: HTMLElement, index: number) {
    await act(async () => {
      ;(
        host.querySelectorAll('.rule-row .select')[index] as HTMLElement
      ).click()
      await Promise.resolve()
      await Promise.resolve()
    })
  }

  /** The element `selector` finds in `host`, which must be there. */
  const find = (host: HTMLElement, selector: string) => {
    const el = host.querySelector<HTMLElement>(selector)
    if (el === null) throw new Error(`no ${selector}`)
    return el
  }

  /** The list's box, the rule's pane's box and the preview's box. */
  const boxesOf = (host: HTMLElement) => ({
    list: find(host, '.editor-panes > .settings-section:nth-child(1)'),
    pane: find(host, '.editor-panes > .settings-section:nth-child(2)'),
    preview: find(host, '.rule-editor > .settings-section:has(.preview)'),
  })

  /** How far a box's rows are in from its outer edge, on each side. */
  const insetOf = (box: Element) => lengthOf(box, '--settings-box-inset')

  /** How wide `el`'s words are drawn: on one line, as wide as they need. */
  const wordsWidth = (el: Element) => {
    const range = document.createRange()
    range.selectNodeContents(el)
    return range.getBoundingClientRect().width
  }

  /**
   * Where `el`'s words go onto another line: the offset in its text of each
   * character, other than a space, that is drawn lower than the one before
   * it. A space is left out, since one at a line's end may be drawn on
   * either line.
   */
  const lineStarts = (el: Element) => {
    const drawn: { at: number; top: number }[] = []
    let offset = 0
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.textContent ?? ''
      for (let i = 0; i < text.length; i++) {
        if (/\s/.test(text[i])) continue
        const one = document.createRange()
        one.setStart(node, i)
        one.setEnd(node, i + 1)
        const at = one.getClientRects()[0]
        if (at !== undefined) drawn.push({ at: offset + i, top: at.top })
      }
      offset += text.length
    }
    return drawn
      .filter((char, i) => i > 0 && char.top > drawn[i - 1].top + 1)
      .map((char) => char.at)
  }

  /** What a row draws beside its name: its edges, other parts and gaps. */
  const beside = (row: Element) => {
    const style = getComputedStyle(row)
    const parts = [...row.children]
    return (
      [
        style.paddingLeft,
        style.paddingRight,
        style.borderLeftWidth,
        style.borderRightWidth,
      ].reduce((sum, length) => sum + Number.parseFloat(length), 0) +
      (parts.length - 1) * Number.parseFloat(style.columnGap) +
      parts
        .filter((part) => !part.classList.contains('select'))
        .reduce((sum, part) => sum + part.getBoundingClientRect().width, 0)
    )
  }

  /**
   * How wide `control` is drawn on its own, as nothing else sizes it: a
   * copy, taken out of the flow beside it, in the same field.
   */
  const ownWidth = (control: HTMLElement) => {
    const copy = control.cloneNode(true) as HTMLElement
    copy.style.cssText = 'position: absolute; width: auto; visibility: hidden'
    control.after(copy)
    const width = copy.getBoundingClientRect().width
    copy.remove()
    return width
  }

  /**
   * `token`, a length, in px as `control` resolves it: a probe drawn that
   * wide beside it in its own font, since a `ch` is a width of that font.
   */
  const lengthIn = (control: HTMLElement, token: string) => {
    const probe = document.createElement('div')
    probe.style.font = getComputedStyle(control).font
    probe.style.width = `var(${token}, 0px)`
    control.after(probe)
    const width = probe.getBoundingClientRect().width
    probe.remove()
    return width
  }

  /** How wide `el` draws what it holds: inside its padding and border. */
  const contentWidthOf = (el: Element) => {
    const style = getComputedStyle(el)
    return [
      style.paddingLeft,
      style.paddingRight,
      style.borderLeftWidth,
      style.borderRightWidth,
    ].reduce(
      (width, length) => width - Number.parseFloat(length),
      el.getBoundingClientRect().width,
    )
  }

  /**
   * Where each of `names` goes onto another line: after a space or a `::`,
   * between its words, or inside a word.
   */
  const lineBreaksOf = (names: Element[]) =>
    names.flatMap((name, i) => {
      const text = name.textContent ?? ''
      return lineStarts(name).map((at) => {
        const before = text.slice(0, at)
        return {
          betweenWords: /\s$/.test(before) || before.endsWith('::'),
          where: `rule ${i + 1} broken at ${text.slice(at - 8, at)}|${text.slice(at, at + 8)}`,
        }
      })
    })

  /**
   * Checks that each rule's name (or each of `names`) goes onto another
   * line only after a space or a `::`, never inside a word, and gives how
   * many times the names go onto another line in all.
   */
  const expectNamesBrokenBetweenWords = (
    host: HTMLElement,
    names: Element[] = [...host.querySelectorAll('.rule-row .rule-name')],
  ) => {
    const breaks = lineBreaksOf(names)
    for (const { betweenWords, where } of breaks) {
      expect.soft(betweenWords, where).toBe(true)
    }
    return breaks.length
  }

  /**
   * Checks that the four boxes under Sets share a line, each as wide as
   * `--rule-sets-box` resolves in its own font, that the pane is as wide as
   * that line of boxes inside the fieldset's edge and the box's insets, and
   * that the note is whole in its box. The fieldset stretches to the form,
   * so the line is read from its first box's start to its last box's end.
   */
  const expectFormAtItsOwnWidth = (host: HTMLElement) => {
    const { pane } = boxesOf(host)
    const sets = [
      ...host.querySelectorAll<HTMLInputElement>('.rule-sets input'),
    ]
    expect(sets).toHaveLength(4)
    const top = sets[0].getBoundingClientRect().top
    for (const input of sets) {
      expect.soft(input.getBoundingClientRect().top, input.name).toBe(top)
      expect
        .soft(contentWidthOf(input), input.name)
        .toBeCloseTo(lengthIn(input, '--rule-sets-box'), 0)
    }
    const fieldset = find(host, '.rule-sets')
    const edge =
      fieldset.getBoundingClientRect().width - contentWidthOf(fieldset)
    expect(pane.getBoundingClientRect().width, 'the pane').toBeCloseTo(
      sets[3].getBoundingClientRect().right -
        sets[0].getBoundingClientRect().left +
        edge +
        2 * insetOf(pane),
      0,
    )
    const note = find(host, 'input[name="note"]') as HTMLInputElement
    expect(note.value).toBe(seeded[2].note)
    expect(note.scrollWidth, 'the note whole').toBeLessThanOrEqual(
      note.clientWidth,
    )
  }

  /** Shows the preview's tab `name`, as a click on it does. */
  async function showTab(host: HTMLElement, name: string) {
    await act(async () => {
      const tab = [...host.querySelectorAll<HTMLElement>('[role="tab"]')].find(
        (el) => el.textContent?.startsWith(name),
      )
      if (tab === undefined) throw new Error(`no ${name} tab`)
      tab.click()
      await Promise.resolve()
    })
  }

  /**
   * Where `main` draws its content: inside its edge, and short of a
   * scrollbar's gutter, which is the scroller's and not the page's.
   */
  const contentOf = (host: HTMLElement) => {
    const main = find(host, 'main')
    const start = main.getBoundingClientRect().left + main.clientLeft
    return { main, start, end: start + main.clientWidth }
  }

  /**
   * With no rule picked the pane holds a sentence, which wraps before the
   * list gives any width: at 1280 the list and the sentence on one line do
   * not both fit, and the list keeps its widest rule's width.
   */
  it.each([1920, 1280])(
    'draws the list as wide as its widest rule, each rule on one line, short of its cap, with no rule picked, at %ipx',
    async (width) => {
      const host = await rulesAt(width, seeded)
      const { list } = boxesOf(host)
      const rows = [...host.querySelectorAll('.rule-row')]
      expect(rows).toHaveLength(seeded.length)
      const widest = Math.max(
        ...rows.map((row, i) => {
          const words = [
            row.querySelector('.rule-name'),
            row.querySelector('.mechanism'),
          ].map((el) => {
            if (el === null) throw new Error(`rule ${i + 1} has no name`)
            expect.soft(lineStarts(el), `rule ${i + 1}’s words`).toEqual([])
            return wordsWidth(el)
          })
          return Math.max(...words) + beside(row)
        }),
      )
      const box = list.getBoundingClientRect().width
      expect(box).toBeCloseTo(widest + 2 * insetOf(list), 0)
      expect(box, 'short of the cap').toBeLessThan(
        lengthOf(find(host, '.rule-editor'), '--rule-list-widest'),
      )
    },
  )

  /**
   * At the breakpoint the list is as narrow as it is drawn, its longest
   * unbreakable piece wide, so a name holding a Rust path is broken after
   * the path's `::`; each name wraps between its words and there, never
   * inside a word.
   */
  it('breaks a rule’s name between its words and after a ::, never inside a word, at 900px', async () => {
    const host = await rulesAt(900, seeded)
    expectNamesBrokenBetweenWords(host)
    for (const [i, name] of [
      ...host.querySelectorAll('.rule-row .rule-name'),
    ].entries()) {
      const text = name.textContent ?? ''
      if (text.includes('::')) {
        expect
          .soft(lineStarts(name), `rule ${i + 1} after its ::`)
          .toContain(text.indexOf('::') + 2)
      }
    }
  })

  it('draws the rule’s pane as wide as its sentence while no rule is picked, at 1920px', async () => {
    const host = await rulesAt(1920, seeded)
    const { pane } = boxesOf(host)
    const sentence = find(host, '.editor-panes .settings-box > .empty')
    expect(lineStarts(sentence), 'the sentence on one line').toEqual([])
    expect(pane.getBoundingClientRect().width).toBeCloseTo(
      wordsWidth(sentence) + 2 * insetOf(pane),
      0,
    )
  })

  /**
   * Picked, the rule's form sets the pane's width: the four boxes under
   * Sets side by side, each as wide as what it holds, and the note on a row
   * of its own, where the longest seeded rule's note is whole. The
   * fixture's library has a path longer than the form, so the scope select
   * would be the widest thing in it if a select gave the form width.
   */
  it('draws the rule’s pane as wide as its form once a rule is picked, the boxes under Sets side by side at --rule-sets-box and the longest note whole, the row narrower than 1370px, at 1920px', async () => {
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [
        {
          id: 1,
          path: `/mnt/user/${'media/archive/'.repeat(12)}Volumes`,
          edition_key: 'en',
          default_kind: 'manga',
          default_language: 'en',
          exclude_globs: null,
          scan_interval_secs: 86400,
          keeps_documents: false,
          last_scan_at: null,
          last_scan_status: null,
          next_scan_at: null,
          paused_at: null,
        },
      ],
    })
    const host = await rulesAt(1920, seeded)
    await pick(host, 2)
    const { pane } = boxesOf(host)
    const scope = find(host, 'select[name="root_id"]')
    expect(scope.querySelectorAll('option')).toHaveLength(2)
    expect(ownWidth(scope), 'the scope select on its own').toBeGreaterThan(
      pane.getBoundingClientRect().width,
    )

    expectFormAtItsOwnWidth(host)
    // U17: the user's bound for a picked row at this width.
    expect(
      find(host, '.editor-panes').getBoundingClientRect().width,
    ).toBeLessThan(1370)
  })

  /**
   * A coarse pointer draws the boxes border-box, so that the 44px it floors
   * them at is the drawn box's; each still holds its twelve characters
   * inside its padding and border.
   */
  it('draws the boxes under Sets --rule-sets-box wide inside their edges under a coarse pointer too, at 1920px', async () => {
    onTestFinished(coarseRulesOn())
    const host = await rulesAt(1920, seeded)
    await pick(host, 2)
    expectFormAtItsOwnWidth(host)
  })

  /**
   * Picked, where the list at its widest rule's width and the form at its
   * own do not both fit, the list gives way: its names wrap between their
   * words, and the form keeps the width that holds the boxes under Sets on
   * one line and the note whole.
   */
  it('gives way with the list and not the form once a rule is picked, the list’s names wrapping between their words, at 1280px', async () => {
    const host = await rulesAt(1280, seeded)
    const unpicked = boxesOf(host).list.getBoundingClientRect().width
    await pick(host, 2)
    expectFormAtItsOwnWidth(host)
    expect(
      boxesOf(host).list.getBoundingClientRect().width,
      'the list narrower than with no rule picked',
    ).toBeLessThan(unpicked - 1)
    const { main } = contentOf(host)
    expect(main.scrollWidth, 'nothing past main').toBe(main.clientWidth)
    expect(
      expectNamesBrokenBetweenWords(host),
      'names wrapped',
    ).toBeGreaterThan(0)
  })

  /**
   * Impact holds a list narrower than the panes, so the preview is as wide
   * as they are; Provenance's table is wider than both, so the preview is
   * as wide as the table, and the panes stay as they were. A path wider
   * than `main` wraps in its cell, and the preview stops where `main`'s
   * content does.
   */
  it.each([
    ['wider than the panes', 6],
    ['wider than main', 30],
  ] as const)(
    'draws the preview as wide as the panes, and as its table where the table is %s, the panes where they were, at 1920px',
    async (_, depth) => {
      vi.mocked(rulesApi.previewRules).mockResolvedValue({
        impact,
        rows: [
          {
            rel_path: `Manga/${'A Series With A Long Name/'.repeat(depth)}v01.cbz`,
            root_id: 1,
            before: null,
            after: null,
            matched: true,
            decided_by: { series: { kind: 'rule', index: 0 } },
          },
        ],
        rows_truncated: false,
        overrides: [],
      })
      const host = await rulesAt(1920, seeded)
      const { list, pane, preview } = boxesOf(host)
      const panes = [list, pane].map((box) => box.getBoundingClientRect())
      expect(preview.getBoundingClientRect().left).toBeCloseTo(panes[0].left, 0)
      expect(preview.getBoundingClientRect().right).toBeCloseTo(
        panes[1].right,
        0,
      )

      await act(async () => {
        const tab = [
          ...host.querySelectorAll<HTMLElement>('[role="tab"]'),
        ].find((el) => el.textContent === 'Provenance')
        tab?.click()
        await Promise.resolve()
      })
      const table = find(host, '.preview table').getBoundingClientRect()
      const drawn = preview.getBoundingClientRect()
      const content = contentOf(host)
      expect(drawn.right, 'wider than the panes').toBeGreaterThan(
        panes[1].right + 1,
      )
      if (depth === 6) {
        expect(drawn.width).toBeCloseTo(table.width + 2 * insetOf(preview), 0)
      } else {
        // As far from main's end as the boxes start from its start.
        expect(content.end - drawn.right).toBeCloseTo(
          panes[0].left - content.start,
          0,
        )
        // The path wraps in its cell rather than widening the table.
        expect(lineStarts(find(host, '.preview td')).length).toBeGreaterThan(0)
      }
      expect(content.main.scrollWidth).toBe(content.main.clientWidth)
      for (const [i, box] of [list, pane].entries()) {
        expect
          .soft(box.getBoundingClientRect().toJSON())
          .toEqual(panes[i].toJSON())
      }
    },
  )

  /**
   * The fixture's one rule leaves the panes narrower than Import's help on
   * one line, so a help that gave the column its width would widen it here.
   */
  it('keeps the column as wide with Import’s help as without it, at 1920px', async () => {
    const host = await rulesAt(1920)
    const editor = find(host, '.rule-editor')
    const help = find(host, '.editor-actions .import > p:not([role])')
    const words = help.textContent ?? ''
    const line = document.createElement('span')
    line.style.whiteSpace = 'nowrap'
    line.textContent = words
    help.replaceChildren(line)
    expect(
      line.getBoundingClientRect().width,
      'the help on one line',
    ).toBeGreaterThan(
      boxesOf(host).pane.getBoundingClientRect().right -
        boxesOf(host).list.getBoundingClientRect().left,
    )
    help.replaceChildren(words)
    const withWords = editor.getBoundingClientRect().width
    help.replaceChildren()
    const without = editor.getBoundingClientRect().width
    help.replaceChildren(words)
    expect(withWords).toBeCloseTo(without, 0)
  })

  /**
   * Import draws no box of its own, so its help, under the three, does not
   * hold Export off past the help's end.
   */
  it.each([1920, 1366])(
    'sets Export one gap past Import, with Import’s help under the three, at %ipx',
    async (width) => {
      const host = await rulesAt(width)
      const row = find(host, '.editor-actions')
      const save = find(
        host,
        '.editor-actions button.save',
      ).getBoundingClientRect()
      const importing = find(
        host,
        '.editor-actions .import button',
      ).getBoundingClientRect()
      const exporting = find(
        host,
        '.editor-actions a.export',
      ).getBoundingClientRect()
      const help = find(
        host,
        '.editor-actions .import > p:not([role])',
      ).getBoundingClientRect()
      expect(exporting.left).toBeCloseTo(
        importing.right + Number.parseFloat(getComputedStyle(row).columnGap),
        0,
      )
      expect(help.top).toBeGreaterThanOrEqual(
        Math.max(save.bottom, importing.bottom, exporting.bottom),
      )
      expect(help.left, 'from the row’s start').toBeCloseTo(save.left, 0)
    },
  )

  /**
   * At 1024 the two panes are still side by side, and neither the list, the
   * pane's sentence nor its form takes more than `main` has. Picked, the
   * list is at its floor there, its longest unbreakable piece, and the form
   * narrower than its own width, so the boxes under Sets go two to a line,
   * each field as wide as its box.
   */
  it('draws the list and the rule’s pane side by side within main, with a rule picked and without, the rules inside the list and no name broken inside a word, and the boxes under Sets each its field’s width, at 1024px', async () => {
    const host = await rulesAt(1024, seeded)
    const { main } = contentOf(host)
    for (const picked of [false, true]) {
      if (picked) await pick(host, 2)
      const { list, pane } = boxesOf(host)
      const [l, p] = [list, pane].map((box) => box.getBoundingClientRect())
      expect
        .soft(l.top, `side by side, picked: ${picked}`)
        .toBeCloseTo(p.top, 0)
      expect
        .soft(l.right, `side by side, picked: ${picked}`)
        .toBeLessThan(p.left)
      expect
        .soft(main.scrollWidth, `nothing past main, picked: ${picked}`)
        .toBe(main.clientWidth)
      const listBox = find(list, '.settings-box')
      expect
        .soft(
          listBox.scrollWidth,
          `the rules inside the list, picked: ${picked}`,
        )
        .toBe(listBox.clientWidth)
      expectNamesBrokenBetweenWords(host)
    }
    const sets = [
      ...host.querySelectorAll<HTMLInputElement>('.rule-sets input'),
    ]
    expect(sets).toHaveLength(4)
    expect(
      sets[2].getBoundingClientRect().top,
      'two to a line',
    ).toBeGreaterThan(sets[1].getBoundingClientRect().top)
    for (const input of sets) {
      expect
        .soft(input.parentElement?.getBoundingClientRect().width, input.name)
        .toBeCloseTo(input.getBoundingClientRect().width, 0)
    }
  })

  /**
   * Names with long pieces that hold no space: two regex rules with no
   * note, named by their patterns, and a note written as one identifier.
   */
  const unbreakable: rulesApi.Rule[] = [
    ...[
      '^Manga/Shonen_Jump_Collection/(?<series>[^/]+)/',
      '^Weekly_Shonen_Jump_(?<year>\\d{4})_Issue_(?<issue>\\d+)',
    ].map((pattern, i) => ({
      ...rule,
      id: 10 + i,
      ordinal: 5 + i,
      pattern,
      set_series: null,
    })),
    {
      ...rule,
      id: 20,
      ordinal: 7,
      set_series: null,
      note: 'series_from_the_second_directory_for_publishers_sorted_scans_of_the_old_nas',
    },
  ]

  /**
   * The identifier, wider than any line here, is broken inside, while the
   * seeded rules' names, whose pieces fit, break only between their words.
   * Nothing runs past the list or `main`, and beside the sidebar the rule's
   * pane keeps `--rule-pane-narrowest` beside the list.
   */
  it.each([390, 900, 1024, 1920])(
    'breaks inside a name’s piece too wide for any line, the seeded names only between their words, with nothing past the list or main and the pane kept, with a rule picked and without, at %ipx',
    async (width) => {
      const host = await rulesAt(width, [...seeded, ...unbreakable])
      const { main } = contentOf(host)
      const names = [...host.querySelectorAll('.rule-row .rule-name')]
      expect(names).toHaveLength(7)
      for (const picked of [false, true]) {
        if (picked) await pick(host, 6)
        const { list, pane } = boxesOf(host)
        const listBox = find(list, '.settings-box')
        expect
          .soft(
            listBox.scrollWidth,
            `the rules inside the list, picked: ${picked}`,
          )
          .toBe(listBox.clientWidth)
        expect
          .soft(main.scrollWidth, `nothing past main, picked: ${picked}`)
          .toBe(main.clientWidth)
        if (matchMedia(SIDEBAR_LAYOUT).matches) {
          expect
            .soft(
              pane.getBoundingClientRect().width,
              `the pane, picked: ${picked}`,
            )
            .toBeGreaterThanOrEqual(
              lengthOf(find(host, '.rule-editor'), '--rule-pane-narrowest') -
                0.5,
            )
        }
        expect(
          lineStarts(names[6]).length,
          'the identifier broken',
        ).toBeGreaterThan(0)
        expectNamesBrokenBetweenWords(host, names.slice(0, seeded.length))
      }
    },
  )

  /**
   * A larger root font widens the sidebar, `main`'s padding, the names and
   * the rule's pane but moves no px bound of the phone layout, so beside
   * the sidebar Rules has less room than the window's width says. Side by
   * side, the rule's pane keeps `--rule-pane-narrowest` and the seeded
   * names break only between their words; where Rules has not the room for
   * that, it is one column, the list as wide as Rules, the most room a name
   * can be given. Nothing runs past the list or `main` either way.
   *
   * The turn, the narrowest window that puts the panes side by side, is
   * found by halving and checked with the window a pixel either side of it:
   * a turn set short of the room the seeded list needs breaks a name there
   * and nowhere a coarser step would see. Then every 20px from the
   * breakpoint to past the turn, and 1024px; what fails anywhere is listed
   * together. A coarse pointer grows each row's enable box, so the list
   * needs the most room under one at the smallest root font past the
   * breakpoint's.
   */
  it.each([
    ['18px', 'fine'],
    ['20px', 'fine'],
    ['24px', 'fine'],
    ['32px', 'fine'],
    ['17px', 'coarse'],
  ] as const)(
    'keeps every rule inside the list and nothing past main at a %s root font under a %s pointer, side by side only with the pane kept and the seeded names whole, else one column with the list as wide as Rules, at the turn to side by side, a pixel either side of it and every 20px from 900px to 60px past it, with a rule picked and without',
    async (rootFont, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      rootFontAt(rootFont)
      const host = await rulesAt(900, [...seeded, ...unbreakable])
      const { main } = contentOf(host)
      const editor = find(host, '.rule-editor')
      const names = [...host.querySelectorAll('.rule-row .rule-name')]
      const sideBySide = () => {
        const { list, pane } = boxesOf(host)
        return (
          Math.abs(
            list.getBoundingClientRect().top - pane.getBoundingClientRect().top,
          ) < 1
        )
      }
      let [narrow, wide] = [899, 2560]
      await page.viewport(wide, 900)
      expect(sideBySide(), `side by side at ${wide}px`).toBe(true)
      while (wide - narrow > 1) {
        const width = Math.floor((narrow + wide) / 2)
        await page.viewport(width, 900)
        if (sideBySide()) {
          wide = width
        } else {
          narrow = width
        }
      }
      const turn = wide
      const widths = new Set([1024, turn - 1, turn, turn + 1])
      for (let width = 900; width <= turn + 60; width += 20) widths.add(width)
      const failures: string[] = []
      for (const picked of [false, true]) {
        if (picked) await pick(host, 2)
        for (const width of [...widths].sort((a, b) => a - b)) {
          await page.viewport(width, 900)
          const at = `, at ${width}px (the turn ${turn}px), picked: ${picked}`
          const { list, pane } = boxesOf(host)
          const [l, p] = [list, pane].map((box) => box.getBoundingClientRect())
          const listBox = find(list, '.settings-box')
          const spill = listBox.scrollWidth - listBox.clientWidth
          if (spill !== 0) {
            failures.push(`the rules ${spill}px past the list${at}`)
          }
          const past = main.scrollWidth - main.clientWidth
          if (past !== 0) {
            failures.push(`${past}px past main${at}`)
          }
          if (Math.abs(l.top - p.top) < 1) {
            if (p.width < lengthOf(editor, '--rule-pane-narrowest') - 0.5) {
              failures.push(`the pane ${p.width}px${at}`)
            }
            for (const { betweenWords, where } of lineBreaksOf(
              names.slice(0, seeded.length),
            )) {
              if (!betweenWords) failures.push(where + at)
            }
          } else {
            const rules = editor.getBoundingClientRect().width
            if (Math.abs(l.width - rules) > 0.5) {
              failures.push(`the list ${l.width}px of Rules' ${rules}px${at}`)
            }
          }
        }
      }
      expect(failures).toEqual([])
    },
  )

  /**
   * In the phone layout the sidebar is a drawer and Rules is one column at
   * any width, though at these widths it has more than the room the panes
   * are side by side in beside the sidebar.
   */
  it.each([800, 899])(
    'draws the list above the rule’s pane, each as wide as Rules, with a rule picked and without, at %ipx',
    async (width) => {
      const host = await rulesAt(width, seeded)
      const editor = find(host, '.rule-editor')
      for (const picked of [false, true]) {
        if (picked) await pick(host, 2)
        const { list, pane } = boxesOf(host)
        const [l, p] = [list, pane].map((box) => box.getBoundingClientRect())
        const rules = editor.getBoundingClientRect().width
        expect
          .soft(p.top, `the pane under the list, picked: ${picked}`)
          .toBeGreaterThanOrEqual(l.bottom)
        expect
          .soft(l.width, `the list as wide as Rules, picked: ${picked}`)
          .toBeCloseTo(rules, 0)
        expect
          .soft(p.width, `the pane as wide as Rules, picked: ${picked}`)
          .toBeCloseTo(rules, 0)
      }
    },
  )

  /**
   * Its seven fields all decided and its series as long as real ones,
   * either table is wider than `main` at 1024.
   */
  const wideRows: rulesApi.Row[] = Array.from({ length: 3 }, (_, i) => ({
    rel_path: `Manga/Monster/Monster v0${i + 1}.cbz`,
    root_id: 1,
    before: {
      series: 'Monster - The Perfect Edition - Colored',
      work: 'Monster',
      edition: 'en',
    },
    after: {
      series: 'Monster - The Perfect Edition - Colored Deluxe',
      work: 'Monster',
      edition: 'en',
    },
    matched: i % 2 === 0,
    decided_by: Object.fromEntries(
      rulesApi.FIELDS.map((field) => [
        field,
        field === 'title' || field === 'kind'
          ? { kind: 'root_default' as const }
          : { kind: 'rule' as const, index: 0 },
      ]),
    ),
  }))

  it.each(['Provenance', 'Matched files'])(
    'scrolls a %s table wider than main inside its box, with the panes where they were and nothing past main, at 1024px',
    async (tab) => {
      vi.mocked(rulesApi.previewRules).mockResolvedValue({
        impact,
        rows: wideRows,
        rows_truncated: false,
        overrides: [],
      })
      const host = await rulesAt(1024, seeded)
      const { list, pane } = boxesOf(host)
      const panes = [list, pane].map((box) =>
        box.getBoundingClientRect().toJSON(),
      )
      await showTab(host, tab)
      const scroller = find(host, '.preview .table-scroll')
      expect(scroller.scrollWidth, 'the table scrolls').toBeGreaterThan(
        scroller.clientWidth,
      )
      const { main } = contentOf(host)
      expect(main.scrollWidth, 'nothing past main').toBe(main.clientWidth)
      for (const [i, box] of [list, pane].entries()) {
        expect.soft(box.getBoundingClientRect().toJSON()).toEqual(panes[i])
      }
    },
  )

  it('breaks an override’s path inside the preview, with the panes where they were and nothing past main, at 1024px', async () => {
    vi.mocked(rulesApi.previewRules).mockResolvedValue({
      impact,
      rows: [],
      rows_truncated: false,
      overrides: [
        {
          field: 'series',
          rel_path: `Manga/${'Some_Series_Name_Long/'.repeat(12)}v01.cbz`,
          first: 0,
          wins: 3,
          first_value: 'Some_Series_Name_Long',
          wins_value: 'Some Series Name Long',
          others: 0,
          examples: 1,
        },
      ],
    })
    const host = await rulesAt(1024, seeded)
    const { list, pane, preview } = boxesOf(host)
    const panes = [list, pane].map((box) =>
      box.getBoundingClientRect().toJSON(),
    )
    await showTab(host, 'Overrides')
    const where = find(host, '.preview .where')
    expect(lineStarts(where).length, 'the path broken').toBeGreaterThan(0)
    expect(
      where.getBoundingClientRect().right,
      'inside the preview',
    ).toBeLessThanOrEqual(
      preview.getBoundingClientRect().right - insetOf(preview) + 0.5,
    )
    const { main } = contentOf(host)
    expect(main.scrollWidth, 'nothing past main').toBe(main.clientWidth)
    for (const [i, box] of [list, pane].entries()) {
      expect.soft(box.getBoundingClientRect().toJSON()).toEqual(panes[i])
    }
  })

  /**
   * A note long enough to reach the cap, written in words, so it wraps
   * between them at the cap's width.
   */
  it.each([1920, 1280])(
    'stops the list at --rule-list-widest, a long name wrapping between its words, at %ipx',
    async (width) => {
      const host = await rulesAt(width, [
        ...seeded,
        {
          ...rule,
          id: 20,
          ordinal: 5,
          set_series: null,
          note: 'series from the second directory under the root where the first is a publisher and the second a series, for the scans that came in sorted that way',
        },
      ])
      const { list } = boxesOf(host)
      expect(list.getBoundingClientRect().width).toBeCloseTo(
        lengthOf(find(host, '.rule-editor'), '--rule-list-widest'),
        0,
      )
      const names = host.querySelectorAll('.rule-row .rule-name')
      expect(lineStarts(names[4]).length, 'the note wrapped').toBeGreaterThan(0)
      expectNamesBrokenBetweenWords(host)
    },
  )

  /**
   * Overrides' lead is a sentence wider on one line than the panes, so a
   * lead that gave the column width would widen it.
   */
  it('keeps the column as wide with the Overrides lead as without it, at 1920px', async () => {
    vi.mocked(rulesApi.previewRules).mockResolvedValue({
      impact,
      rows: [],
      rows_truncated: false,
      overrides: [
        {
          field: 'series',
          rel_path: 'Manga/Monster/Monster v01.cbz',
          first: 0,
          wins: 3,
          first_value: 'Monster',
          wins_value: 'Monster (Colored)',
          others: 0,
          examples: 1,
        },
      ],
    })
    const host = await rulesAt(1920, seeded)
    await showTab(host, 'Overrides')
    const editor = find(host, '.rule-editor')
    const lead = find(host, '.preview .overrides-lead')
    const words = lead.textContent ?? ''
    const line = document.createElement('span')
    line.style.whiteSpace = 'nowrap'
    line.textContent = words
    lead.replaceChildren(line)
    expect(
      line.getBoundingClientRect().width,
      'the lead on one line',
    ).toBeGreaterThan(find(host, '.editor-panes').getBoundingClientRect().width)
    lead.replaceChildren(words)
    const withWords = editor.getBoundingClientRect().width
    lead.replaceChildren()
    const without = editor.getBoundingClientRect().width
    lead.replaceChildren(words)
    expect(withWords).toBeCloseTo(without, 0)
  })

  /**
   * It starts where the other Configuration pages' content starts, Backup's
   * here, and the room past it is on its right.
   */
  it.each([2560, 1920])(
    'starts its boxes where Backup’s do, with the room past them on the right, at %ipx',
    async (width) => {
      const host = await rulesAt(width)
      const boxes = [
        ...host.querySelectorAll('.rule-editor .settings-box'),
      ].map((box) => box.getBoundingClientRect())
      expect(boxes.length).toBeGreaterThanOrEqual(3)
      const left = Math.min(...boxes.map((box) => box.left))
      const right = Math.max(...boxes.map((box) => box.right))
      expect(right, 'short of main’s end').toBeLessThan(contentOf(host).end)

      await act(async () => {
        rerender(framed('/config/backup'))
        await Promise.resolve()
        await Promise.resolve()
      })
      const backup = host.querySelector('.backup .settings-box')
      expect(backup, 'Backup’s box').not.toBeNull()
      expect(left).toBeCloseTo(backup?.getBoundingClientRect().left ?? 0, 0)
    },
  )
})

/**
 * A control that a save, an import or a preview takes while it has focus
 * keeps focus, in a real browser: jsdom never moves focus off a disabled
 * control, and Chromium moves it to the page. Where focus is is read before
 * how the control is held, so that a control held by `disabled` fails on
 * focus left on the page.
 */
describe('the Rules screen’s held controls, in a real browser', () => {
  const second: rulesApi.Rule = {
    ...rule,
    id: 2,
    ordinal: 2,
    pattern: 'Colour/.*',
  }
  const override: rulesApi.Override = {
    field: 'series',
    rel_path: 'a/b',
    first: 0,
    wins: 1,
    first_value: 'A',
    wins_value: 'B',
    others: 0,
    examples: 3,
  }
  const previewed = (
    overrides: rulesApi.Override[] = [],
  ): rulesApi.Preview => ({
    impact,
    rows: [],
    rows_truncated: false,
    overrides,
  })

  beforeEach(() => {
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule, second],
      complaints: [],
    })
  })

  async function rulesScreen(): Promise<HTMLElement> {
    await page.viewport(1280, 800)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          <div className="sidebar-column" />
          <main>
            <div className="config">
              <RuleEditor />
            </div>
          </main>
        </div>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await settle()
    return host
  }

  /** Real keys, inside `act` so that React sees what they start. */
  const keys = (text: string) =>
    act(async () => {
      await userEvent.keyboard(text)
    })

  async function settle(ms = 120) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, ms))
    })
  }

  /**
   * Gives the test's frame focus with a real click on words that take none,
   * so that the keys pressed next reach the page and not the frame, then
   * puts focus on `el`.
   */
  async function focusFirst(host: HTMLElement, el: HTMLElement) {
    await act(async () => {
      await userEvent.click(host.querySelector('.import p') as Element)
    })
    act(() => el.focus())
    expect(document.activeElement).toBe(el)
  }

  const saveButton = (host: HTMLElement) =>
    host.querySelector('button.save') as HTMLButtonElement

  it('keeps focus on Save when a derive’s hold comes on while it has focus, says why beside it, and sends nothing for Enter or a real mouse press; focus stays as the hold ends (O51)', async () => {
    const first = deferred<derive.DeriveStatus>()
    vi.mocked(derive.deriveStatus)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({ derive_running: true, job_held: false })
    const host = await rulesScreen()
    const save = saveButton(host)
    expect(holdOf(save)).toBe('free')
    await focusFirst(host, save)

    // The first read was out as the first preview was sent, so the derive it
    // finds may be that preview: it holds nothing, and the read after it,
    // sent with nothing of the page's out, brings the hold.
    await act(async () => {
      first.resolve({ derive_running: true, job_held: false })
    })
    await settle()
    expect(holdOf(save)).toBe('free')
    await settleUntil('Save, held', () => holdOf(save) === 'held', 3000)
    expect(document.activeElement).toBe(save)
    expect(holdOf(save)).toBe('held')
    expect(
      document.getElementById(save.getAttribute('aria-describedby') ?? '')
        ?.textContent,
    ).toBe('Waits for a scan or another library job to finish.')
    await keys('{Enter}')
    await pressOn(save)
    expect(rulesApi.saveRules).not.toHaveBeenCalled()

    // The poll finds the lease free, and the list on screen is previewed.
    await settleUntil('Save, free', () => holdOf(save) === 'free', 3000)
    expect(document.activeElement).toBe(save)
    await pressOn(save)
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
  })

  it('draws why Save is held in the muted colour, as the stamp’s and the directory page’s why-lines are (O51)', async () => {
    vi.mocked(derive.deriveStatus).mockResolvedValue({
      derive_running: false,
      job_held: true,
    })
    const host = await rulesScreen()
    const save = saveButton(host)
    await settleUntil('Save, held', () => holdOf(save) === 'held', 3000)
    const why = document.getElementById(
      save.getAttribute('aria-describedby') ?? '',
    ) as HTMLElement
    expect(why.textContent).toBe(
      'Waits for a scan or another library job to finish.',
    )
    const around = why.parentElement as Element
    expect(tokenColour(around, '--muted')).not.toBe(
      tokenColour(around, '--text'),
    )
    expect(getComputedStyle(why).color).toBe(tokenColour(around, '--muted'))
  })

  it('keeps focus on Save, pressed with Enter, through the save’s drawn wait and the preview that follows it; Enter on it sends nothing while it is held', async () => {
    const sent = deferred<rulesApi.Saved>()
    vi.mocked(rulesApi.saveRules).mockReturnValue(sent.promise)
    const host = await rulesScreen()
    const save = saveButton(host)
    await focusFirst(host, save)

    await keys('{Enter}')
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    // A second Enter inside the wait's delay, the hold not drawn before or
    // after it: what turns it down is `save`'s own guards, not the hold.
    expect(holdOf(save)).toBe('free')
    await keys('{Enter}')
    expect(holdOf(save)).toBe('free')
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(save)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(save)
    expect([save.textContent, holdOf(save)]).toEqual(['Saving…', 'held'])
    await keys('{Enter}')
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)

    // The save lands, and the preview of the list it saved is held open.
    const following = deferred<rulesApi.Preview>()
    vi.mocked(rulesApi.previewRules).mockReturnValueOnce(following.promise)
    await act(async () => {
      sent.resolve({
        impact,
        rules: [rule, second],
        reopened_proposals: [],
        complaints: [],
        derive: emptyDerive,
      })
    })
    expect(document.activeElement).toBe(save)
    expect([save.textContent, holdOf(save)]).toEqual([
      'Save — apply to 71 works',
      'held',
    ])
    await keys('{Enter}')
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)

    await act(async () => {
      following.resolve(previewed())
    })
    expect(document.activeElement).toBe(save)
    expect(holdOf(save)).toBe('free')
  })

  it('keeps focus on Save, clicked while a changed box has focus, through the preview the box’s blur starts; that click sends nothing', async () => {
    const host = await rulesScreen()
    await act(async () => {
      await userEvent.click(
        host.querySelector('.rule-row button.select') as Element,
      )
    })
    await settle()
    const pattern = host.querySelector(
      'input[name="pattern"]',
    ) as HTMLInputElement
    await act(async () => {
      await userEvent.click(pattern)
      await userEvent.keyboard('{End}x')
    })
    const previewing = deferred<rulesApi.Preview>()
    vi.mocked(rulesApi.previewRules).mockReturnValueOnce(previewing.promise)
    const previews = vi.mocked(rulesApi.previewRules).mock.calls.length
    const save = saveButton(host)

    await act(async () => {
      await userEvent.click(save)
    })
    // The blur committed the box, so a preview is out and Save is held.
    expect(vi.mocked(rulesApi.previewRules).mock.calls.length).toBe(
      previews + 1,
    )
    expect(vi.mocked(rulesApi.previewRules).mock.calls.at(-1)?.[0][0]).toEqual(
      expect.objectContaining({ pattern: 'Manga/.*x' }),
    )
    expect(rulesApi.saveRules).not.toHaveBeenCalled()
    // Not drawn held after the click, so it was `save`'s guard that turned
    // the click down.
    expect(holdOf(save)).toBe('free')
    expect(document.activeElement).toBe(save)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(save)
    expect(holdOf(save)).toBe('held')

    await act(async () => {
      previewing.resolve(previewed())
    })
    expect(document.activeElement).toBe(save)
    expect(holdOf(save)).toBe('free')
    expect(rulesApi.saveRules).not.toHaveBeenCalled()
  })

  it('keeps focus on Import through the drawn wait of the file it picked; Enter on it opens no picker and refuses nothing while it is held', async () => {
    const sent = deferred<Awaited<ReturnType<typeof rulesApi.importRules>>>()
    vi.mocked(rulesApi.importRules).mockReturnValue(sent.promise)
    const host = await rulesScreen()
    const importButton = host.querySelector(
      '.import button',
    ) as HTMLButtonElement
    const input = host.querySelector('input[type="file"]') as HTMLInputElement
    // The picker itself is the browser's; what is read is whether it opened.
    const picker = vi.spyOn(input, 'click').mockImplementation(() => {})
    await focusFirst(host, importButton)

    await keys('{Enter}')
    expect(picker).toHaveBeenCalledTimes(1)
    // The picker answers with a file, and leaves focus where it was.
    const files = new DataTransfer()
    files.items.add(
      new File(['[]'], 'rules.json', { type: 'application/json' }),
    )
    await act(async () => {
      input.files = files.files
      input.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(document.activeElement).toBe(importButton)
    // Enter inside the wait's delay, the hold not drawn before or after it:
    // what turns it down is Import's own handler, not the hold.
    expect(holdOf(importButton)).toBe('free')
    await keys('{Enter}')
    expect(holdOf(importButton)).toBe('free')
    expect(picker).toHaveBeenCalledTimes(1)
    expect(host.querySelector('p.refused')).toBeNull()
    expect(document.activeElement).toBe(importButton)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(importButton)
    expect([importButton.textContent, holdOf(importButton)]).toEqual([
      'Importing…',
      'held',
    ])
    await keys('{Enter}')
    expect(picker).toHaveBeenCalledTimes(1)
    expect(host.querySelector('p.refused')).toBeNull()

    await act(async () => {
      sent.resolve({
        rules: [rule, second],
        complaints: [],
        reopened_proposals: [],
      })
    })
    await settle()
    expect(document.activeElement).toBe(importButton)
    expect([importButton.textContent, holdOf(importButton)]).toEqual([
      'Import',
      'free',
    ])
  })

  /** The Overrides tab open on a preview that draws one override row. */
  async function overridesOpen(): Promise<HTMLElement> {
    vi.mocked(rulesApi.previewRules).mockResolvedValue(previewed([override]))
    const host = await rulesScreen()
    await act(async () => {
      await userEvent.click(
        [...host.querySelectorAll('.preview-tabs button')].find((b) =>
          b.textContent?.startsWith('Overrides'),
        ) as Element,
      )
    })
    return host
  }

  it('keeps focus on Move rule 2 above rule 1, pressed with Enter, through the preview it starts; Enter on it sends nothing while it is held', async () => {
    const host = await overridesOpen()
    const reorder = host.querySelector('button.reorder') as HTMLButtonElement
    const previewing = deferred<rulesApi.Preview>()
    vi.mocked(rulesApi.previewRules).mockReturnValueOnce(previewing.promise)
    const previews = vi.mocked(rulesApi.previewRules).mock.calls.length
    await focusFirst(host, reorder)

    await keys('{Enter}')
    expect(vi.mocked(rulesApi.previewRules).mock.calls.length).toBe(
      previews + 1,
    )
    // A second Enter inside the wait's delay, the hold not drawn before or
    // after it: what turns it down is the button's own handler, not the hold.
    expect(holdOf(reorder)).toBe('free')
    await keys('{Enter}')
    expect(holdOf(reorder)).toBe('free')
    expect(vi.mocked(rulesApi.previewRules).mock.calls.length).toBe(
      previews + 1,
    )
    expect(document.activeElement).toBe(reorder)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(reorder)
    expect(holdOf(reorder)).toBe('held')
    await keys('{Enter}')
    expect(vi.mocked(rulesApi.previewRules).mock.calls.length).toBe(
      previews + 1,
    )

    await act(async () => {
      previewing.resolve(previewed([override]))
    })
    expect(document.activeElement).toBe(reorder)
    expect(holdOf(reorder)).toBe('free')
  })

  it('keeps focus on Edit rule 2, pressed with Enter, through the preview it starts; a second Enter, which opens the rule already open, sends nothing', async () => {
    const host = await overridesOpen()
    const edit = host.querySelector('button.edit-winner') as HTMLButtonElement
    const previewing = deferred<rulesApi.Preview>()
    vi.mocked(rulesApi.previewRules).mockReturnValueOnce(previewing.promise)
    const previews = vi.mocked(rulesApi.previewRules).mock.calls.length
    await focusFirst(host, edit)

    await keys('{Enter}')
    expect(vi.mocked(rulesApi.previewRules).mock.calls.length).toBe(
      previews + 1,
    )
    expect(document.activeElement).toBe(edit)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(edit)
    expect(holdOf(edit)).toBe('held')
    // Rule 2 is open by now, so a second press would select it again and
    // send nothing even with no hold at all: this reads what a press does,
    // not what turns it down. The handler's guard is pinned in jsdom, by
    // `PreviewTabs.test.tsx`'s "offers neither option".
    await keys('{Enter}')
    expect(vi.mocked(rulesApi.previewRules).mock.calls.length).toBe(
      previews + 1,
    )

    await act(async () => {
      previewing.resolve(previewed([override]))
    })
    expect(document.activeElement).toBe(edit)
    expect(holdOf(edit)).toBe('free')
  })
})

/**
 * The server's preview takes the derive lease without waiting for it, and
 * holds it for its whole derive, so a second preview sent while the page's
 * first is out is refused for the first's lease (O51). The server here does
 * the same: it refuses a preview while one is out, and its status says a
 * derive runs while one is. Gestures are real clicks, made as the app runs,
 * outside `act`.
 */
describe('the Rules screen’s previews, one at a time (O51)', () => {
  const second: rulesApi.Rule = {
    ...rule,
    id: 2,
    ordinal: 2,
    pattern: 'Colour/.*',
  }
  const previewed = (works_rehomed: number): rulesApi.Preview => ({
    impact: { ...impact, works_rehomed },
    rows: [],
    rows_truncated: false,
    overrides: [],
  })

  /** Waits, without `act`, until `drawn` holds. */
  async function until(what: string, drawn: () => boolean, within = 2000) {
    const end = performance.now() + within
    while (!drawn()) {
      if (performance.now() > end) {
        throw new Error(`${what}: not drawn within ${within} ms`)
      }
      await new Promise((resolve) => setTimeout(resolve, 20))
    }
  }

  it('sends a second gesture’s list once the first’s preview lands, not beside it: nothing refused, the newest list’s preview drawn', async () => {
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule, second],
      complaints: [],
    })
    /** Each preview the server took, with its answer held open. */
    const taken: { rules: rulesApi.Rule[]; answer: (works: number) => void }[] =
      []
    /** Each preview the server refused for the lease another held. */
    const refused: rulesApi.Rule[][] = []
    let out = 0
    vi.mocked(rulesApi.previewRules).mockImplementation((rules) => {
      if (out > 0) {
        refused.push(rules)
        return Promise.reject(
          new Error(
            '/api/library/rules/preview: 409 {"error":"a scan or another library job is running; preview again once it has finished"}',
          ),
        )
      }
      out += 1
      const answer = deferred<rulesApi.Preview>()
      taken.push({
        rules,
        answer: (works) => answer.resolve(previewed(works)),
      })
      return answer.promise.finally(() => {
        out -= 1
      })
    })
    vi.mocked(derive.deriveStatus).mockImplementation(async () => ({
      derive_running: out > 0,
      job_held: false,
    }))

    await page.viewport(1280, 800)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          <main>
            <div className="config">
              <RuleEditor />
            </div>
          </main>
        </div>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    const shown = host
    const save = () => shown.querySelector('button.save') as HTMLElement
    const pane = () =>
      shown.querySelector('.preview [role="tabpanel"]')?.textContent ?? ''
    const boxes = () =>
      shown.querySelectorAll<HTMLInputElement>(
        '.rule-row input[type="checkbox"]',
      )

    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      await until('the first preview, sent', () => taken.length === 1)
      taken[0].answer(71)
      await until('the first preview, drawn', () =>
        save().textContent?.includes('71'),
      )
      const panes = watchDrawn(pane)

      await userEvent.click(boxes()[0])
      await until('a preview of rule 1 off', () => taken.length === 2)
      // Two more while it is out: rule 2 off, then rule 1 on again.
      await userEvent.click(boxes()[1])
      await userEvent.click(boxes()[0])
      await new Promise((resolve) => setTimeout(resolve, 150))
      expect(refused).toEqual([])
      expect(taken.length).toBe(2)

      taken[1].answer(50)
      await until('a preview of the newest list', () => taken.length === 3)
      expect(taken[2].rules.map((one) => one.enabled)).toEqual([true, false])
      expect(save().textContent).not.toContain('50')
      await until('Save, held', () => holdOf(save()) === 'held')

      taken[2].answer(12)
      await until('the newest list’s preview, drawn', () =>
        save().textContent?.includes('12'),
      )
      expect(holdOf(save())).toBe('free')
      panes.stop()
      expect(refused).toEqual([])
      expect(taken.length).toBe(3)
      expect(
        [...panes.seen].filter((one) => one.startsWith('No preview')),
      ).toEqual([])
      expect(
        [...panes.seen].filter((one) => one.startsWith('Waits for')),
      ).toEqual([])
      expect(
        [...shown.querySelectorAll('[role="alert"]')].map(
          (one) => one.textContent,
        ),
      ).toEqual([])
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
  })
})

/**
 * "Previewing…" is drawn once a preview has lasted 200 ms, and drawing it
 * moves nothing: the Impact list's top is where it was before the preview
 * started, while the words are drawn, and once the preview has landed. The
 * preview is started by opening the second rule after the first, so the
 * rule's pane holds a form of the same rows before and after.
 */
describe('the Rules screen’s Previewing…', () => {
  /** The Rules screen in the shell at `width`, with two rules to open. */
  async function previewScreen(width: number) {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(width, 900)
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule, { ...rule, id: 2, ordinal: 2, pattern: 'Colour/.*' }],
      complaints: [],
    })
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          <div className="sidebar-column" />
          <main>
            <div className="config">
              <RuleEditor />
            </div>
          </main>
        </div>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    const shown = host
    const wait = (ms: number) =>
      act(async () => {
        await new Promise((r) => setTimeout(r, ms))
      })
    const open = async (n: number) => {
      const select = shown.querySelectorAll<HTMLElement>(
        '.rule-row button.select',
      )[n - 1]
      if (select === undefined) throw new Error(`no rule ${n}`)
      await act(async () => select.click())
    }
    /** The element whose words are "Previewing…". */
    const previewing = () => {
      const words = [...shown.querySelectorAll('*')].find(
        (el) => el.childElementCount === 0 && el.textContent === 'Previewing…',
      )
      if (words === undefined) throw new Error('no Previewing…')
      return words
    }
    /** Whether "Previewing…" is drawn where a person can see it. */
    const previewingSeen = () =>
      [...shown.querySelectorAll('*')].some(
        (el) =>
          el.childElementCount === 0 &&
          el.textContent === 'Previewing…' &&
          el.checkVisibility({ visibilityProperty: true }),
      )
    return { shown, wait, open, previewing, previewingSeen }
  }

  it.each([390, 1920])(
    'is drawn without moving the Impact list, at %ipx',
    async (width) => {
      const { shown, wait, open, previewingSeen } = await previewScreen(width)
      const impactTop = () => {
        const list = shown.querySelector('.preview .impact')
        if (list === null) throw new Error('no Impact list')
        return list.getBoundingClientRect().top
      }
      await open(1)
      await wait(120)
      await document.fonts.ready
      const before = impactTop()
      expect(previewingSeen()).toBe(false)

      const held = deferred<rulesApi.Preview>()
      vi.mocked(rulesApi.previewRules).mockReturnValueOnce(held.promise)
      await open(2)
      await wait(WAIT_DRAWN_AFTER_MS + 100)
      // The case's premise: the words are drawn now.
      expect(previewingSeen()).toBe(true)
      const during = impactTop()

      await act(async () => {
        held.resolve({ impact, rows: [], rows_truncated: false, overrides: [] })
      })
      expect(previewingSeen()).toBe(false)
      expect({ during, after: impactTop() }).toEqual({
        during: before,
        after: before,
      })
    },
  )

  /**
   * A section heading's actions end where the box under the heading ends
   * its rows, as Licence's Back to top does, and not at the box's outer
   * edge, its padding and its edge further out.
   */
  it.each([390, 1920])(
    'ends where the Preview box’s rows end, on the heading’s line, at %ipx',
    async (width) => {
      const { wait, open, previewing, previewingSeen } =
        await previewScreen(width)
      await open(1)
      await wait(120)
      const held = deferred<rulesApi.Preview>()
      vi.mocked(rulesApi.previewRules).mockReturnValueOnce(held.promise)
      await open(2)
      await wait(WAIT_DRAWN_AFTER_MS + 100)
      await document.fonts.ready
      // The case's premise: the words are drawn now.
      expect(previewingSeen()).toBe(true)
      const words = previewing()
      const section = words.closest('.settings-section')
      const title = section?.querySelector('.settings-section-title')
      const rows = [
        ...(section?.querySelectorAll(':scope > .settings-box > *') ?? []),
      ]
      if (!title || rows.length === 0) throw new Error('no heading or no row')
      const range = document.createRange()
      range.selectNodeContents(words)
      const said = range.getBoundingClientRect()
      const heading = title.getBoundingClientRect()
      for (const [i, row] of rows.entries()) {
        expect
          .soft(said.right, `the box’s row ${i + 1}`)
          .toBeCloseTo(row.getBoundingClientRect().right, 0)
      }
      // On the heading's line: the two share some of their height.
      expect({
        onHeadingLine: said.top < heading.bottom && said.bottom > heading.top,
      }).toEqual({ onHeadingLine: true })
      await act(async () => {
        held.resolve({ impact, rows: [], rows_truncated: false, overrides: [] })
      })
    },
  )
})

/**
 * The foot row, Save, Import with its help under it, and Export, sets the
 * three controls' words on one baseline wherever they share a line, and
 * where they do not, each goes to a line of its own whole: neither Save
 * nor Export sits beside Import's help. A baseline is read from a box of
 * no size put at the end of each control's words, which sits on it. With
 * a coarse pointer Export's box grows to 44px about its words.
 */
describe('the Rules screen’s foot row', () => {
  it.each([
    [1920, 'fine'],
    [1366, 'fine'],
    [390, 'fine'],
    [1920, 'coarse'],
    [390, 'coarse'],
  ] as const)(
    'sets Save, Import and Export on one baseline where they share a line, with none beside Import’s help, at %ipx with a %s pointer',
    async (width, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      const was = [window.innerWidth, window.innerHeight] as const
      onTestFinished(() => page.viewport(...was))
      await page.viewport(width, 900)
      const host = await act(async () => {
        const mounted = mount(
          <div className="library">
            <div className="sidebar-column" />
            <main>
              <MemoryRouter initialEntries={['/config/rules']}>
                <Routes>
                  <Route path="/config/*" element={<Config />} />
                </Routes>
              </MemoryRouter>
            </main>
          </div>,
        )
        await Promise.resolve()
        await Promise.resolve()
        return mounted
      })
      await document.fonts.ready
      const find = (selector: string) => {
        const el = host.querySelector<HTMLElement>(selector)
        if (el === null) throw new Error(`no ${selector}`)
        return el
      }
      const controls = {
        Save: find('.editor-actions button.save'),
        Import: find('.editor-actions .import button'),
        Export: find('.editor-actions a.export'),
      }
      const help = find('.editor-actions .import > p:not([role])')
      // Into the span holding a button's words, and into one made around
      // the link's: a box put straight into a flex or grid container is an
      // item of its own, placed apart from the words.
      const baselineOf = (control: HTMLElement) => {
        let words = control.querySelector(':scope > span')
        const made = words === null ? document.createElement('span') : null
        if (made !== null) {
          made.append(...control.childNodes)
          control.append(made)
          words = made
        }
        const probe = document.createElement('span')
        probe.style.cssText = 'display: inline-block; width: 0; height: 0'
        words?.append(probe)
        const at = probe.getBoundingClientRect().bottom
        probe.remove()
        if (made !== null) {
          control.append(...made.childNodes)
          made.remove()
        }
        return at
      }
      const placed = Object.entries(controls).map(([name, control]) => ({
        name,
        box: control.getBoundingClientRect(),
        baseline: baselineOf(control),
      }))
      const meet = (a: DOMRect, b: DOMRect) =>
        a.top < b.bottom && b.top < a.bottom
      const pairs = placed.flatMap((a, i) =>
        placed.slice(i + 1).map((b) => [a, b] as const),
      )
      for (const [a, b] of pairs) {
        if (!meet(a.box, b.box)) continue
        expect
          .soft(b.baseline, `${a.name} and ${b.name} share a line`)
          .toBeCloseTo(a.baseline, 0)
      }
      if (width === 1920) {
        // The three fit on one line at this width, with either pointer.
        expect(pairs.every(([a, b]) => meet(a.box, b.box))).toBe(true)
      }
      const helpBox = help.getBoundingClientRect()
      for (const { name, box } of placed) {
        if (name === 'Import') continue
        expect
          .soft(meet(box, helpBox), `${name} beside Import’s help`)
          .toBe(false)
      }
    },
  )
})

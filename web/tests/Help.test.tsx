import { describe, expect, it, vi } from 'vitest'
import type { ReaderModule } from '../src/module'
import { mount } from './support/mount'
import { Help } from '../src/Help'

const noop = () => {}

// `helpGroups` (modules/help.ts) owns the derivation rules — key ranges,
// label fallbacks, the reading-direction swap. These tests pin what `Help`
// itself does with whatever `helpGroups` hands back — the `visible` gate, the
// group→section and row→dt/dd mapping, and the close affordance — using the
// real function as a source of groups to render.

describe('Help', () => {
  it('renders nothing when not visible', () => {
    const host = mount(
      <Help
        visible={false}
        modules={[]}
        reading="ltr"
        mayCorrect
        onClose={noop}
      />,
    )
    expect(host.firstElementChild).toBeNull()
  })

  it('renders a labelled dialog with the Reading group, even with no modules', () => {
    const host = mount(
      <Help
        visible={true}
        modules={[]}
        reading="ltr"
        mayCorrect
        onClose={noop}
      />,
    )
    const dialog = host.querySelector('[role="dialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog?.getAttribute('aria-label')).toBe('Keyboard help')

    const headings = [...host.querySelectorAll('h3')].map((h) => h.textContent)
    expect(headings).toEqual(['Reading'])

    // Every reader-table row rendered as one dt/dd pair.
    const rows = host.querySelectorAll('dl > div')
    expect(rows.length).toBeGreaterThan(0)
    for (const row of rows) {
      expect(row.querySelector('dt kbd')?.textContent).not.toBe('')
      expect(row.querySelector('dd')?.textContent).not.toBe('')
    }

    // Assert at least one known literal row from READER_KEYS to ensure
    // helpGroups derives the correct keys and descriptions, not just that
    // rows exist and contain some text.
    const tabRow = [...rows].find(
      (row) => row.querySelector('dt kbd')?.textContent === 'Tab',
    )
    expect(tabRow).toBeDefined()
    expect(tabRow?.querySelector('dd')?.textContent).toBe(
      'move into the bars, and on round to the page',
    )
  })

  it('gives a passed-in module its own heading and folds a shared intent into one row', () => {
    const modules: ReaderModule<unknown>[] = [
      {
        id: 'help',
        label: 'Keyboard help',
        defaults: { enabled: true },
        keys: [
          { key: '?', intent: 'help:toggle', label: 'show this list of keys' },
          { key: 'h', intent: 'help:toggle', label: 'show this list of keys' },
        ],
      },
    ]
    const host = mount(
      <Help
        visible={true}
        modules={modules}
        reading="ltr"
        mayCorrect
        onClose={noop}
      />,
    )
    const headings = [...host.querySelectorAll('h3')].map((h) => h.textContent)
    expect(headings).toEqual(['Reading', 'Keyboard help'])

    // helpGroups folds the two keys sharing one intent into a single row —
    // Help renders that one row as one dt/dd pair, not two.
    const moduleSection = [...host.querySelectorAll('section')].find(
      (s) => s.querySelector('h3')?.textContent === 'Keyboard help',
    )
    const moduleRows = moduleSection?.querySelectorAll('dl > div')
    expect(moduleRows).toHaveLength(1)
    expect(moduleRows?.[0].querySelector('dt kbd')?.textContent).toBe('? / h')
    expect(moduleRows?.[0].querySelector('dd')?.textContent).toBe(
      'show this list of keys',
    )
  })

  // O58: the server refuses a correction from anyone but an administrator, so
  // the help offers `c` to an administrator alone.
  it('lists the correcting row only for an account that may correct', () => {
    const rowsSaid = (mayCorrect: boolean) => {
      const host = mount(
        <Help
          visible={true}
          modules={[]}
          reading="ltr"
          mayCorrect={mayCorrect}
          onClose={noop}
        />,
      )
      return [...host.querySelectorAll('dl > div dd')].map(
        (dd) => dd.textContent,
      )
    }
    expect(rowsSaid(true)).toContain('correct this pairing')
    const without = rowsSaid(false)
    expect(without).not.toContain('correct this pairing')
    expect(without).toContain('move into the bars, and on round to the page')
  })

  it('calls onClose when the Close button is pressed', () => {
    const onClose = vi.fn()
    const host = mount(
      <Help
        visible={true}
        modules={[]}
        reading="ltr"
        mayCorrect
        onClose={onClose}
      />,
    )
    const button = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    )
    expect(button).not.toBeUndefined()
    button?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('docks as a panel between the bars, its look in the stylesheet', () => {
    const host = mount(
      <Help
        visible={true}
        modules={[]}
        reading="ltr"
        mayCorrect
        onClose={noop}
      />,
    )
    const panel = host.querySelector('[role="dialog"]') as HTMLElement
    expect(panel.className).toBe('docked-panel')
    expect(panel.getAttribute('style')).toBeNull()
  })
})

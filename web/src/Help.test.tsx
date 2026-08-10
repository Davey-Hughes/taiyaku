import { describe, expect, it, vi } from 'vitest'
import type { ReaderModule } from './module'
import { mount } from './testing/mount'
import { Help } from './Help'

const noop = () => {}

// `helpGroups` (modules/help.ts) is already at 100% and owns the derivation
// rules — key ranges, label fallbacks, the reading-direction swap. These
// tests stay off that ground: they pin what `Help` itself does with whatever
// `helpGroups` hands back — the `visible` gate, the group→section and
// row→dt/dd mapping, and the close affordance — using the real function only
// as a source of *some* groups to render, never asserting on its formatting.

describe('Help', () => {
  it('renders nothing when not visible', () => {
    const host = mount(
      <Help visible={false} modules={[]} reading="ltr" onClose={noop} />,
    )
    expect(host.firstElementChild).toBeNull()
  })

  it('renders a labelled dialog with the Reading group, even with no modules', () => {
    const host = mount(
      <Help visible={true} modules={[]} reading="ltr" onClose={noop} />,
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
      'show or hide the filmstrip and readouts',
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
      <Help visible={true} modules={modules} reading="ltr" onClose={noop} />,
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

  it('calls onClose when the Close button is pressed', () => {
    const onClose = vi.fn()
    const host = mount(
      <Help visible={true} modules={[]} reading="ltr" onClose={onClose} />,
    )
    const button = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    )
    expect(button).not.toBeUndefined()
    button?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})

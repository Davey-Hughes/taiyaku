import { describe, expect, it, vi } from 'vitest'
import { thumbUrl } from './api'
import { mount } from './testing/mount'
import { CreditsPanel } from './CreditsPanel'

const noop = () => {}

describe('CreditsPanel', () => {
  it('renders nothing when not visible', () => {
    const host = mount(
      <CreditsPanel
        visible={false}
        work="work"
        edition="raw"
        pages={[]}
        skipping={false}
        onGo={noop}
        onClose={noop}
      />,
    )
    expect(host.firstElementChild).toBeNull()
  })

  it('says the sentence rather than showing an empty list when there are no credit pages', () => {
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="v1"
        pages={[]}
        skipping={false}
        onGo={noop}
        onClose={noop}
      />,
    )
    const dialog = host.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-label')).toBe('Credit pages in v1')
    expect(host.querySelector('strong')?.textContent).toBe('Credit pages — v1')
    expect(host.textContent).toContain('No credit pages found in v1.')
    // The empty case takes the ternary's true branch, so no page buttons.
    expect(host.querySelectorAll('img')).toHaveLength(0)
  })

  it('says the pages are kept out of the reading flow when skip is on', () => {
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="v1"
        pages={[2, 5]}
        skipping={true}
        onGo={noop}
        onClose={noop}
      />,
    )
    expect(host.textContent).toContain(
      'These are kept out of the reading flow. Taking one still goes there.',
    )
  })

  it('says the pages are still in the reading flow when skip is off', () => {
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="v1"
        pages={[2]}
        skipping={false}
        onGo={noop}
        onClose={noop}
      />,
    )
    expect(host.textContent).toContain(
      'These are still in the reading flow. Switch on “skip” in settings to set them aside.',
    )
  })

  it('renders each page 1-based and zero-based, in the same row, from the same index', () => {
    const host = mount(
      <CreditsPanel
        visible={true}
        work="my-work"
        edition="v1"
        pages={[2, 5]}
        skipping={true}
        onGo={noop}
        onClose={noop}
      />,
    )
    const buttons = [...host.querySelectorAll('button')].filter(
      (b) => b.textContent !== 'Close',
    )
    expect(buttons).toHaveLength(2)

    // 1-based in the label a reader sees...
    expect(buttons[0].textContent).toContain('page 3')
    expect(buttons[1].textContent).toContain('page 6')

    // ...zero-based everywhere thumbUrl and onGo deal in indices.
    const img0 = buttons[0].querySelector('img')
    expect(img0?.getAttribute('src')).toBe(thumbUrl('my-work', 'v1', 2))
    expect(img0?.getAttribute('alt')).toBe('')
    const img1 = buttons[1].querySelector('img')
    expect(img1?.getAttribute('src')).toBe(thumbUrl('my-work', 'v1', 5))
  })

  it('takes an entry to the zero-based page it shows, regardless of skip', () => {
    const onGo = vi.fn()
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="v1"
        pages={[2, 5]}
        skipping={true}
        onGo={onGo}
        onClose={noop}
      />,
    )
    const buttons = [...host.querySelectorAll('button')].filter(
      (b) => b.textContent !== 'Close',
    )
    buttons[1].dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onGo).toHaveBeenCalledExactlyOnceWith(5)
  })

  it('calls onClose when the Close button is pressed', () => {
    const onClose = vi.fn()
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="v1"
        pages={[]}
        skipping={false}
        onGo={noop}
        onClose={onClose}
      />,
    )
    const button = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    )
    button?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('docks as a panel between the bars, its look in the stylesheet', () => {
    const host = mount(
      <CreditsPanel
        visible={true}
        work="work"
        edition="raw"
        pages={[3]}
        skipping={false}
        onGo={noop}
        onClose={noop}
      />,
    )
    const panel = host.querySelector('[role="dialog"]') as HTMLElement
    expect(panel.className).toBe('docked-panel')
    expect(panel.getAttribute('style')).toBeNull()
    expect(host.querySelector('img')?.className).toBe('reader-thumb')
  })
})

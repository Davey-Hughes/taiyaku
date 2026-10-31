/**
 * The banner a reader confirms an irreversible write from.
 *
 * Two states with almost nothing in common, and a consequence sentence whose
 * two branches say opposite things — which is why this is worth mounting
 * rather than reading. The `disabled` pair stands between a reader and a
 * second concurrent write against one validator.
 */
import { act } from 'react'
import { describe, expect, it } from 'vitest'
import {
  CorrectionBanner,
  type CorrectionBannerProps,
} from '../src/CorrectionBanner'
import type { Pending } from '../src/correction'
import { mount } from './support/mount'

const picking: Pending = { from: 'raw', to: 'en', a: { page: 3 } }
const picked: Pending = { ...picking, b: { page: 4 } }

function props(
  over: Partial<CorrectionBannerProps> = {},
): CorrectionBannerProps {
  return {
    pending: picking,
    displaced: undefined,
    notice: undefined,
    submitting: false,
    onConfirm: () => undefined,
    onCancel: () => undefined,
    ...over,
  }
}

const buttons = (host: HTMLElement) => [
  ...host.querySelectorAll<HTMLButtonElement>('button'),
]

describe('CorrectionBanner, before a counterpart is picked', () => {
  it('asks for a page and says what backs out', () => {
    const host = mount(<CorrectionBanner {...props()} />)
    expect(host.textContent).toContain('Pick the')
    expect(host.textContent).toContain('en')
    // 1-based, through `describePosition` — `a.page` is 3 internally.
    expect(host.textContent).toContain('page 4')
    expect(host.textContent).toContain('Escape')
  })

  /**
   * There is nothing to confirm yet, and a Confirm button that did nothing
   * would be worse than none: `confirmCorrection` refuses without a `b`, so a
   * reader could press it and watch nothing happen.
   */
  it('offers no controls at all', () => {
    const host = mount(<CorrectionBanner {...props()} />)
    expect(buttons(host)).toHaveLength(0)
  })

  it('is a status region, not an alert', () => {
    const host = mount(<CorrectionBanner {...props()} />)
    expect(host.querySelector('[role="status"]')).not.toBeNull()
  })
})

describe('CorrectionBanner, once a counterpart is picked', () => {
  /**
   * The assertion, restated at the moment of confirming. Without it the banner
   * would swap the prompt for the consequence line alone, so the one screen a
   * reader commits from would never say what was being asserted, and the
   * picked tile can scroll out of view in the strip.
   */
  it('restates what is being asserted, both sides of it', () => {
    const host = mount(<CorrectionBanner {...props({ pending: picked })} />)
    const text = host.textContent ?? ''
    expect(text).toContain('Pair')
    expect(text).toContain('raw')
    expect(text).toContain('page 4')
    expect(text).toContain('en')
    expect(text).toContain('page 5')
  })

  /** The 1:1 rule: asserting `a` pairs with `b` also asserts that whatever holds `b` today does not. */
  it('names the slot that loses its counterpart', () => {
    const host = mount(
      <CorrectionBanner
        {...props({ pending: picked, displaced: { page: 6, half: 'L' } })}
      />,
    )
    expect(host.textContent).toContain(
      'raw page 7 L will lose its en counterpart.',
    )
  })

  /**
   * **The two branches say opposite things, and the second is shown rather
   * than omitted.** A reader who saw a warning sometimes and nothing other
   * times would have to work out whether the absence of a warning was itself
   * the warning.
   */
  it('says so plainly when nothing is displaced', () => {
    const host = mount(<CorrectionBanner {...props({ pending: picked })} />)
    expect(host.textContent).toContain('Nothing loses its counterpart')
    expect(host.textContent).toContain('pairs with nothing today')
    expect(host.textContent).not.toContain('will lose its')
  })

  it('shows a notice beside the controls when there is one', () => {
    const host = mount(
      <CorrectionBanner
        {...props({
          pending: picked,
          notice: 'This work changed since it was read',
        })}
      />,
    )
    expect(host.textContent).toContain('This work changed since it was read')
  })

  it('shows no notice line when there is none', () => {
    const host = mount(<CorrectionBanner {...props({ pending: picked })} />)
    expect(host.querySelectorAll('p')).toHaveLength(2)
  })

  it('offers Confirm and Cancel, both live', () => {
    const host = mount(<CorrectionBanner {...props({ pending: picked })} />)
    const [confirm, cancel] = buttons(host)
    expect(confirm.textContent).toBe('Confirm')
    expect(cancel.textContent).toBe('Cancel')
    expect(confirm.disabled).toBe(false)
    expect(cancel.disabled).toBe(false)
  })

  it('calls back on each', () => {
    let confirmed = 0
    let cancelled = 0
    const host = mount(
      <CorrectionBanner
        {...props({
          pending: picked,
          onConfirm: () => confirmed++,
          onCancel: () => cancelled++,
        })}
      />,
    )
    const [confirm, cancel] = buttons(host)
    act(() => confirm.click())
    act(() => cancel.click())
    expect(confirmed).toBe(1)
    expect(cancelled).toBe(1)
  })

  /**
   * **Both, not just Confirm.** Cancel is the subtler one — it is the control
   * that most looks like it undoes the write, and it is the one that cannot.
   */
  it('stands both controls down while the write is on the wire', () => {
    const host = mount(
      <CorrectionBanner {...props({ pending: picked, submitting: true })} />,
    )
    const [confirm, cancel] = buttons(host)
    expect(confirm.disabled).toBe(true)
    expect(cancel.disabled).toBe(true)
    expect(confirm.textContent).toBe('Writing…')
  })

  it('does not fire while disabled', () => {
    let confirmed = 0
    const host = mount(
      <CorrectionBanner
        {...props({
          pending: picked,
          submitting: true,
          onConfirm: () => confirmed++,
        })}
      />,
    )
    act(() => buttons(host)[0].click())
    expect(confirmed).toBe(0)
  })
})

describe('CorrectionBanner, where it stands', () => {
  /** Spec 2026-09-17 §8.3: the top bar is not drawn while correcting, and this takes its place. */
  it('takes the top bar’s place: the reader’s top edge, full width, over the page', () => {
    const host = mount(<CorrectionBanner {...props()} />)
    const banner = host.firstElementChild as HTMLElement
    expect(banner.className).toBe('reader-banner')
    expect([banner.style.top, banner.style.left, banner.style.right]).toEqual([
      '0px',
      '0px',
      '0px',
    ])
    expect(Number(banner.style.zIndex)).toBe(3)
    expect(banner.style.padding).toContain('var(--safe-top)')
  })

  it('gives its buttons the reader’s button look', () => {
    const host = mount(<CorrectionBanner {...props({ pending: picked })} />)
    for (const b of buttons(host)) expect(b.className).toBe('reader-button')
  })
})

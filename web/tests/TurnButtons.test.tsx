import { describe, expect, it, vi } from 'vitest'
import { mount } from './support/mount'
import { TurnButtons } from '../src/TurnButtons'

const turns = (host: HTMLElement) =>
  [...host.querySelectorAll('button')].map((b) => ({
    name: b.textContent,
    left: b.style.left,
    right: b.style.right,
  }))

describe('TurnButtons (spec 2026-09-26 §3.4)', () => {
  it('names the right third Next in a left-to-right work, and lists Next first', () => {
    const host = mount(<TurnButtons reading="ltr" onStep={() => {}} />)
    expect(turns(host)).toEqual([
      { name: 'Next page', left: '', right: '0px' },
      { name: 'Previous page', left: '0px', right: '' },
    ])
  })

  it('names the left third Next in a right-to-left work', () => {
    const host = mount(<TurnButtons reading="rtl" onStep={() => {}} />)
    expect(turns(host)).toEqual([
      { name: 'Next page', left: '0px', right: '' },
      { name: 'Previous page', left: '', right: '0px' },
    ])
  })

  /** Physical, as the arrow keys are: `readerIntent` mirrors it through `stepFor`. */
  it('steps by the physical side it stands on', () => {
    const onStep = vi.fn()
    const host = mount(<TurnButtons reading="rtl" onStep={onStep} />)
    const [next, previous] = [...host.querySelectorAll('button')]
    next.click()
    previous.click()
    expect(onStep.mock.calls).toEqual([[-1], [1]])
  })

  it('stays out of the Tab order and out of a pointer’s road', () => {
    const host = mount(<TurnButtons reading="ltr" onStep={() => {}} />)
    for (const b of host.querySelectorAll('button')) {
      expect(b.tabIndex).toBe(-1)
      expect(b.style.pointerEvents).toBe('none')
    }
  })
})

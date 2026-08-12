import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { LabelBox } from './labelling'
import { RegionOverlay } from './RegionOverlay'
import { mount } from './testing/mount'

const boxes: LabelBox[] = [
  {
    id: 'a',
    x: 0.1,
    y: 0.2,
    w: 0.3,
    h: 0.4,
    kind: 'dialogue',
    axis: 'v',
    ignore: false,
  },
  {
    id: 'b',
    x: 0.5,
    y: 0.5,
    w: 0.2,
    h: 0.2,
    kind: 'sfx',
    axis: null,
    ignore: true,
  },
]

function region(host: HTMLElement, id: string): HTMLElement {
  return host.querySelector(`[data-testid="region-${id}"]`) as HTMLElement
}

describe('RegionOverlay', () => {
  it('positions each box in page fractions', () => {
    const host = mount(<RegionOverlay boxes={boxes} />)
    const drawn = region(host, 'a')
    expect(drawn.style.left).toBe('10%')
    expect(drawn.style.top).toBe('20%')
    expect(drawn.style.width).toBe('30%')
    expect(drawn.style.height).toBe('40%')
  })

  it('marks the selected box so it can be styled apart', () => {
    const host = mount(<RegionOverlay boxes={boxes} selectedId="b" />)
    expect(region(host, 'b').getAttribute('data-selected')).toBe('true')
    expect(region(host, 'a').getAttribute('data-selected')).toBe('false')
  })

  it('carries the kind and the ignore flag as data, not as colour alone', () => {
    // A colour difference is invisible to a test and to anyone who cannot see
    // it. The attribute is what both read.
    const host = mount(<RegionOverlay boxes={boxes} />)
    expect(region(host, 'b').getAttribute('data-kind')).toBe('sfx')
    expect(region(host, 'b').getAttribute('data-ignore')).toBe('true')
  })

  it('reports hover so the box can brighten out of the way of the artwork', () => {
    const onHover = vi.fn()
    const host = mount(<RegionOverlay boxes={boxes} onHover={onHover} />)
    const target = region(host, 'a')
    act(() => {
      target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    })
    expect(onHover).toHaveBeenCalledWith('a')
    act(() => {
      target.dispatchEvent(new MouseEvent('mouseout', { bubbles: true }))
    })
    expect(onHover).toHaveBeenCalledWith(undefined)
  })

  it('reports a click as a selection', () => {
    const onSelect = vi.fn()
    const host = mount(<RegionOverlay boxes={boxes} onSelect={onSelect} />)
    const target = region(host, 'a')
    act(() => {
      target.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })
    expect(onSelect).toHaveBeenCalledWith('a')
  })

  it('draws nothing at all when there are no boxes', () => {
    const host = mount(<RegionOverlay boxes={[]} />)
    expect(host.querySelectorAll('[data-testid^="region-"]')).toHaveLength(0)
  })
})

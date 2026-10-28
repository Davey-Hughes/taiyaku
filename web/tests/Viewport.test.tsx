import { describe, expect, it } from 'vitest'
import { mount } from './support/mount'
import { Viewport } from '../src/Viewport'

const transformOf = (host: HTMLElement) =>
  (host.firstElementChild as HTMLElement).style.transform

describe('Viewport', () => {
  it('scales by the reciprocal of the rect so it fills the box', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 0.5, h: 0.5 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('scale(2, 2)')
  })

  it('translates in the untransformed box’s own percent units', () => {
    const host = mount(
      <Viewport rect={{ x: 0.25, y: 0.1, w: 1, h: 1 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('translate(-25%, -10%)')
  })

  it('guards a zero width rather than dividing by zero', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 0, h: 0.5 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('scale(1, 2)')
  })

  it('guards a zero height rather than dividing by zero', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 0.5, h: 0 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('scale(2, 1)')
  })

  it('sets transform-origin to the top left so the terms compose in order', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 1, h: 1 }}>
        <span />
      </Viewport>,
    )
    expect((host.firstElementChild as HTMLElement).style.transformOrigin).toBe(
      '0 0',
    )
  })

  it('renders its children inside the transformed wrapper', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 1, h: 1 }}>
        <span data-testid="child" />
      </Viewport>,
    )
    expect(host.querySelector('[data-testid="child"]')).not.toBeNull()
  })
})

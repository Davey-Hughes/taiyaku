import { act, useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { useFitsPair } from '../src/fitsPair'
import { mount } from './support/mount'

// jsdom lays nothing out, so each element's width is stubbed onto it, and
// the observer is one the test fires itself. What a real window does is the
// browser project's to show.
class StubResizeObserver {
  static made: StubResizeObserver[] = []
  readonly changed: () => void
  readonly observed: Element[] = []
  constructor(changed: () => void) {
    this.changed = changed
    StubResizeObserver.made.push(this)
  }
  observe(target: Element) {
    this.observed.push(target)
  }
  unobserve() {}
  disconnect() {}
}

const held = globalThis.ResizeObserver
beforeEach(() => {
  StubResizeObserver.made = []
  globalThis.ResizeObserver =
    StubResizeObserver as unknown as typeof ResizeObserver
})
afterEach(() => {
  globalThis.ResizeObserver = held
})

/** The widths the stubbed layout gives the split and the probe. */
const widths = { split: 0, probe: 0 }

/**
 * The hook over a split holding a probe, drawing what it answered. Made here
 * because `useComponentExportOnlyModules` refuses an unexported component at
 * module scope, and `noExportsInTest` refuses the export.
 */
function fits() {
  function Fits() {
    const split = useRef<HTMLDivElement>(null)
    const probe = useRef<HTMLDivElement>(null)
    const answer = useFitsPair(split, probe)
    return (
      <div
        ref={(el) => {
          split.current = el
          if (el !== null) {
            Object.defineProperty(el, 'clientWidth', {
              configurable: true,
              get: () => widths.split,
            })
          }
        }}
        data-fits={String(answer)}
      >
        <div
          ref={(el) => {
            probe.current = el
            if (el !== null) {
              el.getBoundingClientRect = () =>
                ({ width: widths.probe }) as DOMRect
            }
          }}
        />
      </div>
    )
  }
  return <Fits />
}

const answer = (host: HTMLElement) =>
  host.querySelector('[data-fits]')?.getAttribute('data-fits')

/** The window was resized: the observer reports it. */
function resized() {
  act(() => {
    for (const observer of StubResizeObserver.made) observer.changed()
  })
}

describe('useFitsPair', () => {
  it('fits a split as wide as the probe, from the first measure', () => {
    widths.split = 992
    widths.probe = 992
    const host = mount(fits())
    expect(answer(host)).toBe('true')
  })

  it('does not fit a split narrower than the probe', () => {
    widths.split = 991
    widths.probe = 992
    const host = mount(fits())
    expect(answer(host)).toBe('false')
  })

  it('never fits a probe that measures nothing', () => {
    widths.split = 1200
    widths.probe = 0
    const host = mount(fits())
    expect(answer(host)).toBe('false')
  })

  it('measures again when the observer reports a change, of either element', () => {
    widths.split = 800
    widths.probe = 992
    const host = mount(fits())
    expect(answer(host)).toBe('false')
    const [observer] = StubResizeObserver.made
    expect(observer.observed).toEqual([
      host.firstElementChild,
      host.firstElementChild?.firstElementChild,
    ])
    widths.split = 1200
    resized()
    expect(answer(host)).toBe('true')
    widths.probe = 1300
    resized()
    expect(answer(host)).toBe('false')
  })

  it('does not fit without a ResizeObserver, however wide the split', () => {
    globalThis.ResizeObserver = undefined as unknown as typeof ResizeObserver
    widths.split = 1200
    widths.probe = 992
    const host = mount(fits())
    expect(answer(host)).toBe('false')
  })
})

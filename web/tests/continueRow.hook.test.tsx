import { act, useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useContinueRow } from '../src/continueRow'
import { mount, rerender } from './support/mount'

/**
 * `useContinueRow` reads the layout, which jsdom does not lay out, so
 * `Library.continue.browser.test.tsx` proves the row in a browser and the
 * coverage run never reaches it. These hand the hook what a browser would:
 * the list's computed tokens and padding, its box, a `ResizeObserver` the
 * test answers and a frame clock the test runs.
 */

let tokens: Record<string, string>
let boxWidth: number
let observers: Array<{ answer: () => void; observed: Element[] }>
let frames: Map<number, FrameRequestCallback>

/**
 * The list, `count` cards long, laid by the hook. Made in a function
 * because `useComponentExportOnlyModules` refuses an unexported component at
 * module scope, and `noExportsInTest` refuses the export.
 */
function rowOf() {
  function Row({
    drawn = true,
    size = 144,
    count = 12,
    attached = true,
  }: {
    drawn?: boolean
    size?: number
    count?: number
    attached?: boolean
  }) {
    const list = useRef<HTMLUListElement>(null)
    useContinueRow(list, drawn, size, count)
    return (
      <ul ref={attached ? list : undefined}>
        {Array.from({ length: count }, (_, i) => (
          <li key={i} />
        ))}
      </ul>
    )
  }
  return Row
}

const Row = rowOf()

const listIn = (host: HTMLElement) => host.querySelector('ul') as HTMLElement

/** The list's two properties as its inline style holds them. */
const laid = (ul: HTMLElement) => ({
  gap: ul.style.getPropertyValue('--continue-gap'),
  column: ul.style.getPropertyValue('--continue-col'),
})

/**
 * Answers each `ResizeObserver` watching something, as the browser does
 * after a change of size.
 */
const resized = () =>
  act(() => {
    for (const each of observers) each.answer()
  })

/** Draws a frame: runs the callbacks requested for it. */
const frame = () =>
  act(() => {
    const due = [...frames.values()]
    frames.clear()
    for (const run of due) run(0)
  })

beforeEach(() => {
  tokens = { '--tile-min': '144px', '--tile-gap': '16px', '--tile-fill': '0' }
  // 1008 px inside the list's 4 px of padding a side.
  boxWidth = 1016
  observers = []
  frames = new Map()
  vi.spyOn(window, 'getComputedStyle').mockImplementation(
    () =>
      ({
        getPropertyValue: (name: string) => tokens[name] ?? '',
        paddingLeft: '4px',
        paddingRight: '4px',
        borderLeftWidth: '0px',
        borderRightWidth: '0px',
      }) as unknown as CSSStyleDeclaration,
  )
  vi.spyOn(
    HTMLUListElement.prototype,
    'getBoundingClientRect',
  ).mockImplementation(() => ({ width: boxWidth }) as DOMRect)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observed: Element[] = []
      answer: () => void
      constructor(callback: ResizeObserverCallback) {
        this.answer = () => {
          if (this.observed.length === 0) return
          callback([], this as unknown as ResizeObserver)
        }
        observers.push(this)
      }
      observe(el: Element) {
        this.observed.push(el)
      }
      unobserve(el: Element) {
        this.observed = this.observed.filter((each) => each !== el)
      }
      disconnect() {
        observers = observers.filter((each) => each !== this)
      }
    },
  )
  let next = 0
  vi.stubGlobal('requestAnimationFrame', (run: FrameRequestCallback) => {
    next += 1
    frames.set(next, run)
    return next
  })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => {
    frames.delete(id)
  })
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('useContinueRow', () => {
  it('sets the spread gap and no column where the grid lays exact covers', () => {
    const ul = listIn(mount(<Row />))
    expect(laid(ul)).toEqual({ gap: '28.8px', column: '' })
    expect(observers.map((one) => one.observed)).toEqual([[ul]])
  })

  it('sets the filled column and no gap where the grid fills the row', () => {
    tokens['--tile-fill'] = ' 1'
    boxWidth = 366
    const ul = listIn(mount(<Row />))
    expect(laid(ul)).toEqual({ gap: '', column: '171px' })
  })

  it('lays the other layout on a resize, leaving nothing of the first', () => {
    const ul = listIn(mount(<Row />))
    tokens['--tile-fill'] = '1'
    boxWidth = 366
    resized()
    const filled = laid(ul)
    frame()
    tokens['--tile-fill'] = '0'
    boxWidth = 1016
    resized()
    expect({ filled, exact: laid(ul) }).toEqual({
      filled: { gap: '', column: '171px' },
      exact: { gap: '28.8px', column: '' },
    })
  })

  it('removes both where the gap or the track is not a px length', () => {
    const ul = listIn(mount(<Row />))
    tokens['--tile-gap'] = 'clamp(0.6rem, 1px, 1rem)'
    resized()
    const noGap = laid(ul)
    frame()
    tokens['--tile-gap'] = '16px'
    tokens['--tile-fill'] = '1'
    resized()
    frame()
    tokens['--tile-min'] = ''
    resized()
    expect({ noGap, noTrack: laid(ul) }).toEqual({
      noGap: { gap: '', column: '' },
      noTrack: { gap: '', column: '' },
    })
  })

  it('reads the size again when it changes, with no resize', () => {
    const ul = listIn(mount(<Row />))
    tokens['--tile-min'] = '240px'
    rerender(<Row size={240} />)
    // Four 240 px columns across 1008 px.
    expect(laid(ul).gap).toBe('16px')
  })

  it('lays the row in a document with no ResizeObserver', () => {
    vi.stubGlobal('ResizeObserver', undefined)
    const ul = listIn(mount(<Row />))
    expect(laid(ul)).toEqual({ gap: '28.8px', column: '' })
  })

  it('lays nothing while the row is not drawn, or has no list', () => {
    const hidden = listIn(mount(<Row drawn={false} />))
    const loose = listIn(mount(<Row attached={false} />))
    expect({
      hidden: laid(hidden),
      loose: laid(loose),
      observers: observers.length,
    }).toEqual({
      hidden: { gap: '', column: '' },
      loose: { gap: '', column: '' },
      observers: 0,
    })
  })

  it('marks the sides with cards past them, once a frame', () => {
    const ul = listIn(mount(<Row />))
    const scroll = { left: 0, width: 2000, client: 1008 }
    Object.defineProperties(ul, {
      scrollLeft: { get: () => scroll.left },
      scrollWidth: { get: () => scroll.width },
      clientWidth: { get: () => scroll.client },
    })
    resized()
    const atStart = [
      ul.hasAttribute('data-more-before'),
      ul.hasAttribute('data-more-after'),
    ]
    scroll.left = 500
    ul.dispatchEvent(new Event('scroll'))
    ul.dispatchEvent(new Event('scroll'))
    const requested = frames.size
    const beforeFrame = ul.hasAttribute('data-more-before')
    frame()
    const between = [
      ul.hasAttribute('data-more-before'),
      ul.hasAttribute('data-more-after'),
    ]
    scroll.left = 992
    ul.dispatchEvent(new Event('scroll'))
    frame()
    expect({
      atStart,
      requested,
      beforeFrame,
      between,
      atEnd: [
        ul.hasAttribute('data-more-before'),
        ul.hasAttribute('data-more-after'),
      ],
    }).toEqual({
      atStart: [false, true],
      requested: 1,
      beforeFrame: false,
      between: [true, true],
      atEnd: [true, false],
    })
  })

  it('stops watching the list while what it set is laid, and watches it again a frame later', () => {
    const ul = listIn(mount(<Row />))
    const watched = () => observers.flatMap((one) => one.observed)
    // A report that changes nothing leaves the list watched.
    resized()
    const unchanged = watched()
    tokens['--tile-fill'] = '1'
    boxWidth = 366
    resized()
    const changed = watched()
    frame()
    expect({ unchanged, changed, again: watched() }).toEqual({
      unchanged: [ul],
      changed: [],
      again: [ul],
    })
  })

  it('lets go of the list when it goes: the listener, the frames and the observer', () => {
    const ul = listIn(mount(<Row />))
    ul.dispatchEvent(new Event('scroll'))
    tokens['--tile-fill'] = '1'
    resized()
    const pending = frames.size
    rerender(<Row drawn={false} />)
    ul.dispatchEvent(new Event('scroll'))
    expect({
      pending,
      frames: frames.size,
      observers: observers.length,
    }).toEqual({
      pending: 2,
      frames: 0,
      observers: 0,
    })
  })
})

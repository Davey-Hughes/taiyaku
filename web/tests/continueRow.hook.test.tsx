import { act, useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useContinueRow } from '../src/continueRow'
import { mount, rerender } from './support/mount'

/**
 * `useContinueRow` reads the layout, which jsdom does not lay out, so
 * `Library.continue.browser.test.tsx` proves the row in a browser and the
 * coverage run never reaches it. These hand the hook what a browser would:
 * the list's computed tokens and padding, its box, its cards' edges, its
 * scroll range, a `ResizeObserver` the test answers and a frame clock the
 * test runs.
 */

let tokens: Record<string, string>
let boxWidth: number
/** How far each card's edge lies past the one before it: the row's pitch. */
let step: number
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

type Scroll = { left: number; width: number; client: number }

/**
 * Gives the list the scroll range `scroll` holds, which the test changes.
 * A `scrollLeft` set from code is clamped to the range, as a browser
 * clamps it.
 */
function scrollable(ul: HTMLElement, scroll: Scroll): Scroll {
  Object.defineProperties(ul, {
    scrollLeft: {
      get: () => scroll.left,
      set: (to: number) => {
        scroll.left = Math.max(0, Math.min(to, scroll.width - scroll.client))
      },
    },
    scrollWidth: { get: () => scroll.width },
    clientWidth: { get: () => scroll.client },
  })
  return scroll
}

/**
 * Scrolls the list to `left`, as a user does, and draws the frame that
 * marks it.
 */
function scrolledTo(ul: HTMLElement, scroll: Scroll, left: number) {
  scroll.left = left
  ul.dispatchEvent(new Event('scroll'))
  frame()
}

/**
 * Lays the cards `pitch` apart and gives the list that layout's box and
 * scroll range, for a resize or a size change the test then makes. A range
 * that shrinks clamps the row into it, as a browser's layout does.
 */
function layAt(scroll: Scroll, pitch: number, width: number, client: number) {
  step = pitch
  boxWidth = client
  scroll.width = width
  scroll.client = client
  scroll.left = Math.max(0, Math.min(scroll.left, width - client))
}

/**
 * The list of `count` cards `pitch` apart, scrolled to `left` of the
 * range `width` and `client` make, and marked there.
 */
function placed({
  count = 12,
  pitch,
  width,
  client,
  left,
}: {
  count?: number
  pitch: number
  width: number
  client: number
  left: number
}) {
  step = pitch
  boxWidth = client
  const ul = listIn(mount(<Row count={count} />))
  const scroll = scrollable(ul, { left: 0, width, client })
  scrolledTo(ul, scroll, left)
  return { ul, scroll }
}

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
  step = 0
  vi.spyOn(HTMLLIElement.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: HTMLLIElement) {
      const cards = [...(this.parentElement?.children ?? [])]
      return { left: cards.indexOf(this) * step } as DOMRect
    },
  )
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
    const scroll = scrollable(ul, { left: 0, width: 2000, client: 1008 })
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

  it('keeps the card at its start edge across a resize, the same fraction past its edge', () => {
    // Two and a half cards along.
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 400,
    })
    layAt(scroll, 200, 2400, 1200)
    resized()
    expect(ul.scrollLeft).toBeCloseTo(500)
  })

  it('keeps its end across a resize', () => {
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 920,
    })
    layAt(scroll, 200, 2400, 1200)
    resized()
    expect(ul.scrollLeft).toBe(1200)
  })

  it('scales the offset into the start card to the new pitch on a size change', () => {
    // Half a card past the third card's edge: 80 px at this pitch.
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 400,
    })
    layAt(scroll, 180, 2160, 1000)
    rerender(<Row size={164} />)
    expect(ul.scrollLeft).toBe(450)
  })

  it('never counts a row that fits as at its end', () => {
    const { ul, scroll } = placed({
      count: 6,
      pitch: 160,
      width: 1000,
      client: 1000,
      left: 0,
    })
    layAt(scroll, 200, 1200, 1000)
    rerender(<Row count={6} size={200} />)
    expect(ul.scrollLeft).toBe(0)
  })

  it('drops a kept place when the number of cards changes, before a size change (O301’s sequence)', () => {
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 920,
    })
    // Down to a size at which the row fits, which clamps it to its start.
    layAt(scroll, 80, 960, 1000)
    rerender(<Row size={96} />)
    const fitted = ul.scrollLeft
    // A card more: the row overflows again, still at its start.
    layAt(scroll, 80, 1040, 1000)
    rerender(<Row size={96} count={13} />)
    layAt(scroll, 160, 2080, 1000)
    rerender(<Row size={144} count={13} />)
    expect({ fitted, left: ul.scrollLeft }).toEqual({ fitted: 0, left: 0 })
  })

  it('drops a kept place when the number of cards changes in the same commit as the size', () => {
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 920,
    })
    // Down to a size at which the row fits, which clamps it to its start.
    layAt(scroll, 80, 960, 1000)
    rerender(<Row size={96} />)
    const fitted = ul.scrollLeft
    // A card more and the size back up, in one commit.
    layAt(scroll, 160, 2080, 1000)
    rerender(<Row size={144} count={13} />)
    expect({ fitted, left: ul.scrollLeft }).toEqual({ fitted: 0, left: 0 })
  })

  it('drops a kept place when the number of cards changes, before a resize', () => {
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 920,
    })
    // Down to a size at which the row fits, which clamps it to its start.
    layAt(scroll, 80, 960, 1000)
    rerender(<Row size={96} />)
    // A card more: the row overflows again, still at its start.
    layAt(scroll, 80, 1040, 1000)
    rerender(<Row size={96} count={13} />)
    // A wider list, over which the row still overflows.
    layAt(scroll, 100, 1300, 1200)
    resized()
    expect(ul.scrollLeft).toBe(0)
  })

  it.each([
    ['the card at its start edge', 400, 500],
    ['its end', 920, 1600],
  ] as const)(
    'keeps %s as last seen through a size change in the same commit as a change in the number of cards',
    (_, left, expected) => {
      // Two and a half cards along, or the end.
      const { ul, scroll } = placed({
        pitch: 160,
        width: 1920,
        client: 1000,
        left,
      })
      // A range that grows, so a row left where it was is short of both.
      layAt(scroll, 200, 2600, 1000)
      rerender(<Row size={200} count={13} />)
      expect(ul.scrollLeft).toBe(expected)
    },
  )

  it('keeps its end as last seen where a resize clamped the row to it, through a size change in the same commit as a change in the number of cards', () => {
    // Five and a half cards along, short of the end.
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 880,
    })
    // A wider list, over which five and a half cards lie past the end.
    layAt(scroll, 200, 2400, 1500)
    resized()
    const clamped = ul.scrollLeft
    // A range that grows, so a row left where it was is short of its end.
    layAt(scroll, 240, 3120, 1000)
    rerender(<Row size={240} count={13} />)
    expect({ clamped, left: ul.scrollLeft }).toEqual({
      clamped: 900,
      left: 2120,
    })
  })

  it('keeps a place scrolled to between two resizes', () => {
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 400,
    })
    layAt(scroll, 200, 2400, 1200)
    resized()
    // Three cards along at this pitch.
    scrolledTo(ul, scroll, 600)
    layAt(scroll, 160, 1920, 1000)
    resized()
    expect(ul.scrollLeft).toBe(480)
  })

  it('keeps the place through a scroll marked at a width the row is not yet laid for', () => {
    // Five and a half cards along, short of the end.
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 880,
    })
    // A wider list still at the old pitch, which clamps the row to its end,
    // and a scroll marked in that frame before the observer reports it.
    layAt(scroll, 160, 1920, 1200)
    ul.dispatchEvent(new Event('scroll'))
    frame()
    const clamped = ul.scrollLeft
    layAt(scroll, 200, 2400, 1200)
    resized()
    expect({ clamped, left: ul.scrollLeft }).toEqual({
      clamped: 720,
      left: 1100,
    })
  })

  it('marks a scroll at a new width where no ResizeObserver lays the row again', () => {
    vi.stubGlobal('ResizeObserver', undefined)
    // Two and a half cards along.
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 400,
    })
    // A wider list, which nothing lays again, scrolled three cards along.
    layAt(scroll, 160, 1920, 1200)
    scrolledTo(ul, scroll, 480)
    // A size change puts the row back from the place last marked.
    layAt(scroll, 200, 2400, 1200)
    rerender(<Row size={184} />)
    expect(ul.scrollLeft).toBe(600)
  })

  it('keeps a resize’s place through a cover-size change after it', () => {
    const { ul, scroll } = placed({
      pitch: 160,
      width: 1920,
      client: 1000,
      left: 400,
    })
    layAt(scroll, 200, 2400, 1200)
    resized()
    layAt(scroll, 220, 2640, 1200)
    rerender(<Row size={164} />)
    // Two and a half cards along at this pitch.
    expect(ul.scrollLeft).toBe(550)
  })
})

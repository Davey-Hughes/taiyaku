import { act, type CSSProperties, useRef } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { barLine } from '../src/phoneBar'
import { rowLaid } from '../src/reseatTiming'
import { useScrollerSwap } from '../src/scrollerSwap'
import { mount, rerender } from './support/mount'

// No bar's script runs here, so the line is the bar's box, unless a case
// gives the line the script would.
vi.mock('../src/phoneBar', () => ({ barLine: vi.fn(() => null) }))

/**
 * `useScrollerSwap` reads the layout, which jsdom does not lay out, so
 * `scrollKeeper.browser.test.tsx` proves it in a browser and the coverage
 * run never reaches it. These hand the hook a page laid out by arithmetic
 * in two layouts: beside the sidebar `<main>` scrolls from the window's
 * top; in the phone layout the document scrolls, under a 52 px bar.
 */

type Layout = 'sidebar' | 'phone'

const BAR = 52
const VIEW = 800
const GAP = 20
/** Each layout's covers, and the room above the list and the heading's top. */
const LAID = {
  sidebar: { columns: 4, height: 200, above: 100, head: 16, width: 1000 },
  phone: { columns: 3, height: 250, above: 120, head: 24, width: 880 },
}

let layout: Layout
let scroll: { main: number; doc: number }
/**
 * Whether the bar goes up with the document's scroll, as on a track the
 * hook has not set.
 */
let barCarried: boolean
let observers: Array<{ answer: () => void; observed: Element[] }>
let frames: Map<number, FrameRequestCallback>

const main = () => document.getElementById('content') as HTMLElement
const docScroller = () =>
  (document.scrollingElement ?? document.documentElement) as HTMLElement

/** `<main>`'s content's height with `count` covers, in the layout drawn. */
function heightOf(count: number) {
  const { columns, height, above } = LAID[layout]
  return above + Math.ceil(count / columns) * (height + GAP) + 16
}

/** The room the element that scrolls has, in the layout drawn. */
const room = () =>
  layout === 'sidebar'
    ? heightOf(main().querySelectorAll('li').length) - VIEW
    : BAR + heightOf(main().querySelectorAll('li').length) - VIEW

/** Where `<main>`'s content starts in the window. */
const origin = () => (layout === 'sidebar' ? -scroll.main : BAR - scroll.doc)

/** An element's box in `<main>`'s content, by arithmetic. */
function boxOf(el: Element): { y: number; h: number } | undefined {
  const { columns, height, above, head } = LAID[layout]
  if (el.classList.contains('head')) return { y: head, h: 60 }
  if (el.tagName === 'UL') {
    return { y: above - 8, h: heightOf(el.children.length) - above + 8 }
  }
  if (el.tagName === 'LI') {
    const index = [...(el.parentElement?.children ?? [])].indexOf(el)
    return {
      y: above + Math.floor(index / columns) * (height + GAP),
      h: height,
    }
  }
  return undefined
}

function rectOf(el: Element): DOMRect {
  // The root is the viewport's size in both layouts (`base.css`).
  if (el === document.documentElement) {
    return {
      top: 0,
      bottom: innerHeight,
      width: innerWidth,
      height: innerHeight,
    } as DOMRect
  }
  if (!el.isConnected) return { top: 0, bottom: 0, height: 0 } as DOMRect
  if (el.id === 'content') {
    const height =
      layout === 'sidebar' ? VIEW : heightOf(el.querySelectorAll('li').length)
    const top = layout === 'sidebar' ? 0 : BAR - scroll.doc
    return {
      top,
      bottom: top + height,
      height,
      width: LAID[layout].width,
    } as DOMRect
  }
  if (el.classList.contains('phone-bar')) {
    if (layout !== 'phone') return { top: 0, bottom: 0, height: 0 } as DOMRect
    const top = barCarried ? -scroll.doc : 0
    return { top, bottom: top + BAR, height: BAR } as DOMRect
  }
  // A visually hidden heading, placed outside `<main>`'s scroll.
  if (el.tagName === 'H1') return { top: 30, bottom: 31, height: 1 } as DOMRect
  const box = boxOf(el)
  if (box === undefined) return { top: 0, bottom: 0, height: 0 } as DOMRect
  const top = origin() + box.y
  return { top, bottom: top + box.h, height: box.h } as DOMRect
}

/** The scroll offset the stub reports for each scroller, clamped to the room. */
function scrollable() {
  const clamp = (y: number) => Math.max(0, Math.min(y, room()))
  Object.defineProperty(main(), 'scrollTop', {
    configurable: true,
    get: () => (layout === 'sidebar' ? scroll.main : 0),
    set: (y: number) => {
      if (layout === 'sidebar') scroll.main = clamp(y)
    },
  })
  Object.defineProperty(docScroller(), 'scrollTop', {
    configurable: true,
    get: () => (layout === 'phone' ? scroll.doc : 0),
    set: (y: number) => {
      if (layout === 'phone') scroll.doc = clamp(y)
    },
  })
}

/** Runs the callbacks requested for the next frame. */
const frame = () =>
  act(() => {
    const due = [...frames.values()]
    frames.clear()
    for (const run of due) run(0)
  })

/** The observer's report of the viewport's new size, after layout. */
const observed = () =>
  act(() => {
    for (const each of observers) each.answer()
  })

/** The reader scrolls to `y`, and the frame that sees it is drawn. */
function scrolledTo(y: number) {
  if (layout === 'sidebar') scroll.main = y
  else scroll.doc = y
  main().dispatchEvent(new Event('scroll'))
  frame()
}

/** The window's `resize` event, which a window's frame fires before its observers. */
const resizeEvent = () => window.dispatchEvent(new Event('resize'))

/**
 * The window resized into `to`: the new scroller starts at its top, as a
 * browser's does, and the observer runs.
 */
function swapped(to: Layout) {
  layout = to
  scroll = { main: 0, doc: 0 }
  vi.stubGlobal('innerWidth', to === 'sidebar' ? 1280 : 880)
  resizeEvent()
  observed()
}

/** How far below the view's top the cover keyed `key` is laid out. */
function offsetOf(key: string) {
  const li = main().querySelector(`[data-key="${key}"]`) as HTMLElement
  const line = layout === 'sidebar' ? 0 : BAR
  return rectOf(li).top - line
}

/** The scroller's offset in the layout drawn. */
const scrolled = () => (layout === 'sidebar' ? scroll.main : scroll.doc)

/**
 * The shell's frame around a page of covers keyed from `from`. Made in a
 * function for the reason `continueRow.hook.test.tsx`'s `rowOf` gives.
 */
function pageOf() {
  function Page({
    from = 0,
    count = 60,
    attached = true,
    coverSize = 144,
  }: {
    from?: number
    count?: number
    attached?: boolean
    coverSize?: number
  }) {
    const content = useRef<HTMLElement>(null)
    useScrollerSwap(content)
    return (
      <div
        className="library"
        style={{ '--cover-n': coverSize } as CSSProperties}
      >
        <header className="phone-bar" />
        <main id="content" ref={attached ? content : undefined}>
          <svg aria-hidden="true" />
          <h1>Library</h1>
          <div className="head" />
          {count > 0 && (
            <ul className="grid">
              {Array.from({ length: count }, (_, i) => (
                <li key={from + i} data-key={from + i} />
              ))}
            </ul>
          )}
        </main>
      </div>
    )
  }
  return Page
}

const Page = pageOf()

/** A `ResizeObserver` the test answers, as the browser does after layout. */
class Observer {
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
  disconnect() {
    observers = observers.filter((each) => each !== this)
  }
}

/** The page as drawn, after the observer's first report. */
function drawn(props: Parameters<typeof Page>[0] = {}) {
  mount(<Page {...props} />)
  scrollable()
  observed()
}

beforeEach(() => {
  layout = 'sidebar'
  scroll = { main: 0, doc: 0 }
  barCarried = false
  observers = []
  frames = new Map()
  vi.stubGlobal('innerWidth', 1280)
  vi.stubGlobal('innerHeight', VIEW)
  vi.spyOn(window, 'getComputedStyle').mockImplementation(
    (el: Element) =>
      ({
        overflowY:
          el.id === 'content' && layout === 'sidebar' ? 'auto' : 'visible',
        position: el.tagName === 'H1' ? 'absolute' : 'static',
      }) as CSSStyleDeclaration,
  )
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
    function (this: Element) {
      return rectOf(this)
    },
  )
  vi.stubGlobal('ResizeObserver', Observer)
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

describe('useScrollerSwap', () => {
  it('puts the first cover on the top row seen back where it stood, into the phone layout and back', () => {
    drawn()
    // Row 4 of four columns, from 980 to 1180: cover 16 leads it.
    scrolledTo(1000)
    swapped('phone')
    const phone = { at: scrolled(), off: offsetOf('16') }
    swapped('sidebar')
    expect({ phone, sidebar: { at: scrolled(), off: offsetOf('16') } }).toEqual(
      {
        // A tenth of the way down it in both: 200 px tall beside the
        // sidebar, 250 px in the phone layout.
        phone: { at: 1495, off: -25 },
        sidebar: { at: 1000, off: -20 },
      },
    )
  })

  // The list's `change` can come after the put-back, so the bar's script
  // has not started and its track is the CSS's one bar tall: the line is
  // the bar's box, which the put-back's own scroll carries off.
  it('puts the place back in one write where the line is a bar the put-back’s scroll carries', () => {
    barCarried = true
    drawn()
    scrolledTo(1000)
    swapped('phone')
    expect(scrolled()).toBe(1495)
  })

  it.each([
    [
      'the next row’s first cover, where the top falls between two rows, with its top at the top',
      1190,
      '20',
      0,
    ],
    [
      'the first cover, where the top falls above the covers in their list, with its top at the top',
      96,
      '0',
      0,
    ],
    ['the deepest element the top crosses, outside a list', 30, 'head', -14],
  ])('keeps %s', (_, y, key, off) => {
    drawn()
    scrolledTo(y)
    swapped('phone')
    const at =
      key === 'head'
        ? (main().querySelector('.head') as HTMLElement).getBoundingClientRect()
            .top - BAR
        : offsetOf(key)
    expect(at).toBe(off)
  })

  it('puts a page at its top back at its top, where its first element stands lower', () => {
    drawn()
    swapped('phone')
    const phone = scrolled()
    swapped('sidebar')
    expect([phone, scrolled()]).toEqual([0, 0])
  })

  it('rests at the end where the layout cannot scroll that far, and puts the place back on the way back', () => {
    layout = 'phone'
    vi.stubGlobal('innerWidth', 880)
    drawn()
    // Row 17 of three columns crosses the top at the end: cover 51 leads it.
    scrolledTo(room())
    const end = { at: scrolled(), off: offsetOf('51') }
    swapped('sidebar')
    const sidebar = { atEnd: scrolled() === room(), off: offsetOf('51') }
    swapped('phone')
    expect({
      end,
      sidebar,
      phone: { at: scrolled(), off: offsetOf('51') },
    }).toEqual({
      end: { at: 4788, off: -78 },
      sidebar: { atEnd: true, off: 124 },
      phone: { at: 4788, off: -78 },
    })
  })

  it.each([
    ['a resize its observer has not run for', false],
    [
      'a second resize its observer has not run for, the frame after a first',
      true,
    ],
  ])('reads no layout at a frame after %s', (_, twice) => {
    drawn()
    scrolledTo(1000)
    if (twice) {
      vi.stubGlobal('innerWidth', 1240)
      resizeEvent()
      frame()
    }
    vi.stubGlobal('innerWidth', 1200)
    resizeEvent()
    main().dispatchEvent(new Event('scroll'))
    const box = vi.mocked(Element.prototype.getBoundingClientRect)
    const style = vi.mocked(window.getComputedStyle)
    box.mockClear()
    style.mockClear()
    frame()
    expect([box.mock.calls.length, style.mock.calls.length]).toEqual([0, 0])
  })

  // A window fires the event before its observers; in a frame the observer
  // can run first.
  it.each([
    ['before', true],
    ['after', false],
  ])(
    'takes a place from a scroll once its observer has run, the resize event %s it',
    (_, first) => {
      drawn()
      scrolledTo(1000)
      vi.stubGlobal('innerWidth', 1200)
      if (first) resizeEvent()
      observed()
      if (!first) resizeEvent()
      // Six tenths of the way down cover 16.
      scrolledTo(1100)
      swapped('phone')
      expect(offsetOf('16')).toBe(-150)
    },
  )

  it('takes the place a scroll leaves a frame or two after a resize its observer never runs for', () => {
    drawn()
    scrolledTo(1000)
    // A toolbar retracting changes the window's height and not the root's.
    vi.stubGlobal('innerHeight', 700)
    resizeEvent()
    // The frame the window resized in, and the one after.
    frame()
    frame()
    // Six tenths of the way down cover 16.
    scrolledTo(1100)
    swapped('phone')
    expect(offsetOf('16')).toBe(-150)
  })

  it('takes no place in a layout swapped in before its observer has put the place back', () => {
    drawn()
    scrolledTo(1000)
    // Swapped with no resize event yet, as a frame's first read can see.
    layout = 'phone'
    scroll = { main: 0, doc: 0 }
    vi.stubGlobal('innerWidth', 880)
    main().dispatchEvent(new Event('scroll'))
    frame()
    observed()
    expect([scrolled(), offsetOf('16')]).toEqual([1495, -25])
  })

  it('keeps the place across its own put-back, so the way back finds the same cover', () => {
    drawn()
    scrolledTo(1000)
    swapped('phone')
    // The put-back's own scroll, seen at the next frame: cover 16 is not
    // the first on its row here, and a place taken now would be cover 15.
    main().dispatchEvent(new Event('scroll'))
    frame()
    swapped('sidebar')
    expect([scrolled(), offsetOf('16')]).toEqual([1000, -20])
  })

  it('puts a cover whose top stands below the top at the top across the swap', () => {
    drawn()
    // In the gap between rows 3 and 4: cover 16 leads row 4, 10 px down.
    scrolledTo(970)
    swapped('phone')
    expect(offsetOf('16')).toBe(0)
  })

  it.each([
    ['a narrower window beside the sidebar', 'sidebar', 'innerWidth', 1200],
    ['a shorter window beside the sidebar', 'sidebar', 'innerHeight', 700],
    ['a narrower window in the phone layout', 'phone', 'innerWidth', 844],
  ] as const)(
    'puts nothing back at %s, which keeps the layout',
    (_, at, side, size) => {
      layout = at
      vi.stubGlobal('innerWidth', LAID[at].width)
      drawn()
      scrolledTo(1000)
      // Moved since the last frame, as a scroll mid-resize is.
      if (at === 'sidebar') scroll.main = 1100
      else scroll.doc = 1100
      vi.stubGlobal(side, size)
      resizeEvent()
      observed()
      expect(scrolled()).toBe(1100)
    },
  )

  describe('at a phone turn, which keeps the layout', () => {
    /** On its side, 880 × 800, where the view's orientation can be asked. */
    beforeEach(() => {
      layout = 'phone'
      vi.stubGlobal('innerWidth', LAID.phone.width)
      vi.stubGlobal('matchMedia', (query: string) => ({
        matches:
          query === '(orientation: portrait)' && innerHeight > innerWidth,
      }))
    })

    /**
     * Turned upright, with the scroll where iOS sets it, at the top, before
     * the resize event and the observer.
     */
    function turned() {
      vi.stubGlobal('innerWidth', VIEW)
      vi.stubGlobal('innerHeight', LAID.phone.width)
      scroll.doc = 0
      resizeEvent()
      observed()
    }

    it('puts the place back', () => {
      drawn()
      // Row 3 of three columns crosses the top: cover 9 leads it.
      scrolledTo(1000)
      turned()
      expect([scrolled(), offsetOf('9')]).toEqual([1000, -70])
    })

    // Half the bar held through the turn, and near the top the track holds
    // the bar whole, so its foot moves with the scroll there.
    it('puts the place under the bar’s foot where the put-back’s own scroll moves the foot', () => {
      vi.mocked(barLine).mockImplementation(() =>
        Math.max(26, BAR - scroll.doc),
      )
      onTestFinished(() => {
        vi.mocked(barLine).mockImplementation(() => null)
      })
      drawn()
      // The line at 26 crosses row 3, which cover 9 leads, 44 px down it.
      scrolledTo(1000)
      turned()
      expect([scrolled(), barLine()]).toEqual([1000, 26])
    })

    it('puts a cover whose top stands below the top back as far below it', () => {
      drawn()
      // In the gap between rows 3 and 4: cover 12 leads row 4, 10 px down.
      scrolledTo(1190)
      turned()
      expect([scrolled(), offsetOf('12')]).toEqual([1190, 10])
    })

    // A desktop window dragged across square stands upright and back, and
    // its screen faces as it did.
    it.each([
      ['puts the place back where the screen turns', 'portrait-primary', 1000],
      [
        'puts nothing back where the screen faces as it did',
        'landscape-primary',
        0,
      ],
    ])('%s', (_, to, expected) => {
      const orientation = { type: 'landscape-primary' }
      vi.stubGlobal('screen', { orientation })
      drawn()
      scrolledTo(1000)
      orientation.type = to
      turned()
      expect(scrolled()).toBe(expected)
    })

    it('takes the place again after a step in the covers’ size, and puts that back at the return', async () => {
      drawn()
      scrolledTo(1000)
      turned()
      // Taller covers: cover 9 moves 50 px down, and row 2, which cover 6
      // leads, now crosses the top at 0.8 of its height.
      LAID.phone.height += 50
      onTestFinished(() => {
        LAID.phone.height -= 50
      })
      rerender(<Page coverSize={150} />)
      await Promise.resolve()
      frame()
      // Turned back on its side, with the scroll where iOS sets it.
      vi.stubGlobal('innerWidth', LAID.phone.width)
      vi.stubGlobal('innerHeight', VIEW)
      scroll.doc = 0
      resizeEvent()
      observed()
      expect([scrolled(), offsetOf('6')]).toEqual([1000, -240])
    })

    // The row's observer runs after the shell's: a column there changes
    // its height, which moves the covers below it.
    it.each([
      ['in the frame it was put back in, puts it back again', 100, [1040, -70]],
      ['in a later frame, leaves it', 116, [1000, -30]],
    ])(
      'where the Continue row lays a new column above the place %s',
      (_, time, expected) => {
        const now = vi.spyOn(performance, 'now').mockReturnValue(100)
        drawn()
        scrolledTo(1000)
        turned()
        LAID.phone.above += 40
        onTestFinished(() => {
          LAID.phone.above -= 40
        })
        now.mockReturnValue(time)
        rowLaid()
        expect([scrolled(), offsetOf('9')]).toEqual(expected)
      },
    )
  })

  it('takes a new place at the frame after a change in the page moves the anchor', async () => {
    drawn()
    scrolledTo(1000)
    // Four covers fewer before it: cover 16 moves up a row, and cover 20
    // now leads the row the top crosses.
    rerender(<Page from={4} count={56} />)
    // The change is seen once the task that made it ends.
    await Promise.resolve()
    frame()
    swapped('phone')
    expect([offsetOf('20'), offsetOf('16')]).toEqual([-25, -295])
  })

  it('takes a new place where the anchor is gone by the swap', () => {
    drawn()
    // Cover 16 leads the row the top crosses, 170 px above the top.
    scrolledTo(1150)
    // Every cover drawn again under new keys, as another library's are,
    // and the window resized in the same task, before a frame sees it.
    rerender(<Page from={100} />)
    swapped('phone')
    expect(scrolled()).toBe(0)
  })

  it('asks for one frame however many scrolls and changes it sees, and lets go on unmount', async () => {
    const listen = vi.spyOn(window, 'addEventListener')
    const forget = vi.spyOn(window, 'removeEventListener')
    const onResize = (spy: typeof listen) =>
      spy.mock.calls.filter(([type]) => type === 'resize').map(([, f]) => f)
    drawn()
    const content = main()
    content.dispatchEvent(new Event('scroll'))
    content.append(document.createElement('p'))
    await Promise.resolve()
    content.dispatchEvent(new Event('scroll'))
    const asked = frames.size
    // A height alone, whose clearing frame the unmount cancels.
    vi.stubGlobal('innerHeight', 700)
    resizeEvent()
    rerender(null)
    const left = { frames: frames.size, observers: observers.length }
    // Unmounted with no frame asked for, so a listener left would ask.
    drawn()
    const again = main()
    rerender(null)
    document.dispatchEvent(new Event('scroll'))
    again.append(document.createElement('p'))
    await Promise.resolve()
    expect({
      asked,
      left,
      after: frames.size,
      resize: onResize(forget),
    }).toEqual({
      asked: 1,
      left: { frames: 0, observers: 0 },
      after: 0,
      resize: onResize(listen),
    })
  })

  it('does nothing where the window has no ResizeObserver, or `<main>` is not drawn', () => {
    vi.stubGlobal('ResizeObserver', undefined)
    mount(<Page />)
    document.dispatchEvent(new Event('scroll'))
    vi.stubGlobal('ResizeObserver', Observer)
    mount(<Page attached={false} />)
    document.dispatchEvent(new Event('scroll'))
    expect({ observers: observers.length, frames: frames.size }).toEqual({
      observers: 0,
      frames: 0,
    })
  })
})

import { act, useRef } from 'react'
import { MemoryRouter, type NavigateFunction, useNavigate } from 'react-router'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { barLine, usePhoneBar } from '../src/phoneBar'
import { PHONE_LAYOUT } from '../src/phoneLayout'
import { scrollPutBack } from '../src/reseatTiming'
import { useKeptScroll } from '../src/scrollKeeper'
import { mount, rerender } from './support/mount'

/**
 * `usePhoneBar` moves the track from the window's events and reads a
 * layout jsdom does not have, so these hand it what a phone would: the
 * scroll, the window's size, the page's height, the bar's 52 px, the phone
 * layout's list, a `ResizeObserver` and frames the case answers, and timers
 * the case runs. The motion itself is proved in `phoneBar.browser.test.tsx`.
 */

/** What the window and the page report, which each case sets. */
let page: {
  y: number
  width: number
  height: number
  scrollHeight: number
  barHeight: number
}
let phone: ReturnType<typeof layoutList>
let observers: Array<{ observed: Element[]; answer: () => void }>
let frames: Map<number, FrameRequestCallback>
/** What each stubbed property of the window held before the case. */
const saved = new Map<string, PropertyDescriptor | undefined>()

/** The phone layout's list: the case sets `matches` and says `change`. */
function layoutList() {
  const listeners = new Set<() => void>()
  return {
    matches: true,
    media: PHONE_LAYOUT,
    addEventListener: (type: string, listener: () => void) => {
      if (type === 'change') listeners.add(listener)
    },
    removeEventListener: (_: string, listener: () => void) => {
      listeners.delete(listener)
    },
    listening: () => listeners.size,
    /** Says `change`, as the browser does after `matches` has turned. */
    change: () => {
      for (const listener of [...listeners]) listener()
    },
  }
}

/** Stands `get` in for the window's own `name` until the case ends. */
function stubWindow(name: string, get: () => unknown) {
  if (!saved.has(name)) {
    saved.set(name, Object.getOwnPropertyDescriptor(window, name))
  }
  Object.defineProperty(window, name, { configurable: true, get })
}

/** Takes `onscrollend` away, as a browser without the event lacks it. */
function withoutScrollEnd() {
  if (!saved.has('onscrollend')) {
    saved.set(
      'onscrollend',
      Object.getOwnPropertyDescriptor(window, 'onscrollend'),
    )
  }
  Reflect.deleteProperty(window, 'onscrollend')
}

/**
 * The track and its bar, moved by the hook. Made in a function because
 * `useComponentExportOnlyModules` refuses an unexported component at
 * module scope, and `noExportsInTest` refuses the export.
 */
function barOf() {
  function Bar({
    drawerOpen = false,
    attached = true,
    path = '/',
    search = '',
    replace = false,
  }: {
    drawerOpen?: boolean
    attached?: boolean
    path?: string
    search?: string
    replace?: boolean
  }) {
    const bar = useRef<HTMLElement>(null)
    const track = useRef<HTMLDivElement>(null)
    usePhoneBar(bar, track, drawerOpen, { path, search, replace })
    return (
      <div ref={attached ? track : undefined} className="phone-bar-track">
        <header
          ref={(node) => {
            if (attached) bar.current = node
            node &&
              Object.defineProperty(node, 'offsetHeight', {
                configurable: true,
                get: () => page.barHeight,
              })
          }}
          className="phone-bar"
        >
          <button type="button">Menu</button>
        </header>
      </div>
    )
  }
  return Bar
}

const Bar = barOf()

let go: NavigateFunction = () => {
  throw new Error('no router')
}

/**
 * The bar, while `bar` holds, beside a view that keeps its scroll on the
 * document (`useKeptScroll`), under one router: the bar's path stays
 * `/`, so only the keeper's landing can show it. Made in a function for
 * the reason `barOf` gives.
 */
function keeperOf() {
  function Kept({ ready }: { ready: boolean }) {
    go = useNavigate()
    useKeptScroll(ready)
    return null
  }
  function BesideKeeper({
    ready = true,
    bar = true,
  }: {
    ready?: boolean
    bar?: boolean
  }) {
    return (
      <MemoryRouter>
        {bar && <Bar />}
        <Kept ready={ready} />
      </MemoryRouter>
    )
  }
  return BesideKeeper
}

const BesideKeeper = keeperOf()

/**
 * Has `:focus-visible` match the focused element while `keyboard` is set:
 * jsdom's own answer follows its key heuristics, so the case says which
 * focus is the keyboard's.
 */
function keyboardOrNot(): { keyboard: boolean } {
  const focus = { keyboard: false }
  const matches = Element.prototype.matches
  const spy = vi
    .spyOn(Element.prototype, 'matches')
    .mockImplementation(function (this: Element, selector: string) {
      return selector === ':focus-visible'
        ? focus.keyboard && this === document.activeElement
        : matches.call(this, selector)
    })
  onTestFinished(() => spy.mockRestore())
  return focus
}

function mounted(props: { drawerOpen?: boolean; attached?: boolean } = {}) {
  const host = mount(<Bar {...props} />)
  return {
    track: host.querySelector('.phone-bar-track') as HTMLElement,
    bar: host.querySelector('header.phone-bar') as HTMLElement,
  }
}

/** Moves the page to `y` and says `scroll`, as the browser does. */
function scrollTo(y: number) {
  page.y = y
  dispatchEvent(new Event('scroll'))
}

const say = (type: string) => dispatchEvent(new Event(type))

/** Says a touch event `type` with `fingers` still on the screen. */
function touched(type: string, fingers: number) {
  const event = new Event(type)
  Object.defineProperty(event, 'touches', { value: { length: fingers } })
  dispatchEvent(event)
}

/** Sets the window `width` wide and says `resize`. */
function resizedTo(width: number) {
  page.width = width
  say('resize')
}

/** Draws a frame: runs the callbacks requested for it. */
function frame() {
  const due = [...frames.values()]
  frames.clear()
  for (const run of due) run(0)
}

/** Answers each `ResizeObserver` watching something, as after a resize. */
function observed() {
  for (const each of observers) each.answer()
}

/** Answers only the observers watching `el`, as after `el` alone resized. */
function observedOn(el: Element) {
  for (const each of observers) if (each.observed.includes(el)) each.answer()
}

const rest = (bar: HTMLElement) => ({
  atRest: bar.hasAttribute('data-at-rest'),
  top: bar.style.getPropertyValue('--rest-top'),
})

/** Scrolls to 600, 500, then 520: the track ends at 552, 32 px in view. */
function partlyShown() {
  scrollTo(600)
  scrollTo(500)
  scrollTo(520)
}

beforeEach(() => {
  page = { y: 0, width: 390, height: 800, scrollHeight: 5000, barHeight: 52 }
  stubWindow('scrollY', () => page.y)
  stubWindow('innerWidth', () => page.width)
  stubWindow('innerHeight', () => page.height)
  Object.defineProperty(document.documentElement, 'scrollHeight', {
    configurable: true,
    get: () => page.scrollHeight,
  })
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  for (const [name, descriptor] of saved) {
    if (descriptor) Object.defineProperty(window, name, descriptor)
    else Reflect.deleteProperty(window, name)
  }
  saved.clear()
  Reflect.deleteProperty(document.documentElement, 'scrollHeight')
  Reflect.deleteProperty(document.documentElement, 'scrollTop')
})

describe('usePhoneBar', () => {
  beforeEach(() => {
    phone = layoutList()
    vi.stubGlobal('matchMedia', (query: string) =>
      query === PHONE_LAYOUT ? phone : { matches: false },
    )
    observers = []
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
        disconnect() {
          this.observed = []
        }
      },
    )
    frames = new Map()
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

  it('starts shown with a track one bar tall at the top', () => {
    const { track } = mounted()
    expect(track.style.height).toBe('52px')
    expect(barLine()).toBe(52)
  })

  it("moves the track's end by the band rule on each scroll", () => {
    const { track } = mounted()
    const ends = [20, 120, 100, 50].map((y) => {
      scrollTo(y)
      return track.style.height
    })
    expect(ends).toEqual(['52px', '120px', '120px', '102px'])
  })

  it('holds the bar shown while the drawer is open', () => {
    const { track } = mounted()
    scrollTo(500)
    expect(track.style.height).toBe('500px')
    rerender(<Bar drawerOpen />)
    expect(track.style.height).toBe('552px')
    scrollTo(700)
    expect(track.style.height).toBe('752px')
  })

  // A browser scrolls to a control Tab reaches before it says `focusin`:
  // here the page moves with no event after the key, and goes back.
  it('holds the bar and brings it in for keyboard focus in it, and for no other', () => {
    const focus = keyboardOrNot()
    vi.stubGlobal('scrollTo', (_: number, y: number) => {
      page.y = y
    })
    const { track, bar } = mounted()
    const menu = bar.querySelector('button') as HTMLButtonElement
    scrollTo(600)
    menu.focus()
    const pointer = [track.style.height]
    scrollTo(700)
    pointer.push(track.style.height)
    menu.blur()
    focus.keyboard = true
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true }),
    )
    page.y = 650
    menu.focus()
    const shown = [page.y, track.style.height]
    scrollTo(1000)
    expect({ pointer, shown, held: track.style.height }).toEqual({
      pointer: ['600px', '700px'],
      shown: [700, '752px'],
      held: '1052px',
    })
  })

  it('shows the bar when the page changes and not when it stays', () => {
    const { track } = mounted()
    scrollTo(600)
    expect(track.style.height, 'hidden mid-page').toBe('600px')
    rerender(<Bar path="/" />)
    expect(track.style.height).toBe('600px')
    rerender(<Bar path="/series/1" />)
    expect(track.style.height).toBe('652px')
  })

  // A replace on the same path is a key typed into the search box, a chip,
  // the sort or an edition; one to a new path is a redirect.
  it('shows the bar for a replace to a new path, and not for one on the same path', () => {
    const { track } = mounted()
    scrollTo(600)
    rerender(<Bar search="?q=k" replace />)
    const samePath = track.style.height
    rerender(<Bar path="/config/libraries" replace />)
    expect({ samePath, newPath: track.style.height }).toEqual({
      samePath: '600px',
      newPath: '652px',
    })
  })

  // The keeper writes the document's scroll, here `page.y`, and the browser
  // reports it a frame later, at the document, whence it reaches the window.
  it("shows the bar at the keeper's landing", () => {
    Object.defineProperty(document.documentElement, 'scrollTop', {
      configurable: true,
      get: () => page.y,
      set: (y: number) => {
        page.y = y
      },
    })
    const reported = (y: number) => {
      page.y = y
      document.dispatchEvent(new Event('scroll', { bubbles: true }))
    }
    const host = mount(<BesideKeeper />)
    const track = host.querySelector('.phone-bar-track') as HTMLElement
    reported(1500)
    expect(track.style.height, 'hidden mid-page').toBe('1500px')
    act(() => {
      void go('/series/1')
    })
    reported(0)
    reported(600)
    rerender(<BesideKeeper ready={false} />)
    act(() => {
      void go(-1)
    })
    const waiting = track.style.height
    rerender(<BesideKeeper ready />)
    const landed = [page.y, track.style.height]
    reported(page.y)
    expect({ waiting, landed, after: track.style.height }).toEqual({
      waiting: '600px',
      landed: [1500, '1552px'],
      after: '1552px',
    })
    rerender(<BesideKeeper bar={false} />)
    act(() => {
      void go('/series/2')
    })
    expect(track.style.height, 'nothing heard once gone').toBe('')
  })

  // The drawer gives ☰ keyboard focus back as it closes on a link chosen by
  // key, after the keeper has landed the new page and before its scroll is
  // reported, and frames after the Tab that reached the link: the landing
  // is where the page stays.
  it('leaves a landing where it is when keyboard focus comes back to the bar', () => {
    const focus = keyboardOrNot()
    vi.stubGlobal('scrollTo', (_: number, y: number) => {
      page.y = y
    })
    Object.defineProperty(document.documentElement, 'scrollTop', {
      configurable: true,
      get: () => page.y,
      set: (y: number) => {
        page.y = y
      },
    })
    const host = mount(<BesideKeeper />)
    const track = host.querySelector('.phone-bar-track') as HTMLElement
    const menu = host.querySelector('.phone-bar button') as HTMLButtonElement
    page.y = 1500
    document.dispatchEvent(new Event('scroll', { bubbles: true }))
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab' }))
    frame()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }))
    act(() => {
      void go('/series/1')
    })
    focus.keyboard = true
    menu.focus()
    expect({ y: page.y, track: track.style.height }).toEqual({
      y: 0,
      track: '52px',
    })
  })

  it('rests at scrollend with the bar in view, at its own pixels', () => {
    const { track, bar } = mounted()
    partlyShown()
    expect(track.style.height).toBe('552px')
    say('scrollend')
    expect(rest(bar)).toEqual({ atRest: true, top: '500px' })
    expect(barLine()).toBe(32)
  })

  it('rests 150 ms after the last scroll where scrollend is missing', () => {
    withoutScrollEnd()
    const { bar } = mounted()
    // 100 ms apart, so a wait timed from an earlier scroll ends first.
    scrollTo(600)
    vi.advanceTimersByTime(100)
    scrollTo(500)
    vi.advanceTimersByTime(100)
    scrollTo(520)
    vi.advanceTimersByTime(149)
    expect(rest(bar).atRest).toBe(false)
    vi.advanceTimersByTime(1)
    expect(rest(bar)).toEqual({ atRest: true, top: '500px' })
  })

  it('leaves rest at a wheel, and at a touchmove, at the same pixels', () => {
    const { track, bar } = mounted()
    partlyShown()
    for (const type of ['wheel', 'touchmove']) {
      say('scrollend')
      expect(rest(bar).atRest, type).toBe(true)
      say(type)
      expect(rest(bar).atRest, type).toBe(false)
      expect(track.style.height, type).toBe('552px')
    }
  })

  it('leaves rest at a scroll that moves the page', () => {
    const { track, bar } = mounted()
    partlyShown()
    say('scrollend')
    scrollTo(540)
    expect(rest(bar).atRest).toBe(false)
    expect(track.style.height).toBe('552px')
  })

  it('leaves the track alone at a wheel while the bar is not at rest', () => {
    const { track, bar } = mounted()
    partlyShown()
    say('scrollend')
    say('wheel')
    scrollTo(1000)
    scrollTo(960)
    expect(rest(bar).atRest).toBe(false)
    expect(track.style.height).toBe('1000px')
    say('wheel')
    expect(track.style.height).toBe('1000px')
  })

  // A touch that moves only something inside the page, such as the Continue
  // row sideways, ends no page scroll; one that moves the page does.
  it('rests again 150 ms after a touchmove that scrolls nothing, and not when a page scroll follows', () => {
    const { bar } = mounted()
    partlyShown()
    say('scrollend')
    say('touchmove')
    const left = rest(bar).atRest
    vi.advanceTimersByTime(149)
    const waiting = rest(bar).atRest
    vi.advanceTimersByTime(1)
    const again = rest(bar)
    say('touchmove')
    scrollTo(540)
    vi.advanceTimersByTime(1000)
    expect({ left, waiting, again, scrolled: rest(bar).atRest }).toEqual({
      left: false,
      waiting: false,
      again: { atRest: true, top: '500px' },
      scrolled: false,
    })
  })

  // A gesture's scroll ends with its `scrollend`, the glide after the finger
  // lifts included; a scroll nothing began, such as the browser's clamp of
  // a page grown shorter, has none, so the wait rests the bar.
  it('rests 150 ms after a scroll no input began, and not by the wait during a scroll a touch or a key began', () => {
    let now = 10_000
    const clock = vi.spyOn(performance, 'now').mockImplementation(() => now)
    onTestFinished(() => clock.mockRestore())
    const { bar } = mounted()
    partlyShown()
    vi.advanceTimersByTime(149)
    const other = [rest(bar).atRest]
    vi.advanceTimersByTime(1)
    other.push(rest(bar).atRest)
    touched('touchstart', 1)
    say('touchmove')
    now += 1000
    scrollTo(560)
    vi.advanceTimersByTime(1000)
    const touch = rest(bar).atRest
    touched('touchend', 0)
    now += 1000
    scrollTo(600)
    vi.advanceTimersByTime(1000)
    const glide = rest(bar).atRest
    say('scrollend')
    const ended = rest(bar).atRest
    now += 1000
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'PageDown' }))
    scrollTo(1000)
    vi.advanceTimersByTime(1000)
    const key = rest(bar).atRest
    say('scrollend')
    now += 1000
    scrollTo(980)
    vi.advanceTimersByTime(150)
    expect({
      other,
      touch,
      glide,
      ended,
      key,
      after: rest(bar).atRest,
    }).toEqual({
      other: [false, true],
      touch: false,
      glide: false,
      ended: true,
      key: false,
      after: true,
    })
  })

  it('shows a resting bar whole when the drawer opens', () => {
    const { track } = mounted()
    partlyShown()
    say('scrollend')
    expect(barLine()).toBe(32)
    rerender(<Bar drawerOpen />)
    expect(track.style.height).toBe('572px')
    expect(barLine()).toBe(52)
  })

  it("keeps a resting bar's share when the page shrinks under it", () => {
    const { track, bar } = mounted()
    frame()
    partlyShown()
    say('scrollend')
    // The page now scrolls 500 px at most, so the browser clamps the scroll.
    page.scrollHeight = 1300
    page.y = 500
    observed()
    say('scroll')
    expect(rest(bar)).toEqual({ atRest: true, top: '480px' })
    expect(track.style.height).toBe('532px')
    expect(barLine()).toBe(32)
  })

  // In a browser the root's box is the window's, so a shrink in place
  // resizes only the track's clip, a frame before the clamp's scroll event.
  it('takes a resting bar back to the track at a clamp the clip’s observer sees before its scroll event', () => {
    const { track, bar } = mounted()
    frame()
    partlyShown()
    say('scrollend')
    page.scrollHeight = 1300
    page.y = 400
    observedOn(track.parentElement as HTMLElement)
    const seen = { atRest: rest(bar).atRest, end: track.style.height }
    say('scroll')
    expect({ seen, end: track.style.height, line: barLine() }).toEqual({
      seen: { atRest: false, end: '452px' },
      end: '452px',
      line: 52,
    })
    vi.advanceTimersByTime(150)
    expect(rest(bar)).toEqual({ atRest: true, top: '400px' })
  })

  it('leaves the wait for rest alone when the clip resizes with the scroll where it was', () => {
    const { track, bar } = mounted()
    frame()
    scrollTo(600)
    vi.advanceTimersByTime(100)
    page.scrollHeight = 4000
    observedOn(track.parentElement as HTMLElement)
    vi.advanceTimersByTime(50)
    expect(rest(bar).atRest).toBe(true)
  })

  it('leaves the track to the scroll when the page resizes with the bar on it', () => {
    const { track } = mounted()
    frame()
    scrollTo(120)
    page.scrollHeight = 4000
    observed()
    expect(track.style.height).toBe('120px')
  })

  // The held share, 32 px, would put the bar 10 px above the page's top at
  // a scroll of 10; the band holds it whole at the top there, at rest too.
  it('rests no higher than the page’s top through a turn near the top, its foot the line, and rejoins the track there', () => {
    const { track, bar } = mounted()
    partlyShown()
    say('scrollend')
    resizedTo(844)
    scrollTo(10)
    expect(rest(bar)).toEqual({ atRest: true, top: '0px' })
    expect(barLine()).toBe(42)
    vi.advanceTimersByTime(600)
    expect(rest(bar)).toEqual({ atRest: true, top: '0px' })
    say('wheel')
    expect(track.style.height).toBe('52px')
  })

  it('ignores a scroll at rest that leaves the page where it was', () => {
    const { track, bar } = mounted()
    partlyShown()
    say('scrollend')
    say('scroll')
    expect(rest(bar)).toEqual({ atRest: true, top: '500px' })
    expect(track.style.height).toBe('552px')
  })

  it('holds the share in view through a turn and puts it back against each new scroll', () => {
    const { track } = mounted()
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    expect(barLine()).toBe(26)
    resizedTo(844)
    for (const y of [300, 10]) {
      scrollTo(y)
      expect(track.style.height, `at ${y}`).toBe(`${Math.max(52, y + 26)}px`)
      expect(barLine(), `at ${y}`).toBe(Math.max(26, 52 - y))
    }
    vi.advanceTimersByTime(400)
    resizedTo(390)
    vi.advanceTimersByTime(599)
    scrollTo(1000)
    expect(track.style.height).toBe('1026px')
    vi.advanceTimersByTime(1)
    scrollTo(1100)
    expect(track.style.height).toBe('1100px')
    expect(barLine()).toBe(0)
  })

  it.each([
    ['a resize that changes the width', () => resizedTo(844)],
    ['an orientationchange', () => say('orientationchange')],
    [
      'a width change a scroll sees first',
      () => {
        page.width = 844
      },
    ],
  ])('counts %s as a turn', (_, turn) => {
    const { track } = mounted()
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    turn()
    scrollTo(300)
    expect(track.style.height).toBe('326px')
  })

  it('counts a resize that keeps the width as no turn', () => {
    const { track } = mounted()
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    page.height = 700
    say('resize')
    scrollTo(300)
    expect(track.style.height).toBe('352px')
  })

  it("re-places the bar at the keeper's put-back and in the observer during a turn", () => {
    const { track } = mounted()
    frame()
    expect(observers.map((one) => one.observed)).toEqual([
      [document.documentElement],
      [track.parentElement],
    ])
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    // The keeper writes the scroll and says so before any scroll event.
    page.y = 1526
    scrollPutBack()
    expect(track.style.height).toBe('1552px')
    expect(barLine()).toBe(26)
    // The observer sees a turn before its resize event comes.
    page.width = 844
    page.y = 2000
    observed()
    expect(track.style.height).toBe('2026px')
    page.y = 2100
    observed()
    expect(track.style.height).toBe('2126px')
    page.y = 2500
    scrollPutBack()
    expect(track.style.height).toBe('2526px')
    expect(barLine()).toBe(26)
  })

  // Near the top `max(H, …)` lifts the track, so the share the scroll shows
  // is more than the share held; a put-back must keep the held one.
  it("keeps the turn's share at the keeper's put-back near the top", () => {
    const { track } = mounted()
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    resizedTo(844)
    scrollTo(10)
    expect(track.style.height).toBe('52px')
    page.y = 300
    scrollPutBack()
    expect(track.style.height).toBe('326px')
    expect(barLine()).toBe(26)
  })

  it("keeps a resting bar's share at the keeper's put-back after a turn near the top", () => {
    const { track, bar } = mounted()
    partlyShown()
    say('scrollend')
    resizedTo(844)
    scrollTo(10)
    vi.advanceTimersByTime(600)
    expect(rest(bar).atRest).toBe(true)
    page.y = 300
    scrollPutBack()
    expect(rest(bar)).toEqual({ atRest: true, top: '280px' })
    expect(track.style.height).toBe('332px')
    expect(barLine()).toBe(32)
  })

  // Near the top the track is never shorter than the bar, so more of the
  // bar is in view than the share held: the keeper's line is the bar's
  // foot, never above it.
  it('gives the line the band forces near the top during a turn, where it is below the held share’s', () => {
    mounted()
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    resizedTo(844)
    scrollTo(10)
    expect(barLine()).toBe(42)
    scrollTo(40)
    expect(barLine()).toBe(26)
  })

  it('re-places on the first width-changing resize after an orientationchange', () => {
    const { track, bar } = mounted()
    scrollTo(600)
    scrollTo(500)
    scrollTo(526)
    say('orientationchange')
    // iOS sets its scroll with the new size, before any scroll event.
    page.y = 1526
    resizedTo(844)
    expect(track.style.height).toBe('1552px')
    expect(barLine()).toBe(26)
    // At rest, the bar's own offset follows.
    vi.advanceTimersByTime(600)
    say('scrollend')
    expect(rest(bar)).toEqual({ atRest: true, top: '1500px' })
    say('orientationchange')
    page.y = 700
    resizedTo(390)
    expect(rest(bar)).toEqual({ atRest: true, top: '674px' })
  })

  it("rests at the turn's end", () => {
    const { track, bar } = mounted()
    partlyShown()
    resizedTo(844)
    scrollTo(1000)
    expect(rest(bar).atRest).toBe(false)
    vi.advanceTimersByTime(599)
    expect(rest(bar).atRest).toBe(false)
    vi.advanceTimersByTime(1)
    expect(rest(bar)).toEqual({ atRest: true, top: '980px' })
    expect(track.style.height).toBe('1032px')
    expect(barLine()).toBe(32)
  })

  it.each([
    ['at rest', () => say('scrollend')],
    ['on the track', () => {}],
  ])(
    'shows the bar whole when the drawer opens during a turn, %s, and keeps it whole once the turn is over',
    (_, settle) => {
      const { track, bar } = mounted()
      partlyShown()
      settle()
      resizedTo(844)
      scrollTo(1000)
      rerender(<Bar drawerOpen />)
      expect(barLine()).toBe(52)
      scrollTo(1100)
      expect(track.style.height).toBe('1152px')
      vi.advanceTimersByTime(600)
      expect(rest(bar)).toEqual({ atRest: true, top: '1100px' })
      expect(barLine()).toBe(52)
    },
  )

  it('does nothing outside the phone layout', () => {
    const { track, bar } = mounted()
    frame()
    scrollTo(600)
    expect(track.style.height).toBe('600px')
    // A window widening past the layout resizes before the list's change.
    // The bar keeps its height here so that a share taken of it would show.
    phone.matches = false
    resizedTo(1280)
    scrollTo(700)
    say('scrollend')
    observed()
    vi.advanceTimersByTime(1000)
    expect(track.style.height).toBe('600px')
    expect(rest(bar).atRest).toBe(false)
    expect(barLine()).toBeNull()
    // The keeper's put-back reads the list too, and stops the track.
    scrollPutBack()
    expect(track.style.height).toBe('')
    expect(barLine()).toBeNull()
    phone.change()
    scrollTo(800)
    expect(track.style.height).toBe('')
    expect(phone.listening()).toBe(1)
  })

  it('starts nothing at a mount outside the phone layout', () => {
    phone.matches = false
    const { track, bar } = mounted()
    frame()
    scrollTo(600)
    resizedTo(1280)
    scrollPutBack()
    say('scrollend')
    vi.advanceTimersByTime(1000)
    expect(track.style.height).toBe('')
    expect(rest(bar).atRest).toBe(false)
    expect(barLine()).toBeNull()
    expect(observers).toEqual([])
  })

  /** Out of the phone layout, where the bar is not drawn and has no height. */
  function leaves() {
    phone.matches = false
    page.barHeight = 0
  }
  /** Back in the phone layout. */
  function returns() {
    phone.matches = true
    page.barHeight = 52
  }

  it('starts shown on coming back to the phone layout', () => {
    const { track } = mounted()
    frame()
    scrollTo(600)
    expect(track.style.height).toBe('600px')
    leaves()
    phone.change()
    expect(track.style.height).toBe('')
    scrollTo(900)
    returns()
    phone.change()
    expect(track.style.height).toBe('952px')
    expect(barLine()).toBe(52)
  })

  // A return to the phone layout starts the track with no page change to
  // show the bar, so the track's own start rests it there; the bar alone
  // keeps its path, so only the keeper's landing shows it beside the keeper.
  it('rests at mount, under the drawer and once it closes, on a return to the phone layout, and at a landing and after its scroll', () => {
    Object.defineProperty(document.documentElement, 'scrollTop', {
      configurable: true,
      get: () => page.y,
      set: (y: number) => {
        page.y = y
      },
    })
    const { track, bar } = mounted()
    const atMount = rest(bar)
    scrollTo(600)
    rerender(<Bar drawerOpen />)
    const open = [rest(bar), track.style.height]
    rerender(<Bar />)
    const closed = rest(bar)
    leaves()
    phone.change()
    page.y = 900
    returns()
    phone.change()
    const returned = rest(bar)
    rerender(null)
    const reported = (y: number) => {
      page.y = y
      document.dispatchEvent(new Event('scroll', { bubbles: true }))
    }
    const host = mount(<BesideKeeper />)
    const kept = host.querySelector('header.phone-bar') as HTMLElement
    reported(1500)
    act(() => {
      void go('/series/1')
    })
    reported(0)
    reported(600)
    rerender(<BesideKeeper ready={false} />)
    act(() => {
      void go(-1)
    })
    const waiting = rest(kept).atRest
    rerender(<BesideKeeper ready />)
    const landed = [page.y, rest(kept)]
    reported(page.y)
    expect({
      atMount,
      open,
      closed,
      returned,
      waiting,
      landed,
      reported: rest(kept),
    }).toEqual({
      atMount: { atRest: true, top: '0px' },
      open: [{ atRest: true, top: '600px' }, '652px'],
      closed: { atRest: true, top: '600px' },
      returned: { atRest: true, top: '900px' },
      waiting: false,
      landed: [1500, { atRest: true, top: '1500px' }],
      reported: { atRest: true, top: '1500px' },
    })
  })

  it('a width change outside the phone layout does nothing', () => {
    const { track, bar } = mounted()
    frame()
    partlyShown()
    say('scrollend')
    leaves()
    resizedTo(1280)
    phone.change()
    for (const width of [1213, 1440, 1280]) {
      page.width = width
      observed()
      resizedTo(width)
      say('orientationchange')
      scrollTo(700)
    }
    scrollPutBack()
    vi.advanceTimersByTime(1000)
    expect({
      height: track.style.height,
      rest: rest(bar),
      line: barLine(),
      timers: vi.getTimerCount(),
    }).toEqual({
      height: '',
      rest: { atRest: false, top: '' },
      line: null,
      timers: 0,
    })
    returns()
    phone.change()
    expect([track.style.height, barLine()]).toEqual(['752px', 52])
  })

  // Between the two the bar is not drawn and has no height, so a wheel or
  // the drawer moves the track by a bar of 0 px; the list's change then
  // stops the track and clears what it wrote, and a return starts afresh.
  it('keeps nothing a wheel or the drawer wrote between a resize out of the layout and the list’s change', () => {
    const { track, bar } = mounted()
    partlyShown()
    say('scrollend')
    leaves()
    resizedTo(1280)
    say('wheel')
    say('touchmove')
    rerender(<Bar drawerOpen />)
    phone.change()
    rerender(<Bar />)
    expect([track.style.height, rest(bar)]).toEqual([
      '',
      { atRest: false, top: '' },
    ])
    returns()
    phone.change()
    expect([track.style.height, barLine()]).toEqual(['572px', 52])
  })

  it('re-places nothing in the observer between a resize out of the layout and the list’s change', () => {
    const { track, bar } = mounted()
    frame()
    partlyShown()
    say('scrollend')
    leaves()
    page.width = 1280
    observed()
    expect([track.style.height, rest(bar)]).toEqual([
      '552px',
      { atRest: true, top: '500px' },
    ])
  })

  // A clip observer left watching after a stop would run the stopped
  // track's scroll handling against the bar a return starts again.
  it('watches nothing once the track stops', () => {
    mounted()
    frame()
    leaves()
    phone.change()
    expect(observers.flatMap((one) => one.observed)).toEqual([])
  })

  // After a return the track's own put-back listener comes after the one
  // that stops it, and runs in the same put-back once stopped.
  it('leaves a track the keeper’s put-back stops outside the layout as stopped, after a return', () => {
    const { track, bar } = mounted()
    leaves()
    phone.change()
    returns()
    phone.change()
    partlyShown()
    say('scrollend')
    leaves()
    resizedTo(1280)
    scrollPutBack()
    phone.change()
    expect([track.style.height, rest(bar)]).toEqual([
      '',
      { atRest: false, top: '' },
    ])
  })

  it.each(['wheel', 'touchmove'])(
    'hears a %s only from the track running, after a return',
    (type) => {
      const { track } = mounted()
      partlyShown()
      say('scrollend')
      leaves()
      phone.change()
      returns()
      phone.change()
      // Rests again with 40 px in view, at 1448; leaves rest by a scroll and
      // is pinned at 1500, past the rest's line.
      scrollTo(1500)
      scrollTo(1460)
      say('scrollend')
      scrollTo(1600)
      scrollTo(1500)
      expect(track.style.height).toBe('1552px')
      say(type)
      expect(track.style.height).toBe('1552px')
    },
  )

  it('starts nothing where the refs hold no element', () => {
    const { track } = mounted({ attached: false })
    scrollTo(600)
    expect(track.style.height).toBe('')
    expect(barLine()).toBeNull()
  })

  it('lets go on unmount', () => {
    withoutScrollEnd()
    const { track, bar } = mounted()
    partlyShown()
    vi.advanceTimersByTime(150)
    resizedTo(844)
    expect(rest(bar).atRest).toBe(true)
    expect(vi.getTimerCount()).toBe(1)
    expect(frames.size).toBe(1)
    rerender(null)
    scrollTo(1000)
    expect(track.style.height).toBe('')
    expect(rest(bar)).toEqual({ atRest: false, top: '' })
    expect(barLine()).toBeNull()
    expect(vi.getTimerCount()).toBe(0)
    expect(frames.size).toBe(0)
    expect(phone.listening()).toBe(0)
  })
})

describe('usePhoneBar without the APIs it needs', () => {
  it('does nothing where matchMedia is missing', () => {
    vi.stubGlobal('ResizeObserver', class {})
    expect(typeof matchMedia).toBe('undefined')
    const { track } = mounted()
    scrollTo(600)
    expect(track.style.height).toBe('')
    expect(barLine()).toBeNull()
  })

  it('does nothing where ResizeObserver is missing', () => {
    vi.stubGlobal('matchMedia', () => layoutList())
    expect(typeof ResizeObserver).toBe('undefined')
    const { track } = mounted()
    scrollTo(600)
    expect(track.style.height).toBe('')
    expect(barLine()).toBeNull()
  })
})

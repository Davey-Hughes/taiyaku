import { act, Profiler } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { Chrome, type ChromeProps } from '../src/Chrome'
import type { Group } from '../src/pairing'
import type { Position } from '../src/resolver'
import { mount, rerender } from './support/mount'

/**
 * `Chrome` is the two bars — `TopBar.tsx` (its own test) owns the top bar's
 * controls, `Filmstrip.tsx` (its own test) owns tile layout,
 * `modules/motion.ts`'s `scheduleFade` (pinned in `motion.test.ts`) owns the
 * frame-scheduling arithmetic, and `modules/readouts.ts`'s `readoutRow`
 * (pinned in `readouts.test.ts`) owns the join. What is only pinned here is
 * what `Chrome` itself does: the `visible` unmount gate, where each bar docks
 * and what goes in it, the top bar standing down while correcting, wiring the
 * fade through a real `useEffect` across a hide/show cycle, the hover-preview
 * state it owns, passing the right, gated fields to `readoutRow`, and the mark
 * it draws on the progress line from what its `Filmstrip` tells it.
 */

// jsdom has no layout engine and implements no `scrollIntoView` —
// `Filmstrip.tsx`'s own centring effect calls it on mount. Stubbed only so
// that effect does not throw; what it centres on is `Filmstrip`'s test's
// concern.
Element.prototype.scrollIntoView ??= () => {}

// Nor a `ResizeObserver`, which `Filmstrip` builds on its rail. One that does
// nothing is enough here: what the observer is for is `Filmstrip`'s test's
// concern as well.
class StubResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??=
  StubResizeObserver as unknown as typeof ResizeObserver

const slots: Position[] = [{ page: 0 }, { page: 1, half: 'L' }, { page: 2 }]

const groups: Group[] = [{ slots: [0] }, { slots: [1] }, { slots: [2] }]

const baseProps: ChromeProps = {
  visible: true,
  motion: { animate: false, ms: 0 },
  readouts: {
    position: true,
    time: true,
    chapter: true,
    health: true,
    notes: true,
  },
  position: 'slot 1 / 3',
  time: '2m read',
  chapter: 'c001',
  health: 'sound pair',
  notes: '3 notes',
  progress: 0.25,
  title: 'Volume One',
  editionLine: 'Vol. 1 · Raw',
  mode: 'single',
  onMode: vi.fn(),
  settingsOpen: false,
  onSettings: vi.fn(),
  reading: 'ltr',
  work: 'work1',
  edition: 'ed1',
  slots,
  groups,
  currentIndex: 0,
  onNavigate: vi.fn(),
  onLeave: vi.fn(),
}

const buttonNamed = (host: ParentNode, text: string) =>
  [...host.querySelectorAll('button')].find((b) => b.textContent === text) as
    | HTMLButtonElement
    | undefined

const bottomBar = (host: HTMLElement) =>
  host.querySelector('.reader-bottom-bar') as HTMLElement

describe('Chrome', () => {
  it('renders nothing while hidden — an unmount, not merely invisible chrome', () => {
    const host = mount(<Chrome {...baseProps} visible={false} />)
    expect(host.children.length).toBe(0)
  })

  it('renders both bars while visible', () => {
    const host = mount(<Chrome {...baseProps} />)
    // One button per group's tile, plus the top bar's five: Library, the
    // three layouts and Settings.
    expect(host.querySelectorAll('button').length).toBe(groups.length + 5)
    expect(host.querySelector('.reader-top-bar')).not.toBeNull()
    expect(host.querySelector('.reader-readout')).not.toBeNull()
  })

  it('gives the reader a way back to the library', () => {
    const onLeave = vi.fn()
    const host = mount(<Chrome {...baseProps} onLeave={onLeave} />)
    buttonNamed(host, 'Library')?.click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  /**
   * Spec 2026-09-17 §8.3: the way back is in the top bar, which is inside this
   * component, so it keeps the fade and stands down while correcting.
   * `App.browser.test.tsx` measures that nothing covers it.
   */
  it('puts the way back in the top bar, on the reader’s top edge', () => {
    const host = mount(<Chrome {...baseProps} />)
    const bar = host.querySelector('.reader-top-bar') as HTMLElement
    expect(bar.style.top).toBe('0px')
    expect(bar.contains(buttonNamed(host, 'Library') as Node)).toBe(true)
  })

  it('hands the top bar its title, edition line, layout and settings', () => {
    const onMode = vi.fn()
    const onSettings = vi.fn()
    const host = mount(
      <Chrome
        {...baseProps}
        mode="two"
        settingsOpen
        onMode={onMode}
        onSettings={onSettings}
      />,
    )
    expect(host.querySelector('.reader-title')?.textContent).toBe(
      'Volume OneVol. 1 · Raw',
    )
    expect(buttonNamed(host, 'two')?.getAttribute('aria-pressed')).toBe('true')
    expect(buttonNamed(host, 'Settings')?.getAttribute('aria-expanded')).toBe(
      'true',
    )
    buttonNamed(host, 'scroll')?.click()
    expect(onMode).toHaveBeenCalledWith('scroll')
    buttonNamed(host, 'Settings')?.click()
    expect(onSettings).toHaveBeenCalledOnce()
  })

  it('draws no top bar while correcting, when the banner has the top edge', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(buttonNamed(host, 'Library')).toBeUndefined()
    expect(bottomBar(host)).not.toBeNull()
  })

  it('docks one bottom bar to the reader’s bottom edge, padded by the safe areas', () => {
    const host = mount(<Chrome {...baseProps} />)
    const bar = bottomBar(host)
    expect(bar.style.position).toBe('absolute')
    expect([bar.style.left, bar.style.right, bar.style.bottom]).toEqual([
      '0px',
      '0px',
      '0px',
    ])
    expect(Number(bar.style.zIndex)).toBe(3)
    expect(bar.style.paddingBottom).toBe('var(--safe-bottom)')
    expect(bar.style.paddingLeft).toBe('var(--safe-left)')
    expect(bar.style.paddingRight).toBe('var(--safe-right)')
  })

  it('stacks the strip, the progress line and the readout row inside it, in that order', () => {
    const host = mount(<Chrome {...baseProps} />)
    const [strip, progress, readout] = [
      ...bottomBar(host).children,
    ] as HTMLElement[]
    expect(strip.style.height).toBe('var(--strip-height)')
    expect(strip.querySelectorAll('button')).toHaveLength(groups.length)
    expect(progress.classList.contains('reader-progress')).toBe(true)
    expect(progress.getAttribute('aria-hidden')).toBe('true')
    expect(progress.style.height).toBe('var(--progress-height)')
    expect(readout.getAttribute('role')).toBeNull()
    expect(readout.style.height).toBe('var(--readout-height)')
    expect(readout.classList.contains('reader-readout')).toBe(true)
  })

  it('fills the progress line to the fraction it is handed', () => {
    const host = mount(<Chrome {...baseProps} progress={0.25} />)
    const fill = host.querySelector('.reader-progress-fill') as HTMLElement
    expect(fill.style.width).toBe('25%')
    rerender(<Chrome {...baseProps} progress={1} />)
    expect(fill.style.width).toBe('100%')
  })

  it('joins the readout row in position/health/time/chapter/notes order', () => {
    const host = mount(<Chrome {...baseProps} />)
    expect(host.querySelector('.reader-readout')?.textContent).toBe(
      'slot 1 / 3 · sound pair · 2m read · c001 · 3 notes',
    )
  })

  it('drops every readout switched off in prefs, leaving nothing to join', () => {
    const host = mount(
      <Chrome
        {...baseProps}
        readouts={{
          position: false,
          time: false,
          chapter: false,
          health: false,
          notes: false,
        }}
      />,
    )
    expect(host.querySelector('.reader-readout')?.textContent).toBe('')
  })

  it('shows no hover preview before any tile is focused, even while correcting', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    expect(host.querySelector('img[alt=""]')).toBeNull()
  })

  it('reveals the hovered tile’s page as a preview while correcting, labelled with its half', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    // Scoped to the bottom bar: the top bar is absent while correcting, so
    // `host.querySelectorAll('button')` would happen to find only tiles
    // either way, but this names the reason rather than relying on it.
    const tiles = bottomBar(host).querySelectorAll('button')

    // Tile 0 is slot 0, a plain page with no half.
    act(() => tiles[0]?.focus())
    let img = host.querySelector('img[alt=""]') as HTMLImageElement
    expect(img).not.toBeNull()
    expect(img.getAttribute('src')).toBe('/api/works/work1/ed1/pages/0')
    expect(img.classList.contains('reader-preview-img')).toBe(true)
    expect(img.nextElementSibling?.textContent).toBe('page 1')
    expect(
      img.nextElementSibling?.classList.contains('reader-preview-label'),
    ).toBe(true)

    // Tile 1 is slot 1, one half ('L') of a joined spread.
    act(() => tiles[1]?.focus())
    img = host.querySelector('img[alt=""]') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('/api/works/work1/ed1/pages/1')
    expect(img.nextElementSibling?.textContent).toBe('page 2 L')

    // Blurring the tile clears the preview rather than leaving it stuck.
    act(() => tiles[1]?.blur())
    expect(host.querySelector('img[alt=""]')).toBeNull()
  })

  it('stands the hover preview just above the bottom bar', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    // Scoped to the bottom bar for the same reason as the test above.
    act(() => bottomBar(host).querySelectorAll('button')[0]?.focus())
    const preview = host.querySelector('img[alt=""]')
      ?.parentElement as HTMLElement
    expect(preview.style.bottom).toBe(
      'calc(var(--safe-bottom) + var(--reader-bottom-bar) + 8px)',
    )
    // The bars are `zIndex: 3` (`Annotations`' overlay is 2) and the preview
    // matches them, so note tints, tabs and the side list cannot draw over
    // the page it shows.
    expect(preview.style.zIndex).toBe('3')
  })

  /**
   * O216. `Chrome` draws what its own `Filmstrip` tells it, so each case has
   * the real strip tell: the rail is handed the numbers a layout would give
   * it and a `scroll` is dispatched at it, and the share reaches `Chrome`
   * through the `onShown` it passes down, as it does in the app.
   *
   * jsdom lays nothing out, so what is read back is the style `Chrome`
   * writes on the mark: a `max()` and a `calc()`, unresolved. Where those put
   * the mark on a real line, and which half of the `max()` decides its width
   * there, is `Chrome.browser.test.tsx`'s to measure.
   *
   * The mark is written onto its element and not rendered, so a scroll costs
   * no render: the cases at the end of this block hold that, and that what
   * was written survives the renders `Chrome` does make.
   */
  describe('the mark on the progress line', () => {
    // `Filmstrip`'s scroll listener works in an animation frame; here the
    // frame lands at once, inside the `act` the scroll is dispatched in.
    beforeEach(() => {
      vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        cb(0)
        return 0
      })
    })
    afterEach(() => {
      vi.unstubAllGlobals()
    })

    const line = (host: HTMLElement) =>
      host.querySelector('.reader-progress') as HTMLElement
    /** The mark's element, which is there whenever the bars are. */
    function mark(host: HTMLElement): HTMLElement {
      const found = host.querySelector<HTMLElement>(
        '[data-testid="strip-shown"]',
      )
      if (!found) throw new Error('the bars hold no mark')
      return found
    }

    function railStandsAt(
      host: HTMLElement,
      at: { scrollLeft: number; clientWidth: number; scrollWidth: number },
    ) {
      const rail = host.querySelector('.reader-strip-rail') as HTMLElement
      Object.defineProperty(rail, 'scrollWidth', {
        value: at.scrollWidth,
        configurable: true,
      })
      Object.defineProperty(rail, 'clientWidth', {
        value: at.clientWidth,
        configurable: true,
      })
      rail.scrollLeft = at.scrollLeft
      act(() => {
        rail.dispatchEvent(new Event('scroll'))
      })
    }

    // A quarter of the rail on show, from three eighths along it: half way
    // along the rail's travel.
    const QUARTER = { scrollLeft: 600, clientWidth: 400, scrollWidth: 1600 }
    // A sixteenth on show, a quarter of the way along the travel.
    const SIXTEENTH = { scrollLeft: 1500, clientWidth: 400, scrollWidth: 6400 }

    /** A rail that holds everything it has. */
    const WHOLE = { scrollLeft: 0, clientWidth: 400, scrollWidth: 400 }

    it('hides the mark where the strip shows the whole volume, and shows it where it does not', () => {
      // jsdom's rail overflows nothing, so the share told on mount is the
      // whole.
      const host = mount(<Chrome {...baseProps} />)
      const shown = mark(host)
      expect(shown.hidden).toBe(true)

      // A rail that overflows is marked; the mark goes once the rail holds
      // everything; and it comes back when the rail overflows again.
      railStandsAt(host, QUARTER)
      expect(shown.hidden).toBe(false)
      railStandsAt(host, WHOLE)
      expect(shown.hidden).toBe(true)
      railStandsAt(host, QUARTER)
      expect(shown.hidden).toBe(false)
      // One element throughout: hidden and shown, never taken away.
      expect(mark(host)).toBe(shown)
    })

    // A share that is the whole has no place along the line to stand at:
    // nothing is worked out for it, and the hidden mark keeps what it had.
    it('leaves a hidden mark as it was', () => {
      const host = mount(<Chrome {...baseProps} />)
      railStandsAt(host, QUARTER)
      const { left, width } = mark(host).style
      expect(left).not.toBe('')

      railStandsAt(host, WHOLE)

      expect(mark(host).hidden).toBe(true)
      expect([mark(host).style.left, mark(host).style.width]).toEqual([
        left,
        width,
      ])
    })

    it('draws the mark on the line, as its child, proud of it above and below', () => {
      const host = mount(<Chrome {...baseProps} />)
      railStandsAt(host, QUARTER)
      const shown = mark(host)
      expect(shown.parentElement).toBe(line(host))
      expect(shown.classList.contains('reader-progress-shown')).toBe(true)
      // Placed against the line and not against the bar the line is in.
      expect(line(host).style.position).toBe('relative')
      expect(shown.style.position).toBe('absolute')
      expect([shown.style.top, shown.style.bottom]).toEqual(['-1px', '-1px'])
      // And the line it is proud of clips nothing: a line that did would cut
      // off the row above it and the row below. `reader.css.test.ts` holds
      // the same of the line's rule, and `Chrome.browser.test.tsx` reads the
      // two rows as they are drawn.
      expect(line(host).style.overflow).toBe('')
    })

    // While a correction is being made the top bar stands down and the strip
    // shows the other edition's tiles. Its rail is scrolled all the same, and
    // the mark is still what says where it stands.
    it('marks where the rail stands while correcting too', () => {
      const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
      // THE STATE THIS TEST NAMES: the bars are the correction's.
      expect(buttonNamed(host, 'Library')).toBeUndefined()

      railStandsAt(host, QUARTER)

      expect(mark(host).hidden).toBe(false)
      expect([mark(host).style.left, mark(host).style.width]).toEqual([
        'calc(0.5 * (100% - max(32px, 3.125%)))',
        'max(32px, 3.125%)',
      ])
    })

    // The rail of a work read right to left scrolls negative, and the share
    // it tells is measured from the start of the volume all the same. So the
    // mark is given the one width and the one place in both directions, and
    // the line is what turns round.
    for (const reading of ['ltr', 'rtl'] as const) {
      /** The rail's numbers as this direction's rail reports them. */
      const stands = (rail: typeof QUARTER) =>
        reading === 'rtl' ? { ...rail, scrollLeft: -rail.scrollLeft } : rail

      it(`is as wide as an eighth of the share shown, or the floor where that is wider, ${reading}`, () => {
        const host = mount(<Chrome {...baseProps} reading={reading} />)
        railStandsAt(host, stands(QUARTER))
        expect(mark(host).style.width).toBe('max(32px, 3.125%)')
        railStandsAt(host, stands(SIXTEENTH))
        expect(mark(host).style.width).toBe('max(32px, 0.78125%)')
      })

      it(`stands as far along the room its width leaves as the rail is along its travel, ${reading}`, () => {
        const host = mount(<Chrome {...baseProps} reading={reading} />)
        railStandsAt(host, stands(QUARTER))
        expect(mark(host).style.left).toBe(
          'calc(0.5 * (100% - max(32px, 3.125%)))',
        )
        railStandsAt(host, stands(SIXTEENTH))
        expect(mark(host).style.left).toBe(
          'calc(0.25 * (100% - max(32px, 0.78125%)))',
        )
      })

      it(`stands flush with the start of the line at the start of the volume, and with its end at the end, ${reading}`, () => {
        const host = mount(<Chrome {...baseProps} reading={reading} />)
        railStandsAt(host, stands({ ...QUARTER, scrollLeft: 0 }))
        expect(mark(host).style.left).toBe(
          'calc(0 * (100% - max(32px, 3.125%)))',
        )
        railStandsAt(host, stands({ ...QUARTER, scrollLeft: 1200 }))
        expect(mark(host).style.left).toBe(
          'calc(1 * (100% - max(32px, 3.125%)))',
        )
      })
    }

    // One pixel out of sight in two thousand: next to nothing against the
    // rail's length, and still a rail that does not show everything.
    it('marks a rail that stands a pixel short of showing everything', () => {
      const host = mount(<Chrome {...baseProps} />)
      expect(mark(host).hidden).toBe(true)

      railStandsAt(host, {
        scrollLeft: 0,
        clientWidth: 2000,
        scrollWidth: 2001,
      })

      expect(mark(host).hidden).toBe(false)
      expect(mark(host).style.left).toMatch(/^calc\(0 \* \(100% - max\(32px, /)
    })

    it('mirrors the line for a work read right to left', () => {
      const host = mount(<Chrome {...baseProps} reading="rtl" />)
      expect(line(host).style.transform).toBe('scaleX(-1)')
    })

    it('leaves the line as it is for a work read left to right', () => {
      const host = mount(<Chrome {...baseProps} reading="ltr" />)
      expect(line(host).style.transform).toBe('')
    })

    // A rail half as long again as it is wide, at the end of its travel. In
    // floating point its place comes out a hair past 1, which would stand the
    // mark that hair past the end of the line.
    it('stands the mark at the end of the line and not past it where the arithmetic overshoots', () => {
      const host = mount(<Chrome {...baseProps} />)
      const rail = { scrollLeft: 200, clientWidth: 400, scrollWidth: 600 }
      // THE STATE THIS TEST NAMES: these numbers do overshoot.
      const start = rail.scrollLeft / rail.scrollWidth
      expect(start / (1 - (1 - start))).toBeGreaterThan(1)

      railStandsAt(host, rail)

      expect(mark(host).style.left).toMatch(/^calc\(1 \* \(100% - max\(32px, /)
    })

    // A scroll tells on every frame that moves the rail. React's `Profiler`
    // counts the commits under it, and `Chrome` and its strip are both under
    // this one: the mark has to move with no commit at all.
    it('moves the mark for a scroll with no render of the bars or of the strip', () => {
      let commits = 0
      const bars = (props: ChromeProps) => (
        <Profiler
          id="bars"
          onRender={() => {
            commits += 1
          }}
        >
          <Chrome {...props} />
        </Profiler>
      )
      const host = mount(bars(baseProps))
      railStandsAt(host, QUARTER)
      expect(mark(host).style.left).toBe(
        'calc(0.5 * (100% - max(32px, 3.125%)))',
      )
      const before = commits
      expect(before).toBeGreaterThan(0)

      railStandsAt(host, { ...QUARTER, scrollLeft: 1200 })

      expect(mark(host).style.left).toBe('calc(1 * (100% - max(32px, 3.125%)))')
      expect(commits).toBe(before)

      // THE CONTROL: the count does move for a render of the bars.
      rerender(bars({ ...baseProps, time: '3m read' }))
      expect(commits).toBe(before + 1)
    })

    // What the mark was told is written on its element and `Chrome` keeps no
    // copy to draw it from again, so a render of `Chrome` for another reason
    // has to leave that element alone: the same one, with the place it had.
    it('leaves the mark where it was through a render for another reason', () => {
      const host = mount(<Chrome {...baseProps} />)
      railStandsAt(host, QUARTER)
      const shown = mark(host)
      const { left, width } = shown.style
      expect(left).toBe('calc(0.5 * (100% - max(32px, 3.125%)))')

      rerender(
        <Chrome
          {...baseProps}
          position="slot 2 / 3"
          time="3m read"
          progress={0.5}
        />,
      )

      // THE CONTROL: the render did land.
      expect(host.querySelector('.reader-readout')?.textContent).toContain(
        '3m read',
      )
      expect(mark(host)).toBe(shown)
      expect(shown.hidden).toBe(false)
      expect([shown.style.left, shown.style.width]).toEqual([left, width])
    })

    // Hiding the bars takes the strip and the mark away, and showing them
    // makes new ones. With the rail where it stood, nothing has changed for
    // the new strip to tell of, and it has to tell all the same: the mark it
    // tells is new, and has nothing written on it.
    //
    // jsdom gives each new rail its own numbers, all zero. So here every
    // element is made to report one rail's numbers, and the rail that comes
    // back stands as the one that went. A standing no other case in this
    // file tells, so that the first strip here has something new to tell.
    it('places the new mark as the old one was when the bars come back with the rail where it stood', () => {
      const numbers = [
        vi.spyOn(Element.prototype, 'scrollWidth', 'get').mockReturnValue(1600),
        vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(400),
        vi.spyOn(Element.prototype, 'scrollLeft', 'get').mockReturnValue(300),
      ]
      onTestFinished(() => {
        for (const spied of numbers) spied.mockRestore()
      })
      const host = mount(<Chrome {...baseProps} />)
      const first = mark(host)
      const { left, width } = first.style
      expect([left, width]).toEqual([
        'calc(0.25 * (100% - max(32px, 3.125%)))',
        'max(32px, 3.125%)',
      ])

      rerender(<Chrome {...baseProps} visible={false} />)
      expect(host.children.length).toBe(0)
      rerender(<Chrome {...baseProps} />)

      const second = mark(host)
      expect(second).not.toBe(first)
      expect(second.hidden).toBe(false)
      expect([second.style.left, second.style.width]).toEqual([left, width])
    })

    // And where the rail that comes back is scrolled afterwards, it is the
    // mark that came back with it that moves.
    it('marks again after the bars have been hidden and shown', () => {
      const host = mount(<Chrome {...baseProps} />)
      railStandsAt(host, QUARTER)
      const first = mark(host)
      expect(first.hidden).toBe(false)

      rerender(<Chrome {...baseProps} visible={false} />)
      expect(host.children.length).toBe(0)
      rerender(<Chrome {...baseProps} />)

      const second = mark(host)
      expect(second).not.toBe(first)
      railStandsAt(host, QUARTER)
      expect(second.hidden).toBe(false)
      expect(second.style.left).toBe('calc(0.5 * (100% - max(32px, 3.125%)))')
    })
  })

  describe('the fade, driven through a real useEffect across a hide/show cycle', () => {
    // A stubbed requestAnimationFrame/cancelAnimationFrame pair, the same
    // shape `motion.test.ts` uses for `scheduleFade` itself: frames are
    // recorded rather than auto-fired, so a test controls exactly when a
    // scheduled frame lands.
    let pending: Map<number, FrameRequestCallback>
    let nextId: number

    beforeEach(() => {
      pending = new Map()
      nextId = 0
      vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        nextId += 1
        pending.set(nextId, cb)
        return nextId
      })
      vi.stubGlobal('cancelAnimationFrame', (id: number) => {
        pending.delete(id)
      })
    })

    afterEach(() => {
      vi.unstubAllGlobals()
    })

    const flush = () => {
      const callbacks = [...pending.values()]
      pending.clear()
      for (const cb of callbacks) cb(0)
    }

    /** Both bars' opacity — they fade as one. */
    const opacities = (host: HTMLElement) => [
      (host.querySelector('.reader-top-bar') as HTMLElement).style.opacity,
      bottomBar(host).style.opacity,
    ]

    it('starts fully shown, with no opacity declared, when animation is off', () => {
      const host = mount(<Chrome {...baseProps} />)
      expect(opacities(host)).toEqual(['', ''])
      expect(pending.size).toBe(0)
    })

    it('paints both bars at opacity 0 on first show, then flips them to 1 once the scheduled frame lands', () => {
      const host = mount(
        <Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />,
      )
      expect(opacities(host)).toEqual(['0', '0'])
      expect(pending.size).toBe(1)

      act(() => flush())
      expect(opacities(host)).toEqual(['1', '1'])
    })

    it('resets shown on hide and repeats the same one-frame-late fade on the next show — no flash', () => {
      const host = mount(
        <Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />,
      )
      act(() => flush())
      expect(opacities(host)).toEqual(['1', '1'])

      // Hide: the component instance survives (only its return value goes to
      // null), and the effect's own `!visible` branch is what resets `shown`
      // rather than it surviving stale from before.
      rerender(
        <Chrome
          {...baseProps}
          motion={{ animate: true, ms: 80 }}
          visible={false}
        />,
      )
      expect(host.children.length).toBe(0)

      // Show again, on the same instance: if `shown` had survived the hide
      // unreset, this would paint straight at opacity 1 — a flash of chrome
      // already fully visible with nothing having animated it there.
      rerender(<Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />)
      expect(opacities(host)).toEqual(['0', '0'])

      act(() => flush())
      expect(opacities(host)).toEqual(['1', '1'])
    })
  })

  /**
   * O216. A drag of the strip let go while the pointer is moving carries the
   * rail on, and only where the reader's animation is on: `Chrome` knows that
   * and its `Filmstrip` does not. What a glide is and what stops one is
   * `Filmstrip.test.tsx`'s. Held here is whether one is begun, for animation
   * on and for each of the two ways it can be off.
   *
   * The same flick each time, on the real strip: every event carries the
   * time the case gives it and each frame is run by the case, so nothing
   * here waits on a clock.
   */
  describe('the strip’s glide and the reader’s animation setting', () => {
    let pending: Map<number, FrameRequestCallback>
    let nextId: number
    let clock: number

    beforeEach(() => {
      pending = new Map()
      nextId = 0
      clock = 0
      vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        nextId += 1
        pending.set(nextId, cb)
        return nextId
      })
      vi.stubGlobal('cancelAnimationFrame', (id: number) => {
        pending.delete(id)
      })
      const now = vi.spyOn(performance, 'now').mockImplementation(() => clock)
      onTestFinished(() => now.mockRestore())
      // jsdom implements no part of the Pointer Capture API, and the rail
      // captures the pointer when a press becomes a drag.
      Element.prototype.setPointerCapture = () => {}
      Element.prototype.hasPointerCapture = () => false
      Element.prototype.releasePointerCapture = () => {}
    })

    afterEach(() => {
      vi.unstubAllGlobals()
      Reflect.deleteProperty(Element.prototype, 'setPointerCapture')
      Reflect.deleteProperty(Element.prototype, 'hasPointerCapture')
      Reflect.deleteProperty(Element.prototype, 'releasePointerCapture')
    })

    /** Runs the frames asked for, each stamped `at`, which is what the clock reads by then. */
    const frameAt = (at: number) =>
      act(() => {
        clock = at
        const callbacks = [...pending.values()]
        pending.clear()
        for (const cb of callbacks) cb(at)
      })

    /**
     * A flick of the strip's second tile, 100 px along the rail: pressed at
     * 1000 ms, ten moves 10 ms apart and let go with the last of them, which
     * is a pointer going at 1 px a millisecond. The rail stands 300 px from
     * the start of the volume first, which for a reversed row is a
     * `scrollLeft` under 0. Answers the rail and where it stood.
     */
    function flick(
      host: HTMLElement,
      reading: 'ltr' | 'rtl',
    ): { rail: HTMLElement; stood: number } {
      const rail = host.querySelector('.reader-strip-rail') as HTMLElement
      const stood = reading === 'rtl' ? -300 : 300
      rail.scrollLeft = stood
      const tile = rail.querySelectorAll('button')[1]
      const send = (
        target: Element,
        type: string,
        x: number,
        at: number,
        init: PointerEventInit,
      ) => {
        const event = new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          pointerId: 1,
          pointerType: 'mouse',
          clientX: x,
          ...init,
        })
        Object.defineProperty(event, 'timeStamp', { value: at })
        target.dispatchEvent(event)
      }
      send(tile, 'pointerdown', 500, 1000, { button: 0, buttons: 1 })
      for (let move = 1; move <= 10; move++) {
        send(rail, 'pointermove', 500 + 10 * move, 1000 + 10 * move, {
          button: -1,
          buttons: 1,
        })
      }
      clock = 1100
      send(rail, 'pointerup', 600, 1100, { button: 0, buttons: 0 })
      return { rail, stood }
    }

    for (const reading of ['ltr', 'rtl'] as const) {
      it(`carries the strip on where animation is on, ${reading}`, () => {
        const host = mount(
          <Chrome
            {...baseProps}
            reading={reading}
            motion={{ animate: true, ms: 80 }}
          />,
        )
        // The bars' own fade asks for a frame on showing, and that is past.
        frameAt(900)
        expect(pending.size).toBe(0)

        const { rail, stood } = flick(host, reading)
        expect(rail.scrollLeft).toBe(stood - 100)
        expect(pending.size).toBe(1)
        frameAt(1116)
        expect(rail.scrollLeft).toBeLessThan(stood - 110)
      })

      for (const [how, motion] of [
        ['turned off', { animate: false, ms: 80 }],
        ['on with no time to take', { animate: true, ms: 0 }],
      ] as const) {
        it(`leaves the strip where it was let go where animation is ${how}, ${reading}`, () => {
          const host = mount(
            <Chrome {...baseProps} reading={reading} motion={motion} />,
          )
          expect(pending.size).toBe(0)

          const { rail, stood } = flick(host, reading)
          // THE CONTROL: the flick did drag the rail.
          expect(rail.scrollLeft).toBe(stood - 100)
          expect(pending.size).toBe(0)
          frameAt(1116)
          expect(rail.scrollLeft).toBe(stood - 100)
        })
      }
    }
  })
})

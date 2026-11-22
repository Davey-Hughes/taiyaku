import { act, Profiler } from 'react'
import { MemoryRouter } from 'react-router'
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
import { PAGE_LAYOUTS } from '../src/layouts'

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

// Nor any of the Pointer Capture API, which the rail's drag and a hold of
// the progress line call. For the whole file and not per case: bars a case
// leaves held are let go when they are unmounted, which comes after the
// case's own `afterEach`. Nothing is ever captured here.
Element.prototype.setPointerCapture ??= () => {}
Element.prototype.hasPointerCapture ??= () => false
Element.prototype.releasePointerCapture ??= () => {}

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
  layouts: PAGE_LAYOUTS,
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
    const host = mount(
      <MemoryRouter>
        <Chrome
          {...baseProps}
          series={{ id: 41, title: 'Dandadan' }}
          correcting={{ to: 'ed2' }}
        />
      </MemoryRouter>,
    )
    expect(host.querySelector('.reader-top-bar')).toBeNull()
    expect(buttonNamed(host, 'Library')).toBeUndefined()
    expect(host.querySelector('.reader-title-series')).toBeNull()
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

  describe('Help in the readout row (O425)', () => {
    const helpIn = (host: HTMLElement) =>
      [...host.querySelectorAll('button')].filter(
        (b) => b.getAttribute('aria-label') === 'Help',
      )

    it('draws no Help without onHelp', () => {
      const host = mount(<Chrome {...baseProps} />)
      expect(helpIn(host)).toEqual([])
    })

    it('ends the readout row with one ? named Help, which toggles Help and says whether it is open', () => {
      const onHelp = vi.fn()
      const host = mount(
        <Chrome {...baseProps} onHelp={onHelp} helpOpen={false} />,
      )
      const helps = helpIn(host)
      expect(helps).toHaveLength(1)
      const [help] = helps
      expect(
        (host.querySelector('.reader-readout') as HTMLElement).lastElementChild,
      ).toBe(help)
      expect(help.textContent).toBe('?')
      expect(help.type).toBe('button')
      expect(help.tabIndex).toBe(0)
      expect(help.getAttribute('aria-expanded')).toBe('false')
      // The readout keeps its own text, joined as before.
      expect(host.querySelector('.reader-readout span')?.textContent).toBe(
        'slot 1 / 3 · sound pair · 2m read · c001 · 3 notes',
      )
      help.click()
      expect(onHelp).toHaveBeenCalledOnce()
      rerender(<Chrome {...baseProps} onHelp={onHelp} helpOpen={true} />)
      expect(helpIn(host)[0].getAttribute('aria-expanded')).toBe('true')
    })
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
      // The insets are the stylesheet's, which grows them as the line wakes
      // (O286): one set inline would outweigh every rule that does.
      // `reader.css.test.ts` holds the pixel at rest.
      expect([shown.style.top, shown.style.bottom]).toEqual(['', ''])
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
        'calc(0.5 * (100% - clamp(64px, 25%, 160px)))',
        'clamp(64px, 25%, 160px)',
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

      it(`is as wide as the share shown, held between 64 and 160 px, ${reading}`, () => {
        const host = mount(<Chrome {...baseProps} reading={reading} />)
        railStandsAt(host, stands(QUARTER))
        expect(mark(host).style.width).toBe('clamp(64px, 25%, 160px)')
        railStandsAt(host, stands(SIXTEENTH))
        expect(mark(host).style.width).toBe('clamp(64px, 6.25%, 160px)')
      })

      it(`stands as far along the room its width leaves as the rail is along its travel, ${reading}`, () => {
        const host = mount(<Chrome {...baseProps} reading={reading} />)
        railStandsAt(host, stands(QUARTER))
        expect(mark(host).style.left).toBe(
          'calc(0.5 * (100% - clamp(64px, 25%, 160px)))',
        )
        railStandsAt(host, stands(SIXTEENTH))
        expect(mark(host).style.left).toBe(
          'calc(0.25 * (100% - clamp(64px, 6.25%, 160px)))',
        )
      })

      it(`stands flush with the start of the line at the start of the volume, and with its end at the end, ${reading}`, () => {
        const host = mount(<Chrome {...baseProps} reading={reading} />)
        railStandsAt(host, stands({ ...QUARTER, scrollLeft: 0 }))
        expect(mark(host).style.left).toBe(
          'calc(0 * (100% - clamp(64px, 25%, 160px)))',
        )
        railStandsAt(host, stands({ ...QUARTER, scrollLeft: 1200 }))
        expect(mark(host).style.left).toBe(
          'calc(1 * (100% - clamp(64px, 25%, 160px)))',
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
      expect(mark(host).style.left).toMatch(
        /^calc\(0 \* \(100% - clamp\(64px, /,
      )
    })

    it('mirrors the line for a work read right to left', () => {
      const host = mount(<Chrome {...baseProps} reading="rtl" />)
      expect(line(host).style.transform).toBe('scaleX(-1)')
    })

    it('leaves the line as it is for a work read left to right', () => {
      const host = mount(<Chrome {...baseProps} reading="ltr" />)
      expect(line(host).style.transform).toBe('')
    })

    // O286: the line wakes wherever its press band is drawn, which is
    // wherever the device has a fine pointer (`reader.css.test.ts` holds the
    // stylesheet's one condition). A query that also asked for hover would
    // leave a band taking presses on a line that never wakes.
    it('asks for the wake by `(any-pointer: fine)`', () => {
      const asked = vi.fn((_query: string) => ({ matches: false }))
      vi.stubGlobal('matchMedia', asked)

      mount(<Chrome {...baseProps} />)

      const queries = asked.mock.calls.map(([query]) => query)
      expect(queries).toContain('(any-pointer: fine)')
      expect(queries.filter((query) => /hover/.test(query))).toEqual([])
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

      expect(mark(host).style.left).toMatch(
        /^calc\(1 \* \(100% - clamp\(64px, /,
      )
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
        'calc(0.5 * (100% - clamp(64px, 25%, 160px)))',
      )
      const before = commits
      expect(before).toBeGreaterThan(0)

      railStandsAt(host, { ...QUARTER, scrollLeft: 1200 })

      expect(mark(host).style.left).toBe(
        'calc(1 * (100% - clamp(64px, 25%, 160px)))',
      )
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
      expect(left).toBe('calc(0.5 * (100% - clamp(64px, 25%, 160px)))')

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
        'calc(0.25 * (100% - clamp(64px, 25%, 160px)))',
        'clamp(64px, 25%, 160px)',
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
      expect(second.style.left).toBe(
        'calc(0.5 * (100% - clamp(64px, 25%, 160px)))',
      )
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
    })

    afterEach(() => {
      vi.unstubAllGlobals()
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

  /**
   * O286: the progress line as a scrollbar for a mouse, on numbers given by
   * hand. The line is 1000 px wide and the mark 64, so the mark has 936 px to
   * travel; the rail is 1600 px long with 400 on show, so 1200. Where the
   * mark and the rail land on a real strip is `Chrome.drag.browser.test.tsx`'s.
   */
  describe('the progress line under a mouse (O286)', () => {
    // The strip's scroll listener works in a frame, which lands at once here.
    beforeEach(() => {
      vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        cb(0)
        return 0
      })
    })

    afterEach(() => {
      vi.unstubAllGlobals()
    })

    const box = (left: number, width: number) =>
      ({
        left,
        right: left + width,
        width,
        top: 0,
        bottom: 2,
        height: 2,
        x: left,
        y: 0,
        toJSON: () => ({}),
      }) as DOMRect

    /** The window `x` that lies `d` px from the line's start, which is its right for a work read right to left. */
    const along = (reading: 'ltr' | 'rtl', d: number) =>
      reading === 'ltr' ? d : 1000 - d

    /** The bars over a rail that stands at its start, with the mark drawn at the line's start. */
    function bars(reading: 'ltr' | 'rtl') {
      const host = mount(<Chrome {...baseProps} reading={reading} />)
      const rail = host.querySelector('.reader-strip-rail') as HTMLElement
      Object.defineProperty(rail, 'scrollWidth', {
        value: 1600,
        configurable: true,
      })
      Object.defineProperty(rail, 'clientWidth', {
        value: 400,
        configurable: true,
      })
      act(() => {
        rail.dispatchEvent(new Event('scroll'))
      })
      const line = host.querySelector('.reader-progress') as HTMLElement
      const mark = host.querySelector(
        '[data-testid="strip-shown"]',
      ) as HTMLElement
      expect(mark.hidden).toBe(false)
      line.getBoundingClientRect = () => box(0, 1000)
      mark.getBoundingClientRect = () => box(reading === 'ltr' ? 0 : 936, 64)
      return { rail, line }
    }

    const send = (target: Element, type: string, init: PointerEventInit) =>
      act(() => {
        target.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            pointerId: 1,
            pointerType: 'mouse',
            ...init,
          }),
        )
      })

    // 19.2 px is 0.3 of the way into the mark, and a move of 93.6 px from
    // there stands the mark 93.6 px along its 936: 0.1 of the travel.
    it('scrolls the rail to where a press on the mark and a move put it, both ways', () => {
      for (const [reading, later] of [
        ['ltr', 1],
        ['rtl', -1],
      ] as const) {
        const { rail, line } = bars(reading)
        send(line, 'pointerdown', {
          button: 0,
          buttons: 1,
          clientX: along(reading, 19.2),
        })
        send(line, 'pointermove', {
          button: -1,
          buttons: 1,
          clientX: along(reading, 19.2 + 93.6),
        })
        expect(rail.scrollLeft, reading).toBeCloseTo(later * 120, 6)
      }
    })

    // Pressed half way along the line, well clear of the mark, where a press
    // the line took would move the rail half way along its travel.
    it('takes no press but a mouse’s primary button', () => {
      const { rail, line } = bars('ltr')
      for (const [kind, init] of [
        ['a pen', { pointerType: 'pen', button: 0, buttons: 1 }],
        ['a finger', { pointerType: 'touch', button: 0, buttons: 1 }],
        ['the middle button', { button: 1, buttons: 4 }],
        ['the secondary button', { button: 2, buttons: 2 }],
      ] as const) {
        send(line, 'pointerdown', { ...init, clientX: 500 })
        send(line, 'pointermove', { ...init, button: -1, clientX: 600 })
        expect(rail.scrollLeft, kind).toBe(0)
        expect(line.dataset.held, kind).toBeUndefined()
        send(line, 'pointerup', { ...init, buttons: 0 })
      }

      // THE CONTROL: a mouse's primary button, at the same place, does.
      send(line, 'pointerdown', { button: 0, buttons: 1, clientX: 500 })
      expect(rail.scrollLeft).toBeCloseTo(600, 6)
    })

    it('ends the hold on a move with no button down, on a release, on a cancel and on lost capture', () => {
      for (const [type, init] of [
        ['pointermove', { button: -1, buttons: 0, clientX: 112.8 }],
        ['pointerup', { button: 0, buttons: 0, clientX: 112.8 }],
        ['pointercancel', { button: 0, buttons: 0, clientX: 112.8 }],
        ['lostpointercapture', {}],
      ] as const) {
        const { rail, line } = bars('ltr')
        send(line, 'pointerdown', { button: 0, buttons: 1, clientX: 19.2 })
        send(line, 'pointermove', { button: -1, buttons: 1, clientX: 112.8 })
        // THE STATE THIS TEST NAMES: a hold that moves the rail.
        expect(line.dataset.held, type).toBe('')
        expect(rail.scrollLeft, type).toBeCloseTo(120, 6)

        send(line, type, init)

        expect(line.dataset.held, type).toBeUndefined()
        send(line, 'pointermove', { button: -1, buttons: 1, clientX: 300 })
        expect(rail.scrollLeft, type).toBeCloseTo(120, 6)
      }
    })

    // The default of a `mousedown` is what selects text and moves focus.
    it('prevents the mousedown that follows a held press', () => {
      const { line } = bars('ltr')
      send(line, 'pointerdown', { button: 0, buttons: 1, clientX: 19.2 })
      // THE STATE THIS TEST NAMES: the press took hold of the line.
      expect(line.dataset.held).toBe('')

      const down = new MouseEvent('mousedown', {
        bubbles: true,
        cancelable: true,
        button: 0,
        buttons: 1,
      })
      act(() => {
        line.dispatchEvent(down)
      })
      expect(down.defaultPrevented).toBe(true)
    })

    // Its default would move focus and select text, held or not.
    it('prevents a primary mousedown on the line that holds nothing, and leaves a secondary button’s alone', () => {
      const { line } = bars('ltr')
      const down = (button: number) => {
        const event = new MouseEvent('mousedown', {
          bubbles: true,
          cancelable: true,
          button,
          buttons: button === 0 ? 1 : 2,
        })
        act(() => {
          line.dispatchEvent(event)
        })
        return event
      }
      // THE STATE THIS TEST NAMES: nothing holds the line.
      expect(line.dataset.held).toBeUndefined()

      expect(down(0).defaultPrevented).toBe(true)
      expect(down(2).defaultPrevented).toBe(false)
    })

    it('prevents a mouse press on the band where the mark is hidden, and leaves a pen’s alone', () => {
      const host = mount(<Chrome {...baseProps} />)
      const rail = host.querySelector('.reader-strip-rail') as HTMLElement
      for (const width of ['scrollWidth', 'clientWidth']) {
        Object.defineProperty(rail, width, { value: 400, configurable: true })
      }
      act(() => {
        rail.dispatchEvent(new Event('scroll'))
      })
      const line = host.querySelector('.reader-progress') as HTMLElement
      const grip = host.querySelector('.reader-progress-grip') as HTMLElement
      line.getBoundingClientRect = () => box(0, 1000)
      const press = (pointerType: string, pointerId: number) => {
        const event = new PointerEvent('pointerdown', {
          bubbles: true,
          cancelable: true,
          pointerId,
          pointerType,
          button: 0,
          buttons: 1,
          clientX: 500,
        })
        act(() => {
          grip.dispatchEvent(event)
        })
        return event
      }
      // THE STATE THIS TEST NAMES: the strip shows the whole volume, so
      // there is no mark to take.
      expect(
        host.querySelector<HTMLElement>('[data-testid="strip-shown"]')?.hidden,
      ).toBe(true)

      expect(press('mouse', 1).defaultPrevented).toBe(true)
      expect(line.dataset.held).toBeUndefined()
      expect(press('pen', 2).defaultPrevented).toBe(false)
    })

    // Held 16 px into the mark (0.25 of its 64) and moved 234 px along its
    // 936, the pointer asks for 0.25 of the travel: 300 px of the rail's
    // 1200. A load then grows the rail to 3200 px, where 300 px is 0.107 of
    // the 2800 it can travel, and shows 12.5 % of it.
    it('writes the mark where the held pointer puts it when the rail’s length changes, both ways', () => {
      for (const [reading, later] of [
        ['ltr', 1],
        ['rtl', -1],
      ] as const) {
        const { rail, line } = bars(reading)
        const mark = line.querySelector(
          '[data-testid="strip-shown"]',
        ) as HTMLElement
        send(line, 'pointerdown', {
          button: 0,
          buttons: 1,
          clientX: along(reading, 16),
        })
        send(line, 'pointermove', {
          button: -1,
          buttons: 1,
          clientX: along(reading, 250),
        })
        expect(rail.scrollLeft, reading).toBeCloseTo(later * 300, 6)
        Object.defineProperty(rail, 'scrollWidth', {
          value: 3200,
          configurable: true,
        })
        // THE STATE THIS TEST NAMES: the grown rail stands somewhere along
        // its travel other than where the pointer asks.
        expect(
          Math.abs(rail.scrollLeft) / (rail.scrollWidth - rail.clientWidth),
          reading,
        ).not.toBeCloseTo(0.25, 2)

        act(() => {
          rail.dispatchEvent(new Event('load'))
        })

        expect(mark.style.left, reading).toBe(
          'calc(0.25 * (100% - clamp(64px, 12.5%, 160px)))',
        )
        expect(rail.scrollLeft, reading).toBeCloseTo(later * 700, 6)
      }
    })
  })

  /**
   * O286: the line wakes for a mouse, on a clock run by hand. The bar is
   * 1440 px wide and the line 12 to 1428 within it, 2 px tall at y 600, so
   * the keep zone runs from y 584 to 618 and past the line's ends to the
   * bar's. The rail stands as the describe above stands it, so a press on
   * the line takes hold. A real mouse in real time is
   * `Chrome.wake.browser.test.tsx`'s.
   */
  describe('the line wakes for a mouse (O286)', () => {
    /** `matchMedia`, answering the one query the wake asks as `fine` says, and no to any other. */
    const finePointer = (fine: boolean) =>
      vi.stubGlobal('matchMedia', (query: string) => ({
        matches: fine && query === '(any-pointer: fine)',
      }))

    beforeEach(() => {
      vi.useFakeTimers({
        toFake: [
          'setTimeout',
          'clearTimeout',
          'requestAnimationFrame',
          'cancelAnimationFrame',
          'performance',
        ],
      })
      finePointer(true)
    })

    afterEach(() => {
      vi.useRealTimers()
      vi.unstubAllGlobals()
    })

    const rect = (left: number, right: number, top: number, bottom: number) =>
      ({
        left,
        right,
        top,
        bottom,
        width: right - left,
        height: bottom - top,
        x: left,
        y: top,
        toJSON: () => ({}),
      }) as DOMRect

    /** Stands the rail in `host` at its start, 1600 px long with 400 on show, and gives the bar, the line and the mark their boxes. */
    function stand(host: HTMLElement) {
      const rail = host.querySelector('.reader-strip-rail') as HTMLElement
      Object.defineProperty(rail, 'scrollWidth', {
        value: 1600,
        configurable: true,
      })
      Object.defineProperty(rail, 'clientWidth', {
        value: 400,
        configurable: true,
      })
      act(() => {
        rail.dispatchEvent(new Event('scroll'))
        vi.advanceTimersToNextFrame()
      })
      const line = host.querySelector('.reader-progress') as HTMLElement
      const mark = host.querySelector(
        '[data-testid="strip-shown"]',
      ) as HTMLElement
      expect(mark.hidden).toBe(false)
      bottomBar(host).getBoundingClientRect = () => rect(0, 1440, 400, 636)
      line.getBoundingClientRect = () => rect(12, 1428, 600, 602)
      mark.getBoundingClientRect = () => rect(12, 76, 599, 603)
      return { rail, line }
    }

    function bars() {
      const host = mount(<Chrome {...baseProps} />)
      return { host, ...stand(host) }
    }

    /** Sends a mouse's pointer event, unless `init` names another pointer, to the window or to `target`, and returns when. */
    function send(
      type: string,
      init: PointerEventInit,
      target: EventTarget = window,
    ): number {
      act(() => {
        target.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            pointerId: 1,
            pointerType: 'mouse',
            ...init,
          }),
        )
      })
      return performance.now()
    }

    /** A move to (`x`, `y`) with no button down, and when it was sent. */
    const moveTo = (x: number, y: number, init: PointerEventInit = {}) =>
      send('pointermove', {
        clientX: x,
        clientY: y,
        button: -1,
        buttons: 0,
        ...init,
      })

    const nextFrame = () =>
      act(() => {
        vi.advanceTimersToNextFrame()
      })

    /** Runs the clock on to `ms` after `since`, a time `send` returned. */
    const runTo = (since: number, ms: number) =>
      act(() => {
        vi.advanceTimersByTime(since + ms - performance.now())
      })

    const awake = (line: HTMLElement) => line.dataset.awake !== undefined

    type Calls = readonly (readonly [
      string,
      unknown,
      (boolean | EventListenerOptions)?,
    ])[]

    /**
     * The wake's listeners among those the window and the document were
     * given or had taken off: moves and releases on the window, and the
     * pointer's leaving on the document. The strip's drag hears a release on
     * the window too, by the listener it hears a cancel by; the wake hears
     * no cancel.
     */
    function wakeListeners(calls: { window: Calls; document: Calls }) {
      const of = (made: Calls, types: string[]) => {
        const strips = new Set(
          made
            .filter(([type]) => type === 'pointercancel')
            .map(([, listener]) => listener),
        )
        return made
          .filter(
            ([type, listener]) => types.includes(type) && !strips.has(listener),
          )
          .map(([type, listener, options]) => ({
            type,
            listener,
            capture:
              typeof options === 'boolean'
                ? options
                : Boolean(options?.capture),
          }))
      }
      return [
        ...of(calls.window, ['pointermove', 'pointerup']),
        ...of(calls.document, ['pointerout']),
      ]
    }

    /** Spies on the window's and the document's `method`, from now to the end of the case. */
    function listening(method: 'addEventListener' | 'removeEventListener') {
      const onWindow = vi.spyOn(window, method)
      const onDocument = vi.spyOn(document, method)
      onTestFinished(() => {
        onWindow.mockRestore()
        onDocument.mockRestore()
      })
      return {
        calls: () => ({
          window: onWindow.mock.calls,
          document: onDocument.mock.calls,
        }),
        clear: () => {
          onWindow.mockClear()
          onDocument.mockClear()
        },
      }
    }

    it('wakes a frame after a mouse moves anywhere, and sleeps 1000 ms after the last move', () => {
      const { line } = bars()
      // THE STATE THIS TEST NAMES: asleep, with nothing moved yet.
      expect(awake(line)).toBe(false)

      const moved = moveTo(700, 300)
      expect(awake(line)).toBe(false)
      nextFrame()
      expect(awake(line)).toBe(true)
      runTo(moved, 999)
      expect(awake(line)).toBe(true)
      runTo(moved, 1000)
      expect(awake(line)).toBe(false)

      const first = moveTo(700, 300)
      nextFrame()
      runTo(first, 500)
      moveTo(720, 310)
      nextFrame()
      runTo(first, 1499)
      expect(awake(line)).toBe(true)
      runTo(first, 1500)
      expect(awake(line)).toBe(false)
    })

    it('counts a release as a move', () => {
      const { line } = bars()
      const released = send('pointerup', {
        clientX: 700,
        clientY: 300,
        button: 0,
        buttons: 0,
      })
      nextFrame()
      expect(awake(line)).toBe(true)
      runTo(released, 999)
      expect(awake(line)).toBe(true)
      runTo(released, 1000)
      expect(awake(line)).toBe(false)
    })

    it('wakes for no touch and no pen', () => {
      const { line } = bars()
      for (const pointerType of ['touch', 'pen']) {
        const moved = moveTo(700, 300, { pointerType })
        send('pointerup', {
          pointerType,
          clientX: 700,
          clientY: 300,
          button: 0,
          buttons: 0,
        })
        nextFrame()
        runTo(moved, 100)
        expect(awake(line), pointerType).toBe(false)
      }

      // THE CONTROL: a mouse's move, at the same place, does.
      moveTo(700, 300)
      nextFrame()
      expect(awake(line)).toBe(true)
    })

    // `x` 5 and 1435 lie on the bar, and off the line's ends.
    it('stays awake while the pointer rests within 16 px of the line, across the whole bar', () => {
      const { line } = bars()
      for (const [x, y, kept] of [
        [5, 618, true],
        [5, 619, false],
        [5, 583, false],
        [1435, 610, true],
        [5, 584, true],
      ] as const) {
        const at = `(${x}, ${y})`
        const moved = moveTo(x, y)
        nextFrame()
        runTo(moved, 999)
        expect(awake(line), at).toBe(true)
        runTo(moved, kept ? 5000 : 1000)
        expect(awake(line), at).toBe(kept)
      }

      // Rested at (5, 584), then moved out of the zone.
      const moved = moveTo(700, 300)
      nextFrame()
      runTo(moved, 999)
      expect(awake(line)).toBe(true)
      runTo(moved, 1000)
      expect(awake(line)).toBe(false)
    })

    // Pressed on the mark and dragged 98 px below the line, out of the zone,
    // where only the hold keeps it awake.
    it('stays awake through a hold, and sleeps 1000 ms after its release', () => {
      const { line } = bars()
      send(
        'pointerdown',
        { clientX: 44, clientY: 601, button: 0, buttons: 1 },
        line,
      )
      const moved = send(
        'pointermove',
        { clientX: 84, clientY: 700, button: -1, buttons: 1 },
        line,
      )
      // THE STATE THIS TEST NAMES: a hold, resting out of the zone.
      expect(line.dataset.held).toBe('')
      nextFrame()
      runTo(moved, 5000)
      expect(awake(line)).toBe(true)

      const released = send(
        'pointerup',
        { clientX: 84, clientY: 700, button: 0, buttons: 0 },
        line,
      )
      expect(line.dataset.held).toBeUndefined()
      nextFrame()
      runTo(released, 999)
      expect(awake(line)).toBe(true)
      runTo(released, 1000)
      expect(awake(line)).toBe(false)
    })

    // A browser says a pointer left the window by a `pointerout` with nothing
    // to go to, straight after its last move there.
    it('sleeps once the pointer leaves the window, wherever it last was', () => {
      const { line } = bars()
      // Neither a move onto another element nor a pen's leaving is the
      // mouse's leaving.
      const rested = moveTo(5, 610)
      send(
        'pointerout',
        { clientX: 5, clientY: 610, relatedTarget: document.body },
        document,
      )
      send('pointerout', { pointerType: 'pen', relatedTarget: null }, document)
      nextFrame()
      runTo(rested, 5000)
      // THE STATE THIS TEST NAMES: the pointer rests in the zone, which
      // keeps the line awake.
      expect(awake(line)).toBe(true)

      const moved = moveTo(5, 610)
      send(
        'pointerout',
        { clientX: 5, clientY: 610, relatedTarget: null },
        document,
      )
      nextFrame()
      runTo(moved, 999)
      expect(awake(line)).toBe(true)
      runTo(moved, 1000)
      expect(awake(line)).toBe(false)
    })

    // Moved, a frame let pass, and moved again, so the wake has a timer and
    // a frame in hand when it goes.
    it('leaves nothing behind when the bars hide, or when the reader goes', () => {
      const added = listening('addEventListener')
      const removed = listening('removeEventListener')
      for (const [how, go] of [
        [
          'the bars hide',
          () => rerender(<Chrome {...baseProps} visible={false} />),
        ],
        ['the reader goes', () => rerender(null)],
      ] as const) {
        added.clear()
        removed.clear()
        const { line } = bars()
        moveTo(700, 300)
        nextFrame()
        moveTo(710, 300)
        const listened = wakeListeners(added.calls())
        // THE STATE THIS TEST NAMES: the wake listens, and the line is awake
        // with a frame and a timer pending.
        expect(listened.map(({ type }) => type).sort(), how).toEqual([
          'pointermove',
          'pointerout',
          'pointerup',
        ])
        expect(awake(line), how).toBe(true)
        expect(vi.getTimerCount(), how).toBe(2)

        go()

        expect(wakeListeners(removed.calls()), how).toEqual(
          expect.arrayContaining(listened),
        )
        expect(vi.getTimerCount(), how).toBe(0)
      }
    })

    // The bars hidden mid-hold take the line it was on with them. A hold
    // left on would keep the next line awake until a press on it, or a move
    // over it with no button down, ended it: the sleep a second after the
    // move is what holds that. The button-down move after it is a second
    // check and not the reason: in a browser the removed line's box is all
    // zeros, so a hold left on drags nothing, but here the stub's box stays
    // on it.
    it('ends a hold when the bars hide, and the line comes back at rest', () => {
      const { host, line } = bars()
      const released = vi.fn()
      line.hasPointerCapture = () => true
      line.releasePointerCapture = released
      send(
        'pointerdown',
        { clientX: 44, clientY: 601, button: 0, buttons: 1 },
        line,
      )
      // THE STATE THIS TEST NAMES: a hold.
      expect(line.dataset.held).toBe('')

      rerender(<Chrome {...baseProps} visible={false} />)
      expect(line.dataset.held).toBeUndefined()
      expect(released).toHaveBeenCalledWith(1)

      rerender(<Chrome {...baseProps} />)
      const { rail, line: again } = stand(host)
      expect(again).not.toBe(line)
      expect(again.dataset.held).toBeUndefined()
      expect(awake(again)).toBe(false)

      const moved = moveTo(700, 300)
      nextFrame()
      expect(awake(again)).toBe(true)
      runTo(moved, 1000)
      expect(awake(again)).toBe(false)

      const stood = rail.scrollLeft
      send(
        'pointermove',
        { clientX: 600, clientY: 601, button: -1, buttons: 1 },
        again,
      )
      expect(rail.scrollLeft).toBe(stood)
    })

    it('neither listens nor wakes where the device has no fine pointer', () => {
      finePointer(false)
      const added = listening('addEventListener')
      const { line } = bars()
      // THE STATE THIS TEST NAMES: the spies hear the mount, where the
      // strip's drag listens on the window.
      expect(added.calls().window.map(([type]) => type)).toContain(
        'pointercancel',
      )

      expect(wakeListeners(added.calls())).toEqual([])
      const moved = moveTo(700, 300)
      nextFrame()
      runTo(moved, 100)
      expect(awake(line)).toBe(false)
    })
  })
})

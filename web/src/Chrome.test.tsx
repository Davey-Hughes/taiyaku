import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Chrome, type ChromeProps } from './Chrome'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { mount, rerender } from './testing/mount'

/**
 * `Chrome` is the two bars — `TopBar.tsx` (its own test) owns the top bar's
 * controls, `Filmstrip.tsx` (its own test) owns tile layout,
 * `modules/motion.ts`'s `scheduleFade` (pinned in `motion.test.ts`) owns the
 * frame-scheduling arithmetic, and `modules/readouts.ts`'s `readoutRow`
 * (pinned in `readouts.test.ts`) owns the join. What is only pinned here is
 * what `Chrome` itself does: the `visible` unmount gate, where each bar docks
 * and what goes in it, the top bar standing down while correcting, wiring the
 * fade through a real `useEffect` across a hide/show cycle, the hover-preview
 * state it owns, and passing the right, gated fields to `readoutRow`.
 */

// jsdom has no layout engine and implements no `scrollIntoView` —
// `Filmstrip.tsx`'s own centring effect calls it on mount. Stubbed only so
// that effect does not throw; what it centres on is `Filmstrip`'s test's
// concern.
Element.prototype.scrollIntoView ??= () => {}

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
   * Spec 2026-09-17 §8.3, reversing O65's dock above the strip: the way back
   * is in the top bar now, which is inside this component, so it keeps the
   * fade and stands down while correcting as O65's button did.
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
})

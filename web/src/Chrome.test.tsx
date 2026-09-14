import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Chrome, type ChromeProps } from './Chrome'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { mount, rerender } from './testing/mount'

/**
 * `Chrome` is the strip-plus-readout-row wrapper — `Filmstrip.tsx` (its own
 * test, elsewhere) owns tile layout, `modules/motion.ts`'s `scheduleFade`
 * (pinned in `motion.test.ts`) owns the frame-scheduling arithmetic, and
 * `modules/readouts.ts`'s `readoutRow` (pinned in `readouts.test.ts`) owns
 * the join. What is only pinned here is what `Chrome` itself does: the
 * `visible` unmount gate, wiring the fade schedule through a real
 * `useEffect` across a hide/show cycle on one instance, the hover-preview
 * state it owns (because the preview has to escape the strip's own
 * `overflow: hidden`), and passing the right, gated fields to `readoutRow`.
 */

// jsdom has no layout engine and, relatedly, implements no `scrollIntoView`
// at all — `Filmstrip.tsx`'s own centring effect calls it unconditionally on
// mount and on every dependency change, and `Chrome` always mounts a
// `Filmstrip` as a child. Stubbed here only so that effect does not throw;
// what it centres on is `Filmstrip`'s own test's concern, not this file's.
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
  reading: 'ltr',
  work: 'work1',
  edition: 'ed1',
  slots,
  groups,
  currentIndex: 0,
  onNavigate: vi.fn(),
  onLeave: vi.fn(),
}

describe('Chrome', () => {
  it('renders nothing while hidden — an unmount, not merely invisible chrome', () => {
    const host = mount(<Chrome {...baseProps} visible={false} />)
    expect(host.children.length).toBe(0)
  })

  it('renders the strip and a status row while visible', () => {
    const host = mount(<Chrome {...baseProps} />)
    // One button per group's tile, plus the control back to the library.
    expect(host.querySelectorAll('button').length).toBe(groups.length + 1)
    expect(host.querySelector('[role="status"]')).not.toBeNull()
  })

  it('gives the reader a way back to the library', () => {
    const onLeave = vi.fn()
    const host = mount(<Chrome {...baseProps} onLeave={onLeave} />)
    const leave = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Library',
    )
    expect(leave).toBeDefined()
    leave?.click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('docks the way back at a corner none of the reader’s own panels use, and stacks at or above them', () => {
    const host = mount(<Chrome {...baseProps} />)
    const leave = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Library',
    ) as HTMLElement

    // jsdom has no layout engine (`App.test.tsx`'s own note on this), so
    // nothing here can ask whether this button's box actually overlaps
    // `CreditsPanel`'s, `Help`'s or `NotePanel`'s — all three dock their
    // shared shell at `top: 16, right: 16, zIndex: 3` (see e.g.
    // `NotePanel.tsx`'s `panelStyle`). What jsdom answers honestly is the
    // style actually declared: this button used to carry that exact same
    // `top: 16, right: 16` with no `zIndex` at all, which is what let an
    // open panel's explicit `zIndex: 3` cover it outright. The pin is that
    // `right` is gone — not merely that `left` is present — since `left`
    // alongside a leftover `right` would still be the same box.
    expect(leave.style.left).toBe('16px')
    expect(leave.style.right).toBe('')

    // Belt and suspenders: an explicit `zIndex` at least as high as the
    // panels' own `zIndex: 3`, so a future change that ever does share their
    // corner again loses no stacking fight the way the implicit `auto` did.
    expect(Number(leave.style.zIndex)).toBeGreaterThanOrEqual(3)
  })

  it('docks the way back just above the strip, not on the top edge where App’s layout buttons sit', () => {
    const host = mount(<Chrome {...baseProps} />)
    const leave = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Library',
    ) as HTMLElement

    // O65: at `top: 16` this button sat over `App.tsx`'s `single` and `two`
    // mode buttons, which lay out in normal flow at 8,8, so a click meant for
    // either one left the reader. `App.browser.test.tsx` measures that with a
    // real layout; jsdom has none, so what this pins is the declared
    // geometry — no `top` at all, and a `bottom` that clears the strip's own
    // top edge, read from the strip's style rather than copied from it.
    expect(leave.style.top).toBe('')
    const strip = host.querySelector('[role="status"]')
      ?.previousElementSibling as HTMLElement
    const stripTop =
      Number.parseFloat(strip.style.bottom) +
      Number.parseFloat(strip.style.height)
    expect(Number.parseFloat(leave.style.bottom)).toBeGreaterThan(stripTop)
  })

  it('hides the way back to the library while correcting, when the hover preview stands in its band', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    const leave = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Library',
    )
    expect(leave).toBeUndefined()
  })

  it('shows the way back to the library when not correcting', () => {
    const host = mount(<Chrome {...baseProps} />)
    const leave = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Library',
    )
    expect(leave).toBeDefined()
  })

  it('joins the readout row in position/health/time/chapter/notes order', () => {
    const host = mount(<Chrome {...baseProps} />)
    expect(host.querySelector('[role="status"]')?.textContent).toBe(
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
    expect(host.querySelector('[role="status"]')?.textContent).toBe('')
  })

  it('shows no hover preview before any tile is focused, even while correcting', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    expect(host.querySelector('img[alt=""]')).toBeNull()
  })

  it('reveals the hovered tile’s page as a preview while correcting, labelled with its half', () => {
    const host = mount(<Chrome {...baseProps} correcting={{ to: 'ed2' }} />)
    const tiles = host.querySelectorAll('button')

    // Tile 0 is slot 0, a plain page with no half.
    act(() => tiles[0]?.focus())
    let img = host.querySelector('img[alt=""]') as HTMLImageElement
    expect(img).not.toBeNull()
    expect(img.getAttribute('src')).toBe('/api/works/work1/ed1/pages/0')
    expect(img.nextElementSibling?.textContent).toBe('page 1')

    // Tile 1 is slot 1, one half ('L') of a joined spread.
    act(() => tiles[1]?.focus())
    img = host.querySelector('img[alt=""]') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('/api/works/work1/ed1/pages/1')
    expect(img.nextElementSibling?.textContent).toBe('page 2 L')

    // Blurring the tile clears the preview rather than leaving it stuck.
    act(() => tiles[1]?.blur())
    expect(host.querySelector('img[alt=""]')).toBeNull()
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

    const opacityOf = (host: HTMLElement) =>
      (host.querySelector('[role="status"]') as HTMLElement).style.opacity

    it('starts fully shown, with no opacity declared, when animation is off', () => {
      const host = mount(<Chrome {...baseProps} />)
      expect(opacityOf(host)).toBe('')
      expect(pending.size).toBe(0)
    })

    it('paints at opacity 0 on first show, then flips to 1 once the scheduled frame lands', () => {
      const host = mount(
        <Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />,
      )
      expect(opacityOf(host)).toBe('0')
      expect(pending.size).toBe(1)

      act(() => flush())
      expect(opacityOf(host)).toBe('1')
    })

    it('resets shown on hide and repeats the same one-frame-late fade on the next show — no flash', () => {
      const host = mount(
        <Chrome {...baseProps} motion={{ animate: true, ms: 80 }} />,
      )
      act(() => flush())
      expect(opacityOf(host)).toBe('1')

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
      expect(opacityOf(host)).toBe('0')

      act(() => flush())
      expect(opacityOf(host)).toBe('1')
    })
  })
})

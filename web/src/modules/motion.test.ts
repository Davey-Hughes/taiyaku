import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  fadeStyle,
  isAnimating,
  motionModule,
  scheduleFade,
  transitionStyle,
} from './motion'

describe('transitionStyle', () => {
  it('emits no transition at all when animation is off', () => {
    expect(transitionStyle({ animate: false, ms: 120 })).toEqual({})
  })

  it('uses the configured duration when animation is on', () => {
    expect(transitionStyle({ animate: true, ms: 80 })).toEqual({
      transition: 'opacity 80ms ease',
    })
  })

  it('treats a zero duration as no transition rather than a 0ms one', () => {
    expect(transitionStyle({ animate: true, ms: 0 })).toEqual({})
  })
})

describe('isAnimating', () => {
  it('is true with animation on and a positive duration', () => {
    expect(isAnimating({ animate: true, ms: 80 })).toBe(true)
  })

  it('is false when animation is off', () => {
    expect(isAnimating({ animate: false, ms: 120 })).toBe(false)
  })

  it('is false at a zero duration, even with animation on', () => {
    expect(isAnimating({ animate: true, ms: 0 })).toBe(false)
  })

  it('agrees with transitionStyle on every input, asked directly rather than duck-typed', () => {
    for (const prefs of [
      { animate: true, ms: 80 },
      { animate: false, ms: 80 },
      { animate: true, ms: 0 },
    ]) {
      expect(isAnimating(prefs)).toBe('transition' in transitionStyle(prefs))
    }
  })
})

describe('fadeStyle', () => {
  it('emits nothing when not animating, whether settled shown or not', () => {
    expect(fadeStyle({ animate: false, ms: 120 }, true)).toEqual({})
    expect(fadeStyle({ animate: false, ms: 120 }, false)).toEqual({})
  })

  it('merges the transition with opacity 0 before the fade has settled', () => {
    expect(fadeStyle({ animate: true, ms: 80 }, false)).toEqual({
      transition: 'opacity 80ms ease',
      opacity: 0,
    })
  })

  it('merges the transition with opacity 1 once settled', () => {
    expect(fadeStyle({ animate: true, ms: 80 }, true)).toEqual({
      transition: 'opacity 80ms ease',
      opacity: 1,
    })
  })
})

describe('scheduleFade', () => {
  // A minimal fake `requestAnimationFrame`/`cancelAnimationFrame` pair — no
  // jsdom, no happy-dom, just two functions stubbed onto `globalThis`. Frames
  // are recorded rather than auto-fired, so a test controls exactly when (or
  // whether) a scheduled callback runs.
  let pending: Map<number, FrameRequestCallback>
  let nextId: number
  let cancelled: number[]

  beforeEach(() => {
    pending = new Map()
    nextId = 0
    cancelled = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      nextId += 1
      pending.set(nextId, cb)
      return nextId
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      cancelled.push(id)
      pending.delete(id)
    })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('settles shown immediately, without scheduling a frame, when not animating', () => {
    const setShown = vi.fn()
    scheduleFade(true, false, setShown)
    expect(setShown).toHaveBeenCalledTimes(1)
    expect(setShown).toHaveBeenCalledWith(true)
    expect(pending.size).toBe(0)
  })

  it('schedules exactly one frame to flip shown to true when visible and animating', () => {
    const setShown = vi.fn()
    scheduleFade(true, true, setShown)
    expect(pending.size).toBe(1)
    expect(setShown).not.toHaveBeenCalled()
  })

  it('does not schedule a frame when hiding, animating or not', () => {
    const setShown = vi.fn()
    scheduleFade(false, true, setShown)
    expect(pending.size).toBe(0)
    expect(setShown).toHaveBeenCalledTimes(1)
    expect(setShown).toHaveBeenCalledWith(false)
  })

  it('cancels a still-pending frame on a rapid toggle, rather than leaving it to fire against stale state', () => {
    const setShown = vi.fn()
    const cancelFirst = scheduleFade(true, true, setShown)
    const [firstId] = pending.keys()

    // Simulates React re-running the effect before the frame ever fires —
    // exactly a rapid show/hide/show toggle: the previous effect's cleanup
    // runs first.
    cancelFirst()

    expect(cancelled).toEqual([firstId])
    expect(pending.size).toBe(0)
    // The frame must never get the chance to call back — if it did, `shown`
    // would flip to true against whatever toggle happened after this one.
    expect(setShown).not.toHaveBeenCalled()
  })
})

describe('motionModule', () => {
  it('defaults to animating, at a duration a reader can lower to zero', () => {
    expect(motionModule.defaults).toEqual({ animate: true, ms: 120 })
  })

  it('claims no keys — it is tuned in settings, not mid-read', () => {
    expect(motionModule.keys ?? []).toEqual([])
  })
})

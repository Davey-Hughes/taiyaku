import { describe, expect, it } from 'vitest'
import { prefetchModule, window as prefetchWindow } from './prefetch'

describe('window', () => {
  it('takes N ahead and M behind', () => {
    expect(prefetchWindow(10, 100, 2, 1)).toEqual([9, 11, 12])
  })

  it('clamps at the start of a volume', () => {
    expect(prefetchWindow(0, 100, 2, 2)).toEqual([1, 2])
  })

  it('clamps at the end', () => {
    expect(prefetchWindow(99, 100, 2, 1)).toEqual([98])
  })

  it('never includes the current slot, which is already on screen', () => {
    expect(prefetchWindow(5, 100, 3, 3)).not.toContain(5)
  })

  it('is empty when both depths are zero, so the module can be switched off', () => {
    expect(prefetchWindow(5, 100, 0, 0)).toEqual([])
  })

  it('orders nearest-first, so a cancelled flick has fetched the useful ones', () => {
    expect(prefetchWindow(10, 100, 3, 0)).toEqual([11, 12, 13])
  })

  it('interleaves ahead and behind by distance — both slots one away before either slot two away', () => {
    expect(prefetchWindow(10, 100, 2, 2)).toEqual([9, 11, 8, 12])
  })

  it('is empty on a single-slot volume, with nothing either side to fetch', () => {
    expect(prefetchWindow(0, 1, 3, 3)).toEqual([])
  })
})

describe('prefetchModule', () => {
  it('defaults to enabled, a small asymmetric window and a capped decode concurrency', () => {
    expect(prefetchModule.defaults).toEqual({
      enabled: true,
      ahead: 2,
      behind: 1,
      maxConcurrent: 3,
    })
  })

  it('claims no keys — tuned in settings, not pressed mid-read', () => {
    expect(prefetchModule.keys ?? []).toEqual([])
  })
})

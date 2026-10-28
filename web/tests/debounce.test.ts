import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { debounce } from '../src/debounce'

describe('debounce', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('coalesces a burst of calls into one, keeping only the last arguments', () => {
    // Same shape as arrowing through twenty pages: twenty calls in quick
    // succession must not become twenty `PUT`s.
    const fn = vi.fn()
    const write = debounce(fn, 400)

    for (let i = 0; i < 20; i++) write(i)
    expect(fn).not.toHaveBeenCalled()

    vi.advanceTimersByTime(400)
    expect(fn).toHaveBeenCalledTimes(1)
    expect(fn).toHaveBeenCalledWith(19)
  })

  it('does not fire before the delay has elapsed', () => {
    const fn = vi.fn()
    const write = debounce(fn, 400)

    write('a')
    vi.advanceTimersByTime(399)
    expect(fn).not.toHaveBeenCalled()
  })

  it('resets the window on every call, not just the first', () => {
    const fn = vi.fn()
    const write = debounce(fn, 400)

    write(1)
    vi.advanceTimersByTime(300)
    write(2) // resets the clock; 300ms since call 1 must not count toward this one
    vi.advanceTimersByTime(300)
    expect(fn).not.toHaveBeenCalled()

    vi.advanceTimersByTime(100)
    expect(fn).toHaveBeenCalledTimes(1)
    expect(fn).toHaveBeenCalledWith(2)
  })

  it('fires separately for two calls a full window apart', () => {
    const fn = vi.fn()
    const write = debounce(fn, 400)

    write('a')
    vi.advanceTimersByTime(400)
    write('b')
    vi.advanceTimersByTime(400)

    expect(fn).toHaveBeenCalledTimes(2)
    expect(fn).toHaveBeenNthCalledWith(1, 'a')
    expect(fn).toHaveBeenNthCalledWith(2, 'b')
  })

  describe('flush', () => {
    // This is the whole point: a tab closing within the debounce window must
    // not lose the position it was about to write. The reader shell calls
    // `flush` from a `visibilitychange`/`pagehide` handler for exactly this
    // reason — see `debounce.ts`'s module doc.
    it('sends the pending call immediately, without waiting out the delay', () => {
      const fn = vi.fn()
      const write = debounce(fn, 400)

      write('a')
      expect(fn).not.toHaveBeenCalled()
      write.flush()

      expect(fn).toHaveBeenCalledTimes(1)
      expect(fn).toHaveBeenCalledWith('a')
    })

    it('does not double-send when the timer later fires after a flush', () => {
      const fn = vi.fn()
      const write = debounce(fn, 400)

      write('a')
      write.flush()
      vi.advanceTimersByTime(400)

      expect(fn).toHaveBeenCalledTimes(1)
    })

    it('is a no-op when nothing is pending', () => {
      const fn = vi.fn()
      const write = debounce(fn, 400)

      write.flush()

      expect(fn).not.toHaveBeenCalled()
    })

    it('is a no-op on a second flush once the first already sent', () => {
      const fn = vi.fn()
      const write = debounce(fn, 400)

      write('a')
      write.flush()
      write.flush()

      expect(fn).toHaveBeenCalledTimes(1)
    })

    it('does not resurrect a call already sent by the timer firing naturally', () => {
      // The ordinary case that must not regress: the debounce flushed on its
      // own, and a later flush (e.g. from `pagehide` after `visibilitychange`
      // already handled it) must not re-send.
      const fn = vi.fn()
      const write = debounce(fn, 400)

      write('a')
      vi.advanceTimersByTime(400)
      expect(fn).toHaveBeenCalledTimes(1)

      write.flush()
      expect(fn).toHaveBeenCalledTimes(1)
    })
  })
})

describe('cancel', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('drops a pending call rather than sending it', () => {
    const sent: number[] = []
    const d = debounce((n: number) => sent.push(n), 50)
    d(1)
    d.cancel()
    vi.advanceTimersByTime(100)
    expect(sent).toEqual([])
  })

  // Undo and delete both need this: each replaces the document outright, and a
  // pending write from before would land on top a moment later and put back
  // exactly what the reader just removed.
  it('leaves later calls working', () => {
    const sent: number[] = []
    const d = debounce((n: number) => sent.push(n), 50)
    d(1)
    d.cancel()
    d(2)
    vi.advanceTimersByTime(100)
    expect(sent).toEqual([2])
  })

  it('is harmless with nothing pending', () => {
    const d = debounce(() => undefined, 50)
    expect(() => d.cancel()).not.toThrow()
  })
})

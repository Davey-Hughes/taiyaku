import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { CLICK_WAIT_MS, takeTapClick } from '../src/tapClick'

/**
 * `takeTapClick` with synthetic events: which click is taken and when the
 * wait for it ends. That a real tap's click is the one taken, and lands where
 * it would without this, is `bookTaps.browser.test.tsx`'s.
 */

let button: HTMLButtonElement
let heard: number

beforeEach(() => {
  vi.useFakeTimers()
  button = document.createElement('button')
  document.body.append(button)
  heard = 0
  button.addEventListener('click', () => heard++)
})

afterEach(() => {
  // Any wait a case left open, ended, so it cannot take the next case's click.
  vi.advanceTimersByTime(CLICK_WAIT_MS)
  vi.useRealTimers()
  button.remove()
})

/** A click on the button as a pointer's carries (`detail` 1) or a key's (0); whether it went unprevented. */
function click(detail = 1): boolean {
  return button.dispatchEvent(
    new MouseEvent('click', { bubbles: true, cancelable: true, detail }),
  )
}

const press = () => {
  button.dispatchEvent(new Event('pointerdown', { bubbles: true }))
}

describe('takeTapClick', () => {
  it('takes the next click after a touch tap: stopped before the page and its default prevented', () => {
    takeTapClick({ pointerType: 'touch' })
    expect(click()).toBe(false)
    expect(heard).toBe(0)
  })

  it('takes a pen tap’s click too', () => {
    takeTapClick({ pointerType: 'pen' })
    expect(click()).toBe(false)
    expect(heard).toBe(0)
  })

  it('leaves a mouse’s click alone', () => {
    takeTapClick({ pointerType: 'mouse' })
    expect(click()).toBe(true)
    expect(heard).toBe(1)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('takes one click only, and leaves no wait behind it', () => {
    takeTapClick({ pointerType: 'touch' })
    expect(click()).toBe(false)
    expect(vi.getTimerCount()).toBe(0)
    expect(click()).toBe(true)
    expect(heard).toBe(1)
  })

  it('leaves a keyboard’s click alone, and still takes the tap’s after it', () => {
    takeTapClick({ pointerType: 'touch' })
    expect(click(0)).toBe(true)
    expect(heard).toBe(1)
    expect(click()).toBe(false)
    expect(heard).toBe(1)
  })

  it('ends the wait at the next press, so that press’s click is not taken', () => {
    takeTapClick({ pointerType: 'touch' })
    press()
    expect(vi.getTimerCount()).toBe(0)
    expect(click()).toBe(true)
    expect(heard).toBe(1)
  })

  it('waits for the click until its time is out, and no longer', () => {
    takeTapClick({ pointerType: 'touch' })
    vi.advanceTimersByTime(CLICK_WAIT_MS - 1)
    expect(click()).toBe(false)

    takeTapClick({ pointerType: 'touch' })
    vi.advanceTimersByTime(CLICK_WAIT_MS)
    expect(click()).toBe(true)
    expect(heard).toBe(1)
  })

  it.each([
    ['its click taken', () => click()],
    ['the next press', press],
    ['its time out', () => vi.advanceTimersByTime(CLICK_WAIT_MS)],
  ])('leaves no listener on the window once the wait ends by %s', (_, end) => {
    // Spies that still add and remove, so the wait ends as it would.
    const added = vi.spyOn(window, 'addEventListener')
    const removed = vi.spyOn(window, 'removeEventListener')
    const calls = (spy: typeof added | typeof removed) =>
      spy.mock.calls.map(([type, listener, capture]) => [
        type,
        listener,
        capture,
      ])
    try {
      takeTapClick({ pointerType: 'touch' })
      end()
      expect(calls(added)).toHaveLength(2)
      for (const call of calls(added)) {
        expect(calls(removed)).toContainEqual(call)
      }
    } finally {
      added.mockRestore()
      removed.mockRestore()
    }
  })

  it('waits again for the next tap acted on, its press having ended the last wait', () => {
    takeTapClick({ pointerType: 'touch' })
    press()
    takeTapClick({ pointerType: 'touch' })
    expect(click()).toBe(false)
    expect(heard).toBe(0)
    expect(click()).toBe(true)
    expect(heard).toBe(1)
  })
})

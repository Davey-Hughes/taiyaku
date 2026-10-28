import { act } from 'react'
import { afterEach, vi } from 'vitest'
import { WAIT_DRAWN_AFTER_MS } from '../../src/hooks'

export { WAIT_DRAWN_AFTER_MS }

/** Every `watchDrawn` observer not yet stopped. */
const watching = new Set<MutationObserver>()

// Put back after each test, whether or not it finished, so that a failed
// one does not hand the next a clock that never moves, nor an observer
// that reads the next test's document: its `read` can throw against a host
// that is gone, and the throw would be blamed on that next test.
afterEach(() => {
  vi.useRealTimers()
  for (const observer of watching) observer.disconnect()
  watching.clear()
})

/**
 * Hand the clock of `useDrawnWait`'s timer to the test, so that what is
 * drawn 1 ms before `WAIT_DRAWN_AFTER_MS` and at it is read with no race
 * against the time the test itself takes. Only the timeouts are faked: the
 * answers are promises the test settles.
 */
export function holdTheClock() {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
}

/** Move the held clock on by `ms`, and let React draw what that set. */
export function pass(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms)
  })
}

/**
 * Every state `read` returns, from now until `stop`, or until the test ends
 * if it ends first. It is read on every change to the document, and on
 * `sample`, so a state drawn and taken back between two steps of a test is
 * seen too.
 */
export function watchDrawn(read: () => string) {
  const seen = new Set<string>()
  const sample = () => {
    seen.add(read())
  }
  const observer = new MutationObserver(sample)
  observer.observe(document.body, {
    attributes: true,
    childList: true,
    characterData: true,
    subtree: true,
  })
  watching.add(observer)
  sample()
  return {
    seen,
    sample,
    stop: () => {
      sample()
      observer.disconnect()
      watching.delete(observer)
    },
  }
}

/**
 * How `host` draws the paragraph that says `words`: not at all, laid out
 * unseen until its wait is drawn (`.wait-unseen`), or drawn.
 */
export function sentenceDrawn(
  host: Element,
  words: string,
): 'none' | 'unseen' | 'drawn' {
  const el = [...host.querySelectorAll('p')].find(
    (p) => p.textContent === words,
  )
  if (el === undefined) return 'none'
  return el.classList.contains('wait-unseen') ? 'unseen' : 'drawn'
}

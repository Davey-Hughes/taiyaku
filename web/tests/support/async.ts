import { act } from 'react'

/**
 * A promise with its settlement handed back, for holding a request open.
 *
 * Two reads can be in flight at once and the server need not answer them in
 * order, so the code guards against an answer that is no longer wanted
 * (`if (cancelled) return`, `if (seq !== latest.current) return`). A mock
 * that resolves at once answers in call order, so the guard is never true
 * and its branch never taken: holding the *first* call open and settling it
 * last is the only way to reach it.
 */
export function deferred<T>(): {
  promise: Promise<T>
  resolve: (value: T) => void
  reject: (why: unknown) => void
} {
  let resolve!: (value: T) => void
  let reject!: (why: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/**
 * Settle something and let React see the result.
 *
 * Two `await Promise.resolve()` rather than one, matching the mount helpers
 * already written in these suites: a `.then` that itself awaits needs a
 * second turn of the microtask queue before its `setState` has run.
 */
export async function flush(work?: () => void): Promise<void> {
  await act(async () => {
    work?.()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/**
 * Let the reads sent so far land and be drawn, with no time passing: a
 * Library view read waits for the progress saves sent before it, and the
 * store applies an answer some microtask turns after its client call
 * settles, which is more than one `flush` lets run.
 */
export async function land(): Promise<void> {
  await flush()
  await flush()
  await flush()
}

/**
 * Lets the clock run, inside `act`, until `drawn` holds, so that a test waits
 * on what it needs rather than on a guess at how long that takes. In slices,
 * each its own `act`, since `act` draws what an update set only as it ends.
 * Throws, naming `what`, once `within` ms have passed without it.
 */
export async function settleUntil(
  what: string,
  drawn: () => boolean,
  within: number,
): Promise<void> {
  const until = performance.now() + within
  while (!drawn()) {
    if (performance.now() > until) {
      throw new Error(`${what}: not drawn within ${within} ms`)
    }
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20))
    })
  }
}

import { act } from 'react'

/**
 * A promise with its settlement handed back, for holding a request open.
 *
 * **Why these screens need one.** Every list screen here guards its `.then`
 * with `if (cancelled) return` or `if (seq !== latest.current) return`,
 * because two reads are routinely in flight and the server need not answer
 * them in order. A mock that resolves immediately can never stage the case
 * those guards exist for: the answers arrive in call order, the guard is
 * never true, and the branch is never taken. Holding the *first* call open
 * and settling it last is the only way to reach it.
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

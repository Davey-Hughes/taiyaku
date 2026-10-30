/** A debounced function, with a way to force the pending call through early. */
export interface Debounced<Args extends unknown[]> {
  (...args: Args): void
  /**
   * Send the pending call now, rather than waiting out the rest of the
   * delay. A no-op if nothing is pending — either because no call has been
   * made since the last one fired (naturally or via a previous `flush`), or
   * because the wrapped function has already run for the latest arguments.
   *
   * `hooks.ts`'s `useFlushOnHide` calls it on `visibilitychange` and
   * `pagehide`, so a pending write survives the page going away, and
   * `App.tsx` calls it when the note editor closes. The ordinary,
   * timer-fires-naturally case must not be double-sent by a `flush` that
   * runs afterwards (e.g. `pagehide` following a `visibilitychange` that
   * already flushed), which is exactly what clearing the pending state below
   * before calling `fn` guarantees.
   */
  flush(): void
  /**
   * Drop the pending call without sending it.
   *
   * The opposite of `flush`, and it exists for the two acts that replace the
   * document outright rather than changing it: undo, and delete. Both write
   * immediately, and a debounced write scheduled *before* them would land a
   * moment later carrying the state they had just replaced — putting back
   * exactly the note the reader removed, or the edit they stepped away from.
   */
  cancel(): void
}

/**
 * Wrap a function so a burst of calls collapses into the one call whose
 * arguments were still standing after `delayMs` of quiet.
 *
 * This is what `App.tsx` needs around `putProgress`: arrowing through twenty
 * pages fires `navigate` twenty times, and writing on every one would be
 * twenty `PUT`s for a position the reader will keep moving away from before
 * any of them matter. Each call here restarts the wait rather than queuing
 * behind it, so only the last one — the position actually settled on — ever
 * reaches the wrapped function.
 *
 * Deliberately fire-and-forget: the wrapped function's return value, if any,
 * is discarded. A caller that needs to know whether the eventual call
 * succeeded (`putProgress` returns a `Promise` that can reject) has to handle
 * that inside the function it passes in, not from what this returns.
 */
export function debounce<Args extends unknown[]>(
  fn: (...args: Args) => void,
  delayMs: number,
): Debounced<Args> {
  let timer: ReturnType<typeof setTimeout> | undefined
  let pending: Args | undefined
  const call = ((...args: Args) => {
    if (timer !== undefined) clearTimeout(timer)
    pending = args
    timer = setTimeout(() => {
      timer = undefined
      pending = undefined
      fn(...args)
    }, delayMs)
  }) as Debounced<Args>
  call.flush = () => {
    if (timer === undefined) return
    clearTimeout(timer)
    timer = undefined
    const args = pending
    pending = undefined
    if (args !== undefined) fn(...args)
  }
  call.cancel = () => {
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
    pending = undefined
  }
  return call
}

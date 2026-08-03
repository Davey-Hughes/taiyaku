/**
 * Wrap a nullary async function so it runs exactly once no matter how many
 * times the returned function is called — even concurrently, before the
 * first call has settled. Every caller, however many are in flight, awaits
 * the one real call's result.
 *
 * This is what `resolver.ts` needs around wasm-bindgen's `init()`: its own
 * re-entry guard (`if (wasm !== undefined) return wasm`) is synchronous
 * only, so two calls that are both already in-flight both pass it. The
 * guard here is different in exactly the way that matters — the check and
 * the assignment below happen with no `await` between them, so whichever
 * caller's synchronous prologue runs first is the one that sets `promise`,
 * and every other caller — no matter how it got here — sees it already set
 * and awaits that instead of invoking `fn` again. A guard that read a flag
 * `fn` itself sets only after finishing would not have this property: two
 * concurrent callers could both pass the check before either call finishes.
 *
 * A rejection is not memoised. As soon as `fn`'s promise rejects, the memo
 * is cleared, so the *next* call to the returned function retries `fn` from
 * scratch rather than replaying the same failure forever — a transient
 * error (a network blip fetching the `.wasm` asset, say) stays transient
 * instead of permanently poisoning every later, unrelated caller. Callers
 * that are already awaiting the failed call still see that rejection: the
 * clearing happens as a reaction attached to the one shared promise, after
 * it has already been handed out to every concurrent caller, so it cannot
 * retroactively change what they're awaiting — it only affects calls made
 * after the memo has been cleared.
 */
export function once<T>(fn: () => Promise<T>): () => Promise<T> {
  let promise: Promise<T> | undefined
  return () => {
    if (!promise) {
      promise = fn().catch((err: unknown) => {
        promise = undefined
        throw err
      })
    }
    return promise
  }
}

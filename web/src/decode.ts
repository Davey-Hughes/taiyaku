/**
 * The subset of `HTMLImageElement` this module needs to decode a page
 * off-screen before presenting it: a writable `src` and `decode()`, the
 * promise-returning method that resolves once the browser can paint the
 * image without a visible stall. A real `<img>` node already satisfies this
 * shape, so `PageView.tsx` hands one in directly; a test hands in a plain
 * object instead, with no DOM and no `jsdom` — see `decode.test.ts`.
 */
export interface Decodable {
  src: string
  decode(): Promise<void>
}

/** Builds a fresh `Decodable` for one decode attempt — `PageView.tsx`'s default is `() => new Image()`; `decode.ts` itself never calls this, or `new Image()`, so it stays DOM-free and importable by Vitest with nothing stubbed. */
export type ImageFactory = () => Decodable

/**
 * Point `image` at `src` and wait for it to become paintable off-screen.
 *
 * A rejection from `decode()` — an undecodable source (a 404 page, a
 * corrupt file), or `src` reassigned again before this decode settled, which
 * browsers may reject for — is swallowed rather than propagated. Holding the
 * previous page on screen forever because one source can never decode would
 * defeat the entire point of this module; the browser's own broken-image
 * glyph, reached by presenting `src` anyway, is a more honest failure than a
 * frozen page that never advances.
 */
export async function decodeTo(image: Decodable, src: string): Promise<void> {
  image.src = src
  try {
    await image.decode()
  } catch {
    // See doc above: presented anyway, on the same footing as a clean decode.
  }
}

/** A cap on how many `decodeTo` calls run at once — see `createDecodeQueue`. */
export interface DecodeQueue {
  run<T>(job: () => Promise<T>): Promise<T>
}

/**
 * Queue decode jobs so at most `max` run at once.
 *
 * A fast flick through twenty slots must not fire twenty simultaneous
 * decodes — `prefetchModule`'s own `maxConcurrent` setting (`prefetch.ts`)
 * is what a caller passes here. `App.tsx` builds two of these rather than
 * one: a small queue for `PageView`'s own swap decodes and a separate one
 * for the prefetch window, so a page turn's decode can never be stuck behind
 * prefetch jobs queued for slots the reader has already moved past — see
 * `App.tsx`'s `swapQueue`/`prefetchQueue` doc for why sharing one queue
 * between the two was the actual bug a review found here. Nothing in this
 * function assumes how many callers a given instance has; each `max` is a
 * cap on that one instance's own concurrency, nothing more.
 *
 * FIFO beyond the cap, deliberately: `prefetch.ts`'s `window` already orders
 * its slots nearest-first, and a queue that ran jobs in any other order
 * (newest-first, say) would undo that ordering the moment the window's size
 * exceeds `max` — the near, useful pages must be the ones that get a
 * concurrency slot first.
 *
 * `max` is clamped to at least 1. A queue built with 0, a negative number,
 * or `NaN` would compare `active >= max` as either always-true (0 or
 * negative: nothing ever starts, forever, with no error and no diagnostic —
 * every caller's `run` promise simply hangs) or always-false (`NaN`: the cap
 * silently stops capping anything). Both are reachable in practice, not just
 * in theory: the documented way to tune this module today is hand-editing
 * `taiyaku.prefs.prefetch.maxConcurrent` in storage (see `prefetch.ts`), and
 * `prefs.ts`'s `loadPrefs` merges whatever shape is stored without
 * validating the values inside it.
 */
export function createDecodeQueue(max: number): DecodeQueue {
  const cap = Number.isFinite(max) ? Math.max(1, Math.floor(max)) : 1
  let active = 0
  const pending: (() => void)[] = []

  function schedule(): void {
    if (active >= cap || pending.length === 0) return
    active += 1
    const run = pending.shift()
    run?.()
  }

  return {
    run<T>(job: () => Promise<T>): Promise<T> {
      return new Promise<T>((resolve, reject) => {
        pending.push(() => {
          // `job`'s type promises a `Promise<T>`, but nothing enforces that
          // at runtime, and a job that throws synchronously instead of
          // returning a rejected promise would skip straight past
          // `.then(resolve, reject)` — leaving this slot's `active`
          // increment (above, in `schedule`) with no matching decrement,
          // forever. The `try`/`catch` turns that throw into an ordinary
          // rejected promise instead, so `.finally` below still runs and
          // still frees the slot — without deferring the *ordinary* case to
          // an extra microtask the way routing every call through
          // `Promise.resolve().then(job)` would have. Unreachable today —
          // the one caller (`presentWhenDecoded`, below) always passes an
          // `async` function, which never throws synchronously by
          // construction — but a two-line guard against a caller that stops
          // being true.
          let result: Promise<T>
          try {
            result = job()
          } catch (err) {
            result = Promise.reject(err as unknown)
          }
          result.then(resolve, reject).finally(() => {
            active -= 1
            schedule()
          })
        })
        schedule()
      })
    },
  }
}

/**
 * Decode `src` off-screen — a fresh image from `makeImage`, run through
 * `queue` — and, once it settles, hand it to `present` unless `stale()` says
 * a later request has already superseded this one.
 *
 * This is the mechanism `PageView.tsx` swaps its one visible `<img>`
 * through: nothing is ever presented until decode has resolved, so a `src`
 * prop changing on an already-mounted `PageView` instance never shows a
 * blank or half-painted frame — and, because `stale` is evaluated only once
 * decode settles, not when this was called, a fast flick's intermediate
 * targets never flash onto screen after the reader has already moved past
 * them. `stale` is a callback rather than a value captured at call time for
 * exactly that reason: the answer can only be known at the moment decode
 * finishes, against whatever request is current *then*.
 *
 * That guarantee is scoped to one mounted instance, deliberately: this
 * function has no say over whether a caller keeps the *same* `PageView`
 * mounted across a change, only over what that instance shows once its
 * `src` prop changes. A caller that remounts a fresh instance instead gets
 * none of the above — `useState(src)`'s initial value is the new, undecoded
 * source, with no decode gate in the way of the first paint. `App.tsx`'s
 * two-page layout used to do exactly that on any turn that changed a
 * group's shape (a joined spread interrupting or resuming pairing); see
 * `pairing.ts`'s `cellsFor` for how it now keeps the same instances mounted
 * across that change instead.
 *
 * `makeImage` is called fresh on every invocation rather than once and
 * reused, so two overlapping requests (the ordinary case mid-flick, before
 * the cap in `queue` even matters) each decode into their own image instead
 * of one clobbering the other's `src` while its decode is still pending.
 */
export function presentWhenDecoded(
  makeImage: ImageFactory,
  queue: DecodeQueue,
  src: string,
  stale: () => boolean,
  present: (src: string) => void,
): Promise<void> {
  const image = makeImage()
  return queue
    .run(() => decodeTo(image, src))
    .then(() => {
      if (!stale()) present(src)
    })
}

/** A `DecodeQueue` that runs every job immediately, uncapped — `PageView.tsx`'s fallback when no shared queue is handed in, so the component works standalone without forcing every caller to construct one. */
export const unboundedDecodeQueue: DecodeQueue = {
  run<T>(job: () => Promise<T>): Promise<T> {
    return job()
  },
}

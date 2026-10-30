import { noCohort, type Cohort } from './cohort'

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
  /**
   * How many jobs are waiting for a slot, not counting those running.
   *
   * One caller, one question: `presentWhenDecoded` asks whether newer work is
   * already queued behind a superseded job. Zero means this job is still the
   * freshest thing asked for and decoding it is worthwhile; above zero means it
   * would be obsolete before it appeared.
   */
  pending(): number
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
 * `App.tsx`'s `swapQueue`/`prefetchQueue` doc. Nothing in this function
 * assumes how many callers a given instance has; each `max` is a cap on that
 * one instance's own concurrency, nothing more.
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
 * in theory: the settings panel accepts any finite number for `maxConcurrent`,
 * and `prefs.ts`'s `loadPrefs` merges whatever shape is stored without
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
    pending: () => pending.length,
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
          // `Promise.resolve().then(job)` would have. `presentWhenDecoded`
          // (below) passes an `async` function, which never throws
          // synchronously by construction; `App.tsx`'s prefetch effect passes
          // a job that calls `stale()` before it returns a promise — a
          // two-line guard against that call, or a later caller, throwing.
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
 * `queue` — and hand it to `present` once decoded; a job `stale()` calls
 * superseded, with newer work queued behind it, is skipped before it decodes.
 *
 * This is the mechanism `PageView.tsx` swaps its one visible `<img>`
 * through: nothing is ever presented until decode has resolved, so a `src`
 * prop changing on an already-mounted `PageView` instance never shows a
 * blank or half-painted frame. `stale` is a callback rather than a value
 * captured at call time because it is asked when the queue reaches the job,
 * against whatever request is current *then*.
 *
 * That guarantee is scoped to one mounted instance, deliberately: this
 * function has no say over whether a caller keeps the *same* `PageView`
 * mounted across a change, only over what that instance shows once its
 * `src` prop changes. A caller that remounts a fresh instance instead gets
 * none of the above — `useState(src)`'s initial value is the new, undecoded
 * source, with no decode gate in the way of the first paint. `App.tsx`'s
 * two-page layout keeps the same instances mounted across a turn that changes
 * a group's shape (a joined spread interrupting or resuming pairing); see
 * `pairing.ts`'s `cellsFor`.
 *
 * `makeImage` is called fresh on every invocation rather than once and
 * reused, so two overlapping requests (the ordinary case mid-flick, before
 * the cap in `queue` even matters) each decode into their own image instead
 * of one clobbering the other's `src` while its decode is still pending.
 *
 * **`stale` is asked once, inside the job, which is what makes a fast flick
 * survivable.** Asked only at the end — "has the reader moved on while this
 * decoded?" — it would decide whether to *show* a finished decode and do
 * nothing about starting one nobody wants. Under a flick that is most of the
 * work: each such request would fetch and decode a page that is discarded on
 * arrival, while the page the reader is actually looking at waits its turn
 * behind them.
 *
 * Asking inside the job rather than before `queue.run` is the whole point: the
 * question has to be answered when the queue *reaches* this job, not when it
 * was enqueued, because at enqueue time it is never stale. A job that has gone
 * stale by then resolves immediately and never touches `makeImage`, so it costs
 * neither the decode nor the fetch — `decodeTo` is what assigns `src`, and no
 * assignment means no request.
 *
 * Skipped jobs still resolve rather than hanging, so `cohort` releases and the
 * `.then` below runs — where `decoded` is false and nothing is presented.
 *
 * **`cohort` is what makes two of these present together**: the gate above is
 * per call, so without it `two` mode's pair would each present as its own
 * decode settled. Running the queue *inside* the cohort rather than the other
 * way round is what keeps the two jobs doing different things — the queue
 * still caps how many decodes are in flight, the cohort only holds back the
 * presentation of the ones that have finished. See `cohort.ts` for why
 * membership closes on a tick rather than a count, and its entry in
 * `docs/superpowers/notes/2026-10-29-code-history-web-src.md` for the
 * measurement that put it here. Defaults to `noCohort`, so a caller that has
 * not been threaded one presents each image as its own decode settles.
 */
export function presentWhenDecoded(
  makeImage: ImageFactory,
  queue: DecodeQueue,
  src: string,
  stale: () => boolean,
  present: (src: string) => void,
  cohort: Cohort = noCohort,
): Promise<void> {
  return cohort
    .run((member) =>
      queue.run(async () => {
        // Asked at the moment the queue reaches this job, which under a flick
        // may be long after it was enqueued. See the doc above for why the
        // question cannot be answered earlier.
        //
        // **Superseded is not on its own a reason to skip** — whether newer
        // work is already waiting is. A superseded job with an empty queue
        // behind it is still the freshest frame that exists, and decoding it is
        // what lets a flick show pages going by instead of holding one spread
        // and jumping. With newer work queued it would be obsolete before it
        // appeared, and the decode is pure cost.
        //
        // Self-tuning: at reading size a page decodes quickly, the queue drains
        // about as fast as a reader can press, and most frames get through.
        // Zoomed to full resolution a decode is slow, the queue stays backed
        // up, and the same test skips nearly everything.
        // Decided once per cohort, not once per cell. Two halves of one turn
        // asking this question moments apart could otherwise get different
        // answers — one skipping while the other decodes — and the pair would
        // show two generations at once. `decideOnce` makes them skip together
        // or decode together, which is what turns "they move together in
        // practice" into "they cannot do otherwise".
        if (member.decideOnce(() => stale() && queue.pending() > 0))
          return false
        await decodeTo(makeImage(), src)
        return true
      }),
    )
    .then((decoded) => {
      // Only a job that actually decoded has anything to show; a skipped one
      // would hand `present` a source no image was ever pointed at.
      //
      // **`stale` deliberately does not gate this**, which is what makes a
      // flick visible. Ordering is `present`'s responsibility: `PageView`
      // hands in a callback that ignores anything older than what it is
      // already drawing, so a frame the reader has moved past can be shown
      // *on the way* and can never paint over a newer one. That guard belongs
      // there rather than here, because ordering needs a sequence and this
      // function only ever sees opaque `src` strings.
      if (decoded) present(src)
    })
}

/** A `DecodeQueue` that runs every job immediately, uncapped — `PageView.tsx`'s fallback when no shared queue is handed in, so the component works standalone without forcing every caller to construct one. */
export const unboundedDecodeQueue: DecodeQueue = {
  /** Nothing ever waits here, so no job is ever superseded with newer work behind it: every frame is worth decoding. */
  pending: () => 0,
  run<T>(job: () => Promise<T>): Promise<T> {
    return job()
  },
}

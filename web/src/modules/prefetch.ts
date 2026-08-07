import type { ReaderModule } from '../module'

/**
 * How far ahead and behind the current slot to fetch, how many editions'
 * decodes may run at once, and whether any of this happens at all.
 *
 * `enabled` is the whole module's own on/off switch — not `ahead`/`behind`
 * both at zero, though `window` below treats that the same way.
 */
export interface PrefetchPrefs {
  enabled: boolean
  ahead: number
  behind: number
  maxConcurrent: number
}

export const prefetchModule: ReaderModule<PrefetchPrefs> = {
  id: 'prefetch',
  label: 'Prefetch',
  // Asymmetric toward the forward direction deliberately: `ArrowRight`
  // (`keys.ts`'s `{ kind: 'step', by: 1 }`) is the common case a reader
  // presses far more often than `ArrowLeft`, and `window`'s slot space is
  // always physical page order regardless of `rtl`/`ltr` (see its own doc) —
  // an edition read right-to-left still steps forward by increasing slot
  // index, so "ahead" here means the same thing for every edition. `ahead:
  // 2` covers a fast flick's next keypress with one slot of headroom;
  // `behind: 1` only needs to cover a single reversal, since `window`'s own
  // "both zero" test is what actually turns the module off, not either
  // depth alone.
  defaults: { enabled: true, ahead: 2, behind: 1, maxConcurrent: 3 },
}

/**
 * The slots to fetch around `slot`, in `[0, total)`: up to `ahead` slots at
 * higher indices, up to `behind` at lower ones, nearest-first — the current
 * slot itself is never included, since it is already on screen and has
 * nothing to prefetch.
 *
 * Nearest-first by interleaving the two directions one distance at a time
 * (behind before ahead at each tier — not matching the module's own
 * forward-asymmetric default above, which is about *depth*, `ahead` being
 * the larger of the two, not per-tier order; the reason to reach behind
 * first at each tier is that a reversal is the single most likely
 * correction right after a forward press, so the page just left is worth
 * fetching before the next page further ahead) rather than emitting every
 * `behind` slot and then every `ahead` slot: a flick that reverses or stops
 * after one press has already fetched the slot immediately adjacent on both
 * sides before either the second-behind or the second-ahead slot ever gets
 * a turn. This is also what makes the order meaningful once `decode.ts`'s
 * `createDecodeQueue` caps concurrency below the total window size — the
 * near, useful pages start decoding first.
 *
 * `ahead`/`behind` name a direction in *slot* space, not reading direction:
 * `keys.ts`'s `ArrowRight` is always `{ kind: 'step', by: 1 }` regardless of
 * `rtl`/`ltr` (direction only changes which side of a two-page spread a slot
 * is drawn on, never which way stepping moves through slot indices), so
 * "ahead" always means higher slot indices and this function takes no
 * direction parameter at all.
 *
 * Clamped at both ends of `[0, total)` — a volume's first or last few slots
 * have fewer than `ahead`/`behind` real neighbours on one side, and this
 * returns only the ones that exist rather than a slot index nothing can
 * resolve.
 */
export function window(
  slot: number,
  total: number,
  ahead: number,
  behind: number,
): number[] {
  const result: number[] = []
  const depth = Math.max(ahead, behind)
  for (let distance = 1; distance <= depth; distance += 1) {
    const before = slot - distance
    if (distance <= behind && before >= 0) result.push(before)
    const after = slot + distance
    if (distance <= ahead && after < total) result.push(after)
  }
  return result
}

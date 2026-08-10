import type { ReaderModule } from '../module'
import type { Mode } from '../pairing'
import type { Display } from '../reading'
import type { Position } from '../resolver'

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
  // **One turn each way, and reaching further makes the reader slower.** These
  // count *groups* now (`groupWindow` below), so `ahead: 1` is the next thing
  // that will be drawn rather than the next slot — which in `two` mode is what
  // `ahead: 2` used to half-cover, reaching the next pair's first slot and
  // stopping short of its second.
  //
  // The depths came down when the unit changed, and that is a measurement
  // rather than arithmetic. A decoded `en-mono` page is 2147×3056×4 ≈ 26 MB,
  // and a joined spread twice that, so a wide window churns the browser's own
  // decoded-bitmap cache and evicts the pages the reader is about to arrive
  // at — prefetch competing with itself. Measured in the browser against
  // `gotoubun-v01` `en-mono`, keypress to both halves on screen, 20 turns per
  // setting:
  //
  // | window | median | p90 | worst | turns over 100 ms |
  // | ------ | -----: | --: | ----: | ----------------: |
  // | `ahead: 2, behind: 1` — 6 slots | 14 ms | 232 ms | 335 ms | 29% |
  // | `ahead: 1, behind: 0` — 2 slots |  4 ms | 125 ms | 182 ms | 10% |
  // | `ahead: 1, behind: 1` — 4 slots |  4 ms |  70 ms | 216 ms |  5–20% |
  //
  // The same effect from the other side: raising `maxConcurrent` from 3 to 6,
  // so more of that window decodes at once, made it *worse* (5 slow turns in 16
  // against 3 in 14) rather than better. More decode in flight is more
  // eviction, not more readiness.
  //
  // `behind: 1` rather than `0`, though the two measure alike forward: a
  // reversal is the single most likely correction after a forward press, and
  // the run above that reversed every fourth turn is the one `behind` exists
  // for. `window`'s own "both zero" test is what actually turns the module off,
  // not either depth alone.
  //
  // Direction does not enter into it. `keys.ts` reports `ArrowRight` as
  // `{ kind: 'step', by: 1 }` whatever the work's reading direction, and slot
  // space is always physical page order, so "ahead" means higher indices for
  // every edition.
  defaults: { enabled: true, ahead: 1, behind: 1, maxConcurrent: 3 },
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

/** The shape `groupWindow` needs from `pairing.ts`'s `Group` — its slots, and nothing else. */
export interface SlotGroup {
  slots: number[]
}

/**
 * The same window, counted in **groups** rather than slots: every slot of the
 * `ahead` groups after the reader's and the `behind` groups before it, nearest
 * group first, with the group they are standing in left out.
 *
 * **`window` above counts the wrong unit in `two` mode, and that is a defect a
 * reader felt rather than a refinement.** An arrow steps by what is *drawn* —
 * `pairing.ts`'s `stepToGroup` — and in `two` mode a group is two slots, so a
 * slot-space window centred on the group's first slot reaches the next group's
 * first half and stops one short of its second. Measured on `gotoubun-v01`
 * `en-mono` with the shipped defaults (`ahead: 2, behind: 1`), watching
 * `PerformanceObserver` resource entries: **every forward turn requested exactly
 * one page only after the keypress**, and it was the second half of the group
 * being turned to, every time — 16,17 then 18,19 then 20,21 then 22,23, cold on
 * 17, 19, 21, 23. Median time from keypress to the pair on screen was 110 ms
 * against 9–14 ms for a turn whose pages were both warm.
 *
 * That one cold half is also where the *first* complaint about this reader came
 * from. Before `cohort.ts` the warm half painted immediately and the cold one
 * arrived ~90 ms later, which is precisely the tear that was measured and fixed;
 * gating the pair together turned the same missing fetch into a wait instead. One
 * defect, two symptoms, and prefetching the whole group is the end of both.
 *
 * `single` and `scroll` put one slot per group, so this is identical to `window`
 * there and the defaults keep their old meaning. In `two` mode `ahead: 2` now
 * means two *turns* ahead rather than one turn and one orphaned half, which is
 * what the module's own doc above always claimed it meant.
 *
 * Nearest-first by the same interleaving `window` uses and for the same reason —
 * behind before ahead at each tier, since a reversal is the likeliest correction
 * right after a forward press — except that the tiers are groups, so each one
 * contributes all of its slots at once. A group is drawn as a unit and is
 * useless half-fetched, which is the whole point.
 *
 * The reader's own group is excluded entirely, not just the slot they are
 * standing on: in `two` mode both of its slots are already on screen.
 *
 * A `slot` no group holds returns nothing. That is unreachable as the reader is
 * wired — `modules/credits.ts`'s `skippedApartFrom` guarantees every position a
 * reader can hold is in some group, the same invariant `stepToGroup` leans on —
 * and answering with an empty window rather than guessing a group keeps a
 * prefetch miss from becoming a fetch of the wrong pages.
 */
export function groupWindow(
  groups: SlotGroup[],
  slot: number,
  ahead: number,
  behind: number,
): number[] {
  const here = groups.findIndex((group) => group.slots.includes(slot))
  if (here === -1) return []

  const result: number[] = []
  const depth = Math.max(ahead, behind)
  for (let distance = 1; distance <= depth; distance += 1) {
    if (distance <= behind) {
      const before = groups[here - distance]
      if (before) result.push(...before.slots)
    }
    if (distance <= ahead) {
      const after = groups[here + distance]
      if (after) result.push(...after.slots)
    }
  }
  // A joined spread's two slots share one URL, and `cellsFor` mirrors a solo
  // group's single slot into both cells, so the same slot can legitimately
  // arrive twice across neighbouring groups. Fetching it twice is harmless but
  // spends a concurrency slot that a further-out group could have used.
  return [...new Set(result)]
}

/** One page to warm: the slot, and the page index its URL is built from. */
export interface PrefetchTarget {
  slot: number
  page: number
}

/**
 * The pages either side of `at` worth warming, in `groupWindow`'s
 * nearest-group-first order.
 *
 * **Empty in three states, and all three are answered here** rather than by
 * three guards in a caller:
 *
 * - **Scroll mode.** Every group is already mounted there (see `PageView`'s
 *   `buffered` doc), so "the slots around the current one" is not a meaningful
 *   subset to warm ahead of the rest — the rest are either already requested or
 *   deliberately deferred to `loading="lazy"`, and this would only fight that.
 * - **Disabled**, which is this module's own on/off switch.
 * - **A gap**, where there is nothing paired to prefetch around.
 *
 * `grouping` is the grouping already on screen rather than one derived here: an
 * effect reading a stale one would warm the pages of a layout the reader has
 * left. `undefined` — no grouping assigned yet — is empty for the same reason a
 * gap is, and needs no fallback to slot-space `window`, which would warm the
 * wrong unit in `two` mode.
 *
 * `slotAt` rather than a `Resolver`, so a test hands in a function and this
 * module keeps taking no dependency on `resolver.ts` at all. A slot the
 * resolver cannot place is skipped rather than guessed at: a prefetch miss is
 * cheaper than a fetch of the wrong page.
 */
export function prefetchTargets(
  grouping: SlotGroup[] | undefined,
  at: Display,
  prefs: PrefetchPrefs,
  mode: Mode,
  slotAt: (edition: string, index: number) => Position | undefined,
): PrefetchTarget[] {
  if (mode === 'scroll' || !prefs.enabled || !grouping) return []
  if (at.kind !== 'at') return []
  const targets: PrefetchTarget[] = []
  for (const slot of groupWindow(
    grouping,
    at.slot,
    prefs.ahead,
    prefs.behind,
  )) {
    const position = slotAt(at.edition, slot)
    if (!position) continue
    targets.push({ slot, page: position.page })
  }
  return targets
}

/**
 * Whether a window built for `queuedFor` still matters, asked when the queue
 * reaches each job rather than when the job was made.
 *
 * Under a flick the window is rebuilt once per group crossed, so without this
 * the queue works through every intermediate position's neighbours — fetching
 * and decoding pages for somewhere the reader left several turns ago, while the
 * page they are actually on waits. The same defect `presentWhenDecoded` closes
 * for the swap path, on the other queue.
 *
 * Strict equality on the slot, not proximity: a window built for a position the
 * reader has left is worthless whatever the distance, because the position they
 * moved *to* has already queued its own window. Nothing is lost by dropping the
 * old one.
 *
 * `undefined` — no session to ask — is superseded, which is what stops a queue
 * draining into a reader that has gone.
 */
export function superseded(
  queuedFor: { edition: string; slot: number },
  now: Display | undefined,
): boolean {
  if (!now) return true
  return (
    now.kind !== 'at' ||
    now.edition !== queuedFor.edition ||
    now.slot !== queuedFor.slot
  )
}

/**
 * A barrier over decode jobs, so several images become visible in one frame
 * rather than one at a time.
 *
 * `decode.ts`'s gate is per image: a `PageView` never shows a source until that
 * source has decoded, which is what stops a page turn landing on a blank or
 * half-painted frame. It says nothing about *two* images, and `two` mode draws
 * two — so an ordinary pair's halves each presented the moment their own decode
 * settled, leaving one new page beside one old page for as long as the slower
 * of the two took. Measured in the browser against `gotoubun-v01` with a warm
 * cache: a median of 89.7 ms between the two cells' `src` changes at a slow
 * reading pace, 158.4 ms at worst, and **every one of eight turns tore by more
 * than a frame**. Flicking at 120 ms between turns took the median to 213.8 ms
 * and the worst case to 1.28 s.
 *
 * `App.tsx` used to carry a comment arguing the other way — that gating the two
 * together would make the faster half wait on the slower one, "for a mismatch
 * nothing has measured a reader noticing". The measurement above is that
 * measurement, and it was a reader who asked for it. The latency the old
 * comment was protecting is real and is paid: a pair now appears when its
 * slower half is ready. What it buys is that the pair appears *as a pair*, and
 * the wait was always going to happen anyway — the reader was already looking
 * at the slower half's old page for exactly that long.
 *
 * **Membership closes at the end of the current task, not on a count**, because
 * nothing here knows how many images a turn will change. React flushes a
 * commit's effects in one synchronous pass, so both cells' `useEffect` bodies —
 * and so both `run` calls — land before any deferred callback can fire. A turn
 * that changes only one cell's `src` (the other's is unchanged, so `PageView`'s
 * own `src === shown` guard returns before starting a decode) forms a cohort of
 * one and releases as soon as it settles. Nothing waits for a member that never
 * arrives.
 *
 * `defer` is a parameter rather than a hard-coded `queueMicrotask` so a test
 * can close the cohort exactly where it means to; `cohort.test.ts` fires it by
 * hand.
 */
/**
 * A member's handle on the cohort it joined, handed to its job so a decision
 * can be made *once for the whole turn* rather than once per cell.
 */
export interface CohortMember {
  /**
   * Evaluate `decide` on the first member of this cohort to ask, and give every
   * other member the same answer.
   *
   * **This is what keeps a two-page turn's halves in lockstep.** Each cell
   * independently decides whether its decode is still worth doing; two cells
   * asking the same question moments apart can get different answers, and the
   * pair then shows two generations at once — the tear this whole mechanism
   * exists to prevent. Deciding once per cohort means both halves of a turn
   * always skip together or decode together.
   *
   * Memoised on the *member's own* cohort, not on whichever one is currently
   * open: a job is asked at dequeue, which can be long after its cohort closed
   * to newcomers.
   */
  decideOnce(decide: () => boolean): boolean
}

export interface Cohort {
  /**
   * Start `job` now, and resolve once every job that joined this cohort has
   * settled — with `job`'s own value, or its own rejection.
   *
   * Deliberately the same shape as `DecodeQueue.run`, so the two compose by
   * nesting rather than by either knowing about the other: `decode.ts` runs the
   * queue *inside* the cohort, which is what keeps the concurrency cap on the
   * decodes while the barrier governs only when their results are presented.
   */
  run<T>(job: (member: CohortMember) => Promise<T>): Promise<T>
}

/** How a cohort's membership is closed — `queueMicrotask` in the reader, a manual trigger in tests. */
export type Defer = (fn: () => void) => void

/** One open cohort: the jobs that have joined, and the promise that closes it to newcomers. */
interface Group {
  jobs: Promise<unknown>[]
  closed: Promise<void>
  close: () => void
  /** `decideOnce`'s memo: `undefined` until the first member asks. */
  decision?: boolean
}

/**
 * A `Cohort` whose membership is whatever joins before `defer` fires.
 *
 * One instance is shared by every `PageView` — `App.tsx` builds it beside
 * `swapQueue` — rather than one per group. A cohort is defined by *when* its
 * members join, so a shared instance costs nothing when only one image is
 * decoding and needs no plumbing to tell the two cells they belong together.
 */
export function createCohort(defer: Defer = queueMicrotask): Cohort {
  let open: Group | undefined

  return {
    run<T>(job: (member: CohortMember) => Promise<T>): Promise<T> {
      if (open === undefined) {
        let close!: () => void
        const closed = new Promise<void>((resolve) => {
          close = resolve
        })
        const group: Group = { jobs: [], closed, close }
        open = group
        // Closing to newcomers and releasing the members are two different
        // moments: this only stops the *next* `run` joining this group. The
        // release waits on the jobs, below.
        defer(() => {
          open = undefined
          group.close()
        })
      }
      const group = open

      const member: CohortMember = {
        decideOnce(decide) {
          if (group.decision === undefined) group.decision = decide()
          return group.decision
        },
      }

      // The same guard `createDecodeQueue` carries, for the same reason: a job
      // that throws synchronously rather than returning a rejected promise
      // would never reach `group.jobs`, so the cohort would release without
      // ever having accounted for it — and this caller would get a synchronous
      // throw where every other path gives a rejection.
      let result: Promise<T>
      try {
        result = job(member)
      } catch (err) {
        result = Promise.reject(err as unknown)
      }
      group.jobs.push(result)

      return (
        group.closed
          // Read after `closed`, so the array holds every member rather than
          // however many had joined when this call was made.
          .then(() => Promise.allSettled(group.jobs))
          // `allSettled` never rejects, so one undecodable source cannot strand
          // the pages beside it — the same choice `decodeTo` makes when it
          // swallows a rejected `decode()`. This member's own outcome is
          // restored by re-awaiting `result`, so a caller still sees its
          // rejection.
          .then(() => result)
      )
    },
  }
}

/** A `Cohort` with no barrier: every job releases on its own, which is exactly how `decode.ts` behaved before this module existed. The default wherever a cohort is optional, so a caller that has not been threaded one keeps the old behaviour rather than a third. */
export const noCohort: Cohort = {
  run<T>(job: (member: CohortMember) => Promise<T>): Promise<T> {
    // A cohort of one: nothing else shares the decision, so "once" is just
    // "now".
    return job({ decideOnce: (decide) => decide() })
  },
}

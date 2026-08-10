import { describe, expect, it } from 'vitest'
import type { CorrectionRequest, FetchedSidecar } from './api'
import {
  applyLanded,
  correctionView,
  counterpartEdition,
  counterpartPick,
  describePosition,
  displacedBy,
  spanOf,
  submitCorrection,
  type CorrectionOutcome,
  type CorrectionPorts,
} from './correction'
import type { Position, Resolver } from './resolver'

/**
 * A resolver where `raw` slot n pairs with `en` slot n+1, and nothing else.
 *
 * Deliberately asymmetric — `resolve('raw', 'en', {page: 3})` lands on page 4,
 * and `resolve('en', 'raw', {page: 4})` lands on page 3 — rather than an
 * identity mapping. `displacedBy` matters precisely because it resolves in
 * the direction opposite the one named by its own `from`/`to` arguments
 * (`resolver.resolve(pending.to, pending.from, pending.b)`); a fixture that
 * answers the same way regardless of which edition is `from` and which is
 * `to` cannot tell a correct call from one with the arguments swapped.
 */
function offsetResolver(): Resolver {
  const at = (index: number): Position => ({ page: index })
  return {
    editions: () => ['raw', 'en'],
    slotCount: () => 10,
    slotAt: (_edition, index) => (index < 10 ? at(index) : undefined),
    indexOf: (_edition, position) => position.page,
    resolve: (from, to, position) => {
      if (from === to) return { ...position, via: 'identity' }
      const offset =
        from === 'raw' && to === 'en'
          ? 1
          : from === 'en' && to === 'raw'
            ? -1
            : undefined
      if (offset === undefined) return undefined
      const page = position.page + offset
      return page >= 0 && page < 10 ? { page, via: 'direct' } : undefined
    },
    chapterAt: () => undefined,
    credits: () => [],
  }
}

describe('spanOf', () => {
  it('writes a slot the way the sidecar writes one', () => {
    expect(spanOf({ page: 7 })).toBe('7')
    expect(spanOf({ page: 45, half: 'R' })).toBe('45R')
    expect(spanOf({ page: 46, half: 'L' })).toBe('46L')
  })
})

describe('displacedBy', () => {
  /**
   * The consequence a reader has to be shown before committing: asserting that
   * this slot pairs with that one takes the target away from whoever holds it,
   * and that slot ends up pairing with nothing.
   */
  it('names the slot that currently holds the target', () => {
    // Under offsetResolver's raw-n/en-(n+1) mapping, en slot 5 is currently
    // held by raw slot 4 — not by raw slot 3, the one this correction names.
    const displaced = displacedBy(
      { from: 'raw', to: 'en', a: { page: 3 }, b: { page: 5 } },
      offsetResolver(),
    )
    expect(displaced).toEqual({ page: 4 })
  })

  it('is undefined when nothing holds the target', () => {
    const resolver = { ...offsetResolver(), resolve: () => undefined }
    expect(
      displacedBy(
        { from: 'raw', to: 'en', a: { page: 3 }, b: { page: 5 } },
        resolver,
      ),
    ).toBeUndefined()
  })

  /**
   * `pending.b` is absent before a counterpart has been picked (see
   * `Pending`'s own doc) — there is nothing to name a displaced slot for yet.
   */
  it('is undefined before a counterpart has been picked', () => {
    expect(
      displacedBy({ from: 'raw', to: 'en', a: { page: 3 } }, offsetResolver()),
    ).toBeUndefined()
  })

  /** A correction that asserts what is already true displaces nobody either. */
  it('is undefined when the slot being corrected already holds the target', () => {
    // raw slot 4 already pairs with en slot 5 under offsetResolver's mapping.
    expect(
      displacedBy(
        { from: 'raw', to: 'en', a: { page: 4 }, b: { page: 5 } },
        offsetResolver(),
      ),
    ).toBeUndefined()
  })
})

describe('counterpartPick', () => {
  /** The filmstrip's click job: a slot of `pending.to` becomes `pending.b`. */
  it('sets b to the slot the reader clicked', () => {
    const pending = { from: 'raw', to: 'en', a: { page: 3 } }
    const picked = counterpartPick(pending, offsetResolver(), 5)
    expect(picked).toEqual({ ...pending, b: { page: 5 } })
  })

  /** Re-picking replaces whichever tile was picked before. */
  it('replaces an already-picked b', () => {
    const pending = { from: 'raw', to: 'en', a: { page: 3 }, b: { page: 5 } }
    const picked = counterpartPick(pending, offsetResolver(), 6)
    expect(picked).toEqual({ ...pending, b: { page: 6 } })
  })

  /**
   * A slot that does not resolve to a `Position` — clicking a gap in `to` —
   * leaves `pending` unchanged rather than clearing `b` or throwing, which is
   * what lets a reader click a gap on the filmstrip without the banner
   * reacting.
   */
  it('leaves pending unchanged when the slot does not resolve', () => {
    const pending = { from: 'raw', to: 'en', a: { page: 3 }, b: { page: 5 } }
    const picked = counterpartPick(pending, offsetResolver(), 99)
    expect(picked).toBe(pending)
  })
})

describe('describePosition', () => {
  /**
   * The boundary rule this codebase applies everywhere a page number reaches
   * a person: stored zero-based, shown one-based. `spanOf` above deliberately
   * does the opposite for the wire, and the two must not converge.
   */
  it('reads a page out loud one-based, keeping the half', () => {
    expect(describePosition({ page: 0 })).toBe('page 1')
    expect(describePosition({ page: 44, half: 'R' })).toBe('page 45 R')
    expect(describePosition({ page: 45, half: 'L' })).toBe('page 46 L')
  })

  it('disagrees with the wire format, which stays zero-based', () => {
    const at = { page: 44, half: 'R' } as const
    expect(spanOf(at)).toBe('44R')
    expect(describePosition(at)).toBe('page 45 R')
  })
})

/**
 * The write, the install, and the two rules the render body used to inline.
 *
 * Every case below is either an assertion about *which arguments a port
 * received* or about *which outcome came back* — the same seam, and for the
 * same reason, `notesWrite.ts`'s ten cases use: there is no way to assert on
 * a `PATCH` body or a call count from outside a mounted component, and
 * `App.tsx` cannot be mounted at all.
 */

const WORK = 'gotoubun'

const sidecarJson = (verdict: string) =>
  JSON.stringify({
    alignments: [{ from: 'raw', to: 'en', quality: { verdict } }],
  })

interface PatchCall {
  work: string
  etag: string
  correction: CorrectionRequest
}

/** The server's own 412 sentence, from `routes.rs`'s `patch_alignments`. */
const staleError = () =>
  new Error(
    'correcting alignment: 412 this work has changed since you read it; its ETag is now "v2".',
  )

function scripted(script: {
  patch?: (call: PatchCall) => Promise<FetchedSidecar>
  sidecar?: (work: string) => Promise<FetchedSidecar>
  resolve?: (json: string, attempt: number) => Promise<Resolver>
}) {
  const patches: PatchCall[] = []
  const sidecars: string[] = []
  const resolves: string[] = []
  const ports: CorrectionPorts = {
    patch(work, etag, correction) {
      const call = { work, etag, correction }
      patches.push(call)
      if (!script.patch) throw new Error('unscripted patch')
      return script.patch(call)
    },
    sidecar(work) {
      sidecars.push(work)
      if (!script.sidecar) throw new Error('unscripted sidecar')
      return script.sidecar(work)
    },
    resolve(json) {
      resolves.push(json)
      return script.resolve
        ? script.resolve(json, resolves.length)
        : Promise.resolve(offsetResolver())
    },
  }
  return { patches, sidecars, resolves, ports }
}

const pending = {
  from: 'raw',
  to: 'en',
  a: { page: 3 } as Position,
  b: { page: 4 } as Position,
}

describe('counterpartEdition', () => {
  /**
   * The reader has swapped away from the authority, so the authority is what
   * they are comparing the page on screen against.
   */
  it('is the authority while the reader has swapped away from it', () => {
    const state = { authority: 'raw', slot: 3, viewing: 'en' }
    expect(counterpartEdition(state, 'jp', 'en')).toBe('raw')
  })

  /** Settled on the authority, the most recent "other" edition is `previous`. */
  it('is `previous` while the reader is on the authority', () => {
    const state = { authority: 'raw', slot: 3, viewing: 'raw' }
    expect(counterpartEdition(state, 'en', 'raw')).toBe('en')
  })

  /**
   * A correction with nothing on the other side of it. Reachable at load on a
   * one-edition work, where `initialPrevious` has nowhere else to point.
   */
  it('is nothing where that lands back on the edition on screen', () => {
    const state = { authority: 'raw', slot: 3, viewing: 'raw' }
    expect(counterpartEdition(state, 'raw', 'raw')).toBeUndefined()
  })
})

describe('submitCorrection', () => {
  it('sends the assertion in the sidecar span notation', async () => {
    const { patches, ports } = scripted({
      patch: async () => ({ json: sidecarJson('sound'), etag: '"v2"' }),
    })
    await submitCorrection(
      WORK,
      '"v1"',
      { ...pending, a: { page: 44, half: 'R' } },
      ports,
    )
    expect(patches).toHaveLength(1)
    expect(patches[0].work).toBe(WORK)
    expect(patches[0].etag).toBe('"v1"')
    expect(patches[0].correction).toEqual({
      from: 'raw',
      to: 'en',
      a: '44R',
      b: '4',
    })
  })

  it('lands, carrying the fresh resolver, alignments, validator and slot', async () => {
    const fresh = offsetResolver()
    const { ports } = scripted({
      patch: async () => ({ json: sidecarJson('fragmented'), etag: '"v2"' }),
      resolve: async () => fresh,
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome.kind).toBe('landed')
    if (outcome.kind !== 'landed') return
    expect(outcome.resolver).toBe(fresh)
    expect(outcome.etag).toBe('"v2"')
    expect(outcome.to).toBe('en')
    // `indexOf('en', {page: 4})` on this fixture, which is the page itself.
    expect(outcome.slot).toBe(4)
    // Refreshed from the response body, not left at whatever loaded at
    // session start — `Sidecar::rewrite` recomputes `quality` on every
    // correction.
    expect(outcome.alignments).toEqual([
      { from: 'raw', to: 'en', quality: { verdict: 'fragmented' } },
    ])
  })

  /**
   * `b` resolving to no slot in the fresh document is a real answer, not a
   * bug: the caller leaves the reader where they are rather than navigating.
   */
  it('lands with no slot where the picked page indexes to nothing', async () => {
    const { ports } = scripted({
      patch: async () => ({ json: sidecarJson('sound'), etag: '"v2"' }),
      resolve: async () => ({ ...offsetResolver(), indexOf: () => undefined }),
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome.kind).toBe('landed')
    if (outcome.kind !== 'landed') return
    expect(outcome.slot).toBeUndefined()
  })

  it('refuses with the server message for anything but a 412', async () => {
    const { sidecars, ports } = scripted({
      patch: async () => {
        throw new Error('correcting alignment: 500 the aligner fell over')
      },
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome).toEqual({
      kind: 'refused',
      notice: 'correcting alignment: 500 the aligner fell over',
    })
    // No re-read: nothing about a 500 says the document moved.
    expect(sidecars).toEqual([])
  })

  /** A thrown non-`Error` still reaches the reader as words rather than `[object Object]`. */
  it('refuses with a stringified non-Error', async () => {
    const { ports } = scripted({
      patch: async () => {
        throw 'the network went away'
      },
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome).toEqual({
      kind: 'refused',
      notice: 'the network went away',
    })
  })

  /**
   * **A parse failure on the success path is `refused`, not `landed`.** The
   * write did land on the server, but this build cannot show its result, and
   * saying it worked while the screen still shows the old pairing is the worse
   * of the two wrong answers. Pinned because it falls out of the single `try`
   * rather than being written as a case, which is exactly the kind of thing a
   * later tidy-up removes.
   */
  it('refuses where the fresh document will not parse', async () => {
    const { ports } = scripted({
      patch: async () => ({ json: 'not json', etag: '"v2"' }),
      resolve: async () => {
        throw new Error('bad sidecar')
      },
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome).toEqual({ kind: 'refused', notice: 'bad sidecar' })
  })

  it('re-reads on a stale validator and hands back the current document', async () => {
    const reread = offsetResolver()
    const { patches, sidecars, ports } = scripted({
      patch: async () => {
        throw staleError()
      },
      sidecar: async () => ({ json: sidecarJson('refused'), etag: '"v9"' }),
      resolve: async () => reread,
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome.kind).toBe('stale')
    if (outcome.kind !== 'stale') return
    expect(outcome.resolver).toBe(reread)
    expect(outcome.etag).toBe('"v9"')
    expect(outcome.alignments).toEqual([
      { from: 'raw', to: 'en', quality: { verdict: 'refused' } },
    ])
    expect(outcome.notice).toBe(
      'This work changed since it was read — check the pairing below and confirm again.',
    )
    expect(outcome.error).toBeUndefined()
    // **Re-read, never retried.** One `PATCH`, and the reader confirms again
    // against the document that is actually there.
    expect(patches).toHaveLength(1)
    expect(sidecars).toEqual([WORK])
  })

  /**
   * The re-read failing does not change what the reader is told — the pairing
   * moved under them, which is the fact they can act on. The fetch's own
   * failure goes to the caller to log.
   */
  it('is still stale where the re-read itself fails', async () => {
    const boom = new Error('the sidecar route is down')
    const { ports } = scripted({
      patch: async () => {
        throw staleError()
      },
      sidecar: async () => {
        throw boom
      },
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome.kind).toBe('stale')
    if (outcome.kind !== 'stale') return
    expect(outcome.resolver).toBeUndefined()
    expect(outcome.etag).toBeUndefined()
    expect(outcome.error).toBe(boom)
    expect(outcome.notice).toBe(
      'This work changed since it was read — check the pairing below and confirm again.',
    )
  })

  it('is stale where the re-read parses to nothing', async () => {
    const { ports } = scripted({
      patch: async () => {
        throw staleError()
      },
      sidecar: async () => ({ json: 'not json' }),
      resolve: async () => {
        throw new Error('bad sidecar')
      },
    })
    const outcome = await submitCorrection(WORK, '"v1"', pending, ports)
    expect(outcome.kind).toBe('stale')
    if (outcome.kind !== 'stale') return
    expect(outcome.resolver).toBeUndefined()
    expect((outcome.error as Error).message).toBe('bad sidecar')
  })

  /**
   * **Never disposes anything.** Asserted directly, because the ordering
   * `resolver.ts`'s `dispose` doc requires — free the outgoing one only once a
   * replacement exists — is the caller's to keep, and a `dispose` added here
   * would break it silently on the parse-failure path above.
   */
  it('disposes no resolver, on either path', async () => {
    let disposed = 0
    const outgoing = { ...offsetResolver(), dispose: () => disposed++ }
    const { ports } = scripted({
      patch: async () => ({ json: sidecarJson('sound'), etag: '"v2"' }),
      resolve: async () => outgoing,
    })
    await submitCorrection(WORK, '"v1"', pending, ports)
    expect(disposed).toBe(0)
  })
})

describe('applyLanded', () => {
  const session = {
    work: WORK,
    resolver: offsetResolver(),
    reading: 'rtl' as const,
    editions: ['raw', 'en'],
    alignments: [],
    state: { authority: 'raw', slot: 3, viewing: 'raw' },
    previous: 'en',
  }
  const landed = (over: object = {}) =>
    ({
      kind: 'landed',
      resolver: offsetResolver(),
      alignments: [{ from: 'raw', to: 'en' }],
      etag: '"v2"',
      slot: 4,
      to: 'en',
      ...over,
    }) as Extract<CorrectionOutcome, { kind: 'landed' }>

  it('moves the reader onto the page they picked, in the edition they picked it in', () => {
    const next = applyLanded(session, landed())
    expect(next.state).toEqual({ authority: 'en', slot: 4, viewing: 'en' })
  })

  it('installs the fresh resolver and the fresh alignments', () => {
    const outcome = landed()
    const next = applyLanded(session, outcome)
    expect(next.resolver).toBe(outcome.resolver)
    expect(next.alignments).toEqual([{ from: 'raw', to: 'en' }])
  })

  /**
   * `previous` moves to the edition being *left*, as it must on every change
   * to `viewing` — `session.ts`'s own doc on `previous` is explicit that
   * skipping it on any one route is what makes `Space` lose track. Reached
   * through a correction rather than a keypress is still such a change, and
   * this is the route easiest to forget.
   */
  it('leaves `previous` on the edition being left, not on the old `previous`', () => {
    const next = applyLanded(session, landed())
    expect(next.previous).toBe('raw')
    expect(next.previous).not.toBe(session.previous)
  })

  it('leaves the reader where they are where the pick resolves to no slot', () => {
    const next = applyLanded(session, landed({ slot: undefined }))
    expect(next.state).toEqual(session.state)
    // Still a change to the session, and `previous` still moves — the
    // resolver was replaced whether or not the reader went anywhere.
    expect(next.previous).toBe('raw')
  })
})

describe('correctionView', () => {
  const view = () =>
    correctionView({ from: 'raw', to: 'en', a: { page: 3 } }, offsetResolver())

  it('gives every slot of the edition being corrected against', () => {
    expect(view().slots).toHaveLength(10)
  })

  /**
   * One tile per slot, so a spread's half can be named on its own — see
   * `correctionTiles`' own doc for what `groups` would fold together instead.
   */
  it('gives one tile per slot, never a paired one', () => {
    const { slots, tiles } = view()
    expect(tiles).toHaveLength(slots.length)
    expect(tiles.every((tile) => tile.slots.length === 1)).toBe(true)
  })

  it('marks where the alignment resolves `a` to today', () => {
    // `raw` page 3 pairs with `en` page 4 on this fixture.
    expect(view().pairedIndex).toBe(4)
  })

  /** A real answer `Resolver.resolve` gives, not a bug this may crash on. */
  it('marks nothing where `a` pairs with nothing there', () => {
    const alone = correctionView(
      { from: 'raw', to: 'jp', a: { page: 3 } },
      offsetResolver(),
    )
    expect(alone.pairedIndex).toBeUndefined()
  })

  it('names what a picked counterpart would displace', () => {
    const picked = correctionView(
      { from: 'raw', to: 'en', a: { page: 3 }, b: { page: 7 } },
      offsetResolver(),
    )
    // `en` page 7 is held by `raw` page 6 today, which is not `a`.
    expect(picked.displaced).toEqual({ page: 6, half: undefined })
  })

  it('names nothing displaced before a counterpart is picked', () => {
    expect(view().displaced).toBeUndefined()
  })
})

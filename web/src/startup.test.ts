/**
 * `openWork`, and the two things about a load that only a port seam can see:
 * the *order* the four reads happen in, and which of them is allowed to fail.
 *
 * Both were untestable while this was a `useEffect` in `App.tsx` — not because
 * an effect is hard to drive, but because that file value-imports
 * `resolver.ts` and so is unreachable from Vitest at all. The resolver arrives
 * here as `ports.resolve`, which is what lets `fakeResolver` stand in for the
 * wasm one.
 */
import { describe, expect, it, vi } from 'vitest'
import { bSlots, fakeResolver, whole } from './fixtures'
import type { Resolver } from './resolver'
import { allSlots, openWork, type StartupPorts } from './startup'

const resolver = fakeResolver({ a: whole(8), b: bSlots }, {})

const sidecar = (over: object = {}) =>
  JSON.stringify({
    work: { reading: 'rtl' },
    editions: { a: {}, b: {} },
    hub: 'a',
    alignments: [{ from: 'a', to: 'b', quality: { verdict: 'sound' } }],
    ...over,
  })

/** Every read recorded in the order it was made, so ordering is assertable. */
function scripted(over: Partial<StartupPorts> = {}) {
  const calls: string[] = []
  const ports: StartupPorts = {
    sidecar: async () => {
      calls.push('sidecar')
      return { json: sidecar(), etag: '"v1"' }
    },
    resolve: async () => {
      calls.push('resolve')
      return resolver
    },
    progress: async () => {
      calls.push('progress')
      return undefined
    },
    notes: async () => {
      calls.push('notes')
      return { doc: { work: 'gotoubun', notes: [] }, etag: '"n1"' }
    },
    ...over,
  }
  // The overrides above bypass the recording wrappers, so anything a test
  // replaces records itself.
  return { calls, ports }
}

describe('openWork', () => {
  it('opens the work at the slug it was given', async () => {
    const { ports } = scripted()
    const opened = await openWork(ports, 'gotoubun')
    expect(opened.session.work).toBe('gotoubun')
    expect(opened.session.resolver).toBe(resolver)
    expect(opened.session.reading).toBe('rtl')
    expect(opened.session.editions).toEqual(['a', 'b'])
    expect(opened.session.alignments).toHaveLength(1)
    expect(opened.etag).toBe('"v1"')
  })

  it('reads the sidecar for the slug it was given, not the first work listed', async () => {
    const sidecarFn = vi.fn(async () => ({ json: sidecar(), etag: '"v1"' }))
    const progressFn = vi.fn(async () => undefined)
    const notesFn = vi.fn(async () => ({
      doc: { work: 'gotoubun-v03', notes: [] },
      etag: '"n1"',
    }))
    const ports: StartupPorts = {
      sidecar: sidecarFn,
      resolve: async () => resolver,
      progress: progressFn,
      notes: notesFn,
    }
    await openWork(ports, 'gotoubun-v03')
    expect(sidecarFn).toHaveBeenCalledWith('gotoubun-v03')
    expect(progressFn).toHaveBeenCalledWith('gotoubun-v03')
    expect(notesFn).toHaveBeenCalledWith('gotoubun-v03')
  })

  it('starts at slot 0 of the hub with nothing recorded', async () => {
    const { ports } = scripted()
    const { session } = await openWork(ports, 'gotoubun')
    expect(session.state).toEqual({ authority: 'a', slot: 0, viewing: 'a' })
  })

  it('restores a stored position', async () => {
    const { ports } = scripted({
      progress: async () => ({ edition: 'b', slot: 4 }),
    })
    const { session } = await openWork(ports, 'gotoubun')
    expect(session.state).toEqual({ authority: 'b', slot: 4, viewing: 'b' })
  })

  /**
   * The reason the resolver is built before progress is read, stated as a
   * test rather than only as a comment: `restore` checks a stored slot
   * against the edition it names, so a slot that no longer exists is dropped
   * rather than trusted.
   */
  it('drops a stored slot the edition no longer has', async () => {
    const { ports } = scripted({
      progress: async () => ({ edition: 'b', slot: 400 }),
    })
    const { session } = await openWork(ports, 'gotoubun')
    expect(session.state).toEqual({ authority: 'b', slot: 0, viewing: 'b' })
  })

  /** Never the hub itself — see `initialPrevious`'s own doc for the no-op `Space` that costs. */
  it('seeds `previous` one step ahead of the authority, not at it', async () => {
    const { ports } = scripted()
    const { session } = await openWork(ports, 'gotoubun')
    expect(session.previous).toBe('b')
    expect(session.previous).not.toBe(session.state.authority)
  })

  it('orders the four reads: sidecar, then resolve, then progress, then notes', async () => {
    const { calls, ports } = scripted()
    await openWork(ports, 'gotoubun')
    expect(calls).toEqual(['sidecar', 'resolve', 'progress', 'notes'])
  })

  /**
   * The point of that order: a sidecar this build cannot parse costs one
   * request, not three. Asserted on what was *not* called, which is the only
   * way to state it.
   */
  it('makes no further request once the resolver refuses the document', async () => {
    const { calls, ports } = scripted({
      resolve: async () => {
        calls.push('resolve')
        throw new Error('bad sidecar')
      },
    })
    await expect(openWork(ports, 'gotoubun')).rejects.toThrow('bad sidecar')
    expect(calls).not.toContain('progress')
    expect(calls).not.toContain('notes')
  })

  it('rejects when the sidecar read fails, rather than opening something else', async () => {
    const { ports } = scripted({
      sidecar: async () => {
        throw new Error('loading x: 404 no work `x`')
      },
    })
    await expect(openWork(ports, 'x')).rejects.toThrow(/404/)
  })

  /**
   * **The reachable shape of this throw is a sidecar with no `hub`, not one
   * with an empty `editions` map** — `orderedEditions` puts the hub first
   * unconditionally, so `editions: {}` still yields a one-edition list and
   * opens fine. Written against the state that actually reaches the guard,
   * because a test that cannot reach the line it names tests nothing.
   */
  it('throws where the work names no hub edition', async () => {
    const { ports } = scripted({
      sidecar: async () => ({ json: sidecar({ hub: '', editions: {} }) }),
    })
    await expect(openWork(ports, 'gotoubun')).rejects.toThrow(
      'gotoubun defines no editions',
    )
  })

  it('opens a work whose only edition is its hub', async () => {
    const { ports } = scripted({
      sidecar: async () => ({ json: sidecar({ editions: {} }) }),
    })
    const { session } = await openWork(ports, 'gotoubun')
    expect(session.editions).toEqual(['a'])
  })

  /** A server older than the correction route answers no validator, and a load is still a load. */
  it('carries an absent sidecar validator through as undefined', async () => {
    const { ports } = scripted({
      sidecar: async () => ({ json: sidecar() }),
    })
    const opened = await openWork(ports, 'gotoubun')
    expect(opened.etag).toBeUndefined()
    expect(opened.session.work).toBe('gotoubun')
  })

  it('carries the notes document and its validator', async () => {
    const note = {
      id: '01',
      edition: 'a',
      slot: 1,
      body: 'hi',
      created: 'T',
      updated: 'T',
    }
    const { ports } = scripted({
      notes: async () => ({
        doc: { work: 'gotoubun', notes: [note] },
        etag: '"n7"',
      }),
    })
    const opened = await openWork(ports, 'gotoubun')
    expect(opened.notes.notes).toEqual([note])
    expect(opened.notesEtag).toBe('"n7"')
    expect(opened.notesError).toBeUndefined()
  })

  /**
   * The one read that is allowed to fail. A reader who cannot see their notes
   * should still get their pages — so the session arrives, the document is
   * empty rather than absent, and the validator is missing, which is what
   * makes a later write refuse rather than write unconditionally.
   */
  it('still opens the work when the notes read fails', async () => {
    const boom = new Error('notes are down')
    const { ports } = scripted({
      notes: async () => {
        throw boom
      },
    })
    const opened = await openWork(ports, 'gotoubun')
    expect(opened.session.work).toBe('gotoubun')
    expect(opened.notes).toEqual({ work: 'gotoubun', notes: [] })
    expect(opened.notesEtag).toBeUndefined()
    expect(opened.notesError).toBe(boom)
  })

  /** Every other read is not: a work whose sidecar cannot be fetched has no reader to show. */
  it('rejects when the sidecar read fails', async () => {
    const { ports } = scripted({
      sidecar: async () => {
        throw new Error('sidecar is down')
      },
    })
    await expect(openWork(ports, 'gotoubun')).rejects.toThrow('sidecar is down')
  })
})

describe('allSlots', () => {
  it('gives every slot of an edition in order', () => {
    expect(allSlots(resolver, 'b')).toEqual(bSlots)
  })

  /** `slotCount` answers `undefined` for an edition the sidecar does not name. */
  it('gives nothing for an unknown edition', () => {
    expect(allSlots(resolver, 'nope')).toEqual([])
  })

  it('gives nothing for an edition with no slots', () => {
    const empty: Resolver = fakeResolver({ a: [] }, {})
    expect(allSlots(empty, 'a')).toEqual([])
  })
})

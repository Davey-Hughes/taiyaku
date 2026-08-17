/**
 * `openFirstWork`, and the two things about a load that only a port seam can
 * see: the *order* the five reads happen in, and which of them is allowed to
 * fail.
 *
 * Both were untestable while this was a `useEffect` in `App.tsx` — not because
 * an effect is hard to drive, but because that file value-imports
 * `resolver.ts` and so is unreachable from Vitest at all. The resolver arrives
 * here as `ports.resolve`, which is what lets `fakeResolver` stand in for the
 * wasm one.
 */
import { describe, expect, it } from 'vitest'
import { bSlots, fakeResolver, whole } from './fixtures'
import type { Resolver } from './resolver'
import { allSlots, openFirstWork, type StartupPorts } from './startup'

const resolver = fakeResolver({ a: whole(8), b: bSlots }, {})

const sidecar = (over: object = {}) =>
  JSON.stringify({
    work: { reading: 'rtl' },
    editions: { a: {}, b: {} },
    hub: 'a',
    alignments: [{ from: 'a', to: 'b', quality: { verdict: 'sound' } }],
    ...over,
  })

/** The default fixture's sidecar, fixed rather than rebuilt per test — the
 * one test below that opens straight from a listing of objects, with no
 * `scripted()` in between, wants a literal it can point `sidecar` at. */
const SIDECAR_JSON = sidecar()

/** Every read recorded in the order it was made, so ordering is assertable. */
function scripted(over: Partial<StartupPorts> = {}) {
  const calls: string[] = []
  const ports: StartupPorts = {
    works: async () => {
      calls.push('works')
      return [{ slug: 'gotoubun', title: 'Gotoubun', editions: ['a', 'b'] }]
    },
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

describe('openFirstWork', () => {
  it('opens the first work the server offers', async () => {
    const { ports } = scripted()
    const opened = await openFirstWork(ports)
    expect(opened.session.work).toBe('gotoubun')
    expect(opened.session.resolver).toBe(resolver)
    expect(opened.session.reading).toBe('rtl')
    expect(opened.session.editions).toEqual(['a', 'b'])
    expect(opened.session.alignments).toHaveLength(1)
    expect(opened.etag).toBe('"v1"')
  })

  /** Task 14: `works` answers a listing of objects rather than bare ids, and
   * a load opens the first one's `.slug` — driven with a literal `StartupPorts`
   * rather than through `scripted()`, so this stands on its own as the shape
   * `GET /api/works` actually answers. `resolve` answers the module-level
   * `resolver` rather than a bare `fakeResolver()`: that helper takes the
   * slots and pairs its first two (required) arguments describe, and the
   * shared fixture above already builds one matching `SIDECAR_JSON`'s `a`/`b`
   * editions. */
  it('opens the first work from a listing of objects', async () => {
    const opened = await openFirstWork({
      works: async () => [
        {
          slug: 'gotoubun-v01',
          title: 'Go-Toubun v01',
          editions: ['raw', 'en'],
        },
      ],
      sidecar: async () => ({ json: SIDECAR_JSON, etag: 'W/"1"' }),
      resolve: async () => resolver,
      progress: async () => undefined,
      notes: async () => ({
        doc: { work: 'gotoubun-v01', notes: [] },
        etag: 'W/"2"',
      }),
    })
    expect(opened.session.work).toBe('gotoubun-v01')
  })

  it('starts at slot 0 of the hub with nothing recorded', async () => {
    const { ports } = scripted()
    const { session } = await openFirstWork(ports)
    expect(session.state).toEqual({ authority: 'a', slot: 0, viewing: 'a' })
  })

  it('restores a stored position', async () => {
    const { ports } = scripted({
      progress: async () => ({ edition: 'b', slot: 4 }),
    })
    const { session } = await openFirstWork(ports)
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
    const { session } = await openFirstWork(ports)
    expect(session.state).toEqual({ authority: 'b', slot: 0, viewing: 'b' })
  })

  /** Never the hub itself — see `initialPrevious`'s own doc for the no-op `Space` that costs. */
  it('seeds `previous` one step ahead of the authority, not at it', async () => {
    const { ports } = scripted()
    const { session } = await openFirstWork(ports)
    expect(session.previous).toBe('b')
    expect(session.previous).not.toBe(session.state.authority)
  })

  it('orders the five reads: sidecar, then resolve, then progress, then notes', async () => {
    const { calls, ports } = scripted()
    await openFirstWork(ports)
    expect(calls).toEqual(['works', 'sidecar', 'resolve', 'progress', 'notes'])
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
    await expect(openFirstWork(ports)).rejects.toThrow('bad sidecar')
    expect(calls).not.toContain('progress')
    expect(calls).not.toContain('notes')
  })

  it('throws where the server offers no works', async () => {
    const { ports } = scripted({ works: async () => [] })
    await expect(openFirstWork(ports)).rejects.toThrow('no works available')
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
    await expect(openFirstWork(ports)).rejects.toThrow(
      'gotoubun defines no editions',
    )
  })

  it('opens a work whose only edition is its hub', async () => {
    const { ports } = scripted({
      sidecar: async () => ({ json: sidecar({ editions: {} }) }),
    })
    const { session } = await openFirstWork(ports)
    expect(session.editions).toEqual(['a'])
  })

  /** A server older than the correction route answers no validator, and a load is still a load. */
  it('carries an absent sidecar validator through as undefined', async () => {
    const { ports } = scripted({
      sidecar: async () => ({ json: sidecar() }),
    })
    const opened = await openFirstWork(ports)
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
    const opened = await openFirstWork(ports)
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
    const opened = await openFirstWork(ports)
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
    await expect(openFirstWork(ports)).rejects.toThrow('sidecar is down')
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

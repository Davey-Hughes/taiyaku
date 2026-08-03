import { describe, expect, it } from 'vitest'
import { fakeResolver, whole, bSlots } from './fixtures'
import type { ReadingState } from './reading'
import {
  applyIntent,
  initialPrevious,
  orderedEditions,
  resolveOffered,
  takeOffered,
  verdictFor,
  type Alignment,
} from './session'

// Same fixture `reading.test.ts` uses: `a` slots 1,2,3 pair with `b` slots
// 0,1,2, `a` slot 3 pairs with `b` slot 2 (one half of a joined spread), and
// `a` slot 0 and 4..7 pair with nothing.
const r = fakeResolver(
  { a: whole(8), b: bSlots },
  { 'a|b|1': 0, 'a|b|2': 1, 'a|b|3': 2, 'b|a|0': 1, 'b|a|1': 2, 'b|a|2': 3 },
)

describe('applyIntent', () => {
  it('a toggle intent never changes state.authority', () => {
    let state: ReadingState = { authority: 'a', slot: 2, viewing: 'a' }
    let previous = 'b'

    ;({ state, previous } = applyIntent({ kind: 'toggle' }, state, previous, r))
    expect(state.authority).toBe('a')
    expect(state.viewing).toBe('b')

    ;({ state, previous } = applyIntent({ kind: 'toggle' }, state, previous, r))
    expect(state.authority).toBe('a')
    expect(state.viewing).toBe('a')
  })

  it('a view intent never changes state.authority', () => {
    const state: ReadingState = { authority: 'a', slot: 2, viewing: 'a' }
    const { state: next } = applyIntent({ kind: 'view', edition: 'b' }, state, 'a', r)
    expect(next.authority).toBe('a')
    expect(next.viewing).toBe('b')
  })

  it('a step intent is the only thing that changes state.authority', () => {
    const start: ReadingState = { authority: 'a', slot: 3, viewing: 'a' }

    const { state: viewed } = applyIntent({ kind: 'view', edition: 'b' }, start, 'a', r)
    expect(viewed.authority).toBe('a') // view alone: unchanged, per the test above

    const { state: stepped } = applyIntent({ kind: 'step', by: 1 }, viewed, 'a', r)
    expect(stepped.authority).toBe('b') // step: authority now follows viewing
    expect(stepped.slot).toBe(3)
  })

  it('step at slot 0 going backward stays at 0', () => {
    const state: ReadingState = { authority: 'a', slot: 0, viewing: 'a' }
    const { state: next } = applyIntent({ kind: 'step', by: -1 }, state, 'a', r)
    expect(next).toEqual({ authority: 'a', slot: 0, viewing: 'a' })
  })

  it('step at the last slot going forward stays there', () => {
    const state: ReadingState = { authority: 'a', slot: 7, viewing: 'a' }
    const { state: next } = applyIntent({ kind: 'step', by: 1 }, state, 'a', r)
    expect(next).toEqual({ authority: 'a', slot: 7, viewing: 'a' })
  })

  // The reviewer's trace: from the hub, press a digit (view C), press a digit
  // (view hub), then Space. `previous` must have tracked the outgoing edition
  // on *both* digit presses, not just the last one, or Space lands back on
  // the hub instead of C — a silent no-op standing in for "go back".
  it('tracks previous across consecutive view intents, so Space returns to the edition just left', () => {
    let state: ReadingState = { authority: 'a', slot: 0, viewing: 'a' } // hub
    let previous = 'a'

    ;({ state, previous } = applyIntent({ kind: 'view', edition: 'b' }, state, previous, r)) // digit -> C
    expect(state.viewing).toBe('b')

    ;({ state, previous } = applyIntent({ kind: 'view', edition: 'a' }, state, previous, r)) // digit -> hub
    expect(state.viewing).toBe('a')

    ;({ state, previous } = applyIntent({ kind: 'toggle' }, state, previous, r)) // Space
    expect(state.viewing).toBe('b') // back to C, not a no-op onto the hub
  })
})

describe('resolveOffered', () => {
  // Same gap `nearestPaired`'s tests use: `a` slot 4 is unpaired in `b`, with
  // slot 3 the nearest paired slot before it, landing on `b` slot 2 — one
  // half of a joined spread, so its page (2) is not the same number as its
  // slot.
  const gap: ReadingState = { authority: 'a', slot: 4, viewing: 'b' }

  it('resolves the offered authority-space slot to where it lands in the viewed edition', () => {
    expect(resolveOffered(gap, 3, r)).toEqual({ edition: 'b', slot: 2, page: 2 })
  })

  it('is undefined if the offered slot does not actually resolve', () => {
    expect(resolveOffered(gap, 4, r)).toBeUndefined()
  })
})

describe('takeOffered', () => {
  const gap: ReadingState = { authority: 'a', slot: 4, viewing: 'b' }

  it('resolves the offered authority-space slot into the viewed edition and commits authority there', () => {
    expect(takeOffered(gap, 3, r)).toEqual({ authority: 'b', slot: 2, viewing: 'b' })
  })

  it('is a no-op if the offered slot does not actually resolve', () => {
    expect(takeOffered(gap, 4, r)).toEqual(gap)
  })
})

describe('verdictFor', () => {
  const alignments: Alignment[] = [
    { from: 'raw-mono', to: 'en-mono', quality: { verdict: 'sound' } },
    { from: 'raw-mono', to: 'en-fan', quality: { verdict: 'fragmented' } },
  ]

  it('finds the verdict regardless of which side is asked first', () => {
    expect(verdictFor(alignments, 'raw-mono', 'en-fan')).toBe('fragmented')
    expect(verdictFor(alignments, 'en-fan', 'raw-mono')).toBe('fragmented')
  })

  it('does not claim trouble for a sound pair', () => {
    expect(verdictFor(alignments, 'raw-mono', 'en-mono')).toBe('sound')
  })

  it('is undefined where no direct edge is stored, rather than composing through the hub', () => {
    expect(verdictFor(alignments, 'en-mono', 'en-fan')).toBeUndefined()
  })
})

describe('initialPrevious', () => {
  it('seeds the edition one step ahead of authority in display order', () => {
    expect(initialPrevious(['a', 'b', 'c'], 'a')).toBe('b')
    expect(initialPrevious(['a', 'b', 'c'], 'b')).toBe('c')
  })

  it('wraps from the last edition back to the first', () => {
    expect(initialPrevious(['a', 'b', 'c'], 'c')).toBe('a')
  })

  it('is authority itself when there is no other edition to seed with', () => {
    expect(initialPrevious(['a'], 'a')).toBe('a')
  })

  it('is authority itself if authority is not even in the list', () => {
    expect(initialPrevious(['a', 'b'], 'z')).toBe('z')
  })
})

// Seeding `previous` with `authority` itself would make every `Space` after a
// fresh load a silent no-op — `applyIntent`'s toggle arm computes
// `state.viewing === state.authority ? previous : state.authority`, so
// `previous === authority === viewing` targets the edition already shown, and
// `swap` reassigns `previous` right back to that same value. This pins the
// fix at the seam the two functions actually meet: a freshly-restored state,
// seeded via `initialPrevious`, must have its very first toggle change
// `viewing`.
describe('a freshly-restored session, seeded via initialPrevious', () => {
  it("the first toggle changes viewing, for every edition restore might start on", () => {
    for (const authority of r.editions()) {
      const state: ReadingState = { authority, slot: 0, viewing: authority }
      const previous = initialPrevious(r.editions(), authority)

      const { state: toggled } = applyIntent({ kind: 'toggle' }, state, previous, r)

      expect(toggled.viewing, `starting from ${authority}`).not.toBe(authority)
    }
  })
})

describe('orderedEditions', () => {
  it('puts the hub first regardless of where it sorts', () => {
    expect(orderedEditions('en-mono', ['raw', 'en-colour', 'en-mono'])).toEqual([
      'en-mono', 'en-colour', 'raw',
    ])
  })

  it('sorts the rest alphabetically, not by input order', () => {
    expect(orderedEditions('raw', ['en-mono', 'en-colour', 'en-fan'])).toEqual([
      'raw', 'en-colour', 'en-fan', 'en-mono',
    ])
  })

  it('is stable across differently-ordered input, which is the point', () => {
    const shuffled = orderedEditions('raw', ['en-fan', 'raw', 'en-mono'])
    const alsoShuffled = orderedEditions('raw', ['en-mono', 'raw', 'en-fan'])
    expect(shuffled).toEqual(alsoShuffled)
  })
})

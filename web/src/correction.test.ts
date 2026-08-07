import { describe, expect, it } from 'vitest'
import { describePosition, displacedBy, spanOf } from './correction'
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

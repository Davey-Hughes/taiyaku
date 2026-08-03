import { describe, expect, it } from 'vitest'
import { fakeResolver, whole, bSlots } from './fixtures'
import { nearestPaired } from './nearest'

const r = fakeResolver(
  { a: whole(8), b: bSlots },
  { 'a|b|1': 0, 'a|b|2': 1, 'a|b|3': 2 },
)

describe('nearestPaired', () => {
  it('offers the paired slots either side of a gap', () => {
    // `a` slot 4 is unpaired; 3 is the nearest paired before it and there is
    // nothing paired after it.
    expect(nearestPaired({ authority: 'a', slot: 4, viewing: 'b' }, r, 8))
      .toEqual({ before: 3 })
  })

  it('offers only what exists at the start of a volume', () => {
    expect(nearestPaired({ authority: 'a', slot: 0, viewing: 'b' }, r, 8))
      .toEqual({ after: 1 })
  })

  it('offers nothing where the whole edition is unpaired', () => {
    const none = fakeResolver({ a: whole(3), b: whole(3) }, {})
    expect(nearestPaired({ authority: 'a', slot: 1, viewing: 'b' }, none, 3)).toEqual({})
  })
})

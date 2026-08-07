import { describe, expect, it } from 'vitest'
import { fakeResolver, whole } from './fixtures'

// `credits` is a table lookup keyed on the edition alone, unlike `chapterAt`'s
// `edition|index` key — a credits list belongs to the edition as a whole, not
// to one slot of it. Mirrors `fixtures.test.ts`'s `chapterAt` coverage: a
// present entry, and a key absent from the table (which covers both "this
// edition has no list" and "there is no such edition" — the fake does not
// distinguish them any more than the wasm binding does).
describe('Resolver.credits', () => {
  const fake = fakeResolver({ fan: whole(6) }, {}, {}, { fan: [3, 4] })

  it('answers the pages a table names for a present edition', () => {
    expect(fake.credits('fan')).toEqual([3, 4])
  })

  it('answers empty for an edition with no list, or none at all', () => {
    expect(fake.credits('no-such-edition')).toEqual([])
  })
})

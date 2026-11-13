import { describe, expect, it } from 'vitest'
import { bSlots, fakeResolver, whole } from './support/fixtures'

// `fakeResolver`'s `chapterAt` is a table lookup, same shape as `pairs`. A
// table-driven fake can silently encode the wrong answer (`member`/`of`
// swapped, a key format the lookup never builds). Mirrors `taiyaku-core`'s
// `chapter_at_names_the_member_a_slot_lands_in` and
// `chapter_at_is_none_where_no_edition_is_chapter_organised`: a present
// chapter, and an absent one.
describe('fakeResolver chapterAt', () => {
  const fake = fakeResolver(
    { volume: whole(6) },
    {},
    { 'volume|0': { member: 0, of: 3, path: 'c001.cbz' } },
  )

  it('answers the entry the table names for a present key', () => {
    expect(fake.chapterAt('volume', 0)).toEqual({
      member: 0,
      of: 3,
      path: 'c001.cbz',
    })
  })

  it('answers undefined for a key absent from the table', () => {
    expect(fake.chapterAt('volume', 1)).toBeUndefined()
  })
})

// `fakeResolver`'s `memberPage` is derived, not looked up: tests that check a
// note's `at` lean on its half spelling and on no two slots sharing one.
describe('fakeResolver memberPage', () => {
  const fake = fakeResolver({ a: whole(3), b: bSlots }, {})

  it('names a spread half in the wire spelling, and a whole page with none', () => {
    expect(fake.memberPage('b', 2)).toEqual({
      sha256: 'b',
      nth: 0,
      page: 2,
      half: 'right',
    })
    expect(fake.memberPage('b', 3)).toEqual({
      sha256: 'b',
      nth: 0,
      page: 2,
      half: 'left',
    })
    expect(fake.memberPage('b', 4)).not.toHaveProperty('half')
  })

  it('gives every slot of every edition an anchor of its own', () => {
    const anchors = [
      ...[0, 1, 2].map((i) => fake.memberPage('a', i)),
      ...bSlots.map((_, i) => fake.memberPage('b', i)),
    ].map((at) => JSON.stringify(at))
    expect(anchors).not.toContain(undefined)
    expect(new Set(anchors).size).toBe(3 + bSlots.length)
  })

  it('answers undefined past the last slot and for an unknown edition', () => {
    expect(fake.memberPage('a', 3)).toBeUndefined()
    expect(fake.memberPage('ghost', 0)).toBeUndefined()
  })
})

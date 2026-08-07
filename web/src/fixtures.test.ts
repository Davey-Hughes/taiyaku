import { describe, expect, it } from 'vitest'
import { fakeResolver, whole } from './fixtures'

// `fakeResolver`'s `chapterAt` is a table lookup, same shape as `pairs` — but
// unlike `slots`/`pairs`, nothing exercises it through ordinary use of the
// fake elsewhere in this suite. A table-driven fake can silently encode the
// wrong answer (`member`/`of` swapped, a key format the lookup never
// builds) and nothing would catch it until a later task's tests depend on
// it. Mirrors `taiyaku-core`'s `chapter_at_names_the_member_a_slot_lands_in`
// and `chapter_at_is_none_where_no_edition_is_chapter_organised`: a present
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

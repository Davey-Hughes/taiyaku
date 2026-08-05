import { describe, expect, it } from 'vitest'
import { ulid } from './ulid'

const CROCKFORD = /^[0123456789ABCDEFGHJKMNPQRSTVWXYZ]{26}$/

describe('ulid', () => {
  it('is 26 Crockford base32 characters', () => {
    expect(ulid(Date.parse('2026-08-05T10:00:00Z'), () => 0.5)).toMatch(CROCKFORD)
  })

  it('sorts by the time it was minted, as plain strings', () => {
    const early = ulid(Date.parse('2026-08-05T10:00:00Z'), () => 0.5)
    const later = ulid(Date.parse('2026-08-05T10:00:01Z'), () => 0.5)
    expect([later, early].sort()).toEqual([early, later])
  })

  it('differs within one millisecond, which is where two notes actually collide', () => {
    const at = Date.parse('2026-08-05T10:00:00Z')
    let n = 0
    const counter = () => {
      n += 1
      return (n % 32) / 32
    }
    expect(ulid(at, counter)).not.toBe(ulid(at, counter))
  })

  // Two devices minting an id in the same millisecond is the case the random
  // half exists for: the timestamp is identical, so only randomness separates
  // them, and a build that dropped it would collide silently.
  it('varies in its random half when the timestamp is identical', () => {
    const at = Date.parse('2026-08-05T10:00:00Z')
    const a = ulid(at, () => 0.25)
    const b = ulid(at, () => 0.75)
    expect(a.slice(0, 10)).toBe(b.slice(0, 10))
    expect(a.slice(10)).not.toBe(b.slice(10))
  })

  // `random()` is documented as `[0, 1)`, but a source that ever returned
  // exactly 1 would index one past the alphabet and put `undefined` in an id
  // that is then written to a file and used as a key forever.
  it('cannot produce an id with a character outside the alphabet', () => {
    expect(ulid(Date.parse('2026-08-05T10:00:00Z'), () => 1)).toMatch(CROCKFORD)
  })
})

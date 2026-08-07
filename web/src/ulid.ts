/**
 * A sortable, collision-resistant id, minted where the note is written.
 *
 * Notes are created on whichever device the reader is holding and written as a
 * whole document, so two devices annotating one work mint ids without ever
 * seeing each other's. A server-assigned id would need a round trip before the
 * reader could type; a counter would collide the first time two devices were
 * both offline.
 *
 * ULID's layout, in 26 Crockford base32 characters: 48 bits of milliseconds
 * first, so a plain string sort is a chronological sort and the review list
 * needs no comparator, then 80 bits of randomness, which is what separates two
 * ids minted in the same millisecond.
 *
 * Written here rather than taken as a dependency: the whole implementation is
 * the thirty lines below, and `docs/status.md` states this project's
 * dependency list as a standing claim.
 */
const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'

/** Crockford base32, `length` characters, most significant first. */
function encode(value: number, length: number): string {
  let out = ''
  let left = Math.floor(value)
  for (let i = 0; i < length; i++) {
    out = ALPHABET[left % 32] + out
    left = Math.floor(left / 32)
  }
  return out
}

/**
 * Pure over an injected clock and randomness — `Date.now` and `crypto` are
 * both things a test cannot pin, and "did the random half actually vary" is
 * the one property here worth testing. `App.tsx` calls
 * `ulid(Date.now(), cryptoRandom)`.
 */
export function ulid(now: number, random: () => number): string {
  const time = encode(now, 10)
  let entropy = ''
  // Sixteen characters of five bits each, drawn one at a time rather than from
  // one 80-bit number: JavaScript has no 80-bit integer, and assembling one
  // out of two doubles would lose bits in exactly the half that exists to be
  // unpredictable.
  //
  // `Math.min(…, 31)` rather than a modulo: `random()` is documented as
  // `[0, 1)`, but a source that ever returned exactly 1 would index one past
  // the alphabet and put the string `undefined` inside an id that is then
  // written to a file and used as a key forever. Clamping keeps that a
  // vanishingly rare `Z` instead of a corrupt document.
  for (let i = 0; i < 16; i++) {
    entropy += ALPHABET[Math.min(Math.floor(random() * 32), 31)]
  }
  return time + entropy
}

/**
 * `crypto.getRandomValues`, as a `() => number` in `[0, 1)`.
 *
 * `Math.random` is not seeded per device and is explicitly not required to be
 * unpredictable; two devices starting from the same page load are exactly the
 * case the random half exists for.
 */
export const cryptoRandom = (): number =>
  crypto.getRandomValues(new Uint32Array(1))[0] / 2 ** 32

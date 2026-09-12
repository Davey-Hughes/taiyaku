import { describe, expect, it } from 'vitest'
import { messageOf } from './message'

describe('messageOf', () => {
  it('reads an Error by its message rather than its toString', () => {
    // `String(new Error('nope'))` is `'Error: nope'`. The prefix is the thing
    // this arm exists to drop: the screens that print this put it in a
    // failure notice a person reads, not in a log.
    expect(messageOf(new Error('nope'))).toBe('nope')
  })

  it('reads a subclass the same way', () => {
    expect(messageOf(new TypeError('wrong shape'))).toBe('wrong shape')
  })

  it('stringifies anything else, which is the arm forty call sites never reached', () => {
    // Nothing in this codebase throws a non-Error deliberately; the arm is
    // there because `unknown` is what a `catch` binds and a thrown string is
    // what a dependency can hand back. Pinned once here rather than being
    // thirty uncovered branches spread over twenty-one files.
    expect(messageOf('a bare string')).toBe('a bare string')
    expect(messageOf(undefined)).toBe('undefined')
    expect(messageOf({ code: 500 })).toBe('[object Object]')
  })
})

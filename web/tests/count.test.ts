import { describe, expect, it } from 'vitest'
import { count } from '../src/count'

describe('count', () => {
  it.each([
    [0, '0 works'],
    [1, '1 work'],
    [2, '2 works'],
  ])('says %i of a thing in the number it is', (n, said) => {
    expect(count(n, 'work', 'works')).toBe(said)
  })
})

import { describe, expect, it } from 'vitest'
import {
  bookTextModule,
  LARGER_INTENT,
  resized,
  SMALLER_INTENT,
  stepped,
  USUAL_INTENT,
} from '../../src/modules/bookText'

describe('the book’s text size', () => {
  const at = (textSize: number) => ({ ...bookTextModule.defaults, textSize })

  it('steps by two pixels, and back to the usual size', () => {
    expect(resized(at(18), LARGER_INTENT).textSize).toBe(20)
    expect(resized(at(18), SMALLER_INTENT).textSize).toBe(16)
    expect(resized(at(30), USUAL_INTENT).textSize).toBe(18)
  })

  it('stays between twelve and forty pixels', () => {
    expect(resized(at(40), LARGER_INTENT).textSize).toBe(40)
    expect(resized(at(12), SMALLER_INTENT).textSize).toBe(12)
  })

  it('steps several at once, within the same bounds', () => {
    expect(stepped(at(18), 3).textSize).toBe(24)
    expect(stepped(at(18), -2).textSize).toBe(14)
    expect(stepped(at(38), 3).textSize).toBe(40)
  })

  it('leaves the size alone for another intent', () => {
    expect(resized(at(22), 'help:toggle')).toEqual(at(22))
  })
})

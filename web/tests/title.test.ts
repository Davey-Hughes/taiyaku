import { describe, expect, it } from 'vitest'
import { documentTitle } from '../src/title'

describe('documentTitle — O173', () => {
  it('puts the screen first and the app last', () => {
    expect(documentTitle('Library')).toBe('Library — taiyaku')
  })

  it('says the app alone while the screen’s name is not known', () => {
    expect(documentTitle(undefined)).toBe('taiyaku')
    expect(documentTitle('')).toBe('taiyaku')
  })
})

import { describe, expect, it } from 'vitest'
import { humanBytes } from './bytes'

describe('humanBytes', () => {
  // The block moved here from `backup.test.ts` when the function moved out of
  // `backup.ts`; the assertions are unchanged.
  it('counts bytes in bytes', () => {
    expect(humanBytes(0)).toBe('0 B')
    expect(humanBytes(512)).toBe('512 B')
  })

  it('uses binary units and names them as such', () => {
    // KiB rather than kB, because the divisor is 1024. A label that says kB
    // over a 1024-divisor is a figure that disagrees with `ls -l`.
    expect(humanBytes(1024)).toBe('1.0 KiB')
    expect(humanBytes(1536)).toBe('1.5 KiB')
    expect(humanBytes(5 * 1024 * 1024 * 1024)).toBe('5.0 GiB')
  })
})

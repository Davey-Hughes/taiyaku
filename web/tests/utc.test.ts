import { describe, expect, it } from 'vitest'
import { readableTaken } from '../src/backup'
import { inUtc } from '../src/utc'

describe('inUtc', () => {
  it('says a time the server wrote is UTC, in the form the Backups list gives a backup’s time', () => {
    expect(inUtc('2026-10-22 02:01:26')).toBe('2026-10-22 02:01:26 UTC')
    expect(readableTaken('20261022T020126Z')).toBe(inUtc('2026-10-22 02:01:26'))
  })
})

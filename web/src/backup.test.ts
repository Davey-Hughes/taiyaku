import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  backupUrl,
  humanBytes,
  listBackups,
  readableTaken,
  takeBackup,
} from './backup'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('humanBytes', () => {
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

describe('readableTaken', () => {
  it('spaces out the stamp the name carries', () => {
    expect(readableTaken('20260906T143502Z')).toBe('2026-09-06 14:35:02 UTC')
  })

  it('hands back anything that is not that shape, verbatim', () => {
    // `backup::stamp_of` enforces the 16-character shape, so reaching this is
    // a server that is not this one. Passing the string through says "this is
    // what the server sent"; reformatting a guess would invent a date.
    expect(readableTaken('yesterday')).toBe('yesterday')
  })
})

describe('the backup client', () => {
  it('asks for the list', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json({ backups: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await listBackups()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/backups')
  })

  it('takes one with an empty POST and unwraps it', async () => {
    const fetchMock = vi.fn().mockImplementation(() =>
      json({
        backup: {
          name: 'library-20260906T143502Z.db',
          bytes: 4096,
          taken: '20260906T143502Z',
        },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const taken = await takeBackup()
    expect(taken.name).toBe('library-20260906T143502Z.db')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    expect(init.body).toBeUndefined()
  })

  it('addresses a download by name, encoded', () => {
    expect(backupUrl('library-20260906T143502Z.db')).toBe(
      '/api/library/backups/library-20260906T143502Z.db',
    )
    expect(backupUrl('a b')).toBe('/api/library/backups/a%20b')
  })
})

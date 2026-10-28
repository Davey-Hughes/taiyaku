import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  addDir,
  createLibrary,
  libraryRemovalCost,
  libraryScanStatus,
  listDirs,
  listLibraries,
  moveCost,
  moveDir,
  noLibrarySentence,
  removeLibrary,
  renameLibrary,
  startLibraryScan,
} from '../src/libraries'

const answering = (body: unknown) => {
  const fetchMock = vi.fn().mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    ),
  )
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

/** What the first request was: its path, its method, and its body parsed. */
const sent = (fetchMock: ReturnType<typeof vi.fn>) => {
  const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit?]
  return {
    path,
    method: init?.method ?? 'GET',
    body:
      typeof init?.body === 'string'
        ? (JSON.parse(init.body) as unknown)
        : undefined,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the libraries client', () => {
  it.each([
    [
      'listLibraries',
      () => listLibraries(),
      'GET',
      '/api/library/libraries',
      undefined,
    ],
    [
      'createLibrary',
      () => createLibrary('Manga', ['/media/a', '/media/b']),
      'POST',
      '/api/library/libraries',
      { name: 'Manga', dirs: ['/media/a', '/media/b'] },
    ],
    [
      'renameLibrary',
      () => renameLibrary(7, 'Comics'),
      'PATCH',
      '/api/library/libraries/7',
      { name: 'Comics' },
    ],
    [
      'addDir',
      () => addDir(7, '/media/c'),
      'POST',
      '/api/library/libraries/7/dirs',
      { path: '/media/c' },
    ],
    [
      'libraryRemovalCost',
      () => libraryRemovalCost(7),
      'DELETE',
      '/api/library/libraries/7?dry_run=true',
      undefined,
    ],
    [
      'removeLibrary',
      () => removeLibrary(7),
      'DELETE',
      '/api/library/libraries/7',
      undefined,
    ],
    [
      'moveCost',
      () => moveCost(3, { library: 7 }),
      'POST',
      '/api/library/roots/3/move?dry_run=true',
      { library: 7 },
    ],
    [
      'moveDir',
      () => moveDir(3, { new_library: 'Extras' }),
      'POST',
      '/api/library/roots/3/move',
      { new_library: 'Extras' },
    ],
    [
      'startLibraryScan',
      () => startLibraryScan(7),
      'POST',
      '/api/library/libraries/7/scan',
      undefined,
    ],
    [
      'libraryScanStatus',
      () => libraryScanStatus(7),
      'GET',
      '/api/library/libraries/7/scan',
      undefined,
    ],
    [
      'listDirs, at the prefixes',
      () => listDirs(),
      'GET',
      '/api/library/dirs',
      undefined,
    ],
    [
      'listDirs, at a path',
      () => listDirs('/mnt/user/media 1/Manga & more'),
      'GET',
      '/api/library/dirs?path=%2Fmnt%2Fuser%2Fmedia%201%2FManga%20%26%20more',
      undefined,
    ],
  ])(
    '%s asks the route it is named for',
    async (_name, call, method, path, body) => {
      const fetchMock = answering({})
      await call()
      expect(sent(fetchMock)).toEqual({ path, method, body })
    },
  )

  it('refuses an empty 204 where a body was expected', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockImplementation(() =>
          Promise.resolve(new Response(null, { status: 204 })),
        ),
    )
    await expect(renameLibrary(7, 'Comics')).rejects.toThrow(
      'expected a JSON body, got an empty 204',
    )
  })
})

describe('noLibrarySentence', () => {
  it('names one account twice, and no pronoun stands for it', () => {
    expect(noLibrarySentence(['alice'], 'this library')).toBe(
      'alice can see only this library. After this, alice will see no library until an administrator grants one.',
    )
  })

  it('joins two names with "and", and says "they" after', () => {
    expect(noLibrarySentence(['alice', 'bob'], 'this library')).toBe(
      'alice and bob can see only this library. After this, they will see no library until an administrator grants one.',
    )
  })

  it('joins three names with commas and a last "and", and names the directory’s library', () => {
    expect(
      noLibrarySentence(['alice', 'bob', 'carol'], "this directory's library"),
    ).toBe(
      "alice, bob and carol can see only this directory's library. After this, they will see no library until an administrator grants one.",
    )
  })
})

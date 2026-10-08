import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  addFolder,
  createLibrary,
  libraryRemovalCost,
  libraryScanStatus,
  listFolders,
  listLibraries,
  removeLibrary,
  renameLibrary,
  startLibraryScan,
} from './libraries'

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
      { name: 'Manga', folders: ['/media/a', '/media/b'] },
    ],
    [
      'renameLibrary',
      () => renameLibrary(7, 'Comics'),
      'PATCH',
      '/api/library/libraries/7',
      { name: 'Comics' },
    ],
    [
      'addFolder',
      () => addFolder(7, '/media/c'),
      'POST',
      '/api/library/libraries/7/folders',
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
      'listFolders, at the prefixes',
      () => listFolders(),
      'GET',
      '/api/library/folders',
      undefined,
    ],
    [
      'listFolders, at a path',
      () => listFolders('/mnt/user/media 1/Manga & more'),
      'GET',
      '/api/library/folders?path=%2Fmnt%2Fuser%2Fmedia%201%2FManga%20%26%20more',
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

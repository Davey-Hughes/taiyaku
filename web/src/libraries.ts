/**
 * The `/api/library/libraries*` and folder-browser client: Configuration →
 * Libraries.
 *
 * **A library that exists is addressed by its id.** A name is sent only to
 * make one or to rename one.
 *
 * **A folder's own routes are still `config.ts`'s.** A library's card holds a
 * `RootRow` per folder, and that row patches, scans and removes its folder
 * through `/api/library/roots/{id}`. What a library does whole is here.
 */
import { getJson, sendJson } from './api'
import type { Added, RemovalAnswer, Root } from './config'

/** One library, as `GET /api/library/libraries` sends it. */
export interface Library {
  id: number
  name: string
  /** The works filed under it. */
  works: number
  /** EPUB and PDF entries its scans recorded, which nothing reads yet. */
  books: number
  /** Its folders, oldest first. */
  folders: Root[]
}

/** `POST /api/library/libraries`'s answer: the library, and each folder's probe. */
export interface Created {
  library: Library
  added: Added[]
}

/** `GET /api/library/libraries/{id}/scan`. */
export interface LibraryScanStatus {
  library_id: number
  /** Whether a scan of any of its folders is running. */
  running: boolean
  /** `null` when nothing is running. */
  elapsed_secs: number | null
  folders: { root_id: number; running: boolean }[]
}

/** `GET /api/library/folders`: one directory's subdirectories, or the prefixes. */
export interface FolderListing {
  /** `null` where the prefixes themselves were listed. */
  path: string | null
  /** Where "up" goes. `null` at a prefix. */
  parent: string | null
  folders: { name: string; path: string }[]
}

/** `sendJson` resolves `undefined` for a 204, which no route here answers. */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

export const listLibraries = () =>
  getJson<{ libraries: Library[] }>('/api/library/libraries')

/** A library and its folders: all of them, or a refusal and nothing made. */
export const createLibrary = (
  name: string,
  folders: string[],
): Promise<Created> =>
  expectBody(
    sendJson<Created>('/api/library/libraries', 'POST', { name, folders }),
  )

/** Answers the name as stored, which is the name sent with its spaces stripped. */
export const renameLibrary = (id: number, name: string) =>
  expectBody(
    sendJson<{ id: number; name: string }>(
      `/api/library/libraries/${id}`,
      'PATCH',
      { name },
    ),
  )

export const addFolder = (id: number, path: string): Promise<Added> =>
  expectBody(
    sendJson<Added>(`/api/library/libraries/${id}/folders`, 'POST', { path }),
  )

/** What removing library `id` would cost. `?dry_run=true`, spelt exactly. */
export const libraryRemovalCost = (id: number): Promise<RemovalAnswer> =>
  expectBody(
    sendJson<RemovalAnswer>(
      `/api/library/libraries/${id}?dry_run=true`,
      'DELETE',
    ),
  )

export const removeLibrary = (id: number): Promise<RemovalAnswer> =>
  expectBody(sendJson<RemovalAnswer>(`/api/library/libraries/${id}`, 'DELETE'))

/**
 * Start a scan of every folder of library `id`. Answers 409 while a scan of
 * any one of them is running.
 */
export const startLibraryScan = (
  id: number,
): Promise<{ library_id: number; status_url: string }> =>
  expectBody(
    sendJson<{ library_id: number; status_url: string }>(
      `/api/library/libraries/${id}/scan`,
      'POST',
    ),
  )

export const libraryScanStatus = (id: number) =>
  getJson<LibraryScanStatus>(`/api/library/libraries/${id}/scan`)

/** The subdirectories of `path`, or the prefixes when it is left out. */
export const listFolders = (path?: string) =>
  getJson<FolderListing>(
    path === undefined
      ? '/api/library/folders'
      : `/api/library/folders?path=${encodeURIComponent(path)}`,
  )

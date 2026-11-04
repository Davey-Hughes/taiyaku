/**
 * The `/api/library/libraries*` and directory-browser client: Configuration →
 * Libraries.
 *
 * **A library that exists is addressed by its id.** A name is sent only to
 * make one or to rename one.
 *
 * **A directory's own routes are `config.ts`'s.** A library's card holds a
 * `RootRow` per directory, and that row patches, scans and removes its
 * directory through `/api/library/roots/{id}`. What a library does whole is
 * here, and so is moving a directory from one library to another.
 */
import { getJson, sendJson } from './api'
import type { Added, RemovalAnswer, Root } from './config'

/** One library, as `GET /api/library/libraries` sends it. */
export interface Library {
  id: number
  name: string
  /** The works with a file under its directories. */
  works: number
  /** EPUB and PDF entries its scans recorded, which nothing reads yet. */
  books: number
  /** Its directories, oldest first. */
  dirs: Root[]
}

/** `POST /api/library/libraries`'s answer: the library, and each directory's probe. */
export interface Created {
  library: Library
  added: Added[]
}

/** `GET /api/library/libraries/{id}/scan`. */
export interface LibraryScanStatus {
  library_id: number
  /** Whether a scan of any of its directories is running. */
  running: boolean
  /** `null` when nothing is running. */
  elapsed_secs: number | null
  dirs: { root_id: number; running: boolean }[]
}

/** `GET /api/library/dirs`: one directory's subdirectories, or the prefixes. */
export interface DirListing {
  /** `null` where the prefixes themselves were listed. */
  path: string | null
  /** Where "up" goes. `null` at a prefix. */
  parent: string | null
  dirs: { name: string; path: string }[]
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

/** A library and its directories: all of them, or a refusal and nothing made. */
export const createLibrary = (name: string, dirs: string[]): Promise<Created> =>
  expectBody(
    sendJson<Created>('/api/library/libraries', 'POST', { name, dirs }),
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

export const addDir = (id: number, path: string): Promise<Added> =>
  expectBody(
    sendJson<Added>(`/api/library/libraries/${id}/dirs`, 'POST', { path }),
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
 * Start a scan of every directory of library `id`. Answers 409 while a scan of
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
export const listDirs = (path?: string) =>
  getJson<DirListing>(
    path === undefined
      ? '/api/library/dirs'
      : `/api/library/dirs?path=${encodeURIComponent(path)}`,
  )

/**
 * How long a listing is read before the directory browser says so. A listing
 * answered sooner is itself the answer to the press, and a line drawn and
 * taken away faster than it can be read is a flicker; 400 ms is inside the
 * span where a reply still reads as the press's own.
 */
export const READING_DELAY_MS = 400

/** Where a directory moves to: a library that exists, or one made by the move. */
export type MoveTarget = { library: number } | { new_library: string }

/** What a move did or, rehearsed, would do. */
export interface Moved {
  /** The library the directory is in after the move. */
  library_id: number
  /** That library's name. */
  library: string
  /**
   * The accounts, by name and in name order, that see the directory after the
   * move and not before. An administrator and an account that sees every
   * library see it either way, so neither is named.
   */
  gains_sight: string[]
  /** The accounts that see it before the move and not after, named the same way. */
  loses_sight: string[]
  /** The name of the directory's old library, where the move empties and removes it. */
  library_removed: string | null
  /** Grants that went with that library. */
  grants_removed: number
  /**
   * The accounts, by name, whose only grant was that library, and who see no
   * library after the move. Empty where no library is removed.
   */
  left_with_no_library: string[]
}

/** `names` as a sentence lists them: `a`, `a and b`, `a, b and c`. */
export function namesOf(names: string[]): string {
  return names.length < 2
    ? names.join('')
    : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/**
 * What a removal's or a move's rehearsal says of the accounts it leaves
 * seeing no library, under its counts. `what` is the library as the dialog
 * names it, `"this library"` or `"this directory's library"`.
 *
 * One account is named again in the second sentence rather than given a
 * pronoun, so nothing is assumed of whoever holds it.
 */
export function noLibrarySentence(names: string[], what: string): string {
  const who = namesOf(names)
  const after = names.length < 2 ? who : 'they'
  return `${who} can see only ${what}. After this, ${after} will see no library until an administrator grants one.`
}

/** `POST /api/library/roots/{id}/move`'s answer, a rehearsal's or a move's. */
export interface MoveAnswer {
  moved: Moved
  dry_run: boolean
}

/**
 * What moving directory `rootId` to `to` would do, without doing it. A move
 * that would be refused is refused here in the same words.
 */
export const moveCost = (rootId: number, to: MoveTarget): Promise<MoveAnswer> =>
  expectBody(
    sendJson<MoveAnswer>(
      `/api/library/roots/${rootId}/move?dry_run=true`,
      'POST',
      to,
    ),
  )

/** Moves directory `rootId` to `to`. */
export const moveDir = (rootId: number, to: MoveTarget): Promise<MoveAnswer> =>
  expectBody(
    sendJson<MoveAnswer>(`/api/library/roots/${rootId}/move`, 'POST', to),
  )

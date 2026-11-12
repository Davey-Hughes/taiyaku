/**
 * The `/api/library/libraries*` and directory-browser client: Configuration →
 * Libraries.
 *
 * **A library that exists is addressed by its id.** A name is sent only to
 * make one or to rename one.
 *
 * **A directory's own routes are `config.ts`'s.** A directory's page draws
 * its `RootRow`, which patches, scans and removes the directory through
 * `/api/library/roots/{id}`. What a library does whole is here, and so is
 * moving a directory from one library to another, and taking one out of a
 * library alone (`unlinkCost`, `unlinkDir`), which another library keeps.
 */
import { getJson, sendJson } from './api'
import type { Added, RemovalAnswer, Root } from './config'
import { count } from './count'

/** One library, as `GET /api/library/libraries` sends it. */
export interface Library {
  id: number
  name: string
  /** The works with a file under a directory it shows. */
  works: number
  /** EPUB and PDF entries its scans recorded, which nothing reads yet. */
  books: number
  /** Its directories, oldest first. */
  dirs: LibraryDir[]
}

/**
 * One of a library's directories, with what the server says of the
 * directories inside and around it, each by path, outermost first. Whether a
 * library shows a directory's books is the server's to say: it turns on
 * patterns this client does not compile (C4 screens spec §3.2, §8).
 */
export interface LibraryDir extends Root {
  /**
   * Every library holding it, ascending by id, this one included: two
   * libraries may hold one directory, which shares its settings and scans.
   */
  libraries: { id: number; name: string }[]
  /** Directories inside this one, whose settings govern their own files. */
  nested: NestedDir[]
  /** Directories this one sits in. */
  inside: AroundDir[]
}

/** A directory inside one of a library's. */
export interface NestedDir {
  /** How the library's scan status names it, since a scan of the library walks it. */
  id: number
  path: string
  /** Every library holding it, by name, ascending by id. */
  libraries: string[]
  /** Whether the outer directory's library shows its books. */
  shown: boolean
  /** Why it does not, `null` where it does. */
  kept_out_by: KeptOutBy | null
}

/**
 * Why a library does not show a directory nested in one of its own: one of
 * that directory's exclude patterns, or a name on the way starting with `.`,
 * which every walk skips.
 */
export type KeptOutBy = 'exclusion' | 'dot_name'

/**
 * `inner`'s path from `outer`, which the server says holds it, as a folder:
 * `kids/`. Its whole path where it does not start with `outer`'s.
 */
export function folderIn(inner: string, outer: string): string {
  const from = outer.endsWith('/') ? outer : `${outer}/`
  return `${inner.startsWith(from) ? inner.slice(from.length) : inner}/`
}

/**
 * How a directory left a library: unlinked from it alone, which another
 * library keeps, or removed from every library holding it.
 */
export type DirRemoval = 'unlinked' | 'removed'

/**
 * `held` with the names on each line, inside or around any directory, of a
 * directory whose path `written` picks, as `names` rewrites them. A line left
 * naming no library goes, as no library holds that directory any more. The
 * fold and "Also shown in …" lines and the Overlaps cell are drawn from these
 * lines (O252).
 */
function withLinesOf(
  held: Library[],
  written: (path: string) => boolean,
  names: (libraries: string[]) => string[],
): Library[] {
  const relined = <T extends { path: string; libraries: string[] }>(
    lines: T[],
  ) =>
    lines.flatMap((line) => {
      if (!written(line.path)) return [line]
      const libraries = names(line.libraries)
      return libraries.length > 0 ? [{ ...line, libraries }] : []
    })
  return held.map((one) => ({
    ...one,
    dirs: one.dirs.map((dir) => ({
      ...dir,
      nested: relined(dir.nested),
      inside: relined(dir.inside),
    })),
  }))
}

/**
 * `held` once directory `path` has left library `id` as `how` says, without
 * each library the answer names as `emptied`: the server deletes a library
 * with its last directory. After an unlink, each library still holding the
 * directory no longer names `id` among its holders, nor does any line
 * another directory draws for it; after a removal, no such line is left. An
 * unlink empties no library but `id`.
 */
export function withoutDir(
  held: Library[],
  id: number,
  path: string,
  how: DirRemoval,
  emptied: string[],
): Library[] {
  // Read before `emptied` is taken out: an unlink can empty `id` itself.
  const left = held.find((one) => one.id === id)?.name
  const kept = held
    .filter((one) => !emptied.includes(one.name))
    .map((one) =>
      how === 'removed' || one.id === id
        ? { ...one, dirs: one.dirs.filter((dir) => dir.path !== path) }
        : {
            ...one,
            dirs: one.dirs.map((dir) =>
              dir.path === path
                ? {
                    ...dir,
                    // Its page draws "Also in", the Removal rows and
                    // Move's list from these.
                    libraries: dir.libraries.filter((one) => one.id !== id),
                  }
                : dir,
            ),
          },
    )
  return withLinesOf(
    kept,
    (at) => at === path,
    (names) => (how === 'removed' ? [] : names.filter((name) => name !== left)),
  )
}

/**
 * `held` without library `id`, which a removal took with every directory it
 * alone held: each directory it shared stays in the libraries that keep it,
 * whose copies no longer name it among its holders, and the lines of the
 * directories inside and around one no longer name it either, nor a
 * directory that it alone held.
 */
export function withoutLibrary(held: Library[], id: number): Library[] {
  const name = held.find((one) => one.id === id)?.name
  return withLinesOf(
    held
      .filter((one) => one.id !== id)
      .map((one) => ({
        ...one,
        dirs: one.dirs.map((dir) => ({
          ...dir,
          libraries: dir.libraries.filter((holder) => holder.id !== id),
        })),
      })),
    () => true,
    (names) => names.filter((one) => one !== name),
  )
}

/**
 * `held` with library `id` called `name`, wherever it is named: as a
 * library, among a directory's holders, and on the lines of the directories
 * inside and around one.
 */
export function withLibraryRenamed(
  held: Library[],
  id: number,
  name: string,
): Library[] {
  const from = held.find((one) => one.id === id)?.name
  const renamed = <T extends { libraries: string[] }>(lines: T[]) =>
    lines.map((line) => ({
      ...line,
      libraries: line.libraries.map((one) => (one === from ? name : one)),
    }))
  return held.map((one) => ({
    ...one,
    name: one.id === id ? name : one.name,
    dirs: one.dirs.map((dir) => ({
      ...dir,
      libraries: dir.libraries.map((holder) =>
        holder.id === id ? { ...holder, name } : holder,
      ),
      nested: renamed(dir.nested),
      inside: renamed(dir.inside),
    })),
  }))
}

/**
 * `held` with library `made` last, as the make answered it: each other
 * library's copy of a directory it linked names it among its holders, in
 * the server's order, by id, and so does each line another directory draws
 * for one. Library ids only grow, so a made library's is the largest and
 * its name goes last on a line.
 */
export function withLibraryMade(held: Library[], made: Library): Library[] {
  const linked = new Set(made.dirs.map((dir) => dir.path))
  const me = { id: made.id, name: made.name }
  const others = held
    .filter((one) => one.id !== made.id)
    .map((one) => ({
      ...one,
      dirs: one.dirs.map((dir) =>
        linked.has(dir.path)
          ? {
              ...dir,
              libraries: [
                ...dir.libraries.filter((holder) => holder.id !== made.id),
                me,
              ].sort((a, b) => a.id - b.id),
            }
          : dir,
      ),
    }))
  return [
    ...withLinesOf(
      others,
      (at) => linked.has(at),
      (names) => [...names.filter((name) => name !== made.name), made.name],
    ),
    made,
  ]
}

/** A directory one of a library's sits in. */
export interface AroundDir {
  path: string
  /** Every library holding it, by name, ascending by id. */
  libraries: string[]
  /** Whether those libraries show the inner directory's books. */
  shows: boolean
  /** Why it does not, `null` where it does. */
  kept_out_by: KeptOutBy | null
}

/** `POST /api/library/libraries`'s answer: the library, and each directory's probe. */
export interface Created {
  library: Library
  added: Added[]
}

/** `GET /api/library/libraries/{id}/scan`. */
export interface LibraryScanStatus {
  library_id: number
  /** Whether a scan of any directory a scan of it walks is running. */
  running: boolean
  /** `null` when nothing is running. */
  elapsed_secs: number | null
  dirs: { root_id: number; running: boolean }[]
  /**
   * Its own directories whose removal waits for a scan of a directory around
   * or inside them, of any library, by id. Each one's own scan is not here.
   */
  held_removals: number[]
  /**
   * The running directories by the library whose request reserved them, by
   * its id, which a page compares with its own, and the name it has now,
   * which a page shows; both `null` where none did, or where that library no
   * longer reaches the directory. A scan another library asked for walks
   * directories this one shows.
   */
  scans: {
    library_id: number | null
    library: string | null
    root_ids: number[]
  }[]
  /**
   * Whether a library job holds the server's job lock (the serve crate's
   * `LibraryJob` names them), for which it refuses every move and every
   * removal from a library.
   */
  moves_held: boolean
  /**
   * Whether a derive holds the store's derive lease, in this server or
   * another process, for which it refuses every removal (O51).
   */
  derive_running: boolean
}

/**
 * Why the server refuses a removal now: a scan of a directory around or
 * inside it (`around`), or a derive holding the lease (`derive`). Where both
 * hold, `around` is the one said, as the more specific.
 */
export type RemovalHold = 'around' | 'derive' | undefined

export const removalHold = (around: boolean, derive: boolean): RemovalHold =>
  around ? 'around' : derive ? 'derive' : undefined

/** `GET /api/library/derive`. */
export interface DeriveStatus {
  /** A derive holds the derive lease, in this server or another process. */
  derive_running: boolean
  /** A library job holds the server's job lock (the serve crate's `LibraryJob`). */
  job_held: boolean
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

export const deriveStatus = () => getJson<DeriveStatus>('/api/library/derive')

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

/** `POST /api/library/libraries/{id}/scan`'s answer. */
export interface LibraryScanStarted {
  library_id: number
  status_url: string
  /** The directories the scan reserved, in the order it walks them. */
  root_ids: number[]
}

/**
 * Start a scan of every directory library `id` shows. Answers 409 while a scan
 * of any one of them is running.
 */
export const startLibraryScan = (id: number): Promise<LibraryScanStarted> =>
  expectBody(
    sendJson<LibraryScanStarted>(`/api/library/libraries/${id}/scan`, 'POST'),
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

/**
 * `held` once directory `path` has moved out of library `id` as `moved`
 * says: gone from `id`, which goes too where the move emptied it. Each copy
 * that stays, and each line another directory draws for it, names the
 * library it went to in place of `id` among its holders, in the server's
 * order, by id. No row is drawn in the library it went to: the read that
 * follows draws that, as it draws a row the client cannot know.
 */
export function withMovedDir(
  held: Library[],
  id: number,
  path: string,
  moved: Moved,
): Library[] {
  const to = { id: moved.library_id, name: moved.library }
  const holders = (libraries: LibraryDir['libraries']) =>
    [...libraries.filter((one) => one.id !== id && one.id !== to.id), to].sort(
      (a, b) => a.id - b.id,
    )
  const kept = held
    .filter((one) => !(one.id === id && moved.library_removed !== null))
    .map((one) =>
      one.id === id
        ? { ...one, dirs: one.dirs.filter((dir) => dir.path !== path) }
        : {
            ...one,
            dirs: one.dirs.map((dir) =>
              dir.path === path
                ? { ...dir, libraries: holders(dir.libraries) }
                : dir,
            ),
          },
    )
  // A line carries names alone, so the ids that order it come from the
  // directory's own copy, which names every holder with its id, a library a
  // move made among them; `held` has no row for that library until the list
  // is read again. Where that copy is gone, a read that came after the move
  // has drawn the lines already.
  const own = held
    .find((one) => one.id === id)
    ?.dirs.find((dir) => dir.path === path)
  return withLinesOf(
    kept,
    (at) => at === path,
    (names) =>
      own === undefined ? names : holders(own.libraries).map((one) => one.name),
  )
}

/**
 * What a move, an unlink or their rehearsal says of the library it empties,
 * which the server removes with its grants: `undefined` where it empties
 * none. `past` says it as done. One sentence for all of them, so that what a
 * rehearsal says and what the report of the write says cannot drift apart.
 */
export function libraryRemovedSentence(
  left: Pick<Moved, 'library_removed' | 'grants_removed'>,
  past: boolean,
): string | undefined {
  const { library_removed: name, grants_removed: grants } = left
  if (name === null) return undefined
  const grantsToo =
    grants > 0 ? `, with ${count(grants, 'grant', 'grants')}` : ''
  return `${name} ${past ? 'had' : 'has'} no other directory, so it ${past ? 'was' : 'is'} removed${grantsToo}.`
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
 * What moving directory `rootId` out of library `from` to `to` would do,
 * without doing it. A move that would be refused is refused here in the same
 * words.
 *
 * **`from` is the library whose page asks**, and the server has no default
 * for it: a directory several libraries hold leaves only the one named, and
 * a page older than a link would otherwise move it out of another.
 */
export const moveCost = (
  rootId: number,
  from: number,
  to: MoveTarget,
): Promise<MoveAnswer> =>
  expectBody(
    sendJson<MoveAnswer>(
      `/api/library/roots/${rootId}/move?dry_run=true`,
      'POST',
      { from, ...to },
    ),
  )

/** Moves directory `rootId` out of library `from` to `to`. */
export const moveDir = (
  rootId: number,
  from: number,
  to: MoveTarget,
): Promise<MoveAnswer> =>
  expectBody(
    sendJson<MoveAnswer>(`/api/library/roots/${rootId}/move`, 'POST', {
      from,
      ...to,
    }),
  )

/**
 * What taking a directory out of one library did or, rehearsed, would do. The
 * server sends a move's names for what the two share: an unlink is a move's
 * leaving half, and no account gains sight of anything.
 */
export type Unlinked = Pick<
  Moved,
  'loses_sight' | 'library_removed' | 'grants_removed' | 'left_with_no_library'
>

/** `DELETE /api/library/libraries/{id}/dirs/{dir}`'s answer, a rehearsal's or an unlink's. */
export interface UnlinkAnswer {
  unlinked: Unlinked
  dry_run: boolean
}

/**
 * What taking directory `rootId` out of library `id` alone would do, without
 * doing it. The server refuses it on the directory's last library, which
 * only removing the directory takes it out of. `?dry_run=true`, spelt
 * exactly.
 */
export const unlinkCost = (id: number, rootId: number): Promise<UnlinkAnswer> =>
  expectBody(
    sendJson<UnlinkAnswer>(
      `/api/library/libraries/${id}/dirs/${rootId}?dry_run=true`,
      'DELETE',
    ),
  )

/** Takes directory `rootId` out of library `id`, which another library keeps. */
export const unlinkDir = (id: number, rootId: number): Promise<UnlinkAnswer> =>
  expectBody(
    sendJson<UnlinkAnswer>(
      `/api/library/libraries/${id}/dirs/${rootId}`,
      'DELETE',
    ),
  )

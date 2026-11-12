// The fixtures the Libraries screen's suites share: a directory, a library of
// two directories, its scan status, a removal's dry run, and a poll's turn.
import { act } from 'react'
import { vi } from 'vitest'
import { type RemovalAnswer, SCAN_POLL_MS } from '../../src/config'
import type {
  Library,
  LibraryDir,
  LibraryScanStatus,
} from '../../src/libraries'
import { emptyDerive } from './derive'

/**
 * A directory with only its path and a daily scan set, held by `libraries`:
 * `manga()`'s library alone unless they are named.
 */
export const dir = (
  id: number,
  path: string,
  libraries: LibraryDir['libraries'] = [{ id: 7, name: 'Manga' }],
): LibraryDir => ({
  id,
  path,
  edition_key: null,
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  next_scan_at: null,
  libraries,
  nested: [],
  inside: [],
})

/** Manga, library 7, of directories 1 (Volumes) and 2 (Chapters). */
export const manga = (over: Partial<Library> = {}): Library => ({
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  dirs: [dir(1, '/media/Manga/Volumes'), dir(2, '/media/Manga/Chapters')],
  ...over,
})

/** The status of library `id` while nothing in it is scanning. */
export const idle = (id: number): LibraryScanStatus => ({
  library_id: id,
  held_removals: [],
  moves_held: false,
  derive_running: false,
  scans: [],
  running: false,
  elapsed_secs: null,
  dirs: [],
})

/**
 * The status of `library` while the directories named by id are scanning. It
 * names each of the library's directories, scanning or not, as the server's
 * does, so a reading that ignores `running` is seen.
 */
export const scanningIn = (
  library: Pick<Library, 'id' | 'dirs'>,
  ...ids: number[]
): LibraryScanStatus => ({
  library_id: library.id,
  held_removals: [],
  moves_held: false,
  derive_running: false,
  scans: [],
  running: true,
  elapsed_secs: 3,
  dirs: library.dirs.map((one) => ({
    root_id: one.id,
    running: ids.includes(one.id),
  })),
})

/** A removal's dry run that would take `works` works. */
export const counted = (works = 5): RemovalAnswer => ({
  removed: {
    works,
    positions: 2,
    notes: 0,
    left_with_no_library: [],
    libraries_removed: [],
    kept: [],
  },
  derive: emptyDerive,
  dry_run: true,
})

/** One poll's interval, under fake timers, and what it started settles. */
export const tick = () =>
  act(async () => {
    vi.advanceTimersByTime(SCAN_POLL_MS)
    await Promise.resolve()
    await Promise.resolve()
  })

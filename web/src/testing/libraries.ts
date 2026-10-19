// The fixtures the Libraries screen's suites share: a folder, a library of
// two folders, its scan status, a removal's dry run, and a poll's turn.
import { act } from 'react'
import { vi } from 'vitest'
import { type RemovalAnswer, type Root, SCAN_POLL_MS } from '../config'
import type { Library, LibraryScanStatus } from '../libraries'
import { emptyDerive } from './derive'

/** A folder with only its path and a daily scan set. */
export const folder = (id: number, path: string): Root => ({
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
})

/** Manga, library 7, of folders 1 (Volumes) and 2 (Chapters). */
export const manga = (over: Partial<Library> = {}): Library => ({
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  folders: [
    folder(1, '/media/Manga/Volumes'),
    folder(2, '/media/Manga/Chapters'),
  ],
  ...over,
})

/** The status of library `id` while nothing in it is scanning. */
export const idle = (id: number): LibraryScanStatus => ({
  library_id: id,
  running: false,
  elapsed_secs: null,
  folders: [],
})

/**
 * The status of `library` while the folders named by id are scanning. It
 * names each of the library's folders, scanning or not, as the server's
 * does, so a reading that ignores `running` is seen.
 */
export const scanningIn = (
  library: Pick<Library, 'id' | 'folders'>,
  ...ids: number[]
): LibraryScanStatus => ({
  library_id: library.id,
  running: true,
  elapsed_secs: 3,
  folders: library.folders.map((one) => ({
    root_id: one.id,
    running: ids.includes(one.id),
  })),
})

/** A removal's dry run that would take `works` works. */
export const counted = (works = 5): RemovalAnswer => ({
  removed: { works, positions: 2, notes: 0, left_with_no_library: [] },
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

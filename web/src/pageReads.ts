/**
 * Every `Read` the pages draw from, each named once, so that two screens
 * reading one address share its entry in `reads.ts`. The address is the one
 * its client function sends, query included.
 *
 * **Each wraps its client call in an arrow**, `() => listLibraries()`, never
 * `listLibraries` itself. The arrow looks the function up in its module when
 * the store reads, not when this module is first evaluated, so whatever a
 * test's `vi.mock` of the client module puts there is the function called.
 * The client function is also what turns a 401 into a sign-out and a failure
 * into the sentence the screen draws, which is why the store calls it rather
 * than sending the `GET` itself.
 */
import { about } from './about'
import { listKeys, listSessions, progressSaved } from './api'
import { listBackups } from './backup'
import { listRoots } from './config'
import { listProposals } from './curation'
import { libraryScanStatus, listLibraries } from './libraries'
import { grid, home, narrowedTo, oneSeries, search, status } from './library'
import { invalidate, type Read, read, readsUnder } from './reads'
import { listRules } from './rules'
import { listSplitEditions } from './splitEditions'
import { listUsers } from './users'

/** Every library and its directories: Configuration → Libraries' list. */
export const LIBRARIES = read('/api/library/libraries', () => listLibraries())

/** The libraries this account may read, which the sidebar lists. */
export const LIBRARY_STATUS = read('/api/library/status', () => status())

/**
 * Whether a scan of library `id`, or of one of its directories, is running:
 * what a library's page draws in its first body, and what its poll reads.
 */
export const libraryScan = (id: number) =>
  read(`/api/library/libraries/${id}/scan`, () => libraryScanStatus(id))

// The four reads of the Library views each wait for the progress saves sent
// before them: a read sent as the reader closes would otherwise answer with
// the position from before its last page turn, and since a progress save
// marks nothing stale, the view drawn from that answer would not be read
// again to correct it.

/** A library's series grid, or every library's: the Library view's grid. */
export const gridRead = (library: number | undefined) =>
  read(narrowedTo('/api/library/series', library), async () => {
    await progressSaved()
    return grid(library)
  })

/** A library's Continue and Recently added lists, or every library's. */
export const homeRead = (library: number | undefined) =>
  read(narrowedTo('/api/library/home', library), async () => {
    await progressSaved()
    return home(library)
  })

/**
 * Series `id` and its works, or only what `library` holds of them: a series
 * page's body. The two are separate entries, as their addresses are.
 */
export const seriesRead = (id: number, library?: number) =>
  read(narrowedTo(`/api/library/series/${id}`, library), async () => {
    await progressSaved()
    return oneSeries(id, library)
  })

/** The hits for `q` in a library, or in every library: search's body. */
export const searchRead = (q: string, library: number | undefined) =>
  read(
    narrowedTo(`/api/library/search?q=${encodeURIComponent(q)}`, library),
    async () => {
      await progressSaved()
      return search(q, library)
    },
  )

/**
 * The prefixes of every address the four reads above name, for
 * `readsUnder`. `/api/library/series` covers a series page as well as
 * the grid. No cover is here: a cover is an image the browser keeps by its
 * versioned address, not an entry of the store.
 */
export const LIBRARY_VIEWS: readonly string[] = [
  '/api/library/series',
  '/api/library/home',
  '/api/library/search',
]

/**
 * Every Library view read kept, and the split editions with them: every write
 * that changes which works a library's views hold changes which editions are
 * split as well. A list, so that a scan's end can send them in one set with
 * the other reads it changes.
 */
export function libraryViewReads(): Read<unknown>[] {
  return [...readsUnder(...LIBRARY_VIEWS), SPLIT_EDITIONS]
}

/**
 * Marks every Library view read kept stale, and the split editions, and reads
 * now the ones drawn as one set, so a page drawn from a view and the split
 * editions moves once where no read overtakes one. Resolves as `invalidate`
 * does.
 */
export function invalidateLibraryViews(): Promise<void> {
  return invalidate(...libraryViewReads())
}

/**
 * Every account, with what it may do and what it sees: Configuration →
 * Users' list, and what an account's page is drawn from.
 */
export const USERS = read('/api/library/users', () => listUsers())

/** The backups the server holds: Configuration → Backup's list. */
export const BACKUPS = read('/api/library/backups', () => listBackups())

/**
 * The editions split between libraries: Configuration → Split editions'
 * list. Each opening of the tab asks again, as `usePage` reads an address on
 * every arrival; `invalidateLibraryViews` marks it, so an opening after a
 * scan waits for the new list rather than drawing the one from before.
 */
export const SPLIT_EDITIONS = read('/api/library/split-editions', () =>
  listSplitEditions(),
)

/** The rule list as stored: what Configuration → Rules edits. */
export const RULES = read('/api/library/rules', () => listRules())

/** Every directory a rule may be scoped to: Rules' choice of root. */
export const ROOTS = read('/api/library/roots', () => listRoots())

/**
 * The accepted proposals, each naming the rule it wrote: what marks a rule
 * on Rules as made from a proposal. A rules write changes which rule each
 * names, or reopens it.
 */
export const ACCEPTED_PROPOSALS = read(
  '/api/library/proposals?state=accepted',
  () => listProposals({ state: 'accepted' }),
)

/** This account's API keys: `/account`'s and the reader panel's key list. */
export const KEYS = read('/api/keys', () => listKeys())

/** Where this account is signed in: their Signed in list. */
export const SESSIONS = read('/api/sessions', () => listSessions())

/**
 * The build's facts, the catalog's path and the formats: what About draws,
 * the sidebar's build label names, and `/account` takes the catalog from.
 */
export const ABOUT = read('/api/about', () => about())

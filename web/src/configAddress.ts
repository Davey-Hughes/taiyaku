/**
 * Configuration's addresses for things with names: a library's page and an
 * account's are each at its name.
 *
 * **The exact name, escaped only where an address needs it.** A segment is
 * `encodeURIComponent` of the name, so a name holding `/`, `?`, `#` or `%`
 * stays one segment of the path and is read back whole, and the browser draws
 * a space as a space. No slug: two names a slug would make one would be one
 * address.
 *
 * **A segment is read from the address as the browser holds it**, not from
 * the router's params. The router decodes the path and then turns each `%2F`
 * back into `/`, so a name holding the text `%2F` would be read as another
 * name.
 *
 * **A name is found without joining two the store keeps apart.** Library
 * names are unique under SQLite's `COLLATE NOCASE` (migration 016), which
 * folds ASCII case only;
 * account names are unique as written (`001_initial.sql`), so `Sam` and `sam`
 * can be two accounts. The exact name is looked for first, then the name that
 * equals it with ASCII case folded, where only one does: an address that
 * folds to two accounts names neither. `toLowerCase` is not that fold: it
 * would make `É` and `é`, which can be two libraries, one name.
 *
 * **The pages that make a thing are outside the names' space**, so that no
 * name is reserved: `/config/libraries/new` is the library named "new", and
 * `/config/users/new` the account. The server refuses `.` and `..` as names,
 * since the router and the URL parser collapse either segment, `%2E` too, so
 * no address could hold it, and an empty account name, whose address would
 * be the table's.
 */

/** The libraries table: Configuration → Libraries. */
export const LIBRARIES = '/config/libraries'

/** The form that makes a library. */
export const NEW_LIBRARY = '/config/new-library'

/** The accounts table: Configuration → Users. */
export const USERS = '/config/users'

/** The form that makes an account. */
export const NEW_ACCOUNT = '/config/new-account'

/** The segment of an address that names `name`. */
export const nameSegment = (name: string): string => encodeURIComponent(name)

/** A library's page, at its name. */
export const libraryPath = (name: string): string =>
  `${LIBRARIES}/${nameSegment(name)}`

/** The page of a library's directory, which keeps its number. */
export const dirPath = (name: string, dir: number): string =>
  `${libraryPath(name)}/dirs/${dir}`

/** An account's page, at its name. */
export const accountPath = (name: string): string =>
  `${USERS}/${nameSegment(name)}`

/**
 * The name the segment after `base` in `pathname` spells, and the rest of
 * the path after that segment; `undefined` where there is no such segment.
 * `base` is matched folding case, as the router matches it. A segment that
 * is no valid escape, such as a bare `%` typed into the address bar, is the
 * name as written, which is how the router reads it too.
 */
export function nameAt(
  pathname: string,
  base: string,
): { name: string; rest: string } | undefined {
  const head = `${base}/`
  if (pathname.slice(0, head.length).toLowerCase() !== head.toLowerCase()) {
    return undefined
  }
  const after = pathname.slice(head.length)
  const cut = after.indexOf('/')
  const segment = cut === -1 ? after : after.slice(0, cut)
  if (segment === '') return undefined
  const rest = cut === -1 ? '' : after.slice(cut)
  try {
    return { name: decodeURIComponent(segment), rest }
  } catch {
    return { name: segment, rest }
  }
}

/** `text` with A–Z folded to a–z and every other character as it is. */
const foldAscii = (text: string): string =>
  text.replace(/[A-Z]/g, (letter) => letter.toLowerCase())

/**
 * The one of `items` that `name` names: the one whose name it is exactly,
 * else the one whose name it is with ASCII case folded, where only one is.
 */
export function byName<T extends { name: string }>(
  items: readonly T[],
  name: string,
): T | undefined {
  const exact = items.find((one) => one.name === name)
  if (exact !== undefined) return exact
  const folded = foldAscii(name)
  const matches = items.filter((one) => foldAscii(one.name) === folded)
  return matches.length === 1 ? matches[0] : undefined
}

/**
 * The server's sentence for a library or an account given `.` or `..` as a
 * name (`NO_ADDRESS`, `crates/taiyaku-library/src/lib.rs`).
 */
export const NO_ADDRESS = 'An address cannot hold “.” or “..” as a name.'

/** Whether `name` is exactly `.` or `..`, which no address can hold. */
const hasNoAddress = (name: string): boolean => name === '.' || name === '..'

/**
 * `name` as the store keeps a library's name (`trim_name`): stripped at both
 * ends of the characters Unicode calls White_Space. Not `trim()`, which
 * strips U+FEFF, which the store keeps, and keeps U+0085, which it strips.
 */
const libraryNameStored = (name: string): string =>
  name.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '')

/**
 * The server's sentence refusing `name` for a library, as the store judges
 * it, or `undefined` where the name draws no refusal of its own: `.` or `..`
 * once trimmed, or the name of another of `libraries` once trimmed and with
 * ASCII case folded, named as that library holds it. `renaming` is the id
 * of the library being renamed, whose own name in any case is no clash; a
 * make and a rename are refused in different words, which are the server's
 * own (`scan::create_library`, `libraries::rename`).
 */
export function libraryNameClash(
  name: string,
  libraries: readonly { id: number; name: string }[],
  renaming?: number,
): string | undefined {
  const stored = libraryNameStored(name)
  if (hasNoAddress(stored)) return NO_ADDRESS
  const folded = foldAscii(stored)
  const holder = libraries.find(
    (one) => one.id !== renaming && foldAscii(one.name) === folded,
  )
  if (holder === undefined) return undefined
  const taken = `a library is already called \`${holder.name}\``
  return renaming === undefined
    ? `${taken}; add the directory to it instead`
    : taken
}

/**
 * The server's sentence refusing `name` for a new account, or `undefined`
 * where the name draws no refusal of its own: `.` or `..` exactly, or the
 * name of one of `accounts` exactly, since account names are stored as
 * written and compared byte for byte. The second is the store's sentence
 * for a taken account name (`account::taken`), so the two change together.
 */
export function accountNameClash(
  name: string,
  accounts: readonly { name: string }[],
): string | undefined {
  if (hasNoAddress(name)) return NO_ADDRESS
  return accounts.some((one) => one.name === name)
    ? `an account is already called \`${name}\``
    : undefined
}

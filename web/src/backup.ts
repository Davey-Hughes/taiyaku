/**
 * The `/api/library/backups*` client — spec C4 §5 and the build design's §6.3,
 * over the routes that shipped in #106.
 *
 * **There is no restore here and no schedule here**, and neither is an
 * omission: C4 §5 puts restore out of band (stop the server, move the file,
 * start it) and the server half declined an in-server timer. A cron entry runs
 * `taiyaku library backup`, and `--keep` is where retention lives.
 *
 * **The download is a URL rather than a fetch**, for the reason `api.ts` gives
 * for `pageUrl`: it goes straight into an `<a href download>`, the route sends
 * `Content-Disposition: attachment`, and the session cookie rides on the
 * request. Fetching a whole database into memory first would buy nothing and
 * cost the file's size in JS heap.
 */
import { getJson, sendJson } from './api'

/** One backup file, as `GET /api/library/backups` sends it. */
export interface Backup {
  /** The file name alone, never a path — the route resolves it against its own listing. */
  name: string
  bytes: number
  /** The timestamp inside the name, `YYYYMMDDTHHMMSSZ`. */
  taken: string
}

/** See `users.ts` for why each module narrows this itself. */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

/** What exists, newest first — the order the route answers in, not re-sorted here. */
export const listBackups = () =>
  getJson<{ backups: Backup[] }>('/api/library/backups')

/**
 * C4 §5's "run now".
 *
 * Two runs inside one second collide and the second is refused `409` with the
 * server's own sentence — the name's timestamp has one-second resolution. That
 * is timing rather than a fault, and the sentence says so. The screen draws
 * that sentence rather than reading the status, inside the `path: status body`
 * envelope every thrown message in `api.ts` carries.
 */
export async function takeBackup(): Promise<Backup> {
  const body = await expectBody(
    sendJson<{ backup: Backup }>('/api/library/backups', 'POST'),
  )
  return body.backup
}

/** Where one backup is downloaded from. */
export const backupUrl = (name: string) =>
  `/api/library/backups/${encodeURIComponent(name)}`

const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB']

/**
 * A size in the coarsest binary unit that does not lie about it.
 *
 * **KiB rather than kB**, because the divisor is 1024: a label saying kB over
 * a 1024-divisor is a figure that disagrees with `ls -l`. Bytes are drawn
 * whole; everything above takes one decimal, which is the precision an
 * operator deciding whether a file will fit actually reads.
 */
export function humanBytes(bytes: number): string {
  let size = bytes
  let unit = 0
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024
    unit += 1
  }
  return unit === 0 ? `${size} B` : `${size.toFixed(1)} ${UNITS[unit]}`
}

/**
 * The stamp inside a backup's name, spaced out.
 *
 * **Anything that is not `YYYYMMDDTHHMMSSZ` is handed back verbatim.**
 * `backup::stamp_of` is what decides a backup file is one, and it enforces
 * that shape — so reaching the fallback means a server that is not this one,
 * and passing the string through says what the server sent rather than
 * inventing a date from a guess.
 */
export function readableTaken(taken: string): string {
  const found = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(taken)
  if (found === null) return taken
  const [, year, month, day, hour, minute, second] = found
  return `${year}-${month}-${day} ${hour}:${minute}:${second} UTC`
}

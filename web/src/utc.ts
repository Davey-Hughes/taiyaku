/**
 * A time the server wrote, said as UTC.
 *
 * The server stamps sessions, keys and scans with SQLite's `datetime('now')`,
 * which is UTC and names no zone, so the words have to: the stamp, a space and
 * "UTC", the form the Backups list gives a backup's time (`readableTaken`), so
 * that every time Configuration draws is read the same way.
 */
export const inUtc = (stamp: string): string => `${stamp} UTC`

/**
 * The backup screen — spec C4 §5 and the build design's §6.3.
 *
 * **The copy here is load-bearing rather than decorative**, which is why four
 * sentences are on the screen and not in a doc:
 *
 * - **Restore is out of band.** Stop the server, move the file, start it. C4
 *   §5 declines an in-server restore, so there is no button and the screen
 *   says what to do instead of leaving a person hunting for one.
 * - **A backup is the whole database, password hashes included.**
 *   `backup_api`'s own module doc says this is worth naming because "the admin
 *   screen has a download button" does not, on its face, look like "the admin
 *   screen hands out the password file".
 * - **A sidecar export is not a backup.** Five documents per work and none of
 *   the cross-work data — collections, tags, external ids, users, read
 *   history. That distinction is invisible unless something says it.
 * - **There is no schedule here.** A cron entry runs `taiyaku library backup`
 *   and `--keep` is where retention lives; the screen states the command
 *   rather than offering a control the server half deliberately declined.
 *
 * **The failure is never classified by status, and what it carries is the
 * server's own sentence inside `api.ts`'s envelope.** Both refusals this
 * screen can meet are `409`: a second backup inside one second, and a library
 * that is in memory and has no file to back up. A screen that read the status
 * would have to guess which; the sentences say which, and the first of them
 * already names the whole-second timestamp as the timing it is. What is
 * actually drawn is `api.ts`'s thrown message — `` `${path}: ${status}
 * ${body}` ``, with the body still its JSON envelope — so the operator reads
 * `/api/library/backups: 409 {"error":"a backup named …"}`. That wrapping is
 * app-wide rather than this screen's, and O96's entry records it as a note
 * about the reader rather than about any one caller; it is written down here
 * so the claim above is not read as more than it is.
 *
 * **The download is an `<a href download>`**, not a fetch — see `backup.ts`.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  type Backup as StoredBackup,
  backupUrl,
  humanBytes,
  listBackups,
  readableTaken,
  takeBackup,
} from './backup'

export function Backup() {
  const [backups, setBackups] = useState<StoredBackup[] | undefined>()
  /** Why the list could not be read. Separate from `takeFailure` below. */
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * Why the last *take* was refused. Cleared only by the next take.
   *
   * **One slot served both, and the list read wiped the refusal.** A take that
   * succeeds fires a reload, and the natural second press lands inside the
   * same second — which is refused with the sentence this whole screen is
   * written around, and then erased by that first reload's `.then` a moment
   * later, leaving a screen that looks as though nothing happened. `Users.tsx`
   * splits its two slots for exactly this; this screen did not until the
   * second whole-branch review pass.
   */
  const [takeFailure, setTakeFailure] = useState<string | undefined>()
  const [taking, setTaking] = useState(false)
  /** Whether a `POST` is already on its way. See `Users.tsx`'s own note. */
  const inFlight = useRef(false)
  /**
   * The newest `listBackups` call.
   *
   * **Responses do not have to arrive in order**, and two reads are routinely
   * in flight here: the mount's, and the one a finished `POST` fires. Without
   * this, a slow mount read landing after the take's answer put the
   * *pre-backup* list back on screen, so the file that had just been written
   * was missing from the list that reported it. `Roots.tsx` carries the same
   * counter, added by its fourth review pass.
   */
  const latest = useRef(0)

  const reload = useCallback(() => {
    const seq = ++latest.current
    listBackups()
      .then((body) => {
        if (seq !== latest.current) return
        setBackups(body.backups)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const take = () => {
    if (inFlight.current) return
    inFlight.current = true
    setTaking(true)
    setTakeFailure(undefined)
    takeBackup()
      .then(() => {
        inFlight.current = false
        setTaking(false)
        reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setTaking(false)
        setTakeFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <section className="backup">
      <h2>Backup</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {takeFailure !== undefined && <p role="alert">{takeFailure}</p>}

      <button type="button" name="take" onClick={take} disabled={taking}>
        {taking ? 'Backing up…' : 'Back up now'}
      </button>

      {backups !== undefined &&
        (backups.length === 0 ? (
          <p className="empty">
            No backups yet. The button above takes one; it is a
            <code> VACUUM INTO</code> of the live database, so nothing has to
            stop for it.
          </p>
        ) : (
          <ul className="backup-list">
            {backups.map((held) => (
              <li key={held.name}>
                <a href={backupUrl(held.name)} download>
                  {held.name}
                </a>
                <span className="when">{readableTaken(held.taken)}</span>
                <span className="size">{humanBytes(held.bytes)}</span>
              </li>
            ))}
          </ul>
        ))}

      <div className="what-a-backup-is">
        <p>
          <strong>Restore is out of band.</strong> Stop the server, move the
          file into place, start it again. There is no restore button here: a
          restore through a running server would have to quiesce every writer in
          the process, and that is not worth the failure modes it introduces.
        </p>
        <p>
          <strong>A backup is the whole database</strong>, password hashes
          included. Anyone who downloads one can read every account&rsquo;s
          stored hash offline. That is what a backup is — the same file an
          operator would copy off the disk — and it is why every route behind
          this screen is administrator-only.
        </p>
        <p>
          <strong>A sidecar export is not a backup.</strong> An export writes
          five documents per work and carries none of the cross-work data:
          collections, tags, external ids, users, read history. Somebody who
          exported their library still needs this.
        </p>
        <p>
          <strong>There is no schedule here.</strong> Run{' '}
          <code>taiyaku library backup</code> from cron for that, and{' '}
          <code>--keep N</code> to prune old ones. The server takes a backup
          only when asked.
        </p>
      </div>
    </section>
  )
}

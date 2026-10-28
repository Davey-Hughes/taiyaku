/**
 * The backup screen — spec C4 §5 and the build design's §6.3.
 *
 * **The copy here is load-bearing rather than decorative**, which is why four
 * sentences are on the screen and not in a doc. The hashes are the header's
 * line, since they are the one a person must not miss before pressing; the
 * other three are behind "How backups work":
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
import { useRef, useState } from 'react'
import {
  type Backup as StoredBackup,
  backupUrl,
  readableTaken,
  takeBackup,
} from './backup'
import { humanBytes } from './bytes'
import { HeldButton } from './HeldButton'
import { useDrawnWait } from './hooks'
import { messageOf } from './message'
import { BACKUPS } from './pageReads'
import { invalidate } from './reads'
import { PageHeader, PageLoading, Row, Section } from './settingsLayout'
import { usePage } from './usePage'
import { WaitWords } from './WaitWords'
import { JOINING_DOT } from './joiningDot'

/** Back up now's words while a backup is out. */
const BACKING_UP = 'Backing up…'

export function Backup() {
  const {
    ready,
    answers: [list],
    failures: [listFailure],
    loadingDrawn,
  } = usePage<[{ backups: StoredBackup[] }]>([BACKUPS])
  /** The list, once the page may draw it: with it comes the whole body. */
  const backups = ready ? list?.backups : undefined
  /** Why the list could not be read. Separate from `takeFailure` below. */
  const failure = listFailure === undefined ? undefined : messageOf(listFailure)
  /**
   * Why the last *take* was refused. Cleared only by the next take, so the
   * list read that follows a take cannot take the refusal with it: the
   * natural second press lands inside the same second, and its refusal is
   * the sentence this whole screen is written around. Users keeps the same
   * pair apart: the list's failure in `Users.tsx`, a form's refusal in the
   * form (`NewAccount.tsx`).
   */
  const [takeFailure, setTakeFailure] = useState<string | undefined>()
  const [taking, setTaking] = useState(false)
  const takingDrawn = useDrawnWait(taking)
  /**
   * Whether a `POST` is already on its way, which turns a second press down.
   * A ref, not `taking`: two presses before a render both read `taking` as
   * false, and Back up now is held only once the wait is drawn, so a press
   * made before then reaches `take`.
   */
  const inFlight = useRef(false)

  const take = () => {
    if (inFlight.current) return
    inFlight.current = true
    setTaking(true)
    setTakeFailure(undefined)
    takeBackup()
      .then(() => {
        inFlight.current = false
        setTaking(false)
        void invalidate(BACKUPS)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setTaking(false)
        setTakeFailure(messageOf(err))
      })
  }

  return (
    <section className="backup">
      <PageHeader
        title="Backup"
        description={
          <>
            A backup is the whole database, password hashes included: anyone
            holding one can read every account&rsquo;s stored hash offline.
          </>
        }
        actions={
          <HeldButton
            type="button"
            name="take"
            className="keeps-wait-width"
            onClick={take}
            held={takingDrawn}
          >
            <WaitWords
              drawn={takingDrawn}
              words="Back up now"
              wait={BACKING_UP}
            />
          </HeldButton>
        }
      />
      {failure !== undefined && <p role="alert">{failure}</p>}
      {takeFailure !== undefined && <p role="alert">{takeFailure}</p>}
      {backups === undefined && <PageLoading drawn={loadingDrawn} />}

      {backups !== undefined && (
        <>
          <Section title="Backups">
            {backups.length === 0 ? (
              <p className="empty">
                None yet. Back up now takes one with <code>VACUUM INTO</code> on
                the live database, so nothing has to stop.
              </p>
            ) : (
              <ul className="backup-rows settings-group">
                {backups.map((held) => (
                  <li key={held.name}>
                    <Row
                      label={
                        <a href={backupUrl(held.name)} download>
                          {held.name}
                        </a>
                      }
                      help={
                        <>
                          <span className="when">
                            {readableTaken(held.taken)}
                          </span>
                          {JOINING_DOT}
                          <span className="size">{humanBytes(held.bytes)}</span>
                        </>
                      }
                    />
                  </li>
                ))}
              </ul>
            )}
          </Section>

          <details className="settings-details">
            <summary>How backups work</summary>
            <p>
              <strong>
                It is the same file an operator would copy off the disk
              </strong>
              , which is why every route behind this screen is
              administrator-only.
            </p>
            <p>
              <strong>Restore is out of band.</strong> Stop the server, move the
              file into place, start it again. There is no restore button: a
              restore through a running server would have to quiesce every
              writer in the process, and that is not worth the failure modes it
              brings.
            </p>
            <p>
              <strong>A sidecar export is not a backup.</strong> It writes five
              documents per work and none of the cross-work data: collections,
              tags, external ids, users, read history. An exported library still
              needs this.
            </p>
            <p>
              <strong>There is no schedule here.</strong> Run{' '}
              <code>taiyaku library backup</code> from cron, with{' '}
              <code>--keep N</code> to prune old ones. The server backs up only
              when asked.
            </p>
          </details>
        </>
      )}
    </section>
  )
}

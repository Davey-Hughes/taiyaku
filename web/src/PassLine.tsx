/**
 * A pass's line and its Cancel, the fingerprint pass's or the match pass's,
 * drawn on the Libraries list and on the page of each library the pass
 * covers.
 *
 * **Apart from the scan lines.** A pass holds no directory and nothing a
 * scan holds back waits on it, so it never reads Scan library as Cancel nor
 * a row as Scanning: its own Cancel, beside its line, stops the pass alone,
 * whichever page it is pressed on. Held until the feed's answer after the
 * send lands, as Scan library's Cancel is.
 */
import { useId, useState } from 'react'
import { HeldButton } from './HeldButton'
import { sentenceOf } from './message'
import { Refusal } from './Refusal'
import { askScanFeed, cancelScan, type FeedJob, stepLine } from './scanFeed'

export function PassLine({ job }: { job: FeedJob }) {
  const line = useId()
  /** Cancel was pressed, and its send, or the feed's answer after it, is out. */
  const [sent, setSent] = useState(false)
  /** Why the last Cancel was refused. */
  const [failure, setFailure] = useState<string | undefined>()
  const cancel = () => {
    setSent(true)
    setFailure(undefined)
    cancelScan(job.id)
      .then(
        () => askScanFeed(),
        (err: unknown) => setFailure(sentenceOf(err)),
      )
      .then(() => setSent(false))
  }
  return (
    <>
      <div className="pass-line">
        <p className="scanning" id={line}>
          {job.cancelling ? 'Cancelling…' : stepLine(job)}
        </p>
        {/* Described by the line, so a screen reader hears which job a
            Cancel beside Scan library's own stops. */}
        <HeldButton
          type="button"
          name="cancel-pass"
          aria-describedby={line}
          held={sent || job.cancelling}
          onClick={cancel}
        >
          Cancel
        </HeldButton>
      </div>
      {failure !== undefined && <Refusal>{failure}</Refusal>}
    </>
  )
}

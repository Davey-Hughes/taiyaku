/**
 * Removing a library, and what it costs.
 *
 * `RemoveRoot`'s rules, one level up: the counts are stated before the
 * confirm exists, zero is drawn, the real `DELETE` is sent from here and
 * nowhere else, and the report of the re-derive the rehearsal ran is offered
 * beneath the counts. The report of the real removal's own re-derive is
 * handed up with the answer: this dialog is gone by then, and the screen
 * draws it.
 *
 * **The counts are the library's, not its directories' added up.** A work with
 * an edition in each of two directories is in neither directory's own count,
 * and the server counts it once for the library.
 *
 * **An account whose only grant is this library is named under the counts,
 * and the confirm is still offered.** The removal leaves it seeing no library
 * until an administrator grants one, which the person removing may not know
 * of; the server's dry run names it, and the real removal does what the dry
 * run described.
 *
 * **A directory another library holds too is left to it, and not counted as
 * removed.** The dry run names each one with the libraries that keep it, so
 * the confirm does not say a directory goes that stays.
 */
import { Fragment, useEffect, useRef, useState } from 'react'
import { type RemovalAnswer, statusOf } from './config'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import {
  type Library,
  libraryRemovalCost,
  namesOf,
  noLibrarySentence,
  pagesGoneSentence,
  removeLibrary,
} from './libraries'
import { messageOf, sentenceOf } from './message'
import { PathWithBreaks } from './PathWithBreaks'
import { DeriveSection } from './ScanReport'
import { WaitWords } from './WaitWords'
import { count } from './count'

/** The confirm's words while the removal is out. */
const REMOVING = 'Removing…'

export interface RemoveLibraryProps {
  library: Library
  /**
   * A scan of the library is known, or one of its directories is scanning or
   * confirming a removal of its own. The confirm is held and sends nothing
   * until none is.
   */
  blocked?: boolean
  onCancel: () => void
  /** Called once the library is gone, with what the removal did. */
  onRemoved: (answer: RemovalAnswer) => void
  /**
   * The dry run or the removal was refused while the dialog was drawn, with
   * why and the answer's status. The dialog draws the sentence itself: this
   * tells its owner, for whom a 409 is a hold that may not be drawn yet.
   */
  onRefused?: (sentence: string, status?: number) => void
  /** The removal was refused once this dialog had gone, with why. */
  onLateRefusal?: (sentence: string) => void
}

export function RemoveLibrary({
  library,
  blocked = false,
  onCancel,
  onRemoved,
  onRefused,
  onLateRefusal,
}: RemoveLibraryProps) {
  const drawn = useDrawn()
  /** What was asked, and what it was asked about. */
  const [asked, setAsked] = useState<
    { about: string; answer: RemovalAnswer } | undefined
  >()
  const [failure, setFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
  const removingDrawn = useDrawnWait(removing)
  /**
   * A hold ending bumps `epoch`: whatever went on under the dialog may have
   * changed what removing would cost, so the figures from before it are not
   * drawn and the cost is asked again, whether or not a refusal stands. Not
   * while the removal itself is out, which is answered whatever the cost now
   * is.
   */
  const [wasBlocked, setWasBlocked] = useState(blocked)
  const [epoch, setEpoch] = useState(0)
  if (wasBlocked !== blocked) {
    setWasBlocked(blocked)
    if (!blocked && !removing) setEpoch((n) => n + 1)
  }
  /**
   * The counts are asked again when the library's directories change, and one
   * is never drawn beside the directory count of another asking.
   */
  const dirs = library.dirs.map((f) => f.path).join('\n')
  const about = `${epoch}:${dirs}`
  const cost = asked?.about === about ? asked.answer : undefined
  /** The dry run's wait, drawn once it has lasted; until then it keeps its room. */
  const costDrawn = useDrawnWait(cost === undefined && failure === undefined)
  /** A ref as well as `removing`: two presses before a render both read false. */
  const inFlight = useRef(false)
  // Read by the dry run's answer, which may come after a render that handed
  // over a fresh closure.
  const refused = useRef(onRefused)
  refused.current = onRefused
  const confirmButton = useRef<HTMLButtonElement>(null)
  const cancelButton = useRef<HTMLButtonElement>(null)
  // The confirm goes with the counts: on a refusal for a hold, at a hold's
  // end, and when the directories change. Focus in it would fall to the
  // page, so Cancel, the one control left, takes it.
  useFocusLeavesHeld(cost === undefined, confirmButton, cancelButton)

  useEffect(() => {
    let cancelled = false
    libraryRemovalCost(library.id)
      .then((answer) => {
        if (cancelled) return
        setAsked({ about, answer })
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(sentenceOf(err))
        refused.current?.(sentenceOf(err), statusOf(messageOf(err)))
      })
    return () => {
      cancelled = true
    }
  }, [library.id, about])

  // A held confirm turns its press down (`HeldButton`), so `blocked` needs
  // no check here; `inFlight` holds it from the send, before that is drawn.
  const confirm = () => {
    if (inFlight.current || cost === undefined) return
    inFlight.current = true
    setRemoving(true)
    setFailure(undefined)
    removeLibrary(library.id)
      .then((answer) => onRemoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setRemoving(false)
        const sentence = sentenceOf(err)
        if (!drawn.current) {
          onLateRefusal?.(`${library.name} was not removed (${sentence}).`)
          return
        }
        // As a directory's removal does (`RemoveRoot`): a refusal for a hold
        // takes the counts and the confirm away, and any other failure keeps
        // them to try again. A hold this page sees end asks the cost again.
        const status = statusOf(messageOf(err))
        if (status === 409) setAsked(undefined)
        setFailure(sentence)
        refused.current?.(sentence, status)
      })
  }

  const kept = cost?.removed.kept ?? []
  /**
   * The library's directories the dry run does not name as kept, matched
   * by path: the dry run counts no directories, and is asked again whenever
   * the list's directories change. A filter, so a list staler than it cannot
   * go below 0.
   */
  const removed = library.dirs.filter(
    (dir) => !kept.some((one) => one.path === dir.path),
  ).length
  /**
   * Each directory left, with the libraries that keep it, apart by
   * semicolons: a list of libraries has its own commas and "and".
   */
  const leaves = kept.map((one, i) => (
    <Fragment key={one.path}>
      {i === 0 ? '' : '; '}
      <code>
        <PathWithBreaks path={one.path} />
      </code>{' '}
      to {namesOf(one.libraries)}
    </Fragment>
  ))
  const figures =
    cost === undefined ? undefined : (
      <>
        {count(cost.removed.works, 'work', 'works')},{' '}
        {count(
          cost.removed.positions,
          'in-progress position',
          'in-progress positions',
        )}{' '}
        and {count(cost.removed.notes, 'note', 'notes')}
      </>
    )

  return (
    <div
      className="remove-library"
      role="group"
      aria-label={`Remove the library ${library.name}`}
    >
      {failure !== undefined && <p role="alert">{failure}</p>}
      {cost === undefined ? (
        failure === undefined && (
          <p className={costDrawn ? undefined : 'wait-unseen'}>
            Working out what this would remove…
          </p>
        )
      ) : (
        <>
          <p className="counts">
            Removing {library.name}{' '}
            {removed === 0 && kept.length > 0 ? (
              <>
                leaves {leaves}, and removes {figures}
              </>
            ) : (
              <>
                removes its {count(removed, 'directory', 'directories')},{' '}
                {figures}
                {kept.length > 0 && <>, and leaves {leaves}</>}
              </>
            )}
            .
          </p>
          {(cost.removed.positions_page_gone > 0 ||
            cost.removed.notes_page_gone > 0) && (
            <p className="pages-gone">
              {pagesGoneSentence(
                cost.removed.positions_page_gone,
                cost.removed.notes_page_gone,
              )}
            </p>
          )}
          {cost.removed.left_with_no_library.length > 0 && (
            <p className="left-with-no-library">
              {noLibrarySentence(
                cost.removed.left_with_no_library,
                'this library',
              )}
            </p>
          )}
          <p className="irreversible">
            Nothing here can be undone from this screen. What goes is everything
            derived from files no other library's directories hold, and
            everything anybody recorded against a work that goes. No archive on
            disk is changed. A work that stays takes its documents under
            .taiyaku/ to the directory that now holds its files, if that
            directory keeps documents; the report after the removal lists any
            left behind as orphaned documents.
          </p>
        </>
      )}
      {/* The confirm and Cancel, apart, then the disclosure, so that a
          press meant for one does not land on another; in the document in
          that order, so Tab goes the way the eye does. */}
      <div className="settings-actions">
        {cost !== undefined && (
          <HeldButton
            ref={confirmButton}
            type="button"
            className="confirm keeps-wait-width"
            onClick={confirm}
            held={removingDrawn || blocked}
          >
            <WaitWords
              drawn={removingDrawn}
              words={`Remove ${library.name}`}
              wait={REMOVING}
            />
          </HeldButton>
        )}
        <HeldButton
          ref={cancelButton}
          type="button"
          onClick={() => {
            if (!removing) onCancel()
          }}
          held={removingDrawn}
        >
          Cancel
        </HeldButton>
      </div>
      {cost !== undefined && (
        <details className="removal-derive">
          <summary>What re-deriving without this library would find</summary>
          <DeriveSection derive={cost.derive} level={4} />
        </details>
      )}
    </div>
  )
}

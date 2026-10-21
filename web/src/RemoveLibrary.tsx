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
 */
import { useEffect, useRef, useState } from 'react'
import type { RemovalAnswer } from './config'
import { useDrawn } from './hooks'
import {
  type Library,
  libraryRemovalCost,
  noLibrarySentence,
  removeLibrary,
} from './libraries'
import { sentenceOf } from './message'
import { DeriveSection } from './ScanReport'

/** `n` of a thing, in the number the count is. Zero takes the plural. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

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
  /** The removal was refused once this dialog had gone, with why. */
  onLateRefusal?: (sentence: string) => void
}

export function RemoveLibrary({
  library,
  blocked = false,
  onCancel,
  onRemoved,
  onLateRefusal,
}: RemoveLibraryProps) {
  const drawn = useDrawn()
  /** What was asked, and what it was asked about. */
  const [asked, setAsked] = useState<
    { about: string; answer: RemovalAnswer } | undefined
  >()
  const [failure, setFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
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
  /** A ref as well as `removing`: two presses before a render both read false. */
  const inFlight = useRef(false)

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
      })
    return () => {
      cancelled = true
    }
  }, [library.id, about])

  // A held confirm is a disabled button, which sends nothing when pressed.
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
        if (drawn.current) setFailure(sentence)
        else onLateRefusal?.(`${library.name} was not removed (${sentence}).`)
      })
  }

  return (
    <div
      className="remove-library"
      role="group"
      aria-label={`Remove the library ${library.name}`}
    >
      {failure !== undefined && <p role="alert">{failure}</p>}
      {cost === undefined ? (
        failure === undefined && <p>Working out what this would remove…</p>
      ) : (
        <>
          <p className="counts">
            Removing {library.name} removes its{' '}
            {count(library.dirs.length, 'directory', 'directories')},{' '}
            {count(cost.removed.works, 'work', 'works')},{' '}
            {count(
              cost.removed.positions,
              'in-progress position',
              'in-progress positions',
            )}{' '}
            and {count(cost.removed.notes, 'note', 'notes')}.
          </p>
          {cost.removed.left_with_no_library.length > 0 && (
            <p className="left-with-no-library">
              {noLibrarySentence(
                cost.removed.left_with_no_library,
                'this library',
              )}
            </p>
          )}
          <p className="irreversible">
            Nothing here can be undone from this screen. The files on disk are
            untouched; what goes is everything this library derived from them,
            and everything anybody recorded against it.
          </p>
          <button
            type="button"
            className="confirm"
            onClick={confirm}
            disabled={removing || blocked}
          >
            {removing ? 'Removing…' : `Remove ${library.name}`}
          </button>
          <details className="removal-derive">
            <summary>What re-deriving without this library would find</summary>
            <DeriveSection derive={cost.derive} level={4} />
          </details>
        </>
      )}
      <button type="button" onClick={onCancel} disabled={removing}>
        Cancel
      </button>
    </div>
  )
}

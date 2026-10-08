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
 * **The counts are the library's, not its folders' added up.** A work with
 * an edition in each of two folders is in neither folder's own count, and
 * the server counts it once for the library.
 *
 * **A refusal is the sentence and nothing else.** A library that is some
 * account's only grant is not removed, and the server names the accounts.
 */
import { useEffect, useRef, useState } from 'react'
import type { RemovalAnswer } from './config'
import { type Library, libraryRemovalCost, removeLibrary } from './libraries'
import { sentenceOf } from './message'
import { DeriveSection } from './ScanReport'

/** `n` of a thing, in the number the count is. Zero takes the plural. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export interface RemoveLibraryProps {
  library: Library
  /**
   * A scan of the library is known, or one of its folders is scanning or
   * confirming a removal of its own. The confirm is held and sends nothing
   * until none is.
   */
  blocked?: boolean
  onCancel: () => void
  /** Called once the library is gone, with what the removal did. */
  onRemoved: (answer: RemovalAnswer) => void
}

export function RemoveLibrary({
  library,
  blocked = false,
  onCancel,
  onRemoved,
}: RemoveLibraryProps) {
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
   * The counts are asked again when the library's folders change, and one is
   * never drawn beside the folder count of another asking.
   */
  const folders = library.folders.map((f) => f.path).join('\n')
  const about = `${epoch}:${folders}`
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
        setFailure(sentenceOf(err))
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
            {count(library.folders.length, 'folder', 'folders')},{' '}
            {count(cost.removed.works, 'work', 'works')},{' '}
            {count(
              cost.removed.positions,
              'in-progress position',
              'in-progress positions',
            )}{' '}
            and {count(cost.removed.notes, 'note', 'notes')}.
          </p>
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

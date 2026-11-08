/**
 * Removing a directory, and what it costs — spec C4 §2.
 *
 * **The most destructive action in the product, and it does not look like
 * one.** Works derive from `scan_entry` + `rule`; dropping a directory drops
 * its entries, and progress, notes and read history all hang off works. So this
 * states its counts before it acts, and the real `DELETE` is sent from here
 * and nowhere else.
 *
 * **The dry run is fetched when this mounts, not when the directory's page is
 * drawn.** A count per directory per page load is a derivation-shaped question
 * asked about directories nobody is removing: `removal_cost` runs the same
 * apply-read-rollback machinery the rule preview does.
 *
 * **The confirm does not exist until the counts do.** A button that removes a
 * directory while the sentence above it says nothing is the failure this whole
 * component exists to prevent, and "the dry run failed" is exactly when a
 * person is most likely to press on regardless.
 *
 * **Each library the removal empties is named beneath the counts, from the
 * dry run**, which the server deletes with its last directory: a directory
 * several libraries hold can be the last of each, which no one library's
 * list says. **So is each account all of whose grants are of those
 * libraries**: it will see no library afterwards, which nothing else here
 * says. The confirm is still offered.
 *
 * **Zero is drawn.** "This removes 0 works, 0 in-progress positions and 0
 * notes" is a different sentence from a dialog that mentions no counts, and
 * the second reads as "counts unavailable" to whoever is about to press the
 * button.
 *
 * **And the re-derive's own report is drawn beneath them.** A removal is
 * delete-plus-derive, so it produces a whole `DeriveReport`. Showing only the
 * three counts would leave a removal whose re-derive reported
 * `unwritable_documents` or `lost_positions` saying nothing about either, on
 * the one screen in this product built around a pass naming what it did
 * not do.
 */
import { useEffect, useRef, useState } from 'react'
import {
  type RemovalAnswer,
  removalCost,
  removeRoot,
  type Root,
} from './config'
import { namesOf, noLibrarySentence } from './libraries'
import { DeriveSection } from './ScanReport'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait } from './hooks'
import { sentenceOf } from './message'
import { WaitWords } from './WaitWords'
import { count } from './count'

/** The confirm's words while the removal is out. */
const REMOVING = 'Removing…'

export interface RemoveRootProps {
  root: Root
  /**
   * Every library holding the directory, by name, where more than one does:
   * the counts say it goes from each.
   */
  holders?: string[]
  /**
   * A scan of the directory, or of one around or inside it, is reserved, so
   * the server would refuse the removal and the counts quoted here are from
   * before it. The dialog cancels itself when this becomes true, as its
   * Cancel does, and so not while its removal is out.
   */
  held?: boolean
  onCancel: () => void
  /**
   * Called once the directory is actually gone, with what the removal actually
   * did.
   *
   * **The real answer, not the rehearsal's.** A removal whose own re-derive
   * reports `unwritable_documents` or `lost_positions` would otherwise reach
   * nobody, and the only figures anybody saw would be the rolled-back dry
   * run's, which can differ.
   */
  onRemoved: (answer: RemovalAnswer) => void
  /** The removal was refused once this dialog had gone, with why. */
  onLateRefusal?: (sentence: string) => void
}

/**
 * Whom the dialog says an account left seeing no library could see: the
 * directory's one library, the emptied library by name where another library
 * holds the directory too, and for several, only that each is among them,
 * since an account granted some of them sees none afterwards all the same.
 */
function emptiedOnes(emptied: string[], holders: number): string {
  if (emptied.length > 1) return 'libraries this removes'
  return holders > 1 ? namesOf(emptied) : "this directory's library"
}

export function RemoveRoot({
  root,
  holders = [],
  held = false,
  onCancel,
  onRemoved,
  onLateRefusal,
}: RemoveRootProps) {
  const drawn = useDrawn()
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
  const removingDrawn = useDrawnWait(removing)
  /** The dry run's wait, drawn once it has lasted; until then it keeps its room. */
  const costDrawn = useDrawnWait(cost === undefined && failure === undefined)
  // Read when `held` changes, and not a dependency: a removal that is refused
  // while the hold stands must leave its refusal drawn.
  const state = useRef({ removing, onCancel })
  state.current = { removing, onCancel }
  useEffect(() => {
    if (held && !state.current.removing) state.current.onCancel()
  }, [held])
  /**
   * Whether the real `DELETE` is already on its way.
   *
   * **A ref, for the reason `RootRow`'s and `Libraries`' own are** — React batches
   * state, so two presses dispatched before the next render both read
   * `removing` false, and the confirm is drawn held only once the wait
   * has lasted, so nothing else holds a second press until then. This is
   * the button where an unguarded second dispatch is least worth leaving to
   * luck: C4 §2 calls it the most destructive action in the product.
   */
  const inFlight = useRef(false)

  useEffect(() => {
    let cancelled = false
    removalCost(root.id)
      .then((answer) => {
        if (cancelled) return
        setCost(answer)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(sentenceOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [root.id])

  const emptied = cost?.removed.libraries_removed ?? []

  const confirm = () => {
    if (inFlight.current) return
    inFlight.current = true
    setRemoving(true)
    setFailure(undefined)
    removeRoot(root.id)
      .then((answer) => onRemoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setRemoving(false)
        const sentence = sentenceOf(err)
        if (drawn.current) setFailure(sentence)
        else onLateRefusal?.(`${root.path} was not removed (${sentence}).`)
      })
  }

  return (
    <div
      className="remove-root"
      role="group"
      aria-label={`Remove the directory at ${root.path}`}
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
            This removes{' '}
            {holders.length > 1 && `it from ${namesOf(holders)}, and `}
            {count(cost.removed.works, 'work', 'works')},{' '}
            {count(
              cost.removed.positions,
              'in-progress position',
              'in-progress positions',
            )}{' '}
            and {count(cost.removed.notes, 'note', 'notes')}.
          </p>
          {emptied.length > 0 && (
            <p className="last-dir">
              {emptied.length === 1
                ? `This is the only directory of the library ${emptied[0]}, so removing it removes the library too.`
                : `This is the only directory of the libraries ${namesOf(emptied)}, so removing it removes them too.`}
            </p>
          )}
          {cost.removed.left_with_no_library.length > 0 && (
            <p className="left-with-no-library">
              {noLibrarySentence(
                cost.removed.left_with_no_library,
                emptiedOnes(emptied, holders.length),
              )}
            </p>
          )}
          <p className="irreversible">
            Nothing here can be undone from this screen. The files on disk are
            untouched; what goes is everything derived from files no other
            directory holds, and everything anybody recorded against a work that
            goes.
          </p>
        </>
      )}
      {/* As a library's removal draws them (`RemoveLibrary`): the confirm
          and Cancel, then the disclosure, in that order in the document
          too. */}
      <div className="settings-actions">
        {cost !== undefined && (
          <HeldButton
            type="button"
            className="confirm keeps-wait-width"
            onClick={confirm}
            held={removingDrawn}
          >
            <WaitWords
              drawn={removingDrawn}
              words={`Remove ${root.path}`}
              wait={REMOVING}
            />
          </HeldButton>
        )}
        <HeldButton
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
          <summary>What re-deriving without this directory would find</summary>
          <DeriveSection derive={cost.derive} level={5} />
        </details>
      )}
    </div>
  )
}

/**
 * Removing a root, and what it costs — spec C4 §2.
 *
 * **The most destructive action in the product, and it does not look like
 * one.** Works derive from `scan_entry` + `rule`; dropping a root drops its
 * entries, and progress, notes and read history all hang off works. So this
 * states its counts before it acts, and the real `DELETE` is sent from here
 * and nowhere else.
 *
 * **The dry run is fetched when this mounts, not when the card renders.** A
 * count per root per page load is a derivation-shaped question asked about
 * roots nobody is removing — `removal_cost` runs the same apply-read-rollback
 * machinery the rule preview does, measured at a median of 281 ms on a real
 * library.
 *
 * **The confirm does not exist until the counts do.** A button that removes a
 * root while the sentence above it says nothing is the failure this whole
 * component exists to prevent, and "the dry run failed" is exactly when a
 * person is most likely to press on regardless.
 *
 * **Zero is drawn.** "This removes 0 works, 0 in-progress positions and 0
 * notes" is a different sentence from a dialog that mentions no counts, and
 * the second reads as "counts unavailable" to whoever is about to press the
 * button.
 *
 * **And the re-derive's own report is drawn beneath them.** A removal is
 * delete-plus-derive, so it produces a whole `DeriveReport` — the server's own
 * whole-branch review added that key so that "nothing a removal's re-derive
 * found ever reached a caller" would stop being true. Showing only the three
 * counts put it straight back: a removal whose re-derive reported
 * `unwritable_documents` or `lost_positions` said nothing about either, on the
 * one screen in this product built around a pass naming what it did not do.
 * Whole-branch review, 2026-09-04.
 */
import { useEffect, useRef, useState } from 'react'
import {
  type RemovalAnswer,
  removalCost,
  removeRoot,
  type Root,
} from './config'
import { DeriveSection } from './ScanReport'

/**
 * `n` of a thing, in the number the count actually is.
 *
 * Zero takes the plural — "0 works" — which is English rather than an
 * oversight; only one is singular. Found by rendering this dialog in a real
 * browser, where it read "This removes 1 works": jsdom has no opinion about
 * prose, and every fixture until then happened to use plurals.
 */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export interface RemoveRootProps {
  root: Root
  onCancel: () => void
  /**
   * Called once the root is actually gone, with what the removal actually
   * did.
   *
   * **The real answer, not the rehearsal's.** This used to be `() => void`
   * and the `RemovalAnswer` `removeRoot` resolves was dropped on the floor —
   * so a removal whose own re-derive reported `unwritable_documents` or
   * `lost_positions` reached nobody, and the only figures anybody saw were
   * the rolled-back dry run's, which can differ. Pass 4 of the whole-branch
   * review; the same defect this component's doc comment says it exists to
   * prevent, one step later in the flow.
   */
  onRemoved: (answer: RemovalAnswer) => void
}

export function RemoveRoot({ root, onCancel, onRemoved }: RemoveRootProps) {
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
  /**
   * Whether the real `DELETE` is already on its way.
   *
   * **A ref, for the reason `RootRow`'s and `Roots`' own are** — React batches
   * state, so `disabled={removing}` alone let two presses dispatched before
   * the next render both send. This is the one button on the branch where an
   * unguarded second dispatch is least worth leaving to luck: C4 §2 calls it
   * the most destructive action in the product. Review pass 2, 2026-09-04.
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
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [root.id])

  const confirm = () => {
    if (inFlight.current) return
    inFlight.current = true
    setRemoving(true)
    removeRoot(root.id)
      .then((answer) => onRemoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setRemoving(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <div
      className="remove-root"
      role="group"
      aria-label={`Remove the root at ${root.path}`}
    >
      {failure !== undefined && <p role="alert">{failure}</p>}
      {cost === undefined ? (
        failure === undefined && <p>Working out what this would remove…</p>
      ) : (
        <>
          <p className="counts">
            This removes {count(cost.removed.works, 'work', 'works')},{' '}
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
          <button type="button" onClick={confirm} disabled={removing}>
            {removing ? 'Removing…' : `Remove ${root.path}`}
          </button>
          <details className="removal-derive">
            <summary>What re-deriving without this root would find</summary>
            <DeriveSection derive={cost.derive} />
          </details>
        </>
      )}
      <button type="button" onClick={onCancel} disabled={removing}>
        Cancel
      </button>
    </div>
  )
}

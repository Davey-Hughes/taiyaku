import { GapOffers } from './GapOffers'
import type { Direction } from './modules/direction'
import type { Verdict } from './session'

/** One of `nearestPaired`'s offers, resolved to something a person reads. */
export interface GapMove {
  /** The slot to hand back to `onTake` — authority-space, what `takeOffered` expects. */
  slot: number
  /**
   * The page this move lands on, in the edition currently shown. Display
   * only, and 1-based — `App.tsx` converts at the point it builds this prop,
   * the same boundary convention its own `altAt` follows; every internal
   * page/slot value stays zero-based.
   */
  page: number
}

export interface GapProps {
  /**
   * Which way the work reads. The two offers point the way the pages go: in a
   * right-to-left work the earlier page is to the right, and an unconditional
   * `←`/`→` sent a reader the wrong way on the one screen they reach by being
   * lost.
   */
  reading: Direction
  /** The edition with no counterpart — the one being looked at. */
  edition: string
  /**
   * Where the gap was found: the authority edition, and the page in it.
   * `page` is 1-based for display, same as `GapMove.page` above.
   */
  from: { edition: string; page: number }
  /** This pair's alignment verdict, when the sidecar records one. */
  verdict?: Verdict
  before?: GapMove
  after?: GapMove
  onTake: (slot: number) => void
}

const sentence: Record<Verdict, string> = {
  sound:
    'This pairing is sound overall — this is one isolated gap, not a sign of trouble.',
  fragmented:
    'This pairing is fragmented: matches are sparse across the whole volume, so gaps like this one are expected.',
  refused:
    'This pairing was refused: these editions were not found to correspond at all.',
}

/**
 * The no-counterpart state.
 *
 * A view, not a toast: nothing here dismisses itself or times out, because
 * the gap is exactly as real a state as any page and stays until a real
 * navigation — one of the buttons below, or the toggle back — leaves it.
 * `notes.md`'s swap gotcha is what a silent jump to the nearest would cause;
 * this names the edition and the page instead, and offers the move rather
 * than taking it.
 */
export function Gap({
  reading,
  edition,
  from,
  verdict,
  before,
  after,
  onTake,
}: GapProps) {
  return (
    <div
      role="status"
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 16,
        height: '100%',
        padding: 24,
        textAlign: 'center',
      }}
    >
      <p className="reader-gap-title">
        No <strong>{edition}</strong> page for <strong>{from.edition}</strong>{' '}
        page {from.page}.
      </p>
      {verdict && (
        <p className="reader-muted" style={{ margin: 0 }}>
          {sentence[verdict]}
        </p>
      )}
      <GapOffers
        reading={reading}
        edition={edition}
        before={before}
        after={after}
        onTake={onTake}
      />
    </div>
  )
}

import type { GapMove } from './Gap'
import type { Direction } from './modules/direction'

export interface GapOffersProps {
  reading: Direction
  /** The edition with no page here — whose pages the offers land on. */
  edition: string
  before?: GapMove
  after?: GapMove
  onTake: (slot: number) => void
}

/**
 * The two moves either side of a gap, in reading order.
 *
 * Shared by `Gap` and `Borrowed` rather than duplicated: they are the same two
 * destinations, and since the arrows take them too (`session.ts`'s `step`) a
 * reader who learns them on one screen must find them identical on the other.
 *
 * The earlier page is on the right in a right-to-left work, so the row sits
 * the way the pages do — the same rule as the filmstrip's rail, and for the
 * same reason: this is a screen a reader reaches by being lost.
 */
export function GapOffers({
  reading,
  edition,
  before,
  after,
  onTake,
}: GapOffersProps) {
  if (before === undefined && after === undefined) {
    return (
      <p style={{ margin: 0, opacity: 0.8 }}>
        No paired page nearby in {edition} either.
      </p>
    )
  }
  return (
    <div
      style={{
        display: 'flex',
        gap: 12,
        flexDirection: reading === 'rtl' ? 'row-reverse' : 'row',
      }}
    >
      {before && (
        <button type="button" onClick={() => onTake(before.slot)}>
          {reading === 'rtl' ? '→' : '←'} {edition} page {before.page}
        </button>
      )}
      {after && (
        <button type="button" onClick={() => onTake(after.slot)}>
          {edition} page {after.page} {reading === 'rtl' ? '←' : '→'}
        </button>
      )}
    </div>
  )
}

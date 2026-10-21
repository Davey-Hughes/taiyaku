/**
 * What a move does or did, in words: the rehearsal's sentences in the dialog
 * and the kept report's after the move are this one rendering, so they cannot
 * drift apart.
 *
 * **Zero is drawn** for the three things a work can do and for what goes
 * with them: a sentence that leaves a count out reads as a count that could
 * not be taken. The one exception is the join where the target is a library
 * the move makes and nothing joined: nothing can join a work there, so a
 * zero would describe something that cannot happen. A count that is not zero
 * is always said.
 */
import type { Moved } from './libraries'

/** `n` of a thing with its verb, in the number the count is. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export interface MoveCountsProps {
  moved: Moved
  /**
   * The name of the library the directory was in. The rehearsal opens with it;
   * the kept report, whose heading says it, passes none.
   */
  from?: string
  /** Said as done, not as about to be. */
  past?: boolean
  /** The target is a library the move makes, so it holds no work to join. */
  newLibrary?: boolean
}

export function MoveCounts({
  moved,
  from,
  past = false,
  newLibrary = false,
}: MoveCountsProps) {
  const keeps = past
    ? count(moved.works_kept, 'work kept its', 'works kept their')
    : count(moved.works_kept, 'work keeps its', 'works keep their')
  const joins = past
    ? count(moved.works_joined, 'work joined', 'works joined')
    : count(moved.works_joined, 'work joins', 'works join')
  const split = past
    ? count(moved.works_split, 'work was', 'works were')
    : count(moved.works_split, 'work is', 'works are')
  const holds = past ? 'already held' : 'already holds'
  const grants = count(moved.grants_removed, 'grant', 'grants')
  return (
    <div className="move-counts">
      <p className="counts">
        {from !== undefined &&
          `Moving this directory from ${from} to ${moved.library}: `}
        {keeps} address
        {newLibrary && moved.works_joined === 0
          ? ' and '
          : `, ${joins} a work ${moved.library} ${holds}, as another edition, and `}
        {split} split between the two libraries.
      </p>
      <p>
        {count(
          moved.positions,
          'in-progress position',
          'in-progress positions',
        )}{' '}
        and {count(moved.notes, 'note', 'notes')} {past ? 'went' : 'go'} with
        them.
      </p>
      {moved.series_joined > 0 && (
        <p>
          {count(moved.series_joined, 'series', 'series')} already in{' '}
          {moved.library}{' '}
          {past ? 'gained' : moved.series_joined === 1 ? 'gains' : 'gain'}{' '}
          volumes.
        </p>
      )}
      {moved.alignments_dropped > 0 && (
        <p>
          {count(
            moved.alignments_dropped,
            `alignment between editions the move ${past ? 'separated was' : 'separates is'}`,
            `alignments between editions the move ${past ? 'separated were' : 'separates are'}`,
          )}{' '}
          dropped.
        </p>
      )}
      {moved.library_removed !== null && (
        <p>
          {moved.library_removed} {past ? 'had' : 'has'} no other directory, so
          it {past ? 'was' : 'is'} removed
          {moved.grants_removed > 0 && `, with ${grants}`}.
        </p>
      )}
    </div>
  )
}

/**
 * What a move does or did, in words: the rehearsal's sentences in the dialog
 * and the kept report's after the move are this one rendering, so they cannot
 * drift apart.
 *
 * **A move changes only which library holds the directory** (spec §3.5), so
 * what it says is who gains sight of the directory, who loses it, and the
 * library it empties.
 *
 * **Both lists are said, an empty one as no account**: a sentence that leaves
 * one out reads as a list that could not be read. Where both are empty, that
 * is said once.
 */
import { libraryRemovedSentence, type Moved, namesOf } from './libraries'

/**
 * Who gains sight of the directory and who loses it, as one clause with no
 * stop. `none` is "no account" as the clause's first words are written.
 */
function sightOf(moved: Moved, past: boolean, none: string): string {
  const { gains_sight: gains, loses_sight: loses } = moved
  if (gains.length === 0 && loses.length === 0) {
    return `${none} ${past ? 'gained or lost' : 'will gain or lose'} sight of it`
  }
  const gained = gains.length === 0 ? none : namesOf(gains)
  const lost = loses.length === 0 ? 'no account' : namesOf(loses)
  return `${gained} ${past ? 'gained' : 'will gain'} sight of it, and ${lost} ${past ? 'lost' : 'will lose'} sight of it`
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
}

export function MoveCounts({ moved, from, past = false }: MoveCountsProps) {
  const removed = libraryRemovedSentence(moved, past)
  return (
    <div className="move-counts">
      <p className="counts">
        {from === undefined
          ? `${sightOf(moved, past, 'No account')}.`
          : `Moving this directory from ${from} to ${moved.library}: ${sightOf(moved, past, 'no account')}.`}
      </p>
      {removed !== undefined && <p>{removed}</p>}
    </div>
  )
}

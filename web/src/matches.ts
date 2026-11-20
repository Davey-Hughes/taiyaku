/**
 * The two content-match presses an administrator makes from a volume's panel
 * (spec `2026-11-19-content-matching-volume-joins-design.md` §5): Split, and
 * its Undo. Each derives the whole store before it answers, so its answer
 * carries the derive's report.
 */
import { sendJson } from './api'
import { type Derived, expectBody } from './config'

/** What a Split answers: each pair it set apart, and the derive it ran. */
export interface SplitAnswer {
  split: number[]
  derive: Derived
}

/** What an Undo answers: the derive it ran. */
export interface UndoAnswer {
  derive: Derived
}

/**
 * Splits the edition whose mark was pressed off the work pair `id` joined
 * it to: `work` is that work's `WorkCard.slug` and `edition` the edition's
 * `EditionCard.key`. Named by both, since in a triangle of matches two
 * editions of one work can carry the same pair, and only the one pressed
 * leaves. The server refuses a pair that did not join that edition.
 */
export const splitMatch = (
  id: number,
  work: string,
  edition: string,
): Promise<SplitAnswer> =>
  expectBody(
    sendJson<SplitAnswer>(`/api/library/matches/${id}/split`, 'POST', {
      work,
      edition,
    }),
  )

/**
 * Undoes the whole Split that set pair `id` apart: every pair it set apart
 * joins again where it still applies, with one derive.
 */
export const undoSplit = (id: number): Promise<UndoAnswer> =>
  expectBody(sendJson<UndoAnswer>(`/api/library/matches/${id}/split`, 'DELETE'))

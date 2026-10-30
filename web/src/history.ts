/**
 * A there-and-back-again stack over whole documents.
 *
 * **Snapshots rather than a log of operations.** An operation log has to know
 * how to invert every kind of change — a body edited, a region moved, a note
 * deleted, a colour picked — and gains a new inverse every time the editor
 * gains a verb. A notes document is a handful of small records, so keeping the
 * whole thing per step costs nothing measurable and cannot disagree with the
 * change it is meant to undo.
 *
 * This exists because writes are automatic: every change lands on its own, so
 * a reader needs a way back that does not depend on having hesitated.
 */
export interface History<T> {
  /** Every state kept, oldest first. `at` indexes the one in force. */
  entries: T[]
  at: number
}

/** How many states to keep. Beyond this the oldest are dropped. */
const LIMIT = 50

export function emptyHistory<T>(initial: T): History<T> {
  return { entries: [initial], at: 0 }
}

/**
 * Record a new state as the current one.
 *
 * **Everything after `at` is discarded**, which is what makes this a history
 * rather than a graph: having gone back three steps and then written
 * something, the three states you had gone back *from* are no longer reachable
 * by going forward, because forward now means something else. Every editor
 * works this way and readers expect it.
 *
 * A state identical to the one in force is not recorded — a write that changed
 * nothing should not cost a step of history, or undo would need pressing twice
 * to do anything visible.
 */
export function record<T>(
  history: History<T>,
  next: T,
  same: (a: T, b: T) => boolean,
): History<T> {
  if (same(history.entries[history.at], next)) return history
  const kept = [...history.entries.slice(0, history.at + 1), next]
  const overflow = Math.max(kept.length - LIMIT, 0)
  const entries = kept.slice(overflow)
  return { entries, at: entries.length - 1 }
}

export const canUndo = <T>(history: History<T>): boolean => history.at > 0
export const canRedo = <T>(history: History<T>): boolean =>
  history.at < history.entries.length - 1

/** One step back, or the same history where there is nowhere to go. */
export function undo<T>(history: History<T>): History<T> {
  return canUndo(history) ? { ...history, at: history.at - 1 } : history
}

/** One step forward, or the same history where there is nowhere to go. */
export function redo<T>(history: History<T>): History<T> {
  return canRedo(history) ? { ...history, at: history.at + 1 } : history
}

/** The state in force. */
export const current = <T>(history: History<T>): T =>
  history.entries[history.at]

/**
 * A step, and the value it has to write. `undefined` where it is a no-op.
 *
 * **Identity is the no-op rule**, not a bounds check restated here: `undo` and
 * `redo` above hand back the *same* history object at either end, so a caller
 * comparing what it asked for against what it holds already knows whether
 * anything moved. A second `at > 0` here would be the same rule written twice,
 * and the two could disagree the moment either grew a case.
 *
 * The value comes back with the history because a step is written as well as
 * shown: with automatic saving, what is on the server *is* the document, so an
 * undo that only changed the screen would be undone again by the next
 * keystroke. Returning them together is what stops a caller writing one
 * history's value while holding another's.
 *
 * Generic, like everything else in this file. The reader has exactly one undo
 * stack and it is the notes', but the rule is about `History<T>` and writing
 * it as `History<Note[]>` would be the only thing in this module that knew
 * what it was holding.
 */
export function stepHistory<T>(
  from: History<T>,
  to: History<T>,
): { history: History<T>; write: T } | undefined {
  if (to === from) return undefined
  return { history: to, write: current(to) }
}

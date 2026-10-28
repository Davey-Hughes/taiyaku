import { describe, expect, it } from 'vitest'
import {
  canRedo,
  canUndo,
  current,
  emptyHistory,
  record,
  redo,
  stepHistory,
  undo,
} from '../src/history'

const same = (a: string, b: string) => a === b
const build = (...steps: string[]) =>
  steps.reduce((h, step) => record(h, step, same), emptyHistory('start'))

describe('history', () => {
  it('starts with nowhere to go in either direction', () => {
    const h = emptyHistory('start')
    expect(current(h)).toBe('start')
    expect(canUndo(h)).toBe(false)
    expect(canRedo(h)).toBe(false)
  })

  it('walks back and forth over what was recorded', () => {
    const h = build('a', 'b')
    expect(current(h)).toBe('b')
    expect(current(undo(h))).toBe('a')
    expect(current(undo(undo(h)))).toBe('start')
    expect(current(redo(undo(h)))).toBe('b')
  })

  it('stays put rather than falling off either end', () => {
    const h = build('a')
    expect(current(undo(undo(undo(h))))).toBe('start')
    expect(current(redo(h))).toBe('a')
  })

  // A history, not a graph: having gone back and then written something, the
  // states gone back *from* are no longer reachable forward, because forward
  // now means something else.
  it('discards the forward states once something new is recorded', () => {
    const h = record(undo(build('a', 'b')), 'c', same)
    expect(current(h)).toBe('c')
    expect(canRedo(h)).toBe(false)
    expect(current(undo(h))).toBe('a')
  })

  // A write that changed nothing must not cost a step, or undo would need
  // pressing twice to do anything a reader can see.
  it('does not record a state identical to the one in force', () => {
    const h = build('a')
    expect(record(h, 'a', same)).toBe(h)
  })

  // The step is written as well as shown, so what a caller has to write comes
  // back with the history it has to hold — one answer rather than two that
  // could be taken from different histories.
  it('hands a step back with the state that step has to write', () => {
    const h = build('a', 'b')
    expect(stepHistory(h, undo(h))).toEqual({ history: undo(h), write: 'a' })
    expect(stepHistory(undo(h), redo(undo(h)))).toEqual({
      history: h,
      write: 'b',
    })
  })

  // Identity, not a second bounds check: `undo` and `redo` hand back the same
  // object at either end, and that is the whole no-op rule.
  it('is a no-op where the step had nowhere to go', () => {
    const floor = emptyHistory('start')
    expect(stepHistory(floor, undo(floor))).toBeUndefined()
    const top = build('a')
    expect(stepHistory(top, redo(top))).toBeUndefined()
  })

  it('drops the oldest states rather than growing without bound', () => {
    let h = emptyHistory('start')
    for (let i = 0; i < 80; i++) h = record(h, `step ${i}`, same)
    expect(h.entries.length).toBeLessThanOrEqual(50)
    expect(current(h)).toBe('step 79')
    // Still walkable back through what it kept.
    expect(current(undo(h))).toBe('step 78')
  })
})

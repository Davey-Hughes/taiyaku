import { describe, expect, it } from 'vitest'
import {
  COLOUR_NAMES,
  DEFAULT_COLOUR,
  nextColour,
  noteColour,
} from '../src/palette'

describe('noteColour', () => {
  it('answers the default for a note that names no colour', () => {
    expect(noteColour(undefined)).toBe(noteColour(DEFAULT_COLOUR))
  })

  it('answers a distinct set of pixels for every name it offers', () => {
    const tabs = COLOUR_NAMES.map((name) => noteColour(name).tab)
    expect(new Set(tabs).size).toBe(COLOUR_NAMES.length)
  })

  // A note written by a later build, or by hand, may name a colour this build
  // has never heard of. Falling back is right here where it would be wrong
  // almost anywhere else in this project: the alternative is a note that does
  // not draw at all, and losing sight of a note is far worse than seeing it in
  // the wrong colour.
  it('falls back to the default for a name it does not know', () => {
    expect(noteColour('chartreuse-supreme')).toBe(noteColour(DEFAULT_COLOUR))
  })

  // The picker renders `COLOUR_NAMES` and marks the current one; a default
  // that was not among them would leave a new note showing no selection at
  // all, which reads as broken rather than as "this is the default".
  it('offers the default among the names it lists', () => {
    expect(COLOUR_NAMES).toContain(DEFAULT_COLOUR)
  })

  it('gives every listed name a full set of pixels', () => {
    for (const name of COLOUR_NAMES) {
      const colour = noteColour(name)
      expect(colour.tab).toMatch(/^rgba\(/)
      expect(colour.edge).toMatch(/^rgba\(/)
      expect(colour.tint).toMatch(/^rgba\(/)
    }
  })
})

describe('nextColour', () => {
  it('starts at the default when nothing is used', () => {
    expect(nextColour([])).toBe(DEFAULT_COLOUR)
  })

  it('walks to a colour not yet on the page', () => {
    expect(nextColour([DEFAULT_COLOUR])).toBe(COLOUR_NAMES[1])
    expect(nextColour(COLOUR_NAMES.slice(0, 3))).toBe(COLOUR_NAMES[3])
  })

  // Cycling rather than giving up: a page with one of everything gets the
  // least-used next, so a sixth note on a five-colour page repeats the colour
  // that is furthest from the others rather than always repeating the first.
  it('cycles once every colour is in use', () => {
    expect(nextColour(COLOUR_NAMES)).toBe(DEFAULT_COLOUR)
    expect(nextColour([...COLOUR_NAMES, DEFAULT_COLOUR])).toBe(COLOUR_NAMES[1])
  })

  // A note may name a colour this build does not know; it should not stop the
  // cycle, and it should not be counted as using one of ours.
  it('ignores names it does not know', () => {
    expect(nextColour(['chartreuse-supreme'])).toBe(DEFAULT_COLOUR)
  })

  it('ignores notes with no colour, which are the default', () => {
    expect(nextColour([undefined])).toBe(COLOUR_NAMES[1])
  })
})

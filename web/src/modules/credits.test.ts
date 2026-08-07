import { describe, expect, it } from 'vitest'
import { creditSlots, nextReadable, creditsModule, PANEL_INTENT } from './credits'
import { intentOf } from '../keys'
import { registry } from './registry'

const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  ({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods }) as KeyboardEvent

/** Four pages, page 1 a spread: slots are [0, 1R, 1L, 2, 3]. */
const spreadAt1 = { pages: 4, spreads: [1] }
/** Four pages, no spreads: slots are [0, 1, 2, 3]. */
const flat = { pages: 4, spreads: [] as number[] }

describe('creditSlots', () => {
  it('converts page indices to every slot that page presents', () => {
    // Page 1 is a spread, so skipping it must skip both halves — skipping one
    // would leave the reader on half a credits card.
    expect(creditSlots([1], spreadAt1)).toEqual([1, 2])
  })

  it('is the identity on an edition with no spreads', () => {
    expect(creditSlots([0, 3], flat)).toEqual([0, 3])
  })
})

describe('nextReadable', () => {
  it('steps over a skipped slot', () => {
    expect(nextReadable(0, 1, [1], 4)).toBe(2)
  })

  it('steps over a consecutive stretch', () => {
    expect(nextReadable(0, 1, [1, 2], 4)).toBe(3)
  })

  it('stops at the end rather than walking past it', () => {
    expect(nextReadable(2, 1, [3], 4)).toBe(2)
  })

  it('steps backwards too', () => {
    expect(nextReadable(3, -1, [1, 2], 4)).toBe(0)
  })

  it('returns the slot unchanged when skipping is off', () => {
    // The caller passes an empty list rather than a flag: a module that is
    // switched off contributes nothing, per module.ts's contract.
    expect(nextReadable(0, 1, [], 4)).toBe(1)
  })
})

describe('the module contract', () => {
  it('claims C and nothing else', () => {
    expect(creditsModule.keys?.map((k) => k.key)).toEqual(['C'])
  })

  it('declares defaults the settings panel can generate from', () => {
    expect(creditsModule.defaults).toEqual({ skip: false, panel: true })
  })

  // `c` (lower-case) is one of `intentOf`'s reserved built-ins, checked before
  // any module's own keys, so a module claiming it would collision-check clean
  // against every other module and then simply never fire. This test presses
  // whatever key the module actually declares — through `intentOf`, against
  // the real registry — rather than hardcoding the key, so it would have
  // caught that: it fails red against `c` (yielding `{ kind: 'correct' }`
  // instead) and only goes green once the module claims a key `intentOf`
  // hands to its module loop.
  it('reaches the panel intent through intentOf, against the real registry', () => {
    const key = creditsModule.keys?.[0]?.key
    expect(key).toBeDefined()
    expect(intentOf(press(key as string), ['raw', 'en'], registry.all())).toEqual({
      kind: 'module',
      intent: PANEL_INTENT,
    })
  })
})

describe('skipping is presentational', () => {
  it('changes which slot an arrow lands on, never what a slot is called', () => {
    // The whole content of the numbering decision. If someone later renumbers
    // for readability, this goes red — which is the point.
    const slotCount = 6
    const skipped = [2, 3]
    const visited: number[] = []
    let at = 0
    for (let i = 0; i < 3; i++) {
      at = nextReadable(at, 1, skipped, slotCount)
      visited.push(at)
    }
    expect(visited).toEqual([1, 4, 5])

    const withoutSkipping: number[] = []
    at = 0
    for (let i = 0; i < 3; i++) {
      at = nextReadable(at, 1, [], slotCount)
      withoutSkipping.push(at)
    }
    expect(withoutSkipping).toEqual([1, 2, 3])

    // Same slot numbers in both worlds; only which ones are reached differs.
    expect(Math.max(...visited)).toBeLessThan(slotCount)
  })
})

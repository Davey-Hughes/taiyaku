import { describe, expect, it } from 'vitest'
import {
  creditSlotsIn,
  creditsModule,
  PANEL_INTENT,
  skippedApartFrom,
} from '../../src/modules/credits'
import type { Position } from '../../src/resolver'
import { groups } from '../../src/pairing'
import { intentOf } from '../../src/keys'
import { registry } from '../../src/modules/registry'

const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  ({
    key,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    ...mods,
  }) as KeyboardEvent

/** Four pages, page 1 a spread — the slot table `allSlots` would return. */
const spreadAt1: Position[] = [
  { page: 0 },
  { page: 1, half: 'R' },
  { page: 1, half: 'L' },
  { page: 2 },
  { page: 3 },
]
/** Four pages, no spreads: slot index equals page index. */
const flat: Position[] = [{ page: 0 }, { page: 1 }, { page: 2 }, { page: 3 }]

describe('creditSlotsIn', () => {
  it('converts page indices to every slot that page presents', () => {
    // Page 1 is a spread, so skipping it must skip both halves — skipping one
    // would leave the reader on half a credits card. Both halves carry the
    // same `page`, which is why filtering on `page` gets this for free.
    expect(creditSlotsIn(spreadAt1, [1])).toEqual([1, 2])
  })

  it('is the identity on an edition with no spreads', () => {
    expect(creditSlotsIn(flat, [0, 3])).toEqual([0, 3])
  })

  it('ignores a page the edition does not have', () => {
    // The sidecar and the slot table are separate documents; a `credits` entry
    // naming a page past the end must not produce a slot number that indexes
    // nothing. Returns what it can rather than throwing — the panel and the
    // skip both degrade to "fewer pages", which is the safe direction.
    expect(creditSlotsIn(flat, [2, 99])).toEqual([2])
  })

  it('answers nothing for an empty credits list', () => {
    // The common case: a sidecar with no `credits` field.
    expect(creditSlotsIn(flat, [])).toEqual([])
  })
})

describe('skippedApartFrom', () => {
  it('exempts the page the reader is standing on', () => {
    // Taking a credit page from the panel is a deliberate act, and the panel
    // says so.
    expect([...skippedApartFrom([1, 3], flat, 1)]).toEqual([3])
  })

  it('exempts both halves of a joined spread', () => {
    // Matched by page, not by slot: exempting one half would draw half a
    // credits card.
    expect([...skippedApartFrom([1, 2], spreadAt1, 1)]).toEqual([])
    expect([...skippedApartFrom([1, 2], spreadAt1, 2)]).toEqual([])
  })

  it('changes nothing where the reader is not on a skipped page', () => {
    expect([...skippedApartFrom([1, 3], flat, 0)]).toEqual([1, 3])
  })

  it('changes nothing with no reader position — a gap has no slot', () => {
    expect([...skippedApartFrom([1, 3], flat, undefined)]).toEqual([1, 3])
  })

  it('leaves the reader in some group, which is what keeps a page on screen', () => {
    // The invariant `App.tsx` draws by and `pairing.ts`'s `nearestGroup` is the
    // backstop for: every position the reader can hold belongs to a group.
    // Its violation is not a crash but a *blank page area* — `findIndex`
    // answers `-1` and nothing renders — so nothing else would go red if this
    // broke. Checked here against the shape that would break it first, a
    // joined spread whose halves are exempted together.
    const everyPage = [0, 1, 2, 3]
    for (const slots of [flat, spreadAt1]) {
      const credits = everyPage
      for (let standing = 0; standing < slots.length; standing++) {
        const skipped = skippedApartFrom(
          creditSlotsIn(slots, credits),
          slots,
          standing,
        )
        const grouped = groups(slots, 'two', 'rtl', [], skipped)
        expect(
          grouped.some((g) => g.slots.includes(standing)),
          `standing on slot ${standing} with every page skipped left it in no group`,
        ).toBe(true)
      }
    }
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
  // the real registry — rather than hardcoding the key. It fails for `c`
  // (yielding `{ kind: 'correct' }` instead) and passes only once the module
  // claims a key `intentOf` hands to its module loop.
  it('reaches the panel intent through intentOf, against the real registry', () => {
    const key = creditsModule.keys?.[0]?.key
    expect(key).toBeDefined()
    expect(
      intentOf(press(key as string), ['raw', 'en'], registry.all()),
    ).toEqual({
      kind: 'module',
      intent: PANEL_INTENT,
    })
  })
})

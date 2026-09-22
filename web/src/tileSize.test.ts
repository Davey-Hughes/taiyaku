import { describe, expect, it } from 'vitest'
import {
  requestedTileEdge,
  STRIP_BREAKPOINT_PX,
  STRIP_TILE_PX,
  stripTileEdge,
  TILE_LADDER,
} from './tileSize'

describe('requestedTileEdge', () => {
  it('rounds the pixels a screen wants up to a rung', () => {
    // The six cases that exist: two strip heights, three densities.
    expect(requestedTileEdge(96, 1)).toBe(160)
    expect(requestedTileEdge(96, 2)).toBe(320)
    expect(requestedTileEdge(96, 3)).toBe(320)
    expect(requestedTileEdge(160, 1)).toBe(160)
    expect(requestedTileEdge(160, 2)).toBe(320)
    expect(requestedTileEdge(160, 3)).toBe(480)
  })

  it('rounds up rather than down at a rung’s own value and one past it', () => {
    expect(requestedTileEdge(160, 1)).toBe(160)
    expect(requestedTileEdge(161, 1)).toBe(320)
    expect(requestedTileEdge(320, 1)).toBe(320)
    expect(requestedTileEdge(321, 1)).toBe(480)
  })

  it('clamps above the top rung rather than answering nothing', () => {
    // A 4× display wants 640 and there is no larger tile to offer it —
    // unlike a page, where `pageSize.ts` can fall back to the original bytes.
    expect(requestedTileEdge(160, 4)).toBe(480)
    expect(requestedTileEdge(1000, 3)).toBe(480)
  })

  it('falls to the smallest rung on any input that is not a positive number', () => {
    // The opposite direction from `pageSize.ts`'s `requestedLongEdge`, which
    // falls toward the original: soft is the failure mode for a page, and 237
    // tiles at 480 px is the failure mode here.
    for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(requestedTileEdge(bad, 3)).toBe(160)
      expect(requestedTileEdge(96, bad)).toBe(160)
    }
  })
})

describe('stripTileEdge', () => {
  const view = (matches: boolean, devicePixelRatio: number) => ({
    matchMedia: (query: string) => {
      expect(query).toBe(`(width < ${STRIP_BREAKPOINT_PX}px)`)
      return { matches }
    },
    devicePixelRatio,
  })

  it('asks for the narrow tile below the breakpoint and the wide one above', () => {
    expect(stripTileEdge(view(true, 3))).toBe(
      requestedTileEdge(STRIP_TILE_PX.narrow, 3),
    )
    expect(stripTileEdge(view(false, 3))).toBe(
      requestedTileEdge(STRIP_TILE_PX.wide, 3),
    )
    // And those two are different, or this test would pass against a constant.
    expect(stripTileEdge(view(true, 3))).not.toBe(stripTileEdge(view(false, 3)))
  })

  it('treats a view without matchMedia as the wide one', () => {
    // jsdom has no `matchMedia` at all — probed 2026-09-21, it is `undefined`
    // rather than a stub — so this is the branch every component test in that
    // project takes, not a hypothetical.
    //
    // dpr 3, not 1: at dpr 1 both STRIP_TILE_PX heights round up to the same
    // bottom rung, 160, so that dpr cannot tell wide from narrow and the
    // assertion would pass whichever one the fallback picked. At dpr 3 wide
    // gives 480 and narrow gives 320, so this actually proves the
    // missing-`matchMedia` branch is the wide one.
    expect(stripTileEdge({ devicePixelRatio: 3 })).toBe(480)
    expect(stripTileEdge(undefined)).toBe(160)
  })

  it('treats a missing devicePixelRatio as 1', () => {
    expect(stripTileEdge({ matchMedia: () => ({ matches: false }) })).toBe(160)
  })

  it('offers the three rungs the server holds', () => {
    // `cover.rs`'s `TILE_LONG_EDGES` is the same three. A rung added here
    // alone is a `w` the server answers with the 160 px tile.
    expect(TILE_LADDER).toEqual([160, 320, 480])
  })
})

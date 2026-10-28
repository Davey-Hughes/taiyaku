import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fakeResolver, whole } from './support/fixtures'

// `credits` is a table lookup keyed on the edition alone, unlike `chapterAt`'s
// `edition|index` key — a credits list belongs to the edition as a whole, not
// to one slot of it. Mirrors `fixtures.test.ts`'s `chapterAt` coverage: a
// present entry, and a key absent from the table (which covers both "this
// edition has no list" and "there is no such edition" — the fake does not
// distinguish them any more than the wasm binding does).
describe('Resolver.credits', () => {
  const fake = fakeResolver({ fan: whole(6) }, {}, {}, { fan: [3, 4] })

  it('answers the pages a table names for a present edition', () => {
    expect(fake.credits('fan')).toEqual([3, 4])
  })

  it('answers empty for an edition with no list, or none at all', () => {
    expect(fake.credits('no-such-edition')).toEqual([])
  })
})

// `resolver.ts` imports `taiyaku-wasm` **as a value** (`init`, `WasmSidecar`)
// — every other module in this tree imports only its types, which is why
// this is the one file `vitest run --project node` never loaded and the one
// the coverage gate found at 0%. The fix is this file: mock the module so
// `wasmResolver` runs for real against a fake `WasmSidecar`, and pin the
// conversion at the boundary — page/half/via/member/of/path crossing from a
// wasm-bindgen class into the plain `Position`/`Landed`/`Chapter` objects the
// rest of the reader consumes, plus the `free()` calls that keep the wasm
// side from leaking.
//
// `node`, not `jsdom`: nothing here touches the DOM, and `resolver.ts` is
// already written to be importable without one (its own doc: "no WASM, no
// async init, no CI toolchain" is what the `Resolver` interface exists to
// let `reading.ts`/`pairing.ts` avoid — this file is what finally exercises
// the WASM-backed implementation of that interface itself).
//
// `vi.mock`'s factory is hoisted above every import in this file, including
// the plain `const` declarations that would otherwise back it — referencing
// those directly throws "Cannot access before initialization". `vi.hoisted`
// runs its own callback during that same hoisting pass, before the mock
// factory needs the result, which is what makes `hoisted.init`/`hoisted.fromJson`
// safe to close over below.
const hoisted = vi.hoisted(() => ({
  init: vi.fn(async () => {}),
  fromJson: vi.fn(),
}))

vi.mock('taiyaku-wasm', () => ({
  default: hoisted.init,
  WasmSidecar: { fromJson: hoisted.fromJson },
}))

const { wasmResolver } = await import('../src/resolver')

interface FakeSidecar {
  editions: ReturnType<typeof vi.fn>
  slotCount: ReturnType<typeof vi.fn>
  slotAt: ReturnType<typeof vi.fn>
  indexOf: ReturnType<typeof vi.fn>
  resolveSlot: ReturnType<typeof vi.fn>
  chapterAt: ReturnType<typeof vi.fn>
  credits: ReturnType<typeof vi.fn>
  free: ReturnType<typeof vi.fn>
}

/**
 * A fake `WasmSidecar`. Every method starts stubbed to a harmless default so
 * a test that only cares about one corner of the interface does not have to
 * name the rest — only the overrides it actually exercises.
 */
function fakeSidecar(overrides: Partial<FakeSidecar> = {}): FakeSidecar {
  return {
    editions: vi.fn(() => []),
    slotCount: vi.fn(() => undefined),
    slotAt: vi.fn(() => undefined),
    indexOf: vi.fn(() => undefined),
    resolveSlot: vi.fn(() => undefined),
    chapterAt: vi.fn(() => undefined),
    credits: vi.fn(() => new Uint32Array()),
    free: vi.fn(),
    ...overrides,
  }
}

// Each mirrors one of `taiyaku_wasm.d.ts`'s `WasmPosition`/`WasmSlot`/
// `WasmChapter` handles closely enough to exercise the real read-then-free
// shape in `resolver.ts`: a `free()` spy, so a test can assert the wasm-side
// handle was actually released rather than merely that a value came back.
const fakePosition = (page: number, half?: string) => ({
  page,
  half,
  free: vi.fn(),
})
const fakeSlot = (page: number, half: string | undefined, via: string) => ({
  page,
  half,
  via,
  free: vi.fn(),
})
const fakeChapter = (member: number, of: number, path: string) => ({
  member,
  of,
  path,
  free: vi.fn(),
})

beforeEach(() => {
  hoisted.init.mockClear()
  hoisted.fromJson.mockReset()
})

describe('wasmResolver', () => {
  it('parses the given json through WasmSidecar.fromJson', async () => {
    hoisted.fromJson.mockReturnValue(fakeSidecar())
    await wasmResolver('{"editions":{}}')
    expect(hoisted.fromJson).toHaveBeenCalledExactlyOnceWith('{"editions":{}}')
  })

  // The property `ensureWasmInit`/`once` exist for: two calls both go
  // through the shared memo, so wasm-bindgen's module `init()` runs exactly
  // once. A regression that called the raw `init` directly instead of the
  // memoised `ensureWasmInit` would still pass every other test in this file
  // — each of them only ever calls `wasmResolver` once — so this is the one
  // test that would catch it.
  //
  // `ensureWasmInit`'s memo is a module-level singleton, set the first time
  // any test in this file calls `wasmResolver` — by this point in the suite
  // that has already happened, so a fresh module instance is what lets this
  // test observe `init` from zero calls rather than inheriting another
  // test's already-settled promise. `vi.mock`'s registration survives
  // `resetModules` (it is not part of the module cache), so the re-import
  // still resolves `taiyaku-wasm` to the same `hoisted` fakes.
  it('runs the wasm module init exactly once across two calls', async () => {
    hoisted.fromJson.mockReturnValue(fakeSidecar())
    vi.resetModules()
    const fresh = await import('../src/resolver')
    await fresh.wasmResolver('{}')
    await fresh.wasmResolver('{}')
    expect(hoisted.init).toHaveBeenCalledTimes(1)
  })

  // "only the module initialisation is shared, not the parse" — each call
  // gets its own `WasmSidecar`, read from whichever fake `fromJson` was
  // told to return for that call.
  it('parses a fresh WasmSidecar per call, keyed by its own json', async () => {
    const first = fakeSidecar({ editions: vi.fn(() => ['en-mono']) })
    const second = fakeSidecar({ editions: vi.fn(() => ['ja-raw']) })
    hoisted.fromJson.mockReturnValueOnce(first).mockReturnValueOnce(second)

    const a = await wasmResolver('{"a":1}')
    const b = await wasmResolver('{"b":2}')

    expect(hoisted.fromJson).toHaveBeenNthCalledWith(1, '{"a":1}')
    expect(hoisted.fromJson).toHaveBeenNthCalledWith(2, '{"b":2}')
    expect(a.editions()).toEqual(['en-mono'])
    expect(b.editions()).toEqual(['ja-raw'])
  })

  describe('editions', () => {
    it('copies the wasm array rather than handing back its own reference', async () => {
      const wasmEditions = ['en-mono', 'ja-raw']
      hoisted.fromJson.mockReturnValue(
        fakeSidecar({ editions: vi.fn(() => wasmEditions) }),
      )
      const resolver = await wasmResolver('{}')
      const result = resolver.editions()
      expect(result).toEqual(wasmEditions)
      expect(result).not.toBe(wasmEditions)
    })
  })

  describe('slotCount', () => {
    it('passes the count straight through for a known edition', async () => {
      hoisted.fromJson.mockReturnValue(
        fakeSidecar({
          slotCount: vi.fn((edition: string) =>
            edition === 'en-mono' ? 187 : undefined,
          ),
        }),
      )
      const resolver = await wasmResolver('{}')
      expect(resolver.slotCount('en-mono')).toBe(187)
    })

    it('is undefined for an edition the sidecar does not know', async () => {
      hoisted.fromJson.mockReturnValue(fakeSidecar())
      const resolver = await wasmResolver('{}')
      expect(resolver.slotCount('missing')).toBeUndefined()
    })
  })

  describe('slotAt', () => {
    it('is undefined, with nothing to free, when the sidecar has no such slot', async () => {
      const sc = fakeSidecar()
      hoisted.fromJson.mockReturnValue(sc)
      const resolver = await wasmResolver('{}')
      expect(resolver.slotAt('en-mono', 999)).toBeUndefined()
    })

    // The conversion itself: a `WasmPosition` handle's `page`/`half` read
    // into a plain object, and the handle freed before the plain object is
    // handed back — a caller holding the `Position` must never be able to
    // reach into wasm memory through it.
    it('reads page and half into a plain Position and frees the handle', async () => {
      const p = fakePosition(4, 'R')
      hoisted.fromJson.mockReturnValue(
        fakeSidecar({
          slotAt: vi.fn((edition: string, index: number) =>
            edition === 'en-mono' && index === 8 ? p : undefined,
          ),
        }),
      )
      const resolver = await wasmResolver('{}')
      const position = resolver.slotAt('en-mono', 8)
      expect(position).toEqual({ page: 4, half: 'R' })
      expect(p.free).toHaveBeenCalledTimes(1)
    })

    it('carries an undefined half through for a whole page, not a spread half', async () => {
      const p = fakePosition(4, undefined)
      hoisted.fromJson.mockReturnValue(fakeSidecar({ slotAt: vi.fn(() => p) }))
      const resolver = await wasmResolver('{}')
      expect(resolver.slotAt('en-mono', 8)).toEqual({
        page: 4,
        half: undefined,
      })
    })
  })

  describe('indexOf', () => {
    it('unpacks a Position into edition/page/half for the sidecar call', async () => {
      const indexOf = vi.fn(() => 12)
      hoisted.fromJson.mockReturnValue(fakeSidecar({ indexOf }))
      const resolver = await wasmResolver('{}')
      const result = resolver.indexOf('en-mono', { page: 4, half: 'L' })
      expect(indexOf).toHaveBeenCalledExactlyOnceWith('en-mono', 4, 'L')
      expect(result).toBe(12)
    })

    it('is undefined when the sidecar finds no such position', async () => {
      hoisted.fromJson.mockReturnValue(fakeSidecar())
      const resolver = await wasmResolver('{}')
      expect(resolver.indexOf('en-mono', { page: 0 })).toBeUndefined()
    })
  })

  describe('resolve', () => {
    it('is undefined, with nothing to free, when there is no counterpart', async () => {
      hoisted.fromJson.mockReturnValue(fakeSidecar())
      const resolver = await wasmResolver('{}')
      expect(resolver.resolve('en-mono', 'ja-raw', { page: 0 })).toBeUndefined()
    })

    // Same read-then-free shape as `slotAt`, plus `via` — the field that
    // says which path the resolution walked, which `slotAt`'s `Position`
    // does not carry at all (see `Resolver`'s own doc on the distinction).
    it('reads page/half/via into a plain Landed and frees the handle', async () => {
      const r = fakeSlot(6, 'L', 'inverted')
      const resolveSlot = vi.fn(
        (from: string, to: string, page: number, half?: string) =>
          from === 'en-mono' && to === 'ja-raw' && page === 4 && half === 'R'
            ? r
            : undefined,
      )
      hoisted.fromJson.mockReturnValue(fakeSidecar({ resolveSlot }))
      const resolver = await wasmResolver('{}')
      const landed = resolver.resolve('en-mono', 'ja-raw', {
        page: 4,
        half: 'R',
      })
      expect(landed).toEqual({ page: 6, half: 'L', via: 'inverted' })
      expect(r.free).toHaveBeenCalledTimes(1)
    })

    it('passes an undefined half through resolveSlot for a whole-page slot', async () => {
      const resolveSlot = vi.fn(() => fakeSlot(1, undefined, 'identity'))
      hoisted.fromJson.mockReturnValue(fakeSidecar({ resolveSlot }))
      const resolver = await wasmResolver('{}')
      resolver.resolve('en-mono', 'ja-raw', { page: 1 })
      expect(resolveSlot).toHaveBeenCalledExactlyOnceWith(
        'en-mono',
        'ja-raw',
        1,
        undefined,
      )
    })
  })

  describe('chapterAt', () => {
    it('is undefined, with nothing to free, past the last chapter', async () => {
      hoisted.fromJson.mockReturnValue(fakeSidecar())
      const resolver = await wasmResolver('{}')
      expect(resolver.chapterAt('en-mono', 999)).toBeUndefined()
    })

    it('reads member/of/path into a plain Chapter and frees the handle', async () => {
      const c = fakeChapter(2, 5, 'v01/c03.zip')
      const chapterAt = vi.fn((edition: string, index: number) =>
        edition === 'en-mono' && index === 40 ? c : undefined,
      )
      hoisted.fromJson.mockReturnValue(fakeSidecar({ chapterAt }))
      const resolver = await wasmResolver('{}')
      const chapter = resolver.chapterAt('en-mono', 40)
      expect(chapter).toEqual({ member: 2, of: 5, path: 'v01/c03.zip' })
      expect(c.free).toHaveBeenCalledTimes(1)
    })
  })

  describe('credits', () => {
    // The sidecar hands back a `Uint32Array` (`taiyaku_wasm.d.ts`); the
    // reader-facing `Resolver` interface promises `number[]`. `Array.from`
    // is the conversion — pinned by checking the result is a real `Array`,
    // not merely by checking its contents, since a typed array and a plain
    // array can compare equal element-by-element while still being the
    // wrong thing to hand a caller expecting `Array.prototype` methods.
    it('converts the wasm Uint32Array into a plain number array', async () => {
      hoisted.fromJson.mockReturnValue(
        fakeSidecar({ credits: vi.fn(() => new Uint32Array([2, 5, 9])) }),
      )
      const resolver = await wasmResolver('{}')
      const result = resolver.credits('en-mono')
      expect(result).toEqual([2, 5, 9])
      expect(Array.isArray(result)).toBe(true)
    })
  })

  describe('dispose', () => {
    it('frees the underlying WasmSidecar', async () => {
      const sc = fakeSidecar()
      hoisted.fromJson.mockReturnValue(sc)
      const resolver = await wasmResolver('{}')
      resolver.dispose?.()
      expect(sc.free).toHaveBeenCalledTimes(1)
    })
  })
})

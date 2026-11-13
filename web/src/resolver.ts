// biome-ignore-all lint/suspicious/noTsIgnore: the `@ts-ignore` below must stay
// `@ts-ignore` and not become `@ts-expect-error` — see the comment on it, and
// `docs/ci.md`. Biome's suggested fix breaks the build in the state where
// `crates/taiyaku-wasm/pkg` exists. The rule also fires a second time on the
// prose above the directive, which merely names the token.
import { once } from './once'

// `taiyaku-wasm` is a `file:` dependency on the gitignored, generated
// `crates/taiyaku-wasm/pkg/` (see docs/ci.md). In a fresh clone or CI, that
// directory does not exist yet, so `npm ci` leaves `node_modules/taiyaku-wasm`
// a dangling symlink and this import has no module to resolve against —
// `tsc` reports TS2307 here, and only here, in that state. `@ts-ignore`
// rather than `@ts-expect-error`: it must be silent (not itself an error)
// once `./scripts/build-wasm.sh` has run and the import resolves normally,
// which is the state every real build and every human editing this file is
// in. The rest of this file still typechecks fully against the real
// generated `.d.ts` whenever `pkg/` exists — this line is the only thing
// that is ever skipped, not a project-wide type-check bypass.
// @ts-ignore
import init, { WasmSidecar } from 'taiyaku-wasm'

/** Where a slot sits inside one edition. */
export interface Position {
  page: number
  half?: 'L' | 'R'
}

/** Where a slot landed, and which path the answer walked. */
export interface Landed extends Position {
  via: 'identity' | 'direct' | 'inverted' | 'hub'
}

/**
 * One page of one member, as the wire carries it (O240).
 *
 * The anchor a note or a position is written with, in the core's
 * `MemberPage` wire form: `half` is `left`/`right`, not `Position`'s `L`/`R`,
 * so the object goes onto the wire as it is and is never converted.
 */
export interface At {
  sha256: string
  nth: number
  page: number
  half?: 'left' | 'right'
}

/** Which chapter a slot falls in: `member`'s zero-based index among `of`. */
export interface Chapter {
  member: number
  of: number
  path: string
}

/**
 * Everything the reading model needs from a sidecar.
 *
 * An interface rather than the WASM type directly, for two reasons. It lets
 * every rule in `reading.ts` and `pairing.ts` be tested with a hand-written
 * fake — no WASM, no async init, no CI toolchain. And it is the seam a native
 * shell substitutes at, which is the same reason `PageSource` is a trait.
 *
 * `indexOf` and `resolve` can throw. The WASM implementation rejects a `half`
 * outside `'L' | 'R' | undefined` by throwing rather than answering
 * `undefined` — a malformed value crossing this boundary is a real defect,
 * not an ordinary gap, and the two must not collapse into the same answer.
 * Whoever implements this interface for a native shell needs to preserve
 * that distinction rather than swallow it.
 */
export interface Resolver {
  editions(): string[]
  slotCount(edition: string): number | undefined
  slotAt(edition: string, index: number): Position | undefined
  indexOf(edition: string, at: Position): number | undefined
  resolve(from: string, to: string, at: Position): Landed | undefined
  chapterAt(edition: string, index: number): Chapter | undefined
  /**
   * The page a slot shows, as the member, occurrence and page that hold it:
   * the anchor the server places a note or a position by, whatever
   * renumbering comes before the write. Computed by the core
   * (`Edition::member_page`), never in TypeScript.
   */
  memberPage(edition: string, index: number): At | undefined
  /**
   * The edition's credit pages, as **page** indices — the sidecar's own
   * `credits` field, which is the page-sequence kind like `spreads`.
   * `modules/credits.ts` converts to slots where it draws.
   */
  credits(edition: string): number[]
  /**
   * Frees whatever native resource backs this resolver, if any. Optional —
   * the hand-written fakes `reading.test.ts`/`session.test.ts`/etc. build
   * (`fixtures.ts`'s `fakeResolver`) have nothing to free, and requiring
   * every one of them to grow a no-op implementation would be pure ceremony
   * for a distinction that only matters on the WASM side.
   *
   * A caller that replaces a resolver mid-session — `App.tsx` does, after a
   * correction lands and after a stale-`ETag` re-read — must call the
   * outgoing one's `dispose` before discarding it, or (for `wasmResolver`,
   * below) its `WasmSidecar` is stranded until the tab closes. See
   * `wasmResolver`'s own doc for why this exists.
   */
  dispose?(): void
}

/**
 * Runs wasm-bindgen's module `init()` exactly once, however many times
 * `wasmResolver` is called — see `once.ts` for why the memo has to be
 * synchronous to actually close the race.
 *
 * `init()`'s own re-entry guard (`if (wasm !== undefined) return wasm`) is
 * synchronous only: it does nothing for two calls that are both already
 * in-flight when it runs. React StrictMode's double-invoked effects produce
 * exactly that — two concurrent `wasmResolver` calls — and without this memo
 * each one instantiates its own `WebAssembly.Instance`, and both finish by
 * overwriting the same module-global `wasm` binding. Whichever finishes last
 * wins, silently invalidating every `WasmSidecar` handle resolved against the
 * instance that finished first — including one already installed in caller
 * state and in active use, which is how a resolver that was working a moment
 * earlier starts throwing `RuntimeError: memory access out of bounds`.
 *
 * The fix has to live here and not in the caller: a caller can only skip a
 * *redundant* call it knows about (e.g. one it's about to discard), and that
 * still leaves two genuinely concurrent callers — both wanting a resolver,
 * neither redundant — racing the same way.
 */
const ensureWasmInit = once(init)

/**
 * Parse a sidecar into a `Resolver`. Safe to call concurrently — see
 * `ensureWasmInit` above — but each call still parses its own `WasmSidecar`
 * from the module instance that memo settles on; only the module
 * initialisation is shared, not the parse.
 *
 * The returned `WasmSidecar` (`sc`) is freed by the returned `dispose`, not
 * implicitly. Every correction that lands, and every stale-`ETag` re-read,
 * calls `wasmResolver` again and replaces the resolver in state, which would
 * strand the previous call's `sc` (and, since it also caches a `Route`, more
 * than just the base handle) for as long as the tab stays open. `App.tsx`
 * calls the outgoing resolver's `dispose?.()` at both of those call sites,
 * before installing the new one.
 *
 * `slotAt`, `resolveSlot`, `chapterAt` and `memberPage` each return a fresh
 * handle — unlike `sc`, called once per resolution rather than once per
 * session, so a reader loop would otherwise leak one per keypress. Their
 * fields are read into a plain object and the wasm-side handle is freed
 * immediately after, before returning.
 */
export async function wasmResolver(json: string): Promise<Resolver> {
  await ensureWasmInit()
  const sc = WasmSidecar.fromJson(json)
  return {
    editions: () => [...sc.editions()],
    slotCount: (edition) => sc.slotCount(edition),
    slotAt: (edition, index) => {
      const p = sc.slotAt(edition, index)
      if (p === undefined) return undefined
      const position: Position = {
        page: p.page,
        half: p.half as 'L' | 'R' | undefined,
      }
      p.free()
      return position
    },
    indexOf: (edition, at) => sc.indexOf(edition, at.page, at.half),
    resolve: (from, to, at) => {
      const r = sc.resolveSlot(from, to, at.page, at.half)
      if (r === undefined) return undefined
      const landed: Landed = {
        page: r.page,
        half: r.half as 'L' | 'R' | undefined,
        via: r.via as Landed['via'],
      }
      r.free()
      return landed
    },
    chapterAt: (edition, index) => {
      const c = sc.chapterAt(edition, index)
      if (c === undefined) return undefined
      const chapter: Chapter = { member: c.member, of: c.of, path: c.path }
      c.free()
      return chapter
    },
    // Copied field by field: the handle itself stringifies to its pointer,
    // so it cannot go on the wire.
    memberPage: (edition, index) => {
      const a = sc.memberPage(edition, index)
      if (a === undefined) return undefined
      const at: At = {
        sha256: a.sha256,
        nth: a.nth,
        page: a.page,
        half: a.half as At['half'],
      }
      a.free()
      return at
    },
    credits: (edition) => Array.from(sc.credits(edition)),
    dispose: () => sc.free(),
  }
}

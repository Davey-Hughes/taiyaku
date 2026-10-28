/**
 * An in-memory `Storage`, for tests that touch preferences.
 *
 * Neither test environment provides a real one: the `node` project has no DOM
 * at all, and the `jsdom` project — despite the name — leaves `localStorage`
 * `undefined`, which is easy to miss because `prefs.ts` swallows the failure by
 * design. A component that saved and restored through it therefore passed its
 * tests while persisting nothing, until something asserted the value came back.
 *
 * Stubbed with `vi.stubGlobal('localStorage', fakeStorage())` and cleared with
 * `vi.unstubAllGlobals()`, which is the shape `prefs.test.ts` established for
 * its own tests and this file extracts so the two cannot drift.
 */
export function fakeStorage(): Storage {
  const map = new Map<string, string>()
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => {
      map.delete(k)
    },
    setItem: (k, v) => {
      map.set(k, v)
    },
  }
}

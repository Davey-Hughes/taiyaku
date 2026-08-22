/**
 * What the library's chrome knows, and how a screen under it asks.
 *
 * Separate from `Shell.tsx`'s component for the reason `gate.ts` states: a
 * non-component export sharing a file with a component defeats React Fast
 * Refresh, and biome's `style/useComponentExportOnlyModules` refuses it.
 */
import { createContext, useContext } from 'react'
import type { LibraryStatus } from './library'

/**
 * `undefined` where no shell sits above the reader of this context, and
 * **also while the shell's own fetch is still in flight** — the two are
 * deliberately one value. A screen that has to tell them apart is a screen
 * asking the chrome a question the chrome cannot answer.
 *
 * **`Library` does not wait for it.** Its empty-state branch only fires once
 * both `gridData` and `status` are defined; until then it falls through to
 * the plain `<ul className="grid">`, which — with no rows yet — renders
 * empty. That was already true before this shell existed: `gridData` alone
 * used to gate that branch, and was `undefined` for exactly as long, which is
 * why the `gridData?.unfiled` optional chaining still in that branch exists —
 * it dates from when the branch had to tolerate being read before any data
 * had arrived. This shell only widens that window, for the case where the
 * status request is slower than the grid request. That is acceptable because
 * an empty `<ul>` and a not-yet-rendered empty state look identical — both
 * are nothing on screen — and the empty-state screens are a first-run
 * concern, not something a reader watches for on every visit.
 */
export const StatusContext = createContext<LibraryStatus | undefined>(undefined)

/** The library's roots and whether anything has scanned them, once known. */
export function useLibraryStatus(): LibraryStatus | undefined {
  return useContext(StatusContext)
}

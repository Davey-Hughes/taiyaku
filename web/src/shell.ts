/**
 * What the library's chrome knows, and how a screen under it asks.
 *
 * Separate from `Shell.tsx`'s component for the reason `gate.ts` states: a
 * non-component export sharing a file with a component defeats React Fast
 * Refresh, and biome's `style/useComponentExportOnlyModules` refuses it.
 */
import { createContext, useContext } from 'react'
import type { LibraryStatus } from './library'

/** What `StatusContext` holds where the status read failed, none kept. */
export const STATUS_FAILED = 'failed'

/**
 * `undefined` where no shell sits above the reader of this context, and
 * **also while the shell's own fetch is still in flight** — the two are
 * deliberately one value. A screen that has to tell them apart is a screen
 * asking the chrome a question the chrome cannot answer.
 *
 * **`Library`'s grid does not wait for it.** It draws `PageLoading` until its
 * own reads have answered. Its empty-state branch, but for a `?library=` that
 * is no library id, only fires once `status` is defined too, so a grid that
 * answers with no rows while the status is still out falls through to the
 * plain `<ul className="grid">`, which renders empty. That is acceptable
 * because an empty `<ul>` and a not-yet-rendered empty state look identical —
 * both are nothing on screen — and the empty state is drawn as soon as the
 * status lands.
 *
 * **`STATUS_FAILED` where the shell's read failed with no status kept.** It
 * holds until a read of the status answers, so a screen waiting on the
 * status, such as `Library`'s search area before it offers the box, stops
 * waiting rather than wait on a read that may never be sent; `useStatusFailed`
 * tells it so. `useLibraryStatus` reads it as `undefined`, so a screen that
 * only draws the status sees the three states as one.
 */
export const StatusContext = createContext<
  LibraryStatus | typeof STATUS_FAILED | undefined
>(undefined)

/** The libraries this account may read, and whether each has been scanned, once known. */
export function useLibraryStatus(): LibraryStatus | undefined {
  const status = useContext(StatusContext)
  return status === STATUS_FAILED ? undefined : status
}

/** The shell's status read failed, and no status is kept to draw. */
export function useStatusFailed(): boolean {
  return useContext(StatusContext) === STATUS_FAILED
}

/**
 * Ask the chrome to re-read the sidebar's open-proposals count.
 *
 * The default is a no-op, for the same reason `StatusContext`'s is
 * `undefined`: a screen mounted with no shell above it — which is what every
 * test of one screen is — must not have to know that.
 *
 * **This exists because the two numbers are on screen together.** The inbox
 * re-reads its own list after an accept or a dismiss; the sidebar's count is
 * fetched once, when the shell mounts. Without a way to say "that write moved
 * your number too", accepting a proposal leaves a badge reading 3 beside a
 * header reading 2, on the same screen, until a navigation. The alternative —
 * having the inbox *publish* its own total upward — was not taken: the inbox
 * asks at whichever threshold "Show weak proposals" is currently on, so
 * leaving the screen with weak shown would leave the badge counting a set
 * pressing it no longer produces.
 */
export const InboxRefreshContext = createContext<() => void>(() => {})

/** Re-read the open-proposals count after a write that moved it. */
export function useInboxRefresh(): () => void {
  return useContext(InboxRefreshContext)
}

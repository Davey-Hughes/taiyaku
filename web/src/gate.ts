/**
 * What the sign-in gate knows, and how anything under it asks.
 *
 * Separate from `Gate.tsx`'s component for the reason `tiles.ts` states: a
 * non-component export sharing a file with a component defeats React Fast
 * Refresh, and biome's `style/useComponentExportOnlyModules` refuses it. The
 * split follows the same seam this codebase draws everywhere else — `api.ts`
 * beside `Account.tsx`, `library.ts` beside `Library.tsx`.
 */
import { createContext, useContext } from 'react'
import type { Me } from './api'

/** What the gate knows, offered to everything it renders. */
export interface Gated {
  /** Who the server resolved this browser to, and what it may do. */
  who: Me
  /**
   * Back to the sign-in screen — the account panel's "Sign out" button, and
   * nothing else. **Not the 401 path**: `api.ts`'s `whenSignedOut` carries
   * that, and the gate subscribes to it itself precisely so no screen has to
   * remember to.
   */
  signOut: () => void
}

/**
 * `undefined` where no gate sits above the reader of this context.
 *
 * Exported for `Gate.tsx`, which provides it, and for the tests of the screens
 * that read it — which mount one screen rather than the whole app, and so have
 * to supply the account themselves.
 */
export const GateContext = createContext<Gated | undefined>(undefined)

/**
 * The account this browser is signed in as, or `undefined` where no gate sits
 * above the caller.
 *
 * `undefined` means "no gate", which in this app is a test that mounted one
 * screen on its own — `main.tsx` puts every route under one. It is deliberately
 * not a permission check: what an account may *do* is enforced by the server on
 * every request, and a screen reading `who.admin` is choosing what to *show*.
 * Getting `undefined` therefore has to be safe, and the safe reading is "no
 * account known", never "an admin".
 */
export function useGate(): Gated | undefined {
  return useContext(GateContext)
}

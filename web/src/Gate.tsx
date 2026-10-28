/**
 * The sign-in gate: the one place a 401 is noticed, above the router.
 *
 * **This used to live inside `App`, and that is exactly why it had to move.**
 * `main.tsx` renders `<AppRoutes>` since spec C1 §11 step 2, and `/` is the
 * library rather than the reader — so a signed-out visitor landing on `/`
 * reached a screen that had never registered `api.ts`'s `whenSignedOut` hook,
 * saw `library.ts`'s 401 turn into "not signed in" in an alert banner, and had
 * nowhere to go: `<Navigate to="/" replace/>` sends every unknown path back to
 * the same place, and the only `<Login>` in the app was mounted inside a reader
 * you had to already know a slug to reach. Spec C1 §5 says a 401 routes to the
 * login screen; §10's test list names it too.
 *
 * Three properties follow from the gate sitting above the router rather than
 * inside a screen:
 *
 * - **One registration of `whenSignedOut`.** That hook is a single module-level
 *   slot (see its doc for the bug that made it module-level in the first
 *   place), so two components registering means the last one to mount silently
 *   wins. One gate above every route is the only shape where that cannot
 *   happen.
 * - **Signing out unmounts everything below.** `App` used to reset thirteen
 *   pieces of reader state by hand on sign-out, and cancel two debounced
 *   writers — three whole-branch reviews found items missing from that list,
 *   the last of them three at once, among them a pending correction that would
 *   have written the previous reader's pairing into the shared sidecar under
 *   the next reader's session. An unmount cannot forget a field.
 *   The two debounced writers, which unmounting alone would leave armed, are
 *   flushed by `hooks.ts`'s `useFlushOnHide` cleanup — at the unmount commit,
 *   which is before any next account can possibly have signed in, and against
 *   a session the server has just ended.
 * - **`me()` is asked once per page load, not once per screen.** Its answer is
 *   also the account's capabilities, which is what `Library` reads to decide
 *   whether the roots list belongs on screen at all.
 *
 * **Nothing here may import `App`.** `App` value-imports `resolver.ts`, which
 * value-imports `taiyaku-wasm` out of a gitignored directory — a module that
 * reaches it passes locally and breaks CI and every fresh clone. The gate takes
 * `children`, so the composition happens in `main.tsx` and this file stays
 * importable on its own.
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Unauthenticated, me, whenSignedOut, type Me } from './api'
import { GateContext } from './gate'
import { Login } from './Login'
import { emptyReads } from './reads'

export interface GateProps {
  children: ReactNode
}

export function Gate({ children }: GateProps) {
  // `undefined` — not asked yet, so nothing is drawn. `null` — asked, and
  // signed out. A `Me` — signed in, and its capabilities are the context value.
  // Three states rather than a boolean beside an account, because two pieces of
  // state can disagree and this cannot.
  const [who, setWho] = useState<Me | null | undefined>(undefined)

  // Which `me()` answer is still wanted. Bumped by every ask and by teardown,
  // so a slow first answer cannot overwrite the fast answer to a later ask —
  // which is exactly what signing in does: `Login` calls `ask()` again while
  // the original may still be in flight under StrictMode's double mount.
  const asked = useRef(0)

  const ask = useCallback(() => {
    asked.current += 1
    const id = asked.current
    me()
      .then((account) => {
        if (asked.current !== id) return
        // The read store outlives the unmount below the gate, and what it
        // keeps was read for the account that read it.
        emptyReads()
        setWho(account)
      })
      .catch((why: unknown) => {
        if (asked.current !== id) return
        // Anything that is not "signed out" is still a reason not to load, but
        // it is worth seeing: a server that is down and a server that wants a
        // password look the same from a login screen otherwise.
        if (!(why instanceof Unauthenticated)) console.error(why)
        setWho(null)
      })
  }, [])

  // The one sign-out path, a 401's included: emptied here, so an answer still
  // in flight for the account signing out is dropped when it lands.
  const signOut = useCallback(() => {
    emptyReads()
    setWho(null)
  }, [])

  // Registered before the first `me()` below, and by declaration order rather
  // than by hope: React runs a component's effects top to bottom, so the hook
  // is in place before anything can 401 through it.
  useEffect(() => {
    whenSignedOut(signOut)
  }, [signOut])

  useEffect(() => {
    ask()
    return () => {
      asked.current += 1
    }
  }, [ask])

  // **Nothing is drawn until sign-in is known.** Rendering the library first
  // and replacing it once `me()` answers would flash a broken grid at every
  // signed-out visitor, and `undefined` here is "not asked yet" rather than a
  // third state anyone below has to handle.
  if (who === undefined) return null
  // `ask()` rather than an optimistic `setWho`: the sign-in response carries no
  // account, only whether it worked, so the capabilities have to be read from
  // the server that just issued the session. `Login` stays on screen for that
  // one round trip, which is a form that has not gone away yet rather than a
  // blank page that has.
  if (who === null) return <Login onSignedIn={ask} />
  return <GateContext value={{ who, signOut }}>{children}</GateContext>
}

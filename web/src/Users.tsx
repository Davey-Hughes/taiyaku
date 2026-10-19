/**
 * The users screen — spec C4 §4 and the build design's §6.2 — and the pages
 * beneath it: the table of accounts, the form that makes one, and an
 * account's own page.
 *
 * **The screen owns who exists; an account's page owns what it is doing.**
 * `Libraries`' split, and the same reason: a patch, a revocation and a dry
 * run belong to the account being administered. Each page reads the list
 * this screen holds and picks its account by id; there is no route that
 * answers one account. The list is read when the screen mounts, again on
 * each move between its pages, and after every write the server carries out.
 *
 * **The libraries list is fetched here and handed down**, on mount and again
 * on each move between pages, as the list of accounts is. `libraries.ts`
 * already has the client — one client and two screens rather than a second
 * fetch that could disagree (§6.2). **A failed read is drawn**,
 * because a grant editor with nothing to offer looks the same whether there
 * are no libraries or the list could not be read, and only one of those is
 * true.
 *
 * **The caller's own account comes from the gate, not from a second request.**
 * `useGate()` already holds `me()`'s answer for the page load. `undefined`
 * there means "no account known" and is safe: the server refuses a
 * self-demotion whether or not this screen drew the control.
 *
 * **An account's page is keyed by name rather than id.** `user.id` is
 * `INTEGER PRIMARY KEY` with no `AUTOINCREMENT`, so SQLite reissues a freed id
 * to the next account created — and React matching on `key` would hand a
 * recycled id the removed account's page instance, with its open removal
 * dialog and typed password. `user.name` is `UNIQUE`; a rename remounts the
 * page, which is the right thing to lose.
 *
 * **What a write takes away is dropped from what is drawn at once**, in the
 * same transition as the move back to the table, so the page that asked is
 * never drawn over an account that is gone.
 *
 * **An answer that comes once the person has gone to another page of the
 * screen is said on the page they are on**, as `configPages.tsx` says for
 * both screens: a removal's line, or the refusal of any write whose page or
 * form went before its answer came.
 */
import {
  type ReactNode,
  startTransition,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'
import {
  Link,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
} from 'react-router'
import { AccountPage } from './AccountPage'
import { useLateAnswers } from './configPages'
import { useGate } from './gate'
import { type Library, listLibraries } from './libraries'
import { sentenceOf } from './message'
import { Missing } from './Missing'
import { NewAccount } from './NewAccount'
import { UserTable } from './UserTable'
import { listUsers, type User } from './users'

const TABLE = '/config/users'

/**
 * The account the last removal took, and the page its line is drawn on: the
 * table the removal went back to, or the page the person had gone to before
 * it answered.
 */
interface Gone {
  at: string
  name: string
  /** Taken when the answer itself moved the person to the table. */
  focus: boolean
}

export function Users() {
  const [users, setUsers] = useState<User[] | undefined>()
  const [libraries, setLibraries] = useState<Library[] | undefined>()
  const [librariesFailure, setLibrariesFailure] = useState<string | undefined>()
  /** Why the list could not be read. Separate from a form's refusal. */
  const [failure, setFailure] = useState<string | undefined>()
  /** The newest `listUsers` call — responses need not arrive in order. */
  const latest = useRef(0)
  const gate = useGate()
  const { pathname } = useLocation()
  const navigate = useNavigate()

  /**
   * Re-read the list, and **hand the promise back**.
   *
   * An account's page holds its own controls until the reload it triggered
   * has landed — `AccountPage`'s `settled` is the other end of this —
   * because between a write's 200 and this answer the page is still drawing
   * its pre-write props with its controls live. Returning the promise is
   * what lets it wait without knowing anything about this screen's state.
   */
  const reload = useCallback(() => {
    const seq = ++latest.current
    return listUsers()
      .then((body) => {
        if (seq !== latest.current) return
        setUsers(body.users)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure(sentenceOf(err))
      })
  }, [])

  // biome-ignore lint/correctness/useExhaustiveDependencies: `pathname` is the trigger: each page reads the list afresh.
  useEffect(() => {
    void reload()
  }, [reload, pathname])

  const goneEl = useRef<HTMLParagraphElement>(null)
  const {
    place,
    shown,
    setLine: setGone,
    say,
    arrived,
    setArrival,
    cancel,
    drawNotices,
  } = useLateAnswers<Gone>(TABLE, goneEl)

  // biome-ignore lint/correctness/useExhaustiveDependencies: `pathname` is the trigger: each page reads the list afresh.
  useEffect(() => {
    // A read a later page's read has replaced is dropped: the two need not
    // answer in order.
    let cancelled = false
    listLibraries()
      .then((body) => {
        if (cancelled) return
        setLibraries(body.libraries)
        setLibrariesFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        // The list an earlier read brought goes too: the alert says the
        // editor shows nothing, and the editor then says it has not been
        // read rather than offering libraries as they were.
        setLibraries(undefined)
        setLibrariesFailure(sentenceOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [pathname])

  /**
   * An account was made. The person is taken to its page only while the
   * form that asked is still drawn; an answer that comes after they left it,
   * even for a fresh form at the same address, leaves them where they are.
   * The list is read once either way, by the change of page or here.
   */
  const added = useCallback(
    (user: User, stillHere: boolean) => {
      startTransition(() => {
        // Drawn from the add's own answer, so the account's page has it
        // whether or not the read that follows lands.
        setUsers((held) =>
          held === undefined
            ? [user]
            : [...held.filter((one) => one.id !== user.id), user],
        )
        if (stillHere) {
          setGone(undefined)
          setArrival(`${TABLE}/${user.id}`)
          navigate(`${TABLE}/${user.id}`)
        }
      })
      if (!stillHere) void reload()
    },
    [setGone, setArrival, navigate, reload],
  )

  /**
   * An account sent from the page `from` was removed. The person goes back
   * to the table only while on `from`; an answer that comes after they went
   * elsewhere leaves them where they are, with the line drawn on that page.
   * The account's page has already asked for the list again.
   */
  const removed = useCallback(
    (from: string, id: number, name: string) => {
      const { sent, line } = place(from, { at: TABLE, name })
      startTransition(() => {
        setGone(line)
        setUsers((held) => held?.filter((one) => one.id !== id))
        if (sent) navigate(TABLE)
      })
    },
    [place, setGone, navigate],
  )

  const goneLine = shown !== undefined && (
    <p ref={goneEl} className="gone-line" tabIndex={-1}>
      Removed {shown.name}.
    </p>
  )

  const notices = drawNotices(
    failure !== undefined && <p role="alert">{failure}</p>,
    goneLine,
  )
  // Handed to an account's page alone: it speaks of the grant editor below,
  // and no other page draws one.
  const grantNotices: ReactNode = (
    <>
      {notices}
      {librariesFailure !== undefined && (
        <p role="alert">
          The libraries list could not be read ({librariesFailure}), so the
          grant editor below shows nothing rather than what there is.
        </p>
      )}
    </>
  )

  return (
    <section className="users">
      <Routes>
        <Route
          index
          element={
            <UserTable
              users={users}
              actingAs={gate?.who.user_id}
              notices={notices}
              arrived={arrived}
            />
          }
        />
        <Route
          path="new"
          element={
            <NewAccount
              notices={notices}
              onAdded={added}
              onLateRefusal={say}
              onCancel={cancel}
            />
          }
        />
        <Route
          path=":id"
          element={
            <AccountAt users={users} notices={notices}>
              {(user, all) => (
                <AccountPage
                  key={user.name}
                  user={user}
                  users={all}
                  libraries={libraries}
                  actingAs={gate?.who.user_id}
                  notices={grantNotices}
                  arrived={arrived}
                  onChanged={reload}
                  onLateRefusal={say}
                  onRemoved={(name) => removed(pathname, user.id, name)}
                />
              )}
            </AccountAt>
          }
        />
        <Route path="*" element={<Navigate to={TABLE} replace />} />
      </Routes>
    </section>
  )
}

/**
 * The account the path names, or a line saying there is none. Nothing is
 * drawn while the list has not been read: the failure, if it failed, is.
 */
function AccountAt({
  users,
  notices,
  children,
}: {
  users: User[] | undefined
  notices: ReactNode
  children: (user: User, users: User[]) => ReactNode
}) {
  const { id } = useParams()
  if (users === undefined) return notices
  const user = users.find((one) => String(one.id) === id)
  if (user === undefined) {
    return (
      <div className="user-row">
        <p className="back">
          <Link to={TABLE}>‹ Users</Link>
        </p>
        {notices}
        <Missing key={id}>No such account.</Missing>
      </div>
    )
  }
  return children(user, users)
}

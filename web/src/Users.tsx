/**
 * The users screen — spec C4 §4 and the build design's §6.2 — and the pages
 * beneath it: the table of accounts, the form that makes one, and an
 * account's own page.
 *
 * **The screen owns who exists; an account's page owns what it is doing.**
 * `Libraries`' split, and the same reason: a patch, a revocation and a dry
 * run belong to the account being administered. Each page reads the list
 * this screen holds and picks its account by the name its address spells, as
 * `configAddress.ts` finds it; there is no route that answers one account.
 * The list is the read store's (`pageReads.ts`'s `USERS`): read when the
 * screen mounts, again on each move between its pages, and after every write
 * the server carries out. Until it has been read, the table and an
 * account's page draw their header and, after a while, "Loading…" in their
 * body's place, and an account's page names itself from its address; the
 * make page draws its form at once.
 *
 * **The libraries list is read here and handed down**, in one set with the
 * list of accounts, so a page waits for both and an account's grant editor
 * is never drawn before the list it offers. It is the store's `LIBRARIES`,
 * the list Configuration → Libraries draws: one read for two screens rather
 * than a second that could disagree (§6.2). **A failed read is drawn**,
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
  Navigate,
  Route,
  Routes,
  useLocation,
  useMatch,
  useNavigate,
  useParams,
} from 'react-router'
import { AccountPage } from './AccountPage'
import {
  accountPath,
  byName,
  NEW_ACCOUNT,
  nameAt,
  USERS,
} from './configAddress'
import { useLateAnswers, usePageKey } from './configPages'
import { useGate } from './gate'
import type { Library } from './libraries'
import { sentenceOf } from './message'
import { Missing } from './Missing'
import { NewAccount } from './NewAccount'
import { LIBRARIES, USERS as USER_LIST } from './pageReads'
import { invalidate } from './reads'
import { PageHeader } from './settingsLayout'
import { SideBySide } from './SideBySide'
import { usePage } from './usePage'
import { UserTable } from './UserTable'
import type { User } from './users'

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
  const {
    ready,
    answers: [usersRead, librariesRead],
    // Why each list could not be read, once this visit has read it: no
    // failure kept from an earlier visit is handed over. The list's is apart
    // from a form's refusal.
    failures: [failure, librariesFailure],
    loadingDrawn,
  } = usePage<[{ users: User[] }, { libraries: Library[] }]>([
    USER_LIST,
    LIBRARIES,
  ])
  /** The store's list of accounts, once the page may draw it. */
  const listed = ready ? usersRead?.users : undefined
  /**
   * What is drawn: the store's list, with what a write's answer made or took
   * away laid over it until the store holds another list. A read that fails
   * leaves the store's list as it was, so what the write changed stays
   * drawn.
   */
  const [users, setUsers] = useState(listed)
  const [drawnFrom, setDrawnFrom] = useState(listed)
  if (listed !== drawnFrom) {
    setDrawnFrom(listed)
    setUsers(listed)
  }
  /**
   * The libraries list, once the page may draw it. A read that fails takes
   * the list an earlier one brought with it: the alert says the grant editor
   * shows nothing, and the editor then says the list has not been read
   * rather than offering libraries as they were.
   */
  const libraries =
    ready && librariesFailure === undefined
      ? librariesRead?.libraries
      : undefined
  const gate = useGate()
  const { pathname } = useLocation()
  const navigate = useNavigate()

  /**
   * Re-read the list, and **hand the promise back**: the store's, which
   * resolves once the read has landed.
   *
   * An account's page holds its own controls until the reload it triggered
   * has landed — `AccountPage`'s `settled` is the other end of this —
   * because between a write's 200 and this answer the page is still drawing
   * its pre-write props with its controls live. Returning the promise is
   * what lets it wait without knowing anything about this screen's state.
   */
  const reload = useCallback(() => invalidate(USER_LIST), [])

  /** The page the effect below last saw. */
  const pageSeen = useRef(pathname)
  // Each page reads both lists afresh. The first page's read is `usePage`'s.
  useEffect(() => {
    if (pageSeen.current === pathname) return
    pageSeen.current = pathname
    void invalidate(USER_LIST, LIBRARIES)
  }, [pathname])

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
  } = useLateAnswers<Gone>(USERS, goneEl)

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
          setArrival(accountPath(user.name))
          navigate(accountPath(user.name))
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
      const { sent, line } = place(from, { at: USERS, name })
      startTransition(() => {
        setGone(line)
        setUsers((held) => held?.filter((one) => one.id !== id))
        if (sent) navigate(USERS)
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
    failure !== undefined && <p role="alert">{sentenceOf(failure)}</p>,
    goneLine,
  )
  // Handed to an account's page alone: it speaks of the grant editor below,
  // and no other page draws one, nor that page while it has no account.
  const grantNotices: ReactNode = (
    <>
      {notices}
      {librariesFailure !== undefined && users !== undefined && (
        <p role="alert">
          The libraries list could not be read ({sentenceOf(librariesFailure)}
          ), so the grant editor below shows nothing rather than what there is.
        </p>
      )}
    </>
  )

  /**
   * The make page, whose address `Config` routes to this screen outside the
   * table's, so that the accounts' names are the only segments beneath it.
   */
  const making = useMatch(NEW_ACCOUNT) !== null
  /** The address goes beyond the table's: an account's page. */
  const beyond = (useParams()['*'] ?? '') !== ''
  /**
   * Whether a page is open, the make form or an account's: either is the end
   * of this screen's tree, which `SideBySide` draws beside the table where
   * the width allows and alone where it does not.
   */
  const open = making || beyond
  /**
   * The account the address names, found in the list as `byName` finds
   * names, and read from the address as the browser holds it, for the reason
   * `configAddress.ts` gives. `undefined` while the list is unread, and where
   * the address names no account.
   */
  const named = nameAt(pathname, USERS)
  const account =
    named === undefined || users === undefined
      ? undefined
      : byName(users, named.name)

  return (
    <section className="users">
      <SideBySide
        list={
          <UserTable
            users={users}
            loadingDrawn={loadingDrawn}
            actingAs={gate?.who.user_id}
            // The screen says each thing once: on the page open beside the
            // table where there is one.
            notices={open ? undefined : notices}
            arrived={arrived && !open}
            opened={account?.id}
            onOpen={setArrival}
          />
        }
        page={
          making ? (
            <NewAccount
              users={users}
              notices={notices}
              arrived={arrived}
              onAdded={added}
              onLateRefusal={say}
              onCancel={cancel}
            />
          ) : open ? (
            <Routes>
              <Route
                path=":name"
                element={
                  <AccountAt
                    // Once both lists are in: a make's answer may lay an
                    // account over the list before the libraries list is.
                    users={ready ? users : undefined}
                    account={account}
                    name={named?.name}
                    notices={notices}
                  >
                    {(user, all, key) => (
                      <AccountPage
                        // A page of its own for each account (`usePageKey`).
                        key={key}
                        user={user}
                        users={all}
                        loadingDrawn={loadingDrawn}
                        libraries={libraries}
                        actingAs={gate?.who.user_id}
                        notices={grantNotices}
                        arrived={arrived}
                        onChanged={reload}
                        onLateRefusal={say}
                        onRemoved={
                          user && ((name) => removed(pathname, user.id, name))
                        }
                      />
                    )}
                  </AccountAt>
                }
              />
              <Route path="*" element={<Navigate to={USERS} replace />} />
            </Routes>
          ) : null
        }
      />
    </section>
  )
}

/**
 * The account's page, or a page saying there is none, whose line takes
 * focus. `children` draws the account's page from the moment the screen is
 * drawn, with the account `undefined` while the lists are read or no read
 * of the list of accounts has answered, so that the header drawn then is the
 * one kept (`usePageKey`). The page draws the list's failure under that
 * header.
 */
function AccountAt({
  users,
  account,
  name,
  notices,
  children,
}: {
  /**
   * `undefined` while the page waits for the lists, or where no read of this
   * one has answered.
   */
  users: User[] | undefined
  /** The account `name` finds in `users`. */
  account: User | undefined
  /** The name the address spells, so another such address says so afresh. */
  name: string | undefined
  notices: ReactNode
  children: (
    user: User | undefined,
    users: User[] | undefined,
    key: number,
  ) => ReactNode
}) {
  const key = usePageKey(account?.name)
  if (users === undefined) return children(undefined, undefined, key)
  if (account === undefined) {
    return (
      <div className="settings-page">
        <PageHeader
          back={{ to: USERS, label: 'Users' }}
          title="No such account"
        />
        {notices}
        <Missing key={name}>
          There is no account at this address: it may have been removed.
        </Missing>
      </div>
    )
  }
  return children(account, users, key)
}

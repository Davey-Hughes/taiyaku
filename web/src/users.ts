/**
 * The `/api/library/users*` client — spec C4 §4 and the build design's §6.
 *
 * **Separate from `config.ts`** for the reason that file gives for being
 * separate from `library.ts`: the two share a session and nothing else. What
 * they do share is the libraries list, which is `libraries.ts`'s and which
 * `Users.tsx` fetches once rather than declaring a second one — one client,
 * two screens, rather than a second fetch that could disagree.
 *
 * **`SessionRow` and `ApiKey` are imported, not redeclared.** `user_json`'s
 * `sessions[]` and `keys[]` are field-for-field the shapes `/api/sessions` and
 * `/api/keys` already answer for the account panel, and two declarations of
 * one wire shape is one that drifts.
 *
 * **`onlyAdminWhoCanLogIn` is the only opinion this module holds**, and it is
 * a copy of a store invariant on purpose: §6.1 makes the duplication
 * one-directional, so the client may disable what the server would refuse and
 * may never enable what it would not. It mirrors
 * `account::another_admin_can_log_in` exactly — *another* account,
 * `role = 'admin'`, not locked — because the near miss (count the
 * administrators) is wrong precisely when the second one is locked.
 */
import { type ApiKey, getJson, sendJson, type SessionRow } from './api'

/** The two values `004_auth.sql`'s `CHECK (role IN (…))` accepts. */
export const ROLES = ['user', 'admin'] as const

/** One library this account is granted, as `user_json`'s `libraries` sends it. */
export interface Grant {
  id: number
  name: string
}

/** One account, whole, as `GET /api/library/users` sends it. */
export interface User {
  id: number
  name: string
  /** `user` or `admin`. The schema's `CHECK` owns the set, not this type. */
  role: string
  can_download: boolean
  can_stream: boolean
  /**
   * False for an account still holding `credential::LOCKED` — a row with no
   * way in. `account::list` calls this half "the one worth printing", and
   * nothing else about such an account says so.
   */
  can_log_in: boolean
  /**
   * This account's wrong passwords in the current window, from every address
   * (O279). It refuses nothing: the lockouts are an address-and-name pair's
   * and an address's, never the account's.
   */
  wrong_passwords: number
  /**
   * The server's verdict that `wrong_passwords` reached its ceiling; the web
   * holds no ceiling of its own. The mark it draws says "in 15 minutes",
   * repeating `LOCKOUT_WINDOW_MINUTES` in
   * `crates/taiyaku-library/src/account.rs`, the window's length.
   */
  wrong_password_alarm: boolean
  sessions: SessionRow[]
  keys: ApiKey[]
  /**
   * Whether this account sees every library, including any added later.
   * **The only field that says so**: `libraries` is never read for it, so
   * an account limited to no library is not mistaken for an unlimited one.
   */
  every_library: boolean
  /**
   * The libraries this account is limited to when `every_library` is false.
   * Empty then means it sees no library.
   */
  libraries: Grant[]
  /**
   * **The one capability `role` does not imply.** Every other capability on
   * this row is read as `admin || column` by `Principal::from_account`; this
   * one is the column alone — the documented exception to O86, because it
   * decides whether an account may rewrite archives taiyaku does not own
   * rather than widening what it may read. Default 0, so the upgrade that
   * adds the column grants it to nobody.
   */
  can_stamp: boolean
}

/**
 * What `PATCH /api/library/users/{id}` will change.
 *
 * **Absent and present mean different things**, which is why every field is
 * optional: the server reads an absent key as "leave this column alone".
 * `every_library` and `libraries` are never sent together; the server refuses
 * a body holding both.
 */
export interface UserPatch {
  name?: string
  role?: string
  can_download?: boolean
  can_stream?: boolean
  can_stamp?: boolean
  /**
   * Lift the limit: the account sees every library, including any added
   * later, and its grants are deleted. Only `true` is sent — `false` has no
   * meaning `libraries` does not already carry, and the server refuses it.
   */
  every_library?: true
  /**
   * Limit the account to these libraries, by id, replacing what is there.
   * `[]` limits it to no library: it sees nothing until one is granted.
   */
  libraries?: number[]
  /**
   * An administrator resetting **somebody else's** password. The store ends
   * every session the target holds and leaves the caller's alone; the route
   * refuses the case where those are the same account.
   */
  password?: string
}

/** What removing an account costs. No work, edition or member is among them. */
export interface Removal {
  name: string
  positions: number
  notes: number
  sessions: number
  keys: number
  grants: number
}

/**
 * `DELETE /api/library/users/{id}`'s answer, for a dry run and a real removal
 * alike — the same body either way, with `dry_run` saying which it was.
 */
export interface RemovalAnswer {
  removed: Removal
  dry_run: boolean
}

/**
 * `sendJson` resolves `T | undefined` because a 204 has nothing to parse, and
 * the two revocations below are this module's routes that answer one.
 * Narrowed here rather than by overloading `sendJson`, for the reason
 * `config.ts` and `curation.ts` each give for their own copy: teaching the
 * shared plumbing that a body argument implies a non-empty reply would be a
 * lie told to every caller.
 */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

export const listUsers = () => getJson<{ users: User[] }>('/api/library/users')

/**
 * Create an account. C4 §4: no self-service registration — an administrator
 * makes accounts, per A2 §11.
 *
 * A refusal throws with the server's own sentence, which names whether the
 * name is taken or the role is one the schema refuses.
 */
export async function addUser(
  name: string,
  password: string,
  role: string,
): Promise<User> {
  const body = await expectBody(
    sendJson<{ user: User }>('/api/library/users', 'POST', {
      name,
      password,
      role,
    }),
  )
  return body.user
}

/** Change only what `patch` names. Answers the account whole. */
export async function patchUser(id: number, patch: UserPatch): Promise<User> {
  const body = await expectBody(
    sendJson<{ user: User }>(`/api/library/users/${id}`, 'PATCH', patch),
  )
  return body.user
}

/**
 * What removing account `id` would cost, without removing anything.
 *
 * **`?dry_run=true` spelled exactly.** `DeleteUserQuery` carries
 * `deny_unknown_fields`, so a misspelled key is refused rather than dropped:
 * a dropped `dry_run` would run the real removal.
 *
 * **The rehearsal runs the refusal too**: removing the last administrator is
 * refused here as well as at the commit, so a person is never shown five
 * counts for a deletion that will then be refused.
 */
export const removalCost = (id: number): Promise<RemovalAnswer> =>
  expectBody(
    sendJson<RemovalAnswer>(`/api/library/users/${id}?dry_run=true`, 'DELETE'),
  )

/** The real thing, with the counts taken inside the transaction that deletes. */
export const removeUser = (id: number): Promise<RemovalAnswer> =>
  expectBody(sendJson<RemovalAnswer>(`/api/library/users/${id}`, 'DELETE'))

/**
 * End one session of one account — C4 §4's "a device was lost", for a browser.
 *
 * **The account is in the path, not just the session id.**
 * `account::revoke_session` scopes the delete in SQL, and a session that is
 * not this account's answers 404 — the same answer as one that never existed.
 */
export const revokeUserSession = (id: number, sid: string) =>
  sendJson<void>(
    `/api/library/users/${id}/sessions/${encodeURIComponent(sid)}`,
    'DELETE',
  )

/** The same, for the credential an OPDS reader holds. */
export const revokeUserKey = (id: number, kid: number) =>
  sendJson<void>(`/api/library/users/${id}/keys/${kid}`, 'DELETE')

/**
 * Whether removing **or demoting** `user` would leave this library with no
 * administrator who can log in.
 *
 * One function for both, because it is one invariant reached through two
 * routes: a screen that disabled only Remove would leave the role control open
 * on the identical accident (§6.1).
 */
export function onlyAdminWhoCanLogIn(users: User[], user: User): boolean {
  return (
    user.role === 'admin' &&
    !users.some((u) => u.id !== user.id && u.role === 'admin' && u.can_log_in)
  )
}

/** What `user` sees, in a word or the names it is granted. */
export function seesLine(user: User): string {
  if (user.every_library) return 'Every library'
  if (user.role === 'admin') return 'Every library, as an administrator'
  if (user.libraries.length === 0) return 'None'
  return user.libraries.map((g) => g.name).join(', ')
}

/** The latest of `user`'s sessions' last-seen times, or `undefined`. */
export function lastSeen(user: User): string | undefined {
  let latest: string | undefined
  // The server writes these as `YYYY-MM-DD HH:MM:SS`, which sort as text.
  for (const session of user.sessions) {
    if (latest === undefined || session.last_seen > latest)
      latest = session.last_seen
  }
  return latest
}

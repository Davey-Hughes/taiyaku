# C4 PR C — users and backup, and the one flag the wire never set

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the last two settings screens — user administration and backup
— over the routes that shipped in #106, and fix the one thing the users screen
needs that the wire does not carry.

**Architecture:** Two pure client modules (`users.ts`, `backup.ts`) and three
components (`Users`, `UserRow`, `Backup`), on the `Roots`/`RootRow` split that
PR A proved: the list owns which accounts there are, each card owns what it is
doing. Every refusal is enforced in `taiyaku-library` and answered as a `409`
with a sentence; the client renders that sentence and *additionally* disables
a control where it can tell in advance. The one server change is one flag:
`GET /api/library/users` passes `None` for the caller's session token, so
`session.current` is `false` for every session it lists — including the
browser doing the asking.

**Tech Stack:** Rust (rusqlite, axum, tokio), TypeScript/React 19
(react-router, vitest with `node` and `jsdom` projects), biome.

## Global Constraints

Copied verbatim from `docs/superpowers/specs/2026-09-04-c4-screens-design.md`
§6 and the repository's standing rules. Every task's requirements implicitly
include this section.

- **The server is the authority; the client disables and explains** (§6.1).
  The duplication is one-directional: the client may refuse something the
  server would also refuse, and may **never permit** something the server
  would not.
- **Three refusals plus a fourth, and two of them are one invariant.** (1) the
  last administrator who can log in cannot be deleted **or demoted** — same
  invariant, two routes; (2) an administrator cannot demote *themselves*, which
  fires even beside a second administrator; (3) a password reset here ends the
  *target's* sessions and keeps the caller's; (4) an administrator cannot reset
  their **own** password here at all — the route refuses it and names
  `/api/password`.
- **An empty root grant means unrestricted, not blind** (spec A2, O54). The
  screen says so where it draws the grant editor.
- **No client-side admin guard** (§2). `/config/*` is reachable by URL and
  every screen renders the server's own 403 sentence. A client that *decides*
  who may administer is the shape O93 closed.
- **No self-service anything** (§8): this is the administrative view of other
  people. No key *creation* for another account — the routes revoke only.
- **Restore is out of band, a backup is the whole database including password
  hashes, a sidecar export is not a backup, and there is no schedule here**
  (§6.3). All four are load-bearing copy, not decoration.
- **The backup download is a plain `<a href download>`** (§6.3): the route
  sends `Content-Disposition: attachment` and the session cookie rides on the
  request, so fetching it into memory first would buy nothing and cost the
  file's size in JS heap.
- **Never add AI/Claude attribution** to any file, comment, doc, commit or PR.
- **Shell:** the Bash tool runs zsh with `noclobber`; fish syntax is a parse
  error. `cd` is zoxide — avoid `cd subdir && …` chains.
- **The gate:** `env -u TAIYAKU_CORPUS scripts/check-all.sh`, and quote its
  final `all green` line rather than piping through `tail`.

---

## Four findings that correct the spec

Established by reading the code on 2026-09-06, before this plan was written.
Each changes a task below.

1. **`session.current` is always `false` on this wire.**
   `users_api::assembled` calls `account::sessions_of(conn, account.id, None)`
   — the third argument is the caller's session token and it is hard-coded
   `None`. §6.2 says the expanded card shows "whether it is the browser doing
   the asking", and today it cannot: the field ships and is a constant. The
   self-service route gets this right (`session::list_sessions` passes
   `token_of(&headers)`), so this is one route out of step with its sibling
   rather than a missing feature. **Task 1 fixes it in the server**, because no
   client can compute it: the raw token is in an `HttpOnly` cookie that no
   script can read, and the wire carries only its sha256.
2. **`api.ts` already declares both of this screen's sub-shapes.**
   `SessionRow` and `ApiKey` are exactly `user_json`'s `sessions[]` and
   `keys[]` entries, field for field. `users.ts` imports them rather than
   declaring a second copy of one wire shape.
3. **The last-administrator rule is client-computable, exactly.** The store
   asks `another_admin_can_log_in(conn, id)` — is there *another* account with
   `role = 'admin'` whose `password_hash` is not `LOCKED` — and
   `GET /api/library/users` sends `role` and `can_log_in` for every account.
   So the disable is a pure function over the list the screen already has, not
   a guess: Task 2 writes it as one, and the naive "count the admins" version
   is wrong precisely when the second admin is locked.
4. **There is no route that grants or revokes one root.** `PATCH`'s `roots`
   key replaces the whole grant list (`account::Patch::roots` is
   `Option<Vec<i64>>`, absent means "leave alone", `[]` means unrestricted),
   so the grant editor sends the whole array on every change. The roots
   themselves come from `config.ts`'s `listRoots` — one client, two screens,
   per §6.2.

---

## File structure

**Rust — the one flag.**

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-serve/src/users_api.rs` | `assembled` takes the caller's token; all three handlers pass it |
| `crates/taiyaku-serve/src/users_api/tests.rs` | the session doing the asking is marked, and only it |

**Web — two pure modules and three components.** The split is
`Roots`/`RootRow`'s, and `UserRow` carries the removal itself rather than a
`RemoveUser.tsx` — §6.4's file list, and a user's removal is a dry run plus a
confirm rather than `RemoveRoot`'s dry run plus a whole `DeriveReport`.

| File | Responsibility |
| --- | --- |
| `web/src/users.ts` | wire types, the client, `onlyAdminWhoCanLogIn` |
| `web/src/Users.tsx` | the list, the add form, the roots fetch |
| `web/src/UserRow.tsx` | one account: fields, grants, sessions, keys, removal |
| `web/src/backup.ts` | `Backup`, list/take/download URL, `humanBytes`, `readableTaken` |
| `web/src/Backup.tsx` | the screen and C4 §5's four sentences |
| `web/src/config.ts` | `TABS` appends Users and Backup |
| `web/src/Config.tsx` | `SCREENS.users`, `SCREENS.backup` |
| `web/src/index.css` | `.users`, `.user-row`, `.backup` |

---

## Task 1: the session doing the asking

**Files:**
- Modify: `crates/taiyaku-serve/src/users_api.rs` (`assembled`, `list_users`, `create_user`, `patch_user`)
- Test: `crates/taiyaku-serve/src/users_api/tests.rs`

**Interfaces:**
- Consumes: `crate::session::token_of(&HeaderMap) -> Option<String>` (already `pub(crate)`), `taiyaku_library::account::sessions_of(conn, user_id, current_token: Option<&str>)`.
- Produces: `sessions[].current` is `true` for the one session whose token is the caller's cookie, and `false` for every other session of every account.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-serve/src/users_api/tests.rs`:

```rust
/// C4 §6.2: the expanded card shows "whether it is the browser doing the
/// asking", so a person can tell which row is the session in front of them
/// before revoking one.
///
/// **It was `false` for every session on this route.** `assembled` passed
/// `None` where `session::list_sessions` passes `token_of(&headers)`, so the
/// field shipped as a constant — on the one screen where revoking the wrong
/// row signs the administrator out of the browser they are working in.
#[tokio::test]
async fn the_user_list_marks_the_session_doing_the_asking_and_no_other() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();

    // A second account with a session of its own, so "marked" cannot pass by
    // marking everything.
    let reader = a_reader(&router, &state).await;
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(
            &conn,
            reader,
            "a-readers-token",
            Some("another browser"),
            None,
        )
        .expect("opens the reader's session");
    }

    let json = get_json(&router, "/api/library/users").await;
    let users = json["users"].as_array().expect("a users array");

    let admin = users
        .iter()
        .find(|u| u["id"] == 1)
        .expect("the fixture administrator");
    let marked = admin["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .filter(|s| s["current"] == true)
        .count();
    assert_eq!(
        marked, 1,
        "exactly the session this request arrived on is the current one: {json}"
    );

    let other = users
        .iter()
        .find(|u| u["id"] == reader)
        .expect("the reader");
    let reader_sessions = other["sessions"].as_array().expect("sessions");
    assert_eq!(reader_sessions.len(), 1, "{json}");
    assert_eq!(
        reader_sessions[0]["current"], false,
        "another account's session is never the browser doing the asking: {json}"
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /home/davey/projects/taiyaku && cargo nextest run -p taiyaku-serve the_user_list_marks_the_session 2>&1 | tail -20
```

Expected: FAIL — `exactly the session this request arrived on is the current
one`, `left: 0, right: 1`.

- [ ] **Step 3: Thread the token through `assembled`**

In `crates/taiyaku-serve/src/users_api.rs`, change `assembled` and its three
callers. The doc comment carries why:

```rust
/// Read one account's sessions, keys and grants and render it whole.
///
/// **`current_token` is the caller's own session cookie**, and it is a
/// parameter rather than `None` because `SessionSummary::current` is otherwise
/// a constant on this route: it was, and C4 §6.2's "whether it is the browser
/// doing the asking" was a field that shipped and never said yes. Only the
/// caller's own account can hold a session whose sha256 matches, so passing it
/// for every account is correct rather than merely harmless —
/// `account::sessions_of` compares hashes, and inverting one is the sha256
/// preimage problem. `session::list_sessions` has passed it since spec A2;
/// this route is the one that did not.
fn assembled(
    conn: &rusqlite::Connection,
    account: &Account,
    can_log_in: bool,
    current_token: Option<&str>,
) -> Result<serde_json::Value, String> {
    let sessions = account::sessions_of(conn, account.id, current_token)?;
    let keys = account::keys_of(conn, account.id)?;
    let roots = account::granted_roots(conn, account.id)?;
    Ok(user_json(account, can_log_in, &sessions, &keys, &roots))
}
```

`list_users` gains a `HeaderMap` extractor and reads the token before the
blocking closure takes ownership of it:

```rust
pub(crate) async fn list_users(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let token = crate::session::token_of(&headers);

    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let mut rendered = Vec::new();
        for (account, can_log_in) in account::list(&conn)? {
            rendered.push(assembled(&conn, &account, can_log_in, token.as_deref())?);
        }
        Ok::<_, String>(rendered)
    })
    .await;
```

`patch_user` gains the same extractor — an administrator patching their *own*
capabilities gets their own row back, and a row that disagreed with the list
about which session is current would be a screen contradicting itself one
render later. **`HeaderMap` goes before `Json(body)`**, which must stay last:
it is the only body-consuming extractor.

```rust
pub(crate) async fn patch_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
```

and inside it, before the closure:

```rust
    let token = crate::session::token_of(&headers);
```

with the `assembled` call becoming:

```rust
        assembled(&conn, &account, can, token.as_deref()).map_err(UserAnswer::Failed)
```

`create_user` passes `None`, with the reason written down rather than left to
look like an oversight:

```rust
        // **`None`, and not the caller's token.** An account created a moment
        // ago has no sessions at all, so there is nothing here for a token to
        // match — and the caller's session belongs to a different account by
        // construction.
        assembled(&conn, &account, true, None).map_err(UserAnswer::Failed)
```

- [ ] **Step 4: Run the test, then the whole crate**

```bash
cd /home/davey/projects/taiyaku && cargo nextest run -p taiyaku-serve the_user_list_marks_the_session 2>&1 | tail -5
cd /home/davey/projects/taiyaku && cargo nextest run -p taiyaku-serve 2>&1 | tail -5
```

Expected: PASS, and the rest of `taiyaku-serve` unchanged.

- [ ] **Step 5: Commit**

```bash
cd /home/davey/projects/taiyaku && git add crates/taiyaku-serve/src/users_api.rs crates/taiyaku-serve/src/users_api/tests.rs && git commit -m "the users list marks the session doing the asking"
```

---

## Task 2: `web/src/users.ts` — types, client, and the one rule the client may hold

**Files:**
- Create: `web/src/users.ts`
- Test: `web/src/users.test.ts` (the `node` project — `.ts`, per §7)

**Interfaces:**
- Consumes: `getJson`, `sendJson`, and the types `SessionRow` and `ApiKey` from `./api`.
- Produces, and every later task uses these names unchanged:
  `Grant {id: number; path: string}`,
  `User {id, name, role, can_download, can_stream, can_log_in, sessions: SessionRow[], keys: ApiKey[], roots: Grant[]}`,
  `UserPatch {name?, role?, can_download?, can_stream?, roots?: number[], password?}`,
  `Removal {name, positions, notes, sessions, keys, grants}`,
  `RemovalAnswer {removed: Removal; dry_run: boolean}`,
  `listUsers(): Promise<{users: User[]}>`,
  `addUser(name, password, role): Promise<User>`,
  `patchUser(id, patch): Promise<User>`,
  `removalCost(id): Promise<RemovalAnswer>`,
  `removeUser(id): Promise<RemovalAnswer>`,
  `revokeUserSession(id, sid): Promise<void | undefined>`,
  `revokeUserKey(id, kid): Promise<void | undefined>`,
  `onlyAdminWhoCanLogIn(users: User[], user: User): boolean`,
  `ROLES: readonly ['user', 'admin']`.

- [ ] **Step 1: Write the failing test**

Create `web/src/users.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  addUser,
  listUsers,
  onlyAdminWhoCanLogIn,
  patchUser,
  removalCost,
  removeUser,
  revokeUserKey,
  revokeUserSession,
  ROLES,
  type User,
} from './users'

const user = (over: Partial<User> = {}): User => ({
  id: 1,
  name: 'davey',
  role: 'admin',
  can_download: true,
  can_stream: true,
  can_log_in: true,
  sessions: [],
  keys: [],
  roots: [],
  ...over,
})

/**
 * A fresh `Response` per call, not one shared instance — `Response.json()`
 * consumes its body stream, so a second read of the same instance throws
 * "Body is unusable". `config.test.ts` and `library.test.ts` carry the same
 * note for the same reason.
 */
const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ROLES', () => {
  it('spells the two values the schema CHECK accepts', () => {
    // `004_auth.sql`'s `CHECK (role IN ('admin', 'user'))`. A third value here
    // is a 400 the add form would offer in a dropdown.
    expect([...ROLES]).toEqual(['user', 'admin'])
  })
})

describe('onlyAdminWhoCanLogIn', () => {
  it('is true for the sole administrator', () => {
    const sole = user({ id: 1, role: 'admin' })
    expect(onlyAdminWhoCanLogIn([sole, user({ id: 2, role: 'user' })], sole)).toBe(
      true,
    )
  })

  it('is false beside a second administrator who can log in', () => {
    const first = user({ id: 1, role: 'admin' })
    const second = user({ id: 2, name: 'sam', role: 'admin' })
    expect(onlyAdminWhoCanLogIn([first, second], first)).toBe(false)
  })

  it('is true beside a second administrator who cannot log in', () => {
    // The store asks `another_admin_can_log_in` — an account still holding
    // `credential::LOCKED` has the role and no way in, so it cannot be the
    // administrator this library is left with. A client that counted admins
    // would enable Remove on the one account that must not be removed, which
    // is the one direction §6.1 forbids: the client may never permit what the
    // server would refuse.
    const live = user({ id: 1, role: 'admin' })
    const locked = user({ id: 2, name: 'sam', role: 'admin', can_log_in: false })
    expect(onlyAdminWhoCanLogIn([live, locked], live)).toBe(true)
  })

  it('is false for a reader, even the only account in the library', () => {
    const reader = user({ id: 1, role: 'user' })
    expect(onlyAdminWhoCanLogIn([reader], reader)).toBe(false)
  })
})

describe('the users client', () => {
  it('asks for the users list', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json({ users: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await listUsers()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users')
  })

  it('posts the name, password and role, and unwraps the account', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ user: user({ id: 7, name: 'sam' }) }))
    vi.stubGlobal('fetch', fetchMock)
    const made = await addUser('sam', 'hunter2', 'user')
    expect(made.name).toBe('sam')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    expect(init.body).toBe(
      JSON.stringify({ name: 'sam', password: 'hunter2', role: 'user' }),
    )
  })

  it('patches with only the keys it was handed', async () => {
    // An absent key leaves the column alone and `roots: []` grants nothing —
    // which spec A2 defines as unrestricted. Sending a whole row would rewrite
    // five columns to answer a question about one.
    const fetchMock = vi.fn().mockImplementation(() => json({ user: user() }))
    vi.stubGlobal('fetch', fetchMock)
    await patchUser(3, { can_download: false })
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users/3')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('PATCH')
    expect(init.body).toBe(JSON.stringify({ can_download: false }))
  })

  it('spells dry_run exactly, because the server refuses any other key', async () => {
    // `deny_unknown_fields` on `DeleteUserQuery`: `?dryrun=true` — one key off
    // — answered 200 and ran the real, committed removal on the roots route
    // before that attribute existed. This route is the second-most destructive
    // in the product, so the exact string is pinned rather than assumed.
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ removed: {}, dry_run: true }))
    vi.stubGlobal('fetch', fetchMock)
    await removalCost(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/users/3?dry_run=true',
    )
  })

  it('removes without the dry-run key at all', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ removed: {}, dry_run: false }))
    vi.stubGlobal('fetch', fetchMock)
    await removeUser(3)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users/3')
    expect(fetchMock.mock.calls[0]?.[0]).not.toContain('dry_run')
  })

  it('revokes a session under the account that holds it', async () => {
    // `account::revoke_session` is scoped to the account in SQL, and the path
    // is what carries the scope: an id from one account's listing must not end
    // another account's session.
    const fetchMock = vi
      .fn()
      .mockImplementation(() => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await revokeUserSession(4, 'a/b')
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/users/4/sessions/a%2Fb',
    )
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('DELETE')
  })

  it('revokes a key under the account that holds it', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await revokeUserKey(4, 9)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/users/4/keys/9')
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node users.test.ts 2>&1 | tail -20
```

Expected: FAIL — `Failed to resolve import "./users"`.

- [ ] **Step 3: Write `web/src/users.ts`**

```ts
/**
 * The `/api/library/users*` client — spec C4 §4 and the build design's §6,
 * over the routes that shipped in #106.
 *
 * **Separate from `config.ts`** for the reason that file gives for being
 * separate from `library.ts`: the two share a session and nothing else. What
 * they do share is the roots list, and `Users.tsx` fetches it from `config.ts`
 * rather than declaring a second one — one client, two screens, rather than a
 * second fetch that could disagree.
 *
 * **`SessionRow` and `ApiKey` are imported, not redeclared.** `user_json`'s
 * `sessions[]` and `keys[]` are field-for-field the shapes `/api/sessions` and
 * `/api/keys` already answer for the account panel, and two declarations of
 * one wire shape is one that drifts.
 *
 * **`onlyAdminWhoCanLogIn` is the only opinion this module holds**, and it is
 * a copy of a store invariant on purpose: §6.1 makes the duplication
 * one-directional, so the client may disable what the server would refuse and
 * may never enable what it would not. It mirrors `account::
 * another_admin_can_log_in` exactly — *another* account, `role = 'admin'`,
 * not locked — because the near miss (count the admins) is wrong precisely
 * when the second administrator is the locked one.
 */
import { type ApiKey, getJson, sendJson, type SessionRow } from './api'

/** The two values `004_auth.sql`'s `CHECK (role IN (…))` accepts. */
export const ROLES = ['user', 'admin'] as const

/** One root this account may see, as `user_json`'s `roots` sends it. */
export interface Grant {
  id: number
  path: string
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
   * way in. `account::list` calls it "the second half worth printing", and
   * nothing else about such an account says so.
   */
  can_log_in: boolean
  sessions: SessionRow[]
  keys: ApiKey[]
  /** **Empty means unrestricted, not blind** — spec A2, O54. */
  roots: Grant[]
}

/**
 * What `PATCH /api/library/users/{id}` will change.
 *
 * **Absent and present mean different things**, which is why every field is
 * optional: the server reads an absent key as "leave this column alone".
 * `roots: []` is the one that reads oddly and is deliberate — it grants
 * nothing, which spec A2 defines as *unrestricted*.
 */
export interface UserPatch {
  name?: string
  role?: string
  can_download?: boolean
  can_stream?: boolean
  /** The whole grant list, replacing what is there. There is no per-root route. */
  roots?: number[]
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
 * `deny_unknown_fields`, so a misspelled key is refused rather than dropped —
 * which on the roots route, before that attribute, meant `?dryrun=true`
 * answered 200 and ran the real removal.
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
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node users.test.ts 2>&1 | tail -10
```

Expected: PASS, 11 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/users.ts web/src/users.test.ts && git commit -m "web: the users client, and the last-administrator rule as a pure function"
```

---

## Task 3: `web/src/UserRow.tsx` — one account

**Files:**
- Create: `web/src/UserRow.tsx`
- Test: `web/src/UserRow.test.tsx` (the `jsdom` project — `.tsx`, per §7)

**Interfaces:**
- Consumes: everything Task 2 produced, plus `type Root` from `./config`.
- Produces: `UserRow`, `UserRowProps {user, users, roots, actingAs?, onChanged}` — Task 4 mounts one per account and supplies all five.

- [ ] **Step 1: Write the failing test**

Create `web/src/UserRow.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Root } from './config'
import { UserRow } from './UserRow'
import { mount } from './testing/mount'
import * as usersApi from './users'

vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

const user = (over: Partial<usersApi.User> = {}): usersApi.User => ({
  id: 1,
  name: 'davey',
  role: 'admin',
  can_download: true,
  can_stream: true,
  can_log_in: true,
  sessions: [],
  keys: [],
  roots: [],
  ...over,
})

const root = (over: Partial<Root> = {}): Root => ({
  id: 3,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  ...over,
})

const sam = () => user({ id: 2, name: 'sam', role: 'admin' })

interface Mounted {
  host: HTMLElement
  changed: () => void
}

async function show(
  who: usersApi.User,
  everyone: usersApi.User[],
  over: { roots?: Root[]; actingAs?: number } = {},
): Promise<Mounted> {
  const changed = vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <UserRow
        user={who}
        users={everyone}
        roots={over.roots ?? [root()]}
        actingAs={over.actingAs}
        onChanged={changed}
      />,
    )
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, changed }
}

const control = <T extends Element>(host: HTMLElement, selector: string): T => {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

async function click(el: Element) {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(usersApi.patchUser).mockResolvedValue(user())
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
  vi.mocked(usersApi.revokeUserKey).mockResolvedValue(undefined)
})

describe('the two invariants on the last administrator', () => {
  it('disables both the role control and Remove, and says why', async () => {
    // §6.1: one invariant reached through two routes. A screen that disabled
    // only Remove leaves the role dropdown open on the identical accident.
    const sole = user()
    const { host } = await show(sole, [sole, user({ id: 2, name: 'reader', role: 'user' })])
    expect(control<HTMLSelectElement>(host, 'select[name="role"]').disabled).toBe(true)
    expect(control<HTMLButtonElement>(host, 'button[name="remove"]').disabled).toBe(true)
    expect(host.textContent).toContain('only administrator who can log in')
  })

  it('leaves both live beside a second administrator who can log in', async () => {
    // The negative case, so the assertions above cannot pass against a card
    // that disables everything.
    const first = user()
    const { host } = await show(first, [first, sam()], { actingAs: 99 })
    expect(control<HTMLSelectElement>(host, 'select[name="role"]').disabled).toBe(false)
    expect(control<HTMLButtonElement>(host, 'button[name="remove"]').disabled).toBe(false)
  })

  it('still disables the role control beside an administrator who cannot log in', async () => {
    const live = user()
    const locked = user({ id: 2, name: 'sam', role: 'admin', can_log_in: false })
    const { host } = await show(live, [live, locked], { actingAs: 99 })
    expect(control<HTMLSelectElement>(host, 'select[name="role"]').disabled).toBe(true)
  })
})

describe('the actor rule', () => {
  it('will not let an administrator demote themselves, even beside another', async () => {
    // Fires about the actor rather than about the population — `acting_as` is
    // what carries it in the store, and this card is where a person meets it.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 1 })
    expect(control<HTMLSelectElement>(host, 'select[name="role"]').disabled).toBe(true)
    expect(host.textContent).toContain('another administrator has to do it')
  })

  it('will not let an administrator reset their own password, and says where to', async () => {
    // The fourth refusal, and the one whose advice names a screen with no URL:
    // `Account.tsx` mounts from inside the reader. O94.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 1 })
    expect(control<HTMLInputElement>(host, 'input[name="password"]').disabled).toBe(true)
    expect(host.textContent).toContain('/api/password')
    expect(host.textContent).toContain('press A')
  })

  it("resets somebody else's password and says what that ends", async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host, changed } = await show(other, [me, other], { actingAs: 1 })
    const box = control<HTMLInputElement>(host, 'input[name="password"]')
    expect(box.disabled).toBe(false)
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'hunter2')
      box.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await click(control(host, 'button[name="reset"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { password: 'hunter2' })
    expect(changed).toHaveBeenCalled()
    expect(host.textContent).toContain('API keys are untouched')
  })
})

describe('the fields', () => {
  it('patches only the capability column that changed', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'input[name="can_download"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { can_download: false })
  })

  it('sends the whole grant list when a root is ticked, because there is no per-root route', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], {
      actingAs: 1,
      roots: [root({ id: 3 }), root({ id: 4, path: '/srv/raw' })],
    })
    await click(control(host, 'input[name="root-4"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { roots: [4] })
  })

  it('says that nothing ticked means unrestricted', async () => {
    // Counter-intuitive and load-bearing: an empty grant is every root, not
    // none. Spec A2, O54 — every account in every existing library holds one.
    const me = user()
    const { host } = await show(me, [me, sam()], { actingAs: 99 })
    expect(host.textContent).toContain('unrestricted')
  })

  it('names an account nobody can sign in as', async () => {
    const locked = user({ id: 2, name: 'sam', role: 'user', can_log_in: false })
    const { host } = await show(locked, [user(), locked], { actingAs: 1 })
    expect(host.textContent).toContain('Nobody can sign in as this account')
  })
})

describe('sessions and keys', () => {
  it('marks the browser doing the asking and revokes under this account', async () => {
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01T10:00:00Z',
          last_seen: '2026-09-06T09:00:00Z',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: true,
        },
        {
          id: 'bbb',
          created: '2026-08-01T10:00:00Z',
          last_seen: '2026-08-02T10:00:00Z',
          user_agent: null,
          ip: null,
          current: false,
        },
      ],
      keys: [
        { id: 9, name: 'the tablet', created: '2026-08-01T10:00:00Z', last_used: null },
      ],
    })
    const { host, changed } = await show(other, [user(), other], { actingAs: 1 })

    // Asserted per element, not by `toContain` over the whole card: a
    // whole-page match cannot see which session carries the marker.
    const rows = [...host.querySelectorAll('.session-list > li')]
    expect(rows).toHaveLength(2)
    expect(rows[0]?.textContent).toContain('this browser')
    expect(rows[1]?.textContent).not.toContain('this browser')
    expect(rows[1]?.textContent).toContain('no user-agent recorded')

    await click(control(host, 'button[name="revoke-session-bbb"]'))
    expect(usersApi.revokeUserSession).toHaveBeenCalledWith(2, 'bbb')
    expect(changed).toHaveBeenCalled()

    await click(control(host, 'button[name="revoke-key-9"]'))
    expect(usersApi.revokeUserKey).toHaveBeenCalledWith(2, 9)
    expect(host.textContent).toContain('never used')
  })
})

describe('removal', () => {
  it('states all five counts and the name, and sends no DELETE until confirmed', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 4,
        notes: 1,
        sessions: 2,
        keys: 0,
        grants: 3,
      },
      dry_run: true,
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))

    const counts = control(host, '.counts').textContent ?? ''
    expect(counts).toContain('sam')
    expect(counts).toContain('4 in-progress positions')
    expect(counts).toContain('1 note')
    expect(counts).toContain('2 open sessions')
    expect(counts).toContain('0 API keys')
    expect(counts).toContain('3 root grants')
    expect(usersApi.removeUser).not.toHaveBeenCalled()

    await click(control(host, 'button[name="confirm-remove"]'))
    expect(usersApi.removeUser).toHaveBeenCalledWith(2)
  })

  it('offers no confirm button while the counts are unknown', async () => {
    // The one failure this dialog exists to prevent: a button that removes an
    // account while the sentence above it says nothing. "The dry run failed"
    // is exactly when a person presses on regardless.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.removalCost).mockRejectedValue(
      new Error('/api/library/users/2?dry_run=true: 500 counting failed'),
    )
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(control(host, 'button[name="remove"]'))
    expect(host.querySelector('button[name="confirm-remove"]')).toBeNull()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'counting failed',
    )
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom UserRow.test.tsx 2>&1 | tail -20
```

Expected: FAIL — `Failed to resolve import "./UserRow"`.

- [ ] **Step 3: Write `web/src/UserRow.tsx`**

```tsx
/**
 * One account: what it is, what it may do, what is signed in as it, and what
 * removing it would cost — spec C4 §4 and the build design's §6.2.
 *
 * **The card owns its own writes, the list owns who exists.** `RootRow`'s
 * split and its stated reason: a patch, a revocation and a dry run all belong
 * to the account being administered, and the list reloads on `onChanged`.
 *
 * **The server is the authority; this disables and explains.** Every refusal
 * here is enforced in `taiyaku-library` and answered as a 409 with a sentence,
 * which this card renders when it arrives. Where it can tell in advance it
 * *also* disables the control and says why — and the duplication is
 * one-directional: this may refuse what the server would refuse, and may never
 * permit what the server would not. That is why `onlyAdminWhoCanLogIn` mirrors
 * `another_admin_can_log_in` rather than counting administrators.
 *
 * **One reason covers two controls.** The last-administrator rule reaches the
 * role dropdown and Remove through different routes and is one invariant; a
 * card that disabled only Remove leaves the identical accident one dropdown
 * away.
 *
 * **The self-password refusal names a screen with no URL of its own.**
 * `/api/password` is the route that keeps the session making the change, and
 * the screen that calls it is `Account.tsx` — mounted from `App.tsx` on
 * `/read/:work` and reachable only by opening a work and pressing `A`. That is
 * O94, and it is stated rather than worked around: the fix is a route, and it
 * belongs to the reader's chrome rather than to this branch.
 *
 * **Grants are sent whole.** There is no route that grants or revokes one
 * root; `account::Patch::roots` replaces the list, an absent key leaves it
 * alone, and `[]` means unrestricted rather than blind — spec A2 and O54,
 * which is why the empty case is a sentence on screen rather than a silence.
 *
 * **The confirm does not exist until the counts do**, and the real `DELETE` is
 * sent from here and nowhere else. `RemoveRoot`'s reasoning, on the route
 * `users_api` calls the second-most destructive in the product.
 */
import { useRef, useState } from 'react'
import type { Root } from './config'
import {
  onlyAdminWhoCanLogIn,
  patchUser,
  type RemovalAnswer,
  removalCost,
  removeUser,
  revokeUserKey,
  revokeUserSession,
  ROLES,
  type User,
  type UserPatch,
} from './users'

export interface UserRowProps {
  user: User
  /**
   * Every account, so the last-administrator rule is answered from the list
   * the screen already holds rather than by a second request per card.
   */
  users: User[]
  /** This library's roots, for the grant editor. `Users` fetches them once. */
  roots: Root[]
  /**
   * The account this browser is signed in as, or `undefined` where no gate
   * sits above — which `gate.ts` defines as "no account known", never "an
   * admin". Both readings are safe here: the server refuses either way.
   */
  actingAs?: number
  onChanged: () => void
}

/**
 * `n` of a thing, in the number the count actually is.
 *
 * Zero takes the plural — "0 API keys" — which is English rather than an
 * oversight. `RemoveRoot` carries the same helper and the same note; it was
 * found by rendering that dialog in a real browser, where it read "1 works".
 */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export function UserRow({
  user,
  users,
  roots,
  actingAs,
  onChanged,
}: UserRowProps) {
  const [failure, setFailure] = useState<string | undefined>()
  const [saving, setSaving] = useState(false)
  const [password, setPassword] = useState('')
  const [wasReset, setWasReset] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  const [removing, setRemoving] = useState(false)
  /**
   * Whether the real `DELETE` is already on its way.
   *
   * **A ref, for the reason `RemoveRoot`'s and `RootRow`'s are**: React
   * batches state, so `disabled={removing}` alone lets two presses dispatched
   * before the next render both send.
   */
  const inFlight = useRef(false)

  const isSelf = actingAs === user.id
  const lastAdmin = onlyAdminWhoCanLogIn(users, user)

  const save = (patch: UserPatch, after?: () => void) => {
    setSaving(true)
    setFailure(undefined)
    patchUser(user.id, patch)
      .then(() => {
        setSaving(false)
        after?.()
        onChanged()
      })
      .catch((err: unknown) => {
        setSaving(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  const granted = user.roots.map((g) => g.id)

  const toggleGrant = (id: number, on: boolean) =>
    save({
      roots: on ? [...granted, id] : granted.filter((held) => held !== id),
    })

  const submitPassword = (e: React.FormEvent) => {
    e.preventDefault()
    if (password === '') return
    save({ password }, () => {
      setPassword('')
      setWasReset(true)
    })
  }

  const revoke = (what: Promise<void | undefined>) => {
    setFailure(undefined)
    what
      .then(() => onChanged())
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  const askCost = () => {
    setConfirming(true)
    setCost(undefined)
    setFailure(undefined)
    removalCost(user.id)
      .then((answer) => setCost(answer))
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  const confirmRemoval = () => {
    if (inFlight.current) return
    inFlight.current = true
    setRemoving(true)
    removeUser(user.id)
      // No `inFlight.current = false` on success: the account is gone and the
      // reload takes this card with it, exactly as `RemoveRoot` leaves its
      // own flag set.
      .then(() => onChanged())
      .catch((err: unknown) => {
        inFlight.current = false
        setRemoving(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  const roleWhy = isSelf
    ? 'An administrator cannot take their own admin role away; another administrator has to do it.'
    : lastAdmin
      ? `${user.name} is the only administrator who can log in, so neither the role nor Remove can change that. Give another account the admin role first.`
      : undefined

  return (
    <li className="user-row">
      <h3>
        {user.name}
        {isSelf && <span className="you"> — this is you</span>}
      </h3>
      {failure !== undefined && <p role="alert">{failure}</p>}

      {!user.can_log_in && (
        <p className="locked">
          Nobody can sign in as this account: it holds no usable password.
          Setting one below gives it a way in.
        </p>
      )}

      <div className="fields">
        <label>
          <span>Role</span>
          <select
            name="role"
            value={user.role}
            disabled={saving || isSelf || lastAdmin}
            onChange={(e) => save({ role: e.target.value })}
          >
            {ROLES.map((role) => (
              <option key={role} value={role}>
                {role}
              </option>
            ))}
          </select>
        </label>
        {roleWhy !== undefined && <p className="why">{roleWhy}</p>}

        <label>
          <input
            type="checkbox"
            name="can_download"
            checked={user.can_download}
            disabled={saving}
            onChange={(e) => save({ can_download: e.target.checked })}
          />
          <span>May download whole editions</span>
        </label>
        <label>
          <input
            type="checkbox"
            name="can_stream"
            checked={user.can_stream}
            disabled={saving}
            onChange={(e) => save({ can_stream: e.target.checked })}
          />
          <span>May stream pages</span>
        </label>
      </div>

      <fieldset className="grants">
        <legend>Roots this account may see</legend>
        {roots.length === 0 ? (
          <p className="empty">This library has no roots yet.</p>
        ) : (
          <ul className="grant-list">
            {roots.map((root) => (
              // Keyed by path rather than id: SQLite reissues a freed
              // `root.id`, and `Roots.tsx` carries the same note for the same
              // reason. Two roots may not overlap, so a path cannot collide.
              <li key={root.path}>
                <label>
                  <input
                    type="checkbox"
                    name={`root-${root.id}`}
                    checked={granted.includes(root.id)}
                    disabled={saving}
                    onChange={(e) => toggleGrant(root.id, e.target.checked)}
                  />
                  <span>{root.path}</span>
                </label>
              </li>
            ))}
          </ul>
        )}
        <p className="unrestricted">
          Nothing ticked means unrestricted — every root, including any added
          later. It does not mean no roots at all, which is why an account with
          an empty list sees the whole library.
        </p>
      </fieldset>

      <form className="reset" onSubmit={submitPassword}>
        <label>
          <span>Set a new password</span>
          <input
            type="password"
            name="password"
            value={password}
            disabled={isSelf || saving}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <button
          type="submit"
          name="reset"
          disabled={isSelf || saving || password === ''}
        >
          Set password
        </button>
        {isSelf ? (
          <p className="why">
            You cannot reset your own password here: this ends every session the
            account has, and yours is the session making the change. The server
            refuses it and names <code>/api/password</code>, which keeps the
            session making the change. That is the account panel — open any work
            and press A. It has no URL of its own yet (O94).
          </p>
        ) : (
          <p className="why">
            This ends every session {user.name} has open. Their API keys are
            untouched — independent credentials with their own revocation.
          </p>
        )}
        {wasReset && (
          <p className="done">
            Password set. {user.name} is signed out everywhere.
          </p>
        )}
      </form>

      <details className="credentials">
        <summary>
          {count(user.sessions.length, 'session', 'sessions')} ·{' '}
          {count(user.keys.length, 'API key', 'API keys')}
        </summary>
        {user.sessions.length === 0 ? (
          <p className="empty">No sessions open.</p>
        ) : (
          <ul className="session-list">
            {user.sessions.map((session) => (
              <li key={session.id}>
                <span className="what">
                  {session.user_agent ?? 'no user-agent recorded'} ·{' '}
                  {session.ip ?? 'no address recorded'}
                </span>
                <span className="when">
                  started {session.created} · last seen {session.last_seen}
                </span>
                {session.current && (
                  <span className="current">this browser</span>
                )}
                <button
                  type="button"
                  name={`revoke-session-${session.id}`}
                  onClick={() =>
                    revoke(revokeUserSession(user.id, session.id))
                  }
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        )}
        {user.keys.length > 0 && (
          <ul className="key-list">
            {user.keys.map((key) => (
              <li key={key.id}>
                <span className="what">{key.name}</span>
                <span className="when">
                  made {key.created} ·{' '}
                  {key.last_used === null
                    ? 'never used'
                    : `last used ${key.last_used}`}
                </span>
                <button
                  type="button"
                  name={`revoke-key-${key.id}`}
                  onClick={() => revoke(revokeUserKey(user.id, key.id))}
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        )}
      </details>

      <div className="actions">
        {!confirming && (
          <button
            type="button"
            name="remove"
            disabled={lastAdmin}
            onClick={askCost}
          >
            Remove…
          </button>
        )}
        {confirming && (
          <div
            className="remove-user"
            role="group"
            aria-label={`Remove the account ${user.name}`}
          >
            {cost === undefined
              ? failure === undefined && (
                  <p>Working out what this would remove…</p>
                )
              : null}
            {cost !== undefined && (
              <>
                <p className="counts">
                  Removing {cost.removed.name} takes{' '}
                  {count(
                    cost.removed.positions,
                    'in-progress position',
                    'in-progress positions',
                  )}
                  , {count(cost.removed.notes, 'note', 'notes')},{' '}
                  {count(cost.removed.sessions, 'open session', 'open sessions')}
                  , {count(cost.removed.keys, 'API key', 'API keys')} and{' '}
                  {count(cost.removed.grants, 'root grant', 'root grants')}.
                </p>
                <p className="irreversible">
                  No work, edition or member goes with it — the library itself
                  is untouched. What goes is everything this person recorded
                  against it.
                  {isSelf &&
                    ' This is the account you are signed in as; removing it signs you out.'}
                </p>
                <button
                  type="button"
                  name="confirm-remove"
                  onClick={confirmRemoval}
                  disabled={removing}
                >
                  {removing ? 'Removing…' : `Remove ${user.name}`}
                </button>
              </>
            )}
            <button
              type="button"
              name="cancel-remove"
              onClick={() => {
                setConfirming(false)
                setCost(undefined)
              }}
              disabled={removing}
            >
              Cancel
            </button>
          </div>
        )}
      </div>
    </li>
  )
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom UserRow.test.tsx 2>&1 | tail -12
```

Expected: PASS, 13 tests. (The one test name carrying an apostrophe is
written in double quotes above — biome's formatter keeps it that way rather
than escaping inside single quotes.)

- [ ] **Step 5: Commit**

```bash
git add web/src/UserRow.tsx web/src/UserRow.test.tsx && git commit -m "web: one account's card, with both faces of the last-administrator rule"
```

---

## Task 4: `web/src/Users.tsx` — the list, the add form, and the roots one fetch

**Files:**
- Create: `web/src/Users.tsx`
- Test: `web/src/Users.test.tsx` (`jsdom`)

**Interfaces:**
- Consumes: `listUsers`, `addUser`, `ROLES`, `type User` (Task 2); `UserRow` (Task 3); `listRoots`, `type Root` from `./config`; `useGate` from `./gate`.
- Produces: `Users` — Task 7 mounts it as `SCREENS.users`.

- [ ] **Step 1: Write the failing test**

Create `web/src/Users.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { GateContext } from './gate'
import { mount } from './testing/mount'
import * as usersApi from './users'
import { Users } from './Users'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
}))
vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  addUser: vi.fn(),
  listUsers: vi.fn(),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

const user = (over: Partial<usersApi.User> = {}): usersApi.User => ({
  id: 1,
  name: 'davey',
  role: 'admin',
  can_download: true,
  can_stream: true,
  can_log_in: true,
  sessions: [],
  keys: [],
  roots: [],
  ...over,
})

const who = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
}

async function settle(signedInAs = who) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <GateContext value={{ who: signedInAs, signOut: () => undefined }}>
        <Users />
      </GateContext>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

async function fill(host: HTMLElement, name: string, value: string) {
  const box = host.querySelector(`input[name="${name}"]`) as HTMLInputElement
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, value)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

async function submitAdd(host: HTMLElement) {
  const form = host.querySelector('form.add-user') as HTMLFormElement
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [user()] })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(usersApi.revokeUserSession).mockResolvedValue(undefined)
})

describe('Users', () => {
  it('lists the accounts the server sent', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await settle()
    expect(host.querySelectorAll('li.user-row')).toHaveLength(2)
  })

  it('marks the card of the account this browser is signed in as', async () => {
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [user(), user({ id: 2, name: 'sam', role: 'user' })],
    })
    const host = await settle()
    const cards = [...host.querySelectorAll('li.user-row')]
    expect(cards[0]?.textContent).toContain('this is you')
    expect(cards[1]?.textContent).not.toContain('this is you')
  })

  it('posts the name, password and role, then reloads', async () => {
    vi.mocked(usersApi.addUser).mockResolvedValue(user({ id: 2, name: 'sam' }))
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledWith('sam', 'hunter2', 'user')
    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
  })

  it('draws the server sentence when a name is already taken', async () => {
    // C4 §4: no client-side uniqueness check. `user.name` is `UNIQUE` and the
    // route answers 400 with its own sentence; a second opinion here would be
    // a race with a store this browser does not own.
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error('/api/library/users: 400 an account named sam already exists'),
    )
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already exists',
    )
  })

  it('keeps that refusal when a background reload lands', async () => {
    // **PR A's review pass 2, one screen over.** One failure slot served both
    // the list and the add form, and `reload` — every row's `onChanged` —
    // cleared it, so a revocation in any card silently erased the one message
    // naming why an account could not be created.
    vi.mocked(usersApi.listUsers).mockResolvedValue({
      users: [
        user({
          sessions: [
            {
              id: 'aaa',
              created: '2026-09-01T10:00:00Z',
              last_seen: '2026-09-06T09:00:00Z',
              user_agent: 'Firefox',
              ip: '10.0.0.2',
              current: true,
            },
          ],
        }),
      ],
    })
    vi.mocked(usersApi.addUser).mockRejectedValue(
      new Error('/api/library/users: 400 an account named sam already exists'),
    )
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)

    const revoke = host.querySelector(
      'button[name="revoke-session-aaa"]',
    ) as HTMLButtonElement
    await act(async () => {
      revoke.click()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(usersApi.listUsers).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already exists',
    )
  })

  it('sends one POST for two submits dispatched before the first answers', async () => {
    // A ref rather than `disabled={adding}`: React batches state, so both
    // handlers read `adding === false`. Reachable by holding Enter in the name
    // box, and the second request trips `user.name UNIQUE` — a 400 rendered
    // over the account that was just created.
    vi.mocked(usersApi.addUser).mockImplementation(
      () => new Promise(() => undefined),
    )
    const host = await settle()
    await fill(host, 'new-name', 'sam')
    await fill(host, 'new-password', 'hunter2')
    await submitAdd(host)
    await submitAdd(host)
    expect(usersApi.addUser).toHaveBeenCalledTimes(1)
  })

  it('says so when the roots list could not be read, rather than drawing an empty one', async () => {
    // The grant editor draws "This library has no roots yet" for an empty
    // array, and a failed read would make that sentence a lie about the one
    // control whose empty state means *unrestricted*.
    vi.mocked(config.listRoots).mockRejectedValue(
      new Error('/api/library/roots: 500 the store is unavailable'),
    )
    const host = await settle()
    expect(host.textContent).toContain('roots list could not be read')
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom Users.test.tsx 2>&1 | tail -20
```

Expected: FAIL — `Failed to resolve import "./Users"`.

- [ ] **Step 3: Write `web/src/Users.tsx`**

```tsx
/**
 * The users screen — spec C4 §4 and the build design's §6.2.
 *
 * **The list owns who exists; each card owns what it is doing.**
 * `Roots`/`RootRow`'s split, and the same reason: a patch, a revocation and a
 * dry run belong to the account being administered.
 *
 * **Two failure slots, not one.** `reload` is handed to every card as
 * `onChanged`, so a revocation, a capability toggle or a removal fires a list
 * read — and a single slot meant any of them silently erased the add form's
 * refusal, which is the one message naming why an account could not be
 * created. Found on the roots screen by PR A's second review pass; written
 * this way here rather than found again.
 *
 * **The roots list is fetched here, once, and handed down.** `config.ts`
 * already has the client — one client and two screens rather than a second
 * fetch that could disagree (§6.2) — and a card fetching its own would ask N
 * times for one answer. **A failed read is drawn**, because the grant editor's
 * empty state and its failure look identical on screen and mean opposite
 * things: an empty grant is *unrestricted*.
 *
 * **The caller's own account comes from the gate, not from a second request.**
 * `useGate()` already holds `me()`'s answer for the page load. `undefined`
 * there means "no account known" and is safe: the server refuses a
 * self-demotion whether or not this screen drew the control.
 *
 * **Keyed by name rather than id.** `user.id` is `INTEGER PRIMARY KEY` with no
 * `AUTOINCREMENT`, so SQLite reissues a freed id to the next account created —
 * and React matching on `key` would hand a recycled id the removed account's
 * card instance, with its open removal dialog and typed password. `user.name`
 * is `UNIQUE`; a rename remounts the card, which is the right thing to lose.
 * `Roots.tsx` carries the same note about `root.id`.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { listRoots, type Root } from './config'
import { useGate } from './gate'
import { addUser, listUsers, ROLES, type User } from './users'
import { UserRow } from './UserRow'

export function Users() {
  const [users, setUsers] = useState<User[] | undefined>()
  const [roots, setRoots] = useState<Root[] | undefined>()
  const [rootsFailure, setRootsFailure] = useState<string | undefined>()
  /** Why the list could not be read. Separate from `addFailure`. */
  const [failure, setFailure] = useState<string | undefined>()
  /** Why the last add was refused. Cleared only by the next add. */
  const [addFailure, setAddFailure] = useState<string | undefined>()
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<string>('user')
  const [adding, setAdding] = useState(false)
  /** Whether an add is already on its way. See this file's doc comment. */
  const inFlight = useRef(false)
  /** The newest `listUsers` call — responses need not arrive in order. */
  const latest = useRef(0)
  const gate = useGate()

  const reload = useCallback(() => {
    const seq = ++latest.current
    listUsers()
      .then((body) => {
        if (seq !== latest.current) return
        setUsers(body.users)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  useEffect(() => {
    let cancelled = false
    listRoots()
      .then((body) => {
        if (cancelled) return
        setRoots(body.roots)
        setRootsFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setRootsFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || password === '') return
    if (inFlight.current) return
    inFlight.current = true
    setAdding(true)
    setAddFailure(undefined)
    addUser(name, password, role)
      .then(() => {
        inFlight.current = false
        setAdding(false)
        setName('')
        setPassword('')
        setRole('user')
        reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setAdding(false)
        setAddFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <section className="users">
      <h2>Users</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {rootsFailure !== undefined && (
        <p role="alert">
          The roots list could not be read ({rootsFailure}), so the grant
          editors below show nothing rather than what this library has.
        </p>
      )}

      {users !== undefined && (
        <ul className="user-list">
          {users.map((user) => (
            <UserRow
              key={user.name}
              user={user}
              users={users}
              roots={roots ?? []}
              actingAs={gate?.who.user_id}
              onChanged={reload}
            />
          ))}
        </ul>
      )}

      {addFailure !== undefined && <p role="alert">{addFailure}</p>}
      <form onSubmit={submit} className="add-user">
        <h3>Add an account</h3>
        <label>
          <span>Name</span>
          <input
            type="text"
            name="new-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label>
          <span>Password</span>
          <input
            type="password"
            name="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <label>
          <span>Role</span>
          <select
            name="new-role"
            value={role}
            onChange={(e) => setRole(e.target.value)}
          >
            {ROLES.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>
        <button
          type="submit"
          disabled={adding || name === '' || password === ''}
        >
          {adding ? 'Creating…' : 'Add'}
        </button>
        <p className="why">
          C4 §4: there is no self-service registration — an administrator makes
          accounts. A new account is a reader unless this says otherwise, which
          is the column default `004_auth.sql` chose because any insert that
          omitted the role minted an administrator.
        </p>
      </form>
    </section>
  )
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom Users.test.tsx 2>&1 | tail -12
```

Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/Users.tsx web/src/Users.test.tsx && git commit -m "web: the users screen, with the add form's refusal kept apart from the list's"
```

---

## Task 5: `web/src/backup.ts` — the client, and two conversions

**Files:**
- Create: `web/src/backup.ts`
- Test: `web/src/backup.test.ts` (`node`)

**Interfaces:**
- Consumes: `getJson`, `sendJson` from `./api`.
- Produces: `Backup {name, bytes, taken}`, `listBackups(): Promise<{backups: Backup[]}>`, `takeBackup(): Promise<Backup>`, `backupUrl(name): string`, `humanBytes(bytes): string`, `readableTaken(taken): string`.

- [ ] **Step 1: Write the failing test**

Create `web/src/backup.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  backupUrl,
  humanBytes,
  listBackups,
  readableTaken,
  takeBackup,
} from './backup'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('humanBytes', () => {
  it('counts bytes in bytes', () => {
    expect(humanBytes(0)).toBe('0 B')
    expect(humanBytes(512)).toBe('512 B')
  })

  it('uses binary units and names them as such', () => {
    // KiB rather than kB, because the divisor is 1024. A label that says kB
    // over a 1024-divisor is a figure that disagrees with `ls -l`.
    expect(humanBytes(1024)).toBe('1.0 KiB')
    expect(humanBytes(1536)).toBe('1.5 KiB')
    expect(humanBytes(5 * 1024 * 1024 * 1024)).toBe('5.0 GiB')
  })
})

describe('readableTaken', () => {
  it('spaces out the stamp the name carries', () => {
    expect(readableTaken('20260906T143502Z')).toBe('2026-09-06 14:35:02 UTC')
  })

  it('hands back anything that is not that shape, verbatim', () => {
    // `backup::stamp_of` enforces the 16-character shape, so reaching this is
    // a server that is not this one. Passing the string through says "this is
    // what the server sent"; reformatting a guess would invent a date.
    expect(readableTaken('yesterday')).toBe('yesterday')
  })
})

describe('the backup client', () => {
  it('asks for the list', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json({ backups: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await listBackups()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/backups')
  })

  it('takes one with an empty POST and unwraps it', async () => {
    const fetchMock = vi.fn().mockImplementation(() =>
      json({
        backup: {
          name: 'library-20260906T143502Z.db',
          bytes: 4096,
          taken: '20260906T143502Z',
        },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const taken = await takeBackup()
    expect(taken.name).toBe('library-20260906T143502Z.db')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    expect(init.body).toBeUndefined()
  })

  it('addresses a download by name, encoded', () => {
    expect(backupUrl('library-20260906T143502Z.db')).toBe(
      '/api/library/backups/library-20260906T143502Z.db',
    )
    expect(backupUrl('a b')).toBe('/api/library/backups/a%20b')
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node backup.test.ts 2>&1 | tail -20
```

Expected: FAIL — `Failed to resolve import "./backup"`.

- [ ] **Step 3: Write `web/src/backup.ts`**

```ts
/**
 * The `/api/library/backups*` client — spec C4 §5 and the build design's §6.3,
 * over the routes that shipped in #106.
 *
 * **There is no restore here and no schedule here**, and neither is an
 * omission: C4 §5 puts restore out of band (stop the server, move the file,
 * start it) and the server half declined an in-server timer. A cron entry runs
 * `taiyaku library backup`, and `--keep` is where retention lives.
 *
 * **The download is a URL rather than a fetch**, for the reason `api.ts` gives
 * for `pageUrl`: it goes straight into an `<a href download>`, the route sends
 * `Content-Disposition: attachment`, and the session cookie rides on the
 * request. Fetching a whole database into memory first would buy nothing and
 * cost the file's size in JS heap.
 */
import { getJson, sendJson } from './api'

/** One backup file, as `GET /api/library/backups` sends it. */
export interface Backup {
  /** The file name alone, never a path — the route resolves it against its own listing. */
  name: string
  bytes: number
  /** The timestamp inside the name, `YYYYMMDDTHHMMSSZ`. */
  taken: string
}

/** See `users.ts` for why each module narrows this itself. */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

/** What exists, newest first — the order the route answers in, not re-sorted here. */
export const listBackups = () =>
  getJson<{ backups: Backup[] }>('/api/library/backups')

/**
 * C4 §5's "run now".
 *
 * Two runs inside one second collide and the second is refused `409` with the
 * server's own sentence — the name's timestamp has one-second resolution. That
 * is timing rather than a fault, and the sentence says so; the screen renders
 * it as sent.
 */
export async function takeBackup(): Promise<Backup> {
  const body = await expectBody(
    sendJson<{ backup: Backup }>('/api/library/backups', 'POST'),
  )
  return body.backup
}

/** Where one backup is downloaded from. */
export const backupUrl = (name: string) =>
  `/api/library/backups/${encodeURIComponent(name)}`

const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB']

/**
 * A size in the coarsest binary unit that does not lie about it.
 *
 * **KiB rather than kB**, because the divisor is 1024: a label saying kB over
 * a 1024-divisor is a figure that disagrees with `ls -l`. Bytes are drawn
 * whole; everything above takes one decimal, which is the precision an
 * operator deciding whether a file will fit actually reads.
 */
export function humanBytes(bytes: number): string {
  let size = bytes
  let unit = 0
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024
    unit += 1
  }
  return unit === 0 ? `${size} B` : `${size.toFixed(1)} ${UNITS[unit]}`
}

/**
 * The stamp inside a backup's name, spaced out.
 *
 * **Anything that is not `YYYYMMDDTHHMMSSZ` is handed back verbatim.**
 * `backup::stamp_of` is what decides a backup file is one, and it enforces
 * that shape — so reaching the fallback means a server that is not this one,
 * and passing the string through says what the server sent rather than
 * inventing a date from a guess.
 */
export function readableTaken(taken: string): string {
  const found = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/.exec(taken)
  if (found === null) return taken
  const [, year, month, day, hour, minute, second] = found
  return `${year}-${month}-${day} ${hour}:${minute}:${second} UTC`
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node backup.test.ts 2>&1 | tail -10
```

Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/backup.ts web/src/backup.test.ts && git commit -m "web: the backup client, in binary units that agree with ls"
```

---

## Task 6: `web/src/Backup.tsx` — the screen, and C4 §5's four sentences

**Files:**
- Create: `web/src/Backup.tsx`
- Test: `web/src/Backup.test.tsx` (`jsdom`)

**Interfaces:**
- Consumes: everything Task 5 produced.
- Produces: `Backup` — the component, which Task 7 mounts as `SCREENS.backup`.
  **The component and the wire type share a name**, so the file imports the
  type under an alias: `import { type Backup as StoredBackup, … } from './backup'`.
  The component keeps the plain name, because that is what the tab is called.

- [ ] **Step 1: Write the failing test**

Create `web/src/Backup.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as backupApi from './backup'
import { Backup } from './Backup'
import { mount } from './testing/mount'

vi.mock('./backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./backup')>()),
  listBackups: vi.fn(),
  takeBackup: vi.fn(),
}))

const entry = (over: Partial<backupApi.Backup> = {}): backupApi.Backup => ({
  name: 'library-20260906T143502Z.db',
  bytes: 1024 * 1024 * 12,
  taken: '20260906T143502Z',
  ...over,
})

async function settle() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Backup />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

async function press(host: HTMLElement, selector: string) {
  const button = host.querySelector(selector) as HTMLButtonElement
  if (!button) throw new Error(`no ${selector}`)
  await act(async () => {
    button.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

describe('Backup', () => {
  it('lists what exists, in the order the server sent, with a link each', async () => {
    vi.mocked(backupApi.listBackups).mockResolvedValue({
      backups: [
        entry(),
        entry({ name: 'library-20260901T090000Z.db', taken: '20260901T090000Z' }),
      ],
    })
    const host = await settle()
    const rows = [...host.querySelectorAll('.backup-list > li')]
    expect(rows).toHaveLength(2)
    // Per element rather than over the page: the newest-first order and the
    // per-row link are the two things a whole-page match cannot see.
    expect(rows[0]?.querySelector('a')?.getAttribute('href')).toBe(
      '/api/library/backups/library-20260906T143502Z.db',
    )
    expect(rows[0]?.querySelector('a')?.hasAttribute('download')).toBe(true)
    expect(rows[0]?.textContent).toContain('2026-09-06 14:35:02 UTC')
    expect(rows[0]?.textContent).toContain('12.0 MiB')
    expect(rows[1]?.textContent).toContain('2026-09-01 09:00:00 UTC')
  })

  it('says so when there are none, rather than drawing an empty list', async () => {
    const host = await settle()
    expect(host.textContent).toContain('No backups yet')
  })

  it('takes one and shows it without a page reload', async () => {
    vi.mocked(backupApi.takeBackup).mockResolvedValue(entry())
    vi.mocked(backupApi.listBackups)
      .mockResolvedValueOnce({ backups: [] })
      .mockResolvedValueOnce({ backups: [entry()] })
    const host = await settle()
    await press(host, 'button[name="take"]')
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
    expect(host.querySelectorAll('.backup-list > li')).toHaveLength(1)
  })

  it('draws the same-second collision as the timing it is', async () => {
    // The server's own sentence names the whole-second timestamp. The screen
    // renders it rather than adding a word like "failed" over it — and it
    // cannot classify by status, because the in-memory refusal below is also
    // a 409.
    vi.mocked(backupApi.takeBackup).mockRejectedValue(
      new Error(
        '/api/library/backups: 409 a backup named library-20260906T143502Z.db is already there; the name carries a whole-second timestamp, so a second backup inside the same second has nowhere to go',
      ),
    )
    const host = await settle()
    await press(host, 'button[name="take"]')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'whole-second timestamp',
    )
  })

  it('draws the in-memory library refusal, which is the other 409', async () => {
    vi.mocked(backupApi.listBackups).mockRejectedValue(
      new Error(
        '/api/library/backups: 409 this library is in memory and has no file to back up',
      ),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'in memory and has no file to back up',
    )
  })

  it('sends one POST for two presses before the first answers', async () => {
    vi.mocked(backupApi.takeBackup).mockImplementation(
      () => new Promise(() => undefined),
    )
    const host = await settle()
    await press(host, 'button[name="take"]')
    await press(host, 'button[name="take"]')
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
  })

  it('carries all four of C4 §5 sentences', async () => {
    // Load-bearing copy, asserted one clause at a time: each of the four says
    // something a person cannot find out from the buttons.
    const host = await settle()
    const text = host.textContent ?? ''
    expect(text).toContain('Stop the server')
    expect(text).toContain('password hashes')
    expect(text).toContain('A sidecar export is not a backup')
    expect(text).toContain('taiyaku library backup')
    expect(text).toContain('--keep')
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom Backup.test.tsx 2>&1 | tail -20
```

Expected: FAIL — `Failed to resolve import "./Backup"`.

- [ ] **Step 3: Write `web/src/Backup.tsx`**

```tsx
/**
 * The backup screen — spec C4 §5 and the build design's §6.3.
 *
 * **The copy here is load-bearing rather than decorative**, which is why four
 * sentences are on the screen and not in a doc:
 *
 * - **Restore is out of band.** Stop the server, move the file, start it. C4
 *   §5 declines an in-server restore, so there is no button and the screen
 *   says what to do instead of leaving a person hunting for one.
 * - **A backup is the whole database, password hashes included.**
 *   `backup_api`'s own module doc says this is worth naming because "the admin
 *   screen has a download button" does not, on its face, look like "the admin
 *   screen hands out the password file".
 * - **A sidecar export is not a backup.** Five documents per work and none of
 *   the cross-work data — collections, tags, external ids, users, read
 *   history. That distinction is invisible unless something says it.
 * - **There is no schedule here.** A cron entry runs `taiyaku library backup`
 *   and `--keep` is where retention lives; the screen states the command
 *   rather than offering a control the server half deliberately declined.
 *
 * **The failure is rendered as the server's sentence, never classified by
 * status.** Both refusals this screen can meet are `409`: a second backup
 * inside one second, and a library that is in memory and has no file to back
 * up. A screen that read the status would have to guess which; the sentences
 * say which, and the first of them already names the whole-second timestamp
 * as the timing it is.
 *
 * **The download is an `<a href download>`**, not a fetch — see `backup.ts`.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  type Backup as StoredBackup,
  backupUrl,
  humanBytes,
  listBackups,
  readableTaken,
  takeBackup,
} from './backup'

export function Backup() {
  const [backups, setBackups] = useState<StoredBackup[] | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [taking, setTaking] = useState(false)
  /** Whether a `POST` is already on its way. See `Users.tsx`'s own note. */
  const inFlight = useRef(false)

  const reload = useCallback(() => {
    listBackups()
      .then((body) => {
        setBackups(body.backups)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const take = () => {
    if (inFlight.current) return
    inFlight.current = true
    setTaking(true)
    setFailure(undefined)
    takeBackup()
      .then(() => {
        inFlight.current = false
        setTaking(false)
        reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setTaking(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <section className="backup">
      <h2>Backup</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}

      <button type="button" name="take" onClick={take} disabled={taking}>
        {taking ? 'Backing up…' : 'Back up now'}
      </button>

      {backups !== undefined &&
        (backups.length === 0 ? (
          <p className="empty">
            No backups yet. The button above takes one; it is a
            <code> VACUUM INTO</code> of the live database, so nothing has to
            stop for it.
          </p>
        ) : (
          <ul className="backup-list">
            {backups.map((entry) => (
              <li key={entry.name}>
                <a href={backupUrl(entry.name)} download>
                  {entry.name}
                </a>
                <span className="when">{readableTaken(entry.taken)}</span>
                <span className="size">{humanBytes(entry.bytes)}</span>
              </li>
            ))}
          </ul>
        ))}

      <div className="what-a-backup-is">
        <p>
          <strong>Restore is out of band.</strong> Stop the server, move the
          file into place, start it again. There is no restore button here: a
          restore through a running server would have to quiesce every writer
          in the process, and that is not worth the failure modes it
          introduces.
        </p>
        <p>
          <strong>A backup is the whole database</strong>, password hashes
          included. Anyone who downloads one can read every account&rsquo;s
          stored hash offline. That is what a backup is — the same file an
          operator would copy off the disk — and it is why every route behind
          this screen is administrator-only.
        </p>
        <p>
          <strong>A sidecar export is not a backup.</strong> An export writes
          five documents per work and carries none of the cross-work data:
          collections, tags, external ids, users, read history. Somebody who
          exported their library still needs this.
        </p>
        <p>
          <strong>There is no schedule here.</strong> Run{' '}
          <code>taiyaku library backup</code> from cron for that, and{' '}
          <code>--keep N</code> to prune old ones. The server takes a backup
          only when asked.
        </p>
      </div>
    </section>
  )
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom Backup.test.tsx 2>&1 | tail -12
```

Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/Backup.tsx web/src/Backup.test.tsx && git commit -m "web: the backup screen, and the four things C4 §5 says it must say"
```

---

## Task 7: the two tabs, and the styles

**Files:**
- Modify: `web/src/config.ts` (`TABS`)
- Modify: `web/src/Config.tsx` (`SCREENS`)
- Modify: `web/src/Config.test.tsx` (the new tabs' clients need mocks)
- Modify: `web/src/config.test.ts` (the `TABS` assertion, if it names the list)
- Modify: `web/src/index.css`

**Interfaces:**
- Consumes: `Users` (Task 4), `Backup` (Task 6).
- Produces: `/config/users` and `/config/backup` as routes, and the two tabs in the sub-nav.

- [ ] **Step 1: Check what `config.test.ts` asserts about `TABS`**

```bash
grep -n "TABS" -A 12 web/src/config.test.ts | head -40
```

The existing assertion pins `TABS[0].path === 'rules'` — the landing screen,
which this branch must **not** move. If it also asserts a length, update it;
if it asserts set membership, add the two.

- [ ] **Step 2: Append the tabs, in C4's own section order**

In `web/src/config.ts`:

```ts
export const TABS: Tab[] = [
  { path: 'rules', label: 'Rules' },
  { path: 'roots', label: 'Roots' },
  { path: 'users', label: 'Users' },
  { path: 'backup', label: 'Backup' },
]
```

**Appended, not prepended.** `TABS[0]` is where a bare `/config` lands, and
the build design's §2 records the rule editor becoming the landing screen as
the one intended move of this list.

In `web/src/Config.tsx`:

```tsx
import { Backup } from './Backup'
import { TABS } from './config'
import { RuleEditor } from './RuleEditor'
import { Roots } from './Roots'
import { Users } from './Users'

/** Which component draws each tab. Keyed by `Tab.path`. */
const SCREENS: Record<string, React.ReactNode> = {
  rules: <RuleEditor />,
  roots: <Roots />,
  users: <Users />,
  backup: <Backup />,
}
```

- [ ] **Step 3: Teach `Config.test.tsx` the two new clients**

That test drives **every** tab in `TABS` and asserts each draws something, so
the two new screens' load-time calls need mocks or the suite fails on an
unmocked `fetch`. Add beside the existing three:

```tsx
vi.mock('./users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./users')>()),
  listUsers: vi.fn(),
}))
vi.mock('./backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./backup')>()),
  listBackups: vi.fn(),
}))
```

and in its `beforeEach`:

```tsx
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
```

with `import * as backupApi from './backup'` and `import * as usersApi from
'./users'` at the top.

- [ ] **Step 4: Add the styles**

Append to `web/src/index.css`, after the `.scan-report` block, following the
rule stated in that file's own settings-area comment — **system colours
throughout**, checked by `index.css.test.ts`:

```css
.users .user-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.user-row {
  border: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  border-radius: 4px;
  margin-bottom: 1rem;
  padding: 1rem;
}

.user-row h3 {
  margin: 0;
}

.user-row .why,
.user-row .locked {
  font-size: 0.9em;
  opacity: 0.8;
}

.user-row .fields {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  gap: 0.5rem;
  margin-top: 0.5rem;
}

.user-row .grants ul,
.user-row .session-list,
.user-row .key-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.user-row .session-list li,
.user-row .key-list li {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.5rem;
  padding: 0.25rem 0;
}

.user-row .session-list .when,
.user-row .key-list .when {
  opacity: 0.8;
  font-size: 0.9em;
}

.user-row .session-list .current {
  font-weight: 600;
}

.user-row .actions {
  display: flex;
  gap: 0.5rem;
  margin-top: 1rem;
}

/* The same heavier border `.remove-root` carries, for the same reason: this
   is the second-most destructive route in the product. */
.remove-user {
  border: 2px solid color-mix(in srgb, currentColor 50%, transparent);
  border-radius: 4px;
  padding: 1rem;
}

.remove-user .counts {
  font-weight: 600;
}

.add-user label {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  margin-bottom: 0.5rem;
}

.backup .backup-list {
  list-style: none;
  margin: 1rem 0;
  padding: 0;
}

.backup .backup-list li {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  padding: 0.25rem 0;
}

.backup .backup-list .when,
.backup .backup-list .size {
  opacity: 0.8;
}

.backup .what-a-backup-is {
  margin-top: 1.5rem;
  border-top: 1px solid color-mix(in srgb, currentColor 25%, transparent);
  padding-top: 1rem;
  max-width: 60ch;
}
```

- [ ] **Step 5: Run the whole web suite**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run 2>&1 | tail -15
```

Expected: green, with `Config.test.tsx` now driving four tabs.

- [ ] **Step 6: Commit**

```bash
cd /home/davey/projects/taiyaku && git add web/src/config.ts web/src/Config.tsx web/src/Config.test.tsx web/src/config.test.ts web/src/index.css && git commit -m "web: the settings area gains its last two tabs"
```

---

## Task 8: the gate, the counts, and the docs

**Files:**
- Modify: `docs/status.md` (test counts at both sites, the `web/` prose, `taiyaku-serve`'s users-route prose)
- Modify: `README.md` (administering accounts and taking a backup stop being CLI-only)
- Modify: `scripts/coverage-floor.mjs` (per-file floors and the file count)
- Modify: `web/vite.config.ts` (aggregate thresholds, only if they moved)
- Modify: `docs/notes.md` (anything measured on the way)

**Interfaces:** none — this task ships no product code.

- [ ] **Step 1: Run the full gate and read the failures**

```bash
cd /home/davey/projects/taiyaku && env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | tail -40
```

Expected to fail on the test-count check and probably on the coverage floor's
file count. Both are claims this branch moved.

- [ ] **Step 2: Get the real counts**

```bash
cd /home/davey/projects/taiyaku && cargo nextest run --workspace 2>&1 | tail -3
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node 2>&1 | grep -E 'Tests +[0-9]+'
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom 2>&1 | grep -E 'Tests +[0-9]+'
```

- [ ] **Step 3: Update `docs/status.md`**

Per the open-items ritual:

1. **The Rust test count at both sites** — the `Six crates, N tests` line and
   the `Nine of the N are standing checks` line two lines later.
2. **The `web/` figure** on the same line: node and jsdom, separately.
3. **`taiyaku-serve`'s route prose** — the users routes now mark the caller's
   own session, which is what makes C4 §6.2's "which of these is this browser"
   answerable at all.
4. **The `web/` prose** — the settings area is complete: rules, roots, users,
   backup.
5. **The open-items sweep**, on the way in and out:

```bash
cd /home/davey/projects/taiyaku && grep -c '^| O[0-9]' docs/status.md && grep -c '^| O[0-9]* | \[x\]' docs/status.md && grep -n -A1 '^What remains:' docs/status.md
```

Expected before this branch: 97 rows, 48 closed. **This branch closes nothing
by itself.** Two rows want a sentence rather than a tick, and both are the
"this is what overtook one clause" form the ritual defines:

- **O94** — the users screen is now the thing that makes the unroutable
  account panel visible to a person: `UserRow` says "open any work and press
  A" because there is nowhere else to send them. The row stays **open**; add
  the sentence naming the screen that now depends on it.
  *O94 has since closed — see its entry in `docs/status.md`.*
- **O97** — untouched by this branch; named here only so a reader does not
  expect it closed.

If a review opens or closes anything, do the **full five-part edit** rather
than a tick, and remember the total count *word* at the head of the section is
the one that drifts.

- [ ] **Step 4: Update `README.md`**

Find where the README describes administering accounts and taking backups
through the CLI, and add that the settings area's Users and Backup tabs now do
both from a browser — with restore still out of band and the schedule still a
cron entry. Two sentences; `docs/status.md` is where the argument lives.

- [ ] **Step 5: Update `scripts/coverage-floor.mjs` and re-run**

```bash
cd /home/davey/projects/taiyaku && node scripts/coverage-floor.mjs 2>&1 | tail -25
```

Add the five new files at their honest measured figures and bump the minimum
file count. **Do not lower an existing floor to make the run pass** — if a
file dropped, that is a test to write.

- [ ] **Step 6: Record the measurements in `docs/notes.md`**

Under a new heading dated 2026-09-06, at minimum:

- what `GET /api/library/users` costs on the real library, and how many
  accounts it answered for — the route assembles sessions, keys and grants per
  account, which is three queries per row and the first figure anybody has for
  it;
- what a backup of the real library costs and how big it is —
  `POST /api/library/backups` is a `VACUUM INTO` and no figure exists for one
  on real material;
- the new Rust and web test counts.

Every figure in `status.md` has to trace to one here.

- [ ] **Step 7: Run the gate to green, and quote the line**

```bash
cd /home/davey/projects/taiyaku && env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | grep -E 'all green|FAIL|not measured'
```

Expected: the literal `all green` line, and no `FAIL`. **Do not pipe through
`tail` or append `; echo`** — both mask the exit status.

- [ ] **Step 8: Commit**

```bash
cd /home/davey/projects/taiyaku && git add docs/status.md docs/notes.md README.md scripts/coverage-floor.mjs web/vite.config.ts && git commit -m "docs: the settings area is complete, and a browser can administer accounts"
```

---

## Task 9: verify against real material, sweep, review, open the PR

**Files:** none — this task changes nothing unless it finds something.

- [ ] **Step 1: Drive it against a copy of the real library**

Per `verifying-against-real-material`: **isolate the writes and prove it
afterwards.** Copy, do not point at, the real library — this branch's screens
delete accounts and write the `user` table.

```bash
cd /home/davey/projects/taiyaku && ls /home/davey/temp/manga-test 2>/dev/null || echo "no scratch library — make one before this step"
```

Then serve against a scratch copy, open `/config/users` and `/config/backup`,
and check by eye:

1. The administrator's own card marks **one** session as this browser, and it
   is the right one — the whole point of Task 1.
2. The role control and Remove are both dead on the last administrator, with
   the reason beside them.
3. Removing a reader states five counts that match what that account actually
   has, and nothing is deleted until the confirm.
4. A root ticked and unticked round-trips: reload the page and the grant is
   where it was left.
5. `Back up now` writes a file next to the database, the list grows, and the
   download link actually downloads it. **Read the user-agent and the byte
   count off the log rather than believing the browser** — per
   `taiyaku-device-testing`, the log's byte count is always 0 for a download.
6. Press `Back up now` twice inside a second and read the sentence that comes
   back.

**Look at the pixels** — crop the region before writing any finding up, and
read the numbers rather than the nouns.

- [ ] **Step 2: Run the mutation sweep on what this branch changed**

```bash
cd /home/davey/projects/taiyaku && scripts/mutants.sh --since main 2>&1 | tail -30
```

`--since main` reads `git diff main...HEAD`, so **every fix must be committed
first** or it re-reports it. Read the result against
`taiyaku-mutation-testing-next`: neither a survivor list nor a clean run can be
taken at face value, an *errored* run's verdict is fiction in both directions,
and **check the `unviable` count against what you actually changed**. This
branch's Rust surface is one function's signature and three call sites, so a
sweep that reports a large number of anything is reporting something other
than this branch.

- [ ] **Step 3: Whole-branch review**

Per `whole-branch-review-finds-what-task-review-cannot`: per-task review passed
every task on #91, #105, #106 and #108 while whole-branch review found seven,
two, three and twenty-nine defects respectively. **Review the fixes too** — on
#106 that found three more, one a regression the fix round itself introduced —
and on #108 four passes did not converge.

**After fixing a finding, grep for its shape rather than its function.** This
branch has three shapes worth grepping on sight: a control disabled without
the server also refusing it (the direction §6.1 forbids), a failure slot
shared between two causes, and a `.then` that drops an answer the caller
needed.

```bash
cd /home/davey/projects/taiyaku && git diff main...HEAD --stat
```

Read the whole diff, not the task list.

- [ ] **Step 4: Open the PR**

Per `taiyaku-gitea-not-github`: `gh` is authenticated against the wrong host —
use the gitea MCP. **Never hard-wrap the PR body**; the PR body *is* the squash
commit message, and it appears once.

---

## Self-review

**Spec coverage.**

| Spec | Task |
| --- | --- |
| §6.1 refusal 1, both faces (delete and demote) | 2 (`onlyAdminWhoCanLogIn`), 3 (both controls) |
| §6.1 refusal 2, self-demotion | 3 |
| §6.1 refusal 3, a reset ends the target's sessions | 3 (the copy), server (already shipped) |
| §6.1 refusal 4, no self-reset, `/api/password` named | 3 |
| §6.1 the server is the authority, one-directional duplication | 2, 3 |
| §6.1 the unroutable account panel is named, not fixed | 3, 8 (O94's sentence) |
| §6.2 one card per account: name, role, capabilities, grants, `can_log_in` | 3 |
| §6.2 expanding shows sessions and keys, each revocable | 3 |
| §6.2 "whether it is the browser doing the asking" | **1** — it was `false` on the wire |
| §6.2 removal quotes five counts and the name | 3 |
| §6.2 the roots list comes from `config.ts`'s client | 4 |
| §6.3 back up now, list newest-first, a download link each | 6 |
| §6.3 the four load-bearing sentences | 6 |
| §6.3 the same-second 409 drawn as timing | 5 (the client), 6 (the screen) |
| §6.3 the download is a plain `<a href download>` | 5, 6 |
| §6.4 files | file structure table |
| §7 "the last admin cannot be deleted, and an admin cannot demote themselves — asserted on the controls" | 3 |
| §9 what this invalidates | 8 |

**§7's other named tests belong to PRs A and B** and shipped with them. The two
this design adds that fall here are the control assertions above; the four
scan- and rule-shaped ones are #108's and #109's.

**Type consistency.** `User`, `Grant`, `UserPatch`, `Removal`,
`RemovalAnswer`, `ROLES` and `onlyAdminWhoCanLogIn` are defined once in Task 2
and used unchanged in Tasks 3 and 4. `Backup`, `humanBytes`, `readableTaken`
and `backupUrl` are defined once in Task 5 and used unchanged in Task 6 — where
the type is imported as `StoredBackup` because the component takes the plain
name. `SessionRow` and `ApiKey` are `api.ts`'s, imported rather than
redeclared. `Root` is `config.ts`'s, passed down from Task 4 to Task 3.

**Known unknowns the executor must check rather than assume**, each flagged at
its step: what `config.test.ts` already asserts about `TABS` (Task 7, step 1);
whether `Config.test.tsx`'s existing mocks are enough for two more screens
(Task 7, step 3); the exact `coverage-floor.mjs` table shape and whether
`vite.config.ts`'s aggregate thresholds move (Task 8); and whether
`/home/davey/temp/manga-test` still holds a scratch library (Task 9).

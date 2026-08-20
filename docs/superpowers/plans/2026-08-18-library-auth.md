# The library's authentication — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put every `/api/*` and `/opds/*` route behind a resolved `Principal`, so this
server can replace a Komga and a Stump host reachable from outside the house.

**Architecture:** One `axum` middleware resolves a request to a `Principal` from either
a session cookie or `Authorization: Basic`, and every route reads its user id from that
principal instead of the two hardcoded `LOCAL_USER = 1` constants. Passwords hash with
argon2id; session tokens and API keys are 256-bit CSPRNG secrets stored as sha256, which
is what keeps a 400-page OPDS volume from paying a slow KDF 400 times. Credential
verification lives in `taiyaku-library` because both `taiyaku-serve` and `taiyaku-cli`
need it; the guard, the cookie and the origin check live in `taiyaku-serve`.

**Tech Stack:** Rust, `axum` 0.8, `rusqlite` 0.38, SQLite, `argon2` (the one new crate),
`sha2` 0.11, `rand` 0.10, `base64` 0.23, React + Vite for the reader's login and account
screens.

**Design:** `docs/superpowers/specs/2026-08-14-library-auth-design.md` — spec A2 of the
roadmap in `2026-08-14-library-store-design.md`, §10. Read it first. This plan does not
restate its reasoning; where a decision is not obvious from a task, the design's section
number is cited.

## Global Constraints

- **Migration files are append-only.** `schema.rs`'s own doc: *"Append, never edit. A
  database already at version 1 is never shown migration 1 again."* The `password_hash`
  nullability change is therefore a rebuild inside `004_auth.sql`, never an edit to
  `001_initial.sql`.
- **`SCHEMA_VERSION` and `MIGRATIONS.len()` are one fact written twice**, pinned by
  `const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize);` in
  `crates/taiyaku-library/src/schema.rs`. Both move together or the crate does not compile.
- **`LOGGED_HEADERS` stays an allowlist.** `crates/taiyaku-serve/src/reqlog.rs`, currently
  a 5-entry array. No task adds `cookie` or `authorization` to it, and no task changes the
  request-log middleware to read raw headers.
- **A raw token is never stored.** `session.token_sha256` and `api_key.secret_sha256` are
  the only columns that exist; there is no column a raw token could land in by accident.
- **Argon2id is reached only by the password path.** An API key carries a `tyk_` prefix
  precisely so one string comparison routes it to sha256 instead. A test asserts the key
  path does not reach argon2 (§2, §10).
- **No `--no-auth` and no dev bypass** (§9). Tests construct principals directly and drive
  `build_router` with `tower::ServiceExt::oneshot`; they never log in.
- **Rust edition, lints and gate** are the workspace's. Run
  `env -u TAIYAKU_CORPUS scripts/check-all.sh` before pushing — never with
  `TAIYAKU_CORPUS` set, which drags `geometry_experiment` into a debug-profile
  workspace run measured at over an hour.
- **`docs/status.md` is edited by the ritual its own "Open items" section describes**, and
  measurements go in `docs/notes.md`, not `status.md`.

---

## What changed under this design since it was written

The design is dated 2026-08-14 and says "Not built." Specs A (#76) and B (#77) have both
landed since. Six of its statements are now stale; each is verified against the code at
`cec59e2` and carried into the task that owns it. **The design document is corrected in
place by Task 12** rather than left disagreeing with the plan, following this project's
convention of correcting and marking rather than deleting.

1. **§3 "Migration 2" is now migration 4.** `002_absorbed.sql` and `003_scanner.sql` both
   exist; `SCHEMA_VERSION` is 3. → Task 1.

2. **§8's premise is inverted, and its conclusion survives anyway.** The design says
   `work.root_id` "belongs in spec B, which lands *after* this one." B landed *first* and
   added no work→root link at all: `scan_entry.root_id` and `rule.root_id` exist,
   `member` has no `scan_entry_id`, and `work` has no `root_id`. So the per-root filter is
   still unpopulated, for a different reason than the design gives. **Decided 2026-08-18:**
   A2 ships `user_root` and resolves `Principal.roots`, and the filter lands in C1 with the
   browse API — the first route that can list by root. Adding `work.root_id` here would
   reproduce O45's exact shape, a column written on every scan and read by nothing.
   → Task 3, and recorded in Task 12.

3. **§5.1's export ambiguity is decided.** The database is already multi-user — `note` and
   `progress` are both keyed `(user_id, work_id)` in `001_initial.sql`. Only the *file*
   format is single-user, and that is intended: a sidecar carries one set of notes.
   **Decided 2026-08-18:** `export::work` writes the requesting user's documents and names
   every other user holding state for that work in its report, following the
   `suppressed`/`rekeyed`/`merged` rule — any outcome a person cares about that cannot be
   inferred from the other counts gets a field. Versioning the sibling format was ruled
   out; refusing was ruled out because one-set-per-file is the design, not an error.
   → Task 6.

4. **There are two `LOCAL_USER = 1` constants, not one.**
   `taiyaku-library/src/export.rs:44` and `taiyaku-serve/src/works.rs:288`. Each carries a
   doc comment pointing at A2 and at the other. Both must go. → Task 5 and Task 6.

5. **`truth` is not per-user.** `state::truth(conn, work_id)` and
   `state::put_truth(conn, work_id, doc)` take no `user_id` — ground-truth labels are
   library-wide, not per reader. So A2's per-user surface is `progress` and `notes` only,
   and `/api/works/{work}/truth` is guarded like every other route but resolves no user id.
   → Task 5.

6. **The line numbers in §1 have moved.** `LOGGED_HEADERS` is at `reqlog.rs:44`, not 24.
   It is still a five-entry allowlist and still correct. Cited by symbol from here on.

**One new dependency, and one only.** `base64` 0.23.0, `rand` 0.10.2 and `getrandom` 0.4.3
are already in `Cargo.lock` transitively. `argon2` is not, and is the single genuinely new
crate — the same shape as `notify` in spec B, so Task 2 measures its build cost the same
way rather than asserting it is cheap.

---

## File structure

**Created:**

| Path | Responsibility |
| --- | --- |
| `crates/taiyaku-library/src/schema/004_auth.sql` | Migration 4: capability columns, the `password_hash` rebuild, `session`, `api_key`, `user_root`, `login_attempt` |
| `crates/taiyaku-library/src/credential.rs` | argon2id password hashing, sha256 token hashing, token minting, the `tyk_` prefix. Shared by `serve` and `cli`, which is why it is here and not in either |
| `crates/taiyaku-library/src/credential/tests.rs` | Its unit tests, including the cost band |
| `crates/taiyaku-library/src/account.rs` | Row-level account operations: promote, add, set password, create/revoke key, session lifecycle, `user_root` reads |
| `crates/taiyaku-library/src/account/tests.rs` | Its unit tests, including id preservation |
| `crates/taiyaku-serve/src/auth.rs` | The `Principal`, the middleware, the two 401 shapes, Basic parsing, the origin check, `--trusted-proxy` peer resolution |
| `crates/taiyaku-serve/src/auth/tests.rs` | Guard tests, including the route-enumeration test |
| `crates/taiyaku-serve/src/session.rs` | Cookie construction and parsing, login/logout/claim routes |
| `crates/taiyaku-serve/src/session/tests.rs` | Cookie-flag and lifecycle tests |
| `crates/taiyaku-cli/src/user.rs` | `taiyaku library user …` and `taiyaku library key …` |
| `crates/taiyaku-cli/src/user/tests.rs` | Its tests |
| `web/src/Login.tsx`, `web/src/Account.tsx` | The two new screens |
| `web/src/Login.test.tsx`, `web/src/Account.test.tsx` | Their tests |

**Modified:**

| Path | Change |
| --- | --- |
| `crates/taiyaku-library/src/schema.rs` | `SCHEMA_VERSION` 3 → 4, `MIGRATIONS` gains `004_auth.sql` |
| `crates/taiyaku-library/src/lib.rs` | `mod credential; mod account;` |
| `crates/taiyaku-library/src/export.rs` | `LOCAL_USER` deleted; `work` takes a `user_id`; `Exported` gains `other_users` |
| `crates/taiyaku-library/src/migrate.rs` | Its `export::LOCAL_USER` use becomes an explicit argument |
| `crates/taiyaku-serve/src/works.rs` | `LOCAL_USER` deleted |
| `crates/taiyaku-serve/src/progress.rs`, `notes.rs` | Read the user id from the `Principal` extension |
| `crates/taiyaku-serve/src/routes.rs` | `build_router` gains the guard layer |
| `crates/taiyaku-serve/src/lib.rs` | `ServeState` gains `auth: AuthConfig`; `mod auth; mod session;` |
| `crates/taiyaku-serve/src/test_support.rs` | Fixtures construct an authenticated principal |
| `crates/taiyaku-serve/Cargo.toml` | `base64`, `rand`; `tower` promoted out of dev-dependencies is **not** needed |
| `crates/taiyaku-library/Cargo.toml` | `argon2`, `rand` |
| `crates/taiyaku-cli/src/main.rs` | The `serve` synopsis gains three flags; `library user`/`library key` in usage |
| `crates/taiyaku-cli/src/library.rs` | Dispatch gains `user` and `key` |
| `web/src/api.ts` | 401 routes to login rather than throwing |
| `README.md`, `web/README.md`, `docs/status.md`, `docs/notes.md` | Task 12 |
| `scripts/coverage-floor.mjs`, `scripts/status-test-count.mjs` | Task 12 |

---

## The measurement that shapes Task 1

**Taken 2026-08-18 with `sqlite3` 3.x on this machine, against a two-table
reproduction of `001_initial.sql`'s `user`/`note` pair.**

`Library::open` sets `PRAGMA foreign_keys = ON` (`lib.rs:235`) before migrations run, and
`schema.rs::apply_pending` runs each migration **inside a transaction**. SQLite's
`foreign_keys` pragma *is a no-op inside a transaction* — it neither errors nor warns, it
simply does not take effect. And `DROP TABLE` on a parent performs an implicit `DELETE`
that fires `ON DELETE CASCADE`.

So the twelve-step rebuild written the obvious way, inside migration 4, does this:

```
BEGIN;
PRAGMA foreign_keys=OFF;          -- silently ignored: still reports 1
CREATE TABLE user_new (...);
INSERT INTO user_new SELECT ... FROM user;
DROP TABLE user;                  -- cascades into note and progress
ALTER TABLE user_new RENAME TO user;
COMMIT;
→ notes surviving: 0
```

`progress` and `note` are the only two tables that `REFERENCES user(id) ON DELETE
CASCADE`, and they are the two holding every note and every reading position in the
library. The design's §3 comment warns that getting the rebuild wrong "silently drops the
rows it copies." The real failure is worse and in the other direction: it copies the
`user` rows correctly and drops the **children**.

The fix, verified in the same session:

```
PRAGMA foreign_keys=OFF;          -- OUTSIDE the transaction
BEGIN;
CREATE TABLE user_new (...);
INSERT INTO user_new (id,name,password_hash) SELECT id,name,COALESCE(password_hash,'!') FROM user;
DROP TABLE user;
ALTER TABLE user_new RENAME TO user;
SELECT COUNT(*) FROM pragma_foreign_key_check;   -- 0
COMMIT;
PRAGMA foreign_keys=ON;
→ notes surviving: 1;  user row: 1 local ! ;  note still joins: 1
```

**Two consequences for Task 1.** The pragma has to move outside the transaction, which
means changing `schema.rs::apply_pending` rather than only adding a `.sql` file. And
migration 1's seeded row (`INSERT INTO user (id, name, role) VALUES (1, 'local', 'admin')`)
has a **NULL** `password_hash`, so a straight `NOT NULL` rebuild would fail on it — the
design says "NULL -> NOT NULL" without saying what the existing row gets. It gets `'!'`,
which is not a well-formed PHC string, so `credential::verify` refuses it the way
`/etc/shadow` refuses a locked account. `user promote` replaces it.

This measurement goes in `docs/notes.md` in Task 12, not in `status.md`.

---

## Tasks

### Task 1: Migration 4, and the foreign-key pragma that has to move

**Files:**
- Create: `crates/taiyaku-library/src/schema/004_auth.sql`
- Modify: `crates/taiyaku-library/src/schema.rs` — `SCHEMA_VERSION`, `MIGRATIONS`, `apply_pending`
- Test: `crates/taiyaku-library/src/schema/tests.rs` (exists; add to it)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: schema version 4; tables `session`, `api_key`, `user_root`, `login_attempt`;
  `user.can_download`, `user.can_stream`, `user.password_hash NOT NULL`. Task 2 hashes into
  `user.password_hash`; Task 3 reads `user_root`; Task 4 writes `session`; Task 7 writes
  `login_attempt`; Task 8 writes `api_key`.

- [ ] **Step 1: Write the failing test — the one that would have caught the cascade**

Add to `crates/taiyaku-library/src/schema/tests.rs`:

```rust
/// Migration 4 rebuilds `user` to make `password_hash` NOT NULL. `progress`
/// and `note` both `REFERENCES user(id) ON DELETE CASCADE`, and `DROP TABLE`
/// on a parent fires those cascades — so a rebuild that runs with foreign
/// keys enabled copies the `user` rows correctly and deletes every note and
/// reading position in the library. Measured on 2026-08-18: notes surviving,
/// 0. This test is the reason the pragma sits outside the transaction.
#[test]
fn migration_4_rebuilds_user_without_taking_the_notes_with_it() {
    let conn = rusqlite::Connection::open_in_memory().expect("opens");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");

    // Stop at version 3, then plant the state a real library would hold.
    migrate_with(&conn, &MIGRATIONS[..3]).expect("migrates to 3");
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub)
         VALUES ('k', 's', 'T', 'rtl', 'raw')",
        [],
    )
    .expect("work");
    let work_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('n1', 1, ?1, 'raw', 0, 'a note', 'now', 'now')",
        [work_id],
    )
    .expect("note");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 7)",
        [work_id],
    )
    .expect("progress");

    migrate(&conn).expect("migrates to 4");

    let notes: i64 = conn
        .query_row("SELECT COUNT(*) FROM note", [], |r| r.get(0))
        .expect("counts notes");
    let positions: i64 = conn
        .query_row("SELECT COUNT(*) FROM progress", [], |r| r.get(0))
        .expect("counts progress");
    assert_eq!(notes, 1, "migration 4 cascaded the notes away");
    assert_eq!(positions, 1, "migration 4 cascaded the reading positions away");

    // The id is the thing spec A2 §5 turns on: promotion must reach this row,
    // not insert beside it.
    let (id, hash): (i64, String) = conn
        .query_row("SELECT id, password_hash FROM user", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("reads the user");
    assert_eq!(id, 1, "the seeded user's id did not survive the rebuild");
    assert_eq!(hash, "!", "a NULL password_hash must become the locked placeholder");

    // And the child rows still join to it, which a rebuild that renamed the
    // parent without repointing the children would fail.
    let joined: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM note JOIN user ON user.id = note.user_id",
            [],
            |r| r.get(0),
        )
        .expect("joins");
    assert_eq!(joined, 1, "note no longer joins to user");
}
```

- [ ] **Step 2: Run it and confirm it fails for the right reason**

```bash
cargo test -p taiyaku-library schema::tests::migration_4 -- --nocapture
```

Expected: FAIL — `index out of bounds` or `MIGRATIONS` has 3 entries, because migration 4
does not exist yet. That is the correct first failure.

- [ ] **Step 3: Write `004_auth.sql`**

```sql
-- Spec A2's schema. See docs/superpowers/specs/2026-08-14-library-auth-design.md §3.
--
-- **This migration rebuilds `user`, and that is why `apply_pending` turns
-- foreign keys off around the transaction rather than here.** `PRAGMA
-- foreign_keys` is a no-op inside a transaction — it does not error, it does
-- not warn, it simply does not apply — and `DROP TABLE` on a parent fires
-- `ON DELETE CASCADE`. `progress` and `note` are the two tables that
-- reference `user(id)`, and they hold every note and every reading position
-- in the library. Written the obvious way, inside the transaction, this
-- migration copies the `user` rows correctly and deletes all of them.
-- Measured 2026-08-18: notes surviving, 0. See `docs/notes.md`.

ALTER TABLE user ADD COLUMN can_download INTEGER NOT NULL DEFAULT 1;
ALTER TABLE user ADD COLUMN can_stream   INTEGER NOT NULL DEFAULT 1;

-- `password_hash` NULL -> NOT NULL, the documented twelve-step rebuild.
--
-- Migration 1 seeds `(1, 'local', 'admin')` with a NULL hash, so "make it NOT
-- NULL" needs an answer for that row rather than a constraint it violates. It
-- gets `'!'`: not a well-formed PHC string, so `credential::verify` refuses it
-- for every password, the way /etc/shadow refuses a locked account. `taiyaku
-- library user promote` replaces it, and §5 requires that promotion reach
-- THIS row — id 1 — because every note and position recorded between spec A
-- and now hangs off it.
CREATE TABLE user_new (
  id            INTEGER PRIMARY KEY,
  name          TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  role          TEXT NOT NULL DEFAULT 'admin',
  created       TEXT NOT NULL DEFAULT (datetime('now')),
  can_download  INTEGER NOT NULL DEFAULT 1,
  can_stream    INTEGER NOT NULL DEFAULT 1
);

INSERT INTO user_new (id, name, password_hash, role, created, can_download, can_stream)
SELECT id, name, COALESCE(password_hash, '!'), role, created, can_download, can_stream
FROM user;

DROP TABLE user;
ALTER TABLE user_new RENAME TO user;

-- Keyed by the token's HASH rather than carrying a separate id, so there is no
-- second identifier and no column a raw token could be stored in by accident:
-- the column that would hold it does not exist (§2).
CREATE TABLE session (
  token_sha256 TEXT PRIMARY KEY,
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  created      TEXT NOT NULL DEFAULT (datetime('now')),
  last_seen    TEXT NOT NULL DEFAULT (datetime('now')),
  expires      TEXT NOT NULL,
  user_agent   TEXT,
  ip           TEXT
) WITHOUT ROWID;

CREATE INDEX session_by_user ON session (user_id);

-- `name` is what a person calls the device, so revoking the right one does not
-- require guessing. `expires` NULL means it does not.
CREATE TABLE api_key (
  id            INTEGER PRIMARY KEY,
  user_id       INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  secret_sha256 TEXT NOT NULL UNIQUE,
  created       TEXT NOT NULL DEFAULT (datetime('now')),
  last_used     TEXT,
  expires       TEXT
);

CREATE INDEX api_key_by_user ON api_key (user_id);

-- Per-root sharing. **Nothing filters on this yet, and that is recorded rather
-- than discovered**: `serve` takes sidecar paths, no route lists by root, and
-- `work` has no `root_id` — spec B landed without adding one. The guard
-- resolves `Principal.roots` from here; spec C1's browse API is the first
-- caller that can act on it. In migration 4 rather than deferred because it
-- holds hand-entered data, which is the store design's §10 rule.
CREATE TABLE user_root (
  user_id INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  root_id INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  PRIMARY KEY (user_id, root_id)
) WITHOUT ROWID;

-- `key` is the (client address, username) pair joined, per §7 — NOT the
-- account alone. OPDS clients retry hard, and keying on the account would let
-- one misconfigured reader lock its owner out of every device at once. Keyed
-- on the pair, the misconfigured client locks out only itself.
CREATE TABLE login_attempt (
  key          TEXT PRIMARY KEY,
  window_start TEXT NOT NULL,
  count        INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
```

- [ ] **Step 4: Register it, and move the pragma**

In `crates/taiyaku-library/src/schema.rs`, bump both halves of the one fact:

```rust
/// The schema version this build expects.
pub const SCHEMA_VERSION: u32 = 4;

const MIGRATIONS: [&str; 4] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
];
```

Then wrap `apply_pending`'s loop. Add this to `migrate_with`, around the `apply_pending`
call — outside any transaction, which is the whole point:

```rust
/// Foreign keys, off for the duration of the migration run and on again after.
///
/// **This cannot live inside a migration.** `PRAGMA foreign_keys` is a no-op
/// within a transaction — SQLite neither errors nor warns, it simply does not
/// apply it — and `apply_pending` runs every migration inside one. A migration
/// that rebuilds a table therefore runs with foreign keys ON however loudly its
/// own SQL asks otherwise, and `DROP TABLE` on a parent fires `ON DELETE
/// CASCADE` into its children. Migration 4 rebuilds `user`; `progress` and
/// `note` reference it; measured on 2026-08-18, the obvious version of that
/// migration leaves zero notes behind.
///
/// The restored value is read rather than assumed ON, because `Library::open`
/// sets it and a caller holding a bare `Connection` — which is what the tests
/// in this module use — may not have.
fn without_foreign_keys<T>(
    conn: &Connection,
    body: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let was: bool = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|e| format!("reading the foreign_keys pragma: {e}"))?;
    conn.pragma_update(None, "foreign_keys", "OFF")
        .map_err(|e| format!("disabling foreign keys for the migration: {e}"))?;
    let outcome = body();
    // Restored on both paths. A failed migration that left foreign keys off
    // would hand the caller a Connection that silently accepts orphans for the
    // rest of its life, which is a worse outcome than the migration failure.
    conn.pragma_update(None, "foreign_keys", if was { "ON" } else { "OFF" })
        .map_err(|e| format!("restoring the foreign_keys pragma: {e}"))?;
    outcome
}
```

and call it from `migrate_with`:

```rust
fn migrate_with(conn: &Connection, migrations: &[&str]) -> Result<(), String> {
    let known = u32::try_from(migrations.len()).unwrap_or(u32::MAX);
    ensure_meta(conn)?;
    let current = version(conn)?;
    if current > known {
        return Err(written_by_a_newer_taiyaku(current, known));
    }
    without_foreign_keys(conn, || apply_pending(conn, migrations, current))
}
```

- [ ] **Step 5: Add the `foreign_key_check` that makes the disabling safe**

Turning foreign keys off for a migration means a migration *could* leave orphans and
nothing would notice. Inside `apply_pending`'s per-migration transaction, after the
migration's SQL and before the version row is written, add:

```rust
// **Disabling foreign keys for the run means nothing enforces them during
// it**, so the check that would have happened continuously happens once,
// here, inside the same transaction — a migration that leaves an orphan
// rolls back rather than committing a database the next `Library::open`
// will enforce constraints over and fail on.
let orphans: i64 = tx
    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("checking foreign keys after migration {target}: {e}"))?;
if orphans > 0 {
    return Err(format!(
        "migration {target} left {orphans} row(s) violating a foreign key; \
         rolling back rather than committing a database that cannot be reopened"
    ));
}
```

- [ ] **Step 6: Run the test**

```bash
cargo test -p taiyaku-library schema:: -- --nocapture
```

Expected: PASS, including `migration_4_rebuilds_user_without_taking_the_notes_with_it`
and every pre-existing migration test.

- [ ] **Step 7: Prove the guard is load-bearing, not decorative**

Temporarily change `migrate_with` back to `apply_pending(conn, migrations, current)` with
no wrapper, re-run the test, and confirm it **fails** with `migration 4 cascaded the notes
away`. Then put the wrapper back. A guard whose removal breaks no test is not tested.

```bash
cargo test -p taiyaku-library schema::tests::migration_4 -- --nocapture
```

Expected with the wrapper removed: FAIL, `assertion `left == right` failed: migration 4
cascaded the notes away; left: 0, right: 1`.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/004_auth.sql crates/taiyaku-library/src/schema/tests.rs
git commit -m "Migration 4 rebuilds user, and the foreign-key pragma moves out of the transaction that made it a no-op"
```

---

### Task 2: Credential verification — argon2id for passwords, sha256 for tokens

**Files:**
- Create: `crates/taiyaku-library/src/credential.rs`, `crates/taiyaku-library/src/credential/tests.rs`
- Modify: `crates/taiyaku-library/Cargo.toml`, `crates/taiyaku-library/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces, and every later task uses these exact names:
  ```rust
  pub const API_KEY_PREFIX: &str = "tyk_";
  pub fn hash_password(password: &str) -> Result<String, String>;
  pub fn verify_password(password: &str, phc: &str) -> bool;
  pub fn mint_token() -> String;              // 32 CSPRNG bytes, base64url, unpadded
  pub fn mint_api_key() -> String;            // API_KEY_PREFIX + mint_token()
  pub fn looks_like_api_key(secret: &str) -> bool;
  pub fn token_sha256(token: &str) -> String; // lowercase hex, 64 chars
  ```

- [ ] **Step 1: Add the dependency and pin what resolves**

`argon2` is the one genuinely new crate. `rand` and `base64` are already in `Cargo.lock`
transitively, so naming them here adds no new tree.

```bash
cargo add --package taiyaku-library argon2
cargo add --package taiyaku-library rand
cargo add --package taiyaku-library base64
```

Record the resolved versions — do not assume them. Then measure what the new crate costs
to build, the way spec B measured `notify`:

```bash
cargo clean && /usr/bin/time -v cargo build --workspace 2>&1 | grep -E "Elapsed|User time|System time"
```

Note the figure. It goes in `docs/notes.md` in Task 12, and no inference is drawn from it
about CI's budget — spec B's precedent, and the reason its `notify` measurement says so
explicitly.

- [ ] **Step 2: Write the failing tests**

`crates/taiyaku-library/src/credential/tests.rs`:

```rust
use super::*;

#[test]
fn a_password_round_trips_and_a_wrong_one_does_not() {
    let phc = hash_password("correct horse").expect("hashes");
    assert!(verify_password("correct horse", &phc));
    assert!(!verify_password("correct horse ", &phc));
    assert!(!verify_password("", &phc));
}

/// Migration 4 writes `'!'` for the seeded row, which is not a well-formed PHC
/// string. `verify_password` must answer **false** rather than erroring, so a
/// locked account is a failed login and not a 500.
#[test]
fn the_locked_placeholder_verifies_against_nothing() {
    assert!(!verify_password("", "!"));
    assert!(!verify_password("anything", "!"));
    assert!(!verify_password("!", "!"));
}

/// Two hashes of one password differ, because each carries its own salt. A
/// scheme that produced the same string twice would let a database leak show
/// which accounts share a password.
#[test]
fn the_same_password_hashes_differently_every_time() {
    let a = hash_password("shared").expect("hashes");
    let b = hash_password("shared").expect("hashes");
    assert_ne!(a, b);
    assert!(verify_password("shared", &a));
    assert!(verify_password("shared", &b));
}

/// §2: the whole point of the `tyk_` prefix is that a key never reaches
/// argon2. Asserted on the discriminator itself, because the cost is
/// unobservable otherwise and a later refactor could quietly reintroduce it.
#[test]
fn an_api_key_is_recognisable_without_hashing_anything() {
    let key = mint_api_key();
    assert!(key.starts_with(API_KEY_PREFIX));
    assert!(looks_like_api_key(&key));
    assert!(!looks_like_api_key("a password"));
    // A password that happens to start with the prefix is still routed to the
    // key verifier, which will simply not find it. That is correct: the
    // alternative is trying both, which is the cost this design avoids.
    assert!(looks_like_api_key("tyk_not-really-a-key"));
}

#[test]
fn a_minted_token_is_256_bits_and_never_repeats() {
    let a = mint_token();
    let b = mint_token();
    assert_ne!(a, b);
    // 32 bytes base64url-unpadded is 43 characters.
    assert_eq!(a.len(), 43, "token is not 32 bytes of entropy");
    assert!(!a.contains('='), "padding would make the cookie value ambiguous");
}

#[test]
fn token_hashing_is_hex_sha256_and_is_not_the_token() {
    let token = mint_token();
    let hashed = token_sha256(&token);
    assert_eq!(hashed.len(), 64);
    assert!(hashed.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    assert_ne!(hashed, token, "the stored value must not be the token");
    assert_eq!(hashed, token_sha256(&token), "hashing is not stable");
}

/// §10: "argon2id cost measured and asserted to be within a band, not merely
/// correct — a parameter set that verifies correctly and takes 2 ms is the
/// defect."
///
/// **A lower bound only.** An upper bound is flaky on a loaded machine and
/// meaningless across profiles; a debug build is slower than release, never
/// faster, so a floor holds in both. The defect this guards against is a
/// parameter set that is too cheap, which is exactly what a floor catches.
#[test]
fn argon2_is_slow_on_purpose() {
    let started = std::time::Instant::now();
    let phc = hash_password("measure me").expect("hashes");
    assert!(verify_password("measure me", &phc));
    let elapsed = started.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(15),
        "argon2id took {elapsed:?}; a parameter set this cheap is the defect \
         §2 names, not a fast machine"
    );
}
```

- [ ] **Step 3: Run them and confirm they fail**

```bash
cargo test -p taiyaku-library credential:: -- --nocapture
```

Expected: FAIL to compile — `credential` module does not exist.

- [ ] **Step 4: Write `credential.rs`**

```rust
//! The two hashing algorithms, chosen by the entropy of what they protect.
//!
//! | Secret | Hash | Why |
//! | --- | --- | --- |
//! | Account password | argon2id | low entropy, human-chosen. Slowness *is* the defence |
//! | Session token | sha256 | 256 bits from a CSPRNG. Nothing to brute-force |
//! | API key secret | sha256 | same |
//!
//! **Argon2 on an API key would be a performance defect, not a conservative
//! choice.** Every page request from an OPDS client carries the credential, so
//! a deliberately slow KDF at ~100 ms puts roughly 40 seconds of pure hashing
//! on a 400-page volume. Fast hashing is correct for a high-entropy secret
//! precisely because there is nothing for slowness to buy.
//!
//! [`API_KEY_PREFIX`] is what makes that split reachable in one comparison
//! rather than by attempting an argon2 verify and falling back — which would
//! reintroduce the exact cost the table above exists to avoid.

use base64::Engine as _;
use sha2::Digest as _;

/// What every API key starts with, and the whole of how a key is told from a
/// password in Basic auth's opaque password field.
///
/// Load-bearing rather than cosmetic: without it the server would have to try
/// argon2 on every OPDS request before falling back to the key verifier.
pub const API_KEY_PREFIX: &str = "tyk_";

/// OWASP's current recommendation for argon2id, and a **starting point rather
/// than a measurement**. Spec A2 §14 records that these are unverified on the
/// host that will run this; `tests::argon2_is_slow_on_purpose` is the floor
/// that stops them being silently lowered, and the measurement on the
/// deployment host is what makes them final.
const M_COST_KIB: u32 = 19_456;
const T_COST: u32 = 2;
const P_COST: u32 = 1;

fn params() -> argon2::Params {
    argon2::Params::new(M_COST_KIB, T_COST, P_COST, None)
        .expect("OWASP's argon2id parameters are in range")
}

fn hasher() -> argon2::Argon2<'static> {
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params())
}

/// Hash a human-chosen password for storage in `user.password_hash`.
///
/// # Errors
///
/// Returns an error if the hasher rejects the input, which it does only for
/// inputs longer than argon2's own maximum.
pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{PasswordHasher as _, SaltString};
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("hashing the password: {e}"))
}

/// Whether `password` matches the stored PHC string.
///
/// **Answers `false` rather than erroring on a malformed hash**, which is what
/// migration 4's `'!'` placeholder is: a locked account must be a failed login,
/// not a 500 that tells an attacker the account exists in a different way from
/// every other failure.
#[must_use]
pub fn verify_password(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier as _};
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    hasher().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// 32 bytes from the OS CSPRNG, base64url without padding — 43 characters.
///
/// Unpadded deliberately: `=` in a cookie value is legal but needs quoting, and
/// a token that round-trips through a header without escaping is one less thing
/// to get wrong.
#[must_use]
pub fn mint_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A minted API key: the prefix, then a token.
#[must_use]
pub fn mint_api_key() -> String {
    format!("{API_KEY_PREFIX}{}", mint_token())
}

/// Whether a Basic-auth password field holds an API key rather than a password.
///
/// A password that happens to start with `tyk_` is routed to the key verifier
/// and simply not found. That is correct: the alternative is trying both, which
/// is the cost this design exists to avoid.
#[must_use]
pub fn looks_like_api_key(secret: &str) -> bool {
    secret.starts_with(API_KEY_PREFIX)
}

/// The stored form of a session token or API key: lowercase hex sha256.
///
/// The **only** form either is ever written in. `session` is keyed by this and
/// `api_key.secret_sha256` is unique on it, so there is no column a raw token
/// could land in by accident.
#[must_use]
pub fn token_sha256(token: &str) -> String {
    let digest = sha2::Sha256::digest(token.as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut acc, byte| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{byte:02x}");
        acc
    })
}

#[cfg(test)]
mod tests;
```

Add `pub mod credential;` to `crates/taiyaku-library/src/lib.rs`.

- [ ] **Step 5: Run them**

```bash
cargo test -p taiyaku-library credential:: -- --nocapture
```

Expected: PASS, all seven.

If `rand::fill` or the `SaltString::generate` signature does not match the version that
resolved in Step 1, fix the call rather than the test — the tests assert behaviour, not
API shape.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/credential.rs crates/taiyaku-library/src/credential/tests.rs crates/taiyaku-library/src/lib.rs crates/taiyaku-library/Cargo.toml Cargo.lock
git commit -m "Two hashing algorithms, chosen by entropy, and a prefix that keeps an OPDS volume from paying argon2 four hundred times"
```

---

### Task 3: `Principal`, the guard, and the two 401 shapes

**Files:**
- Create: `crates/taiyaku-library/src/account.rs`, `crates/taiyaku-library/src/account/tests.rs`
- Create: `crates/taiyaku-serve/src/auth.rs`, `crates/taiyaku-serve/src/auth/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (`build_router`), `crates/taiyaku-serve/src/lib.rs`, `crates/taiyaku-serve/Cargo.toml`, `crates/taiyaku-serve/src/test_support.rs`

**Interfaces:**
- Consumes: `credential::{verify_password, looks_like_api_key, token_sha256, API_KEY_PREFIX}`.
- Produces:
  ```rust
  // taiyaku-library
  pub struct Account { pub id: i64, pub name: String, pub role: String,
                       pub can_download: bool, pub can_stream: bool }
  pub fn by_name(conn: &Connection, name: &str) -> Result<Option<(Account, String)>, String>;
  pub fn by_api_key(conn: &Connection, secret: &str) -> Result<Option<Account>, String>;
  pub fn roots_for(conn: &Connection, user_id: i64) -> Result<Vec<i64>, String>;
  pub fn admin_exists(conn: &Connection) -> Result<bool, String>;

  // taiyaku-serve
  pub struct Principal { pub user_id: i64, pub role: Role, pub can_download: bool,
                         pub can_stream: bool, pub roots: Vec<i64> }
  pub enum Role { Admin, User }
  pub(crate) fn guarded_routes() -> Vec<(&'static str, MethodRouter<Arc<ServeState>>)>;
  ```
  Task 4 inserts `Principal` as a request extension from the cookie path; Task 5 reads
  `Principal::user_id` in the progress and notes routes.

- [ ] **Step 1: Write the account lookups' failing tests**

`crates/taiyaku-library/src/account/tests.rs`:

```rust
use super::*;
use crate::credential;

fn library() -> crate::Library {
    crate::Library::open_in_memory().expect("in-memory library")
}

#[test]
fn an_account_is_found_by_name_with_its_stored_hash() {
    let lib = library();
    let conn = lib.conn();
    let phc = credential::hash_password("hunter2").expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("sets the hash");

    let (account, stored) = by_name(&conn, "local").expect("reads").expect("exists");
    assert_eq!(account.id, 1);
    assert_eq!(account.name, "local");
    assert_eq!(account.role, "admin");
    assert!(account.can_download);
    assert!(account.can_stream);
    assert!(credential::verify_password("hunter2", &stored));
    assert!(by_name(&conn, "nobody").expect("reads").is_none());
}

#[test]
fn an_api_key_resolves_to_its_owner_and_a_revoked_one_does_not() {
    let lib = library();
    let conn = lib.conn();
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'phone', ?1)",
        [credential::token_sha256(&secret)],
    )
    .expect("inserts");

    let found = by_api_key(&conn, &secret).expect("reads").expect("resolves");
    assert_eq!(found.id, 1);
    assert!(by_api_key(&conn, "tyk_wrong").expect("reads").is_none());

    conn.execute("DELETE FROM api_key", []).expect("revokes");
    assert!(by_api_key(&conn, &secret).expect("reads").is_none());
}

/// An expired key is refused, and the refusal is the same shape as a wrong one
/// — a client cannot tell "this key was real once" from "this key never was".
#[test]
fn an_expired_api_key_is_refused() {
    let lib = library();
    let conn = lib.conn();
    let secret = credential::mint_api_key();
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256, expires)
         VALUES (1, 'old', ?1, datetime('now', '-1 day'))",
        [credential::token_sha256(&secret)],
    )
    .expect("inserts");
    assert!(by_api_key(&conn, &secret).expect("reads").is_none());
}

/// §8: `user_root` is populated by hand and read by the guard. Nothing filters
/// on the answer until spec C1, which is recorded rather than discovered.
#[test]
fn roots_are_read_per_user() {
    let lib = library();
    let conn = lib.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (7, '/library')", [])
        .expect("root");
    assert_eq!(roots_for(&conn, 1).expect("reads"), Vec::<i64>::new());
    conn.execute("INSERT INTO user_root (user_id, root_id) VALUES (1, 7)", [])
        .expect("grant");
    assert_eq!(roots_for(&conn, 1).expect("reads"), vec![7]);
}

/// §5's bootstrap turns on this answer: `--allow-web-setup`'s claim route is
/// open only while it is false. The seeded row has role 'admin' but a locked
/// password, so "an admin exists" must mean an admin who can *log in*.
#[test]
fn a_locked_admin_does_not_count_as_an_admin_existing() {
    let lib = library();
    let conn = lib.conn();
    assert!(!admin_exists(&conn).expect("reads"), "the seeded locked row counted");
    let phc = credential::hash_password("real").expect("hashes");
    conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
        .expect("promotes");
    assert!(admin_exists(&conn).expect("reads"));
}
```

- [ ] **Step 2: Run and confirm they fail**

```bash
cargo test -p taiyaku-library account:: -- --nocapture
```

Expected: FAIL to compile — no `account` module.

- [ ] **Step 3: Write `account.rs`**

```rust
//! Row-level account operations: who an incoming credential is, and what they
//! may see.
//!
//! Kept in `taiyaku-library` rather than in `taiyaku-serve` because
//! `taiyaku-cli` needs the same operations for `library user promote` and
//! `library key create`, and `taiyaku-library` is the only crate both depend
//! on. The HTTP shape of a refusal is `taiyaku-serve`'s business; who the
//! credential belongs to is this crate's.

use rusqlite::{Connection, OptionalExtension as _, params};

use crate::credential;

/// One account, minus its password hash — which is returned separately by
/// [`by_name`] so that no struct in this crate can carry a hash somewhere it
/// was not expected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub can_download: bool,
    pub can_stream: bool,
}

fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get("id")?,
        name: row.get("name")?,
        role: row.get("role")?,
        can_download: row.get::<_, i64>("can_download")? != 0,
        can_stream: row.get::<_, i64>("can_stream")? != 0,
    })
}

/// The account with this name, and its stored PHC string.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn by_name(conn: &Connection, name: &str) -> Result<Option<(Account, String)>, String> {
    conn.query_row(
        "SELECT id, name, role, can_download, can_stream, password_hash
         FROM user WHERE name = ?1",
        params![name],
        |row| Ok((account_from_row(row)?, row.get::<_, String>("password_hash")?)),
    )
    .optional()
    .map_err(|e| format!("looking up the account `{name}`: {e}"))
}

/// The account this API key belongs to, or `None` for a key that is unknown,
/// revoked or expired — one answer for all three, so a client cannot tell them
/// apart.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn by_api_key(conn: &Connection, secret: &str) -> Result<Option<Account>, String> {
    let hashed = credential::token_sha256(secret);
    let found = conn
        .query_row(
            "SELECT u.id, u.name, u.role, u.can_download, u.can_stream
             FROM api_key k JOIN user u ON u.id = k.user_id
             WHERE k.secret_sha256 = ?1
               AND (k.expires IS NULL OR k.expires > datetime('now'))",
            params![hashed],
            account_from_row,
        )
        .optional()
        .map_err(|e| format!("looking up an API key: {e}"))?;
    if found.is_some() {
        // Best effort: a failed touch must not fail the request it was
        // recording, since the credential was already valid.
        let _ = conn.execute(
            "UPDATE api_key SET last_used = datetime('now') WHERE secret_sha256 = ?1",
            params![hashed],
        );
    }
    Ok(found)
}

/// The root ids this user has been granted, ascending.
///
/// **Nothing filters on the answer yet**, and that is spec A2 §8 as amended on
/// 2026-08-18: `serve` is handed sidecar paths, no route lists by root, and
/// spec B landed without giving `work` a `root_id`. Spec C1's browse API is the
/// first caller that can act on this. Resolved here anyway so the guard carries
/// a complete principal and C1 adds a filter rather than a lookup.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn roots_for(conn: &Connection, user_id: i64) -> Result<Vec<i64>, String> {
    let mut statement = conn
        .prepare("SELECT root_id FROM user_root WHERE user_id = ?1 ORDER BY root_id")
        .map_err(|e| format!("preparing the root lookup: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| row.get(0))
        .map_err(|e| format!("reading granted roots: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| format!("reading granted roots: {e}"))
}

/// Whether any admin can actually log in.
///
/// **Role is not enough.** Migration 1 seeds `(1, 'local', 'admin')` and
/// migration 4 gives it the locked `'!'` placeholder, so a fresh library has an
/// admin row and no admin. §5's claim route is open only while this is false,
/// and reading role alone would close it before anyone could claim the server.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn admin_exists(conn: &Connection) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM user WHERE role = 'admin' AND password_hash <> '!'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .map_err(|e| format!("counting admins: {e}"))
}

#[cfg(test)]
mod tests;
```

Add `pub mod account;` to `crates/taiyaku-library/src/lib.rs`.

- [ ] **Step 4: Run the account tests**

```bash
cargo test -p taiyaku-library account:: -- --nocapture
```

Expected: PASS, all five.

- [ ] **Step 5: Write the guard's failing tests, including the enumeration**

`crates/taiyaku-serve/src/auth/tests.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt as _;

use super::*;

/// **Every guarded route refuses an anonymous request, enumerated from the
/// router's own route list rather than from a checklist.**
///
/// A reviewer's checklist goes stale the first time someone adds a route and
/// does not think about auth, which is exactly the failure this guards. Driving
/// it off `guarded_routes()` — the same list `build_router` builds from — means
/// a new unguarded route fails the suite on the commit that adds it.
#[test]
fn every_route_refuses_an_anonymous_request() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let patterns: Vec<&'static str> = crate::routes::guarded_routes()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert!(
        patterns.len() >= 8,
        "guarded_routes() answered {} paths; the router had eight /api routes \
         plus the OPDS feed when this test was written, so a smaller number \
         means routes moved out from behind the guard",
        patterns.len()
    );

    for pattern in patterns {
        // `{work}` -> `t`, `{edition}` -> `raw`, `{index}` -> `0`: the fixture
        // work's own identifiers, so a 401 cannot be a 404 wearing a disguise.
        let path = pattern
            .replace("{work}", "t")
            .replace("{edition}", "raw")
            .replace("{index}", "0");
        let state = std::sync::Arc::new(crate::test_support::state("auth-enum"));
        let router = crate::routes::build_router(state);
        let response = runtime
            .block_on(router.oneshot(Request::get(&path).body(Body::empty()).unwrap()))
            .expect("router answers");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} answered {} to an anonymous request",
            response.status()
        );
    }
}

/// §4: `/opds` sends `WWW-Authenticate` because that is what makes an OPDS
/// client prompt. `/api` must not, because it makes the browser raise its own
/// native dialog over the reader — which the web app cannot style, pre-fill or
/// recover from.
#[test]
fn the_two_401_shapes_differ_by_path() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let state = std::sync::Arc::new(crate::test_support::state("auth-401-api"));
    let api = runtime
        .block_on(
            crate::routes::build_router(state)
                .oneshot(Request::get("/api/works").body(Body::empty()).unwrap()),
        )
        .expect("answers");
    assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
    assert!(
        api.headers().get("www-authenticate").is_none(),
        "an /api 401 carrying WWW-Authenticate raises the browser's own dialog"
    );

    let state = std::sync::Arc::new(crate::test_support::state("auth-401-opds"));
    let opds = runtime
        .block_on(
            crate::routes::build_router(state)
                .oneshot(Request::get("/opds").body(Body::empty()).unwrap()),
        )
        .expect("answers");
    assert_eq!(opds.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        opds.headers()
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok()),
        Some("Basic realm=\"taiyaku\""),
    );
}

/// A password and a `tyk_` key both authenticate through Basic, and the key
/// path is asserted **not** to reach argon2 — §2's whole point is unobservable
/// otherwise.
#[test]
fn basic_accepts_a_password_and_an_api_key() {
    // ... constructed against test_support::state_with_account, added in Step 7
}
```

- [ ] **Step 6: Confirm they fail**

```bash
cargo test -p taiyaku-serve auth:: -- --nocapture
```

Expected: FAIL to compile — no `auth` module, no `guarded_routes`.

- [ ] **Step 7: Write `auth.rs`, and rebuild `build_router` around a route list**

The guard is a `from_fn` layer, the same mechanism the request log already uses. The
route *list* is what makes the enumeration test honest: `build_router` iterates it, so a
route that is not in it does not exist.

In `crates/taiyaku-serve/src/routes.rs`, replace the body of `build_router`'s route chain:

```rust
/// Every path this server answers, and the handler for each.
///
/// **The router's own source of truth, and the reason
/// `auth::tests::every_route_refuses_an_anonymous_request` is a test rather
/// than a checklist.** `build_router` folds this list; a route that is not here
/// does not exist, and the enumeration test walks the same list. A route added
/// here therefore arrives guarded and covered on the commit that adds it.
///
/// Returning a `Vec` rather than a const array because `MethodRouter` is not
/// const-constructible; the cost is one allocation per `build_router` call, of
/// which there is one per process and one per test.
pub(crate) fn guarded_routes() -> Vec<(&'static str, MethodRouter<Arc<ServeState>>)> {
    vec![
        ("/api/works", get(list_works)),
        ("/api/works/{work}", get(get_sidecar)),
        ("/api/works/{work}/{edition}/pages/{index}", get(get_page)),
        ("/api/works/{work}/{edition}/pages/{index}/thumb", get(get_thumb)),
        ("/api/works/{work}/progress", get(get_progress).put(put_progress)),
        ("/api/works/{work}/notes", get(get_notes).put(put_notes)),
        ("/api/works/{work}/truth", get(get_truth).put(put_truth)),
        ("/api/works/{work}/alignments", axum::routing::patch(patch_alignments)),
    ]
}

pub fn build_router(state: Arc<ServeState>) -> Router {
    let log = state.log.clone();
    let guarded = guarded_routes()
        .into_iter()
        .fold(Router::new(), |router, (path, handler)| router.route(path, handler))
        .merge(crate::opds::routes());

    guarded
        // **The guard sits above every route and below the request log.** Above,
        // so a route cannot be added outside it: `Router::layer` applies only to
        // routes registered before the call, and `guarded_routes()` is the only
        // place routes are registered. Below the log, so an anonymous 401 is
        // still logged — a request that never reaches a handler is exactly the
        // one worth having in the file.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::guard,
        ))
        .layer(axum::middleware::from_fn(/* the existing request-log closure, unchanged */))
        .with_state(state)
}
```

Then `crates/taiyaku-serve/src/auth.rs`:

```rust
//! The guard: one middleware, one `Principal`, two shapes of refusal.

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::ServeState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

/// Who this request is, resolved once and inserted as a request extension.
#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: i64,
    pub role: Role,
    pub can_download: bool,
    pub can_stream: bool,
    /// The roots this account was granted. **Read by no filter yet** — spec A2
    /// §8 as amended 2026-08-18; spec C1's browse API is the first caller that
    /// can act on it.
    pub roots: Vec<i64>,
}

impl Principal {
    fn from_account(account: &taiyaku_library::account::Account, roots: Vec<i64>) -> Self {
        Self {
            user_id: account.id,
            role: if account.role == "admin" { Role::Admin } else { Role::User },
            can_download: account.can_download,
            can_stream: account.can_stream,
            roots,
        }
    }
}

/// The 401 an OPDS client needs and the 401 the reader needs, which are not the
/// same response.
///
/// §4: `WWW-Authenticate` is what makes Panels or Chunky prompt for
/// credentials, and it is also what makes a browser raise its own native auth
/// dialog — unstyleable, un-pre-fillable, and with no way back to the app. So
/// `/opds` gets the header and `/api` gets plain JSON, and the reader routes
/// itself to its own login screen.
fn refuse(path: &str) -> Response {
    if path.starts_with("/opds") {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"taiyaku\"")],
            "unauthorized",
        )
            .into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"unauthorized"}"#,
        )
            .into_response()
    }
}

/// Resolve every request to a `Principal`, or refuse it.
pub(crate) async fn guard(
    State(state): State<std::sync::Arc<ServeState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let Some(principal) = resolve(&state, &request) else {
        return refuse(&path);
    };
    request.extensions_mut().insert(principal);
    next.run(request).await
}

/// The session cookie first, then `Authorization: Basic`.
///
/// Cookie first because it is the reader's path and the common one; a browser
/// that sends both is answered by its session rather than by whatever a
/// password manager filled in.
fn resolve(state: &ServeState, request: &Request) -> Option<Principal> {
    if let Some(principal) = crate::session::from_cookie(state, request) {
        return Some(principal);
    }
    from_basic(state, request)
}

/// `Authorization: Basic`, routed by [`credential::looks_like_api_key`].
///
/// **The key path never reaches argon2**, which is §2's whole point: one string
/// comparison decides, rather than an argon2 verify that falls back.
fn from_basic(state: &ServeState, request: &Request) -> Option<Principal> {
    use base64::Engine as _;
    use taiyaku_library::{account, credential};

    let header = request.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let pair = String::from_utf8(decoded).ok()?;
    let (name, secret) = pair.split_once(':')?;

    let conn = state.library.conn();
    if credential::looks_like_api_key(secret) {
        let found = account::by_api_key(&conn, secret).ok()??;
        let roots = account::roots_for(&conn, found.id).ok()?;
        return Some(Principal::from_account(&found, roots));
    }
    let (found, phc) = account::by_name(&conn, name).ok()??;
    if !credential::verify_password(secret, &phc) {
        return None;
    }
    let roots = account::roots_for(&conn, found.id).ok()?;
    Some(Principal::from_account(&found, roots))
}

#[cfg(test)]
mod tests;
```

Add `base64` to `crates/taiyaku-serve/Cargo.toml`, and `mod auth;` plus `mod session;` to
`crates/taiyaku-serve/src/lib.rs`.

- [ ] **Step 8: Run the guard tests**

```bash
cargo test -p taiyaku-serve auth:: -- --nocapture
```

Expected: PASS for the enumeration and the two 401 shapes. `basic_accepts_a_password_and_an_api_key`
is completed in Task 4 once `test_support` can build an authenticated fixture; leave it
`#[ignore]`d with a comment naming Task 4 rather than half-written.

- [ ] **Step 9: Expect the rest of the suite to fail, and see how**

```bash
cargo test -p taiyaku-serve 2>&1 | tail -30
```

Expected: every pre-existing route test now fails with 401. That is the correct state —
Task 4 gives `test_support` an authenticated fixture and they go green there. Do **not**
weaken the guard to make them pass.

- [ ] **Step 10: Commit**

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs crates/taiyaku-library/src/lib.rs crates/taiyaku-serve/src/auth.rs crates/taiyaku-serve/src/auth/tests.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/Cargo.toml Cargo.lock
git commit -m "Every route resolves to a principal, and the enumeration that proves it comes from the router rather than from a checklist"
```

---

### Task 4: Cookie sessions, the origin check, and the lifecycle

**Files:**
- Create: `crates/taiyaku-serve/src/session.rs`, `crates/taiyaku-serve/src/session/tests.rs`
- Modify: `crates/taiyaku-library/src/account.rs` (session rows), `crates/taiyaku-serve/src/routes.rs` (login/logout), `crates/taiyaku-serve/src/test_support.rs`, `crates/taiyaku-serve/src/lib.rs` (`ServeState.auth`)

**Interfaces:**
- Consumes: `credential::{mint_token, token_sha256}`, `account::{by_name, roots_for}`, `auth::Principal`.
- Produces:
  ```rust
  // taiyaku-library::account
  pub fn open_session(conn: &Connection, user_id: i64, token: &str,
                      user_agent: Option<&str>, ip: Option<&str>) -> Result<(), String>;
  pub fn session_owner(conn: &Connection, token: &str) -> Result<Option<Account>, String>;
  pub fn close_session(conn: &Connection, token: &str) -> Result<(), String>;
  pub fn close_other_sessions(conn: &Connection, user_id: i64, keep: &str) -> Result<usize, String>;
  pub fn sweep_expired_sessions(conn: &Connection) -> Result<usize, String>;

  // taiyaku-serve
  pub struct AuthConfig { pub trusted_proxies: Vec<IpNet>, pub secure_cookies: bool,
                          pub allow_web_setup: bool }
  pub(crate) fn from_cookie(state: &ServeState, request: &Request) -> Option<Principal>;
  pub(crate) const COOKIE_NAME: &str = "taiyaku_session";
  pub(crate) const SESSION_DAYS: i64 = 30;
  ```
  Task 5 needs `test_support::authenticated_state`; Task 7 fills `AuthConfig.trusted_proxies`;
  Task 9 reads `AuthConfig.allow_web_setup`.

- [ ] **Step 1: Write the failing tests**

`crates/taiyaku-serve/src/session/tests.rs`:

```rust
use super::*;

/// §10: "The session token in the database is not the token in the cookie —
/// asserted by inequality, so storing it raw fails rather than passing by
/// inspection."
#[test]
fn the_stored_session_is_not_the_cookie_value() {
    let lib = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let token = taiyaku_library::credential::mint_token();
    taiyaku_library::account::open_session(&conn, 1, &token, None, None).expect("opens");

    let stored: String = conn
        .query_row("SELECT token_sha256 FROM session", [], |r| r.get(0))
        .expect("reads");
    assert_ne!(stored, token, "the raw token reached the database");
    assert_eq!(stored, taiyaku_library::credential::token_sha256(&token));
}

/// §4's cookie flags. `Secure` is the one that varies, and it varies on
/// configuration rather than on environment.
#[test]
fn cookie_flags_follow_the_configuration() {
    let plain = set_cookie(&"tok".to_string(), false);
    assert!(plain.contains("taiyaku_session=tok"));
    assert!(plain.contains("HttpOnly"));
    assert!(plain.contains("SameSite=Lax"));
    assert!(plain.contains("Path=/"));
    assert!(plain.contains("Max-Age=2592000"), "30 days in seconds");
    assert!(!plain.contains("Secure"), "Secure without a trusted proxy");

    let secured = set_cookie(&"tok".to_string(), true);
    assert!(secured.contains("Secure"));
}

/// §4: changing a password ends every other session, so a compromised one
/// cannot outlive the change that was meant to end it — **and keeps the
/// current one**, so changing your own password does not log you out.
#[test]
fn a_password_change_ends_every_session_but_this_one() {
    let lib = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let mine = taiyaku_library::credential::mint_token();
    let theirs = taiyaku_library::credential::mint_token();
    taiyaku_library::account::open_session(&conn, 1, &mine, None, None).expect("opens");
    taiyaku_library::account::open_session(&conn, 1, &theirs, None, None).expect("opens");

    let closed = taiyaku_library::account::close_other_sessions(&conn, 1, &mine).expect("closes");
    assert_eq!(closed, 1);
    assert!(taiyaku_library::account::session_owner(&conn, &mine).expect("reads").is_some());
    assert!(taiyaku_library::account::session_owner(&conn, &theirs).expect("reads").is_none());
}

/// An expired row resolves to nobody and is deleted on the lookup that found
/// it, per §4's "deleted lazily on lookup and swept at startup".
#[test]
fn an_expired_session_resolves_to_nobody_and_is_removed() {
    let lib = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let token = taiyaku_library::credential::mint_token();
    conn.execute(
        "INSERT INTO session (token_sha256, user_id, expires)
         VALUES (?1, 1, datetime('now', '-1 hour'))",
        [taiyaku_library::credential::token_sha256(&token)],
    )
    .expect("inserts");

    assert!(taiyaku_library::account::session_owner(&conn, &token).expect("reads").is_none());
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
        .expect("counts");
    assert_eq!(left, 0, "the expired row was not swept on lookup");
}

/// §4's CSRF rule: a state-changing method needs an `Origin` matching the
/// server's own. **An absent header is refused rather than assumed
/// same-origin**, because absent is precisely what a forged cross-site form
/// produces.
#[test]
fn state_changing_methods_need_a_matching_origin() {
    assert!(origin_is_acceptable(&axum::http::Method::GET, None, "example:8484"));
    assert!(!origin_is_acceptable(&axum::http::Method::PUT, None, "example:8484"));
    assert!(!origin_is_acceptable(
        &axum::http::Method::PUT,
        Some("http://evil.test"),
        "example:8484",
    ));
    assert!(origin_is_acceptable(
        &axum::http::Method::PUT,
        Some("http://example:8484"),
        "example:8484",
    ));
    for method in [
        axum::http::Method::POST,
        axum::http::Method::PATCH,
        axum::http::Method::DELETE,
    ] {
        assert!(!origin_is_acceptable(&method, None, "example:8484"), "{method} allowed");
    }
}
```

- [ ] **Step 2: Run and confirm they fail**

```bash
cargo test -p taiyaku-serve session:: -- --nocapture
```

Expected: FAIL to compile.

- [ ] **Step 3: Write the session row operations in `account.rs`**

```rust
/// How long a session lives without being used. §4: thirty days, slid forward
/// on use — one login per browser per month, which is what makes §9's refusal
/// of a `--no-auth` bypass cost nothing worth having.
const SESSION_DAYS: i64 = 30;

/// Record a new session. The token is hashed here and nowhere else, so no
/// caller can pass the stored form by mistake — this function takes the raw
/// token and is the only thing that writes the column.
///
/// # Errors
///
/// Returns an error if the insert fails.
pub fn open_session(
    conn: &Connection,
    user_id: i64,
    token: &str,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO session (token_sha256, user_id, expires, user_agent, ip)
         VALUES (?1, ?2, datetime('now', ?3), ?4, ?5)",
        params![
            credential::token_sha256(token),
            user_id,
            format!("+{SESSION_DAYS} days"),
            user_agent,
            ip
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("opening a session: {e}"))
}

/// Who this session belongs to, sliding its expiry forward — or `None` for a
/// token that is unknown or expired, **deleting the expired row as it goes**.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn session_owner(conn: &Connection, token: &str) -> Result<Option<Account>, String> {
    let hashed = credential::token_sha256(token);
    // Lazily, per §4: the sweep at startup is the other half, and neither is
    // enough alone — a server that runs for a month never sweeps, and a row
    // nobody presents is never looked up.
    conn.execute(
        "DELETE FROM session WHERE token_sha256 = ?1 AND expires <= datetime('now')",
        params![hashed],
    )
    .map_err(|e| format!("sweeping an expired session: {e}"))?;

    let found = conn
        .query_row(
            "SELECT u.id, u.name, u.role, u.can_download, u.can_stream
             FROM session s JOIN user u ON u.id = s.user_id
             WHERE s.token_sha256 = ?1",
            params![hashed],
            account_from_row,
        )
        .optional()
        .map_err(|e| format!("resolving a session: {e}"))?;

    if found.is_some() {
        let _ = conn.execute(
            "UPDATE session
                SET last_seen = datetime('now'),
                    expires   = datetime('now', ?2)
              WHERE token_sha256 = ?1",
            params![hashed, format!("+{SESSION_DAYS} days")],
        );
    }
    Ok(found)
}

/// End one session. Logout.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn close_session(conn: &Connection, token: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM session WHERE token_sha256 = ?1",
        params![credential::token_sha256(token)],
    )
    .map(|_| ())
    .map_err(|e| format!("closing a session: {e}"))
}

/// End every session of this user's except the one named, and answer how many.
///
/// §4: a password change must not outlive itself in a session that was already
/// compromised, and must not log out the browser that made the change.
/// **API keys are deliberately untouched** — they are independent credentials
/// with their own revocation, and bundling them would make rotating a password
/// silently break every reader in the house.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn close_other_sessions(conn: &Connection, user_id: i64, keep: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM session WHERE user_id = ?1 AND token_sha256 <> ?2",
        params![user_id, credential::token_sha256(keep)],
    )
    .map_err(|e| format!("ending other sessions: {e}"))
}

/// Delete every expired session. Called once at `serve` startup.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn sweep_expired_sessions(conn: &Connection) -> Result<usize, String> {
    conn.execute("DELETE FROM session WHERE expires <= datetime('now')", [])
        .map_err(|e| format!("sweeping expired sessions: {e}"))
}
```

- [ ] **Step 4: Write `session.rs`**

```rust
//! The browser's half of the guard: a cookie, an origin check, and the two
//! routes that start and end a session.

use axum::extract::Request;
use axum::http::{Method, header};

use crate::ServeState;
use crate::auth::Principal;

pub(crate) const COOKIE_NAME: &str = "taiyaku_session";
pub(crate) const SESSION_DAYS: i64 = 30;
const MAX_AGE_SECS: i64 = SESSION_DAYS * 24 * 60 * 60;

/// The `Set-Cookie` value for a new session.
///
/// **`SameSite=Lax` rather than `Strict`.** `Strict` breaks following a link
/// into the app from anywhere else — a bookmark, a chat message, an OPDS
/// client's own browser hand-off — and the gap it closes is covered by
/// [`origin_is_acceptable`] below. `Secure` follows configuration rather than
/// environment: a trusted proxy implies TLS terminated in front, and
/// `--insecure-cookies` is the deliberate opt-out for a plain-HTTP private
/// network.
pub(crate) fn set_cookie(token: &str, secure: bool) -> String {
    let base = format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={MAX_AGE_SECS}"
    );
    if secure { format!("{base}; Secure") } else { base }
}

/// The `Set-Cookie` that ends one.
pub(crate) fn clear_cookie(secure: bool) -> String {
    let base = format!("{COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0");
    if secure { format!("{base}; Secure") } else { base }
}

/// Whether a request may change state, given its method and `Origin`.
///
/// **Header auth is not CSRF-able**, so this applies to the cookie path only —
/// an `Authorization: Basic` request is not something a forged form can send.
/// On the cookie path every `PUT`, `PATCH`, `POST` and `DELETE` needs an
/// `Origin` matching the server's own. Browsers send it on all four; a request
/// without one is **refused rather than assumed same-origin**, because absent
/// is precisely what a forged cross-site form produces.
pub(crate) fn origin_is_acceptable(method: &Method, origin: Option<&str>, host: &str) -> bool {
    if !matches!(*method, Method::PUT | Method::PATCH | Method::POST | Method::DELETE) {
        return true;
    }
    let Some(origin) = origin else { return false };
    origin
        .rsplit_once("://")
        .is_some_and(|(_, authority)| authority == host)
}

/// Resolve a `Principal` from the session cookie, if there is one.
pub(crate) fn from_cookie(state: &ServeState, request: &Request) -> Option<Principal> {
    let token = request
        .headers()
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(name, value)| (name == COOKIE_NAME).then_some(value))?;

    let host = request.headers().get(header::HOST)?.to_str().ok()?;
    let origin = request.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if !origin_is_acceptable(request.method(), origin, host) {
        return None;
    }

    let conn = state.library.conn();
    let found = taiyaku_library::account::session_owner(&conn, token).ok()??;
    let roots = taiyaku_library::account::roots_for(&conn, found.id).ok()?;
    Some(Principal::from_account(&found, roots))
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 5: Add the login and logout routes**

Add to `guarded_routes()`'s sibling — these two are the only routes **outside** the
guard, since a request that has to log in cannot already be authenticated:

```rust
/// The two routes that are not behind the guard, because a request that is
/// about to log in cannot already have. Kept in one function beside
/// `guarded_routes()` so the split is visible in one screen rather than
/// scattered, and so `auth::tests::every_route_refuses_an_anonymous_request`
/// has an explicit list of what it is *not* asserting over.
pub(crate) fn open_routes() -> Vec<(&'static str, MethodRouter<Arc<ServeState>>)> {
    vec![
        ("/api/session", axum::routing::post(login).delete(logout)),
        ("/api/me", get(whoami)),
    ]
}
```

`/api/me` is behind the guard rather than open — move it into `guarded_routes()`; the
reader calls it on load to find out whether it has a session, and a 401 there is the
signal to show the login screen. `login` reads `{"name","password"}`, verifies with
`credential::verify_password`, mints a token, calls `account::open_session`, and answers
`200` with `Set-Cookie`. `logout` reads the cookie, calls `account::close_session`, and
answers `204` with `clear_cookie`.

- [ ] **Step 6: Give `test_support` an authenticated fixture**

```rust
/// The shared fixture, plus an account with a known password and an open
/// session, and the cookie header that presents it.
///
/// **Every route test needs this now**, which is the point: there is no
/// `--no-auth` bypass (§9), so a fixture is how a test authenticates rather
/// than a flag. Returns the state and the `Cookie` header value.
pub(crate) fn authenticated_state(dir: &str) -> (ServeState, String) {
    let state = state(dir);
    let token = taiyaku_library::credential::mint_token();
    {
        let conn = state.library.conn();
        let phc = taiyaku_library::credential::hash_password("fixture").expect("hashes");
        conn.execute("UPDATE user SET password_hash = ?1 WHERE id = 1", [&phc])
            .expect("sets a password");
        taiyaku_library::account::open_session(&conn, 1, &token, None, None).expect("opens");
    }
    let cookie = format!("{}={token}", crate::session::COOKIE_NAME);
    (state, cookie)
}
```

- [ ] **Step 7: Update every existing route test to present the cookie**

Mechanical: each `Request::get(path)` becomes

```rust
Request::get(path)
    .header("cookie", &cookie)
    .header("host", "test")
    .body(Body::empty())
    .unwrap()
```

and each state-changing one adds `.header("origin", "http://test")`. Files:
`crates/taiyaku-serve/src/routes/tests.rs`, and any test module under
`crates/taiyaku-serve/src/` that drives `build_router`.

- [ ] **Step 8: Run the whole serve suite**

```bash
cargo test -p taiyaku-serve 2>&1 | tail -20
```

Expected: PASS. Then un-`#[ignore]` `basic_accepts_a_password_and_an_api_key` from Task 3
and complete it against `authenticated_state`.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-serve/src/session.rs crates/taiyaku-serve/src/session/tests.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/routes/tests.rs crates/taiyaku-serve/src/test_support.rs crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs
git commit -m "A session is a cookie the database never holds the value of, and a state change without an Origin is refused rather than assumed same-origin"
```

---

### Task 5: The routes stop hardcoding user 1

**Files:**
- Modify: `crates/taiyaku-serve/src/works.rs` (delete `LOCAL_USER`), `crates/taiyaku-serve/src/progress.rs`, `crates/taiyaku-serve/src/notes.rs`, `crates/taiyaku-serve/src/routes.rs`

**Interfaces:**
- Consumes: `auth::Principal` from the request extension inserted in Task 3.
- Produces: nothing new. This task deletes a constant and threads an argument.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/routes/tests.rs`:

```rust
/// Two accounts, two positions, one work. Before A2 both reads answered user
/// 1's row, which is what `LOCAL_USER` meant; after it, each reads its own.
#[test]
fn two_readers_hold_separate_positions_in_one_work() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let (state, alice) = crate::test_support::authenticated_state("two-readers");
    let bob = {
        let conn = state.library.conn();
        let phc = taiyaku_library::credential::hash_password("bob").expect("hashes");
        conn.execute(
            "INSERT INTO user (name, password_hash, role) VALUES ('bob', ?1, 'user')",
            [&phc],
        )
        .expect("adds bob");
        let bob_id = conn.last_insert_rowid();
        let token = taiyaku_library::credential::mint_token();
        taiyaku_library::account::open_session(&conn, bob_id, &token, None, None).expect("opens");
        format!("{}={token}", crate::session::COOKIE_NAME)
    };
    let state = std::sync::Arc::new(state);

    let put = |cookie: &str, slot: i64| {
        let request = Request::put("/api/works/t/progress")
            .header("cookie", cookie)
            .header("host", "test")
            .header("origin", "http://test")
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"edition":"raw","slot":{slot}}}"#)))
            .unwrap();
        runtime
            .block_on(crate::routes::build_router(state.clone()).oneshot(request))
            .expect("answers")
    };
    assert_eq!(put(&alice, 3).status(), StatusCode::NO_CONTENT);
    assert_eq!(put(&bob, 11).status(), StatusCode::NO_CONTENT);

    let get = |cookie: &str| {
        let request = Request::get("/api/works/t/progress")
            .header("cookie", cookie)
            .header("host", "test")
            .body(Body::empty())
            .unwrap();
        let response = runtime
            .block_on(crate::routes::build_router(state.clone()).oneshot(request))
            .expect("answers");
        runtime.block_on(axum::body::to_bytes(response.into_body(), usize::MAX)).unwrap()
    };
    let alices = String::from_utf8(get(&alice).to_vec()).unwrap();
    let bobs = String::from_utf8(get(&bob).to_vec()).unwrap();
    assert!(alices.contains("\"slot\":3"), "alice read {alices}");
    assert!(bobs.contains("\"slot\":11"), "bob read {bobs}");
}
```

- [ ] **Step 2: Run and confirm it fails**

```bash
cargo test -p taiyaku-serve two_readers_hold_separate_positions -- --nocapture
```

Expected: FAIL — bob's read answers `"slot":3`, because both went to `LOCAL_USER`.

- [ ] **Step 3: Delete `LOCAL_USER` from `works.rs` and thread the principal**

Delete the constant and its doc comment at `crates/taiyaku-serve/src/works.rs:288`
entirely. In `progress.rs` and `notes.rs`, each handler gains an
`Extension(principal): Extension<Principal>` extractor and passes `principal.user_id`
where `LOCAL_USER` stood. `truth.rs` is **not** touched: `state::truth` and
`state::put_truth` take no user id, because ground-truth labels are library-wide rather
than per reader.

- [ ] **Step 4: Run**

```bash
cargo test -p taiyaku-serve 2>&1 | tail -10
grep -rn "LOCAL_USER" crates/taiyaku-serve/src/ || echo "gone from serve"
```

Expected: PASS, and no `LOCAL_USER` anywhere under `crates/taiyaku-serve/src/`.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/
git commit -m "Two readers hold two positions in one work, which the constant that answered user 1 for both could not express"
```

---

### Task 6: Export writes one reader's documents and names the rest

**Files:**
- Modify: `crates/taiyaku-library/src/export.rs`, `crates/taiyaku-library/src/migrate.rs`, `crates/taiyaku-cli/src/library.rs`
- Test: `crates/taiyaku-library/src/export/tests.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub fn work(conn: &Connection, work_id: i64, user_id: i64, dir: Option<&Path>) -> Result<Exported, String>`,
  and `Exported` gains `pub other_users: Vec<String>`.

**The decision this implements** (design §5.1, decided 2026-08-18): the database holds
every user's notes — `note` and `progress` are keyed `(user_id, work_id)` — and the
*file* holds one set, which is what a sidecar is for. So an export writes the requesting
user's and **names every other user holding state for that work**, following the rule
`suppressed`, `rekeyed`, `merged` and `kept_editions` were all built on: any outcome that
changes what a person sees and cannot be inferred from the other counts gets a field.

- [ ] **Step 1: Write the failing test**

```rust
/// §5.1, decided 2026-08-18. One work, two readers with notes, one export.
/// The file carries one set — that is what a sidecar is — and the report names
/// the reader whose notes are not in it, so nothing is dropped in silence.
#[test]
fn an_export_names_the_readers_it_did_not_write() {
    let lib = crate::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let work_id = fixture_work(&conn);
    let bob = add_user(&conn, "bob");
    put_a_note(&conn, work_id, 1, "alice's");
    put_a_note(&conn, work_id, bob, "bob's");

    let dir = tempdir();
    let exported = work(&conn, work_id, 1, Some(dir.path())).expect("exports");

    assert!(exported.written.iter().any(|p| p.ends_with("taiyaku.notes.json")));
    assert_eq!(exported.other_users, vec!["bob".to_string()]);

    let written = std::fs::read_to_string(
        dir.path().join("s").join("taiyaku.notes.json")
    ).expect("reads");
    assert!(written.contains("alice's"));
    assert!(!written.contains("bob's"), "the file holds one reader's notes");
}

/// The single-reader case is unchanged and reports nothing, so an ordinary
/// export does not grow a line telling a person about a situation they are not
/// in.
#[test]
fn a_single_reader_export_names_nobody() {
    let lib = crate::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let work_id = fixture_work(&conn);
    put_a_note(&conn, work_id, 1, "only mine");

    let dir = tempdir();
    let exported = work(&conn, work_id, 1, Some(dir.path())).expect("exports");
    assert!(exported.other_users.is_empty());
}
```

- [ ] **Step 2: Confirm it fails**

```bash
cargo test -p taiyaku-library export::tests::an_export_names -- --nocapture
```

Expected: FAIL to compile — `work` takes three arguments.

- [ ] **Step 3: Change the signature, delete the constant, add the field**

In `export.rs`: delete `pub(crate) const LOCAL_USER: i64 = 1;` and its doc comment
(lines 28–44). `work` takes `user_id: i64` after `work_id`. `Exported` gains:

```rust
    /// The other readers holding notes or a reading position for this work,
    /// whose state is **not** in the documents just written.
    ///
    /// A sidecar carries one reader's notes — that is what the format is, and
    /// versioning it to hold several was declined (§5.1, decided 2026-08-18).
    /// So the ambiguity is reported rather than resolved: this list is the
    /// difference between "exported" and "exported everything", which no other
    /// field here can express. Empty for the ordinary single-reader library,
    /// so nobody is told about a situation they are not in.
    pub other_users: Vec<String>,
```

populated by:

```sql
SELECT DISTINCT u.name
  FROM user u
 WHERE u.id <> ?2
   AND (EXISTS (SELECT 1 FROM note     n WHERE n.work_id = ?1 AND n.user_id = u.id)
     OR EXISTS (SELECT 1 FROM progress p WHERE p.work_id = ?1 AND p.user_id = u.id))
 ORDER BY u.name
```

- [ ] **Step 4: Fix `migrate.rs`'s four call sites**

`migrate.rs` reads sibling documents back on a work's first import, and used
`export::LOCAL_USER` for the same reason `export` did. It takes an explicit `user_id`
parameter threaded from its caller. The caller passes the id of the account running the
import — which on a fresh deployment is still 1, since §5 requires promotion to reach
that row rather than insert beside it, so existing behaviour is preserved rather than
changed by accident.

- [ ] **Step 5: Print the report in the CLI**

In `crates/taiyaku-cli/src/library.rs`'s `export`, after the written/removed/kept lines:

```rust
if !exported.other_users.is_empty() {
    eprintln!(
        "  {} other reader(s) hold notes or a position for this work and are not in \
         the file: {}",
        exported.other_users.len(),
        exported.other_users.join(", ")
    );
}
```

- [ ] **Step 6: Run**

```bash
cargo test -p taiyaku-library export:: -- --nocapture
cargo test --workspace 2>&1 | tail -5
grep -rn "LOCAL_USER" crates/ || echo "gone from the workspace"
```

Expected: PASS, and `LOCAL_USER` gone from every crate.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/export.rs crates/taiyaku-library/src/export/tests.rs crates/taiyaku-library/src/migrate.rs crates/taiyaku-cli/src/library.rs
git commit -m "An export writes one reader's documents and names the readers it did not, which is the ambiguity a second account creates"
```

---

### Task 7: `--trusted-proxy`, and rate limiting on the pair

**Files:**
- Modify: `crates/taiyaku-serve/src/auth.rs` (CIDR matching, peer resolution), `crates/taiyaku-library/src/account.rs` (`login_attempt`), `crates/taiyaku-cli/src/main.rs` (the three flags)
- Test: `crates/taiyaku-serve/src/auth/tests.rs`, `crates/taiyaku-library/src/account/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  // taiyaku-serve::auth
  pub struct Cidr { base: IpAddr, prefix: u8 }
  impl std::str::FromStr for Cidr { type Err = String; }
  impl Cidr { pub fn contains(&self, addr: IpAddr) -> bool; }
  pub(crate) fn client_address(config: &AuthConfig, peer: IpAddr, forwarded: Option<&str>) -> IpAddr;

  // taiyaku-library::account
  pub fn note_failure(conn: &Connection, key: &str) -> Result<u32, String>;
  pub fn is_locked_out(conn: &Connection, key: &str) -> Result<bool, String>;
  pub fn clear_failures(conn: &Connection, key: &str) -> Result<(), String>;
  ```

**No new dependency.** CIDR matching is thirty lines over `std::net::IpAddr`; pulling in
`ipnet` for it would make this branch's dependency count two rather than one, against a
design that names `argon2` as the only crate it needs.

- [ ] **Step 1: Write the failing tests**

```rust
/// §6's table, first row: trusting `X-Forwarded-For` from whatever connected
/// means trusting the internet on a misconfigured deployment. So the header is
/// honoured only when the **direct peer** falls inside a configured range.
#[test]
fn forwarded_for_is_honoured_only_from_inside_a_configured_range() {
    let config = AuthConfig {
        trusted_proxies: vec!["10.0.0.0/8".parse().expect("cidr")],
        secure_cookies: true,
        allow_web_setup: false,
    };
    let inside: IpAddr = "10.1.2.3".parse().unwrap();
    let outside: IpAddr = "203.0.113.9".parse().unwrap();
    let claimed = Some("198.51.100.7");

    assert_eq!(
        client_address(&config, inside, claimed),
        "198.51.100.7".parse::<IpAddr>().unwrap(),
        "a trusted proxy's X-Forwarded-For was ignored",
    );
    assert_eq!(
        client_address(&config, outside, claimed),
        outside,
        "an untrusted peer spoofed its address",
    );
    // With nothing configured, nothing is trusted.
    let bare = AuthConfig { trusted_proxies: vec![], secure_cookies: false, allow_web_setup: false };
    assert_eq!(client_address(&bare, inside, claimed), inside);
}

/// The last hop is what a proxy appends, and the earlier entries are whatever
/// the client sent. Taking the first would let a client prepend its own.
#[test]
fn a_forwarded_chain_is_read_from_the_right_end() {
    let config = AuthConfig {
        trusted_proxies: vec!["10.0.0.0/8".parse().expect("cidr")],
        secure_cookies: true,
        allow_web_setup: false,
    };
    let peer: IpAddr = "10.0.0.1".parse().unwrap();
    assert_eq!(
        client_address(&config, peer, Some("1.2.3.4, 198.51.100.7")),
        "198.51.100.7".parse::<IpAddr>().unwrap(),
    );
}

#[test]
fn cidr_parses_and_matches_both_families() {
    let v4: Cidr = "192.168.1.0/24".parse().expect("v4");
    assert!(v4.contains("192.168.1.255".parse().unwrap()));
    assert!(!v4.contains("192.168.2.0".parse().unwrap()));
    let v6: Cidr = "fd00::/8".parse().expect("v6");
    assert!(v6.contains("fd12::1".parse().unwrap()));
    assert!(!v6.contains("fe80::1".parse().unwrap()));
    // A bare address is a /32 or /128, which is what a person means by
    // `--trusted-proxy 10.0.0.1`.
    let one: Cidr = "10.0.0.1".parse().expect("bare");
    assert!(one.contains("10.0.0.1".parse().unwrap()));
    assert!(!one.contains("10.0.0.2".parse().unwrap()));
    assert!("nonsense".parse::<Cidr>().is_err());
    assert!("10.0.0.0/33".parse::<Cidr>().is_err());
}

/// §7: keyed on the **pair**, so one misconfigured OPDS client retrying hard
/// locks out only itself rather than its owner's every device.
#[test]
fn lockout_is_keyed_on_the_pair_not_the_account() {
    let lib = crate::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    for _ in 0..10 {
        note_failure(&conn, "203.0.113.9|alice").expect("notes");
    }
    assert!(is_locked_out(&conn, "203.0.113.9|alice").expect("reads"));
    assert!(
        !is_locked_out(&conn, "198.51.100.1|alice").expect("reads"),
        "alice's other device was locked out by the first one's failures",
    );
    assert!(!is_locked_out(&conn, "203.0.113.9|bob").expect("reads"));

    clear_failures(&conn, "203.0.113.9|alice").expect("clears");
    assert!(!is_locked_out(&conn, "203.0.113.9|alice").expect("reads"));
}
```

- [ ] **Step 2: Confirm they fail, then implement**

```bash
cargo test -p taiyaku-serve auth::tests::cidr -- --nocapture
```

Implement `Cidr` over `IpAddr::to_canonical()`'s octets, comparing the first `prefix`
bits. `client_address` returns the peer unless the peer is inside a configured range, in
which case it returns the **last** comma-separated entry of `X-Forwarded-For`.
`note_failure` upserts `login_attempt` with a fixed window; `is_locked_out` answers true
above a threshold within the window. Wire the rate limit into `auth::from_basic`'s
password branch and into `session::login` — **not** into the API-key branch, which is a
high-entropy secret with nothing to guess.

- [ ] **Step 3: Add the flags**

`crates/taiyaku-cli/src/main.rs`: `--trusted-proxy CIDR` (repeatable, so read with a
values-collecting helper rather than `option_value`), `--insecure-cookies`,
`--allow-web-setup`. Add all three to `reject_unknown_flags`'s list for `serve` and to
the synopsis at `main.rs:185`. Configuring any trusted proxy implies `secure_cookies`;
`--insecure-cookies` overrides it.

- [ ] **Step 4: Run and commit**

```bash
cargo test --workspace 2>&1 | tail -5
git add crates/taiyaku-serve/src/auth.rs crates/taiyaku-serve/src/auth/tests.rs crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs crates/taiyaku-cli/src/main.rs
git commit -m "A forwarded address is trusted only from a configured peer, and a lockout keyed on the pair costs one device rather than every device"
```

---

### Task 8: The CLI — `library user` and `library key`

**Files:**
- Create: `crates/taiyaku-cli/src/user.rs`, `crates/taiyaku-cli/src/user/tests.rs`
- Modify: `crates/taiyaku-cli/src/library.rs` (dispatch), `crates/taiyaku-cli/src/main.rs` (usage), `crates/taiyaku-library/src/account.rs` (writes)

**Commands** (§5, §9 — the CLI covers user administration until spec C4's UI):

```
taiyaku library user promote [<name>] --admin   [--db PATH]
taiyaku library user add <name> [--role user|admin] [--db PATH]
taiyaku library user password <name>            [--db PATH]
taiyaku library user list                       [--db PATH]
taiyaku library key create <user> --name <device> [--db PATH]
taiyaku library key list <user>                 [--db PATH]
taiyaku library key revoke <id>                 [--db PATH]
```

- [ ] **Step 1: Write the failing test — the one §5 turns on**

```rust
/// §5: "Whichever path runs, it must **promote that row** — same `user.id` —
/// and not insert a new one. Inserting would orphan everything recorded
/// between A and A2 on an account nobody can log in as, and the loss would be
/// silent, because the reader would simply show an empty library and a fresh
/// position."
#[test]
fn promotion_reaches_the_seeded_row_and_keeps_what_hangs_off_it() {
    let lib = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let work_id = fixture_work(&conn);
    put_a_note(&conn, work_id, 1, "written before there was a login");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 42)",
        [work_id],
    )
    .expect("position");

    taiyaku_library::account::promote(&conn, "davey", "a password", "admin").expect("promotes");

    let users: i64 = conn.query_row("SELECT COUNT(*) FROM user", [], |r| r.get(0)).expect("counts");
    assert_eq!(users, 1, "promotion inserted a second row instead of promoting");

    let (id, name): (i64, String) = conn
        .query_row("SELECT id, name FROM user", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("reads");
    assert_eq!(id, 1, "the id moved, and every note and position hangs off it");
    assert_eq!(name, "davey");

    let note: String = conn
        .query_row("SELECT body FROM note WHERE user_id = 1", [], |r| r.get(0))
        .expect("the note survived");
    assert_eq!(note, "written before there was a login");
    let slot: i64 = conn
        .query_row("SELECT slot FROM progress WHERE user_id = 1", [], |r| r.get(0))
        .expect("the position survived");
    assert_eq!(slot, 42);
}

/// A key is shown once and never again — the row holds only its hash, so
/// there is nothing to show a second time even by mistake.
#[test]
fn a_created_key_is_returned_once_and_stored_hashed() {
    let lib = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = lib.conn();
    let secret = taiyaku_library::account::create_key(&conn, 1, "phone").expect("creates");
    assert!(secret.starts_with(taiyaku_library::credential::API_KEY_PREFIX));
    let stored: String = conn
        .query_row("SELECT secret_sha256 FROM api_key", [], |r| r.get(0))
        .expect("reads");
    assert_ne!(stored, secret, "the raw key reached the database");
    assert_eq!(stored, taiyaku_library::credential::token_sha256(&secret));
}
```

- [ ] **Step 2: Implement `account::{promote, add, set_password, create_key, revoke_key, list_users, list_keys}`**

`promote` **updates** `user` where `id = (SELECT MIN(id) FROM user)` when exactly one row
exists with the locked placeholder, and errors naming the situation otherwise — it never
inserts. `set_password` calls `close_other_sessions` in the same transaction, per §4.

- [ ] **Step 3: Prompt for the password without echoing it**

The CLI prompts rather than taking `--password`, so a password never lands in shell
history or in `ps`. Read from stdin with echo disabled; when stdin is not a terminal,
read a line and say so on stderr, which is what makes the command scriptable for a first
deployment without making the interactive path unsafe.

- [ ] **Step 4: Wire dispatch and usage**

`library.rs`'s `command` gains `Some("user") => user::command(&args[1..])` and
`Some("key") => user::key_command(&args[1..])`, and both error strings listing the
subcommands gain the two new names. `main.rs`'s usage block gains the seven lines above —
the synopsis is part of the contract, per the open-items ritual.

- [ ] **Step 5: Run and commit**

```bash
cargo test --workspace 2>&1 | tail -5
git add crates/taiyaku-cli/src/user.rs crates/taiyaku-cli/src/user/tests.rs crates/taiyaku-cli/src/library.rs crates/taiyaku-cli/src/main.rs crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs
git commit -m "Promotion reaches the row spec A seeded, because inserting beside it would orphan every note silently"
```

---

### Task 9: `--allow-web-setup` and the claim route

**Files:**
- Modify: `crates/taiyaku-serve/src/session.rs` (the claim route), `crates/taiyaku-serve/src/routes.rs`, `crates/taiyaku-cli/src/main.rs`
- Test: `crates/taiyaku-serve/src/session/tests.rs`

- [ ] **Step 1: Write the failing tests**

```rust
/// §5: the claim route exists only behind the flag, and closes the instant an
/// admin exists — not when the flag is turned off, which nobody will do.
#[test]
fn the_claim_route_closes_the_moment_an_admin_exists() {
    // flag off, no admin      -> 404
    // flag on,  no admin      -> 200, and the account is user 1 promoted
    // flag on,  admin exists  -> 404
}

/// §5's constraint again, through the web path this time: claiming promotes
/// the seeded row rather than inserting beside it.
#[test]
fn claiming_from_the_browser_promotes_the_same_row_the_cli_would() { /* ... */ }
```

- [ ] **Step 2: Implement**

`POST /api/claim`, registered only when `state.auth.allow_web_setup`, answering 404
otherwise so the route's existence is not itself a signal. It calls the same
`account::promote` the CLI does — one code path, per §5 — and opens a session in the same
response.

- [ ] **Step 3: Log it loudly**

At startup and **on every request it serves**, per §5: *"a flag whose danger is invisible
while it is doing nothing is a flag that stays on."*

```rust
eprintln!(
    "WARNING: --allow-web-setup is on. Anyone who can reach this server can \
     claim it until an admin exists. It closes automatically once one does."
);
```

- [ ] **Step 4: Run and commit**

```bash
cargo test -p taiyaku-serve session:: -- --nocapture
git add crates/taiyaku-serve/src/ crates/taiyaku-cli/src/main.rs
git commit -m "A server can be claimed from a browser only behind a flag that says so on every request it serves"
```

---

### Task 10: The reader — login, logout, account screen, and one 401 handler

**Files:**
- Create: `web/src/Login.tsx`, `web/src/Login.test.tsx`, `web/src/Account.tsx`, `web/src/Account.test.tsx`
- Modify: `web/src/api.ts`, `web/src/App.tsx`

- [ ] **Step 1: Write the failing api.ts test**

```ts
/**
 * Eight call sites in `api.ts` each throw on a non-ok response, and a 401 is
 * not an error the reader can show — it is a state change. Handled in **one**
 * place rather than eight, so a ninth call site cannot forget.
 */
test('a 401 raises Unauthenticated rather than a generic error', async () => {
  globalThis.fetch = vi.fn().mockResolvedValue(
    new Response('', { status: 401 }),
  ) as unknown as typeof fetch
  await expect(listWorks()).rejects.toBeInstanceOf(Unauthenticated)
  await expect(getNotes('t')).rejects.toBeInstanceOf(Unauthenticated)
})
```

- [ ] **Step 2: Add one wrapper, not eight branches**

```ts
/**
 * A 401 is a state change rather than an error the reader can show, and it can
 * arrive from any of this module's calls. Raised as its own type from one
 * place — `App` catches it and routes to the login screen — because eight
 * separate branches is eight chances for a ninth call site to forget.
 */
export class Unauthenticated extends Error {}

const request = async (input: RequestInfo, init?: RequestInit) => {
  const res = await fetch(input, init)
  if (res.status === 401) throw new Unauthenticated('not signed in')
  return res
}
```

Then replace each `await fetch(` with `await request(` — eight sites — leaving each
existing `if (!res.ok) throw new Error(...)` exactly as it is. `pageUrl` and `thumbUrl`
are untouched: they return URLs for `<img src>` and never fetch.

- [ ] **Step 3: Build the two screens**

`Login.tsx`: name, password, submit to `POST /api/session`, error line on 401. `Account.tsx`:
change password, create an API key (**shown once**, with the sentence saying so), list and
revoke keys, list and revoke sessions. Follow `Settings.tsx`'s existing shape rather than
inventing a second panel idiom.

- [ ] **Step 4: Route on the exception**

`App.tsx` calls `GET /api/me` on load; `Unauthenticated` anywhere renders `Login`.

- [ ] **Step 5: Run the web gate**

```bash
cd web && pnpm test && pnpm run lint && pnpm run typecheck
```

- [ ] **Step 6: Commit**

```bash
git add web/src/
git commit -m "The reader routes a 401 to its own login screen, from one place rather than from eight"
```

---

### Task 11: The two things §14 says are checkable, checked

**Files:** none — this task produces measurements for `docs/notes.md`.

§14 names three things that could not be measured when the design was written. Two are
checkable now, and the design says the first *should* be checked "before the account-screen
UI promises it."

- [ ] **Step 1: Does Panels accept a `tyk_` key in the Basic password field?**

Run `serve` with an account and a created key, add the OPDS feed to Panels with the key as
the password, and record what happens. **The whole of §2's prefix design rests on OPDS
clients treating that field as opaque**, which the specification implies and nothing here
has confirmed against the real client.

If it does not work, the fallback is stated rather than invented on the spot: the account
screen offers a generated *password-shaped* credential per device instead, and §2's table
is amended rather than the prefix removed.

- [ ] **Step 2: How is the session cookie scoped through the dev proxy?**

`web/vite.config.ts` sets `changeOrigin: true`, which rewrites `Host` toward `serve`. Log
in through `pnpm dev` on `localhost:5173` and record whether the cookie comes back scoped
correctly and whether `Secure` is honoured on `http://localhost`.

**If it misbehaves the fix is in the dev proxy, not in the cookie policy** — a production
cookie must not be weakened for a development convenience, which is the rule
`vite.config.ts` already states about choosing a proxy over CORS.

- [ ] **Step 3: Measure argon2id on the host that will run this**

§14's first item and §2's open parameter set. Time `hash_password` on the NAS rather than
on this workstation, and adjust `M_COST_KIB`/`T_COST` there. A parameter set that is fast
on a workstation and slow on the server is the failure this project's measurement
discipline exists to catch.

- [ ] **Step 4: Write all three into `docs/notes.md`** under a heading naming the branch.

---

### Task 12: The documents this moves, and the ritual that moves them

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `README.md`, `web/README.md`,
  `docs/superpowers/specs/2026-08-14-library-auth-design.md`,
  `scripts/coverage-floor.mjs`, `scripts/status-test-count.mjs`

- [ ] **Step 1: Run the open-items sweep on the way in**

```bash
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

At `cec59e2` this reads 53 rows, 33 closed, 20 open, and the count words agree. Confirm
they still do before touching anything — the sweep catches drift on the way in as well as
on the way out.

- [ ] **Step 2: Correct the design document in place**

Six statements in `2026-08-14-library-auth-design.md` are stale; each is listed in this
plan's "What changed under this design since it was written". Correct them **in place with
a dated marker**, following this project's convention of correcting rather than deleting —
§3's heading becomes "Migration 4", §8's ordering paragraph gains the amendment, §5.1 gains
the decision and the date, and §12's step 1 gains the foreign-key finding.

- [ ] **Step 3: Close what this branch closes, and open what it opens**

Nothing in the current table is closed by this branch. **O44 and O53 are narrowed rather
than closed** — `librarydoc::apply` still has no route — so if their prose is touched at
all it is annotated, not ticked.

New items go in by the five-part shape inverted: the total count word at the head of the
section, the "leaving N" sentence, a table row, a prose bullet, and the "What remains"
line — whose count word **wraps across a line break**, so grep the sentence opener and
take the next line.

- [ ] **Step 4: Update the crate table, the test count and the synopses**

`docs/status.md`'s `taiyaku-serve` row, the test count in **both** places it appears
("Five crates, N tests" and "Nine of the N are standing checks"), the dependency list which
gains `argon2`, and the "Seven commands" block — which gains `library user` and
`library key`, making it nine. `README.md`'s `taiyaku serve` synopsis gains
`--trusted-proxy`, `--insecure-cookies` and `--allow-web-setup`, and "Using it" now needs a
credential in every example. `web/README.md` gains how to run the reader against an
authenticated `serve`.

- [ ] **Step 5: Re-floor the measurements this invalidates**

```bash
node scripts/coverage-floor.mjs --update
node scripts/status-test-count.mjs --update
```

- [ ] **Step 6: Run the whole gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

Expected: four green jobs, then the corpus checks green. **Never run `check-all.sh` with
`TAIYAKU_CORPUS` set** — it exports it into an unfiltered workspace test run that drags
`geometry_experiment` into the debug profile; a run was killed at 1h09m.

- [ ] **Step 7: Sweep on the way out**

Re-run Step 1's three greps, then `grep -n 'O4[4-9]\|O5[0-3]' docs/ web/src/ -r` and read
every hit — the five parts are necessary, not sufficient, and prose outside the table
describes open items in wording no grep can predict.

- [ ] **Step 8: Commit**

```bash
git add docs/ README.md web/README.md scripts/
git commit -m "The documents this branch moved, and the design's six statements that specs A and B made stale"
```

---

## Self-review

**Spec coverage.** §1 → Task 3 (guard) and the `LOGGED_HEADERS` constraint. §2 → Task 2.
§3 → Task 1. §4 → Tasks 3 and 4. §5 → Tasks 8 and 9. §5.1 → Task 6. §6 → Task 7. §7 →
Task 7. §8 → Task 3 (`user_root` resolved, filter deferred to C1 and recorded). §9 → Task
10 for what the reader gains; what it does not gain needs no task. §10's list: route
enumeration → Task 3; Basic with both credential kinds → Tasks 3 and 4; the two
`WWW-Authenticate` shapes → Task 3; cookie flags → Task 4; token ≠ stored value → Task 4;
origin check → Task 4; `X-Forwarded-For` → Task 7; promotion preserves the id → Task 8
(and Task 1 pins it through the migration); argon2 cost band → Task 2. §11 needs no task.
§12's eight steps map onto Tasks 1–10, with the plan's Tasks 5, 6, 11 and 12 added — Task
5 and Task 6 because the two `LOCAL_USER` constants are work the design's step list does
not name, Task 11 because §14 says two of its unknowns are checkable and one should be
checked before the UI promises it, Task 12 because §13 lists four documents and this
project's ritual is more than four edits. §13 → Task 12. §14 → Task 11.

**Two things this plan adds that the design does not name**, both because the code moved
under it: the foreign-key pragma in Task 1, and the second `LOCAL_USER` in
`taiyaku-serve/src/works.rs`.

**One task is deliberately less detailed than its neighbours.** Task 9's tests are named
and their three cases stated, but not written out, because they are the same shape as Task
4's and the implementer will have just written those. Task 10's screens follow
`Settings.tsx` rather than carrying full JSX here, for the same reason — the existing panel
idiom is the specification, and reproducing it in a plan would make it two specifications.

**Interface consistency check.** `credential::token_sha256` is the only hashing entry point
used by `account::{by_api_key, open_session, session_owner, close_session,
close_other_sessions, create_key}`; `Principal::from_account` is the only constructor, used
by both `auth::from_basic` and `session::from_cookie`; `guarded_routes()` is named
identically in `routes.rs`, in `auth/tests.rs` and in Task 3's interface block.
`account::promote` is called by both Task 8's CLI and Task 9's claim route, which is §5's
"no second code path" stated as a call graph.

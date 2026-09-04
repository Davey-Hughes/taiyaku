# C4 PR 2 — users and backup: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the rest of spec C4's server half — user administration with its
three refusals, session and API-key revocation, and backup as a store function,
a CLI command and three routes. No UI.

**Architecture:** Every account invariant lives in `taiyaku-library::account`,
beside `promote`'s existing "no administrator left" refusal, so the CLI and the
routes share one copy. `account::removal_cost`/`remove` mirror
`scan::removal_cost`/`remove_root` field for field, and the `DELETE` route
reuses PR 1's `?dry_run=true` shape. Backup is a new `taiyaku-library::backup`
module over SQLite's `VACUUM INTO`, run on `Library::working()` inside
`spawn_blocking` so a file copy never holds the server's one mutex.

**Tech Stack:** Rust, `rusqlite` (system SQLite, WAL, `backup` feature already
enabled), `axum`, `tokio`, `serde_json`; tests with `cargo nextest`, gate with
`scripts/check-all.sh`.

**Design documents.** This PR's build design is
[`2026-09-04-c4-pr2-users-and-backup-design.md`](../specs/2026-09-04-c4-pr2-users-and-backup-design.md);
the C4 build design it continues is
[`2026-09-02-library-configuration-build-design.md`](../specs/2026-09-02-library-configuration-build-design.md);
the product design both build is
[`2026-08-14-library-configuration-design.md`](../specs/2026-08-14-library-configuration-design.md).
Read the first before Task 1. Section references below are `PR2 §n` for this
PR's design, `build §n` for the C4 build design, and `C4 §n` for the product
design — all three have a §1 and a §4, so the prefix is never optional.

## Global Constraints

- **The gate before pushing is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.**
  Never run it with `TAIYAKU_CORPUS` set — the script exports it into an
  unfiltered `cargo test --workspace` and a run has been killed at 1h09m.
  Confirm success by quoting its `all green` line; piping it through `tail` or
  appending `; echo` masks the exit status.
- **The corpus job is separate:**
  `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`
  (~85 s).
- **Every new route goes in FOUR lists, not two — corrected 2026-09-04 during
  Task 7, which is when the gate found the other two.** In `routes.rs`:
  `guarded_routes()` and the grant-posture table beneath it. In
  `auth/tests.rs`: the `expected` array of guarded patterns that
  `every_route_refuses_an_anonymous_request` compares against, **and** the
  running `table.len() - driven` count that
  `every_route_declares_its_grant_posture` asserts — whose message carries a
  history line that has to be extended too, in the same shape as the entries
  already in it. A route added to `open_routes()` instead ships
  unauthenticated.
- **`docs/status.md` states the test count twice** and CI checks it against the
  JUnit report. The two exact strings, verified 2026-09-04 on `main` at
  `46bf9e1`: `Six crates, 1939 tests` (line 121) and
  `Nineteen of the 1939 are standing checks` (line 179). The baseline entering
  this plan is **1939 Rust tests**. Any task that adds a test moves both
  figures; Task 11 sweeps them, but a mid-plan push needs them current.
- **No attribution lines in commit messages or the PR body.** The remote is
  squash-only: the PR body becomes the single commit message, so local messages
  are working notes.
- **Test names are descriptive sentences in snake_case**, per the house style
  (`folding_a_work_away_orphans_its_hand_document_on_the_same_pass`).
- **Errors are `Result<_, String>` with a message naming the thing that
  failed**, matching every function in `taiyaku-library`.
- **A transaction inside `taiyaku-library` opens through `crate::Nested`**, not
  `unchecked_transaction` — build §2's first corrected design error. Nothing in
  this PR runs beneath `derive`, but the rule is the rule.

---

### Task 1: `account::Patch` and `update` — the scalar columns and the two refusals

O54 records that `can_download` and `can_stream` "cannot be set to 0 outside a
fixture until spec C4's user administration". This task is that administration's
writer, and it carries PR2 §2's two role rules.

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs` (add after `add`, around :674)
- Test: `crates/taiyaku-library/src/account/tests.rs`

**Interfaces:**
- Consumes: `account::Account`, `account::add`, `account::by_name`,
  `credential::LOCKED`, `crate::Nested`.
- Produces:
  ```rust
  #[derive(Debug, Clone, Default, PartialEq, Eq)]
  pub struct Patch {
      pub name: Option<String>,
      pub role: Option<String>,
      pub can_download: Option<bool>,
      pub can_stream: Option<bool>,
      pub roots: Option<Vec<i64>>,   // Task 2 reads it; Task 1 ignores it
  }
  pub fn by_id(conn: &Connection, id: i64) -> Result<Option<Account>, String>;
  pub fn update(conn: &Connection, id: i64, patch: &Patch, acting_as: Option<i64>)
      -> Result<Account, String>;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/account/tests.rs`:

```rust
/// O54's note — "nothing can set either to 0 outside a fixture until spec C4's
/// user administration" — stops being true here. A patch writes the columns it
/// names and leaves the ones it does not.
#[test]
fn a_patch_writes_the_columns_it_names_and_leaves_the_others() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let patch = Patch {
        can_download: Some(false),
        ..Patch::default()
    };
    let after = update(&conn, id, &patch, None).expect("updates");

    assert!(!after.can_download, "the named column changed");
    assert!(after.can_stream, "an unnamed column is left alone");
    assert_eq!(after.name, "reader", "an unnamed column is left alone");
    assert_eq!(after.role, "user", "an unnamed column is left alone");
}

/// PR2 §2's invariant 1, by the demotion route rather than the delete one.
/// `promote` already refuses the same lockout from the other end.
#[test]
fn demoting_the_only_administrator_who_can_log_in_is_refused() {
    let library = claimed_library();
    let conn = library.conn();

    let patch = Patch {
        role: Some("user".to_string()),
        ..Patch::default()
    };
    let why = update(&conn, 1, &patch, None).expect_err("refuses");

    assert!(
        why.contains("no administrator"),
        "the message says what would be left: {why}"
    );
    assert_eq!(
        by_id(&conn, 1).expect("reads").expect("exists").role,
        "admin",
        "the refusal wrote nothing"
    );
}

/// PR2 §2's rule 2, and it is deliberately tested **with a second admin
/// present** so it cannot pass by accidentally re-testing invariant 1: the
/// library keeps an administrator either way, and the refusal is about who is
/// asking.
#[test]
fn an_administrator_cannot_demote_themselves_even_beside_another_one() {
    let library = claimed_library();
    let conn = library.conn();
    let second = add(&conn, "second", "pw", "admin").expect("adds a second admin");
    let patch = Patch {
        role: Some("user".to_string()),
        ..Patch::default()
    };

    let why = update(&conn, second, &patch, Some(second)).expect_err("refuses");
    assert!(
        why.contains("another administrator"),
        "the message names the way out: {why}"
    );

    let done = update(&conn, second, &patch, Some(1)).expect("a different admin may");
    assert_eq!(done.role, "user");
}

/// The CLI has no session to be, so it passes `None` and rule 2 does not fire
/// for it — invariant 1 still does, which the test above this one covers.
#[test]
fn an_actorless_caller_may_demote_an_administrator_beside_another_one() {
    let library = claimed_library();
    let conn = library.conn();
    let second = add(&conn, "second", "pw", "admin").expect("adds a second admin");

    let done = update(
        &conn,
        second,
        &Patch {
            role: Some("user".to_string()),
            ..Patch::default()
        },
        None,
    )
    .expect("no actor, no self-demotion rule");
    assert_eq!(done.role, "user");
}

/// `004_auth.sql` gives `role` a `CHECK (role IN ('admin', 'user'))`, added
/// because "a typo stored a role that is neither and `Principal::from_account`'s
/// `== "admin"` quietly read it as a reader". `update` lets the constraint
/// refuse rather than keeping a second list of the two words.
#[test]
fn a_role_that_is_neither_word_is_refused_and_writes_nothing() {
    let library = claimed_library();
    let conn = library.conn();
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let why = update(
        &conn,
        id,
        &Patch {
            role: Some("moderator".to_string()),
            ..Patch::default()
        },
        None,
    )
    .expect_err("refuses");

    assert!(why.contains("moderator"), "the message names the value: {why}");
    assert_eq!(
        by_id(&conn, id).expect("reads").expect("exists").role,
        "user",
        "the refusal wrote nothing"
    );
}

/// An id naming no account is an error rather than a silent no-op: every
/// caller of this function is about to render what it returns.
#[test]
fn patching_an_account_that_does_not_exist_names_the_id() {
    let library = claimed_library();
    let conn = library.conn();

    let why = update(&conn, 4242, &Patch::default(), None).expect_err("refuses");
    assert!(why.contains("4242"), "{why}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-library account::tests::`
Expected: FAIL — `cannot find struct Patch`, `cannot find function update`,
`cannot find function by_id`.

- [ ] **Step 3: Write the implementation**

Insert into `crates/taiyaku-library/src/account.rs`, after `add`:

```rust
/// The account with this id, or `None`.
///
/// [`by_name`] answers the login question and returns the stored hash beside
/// the account; this one answers "which row is id 42" for an administrative
/// screen that already has the id and must never see a hash.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn by_id(conn: &Connection, id: i64) -> Result<Option<Account>, String> {
    conn.query_row(
        "SELECT id, name, role, can_download, can_stream FROM user WHERE id = ?1",
        params![id],
        account_from_row,
    )
    .optional()
    .map_err(|e| format!("looking up account {id}: {e}"))
}

/// What [`update`] may change about one account.
///
/// **Every field is "leave it alone" when `None`**, including `roots`: the
/// difference between `None` (do not touch the grants) and `Some(vec![])`
/// (grant nothing, which spec A2 defines as *unrestricted* — see
/// [`grant_root`]) is the whole reason this is an `Option<Vec<_>>` rather than
/// a `Vec<_>`.
///
/// **No `password` field.** [`set_password`] already exists, already hashes,
/// and already decides what happens to the account's sessions; folding it in
/// here would put an argon2 hash inside this function's transaction for every
/// caller that was only changing a checkbox.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patch {
    pub name: Option<String>,
    pub role: Option<String>,
    pub can_download: Option<bool>,
    pub can_stream: Option<bool>,
    pub roots: Option<Vec<i64>>,
}

/// Whether some account **other than** `excluding` is an administrator who can
/// actually log in.
///
/// **The same "role is not enough" reading [`admin_exists`] takes**, and for
/// the same reason: migration 1 seeds an admin row that migration 4 locks, so
/// counting roles alone would let the last real administrator be demoted or
/// deleted behind a row nobody can log in as.
fn another_admin_can_log_in(conn: &Connection, excluding: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM user
          WHERE role = 'admin' AND password_hash <> ?1 AND id <> ?2",
        params![credential::LOCKED, excluding],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .map_err(|e| format!("counting the other administrators: {e}"))
}

/// Change an account, refusing the two changes spec C4 §4 says would lock the
/// library's administrator out of it.
///
/// `acting_as` is the account making the change, or `None` for a caller with
/// no session to be — which is what the CLI is. It exists for exactly one
/// rule, C4 §4's second refusal, and that rule cannot be enforced without it.
///
/// **The two role refusals are not one rule, and merging them ships neither.**
/// The first is about the *library's* state after the change and fires for any
/// caller; the second is about *who* is asking and fires even when the library
/// keeps an administrator. A single "is this the last admin" check satisfies
/// the letter of both and lets an administrator demote themselves whenever a
/// second one exists, which C4 §4 forbids in as many words.
///
/// **`role` is validated by the schema, not here.** `004_auth.sql` gives the
/// column `CHECK (role IN ('admin', 'user'))`; a second list of the two words
/// in this function is a second list to drift.
///
/// # Errors
///
/// Returns an error if `id` names no account, if either refusal fires, or if
/// SQLite refuses the write — which includes a `role` the schema's `CHECK`
/// rejects.
pub fn update(
    conn: &Connection,
    id: i64,
    patch: &Patch,
    acting_as: Option<i64>,
) -> Result<Account, String> {
    let current = by_id(conn, id)?.ok_or_else(|| format!("no account with id {id}"))?;

    if let Some(role) = patch.role.as_deref()
        && role != current.role
        && current.role == "admin"
    {
        if acting_as == Some(id) {
            return Err(
                "an administrator cannot remove their own admin role; another \
                 administrator has to do it"
                    .to_string(),
            );
        }
        if !another_admin_can_log_in(conn, id)? {
            return Err(format!(
                "demoting `{}` would leave this library with no administrator who \
                 can log in. Give another account the admin role first",
                current.name
            ));
        }
    }

    let scope = crate::Nested::open(conn, "account_update")
        .map_err(|e| format!("updating account {id}: {e}"))?;

    let mut sets: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(v) = &patch.name {
        values.push(Box::new(v.clone()));
        sets.push(format!("name = ?{}", values.len()));
    }
    if let Some(v) = &patch.role {
        values.push(Box::new(v.clone()));
        sets.push(format!("role = ?{}", values.len()));
    }
    if let Some(v) = patch.can_download {
        values.push(Box::new(i64::from(v)));
        sets.push(format!("can_download = ?{}", values.len()));
    }
    if let Some(v) = patch.can_stream {
        values.push(Box::new(i64::from(v)));
        sets.push(format!("can_stream = ?{}", values.len()));
    }

    if !sets.is_empty() {
        values.push(Box::new(id));
        let sql = format!("UPDATE user SET {} WHERE id = ?{}", sets.join(", "), values.len());
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(AsRef::as_ref).collect();
        conn.execute(&sql, refs.as_slice())
            .map_err(|e| format!("updating account {id}: {e}"))?;
    }

    scope
        .commit()
        .map_err(|e| format!("committing the update of account {id}: {e}"))?;

    by_id(conn, id)?.ok_or_else(|| format!("account {id} vanished mid-update"))
}
```

Add `use crate::credential;` only if the file does not already have it — it
does (`account.rs:11`), so no import change is needed.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-library account::tests::`
Expected: PASS, six new tests among them.

- [ ] **Step 5: Hand-apply one mutation and watch a test go red**

The self-demotion rule is the one a "last admin" check would silently absorb.
Temporarily change `if acting_as == Some(id) {` to `if false {`, run
`cargo nextest run -p taiyaku-library account::tests::an_administrator_cannot_demote_themselves_even_beside_another_one`,
confirm it FAILS, then revert. Per
[[taiyaku-plans-need-executed-fixtures]]: a fixture where right and wrong
render the same is the failure this two-minute check exists to prevent.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs
git commit -m "account::update, with C4 §4's two role refusals"
```

---

### Task 2: `update`'s grant list — replace-all, in the same transaction

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs` (`update`, from Task 1)
- Test: `crates/taiyaku-library/src/account/tests.rs`

**Interfaces:**
- Consumes: `Patch::roots` (Task 1), `grant_root`, `revoke_root`, `roots_for`.
- Produces: nothing new — `update` gains behaviour, not a signature.

- [ ] **Step 1: Write the failing tests**

The account tests need a root to grant. Append this helper beside `a_work`:

```rust
/// Two roots to grant, since a test about *replacing* a grant list needs more
/// than one id to tell "replaced" from "appended".
fn two_roots(conn: &rusqlite::Connection) -> (i64, i64) {
    let first = crate::scan::add_root(conn, std::path::Path::new("/srv/manga"))
        .expect("adds the first root");
    let second = crate::scan::add_root(conn, std::path::Path::new("/srv/raw"))
        .expect("adds the second root");
    (first, second)
}
```

Then the tests:

```rust
/// PR2 §1.2: a `roots` list replaces the grant set wholesale rather than
/// adding to it.
#[test]
fn a_roots_list_replaces_the_grants_rather_than_adding_to_them() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, second) = two_roots(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_root(&conn, id, first).expect("grants the first");

    update(
        &conn,
        id,
        &Patch {
            roots: Some(vec![second]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert_eq!(
        roots_for(&conn, id).expect("reads"),
        vec![second],
        "the list replaced the grants; it did not append to them"
    );
}

/// An absent `roots` key leaves the grants alone, which is the difference
/// between `None` and `Some(vec![])` that makes the field an `Option<Vec<_>>`.
#[test]
fn a_patch_without_a_roots_key_leaves_the_grants_alone() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_root(&conn, id, first).expect("grants the first");

    update(
        &conn,
        id,
        &Patch {
            can_stream: Some(false),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert_eq!(roots_for(&conn, id).expect("reads"), vec![first]);
}

/// **An empty list means unrestricted, not blind**, which is spec A2's
/// sharpest edge and is stated on [`grant_root`]: every account in every
/// existing library holds an empty grant, so any other reading would have
/// emptied every library the day migration 6 applied.
#[test]
fn an_empty_roots_list_makes_an_account_unrestricted_rather_than_blind() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_root(&conn, id, first).expect("grants the first");

    update(
        &conn,
        id,
        &Patch {
            roots: Some(Vec::new()),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");

    assert!(
        roots_for(&conn, id).expect("reads").is_empty(),
        "an empty grant is what `Principal::from_account` reads as unrestricted"
    );
}

/// A refused role change must not leave a half-written grant list behind it —
/// both halves are inside one `Nested` scope.
#[test]
fn a_refused_role_change_writes_no_grants_either() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, second) = two_roots(&conn);
    grant_root(&conn, 1, first).expect("grants the admin the first root");

    update(
        &conn,
        1,
        &Patch {
            role: Some("user".to_string()),
            roots: Some(vec![second]),
            ..Patch::default()
        },
        None,
    )
    .expect_err("the only administrator cannot be demoted");

    assert_eq!(
        roots_for(&conn, 1).expect("reads"),
        vec![first],
        "the refusal happened before anything was written"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-library account::tests::`
Expected: the first three FAIL — the grants are untouched, so
`a_roots_list_replaces_the_grants_rather_than_adding_to_them` reads `[first]`
where it wants `[second]`.
(`a_patch_without_a_roots_key_leaves_the_grants_alone` and
`a_refused_role_change_writes_no_grants_either` pass already: they assert what
"do nothing" happens to produce. They are kept because they will *stop* passing
if the next step writes the grants unconditionally, which is the mistake they
exist to catch.)

- [ ] **Step 3: Write the implementation**

In `update`, immediately before `scope.commit()`:

```rust
    // **Replace-all, diffed rather than deleted and reinserted.** C4 PR 1's
    // §3 correction records what delete-and-reinsert cost the rule table —
    // fresh ids reopened every accepted proposal through an
    // `ON DELETE SET NULL` — and while `user_root` carries no such foreign
    // key, a diff is the same size of code and cannot acquire that class of
    // bug later.
    if let Some(wanted) = &patch.roots {
        let held = roots_for(conn, id)?;
        for root_id in &held {
            if !wanted.contains(root_id) {
                revoke_root(conn, id, *root_id)?;
            }
        }
        for root_id in wanted {
            if !held.contains(root_id) {
                grant_root(conn, id, *root_id)?;
            }
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-library account::tests::`
Expected: PASS, four new tests among them.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs
git commit -m "a patch's roots list replaces an account's grants"
```

---

### Task 3: `account::removal_cost` and `remove`

C4 §4: *"Deleting a user deletes their progress, notes and read history … Stated
with counts before it happens, for the same reason as §2."*

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs`
- Test: `crates/taiyaku-library/src/account/tests.rs`

**Interfaces:**
- Consumes: `another_admin_can_log_in` (Task 1), `by_id` (Task 1),
  `crate::Nested`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Removal {
      pub name: String,
      pub positions: usize,
      pub notes: usize,
      pub sessions: usize,
      pub keys: usize,
      pub grants: usize,
  }
  pub fn removal_cost(conn: &Connection, id: i64) -> Result<Removal, String>;
  pub fn remove(conn: &Connection, id: i64) -> Result<Removal, String>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
/// C4 §4's counts, taken before anything is deleted. Deliberately over an
/// account carrying one of everything, so a `Removal` that reported zeros for
/// four of the five fields could not pass.
#[test]
fn removing_an_account_states_what_it_costs_before_it_acts() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let work = a_work(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_root(&conn, id, first).expect("grants a root");
    create_key(&conn, id, "tablet").expect("mints a key");
    open_session(&conn, "tok", id, None, None).expect("opens a session");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'raw', 3)",
        rusqlite::params![id, work],
    )
    .expect("records a position");
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('n1', ?1, ?2, 'raw', 3, 'hi', '2026-01-01', '2026-01-01')",
        rusqlite::params![id, work],
    )
    .expect("records a note");

    let quoted = removal_cost(&conn, id).expect("costs it");
    assert_eq!(
        quoted,
        Removal {
            name: "reader".to_string(),
            positions: 1,
            notes: 1,
            sessions: 1,
            keys: 1,
            grants: 1,
        }
    );
    assert!(
        by_id(&conn, id).expect("reads").is_some(),
        "a quote deletes nothing"
    );

    let done = remove(&conn, id).expect("removes");
    assert_eq!(done, quoted, "the removal answers the counts from before it");
    assert!(by_id(&conn, id).expect("reads").is_none());
}

/// Five tables cascade from `user(id)`, and they only cascade because
/// `PRAGMA foreign_keys` is ON per connection (`lib.rs:339`). A test that
/// opened a bare `rusqlite::Connection` would see no cascade at all and would
/// pass a wrong answer as right — so this asserts the rows are gone, not just
/// that the account is.
#[test]
fn removing_an_account_takes_its_positions_notes_sessions_keys_and_grants() {
    let library = claimed_library();
    let conn = library.conn();
    let (first, _second) = two_roots(&conn);
    let work = a_work(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_root(&conn, id, first).expect("grants a root");
    create_key(&conn, id, "tablet").expect("mints a key");
    open_session(&conn, "tok", id, None, None).expect("opens a session");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'raw', 3)",
        rusqlite::params![id, work],
    )
    .expect("records a position");

    remove(&conn, id).expect("removes");

    for table in ["progress", "session", "api_key", "user_root"] {
        let left: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE user_id = ?1"),
                [id],
                |r| r.get(0),
            )
            .expect("counts");
        assert_eq!(left, 0, "`{table}` kept a row for a deleted account");
    }
    let works: i64 = conn
        .query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))
        .expect("counts");
    assert_eq!(works, 1, "deleting a person deletes none of the library");
}

/// PR2 §2's invariant 1, by the delete route.
#[test]
fn the_last_administrator_who_can_log_in_cannot_be_deleted() {
    let library = claimed_library();
    let conn = library.conn();
    add(&conn, "reader", "pw", "user").expect("adds a reader, who is not an admin");

    let why = remove(&conn, 1).expect_err("refuses");
    assert!(
        why.contains("no administrator"),
        "the message says what would be left: {why}"
    );
    assert!(by_id(&conn, 1).expect("reads").is_some());
}

/// The same invariant, satisfied: a second administrator makes the first
/// removable.
#[test]
fn an_administrator_beside_another_one_can_be_deleted() {
    let library = claimed_library();
    let conn = library.conn();
    let second = add(&conn, "second", "pw", "admin").expect("adds a second admin");

    remove(&conn, second).expect("removes");
    assert!(by_id(&conn, second).expect("reads").is_none());
}

/// An id naming no account is an error, not a silent success reporting zeros.
#[test]
fn removing_an_account_that_does_not_exist_names_the_id() {
    let library = claimed_library();
    let conn = library.conn();
    assert!(removal_cost(&conn, 4242).expect_err("refuses").contains("4242"));
    assert!(remove(&conn, 4242).expect_err("refuses").contains("4242"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-library account::tests::`
Expected: FAIL — `cannot find function removal_cost`, `cannot find function
remove`, `cannot find struct Removal`.

- [ ] **Step 3: Write the implementation**

Append to `crates/taiyaku-library/src/account.rs`, before `#[cfg(test)] mod tests;`:

```rust
/// What deleting one account costs.
///
/// **Named after [`crate::scan::Removal`] field for field where the two
/// overlap** — `positions` and `notes` mean the same words on both — because
/// root removal and account removal are the same shape (count, then act,
/// through one route that answers the identical body for a quote and a
/// commit), and two destructive paths that look alike should read alike.
///
/// **No work, edition or member appears here, and that is the difference from
/// a root's removal.** Deleting a person deletes none of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub name: String,
    pub positions: usize,
    pub notes: usize,
    pub sessions: usize,
    pub keys: usize,
    pub grants: usize,
}

/// Count what [`remove`] would take, without taking any of it.
///
/// # Errors
///
/// Returns an error if `id` names no account, or if a count fails.
pub fn removal_cost(conn: &Connection, id: i64) -> Result<Removal, String> {
    let name = by_id(conn, id)?
        .ok_or_else(|| format!("no account with id {id}"))?
        .name;
    count_for(conn, id, name)
}

/// The five counts and the name, as one row.
///
/// **One statement rather than five round trips**, and shared by
/// [`removal_cost`] and [`remove`] so the quote and the commit can never
/// count differently.
fn count_for(conn: &Connection, id: i64, name: String) -> Result<Removal, String> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM progress  WHERE user_id = ?1),
                (SELECT COUNT(*) FROM note      WHERE user_id = ?1),
                (SELECT COUNT(*) FROM session   WHERE user_id = ?1),
                (SELECT COUNT(*) FROM api_key   WHERE user_id = ?1),
                (SELECT COUNT(*) FROM user_root WHERE user_id = ?1)",
        params![id],
        |row| {
            Ok(Removal {
                name,
                positions: row.get::<_, i64>(0)?.unsigned_abs() as usize,
                notes: row.get::<_, i64>(1)?.unsigned_abs() as usize,
                sessions: row.get::<_, i64>(2)?.unsigned_abs() as usize,
                keys: row.get::<_, i64>(3)?.unsigned_abs() as usize,
                grants: row.get::<_, i64>(4)?.unsigned_abs() as usize,
            })
        },
    )
    .map_err(|e| format!("counting what removing account {id} costs: {e}"))
}

/// Delete an account and everything `ON DELETE CASCADE` takes with it,
/// answering the counts from **before** the delete.
///
/// **The counts are taken inside the transaction that deletes**, exactly as
/// `scan::apply_removal` takes a root's: counting afterwards answers zero for
/// the numbers this function exists to state honestly.
///
/// **Refuses to delete the last administrator who can log in**, which is C4
/// §4's first refusal and the same invariant [`update`] holds against a
/// demotion. `promote` states the third face of it.
///
/// # Errors
///
/// Returns an error if `id` names no account, if the refusal fires, or if
/// SQLite refuses the delete.
pub fn remove(conn: &Connection, id: i64) -> Result<Removal, String> {
    let account = by_id(conn, id)?.ok_or_else(|| format!("no account with id {id}"))?;
    if account.role == "admin" && !another_admin_can_log_in(conn, id)? {
        return Err(format!(
            "removing `{}` would leave this library with no administrator who can \
             log in. Give another account the admin role first",
            account.name
        ));
    }

    let scope = crate::Nested::open(conn, "account_remove")
        .map_err(|e| format!("removing account {id}: {e}"))?;
    let cost = count_for(conn, id, account.name)?;
    conn.execute("DELETE FROM user WHERE id = ?1", params![id])
        .map_err(|e| format!("removing account {id}: {e}"))?;
    scope
        .commit()
        .map_err(|e| format!("committing the removal of account {id}: {e}"))?;
    Ok(cost)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-library account::tests::`
Expected: PASS, five new tests among them.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs
git commit -m "account::removal_cost and remove, with the last-admin refusal"
```

---

### Task 4: `backup.rs` — `directory_for`, `take`, `list`

**Files:**
- Create: `crates/taiyaku-library/src/backup.rs`
- Create: `crates/taiyaku-library/src/backup/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (add `pub mod backup;` beside the
  other module declarations)

**Interfaces:**
- Consumes: `rusqlite::Connection`, `Library::open`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Backup { pub name: String, pub bytes: u64, pub taken: String }
  pub const PREFIX: &str = "library-";
  pub const SUFFIX: &str = ".db";
  pub fn directory_for(db: &Path) -> PathBuf;
  pub fn take(conn: &Connection, into_dir: &Path) -> Result<Backup, String>;
  pub fn list(dir: &Path) -> Result<Vec<Backup>, String>;
  ```

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-library/src/backup/tests.rs`:

```rust
use super::*;
use crate::Library;

/// A scratch directory that cleans itself up, matching the shape the rest of
/// this crate's file-touching tests use.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("taiyaku-backup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creates the scratch directory");
    dir
}

/// PR2 §1.3: the directory is derived from the database path, because the
/// server has to be able to compute it in order to list it.
#[test]
fn the_backup_directory_sits_beside_the_database() {
    assert_eq!(
        directory_for(std::path::Path::new("/home/x/.local/share/taiyaku/library.db")),
        std::path::PathBuf::from("/home/x/.local/share/taiyaku/backups")
    );
}

/// **A backup restores.** Not "the file exists and is non-empty": the whole
/// point of `VACUUM INTO` over a file copy is that the snapshot is consistent,
/// and the only way to assert that is to open it and read something back.
#[test]
fn a_backup_opens_as_a_library_and_still_holds_the_work() {
    let dir = scratch("restores");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let work = {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Restored","reading":"rtl"},
            "editions":{"raw":{"label":"R","members":[
              {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("imports")
    };

    let into = directory_for(&db);
    let taken = {
        let conn = library.conn();
        take(&conn, &into).expect("takes a backup")
    };

    assert!(taken.name.starts_with(PREFIX) && taken.name.ends_with(SUFFIX), "{taken:?}");
    assert!(taken.bytes > 0, "{taken:?}");

    let restored = Library::open(&into.join(&taken.name)).expect("opens the backup");
    let title: String = restored
        .conn()
        .query_row("SELECT title FROM work WHERE id = ?1", [work], |r| r.get(0))
        .expect("reads the work back out of the backup");
    assert_eq!(title, "Restored");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `list` answers newest first and reports each file's real size, so a screen
/// can render "3 backups, newest 2026-09-04" without a second stat call.
#[test]
fn listing_answers_newest_first_and_ignores_anything_else_in_the_directory() {
    let dir = scratch("listing");
    std::fs::write(dir.join("library-20260901T000000Z.db"), b"aaaa").expect("writes");
    std::fs::write(dir.join("library-20260903T000000Z.db"), b"bb").expect("writes");
    std::fs::write(dir.join("notes.txt"), b"not a backup").expect("writes");
    std::fs::create_dir(dir.join("library-20260902T000000Z.db.d")).expect("creates a decoy dir");

    let found = list(&dir).expect("lists");

    assert_eq!(
        found.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        vec!["library-20260903T000000Z.db", "library-20260901T000000Z.db"],
        "newest first, and nothing that is not a backup"
    );
    assert_eq!(found[0].bytes, 2);
    assert_eq!(found[1].bytes, 4);
    assert_eq!(found[0].taken, "20260903T000000Z");

    let _ = std::fs::remove_dir_all(&dir);
}

/// A directory that does not exist yet is an empty listing, not an error: a
/// library that has never been backed up is the ordinary case, and the screen
/// that lists backups is the one an operator opens *before* taking the first.
#[test]
fn listing_a_directory_that_does_not_exist_yet_is_empty_rather_than_an_error() {
    let dir = scratch("absent").join("never-made");
    assert_eq!(list(&dir).expect("lists"), Vec::new());
}

/// `VACUUM INTO` refuses an existing target rather than overwriting it, and
/// PR2 §4 keeps that failure as itself rather than adding a suffix counter for
/// a case only a test loop produces.
#[test]
fn a_second_backup_in_the_same_second_is_refused_rather_than_overwriting() {
    let dir = scratch("collision");
    let db = dir.join("library.db");
    let library = Library::open(&db).expect("opens a library");
    let into = directory_for(&db);
    let conn = library.conn();

    let first = take(&conn, &into).expect("takes the first");
    std::fs::write(into.join(&first.name), b"stand-in for a same-second name")
        .expect("keeps the name taken");

    // Re-taking with the same name must not silently replace it. Rather than
    // race the clock, assert the property directly on the name that exists.
    let why = take_named(&conn, &into, &first.name).expect_err("refuses");
    assert!(why.contains(&first.name), "the message names the file: {why}");

    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-library backup::`
Expected: FAIL — `file not found for module backup`.

- [ ] **Step 3: Write the implementation**

Create `crates/taiyaku-library/src/backup.rs`:

```rust
//! Backup, per store spec §13.4 and spec C4 §5.
//!
//! **`VACUUM INTO`, not a file copy.** SQLite's own statement takes a
//! consistent snapshot of a live database without stopping the server and
//! needs no external tool; copying the file out from under a WAL-mode writer
//! does not. `rusqlite`'s `backup` feature is enabled for exactly this, and
//! `Cargo.toml` has said so since before anything called it.
//!
//! **A backup is not a sidecar export**, which store spec §13.4 states and C4
//! §5 puts on the screen: `export` writes five documents per work and carries
//! none of the cross-work data — collections, tags, external ids, users, read
//! history — so somebody who exported their library still needs this.
//!
//! **Restore is deliberately out-of-band**: stop the server, move the file,
//! start it. C4 §5 declines an in-server restore, and this module has no
//! function for one.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// What every backup file's name starts with.
///
/// **A constant because three things read it** — [`take`] mints it, [`list`]
/// filters on it and [`prune`] deletes on it — and `prune` is the only
/// file-deleting function in this crate. A literal repeated at the delete site
/// is a literal that can drift away from the one at the create site.
pub const PREFIX: &str = "library-";

/// What every backup file's name ends with. See [`PREFIX`].
pub const SUFFIX: &str = ".db";

/// One backup file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The file name alone, never a path — see `backup_api`'s download route,
    /// which resolves a requested name against this listing rather than
    /// joining a caller's string onto a directory.
    pub name: String,
    pub bytes: u64,
    /// The timestamp embedded in the name, `YYYYMMDDTHHMMSSZ`.
    pub taken: String,
}

/// Where backups of `db` live: `<db parent>/backups`.
///
/// **Derived rather than configured, because the server has to compute it.**
/// `GET /api/library/backups` answers "what exists", and it can only answer
/// for a directory it can find from the one thing it holds — the database
/// path, which `Library::path` already returns. The CLI's `--out DIR` writes
/// somewhere else on purpose and the server will not list what lands there;
/// the command's own help says so.
#[must_use]
pub fn directory_for(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("backups")
}

/// The name a backup taken now would get, from **SQLite's own clock**.
///
/// This workspace has no time crate — `docs/status.md` states the short
/// dependency list as a fact about the project — and every other timestamp in
/// the store comes from `datetime('now')`. One `query_row` costs nothing and
/// adds no dependency.
fn name_now(conn: &Connection) -> Result<String, String> {
    let stamp: String = conn
        .query_row("SELECT strftime('%Y%m%dT%H%M%SZ', 'now')", [], |row| row.get(0))
        .map_err(|e| format!("reading the clock for a backup name: {e}"))?;
    Ok(format!("{PREFIX}{stamp}{SUFFIX}"))
}

/// Take a backup into `into_dir`, creating the directory if it is not there.
///
/// **Never call this on the server's own connection.** `ServeState.library`
/// holds one `Mutex<Connection>` for the whole process and a `VACUUM INTO`
/// stalls every request for the length of a file copy; the route runs it on
/// `Library::working()` inside `spawn_blocking`, which is the third caller of
/// that reasoning after the rule preview and the scan.
///
/// # Errors
///
/// Returns an error if the directory cannot be created, if the clock cannot be
/// read, or if SQLite refuses the `VACUUM INTO` — which includes the target
/// file already existing.
pub fn take(conn: &Connection, into_dir: &Path) -> Result<Backup, String> {
    take_named(conn, into_dir, &name_now(conn)?)
}

/// [`take`] with the name decided by the caller.
///
/// Split out so a test can drive the same-second collision without racing the
/// clock, and so the error message names the file rather than the statement.
///
/// # Errors
///
/// As [`take`].
pub fn take_named(conn: &Connection, into_dir: &Path, name: &str) -> Result<Backup, String> {
    std::fs::create_dir_all(into_dir)
        .map_err(|e| format!("creating {}: {e}", into_dir.display()))?;
    let target = into_dir.join(name);

    // `VACUUM INTO` refuses an existing target rather than overwriting it, and
    // that refusal is kept as itself: a suffix counter would be machinery for
    // a case only a test loop produces (PR2 §4).
    conn.execute("VACUUM INTO ?1", rusqlite::params![target.to_string_lossy()])
        .map_err(|e| format!("writing the backup {name}: {e}"))?;

    let bytes = std::fs::metadata(&target)
        .map_err(|e| format!("reading the size of {}: {e}", target.display()))?
        .len();
    Ok(Backup {
        taken: stamp_of(name).unwrap_or_default(),
        name: name.to_string(),
        bytes,
    })
}

/// The timestamp inside a backup's name, or `None` for a name that is not one.
///
/// **The one place that decides what counts as a backup file**, read by
/// [`list`] and therefore by [`prune`].
fn stamp_of(name: &str) -> Option<String> {
    let stamp = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    // `YYYYMMDDTHHMMSSZ` — 16 characters, digits with a `T` and a `Z`. Checked
    // rather than assumed so `prune` cannot be talked into deleting
    // `library-.db` or `library-../../etc/passwd.db`.
    let shaped = stamp.len() == 16
        && stamp.as_bytes()[8] == b'T'
        && stamp.as_bytes()[15] == b'Z'
        && stamp
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 8 || i == 15 || b.is_ascii_digit());
    shaped.then(|| stamp.to_string())
}

/// Every backup in `dir`, newest first.
///
/// **A directory that does not exist is an empty listing**, not an error: a
/// library nobody has backed up yet is the ordinary case, and the screen that
/// lists backups is the one an operator opens before taking the first.
///
/// Files that are not backups are ignored, and so is a *directory* whose name
/// happens to match — the entry has to be a file.
///
/// # Errors
///
/// Returns an error if the directory exists and cannot be read.
pub fn list(dir: &Path) -> Result<Vec<Backup>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("reading {}: {e}", dir.display())),
    };

    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(taken) = stamp_of(&name) else {
            continue;
        };
        let metadata = entry
            .metadata()
            .map_err(|e| format!("reading the size of {name}: {e}"))?;
        if !metadata.is_file() {
            continue;
        }
        found.push(Backup {
            name,
            bytes: metadata.len(),
            taken,
        });
    }
    // The stamp sorts lexicographically in time order, which is the whole
    // reason the name carries `YYYYMMDDTHHMMSSZ` rather than anything friendlier.
    found.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.name.cmp(&a.name)));
    Ok(found)
}

#[cfg(test)]
mod tests;
```

Add to `crates/taiyaku-library/src/lib.rs`, beside the other `pub mod` lines:

```rust
pub mod backup;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-library backup::`
Expected: PASS, five tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/backup.rs crates/taiyaku-library/src/backup/tests.rs crates/taiyaku-library/src/lib.rs
git commit -m "taiyaku-library::backup: VACUUM INTO, and a listing of what exists"
```

---

### Task 5: `backup::prune`

The only file-deleting function in `taiyaku-library`. It is written like one.

**Files:**
- Modify: `crates/taiyaku-library/src/backup.rs`
- Test: `crates/taiyaku-library/src/backup/tests.rs`

**Interfaces:**
- Consumes: `list`, `stamp_of` (Task 4).
- Produces: `pub fn prune(dir: &Path, keep: usize) -> Result<Vec<String>, String>;`

- [ ] **Step 1: Write the failing tests**

```rust
/// PR2 §8: five backups, `keep: 2`, the three oldest go and **nothing else in
/// the directory does** — the unrelated file is the whole point of the
/// fixture, since a `prune` that globbed the directory would pass without it.
#[test]
fn pruning_keeps_the_newest_and_deletes_nothing_else_in_the_directory() {
    let dir = scratch("prune");
    for day in ["01", "02", "03", "04", "05"] {
        std::fs::write(dir.join(format!("library-202609{day}T000000Z.db")), b"x")
            .expect("writes a backup");
    }
    std::fs::write(dir.join("notes.txt"), b"keep me").expect("writes an unrelated file");
    std::fs::write(dir.join("library-.db"), b"keep me too").expect("writes a near-miss");

    let removed = prune(&dir, 2).expect("prunes");

    assert_eq!(
        removed,
        vec![
            "library-20260901T000000Z.db".to_string(),
            "library-20260902T000000Z.db".to_string(),
            "library-20260903T000000Z.db".to_string(),
        ],
        "oldest first, and it reports what it removed rather than a count"
    );
    assert_eq!(
        list(&dir).expect("lists").len(),
        2,
        "the two newest survive"
    );
    assert!(dir.join("notes.txt").exists(), "an unrelated file survives");
    assert!(
        dir.join("library-.db").exists(),
        "a name that is not a real backup name survives"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `keep` at or above the count deletes nothing, and `keep: 0` is a caller
/// asking for everything to go — which is allowed, said out loud, and is why
/// the CLI's flag is absent by default rather than defaulting to a number.
#[test]
fn pruning_more_than_there_are_deletes_nothing_and_zero_deletes_all() {
    let dir = scratch("prune-edges");
    for day in ["01", "02"] {
        std::fs::write(dir.join(format!("library-202609{day}T000000Z.db")), b"x")
            .expect("writes a backup");
    }

    assert!(prune(&dir, 5).expect("prunes").is_empty());
    assert_eq!(list(&dir).expect("lists").len(), 2);

    assert_eq!(prune(&dir, 0).expect("prunes").len(), 2);
    assert!(list(&dir).expect("lists").is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// A directory with no backups in it is not an error, for the same reason
/// `list` is not: the first run of a cron entry meets exactly this state.
#[test]
fn pruning_a_directory_that_does_not_exist_yet_removes_nothing() {
    let dir = scratch("prune-absent").join("never-made");
    assert!(prune(&dir, 3).expect("prunes").is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-library backup::`
Expected: FAIL — `cannot find function prune`.

- [ ] **Step 3: Write the implementation**

Append to `crates/taiyaku-library/src/backup.rs`, before `#[cfg(test)] mod tests;`:

```rust
/// Keep the `keep` newest backups in `dir` and delete the rest, answering
/// which files were removed, oldest first.
///
/// **This is the only function in this crate that deletes a file, and it is
/// written to be unable to delete the wrong one.** It deletes only names
/// [`list`] returned — which is to say only names [`stamp_of`] accepted, in
/// this one directory, never recursing and never following a subdirectory —
/// and it returns the names rather than a count, so the CLI prints what
/// happened instead of a number the operator has to trust.
///
/// A `keep` larger than the number of backups removes nothing. A `keep` of `0`
/// removes all of them, which is a caller asking for exactly that; the CLI's
/// `--keep` is absent by default rather than defaulting to a number, so
/// nothing reaches this with a value nobody typed.
///
/// # Errors
///
/// Returns an error if the directory cannot be read, or if a delete fails —
/// after which the returned error names the file and the files already
/// deleted stay deleted.
pub fn prune(dir: &Path, keep: usize) -> Result<Vec<String>, String> {
    let found = list(dir)?;           // newest first
    if found.len() <= keep {
        return Ok(Vec::new());
    }

    let mut removed = Vec::new();
    // `found` is newest-first, so everything past `keep` is what goes; reversed
    // so the answer reads oldest-first, which is the order a person expects a
    // retention sweep to have worked in.
    for backup in found.into_iter().skip(keep).rev() {
        let path = dir.join(&backup.name);
        std::fs::remove_file(&path)
            .map_err(|e| format!("removing the backup {}: {e}", backup.name))?;
        removed.push(backup.name);
    }
    Ok(removed)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-library backup::`
Expected: PASS, eight tests.

- [ ] **Step 5: Hand-apply one mutation and watch a test go red**

Change `for backup in found.into_iter().skip(keep).rev()` to
`.skip(keep)` without `.rev()` and confirm
`pruning_keeps_the_newest_and_deletes_nothing_else_in_the_directory` FAILS on
the order of `removed`, then revert. The deletions are the same either way, so
only the reported order distinguishes them — which is what the assertion is
for.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/backup.rs crates/taiyaku-library/src/backup/tests.rs
git commit -m "backup::prune, the one file-deleting function in the store"
```

---

### Task 6: `taiyaku library backup`

**Files:**
- Modify: `crates/taiyaku-cli/src/library.rs` (`command` dispatch at :67-84, and
  a new `backup` fn)
- Modify: `crates/taiyaku-cli/src/main.rs` (the synopsis block, after the
  `library key` line at :200)
- Modify: `README.md` (the command list)
- Test: `crates/taiyaku-cli/src/library/tests.rs`

**Interfaces:**
- Consumes: `taiyaku_library::backup::{directory_for, list, prune, take}`,
  `library::resolve_db_path`, `crate::{option_value, reject_unknown_flags}`.
- Produces: nothing other crates read.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-cli/src/library/tests.rs`:

```rust
/// `taiyaku library backup` writes a real, openable snapshot into the
/// directory the server will look in, and `--keep` reports what it pruned.
#[test]
fn backup_writes_a_snapshot_beside_the_database_and_keep_prunes() {
    let dir = scratch("backup-command");
    let db = dir.join("library.db");
    taiyaku_library::Library::open(&db).expect("creates the library");

    crate::library::command(&[
        "backup".to_string(),
        "--db".to_string(),
        db.to_string_lossy().into_owned(),
    ])
    .expect("takes a backup");

    let into = taiyaku_library::backup::directory_for(&db);
    let found = taiyaku_library::backup::list(&into).expect("lists");
    assert_eq!(found.len(), 1, "one backup, beside the database");
    taiyaku_library::Library::open(&into.join(&found[0].name)).expect("the backup opens");

    // Two more, minted by hand so the test does not have to wait out a second.
    for day in ["01", "02"] {
        std::fs::write(into.join(format!("library-202601{day}T000000Z.db")), b"x")
            .expect("writes an older backup");
    }
    crate::library::command(&[
        "backup".to_string(),
        "--db".to_string(),
        db.to_string_lossy().into_owned(),
        "--keep".to_string(),
        "1".to_string(),
    ])
    .expect("takes a backup and prunes");

    let left = taiyaku_library::backup::list(&into).expect("lists");
    assert_eq!(left.len(), 1, "--keep 1 leaves exactly the newest: {left:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `--keep` that is not a number is the caller's mistake and says so, rather
/// than silently pruning nothing.
#[test]
fn backup_refuses_a_keep_that_is_not_a_number() {
    let dir = scratch("backup-keep");
    let db = dir.join("library.db");
    taiyaku_library::Library::open(&db).expect("creates the library");

    let why = crate::library::command(&[
        "backup".to_string(),
        "--db".to_string(),
        db.to_string_lossy().into_owned(),
        "--keep".to_string(),
        "some".to_string(),
    ])
    .expect_err("refuses");
    assert!(why.contains("--keep"), "{why}");

    let _ = std::fs::remove_dir_all(&dir);
}
```

`scratch` is that module's existing per-run temp-directory helper
(`library/tests.rs:34`) — reused rather than duplicated, and its own doc comment
records why the name carries the process id.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-cli library::tests::backup`
Expected: FAIL — `library has no subcommand \`backup\``.

- [ ] **Step 3: Write the implementation**

In `crates/taiyaku-cli/src/library.rs`, add to the `match` in `command`:

```rust
        Some("backup") => backup(&args[1..]),
```

and update both message strings in the same `match`:

```rust
        Some(other) => Err(format!(
            "library has no subcommand `{other}`; it has import, export, scan, rules, \
             proposals, user, key, root and backup"
        )),
        None => Err(
            "library needs a subcommand: import, export, scan, rules, proposals, user, key, \
             root or backup"
                .to_string(),
        ),
```

Then add the function:

```rust
/// `taiyaku library backup [--out DIR] [--keep N] [--db PATH]` — store spec
/// §13.4 and spec C4 §5.
///
/// **`--out` writes somewhere the server will not look**, which the usage line
/// says out loud: `GET /api/library/backups` lists `backup::directory_for(db)`
/// and can only compute that one directory. The flag is an escape hatch for an
/// operator writing to another volume, not a second configured location.
///
/// **`--keep` is absent by default.** A command that deletes files the first
/// time it runs is a surprise on the run where the operator is least sure what
/// it does; retention is opt-in, and the cron entry that wants it says so.
///
/// **No schedule**, per this PR's build design §5 and C4's: a cron entry runs
/// this, and the server gains no timer at all in C4.
fn backup(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library backup", args, &["--db", "--out", "--keep"])?;
    reject_positional_arguments("library backup", args)?;
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let into = match option_value(args, "--out", "a directory")? {
        Some(dir) => PathBuf::from(dir),
        None => taiyaku_library::backup::directory_for(&db),
    };
    let keep = match option_value(args, "--keep", "a count")? {
        Some(n) => Some(
            n.parse::<usize>()
                .map_err(|_| format!("`--keep {n}` is not a count"))?,
        ),
        None => None,
    };

    let library = Library::open(&db)?;
    let taken = {
        let conn = library.conn();
        taiyaku_library::backup::take(&conn, &into)?
    };
    println!(
        "wrote {} ({} bytes)",
        into.join(&taken.name).display(),
        taken.bytes
    );

    if let Some(keep) = keep {
        for name in taiyaku_library::backup::prune(&into, keep)? {
            println!("pruned {}", into.join(&name).display());
        }
    }
    Ok(())
}
```

Add `use std::path::PathBuf;` to the imports if it is not already there, and
`reject_positional_arguments` to the `use crate::{…}` list.

In `crates/taiyaku-cli/src/main.rs`, after the `library key` synopsis line:

```rust
    println!(
        "       taiyaku library backup [--out DIR] [--keep N] [--db PATH]  \
         (--out writes where the server will not list it)"
    );
```

In `README.md`, add the same line to the command list beside the other
`taiyaku library` entries.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-cli library::tests::backup`
Expected: PASS, two tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/library.rs crates/taiyaku-cli/src/library/tests.rs crates/taiyaku-cli/src/main.rs README.md
git commit -m "taiyaku library backup, with --out and --keep"
```

---

### Task 7: `users_api.rs` — list and create

**Files:**
- Create: `crates/taiyaku-serve/src/users_api.rs`
- Create: `crates/taiyaku-serve/src/users_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod users_api;`)
- Modify: `crates/taiyaku-serve/src/routes.rs` (`guarded_routes()` around :1948,
  and the grant-posture table around :2384)

**Interfaces:**
- Consumes: `config_api::admin_or_forbidden` (already `pub(crate)`),
  `taiyaku_library::account::{Account, KeySummary, SessionSummary, add, list,
  keys_of, sessions_of, granted_roots}`, `crate::ServeState`,
  `crate::auth::Principal`.
- Produces:
  ```rust
  pub(crate) async fn list_users(State<Arc<ServeState>>, Extension<Principal>) -> Response;
  pub(crate) async fn create_user(State<Arc<ServeState>>, Extension<Principal>, Json<Value>) -> Response;
  ```
  and, for Tasks 8–9 to reuse:
  ```rust
  enum UserAnswer { Missing(i64), Unusable(String), Failed(String) }  // Task 8 adds Refused
  fn user_json(account: &Account, can_log_in: bool,
               sessions: &[SessionSummary], keys: &[KeySummary],
               roots: &[(i64, String)]) -> serde_json::Value;
  ```

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/users_api/tests.rs`:

```rust
use crate::test_support::{self, get_json, send_json};
use axum::http::{Method, StatusCode};

/// C4 §4: the list carries each account's sessions, keys and grants, because
/// the screen shows all three beside the account and a second request per row
/// is a screen that renders in stages.
#[tokio::test]
async fn the_user_list_carries_sessions_keys_and_grants() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let json = get_json(&router, "/api/library/users").await;
    let users = json["users"].as_array().expect("a users array");
    assert!(!users.is_empty(), "{json}");

    let first = &users[0];
    for key in [
        "id", "name", "role", "can_download", "can_stream", "can_log_in", "sessions", "keys",
        "roots",
    ] {
        assert!(!first[key].is_null(), "`{key}` is missing from a user row: {json}");
    }
    assert!(first["sessions"].is_array(), "{json}");
    assert!(first["keys"].is_array(), "{json}");
    assert!(first["roots"].is_array(), "{json}");
}

/// A created account can log in and is a reader unless the body says
/// otherwise — `004_auth.sql` defaults the column to `user` because "any
/// insert that omitted the column minted an administrator".
#[tokio::test]
async fn creating_a_user_defaults_to_a_reader_and_never_echoes_a_hash() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(response).await).expect("json");

    assert_eq!(body["user"]["name"], "reader");
    assert_eq!(body["user"]["role"], "user");
    assert_eq!(body["user"]["can_log_in"], true);
    assert!(
        !serde_json::to_string(&body)
            .expect("serialises")
            .contains("argon2"),
        "no PHC string may reach the wire: {body}"
    );
}

/// A body missing a required field is the caller's own mistake — 400, before
/// any store call.
#[tokio::test]
async fn creating_a_user_without_a_password_is_the_callers_mistake() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();

    let response = send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A duplicate name is refused by `user.name`'s `UNIQUE`, and the refusal is
/// the caller's mistake rather than a 500.
#[tokio::test]
async fn creating_a_user_with_a_name_already_taken_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let body = serde_json::json!({ "name": "reader", "password": "hunter2" });

    send_json(&router, Method::POST, "/api/library/users", &body).await;
    let again = send_json(&router, Method::POST, "/api/library/users", &body).await;
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
}

/// `config_api::tests::every_config_api_route_refuses_a_reader`'s own
/// reasoning, for this module's routes: a reviewer's checklist goes stale the
/// first time somebody adds a route and does not think about it.
#[tokio::test]
async fn every_users_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::GET, "/api/library/users"),
        (Method::POST, "/api/library/users"),
        (Method::PATCH, "/api/library/users/1"),
        (Method::DELETE, "/api/library/users/1"),
        (Method::DELETE, "/api/library/users/1/sessions/abc"),
        (Method::DELETE, "/api/library/users/1/keys/1"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-serve users_api::`
Expected: FAIL — `file not found for module users_api`.

- [ ] **Step 3: Write the implementation**

Create `crates/taiyaku-serve/src/users_api.rs`:

```rust
//! The `/api/library/users*` handlers — spec C4 §4 and §6.
//!
//! Thin by design, like `config_api.rs` and `curation.rs`: parse, call
//! [`taiyaku_library::account`], serialise. **Every refusal that stops this
//! screen locking an administrator out lives in the store**, beside
//! `account::promote`'s own — the CLI is a second writer, and a copy of an
//! invariant in each is a copy that drifts. This file's only job around them
//! is to give each the right status code.
//!
//! **Not `config_api`.** That module is 1872 lines and its tests 3006; a third
//! and fourth route family inside it would put the pair past 2500 and 4000.
//! It does lend this one its `admin_or_forbidden`, which it already declared
//! `pub(crate)` and whose own doc comment anticipated exactly this rather than
//! a third copy.
//!
//! **No password ever leaves here.** `account::Account` carries no hash by
//! construction — `by_name` returns it separately for exactly one comparison —
//! so there is no field for [`user_json`] to leak even by accident.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_library::account::{self, Account, KeySummary, SessionSummary};

use crate::{ServeState, auth::Principal, config_api::admin_or_forbidden};

/// Why a users route could not answer.
///
/// Shaped like `config_api::RootAnswer`. Task 8 adds a fourth variant,
/// `Refused`, when it adds the routes that can produce one — declared there
/// rather than here so this task does not leave a variant nothing constructs.
#[derive(Debug)]
enum UserAnswer {
    Missing(i64),
    Unusable(String),
    Failed(String),
}

impl IntoResponse for UserAnswer {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Missing(id) => (StatusCode::NOT_FOUND, format!("there is no account {id}")),
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// `config_api::root_panicked`'s twin for this module.
fn user_panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    UserAnswer::Failed(format!("{what} panicked: {why}")).into_response()
}

/// One account, whole, as C4 §4's screen shows it: the row, whether anybody
/// can log in as it, its sessions, its keys and its root grants.
///
/// **All five together rather than a row plus three more requests.** The
/// screen shows them beside the account, and a list that answered the row
/// alone would render in stages.
fn user_json(
    account: &Account,
    can_log_in: bool,
    sessions: &[SessionSummary],
    keys: &[KeySummary],
    roots: &[(i64, String)],
) -> serde_json::Value {
    serde_json::json!({
        "id": account.id,
        "name": account.name,
        "role": account.role,
        "can_download": account.can_download,
        "can_stream": account.can_stream,
        // An account still holding `credential::LOCKED` has a row and no way
        // in, and nothing else about it says so — `account::list`'s own doc
        // comment calls this "the second half worth printing".
        "can_log_in": can_log_in,
        "sessions": sessions.iter().map(|s| serde_json::json!({
            "id": s.id,
            "created": s.created,
            "last_seen": s.last_seen,
            "user_agent": s.user_agent,
            "ip": s.ip,
            "current": s.current,
        })).collect::<Vec<_>>(),
        "keys": keys.iter().map(|k| serde_json::json!({
            "id": k.id,
            "name": k.name,
            "created": k.created,
            "last_used": k.last_used,
        })).collect::<Vec<_>>(),
        "roots": roots.iter().map(|(id, path)| serde_json::json!({
            "id": id,
            "path": path,
        })).collect::<Vec<_>>(),
    })
}

/// Read one account's sessions, keys and grants and render it whole.
fn assembled(
    conn: &rusqlite::Connection,
    account: &Account,
    can_log_in: bool,
) -> Result<serde_json::Value, String> {
    let sessions = account::sessions_of(conn, account.id, None)?;
    let keys = account::keys_of(conn, account.id)?;
    let roots = account::granted_roots(conn, account.id)?;
    Ok(user_json(account, can_log_in, &sessions, &keys, &roots))
}

/// `GET /api/library/users` — every account, with what C4 §4's screen shows
/// beside each.
pub(crate) async fn list_users(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }

    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let mut rendered = Vec::new();
        for (account, can_log_in) in account::list(&conn)? {
            rendered.push(assembled(&conn, &account, can_log_in)?);
        }
        Ok::<_, String>(rendered)
    })
    .await;

    match built {
        Ok(Ok(users)) => Json(serde_json::json!({ "users": users })).into_response(),
        Ok(Err(why)) => UserAnswer::Failed(why).into_response(),
        Err(why) => user_panicked("listing accounts", &why),
    }
}

/// `POST /api/library/users` — create an account. C4 §4: no self-service
/// registration; an admin creates accounts, per A2 §11.
///
/// **`role` defaults to `user`.** `004_auth.sql` made the column default that
/// way because "any insert that omitted the column minted an administrator",
/// and this route must not have a second opinion.
pub(crate) async fn create_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }

    let (Some(name), Some(password)) = (
        body.get("name").and_then(serde_json::Value::as_str),
        body.get("password").and_then(serde_json::Value::as_str),
    ) else {
        return UserAnswer::Unusable(
            "creating an account needs a `name` and a `password`".to_string(),
        )
        .into_response();
    };
    let role = body
        .get("role")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("user")
        .to_string();
    let (name, password) = (name.to_string(), password.to_string());

    let created = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        // **A duplicate name, and a role the schema's CHECK rejects, are both
        // the caller's own mistake**, not a fault — `user.name` is `UNIQUE`
        // and `role` carries `CHECK (role IN ('admin', 'user'))`, so a 500
        // here would report a typo as a server failure.
        let id = account::add(&conn, &name, &password, &role).map_err(UserAnswer::Unusable)?;
        let account = account::by_id(&conn, id)
            .map_err(UserAnswer::Failed)?
            .ok_or(UserAnswer::Missing(id))?;
        assembled(&conn, &account, true).map_err(UserAnswer::Failed)
    })
    .await;

    match created {
        Ok(Ok(user)) => Json(serde_json::json!({ "user": user })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("creating an account", &why),
    }
}

#[cfg(test)]
mod tests;
```

Add `mod users_api;` to `crates/taiyaku-serve/src/lib.rs` beside the other
module declarations.

In `routes.rs`, add to `guarded_routes()` immediately after the
`/api/library/roots/{id}/scan` entry:

```rust
        // **Spec C4 §4's user-administration routes, PR 2.** Admin-gated for
        // the reason every route in this block is, with one of its own: this
        // is the only writer of `role`, `can_download`, `can_stream` and
        // `user_root` in the product, so a route here that landed in
        // `open_routes` would let anybody grant themselves anything.
        // `users_api::tests::every_users_route_refuses_a_reader` drives all
        // six for a 403.
        (
            "/api/library/users",
            get(crate::users_api::list_users).post(crate::users_api::create_user),
        ),
```

and to the grant-posture table, after the `/api/library/roots/{id}/scan` entry:

```rust
        // Spec C4 §4's user routes, PR 2. Every entry states its own reason
        // rather than inheriting a shared one, the same rule the rules and
        // roots blocks above hold this table to.
        (
            "/api/library/users",
            GrantIndependent {
                reason: "admin-only; lists and creates accounts, and an \
                          account's own root grants are the thing being \
                          edited here rather than a filter to apply to the \
                          answer — filtering this list by the *administrator's* \
                          grant would hide the very accounts being administered",
            },
        ),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-serve users_api::`
Expected: PASS for the list, create and reader-refusal tests. The four routes
Tasks 8 and 9 add are still absent, so
`every_users_route_refuses_a_reader` FAILS on the last three rows — that is
expected and Task 8 closes it. Comment the three unbuilt rows out with a
`// Task 8` marker rather than deleting them.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/users_api.rs crates/taiyaku-serve/src/users_api/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs
git commit -m "GET and POST /api/library/users"
```

---

### Task 8: `users_api.rs` — patch and delete

**Files:**
- Modify: `crates/taiyaku-serve/src/users_api.rs`
- Modify: `crates/taiyaku-serve/src/users_api/tests.rs` (uncomment the two
  `Task 8` rows for `PATCH` and `DELETE`)
- Modify: `crates/taiyaku-serve/src/routes.rs` (both lists)

**Interfaces:**
- Consumes: `account::{Patch, update, removal_cost, remove, set_password}`
  (Tasks 1–3), `UserAnswer`, `assembled` (Task 7).
- Produces:
  ```rust
  pub(crate) async fn patch_user(State, Extension, AxPath<i64>, Json<Value>) -> Response;
  pub(crate) async fn delete_user(State, Extension, AxPath<i64>, Query<DeleteUserQuery>) -> Response;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/users_api/tests.rs`:

```rust
/// The id of the account named `name`, read straight from the database rather
/// than from a route's own answer.
fn user_id(state: &crate::ServeState, name: &str) -> i64 {
    let conn = state.library.conn();
    conn.query_row("SELECT id FROM user WHERE name = ?1", [name], |r| r.get(0))
        .expect("the account exists")
}

/// O54's note stops being true here: this is the first path outside a fixture
/// that can set either capability column to 0.
#[tokio::test]
async fn patching_a_user_sets_the_capability_columns_o54_could_not_reach() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let id = user_id(&state, "reader");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "can_download": false }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let json = get_json(&router, "/api/library/users").await;
    let row = json["users"]
        .as_array()
        .expect("users")
        .iter()
        .find(|u| u["id"] == id)
        .expect("the patched account");
    assert_eq!(row["can_download"], false);
    assert_eq!(row["can_stream"], true, "an unnamed column is left alone");
}

/// PR2 §1.1: an admin's reset ends every session the target has, and leaves
/// its API keys alone. **Both halves**, since a "revoke everything" instinct
/// would break the second.
#[tokio::test]
async fn resetting_a_password_ends_the_targets_sessions_and_keeps_its_keys() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let id = user_id(&state, "reader");
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, "tok", id, None, None)
            .expect("opens a session");
        taiyaku_library::account::create_key(&conn, id, "tablet").expect("mints a key");
    }

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "password": "rotated" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let conn = state.library.conn();
    let sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM session WHERE user_id = ?1", [id], |r| r.get(0))
        .expect("counts");
    let keys: i64 = conn
        .query_row("SELECT COUNT(*) FROM api_key WHERE user_id = ?1", [id], |r| r.get(0))
        .expect("counts");
    assert_eq!(sessions, 0, "every session of the target's is ended");
    assert_eq!(keys, 1, "its API keys are independent credentials and survive");
}

/// C4 §4's two role refusals reach the wire as 409, not 500: the request is
/// well-formed and names a real account, and retrying it unchanged will keep
/// failing for a reason about the library's state.
#[tokio::test]
async fn the_two_role_refusals_answer_409_rather_than_500() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let admin = user_id(&state, "local");

    let demote = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{admin}"),
        &serde_json::json!({ "role": "user" }),
    )
    .await;
    assert_eq!(demote.status(), StatusCode::CONFLICT);

    let removed = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{admin}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(removed.status(), StatusCode::CONFLICT);
}

/// C4 §4: deleting a user states its counts before it happens, on the same
/// `?dry_run=true` shape PR 1 chose for a root. And the deleted account's live
/// session is refused on its next request — the cascade reaching the guard,
/// not just the table.
#[tokio::test]
async fn deleting_a_user_quotes_its_counts_first_and_ends_its_session() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let id = user_id(&state, "reader");
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, "reader-token", id, None, None)
            .expect("opens a session");
    }

    let quoted = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{id}?dry_run=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(quoted.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(quoted).await).expect("json");
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["removed"]["sessions"], 1);
    assert_eq!(body["removed"]["name"], "reader");
    {
        let conn = state.library.conn();
        assert!(
            taiyaku_library::account::by_id(&conn, id).expect("reads").is_some(),
            "a quote deletes nothing"
        );
    }

    let done = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(done.status(), StatusCode::OK);

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::session_owner(&conn, "reader-token")
            .expect("reads")
            .is_none(),
        "the deleted account's session no longer resolves to anybody"
    );
}

/// **`deny_unknown_fields` on the query struct**, which PR 1's whole-branch
/// review found the hard way: without it `?dryrun=true` — one key off — ran
/// the real, committed removal and answered 200.
#[tokio::test]
async fn a_misspelled_dry_run_is_refused_rather_than_deleting() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let id = user_id(&state, "reader");

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{id}?dryrun=true"),
        &serde_json::json!({}),
    )
    .await;
    assert_ne!(response.status(), StatusCode::OK, "a misspelling must not delete");

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::by_id(&conn, id).expect("reads").is_some(),
        "the account survives a misspelled query parameter"
    );
}

/// A `roots` list in the patch body writes the grants, which is PR2 §1.2's
/// whole reason for putting it there rather than on a fifth route.
#[tokio::test]
async fn a_roots_list_in_the_patch_body_writes_the_grants() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root = {
        let conn = state.library.conn();
        conn.query_row("SELECT id FROM root", [], |r| r.get::<_, i64>(0))
            .expect("the fixture's root")
    };
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let id = user_id(&state, "reader");

    send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{id}"),
        &serde_json::json!({ "roots": [root] }),
    )
    .await;

    let conn = state.library.conn();
    assert_eq!(
        taiyaku_library::account::roots_for(&conn, id).expect("reads"),
        vec![root]
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-serve users_api::`
Expected: FAIL — the `PATCH` and `DELETE` paths answer 405 Method Not Allowed.

- [ ] **Step 3: Add `UserAnswer::Refused`**

Task 7 left the variant out so nothing declared a case it could not construct.
This task is what constructs it. In `crates/taiyaku-serve/src/users_api.rs`,
extend the enum and its `IntoResponse`:

```rust
#[derive(Debug)]
enum UserAnswer {
    Missing(i64),
    Unusable(String),
    /// A request that is **well-formed, names a real account and is still not
    /// allowed** — C4 §4's refusals. `409 Conflict` rather than `400`, because
    /// nothing about the request is malformed and retrying it unchanged will
    /// keep failing for a reason about the library's state; and rather than
    /// `500`, because nothing failed.
    Refused(String),
    Failed(String),
}
```

and, in the `match` inside `into_response`, between the `Unusable` and `Failed`
arms:

```rust
            Self::Refused(why) => (StatusCode::CONFLICT, why),
```

- [ ] **Step 4: Write the implementation**

Append to `crates/taiyaku-serve/src/users_api.rs`, before `#[cfg(test)] mod tests;`:

```rust
/// A patch, as [`patch_user`]'s body is read.
///
/// **`roots` is `Option<Vec<i64>>` and the two `None`s mean different
/// things**: an absent key leaves the grants alone, and `[]` grants nothing —
/// which spec A2 defines as *unrestricted*, not blind. `account::Patch`'s own
/// doc comment carries the argument; this function is where JSON's
/// absent/present distinction is turned into it.
///
/// **`password` is not on `account::Patch`** and is applied separately, by
/// [`account::set_password`], which already hashes and already decides what
/// happens to the account's sessions.
fn user_patch_from_json(body: &serde_json::Value) -> Result<(account::Patch, Option<String>), String> {
    let mut patch = account::Patch::default();
    if let Some(v) = body.get("name") {
        patch.name = Some(
            v.as_str()
                .ok_or_else(|| "`name` must be a string".to_string())?
                .to_string(),
        );
    }
    if let Some(v) = body.get("role") {
        patch.role = Some(
            v.as_str()
                .ok_or_else(|| "`role` must be a string".to_string())?
                .to_string(),
        );
    }
    if let Some(v) = body.get("can_download") {
        patch.can_download = Some(v.as_bool().ok_or_else(|| "`can_download` must be a boolean".to_string())?);
    }
    if let Some(v) = body.get("can_stream") {
        patch.can_stream = Some(v.as_bool().ok_or_else(|| "`can_stream` must be a boolean".to_string())?);
    }
    if let Some(v) = body.get("roots") {
        let listed = v
            .as_array()
            .ok_or_else(|| "`roots` must be an array of root ids".to_string())?;
        let mut ids = Vec::with_capacity(listed.len());
        for entry in listed {
            ids.push(
                entry
                    .as_i64()
                    .ok_or_else(|| "`roots` must be an array of root ids".to_string())?,
            );
        }
        patch.roots = Some(ids);
    }
    let password = match body.get("password") {
        Some(v) => Some(
            v.as_str()
                .ok_or_else(|| "`password` must be a string".to_string())?
                .to_string(),
        ),
        None => None,
    };
    Ok((patch, password))
}

/// `PATCH /api/library/users/{id}` — change only what the body names.
///
/// **A `password` here is an administrator resetting somebody else's**, so it
/// reaches [`account::set_password`] with `keep: None` and ends every session
/// the target has. A2 §4's "keeps the session that made it" is a rule about
/// self-service, and there is no session of the *target's* that made this one.
/// PR2 §1.1 records what that beat. API keys are deliberately untouched.
pub(crate) async fn patch_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let (patch, password) = match user_patch_from_json(&body) {
        Ok(read) => read,
        Err(why) => return UserAnswer::Unusable(why).into_response(),
    };
    let acting_as = principal.user_id;

    let updated = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        if account::by_id(&conn, id).map_err(UserAnswer::Failed)?.is_none() {
            return Err(UserAnswer::Missing(id));
        }
        // **The refusals are `Refused`, not `Failed`.** They are the whole
        // reason C4 §4 calls them refusals: the request is well-formed and
        // names a real account, and the reason it cannot happen is about the
        // library's state.
        //
        // **Known trade, and the whole-branch review's to confirm rather than
        // discover:** `account::update` returns `String`, so a genuine SQLite
        // fault inside it lands here as a 409 too. Everything it can refuse
        // *is* one of C4 §4's refusals or the schema's own `CHECK` on `role`,
        // and both are the caller's problem rather than the server's — but a
        // disk error would be reported as a conflict, which is wrong. The fix
        // is a typed error on `update`; it is not taken here because it would
        // change a store signature three tasks after those tasks were
        // reviewed, and the review is where that call belongs.
        let account = account::update(&conn, id, &patch, Some(acting_as))
            .map_err(UserAnswer::Refused)?;
        if let Some(password) = password {
            account::set_password(&conn, id, &password, None).map_err(UserAnswer::Failed)?;
        }
        let can_log_in = account::list(&conn)
            .map_err(UserAnswer::Failed)?
            .into_iter()
            .find(|(a, _)| a.id == id)
            .map(|(_, can)| can)
            .unwrap_or(false);
        assembled(&conn, &account, can_log_in).map_err(UserAnswer::Failed)
    })
    .await;

    match updated {
        Ok(Ok(user)) => Json(serde_json::json!({ "user": user })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("patching an account", &why),
    }
}

/// The one query parameter [`delete_user`] reads.
///
/// **`deny_unknown_fields`, for the reason `config_api::DeleteRootQuery`
/// carries in full**: without it `serde_urlencoded` drops any key this struct
/// does not name rather than refusing the request, so `?dryrun=true` — one
/// key off — answered `200 OK` and ran the real, committed removal on the
/// route PR 1's own review calls the most destructive in the product. This
/// route is the second-most.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteUserQuery {
    #[serde(default)]
    dry_run: bool,
}

/// `DELETE /api/library/users/{id}` — remove an account and everything
/// `ON DELETE CASCADE` takes with it; or, with `?dry_run=true`, find out what
/// that would cost without doing any of it.
///
/// **C4 §4 requires the counts stated before it acts**, and this answers the
/// identical body shape for both requests — `{"removed": {…}, "dry_run": …}`
/// — which is exactly what `config_api::delete_root` does for a root and for
/// the same reason: a plain `DELETE` cannot show a caller anything before
/// removing.
///
/// **The counts a real removal answers with are `account::remove`'s own return
/// value**, counted inside the transaction that deletes. Recomputing them
/// afterwards would answer zero for the numbers this route exists to state
/// honestly.
pub(crate) async fn delete_user(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    axum::extract::Query(query): axum::extract::Query<DeleteUserQuery>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let dry_run = query.dry_run;

    let removed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        if account::by_id(&conn, id).map_err(UserAnswer::Failed)?.is_none() {
            return Err(UserAnswer::Missing(id));
        }
        if dry_run {
            account::removal_cost(&conn, id).map_err(UserAnswer::Failed)
        } else {
            account::remove(&conn, id).map_err(UserAnswer::Refused)
        }
    })
    .await;

    match removed {
        Ok(Ok(removal)) => Json(serde_json::json!({
            "removed": {
                "name": removal.name,
                "positions": removal.positions,
                "notes": removal.notes,
                "sessions": removal.sessions,
                "keys": removal.keys,
                "grants": removal.grants,
            },
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("removing an account", &why),
    }
}
```

In `routes.rs`, add to `guarded_routes()` after the `/api/library/users` entry:

```rust
        (
            "/api/library/users/{id}",
            axum::routing::patch(crate::users_api::patch_user)
                .delete(crate::users_api::delete_user),
        ),
```

and to the grant-posture table after its `/api/library/users` entry:

```rust
        (
            "/api/library/users/{id}",
            GrantIndependent {
                reason: "edits or removes one account, including the root \
                          grants that decide what *that* account may see — a \
                          write about somebody else's access, with no meaning \
                          to filter against the administrator's own",
            },
        ),
```

Uncomment the `PATCH` and `DELETE` rows in
`every_users_route_refuses_a_reader`, leaving the two revocation rows for
Task 9.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-serve users_api::`
Expected: PASS for everything except the two Task 9 rows still commented out.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/users_api.rs crates/taiyaku-serve/src/users_api/tests.rs crates/taiyaku-serve/src/routes.rs
git commit -m "PATCH and DELETE /api/library/users/{id}, with the refusals as 409"
```

---

### Task 9: session and API-key revocation

C4 §4: *"their sessions and their API keys — both revocable individually by an
admin, because 'a device was lost' is the case that matters."*

**Files:**
- Modify: `crates/taiyaku-serve/src/users_api.rs`
- Modify: `crates/taiyaku-serve/src/users_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (both lists)

**Interfaces:**
- Consumes: `account::{revoke_session, revoke_key}`, `UserAnswer` (Task 7).
- Produces:
  ```rust
  pub(crate) async fn revoke_session(State, Extension, AxPath<(i64, String)>) -> Response;
  pub(crate) async fn revoke_key(State, Extension, AxPath<(i64, i64)>) -> Response;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
/// The "a device was lost" case. `account::revoke_session` is already scoped
/// to the account, so an id from one account's listing cannot end another's —
/// asserted here rather than assumed, since the route is what supplies both.
#[tokio::test]
async fn an_admin_revokes_one_session_and_not_another_accounts() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let reader = user_id(&state, "reader");
    let admin = user_id(&state, "local");
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, "reader-token", reader, None, None)
            .expect("opens the reader's session");
        taiyaku_library::account::open_session(&conn, "admin-token", admin, None, None)
            .expect("opens the admin's session");
    }
    let sid = {
        let conn = state.library.conn();
        taiyaku_library::account::sessions_of(&conn, reader, None).expect("lists")[0]
            .id
            .clone()
    };

    // The reader's own session id, offered under the *admin's* path, must not
    // end it: `revoke_session` is scoped to the account in the URL.
    let wrong = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{admin}/sessions/{sid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::NOT_FOUND);

    let right = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{reader}/sessions/{sid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(right.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::session_owner(&conn, "reader-token")
            .expect("reads")
            .is_none(),
        "the revoked session no longer resolves"
    );
    assert!(
        taiyaku_library::account::session_owner(&conn, "admin-token")
            .expect("reads")
            .is_some(),
        "the other account's session is untouched"
    );
}

/// The same shape for an API key, which is the credential an OPDS reader
/// holds — the other half of "a device was lost".
#[tokio::test]
async fn an_admin_revokes_one_api_key() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    send_json(
        &router,
        Method::POST,
        "/api/library/users",
        &serde_json::json!({ "name": "reader", "password": "hunter2" }),
    )
    .await;
    let reader = user_id(&state, "reader");
    let secret = {
        let conn = state.library.conn();
        taiyaku_library::account::create_key(&conn, reader, "tablet").expect("mints a key")
    };
    let kid = {
        let conn = state.library.conn();
        taiyaku_library::account::keys_of(&conn, reader).expect("lists")[0].id
    };

    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{reader}/keys/{kid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::by_api_key(&conn, &secret)
            .expect("reads")
            .is_none(),
        "the revoked key authenticates as nobody"
    );

    let again = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/users/{reader}/keys/{kid}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(again.status(), StatusCode::NOT_FOUND, "revoking twice is a 404");
}
```

Uncomment the two revocation rows in `every_users_route_refuses_a_reader`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-serve users_api::`
Expected: FAIL — both revocation paths answer 405.

- [ ] **Step 3: Write the implementation**

Append to `crates/taiyaku-serve/src/users_api.rs`, before `#[cfg(test)] mod tests;`:

```rust
/// `DELETE /api/library/users/{id}/sessions/{sid}` — C4 §4's "a device was
/// lost", for a browser.
///
/// **`{sid}` is the token's sha256**, which is what `SessionSummary.id`
/// already is and what `account::sessions_of`'s own doc comment argues is safe
/// to hand a browser: "it is what the server compares against, not what a
/// client presents, and inverting it is the sha256 preimage problem."
///
/// **Scoped to `{id}`**, because `account::revoke_session` is: an id from one
/// account's listing must not end another account's session. A session that is
/// not this account's is a 404, the same answer as one that never existed —
/// the route does not confirm that somebody else holds it.
pub(crate) async fn revoke_session(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath((id, sid)): AxPath<(i64, String)>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }

    let revoked = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        account::revoke_session(&conn, id, &sid).map_err(UserAnswer::Failed)
    })
    .await;

    match revoked {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => UserAnswer::Missing(id).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("revoking a session", &why),
    }
}

/// `DELETE /api/library/users/{id}/keys/{kid}` — the same, for the credential
/// an OPDS reader holds.
///
/// Scoped to `{id}` for the reason [`revoke_session`] states.
pub(crate) async fn revoke_key(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath((id, kid)): AxPath<(i64, i64)>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }

    let revoked = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        account::revoke_key(&conn, id, kid).map_err(UserAnswer::Failed)
    })
    .await;

    match revoked {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => UserAnswer::Missing(id).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => user_panicked("revoking an API key", &why),
    }
}
```

In `routes.rs`, add to `guarded_routes()`:

```rust
        (
            "/api/library/users/{id}/sessions/{sid}",
            axum::routing::delete(crate::users_api::revoke_session),
        ),
        (
            "/api/library/users/{id}/keys/{kid}",
            axum::routing::delete(crate::users_api::revoke_key),
        ),
```

and to the grant-posture table:

```rust
        (
            "/api/library/users/{id}/sessions/{sid}",
            GrantIndependent {
                reason: "ends one browser session of one account — C4 §4's \
                          'a device was lost', which is about a credential \
                          rather than about any material a grant could scope",
            },
        ),
        (
            "/api/library/users/{id}/keys/{kid}",
            GrantIndependent {
                reason: "ends one API key of one account, the identical \
                          standing the sessions route above holds, for the \
                          credential an OPDS reader presents instead",
            },
        ),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-serve users_api::`
Expected: PASS, all of them, including
`every_users_route_refuses_a_reader` with all six rows live.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/users_api.rs crates/taiyaku-serve/src/users_api/tests.rs crates/taiyaku-serve/src/routes.rs
git commit -m "an admin revokes one session or one API key"
```

---

### Task 10: `backup_api.rs` — the three routes

**Files:**
- Create: `crates/taiyaku-serve/src/backup_api.rs`
- Create: `crates/taiyaku-serve/src/backup_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod backup_api;`)
- Modify: `crates/taiyaku-serve/src/routes.rs` (both lists)
- Modify: `crates/taiyaku-serve/src/test_support.rs` (`library` at :63, and a
  new file-backed fixture beside `router_over_a_scanned_tree` at :696)

**Verified before this task was written, 2026-09-04: every fixture in
`test_support.rs` is in-memory.** `test_support::library` opens
`Library::open_in_memory`, so `Library::path()` answers `None` and every backup
route would answer 409 against every existing fixture. Step 1 adds a
file-backed one; without it the tests below assert the refusal instead of the
feature.

**Interfaces:**
- Consumes: `taiyaku_library::backup::{Backup, directory_for, list, take}`,
  `Library::path`, `Library::working`, `config_api::admin_or_forbidden`.
- Produces:
  ```rust
  pub(crate) async fn list_backups(State, Extension) -> Response;
  pub(crate) async fn create_backup(State, Extension) -> Response;
  pub(crate) async fn download_backup(State, Extension, AxPath<String>) -> Response;
  ```

- [ ] **Step 1: Add a file-backed fixture to `test_support.rs`**

Extract the seeding `library()` already does, so two fixtures share one copy of
it rather than drifting. Replace the body of `library()` (`test_support.rs:63`)
with a call, and add the two functions beside it:

```rust
pub(crate) fn library() -> std::sync::Arc<taiyaku_library::Library> {
    let library = taiyaku_library::Library::open_in_memory().expect("in-memory library");
    seed_fixture_account(&library);
    std::sync::Arc::new(library)
}

/// Migration 4 leaves the seeded account locked, which is correct for a real
/// fresh library and useless for a fixture: every route is behind the guard
/// since spec A2 and there is no bypass (§9), so a fixture library nobody can
/// authenticate against is a fixture no route test can use.
///
/// **Extracted from [`library`] rather than copied into the file-backed
/// fixture below**, which is the ordinary reason: two copies of a fixture's
/// credential are two things to forget to change together.
fn seed_fixture_account(library: &taiyaku_library::Library) {
    let conn = library.conn();
    conn.execute(
        "UPDATE user SET password_hash = ?1 WHERE id = 1",
        [FIXTURE_PHC],
    )
    .expect("gives the fixture account a password");
    conn.execute(
        "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'the suite', ?1)",
        [taiyaku_library::credential::token_sha256(FIXTURE_KEY)],
    )
    .expect("gives the fixture account a key");
}

/// A router over a library **that is a file**, which every backup test needs:
/// `Library::path` answers `None` for `:memory:` and `backup_api` refuses with
/// 409 when it does — correctly, since there is nothing to copy.
///
/// **Three directories inside one `Fixture`, rather than siblings of it.**
/// The scan root is `tree/`, the database is `db/library.db` and covers go in
/// `covers/`, so the database and the `backups/` directory beside it are
/// outside the root — anything under a root is material the next scan walks —
/// while all three still go away with the one `Drop`.
/// [`router_over_a_scanned_tree`] puts its `cache_dir` beside the fixture with
/// `with_file_name` instead, which leaks it; that is not copied here, and the
/// 9,147 leaked directories `Fixture`'s own comment records are why.
///
/// # Panics
///
/// Panics if the directories cannot be created or the library cannot be
/// opened — a fixture that silently fell back to memory would make every test
/// below assert the 409 refusal rather than the feature.
pub(crate) fn router_over_a_file_backed_library()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("taiyaku-test-file-backed-library");
    let tree = fixture.path().join("tree");
    let db_dir = fixture.path().join("db");
    write_pages(&tree.join("Scanned").join("Scanned - v01"), 2);
    std::fs::create_dir_all(&db_dir).expect("creates the database directory");

    let library = taiyaku_library::Library::open(&db_dir.join("library.db"))
        .expect("opens a file-backed library");
    seed_fixture_account(&library);
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &tree).expect("adds the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let library = std::sync::Arc::new(library);
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        cache_dir: fixture.path().join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()));
    (router, state, fixture)
}

/// [`router_over_a_file_backed_library`], with the fixture account demoted to
/// a reader — the same demotion `router_over_a_single_scanned_work_as_a_reader`
/// applies, and for the same reason.
///
/// # Panics
///
/// Panics if the demotion updates no row.
pub(crate) fn router_over_a_file_backed_library_as_a_reader()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let (router, state, fixture) = router_over_a_file_backed_library();
    {
        let conn = state.library.conn();
        let demoted = conn
            .execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("demotes the fixture account");
        assert_eq!(demoted, 1, "the fixture account must actually stop being an administrator");
    }
    (router, state, fixture)
}
```

`Library::open` takes `&Path` (`lib.rs:302`), which is why the call passes a
reference to the joined path rather than the `PathBuf` itself.

Run `cargo nextest run -p taiyaku-serve` and confirm the existing suite is
still green after the `library()` extraction, **before** writing any backup
test: that refactor touches the fixture every route test in the crate uses.

- [ ] **Step 2: Write the failing tests**

Create `crates/taiyaku-serve/src/backup_api/tests.rs`:

```rust
use crate::test_support::{self, get_json, send_json};
use axum::http::{Method, StatusCode};

/// Take one, list it, download it, and open what came back as a library —
/// the whole round trip, because "the response was 200" would pass over a
/// body of zero bytes.
#[tokio::test]
async fn a_backup_is_taken_listed_and_downloaded_whole() {
    let (router, state, _fixture) = test_support::router_over_a_file_backed_library();

    let empty = get_json(&router, "/api/library/backups").await;
    assert_eq!(
        empty["backups"].as_array().expect("an array").len(),
        0,
        "a library nobody has backed up lists nothing rather than erroring"
    );

    let taken = send_json(&router, Method::POST, "/api/library/backups", &serde_json::json!({})).await;
    assert_eq!(taken.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&test_support::body_string(taken).await).expect("json");
    let name = body["backup"]["name"].as_str().expect("a name").to_string();
    assert!(body["backup"]["bytes"].as_u64().expect("bytes") > 0, "{body}");

    let listed = get_json(&router, "/api/library/backups").await;
    assert_eq!(listed["backups"].as_array().expect("an array").len(), 1);

    let response = test_support::get(&router, &format!("/api/library/backups/{name}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("reads the body");

    let db = state.library.path().expect("a file-backed library").to_path_buf();
    let restored_path = taiyaku_library::backup::directory_for(&db).join("downloaded.db");
    std::fs::write(&restored_path, &bytes).expect("writes what was downloaded");
    let restored = taiyaku_library::Library::open(&restored_path).expect("it opens as a library");
    let works: i64 = restored
        .conn()
        .query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))
        .expect("reads");
    assert!(works > 0, "the downloaded bytes are the library, not an empty file");
}

/// **The download route resolves `{name}` against the listing and never joins
/// a caller's string onto the directory.** A traversal attempt is therefore a
/// 404 — "no such backup" — rather than something a path check has to catch.
#[tokio::test]
async fn a_name_that_is_not_in_the_listing_is_not_found() {
    let (router, _state, _fixture) = test_support::router_over_a_file_backed_library();
    send_json(&router, Method::POST, "/api/library/backups", &serde_json::json!({})).await;

    for name in [
        "library-19700101T000000Z.db",
        "..%2F..%2Fetc%2Fpasswd",
        "library.db",
    ] {
        let response = test_support::get(&router, &format!("/api/library/backups/{name}")).await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "`{name}` answered something other than 404"
        );
    }
}

/// Every backup route is an administrator's, for the reason C4 §5's own screen
/// is: the file is the whole database, password hashes included.
#[tokio::test]
async fn every_backup_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_file_backed_library_as_a_reader();
    let body = serde_json::json!({});

    for (method, path) in [
        (Method::GET, "/api/library/backups"),
        (Method::POST, "/api/library/backups"),
        (Method::GET, "/api/library/backups/library-19700101T000000Z.db"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo nextest run -p taiyaku-serve backup_api::`
Expected: FAIL — `file not found for module backup_api`.

- [ ] **Step 4: Write the implementation**

Create `crates/taiyaku-serve/src/backup_api.rs`:

```rust
//! The `/api/library/backups*` handlers — spec C4 §5.
//!
//! **A backup is the whole database, password hashes included**, and that is
//! stated here rather than left to be found: an administrator who downloads
//! one can read every account's PHC string offline at their leisure. That is
//! what a backup *is* — the same file an operator would copy off the disk —
//! and every route in this module is admin-gated on that basis. It is worth
//! naming because "the admin screen has a download button" does not, on its
//! face, look like "the admin screen hands out the password file".
//!
//! **No restore route.** C4 §5 puts restore deliberately out of band — stop
//! the server, move the file, start it — because a restore through a running
//! server would have to quiesce every writer in the process, and the value of
//! that over "stop the server" is not worth the failure modes.
//!
//! **No schedule.** The C4 build design §1 declines an in-server timer; a cron
//! entry runs `taiyaku library backup`.

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path as AxPath, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use taiyaku_library::backup::{self, Backup};

use crate::{ServeState, auth::Principal, config_api::admin_or_forbidden};

/// Where this server's backups live, or the refusal explaining that this
/// library has no file to sit beside.
///
/// **An in-memory library has no backup directory**, and that is a real state
/// rather than a test-only one: `Library::path` answers `None` for `:memory:`,
/// which is what several of this crate's own fixtures open.
fn directory(state: &ServeState) -> Result<std::path::PathBuf, Response> {
    match state.library.path() {
        Some(db) => Ok(backup::directory_for(db)),
        None => Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "this library is in memory and has no file to back up"
            })),
        )
            .into_response()),
    }
}

fn backup_json(entry: &Backup) -> serde_json::Value {
    serde_json::json!({
        "name": entry.name,
        "bytes": entry.bytes,
        "taken": entry.taken,
    })
}

fn failed(what: &str, why: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": format!("{what}: {why}") })),
    )
        .into_response()
}

/// `GET /api/library/backups` — what exists, newest first.
pub(crate) async fn list_backups(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let dir = match directory(&state) {
        Ok(dir) => dir,
        Err(refusal) => return refusal,
    };

    match tokio::task::spawn_blocking(move || backup::list(&dir)).await {
        Ok(Ok(found)) => Json(serde_json::json!({
            "backups": found.iter().map(backup_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Ok(Err(why)) => failed("listing backups", &why),
        Err(why) => failed("listing backups panicked", &why.to_string()),
    }
}

/// `POST /api/library/backups` — C4 §5's "run now".
///
/// **On `Library::working()`, not the server's own connection**, for the
/// reason the rule preview and the scan both take a second one: a
/// `VACUUM INTO` on the process's single mutexed connection stalls every
/// request for the length of a file copy.
pub(crate) async fn create_backup(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let dir = match directory(&state) {
        Ok(dir) => dir,
        Err(refusal) => return refusal,
    };

    let taken = tokio::task::spawn_blocking(move || {
        let working = state.library.working()?;
        backup::take(&working, &dir)
    })
    .await;

    match taken {
        Ok(Ok(entry)) => Json(serde_json::json!({ "backup": backup_json(&entry) })).into_response(),
        Ok(Err(why)) => failed("taking a backup", &why),
        Err(why) => failed("taking a backup panicked", &why.to_string()),
    }
}

/// `GET /api/library/backups/{name}` — download one.
///
/// **`name` is resolved against [`backup::list`]'s own answer and is never
/// joined onto the directory.** A path-traversal check on a filename is a
/// thing that gets written wrong; matching a requested name against a listing
/// the server produced has no such failure mode, and the `readdir` it costs is
/// one this route wants anyway in order to answer 404 honestly.
///
/// Read whole into memory rather than streamed: the file is a SQLite database
/// an administrator asked for, and the alternative — a streaming body over a
/// file handle — buys nothing until somebody has measured a library big enough
/// for it to matter. Recorded rather than assumed.
pub(crate) async fn download_backup(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(name): AxPath<String>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let dir = match directory(&state) {
        Ok(dir) => dir,
        Err(refusal) => return refusal,
    };

    let read = tokio::task::spawn_blocking(move || {
        let found = backup::list(&dir)?;
        let Some(entry) = found.into_iter().find(|b| b.name == name) else {
            return Ok(None);
        };
        std::fs::read(dir.join(&entry.name))
            .map(|bytes| Some((entry, bytes)))
            .map_err(|e| format!("reading the backup {}: {e}", entry.name))
    })
    .await;

    match read {
        Ok(Ok(Some((entry, bytes)))) => (
            [
                (header::CONTENT_TYPE, "application/vnd.sqlite3".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", entry.name),
                ),
            ],
            bytes,
        )
            .into_response(),
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "there is no such backup" })),
        )
            .into_response(),
        Ok(Err(why)) => failed("reading a backup", &why),
        Err(why) => failed("reading a backup panicked", &why.to_string()),
    }
}

#[cfg(test)]
mod tests;
```

Add `mod backup_api;` to `crates/taiyaku-serve/src/lib.rs`.

In `routes.rs`, add to `guarded_routes()`:

```rust
        // **Spec C4 §5's backup routes, PR 2.** Admin-gated with a reason of
        // their own on top of the block's: a backup is the whole database,
        // password hashes included, so the download route hands an
        // administrator every account's PHC string. There is no restore route
        // — C4 §5 puts restore out of band on purpose.
        (
            "/api/library/backups",
            get(crate::backup_api::list_backups).post(crate::backup_api::create_backup),
        ),
        (
            "/api/library/backups/{name}",
            get(crate::backup_api::download_backup),
        ),
```

and to the grant-posture table:

```rust
        (
            "/api/library/backups",
            GrantIndependent {
                reason: "admin-only; lists and takes snapshots of the whole \
                          database, which is not any one account's material \
                          and has no per-root shape to filter",
            },
        ),
        (
            "/api/library/backups/{name}",
            GrantIndependent {
                reason: "hands back one whole-database snapshot — password \
                          hashes and every root's rows together, which is \
                          precisely why it is admin-only rather than filtered",
            },
        ),
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo nextest run -p taiyaku-serve backup_api::`
Expected: PASS, three tests.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/backup_api.rs crates/taiyaku-serve/src/backup_api/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/test_support.rs
git commit -m "the three /api/library/backups routes"
```

---

### Task 11: the docs sweep, the measurement, and the gate

**Files:**
- Modify: `docs/status.md` (both test-count sites, O54's prose, the route prose,
  the command synopsis, "What remains")
- Modify: `docs/notes.md` (the `VACUUM INTO` measurement)
- Modify: `README.md` (if Task 6 did not already)

- [ ] **Step 1: Run the open-items sweep on the way in**

```bash
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -nE '[0-9]{4} tests|Nine.* of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

Record the four numbers before touching anything. The count word wraps across a
line break, so `grep "forty-seven open items"` finds nothing and reads as
"already updated" when it is not — grep the sentence opener and take the next
line.

- [ ] **Step 2: Measure `VACUUM INTO` against a copy of the real library**

**Never against the real library itself.** Copy it first:

```bash
REAL="$HOME/.local/share/taiyaku/library.db"
WORK="/tmp/claude-1000/-home-davey-projects-taiyaku/d8a8c5c4-7fad-4d1e-ae36-b121367bb796/scratchpad/vacuum-measure"
mkdir -p "${WORK:?}" && cp "$REAL" "${WORK:?}/library.db"
cargo build --release -p taiyaku-cli
for i in 1 2 3 4 5; do
  rm -rf "${WORK:?}/backups"
  /usr/bin/time -f "%e" ./target/release/taiyaku library backup --db "${WORK:?}/library.db"
done
ls -l "${WORK:?}/backups"
```

Record the five timings, the database's size and the backup's size in
`docs/notes.md` under a new heading **"What a `VACUUM INTO` of the real library
costs"**, beside the rule-preview figures. Then say in the PR 2 design's §10
whether the `POST` route staying synchronous is still the right call, and edit
§4's "unmeasured" paragraph in place rather than deleting it — this repo marks
corrections rather than rewriting them.

- [ ] **Step 3: Edit `docs/status.md`**

Five parts, and prose outside the table as well:

1. **Both test-count sites.** Run
   `cargo nextest run --workspace 2>&1 | tail -3` for the real figure and
   replace `Six crates, 1939 tests` and `Nineteen of the 1939 are standing
   checks` with it.
2. **O54's prose.** It says the two capability columns "cannot be set to 0
   outside a fixture until spec C4's user administration exists". That stops
   being true. **Do not rewrite the closed item's prose** — closed items are
   left in the present tense of when they closed. Append a marked note in the
   house form: `*C4 PR 2 shipped that writer — `PATCH /api/library/users/{id}`
   — 2026-09-0X.*`
3. **The route prose** — the paragraph listing what `/api/library/*` covers
   gains users and backups.
4. **The command synopsis block** — `taiyaku library backup` joins it, matching
   `main.rs` exactly.
5. **Grep the bare O-numbers** across `docs/` and `web/src/` for O54, O58 and
   O60 and read every hit. O58 and O60 do **not** close; if any prose implies
   C4 will close them, it needs the correction the PR 2 design §7 states.

- [ ] **Step 4: Run the gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: the script's own `all green` line. Quote it verbatim in the commit or
the PR body — piping through `tail` or appending `; echo` masks the exit
status.

- [ ] **Step 5: Run the corpus job separately**

```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

Expected: ~85 s, twelve standing checks, all passing.

- [ ] **Step 6: Run the open-items sweep on the way out**

The same four greps from Step 1. The row count and the closed count must agree
with the preamble's count word and with the "What remains" line.

- [ ] **Step 7: Commit and open the PR**

```bash
git add docs/status.md docs/notes.md README.md docs/superpowers/specs/2026-09-04-c4-pr2-users-and-backup-design.md
git commit -m "the docs sweep, and what a VACUUM INTO of the real library costs"
git push -u origin c4-pr2-users-and-backup
```

Open the PR through the **gitea** MCP, not `gh` — `gh` is authenticated against
the wrong host. Never hard-wrap the PR body: the remote is squash-only and the
body becomes the single commit message verbatim.

---

## Whole-branch review, before the PR is merged

**Per-task review passed on every task of #91 and #105 while whole-branch review
found seven Criticals and two respectively.** Do not skip it, and **review the
fixes too**. The three questions this branch's shape makes most likely to
matter:

1. **Is any account invariant enforced in only one of the two writers?** The
   CLI and the routes both write accounts. `promote`, `update` and `remove` are
   three faces of one rule; a fourth path that skips it is the defect this
   branch's whole design is arranged against.
2. **Can `prune` be talked into deleting something that is not a backup?** It
   is the only file-deleting function in the store. Re-read `stamp_of` against
   the names `take` actually mints, and check that `list` — not a glob — is
   what feeds it.
3. **Does any route return a password hash, a raw token or a raw API-key
   secret?** `Account` carries no hash by construction and `SessionSummary.id`
   is a sha256, but `assembled` renders three types into one body and the
   review is where that gets checked rather than assumed.
4. **Should `account::update` and `account::remove` return a typed error?**
   Task 8 maps every `Err` from both to `409 Conflict`, so a genuine SQLite
   fault would be reported as a conflict. Everything they can currently refuse
   really is the caller's problem, which is why the plan takes the trade — but
   it is a trade, it is written down at the call site, and this is the round
   that decides whether to keep it.

Then a mutation sweep, per the harness's own rules: **after a rule change,
sweep what the rule governs, not what the diff touched** — `--since main` reads
`git diff main...HEAD` and cannot see a guard whose file did not change. Commit
the fixes before re-sweeping or it re-reports them. An *errored* run's verdict
is fiction in both directions; check the unviable count against what you
changed.

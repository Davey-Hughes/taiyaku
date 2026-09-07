# C3 §6 PR A — the ComicInfo write-back, server half: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the server half of the ComicInfo write-back — the `can_stamp`
capability, `taiyaku-library`'s `stamp` module, and the two routes — so that
an account holding `can_stamp` can plan and perform a per-work stamp over
`HTTP`, with no screen yet.

**Architecture:** `taiyaku-library/src/stamp.rs` owns the whole operation:
plan (read each zip's existing `ComicInfo.xml` and keep only the archives
whose content would differ) → rewrite one archive at a time into a sibling
temp and rename → one SQLite transaction for the member rows and
`content_key` → move the hand document and re-export the sidecar.
`taiyaku-serve/src/stamp.rs` is two thin routes over it. `comicinfo.rs` moves
down from `taiyaku-serve` to `taiyaku-library` so the store owns the XML it
writes.

**Tech Stack:** Rust, `rusqlite` 0.38, `zip` 7.2.0, `axum`, `sha2`.

The design is
[`docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md`](../specs/2026-09-07-comicinfo-write-back-design.md).
Section references below (§1.1, §4, …) name it.

## Global Constraints

- **`can_stamp` defaults to 0 and is NOT dominated by `role`.** Every other
  capability reads `admin || column` in `Principal::from_account`; this one
  reads the column alone. §1.1. This is the single exception to O86 and it
  must be pinned by a test, not by a comment.
- **`rar` and `7z` are never written.** §1.2.
- **Never bulk.** One work per call. §7.
- **Pages must be byte-identical after a rewrite** — `raw_copy_file`, never
  a re-encode, never `STORE`. §4.
- **Append migrations, never edit one.** `schema.rs`'s own doc comment.
- **The gate is `scripts/check-all.sh`** and the line to quote is `all
  green`. Piping it through `tail` or appending `; echo` masks the exit
  status.
- **House documentation style:** every decision records the alternative it
  beat; a figure was measured on this machine or says it is a guess.
- **Each test below gets its defect hand-applied and watched to fail** before
  the task is offered. A test that passes against the defect it names is this
  project's most-recorded failure.
- Rust edition and lint config are the workspace's; `#[expect]`, never
  `#[allow]`, for a lint that should come off on a known day.

---

### Task 1: Migration 013 and `can_stamp` on the account row

**Files:**
- Create: `crates/taiyaku-library/src/schema/013_can_stamp.sql`
- Modify: `crates/taiyaku-library/src/schema.rs:28` and `:36-49`
- Modify: `crates/taiyaku-library/src/account.rs` — struct at `:24`,
  `account_from_row` at `:32`, the five `SELECT` sites at `:49`, `:85`,
  `:329`, `:685`, `:1089`, `Patch` at `:717`, `update`'s set-builder at
  `:862`
- Test: `crates/taiyaku-library/src/schema/tests.rs`,
  `crates/taiyaku-library/src/account/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `taiyaku_library::account::Account.can_stamp: bool`,
  `taiyaku_library::account::Patch.can_stamp: Option<bool>`,
  `taiyaku_library::schema::SCHEMA_VERSION == 13`.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/schema/tests.rs`:

```rust
#[test]
fn migration_13_gives_every_existing_account_can_stamp_off() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::schema::migrate(&conn).unwrap();

    // Migration 1 seeds `(1, 'local', 'admin')`. It is an administrator, and
    // it must still not be able to stamp: §1.1's default is 0 for everybody
    // including the account that applies the migration.
    let can_stamp: i64 = conn
        .query_row("SELECT can_stamp FROM user WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(can_stamp, 0, "the seeded administrator may not stamp");
}
```

In `crates/taiyaku-library/src/account/tests.rs`:

```rust
/// Every resolver reads the column, not just the one the patch test drives.
///
/// `account.rs` holds five separate `SELECT` lists naming the capability
/// columns — `by_name`, the API-key resolver, the session resolver, `by_id`
/// and the password-check read. Adding a column to four of them leaves the
/// fifth reporting `can_stamp = false` for an account that holds it, and
/// `account_from_row` reads by name so the miss is a runtime error in one
/// code path rather than a compile error anywhere.
#[test]
fn every_resolver_reports_can_stamp() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::schema::migrate(&conn).unwrap();
    conn.execute(
        "UPDATE user SET can_stamp = 1, password_hash = 'x' WHERE id = 1",
        [],
    )
    .unwrap();

    let (by_name, _) = crate::account::by_name(&conn, "local").unwrap().unwrap();
    assert!(by_name.can_stamp, "by_name");

    let by_id = crate::account::by_id(&conn, 1).unwrap().unwrap();
    assert!(by_id.can_stamp, "by_id");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library can_stamp`
Expected: FAIL — `no such column: can_stamp`.

- [ ] **Step 3: Write the migration**

Create `crates/taiyaku-library/src/schema/013_can_stamp.sql`:

```sql
-- Spec C3 §6's capability: may this account rewrite the archives of a work?
-- See docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md §1.1.
--
-- **DEFAULT 0, where `can_download` and `can_stream` default to 1.** Those two
-- widen what an account may read and every existing account already did both,
-- so defaulting them on changed nothing the day migration 4 applied. This one
-- decides whether an account may rewrite files taiyaku does not own, and there
-- is no account anywhere that was already doing that. Defaulting it on would
-- grant the whole library's administrators a power none of them asked for, on
-- an upgrade none of them read.
--
-- **The seeded administrator (id 1, from migration 1) gets 0 like everyone
-- else.** `Principal::from_account` does NOT fold this column into the admin
-- role the way it folds the other three — that is the one exception to O86,
-- and this default is the half of it that lives in the schema.
ALTER TABLE user ADD COLUMN can_stamp INTEGER NOT NULL DEFAULT 0;
```

- [ ] **Step 4: Register the migration**

In `crates/taiyaku-library/src/schema.rs`, change line 28 and the array:

```rust
pub const SCHEMA_VERSION: u32 = 13;
```

```rust
const MIGRATIONS: [&str; 13] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
    include_str!("schema/005_note_id_per_user.sql"),
    include_str!("schema/006_member_root.sql"),
    include_str!("schema/007_backfill_member_root.sql"),
    include_str!("schema/008_trigram_fts.sql"),
    include_str!("schema/009_progress_per_edition.sql"),
    include_str!("schema/010_hand_metadata.sql"),
    include_str!("schema/011_hand_provenance.sql"),
    include_str!("schema/012_root_keeps_documents.sql"),
    include_str!("schema/013_can_stamp.sql"),
];
```

- [ ] **Step 5: Carry the column through `account.rs`**

Struct at `:24`:

```rust
pub struct Account {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub can_download: bool,
    pub can_stream: bool,
    pub can_stamp: bool,
}
```

`account_from_row` at `:32`:

```rust
fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get("id")?,
        name: row.get("name")?,
        role: row.get("role")?,
        can_download: row.get::<_, i64>("can_download")? != 0,
        can_stream: row.get::<_, i64>("can_stream")? != 0,
        can_stamp: row.get::<_, i64>("can_stamp")? != 0,
    })
}
```

Add `can_stamp` to the column list of **all five** `SELECT`s — at `:49`,
`:85`, `:329`, `:685` and `:1089`. The three that qualify with `u.` need
`u.can_stamp`. Search for the string `can_stream` in this file and change
every line that names it in SQL; there must be no `SELECT` naming
`can_stream` without `can_stamp` beside it when this step is done.

`Patch` at `:717`:

```rust
pub struct Patch {
    pub name: Option<String>,
    pub role: Option<String>,
    pub can_download: Option<bool>,
    pub can_stream: Option<bool>,
    pub can_stamp: Option<bool>,
    pub roots: Option<Vec<i64>>,
    pub password: Option<String>,
}
```

`update`'s set-builder, after the `can_stream` block at `:869`:

```rust
    if let Some(v) = patch.can_stamp {
        values.push(Box::new(i64::from(v)));
        sets.push(format!("can_stamp = ?{}", values.len()));
    }
```

Every other construction of `Account` or `Patch` in the workspace now fails
to compile; add `can_stamp: false` / `can_stamp: None` at each. That compile
error is the point — it is what makes the fifth `SELECT` the only site the
compiler cannot find for you, which is why Step 1's test exists.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library can_stamp`
Expected: PASS, both tests.

Then: `cargo test -p taiyaku-library`
Expected: PASS — no existing test regressed by the schema bump.

- [ ] **Step 7: Hand-apply the defect and watch it fail**

Remove `can_stamp` from the `SELECT` at `:685` (`by_id`) only. Run
`cargo test -p taiyaku-library every_resolver_reports_can_stamp`. It MUST
fail. Restore it.

If it passes, the test is not reaching `by_id` and must be fixed before the
task is done.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/schema/013_can_stamp.sql \
        crates/taiyaku-library/src/schema.rs \
        crates/taiyaku-library/src/schema/tests.rs \
        crates/taiyaku-library/src/account.rs \
        crates/taiyaku-library/src/account/tests.rs
git commit -m "migration 013: can_stamp, default 0 for every account"
```

---

### Task 2: `Principal.can_stamp`, the O86 exception, and the refusal

**Files:**
- Modify: `crates/taiyaku-serve/src/auth.rs` — `Principal` at `:268`,
  `from_account` at `:337`
- Modify: `crates/taiyaku-serve/src/routes.rs` — after
  `refuse_if_cannot_stream` at `:512`
- Test: `crates/taiyaku-serve/src/auth/tests.rs`

**Interfaces:**
- Consumes: `taiyaku_library::account::Account.can_stamp` (Task 1).
- Produces: `crate::auth::Principal.can_stamp: bool`;
  `crate::routes::refuse_if_cannot_stamp(&axum::http::Extensions) ->
  Option<Response>`.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/auth/tests.rs`:

```rust
/// **The one exception to O86, pinned here rather than asserted in a
/// comment.**
///
/// `Principal::from_account` folds `can_download`, `can_stream` and `roots`
/// into the admin role — an administrator is unrestricted in all three
/// whatever the columns say. `can_stamp` is not folded, because O86's
/// argument is about an admin editing shared rows they cannot see and no
/// half-state follows from withholding a per-work file rewrite. Spec §1.1.
///
/// A comment claiming a guard that does not exist is O95's third-pass shape
/// and #111's second finding. This test is what stops a third instance.
#[test]
fn an_administrator_does_not_inherit_can_stamp() {
    let account = taiyaku_library::account::Account {
        id: 1,
        name: "root".into(),
        role: "admin".into(),
        can_download: false,
        can_stream: false,
        can_stamp: false,
    };

    let principal = crate::auth::Principal::from_account(&account, vec![]);

    // The other three ARE inherited — asserted here so that a future change
    // folding can_stamp in alongside them fails on the line that says why.
    assert!(principal.can_download, "O86: role dominates can_download");
    assert!(principal.can_stream, "O86: role dominates can_stream");
    assert!(
        !principal.can_stamp,
        "an admin holding can_stamp = 0 may not stamp"
    );
}

#[test]
fn a_granted_administrator_may_stamp() {
    let account = taiyaku_library::account::Account {
        id: 1,
        name: "root".into(),
        role: "admin".into(),
        can_download: true,
        can_stream: true,
        can_stamp: true,
    };
    assert!(crate::auth::Principal::from_account(&account, vec![]).can_stamp);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-serve can_stamp`
Expected: FAIL to compile — `Principal` has no field `can_stamp`.

- [ ] **Step 3: Add the field and the exception**

In `crates/taiyaku-serve/src/auth.rs`, add to `Principal`:

```rust
    /// **Not dominated by `role`, and the only field here that is not.**
    ///
    /// [`Principal::from_account`] folds `can_download`, `can_stream` and
    /// `roots` into the admin role per O86 — an administrator is
    /// unrestricted in every dimension the schema has. This column is the
    /// exception, and the reason is in O86's own argument rather than
    /// against it: O86 exists because an administrator edits rows no grant
    /// can scope (`series_title` is keyed `(series_id, title)` with no
    /// per-work column), so a half-sighted admin could rename a series for
    /// the whole library while refreshing only the works their grant shows.
    /// **Every capability O86 dominates widens what an account may read.**
    /// This one decides whether an account may rewrite archives taiyaku does
    /// not own, and withholding it leaves no incoherent half-state, because
    /// a stamp is per work and either happens or does not.
    ///
    /// Spec C3 §6/§7 and
    /// `docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md`
    /// §1.1. Pinned by `auth::tests::an_administrator_does_not_inherit
    /// _can_stamp`, so this comment cannot drift from the code the way
    /// O95's third pass found twice.
    pub can_stamp: bool,
```

In `from_account`:

```rust
        Self {
            user_id: account.id,
            role,
            can_download: admin || account.can_download,
            can_stream: admin || account.can_stream,
            // No `admin ||`. See the field's doc comment.
            can_stamp: account.can_stamp,
            roots: if admin { Vec::new() } else { roots },
        }
```

- [ ] **Step 4: Add the refusal**

In `crates/taiyaku-serve/src/routes.rs`, after `refuse_if_cannot_stream`:

```rust
/// The same for rewriting a work's archives — spec C3 §6.
///
/// **403, and unreachable by an administrator who was not granted it.**
/// `Principal.can_stamp` is the column rather than `admin || column`, which
/// is the whole of §1.1's exception to O86; this function does not restate
/// that rule, it just reads the field that holds it.
///
/// Defaults to 0 (`schema/013_can_stamp.sql`), so no account in any existing
/// library gained this on the upgrade that added the column.
pub(crate) fn refuse_if_cannot_stamp(extensions: &axum::http::Extensions) -> Option<Response> {
    let principal = extensions.get::<crate::auth::Principal>()?;
    (!principal.can_stamp).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may edit metadata but not rewrite archives",
        )
            .into_response()
    })
}
```

Mark it `#[cfg_attr(not(test), expect(dead_code, reason = "the routes that \
read it land in Task 8 of this branch's plan"))]` **only if** the build
complains before Task 8, and delete the attribute in Task 8. `#[expect]`
fails the build when the lint does not fire, so it cannot linger — the choice
spec A2 made for `Principal.roots`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve can_stamp`
Expected: PASS, both tests.

- [ ] **Step 6: Hand-apply the defect and watch it fail**

Change `can_stamp: account.can_stamp` to `can_stamp: admin || account.can_stamp`.
Run `cargo test -p taiyaku-serve an_administrator_does_not_inherit_can_stamp`.
It MUST fail. Revert.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/auth.rs \
        crates/taiyaku-serve/src/auth/tests.rs \
        crates/taiyaku-serve/src/routes.rs
git commit -m "can_stamp on Principal: the one column role does not dominate"
```

---

### Task 3: `/api/me` and `PATCH /api/library/users/{id}` carry `can_stamp`

**Files:**
- Modify: `crates/taiyaku-serve/src/session.rs:997`
- Modify: `crates/taiyaku-serve/src/users_api.rs:121` and `:307`
- Test: `crates/taiyaku-serve/src/users_api/tests.rs`

**Interfaces:**
- Consumes: `Principal.can_stamp` (Task 2), `account::Patch.can_stamp`
  (Task 1).
- Produces: the JSON key `can_stamp` on `/api/me` and on each row of
  `GET /api/library/users`; `PATCH` accepting `{"can_stamp": bool}`.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/users_api/tests.rs`, following the shape of the
existing `can_download` patch test:

```rust
#[tokio::test]
async fn patch_sets_can_stamp() {
    let (router, state) = crate::test_support::router_over_a_file_backed_library();
    let id = crate::users_api::tests::a_second_user(&state);

    let response = crate::users_api::tests::patch(&router, id, r#"{"can_stamp": true}"#).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let conn = state.library.connection().unwrap();
    let account = taiyaku_library::account::by_id(&conn, id).unwrap().unwrap();
    assert!(account.can_stamp, "the column was written");
}

#[tokio::test]
async fn patch_refuses_a_non_boolean_can_stamp() {
    let (router, state) = crate::test_support::router_over_a_file_backed_library();
    let id = crate::users_api::tests::a_second_user(&state);

    let response = crate::users_api::tests::patch(&router, id, r#"{"can_stamp": "yes"}"#).await;
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}
```

If `a_second_user` and `patch` do not exist under those names in this file,
read the existing `can_download` patch test at `users_api/tests.rs` and reuse
whatever helpers it uses verbatim — do not invent new ones.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve can_stamp`
Expected: FAIL — the patch is accepted but the column stays 0.

- [ ] **Step 3: Report it and accept it**

`session.rs:997`, in the `/api/me` body:

```rust
    "can_stamp": principal.can_stamp,
```

`users_api.rs:121`, in each user row:

```rust
        "can_stamp": account.can_stamp,
```

`users_api.rs`, after the `can_stream` arm at `:313`:

```rust
    if let Some(v) = body.get("can_stamp") {
        patch.can_stamp = Some(
            v.as_bool()
                .ok_or_else(|| "`can_stamp` must be a boolean".to_string())?,
        );
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p taiyaku-serve can_stamp`
Expected: PASS.

- [ ] **Step 5: Hand-apply the defect and watch it fail**

Delete the `patch.can_stamp` block from `users_api.rs`. Run
`cargo test -p taiyaku-serve patch_sets_can_stamp`. It MUST fail — a test
that only checked the `200` would pass here, which is the whole-page
`toContain` mistake in its Rust form. Restore.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/session.rs \
        crates/taiyaku-serve/src/users_api.rs \
        crates/taiyaku-serve/src/users_api/tests.rs
git commit -m "can_stamp on /api/me and the users PATCH"
```

---

### Task 4: Move `comicinfo.rs` into `taiyaku-library`

**Files:**
- Move: `crates/taiyaku-serve/src/comicinfo.rs` →
  `crates/taiyaku-library/src/comicinfo.rs`
- Move: `crates/taiyaku-serve/src/comicinfo/tests.rs` →
  `crates/taiyaku-library/src/comicinfo/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs:12` (delete `mod comicinfo;`)
- Modify: `crates/taiyaku-library/src/lib.rs` (add `pub mod comicinfo;` in
  alphabetical position, between `browse` and `credential`)
- Modify: `crates/taiyaku-serve/src/download.rs:772-777`

**Interfaces:**
- Consumes: nothing new.
- Produces: `taiyaku_library::comicinfo::{ComicInfo, read, xml}`, all `pub`.
  `read(conn: &Connection, slug: &str, edition_key: &str, page_count: u32)
  -> Result<ComicInfo, String>`; `xml(info: &ComicInfo) -> String`.

**This task changes no behaviour.** Its test is that every existing test
passes unchanged.

- [ ] **Step 1: Move the files**

```bash
git mv crates/taiyaku-serve/src/comicinfo.rs crates/taiyaku-library/src/comicinfo.rs
mkdir -p crates/taiyaku-library/src/comicinfo
git mv crates/taiyaku-serve/src/comicinfo/tests.rs crates/taiyaku-library/src/comicinfo/tests.rs
```

- [ ] **Step 2: Widen the visibility**

In `crates/taiyaku-library/src/comicinfo.rs`, change every `pub(crate)` on an
item to `pub`: the `ComicInfo` struct, its six fields, `read` and `xml`.
`element_for`, `escape` and `number_text` stay private.

Add to the module's own doc comment, at the end:

```rust
//! **This module was `taiyaku-serve`'s until spec C3 §6.** It reads the
//! database and renders XML, which is a store concern; it sat in the server
//! crate only because C2's generated download happened to be its first
//! caller. The stamp is its second, and a stamp is a store mutation, so the
//! two callers now share it from here rather than one reaching up into the
//! other's crate.
```

- [ ] **Step 3: Re-declare the module**

Delete `mod comicinfo;` from `crates/taiyaku-serve/src/lib.rs:12`.

Add to `crates/taiyaku-library/src/lib.rs`, between `pub mod browse;` and
`pub mod credential;`:

```rust
pub mod comicinfo;
```

- [ ] **Step 4: Repoint the download**

In `crates/taiyaku-serve/src/download.rs`, replace the three `crate::comicinfo`
paths at `:772-777` with `taiyaku_library::comicinfo`:

```rust
        taiyaku_library::comicinfo::read(&conn, slug, edition_key, page_count)
            .map(|info| taiyaku_library::comicinfo::xml(&info).into_bytes())
```

and, at `:777`:

```rust
                taiyaku_library::comicinfo::xml(&taiyaku_library::comicinfo::ComicInfo {
```

- [ ] **Step 5: Run the full suite**

Run: `cargo test -p taiyaku-library -p taiyaku-serve`
Expected: PASS, with the same test count as before the move. The moved
`comicinfo::tests` now run under `taiyaku-library`; the totals across the two
crates must not change.

- [ ] **Step 6: Commit**

```bash
git add -A crates/taiyaku-serve/src crates/taiyaku-library/src
git commit -m "move comicinfo.rs to taiyaku-library, where its second caller is"
```

---

### Task 5: `stamp::plan` — which archives would actually change

**Files:**
- Modify: `crates/taiyaku-library/Cargo.toml` (add `zip`)
- Create: `crates/taiyaku-library/src/stamp.rs`
- Create: `crates/taiyaku-library/src/stamp/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (add `pub mod stamp;` between
  `pub mod schema;` and `pub mod state;`)

**Interfaces:**
- Consumes: `taiyaku_library::comicinfo::{read, xml}` (Task 4);
  `taiyaku_read::member_entry(path: &Path, name: &str) -> Result<Option<Vec<u8>>, String>`.
- Produces:

```rust
pub struct Planned {
    pub member_id: i64,
    pub edition_key: String,
    pub path: std::path::PathBuf,
    pub bytes: u64,
}

pub fn plan(conn: &rusqlite::Connection, work_id: i64) -> Result<Vec<Planned>, String>;
```

`Planned` derives `Debug, Clone, PartialEq, Eq` — `apply` in Task 7 compares
two lists field for field and needs `Eq`.

- [ ] **Step 1: Add the dependency**

In `crates/taiyaku-library/Cargo.toml`, after `serde_json`:

```toml
# Spec C3 §6's write-back rewrites a member's archive, which means reading its
# central directory and writing it back out. Already in the tree via
# `taiyaku-read`, and named here for the reason that manifest names it: a
# direct dependency of this crate's own code should be a line in this crate's
# own manifest. `docs/status.md` states this workspace's dependency list as a
# fact about the project, so that file gains a line on the same branch.
zip = "7.2.0"
```

- [ ] **Step 2: Write the failing tests**

Create `crates/taiyaku-library/src/stamp/tests.rs`:

```rust
use super::*;

/// One scanned work, one edition, one member — built the way every other
/// fixture in this crate is built (`write_archive` into a `tempdir`, then
/// `add_root`, `scan_root` and `derive`) rather than by inserting rows, so
/// the `member` row carries the `sha256`, `pages`, `format` and `root_id` a
/// real scan writes and a stamp then has to move.
struct Fixture {
    library: crate::Library,
    dir: crate::tests::TempDir,
    work_id: i64,
}

impl Fixture {
    fn conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.library.conn()
    }

    fn archive(&self, name: &str) -> std::path::PathBuf {
        self.dir.path.join("Series").join(name)
    }
}

fn one_work() -> Fixture {
    let dir = crate::tests::tempdir("stamp");
    crate::scan::tests::write_archive(&dir.path.join("Series").join("Series - v01.cbz"), 2);

    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let work_id = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, &dir.path).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("the scan");
        crate::derive::derive(&conn).expect("the derive");
        conn.query_row("SELECT id FROM work", [], |r| r.get(0))
            .expect("the one work the fixture archive derived")
    };
    Fixture {
        library,
        dir,
        work_id,
    }
}

/// Put `entries` into the fixture's archive, replacing it, and bring the
/// member row's `sha256` and `pages` back in line with what is now on disk.
///
/// **Entries are written with `SimpleFileOptions::default()`, which
/// DEFLATEs** — deliberately, and Task 6 depends on it: the claim under test
/// there is that a rewrite copies each entry's *stored* bytes, and a
/// STORE-only fixture cannot tell a raw copy from a full re-encode. That is
/// the exact shape of a test passing against its own defect.
fn rewrite_fixture_archive(fixture: &Fixture, name: &str, entries: &[(&str, &[u8])]) {
    let path = fixture.archive(name);
    {
        let file = std::fs::File::create(&path).expect("creating the fixture archive");
        let mut zip = zip::ZipWriter::new(file);
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .expect("starting an entry");
            std::io::Write::write_all(&mut zip, bytes).expect("writing an entry");
        }
        zip.finish().expect("finishing the fixture archive");
    }

    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(std::fs::read(&path).unwrap());
        hasher.finalize().iter().fold(String::new(), |mut hex, b| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
            hex
        })
    };
    let pages = entries
        .iter()
        .filter(|(entry, _)| !entry.eq_ignore_ascii_case("ComicInfo.xml"))
        .count();

    let conn = fixture.conn();
    conn.execute(
        "UPDATE member SET sha256 = ?1, pages = ?2",
        rusqlite::params![digest, pages as i64],
    )
    .expect("bringing the member row back in line with the file");
}

/// Change the one member's `format` column.
///
/// A real `.rar` cannot be written here and does not need to be: the filter
/// under test reads `member.format`, so setting the column is what exercises
/// it. A fixture that shipped a real rar would test the rar crate.
fn set_format(fixture: &Fixture, format: &str) {
    let conn = fixture.conn();
    conn.execute("UPDATE member SET format = ?1", [format])
        .expect("setting the member's format");
}

#[test]
fn an_archive_with_no_comicinfo_is_in_the_plan() {
    let fixture = one_work();

    let planned = plan(&fixture.conn(), fixture.work_id).unwrap();

    assert_eq!(planned.len(), 1);
    assert!(
        planned[0].bytes > 0,
        "the size a person is shown before agreeing"
    );
    assert!(planned[0].path.ends_with("Series - v01.cbz"));
}

#[test]
fn an_archive_whose_comicinfo_already_matches_is_absent_from_the_plan() {
    // Render what a stamp would write, put exactly that into the archive, and
    // plan again. §1.3's whole claim is that the second plan is empty — this
    // is the test that claim lives or dies by.
    let fixture = one_work();
    let (slug, edition_key, rendered) = {
        let conn = fixture.conn();
        let (slug, edition_key): (String, String) = conn
            .query_row(
                "SELECT w.slug, e.key FROM work w JOIN edition e ON e.work_id = w.id
                  WHERE w.id = ?1",
                [fixture.work_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let info = crate::comicinfo::read(&conn, &slug, &edition_key, 2).unwrap();
        (slug, edition_key, crate::comicinfo::xml(&info).into_bytes())
    };
    let _ = (slug, edition_key);

    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[
            ("p1.jpg", b"jpegbytes"),
            ("p2.jpg", b"morebytes"),
            ("ComicInfo.xml", &rendered),
        ],
    );

    assert!(
        plan(&fixture.conn(), fixture.work_id).unwrap().is_empty(),
        "an archive already carrying exactly this document has nothing to do"
    );
}

#[test]
fn an_archive_whose_comicinfo_differs_is_in_the_plan() {
    let fixture = one_work();
    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[
            ("p1.jpg", b"jpegbytes"),
            ("p2.jpg", b"morebytes"),
            (
                "ComicInfo.xml",
                b"<?xml version=\"1.0\"?>\n<ComicInfo><Series>Something Else</Series></ComicInfo>\n",
            ),
        ],
    );

    assert_eq!(plan(&fixture.conn(), fixture.work_id).unwrap().len(), 1);
}

#[test]
fn a_rar_member_never_enters_a_plan() {
    let fixture = one_work();
    set_format(&fixture, "rar");

    assert!(plan(&fixture.conn(), fixture.work_id).unwrap().is_empty());
}

#[test]
fn a_7z_member_never_enters_a_plan() {
    let fixture = one_work();
    set_format(&fixture, "7z");

    assert!(plan(&fixture.conn(), fixture.work_id).unwrap().is_empty());
}

/// A `NULL` format is not a zip either, and the filter must say so
/// positively rather than by excluding the two names it knows.
#[test]
fn a_member_of_unknown_format_never_enters_a_plan() {
    let fixture = one_work();
    {
        let conn = fixture.conn();
        conn.execute("UPDATE member SET format = NULL", []).unwrap();
    }

    assert!(plan(&fixture.conn(), fixture.work_id).unwrap().is_empty());
}
```

`crate::tests::tempdir`, `crate::tests::TempDir` and
`crate::scan::tests::write_archive` are `pub(crate)` already and are what
every other fixture in this crate is built from; if `write_archive` is not
visible from `stamp/tests.rs`, widen it rather than writing a second zip
writer — two fixtures describing the same shape is how two tests come to
disagree about what the library looks like, which is the reason
`library_with_a_kept_document`'s own doc comment gives.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p taiyaku-library stamp::`
Expected: FAIL to compile — no `stamp` module.

- [ ] **Step 4: Write `stamp.rs`**

Create `crates/taiyaku-library/src/stamp.rs`:

```rust
//! Spec C3 §6: the one feature that writes to the user's library.
//!
//! Designed in
//! `docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md`, whose
//! gates are the store spec's §13.2. Every decision that matters is there;
//! what is here is the order they run in.
//!
//! **A plan is computed by reading, not remembered in a column.** An earlier
//! design carried `member.stamped_sha256` so a later scan could recognise
//! taiyaku's own output. It was cut: nothing in this codebase reads a chapter
//! number out of a `ComicInfo.xml`, so there was no loop to break, and
//! reading each archive's existing document answers the same question better
//! — it stays true across a database rebuild and across a hand-edit, neither
//! of which a column survives. §1.3.

use std::path::PathBuf;

use rusqlite::Connection;

/// One archive a stamp would rewrite, as the person confirming it sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub member_id: i64,
    pub edition_key: String,
    /// Absolute, resolved the way `download::member_paths` resolves one.
    pub path: PathBuf,
    /// The archive as it is now — the number on the confirmation.
    pub bytes: u64,
}

/// Which of this work's archives would change, and nothing else.
///
/// Reads each zip member's existing `ComicInfo.xml` and keeps the member only
/// when the bytes differ from what [`crate::comicinfo::xml`] renders now. So
/// a re-stamp after a one-field edit names the archives that edit reached,
/// and a stamp with nothing to do costs a central-directory read per member
/// rather than a copy of the whole work.
///
/// **`rar` and `7z` never appear**, per C3 §6 — only zip is written, and the
/// feature is absent rather than disabled for the rest.
///
/// # Errors
///
/// Returns an error if the rows cannot be read, if a member's root cannot be
/// resolved, or if an archive cannot be opened. An archive that is *missing*
/// is an error rather than a silent omission: a plan that quietly drops a
/// file it cannot see would under-report what the stamp is about to do.
pub fn plan(conn: &Connection, work_id: i64) -> Result<Vec<Planned>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.path, m.pages, m.format, e.key, w.slug, r.path
               FROM member m
               JOIN edition e ON e.id = m.edition_id
               JOIN work w    ON w.id = e.work_id
               LEFT JOIN root r ON r.id = m.root_id
              WHERE e.work_id = ?1
              ORDER BY e.key, m.ordinal",
        )
        .map_err(|e| format!("preparing this work's members: {e}"))?;

    let rows: Vec<(i64, String, u32, Option<String>, String, String, Option<String>)> = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .map_err(|e| format!("reading this work's members: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading this work's members: {e}"))?;

    let mut planned = Vec::new();
    for (member_id, member_path, pages, format, edition_key, slug, root) in rows {
        if format.as_deref() != Some("zip") {
            continue;
        }
        let path = match &root {
            Some(root) => std::path::Path::new(root).join(&member_path),
            None => PathBuf::from(&member_path),
        };

        let info = crate::comicinfo::read(conn, &slug, &edition_key, pages)?;
        let rendered = crate::comicinfo::xml(&info).into_bytes();

        let existing = taiyaku_read::member_entry(&path, "ComicInfo.xml")?;
        if existing.as_deref() == Some(rendered.as_slice()) {
            continue;
        }

        let bytes = std::fs::metadata(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();

        planned.push(Planned {
            member_id,
            edition_key,
            path,
            bytes,
        });
    }
    Ok(planned)
}

#[cfg(test)]
mod tests;
```

**Check the `member`/`root` join against the schema before running.** If
`member.root_id` or the `root` table is named differently, read
`crates/taiyaku-library/src/schema/006_member_root.sql` and
`012_root_keeps_documents.sql` and correct the query — the join above is the
shape `download::member_paths` uses and must match it, including its
scanned-work case where `member.path` is already absolute.

Add to `crates/taiyaku-library/src/lib.rs`:

```rust
pub mod stamp;
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p taiyaku-library stamp::`
Expected: PASS, all six.

- [ ] **Step 6: Hand-apply two defects and watch each fail**

1. Change `if format.as_deref() != Some("zip")` to
   `if format.as_deref() == Some("rar")`. Run
   `cargo test -p taiyaku-library _never_enters_a_plan`. All three MUST
   fail — the `7z` and `NULL` cases are what a defect written as an
   exclusion list gets wrong, and a single `rar` test would have passed
   against it. Revert.
2. Change `existing.as_deref() == Some(rendered.as_slice())` to
   `existing.is_some()`. Run
   `cargo test -p taiyaku-library an_archive_whose_comicinfo_differs`. It
   MUST fail. Revert.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/Cargo.toml Cargo.lock \
        crates/taiyaku-library/src/lib.rs \
        crates/taiyaku-library/src/stamp.rs \
        crates/taiyaku-library/src/stamp/tests.rs
git commit -m "stamp::plan: the archives a stamp would actually change"
```

---

### Task 6: the rewrite — one archive, raw-copied, atomically renamed

**Files:**
- Modify: `crates/taiyaku-library/src/stamp.rs`
- Modify: `crates/taiyaku-library/src/stamp/tests.rs`

**Interfaces:**
- Consumes: `Planned` (Task 5).
- Produces: `fn rewrite_one(path: &Path, document: &[u8]) -> Result<String, String>`
  — private to the module, returning the **new sha256** of the rewritten
  archive as lowercase hex.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/stamp/tests.rs`:

```rust
/// Reads every entry of a zip as `(name, compressed bytes)`.
///
/// **Compressed bytes, not decompressed.** The claim under test is that a
/// rewrite copies each entry's stored form untouched; comparing decompressed
/// pages would pass against a full re-encode, which is the thing §4 rejected.
fn raw_entries(path: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).unwrap();
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        let mut entry = entry;
        std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
        out.push((name, bytes));
    }
    out
}

#[test]
fn a_rewrite_leaves_every_page_byte_identical() {
    let fixture = one_work();
    let path = fixture.archive("Series - v01.cbz");
    let before: Vec<_> = raw_entries(&path)
        .into_iter()
        .filter(|(name, _)| name != "ComicInfo.xml")
        .collect();

    super::rewrite_one(&path, b"<ComicInfo/>").unwrap();

    let after: Vec<_> = raw_entries(&path)
        .into_iter()
        .filter(|(name, _)| name != "ComicInfo.xml")
        .collect();
    assert_eq!(before, after, "the pages' stored bytes are untouched");
}

#[test]
fn a_rewrite_replaces_an_existing_comicinfo_rather_than_duplicating_it() {
    let fixture = one_work();
    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[("p1.jpg", b"jpegbytes"), ("ComicInfo.xml", b"<old/>")],
    );
    let path = fixture.archive("Series - v01.cbz");

    super::rewrite_one(&path, b"<ComicInfo/>").unwrap();

    let names: Vec<String> = raw_entries(&path).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names.iter().filter(|n| n.as_str() == "ComicInfo.xml").count(),
        1,
        "one document, not two"
    );
    let file = std::fs::File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("ComicInfo.xml").unwrap(), &mut bytes).unwrap();
    assert_eq!(bytes, b"<ComicInfo/>", "and it is the new one");
}

#[test]
fn a_rewrite_adds_exactly_one_document_to_an_archive_that_had_none() {
    let fixture = one_work();
    rewrite_fixture_archive(&fixture, "Series - v01.cbz", &[("p1.jpg", b"jpegbytes")]);
    let path = fixture.archive("Series - v01.cbz");

    super::rewrite_one(&path, b"<ComicInfo/>").unwrap();

    let names: Vec<String> = raw_entries(&path).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"ComicInfo.xml".to_string()));
}

#[test]
fn a_rewrite_returns_the_new_digest_and_leaves_no_temp_behind() {
    let fixture = one_work();
    let path = fixture.archive("Series - v01.cbz");

    let digest = super::rewrite_one(&path, b"<ComicInfo/>").unwrap();

    assert_eq!(digest.len(), 64, "lowercase hex sha256");
    let on_disk = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(std::fs::read(&path).unwrap());
        hasher.finalize().iter().fold(String::new(), |mut hex, b| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
            hex
        })
    };
    assert_eq!(digest, on_disk, "the digest names the file that is there now");

    let strays: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".taiyaku-stamp"))
        .collect();
    assert!(strays.is_empty(), "the staging file was renamed, not left");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library stamp::tests`
Expected: FAIL to compile — no `rewrite_one`.

- [ ] **Step 3: Implement `rewrite_one`**

Append to `crates/taiyaku-library/src/stamp.rs`:

```rust
/// The entry a stamp writes. Matched case-insensitively when replacing, and
/// written under exactly this spelling.
const DOCUMENT: &str = "ComicInfo.xml";

/// Rewrite one archive with `document` in place of any `ComicInfo.xml` it
/// holds, and answer the new file's sha256.
///
/// **Every other entry is `raw_copy_file`d**, so its stored bytes and its
/// compression method survive untouched. Two reasons, and neither is
/// tidiness: re-encoding would change every page of a file the user owns,
/// and `STORE` — which `cbz.rs` chose for the *generated* download so that
/// `Content-Length` is exact before the first byte — would inflate 97.9% of
/// this corpus's entries, turning a metadata edit into a size increase across
/// the library. `cbz.rs` measured that on 2026-08-23.
///
/// **Staged as a sibling and renamed**, so the replacement is atomic and a
/// crash leaves either the old archive or the new one, never a half-written
/// file. The sibling shares the target's directory deliberately: a temp
/// directory elsewhere would put the rename across a filesystem boundary,
/// where it stops being atomic and becomes a copy.
///
/// # Errors
///
/// Returns an error if the archive cannot be read or written, or if the
/// rename fails. On any error the staging file is removed and the original is
/// untouched.
fn rewrite_one(path: &std::path::Path, document: &[u8]) -> Result<String, String> {
    use std::io::Read as _;

    let staged = path.with_extension(format!(
        "{}.taiyaku-stamp",
        path.extension().unwrap_or_default().to_string_lossy()
    ));

    let result = (|| -> Result<String, String> {
        let source = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut archive =
            zip::ZipArchive::new(source).map_err(|e| format!("{}: {e}", path.display()))?;

        let out =
            std::fs::File::create(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
        let mut writer = zip::ZipWriter::new(out);

        for i in 0..archive.len() {
            let entry = archive
                .by_index_raw(i)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if entry.name().eq_ignore_ascii_case(DOCUMENT) {
                continue;
            }
            writer
                .raw_copy_file(entry)
                .map_err(|e| format!("{}: {e}", staged.display()))?;
        }

        writer
            .start_file(DOCUMENT, zip::write::SimpleFileOptions::default())
            .map_err(|e| format!("{}: {e}", staged.display()))?;
        std::io::Write::write_all(&mut writer, document)
            .map_err(|e| format!("{}: {e}", staged.display()))?;
        writer
            .finish()
            .map_err(|e| format!("{}: {e}", staged.display()))?;

        let mut file =
            std::fs::File::open(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|e| format!("{}: {e}", staged.display()))?;
            if read == 0 {
                break;
            }
            sha2::Digest::update(&mut hasher, &buffer[..read]);
        }
        let digest = sha2::Digest::finalize(hasher)
            .iter()
            .fold(String::new(), |mut hex, b| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{b:02x}");
                hex
            });

        std::fs::rename(&staged, path)
            .map_err(|e| format!("renaming {} onto {}: {e}", staged.display(), path.display()))?;
        Ok(digest)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-library stamp::tests`
Expected: PASS, all nine tests in the module.

- [ ] **Step 5: Hand-apply the defect and watch it fail**

Replace the `raw_copy_file` loop body with a decompress-and-recompress:

```rust
            let mut bytes = Vec::new();
            let mut entry = archive.by_index(i).unwrap();
            std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
            writer.start_file(entry.name(), zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut writer, &bytes).unwrap();
```

Run `cargo test -p taiyaku-library a_rewrite_leaves_every_page_byte_identical`.
It MUST fail. **If it passes, the fixture is writing `STORE` entries and the
test cannot see the difference** — fix the fixture to DEFLATE before
continuing, because this is the exact shape of a test passing against its own
defect. Revert.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/stamp.rs crates/taiyaku-library/src/stamp/tests.rs
git commit -m "stamp::rewrite_one: raw-copy every entry, replace the document, rename"
```

---

### Task 7: `stamp::apply` — the transaction, the rekey, the re-export

**Files:**
- Modify: `crates/taiyaku-library/src/stamp.rs`
- Modify: `crates/taiyaku-library/src/stamp/tests.rs`

**Interfaces:**
- Consumes: `plan` (Task 5), `rewrite_one` (Task 6),
  `crate::project::sidecar(conn, work_id) -> Result<Sidecar, String>`,
  `crate::identity::content_key(&Sidecar) -> Result<String, String>`,
  `crate::hand::rename_for_rekey(conn, work_id, old_key) -> Result<bool, String>`,
  `crate::export::work(conn, work_id, user_id, None) -> Result<Exported, String>`.
- Produces:

```rust
pub struct Applied {
    pub members: Vec<(i64, String)>,
    pub content_key: Option<String>,
    pub document_moved: bool,
}

pub fn apply(
    conn: &rusqlite::Connection,
    work_id: i64,
    user_id: i64,
    expected: &[Planned],
) -> Result<Applied, String>;
```

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/stamp/tests.rs`:

```rust
#[test]
fn apply_refuses_a_plan_that_no_longer_matches() {
    let fixture = one_work();
    let conn = fixture.conn();
    let work_id = fixture.work_id;
    let mut stale = plan(&conn, work_id).unwrap();
    // The archive grew between the plan and the confirmation: same member, same
    // path, a different number from the one the person read and agreed to.
    stale[0].bytes += 1;

    let refused = apply(&conn, work_id, 1, &stale);

    assert!(refused.is_err(), "a plan that no longer holds is refused");
    assert!(
        refused.unwrap_err().contains("changed"),
        "and the message says why"
    );
}

#[test]
fn apply_writes_the_new_digest_into_the_member_row() {
    let fixture = one_work();
    let conn = fixture.conn();
    let work_id = fixture.work_id;
    let before: String = conn
        .query_row("SELECT sha256 FROM member LIMIT 1", [], |r| r.get(0))
        .unwrap();

    let applied = apply(&conn, work_id, 1, &plan(&conn, work_id).unwrap()).unwrap();

    let after: String = conn
        .query_row("SELECT sha256 FROM member LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_ne!(before, after, "the row moved");
    assert_eq!(applied.members.len(), 1);
    assert_eq!(applied.members[0].1, after, "and it is what apply reported");

    let on_disk = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(std::fs::read(fixture.archive("Series - v01.cbz")).unwrap());
        hasher.finalize().iter().fold(String::new(), |mut hex, b| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
            hex
        })
    };
    assert_eq!(after, on_disk, "and the row names the file that is there");
}

#[test]
fn apply_moves_content_key_when_the_hub_edition_changes() {
    let fixture = one_work();
    let conn = fixture.conn();
    let work_id = fixture.work_id;
    let before: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [work_id], |r| r.get(0))
        .unwrap();

    let applied = apply(&conn, work_id, 1, &plan(&conn, work_id).unwrap()).unwrap();

    let after: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [work_id], |r| r.get(0))
        .unwrap();
    assert_ne!(before, after, "the work's identity moved with its hub");
    assert_eq!(applied.content_key.as_deref(), Some(after.as_str()));
}

#[test]
fn a_second_apply_has_nothing_to_do() {
    let fixture = one_work();
    let conn = fixture.conn();
    let work_id = fixture.work_id;
    apply(&conn, work_id, 1, &plan(&conn, work_id).unwrap()).unwrap();

    assert!(
        plan(&conn, work_id).unwrap().is_empty(),
        "the archive now carries exactly the document that would be written"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library stamp::tests::apply`
Expected: FAIL to compile — no `apply`.

- [ ] **Step 3: Implement `apply`**

Append to `crates/taiyaku-library/src/stamp.rs`:

```rust
/// What one stamp changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// `(member id, its new sha256)`, in the order the plan named them.
    pub members: Vec<(i64, String)>,
    /// `Some` when the hub edition moved and the work's identity with it.
    pub content_key: Option<String>,
    /// What [`crate::hand::rename_for_rekey`] answered.
    pub document_moved: bool,
}

/// Rewrite this work's planned archives and bring the database up to them.
///
/// **`expected` is compared field for field against a freshly derived plan,
/// and a mismatch refuses.** Not a count and not a set of member ids: the
/// case this guard exists for is an archive that grew between the plan and
/// the confirmation, which is the same member at the same path, and whose
/// size is the number the person read before agreeing. C3 §3's principle —
/// the server writes what the client was shown — applied to the one route
/// that rewrites files.
///
/// **Stage one, rename one, then one transaction.** Peak extra disk is one
/// archive rather than the whole work, which on §6's own example is 4.1 GB of
/// difference. The cost is a window: a crash after some renames leaves those
/// files stamped and their rows stale, which is exactly what `member.sha256`
/// exists to detect — the scanner reports the work stale and a rescan
/// reconciles it. Spec §1.4 records why the two alternatives lost.
///
/// The hand document and the sidecar are rewritten **after** the transaction
/// commits, because both are projections of the rows it wrote.
///
/// # Errors
///
/// Returns an error if the plan no longer holds, if any archive cannot be
/// rewritten, or if the transaction fails. An error after some archives have
/// been renamed leaves those files stamped and says so in its message; the
/// database is then behind the disk until a rescan.
pub fn apply(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    expected: &[Planned],
) -> Result<Applied, String> {
    let current = plan(conn, work_id)?;
    if current.as_slice() != expected {
        return Err(
            "this work's archives have changed since the plan was made; \
             nothing was written"
                .to_string(),
        );
    }

    let old_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [work_id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("reading the work's content key: {e}"))?;

    let mut written: Vec<(i64, String)> = Vec::new();
    for item in &current {
        let info = {
            let pages: u32 = conn
                .query_row("SELECT pages FROM member WHERE id = ?1", [item.member_id], |row| {
                    row.get(0)
                })
                .map_err(|e| format!("reading member {}: {e}", item.member_id))?;
            let slug: String = conn
                .query_row(
                    "SELECT w.slug FROM work w
                       JOIN edition e ON e.work_id = w.id
                       JOIN member m ON m.edition_id = e.id
                      WHERE m.id = ?1",
                    [item.member_id],
                    |row| row.get(0),
                )
                .map_err(|e| format!("reading the work of member {}: {e}", item.member_id))?;
            crate::comicinfo::read(conn, &slug, &item.edition_key, pages)?
        };
        let document = crate::comicinfo::xml(&info).into_bytes();

        let digest = rewrite_one(&item.path, &document).map_err(|e| {
            format!(
                "{e} — {} of this work's archives were already rewritten and their \
                 rows are now stale; a rescan reconciles them",
                written.len()
            )
        })?;
        written.push((item.member_id, digest));
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the transaction for this stamp: {e}"))?;
    for (member_id, digest) in &written {
        tx.execute(
            "UPDATE member SET sha256 = ?1 WHERE id = ?2",
            rusqlite::params![digest, member_id],
        )
        .map_err(|e| format!("recording the new digest of member {member_id}: {e}"))?;
    }

    // Derived from the rows just written, so the hash algorithm is stated in
    // `identity::content_key` alone and not restated here.
    let new_key = crate::identity::content_key(&crate::project::sidecar(&tx, work_id)?)?;
    let moved_key = (new_key != old_key).then(|| new_key.clone());
    if moved_key.is_some() {
        tx.execute(
            "UPDATE work SET content_key = ?1 WHERE id = ?2",
            rusqlite::params![new_key, work_id],
        )
        .map_err(|e| format!("recording the work's new content key: {e}"))?;
    }
    tx.commit()
        .map_err(|e| format!("committing this stamp: {e}"))?;

    let document_moved = match &moved_key {
        Some(_) => crate::hand::rename_for_rekey(conn, work_id, &old_key)?,
        None => false,
    };
    crate::export::work(conn, work_id, user_id, None)?;

    Ok(Applied {
        members: written,
        content_key: moved_key,
        document_moved,
    })
}
```

If `unchecked_transaction` is not what this crate uses for a nested
transaction, read `crates/taiyaku-library/src/state.rs`'s `Nested` type and
use that instead — its `Drop` behaviour is asserted by a test and this call
must not bypass it.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-library stamp::`
Expected: PASS, all thirteen.

- [ ] **Step 5: Hand-apply two defects and watch each fail**

1. Change `if current != expected` to `if current.len() != expected.len()`.
   Run `cargo test -p taiyaku-library apply_refuses_a_plan_that_no_longer_matches`.
   It MUST fail — this is the exact weakening §3 of the spec names.
   Revert.
2. Delete the `UPDATE work SET content_key` statement. Run
   `cargo test -p taiyaku-library apply_moves_content_key`. It MUST fail.
   Revert.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/stamp.rs crates/taiyaku-library/src/stamp/tests.rs
git commit -m "stamp::apply: rewrite, then move the rows, the key and the document"
```

---

### Task 8: the two routes

**Files:**
- Create: `crates/taiyaku-serve/src/stamp.rs`
- Create: `crates/taiyaku-serve/src/stamp/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (add `mod stamp;`)
- Modify: `crates/taiyaku-serve/src/routes.rs` — `guarded_routes()` near
  `:1884`, and `grant_postures()` near `:2225`
- Modify: `crates/taiyaku-serve/src/routes.rs` — delete the
  `#[cfg_attr(not(test), expect(dead_code, …))]` from
  `refuse_if_cannot_stamp` if Task 2 added one

**Interfaces:**
- Consumes: `taiyaku_library::stamp::{plan, apply, Planned, Applied}`
  (Tasks 5–7); `crate::routes::refuse_if_cannot_stamp` (Task 2).
- Produces: `GET`/`POST /api/library/works/{slug}/stamp-comicinfo`.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/stamp/tests.rs`:

```rust
#[tokio::test]
async fn the_stamp_routes_refuse_a_reader() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_scanned_work_as_a_reader();

    for (method, path) in [
        ("GET", "/api/library/works/split/stamp-comicinfo"),
        ("POST", "/api/library/works/split/stamp-comicinfo"),
    ] {
        let response = crate::test_support::drive(&router, method, path, "{}").await;
        assert_eq!(
            response.status(),
            axum::http::StatusCode::FORBIDDEN,
            "{method} {path}"
        );
    }
}

/// **The capability, not the role** — §1.1's exception proved at the route
/// rather than only at `Principal`.
#[tokio::test]
async fn an_administrator_without_can_stamp_is_refused() {
    let (router, state, _fixture) = crate::test_support::router_over_a_scanned_work();
    // The fixture's administrator holds can_stamp = 0, which is the schema's
    // default and what every existing library's admin holds after migration 13.

    let response = crate::test_support::drive(
        &router,
        "POST",
        "/api/library/works/split/stamp-comicinfo",
        r#"{"plan": []}"#,
    )
    .await;

    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
    let _ = state;
}

#[tokio::test]
async fn the_plan_route_reports_each_archive_and_the_total() {
    let (router, state, _fixture) = crate::test_support::router_over_a_scanned_work();
    grant_can_stamp(&state);

    let response = crate::test_support::drive(
        &router,
        "GET",
        "/api/library/works/split/stamp-comicinfo",
        "",
    )
    .await;

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body: serde_json::Value = crate::test_support::json(response).await;
    assert!(body["archives"].is_array());
    assert!(body["total_bytes"].as_u64().is_some());
}

#[tokio::test]
async fn a_work_with_nothing_to_do_answers_200_and_an_empty_list() {
    let (router, state, _fixture) = crate::test_support::router_over_a_scanned_work();
    grant_can_stamp(&state);

    // Stamp once, then ask again.
    let plan = crate::test_support::json(
        crate::test_support::drive(&router, "GET", "/api/library/works/split/stamp-comicinfo", "")
            .await,
    )
    .await;
    let _ = crate::test_support::drive(
        &router,
        "POST",
        "/api/library/works/split/stamp-comicinfo",
        &serde_json::to_string(&plan).unwrap(),
    )
    .await;

    let response =
        crate::test_support::drive(&router, "GET", "/api/library/works/split/stamp-comicinfo", "")
            .await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body: serde_json::Value = crate::test_support::json(response).await;
    assert_eq!(body["archives"].as_array().unwrap().len(), 0);
}

/// **The refusal order, pinned.** An account that may not stamp must be
/// answered the same way for a work that does not exist as for one that
/// does; a 404 here and a 403 elsewhere is an existence oracle. Spec §2.
#[tokio::test]
async fn an_administrator_without_can_stamp_cannot_tell_a_missing_work_from_a_real_one() {
    let (router, _state, _fixture) = crate::test_support::router_over_a_scanned_work();

    let real = crate::test_support::drive(
        &router,
        "GET",
        "/api/library/works/split/stamp-comicinfo",
        "",
    )
    .await;
    let missing = crate::test_support::drive(
        &router,
        "GET",
        "/api/library/works/no-such-work/stamp-comicinfo",
        "",
    )
    .await;

    assert_eq!(real.status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(
        missing.status(),
        real.status(),
        "the two must be indistinguishable to an account that may not stamp"
    );
}

fn grant_can_stamp(state: &std::sync::Arc<crate::ServeState>) {
    let conn = state.library.connection().unwrap();
    conn.execute("UPDATE user SET can_stamp = 1", []).unwrap();
}
```

`crate::test_support::drive` and `::json` may not exist under those names.
Read `crates/taiyaku-serve/src/curation/tests.rs` — which drives six routes
for a 403 in `every_curation_route_refuses_a_reader` — and reuse whatever it
uses, verbatim.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve stamp::`
Expected: FAIL — 404, no such route.

- [ ] **Step 3: Write the routes**

Create `crates/taiyaku-serve/src/stamp.rs`:

```rust
//! Spec C3 §6's two routes: what a stamp would rewrite, and doing it.
//!
//! **Its own module rather than `metadata.rs`'s.** C3 §6's argument is that
//! this is not a save path — *"putting it on the same Save button would make
//! the irreversible thing a side effect of the reversible one"* — and
//! sharing a module with the four `PATCH` routes is the same mistake one
//! layer down.
//!
//! **The capability is read before the slug is resolved, which is the
//! opposite of what the build design's §2 wrote.** §2 ordered visibility
//! first *"so a work the caller may not see 404s rather than admitting it
//! exists with a 403"*, and that reasoning inverts here: an account without
//! `can_stamp` that is answered 404 for a hidden work and 403 for a visible
//! one has just been told which works exist. Reading the capability first
//! answers 403 for every slug alike and leaks nothing. §9 of this plan
//! corrects the spec.

use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    response::{IntoResponse, Response},
    Extension, Json,
};

use crate::{auth::Principal, metadata::NoAnswer, ServeState};

/// One archive on the wire, in both directions.
///
/// The `POST` sends back exactly what the `GET` returned, and
/// [`taiyaku_library::stamp::apply`] compares the two field for field — so
/// this type is the plan's identity and not a summary of it.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct WireArchive {
    member_id: i64,
    edition_key: String,
    path: String,
    bytes: u64,
}

impl From<&taiyaku_library::stamp::Planned> for WireArchive {
    fn from(planned: &taiyaku_library::stamp::Planned) -> Self {
        Self {
            member_id: planned.member_id,
            edition_key: planned.edition_key.clone(),
            path: planned.path.to_string_lossy().into_owned(),
            bytes: planned.bytes,
        }
    }
}

impl From<&WireArchive> for taiyaku_library::stamp::Planned {
    fn from(wire: &WireArchive) -> Self {
        Self {
            member_id: wire.member_id,
            edition_key: wire.edition_key.clone(),
            path: std::path::PathBuf::from(&wire.path),
            bytes: wire.bytes,
        }
    }
}

#[derive(serde::Deserialize)]
pub(crate) struct Confirmation {
    archives: Vec<WireArchive>,
}

/// The formats this work holds that a stamp will not write.
///
/// Distinct from the plan being short: "4 archives" reads as "all of them"
/// unless the screen can also say the rar was left alone. Spec C3 §6.
fn skipped_formats(conn: &rusqlite::Connection, work_id: i64) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT m.format FROM member m
           JOIN edition e ON e.id = m.edition_id
          WHERE e.work_id = ?1 AND (m.format IS NULL OR m.format <> 'zip')",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([work_id], |row| row.get::<_, Option<String>>(0))
        .map(|rows| {
            rows.filter_map(Result::ok)
                .map(|f| f.unwrap_or_else(|| "unknown".to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn describe(planned: &[taiyaku_library::stamp::Planned], skipped: Vec<String>) -> serde_json::Value {
    serde_json::json!({
        "archives": planned.iter().map(WireArchive::from).collect::<Vec<_>>(),
        "total_bytes": planned.iter().map(|p| p.bytes).sum::<u64>(),
        "skipped_formats": skipped,
    })
}

/// `GET /api/library/works/{slug}/stamp-comicinfo` — what would change.
///
/// An empty list is a `200`, not a `404`: "nothing to do" is an answer, and
/// the screen renders it as an absent control rather than as an error.
pub(crate) async fn plan(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(slug): AxPath<String>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stamp(&extensions) {
        return refusal;
    }
    let joined = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, NoAnswer> {
        let conn = state.library.conn();
        let work_id = crate::metadata::work_by_slug(&conn, &principal.roots, &slug)?;
        let planned = taiyaku_library::stamp::plan(&conn, work_id).map_err(NoAnswer::Refused)?;
        let skipped = skipped_formats(&conn, work_id);
        Ok(describe(&planned, skipped))
    })
    .await;
    answer(joined)
}

/// `POST /api/library/works/{slug}/stamp-comicinfo` — do it.
///
/// The body carries the plan the screen was shown, and
/// [`taiyaku_library::stamp::apply`] refuses one that no longer matches. That
/// refusal is a **409**, not a 400: the request was well formed and the world
/// moved under it.
pub(crate) async fn perform(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(slug): AxPath<String>,
    Json(confirmation): Json<Confirmation>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stamp(&extensions) {
        return refusal;
    }
    let joined = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, NoAnswer> {
        let conn = state.library.conn();
        let work_id = crate::metadata::work_by_slug(&conn, &principal.roots, &slug)?;
        let expected: Vec<taiyaku_library::stamp::Planned> = confirmation
            .archives
            .iter()
            .map(taiyaku_library::stamp::Planned::from)
            .collect();
        let applied =
            taiyaku_library::stamp::apply(&conn, work_id, principal.user_id, &expected)
                .map_err(NoAnswer::Refused)?;
        Ok(serde_json::json!({
            "rewritten": applied.members.len(),
            "content_key": applied.content_key,
            "document_moved": applied.document_moved,
        }))
    })
    .await;
    answer(joined)
}

/// The `spawn_blocking` join, mapped exactly as `metadata.rs` maps its own.
fn answer(
    joined: Result<Result<serde_json::Value, NoAnswer>, tokio::task::JoinError>,
) -> Response {
    match joined {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no)) => no.into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("the stamp did not run: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests;
```

**Three things this code needs that do not exist yet, and each is a one-line
change rather than a new abstraction:**

1. `metadata::work_by_slug` is private (`metadata.rs:422`). Widen it to
   `pub(crate)`. Do not copy it — a second copy of the grant clause is how
   two routes come to disagree about what a caller may see.
2. `metadata::NoAnswer` is private. Widen it to `pub(crate)` too, and check
   how `metadata.rs` turns it into a `Response`; if that mapping is a private
   `impl IntoResponse` it is already what `answer` above needs, and if it is
   an inline `match` in each handler, lift it into an
   `impl IntoResponse for NoAnswer` and use it from both modules.
3. `NoAnswer::Refused` takes a `String` in `metadata.rs`; confirm the variant
   shape before using `map_err(NoAnswer::Refused)` and adjust if it differs.

- [ ] **Step 4: Register the routes and the posture**

In `guarded_routes()`, after the metadata block at `:1897`:

```rust
        // **Spec C3 §6's write-back, and the only route in this product that
        // modifies a file the user owns.** Guarded four ways — authenticated,
        // admin, visible, and `can_stamp` — and the fourth is a capability an
        // administrator does not inherit, which is this branch's one
        // documented exception to O86. See `auth::Principal::can_stamp`.
        (
            "/api/library/works/{slug}/stamp-comicinfo",
            axum::routing::get(crate::stamp::plan).post(crate::stamp::perform),
        ),
```

In `grant_postures()`, beside the other work routes at `:2225`:

```rust
        (
            "/api/library/works/{slug}/stamp-comicinfo",
            Refuses,
        ),
```

**`Refuses`, not `Filters`**, because this route does not narrow what it
returns to the grant the way `GET /api/library/works/{slug}` narrows a work's
editions — it refuses outright on a work outside the grant, via
`refuse_if_hidden`, which answers 404. If
`auth::tests::every_route_declares_its_grant_posture` disagrees, read what it
drove and correct the posture rather than the test.

Add `mod stamp;` to `crates/taiyaku-serve/src/lib.rs` beside the other module
declarations, and delete the `expect(dead_code)` attribute from
`refuse_if_cannot_stamp` if Task 2 added one — `#[expect]` fails the build
once the lint stops firing, so this is not optional.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p taiyaku-serve stamp::`
Expected: PASS, all five.

Run: `cargo test -p taiyaku-serve auth::tests`
Expected: PASS — `every_route_refuses_an_anonymous_request` and
`every_route_declares_its_grant_posture` both see the two new paths.

- [ ] **Step 6: Hand-apply the defect and watch it fail**

Delete `refuse_if_cannot_stamp` from the `POST` handler's refusal chain. Run
`cargo test -p taiyaku-serve an_administrator_without_can_stamp_is_refused`.
It MUST fail. **If it passes, the test is being refused by an earlier guard
and proves nothing about the capability** — the same shape as #112's two
double-press tests, which proved `disabled` rather than the ref they named.
Fix the test, then revert the defect.

- [ ] **Step 7: Run the gate**

Run: `scripts/check-all.sh`
Expected: the final line reads `all green`. Quote that line; do not pipe the
script through `tail` or append `; echo`, either of which masks its exit
status.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-serve/src/stamp.rs \
        crates/taiyaku-serve/src/stamp/tests.rs \
        crates/taiyaku-serve/src/lib.rs \
        crates/taiyaku-serve/src/routes.rs
git commit -m "the two stamp-comicinfo routes, guarded four ways"
```

---

### Task 9: the documents this branch changes

**Files:**
- Modify: `docs/status.md` — the dependency list gains `zip` for
  `taiyaku-library`; the C3 row of "What is built"; test counts
- Modify: `docs/superpowers/specs/2026-08-14-library-store-design.md` — §10's
  table row for C3 (stale: it says §5 is not built, and §5 shipped as #91)
  and C4's row (no shipped marker though #112 finished it)
- Modify: `docs/superpowers/specs/2026-08-14-library-curation-design.md` — §6
  and §7 gain shipped markers for the server half
- Modify: `docs/superpowers/specs/2026-08-14-library-auth-design.md` — the
  capability list gains `can_stamp` and the O86 exception
- Modify: `README.md` — *"Archives are read, never rewritten"*
- The spec's §2 was corrected while this plan was written — the capability is
  read before the slug is resolved, not after. No further edit is needed; it
  is listed so a reader comparing the two documents knows which way round is
  current.
- Modify: `docs/notes.md` — the measurements below

This task is listed last and is not optional. Spec §9 lists these because
O95's own §6 listed the docs to update and missed the design for the very
screen that changed.

- [ ] **Step 1: Measure what the spec's §11 asks for**

Against the real library, per
`docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md` §11:

1. The cost of `GET .../stamp-comicinfo` on a work with several members.
2. The cost of one `POST` over the largest work available.

Record both in `docs/notes.md` with the date, the machine and the command.
A figure that was not measured says so.

- [ ] **Step 2: Narrow README's principle rather than deleting it**

```markdown
Your library is not modified. Archives are read, never rewritten — with one
exception, which is opt-in per account and confirmed per work: the ComicInfo
write-back (`can_stamp`), which rewrites a work's archives to carry the
metadata taiyaku holds. No account has it until an administrator grants it.
```

- [ ] **Step 3: Update the rest**

Correct the two stale rows in the store spec's §10 table while that file is
open — they are stale independently of this work and the spec says to fix
them here.

- [ ] **Step 4: Run the gate and commit**

Run: `scripts/check-all.sh`
Expected: `all green`. The doc-link check added by #95 is part of it.

```bash
git add docs/ README.md
git commit -m "docs: the write-back's server half, and two stale rows in §10"
```

---

## Before the PR

1. `scripts/check-all.sh` on the exact head that will merge; quote the `all
   green` line.
2. The mutation sweep: `scripts/mutants.sh --since main`. **Commit first** —
   it diffs `git diff main...HEAD`, so uncommitted fixes get re-reported.
   Check the unviable count against what you changed; a `0 survivors` on a
   sweep that examined nothing is not a result.
3. A whole-branch review, then a review of the fix round. This project's
   record is that per-task review passes while whole-branch finds Criticals,
   and that passes do not converge — #112 ran four and passes 2–4 found only
   defects inside earlier fixes.
4. The PR body is the squash commit message and appears once. Do not
   hard-wrap it. Use the gitea MCP, not `gh`.

## Self-review of this plan against the spec

- §1.1 capability → Tasks 1, 2, 3, and the route guard in Task 8. ✅
- §1.2 every zip member of every edition, `PageCount` = `member.pages`,
  rar/7z absent → Task 5's query and its two format tests. ✅
- §1.3 plan by reading, no column → Task 5. ✅
- §1.4 stage one rename one, one transaction, rekey and re-export after →
  Tasks 6 and 7. ✅
- §1.5 placement, `comicinfo.rs` moves, `zip` in the manifest → Tasks 4
  and 5. ✅
- §2's four refusals and the fifth guard → Task 8's refusal order and Task 7's
  plan comparison. ✅
- §3 `Planned`/`Applied` → Tasks 5 and 7. ✅
- §4 raw copy, replace, atomic rename → Task 6. ✅
- §5 two routes, `skipped_formats` → Task 8. ✅
- §6 the screen → **PR B, not this plan.** ✅
- §8's test list → one test per item across Tasks 1–8; every one has a
  hand-applied defect step. ✅
- §9 documents → Task 9. ✅
- §11's measurements → Task 9 Step 1. ✅

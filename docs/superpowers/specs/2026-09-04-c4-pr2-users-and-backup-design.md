# C4 PR 2 — users and backup: build design

**Status: settled in brainstorming on 2026-09-04. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is the **second and last server-half PR of spec C4**, whose product design
is [`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
and whose build design is
[`2026-09-02-library-configuration-build-design.md`](2026-09-02-library-configuration-build-design.md).
Neither is restated here. That second document's §1 already scoped this PR —
*"user CRUD, the three refusals, session and key revocation, and backup as a
store function, a CLI command and three routes. CRUD over tables that already
exist, with no new mechanism in it"* — and this one settles the four things it
left open, plus the module boundaries.

Section references below are to the **product** design (`C4 §4`) and the
**build** design (`build §1`) explicitly, because both have a §1 and a §4.

## 0. What already exists, and the four gaps

**The store's account layer is most of the way there already.** `account.rs`
carries `add`, `set_password`, `list`, `keys_of`, `sessions_of`,
`revoke_session`, `revoke_key`, `create_key`, `grant_root`, `revoke_root`,
`granted_roots`, `promote` and `id_of` — every reader a users screen needs and
several of its writers, built for `taiyaku library user` and `taiyaku library
key`.

**Two of C4 §4's three refusals are also already there, in one form or
another.**

- `promote` refuses to leave the library with no administrator, in as many
  words: *"promoting the only account to `{role}` would leave this library with
  no administrator and no way to add one"*. It was added by A2's fourth
  whole-branch review, and it is the precedent this design follows — **an
  invariant of the account table lives in the store, not in a route**, because
  the CLI is a second writer and a copy in each is a copy that drifts.
- `set_password`'s `keep: Option<&str>` already does what C4 §4's third refusal
  asks. Its own doc comment says so: *"`None` ends them all, which is what an
  admin resetting somebody else's password means"*. PR 2 routes to it and tests
  it; it does not build it.

**Four things do not exist.**

1. **No way to change a role, `can_download`, `can_stream` or a name.** O54
   records the consequence — the two capability columns *"cannot be set to 0
   outside a fixture until spec C4's user administration"* — and it is still
   true on `main` at `46bf9e1`.
2. **No way to delete an account.** Nothing in the tree issues
   `DELETE FROM user`.
3. **No writer for the whole grant list.** `grant_root` and `revoke_root` are
   one row at a time, which is what `taiyaku library user grant-root` needed.
4. **No backup of any kind.** The only trace in the tree is a comment in
   `taiyaku-library/Cargo.toml` explaining why `rusqlite`'s `backup` feature is
   enabled.

## 1. Four decisions the build design left open

Settled with the owner, 2026-09-04. Each records what it beat.

### 1.1 An admin's password reset ends every session of the target's, and leaves its API keys

**Chosen: close all the target's sessions; do not touch its keys.**

A2 §4's *"a change keeps the session that made it"* is a rule about
**self-service** — the browser in front of the person doing the changing. When
an admin resets somebody *else's* password there is no session of the target's
that made it, so there is nothing to keep, and the case an admin reset exists
for is the compromised one. `set_password(.., keep: None)` is already exactly
this call.

**Beaten: keeping every session**, on the ground that a reset only changes a
hash. It leaves a compromised account logged in on every device it is on, which
is the failure the reset was meant to end.

**Also beaten: revoking the API keys alongside.** A2 already declines that for
self-service — *"bundling them would make rotating a password silently break
every reader in the house"* — and an admin reset is not a different enough
event to earn the opposite answer. An admin who wants the keys gone has
`DELETE .../keys/{kid}`, which is on this same screen and says what it is doing.

### 1.2 Root grants are written as a whole list in the `PATCH` body

**Chosen: `"roots": [1, 3]` inside `PATCH /api/library/users/{id}`,
replace-all.** The key **absent** means "leave the grants alone"; present means
"these exactly". The store diffs it against `granted_roots` and issues the
`grant_root`/`revoke_root` calls that differ.

C4 §6 lists no route for `user_root` at all, which is a gap in the settled spec
rather than a decision it made: §4 puts *"the roots they may see"* on the screen
and the API section forgets to give it a path.

**`"roots": []` means unrestricted, not blind**, which is A2's empty-grant rule
and is counter-intuitive enough that it gets its own test rather than a comment.
`grant_root`'s doc comment already states the rule and why it is forced: every
account in every existing library holds an empty grant, so any other reading
would have emptied every library the day migration 6 applied.

**Beaten: `PUT /api/library/users/{id}/roots`.** A fifth user route and a second
request per save, in exchange for keeping `PATCH` to scalar columns — a tidiness
argument with nothing behind it.

**Beaten: `POST`/`DELETE` per grant**, mirroring `grant_root`/`revoke_root`
exactly. A screen with three checkboxes then sends three requests and has no
atomic save, and a half-applied change to a permission set is the wrong thing to
be able to produce.

### 1.3 Backups live at `<db parent>/backups`, and `--keep N` prunes

**Chosen: a location derived from the database path, an `--out DIR` override the
server does not know about, and pruning only when asked.**

The load-bearing half is that **the location cannot be arbitrary, because the
server has to list it.** `GET /api/library/backups` answers "what exists", and
it can only answer for a directory it can compute. `Library::path()` already
exists and already returns the database file, so `directory_for(db)` is
`<db parent>/backups` — `$XDG_DATA_HOME/taiyaku/backups/` on the default path.

`--out DIR` stays as an escape hatch for an operator writing to another volume,
and **its help text says the server will not list what lands there.** A flag
whose effect is invisible from the screen that operates the feature is worth one
sentence.

`--keep N` deletes oldest-first and is **off unless given**. A command that
deletes files by default is a surprise the first time it runs, and the first run
is the one where the person is least sure what the command does.

**Beaten: no `--keep` at all**, leaving retention to the cron entry
(`find -mtime +30 -delete`). Least code, and it leaves store spec §13.4's
*"a retention count"* unbuilt while claiming the section is done.

**Beaten: `--keep 7` by default.** Safe in steady state; a deleting default.

### 1.4 Three backup routes: list, run now, download one

**Chosen: `GET /api/library/backups`, `POST /api/library/backups`,
`GET /api/library/backups/{name}`** — exactly C4 §5's *"run now, list what
exists, download one"*, minus the schedule build §1 already declined.

**Beaten: no download, fetch it with `scp`.** It drops the one thing §5 names
that a browser can actually do, on a product whose whole C4 arc is "the CLI
covers all of it in the meantime".

**Beaten: no `POST`, cron only.** Removes the button §5 leads with.

## 2. The refusals, and where each one lives

C4 §4 lists three refusals *"that stop this screen locking you out"*. They
resolve into **two invariants of the account table and one rule about the
actor**, and that split decides where each is enforced.

**Invariant 1 — nothing may leave the library with zero administrators.**
This covers §4's first refusal (the last admin cannot be deleted) *and* the
same lockout by the other route: demoting the last admin. Both go in the store,
beside `promote`'s identical rule, and both are reachable from the CLI as well
as from a route.

> **§4's two refusals are not two rules, and reading them as two is how you
> ship one.** *"The last admin cannot be deleted"* and *"an admin cannot remove
> their own admin role"* look like a pair about administrators. They are not:
> the first is about the **library's** state after the change and applies to
> any actor; the second is about **who** is making it and fires even when the
> library keeps an administrator. A single `if role_change && is_last_admin`
> satisfies the letter of both and enforces neither properly — it lets an admin
> demote themselves whenever a second admin exists, which §4 forbids, and it is
> silent on `DELETE`.

**Rule 2 — an admin cannot demote themselves, blanket.** Even with a second
admin present. That is C4 §4's letter (*"the same accident by a different
route"*) and it is the simpler rule to hold: "another admin has to do it" needs
no case analysis. It needs to know who is acting, so `update` takes
`acting_as: Option<i64>`; the route passes `Some(principal.user_id)` and the CLI
passes `None`, **documented as "the CLI has no session to be"** rather than left
to look like an oversight.

**Rule 3 — a password reset keeps the session that made it.** Already built
(§0). PR 2 adds the route that reaches it and the test that pins it.

## 3. The store: `account.rs` gains three writers

```rust
pub struct Patch {
    pub name: Option<String>,
    pub role: Option<String>,
    pub can_download: Option<bool>,
    pub can_stream: Option<bool>,
    /// `None` leaves the grants alone; `Some(ids)` replaces them wholesale,
    /// and `Some(vec![])` means unrestricted — see §1.2.
    pub roots: Option<Vec<i64>>,
}

pub fn update(conn: &Connection, id: i64, patch: &Patch, acting_as: Option<i64>)
    -> Result<Account, String>;

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

**`removal_cost` and `remove` are named after `scan::removal_cost` and
`scan::remove_root` on purpose.** Root removal and account removal are the same
shape — count first, then act, with a route that answers one body for both — and
PR 1 already chose `?dry_run=true` over a fifth route and over a server-held
confirmation token. Two destructive paths that look alike should read alike —
down to `Removal`'s field names and `usize`, so `scan::Removal`'s `positions`
and `notes` mean the same words here.

**`remove` counts inside the transaction that deletes**, exactly as
`scan::apply_removal` does and for the identical reason PR 1's `delete_root`
doc comment gives: counting afterwards answers zero for the numbers the route
exists to state honestly.

**What the counts cover, and what they deliberately do not.** Five tables
reference `user(id)` with `ON DELETE CASCADE`, across seven declarations —
`progress` and `note` from migration 1, `note` again from 005's rebuild,
`session`, `api_key` and `user_root` from 004, and `progress` again from 009's.
`Removal` names all five, as `positions`, `notes`, `sessions`, `keys` and
`grants`. **No work, edition or member is touched**, which is
the difference from root removal and is worth the screen saying: deleting a
person does not delete any of the library.

> **`PRAGMA foreign_keys` is ON per connection** (`lib.rs:339`), so the
> cascades fire. This is the pragma `004_auth.sql`'s own header warns is *"a
> no-op within a transaction"* — it is set at open, before anything begins one,
> which is why it works. A test that opened a bare `rusqlite::Connection`
> instead of going through `Library` would see no cascade and would pass a
> wrong `Removal` as correct.

`update` writes the scalar columns in one `UPDATE`, then reconciles the grants
against `granted_roots`. Both inside one transaction, so a refused role change
cannot leave a grant list half-written.

**`role` is validated by the schema**, not by this function:
`004_auth.sql` gives the column `CHECK (role IN ('admin', 'user'))`, added
because *"a typo stored a role that is neither and `Principal::from_account`'s
`== "admin"` quietly read it as a reader"*. `update` lets the constraint refuse
and maps the error, rather than growing a second list of the two words.

## 4. The store: a new `backup.rs`

```rust
pub struct Backup { pub name: String, pub bytes: u64, pub taken: String }

/// `<db parent>/backups`.
pub fn directory_for(db: &Path) -> PathBuf;
pub fn take(conn: &Connection, into_dir: &Path) -> Result<Backup, String>;
pub fn list(dir: &Path) -> Result<Vec<Backup>, String>;      // newest first
pub fn prune(dir: &Path, keep: usize) -> Result<Vec<String>, String>;
```

**`VACUUM INTO`, per store spec §13.4**, which takes a consistent snapshot of a
live database without stopping the server and needs no external tool. It
**refuses an existing target file**, so there is no clobber path to guard
against — the guard is SQLite's.

**The name is `library-YYYYMMDDTHHMMSSZ.db`, timestamped from SQLite's own
clock.** This workspace has no time crate — `docs/status.md` states the short
dependency list as a fact about the project — and every other timestamp in the
store comes from `datetime('now')`. `strftime('%Y%m%dT%H%M%SZ', 'now')` is one
`query_row` and adds nothing to `Cargo.toml`.

> **A second backup inside the same second collides**, and `VACUUM INTO`
> refuses rather than overwriting. That is the correct failure and it is
> reported as itself; a suffix counter would be machinery for a case only a
> test loop produces. Recorded so it is a known edge rather than a surprise.

**`prune` is the only file-deleting function in `taiyaku-library`, and it is
written like one.** It deletes only files in the one directory whose names match
the exact pattern `take` mints, never recurses, never follows a directory
symlink, and returns the list of what it removed so the CLI prints truth rather
than a count. A `--keep` pointed at the wrong directory must be able to do
nothing rather than something.

**`take` never runs on the server's connection.** `ServeState.library` is one
`Mutex<Connection>` for the whole process, and a `VACUUM INTO` on it stalls
every request for the length of a file copy. It runs on `Library::working()`
inside `spawn_blocking`, which is the same reasoning build §2 applies to the
preview and build §4 to the scan, and this is the third caller of it.

~~**Unmeasured, and stated as such:** what a `VACUUM INTO` of the real library
costs, and therefore whether the `POST` route should stream progress rather than
answer when it is done. The database on this machine is small enough that the
guess is "well under a second", and it is a guess.~~

> **Measured 2026-09-04, and the guess was right by two orders of magnitude.**
> On a copy of the real library — 130 series, 1858 works, 1883 members,
> 3,534,848 bytes — the shipped command takes a **median of 6 ms** over seven
> `--release` runs, whole-process wall clock, so the statement itself is some
> fraction of that. The same library's rule preview costs a median of 281 ms,
> so a backup is roughly **one fiftieth** of something the screen already runs
> on demand. The `POST` route stays synchronous and does not stream. Figures in
> `docs/notes.md`, "What a `VACUUM INTO` of the real library costs".

## 5. The CLI: `taiyaku library backup`

```
taiyaku library backup [--out DIR] [--keep N] [--db PATH]
```

Prints the path written, its byte count, and one line per file pruned. Joins the
synopsis block in `main.rs` and the command list in `README.md`, both of which
`docs/status.md` treats as part of the contract.

**No schedule, and that is build §1's decision restated rather than reopened:**
a cron entry runs this, the same call already made for the scan interval, so the
server gains no timer at all in C4. Store spec §13.4's *"a backup nobody
remembers to take is not one"* is answered by the command existing and being
cronnable.

## 6. The server: two new modules

**`config_api.rs` is 1872 lines and `config_api/tests.rs` is 3006.** Users and
backup would push the pair past 2500 and 4000. So this PR adds
**`users_api.rs`** and **`backup_api.rs`** rather than a third and fourth family
inside the file PR 1 built.

Both reuse **`config_api::admin_or_forbidden`**, which PR 1 already declared
`pub(crate)` and whose module doc already anticipated exactly this: *"the roots
and scan routes tasks 11–12 add are C4 routes too and reach this one directly
rather than each growing a third copy."* A third copy is what this design
declines.

```
GET    /api/library/users                       list; each with sessions, keys, grants
POST   /api/library/users                       create
PATCH  /api/library/users/{id}                  role, capabilities, password, roots
DELETE /api/library/users/{id}                  ?dry_run=true states its counts
DELETE /api/library/users/{id}/sessions/{sid}   sid is sessions_of's sha256 id
DELETE /api/library/users/{id}/keys/{kid}

GET    /api/library/backups
POST   /api/library/backups
GET    /api/library/backups/{name}
```

**Every one goes in both `guarded_routes()` and the grant-posture table beneath
it.** `every_route_declares_its_grant_posture` and
`every_route_refuses_an_anonymous_request` each fail on a route in only one, and
a route added to `open_routes()` instead ships unauthenticated.

**`{sid}` is the token's sha256**, which `SessionSummary.id` already is and which
`sessions_of`'s doc comment already argues is safe to hand a browser: *"it is
what the server compares against, not what a client presents, and inverting it
is the sha256 preimage problem."* `revoke_session` is already scoped to
`user_id` as well, so an id from one account's listing cannot end another's.

**The download route resolves `{name}` against `list`'s own answer and never
joins it onto the directory.** A path-traversal check on a filename is a thing
that gets written wrong; matching a requested name against a listing the server
produced has no such failure mode, and it costs one `readdir` that the route
would want anyway to answer 404 honestly.

**Stated here rather than found in review: a backup is the whole database,
password hashes included.** An admin who downloads one can read every account's
PHC string offline at their leisure. That is what a backup *is* — the same file
an operator would copy off the disk — and the route is admin-gated, so this is
a property to name rather than a hole to close. It is named because "the admin
screen has a download button" does not, on its face, look like "the admin screen
hands out the password file".

## 7. What PR 2 does **not** do

- **O58 and O60 do not close.** Build §1 already decided this: PR 2 ships the
  *writer* for `can_download`, `can_stream` and `user_root`, and what a root
  grant *means* for progress, notes and truth — and whether editing shared
  content counts as administering — are decisions with no incident behind them.
- **No API-key creation for another account.** C4 §4 says sessions and keys are
  *"both revocable individually by an admin"* and stops there. Minting a
  credential *for* somebody else, which then has to be shown to them out of
  band, is a different feature and nothing has asked for it.
- **No `api_key.expires` writer**, although `account.rs`'s own comment names C4
  as where a flag for it belongs. It is a fourth column on a screen that already
  gained three, `by_api_key` already honours the column, and adding it is an
  `ALTER`-free one-liner whenever something wants it. Recorded as declined
  rather than missed.
- **No self-service anything.** C4 §8 already declines it; this is the
  administrative view of other people.
- **No screens.** The four UI PRs follow these two.
- **No in-server schedule.** §5.

## 8. Testing

C4 §7's last line, plus what this design's shape adds:

- **The last admin cannot be deleted**, and **demoting the last admin is
  refused by the same rule** — two callers, one invariant, asserted separately
  so a fix to one cannot silently drop the other.
- **An admin cannot demote themselves with a second admin present**, which
  tests §2's rule 2 rather than accidentally re-testing invariant 1.
- **An admin's reset of another account's password ends every session that
  account has and leaves its API keys alone** — both halves, since the second is
  the one a "revoke everything" instinct would break.
- **Deleting a user states its position and note counts before acting**, and
  **the deleted account's live session is refused on its next request** — the
  cascade reaching the guard, not just the table.
- **`"roots": []` makes an account unrestricted rather than blind**, named after
  the rule so the test reads as documentation of A2's sharpest edge.
- **A backup restores**: take one, open the file as a library, read a work back
  out of it. Not "the file exists and is non-empty".
- **`prune(keep: 2)` over five backups deletes the three oldest and nothing
  else in the directory**, with an unrelated file present that must survive.
- **A `{name}` that is not in the listing is 404**, including one containing
  `..`, which must be refused as "no such backup" rather than by a path check.

## 9. What this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs`.
- `docs/status.md` — the test count at **both** of its sites, the route prose,
  and **O54's closing note that nothing can set the two capability columns
  outside a fixture**, which stops being true the day this merges.
- `README.md` — `taiyaku library backup` joins the command list.
- `main.rs`'s synopsis block — the same command.
- `docs/notes.md` — §4's unmeasured `VACUUM INTO` cost, when it is taken.

## 10. What could not be measured

- ~~**What a `VACUUM INTO` of the real library costs.**~~ **Measured
  2026-09-04** — a median of 6 ms on 130 series and 1858 works, against the
  rule preview's 281 ms on the same library. §4 carries the consequence
  (the `POST` route stays synchronous); `docs/notes.md` carries the figures.
  What is still unmeasured is the library of *thousands* the C4 product design
  §11 names, which the preview measurement leaves open too.
- **Whether a second connection's `VACUUM INTO` is as invisible to the server's
  readers as WAL promises.** Build §9 carries the same open question for the
  preview's write transaction, from the same reasoning about the mode rather
  than a measurement under load. Both are cheap to check together and neither
  has been.

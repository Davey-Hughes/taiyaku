# Pre-deploy lane 3: scheduled scans and O186 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close O186 and narrow O51.
- The server scans each directory on a schedule of its own: Automatic, Every (at least 5 minutes) or Off. It also scans every directory that is not Off once at startup.
- A scan cut during its derive reads `interrupted` after a restart, not `ok`, until its next scan. For a directory that is not Off, the startup scan is that next scan.

**Architecture:**
- Task 1: the schedule's rule in the library (`taiyaku_library::schedule`). The add stops writing the probe's interval. The server refuses an interval under the minimum, and `root_json` sends `next_scan_at`. Task 1 also commits this plan.
- Task 2: O186. `scan.rs` writes `deriving` and `interrupted`. `scan_jobs::run_with` takes the walk and the derive as arguments. `serve` rewrites `deriving` to `interrupted` before the bind (`scheduler::interrupt_cut_derives`).
- Task 3: `Reread::Batch`.
- Task 4: the scheduler's task, spawned by `serve`, and the README.
- Task 5: the web's facts: `next_scan_at`, "scan every" and "next scan", the Scan every column, and `WIDEST_ENDS`.
- Task 6: the directory page's Scan schedule control.
- Task 7: the closes in `docs/status.md`.
- Then the Finish.

There is no migration, no watcher and no `ServeState` field. The CLI is unchanged.

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`), TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`.

**Spec:** `docs/superpowers/specs/2026-11-10-pre-deploy-design.md`, §1 (lanes and coordination) and §4 (this lane). §4 builds Part C, which is §4 of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`, with the deltas §4 states. They are cited as "spec §4, <subsection>" and "Part C §4.n". Pre-flight: `.superpowers/sdd/pre-deploy-evidence/daily-preflight.md` §1. Locations are by symbol. Line numbers are hints taken at main 10ae1e74.

**The owner's rulings of 2026-11-10** (binding; do not re-open; `.superpowers/sdd/pre-deploy-evidence/rulings.md`):
- **Ruled:** the schedule is per directory. It is one setting, `root.scan_interval_secs`, shared by every library that holds the directory. The libraries design's decision 6 is superseded, and that spec already says so.
- **Told to the owner, no objection:**
  - Off is stored as 0, and Automatic stays NULL.
  - Every is at least 5 minutes, enforced by the server.
  - The file watcher waits for a NAS measurement.

**The spec's own calls that this plan builds** (spec §4, Decisions 4 and 6-11):
- A tick batches the due directories into one job, through the scan registry and the job lock.
- The server's job writes `deriving` before the derive, and startup rewrites it to `interrupted`.
- The new words join `WIDEST_ENDS`.
- An Off directory is not in the startup scan.
- A batch goes on past a directory it cannot walk.
- The CLI's `library scan` writes no `deriving`.
- The probe's interval stays in the add's answer and in the CLI's line.

**What the plan settles** (the controller's calls; they are told to the owner and are not rulings):
- **`next_scan_at` is computed from the row alone**, in `schedule::next_scan_at`, with the calendar arithmetic done in Rust.
  - Why: `root_json` is drawn by `added_json`, which has read nothing since O254, so SQLite's `datetime()` would need a connection there.
  - Computed from `last_scan_at` and `scan_interval_secs`, `root_json` stays a function of the row, and no caller changes. That also keeps lane 4's conflict in `root_json` to one line.
  - `now` still comes from the store's clock (`schedule::now`), the one that writes `last_scan_at`.
- **A `last_scan_at` that does not read as `YYYY-MM-DD HH:MM:SS`** gives no `next_scan_at`. The scheduler counts such a directory due, as it counts one never scanned. Its scan then writes a time that does read.
- **The startup tick lasts until it lands.** A `Scheduler`'s first tick takes every directory that is not Off.
  - It stays the first tick until a tick has read the store and has either reserved its batch or found nothing to start.
  - So a failed read, or a startup batch refused because a press raced it, is tried again whole at the next tick. Spec §4, "The scheduler", says a refusal leaves the batch to the next tick.
- **A batch that walked nothing derives nothing.** Every slot then holds its own error.
- **A batch's failed derive** answers every walked directory's slot with this sentence, `{why}` being the derive's error:
  `the directories a scheduled scan walked were scanned, but deriving their structure afterward failed: {why}`
- **The minimum's 400** reads, with `300` and `5` taken from `schedule::MIN_INTERVAL_SECS`:
  ``the directory's `scan_interval_secs` must be null (automatic), 0 (off) or at least 300 (5 minutes)``
- **`serve` gets two one-line calls into the new `scheduler` module, not one.**
  - Spec §1 and §4 put the rewrite before the bind, beside the session sweeps.
  - They put the spawn after the bind, beside `registry::spawn_refresher`, for a live registry only.
  - Lane 5 adds only a spawn in that block, so the rebase is a text conflict on adjacent lines.
- **The tick is 30 s** (`scheduler::TICK`), from the pre-flight's C-5. A `const` assertion holds it under the minimum.
- **A tick that fails prints a line on stderr each time:** `taiyaku: a scheduled scan could not start: {why}`.
  - It goes through `writeln!(std::io::stderr(), …)`, as `registry::spawn_refresher` prints, never `eprintln!`, which panics when nothing reads stderr.
  - A panic in a tick is printed as `taiyaku: a scheduled scan panicked: {panic}`, and the loop goes on. The tick after a panic is a startup tick.
- **The startup rewrite's lines.** It prints only when it marked something: `marked {n} directory scan(s) cut during their derive as interrupted`. Its failure prints `could not mark directory scans cut during their derive as interrupted: {why}`. Like the session sweeps, it is reported and never fatal.
- **A job that panics** leaves what it had marked `deriving` until its directory's next scan, or until the next start rewrites it to `interrupted`. `spawn_job`'s catch writes no status, as today.
- **The page draws a stored interval as it is stored**, including one under the minimum. Only a database written before this lane can hold one, and "next scan" shows when the scheduler will run it. A stored value of 0 or less draws `off`.
- **The facts' words:**
  - "scan every" reads `automatic`, `off`, or the interval (`humanInterval`).
  - "next scan" comes after "last scan" (Part C §4.3's order). It reads `off`, `due` (never scanned), or the time (`inUtc`).
  - The library page's Scan every column reads the same words as "scan every".
- **The control** (Task 6) is three radios. The number and its unit are drawn beside Every only while Every is chosen.
  - Choosing Automatic or Off saves at once.
  - Every saves when its box is left, or when its unit is changed.
  - The owner sees it at the drive and may ask for another shape.

**Spec gaps** (written down here, not resolved silently):
1. **What a walk that failed before a later one ends with.**
   - Spec §4, "The scheduler", says `Reread::Dir` and `Reread::Library` "mark the directories walked before it `failed`" when a later walk fails.
   - Spec §4, "O186", says `mark_derive_failed` "now writes only where the status reads `deriving`: a refused walk keeps `refused`".
   - The two disagree for an earlier walk that was refused. The plan follows the guard: that walk keeps `refused`, since it recorded nothing and so no derive is missing for it.
   - The earlier walks that read `deriving` still become `failed`. Task 2's tests pin the guard. No test pins this path's refused case.

**New interfaces this plan names** (this plan is where they are first written):

- `crates/taiyaku-library/src/schedule.rs` (new; `pub mod schedule;`):
  - `pub const MIN_INTERVAL_SECS: i64 = 300;`
  - `pub fn every(stored: Option<i64>) -> Option<i64>`: the interval a directory is scanned at, in seconds, from its stored `scan_interval_secs`.
    - `None` (Automatic) gives `watch::UNWATCHED_INTERVAL` in seconds.
    - 0 or less gives `None` (Off).
    - 1 to `MIN_INTERVAL_SECS - 1` gives `MIN_INTERVAL_SECS`.
    - A larger value gives itself.
  - `pub fn next_scan_at(last_scan_at: Option<&str>, stored: Option<i64>) -> Option<String>`: `last_scan_at` plus `every(stored)`.
    - Read and written in SQLite's `datetime()` shape, `YYYY-MM-DD HH:MM:SS`, UTC, on the proleptic Gregorian calendar.
    - `None` for Off, for `None`, and for a stamp not in that shape.
  - `pub struct Scheduled { pub root: scan::Root, pub due: bool }`
  - `pub fn scheduled(conn: &Connection, now: &str) -> Result<Vec<Scheduled>, String>`:
    - It answers every directory whose `every` is not `None`, sorted by path as `reach::walked_by` sorts.
    - `due` is true where `next_scan_at` is `None`, or is at or before `now`. Both are in the `datetime()` shape, which is fixed-width, so they are compared as text.
    - It reads `scan::roots` and each row's `last_scan_at` inside one `crate::one_snapshot`.
  - `pub fn now(conn: &Connection) -> Result<String, String>`: `SELECT datetime('now')`.
- `crates/taiyaku-library/src/watch.rs`: `UNWATCHED_INTERVAL` becomes `pub(crate)`.
- `crates/taiyaku-library/src/scan.rs`, beside `mark_derive_failed`. The module keeps its rule that it alone writes `last_scan_status`.
  - `pub fn mark_deriving(conn: &Connection, root: &Root) -> Result<(), String>`: `deriving` where the row is `root.id` at `root.path` and reads `ok`.
  - `pub fn mark_derived(conn: &Connection, roots: &[Root]) -> Result<(), String>`: `ok` where each row reads `deriving`, with the same id-and-path guard.
  - `pub fn interrupt_derives(conn: &Connection) -> Result<usize, String>`: rewrites every `deriving` to `interrupted`, and answers how many it rewrote.
  - `mark_derive_failed` writes only where the status reads `deriving`.
- `crates/taiyaku-serve/src/scan_jobs.rs`:
  - `Reread::Batch`.
  - `pub(crate) fn run_with(library, asked, reread, job, walk, derive) -> Result<Vec<serde_json::Value>, String>`.
    - `walk: impl FnMut(&Connection, &scan::Root) -> Result<scan::ScanReport, String>`.
    - `derive: impl FnOnce(&Connection) -> Result<derive::DeriveReport, String>`.
    - `run` becomes `run_with(…, scan::scan_root, derive::derive)`. `schema::migrate_with` is the model.
- `crates/taiyaku-serve/src/scheduler.rs` (new; `mod scheduler;`):
  - `pub(crate) fn interrupt_cut_derives(library: &taiyaku_library::Library)` (Task 2).
  - `pub(crate) const TICK: Duration = Duration::from_secs(30);` (Task 4).
  - `pub(crate) struct Scheduler { first: bool }` and `Scheduler::new()` (Task 4).
  - `pub(crate) fn tick(&mut self, state: &ServeState, now: &str, mut start: impl FnMut(Vec<scan::Root>, Instant)) -> Result<(), String>` (Task 4).
    - `start` is `FnMut` and not `FnOnce` so that a test can count the jobs. A defect that started each directory alone then fails a count rather than the type check.
  - `pub(crate) fn spawn(state: Arc<ServeState>, every: Duration) -> tokio::task::JoinHandle<()>` (Task 4).
- The wire: `root_json` gains `"next_scan_at": <string | null>`.
- The web:
  - `Root` gains `next_scan_at: string | null`.
  - `config.ts` gains `scanEvery(root)` and `nextScan(root)`, which give the words above.

## Global Constraints

- **Parallel lanes.**
  - Five lanes run at once, each in its own git worktree on its own branch, cut from main 10ae1e74.
  - This lane's branch is `pre-deploy-scans`. `W` is its worktree, the absolute path the controller gives each dispatch.
  - `E=/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/lane-3-scans`. It is absolute and in the MAIN checkout, so reports survive the worktree.
  - Every command runs against `$W` by absolute path. Never `cd` (it is zoxide here). Where a tool must start inside a directory, use `builtin cd <dir> && …`.
  - Lane 4 also edits `scan.rs`, `config_api.rs` and web `config.ts`, and lane 5 edits `serve`'s live-registry block. Keep this lane's edits there to the symbols named, so the rebase stays text-only.
- **Target dirs.** Never set `CARGO_TARGET_DIR`. Each worktree builds into its own `$W/target`.
- **Web tools.** `$W/web/node_modules` is a real copy, never a symlink. Never `pnpm exec`; call `./node_modules/.bin/<tool>` directly.
- **Commits.** One commit per task, with `git -C $W`, its subject naming the O-number(s). Never `--amend`.
- **Comments.** A code comment says why: the reason, the invariant, what breaks if it changes.
  - It never carries history or measurements. It may point at an O-number or a spec section.
  - `scripts/check-comments.mjs` enforces part of this in the gate, including each area's share in `scripts/comment-budget.json`. If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **Rust, per task.**
  - `cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/T<n>-nextest-<crate>.log 2>&1`, for each crate the task touches. Read the summary lines from the log.
  - Then `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings`.
  - Then `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`.
- **Web, per task.**
  - Tests: `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`. Under Claude Code, vitest otherwise picks a reporter that hides passing output.
  - Then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests` the same way.
  - Run whole projects, never a named file.
  - A task that touches a `*.browser.test.tsx` file, or any CSS, runs the whole browser project. CI never runs it.
- **Browser crashes.** A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing.
  - Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
  - The Filmstrip "Tab brings focus to the rail" case also goes red under load, on main too.
- **The gate.** Run the repo's own `scripts/check-all.sh`, never a copy.
  - It changes to `git rev-parse --show-toplevel` of the directory it starts in, so start it inside `$W`:
    `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 ./scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`
  - Wait for load < 24 (`cat /proc/loadavg`) before starting it, and record the load.
  - Read the `.exit` file, never the harness's exit code. Quote the `all green` line. Never pipe the gate through `tail`.
  - **Run the gate first in Task 7.** Spec §1 has this lane edit `docs/status.md` only in its last task. Until then the Rust test count there is stale, and the gate's `status-test-count` step fails.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at main 10ae1e74, not at another commit of this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one.
  - Put the named defect back by hand, and watch the test fail.
  - Restore, and check with `git -C $W diff` that the restore is complete.
  - Record each in the task report: the defect, the failing test, and the assertion's message.
  - A test that names a guard must reach it. Check that the fixture tells right from wrong before trusting a green run.
- **Sentences are literals.** A test asserts a user-facing sentence written out in the test, never one computed through the helper under test.
- **Real interaction.** Web tests that navigate do it outside `act`, with real clicks. A real-key browser test focuses the page first.
- **No `ServeState` field** (spec §1). Each task is handed what it needs as an argument.
- **SQL.** Task 2 adds `UPDATE` statements. Task 4's last step runs the library and serve crates' tests under SQLite 3.40.1 in `rust:1-bookworm`.
- **Reports.** Each task's report goes in `$E/T<n>-report.md`, with its logs in `$E`. Write them by Bash (`cat >| … <<'EOF'`), never under `target/`. The Write tool is refused under `.superpowers/`.
- **Order.** The tasks run one after another.

---

### Task 1: The schedule's rule, the minimum, and `next_scan_at` (O51)

Spec §4: "The setting", the first bullet of "The facts and the screen", and Decisions 2, 3 and 11. Pre-flight §1.5 and §1.9.

**Files:**
- `docs/superpowers/plans/2026-11-10-pre-deploy-scheduled-scans.md`: this plan, copied from `/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/plans/`.
- New `crates/taiyaku-library/src/schedule.rs` and `crates/taiyaku-library/src/schedule/tests.rs`.
- `crates/taiyaku-library/src/lib.rs`: the module list (~11-45).
- `crates/taiyaku-library/src/watch.rs`: `UNWATCHED_INTERVAL` (~39).
- `crates/taiyaku-library/src/scan.rs`:
  - `probed` (~815) stops calling `update_root`.
  - Docs that say the add writes the probe's interval:
    - `Added`'s doc and `Added::probe`'s doc (~366-375);
    - `AddRootError::Failed` (~414-417);
    - `add_root_checked_in`'s third item (~462-470).
  - A link still runs no probe, as the overlapping-libraries PR 3 settled. `Added::probe`'s doc gives a reason that holds after this task: the probe writes a file into the directory, and the directory was probed when it was first added.
- `crates/taiyaku-library/src/scan/tests.rs`:
  - The two flips: `adding_a_root_returns_the_same_interval_it_wrote_to_the_row` (~1605) and `adding_a_root_writes_the_probes_interval_onto_the_stored_row` (~1624).
  - `adding_a_root_probes_the_watcher_before_the_first_scan` (~1578) keeps its probe half and is not edited.
- `crates/taiyaku-serve/src/config_api.rs`: `root_patch_from_json` (~1261), and `root_json` (~1201) with its doc.
- `crates/taiyaku-serve/src/config_api/tests.rs`:
  - `a_roots_patch_leaves_scan_interval_secs_alone_when_omitted_but_clears_it_on_null` (~2125) patches 120, which this task refuses. Make it 600.
  - `the_root_list_carries_the_interval_and_the_last_scan` (~1527) is the model for the new list test.
- `crates/taiyaku-serve/src/libraries_api/tests.rs`: the exact key list of a directory (~375-388) gains `next_scan_at`.

- [ ] **Step 1: The plan.** Copy this plan to `$W/docs/superpowers/plans/2026-11-10-pre-deploy-scheduled-scans.md`. It goes in this task's commit.
- [ ] **Step 2: Write the library tests first** in `schedule/tests.rs`. Put each directory's interval and `last_scan_at` in place by SQL. Make the directories with `scan::add_root` over temp directories (`crate::tests::tempdir`).
  - **The due rule** (`scheduled`), with `now` written out as a literal:
    - Automatic and never scanned: listed, due.
    - Automatic with `last_scan_at` `2026-11-10 12:00:00`: not due at `2026-11-10 12:14:59`, due at `2026-11-10 12:15:00`.
    - Every 3600 from the same time: not due at `12:59:59`, due at `13:00:00`.
    - Off (0), never scanned: not listed.
    - Stored 60 from `12:00:00`: not due at `12:01:00`, due at `12:05:00`.
    - Stored -1, never scanned: not listed.
    - Order: two directories added so that id order and path order differ (`…/z` first, then `…/b`) come back by path.
  - **`next_scan_at`**, pure:
    - `("2026-11-10 12:00:00", None)` gives `2026-11-10 12:15:00`.
    - `(None, None)` gives `None`, and `("2026-11-10 12:00:00", Some(0))` gives `None`.
    - `("2026-12-31 23:55:00", Some(900))` gives `2027-01-01 00:10:00`.
    - `("2028-02-28 23:55:00", Some(600))` gives `2028-02-29 00:05:00`.
    - `("2100-02-28 23:55:00", Some(600))` gives `2100-03-01 00:05:00`.
    - `("2026-11-10 12:00:00", Some(60))` gives `2026-11-10 12:05:00`.
    - `("not a time", None)` gives `None`.
  - **`now`:** `next_scan_at(Some(&now), Some(300))` is `Some`, so the store's clock reads in the shape the rule parses.
  - Run them. They fail: the module does not exist yet.
- [ ] **Step 3: Implement `schedule.rs`.** Its doc comments say why:
  - Automatic is the unwatched interval until the watcher ships (spec §4, "The setting").
  - The minimum keeps the job lock free most of the time (Decision 3).
  - `next_scan_at` is computed in Rust because `root_json` has no connection.
  - Text comparison holds because both sides are `datetime()`'s fixed-width shape.
  - Run the library tests to green.
- [ ] **Step 4: The add writes no interval.**
  - Flip the two tests first, and rename each for what it now holds:
    - `adding_a_root_writes_no_interval_onto_the_stored_row`: the stored row's `scan_interval_secs` is `None`, while `added.interval` is `Some`.
    - `adding_a_root_returns_the_row_it_stored`: `added.root.scan_interval_secs` is `None`.
  - Run them. They fail on today's code.
  - Change `probed`, and fix the docs named under Files. If `probed` can no longer fail, simplify its signature where clippy asks.
  - Run the tests to green.
- [ ] **Step 5: The server's minimum.** In `config_api/tests.rs`, beside the interval patch test:
  - `a_roots_patch_refuses_an_interval_under_five_minutes_and_one_below_zero`:
    - Set the column to 600, then PATCH 299, then -1.
    - Each answers 400 with exactly this `error`: ``the directory's `scan_interval_secs` must be null (automatic), 0 (off) or at least 300 (5 minutes)``.
    - The column still reads 600 after each.
  - `a_roots_patch_stores_off_automatic_and_five_minutes`: PATCH 300, then 0, then null. Each answers 204, and the column reads 300, 0 and NULL in turn.
  - Make the 120 in the existing test 600.
  - Run them. The first fails on today's code.
  - Add the check to `root_patch_from_json`, after `patch_nullable_i64`, reading `schedule::MIN_INTERVAL_SECS`. Run to green.
- [ ] **Step 6: `root_json` gains `next_scan_at`**, from `schedule::next_scan_at(last_scan_at, root.scan_interval_secs)`.
  - Test `the_root_list_says_when_each_directory_is_scanned_next`, over `test_support::state_with_two_scanned_roots`:
    - By SQL, give the first root `scan_interval_secs = NULL` and `last_scan_at = '2026-11-10 12:00:00'`, and the second `last_scan_at = NULL`.
    - `GET /api/library/roots`: the first's `next_scan_at` is `"2026-11-10 12:15:00"`, and the second's is `null`.
    - Then set the first to 0: its `next_scan_at` is `null`.
  - Add `next_scan_at` to the key list in `libraries_api/tests.rs`. The libraries list carries it through `root_json`.
- [ ] **Step 7: Watched mutations.** Restore after each.
  - `<=` made `<` in the due comparison: the 12:15:00 case fails.
  - 0 read as Automatic: the Off case fails.
  - Automatic reading the probe's watched interval (`watch::WATCHED_INTERVAL`): the 12:15:00 case fails.
  - The stored value used under the minimum: the 12:01:00 case fails.
  - A negative value read as Automatic: the -1 case fails.
  - Sorted by id: the order case fails.
  - Every year a leap year: the 2100 case fails. No leap years: the 2028 case fails.
  - `probed`'s write restored: both flipped tests fail.
  - The server's check made `<= 0`, the web's old rule: the 299 and the 0 cases fail.
  - The check made `< MIN_INTERVAL_SECS`, which forgets that 0 is Off: the 0 case fails.
  - `next_scan_at` reading a stored NULL as no interval: the list test's first root fails.
- [ ] **Step 8: Run** nextest for `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` (the CLI reads `Added`), then clippy and fmt. `rg -n 'interval.*probe|probe.*interval' $W/crates/taiyaku-library/src/scan.rs`: read every hit, and none may say the add writes it.
- [ ] **Step 9: Commit:** "The schedule's rule in the library, a 5-minute minimum the server enforces, next_scan_at on every directory, and an add that writes no interval (O51)". Write `$E/T1-report.md`.

---

### Task 2: A scan cut during its derive reads `interrupted` (O186)

Spec §4, "O186", and Decisions 6 and 10. Spec §4, Testing: "O186 in the job", "A failed derive" and "O186 at startup".

**Files:**
- `crates/taiyaku-library/src/scan.rs`:
  - The three new functions beside `mark_derive_failed` (~2366).
  - `mark_derive_failed`'s SQL and its doc.
  - `scan_root`'s doc (~2273-2290), which says `last_scan_status` is "one of three words". It now names the two the server's job adds through the functions beside it.
- `crates/taiyaku-library/src/scan/tests.rs`:
  - `mark_derive_failed_corrects_a_status_a_successful_scan_already_wrote` (~1539).
  - `mark_derive_failed_leaves_the_dir_that_took_a_removed_ones_id_alone` (~4859).
  - The fixtures `stale_root_and_the_dir_that_took_its_id` (~4791) and `status_and_entries` (~4809).
  - `a_refused_empty_scan_records_the_refusal_rather_than_nothing` (~393), which is the model for a refused walk.
- `crates/taiyaku-serve/src/scan_jobs.rs`:
  - `run` (~144) becomes `run_with` plus a `run` that calls it.
  - `run`'s doc: "A derive failure also corrects…" and "A walk that fails ends the scan".
  - A new `#[cfg(test)] mod tests;` and `crates/taiyaku-serve/src/scan_jobs/tests.rs`.
- New `crates/taiyaku-serve/src/scheduler.rs`, with its module doc and `interrupt_cut_derives`.
- `crates/taiyaku-serve/src/lib.rs`:
  - `mod scheduler;` in the module list.
  - In `serve`, one call, `scheduler::interrupt_cut_derives(&state.library);`. It goes after `sweep_lapsed_attempts`' `match` (~590-595) and before `shutdown_signal()` and the bind. Its comment says why it comes before the bind: only this server writes `deriving`, and no scan of its own has begun.
- `crates/taiyaku-serve/src/tests.rs`: a serve test beside `serve_binds_the_address_and_answers_on_it` (~386), shaped like it.
- `crates/taiyaku-serve/src/config_api/tests.rs`: `a_derive_failure_after_a_successful_scan_corrects_last_scan_status` (~2211) must pass unchanged. It holds the `Reread::Dir` path.

**Behaviour:**
- `run_with`:
  - It marks each directory `deriving` (`mark_deriving`) right after its walk answers `Ok`. A stop between two walks therefore reads `interrupted` too.
  - Once the derive returns `Ok`, it marks the walked directories `ok` (`mark_derived`).
  - A failed derive, and a failed later walk, still go through `mark_derive_failed`, which now writes only over `deriving`.
  - The status writes are best-effort, as `scan_root`'s own write is, and a comment says why.
- `interrupt_cut_derives` calls `scan::interrupt_derives` on `library.conn()` and prints the lines settled above.
- The CLI is not touched. `rg -n 'mark_deriving|mark_derived|interrupt_derives' $W/crates/taiyaku-cli` has no hit.

- [ ] **Step 1: Write the library tests first** (`scan/tests.rs`):
  - `rewriting_cut_derives_marks_only_deriving_interrupted`:
    - Three directories, set by SQL to `deriving`, `ok` and `failed`.
    - `interrupt_derives` answers 1. The first reads `interrupted`, and the other two are unchanged.
  - `marking_deriving_or_derived_leaves_the_dir_that_took_a_removed_ones_id_alone`, over `stale_root_and_the_dir_that_took_its_id`:
    - Set the new directory to `ok`. After `mark_deriving(stale)` it still reads `ok`.
    - Set it to `deriving`. After `mark_derived(&[stale])` it still reads `deriving`.
  - Change `mark_derive_failed_corrects_a_status_a_successful_scan_already_wrote` to call `mark_deriving` before `mark_derive_failed`.
  - Change `mark_derive_failed_leaves_the_dir_that_took_a_removed_ones_id_alone`: mark the new directory `deriving` before the call, and assert it still reads `deriving` after. A status left NULL passes that test with the id-and-path guard removed.
  - Run them. They fail: the functions do not exist yet.
- [ ] **Step 2: Implement the three functions and the guard.** Run the library tests to green.
- [ ] **Step 3: Write the job tests** in `scan_jobs/tests.rs`.
  - Fixtures: two page directories A and B under one `test_support::Fixture`, with A before B by path. Use `test_support::write_pages` and `scan::add_root` on `test_support::library()`. Call `run_with` with `Reread::Dir` over `[A, B]`, which re-reads each directory asked for.
  - Drop every `library.conn()` guard the test holds before calling `run_with`. An in-memory library's `working()` takes the same mutex, and a held guard deadlocks the test. Read statuses inside the injected closures from the `&Connection` they are handed.
  - A refused walk is a directory scanned once, then emptied on disk and made again, as in the library's refused-scan test.
  - `each_directory_reads_deriving_from_its_walk_until_its_derive_ends`:
    - The injected walk reads A's status from the `&Connection` it is handed, when it is called for B, then calls `scan::scan_root`.
    - The injected derive reads A's and B's statuses, then calls `derive::derive`.
    - Expect A `deriving` inside B's walk, both `deriving` inside the derive, and both `ok` after `run_with`.
  - `a_failed_derive_marks_failed_only_the_walks_that_read_deriving`:
    - A refused walk and an ok walk, then an injected derive that answers `Err`.
    - A reads `refused`, and B reads `failed`.
  - `a_derive_that_succeeds_leaves_a_refused_walk_refused`: the same fixture with the real derive. A reads `refused`, and B reads `ok`.
  - Run them. They fail: `run_with` does not exist yet.
- [ ] **Step 4: Implement `run_with`, its marks, and `run`.** Update `run`'s doc. Run `taiyaku-serve`'s nextest to green, including the config test named under Files.
- [ ] **Step 5: The startup rewrite.**
  - Test `serve_marks_a_scan_cut_during_its_derive_interrupted_before_it_listens`, in `tests.rs`:
    - Keep an `Arc` of `test_support::library()`, and add two directories to it. Set them by SQL to `deriving` and `ok`.
    - Serve it with `crate::Works::fixed(…)`, so no scheduler runs.
    - Poll until the server answers, as the bind test does. Then the first reads `interrupted` and the second `ok`.
  - Run it. It fails.
  - Create `scheduler.rs` with `interrupt_cut_derives`, and add the call to `serve`. Run it to green.
- [ ] **Step 6: Watched mutations.** Restore after each.
  - `deriving` written for all walks once the walks are done: the walk case fails, since A reads `ok` in B's walk.
  - `deriving` written after the derive: the walk case fails inside the derive.
  - `mark_derived` dropped: the walk case fails after `run_with`.
  - `mark_derived` made unconditional: the succeeding-derive case fails, since A reads `ok`.
  - `mark_deriving` made unconditional: both refused cases fail.
  - `mark_derive_failed` writing whatever the status reads, as today: the failed-derive case fails, since A reads `failed`.
  - The path half of each guard dropped: the stale-directory cases fail.
  - `interrupt_derives` unconditional: the library case fails.
  - The call in `serve` removed: the serve case fails. The call moved into the live-registry block: the serve case fails, since its registry is fixed.
- [ ] **Step 7: Run** nextest for `taiyaku-library` and `taiyaku-serve`, then clippy and fmt.
- [ ] **Step 8: Commit:** "The server's scan job marks a directory deriving until its derive ends, and serve marks a scan cut during its derive interrupted before it listens (O186)". Write `$E/T2-report.md`.

---

### Task 3: `Reread::Batch`: a scheduled scan goes on past a directory it cannot walk

Spec §4, "The scheduler": the `Reread::Batch` bullet and Decision 9. Spec §4, Testing: "A batch past a failed walk".

**Files:**
- `crates/taiyaku-serve/src/scan_jobs.rs`:
  - `Reread` and its docs, `read_again`, `walk_failed`, `derive_failed`, and `run_with`.
  - The module doc names the scheduler's batch beside the directory and library routes.
- `crates/taiyaku-serve/src/scan_jobs/tests.rs`.

**Behaviour:**
- `Reread::Batch` answers one value per directory asked for, in the order asked. `spawn_job` already lands each value on its own slot.
- A directory that no longer holds its id at its path, once the job holds the lock, is not walked. Its slot gets `{"error": DIR_CHANGED}`.
- A walk that fails is recorded by `scan_root` as `failed`. Its slot gets `{"error": <the walk's own error>}`, and the next directory is walked.
- A walk that answers `Ok` is marked `deriving`.
- With at least one directory walked, the derive runs once:
  - If it answers `Ok`, the walked directories are marked `ok`, and each walked slot gets `finished_scan_json(its report, &derived)`.
  - If it answers `Err`, the walked directories go through `mark_derive_failed`, and each walked slot gets `{"error": "the directories a scheduled scan walked were scanned, but deriving their structure afterward failed: {why}"}`.
- With nothing walked, nothing is derived.
- `Err` from `run_with` is kept for what fails the whole job: the working connection, or reading the directories. `reports_of` already answers that on every slot.
- `Reread::Dir` and `Reread::Library` are unchanged.

- [ ] **Step 1: Write the failing tests** in `scan_jobs/tests.rs`, over two directories A and B, with A before B by path. Task 2's fixtures and its rule on held guards apply.
  - `a_batch_walks_past_a_directory_it_cannot_walk`:
    - Delete A's directory on disk after it is added. The walk then fails for any uid. CI runs as root, so a `chmod` fixture would prove nothing there.
    - `run(Batch)`: A's value is an `error` string starting with A's path, B's value has `scan` and `derive`, A reads `failed` and B reads `ok`.
  - `a_batch_answers_a_directory_changed_while_it_waited_on_its_own_slot`:
    - Remove A from the store (`scan::remove_root`) after asking.
    - A's value is exactly `{"error": "the directory was removed or changed while the scan waited to start, so nothing was scanned"}`, and B is walked.
  - `a_batch_derives_once_over_what_it_walked`: `run_with`, with a derive that counts its calls and then calls `derive::derive`. It is called once, and both values carry `derive`.
  - `a_batch_says_a_failed_derive_without_naming_a_directory`:
    - The derive answers `Err("the rules could not be read")`.
    - Each value is exactly `{"error": "the directories a scheduled scan walked were scanned, but deriving their structure afterward failed: the rules could not be read"}`.
    - Both read `failed`.
  - `a_batch_that_walked_nothing_derives_nothing`: remove both from the store. The counting derive is called 0 times, and each value is the changed-directory sentence above.
  - Run them. They fail: `Reread::Batch` does not exist yet.
- [ ] **Step 2: Implement.** Run `taiyaku-serve`'s nextest to green.
- [ ] **Step 3: Watched mutations.** Restore after each.
  - The batch ending at the first failed walk, as `Reread::Dir` does: the first case fails, since B is not walked.
  - A changed directory cancelling the whole batch, as `Reread::Dir` does: the second case fails.
  - A derive per walked directory: the third case fails.
  - `Reread::Dir`'s derive words: the fourth case fails.
  - A derive run when nothing was walked: the fifth case fails.
- [ ] **Step 4: Run** clippy and fmt.
- [ ] **Step 5: Commit:** "A scheduled scan's batch walks past a directory it cannot walk, answers each directory on its own slot and derives once (O51)". Write `$E/T3-report.md`.

---

### Task 4: The scheduler, and the README (O51)

Spec §4, "The scheduler" (from "It wakes on a fixed tick" on), Decisions 4 and 8, and the README bullet of "The facts and the screen". Spec §4, Testing: "One job", "A busy directory", "Startup" and "A change between ticks".

**Files:**
- `crates/taiyaku-serve/src/scheduler.rs` (Task 2 made it): `TICK`, `Scheduler`, `tick` and `spawn`, and `crates/taiyaku-serve/src/scheduler/tests.rs`.
- `crates/taiyaku-serve/src/lib.rs`:
  - In `serve`'s `if state.works.is_live()` block (~623-626), one line after `registry::spawn_refresher`: `scheduler::spawn(Arc::clone(&state), scheduler::TICK);`.
  - `ServeState::scans`' doc (~126-141) names the scheduler among the registry's users.
- `crates/taiyaku-serve/src/scan_jobs.rs`: `reserve` and `a_scan_reserved_on` ask a slot whether it is reserved and not done. Share that test with the tick if a helper reads better.
- `crates/taiyaku-serve/src/tests.rs`: the startup case through `serve`.
- `README.md`: the aside that ends the `keeps_documents` paragraph (~512-513).

**Behaviour of `tick`** (spec §4, "The scheduler"):
1. Read `schedule::scheduled(&state.library.conn(), now)`. The connection's guard drops before anything below, since a job's `Library::working` takes the same mutex where the library is in memory.
2. Take every directory listed when `first` is set, and otherwise the due ones.
3. Drop each directory whose registry slot is reserved and not done.
4. If none is left, clear `first` and answer `Ok`.
5. Call `scan_jobs::reserve(state, &batch, started, None)`, with `started = Instant::now()`. A refusal answers `Ok` and leaves `first` as it was.
6. Call `start(batch, started)` and clear `first`.

**`spawn`:**
- `tokio::time::interval(every)` with `MissedTickBehavior::Delay`, as `spawn_refresher` does.
- Each tick runs on the blocking pool:
  - It reads `schedule::now` from a guard dropped at once.
  - It calls `tick` with a `start` that calls `scan_jobs::spawn(Arc::clone(&state), roots, started, Reread::Batch)`.
- An interval's first tick completes at once, so the startup batch starts as soon as `serve` spawns it, which is after the bind.
- Failures and panics are printed as "What the plan settles" says.
- A `const` assertion holds `TICK` under `schedule::MIN_INTERVAL_SECS`.

**The README's words**, in place of "(Nothing schedules scans yet; a later change adds that.)":
> The server scans each directory when it starts, unless its schedule is Off, and again on its schedule, which the directory's page sets: Automatic, every 15 minutes; Every, at least 5 minutes; or Off, only when Scan directory or Scan library is pressed.

- [ ] **Step 1: Write the failing tests** in `scheduler/tests.rs`.
  - Fixture: `test_support::state_with_two_scanned_roots()`, with A first by path. Set each directory's interval and `last_scan_at` by SQL.
  - Drop every `state.library.conn()` guard the test holds before each `tick`. The tick takes the same mutex, and so does a job's `working()` where the library is in memory.
  - `NOW` is the literal `2026-11-10 12:00:00`. "Due" means `last_scan_at` an hour before it, with Automatic.
  - `start` records each call's roots and instant. A scheduler past its first tick is built as `Scheduler { first: false }`.
  - `the_first_tick_takes_every_directory_that_is_not_off_due_or_not`:
    - A's `last_scan_at` is `NOW`, so A is not due. B is Off.
    - `Scheduler::new().tick(…)`: `start` is called once, with `[A]`.
  - `a_tick_starts_its_due_directories_as_one_job`:
    - A and B are due. `start` is called exactly once, with `[A, B]`.
    - Each slot is reserved and not done, with `library: None` and the `started` that `start` was handed.
  - `a_tick_passes_over_a_directory_a_press_holds_and_the_press_satisfies_its_timer`:
    - A and B are due. Reserve A as a press would, with `scan_jobs::reserve(&state, &[A], Instant::now(), Some(<A's library>))`.
    - The tick starts `[B]` only.
    - Then end A's press: run `scan::scan_root` on A, which writes A's `last_scan_at` by the store's clock, and `scan_jobs::finish_reservation`.
    - Tick again at `schedule::now(&conn)`. `start` is not called: A is not due, and B's slot is still held by the batch the first tick reserved.
  - `a_change_between_ticks_is_read_at_the_next_tick`:
    - A is due. The tick starts `[A]`. Finish A's reservation without a scan, so A is still due.
    - Set A to Off (`scan::update_root`). The next tick does not call `start`.
  - `a_first_tick_that_cannot_read_the_store_leaves_the_next_tick_the_first` (the plan's own rule):
    - A is not due. `ALTER TABLE library_root RENAME TO library_root_away`, and the tick answers `Err`.
    - Rename it back. The next tick starts `[A]`.
    - If SQLite refuses the rename, drop this case and say so in the report. Do not invent another seam without asking.
  - In `tests.rs`, `serve_scans_each_directory_but_an_off_one_once_it_listens`:
    - Use an `Arc` of `test_support::library()` holding two never-scanned page directories, with B Off.
    - Serve it with `crate::Works::library(&library)`, a live registry.
    - Poll the store until A reads `ok`, failing with a message after a bound of seconds. B is still NULL.
  - Run them. They fail.
- [ ] **Step 2: Implement** `TICK`, `Scheduler`, `tick` and `spawn`, and the line in `serve`. Run `taiyaku-serve`'s nextest to green.
- [ ] **Step 3: Watched mutations.** Restore after each.
  - The first tick using the due rule: the first case fails.
  - The first tick ignoring Off: the first case fails.
  - Each directory reserved and started alone: the one-job case fails on the count.
  - A slot reserved with a library: the one-job case fails.
  - Every due directory handed to `reserve`, so that it refuses: the press case's first tick fails.
  - Due ignoring `last_scan_at`: the press case's second tick fails.
  - The directories read at the first tick kept for later ticks: the change case fails.
  - `first` cleared before the read: the read-failure case fails.
  - The spawn line removed from `serve`: the serve case fails at its bound.
- [ ] **Step 4: The README.** Replace the aside with the words above, and keep the paragraph's other sentences.
- [ ] **Step 5: SQLite 3.40.1.** Tasks 1-4 add SQL, `UPDATE`s among it. Run the library and serve crates in the image, from `$W`, at this task's commit. `target/bookworm/` is gitignored build cache.
  ```sh
  H=$(git -C $W rev-parse --short=8 HEAD); B=$W/target/bookworm
  mkdir -p "$B/src-$H" "$B/target" "$B/cargo-home"
  git -C $W archive HEAD | tar -x -C "$B/src-$H"
  docker run --rm --user "$(id -u):$(id -g)" \
    -v "$B/src-$H:/src" -v "$B/target:/t" -v "$B/cargo-home:/c" \
    -e CARGO_TARGET_DIR=/t -e CARGO_HOME=/c -e HOME=/c -e RUSTUP_TOOLCHAIN=<the image's toolchain> \
    -w /src rust:1-bookworm \
    cargo test --locked --no-fail-fast -p taiyaku-library -p taiyaku-serve -- --test-threads=6 \
    >| $E/T4-bookworm.log 2>&1; echo "exit $?" >| $E/T4-bookworm.exit
  ```
  - Name the toolchain from `rustup toolchain list` inside the image.
  - Two tests fail there by design, because they need `strace`, which the image lacks: `stamp::tests::a_rewrite_syncs_…` and `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool`. Anything else failing is real.
- [ ] **Step 6: Run** clippy and fmt.
- [ ] **Step 7: Commit:** "The server scans each directory on its schedule, every one not Off once at startup, as one job per tick (O51)". Write `$E/T4-report.md`.

---

### Task 5: The facts on the web: scan every, next scan, and the new words (O51, O186)

Spec §4, "The facts and the screen" (the facts and the column) and the last bullet of "O186". Spec §4, Testing: "`WIDEST_ENDS`", and the facts half of "The control".

**Files:**
- `web/src/config.ts`:
  - `Root` (~74-93) gains `next_scan_at`, and `last_scan_status`'s doc names `deriving` and `interrupted`.
  - `scanEvery` and `nextScan` go beside `lastScan` (~422).
  - `regimeFacts` (~454-484) draws "scan every" through `scanEvery` and adds "next scan" after "last scan". Update its doc.
- `web/src/LibraryPage.tsx`: the Scan every cell (~1388-1391) draws `scanEvery(root)`. `WIDEST_ENDS` (~420) and its doc (~412-419) gain the two statuses.
- Every `Root` fixture that `tsc -b` names, under `web/tests` and `web/src`: add `next_scan_at: null` unless the test reads it.
- Tests:
  - `web/tests/config.test.ts`: `regimeFacts` (~81-113), and a `lastScan` case.
  - `web/tests/LibraryPage.test.tsx`: the Directories table case (~1049-1100).
  - `web/tests/RootRow.test.tsx`: the facts' pairs (~210-225).
  - `web/tests/DirPage.test.tsx`: the facts' labels (~175).
  - `web/tests/Libraries.shell.browser.test.tsx`: the held list (~1505-1513).

**Words** (literal; see "What the plan settles"):
- `scanEvery`: `automatic` for null, `off` for 0 or less, and `humanInterval(secs)` otherwise.
- `nextScan`: `off` where `scanEvery` says `off`, `due` where `next_scan_at` is null, and `inUtc(next_scan_at)` otherwise.

- [ ] **Step 1: Write the failing tests.**
  - `config.test.ts`:
    - The full-array case: a bare root (interval null, never scanned, `next_scan_at` null) gives the four facts `watch` `unknown — probed when a directory is added`, `scan every` `automatic`, `last scan` `never`, and `next scan` `due`.
    - Off: interval 0, scanned, `next_scan_at` null, gives `off` and `off`.
    - Interval -1 gives `off` for both.
    - Every 3600 with `next_scan_at` `2026-11-10 13:00:00` gives `1h` and `2026-11-10 13:00:00 UTC`.
    - `lastScan` with `2026-11-10 12:00:00` draws `2026-11-10 12:00:00 UTC\u00a0·\u00a0interrupted` and `2026-11-10 12:00:00 UTC\u00a0·\u00a0deriving`. Check `JOINING_DOT` against the existing failed case.
  - `LibraryPage.test.tsx`:
    - The table case's rows read `automatic` for a null interval, `24h` for 86400, and `off` for a third row at 0.
    - The header stays `['Path', 'Scan every', 'Last scan', 'Overlaps']`.
  - `RootRow.test.tsx` and `DirPage.test.tsx`: the facts' pairs and labels gain "next scan", last.
  - `Libraries.shell.browser.test.tsx`: the held list gains `8888-88-88 88:88:88 UTC\u00a0·\u00a0deriving` and `…\u00a0·\u00a0interrupted`, in `WIDEST_ENDS`' order.
  - Run the node and jsdom projects and the whole browser project. The new cases fail.
- [ ] **Step 2: Implement.** Add `next_scan_at` to the fixtures `tsc -b` names. Run all three projects to green.
- [ ] **Step 3: Watched mutations.** Restore after each.
  - The old `not set` for null: the bare case and the column fail.
  - The `<= 0` arm dropped (0 drawn as `0s`): the Off case and the column fail.
  - `<= 0` made `=== 0`: the -1 case fails.
  - `nextScan`'s Off arm dropped (Off drawn `due`): the Off case fails.
  - "next scan" placed before "last scan": the pairs case fails.
  - `deriving` left out of `WIDEST_ENDS`: the browser case fails. `interrupted` left out: the browser case fails.
  - `lastScan` drawing a status it does not know as the time alone: the `lastScan` case fails.
- [ ] **Step 4: Run** `tsc -b` and biome.
- [ ] **Step 5: Commit:** "The directory's facts and the Scan every column read automatic, off or the interval, with the next scan, and a scanning row holds the deriving and interrupted words (O51, O186)". Write `$E/T5-report.md`.

---

### Task 6: The Scan schedule control on the directory page (O51)

Spec §4, "The facts and the screen": the control bullet. Part C §4.3. Spec §4, Testing: "The control".

**Files:**
- `web/src/RootRow.tsx`:
  - `BAD_INTERVAL` (~320), `intervalPatch` (~342) and `leftPatch` (~359).
  - The "How it is scanned" section's Scan interval row (~1040-1068).
  - The `edited` map and the left-on-unmount effect (~640-660).
- The model for a radio row is `AccountPage.tsx`'s `Row` with `group="radiogroup"` and `.settings-choices` (~874-915).
- CSS: only if the number and its unit need it, in the stylesheet that holds `.settings-choices`.
- Tests:
  - `web/tests/RootRow.test.tsx`:
    - ~989, ~1004, ~1428 and ~1456. "Refuses to store an interval of zero or less" flips for 0.
    - The field lists (~1592, ~1761, ~1849-1858).
    - The left-on-unmount cases (~2683-2771), and the late refusal's label (~2930).
  - `web/tests/DirPage.test.tsx` (~240, ~1570-1579).
  - `web/tests/Libraries.test.tsx` (~1320-1334).
  - `web/tests/heldFocus.browser.test.tsx` (~1543): Tab now lands on the checked radio.
  - `web/tests/fieldNames.browser.test.tsx` (~471-481).

**The control** (literal; first written here):
- **The row.** The section's first row has the label "Scan schedule" and `group="radiogroup"`.
  - Its help: "Every is at least 5 minutes. Off scans the directory only when Scan directory or Scan library is pressed."
  - It has three radios, `name="scan_schedule"`, labelled "Automatic", "Every" and "Off".
- **The box and its unit.** While Every is chosen, two controls follow Every's label. They are not inside it, since a control inside a label is that label's too.
  - A number box: `name="scan_every"`, `aria-label="Scan every"`, `type="number"`, `min="1"`, `step="1"`.
  - A select: `name="scan_every_unit"`, `aria-label="Scan every unit"`, with the options "minutes" and "hours".
- **What the row draws.** The stored value picks the radio:
  - null is Automatic, and 0 or less is Off.
  - Any other value is Every. The box holds it in hours where it is a whole number of hours, and otherwise in minutes, rounded down.
  - Drawing sends nothing.
- **What a choice sends.**
  - Choosing Automatic sends `{scan_interval_secs: null}`, and choosing Off sends `{scan_interval_secs: 0}`, at once, through `savePatch` with the label "Scan schedule".
  - Choosing Every sends nothing.
- **What the box sends.** Leaving the box, or changing its unit, sends `{scan_interval_secs: count × 60 or 3600}`:
  - when the box holds a whole number and that number of seconds is at least 300;
  - and not when it equals the stored value.
  - An empty box sends nothing and says nothing.
  - An entry that is not a whole number, one the box cannot read (`validity.badInput`), and one under 5 minutes send nothing, and the row's alert says: "Every has to be a whole number of minutes or hours, at least 5 minutes."
  - The web's minimum is one constant, with a doc that points at the server's `schedule::MIN_INTERVAL_SECS`.
- **A refusal from the server** (400) is drawn in the row's alert in the server's words, as every save's refusal is.
- **A box typed in and not left when the row goes** is sent as leaving it would send it, against the row as last drawn, with the unit as drawn. A refusal then sends nothing. A late refusal reads "Scan schedule for <path> was not saved (<sentence>)."

- [ ] **Step 1: Write the failing jsdom tests** in `RootRow.test.tsx`. Rework the old interval cases into these rather than keeping both.
  - **The stored value picks the radio.** null gives Automatic and no box. 0 gives Off. 86400 gives Every, `24` and `hours`. 900 gives Every, `15` and `minutes`.
  - **Automatic and Off save at once.** From a row at 86400, Off sends `{scan_interval_secs: 0}`, then Automatic sends `{scan_interval_secs: null}`.
  - **Every in minutes and in hours.** From Automatic, choose Every, type `15` and leave the box: it sends 900. With the box at `2`, choosing hours sends 7200.
  - **Under the minimum is refused.**
    - `4` minutes, `0`, `1.5` and a `badInput` box send nothing, and the alert holds the refusal sentence written out.
    - `5` minutes sends 300.
  - **Nothing is sent** for an empty box left, or for a box left at the stored value: a row at 900 with `15` minutes.
  - **The server's words.** `patchRoot` rejects with a 400 whose body is `{"error": "<the server's sentence>"}`, and the alert holds that sentence.
  - **Left when the row goes.** `10` minutes typed and not left is sent as 600 on unmount, and `4` is not sent. A late refusal reads "Scan schedule for /srv/manga was not saved (…)."
  - Update the field lists, DirPage's and Libraries' 600 cases (Every, `10` minutes), and the two browser files named under Files.
  - Run the jsdom project. The new cases fail.
- [ ] **Step 2: Implement** the control. Run the node and jsdom projects, then the whole browser project, to green.
- [ ] **Step 3: The layout at 390 px.**
  - Find the browser test file that already lays out the directory page at 390 (`rg -n '390' $W/web/tests/*.browser.test.tsx`).
  - Add a case: with Every chosen, the radios, the box and its unit stay inside the page, with no horizontal overflow. Follow the file's existing overflow checks.
  - If a CSS rule makes it fit, removing the rule by hand must fail the case. If none is needed, say so in the report.
  - Run the whole browser project.
- [ ] **Step 4: Watched mutations.** Restore after each.
  - Off saved as null (the spec's named defect): the Automatic-and-Off case fails.
  - 0 drawn as Automatic: the drawing case fails.
  - The unit ignored: the hours case fails.
  - The minimum put back to one second: the refusal case fails, since 4 minutes sends 240.
  - The client's sentence drawn in place of the server's: the server case fails.
  - The box dropped from the left-on-unmount map: the unmount case fails.
- [ ] **Step 5: Run** `tsc -b` and biome.
- [ ] **Step 6: Commit:** "A directory's page sets its scan schedule: Automatic, Every at least 5 minutes, or Off (O51)". Write `$E/T6-report.md`.

---

### Task 7: The closes in `docs/status.md`

Read `docs/status.md`: O186's row (~1795), O51's row (~1659), the Open items head (~1447), the closed list and "leaving" sentence (~1482), "What remains" (~4713), and the notes paragraphs after the table, whose last ones introduce a PR and its bullets (~4080-4140).

- [ ] **Step 1: Close O186 with the five-part edit.**
  1. **The row's checkbox.**
  2. **The row's text.** It opens with "**Closed <date>: …**", then says how, naming the tests, then "*As filed:*" with the original text, as O245's row does.
     - It says that the CLI's `library scan` writes no `deriving` (spec Decision 10). A CLI scan cut during its derive still reads `ok`, and the scheduler's next scan of the directory heals the derive.
     - It says that an Off directory keeps `interrupted` until Scan is pressed.
     - Sites: `scan::mark_deriving`, `scan::mark_derived`, `scan::interrupt_derives`, `scan::mark_derive_failed`, `scan_jobs::run_with` and `scheduler::interrupt_cut_derives`.
  3. **The notes.** After the last notes paragraph, add one that introduces this PR: it closed O186 and narrowed O51. O186's bullet ("… Closed.") and O51's bullet ("… Narrowed.") follow it, shaped like the O252-O256 bullets.
  4. **"N are now closed — leaving M".** Generate the closed list from the table, never by hand:
     ```sh
     grep -a -c '^| O[0-9]' $W/docs/status.md
     grep -a -o '^| O[0-9]* | \[x\]' $W/docs/status.md | grep -a -o 'O[0-9]*' | paste -sd, - | sed 's/,/, /g'
     grep -a -c '^| O[0-9]* | \[x\]' $W/docs/status.md
     grep -a -n -A1 '^What remains:' $W/docs/status.md
     ```
  5. **"What remains … M open items".** Its count word wraps across a line break, so read the line after the opener. The sentence after it, on how many came from a review, moves too if O186's origin counts there.
- [ ] **Step 2: Narrow O51.**
  - The row stays open, and its prose stays as written.
  - Append "*Narrowed on <date>: …*". Say what is built:
    - the per-directory schedule, Automatic (NULL, the unwatched interval), Every (at least 300 s, enforced by the server) and Off (0);
    - the startup scan;
    - one job per tick;
    - and the add no longer writes the probe's interval.
  - Then what remains: the watcher, and the probe kept at startup, after the NAS measurement (spec §4, Decision 5). Name `scheduler` and `schedule` as sites.
- [ ] **Step 3: Counts.**
  - Run the gate now (Global Constraints) into `$E/gate-1.log`.
  - Set "Six crates, N tests" (~179) and "of the N are standing checks" (~256) from the JUnit total that `status-test-count` reads, and run the gate again until it passes.
  - Measure the web counts: run the node, jsdom and browser projects, and set the "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one" figures and the browser project's count word from those runs' summary lines. Never add to the old figures.
- [ ] **Step 4: Sweep.**
  - Grep each bare O-number (`O186`, `O51`) across `docs/`, `web/src/` and `crates/`, and read every hit. Prose that speaks of O186 as open in the present tense is marked `*O186 has since closed — see its entry above.*`.
  - `rg -n -i 'nothing schedules|no schedul' $W/README.md $W/docs/status.md`: read every hit.
- [ ] **Step 5: Commit:** "O186 closes and O51 narrows in docs/status.md". Write `$E/T7-report.md`, quoting the gate's `all green` line and its `.exit`.

---

### Finish (the controller)

1. **Whole-branch review** of `main..HEAD`, in two lanes. Give the reviewers this plan, spec §1 and §4, and the rulings.
   - **(a) Rust, Tasks 1-4.** Ask in particular:
     - whether anything outside `scan.rs` writes `last_scan_status`;
     - whether every way out of `run_with` (a changed directory, a failed walk, a failed or panicked derive) leaves no `deriving` that a successful job should have cleared;
     - whether a tick can reserve a slot and never start it, or hold the connection's guard across `start`;
     - whether `first` clears only where this plan says;
     - whether the minimum has one source on the server;
     - whether `next_scan_at`'s arithmetic agrees with SQLite's `datetime()` across a month, a year, and a leap day;
     - whether any new test passes against its defect.
   - **(b) Web and docs, Tasks 5-7.** The control's states and its unmount path, the facts' words, `WIDEST_ENDS`, and the closes against the code.
   - After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **One fix wave.** Findings go back to the implementer of the task by SendMessage. Late code fixes cost a gate again.
3. **The final gate** on the final commit, into `$E/gate-final.log`, at load < 24. Read the `.exit` file. Re-read the Rust and web counts in `docs/status.md` against that run.
4. **The drive**, on a live server, by a subagent that reports back. Start the server from the controller, or tell the agent never to restart it.
   - **Setup.**
     - Record `sha256sum /home/davey/temp/manga-test/library.db`, and `touch` a marker in the scratch dir.
     - Copy `library.db` into the session scratchpad. Copy only; never open the original.
     - Build a release `taiyaku` from `$W`, and copy it into the scratch dir. Build `web/dist` into the scratch dir.
     - On the copy, with `sqlite3`, read `SELECT id, path, keeps_documents, scan_interval_secs FROM root`. Set `keeps_documents = 0` where it is not, so that no derive writes a document under the real tree. A server scan never probes, but an Add directory does, so add no directory inside the test library.
     - On the copy, set one directory `last_scan_status = 'deriving'` and `scan_interval_secs = 0`. If there is a second directory, set it `deriving` and leave it Automatic.
     - The copy's one account is `local`. Set its password with `printf 'pw\npw\n' | <binary> library user password local --db <scratch>/library.db`.
     - Serve with `<binary> serve --db <scratch>/library.db --cache <scratch>/cache --web <scratch>/dist --addr 127.0.0.1:<port> --insecure-cookies`, in the background. Pick a port that `ss -lptn` shows free, since other lanes may be driving at the same time.
     - A headless login is `POST /api/session` with `{name, password}` and an `origin: http://127.0.0.1:<port>` header. Without that header the answer is 403.
   - **Checks**, with a real mouse, at 1280 and 390 px:
     1. **O186.** Stderr has the rewrite line. The Off directory reads "… · interrupted". A directory that is not Off reads ok once the startup scan ends.
     2. **The control.** The Off directory's page shows Off. Choose Automatic: "next scan" reads `due` or a time 15 minutes after the last scan, and within a tick of it falling due, Last scan moves.
     3. **Every.** 4 minutes is refused in the literal sentence. 5 minutes saves, and "next scan" is the last scan plus 5 minutes. Once that passes, Last scan moves within a tick.
     4. **The server's minimum.** A `PATCH` of 299 by `curl`, with a session, answers 400 in the literal sentence.
     5. **The column.** The library page's Scan every column reads `automatic`, `off` and `5m`.
     6. **A restart.** Stop the server by PID and start it again: every directory not Off is scanned, and the Off one is not.
     7. **Layout.** At 390 the control fits, with no horizontal scroll.
   - **Teardown.** Stop by PID and check the port again. `find /home/davey/temp/manga-test -newer <marker>` finds nothing, and the sha256 is unchanged. Then delete the scratch copy.
5. **Claim check** of the closes, the plan's settled calls as written into code comments, and the PR body.
   - Put the PR body in pass 1.
   - Run a pass after each correction, scoped to the correction commit plus the PR body.
   - Grep a corrected sentence's shape everywhere.
6. **The PR body** at `$E/pr-body.md`, for the gitea MCP, never hard-wrapped.
   - Its first line is the title, under 255 characters: "Scheduled scans: each directory on its own schedule, every one not Off once at startup, and a scan cut during its derive reads interrupted (O186, narrows O51)".
   - It names O186 closed and O51 narrowed.
   - It gives the owner's rulings of 2026-11-10, the plan's settled calls, and the spec gap above.
   - It carries no attribution.
   - Ask the owner before pushing.

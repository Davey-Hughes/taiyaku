# Pre-deploy lane 5: cache cleanup (O11) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound the server's cover and page caches before the first deploy, and narrow O11 to its Stump half.
- An orphan sweep at startup and every 24 h deletes the renderings no `member.sha256` holds, and the staging files a crashed write left.
- A cap, 10 GiB by default and set by `--cache-max` or `TAIYAKU_CACHE_MAX`, evicts the least recently used renderings at each sweep.
- Use is a file's mtime, refreshed on a server-side hit at most once a day.

**Architecture:**
- Task 1: a new module `cache_sweep` in `taiyaku-serve`: the key parser, the listing, the digest read, and the deletions by name (orphans, leftovers). This plan rides in the same commit.
- Task 2: the cap's eviction in `cache_sweep`, and the touch on a hit in `cover::cached`.
- Task 3: the task `serve` spawns (start, then every 24 h, a live registry only), its line on stderr, and `ServeOptions::cache_max`.
- Task 4: the CLI's `--cache-max` and `TAIYAKU_CACHE_MAX`, the usage line, and the README.
- Task 5: O11 narrowed in `docs/status.md`.
- Then the Finish.

No migration, no web change, and no `ServeState` field.

**Tech Stack:** Rust (`taiyaku-serve`, `taiyaku-cli`). The one SQL statement is a plain `SELECT` of an existing column.

**Spec:** `docs/superpowers/specs/2026-11-10-pre-deploy-design.md` §1 (lanes and coordination) and §6 (this lane), cited as "spec §n". Pre-flight: `.superpowers/sdd/pre-deploy-evidence/daily-preflight.md` item 3 (local, not committed). The code was read at main 10ae1e74. Locations are by symbol; line numbers are hints taken there.

**The owner's rulings of 2026-11-10** (binding; do not re-open):
- An orphan sweep at startup and every 24 h.
- A default 10 GiB cap, least recently used first, set by `--cache-max` or `TAIYAKU_CACHE_MAX`.

The spec's own calls (spec §6 decisions 3 to 6) are settled too, and are not re-opened here: use is mtime refreshed at most once a day on a hit, not atime (spec §6 decision 3); a cap of 0 is no cap, and a size is a byte count with an optional binary unit (4); a staging leftover is an orphan (5); O11 is narrowed to its Stump half (6).

**What the plan settles** (the controller's calls; they are told to the owner and are not rulings):
- **The live-registry check lives in `cache_sweep::spawn`**, which answers `None` for a fixed registry. A test holds it without binding a port, and `serve`'s change stays one call.
- **`started` is taken inside `spawn`**, after the bind and before `axum::serve` accepts. Every staging file this process writes follows a request served after that and a decode, so a filesystem clock that lags by a tick cannot date it before `started`. Taking it at the top of `serve` would be a second edit in the function lane 3 also edits.
- **A digest is exactly 64 lowercase hex characters**, what `taiyaku_read::member_digest` mints. A name with any other digest is unrecognised and left. So a `--cache` root set by mistake over someone else's `covers` or `thumbs` can never cost an unrelated file.
- **The size grammar:** one or more ASCII digits, then optionally one of `K`, `M`, `G`, `T`, in either case, as powers of 1024. Nothing else: no space, no fraction, no sign, no `B` or `iB`. A value whose byte count overflows `u64` is refused. The variable is not read when the flag is given, the precedence `--stump-url` holds over `TAIYAKU_STUMP_URL`. The 10 GiB default is the CLI's (`DEFAULT_CACHE_MAX`, beside `--addr`'s default); `ServeOptions::default()` is no cap, which every test's options use.
- **What the sweep does not count:** an entry that is not a regular file (a directory, a symlink) is left alone, and counted neither in the total nor as unrecognised. A cache directory that does not exist yet (`thumbs` before the first tile) is an empty one, not a failure.
- **A failure is never destructive.** A listing that fails for any reason but `NotFound`, or a digest read that fails, ends the sweep before it deletes anything, and the line says why. A delete that fails leaves the file in the total, the eviction moves on to the next candidate, and the line counts the files left undeleted and names the first error. A delete answered `NotFound` counts as done: the file is gone.
- **Ties in eviction are broken by file name**, so the order is total and a test's order is fixed.
- **The sweep takes no job lock**, so it never refuses or delays a rule save, a stamp, a move or a scan. A derive is atomic per work, not across the store (`derive::derive_with`'s doc), so a member that moves from one work to another can be held by neither for a moment (inferred from that doc, not run). A sweep whose digest read lands then deletes that member's renderings, and the next request regenerates them. Lane 3's startup scan and this startup sweep both start after the bind, so they overlap on every start, but only a member that moves between works is exposed. Taking the lock would also add a sixth taker to `LibraryJob`'s doc, which lane 4 edits.
- **The races with a request, and why each is safe:**
  - A read racing a delete: `cached` reads through one `fs::read`. A file unlinked after it was opened stays readable, and one gone before is a miss (`cached` answers `None`), which the caller regenerates. Never an error.
  - The touch runs after the hit is decided and its result is discarded, so it cannot turn a hit into a miss or an error. A touch on a file deleted in between fails quietly.
  - A write racing a delete: the sweep deletes only names it listed. A staging file of this process is younger than `started`, so it is never a leftover. A file renamed into place after the listing is unlisted and stays. One renamed over a listed name just before that name is deleted is lost, and regenerated at the next request.
  - A touch racing an eviction: eviction uses the listing's sizes and mtimes, so a file touched after the listing can still be evicted. It is regenerated.
  - Two sweeps never overlap: the loop awaits each sweep before its next tick.
- **The stderr lines** (new here, so written out):
  - Each sweep: `cache sweep: deleted {o} orphan(s) ({ob} bytes), {l} leftover(s) ({lb} bytes) and {e} eviction(s) ({eb} bytes); {n} file(s) ({b} bytes) left, {u} of them not recognised`. When a delete failed, followed by `; {f} could not be deleted, the first: {error}`. Bytes are written as plain integers.
  - A sweep that could not run: `could not sweep the cache: {why}`.
  - A sweep that panicked: `taiyaku: the cache sweep panicked: {panicked}`.
  The first two follow `serve`'s session-sweep lines; the third follows the refresher's.
- **The schedule test needs a paused clock.** `taiyaku-serve`'s `[dev-dependencies]` gain `tokio = { version = "1", features = ["test-util"] }`, for `#[tokio::test(start_paused = true)]`. Tokio stops a paused clock from advancing while a `spawn_blocking` task runs, so a sweep on the blocking pool is waited for.
- **No row is opened for the NAS measurements** (spec §6 "Measured on the NAS later"). The spec holds them.

**Spec gaps:** none found against the code at 10ae1e74. The race between the sweep and a derive in progress is not named by the spec; the plan's call is above.

**New interfaces this plan names** (the plan is where they are first written):
- `crates/taiyaku-serve/src/cache_sweep.rs`, with its tests in `cache_sweep/tests.rs` (the crate's `#[cfg(test)] mod tests;` pattern), and `mod cache_sweep;` in `lib.rs` after `mod cache_root;`. All items `pub(crate)` or private:
  - `const EVERY: Duration`, 24 hours.
  - `enum Parsed<'a> { Key { digest: &'a str }, Staging }` (`Debug`, `PartialEq`, `Eq`) and `fn parse(name: &str) -> Option<Parsed<'_>>`. A staging file carries no digest: a leftover goes by its age whatever its key, so nothing would read one. The shapes, with any long edge, offset and version as one or more ASCII digits:
    - a cover `{digest}-{edge}v{n}.jpg`;
    - a tile `{digest}-p{offset}-{edge}v{n}.jpg`;
    - a reading page `{digest}-p{offset}-r{edge}v{n}.jpg`;
    - a staging file: one of those names followed by `.tmp.{pid}.{n}`, as `taiyaku_read::atomic::temp_path` mints it.
  - `struct Tally { files: u64, bytes: u64 }` (`Default`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Debug`).
  - `struct Swept { orphans: Tally, leftovers: Tally, evicted: Tally, left: Tally, unrecognised: u64, undeleted: u64, first_failure: Option<String> }`. `evicted` arrives in Task 2.
  - `fn sweep_with(dirs: &[PathBuf], library: &taiyaku_library::Library, cap: u64, started: SystemTime, after_listing: impl FnOnce()) -> Result<Swept, String>`: the whole sweep. `after_listing` runs between the listing and the digest read; production passes a no-op.
  - `fn sweep(state: &ServeState, cap: u64, started: SystemTime) -> Result<Swept, String>`: `sweep_with` over `[state.cache_dir, state.thumb_cache_dir()]` and `state.library` (Task 3).
  - `fn report(swept: &Swept) -> String`: the line above (Task 3).
  - `fn every(period: Duration, tick: impl Fn() + Send + Sync + 'static) -> tokio::task::JoinHandle<()>`: the schedule. It ticks at once and then every `period`, runs each `tick` on the blocking pool and awaits it (Task 3).
  - `fn spawn(state: &Arc<ServeState>, cap: u64) -> Option<tokio::task::JoinHandle<()>>`: `None` for a fixed registry; otherwise takes `started`, and runs `every(EVERY, …)` whose tick sweeps and writes its line (Task 3).
- `ServeOptions` gains `pub cache_max: u64`, in bytes, 0 for no cap (Task 3).
- `cover.rs`: a constant for "a day" beside `cached`, which the touch reads (Task 2).
- `taiyaku-cli` `main.rs`: `const DEFAULT_CACHE_MAX: u64` (10 GiB), and `fn cache_size(text: &str) -> Option<u64>` (Task 4). The refusals, literal:
  - ``--cache-max `{value}` is not a size: give a byte count, optionally followed by K, M, G or T, or 0 for no cap``
  - ``TAIYAKU_CACHE_MAX `{value}` is not a size: give a byte count, optionally followed by K, M, G or T, or 0 for no cap``

## Global Constraints

- **Five lanes in parallel.** This lane runs in its own git worktree `W` (an absolute path the controller gives each dispatch), on branch `pre-deploy-cache` cut from main 10ae1e74. Every command runs against `$W` by absolute path. Never `cd` (it is zoxide here); where a command must start inside `$W`, write `builtin cd $W && …` as the only `cd`.
- **Evidence.** `E=/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/lane-5-cache`, absolute, in the main checkout, so reports outlive the worktree. Logs and each task's report (`$E/T<n>-report.md`) go there, written by Bash (`cat >| … <<'EOF'`), never under `target/`. The Write tool is refused under `.superpowers/`.
- **Build directories.** Never set `CARGO_TARGET_DIR`: the worktree builds into its own `$W/target`. Never share one between worktrees.
- **Shared files** (spec §1). Lane 3 adds startup work to `taiyaku_serve::serve` too. This lane's changes to `crates/taiyaku-serve/src/lib.rs` are exactly four: `mod cache_sweep;`, the `cache_max` field on `ServeOptions`, that field in `serve`'s `let ServeOptions { … } = options;`, and one call to `cache_sweep::spawn` beside `registry::spawn_refresher`, after the bind. Nothing else in `serve` moves. `README.md` and `docs/status.md` are shared with other lanes; `docs/status.md` is edited only in Task 5.
- **No web change.** `web/` is not touched. If a task finds it needs a web change, it stops and reports. The gate's web job still runs: it needs `$W/web/node_modules` as a real copy (never a symlink) and the wasm package the controller put in the worktree. Never `pnpm exec`; call `./node_modules/.bin/<tool>` directly if a web tool is ever needed.
- **Rust, per task.** Run with `--manifest-path $W/Cargo.toml`:
  - `cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/T<n>-<crate>.log 2>&1; echo "exit $?" >| $E/T<n>-<crate>.exit`. Read the summary lines from the log and the status from the `.exit` file.
  - `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`, the same way.
  - Run whole crates, never a named test.
  - rustdoc runs in the gate with `-D rustdoc::broken_intra_doc_links`: a ``[`name`]`` link in a doc comment must resolve.
- **Gate.** The repo's own `scripts/check-all.sh`, never a copy. It starts with `cd "$(git rev-parse --show-toplevel)"`, so it must be started inside `$W`:
  - `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`.
  - Wait for load < 24 (`cat /proc/loadavg`) first, and record the load.
  - Quote its `all green` line and the `.exit` file. Never trust the harness's exit code, and never pipe the gate through `tail`.
  - Its `status-test-count` step only agrees after Task 5, so the gate runs in the Finish, not per task.
- **Pre-existing failures.** A failure in a file the task did not touch is pre-existing only if the same command fails at main 10ae1e74, not at another commit on this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one:
  - Put the named defect back by hand and watch the test fail.
  - Restore, and check with `git diff` that the restore is complete.
  - Record each in the task report: the defect, the failing test, and the assertion's message.
  - A test that names a guard must reach it, and its fixture must tell right from wrong: a cap test whose files all fit under the cap proves nothing.
  - A mutation that can loop runs its test on a thread with a bounded wait, as the CLI tests' `serve_refusal` does, so it fails in seconds rather than at nextest's `terminate-after`.
- **Sentences are literals.** A test asserts a line or refusal written out in the test, never one computed through the function under test.
- **Comments say why.** The reason, the invariant, what breaks if it changes; never history or measurements; a comment may point at an O-number. `scripts/check-comments.mjs` runs in the gate:
  - no comment block over 8 lines of text;
  - no dates, commit hashes or PR numbers in an added comment;
  - each area's comment share against `scripts/comment-budget.json`. The new module and its tests count in `crates/taiyaku-serve`'s share, so say each why once, on the module and on each decision, not per line.

  If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **stderr from a background task** is written with `let _ = writeln!(std::io::stderr(), …)`, as `registry::spawn_refresher` does. `eprintln!` panics when nothing reads stderr, and a panic would end the task.
- **No AI attribution anywhere**: code, comments, docs, commits, PR body.
- **Commits.** One commit per task, its subject naming O11. Never `git commit --amend`. Check each new hash is an ancestor of `HEAD`.
- **No migration, and no SQL beyond the one plain `SELECT DISTINCT sha256 FROM member`.** No `rust:1-bookworm` run is needed.

---

### Task 1: The sweep by name: orphans, leftovers and what it leaves (O11)

Spec §6 "The sweep", steps 1 to 5. Pre-flight §3.1, §3.2.

**Files:**
- Create `crates/taiyaku-serve/src/cache_sweep.rs` and `crates/taiyaku-serve/src/cache_sweep/tests.rs`.
- `crates/taiyaku-serve/src/lib.rs`: `mod cache_sweep;` after `mod cache_root;` (~11), and nothing else in this task.
- **Dead code until Task 3.** Nothing outside the tests calls the module before Task 3 wires it into `serve`, and clippy runs `--all-targets -D warnings`. So `cache_sweep.rs` opens with `#![allow(dead_code, reason = "…")]`, whose reason says Task 3's call is what uses it. An inner attribute, so `lib.rs` keeps its one line. `allow`, not `expect`: the test build reads most of the module and the library build reads none of it, so one `expect` could not be fulfilled in both. Task 3 removes it.
- Read, do not change: `cover::cache_path`, `cover::thumb_cache_path`, `cover::thumb_keyed_path`, `cover::keyed_path`, `cover::page_cache_path`, `cover::page_key_at` (`cover.rs` ~101-177), `taiyaku_read::atomic::temp_path` (`crates/taiyaku-read/src/atomic.rs` ~21-37).
- Fixtures to reuse: `test_support::scanned_file_backed_library` (one root at `{fixture}/tree`, one volume at `tree/Scanned/Scanned - v01`, scanned and derived), `test_support::Fixture`, `taiyaku_library::scan::roots` and `scan_root`.
- Copy this plan to `$W/docs/superpowers/plans/2026-11-10-pre-deploy-cache-cleanup.md`. It rides in this task's commit.

**Behaviour** (spec §6, in this order, inside `sweep_with`):
1. List every entry of each directory in `dirs`: name, size and mtime of each regular file. `NotFound` for a directory is an empty one; any other listing error ends the sweep with `Err`.
2. Call `after_listing()`.
3. Read the held digests with one `SELECT DISTINCT sha256 FROM member` on `library.conn()`, held only for that statement. An error ends the sweep with `Err`, before anything is deleted.
4. Delete each listed keyed file whose digest is not held (`orphans`).
5. Delete each listed staging file last modified before `started` (`leftovers`), whatever its digest.
6. Count each listed file that does not parse (`unrecognised`), and total what is left (`left`).

`parse` takes the name as a whole: a name that only starts like a key, ends in something else, or has a digest that is not 64 lowercase hex characters, is `None`.

- [ ] **Step 1: Write the tests first** in `cache_sweep/tests.rs`. Each plants files in two scratch directories named `covers` and `thumbs`, beside the scanned tree and never inside it, and reads the held digest from the store's `member` row. An unheld digest is a 64-character hex string no member holds.
  - **Every name the cache mints parses, with its digest.** `cover::cache_path`, `cover::thumb_cache_path` at each of `cover::TILE_LONG_EDGES`, and `cover::page_cache_path` at a reading edge each give `Parsed::Key` with the digest they were given. `taiyaku_read::atomic::temp_path` of each, with extension `jpg`, gives `Parsed::Staging`. *Fails on:* the tile shape dropped from the parser; the reading page's `r` not accepted; the staging suffix not recognised.
  - **Orphans.** A cover in `covers`, and a tile and a reading page in `thumbs`, for the held digest and for an unheld one: the unheld three go and the held three stay, and `orphans` counts three files and their bytes. *Fails on:* the digest parsed from the wrong part of the name (up to the last `-`, say).
  - **A member whose file has gone keeps its renderings.** Remove the volume's directory under the tree and `scan_root` again, with no derive: its `scan_entry` row goes and its `member` row stands. Its three renderings stay. *Fails on:* digests read from `scan_entry.sha256` instead of `member.sha256`.
  - **Leftovers.** With `started` some minutes before now:
    - a staging file of a held key, named with this process's own pid (`std::process::id()`) and last modified an hour before `started`, goes;
    - one modified after `started` stays.

    *Fails on:* the age check dropped; a leftover told by its pid rather than its time; a leftover deleted only when its digest is unheld.
  - **Unrecognised files stay.** `notes.txt`; `cover-512v1.jpg`, whose would-be digest is not hex; a key whose digest is 63 characters; one in upper-case hex; `{held}-512.jpg` with no version; `{held}-512v1.png`; and an old staging name whose front is not a key. Every one stays, and `unrecognised` counts them. *Fails on:* every unkeyed file deleted; a digest taken as whatever comes before the first `-` (the non-hex `cover` is then an unheld digest, and its file goes).
  - **Listing first.** `after_listing` writes a keyed file with a new digest into `thumbs` and adds that digest to `member` (a new `member` row in the volume's edition, or the existing row's `sha256` changed). The file stays. *Fails on:* the digests read before the listing.
  - **A directory not made yet.** `thumbs` does not exist: the sweep answers `Ok`, and an unheld cover in `covers` still goes. *Fails on:* `NotFound` treated as a failure.
  - **A digest read that fails deletes nothing.** Make the read fail before the sweep (rename or drop the `member` table on the store's connection): it answers `Err`, and an unheld cover stays. *Fails on:* a failed read taken as "nothing held".

  Run `taiyaku-serve`; the new tests fail to compile or fail. Record which.
- [ ] **Step 2: Implement** `parse`, `Tally`, `Swept` (without `evicted`) and `sweep_with`. `parse` is plain string handling over the shapes in "New interfaces"; no new dependency. Its doc says why the digest must be hex: what a mis-set root costs. `sweep_with`'s doc says why the listing comes before the digest read. Run `taiyaku-serve` to green.
- [ ] **Step 3: Watched mutations**, each from the *Fails on* lists above.
- [ ] **Step 4:** Run clippy and fmt. Copy the plan into `docs/superpowers/plans/`. Commit `Sweep the cover and page caches by name: orphans and crash leftovers go, what does not parse stays (O11)`. Write `$E/T1-report.md`.

---

### Task 2: The cap, and use on a hit (O11)

Spec §6 "The sweep" step 6, "Use", "The cap". Pre-flight §3.3 B.

**Files:**
- `crates/taiyaku-serve/src/cache_sweep.rs` and its tests: `Swept` gains `evicted`; `sweep_with` gains step 7 below.
- `crates/taiyaku-serve/src/cover.rs`: `cached` (~353-377) and its doc, and the new "a day" constant beside it.
- `crates/taiyaku-serve/src/cover/tests.rs`: beside `a_cache_hit_reads_back_the_same_bytes_without_regenerating` (~162).

**Behaviour:**
- Step 7 of `sweep_with` (numbered on from Task 1's list), after the deletions by name: when `cap` is not 0 and `left.bytes` is over `cap`, delete keyed files, oldest mtime first and ties by name, until `left.bytes` is at or under `cap` or no keyed file is left. The total counts every regular file left in both directories: keyed, unrecognised and young staging files alike. Only keyed files are evicted. A failed delete moves on to the next candidate.
- `cached`: on a hit (the marker check passed), when the file's mtime is more than a day before now, set it to now (`File::set_modified`). An mtime in the future counts as young. Any failure in the touch is ignored, and the hit is answered whatever the touch did.

- [ ] **Step 1: Write the tests first.** Every keyed file in these fixtures has a held digest, so Task 1's step 4 does not take it first. Sizes and mtimes are set by the test (`File::set_modified`).
  - **The cap.** Keyed files with set sizes and distinct mtimes, over a cap chosen so the eviction stops with the total exactly at the cap. The oldest go, the rest stay, and `evicted` counts them. *Fails on:* eviction newest first; eviction one file past the cap (`>=` for `>`).
  - **Old keys.** The two oldest files are a reading page of an old page version (`v1`) and a tile at a rung no ladder holds now. The cap is chosen so evicting exactly those two brings the total to it. Both go, and a newer keyed file stays. *Fails on:* the parser accepting only the current version or the current edges (the old file is left as unrecognised, and a newer one is evicted in its place).
  - **A cap held up by files it may not evict.** An unrecognised file and a young staging file, which together are over the cap, beside two keyed files small enough that either of the other two plus both keyed files is at or under it. Both keyed files go, the sweep answers, and the other two stay. Run the sweep on a thread with a bounded wait. *Fails on:* eviction looping while the total stays over; unrecognised files left out of the total; staging files left out of the total.
  - **No cap.** The same files over any size, with `cap` 0: nothing is evicted. *Fails on:* 0 read as zero bytes.
  - **Use** (`cover/tests.rs`). A whole JPEG at `cover::cache_path`, read through `cached_or_generated` with a `page` closure that panics, so the read is a hit:
    - one last modified two days ago comes back with its mtime between the instants just before and just after the call;
    - one last modified an hour ago keeps exactly the mtime it had.

    *Fails on:* every hit touching the file; no hit touching it.
- [ ] **Step 2: Implement** step 7 and the touch. `cached`'s doc says why mtime and not atime, and why the touch can never turn a hit into a miss. `sweep_with`'s doc says that only keyed files are evicted while every file counts.
- [ ] **Step 3: Watched mutations**, each from the *Fails on* lists. No test makes `set_modified` fail: CI runs as root, which can touch any file, so such a test would skip itself there. The report says so, and names the lines that discard the touch's result after the hit is decided, for the reviewer to read.
- [ ] **Step 4:** Run `taiyaku-serve`, clippy and fmt. Commit `Evict the least recently used renderings down to the cache cap, and refresh a rendering's use on a hit at most once a day (O11)`. Write `$E/T2-report.md`.

---

### Task 3: The sweep's task in `serve` (O11)

Spec §6 "The sweep" (when it runs, its line, a live registry only), "The cap" (`ServeOptions`), and §1 "Coordination".

**Files:**
- `crates/taiyaku-serve/src/cache_sweep.rs` and its tests: `EVERY`, `every`, `spawn`, `sweep`, `report`; and Task 1's `allow(dead_code)` removed. Clippy must then pass with no `allow` or `expect` for dead code anywhere in the module.
- `crates/taiyaku-serve/src/lib.rs`:
  - `ServeOptions` (~366-386): `cache_max`, with a doc saying 0 is no cap and that the cap holds at each sweep.
  - `serve` (~423): `cache_max` in the destructure (~428-437), and one call, `cache_sweep::spawn(&state, cache_max);`, beside `registry::spawn_refresher` (~622-626), after the bind. Nothing else in `serve`.
- `crates/taiyaku-cli/src/main.rs`: `serve_options`' `ServeOptions` literal (~1358) names every field, so it gains `cache_max: 0` here, and Task 4 replaces the 0 with the parsed cap. Until Task 4 the server sweeps by name and caps nothing, which is a whole state, not a broken one.
- `crates/taiyaku-serve/Cargo.toml`: `[dev-dependencies]` gains `tokio = { version = "1", features = ["test-util"] }`, with a comment saying it is for the schedule test's paused clock. `Cargo.lock` should not change; if it does, the report says why.
- `crates/taiyaku-serve/src/tests.rs`: a new test beside `serve_keeps_a_live_library_current_without_a_request_naming_the_work` (~644).
- Model: `registry::spawn_refresher` (`registry.rs` ~340-370) for the loop, the blocking pool and the panic line.

**Behaviour:**
- `every` ticks at once and then every `period` (`tokio::time::interval`, `MissedTickBehavior::Delay`), runs `tick` on the blocking pool and awaits it before the next tick.
- `spawn` answers `None` when `!state.works.is_live()`. Otherwise it takes `started = SystemTime::now()` and answers `Some(every(EVERY, …))`. Each tick runs `sweep(&state, cap, started)` and writes `report`'s line, or the failure line, to stderr. A panic is reported by the awaiting loop, which goes on.
- `sweep` is `sweep_with` over `state.cache_dir` and `state.thumb_cache_dir()`, with `state.library` and a no-op `after_listing`.

- [ ] **Step 1: Write the tests first.**
  - **The schedule** (`#[tokio::test(start_paused = true)]`). `every(EVERY, tick)` with a `tick` that sends on a channel. One tick arrives at once. None arrives within `EVERY` less a second (`tokio::time::timeout`, which the paused clock runs through without waiting). The next arrives at `EVERY`. *Fails on:* a single sweep (the loop gone); no sweep at start (a sleep before the first tick); a shorter period.
  - **A fixed registry is not swept.** `spawn` over `Arc::new(test_support::state(…))` (a fixed registry) answers `None`. Over `test_support::router_over_a_live_library()`'s state it answers `Some`; abort the handle. *Fails on:* the check dropped; the check inverted.
  - **The line.** `report` over a `Swept` with a different figure in every field gives the sentence in "What the plan settles", written out; and with `undeleted` and `first_failure` set, the same sentence with the failure clause. *Fails on:* two fields swapped (say orphans and leftovers); the failure clause always or never added.
  - **One integration test over a real `ServeState`** (spec §6 Testing, last item). Use `test_support::router_over_two_scanned_volumes_of_decodable_pages()`:
    - request page 0's tile through the fixture's router with `test_support::get` (`/api/works/{work}/{edition}/pages/0/thumb`; the router already authenticates), so the real route writes a held rendering under `state.thumb_cache_dir()`;
    - plant an unheld cover in `state.cache_dir`, an unheld tile and reading page in `thumbs`, and a staging leftover of a held key modified before `started`;
    - run `sweep(&state, 0, started)`. The route's tile stays, the four planted files go, and `orphans` and `leftovers` count them.

    *Fails on:* `sweep` reading only `state.cache_dir` (the `thumbs` orphans stay).
  - **`serve` sweeps a live library once it is listening** (`tests.rs`, shaped like `serve_keeps_a_live_library_current_without_a_request_naming_the_work`). Over `test_support::scanned_file_backed_library` and `Works::library`, with `ServeOptions { addr, cache: Some(<fixture>/cache), ..ServeOptions::default() }`, plant an unheld cover in `<fixture>/cache/covers` before `serve` starts. Poll, with a bound, until it is gone; then abort the server. *Fails on:* the call in `serve` removed.

  Run `taiyaku-serve`; record which tests fail.
- [ ] **Step 2: Implement** `EVERY`, `every`, `spawn`, `sweep`, `report`, `ServeOptions::cache_max`, and the two changes in `serve`. `spawn`'s doc says why a fixed registry is not swept (its renderings have no `member` rows to be held by, spec §6) and why `started` is taken where it is.
- [ ] **Step 3: Watched mutations**, each from the *Fails on* lists.
- [ ] **Step 4:** Run `taiyaku-serve` and `taiyaku-cli` (the CLI builds `ServeOptions`), clippy and fmt. Commit `Sweep the cache once the server is listening and every 24 hours after, for a live library only (O11)`. Write `$E/T3-report.md`.

---

### Task 4: `--cache-max` and `TAIYAKU_CACHE_MAX` (O11)

Spec §6 "The cap". Pre-flight §3.5 (the CLI's flag list, `serve_options`).

**Files:**
- `crates/taiyaku-cli/src/main.rs`:
  - `serve`'s `reject_unknown_flags` list (~1214-1232): `--cache-max` after `--cache`.
  - `serve_options` (~1307-1368): read the cap beside `--cache` (~1350) and set `cache_max` in place of Task 3's 0; `DEFAULT_CACHE_MAX` and `cache_size` beside it. `option_value(args, "--cache-max", "a size")` gives the missing-value refusal.
  - `print_usage`'s `serve` line (~177-182): `[--cache-max SIZE]` after `[--cache DIR]`.
- `crates/taiyaku-cli/src/tests.rs`: `serve_refuses_an_unknown_flag` (~2501); new tests beside `serve_refuses_a_stump_url_with_no_key` (~2512) and `serve_options_carries_the_dirs_from_the_flag_or_the_variable` (~3100).
- `crates/taiyaku-cli/tests/cli.rs`: a usage test shaped like `the_usage_advertises_serve_cache` (~185).
- `README.md`, "Running it on a server": the `/cache` row of the mounts table (~478), and a paragraph after the `mkdir`/`chown` block (~504-507).

- [ ] **Step 1: Write the tests first.**
  - **The size** (`cache_size`): `0` is 0; `512` is 512; `1K` and `1k` are 1024; `5G` is 5,368,709,120. `""`, `G`, `5GB`, `5 G`, `1.5G`, `-1`, `5Q`, and a `T` value whose bytes overflow `u64`, are each `None`. *Fails on:* `K` as 1000; an overflow wrapped or saturated rather than refused.
  - **The option** (`serve_options` with `an_environment`):
    - `--cache-max 5G` gives 5,368,709,120;
    - `TAIYAKU_CACHE_MAX=3M` alone gives 3,145,728;
    - the flag with the variable beside it gives the flag's value;
    - neither gives 10,737,418,240;
    - `--cache-max 0` gives 0;
    - a bad flag value and a bad variable value are refused with the two sentences in "New interfaces", written out.

    *Fails on:* the variable ignored; the variable winning over the flag; a default of 0 or of 10^10.
  - **`serve` refuses to start on a bad size.** `serve(&argv(&["--cache-max", "12Q"]))` answers the flag's sentence, as `serve_refuses_a_stump_url_with_no_key` does. *Fails on:* a bad value falling back to the default.
  - **The alphabet.** `serve_refuses_an_unknown_flag`'s expected string gains `--cache-max` after `--cache`. *Fails on:* the flag left out of the list (the bad-size test above then gets the unknown-flag refusal instead of its own).
  - **The usage.** `the_usage_advertises_serve_cache_max` finds `[--cache-max SIZE]` on `serve`'s usage line. *Fails on:* the usage line unchanged.

  Run `taiyaku-cli`; record which fail.
- [ ] **Step 2: Implement.** The doc on `serve_options`' reading of the cap says why the flag wins and why a bad value refuses rather than falls back.
- [ ] **Step 3: README.**
  - The `/cache` row's "What" becomes: `covers, tiles and reading-size pages, kept to 10 GiB by default`.
  - After the `mkdir`/`chown` block, this paragraph:

    > The server looks after `/cache` itself. Once it is listening, and every 24 hours after that, it deletes what nothing will read again: the renderings of files the library no longer holds, and any half-written file a crash left. Then, while the cache is over its cap, it deletes the renderings it has served least recently. The cap is 10 GiB. `TAIYAKU_CACHE_MAX`, or `--cache-max`, which wins over it, sets another: a byte count, optionally followed by `K`, `M`, `G` or `T`, or `0` for no cap. The cap holds at each sweep, so between two sweeps the cache can grow past it by what a day's reading writes. Each sweep prints one line saying what it deleted and what is left.
- [ ] **Step 4: Watched mutations**, each from the *Fails on* lists. `tests/cli.rs` runs the built binary, so a mutation's run rebuilds `$W/target/debug/taiyaku`; rebuild after the restore.
- [ ] **Step 5:** Run `taiyaku-cli` and `taiyaku-serve`, clippy and fmt. Commit `A cache cap of 10 GiB by default, set by --cache-max or TAIYAKU_CACHE_MAX (O11)`. Write `$E/T4-report.md`.

---

### Task 5: O11 narrowed in `docs/status.md`

Read `docs/status.md`'s O11 row (~1619) and its notes bullet ("**O11 — Two on-disk caches never notice a change on the far side**", ~2385). Rows are long: read them through `cut -c1-2000`. Grep with `grep -a`.

- [ ] **Step 1: Narrow O11** with the five-part edit:
  1. The checkbox stays `[ ]`.
  2. The row text is rewritten in the shape of O189's "**Narrowed … by …: <what remains>.**". What remains is the Stump resolution cache: `taiyaku.stump.json`, written beside a work's sidecar (`stump/cache.rs`), remembers what each member resolved to, misses included, and nothing notices a book added on Stump's side afterwards. `--refresh-stump` is its escape hatch. One sentence says what this branch did for the cover and page caches, by symbol (`cache_sweep`, `cover::cached`, `--cache-max`), with no line numbers. The "Where" column becomes `stump/cache.rs`.
  3. The notes bullet gains a paragraph on what shipped: the sweep and its order, the cap and its default, use by mtime, the races accepted (this plan's "What the plan settles"), and the NAS measurements still owed (spec §6). **Keep the bullet's measured figures**: `web/src/styles/reader.css`'s `--strip-tile` comment points at "the per-edition size O11 records", and stays true only while the bullet records it.
  4. "N are now closed — leaving M" does not move: O11 stays open.
  5. "What remains … M open items" does not move either. Its count word can wrap across a line break, so grep the sentence opener and read the next line.
- [ ] **Step 2: Rust test count.** Run the whole workspace (`cargo nextest run --manifest-path $W/Cargo.toml --workspace --test-threads=8`, into `$E/T5-workspace.log`), which writes `$W/target/nextest/default/junit.xml`. Set "Six crates, N tests" and "of the N are standing checks" from it, then run `node $W/scripts/status-test-count.mjs $W/target/nextest/default/junit.xml $W/docs/status.md` and quote its answer. Measure; do not add. The web counts do not move: no web file changed.
- [ ] **Step 3: Sweep.**
  - Run `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`, and check them against the count sentences.
  - Grep the bare `O11` as a word (`grep -a -rnw O11`) across `docs/`, `web/src/`, `crates/` and `README.md`, and read every hit. Prose that says the cover or page cache is never pruned or evicted, or speaks of O11 as covering it, is corrected or marked. Historical plans and specs under `docs/superpowers/` stay as they are. `ServeState::thumb_cache_dir`'s "(O11)" gives a reason that still holds; read it and leave it unless it says otherwise.
- [ ] **Step 4:** Commit `Narrow O11 to the Stump resolution cache in status.md`. Write `$E/T5-report.md`.

---

### Finish

1. **Whole-branch review** of `main..HEAD`, in two lanes:
   - (a) the sweep, its races and its task: Tasks 1-3. Each race in "What the plan settles" is checked against the code, and the serve change against "Shared files".
   - (b) the option, the docs and the narrow: Tasks 4 and 5, with the README read against the code.

   Each lane reviews against spec §6 and the rulings above. One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **Final gate** on the final commit, at load < 24, as in the Global Constraints. Read the `.exit` file.
3. **The drive.** A subagent drives a release build against a scratch copy of the test library, and never restarts a server it did not start. The controller re-checks the port after every stop.
   - In a scratch directory under the session scratchpad: `sha256sum` and `stat` `/home/davey/temp/manga-test/library.db`, and `touch` a marker. Copy the database only; never open the original.
   - Build `cargo build --release --manifest-path $W/Cargo.toml -p taiyaku-cli` and copy the binary into the scratch directory. Set the copy's `local` password to `pw` with `library user password`.
   - **If the branch holds lane 3's scheduler by then** (rebased onto it), set every directory in the copy to Off (`UPDATE root SET scan_interval_secs = 0`) before the first start: its startup scan would walk the test library, and the test library is never scanned.
   - Before the first start, seed `<scratch>/cache/covers` and `thumbs`:
     - keyed files under one digest the copy's `member` table holds, with set sizes and old mtimes, totalling over a small cap;
     - an orphan of each shape under an unheld digest;
     - a staging leftover modified before the start;
     - an unrecognised file.
   - Start `serve --db <scratch>/library.db --cache <scratch>/cache --cache-max <small> --addr 127.0.0.1:<free port> --insecure-cookies` (check the port with `ss -lptn` first). Read stderr: the sweep's line follows the `listening` line, and its counts match the files. The orphans and the leftover are gone, the unrecognised file stays, the oldest keyed files are gone down to the cap.
   - Log in (`POST /api/session` with an `origin: http://127.0.0.1:<port>` header) and fetch one page's tile twice. Set the tile's mtime two days back, fetch it again, and see the mtime become now. A fetch of a fresh tile leaves its mtime alone.
   - Stop the server by PID. Start it once with `TAIYAKU_CACHE_MAX=12Q` and see the variable's refusal, word for word, and exit status 1.
   - Prove the test library unchanged: `find /home/davey/temp/manga-test -newer <marker>` is empty, and the `library.db` sha256 matches. Then delete the scratch directory.
   - The drive's report goes to `$E/drive.md`.
4. **Claim check** of the narrow, the README, the plan's figures and the PR body. Put the PR body in pass 1, and scope later passes to the correction commit plus the PR body.
5. **The PR body** at `$E/pr-body.md`. Its title is its first line, under 255 characters, measured before `create`, and it names O11 as narrowed. Ask the user before pushing.

# First scan: progress, Cancel, and a scan every page can see — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A library's first scan walks an archive by its central directory alone, shows from every page which step it is in with figures the server counts, and stops at a Cancel that pauses its directories until Scan is pressed.
**Architecture:** `taiyaku-read` lists a `.cbz`'s pages from the central directory, read through one buffer. `taiyaku-library` reports each step's count and reads a cancel flag through a `progress::Watch` the caller hands in, and keeps a per-directory pause (`root.paused_at`) that the schedule leaves out. `taiyaku-serve` holds each job's step, counts, directories and libraries in memory, serves them on one feed and cancels by job id; the web client polls that feed once per tab, and every page draws its scan from it.
**Tech Stack:** Rust (zip 9.1.3, rusqlite on SQLite 3.53 here and 3.40.1 in CI and the image, axum, tokio); TypeScript/React 19 with the vitest projects `node`, `jsdom` and `browser`.
**Spec:** docs/superpowers/specs/2026-11-15-first-scan-progress-and-cancel-design.md

## Global Constraints

- **Where.** Worktree `/home/davey/projects/taiyaku/.claude/worktrees/first-scan` (`W` below), branch `first-scan`. Never `cd` into the main checkout or another worktree; run by absolute path or `env -C`. Line numbers below are hints at 05ddfc2c; locate by symbol.
- **Evidence.** `E=/home/davey/projects/taiyaku/.superpowers/sdd/first-scan-evidence`, set in every Bash call that uses it. `.superpowers/` is gitignored in the main checkout (`.gitignore:185`). Reports and logs go there by Bash (`cat >| $E/T<n>-report.md <<'EOF'`); the Write tool is refused under `.superpowers/`.
- **Comments** say why: the reason, the invariant, what breaks if it changes. No history, no measurements; an O-number pointer is fine. A new block holds at most 8 lines. `node scripts/check-comments.mjs` from `W` checks it.
- **Attribution:** none, anywhere.
- **Rust per task:** `env -C $W cargo test -p <crate> -- --test-threads=8 >| $E/T<n>-<crate>.log 2>&1; echo "exit $?" >| $E/T<n>-<crate>.exit`, then `grep -a 'test result:' $E/T<n>-<crate>.log`. Also `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Never time anything with a debug build.
- **Web per task**, under `env -C $W/web`, never `pnpm exec`:
  - `./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`, summary lines read from the log.
  - `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`.
  - Coverage: `vitest run --project node --project jsdom --coverage --reporter=default`, then `node scripts/coverage-floor.mjs` from `W`. A floor is never lowered.
  - A task that touches CSS or a `*.browser.test.tsx` runs the whole browser project. A new class needs a selector (`classNames.test.ts`).
- **Gate** (Finish, and after any late code fix): `env -C $W -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/gate.log 2>&1; echo "exit $?" >| $E/gate.exit`, at `/proc/loadavg` < 24. Read `gate.exit`; quote the `all green` line. Never the gate through `tail`, never a copy of the script.
- **SQLite 3.40.1.** Task 5 adds a migration, so it and the Finish run `-p taiyaku-library -p taiyaku-serve -p taiyaku-cli` in `rust:1-bookworm` by the recipe in memory `taiyaku-sqlite-340-in-ci` (host toolchain 1.100.0 mounted read-only). Only the named `strace` tests may fail there, for want of `strace`: the memory's three plus Task 2's (a). Any other failure is real.
- **Watched mutations.** Every new or rewritten assertion is watched failing: put the named defect in by hand, see the test fail on its assertion (not a setup throw), restore, check `git diff`. Record defect, test and message in the task report.
- **Pre-existing failures** count only if the same command fails at main 210ec5e3, in a detached scratch worktree under the session scratchpad with `web/node_modules` and `crates/taiyaku-wasm/pkg` copied, never symlinked.
- **The real library is read-only.** `/home/davey/temp/manga-test` and `/mnt/daveynet/nfs/media 1` (an RW NFS mount of the NAS share) are only ever opened read-only, by Task 1's bench and the drive's `cp`. No server, scan, `chmod` or write touches either. Nothing hard-links out of them.
- **Servers.** Never touch 127.0.0.1:4398 or :4399 or anything under another worktree. Kill by PID after reading `/proc/<pid>/exe`; check the port again.
- **Commits:** one per task, never `--amend`, no push. Before committing, check the previous task's hash is an ancestor of `HEAD`.
- **Pinned values:** feed poll 1000 ms (`SCAN_POLL_MS`) while any job runs, 5000 ms (`FEED_IDLE_MS`) while idle, none while `document.visibilityState === 'hidden'`. Button word **Cancel**; line "Paused until Scan is pressed, since <`inUtc`>". Status word `cancelled`. Archive read buffer 64 KiB. Job-lock poll 100 ms.
- **Not in this plan** (spec §4): series as they finish, batched or resumed walks, setup help and picker changes, a time estimate, faster hashing, cancelling a CLI scan, SSE, a confirmation for Cancel.

## Review Focus

1. **A cancel during hashing of a first scan**, after every walk committed: each directory still reads Never, says `cancelled`, is paused; a reader sees "Not read yet", not "nothing here"; Scan resumes with the digests kept. Task 5 (`scanned`), Task 6 (c), Task 11 (f).
2. **Cancel pressed while the job waits for the job lock** behind the startup tick's job: it ends at once, walks nothing, and does not wait for the holder. Task 6 (b).
3. **A directory two libraries share, or one nested in another library's directory:** both libraries' pages show the job and its pause, and a reader granted only the other library sees Scanning. Task 6 (f), Task 7 (b, g), Task 9 (d).
4. **A job too short to fall between two idle polls**, or a tab hidden for a whole job: every page still reads again, through the feed's `ended` marker. Task 7 (e), Task 8 (e).
5. **A tab left open across a cancel made elsewhere:** it shows "Cancelling…", then the paused line, and its button reads Scan library again. Task 9 (c).

---

### Task 1: The walk bench, the "before" figure, and the damaged-entry guard

Spec: §3 Goal 1 (the guard), §8 "Walk timing".

**Files:**
- Create `crates/taiyaku-read/examples/walk.rs`.

**Interfaces:**
- Produces: `cargo run --release -p taiyaku-read --example walk -- walk <dir> [--stride N --offset K]` and `-- guard <dir>`. It uses only `taiyaku_read::{sniff, Format, member_pages, member_page_names, is_page_name}` and `zip`, so the same source times the walk before and after Task 2.

- [ ] **Step 1: Setup.** `cp -a /home/davey/projects/taiyaku/web/node_modules $W/web/` and `cp -a /home/davey/projects/taiyaku/crates/taiyaku-wasm/pkg $W/crates/taiyaku-wasm/` (copies, never links). `mkdir -p $E`; `git -C /home/davey/projects/taiyaku check-ignore -v .superpowers/sdd/first-scan-evidence/x` prints the ignore rule.
- [ ] **Step 2: The bench.** Not derivable from existing code, so the algorithm is pinned here:
  - `walk`: take the sorted top-level subdirectories of `<dir>` whose index `i % N == K` (all of them by default). Time the listing: a recursive `read_dir` with `symlink_metadata` of each entry, symlinks skipped. Then time resolving each regular file as `scan::format_of` and `pages_of` do: `sniff`, and `member_pages` for a `Format::Zip`. Print one line: `files= archives= pages= list_secs= resolve_secs= per_archive_ms mean= p50= p90= max=`.
  - `guard`: for each archive that opens as a zip, and each index `i`, read `by_index_data(i)`. An entry that is a file, has a name, is not under `__MACOSX`, is not a dotfile and passes `is_page_name` is a page. Count the pages for which `by_index(i)` errors (`dropped_by_local_header`) and print each with its archive, name and error. Also compare `member_page_names(path)` as a set with the pages less the dropped (`lists_differ_otherwise`), and count archives that do not open (`unopened`, not part of the guard).
  - Files are opened read-only and nothing is written. Check: run `walk` and `guard` on one series of `/home/davey/temp/manga-test/Volumes`; both print their line.
- [ ] **Step 3: The guard.** `guard /home/davey/temp/manga-test/Volumes` (release) `>| $E/guard.txt`. **If `dropped_by_local_header` or `lists_differ_otherwise` is not 0, stop here and report**: the owner decides before Task 2.
- [ ] **Step 4: The "before" figure, NFS.** `ls '/mnt/daveynet/nfs/media 1/Manga/Volumes' | head` first. Then `walk '/mnt/daveynet/nfs/media 1/Manga/Volumes' --stride 4 --offset 0`, release, `run_in_background`, into `$E/walk-before-nfs.txt`. Head the file with `NFS (nfs4, /mnt/daveynet/nfs/media 1), stride 4 offset 0, <date -Iseconds>, HEAD <sha>`. Wait with Monitor, never a foreground sleep.
- [ ] **Step 5:** fmt and clippy. Commit `Add a walk-only bench for timing how a scan lists archives`. Write `$E/T1-report.md` with both output lines.

---

### Task 2: List a `.cbz`'s pages from its central directory (Goal 1)

Spec: §3 Goal 1, §8 Rust first two lines, Walk timing.

**Files:**
- Modify `crates/taiyaku-read/src/pages.rs`:
  - `page_names_from` (280-298) reads each entry through `by_index_data` (central directory only) and becomes generic over `R: Read + Seek`.
  - `open_archive` (324-335) returns `ZipArchive<BufReader<File>>`, with `const ARCHIVE_BUFFER: usize = 64 * 1024`. zip 9.1.3 parses each central-directory entry with separate `read_exact` calls, so an unbuffered file pays reads per page even with no local header touched.
  - The doc comment says why: one seek per page on a share.
- Modify `crates/taiyaku-read/src/format.rs`: `zip_flavour` (231-261) opens through the same buffer.
- Test `crates/taiyaku-read/src/pages/tests.rs`, beside the order tests (191-298) and the `cbz` helper (168).

**Interfaces:** none new; `member_pages`, `member_page_names`, `member_page`, `read_member` keep their signatures and page order.

- [ ] **Step 1:** Read `$E/T1-report.md`: both guard counts are 0, or stop.
- [ ] **Step 2: Failing tests.**
  - (a) `listing_an_archive_reads_no_more_for_four_hundred_pages_than_for_four`, modelled on `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool` through `test_support::strace` with `trace=read,pread64`. Two cbz with tiny entries `p001.jpg`…, 4 and 400. The child runs `member_pages` on each. The parent counts the `read(`/`pread64(` lines on each file's `-y` path: both nonzero and equal. Record both counts.
  - (b) `a_central_directory_listing_matches_one_read_through_each_local_header`: for every entry list the order tests use (with `__MACOSX`, a dotfile, a directory entry, nested paths, a cp437 name and a cleared UTF-8 flag), `member_page_names` equals an oracle the test computes as today's code does (`by_index`, the same filters, `natural_cmp`).
  - (c) `an_entry_whose_local_header_is_damaged_is_listed_and_fails_when_read` (spec's "my call"). Overwrite one entry's local-header signature. `member_page_names` lists it in its sort position. `member_page` of its index errs naming the path. The other pages read.
  - Run: (a) and (c) fail; (b) passes, and must stay green.
- [ ] **Step 3: Implement**, then run `taiyaku-read`, `taiyaku-library` and `taiyaku-serve` whole.
- [ ] **Step 4: Watched mutations.** `by_index` back in `page_names_from`: (a) and (c) fail. `BufReader` removed: (a) fails. The `is_file` check dropped: (b) fails.
- [ ] **Step 5: The "after" figure, NFS.** Same bench, release, into `$E/walk-after-nfs.txt`, each run headed as in Task 1:
  - `--stride 4 --offset 1`: series this bench has not read;
  - `--stride 4 --offset 0`: the "before" series, possibly in the client's page cache, labelled so.
- [ ] **Step 6:** Task checks (Rust). Commit `List an archive's pages from its central directory, read through one buffer`. Report.

---

### Task 3: Progress and cancel in the walk

Spec: §7 Server, "Job state" and "Cancel checks" (resolve loop); §3 Goal 2 (listing, resolving).

**Files:**
- Create `crates/taiyaku-library/src/progress.rs`; `pub mod progress;` in `lib.rs`.
- Modify `crates/taiyaku-library/src/scan.rs`:
  - `scan_root` (2576-2604) becomes `scan_root_watched`, plus a wrapper that keeps the old signature.
  - `scan_root_inner` (2845-3083): the walk's `measure` call and `measure` itself (~3790) report each file found. The resolve `.map().collect()` (2985-3000) becomes a loop that reports each entry and checks the flag before it, all before `BEGIN`.
- Create a recording watch in `crates/taiyaku-library/src/test_support.rs`. Tests in `scan/tests.rs`.

**Interfaces (Produces):**
- `pub enum Step { Waiting, Listing, Resolving, Hashing, Writing, Finishing }`, with `fn as_str(self) -> &'static str`: `"waiting"`, `"listing"`, `"resolving"`, `"hashing"`, `"writing"`, `"finishing"`.
- `pub trait Watch { fn at(&self, step: Step, done: u64, of: Option<u64>); fn cancelled(&self) -> bool; }`, and `pub struct Unwatched;` (reports nothing, never cancelled).
- `pub enum Outcome<T> { Done(T), Cancelled(Step) }`.
- `scan::scan_root_watched(conn: &Connection, root: &Root, watch: &dyn Watch) -> Result<Outcome<ScanReport>, String>`:
  - Listing: `at(Listing, files found, None)`.
  - Resolving: `at(Resolving, entries read, Some(entries to read))`, the entries being those not `unchanged`.
  - `Cancelled(Resolving)` returns before `BEGIN` and writes no row and no `last_scan_at`/`last_scan_status`.
- `scan::scan_root(conn, root)` is `scan_root_watched` under `Unwatched`, unchanged for every caller.

- [ ] **Step 1: Failing tests.**
  - (a) `a_watched_walk_counts_files_found_then_entries_read_of_entries_to_read`: a root with one subdirectory and three archives. Listing ends at `(3, None)`. Resolving reports `(0,5)` through `(5,5)`: the root, the subdirectory and three files. A second walk over unchanged files reports Resolving `of` 0.
  - (b) `a_walk_cancelled_between_entries_records_nothing`: the watch says cancelled after two entries. The walk answers `Cancelled(Resolving)`; the root has 0 `scan_entry` rows and no `dir_state`; `last_scan_at` and `last_scan_status` are still NULL.
  - Run: neither compiles, then both fail.
- [ ] **Step 2: Implement.** Run `taiyaku-library` whole.
- [ ] **Step 3: Mutations.** The flag check removed: (b) fails. The count taken over every entry seen: (a)'s second walk fails.
- [ ] **Step 4:** Task checks. Commit `Report the walk's counts and stop it between entries`. Report.

---

### Task 4: Progress and cancel in the derive

Spec: §7 "Cancel checks" (classify, write, before the tail); §5 "What a cancel keeps and loses"; §8 "A cancel at each step leaves the store as a stop at that point would".

**Files:**
- Modify `crates/taiyaku-library/src/derive.rs`:
  - `derive_with` (606-905) becomes `derive_watched`, plus a wrapper.
  - The per-root loop (629-660) splits in two: admit and claim over every root first, then classify. That way the bytes to hash are known before the first digest.
    - The split must not change any derive output, and every existing `derive`, `scan` and `serve` test is the guard.
    - Before splitting, read whether one root's classify depends on state that a later root's admit or claim writes, or the other way round (nested roots, shared paths). If it does, stop and report rather than reorder. The fallback is a total computed by a read before the loop, over the same `hashes` predicate.
  - `classify` (1280-1367) and the total share one predicate, `fn hashes(seen: &Seen) -> bool`, so the two cannot disagree. It is true when the entry is not chapter-only, its pages are more than 0, and it has no `sha256`.
  - `Entry` (947) gains `size`; `entries` (1046-1066) and `digest_held_members` (2954-3012) select it.
- Tests in `derive/tests.rs`.

**Interfaces (Produces):** `derive::derive_watched(conn: &Connection, documents: Documents, watch: &dyn Watch) -> Result<Outcome<DeriveReport>, String>`:
- Hashing: `at(Hashing, bytes, Some(total))`. A digest adds its entry's `scan_entry.size`, and so does a file found gone, so `done` ends at `of`.
- Writing: `at(Writing, works written, Some(groups after place))`.
- Finishing: `at(Finishing, 0, None)` as the fold loop (734) begins.
- The flag is read before each `classify`, before each `write_work`, and before the fold loop. Those answer `Cancelled(Hashing)`, `Cancelled(Writing)` and `Cancelled(Writing)`.
- `derive_with` and `derive` are unchanged for every caller.

- [ ] **Step 1: Failing tests**, each over a fixture of three undigested archives of known sizes in two series:
  - (a) `a_watched_derive_counts_bytes_hashed_of_bytes_to_hash_then_works_written`. Hashing goes `(0,S)` to `(S,S)`, where S is the sum of the three sizes. Then Writing `(0,2)`, `(1,2)`, `(2,2)`, then Finishing. A second derive reports Hashing `of` 0.
  - (b) `the_bytes_to_hash_leave_out_what_classify_does_not_hash`: adding a chapter-only entry, a `pages = 0` directory and a digested entry leaves S as it was.
  - (c) `a_derive_cancelled_after_one_digest_keeps_it_and_writes_no_work`: exactly 1 `scan_entry.sha256` set; 0 `work`; `Cancelled(Hashing)`.
  - (d) `a_derive_cancelled_after_one_work_keeps_that_work`: 1 `work`; `Cancelled(Writing)`.
  - (e) `a_derive_cancelled_before_the_tail_raises_and_marks_nothing`: a fixture with an unattached chapter directory. Cancelled after the last work: every work is written, 0 `proposal`, and `work.missing_since` is untouched. The same fixture uncancelled raises at least 1.
- [ ] **Step 2: Implement.** Run `taiyaku-library` and `taiyaku-serve` whole.
- [ ] **Step 3: Mutations.** A check removed at each site: (c), (d) or (e) fails. `hashes` without its `sha256` clause: (b) fails. Size added before the digest instead of after: (a) fails on its first report.
- [ ] **Step 4:** Task checks. Commit `Report the derive's counts and stop it between digests, between works and before the tail`. Report.

---

### Task 5: The pause, the `cancelled` word, and what reads them

Spec: §3 Goal 5; §7 "Pause", "The status word", "`last_scan_at`"; §7 Web "A paused, never-scanned library"; §8 four lines.

**Files:**
- Create `crates/taiyaku-library/src/schema/026_root_paused_at.sql` (`ALTER TABLE root ADD COLUMN paused_at TEXT;`, with a why comment). `schema.rs` `MIGRATIONS` (53-78) grows to 26.
- Modify `crates/taiyaku-library/src/scan.rs`, beside `mark_derive_failed` (2629): add the four functions below. `interrupt_derives` (2830-2837) is untouched.
- Modify `crates/taiyaku-library/src/schedule.rs`: `scheduled` (72-98) reads `paused_at` and leaves a paused directory out.
- Modify `crates/taiyaku-library/src/browse.rs`: `status` (1906-1934). A library is `scanned` only where one of its directories with `dir_state` rows also has a `last_scan_at`.
- Modify `crates/taiyaku-cli/src/library.rs`: `scan` (320-468) calls `scan::unpause` on `selected`, after `select_roots`, before the walks.
- Tests: `schema/tests.rs`, `scan/tests.rs`, `schedule/tests.rs`, `browse/tests.rs`, `crates/taiyaku-cli/tests/cli.rs` (helpers ~279).

**Interfaces (Produces), in `scan`, each writing only where `id` still names `path`:**
- `pub struct Unfinished { pub root: Root, pub last_scan_at: Option<String> }`.
- `pub fn last_scans(conn, roots: &[Root]) -> Result<Vec<Unfinished>, String>`.
- `pub fn mark_cancelled(conn, unfinished: &[Unfinished]) -> Result<(), String>`: `last_scan_status = 'cancelled'`, and `last_scan_at` back to the value held. `scan_root` writes `last_scan_at` as each walk commits, before the derive.
- `pub fn pause(conn, roots: &[Root]) -> Result<(), String>`: `paused_at = datetime('now')`.
- `pub fn unpause(conn, roots: &[Root]) -> Result<(), String>`: `paused_at = NULL`.

- [ ] **Step 1: Failing tests.**
  - (a) `the_migration_gives_every_directory_a_paused_at_of_null`.
  - (b) `a_cancelled_directory_keeps_its_last_scan_and_reads_cancelled`. For a directory walked by `scan_root` (`last_scan_at` now set), `mark_cancelled` with `None` leaves `last_scan_at` NULL and status `cancelled`.
  - (c) `a_cancelled_status_survives_the_servers_start`: `interrupt_derives` leaves `cancelled` and answers 0.
  - (d) `a_paused_directory_is_not_scheduled_though_never_scanned_and_due`, and `…is_scheduled_again_once_unpaused`.
  - (e) `a_library_whose_cancelled_first_scan_committed_its_walk_reads_not_scanned`: `dir_state` rows exist and `last_scan_at` is NULL, so `scanned` is false. The existing `status` tests stay green.
  - (f) `cli.rs`: `library_scan_of_a_paused_directory_clears_its_pause`, and `library_scan_root_of_another_directory_leaves_a_pause`.
- [ ] **Step 2: Implement.** Run `taiyaku-library`, `taiyaku-cli` (rebuild `target/debug/taiyaku` first) and `taiyaku-serve`.
- [ ] **Step 3: Mutations.** `scheduled`'s filter removed: (d) fails. `mark_cancelled` without the `last_scan_at` write: (b) fails. The `scanned` conjunct removed: (e) fails. The CLI call removed: (f) fails. `'cancelled'` added to `interrupt_derives`'s list: (c) fails.
- [ ] **Step 4: SQLite 3.40.1** by the recipe: only the named `strace` tests fail (Global Constraints). Log to `$E/T5-bookworm.log`.
- [ ] **Step 5:** Task checks. Commit `Pause a directory until Scan is pressed, and say cancelled without moving its last scan`. Report.

---

### Task 6: The job: its state, its sink, cancel and pause

Spec: §3 Goals 2, 4, 5; §5 "When Cancel takes effect", "What a cancel keeps and loses"; §7 "Job state", "Cancel checks", "Pause" (accepted press, refused press).

**Files:**
- Modify `crates/taiyaku-serve/src/scan_jobs.rs`:
  - `run` / `run_with` / `run_watched` (168-321), `the_job_lock` (331-357), `lease_for_the_derive` (369-397), `reserve` (518-549), `finish_reservation` (557-575), `spawn` / `spawn_job` (584-642).
- Modify `crates/taiyaku-serve/src/lib.rs`: `ScanProgress` (197) gains `pub job: Arc<scan_jobs::Job>`.
- Modify `crates/taiyaku-library/src/reach.rs`: add `libraries_reaching`.
- Fix the existing `ScanProgress { … }` literals and `spawn_job` closures in `config_api/tests.rs`, `libraries_api/tests.rs`, `scan_jobs/tests.rs` and `scheduler/tests.rs`, where the compiler names them.
- New tests in `scan_jobs/tests.rs`, `scheduler/tests.rs`, `config_api/tests.rs`, `libraries_api/tests.rs`.

**Interfaces (Produces):**
- `pub(crate) struct Job`: `id: u64` (from a process-wide `AtomicU64`, starting at 1), `started: Instant`, `dirs: Vec<(i64, PathBuf)>`, `libraries: Mutex<Vec<(i64, String)>>`. Private: the cancel flag, a `Snapshot`, and `Option<JobEnded>`.
- Methods: `request_cancel(&self)`, `cancelling(&self) -> bool`, `snapshot(&self) -> Snapshot`, `ended(&self) -> Option<JobEnded>`.
- `pub(crate) struct Snapshot { step: Step, done: Option<u64>, of: Option<u64>, dir: Option<(usize, usize)> }`. `dir` is 1-based and set during Listing and Resolving.
- `pub(crate) enum JobEnded { Finished, Cancelled(Step) }`.
- `pub(crate) fn jobs_ended() -> u64`: a process-wide counter, bumped as each job ends.
- `pub(crate) struct Running<'a>`: what a job reports through. It implements `Watch` over its `Job`. `waiting(&self, roots, bool)` replaces the old callback and sets step Waiting. `at_dir(&self, at, of)`.
- `reserve(state, roots, started, library) -> Result<Arc<Job>, String>`. The arguments are unchanged; the job's initial step is Waiting.
- `spawn` and `spawn_job` keep their arguments and take the `Job` from the reservation `started` names. `spawn_job` sets `job.libraries` from `held_libraries` first, then calls the job with `&Running`. `finish_reservation` keeps the entry's `job`.
- `run_watched(library, asked, reread, job_lock, running: &Running, walk: impl FnMut(&Connection, &Root, &dyn Watch) -> Result<Outcome<ScanReport>, String>, derive: impl FnOnce(&Connection, &dyn Watch) -> Result<Outcome<DeriveReport>, String>)`. `run_with` keeps its present closure types and adapts them, so its 13 callers do not change.
- `pub(crate) fn held_libraries(conn, roots) -> Result<Vec<(i64, String)>, String>`, over `reach::libraries_reaching(all: &[Root], dirs: &[i64]) -> Vec<i64>`: each library whose reach (`of_libraries`) meets `dirs`.
- A cancelled job answers each directory it had not finished `{"cancelled": {"step": "<step>"}}`. A slot already answered keeps its error.
- **The cancel algorithm** (not derivable):
  - Pressed jobs (`Reread::Dir`, `Library`) call `scan::unpause(asked)` before the lock, best-effort as `mark_queued` is.
  - The job lock is waited for by `try_lock` every 100 ms, reading the flag between tries. Cancelled there: `unmark_queued`, then `mark_cancelled` and `pause`, and no walk.
  - After `read_again`, `scan::last_scans` reads the directories' `last_scan_at`.
  - The flag is read before each walk. The lease is asked with `Wait::No` in a loop, reading the flag between tries.
  - On `Cancelled(step)`, or the flag at a check: `mark_cancelled` the unfinished (every directory `read_again` kept, less refused and failed walks), and `pause` every directory of `asked`. The derive's tail is not run.

- [ ] **Step 1: Failing tests** (`scan_jobs/tests.rs` unless named):
  - (a) `a_job_reports_its_directory_and_step_as_it_goes`, over two directories on disk. Snapshots in order: Waiting, Listing at dir `(1,2)`, Resolving at `(1,2)`, Listing at `(2,2)`, Resolving at `(2,2)`, Hashing, Writing, Finishing. `ended` is `Finished`.
  - (b) `a_job_cancelled_while_it_waits_for_the_job_lock_ends_without_the_holder`: the test holds the job lock, spawns, cancels, and the reservation is `done` within 2 s with the lock still held. 0 `scan_entry` rows; status `cancelled`; `paused_at` set; report `{"cancelled":{"step":"waiting"}}`.
  - (c) `a_cancelled_first_scan_reads_never_cancelled_and_paused_with_its_digests_kept`, cancelled at the first Hashing report after one digest. Both directories have `last_scan_at` NULL, status `cancelled` and `paused_at` set; 1 digest; 0 works; reports `{"cancelled":{"step":"hashing"}}`.
  - (d) `a_job_cancelled_between_directories_walks_no_more`: the second directory has 0 rows.
  - (e) `a_pressed_scan_clears_the_pause_of_what_it_covers` (Dir and Library), and `a_scheduled_batch_leaves_a_pause`.
  - (f) `a_cancelled_library_scan_pauses_a_directory_another_library_shares`, and `held_libraries` names both libraries for a job holding that directory. It also names the outer library for a job holding a nested directory it reaches.
  - (g) `config_api/tests.rs` `a_refused_scan_directory_leaves_the_pause`, and `libraries_api/tests.rs` `a_refused_scan_library_leaves_the_pause`: a slot is already reserved, so 409, and `paused_at` is unchanged.
  - (h) `scheduler/tests.rs` `the_first_tick_after_a_start_passes_over_a_paused_directory`, and `a_later_tick_passes_over_a_paused_directory_that_is_due`.
  - The existing waiting and queued tests (e.g. `a_scan_queued_behind_another_reads_waiting_until_it_holds_the_job_lock`) stay green.
- [ ] **Step 2: Implement.** Run `taiyaku-serve` and `taiyaku-library` whole.
- [ ] **Step 3: Mutations.** A blocking `lock()` again: (b) times out. `mark_cancelled` given `None` for every directory, not the read value: a variant of (c) with a directory scanned before fails. The check before each walk removed: (d) fails. The unpause on a batch too: (e)'s second case fails. `pause` over `unfinished` only: (b)'s or (f)'s pause assertion fails.
- [ ] **Step 4:** Task checks. Commit `Hold each scan job's step, counts and libraries, and cancel it into a pause`. Report.

---

### Task 7: The feed, Cancel by job, and the pause on the wire

Spec: §7 "The feed", "Cancel endpoint"; §8 "The feed's JSON in both forms, and that its handler makes no store access beyond the guard's".

**Files:**
- Create `crates/taiyaku-serve/src/scan_feed.rs` and `scan_feed/tests.rs`; `mod scan_feed;` in `lib.rs`.
- Modify `crates/taiyaku-serve/src/routes.rs`:
  - `guarded_routes`: after `/api/library/libraries/{id}/scan` (~2496).
  - `grant_postures`: the feed is `Filters`; cancel is `GrantIndependent` with its own reason.
  - `narrowing_postures` (~3174). The typed lists in `auth/tests.rs` (~132, ~3600).
- Modify `crates/taiyaku-serve/src/config_api.rs`:
  - `root_json` (1227-1245) gains `paused_at`.
  - `RootWithScan` and `roots_with_last_scan` (1812-1843) read it.
  - `libraries_api.rs` `libraries_json` (126-176) follows.
- Tests: `scan_feed/tests.rs`, `config_api/tests.rs`, `libraries_api/tests.rs`.

**Interfaces (Produces): the wire, which Tasks 8–11 rely on.**
- `GET /api/library/scans`, for any signed-in account; the handler reads `state.scans` only. The admin form (`session::administers`) is:
  ```json
  {"ended": 4,
   "libraries": [{"id": 1, "step": "hashing"}],
   "jobs": [{"id": 7, "step": "hashing", "done": 1048576, "of": 5368709120, "dir": null,
             "dirs": [{"id": 3, "path": "/srv/Manga/Volumes"}],
             "libraries": [{"id": 1, "name": "Manga"}],
             "elapsed_secs": 812.4, "cancelling": false}]}
  ```
  - `done` and `of` are null for waiting and finishing; `of` is null for listing.
  - `dir` is `{"at": 2, "of": 5}` during listing and resolving, else null.
  - `libraries` lists each library a running job holds, with the step of the lowest-id job holding it.
  - `ended` is `jobs_ended()`: a change marker, never drawn.
- The form for any other account, an admin's device key included, is `{"ended": 4, "libraries": [{"id": 1, "step": "hashing"}]}`. It lists only libraries the caller is granted (`principal.access.grants(id)`); it has no `jobs` key, no count and no path.
- `POST /api/library/scans/{job}/cancel`, admin only (`administering_or_forbidden`):
  - A running job answers 202 `{"job": 7, "cancelling": true}`, however often it is pressed.
  - An ended one answers 200 `{"job": 7, "ended": "finished"}` or `{"job": 7, "ended": "cancelled", "step": "hashing"}`.
  - A job no registry entry names answers 404.
- Each directory in `GET /api/library/roots` and `GET /api/library/libraries` gains `"paused_at": "2026-11-16 03:33:10" | null`. `GET /api/library/roots/{id}/scan`'s `report` may be `{"cancelled": {"step": "hashing"}}`.

- [ ] **Step 1: Failing tests.**
  - (a) `the_feed_answers_an_admin_each_running_job_with_its_step_counts_dirs_and_libraries`: the exact JSON for a job held in Resolving `(120, 2097)` at dir `(2,5)`.
  - (b) `the_feed_answers_a_reader_the_granted_libraries_a_job_holds_with_the_step_alone`:
    - The job holds A's directory and B's directory, which is nested and reached by A. A reader granted B gets exactly `libraries: [{id: B, step}]`.
    - The body has no `jobs` key and does not contain either path.
  - (c) `an_admins_device_key_gets_the_reader_form`.
  - (d) `the_feeds_handler_runs_no_statement`: an in-memory state, `trace` installed on `state.library.conn()`, the handler called directly with a `Principal`: 0 statements.
  - (e) `the_feeds_ended_marker_moves_when_a_job_ends`, including a job that started and ended between two reads.
  - (f) `cancel_pressed_twice_answers_cancelling_both_times`, `cancelling_an_ended_job_answers_how_it_ended`, `cancelling_an_unknown_job_is_404` and `a_reader_cannot_cancel` (403).
  - (g) `a_paused_directory_lists_when_it_was_paused` (roots list, and the libraries list for both libraries of a shared directory).
  - (h) `a_cancelled_directorys_status_says_cancelled_and_at_which_step`.
  - The route-table tests fail until the four tables name both paths.
- [ ] **Step 2: Implement.** Run `taiyaku-serve` whole.
- [ ] **Step 3: Mutations.** A `libraries::name_of` read put in the handler: (d) fails. The grant filter removed: (b) fails. `administers` replaced by a role check: (c) fails. The marker bumped at start instead of end: (e) fails. Cancelling a finished job sets the flag and answers 202: (f) fails.
- [ ] **Step 4:** Task checks. Commit `Serve one scan feed, cancel a job by its id, and send each directory's pause`. Report, with the JSON copied from a test's actual output.

---

### Task 8: The web feed client

Spec: §6 A; §7 Web "One feed client per tab"; §8 "A single poll serves several consumers".

**Files:**
- Create `web/src/scanFeed.ts` and `web/tests/scanFeed.test.tsx` (jsdom: it reads `document.visibilityState`, as `hooks.ts`'s `useFlushOnHide` does at 341-358).
- Create `web/tests/support/scanFeed.ts`: `serveFeed(answer)` stubs `fetch` for `/api/library/scans` (and the cancel POST); `feedJob(partial)` builds a `FeedJob`.
- Modify `web/tests/support/browser-setup.ts`: `resetScanFeed()` after each test, beside `emptyReads()`.
- Modify `web/src/config.ts`:
  - `ScanOutcome` (284) gains `CancelledScan = { cancelled: { step: ScanStep } }` and its guard.
  - `Root` (80-110) gains `paused_at: string | null`; the status-word doc (92-98) names `cancelled`.

**Interfaces (Produces), `scanFeed.ts`:**
- `type ScanStep`; `interface FeedJob` (the Task 7 job); `interface ScanFeed { ended: number; libraries: {id; step}[]; jobs?: FeedJob[] }`.
- `FEED_IDLE_MS = 5000`; `useScanFeed(): ScanFeed | undefined` (`useSyncExternalStore`, as `usePage.ts` does at 97).
- `jobsHolding(feed, libraryId): FeedJob[]`; `jobHoldingDir(feed, rootId): FeedJob | undefined`; `libraryStep(feed, libraryId): ScanStep | undefined`.
- `stepLine(job: FeedJob): string`:
  - waiting: `Waiting for another job`;
  - listing: `Listing · 340 files found`;
  - resolving: `Reading files · 120 of 2,097`;
  - hashing: `Hashing · 1.0 MiB of 5.0 GiB` (`humanBytes`);
  - writing: `Writing works · 40 of 138`;
  - finishing: `Finishing`;
  - during listing and resolving, `` · directory 2 of 5 `` is appended;
  - numbers use `toLocaleString('en')`.
- `cancelScan(job: number): Promise<{job: number; cancelling?: true; ended?: string; step?: ScanStep}>`; `resetScanFeed()`.
- **The feed's algorithm:**
  - The first subscriber starts polling, and the last unsubscribe stops it.
  - Polls are 1000 ms apart while `libraries` is non-empty, else 5000 ms.
  - While hidden it sends nothing. On `visibilitychange` to visible it asks at once.
  - When `ended` differs from the last answer, or a job id or library id seen before is gone, it invalidates `LIBRARY_STATUS`, `LIBRARIES`, `ROOTS` and the `/api/library/libraries/` prefix, and calls `invalidateLibraryViews()` (`pageReads.ts` 27-124).
  - A failed read keeps the last answer and asks again at the idle interval.

- [ ] **Step 1: Failing tests** (`scanFeed.test.tsx`, fake timers `shouldAdvanceTime`, `serveFeed`):
  - (a) `one poll serves every consumer`: three components each call `useScanFeed`. Advancing 5000 ms sends exactly one more request, and all three draw its `ended`.
  - (b) `asks every second while a job runs and every five seconds while none does`.
  - (c) `sends nothing while the tab is hidden and asks at once when it is shown`: 0 requests over 20000 ms hidden, 1 on `visible`.
  - (d) `reads the library status again when a job leaves the feed`: a component reading `LIBRARY_STATUS` through `usePage` fetches `/api/library/status` once, then once more after job 7 leaves.
  - (e) `reads again when ended moves though no job was ever seen`.
  - (f) `stops asking once the last consumer unmounts`.
  - (g) `stepLine` for each step, with the strings above.
- [ ] **Step 2: Implement.** jsdom and node whole.
- [ ] **Step 3: Mutations.** A timer per consumer: (a) fails. One fixed interval: (b) fails. Visibility ignored: (c) fails. Invalidation removed: (d) fails. Only job ids compared: (e) fails. The timer never cleared: (f) fails.
- [ ] **Step 4:** Task checks. Commit `Poll one scan feed per tab and read the library again when a scan ends`. Report.

---

### Task 9: The library page

Spec: §3 Goals 3–5; §7 Web "Library page", "No confirmation dialog", "The per-page polling … is deleted"; closes O273 (library page half).

**Files:**
- Modify `web/src/LibraryPage.tsx`:
  - Deleted: the main poll (769-818), `elapsed` (597, 710-782), the elapsed line (1356-1361), and `watch`'s `press` path, which existed to start that poll.
  - `running` and `scanning` come from `jobsHolding(feed, libraryId)` and their `dirs`. `scansInside` / `scansShared` (281-307) become the names of each job's other libraries.
  - The button (1303-1311) reads **Cancel** while a job holds a directory. It calls `cancelScan` for each such job, with no dialog, and is held while `cancelling`, with "Cancelling…" beside it. Scan library is unchanged otherwise.
  - Once no job holds it and a directory's `paused_at` is set, the line "Paused until Scan is pressed, since <`inUtc`(earliest)>" is drawn.
  - The `ended` effect (860-886) runs when a job holding this library leaves the feed, keeping its one `scanStatus` read per covered directory.
  - The held poll (827-854) runs only while the feed shows no job. A hold the feed explains is read again at the job's end.
  - `WIDEST_ENDS` (450-458) gains `cancelled`.
- Tests: `web/tests/LibraryPage.test.tsx` (its mocks 30-44, fake timers 184), and new `web/tests/scanEnds.test.tsx` (fetch-level, so that it runs at main).

**Interfaces:** Consumes Task 8's `useScanFeed`, `jobsHolding`, `stepLine`, `cancelScan`, `serveFeed`, `feedJob`, and `Root.paused_at`.

- [ ] **Step 1: Failing tests.**
  - (a) `shows a scan it did not start within one idle poll`, mounted idle. A job holding this library appears in Resolving `(120, 2097)` at dir `(2,5)`. After 5000 ms:
    - the button reads Cancel;
    - the line reads `Reading files · 120 of 2,097 · directory 2 of 5`;
    - no text matches `/\d+s\b/`.
  - (b) `Cancel cancels each job holding the library and draws Cancelling…`: POST `/api/library/scans/7/cancel` once; then, with `cancelling: true`, the button is held.
  - (c) `a cancel made elsewhere ends in the paused line and Scan library` (Review Focus 5). The feed goes cancelling, then empty; the list answers `paused_at: "2026-11-16 03:33:10"`. The line reads `Paused until Scan is pressed, since ` followed by `inUtc('2026-11-16 03:33:10')`, and the button reads Scan library.
  - (d) `names the other libraries a job covers`: a job over Manga and Comics names Comics on Manga's page.
  - (e) `sends no scan-status poll while the feed shows a job`: the `libraryScanStatus` mock's calls do not grow over five 1000 ms ticks.
  - (f) `scanEnds.test.tsx` `a library page reads its list again when a scan it did not start ends (O273)`. Last scan reads Never. A job appears and leaves; the list then has `last_scan_at`; Last scan shows that time within one 1000 ms poll.
- [ ] **Step 2: Watch (f) fail at main 210ec5e3** in the scratch worktree, on its Last scan assertion. Record the message.
- [ ] **Step 3: Implement.** jsdom whole, node whole.
- [ ] **Step 4: Mutations.** The idle poll at 50000 ms: (a) fails. `cancelling` ignored: (b) fails. The paused line keyed on `last_scan_status` instead of `paused_at`: (c) fails. The old main poll restored: (e) fails. The feed's invalidation removed: (f) fails.
- [ ] **Step 5:** Task checks. Commit `Draw a library's scan from the feed, with Cancel, its step and its pause (O273)`. Report.

---

### Task 10: The directory page

Spec: §3 Goals 3–5; §7 Web "Directory page"; closes O273 (directory half); §9 O257.

**Files:**
- Modify `web/src/RootRow.tsx`:
  - The doc comment's "Live progress is elapsed seconds and nothing else" (51-54) becomes the why of the step line: every figure is the server's.
  - Deleted: the mount read's adoption (841-862), `begin`'s poll (972-1023), `elapsed` (543) and `waiting` (545).
  - `running` comes from `jobHoldingDir(feed, root.id)`. The button (1290-1300) reads Cancel, and `Cancelling…` follows.
  - The line (1316-1321) is `stepLine(job)`.
  - The paused line comes from `root.paused_at`.
  - At the job's end, one `scanStatus(root.id)` read for the report keeps 991-997's restart text.
  - The report draws `CancelledScan` as `Cancelled while <step>`.
- Modify `web/src/DirPage.tsx`: `useOtherScan` (110-217) takes a sibling's scan from the feed. Its `libraryScanStatus` re-read runs only while the feed shows no job (`moves_held`, `derive_running`).
- Tests: `web/tests/RootRow.test.tsx` (fake timers 157), `web/tests/DirPage.test.tsx` (135), `web/tests/scanEnds.test.tsx`.

- [ ] **Step 1: Failing tests.**
  - (a) `Scan directory reads Cancel while any job holds the directory, whoever started it`, within one idle poll.
  - (b) `the step line replaces elapsed seconds`.
  - (c) `Cancelling… while the cancel is pending, then the paused line`.
  - (d) `reads its report once when the job ends, and polls nothing while it runs`: `scanStatus` is called 0 times during the job and once after.
  - (e) `a cancelled report says at which step`: `Cancelled while hashing`.
  - (f) `scanEnds.test.tsx` `a directory page reads its facts again when a scan it did not start ends (O273)`: Last scan and next scan change within one poll.
  - (g) O257's shape: `a directory's own scan named beside a sibling's draws Scanning`. Run it at main too. O257 closes only if it fails there.
- [ ] **Step 2:** Watch (f) and (g) at main 210ec5e3; record each message, or (g)'s pass.
- [ ] **Step 3: Implement.** jsdom whole.
- [ ] **Step 4: Mutations.** `jobHoldingDir` matched on library instead of directory: (a) fails. `scanStatus` read on every feed answer: (d) fails. The `cancelled` branch removed: (e) fails.
- [ ] **Step 5:** Task checks. Commit `Draw a directory's scan from the feed, with Cancel, its step and its pause (O273)`. Report, with O257's outcome.

---

### Task 11: The Libraries table and the reader's empty view

Spec: §3 Goal 3; §7 Web "Libraries table", "Reader's empty view" (with a filter or search, when the job ends, paused never-scanned); §8 Web.

**Files:**
- Modify `web/src/LibraryTable.tsx`: the Last scan cell (122-125) reads `Scanning · <stepLine>` while `jobsHolding` names the row's library.
- Modify `web/src/EmptyState.tsx`:
  - `EmptyKind` (56-67) gains `scanning`: `<h2>Scanning</h2>`, then `stepLine(job)` for an admin and the step's word alone for anyone else.
  - `no-matches` (202-211) takes an optional `scanning` line, `A scan is still running.`
- Modify `web/src/Library.tsx`:
  - `emptyKind` (109-124) takes the feed. For the library shown (or, with none shown, any listed library) a held library gives `scanning` in place of `not-scanned` and of an unfiltered `no-matches`. With a search (196-197) or chip filter (258-263) active, the view keeps `no-matches` and adds the line.
  - The view subscribes to the feed only while it draws an empty state.
- Tests: `web/tests/Library.test.tsx` (441-476), `web/tests/EmptyState.test.tsx`, `web/tests/Libraries.test.tsx`, `web/tests/scanEnds.test.tsx`.

- [ ] **Step 1: Failing tests.**
  - (a) `the Libraries table shows a library a job holds as Scanning with its step's count`, within one idle poll.
  - (b) `a library a job holds draws Scanning, never Not read yet`, with `scanned` false.
  - (c) `…and never nothing here`, with `scanned` true and no works.
  - (d) `a reader sees the step alone`: the reader form, `Hashing`, with no digit in the section.
  - (e) `with a search active the view keeps its no-match state and says a scan is still running`.
  - (f) `a paused never-scanned library shows Not read yet to a reader`: the feed is empty and `scanned` false (Task 5).
  - (g) `scanEnds.test.tsx` `the reader's view reads again when the scan ends`: works drawn within one poll after the job leaves.
  - (h) `the view asks the feed nothing while it draws works`: 0 feed requests over 20000 ms.
- [ ] **Step 2: Implement.** jsdom whole. If CSS changed, the browser project whole.
- [ ] **Step 3: Mutations.** `scanning` only replacing `not-scanned`: (c) fails. Counts for every account: (d) fails. The filter case drawing Scanning: (e) fails. An unconditional subscription: (h) fails.
- [ ] **Step 4:** Task checks. Commit `Show a running scan in the Libraries table and the reader's empty view`. Report.

---

### Task 12: The ledger, the notes, and the docs

Spec: §8 "Status rows". The close date is `date +%F` when this runs. Every figure is measured or quoted from a file named beside it.

- [ ] **Step 1: Open, then close, four rows from O310** in `docs/status.md`'s Open items table (rows end at O300; lane 2 holds O301–O309):
  - O310: the slow walk;
  - O311: the reader's "Not read yet" during a scan;
  - O312: no progress;
  - O313: no Cancel, and its pause.
  - Each row names its site by symbol: `pages.rs`'s `page_names_from`; `Library.tsx`'s `emptyKind`; `scan_jobs::Job` and `scanFeed.ts`; `scan_jobs::run_watched` and `scan::pause`.
  - Opened by "the owner's first NAS import (2026-11-16)". Each `[x]` with `**Closed <date> by the first scan's progress and Cancel: …**`.
- [ ] **Step 2: Close O273** (row ~1899, bullet ~4654) by the five-part edit. Name `scanEnds.test.tsx`'s two O273 cases and Task 9/10's at-main messages. Close O257 only if Task 10 (g) failed at main. O259 and O260 stay open unless a test failing at main says otherwise.
- [ ] **Step 3: Open four rows for later**, unchecked, not built:
  - O314: PR 2's series as they finish;
  - O315: help for "How it is read";
  - O316: the directory picker's buttons above a long listing;
  - O317: an experimental compact picker list.
- [ ] **Step 4: Bullets.** After "The books PR of 2026-11-15" (~5029), before "## What comes next" (~5110), add `**The first scan's progress and Cancel of <date>**` (the spec's path). Add a bullet for each of O310–O317 and O273, saying what was done or what is open.
- [ ] **Step 5: Counts.**
  - Run the open-items sweep at the time: rows, closed, `What remains` (`grep -a -n -A1 '^What remains:'`).
  - Fix the head count word, the "are now closed" sentence (~1499) and "What remains" (~5631).
  - Set the Rust and web test counts (~183) from fresh `--reporter=default` runs and the gate's JUnit figure.
  - Every table row has exactly six `|`.
- [ ] **Step 6: Notes.** Add `## The first scan: the walk on NFS, the guard, and Cancel (<date>)` at the end of `docs/notes.md`:
  - the bench's three lines, quoted from `$E/walk-*.txt` and labelled NFS;
  - the guard's line;
  - Task 2's strace counts before and after.
- [ ] **Step 7: Docs.**
  - README:
    - the Libraries paragraph (~320-327) gains Cancel, the step line and the pause;
    - "scans each directory when it starts, unless its schedule is Off" (~552) gains "or a Cancel paused it until Scan is pressed".
  - `docs/superpowers/specs/2026-09-04-c4-screens-design.md` (229-232, 666) gets italic lines beginning `*Replaced <date> by the first-scan design:*`.
  - Then `rg -n -i 'elapsed|not Off|interrupted|O273|Not read yet' README.md docs web/src`: read every hit and correct present-tense prose that is now false.
- [ ] **Step 8:** Commit `Close O273 and the first scan's rows, open PR 2's and PR 3's, and record the walk's figures`. Report.

---

### Finish

1. **Whole-branch review** of `main..HEAD` in three lanes, each against the spec and the Review Focus: (a) Rust, Tasks 2–7; (b) web, Tasks 8–11; (c) docs, Task 12.
   - Fix rounds go back to the same implementer by SendMessage.
   - After a fix, grep for the finding's shape, and review the fixes too.
   - A fix that adds tests after Task 12 re-measures the counts.
2. **Gate** on the final commit (Gate line), then **SQLite 3.40.1** again if SQL changed after Task 5.
3. **The drive**, on a live server, set up by the controller, which starts and stops the server itself.
   - **Setup:**
     - `D=$W/target/first-scan-drive` (on disk; `target/` is gitignored); `date -Iseconds`; `touch $D/marker`; `sha256sum /home/davey/temp/manga-test/library.db >| $D/original.sha256`.
     - `cp -r`, never `ln`, six series of `/home/davey/temp/manga-test/Volumes` into `$D/lib/Volumes`, `du -sh` ≤ 4 GiB.
     - A fresh `$D/library.db`: `printf 'pw\npw\n' | <bin> library user add local --role admin --db …`, and `reader` with `--role user`.
     - A **debug** build copied to `$D/taiyaku`: its slow hashing keeps each step on screen, and nothing is timed.
     - `vite build --outDir $D/dist`; `serve --db $D/library.db --cache $D/cache --web $D/dist --dirs $D/lib --addr 127.0.0.1:<free port> --insecure-cookies`, the port checked with `ss -lptn`.
   - **Driven:**
     - Make library Manga from `$D/lib/Volumes` on Configuration → Libraries, and `library user grant-library reader Manga`.
     - The scheduler's tick starts the first scan. Watch the steps count on the Libraries table, the library page, the directory page and, as `reader`, the reader's view with the step alone. Screenshot each.
     - Cancel when the line says Reading files; read Cancelling…, then the paused line, the `cancelled` word and Never.
     - Scan library, then Cancel during Hashing. The reader sees Not read yet.
     - Restart the server by PID: for 45 s the feed shows no job, and the paused line stays.
     - Scan library again: it completes, works appear, the reader's view reads again, and the pause is gone.
   - **Record and clean up:**
     - Readings go to `$E/drive.md` and the notes section.
     - Stop by PID and check the port. `find /home/davey/temp/manga-test -newer $D/marker` prints nothing, and `sha256sum -c $D/original.sha256` passes.
     - Delete `$D` in its own Bash call by its literal path.
4. **Claim check** of the status rows, bullets, notes, README lines and the PR body: pass 1 includes the PR body, and later passes are scoped to the correction commit.
5. **PR body** at `$E/pr-body.md`, its title the first line, measured under 255 characters. It names the spec corrections and the damaged-entry guard's count. Ask the owner before pushing.

**Spec coverage.**

| Spec requirement | Task |
|---|---|
| Goal 1 | 2 (guard 1) |
| Goal 2 | 3, 4, 6, 7 |
| Goal 3 | 8–11 |
| Goal 4 | 6, 7, 9, 10 |
| Goal 5 | 5, 6, 9, 10 |
| §7 job state, sink, cancel checks | 3, 4, 6 |
| `cancelled` word | 5, 6 |
| `last_scan_at` | 5, 6 |
| pause column, migration, schedule, presses, CLI | 5, 6 |
| feed and its store cost | 7 |
| cancel endpoint | 7 |
| feed client | 8 |
| per-page polls deleted | 9, 10 |
| §8 Rust | 2–7 |
| §8 Web | 8–11 |
| §8 by hand | Finish |
| Walk timing | 1, 2 |
| status rows | 12 |

---

### As built

*The plan above is kept as written and superseded here, per this project's convention. What shipped differs from it in five places:*

- **A folder's Hashing bytes** are the summed sizes of the page files its digest reads (`derive::bytes_to_hash`, over `taiyaku_read::member_page_sizes`), not its `scan_entry.size`, which for a folder is its directory's own size. Task 4's "a digest adds its entry's `scan_entry.size`" holds for archives only.
- **Hashing is first reported with no total**, `at(Hashing, 0, None)`, while the folders are sized, and the flag is read before each folder is sized, answering `Cancelled(Hashing)`. So the feed's `of` is null for hashing until the total is known, and the step line then reads "Hashing · sizing files".
- **The ended-jobs counter belongs to the server's state** (`ServeState::jobs_ended`), not to a process-wide `jobs_ended()`, so a test's feed marker counts only its own server's jobs.
- **A pressed job clears its pause again once it holds the job lock** (`scan_jobs::run_watched`), beside `unmark_queued`: a rules save that holds the store's write lock past the busy timeout refuses the clearing made before the wait.
- **Task 12** also opened O318 and O319, and left `docs/status.md`'s test counts to Finish, after the final review's fixes. O320 and O321 came from the whole-branch review.

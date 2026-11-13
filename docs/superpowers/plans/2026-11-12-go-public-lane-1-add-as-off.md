# Go-public lane 1: Add as Off, and the README (O275, O276) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close O275 and O276.
- O275: an added directory is scanned at the scheduler's next tick, so its edition key, patterns or Off cannot be set before its first scan. Both add forms get a box, "Don’t scan yet (schedule Off)", that adds it Off.
- O276: the README's `$host` lines, its restore steps, its compose block's missing WUD labels, and its silence about an add starting a scan.

**Architecture:**
- Task 1: the store writes the asked schedule in the add's own `INSERT`; both add `POST`s read `scan_interval_secs` through the check the `PATCH` uses; a link keeps the shared schedule and its answer says so (`schedule_kept`).
- Task 2: the client sends the field, and both forms draw the box.
- Task 3: what the pages say: a kept shared schedule beside the link's line, and an Off directory never scanned beside its "Never".
- Task 4: the README, D1–D4.
- Task 5: the closes in `docs/status.md`.
- Then the Finish.

No migration: `root.scan_interval_secs` exists since migration 1.

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`), TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`.

**Spec:** `docs/superpowers/specs/2026-11-12-go-public-design.md`, §3 (lane 1), §7 (overlaps) and §9 ("Lane 1"). Cited as "spec §n". The survey is `.superpowers/sdd/deploy-evidence/survey-hardening.md`, sections A and D. Locations are by symbol; line numbers are hints at main ec4eb11c.

**The owner's rulings of 2026-11-12** (binding; do not re-open; `.superpowers/sdd/deploy-evidence/rulings.md`):
- Build "Add as Off". The add form's "Don't scan yet (schedule Off)" starts **unticked**.
- The builder's calls, told to the owner: the field is `scan_interval_secs` on both add `POST`s, in the `PATCH`'s vocabulary; a directory another library already holds keeps its shared schedule and the answer says so; it stays Off after the first Scan press; the CLI's `library scan --add` is unchanged.
- Do the README fixes: `$host` vs `$http_host`, the restore's `-wal`/`-shm` and `chown 99:100`, the WUD labels, and that an add starts a scan.

**Closed decisions this plan must not reverse:**
- Pre-deploy lane 3 decisions 1–3 (`2026-11-10-pre-deploy-design.md` ~646-656): one schedule per directory, shared by its libraries; Automatic NULL, Off 0, Every at least 300 seconds.
- Pre-deploy lane 3 decision 11: the add still probes, and `added_json` still reports the probe. An Off add probes too: a probe is not a scan.
- O263 (`docs/status.md`): the README restores by a **move**, which `schema/tests.rs`' `a_database_moved_into_place_between_two_failed_migrations_is_backed_up_again` rests on. Never "copy".

**What the plan settles** (the plan's calls; the controller tells the owner):
- **Signatures.** `add_root_checked_in` keeps its signature (160 call sites at ec4eb11c, most of them fixtures) and becomes `add_root_checked_with(…, None)`. `add_root_in` keeps its signature (92 call sites) and passes `None` to `insert_root`. `create_library` gains the parameter: of its 21 call sites, all but `libraries_api::create_library`'s are tests. The checked paths call `insert_root` directly, as their paths are already canonical.
- **The store writes what it is handed.** The range check lives in `taiyaku-serve`, as it does for the `PATCH` (`scan::update_root` checks nothing either).
- **`schedule_kept`** is true only for a link whose stored `scan_interval_secs` is not equal to the one asked. An absent field asks for Automatic (`None`), per spec §3 decision 2. So an *unticked* add of a link whose shared schedule is Off or Every also answers `schedule_kept: true`, and the page says the schedule it keeps. Raw values are compared, not `schedule::every`'s reading, because Automatic and an Every of the same length are different settings.
- **Names.** `dir_path_from_json` becomes `new_dir_from_json`, as it now reads two fields.
- **Client.** `createLibrary` and `addDir` take a third parameter, `scanOff = false`. The forms always pass it, so existing `toHaveBeenCalledWith(id, path)` assertions gain `false`.
- **Words** (the plan is where they first exist; tests assert them as literals):
  - The box: `Don’t scan yet (schedule Off)`, with U+2019, as `NewLibrary.tsx`'s own help writes `library’s`.
  - New library's row help: `Its schedule is then Off: it is scanned only when Scan library or Scan directory is pressed, so its edition key and patterns can be set before its first scan.`
  - The link's line with a kept schedule: `<path> is also a directory of <names>. It keeps the settings it has there, and its scan schedule stays <schedule>.`, where `<schedule>` is `automatic`, `off`, or `every ` and `humanInterval`'s figure (`every 1h`, `every 90m`). Without a kept schedule the line is as today.
  - An Off directory never scanned: the library page's Last scan cell reads `Never · until Scan is pressed`, and the directory page's last scan fact `never · until Scan is pressed`, the dot being `JOINING_DOT`. Every other "Never"/"never" is as today.
  - The library page's box has no help line: the add's room is measured for the link's line (`roomOf`, `placeLinked`), and the README and New library say what Off does.
- **The box on the library page** starts unticked each time the picker is opened, and keeps its tick across a refused add, as the typed path does. Ticking either box takes away a refusal drawn for the form, as an edit of the name or path does.

**New interfaces this plan names:**
- `taiyaku-library`, `scan.rs`:
  - `pub(crate) fn insert_root(conn: &Connection, canonical: &Path, library: InLibrary<'_>, scan_interval_secs: Option<i64>) -> Result<Root, String>`: the `INSERT` writes `path` and `scan_interval_secs`; the `Root` it returns carries the value written.
  - `pub fn add_root_checked_with(conn: &Connection, path: &Path, library: InLibrary<'_>, scan_interval_secs: Option<i64>) -> Result<Added, AddRootError>`: today's body of `add_root_checked_in`.
  - `pub fn create_library(conn: &Connection, name: &str, paths: &[PathBuf], scan_interval_secs: Option<i64>) -> Result<Created, AddRootError>`: one value for each directory it inserts.
  - `Added` gains `pub schedule_kept: bool`. `linked` gains the asked value as a parameter.
- `taiyaku-serve`:
  - `config_api::scan_interval_from_json(body: &serde_json::Value) -> Result<Option<Option<i64>>, String>`, `pub(crate)`: absent `None`, null `Some(None)`, a number `Some(Some(n))`, with the `PATCH`'s two sentences. `root_patch_from_json` calls it.
  - `libraries_api::asked_interval(body: &serde_json::Value) -> Result<Option<i64>, AdminAnswer>`, private: the above, flattened, its error as `AdminAnswer::Unusable`.
  - `libraries_api::new_library_from_json` answers `(String, Vec<PathBuf>, Option<i64>)`; `libraries_api::new_dir_from_json` answers `(PathBuf, Option<i64>)`.
  - Each add answer (`config_api::added_json`) gains `"schedule_kept": <bool>`.
- `web/`:
  - `libraries.ts`: `createLibrary(name: string, dirs: string[], scanOff = false)` and `addDir(id: number, path: string, scanOff = false)`. Ticked sends `scan_interval_secs: 0`; unticked sends no such key.
  - `config.ts`: `Added.schedule_kept: boolean`; `keptSchedule(added: Pick<Added, 'schedule_kept' | 'root'>): string | undefined`, which answers `<schedule>` above when kept and `undefined` otherwise; `lastScan` takes `Pick<Root, 'last_scan_at' | 'last_scan_status'> & Partial<Pick<Root, 'scan_interval_secs'>>`.
  - The link's shape in `Libraries.tsx` (`madeLink`) and `LibraryPage.tsx` (`linkedAtMaking`, `linked`, `LinkedWords`) gains `schedule?: string`.

## Spec gaps

1. **D2's "They name the host directory under the compose mapping" covers both the restore comment (README ~226) and the roll-back paragraph (~677-693).** The ~226 line is a comment in "Using it"'s command-line walkthrough, whose database is `$XDG_DATA_HOME/taiyaku/library.db` or `--db PATH`, not the container's. This plan names the three files there and points to "Running it on a server". The roll-back paragraph alone names `/mnt/user/appdata/taiyaku/config/taiyaku/` and `chown 99:100`. The owner may overrule.
2. **§3 "Where" says `added_json` already answers the stored `scan_interval_secs`.** That holds for a link, read inside the add. For a new directory, `insert_root` hard-codes `scan_interval_secs: None` in the `Root` it returns. Task 1 returns the value written.

## Global Constraints

- **Parallel lanes.** Lanes 1, 2 and 3 run at once, each in its own git worktree, on a branch cut from main once the spec PR merges (BASE; provisionally ec4eb11c).
  - `W=/home/davey/projects/taiyaku/.claude/worktrees/go-public-1`. Every command runs against `$W` by absolute path. Never `cd` (it is zoxide here); where a tool must run inside a directory, write `builtin cd <dir> && …` as one call.
  - `E=/home/davey/projects/taiyaku/.superpowers/sdd/deploy-evidence/lane-1`, absolute, in the MAIN checkout, so reports survive the worktree. `mkdir -p $E` if it is missing. Logs and each task's report (`$E/T<n>-report.md`) go there, written by Bash (`cat >| … <<'EOF'`); the Write tool is refused under `.superpowers/`. Never write evidence under `target/`.
  - Never set a shared `CARGO_TARGET_DIR`: the worktree builds into `$W/target`.
  - `$W/web/node_modules` is a real copy, never a symlink (`test ! -L $W/web/node_modules`), and `$W/crates/taiyaku-wasm/pkg` is present; the controller prepares both. Never `pnpm exec`; call `./node_modules/.bin/<tool>` directly.
  - Lane 2 also edits `README.md`. This lane edits only the five places Task 4 names, so the two merge cleanly.
- **Gate.** The repo's own `scripts/check-all.sh`, never a copy. It runs `cd "$(git rev-parse --show-toplevel)"`, so start it from `$W`:
  - `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 ./scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`
  - Wait for load < 24 (`cat /proc/loadavg`) before starting it, and record the load.
  - Read the `.exit` file, never the harness's exit code. Quote the log's `all green` line. Never pipe the gate through `tail`.
- **Rust, per task.** From `$W`: `cargo nextest run -p <crate> --test-threads=8 >| $E/T<n>-nextest-<crate>.log 2>&1`, then `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`. Read the summary lines from the log.
- **Web, per task.** From `$W/web`:
  - Tests: `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`. Whole projects, never a named file.
  - Then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests` the same way.
  - Tasks 2 and 3 run the whole browser project too: the add's area and the Last scan column are measured there (`Config.browser.test.tsx`, `Libraries.shell.browser.test.tsx`). CI never runs it.
- **Browser crashes.** A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing. Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project. The Filmstrip "Tab brings focus to the rail" assertion also goes red under load, on main too.
- **Pre-existing failures.** A failure in a file the task did not touch is pre-existing only if the same command fails at BASE, not at another commit of this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one:
  - Put the named defect back by hand and watch the test fail.
  - Restore, and check with `git -C $W diff` that the restore is complete.
  - Record each in the task report: the defect, the failing test, and the assertion's message.
- **Sentences are literals.** A test asserts a sentence written out in the test, never one computed through the helper under test.
- **Real interaction.** jsdom tests that navigate do it outside `act`, with real clicks, as `act` hides transition races.
- **Comments say why**: the reason, the invariant, what breaks if it changes; never history or measurements; an O-number may be cited. `scripts/check-comments.mjs` and `scripts/comment-budget.json` enforce part of this through the gate; if a share fails, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **No attribution** anywhere: code, comments, docs, commits, PR body.
- **The Rust test total.** `docs/status.md` states it twice: "Six crates, N tests" (~179) and "of the N are standing checks" (~256). Task 1 adds Rust tests and raises both by the number it adds, in its own commit; the gate's `status-test-count` step checks them against the run.
- **Commits.** One commit per task, its subject naming the O-number(s). Never `--amend`. Check each new hash is a descendant of the one before.
- **SQL.** Task 1 changes an `INSERT`, so it runs the library and serve crates' tests in `rust:1-bookworm` (SQLite 3.40.1) before the Finish (Task 1, Step 9).

## Review Focus

The five inputs the spec implies and its tests do not exercise, most likely first. Each has its test in the owning task.
1. **A shared directory added unticked whose shared schedule is Off.** The person expects a scan, and none comes. The link's line must say "its scan schedule stays off". (Task 3, Step 1.)
2. **A body whose `scan_interval_secs` is a string or a fraction** (`"0"`, `0.5`). It must answer 400 in the `PATCH`'s number sentence, with nothing added. (Task 1, Step 1, the refusal test.)
3. **The picker opened again after a ticked add, or after Cancel.** The box must be unticked again, or the next directory is added Off by surprise. (Task 2, Step 1.)
4. **A restart between an Off add and its first press** (WUD redeploys on every merge). The startup scan must skip it. This is pinned already by `serve_scans_each_directory_but_an_off_one_once_it_listens` (serve `tests.rs`), which Task 1 runs unchanged.
5. **An Off directory after its first Scan press.** It must stay Off and out of the schedule (spec §3 decision 4). (Task 1, Step 1, the last test.)


## Pre-flight rulings (controller, 2026-11-12; binding, override the task text where they differ)

From the pre-flight scan `.superpowers/sdd/deploy-evidence/lane-1/preflight.md` (0 BLOCKER, 0 Important, 14 Minor). BASE is **28b18b60** (the spec PR's merge; docs-only over ec4eb11c, so every line hint holds).

- **Task 1.**
  - Step 4 also names the mutation for test 4: "the floor written `<=` (300 refused), or `null` refused as not a number: test 4 fails".
  - Test 3's named mutation is "an inline `as_i64` read with no range check" (`patch_nullable_i64` is private to `config_api`).
  - Test hints: the tests return `()`, so use `.expect(…)` not `?`. Read the column and the schedule in a block that drops `state.library.conn()` before any `.await`: the in-memory library's `working()` takes the same mutex (scheduler.rs ~84), so holding it across `until_the_library_scan_ends` or a POST hangs.
  - Step 5's doc sweep also runs `grep -an 'add_root_in\]\|add_root_checked_in\]'` over `scan.rs`, `config_api.rs` and `libraries_api.rs` and re-reads every hit (known: scan.rs ~368, ~418, ~477, ~502; config_api.rs ~1248 `added_json`), correcting each that now names the wrong function. The long doc lives on `add_root_checked_with`; `add_root_checked_in`'s doc says it is the Automatic shorthand and points there.
  - "The Rust test total … in its own commit" means **in Task 1's commit** (one commit per task).
- **Task 2.**
  - Files also include the existing tests that pin New library's form and the call arguments: `web/tests/Libraries.test.tsx` ~2493-2532 (the O175 rule: every box's `aria-label` equals its label, so the box carries `aria-label="Don’t scan yet (schedule Off)"`; rename the "…one box of Name and Directory path…" test and update its parts list), `web/tests/fieldNames.browser.test.tsx` ~449-457 ("whose form's two boxes are the only ones on it"), and the call-argument assertions gaining `false` (LibraryPage.test.tsx ~703, ~1237, ~2365; Libraries.test.tsx ~1822, ~1967, ~2112, ~3798, ~3827, ~3901, ~3953, ~3999, ~4679; Config.browser.test.tsx ~530). Line numbers are hints: find every `toHaveBeenCalledWith` on `createLibrary`/`addDir` by grep.
  - Step 3 also names the mutation "the refusal unticks the box: the refused-add case fails".
  - The library page's box: Ruling — it goes right after the `DirPicker` element as the plan says. `.add-root > .dir-picker { display: contents }` (admin.css ~1705) may make it follow Cancel and the open directory list; that is accepted for now, and the Finish drive checks its place with Browse… open at 390 and 1280 and reports. Cost if wrong: one markup move after the drive.
- **Task 2/3 words.** New library's row help becomes exactly: `A directory no other library holds is then scheduled Off: the server scans it only when Scan library or Scan directory is pressed, so its edition key and patterns can be set before its first scan. One another library holds keeps its schedule.` (the plan's sentence was false for a link).
- **Task 3.** `keptSchedule` is written over `scanEvery` (config.ts ~445), prefixing `every ` only for a positive interval; it does not restate `scanEvery`'s arms.
- **Task 5.** Step 2's Rust total check: run a workspace nextest from `$W` (`cargo nextest run --workspace --test-threads=8 >| $E/T5-nextest.log 2>&1`), then `node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md` from `$W`, and quote its line.
- **Finish drive.** The marker file goes in the scratch dir, never inside /home/davey/temp/manga-test. "Never · until Scan is pressed" is checked on the library's page; the Libraries table keeps "Never" (by the plan's own "every other Never").
- **Spec gaps (added).** 3: `add_root_checked_in` keeps its signature and the new path is `add_root_checked_with`, where spec §3 "Where" reads as if the existing function gains the parameter.
- **Cross-lane, for the record.** Lanes 2 and 3 rebase onto this lane: they re-measure the Rust total and node/jsdom counts, redo the closed/open counts, and lane 3 passes `None` to any new `create_library(`/`insert_root(` call.

---

### Task 1: The add takes a schedule (O275, the server)

Spec §3 decisions 2–6 and "Where", first three bullets; §9 "Lane 1". Survey A, "Add paths" and "Touch".

**Step 0 (this task only):** copy this plan to `$W/docs/superpowers/plans/2026-11-12-go-public-lane-1-add-as-off.md`. It rides in this task's commit.

**Files:**
- `crates/taiyaku-library/src/scan.rs`:
  - `insert_root` (~199-257): the `INSERT` (~215) and the returned `Root` (~246-256).
  - `add_root_in` (~189): passes `None`.
  - `add_root_checked_in` (~504): its body moves into `add_root_checked_with`; its `Vetted::New` arm calls `insert_root`.
  - `linked` (~585-614) and `probed` (~814-825): `schedule_kept`.
  - `Added` (~373-392): the new field, and `Added::interval`'s doc.
  - `create_library` (~854-946): the parameter, `insert_root` for a new path, the asked value to `linked`.
  - The docs that say the row stays Automatic: `add_root_checked_in`'s point 3 (~463-469) and `probed`'s (~811-813).
- Test call sites of the changed signatures: `create_library(` in `scan/tests.rs` and `libraries_api/tests.rs` (~5348), and `insert_root(` in `scan/tests.rs` (~2405) and `schema/tests.rs` (~2200, ~2230). Each passes `None`.
- `crates/taiyaku-serve/src/config_api.rs`: `root_patch_from_json` (~1288-1325) and the new `scan_interval_from_json` beside it; `added_json` (~1256) and its doc.
- `crates/taiyaku-serve/src/libraries_api.rs`: `new_library_from_json` (~312), `create_library` (~335), `dir_path_from_json` → `new_dir_from_json` (~447), `add_dir` (~459) and its doc, which names `add_root_checked_in`.
- Tests: `crates/taiyaku-serve/src/libraries_api/tests.rs`, beside `a_library_is_created_with_its_dirs_and_then_listed` (~415) and `a_dir_is_added_to_a_library_by_the_librarys_id` (~695). Both of those tests' `keys_of` lists gain `"schedule_kept"`.
- `docs/status.md`: the two Rust totals.

**Interfaces:** produces everything under "New interfaces" for `taiyaku-library` and `taiyaku-serve`. Task 2 relies on the wire shape: the field is `scan_interval_secs`, and each add answer carries `schedule_kept`.

- [ ] **Step 1: Write the failing serve tests.** Use the file's helpers: `answered`, `get_json`, `the_library`, `the_store`, `a_dir_of`, `keys_of`, `until_the_library_scan_ends`. Read the stored column with `SELECT scan_interval_secs FROM root WHERE id = ?1` on `state.library.conn()`. Read the schedule with `taiyaku_library::schedule::scheduled(&conn, &taiyaku_library::schedule::now(&conn)?)`. The refusal sentences, as literals:
  - range: `the directory's \`scan_interval_secs\` must be null (automatic), 0 (off) or at least 300 (5 minutes)`
  - not a number: `the directory's \`scan_interval_secs\` must be a number or null`

  The tests:
  1. `a_library_made_off_stores_off_in_each_dir_and_leaves_them_out_of_the_schedule`.
     - `POST /api/library/libraries` with two new directories and `"scan_interval_secs": 0`.
     - Both rows store `0`; each `added[i]["root"]["scan_interval_secs"]` is `0` and `schedule_kept` is `false`; `scheduled` lists neither id.
     - Control in the same test: a second library made from a third directory with no field stores NULL, and `scheduled` lists it with `due == true`.
  2. `a_dir_added_off_stores_off_and_is_left_out_of_the_schedule`: the same over `POST /api/library/libraries/{id}/dirs`, with its own no-field control.
  3. `both_adds_refuse_an_interval_the_patch_refuses_in_its_words`.
     - For each `POST`, and each of `299`, `-1`, `"0"` and `0.5`: 400, and a body equal to `{"error": <sentence>}`.
     - `299` and `-1` get the range sentence; `"0"` and `0.5` the number sentence.
     - `the_store(&state)` is unchanged after each.
  4. `both_adds_store_automatic_and_five_minutes`: for each `POST`, `null` stores NULL and `300` stores 300, each answered 200.
  5. `a_link_keeps_its_shared_schedule_and_says_so_when_the_add_asked_for_another`, over `state_with_two_scanned_roots`.
     - Fixture check first: root-a's directory stores NULL.
     - `POST` it into root-b's library with `0`: `schedule_kept` is `true`, `root.scan_interval_secs` is null, `also_in` is `["root-a"]`, and the stored column is still NULL.
     - Make a library from `[a new directory, root-a's directory]` with `0`: the new one stores 0 with `schedule_kept: false`; the held one keeps NULL with `schedule_kept: true`.
     - Link root-b's directory (stored NULL) into the library just made, with no field: `schedule_kept` is `false`.
  6. `an_off_dir_is_scanned_by_scan_library_and_stays_off`.
     - Over `router_over_a_single_scanned_work`, as `scan_now_walks_each_dir_and_derives_once` is, make a library Off over `a_dir_of`'s directory (one two-page volume).
     - `POST` `the_scan_of(id)` (202), then `until_the_library_scan_ends`.
     - The directory reads `last_scan_status` `ok`, still stores `0`, and `scheduled` still leaves it out.

  Run `cargo nextest run -p taiyaku-serve`. Tests 1–5 fail. Test 6 fails at the make, as the field is ignored and the directory stores NULL. Record each failure.
- [ ] **Step 2: The store.** In `scan.rs`, add the parameter to `insert_root` and `create_library`, and add `add_root_checked_with`, as under "New interfaces".
  - `linked(conn, into, held, asked: Option<i64>)` sets `schedule_kept: held.scan_interval_secs != asked`. It writes no schedule: the row is shared, and one library must not silence another's scans (spec §3 decision 3). `probed` sets `false`.
  - Rewrite the docs that say the row stays Automatic: the row stores what the add asked; the probe's interval is still reported and not written.
  - `Added::schedule_kept`'s doc says why it exists: a link keeps the shared row's schedule, so the answer says when that is not what was asked.
  - Update the test call sites to pass `None`.
- [ ] **Step 3: The routes.**
  - Lift the interval check out of `root_patch_from_json` into `scan_interval_from_json`; `root_patch_from_json` calls it. The `PATCH`'s behaviour and sentences do not change.
  - `asked_interval` flattens it for the `POST`s.
  - `new_library_from_json` and `new_dir_from_json` read it. `create_library` passes it to `scan::create_library`, and `add_dir` passes it to `scan::add_root_checked_with`.
  - `added_json` sends `schedule_kept`. Its doc names the field.
  - Run `cargo nextest run -p taiyaku-serve` and `-p taiyaku-library`; all pass, including `config_api/tests.rs`' `a_roots_patch_refuses_an_interval_under_five_minutes_and_one_below_zero`, `a_roots_patch_stores_off_automatic_and_five_minutes`, and serve `tests.rs`' `serve_scans_each_directory_but_an_off_one_once_it_listens`, unchanged.
- [ ] **Step 4: Watched mutations.** Each must turn the named test red:
  - The field ignored by `create_library`'s route (pass `None`): test 1. The same in `add_dir`: test 2.
  - A check of the `POST`s' own: `asked_interval` reads `patch_nullable_i64` with no range check: test 3. Or answers another sentence: test 3.
  - The shared row rewritten: `linked` runs `UPDATE root SET scan_interval_secs = ?2 WHERE id = ?1` with the asked value: test 5.
  - `schedule_kept` always `true`: test 5's last link. Always `false`: test 5's first link.
  - `scan::create_library` applies the value to its first new path only: test 1.
  - `insert_root` writes the value but returns `scan_interval_secs: None`: tests 1 and 2, on the answer's `root`.
  - The first press flips Off to Automatic: add `, scan_interval_secs = NULLIF(scan_interval_secs, 0)` to `scan_root`'s `UPDATE root SET last_scan_at = …` (~2533): test 6.
- [ ] **Step 5: Docs sweep.** `grep -arn 'dir_path_from_json\|stays Automatic' $W/crates` gives no hit. Every doc comment that says what an add writes says it as now.
- [ ] **Step 6: The Rust totals.** Raise "Six crates, N tests" and "of the N are standing checks" in `docs/status.md` by the number of tests added (six as written above).
- [ ] **Step 7: Run.** `cargo nextest run -p taiyaku-library` and `-p taiyaku-serve`, then clippy and fmt as in Global Constraints.
- [ ] **Step 8: Commit** with a subject such as "Both add routes take scan_interval_secs in the PATCH's words, write it in the add's INSERT, and a link keeps its shared schedule and says so (O275)". Write `$E/T1-report.md`.
- [ ] **Step 9: SQLite 3.40.1.** Run the library, serve and cli crates' tests in `rust:1-bookworm`, from a `git -C $W archive HEAD` export under `$W/target/bookworm/`. The recipe is the memory "SQLite 3.40.1 in CI and the image": `--user`, `RUSTUP_TOOLCHAIN` as the image names it, `--test-threads=6`. Log to `$E/T1-bookworm.log`. Only the two `strace` tests may fail. If a later fix changes this SQL, run it again.

---

### Task 2: The box, and the client that sends it (O275, the web's request)

Spec §3 decision 1 and "Where", fifth bullet; §9 "Lane 1"'s jsdom sentence.

**Files:**
- `web/src/libraries.ts`: `createLibrary` (~342), `addDir` (~357).
- `web/src/NewLibrary.tsx`: a `Row` with `check` after "Directory path" (~162-175); the box's state; `createLibrary(name, [dir], scanOff)` in `submit` (~120). The file's header gains a sentence on the box.
- `web/src/LibraryPage.tsx`: the add's area (`div.add-root`, ~1450-1505): the box, drawn while `adding`, after the `DirPicker`; the "Add directory…" press (~1475) unticks it; `add` (~1150) sends `addDir(library.id, path, scanOff)`; a tick takes `addFailure` away.
- CSS only if the box needs it, beside the `.add-root` rules in `web/src/styles/admin.css` (~519-600, ~1378).
- Tests: `web/tests/libraries.test.ts` (node), `web/tests/Libraries.test.tsx` (New library, jsdom), `web/tests/LibraryPage.test.tsx` (Add directory, jsdom). Existing `toHaveBeenCalledWith` on `createLibrary`/`addDir` (e.g. `LibraryPage.test.tsx` ~703 and ~1237, `Libraries.test.tsx` ~1822) gain `false`.

**Interfaces:**
- Consumes: Task 1's wire field `scan_interval_secs`.
- Produces: `createLibrary(name, dirs, scanOff = false)` and `addDir(id, path, scanOff = false)`. Both boxes are `input[type="checkbox"][name="scan-off"]`, named by the words `Don’t scan yet (schedule Off)`.

- [ ] **Step 1: Write the failing tests.**
  - `libraries.test.ts`: `createLibrary('Manga', ['/media/a'], true)` sends `{ name: 'Manga', dirs: ['/media/a'], scan_interval_secs: 0 }`, and `addDir(7, '/media/c', true)` sends `{ path: '/media/c', scan_interval_secs: 0 }`. With `false`, and with the parameter left out, the parsed body `not.toHaveProperty('scan_interval_secs')`. The existing `it.each` rows stay.
  - `Libraries.test.tsx`, New library:
    - The box is found by its label's words and is unticked on arrival.
    - A make with it unticked calls `createLibrary('Manga', ['/media/Manga/Volumes'], false)`; ticked, `true`.
    - Ticking it after a refusal takes the refusal away.
  - `LibraryPage.test.tsx`, Add directory…:
    - The box is unticked when the picker opens.
    - Unticked sends `addDir(7, path, false)`; ticked, `(7, path, true)`.
    - After a ticked add answers, Add directory… again shows it unticked. Do the same after Cancel.
    - Ticked, then a refused add: the box stays ticked, and a tick takes the refusal away.
  - Run the node and jsdom projects; the new cases fail.
- [ ] **Step 2: Implement.** Run node and jsdom to green, then `tsc -b`, `biome check`, and the whole browser project.
- [ ] **Step 3: Watched mutations.**
  - The client sends `scan_interval_secs: null` when unticked: `libraries.test.ts`. It sends nothing when ticked: the same.
  - New library's box starts ticked (`useState(true)`): the arrival case. It sends `false` always: the ticked case.
  - The library page's box is not unticked at "Add directory…": the reopen cases. It sends `true` always: the unticked case.
  - A tick leaves the refusal: each refusal case.
- [ ] **Step 4: Commit** `… "Don’t scan yet (schedule Off)" … (O275)`. Write `$E/T2-report.md`.

---

### Task 3: What the pages say: a kept schedule, and an Off directory never scanned (O275, the web's words)

Spec §3 "Where", fifth bullet: "A kept shared schedule is said, naming the libraries. An Off directory never scanned says so beside 'Never'."

**Files:**
- `web/src/config.ts`: `Added` (~130-138) gains `schedule_kept`; `keptSchedule` beside `scanEvery` (~447); `lastScan` (~470-477) and `regimeFacts`' `last` (~516-519) gain the Off-never arm, which reads Off through `isOff` (~439).
- `web/src/LibraryPage.tsx`: `LinkedWords` (~395-404) takes `schedule?: string`; `linkedAtMaking`'s prop type (~172), the `linked` state (~522-535) and `add` (~1150-1166) carry `schedule: keptSchedule(added)`; both `LinkedWords` uses (~1496, ~1501) pass it.
- `web/src/Libraries.tsx`: `madeLink`'s type (~365-367) and `made` (~381-401) carry it.
- Fixtures that build an `Added`: `tsc -b` lists every one (in `web/tests/Libraries.test.tsx`, `LibraryPage.test.tsx` and `Config.browser.test.tsx`). Each gains `schedule_kept: false`.
- Tests: `web/tests/config.test.ts` (node), `LibraryPage.test.tsx` and `Libraries.test.tsx` (jsdom).

**Interfaces:** consumes Task 1's `schedule_kept` and Task 2's forms. Produces `keptSchedule` and the widened `lastScan`.

- [ ] **Step 1: Write the failing tests.**
  - `config.test.ts`:
    - `keptSchedule` answers `undefined` for `schedule_kept: false`, whatever the root.
    - For `schedule_kept: true`, it answers `automatic` for null, `off` for 0 and for -1, `every 1h` for 3600 and `every 90m` for 5400.
    - `lastScan({ last_scan_at: null, last_scan_status: null, scan_interval_secs: 0 })` is `'Never · until Scan is pressed'`. With `null` or `86400`, it is `'Never'`; with the interval left out, `'Never'`.
    - Tighten the existing "says off for an interval below zero" `regimeFacts` case: its `last scan` becomes `'never · until Scan is pressed'`. The Off case scanned at 12:00 keeps its time.
  - `LibraryPage.test.tsx`:
    - The Directories table case (~1050): `/media/Manga/Oneshots` (Off, never scanned) now reads `'Never · until Scan is pressed'`. The never-scanned `24h`, `90m` and `150m` rows still read `'Never'`, and that is what tells the arm from one keyed on "never scanned" alone.
    - In "a directory another library holds too" (~3247): an add answering `schedule_kept: true`, with the root's `scan_interval_secs` `null`, draws `'/m is also a directory of Manga. It keeps the settings it has there, and its scan schedule stays automatic.'`
    - The same with `0`, the Review Focus 1 case, draws `… stays off.`
    - The existing cases, answering `schedule_kept: false`, keep today's sentence.
  - `Libraries.test.tsx`, "a library made over a directory another library holds" (~2760): the same make answering `schedule_kept: true` with `3600` draws `'/media/Manga is also a directory of Manga. It keeps the settings it has there, and its scan schedule stays every 1h.'`
  - Run node and jsdom; the new cases fail (and `tsc -b` lists the fixtures).
- [ ] **Step 2: Implement**, and add `schedule_kept: false` to the fixtures `tsc -b` names. Run node, jsdom, `tsc -b`, `biome check`, and the whole browser project.
- [ ] **Step 3: Watched mutations.**
  - `lastScan`'s arm keyed on `last_scan_at === null` alone: the table case's `24h` row and `config.test.ts`.
  - The arm removed: the Oneshots row.
  - `regimeFacts`' arm removed: the below-zero case.
  - `keptSchedule` ignores `schedule_kept`: the existing link sentences.
  - `LinkedWords` drops `schedule`: the add's kept cases.
  - `made` drops it from `madeLink`: the New library case.
  - `keptSchedule` writes the interval without `every `: the `every 1h` case.
- [ ] **Step 4: Commit** `… (O275)`. Write `$E/T3-report.md`.

---

### Task 4: The README, D1–D4 (O276)

Spec §3, D1–D4; survey D. Wording is the builder's (spec §8). What each place must say is fixed here, and the claim check reads every changed sentence against the code.

**Files:** `README.md`, these five places only:
1. **D1**, "Using it", the `--trusted-proxy` comment block (~154-155): `Host` and `X-Forwarded-Host` take `$http_host`, as the NPM block does (~623-624), for the reason its bullet gives (~638-640). The comment may point there; it must not restate the NPM block.
2. **D2a**, the restore comment (~226): stop the server; move `library.db`, `library.db-wal` and `library.db-shm` aside; move the backup in as `library.db`, owned by the account the server runs as; start it. Point to "Running it on a server" for the container (Spec gap 1).
3. **D3**, the compose block (~497-510): add, under the service, the labels the paragraph at ~669 names, in the list form the NAS's own file uses (`nas-report.md` §2):
   ```yaml
       labels:
         - wud.watch=true
         - wud.watch.digest=true
         - wud.trigger.include=docker.redeploy
   ```
4. **D4**, the schedule paragraph (~543-548) and "Then sign in and make the libraries…" (~557-566):
   - An added directory's first scan **starts** at the scheduler's next tick, within about 30 seconds. "Starts" is the right verb: the walk takes as long as reading its files, and it waits behind another scan.
   - Unless "Don’t scan yet (schedule Off)" is ticked, on New library or Add directory….
   - That is how to set an edition key or patterns on the directory's page before its first scan; then press Scan directory or Scan library.
   - It stays Off after that press, until its schedule is changed.
   - A directory another library already holds keeps the schedule it shares.
5. **D2b**, the roll-back paragraph (~677-693): after pinning the older image, stop the container. In `/mnt/user/appdata/taiyaku/config/taiyaku/` (the host side of `/config`, ~507, holding `library.db` and `backups/`, ~490): move `library.db`, `library.db-wal` and `library.db-shm` aside; move the chosen `backups/library-<stamp>-before-v<N>.db` in as `library.db`; `chown 99:100 library.db`; start the container. Say why the `-wal` goes: one left beside a restored file can be applied over it. Keep "restore by `mv`" and the sentences after it as they are (O263).

Do not touch the NPM block, its "Why the whole `location`" list, the `TAIYAKU_TRUSTED_PROXY` steps, or the WUD paragraph at ~669: lane 2 may edit around them.

- [ ] **Step 1: Check each claim before writing it**, and record the check in the report:
  - The tick: `scheduler.rs` `TICK` and `Scheduler::tick`'s `first || due`.
  - The due rule: `schedule::scheduled`.
  - The Scan presses walk an Off directory: `start_library_scan` (`libraries_api.rs`) and `config_api::start_scan`.
  - A link keeps the schedule: Task 1's `linked`.
  - The host path: the image's `XDG_DATA_HOME=/config` in `Dockerfile` and the compose mapping.
  - The labels: `nas-report.md` §2.
- [ ] **Step 2: Edit the five places.**
- [ ] **Step 3: Checks.**
  - `git -C $W diff -U0 README.md`: every hunk lies in one of the five places.
  - `grep -n '\$host' $W/README.md`: the only hit left is the bullet that contrasts `$http_host` with `$host`.
  - Write the edited compose block to `$E/compose-check.yaml` and run `docker compose -f $E/compose-check.yaml config --quiet`; exit 0.
- [ ] **Step 4: Commit** `The README's proxy example sets $http_host, its restore moves the -wal and -shm aside and gives the owner, its compose block carries the WUD labels, and it says an added directory is scanned at the next tick unless added Off (O276)`. Write `$E/T4-report.md`.

---

### Task 5: The closes in `docs/status.md`

Read the Open items section's head, both rows, and the last notes paragraphs before "## What comes next".

- [ ] **Step 1: Close O275 and O276** with the five-part edit:
  1. Each row's checkbox: `[x]`.
  2. Each row's text: "Closed <date>: …", saying what was done, by symbol, with no line numbers. O275 names `scan::add_root_checked_with`, `scan::create_library`, `config_api::scan_interval_from_json` and the box. O276 names the four README fixes.
  3. The notes: no bullet exists for either, so add a paragraph after the last lane paragraph of the notes, in its shape ("**Lane 1 of the go-public spec** … closed O275 and O276 on <date>; their bullets follow."), with one bullet for each.
  4. The "… are now closed — … — leaving M" sentence: the count word, O275 and O276 in the list, and M.
  5. "What remains: the … open items": its count word wraps across a line break, so grep the opener and read the next line.

  Derive every count from the table at this commit, not from this plan. Another lane may have closed rows first; at rebase, redo the counts (spec §7).
- [ ] **Step 2: Counts.**
  - Check the Rust totals Task 1 set, with the gate's `status-test-count` line.
  - Measure the web counts. Run the node and jsdom projects and set "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one" from their summary lines. The browser figure moves only if a task added a browser test; none does.
- [ ] **Step 3: Sweep.**
  - `grep -a -c '^| O[0-9]' $W/docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' $W/docs/status.md` and `grep -a -n -A1 '^What remains:' $W/docs/status.md`. Check them against the sentences.
  - Grep the bare `O275` and `O276` across `$W/docs` and `$W/web/src`, and read every hit. Prose that speaks of either as open is corrected or marked. The spec and other plans stay as written.
  - Check each edited row has exactly six `|`.
- [ ] **Step 4: Commit** `Close O275 and O276 in status.md`. Write `$E/T5-report.md`.

---

### Finish

1. **Whole-branch review** of `BASE..HEAD`, in two lanes:
   - (a) Tasks 1–3: the store, the routes, the client and the pages, against spec §3, the rulings and the closed decisions above.
   - (b) Tasks 4–5: the README and the closes.

   One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **Final gate** on the final commit, at load < 24. Read the `.exit` file.
3. **The drive**, on a live server over a scratch copy of the test library. A subagent drives with chrome-devtools; the controller starts and stops the server.
   - **Set up.**
     - Make a scratch dir in the session scratchpad. Record `sha256sum` and `stat` of `/home/davey/temp/manga-test/library.db`, and `touch` a marker there.
     - `cp` the db into the scratch dir. Never open the original.
     - In the **copy**, set every root Off (`UPDATE root SET scan_interval_secs = 0`). Neither the startup scan nor a tick may walk the test library's own directories, and the drive never presses Scan on their libraries.
     - Make two scratch directories, `off/` and `on/`, each holding a `cp` of one small archive from the test library. `cp`, never a hard link.
   - **Serve.**
     - Build `cargo build --release -p taiyaku-cli` from `$W`, and copy the binary into the scratch dir.
     - Build the web with `builtin cd $W/web && ./node_modules/.bin/vite build --outDir <scratch>/dist --emptyOutDir`.
     - Set the copy's one account's password: `printf 'pw\npw\n' | <scratch>/taiyaku library user password local --db <scratch>/library.db`.
     - Check the port is free (`ss -lptn 'sport = :4411'`), then `serve --db <scratch>/library.db --cache <scratch>/cache --web <scratch>/dist --addr 127.0.0.1:4411 --insecure-cookies`.
   - **Check**, at 1280 and 390 px:
     - New library over `off/` with the box ticked. Past two ticks (more than 60 s), its Last scan reads "Never · until Scan is pressed". `sqlite3 -readonly` on the copy shows `scan_interval_secs` 0 and `last_scan_at` NULL.
     - Set its edition key on its page and press Scan directory. It scans, the scan reads ok, and Scan every still reads off.
     - Add directory… `on/` unticked, to the same library. It is scanned within about 30 s.
     - Make a third library over `on/` with the box ticked. The link's line says the schedule stays automatic, and `on/` still stores NULL.
     - Open Add directory… again: the box is unticked.
   - **Tear down.**
     - Stop the server by PID, and check the port again.
     - `find /home/davey/temp/manga-test -newer <marker>` is empty, and the db's sha256 is unchanged.
     - Then delete the scratch dir.
   - Record everything in `$E/drive.md`.
4. **Claim check** of the closes, every changed README sentence against the code, and the PR body. Put the PR body in pass 1, and scope later passes to the correction commit.
5. **The PR body** at `$E/pr-body.md`, its first line the title. Measure the title under 255 characters. Ask before pushing.

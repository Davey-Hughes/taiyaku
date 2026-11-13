# PR A: a reader is sent file names (O280), and three housekeeping items (O281, the freshness sentence, the coverage guard) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- O280: the work document a non-administrator is sent names each member by its file name, never by the server's path.
- O281 (opened and closed here): a fast burst of input into New library's directory path no longer drops a keystroke or throws React's error #185.
- `docs/status.md`'s `taiyaku-serve` row stops claiming "a day's freshness" for the byte routes.
- `scripts/check-all.sh` refuses to measure coverage over a `target/llvm-cov-target` another rustc built.

**Architecture:**
- Task 1: `get_sidecar` asks a new `session::administers` and, for anyone it does not admit, sends a copy of the view's document whose member paths are file names, tagged over that copy.
- Task 2: `DirPicker` calls `onPath` beside each `setPath`, and the effect goes.
- Task 3: a small guard script, called by `check-all.sh` before its first step.
- Task 4: `docs/status.md`: close O280, open and close O281, the freshness sentence, the counts.
- Then the Finish.

No migration, no SQL, no change to `taiyaku-core`, `taiyaku-wasm`, `View` or `Sidecar::to_json`.

**Tech Stack:** Rust (`taiyaku-serve`), TypeScript/React (`web/`), bash.

**Inputs.** There is no spec. The inputs are:
- the survey `.superpowers/sdd/deploy-evidence/o280/survey.md` (read at main 4b9d1d01; it marks each claim read or inferred), cited as "survey §n";
- the owner's rulings in `.superpowers/sdd/deploy-evidence/rulings.md`, section "2026-11-13".

Locations are by symbol. Line numbers are hints at BASE = main 4b9d1d01.

**The owner's rulings of 2026-11-13, as this plan reads them** (final; do not re-open):
1. **O280.** A non-administrator is sent each member's FILE NAME only (not omitted, not relative), per caller.
   - An administrator keeps the full stored path, so the correction's `ETag` stays as it is.
   - "Administrator" means the existing gate `session::administering_or_forbidden` admits the caller: a browser session or the account's password, never a key. So an administrator's API key gets file names.
   - The view is built in `get_sidecar`, and its `ETag` is computed over the document sent.
   - `Sidecar::to_json` (the on-disk format) and `View` stay unchanged.
   - No role-only check: `auth::tests::no_module_answers_the_role_question_alone` forbids it.
2. **O281.** Open a row for the dropped keystroke (React #185: `DirPicker` reports each change to its parent from an effect) and fix it here: call `onPath` beside each `setPath`, drop the effect, rewrite the comments. The regression test is a burst that fails on today's code, watched failing before the fix.
3. **`docs/status.md` ~392.** Replace the "a day's freshness" claim with the survey's corrected sentence, after re-reading each route's `Cache-Control`.
4. **Coverage guard, "Guard in check-all".**
   - `check-all.sh` records the rustc version beside `target/llvm-cov-target` when it measures.
   - Before measuring, it refuses when a recorded version differs from the current one, with a message naming the folder to delete and both versions.
   - It never deletes anything itself. A first run with no record writes one and proceeds.
   - One line goes in `docs/ci.md`'s toolchain-bump paragraph.

**What the plan settles** (the controller's calls, told to the owner; not rulings):
1. **The predicate is `session::administers(extensions) -> bool`.** It answers whether a `Principal` is present and `administering_or_forbidden` admits it, by calling the gate and dropping its refusal.
   - One answer for "refuse" and for "shape", so the two cannot drift. The cost is a 403 built and dropped on a non-administrator's document read.
   - A missing `Principal` is not an administrator, as `auth::access_of` reads a missing one as seeing nothing.
   - `session.rs` is not in the guard test's source list. A role-only `administers` would pass that test, so Task 1's key test is what holds the rule.
2. **The shaping is `routes::with_file_names`**, a private function beside `filter_sidecar_editions`.
   - It clones the document and sets each member's `path` to `Path::file_name`'s text.
   - A path with no file name (`/`, `..`, empty) is sent as `""`, never as the path.
   - Every edition is mapped, and so is a relative path: a document-backed work stores paths like `../../Manga/…` (`docs/sidecar.md`, "Scope"), which name folders.
3. **The notes-tag guess test takes the stored path.** `View::etag_naming_numbering` hashes each sent member's stored `path` (`view.rs` ~185), and ruling 1 keeps `View` unchanged. So `manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent` builds its guesses from the administrator's paths, matched by `sha256`. Otherwise it passes for the wrong reason (survey §5, item 3).
4. **The coverage guard is its own script**, `scripts/coverage-build-guard.sh`. `check-all.sh` calls it after the hidden-Rust-sources check and before the lint job, under the same tool condition as the coverage block.
   - Every case then runs in under a second by calling the script directly, and one real `check-all.sh` run shows the gate refusing before its first step.
   - **The record** is `<dir>.rustc`: `target/llvm-cov-target.rustc` by default. It holds the one line `${RUSTC:-rustc} -V` prints, e.g. `rustc 1.100.0 (0ef1bc386 2026-11-09)`, which carries the release and the commit hash.
   - **`<dir>`** is resolved as `${CARGO_LLVM_COV_TARGET_DIR:-${CARGO_TARGET_DIR:-target}/llvm-cov-target}`, relative to the repository's top. No cargo config here sets `build.target-dir`.
   - **A record whose folder is gone is rewritten, not obeyed.** Ruling 4 is silent on this. Without it, deleting the folder as the message says would not clear the refusal.
5. **O281's taken pick reads a ref.** A ref holding the box's current text is written beside each `setPath`. A taken pick reads it to decide whether to empty the box and tell `onPath('')`. The call stays out of the functional updater, which must stay pure: StrictMode calls an updater twice. `onPath` is no longer called on mount.
6. **The freshness sentence** is the survey's (§6b), with one correction found on re-reading (spec gap 2).
7. **`view.rs` is not touched**, not even its comments (spec gap 3). Lane 4 edits `view.rs` (`anchor_sent`, `slot_of_anchor`).
8. **New Rust tests go inside `routes::tests::narrowing`, directly after `a_narrowed_document_has_its_own_etag`**, not at the module's end, where lane 4 adds its tests.
9. **The drive uses a script-launched Playwright Chromium on port 4405.** Lane 4's drive uses the chrome-devtools MCP browser on 4404 at the same time, and parallel drives have collided in the shared MCP browser before.
   - The test library has no edition of more than one member, so its chapter readout reads "no chapter edition" for every work. The drive therefore adds a two-member work from a scratch tree.

**New interfaces this plan names:**
- `crate::session::administers(extensions: &axum::http::Extensions) -> bool`, `pub(crate)`, placed directly after `administering_or_forbidden`.
- `routes::with_file_names(sidecar: &taiyaku_core::Sidecar) -> taiyaku_core::Sidecar`, private, beside `filter_sidecar_editions`.
- `scripts/coverage-build-guard.sh`: no arguments; it reads the environment above. It exits 0 after writing the record, and 1 when it refuses. Mode 100755, `#!/usr/bin/env bash`, `set -euo pipefail`, run from the repository's top as `check-all.sh` is.
- `DirPicker`'s `onPath` contract: it is called in the same handler as each change to the box (typed, browsed to, emptied by a taken pick), never on mount, and never from an effect.

**The guard's behaviour** (Task 3; `<dir>` and `<record>` as above; `<now>` is `${RUSTC:-rustc} -V`'s line; `<was>` is the record's line):

| `<dir>` | `<record>` | does |
|---|---|---|
| absent | any or none | creates `<record>`'s directory if missing, writes `<now>`, exits 0 |
| present | none | writes `<now>`, exits 0 |
| present | `<was>` = `<now>` | exits 0 |
| present | `<was>` ≠ `<now>` | writes nothing, deletes nothing, prints the refusal to stderr, exits 1 |

**The refusal, exactly** (three lines on stderr; `<dir>` as resolved, `<was>` and `<now>` verbatim):
```
error: <dir> holds a coverage build made by <was>, and this run would build with <now>.
The report would read both builds' test binaries, and count every line the older ones never ran as uncovered.
Delete <dir> and run this again.
```

## Spec gaps

There is no spec. These are places where the survey or a ruling is wrong against the code, or silent. Each says what the plan does.
1. **Survey §5, item 2 is wrong.** `a_hidden_work_is_refused_before_its_validator_is_answered` will not go to 200.
   - Its fixture (`test_support::two_editions_in_a_library`) is an imported document whose member paths are already bare file names (`c1.cbz` …). `import` stores `member.path` as the document has it.
   - So the file-name copy equals the stored document, and the tag is unchanged.
   - The plan expects the test to stay green unchanged. If it goes red, the implementer stops and reports.
2. **The survey's freshness sentence (§6b) misses one case.** A library cover asked for without a matching `?v=` is still answered by `serve_page`, with an `ETag` and `private, no-cache`. The survey's sentence implies everything on the tile's route has no validator. Task 4's sentence names that case.
3. **`View::etag_naming_numbering`'s comment** says "All else hashed is what it is sent". After O280, the member `path` it hashes is the stored one, which a non-administrator is no longer sent.
   - The security argument is unaffected: the stored path is a further secret, not a guessable one.
   - Ruling 1 keeps `View` unchanged, and lane 4 edits `view.rs`, so the plan leaves the sentence. The PR body names it as a one-clause follow-up for the owner.
   - Superseded by a controller ruling before dispatch: since this PR makes the sentence false, Task 1 fixes that one clause of the comment, and no code in `view.rs` changes (this overrides settled call 7 for that comment).
4. **Ruling 4 is silent on a record whose folder is gone.** The plan rewrites it (settled call 4).
   - A folder that predates the guard is trusted on its first run: there is no record to compare.
   - At BASE no checkout or worktree on this machine has a `target/llvm-cov-target`.
5. **Whether jsdom reproduces React #185 is inferred** (survey §6a). Task 2 tries jsdom and falls back to the browser project. Either way the test is watched failing at BASE before the fix.

## Global Constraints

- **Paths.**
  - `W=/home/davey/projects/taiyaku/.claude/worktrees/o280-housekeeping`, branch `o280-housekeeping`, cut from BASE = main 4b9d1d01.
  - `E=/home/davey/projects/taiyaku/.superpowers/sdd/deploy-evidence/o280`, absolute and in the MAIN checkout, so reports survive the worktree.
  - Every command names `$W` by absolute path. Never `cd`: it is zoxide here. Where a tool must run from a directory, write `builtin cd <dir> && …` within that one call.
- **Before Task 1**, the controller:
  - makes `$W/web/node_modules` a real copy, never a symlink;
  - copies `crates/taiyaku-wasm/pkg` in from the main checkout.
  - Never `pnpm exec`; call `./node_modules/.bin/<tool>` directly.
  - The worktree is fresh, so it has no `target/llvm-cov-target` from before the 1.100.0 pin.
- **The parallel branch: lane 4** (anchors on the wire, O240/O244), planned at `.superpowers/sdd/deploy-evidence/plans/2026-11-13-go-public-lane-4-anchors.md`. It runs at the same time in `.claude/worktrees/go-public-4`.
  - **PR A merges FIRST, and lane 4 rebases onto it.**
  - **Files both touch.** At rebase, `git diff` of each must show only the named functions changed:
    - `crates/taiyaku-serve/src/routes.rs`: PR A changes `get_sidecar` and adds `with_file_names` beside `filter_sidecar_editions`. Lane 4 changes `get_notes`, `put_notes`, `get_progress` and `put_progress`.
    - `crates/taiyaku-serve/src/routes/tests.rs`:
      - PR A edits `both_libraries_are_sent_raw_whole` and `manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent`.
      - PR A inserts its new tests after `a_narrowed_document_has_its_own_etag`, all inside `mod narrowing`.
      - Lane 4 adds tests to `mod narrowing`.
    - `docs/status.md`: PR A closes O280, adds and closes O281, and rewrites the ~392 sentence. Lane 4's last task edits O240 and O244 and the counts.
    - `scripts/comment-budget.json`: either may move a share.
    - `scripts/check-all.sh`: PR A adds one guarded call. Lane 4 only runs the script.
    - `docs/sidecar.md`: PR A adds one sentence to the `Edition` section's `path` bullet. Lane 4 may edit the notes documents' paragraphs further down.
  - **Keep PR A's change to the shared writers minimal.** PR A touches no notes or progress handler, and no correction (`patch_alignments`) line.
  - **PR A does not touch** `taiyaku-core`'s `sidecar.rs` (`Member`, `to_json`), `view.rs`, `taiyaku-wasm` or `web/src/resolver.ts`. The conditional overlaps that lane 4's plan lists for those files therefore do not arise.
- **Commits.** One commit per task, its subject naming the O-number(s). Never `git commit --amend`. Before reporting, check with `git -C $W log --oneline 4b9d1d01..HEAD` that each task's commit is an ancestor of HEAD.
- **Comments say why**: the reason, the invariant, what breaks if it changes. Never history or measurements, never a commit hash or a version figure; a comment may point at an O-number.
  - `scripts/check-comments.mjs` and `scripts/comment-budget.json` hold this in the gate. The `scripts` area has a budget too, and Task 3 adds a file to it.
  - Run `node $W/scripts/check-comments.mjs --base <the task's start head>` before committing.
  - If a share fails, follow `docs/ci.md`, "The comment guard" and "Raising a budget, deliberately", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **The gate.** Run the repo's own `scripts/check-all.sh`, never a copy or a modified one.
  - It runs from the worktree root, so the call is `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`.
  - Wait for load < 24 (`cat /proc/loadavg`) and record it.
  - Read the `.exit` file, never the harness's exit code, and quote the `all green` line.
  - Never pipe the gate through `tail`.
  - The tasks run no gate; the Finish does.
- **Rust, per task.** Whole crates, never a named test.
  - `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml -p taiyaku-serve --test-threads=8 >| $E/T<n>-nextest-serve.log 2>&1`. Read the summary lines from the log.
  - Then `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`.
  - Never set `CARGO_TARGET_DIR`: `$W` builds into `$W/target`.
- **Web, per task.** Whole projects, never a named file.
  - `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/T<n>-<project>.log 2>&1`.
  - Then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`, the same way.
  - A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing. Read `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
  - The Filmstrip "Tab brings focus to the rail" case also goes red under load, on main too.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at BASE 4b9d1d01, not at another commit of this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one.
  - Put the named defect back by hand and watch the test fail.
  - Restore, and check with `git -C $W diff` that the restore is complete.
  - Record in the report the defect, the failing test and the assertion's message.
  - A test whose fixture renders right and wrong alike is a plan failure: say so rather than ship it.
- **The fallout rule (Task 1).** A test outside the ones this plan names may go red because a non-administrator's document now carries file names.
  - Keep its assertion. Compare through a mapping to file names written in the test, or state the expected names as literals.
  - Never delete the assertion or loosen it.
  - List each in the report.
- **Literals.** A test states the file names, the refusal's words and every user-facing sentence written out. Never compute them through the code under test: a test's own mapping uses `std::path::Path::file_name`, not `with_file_names`.
- **Real interaction.** jsdom tests navigate outside `act`, with real events.
- **Evidence.**
  - Logs and each task's report (`$E/T<n>-report.md`) go to `$E`, written by Bash (`cat >| … <<'EOF'`). The Write tool is refused under `.superpowers/`. Never under `target/`.
  - A delete gets its own Bash call, naming each target by its full absolute path.
- **The counts.** `docs/status.md`'s test counts are Task 4's. Tasks 1–3 leave the totals stale; the gate's `status-test-count` step runs only in the Finish.

## Review Focus

The failure modes most likely to reach a person, most likely first:
1. **A caller put in the wrong class.**
   - An administrator's API key sent full paths, by a role-only check. Task 1's key test holds this; the guard test does not, since `session.rs` is not in its list.
   - An administrator's password sent file names, by a `By::Cookie`-only check.
2. **The tag computed over the stored document for a non-administrator.** Two accounts' copies would then revalidate each other, and a reader would be told its file-name copy is current against an administrator's tag. Held by Task 1's tag test.
3. **The notes-tag guess test passing for the wrong reason.** Task 1 watches it under the key mutation before and after its change.
4. **A burst test that does not reproduce in jsdom**, and is green at BASE. Task 2 must show it red at BASE.
5. **The guard checking a folder cargo-llvm-cov does not use, or deleting anything.** Task 3's case 4 checks that the folder survives byte for byte; the reviewer reads the resolution against cargo-llvm-cov's README.

---

### Task 1: A non-administrator is sent each member's file name (O280)

Survey §1 (the gate), §2 (`to_json`'s other callers), §4 (the reader reads only the basename) and §5 (the tests). Ruling 1.

**Step 0 (this task only):** copy this plan to `$W/docs/superpowers/plans/2026-11-13-o280-housekeeping.md`. It rides in this task's commit.

**Files:**
- `crates/taiyaku-serve/src/session.rs`:
  - The new `administers`, directly after `administering_or_forbidden` (~889-897).
  - One sentence in `administering_or_forbidden`'s doc, under "The rule, stated so a new route cannot miss it": a route that shapes what it sends by this question, rather than refusing, asks `administers`, which asks this gate (O280).
- `crates/taiyaku-serve/src/routes.rs`:
  - The new `with_file_names`, beside `filter_sidecar_editions` (~620).
  - `get_sidecar` (~646-678): its doc, and the inline comment over the tag. That comment's "`patch_alignments` compares `If-Match` with the same view's tag" becomes: only a caller `administers` admits reaches `patch_alignments`, and it is sent the view's own document, whose tag that route compares.
  - **Untouched:** `sidecar_etag`, `patch_alignments` (its `If-Match` ~1943 and its answer ~2111-2118), `filter_sidecar_editions` and every other handler.
- `crates/taiyaku-serve/src/routes/tests.rs`, `mod narrowing` (~7286):
  - `both_libraries_are_sent_raw_whole` (~7790).
  - `manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent` (~8281).
  - New tests after `a_narrowed_document_has_its_own_etag` (~7798).
  - The helpers to use: `document`, `fetched`, `editions`, `the_administrators_document` (~7442); `test_support::key_authenticating`, `FIXTURE_NAME`, `FIXTURE_PASSWORD`; the `basic` closure shape at ~4078.
- `docs/sidecar.md`, the `Edition` section's first bullet (~273). It becomes, exactly:
  `- \`path\` is relative to the sidecar. The server sends it as stored only to an administrator signed in by browser or password; anyone else, an administrator's API key included, is sent each member's file name in its place (O280).`

**Behaviour:**
- `get_sidecar`: after the view and the missing-work check:
  - If `administers(&request_extensions)` is true, it sends `view.sidecar()` as today.
  - Otherwise it sends `with_file_names(view.sidecar())`.
  - The `ETag` is `sidecar_etag` over the document sent, and the `If-None-Match` comparison and the body use that same document.
- `with_file_names`: every member of every edition gets its `path`'s last component (`Path::file_name`, then `to_str`), or `""` when there is none. Every other field is as given.

- [ ] **Step 1: The tests first.** Run them against BASE's `get_sidecar` and record which fail.
  1. **`a_readers_document_names_each_member_by_its_file_name`**, for `Reader::MangaOnly`, `KidsOnly` and `Both`.
     - Read each member's raw `path` string from the JSON, not through `members()`, which maps to file names already.
     - Expected per edition, as literals:
       - Manga alone: `raw` `["Foo v01 part1.cbz", "Foo v01 part3.cbz"]`, `en` `["Foo v01.cbz"]`, `fr` `["Foo v01.cbz"]`.
       - Kids alone: `raw` `["Foo v01 part2.cbz"]`.
       - Both: `raw` holds all three in order, plus `en` and `fr`.
     - The body holds no part of the fixture's directory. Take the directory from the administrator's document: two levels above `raw`'s first stored path. First assert that path is absolute, so the check is not vacuous.
     - Named mutations:
       - `administers` answering true for everyone: every case fails.
       - `with_file_names` mapping only `raw`'s members: the `en`/`fr` assertions fail.
  2. **`the_administrator_by_session_or_password_is_sent_the_stored_paths`.**
     - By session: `Reader::Administrator`.
     - By password: `crate::build_router(merged.state.clone())`, with `authorization: Basic` for `FIXTURE_NAME`/`FIXTURE_PASSWORD`.
     - Each of `raw`'s three paths is absolute and ends with `m/Foo v01 part1.cbz`, `k/Foo v01 part2.cbz` and `m/Foo v01 part3.cbz` in turn, and the two bodies are byte-equal.
     - Named mutations:
       - `administers` admitting `By::Cookie` only: the password half fails.
       - `administers` answering false for everyone: the session half fails.
  3. **`an_administrators_key_is_sent_file_names`.**
     - `test_support::key_authenticating(crate::build_router(merged.state.clone()))`, `GET /api/works/{slug}`: 200, and every member's `path` equals its file name.
     - The body is byte-equal to `Reader::Both`'s, which sees the whole work too.
     - Named mutation: `administers` asking only `principal.role == Role::Admin`, ignoring `By`. It fails, and nothing else in the suite would catch it.
  4. **`each_document_is_tagged_over_the_bytes_it_is_sent`**, for `Both`, the administrator by session, and the administrator's key.
     - The `ETag` equals the quoted SHA-256 hex of the body bytes, computed in the test with `sha2` (not with `sidecar_etag`).
     - Each reader's own tag answers 304.
     - `Both` sending the administrator's session tag gets 200, and the administrator sending `Both`'s tag gets 200.
     - Named mutation: the tag computed over `view.sidecar()` while the body is the file-name copy. The hash check fails, and the cross-revalidation answers 304.
  5. **`with_file_names_keeps_only_each_paths_last_part`**, a unit test with a `Sidecar` built by `Sidecar::from_json` from a literal.
     - Members `/srv/Manga/Foo/Foo v01.cbz`, `../../Manga/Foo v02.cbz`, `Foo v03.cbz`, `/` and `..` come out as `Foo v01.cbz`, `Foo v02.cbz`, `Foo v03.cbz`, `""` and `""`.
     - With every path blanked on both sides, the copy's `to_json` equals the input's: the title, hub, alignments, `sha256`s and pages are untouched.
     - Named mutations:
       - Falling back to the whole path: the `/` case fails.
       - Mapping only absolute paths: the `../../` case fails.
       - Dropping the alignments from the copy: the equality fails.
- [ ] **Step 2: Implement** `administers`, `with_file_names` and `get_sidecar`'s branch. Run the crate.
- [ ] **Step 3: The two tests that pin today's shape.**
  - **`both_libraries_are_sent_raw_whole`:** `Both`'s document equals the administrator's with each member's `path` mapped to its `Path::file_name`, the mapping written in the test.
    - Named mutation: `administers` answering true for everyone (the unmapped document is sent).
  - **`manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent`:** each guess hashes the sent member's **stored** path, looked up by `sha256` in the administrator's document (raw's members have distinct digests). Its doc comment says why: `View::etag_naming_numbering` hashes the stored path, and the guess must be the strongest an account could build.
    - The named mutation is the one the test exists for: delete `field(member.sha256.as_bytes())` from the hidden-member loop in `View::etag_naming_numbering`. Watch it twice and record both:
      - (a) with Step 2 in and the guess **unchanged**: the test stays green under the mutation. That is the hole.
      - (b) with the guess changed: the test fails under it.
  - `a_hidden_work_is_refused_before_its_validator_is_answered` is expected green unchanged (spec gap 1). If it is not, stop and report.
- [ ] **Step 4: Run the whole crate.** Apply the fallout rule to any other red test, and list each.
  - `the_administrators_document`, `a_narrowed_document_has_its_own_etag` and `auth::tests::get_sidecar_names_only_visible_editions` are expected green.
  - Then clippy and fmt.
- [ ] **Step 5: Watched mutations**, as named in Steps 1 and 3.
- [ ] **Step 6:** Comment guard, then commit `A non-administrator is sent each member's file name in the work document, tagged over what it is sent (O280)`. Write `$E/T1-report.md`. It lists the new tests, the fallout list (or "none") and each mutation's failing message.

---

### Task 2: New library's directory path keeps every keystroke of a burst (O281)

Survey §6a. Ruling 2. The evidence that it happens on BASE's ancestor 28b18b60:
- `.superpowers/sdd/deploy-evidence/lane-1/FU-B-main-burst.log`: the `sync-burst` rows on `path` drop a keystroke and log React #185.
- `FU-B-main-adddir-control.log`: Add directory keeps every one.
- The burst is `syncBurst` in `FU-B-repro.mjs`.

**Files:**
- `web/src/DirPicker.tsx`:
  - The ref and effect (`told`, ~91-97) go.
  - `onPath?.(next)` is called beside each `setPath`:
    - in the box's `onChange` (~224-229);
    - in `go`'s answer (~178-181);
    - on a taken pick (~122-126), through the ref of the box's current text (settled call 5).
  - The module doc's "`onPath` reports what the box holds a render after it changes …" (~8-12) is rewritten. It says `onPath` is told in the same handler that changes the box, never from an effect: a caller that sets state from it would otherwise commit twice per keystroke, and a burst of input runs past React's nested-update limit (O281).
  - The prop doc (~63-68) gains: not on mount.
- `web/src/NewLibrary.tsx`: the comment in `submit` (~113-117), whose reason ("the picker reports a keystroke a render after it draws it") stops being true.
  - Reading the box on submit stays.
  - Rewrite the comment to a reason that holds: what is sent is the text the person sees in the box. Keep the existing "without one box of that name nothing is sent" half.
- `web/tests/DirPicker.test.tsx`: "tells onPath each text the box holds…" (~229-245).
- The burst test:
  - First choice: jsdom, in `web/tests/Libraries.test.tsx`, `describe('Libraries, making one')` (~1724), on the page `at(NEW_LIBRARY)` mounts.
  - Fallback: the browser test file that already mounts New library (grep `web/tests/*.browser.test.tsx` for `New library`).

- [ ] **Step 1: The burst test, watched RED at BASE before any source change.**
  - Use `globalThis.IS_REACT_ACT_ENVIRONMENT = false` around the burst and restore it in `finally`, as `web/tests/Licence.test.tsx` (~676-726) does.
  - Find New library's path box (`input[name="path"]`) and focus it.
  - In one synchronous loop of 120 steps (no `await`, no `act`), set its value through the native `HTMLInputElement.prototype` value setter to one more character each step, and dispatch a bubbling `input` event.
  - Then let a macrotask pass.
  - Collect errors from a `window` `error` listener and a `console.error` spy, both installed before the loop.
  - Assert:
    - the box holds all 120 characters exactly (a literal string built in the test);
    - no error was recorded.
  - Run the jsdom project at BASE and record the failure: a short value, a #185 or "Maximum update depth" error, or both.
  - **If jsdom stays green at BASE**, move the test to the browser fallback, run the whole browser project at BASE, and record that failure instead. A burst test green at BASE is not shipped.
- [ ] **Step 2: Tighten the DirPicker unit test** before the fix. It fails at BASE on its first assertion.
  - On mount, `onPath` has not been called.
  - After `typeInto(box, '/srv/typed')`, `onPath.mock.calls` is exactly `[['/srv/typed']]`.
  - Browsing to an entry tells it `'/media'`, and a taken pick tells it `''`, as today.
  - Add a case: a pick is pending (a `deferred` from `./support/async`), and `/srv/next` is typed while it waits. When the pick resolves, the box keeps `/srv/next`, and `onPath` is not told `''` after it.
- [ ] **Step 3: The fix**, as Files says. Run the jsdom project, then the node and browser projects whole: `DirPicker` is drawn by several browser tests. Then `tsc -b` and biome.
- [ ] **Step 4: Watched mutations.**
  - Put the effect back (`useEffect(() => { told.current?.(path) }, [path])`), with the direct calls removed: the burst test fails as it did at BASE.
  - Report `''` on every taken pick: the pending-typing case fails.
  - Drop the call in `go`'s answer: the browsed-to assertion fails.
  - Add a mount report (`useEffect(() => onPath?.(''), [])`): the mount assertion fails.
- [ ] **Step 5:** Comment guard, then commit `New library's directory path keeps every keystroke of a fast burst: the picker tells its caller in the event, not from an effect (O281)`. Write `$E/T2-report.md`, naming the project the burst test landed in and its BASE failure.

---

### Task 3: The coverage gate refuses a build another rustc made

Ruling 4. Settled call 4 and the table and words above.

Where the script measures: `scripts/check-all.sh`, the `test` job block (~174-250). The block runs `cargo llvm-cov nextest …` (~225) when both `cargo-llvm-cov` and `cargo-nextest` are on `PATH`, and plain `cargo test` otherwise. No flag or variable there names a target directory.

**Files:**
- `scripts/coverage-build-guard.sh`, new, mode 100755: the table above, and the refusal's three lines as written.
  - It resolves `<dir>` from the top of the repository (`git rev-parse --show-toplevel`), as `check-all.sh` does.
  - It runs `${RUSTC:-rustc} -V` there, so `rust-toolchain.toml`'s pin applies as it does to cargo.
  - It contains no `rm`, `find -delete` or other delete.
- `scripts/check-all.sh`: one guarded call, after the hidden-Rust-sources check and before `# --- the lint job ---`:
  `if command -v cargo-llvm-cov >/dev/null 2>&1 && command -v cargo-nextest >/dev/null 2>&1; then run scripts/coverage-build-guard.sh; fi`.
  - Its comment says why it is early (a stale build is refused in seconds, not after the build) and that it is local only (CI starts from an empty target directory).
  - The six coverage lines that `.forgejo/workflows/ci.yml` mirrors are not touched.
- `docs/ci.md`, "Why the Rust floor is 91, and the toolchain is pinned". After "Re-set the floor only if the counting changed, as it did here." add, exactly:
  `\`scripts/check-all.sh\` writes the rustc that builds \`target/llvm-cov-target\` beside it, in \`target/llvm-cov-target.rustc\`, and refuses to measure over a folder another rustc built, naming the folder to delete and both versions; it deletes nothing itself.`

- [ ] **Step 1: Write the script and the call.**
- [ ] **Step 2: The watched runs.** Each case uses a fresh directory under `P=$E/guard`, and sets `CARGO_LLVM_COV_TARGET_DIR` to an absolute path inside it. Nothing is deleted between runs. Call the script as `builtin cd $W && CARGO_LLVM_COV_TARGET_DIR=<dir> scripts/coverage-build-guard.sh`, recording stdout, stderr and the exit status for each.
  1. `$P/c1/llvm-cov-target`, with `$P/c1` absent: exit 0. `$P/c1/llvm-cov-target.rustc` exists and holds exactly `builtin cd $W && rustc -V`'s line.
  2. The folder made (`mkdir -p`), no record: exit 0, and the record is written.
  3. The folder and a record equal to `rustc -V`'s line: exit 0, and the record is unchanged.
  4. The folder holding one file `inside`, and a record `rustc 1.99.1 (planted 2026-01-01)`:
     - exit 1, and stderr is exactly the three lines, with this `<dir>`, the planted line and the current one;
     - `sha256sum` of the record and of `inside`, and `ls -la` of the folder, are the same before and after.
  5. No folder, and a record `rustc 1.99.1 (planted 2026-01-01)`: exit 0, and the record now holds the current line.
  6. **The real gate, refusing:** `builtin cd $W && CARGO_LLVM_COV_TARGET_DIR=$P/c4/llvm-cov-target scripts/check-all.sh >| $E/T3-gate-refuses.log 2>&1; echo "exit $?" >| $E/T3-gate-refuses.exit`.
     - Expect `exit 1`.
     - The log holds `==> scripts/coverage-build-guard.sh` and the three lines, and no `==> cargo fmt` line.
     - Case 4's files are unchanged.
- [ ] **Step 3: Watched mutations**, each against fresh case directories:
  - Drop the comparison, so it never refuses: case 4 exits 0.
  - Drop the folder test, so it refuses whenever the record differs: case 5 exits 1.
  - Never rewrite an existing record: case 5's record stays planted.
  - Drop the `mkdir` of the record's directory: case 1 fails with the shell's error.
- [ ] **Step 4:** `bash -n` the script. Then `node $W/scripts/check-comments.mjs --base <start head>`: the `scripts` share may move (Global Constraints). `git -C $W ls-files -s scripts/coverage-build-guard.sh` shows `100755` once added.
- [ ] **Step 5:** Commit `The local gate refuses to measure coverage over a build another rustc made, and names the folder to delete`. Write `$E/T3-report.md` with each case's output.
  - Leave `$P` in place as evidence. The controller deletes it after reading, in a call of its own.

---

### Task 4: The closes, the new row and the freshness sentence in `docs/status.md`

Read the Open items section, row O280 (~1903), the O280 notes bullet (~4726), the lane 3 paragraph above it (~4690), and row O267 (~1890) for the closed-row shape.

- [ ] **Step 1: The freshness sentence**, in the `taiyaku-serve` row (~392).
  - First re-read each route's `Cache-Control` at `$W` HEAD, and record each in the report with its symbol:
    - `serve_page` (`PAGE_CACHE_CONTROL`, `COVER_CACHE_CONTROL`, and its `not_modified`);
    - `get_thumb`'s tile;
    - `opds.rs`'s `stream_page` and `cover`;
    - `download.rs`'s `private, no-cache`;
    - `per_reader_by_default` and where it is layered;
    - the three `not_modified(&tag, None)` calls.
  - Then replace from "A caching split runs the length of the file:" through "(O1)" with the sentence below. The original has no full stop after "(O1)" before the next bold sentence; this one ends with one.

    A caching split runs the length of the file: the four routes whose bytes are fixed by a member's `sha256` (the page, the tile, and OPDS's page and cover) are kept and revalidated, `private, no-cache` (`PAGE_CACHE_CONTROL`), with no freshness lifetime — the two page routes with an `ETag` as well, and the tile and OPDS's cover with none, since no validator is defined for either (`get_thumb`'s doc says what a tile's would have to name) — save a library cover, page 0 at a cover size on the tile's route, which `serve_page` answers as it answers a page, with an `ETag`, and as `private, max-age=31536000, immutable` (`COVER_CACHE_CONTROL`) when its `?v=` names its bytes; a download is `private, no-cache`; and every route answering a document this server rewrites sets none, so `per_reader_by_default` sends it `private, no-store`, on the `304` as well as the `200`, which is the half that is easy to get wrong (O1).

  - If the re-read disagrees with any clause, stop and report; do not reword around it.
- [ ] **Step 2: Close O280, and open and close O281.**
  1. **O280's row:** `[x]`, and text saying what was done, with symbols and no line numbers:
     - A caller `session::administers` admits (an administrator by browser session or password, the gate's own meaning, O93) is sent the view's document as it is.
     - Anyone else, an administrator's API key included, is sent `routes::with_file_names`' copy, with the `ETag` over the copy. So an administrator's and a reader's copies never revalidate each other.
     - `Sidecar::to_json`, `View`, the stored work and the correction path are unchanged.
     - The reader reads only a path's last part (`chapterCode`), so neither the web nor the wasm changed.
     - The owner's rulings of 2026-11-13.
  2. **A new row O281 after O280's**, `[x]`:
     - **Title:** a fast burst of input into New library's directory path dropped a keystroke and threw React's error #185 (maximum update depth).
     - **Cause:** `DirPicker` told its caller each change from an effect, a commit after the change. New library sets state from that report, so each keystroke committed twice, and a burst of input events in one task ran past React's nested-update limit.
     - **Fix:** `onPath` is now called in the same handler as each change (typed, browsed to, emptied by a taken pick), and not on mount. Add directory passes no `onPath` and was not affected.
     - Symbols: `DirPicker`, `NewLibrary`. Found by: go-public lane 1's drive, 2026-11-12.
     - No figure from the FU-B logs goes in the row.
  3. **The notes.**
     - O280's bullet (~4726) becomes `… Closed.` and says the same in prose.
     - After it, a paragraph headed like the lane paragraphs (e.g. "**O280 and housekeeping, 2026-11-13**") says what this PR closed and opened.
     - Then O281's bullet, shaped like O280's.
  4. **The counts:**
     - The heading "Two hundred and eighty," becomes "Two hundred and eighty-one,".
     - "One hundred and eighty-four are now closed — … O277, O278 and O279 — leaving ninety-six." becomes "One hundred and eighty-six … O277, O278, O279, O280 and O281 — leaving ninety-five."
     - "What remains: the ninety-six open items" becomes "ninety-five". The count word can wrap across a line break: grep the opener and read the next line.
- [ ] **Step 3: Test counts.** Measure them; do not add.
  - Rust:
    - Run `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml --workspace --all-features --test-threads=8 >| $E/T4-nextest.log 2>&1`.
    - Then `node $W/scripts/status-test-count.mjs $W/target/nextest/default/junit.xml $W/docs/status.md`.
    - Set "Six crates, N tests" and "of the N are standing checks" to what it reports, and run it again to see it pass.
  - Web:
    - Run the node and jsdom projects, and set "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one" from their summary lines.
    - If Task 2's burst test landed in the browser project, set "one thousand and eighty-one more in a **browser** project" from a whole browser run too; otherwise say it did not move.
- [ ] **Step 4: Sweep.**
  - Check the sentences against `grep -a -c '^| O[0-9]' docs/status.md` (expect 281), `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` (expect 186), and `grep -a -n -A1 '^What remains' docs/status.md`.
  - Grep `O280` and `O281` across `docs/`, `web/src/`, `crates/` and `README.md`. Read every hit, and correct prose that speaks of O280 as open. O267's bullet ("… are O280.") is history and stays.
  - Grep `a day's freshness` in `docs/status.md` (no hit left at ~392), and `render after` in `web/src` (no hit left in `DirPicker.tsx`).
- [ ] **Step 5:** Commit `Close O280 and O281 in status.md, and correct the taiyaku-serve row's caching sentence`. Write `$E/T4-report.md` with the re-read headers and the measured counts.

---

### Finish

1. **Whole-branch review** of `4b9d1d01..HEAD`, against the rulings, the settled calls, the spec gaps and Review Focus, in two lanes:
   - (a) the server: Task 1, with Review Focus 1–3. The reviewer reads `administers` against `administering_or_forbidden`, and the tag path in `get_sidecar`.
   - (b) the web, the guard and the docs: Tasks 2–4, with Review Focus 4–5. The reviewer re-reads each clause of the freshness sentence against the code.

   One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **Gate** on the final commit, in the fresh worktree, at load < 24. Read the `.exit` file.
   - The gate includes the browser project and the coverage floors.
   - The first gate in this worktree is the guard's case 1. After it, `$W/target/llvm-cov-target.rustc` holds `builtin cd $W && rustc -V`'s line: quote both.
   - A gate after the fix wave is case 3, and proceeds.
3. **The drive.** Run by a subagent; the controller starts the server, or tells the agent never to restart it.

   **Setup:** `S=$E/drive`.
   - Check `cat /proc/loadavg`, and that `ss -lptn 'sport = :4405'` is empty. If it is not, take another free port and say so.
   - Write `sha256sum /home/davey/temp/manga-test/library.db` to `$E/drive-before.sha256`, `stat` it, and `touch $S/marker`.
   - Copy, never hard-link: `cp /home/davey/temp/manga-test/library.db $S/library.db`. Never open the original.
   - Build:
     - `cargo build --release --manifest-path $W/Cargo.toml -p taiyaku-cli`, then copy `$W/target/release/taiyaku` to `$S/taiyaku`;
     - `node $W/web/node_modules/vite/bin/vite.js build $W/web --outDir $S/dist --emptyOutDir`.
   - Accounts on the copy:
     - `printf 'pw\npw\n' | $S/taiyaku library user password local --db $S/library.db`;
     - `$S/taiyaku library user add reader --role user --db $S/library.db`, then a password `pw` for `reader` the same way;
     - `library user every-library reader`;
     - `library key create local --name drive`: keep the printed key in `$S/local.key`, never in a report.
   - **The scratch tree** is `$S/tree`, never under the test library. Copy (`cp`, not `ln`) two distinct small `.cbz` files from `/home/davey/temp/manga-test/Volumes` to `$S/tree/m/Foo v01 part1.cbz` and `$S/tree/m/Foo v01 part2.cbz`.
   - Serve: `$S/taiyaku serve --db $S/library.db --cache $S/cache --web $S/dist --addr 127.0.0.1:4405 --insecure-cookies >| $E/drive-server.log 2>&1`, with `run_in_background`. Never restart it detached; stop it by PID.

   **The script** is `$E/drive.mjs`, modelled on `.superpowers/sdd/deploy-evidence/lane-1/FU-B-repro.mjs`.
   - Run it with `node`. It imports `chromium` from `$W/web/node_modules/playwright/index.mjs` by absolute path; it does not use the chrome-devtools MCP.
   - It records each phase into `$E/drive.json`: `pageerror`s, console errors and statuses. It saves screenshots to `$E/drive-<phase>.png`.
   - Phases:
     1. **As `local` in a browser context:**
        - Add a library `Scratch` over `$S/tree/m` through New library, the "Don't scan yet" box unticked, and wait for its scan.
        - Assert the fixture first: one work whose edition holds `Foo v01 part1.cbz` and `Foo v01 part2.cbz` in that order. If the derive does not put them in one edition, stop and report: the readout check needs a multi-member edition.
     2. **The burst, as `local`:** open New library again.
        - Run `syncBurst`'s 120 `input` events on `input[name="path"]`: the box holds all 120 characters, and no `pageerror` or console error names React.
        - Then clear the box and `page.keyboard.type` a 60-character path with no delay: every character is kept.
        - Leave without making a library.
        - `FU-B-main-burst.log` is the BASE-side control for the same burst.
     3. **The documents.** In-page `fetch('/api/works/<slug>')` for the work of phase 1:
        - As `local` by session: each member's `path` begins with `$S/tree/m/`. Keep its `ETag`.
        - In a second context, signed in as `reader`: each `path` is exactly its file name, and the body holds no `$S`.
        - `reader`'s own tag in `If-None-Match` answers 304, and `local`'s tag answers 200.
     4. **By `curl`:**
        - `local`'s key over Basic: file names.
        - `local`'s password over Basic: full paths.
        - `reader`'s password: file names.
     5. **The readout.** As `reader` and then as `local`, open `/read/<slug>` on a page of part2. Switch the chapter readout on in the reader's readout row (`Chrome.tsx`, `readoutRow`) if it is off.
        - The readout reads `Foo v01 part2 · 2 of 2` for both, possibly with the leading `~` that `chapterReadout` adds for a fragile hub. The two strings are equal.
        - Screenshot both at 1280 and 390 px, and look at them.

   **Stop and prove:**
   - Stop the server by PID, and check `ss -lptn 'sport = :4405'` is empty again.
   - `find /home/davey/temp/manga-test -newer $S/marker` prints nothing, and the `library.db` sha256 equals `$E/drive-before.sha256`.
   - After the controller has read the reports, delete `$S` and `$E/guard` by their absolute paths, in a call of their own.
4. **Claim check** of the closes, the new row, the freshness sentence, the `docs/ci.md` and `docs/sidecar.md` sentences, and the PR body.
   - Put the PR body in pass 1.
   - Scope later passes to the correction commit plus the PR body.
   - Grep each corrected sentence's shape everywhere.
5. **The PR body** at `$E/pr-body.md`, never hard-wrapped. The title is the first line, under 255 characters: measure it.
   - It names O280 and O281, the four rulings, the settled calls and spec gaps 1–4. It names gap 3 as a follow-up the owner may want.
   - It tells lane 4 which shared functions changed: `get_sidecar` and the new `with_file_names` in `routes.rs`; the tests in `mod narrowing` named in Global Constraints; the status rows and counts.

   Ask the owner before pushing; the forge is Gitea, through its MCP.

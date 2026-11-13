# Go-public lane 3: second accounts (O58, O241, O267, O184, O248) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the server safe for a second account before one exists.
- O58 (closing O241): only an administrator signed in through a browser writes the shared truth and alignment documents. Keys may still read them.
- O267 and its path sweep: no answer a non-administrator can be sent names a server path.
- O184: a write never brings a deleted work back.
- O248: Move and Remove from this library name every account that gains or loses sight of the directory or of a directory inside it.

**Architecture:**
- Tasks 1–2: `put_truth`, then `patch_alignments`, open with `session::administering_or_forbidden`. Each task brings its own test fallout and route-table changes.
- Task 3: the web offers correcting and labelling only to an administrator.
- Tasks 4–5: one helper answers a path-free sentence and logs the full error to stderr. The page, thumbnail, OPDS, download and store-failure answers go through it.
- Task 6: O184, as the pre-deploy plan's Part 2 Task 4 wrote it.
- Task 7: O248, in `taiyaku-library` and in the dialogs' words.
- Task 8: the closes. Then the Finish.

No migration.

**Tech Stack:** Rust (`taiyaku-serve`, `taiyaku-library`), TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`.

**Spec:** `docs/superpowers/specs/2026-11-12-go-public-design.md`, cited as "spec §n". This plan implements §5 (lane 3) and follows §7 (overlaps, merge order) and §9 "Lane 3". The code was surveyed at main ec4eb11c in `.superpowers/sdd/deploy-evidence/survey-accounts.md`. O184's task is `docs/superpowers/plans/2026-11-10-pre-deploy-missing-works.md`, "Part 2, Task 4". Its line numbers are stale; this plan gives them again at ec4eb11c. Locations are by symbol. Line numbers are hints taken at ec4eb11c.

**Rulings** (binding; do not re-open):
- **The owner, 2026-11-12:**
  - O58: the shared truth and alignment documents are writable only by an administrator signed in through the browser; keys may read them. It closes O241 by consequence.
- **The builder's calls, told to the owner the same day:**
  - O267 is done with the path sweep, and `atomic::write` keeps naming its path (closed O29).
  - O248 asks about the moved directory and every directory nested in it.
  - O184 is built as Part 2 Task 4, after O58.
- **The spec's own calls:**
  - `patch_alignments`' 500 keeps its path, as an administrator's route (decision 2).
  - The gate is `administering_or_forbidden` (decision 5).
  - Decision 8's write halves and `truth::as_stored` stay, unreachable (decision 6).
  - The web offers correcting and labelling from `/api/me`'s `admin`, and a path-free answer logs the full error to stderr (decision 7).
- **Not reversed** (spec §2's anti-goals):
  - O29 and O43.
  - O86 and O93.
  - Overlapping-libraries decisions 8 and 11.
  - Auth spec §7.

**What the plan settles** (the plan writer's calls; the controller tells the owner):
1. **Where the gate sits.**
   - Each handler opens with the gate, before its body is parsed. So a key's malformed or unconditional write is refused for the credential, and `exactly_the_declared_routes_refuse_an_api_key`, which sends `{}` with no `If-Match`, classifies it.
   - The `Principal` is read from the `axum::http::Extensions` each handler already takes. No extractor is added, so the direct callers in `routes/tests.rs` keep their argument lists.
   - An absent principal answers `NoPrincipal`'s 500, `routes::reader`'s rule.
   - A password credential passes the gate, as on every administrative route.
2. **Test fixtures.**
   - `routes::tests::as_the_fixture_account()` gains `crate::auth::By::Cookie`. It already builds a `Role::Admin` principal, and the gate refuses a principal with no `By`.
   - The suite's default router is already the administrator's browser session (`test_support::authenticating`), so most write tests pass unchanged.
3. **The fallout rule, for every test that goes red when a gate lands.** "Limited writer" means a `user` account, a key, or a direct call with a principal that is not an administrator's browser session.
   - (a) **The write is incidental to the test's subject** (stalls, `ETag`s, bridging, sidecars): send it as the administrator's browser session.
   - (b) **The subject is what a limited writer's write does after the gate** (decision 8's refusals, O191's kept entries, the per-side correction refusal, the response filter, a hidden work refused before the body): that proof is **void**, as `every_route_declares_its_grant_posture`'s count note says O86 voided I8's proofs.
     - Where the request still says something, keep it and expect the gate's 403, keeping the test's "the store is unchanged" check.
     - Where nothing after the gate is left to prove, delete the test and any helper only it used.
     - Never hand-build an administrator holding a grant: no account can be one (`Principal::from_account`).
   - Every red test is classified by this rule and listed in the task report. The hints in Tasks 1–2 are a starting list, not the list.
4. **The route tables** (spec §5 says "read again"):
   - `/api/works/{work}/truth` stays `RefusesAndFilters` in `grant_postures`, because its `GET` refuses and filters. Its narrowing posture becomes `Narrows`, because a narrowed account only reads it.
   - `/api/works/{work}/alignments` becomes `GrantIndependent`, worded like the four metadata routes' entries. Its narrowing posture becomes `NoPages` with the `ADMIN` reason.
   - `NarrowingPosture::Refuses` loses its only route, so the variant, its match arm and its count go.
5. **Correcting in the reader.**
   - For a non-administrator, `keyPlan` answers `c` with `undefined`, so the key is not the reader's, and Help leaves out its row.
   - `beginCorrection` is the only way to reach `CORRECTION_PORTS`, so the constant stays as it is. Spec §5's "wired for an administrator only" is met at that one entry.
   - This follows `Series.tsx`'s `mayEdit = useGate()?.who.admin` precedent. The server still decides, which keeps `Config.tsx`'s O93 argument intact.
6. **The labeller.**
   - `label.tsx` renders a new `LabelPage`. It asks `me()` once and draws the `Labeller` for an administrator, the sentence for anyone else, and the error's text if `me()` fails.
   - No truth port is called before `me()` answers.
7. **O267.**
   - Each path-free answer keeps its status: the 404s stay 404s, as `ServeState::page`'s doc argues.
   - Sentences name the work, the edition, and the page or member. A page is numbered as the account asked for it.
   - `get_thumb` and `opds::cover` take `EditionView::locate` (and the cover's `page_to_full(0)`) out of their blocking closures, as `serve_page` already does. A locate refusal then keeps its own words, and only a read or decode failure becomes the path-free answer.
   - The sweep's finds are fixed too: OPDS's page and cover, the generated download's `member_page_entries` error, `EditionView::locate`'s missing-edition message, and the `Failed` arms of `NoProgress` and `NoNotes`. Those arms can carry `Library::working`'s "opening a second connection to <store path>".
   - `EditionView::locate`'s missing-edition branch gets no test. No request reaches it: the view was built with that key. O247's plan made the same call for an arm with no seam.
8. **O184.**
   - As Part 2 Task 4.
   - It adds a live registry over a document-backed work, which the task's "row resolved after the file write" mutation needs. `router_over_a_live_library_with_an_edge`'s work is scanned and has no file, so that mutation would pass against it.
9. **O248.**
   - The directory set is the directory and every `root` row that `reach::nested_in` places strictly inside it.
   - It is read once, before the write: a move or an unlink changes which library holds a directory, never its path.
   - Each limited account's sight is a set, compared before and after the write.
   - An unlink keeps `loses_sight` alone. Removing a link only shrinks an account's access: a library the unlink empties takes its grants with it, and O220 gives that account nothing in their place.
10. **No task adds SQL beyond a plain `SELECT`, so no `rust:1-bookworm` run.** A task that finds it needs one stops and reports.

**Spec gaps** (not resolved silently; the controller tells the owner):
1. **§9 and decision 2 disagree on `taiyaku.json`.**
   - §9's O267 test lists "an unwritable `taiyaku.json`" among the answers that must carry no part of the scratch path.
   - Decision 2 and §5's "Where", both corrected at 0091a560, keep `patch_alignments`' 500 path as an administrator's route.
   - Truth is stored in rows alone (`truth::store`), so that 500 is the only `taiyaku.json` write a request reaches.
   - This plan follows decision 2. No test strips that path. O58's tests show a non-administrator never reaches it.
2. **O267's rule is wider than its sweep.**
   - The rule: "a body a non-administrator can be sent never carries a server path".
   - The work document breaks it. `get_sidecar` sends `Sidecar::to_json`, every member's stored `path` included, and a scanned work's member paths are absolute. So every account is sent the server's paths for every work it can open.
   - It is not a `.display()` or a source's error, so it is outside the sweep §5 describes, and stripping it would change the document's format.
   - Unless the owner rules otherwise, Task 8 opens a row for it, and the PR names it.
3. **The spec does not say what happens to the tests of the code decision 6 keeps.** Settled call 3 voids the tests that drove a limited writer or a key through that code, on O86's precedent. The code then has no test until a capability reaches it again. The alternative is to keep those tests over a hand-built administrator holding a grant, a principal no account can be.

**New interfaces this plan names** (the plan is where they are first written):
- **Path-free answers** (`crates/taiyaku-serve/src/routes.rs`, beside `not_found`):
  - `pub(crate) fn answered_and_logged(status: StatusCode, said: String, why: &str) -> Response`. It writes `log_line`'s line to stderr, the `writeln!(std::io::stderr(), …)` way `ServeState::find` does, never `eprintln!`. It answers `status` with the body `"{said}; the server's log says why"`.
  - `fn log_line(out: &mut impl std::io::Write, said: &str, why: &str)`. It writes `"taiyaku: {said}: {why}\n"` and ignores a write error.
  - `pub(crate) fn unreadable(what: &str, why: &str) -> Response`, which is `answered_and_logged(StatusCode::NOT_FOUND, format!("{what} could not be read"), why)`.
- **O184** (`crates/taiyaku-serve/src/works.rs`), as Part 2 Task 4 names them:
  - `pub(crate) enum Mint { Bridge, Never }`, and `pub(crate) fn Mint::of(works: &crate::Works) -> Mint`, which is `Never` for a live registry.
  - `pub(crate) enum NoRow { Gone(String), Failed(String) }`. `Gone` carries `` no work `{slug}` ``.
  - `row_id_for_write(conn, work, mint: Mint)` and `row_id_for_document(conn, slug, sidecar, path, mint: Mint)` answer `Result<i64, NoRow>`.
  - `progress::store`, `notes::store` and `truth::store` take a final `mint: Mint` and answer `Result<(), NoRow>`. `notes` is a `pub mod`: if `private_interfaces` fires on `notes::store`, narrow that function to `pub(crate)`, since nothing outside the crate calls it.
  - `alignments::store(conn: &Connection, work_id: i64, doc: &Sidecar) -> Result<(), String>`.
  - `test_support::router_over_a_live_library_with_a_document() -> (axum::Router, Arc<ServeState>, Fixture, String)`: a live registry over one imported work whose `taiyaku.json` is on disk, with a solved edge. It answers the slug.
- **Correcting and labelling** (web):
  - `keyplan.ts`: `Guards` gains `mayCorrect: boolean`.
  - `keys.ts`: `export function withoutCorrecting(bindings: ReaderBinding[]): ReaderBinding[]`. It drops the binding whose `resolve(binding.keys[0], [])` answers `{ kind: 'correct' }`, so the binding is found by its intent, not its letter.
  - `Help.tsx`: `HelpProps` gains `mayCorrect: boolean`.
  - `Labeller.tsx`: `export interface LabelPageProps { ports: TruthPorts; who: () => Promise<Me> }` and `export function LabelPage({ ports, who }: LabelPageProps)`.
- **O248** (`crates/taiyaku-library/src/relocate.rs`): `fn sighted(conn: &Connection, roots: &[i64]) -> Result<BTreeMap<String, BTreeSet<i64>>, String>`. It covers every limited account, as `sighted` filters them today, keyed by name, with the subset of `roots` its access allows. The set may be empty.

**Sentences** (literal; tests assert them written out):
- Unchanged and reused: the gate's `this route is an administrator's`, and a key's refusal, which contains `API key`.
- **The labeller, for a non-administrator:** `Only an administrator can label pages: the labels are shared by every account on this server.`
- **O267 bodies.** `` `{edition}` `` is the edition key, `{slug}` the work's slug, `{n}` the page as the account asked for it, and `{ordinal}` the member:
  - a page or a thumbnail, through `/api` or OPDS: `` page {n} of `{edition}` of `{slug}` could not be read; the server's log says why ``
  - the OPDS cover: `` the cover of `{edition}` of `{slug}` could not be read; the server's log says why ``
  - an edition download: `` `{edition}` of `{slug}` could not be read; the server's log says why ``
  - a member download: `` member {ordinal} of `{edition}` of `{slug}` could not be read; the server's log says why ``
  - a progress or notes store failure (500): `the library could not answer this request; the server's log says why`
- **O248.** Each "sight of it" becomes "sight of it or a directory inside it":
  - Move, asked: `Moving this directory from {from} to {library}: {gained} will gain sight of it or a directory inside it, and {lost} will lose sight of it or a directory inside it.` With both lists empty: `Moving this directory from {from} to {library}: no account will gain or lose sight of it or a directory inside it.`
  - Move, done (the kept report): `{Gained} gained sight of it or a directory inside it, and {lost} lost sight of it or a directory inside it.` and `No account gained or lost sight of it or a directory inside it.`
  - Unlink: `Removing this directory from {from}: {names} will lose sight of it or a directory inside it.`

**Functions this lane touches**, so a rebase over lane 2 (`session.rs`, `auth.rs`, `auth/tests.rs`) or under lane 4 (the notes and progress writers) can be checked:
- **`taiyaku-serve`, `routes.rs`:**
  - handlers: `put_truth`, `patch_alignments`, `put_notes` and `put_progress` (O184's store calls only), `serve_page`, `get_thumb`;
  - error answers: `impl IntoResponse for NoProgress` and `for NoNotes`;
  - the new helpers;
  - the test tables: `grant_postures`, `narrowing_postures`, `GrantPosture`'s doc, `NarrowingPosture`.
- **`taiyaku-serve`, other modules:**
  - `session.rs`: `administering_or_forbidden`'s doc comment only. `auth.rs` is not touched.
  - `download.rs`: `prepare`, `prepare_member`.
  - `opds.rs`: `stream_page`, `cover`.
  - `view.rs`: `EditionView::locate`.
  - `works.rs`: `row_id_for_write`, `row_id_for_document`.
  - `progress.rs`, `notes.rs`, `truth.rs`: `store`. `alignments.rs`: `store` and the module doc.
  - `lib.rs`: the docs of `ServeState::library` and `ServeState::page`.
  - `test_support.rs`: the new fixture.
- **`taiyaku-serve` tests:**
  - `auth/tests.rs`: `REFUSE_A_DEVICE_CREDENTIAL`, `every_route_declares_its_grant_posture`, `every_route_declares_what_it_does_with_a_narrowed_edition`, and the four tests Task 2 removes;
  - `routes/tests.rs`, `download/tests.rs`, `opds/tests.rs`, `works/tests.rs`.
- **`taiyaku-library`:**
  - `relocate.rs`: `sighted`, `leaving`, the docs of `Moved` and `Leaving`.
  - `scan.rs`: `Unlinked`'s doc only.
- **Web:**
  - `App.tsx`: the keydown handler's `Guards` object, and the `<Help>` element.
  - `keyplan.ts`: `Guards`, and `keyPlan`'s `correct` branch.
  - `keys.ts`, `Help.tsx`, `Labeller.tsx`, `label.tsx`.
  - `MoveCounts.tsx`: `sightOf`. `UnlinkRoot.tsx`: the counts paragraph.
- **Docs:**
  - `docs/status.md`;
  - the overlapping-libraries design's §3.5;
  - the pre-deploy design's decision 5 and the pre-deploy missing-works plan's Part 2 Task 4 (marked done here).

## Global Constraints

- **Lanes.** Lanes 1, 2 and 3 run at once, each in its own git worktree on its own branch, cut from main once the spec PR merges. Call that commit BASE; ec4eb11c is provisional.
  - `W=/home/davey/projects/taiyaku/.claude/worktrees/go-public-3`.
  - `E=/home/davey/projects/taiyaku/.superpowers/sdd/deploy-evidence/lane-3`, in the MAIN checkout, so reports survive the worktree.
  - Every command runs against `$W` by absolute path. Never `cd`: it is zoxide here. Where a tool must run from a directory, write `builtin cd <dir> && …` in that one call.
  - Merge order is 1, 2, 3, 4. Lane 4 rebases onto this lane.
- **The worktree.** Before Task 1 the controller makes `$W/web/node_modules` a real copy, never a symlink, and copies `crates/taiyaku-wasm/pkg` in.
  - Never `pnpm exec`. Call `./node_modules/.bin/<tool>` directly.
  - Never set `CARGO_TARGET_DIR`. `$W` builds into `$W/target`.
- **Commits.** One commit per task, its subject naming the O-number(s). Never `git commit --amend`. Before reporting, check with `git -C $W log --oneline BASE..HEAD` that each task's commit is an ancestor of HEAD.
- **Comments say why**: the reason, the invariant, what breaks if it changes. Never history or measurements; a comment may point at an O-number. `scripts/check-comments.mjs` and `scripts/comment-budget.json` hold this in the gate. If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **The gate.** Run the repo's own `scripts/check-all.sh`, never a copy. It runs from the worktree root (it `cd`s to `git rev-parse --show-toplevel`), so the call is `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`.
  - Wait for load < 24 (`cat /proc/loadavg`) and record it.
  - Read the `.exit` file, not the harness's exit code, and quote the `all green` line.
  - Never pipe the gate through `tail`.
- **Rust, per task:**
  - `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/T<n>-nextest-<crate>.log 2>&1`. Read the summary lines from the log.
  - Then `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`.
  - Whole crates, never a named test.
- **Web, per task:**
  - `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom> --reporter=default >| $E/T<n>-<project>.log 2>&1`.
  - Then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`, the same way.
  - Whole projects, never a named file.
  - A task touching a `*.browser.test.tsx` file or any CSS runs the whole browser project. CI never runs it.
  - A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing. Read `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at BASE, not at another commit of this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one:
  - Put the named defect back by hand and watch the test fail.
  - Restore, and check with `git -C $W diff` that the restore is complete.
  - Record in the report the defect, the failing test and the assertion's message.
  - A test whose fixture renders right and wrong alike is a plan failure: say so rather than ship it.
- **Sentences are literals** in tests, never built through the helper under test.
- **Real interaction.** jsdom tests navigate outside `act`, with real events.
- **Evidence.** Logs and each task's report (`$E/T<n>-report.md`) go to `$E`, written by Bash (`cat >| … <<'EOF'`). The Write tool is refused under `.superpowers/`. Never under `target/`.
- **No migration**, and no SQL beyond a plain `SELECT` (settled call 10).
- **Counts.** `docs/status.md`'s test counts and the closes are Task 8's. Tasks 1–7 leave the Rust total stale. The gate's `status-test-count` step only runs in the Finish.

## Review Focus

The failure modes the spec implies that no task's tests would otherwise exercise, most likely first. Each one's test is in the task named:
1. **A signed-out visit to `/label.html`** (`me()` fails) draws the error's text and never the editor, and calls no truth port. Task 3.
2. **A `user` account that sees every library is refused on both writes.** The refusal is for the role and not the grant, so the fixture account must see every library. Tasks 1 and 2.
3. **The administrator's own API key is refused on both writes** for its credential kind. Tasks 1 and 2.
4. **A cover-size thumbnail (`…/thumb?w=256`) of an unreadable archive** goes through `serve_page`, not `get_thumb`'s tile branch, and is path-free too. Task 4.
5. **Two-level moves keep their answers.** A move of a directory with nothing nested inside it, or one an account sees on both sides through another library, names exactly whom it named before. The existing relocate tests pin this and Task 7 runs them unchanged.


## Pre-flight rulings (controller, 2026-11-12; binding, override the task text where they differ)

From the pre-flight scan `.superpowers/sdd/deploy-evidence/lane-3/preflight.md` (0 BLOCKER, 3 Important, 24 Minor). BASE is **28b18b60** (the spec PR's merge; docs-only over ec4eb11c).

- **Task 1 (I3, Important).** `a_document_that_does_not_parse_is_refused_before_the_grant_is_asked` (routes/tests.rs ~6104) joins class (b): its truth half expects the gate's 403, alike for the granted, the hidden and the unknown slug; its notes half is unchanged. Files also: `held` in auth/tests.rs (~3128, ~3190-3209) reads notes alone, and the KeepsUnseen comment ("a note and a truth box") is corrected — notes are the only `KeepsUnseen` route left.
- **Task 2 (I2, Important).** `every_route_naming_a_work_answers_a_hidden_one_as_a_slug_no_work_holds` (routes/tests.rs ~5932-6012) is kept: exempt `/api/works/{work}/alignments` from `told_apart` by name, with the reason (O58: no request from a limited account reaches its grant); keep the hidden-equals-unknown assertion for it and keep `walked == 13`. Record it as its own class in T2-report.
- **Task 2 (Minors).** Class (b) also takes `narrowing::a_correction_naming_a_side_kids_does_not_see_is_refused_before_its_body` (~7460) and `narrowing::manga_alone_corrects_en_to_fr_with_its_own_documents_etag` (~7502). "The alignments entry leaves the `Refuses` loop's skip list" means BOTH skip lists (auth/tests.rs ~1739-1765 and ~2070-2085) and the comment calling it "Driven in its own dedicated block below". `get_for`'s alignments early return (routes/tests.rs ~4472-4477, ~4536-4539) goes, with its comment; if `get_for` is then always `Some`, change its return type rather than `#[expect]` pedantic. `patch_alignments`' O264 comment (routes.rs ~1983-1986) keeps the 409 path-free but states the reason without "any account granted the work calls this". The `driven` assertion message states the reason, not the story.
- **Task 3.** The signed-out fixture rejects with `new Unauthenticated()` (what `request` throws on a 401), and the test asserts `not signed in`. Ruling: drop the "`patchAlignment` port is never called" assertion — it cannot fail under the named mutation (no Confirm is clicked) and the banner assertion carries the test — cost if wrong: one weaker assertion gone. `App.test.tsx`'s `ports.me` already answers `admin: true` at BASE: leave it unchanged.
- **Task 4.** "Five sites" are four sites with five cases (`serve_page` carries the page and the `?w=256` case). There are no `not_found` tests: put the `log_line` unit test in `routes/tests.rs` beside the other path-free answer tests this task adds, and name the place in T4-report. Ruling: `answered_and_logged`'s call of `log_line` rests on the Finish drive (no test sees it), stated in T4-report — passing the writer as a parameter adds surface for one call — cost if wrong: a dropped log line goes unseen until a drive. Step 1's grep adds `StatusCode::[A-Z_]+, (e|why|message)\)`. `get_thumb`'s ETag paragraph (routes.rs ~874-880), which T4's hoist makes false, is reworded.
- **Task 5 (I1, Important).** `prepare`'s `std::fs::metadata` site (download.rs ~712) has no seam: only a race between `is_file` (~263) and `metadata` reaches it. Fix it untested, as settled call 7 does for `locate`. Relabel "Edition download, one member: the member's file is deleted" as a second `member_page_entries` (~730) case. Step 3 names four sites with tests and one without; its "put back the pass-through at each of the five sites in turn" covers the four.
- **Task 6.** Pin the new fixture's edge (`_with_an_edge`'s run) and the `PATCH` body: the body must ask a real change (not already true), so the row resolution and file write run and the "row after the file write" mutation can fail.
- **Task 7.** `leaving` has no `moved.path`: take the path from the same `scan::roots` row by `id == root_id`. New tests build directories through `add_root_checked_in` / `a_scanned_dir_in`, never `create_library(`/`insert_root(` (lane 1 changes both signatures). Files also: `web/src/libraries.ts` ~419-426 (`Moved.gains_sight`/`loses_sight` docs) and `README.md` ~312 ("gain and lose sight of the directory" gains "or a directory inside it").
- **Task 8.** Step 3 also greps `docs/status.md` for the name of every test voided or changed by Tasks 1, 2 and 6 (their reports list them) and marks each closed row citing one "voided by O58 (spec decision 6)" — known: O191 (~1800), O239 (~1848), O115 (~1724) — and corrects O60's (~1668) truth sentence ("a body that does not parse is refused as that" now holds for an administrator; a limited account gets the gate's 403 first). O264's (~1873) reason for the path-free 409 is marked beside the O93 marker. At rebase (after lanes 1 and 2 merge) Task 8's counts are redone: the heading count, "leaving", What remains, the Rust total and node/jsdom counts, and spec gap 2's "next free O-number" is taken again.
- **Finish.** The O267 thumbnail check uses a thumbnail not already cached (or clears the scratch cache first), so a cached 200 cannot pass it. The PR body says lane 4 owns the choice of resolving `at` before `store` or adding a `NoRow` variant (Task 6 changes the three `store` signatures to a final `mint` and `Result<(), NoRow>`).

---

### Task 1: Only an administrator's browser session writes the truth document (O58, part 1)

Spec §5 decisions 1, 5 and 6, and "Where", O58. Survey §O58.

**Files:**
- `docs/superpowers/plans/2026-11-12-go-public-lane-3-second-accounts.md`: copy this plan in from `$E/../plans/`. It rides in this task's commit.
- `crates/taiyaku-serve/src/routes.rs`:
  - `put_truth` (~1350) opens with the gate, before `std::str::from_utf8`. Its doc says the route is an administrator's (O58) and why: every reader is served the document. It also says the `view` and `truth::as_stored` stay for a later per-account capability (spec decision 6).
  - `grant_postures`' truth comment (~2405): the read refuses and filters, and the write is an administrator's.
  - `narrowing_postures` (~2893): truth becomes `Narrows`.
- `crates/taiyaku-serve/src/auth/tests.rs`:
  - `REFUSE_A_DEVICE_CREDENTIAL` gains `"/api/works/{work}/truth"`.
  - `every_route_declares_what_it_does_with_a_narrowed_edition` (~3029): the counts `[17, 6, 2, 1]` become `[18, 6, 1, 1]`, and `named_by_the_administrator` becomes `["/api/works/{work}", "/api/works/{work}/truth"]`. The administrator's truth answer holds part2's `sha256` and the text "on part2"; Manga's leaves `raw` out.
- `crates/taiyaku-serve/src/routes/tests.rs`: `as_the_fixture_account` (~533), and the fallout below.

**Fallout, a starting list** (classify by settled call 3 whatever goes red):
- `no_grant_check_stalls_the_runtime`: `put_truth` in both the granted and the granted-nothing halves. Drop it from both, and say why in the test's doc.
- `the_six_requests`, with its two users:
  - `progress_notes_and_truth_answer_a_hidden_work_as_a_slug_no_work_holds`: the truth `PUT` leaves the set, and the closing "labelled" checks go. Rename whatever now names a wrong count.
  - `a_write_to_a_hidden_work_is_refused_before_its_own_checks_are_made`: `writes_that_fail_their_own_checks`' two truth cases go.
- `a_truth_write_keeps_the_entry_of_an_edition_the_account_cannot_see`: the read half stays. Each `PUT` now expects 403 `this route is an administrator's` and the store unchanged.
- In `mod narrowing`:
  - `a_truth_write_manga_read_before_it_was_granted_kids_is_refused`, `manga_alones_truth_write_keeps_raws_entry` and `a_truth_write_changing_raw_as_manga_alone_is_refused` are void.
  - `manga_alone_reads_truth_for_en_alone_under_its_numberings_etag` stays.

- [ ] **Step 1: Copy the plan in.** As in Files.
- [ ] **Step 2: Write the failing test** `only_an_administrators_browser_session_writes_the_truth_document` in `routes/tests.rs`, over `test_support::router_over_a_live_library()`. Use any work it holds. The stored document is read through `crate::truth::load`.
  - **The fixture key** (`test_support::fixture_authorization()`, the administrator's own key, set as the request's `authorization` header):
    - `GET /api/works/{slug}/truth` answers 200 with an `ETag`.
    - `PUT` with that tag and a body labelling one page answers 403, and the body contains `API key`.
    - The store is unchanged.
  - **A `user` that sees every library:**
    - Set it up with `taiyaku_library::account::add(conn, "reader", "pw", "user")`, then `account::set_every_library`, then `test_support::authenticating_as(crate::build_router(state.clone()), &state.library, id)`.
    - `GET` answers 200. `PUT` answers 403 with the body `this route is an administrator's`. The store is unchanged.
  - **The fixture's own router** (the administrator's session): the same `PUT` answers 200, and the store holds the body.
  - Run the serve crate. The key and `user` cases fail with 200.
- [ ] **Step 3: Implement the gate.**
  - Read the principal from the handler's `extensions`; an absent one answers `NoPrincipal`. Then `if let Some(refusal) = crate::session::administering_or_forbidden(principal, &extensions) { return refusal; }`.
  - Add `By::Cookie` to `as_the_fixture_account`.
  - Run the serve crate and apply the fallout rule to every red test. Record each one in the report with its class (a) or (b) and what became of it.
  - Change the two tables and the narrowed-edition test's figures.
- [ ] **Step 4: Watched mutations.**
  - Delete the gate: the key and `user` cases fail.
  - Replace it with `refuse_a_device_credential` alone: the `user` case fails.
  - Replace it with a role comparison alone: the key case fails, and so does `no_module_answers_the_role_question_alone`. Record both.
  - Move the gate below the body's refusal (`TruthDoc::from_json`'s `bad_request`): `exactly_the_declared_routes_refuse_an_api_key` fails.
- [ ] **Step 5: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `Only an administrator signed in through a browser writes a work's truth document (O58)`. Write `$E/T1-report.md`.

---

### Task 2: Only an administrator's browser session corrects an alignment (O58, part 2)

Spec §5 decisions 1, 5 and 6, and "Where", O58. Survey §O58 and "Collisions".

**Files:**
- `crates/taiyaku-serve/src/routes.rs`:
  - `patch_alignments` (~1762) opens with the gate, before `serde_json::from_slice`. Its doc's grant paragraphs ("Applies the grant itself", "Filters its response", "Refuses a correction that itself names a withheld edition") now say the route is an administrator's (O58) and those halves are kept for a later per-account capability (spec decision 6). The "what is stored is never filtered" paragraph stays.
  - `grant_postures` (~2409-2426): alignments becomes `GrantIndependent`, its reason worded as the four metadata routes' are (O58 makes it an administrator's, and O86 drops an administrator's root grant).
  - `GrantPosture::RefusesAndFilters`' doc: its example becomes progress, notes and truth.
  - `narrowing_postures`: alignments becomes `ADMIN`. `NarrowingPosture::Refuses` and its import go.
- `crates/taiyaku-serve/src/session.rs`: `administering_or_forbidden`'s doc. The paragraph "O58's two routes are deliberately untouched…" (~852-857) becomes: both shared-document writes open with this gate (O58).
- `crates/taiyaku-serve/src/auth/tests.rs`:
  - `REFUSE_A_DEVICE_CREDENTIAL` gains `"/api/works/{work}/alignments"`.
  - In `every_route_declares_its_grant_posture`:
    - `driven` goes from 26 to 25. Its message gains one clause saying O58 returned the alignments route to `GrantIndependent`, as O86 returned four.
    - `table.len() - driven` goes from 40 to 41.
    - The alignments entry leaves the `Refuses` loop's skip list.
    - The alignments `Refuses` and `Filters` blocks (~2391-2483) go.
  - In `every_route_declares_what_it_does_with_a_narrowed_edition`: the `Refuses` arm goes, the counts array loses its last entry, and `NoPages` goes from 40 to 41.
  - Four tests go: `patch_alignments_refuses_a_correction_naming_a_withheld_edition`, `a_restricted_readers_own_etag_round_trips_through_a_correction_between_visible_editions`, `a_correction_between_visible_editions_omits_the_withheld_one_from_its_response` and `a_correction_naming_the_withheld_edition_on_the_from_side_is_still_refused`. So does any helper only they used; clippy's `dead_code` names it.
- `crates/taiyaku-serve/src/routes/tests.rs`, a starting list:
  - `a_hidden_work_is_refused_before_its_correction_is_read`: void.
  - `no_grant_check_stalls_the_runtime`: drop `patch_alignments` from both halves. Its doc's sentence on why `patch_alignments` asks its three questions in one trip goes.
  - `narrowing::correcting_raw_and_en_as_manga_alone_is_refused`, `narrowing::correcting_a_side_the_view_hides_is_refused_as_a_hidden_work` and the `corrected` helper: void.

- [ ] **Step 1: Write the failing test** `only_an_administrators_browser_session_corrects_an_alignment` in `routes/tests.rs`, over `test_support::router_over_a_live_library_with_an_edge()`. The `run` rows are read by SQL, as the grant-posture test's `seeded_run` reads them.
  - **The fixture key:**
    - `GET /api/works/{slug}` answers 200 with an `ETag`: a key reads.
    - `PATCH /api/works/{slug}/alignments` with that tag and `{"from":"raw","to":"en","a":"1","b":"0"}` answers 403, and the body contains `API key`.
    - The `run` rows are unchanged.
  - **A `user` that sees every library**, made as in Task 1: the same request answers 403 `this route is an administrator's`, and the rows are unchanged.
  - **The administrator's session:** the same request answers 200.
  - Run the serve crate. The key and `user` cases fail.
- [ ] **Step 2: Implement the gate as in Task 1.** Apply the fallout rule to every red test and record each in the report. Change the tables, the doc comments and the two posture tests' figures.
- [ ] **Step 3: Watched mutations.** As Task 1's four, on `patch_alignments`. The fourth moves the gate below the `Correction` parse's `bad_request`, and turns `exactly_the_declared_routes_refuse_an_api_key` red through the alignments route.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `Only an administrator signed in through a browser corrects a work's alignments (O58)`. Write `$E/T2-report.md`.

---

### Task 3: The web offers correcting and labelling to an administrator alone (O58, the web half)

Spec §5 decision 7 and "Where", O58. §9 "Lane 3" ("jsdom: no editor for a `user`").

**Files:**
- `web/src/keyplan.ts`:
  - `Guards` gains `mayCorrect: boolean`.
  - In `keyPlan`'s `intent.kind === 'correct'` branch, a `false` answers `undefined` ahead of the `submitting` check. The key is then not the reader's, and the browser keeps it.
- `web/src/App.tsx`:
  - The keydown handler's guards object (~837-850) sets `mayCorrect` from `gate?.who.admin === true`. Read it as the handler reads its other values, through a ref assigned during render. The listener subscribes once, so a value it captured at subscription would go stale.
  - `<Help>` (~3071) gets `mayCorrect`.
- `web/src/keys.ts`: `withoutCorrecting`.
- `web/src/Help.tsx`: `mayCorrect`. When it is `false`, the help is built from `helpGroups(modules, reading, withoutCorrecting(READER_KEYS))`.
- `web/src/Labeller.tsx`: `LabelPage`.
  - It asks `who()` once on mount and draws nothing until the answer arrives.
  - `admin` draws `<Labeller ports={ports} />`.
  - Any other account draws `<p role="status">` with the labeller sentence.
  - A rejection draws `<p role="alert">` with `messageOf(error)`.
  - No `ports` call is made unless `who()` answered `admin`.
- `web/src/label.tsx`: renders `<LabelPage ports={TRUTH_PORTS} who={me} />`, with `me` from `./api.ts`.
- Tests:
  - `web/tests/keyplan.test.ts`: its guards fixture gains the field.
  - `web/tests/Help.test.tsx`.
  - `web/tests/App.test.tsx`: its `ports.me` mock answers `admin: true` by default (~268).
  - `web/tests/Labeller.test.tsx`.
  - `web/tests/keys.test.ts` (`describe('READER_KEYS')`) gets `withoutCorrecting`'s case.

- [ ] **Step 1: Write the failing tests.**
  - **keyplan:** `c` with `mayCorrect: false` answers `undefined`. With `true` it answers `begin-correction`, as today.
  - **`withoutCorrecting`:** it drops exactly the one row labelled `correct this pairing` and keeps the rest in order.
  - **Help:** with `mayCorrect={false}` there is no `correct this pairing` row; with `true` the row is there.
  - **App, jsdom, `me` answering `admin: false`:**
    - After the load, a real `c` keydown draws no correction banner. Find it as the existing correction tests do.
    - The `patchAlignment` port is never called.
    - The opened Help has no `correct this pairing` row.
    - The existing correction tests, as an administrator, stay green.
  - **`LabelPage`:**
    - `who` answering `admin: false` draws the sentence (literal), no work picker, and no `works` call.
    - `admin: true` draws the picker and calls `works` once.
    - `who` rejecting with `new Error('reading the account: 401')` draws an alert with that text, no picker, and no port call.
  - Run the node and jsdom projects. They fail.
- [ ] **Step 2: Implement.** Run both projects to green.
- [ ] **Step 3: Watched mutations.**
  - Drop the `mayCorrect` check: the keyplan and App cases fail.
  - Make Help ignore `mayCorrect`: the Help case fails.
  - Make `LabelPage` draw the `Labeller` for everyone: the non-admin case fails.
  - Call `ports.works` before `who()` answers: the no-call assertions fail.
- [ ] **Step 4: Run and commit.** Run `tsc -b` and `biome check`. Commit `The reader offers correcting a pairing, and the labeller labelling, to an administrator alone (O58)`. Write `$E/T3-report.md`.

---

### Task 4: Pages, thumbnails and OPDS answer without a server path (O267, part 1)

Spec §5 decision 2, decision 7 ("logs the full error to stderr") and "Where", O267. Survey §O267.

**Files:**
- `crates/taiyaku-serve/src/routes.rs`:
  - The three new helpers, beside `not_found`.
  - `serve_page`: keep the asked index before `page_to_full` translates it, and its fetch `Ok(Err(e))` answers `unreadable` with the page sentence.
  - `get_thumb` (~896): take `view.locate(index)` out of the closure, as `serve_page` does, and answer its refusal with `not_found(message)`. The fetch's `Ok(Err(e))` answers `unreadable` with the page sentence.
- `crates/taiyaku-serve/src/opds.rs`:
  - `stream_page`: the fetch's `Ok(Err(e))` answers the page sentence.
  - `cover`: take `page_to_full(0)` and `locate(first)` out of the closure (their refusals keep `missing(message)`); the fetch's `Ok(Err(e))` answers the cover sentence.
- `crates/taiyaku-serve/src/view.rs`: `EditionView::locate`'s missing-edition message names `` `{slug}` `` (`self.work.id`), not `self.work.path`.
- `crates/taiyaku-serve/src/lib.rs`: `ServeState::page`'s doc. Its message still names the work's path, for the log. The routes answer without it (O267).
- Tests:
  - `routes/tests.rs` for the page and both thumbnail branches.
  - `opds/tests.rs` for OPDS, sending a key as OPDS clients do (`test_support::key_authenticating`).
  - A unit test of `log_line` beside `not_found`'s tests.

- [ ] **Step 1: The sweep.** Before writing anything, list every answer body on a route a non-administrator can call that is built from `.display()`, a path, or a source's error:
  - `grep -n '\.display()' crates/taiyaku-serve/src/*.rs`;
  - `grep -n 'not_found(e)\|not_found)\|missing(e)\|Failed(message) =>' crates/taiyaku-serve/src/*.rs`;
  - skip test files, and read each hit.
  - Write the list to `$E/T4-sweep.md`. For each hit give its route, whether a non-administrator reaches it, whether it can carry a path, and which task fixes it.
  - The finds this plan expects are settled call 7's and Task 5's. **A find outside them: stop and report to the controller before fixing it.**
- [ ] **Step 2: Write the failing tests.** Use a fixture whose archive is real, such as `test_support::router_over_a_scanned_cbz()`, and overwrite its file with `b"not an archive"` before the request. Each case asserts:
  - the status is 404;
  - the body equals its sentence, written out with the fixture's slug and edition;
  - the body does not contain the fixture directory's path as a string.

  The cases:
  - `GET …/pages/0`;
  - `GET …/pages/0/thumb`, the tile;
  - `GET …/pages/0/thumb?w=256`, the cover branch through `serve_page`;
  - OPDS `…/pages/{number}`;
  - OPDS `…/cover`.

  `log_line` into a `Vec<u8>` with a `why` naming a path writes exactly `taiyaku: <said>: <why>\n`. Run the serve crate. The five route cases fail.
- [ ] **Step 3: Implement.** Run the serve crate to green. The existing page and thumbnail refusals for an index past the end keep their words.
- [ ] **Step 4: Watched mutations.**
  - Put `not_found(e)` or `missing(e)` back at each of the five sites in turn: its case fails.
  - Drop the write in `log_line`: the unit test fails.
- [ ] **Step 5: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `A page, thumbnail or cover that cannot be read is answered without a server path, and the path goes to the log (O267)`. Write `$E/T4-report.md`, with the sweep list.

---

### Task 5: Downloads and failed saves answer without a server path (O267, part 2)

Spec §5 "Where", O267. Task 4's sweep list.

**Files:**
- `crates/taiyaku-serve/src/download.rs`:
  - `prepare`: the passthrough's `std::fs::metadata` error (~712) and the generated edition's `taiyaku_read::member_page_entries` error (~730) answer `unreadable` with the edition sentence.
  - `prepare_member` (~1281) answers it with the member sentence.
- `crates/taiyaku-serve/src/routes.rs`: the `Failed` arms of `impl IntoResponse for NoProgress` (~305) and `for NoNotes` answer `answered_and_logged(StatusCode::INTERNAL_SERVER_ERROR, "the library could not answer this request".to_string(), &message)`.
- Tests:
  - `download/tests.rs`;
  - `routes/tests.rs`, over `test_support::router_over_a_file_backed_library()`.

- [ ] **Step 1: Write the failing tests.** Each asserts its literal sentence and that no part of the fixture's path is in the body.
  - **Edition download, one member:** the member's file is deleted. Answers 404 with the edition sentence.
  - **Edition download, two members:** a fixture whose edition has two member files, one overwritten with garbage. Answers 404 with the edition sentence.
  - **Member download:** the member's file is deleted. Answers 404 with the member sentence.
  - **Progress `PUT` after the store's directory is renamed.** Rename `db/` once the router has opened it. The shared connection keeps its open file, and `Library::saving` cannot open a second one. Answers 500 with the store sentence.
  - **Notes `PUT` after the same rename,** its `If-Match` read before it. Answers 500 with the store sentence.
  - Run the serve crate. They fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations.** Put back the pass-through at each of the five sites in turn: its case fails.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `A download that cannot be read, and a position or note the store cannot save, are answered without a server path (O267)`. Write `$E/T5-report.md`.

---

### Task 6: A live registry never mints a `bridge:` row (O184)

Spec §5 decision 4 and "Where", O184. `docs/superpowers/plans/2026-11-10-pre-deploy-missing-works.md`, "Part 2, Task 4", whose interfaces are restated above. Survey §O184.

**Files:**
- `crates/taiyaku-serve/src/works.rs`:
  - `Mint`, `NoRow` and `Mint::of`.
  - `row_id_for_write` (~425) and `row_id_for_document` (~448): under `Mint::Never` a missing row is `NoRow::Gone`, and nothing is inserted.
  - Their docs say who mints: a fixed registry does, a live one never does.
- `progress.rs`, `notes.rs`, `truth.rs`: `store` and its doc.
- `alignments.rs`: `store(conn, work_id, doc)`. Its doc and the module doc say the caller resolves the row and that a fixed registry still mints: `test_support::state`'s fixtures and `corpus.rs` pair a fixed registry with an empty library. That replaces "in production it is never minted".
- `crates/taiyaku-serve/src/routes.rs`:
  - `put_progress` (~1508), `put_notes` (~1118) and `put_truth` (~1350) pass `Mint::of(&state.works)`. `Gone` maps to their `NotFound` and `Failed` to their `Failed`.
  - `patch_alignments`: inside `if what != taiyaku_core::Corrected::AlreadyTrue`, first resolve the row with `works::row_id_for_document(&held, &found.id, &next, &found.path, Mint::of(&state.works))`. Do it before the O264 check and the file write. `Gone` answers 404 `` no work `{slug}` ``. Then pass the id to `alignments::store`.
- `crates/taiyaku-serve/src/lib.rs`: the doc of `ServeState::library` (~92-99) on the row `row_id_for_write` "might still mint".
- `crates/taiyaku-serve/src/test_support.rs`: `router_over_a_live_library_with_a_document`.
  - Build it as `router_over_a_live_library_with_an_edge` is.
  - Write a `taiyaku.json` with a solved edge into the fixture, and import it with `taiyaku_library::import::sidecar`.
  - Build the registry with `crate::Works::library`.
- Tests:
  - `works/tests.rs`: the bridge tests (~251-390) pass `Mint::Bridge`, and one still asserts the `bridge:` row.
  - `routes/tests.rs`.
  - Test code that calls a `store` directly (`auth/tests.rs`' fixtures among them) passes `Mint::Bridge`.

- [ ] **Step 1: Write the failing tests.** Each test:
  - deletes the work's row by SQL with no refresh after it (the registry still holds the work, and a lookup hit does not refresh);
  - sends the write as the administrator's browser session, which is `Access::Every`, the only access that reaches the mint;
  - reads `SELECT count(*) FROM work`.

  The cases:
  - **Over `router_over_a_live_library`:** a progress `PUT`, a notes `PUT` and a truth `PUT` each answer 404 `` no work `{slug}` `` (literal), and no work row appears.
  - **Over `router_over_a_live_library_with_a_document`:** an alignments `PATCH` answers 404, no work row appears, and the `taiyaku.json`'s bytes are unchanged.
  - Run the serve crate. They fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations.**
  - The mint restored under `Mint::Never`: each case answers 2xx and a row appears.
  - `Mint::of` answering `Bridge` for a live registry: the same.
  - The `PATCH`'s row resolved after the file write: the file changes.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve` and `taiyaku-cli`, then clippy and fmt. Commit `A write to a work whose row is gone answers 404 from a live registry instead of minting a bridge row (O184)`. Write `$E/T6-report.md`.

---

### Task 7: Move and Remove from this library ask about every directory inside the one they change (O248)

Spec §5 decision 3 and "Where", O248. Survey §O248. O248's row (`docs/status.md` ~1857) holds the example.

**Files:**
- `crates/taiyaku-library/src/relocate.rs`:
  - `sighted`, with the new signature.
  - `leaving`. Before `write`, it reads the directory set: `root_id`, and every `scan::roots(conn)` row whose path `crate::reach::nested_in(&row.path, &moved.path)` answers `Some` for. It asks `sighted` for that set before and after `write`.
    - `gains_sight` holds each account whose after-set has a root its before-set lacks.
    - `loses_sight` holds the reverse.
    - Both are in name order. An account may be in both.
  - The docs of `Moved`'s two lists and of `Leaving`: "the directory or a directory nested in it".
- `crates/taiyaku-library/src/scan.rs`: `Unlinked::loses_sight`'s doc, the same.
- `web/src/MoveCounts.tsx`: `sightOf`'s three strings, and the module doc.
- `web/src/UnlinkRoot.tsx`: the counts paragraph (~157-160).
- `web/src/Libraries.tsx`: the doc at ~33.
- `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md` §3.5 (~408): "who will gain and lose sight of the directory or a directory inside it".
- Tests:
  - `crates/taiyaku-library/src/relocate/tests.rs`: its helpers are `an_account` and `moved`.
  - `crates/taiyaku-library/src/scan/tests.rs`: beside `unlinking_a_dir_another_library_holds_deletes_nothing_and_only_its_accounts_lose_sight`.
  - The web fixtures with "sight of it": `web/tests/MoveRoot.test.tsx`, `Libraries.test.tsx`, `DirPage.test.tsx` and `UnlinkRoot.test.tsx`.

- [ ] **Step 1: Write the failing Rust tests** on the row's three levels:
  - Manga holds `/m` and excludes `kids/tiny`.
  - Kids holds `/m/kids`.
  - Tiny holds `/m/kids/tiny`.
  - Each directory holds a volume.
  - First assert the fixture: before any change, `ann`'s access allows `/m/kids/tiny`, and `reach::of_libraries` over Manga alone does not hold it. Without the exclude, `ann` would keep the directory through Manga and every case below would pass against the defect.

  The cases:
  - `moving_a_directory_names_who_loses_one_nested_in_it`: `ann` is granted Manga and Kids. Move `/m/kids` to `Target::New("Elsewhere")`. `loses_sight == ["ann"]` and `gains_sight` is empty.
  - `moving_a_directory_names_who_gains_one_nested_in_it`: a library Other holds an unrelated `/o`, and `bo` is granted Manga and Other. Move `/m/kids` to `Target::Library(other)`. `gains_sight == ["bo"]` and `loses_sight` is empty.
  - `an_unlink_names_who_loses_a_directory_nested_in_it` (scan tests): `/m/kids` is held by Kids and by a library Also, and `ann` is granted Manga and Kids. Unlink `/m/kids` from Kids. `loses_sight == ["ann"]`.
  - Run `taiyaku-library`. All three fail with empty lists.
- [ ] **Step 2: Implement.** Run `taiyaku-library` and `taiyaku-serve` to green. `libraries_api/tests.rs`' JSON cases must pass unchanged.
- [ ] **Step 3: Change the web words.**
  - Change the four test files' sentences first, as literals. The jsdom project fails.
  - Then change `sightOf` and `UnlinkRoot`. Green.
  - Then the docs and the design's §3.5.
- [ ] **Step 4: Watched mutations.**
  - Make `leaving` ask `sighted` about `&[root_id]` alone: all three Rust cases fail.
  - Restore "sight of it" in `sightOf`: the MoveRoot and Libraries cases fail.
  - Restore it in `UnlinkRoot`: its cases fail.
- [ ] **Step 5: Run and commit.** Run clippy and fmt. Run the jsdom project, `tsc -b` and `biome check`. Commit `Move and Remove from this library name who gains or loses sight of the directory or a directory inside it (O248)`. Write `$E/T7-report.md`.

---

### Task 8: The closes in `docs/status.md`

Read the Open items section, the five rows (O58 ~1666, O184 ~1793, O241 ~1850, O248 ~1857, O267 ~1876) and their notes bullets.

- [ ] **Step 1: Close O58, O241, O267, O184 and O248** with the five-part edit:
  1. The row's checkbox.
  2. The row's text, rewritten to say what was done, with symbols and no line numbers. What each row says:
     - **O58:** both writes open with `administering_or_forbidden`; the web offers both to an administrator alone; decision 8's write halves are kept but unreachable, and their proofs are void.
     - **O241:** closes by consequence. An administrator is `Access::Every` and so is never narrowed.
     - **O267:** the sweep's finds as fixed; the alignments 500 keeps its path, as an administrator's route; `atomic::write` is unchanged (O29).
     - **O184:** Part 2 Task 4, done here.
     - **O248:** the directory and every one nested in it.
  3. The notes bullet.
  4. The "N are now closed — leaving M" sentence (~1482).
  5. The "What remains … M open items" line (~5137). Its count word wraps across a line break, so grep the sentence opener and read the next line.

  Then the edits outside the five rows:
  - **"What is built"** (~142): after "it cannot write that edition's ground truth or correct its alignments (spec §3.8, O239)", say that only an administrator signed in through a browser writes either document (O58).
  - **O93's closed row:** its last sentence ("O58's two routes are not an exception… still open question") gets a marker that O58 closed it.
  - **Spec gap 2:** unless the owner has ruled otherwise, open a row for the work document's member paths, with the next free O-number. Move the "Two hundred and seventy-nine" heading count, the "leaving" count and What remains with it.
  - **The pre-deploy documents:** mark `docs/superpowers/specs/2026-11-10-pre-deploy-design.md`'s decision 5 (~911) and its O184 bullet (~1058), and `docs/superpowers/plans/2026-11-10-pre-deploy-missing-works.md`'s "Part 2, Task 4", as done in go-public lane 3.
- [ ] **Step 2: Counts.** Measure them; never add.
  - **The Rust total:** run `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml --workspace --all-features --test-threads=8`, then `node $W/scripts/status-test-count.mjs $W/target/nextest/default/junit.xml $W/docs/status.md`. Set "Six crates, N tests" (~179) and "of the N are standing checks" (~256) to the figure it wants.
  - **The web counts** in the same sentence: set them from the node and jsdom runs' summary lines.
- [ ] **Step 3: Sweep.**
  - Run `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`, and check each against its sentence.
  - Grep each bare O-number (`O58`, `O241`, `O267`, `O184`, `O248`) across `docs/`, `web/src/`, `crates/` and `scripts/`, and read every hit. Prose that speaks of one as open is marked or corrected.
  - A hit in an older plan or spec under `docs/superpowers/` is history and stays, except the pre-deploy documents in Step 1.
- [ ] **Step 4: Commit.** Commit `O58, O241, O267, O184 and O248 close in docs/status.md`. Write `$E/T8-report.md`.

---

### Finish

1. **Whole-branch review** of `BASE..HEAD`, against the spec, the rulings and this plan's settled calls, in three lanes:
   - (a) the gate, its fallout, the tables and O184: Tasks 1, 2 and 6;
   - (b) the path sweep: Tasks 4 and 5, with `$E/T4-sweep.md`;
   - (c) the web, O248 and the docs: Tasks 3, 7 and 8.

   For (a), the reviewer reads each voided test against settled call 3, and checks that no class (a) test was voided.

   One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **Final gate** on the final commit, at load < 24. Read the `.exit` file. The gate includes the browser project and the coverage floors.
3. **The drive.** Start the server from the controller, or tell the agent never to restart it.

   **Setup:**
   - Record the test library's state:
     - `sha256sum /home/davey/temp/manga-test/library.db`;
     - `stat` it;
     - `touch` a marker in the scratch directory.
   - Build:
     - `cp` the db into a scratch directory under the session scratchpad;
     - build a release `taiyaku-cli` from `$W` and copy the binary in;
     - `node $W/web/node_modules/vite/bin/vite.js build $W/web --outDir <scratch>/dist --emptyOutDir`.
   - Check the port is free (`ss -lptn 'sport = :4399'`).
   - Serve: `<bin> serve --db <scratch>/library.db --cache <scratch>/cache --web <scratch>/dist --addr 127.0.0.1:4399 --insecure-cookies`, with its stderr kept in `$E/drive-server.log`.
   - Accounts on the copy:
     - `printf 'pw\npw\n' | <bin> library user password local --db …`;
     - `library user add reader --role user`, and a password for `reader`;
     - `library user grant-library reader <a library>`;
     - `library key create reader --name drive`, and one for `local`.
   - A scratch tree beside the copy, **never under the test library**, of copied (not linked) small `.cbz` files at `m/v01.cbz`, `m/kids/v01.cbz` and `m/kids/tiny/v01.cbz`. As `local`, add the libraries Manga (`m`, excluding `kids/tiny`), Kids (`m/kids`) and Tiny (`m/kids/tiny`) over it, and grant `reader` Manga and Kids.

   **Check, with chrome-devtools at 1280 and 390 px:**
   - **O58:**
     - As `reader` in a browser: in the reader, `c` opens no correction, and Help has no `correct this pairing` row. `/label.html` shows the labeller sentence.
     - As `local`: `c` opens the correction banner, and `/label.html` shows the picker.
     - By `curl`: `reader`'s key and `local`'s key each get 200 on a truth `GET` and 403 on a truth `PUT` and an alignments `PATCH`.
   - **O267:**
     - Overwrite the scratch tree's `m/v01.cbz` with random bytes. Its page and its thumbnail answer 404 with no part of the scratch path, and `$E/drive-server.log` holds that path.
     - Delete another member, and its download answers the same way.
   - **O248:** Move `m/kids` from Kids to a new library. The dialog names `reader` as losing sight "of it or a directory inside it". Cancel it, link `m/kids` into a second library, and Remove it from Kids: that dialog names `reader` too.

   **Tear-down:**
   - Stop the server by PID and check the port again.
   - `find /home/davey/temp/manga-test -newer <marker>` must print nothing, and the `sha256sum` must equal the first.
   - Then delete the scratch directory.
4. **Claim check** of the closes, the spec edit and the PR body. Put the PR body in pass 1, and scope later passes to the correction commit plus the PR body.
5. **The PR body** at `$E/pr-body.md`, never hard-wrapped. The title is the first line, under 255 characters: measure it.
   - It names the five rows closed, the rulings and the settled calls.
   - It lists the sweep's finds, the voided tests by name, and spec gap 2's row.
   - It tells lane 4 which functions changed.

   Ask the user before pushing.

# Content Matching PR 2: Fingerprints in the Server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fingerprint every page of every readable file in each library with matching on, once per content digest, in a low-priority background pass the admin sees in the scan feed and can cancel, with hashes bit for bit those PR 1 measured. Nothing a reader sees changes.

**Architecture:** PR 1's decode, border, dhash and halves move from `taiyaku-cli` into `taiyaku-serve` (`fingerprint.rs`), pinned to hashes the CLI produced before the move. `taiyaku-library` gains migration 028 (the per-library switch and a `fingerprint` table keyed by digest), a claimed-unit listing built from derive's own claim rule, and the due/store SQL. `taiyaku-serve` gains `fingerprint_pass.rs`: a store-wide pass kicked when a scan job ends uncancelled and at startup, run by four niced, idle-I/O worker threads that take no job lock, wait while the job lock is held or a reader was served in the last 30 s, and report as a second kind of job in `GET /api/library/scans`.

**Tech Stack:** Rust (`image` 0.25 in `taiyaku-serve`, `rusqlite` against system SQLite, 3.40.1 in CI, `std::thread`, `renice`/`ionice` from util-linux), TypeScript/React (vitest, jsdom).

**Spec:** `docs/superpowers/specs/2026-11-17-content-matching-fingerprints-design.md` (approved; cited below as "Design N"), with its parent `docs/superpowers/specs/2026-11-15-edition-matching-by-content-design.md` §1, §7 and Delivery 2.

## Global Constraints

- **Bit for bit with PR 1.** A whole file's stored records equal what `taiyaku-cli`'s `describe_pages` gave at main `9e50c44f` for the same pages: one border per file from the sampled median, a 16×16 dhash of each cropped page, halves for a page wider than tall after the crop. `fingerprint::VERSION = 1`. PR 1's low-information test (`match_experiment/lookup.rs:26-32`, bit count 8..=248) is **not** applied: PR 2 stores hashes only.
- **No codec in `taiyaku-read`'s shipped build**: its `[dependencies]` keep `image` optional behind `test-support` (its `Cargo.toml`).
- **No new crate.** The workspace forbids `unsafe_code` (root `Cargo.toml`, `[workspace.lints.rust]`), and `docs/status.md` states the dependency list as a fact. Thread priority goes through `renice`/`ionice` (Task 5), as `taiyaku-cli/src/user.rs:120-125` goes through `stty`.
- **SQLite 3.40.1.** No `unixepoch()`, no `->>`, no `STRICT` table, nothing newer than 3.40. Upsert (`ON CONFLICT … DO UPDATE`) and temp tables are fine.
- **Migration 028.** `paused-frozen` merged first (#258, main `b9dcb843`): main holds `027_held_reading.sql` and its edits to `scan_jobs::run/run_with/run_watched` and `derive::entries` (`frozen_entries`), and this branch is rebased onto it. This branch's migration is 028 (file name, `MIGRATIONS` entry, `SCHEMA_VERSION` 28). This plan edits neither `run_watched` nor `entries`/`derive_watched`: its `scan_jobs.rs` edits are the `Job` struct (`:40-130`), one match arm (`:207`) and `spawn_job`'s end (`:1011-1016`), and its `derive.rs` edit is one new function after `unattached_chapter_dirs` (`:3402`).
- **Nothing a reader sees changes.** A reader's feed answer, every library view and every page route answer as on main.
- **The pass never takes the job lock** (`ServeState::library_job`) and holds no directory reservation, so a rule save, stamp, Move, Remove and Scan never wait on it or refuse because of it.
- **Comments hold the why** (`node scripts/check-comments.mjs`). No history or measurement in a comment; a block is at most 8 lines. Moved code carries its comments unchanged.
- **Tests.** Rust: `-- --test-threads=8` on every `cargo test` (`NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate). Web, in a worktree: copy `web/node_modules` (never symlink) and call `./node_modules/.bin/vitest --reporter=default` directly. Each task ends with `cargo fmt --all -- --check`, `cargo clippy -p <each crate touched> --all-targets --all-features -- -D warnings` and `node scripts/check-comments.mjs` (web tasks: `pnpm -C web run lint` and `typecheck`). Only Task 12 runs `scripts/check-all.sh` and sets the test count in `docs/status.md`.
- **A test that pins a defect names its mutation.** Each step below says what to hand-apply: apply it, watch that test fail, revert, and confirm with `git diff` that the revert is clean. Assert per element (per page, per unit, per route, per field), never one `toContain` over a whole page. A fixture where right and wrong give the same answer proves nothing: each fixture below says what it separates.
- **Shell.** zsh with noclobber: start logs with `>|`, read them with `rg -a`; `cd` is zoxide, so use absolute paths; never pipe a gate through `tail`.
- **O-numbers.** O338 stays open (PR 2 is one step of it). A deferral gets a new row from **O342**: O336 and O341 are taken on `paused-frozen`, O337 by another branch.
- **Commits** carry no AI attribution, and say what their own commit holds.

## Review Focus

1. **A worker panics on a file** (a decoder bug, a crafted page). The pass goes on, that file is stored unreadable for this build, and a later kick still starts a pass: the pass's state is never left running for good, as `spawn_job` guards a scan (`scan_jobs.rs:953-958`). Test in Task 6.
2. **The share goes away, or a file vanishes, mid-pass.** Nothing is stored for a file whose stamp cannot be read or has changed, so it stays due and the next pass reads it. Test in Task 5.
3. **Four workers meet archival pages** (430 MB pages exist in the corpus). Only one page at or above `LARGE_PAGE_BYTES` decodes at a time across the store, the rule `partition_by_size` keeps in the CLI. Test in Task 5.
4. **A file being copied in while the pass reads it, every time.** The guard drops it, it is tried once per pass, and the pass still ends. Test in Task 8.
5. **The store refuses a row** (a rules save's whole-library derive holds the write lock; a full disk). The worker waits for the job lock to be free before it writes; a write that still fails is said once on stderr, leaves the file due, and the pass goes on. Tests in Tasks 7 and 8.

---

## Choices this plan settles

| Question | Choice | Where |
|---|---|---|
| Crate for the moved code | `taiyaku-serve`, `pub mod fingerprint`. The CLI already depends on serve, and serve already on `image` 0.25.9 with default features, so no edge and no crate is added (a new crate would also need the workspace members, `mutants.yml`'s two crate lists, `comment-budget.json` and README's crate table). The CLI keeps its rayon batch code and re-exports the moved names, so its call sites and its tests compile unchanged. | Task 2 |
| Claim rule without a second copy | `derive::claimed_units`, built from the private helpers `unattached_chapter_dirs` (`derive.rs:3372`) already shares: `Engine::load`, `entries`, `admit`, `claims_of`, `nested_in_claim`, `Seen::claims_subtree` | Task 4 |
| Kick | `spawn_job`, after `reserved.end(..)` (`scan_jobs.rs:1011`), when the job ended `Finished`; and `serve()` after `scheduler::spawn` (`lib.rs:661`) | Task 6 |
| "The server's build" | `about::build_label`'s commit (`about.rs:36`), with `-dirty` when marked; no commit gives no stamp. It is the only per-build value the server has: `CARGO_PKG_VERSION` is `0.0.0` for every build, the Dockerfile passes `TAIYAKU_COMMIT` (`Dockerfile:67`), and `build.rs` reads HEAD. A row stamped with no build is never retried by a build with none (SQL `IS NOT`). | Task 5 |
| `nice 19` and idle I/O per thread | Each worker and the coordinator run `renice --priority 19 -p <tid>` (absolute across util-linux versions) and `ionice -c 3 -p <tid>` on their own thread id (`/proc/thread-self`): niceness and I/O priority are per thread on Linux, `libc` needs `unsafe`, and no crate in the shipped graph wraps `ioprio_set` (`rustix` is only in `tract-onnx`'s dev graph and has no ioprio call). Both commands are in `debian:bookworm-slim` (util-linux, bsdutils). Workers are dedicated `std::thread`s, never the blocking pool, whose threads later serve pages. | Task 5 |
| "Served to a reader in the last 30 s" | `Pass::reader_seen()` at the top of the eight handlers that read a member's bytes for a reader: `routes::get_page` (`:748`), `routes::get_thumb` (`:968`), `book::get_book` (`:78`), `book::get_book_image` (`:125`), `download::edition` (`:414`, also the OPDS download), `download::member` (`:1082`), `opds::stream_page` (`:721`), `opds::cover` (`:806`) | Task 7 |
| Streaming | Border from `member_page(path, i)` for each sampled `i`, one at a time; then `for_each_member_page`, one page in memory | Task 5 |
| A page that fails to decode | Keeps its position as a not-decoded record; the border sample skips it as PR 1's `common_border` does; the file is partial | Task 5 |
| Feed shape | Every job gains `"kind"`, `"scan"` or `"fingerprint"`; the pass has `step: "fingerprinting"`, `dirs: []`, `dir: null`, `libraries` = libraries with matching on; it is never in the held `libraries`, and its end does not move `ended` | Tasks 9, 10 |
| Old rows | Dropped at the end of every pass when no claimed unit (any root, covered or not) holds the digest | Task 4 |

## Shapes shared across tasks

**Migration `028_fingerprints.sql`** (decided here first):

```sql
ALTER TABLE library ADD COLUMN match_by_content INTEGER NOT NULL DEFAULT 1;
CREATE TABLE fingerprint (
  sha256  TEXT PRIMARY KEY,
  version INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('whole', 'partial', 'unreadable')),
  reason  TEXT,
  build   TEXT,
  pages   INTEGER NOT NULL,
  prints  BLOB NOT NULL,
  read_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**The `prints` blob, version 1**, one record per page entry in reading order: a tag byte, `0` not decoded, `1` hashed, `2` hashed with halves; tags 1 and 2 then hold width and height (`u32` LE) and the hash (4 × `u64` LE); tag 2 then the left and right half hashes (4 × `u64` LE each). A change to this layout or to the algorithm raises `fingerprint::VERSION`.

**A unit is due** when every one of these holds: it is a claimed unit (`derive::claimed_units`); its root is covered (in `reach::of_libraries(roots, <libraries with match_by_content>)` and not in `scan::paused`); its `format` is the token of one of `taiyaku_read::Format::PAGED`; it is a file, or a folder with `pages > 0`; and its digest is unknown, or has no row, or a row whose `version` is not `VERSION`, or a `partial`/`unreadable` row whose `build IS NOT` the current build. Due units come newest `scan_entry.first_seen` first, then by path, one per known digest (the first in that order).

**Feed job** (admin only), a pass:
`{"id": 9, "kind": "fingerprint", "step": "fingerprinting", "done": 120, "of": 2097, "dir": null, "dirs": [], "libraries": [{"id": 1, "name": "Manga"}], "elapsed_secs": 12.5, "cancelling": false}`. A scan job is as on main plus `"kind": "scan"`.

**`PATCH /api/library/libraries/{id}`** takes `{"name"?: string, "match_by_content"?: boolean}` (at least one; both apply in one transaction) and answers `{"id", "name", "match_by_content"}`. Each library in `GET /api/library/libraries` gains `"match_by_content": boolean`.

## Lanes

- **Lane A** (worktree `/home/davey/projects/taiyaku/.claude/worktrees/content-fingerprints-a`): Task 1 → Task 2.
- **Lane B** (`…-b`): Task 3 → Task 4.
- **Lane C** (`…-c`): Task 10 → Task 11. Needs only the shapes above; its tests stub the server.
- **Lane D** (`…-d`): Task 5a. No dependency.
- **Main line** (this worktree), after A, B and D are merged in: Task 5 → 6 → 7 → 8 → 9; then C is merged in; then Task 12.

Each lane branches from `content-fingerprints` at the plan's commit, builds with its own `CARGO_TARGET_DIR`, never amends, and is merged back with `git merge --no-ff` in the order A, B, D, C; after each merge the touched crates' tests run once on the merged tree. Lanes A and B both touch `taiyaku-read` (A: `test_support.rs`; B: `format.rs`). Lanes A and D both add a module line to `crates/taiyaku-serve/src/lib.rs`'s module list (`:34-35`): the merge keeps both lines. A lane may set `scripts/comment-budget.json` to what `check-comments` reports in its own tree; at each merge a conflict there is resolved by running `node scripts/check-comments.mjs` on the merged tree and setting the values it reports, never by picking a side.

---

### Task 1: Pin PR 1's output before anything moves

**Files:**
- Modify: `crates/taiyaku-read/src/test_support.rs`, after `cbz_pages` (`:404-432`), behind the same `#[cfg(feature = "test-support")]`
- Test: `crates/taiyaku-cli/src/fingerprint/tests.rs`, one test appended; no existing line changes

**Interfaces (produces):**

```rust
pub fn print_fixture_pages() -> Vec<(String, Vec<u8>)>   // "p00.png".."p25.png", greyscale PNG, in order
pub fn print_fixture_cbz(dir: &Path) -> PathBuf           // dir/print-fixture.cbz holding them
pub fn print_fixture_folder(dir: &Path) -> PathBuf        // dir/print-fixture/ holding them as files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedPrint { pub width: u32, pub height: u32, pub hash: [u64; 4], pub halves: Option<([u64; 4], [u64; 4])> }
pub const PRINT_FIXTURE: [PinnedPrint; 26];               // what PR 1's describe_pages gives for them
```

**The fixture**, and what each property separates:
- Art: ink that varies along both axes, never reaches 255, and differs per page (seeded by the index), as `ink` in the CLI's fingerprint tests does. No line inside the art is flat, and no two pages hash alike.
- `p00`: 96×128, no bars (a full-bleed cover; a minimum instead of the median cancels the crop).
- Even pages `p02`–`p24`: flat 255 bars, left 6, right 3, top 2, bottom 5; 96×128, except `p08` at 201×128 (an odd width, so the right half's width is pinned) and `p10` at 130×128.
- Odd pages: 96×128 with 12-pixel bars on every side (unsampled at step 2; sampling every page moves the median to 12).
- So the samples are pages 0, 2, …, 24, the median is (6, 3, 2, 5), and the symmetric border is (6, 6, 5, 5) (dropping `symmetric` crops 3 and 2). Every page is cropped by it: 84×118, `p08` 189×118 with halves, `p10` 118×118 with none (wider than tall only before the crop).

- [ ] **Step 1: Write the fixture and the failing test.** `PRINT_FIXTURE` starts all zero. Test `pr1_describes_the_print_fixture_as_pinned`: read `print_fixture_cbz(dir)` with `taiyaku_read::read_edition`; assert `common_border(&pages) == Border { left: 6, right: 6, top: 5, bottom: 5 }`; assert the 26 hashes pairwise distinct; then `describe_pages(&pages)` and, **for each index `i` separately**, width, height, hash words and halves equal `PRINT_FIXTURE[i]`, with `i` in the message. The failure message prints the actual records as a paste-ready `PinnedPrint` array literal (hex words).
- [ ] **Step 2: Run it.** `cargo test -p taiyaku-cli --all-features -- --test-threads=8 fingerprint::tests::pr1_describes_the_print_fixture_as_pinned`. Expected: FAIL on page 0's hash, printing the literal.
- [ ] **Step 3: Paste the printed literal into `PRINT_FIXTURE`.** These are PR 1's values by construction: `git diff main --stat -- crates/taiyaku-cli/src ':!crates/taiyaku-cli/src/fingerprint/tests.rs'` prints nothing. Its doc comment says why the values must not change (PR 1's bars were set on them; `docs/notes.md`, "Edition matching by content: PR 1's measurement (2026-11-16)").
- [ ] **Step 4: Run it.** Expected: PASS.
- [ ] **Step 5: Watch the fixture separate.** In `crates/taiyaku-cli/src/fingerprint.rs` hand-apply each, run Step 2's command, see it FAIL, revert: (a) the median read as `v[0]`; (b) `.step_by(step)` removed; (c) `.symmetric()` removed; (d) halves cut from `raw` instead of the cropped page; (e) `width > height` as `>=`; (f) `Lanczos3` as `Triangle`. Then `git diff --exit-code -- crates/taiyaku-cli/src/fingerprint.rs`.
- [ ] **Step 6: Commit** `test_support.rs` and `fingerprint/tests.rs`: "Pin what PR 1's describe_pages gives for a 26-page fixture, before the code moves".

### Task 2: Move the algorithm into taiyaku-serve

**Files:**
- Create: `crates/taiyaku-serve/src/fingerprint.rs`, `crates/taiyaku-serve/src/fingerprint/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`pub mod fingerprint;` in the module list, `:7-63`)
- Modify: `crates/taiyaku-cli/src/fingerprint.rs` (delete what moved; re-export), `crates/taiyaku-cli/src/cache.rs` (`From<PagePrint> for CachedPage`; `CachedPage` and its serde attributes unchanged, so cache files stay byte-identical)
- Modify, only if `check-comments` reports a share moved: `scripts/comment-budget.json` (serve up and CLI down by the moved doc comments, each set to the reported share, per `docs/ci.md`, "The comment guard"). Modify the comment at `crates/taiyaku-cli/Cargo.toml:15-16` only if it now misplaces image decoding.

**Interfaces (produces):** `taiyaku_serve::fingerprint`:

```rust
pub const VERSION: u32 = 1;
pub const LARGE_PAGE_BYTES: usize;                    // moved unchanged
pub struct Border { pub left: u32, pub right: u32, pub top: u32, pub bottom: u32 }   // moved; derives as now
impl Border { pub fn symmetric(self) -> Self }        // now pub: the CLI's tests call it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePrint { pub width: u32, pub height: u32, pub hash: PageHash, pub halves: Option<(PageHash, PageHash)> }
pub fn decode_luma(bytes: &[u8]) -> Result<GrayImage, String>;    // moved
pub fn flat_border(page: &GrayImage) -> Border;                   // moved, now pub
pub fn crop(page: &GrayImage, border: Border) -> GrayImage;      // moved, now pub
pub fn fingerprint(page: &GrayImage) -> PageHash;                 // moved
pub fn border_sample(page_count: usize) -> impl Iterator<Item = usize>;   // 0, step, 2·step… with step = max(n/12, 1)
pub fn median_border(seen: &[Border]) -> Border;                 // per-side median, then symmetric; none gives Border::default()
pub fn common_border(pages: &[Page]) -> Border;                  // border_sample + decode_luma + flat_border + median_border
pub fn describe_page(raw: &GrayImage, border: Border) -> PagePrint;   // crop, hash, halves when wider than tall
pub fn encode(pages: &[Option<PagePrint>]) -> Vec<u8>;           // "The prints blob" above
pub fn decode(blob: &[u8]) -> Result<Vec<Option<PagePrint>>, String>;
```

The CLI keeps `partition_by_size`, `decode_all`, `fingerprint_edition`, `describe_pages` (now `common_border` plus `decode_all(pages, |_, raw| CachedPage::from(describe_page(raw, border)))`) and `hamming`, and adds `pub use taiyaku_serve::fingerprint::{Border, LARGE_PAGE_BYTES, common_border, decode_luma, fingerprint};` plus a `#[cfg(test)]` import of `crop` and `flat_border` and whatever else its unchanged tests' `use super::*` needs. Every call site (`main.rs:812`, `cache.rs:67`, `geometry_experiment.rs:61`, `report.rs:15`, `score_corpus.rs:135`, `region_experiment.rs:94`, the `decode_all` callers) compiles unchanged.

- [ ] **Step 1: Write the failing serve tests** in `fingerprint/tests.rs`:
  - `the_moved_code_describes_the_print_fixture_as_pinned`: pages from `print_fixture_pages`, `common_border`, `describe_page` per decoded page; each index equals `PRINT_FIXTURE[i]` separately.
  - `border_sample_steps_by_a_twelfth_of_the_count_from_the_first`: `border_sample(26)` is `[0, 2, …, 24]`; `border_sample(25)` is `[0, 2, …, 24]` (n/13 gives step 1 there, `div_ceil` step 3); `border_sample(11)` is `0..11`; `border_sample(0)` is empty. Mutations: drop `.max(1)`; step `n / 13`; step `div_ceil(n, 12)`.
  - Serve copies of Task 1's two extra CLI pins, against the moved code directly (cargo-mutants mutating serve runs only serve's tests): `median_border_takes_the_upper_middle_of_an_even_sample` (the CLI's `pr1_takes_the_upper_middle_of_an_even_sample`, same pages, same pinned border) and `common_border_samples_at_a_step_of_the_count_over_twelve_rounded_down` (the CLI's 25-page pin). Mutations: lower median `v[(len-1)/2]`; average of the two middles; step `n / 13`; step `div_ceil`.
  - `a_blob_reads_back_page_for_page`: `[None, Some(portrait), Some(landscape with halves)]` round-trips, each index asserted; the blob is `1 + 41 + 105` bytes long.
  - `a_cut_or_unknown_blob_is_refused`: the same blob less its last byte, and one whose first tag is `3`, are each `Err`.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 fingerprint::`. Expected: FAIL to compile (no module).
- [ ] **Step 3: Move the code**, carrying each moved item's comments unchanged, and write `border_sample`, `median_border`, `describe_page`, `encode`, `decode`.
- [ ] **Step 4: Run** Step 2's command, then `cargo test -p taiyaku-cli --all-features -- --test-threads=8 fingerprint:: cache::`. Expected: PASS, Task 1's test included. `git diff main --numstat -- crates/taiyaku-cli/src/fingerprint/tests.rs crates/taiyaku-cli/src/cache/tests.rs` shows 0 deletions.
- [ ] **Step 5: Watch it separate in its new home.** Apply Task 1's mutations (a)–(k) to `crates/taiyaku-serve/src/fingerprint.rs` ((g) right half cut `width - mid` wide; (h) lower median; (i) average of the middles; (j) step n/13; (k) step div_ceil): each fails a serve test (`the_moved_code_describes_the_print_fixture_as_pinned` or the two serve copies above) (cargo-mutants tests only the mutated crate, so this serve test is what stands guard). Revert; `git diff --exit-code` on the file.
- [ ] **Step 6: Commit**: "Move PR 1's fingerprint into taiyaku-serve, and the CLI calls it there".

### Task 3: Migration 028 and the library switch

**Files:**
- Create: `crates/taiyaku-library/src/schema/028_fingerprints.sql` ("Shapes" above, with a why comment per statement: on by default so every library is matched unless an admin turns it off; keyed by content digest so a rename or a copy costs nothing; `build` is what a partial or unreadable row is retried against)
- Modify: `crates/taiyaku-library/src/schema.rs` (`SCHEMA_VERSION` to 28; `MIGRATIONS` to 28 entries, appending after 027)
- Modify: `crates/taiyaku-library/src/libraries.rs` (`Listed` `:26-36` and `list` `:43`; the setter after `rename`, `:215-245`)
- Modify: `crates/taiyaku-serve/src/libraries_api.rs` (`libraries_json` `:159-165`; `patch_library` `:403-440` and its doc)
- Test: `crates/taiyaku-library/src/schema/tests.rs`, `crates/taiyaku-library/src/libraries/tests.rs`, `crates/taiyaku-serve/src/libraries_api/tests.rs`

**Interfaces (produces):**

```rust
// taiyaku_library::libraries
pub struct Listed { /* as now */ pub match_by_content: bool }
#[derive(Debug, PartialEq, Eq)] pub enum SwitchError { Missing(i64), Failed(String) }
pub fn set_match_by_content(conn: &Connection, id: i64, on: bool) -> Result<(), SwitchError>
```

- [ ] **Step 1: Write the failing tests.**
  - `migration_28_turns_matching_on_for_every_library`: two libraries planted at version 27 (`migrate_with(&conn, &MIGRATIONS[..27])`, as the file's neighbours do), then migrate: both read `match_by_content = 1` (each asserted); a library made after the migration reads 1; `fingerprint` exists and refuses `outcome = 'other'`. Mutation: `DEFAULT 0`.
  - `turning_matching_off_is_one_library_s_alone`: two libraries; `set_match_by_content(a, false)`; `list` gives a `false` and b `true`; turning a back on reads `true`; an unknown id is `SwitchError::Missing(id)`. Mutation: the `UPDATE` without its `WHERE id`.
  - In `libraries_api/tests.rs`, beside the rename tests: `PATCH {"match_by_content": false}` answers 200 with `id`, `name` and `match_by_content: false` (each field asserted); `GET /api/library/libraries` then shows `false` for it and `true` for the other; `PATCH {}` and `PATCH {"match_by_content": "no"}` are each 400; `PATCH {"name": "X"}` answers `match_by_content: true`; a body with both renames and switches; a reader is refused as the rename refuses one.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 schema:: libraries::` and `cargo test -p taiyaku-serve --all-features -- --test-threads=8 libraries_api::`. Expected: FAIL.
- [ ] **Step 3: Implement** the migration, `Listed.match_by_content`, `set_match_by_content`, the list field and the PATCH (one transaction when both fields are given; refusals in the route's existing `AdminAnswer::Unusable` words, naming `match_by_content` as a boolean).
- [ ] **Step 4: Run** Step 2's commands. Expected: PASS, the generic schema tests (backup before migrating, stepwise equals whole) included at 28.
- [ ] **Step 5: Commit**: "Migration 028: a per-library match-by-content switch, on by default, and the fingerprint table".

### Task 4: Which units are due

**Files:**
- Modify: `crates/taiyaku-read/src/format.rs` (`:95-97`): `Format::PAGED`; `is_readable` reads it
- Modify: `crates/taiyaku-library/src/derive.rs`: `Claimed` and `claimed_units` after `unattached_chapter_dirs` (`:3402`); nothing else in the file
- Modify: `crates/taiyaku-library/src/scan.rs:4210`: `mtime_secs` to `pub(crate)`, so the guard compares a stamp the way the walk recorded it
- Create: `crates/taiyaku-library/src/fingerprints.rs`, `crates/taiyaku-library/src/fingerprints/tests.rs`; `pub mod fingerprints;` in `lib.rs` (`:11-50`)
- Test: `crates/taiyaku-read/src/format/tests.rs`, `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces (produces):**

```rust
// taiyaku_read::Format
pub const PAGED: &'static [Format] = &[Format::Dir, Format::Zip];   // what the reader reads pages from
// is_readable(self) == Self::PAGED.contains(&self) || self == Self::EpubReflow

// taiyaku_library::derive
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed { pub root_id: i64, pub rel_path: String, pub format: Option<String>, pub pages: Option<u32>, pub sha256: Option<String> }
pub fn claimed_units(conn: &Connection) -> Result<Vec<Claimed>, String>   // every root: an admitted entry that claims its subtree and is in no other claim

// taiyaku_library::fingerprints
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Outcome { Whole, Partial, Unreadable }   // as_str: "whole" | "partial" | "unreadable"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row { pub sha256: String, pub version: u32, pub outcome: Outcome, pub reason: Option<String>, pub build: Option<String>, pub pages: u32, pub prints: Vec<u8> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit { pub root_id: i64, pub rel_path: String, pub path: PathBuf, pub folder: bool, pub sha256: Option<String>, pub size: i64, pub mtime: i64 }
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage { pub roots: HashSet<i64>, pub libraries: Vec<(i64, String)> }   // libraries: matching on, by name
pub fn coverage(conn: &Connection) -> Result<Coverage, String>;
pub fn due(conn: &Connection, version: u32, build: Option<&str>) -> Result<Vec<Unit>, String>;
pub fn still_due(conn: &Connection, unit: &Unit, version: u32, build: Option<&str>) -> Result<bool, String>;  // covered now, and its digest (if known) due
pub fn stamp_of(path: &Path) -> Option<(i64, i64)>;     // (size, mtime) as scan_entry records them; None if it cannot be read
pub fn record_digest(conn: &Connection, unit: &Unit, sha256: &str) -> Result<bool, String>;  // false: already set, size/mtime moved, or its root is paused (one statement)
pub fn store(conn: &Connection, row: &Row) -> Result<(), String>;   // upsert, one statement
pub fn read(conn: &Connection, sha256: &str) -> Result<Option<Row>, String>;
pub fn drop_unclaimed(conn: &Connection) -> Result<usize, String>;  // rows no claimed unit's digest holds, any root
```

The due rule is "A unit is due" in "Shapes". `record_digest` writes only where `sha256 IS NULL` and `size` and `mtime` still match, since the pass runs beside scans; the derive's own write-back (`derive.rs:1643`) runs under the job lock and needs neither. `drop_unclaimed` keeps a row a claimed unit holds in a root no library with matching on reaches, so turning a switch off keeps what was stored (Design 8), and drops a row whose digest only an unclaimed entry still holds, which is what a rule edit that moves a claim leaves (Design 2).

- [ ] **Step 1: Write the failing tests.** Fixtures are directories of `cbz_pages` archives and page folders walked by `scan::scan_root` and derived, as `derive/tests.rs`'s neighbours build them.
  - `paged_formats_are_the_readable_ones_less_reflowable_books` (format tests): `Dir` and `Zip` in `PAGED`; `EpubReflow` readable and not paged; `Rar` neither.
  - `claimed_units_are_the_entries_the_derive_claims`: a volume folder with two chapter subfolders gives the volume alone; a chapter-only directory gives each chapter file; a series folder and the root give nothing (each asserted). Mutation: drop the `nested_in_claim` filter (the subfolders appear).
  - `due_takes_paged_units_of_covered_roots`, one assertion per entry: a cbz volume, a chapter cbz with no digest and a folder volume are due; an EPUB with text, a folder with no page and a RAR are not. Mutation: drop the `PAGED` filter (the EPUB appears).
  - `a_directory_is_covered_when_any_library_with_it_matches`: a directory in libraries A (on) and B (off) is covered; with A off too it is not; a directory nested in A's and reached by A (`reach`) is covered though no library with matching on links it. Mutations: `all` for `any`; `root.libraries` for `reach::of_libraries`.
  - `a_paused_directory_has_nothing_due`: `scan::pause` empties its units; `unpause` restores them.
  - `stored_rows_decide_what_is_due`, per case: a `whole` row at `VERSION` is not due under its own build or another; a row at an older version is due; a `partial` and an `unreadable` row stamped `"a"` are not due under `Some("a")` and are under `Some("b")`; a `partial` row stamped `None` is not due under `None` and is under `Some("b")`. Mutation: `build <> ?` for `build IS NOT ?` (the `None` case).
  - `twins_are_one_unit_and_the_newest_comes_first`: two entries with one digest are one unit; with `first_seen` set apart by SQL, the newer entry comes first. Mutations: no dedupe; ascending order.
  - `still_due_sees_a_switch_and_a_stored_twin`: true for a fresh unit; false once its root's library is off; false once a row for its digest is stored.
  - `a_digest_is_written_back_only_where_empty_and_unchanged`: `record_digest` writes and answers true; on an entry whose `sha256` is set it answers false and leaves it; on one whose `size` or `mtime` no longer matches it answers false. Mutations: drop `sha256 IS NULL`; drop the stamp condition.
  - `stamp_of_matches_what_the_walk_recorded`: for a scanned file and a scanned folder, `stamp_of` equals the entry's `(size, mtime)`; after the file is rewritten with other bytes it differs.
  - `store_replaces_an_older_row`, and `read` gives back every column.
  - `rows_no_claim_holds_are_dropped`: a row whose digest no entry holds goes; a row whose digest only a nested, unclaimed chapter folder holds goes; a row a claimed unit holds in a switched-off library's directory stays (each asserted). Mutation: keep every digest `scan_entry` holds (the nested case stays).
- [ ] **Step 2: Run** `cargo test -p taiyaku-read --all-features -- --test-threads=8 format::` and `cargo test -p taiyaku-library --all-features -- --test-threads=8 fingerprints:: derive::tests::claimed`. Expected: FAIL.
- [ ] **Step 3: Implement.** `claimed_units` has `unattached_chapter_dirs`'s loop shape, keeping an admitted entry when it `claims_subtree` and is not `nested_in_claim`. `due` reads `size`, `mtime` and `first_seen` from `scan_entry` for the covered roots and joins them in memory. `drop_unclaimed` deletes against a temp table of the claimed digests, in one transaction.
- [ ] **Step 4: Run** Step 2's commands, then `cargo test -p taiyaku-library --all-features -- --test-threads=8 derive::` (derive's behaviour unchanged). Expected: PASS.
- [ ] **Step 5: Commit**: "List the units a fingerprint pass is due to read, by the derive's own claims".

### Task 5a: The reader's unread entries, the build stamp, thread priority and the large-page lock

Lane D; no dependency. Split from Task 5 so the read itself is a task of its own; these four helpers need nothing from Tasks 1–4.

**Files:**
- Modify: `crates/taiyaku-read/src/pages.rs` (beside `member_entry`, `:848`) and the re-export at `lib.rs:22-26`: `member_unread_entries`
- Create: `crates/taiyaku-serve/src/fingerprint_pass.rs` (module doc and `mod read; mod priority;` for now), `fingerprint_pass/read.rs` (`LargePages` only), `fingerprint_pass/read/tests.rs`, `fingerprint_pass/priority.rs`, `fingerprint_pass/priority/tests.rs`; `mod fingerprint_pass;` in `lib.rs`
- Modify: `crates/taiyaku-serve/src/about.rs` (beside `build_label`), `about/tests.rs`
- Modify: `.config/nextest.toml` (the `success-output = "final"` filter, `:118`): the priority test joins it, since it can skip itself
- Test: `crates/taiyaku-read/src/pages/tests.rs`

**Interfaces (produces):**

```rust
// taiyaku_read
pub fn member_unread_entries(path: &Path) -> Result<Vec<String>, String>;
// every file entry that is neither a page nor a companion: ComicInfo.xml (any case), __MACOSX/ entries and
// dot-files are companions, and so is any entry of a non-image type (txt nfo url sfv md5 sha1 xml json htm html md rtf ini db ds_store, any case, any depth: ComicInfo.xml, Thumbs.db, desktop.ini, credits files), ruled in Task 5a so an ordinary scanlation archive is not partial and re-read every deploy; a .jxl or .heic page, an unknown type, an encrypted or undecodable-method entry are not

// taiyaku_serve::about
pub(crate) fn build_stamp(label: &BuildLabel) -> Option<String>;   // commit, "<commit>-dirty" when marked, None without one
pub(crate) fn this_build() -> Option<String>;                      // build_stamp of this binary's label

// taiyaku_serve::fingerprint_pass::priority
pub(crate) fn lower() -> Result<(), String>;   // this thread: renice --priority 19, ionice -c 3, by its id from /proc/thread-self

// taiyaku_serve::fingerprint_pass::read
pub(crate) struct LargePages { /* threshold, Mutex<()> */ }
impl LargePages {
    pub(crate) fn new(threshold: usize) -> Self;
    pub(crate) fn run<T>(&self, size: usize, f: impl FnOnce() -> T) -> T;   // f under the lock when size >= threshold
}
```

- [ ] **Step 1: Write the failing tests.**
  - `unread_entries_are_what_the_reader_neither_reads_nor_knows` (taiyaku-read): an archive of `001.png`, `ComicInfo.xml`, `__MACOSX/._001.png`, `.DS_Store`, `002.jxl`, `notes.txt` lists `002.jxl` and `notes.txt` and nothing else (each asserted; as built after the Task 5a ruling, `notes.txt` is a companion and the unread list is `002.jxl`, `003.heic`, `extra.xyz`); a folder of the same lists the same; `comicinfo.XML` is a companion too. Mutation: drop the ComicInfo arm.
  - `build_stamp_names_the_commit_and_its_mark`: a clean label gives the hash, a marked one `"<hash>-dirty"`, none gives `None`. Mutation: ignore `dirty`.
  - `lower_renices_and_idles_this_thread_alone`: in a spawned thread, `lower()`; then field 19 of `/proc/thread-self/stat` (the parenthesised name is field 2) reads `19`, and `ionice -p <tid>` prints `idle`; the test's own thread keeps its niceness. Mutation: the process id for the thread id. Where `renice` or `ionice` is not on `PATH`, it prints `NOT MEASURED:` to stderr and returns.
  - `large_pages_run_one_at_a_time` (Review Focus 3): four threads share one `LargePages::new(1000)`; at `run(1000, …)` each closure records how many are inside at once (an atomic counter, a short sleep): the maximum is 1; at `run(999, …)` each closure waits until all four are inside (giving up after 2 s) and all four see four. Mutations: `>` for `>=`; the lock taken whatever the size.
- [ ] **Step 2: Run** `cargo test -p taiyaku-read --all-features -- --test-threads=8 pages::` and `cargo test -p taiyaku-serve --all-features -- --test-threads=8 fingerprint_pass:: about::`. Expected: FAIL.
- [ ] **Step 3: Implement.** `member_unread_entries` is the archive's or folder's file entries less what `page_names_from` / `directory_page_paths` already list and less the companions: it reuses the listing's own skips (`pages.rs:294-297`, the `__MACOSX` and dot-file skip; `zip_can_read`, `pages.rs:312-320`) rather than restating them. Until Task 6 calls them, they are used by tests alone: mark each such item `#[cfg_attr(not(test), expect(dead_code, reason = "…"))]` with the reason that the fingerprint pass calls it, as `crates/taiyaku-read/src/cbz.rs:29-35` does, so clippy's `-D warnings` passes; Task 6 removes the marks.
- [ ] **Step 4: Run** Step 2's commands. Expected: PASS. Then `docker run --rm debian:bookworm-slim sh -c 'command -v renice && command -v ionice'` prints both paths; where docker is not available, say so in the task report and the PR body.
- [ ] **Step 5: Commit**: "The reader lists the entries it does not read, the server names its build, and a thread can lower its own priority".

### Task 5: Reading one unit

Depends on Tasks 2, 4 and 5a.

**Files:**
- Modify: `crates/taiyaku-serve/src/fingerprint_pass/read.rs` (beside `LargePages`), `fingerprint_pass/read/tests.rs`

**Interfaces:** consumes Task 5a's `member_unread_entries`, `this_build` and `LargePages`; produces:

```rust
// taiyaku_serve::fingerprint_pass::read
#[derive(Debug)] pub(crate) enum Read { Stored(fingerprints::Row), Gone, Stopped }
pub(crate) fn read_unit(unit: &fingerprints::Unit, digest: &str, build: Option<&str>, large: &LargePages,
                        between: &mut dyn FnMut(Phase, usize) -> bool) -> Read;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub(crate) enum Phase { Sample, Walk }
// between(phase, i) is asked before page i of the border sample (Phase::Sample) and of the walk (Phase::Walk); false stops the read (Stopped, nothing stored). A test that acts "at page k" acts at (Phase::Walk, k) unless it says otherwise.
```

**`read_unit`, in order:**
1. `stamp_of(path)` must equal `(unit.size, unit.mtime)`, else `Gone`.
2. `member_page_names` and `member_unread_entries`; an error from either is `Unreadable` with its words, if step 6 holds.
3. No page: `Unreadable`, the reason naming the unread entries if any.
4. Border: for each `i` in `border_sample(n)`, `member_page(path, i)`, decoded through `large`, `flat_border`, dropped; a page that fails is skipped. `median_border` of what decoded.
5. Walk: `for_each_member_page`; each page decoded through `large` and described with `describe_page`, or kept as `None`; a folder's page bytes feed a running SHA-256. A read error stops the walk: that page and every later one are `None` and the error is the reason.
6. `stamp_of` again must equal step 1's, and a folder's running digest must equal `digest` (the bytes `member_digest` hashes, `pages.rs:559`), else `Gone`.
7. Outcome: every page hashed and no unread entry is `Whole`; some page hashed is `Partial`; none is `Unreadable`. The row carries `VERSION`, `build`, the page count and the `encode`d prints.

- [ ] **Step 1: Write the failing tests.**
  - `a_streamed_cbz_matches_the_pinned_fixture` and `a_streamed_folder_matches_the_pinned_fixture`: `read_unit` over `print_fixture_cbz` / `print_fixture_folder` (the unit from a scan of the temp dir) is `Stored`, `Whole`, 26 pages, and each decoded record equals `PRINT_FIXTURE[i]` separately.
  - `a_page_that_does_not_decode_keeps_its_place`: the fixture with `p05.png`'s bytes replaced by garbage (a page the reader lists and the decoder refuses, as an AVIF page is today) is `Partial`, record 5 is `None`, every other record equals `PRINT_FIXTURE`. With `p04.png` garbage instead (a sampled page) the same holds for record 4: a sample that fails is skipped, as PR 1's `common_border` skips it. Mutations: fail the file on its first bad page; fail it on a bad sample.
  - `an_unread_entry_makes_a_file_partial`: the fixture plus `p26.jxl` is `Partial`, its 26 records unchanged, the reason naming `p26.jxl`; a cbz of `.jxl` entries only is `Unreadable` with 0 pages.
  - `a_file_that_is_not_an_archive_is_unreadable`: a `.cbz` of random bytes is `Unreadable` with the reader's words.
  - `a_file_rewritten_during_its_read_stores_nothing`: `between` rewrites the archive (other bytes, a later mtime) at page 3: `Gone`. Mutation: drop step 6's stamp check.
  - `a_folder_whose_page_changes_during_its_read_stores_nothing`: `between` rewrites `p10.png` at page 3 with other bytes of the same length, leaving the folder's own stamp as it was: `Gone`. Mutation: drop the running-digest check.
  - `a_file_gone_before_or_during_its_read_stores_nothing` (Review Focus 2): the unit's file removed after the scan: `Gone`; removed by `between` at page 3: `Gone`.
  - `a_stop_between_pages_stores_nothing`: `between` answers false at page 7: `Stopped`.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 fingerprint_pass::`. Expected: FAIL.
- [ ] **Step 3: Implement.** Until Task 6 calls it, it is used by tests alone: mark each such item `#[cfg_attr(not(test), expect(dead_code, reason = "…"))]` with the reason that the fingerprint pass calls it, as `crates/taiyaku-read/src/cbz.rs:29-35` does, so clippy's `-D warnings` passes; Task 6 removes the marks.
- [ ] **Step 4: Run** Step 2's command. Expected: PASS.
- [ ] **Step 5: Commit**: "Read one unit's pages a page at a time, as PR 1 hashed them, and store nothing a change during the read would make wrong".

### Task 6: The pass

Depends on Task 5.

**Files:**
- Modify: `crates/taiyaku-library/src/progress.rs`: `Step::Fingerprinting` (`as_str` `"fingerprinting"`), last
- Modify: `crates/taiyaku-serve/src/scan_jobs.rs`: `JobKind`, `Job.kind` and `Job::fingerprint` (`:40-101`); `Job::end` to `pub(crate)` (`:126`); `Job::set_count`; the `Running::at` arm (`:207`) takes `Step::Fingerprinting => None`; the kick after `reserved.end(..)` (`:1011-1016`)
- Modify: `crates/taiyaku-serve/src/fingerprint_pass.rs`; remove the `expect(dead_code)` marks Tasks 5a and 5 put on what this task now calls (`fingerprint_pass/read.rs`, `priority.rs`, `about.rs`)
- Modify: `crates/taiyaku-serve/src/lib.rs`: `ServeState.fingerprints: Arc<fingerprint_pass::Pass>` after `library_job` (`:189`), its doc saying why it is inert unless `serve` builds it live (no test's state starts a pass); `serve()`'s literal (`:546-558`) takes `Arc::new(Pass::live())`; `fingerprint_pass::at_startup(&state)` after `scheduler::spawn` inside the `is_live()` branch (`:659-662`)
- Modify: every other `ServeState { … }` literal (`rg -n 'library_job: ' crates` lists them with the struct itself): add `fingerprints: Default::default(),`
- Test: `crates/taiyaku-serve/src/fingerprint_pass/tests.rs`, `crates/taiyaku-serve/src/scan_jobs/tests.rs`

**Interfaces (produces):**

```rust
// scan_jobs
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub(crate) enum JobKind { Scan, Fingerprint }   // as_str: "scan" | "fingerprint"
impl Job {
    pub(crate) fn fingerprint(started: Instant) -> Self;   // kind Fingerprint, no dirs, step Fingerprinting, done 0, and a
                                                           // jobs_ended counter of its own, so the feed's ended counts scans
    pub(crate) fn set_count(&self, done: u64, of: u64);    // step Fingerprinting
    pub(crate) fn end(&self, ended: JobEnded);
}
// fingerprint_pass
pub struct Pass { /* settings: Option<PassSettings>, state, reader mark, last job */ }
impl Default for Pass;            // inert: kick starts nothing
impl std::fmt::Debug for Pass;    // ServeState derives Debug; the settings' hook is not printed
impl Pass {
    pub fn live() -> Self;
    pub(crate) fn with(settings: PassSettings) -> Self;
    pub(crate) fn job(&self) -> Option<Arc<Job>>;          // the running pass's job, else the last one's
    #[cfg(test)] pub(crate) fn settle(&self, within: Duration) -> bool;   // true once no pass runs
}
pub(crate) struct PassSettings {
    pub(crate) workers: usize, pub(crate) quiet: Duration, pub(crate) poll: Duration, pub(crate) large_page_bytes: usize,
    #[cfg(test)] pub(crate) after_page: Option<Arc<dyn Fn(&Path, usize) + Send + Sync>>,   // asked before each walked page (Phase::Walk only), so "read once" counts see each file once
}
impl Default for PassSettings;    // 4 workers (the setting PR 1 timed), 30 s, 1 s, fingerprint::LARGE_PAGE_BYTES, None
pub(crate) fn kick(state: &Arc<ServeState>);
pub(crate) fn at_startup(state: &Arc<ServeState>);
```

**How a pass runs.** `kick` on an idle live `Pass` marks it starting and spawns a named `std::thread`, the coordinator; on a starting or running one it sets `again` and wakes the coordinator. The coordinator lowers its own priority, takes `library.working()` only for each store step and never across a file read (an in-memory library's `working()` is the shared connection), and lists `fingerprints::due(VERSION, this_build())` less what this pass has tried. With nothing due it drops unclaimed rows and goes idle, and no job ever shows. Otherwise it makes `Job::fingerprint` (the feed shows it from then), fills `job.libraries` from `fingerprints::coverage`, sets `of`, and runs `workers` scoped threads over one queue. A worker lowers its priority, then per unit: stops if the job is cancelling; asks `still_due` and skips the unit if not; for an unknown digest checks the stamp, hashes with `taiyaku_read::member_digest` and `record_digest`s it, skipping if refused or if that digest is already stored; runs `read_unit` inside `catch_unwind` (a panic is `Unreadable` in `scan_jobs::panic_message`'s words); `store`s a `Stored` row; marks the unit tried; raises `done`. Every unit taken counts towards `done` whatever its outcome, so the count ends at `of`. When the queue is empty and the workers are done: with `again` set (read and cleared under the state lock) it lists again and goes on; otherwise it drops unclaimed rows, `end`s the job `Finished` (`Cancelled(Step::Fingerprinting)` if cancelling), keeps it as the last job, and goes idle under the same lock. A drop guard puts the state back to idle however the coordinator exits.

- [ ] **Step 1: Write the failing tests** (a temp library scanned and derived; a state built as `ServeState { fingerprints: Arc::new(Pass::with(settings)), ..test_support::state(root) }`; `settle` to wait):
  - `a_pass_fingerprints_every_due_unit_and_ends_counted`: three cbz volumes and one chapter cbz with no digest; `kick`; each unit's digest has a `whole` row (each asserted); the chapter entry's `sha256` is now set; the job ended `Finished` with `done == of == 4`.
  - `a_pass_with_nothing_due_shows_no_job`: every unit stored; `kick`; `pass.job()` is `None`. Mutation: make the job before listing.
  - `a_cancelled_pass_resumes_without_reading_what_it_stored`: one worker; `after_page` cancels the job at the second unit's page 0; the job ends `Cancelled(Fingerprinting)` with the first unit stored and the second not; a second `kick` reads the second unit and never the first (`after_page` records paths). Mutation: `due` ignoring stored rows.
  - `a_finished_scan_starts_a_pass_and_a_cancelled_one_does_not` (scan_jobs tests, driving `spawn_job` as their neighbours do): a job that returns its reports leaves a stored row once settled; a job that cancels leaves none. Mutations: kick on every end; no kick.
  - `at_startup_starts_a_pass_with_every_directory_off`: every root's interval Off; a `Scheduler::tick` starts nothing; `at_startup` stores every due unit. Mutation: an empty `at_startup`.
  - `every_pass_ends_by_dropping_rows_no_claim_holds`: a planted row for a digest no entry holds is gone after a `kick` that found nothing due. Mutation: drop only when something was due.
  - `a_panic_in_one_file_ends_nothing` (Review Focus 1): `after_page` panics on the first unit; the second is stored `whole`, the first `unreadable` with the panic's words in `reason`; afterwards a `kick` starts a pass with a new job id. Mutation: no `catch_unwind` (the state stays running and the second `kick` does nothing).
  - `a_worker_runs_niced_and_idle`: `after_page` reads its own thread's niceness and `ionice -p`: 19 and idle. Mutation: drop the worker's `lower()`. It skips as Task 5's priority test does, and joins the same nextest filter.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 fingerprint_pass:: scan_jobs::`. Expected: FAIL.
- [ ] **Step 3: Implement**, add `fingerprints: Default::default(),` to every literal, and build `cargo build -p taiyaku-serve -p taiyaku-cli --all-targets --all-features`.
- [ ] **Step 4: Run** Step 2's command, then `cargo test -p taiyaku-serve --all-features -- --test-threads=8` and `cargo test -p taiyaku-library --all-features -- --test-threads=8`. Expected: PASS.
- [ ] **Step 5: Commit**: "A fingerprint pass, kicked when a scan ends and at startup, that resumes where it stopped".

### Task 7: The pass waits for jobs and readers

Depends on Task 6.

**Files:**
- Modify: `crates/taiyaku-serve/src/fingerprint_pass.rs`: the wait before each unit and before each write; `reader_seen`; `waits`
- Modify: the eight handlers in "Choices", one `state.fingerprints.reader_seen();` each, first thing once the state is in hand
- Test: `crates/taiyaku-serve/src/fingerprint_pass/tests.rs`

**Interfaces (produces):**

```rust
impl Pass {
    pub(crate) fn reader_seen(&self);              // a page or book was served now
    fn mark_reader(&self, at: Instant);            // reader_seen's body; tests hand it their own instant
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub(crate) enum Wait { JobLock, Reader }
pub(crate) fn waits(pass: &Pass, job_lock_held: bool, now: Instant) -> Option<Wait>;
// JobLock while held; else Reader while now - last mark < quiet; else None
```

The job lock is read with `LibraryJob::is_held`, which says only that something holds it (`library_job.rs:26-35`), so the pass waits for a rule save, stamp, Move or Remove as it does for a scan. The reader mark is an `AtomicU64` of milliseconds since the `Pass`'s own epoch, so a page request pays one store. A worker waits before it takes each unit while `waits` is `Some`, and before each `store` while the job lock is held (Review Focus 5), polling every `poll` and leaving if the job is cancelling.

- [ ] **Step 1: Write the failing tests.**
  - `waits_names_the_job_lock_then_a_reader_then_nothing`: held gives `JobLock` whatever the mark; not held, with a mark at `t`, gives `Reader` at `t + quiet - 1 ms` and `None` at `t + quiet`; never marked gives `None`. Mutation: `<=` for `<`.
  - `no_file_starts_while_the_job_lock_is_held`: hold `state.library_job.lock()`; `kick`; after three `poll`s no row exists and `after_page` has never been called; drop the guard; once settled every unit is stored. Mutation: skip the wait.
  - `no_file_starts_within_the_quiet_after_a_page`: `reader_seen()`, then `kick` with `quiet` 300 ms: no `after_page` call in the first 200 ms; every unit stored once settled. Mutation: skip the reader half of `waits`.
  - `a_row_waits_for_the_job_lock`: `after_page` at the first unit's last page signals the test and blocks until the test holds the job lock (a channel each way, so the worker cannot store first); no row exists while it holds it; once released the row appears. Mutation: skip the wait before `store`.
  - `each_reading_route_marks_a_reader`: table-driven over the eight routes (`GET /api/works/{work}/{edition}/pages/0`, `…/pages/0/thumb`, `/api/works/{work}/book`, `…/{edition}/book/images/0`, `…/{edition}/download`, `…/{edition}/members/0/download`, `/opds/v1.2/works/{work}/{edition}/pages/0`, `…/{edition}/cover`) on `test_support`'s page and book fixtures: per route, the mark moved, with the route in the message; `GET /api/library/series` leaves it alone. Mutation: remove one handler's call (that route's case fails).
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 fingerprint_pass::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it, and `cargo test -p taiyaku-serve --all-features -- --test-threads=8 routes:: book:: download:: opds::`. Expected: PASS.
- [ ] **Step 5: Commit**: "The fingerprint pass waits while a job holds the lock or a reader was served in the last 30 s".

### Task 8: What the pass rechecks per file

Depends on Task 7.

**Files:**
- Modify: `crates/taiyaku-serve/src/fingerprint_pass.rs`
- Test: `crates/taiyaku-serve/src/fingerprint_pass/tests.rs`

A kick while a pass runs makes the coordinator list again at once and put the new units at the front of the queue, so a volume a scan adds mid-pass is read next (Design 2: newest first), not after the backlog. A worker refreshes `job.libraries` from `coverage` each time it asks `still_due`. A failed `store` or `record_digest` is one stderr line (`writeln!(std::io::stderr(), …)`, never `eprintln!`); the unit stays due and the worker goes on.

- [ ] **Step 1: Write the failing tests** (one worker unless said):
  - `turning_a_library_off_stops_its_files_mid_pass`: libraries A and B, two units each, A's newest; `after_page` at A's first unit turns B off (`set_match_by_content`); B's units are never read, A's second is; the job's `libraries` ends without B. A directory in both A and B (B off) is still read. Mutation: coverage read only when listing.
  - `a_directory_paused_mid_pass_gets_no_new_files`: as above, with `scan::pause` on B's directory.
  - `a_scan_ending_mid_pass_puts_its_new_files_first`: three units; `after_page` blocks at the first while the test adds a cbz, scans, derives and `kick`s; once released the read order is the first unit, the new file, then the other two (each position asserted). Mutation: append at the back.
  - `twins_are_read_once`: one archive's bytes under two directories: one read (`after_page` sees one path at page 0), and the shared digest has a row.
  - `a_file_that_changes_on_every_read_is_tried_once_per_pass` (Review Focus 4): `after_page` rewrites the file at page 0 each time it is read, and a second `kick` lands mid-pass; the pass ends within `settle(5 s)`, that file was read once and has no row, and every other unit was stored. Mutation: forget tried units (the pass never ends).
  - `a_refused_write_leaves_its_file_due_and_the_pass_goes_on` (Review Focus 5): on the in-memory library's shared connection, `CREATE TEMP TRIGGER … BEFORE INSERT ON fingerprint WHEN NEW.sha256 = '<x>' BEGIN SELECT RAISE(ABORT, 'refused'); END`; the pass ends `Finished`; `x` has no row and is still in `due`; every other unit has one. Mutation: `?` on `store`'s error.
  - `four_workers_store_every_unit_once`: eight units, the default four workers: eight rows, `done == of == 8`, and no path read twice.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 fingerprint_pass::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it again. Expected: PASS.
- [ ] **Step 5: Commit**: "The pass rechecks the switch, the pause and the queue per file, and survives a file it cannot store".

### Task 9: The pass in the feed, its Cancel, and the lock

Depends on Task 8.

**Files:**
- Modify: `crates/taiyaku-serve/src/libraries_api.rs`'s `patch_library`: a PATCH that turns `match_by_content` on calls `fingerprint_pass::kick` after it commits, so switching a library on starts its files now rather than at the next scan
- Modify: `crates/taiyaku-serve/src/scan_feed.rs`: `held` skips the pass (`:44-50`); `running_jobs` (`:121`) includes `state.fingerprints.job()` only while that job has not ended, and `job_named` (`:134`) includes it ended or not, so Cancel can say how it ended; the `ended` doc (`:22-29`) says it moves when a *scan* job ends; `job_json` (`:156`) writes `kind`; the module and `feed` docs say a pass is a job of its own kind and holds no library
- Test: `crates/taiyaku-serve/src/scan_feed/tests.rs`, `crates/taiyaku-serve/src/fingerprint_pass/tests.rs`, `crates/taiyaku-serve/src/libraries_api/tests.rs`

- [ ] **Step 1: Write the failing tests** (a pass held inside its first unit by a blocking `after_page`):
  - `an_admin_sees_the_pass_as_a_job_of_its_own_kind`: the admin's answer lists the pass with `kind "fingerprint"`, `step "fingerprinting"`, `done 0`, `of` the due count, `dir null`, `dirs []`, `libraries` the libraries with matching on, `cancelling false` (each field asserted); a scan job started beside it with `spawn_job` reads `kind "scan"`; the answer's held `libraries` names the scan's library alone. Mutation: write no `kind`.
  - `a_reader_is_sent_nothing_of_the_pass`: a reader's answer has `libraries: []` and no `jobs`. Mutation: let `held` take the pass.
  - `ended_stays_put_when_a_pass_ends`: `ended` read before the pass, the pass run to its end, `ended` unchanged. Mutation: give `Job::fingerprint` the server's counter.
  - `cancel_stops_the_pass_and_then_says_how_it_ended`: `POST /api/library/scans/{pass}/cancel` is 202 `{"job", "cancelling": true}`; released and settled, the same POST is 200 `{"job", "ended": "cancelled", "step": "fingerprinting"}`; a later `kick` starts a pass with another id that reads what was not stored. Mutation: `job_named` without the pass (404).
  - `an_ended_pass_leaves_the_feed`: once a pass has ended, the admin's answer lists no job of kind `fingerprint`, while `POST …/cancel` on its id still answers 200 with how it ended. Mutation: `running_jobs` taking `pass.job()` whether or not it has ended (the web would show the line and Cancel for good).
  - `switching_a_library_on_starts_a_pass` (in `libraries_api/tests.rs`): a library switched off, its units unstored; `PATCH {"match_by_content": true}`; once settled each unit has a row (each asserted); a PATCH that only renames, or turns it off, starts nothing (`pass.job()` stays `None`). Mutation: no kick.
  - `rules_stamp_move_and_remove_go_through_while_a_pass_reads`: while the pass is held inside a file, a rules save (`PUT /api/library/rules`), a stamp (`POST /api/library/works/{slug}/stamp-comicinfo`), a Move (`POST /api/library/roots/{id}/move`) and a removal from a library (`DELETE /api/library/libraries/{id}/dirs/{dir}`) each answer 200, each asserted, set up as `config_api`, `stamp` and `libraries_api`'s own tests set them up. Mutation: hold the job lock across a file (each answers 409).
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 scan_feed:: fingerprint_pass:: libraries_api::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it, then `cargo test -p taiyaku-serve --all-features -- --test-threads=8`. Expected: PASS.
- [ ] **Step 5: Commit**: "The fingerprint pass is its own kind of job in the scan feed, an admin can cancel it, and it holds nothing".

### Task 10: The feed client tells a pass from a scan

Lane C; depends only on "Shapes".

**Files:**
- Modify: `web/src/scanFeed.ts`: `ScanStep` (`:34-40`) gains `'fingerprinting'`; `FeedJob` (`:43-61`) gains `kind: 'scan' | 'fingerprint'`; `scanEnded` (`:158-166`) compares scan jobs only, and `ended`'s doc (`:65`) says it moves when a *scan* job ends (a pass's end leaves it, closed row O312's marker); `jobsHolding` (`:246`) and `jobHoldingDir` (`:256`) read scan jobs only; new `scanJobs` and `fingerprintJob`; `STEP_WORDS` (`:288`) `fingerprinting: 'Fingerprinting pages'`; `stepLine` (`:313-336`) a `'fingerprinting'` case
- Modify: `web/src/LibraryPage.tsx:635` and `web/src/DirPage.tsx:116-119`: `feedShowsJob` and `sibling` read `scanJobs(feed)`, since a pass holds no removal and its end may be a day away, so the removal hold's own poll must not stop for it
- Modify: every test fixture and test helper `tsc` flags once `FeedJob.kind` is required (`web/tsconfig.app.json:25` typechecks the tests; about 23 job fixtures in 12 files): add `kind: 'scan'`. The field stays required.
- Test: `web/tests/scanFeed.test.tsx`, `web/tests/LibraryPage.test.tsx`, `web/tests/DirPage.test.tsx`

**Interfaces (produces):**

```ts
export function scanJobs(feed: ScanFeed | undefined): FeedJob[]   // kind 'scan'
export function fingerprintJob(feed: ScanFeed | undefined, libraryId?: number): FeedJob | undefined
// the pass; given libraryId, only while the pass covers that library
```

The poll keeps its cadence: the pass is not in the held `libraries`, so the feed is asked every `FEED_IDLE_MS` while only a pass runs, which is enough for a count that runs for hours. `stepLine` for a pass reads `Fingerprinting pages · 120 of 2,097 files` (`file` when `of` is 1; `done` alone while `of` is `null`). The owner tries the words in Task 12.

- [ ] **Step 1: Write the failing tests.**
  - scanFeed: `stepLine` of a pass at `done 120, of 2097` is exactly `'Fingerprinting pages · 120 of 2,097 files'`, and at `of 1` ends `'1 file'`; `jobsHolding(feed, 1)` with a pass covering library 1 and no scan is `[]`; `fingerprintJob(feed, 1)` is the pass and `fingerprintJob(feed, 2)` is `undefined` when it covers 1 alone; a pass leaving the feed between two answers calls no `onScanEnd` listener and invalidates no read, while a scan leaving does both. Mutations: an unfiltered `jobsHolding`; `scanEnded` over every job.
  - LibraryPage: with only a pass covering the library, the page shows **Scan library**, not Cancel, and a removal hold's status is still read again each `SCAN_POLL_MS` (driven as the existing removal-hold test drives it). Mutation: `feedShowsJob` over every job.
  - DirPage: with only a pass running, the directory's status poll is not stopped. Same mutation.
- [ ] **Step 2: Run** from `web/`: `./node_modules/.bin/vitest run --reporter=default tests/scanFeed.test.tsx tests/LibraryPage.test.tsx tests/DirPage.test.tsx`. Expected: FAIL.
- [ ] **Step 3: Implement.** `tsc` finds every exhaustive use of `ScanStep` (`STEP_WORDS`'s `Record`; `ScanReport.tsx:476` needs nothing).
- [ ] **Step 4: Run** Step 2's command, the node and jsdom projects whole (`./node_modules/.bin/vitest run --reporter=default --project node --project jsdom`; the browser project is Task 12's), `pnpm -C web run lint` and `typecheck`. Expected: PASS.
- [ ] **Step 5: Commit**: "The scan feed client tells a fingerprint pass from a scan, and nothing a scan holds waits on a pass".

### Task 11: The pass line, its Cancel, and the switch

Depends on Task 10.

**Files:**
- Modify: `web/src/libraries.ts`: `Library.match_by_content: boolean` (`:19-28`); `renameLibrary`'s answer type (`:367`); new `setMatchByContent`
- Modify: `web/src/LibraryTable.tsx`: above the table (after `{notices}`, `:77`), while `fingerprintJob(feed)` is defined, a `<p className="scanning">` with `stepLine(job)` (`Cancelling…` while `cancelling`) and a **Cancel** button sending `cancelScan(job.id)`, then `askScanFeed()`
- Modify: `web/src/LibraryPage.tsx`: the same line and Cancel after the scan lines (`:1182-1192`) while `fingerprintJob(feed, library.id)` is defined; a checkbox **Match editions by page content** in the page's settings beside the rename form (`onSubmit={rename}`, about `:1072-1165`; the handler is `:965-1000`), checked from `library.match_by_content`, sending `setMatchByContent`, held while its send is out, a refusal shown as `role="alert"` with the box set back, success calling `onChanged()`; one line under it: "Fingerprints this library's pages in the background, so later versions can match editions by what their pages show."
- Modify: every test fixture and test helper `tsc` flags once `Library.match_by_content` is required (about 77 objects in 19 files): add `match_by_content: true`. The field stays required.
- Modify: `web/src/ProposalRow.tsx`: the `not-yet` line (`:271-274`) and the doc comment (`:29-30`) give as the reason that the fingerprinting code sits in `taiyaku-cli`, which Task 2 makes false. The line reads "Not wired up yet: comparing pages comes with matching editions by content, which is not built yet." and the comment says the same reason. A test in `web/tests/ProposalRow*` (whichever holds the panel's tests) asserts the line's text exactly.
- Test: `web/tests/Libraries.test.tsx`, `web/tests/LibraryPage.test.tsx`, `web/tests/libraries.test.ts`

**Interfaces (produces):**

```ts
export const setMatchByContent = (id: number, on: boolean) => …
// PATCH /api/library/libraries/{id} {match_by_content: on}; answers {id, name, match_by_content}
```

- [ ] **Step 1: Write the failing tests.**
  - Libraries list: with a pass in the feed, the line's text is exactly `'Fingerprinting pages · 120 of 2,097 files'`; Cancel sends `POST /api/library/scans/9/cancel` once; each row's Last scan cell still reads its last scan, not "Scanning". Mutation: the row's cell from every job.
  - LibraryPage: the line and Cancel show on library 1's page when the pass covers 1, and not on library 2's (each asserted); the box is checked for `match_by_content: true` and not for `false`; unchecking sends exactly `PATCH {"match_by_content": false}`; a 500 leaves the box checked with the alert's words. Mutations: `fingerprintJob` without `libraryId`; the box set from the click rather than the answer.
  - libraries: `setMatchByContent(3, true)` sends exactly that method, path and body.
- [ ] **Step 2: Run** `./node_modules/.bin/vitest run --reporter=default tests/Libraries.test.tsx tests/LibraryPage.test.tsx tests/libraries.test.ts`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** Step 2's command, the node and jsdom projects whole (`--project node --project jsdom`), lint and typecheck. Expected: PASS.
- [ ] **Step 5: Commit**: "Admins see the fingerprint pass on the Libraries list and each library's page, can cancel it, and can switch matching off per library".

### Task 12: Docs, the whole gate, mutants and the owner's hand-check

Depends on every task, with all three lanes merged.

**Files:**
- Modify: `docs/status.md`: O338's row gains one italic "*Narrowed on <date>, and left open.*" sentence: PR 2 stores a fingerprint of every page of each library with matching on, in a background pass; PR 3's lookup is next. The test count, at both sites the gate names. A row from O342 only for a deferral this branch made (the candidate found while planning: a download marks a reader only when it starts, so a download longer than 30 s shares the disk with the pass after that).
- Modify: `docs/notes.md`: a section "Content matching PR 2: fingerprints in the server (<date>)" with the reasons from "Choices" the design does not hold (crate, build stamp, priority by subprocess, the drop rule) and the hand-check's results. No figure the claim check cannot re-derive.
- Modify: `docs/status.md`'s crate table (`:405-412`): the `taiyaku-serve` and `taiyaku-cli` rows, and any other line `rg -n 'fingerprint' docs/status.md README.md` shows saying the fingerprint code lives only in `taiyaku-cli`, now say it is in `taiyaku-serve`'s `fingerprint` and the CLI calls it there.
- Modify: `README.md`, the library section where Scan and Cancel are described (`:328`, `:566`): one paragraph on the switch, the pass's line in the feed, and what its Cancel does (stops this pass; the next scan carries on).

- [ ] **Step 1: The whole gate.** `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| <scratchpad>/gate.log 2>&1; echo "exit=$?" >> <scratchpad>/gate.log`, then `rg -a 'all green|FAIL|error|exit=' <scratchpad>/gate.log`. Expected: `exit=0` and `all green`, the browser project included (CI never runs it). A red browser run with import or connection errors and no failed assertion: read `/proc/loadavg` and `coredumpctl`, then rerun the browser project whole before suspecting the branch. Set the test count the gate reports; rerun until green.
- [ ] **Step 2: Mutants.** Commit everything first (it diffs commits), then `scripts/mutants.sh --since main >| <scratchpad>/mutants.log 2>&1`. Each survivor in this branch's code gets a test that kills it, or a line in the PR body saying why it cannot be killed. Check the unviable count against what changed before believing a clean run.
- [ ] **Step 3: The owner's hand-check**, on a scratch copy, never on `/home/davey/temp/manga-test` itself (a scan probes its root) and never through a hard-linked copy:
  1. `touch <scratchpad>/marker`; `cp -r` (no `-l`) three small series from `manga-test` into `<scratchpad>/library`: one of cbz volumes, one of folder volumes, one chapter-only directory.
  2. A fresh store at `<scratchpad>/library.db` with the account `local` / `pw` (`printf 'pw\npw\n' | <binary> library user password local --db <scratchpad>/library.db`, or `user promote` as README says for a fresh store); `cargo build --release -p taiyaku-cli`, the binary copied into the scratchpad; `pnpm -C web exec vite build --outDir <scratchpad>/dist --emptyOutDir`; `ss -lptn 'sport = :4399'` shows the port free; serve in the background with `--db <scratchpad>/library.db --cache <scratchpad>/cache --web <scratchpad>/dist --addr 127.0.0.1:4399 --insecure-cookies`.
  3. The owner, at `http://127.0.0.1:4399`: makes a library of `<scratchpad>/library`; when its first scan ends, sees "Fingerprinting pages · n of N files" on the Libraries list and on the library's page; presses Cancel there and sees the line go; presses Scan library and, once the scan ends, sees the pass carry on with fewer files due; unticks the switch and sees the pass stop starting files; ticks it, sets the directory's schedule Off, restarts the server, and sees the pass resume by itself. The owner also tries the step's and the checkbox's words.
  4. Evidence from the scratch store only: `sqlite3 <scratchpad>/library.db "SELECT outcome, count(*) FROM fingerprint GROUP BY outcome"` before and after the Cancel and after the resume.
  5. `find /home/davey/temp/manga-test -newer <scratchpad>/marker` prints nothing. Stop the server by the PID `ss` names; delete the scratch copy in its own call, one `rm -rf` naming its literal absolute path.
- [ ] **Step 4: Commit** the docs: "Content matching PR 2: notes, status and README for the fingerprint pass".

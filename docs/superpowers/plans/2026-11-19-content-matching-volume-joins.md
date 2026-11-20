# Content matching PR 3: volume joins — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A volume whose pages line up with another volume's at V ≥ 0.87 joins that volume's work as an edition at the next derive, with its alignment stored in whole pages and its readers' state carried, and an admin can Split it in a way every later derive remembers.

**Architecture:** O340's scorer and PR 1's lookup are copied into `taiyaku-serve::volume_match`; the CLI's frozen copy is not touched and a CI test holds the two equal. A `match_pass`, chained after the fingerprint pass, looks up each new file, scores its three leaders and stores pairs from 0.6 in migration 030's `content_match` table, then derives when a pair reached the bar. `derive` reads that table on the connection it is given, after `place`: it links works into components under a host, moves followers, writes `by = solver` alignments, and moves an edition's reader state when a join or a Split moves it; Split and Undo are admin routes that derive at once, and the work API carries the mark to the Series panel.

**Tech Stack:** Rust (rusqlite over the bundled SQLite 3.54.1, `std::thread`, serde_json), TypeScript/React (vitest node, jsdom and browser projects).

**Spec:** `docs/superpowers/specs/2026-11-19-content-matching-volume-joins-design.md` ("Spec §N"), with its parent `docs/superpowers/specs/2026-11-15-edition-matching-by-content-design.md`.

## Global Constraints

- **V is exactly O340's.** The shorter side goes first (equal lengths: the smaller key, a digest in the server, the path in Task 2's reproduction); `taiyaku_core::solve` runs over all slots; a page `spread_flags` marks over the whole file, with halves, gives two slots in reading order (rtl: right then left); V = slots in paired runs of at least `LONG_RUN = 4` over the larger slot count. The CLI's copy is not edited: `git diff main -- crates/taiyaku-cli/src/match_experiment/features.rs crates/taiyaku-cli/src/main.rs` prints nothing at every commit.
- **Bar 0.87, and a pair at exactly 0.87 joins** (`matches::BAR`, held equal to `volume_test/bar.json` by a CLI test). **Floor 0.6** (`matches::FLOOR`): pairs at or above it are stored; below it only the looked-up mark is; nothing in this PR shows a pair under the bar.
- **The lookup is PR 1's** (`match_experiment/lookup.rs:24-78`): a slot is informative when 8 to 248 of its 256 bits are set, inclusive; each informative slot of the query votes for the file holding its nearest informative slot by Hamming distance (a distance tie goes to the earlier indexed slot; the index is ordered by digest, then slot); the 3 files with the most votes are its candidates; a vote tie goes to the same series, then the same volume, then the smaller digest. Names never change a count. A query indexes only files sharing a library with matching on with it.
- **Who takes part** (Spec §1): the only member of its edition, in a work (group) with a volume, with a `whole` fingerprint row, in a directory (primary or a copy's) a library with matching on reaches; a pair needs one such library reaching both files. Pauses are ignored, so a pause never undoes a join.
- **Derive's invariants** (`derive.rs:3-26`): the same files, rules, hand values and match rows give the same library; a second derive with nothing changed commits nothing; no file is opened beyond the existing digest step; `content_match`, `fingerprint` and `library` are read on the connection derive is given, so `preview::preview` shows joins and keeps none.
- **Hand wins** (owner, 2026-11-19). The work that would move is held when a candidate's series came from a rule's literal `set_series` (`Resolved::provenance` names the rule), or when the work that holds its digest and was not placed by a join has `title_hand`, `volume_hand` or `series_id_hand` set. A rule that sets only a key, kind or language holds nothing; hand values on the host never stop a join.
- **Same-key pairs link nothing** and get no alignment; a pair names already put in one work joins nothing and gets an alignment, no mark and no Split.
- **Whole pages only.** No stored run names a half; derive writes no spread flags.
- **Nothing a reader saved is lost.** Positions, notes, truth and stamp aliases keyed (work, edition key) move with an edition, on a join and on a Split; a move that cannot carry one refuses, leaves both editions standing and is counted (as `fold::absorb` does).
- **An emptied series is counted, not deleted** (`DeriveReport::series_holding_no_work`).
- **Routes**: a new route goes in `guarded_routes` (`routes.rs:2401`), `grant_postures` (`:2753`), `narrowing_postures` (`:3269`) and both route lists in `auth/tests.rs` (the blocks at `:85-100` and `:3630-3655`), and refuses a reader through `session::administering_or_forbidden`.
- **Migration 030**, not 029: `029_entry_counted_by.sql` is on main (#264). `SCHEMA_VERSION` 30.
- **An item a later task first calls**, when it is not `pub`, carries `#[cfg_attr(not(test), expect(dead_code, reason = "Task N calls it"))]`, which Task N removes: its own task's tests call it, so a plain `expect` goes unfulfilled in the test target and fails clippy. A `pub` item of a `pub mod` (`volume_match`, `matches`) fires no `dead_code` and carries none. The items: Task 5's `match_pass::kick` (Task 6), Task 7's `joins::applicable` and `Candidate::held_by_rule` (Task 8), Task 8's `Joins::moved` (Task 12) and `Joins::aligned` (Task 10), Task 11's `fold::move_edition` (Task 12).
- **Comments hold the why** (`node scripts/check-comments.mjs`): no history or measurement; a comment may point at a notes section or an O-number. Set `scripts/comment-budget.json` to what the guard reports, per area.
- **Tests.** Assert per element (per work, edition, row, field), never one `toContain` over a page. Every fixture says what it separates. Every task hand-applies the mutations it names: apply, watch the named test FAIL, revert, `git diff --exit-code` on the file.
- **Rust gate (crates):** for each crate named, `cargo test -p <crate> --all-features -- --test-threads=8 >| <scratchpad>/<crate>.log 2>&1`, then `rg -a 'test result|panicked|error\[' <log>`: pass is every `test result: ok` with `0 failed`; then `cargo fmt --all -- --check`, `cargo clippy -p <crate> --all-targets --all-features -- -D warnings` and `node scripts/check-comments.mjs` exit 0.
- **Web gate:** copy `web/node_modules` from the main checkout with `cp -a` (never a symlink), then in `web/`: `./node_modules/.bin/vitest run --reporter=default --project node --project jsdom >| <scratchpad>/web.log 2>&1` (pass: a `Tests` line with no `failed`, and no `AssertionError` in the log), `pnpm -C web run lint`, `pnpm -C web run typecheck` exit 0. Browser tests run alone with `--project browser` and a file filter; read `/proc/loadavg` before believing a red one. Only Task 18 runs `scripts/check-all.sh`.
- **Shell:** zsh with noclobber (start a log with `>|`), absolute paths (`cd` is zoxide), never pipe a gate through `tail`. Scratch goes in the session scratchpad; evidence in `/home/davey/projects/taiyaku/.superpowers/sdd/content-volume-joins-evidence/`; a delete is its own call naming its literal absolute path.
- **O-numbers:** O338 and O340 are narrowed, not closed. A deferral gets a row from the first number after the highest on main at Task 18 (O390 at the time of writing).
- **Commits** say what they hold, and carry no AI attribution.

## Review Focus

1. **A triangle: official, fan and raw volumes all pairwise at the bar** (Oshi no Ko's shape). All three become one work; a Split on the raw must split every pair tying the raw to that work, or the remaining pair rejoins it and Split appears to do nothing. Expected: the raw leaves, the fan stays joined, the raw's panel lists both kept-apart pairs. Tests in Tasks 8 and 13.
2. **A joined file whose bytes change** (a re-download): its new digest has no fingerprint, so it takes no part until the next passes. Expected: the next derive puts it back where its name puts it with its reading position, leaves no copy of it on the host, and the passes rejoin it. Test in Task 12.
3. **A partial join**: the guest's work also holds an edition no pair touches, under a key the host already holds (an untouched edition under a new key moves with its work, Spec §4). Expected: the matched file moves with its reader state, the other edition stays on the old work, and no stale copy of the moved file remains there (`import` leaves an edition its document does not name standing, `import.rs:983-1004`). Test in Task 12.
4. **A derive cancelled between works after a join was written**: the guest's old work is not yet folded, so the file stands in two works. Expected: the next derive converges, and the one after commits nothing. Test in Task 8.
5. **Libraries and copies**: a pair whose files share no library with matching on, and a byte-identical copy of a file under a directory of another library (`Candidate::copies`). Expected: no join across libraries; a copy's directory counts towards reach; switching a library off undoes its joins at the next derive and keeps the scores. Tests in Tasks 7 and 12.

---

## Choices this plan settles

| Question | Choice | Where |
|---|---|---|
| Where the scorer lives | `taiyaku-serve::volume_match`, a copy: editing `match_experiment` would change the code the bar was frozen with (`volume_test/run.rs` `no_code_since_the_freeze`). CI holds the copies equal on fixtures; Task 2 holds the server's to all 12,246 O340 rows | 1, 2 |
| Which rule holds a file | Only a rule's literal `set_series` (owner, 2026-11-19). The `rule` table has no `set_volume` or `set_title`; a rule that captures from a path, seeded or written by hand, only reads names, which content exists to overrule, so editing a seed or re-importing rules changes nothing. No origin column and no seed detection | 7 |
| What moves | Whole groups merge into the host (Spec §4, "each component becomes one work"); an edition whose key the component already holds stays in its own group | 8 |
| Order | Edges by V descending, then same series first, then the older series, then digests; union-find (algorithm in Task 8) | 8 |
| Marker | `edition.joined_by`, the pair that brought the edition in: the Series mark's source, and how a later derive finds an edition a Split or an unjoin leaves on the host | 3, 8, 12 |
| Reader state on a whole-work join | The existing `superseded` + `fold::absorb`: the host's new edition holds the same digests, so `edition_map` (`fold.rs:309-325`) maps it. The new `fold::move_edition` covers what absorb cannot: a work keeping another edition, a Split, an unjoin | 11, 12 |
| Stored runs | The match row holds whole-page paired runs from `sha_a`'s pages to `sha_b`'s, the page counts and the solver floor. Derive orients them host to guest, fills gaps, and computes `verdict`, `slots`, `paired` and `avg_run` from them, so the alignment row agrees with its runs | 4, 5, 10 |
| Alignment direction | From the pair's edition that is the work's hub, else the higher-ranked kind (official, fan, raw), else the smaller key. No solver row where a `by = hand` row holds the pair | 10 |
| What Split splits | Every pair at the bar between the file and the files of its current work (Review Focus 1); Undo restores one pair | 13 |
| Followers | A group follows the series holding strictly more than any other of the joined files that share its directory and its own series before the join: a tally per directory and pre-join series, because a flat or mixed directory holds several series and one series' joins must not move another's volumes; held and volume-less works do not follow | 9 |
| Chain | The fingerprint coordinator kicks the match pass when it goes idle and its job, if any, was not cancelled | 6 |
| The pass's derive | Waits for the job lock and the lease as a scan does; runs to its end (a Cancel during it is honoured after); moves the feed's `ended` once | 6 |
| Due for lookup | No `match_lookup` row for the digest. The versions are recorded and not acted on (Spec non-goal: no re-scoring) | 3, 5 |

## Shapes shared across tasks

**Migration `030_content_matches.sql`** (first written here; each statement gets a why comment):

```sql
CREATE TABLE content_match (
  id          INTEGER PRIMARY KEY,
  sha_a       TEXT NOT NULL,
  sha_b       TEXT NOT NULL,
  v           REAL NOT NULL,
  long_paired INTEGER NOT NULL,
  a_slots     INTEGER NOT NULL,
  b_slots     INTEGER NOT NULL,
  a_pages     INTEGER NOT NULL,
  b_pages     INTEGER NOT NULL,
  floor       REAL,                       -- the calibration floor, where accepted
  runs        TEXT NOT NULL,              -- JSON [[a_start, a_end, b_start, b_end], ...], whole pages, a to b
  state       TEXT NOT NULL DEFAULT 'scored' CHECK (state IN ('scored', 'split')),
  split_by    INTEGER REFERENCES user(id) ON DELETE SET NULL,
  split_at    TEXT,
  scored_at   TEXT NOT NULL DEFAULT (datetime('now')),
  CHECK (sha_a < sha_b),
  UNIQUE (sha_a, sha_b)
);
CREATE INDEX content_match_by_b ON content_match (sha_b);
CREATE TABLE match_lookup (
  sha256              TEXT PRIMARY KEY NOT NULL,
  fingerprint_version INTEGER NOT NULL,
  scorer_version      INTEGER NOT NULL,
  looked_at           TEXT NOT NULL DEFAULT (datetime('now'))
);
ALTER TABLE edition ADD COLUMN joined_by INTEGER REFERENCES content_match(id) ON DELETE SET NULL;
```

**Feed job** (admin only): `{"id": 12, "kind": "match", "step": "matching", "done": 3, "of": 12, "dir": null, "dirs": [], "libraries": [{"id": 1, "name": "Manga"}], "elapsed_secs": 4.0, "cancelling": false}`. The web line: `Matching volumes · 3 of 12`.

**Work API** (`GET /api/library/works/{slug}`), for an administrator only: each edition gains `"matched": {"id": 7, "score": 0.93}` or `null`; the work gains `"kept_apart": [{"id": 9, "series": "Oshinoko", "volume": 5, "slug": "oshinoko-5"}]`. A reader's answer holds neither key.

**Routes:** `POST /api/library/matches/{id}/split` takes the body `{"work": "oshinoko-5", "edition": "ja"}`, naming the edition that leaves as the work API names one (`WorkCard.slug`, `EditionCard.key`, `web/src/library.ts:49`, `:18`): two files of one work can carry the same pair as `joined_by`, so the pair alone cannot say which file leaves. It answers 200 `{"split": [7, 8], "derive": <derive_report_json>}`. `DELETE /api/library/matches/{id}/split` names one pair, takes no body and answers 200 `{"derive": ...}`. Both answer 403 for a reader and 404 for no such pair; the Split answers 404 too unless that work holds that edition, the edition has exactly one member, and that member's digest is one of pair `{id}`'s two; 409 while the job lock or the lease is held (`save_rules`'s pattern, `config_api.rs:1021-1066`).

## Pre-flight (controller, before Task 7)

- `rg -n 'kept_editions|left standing|never writes an alignment|O95|same key|colour' docs/status.md`, and the doc comments of `derive/tests.rs` and `fold/tests.rs` for "left standing" and "kept": this PR has derive write alignments and remove an edition from a work, both of which `import.rs:100-120` and `Series.tsx:278` say derive never does. A closed row recording either as a decision goes to the owner as one batched question before Task 7.
- `ls crates/taiyaku-library/src/schema/` on main: 030 still free.

## Lanes

- **A** (own worktree and `CARGO_TARGET_DIR`): Task 1 → 2 → 4.
- **B**: Task 3 → 7 → 8 → 9 → 10; Task 11 after 3; Task 12 after 8 and 11.
- **C**, after A and 3 are merged: Task 5 → 6.
- **Main line**, all merged: 13 → 14 → 15 → 16 → 17 → 18.

Lanes never amend, merge with `git merge --no-ff`, run the touched crates' tests once on the merged tree, and resolve a `comment-budget.json` conflict by running the guard and setting what it reports. A and C both add a module line to `taiyaku-serve/src/lib.rs`: keep both.

---
### Task 1: The server's scorer, held equal to the frozen one

~70 calls. **Files:** Create `crates/taiyaku-serve/src/volume_match.rs`, `volume_match/tests.rs`; Modify `crates/taiyaku-serve/src/lib.rs` (module list). Test also: `crates/taiyaku-cli/src/match_experiment/tests.rs` (appended).

**Interfaces (produces), `taiyaku_serve::volume_match`:**

```rust
pub const LONG_RUN: u32 = 4;
pub const SCORER_VERSION: u32 = 1;
pub fn hamming(a: PageHash, b: PageHash) -> u32;
pub fn informative(h: PageHash) -> bool;                       // 8..=248 bits set
pub struct Slots { pub hashes: Vec<PageHash>, pub table: Vec<Slot> }
pub fn slots(prints: &[PagePrint], direction: Direction) -> Slots;   // main.rs slot_hashes + taiyaku_core::expand
pub struct Scored { pub v: f64, pub long_paired: u32, pub a_slots: u32, pub b_slots: u32,
                    pub runs: Vec<Run>, pub floor: Option<f32>, pub swapped: bool }  // swapped: y went first
pub fn score(x: (&str, &Slots), y: (&str, &Slots)) -> Scored;     // orients as features::oriented, then aligned
```

- [ ] **Step 1: Failing tests.** Serve: `slots_split_a_spread_with_halves_in_reading_order` (four prints: portrait, landscape with halves, portrait, landscape without halves; rtl hashes `[p0, R1, L1, p2, w3]`, ltr `[p0, L1, R1, p2, w3]`, and each table slot's page and half, per index); `v_counts_runs_of_four_or_more_over_the_larger_side` (10 against 14 slots sharing one run of 3 and one of exactly 4 identical hashes, so the run filter's `>=` and `>` give different answers: `long_paired == 4`, `v == 4.0 / 14.0`); `equal_lengths_put_the_smaller_key_first` (two 6-slot sides: `swapped` false for keys `("a","b")`, true for `("b","a")`); `informative_counts_8_and_248_and_neither_neighbour`. CLI: `the_server_scorer_agrees_with_the_frozen_one`: over the fixtures above plus 20 seeded pairs drawn from one shared pool of hashes (so long runs exist) and one pair with a spread, serve `slots` against `crate::slot_hashes` over `CachedPage::from(PagePrint)` (each hash), and serve `score` against `features::oriented` + `features::aligned` (v, long_paired, a_slots, b_slots, runs), per pair.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 volume_match::` and `cargo test -p taiyaku-cli --all-features -- --test-threads=8 match_experiment::tests::the_server`. Expected: no module.
- [ ] **Step 3: Implement** by copying the frozen code's logic (`features.rs:31-110`, `main.rs:1838-1860`); `hamming` as `crates/taiyaku-cli/src/fingerprint.rs:111`. Comments say why the copy exists (the freeze), not its story. No item carries a `dead_code` expect: each is `pub` in `pub mod volume_match`, which fires none.
- [ ] **Step 4: Run** Step 2's commands, then the Rust gate (taiyaku-serve, taiyaku-cli). The CLI diff in Global Constraints prints nothing.
- [ ] **Step 5: Mutations:** `LONG_RUN = 3`; `a.max(b)` as `a.min(b)`; run filter `>=` as `>`; rtl halves pushed left first; the key comparison inverted (only `equal_lengths…` must catch it); `8..=248` as `9..=247`.
- [ ] **Step 6: Commit:** "The server's copy of O340's volume scorer, held equal to the frozen one by a CLI test".

### Task 2: The server's scorer gives every O340 pair its score (local evidence)

Depends on 1. ~45 calls, plus the run. **Files:** Create `crates/taiyaku-cli/src/match_experiment/volume_test/reproduce.rs`; Modify `volume_test.rs` (module line). Evidence: `/home/davey/projects/taiyaku/.superpowers/sdd/content-volume-joins-evidence/o340-reproduction/`.

- [ ] **Step 1: The test**, `#[ignore]`, `the_server_scorer_gives_every_o340_pair_its_score`. It reads `TAIYAKU_O340_LIBRARY`, `TAIYAKU_O340_SCORES` and `TAIYAKU_O340_OUT` and panics on a relative or missing path. For each member of `members.json`: pages through `taiyaku_read`, `fingerprint::common_border`, `decode_luma` and `describe_page` per page, `encode` then `decode` (the blob the pass reads), `volume_match::slots` right to left (every O340 row is `ltr: false`). Every pair, keyed by path as O340 was, through `volume_match::score`. Each of the 12,246 rows of `scores.tsv` compared on its own: `a`, `b`, `a_slots`, `b_slots`, `long_paired` exactly and `format!("{:.6}", v)` against the V cell. Writes `reproduce.tsv` (row, expected, got) and `summary.txt` (`equal: N of 12246`), then a second section: the 298 equal-length pairs keyed by digest, each V that differs listed. Asserts all 12,246 equal.
- [ ] **Step 2: Control.** Hand-apply `LONG_RUN = 3` in `volume_match.rs`, run Step 3's command, see rows differ, revert.
- [ ] **Step 3: Run** `mkdir -p /home/davey/projects/taiyaku/.superpowers/sdd/content-volume-joins-evidence/o340-reproduction` (the redirect below needs it), then `TAIYAKU_O340_LIBRARY=/home/davey/temp/manga-test TAIYAKU_O340_SCORES=/home/davey/projects/taiyaku/.superpowers/sdd/content-matching-o340-evidence/scores.tsv TAIYAKU_O340_OUT=/home/davey/projects/taiyaku/.superpowers/sdd/content-volume-joins-evidence/o340-reproduction cargo test -p taiyaku-cli --release --all-features -- --ignored --test-threads=8 reproduce:: >| /home/davey/projects/taiyaku/.superpowers/sdd/content-volume-joins-evidence/o340-reproduction/run.log 2>&1`. Expected: PASS, `equal: 12246 of 12246`. A digest-keyed difference is recorded for Task 18's notes; one that crosses 0.87 stops the branch and goes to the owner.
- [ ] **Step 4: Commit** the test only: "A local check that the server's scorer gives all 12,246 O340 pairs their scores".

### Task 3: Migration 030 and the match store

~110 calls. **Files:** Create `crates/taiyaku-library/src/schema/030_content_matches.sql` (Shapes), `crates/taiyaku-library/src/matches.rs`, `matches/tests.rs`; Modify `schema.rs` (`MIGRATIONS`, `SCHEMA_VERSION = 30`), `schema/tests.rs` (`the_schema_version_is_twenty_nine` at `:1469` becomes `…_thirty`), `lib.rs` (`pub mod matches;`). Test also: `crates/taiyaku-cli/src/match_experiment/volume_test/tests.rs`.

**Interfaces (produces), `taiyaku_library::matches`:**

```rust
pub const BAR: f64 = 0.87;
pub const FLOOR: f64 = 0.6;
pub enum State { Scored, Split }                                  // "scored" | "split"
pub struct NewPair { pub sha_a: String, pub sha_b: String, pub v: f64, pub long_paired: u32,
  pub a_slots: u32, pub b_slots: u32, pub a_pages: u32, pub b_pages: u32,
  pub floor: Option<f32>, pub runs: Vec<(PageRange, PageRange)> }  // whole pages, a to b
pub struct Pair { pub id: i64, pub new: NewPair, pub state: State }
pub struct Participant { pub sha256: String, pub prints: Vec<u8>, pub work_id: i64,
  pub series_id: Option<i64>, pub volume: f64, pub reading: Direction, pub libraries: BTreeSet<i64> }
pub fn reach(conn: &Connection) -> Result<HashMap<i64, BTreeSet<i64>>, String>;   // root -> libraries with matching on
pub fn participants(conn: &Connection, fingerprint_version: u32) -> Result<Vec<Participant>, String>;  // by digest
pub fn looked_up(conn: &Connection) -> Result<HashSet<String>, String>;
pub fn scored(conn: &Connection) -> Result<HashSet<(String, String)>, String>;
pub fn store(conn: &Connection, pairs: &[NewPair], looked_up: &str, fingerprint_version: u32,
             scorer_version: u32) -> Result<(), String>;   // one transaction; Err, writing nothing, for a pair under FLOOR
pub fn pairs(conn: &Connection) -> Result<Vec<Pair>, String>;   // by id
pub fn set_state(conn: &Connection, ids: &[i64], state: State, by: Option<i64>) -> Result<usize, String>;
```

- [ ] **Step 1: Failing tests.**
  - `migration_30_adds_the_match_tables_and_joined_by`: a store built at 29 with an edition row, migrated: the row is intact with `joined_by` NULL; both tables have the Shapes columns. Rows are inserted before the migration runs.
  - `a_pair_is_stored_once_with_the_smaller_digest_first`: storing (b, a) gives one row with `sha_a < sha_b`, its runs and slot and page counts turned round, each asserted; storing (a, b) again adds nothing. Mutation: no turn-round of the runs.
  - `store_refuses_a_pair_under_the_floor_and_marks_the_file_looked_up`: a pair at 0.5999 is an `Err` and writes nothing, neither the pair nor the lookup row (the pass never sends one); then a pair at 0.6 stores both the pair and the lookup row, which carries both versions.
  - `participants_are_sole_members_of_volumed_works_with_whole_prints_a_matching_library_reaches`: seven files, each asserted in or out: sole member, volume, whole, reached (in); an edition of two members, a work with no volume, a `partial` row, a row of another fingerprint version, a directory only a switched-off library reaches (each out); a file reached only through a second directory's `member_root` (in, `libraries` that one library). Mutations: drop the member count; drop `match_by_content`.
  - `set_state_records_who_and_undo_clears_it`.
  - CLI: `the_server_bar_is_the_frozen_bar`: `matches::BAR` equals the bar `bar::read_frozen` reads from `bar.json`'s text.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 matches:: schema::`. Expected: FAIL.
- [ ] **Step 3: Implement.** `reach` is `reach::of_libraries(roots, &[library])` per library with matching on. `participants` joins member, edition, work, `fingerprint` (`outcome = 'whole'`, the version given) and `member_root`. No item carries a `dead_code` expect: each is `pub` in `pub mod matches`, which fires none.
- [ ] **Step 4: Run** Step 2's command; Rust gate (taiyaku-library, taiyaku-cli).
- [ ] **Step 5: Mutations** as named; also `store`'s floor test `>` for `>=`.
- [ ] **Step 6: Commit:** "Migration 030: pairs scored by content, the files looked up, and the edition a join brought in".

### Task 4: The lookup and whole-page runs

Depends on 1. ~70 calls. **Files:** Modify `volume_match.rs`, `volume_match/tests.rs`.

**Interfaces (produces):**

```rust
pub const LEADERS: usize = 3;
pub struct Entry { pub key: String, pub slots: Vec<PageHash>, pub libraries: BTreeSet<i64>,
                   pub series: Option<i64>, pub volume: Option<f64> }   // entries sorted by key
pub fn leaders(entries: &[Entry], query: usize) -> Vec<usize>;
pub fn page_runs(runs: &[Run], a: &[Slot], b: &[Slot]) -> Vec<(PageRange, PageRange)>;
```

**Whole pages (first written here):** a page pair (p, q) is kept when the run pairs every slot of p with a slot of q and every slot of q with a slot of p: both whole, or both split with their halves paired in order. Any other page touched by a paired slot is left unpaired, on both sides. Kept pairs consecutive on both sides form one run.

- [ ] **Step 1: Failing tests.** `leaders_rank_by_votes_then_series_then_volume_then_digest` (five entries whose votes tie two by two; the exact top three); `a_slot_votes_only_among_files_sharing_a_library` (the nearest slot sits in a library-2 file; the vote goes to the nearest library-1 file); `uninformative_slots_neither_vote_nor_draw_votes` (0, 7 and 249 bits against 8 and 248); `page_runs_cut_where_one_side_split_a_spread` (A: pages 0-4, 5 split, 6-9; B: 11 whole pages; A's halves of 5 paired with B's 5 and 6: exactly `[(0..=4, 0..=4), (6..=9, 7..=10)]`); `page_runs_keep_a_spread_split_on_both_sides`; `page_runs_break_where_b_skips_a_page`.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 volume_match::`. Expected: FAIL.
- [ ] **Step 3: Implement.** `leaders` is `lookup.rs`'s `votes` with the library filter beside its own-file skip, then the tie order.
- [ ] **Step 4: Run** it; Rust gate (taiyaku-serve).
- [ ] **Step 5: Mutations:** drop the series tie-break; digest order descending; drop the library filter; keep a half pair as a whole page; cut whenever a half appears.
- [ ] **Step 6: Commit:** "The server's lookup and the cut of solver runs to whole pages".

### Task 5: The match pass: look up, score, store, resume

Depends on 3 and 4. ~120 calls. **Files:** Create `crates/taiyaku-serve/src/match_pass.rs`, `match_pass/tests.rs`; Modify `lib.rs` (`pub matches: Arc<match_pass::Pass>` after `fingerprints` `:197`, its doc saying why it is inert unless `serve` builds it; `serve()`'s literal and the `#[cfg(test)] pass` option's pattern `:420-487` for `matches`), every other `ServeState` literal (`rg -n 'fingerprints: ' crates/taiyaku-serve/src`): `matches: Default::default(),`; `test_support.rs` (a state with a live match pass, beside `:3791-3830`); `crates/taiyaku-library/src/progress.rs` (`Step::Matching`, `"matching"`, last); `scan_jobs.rs` (`JobKind::Match` `"match"` `:39-56`, `Job::matching` beside `Job::fingerprint` `:132` with an ended counter of its own as that one has, `set_count` taking the step (the fingerprint pass's calls pass `Step::Fingerprinting`), `Step::Matching` in the `Running::at` arm `:262`); `scan_feed.rs`'s cancel `match` (`:106-108`) is exhaustive, so `JobKind::Match` gets its arm here, sending to `state.matches.cancel`; Task 6 proves it through the route.

**Interfaces (produces):** `match_pass::Pass` shaped as `fingerprint_pass::Pass` (`Default` inert, `live()`, `with(PassSettings)`, `job()`, `cancel(&Job)`, `#[cfg(test)] settle(Duration) -> bool`); `PassSettings { workers: usize, poll: Duration, #[cfg(test)] after_lookup: Option<DigestHook> }` (default 4 workers, 1 s), where `DigestHook` is the pass's own `Arc<dyn Fn(&str) + Send + Sync>`, called with each digest looked up (`fingerprint_pass::FileHook` takes a path); `pub(crate) fn kick(state: &Arc<ServeState>)`, carrying `#[cfg_attr(not(test), expect(dead_code, reason = "Task 6 calls it"))]`.

**How a pass runs:** the coordinator lowers its priority (`fingerprint_pass::priority::lower`) and reads `participants(fingerprint::VERSION)` less `looked_up`. Nothing due: idle, no job. Otherwise every participant's prints become `Slots` in its work's reading direction and one `Entry`, and `workers` threads take the due digests in order. Per file: stop if cancelling; `leaders`; score each leader pair not in `scored` and not scored this pass; keep a pair at `FLOOR` or above as a `NewPair` named first side first, its runs `page_runs` over the two tables in that order (`store` puts the smaller digest first); wait while the job lock is held; `matches::store` in one transaction; `after_lookup`; count. Only slots are kept in memory, not blobs.

- [ ] **Step 1: Failing tests** (synthetic prints over shared hash pools: A and B share 95% of pages in order, A and C 70%, D shares nothing):
  - `a_pass_stores_pairs_from_the_floor_and_marks_every_file`: rows (A,B) and (A,C) with V asserted against `volume_match::score` of the same slots; no row names D; every digest has a lookup row. Mutation: store every pair.
  - `a_cancelled_pass_resumes_without_looking_up_a_file_twice`: one worker; `after_lookup` cancels after the first file; a second `kick` looks up the rest; the hook's digests hold no repeat. Mutation: due ignoring lookup rows.
  - `a_pair_already_stored_is_not_scored_again`: a planted (A,B) row at V 0.99 is still 0.99. Mutation: re-score stored pairs.
  - `a_file_with_new_bytes_is_looked_up_afresh`.
  - `a_left_to_right_work_is_scored_in_its_own_direction`: a pair whose spread halves line up only when the ltr work's halves go left first: stored with V ≥ 0.87. Mutation: always rtl.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 match_pass::`. Expected: FAIL.
- [ ] **Step 3: Implement**; `cargo build -p taiyaku-serve -p taiyaku-cli --all-targets --all-features`.
- [ ] **Step 4: Run** it; Rust gate (taiyaku-serve, taiyaku-library).
- [ ] **Step 5: Mutations** as named.
- [ ] **Step 6: Commit:** "A match pass that looks up each new file, scores its three leaders and stores pairs from 0.6".

### Task 6: The pass in the chain, the feed and its derive

Depends on 5. ~100 calls. **Files:** Modify `crates/taiyaku-serve/src/scan_feed.rs` (`running_jobs` `:135-163` lists the match job and finds it by id; the cancel arm `:106-108`, whose `JobKind::Match` arm Task 5 wrote, proved here through the route; the held loop `:54` skips every kind but `Scan`); `fingerprint_pass.rs` (`coordinate` `:349` takes `&Arc<ServeState>`; at `Next::Idle` `:395` it calls `match_pass::kick` unless its job was cancelled); `match_pass.rs` (the derive; `kick` loses its `dead_code` expect); `library_job.rs`'s doc (`:13-20`), which lists every taker of the lock: the match pass's derive waits for it as a scan does.

**The pass's derive:** when this pass stored a pair at `BAR` or above, the coordinator waits for `library_job` as a scan does (polling, so a cancel during the wait ends the job unrun), takes the lease as `scan_jobs::lease_for_the_derive` does, runs `derive::derive_with(&working, Documents::Reconcile)` to its end, then adds one to `jobs_ended` so open Library views read again. One stderr line names the pairs this pass stored at the bar and the derive's existing counts; Task 13 adds the files it joined, once `DeriveReport::joined` exists.

- [ ] **Step 1: Failing tests.**
  - `a_finished_fingerprint_pass_starts_a_match_pass_and_a_cancelled_one_does_not`. Mutations: kick on every idle; never kick.
  - `the_feed_shows_the_match_pass_and_its_cancel_stops_it`: the job's JSON per field as Shapes; `POST /api/library/scans/{id}/cancel` ends it `Cancelled(Matching)`; `libraries` in the feed's held list unchanged. Mutation: the cancel arm sends to `fingerprints`.
  - `a_pass_that_stored_a_pair_at_the_bar_derives_once_and_moves_ended` (a test hook after the derive counts it; `ended` before and after) and `a_pass_that_stored_only_pairs_under_the_bar_derives_nothing`. Mutation: derive whenever a pair was stored.
  - `the_pass_derive_waits_for_the_job_lock`: lock held, no derive; released, one. Mutation: `try_lock` and skip.
  - `startup_resumes_matching_after_fingerprinting`: `at_startup` with nothing due a fingerprint and two files due a lookup: both are looked up.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 match_pass:: scan_feed:: fingerprint_pass::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it; Rust gate (taiyaku-serve, taiyaku-library).
- [ ] **Step 5: Mutations** as named.
- [ ] **Step 6: Commit:** "The match pass follows the fingerprint pass, shows in the scan feed with its Cancel, and derives when a pair reached the bar".

### Task 7: Derive: who takes part, and which pairs apply

Depends on 3. ~100 calls. **Files:** Create `crates/taiyaku-library/src/derive/joins.rs`, `derive/joins/tests.rs`; Modify `derive.rs`: `mod joins;`; `Candidate` (`:1075`) gains `held_by_rule: bool`, set in `classify` (`:1533`): true when the rule `resolved.provenance` names for `Field::Series` set the series by its literal `set_series` (the series is that literal, which `Engine::resolve` never reconciles, O89). A Regex rule may carry `set_series` and capture `series` too; the capture wins (`rules.rs:464-467`, `:496-505`) while provenance still names the rule, so such a file is not held: `crates/taiyaku-library/src/rules.rs`'s `Resolved` exposes the `series_was_captured` that `Engine::resolve` already tracks.

**Interfaces (produces), private to `derive`:**

```rust
pub(super) struct Edge { pub pair: i64, pub v: f64, pub at: [(GroupKey, String); 2] }   // (group, edition key) per side
pub(super) struct Applicable { pub edges: Vec<Edge>, pub together: Vec<Edge>, pub held: BTreeSet<GroupKey> }
pub(super) fn applicable(conn: &Connection, groups: &BTreeMap<GroupKey, Group>) -> Result<Applicable, String>;
```

An edge is a pair with `v >= BAR`, state `scored`, both digests taking part (Global Constraints), a library in common (through `Candidate::copies` too), and different edition keys; one inside a single group goes to `together`. `held` holds every group with a `held_by_rule` candidate or whose digest sits in a work, outside an edition with `joined_by` set, that has `title_hand`, `volume_hand` or `series_id_hand`. Derive ignores the fingerprint version: pairs exist only where the pass scored them. `Candidate::copies` here and Task 3's `member_root` mean the same reach: every directory holding a copy of the file.

Tests build one library with matching on over two roots, `Volumes` (key `en`, official, `en`) and `Raw` (key `ja`, raw, `ja`), as `derive/tests.rs`'s `archived_volumes` builds its tree, with `fingerprint` rows planted `whole` and pairs stored through `matches::store`.

- [ ] **Step 1: Failing tests.**
  - `a_pair_at_the_bar_applies_and_one_just_under_does_not` (0.87 and 0.8699). Mutation: `>` for `>=`.
  - `a_split_pair_applies_nothing`; `same_key_files_link_nothing` (both under `Volumes`).
  - `only_a_literal_series_holds_a_file`: a prefix rule setting `set_series` holds; one setting only `set_edition_key` does not; the seeds do not; seed 4 with its pattern edited does not; a hand-written regex capturing `series` and `volume` does not; a Regex rule carrying `set_series` that also captures `series` does not. Mutations: any provenance for `Field::Series` holds (the regex and edited-seed cases fail); `set_series` ignored (the prefix case fails); `series_was_captured` ignored (the both-set case fails).
  - `an_admins_hand_value_holds_the_work_that_would_move`: one case each for `title_hand`, `volume_hand`, `series_id_hand`; the same on the host holds nothing.
  - Review Focus 5, `a_pair_no_library_reaches_on_both_sides_applies_nothing`: Manga holds `Volumes`, Comics holds `Raw`, both on: no edge; a byte-identical copy of the raw file in a third directory Manga also holds, keyed `ja`: an edge, reached through `Candidate::copies`; Manga switched off: none.
  - `a_file_sharing_its_edition_or_read_partially_takes_no_part`.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 derive::joins::`. Expected: FAIL.
- [ ] **Step 3: Implement** (called from nowhere yet but its tests: `applicable` and `Candidate::held_by_rule` carry `#[cfg_attr(not(test), expect(dead_code, reason = "Task 8 calls it"))]`).
- [ ] **Step 4: Run** it; Rust gate (taiyaku-library).
- [ ] **Step 5: Mutations** as named.
- [ ] **Step 6: Commit:** "Derive reads the match table: who takes part and which pairs apply".

### Task 8: Derive: components, the host, labels

Depends on 7. ~140 calls; split off the cancel and preview tests into a Task 8b if it passes 120. **Files:** Modify `derive/joins.rs` (`apply`); `derive.rs`: after `place` (`:764`) `let (groups, joins) = joins::apply(conn, groups, &mut report)?;`; `Group` (`:1139`) gains `hub_hint: Option<String>` and `Candidate` `joined_by: Option<i64>`; `document` (`:1891`) takes `held_hub.or(hub_hint)`; `write_work` (`:2206`) sets `edition.joined_by` after the import, beside `write_edition_metadata`, and `unchanged` (`:1984`) reads a different `joined_by` as a change; the write loop (`:794`) records `work_ids` for Tasks 10 and 12; `DeriveReport` gains `joined`, and in this commit `crates/taiyaku-serve/src/config_api.rs`'s `derive_report_json` (`:1488`, which names every field with no `..`, so serve stops compiling without it) gains its `joined` key. `applicable` and `Candidate::held_by_rule` lose their `dead_code` expects.

**Interfaces (produces):** `pub(super) struct Joins { pub moved: BTreeMap<String, GroupKey>, pub aligned: Vec<Edge> }`: each digest that joined and the host group it joined; every edge, applied or `together`, whose two files end in one group, its `at` naming that group. `pub(super) fn apply(conn: &Connection, groups: BTreeMap<GroupKey, Group>, report: &mut DeriveReport) -> Result<(BTreeMap<GroupKey, Group>, Joins), String>`. The write loop keeps `work_ids: BTreeMap<GroupKey, i64>`. `moved` carries `#[cfg_attr(not(test), expect(dead_code, reason = "Task 12 calls it"))]` and `aligned` the same naming Task 10.

**Applying edges (first written here):**
1. A group's rank: its best edition kind (official, fan, raw, then anything else); its series' age (the series row's id where one exists, else after every row, by title); its work's age (the id `held_work`'s lookup would find, read only, else after every row); its key.
2. Edges in order: V descending; an edge whose two groups share a series title first; the older series of the two; then the two digests.
3. Union-find over groups. For each edge, with components C1 and C2: skip when they are one; the host is the best-ranked group of both; skip when a `held` group other than the host would move; skip when either matched file's key is already a key of the other component. Otherwise merge: every edition of the moving groups joins the component unless its key is already there, in which case it stays in its own group. A candidate that moves takes as `joined_by` the first applied edge, in edge order, that names its own file; a candidate that moves only with its group (no applied edge names its file) takes the edge that moved its group, a marker Task 12 finds the edition by, which names another file and so shows no mark and no Split (Task 14).
4. Each component's moving editions go into the host's group; a group left with no edition goes; `hub_hint` is the host's own first key, which `document` uses only for a work not yet written.

- [ ] **Step 1: Failing tests.**
  - `a_raw_volume_joins_the_english_volumes_work_as_an_edition`: one work with the English work's id, series, volume and hub; editions `en` (official, `en`) and `ja` (raw, `ja`), `ja`'s `joined_by` the pair; the raw series counted in `series_holding_no_work`; `report.joined == 1`. Each asserted.
  - `the_host_ranks_official_then_fan_then_raw_then_older_series_then_older_work`: one fixture per step, each where the next criterion would choose the other side.
  - `a_raw_matching_black_and_white_and_colour_joins_the_higher_score` (0.95 against 0.91, both `en`); at equal scores, the same series; then the older series.
  - `a_file_whose_key_the_component_holds_stays_where_its_name_put_it`.
  - `an_untouched_edition_with_a_new_key_moves_with_its_work`: the guest's work holds `ja` (matched) and `ko` (no pair names it; a key the host lacks): both end on the host's work, `ko`'s `joined_by` the pair that moved its group, which does not name its digest.
  - `a_new_host_keeps_its_own_hub`: keys `volumes` (host) and `raw` (guest), neither written before: hub `volumes`. Mutation: `document` ignoring `hub_hint`.
  - Review Focus 1, `a_triangle_becomes_one_work_and_splitting_the_raws_pairs_frees_only_the_raw`.
  - `a_triangle_whose_fan_and_raw_score_highest_marks_each_file_with_a_pair_naming_it`: official O, fan F, raw R with V(F,R) > V(O,R) > V(O,F), all at the bar: one work under O; each moved edition's `joined_by` names its own digest, the fan's being (F,R), not (O,R), the edge that moved its group.
  - `the_outcome_does_not_depend_on_the_order_pairs_were_stored`: one raw at equal V to two hosts holding one key, so only the tie order decides which it joins; two stores, pairs inserted in opposite orders: the same works, editions and `joined_by` digests.
  - `a_second_derive_commits_nothing` (counted as `a_derive_over_unchanged_files_writes_nothing` counts) and `a_preview_shows_the_join_and_keeps_none` (`preview::preview` with the current rules lists the raw file's move; the store afterwards has no `ja` edition on the English work).
  - Review Focus 4, `a_derive_cancelled_after_a_join_converges_on_the_next`: a `Watch` that cancels after the host's write; the next derive leaves one work; the one after commits nothing.
  - Serve, `the_derive_report_renders_joined_under_its_own_key`, shaped like `config_api/tests.rs:2699-2714`: `joined` set to a value no other count holds.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 derive::`. Expected: the new tests FAIL, every existing one passes.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it, then `preview::` and `fold::`, and taiyaku-serve's `config_api::`; Rust gate (taiyaku-library, taiyaku-serve).
- [ ] **Step 5: Mutations:** skip the held check; skip the key check; host by the lowest rank; `unchanged` ignoring `joined_by` (the second-derive test); edges by V ascending; edges by pair id (the store-order test); a moving candidate takes the edge that moved its group (the V(F,R) triangle); the `joined` key given another count's value.
- [ ] **Step 6: Commit:** "Derive joins volumes matched by content into one work under a host".

### Task 9: Followers, and names already together

Depends on 8. ~80 calls. **Files:** Modify `derive/joins.rs` (inside `apply`, after the components), `DeriveReport` (`followed`), and in this commit `crates/taiyaku-serve/src/config_api.rs`'s `derive_report_json` (`:1488`) gains its `followed` key.

**Following (first written here):** each moved candidate counts one for its host's series in the tally of its directory, `(root_id, parent_of(rel_path))`, and its own series before the join (its group's series before `apply` merged it). A directory follows S for series T when S counts strictly more than any other in the tally for that directory and T; joined files of another series in the directory count for nothing there, because a flat or mixed directory holds several series and one series' joins must not move another's volumes. This reads Spec §4's "a tie between series moves nothing" as plurality: the series counting most leads, a majority or not. A group of series T follows series S when no join touched it, it has a volume v, it is not `held`, every candidate's directory follows S for T, and no group of S (after the joins and earlier followers) has v; groups in key order, each re-keyed to (S, v).

- [ ] **Step 1: Failing tests.** `raw_volumes_past_the_last_english_one_follow_their_directory` (raw v1-v5, English v1-v3, v1-v3 joined: v4 and v5 in the English series with volumes 4 and 5, per work); `a_follower_fills_only_volumes_the_series_lacks` (raw v2 unmatched beside English v2 stays); `a_tie_between_series_moves_nothing`; `a_work_follows_only_when_every_file_follows`; `a_volume_held_in_two_copies_still_follows`; `a_held_work_does_not_follow`; `a_mixed_directory_moves_only_the_series_whose_files_joined` (one flat directory holding raw series A v1-v4 and raw series B v1-v2; A v1-v3 join the English A: A v4 follows, B v1 and v2 stay in B, per work); `a_split_that_leaves_a_directory_with_no_joins_takes_its_followers_back` (their work ids unchanged); `a_pair_names_already_together_is_in_aligned_and_marks_nothing` (`joined_by` NULL); serve, `the_derive_report_renders_followed_under_its_own_key`, shaped like `config_api/tests.rs:2699-2714`, `followed` a value no other count holds.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 derive::joins::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it and `derive::`, and taiyaku-serve's `config_api::`; Rust gate (taiyaku-library, taiyaku-serve).
- [ ] **Step 5: Mutations:** strictly more as at least as many; one tally per directory, whatever the series (the mixed-directory test); a follower ignoring the series' volumes; a follower with one file outside the directory moving; the `followed` key given another count's value.
- [ ] **Step 6: Commit:** "Volumes past the last match follow their directory into the series it joined".

### Task 10: Solver alignments in whole pages

Depends on 9. ~90 calls. **Files:** Modify `crates/taiyaku-library/src/import.rs`: `replace_alignments` (`:1479`) and a new `put_solver_alignment` share one `insert_alignment`; the module doc's "no opinion about … how two editions correspond" (`:100-120`) says derive now writes `by = solver` rows for pairs matched by content, from the match table, and still deletes none it did not write. `derive/joins.rs` (`write_alignments`, called in `derive_watched` after the fold loop, before `digest_held_members` `:930`; `Joins::aligned` loses its `dead_code` expect); `derive.rs`'s module doc (`:3-10`) names the second input and the parent design's §6.

**Interfaces (produces):** `pub(crate) fn put_solver_alignment(conn: &Connection, work_id: i64, alignment: &Alignment) -> Result<bool, String>`: false, writing nothing, when a `by = hand` row holds the pair either way round or the solver row already equals it; else replaces `by = solver` rows for the pair either way round. `fn alignment_of(pair: &Pair, from_is_a: bool, from_key: &str, to_key: &str) -> Alignment`: paired runs oriented, a gap run (`b: None`) for every stretch of the from side no run covers, `quality` from `verdict`, `paired_slots` and `rounded_avg_run(average_run_length)` over the runs and the from side's page count, `solver: Some(SolverInfo { hash: "dhash-16", floor })` where the row holds a floor.

- [ ] **Step 1: Failing tests.** `a_joined_pair_is_aligned_from_the_host_to_the_guest` (each run, and each figure); `runs_are_turned_round_when_the_host_is_b`; `a_hand_alignment_for_the_pair_keeps_the_solver_out`; `a_second_derive_rewrites_no_alignment` (row ids and counts unchanged); `names_already_together_get_their_alignment`; `a_same_key_pair_gets_none`; `every_stored_alignment_reads_back_through_runs_of` (`project::sidecar`, then `Sidecar::runs_of` is `Ok` for each); `the_series_card_counts_a_joined_volume_as_aligned` (`browse::series`'s `aligned`).
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 derive:: import::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it; Rust gate (taiyaku-library, taiyaku-serve).
- [ ] **Step 5: Mutations:** no turn-round; gaps not filled; the hand check dropped; the equality check dropped (the second-derive test).
- [ ] **Step 6: Commit:** "Derive stores each matched pair's alignment in whole pages, so ⇄ lights".

### Task 11: Moving one edition's reader state

Depends on 3. ~100 calls. **Files:** Modify `crates/taiyaku-library/src/fold.rs` (a sibling of `absorb` `:1704`, reusing `move_positions` `:2000`, `move_notes` `:2071`, `shared_note_ids` `:588`, `move_truth` `:2122`, `truth_provenance_clashes` `:794`; `absorb` itself carries the `stamp_alias` rows of each edition `edition_map` maps, through the step `move_edition` uses, a row the target already holds kept, since a whole-work join goes through `absorb`); `fold/tests.rs`.

**Interfaces (produces):** `pub(crate) fn move_edition(conn: &Connection, from_work: i64, into_work: i64, key: &str) -> Result<Outcome, String>`, `Moved` counting what it moved and dropped. Before any write it refuses (`Outcome::Refused`, nothing written) when: the works are one; the from edition holds a spread flag (a slot would name another page); the from edition holds a `book_pin` row (a pin names blocks of the old work's hub, so it cannot be carried, and dropping it would lose what a reader saved; only books hold pins, and books never take part); one reader holds one note id on both with copies that differ; truth provenance clashes. Then, in one transaction: positions (the newer wins), notes, truth, `stamp_alias` rows (a row the target already holds kept); `by = solver` alignments on the from work naming the key deleted; `by = hand` ones left and counted uncarried; the from edition deleted last. `move_edition` carries `#[cfg_attr(not(test), expect(dead_code, reason = "Task 12 calls it"))]`.

- [ ] **Step 1: Failing tests,** each table on its own: `a_moved_position_keeps_its_anchor_and_the_newer_wins`; `a_moved_note_names_its_new_work_and_its_key`; `truth_moves_and_a_clash_refuses_the_move`; `a_stamp_alias_moves`; `an_edition_with_a_book_pin_is_refused_and_nothing_is_written`; `a_solver_alignment_naming_the_key_goes_and_a_hand_one_is_counted`; `an_edition_with_a_spread_flag_is_refused_and_nothing_is_written`; `absorb_carries_a_stamp_alias`.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 fold::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it; Rust gate (taiyaku-library).
- [ ] **Step 5: Mutations:** notes moved without the key rewrite; the spread refusal dropped; positions without the newer-wins guard; `absorb` without the alias step.
- [ ] **Step 6: Commit:** "A fold of one edition, carrying its readers' state to another work".

### Task 12: Derive moves an edition on a join, an unjoin and a Split

Depends on 8 and 11. ~110 calls. **Files:** Modify `derive/joins.rs` (`move_left_behind`), `derive.rs` (called once `Step::Finishing` is shown, `:823`, past the write loop's cancel check), `DeriveReport` (`moved_editions`, `unmoved_editions`, and the refusals' words in `unfoldable`), and in this commit `crates/taiyaku-serve/src/config_api.rs`'s `derive_report_json` (`:1488`) gains the `moved_editions` and `unmoved_editions` keys; `Joins::moved` and `fold::move_edition` lose their `dead_code` expects.

**Which editions move (first written here):** an edition on a work this derive wrote or left, whose key this derive did not give that work, holding one member at the same path (root, `rel_path`) as one this derive placed under the same key on another work (a path, not a digest: a file whose bytes changed has a new digest, and the stale member keeps the old one), and either carrying `joined_by` (a Split or an unjoin leaves it on the host) or holding a file this derive joined (its old work keeps another edition). A guest whose whole work emptied is not one: `superseded` and `fold::absorb` carry it.

- [ ] **Step 1: Failing tests.**
  - `a_position_and_a_note_follow_a_join_and_come_back_on_a_split`: per field, after the join (through `absorb`) and after the split (through `move_edition`).
  - Review Focus 2, `a_file_whose_bytes_changed_after_joining_goes_back_with_its_position`: no copy of it on the host afterwards; the position's page on its new work after the move.
  - Review Focus 3, `a_partial_join_leaves_no_copy_on_the_old_work`: the guest's other edition holds a key the host already holds, so it stays (one under a new key moves with its work, Task 8).
  - `a_stamp_alias_survives_a_whole_work_join` (through `absorb`).
  - Review Focus 5, `turning_matching_off_unjoins_and_back_on_rejoins_without_scoring`: pair ids, V and `scored_at` unchanged throughout.
  - `a_refused_move_leaves_both_editions_and_is_counted`.
  - Serve, `the_derive_report_renders_moved_and_unmoved_editions_under_their_own_keys`, shaped like `config_api/tests.rs:2699-2714`: two different values, each no other count's.
- [ ] **Step 2: Run** `cargo test -p taiyaku-library --all-features -- --test-threads=8 derive::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it, and taiyaku-serve's `config_api::`; Rust gate (taiyaku-library, taiyaku-serve).
- [ ] **Step 5: Mutations:** move only editions with `joined_by` (the partial-join test); move without the same-key check; skip the step; the two keys' values swapped.
- [ ] **Step 6: Commit:** "A joined or split edition takes its readers' positions, notes and truth with it".

### Task 13: Split and Undo

Depends on 12. ~90 calls. **Files:** Modify `crates/taiyaku-serve/src/curation.rs` (`split_match`, `undo_split`, after `reopen`), `routes.rs` (one path, two methods, in the three tables), `auth/tests.rs` (both lists), `match_pass.rs` (Task 6's stderr line gains the files the derive joined, `DeriveReport::joined`), `library_job.rs`'s doc (`:13-20`, which lists every taker of the lock: Split and Undo try it once and refuse, as a rule save does), `crates/taiyaku-library/src/matches.rs` (`splits_for`); tests in `curation/tests.rs`.

**Interfaces (produces):** `pub fn splits_for(conn: &Connection, id: i64, sha256: &str) -> Result<Option<Vec<i64>>, String>`: the pair `id` and every other pair at `BAR` or above, not split, between the file `sha256` and a file of its current work; `None` for no such pair, or when `sha256` is not one of its two digests. The Split names the edition that leaves, because two files of one work can carry the same pair as `joined_by` (Task 8's V(F,R)-highest triangle gives the fan and the raw both (F,R)), so the pair alone cannot say which file leaves. `POST /api/library/matches/{id}/split` takes the body `{"work": <slug>, "edition": <key>}`, the identifier the work API's edition objects already carry (`WorkCard.slug`, `EditionCard.key`, `web/src/library.ts:49`, `:18`), and answers 404 unless that work holds that edition, the edition has exactly one member, and that member's digest is one of pair `id`'s two; that digest goes to `splits_for`. `DELETE` still names one pair and takes no body. Handlers as Shapes' Routes: try the job lock, take the lease with `Wait::No`, then in one transaction `set_state` and `derive_with(Documents::Reconcile)`, as `save_rules` does.

- [ ] **Step 1: Failing tests.** `split_sends_the_file_back_at_once_and_a_rescan_keeps_it_apart` (the raw's own work after the answer, and after a further `derive`); Review Focus 1, `split_on_a_triangle_splits_every_pair_tying_the_file_to_its_work` (`"split"` lists both ids; the fan stays); `split_on_the_fan_of_a_triangle_leaves_the_raw_joined` (Task 8's V(F,R)-highest triangle, where the fan and the raw both carry (F,R): Split of (F,R) naming the fan's edition; the fan leaves, the raw stays on the official's work through (O,R)) and its mirror `split_on_the_raw_of_that_triangle_leaves_the_fan_joined` (the same pair naming the raw's edition; the raw leaves, the fan stays through (O,F)); `undo_rejoins_at_once`; `split_answers_409_while_the_job_lock_is_held`; 404 for an unknown id, for an edition that work does not hold, for an edition of two members, and for an edition whose member the pair does not name, each with nothing split; `every_curation_route_refuses_a_reader` and `every_route_declares_its_grant_posture` cover the new path.
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 curation:: auth::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it; Rust gate (taiyaku-serve, taiyaku-library).
- [ ] **Step 5: Mutations:** no derive in the handler; split only the named pair; the admin check dropped; `splits_for` over both files of the pair (the two triangle tests); the endpoint check dropped (its 404 case).
- [ ] **Step 6: Commit:** "Admins can Split a volume joined by content, and Undo it; both derive at once".

### Task 14: The mark and the kept-apart list in the work API

Depends on 13. ~60 calls. **Files:** Modify `crates/taiyaku-library/src/matches.rs` (`marks`); `crates/taiyaku-serve/src/library_api.rs` `work` (`:298`): takes `extensions` and adds both keys when `session::administers`; `web/src/library.ts` types (`EditionCard.matched?`, `WorkDetail.kept_apart?`).

**Interfaces (produces):** `pub struct Marks { pub matched: BTreeMap<String, (i64, f64)>, pub kept_apart: Vec<KeptApart> }` (edition key to pair id and V, only where the edition's `joined_by` pair names that edition's own digest: an edition moved only with its group carries a marker naming another file, and shows no mark and no Split); `pub struct KeptApart { pub id: i64, pub series: String, pub volume: Option<f64>, pub slug: String }` (the other file's current work); `pub fn marks(conn: &Connection, work_id: i64) -> Result<Marks, String>`.

- [ ] **Step 1: Failing tests** (serve): `an_admin_sees_the_mark_and_a_reader_sees_neither_key` (per edition; a reader's JSON holds no `matched` and no `kept_apart` key); `kept_apart_names_the_other_volume_on_both_panels` (each side names the other's series, volume and slug); `a_name_joined_edition_has_no_mark`; `an_edition_moved_with_its_group_has_no_mark` (Task 8's untouched `ko` edition has no `matched`; `ja` beside it has one).
- [ ] **Step 2: Run** `cargo test -p taiyaku-serve --all-features -- --test-threads=8 library_api::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** it; Rust gate (taiyaku-serve, taiyaku-library); web typecheck.
- [ ] **Step 5: Mutations:** the `administers` check dropped; `kept_apart` naming its own side; the own-digest check dropped.
- [ ] **Step 6: Commit:** "The work API tells an admin which editions joined by content and which volumes are kept apart".

### Task 15: The UI as a prototype the owner tries

Depends on 6 and 14. ~60 calls plus the owner's rounds. **Files:** Modify `web/src/scanFeed.ts` (`ScanStep` `:39` gains `'matching'`, `FeedJob.kind` `:56` gains `'match'`, `STEP_WORDS` `:335` `'Matching volumes'`, `stepLine` `:361`'s `matching` case `Matching volumes · 3 of 12`, a `matchJob(feed, libraryId?)` beside `fingerprintJob`, the poll's fast-rate condition); `LibraryTable.tsx` (`:64`, `:81`) and `LibraryPage.tsx` (`:879`, `:1260`): a `PassLine` for the match job too; `web/src/Series.tsx`'s tile panel (`:899-930`, admins only through `mayEdit` `:385`): per joined edition "matched by content · 0.93" with **Split** behind a confirmation, per kept-apart pair "kept apart from <series> v05" with **Undo**, each re-reading the series and the work after its answer; `AlignmentPanel`'s comment (`:278`) and its "run taiyaku align" line no longer say derive never aligns; Create `web/src/matches.ts` (`splitMatch(id, work, edition)`, sending `{"work": <slug>, "edition": <key>}` for the edition whose mark was pressed, its work's `WorkCard.slug` and its `EditionCard.key`; `undoSplit(id)`, with no body).

- [ ] **Step 1: Build the prototype**, then serve it by the recipe in memory "UI work: prototype on a live server": a scratch store in the session scratchpad over a plain copy of `Raw/RuriDragon`, `Volumes/RuriDragon`, `Raw/Tongari Boushi no Atelier` and `Volumes/Tongari Boushi no Atelier` from `/home/davey/temp/manga-test` (`touch` a marker first), two roots with the NAS labels, a release build copied into the scratchpad, `ss -lptn 'sport = :4399'` free first (else :4398). The controller starts the server, never a subagent. Scan, wait for both passes, check that joins exist.
- [ ] **Step 2: The owner tries it**, told first what to look at (the feed line and its Cancel, a joined volume's panel, Split and its confirmation, the kept-apart line on both volumes, Undo) and that readers see none of it. Each round's words and changes go in `.superpowers/sdd/progress.md`.
- [ ] **Step 3: Web gate, then commit** the approved state: "The volume-join marks, Split and Undo, and the match line, as the owner tried them". Tell the owner that an approved prototype is held next by tests, which may correct it.

### Task 16: Hold the prototype with tests

Depends on 15. ~100 calls. **Files:** Test: `web/tests/scanFeed.test.tsx`, `web/tests/Libraries.test.tsx`, `web/tests/LibraryPage.test.tsx`, `web/tests/Series.test.tsx`, `web/tests/Series.panel.browser.test.tsx`, Create `web/tests/matches.test.ts`.

- [ ] **Step 1: Failing tests,** with the words the owner approved, asserted per element:
  - `stepLine` of a match job reads exactly `Matching volumes · 3 of 12`; `matchJob(feed, 2)` is undefined while the pass covers only library 1; the line and its Cancel show on the Libraries list and on library 1's page and not on library 2's; Cancel sends `POST /api/library/scans/12/cancel` once. Mutation: `matchJob` ignoring `libraryId`.
  - Series (jsdom): an admin's panel shows one mark per joined edition with its own score (two editions, 0.93 and 0.88, each beside its key); a reader's shows none; a kept-apart line names the other volume.
  - Series (browser, real clicks through `userEvent`, as its neighbours): Split opens the confirmation; Cancel there sends nothing; confirming on the second of two marked editions sends `POST /api/library/matches/7/split` once, its body `{"work": <the work's slug>, "edition": <that edition's key>}`, and the panel then reads the work again; Undo sends `DELETE …/9/split` once; a 409 shows the server's words in an alert and the button is pressable again; two presses inside one `act` send once.
  - `matches.test.ts`: each call's method, path and body (Split's `{"work", "edition"}`; Undo's none).
- [ ] **Step 2: Run** the files alone (`./node_modules/.bin/vitest run --reporter=default <files>`; the browser file with `--project browser`). Expected: the new tests FAIL where the prototype is wrong, else pass with their mutation shown in Step 5.
- [ ] **Step 3: Fix** what they find; say what changed from the prototype in `progress.md`.
- [ ] **Step 4: Run** the web gate and the browser file alone. Expected: PASS.
- [ ] **Step 5: Mutations:** Split without the confirmation; the reader branch showing the mark; the double-press guard removed; the re-read after Split removed; Split's body naming the first marked edition.
- [ ] **Step 6: Commit:** "Tests that hold the volume-join marks, Split, Undo and the match line".

### Task 17: Real material, before the merge

Depends on 16. ~100 calls. Evidence: `/home/davey/projects/taiyaku/.superpowers/sdd/content-volume-joins-evidence/real-material/`.

- [ ] **Step 1: Predict before the run.** For every series of `/home/davey/temp/manga-test/Raw` with English volumes beside it, a row per volume in `prediction.tsv`: which files take part (and why not: a range pack filed as an extra member, a second copy, no volume, a colour or omnibus edition), which join and to what, which follow, which stay apart and why. Committed to the evidence directory before Step 3.
- [ ] **Step 2: The copy.** `du -sh` the directories first; a plain `cp -r` (never `-l`) of those series' `Raw` and `Volumes` directories, `Volumes/[Oshi no Ko] - Fan Translated` included, into `/home/davey/temp/content-volume-joins-check/library` (same disk, 2 TB free); a marker touched before it. A fresh store; roots `Volumes` (`en`, official, `en`) and `Raw` (`ja`, raw, `ja`) in one library; a prefix rule giving `Volumes/[Oshi no Ko] - Fan Translated` the key `fan`, kind fan, language `en`.
- [ ] **Step 3: Run.** A release build, served on a free port; Scan; wait for the fingerprint pass and the match pass. Read the pass's resident memory (`/proc/<pid>/status` `VmRSS`) at its peak and 30 s after.
- [ ] **Step 4: Compare.** From the store (`sqlite3` on the scratch store only), every join, follower and non-join against `prediction.tsv`, row by row, in `compare.tsv`; every difference explained in `findings.md` before merge. A defect goes back to its task.
- [ ] **Step 5: Check by hand:** ⇄ lights on a joined volume and switching editions lands on the matching page; a reading position on the raw edition survives the join and a Split; the Split survives a rescan; Undo rejoins; the pass's line shows, Cancel stops it, and a restart resumes it with no file looked up twice (`match_lookup` row count and `looked_at` before and after).
- [ ] **Step 6: Clean up.** Stop the server by the PID `ss` names, and check the port again; `find /home/davey/temp/manga-test -newer <marker>` prints nothing; delete the copy in its own call naming `/home/davey/temp/content-volume-joins-check`.

### Task 18: Docs, the whole gate and the mutation sweep

Depends on every task. ~80 calls. **Files:** `docs/status.md` (O338 and O340 each gain one "*Narrowed on <date>, and left open.*" sentence: PR 3 joins volumes at the bar automatically with Split; O340's pack-cut ruling remains; new rows for deferrals, at least: a pair the lookup never offers stays unjoined; Splits live only in the store, as dismissals do; the test counts at both sites); `docs/notes.md` (a section "Content matching PR 3: volume joins (<date>)": Task 2's result, Task 17's comparison, the memory reading, and the reasons in "Choices" the spec does not hold); `README.md`'s library section (joins, the mark, Split and Undo, the match line); every other sentence `rg -n 'never writes an alignment|one input|facts and the rules' docs README.md crates web/src` finds made false, in live text only (`docs/status.md`, `README.md`, the docs of `derive.rs` and `import.rs`, `Series.tsx`); dated plans and specs and shipped migrations stay as written.

- [ ] **Step 1: The open-items sweep** (memory "Open items ritual"), then `rg -n 'O338|O340' docs web/src crates` and read every hit.
- [ ] **Step 2: The whole gate.** `env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| <scratchpad>/gate.log 2>&1; echo "exit=$?" >> <scratchpad>/gate.log`, then `rg -a 'all green|FAIL|exit=' <scratchpad>/gate.log`. Expected: `exit=0` and `all green`. Then the browser project whole. Set the test counts from the runs, not by adding.
- [ ] **Step 3: Mutants**, committed first: `flock /run/user/1000/taiyaku-mutants.lock sh -c 'while pgrep -x cargo-mutants >/dev/null; do sleep 30; done; exec scripts/mutants.sh --since main' >| <scratchpad>/mutants.log 2>&1`. Each survivor in this branch's code gets a test that kills it, or a line in the PR body saying why not; check the unviable count against what changed before believing a clean run; add the run's row to `docs/mutants-baseline.md`.
- [ ] **Step 4: Commit:** "Content matching PR 3: notes, status and README for volume joins".

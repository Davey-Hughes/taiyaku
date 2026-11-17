# Content Matching, PR 1: the Measurement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure, on labelled pairs written down in advance, whether page content can match editions safely enough to apply matches without a person. The output is a score, an automatic bar for volume matches and one for chapter matches (or the finding that chapters cannot have one), the library-wide lookup's vote margins, and the first-pass cost on the NAS. Product behaviour does not change.

**Architecture:** The harness is a `#[cfg(test)]` module of `taiyaku-cli`, `match_experiment`. It is gated on environment variables, which is how every earlier measurement reached the fingerprint and solver code (`geometry_experiment.rs`, `model_experiment.rs`, `book_experiment.rs`; `spike/src/main.rs`'s header says scoring against real pages "belongs in the workspace, gated and reproducible"). From `spike/`, the CLI's `fingerprint.rs` could be reached only by `#[path]`-including its source under a second crate root with its own `Cargo.lock`, and `cached_edition`/`slot_hashes` (private to `main.rs`) would have to be copied, so the measurement would run a copy. A `#[cfg(test)]` module compiles nothing into the `taiyaku` binary, which keeps the "no product change" promise. Its cost is registering the module in the experiments' exempt sets (Task 1). The truth comes first (manifest, Tasks 2–3), then the rule (pre-registration, Task 4), then the runs (Tasks 5–8). The bars are frozen in a committed file before the held-out pairs are scored.

**Tech Stack:** Rust (`taiyaku-cli` test module; `taiyaku-core` solver; `taiyaku-read`; `image`, `rayon`, `serde_json`, `base64` already in the crate), zsh, `rsync`, `ssh`/`docker` on the NAS, `rust:1-bookworm` locally.

**Spec:** `docs/superpowers/specs/2026-11-15-edition-matching-by-content-design.md` (owner-approved 2026-11-16): Delivery 1, Design §3 and §7.

## Global Constraints

- **No product change.** Outside `crates/taiyaku-cli/src/match_experiment*`, the only change under `crates/` is the `#[cfg(test)] mod match_experiment;` declaration in `main.rs`. Check: `git diff main --stat -- crates/` lists only those paths.
- **Truth before features.** `match_experiment/manifest.json` (Tasks 2–3) and the pre-registration (Task 4) are committed before any fingerprint of the measured material is computed. The commit order and the first line of `logs/fingerprint.log` are the proof (Task 9).
- **Bars frozen before held-out.** `match_experiment/bars.json` is committed (Task 6) before `score_held_out` first runs (Task 7).
- **Spec values, verbatim:** "The bar is set so that no wrong pair in the measurement clears it." · "If PR 1 finds no bar that keeps every wrong chapter pair out, chapter matches start as suggestions only, and volume matches go automatic alone." · "If PR 1 measures a first pass over the NAS library at more than a day, this comes back to the owner before PR 2 is planned."
- **Isolation.** Nothing is written into `/home/davey/temp/manga-test` except the comics copy (Task 2). `manga-test/library.db` is never opened: no `sqlite3`, no `cp`, no `sha256sum`, `stat` only. Nothing is written to the NAS's disks or shares. The test binary lives in a container tmpfs, and the library mount is `:ro`.
- **Evidence** lives under `EV=/home/davey/projects/taiyaku/.superpowers/sdd/content-matching-evidence/` (the main checkout's gitignored ledger dir, written by Bash), never under `target/`. `docs/notes.md` states figures and never points at an evidence path.
- **Timing uses release builds only** (`cargo test --release …`).
- **The shell is zsh with noclobber.** Use `>|` to overwrite, start a log with `>|` (not `>>`), and run `grep -a` on logs. Never write `for x in $var`, and never name a variable `path`. `find` is bfs, so capture `2>&1` in any emptiness test. Run long jobs with `run_in_background` into a log under `$EV/logs/`, and never pipe a gate through `tail`. Crate test runs take `-- --test-threads=8`; measurement runs take `RAYON_NUM_THREADS=16` on the workstation.
- **Work in this worktree.** Work only in `/home/davey/projects/taiyaku/.claude/worktrees/content-matching`, building into its own `target/`. Give each delete its own Bash call with literal absolute paths.
- **Comments hold the why** (`node scripts/check-comments.mjs` must pass). A comment block is at most 8 lines, except the module header. `crates/taiyaku-cli`'s comment budget is its current share (`scripts/comment-budget.json`), so the new files keep comments under a quarter of their lines. If the share check fails, move prose to the notes section; do not raise the budget. There is no module-level lint suppression, and a pedantic cast takes a per-site `#[expect(…, reason = "…")]`.
- **Rust writes are create-truncate** (`std::fs::write`). Every `.tsv`/`.txt` evidence file the harness writes starts with `# <test> <UTC time> HEAD <git short hash>`, and every `.json` one carries the same line in a `"stamp"` field, so a stale file shows itself.
- **Commits** carry no AI attribution. Each commit message describes what its own commit holds; after any commit whose message names files, check `git show --stat HEAD`.
- **Every figure written into `docs/notes.md` or `docs/status.md` is a claim** that Task 9's claim check verifies. Write fewer.

## Review Focus

1. **An unreadable member.** RAR named `.cbz`, JPEG XL, or a directory with no images: listed in `members.tsv` with its reason, and the run goes on. The listed count equals fingerprinted plus unreadable. Test in Task 5.
2. **A pair counted twice.** The explicit list and the lookup leaders can both name `(a, b)`, or name it as `(b, a)`, and either duplicate would double-weight it in a bar or a percentage. Test in Task 2.
3. **Orientation.** A pair scores the same whichever member the manifest or the lookup names first, because A is always the side with fewer slots. Test in Task 1.
4. **A query with no informative slot** (an all-blank member): no votes and no NaN margin. It is reported as such. Test in Task 1.
5. **A set-but-unusable path variable** (relative, or missing): the test panics with a message and never skips green (the O30 rule in `corpus.rs`). Test in Task 1. The NAS sample's missing paths are reported, not fatal (Task 8).

---

## Evidence layout (`$EV`)

`isolation/` (marker, `before.txt`, `after.txt`) · `comics-copy.txt` · `manifest-draft.json` · `truth/` (page JPEGs, `transcription.json`, `sheet.html`) · `fp-cache/` (the CLI's per-member cache, passed as its root) · `members.tsv` · `leaders.json` · `lookup.txt` · `dev/` and `held-out/` (`scores.tsv`, `rules.txt`, `lookup.txt`, `adjudicate-N.json`, `adjudicate-N.html`) · `nas/` · `logs/` · `gate-*.log`.

Running a gated test (the pattern every task uses):

```sh
cd /home/davey/projects/taiyaku/.claude/worktrees/content-matching
EV=/home/davey/projects/taiyaku/.superpowers/sdd/content-matching-evidence
TAIYAKU_MATCH_LIBRARY=/home/davey/temp/manga-test TAIYAKU_MATCH_EVIDENCE=$EV RAYON_NUM_THREADS=16 \
  cargo test --release -p taiyaku-cli match_experiment::<test> -- --exact --nocapture >| $EV/logs/<test>.log 2>&1
echo "exit=$?" >> $EV/logs/<test>.log
```

---

### Task 1: The instrument — registration, features, rule and votes

**Files:**
- Create: `crates/taiyaku-cli/src/match_experiment.rs` (module header: what it measures, the env vars, one run command per gated test, pointer to the notes section; the gate helpers), `match_experiment/features.rs`, `match_experiment/rule.rs`, `match_experiment/lookup.rs`, `match_experiment/tests.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (one `#[cfg(test)] mod match_experiment;` after `mod library;`, with a why comment of at most 3 lines: gated one-off measurement, declared like the experiments so none of it is in the binary)
- Modify, registering the module everywhere `book_experiment`/`geometry_experiment` are exempt: `scripts/check-all.sh` (both `--ignore-filename-regex`, ~:233 and ~:247), `.forgejo/workflows/ci.yml` (both, ~:248 and ~:304), `.config/nextest.toml` (the regex copy in the comment ~:42, a one-sentence entry in the prose list ~:82, and `+ test(match_experiment)` in the `filter` ~:85), `.cargo/mutants.toml` (`exclude_globs`: `"**/match_experiment.rs"`, `"**/match_experiment/**"`), `docs/ci.md` (a table row after `book_experiment.rs` ~:603, the regex copy ~:674, and the running count sentence ~:624 extended to name this module). The regex entry, inserted after `geometry_experiment`, is `match_experiment[/a-z_]*`, so the submodule files match too. All four executable copies must stay byte-identical (check-all's header says they hash identically).
- Modify: `docs/status.md`. The Rust test count goes at both sites (`grep -n '3605 ' docs/status.md`, ~:183 and ~:260), set from the gate's figure. After the paragraph that opens "Seven further tests are gated on `TAIYAKU_CORPUS`" (~:300), add one sentence: the `match_experiment` tests that are gated on `TAIYAKU_MATCH_LIBRARY` (and `nas_timing` on `TAIYAKU_MATCH_NAS_ROOT`) are the content-matching measurement, with the notes section's title.

**Interfaces (produces):**

```rust
// match_experiment.rs
struct Material { library: PathBuf, evidence: PathBuf }
fn material() -> Option<Material>              // None when TAIYAKU_MATCH_LIBRARY is unset; panics when it or
                                               // TAIYAKU_MATCH_EVIDENCE is set but unusable
fn unusable(var: &str, value: &str, is_dir: bool) -> Option<String>  // absolute + existing dir, else the message
fn stamp(test: &str) -> String                 // "# <test> <UTC time> HEAD <short hash|unknown>"

// features.rs
pub(super) const LONG_RUN: u32 = 4;
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct PairFeatures { pub a_slots: u32, pub b_slots: u32, pub verdict: Verdict,
    pub separation: Option<f32>, pub margin: Option<f32>, pub avg_run: f64, pub paired: u32,
    pub long_paired: u32, pub span_b: u32, pub volume: f64, pub containment: f64 }
pub(super) fn long_runs(runs: &[Run], min_len: u32) -> (u32, u32) // (slots in paired runs of >= min_len,
                                    // B span from the first such run's b.start to the last one's b.end, inclusive; 0 if none)
pub(super) fn volume_score(long_paired: u32, a: u32, b: u32) -> f64      // long_paired / max(a, b); 0.0 when max is 0
pub(super) fn containment_score(long_paired: u32, a: u32, span_b: u32) -> f64 // (lp / a) * (lp / span_b); 0.0 when either is 0
pub(super) fn oriented<'x, T>(x: (&'x str, &'x [T]), y: (&'x str, &'x [T])) -> [(&'x str, &'x [T]); 2]
                                    // fewer slots first; equal lengths: the lexically smaller path first
pub(super) fn features(a: &[PageHash], b: &[PageHash]) -> PairFeatures   // a is already the shorter side
// separation/margin from taiyaku_core::calibrate (None when it returns None); runs from solve; verdict, avg_run, paired from core.

// rule.rs
pub(super) const GAP: f64 = 0.15;
pub(super) const MIN_CHAPTER_SLOTS: u32 = 8;
pub(super) enum Rule { Volume, Chapter, Pack }
pub(super) struct Outcome { pub positives: usize, pub negatives: usize, pub max_negative: Option<f64>,
    pub bar: Option<f64>, pub positives_clearing: usize }   // bar None = no bar fits (max_negative + GAP > 1.0)
pub(super) fn dev_outcome(scored: &[(bool, f64)]) -> Outcome          // (is_positive, score)
pub(super) fn held_out_failures(scored: &[(bool, f64)], bar: f64) -> Vec<usize> // indices of negatives with score >= bar
pub(super) fn to_adjudicate(unlabelled: &[(usize, f64)], line: f64, cap: usize) -> Result<Vec<usize>, usize>
                                    // pair indices with score >= line; Err(count) when count > cap

// lookup.rs
pub(super) const LEADERS: usize = 3;
pub(super) fn informative(h: PageHash) -> bool       // 8 <= popcount <= 248
pub(super) struct Index { pub hashes: Vec<PageHash>, pub owner: Vec<u32> } // informative slots only
pub(super) fn index(members: &[Vec<PageHash>]) -> Index
pub(super) fn votes(index: &Index, query: u32, slots: &[PageHash]) -> Vec<(u32, u32)>
    // each informative query slot votes for the owner of its nearest index hash (crate::fingerprint::hamming)
    // among owners != query; ties go to the lower index position; result (member, votes) sorted votes desc, member asc
pub(super) fn kernel(queries: &[PageHash], stored: &[PageHash]) -> (u64, std::time::Duration)
    // nearest-neighbour scan of every query over every stored hash, rayon over queries; returns comparisons made
```

The `Rule` → class mapping (`fn class`) is written in Task 2, once labels exist.

- [ ] **Step 1: Write the failing tests** in `match_experiment/tests.rs`. Build synthetic `Run`s with `Run { a: PageRange { start, end }, b: Some(..)/None }` and synthetic hashes from a fixed-seed xorshift helper in the test file.
  - `long_runs_counts_only_paired_runs_of_four_or_more`: runs a0–2↔b10–12, a3 unpaired, a4–9↔b13–18, a10–13↔b40–43 → `(10, 31)`.
  - `long_runs_with_no_long_run_is_zero`: three paired runs of length 3 → `(0, 0)`.
  - `volume_score_is_long_paired_over_the_longer_side`: `(180, 190, 200)` → `0.9`; `(0, 0, 0)` → `0.0`.
  - `containment_score_rewards_one_contiguous_stretch`: `(18, 20, 20)` → `0.81`; `(18, 20, 180)` → `0.09`; `(5, 20, 0)` → `0.0`.
  - `a_member_against_itself_scores_one`: 40 random hashes against themselves → `volume >= 0.99`, `containment >= 0.99`, `verdict == Sound`.
  - `shuffled_pages_collapse`: the same 40 against a fixed permutation of them → `volume <= 0.10`.
  - `a_pair_scores_the_same_whichever_way_it_is_named` (Review Focus 3): 30 and 45 hashes; `features` over `oriented(x, y)` equals `features` over `oriented(y, x)` field by field.
  - `the_bar_sits_the_gap_above_the_highest_negative`: negatives `0.2, 0.41`, positives `0.9, 0.5` → `bar == Some(0.56)` (compare within 1e-9), `positives_clearing == 1`.
  - `no_bar_fits_when_a_negative_is_within_the_gap_of_one`: negative `0.9` → `bar == None`.
  - `a_held_out_negative_at_the_bar_fails`: bar `0.56`, scored `[(false, 0.56), (false, 0.55), (true, 0.9)]` → `vec![0]`.
  - `adjudication_takes_scores_at_or_above_the_line_and_refuses_over_the_cap`: line `0.3`, cap `2`: `[(0,0.3),(1,0.29),(2,0.5)]` → `Ok(vec![0, 2])`; four at `0.4` → `Err(4)`.
  - `a_blank_slot_casts_no_vote`: `informative` false for all-zero, popcount 7 and 249; true for 8 and 248.
  - `each_informative_slot_votes_for_its_nearest_other_member`: three members, query 0's slots are 2-bit perturbations of member 2's → `votes(..)[0] == (2, n_informative)`, and member 0 never appears.
  - `a_query_with_no_informative_slot_has_no_votes` (Review Focus 4): all-zero query → empty vec.
  - `the_kernel_counts_every_comparison`: 3 queries × 5 stored → `15`.
  - `an_unusable_path_says_why` (Review Focus 5): `unusable("TAIYAKU_MATCH_LIBRARY", "manga-test", true)` is `Some` and names the variable and "absolute"; `("…", "/nope", false)` is `Some`; `("…", "/abs", true)` is `None`.
- [ ] **Step 2: Run them and watch them fail.** `cargo test -p taiyaku-cli match_experiment -- --test-threads=8` is expected to fail: the functions are not defined.
- [ ] **Step 3: Implement** the interfaces above. No gated test exists yet. `material()` follows `corpus.rs`'s rule: unset skips, set-but-unusable panics with `unusable`'s message.
- [ ] **Step 4: Run them and watch them pass.** Run `cargo test -p taiyaku-cli -- --test-threads=8` (the whole crate). Then run `cargo clippy -p taiyaku-cli --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, and `node scripts/check-comments.mjs`, all green.
- [ ] **Step 5: Hand-apply two defects and watch tests fail.** The tests pass `LONG_RUN`, never a literal 4. Set `LONG_RUN = 3` → `long_runs_counts_only…` fails; set it to 1 → `shuffled_pages_collapse` fails too. Invert `informative`'s range test → `a_blank_slot…` fails. Restore both, and check `git diff` holds only the intended code.
- [ ] **Step 6: Gate.** `mkdir -p $EV/logs`. Then run `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 env -u TAIYAKU_CORPUS scripts/check-all.sh >| $EV/gate-task1.log 2>&1; echo "exit=$?" >> $EV/gate-task1.log` in the background. Expected: `grep -a -n 'all green\|exit=' $EV/gate-task1.log` shows `all green` and `exit=0`. If `status-test-count` fails, set the count it reports at both sites and re-run. The coverage table must not list any `match_experiment` file: `grep -a 'match_experiment' $EV/gate-task1.log` prints no coverage row.
- [ ] **Step 7: Commit** the module, `main.rs`, the five registration files and `docs/status.md`: "A gated measurement module for matching editions by content: features, the bar rule and library votes, compiled into tests only".

### Task 2: The comics pair and the manifest from names

**Files:**
- Create: `match_experiment/manifest.rs`. Extend `match_experiment.rs` with a gated `#[test] fn draft_manifest()` and `tests.rs` with the tests below.
- Data: `/home/davey/temp/manga-test/Comics/…` (the copy), `$EV/comics-copy.txt`, `$EV/manifest-draft.json`, `$EV/isolation/`.

**Interfaces (produces):**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")]
pub(super) enum Kind { Volume, Pack, Omnibus, Chapter, ChapterPack, Issue, Collected, Extra }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")]
pub(super) enum Split { Dev, HeldOut }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Member { pub path: String /* relative to the library root */, pub series: String,
    pub franchise: String, pub edition: String, pub kind: Kind, pub volumes: Option<(u32, u32)> /* inclusive */,
    pub copy: Option<String> /* suffix after the volume marker: "b", "_DC", "s", "A"… */,
    pub chapter: Option<String>, pub in_volume: Option<u32>, pub issue: Option<u32>,
    pub issues: Option<Vec<u32>>, pub ltr: bool, pub split: Split,
    pub truth: Option<String> /* how in_volume/issues was set: "name", "contents:<path>", "indicia:<path>" */ }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Adjudication { pub a: String, pub b: String, pub same: bool, pub date: String,
    pub by: String, pub note: String }
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Manifest { pub members: Vec<Member>, pub adjudications: Vec<Adjudication> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) enum Label { SameVolume { copy: bool }, Neighbour, SameSeriesOther, InPack, NotInPack,
    ChapterIn, ChapterNotIn, InCollected, NotInCollected, OtherSeries { franchise: bool },
    CoverStories, Omnibus, Unlabelled }
pub(super) fn draft_member(relative: &str, row: &DirRow) -> Member     // name parsing, see the table below
pub(super) fn label(m: &Manifest, a: &Member, b: &Member) -> Label    // symmetric; adjudications override first
pub(super) fn split_of(a: &Member, b: &Member) -> Split               // HeldOut if either side is
pub(super) fn explicit_pairs(m: &Manifest) -> Vec<(usize, usize)>     // generation rules below; unordered (i < j), deduplicated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] pub(super) enum Source { Explicit, Leader, Both }
pub(super) fn scored_pairs(explicit: &[(usize, usize)], leaders: &[(usize, usize)]) -> Vec<((usize, usize), Source)>
    // union, each unordered pair once, sorted by pair
pub(super) fn class(rule: Rule, label: Label, a: &Member, f: &PairFeatures) -> Option<bool> // a = the oriented short side
pub(super) fn is_omnibus_name(dir_and_file: &str) -> bool   // (?i)kanzenban|perfect edition|deluxe|\b\d-in-1\b|omnibus
```

**Labels** (from member fields; `vol(B)` is B's `volumes` range; a `Collected` member counts as a `Volume` here). The first matching line wins:
- A chapter with `in_volume = N` against a volume or pack of the same series: `ChapterIn` when N is in `vol(B)`, else `ChapterNotIn`. A chapter against another chapter, or an issue against another issue, is `ChapterNotIn`.
- An issue against a collected edition: `InCollected` when the issue is listed in B's `issues`, else `NotInCollected`.
- A volume against a `Pack` of the same series: `InPack` when its number lies inside `vol(B)`, else `NotInPack`.
- Two volumes, or two packs, of the same `series`: the same number or range gives `SameVolume`, with `copy` true when both members sit in the same directory. Two volumes one apart give `Neighbour`; any other difference gives `SameSeriesOther`.
- An `Omnibus` member against anything of its franchise: `Omnibus`. `Extra` (Cover Stories) against its franchise: `CoverStories`.
- Different `series`: `OtherSeries { franchise }`, where `franchise` is true when both share a franchise.
- No truth for a side: `Unlabelled`. That means a chapter without `in_volume` (fractional and `c0`/`c000` chapters, chapters no contents page settled), a `ChapterPack`, a numberless volume against its own series, or an issue with no `issues` list on B.

**Class per rule** (`class`):
- **Volume rule** (score `volume`): `SameVolume` is positive. `Omnibus` and `Unlabelled` are excluded. Everything else is negative.
- **Chapter rule** (score `containment`), only when A is a `Chapter` or `Issue` with `a_slots >= MIN_CHAPTER_SLOTS`: `ChapterIn` and `InCollected` are positive. `ChapterNotIn`, `NotInCollected`, `OtherSeries` and `CoverStories` are negative. Everything else is excluded.
- **Pack rule** (score `containment`), only when A is a `Volume` and B a `Pack`: `InPack` is positive. `NotInPack` and `OtherSeries` (a volume against another series' pack) are negative.

**Explicit pairs** (Tasks 6–7 add the lookup's top-3 leaders):
1. For every series with two or more volume editions, each volume N held in two editions gives the pair (X vN, Y vN), and each such pair gives (X vN, Y vN±1) where held.
2. Within a directory, every two members with the same single number (copies).
3. Each pack against each single volume of its series (any edition) inside its range, and the singles at range start−1 and end+1.
4. Each omnibus vN against the ordinary vN, v(2N−1) and v(2N) of its franchise. These pairs are reported only.
5. Each chapter with `in_volume` N against every edition's vN, every pack holding N, and vN±1.
6. Each volume of a dev or held-out series against the same-numbered volume of the next two series (alphabetical by key) that hold that number. Each spin-off volume against its main series' v01 and same-numbered volume.
7. Each Cover Stories file against every One Piece volume.
8. Each issue against each collected edition.

**Directory table** (`DirRow { dir, series, franchise, edition, kind, ltr, split }`, a `const` in `manifest.rs`; `…` below stands for the directory's full name as `ls` prints it). The defaults: `Volumes/*` → edition `en`, kind `volume`; `Raw/*` → `raw`, kind from the name; `Chapters/*` → `en-fan-ch`, kind `chapter`. **Held-out series:** `go-toubun`, `bocchi*`, `zelda-tp`, `bisque*`, `invincible`; everything else is dev. Franchise equals series unless shown after a slash.

| directory | series / franchise | edition, kind |
| --- | --- | --- |
| `Volumes/Berserk`, `Raw/Berserk` | berserk | en / raw |
| `Volumes/Bisque Doll de Chu ♡` | bisque-de-chu / bisque | |
| `Volumes/Sono Bisuku Doru wa Koi o Suru` | bisque | |
| `Volumes/Sono Bisuku Doru wa Koi o Suru (Semi-Colored)` | bisque | en-colour |
| `Chapters/Sono Bisque Doll wa Koi wo Suru - Fan Translated` | bisque | |
| `Volumes/Bocchi the Rock!`, `Chapters/Bocchi the Rock! - Fan Translated` | bocchi | |
| `Volumes/Bocchi the Rock! - Anthology Comic` | bocchi-anthology / bocchi | |
| `Volumes/Bocchi the Rock! - Gaiden - Hiroi Kikuri no Fukazake Nikki` | bocchi-gaiden / bocchi | |
| `Volumes/Chainsaw Man` (+ ` - Digital Colored Comics`) | chainsaw-man | en (en-colour) |
| `Volumes/Death Note` (+ ` (Colored Council)`) | death-note | en (en-colour) |
| `Volumes/Dungeon Meshi`, `Chapters/Dungeon Meshi - Fan Translated` | dungeon-meshi | |
| `Volumes/Go-Toubun no Hanayome`, `Raw/Go-Toubun no Hanayome`, `Chapters/Go-Toubun no Hanayome - Fan Translated` | go-toubun | en / raw / ch |
| `Volumes/Go-Toubun no Hanayome - Full Color Edition`, `Raw/Go-Toubun no Hanayome - Full Color` | go-toubun | en-colour / raw-colour |
| `Volumes/Kaguya-sama … Zunosen`, `Raw/Kaguya-sama …`, `Chapters/Kaguya-sama … - Fan Translated` | kaguya | en / raw (`color` in a raw name → raw-colour) / ch |
| `Volumes/Kaguya-sama … - Full Color` | kaguya | en-colour |
| `Volumes/Kaguya-sama … - Official Doujin` | kaguya-doujin / kaguya | |
| `Volumes/Kobayashi-san Chi no Meidoragon`, `Raw/Kobayashi-san Chi no Meidoragon`, `Chapters/Kobayashi-san cno Meidoragon - Fan Translated` | kobayashi | |
| `Volumes/Kobayashi-san … - Lucoa wa Boku no xx Desu`, `… - Okomori Gurashi no Fafnir` | kobayashi-lucoa, kobayashi-fafnir / kobayashi | |
| `Volumes/Megami no Cafe Terrace`, `Chapters/Megami Cafe no Terrace` | megami | |
| `Volumes/Monster`, `Raw/Monster` | monster | |
| `Volumes/Monster - The Perfect Edition` | monster-perfect / monster | en, omnibus |
| `Raw/Monster - Kanzenban` | monster-kanzenban / monster | raw, omnibus |
| `Volumes/One Piece`, `Chapters/One Piece` | one-piece | |
| `Volumes/One Piece - Cover Stories - Digital Colored Comics` | one-piece-cover-stories / one-piece | en-colour, extra |
| `Volumes/One-Punch Man`, `Chapters/One Punch Man` | one-punch-man | |
| `Volumes/[Oshi no Ko]`, `Raw/[Oshi no Ko]`, `Chapters/[Oshi no Ko] - Fan Translated` | oshi-no-ko | |
| `Volumes/[Oshi no Ko] - Fan Translated` | oshi-no-ko | en-fan, volume |
| `Volumes/The Legend of Zelda - Twilight Princess`, `Raw/Zelda no Densetsu - Twilight Princess` | zelda-tp | |
| `Volumes/Uzumaki (Deluxe Edition 3-in-1)` | uzumaki | omnibus |
| `Volumes/Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)` | aot-kui-naki | en-colour |
| the rest of `Volumes/` (Dandadan, Hunter × Hunter - Kurapika's Memories, Kabushikigaisha Magi-Lumière, Look Back, Sayonara Eri, Yotsuba&!) | kebab of the name | distractors |
| `Comics/Issues/<series>/<run>`, `Comics/Collected/<series>` | invincible (or Step 1's pick) | en, issue / collected, ltr |

**Name parsing** (`draft_member`):
- `v(\d+)-(\d+)` gives a pack; `v(\d+)` gives a volume. Letters or `_XX` right after the number go to `copy`. `v1 c5` gives a chapter `"5"` with `in_volume = 1`. `c(\d+(\.\d+)?)` gives a chapter. `ch\d+-\d+` gives a chapter pack. An issue is the three-digit number after the run's name.
- A `(2023)` year is never a number.
- A fractional chapter, `c0` and `c000` get `in_volume = None`, with `truth` left empty.
- `Look Back`-style files with no number get `volumes = None`.
- Loose images at the series level (`cover.jpg`) are not members, and `Books/` is out of scope.

- [ ] **Step 1: Copy the comics pair.**
  - List `"/mnt/daveynet/nfs/media 1/Comics/Issues"` and `…/Collected` (read-only use of the NFS mount).
  - Pick a series with both an issues run and collected editions whose issues are all held, a few GB at most. First candidate: `Issues/Invincible/Invincible (2003-2018)` issues `000`–`013` and `Collected/Invincible` `v01`–`v03`.
  - Write the chosen relative paths (relative to `Comics/`) to `$EV/comics-list.txt`, then run `rsync -rt --files-from=$EV/comics-list.txt "/mnt/daveynet/nfs/media 1/Comics/" /home/davey/temp/manga-test/Comics/`.
  - Verify: file count and summed `stat -c %s` are equal on both sides, recorded in `$EV/comics-copy.txt`.
- [ ] **Step 2: Isolation baseline.** Write `stat -c '%n %s %Y %i' /home/davey/temp/manga-test/library.db` into `$EV/isolation/before.txt`, then `touch $EV/isolation/marker`. From here on nothing writes under `manga-test`.
- [ ] **Step 3: Write the failing tests.**
  - `names_parse_into_volumes_packs_chapters_and_copies`, one assertion per name: `Oshinoko v01` → Volume (1,1); `Oshinoko v01-03` → Pack (1,3); `Oshinoko v01b` → (1,1) copy `b`; `Kaguya-sama_Zunousen_v01_DC` → (1,1) copy `_DC`; `DLRAW.TO_Kobayashi-san_Maid_Drago_v01-07A` → Pack (1,7); `Kaguya-sama … - v16 c152.cbz` → Chapter `152`, `in_volume` 16; `… - v1 c5.5.cbz` → Chapter `5.5`, `in_volume` None; `Bocchi the Rock - v1 c0.cbz` → `in_volume` None; `One Piece - c1134 (2024).cbz` → Chapter `1134`; `ch01-15` → ChapterPack; `Invincible 001 (2003) (digital) (Minutemen-InnerDemons).cbz` → Issue 1; `[Oshi no Ko] - v01 (2023).cbz` → (1,1).
  - `labels_are_symmetric`: every ordered pair of a dozen synthetic members gives `label(a,b) == label(b,a)`.
  - `labels_follow_the_class_rules`: one assertion per `Label` variant.
  - `an_adjudication_overrides_the_name_label`.
  - `explicit_pairs_hold_both_neighbours_and_no_duplicates`.
  - `scored_pairs_are_deduplicated_across_sources` (Review Focus 2): explicit `[(1,2)]`, leaders `[(2,1),(1,2),(3,1)]` → `[((1,2), Both), ((1,3), Leader)]`.
  - `class_excludes_short_chapters_and_omnibus_pairs`.
  - `omnibus_names_are_recognised`: Perfect Edition, Kanzenban and Uzumaki 3-in-1 are true; `Complete Color Edition` and every other directory in the table are false.
  - Run them and watch them fail.
- [ ] **Step 4: Implement** `manifest.rs` and the gated `draft_manifest`. The draft walks the table's directories under `TAIYAKU_MATCH_LIBRARY`, reading names only, writes `$EV/manifest-draft.json`, and prints per-directory member counts and every name the parser could not classify. Run the whole crate's tests (`-- --test-threads=8`), clippy, fmt and `check-comments`.
- [ ] **Step 5: Run `draft_manifest`** (debug is fine; it opens no file). Read every unclassified name and every directory's counts against `ls`. Fix the parser for patterns and the draft by hand for one-offs, keeping a list of the hand edits.
- [ ] **Step 6: Commit** the code (manifest data comes in Task 3): "The content-matching measurement's truth from names: member kinds, pair labels, and the comics pair it needs".

### Task 3: Truth the names cannot give — contents pages and indicia (OWNER STEP)

**Files:**
- Create: `match_experiment/sheet.rs` (`truth_extract`, `truth_render`, and the pairing sheet used in Task 6). Add a gated `#[test] fn truth_sheet()` to `match_experiment.rs`, which extracts when `$EV/truth/transcription.json` is absent and renders when it is present.
- Create: `crates/taiyaku-cli/src/match_experiment/manifest.json` (the reviewed draft plus this task's truth).

**Interfaces (produces):**

```rust
pub(super) fn truth_extract(library: &Path, out: &Path, volumes: &[&str], pages: u32) -> Result<(), String>
    // first `pages` pages of each volume as <out>/<slug>/pNN.jpg, 1000 px wide (crate::report::thumbnail's encoder)
pub(super) fn truth_render(library: &Path, out: &Path, transcription: &Path) -> Result<(), String>
    // <out>/sheet.html: each volume's named contents page(s) at 900 px beside its transcribed range; self-contained data: URIs
pub(super) struct SheetPair { pub a: String, pub b: String, pub a_pages: Vec<u32>, pub b_pages: Vec<u32> }
pub(super) fn pairing_sheet(library: &Path, out: &Path, pairs: &[SheetPair]) -> Result<(), String>
    // per pair: each A page beside the B page the alignment paired it with (130 px), then the 3 B pages either side
    // of the stretch; names and verdict boxes only — no score, no label
```

**Volumes whose printed contents settle chapter truth** (`truth = "contents:<path>"`). For a "past the last volume" series, go backwards from its last held volume until the directory's lowest chapter number is covered:
- `go-toubun` EN v01–v14; `dungeon-meshi` EN v01–v14; `bisque` EN v01–v14
- `one-piece` EN v111 and back; `megami` EN v16 and back; `one-punch-man` EN v33 and back
- `oshi-no-ko`: EN v12, `[Oshi no Ko] - Fan Translated` v13, raw `Oshinoko v13` and `v13A`
- `kobayashi`: EN v16, raw v16 and v17; `bocchi`: EN v06 and v07
- Comics: each collected edition's indicia (`truth = "indicia:<path>"`, `issues` list)

Chapters whose number lies inside a transcribed range get `in_volume`. Fractional and zero chapters stay unlabelled. So do the chapters of a volume with no contents page in its first 12 pages, and such a volume is noted in `transcription.json`.

- [ ] **Step 1: Write the failing test and implement.**
  - The test is `a_truth_sheet_embeds_each_page_once`: a `TempDir` fixture with two tiny cbz files (`taiyaku_read::test_support`) renders an HTML that holds exactly one `data:image/jpeg` per named page and none of the words `score` or `bar`. Watch it fail, then implement `sheet.rs`.
  - Then run the crate's tests, clippy, fmt and `check-comments`.
- [ ] **Step 2: Extract.** Run `truth_sheet` (release) for the volumes above with `pages = 12`.
- [ ] **Step 3: Transcribe.** Read the page JPEGs with the Read tool. For each volume, record the contents page index and its first and last chapter numbers (and the issues each collected edition collects) in `$EV/truth/transcription.json`. Read the numbers off the image, never infer them from file names.
- [ ] **Step 4: Render.** Run `truth_sheet` again to write `$EV/truth/sheet.html`.
- [ ] **Step 5: OWNER STEP.** Stop and report `NEEDS_OWNER` with the sheet's absolute path. The controller asks the owner to open it locally (`file://…`) and confirm or correct each range. Do not publish it anywhere: these are page images. Record the owner's corrections in `transcription.json`, with the date, before going on.
- [ ] **Step 6: Apply, check and commit.**
  - Apply the transcription and Task 2's hand edits to the draft, and write `crates/taiyaku-cli/src/match_experiment/manifest.json`.
  - Add the ungated test `the_manifest_is_consistent`, which reads the file through `include_str!` and runs in CI. It asserts that paths are unique, every chapter with `in_volume` has a `truth`, every member's directory is a table row, and `label` is symmetric over `explicit_pairs`. It prints counts per kind, per split and per label class; they go into Task 4's text.
  - Run the crate's tests, then commit `manifest.json` and the test: "Ground truth for the content-matching measurement: N members labelled from names, printed contents pages and indicia".

### Task 4: Pre-registration

**Files:**
- Modify: `docs/notes.md`. Append `## Edition matching by content: PR 1's measurement (2026-11-16)` with a subsection `### Pre-registered before any page was fingerprinted`.
- Create: `crates/taiyaku-cli/src/match_experiment/nas_sample.txt`. Each line is `class<TAB>path relative to the NAS's "media 1"` (`Manga/Volumes/…`, `Manga/Raw/…`, `Manga/Chapters/…`, `Comics/…`). Every path must exist under `manga-test` at the same place: `Manga/` maps to `manga-test/`, `Comics/` to `manga-test/Comics/`. The sample:
  - six EN volumes: Kaguya v01 and v02, One Piece v105 and v106, Monster - The Perfect Edition v01, Bocchi v01
  - five raw members: `Oshinoko v01`, `Kaguya-sama_Zunousen_v02`, `Monster v01`, `DLRAW.TO_Kobayashi-san Maid Drago v02`, the pack `Oshinoko v01-03`
  - twelve chapters: Kaguya `v16 c152`–`c161`, One Piece `c1134` and `c1135`
  - comics: two issues and one collected edition

The pre-registration states exactly these items, in prose, each with its value:

1. **Truth:** the manifest's commit hash, members and labelled pairs per class and split, the dev/held-out split by series (Task 2's held-out list), and the owner-confirmed transcription.
2. **Orientation and slots:** A is the side with fewer slots (ties: the lexically smaller path). Spreads are split per member as `align` does (`cached_edition`), right-to-left except comics.
3. **Features** (all reported): verdict, separation, margin, average run, paired/|A|, and long_paired (slots in paired runs of at least 4). Two scores are computed from them: **V = long_paired / max(|A|, |B|)**, which is low unless both sides are mostly covered by long runs, and **C = (long_paired/|A|) × (long_paired/span_B)**, which is the chapter-in-volume test defined in advance: A's long runs must land in one stretch of B about A's own length.
4. **Rules:**
   - **Volume rule** on V: positives are the same volume (editions, colour and fan variants, copies). Negatives are every other labelled pair except omnibus pairs.
   - **Chapter rule** on C: chapters and issues of at least 8 slots. A chapter under 8 slots is never automatic (a short side always aligns, `docs/notes.md:1904-1916`).
   - **Pack rule** on C: volume against pack.
   - **Omnibus:** reported only, and names decide (ruling 5). The check is that `is_omnibus_name` marks every omnibus member and no other.
5. **The bar** = the highest dev negative + **0.15**. A rule passes on dev if its bar is at most 1.0, and on held-out if no held-out negative scores at or above the frozen bar. The fraction of positives at or above the bar is reported per class and sub-class.
6. **Unlabelled pairs and possible mislabels:**
   - Dev: any unlabelled pair scoring at or above the highest labelled dev negative is shown to the owner. Repeat while the line moves.
   - Held-out: unlabelled pairs at or above the frozen bar are shown.
   - Labelled negatives scoring at least 0.5 are shown too. The owner may relabel them only on "same drawings", and the count is reported.
   - A round over 40 pairs stops, and the controller decides. "Can't tell" counts as different.
7. **Consequences:**
   - Chapter rule fails: chapter matches are suggestions only, and volume matches go automatic alone.
   - Volume rule fails: nothing is automatic, and the result goes back to the owner before PR 2.
   - Pack rule fails: PR 4 cuts packs by names, and content only suggests.
   - Comics fail the chapter bar: issues are suggestions only.
8. **Instrument controls** (asserted): a member against itself gives V ≥ 0.99 and C ≥ 0.99. A true pair against its B shuffled gives V ≤ 0.10. One pair's paired, runs and verdict equal `taiyaku align --stats` on the same two members.
9. **Lookup:** each informative slot (popcount 8–248) votes for the member holding its nearest slot among all other members. Leaders rank by votes, and the top 3 of every member are scored.
   - Reported: the share of queries whose best positive leads, the depth needed, the margins, and the time.
   - Predictions: **P1**, every labelled query's best positive is in its top 3. **P2**, for chapter queries a volume holding the chapter (any edition) leads in at least 90%.
10. **Cost:**
    - The NAS runs on `nas_sample.txt`, twice: all cores, then `RAYON_NUM_THREADS=4`. A Hamming kernel is benchmarked at both.
    - The extrapolation: hours = Σ over the NAS's `Manga/*` and `Comics/*` directories of (bytes ÷ cold read rate + bytes × fingerprint seconds per byte for its class), plus lookup = slots² ÷ comparison rate. Slots are bytes × slots per byte for the class, measured on manga-test.
    - **If the 4-thread first pass exceeds 24 h, the result says so and the owner is asked before PR 2.** The all-cores figure is reported beside it.
11. **Fixed from this commit:** the features, the run length 4, the gap 0.15, the 8-slot minimum, the classes, the label function, the split and the sample. Anything else in the results is descriptive and does not replace V or C in this PR.

- [ ] **Step 1:** Write the subsection and `nas_sample.txt` (check every path exists under manga-test with `test -e`).
- [ ] **Step 2:** Self-check the text against items 1–11, one sentence each, values exact. Every `docs/notes.md:N` citation in it must point at the line it names (`sed -n Np docs/notes.md`).
- [ ] **Step 3: Commit:** "Pre-register the content-matching rule: scores, a 0.15 gap above every wrong pair, and what failing it means".

### Task 5: Fingerprints and the library-wide lookup

**Files:**
- Modify: `match_experiment.rs` (gated `#[test] fn fingerprint_and_lookup()`), `lookup.rs` (the lookup report), `tests.rs`.

**Interfaces (produces):**

```rust
pub(super) struct Print { pub slots: Vec<PageHash>, pub table: Vec<taiyaku_core::Slot>, pub pages: u32, pub bytes: u64 }
pub(super) fn print_member(cache: &Path, library: &Path, m: &Member) -> Result<Print, String>
    // crate::cached_edition(cache, &[library.join(&m.path)], Rtl unless m.ltr); bytes = file len or summed dir files
pub(super) fn print_all(cache: &Path, library: &Path, m: &Manifest) -> (Vec<Option<Print>>, Vec<Row>)
    // Row = one members.tsv line: path, kind, pages, slots, bytes, ms, error; never aborts on one member
```

`$EV/leaders.json` maps each member path to its top 10 `[path, votes]`, with its count of informative slots. `$EV/lookup.txt` holds slots indexed, comparisons, wall seconds, threads, and comparisons per second. `$EV/dev/lookup.txt` holds the dev queries' rank of best positive, votes for best positive and best negative, and margin = (pos − neg) / informative slots, "none" when there are no informative slots, followed by P1 and P2 as measured.

- [ ] **Step 1: Write the failing test.** `an_unreadable_member_is_listed_with_its_reason` (Review Focus 1): a `TempDir` holding `good.cbz` (two pages via `taiyaku_read::test_support`), `bad.cbz` (bytes `Rar!\x1a\x07\x00`) and an empty directory, each a `Member`. `print_all` returns two `None`s whose rows carry a non-empty error naming the file, one `Some`, and three rows. Watch it fail; implement; run the crate's tests, clippy, fmt and `check-comments`.
- [ ] **Step 2: Run on the material** (release, background, `RAYON_NUM_THREADS=16`, cache root `$EV/fp-cache`), log `$EV/logs/fingerprint.log`. The run fingerprints every manifest member, writes `members.tsv`, builds the index, runs `votes` for every member, and writes `leaders.json`, `lookup.txt` and `dev/lookup.txt`. It asserts that the rows equal the manifest's members and that at least one member was fingerprinted.
- [ ] **Step 3: Read the summaries.** `grep -a -n 'exit=\|unreadable\|members\|comparisons' $EV/logs/fingerprint.log`. Every unreadable member must be named with its reason. If more than 2% are unreadable, stop and report before Task 6.
- [ ] **Step 4: Interim isolation check.** `find /home/davey/temp/manga-test -newer $EV/isolation/marker 2>&1 | head -5` prints nothing.
- [ ] **Step 5: Commit** the code: "Fingerprint the measured library and rank every member's nearest members by page votes".

### Task 6: Dev scores, adjudication and the frozen bars (OWNER STEP)

**Files:**
- Modify: `match_experiment.rs` (gated `#[test] fn score_dev()` and `#[test] fn adjudication_sheet()`), `rule.rs` (`pub(super) struct ScoredRow { pub a: String, pub b: String, pub label: Label, pub sub_class: String, pub class: Option<bool>, pub score: f64 }` and `pub(super) fn report(rule: Rule, rows: &[ScoredRow]) -> String`, the `rules.txt` writer)
- Create: `crates/taiyaku-cli/src/match_experiment/bars.json`: `{"volume": f64 | null, "chapter": f64 | null, "pack": f64 | null, "manifest_commit": "<hash>"}`
- Modify: `docs/notes.md` (subsection `### Dev: the scores, and the bars frozen`)

`score_dev` takes the dev split of `scored_pairs(explicit_pairs, top-3 leaders)` and computes `features` on oriented slots (cache hits). It writes `dev/scores.tsv` with one row per pair: a, b, source, label, the class under each rule, all features, and the ms taken to score the pair. It asserts the two self/shuffle controls on the first `SameVolume` pair. `dev/rules.txt` holds, per rule: positives, negatives, the max negative and its pair, the bar, the positives clearing it by sub-class (raw↔EN, colour, fan, copy; chapter series; pack), and descriptively, for each other feature, the highest negative and the share of positives above it. It also holds the omnibus rows with V and C, the `is_omnibus_name` check, and the median scoring time per pair. Finally `score_dev` writes `dev/adjudicate-N.json` (Task 4 item 6), and `adjudication_sheet` renders it with `pairing_sheet`.

- [ ] **Step 1: Write a failing test and implement.** `a_rules_report_names_the_pair_behind_each_bar`: synthetic scored rows give a report holding the max negative's two paths and the bar to two decimals. Watch it fail; implement `report`, `score_dev` and `adjudication_sheet`; run the crate's tests, clippy, fmt and `check-comments`.
- [ ] **Step 2: Parity control.** Run `cargo build --release -p taiyaku-cli`. Then run `target/release/taiyaku align "<A>" -- "<B>" --stats --cache $EV/fp-cache` on the first dev `SameVolume` pair, oriented. Its paired, runs and verdict equal that pair's `scores.tsv` row; record both in `$EV/dev/parity.txt`.
- [ ] **Step 3: Run `score_dev`** (release, background, `$EV/logs/score-dev.log`). Read `dev/rules.txt`.
- [ ] **Step 4: OWNER STEP: adjudication rounds.** If `adjudicate-N.json` is non-empty, run `adjudication_sheet` and stop with `NEEDS_OWNER` and the sheet's absolute path. The owner marks each pair same / different / can't tell. Record the verdicts in `manifest.json`'s `adjudications` (`by: "owner"`, date), commit ("Owner's verdicts on N pairs the names could not label"), and re-run `score_dev`. Repeat until the list is empty or a round exceeds 40 pairs; in that case stop and report.
- [ ] **Step 5: Freeze.**
  - Write `bars.json` from the final `rules.txt`.
  - Write the notes subsection: per rule the counts, the bar, the max negative pair, positives clearing, the parity and control results, adjudication rounds and relabel counts, the omnibus check, and P1/P2 on dev.
  - Commit `bars.json` and `docs/notes.md`: "Content matching on the dev series: the bars, frozen before the held-out series are scored".

### Task 7: Held-out series (OWNER STEP)

**Files:** Modify `match_experiment.rs` (gated `#[test] fn score_held_out()`, which reads `bars.json` through `include_str!`) and `docs/notes.md` (subsection `### Held out: Go-Toubun, Bocchi, Zelda, Bisque Doll and the comics pair`).

`score_held_out` scores the held-out split as `score_dev` does, plus held-out queries' lookup figures. It never recomputes a bar. It writes `held-out/rules.txt`: per rule, `held_out_failures` against the frozen bar, the positives clearing it, and the comics pair under the chapter bar. It also writes `held-out/adjudicate-N.json` for unlabelled pairs at or above the frozen bar.

- [ ] **Step 1:** Implement and run (release, background, `$EV/logs/score-held-out.log`). Check: `git log -1 --format=%ci -- crates/taiyaku-cli/src/match_experiment/bars.json` is earlier than the log's first line.
- [ ] **Step 2: OWNER STEP.** Same as Task 6 Step 4, with one difference: a "different" or "can't tell" verdict here is a held-out failure, not a bar change.
- [ ] **Step 3:** Write the subsection with each rule's verdict (passes / fails, and which pair failed it) and the consequence that Task 4 item 7 attaches. Commit `docs/notes.md` (and the manifest if adjudicated): "Content matching on the held-out series against the frozen bars".

### Task 8: Fingerprinting cost on the NAS

**Files:** Modify `match_experiment.rs`: add a gated `#[test] fn nas_timing()`, gated on `TAIYAKU_MATCH_NAS_ROOT` alone, which reads the sample through `include_str!` and nothing else at runtime but the root and `/proc`. Evidence goes under `$EV/nas/`.

`nas_timing` prints:
- a header: the stamp, `available_parallelism`, rayon threads, `/proc/loadavg`, and the `/proc/mounts` line for the root (which must show `ro`)
- one TSV line per sample entry, or `MISSING <path>`: class, path, bytes, pages, digest ms (`taiyaku_read::member_digest`, the cold read), re-read ms (`read_edition`), fingerprint ms (`fingerprint::describe_pages`), and loadavg after
- the kernel: 5,000 random queries × 2,000,000 random stored hashes (fixed seed), with comparisons, seconds and comparisons per second

It asserts that at least 80% of the sample is present and fingerprinted.

- [ ] **Step 1: Implement, and smoke-test on the workstation.**
  - Make a scratchpad dir holding the symlinks `Manga → /home/davey/temp/manga-test` and `Comics → /home/davey/temp/manga-test/Comics`, and run `nas_timing` with `TAIYAKU_MATCH_NAS_ROOT` pointing there (release). Keep the output as `$EV/nas/workstation.log`; it is the workstation figure for the same sample.
  - Run the crate's tests, clippy, fmt and `check-comments`. Commit: "Time the content-matching sample's fingerprints and the vote kernel on any read-only root".
- [ ] **Step 2: Build for the NAS.** Follow `taiyaku-sqlite-340-in-ci` memory's recipe (git archive HEAD into `target/bookworm/src-<hash>`, `rust:1-bookworm`, the host's pinned toolchain mounted per the memory), running `cargo test --release --locked --no-run -p taiyaku-cli`. Take the binary path from the `Executable unittests src/main.rs (…)` line, and record its sha256 in `$EV/nas/binary.txt`.
- [ ] **Step 3: NAS preflight, read-only.** Run `ssh -o BatchMode=yes root@192.168.1.69 'date -u; uptime; nproc; free -g; mdcmd status | grep -E "^mdResync="; docker ps --format "{{.Image}} {{.Names}}"'` into `$EV/nas/preflight.txt`. Stop and report if `mdResync` is not 0 or the 1-minute load is above 8 (a scan or import is running). Note the taiyaku image's ID.
- [ ] **Step 4: Run in a container.**
  - The one remote command, with the binary on stdin and output on stdout, in the shape: `ssh root@NAS "docker run --rm -i --read-only --network none --tmpfs /work:rw,exec,size=1g -v '/mnt/user/media 1:/media:ro' --entrypoint sh <taiyaku image> -c 'cat > /work/t && chmod +x /work/t && TAIYAKU_MATCH_NAS_ROOT=/media nice -n 19 /work/t match_experiment::nas_timing --exact --nocapture && TAIYAKU_MATCH_NAS_ROOT=/media RAYON_NUM_THREADS=4 nice -n 19 /work/t match_experiment::nas_timing --exact --nocapture'" < <binary> >| $EV/nas/run.log 2>&1`, with a `timeout` large enough for the run.
  - If the loader reports a missing library, stop and report; install nothing on the NAS.
  - Afterwards, `docker ps -a` on the NAS shows no leftover container, and `run.log` shows the `ro` mount line.
- [ ] **Step 5: Sizes, read-only.** `ssh … 'du -sb "/mnt/user/media 1/Manga/"* "/mnt/user/media 1/Comics/"*'` goes into `$EV/nas/du.txt`. Disks may spin up, so pass a `timeout`.
- [ ] **Step 6: Extrapolate** per Task 4 item 10 in a scratchpad script, writing `$EV/nas/extrapolation.txt`. The inputs:
  - The cold read rate: median of run 1's digest rates, excluding its first member, which carries any spin-up; the spin-up is reported on its own.
  - Fingerprint seconds per byte per class: run 1 for all cores, run 2 for 4 threads.
  - Slots per byte per class, from `members.tsv`.
  - The kernel rates.
  - A directory with no measured class (Ecchi, Extras, Graphic Novels, Webcomics) uses the nearest class and is flagged.
  - Give per-directory hours, the per-library sums (Manga = Volumes + Raw + Chapters; Ecchi; Comics = Collected + Graphic Novels + Webcomics + Issues), and the total at 4 threads and at all cores.
- [ ] **Step 7:** Write the notes subsection `### On the NAS`: the host's cores and load, the sample, both runs' rates, the kernel, the extrapolation table, and the one-day verdict. Commit `docs/notes.md`: "What fingerprinting the NAS library would cost, measured in a container on the NAS".

### Task 9: Results, status rows, isolation proof and the gate

**Files:** Modify `docs/notes.md` (subsection `### What it decides`) and `docs/status.md`.

- [ ] **Step 1: "What it decides".** State the score (V, C), each bar, whether chapters can go automatic, the pack and comics verdicts, the lookup's depth (from P1/P2: the candidates per member PR 3 should align), the NAS first-pass hours, and, if over a day, that the owner is asked before PR 2. Each sentence names the subsection its figure comes from.
- [ ] **Step 2: O-numbers.**
  - Lane 2 holds O301–O309, and `first-scan` takes O310 and up. Read `git show main:docs/status.md` and `git show first-scan:docs/status.md` (`grep -o '^| O[0-9]*'`), `git grep -o 'O3[0-9][0-9]' first-scan -- docs/`, and the What's-next memory's newest section. Take the lowest numbers after every reservation, and confirm them with the controller before writing.
  - If `first-scan` already opens rows for "chapters make no work" or "issues never join collected editions", point the project row at those rows instead of duplicating them.
- [ ] **Step 3: Open the rows** with the five-part opening (`taiyaku-open-items-ritual` memory): the count word at the head of Open items ("Three hundred and one, …"), the "leaving N" sentence, the table rows (symbol sites, six `|` per row, `\|` inside code), a prose bullet each, and the "What remains" line, whose count word wraps, so `grep -n -A1 '^What remains:'`. Then grep each new bare O-number across `docs/` and read every hit. The rows:
  - **(a) the project:** editions are matched only by name. That covers copies joined end to end (`derive::place`), chapter-only directories making no work (`proposals::accept` refusing `attach-chapters`), and series matched only by exact title. The row gives this PR's result in a clause and points at the design and the notes section.
  - **(b) the omnibus follow-up:** an omnibus edition is never linked to the ordinary volumes it covers (design ruling 5), site: the `alignment` table, whose rows sit inside one work.
  - **(c) any defect the runs found,** each its own row.
- [ ] **Step 4: Isolation proof.**
  - `find /home/davey/temp/manga-test -newer $EV/isolation/marker 2>&1` prints nothing.
  - `stat -c '%n %s %Y %i' /home/davey/temp/manga-test/library.db` goes into `after.txt` and equals `before.txt`.
  - `git log --format='%h %ci %s' -- crates/taiyaku-cli/src/match_experiment/manifest.json docs/notes.md crates/taiyaku-cli/src/match_experiment/bars.json` shows the manifest and pre-registration commits earlier than `logs/fingerprint.log`'s first line, and `bars.json` earlier than `logs/score-held-out.log`'s.
  - Record all of it in `$EV/isolation/proof.txt`.
- [ ] **Step 5: No product change.** `git diff main --stat -- crates/` lists only `main.rs` and `match_experiment*`, and `git diff main -- crates/taiyaku-cli/src/main.rs` is the declaration and its comment.
- [ ] **Step 6: Gate.** As in Task 1 Step 6, into `$EV/gate-final.log`: `all green` and `exit=0`. Update the Rust test count at both sites if it moved.
- [ ] **Step 7: Commit** `docs/notes.md` and `docs/status.md`: "Content matching PR 1: the bars it measured, what goes automatic, the NAS cost, and the rows it opens". Check `git show --stat HEAD`.
- [ ] **Step 8: Claim check** (a fresh subagent, before anyone says "ready"): every figure in the notes section and the new status rows is checked against `$EV` (`scores.tsv`, `rules.txt`, `lookup.txt`, `nas/extrapolation.txt`) and the manifest. Corrections go in one commit, and a re-pass is scoped to that commit.

---

## Self-review

- **Spec coverage:**
  - Delivery 1's classes are covered by Task 2's labels and explicit pairs: raw↔EN, chapter↔volume, copy↔copy, pack↔volumes, N↔N±1, chapter↔neighbour, other series, Cover Stories, comics. The colour and fan variants are added.
  - "Write the decision rule down before running it" is Task 4; "Time fingerprinting on the NAS in a release build" is Task 8.
  - The result (the score, the bar, whether chapters go automatic) is Task 9 Step 1.
  - §3's knowns are honoured: margin alone is not the score; run structure is (long runs); chapters get their own test (C); N±1 is measured.
  - §7: comics pair, Cover Stories as a wrong class, and the one-day trigger.
  - Ruling 5: omnibus pairs are reported only, and `is_omnibus_name` is the check that names decide.
- **Types:** `PairFeatures`, `Label`, `Member`, `Rule` and `Split` are used with the same names and fields from their defining tasks onward. `class` (Task 2) consumes `PairFeatures` (Task 1).
- **Review Focus:** each item's test sits in its owning task (1: Task 5; 2: Task 2; 3, 4, 5: Task 1).

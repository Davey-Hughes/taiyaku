# Content Matching Volume Test (O340) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure, on 157 volumes never scored by this measurement, whether a volume-against-volume rule frozen on PR 1's scores can let PR 3 apply volume matches automatically.

**Architecture:** A new `volume_test` module inside the `#[cfg(test)]` harness `match_experiment`. It names the members and what each pair is, derives the bar from PR 1's scores and freezes it in its own file, proves the current code reproduces PR 1's scores on a sample of already scored pairs, then fingerprints the test series once and judges all 12,246 pairs against the frozen bar. Nothing in the product changes.

**Tech Stack:** Rust; the existing harness (`print_all`, `score_pairs`, `aligned`, `pairing_sheet`, `Log`, `stamp`); `serde_json` and `sha2`, both already dependencies of `taiyaku-cli`.

**Spec:** `docs/superpowers/specs/2026-11-18-content-matching-volume-test-design.md`

## Global Constraints

**Order of work. This is the point of the measurement.**

- This measurement reads or fingerprints no page of a test series before the commit that freezes the bar (Task 4), and that includes smoke runs. Before it, this measurement touches the test directories only by `read_dir` in `draft_volume_test_members`, which opens no archive.
- Two disclosed exceptions came before the design was written. The RuriDragon repack: its four raw archives were unpacked and repacked as zip, and each page file's bytes were hashed to prove the repack identical (`$EV/ruri-1-sha256.txt` to `ruri-4-sha256.txt`), with no RuriDragon page decoded, fingerprinted or scored. Tongari's earlier work: early project work read some of its volumes, as the ruling below sets out. The pre-registration and the result both state both exceptions.
- Tests before the freeze use synthetic data or PR 1's already scored series only.
- After the freeze commit, no commit touches `volume_test/bar.json`. A low score is reported, never adjusted.

**Rulings, not to be reopened.**

- A different-split pair is a WRONG pair even where the volume numbers agree: every Parasyte Colored Council volume against either 8-volume Parasyte edition (160 pairs) and every Yu Yu Hakusho Colored Council volume against the black and white edition (76 pairs), 236 in all.
- True pairs are 60: Tongari raw and English v01 to v13, RuriDragon raw and English v01 to v03, Hunter × Hunter black and white and colour v01 to v36, Parasyte black and white and Full Color Collection v01 to v08.
- Damaged applies to exactly one PAIR: Hunter × Hunter colour v35 against black and white v35. It is outside pass and fail and reported separately, leaving 59 true pairs that count: Tongari 13, RuriDragon 3, Hunter × Hunter 35 and Parasyte 8. The damaged volume's pairs with every other volume stay ordinary wrong pairs.
- Tongari raw v09 against English v09 is a true pair (owner's ruling, 2026-11-18): a complete volume of joined two-page spreads.
- Tongari stays in the test with its earlier work disclosed (owner's ruling, 2026-11-18). Its raw and English v01 and v09 went through the aligner in early project work (`docs/notes.md`, "Tongari confirms the pattern on a fifth series", "A letterbox has to come off both sides equally", and the rows `tongari-v01` and `tongari-v09-letterbox`), and the rule that trims page borders was designed on v09. An early survey also measured volume 13 for gatefolds (`docs/notes.md`, "No gatefolds exist in anything measured"). V was never computed on these volumes, and the bar comes only from PR 1's scores. The pre-registration and the result both state this, and the report shows Tongari's true pairs on their own line.
- Every other pair of the 12,246 is a wrong pair, including Tongari raw v16, Tongari English v14, RuriDragon raw v05 and Hunter × Hunter colour v37, which have no partner.
- Bar = the highest V of any volume-rule negative among pairs whose two members are both `Kind::Volume` in PR 1's dev and held-out scores, plus `rule.rs`'s `GAP`. The harness computes it; no code or test hardcodes its value.
- Pass: no wrong pair at or above the frozen bar, and at least one wrong pair exists. Any wrong pair at or above the bar goes to the owner one pair at a time with its pages beside it. Relabels are counted and reported. The bar never moves.
- No product change. No names or solver verdict rescue a pair. PR 1's `bars.json` and `manifest.json` do not change, and PR 1's behaviour stays identical.
- O340 stays open after this work: the owner has still to rule on pack cuts.

**Paths and commands.** Every command uses absolute paths, because `cd` is zoxide in this shell. `$EV/logs` is created once, in Task 1.

```sh
WT=/home/davey/projects/taiyaku/.claude/worktrees/content-matching-o340
EV=/home/davey/projects/taiyaku/.superpowers/sdd/content-matching-o340-evidence
PR1=/home/davey/projects/taiyaku/.superpowers/sdd/content-matching-evidence
LIB=/home/davey/temp/manga-test
```

- **Standing tests:** `CARGO_TARGET_DIR=$WT/target cargo test --manifest-path $WT/Cargo.toml -p taiyaku-cli match_experiment::volume_test -- --test-threads=8`
- **A gated test `<name>`** (one at a time, in release, into its own log):

```sh
TAIYAKU_MATCH_LIBRARY=$LIB TAIYAKU_MATCH_EVIDENCE=$EV TAIYAKU_MATCH_PR1_EVIDENCE=$PR1 \
CARGO_TARGET_DIR=$WT/target RAYON_NUM_THREADS=8 \
cargo test --manifest-path $WT/Cargo.toml --release -p taiyaku-cli \
  match_experiment::volume_test::<name> -- --exact --nocapture --test-threads=8 >| $EV/logs/<name>.log 2>&1
```

- **The gate:** `env -C $WT -u TAIYAKU_CORPUS -u TAIYAKU_MATCH_LIBRARY CARGO_TARGET_DIR=$WT/target NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $EV/logs/gate.log 2>&1`. It passes when the command exits 0 and the log ends in exactly these two lines: `all green, except NOT MEASURED:` and `  - standing corpus checks (TAIYAKU_CORPUS unset)`. Never pipe it through `tail`. The worktree has no `web/node_modules`: copy the main checkout's before the first gate, never symlink it.

**Code rules.** A comment says why, never history or a measurement (`scripts/check-comments.mjs` holds a budget). Harness evidence goes under `$EV`, never in the repository and never under `$LIB`. Notes and status cite no branch-only commit hash: describe the commit, or give its UTC time.

## Review Focus

1. **A name the parser misreads.** `第05巻` has no `v` marker, so `draft_member` leaves its number empty and PR 1's labels would drop its pairs silently. Pinned by `awkward_names_read_their_numbers` (Task 1).
2. **Editions whose numbers agree but whose volumes differ.** PR 1's `label` calls these `SameVolume`. Pinned per pair by `pair_classes_by_example` (Task 1).
3. **Fingerprint code that no longer matches the code behind the bar.** PR 2 moved the fingerprint functions from `taiyaku-cli` into `taiyaku-serve` and changed how `taiyaku-read` lists a directory, so the control's sample includes two image directories. Stopped by `control_compares_every_field` (Task 2) and by the control that opens the real run (Task 3).
4. **A wrong pair scoring exactly the bar, or a verdict used to hide one.** Pinned by `a_wrong_pair_exactly_at_the_bar_fails` and `a_verdict_below_the_bar_or_on_a_true_pair_is_refused` (Task 3).
5. **A rerun or partial read presented as a clean result.** A used fingerprint cache or an unreadable member stops the run. Pinned by `the_run_refuses_a_used_fingerprint_cache` and `an_unreadable_member_stops_the_run_and_writes_no_report` (Task 3).

## How each hazard is handled

| Hazard | Handling | Task |
| --- | --- | --- |
| Order of operations | `score_volume_test` refuses unless `bar.json` matches HEAD, one commit touches it, and the module is clean; `bar.json` also holds the sha256 of the members and of every pair's class | 3, 4 |
| Comparability | Ten named PR 1 pairs are rescored from a fresh cache; every compared field must equal `scores.tsv`, or the run stops before any test page is read | 2, 3, 5 |
| Naive labels | `pair_class` replaces `label` for this test; counts and examples are pinned over the committed `members.json` | 1 |
| Name parsing | All 157 names were traced through `volume_at`: 156 read correctly, `v01s` reads as 1 with copy `s`, which no label reads, and `第05巻` reads as nothing, so `number_of` adds a fallback for that form alone | 1 |
| Trailing space | Kept in `ROWS` and pinned on a full member path; nothing on the path from `members.json` to `library.join` trims | 1 |
| Mirrored art | `Member.ltr` only orders the two halves of a split spread (`slot_hashes`, `expand`) and mirrors nothing, so no setting can rescue a mirrored edition. Every member is `ltr: false`, as for every manga row in PR 1. A low true pair is listed with its V and solver verdict and left alone | 1, 3 |
| Isolation | `library.db`'s sha256 and a `find` against a start marker, before and after the run | 5 |
| Machine load | The worktree's own target directory, `--test-threads=8`, `RAYON_NUM_THREADS=8` | all |
| Docs | Notes hold the pre-registration and the result; O340's row and bullet are updated and it stays open | 4, 5 |

---

### Task 1: The 157 members and what each pair is

**Files:**
- Create: `crates/taiyaku-cli/src/match_experiment/volume_test.rs`, `volume_test/members.rs`, `volume_test/members.json`, `volume_test/tests.rs` (all under `match_experiment/`)
- Modify: `crates/taiyaku-cli/src/match_experiment.rs` (add `mod volume_test;` to the module list)

**Interfaces:**
- Consumes: `manifest::{DirRow, Kind, Member, Manifest, Source, Split, ARCHIVES, TABLE, draft_member}`
- Produces, in `members.rs`:
  - `TestMember { path: String, series: String, edition: String, number: u32 }` (serde)
  - `ROWS: [DirRow; 11]`
  - `number_of(name: &str) -> Option<u32>`
  - `draft(library: &Path) -> Result<Vec<TestMember>, String>`
  - `committed() -> Vec<TestMember>`, parsing `include_str!("members.json")`
  - `enum PairClass { True, Damaged, DifferentSplit, Wrong }`
  - `pair_class(a: &TestMember, b: &TestMember) -> PairClass`
  - `all_pairs(n: usize) -> Vec<(usize, usize)>`, each pair once with `i < j`
  - `to_manifest(members: &[TestMember]) -> Manifest`: `Kind::Volume`, `volumes: Some((n, n))`, `ltr: false`, `franchise` equal to `series`, no adjudications

`ROWS`, each with `franchise` equal to `series`, `kind: Some(Kind::Volume)`, `ltr: false` and `split: Split::HeldOut`, because nothing in this run reads the split:

| `dir` | `series` | `edition` | Numbers |
| --- | --- | --- | --- |
| `Raw/Tongari Boushi no Atelier` | `tongari` | `raw` | 1 to 13, 16 |
| `Volumes/Tongari Boushi no Atelier` | `tongari` | `en` | 1 to 14 |
| `Raw/RuriDragon` | `ruridragon` | `raw` | 1, 2, 3, 5 |
| `Volumes/RuriDragon` | `ruridragon` | `en` | 1 to 3 |
| `Volumes/Hunter × Hunter` | `hunter-x-hunter` | `bw` | 1 to 36 |
| `"Volumes/Hunter × Hunter (Official Colored) "` (ends in a space) | `hunter-x-hunter` | `colour` | 1 to 37 |
| `Volumes/Parasyte` | `parasyte` | `bw` | 1 to 8 |
| `Volumes/Parasyte - Full Color Collection` | `parasyte` | `collection` | 1 to 8 |
| `Volumes/Parasyte - Full Color (Colored Council)` | `parasyte` | `colored-council` | 1 to 10 |
| `Volumes/Yu Yu Hakusho` | `yu-yu-hakusho` | `bw` | 1 to 19 |
| `Volumes/Yu Yu Hakusho - Colored (Colored Council)` | `yu-yu-hakusho` | `colored-council` | 1 to 4 |

`pair_class` answers in this order, the same whichever member comes first:

1. **DifferentSplit:** Parasyte `colored-council` against Parasyte `bw` or `collection`; Yu Yu Hakusho `colored-council` against Yu Yu Hakusho `bw`.
2. **Damaged:** Hunter × Hunter `colour` 35 against `bw` 35.
3. **True:** same series and number, in editions `raw` and `en` (Tongari, RuriDragon), `bw` and `colour` (Hunter × Hunter), or `bw` and `collection` (Parasyte).
4. **Wrong:** everything else.

- [ ] **Step 1: Write the failing tests in `volume_test/tests.rs`**

```rust
#[test]
fn members_are_the_157_the_rulings_name() {
    // For each ROWS directory: the sorted numbers in members.json equal the table's,
    // and the total is 157.
}

#[test]
fn pairs_are_counted_once_with_their_classes() {
    let m = committed();
    let pairs = all_pairs(m.len());
    assert_eq!(pairs.len(), 12_246);
    assert_eq!(pairs.iter().collect::<BTreeSet<_>>().len(), 12_246);
    // Tally pair_class over the pairs, asserting pair_class(a, b) == pair_class(b, a).
    assert_eq!((t, d, s, w), (59, 1, 236, 11_950));
}

#[test]
fn awkward_names_read_their_numbers() {
    for (name, n) in [
        ("[眞藤雅興] ルリドラゴン 第05巻.cbz", 5),
        ("DLRAW.NET-Ruri Dragon v01s.cbz", 1),
        ("ルリドラゴン v02 DL-Raw.Se.cbz", 2),
        ("Yu Yu Hakusho - v04 - Colored (Colored Council).cbz", 4),
        ("Parasyte - Full Color v10 (2021) (Colored Council).cbz", 10),
        ("Hunter × Hunter (Official Colored) - v37.cbz", 37),
        ("Tongari Boushi no Atelier v16", 16),
    ] {
        assert_eq!(number_of(name), Some(n), "{name}");
    }
    assert_eq!(number_of("Parasyte - Full Color (2021).cbz"), None);
    // PR 1's reader stays as it was: draft_member still gives 第05巻 no number.
    assert_eq!(draft_member("[眞藤雅興] ルリドラゴン 第05巻.cbz", &ROWS[2]).volumes, None);
}

#[test]
fn pair_classes_by_example() {
    use PairClass::{Damaged as D, DifferentSplit as S, True as T, Wrong as W};
    let cases = [
        (("tongari", "raw", 13), ("tongari", "en", 13), T),
        (("tongari", "raw", 9), ("tongari", "en", 9), T),
        (("tongari", "raw", 9), ("tongari", "en", 8), W),
        (("tongari", "raw", 16), ("tongari", "en", 14), W),
        (("ruridragon", "raw", 1), ("ruridragon", "en", 1), T),
        (("ruridragon", "raw", 5), ("ruridragon", "en", 3), W),
        (("hunter-x-hunter", "colour", 36), ("hunter-x-hunter", "bw", 36), T),
        (("hunter-x-hunter", "colour", 35), ("hunter-x-hunter", "bw", 35), D),
        (("hunter-x-hunter", "colour", 35), ("hunter-x-hunter", "bw", 34), W),
        (("hunter-x-hunter", "colour", 37), ("hunter-x-hunter", "bw", 36), W),
        (("parasyte", "bw", 8), ("parasyte", "collection", 8), T),
        (("parasyte", "colored-council", 1), ("parasyte", "bw", 1), S),
        (("parasyte", "colored-council", 8), ("parasyte", "collection", 8), S),
        (("parasyte", "colored-council", 10), ("parasyte", "bw", 8), S),
        (("parasyte", "colored-council", 1), ("parasyte", "colored-council", 2), W),
        (("yu-yu-hakusho", "colored-council", 1), ("yu-yu-hakusho", "bw", 1), S),
        (("yu-yu-hakusho", "colored-council", 4), ("yu-yu-hakusho", "bw", 19), S),
        (("yu-yu-hakusho", "bw", 1), ("yu-yu-hakusho", "bw", 2), W),
        (("tongari", "raw", 1), ("hunter-x-hunter", "bw", 1), W),
    ];
    // Find each member in committed() and assert the class, both ways round.
}

#[test]
fn the_colour_directory_keeps_its_trailing_space() {
    // committed() holds exactly this path:
    // "Volumes/Hunter × Hunter (Official Colored) /Hunter × Hunter (Official Colored) - v01.cbz"
    // and ROWS holds exactly this dir, because editing ROWS does not regenerate members.json:
    // "Volumes/Hunter × Hunter (Official Colored) "
}

#[test]
fn the_series_are_new_and_none_is_left_to_right() {
    // No ROWS dir is a dir in manifest::TABLE, and every to_manifest member has ltr == false.
}
```

- [ ] **Step 2: Run the standing tests and verify they fail**

Expected: FAIL to compile, as `members` does not exist.

- [ ] **Step 3: Implement `members.rs`, and the gated `draft_volume_test_members` in `volume_test.rs`**

`number_of` asks `draft_member` first and takes a number only where the first and last volume agree. Only when that finds none does it read the digits between `第` and `巻`. `manifest.rs` does not change. `draft` reads each row with `read_dir`, skips hidden entries, keeps directories and `ARCHIVES` files, and returns an error naming any entry with no number or a number repeated in its directory. The gated test writes the sorted list, pretty-printed, to `volume_test/members.json` and prints each directory's count. Create `members.json` holding `[]` so the crate compiles before the draft fills it.

- [ ] **Step 4: Draft the members from names alone**

Run: `mkdir -p $EV/logs`, then the gated test `draft_volume_test_members`.
Expected: exit 0; the log shows the eleven directory counts of the table, summing to 157; `rg -c '"path"' $WT/crates/taiyaku-cli/src/match_experiment/volume_test/members.json` gives 157.

- [ ] **Step 5: Run the standing tests and verify they pass**

Expected: PASS.

- [ ] **Step 6: Watch the pins catch their defects**

Hand-apply each, run the standing tests, see the named test fail, restore: drop the DifferentSplit branch (`pair_classes_by_example`, `pairs_are_counted_once_with_their_classes`); check True before Damaged (the same two); remove the `第` fallback (`awkward_names_read_their_numbers`); trim the row's trailing space (`the_colour_directory_keeps_its_trailing_space`).

- [ ] **Step 7: Commit**

`git -C $WT add crates/taiyaku-cli/src/match_experiment.rs crates/taiyaku-cli/src/match_experiment/volume_test.rs crates/taiyaku-cli/src/match_experiment/volume_test`, then commit with the message "The volume test's 157 members and what each pair is (O340)".

---

### Task 2: The bar from PR 1's scores, and the control

**Files:**
- Create: `volume_test/bar.rs`, `volume_test/control.rs`, `volume_test/control_pairs.json`
- Modify: `match_experiment/held_out.rs` (`frozen`, `one_freeze`, `provenance`), `volume_test.rs`, `volume_test/tests.rs`

**Interfaces:**
- Consumes: Task 1's `TestMember`, `ROWS`, `all_pairs`, `pair_class`; `rule::{GAP, dev_outcome, label_name}`; `features::{PairFeatures, volume_score}`; `manifest::label`; `lookup::print_member`; `score::score_pairs`
- Produces, in `held_out.rs`: `frozen_file(name: &str, compiled: &str, committed: Option<&str>) -> Result<(), String>`, `one_commit(name: &str, git_log: Option<&str>) -> Result<String, String>`, `provenance_of(name: &str, git_log: Option<&str>) -> String`. The three existing functions become one-line calls passing `"bars.json"`, and their existing tests stay unchanged.
- Produces, in `bar.rs`:
  - `Pr1Row { a, b, label, class, verdict, v_cell: String, a_slots, b_slots, paired, long_paired: u32, runs: usize }`, with `v(&self) -> f64` from `volume_score`
  - `read_pr1(text: &str) -> Result<Vec<Pr1Row>, String>`
  - `derive(m: &Manifest, dev: &[Pr1Row], held_out: &[Pr1Row]) -> Result<Derived, String>`, where `Derived { max_negative: f64, bar: f64, pair: Pr1Row }`
  - `FrozenBar { max_negative, gap, bar: f64, pair_a, pair_b: String, long_paired, a_slots, b_slots: u32, pr1_dev_sha256, pr1_held_out_sha256, members_sha256, pairs_sha256: String }` with `deny_unknown_fields`
  - `read_frozen(text: &str) -> Result<FrozenBar, String>`
  - `sha256_hex(text: &str) -> String`
  - `pairs_text(members: &[TestMember]) -> String`: one `a<TAB>b<TAB>class` line per pair, in `all_pairs` order
- Produces, in `control.rs`: `differs(row: &Pr1Row, f: &PairFeatures, runs: usize) -> Option<String>` and `run(library: &Path, ev: &Path, pr1: &Path, log: &mut Log) -> Result<(), String>`

**Why V is read from integers.** `scores.tsv` prints V to six places, so `Pr1Row::v` recomputes it from `long_paired` and the slot counts, and `read_pr1` refuses a row whose `v_cell` is not that value printed to six places.

**`derive`** maps each path to PR 1's committed `manifest.json` and refuses: a path the manifest lacks; a row with a member whose parent directory equals a `ROWS` `dir`, since the test series must be unscored; a volume-against-volume row whose `label` cell differs from `label_name(label(m, a, b))`; and no bar fitting. It keeps rows where both members are `Kind::Volume` and the `volume` class cell is `negative`, and takes the bar from `dev_outcome`.

**`control_pairs.json`** holds these ten pairs, as `[{"a": …, "b": …}]`, with the paths written out in full:

| Role | Split | A | B |
| --- | --- | --- | --- |
| The negative that sets the bar | dev | `Volumes/[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v10.cbz` | `…/[Oshi no Ko] - v12.cbz`, same directory |
| Negative, same series | dev | `Volumes/[Oshi no Ko]/[Oshi no Ko] - v02 (2023).cbz` | `…/[Oshi no Ko] - v09 (2025).cbz`, same directory |
| Negative, other series | dev | `Volumes/Megami no Cafe Terrace/Megami no Cafe Terrace - v11 (2024).cbz` | `Volumes/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v11 (2019).cbz` |
| Negative | held-out | `Volumes/Bocchi the Rock!/Bocchi the Rock! - v03 (2024).cbz` | `…/Bocchi the Rock! - v07 (2025).cbz`, same directory |
| True, an image directory against an archive | dev | `Raw/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen/Kaguya-sama Zunousen v01` | `Volumes/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v01 (2018).cbz` |
| True, colour | dev | `Volumes/Chainsaw Man/Chainsaw Man - v07 (2021).cbz` | `Volumes/Chainsaw Man - Digital Colored Comics/Chainsaw Man - Digital Colored Comics v07 (2022) (Colored Council).cbz` |
| True, fan | dev | `Volumes/[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz` | `Volumes/[Oshi no Ko]/[Oshi no Ko] - v01 (2023).cbz` |
| True, which the solver refused | dev | `Raw/Kobayashi-san Chi no Meidoragon/DLRAW.TO_Kobayashi-san Maid Drago v04` | `Volumes/Kobayashi-san Chi no Meidoragon/Kobayashi-san Chi no Meidoragon - v04 (2017).cbz` |
| True, raw against English | held-out | `Volumes/The Legend of Zelda - Twilight Princess/The Legend of Zelda - Twilight Princess - v10 (2022).cbz` | `Raw/Zelda no Densetsu - Twilight Princess/v10.cbz` |
| True, colour | held-out | `Volumes/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2018).cbz` | `Volumes/Go-Toubun no Hanayome - Full Color Edition/Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz` |

**`control::run`** fingerprints those members into a new directory, `$EV/control/fp-cache-<unix seconds>`, so no earlier entry can answer. It refuses any cache hit, finds each pair in PR 1's two `scores.tsv` files whichever way round it was written, refuses a sample that lacks the pair `derive` names, scores with `score_pairs`, logs one line per pair and ends with `control: 10 of 10 pairs reproduced`. Any difference is an `Err` naming the pair and both values.

- [ ] **Step 1: Write the failing tests in `volume_test/tests.rs`**

- `pr1_rows_take_v_from_their_integers`: a fixture row with `long_paired` 1 and slots 3 and 3 gives exactly `1.0 / 3.0`; the same row with `v_cell` `0.333334` is an `Err`.
- `the_bar_is_the_highest_volume_negative_plus_the_gap`: fixture rows with a volume-volume negative at 0.5, a higher chapter-volume negative, a higher volume-volume positive and a higher pack negative give `max_negative == 0.5` and `bar == 0.5 + GAP`.
- `the_bar_refuses_what_it_cannot_trust`: one `Err` each for a path outside the manifest, a row with a member whose parent directory equals a `ROWS` `dir`, a label cell that disagrees, no negative at all, and a negative so high that no bar fits. A row in `Volumes/Hunter × Hunter - Kurapika's Memories/`, held in the fixture's manifest, is not refused.
- `a_frozen_bar_file_reads_whole`: a missing key and an unknown key are each an `Err`.
- `control_compares_every_field`: `differs` gives `None` on equal values and `Some` when `v_cell`, `a_slots`, `b_slots`, `paired`, `long_paired`, `runs` or `verdict` alone differs, one case each.
- `control_pairs_are_ten_scored_volumes`: ten distinct pairs, and every path is a `Kind::Volume` member of PR 1's `manifest.json`.
- `the_frozen_bar_is_its_own_arithmetic`: when `volume_test/bar.json` exists, read at run time, `bar == max_negative + GAP`, `gap == GAP`, `max_negative == volume_score(long_paired, a_slots, b_slots)`, and both sha256 values match `members.json` and `pairs_text(&committed())`. After reading the file the test prints `frozen bar: <bar>`. When it is absent the test returns.

- [ ] **Step 2: Run the standing tests and verify they fail**

Expected: FAIL to compile, as `bar` and `control` do not exist.

- [ ] **Step 3: Implement `held_out.rs`'s three generalised guards, `bar.rs`, `control.rs`, `control_pairs.json`, and the gated `control_rescoring` in `volume_test.rs`**

`control_rescoring` reads `TAIYAKU_MATCH_PR1_EVIDENCE` through `refuse_unusable` and logs to `$EV/logs/control.log`.

- [ ] **Step 4: Run the standing tests, then every `match_experiment` test**

Run the standing tests, then the same command with the filter `match_experiment::`.
Expected: PASS, with PR 1's `frozen`, `one_freeze` and `provenance` tests unedited.

- [ ] **Step 5: Run the control on PR 1's series**

Run the gated test `control_rescoring`. It reads PR 1's series only.
Expected: exit 0 and `control: 10 of 10 pairs reproduced` in `$EV/logs/control.log`.

**If any pair differs, stop.** Do not start Task 3. Report the pair and both values to the owner: the bar cannot be compared with scores from this code, and what happens next is the owner's decision.

- [ ] **Step 6: Commit**

Message: "The volume test's bar comes from PR 1's scores, and a control proves the code still reproduces them (O340)".

---

### Task 3: The run, its judgement and its report

**Files:**
- Create: `volume_test/run.rs`, `volume_test/verdicts.json` (holding `[]`)
- Modify: `volume_test.rs`, `volume_test/tests.rs`

**Interfaces:**
- Consumes: Tasks 1 and 2; `lookup::{print_all, members_tsv}`; `score::{score_pairs, sheet_pages}`; `features::aligned`; `sheet::{pairing_sheet, SheetPair, Question}`; `manifest::Adjudication`; `rule::held_out_failures`
- Produces, in `run.rs`:
  - `PairRow { a, b: String, class: PairClass, edition_kind: String, v: f64, long_paired, a_slots, b_slots: u32, verdict: String, separation: String }`, a true pair's `edition_kind` being one of `tongari raw-en`, `ruridragon raw-en`, `hunter-x-hunter bw-colour` and `parasyte bw-collection`
  - `enum Outcome { Passes { relabels: usize }, Fails, Untested }`
  - `Judged { outcome: Outcome, wrong: usize, at_bar: Vec<usize>, relabelled: Vec<usize> }`, its lists being positions in the rows
  - `preflight(compiled: Option<&str>, committed: Option<&str>, touching: Option<&str>, commit: Option<&str>, members_json: &str, members: &[TestMember], pr1_dev_scores: &str, pr1_held_out_scores: &str, stamp: &str) -> Result<(FrozenBar, String), String>`, the `String` being the log's provenance lines. `score_volume_test` reads `bar.json` at run time and passes `None` as `compiled` when it is absent, because the file does not exist to compile in until Task 4
  - `cache_is_unused(dir: &Path) -> Result<(), String>`
  - `judge(rows: &[PairRow], bar: f64, verdicts: &[Adjudication]) -> Result<Judged, String>`
  - `score(library: &Path, ev: &Path, members: &[TestMember], frozen: &FrozenBar, verdicts: &[Adjudication], log: &mut Log) -> Result<Judged, String>`
  - `rejudge(ev: &Path, frozen: &FrozenBar, verdicts: &[Adjudication]) -> Result<Judged, String>`
- Produces, gated in `volume_test.rs`: `freeze_volume_bar`, `score_volume_test`, `rejudge_volume_test`, `volume_test_sheets`

**`judge`.** A wrong pair is `Wrong` or `DifferentSplit`. `True` and `Damaged` pairs never affect the outcome. An owner's "same" verdict on a `Wrong` pair at or above the bar takes it out of the wrong pairs and counts as a relabel; a "different" verdict on a wrong pair at or above the bar keeps it. A "same" verdict on a `DifferentSplit` pair is an `Err`, because the owner has ruled that pair wrong. A verdict on any other pair, on a path that is no member, or on a pair below the bar is an `Err`. The outcome is `Fails` if any wrong pair remains at or above the bar, `Untested` if no wrong pair exists, and otherwise `Passes`.

**`score_volume_test` runs in this order, each step before the next:**

1. `preflight` refuses unless `bar.json` equals HEAD's copy, exactly one commit touches it, its `members_sha256`, `pairs_sha256`, `pr1_dev_sha256` and `pr1_held_out_sha256` match, the last two against the text of PR 1's two `scores.tsv` files, and `stamp` names a commit with no `+dirty` and no `unknown`. `cache_is_unused($EV/fp-cache)` refuses a cache holding any entry.
2. `Log::create($EV/logs/volume-test.log)` writes the stamp and provenance.
3. `control::run`. A failure ends the run here, before any test page is read.
4. `print_all` into `$EV/fp-cache`, then `members.tsv`, logging `cache hits: 0`. Any unreadable member or any cache hit is an `Err`, and no report is written.
5. `score_pairs` over `all_pairs`, then `$EV/scores.tsv` with each pair's class, `long_paired`, slots, V, solver verdict and separation.
6. `judge`, then `$EV/report.txt` and an identical `$EV/report-first.txt`, which `rejudge` never rewrites.

**`report.txt` holds, in order:** the stamp and provenance; the bar, and the pair that set it; the count of each class; the highest wrong pair; every wrong pair at or above the bar; the verdicts and relabels; true pairs clearing the bar, of 59 and by each of the four edition kinds; every true pair below the bar with its V, `long_paired`, slots, solver verdict and separation; the damaged pair, apart from the 59; the highest different-split pairs; and last, the line `verdict: passes`, `verdict: passes after N relabels`, `verdict: fails` or `verdict: untested`.

`freeze_volume_bar` refuses if `bar.json` exists, derives the bar, and writes the file with both PR 1 hashes, `members_sha256` and `pairs_sha256`. `rejudge_volume_test` rereads `$EV/scores.tsv` and `verdicts.json`, recomputes each row's V with `volume_score` from `long_paired` and the slots, refuses a row whose printed V disagrees, and rewrites `report.txt` only. `volume_test_sheets` creates `$EV/sheets` and writes `$EV/sheets/pair-NN.html`, one pair to a file through `pairing_sheet`, for each unjudged wrong pair at or above the bar.

- [ ] **Step 1: Write the failing tests in `volume_test/tests.rs`**

- `a_wrong_pair_exactly_at_the_bar_fails`: one `Wrong` row with `v == bar` gives `Fails`.
- `a_wrong_pair_just_below_the_bar_passes`: `v == f64::next_down(bar)` gives `Passes { relabels: 0 }`.
- `a_different_split_pair_at_the_bar_fails`.
- `no_wrong_pair_is_untested_not_a_pass`: only `True` rows give `Untested`.
- `true_and_damaged_pairs_never_fail_the_rule`: a `True` row below the bar and a `Damaged` row at it, beside one low `Wrong` row, give `Passes { relabels: 0 }`.
- `a_same_verdict_relabels_and_is_counted`: a "same" verdict on the only pair at the bar gives `Passes { relabels: 1 }` and names it in `relabelled`.
- `a_different_verdict_keeps_the_failure`.
- `a_verdict_below_the_bar_or_on_a_true_pair_is_refused`: one `Err` each for a pair below the bar, a `True` pair, a `Damaged` pair, a "same" verdict on a `DifferentSplit` pair at the bar and a path that is no member.
- `the_run_refuses_an_unfrozen_or_edited_bar`: one `preflight` `Err` each for no file, a file differing from HEAD, two commits touching it, a wrong `members_sha256`, a wrong `pairs_sha256`, a wrong `pr1_dev_sha256`, a wrong `pr1_held_out_sha256`, a `+dirty` stamp and an `unknown` stamp.
- `the_run_refuses_a_used_fingerprint_cache`: an empty or missing directory is `Ok`, and one holding a file is an `Err`.
- `an_unreadable_member_stops_the_run_and_writes_no_report`: fixture members built with `cbz_pages`, one of them a corrupt file; `score` is an `Err` and `report.txt` does not exist.
- `the_report_lists_each_true_pair_and_each_damaged_pair`: `score` over fixture archives named for one true, one damaged and one wrong pair; assert the true pair's row, the damaged pair's row under its own heading, and the last line, each by its whole line.
- `rejudge_keeps_the_first_report`: after a "same" verdict, `report.txt` shows the relabel and `report-first.txt` is byte for byte unchanged.

- [ ] **Step 2: Run the standing tests and verify they fail**

Expected: FAIL to compile, as `run` does not exist.

- [ ] **Step 3: Implement `run.rs` and the four gated tests**

- [ ] **Step 4: Run the standing tests and verify they pass**

Expected: PASS. No gated test has been run, and none is run against the test series in this task.

Run: `env -C $WT cargo fmt --all -- --check`, then `env -C $WT CARGO_TARGET_DIR=$WT/target cargo clippy -p taiyaku-cli --all-targets -- -D warnings`
Expected: exit 0 from both.

- [ ] **Step 5: Watch the pins catch their defects**

Hand-apply each, run the standing tests, see the named test fail, restore: `>=` to `>` in `rule::held_out_failures` (`a_wrong_pair_exactly_at_the_bar_fails`); leave `DifferentSplit` out of the wrong pairs (`a_different_split_pair_at_the_bar_fails`); return `Passes` for no wrong pair (`no_wrong_pair_is_untested_not_a_pass`); skip `cache_is_unused`'s file check (`the_run_refuses_a_used_fingerprint_cache`).

- [ ] **Step 6: Commit**

Message: "The volume test scores every pair against the frozen bar and reports each result (O340)".

---

### Task 4: Freeze the bar

**Files:**
- Create: `volume_test/bar.json`, written only by `freeze_volume_bar`
- Modify: `docs/notes.md` (a new section after "Edition matching by content: PR 1's measurement (2026-11-16)"), `docs/status.md` (the two Rust test counts)

**Interfaces:**
- Consumes: `freeze_volume_bar` and `the_frozen_bar_is_its_own_arithmetic`
- Produces: the freeze commit, which Task 5's `preflight` checks

- [ ] **Step 1: Confirm no test page has been read**

Run: `ls -A $EV/fp-cache`
Expected: no such directory. If it exists, stop and tell the owner.

- [ ] **Step 2: Write the pre-registration into `docs/notes.md`**

Add `## Edition matching by content: the volume-against-volume test (2026-11-18)` and, under it, `### Pre-registered before this measurement fingerprinted any page of the test series`. Cover, in this order: what is measured and why; the disclosed RuriDragon repack under Global Constraints, and that `Hunter × Hunter - Kurapika's Memories`, a separate one-volume book PR 1 scored, is not one of the test series; the earlier Tongari work disclosed under Global Constraints; the members and how a pair is classed, with the rulings under Global Constraints; how the bar is derived and where it is frozen; the control; the pass rule and how a wrong pair at the bar reaches the owner; what each result decides for PR 3, in the design's words; and what will not be adjusted once the run begins.

- [ ] **Step 3: Freeze the bar**

Run the gated test `freeze_volume_bar`.
Expected: exit 0, and `bar.json` names the pair `[Oshi no Ko] - Fan Translated` v10 and v12 with a bar of 0.87, the preview's 0.72 + 0.15.

**If the harness computes any other bar or pair, stop and report it to the owner.** Do not edit the file.

- [ ] **Step 4: Run the gate**

Add the new `#[test]` count to both Rust figures in `docs/status.md` ("Six crates, N tests" and "Twenty-six of the N are standing checks"), then run the gate.
Expected: exit 0 and a log ending in exactly these two lines: `all green, except NOT MEASURED:` and `  - standing corpus checks (TAIYAKU_CORPUS unset)`. `scripts/status-test-count.mjs` names the right count if the figure is off.

- [ ] **Step 5: Commit the freeze**

Add `bar.json`, `docs/notes.md` and `docs/status.md` in one commit: "Freeze the volume test's bar before any test series is read (O340)". Run `git -C $WT log -1 --format='%h %ct'` and save the output to `$EV/logs/freeze-commit.txt`.

- [ ] **Step 6: Prove the freeze holds**

Run: `git -C $WT log --format=%H -- crates/taiyaku-cli/src/match_experiment/volume_test/bar.json`
Expected: exactly one line.

Run: `git -C $WT diff --exit-code main...HEAD -- crates/taiyaku-cli/src/match_experiment/bars.json crates/taiyaku-cli/src/match_experiment/manifest.json`
Expected: exit 0, with no output.

Run: `CARGO_TARGET_DIR=$WT/target cargo test --manifest-path $WT/Cargo.toml -p taiyaku-cli the_frozen_bar_is_its_own_arithmetic -- --nocapture --test-threads=8`
Expected: exit 0, and the output shows `frozen bar: ` followed by the `bar` value in `bar.json`.

---

### Task 5: The real run, the owner's rulings and the result

**Files:**
- Modify: `volume_test/verdicts.json` (only with the owner's rulings), `docs/notes.md`, `docs/status.md`

**Interfaces:**
- Consumes: `score_volume_test`, `volume_test_sheets`, `rejudge_volume_test` and the freeze commit

- [ ] **Step 1: OWNER STEP. Get the owner's go-ahead for the real run**

Show the owner the bar, the pair that set it and the control's result. Raise that `Hunter × Hunter - Kurapika's Memories`, a separate one-volume book PR 1 scored, is not one of the test series, so the owner can drop Hunter × Hunter if they read that differently. Restate the earlier Tongari work disclosed under Global Constraints. Do not continue without their word.

- [ ] **Step 2: Record the library's state before the run**

Run: `sha256sum $LIB/library.db`
Expected: `19f284ec96d9df31d4fa32f1ea04ec764d139988d9456745daf8710796beb3b2`.

Run: `touch $EV/logs/run-start`, and confirm `$EV/fp-cache` is still absent.

- [ ] **Step 3: Run the measurement**

Run the gated test `score_volume_test`.
Expected: exit 0. `$EV/logs/volume-test.log` shows, in order, the stamp, the provenance, `control: 10 of 10 pairs reproduced` and `cache hits: 0`. `$EV/report.txt` ends with a `verdict:` line.

The run refuses to start on an uncommitted change anywhere in the repository, on any commit since the freeze that touches `crates/`, `Cargo.toml` or `Cargo.lock`, on a `bar.json` that PR 1's scores do not derive, on a verdict already in `verdicts.json`, and on `scores.tsv`, `scores.sha256`, `report.txt` or `report-first.txt` already in `$EV`. After the control and before the first fingerprint, it refuses a member holding an entry the reader would pass over that may be a page.

If the run stops at any of these, at the control, or on an unreadable member, tell the owner and change nothing.

- [ ] **Step 4: Prove the library was not written to**

Run: `sha256sum $LIB/library.db`
Expected: the same value as in Step 2.

Run: `find $LIB -newer $EV/logs/run-start >| $EV/logs/isolation-newer.txt`, then `find $LIB -cnewer $EV/logs/run-start >| $EV/logs/isolation-cnewer.txt`, then `wc -l $EV/logs/isolation-newer.txt $EV/logs/isolation-cnewer.txt`
Expected: 0 lines in both.

- [ ] **Step 5: OWNER STEP. Take every wrong pair at or above the bar to the owner, one pair at a time**

Skip this step if `report.txt` lists none. Otherwise run the gated test `volume_test_sheets` and show the owner `$EV/sheets/pair-01.html` first, with its pages beside each other. A `DifferentSplit` pair stands as wrong under the owner's ruling of 2026-11-18: say so when showing it, and record no "same" verdict for it. Allowing a relabel there would need a new ruling from the owner first. Record each ruling in `verdicts.json` as an `Adjudication` dated that day with `by: "owner"` and the owner's words in `note`, commit that file alone, because `rejudge_volume_test` and `volume_test_sheets` refuse any other commit since the freeze that touches `crates/`, `Cargo.toml` or `Cargo.lock`, run the gated test `rejudge_volume_test`, and go on to the next pair. "Can't tell" is recorded as `same: false` with those words in `note`, and an unanswered pair stays wrong. The bar never moves, `report-first.txt` stays as written, and every relabel appears in the result.

- [ ] **Step 6: Write the result into `docs/notes.md`**

Under the pre-registered subsection, add `### The result`, taking every figure from `report.txt`, `report-first.txt` or the logs. Cover: the control; the proof of order, giving the freeze commit's and the run's times in UTC and no branch hash, and restating the disclosed RuriDragon repack and the earlier Tongari work disclosed under Global Constraints; the isolation proof; the result against the bar, with the first report's verdict beside the final one if the owner relabelled; each wrong pair at the bar and the owner's ruling on it; true pairs clearing the bar, of 59 and by edition kind, Tongari's on their own line; every true pair below the bar, reported and unadjusted; the damaged pair, separately; and what the result decides for PR 3. A pass means PR 3 may apply volume matches at or above the bar automatically. A failure means every PR 3 match is a suggestion.

- [ ] **Step 7: Update O340 in `docs/status.md`, leaving it open**

Keep the row's `[ ]`. Replace "Not started." in the row with the result and a pointer to the notes section, and mark the row narrowed with its date. Rewrite O340's prose bullet to say what the test found and that the owner's ruling on pack cuts is what remains. Leave the open and closed counts as they are. Then run `rg -n "O340" $WT/docs $WT/web/src` and read every hit, and confirm the row still has exactly six `|` characters.

- [ ] **Step 8: Run the gate and check every claim**

Run the gate.
Expected: exit 0 and a log ending in exactly these two lines: `all green, except NOT MEASURED:` and `  - standing corpus checks (TAIYAKU_CORPUS unset)`.

Check each figure added to the notes and status against its evidence file, and grep the branch's added lines for 7 to 40 character hex strings that main cannot reach.
Expected: every figure traces to a file, and no branch-only hash is cited.

- [ ] **Step 9: Commit**

Message: "The volume-against-volume test's result (narrows O340)".

- [ ] **Step 10: OWNER STEP. Hand the result to the owner**

Report the verdict, the true pairs clearing the bar and any relabels. The owner decides what PR 3 does with the result and rules on pack cuts. O340 stays open until then.

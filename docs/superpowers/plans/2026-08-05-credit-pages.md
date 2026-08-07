# Skipping scanlation credit pages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A reader can keep a scanlation group's donation cards and staff-credit
pages out of the reading flow and open them deliberately from a panel, driven by
a `credits` list a new `taiyaku credits` command proposes and a person writes.

**Architecture:** Signal 1 — unpaired against the counterpart, split at member
boundaries, ending at a member's last page — is the classifier, and it is pure
sidecar arithmetic, so it lives in `taiyaku-core::credits` beside `route` and
`slot`. Signal 2 — dhash recurrence across sibling members — turned out to be a
*duplicate* detector rather than a credit detector, so it is computed in
`taiyaku-cli` and **printed as a diagnostic, never used to classify**. The list
is stored as a per-edition `credits` array on `Edition`, the shape `spreads`
already has, and reaches the browser through a new `Resolver.credits(edition)`
because `web/` deliberately has one sidecar parser. `taiyaku-serve` is untouched.

**Tech Stack:** Rust (`taiyaku-core`, `taiyaku-cli`, `taiyaku-wasm`, serde),
TypeScript/React 19 (`web/`), Vitest.

## Global Constraints

- `taiyaku-core` must keep compiling for `wasm32-unknown-unknown`. No
  `wasm-bindgen`, no I/O, no `std::time` in it.
- No new third-party dependency in any crate.
- Sites are named by symbol in `docs/`, never by line number.
- Every new refusal names the thing that was missing, in this tool's own words.
  A bare status or a Rust type name in a message is a defect (O5).
- The `credits` field is in **pages**, zero-based, like `spreads`. Runs and
  reader positions are in **slots**. Never mix them; `UnpairedRun` carries both
  so no call site has to convert.
- Commit per task. Keep behaviour changes off documentation-only commits.
- Run `cargo fmt --all` and `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` before every commit; both are CI gates.

---

## What the prototype measured

Both signals were written as throwaway scripts, run against
`corpus/gotoubun-v01`'s real sidecar and the CLI's real fingerprint cache, and
twelve candidate pages were extracted and viewed, **before this plan was
written**. Four things a plan that only asserted would have got wrong:

1. **`notes.md`'s table was mislabelled in two rows, and the design inherited
   both.** `c005` local 26 is a staff-credits page, not a "chapter end".
   `c005.5` locals 0–3 are the volume's own front matter — cover, title page,
   contents, half-title — not "scanlator title and credit pages". Both
   corrections are now in `notes.md` and in the spec; this plan is written
   against the corrected reading.
2. **An unpaired run must be split at member boundaries before its position
   means anything.** Unsplit, `c005` local 26 and `c005.5` locals 0–3 form one
   run of five whose first and last pages are both mid-member, so a rule reading
   flags off the endpoints sees no boundary at all and loses one true positive
   and four true negatives together.
3. **Only a member's *final* index counts.** With the split, the rule is four
   for four (pages 95, 168, 198, 225) and a first-index rule would be zero for
   four.
4. **Signal 2 cannot classify.** Its two tightest matches in the edition, at
   Hamming distance 2, are the volume cover carried by two chapter files. Its
   true positives sit at 14–24, not near zero, because dhash is unstable on
   near-black low-variance cards. It misses both staff-credit pages at distance
   99, because each appears once.

One consequence for testing: `en-fan`'s five spreads all fall *outside* every
unpaired region, so on this volume page indices and slot indices give the same
answer for all 50 unpaired positions. **The corpus cannot catch a page-for-slot
substitution**, which is why Task 3 carries a hand-built fixture that can.

## File structure

| File | Responsibility |
| ---- | -------------- |
| `crates/taiyaku-core/src/sidecar.rs` (modify) | the `credits` field on `Edition`; merge preservation in `merge_spoke` |
| `crates/taiyaku-core/src/credits.rs` (create) | `unpaired_runs` — signal 1, pure |
| `crates/taiyaku-core/src/lib.rs` (modify) | `mod credits;` and its re-exports |
| `crates/taiyaku-cli/src/credits.rs` (create) | signal 2, the dhash recurrence pass, and the command's table |
| `crates/taiyaku-cli/src/main.rs` (modify) | `credits` dispatch, flags, refusals |
| `crates/taiyaku-cli/src/corpus.rs` (modify) | the two gated corpus checks |
| `scripts/credit-survey.sh` (create) | the survey that fixes the bound |
| `scripts/check-all.sh` (modify) | run the new corpus checks |
| `crates/taiyaku-wasm/src/binding.rs` (modify) | expose `credits` across the JS boundary |
| `web/src/resolver.ts` (modify) | `credits(edition)` on the `Resolver` interface |
| `web/src/fixtures.ts` (modify) | the same, for tests that do not load wasm |
| `web/src/modules/credits.ts` (create) | the seventh `ReaderModule`: skipping and the panel |
| `web/src/modules/registry.ts` (modify) | register it |
| `docs/sidecar.md` (modify) | the `credits` paragraph |

---

### Task 1: The `credits` field on `Edition`

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (`Edition`)
- Modify: `docs/sidecar.md`
- Test: `crates/taiyaku-core/src/sidecar.rs` (its `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `Edition::credits: Vec<u32>` — page indices, zero-based, ascending,
  in the edition's flat page sequence. Absent and empty both mean "nothing
  classified".

- [ ] **Step 1: Write the failing tests**

Add to `crates/taiyaku-core/src/sidecar.rs`'s `mod tests`:

```rust
#[test]
fn credits_round_trip_present_empty_and_absent() {
    // Present: parsed, and emitted again in the same order.
    let text = r#"{
      "taiyaku": 1,
      "work": { "title": "W", "reading": "rtl" },
      "editions": {
        "raw": {
          "label": "raw",
          "members": [{ "path": "a.cbz", "sha256": "aa", "pages": 4 }],
          "spreads": [],
          "credits": [1, 3]
        }
      },
      "hub": "raw",
      "alignments": []
    }"#;
    let parsed = Sidecar::from_json(text).expect("parses");
    assert_eq!(parsed.editions["raw"].credits, vec![1, 3]);
    let again = Sidecar::from_json(&parsed.to_json()).expect("re-parses");
    assert_eq!(again.editions["raw"].credits, vec![1, 3]);

    // Absent: defaults to empty rather than failing to parse.
    let without = text.replace(",\n          \"credits\": [1, 3]", "");
    let parsed = Sidecar::from_json(&without).expect("parses without credits");
    assert!(parsed.editions["raw"].credits.is_empty());
}

#[test]
fn an_edition_with_no_credits_emits_no_credits_key() {
    // An empty list and an absent one mean the same thing, so a sidecar that
    // has never met this feature must round-trip without growing a field.
    // `spreads` always emits because every alignment computes one; `credits`
    // exists only once somebody runs the command.
    let text = r#"{
      "taiyaku": 1,
      "work": { "title": "W", "reading": "rtl" },
      "editions": {
        "raw": {
          "label": "raw",
          "members": [{ "path": "a.cbz", "sha256": "aa", "pages": 4 }],
          "spreads": []
        }
      },
      "hub": "raw",
      "alignments": []
    }"#;
    let emitted = Sidecar::from_json(text).expect("parses").to_json();
    assert!(
        !emitted.contains("credits"),
        "an edition with no credits must not emit the key:\n{emitted}"
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-core credits_round_trip -- --nocapture`
Expected: FAIL — `no field `credits` on type `Edition``.

- [ ] **Step 3: Add the field**

In `crates/taiyaku-core/src/sidecar.rs`, inside `pub struct Edition`, after
`spreads`:

```rust
    /// Indices, in the flattened page sequence, of pages a reader can keep out
    /// of the reading flow — a scanlation group's donation cards and staff
    /// credits.
    ///
    /// Page indices, like `spreads`, and not slots: both describe the page
    /// sequence rather than the alignment. Unlike `spreads` this is a
    /// heuristic's output rather than a measurement of the image, which is why
    /// `taiyaku credits` prints it for review and writes it only under
    /// `--write`.
    ///
    /// Skipped when empty, so a sidecar written before this feature existed
    /// round-trips byte for byte. An absent list and an empty one both mean
    /// nothing has been classified.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<u32>,
```

Then fix every struct literal that builds an `Edition` — the compiler will list
them; each takes `credits: Vec::new()`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core -- --nocapture`
Expected: PASS, whole package green.

- [ ] **Step 5: Document the field**

In `docs/sidecar.md`, immediately after the `spreads` bullet in the members
section, add:

```markdown
- `credits` lists the indices, in the edition's flat page sequence, of pages that
  are a scanlation group's own matter rather than the work — donation cards,
  staff credits, recruitment ads. A reader may keep them out of the reading flow
  and open them deliberately; they are never deleted or hidden. **Page indices,
  like `spreads`.** Absent and empty both mean nothing has been classified.
  Unlike `spreads`, which is measured from the image's aspect ratio, this is the
  output of a heuristic — `taiyaku credits` proposes it and a person writes it —
  so a reader consuming it should treat it as an assertion about presentation
  and not about the work.
```

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/taiyaku-core/src/sidecar.rs docs/sidecar.md
git commit -m "Add a per-edition credits list to the sidecar format"
```

---

### Task 2: A merge preserves a `credits` list it did not write

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (`Sidecar::merge_spoke`)
- Test: `crates/taiyaku-core/src/sidecar.rs` (its `mod tests`)

**Interfaces:**
- Consumes: `Edition::credits` from Task 1; the existing `Merged` enum.
- Produces: `Merged::DroppedCredits(usize)` — a new variant, carrying how many
  entries were discarded because the spoke's bytes changed.

- [ ] **Step 1: Write the failing tests**

`align --write` merges an edition in; `taiyaku credits --write` fills the list.
A merge can therefore land on a list it knows nothing about. Add to `mod tests`:

```rust
#[test]
fn re_merging_an_unchanged_spoke_keeps_its_credits() {
    let mut doc = star_fixture();
    doc.editions.get_mut("en").unwrap().credits = vec![7, 51];
    let spoke = doc.editions["en"].clone();

    let merged = doc
        .merge_spoke("raw", "en", spoke, solved_runs(), solver_provenance())
        .expect("merges");

    assert_ne!(merged, Merged::Added, "the spoke already existed");
    assert_eq!(
        doc.editions["en"].credits,
        vec![7, 51],
        "unchanged bytes must keep the list: the pages it names still exist"
    );
}

#[test]
fn replacing_a_spoke_drops_its_credits_and_says_how_many() {
    let mut doc = star_fixture();
    doc.editions.get_mut("en").unwrap().credits = vec![7, 51];

    // Replace the spoke's bytes. `merge_spoke` compares the ordered sha256
    // sequence, so a different hash is a different edition however it is named.
    let mut spoke = doc.editions["en"].clone();
    spoke.members[0].sha256 = "ffff".into();

    let merged = doc
        .merge_spoke("raw", "en", spoke, solved_runs(), solver_provenance())
        .expect("merges");

    assert_eq!(
        merged,
        Merged::DroppedCredits(2),
        "a list addressing pages that no longer exist must not survive silently"
    );
    assert!(doc.editions["en"].credits.is_empty());
}
```

`star_fixture`, `solved_runs` and `solver_provenance` stand for whatever the
existing merge tests in this file already use to build a three-edition document
and a solved run list — reuse those helpers verbatim rather than adding new
ones; find them by reading
`re_aligning_a_spoke_with_no_hand_runs_replaces_it_rather_than_duplicating`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-core spoke -- --nocapture`
Expected: FAIL — `no variant DroppedCredits`, and the first test fails on an
empty list because the wholesale insert discards it.

- [ ] **Step 3: Implement**

Add the variant to `pub enum Merged`:

```rust
    /// The spoke's bytes changed, so a `credits` list on the old edition was
    /// discarded rather than carried onto pages that may not exist. The count
    /// is how many entries went.
    ///
    /// Dropped rather than refused, which is the opposite of the stance on hand
    /// runs (O13) and deliberately so: a hand correction is a human decision
    /// that re-running nothing can recover, while a `credits` list is a tool's
    /// output that `taiyaku credits` reproduces in one command.
    DroppedCredits(usize),
```

In `merge_spoke`, before the spoke is inserted, capture the stored list and
decide by the same hash comparison the existing guard makes:

```rust
        // Content, not paths — the idiom this function's own from-side guard
        // already states. A renamed but byte-identical archive is the same
        // edition and keeps its list.
        let stored_credits = self
            .editions
            .get(to)
            .map(|e| e.credits.clone())
            .unwrap_or_default();
        let unchanged = self.editions.get(to).is_some_and(|existing| {
            existing.members.iter().map(|m| &m.sha256).eq(spoke.members.iter().map(|m| &m.sha256))
        });
        let dropped = match unchanged {
            true => 0,
            false => stored_credits.len(),
        };
```

After the spoke is inserted, restore the list when the bytes were unchanged, and
report the drop otherwise:

```rust
        if unchanged {
            self.editions.get_mut(to).expect("just inserted").credits = stored_credits;
        }
```

and, where `merge_spoke` builds its `Merged` answer, return
`Merged::DroppedCredits(dropped)` when `dropped > 0`, in preference to
`Merged::Replaced` — a caller that has lost a list needs to hear that rather
than the fact of replacement, which it already knows it asked for.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core -- --nocapture`
Expected: PASS. If an existing merge test now sees `DroppedCredits(0)` where it
expected `Replaced`, the guard above is returning the new variant when nothing
was dropped — fix the condition, not the test.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/taiyaku-core/src/sidecar.rs
git commit -m "Carry a credits list across an unchanged merge, drop it on a replaced spoke"
```

---

### Task 3: `unpaired_runs` — signal 1, split at member boundaries

**Files:**
- Create: `crates/taiyaku-core/src/credits.rs`
- Modify: `crates/taiyaku-core/src/lib.rs`
- Test: `crates/taiyaku-core/src/credits.rs` (its own `mod tests`)

**Interfaces:**
- Consumes: `Sidecar::runs_of`, `Edition::slots`, `Edition::locate`,
  `Edition::total_pages`, `Work::reading` — all existing.
- Produces:

```rust
pub struct UnpairedRun {
    pub slots: std::ops::Range<u32>,
    pub pages: std::ops::Range<u32>,
    pub member: usize,
    pub local: std::ops::Range<u32>,
    pub at_member_end: bool,
}
pub fn unpaired_runs(sidecar: &Sidecar, edition: &str, against: &str)
    -> Result<Vec<UnpairedRun>, String>;
```

All four ranges are half-open (`start..end`), so `len()` is the count.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-core/src/credits.rs` with the module doc and tests only:

```rust
//! Which slots of an edition the alignment never mentions, and where they sit.
//!
//! This is signal 1 of credit-page detection, and the only one that classifies.
//! It is pure — no I/O, no image decoding — so it lives here beside `route` and
//! `slot` rather than in the CLI, and it reports geometry rather than applying
//! a threshold: the command decides what counts.
//!
//! **Runs are split at member boundaries.** Measured on `corpus/gotoubun-v01`'s
//! fan edition, an unsplit run spanning `c005`'s last page and `c005.5`'s first
//! four begins and ends mid-member, so a rule reading position off its endpoints
//! sees no boundary at all — and loses one credits page and four front-matter
//! pages in the same stroke. See `docs/notes.md`, "Corrected 2026-08-05".

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sidecar;

    /// Two members of four pages each, no spreads, aligned against a hub that
    /// pairs everything except the pages each test names.
    ///
    /// Written as JSON rather than built with struct literals because the span
    /// notation is what a real sidecar carries, and a fixture that skips the
    /// parse cannot catch a span the parser reads differently.
    fn two_members(b_runs: &str) -> Sidecar {
        let text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "W", "reading": "rtl" }},
              "editions": {{
                "raw": {{
                  "label": "raw",
                  "members": [{{ "path": "r.cbz", "sha256": "r0", "pages": 8 }}],
                  "spreads": []
                }},
                "fan": {{
                  "label": "fan",
                  "members": [
                    {{ "path": "c1.cbz", "sha256": "c1", "pages": 4 }},
                    {{ "path": "c2.cbz", "sha256": "c2", "pages": 4 }}
                  ],
                  "spreads": []
                }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "fan", "by": "solver", "runs": {b_runs} }}
              ]
            }}"#
        );
        Sidecar::from_json(&text).expect("fixture parses")
    }

    #[test]
    fn a_run_at_a_members_final_index_is_flagged_at_its_end() {
        // fan pages 0,1,2 and 4..7 pair; page 3 — c1's last — does not.
        let doc = two_members(
            r#"[{"a":"0-2","b":"0-2"},{"a":"3"},{"a":"4-7","b":"4-7"}]"#,
        );
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].pages, 3..4);
        assert_eq!(runs[0].member, 0);
        assert_eq!(runs[0].local, 3..4);
        assert!(runs[0].at_member_end, "page 3 is c1's last of four");
    }

    #[test]
    fn a_run_at_a_members_first_index_is_not_flagged_at_its_end() {
        // fan page 4 — c2's first — is unpaired. This is the front-matter
        // shape, and it must not read as a credits candidate.
        let doc = two_members(
            r#"[{"a":"0-3","b":"0-3"},{"a":"4"},{"a":"5-7","b":"5-7"}]"#,
        );
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].member, 1);
        assert_eq!(runs[0].local, 0..1);
        assert!(!runs[0].at_member_end, "c2 has four pages; this is its first");
    }

    #[test]
    fn a_run_spanning_a_member_boundary_splits_into_two() {
        // The corpus shape: c1's last page and c2's first two, contiguous.
        // Unsplit this is one run flagged at neither end, which is how the
        // first design lost a credits page and gained four false positives.
        let doc = two_members(
            r#"[{"a":"0-2","b":"0-2"},{"a":"3-5"},{"a":"6-7","b":"6-7"}]"#,
        );
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 2, "one run per member, never one spanning both");
        assert_eq!((runs[0].member, runs[0].local.clone()), (0, 3..4));
        assert!(runs[0].at_member_end);
        assert_eq!((runs[1].member, runs[1].local.clone()), (1, 0..2));
        assert!(!runs[1].at_member_end);
    }

    #[test]
    fn a_run_touching_neither_end_is_flagged_at_neither() {
        let doc = two_members(
            r#"[{"a":"0","b":"0"},{"a":"1-2"},{"a":"3-7","b":"3-7"}]"#,
        );
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].local, 1..3);
        assert!(!runs[0].at_member_end);
    }

    #[test]
    fn an_edition_with_nothing_unpaired_yields_no_runs() {
        let doc = two_members(r#"[{"a":"0-7","b":"0-7"}]"#);
        assert!(unpaired_runs(&doc, "fan", "raw").expect("resolves").is_empty());
    }

    #[test]
    fn a_spread_inside_an_unpaired_run_keeps_pages_and_slots_apart() {
        // The corpus cannot catch a page-for-slot substitution: Go-Toubun's fan
        // edition has five spreads and every one of them falls outside every
        // unpaired region, so both indexings agree there. This fixture makes
        // them disagree by construction.
        //
        // `fan` page 1 is a spread, so the edition is 4 pages and 5 slots:
        //   slot 0 = page 0
        //   slot 1 = page 1 right half
        //   slot 2 = page 1 left half
        //   slot 3 = page 2
        //   slot 4 = page 3
        // Leaving slots 1..3 unpaired means pages 1..2 — one page, two slots.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 5 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [{ "path": "c1.cbz", "sha256": "c1", "pages": 4 }],
              "spreads": [1]
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0","b":"0"},{"a":"1-2"},{"a":"3-4","b":"2-3"}] }
          ]
        }"#;
        let doc = Sidecar::from_json(text).expect("fixture parses");
        let runs = unpaired_runs(&doc, "fan", "raw").expect("resolves");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].slots, 1..3, "two slots");
        assert_eq!(runs[0].pages, 1..2, "one page");
        assert!(!runs[0].at_member_end, "page 1 of 4 is not the last");
    }

    #[test]
    fn a_half_paired_spread_page_is_not_unpaired() {
        // Only one half of the spread at page 1 is unmentioned. The page is
        // partly reachable, so it is not a credits candidate: a credits card is
        // never half of a joined spread.
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 5 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [{ "path": "c1.cbz", "sha256": "c1", "pages": 4 }],
              "spreads": [1]
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0-1","b":"0-1R"},{"a":"2"},{"a":"3-4","b":"2-3"}] }
          ]
        }"#;
        let doc = Sidecar::from_json(text).expect("fixture parses");
        assert!(
            unpaired_runs(&doc, "fan", "raw").expect("resolves").is_empty(),
            "a page with any paired slot is not an unpaired page"
        );
    }

    #[test]
    fn an_edition_the_document_does_not_have_is_refused_by_name() {
        let doc = two_members(r#"[{"a":"0-7","b":"0-7"}]"#);
        let err = unpaired_runs(&doc, "nope", "raw").expect_err("refuses");
        assert!(err.contains("nope"), "the message must name it: {err}");
    }

    #[test]
    fn a_pair_with_no_edge_is_refused_naming_both() {
        let doc = two_members(r#"[{"a":"0-7","b":"0-7"}]"#);
        let err = unpaired_runs(&doc, "fan", "fan").expect_err("refuses");
        assert!(err.contains("fan"), "the message must name the pair: {err}");
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-core credits:: -- --nocapture`
Expected: FAIL to compile — `cannot find function `unpaired_runs``.

- [ ] **Step 3: Implement**

Add to `crates/taiyaku-core/src/lib.rs`, beside the other module declarations
and re-exports:

```rust
pub mod credits;
pub use credits::{UnpairedRun, unpaired_runs};
```

Then prepend the implementation to `credits.rs`, above `mod tests`:

```rust
use std::collections::BTreeSet;
use std::ops::Range;

use crate::Sidecar;

/// A stretch of one member's pages that the alignment never mentions.
///
/// Both index spaces are carried rather than one, because the `credits` field
/// is in pages and a reader's position is in slots, and a call site converting
/// for itself is how the two silently diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpairedRun {
    /// Half-open, in the edition's flat slot sequence.
    pub slots: Range<u32>,
    /// Half-open, in the edition's flat page sequence.
    pub pages: Range<u32>,
    /// Index into the edition's `members`. One member, always: runs are split
    /// at boundaries so this cannot span two.
    pub member: usize,
    /// Half-open, local to that member.
    pub local: Range<u32>,
    /// Does this run end on the member's last page?
    ///
    /// The only positional test that classifies. Measured four for four on
    /// `corpus/gotoubun-v01`'s fan edition, where the first-index version is
    /// zero for four — what accumulates at a member's start is front matter.
    pub at_member_end: bool,
}

/// Which pages of `edition` the alignment against `against` never reaches.
///
/// A page counts as unpaired only when **every** slot it presents is unpaired.
/// A joined spread whose halves are half-reached is content the other edition
/// partly has, and no credits card is half of a spread.
pub fn unpaired_runs(
    sidecar: &Sidecar,
    edition: &str,
    against: &str,
) -> Result<Vec<UnpairedRun>, String> {
    let target = sidecar
        .editions
        .get(edition)
        .ok_or_else(|| format!("this work has no edition `{edition}`"))?;

    // Which side of the stored edge is the edition being examined? The
    // alignment is directed and only one side's spans address this edition's
    // slots, so taking the wrong one silently reports another edition's gaps.
    let (alignment, mine_is_a) = sidecar
        .alignments
        .iter()
        .find_map(|al| match (al.from.as_str(), al.to.as_str()) {
            (f, t) if f == edition && t == against => Some((al, true)),
            (f, t) if f == against && t == edition => Some((al, false)),
            _ => None,
        })
        .ok_or_else(|| {
            format!("no stored alignment joins `{edition}` and `{against}`")
        })?;

    let runs = sidecar.runs_of(alignment)?;

    let mut mentioned: BTreeSet<u32> = BTreeSet::new();
    for run in &runs {
        let range = match mine_is_a {
            true => Some(run.a),
            false => run.b,
        };
        if let Some(r) = range {
            mentioned.extend(r.start..=r.end);
        }
    }

    let slots = target.slots(sidecar.work.reading);

    // Group slots by the page they belong to, and keep only pages every one of
    // whose slots is unmentioned.
    let mut unpaired_pages: Vec<u32> = Vec::new();
    let mut at = 0usize;
    while at < slots.len() {
        let page = slots[at].page;
        let first = at;
        while at < slots.len() && slots[at].page == page {
            at += 1;
        }
        if (first..at).all(|i| !mentioned.contains(&(i as u32))) {
            unpaired_pages.push(page);
        }
    }

    // Where each page's slots begin, so a run can report both index spaces.
    let slot_start = |page: u32| -> u32 {
        slots
            .iter()
            .position(|s| s.page == page)
            .expect("every page has at least one slot") as u32
    };
    let slot_count = |page: u32| -> u32 {
        slots.iter().filter(|s| s.page == page).count() as u32
    };

    let mut out: Vec<UnpairedRun> = Vec::new();
    for page in unpaired_pages {
        let (member, local) = target
            .locate(page)
            .ok_or_else(|| format!("`{edition}` has no page {page}"))?;

        // Extend the previous run only when this page is its immediate
        // successor *within the same member*. The member test is what splits a
        // run at a boundary, and it is the whole reason this function exists
        // rather than a complement over the slot set.
        match out.last_mut() {
            Some(last) if last.member == member && last.local.end == local => {
                last.pages.end = page + 1;
                last.local.end = local + 1;
                last.slots.end = slot_start(page) + slot_count(page);
            }
            _ => out.push(UnpairedRun {
                slots: slot_start(page)..slot_start(page) + slot_count(page),
                pages: page..page + 1,
                member,
                local: local..local + 1,
                at_member_end: false,
            }),
        }
    }

    for run in &mut out {
        run.at_member_end = run.local.end == target.members[run.member].pages;
    }

    Ok(out)
}
```

`Edition::locate` returns `(usize, u32)`; if the compiler reports a mismatch on
`local`, adapt the types rather than casting away the difference.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core credits:: -- --nocapture`
Expected: PASS, nine tests.

- [ ] **Step 5: Verify it still builds for wasm**

Run: `cargo build -p taiyaku-core --target wasm32-unknown-unknown`
Expected: success. This module must stay I/O-free.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/taiyaku-core/src/credits.rs crates/taiyaku-core/src/lib.rs
git commit -m "Report unpaired runs per member, split at boundaries"
```

---

### Task 4: The dhash recurrence pass — reported, never decisive

**Files:**
- Create: `crates/taiyaku-cli/src/credits.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (add `mod credits;`)
- Test: `crates/taiyaku-cli/src/credits.rs` (its own `mod tests`)

**Interfaces:**
- Consumes: the existing fingerprint type in `crates/taiyaku-cli/src/fingerprint.rs`
  — read that file and use its hash type and its cache accessor as they are.
- Produces:

```rust
pub struct Recurrence {
    pub page: u32,
    pub distance: u32,
    pub nearest_member: usize,
    pub nearest_local: u32,
}
pub fn recurrences(members: &[Vec<Hash>]) -> Vec<Recurrence>;
```

where `Hash` is `fingerprint.rs`'s existing 256-bit dhash type, and `members` is
one hash list per member in edition order.

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-cli/src/credits.rs`:

```rust
//! Signal 2: how close each page comes to a page in a *different* member of the
//! same edition, by dhash.
//!
//! **This is a diagnostic and never a classifier**, and that is a measurement
//! rather than a preference. Run over all 235 pages of `corpus/gotoubun-v01`'s
//! fan edition, its two tightest matches in the whole edition — Hamming
//! distance 2 — are the volume *cover*, which the release carries in two chapter
//! files. Its true positives, the three donation cards, sit at 14 to 24 rather
//! than near zero, because a near-black card has few strong gradients and
//! re-encoding flips the weak ones. And it misses both staff-credit pages at
//! distances 93 and 99, because each appears exactly once.
//!
//! So it cannot decide, and gating signal 1 on it would veto the two true
//! positives signal 1 uniquely finds. What it is good for is telling a person
//! reviewing a candidate *why* a page recurs and where. See `docs/notes.md`,
//! "The dhash signal finds duplicated pages, which is not the same thing".

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinct hashes at known Hamming distances, built by flipping bits of a
    /// base rather than by writing literals nobody can check by eye.
    fn hash_with_bits_flipped(n: u32) -> Hash {
        let mut h = Hash::default();
        for bit in 0..n {
            h.flip_bit(bit as usize);
        }
        h
    }

    #[test]
    fn the_nearest_neighbour_is_never_in_the_same_member() {
        // Member 0 holds two pages one bit apart. Member 1 holds one page eight
        // bits away. The within-member pair is closer and must be ignored:
        // a chapter repeating its own page says nothing about recurrence.
        let members = vec![
            vec![hash_with_bits_flipped(0), hash_with_bits_flipped(1)],
            vec![hash_with_bits_flipped(8)],
        ];
        let found = recurrences(&members);
        let page0 = found.iter().find(|r| r.page == 0).expect("page 0 reported");
        assert_eq!(page0.distance, 8, "the 1-bit neighbour is in the same member");
        assert_eq!(page0.nearest_member, 1);
        assert_eq!(page0.nearest_local, 0);
    }

    #[test]
    fn pages_are_numbered_across_the_whole_edition_not_within_a_member() {
        let members = vec![
            vec![hash_with_bits_flipped(0), hash_with_bits_flipped(40)],
            vec![hash_with_bits_flipped(1)],
        ];
        let found = recurrences(&members);
        assert_eq!(
            found.iter().map(|r| r.page).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "member 1's only page is the edition's page 2"
        );
    }

    #[test]
    fn a_single_member_edition_reports_nothing() {
        // Nothing to recur against. Reporting distance 0 here, or the nearest
        // page within the member, would invent a signal.
        let members = vec![vec![hash_with_bits_flipped(0), hash_with_bits_flipped(1)]];
        assert!(recurrences(&members).is_empty());
    }
}
```

If `fingerprint.rs`'s hash type has no `flip_bit` and no `Default`, add a
`#[cfg(test)]` helper in *this* file that builds one from raw `u64`s instead —
do not widen the fingerprint type's public API for a test.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p taiyaku-cli credits:: -- --nocapture`
Expected: FAIL to compile — `cannot find function `recurrences``.

- [ ] **Step 3: Implement**

Add `mod credits;` to `crates/taiyaku-cli/src/main.rs` beside the other module
declarations, then prepend to `credits.rs`:

```rust
use crate::fingerprint::Hash; // whatever `fingerprint.rs` actually names it

/// How close one page comes to a page in another member of the same edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recurrence {
    /// Index in the edition's flat page sequence.
    pub page: u32,
    /// Hamming distance over the 256-bit dhash, 0 to 256.
    pub distance: u32,
    pub nearest_member: usize,
    pub nearest_local: u32,
}

/// For each page, the nearest page in a *different* member.
///
/// Quadratic in the edition's page count, which is fine at the scale this runs
/// at — 235 pages is 27,495 comparisons of four `u64` xors — and worth keeping
/// simple rather than indexing, because the answer is printed for a person and
/// never sits on a request path.
///
/// An edition with fewer than two members reports nothing rather than falling
/// back to within-member neighbours: there is no recurrence to measure, and an
/// invented number would be read as one.
pub fn recurrences(members: &[Vec<Hash>]) -> Vec<Recurrence> {
    if members.len() < 2 {
        return Vec::new();
    }

    // Flatten once, remembering which member each page came from.
    let mut flat: Vec<(usize, u32, Hash)> = Vec::new();
    for (mi, pages) in members.iter().enumerate() {
        for (li, h) in pages.iter().enumerate() {
            flat.push((mi, li as u32, *h));
        }
    }

    flat.iter()
        .enumerate()
        .filter_map(|(page, (mi, _, h))| {
            let (distance, nearest_member, nearest_local) = flat
                .iter()
                .filter(|(mj, _, _)| mj != mi)
                .map(|(mj, lj, g)| (h.distance(g), *mj, *lj))
                .min_by_key(|(d, _, _)| *d)?;
            Some(Recurrence {
                page: page as u32,
                distance,
                nearest_member,
                nearest_local,
            })
        })
        .collect()
}
```

Use `fingerprint.rs`'s existing Hamming-distance function rather than writing a
second one; if it is private, make it `pub(crate)` rather than duplicating it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli credits:: -- --nocapture`
Expected: PASS, three tests.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/taiyaku-cli/src/credits.rs crates/taiyaku-cli/src/main.rs
git commit -m "Measure cross-member page recurrence, as a diagnostic"
```

---

### Task 5: The `taiyaku credits` command

**Files:**
- Modify: `crates/taiyaku-cli/src/credits.rs` (add `command`)
- Modify: `crates/taiyaku-cli/src/main.rs` (dispatch, usage text)
- Test: `crates/taiyaku-cli/src/credits.rs`

**Interfaces:**
- Consumes: `taiyaku_core::unpaired_runs` (Task 3), `recurrences` (Task 4),
  `Sidecar::from_json`, `Sidecar::to_json`, `Sidecar::edition_of`,
  `main::option_value`, `main::reject_unknown_flags`.
- Produces: `pub fn command(args: &[String]) -> Result<(), String>`.

Usage, matching the shapes `inventory` and `duplicates` already use:

```
taiyaku credits <member>...  [--sidecar FILE [--against EDITION]] [--cache DIR] [--write]
```

- [ ] **Step 1: Write the failing tests**

Add to `credits.rs`'s `mod tests`:

```rust
#[test]
fn write_without_a_sidecar_is_refused() {
    let args: Vec<String> = ["a.cbz", "--write"].iter().map(|s| s.to_string()).collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("--write") && err.contains("--sidecar"),
        "the refusal must name both flags: {err}"
    );
}

#[test]
fn a_valued_flag_with_no_value_is_refused_before_any_member_is_read() {
    // O15's rule. The member path does not exist, so a version that resolved
    // options late would fail on a missing file instead of ever refusing.
    let args: Vec<String> = ["no-such-file.cbz", "--sidecar"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let err = command(&args).expect_err("refuses");
    assert!(
        err.contains("--sidecar") && !err.contains("no-such-file"),
        "the flag must be refused before the member is opened: {err}"
    );
}

#[test]
fn a_misspelled_flag_is_refused() {
    // O21's rule.
    let args: Vec<String> = ["a.cbz", "--wrote"].iter().map(|s| s.to_string()).collect();
    let err = command(&args).expect_err("refuses");
    assert!(err.contains("--wrote"), "the refusal must name it: {err}");
}

#[test]
fn no_members_is_refused_with_the_usage() {
    let err = command(&[]).expect_err("refuses");
    assert!(err.contains("credits"), "the refusal must show the usage: {err}");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-cli credits::tests -- --nocapture`
Expected: FAIL to compile — `cannot find function `command``.

- [ ] **Step 3: Implement the command**

Add to `crates/taiyaku-cli/src/credits.rs`:

```rust
/// `taiyaku credits` — propose the credit pages of one edition.
///
/// Prints; writes only under `--write`, the discipline `inventory` and
/// `duplicates` set. Both signals are reported per candidate, but only signal
/// 1 classifies — see this module's own doc comment for why.
pub fn command(args: &[String]) -> Result<(), String> {
    const KNOWN: &[&str] = &["--sidecar", "--against", "--cache", "--write"];
    crate::reject_unknown_flags("credits", args, KNOWN)?;

    // Every valued flag resolves before a member is opened, so a typo does not
    // cost the seconds it takes to fingerprint an edition.
    let sidecar = crate::option_value(args, "--sidecar", "a path to a taiyaku.json")?;
    let against = crate::option_value(args, "--against", "an edition name")?;
    let cache = crate::option_value(args, "--cache", "a directory")?;
    let write = args.iter().any(|a| a == "--write");

    if write && sidecar.is_none() {
        return Err(
            "--write needs --sidecar: there is nothing to write the list into".into(),
        );
    }
    if against.is_some() && sidecar.is_none() {
        return Err(
            "--against names an edition of a sidecar, so it needs --sidecar too".into(),
        );
    }

    let members: Vec<&String> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| Some(a.as_str()) != sidecar)
        .filter(|a| Some(a.as_str()) != against)
        .filter(|a| Some(a.as_str()) != cache)
        .collect();
    if members.is_empty() {
        return Err(USAGE.into());
    }

    // ... the body: fingerprint the members, run `recurrences`, and — when a
    // sidecar was given — match the members to an edition by ordered sha256,
    // resolve the counterpart, run `unpaired_runs`, and print the table.
    todo!("filled in by the following steps")
}

const USAGE: &str = "\
taiyaku credits <member>...  [--sidecar FILE [--against EDITION]] [--cache DIR] [--write]

Propose the pages of one edition that are a scanlation group's own matter
rather than the work. Prints what it found; writes nothing without --write.";
```

Then replace the `todo!` with the body, in this order:

1. **Fingerprint the members**, through `fingerprint.rs`'s cached path, honouring
   `--cache`. Reuse whatever `duplicates.rs` calls to turn a member path into a
   page hash list — do not open archives here.
2. **Run `recurrences`** over those lists.
3. **If `--sidecar` was given**, parse it and match the members to an edition by
   content. **`Sidecar::edition_of` already does this** — do not write a second
   matcher:

```rust
    // Content, not paths: `edition_of` compares the sorted sha256 set, which is
    // the idiom `merge_spoke`'s own guard states. A renamed but byte-identical
    // archive is still this edition.
    let edition = doc
        .edition_of(&hashes)
        .ok_or_else(|| {
            format!(
                "no edition of `{}` is made of these {} members.\nlooked for: {}",
                sidecar.unwrap_or_default(),
                hashes.len(),
                hashes.join(", ")
            )
        })?
        .to_string();
```

   Note that `edition_of` matches the sorted set, so it does not care what order
   the members were given in. That is right for identifying an edition and wrong
   for numbering its pages, so **the flat page order must come from the matched
   `Edition`'s own `members` list, never from the command line's order.**

4. **Resolve the counterpart.** A spoke uses the hub; the hub needs `--against`:

```rust
    let counterpart = match against {
        Some(name) => name.to_string(),
        None if edition != doc.hub => doc.hub.clone(),
        None => {
            let spokes: Vec<&str> = doc
                .alignments
                .iter()
                .map(|a| if a.from == doc.hub { a.to.as_str() } else { a.from.as_str() })
                .collect();
            return Err(format!(
                "`{edition}` is this work's hub, so there is no single edition to \
                 compare it against. Name one with --against: {}",
                spokes.join(", ")
            ));
        }
    };
```

5. **Run `unpaired_runs`** and classify: a run is a candidate when
   `run.at_member_end` is true and `run.pages.len() <= BOUND`. Until Task 6 has
   measured a bound, define it as:

```rust
/// The longest run at a member's last page that still reads as credits.
///
/// **Provisional: 1, which is every true positive measured so far.** Every
/// credit page found on `corpus/gotoubun-v01` is a run of exactly 1, and the
/// only long run there is excluded by position as well as length, so that
/// volume does not constrain this at all. Task 6's survey over
/// `corpus/kaguya-v01` is what fixes it; raising it needs a measured run of 2
/// or 3 that is genuinely credits.
const MAX_CREDIT_RUN: u32 = 1;
```

6. **Print** one row per unpaired run, and a fixed-field `credits:` line, the
   discipline `--stats` and `duplicates`' `set:` line set:

```
page  member          local  run  at-end  recurs  nearest        verdict
  95  c001.cbz        51/52    1     yes      14  c004.cbz#29    credits
 168  c003.cbz        31/32    1     yes      24  c001.cbz#51    credits
 225  c005.cbz        26/27    1     yes      99  c003.cbz#2     credits
 226  c005.5.cbz       0/9     4      no       2  c000.cbz#0     -
   1  c000.cbz         1/44   42      no      88  c002.cbz#17    -

credits: edition=en-fan pages=235 slots=240 runs=5 candidates=4 bound=1
```

`recurs` and `nearest` come from signal 2 and are printed for every row
including the ones nothing classifies — they are what a person uses to judge a
candidate, not what the tool used to pick it.

7. **Under `--write`**, replace that edition's `credits` list wholesale and
   re-emit the sidecar. Wholesale rather than merged: the field holds no hand
   entries, so a re-run after the bound changes must leave nothing stale.

Add the dispatch arm in `main.rs`, beside `duplicates` and `inventory`:

```rust
        Some("credits") => credits::command(&args[1..]),
```

and add the usage line to `main.rs`'s top-level help text and to the command
list in `docs/status.md`'s "Seven commands" block, which becomes eight.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Run it against the corpus by hand**

Run:
```bash
cargo run -p taiyaku-cli -- credits \
  corpus/gotoubun-v01/en-fan/*.cbz \
  --sidecar corpus/gotoubun-v01/taiyaku.json \
  --cache .taiyaku-cache
```
Expected: five rows; candidates 95, 168, 198 and 225 marked `credits`; the
42-page `c000` run and the four `c005.5` pages not. **If any other page is
marked, stop and re-read Task 3's split** — this is the measurement the whole
design rests on.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/taiyaku-cli/src/credits.rs crates/taiyaku-cli/src/main.rs docs/status.md
git commit -m "Add taiyaku credits, which proposes and never writes unasked"
```

---

### Task 6: The survey, the bound, and two gated corpus checks

**Files:**
- Create: `scripts/credit-survey.sh`
- Modify: `crates/taiyaku-cli/src/corpus.rs`
- Modify: `scripts/check-all.sh`
- Modify: `docs/notes.md`

**Interfaces:**
- Consumes: the `credits:` line from Task 5.
- Produces: a measured value for `MAX_CREDIT_RUN`, and two `#[test]`s gated on
  `TAIYAKU_CORPUS`.

**This task is a measurement, and its outcome is not predetermined.** Do not
write the corpus checks before running the survey; write them against what it
measured.

- [ ] **Step 1: Write the survey script**

Create `scripts/credit-survey.sh`, modelled on `scripts/duplicate-survey.sh` —
read that file and follow its shape:

```bash
#!/usr/bin/env bash
# Run `taiyaku credits` over every fan edition in the corpus and collect the
# fixed-field `credits:` lines into one table.
#
#   TAIYAKU_CORPUS=corpus scripts/credit-survey.sh
#
# The bound in `crates/taiyaku-cli/src/credits.rs` is chosen from this output
# and from looking at every candidate, not from this output alone.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
: "${TAIYAKU_CORPUS:?set it to the corpus root}"

for work in "${TAIYAKU_CORPUS:?}"/*/; do
  ed="$work/en-fan"
  [ -d "$ed" ] || continue
  [ -f "$work/taiyaku.json" ] || continue
  cargo run --quiet -p taiyaku-cli -- credits "$ed"/*.cbz \
    --sidecar "$work/taiyaku.json" --cache .taiyaku-cache \
    | tee /dev/stderr | grep '^credits:'
done
```

- [ ] **Step 2: Make Kaguya's fan edition alignable**

`corpus/kaguya-v01` has no `taiyaku.json`. Build one:

```bash
cargo run -p taiyaku-cli -- align \
  corpus/kaguya-v01/raw-mono/*.cbz -- corpus/kaguya-v01/en-fan/*.cbz \
  --cache .taiyaku-cache \
  --write corpus/kaguya-v01/taiyaku.json --title "Kaguya-sama v01" \
  --name-a raw-mono --name-b en-fan
```
Expected: a sidecar, and a printed verdict. **Record the verdict** — if it is
`fragmented`, signal 1's input is unreliable there and the survey must say so
rather than treating its output as evidence.

- [ ] **Step 3: Run the survey and look at every candidate**

Run: `TAIYAKU_CORPUS=corpus scripts/credit-survey.sh`

Then, for **every** page any edition marks `credits`, and every run of length 2
or 3 whether marked or not, extract it and look at it:

```bash
python3 - <<'PY'
import zipfile, io, os, re, sys
from PIL import Image
archive, local = sys.argv[1], int(sys.argv[2])
IMG = {'.jpg','.jpeg','.png','.webp'}
nat = lambda s: [int(t) if t.isdigit() else t.lower() for t in re.split(r'(\d+)', s)]
z = zipfile.ZipFile(archive)
names = sorted([n for n in z.namelist() if os.path.splitext(n)[1].lower() in IMG], key=nat)
im = Image.open(io.BytesIO(z.read(names[local]))); im.thumbnail((620, 620))
im.convert('RGB').save('/tmp/candidate.jpg', quality=72)
print(names[local])
PY
```

This step is not optional and not automatable. All three errors in the first
draft of the design were found here and nowhere else.

- [ ] **Step 4: Fix the bound, or record that it cannot be fixed**

Set `MAX_CREDIT_RUN` in `crates/taiyaku-cli/src/credits.rs` to what the survey
measured, replacing the provisional comment with the real figures and sample
size.

**If the survey finds no clean separation** — a run of 2 that is credits and a
run of 2 that is not, say — then no bound ships: leave it at 1, say so in the
constant's doc comment, and note in `notes.md` that longer credit runs are
known to exist and are not detected. That is roadmap item 1's outcome and an
acceptable result of this task.

- [ ] **Step 5: Write the two corpus checks**

Add to `crates/taiyaku-cli/src/corpus.rs`, following the gating idiom already in
that file (`corpus()` returning `None` when `TAIYAKU_CORPUS` is unset, and a
printed line when it skips):

```rust
/// Go-Toubun v01's fan edition classifies exactly its four credit pages.
///
/// Asserts the *set*, not the count: four-of-six is reachable by classifying
/// the wrong four, and the first design did exactly that in both directions —
/// it would have taken `c005.5`'s front matter and left `c005`'s staff credits.
#[test]
fn gotoubun_fan_edition_classifies_four_credit_pages() {
    let Some(root) = corpus() else {
        println!("skipping: TAIYAKU_CORPUS is not set");
        return;
    };
    // ... load corpus/gotoubun-v01/taiyaku.json, run unpaired_runs("en-fan",
    // "raw-mono"), classify with the same rule the command uses, and:
    assert_eq!(classified, vec![95, 168, 198, 225]);
    println!("gotoubun en-fan: {} runs, classified {:?}", runs.len(), classified);
}
```

and a second, `kaguya_fan_edition_classifies_what_the_survey_measured`, whose
expected set is whatever Step 3 established — with the same printed line.

- [ ] **Step 6: Wire them into `check-all.sh`**

In `scripts/check-all.sh`'s standing-corpus-checks block, add the two test
names to the list run with `--nocapture`. A gated check the script does not run
is invisible to the gate, which is that block's whole purpose.

- [ ] **Step 7: Record the survey in `notes.md`**

Add a section under the corrected one, giving the figures per edition, the
bound chosen, the sample size, and every candidate looked at with what it turned
out to be. This is the artefact; the scratch scripts are not kept.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
TAIYAKU_CORPUS=corpus cargo test -p taiyaku-cli corpus:: -- --nocapture
git add scripts/credit-survey.sh scripts/check-all.sh \
        crates/taiyaku-cli/src/corpus.rs crates/taiyaku-cli/src/credits.rs docs/notes.md
git commit -m "Survey credit pages across both fan editions and fix the bound"
```

---

### Task 7: `credits(edition)` across the wasm boundary

**Files:**
- Modify: `crates/taiyaku-wasm/src/binding.rs`
- Modify: `web/src/resolver.ts`
- Modify: `web/src/fixtures.ts`
- Test: `crates/taiyaku-wasm/src/binding.rs`, `web/src/resolver.test.ts`

**Interfaces:**
- Consumes: `Edition::credits` (Task 1).
- Produces: `Resolver.credits(edition: string): number[]` — page indices,
  ascending; empty for an unknown edition or one with no list.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-wasm/src/binding.rs`'s tests:

```rust
#[test]
fn credits_crosses_the_boundary_as_page_indices() {
    let binding = Binding::parse(SIDECAR_WITH_CREDITS).expect("parses");
    assert_eq!(binding.credits("fan"), vec![3, 4]);
    assert!(
        binding.credits("no-such-edition").is_empty(),
        "an unknown edition answers empty, not an error: the reader asks for \
         every edition it draws and a missing list is not a fault"
    );
}
```

In `web/src/resolver.test.ts`, assert the interface is satisfied by the fixture
resolver — follow whatever shape that file already uses for `slotCount`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-wasm -- --nocapture` and `npm --prefix web test -- resolver`
Expected: FAIL — no method `credits`.

- [ ] **Step 3: Implement**

In `binding.rs`, beside the existing `slot_count` accessor and following its
`#[wasm_bindgen]` shape exactly:

```rust
    /// The edition's credit pages, as page indices.
    ///
    /// Pages, not slots — the field is the page-sequence kind, like `spreads`.
    /// `web/src/modules/credits.ts` converts once, where it draws.
    ///
    /// An unknown edition answers empty rather than erroring: the reader asks
    /// about whatever edition it is showing, and "no list" and "no edition" are
    /// the same non-event to a caller deciding whether to skip a page.
    pub fn credits(&self, edition: &str) -> Vec<u32> {
        self.sidecar
            .editions
            .get(edition)
            .map(|e| e.credits.clone())
            .unwrap_or_default()
    }
```

In `web/src/resolver.ts`, add to the `Resolver` interface beside `slotCount`:

```ts
  /**
   * The edition's credit pages, as **page** indices — the sidecar's own
   * `credits` field, which is the page-sequence kind like `spreads`.
   * `modules/credits.ts` converts to slots where it draws.
   */
  credits(edition: string): number[]
```

and to `wasmResolver`'s returned object:

```ts
    credits: (edition) => Array.from(sc.credits(edition)),
```

`Array.from` because `wasm-bindgen` hands back a typed array and the rest of
`web/` treats these as plain arrays.

In `web/src/fixtures.ts`, add the same method backed by a plain record, matching
how `slotCount` is faked there.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-wasm -- --nocapture && npm --prefix web test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/taiyaku-wasm/src/binding.rs web/src/resolver.ts web/src/fixtures.ts web/src/resolver.test.ts
git commit -m "Expose an edition's credit pages to the reader"
```

---

### Task 8: The `credits` reader module

**Files:**
- Create: `web/src/modules/credits.ts`
- Create: `web/src/modules/credits.test.ts`
- Modify: `web/src/modules/registry.ts`

**Interfaces:**
- Consumes: `Resolver.credits` (Task 7), the `ReaderModule` contract in
  `web/src/module.ts`.
- Produces: `creditsModule: ReaderModule<CreditsPrefs>`, plus two pure functions
  the reader calls: `skipping` and `creditList`.

`c` is free — `n`, `r`, `a`, `z`, `Z`, `b`, `d` and `s` are taken.

- [ ] **Step 1: Write the failing tests**

Create `web/src/modules/credits.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { creditSlots, nextReadable, creditsModule } from './credits'

/** Four pages, page 1 a spread: slots are [0, 1R, 1L, 2, 3]. */
const spreadAt1 = { pages: 4, spreads: [1] }
/** Four pages, no spreads: slots are [0, 1, 2, 3]. */
const flat = { pages: 4, spreads: [] as number[] }

describe('creditSlots', () => {
  it('converts page indices to every slot that page presents', () => {
    // Page 1 is a spread, so skipping it must skip both halves — skipping one
    // would leave the reader on half a credits card.
    expect(creditSlots([1], spreadAt1)).toEqual([1, 2])
  })

  it('is the identity on an edition with no spreads', () => {
    expect(creditSlots([0, 3], flat)).toEqual([0, 3])
  })
})

describe('nextReadable', () => {
  it('steps over a skipped slot', () => {
    expect(nextReadable(0, 1, [1], 4)).toBe(2)
  })

  it('steps over a consecutive stretch', () => {
    expect(nextReadable(0, 1, [1, 2], 4)).toBe(3)
  })

  it('stops at the end rather than walking past it', () => {
    expect(nextReadable(2, 1, [3], 4)).toBe(2)
  })

  it('steps backwards too', () => {
    expect(nextReadable(3, -1, [1, 2], 4)).toBe(0)
  })

  it('returns the slot unchanged when skipping is off', () => {
    // The caller passes an empty list rather than a flag: a module that is
    // switched off contributes nothing, per module.ts's contract.
    expect(nextReadable(0, 1, [], 4)).toBe(1)
  })
})

describe('the module contract', () => {
  it('claims c and nothing else', () => {
    expect(creditsModule.keys?.map((k) => k.key)).toEqual(['c'])
  })

  it('declares defaults the settings panel can generate from', () => {
    expect(creditsModule.defaults).toEqual({ skip: false, panel: true })
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npm --prefix web test -- credits`
Expected: FAIL — cannot resolve `./credits`.

- [ ] **Step 3: Implement**

Create `web/src/modules/credits.ts`:

```ts
import type { ReaderModule } from '../module'

export const PANEL_INTENT = 'credits:panel'

/** What a reader has decided about a scanlation group's own pages. */
export interface CreditsPrefs {
  /** Keep them out of the reading flow. Off by default — see the module doc. */
  skip: boolean
  /** Offer the panel that lists them. */
  panel: boolean
}

/**
 * Scanlation credit pages: kept out of the reading flow, never out of reach.
 *
 * A group's credit is what it asked for in exchange for the work, so this
 * module sets those pages aside rather than hiding them — `c` opens a list of
 * every one, and taking an entry goes there. Nothing is deleted and nothing
 * becomes unreachable.
 *
 * **`skip` is off by default.** The list is a heuristic's output — see
 * `crates/taiyaku-cli/src/credits.rs` — and a reader who has not asked for
 * pages to be skipped should not silently lose one to a false positive. The
 * panel is on, because a list nobody can find is not a section of its own.
 *
 * **Skipping never renumbers.** A slot keeps its number whether or not it is
 * skipped, so the readout jumps rather than counting differently. The slot is
 * the addressed unit — `docs/sidecar.md` is normative — and a per-reader toggle
 * that changed what an address meant would make one device's progress and one
 * reader's notes wrong on another. This is the swap precedent: what is shown
 * changes, what is written does not.
 */
export const creditsModule: ReaderModule<CreditsPrefs> = {
  id: 'credits',
  label: 'Credit pages',
  defaults: { skip: false, panel: true },
  keys: [{ key: 'c', intent: PANEL_INTENT }],
}

/** An edition's page count and its joined spreads — what slot expansion needs. */
export interface Pagination {
  pages: number
  spreads: number[]
}

/**
 * Every slot the given pages present.
 *
 * The sidecar's `credits` is in pages and the reader addresses slots, so this
 * is the one place the two meet. A joined spread presents two slots and both
 * are skipped together: half a credits card is not a page anybody wants.
 */
export function creditSlots(pages: number[], edition: Pagination): number[] {
  const wanted = new Set(pages)
  const out: number[] = []
  let slot = 0
  for (let page = 0; page < edition.pages; page++) {
    const width = edition.spreads.includes(page) ? 2 : 1
    if (wanted.has(page)) {
      for (let i = 0; i < width; i++) out.push(slot + i)
    }
    slot += width
  }
  return out
}

/**
 * The next slot in `step`'s direction that is not skipped.
 *
 * Returns `from` unchanged when every slot ahead is skipped or the edition
 * ends, so an arrow at the boundary does nothing rather than landing somewhere
 * surprising — the same answer the reader already gives at the first and last
 * slot.
 */
export function nextReadable(
  from: number,
  step: number,
  skipped: number[],
  slotCount: number,
): number {
  const hidden = new Set(skipped)
  let at = from + step
  while (at >= 0 && at < slotCount && hidden.has(at)) at += step
  return at >= 0 && at < slotCount ? at : from
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm --prefix web test -- credits`
Expected: PASS, seven tests.

- [ ] **Step 5: Register it**

In `web/src/modules/registry.ts`, import `creditsModule` and add it to the
array. That file's own doc says this is the only hand-edit a seventh module's
existence requires; extend its prose to name the eighth and its `c` key.

- [ ] **Step 6: Add the test that carries the numbering decision**

Add to `web/src/modules/credits.test.ts`:

```ts
describe('skipping is presentational', () => {
  it('changes which slot an arrow lands on, never what a slot is called', () => {
    // The whole content of the numbering decision. If someone later renumbers
    // for readability, this goes red — which is the point.
    const slotCount = 6
    const skipped = [2, 3]
    const visited: number[] = []
    let at = 0
    for (let i = 0; i < 3; i++) {
      at = nextReadable(at, 1, skipped, slotCount)
      visited.push(at)
    }
    expect(visited).toEqual([1, 4, 5])

    const withoutSkipping: number[] = []
    at = 0
    for (let i = 0; i < 3; i++) {
      at = nextReadable(at, 1, [], slotCount)
      withoutSkipping.push(at)
    }
    expect(withoutSkipping).toEqual([1, 2, 3])

    // Same slot numbers in both worlds; only which ones are reached differs.
    expect(Math.max(...visited)).toBeLessThan(slotCount)
  })
})
```

- [ ] **Step 7: Run the whole front end**

Run: `npm --prefix web test`
Expected: PASS, including `registry.test.ts`'s collision check with `c` added.

- [ ] **Step 8: Commit**

```bash
npm --prefix web run lint
git add web/src/modules/credits.ts web/src/modules/credits.test.ts web/src/modules/registry.ts
git commit -m "Add the credits reader module: skip the flow, keep the pages"
```

---

## What this plan does not do

Stated so the gap is visible rather than discovered:

- **The panel is not wired into `App.tsx`.** Task 8 ships the module, its keys
  and its pure functions with tests; mounting the list beside the page is the
  same work the note panel already does, and it belongs with whichever task
  closes O25 — the chrome, including any panel, is unreachable in the gap state
  today, so a credits panel added now would inherit that defect.
- **No credit page the solver paired is found.** `c000` local 43 is one. That
  needs content classification, which is a different feature with its own
  corpus.
- **Nothing is corrected from the reader.** The command proposes and a person
  writes; a wrongly-classified page stays reachable in the panel. Whether
  correction is wanted is a question the survey's false-positive rate answers.

## Self-review

Checked against `docs/superpowers/specs/2026-08-05-credit-pages-design.md`:

- Every one of the spec's six pieces has a task: sidecar field (1), merge rule
  (2), core signal 1 (3), CLI signal 2 (4), the command (5), wasm and resolver
  (7), the reader module (8). The survey the spec makes a gate is Task 6.
- The spec's four refusals are all in Task 5; the two it puts in `unpaired_runs`
  are in Task 3's last two tests.
- The spec's testing section maps to Tasks 3, 6 and 8, including the
  spread-inside-a-run fixture risk 5 asks for and the "assert the set, not the
  count" corpus check.
- `at_member_start` appears nowhere, and Task 3's second test is the assertion
  that keeps it from returning.
- Type names are consistent across tasks: `UnpairedRun`, `Recurrence`,
  `MAX_CREDIT_RUN`, `Resolver.credits`, `creditSlots`, `nextReadable`,
  `Merged::DroppedCredits`.
- Every existing symbol the plan names was read in the tree before being cited:
  `Sidecar::from_json` / `to_json` / `edition_of` / `runs_of`, `Edition::slots` /
  `locate` / `total_pages`, `Work::reading`, `Run::{a, b}`, `PageRange` (which is
  `Copy`, so the Task 3 body may take `run.a` by value), `Merged`,
  `option_value`, `reject_unknown_flags`, and `ReaderModule`. Two names in an
  earlier draft of this plan were wrong — `Sidecar::parse` and `.emit()` — and
  are corrected to `from_json` and `to_json` throughout.
- One deliberate deviation from the spec: it lists the panel as part of the
  reader module, and this plan ships the module without mounting the panel, for
  the O25 reason above. Named in "What this plan does not do" rather than
  silently dropped.

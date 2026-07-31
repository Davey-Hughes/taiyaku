# Hub-and-Spoke Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a sidecar able to hold more than two editions, and make `resolve` answer any ordered pair of them by composing through the hub.

**Architecture:** A new `route` module in `taiyaku-core` owns path selection — identity, a stored edge in either direction, or two edges chained through the hub — and returns a `Route` that holds the composed runs so a reader builds them once rather than per page. A new `Sidecar::merge_spoke` owns the rules for adding an edition to an existing document, so the CLI stays a thin caller and a browser reader inherits the same validation.

**Tech Stack:** Rust (2024 edition, stable channel), `serde`/`serde_json`, no new dependencies. `taiyaku-core` must keep compiling to `wasm32-unknown-unknown`.

**Design spec:** `docs/superpowers/specs/2026-07-30-hub-and-spoke-design.md`. Read it before Task 1.

## Global Constraints

- **No new dependencies.** The workspace has `image`, `zip`, `rayon`, `serde` and nothing else, and the wasm gate exists to keep it that way.
- **`taiyaku-core` must stay portable.** No `std::fs`, no `std::time`, no threads. CI builds it for `wasm32-unknown-unknown`; verify locally with `cargo check -p taiyaku-core --target wasm32-unknown-unknown`.
- **No `taiyaku` format version bump.** `VERSION` stays `1`. Nothing about the stored format changes — a three-edition document is already legal.
- **Errors are `Result<_, String>` and name what a person would search the file for.** This is established in `runs_of`: an error says `` `9-2` `` because that is the string in the JSON, not "invalid range".
- **Comments explain why, not what.** Match the surrounding density — this codebase carries a measurement or a rationale above most non-obvious blocks, and nothing above the obvious ones.
- **Full check before any commit:** `cargo fmt`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace`.
- **Composition is read-time only.** A composed alignment is never written to a sidecar. It carries no `margin`, and the two legs' margins are not combinable.

---

## File Structure

| File | Responsibility | Task |
| ---- | -------------- | ---- |
| `crates/taiyaku-core/src/route.rs` | **Create.** `Via`, `Route`, `route()`, `invert()`, `resolve()`. Path selection and page lookup. | 1–4 |
| `crates/taiyaku-core/src/sidecar.rs` | **Modify.** Loses `resolve`; `slot_index` and `edge_between` become `pub(crate)`; gains `Anchor`, `Merged`, `merge_spoke`. | 2, 5 |
| `crates/taiyaku-core/src/lib.rs` | **Modify.** Declare `mod route`, re-export the new names. | 2, 5 |
| `crates/taiyaku-cli/src/main.rs` | **Modify.** `align` merges; `lookup` names its path; `topology` gains a verdict column. | 6, 7, 8 |
| `crates/taiyaku-cli/src/corpus.rs` | **Create.** The `TAIYAKU_CORPUS`-gated standing check. | 9 |
| `docs/sidecar.md`, `docs/notes.md`, `docs/status.md` | **Modify.** Three files currently describe the unbuilt state. | 10 |

`resolve` moves out of `sidecar.rs` because that file is 961 lines and routing is now a subject of its own rather than one function. `sidecar.rs` keeps the format — parse, emit, validate, merge.

---

### Task 1: `invert`, and the `route` module

**Files:**
- Create: `crates/taiyaku-core/src/route.rs`
- Modify: `crates/taiyaku-core/src/lib.rs:11-23`

**Interfaces:**
- Consumes: `crate::{PageRange, Run}` (already public).
- Produces: `pub fn invert(runs: &[Run]) -> Vec<Run>` and `pub enum Via { Identity, Direct, Inverted, Hub(String) }`, both used by Tasks 2–4, 7 and 9.

- [ ] **Step 1: Create the module with the failing test**

Create `crates/taiyaku-core/src/route.rs`:

```rust
//! Which path a lookup walks between two editions, and the answer it gives.
//!
//! `sidecar.rs` owns the document; this owns what can be asked of one. The two
//! were the same file until composition made routing a subject of its own.

use crate::Run;

/// Which path a lookup walked.
///
/// Reported rather than hidden because a composed answer is weaker than a
/// direct one — measured at about 1.7% of pages lost to hub gaps — and a reader
/// that cannot tell them apart has been told less than the document knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// The same edition on both sides.
    Identity,
    /// An alignment stored in the direction asked for.
    Direct,
    /// An alignment stored the other way round, reversed.
    Inverted,
    /// Two alignments chained through the named hub.
    Hub(String),
}

/// Reverse an alignment: B→A from A→B.
///
/// Paired runs swap sides. Gaps drop, and that is not a loss: a gap records
/// pages of A with no counterpart in B, which says nothing whatever about B.
///
/// This is what lets a sidecar store N-1 edges rather than 2(N-1). It rests on
/// a measurement rather than an assumption — the solver is symmetric on the
/// pairing relation, checked across five pairs in two series including the
/// widest length mismatch in the corpus (195 slots against 240), where every
/// paired run is identical to its transpose. `calibrate` is asymmetric on
/// paper, since `signal` is a median of per-A-page maxima, and it does not
/// reach the DP's output on any material measured. See `notes.md`.
pub fn invert(runs: &[Run]) -> Vec<Run> {
    let mut inverted: Vec<Run> = runs
        .iter()
        .filter_map(|run| run.b.map(|b| Run::paired(b, run.a)))
        .collect();
    // The solver is monotonic, so swapping a sorted list yields a sorted one —
    // but a hand-edited document need not be monotonic and nothing in the
    // format enforces it. Unsorted, `compress` emits one run per page, which
    // would make a sound composed alignment read as confetti.
    inverted.sort_by_key(|run| run.a.start);
    inverted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageRange;

    /// The ordinary case: a paired run reverses, and reversing twice is the
    /// identity. If this were lossy the star would need both directions stored.
    #[test]
    fn inverting_swaps_the_sides_and_round_trips() {
        let runs = vec![
            Run::paired(PageRange::new(0, 3), PageRange::new(10, 13)),
            Run::paired(PageRange::new(4, 9), PageRange::new(20, 25)),
        ];

        let back = invert(&runs);

        assert_eq!(
            back,
            vec![
                Run::paired(PageRange::new(10, 13), PageRange::new(0, 3)),
                Run::paired(PageRange::new(20, 25), PageRange::new(4, 9)),
            ]
        );
        assert_eq!(invert(&back), runs, "inverting twice must change nothing");
    }

    /// A gap is one-sided by construction. Carrying it across would claim B has
    /// pages the alignment never mentioned.
    #[test]
    fn inverting_drops_gaps() {
        let runs = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 2), PageRange::new(1, 1)),
        ];

        let back = invert(&runs);

        assert_eq!(
            back,
            vec![
                Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
                Run::paired(PageRange::new(1, 1), PageRange::new(2, 2)),
            ]
        );
    }

    /// A hand-edited file can list runs in any order. Left unsorted, the
    /// composed output compresses into one run per page.
    #[test]
    fn inverting_sorts_by_the_new_source_side() {
        let runs = vec![
            Run::paired(PageRange::new(0, 1), PageRange::new(8, 9)),
            Run::paired(PageRange::new(2, 3), PageRange::new(0, 1)),
        ];

        let back = invert(&runs);

        assert_eq!(back[0].a, PageRange::new(0, 1));
        assert_eq!(back[1].a, PageRange::new(8, 9));
    }
}
```

- [ ] **Step 2: Declare the module and re-export**

In `crates/taiyaku-core/src/lib.rs`, add `mod route;` to the module list (line 11-13, keeping alphabetical order) so it reads:

```rust
mod route;
mod sidecar;
mod slot;
mod solve;
```

And add a re-export beneath the existing `pub use sidecar::{...}` block:

```rust
pub use route::{Via, invert};
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core route::`

Expected: compiles and the three tests PASS. (`invert` is written in Step 1 — this task is a leaf with no prior interface to stub, so the "fail first" cycle would mean deleting the body and putting it back. Instead, verify the tests genuinely bite by temporarily changing `filter_map(|run| run.b.map(|b| Run::paired(b, run.a)))` to `filter_map(|run| run.b.map(|b| Run::paired(run.a, b)))` and re-running.)

Expected with that change: `inverting_swaps_the_sides_and_round_trips` FAILS. Restore the correct line.

- [ ] **Step 4: Run the full check**

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p taiyaku-core --target wasm32-unknown-unknown
```

Expected: all pass — 57 existing tests (42 in `taiyaku-core`, 15 in `taiyaku-cli`) plus 3 new. Note for Task 10: `status.md` claims 47, which is already stale.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-core/src/route.rs crates/taiyaku-core/src/lib.rs
git commit -m "Reverse an alignment, so one stored edge serves both directions"
```

---

### Task 2: `Route` and `route()`, with `resolve` moved across

Pure refactor. `resolve` keeps its signature and its behaviour; every existing test must pass **unchanged**.

**Files:**
- Modify: `crates/taiyaku-core/src/route.rs`
- Modify: `crates/taiyaku-core/src/sidecar.rs:119-155` (`slot_index`, `resolve`), `:378-382` (`edge_between`), `:553-592` (the `REAL` fixture), `:636-671` (the resolve test)
- Modify: `crates/taiyaku-core/src/lib.rs`

**Interfaces:**
- Consumes: `Via` and `invert` from Task 1; `Sidecar::runs_of`, `Edition::slots`, `crate::counterpart`.
- Produces:
  - `pub struct Route { pub via: Via, .. }` with `pub fn resolve(&self, page: u32) -> Option<Slot>` and `pub fn runs(&self) -> &[Run]`
  - `pub fn route(sidecar: &Sidecar, from: &str, to: &str) -> Option<Route>`
  - `pub fn resolve(sidecar: &Sidecar, from: &str, to: &str, page: u32) -> Option<Slot>`
  - `pub(crate) fn slot_index`, `pub(crate) fn edge_between` in `sidecar.rs`
  - `#[cfg(test)] pub(crate) const REAL: &str` in `sidecar.rs`

  Used by Tasks 3, 4, 7 and 9.

- [ ] **Step 1: Widen the two `sidecar.rs` helpers `route` needs**

In `crates/taiyaku-core/src/sidecar.rs`, change line 119 from `fn slot_index(` to:

```rust
pub(crate) fn slot_index(slots: &[Slot], slot: Slot) -> Option<usize> {
```

and line 378 from `fn edge_between(` to:

```rust
    pub(crate) fn edge_between(&self, from: &str, to: &str) -> Option<&Alignment> {
```

- [ ] **Step 2: Lift the `REAL` fixture out of `mod tests` so `route.rs` can use it**

In `crates/taiyaku-core/src/sidecar.rs`, cut the `const REAL: &str = r#"..."#;` declaration (currently inside `mod tests`, lines 557-592) and paste it at module level immediately above `#[cfg(test)] mod tests {`, prefixed:

```rust
/// The document from `docs/sidecar.md`, verbatim: the real alignment of
/// Go-Toubun volume 1. Shared with `route.rs`, whose tests resolve through it.
#[cfg(test)]
pub(crate) const REAL: &str = r#"{
```

The body is unchanged. Inside `mod tests`, `use super::*;` already brings it into scope, so no test edits are needed for this step.

- [ ] **Step 3: Delete `resolve` from `sidecar.rs`**

Delete lines 125-155 — the doc comment beginning `/// Resolve a page of one edition to its counterpart` through the closing brace of `pub fn resolve`. Also delete the test `a_page_resolves_through_a_stored_sidecar` (lines 632-671 including its doc comment); it moves to `route.rs` in Step 5.

- [ ] **Step 4: Write `Route` and `route()` in `route.rs`**

Add to `crates/taiyaku-core/src/route.rs`, after `invert` and before `mod tests`. Update the `use` line at the top to `use crate::{Run, Sidecar, Slot, counterpart};`.

```rust
/// The correspondence between two editions, in slot indices, and how it was
/// found.
///
/// An object rather than a bare function because composing is not free: it
/// reads two alignments, builds two slot tables and chains them. At one page
/// that is nothing; a reader paging through a 195-page volume would pay it 195
/// times. Building it once is the difference between a lookup and a scan.
pub struct Route {
    pub via: Via,
    runs: Vec<Run>,
    from_slots: Vec<Slot>,
    to_slots: Vec<Slot>,
}

impl Route {
    /// Where a page of the `from` edition lands.
    ///
    /// A page reference with no half names the first slot of that page, which
    /// is what makes `47` mean the same thing whether page 47 is whole or a
    /// spread. The answer carries a half because that is what a reader has to
    /// display — possibly one side of a joined spread.
    pub fn resolve(&self, page: u32) -> Option<Slot> {
        let here = crate::sidecar::slot_index(&self.from_slots, Slot { page, half: None })? as u32;
        let there = counterpart(&self.runs, here)?;
        self.to_slots.get(there as usize).copied()
    }

    /// The runs this route walks, in slot indices.
    ///
    /// Exposed because a caller reporting how far the answer can be trusted has
    /// to judge what will actually be walked, which for a composed route is
    /// neither of the stored run lists.
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }
}

/// Find the path from one edition to another, or `None` when there is none.
///
/// Order: identity, a stored edge in the direction asked for, the same edge
/// reversed, then two edges chained through the hub.
///
/// **A stored edge is the whole answer, including when it is there and cannot
/// be read.** Falling through to the hub in that case would answer a different
/// question than the one asked — not "where does this edge send the page" but
/// "where does some other path send it". `Sidecar::trust` makes the same
/// choice for the same reason.
pub fn route(sidecar: &Sidecar, from: &str, to: &str) -> Option<Route> {
    // Existence is checked before identity: `route("nosuch", "nosuch")` must be
    // `None` rather than a confident identity over an edition that is not here.
    let reading = sidecar.work.reading;
    let from_slots = sidecar.editions.get(from)?.slots(reading);
    let to_slots = sidecar.editions.get(to)?.slots(reading);

    if from == to {
        // An edition corresponds to itself as a property of the edition, not of
        // a run list that need not exist — the same rule `trust` applies.
        let runs = match from_slots.len() {
            0 => Vec::new(),
            n => vec![Run::paired(
                crate::PageRange::new(0, n as u32 - 1),
                crate::PageRange::new(0, n as u32 - 1),
            )],
        };
        return Some(Route {
            via: Via::Identity,
            runs,
            from_slots,
            to_slots,
        });
    }

    if let Some(edge) = sidecar.edge_between(from, to) {
        let stored = sidecar.runs_of(edge).ok()?;
        let (runs, via) = match edge.from == from {
            true => (stored, Via::Direct),
            false => (invert(&stored), Via::Inverted),
        };
        return Some(Route {
            via,
            runs,
            from_slots,
            to_slots,
        });
    }

    None
}

/// Resolve a page of one edition to its counterpart in another, from a stored
/// sidecar alone.
///
/// This is the whole reader API in one call. A caller resolving more than one
/// page should hold a `Route` instead, which builds the path once.
pub fn resolve(sidecar: &Sidecar, from: &str, to: &str, page: u32) -> Option<Slot> {
    route(sidecar, from, to)?.resolve(page)
}
```

- [ ] **Step 5: Move the resolve test into `route.rs`**

Add to `route.rs`'s `mod tests`, and extend its `use` line to `use crate::{Half, PageRange, Sidecar};`:

```rust
    /// The reader's whole job, done from the stored document with no archives
    /// and no solving. These expectations come from the aligner's own output on
    /// Go-Toubun volume 1, so agreement means the file carries what was solved.
    #[test]
    fn a_page_resolves_through_a_stored_sidecar() {
        let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");

        // Ordinary page inside the long final run. Raw 100 lands on English 94,
        // not 99: five of the seven spreads sit before it, and each absorbs a
        // page. Working that out by eye gets it wrong, which is the point.
        assert_eq!(
            resolve(&s, "raw-mono", "en-mono", 100),
            Some(Slot {
                page: 94,
                half: None
            })
        );
        // Raw page 47 lands on the right half of the spread at English page 45.
        // Getting the slot arithmetic wrong here shows the wrong half.
        assert_eq!(
            resolve(&s, "raw-mono", "en-mono", 47),
            Some(Slot {
                page: 45,
                half: Some(Half::Right)
            })
        );
        assert_eq!(
            resolve(&s, "raw-mono", "en-mono", 48),
            Some(Slot {
                page: 45,
                half: Some(Half::Left)
            })
        );
        // Page 1 is the blank the solver refused to pair.
        assert_eq!(resolve(&s, "raw-mono", "en-mono", 1), None);
        // Beyond the volume.
        assert_eq!(resolve(&s, "raw-mono", "en-mono", 9999), None);
        assert_eq!(route(&s, "raw-mono", "en-mono").map(|r| r.via), Some(Via::Direct));
    }
```

Note what is **not** carried over: the old assertion `resolve(&s, "en-mono", "raw-mono", 0) == None`. Task 3 replaces it with the opposite expectation.

- [ ] **Step 6: Update the re-exports**

In `crates/taiyaku-core/src/lib.rs`, replace the Task 1 line with:

```rust
pub use route::{Route, Via, invert, resolve, route};
```

and remove `resolve` from the `pub use sidecar::{...}` list, leaving:

```rust
pub use sidecar::{
    Alignment, Edition, Member, Quality, RunEntry, Sidecar, SlotSpan, SolverInfo, Source, VERSION,
    Work, entries_from_runs,
};
```

- [ ] **Step 7: Run the full check**

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p taiyaku-core --target wasm32-unknown-unknown
```

Expected: all pass. `crates/taiyaku-cli/src/main.rs:391` calls `taiyaku_core::resolve` and needs no change — the signature is identical.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-core/src/route.rs crates/taiyaku-core/src/sidecar.rs crates/taiyaku-core/src/lib.rs
git commit -m "Give a lookup a route to hold, not a function to repeat"
```

---

### Task 3: Answer a reversed question, and an edition asked about itself

**Files:**
- Modify: `crates/taiyaku-core/src/route.rs`

**Interfaces:**
- Consumes: `route()`, `Via` from Task 2.
- Produces: no new signatures — `Via::Inverted` and `Via::Identity` start being returned.

The code for both was written in Task 2. This task is the tests that prove it, plus the deliberate reversal of one existing expectation.

- [ ] **Step 1: Write the failing tests**

Add to `route.rs`'s `mod tests`:

```rust
    /// The behaviour change this work turns on. `resolve` previously answered
    /// only in the stored direction, so a reader holding a two-edition document
    /// could go raw→en and not back. The relation is symmetric — measured, not
    /// assumed — so the document already carried this answer and the library
    /// declined to give it.
    #[test]
    fn a_stored_edge_answers_in_both_directions() {
        let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");

        // The exact inverse of the forward case: raw 47 lands on 45R, so
        // English page 45 — whose first slot is 45R — lands back on raw 47.
        assert_eq!(
            resolve(&s, "en-mono", "raw-mono", 45),
            Some(Slot {
                page: 47,
                half: None
            })
        );
        assert_eq!(
            resolve(&s, "en-mono", "raw-mono", 0),
            Some(Slot {
                page: 0,
                half: None
            })
        );
        assert_eq!(
            route(&s, "en-mono", "raw-mono").map(|r| r.via),
            Some(Via::Inverted)
        );
        // Raw page 1 is a gap, so nothing in English reaches it. Inverting must
        // not invent a counterpart for it.
        assert!(
            !(0..187).any(|p| resolve(&s, "en-mono", "raw-mono", p)
                == Some(Slot { page: 1, half: None })),
            "a dropped gap must not become a pairing"
        );
    }

    /// An edition corresponds to itself, and `trust` has always said so. Before
    /// this, `resolve` disagreed unless a self-alignment happened to be stored.
    #[test]
    fn an_edition_resolves_to_itself() {
        let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");

        assert_eq!(
            resolve(&s, "raw-mono", "raw-mono", 100),
            Some(Slot {
                page: 100,
                half: None
            })
        );
        // A spread page answers with its first slot, exactly as every other
        // lookup does — English page 45 is a spread, and `45` names 45R.
        assert_eq!(
            resolve(&s, "en-mono", "en-mono", 45),
            Some(Slot {
                page: 45,
                half: Some(Half::Right)
            })
        );
        assert_eq!(
            route(&s, "raw-mono", "raw-mono").map(|r| r.via),
            Some(Via::Identity)
        );
        // Identity is a property of the edition, but the edition still has to
        // exist.
        assert!(route(&s, "nosuch", "nosuch").is_none());
    }
```

- [ ] **Step 2: Run to verify they pass**

Run: `cargo test -p taiyaku-core route::`

Expected: PASS. Both behaviours were implemented in Task 2; these tests are what makes them intentional rather than incidental. If `a_stored_edge_answers_in_both_directions` fails on the `Via::Inverted` assertion, the `edge.from == from` branch in `route()` is backwards.

- [ ] **Step 3: Verify the tests bite**

Temporarily change `route()`'s edge branch to `true => (stored, Via::Direct), false => return None,` and re-run.

Expected: `a_stored_edge_answers_in_both_directions` FAILS. Restore it.

- [ ] **Step 4: Run the full check and commit**

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add crates/taiyaku-core/src/route.rs
git commit -m "Answer a lookup the other way round, and an edition about itself"
```

---

### Task 4: Compose through the hub

**Files:**
- Modify: `crates/taiyaku-core/src/route.rs`

**Interfaces:**
- Consumes: `route()` from Task 2, `crate::compose`.
- Produces: `Via::Hub(String)` starts being returned. Used by Tasks 7 and 9.

- [ ] **Step 1: Write the failing tests**

Add to `route.rs`'s `mod tests`:

```rust
    /// Three editions in a star: hub `a`, spokes `b` and `c`, and only the two
    /// hub edges stored. `b -> c` is the question a reader with three editions
    /// asks, and until now the document could not answer it.
    const STAR: &str = r#"{
      "taiyaku": 1,
      "work": { "title": "T", "reading": "rtl" },
      "editions": {
        "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
        "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] },
        "c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] }
      },
      "hub": "a",
      "alignments": [
        { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "0-29" }] },
        { "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }
      ]
    }"#;

    #[test]
    fn two_spokes_resolve_through_the_hub() {
        let s = Sidecar::from_json(STAR).expect("parses");

        // b 10 -> a 10 -> c 8. The first leg is the stored a->b edge inverted;
        // the second is stored in the direction needed.
        assert_eq!(
            resolve(&s, "b", "c", 10),
            Some(Slot {
                page: 8,
                half: None
            })
        );
        // And back, which crosses the same two edges the other way round.
        assert_eq!(
            resolve(&s, "c", "b", 8),
            Some(Slot {
                page: 10,
                half: None
            })
        );
        assert_eq!(
            route(&s, "b", "c").map(|r| r.via),
            Some(Via::Hub("a".into()))
        );
        // Composing two constant offsets is one constant offset, so the whole
        // thing stays a single run rather than fragmenting into 28.
        assert_eq!(route(&s, "b", "c").expect("routes").runs().len(), 1);
    }

    /// The cost hub-and-spoke is measured to carry, at the sidecar layer: about
    /// 1.7% of pages. A page that exists in a spoke but not in the hub has
    /// nowhere to land, so the chain ends there — and the reader sees "no
    /// counterpart" rather than the wrong page, which is the safe direction.
    #[test]
    fn a_page_landing_on_a_hub_gap_has_no_counterpart() {
        let s = Sidecar::from_json(STAR).expect("parses");

        // a has no pairing for pages 0 and 1, so b 0 and b 1 reach the hub and
        // stop. c does have pages there; a direct b->c edge would find them.
        assert_eq!(resolve(&s, "b", "c", 0), None);
        assert_eq!(resolve(&s, "b", "c", 1), None);
        assert_eq!(
            resolve(&s, "b", "c", 2),
            Some(Slot {
                page: 0,
                half: None
            })
        );
    }

    /// A direct edge layered over the star wins, which is the mitigation
    /// `sidecar.md` designs for the pair where composition demonstrably loses.
    #[test]
    fn a_direct_edge_is_preferred_to_the_hub() {
        let with_edge = STAR.replace(
            r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }"#,
            r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] },
               { "from": "b", "to": "c", "by": "hand", "runs": [{ "a": "0-29", "b": "0-29" }] }"#,
        );
        let s = Sidecar::from_json(&with_edge).expect("parses");

        assert_eq!(route(&s, "b", "c").map(|r| r.via), Some(Via::Direct));
        // The direct edge reaches the pages the hub gap swallowed.
        assert_eq!(
            resolve(&s, "b", "c", 0),
            Some(Slot {
                page: 0,
                half: None
            })
        );
    }

    /// An edition nothing aligns to is unreachable, and saying so is the whole
    /// point — a partial answer here would be a wrong page.
    #[test]
    fn a_missing_leg_has_no_route() {
        let lonely = STAR.replace(
            r#""c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] }"#,
            r#""c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 30 }] },
               "d": { "label": "D", "members": [{ "path": "d.cbz", "sha256": "03", "pages": 30 }] }"#,
        );
        let s = Sidecar::from_json(&lonely).expect("parses");

        assert!(route(&s, "b", "d").is_none());
        assert!(route(&s, "d", "a").is_none());
    }

    /// A stored edge that cannot be read is the answer, not a reason to go
    /// looking for another path. `trust` makes the same choice: composing here
    /// would answer a question the caller did not ask.
    #[test]
    fn an_unreadable_direct_edge_does_not_fall_through_to_the_hub() {
        let broken = STAR.replace(
            r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] }"#,
            r#"{ "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "2-29", "b": "0-27" }] },
               { "from": "b", "to": "c", "by": "hand", "runs": [{ "a": "0-1", "b": "0-9" }] }"#,
        );
        let s = Sidecar::from_json(&broken).expect("parses");

        assert!(route(&s, "b", "c").is_none());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-core route::`

Expected: `two_spokes_resolve_through_the_hub`, `a_page_landing_on_a_hub_gap_has_no_counterpart` FAIL — `route` returns `None` for any pair without a stored edge. `a_direct_edge_is_preferred_to_the_hub`, `a_missing_leg_has_no_route` and `an_unreadable_direct_edge_does_not_fall_through_to_the_hub` already PASS.

- [ ] **Step 3: Add hub composition**

In `route.rs`, replace the trailing `None` of `route()` (the last line before the closing brace) with:

```rust
    // No stored edge, so chain two through the hub. This is the topology
    // `sidecar.md` specifies and `topology` measured: 0.00% disagreement across
    // 4,815 page comparisons, about 1.7% of pages lost to hub gaps.
    if !sidecar.editions.contains_key(&sidecar.hub) {
        return None;
    }
    let first = leg(sidecar, from, &sidecar.hub)?;
    let second = leg(sidecar, &sidecar.hub, to)?;
    Some(Route {
        via: Via::Hub(sidecar.hub.clone()),
        runs: crate::compose(&first, &second),
        from_slots,
        to_slots,
    })
}

/// One hop of a composition: a stored edge, oriented the way the caller needs.
///
/// A star stores `hub -> spoke` edges, so the first leg of every spoke-to-spoke
/// lookup is an inversion. That costs nothing the solver would not have
/// produced anyway — see `invert`.
fn leg(sidecar: &Sidecar, from: &str, to: &str) -> Option<Vec<Run>> {
    let edge = sidecar.edge_between(from, to)?;
    let stored = sidecar.runs_of(edge).ok()?;
    Some(match edge.from == from {
        true => stored,
        false => invert(&stored),
    })
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-core route::`

Expected: all route tests PASS.

- [ ] **Step 5: Run the full check and commit**

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p taiyaku-core --target wasm32-unknown-unknown
git add crates/taiyaku-core/src/route.rs
git commit -m "Walk the hub, so three editions are a document a reader can use"
```

---

### Task 5: `Sidecar::merge_spoke`

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs`
- Modify: `crates/taiyaku-core/src/lib.rs`

**Interfaces:**
- Consumes: `Edition`, `Alignment`, `Direction`.
- Produces:
  ```rust
  pub enum Anchor { Hub, Direct }
  pub enum Merged { Added, Replaced }
  impl Sidecar {
      pub fn merge_spoke(
          &mut self,
          reading: Direction,
          anchor: Anchor,
          from_side: &Edition,
          name: &str,
          edition: Edition,
          alignment: Alignment,
      ) -> Result<Merged, String>;
  }
  ```
  Used by Task 6 and Task 9.

- [ ] **Step 1: Write the failing tests**

Add to `sidecar.rs`'s `mod tests`. Put this fixture builder above them:

```rust
    /// A two-edition document to merge into, and the pieces a merge needs.
    fn two_edition() -> Sidecar {
        Sidecar::from_json(
            r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 30 }] },
            "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 30 }] }
          },
          "hub": "a",
          "alignments": [
            { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-29", "b": "0-29" }] }
          ]
        }"#,
        )
        .expect("parses")
    }

    fn edition(path: &str, sha: &str) -> Edition {
        Edition {
            label: path.into(),
            members: vec![Member {
                path: path.into(),
                sha256: sha.into(),
                pages: 30,
            }],
            spreads: Vec::new(),
        }
    }

    fn edge(from: &str, to: &str) -> Alignment {
        Alignment {
            from: from.into(),
            to: to.into(),
            by: Source::Solver,
            solver: None,
            quality: None,
            runs: vec![RunEntry {
                a: SlotSpan::parse("0-29").unwrap(),
                b: Some(SlotSpan::parse("0-29").unwrap()),
                margin: None,
                by: None,
                note: None,
            }],
        }
    }

    /// The ordinary case, and the one the whole piece of work exists for: a
    /// third edition joins a document that had two, and the hub does not move.
    #[test]
    fn a_spoke_joins_an_existing_document() {
        let mut s = two_edition();

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "c",
                edition("c.cbz", "02"),
                edge("a", "c"),
            )
            .expect("merges");

        assert_eq!(merged, Merged::Added);
        assert_eq!(s.editions.len(), 3);
        assert_eq!(s.alignments.len(), 2);
        assert_eq!(s.hub, "a", "a merge must never move the hub");
        // And the point of it: the new edition is reachable from the old one.
        assert_eq!(
            crate::route(&s, "b", "c").map(|r| r.via),
            Some(Via::Hub("a".into()))
        );
    }

    /// Re-solving a spoke after fixing its archive is ordinary. Refusing it
    /// would make the common case the annoying one, so it replaces — edition
    /// and edge together, so neither can outlive the other.
    #[test]
    fn re_aligning_a_spoke_replaces_it_rather_than_duplicating() {
        let mut s = two_edition();

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b2.cbz", "99"),
                edge("a", "b"),
            )
            .expect("merges");

        assert_eq!(merged, Merged::Replaced);
        assert_eq!(s.editions.len(), 2);
        assert_eq!(s.alignments.len(), 1, "the old edge must not survive");
        assert_eq!(s.editions["b"].members[0].sha256, "99");
    }

    /// The answer to "what does a third edition align against". The star says
    /// the hub, and the document names its own hub, so this is checkable rather
    /// than conventional.
    #[test]
    fn a_spoke_aligned_against_a_non_hub_edition_is_refused() {
        let mut s = two_edition();

        let err = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("b.cbz", "01"),
                "c",
                edition("c.cbz", "02"),
                edge("b", "c"),
            )
            .expect_err("must refuse");

        assert!(err.contains("`a`"), "the error must name the hub: {err}");
        assert!(err.contains("--direct"), "and the way round it: {err}");
        assert_eq!(s.editions.len(), 2, "a refused merge must change nothing");
    }

    /// `Anchor::Direct` is the mitigation the format already designs: a direct
    /// edge for a pair where composition demonstrably loses.
    #[test]
    fn a_direct_edge_may_start_anywhere_the_document_defines() {
        let mut s = two_edition();

        s.merge_spoke(
            Direction::RightToLeft,
            Anchor::Direct,
            &edition("b.cbz", "01"),
            "c",
            edition("c.cbz", "02"),
            edge("b", "c"),
        )
        .expect("merges");

        assert_eq!(s.alignments.len(), 2);
        assert_eq!(s.alignments[1].from, "b");
        assert_eq!(s.hub, "a");
    }

    /// Reading direction decides which half of a joined spread comes first, so
    /// merging under the wrong one misfiles every spread in the file while
    /// producing valid JSON.
    #[test]
    fn a_merge_under_the_wrong_reading_direction_is_refused() {
        let mut s = two_edition();

        let err = s
            .merge_spoke(
                Direction::LeftToRight,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "c",
                edition("c.cbz", "02"),
                edge("a", "c"),
            )
            .expect_err("must refuse");

        assert!(err.contains("reading"), "{err}");
    }

    /// The library holds two files both claiming to be volume 1 of the same
    /// work, only one of which is. If the hub's archives changed under the
    /// document, every existing spoke is aligned to something else.
    #[test]
    fn a_merge_against_changed_hub_archives_is_refused() {
        let mut s = two_edition();

        let err = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a-redownloaded.cbz", "ff"),
                "c",
                edition("c.cbz", "02"),
                edge("a", "c"),
            )
            .expect_err("must refuse");

        assert!(
            err.contains("a-redownloaded.cbz"),
            "the error must name the file: {err}"
        );
        assert_eq!(s.editions.len(), 2);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-core sidecar::`

Expected: FAIL to compile — `no variant or associated item named 'Hub' found for enum Anchor` / `cannot find type Anchor`.

- [ ] **Step 3: Write `Anchor`, `Merged` and `merge_spoke`**

Add to `crates/taiyaku-core/src/sidecar.rs`, above `impl Sidecar`:

```rust
/// Where an alignment being merged is allowed to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The star: the alignment must begin at the document's hub, so every
    /// edition is one hop from every other.
    Hub,
    /// A direct edge deliberately layered over the star, for a pair where
    /// composition demonstrably loses. The measurements name the candidate:
    /// the two editions furthest from the hub lose 3.5% to each other.
    Direct,
}

/// What a merge did, so a caller can say so rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merged {
    Added,
    Replaced,
}
```

And inside `impl Sidecar`:

```rust
    /// Add an edition and its alignment to a document that already exists.
    ///
    /// The rules live here rather than in the CLI because a browser reader
    /// writing a correction has to apply the same ones, and because they are
    /// worth testing without archives present.
    ///
    /// `from_side` is the freshly described `alignment.from` edition. It is
    /// passed rather than looked up because the check that matters is whether
    /// the document's copy and the just-aligned copy are the same content.
    pub fn merge_spoke(
        &mut self,
        reading: Direction,
        anchor: Anchor,
        from_side: &Edition,
        name: &str,
        edition: Edition,
        alignment: Alignment,
    ) -> Result<Merged, String> {
        if reading != self.work.reading {
            return Err(format!(
                "this document reads {:?} and the alignment was solved {reading:?}; \
                 reading order decides which half of a spread comes first, so \
                 merging would misfile every spread in the file",
                self.work.reading
            ));
        }
        let start = alignment.from.clone();
        if anchor == Anchor::Hub && start != self.hub {
            return Err(format!(
                "the hub of this document is `{}`; align against it, or pass \
                 --direct to record a `{start}` -> `{name}` edge deliberately",
                self.hub
            ));
        }
        let stored = self
            .editions
            .get(&start)
            .ok_or_else(|| format!("this document defines no edition named `{start}`"))?;

        // Content, not paths. A path in the document is relative to the
        // document and the caller's is relative to the working directory, so
        // only the hashes can be compared — and their order matters, because it
        // is what defines the flattened page sequence.
        let mismatch = stored
            .members
            .iter()
            .zip(&from_side.members)
            .position(|(here, there)| here.sha256 != there.sha256);
        if let Some(i) = mismatch {
            return Err(format!(
                "`{start}` in this document is not the edition just aligned: member {i} is \
                 `{}` here and `{}` there. Re-solve the work rather than merging into it.",
                stored.members[i].path, from_side.members[i].path
            ));
        }
        if stored.members.len() != from_side.members.len() {
            return Err(format!(
                "`{start}` has {} member(s) in this document and {} in the alignment. \
                 Re-solve the work rather than merging into it.",
                stored.members.len(),
                from_side.members.len()
            ));
        }
        if name == start {
            return Err(format!(
                "`{name}` is the edition being aligned against; an edition cannot be its \
                 own counterpart"
            ));
        }

        let replaced = self.editions.insert(name.to_string(), edition).is_some();
        // Edition and edge are replaced together. A surviving edge would
        // address pages of an edition that is no longer there.
        self.alignments
            .retain(|a| !(a.from == start && a.to == name) && !(a.from == name && a.to == start));
        self.alignments.push(alignment);
        Ok(match replaced {
            true => Merged::Replaced,
            false => Merged::Added,
        })
    }
```

Add `Anchor` and `Merged` to the `pub use sidecar::{...}` list in `lib.rs`, and add `Via` plus `route` to the test module's imports in `sidecar.rs` (`use crate::{Via, route};` inside `mod tests`).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-core sidecar::`

Expected: PASS, six new tests.

- [ ] **Step 5: Run the full check and commit**

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p taiyaku-core --target wasm32-unknown-unknown
git add crates/taiyaku-core/src/sidecar.rs crates/taiyaku-core/src/lib.rs
git commit -m "Let a sidecar take a third edition, and say what it took"
```

---

### Task 6: `align --write` merges

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs:34-55` (usage), `:570-650` (the `--write` block), `:21-24` (imports)

**Interfaces:**
- Consumes: `Anchor`, `Merged`, `Sidecar::merge_spoke` from Task 5.
- Produces: the `--direct` flag and merge behaviour, used by Task 9.

- [ ] **Step 1: Extend the usage text**

In `run()`, replace the two `align options:` lines with:

```rust
            println!("align options: --split-spreads --ltr --stats --write FILE --title NAME");
            println!("               --name-a KEY --name-b KEY --direct");
            println!();
            println!("--write merges into an existing FILE rather than overwriting it. On a");
            println!("merge the a-side is the document's hub, so --name-a is optional;");
            println!("--direct records a spoke-to-spoke edge instead and requires it.");
```

- [ ] **Step 2: Replace the `--write` block**

In `align()`, replace everything from `if let Some(out) = option_value(args, "--write") {` to its closing brace (lines 570-650) with:

```rust
    if let Some(out) = option_value(args, "--write") {
        let direct = args.iter().any(|a| a == "--direct");
        // Read before writing. `--write` used to be a bare `fs::write`, which
        // silently discarded a document that may hold four other editions.
        let existing = match std::fs::read_to_string(out) {
            Ok(text) => Some(Sidecar::from_json(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("{out}: {e}")),
        };

        // On a merge the a-side is the edition the document already names, so
        // `--name-a` is optional and, given, must agree. `--direct` has no hub
        // to fall back on and so requires it.
        let a_name = match (&existing, option_value(args, "--name-a")) {
            (Some(_), None) if direct => {
                return Err(
                    "--direct needs --name-a to say which edition the edge starts from".into(),
                );
            }
            (Some(sidecar), None) => sidecar.hub.clone(),
            (Some(_), Some(given)) => given.to_string(),
            (None, given) => slug(&a_members, given),
        };
        let b_name = slug(&b_members, option_value(args, "--name-b"));
        if a_name == b_name {
            return Err(format!(
                "both editions would be named `{a_name}`; pass --name-a and --name-b"
            ));
        }

        let a_edition = describe_edition(&label(&a_members), &a_members, &a_slots)?;
        let b_edition = describe_edition(&label(&b_members), &b_members, &b_slots)?;
        let alignment = Alignment {
            from: a_name.clone(),
            to: b_name.clone(),
            by: Source::Solver,
            // The floor the solver actually used, not a placeholder: it is
            // calibrated from these two editions and a re-solve needs it to be
            // reproducible. Only a calibration the solver accepted, so a
            // refused pair records no solver block rather than a floor that was
            // never used.
            solver: taiyaku_core::calibrate(&a_slots_hashes, &b)
                .filter(taiyaku_core::Calibration::accepted)
                .map(|cal| SolverInfo {
                    hash: "dhash-16".to_string(),
                    floor: cal.floor,
                }),
            // Computed here rather than left for a reader to derive: the whole
            // point of the document is that a reader should not need the
            // archives. `Sidecar::quality_of` recomputes it anyway, so this is
            // a summary for anything not using this library.
            quality: Some(taiyaku_core::Quality {
                verdict: taiyaku_core::verdict(&runs, a_slots.len()),
                slots: a_slots.len() as u32,
                paired: taiyaku_core::paired_slots(&runs),
                avg_run: taiyaku_core::average_run_length(&runs),
            }),
            runs: {
                // The weakest per-page margin inside each run, which is what the
                // verification report shades on. Storing it means a reader can
                // show confidence without the archives.
                let scored = confidence(&a_slots_hashes, &b, &runs);
                let mut entries = entries_from_runs(&runs, &a_slots, &b_slots)?;
                for (entry, run) in entries.iter_mut().zip(&runs) {
                    if entry.b.is_none() {
                        continue;
                    }
                    entry.margin = scored
                        .iter()
                        .filter(|s| run.a.contains(s.page))
                        .map(|s| s.margin)
                        .min_by(f32::total_cmp);
                }
                entries
            },
        };

        let (sidecar, note) = match existing {
            Some(mut sidecar) => {
                if let Some(title) = option_value(args, "--title") {
                    if title != sidecar.work.title {
                        return Err(format!(
                            "{out} is the sidecar for `{}`; --title cannot rename a work \
                             on a merge",
                            sidecar.work.title
                        ));
                    }
                }
                let anchor = match direct {
                    true => Anchor::Direct,
                    false => Anchor::Hub,
                };
                let merged = sidecar.merge_spoke(
                    direction,
                    anchor,
                    &a_edition,
                    &b_name,
                    b_edition,
                    alignment,
                )?;
                let verb = match merged {
                    Merged::Added => "added",
                    Merged::Replaced => "replaced",
                };
                let note = format!(
                    "merged {out}: {verb} `{b_name}` | {} editions, {} alignments",
                    sidecar.editions.len(),
                    sidecar.alignments.len()
                );
                (sidecar, note)
            }
            None => (
                Sidecar {
                    taiyaku: taiyaku_core::VERSION,
                    work: Work {
                        title: option_value(args, "--title")
                            .unwrap_or("Untitled work")
                            .to_string(),
                        reading: direction,
                    },
                    editions: [(a_name.clone(), a_edition), (b_name.clone(), b_edition)]
                        .into_iter()
                        .collect(),
                    // A two-edition sidecar is a degenerate star: A is the
                    // reference simply because it is the only thing B is
                    // aligned to. A third edition makes it a real one.
                    hub: a_name.clone(),
                    alignments: vec![alignment],
                },
                format!("wrote {out}"),
            ),
        };
        std::fs::write(out, sidecar.to_json()).map_err(|e| format!("{out}: {e}"))?;
        eprintln!("{note}");
    }
    Ok(())
}
```

The `None` arm moves `alignment` into the `Sidecar`, and the `Some` arm moves it into `merge_spoke`, so no clone is needed — but the borrow checker needs `alignment` declared before the `match`, which it is.

- [ ] **Step 3: Update the imports**

Change `crates/taiyaku-cli/src/main.rs:21-24` to:

```rust
use taiyaku_core::{
    Alignment, Anchor, Direction, Edition, Half, Member, Merged, Run, Sidecar, Slot, SolverInfo,
    Source, Work, compose, confidence, counterpart, entries_from_runs, solve,
};
```

- [ ] **Step 4: Verify it builds and behaves**

```bash
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Then, with the corpus present, build a three-edition document by hand:

```bash
rm -f /tmp/five.json
target/debug/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-mono/*.cbz \
  --split-spreads --write /tmp/five.json --title "Go-Toubun no Hanayome" \
  --name-a raw-mono --name-b en-mono
target/debug/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/en-fan/*.cbz \
  --split-spreads --write /tmp/five.json --name-b en-fan
```

Expected, on the second command: `merged /tmp/five.json: added `en-fan` | 3 editions, 2 alignments`. Confirm with:

```bash
grep -c '"from"' /tmp/five.json   # expect 2
grep '"hub"' /tmp/five.json       # expect "hub": "raw-mono"
```

Then check the two refusals:

```bash
# a non-hub a-side
target/debug/taiyaku align corpus/gotoubun-v01/en-mono/*.cbz -- corpus/gotoubun-v01/en-color/*.cbz \
  --split-spreads --write /tmp/five.json --name-a en-mono --name-b en-color
```

Expected: `taiyaku: the hub of this document is `raw-mono`; align against it, or pass --direct ...`

```bash
# and the escape hatch, which must succeed
target/debug/taiyaku align corpus/gotoubun-v01/en-mono/*.cbz -- corpus/gotoubun-v01/en-color/*.cbz \
  --split-spreads --write /tmp/five.json --direct --name-a en-mono --name-b en-color
```

Expected: `merged /tmp/five.json: added `en-color` | 4 editions, 3 alignments`

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/main.rs
git commit -m "Add a spoke to a sidecar instead of replacing it"
```

---

### Task 7: `lookup` names the path it walked

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs:326-405` (the `lookup` function)

**Interfaces:**
- Consumes: `route`, `Route`, `Via` from Tasks 2–4.
- Produces: nothing other tasks depend on.

- [ ] **Step 1: Replace the trust line and the page loop**

In `lookup()`, replace everything from the comment block beginning `// Printed on every call rather than only on a bad one.` through the end of the `for page in &args[3..]` loop with:

```rust
    // Printed on every call rather than only on a bad one. A warning that
    // appears sometimes teaches people to skip it; a line that is always there
    // is read as part of the answer. Measured across twenty-two needles, seven
    // pair every page they have while being wholly unrelated, and the run shape
    // is the only thing that says so.
    //
    // The line describes the path this call will actually walk. `route` and
    // `trust` now agree about which paths exist, so the old "not by a path this
    // lookup can walk" case is gone — but a composed answer is weaker than a
    // direct one and the line has to say which it is.
    let path = taiyaku_core::route(&sidecar, from, to);
    match &path {
        Some(route) => match &route.via {
            Via::Identity => eprintln!("  alignment {from} -> {to}: identical edition"),
            via => {
                let verdict = match via {
                    // Composition is only as good as its weaker leg, which is
                    // what `trust` reports. The run length is of the composed
                    // runs, because those are what this call walks.
                    Via::Hub(_) => sidecar.trust(from, to),
                    _ => sidecar.quality_of(from, to).map(|q| q.verdict),
                };
                let how = match via {
                    Via::Inverted => format!(", from the stored {to} -> {from} edge"),
                    Via::Hub(hub) => format!(", composed through {hub}"),
                    _ => String::new(),
                };
                match verdict {
                    Some(v) => eprintln!(
                        "  alignment {from} -> {to}: {}, average run {:.1}{how}",
                        stats::verdict_word(v),
                        taiyaku_core::average_run_length(route.runs())
                    ),
                    None => eprintln!("  alignment {from} -> {to}: this document does not say{how}"),
                }
            }
        },
        // Nothing to say about the pairing — but if an edge is there and
        // unreadable, `route` swallowed the reason. Those messages are written
        // for whoever is hand-editing the file, and this is the only place they
        // can reach them.
        None => match sidecar
            .alignments
            .iter()
            .find(|a| (a.from == *from && a.to == *to) || (a.from == *to && a.to == *from))
            .and_then(|a| sidecar.runs_of(a).err())
        {
            Some(why) => eprintln!("  alignment {from} -> {to}: cannot be read: {why}"),
            None => eprintln!("  alignment {from} -> {to}: this document does not say"),
        },
    }

    for page in &args[3..] {
        let page: u32 = page.parse().map_err(|_| format!("not a page: {page}"))?;
        // Resolved through the route bound above rather than by calling
        // `resolve` per page, which would rebuild the composition every time.
        match path.as_ref().and_then(|route| route.resolve(page)) {
            Some(slot) => println!(
                "  {from} {page:>4} -> {to} {:>4}{}",
                slot.page,
                match slot.half {
                    Some(Half::Right) => " right half",
                    Some(Half::Left) => " left half",
                    None => "",
                }
            ),
            None => println!("  {from} {page:>4} -> no counterpart"),
        }
    }
    Ok(())
}
```

Add `Via` to the `use taiyaku_core::{...}` list at the top of the file.

- [ ] **Step 2: Verify against the document Task 6 built**

```bash
cargo build
target/debug/taiyaku lookup /tmp/five.json raw-mono en-mono 47
target/debug/taiyaku lookup /tmp/five.json en-mono raw-mono 45
target/debug/taiyaku lookup /tmp/five.json en-mono en-fan 47
target/debug/taiyaku lookup /tmp/five.json en-mono en-mono 47
```

Expected shapes, in order:

```
  alignment raw-mono -> en-mono: sound, average run 64.3
  alignment en-mono -> raw-mono: sound, average run 64.3, from the stored raw-mono -> en-mono edge
  alignment en-mono -> en-fan: <verdict>, average run <N.N>, composed through raw-mono
  alignment en-mono -> en-mono: identical edition
```

The third is the line that did not exist before this work: on the same document, before Task 4 it read `sound, but not by a path this lookup can walk` and every page below it said `no counterpart`. Record the actual figures — Task 10 puts them in the docs.

- [ ] **Step 3: Run the full check and commit**

```bash
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/taiyaku-cli/src/main.rs
git commit -m "Say which path a lookup walked, not just how far to trust it"
```

---

### Task 8: `topology` compares the composed verdict against the worse leg

This measures the question Task 4 deliberately left open: whether `Sidecar::trust` should keep taking `min` over the two legs, or judge the composed runs.

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs:212-295` (the `topology` function)

**Interfaces:**
- Consumes: `taiyaku_core::verdict`.
- Produces: a new output column, read by Task 10.

- [ ] **Step 1: Add the column**

In `topology()`, change the header `println!` to:

```rust
    println!(
        "{:<9} {:>6} {:>7} {:>8} {:>9} {:>9} {:>12}",
        "pair", "pages", "direct", "loss", "disagree", "inverse", "verdict"
    );
    let (mut total_pages, mut total_loss, mut total_disagree, mut total_inverse) = (0, 0, 0, 0);
    // Composition can merge two fragmented runs into one clean one, so judging
    // the composed runs is not the same as taking the worse leg. `trust` takes
    // the worse leg; this counts how often that is the more pessimistic answer.
    let mut verdict_splits = 0;
```

Inside the `for j` loop, after `let composed = compose(&to_hub[i], &from_hub[j]);`, add:

```rust
            let composed_verdict = taiyaku_core::verdict(&composed, spoke_hashes[i].len());
            let worse_leg = taiyaku_core::verdict(&to_hub[i], spoke_hashes[i].len())
                .min(taiyaku_core::verdict(&from_hub[j], hub_hashes.len()));
            if composed_verdict != worse_leg {
                verdict_splits += 1;
            }
```

Replace the row `println!` with:

```rust
            println!(
                "{i}->{j:<7} {pages:>6} {direct_paired:>7} {:>7.1}% {:>8.1}% {:>8.1}% {:>12}",
                100.0 * loss as f64 / pages as f64,
                100.0 * disagree as f64 / pages as f64,
                100.0 * inverse as f64 / pages as f64,
                match composed_verdict == worse_leg {
                    true => stats::verdict_word(composed_verdict).to_string(),
                    false => format!(
                        "{} != {}",
                        stats::verdict_word(composed_verdict),
                        stats::verdict_word(worse_leg)
                    ),
                }
            );
```

And after the existing closing summary, add:

```rust
    eprintln!(
        "composed verdict differs from the worse leg on {verdict_splits} of {} pairs: \
         at 0, `trust` may as well keep taking the worse leg",
        spokes.len() * (spokes.len() - 1)
    );
```

- [ ] **Step 2: Run it on both five-edition series**

```bash
cargo build --release
target/release/taiyaku topology \
  corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/gotoubun-v01/raw-color/*.cbz \
  -- corpus/gotoubun-v01/en-mono/*.cbz -- corpus/gotoubun-v01/en-color/*.cbz \
  -- corpus/gotoubun-v01/en-fan/*.cbz 2>&1 | tee /tmp/topology-gotoubun.txt
```

Expected: the baseline in `notes.md` reproduced — loss 1.90%, disagreement 0.00%, inverse 0.00% — plus the new column. **If loss or disagreement has moved, stop and say so**; the hasher or solver has changed and the baseline is what this command exists to protect.

Repeat for `corpus/kaguya-v01/` (raw-mono is a directory, the rest are `*.cbz`), expecting loss 1.56%, disagreement 0.16%, inverse 0.32%.

Note for Task 10: `topology` does **not** split spreads — it fingerprints pages directly. Its figures are page-level, so they are not directly comparable to anything measured through a sidecar, which always splits.

- [ ] **Step 3: Run the full check and commit**

```bash
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/taiyaku-cli/src/main.rs
git commit -m "Ask topology whether a composed verdict differs from its worse leg"
```

---

### Task 9: The standing corpus check

**Files:**
- Create: `crates/taiyaku-cli/src/corpus.rs`
- Modify: `crates/taiyaku-cli/src/main.rs:12-17` (module list)

**Interfaces:**
- Consumes: `crate::align`, `crate::cached_edition`, `taiyaku_core::{route, solve, counterpart}`.
- Produces: nothing other tasks depend on. Task 10 records its printed figures.

It lives in the binary crate rather than `tests/` because it needs `crate::align` and `crate::cached_edition`, which a binary's integration tests cannot reach.

- [ ] **Step 1: Declare the module**

In `crates/taiyaku-cli/src/main.rs`, add to the module list:

```rust
#[cfg(test)]
mod corpus;
```

- [ ] **Step 2: Write the test**

Create `crates/taiyaku-cli/src/corpus.rs`:

```rust
//! The standing check `notes.md` asks for under "Validate it against all-pairs,
//! and keep checking": that composing through the hub still agrees with a
//! direct solve.
//!
//! Gated on `TAIYAKU_CORPUS` and skipping when it is unset, because the
//! archives are 24 GB and are never committed. CI stays green without them; run
//! it locally whenever the hasher or the solver changes.
//!
//! ```sh
//! TAIYAKU_CORPUS=corpus cargo test -p taiyaku-cli corpus:: -- --nocapture
//! ```

use std::path::{Path, PathBuf};

use taiyaku_core::{Sidecar, Slot, counterpart, solve};

use crate::cached_edition;

/// Loss above which the star is not doing its job. `topology` measures 1.90%
/// for this series page-wise; this walks slots, through a sidecar, so it is a
/// different measurement of the same property and gets a ceiling rather than
/// the figure. Task 10 tightens this to what was actually measured.
const MAX_LOSS: f64 = 0.03;

/// The corpus root, or `None` when this machine does not have it.
fn corpus() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var("TAIYAKU_CORPUS").ok()?);
    root.is_dir().then_some(root)
}

/// The members of one edition directory: its `.cbz` files in name order, or the
/// directory itself when it holds loose page images.
fn members(dir: &Path) -> Vec<String> {
    let mut archives: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()?.eq_ignore_ascii_case("cbz"))
                .then(|| path.to_string_lossy().to_string())
        })
        .collect();
    archives.sort();
    match archives.is_empty() {
        true => vec![dir.to_string_lossy().to_string()],
        false => archives,
    }
}

/// Compose the whole five-edition document through the merge path, then check
/// every spoke-to-spoke lookup against a direct solve.
///
/// Disagreement is the number that decides it — a reader landing on a different
/// page depending on which edition it swapped through was the theoretical worry
/// hub-and-spoke carried, and it must be zero. Loss is expected and safe: the
/// reader sees "no counterpart" rather than the wrong page.
#[test]
fn composing_through_the_hub_agrees_with_a_direct_solve() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    let volume = root.join("gotoubun-v01");
    let hub = "raw-mono";
    let spokes = ["raw-color", "en-mono", "en-color", "en-fan"];

    let out = std::env::temp_dir().join("taiyaku-corpus-star.json");
    let _ = std::fs::remove_file(&out);
    for spoke in spokes {
        let mut args = members(&volume.join(hub));
        args.push("--".into());
        args.extend(members(&volume.join(spoke)));
        args.extend([
            "--split-spreads".into(),
            "--write".into(),
            out.to_string_lossy().to_string(),
            "--title".into(),
            "Go-Toubun no Hanayome".into(),
            "--name-a".into(),
            hub.into(),
            "--name-b".into(),
            spoke.into(),
        ]);
        crate::align(&args).unwrap_or_else(|e| panic!("aligning {spoke}: {e}"));
    }

    let sidecar = Sidecar::from_json(&std::fs::read_to_string(&out).expect("written"))
        .expect("the document we just wrote parses");
    assert_eq!(sidecar.editions.len(), 5, "five editions, one hub, four edges");
    assert_eq!(sidecar.alignments.len(), 4);
    assert_eq!(sidecar.hub, hub, "merging must never move the hub");

    let direction = sidecar.work.reading;
    let cache = PathBuf::from(".taiyaku-cache");
    let mut slots = std::collections::BTreeMap::new();
    for edition in spokes {
        let paths: Vec<PathBuf> = members(&volume.join(edition))
            .iter()
            .map(PathBuf::from)
            .collect();
        let (hashes, table, _) =
            cached_edition(&cache, &paths, direction).expect("fingerprints");
        slots.insert(edition, (hashes, table));
    }

    let (mut pages, mut loss, mut disagree) = (0u32, 0u32, 0u32);
    for from in spokes {
        for to in spokes {
            if from == to {
                continue;
            }
            let (from_hashes, from_slots) = &slots[from];
            let (to_hashes, to_slots) = &slots[to];
            let direct = solve(from_hashes, to_hashes);
            let route = taiyaku_core::route(&sidecar, from, to)
                .unwrap_or_else(|| panic!("no route from {from} to {to}"));

            // Both sides are asked the same question: where does the FIRST slot
            // of this page land? A spread's second half is not addressable by
            // page number, so comparing per slot would count a half no caller
            // can ask for.
            for (index, slot) in from_slots.iter().enumerate() {
                if slot.half == Some(taiyaku_core::Half::Left) {
                    continue;
                }
                pages += 1;
                let expected = counterpart(&direct, index as u32)
                    .and_then(|i| to_slots.get(i as usize))
                    .copied();
                let got: Option<Slot> = route.resolve(slot.page);
                match (expected, got) {
                    (Some(_), None) => loss += 1,
                    (Some(x), Some(y)) if x != y => {
                        disagree += 1;
                        if disagree <= 5 {
                            eprintln!(
                                "  {from} p{} -> direct {x:?} but composed {y:?}",
                                slot.page
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let loss_rate = loss as f64 / pages as f64;
    eprintln!(
        "{pages} page comparisons | loss {:.2}% | disagreement {:.2}%",
        100.0 * loss_rate,
        100.0 * disagree as f64 / pages as f64
    );
    assert_eq!(disagree, 0, "composed and direct must not send a page to two different places");
    assert!(loss_rate <= MAX_LOSS, "loss {loss_rate:.4} exceeds {MAX_LOSS}");
}
```

- [ ] **Step 3: Run it, both ways**

```bash
cargo test -p taiyaku-cli corpus:: -- --nocapture
```

Expected: `TAIYAKU_CORPUS unset; skipping`, test PASSES. This is the CI path.

```bash
TAIYAKU_CORPUS=corpus cargo test -p taiyaku-cli corpus:: -- --nocapture
```

Expected: PASS, with a printed line reporting the page count, loss and disagreement. **Record that line** — Task 10 puts it in `notes.md` and tightens `MAX_LOSS`.

If disagreement is not 0, do not weaken the assertion. Per `notes.md`'s own decision rule, disagreement implicates the solver rather than the topology, and the first five disagreeing pages are printed to start the investigation.

- [ ] **Step 4: Run the full check and commit**

```bash
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add crates/taiyaku-cli/src/corpus.rs crates/taiyaku-cli/src/main.rs
git commit -m "Keep checking the star, when the archives are there to check it with"
```

---

### Task 10: Say what is now true

Three documents describe the unbuilt state, and two measurements from Tasks 8 and 9 have no home yet.

**Files:**
- Modify: `docs/sidecar.md:15-53` (the `lookup` example), `:224-249` (the hub section)
- Modify: `docs/notes.md:184-224` ("Result: the star holds")
- Modify: `docs/status.md:34-51` (commands), `:102-140` (known missing), `:141-197` (what comes next)
- Modify: `docs/superpowers/specs/2026-07-30-hub-and-spoke-design.md` (§3's placeholder figures)

**Interfaces:** none — documentation only.

- [ ] **Step 1: Rewrite the hub paragraph in `docs/sidecar.md`**

Replace the paragraph beginning `**Resolving through the hub is not implemented.**` (lines 233-241) with a statement of what `resolve` now does: identity, a stored edge in either direction, then composition through the hub, with a stored edge preferred over composition **including when it is present and unreadable**. Keep the following paragraph about direct edges — it is still correct, and now describes a flag rather than an intention.

State the measurement that permits reading an edge backwards: the solver is symmetric on the pairing relation across five pairs in two series, so inverting a stored edge costs nothing a reverse solve would have avoided.

- [ ] **Step 2: Update the `lookup` example in `docs/sidecar.md`**

The example at lines 42-47 shows a two-edition document. Add the composed case beneath it, using the **actual output** recorded in Task 7 Step 2 — not invented figures. `sidecar.md` claims its examples are byte-for-byte what the tool writes, and that claim has to keep holding.

Also update the `Producing and consuming one` block at lines 17-22 to show a third edition being merged in.

- [ ] **Step 3: Extend `docs/notes.md` §"Result: the star holds"**

Add a subsection recording, in the house style — measurement first, conclusion second:

1. **The solver is symmetric on the pairing relation.** The five-pair table from the design spec. Why it matters: a sidecar stores one edge per spoke and must invert it, so had inversion been lossy the star would need 2(N−1) stored alignments. State that `calibrate` is asymmetric on paper and does not reach the DP's output on any material measured.
2. **The shipped path, measured.** The page count, loss and disagreement printed by Task 9. Note explicitly that `topology` does not split spreads and its figures are page-level, so the two numbers measure the same property differently and are not expected to be equal.
3. **Composed verdict against the worse leg.** The `verdict_splits` count from Task 8 for both series, and what it decides: at 0, `trust` keeps taking the worse leg and the question closes.

- [ ] **Step 4: Update `docs/status.md`**

- The commands block (lines 36-41) gains `--direct` and a note that `--write` merges.
- Under "What is settled", the hub-and-spoke bullet gains "and resolvable — a lookup composes through the hub".
- Under "What is known to be wrong or missing", delete **"No reader. Nothing consumes a sidecar except `lookup`."** only if it is now false — it is not; `lookup` is still the only consumer. Leave it. Do delete any claim that resolution through the hub is unimplemented.
- Under "What comes next", item 3 (the reader) is unchanged; note that its blocker is cleared.
- "Two crates, 47 tests" (line 21) is already wrong before this work — the count is 57. Correct it to whatever `cargo test --workspace` reports after Task 9.

- [ ] **Step 5: Fill in the spec's placeholder figures**

In `docs/superpowers/specs/2026-07-30-hub-and-spoke-design.md`, §3 shows `average run NN.N` with a note saying the figures are a shape rather than a measurement. Replace them with the real output from Task 7 Step 2 and delete that note.

- [ ] **Step 6: Tighten `MAX_LOSS`**

In `crates/taiyaku-cli/src/corpus.rs`, replace the `0.03` ceiling with the figure Task 9 measured, rounded up to the next tenth of a percent, and rewrite the doc comment to say it is measured rather than a ceiling.

Re-run to confirm it still passes:

```bash
TAIYAKU_CORPUS=corpus cargo test -p taiyaku-cli corpus:: -- --nocapture
```

- [ ] **Step 7: Commit**

```bash
git add docs crates/taiyaku-cli/src/corpus.rs
git commit -m "Say that the hub is walkable, and what it cost to walk it"
```

---

## Self-Review

**Spec coverage.** Every section of the design spec maps to a task:

| Spec section | Task |
| ------------ | ---- |
| §1 `Route`, `Via`, `route()`, `invert`, search order | 1, 2, 3, 4 |
| §2 `merge_spoke`, rules 1–5, `--direct`, `Merged` | 5, 6 |
| §3 `lookup` names the path | 7 |
| §4 `trust` keeps `min`, corpus reports disagreement | 8 |
| §5 unit tests | 1, 3, 4, 5 |
| §5 merge tests | 5 |
| §5 corpus test gated on `TAIYAKU_CORPUS` | 9 |
| §5 `topology` re-run + new column | 8 |
| §5 documentation | 10 |
| Behaviour changes (`resolve` B→A, spoke-to-spoke, `--write` merges) | 3, 4, 6 |
| Known limits (composed runs carry no margin; sha check noise) | Global Constraints; Task 5 |

**Type consistency.** `Route::runs()` returns `&[Run]` and is called in Tasks 7 and 9. `Via::Hub(String)` is constructed in Task 4 and matched in Tasks 5, 7 and 9. `merge_spoke`'s six parameters are in the same order in Task 5's definition, Task 5's tests, Task 6's call and Task 9's indirect use through `align`. `Anchor` and `Merged` are exported in Task 5 and imported in Task 6.

**One deviation from strict TDD, stated rather than hidden.** Tasks 1 and 3 write the implementation before the test — Task 1 because `invert` is a leaf with nothing to stub, and Task 3 because Task 2's refactor necessarily carries `Via::Inverted` and `Via::Identity` with it. Both tasks include a step that mutates the implementation to prove the tests bite, which is what the failing-first step is for.

**Known gap, deliberate.** `MAX_LOSS` in Task 9 starts as a ceiling rather than a measurement, because `topology`'s 1.90% is page-level and the corpus test walks slots through a sidecar. Task 10 Step 6 replaces it with what was measured. Anything that ships with a ceiling in place of a figure would be out of step with this codebase.

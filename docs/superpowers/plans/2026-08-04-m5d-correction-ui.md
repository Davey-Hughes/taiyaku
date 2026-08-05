# M5d — the correction UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A reader who sees a wrong pairing can point at the right counterpart in
the filmstrip and have that correction written into `taiyaku.json` as
`by: "hand"`, survive a re-solve, and survive a second device writing at the
same time.

**Architecture:** The arithmetic is one new pure function in `taiyaku-core`
(`correct_runs`) and one document-level operation built on it
(`Sidecar::correct`), so no second implementation of run splitting exists to
disagree with `compose`. `taiyaku-serve` gains its first mutating route —
`PATCH /api/works/{work}/alignments`, guarded by `If-Match` against an `ETag`
on the sidecar — which forces the in-memory document to become replaceable
(`RwLock<Arc<Sidecar>>`) rather than loaded once at startup. The browser sends
the *assertion* ("this slot pairs with that one"), never a computed run list,
so the split is applied once, server-side, against the authoritative document.
O13 is closed in the same branch because the correction UI is what first
constructs a `Source::Hand`.

**Tech Stack:** Rust (`taiyaku-core`, `taiyaku-serve`, axum 0.8, sha2 0.11,
serde), TypeScript/React 19 (`web/`), Vitest. **No `taiyaku-wasm` change is
needed** — see Task 7's note on previewing a correction through the existing
`Resolver.resolve`.

## Global Constraints

- `taiyaku-core` must keep compiling for `wasm32-unknown-unknown`. No `wasm-bindgen`,
  no I/O, no `std::time` in it.
- Sites are named by symbol in `docs/`, never by line number.
- Every new refusal names the thing that was missing, in this server's own words.
  A bare status or a Rust type name in a message is a defect (O5).
- Runs are addressed in **slots**, never pages. A joined spread is one page and
  two slots.
- No new third-party dependency. `sha2` is already a `taiyaku-serve` dependency;
  the in-repo idiom is `use sha2::{Digest, Sha256}` with `new`/`update`/`finalize`
  (`stump/resolve.rs`).
- Commit per task. Keep behaviour changes off the docs commit.

---

## What the prototype measured

This plan's arithmetic was written into the tree, run against the real fixtures,
and reverted before the plan was written. Four things that a plan asserting
rather than running would have got wrong, all reproduced by
`crates/taiyaku-core/src/sidecar.rs`'s `REAL` (Go-Toubun v01: `raw-mono` 195
slots, `en-mono` 194 slots) and the three-edition star fixture:

1. **A point correction breaks the 1:1 invariant unless the previous owner of
   the target gives it up.** Correcting `raw` slot 100 from `en` slot 99 to `en`
   slot 98 left `en` slot 98 claimed by *both* `raw` 99 and `raw` 100 — a run
   list `invert` would turn into two runs starting at the same slot, and
   `counterpart` would answer from whichever came first, silently. Measured: 193
   targets, 192 distinct. With eviction: 192 targets, 192 distinct.
2. **Marking hand runs on "any overlapping slot" marks nearly the document.**
   One correction inside `raw-mono`'s 188-slot final run marked all 188 slots
   `by: "hand"`, which would make Task 3's refusal fire forever after. The rule
   has to be *every* slot of the run, which — because a correction that changes
   the answer always lands as its own single-slot run — marks exactly the slot
   asserted and the slot displaced. Measured with two corrections: hand runs
   `["42", "50"]`, not `["7-41", "42", "43-49", "50", "51-194"]`.
3. **A correction that changes nothing must not rewrite anything.** `raw` 100
   already resolves to `en` page 94 in `REAL`; without a no-op check the
   "correction" still stamped `by: "hand"` and a `note` onto a 188-slot solver
   run.
4. **Two corrections flip Go-Toubun v01 from `sound` to `fragmented`.**
   Measured, in order: `avg_run` 64.33 (`Sound`) → 32.00 (`Sound`) → 21.22
   (`Fragmented`), against the threshold of 27.0. Each correction adds two
   paired runs and strands one slot. This is not fixed here — Task 9 opens it as
   O23 with these figures, because changing the fragmentation policy on a guess
   is exactly what this project does not do.

Also confirmed by running: correcting in the direction an edge is *not* stored
in must be applied in the stored direction rather than inverted-corrected-
inverted, because `invert` drops gaps by construction. Measured: the `en → raw`
correction left the stored edge `raw-mono → en-mono` in place with its gap runs
intact (2 gaps → 3, the third being the displaced slot).

And the five claims the design told this plan to check are checked:
`Source::Hand` is constructed nowhere (`sidecar.rs:130` is the only mention);
`re_aligning_a_spoke_replaces_it_rather_than_duplicating` does pin a
sha256-changed spoke merge as succeeding; `merge_spoke`'s guard compares
`stored.members` against `from_side.members` — the *from* side only, with the
spoke inserted wholesale; `progress::store` writes temp-then-rename with a
per-attempt unique name and `cover::unique_temp_path` is the same shape; and
`Route` holds runs in slot indices with `resolve_slot` the direct operation.

## Deviation from the spec, stated rather than smuggled

`2026-08-02-reader-m5-design.md` says "the client computes a run split, `PATCH`
to serve". **This plan sends the assertion instead and splits server-side.** The
run-splitting arithmetic still lives in `taiyaku-core` exactly as the design
requires, and is still what any other reader would call — it is simply called
once, on the authoritative document, rather than on a copy the client parsed
some time earlier. A client-computed run list would make `If-Match` the only
thing standing between a stale client and a wholesale replacement of the edge,
and would put the 1:1 eviction rule (finding 1 above) in the client where a
second reader implementation could get it wrong. The client still previews the
consequence before committing — see Task 7 — using a resolution it can already
make.

## File structure

| File | Responsibility | Task |
| ---- | -------------- | ---- |
| `crates/taiyaku-core/src/lib.rs` | `correct_runs` — the pure run-splitting arithmetic, beside `compose` | 1 |
| `crates/taiyaku-core/src/sidecar.rs` | `Sidecar::correct`, `Corrected`, `NotCorrected`; `merge_spoke`'s hand-run rules | 2, 3 |
| `crates/taiyaku-serve/src/works.rs` | `Work` holds a replaceable sidecar | 4 |
| `crates/taiyaku-serve/src/atomic.rs` | **new** — one temp-then-rename writer, shared by three callers | 5 |
| `crates/taiyaku-serve/src/routes.rs` | `sidecar_etag`, `If-Match`, `patch_alignments` | 5, 6 |
| `web/src/api.ts` | `getSidecarWithEtag`, `patchAlignment` | 7 |
| `web/src/correction.ts` | **new** — pure: what a pending correction is and what it displaces | 7 |
| `web/src/Filmstrip.tsx` | correction mode: pick a counterpart tile | 8 |
| `web/src/App.tsx` | correction state, commit, refresh | 8 |
| `docs/status.md`, `docs/notes.md`, `docs/sidecar.md` | close O13, open O23, record the measurements | 9 |

---

### Task 1: `correct_runs` — the arithmetic

**Files:**
- Modify: `crates/taiyaku-core/src/lib.rs` (add after `compose`, before `mod tests`)
- Test: `crates/taiyaku-core/src/lib.rs` (its own `mod tests`)

**Interfaces:**
- Consumes: `Run`, `PageRange`, `counterpart`, `solve::compress` — all already in this file.
- Produces: `pub fn correct_runs(runs: &[Run], a: u32, b: Option<u32>) -> Vec<Run>`,
  re-exported from the crate root by being defined there. Task 2 is its only caller.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-core/src/lib.rs`:

```rust
    /// The ordinary correction: one slot in the middle of a long run is
    /// asserted to pair with a different slot, so the run splits around it.
    #[test]
    fn correcting_a_slot_inside_a_run_splits_it() {
        let runs = vec![Run::paired(PageRange::new(0, 9), PageRange::new(0, 9))];

        let fixed = correct_runs(&runs, 5, Some(7));

        assert_eq!(
            fixed,
            vec![
                Run::paired(PageRange::new(0, 4), PageRange::new(0, 4)),
                Run::paired(PageRange::new(5, 5), PageRange::new(7, 7)),
                // Slot 6 still pairs with 6 — it just cannot stay inside
                // either neighbour's run any more, because the offset either
                // side of it changed.
                Run::paired(PageRange::new(6, 6), PageRange::new(6, 6)),
                Run::gap(PageRange::new(7, 7)),
                Run::paired(PageRange::new(8, 9), PageRange::new(8, 9)),
            ],
            "slot 7 is the one that gave up its target; slot 6 keeps its own pairing"
        );
    }

    /// The invariant that makes the rest of the format work. Asserting that `a`
    /// pairs with `b` is also asserting that nothing else does — leave the old
    /// owner in place and two source slots claim one target, which `invert`
    /// turns into two runs starting at the same slot and `counterpart` answers
    /// from whichever comes first, silently.
    #[test]
    fn a_corrected_target_is_taken_from_whoever_held_it() {
        let runs = vec![Run::paired(PageRange::new(0, 9), PageRange::new(0, 9))];

        let fixed = correct_runs(&runs, 5, Some(7));

        let mut targets: Vec<u32> = fixed
            .iter()
            .filter_map(|r| r.b)
            .flat_map(|b| b.start..=b.end)
            .collect();
        let claimed = targets.len();
        targets.sort_unstable();
        targets.dedup();
        assert_eq!(claimed, targets.len(), "no target slot may be claimed twice");
        assert_eq!(counterpart(&fixed, 7), None, "slot 7 gave up slot 7");
        assert_eq!(counterpart(&fixed, 5), Some(7));
    }

    /// A correction that agrees with its neighbour merges rather than
    /// splitting, because `compress` is the only thing deciding run shape.
    /// Measured on the real fixture: pairing `raw-mono`'s gap at slot 6 with
    /// `en-mono` slot 5 takes the stored five runs down to three.
    #[test]
    fn a_correction_that_agrees_with_its_neighbour_merges() {
        let runs = vec![
            Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
            Run::gap(PageRange::new(1, 1)),
            Run::paired(PageRange::new(2, 5), PageRange::new(1, 4)),
            Run::gap(PageRange::new(6, 6)),
            Run::paired(PageRange::new(7, 194), PageRange::new(6, 193)),
        ];

        let fixed = correct_runs(&runs, 6, Some(5));

        assert_eq!(
            fixed,
            vec![
                Run::paired(PageRange::new(0, 0), PageRange::new(0, 0)),
                Run::gap(PageRange::new(1, 1)),
                Run::paired(PageRange::new(2, 194), PageRange::new(1, 193)),
            ]
        );
    }

    /// Unpairing is a correction too, and it displaces nobody.
    #[test]
    fn correcting_a_slot_to_nothing_leaves_a_gap() {
        let runs = vec![Run::paired(PageRange::new(0, 3), PageRange::new(0, 3))];

        let fixed = correct_runs(&runs, 2, None);

        assert_eq!(
            fixed,
            vec![
                Run::paired(PageRange::new(0, 1), PageRange::new(0, 1)),
                Run::gap(PageRange::new(2, 2)),
                Run::paired(PageRange::new(3, 3), PageRange::new(3, 3)),
            ]
        );
    }

    /// A run list need not cover its edition — the star fixture's `a -> c`
    /// starts at slot 2 — and inventing gap runs for the head would put slots
    /// into the document the alignment never claimed anything about.
    #[test]
    fn slots_the_runs_never_mention_stay_unmentioned() {
        let runs = vec![Run::paired(PageRange::new(2, 5), PageRange::new(0, 3))];

        let fixed = correct_runs(&runs, 3, Some(9));

        assert_eq!(fixed[0].a, PageRange::new(2, 2), "nothing before slot 2");
        assert_eq!(counterpart(&fixed, 0), None);
        assert_eq!(counterpart(&fixed, 1), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core correct -- --nocapture`
Expected: FAIL — `cannot find function 'correct_runs' in this scope` (5 errors).

- [ ] **Step 3: Write the implementation**

Add to `crates/taiyaku-core/src/lib.rs`, immediately after `compose`:

```rust
/// Assert by hand that slot `a` of the source edition pairs with slot `b` of
/// the target — or, where `b` is `None`, with nothing.
///
/// The whole run list is rebuilt through the same `compress` that `compose`
/// and the solver itself end on, so a correction cannot produce a run shape
/// the solver could not have produced. That is the point: the correction
/// changes one slot's answer, and nothing else decides what the runs look like
/// afterwards. A correction that disagrees with its neighbours splits a run;
/// one that agrees with them merges two.
///
/// **Whatever used to hold `b` gives it up.** The alignment is strictly 1:1 in
/// slots — that is what expanding spreads before solving buys — so asserting
/// that `a` pairs with `b` is also asserting that nothing else does. Leaving
/// the old owner in place produces a run list where two source slots claim one
/// target, which `invert` turns into two runs starting at the same slot and
/// `counterpart` then answers from whichever comes first, silently. The
/// displaced slot becomes a gap rather than being guessed at: the document
/// knows it no longer pairs with `b` and knows nothing about what it does pair
/// with.
///
/// Slots the run list never mentions stay unmentioned. A run list need not
/// cover its edition, and inventing gap runs for the head of one would put
/// slots into the document that the alignment never claimed anything about.
pub fn correct_runs(runs: &[Run], a: u32, b: Option<u32>) -> Vec<Run> {
    let end = runs.iter().map(|r| r.a.end).max().unwrap_or(a).max(a);
    let mentioned = |slot: u32| runs.iter().any(|r| r.a.contains(slot));
    let displaced = b.and_then(|target| {
        (0..=end)
            .find(|&slot| slot != a && mentioned(slot) && counterpart(runs, slot) == Some(target))
    });
    let pairs: Vec<(u32, Option<u32>)> = (0..=end)
        .filter(|&slot| slot == a || mentioned(slot))
        .map(|slot| match (slot == a, Some(slot) == displaced) {
            (true, _) => (slot, b),
            (_, true) => (slot, None),
            _ => (slot, counterpart(runs, slot)),
        })
        .collect();
    solve::compress(&pairs)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core 2>&1 | grep -E "^test result"`
Expected: `test result: ok. 75 passed` — the 70 that exist today plus these five.

- [ ] **Step 5: Sabotage check**

Delete the `displaced` binding's `find` body (make it `None`), re-run.
Expected: `a_corrected_target_is_taken_from_whoever_held_it` FAILS on
`no target slot may be claimed twice`, and `correcting_a_slot_inside_a_run_splits_it`
FAILS. Restore it.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-core/src/lib.rs
git commit -m "core: correct_runs, the arithmetic a hand correction applies"
```

---

### Task 2: `Sidecar::correct` — applying one to a document

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs`
- Modify: `crates/taiyaku-core/src/lib.rs` (the `pub use sidecar::{...}` list)
- Test: `crates/taiyaku-core/src/sidecar.rs` (`mod tests`)

**Interfaces:**
- Consumes: `correct_runs` (Task 1), `Sidecar::runs_of`, `Sidecar::edge_between`,
  `entries_from_runs`, `slot_index`, `show_slot`, `route`, `counterpart`,
  `verdict`, `paired_slots`, `average_run_length`.
- Produces:
  ```rust
  pub enum Corrected { Edge, DirectEdgeAdded, AlreadyTrue }
  pub enum NotCorrected {
      NoSuchEdition(String),
      NoSuchSlot(String),
      SameEdition(String),
      NoPath(String),
      Unreadable(String),
  }
  impl Sidecar {
      pub fn correct(
          &mut self,
          from: &str,
          to: &str,
          a: Slot,
          b: Option<Slot>,
          note: Option<String>,
      ) -> Result<Corrected, NotCorrected>;
  }
  ```
  Task 6 is the only caller and maps each `NotCorrected` variant to a status.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-core/src/sidecar.rs`. The `STAR` fixture
below is local to these tests; `route.rs`'s own copy is private to that module.

```rust
    const STAR_FOR_CORRECTION: &str = r#"{
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

    fn at(page: u32) -> Slot {
        Slot { page, half: None }
    }

    /// The ordinary case, on the real document: the edge is stored
    /// `raw-mono -> en-mono` and the correction is asked for in that
    /// direction. Slot 50 of the raw currently resolves to English page 49;
    /// asserting page 40 instead must land, and must displace whoever held it.
    #[test]
    fn correcting_a_stored_edge_rewrites_it_in_place() {
        let mut s = Sidecar::from_json(REAL).expect("parses");

        let what = s
            .correct("raw-mono", "en-mono", at(50), Some(at(40)), None)
            .expect("corrects");

        assert_eq!(what, Corrected::Edge);
        assert_eq!(s.alignments.len(), 1, "no new edge for a pair that has one");
        assert_eq!(
            crate::resolve(&s, "raw-mono", "en-mono", 50),
            Some(at(40)),
            "the assertion is what the document now says"
        );
        let hands: Vec<String> = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .map(|e| e.a.to_string())
            .collect();
        assert_eq!(
            hands,
            vec!["42".to_string(), "50".to_string()],
            "exactly the slot asserted and the slot displaced — not the run they sat in"
        );
    }

    /// The edge is stored one way and the reader is looking the other way.
    /// Correcting through `invert` and back would silently drop every gap the
    /// stored direction records, because `invert` drops gaps by construction.
    #[test]
    fn correcting_against_the_stored_direction_keeps_the_stored_gaps() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        let gaps_before = s.alignments[0].runs.iter().filter(|e| e.b.is_none()).count();

        let what = s
            .correct("en-mono", "raw-mono", at(10), Some(at(20)), None)
            .expect("corrects");

        assert_eq!(what, Corrected::Edge);
        assert_eq!(s.alignments.len(), 1);
        assert_eq!(s.alignments[0].from, "raw-mono", "the edge keeps its orientation");
        assert_eq!(
            s.alignments[0].runs.iter().filter(|e| e.b.is_none()).count(),
            gaps_before + 1,
            "the stored gaps survive, plus one for the displaced slot"
        );
        assert_eq!(crate::resolve(&s, "en-mono", "raw-mono", 10), Some(at(20)));
    }

    /// A pair with no stored edge resolves through the hub, so there is
    /// nothing to edit. The correction writes a direct edge seeded from the
    /// composition with the fix applied — which `sidecar.md` already sanctions
    /// for a pair where composition demonstrably loses, and a hand correction
    /// is that demonstration.
    #[test]
    fn correcting_a_composed_pair_writes_a_direct_edge() {
        let mut s = Sidecar::from_json(STAR_FOR_CORRECTION).expect("parses");
        assert_eq!(route(&s, "b", "c").map(|r| r.via), Some(Via::Hub("a".into())));

        let what = s.correct("b", "c", at(0), Some(at(0)), None).expect("corrects");

        assert_eq!(what, Corrected::DirectEdgeAdded);
        assert_eq!(s.alignments.len(), 3);
        let added = s.alignments.last().expect("just pushed");
        assert_eq!((added.from.as_str(), added.to.as_str()), ("b", "c"));
        assert_eq!(added.by, Source::Hand);
        assert_eq!(added.solver, None, "no floor was calibrated for this edge");
        assert_eq!(route(&s, "b", "c").map(|r| r.via), Some(Via::Direct));
        assert_eq!(
            crate::resolve(&s, "b", "c", 0),
            Some(at(0)),
            "the composed gap at the head is what the reader was correcting"
        );
    }

    /// A correction the document already agrees with must change nothing at
    /// all — not the runs, not `by`, not `note`. Without this, "correcting" a
    /// slot to where it already resolves stamps `hand` across the whole run it
    /// sits in, which on the real fixture is 188 slots.
    #[test]
    fn a_correction_that_changes_nothing_rewrites_nothing() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        let before = s.clone();

        let what = s
            .correct("raw-mono", "en-mono", at(100), Some(at(94)), Some("noted".into()))
            .expect("is accepted");

        assert_eq!(what, Corrected::AlreadyTrue);
        assert_eq!(s, before, "an already-true assertion is not an edit");
    }

    /// The quality block is recomputed from the runs that are now there, not
    /// carried over. Measured on this fixture: two corrections take the
    /// average run length from 64.33 to 21.22, past the 27.0 threshold — see
    /// O23 in `docs/status.md`, which records that rather than hiding it.
    #[test]
    fn correcting_recomputes_the_quality_beside_the_runs() {
        let mut s = Sidecar::from_json(REAL).expect("parses");

        s.correct("raw-mono", "en-mono", at(50), Some(at(40)), None).unwrap();
        let after_one = s.alignments[0].quality.expect("recomputed");
        s.correct("raw-mono", "en-mono", at(150), Some(at(140)), None).unwrap();
        let after_two = s.alignments[0].quality.expect("recomputed");

        assert_eq!(after_one.verdict, Verdict::Sound);
        assert!((after_one.avg_run - 32.0).abs() < 0.01, "{after_one:?}");
        assert_eq!(
            after_two.verdict,
            Verdict::Fragmented,
            "two corrections is where this volume crosses the threshold"
        );
        assert!((after_two.avg_run - 21.22).abs() < 0.01, "{after_two:?}");
    }

    /// A hand correction has to survive the file being written and read back,
    /// or nothing else in this milestone means anything.
    #[test]
    fn a_correction_round_trips_through_the_document() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        s.correct("raw-mono", "en-mono", at(50), Some(at(40)), Some("checked".into()))
            .unwrap();

        let reparsed = Sidecar::from_json(&s.to_json()).expect("round-trips");

        assert_eq!(reparsed, s);
        let noted = reparsed.alignments[0]
            .runs
            .iter()
            .find(|e| e.note.is_some())
            .expect("the note survives");
        assert_eq!(noted.note.as_deref(), Some("checked"));
        assert_eq!(noted.by, Some(Source::Hand));
    }

    /// A second correction must not rewrite the first one's explanation.
    /// `note` is why *that* slot was changed, and the run list is rebuilt
    /// wholesale on every correction, so carrying it is deliberate work
    /// rather than something that happens by itself.
    #[test]
    fn a_later_correction_leaves_an_earlier_ones_note_alone() {
        let mut s = Sidecar::from_json(REAL).expect("parses");
        s.correct("raw-mono", "en-mono", at(50), Some(at(40)), Some("first".into()))
            .unwrap();

        s.correct("raw-mono", "en-mono", at(150), Some(at(140)), Some("second".into()))
            .unwrap();

        let notes: Vec<Option<&str>> = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .map(|e| e.note.as_deref())
            .collect();
        assert!(notes.contains(&Some("first")), "{notes:?}");
        assert!(notes.contains(&Some("second")), "{notes:?}");
    }

    /// Each refusal names what was missing, and none of them is a panic.
    #[test]
    fn a_correction_naming_something_absent_is_refused_by_name() {
        let mut s = Sidecar::from_json(REAL).expect("parses");

        let no_edition = s.correct("raw-mono", "nosuch", at(0), Some(at(0)), None);
        let no_slot = s.correct("raw-mono", "en-mono", at(9999), Some(at(0)), None);
        let itself = s.correct("raw-mono", "raw-mono", at(0), Some(at(0)), None);

        assert!(
            matches!(no_edition, Err(NotCorrected::NoSuchEdition(ref m)) if m.contains("`nosuch`")),
            "{no_edition:?}"
        );
        assert!(
            matches!(no_slot, Err(NotCorrected::NoSuchSlot(ref m)) if m.contains("9999")),
            "{no_slot:?}"
        );
        assert!(matches!(itself, Err(NotCorrected::SameEdition(_))), "{itself:?}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core correct`
Expected: FAIL — `no method named 'correct' found for struct 'Sidecar'`, plus
`cannot find type 'Corrected'` / `'NotCorrected'`.

- [ ] **Step 3: Write the implementation**

Add the two enums to `crates/taiyaku-core/src/sidecar.rs`, next to `Merged`:

```rust
/// What a correction did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corrected {
    /// A stored edge was rewritten in place.
    Edge,
    /// The pair resolved through the hub, so a direct edge was written for it.
    DirectEdgeAdded,
    /// The document already said this. Nothing changed.
    AlreadyTrue,
}

/// Why a correction was refused.
///
/// A typed refusal rather than a `String` because the only caller is an HTTP
/// route that has to answer a different status for each of these, and
/// string-matching a message to pick a status is the coupling that breaks the
/// first time someone improves the wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotCorrected {
    NoSuchEdition(String),
    NoSuchSlot(String),
    SameEdition(String),
    NoPath(String),
    /// A stored edge this document cannot read — a hand edit that made a span
    /// name a slot that does not exist, say. The document's problem, not the
    /// caller's.
    Unreadable(String),
}

impl std::fmt::Display for NotCorrected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NoSuchEdition(m)
            | Self::NoSuchSlot(m)
            | Self::SameEdition(m)
            | Self::NoPath(m)
            | Self::Unreadable(m) => m,
        };
        f.write_str(message)
    }
}
```

Add to `impl Sidecar`, above `runs_of`:

```rust
    /// Record by hand that slot `a` of `from` pairs with slot `b` of `to` — or,
    /// where `b` is `None`, with nothing.
    ///
    /// This is the operation the correction UI performs, and it lives here
    /// rather than in the server because it is format logic: any other reader
    /// writing back would need exactly this, and `docs/sidecar.md`'s promise
    /// that a hand-corrected run survives a re-solve is only meaningful if one
    /// place decides what a hand-corrected run *is*.
    ///
    /// Three cases, and the middle one is the subtle one:
    ///
    /// - The edge is stored in the direction asked for: rewrite it.
    /// - The edge is stored the other way round: **rewrite it in its own
    ///   direction**, not inverted-corrected-inverted. The assertion is
    ///   symmetric — "a pairs with b" and "b pairs with a" are one fact — and
    ///   `invert` drops gaps by construction, so a round trip through it would
    ///   discard every unpaired run the stored direction records.
    /// - There is no edge, so the pair resolves through the hub: write a direct
    ///   edge seeded from the composition with the fix applied. `sidecar.md`
    ///   permits a direct edge "for pairs where composition demonstrably
    ///   loses", and a hand correction is that demonstration. The cost is that
    ///   the pair leaves the star and stops inheriting transitivity.
    pub fn correct(
        &mut self,
        from: &str,
        to: &str,
        a: Slot,
        b: Option<Slot>,
        note: Option<String>,
    ) -> Result<Corrected, NotCorrected> {
        if from == to {
            return Err(NotCorrected::SameEdition(format!(
                "`{from}` is the edition being corrected against; an edition cannot be its own \
                 counterpart"
            )));
        }
        let reading = self.work.reading;
        let slots_of = |name: &str| {
            self.editions
                .get(name)
                .map(|e| e.slots(reading))
                .ok_or_else(|| {
                    NotCorrected::NoSuchEdition(format!(
                        "this document defines no edition named `{name}`"
                    ))
                })
        };
        let from_slots = slots_of(from)?;
        let to_slots = slots_of(to)?;
        let a_index = slot_index(&from_slots, a).ok_or_else(|| {
            NotCorrected::NoSuchSlot(format!("`{from}` has no slot `{}`", show_slot(a)))
        })? as u32;
        let b_index = match b {
            Some(b) => Some(slot_index(&to_slots, b).ok_or_else(|| {
                NotCorrected::NoSuchSlot(format!("`{to}` has no slot `{}`", show_slot(b)))
            })? as u32),
            None => None,
        };

        let orientation = self.edge_between(from, to).map(|edge| edge.from == from);
        let (key, source, target) = match orientation {
            Some(true) => ((from.to_string(), to.to_string()), a_index, b_index),
            Some(false) => {
                let edge = self.edge_between(from, to).expect("just found");
                let key = (edge.from.clone(), edge.to.clone());
                let runs = self
                    .runs_of(edge)
                    .map_err(NotCorrected::Unreadable)?;
                match b_index {
                    Some(b) => (key, b, Some(a_index)),
                    // "this slot pairs with nothing", read in the stored
                    // direction, is "whatever currently reaches it pairs with
                    // nothing".
                    None => match (0..to_slots.len() as u32)
                        .find(|&s| crate::counterpart(&runs, s) == Some(a_index))
                    {
                        Some(owner) => (key, owner, None),
                        None => return Ok(Corrected::AlreadyTrue),
                    },
                }
            }
            None => {
                let composed = crate::route(self, from, to).ok_or_else(|| {
                    NotCorrected::NoPath(format!(
                        "no path from `{from}` to `{to}` in this document"
                    ))
                })?;
                if crate::counterpart(composed.runs(), a_index) == b_index {
                    return Ok(Corrected::AlreadyTrue);
                }
                let before = composed.runs().to_vec();
                let fixed = crate::correct_runs(&before, a_index, b_index);
                self.alignments.push(Alignment {
                    from: from.to_string(),
                    to: to.to_string(),
                    by: Source::Hand,
                    solver: None,
                    quality: None,
                    runs: Vec::new(),
                });
                self.rewrite(&(from.to_string(), to.to_string()), fixed, &before, note)?;
                return Ok(Corrected::DirectEdgeAdded);
            }
        };

        let edge = self
            .alignments
            .iter()
            .find(|e| e.from == key.0 && e.to == key.1)
            .expect("orientation was found by the same lookup");
        let before = self.runs_of(edge).map_err(NotCorrected::Unreadable)?;
        if crate::counterpart(&before, source) == target {
            return Ok(Corrected::AlreadyTrue);
        }
        let fixed = crate::correct_runs(&before, source, target);
        self.rewrite(&key, fixed, &before, note)?;
        Ok(Corrected::Edge)
    }

    /// Replace one edge's runs, mark by hand exactly what changed, and
    /// recompute the quality block beside them.
    ///
    /// `key` names the edge in its **stored** orientation, so everything here
    /// is in that edge's own slot space.
    fn rewrite(
        &mut self,
        key: &(String, String),
        runs: Vec<crate::Run>,
        before: &[crate::Run],
        note: Option<String>,
    ) -> Result<(), NotCorrected> {
        let reading = self.work.reading;
        let from_slots = self.editions[&key.0].slots(reading);
        let to_slots = self.editions[&key.1].slots(reading);
        let position = self
            .alignments
            .iter()
            .position(|e| e.from == key.0 && e.to == key.1)
            .expect("the edge was found before this was called");

        // Exactly the slots *this* correction changed the answer for: the one
        // asserted, and the one displaced from the target it held. Derived by
        // diffing rather than tracked, so the two cannot drift apart.
        let changed: Vec<u32> = (0..from_slots.len() as u32)
            .filter(|&s| crate::counterpart(before, s) != crate::counterpart(&runs, s))
            .collect();
        // Every source slot an *earlier* hand decision already covers. Kept
        // separate from `changed` because the note below belongs only to this
        // correction: folding them together would let each correction
        // overwrite the note every earlier one left behind.
        let mut by_hand: Vec<u32> = changed.clone();
        for entry in self.alignments[position]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
        {
            let start = slot_index(&from_slots, entry.a.start).unwrap_or(0) as u32;
            let end = slot_index(&from_slots, entry.a.end).unwrap_or(0) as u32;
            by_hand.extend(start..=end);
        }

        let mut entries =
            entries_from_runs(&runs, &from_slots, &to_slots).map_err(NotCorrected::Unreadable)?;
        let previous = self.alignments[position].runs.clone();
        for entry in entries.iter_mut() {
            let start = slot_index(&from_slots, entry.a.start).unwrap_or(0) as u32;
            let end = slot_index(&from_slots, entry.a.end).unwrap_or(0) as u32;
            // Every slot of the run, not any of them. A correction that
            // changes the answer always lands as its own single-slot run, so
            // `all` marks exactly what a person decided. `any` would mark the
            // whole run it sits inside — measured at 188 slots on the primary
            // corpus volume — and Task 3's refusal would then fire on every
            // later re-solve of an edge somebody once touched.
            if !(start..=end).all(|s| by_hand.contains(&s)) {
                continue;
            }
            entry.by = Some(Source::Hand);
            entry.note = match (start..=end).all(|s| changed.contains(&s)) {
                // This correction's own run: it carries this correction's note.
                true => note.clone(),
                // An earlier correction's run, carried through unchanged. Its
                // note is that correction's explanation and is not this one's
                // to replace or clear.
                false => previous
                    .iter()
                    .find(|e| e.a == entry.a)
                    .and_then(|e| e.note.clone()),
            };
        }

        self.alignments[position].runs = entries;
        self.alignments[position].quality = Some(Quality {
            verdict: crate::verdict(&runs, from_slots.len()),
            slots: from_slots.len() as u32,
            paired: crate::paired_slots(&runs),
            avg_run: crate::average_run_length(&runs),
        });
        Ok(())
    }
```

Export the new types from `crates/taiyaku-core/src/lib.rs`:

```rust
pub use sidecar::{
    Alignment, Anchor, ChapterAt, Corrected, Edition, Member, Merged, NotCorrected, Quality,
    RunEntry, Sidecar, SlotSpan, SolverInfo, Source, VERSION, Work, entries_from_runs,
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core 2>&1 | grep -E "^test result"`
Expected: `test result: ok. 82 passed` — 75 after Task 1, plus these seven.

- [ ] **Step 5: Sabotage check**

Change `.all(|s| by_hand.contains(&s))` to `.any(...)`, re-run.
Expected: `correcting_a_stored_edge_rewrites_it_in_place` FAILS with hand runs
`["7-41", "42", "43-49", "50", "51-194"]`. Restore.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-core/src/sidecar.rs crates/taiyaku-core/src/lib.rs
git commit -m "core: Sidecar::correct — a hand correction, applied to a document"
```

---

### Task 3: O13 — a hand correction survives a re-solve

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs` (`merge_spoke`, `Merged`)
- Test: `crates/taiyaku-core/src/sidecar.rs` (`mod tests`)

**Interfaces:**
- Consumes: `Corrected` is not involved; this reads `RunEntry::by` written by Task 2.
- Produces: `Merged` gains a variant carrying how many hand runs were carried:
  ```rust
  pub enum Merged { Added, Replaced, CarriedHandRuns(usize) }
  ```
  `crates/taiyaku-cli/src/main.rs`'s `align --write` match on `Merged` must gain
  the arm — the compiler will name the site.

The three cases are the design's table, taken literally:

| Merge case | Behaviour |
| ---------- | --------- |
| Spoke `sha256` unchanged | hand runs carry across; the merge reports how many |
| Spoke replaced, hand runs present | **refused**, naming the corrections at risk and how many |
| Spoke replaced, no hand runs | proceeds exactly as today |

- [ ] **Step 1: Write the failing tests**

```rust
    /// A spoke whose bytes are unchanged is the ordinary re-solve: the solver
    /// ran again over the same two editions, and a run somebody fixed by hand
    /// is a statement about those bytes that the solver has no better claim
    /// on. `docs/sidecar.md`: "a hand-corrected run must survive a re-solve;
    /// this is the field that makes the correction UI more than a viewer."
    #[test]
    fn a_hand_run_survives_a_re_solve_of_an_unchanged_spoke() {
        let mut s = two_edition();
        s.correct("a", "b", Slot { page: 5, half: None }, Some(Slot { page: 9, half: None }), None)
            .expect("corrects");
        let hand_before = s.alignments[0]
            .runs
            .iter()
            .filter(|e| e.by == Some(Source::Hand))
            .count();
        assert!(hand_before > 0, "the fixture must actually hold a hand run");

        let merged = s
            .merge_spoke(
                Direction::RightToLeft,
                Anchor::Hub,
                &edition("a.cbz", "00"),
                "b",
                edition("b.cbz", "01"),
                edge("a", "b"),
            )
            .expect("an unchanged spoke merges");

        assert_eq!(merged, Merged::CarriedHandRuns(hand_before));
        assert_eq!(
            s.alignments[0]
                .runs
                .iter()
                .filter(|e| e.by == Some(Source::Hand))
                .count(),
            hand_before,
            "the corrections are still there after the solver's runs replaced the rest"
        );
        assert_eq!(
            crate::resolve(&s, "a", "b", 5),
            Some(Slot { page: 9, half: None }),
            "and they still say what they said"
        );
    }

    /// The case the from-side guard never covered. Re-solving a spoke after
    /// fixing its archive is ordinary and stays allowed — but a correction
    /// addresses slots of the edition that is being replaced, so carrying it
    /// across would resurrect runs pointing into bytes that are gone. What
    /// should become of such a correction is a real question, deliberately
    /// left open in `docs/status.md`; refusing is what stops it being answered
    /// silently.
    #[test]
    fn replacing_a_spoke_that_holds_hand_runs_is_refused() {
        let mut s = two_edition();
        s.correct("a", "b", Slot { page: 5, half: None }, Some(Slot { page: 9, half: None }), None)
            .expect("corrects");

        let refused = s.merge_spoke(
            Direction::RightToLeft,
            Anchor::Hub,
            &edition("a.cbz", "00"),
            "b",
            edition("b2.cbz", "99"),
            edge("a", "b"),
        );

        let message = refused.expect_err("a replaced spoke with corrections must not merge");
        assert!(message.contains("`b`"), "must name the spoke: {message}");
        assert!(
            message.contains("hand"),
            "must say what is at risk: {message}"
        );
        assert!(
            message.chars().any(|c| c.is_ascii_digit()),
            "must say how many: {message}"
        );
        assert_eq!(
            s.editions["b"].members[0].sha256, "01",
            "a refused merge must change nothing"
        );
    }
```

Then **narrow the existing test** rather than deleting it — its subject is that
a replaced spoke merges, which is still true when nothing was corrected. Rename
it and add the reason:

```rust
    /// Re-solving a spoke after fixing its archive is ordinary, so a changed
    /// `sha256` on the spoke side still merges. Narrowed by O13: this is now
    /// specifically the *no hand runs* case, which is what keeps that fix a
    /// narrowing rather than a breaking change.
    #[test]
    fn re_aligning_a_spoke_with_no_hand_runs_replaces_it_rather_than_duplicating() {
```

(the body is unchanged, including its `assert_eq!(merged, Merged::Replaced)`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core spoke`
Expected: FAIL — `no variant named 'CarriedHandRuns'`, and
`replacing_a_spoke_that_holds_hand_runs_is_refused` fails because the merge
succeeds.

- [ ] **Step 3: Write the implementation**

In `crates/taiyaku-core/src/sidecar.rs`, extend `Merged`:

```rust
/// What a merge did, so a caller can say so rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merged {
    Added,
    Replaced,
    /// The spoke's bytes were unchanged, so the corrections on the old edge
    /// were carried onto the new one. The count is how many.
    CarriedHandRuns(usize),
}
```

Replace the tail of `merge_spoke` — everything from `let replaced = ...` — with:

```rust
        // O13. `docs/sidecar.md` promises a hand-corrected run survives a
        // re-solve, and dropping the stored edge wholesale is what broke that
        // promise for as long as nothing could produce one.
        //
        // The guard above compares the *from* side: it refuses when the
        // document's copy of the hub is not the edition just aligned. The
        // spoke is a different question, and the answer divides. Where the
        // spoke's bytes are unchanged, a correction is a statement about those
        // bytes and carries across. Where the spoke is being replaced, a
        // correction addresses slots of an edition that is going away, and
        // carrying it would resurrect runs pointing into bytes that no longer
        // exist — the failure the from-side guard exists to prevent, let back
        // in through the other door. What *should* become of such a correction
        // is left open deliberately; refusing is what keeps it from being
        // answered silently.
        let stored_edge = self
            .alignments
            .iter()
            .find(|e| (e.from == start && e.to == name) || (e.from == name && e.to == start));
        let hand: Vec<RunEntry> = stored_edge
            .map(|e| {
                e.runs
                    .iter()
                    .filter(|r| r.by == Some(Source::Hand))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let spoke_replaced = self
            .editions
            .get(name)
            .is_some_and(|held| held.members != edition.members);
        if spoke_replaced && !hand.is_empty() {
            return Err(format!(
                "`{name}` in this document holds {} hand-corrected run(s), and the edition \
                 being merged is not the one they were made against. A correction addresses \
                 slots of the edition it was made on, so carrying it onto different bytes \
                 would point it at pages that no longer exist. Re-solve the work, or remove \
                 the `\"by\": \"hand\"` runs from `{start}` -> `{name}` deliberately.",
                hand.len()
            ));
        }

        let replaced = self.editions.insert(name.to_string(), edition).is_some();
        // Edition and edge are replaced together. A surviving edge would
        // address pages of an edition that is no longer there.
        self.alignments
            .retain(|a| !(a.from == start && a.to == name) && !(a.from == name && a.to == start));
        let mut alignment = alignment;
        // Carried onto the freshly solved edge by *replacing* the solver's run
        // wherever a hand run covers the same source slots, so the two cannot
        // both claim one slot. The hand run wins: that is the whole point of
        // the field.
        let carried = hand.len();
        if carried > 0 {
            alignment.runs = carry_hand_runs(&alignment.runs, &hand);
        }
        Ok(match (carried, replaced) {
            (0, true) => Merged::Replaced,
            (0, false) => Merged::Added,
            _ => Merged::CarriedHandRuns(carried),
        })
    }
```

Add the helper below `merge_spoke`:

```rust
/// Splice hand-corrected runs into a freshly solved run list.
///
/// A hand run's source span is authoritative, so any solved run overlapping it
/// gives up the overlapping slots. Written over spans rather than by rebuilding
/// a point map because the two lists are already in the document's own page
/// notation here, and converting to slot indices and back would need both slot
/// tables for no gain.
fn carry_hand_runs(solved: &[RunEntry], hand: &[RunEntry]) -> Vec<RunEntry> {
    let covered = |entry: &RunEntry| {
        hand.iter()
            .any(|h| h.a.start.page <= entry.a.end.page && entry.a.start.page <= h.a.end.page)
    };
    let mut out: Vec<RunEntry> = solved.iter().filter(|e| !covered(e)).cloned().collect();
    out.extend(hand.iter().cloned());
    out.sort_by_key(|e| (e.a.start.page, e.a.start.half.is_some()));
    out
}
```

> **Implementer's note, and the one place to be careful.** `carry_hand_runs`
> drops a solved run that *overlaps* a hand run rather than splitting it, so a
> hand run sitting inside a long solved run costs that whole run. On the corpus
> that means a carried correction can remove a large paired run from the merged
> edge. Write `a_hand_run_survives_a_re_solve_of_an_unchanged_spoke` first and
> check what `resolve` answers for slots either side of the carried run; if the
> surrounding pairings are gone, split the overlapping solved run around the
> hand run's span instead of dropping it, and add a test pinning a slot on each
> side. Do not skip this check — the fixture in the test above is 30 slots in
> one run, which is exactly the shape that hides the difference.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core 2>&1 | grep -E "^test result"`
Expected: `test result: ok. 84 passed`.

Then fix the CLI's non-exhaustive match: `cargo build --workspace` will fail in
`crates/taiyaku-cli/src/main.rs` at the `match merged` in `align`. Add:

```rust
                    Merged::CarriedHandRuns(n) => &format!("replaced, carrying {n} hand run(s)"),
```

adjusting for the surrounding `let verb = match merged { ... }` producing a
`&str` — make the binding a `String` and the other arms `.to_string()` if the
borrow does not typecheck.

- [ ] **Step 5: Run the whole workspace**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|error"`
Expected: every package ok, no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-core/src/sidecar.rs crates/taiyaku-cli/src/main.rs
git commit -m "core: carry hand runs across a re-solve, refuse where the spoke changed (O13)"
```

---

### Task 4: the served sidecar becomes replaceable

**Files:**
- Modify: `crates/taiyaku-serve/src/works.rs` (`Work`)
- Modify: `crates/taiyaku-serve/src/lib.rs`, `routes.rs`, `opds.rs`, `composite.rs`,
  `source.rs`, `stump/resolve.rs`, `corpus.rs` — every `work.sidecar` reader
- Test: `crates/taiyaku-serve/src/works.rs`

**Why this is a task rather than a line:** `ServeState` is behind an `Arc` in the
router and holds `works: Vec<Work>` by value, so nothing can replace a parsed
document today. A `PATCH` that wrote the file and left memory alone would make
the correction vanish until the next restart, which is precisely the failure
the design says a correction UI can least afford.

**Interfaces:**
- Produces:
  ```rust
  impl Work {
      pub fn new(id: String, path: PathBuf, sidecar: Sidecar) -> Self;
      /// The document as it stands. An `Arc` clone, not a copy.
      pub fn sidecar(&self) -> Arc<Sidecar>;
      /// Read, change and replace under one exclusive lock, so a
      /// compare-then-write cannot interleave with another writer.
      pub fn update<T>(&self, f: impl FnOnce(&Arc<Sidecar>) -> Result<(Arc<Sidecar>, T), Response>)
          -> Result<T, Response>;
  }
  ```
  Task 6 is `update`'s only caller. Every other site changes from
  `work.sidecar.x` to a local `let sidecar = work.sidecar();` first.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/works.rs`:

```rust
    /// The property the correction route rests on: what a later reader gets
    /// back is what the last write put in, without a restart.
    #[test]
    fn a_replaced_sidecar_is_what_the_next_reader_sees() {
        let work = crate::test_support::work("taiyaku-test-works-replace");
        let before = work.sidecar();

        let swapped: Result<(), axum::response::Response> = work.update(|current| {
            let mut next = (**current).clone();
            next.work.title = "Renamed".into();
            Ok((std::sync::Arc::new(next), ()))
        });

        let after = work.sidecar();
        crate::test_support::cleanup("taiyaku-test-works-replace");
        swapped.expect("the update runs");
        assert_eq!(before.work.title, "T");
        assert_eq!(after.work.title, "Renamed");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p taiyaku-serve replaced_sidecar`
Expected: FAIL — `no method named 'sidecar' found` / `no method named 'update'`.

- [ ] **Step 3: Write the implementation**

Replace the `Work` struct in `crates/taiyaku-serve/src/works.rs`:

```rust
/// One work the server can serve, and where its sidecar came from.
///
/// Not to be confused with `taiyaku_core::Work`, which is the `work` block
/// inside a sidecar — a title and a reading direction. This one is a loaded
/// document plus the identity a URL names it by, and reaches that one as
/// `self.sidecar().work`.
///
/// The document is held behind an `RwLock<Arc<_>>` rather than owned outright
/// because `PATCH /api/works/{work}/alignments` replaces it while the server
/// is running: readers clone an `Arc` and are never blocked by each other, and
/// a writer swaps a whole new document in under one exclusive lock. The same
/// shape `StumpSide` already uses for its resolution map, and for the same
/// reason.
///
/// **Deliberately not `Clone`.** A cloned `Work` would carry an independent
/// lock, so a correction applied to one copy would be invisible to the other —
/// two documents claiming one work, which is the failure `load_works` already
/// refuses at startup.
#[derive(Debug)]
pub struct Work {
    pub id: String,
    pub path: PathBuf,
    sidecar: std::sync::RwLock<std::sync::Arc<Sidecar>>,
}

impl Work {
    pub fn new(id: String, path: PathBuf, sidecar: Sidecar) -> Self {
        Self {
            id,
            path,
            sidecar: std::sync::RwLock::new(std::sync::Arc::new(sidecar)),
        }
    }

    /// The document as it stands. An `Arc` clone, not a copy of the document.
    ///
    /// A poisoned lock is a panic in a handler that already ran, and the
    /// document behind it is still the last one successfully written, so this
    /// takes the value rather than propagating the poison — a work that
    /// refused to serve every subsequent request because one unrelated handler
    /// panicked would be the worse answer.
    pub fn sidecar(&self) -> std::sync::Arc<Sidecar> {
        match self.sidecar.read() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Read, change and replace under one exclusive lock.
    ///
    /// The whole read-modify-write happens inside the guard, which is what
    /// makes `If-Match` mean anything: two devices correcting one work would
    /// otherwise both compare against the same validator, both pass, and the
    /// second would replace the first's correction with a document that never
    /// contained it.
    ///
    /// The closure returns the replacement document *and* whatever the caller
    /// wants back, or a `Response` to refuse with. A refusal leaves the held
    /// document untouched.
    pub fn update<T>(
        &self,
        f: impl FnOnce(&std::sync::Arc<Sidecar>) -> Result<(std::sync::Arc<Sidecar>, T), axum::response::Response>,
    ) -> Result<T, axum::response::Response> {
        let mut held = match self.sidecar.write() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (next, out) = f(&held)?;
        *held = next;
        Ok(out)
    }
}
```

Update `load_works`'s push:

```rust
        works.push(Work::new(id, path.clone(), sidecar));
```

Then follow the compiler. Every `work.sidecar.<field>` becomes a local binding
first, because `work.sidecar()` returns a temporary and borrowing a field out
of it trips E0716:

```rust
    // before
    let found = work.sidecar.editions.get(edition).ok_or_else(...)?;
    // after
    let sidecar = work.sidecar();
    let found = sidecar.editions.get(edition).ok_or_else(...)?;
```

`routes.rs::locate_member` must return owned data for the same reason:

```rust
pub(crate) fn locate_member(work: &Work, edition: &str, index: u32) -> Result<(String, u32), String> {
```

with its two callers taking `let (sha256, offset) = ...;` and passing
`&sha256` to `page_etag`.

`forget_all_works` in `lib.rs` is the one place that needed `Work: Clone`.
Change it to borrow instead:

```rust
fn forget_all_works<'a>(works: impl IntoIterator<Item = &'a Work>) {
```

its production caller to `forget_all_works(&works)`, and its test to
`super::forget_all_works([&recorded, &untouched])`.

Every `Work { id, path, sidecar }` literal in `test_support`, `opds.rs` and
`lib.rs`'s test module becomes `Work::new(id, path, sidecar)`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve 2>&1 | grep -E "^test result|^error"`
Expected: ok, one more test than before, no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src
git commit -m "serve: hold each work's sidecar behind a lock so it can be replaced"
```

---

### Task 5: one atomic writer, and an `ETag` on the sidecar

**Files:**
- Create: `crates/taiyaku-serve/src/atomic.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod atomic;`)
- Modify: `crates/taiyaku-serve/src/progress.rs`, `cover.rs` (use it)
- Modify: `crates/taiyaku-serve/src/routes.rs` (`sidecar_etag`, `if_match_hits`, `get_sidecar`)

**Interfaces:**
- Produces:
  ```rust
  pub(crate) fn atomic::write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String>;
  pub(crate) fn routes::sidecar_etag(sidecar: &Sidecar) -> String;
  pub(crate) fn routes::if_match_hits(headers: &HeaderMap, tag: &str) -> Option<bool>;
  ```
  `if_match_hits` answers `None` where the header is absent — which Task 6
  turns into a 428, a different answer from a mismatch.

There are two copies of temp-then-rename already (`progress::unique_temp_path`,
`cover::unique_temp_path`), and `progress.rs`'s doc comment says so: "The same
reasoning, and the same shape, as `cover::unique_temp_path`. They are not
shared yet." The sidecar writer would be the third, which is where a comment
asking people to keep copies in step stops being good enough.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/atomic.rs` with only its tests at first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The property O2 established for progress and this now shares: the
    /// destination is replaced by a rename, never opened for truncation, so a
    /// crash mid-write cannot leave a short file where a whole one was.
    /// Observable by taking write permission off the existing file — a
    /// truncating writer fails, a renaming one does not.
    #[test]
    fn the_destination_is_replaced_by_rename_not_truncation() {
        let dir = std::env::temp_dir().join("taiyaku-test-atomic-rename");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("thing.json");
        std::fs::write(&path, b"old").unwrap();
        let mut mode = std::fs::metadata(&path).unwrap().permissions();
        mode.set_readonly(true);
        std::fs::set_permissions(&path, mode).unwrap();
        if std::fs::write(&path, b"probe").is_ok() {
            println!("skipping: this filesystem ignores the read-only bit");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        let wrote = write(&path, b"new", "json");

        let read_back = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        wrote.expect("a rename replaces a file the caller cannot open for writing");
        assert_eq!(read_back, "new");
    }

    /// Unique per attempt, not a fixed name: two writers sharing one temp
    /// path can interleave into publishing a half-written file, which is the
    /// whole thing this exists to prevent.
    #[test]
    fn two_temp_paths_for_one_destination_differ() {
        let path = std::path::Path::new("/tmp/taiyaku-test-atomic/thing.json");
        assert_ne!(temp_path(path, "json"), temp_path(path, "json"));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve atomic`
Expected: FAIL — the module is not declared, then `cannot find function 'write'`.

- [ ] **Step 3: Write the implementation**

`crates/taiyaku-serve/src/atomic.rs`, above the tests:

```rust
//! Write a file so a crash cannot publish half of one.
//!
//! Three callers now: the reading position (O2), a generated cover, and a
//! sidecar rewritten by a correction. The first two carried their own copy of
//! this and said so in a comment; a third copy is where a comment asking
//! whoever changes one to keep the others in step stops being good enough.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A temp path nothing else will ever pick.
///
/// A process id plus a monotonic per-process counter. Two devices writing one
/// work through one server is two requests in one process, so the counter
/// carries that case; two servers over one directory is what the pid carries.
pub(crate) fn temp_path(path: &Path, extension: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    path.with_extension(format!("{extension}.tmp.{pid}.{n}"))
}

/// Write `bytes` to `path`, atomically.
///
/// `fs::write` truncates first, and a crash in that window leaves a short file
/// where a whole one was — for progress that answers 500 instead of "nothing
/// recorded", and for a sidecar it destroys the alignment of every edition in
/// the work.
///
/// A failed rename leaves a temp file nothing will ever collect, since its name
/// is unique to this attempt, so it is removed here rather than accumulating one
/// file per failure in the reader's own library directory.
pub(crate) fn write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String> {
    let temporary = temp_path(path, extension);
    std::fs::write(&temporary, bytes).map_err(|e| format!("{}: {e}", temporary.display()))?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!("{}: {e}", path.display()))
        }
    }
}
```

Declare it in `lib.rs` beside the other modules: `mod atomic;`

Rewrite `progress::store`'s body to call it, deleting `progress::unique_temp_path`:

```rust
pub fn store(work: &Work, progress: &Progress) -> Result<(), String> {
    let path = progress_path(work);
    let text =
        serde_json::to_string_pretty(progress).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::atomic::write(&path, text.as_bytes(), "json")
}
```

Do the same for `cover.rs`'s writer, deleting `cover::unique_temp_path` and
passing `"jpg"`.

Add to `crates/taiyaku-serve/src/routes.rs`, beside `page_etag`:

```rust
/// A sidecar's `ETag`: a SHA-256 over exactly the bytes this server would send.
///
/// Over the serialised document rather than a version counter, because the
/// point is that two servers, or one server across a restart, agree about
/// whether a client's copy is current — a counter would restart at zero and
/// silently validate a stale copy. Computed on demand rather than cached: a
/// sidecar is tens of kilobytes and this route is asked once per page load,
/// against a `PATCH` path where being wrong costs a lost correction.
pub(crate) fn sidecar_etag(sidecar: &taiyaku_core::Sidecar) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(sidecar.to_json().as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

/// Whether a request's `If-Match` names `tag`.
///
/// `None` where the header is absent, which is a different answer from `false`
/// and gets a different status: a client that sent no validator has not lost a
/// race, it has not entered one, and telling it "precondition failed" would
/// send it looking for a conflict that does not exist.
pub(crate) fn if_match_hits(headers: &HeaderMap, tag: &str) -> Option<bool> {
    let sent = headers.get(header::IF_MATCH)?.to_str().ok()?;
    Some(sent == tag)
}
```

Send it from `get_sidecar`, and answer a matching `If-None-Match` there too —
the reader fetches this document on every load:

```rust
async fn get_sidecar(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    let sidecar = found.sidecar();
    let tag = sidecar_etag(&sidecar);
    if if_none_match_hits(&headers, &tag) {
        return not_modified(&tag);
    }
    // Served verbatim from the parsed document rather than by re-reading the
    // file, so what the client resolves against is exactly what this server
    // holds — and exactly what the `ETag` above was computed over.
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ETAG, tag.as_str()),
        ],
        sidecar.to_json(),
    )
        .into_response()
}
```

- [ ] **Step 4: Add the route test**

```rust
    /// The validator the correction path writes against. It must change when
    /// the document does, or `If-Match` guards nothing.
    #[tokio::test]
    async fn the_sidecar_carries_an_etag_that_tracks_its_content() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-routes-sidecar-etag"));
        let router = build_router(state.clone());

        let first = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/works/t")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = first.headers().get(axum::http::header::ETAG).cloned();
        state.works[0]
            .update(|current| {
                let mut next = (**current).clone();
                next.work.title = "Different".into();
                Ok((std::sync::Arc::new(next), ()))
            })
            .expect("updates");
        let second = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/works/t")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-routes-sidecar-etag");
        let tag = tag.expect("a sidecar response carries an ETag");
        assert_ne!(
            tag,
            *second.headers().get(axum::http::header::ETAG).unwrap(),
            "a changed document must not validate against the old tag"
        );
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve 2>&1 | grep -E "^test result|^error"`
Expected: ok, four more tests than Task 4 left, no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src
git commit -m "serve: one atomic writer for three callers, and an ETag on the sidecar"
```

---

### Task 6: `PATCH /api/works/{work}/alignments`

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs`
- Test: `crates/taiyaku-serve/src/routes.rs` (`mod tests`)

**Interfaces:**
- Consumes: `Sidecar::correct` (Task 2), `Work::update` (Task 4),
  `sidecar_etag`/`if_match_hits`/`atomic::write` (Task 5).
- Produces: the route, and this request body:
  ```jsonc
  {
    "from": "raw-mono",   // the edition the reader is reading
    "to":   "en-mono",    // the edition they are correcting against
    "a":    "50",         // a slot of `from`, in the sidecar's own span notation
    "b":    "40",         // a slot of `to`, or null to unpair
    "note": "optional"
  }
  ```
  Answers `200` with the updated document and its new `ETag`, so the client
  refreshes in the same round trip it corrected in.

Statuses, each a different thing a client can act on:

| Situation | Status |
| --------- | ------ |
| No such work | 404 |
| Body does not parse, or `a`/`b` is not a single slot | 400 |
| `If-Match` absent | 428 |
| `If-Match` present and stale | 412 |
| `NoSuchEdition` / `NoSuchSlot` / `NoPath` | 404 |
| `SameEdition` | 400 |
| `Unreadable` — a stored edge this document cannot read | 409 |
| Written | 200 |

- [ ] **Step 1: Write the failing tests**

```rust
    /// The milestone in one test: correct a pairing, and have the document say
    /// so afterwards.
    #[tokio::test]
    async fn a_correction_is_written_and_read_back() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct",
        ));
        let router = build_router(state.clone());
        let tag = etag_of(&router, "/api/works/t").await;

        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri("/api/works/t/alignments")
                    .header("if-match", &tag)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let fresh = response.headers().get(axum::http::header::ETAG).cloned();
        let on_disk = std::fs::read_to_string(&state.works[0].path).unwrap();
        crate::test_support::cleanup("taiyaku-test-routes-correct");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_ne!(fresh, Some(tag.parse().unwrap()), "the ETag moves with the write");
        assert!(on_disk.contains("\"hand\""), "the file records who decided: {on_disk}");
        assert_eq!(
            state.works[0].sidecar().alignments[0]
                .runs
                .iter()
                .filter(|r| r.by == Some(taiyaku_core::Source::Hand))
                .count(),
            2,
            "the slot asserted and the slot displaced, in memory as well as on disk"
        );
    }

    /// The lost-update hole the design names. Two devices read one document,
    /// both correct it; the second must be told rather than silently winning.
    #[tokio::test]
    async fn a_correction_against_a_stale_validator_is_refused() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-stale",
        ));
        let router = build_router(state.clone());
        let stale = etag_of(&router, "/api/works/t").await;
        correct(&router, &stale, r#"{"from":"raw","to":"en","a":"2","b":"1"}"#).await;

        let second = correct(
            &router,
            &stale,
            r#"{"from":"raw","to":"en","a":"3","b":"0"}"#,
        )
        .await;

        let after = state.works[0].sidecar();
        crate::test_support::cleanup("taiyaku-test-routes-correct-stale");
        assert_eq!(second, axum::http::StatusCode::PRECONDITION_FAILED);
        assert_eq!(
            taiyaku_core::resolve(&after, "raw", "en", 3),
            None,
            "the refused correction must not have landed"
        );
    }

    /// A client that sent no validator has not lost a race, it has not entered
    /// one — and telling it 412 would send it looking for a conflict that does
    /// not exist.
    #[tokio::test]
    async fn a_correction_with_no_validator_at_all_says_which_header_is_missing() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-bare",
        ));
        let router = build_router(state);

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri("/api/works/t/alignments")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"from":"raw","to":"en","a":"2","b":"1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        crate::test_support::cleanup("taiyaku-test-routes-correct-bare");
        assert_eq!(status, axum::http::StatusCode::PRECONDITION_REQUIRED);
        assert!(
            String::from_utf8_lossy(&body).contains("If-Match"),
            "must name the header a client has to send"
        );
    }

    /// A span is the format's own notation and `45R-46L` is a legal one — but
    /// a correction addresses one slot, and taking the start of a range would
    /// silently correct something other than what was asked.
    #[tokio::test]
    async fn a_correction_naming_a_range_rather_than_a_slot_is_refused() {
        let state = Arc::new(crate::test_support::state_with_two_editions(
            "taiyaku-test-routes-correct-range",
        ));
        let router = build_router(state.clone());
        let tag = etag_of(&router, "/api/works/t").await;

        let status = correct(&router, &tag, r#"{"from":"raw","to":"en","a":"2-3","b":"1"}"#).await;

        crate::test_support::cleanup("taiyaku-test-routes-correct-range");
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
```

with two helpers next to them:

```rust
    async fn etag_of(router: &axum::Router, uri: &str) -> String {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get(axum::http::header::ETAG)
            .expect("an ETag")
            .to_str()
            .unwrap()
            .to_string()
    }

    async fn correct(router: &axum::Router, tag: &str, body: &str) -> axum::http::StatusCode {
        router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri("/api/works/t/alignments")
                    .header("if-match", tag)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }
```

**And a fixture with two editions and an alignment**, which
`test_support::work` does not provide (it has one edition and no alignments).
Add to `crates/taiyaku-serve/src/lib.rs`'s `test_support`:

```rust
    /// The shared fixture plus a second edition and a solved edge between
    /// them, which is the minimum a correction needs to address.
    ///
    /// `raw` is four pages across two members, as `work` builds it; `en` is
    /// the same two members under a second key, so no new archives are needed
    /// and the pages a correction addresses really exist.
    pub(crate) fn state_with_two_editions(dir: &str) -> ServeState {
        let mut state = state(dir);
        let path = state.works[0].path.clone();
        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] },
            "en": { "label": "En", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] }
          ]
        }"#;
        std::fs::write(&path, json).unwrap();
        state.works = vec![crate::Work::new(
            "t".into(),
            path,
            taiyaku_core::Sidecar::from_json(json).unwrap(),
        )];
        state
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve correct`
Expected: FAIL — `state_with_two_editions` is undefined, then 405 Method Not
Allowed once it exists, because nothing routes `PATCH` yet.

- [ ] **Step 3: Write the implementation**

Add to `crates/taiyaku-serve/src/routes.rs`:

```rust
/// One hand correction, as a client asks for it.
///
/// Slots are the format's own span notation (`7`, `4R`, `45R`) rather than
/// bare indices, so the wire, the document and the contact sheet all name a
/// slot the same way — and a client that is off by a half says so visibly
/// instead of resolving to a neighbouring index.
#[derive(Debug, serde::Deserialize)]
struct Correction {
    from: String,
    to: String,
    a: String,
    /// Absent or null unpairs `a`.
    #[serde(default)]
    b: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// One slot from a span, refusing a range.
///
/// `45R-46L` is legal notation and names two slots; a correction addresses
/// one, and quietly taking the start of a range would correct something other
/// than what was asked for.
fn one_slot(text: &str, side: &str) -> Result<taiyaku_core::Slot, Response> {
    let span = taiyaku_core::SlotSpan::parse(text)
        .map_err(|why| bad_request(format!("`{side}`: {why}")))?;
    if span.start != span.end {
        return Err(bad_request(format!(
            "`{side}` is `{text}`, which names a range; a correction addresses one slot"
        )));
    }
    Ok(span.start)
}

/// Write a hand correction into the work's sidecar.
///
/// The client sends the assertion, never a computed run list: the split is
/// applied by `Sidecar::correct` against the document this server holds, so
/// there is one implementation of the arithmetic and a stale client cannot
/// replace an edge wholesale. `If-Match` is what makes "the document this
/// server holds" and "the document the client was looking at" the same thing.
async fn patch_alignments(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let correction: Correction = match serde_json::from_slice(&body) {
        Ok(correction) => correction,
        Err(why) => return bad_request(format!("this is not a correction: {why}")),
    };
    let a = match one_slot(&correction.a, "a") {
        Ok(slot) => slot,
        Err(refusal) => return refusal,
    };
    let b = match correction.b.as_deref().map(|text| one_slot(text, "b")) {
        Some(Ok(slot)) => Some(slot),
        Some(Err(refusal)) => return refusal,
        None => None,
    };

    // Serialising, hashing and writing all block, and the read-modify-write
    // below holds an exclusive lock across the file write — `get_page`'s
    // reasoning about the async worker applies to it unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(not_found)?;
        found.update(|current| {
            let tag = sidecar_etag(current);
            match if_match_hits(&headers, &tag) {
                None => {
                    return Err((
                        StatusCode::PRECONDITION_REQUIRED,
                        "a correction must carry `If-Match` with the sidecar's ETag, so two \
                         devices correcting one work cannot silently overwrite each other"
                            .to_string(),
                    )
                        .into_response());
                }
                Some(false) => {
                    return Err((
                        StatusCode::PRECONDITION_FAILED,
                        format!(
                            "this work has changed since you read it; its ETag is now {tag}. \
                             Re-read the sidecar and correct again."
                        ),
                    )
                        .into_response());
                }
                Some(true) => {}
            }

            let mut next = (**current).clone();
            let what = next
                .correct(&correction.from, &correction.to, a, b, correction.note.clone())
                .map_err(|why| {
                    use taiyaku_core::NotCorrected::*;
                    let status = match why {
                        NoSuchEdition(_) | NoSuchSlot(_) | NoPath(_) => StatusCode::NOT_FOUND,
                        SameEdition(_) => StatusCode::BAD_REQUEST,
                        Unreadable(_) => StatusCode::CONFLICT,
                    };
                    (status, why.to_string()).into_response()
                })?;

            // The file first, then memory. A failed write leaves the served
            // document exactly as it was, so a client that gets a 500 has lost
            // its correction and nothing else — where swapping memory first
            // would leave this server serving an alignment that is on no disk
            // anywhere.
            let json = next.to_json();
            if what != taiyaku_core::Corrected::AlreadyTrue {
                crate::atomic::write(&found.path, json.as_bytes(), "json").map_err(|why| {
                    (StatusCode::INTERNAL_SERVER_ERROR, why).into_response()
                })?;
            }
            let next = std::sync::Arc::new(next);
            let tag = sidecar_etag(&next);
            Ok((next, (json, tag)))
        })
    })
    .await;

    match written {
        Ok(Ok((json, tag))) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ETAG, tag.as_str()),
            ],
            json,
        )
            .into_response(),
        Ok(Err(refusal)) => refusal,
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("correction panicked: {e}"),
        )
            .into_response(),
    }
}
```

Register it in `build_router`:

```rust
        .route(
            "/api/works/{work}/alignments",
            axum::routing::patch(patch_alignments),
        )
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve 2>&1 | grep -E "^test result|^error"`
Expected: ok, four more tests, no errors.

- [ ] **Step 5: Sabotage check**

Comment out the `Some(false) => ...` arm so a stale validator falls through.
Expected: `a_correction_against_a_stale_validator_is_refused` FAILS. Restore.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/lib.rs
git commit -m "serve: PATCH a hand correction, guarded by If-Match"
```

---

### Task 7: the client side of a correction, without the UI

**Files:**
- Modify: `web/src/api.ts`
- Create: `web/src/correction.ts`
- Test: `web/src/correction.test.ts`, `web/src/api.test.ts`

**Interfaces:**
- Produces:
  ```ts
  // api.ts
  export interface FetchedSidecar { json: string; etag?: string }
  export async function getSidecarWithEtag(work: string): Promise<FetchedSidecar>
  export interface CorrectionRequest {
    from: string; to: string; a: string; b: string | null; note?: string
  }
  export async function patchAlignment(
    work: string, etag: string, correction: CorrectionRequest,
  ): Promise<FetchedSidecar>

  // correction.ts
  export interface Pending { from: string; to: string; a: Position; b?: Position }
  export function spanOf(at: Position): string
  export function displacedBy(pending: Pending, resolver: Resolver): Position | undefined
  ```
  Task 8 consumes all of these. `getSidecar` stays, unchanged, so nothing that
  does not need the validator has to learn about it.

**No `taiyaku-wasm` change is needed to preview a correction.** The slot that a
correction will displace is the slot of `from` that currently pairs with `b` —
which is `resolver.resolve(to, from, b)`, an ordinary resolution in the
direction the reader is not looking. That works for a composed pair as well as
a stored edge, because `Route` composes either way.

- [ ] **Step 1: Write the failing tests**

`web/src/correction.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { displacedBy, spanOf } from './correction'
import type { Position, Resolver } from './resolver'

/** A resolver where `en` slot n pairs with `raw` slot n, and nothing else. */
function identityResolver(): Resolver {
  const at = (index: number): Position => ({ page: index })
  return {
    editions: () => ['raw', 'en'],
    slotCount: () => 10,
    slotAt: (_edition, index) => (index < 10 ? at(index) : undefined),
    indexOf: (_edition, position) => position.page,
    resolve: (_from, _to, position) =>
      position.page < 10 ? { ...position, via: 'direct' } : undefined,
    chapterAt: () => undefined,
  }
}

describe('spanOf', () => {
  it('writes a slot the way the sidecar writes one', () => {
    expect(spanOf({ page: 7 })).toBe('7')
    expect(spanOf({ page: 45, half: 'R' })).toBe('45R')
    expect(spanOf({ page: 46, half: 'L' })).toBe('46L')
  })
})

describe('displacedBy', () => {
  /**
   * The consequence a reader has to be shown before committing: asserting that
   * this slot pairs with that one takes the target away from whoever holds it,
   * and that slot ends up pairing with nothing.
   */
  it('names the slot that currently holds the target', () => {
    const displaced = displacedBy(
      { from: 'raw', to: 'en', a: { page: 3 }, b: { page: 5 } },
      identityResolver(),
    )
    expect(displaced).toEqual({ page: 5 })
  })

  it('is undefined when nothing holds the target', () => {
    const resolver = { ...identityResolver(), resolve: () => undefined }
    expect(
      displacedBy({ from: 'raw', to: 'en', a: { page: 3 }, b: { page: 5 } }, resolver),
    ).toBeUndefined()
  })

  /** Unpairing displaces nobody: the target is nothing. */
  it('is undefined when the correction unpairs', () => {
    expect(
      displacedBy({ from: 'raw', to: 'en', a: { page: 3 } }, identityResolver()),
    ).toBeUndefined()
  })

  /** A correction that asserts what is already true displaces nobody either. */
  it('is undefined when the slot being corrected already holds the target', () => {
    expect(
      displacedBy({ from: 'raw', to: 'en', a: { page: 5 }, b: { page: 5 } }, identityResolver()),
    ).toBeUndefined()
  })
})
```

Add to `web/src/api.test.ts`:

```ts
describe('patchAlignment', () => {
  it('sends the validator it was given and returns the fresh one', async () => {
    const calls: RequestInit[] = []
    vi.stubGlobal('fetch', (_url: string, init: RequestInit) => {
      calls.push(init)
      return Promise.resolve(
        new Response('{"taiyaku":1}', { status: 200, headers: { etag: '"new"' } }),
      )
    })

    const fresh = await patchAlignment('w', '"old"', {
      from: 'raw',
      to: 'en',
      a: '3',
      b: '5',
    })

    expect(new Headers(calls[0].headers).get('if-match')).toBe('"old"')
    expect(calls[0].method).toBe('PATCH')
    expect(fresh.etag).toBe('"new"')
    expect(fresh.json).toBe('{"taiyaku":1}')
  })

  /** A 412 is a real answer a reader has to be told about, not a network error. */
  it('reports a stale validator by name', async () => {
    vi.stubGlobal('fetch', () =>
      Promise.resolve(new Response('this work has changed since you read it', { status: 412 })),
    )

    await expect(
      patchAlignment('w', '"old"', { from: 'raw', to: 'en', a: '3', b: '5' }),
    ).rejects.toThrow(/changed since you read it/)
  })
})
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/correction.test.ts src/api.test.ts`
Expected: FAIL — `Failed to resolve import "./correction"`, and
`patchAlignment is not exported`.

- [ ] **Step 3: Write the implementation**

`web/src/correction.ts`:

```ts
/**
 * What a reader is about to assert, and what it will cost.
 *
 * Pure, and deliberately free of any `fetch`: the consequence of a correction
 * is arithmetic over the alignment the reader is already holding, and it has
 * to be shown *before* anything is written, because nothing undoes a
 * correction short of making another one.
 */
import type { Position, Resolver } from './resolver'

export interface Pending {
  /** The edition being read — the one whose slot `a` names. */
  from: string
  /** The edition being corrected against. */
  to: string
  a: Position
  /** Absent means "this slot pairs with nothing". */
  b?: Position
}

/**
 * A slot, in the sidecar's own span notation — `7`, `45R`, `46L`.
 *
 * The wire format for a correction, so the request, the document and the
 * contact sheet all name a slot the same way. A bare index would be a fourth
 * spelling of the same thing and the one nobody could read.
 */
export function spanOf(at: Position): string {
  return at.half ? `${at.page}${at.half}` : `${at.page}`
}

/**
 * Which slot of `from` will lose its counterpart if this correction lands.
 *
 * The alignment is strictly 1:1 in slots, so asserting that `a` pairs with `b`
 * is also asserting that whatever holds `b` today does not. That slot becomes
 * a gap, and a reader who is not shown this before committing finds out by
 * noticing a page stopped resolving.
 *
 * Answered by resolving in the direction the reader is *not* looking — `b`, in
 * `to`, back into `from` — which is one ordinary `Resolver.resolve` and works
 * for a composed pair exactly as it does for a stored edge. Nothing new has to
 * cross the WASM boundary for this.
 */
export function displacedBy(pending: Pending, resolver: Resolver): Position | undefined {
  if (!pending.b) return undefined
  const holder = resolver.resolve(pending.to, pending.from, pending.b)
  if (!holder) return undefined
  const same = holder.page === pending.a.page && holder.half === pending.a.half
  return same ? undefined : { page: holder.page, half: holder.half }
}
```

Add to `web/src/api.ts`:

```ts
export interface FetchedSidecar {
  json: string
  /** Absent only from a server older than the correction route. */
  etag?: string
}

/**
 * The sidecar, and the validator a correction has to be written against.
 *
 * Separate from `getSidecar` rather than replacing it: a caller that only
 * wants to read the document should not have to know a correction path exists,
 * and the validator is meaningless to it.
 */
export async function getSidecarWithEtag(work: string): Promise<FetchedSidecar> {
  const res = await fetch(`/api/works/${seg(work)}`)
  if (!res.ok) throw new Error(`loading ${work}: ${res.status} ${await res.text()}`)
  return { json: await res.text(), etag: res.headers.get('etag') ?? undefined }
}

export interface CorrectionRequest {
  from: string
  to: string
  /** A slot of `from`, in span notation — see `correction.ts`'s `spanOf`. */
  a: string
  /** A slot of `to`, or `null` to unpair `a`. */
  b: string | null
  note?: string
}

/**
 * Write one hand correction, and get the document it produced.
 *
 * `If-Match` is required by the server, not optional here: two devices
 * correcting one work is the same shape as two writing progress, and last
 * write wins would discard the other one's correction with nothing anywhere
 * saying so. The response body is the updated sidecar, so a caller refreshes
 * its resolver from this rather than making a second request that could
 * already be out of date again.
 *
 * No `keepalive` here, unlike `putProgress`: a correction is a deliberate act
 * a reader is watching the result of, not a background write that has to
 * survive the tab going away.
 */
export async function patchAlignment(
  work: string,
  etag: string,
  correction: CorrectionRequest,
): Promise<FetchedSidecar> {
  const res = await fetch(`/api/works/${seg(work)}/alignments`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json', 'if-match': etag },
    body: JSON.stringify(correction),
  })
  if (!res.ok) throw new Error(await res.text())
  return { json: await res.text(), etag: res.headers.get('etag') ?? undefined }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run`
Expected: all green, 6 more tests than before.

- [ ] **Step 5: Commit**

```bash
git add web/src/api.ts web/src/api.test.ts web/src/correction.ts web/src/correction.test.ts
git commit -m "web: the correction request, and what it displaces"
```

---

### Task 8: correction mode in the filmstrip

**Files:**
- Modify: `web/src/Filmstrip.tsx`
- Modify: `web/src/Chrome.tsx` (thread the new props)
- Modify: `web/src/App.tsx`
- Modify: `web/src/keys.ts` (claim `c`)
- Test: `web/src/keys.test.ts`

**Interfaces:**
- Consumes: `Pending`, `spanOf`, `displacedBy` (Task 7), `patchAlignment`,
  `getSidecarWithEtag` (Task 7).
- Produces: no new exported module. `FilmstripProps` gains:
  ```ts
  correcting?: { to: string; pairedIndex?: number }
  onPickCounterpart?: (slot: number) => void
  ```

The filmstrip's third job, from the design: "switches to the counterpart
edition, current resolution highlighted". `c` enters correction mode; the strip
shows the *counterpart* edition's tiles with the current resolution marked;
clicking a tile is the assertion; `Escape` leaves without writing.

- [ ] **Step 1: Write the failing test**

`web/src/keys.test.ts` — the key has to reach the reader as an intent, and must
not fire while a text field has focus:

```ts
  it('c asks to correct, and stands down for a focused control', () => {
    expect(intentOf(press('c'), ['raw', 'en'], [])).toEqual({ kind: 'correct' })
    expect(intentOf(press('c', { ctrlKey: true }), ['raw', 'en'], [])).toBeUndefined()
  })
```

(match the existing helpers in that file — `press` and the `intentOf` signature
already there.)

- [ ] **Step 2: Run to verify it fails**

Run: `cd web && npx vitest run src/keys.test.ts`
Expected: FAIL — `intentOf` returns `undefined` for `c`.

- [ ] **Step 3: Write the implementation**

In `web/src/keys.ts`, add `correct` to the `Intent` union and to `intentOf`,
beside the existing `chrome` case:

```ts
  if (event.key === 'c') return { kind: 'correct' }
```

`session.ts`'s `ReadingIntent` already excludes what it must; add
`{ kind: 'correct' }` to that `Exclude<...>` list so `applyIntent` stays
exhaustive over what it actually handles — a correction is not a reading-state
change.

In `web/src/Filmstrip.tsx`, add to `FilmstripProps`:

```ts
  /**
   * Set while the reader is choosing a counterpart. The strip then shows
   * `to`'s tiles rather than the edition being read, with the slot the
   * alignment currently resolves to marked — which is the thing being
   * disagreed with, so it has to be visible to disagree with it.
   */
  correcting?: { to: string; pairedIndex?: number }
  /** A tile click while `correcting` — the assertion, not navigation. */
  onPickCounterpart?: (slot: number) => void
```

and branch the click and the marking:

```tsx
            onClick={() =>
              correcting && onPickCounterpart
                ? onPickCounterpart(group.slots[0])
                : onNavigate(group.slots[0])
            }
```

with a third tile style for the current resolution while correcting:

```tsx
const resolvedTileStyle: CSSProperties = {
  ...tileStyle,
  // Dashed, not solid: the solid border already means "where you are", and a
  // reader choosing a counterpart has to tell "where the alignment says this
  // lands" apart from it at a glance. Same longhand-only rule as above.
  borderColor: '#f2b134',
  borderStyle: 'dashed',
}
```

In `App.tsx`:

- hold `const [pending, setPending] = useState<Pending>()` and
  `const [etag, setEtag] = useState<string>()`;
- load the sidecar through `getSidecarWithEtag` and keep the `etag`;
- on the `correct` intent, set `pending` to
  `{ from: shown.edition, to: <the other edition>, a: slots[shown.slot] }` —
  the counterpart edition being `state.authority` when it differs from
  `shown.edition`, else `previous`;
- render the filmstrip with `correcting={{ to: pending.to, pairedIndex }}`
  where `pairedIndex` is `resolver.indexOf(pending.to, resolver.resolve(pending.from, pending.to, pending.a))`;
- on `onPickCounterpart`, set `pending.b` and show a confirmation line naming
  `displacedBy(pending, resolver)` when it is defined:

```tsx
  // Named before the write, not after it: the 1:1 rule means this slot loses
  // its counterpart, and nothing undoes a correction short of making another.
  const displaced = pending && displacedBy(pending, resolver)
```

- on confirm, call `patchAlignment(work, etag, {from, to, a: spanOf(a), b: pending.b ? spanOf(pending.b) : null})`,
  then rebuild the resolver from the returned document
  (`await wasmResolver(fresh.json)`), store the new `etag`, and clear `pending`;
- on a rejected promise whose message contains `changed since you read it`,
  re-fetch the sidecar, keep `pending`, and tell the reader to confirm again
  rather than silently retrying — a stale validator means somebody else's
  correction is in the document now, and it may be the one they were about to
  make.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npx vitest run && npx tsc --noEmit`
Expected: green, and no type errors.

- [ ] **Step 5: Drive it in a real browser**

This is the step this milestone exists for, and it is not optional — `App.tsx`
is deliberately unreachable from Vitest (see its own comment), so nothing above
covers the wiring.

```bash
./scripts/build-wasm.sh
cargo run -p taiyaku-cli -- serve corpus/gotoubun-v01/taiyaku.json --addr 127.0.0.1:8080
cd web && npm run dev
```

Confirm, and record what you saw in `docs/notes.md` (Task 9):

1. `c` opens correction mode and the strip shows the counterpart edition.
2. The tile the alignment currently resolves to is marked, and it is the one
   the readout's position line agrees with.
3. Picking a different tile names the slot that will be displaced.
4. Confirming writes: `git diff corpus/gotoubun-v01/taiyaku.json` shows a
   `"by": "hand"` run, and the page on screen changes to the picked counterpart.
5. Reloading the page keeps the correction.
6. `Escape` before confirming writes nothing — `git diff` stays empty.
7. With two browser tabs open on the same work, correcting in one and then
   confirming a correction in the other reports the conflict rather than
   silently winning.

- [ ] **Step 6: Commit**

```bash
git add web/src
git commit -m "web: pick a counterpart in the filmstrip and write the correction"
```

---

### Task 9: the documents

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/notes.md`
- Modify: `docs/sidecar.md`

**Closing O13 is a five-part edit, not a tick.** Miss any part and the document
contradicts itself:

- [ ] **Step 1: `docs/status.md` — close O13**

1. The table row's checkbox: `| O13 | [x] |`.
2. The row's site, which must still exist: `sidecar.rs::merge_spoke` does.
3. The prose bullet, rewritten to say what was done — hand runs carry across an
   unchanged spoke, a replaced spoke holding them is refused, a replaced spoke
   without them is unchanged — and what remains: what should become of a
   correction addressing bytes that are gone is still not answered, deliberately.
4. The test counts, which appear **twice** near the top: "Five crates, 313
   tests" and "Nine of the 299 are standing checks". **Both are stale against
   each other today** — 299 was left behind by an earlier close. Set both to the
   figure `cargo test --workspace` actually reports, and the `web/` figure from
   `npx vitest run`.
5. The "Seventeen are now closed … leaving five" sentence, and the "What
   remains … five open items" line near the bottom. The count word there wraps
   across a line break, so grep the sentence opener and read the next line:
   `grep -n -A1 '^What remains:' docs/status.md`.

- [ ] **Step 2: `docs/status.md` — open O23**

The measurement from Task 2, as its own row and bullet. The total count word at
the head of the Open items section ("Twenty-two, each found by…") moves to
twenty-three — that is the one that drifts, because it only changes when an
item is opened while the closed count changes on every close.

```markdown
| O23 | [ ] | Two hand corrections make a sound alignment report `fragmented` — *measured: avg run 64.33 → 32.00 → 21.22 on Go-Toubun v01, against a 27.0 threshold* | `sidecar.rs::rewrite`, `solve.rs::verdict` | M5d |
```

with a bullet recording: each correction adds two paired runs and strands one
slot; the threshold exists to separate needles (1.1–1.7 slots a run) from good
pairings (36.0–200.0), and a twice-corrected volume at 21.2 is neither; nothing
is changed here because altering the fragmentation policy on a guess is what
this project does not do. **What would decide it:** a work corrected enough
times in real reading to say whether the verdict misleads a reader, or a
measurement showing that hand runs and solver runs should be counted
differently.

- [ ] **Step 3: `docs/status.md` — the milestone**

Update "Where it stands" and the M5 section: the reader can now fix a wrong
pairing and keep it. Remove "**No correction UI.**" and "**The reader has no
correction UI.**" from "What is known to be wrong or missing" — both are false
once this lands, and leaving them is the staleness the open-items table exists
to avoid. The "What remains" line drops M5d and keeps M5c.

Add the seventh command's synopsis change if any (there is none — `serve`'s
flags are unchanged), and add the new route to the `serve` paragraph's list of
what it serves.

- [ ] **Step 4: `docs/notes.md`**

The measurements, since `status.md`'s preamble promises every claim there
traces to one here:

- "A point correction is not 1:1 unless the target's previous owner gives it
  up" — the 193-targets-192-distinct figure and what `invert` would have done
  with it.
- "Marking hand runs on any overlapping slot marks 188 of 195 slots" — and why
  `all` is the rule.
- "Two corrections take Go-Toubun v01 from sound to fragmented" — 64.33 →
  32.00 → 21.22, the run and slot counts behind each.
- "Correcting against the stored direction, without inverting" — the gap count
  before and after, and why `invert` cannot be round-tripped.
- The browser pass from Task 8, step 5, item by item.

- [ ] **Step 5: `docs/sidecar.md`**

`by` is documented; what was never documented is what a merge does with it.
Add to the section that defines `by`: a hand-corrected run carries across a
re-solve where the spoke's members are unchanged, and a merge that would
replace the spoke while hand runs exist is refused rather than silently
dropping them. Note that a correction takes its target from whichever slot held
it, so one correction can leave a second slot unpaired.

- [ ] **Step 6: Sweep the document against itself**

```sh
grep -c '^| O[0-9]' docs/status.md                 # expect 23
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # expect 18
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

- [ ] **Step 7: Commit**

```bash
git add docs
git commit -m "docs: close O13, open O23, record what M5d measured"
```

---

## Self-review against the spec

**Spec coverage.** M5d's four named pieces map to tasks: the correction UI
(7, 8), O13 (3), `If-Match` (5, 6), `ETag` (5 — the page `ETag` already shipped
in M5b, so what lands here is the sidecar's). The design's "run-splitting
arithmetic goes in `taiyaku-core`, written test-first" is Tasks 1 and 2. "A
correction on a composed pair writes a direct edge, seeded from `compose()`
with the fix applied" is Task 2's third case, tested. The seam test the design
names — "hand-run preservation: all three merge cases behave as tabled" — is
Task 3's two tests plus the narrowed existing one.

**Not covered here, deliberately.** The design's other three seam tests belong
to M5c (note round-trip) or already shipped in M5b (keybinding collision,
decode-before-swap). The design's "whether corrections need an undo" stays open;
nothing here answers it, and Task 8 mitigates it by naming the displaced slot
before the write rather than after.

**One thing the plan adds that the spec did not ask for:** Task 5's shared
atomic writer. Justified by there being two copies already, both documented as
copies, and this milestone adding a third — but it can be dropped to a
third copy if a reviewer would rather keep the branch to one claim.

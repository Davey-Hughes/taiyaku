# Borrow lender and arrows — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the borrowed page from redrawing the page the reader just left, and give the arrow keys a destination on both `Gap` and `Borrowed`.

**Architecture:** Three stacked branches. First a gated Rust corpus measurement that reproduces the figures in the design (proving the port faithful). Then a one-line change to `lenderFor` that removes the authority fallback, so 228 of 276 borrows revert to `Gap`. Then `applyIntent` gains a branch that takes the nearest offered move when `display` is not `at`, plus a shared offer row so a borrowed page states what it is.

**Tech Stack:** Rust (`taiyaku-cli`, `taiyaku-wasm`), TypeScript/React (`web/`), vitest, biome, cargo nextest.

## Global Constraints

- The design is `docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`. Every figure quoted below comes from its §1.
- **Never run the gate with `TAIYAKU_CORPUS` set.** `scripts/check-all.sh` exports it into an unfiltered `cargo test --workspace`, which runs `geometry_experiment` in the debug profile (40+ minutes). Run `env -u TAIYAKU_CORPUS scripts/check-all.sh`.
- The corpus job runs separately: `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`
- **No attribution in commit messages.** No `Co-Authored-By`, no `Generated with`.
- The remote is **squash-only**: a branch becomes exactly one commit on main, and the PR body *is* that commit message. Keep each branch to one checkable claim.
- PRs go through the **gitea** MCP tools, not `gh`. Never hard-wrap a PR body.
- `web/` uses **pnpm**, not npm. Biome formatting and `tsc -b` are both gate failures.
- Sites in docs are named **by symbol, never by line number**.
- `docs/status.md` owns project state; measurements go in `docs/notes.md`.

## File Structure

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/taiyaku-cli/src/borrow_frequency.rs` | **Create.** Gated corpus measurement: ports `display`/`withBorrow` over `Binding`, sweeps every ordered edition pair, prints the tables. | 1 |
| `crates/taiyaku-cli/src/main.rs` | **Modify.** Add `#[cfg(test)] mod borrow_frequency;` | 1 |
| `crates/taiyaku-cli/Cargo.toml` | **Modify.** Add `taiyaku-wasm` dev-dependency with its reason. | 1 |
| `web/src/modules/borrow.ts` | **Modify.** `lenderFor` drops the authority fallback; two doc blocks rewritten. | 2 |
| `web/src/modules/borrow.test.ts` | **Modify.** Invert the fallback test. | 2 |
| `web/src/session.ts` | **Modify.** `applyIntent`'s `step` case takes the nearest offer; doc rewritten. | 3 |
| `web/src/session.test.ts` | **Modify.** New arrow-on-gap tests. | 3 |
| `web/src/GapOffers.tsx` | **Create.** The offer-button row, shared by `Gap` and `Borrowed`. | 4 |
| `web/src/Gap.tsx` | **Modify.** Use `GapOffers`. | 4 |
| `web/src/Borrowed.tsx` | **Modify.** Use `GapOffers`, add the always-visible line. | 4 |
| `web/src/Borrowed.test.tsx` | **Modify.** Assert the new line and row. | 4 |
| `docs/status.md`, `docs/notes.md` | **Modify.** The entry and the measurement. | 2, 4 |

---

### Task 1: The corpus measurement (branch 1 of 3)

Reproduces the design's §1 figures in Rust, so the numbers the other two tasks move are checkable. **This task changes no behaviour.**

**Files:**
- Create: `crates/taiyaku-cli/src/borrow_frequency.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (module list, near `mod corpus;`)
- Modify: `crates/taiyaku-cli/Cargo.toml` (`[dev-dependencies]`)

**Interfaces:**
- Consumes: `crate::corpus::corpus()` and `crate::corpus::members()` (both `pub(crate)`); `taiyaku_wasm::binding::Binding` with `from_json`, `editions`, `slot_count`, `slot_at`, `index_of`, `resolve`; `taiyaku_core::{Sidecar, Slot, Half}`.
- Produces: nothing other tasks import. Its printed figures are the contract.

- [ ] **Step 1: Add the dev-dependency, with its reason**

In `crates/taiyaku-cli/Cargo.toml`, under `[dev-dependencies]`, add:

```toml
# `taiyaku-wasm` is here for `borrow_frequency.rs` alone — the gap-frequency
# measurement — and a dev-dependency is what keeps it out of the `taiyaku`
# binary while leaving the module inside the workspace, compiled by CI, and
# unable to rot. The same reasoning as `tract-onnx` above.
#
# It is the *reader's own* resolver rather than a second copy: `binding.rs`
# holds the routing logic as ordinary Rust and the wasm layer above it decides
# nothing, so measuring through it measures what the browser does. Only
# `display` and `withBorrow` are ported by hand, and the design's §5 says so.
taiyaku-wasm = { path = "../taiyaku-wasm" }
```

- [ ] **Step 2: Declare the module**

In `crates/taiyaku-cli/src/main.rs`, beside the other gated modules (`mod corpus;` is at the same indentation):

```rust
#[cfg(test)]
mod borrow_frequency;
```

- [ ] **Step 3: Write the failing unit test for the ported logic**

Create `crates/taiyaku-cli/src/borrow_frequency.rs` with only the test first, so the port is exercised on a fixture before it is pointed at 24 GB of archives:

```rust
//! How often a reader meets a gap, and therefore a borrowed page.
//!
//! The design is `docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`.
//! Gated on `TAIYAKU_CORPUS` through `corpus::corpus()` — the *same* gate as
//! `corpus.rs`, deliberately not a second spelling of it, because two
//! spellings is how one ends up skipping in silence (O30).
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli borrow_frequency -- --nocapture
//! ```

use std::collections::BTreeMap;

use taiyaku_core::{Half, Sidecar, Slot};
use taiyaku_wasm::binding::Binding;

use crate::corpus::corpus;

/// What a reader is shown at one position.
#[derive(Debug, PartialEq, Eq)]
enum Shown {
    /// A real page in the edition being viewed, at this slot of it.
    At(u32),
    /// Another edition's page, and whose.
    Borrowed(String),
    /// No page, and nothing to borrow.
    Gap,
}

/// `web/src/reading.ts`'s `display`, ported.
///
/// Answers the slot in `viewing`, or `None` for a gap. The identity branch
/// matches the original: viewing the authority is always `at`, with no
/// resolver call, because the slot is the authority's own.
fn display(b: &Binding, authority: &str, slot: u32, viewing: &str) -> Option<u32> {
    if viewing == authority {
        return Some(slot);
    }
    let from = b.slot_at(authority, slot)?;
    let landed = b.resolve(authority, viewing, from.page, from.half)?;
    b.index_of(
        viewing,
        Slot {
            page: landed.page,
            half: landed.half,
        },
    )
}

/// `web/src/modules/borrow.ts`'s `lenderFor`, ported.
///
/// `order` is `orderedEditions`: the hub first, then the rest alphabetically.
/// The final `Some(authority)` is the fallback this branch's Task 2 removes;
/// it is here so this measurement reproduces the behaviour that shipped.
fn lender_for(b: &Binding, order: &[String], authority: &str, slot: u32, viewing: &str) -> Option<String> {
    let from = b.slot_at(authority, slot)?;
    for edition in order {
        if edition == viewing || edition == authority {
            continue;
        }
        if let Some(landed) = b.resolve(authority, edition, from.page, from.half) {
            if b
                .index_of(
                    edition,
                    Slot {
                        page: landed.page,
                        half: landed.half,
                    },
                )
                .is_some()
            {
                return Some(edition.clone());
            }
        }
    }
    Some(authority.to_string())
}

/// `web/src/modules/borrow.ts`'s `withBorrow`, ported.
fn shown_at(b: &Binding, order: &[String], authority: &str, slot: u32, viewing: &str) -> Shown {
    match display(b, authority, slot, viewing) {
        Some(at) => Shown::At(at),
        None => match lender_for(b, order, authority, slot, viewing) {
            Some(lender) => Shown::Borrowed(lender),
            None => Shown::Gap,
        },
    }
}

/// `web/src/session.ts`'s `orderedEditions`: hub first, then the rest sorted.
fn ordered_editions(hub: &str, editions: &[String]) -> Vec<String> {
    let mut rest: Vec<String> = editions.iter().filter(|e| *e != hub).cloned().collect();
    rest.sort();
    let mut order = vec![hub.to_string()];
    order.extend(rest);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two editions where `b` has no counterpart for the hub's slot 2.
    ///
    /// Hand-written JSON rather than a corpus volume, so the port is checked
    /// without the archives and this test runs in the ordinary gate.
    fn two_editions() -> String {
        r#"{
          "taiyaku": 1,
          "work": { "title": "fixture", "reading": "rtl" },
          "hub": "a",
          "editions": {
            "a": { "label": "a", "members": [{ "path": "a", "sha256": "aa", "pages": 4 }] },
            "b": { "label": "b", "members": [{ "path": "b", "sha256": "bb", "pages": 4 }] }
          },
          "alignments": [{
            "from": "a", "to": "b", "by": "solver",
            "solver": { "hash": "dhash-16", "floor": 0.5 },
            "quality": { "verdict": "sound", "slots": 4, "paired": 3, "avg_run": 3.0 },
            "runs": [{ "a": "0-1", "b": "0-1" }, { "a": "2" }, { "a": "3", "b": "2" }]
          }]
        }"#
        .to_string()
    }

    #[test]
    fn ordered_editions_puts_the_hub_first_then_sorts() {
        let editions = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert_eq!(ordered_editions("b", &editions), vec!["b", "a", "c"]);
    }

    #[test]
    fn a_paired_slot_is_at_and_an_unpaired_one_borrows_the_authority() {
        let json = two_editions();
        let b = Binding::from_json(&json).expect("fixture parses");
        let order = ordered_editions("a", &b.editions());
        // Slot 0 pairs, so the reader sees a real `b` page.
        assert_eq!(shown_at(&b, &order, "a", 0, "b"), Shown::At(0));
        // Slot 2 does not, and with only two editions the authority lends —
        // which is exactly the behaviour Task 2 removes.
        assert_eq!(
            shown_at(&b, &order, "a", 2, "b"),
            Shown::Borrowed("a".to_string())
        );
    }
}
```

- [ ] **Step 4: Get a genuine red state before trusting green**

Rust will not compile a test against a function that does not exist, so the
usual write-test-first cycle cannot produce a red state here. Manufacture one
instead — this step is **not optional**, because a test that has never been
watched failing is a test nobody knows can fail:

1. Replace `ordered_editions`' body with `todo!()`, and `display`'s with
   `todo!()`.
2. Run: `cargo test -p taiyaku-cli borrow_frequency`
3. Expected: **FAIL**, both tests, panicking at `not yet implemented`.
4. Restore both bodies.

- [ ] **Step 5: Run the unit tests to verify they pass**

Run: `cargo test -p taiyaku-cli borrow_frequency`
Expected: PASS, 2 tests.

- [ ] **Step 6: Commit the port**

```bash
git add crates/taiyaku-cli/src/borrow_frequency.rs crates/taiyaku-cli/src/main.rs crates/taiyaku-cli/Cargo.toml
git commit -m "Port display and withBorrow to Rust, checked against a fixture"
```

- [ ] **Step 7: Add the corpus sweep**

Append to `crates/taiyaku-cli/src/borrow_frequency.rs`, outside the `tests` module:

```rust
/// Every corpus volume that carries a sidecar, swept over every ordered pair.
///
/// Reads `taiyaku.json` where it already exists rather than re-solving: the
/// figures this reproduces were taken against the documents `align --write`
/// produces, and re-solving here would measure the solver as well as the
/// reader. A volume with no sidecar is skipped by name so the skip is visible
/// rather than silent.
#[test]
fn how_often_a_reader_meets_a_gap() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    let mut volumes: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.join("taiyaku.json").is_file().then_some(path)
        })
        .collect();
    volumes.sort();
    assert!(
        !volumes.is_empty(),
        "TAIYAKU_CORPUS is set but no volume under it has a taiyaku.json"
    );

    let mut total = 0_u32;
    let mut at = 0_u32;
    let mut borrowed = 0_u32;
    let mut gap = 0_u32;
    let mut from_authority = 0_u32;
    let mut from_third = 0_u32;
    let mut runs: Vec<u32> = Vec::new();

    for volume in &volumes {
        let name = volume.file_name().unwrap_or_default().to_string_lossy().to_string();
        let json = std::fs::read_to_string(volume.join("taiyaku.json")).expect("readable");
        let hub = Sidecar::from_json(&json).expect("parses").hub;
        let b = Binding::from_json(&json).expect("parses");
        let editions = b.editions();
        let order = ordered_editions(&hub, &editions);

        for authority in &editions {
            let Some(count) = b.slot_count(authority) else {
                continue;
            };
            for viewing in &editions {
                if viewing == authority {
                    continue;
                }
                let mut run = 0_u32;
                for slot in 0..count {
                    total += 1;
                    match shown_at(&b, &order, authority, slot, viewing) {
                        Shown::At(_) => {
                            at += 1;
                            if run > 0 {
                                runs.push(run);
                            }
                            run = 0;
                        }
                        Shown::Borrowed(lender) => {
                            borrowed += 1;
                            run += 1;
                            if &lender == authority {
                                from_authority += 1;
                            } else {
                                from_third += 1;
                            }
                        }
                        Shown::Gap => {
                            gap += 1;
                            run += 1;
                        }
                    }
                }
                if run > 0 {
                    runs.push(run);
                }
            }
        }
        eprintln!("{name}: {} editions", editions.len());
    }

    let len1 = runs.iter().filter(|r| **r == 1).count();
    let len4 = runs.iter().filter(|r| **r >= 4).count();
    eprintln!(
        "\npositions {total} | at {at} | borrowed {borrowed} | gap {gap}\n\
         lender: authority {from_authority}, third {from_third}\n\
         runs {} | length 1: {len1} | 4+: {len4} | longest: {}",
        runs.len(),
        runs.iter().max().copied().unwrap_or(0),
    );

    // Structural only. A threshold on a corpus figure is a check that fails
    // when the corpus changes, which is not a measurement — the numbers print
    // for a person to read. What is asserted is that every position landed in
    // exactly one arm, and `borrow.ts`'s own claim that the authority can
    // always lend, which is what Task 2 removes.
    assert_eq!(at + borrowed + gap, total, "every position classified once");
    assert_eq!(gap, 0, "the authority can always lend, before Task 2");
}
```

- [ ] **Step 8: Run the sweep and check it reproduces the design's figures**

Run: `TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli borrow_frequency -- --nocapture --test-threads=1`

Expected: PASS, and the printed line must read exactly:

```
positions 6340 | at 6064 | borrowed 276 | gap 0
lender: authority 228, third 48
runs 104 | length 1: 64 | 4+: 12 | longest: 43
```

**If any figure differs, stop and report it rather than adjusting the assertion** — the port is wrong, or the sidecars differ from the ones measured on 2026-08-13. Twelve volumes must be found; if fewer, build the missing sidecars first (see `docs/notes.md`).

- [ ] **Step 9: Run the full gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: all four jobs pass. The corpus test skips, printing `TAIYAKU_CORPUS unset; skipping`.

- [ ] **Step 10: Commit and open PR 1**

```bash
git add crates/taiyaku-cli/src/borrow_frequency.rs
git commit -m "Sweep every ordered edition pair, and reproduce the figures"
```

Open the PR with the gitea MCP tools against `main`. Body states: 6,340 positions over twelve volumes; 276 borrows of which 228 are the authority lending its own page back; 104 runs, longest 43; no behaviour change.

---

### Task 2: Fix A — the authority stops lending (branch 2 of 3)

Branch from Task 1's branch (`git rebase --onto` if PR 1 lands first).

**Files:**
- Modify: `web/src/modules/borrow.ts` (`lenderFor`'s final return; its doc block; `borrowModule`'s doc)
- Modify: `web/src/App.tsx` (`borrowedView`'s doc comment about the two arms)
- Modify: `web/src/modules/borrow.test.ts` (the fallback test, inverted)
- Modify: `crates/taiyaku-cli/src/borrow_frequency.rs` (drop the same fallback; update the two assertions)
- Modify: `docs/status.md`, `docs/notes.md`

**Interfaces:**
- Consumes: nothing new.
- Produces: `lenderFor` now returns `undefined` when no edition other than the viewed one and the authority resolves. `withBorrow` already passes a `gap` through unchanged on `undefined` — that code does not move.

- [ ] **Step 1: Invert the failing test**

In `web/src/modules/borrow.test.ts`, replace the `falls back to the authority when no third edition resolves` case with:

```ts
  it('lends nothing when no third edition resolves, rather than the authority', () => {
    // The authority's page is already one `Space` away — `applyIntent`'s
    // toggle goes to `state.authority` whenever `viewing !== authority`, which
    // is true of every gap. Lending it drew a page the reader had just left,
    // without arrows, annotations, zoom or prefetch, in place of the `Gap`
    // view that names the missing edition and offers both escapes.
    // 228 of 276 corpus borrows were this. See the 2026-08-13 design, §2.
    const twoEditions = fakeResolver({ a: whole(4), b: whole(4) }, pairs)
    expect(lenderFor(inGap, twoEditions, ['a', 'b'])).toBeUndefined()
  })

  it('still lends a third edition where one resolves', () => {
    expect(lenderFor(inGap, resolver, order)).toEqual({ edition: 'd', slot: 2 })
  })
```

And in the `withBorrow` block, the pass-through must be asserted by identity —
a caller compares against what it passed in, so a structurally-equal copy is a
regression this test has to catch:

```ts
  it('passes the gap straight through when nothing can lend, by identity', () => {
    const twoEditions = fakeResolver({ a: whole(4), b: whole(4) }, pairs)
    const shown = display(inGap, twoEditions)
    expect(withBorrow(shown, inGap, twoEditions, ['a', 'b'], on)).toBe(shown)
  })
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && pnpm vitest run --project node src/modules/borrow.test.ts`
Expected: FAIL — `expected { edition: 'a', slot: 2 } to be undefined`.

- [ ] **Step 3: Make the change**

In `web/src/modules/borrow.ts`, `lenderFor`'s final statement:

```ts
  return undefined
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd web && pnpm vitest run --project node src/modules/borrow.test.ts`
Expected: PASS. Then run the whole node project — `pnpm vitest run --project node` — and fix any other test that asserted the fallback.

- [ ] **Step 5: Rewrite the two doc blocks that describe the removed behaviour**

In `web/src/modules/borrow.ts`, `lenderFor`'s doc currently says "**The authority can always lend**, which is why the fallback needs no resolution". Replace that paragraph with:

```
 * **Nothing lends where no third edition does.** The authority is skipped like
 * the viewed edition, and there is no fallback to it: `applyIntent`'s toggle
 * goes to `state.authority` whenever `viewing !== authority` — true of every
 * gap — so lending it drew a page one keypress away, and drew it without
 * arrows, annotations, zoom or prefetch, in place of the `Gap` view that
 * explains the state. Measured at 228 of 276 corpus borrows before it was
 * removed; see the 2026-08-13 design, §2. A two-edition work therefore never
 * borrows at all, which is most of the corpus.
```

In `borrowModule`'s doc, replace "Defaults on, matching `zoom`, `prefetch` and `help`." with a sentence that also says what it now governs:

```
 * Defaults on, matching `zoom`, `prefetch` and `help`. Since the authority
 * stopped lending it governs only borrowing from a *third* edition, so on a
 * work with two editions it changes nothing — the switch is honest rather
 * than useless, and `Gap` is what a reader sees either way.
```

**A third comment describes the removed fallback and is easy to miss**, because
it is in `App.tsx` rather than in `borrow.ts`. `borrowedView`'s doc says the
non-null assertion "holds for either arm `lenderFor` can return through: a
resolved lender's slot came back from `indexOf`, and the authority fallback
carries the slot `lenderFor` had already proved exists". There is now one arm.
Rewrite it to say so — the assertion is still sound, for the first reason
alone.

- [ ] **Step 6: Make the Rust measurement match**

In `crates/taiyaku-cli/src/borrow_frequency.rs`, `lender_for`'s final statement becomes `None`, its doc loses "it is here so this measurement reproduces the behaviour that shipped", and the sweep's second assertion becomes:

```rust
    assert_eq!(
        from_authority, 0,
        "the authority no longer lends; every borrow is a third edition"
    );
```

- [ ] **Step 7: Run the corpus sweep and check the numbers moved as predicted**

Run: `TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli borrow_frequency -- --nocapture --test-threads=1`

Expected: PASS, and:

```
positions 6340 | at 6064 | borrowed 48 | gap 228
lender: authority 0, third 48
```

`at` must be **unchanged at 6064** — this change moves borrows to gaps and must not affect whether a real page was found. If `at` moved, stop and report.

- [ ] **Step 8: Update the docs**

In `docs/status.md`, the "Fall back to another edition where this one has no page, marked as such" entry under "Later": the paragraph beginning "**The two-edition degeneration, stated rather than hidden.**" now describes removed behaviour. Mark it in place rather than deleting — the project keeps wrong reasoning and says it was wrong — and add a paragraph stating what replaced it, with the 228/48 figures and a pointer to the new design.

In `docs/notes.md`, add a session titled "How often a reader meets a gap, and who actually lends" carrying §1's tables and naming the harness.

- [ ] **Step 9: Run the full gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: all four jobs pass.

- [ ] **Step 10: Commit and open PR 2**

```bash
git add web/src/modules/borrow.ts web/src/modules/borrow.test.ts web/src/App.tsx crates/taiyaku-cli/src/borrow_frequency.rs docs/status.md docs/notes.md
git commit -m "The authority stops lending: 228 of 276 borrows were the page the reader just left"
```

---

### Task 3: Fix B, arrows — the offered move (branch 3 of 3)

**Files:**
- Modify: `web/src/session.ts` (`applyIntent`'s `step` case, and the doc above `applyIntent`)
- Modify: `web/src/session.test.ts`

**Interfaces:**
- Consumes: `nearestPaired(state, resolver, within)` from `./nearest`, returning `{ before?: NearestOffer; after?: NearestOffer }` where `NearestOffer.slot` is authority-space; `takeOffered(state, offered, resolver)` from this same module.
- Produces: `applyIntent` with `intent.kind === 'step'` now returns a moved state when `display` is not `at` and an offer exists on the pressed side.

- [ ] **Step 1: Write the failing tests**

Add to `web/src/session.test.ts`. The fixture must put the reader in a gap with a real page either side:

```ts
describe('applyIntent: an arrow on a gap or a borrowed page', () => {
  // `a` is the authority and hub; `b` has no counterpart for slot 2, so a
  // reader viewing `b` is in a gap there, with real `b` pages at slots 1 and 3.
  const slots = { a: whole(5), b: whole(5) }
  const pairs = { 'a|b|0': 0, 'a|b|1': 1, 'a|b|3': 3, 'a|b|4': 4 }
  const resolver = fakeResolver(slots, pairs)
  const inGap: ReadingState = { authority: 'a', slot: 2, viewing: 'b' }

  it('takes the offer after it on by: 1', () => {
    const { state } = applyIntent(
      { kind: 'step', by: 1 },
      inGap,
      'a',
      resolver,
    )
    // `takeOffered` navigates, so the viewed edition becomes the authority.
    expect(state).toEqual({ authority: 'b', slot: 3, viewing: 'b' })
  })

  it('takes the offer before it on by: -1', () => {
    const { state } = applyIntent(
      { kind: 'step', by: -1 },
      inGap,
      'a',
      resolver,
    )
    expect(state).toEqual({ authority: 'b', slot: 1, viewing: 'b' })
  })

  it('stays put when that side has no offer', () => {
    // Nothing in `b` pairs at all, so neither side offers anything.
    const nothing = fakeResolver({ a: whole(3), b: whole(3) }, {})
    const stuck: ReadingState = { authority: 'a', slot: 1, viewing: 'b' }
    const { state } = applyIntent({ kind: 'step', by: 1 }, stuck, 'a', nothing)
    expect(state).toBe(stuck)
  })

  it('is unchanged on a real page', () => {
    const onPage: ReadingState = { authority: 'a', slot: 0, viewing: 'b' }
    const { state } = applyIntent({ kind: 'step', by: 1 }, onPage, 'a', resolver)
    expect(state).toEqual({ authority: 'b', slot: 1, viewing: 'b' })
  })

  // The spec's §6 asks for this specifically. `applyIntent` takes `by` already
  // rtl-mapped, and nothing inside it could tell if that stopped being true —
  // the arrows would silently invert on a gap and nowhere else. Composing
  // `stepFor` here is what pins the contract without reaching for `App.tsx`.
  it('takes the earlier offer for a physical right arrow in a rtl work', () => {
    const { state } = applyIntent(
      { kind: 'step', by: stepFor(1, 'rtl') },
      inGap,
      'a',
      resolver,
    )
    expect(state).toEqual({ authority: 'b', slot: 1, viewing: 'b' })
  })
})
```

Add `import { stepFor } from './modules/direction'` to the test file.

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && pnpm vitest run --project node src/session.test.ts`
Expected: FAIL — the first two return the state unchanged (`{ authority: 'a', slot: 2, viewing: 'b' }`).

- [ ] **Step 3: Implement**

In `web/src/session.ts`, add the import:

```ts
import { nearestPaired } from './nearest'
```

and replace `case 'step'`'s guard `if (shown.kind !== 'at') return { state, previous }` with:

```ts
      if (shown.kind !== 'at') {
        // No slot in the viewed edition to add `by` to — so the arrow takes
        // the move the screen is already offering instead. `by` arrives
        // rtl-mapped (`App.tsx` runs it through `stepFor`), so `1` is the
        // next slot in reading order and `after` is its offer.
        const within = resolver.slotCount(state.authority)
        if (within === undefined) return { state, previous }
        const { before, after } = nearestPaired(state, resolver, within)
        const offer = intent.by === 1 ? after : before
        if (offer === undefined) return { state, previous }
        return {
          state: takeOffered(state, offer.slot, resolver),
          previous,
        }
      }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd web && pnpm vitest run --project node src/session.test.ts`
Expected: PASS. Then `pnpm vitest run --project node` for the whole project.

- [ ] **Step 5: Rewrite the doc that this reverses**

In `web/src/session.ts`, the `applyIntent` doc contains:

> there is no slot to add `by` to while showing a gap, so arrows are deliberately inert there. The offered moves (`before`/`after`, taken via `takeOffered`) are the way out of one, not a repurposed arrow key.

Replace that paragraph, and the one after it about `modules/borrow.ts`, with a statement of the new rule that records the old one:

```
 * **An arrow on a gap takes the offered move, and this reverses a decision
 * recorded here on purpose.** That decision read: "the offered moves … are the
 * way out of one, *not a repurposed arrow key*," and its reasoning was sound —
 * there is genuinely no slot to add `by` to. What changed is the evidence
 * about its consequence. `modules/borrow.ts` removed the screen that made an
 * inert arrow self-explanatory, and 64 of 104 unpaired runs in the corpus are
 * a single position, so the offer and the next page are the same destination
 * most of the time. The 2026-08-13 design, §3.1, carries the argument.
 *
 * The `display` called below is the *unwrapped* one — `withBorrow` is applied
 * in `App.tsx`, above this — so a borrowed page is a `gap` here and one branch
 * serves both screens, which is what keeps them from answering one key two
 * ways.
```

- [ ] **Step 6: Run the full gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: all four jobs pass.

- [ ] **Step 7: Commit**

```bash
git add web/src/session.ts web/src/session.test.ts
git commit -m "An arrow on a gap takes the offered move, on both screens"
```

---

### Task 4: Fix B, the borrowed page says what it is

Same branch as Task 3.

**Files:**
- Create: `web/src/GapOffers.tsx`
- Modify: `web/src/Gap.tsx`, `web/src/Borrowed.tsx`, `web/src/Borrowed.test.tsx`
- Modify: `docs/status.md`, and the design's status line

**Interfaces:**
- Consumes: `GapMove` from `./Gap`, `Direction` from `./modules/direction`.
- Produces: `GapOffers({ reading, edition, before, after, onTake })`, rendering the two buttons in reading order, or nothing when both are absent.

- [ ] **Step 1: Write the failing component test**

Add to `web/src/Borrowed.test.tsx`. **This file does not use
`@testing-library/react`** — it mounts through `./testing/mount`, and the
existing `props` object is what every case spreads. Extend that object:

```tsx
const props = {
  lender,
  src: '/api/works/w/raw-mono/pages/46',
  fit: fitModule.defaults,
  offers: <p>the offers</p>,
  viewing: 'en-mono',
  reading: 'rtl' as const,
  before: { slot: 97, page: 89 },
  after: { slot: 104, page: 91 },
  onTake: () => {},
}
```

and add:

```tsx
  it('says which edition has no page here, without the mark being pressed', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.textContent).toContain('No en-mono page here')
  })

  it('offers both escapes beside that line, the two the arrows also take', () => {
    const host = mount(<Borrowed {...props} />)
    const labels = [...host.querySelectorAll('button')].map((b) => b.textContent)
    expect(labels.some((l) => l?.includes('en-mono page 89'))).toBe(true)
    expect(labels.some((l) => l?.includes('en-mono page 91'))).toBe(true)
  })
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd web && pnpm vitest run --project jsdom src/Borrowed.test.tsx`
Expected: FAIL — `Borrowed` has no `viewing`, `reading`, `before`, `after` or `onTake` props.

- [ ] **Step 3: Extract the shared offer row**

Create `web/src/GapOffers.tsx`:

```tsx
import type { GapMove } from './Gap'
import type { Direction } from './modules/direction'

export interface GapOffersProps {
  reading: Direction
  /** The edition with no page here — whose pages the offers land on. */
  edition: string
  before?: GapMove
  after?: GapMove
  onTake: (slot: number) => void
}

/**
 * The two moves either side of a gap, in reading order.
 *
 * Shared by `Gap` and `Borrowed` rather than duplicated: they are the same two
 * destinations, and since the arrows take them too (`session.ts`'s `step`) a
 * reader who learns them on one screen must find them identical on the other.
 *
 * The earlier page is on the right in a right-to-left work, so the row sits
 * the way the pages do — the same rule as the filmstrip's rail, and for the
 * same reason: this is a screen a reader reaches by being lost.
 */
export function GapOffers({
  reading,
  edition,
  before,
  after,
  onTake,
}: GapOffersProps) {
  if (before === undefined && after === undefined) {
    return (
      <p style={{ margin: 0, opacity: 0.8 }}>
        No paired page nearby in {edition} either.
      </p>
    )
  }
  return (
    <div
      style={{
        display: 'flex',
        gap: 12,
        flexDirection: reading === 'rtl' ? 'row-reverse' : 'row',
      }}
    >
      {before && (
        <button type="button" onClick={() => onTake(before.slot)}>
          {reading === 'rtl' ? '→' : '←'} {edition} page {before.page}
        </button>
      )}
      {after && (
        <button type="button" onClick={() => onTake(after.slot)}>
          {edition} page {after.page} {reading === 'rtl' ? '←' : '→'}
        </button>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Use it from `Gap`**

In `web/src/Gap.tsx`, export `GapMove` (it already is), import `GapOffers`, and replace the whole `{before === undefined && after === undefined ? … : …}` expression with:

```tsx
      <GapOffers
        reading={reading}
        edition={edition}
        before={before}
        after={after}
        onTake={onTake}
      />
```

- [ ] **Step 5: Use it from `Borrowed`, with the line**

In `web/src/Borrowed.tsx`, add to `BorrowedProps`:

```ts
  /** The edition being viewed — the one with no page here. */
  viewing: string
  reading: Direction
  before?: GapMove
  after?: GapMove
  onTake: (slot: number) => void
```

and render, inside the non-revealed branch's outer `div`, below `PageView`:

```tsx
      <div
        style={{
          position: 'absolute',
          bottom: 0,
          left: 0,
          right: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 8,
          padding: 8,
          background: 'rgba(0, 0, 0, 0.55)',
          color: '#f2f2f2',
        }}
      >
        {/* The mark alone read as a caption, which the previous design named
            as this feature's real cost. This says the situation in words and
            puts both escapes beside it, the same two the arrows now take. */}
        <p style={{ margin: 0 }}>No {viewing} page here.</p>
        <GapOffers
          reading={reading}
          edition={viewing}
          before={before}
          after={after}
          onTake={onTake}
        />
      </div>
```

- [ ] **Step 6: Pass the new props from `App.tsx`**

`App.tsx`'s `borrowedView` builds the `<Borrowed …>` element, and the `gapView`
immediately above it already has every value needed — `effectiveReading`,
`toMove(gapMoves?.before)`, `toMove(gapMoves?.after)` and `take`. Add these five
props to the `<Borrowed>` element, matching `gapView`'s expressions exactly so
the two cannot drift:

```tsx
        viewing={edition}
        reading={effectiveReading}
        before={toMove(gapMoves?.before)}
        after={toMove(gapMoves?.after)}
        onTake={take}
```

`edition` is `borrowedView`'s own first parameter — the edition being viewed,
not the lender, which is what `Borrowed`'s `viewing` prop means.

- [ ] **Step 7: Run to verify it passes**

Run: `cd web && pnpm vitest run --project jsdom src/Borrowed.test.tsx`
Expected: PASS. Then `pnpm vitest run` for every project.

- [ ] **Step 8: Update the docs and the design's status line**

`docs/status.md`: the gap-fallback entry's paragraphs "**And the arrow keys do nothing**" and "**That is the feature's real cost…**" both describe removed behaviour. Mark them in place and add what replaced them.

In `docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`, change the status line to `**Status: settled in brainstorming on 2026-08-13. Built, and shipped on `borrow-lender-and-arrows`.**`

- [ ] **Step 9: Run the full gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: all four jobs pass.

- [ ] **Step 10: Commit and open PR 3**

```bash
git add web/src/GapOffers.tsx web/src/Gap.tsx web/src/Borrowed.tsx web/src/Borrowed.test.tsx web/src/App.tsx docs/status.md docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md
git commit -m "The borrowed page says what it is, and shares Gap's two escapes"
```

---

## Verification before calling this done

- [ ] `env -u TAIYAKU_CORPUS scripts/check-all.sh` passes on the final branch.
- [ ] `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1` passes, printing `borrowed 48 | gap 228`.
- [ ] Drive it in a browser against `gotoubun-v01`, which is the only corpus material where a third edition can lend: reach a borrowed page, confirm the line and both buttons are visible without pressing the mark, and confirm `←`/`→` move rather than doing nothing.
- [ ] Grep `docs/` and `web/src/` for `borrow` and read every hit — the open-items ritual's rule that a sweep for expected phrasing misses prose that never used it.

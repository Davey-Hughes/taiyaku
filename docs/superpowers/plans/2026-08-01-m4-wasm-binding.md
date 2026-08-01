# M4 — `resolve_slot`, the WASM binding, and the agreement test — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve a *slot* — not just a page — between two editions, and expose that from JavaScript through a `taiyaku-wasm` crate whose answers are checked against the native ones.

**Architecture:** `Route::resolve_slot(Slot) -> Option<Slot>` becomes the primitive and `Route::resolve(page)` becomes a call to it with the half left off, so there is one implementation rather than two. A new `taiyaku-wasm` crate wraps `taiyaku-core` for JS. Its logic lives in a plain-Rust `binding` module that compiles and tests natively; `#[wasm_bindgen]` types are a thin shell over it, so the behaviour under test is the behaviour that ships.

**Tech Stack:** Rust 2024, `wasm-bindgen`, `wasm32-unknown-unknown`.

**Source spec:** [`docs/superpowers/specs/2026-07-30-reader-design.md`](../specs/2026-07-30-reader-design.md) — "The WASM binding", "What `taiyaku-core` gains", and the M4 row of the milestone table.

## Global Constraints

- **Rust edition 2024**, workspace-wide in `Cargo.toml`.
- **CI's first gate is `cargo fmt --all -- --check`**, before clippy or tests. **The snippets below are not guaranteed rustfmt-clean.** Run `cargo fmt --all` after pasting any snippet, and `cargo fmt --all -- --check` before every commit. Reformatting a snippet is expected and is not a deviation; changing an assertion, a message or a value is.
- **CI runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`.** Any warning fails the build, `unused_imports` included.
- **`unsafe_code = "forbid"`, `missing_debug_implementations = "warn"`.** Every public type needs `Debug`.
- **`taiyaku-core` must keep building for `wasm32-unknown-unknown`.** Task 1 is the only task touching it and adds pure arithmetic only. **`wasm-bindgen` goes in `taiyaku-wasm`, never in `taiyaku-core`** — the core's public API must not be shaped by what can cross the JavaScript boundary, which is the spec's own stated constraint.
- **British spelling in prose and doc comments** (`behaviour`, `canonicalise`), matching the existing docs.
- **No attribution lines in commit messages.** No `Co-Authored-By`, no `Generated with`.
- **Sites in docs are named by symbol, never by line number.**
- **Test baseline is 189**, measured on 2026-08-01 at commit `9db9a1d`: `taiyaku-core` 62, `taiyaku-read` 10, `taiyaku-serve` 93, `taiyaku-cli` 24. Each task states the expected total after it.

| After | core | read | serve | cli | wasm | total |
| ----- | ---- | ---- | ----- | --- | ---- | ----- |
| baseline | 62 | 10 | 93 | 24 | — | **189** |
| Task 1 | 65 | 10 | 93 | 24 | — | **192** |
| Task 2 | 65 | 10 | 93 | 24 | 4 | **196** |
| Task 3 | 65 | 10 | 93 | 24 | 5 | **197** |
| Task 4 | 65 | 10 | 93 | 24 | 7 | **199** |

## Scope boundary

**In:** `resolve_slot`, the `taiyaku-wasm` crate, the binding/native agreement test, and extending the wasm gate to cover the new crate.

**Out, deliberately:**

- **The reader itself.** M5. No `web/`, no React, no HTML.
- **The round-trip test** (swap A→B→A returns to the starting slot). The spec lists it, but it is a property of the *authority model*, which lives in the reader. It has nothing to hold onto until M5 exists.
- **Correction, `by: "hand"`, and O13.** M6.
- **Running the binding under an actual wasm runtime.** See Task 3's note — this plan gates that the crate *builds* for wasm32 and tests its logic natively. Executing it under Node via `wasm-pack test --node` would need Node in CI and is recorded as a follow-up rather than built.

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-core/src/route.rs` | **Modify.** `resolve_slot` becomes the primitive; `resolve` delegates to it. |
| `crates/taiyaku-wasm/Cargo.toml` | **Create.** The binding crate's manifest. `wasm-bindgen` lives here and nowhere else. |
| `crates/taiyaku-wasm/src/binding.rs` | **Create.** Plain Rust: parse a sidecar, list editions, resolve a slot. No `wasm_bindgen`. All tests target this. |
| `crates/taiyaku-wasm/src/lib.rs` | **Create.** `#[wasm_bindgen]` shell over `binding`. Type conversion only, no logic. |
| `Cargo.toml` | **Modify.** Add `crates/taiyaku-wasm` to `members`. |
| `scripts/check-all.sh` | **Modify.** The wasm job builds the binding crate too. |
| `.forgejo/workflows/ci.yml` | **Modify.** Same, so local and CI cannot drift. |

---

### Task 1: `Route::resolve_slot` in `taiyaku-core`

**Files:**
- Modify: `crates/taiyaku-core/src/route.rs` — `impl Route`, and its `mod tests`

**Interfaces:**
- Consumes: `Slot { page: u32, half: Option<Half> }`, `Half::{Left, Right}`, `crate::sidecar::slot_index`, `crate::counterpart` — all already exist.
- Produces: `Route::resolve_slot(&self, slot: Slot) -> Option<Slot>`. Task 2 calls this.

**Why this shape:** `Route::resolve` already converts a page to a slot index by asking `slot_index` for `Slot { page, half: None }`, and `slot_index` treats a missing half as "the first slot of that page". So the general operation is already inside the specific one — this exposes it rather than adding anything.

- [ ] **Step 1: Write the failing tests**

Add these three to `mod tests` in `crates/taiyaku-core/src/route.rs`. `Half` and `Slot` are already imported there via `use crate::{Half, PageRange};` and `use super::*;` — add `Slot` to the import if clippy reports it missing.

```rust
    /// A slot with no half names the *first* half of its page in reading
    /// order. That is the rule which makes `resolve` correct as a call to
    /// `resolve_slot`, and it is a real choice rather than a formality: under
    /// `rtl` the right half is the earlier page, so en-mono's spread at page 4
    /// must answer for `4R`, and `4R` and `4L` do not agree here.
    ///
    /// **Revised after review.** This slot was first written as a loop
    /// asserting `resolve(page) == resolve_slot(Slot { page, half: None })`,
    /// which cannot fail: Step 3 defines `resolve` as exactly that call, so
    /// both sides were the same pure call on an immutable `Route`. The
    /// refactor is guarded instead by the pre-existing
    /// `a_page_resolves_through_a_stored_sidecar`, which pins concrete values
    /// from the aligner's own output.
    #[test]
    fn a_slot_with_no_half_names_the_first_half_in_reading_order() {
        let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");
        let r = route(&s, "en-mono", "raw-mono").expect("a route exists");

        let whole = r.resolve_slot(Slot {
            page: 4,
            half: None,
        });
        let right = r.resolve_slot(Slot {
            page: 4,
            half: Some(Half::Right),
        });
        let left = r.resolve_slot(Slot {
            page: 4,
            half: Some(Half::Left),
        });

        assert_eq!(
            whole, right,
            "rtl makes the right half the earlier page, so it is the one a \
             bare page reference names"
        );
        assert_ne!(
            whole, left,
            "and the halves disagree here, which is what stops this asserting \
             a formality"
        );
    }

    /// The reason `resolve_slot` exists. `en-mono` page 4 is a joined spread,
    /// and the stored run `"a": "2-5", "b": "1-4R"` ends on its *right* half —
    /// so read backwards, `4R` has a counterpart and `4L` falls between two
    /// runs and has none. A reader stopped on the left half that swapped to
    /// "page 4" would be shown the right half's counterpart and silently move.
    #[test]
    fn the_halves_of_a_spread_resolve_separately() {
        let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");
        let r = route(&s, "en-mono", "raw-mono").expect("a route exists");

        assert_eq!(
            r.resolve_slot(Slot {
                page: 4,
                half: Some(Half::Right)
            }),
            Some(Slot {
                page: 5,
                half: None
            }),
            "the right half ends run `1-4R`, so it pairs with raw 5"
        );
        assert_eq!(
            r.resolve_slot(Slot {
                page: 4,
                half: Some(Half::Left)
            }),
            None,
            "the left half sits between two runs and pairs with nothing"
        );
    }

    /// A slot naming a page the edition does not have is `None`, not a clamp
    /// and not a panic — the same choice `Edition::locate` makes.
    #[test]
    fn a_slot_past_the_end_resolves_to_nothing() {
        let s = Sidecar::from_json(crate::sidecar::REAL).expect("parses");
        let r = route(&s, "raw-mono", "en-mono").expect("a route exists");
        let past = s
            .editions
            .get("raw-mono")
            .expect("the fixture's hub")
            .total_pages();
        assert_eq!(r.resolve_slot(Slot { page: past, half: None }), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core resolve_slot 2>&1 | tail -20`

Expected: FAIL to compile, with `no method named 'resolve_slot' found for struct 'Route'`.

- [ ] **Step 3: Add `resolve_slot` and route `resolve` through it**

In `crates/taiyaku-core/src/route.rs`, replace the whole existing `resolve` method inside `impl Route` with these two:

```rust
    /// Where a slot of the `from` edition lands.
    ///
    /// A reader stopped on the left half of a joined spread has to swap to the
    /// counterpart of *that half*. Resolving its page instead names the first
    /// half, which is a different slot and, where a run ends mid-spread, a
    /// different answer — so a reader doing that moves a page without being
    /// asked to. `notes.md`, "swapping must not round-trip through the
    /// alignment", is the same failure reached by a different route.
    pub fn resolve_slot(&self, slot: Slot) -> Option<Slot> {
        let here = crate::sidecar::slot_index(&self.from_slots, slot)? as u32;
        let there = counterpart(&self.runs, here)?;
        self.to_slots.get(there as usize).copied()
    }

    /// Where a page of the `from` edition lands.
    ///
    /// A page reference with no half names the first slot of that page, which
    /// is what makes `47` mean the same thing whether page 47 is whole or a
    /// spread. The answer carries a half because that is what a reader has to
    /// display — possibly one side of a joined spread.
    pub fn resolve(&self, page: u32) -> Option<Slot> {
        self.resolve_slot(Slot { page, half: None })
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core 2>&1 | grep "test result"`

Expected: `test result: ok. 65 passed`.

- [ ] **Step 5: Confirm nothing else moved, and the wasm target still builds**

Run: `cargo test --workspace --all-features 2>&1 | grep "test result"`

Expected: four lines summing to 192 — `65`, `10`, `93`, `24`.

Run: `cargo build -p taiyaku-core --target wasm32-unknown-unknown --all-features`

Expected: `Finished`.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo fmt --all -- --check
git add crates/taiyaku-core/src/route.rs
git commit -m "Resolve a slot, not only a page, so a spread's halves swap apart"
```

---

### Task 2: The `taiyaku-wasm` binding, as plain Rust

**Files:**
- Create: `crates/taiyaku-wasm/Cargo.toml`
- Create: `crates/taiyaku-wasm/src/binding.rs`
- Create: `crates/taiyaku-wasm/src/lib.rs` (module wiring only in this task; the `#[wasm_bindgen]` shell arrives in Task 3)
- Modify: `Cargo.toml` — workspace `members`

**Interfaces:**
- Consumes: `taiyaku_core::{Sidecar, Slot, Half, Via, route}` and `Route::resolve_slot` from Task 1.
- Produces: `binding::Binding::{from_json, editions, slot_count, resolve}` and `binding::Resolved { page, half, via }`. Task 3's shell wraps these; Task 4's tests compare them against native calls.

**Why a plain module under the shell:** `#[wasm_bindgen]` types are awkward to unit-test natively and the macro's expansion is not the thing worth testing. Keeping the logic in ordinary Rust means `cargo test` exercises exactly the code that ships, and the shell stays small enough to read.

- [ ] **Step 1: Create the manifest**

Create `crates/taiyaku-wasm/Cargo.toml`:

```toml
[package]
name = "taiyaku-wasm"
description = "WebAssembly binding over taiyaku-core, for a browser reader"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

# This crate exists so that `wasm-bindgen` lives here and not in taiyaku-core,
# whose manifest carries a standing warning about dependencies that must keep
# the wasm target building. The logic is in `binding.rs` as plain Rust; the
# `wasm_bindgen` types in `lib.rs` are a shell over it.
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
taiyaku-core = { path = "../taiyaku-core" }
wasm-bindgen = "0.2"

[lints]
workspace = true
```

- [ ] **Step 2: Add the crate to the workspace**

In the root `Cargo.toml`, change the `members` line to include the new crate:

```toml
members = ["crates/taiyaku-core", "crates/taiyaku-read", "crates/taiyaku-serve", "crates/taiyaku-cli", "crates/taiyaku-wasm"]
```

- [ ] **Step 3: Write the failing tests**

Create `crates/taiyaku-wasm/src/binding.rs` containing **only** the test module for now, so the tests fail on missing items rather than on a missing file:

```rust
//! The binding's logic, as ordinary Rust.
//!
//! Kept apart from the `wasm_bindgen` shell in `lib.rs` so that `cargo test`
//! exercises the code that ships rather than a native stand-in for it.

#[cfg(test)]
mod tests {
    use super::*;

    /// Two editions, one spread, one run that ends mid-spread — the smallest
    /// document that can tell a half-aware resolve from a page-aware one.
    ///
    /// **The run sides must be equal in slots, not pages.** `b`'s page 2 is a
    /// spread, so `"0-2R"` is three slots (`0`, `1`, `2R`) and the `a` side
    /// has to be three as well; `taiyaku-core`'s `runs_of` refuses an unequal
    /// pair and `route` then answers `None`. That is why `a` starts at 1 —
    /// which leaves `a`'s slot 0 deliberately unpaired, and the agreement
    /// check in Task 4 is stronger for having an unpaired slot in range.
    const FIXTURE: &str = r#"{
      "taiyaku": 1,
      "work": { "title": "T", "reading": "rtl" },
      "editions": {
        "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 8 }] },
        "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 8 }], "spreads": [2] }
      },
      "hub": "a",
      "alignments": [
        {
          "from": "a", "to": "b", "by": "solver",
          "runs": [{ "a": "1-3", "b": "0-2R" }, { "a": "5-7", "b": "5-7" }]
        }
      ]
    }"#;

    #[test]
    fn a_sidecar_parses_and_lists_its_editions() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        let mut names = b.editions();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    /// Refusal names the problem. A binding that answered `None` for malformed
    /// input would make "no such work" and "this file is broken" the same
    /// answer, and the browser is the worst place to tell them apart.
    #[test]
    fn malformed_json_is_refused_with_a_message() {
        let refused = Binding::from_json("{ not json").expect_err("must not invent a document");
        assert!(
            refused.contains("not a readable taiyaku.json"),
            "the refusal says what kind of file it failed to read: {refused}"
        );
        assert!(
            refused.contains("line") && refused.contains("column"),
            "and where — serde locates the fault, and a binding that dropped \
             that would leave a browser with nothing to act on: {refused}"
        );
    }

    /// Slot count, not page count: `b` has 8 pages of which one is a joined
    /// spread, so it presents 9 slots. A reader sizing a filmstrip on pages
    /// would come up one short on every spread.
    #[test]
    fn slot_count_counts_slots_rather_than_pages() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        assert_eq!(b.slot_count("a"), Some(8));
        assert_eq!(b.slot_count("b"), Some(9));
        assert_eq!(b.slot_count("nosuch"), None);
    }

    /// The half travels in and out, and `via` says how the answer was found.
    #[test]
    fn a_slot_resolves_with_its_half_and_reports_the_path() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        let got = b.resolve("a", "b", 3, None).expect("a pairs with b directly");
        assert_eq!(got.page, 2);
        assert_eq!(got.half, Some(Half::Right), "run `1-3` ends on b's `2R`");
        assert_eq!(got.via, "direct");
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-wasm 2>&1 | tail -20`

Expected: FAIL to compile, with `cannot find type 'Binding' in this scope` (and `Half` unresolved).

- [ ] **Step 5: Write the implementation**

Insert this **above** the `#[cfg(test)] mod tests` block in `crates/taiyaku-wasm/src/binding.rs`, keeping the module doc comment at the top of the file:

```rust
use taiyaku_core::{Half, Sidecar, Slot, Via, route};

/// A parsed sidecar, held so a reader can ask it many questions cheaply.
#[derive(Debug, Clone)]
pub struct Binding {
    sidecar: Sidecar,
}

/// Where a slot landed, and how the answer was found.
///
/// `via` travels with the answer because a composed correspondence is weaker
/// than a stored one — `route.rs` reports it for the same reason, and a reader
/// that cannot tell them apart has been told less than the document knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub page: u32,
    pub half: Option<Half>,
    pub via: String,
}

impl Binding {
    /// Parse a sidecar from its JSON text.
    pub fn from_json(json: &str) -> Result<Self, String> {
        Ok(Self {
            sidecar: Sidecar::from_json(json)?,
        })
    }

    /// Every edition this document defines, unordered.
    pub fn editions(&self) -> Vec<String> {
        self.sidecar.editions.keys().cloned().collect()
    }

    /// How many slots an edition presents, or `None` if it has no such edition.
    ///
    /// Slots rather than pages, because a joined spread is two of them and the
    /// slot is the addressed unit everywhere else in the format.
    pub fn slot_count(&self, edition: &str) -> Option<u32> {
        let reading = self.sidecar.work.reading;
        Some(self.sidecar.editions.get(edition)?.slots(reading).len() as u32)
    }

    /// Resolve one slot of `from` into `to`.
    ///
    /// `None` covers three different situations on purpose — no such edition,
    /// no path between them, and no counterpart for this slot. The shell in
    /// `lib.rs` passes that through as `undefined`; a reader distinguishing
    /// them would ask `slot_count` first.
    pub fn resolve(
        &self,
        from: &str,
        to: &str,
        page: u32,
        half: Option<Half>,
    ) -> Option<Resolved> {
        let r = route(&self.sidecar, from, to)?;
        let landed = r.resolve_slot(Slot { page, half })?;
        Some(Resolved {
            page: landed.page,
            half: landed.half,
            via: via_word(&r.via).to_string(),
        })
    }
}

/// A stable lowercase name per path, for a JS caller to branch on.
///
/// Spelled out rather than derived from `Debug`, which is a formatting detail
/// nothing promises to keep.
fn via_word(via: &Via) -> &'static str {
    match via {
        Via::Identity => "identity",
        Via::Direct => "direct",
        Via::Inverted => "inverted",
        Via::Hub(_) => "hub",
    }
}
```

Create `crates/taiyaku-wasm/src/lib.rs` with the module wiring only:

```rust
//! WebAssembly binding over `taiyaku-core`.
//!
//! `binding` holds the logic as ordinary Rust and is what the tests exercise.
//! The `wasm_bindgen` shell over it arrives with the browser reader.

pub mod binding;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-wasm 2>&1 | grep "test result"`

Expected: `test result: ok. 4 passed`.

If `a_slot_resolves_with_its_half_and_reports_the_path` fails on the `half` assertion, the fixture's reading direction is doing its job — `rtl` puts the right half of page 2 first, so run `1-3` ↔ `0-2R` ends there. Do not change the assertion to match the output; work out which is wrong first.

- [ ] **Step 7: Run the whole gate**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: no warnings. `crate-type = ["cdylib", "rlib"]` with no `wasm_bindgen` use yet may draw an unused-dependency note from some toolchains; if clippy flags `wasm-bindgen` as unused, leave the dependency in place — Task 3 uses it — and confirm the warning is not `-D`-fatal before continuing.

Run: `cargo test --workspace --all-features 2>&1 | grep "test result"`

Expected: totals summing to 196.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
cargo fmt --all -- --check
git add Cargo.toml crates/taiyaku-wasm
git commit -m "Give the core a binding crate, with its logic in ordinary Rust"
```

---

### Task 3: The `#[wasm_bindgen]` shell, and the gate that covers it

**Files:**
- Modify: `crates/taiyaku-wasm/src/lib.rs`
- Modify: `scripts/check-all.sh` — the `wasm` job section
- Modify: `.forgejo/workflows/ci.yml` — the `wasm` job

**Interfaces:**
- Consumes: `binding::{Binding, Resolved}` from Task 2.
- Produces: `WasmSidecar` with `from_json`, `editions`, `slot_count`, `resolve_slot`. Nothing in this repo consumes it; the reader does, in M5.

**Why the half crosses as a string:** `Option<Half>` is a Rust enum inside an `Option`, which `wasm_bindgen` cannot pass directly. `"L"` / `"R"` / `undefined` is the smallest thing that survives the boundary and reads correctly from JS.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-wasm/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The shell converts and does not decide. This pins the one piece of
    /// judgement it does carry — how a half crosses the language boundary —
    /// because a silent disagreement about `"L"` would show a reader the wrong
    /// half of every spread.
    #[test]
    fn a_half_crosses_the_boundary_as_l_or_r() {
        assert_eq!(half_from_str(None), Ok(None));
        assert_eq!(half_from_str(Some("L")), Ok(Some(taiyaku_core::Half::Left)));
        assert_eq!(half_from_str(Some("R")), Ok(Some(taiyaku_core::Half::Right)));
        assert_eq!(half_to_str(Some(taiyaku_core::Half::Left)), Some("L".to_string()));
        assert_eq!(half_to_str(Some(taiyaku_core::Half::Right)), Some("R".to_string()));
        assert_eq!(half_to_str(None), None);
        assert!(
            half_from_str(Some("middle")).is_err(),
            "an unknown half is refused rather than silently read as a whole page"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p taiyaku-wasm half_crosses 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'half_from_str' in this scope`.

- [ ] **Step 3: Write the shell**

Replace the contents of `crates/taiyaku-wasm/src/lib.rs` above the test module with:

```rust
//! WebAssembly binding over `taiyaku-core`.
//!
//! `binding` holds the logic as ordinary Rust and is what the tests exercise.
//! Everything here converts types across the JS boundary and decides nothing,
//! so that the behaviour under test is the behaviour that ships.

pub mod binding;

use taiyaku_core::Half;
use wasm_bindgen::prelude::*;

/// A parsed sidecar, as held by a JS caller.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmSidecar {
    inner: binding::Binding,
}

/// Where a slot landed. `half` is `"L"`, `"R"`, or absent for a whole page.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct WasmSlot {
    pub page: u32,
    pub half: Option<String>,
    pub via: String,
}

#[wasm_bindgen]
impl WasmSidecar {
    /// Parse a sidecar from its JSON text, throwing on malformed input.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<WasmSidecar, JsError> {
        binding::Binding::from_json(json)
            .map(|inner| WasmSidecar { inner })
            .map_err(|e| JsError::new(&e))
    }

    /// Every edition this document defines.
    pub fn editions(&self) -> Vec<String> {
        self.inner.editions()
    }

    /// How many slots an edition presents, or `undefined` if it has no such
    /// edition.
    #[wasm_bindgen(js_name = slotCount)]
    pub fn slot_count(&self, edition: &str) -> Option<u32> {
        self.inner.slot_count(edition)
    }

    /// Resolve one slot of `from` into `to`, or `undefined` when there is no
    /// counterpart, no path, or no such edition.
    #[wasm_bindgen(js_name = resolveSlot)]
    pub fn resolve_slot(
        &self,
        from: &str,
        to: &str,
        page: u32,
        half: Option<String>,
    ) -> Result<Option<WasmSlot>, JsError> {
        let half = half_from_str(half.as_deref()).map_err(|e| JsError::new(&e))?;
        Ok(self
            .inner
            .resolve(from, to, page, half)
            .map(|r| WasmSlot {
                page: r.page,
                half: half_to_str(r.half),
                via: r.via,
            }))
    }
}

/// `"L"` / `"R"` / absent, into a half.
///
/// An unrecognised string is refused rather than read as a whole page: a typo
/// that quietly means "the first half" would show the wrong side of every
/// spread and look like an alignment bug.
fn half_from_str(half: Option<&str>) -> Result<Option<Half>, String> {
    match half {
        None => Ok(None),
        Some("L") => Ok(Some(Half::Left)),
        Some("R") => Ok(Some(Half::Right)),
        Some(other) => Err(format!("`{other}` is not a half; use `L`, `R`, or nothing")),
    }
}

/// A half, back into `"L"` / `"R"` / absent.
fn half_to_str(half: Option<Half>) -> Option<String> {
    half.map(|h| match h {
        Half::Left => "L".to_string(),
        Half::Right => "R".to_string(),
    })
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p taiyaku-wasm 2>&1 | grep "test result"`

Expected: `test result: ok. 5 passed`.

- [ ] **Step 5: Verify the crate builds for wasm32**

Run: `cargo build -p taiyaku-wasm --target wasm32-unknown-unknown --all-features`

Expected: `Finished`. This is the check that matters most in this task — a `wasm_bindgen` signature that does not survive the boundary fails here and nowhere else.

- [ ] **Step 6: Extend the gate to cover the new crate**

In `scripts/check-all.sh`, find the `wasm` job section (it begins with the comment `# --- the \`wasm\` job ---`) and add a second build beside the existing `taiyaku-core` one, keeping it inside the same `if [ "$no_wasm" -eq 0 ]` guard:

```bash
  run cargo build -p taiyaku-wasm --target wasm32-unknown-unknown --all-features
```

Update that section's comment to say why there are now two:

```bash
# taiyaku-core is built for wasm32 in CI, so a change that compiles natively but not for wasm (a
# stray std-only dependency, most often) fails there and not here. Reproduced rather than trusted.
# taiyaku-wasm is built too, and for a second reason: a `wasm_bindgen` signature that cannot cross
# the boundary compiles natively and fails only for this target.
```

In `.forgejo/workflows/ci.yml`, add a step to the `wasm` job directly after `Build core for wasm`:

```yaml
      - name: Build the binding for wasm
        run: cargo build -p taiyaku-wasm --target wasm32-unknown-unknown --all-features
```

- [ ] **Step 7: Run the gate as CI will**

Run: `scripts/check-all.sh`

Expected: every stage passes, and the tail shows both wasm builds. Totals sum to 197.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
cargo fmt --all -- --check
git add crates/taiyaku-wasm/src/lib.rs scripts/check-all.sh .forgejo/workflows/ci.yml
git commit -m "Cross the JS boundary, and gate the crossing for the wasm target"
```

---

### Task 4: The agreement test

**Files:**
- Modify: `crates/taiyaku-wasm/src/binding.rs` — `mod tests`

**Interfaces:**
- Consumes: `binding::Binding` (Task 2), `taiyaku_core::route` and `Route::resolve_slot` (Task 1).
- Produces: nothing. This task adds checks only.

**What it catches, stated honestly:** the spec calls this "WASM/native agreement" and names the risk as *drift in the binding, which would make "one implementation" false*. The drift it can actually catch is the binding's own conversion logic diverging from the core — a half dropped, a slot index off by one, a `via` mislabelled. It does **not** prove the compiled wasm behaves as the native build does; that would need the module executed under Node via `wasm-pack test --node`, which means Node in CI. Recorded as a follow-up rather than built, because the named risk is the conversion layer and this reaches it.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/taiyaku-wasm/src/binding.rs`:

```rust
    /// Every slot of every ordered pair, through the binding and through the
    /// core, must agree.
    ///
    /// **What this catches, and what it does not.** `Binding::resolve` calls
    /// `route` and `Route::resolve_slot` and copies the answer into
    /// `Resolved`, so both sides of this comparison reach the same core
    /// functions: a bug in spread arithmetic or run composition would be wrong
    /// identically on both sides and this would stay green. What it does catch
    /// is drift in the binding's own plumbing — a transposed `from`/`to`, a
    /// dropped half, a wrong field copied into `Resolved`. It is not a value
    /// compared with itself: the two sides parse the document separately and
    /// build their own `Route`.
    ///
    /// Exhaustive rather than sampled because the bug it *can* catch is a
    /// single slot answering differently, which sampling finds only by luck.
    #[test]
    fn the_binding_agrees_with_the_core_on_every_slot() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        let s = taiyaku_core::Sidecar::from_json(FIXTURE).expect("parses");
        let reading = s.work.reading;

        let mut compared = 0;
        for from in b.editions() {
            for to in b.editions() {
                let Some(r) = taiyaku_core::route(&s, &from, &to) else {
                    continue;
                };
                for slot in s.editions[&from].slots(reading) {
                    let native = r.resolve_slot(slot);
                    let bound = b.resolve(&from, &to, slot.page, slot.half);
                    assert_eq!(
                        native.map(|n| (n.page, n.half)),
                        bound.map(|x| (x.page, x.half)),
                        "{from} -> {to}, slot {slot:?} disagrees"
                    );
                    compared += 1;
                }
            }
        }
        // 8 slots in `a` and 9 in `b` — `b`'s page 2 is a spread — across four
        // ordered pairs, all of which have a route in a two-edition star.
        // Pinned rather than merely non-zero: an edit that narrowed the loop to
        // a subset of pairs would still compare *something* and stay green
        // while quietly dropping the "every ordered pair" claim above.
        assert_eq!(
            compared, 34,
            "the fixture's geometry changed, or the loop stopped covering it"
        );
    }

    /// The paths a two-edition star can produce, named the way JS will branch
    /// on them. `Via::Hub` needs three editions, so it is not reachable from
    /// this fixture and is not asserted here.
    ///
    /// **Slot 1, not slot 0, for the `a -> b` case.** The fixture's first run
    /// starts at `a`'s slot 1, so slot 0 is unpaired and `a -> b` answers
    /// `None` for it — `via` is a property of the path, but this call can only
    /// report one when the slot actually lands somewhere.
    #[test]
    fn each_path_reports_its_own_word() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        assert_eq!(b.resolve("a", "a", 0, None).expect("identity").via, "identity");
        assert_eq!(b.resolve("a", "b", 1, None).expect("stored").via, "direct");
        assert_eq!(b.resolve("b", "a", 0, None).expect("reversed").via, "inverted");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-wasm agrees 2>&1 | tail -20`

Expected: FAIL to compile — `Binding::resolve` takes `Option<Half>` and `slot.half` is already that, so the likely first error is an unresolved `taiyaku_core` path in the test module. Add `use taiyaku_core::Sidecar;` to the test module's imports if so, and re-run to reach a real assertion failure or a pass.

- [ ] **Step 3: Make them pass**

If Task 2 was implemented as written, these pass without new code — they are checks over existing behaviour, and a task whose implementation step is empty is the correct outcome for an agreement test.

**If either fails, do not adjust the assertion.** A disagreement here means the binding and the core genuinely differ, which is the entire thing this task exists to detect. Find which one is wrong before touching either.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-wasm 2>&1 | grep "test result"`

Expected: `test result: ok. 7 passed`.

- [ ] **Step 5: Run the full gate**

Run: `scripts/check-all.sh`

Expected: totals summing to 199, both wasm builds green.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo fmt --all -- --check
git add crates/taiyaku-wasm/src/binding.rs
git commit -m "Check the binding against the core on every slot of every pair"
```

---

### Task 5: Record what M4 changed

**Files:**
- Modify: `docs/status.md`

**Interfaces:** none.

- [ ] **Step 1: Update the crate and test counts**

In `docs/status.md`, the "What is built" section opens `Four crates, 189 tests`. Change it to `Five crates, 199 tests`, and update the two following references — `Three of the 189` becomes `Three of the 199`.

Add a row to the crate table, after `taiyaku-serve`:

```markdown
| `taiyaku-wasm` | the binding a browser reader loads: parse a sidecar, resolve a slot between editions, and say which path the answer walked |
```

- [ ] **Step 2: Say what shipped in the milestone narrative**

In the "3. The reader" section, after the paragraph beginning `**M1, M2 and M3 have landed.**`, add:

```markdown
**M4 has landed, and it is the smallest of them.** `Route::resolve_slot` makes
the slot rather than the page the thing a reader resolves, which matters exactly
where a run ends mid-spread: in the primary sidecar, `en-mono`'s page 4 is a
joined spread whose right half pairs with raw 5 and whose left half pairs with
nothing, so a reader resolving the page instead of the half would show one and
call it the other. `taiyaku-wasm` puts that in the browser's reach without
`taiyaku-core` taking a `wasm-bindgen` dependency, and the gate now builds both
for `wasm32-unknown-unknown` — the binding for a second reason, since a
signature that cannot cross the JS boundary compiles natively and fails only
there. What is **not** claimed: nothing has executed the module under a wasm
runtime. The agreement check compares the binding against the core on every slot
of every ordered pair, which catches conversion drift and not codegen.
```

- [ ] **Step 3: Verify the document does not contradict itself**

Run: `grep -n "189\|199\|Four crates\|Five crates" docs/status.md`

Expected: no remaining `189`, no remaining `Four crates`.

- [ ] **Step 4: Commit**

```bash
git add docs/status.md
git commit -m "Record M4: five crates, and a slot is what a reader resolves"
```

---

## Self-review

**Spec coverage.** The spec's M4 row is "`Route::resolve_slot`, `taiyaku-wasm` + agreement test" — Tasks 1, 2/3 and 4 respectively. The spec's repo layout names `taiyaku-wasm` as "wasm-bindgen over core, emits a TS package"; **emitting the TS package is not in this plan**, because nothing consumes it until `web/` exists in M5 and `wasm-pack` would be a toolchain added for no current caller. Task 3's `js_name` attributes are what make that emission correct when it happens.

**One contradiction inside the source spec, resolved here.** Its "Resolution runs in the browser, in WASM" section says the cost is that "`taiyaku-core` takes `wasm-bindgen`, its first dependency beyond `serde`". Its repo layout says `taiyaku-core` is `unchanged` and lists `taiyaku-wasm` as the crate holding the binding. **The layout is followed**, because the core's API must stay free of JS-boundary types. The prose overstated the cost.

**Type consistency.** `Binding::resolve` returns `Option<Resolved>` throughout; `WasmSidecar::resolve_slot` returns `Result<Option<WasmSlot>, JsError>` because an unrecognised half is a caller error rather than an absent answer. `half_from_str` / `half_to_str` are named identically in Task 3's test and implementation. `Route::resolve_slot` takes `Slot` and returns `Option<Slot>` in Tasks 1, 2 and 4 alike.

**Deliberately absent:** any `web/`, React, HTML or CSS; the round-trip test; anything touching `by: "hand"` or O13.

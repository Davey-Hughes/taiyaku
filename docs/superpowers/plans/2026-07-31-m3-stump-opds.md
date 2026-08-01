# M3 — `StumpLibrary`, the composite source, and OPDS-PSE — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve a taiyaku work whose editions are split between a Stump instance and local disk, and publish those editions as one work over OPDS 1.2 + PSE.

**Architecture:** `PageSource` narrows from "page N of an edition" to "page N of *this member*", with the index → (member, offset) arithmetic moving into `taiyaku_core::Edition`. `StumpLibrary` resolves a member to a Stump book id once (GraphQL), caches the answer beside the sidecar, and fetches bytes over REST. `CompositeSource` routes per member: Stump where it knows the file, local disk where it does not.

**Tech Stack:** Rust 2024, axum 0.8, tokio, `ureq` 3.3 (blocking HTTP), `sha2`, `image` 0.25, hand-written Atom/XML.

**Source spec:** [`docs/superpowers/specs/2026-07-31-m3-stump-opds-design.md`](../specs/2026-07-31-m3-stump-opds-design.md)

## Global Constraints

- **Rust edition 2024, `rust-version = "1.85"`.** Workspace-wide, in `Cargo.toml`.
- **CI's first gate is `cargo fmt --all -- --check`**, before clippy or tests. **The code snippets in this plan are not all rustfmt-clean** — Task 1's test fixture was not, and shipped a failing gate that the task review had no reason to catch. **Run `cargo fmt --all` after pasting any snippet from this plan, and `cargo fmt --all -- --check` before every commit.** Formatting-only changes to a snippet are expected and are not a deviation from the brief; changing an assertion, a message or a value is.
- **CI runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`.** Any warning is a build failure, including `unused_imports`. This bit M2 twice.
- **`unsafe_code = "forbid"`, `missing_debug_implementations = "warn"`.** Every public type needs `Debug`.
- **`taiyaku-core` must build for `wasm32-unknown-unknown`.** It takes no network, filesystem, or `wasm-bindgen` types. Task 1 is the only task touching it, and adds pure arithmetic only.
- **Page index bases:** taiyaku is **0-based**. Stump is **1-based**. OPDS-PSE is **1-based**. Exactly two conversion sites exist (Task 3 and Task 6), each with a test.
- **British spelling in prose and doc comments** (`behaviour`, `canonicalise`, `summarise`), matching the existing docs.
- **No attribution lines in commit messages.** No `Co-Authored-By`, no `Generated with`.
- **Gated tests print their skip reason to stderr and return**, matching `crates/taiyaku-cli/src/corpus.rs`.
- **Test baseline is 97**, measured on 2026-07-31 at commit `e82b2e0`: `taiyaku-core` 59, `taiyaku-read` 10, `taiyaku-serve` 10, `taiyaku-cli` 18. Every task below states the expected total after it.

| After | core | read | serve | cli | total |
| ----- | ---- | ---- | ----- | --- | ----- |
| baseline | 59 | 10 | 10 | 18 | **97** |
| Task 1 | 62 | 10 | 10 | 18 | **100** |
| Task 2 | 62 | 10 | 11 | 18 | **101** |
| Task 3 | 62 | 10 | 17 | 18 | **107** |
| Task 4 | 62 | 10 | 23 | 18 | **113** |
| Task 5 | 62 | 10 | 26 | 18 | **116** |
| Task 6 | 62 | 10 | 31 | 18 | **121** |
| Task 7 | 62 | 10 | 34 | 18 | **124** |
| Task 8 | 62 | 10 | 36 | 18 | **126** |
| Task 9 | 62 | 10 | 37 | 18 | **127** |

> **On the expected test counts.** They are arithmetic over this plan's own snippets and nothing else. **If your measured count differs, that is a finding — report it rather than quietly matching the plan.** The M1/M2 plan's counts were wrong three separate times, each caught by an implementer who measured instead of assuming; the table above exists so the chain is followable rather than merely correct. This plan's own first draft was wrong here too, by three from Task 2 onward.
>
> Task 2 is the only task with a **net** count: it replaces one test with two. Task 9's single test is gated on a reachable Stump and counts as one whether it measures 187 pages or skips.

## File structure

| File | Responsibility | Task |
| ---- | -------------- | ---- |
| `crates/taiyaku-core/src/sidecar.rs` | *modify* — `Edition::total_pages`, `Edition::locate` | 1 |
| `crates/taiyaku-serve/src/source.rs` | *modify* — narrow `PageSource` to one member | 2 |
| `crates/taiyaku-serve/src/lib.rs` | *modify* — `ServeState::page` does the locating | 2 |
| `crates/taiyaku-serve/src/stump/mod.rs` | `StumpLibrary`, its `PageSource` impl, re-exports | 3, 5 |
| `crates/taiyaku-serve/src/stump/http.rs` | `Transport` trait, `UreqTransport`, the fake used in tests | 3 |
| `crates/taiyaku-serve/src/stump/api.rs` | GraphQL queries and the REST page fetch, typed | 3 |
| `crates/taiyaku-serve/src/stump/resolve.rs` | member → book id, matched then confirmed by page hash | 4 |
| `crates/taiyaku-serve/src/stump/cache.rs` | the `taiyaku.stump.json` sibling file | 4 |
| `crates/taiyaku-serve/src/composite.rs` | `CompositeSource` — per-member routing | 5 |
| `crates/taiyaku-serve/src/opds.rs` | OPDS 1.2 + PSE feeds, and XML escaping | 6 |
| `crates/taiyaku-serve/src/cover.rs` | cover generation and its on-disk cache | 7 |
| `crates/taiyaku-serve/src/reqlog.rs` | request-log middleware | 8 |
| `crates/taiyaku-serve/src/corpus.rs` | *modify* — add the equivalence check | 2, 9 |
| `crates/taiyaku-cli/src/main.rs` | *modify* — `--stump-url`, `--stump-key` | 5 |
| `scripts/check-all.sh` | *modify* — make a skipped corpus check visible | 9 |

---

### Task 1: `Edition::total_pages` and `Edition::locate`

The index → (member, offset) walk currently lives inside `LocalLibrary`, and the page-count sum is computed there *and* in `Edition::slots` — two derivations of a normative quantity, one crate apart. Both move here, where `StumpLibrary`, `CompositeSource` and M4's WASM filmstrip all reach them.

**Files:**
- Modify: `crates/taiyaku-core/src/sidecar.rs:154-163` (the `impl Edition` block)
- Test: `crates/taiyaku-core/src/sidecar.rs` (the `mod tests` block at line 619)

**Interfaces:**
- Consumes: `Member { path: String, sha256: String, pages: u32 }`, `Edition { label, members, spreads }` — both already exist in this file.
- Produces: `Edition::total_pages(&self) -> u32` and `Edition::locate(&self, index: u32) -> Option<(usize, u32)>`, returning `(member index, offset within that member)`. Tasks 2, 5, 6 and 7 call both.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `crates/taiyaku-core/src/sidecar.rs`:

```rust
    fn two_member_edition() -> Edition {
        Edition {
            label: "Raw".into(),
            members: vec![
                Member { path: "c1.cbz".into(), sha256: "x".into(), pages: 2 },
                Member { path: "c2.cbz".into(), sha256: "y".into(), pages: 3 },
            ],
            spreads: vec![],
        }
    }

    /// An edition is its members concatenated in array order — `sidecar.md`
    /// rule 1. Getting this wrong shows the wrong page for everything after
    /// the first member while every hash and count in the file still matches,
    /// which is why the arithmetic belongs in one place rather than in each
    /// consumer.
    #[test]
    fn a_page_index_locates_the_member_that_holds_it() {
        let edition = two_member_edition();

        assert_eq!(edition.total_pages(), 5);
        assert_eq!(edition.locate(0), Some((0, 0)));
        assert_eq!(edition.locate(1), Some((0, 1)));
        assert_eq!(edition.locate(2), Some((1, 0)), "the boundary");
        assert_eq!(edition.locate(4), Some((1, 2)), "the last page");
        assert_eq!(edition.locate(5), None, "past the end must not wrap");
        assert_eq!(edition.locate(u32::MAX), None);
    }

    /// An edition with no members has no pages and locates nothing, rather
    /// than panicking or reporting member 0.
    #[test]
    fn an_edition_with_no_members_locates_nothing() {
        let empty = Edition { label: "E".into(), members: vec![], spreads: vec![] };

        assert_eq!(empty.total_pages(), 0);
        assert_eq!(empty.locate(0), None);
    }

    /// A member recorded with zero pages is skipped rather than swallowing an
    /// index. `align` cannot currently write one, but a hand-edited sidecar
    /// can, and returning `(1, 0)` here would serve the wrong member's first
    /// page for the rest of the edition.
    #[test]
    fn a_zero_page_member_is_skipped_rather_than_matched() {
        let edition = Edition {
            label: "E".into(),
            members: vec![
                Member { path: "a.cbz".into(), sha256: "x".into(), pages: 0 },
                Member { path: "b.cbz".into(), sha256: "y".into(), pages: 2 },
            ],
            spreads: vec![],
        };

        assert_eq!(edition.total_pages(), 2);
        assert_eq!(edition.locate(0), Some((1, 0)), "must skip the empty member");
        assert_eq!(edition.locate(1), Some((1, 1)));
        assert_eq!(edition.locate(2), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core locate 2>&1 | tail -20`

Expected: FAIL to compile, `no method named 'total_pages' found for struct 'Edition'`.

- [ ] **Step 3: Implement, and remove the duplicate sum**

Replace the whole `impl Edition` block at `crates/taiyaku-core/src/sidecar.rs:154-163` with:

```rust
impl Edition {
    /// How many pages this edition presents, from its members' recorded counts.
    ///
    /// Derived here rather than by each consumer because it is normative —
    /// `docs/sidecar.md` rule 1 — and a server that sums it differently
    /// disagrees with the aligner about where every member after the first
    /// begins, while still matching every `sha256` in the file.
    pub fn total_pages(&self) -> u32 {
        self.members.iter().map(|m| m.pages).sum()
    }

    /// Which member holds page `index`, and how far into it that page is.
    ///
    /// `None` past the end, rather than wrapping or clamping: a reader asking
    /// for a page that does not exist wants to be told so.
    ///
    /// Members recorded with zero pages are stepped over, so a hand-edited
    /// sidecar cannot make one of them absorb an index.
    pub fn locate(&self, index: u32) -> Option<(usize, u32)> {
        let mut offset = 0u32;
        for (position, member) in self.members.iter().enumerate() {
            if index < offset + member.pages {
                return Some((position, index - offset));
            }
            offset += member.pages;
        }
        None
    }

    /// The slot sequence this edition presents to an alignment.
    ///
    /// Reconstructed from the page count and the spread list rather than
    /// stored, because storing it would be a second source of truth that could
    /// disagree with the first.
    pub fn slots(&self, reading: Direction) -> Vec<Slot> {
        let pages = self.total_pages();
        let wide: Vec<bool> = (0..pages).map(|p| self.spreads.contains(&p)).collect();
        crate::expand(&wide, reading)
    }
}
```

Note `slots` now calls `total_pages()` rather than summing again. That is the second derivation the spec named, removed.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core 2>&1 | rg 'test result'`

Expected: PASS, **62 tests** (59 + 3).

- [ ] **Step 5: Verify the wasm target still builds**

Run: `cargo build -p taiyaku-core --target wasm32-unknown-unknown --all-features`

Expected: success. This is pure arithmetic over owned data, so it cannot fail — the check is here because the constraint is easy to forget and cheap to confirm.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-core/src/sidecar.rs
git commit -m "Put an edition's page arithmetic where every consumer can reach it

The index to (member, offset) walk lived inside LocalLibrary, and the page
count was summed there as well as in Edition::slots — two derivations of a
quantity docs/sidecar.md makes normative, one crate apart.

total_pages and locate move to Edition, slots stops summing separately, and
M3's second PageSource inherits the answer rather than writing the loop again."
```

---

### Task 2: Narrow `PageSource` to one member

`CompositeSource` routes per member and cannot be built against a trait that takes a flat edition index and walks members inside the implementation. This is the reshaping the spec predicted.

**Files:**
- Modify: `crates/taiyaku-serve/src/source.rs` (the trait, `LocalLibrary`, and its tests)
- Modify: `crates/taiyaku-serve/src/lib.rs` (add `ServeState::page`)
- Modify: `crates/taiyaku-serve/src/routes.rs:71-86` (`get_page`)
- Modify: `crates/taiyaku-serve/src/corpus.rs:44-58` (the existing standing check)

**Interfaces:**
- Consumes: `Edition::total_pages`, `Edition::locate` from Task 1.
- Produces: `trait PageSource { fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String>; }` and `ServeState::page(&self, work: &Work, edition: &str, index: u32) -> Result<ServedPage, String>`. Tasks 3, 5, 6, 7 and 9 use both.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/source.rs`, **replace** the existing `a_page_index_walks_across_member_boundaries` test with:

```rust
    /// The source is handed one member and an offset inside it; deciding
    /// which member that is belongs to `taiyaku_core::Edition`. This test
    /// pins the narrowed contract: no edition key, no flat index, no
    /// arithmetic below this line.
    #[test]
    fn a_source_serves_one_member_at_an_offset_within_it() {
        let work = crate::test_support::work("taiyaku-test-source");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions.get("raw").unwrap();

        let first = LocalLibrary
            .page(&root, &edition.members[0], 0)
            .unwrap();
        let second_member = LocalLibrary
            .page(&root, &edition.members[1], 0)
            .unwrap();
        let past_end = LocalLibrary.page(&root, &edition.members[1], 9);

        crate::test_support::cleanup("taiyaku-test-source");
        assert_eq!(first.bytes, b"a1.jpg");
        assert_eq!(
            second_member.bytes, b"b1.jpg",
            "the second member's own first page, not the edition's"
        );
        assert!(past_end.is_err(), "past a member's end must not wrap");
        assert_eq!(
            first.content_type, "image/jpeg",
            "a page carries what it is, not what the route guesses"
        );
    }

    /// The arithmetic the source no longer does still has to happen, and it
    /// has to give the same answers: page 2 of this two-member edition is the
    /// second member's first page. This is the same property the old test
    /// asserted, now checked one level up.
    #[test]
    fn state_walks_a_flat_index_across_member_boundaries() {
        let state = crate::test_support::state("taiyaku-test-state-page");
        let work = state.works[0].clone();

        let count = work.sidecar.editions.get("raw").unwrap().total_pages();
        let first = state.page(&work, "raw", 0).unwrap();
        let across = state.page(&work, "raw", 2).unwrap();
        let last = state.page(&work, "raw", 3).unwrap();
        let past_end = state.page(&work, "raw", 4);
        let no_edition = state.page(&work, "nope", 0);

        crate::test_support::cleanup("taiyaku-test-state-page");
        assert_eq!(count, 4);
        assert_eq!(first.bytes, b"a1.jpg");
        assert_eq!(across.bytes, b"b1.jpg", "page 2 is the second member's first");
        assert_eq!(last.bytes, b"b2.jpg");

        let past_end = past_end.expect_err("past the end must not silently wrap");
        assert!(past_end.contains('4'), "must say how many pages there are: {past_end}");

        let no_edition = no_edition.expect_err("an unknown edition is an error");
        assert!(no_edition.contains("nope"), "must name the edition: {no_edition}");
    }
```

The second test also closes one of the three untested error paths `status.md` lists — "an unknown edition through `LocalLibrary`".

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve 2>&1 | tail -20`

Expected: FAIL to compile — `this method takes 3 arguments but 4 arguments were supplied`, and `no method named 'page' found for struct 'ServeState'`.

- [ ] **Step 3: Narrow the trait and `LocalLibrary`**

In `crates/taiyaku-serve/src/source.rs`, replace the `PageSource` trait and the `impl PageSource for LocalLibrary` block with:

```rust
/// Where a reader's page images come from.
///
/// One member at a time, deliberately. Which member holds page N of an
/// edition is `taiyaku_core::Edition::locate`, and it is the same answer for
/// every source — so a source that computed it would be a second derivation of
/// a normative rule. What genuinely varies below this line is only where one
/// member's bytes live, which is why that is all this trait asks for.
pub trait PageSource: Send + Sync + std::fmt::Debug {
    /// One page of one member, by its offset within that member.
    ///
    /// `root` is the directory the sidecar sits in; a member's `path` is
    /// relative to it — `docs/sidecar.md`.
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String>;
}

/// Pages read from the filesystem, relative to the sidecar.
#[derive(Debug, Clone, Copy)]
pub struct LocalLibrary;

impl PageSource for LocalLibrary {
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String> {
        let path = root.join(&member.path);
        let page = taiyaku_read::member_page(&path, within).map_err(|e| {
            format!(
                "{e} — the sidecar records {} pages for this member; it may have been replaced",
                member.pages
            )
        })?;
        Ok(ServedPage {
            content_type: content_type(&page.name, &page.bytes).to_string(),
            bytes: page.bytes,
        })
    }
}
```

Update the imports at the top of `source.rs`:

```rust
use std::path::Path;

use taiyaku_core::Member;
```

The `use crate::works::Work;` import is now unused — **remove it**, or clippy fails the build under `-D warnings`.

- [ ] **Step 4: Add `ServeState::page`**

In `crates/taiyaku-serve/src/lib.rs`, replace the `impl ServeState` block with:

```rust
impl ServeState {
    pub fn work(&self, id: &str) -> Option<&Work> {
        self.works.iter().find(|w| w.id == id)
    }

    /// One page of an edition, by its index in that edition's flat sequence.
    ///
    /// This is where a flat index becomes a member and an offset, once, for
    /// every source and every route — the API, the OPDS stream and the cover
    /// all arrive here. The failure messages name what was missing, because a
    /// reader shown a blank page with no explanation is the failure this
    /// project exists to avoid.
    pub fn page(&self, work: &Work, edition: &str, index: u32) -> Result<ServedPage, String> {
        let found = work
            .sidecar
            .editions
            .get(edition)
            .ok_or_else(|| format!("no edition `{edition}` in {}", work.path.display()))?;
        let (position, within) = found.locate(index).ok_or_else(|| {
            format!(
                "`{edition}` has {} pages; there is no page {index}",
                found.total_pages()
            )
        })?;
        let root = work.path.parent().unwrap_or(std::path::Path::new("."));
        self.source
            .page(root, &found.members[position], within)
            .map_err(|e| format!("{e} ({})", work.path.display()))
    }
}
```

Add `ServedPage` to the existing `pub use source::{...}` line so this signature resolves:

```rust
pub use source::{LocalLibrary, PageSource, ServedPage};
```

(It is already exported — confirm rather than duplicate.)

- [ ] **Step 5: Point the route at it**

In `crates/taiyaku-serve/src/routes.rs`, replace the body of `get_page`'s match with a call to the new method:

```rust
async fn get_page(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition, index)): AxPath<(String, String, u32)>,
) -> Response {
    let found = match work(&state, &id) {
        Ok(found) => found,
        Err(missing) => return missing.into_response(),
    };
    match state.page(found, &edition, index) {
        // The type the source reported, not a guess: an `<img>` would sniff
        // octet-stream happily, but a direct navigation downloads it and an
        // OPDS client refuses it.
        Ok(page) => ([(header::CONTENT_TYPE, page.content_type)], page.bytes).into_response(),
        Err(e) => not_found(e),
    }
}
```

- [ ] **Step 6: Update the standing corpus check**

In `crates/taiyaku-serve/src/corpus.rs`, the check calls `LocalLibrary.page(work, key, index)`. Replace the imports and the comparison loop:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use crate::{LocalLibrary, ServeState, load_works};
```

and, after `let work = &works[0];`, build a state to go through:

```rust
    let dir = sidecar.parent().unwrap();
    let state = ServeState {
        works: works.clone(),
        source: Arc::new(LocalLibrary),
    };
    let work = &state.works[0];
```

then inside the inner loop replace the `LocalLibrary.page(...)` call with:

```rust
            let served = state
                .page(work, key, index as u32)
                .unwrap_or_else(|e| panic!("{key} page {index}: {e}"));
```

Delete the now-unused `use crate::PageSource;` if present — `-D warnings` will tell you.

- [ ] **Step 7: Run the whole suite**

Run: `cargo test --workspace --all-features 2>&1 | rg 'test result|^error'`

Expected: PASS, **101 total** — 97 baseline, +3 from Task 1, +1 net here (one test replaced by two). `taiyaku-serve` goes 10 → 11.

Then: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: clean. If it reports `unused_imports`, remove the import rather than allowing it.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-serve/src/source.rs crates/taiyaku-serve/src/lib.rs \
        crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/corpus.rs
git commit -m "Ask a source for one member's page, not an edition's

CompositeSource routes per member, and a trait taking a flat edition index
walks the members inside the implementation — so the composite could not be
built against the old shape at all. The deferral recorded while planning M3
was a prerequisite rather than a nicety.

PageSource now takes (root, member, within). ServeState::page does the
locating once, through Edition::locate, for every route that will need it —
the API, the OPDS stream and the cover. The unknown-edition path gains the
test status.md listed as missing."
```

---

### Task 3: The Stump transport and its API calls

> **Executed, then amended. The code blocks below are the original brief and no longer match what shipped.** Task 3 landed at `238c5c6`; its review found four Important defects in these snippets, and the fix wave at `2a9ccff` corrected them. The **Interfaces** block above and the Type consistency section at the end of this plan are accurate; these step snippets are kept as the record of what was asked for. What changed: `FakeTransport` now records request bodies and supports a queued reply form, `page_bytes` takes `zero_based` and converts internally, `find_book` confirms its single candidate against the name, size and page count it already fetched, and `mod stump;` carries `#[expect(dead_code)]` rather than `#[allow]`. **Do not re-derive this task from the snippets below.**

Two protocols, split the way Stump splits them: GraphQL to ask which book a file is, REST to fetch a page. Everything goes through a `Transport` so the tests need no network.

**Files:**
- Create: `crates/taiyaku-serve/src/stump/mod.rs`
- Create: `crates/taiyaku-serve/src/stump/http.rs`
- Create: `crates/taiyaku-serve/src/stump/api.rs`
- Modify: `crates/taiyaku-serve/Cargo.toml`
- Modify: `crates/taiyaku-serve/src/lib.rs` (declare `mod stump;`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `trait Transport { fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String>; fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String>; }` — `get_bytes` returns `(status, content_type, bytes)`.
  - `UreqTransport::new(base: String, key: String) -> UreqTransport`
  - `api::Book { id: String, name: String, path: String, size: i64, pages: u32 }`
  - `api::find_book(t: &dyn Transport, stem: &str, size: u64, pages: u32) -> Result<Option<Book>, String>`
  - `api::page_bytes(t: &dyn Transport, id: &str, zero_based: u32) -> Result<crate::ServedPage, String>` — **takes taiyaku's 0-based index and does the `+1` itself.** Callers never convert.
  - `stump::http::test_support::FakeTransport` (behind `#[cfg(test)]`) — Tasks 4 and 5 use it. It records every request body and answers repeated calls to one path from a queue, so a test can assert *what was sent* and can describe a work where one member is in Stump and another is not.

- [ ] **Step 1: Add the dependencies**

In `crates/taiyaku-serve/Cargo.toml`, add to `[dependencies]`:

```toml
sha2 = "0.11.0"
ureq = "3.3.0"
```

`ureq` is blocking, which is what a synchronous `PageSource` wants; the boundary to async is `spawn_blocking` at the router in Task 5. Its default features include `rustls`, so an https Stump works without further configuration.

Run: `cargo build -p taiyaku-serve`

Expected: success, `ureq v3.3.0` compiled.

- [ ] **Step 2: Write the failing test**

Create `crates/taiyaku-serve/src/stump/http.rs`:

```rust
//! Reaching a Stump instance, and the seam that lets tests not.
//!
//! Every call goes through `Transport`, so the whole of `stump` is testable
//! against recorded responses and the suite does not depend on the NAS being
//! up. The one test that talks to a real instance is the gated equivalence
//! check.

use std::time::Duration;

/// One Stump instance, reachable.
///
/// The base URL is configured and never taken from a response. Stump's own
/// `ImageRef.url` emits its internally configured address — measured as
/// `http://stump.daveynet.xyz:10801/…` on an instance served publicly over
/// https — so believing it would send every page fetch somewhere unreachable.
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// POST a JSON document to a path below the base URL, and read the reply.
    fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String>;

    /// GET a path below the base URL as bytes, with its status and type.
    ///
    /// The status is returned rather than raised because a 404 from a page
    /// route is an answer — "this book has no page N" — and the caller says
    /// so better than a transport error could.
    fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String>;
}
```

Then add the test module at the bottom of the same file:

```rust
#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::Transport;

    /// A `Transport` that answers from a table instead of a network.
    ///
    /// Records the requests it received, so a test can assert that a lookup
    /// was made once and cached rather than repeated on every page turn.
    #[derive(Debug, Default)]
    pub(crate) struct FakeTransport {
        pub posts: HashMap<String, String>,
        pub gets: HashMap<String, (u16, String, Vec<u8>)>,
        pub seen: Mutex<Vec<String>>,
    }

    impl FakeTransport {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// Answer any POST to `path` with `reply`.
        pub(crate) fn on_post(mut self, path: &str, reply: &str) -> Self {
            self.posts.insert(path.to_string(), reply.to_string());
            self
        }

        pub(crate) fn on_get(mut self, path: &str, ct: &str, bytes: &[u8]) -> Self {
            self.gets
                .insert(path.to_string(), (200, ct.to_string(), bytes.to_vec()));
            self
        }

        pub(crate) fn on_get_status(mut self, path: &str, status: u16) -> Self {
            self.gets
                .insert(path.to_string(), (status, String::new(), Vec::new()));
            self
        }

        pub(crate) fn requests(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    impl Transport for FakeTransport {
        fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
            self.seen.lock().unwrap().push(format!("POST {path}"));
            match self.posts.get(path) {
                Some(reply) => Ok(reply.clone().into_bytes()),
                None => Err(format!("fake has no reply for POST {path} with {body}")),
            }
        }

        fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String> {
            self.seen.lock().unwrap().push(format!("GET {path}"));
            match self.gets.get(path) {
                Some((status, ct, bytes)) => Ok((*status, ct.clone(), bytes.clone())),
                None => Err(format!("fake has no reply for GET {path}")),
            }
        }
    }
}
```

Now create `crates/taiyaku-serve/src/stump/api.rs` with only its tests, so they fail:

```rust
//! What taiyaku asks a Stump instance, and what it makes of the answers.

use serde::Deserialize;

use crate::ServedPage;
use crate::stump::http::Transport;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stump::http::test_support::FakeTransport;

    /// The candidate query the design settled on: name, size and page count in
    /// one filter rather than three round trips or a client-side scan.
    ///
    /// `Media.name` is the file **stem** — measured on the live instance as
    /// `Go-Toubun no Hanayome - v01 (2018)`, with no `.cbz`. Sending the full
    /// basename matches nothing, every time, which is a failure that looks
    /// exactly like "Stump does not have this file".
    #[test]
    fn a_book_is_found_by_stem_size_and_page_count() {
        let reply = r#"{"data":{"media":{"nodes":[
            {"id":"a8ed51d8","name":"Go-Toubun no Hanayome - v01 (2018)",
             "path":"/data/Manga/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2018).cbz",
             "size":334752036,"pages":187}
        ]}}}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let found = find_book(&fake, "Go-Toubun no Hanayome - v01 (2018)", 334_752_036, 187)
            .unwrap()
            .expect("the fixture holds one match");

        assert_eq!(found.id, "a8ed51d8");
        assert_eq!(found.pages, 187);
        assert_eq!(found.size, 334_752_036);
    }

    /// A member Stump does not have is not an error. It is the ordinary case
    /// for every raw scan in this library, and the composite falls through to
    /// local disk on it — so it must be `Ok(None)`, not `Err`.
    #[test]
    fn a_member_stump_does_not_have_is_absent_rather_than_an_error() {
        let fake = FakeTransport::new().on_post("/api/graphql", r#"{"data":{"media":{"nodes":[]}}}"#);

        assert_eq!(find_book(&fake, "Go-Toubun no Hanayome - v01", 34_527_438, 195).unwrap(), None);
    }

    /// Two books matching one member is ambiguous, and guessing which is the
    /// wrong-page failure this whole confirmation scheme exists to prevent.
    /// The corpus holds deliberate duplicate sets, so this is reachable.
    #[test]
    fn two_candidates_are_refused_rather_than_guessed_between() {
        let reply = r#"{"data":{"media":{"nodes":[
            {"id":"one","name":"v01","path":"/a/v01.cbz","size":10,"pages":2},
            {"id":"two","name":"v01","path":"/b/v01.cbz","size":10,"pages":2}
        ]}}}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let refused = find_book(&fake, "v01", 10, 2).expect_err("ambiguity must not be guessed at");
        assert!(refused.contains("/a/v01.cbz"), "must name the candidates: {refused}");
        assert!(refused.contains("/b/v01.cbz"), "must name the candidates: {refused}");
    }

    /// GraphQL reports failure in a 200 body, so a transport that only checks
    /// the status reads an error document as an empty result — and a work
    /// silently loses its Stump half.
    #[test]
    fn a_graphql_error_is_a_failure_even_though_the_status_was_200() {
        let reply = r#"{"data":null,"errors":[{"message":"Unknown field \"pages\""}]}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let failed = find_book(&fake, "x", 1, 1).expect_err("errors must not read as no match");
        assert!(failed.contains("Unknown field"), "must carry the message: {failed}");
    }

    /// taiyaku is 0-based and Stump is 1-based. This is one of exactly two
    /// places in the codebase where that conversion happens, and getting it
    /// wrong shows a reader the wrong page while every hash and count in the
    /// sidecar still matches.
    #[test]
    fn page_zero_is_fetched_as_stump_page_one() {
        let fake = FakeTransport::new().on_get("/api/v2/media/abc/page/1", "image/jpeg", b"first");

        let page = page_bytes(&fake, "abc", 1).unwrap();

        assert_eq!(page.bytes, b"first");
        assert_eq!(page.content_type, "image/jpeg");
        assert_eq!(fake.requests(), vec!["GET /api/v2/media/abc/page/1"]);
    }

    /// Stump answers 404 past the end. That has to become a message naming
    /// the book and page rather than an empty 200, which a reader renders as
    /// a blank page and says nothing about.
    #[test]
    fn a_page_past_the_end_names_what_was_missing() {
        let fake = FakeTransport::new().on_get_status("/api/v2/media/abc/page/999", 404);

        let missing = page_bytes(&fake, "abc", 999).expect_err("404 must not be served as bytes");
        assert!(missing.contains("abc"), "must name the book: {missing}");
        assert!(missing.contains("999"), "must name the page: {missing}");
    }
}
```

- [ ] **Step 3: Run to verify it fails**

First wire the modules. Create `crates/taiyaku-serve/src/stump/mod.rs`:

```rust
//! Talking to a Stump instance.

pub mod api;
pub mod http;
```

and add to `crates/taiyaku-serve/src/lib.rs`, in the module list:

```rust
mod stump;
```

Run: `cargo test -p taiyaku-serve stump 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'find_book' in this scope`.

- [ ] **Step 4: Implement the API calls**

Add to `crates/taiyaku-serve/src/stump/api.rs`, above the test module:

```rust
/// One book, as much of it as taiyaku needs.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Book {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub pages: u32,
}

#[derive(Debug, Deserialize)]
struct GraphQlReply<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct MediaData {
    media: MediaPage,
}

#[derive(Debug, Deserialize)]
struct MediaPage {
    nodes: Vec<Book>,
}

/// The book matching this member, if Stump has exactly one.
///
/// All three predicates go in one query. `MediaFilterInput` exposes `name` and
/// `extension` as `FieldFilterString`, `size` as `NumericFilterI64` and
/// `pages` as `NumericFilterI32`, with `_and`, so there is no reason to filter
/// client-side.
///
/// `stem` is the file name **without its extension**, because that is what
/// `Media.name` holds.
pub fn find_book(
    transport: &dyn Transport,
    stem: &str,
    size: u64,
    pages: u32,
) -> Result<Option<Book>, String> {
    let query = r#"query Find($stem: String!, $size: Int!, $pages: Int!) {
        media(
            filter: { _and: [
                { name:  { eq: $stem } },
                { size:  { eq: $size } },
                { pages: { eq: $pages } }
            ] },
            pagination: { offset: { page: 1, pageSize: 8 } }
        ) { nodes { id name path size pages } }
    }"#;
    let body = serde_json::json!({
        "query": query,
        "variables": { "stem": stem, "size": size, "pages": pages },
    })
    .to_string();

    let raw = transport.post_json("/api/graphql", &body)?;
    let reply: GraphQlReply<MediaData> =
        serde_json::from_slice(&raw).map_err(|e| format!("stump: unreadable GraphQL reply: {e}"))?;

    // GraphQL reports failure inside a 200 body. Treating that as "no match"
    // would silently drop a work's Stump half and look exactly like a file
    // Stump does not have.
    if !reply.errors.is_empty() {
        let joined: Vec<&str> = reply.errors.iter().map(|e| e.message.as_str()).collect();
        return Err(format!("stump: {}", joined.join("; ")));
    }

    let mut nodes = reply
        .data
        .ok_or_else(|| "stump: GraphQL reply carried neither data nor errors".to_string())?
        .media
        .nodes;

    match nodes.len() {
        0 => Ok(None),
        1 => Ok(Some(nodes.remove(0))),
        // Guessing here is the wrong-page failure the page-hash confirmation
        // exists to prevent, and this corpus holds deliberate duplicate sets.
        _ => {
            let paths: Vec<&str> = nodes.iter().map(|b| b.path.as_str()).collect();
            Err(format!(
                "stump: {} books match `{stem}` at {size} bytes and {pages} pages: {}",
                nodes.len(),
                paths.join(", ")
            ))
        }
    }
}

/// One page of one book, addressed the way Stump addresses pages.
///
/// **Stump is 1-based and taiyaku is 0-based.** `one_based` is already
/// converted by the caller; this function is where the wire format is spoken
/// and the only place in `stump` that knows the number.
pub fn page_bytes(
    transport: &dyn Transport,
    id: &str,
    one_based: u32,
) -> Result<ServedPage, String> {
    let path = format!("/api/v2/media/{id}/page/{one_based}");
    let (status, content_type, bytes) = transport.get_bytes(&path)?;
    if status != 200 {
        return Err(format!(
            "stump: book {id} has no page {one_based} (HTTP {status})"
        ));
    }
    Ok(ServedPage {
        content_type,
        bytes,
    })
}
```

- [ ] **Step 5: Implement `UreqTransport`**

Append to `crates/taiyaku-serve/src/stump/http.rs`, above the test module:

```rust
/// The real thing: `ureq`, blocking, with a bearer token.
///
/// Blocking is deliberate. `PageSource` is synchronous and the boundary to
/// async is `spawn_blocking` at the router, so a blocking client is the one
/// that does not need a runtime it cannot have.
#[derive(Debug)]
pub struct UreqTransport {
    base: String,
    key: String,
}

/// Pages in this corpus reach 430 MB, and `ureq` reads at most 10 MB unless
/// told otherwise — so a default read would truncate the largest pages into a
/// corrupt image rather than failing.
const MAX_PAGE_BYTES: u64 = 512 * 1024 * 1024;

impl UreqTransport {
    /// `base` is the scheme, host and port, with no trailing slash.
    pub fn new(base: impl Into<String>, key: impl Into<String>) -> Self {
        let base = base.into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            key: key.into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.key)
    }
}

impl Transport for UreqTransport {
    fn post_json(&self, path: &str, body: &str) -> Result<Vec<u8>, String> {
        let mut response = ureq::post(self.url(path))
            .header("Authorization", self.bearer())
            .header("Content-Type", "application/json")
            .config()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .send(body)
            .map_err(|e| format!("stump: POST {path}: {e}"))?;
        response
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| format!("stump: POST {path}: reading reply: {e}"))
    }

    fn get_bytes(&self, path: &str) -> Result<(u16, String, Vec<u8>), String> {
        let mut response = ureq::get(self.url(path))
            .header("Authorization", self.bearer())
            .config()
            // A 404 from a page route is an answer, not a transport failure.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .call()
            .map_err(|e| format!("stump: GET {path}: {e}"))?;

        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_PAGE_BYTES)
            .read_to_vec()
            .map_err(|e| format!("stump: GET {path}: reading body: {e}"))?;
        Ok((status, content_type, bytes))
    }
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p taiyaku-serve 2>&1 | rg 'test result|^error'`

Expected: PASS, **17 tests** in `taiyaku-serve` (11 after Task 2, + 6 here). Workspace total **107**.

Then: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/Cargo.toml crates/taiyaku-serve/src/stump/ crates/taiyaku-serve/src/lib.rs
git commit -m "Ask Stump which book a file is, and fetch one page of it

Two protocols because Stump splits them that way: GraphQL to identify a book,
REST for the bytes. Everything goes through a Transport, so the suite needs no
network and the one test that talks to a real instance is the gated one.

Three things measured on the live instance and easy to get wrong are pinned by
tests here. Media.name is the file stem with no extension, so matching on the
basename finds nothing. GraphQL reports failure inside a 200 body, so a
transport checking only the status reads an error as an empty result and a work
silently loses its Stump half. And ureq reads 10 MB unless told otherwise,
against pages that reach 430 MB in this corpus.

Two candidates for one member is refused rather than guessed between: the
corpus holds deliberate duplicate sets, and guessing is the wrong-page failure
the confirmation step exists to prevent."
```

---

### Task 4: Resolve a member, confirm it, and cache the answer

**Files:**
- Create: `crates/taiyaku-serve/src/stump/resolve.rs`
- Create: `crates/taiyaku-serve/src/stump/cache.rs`
- Modify: `crates/taiyaku-serve/src/stump/mod.rs`

**Interfaces:**
- Consumes: `Transport`, `api::find_book`, `api::page_bytes` (Task 3); `LocalLibrary`, `PageSource` (Task 2).
- Produces:
  - `cache::Resolutions` = `HashMap<String, Option<String>>` keyed by **member `sha256`**, valued by Stump book id (`None` = Stump does not have it).
  - `cache::path_for(work: &Work) -> PathBuf`
  - `cache::load(work: &Work, base: &str) -> Resolutions`
  - `cache::store(work: &Work, base: &str, r: &Resolutions) -> Result<(), String>`
  - `resolve::resolve_work(t: &dyn Transport, work: &Work, base: &str) -> Result<Resolutions, String>`

- [ ] **Step 1: Write the failing cache test**

Create `crates/taiyaku-serve/src/stump/cache.rs`:

```rust
//! Which Stump book each member is, remembered beside the sidecar.
//!
//! Keyed by the member's `sha256` rather than its path: two members with the
//! same hash are the same file, and a work that names one archive twice should
//! resolve it once.
//!
//! **Absence is recorded too.** Most members of most works in this library are
//! not in Stump at all — every raw scan, every fan translation — and a miss
//! that is not remembered is a GraphQL round trip on every page turn.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::works::Work;

/// Member `sha256` → the Stump book it is, or `None` for "Stump has not got it".
pub type Resolutions = HashMap<String, Option<String>>;

#[derive(Debug, Serialize, Deserialize)]
struct Document {
    taiyaku_stump: u32,
    /// The instance these answers came from.
    ///
    /// Recorded so a cache written against one Stump is not silently believed
    /// against another, where the same ids mean different books or nothing.
    base: String,
    members: Resolutions,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(dir: &str) -> Work {
        crate::test_support::work(dir)
    }

    /// A resolution survives a restart, including a recorded absence — which
    /// is the common case here and the one worth not re-asking about.
    #[test]
    fn resolutions_round_trip_beside_the_sidecar() {
        let work = work("taiyaku-test-stump-cache");
        let mut resolutions = Resolutions::new();
        resolutions.insert("hash-of-en-mono".into(), Some("a8ed51d8".into()));
        resolutions.insert("hash-of-raw-mono".into(), None);

        store(&work, "http://nas:10801", &resolutions).unwrap();
        let back = load(&work, "http://nas:10801");

        let path = path_for(&work);
        let existed = path.exists();
        crate::test_support::cleanup("taiyaku-test-stump-cache");

        assert!(existed, "must write beside the sidecar: {}", path.display());
        assert_eq!(back.get("hash-of-en-mono"), Some(&Some("a8ed51d8".to_string())));
        assert_eq!(
            back.get("hash-of-raw-mono"),
            Some(&None),
            "an absence must be remembered, or every page turn asks again"
        );
    }

    /// A cache written against one instance says nothing about another. Book
    /// ids are that instance's, so believing them would serve pages from
    /// whatever happens to share an id — or 404 on everything.
    #[test]
    fn a_cache_from_a_different_instance_is_ignored() {
        let work = work("taiyaku-test-stump-cache-base");
        let mut resolutions = Resolutions::new();
        resolutions.insert("hash".into(), Some("id-on-the-old-server".into()));
        store(&work, "http://old:10801", &resolutions).unwrap();

        let back = load(&work, "http://new:10801");

        crate::test_support::cleanup("taiyaku-test-stump-cache-base");
        assert!(back.is_empty(), "a different base must not be trusted");
    }

    /// Nothing cached yet is not an error, and neither is a corrupt file: both
    /// mean "resolve again". A server that refused to start because a cache it
    /// can rebuild was truncated would be worse than one that rebuilds it.
    #[test]
    fn a_missing_or_unreadable_cache_is_empty_rather_than_fatal() {
        let work = work("taiyaku-test-stump-cache-bad");
        assert!(load(&work, "http://nas:10801").is_empty(), "nothing written yet");

        std::fs::write(path_for(&work), b"{ truncated").unwrap();
        let back = load(&work, "http://nas:10801");

        crate::test_support::cleanup("taiyaku-test-stump-cache-bad");
        assert!(back.is_empty(), "a corrupt cache rebuilds rather than refusing to start");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve cache 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'store' in this scope`.

- [ ] **Step 3: Implement the cache**

Add to `crates/taiyaku-serve/src/stump/cache.rs`, above the tests:

```rust
/// Where a work's Stump mapping sits: beside its sidecar, same stem.
///
/// The convention M2 established for progress. `taiyaku.json` stays a pure
/// alignment document that diffs meaningfully in git.
pub fn path_for(work: &Work) -> PathBuf {
    let stem = work
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "taiyaku".to_string());
    work.path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!("{stem}.stump.json"))
}

/// What was resolved against this instance, or nothing.
///
/// Every failure is "nothing", deliberately: the cache is derived data that
/// can be rebuilt in one round trip per member, so refusing to start over a
/// truncated file would trade a recoverable state for an unrecoverable one.
pub fn load(work: &Work, base: &str) -> Resolutions {
    let Ok(text) = std::fs::read_to_string(path_for(work)) else {
        return Resolutions::new();
    };
    let Ok(document) = serde_json::from_str::<Document>(&text) else {
        return Resolutions::new();
    };
    if document.taiyaku_stump != 1 || document.base != base {
        return Resolutions::new();
    }
    document.members
}

/// Write the mapping, atomically.
///
/// Write-and-rename rather than `fs::write`, which truncates first: a crash
/// mid-write would otherwise leave a short file. The same defect is recorded
/// against progress in `status.md`; this one does not ship with it.
pub fn store(work: &Work, base: &str, members: &Resolutions) -> Result<(), String> {
    let document = Document {
        taiyaku_stump: 1,
        base: base.to_string(),
        members: members.clone(),
    };
    let text = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    let final_path = path_for(work);
    let temporary = final_path.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|e| format!("{}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, &final_path).map_err(|e| format!("{}: {e}", final_path.display()))
}
```

- [ ] **Step 4: Run the cache tests**

Run: `cargo test -p taiyaku-serve cache 2>&1 | rg 'test result'`

Expected: PASS, 3 new tests.

- [ ] **Step 5: Write the failing resolve test**

Create `crates/taiyaku-serve/src/stump/resolve.rs`:

```rust
//! Deciding which Stump book a sidecar member is, and proving it.
//!
//! Matching on name and size alone would be wrong on exactly the material this
//! corpus was built to hold — deliberate duplicate sets, and at least two files
//! that are not what their names claim. So a match is a candidate, and the
//! candidate is confirmed by comparing the SHA-256 of one page fetched through
//! each source.
//!
//! One page rather than the whole file: about 1 MB against 335 MB for this
//! volume, on a corpus with 430 MB pages — and it confirms the property that
//! actually matters, which is that page N agrees, rather than that two files
//! resemble each other.

use sha2::{Digest, Sha256};

use crate::stump::cache::{self, Resolutions};
use crate::stump::{api, http::Transport};
use crate::works::Work;
use crate::{LocalLibrary, PageSource};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stump::http::test_support::FakeTransport;

    /// The fixture's `c1.cbz` holds `a1.jpg` whose bytes are its own name, so
    /// a fake serving those bytes is a book that genuinely agrees, and one
    /// serving anything else is one that does not.
    fn reply_with(size: u64, pages: u32) -> String {
        format!(
            r#"{{"data":{{"media":{{"nodes":[{{"id":"book-1","name":"c1","path":"/data/c1.cbz","size":{size},"pages":{pages}}}]}}}}}}"#
        )
    }

    /// The whole point: a candidate is not believed until a page proves it.
    #[test]
    fn a_candidate_confirmed_by_a_page_hash_is_recorded() {
        let work = crate::test_support::work("taiyaku-test-resolve-ok");
        let root = work.path.parent().unwrap();
        let size = std::fs::metadata(root.join("c1.cbz")).unwrap().len();

        let fake = FakeTransport::new()
            .on_post("/api/graphql", &reply_with(size, 2))
            .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a1.jpg");

        let resolved = resolve_work(&fake, &work, "http://nas:10801").unwrap();

        let first = work.sidecar.editions["raw"].members[0].sha256.clone();
        crate::test_support::cleanup("taiyaku-test-resolve-ok");
        assert_eq!(resolved.get(&first), Some(&Some("book-1".to_string())));
    }

    /// A book that matches by name, size and page count but serves different
    /// bytes is refused. This is the case the corpus's mislabelled files and
    /// duplicate sets make real, and accepting it would show the wrong page
    /// while every hash in the sidecar still matched.
    #[test]
    fn a_candidate_whose_page_disagrees_is_refused() {
        let work = crate::test_support::work("taiyaku-test-resolve-mismatch");
        let root = work.path.parent().unwrap();
        let size = std::fs::metadata(root.join("c1.cbz")).unwrap().len();

        let fake = FakeTransport::new()
            .on_post("/api/graphql", &reply_with(size, 2))
            .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a-different-page");

        let resolved = resolve_work(&fake, &work, "http://nas:10801").unwrap();

        let first = work.sidecar.editions["raw"].members[0].sha256.clone();
        crate::test_support::cleanup("taiyaku-test-resolve-mismatch");
        assert_eq!(
            resolved.get(&first),
            Some(&None),
            "a disagreeing page must record absence, not the book"
        );
    }

    /// Resolution runs once. A second call reads the cache and asks Stump
    /// nothing, which is what stops a page turn costing a GraphQL round trip.
    #[test]
    fn a_second_resolution_asks_stump_nothing() {
        let work = crate::test_support::work("taiyaku-test-resolve-cached");
        let root = work.path.parent().unwrap();
        let size = std::fs::metadata(root.join("c1.cbz")).unwrap().len();

        let fake = FakeTransport::new()
            .on_post("/api/graphql", &reply_with(size, 2))
            .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a1.jpg");
        resolve_work(&fake, &work, "http://nas:10801").unwrap();

        let second = FakeTransport::new();
        let resolved = resolve_work(&second, &work, "http://nas:10801").unwrap();

        let first = work.sidecar.editions["raw"].members[0].sha256.clone();
        crate::test_support::cleanup("taiyaku-test-resolve-cached");
        assert!(
            second.requests().is_empty(),
            "a cached work must not ask again: {:?}",
            second.requests()
        );
        assert_eq!(resolved.get(&first), Some(&Some("book-1".to_string())));
    }
}
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -p taiyaku-serve resolve 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'resolve_work' in this scope`.

- [ ] **Step 7: Implement resolution**

Add to `crates/taiyaku-serve/src/stump/resolve.rs`, above the tests:

```rust
fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Resolve every member of a work, using and updating the cache beside it.
///
/// Members already answered for — including those answered "not in Stump" —
/// cost nothing. Only genuinely new members reach the network.
///
/// A member that cannot be resolved is recorded absent rather than failing the
/// load: the composite falls through to local disk, which is where most of
/// this library lives anyway. A failure here would refuse to serve a work that
/// reads perfectly without Stump.
pub fn resolve_work(
    transport: &dyn Transport,
    work: &Work,
    base: &str,
) -> Result<Resolutions, String> {
    let mut resolutions = cache::load(work, base);
    let root = work
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut learned = false;

    for edition in work.sidecar.editions.values() {
        for member in &edition.members {
            if resolutions.contains_key(&member.sha256) {
                continue;
            }
            match resolve_member(transport, root, member) {
                // An answer, cached: either Stump has this member or it
                // genuinely does not, and neither changes on the next run.
                Ok(answer) => {
                    resolutions.insert(member.sha256.clone(), answer);
                    learned = true;
                }
                // NOT an answer, and so NOT cached. A transport failure, a
                // GraphQL error or an ambiguous candidate means the question
                // could not be asked, which is a different thing from Stump
                // saying no. Caching it would mean one start against an
                // unreachable NAS silently and permanently disables Stump for
                // this work — recoverable only by knowing to delete a file
                // nothing told the reader about. Leaving the key absent costs
                // one retry on the next start.
                Err(why) => {
                    eprintln!("taiyaku: {} not resolved against stump: {why}", member.path);
                }
            }
        }
    }

    if learned {
        // Not `?`. The cache is derived data — `load` already treats every
        // read failure as "resolve again" — so a full disk or a read-only
        // library directory must not refuse to serve a work that reads
        // perfectly from disk without Stump at all. This function's caller
        // runs before the listener binds, so propagating here would abort
        // startup for every work rather than slow one of them down.
        if let Err(why) = cache::store(work, base, &resolutions) {
            eprintln!(
                "taiyaku: could not remember stump resolutions for {}: {why}",
                work.path.display()
            );
        }
    }
    Ok(resolutions)
}

/// One member: find a candidate, then make it prove itself.
fn resolve_member(
    transport: &dyn Transport,
    root: &std::path::Path,
    member: &taiyaku_core::Member,
) -> Result<Option<String>, String> {
    let path = root.join(&member.path);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| format!("{}: no file name", path.display()))?;
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();

    let Some(book) = api::find_book(transport, &stem, size, member.pages)? else {
        return Ok(None);
    };

    // The confirmation: the same page through each source. Both calls take
    // taiyaku's 0-based index; `page_bytes` speaks Stump's 1-based numbering
    // internally so no caller has to remember to.
    let ours = LocalLibrary.page(root, member, 0)?;
    let theirs = api::page_bytes(transport, &book.id, 0)?;
    if digest(&ours.bytes) != digest(&theirs.bytes) {
        eprintln!(
            "taiyaku: {} matches stump book {} by name and size, but their first pages differ — treating it as absent",
            member.path, book.path
        );
        return Ok(None);
    }
    Ok(Some(book.id))
}
```

Add both modules to `crates/taiyaku-serve/src/stump/mod.rs`:

```rust
pub mod api;
pub mod cache;
pub mod http;
pub mod resolve;
```

- [ ] **Step 8: Run the suite**

Run: `cargo test -p taiyaku-serve 2>&1 | rg 'test result|^error'`

Expected: PASS, **23 tests** in `taiyaku-serve` (17 + 3 cache + 3 resolve). Workspace total **113**.

Then: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-serve/src/stump/
git commit -m "Match a member to a Stump book, then make it prove it

Name, size and page count find a candidate; comparing the SHA-256 of one page
through each source decides whether to believe it. The corpus holds deliberate
duplicate sets and at least two files that are not what their names claim,
which is exactly the material a name-and-size match gets wrong — silently, by
showing the wrong page while every hash in the sidecar still agrees.

One page rather than the whole file: about 1 MB against 335 MB for this volume,
on a corpus with 430 MB pages. It also confirms the property that matters,
which is that page N agrees rather than that two files resemble each other.

Answers are cached beside the sidecar, keyed by member sha256 so the same
archive named by two works resolves once. Absence is cached too — most members
here are raws and fan translations Stump has never seen, and an unremembered
miss is a round trip on every page turn. The write is atomic, unlike the
progress write status.md records as thin.

The cache records which instance answered, so a mapping from one Stump is not
believed against another where the same ids mean different books."
```

---

### Task 5: `CompositeSource`, the CLI flags, and `spawn_blocking`

**Files:**
- Create: `crates/taiyaku-serve/src/composite.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (export it; `serve` builds it)
- Modify: `crates/taiyaku-serve/src/routes.rs` (`spawn_blocking` in `get_page`)
- Modify: `crates/taiyaku-cli/src/main.rs:602-624` (`serve`) and the usage block at `:56`

**Interfaces:**
- Consumes: `PageSource`, `LocalLibrary` (Task 2); `Transport`, `UreqTransport`, `api::page_bytes` (Task 3); `Resolutions`, `resolve_work` (Task 4).
- Produces: `CompositeSource::new(local: LocalLibrary, stump: Option<StumpSide>) -> CompositeSource`, and `StumpSide::new(transport: Arc<dyn Transport>, resolutions: Resolutions, cooldown: Duration)`. `StumpSide` carries four fields — the two above plus `cooldown: Duration` and `down_until: Arc<Mutex<Option<Instant>>>`, the request-path circuit breaker.

> **Executed, then amended. The code blocks below are the original brief and no longer match what shipped.** Task 5 landed at `845f841`; its review found the disk fallback unbounded, and the fix wave at `9496b62` plus `873b4de` corrected it. This Interfaces block is accurate; the step snippets below still show the two-field `StumpSide` and do not compile. What changed: `StumpSide` gained the cooldown latch and a `new` constructor, `api::page_bytes` returns a typed `PageError { message, transport }` so only a *transport* failure latches and a 404 does not, the startup summary counts real members rather than answered ones, and `resolve_work`'s loop label became `'lookups`. **Do not re-derive this task from the snippets below.**

**Also remove in this task:** Task 3 had to put `#[expect(dead_code)]` on `mod stump;` in `lib.rs`, because nothing in production called the module until now. **This task is what makes it live, so the attribute must go — delete it as a deliberate step, and confirm the gate is still green afterwards.**

`expect` was chosen over `allow` so the compiler would catch a forgotten removal. **Do not rely on that here.** Task 3's fix wave bisected the behaviour on rustc 1.97.1 and found a gap: when a module becomes reachable only through struct-field reads inside `dyn Trait`-dispatched methods — which is exactly `UreqTransport::url`/`bearer` reading `self.base`/`self.key` through `&dyn Transport` — the unfulfilled-expectation warning does not fire. So the build may stay green with the attribute still in place and nothing dead. Removing it is this task's job, not the compiler's.

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-serve/src/composite.rs`:

```rust
//! One work, two sources, chosen per member.
//!
//! Measured on the development instance: of the three editions
//! `corpus/gotoubun-v01`'s sidecar records, Stump holds one. It indexes the
//! English releases; the raws and fan translations taiyaku aligns them
//! *against* are not in it, and a library of English releases is what a Stump
//! instance normally is.
//!
//! So a source that answered for a whole work would answer for no work here.
//! Routing per member is what makes taiyaku run beside Stump rather than
//! requiring it.

use std::path::Path;
use std::sync::Arc;

use taiyaku_core::Member;

use crate::stump::cache::Resolutions;
use crate::stump::{api, http::Transport};
use crate::{LocalLibrary, PageSource, ServedPage};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stump::http::test_support::FakeTransport;

    /// The property the whole milestone turns on: one work, one member from
    /// Stump and one from disk, and nothing above this line can tell.
    #[test]
    fn each_member_is_served_by_whichever_source_has_it() {
        let work = crate::test_support::work("taiyaku-test-composite");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

        let fake = FakeTransport::new().on_get(
            "/api/v2/media/book-1/page/1",
            "image/png",
            b"from-stump",
        );
        let mut resolutions = Resolutions::new();
        resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));
        resolutions.insert(edition.members[1].sha256.clone(), None);

        let composite = CompositeSource::new(
            LocalLibrary,
            Some(StumpSide { transport: Arc::new(fake), resolutions }),
        );

        let from_stump = composite.page(&root, &edition.members[0], 0).unwrap();
        let from_disk = composite.page(&root, &edition.members[1], 0).unwrap();

        crate::test_support::cleanup("taiyaku-test-composite");
        assert_eq!(from_stump.bytes, b"from-stump");
        assert_eq!(
            from_stump.content_type, "image/png",
            "the remote source's own type, not a guess from the entry name"
        );
        assert_eq!(from_disk.bytes, b"b1.jpg", "the unresolved member came off disk");
    }

    /// With no Stump configured every member comes off disk, which is the
    /// configuration `taiyaku serve` has had since M2 and must keep.
    #[test]
    fn with_no_stump_every_member_is_local() {
        let work = crate::test_support::work("taiyaku-test-composite-local");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

        let composite = CompositeSource::new(LocalLibrary, None);
        let page = composite.page(&root, &edition.members[0], 0).unwrap();

        crate::test_support::cleanup("taiyaku-test-composite-local");
        assert_eq!(page.bytes, b"a1.jpg");
    }

    /// Stump being unreachable must not lose a page that is also on disk.
    /// The alternative is a reader that stops working when the NAS reboots,
    /// for members it never needed the NAS to read.
    #[test]
    fn a_stump_failure_falls_back_to_disk_rather_than_failing() {
        let work = crate::test_support::work("taiyaku-test-composite-fallback");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

        // The fake has no reply registered, so every GET is an error.
        let mut resolutions = Resolutions::new();
        resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

        let composite = CompositeSource::new(
            LocalLibrary,
            Some(StumpSide { transport: Arc::new(FakeTransport::new()), resolutions }),
        );
        let page = composite.page(&root, &edition.members[0], 0).unwrap();

        crate::test_support::cleanup("taiyaku-test-composite-fallback");
        assert_eq!(page.bytes, b"a1.jpg", "disk answers when stump cannot");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve composite 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find struct 'CompositeSource'`.

- [ ] **Step 3: Implement the composite**

Add to `crates/taiyaku-serve/src/composite.rs`, above the tests:

```rust
/// A configured Stump instance and what has been resolved against it.
///
/// `down_until` is the request path's circuit breaker, and it exists for the
/// same reason as the one in `resolve_work`. Falling back to disk is only
/// useful if the fallback is *quick*: with a resolution cache already on disk
/// and the instance unreachable, every page would otherwise pay the full GET
/// timeout before disk answered, on every request, forever — and a reader
/// waiting two minutes a page cannot tell that from broken.
///
/// **Only a transport-level failure latches.** Stump answering 404 for a page
/// a book does not have is an answer, not an outage, and must not demote the
/// whole instance.
#[derive(Debug, Clone)]
pub struct StumpSide {
    pub transport: Arc<dyn Transport>,
    pub resolutions: Resolutions,
    /// How long a transport failure suppresses further attempts.
    pub cooldown: Duration,
    /// When the suppression lifts, if it is in force.
    pub down_until: Arc<Mutex<Option<Instant>>>,
}

/// Pages from Stump where it has them, from disk where it does not.
#[derive(Debug, Clone)]
pub struct CompositeSource {
    local: LocalLibrary,
    stump: Option<StumpSide>,
}

impl CompositeSource {
    pub fn new(local: LocalLibrary, stump: Option<StumpSide>) -> Self {
        Self { local, stump }
    }

    /// The Stump book this member is, if it is one.
    fn book_for(&self, member: &Member) -> Option<&str> {
        self.stump
            .as_ref()?
            .resolutions
            .get(&member.sha256)?
            .as_deref()
    }
}

impl PageSource for CompositeSource {
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String> {
        if let (Some(side), Some(id)) = (self.stump.as_ref(), self.book_for(member)) {
            // `within` stays 0-based here. `page_bytes` owns the conversion to
            // Stump's 1-based numbering, so this call site cannot get it wrong.
            match api::page_bytes(side.transport.as_ref(), id, within) {
                Ok(page) => return Ok(page),
                // Disk is the fallback, not the failure. A member that is also
                // on disk must not become unreadable because the NAS rebooted,
                // and this library keeps most of its members there anyway.
                Err(why) => {
                    eprintln!("taiyaku: stump book {id} page {within}: {why} — falling back to disk");
                }
            }
        }
        self.local.page(root, member, within)
    }
}
```

Declare and export it in `crates/taiyaku-serve/src/lib.rs`:

```rust
mod composite;
```

```rust
pub use composite::{CompositeSource, StumpSide};
```

- [ ] **Step 4: Run the composite tests**

Run: `cargo test -p taiyaku-serve composite 2>&1 | rg 'test result'`

Expected: PASS, 3 new tests.

- [ ] **Step 5: Build the composite in `serve`, and take the configuration**

In `crates/taiyaku-serve/src/lib.rs`, replace `pub async fn serve`:

```rust
/// How to reach a Stump instance, if one is configured.
#[derive(Debug, Clone)]
pub struct StumpConfig {
    pub base: String,
    pub key: String,
}

/// Bind and serve until interrupted.
///
/// Separate from `build_router` so every test drives the router directly and
/// none of them opens a socket.
pub async fn serve(
    works: Vec<Work>,
    addr: &str,
    stump: Option<StumpConfig>,
) -> Result<(), String> {
    let source: Arc<dyn PageSource> = match stump {
        None => Arc::new(CompositeSource::new(LocalLibrary, None)),
        Some(config) => {
            let transport: Arc<dyn stump::http::Transport> = Arc::new(
                stump::http::UreqTransport::new(config.base.clone(), config.key),
            );
            // Resolution happens once, at load, so no page turn costs a
            // GraphQL round trip. It is also where a misconfigured instance
            // announces itself, rather than on the first page a reader opens.
            let mut resolutions = stump::cache::Resolutions::new();
            for work in &works {
                let resolved =
                    stump::resolve::resolve_work(transport.as_ref(), work, &config.base)?;
                let known = resolved.values().filter(|v| v.is_some()).count();
                eprintln!(
                    "{}: {known} of {} members from stump",
                    work.id,
                    resolved.len()
                );
                resolutions.extend(resolved);
            }
            Arc::new(CompositeSource::new(
                LocalLibrary,
                Some(StumpSide { transport, resolutions }),
            ))
        }
    };

    let state = Arc::new(ServeState { works, source });
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("{addr}: {e}"))?;
    eprintln!("taiyaku serve listening on http://{addr}");
    axum::serve(listener, build_router(state))
        .await
        .map_err(|e| format!("{addr}: {e}"))
}
```

- [ ] **Step 6: Move the blocking read off the async worker**

In `crates/taiyaku-serve/src/routes.rs`, replace `get_page`:

```rust
async fn get_page(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition, index)): AxPath<(String, String, u32)>,
) -> Response {
    // Reading a page blocks: a zip decompression from disk, or an HTTP round
    // trip to Stump. On a tokio worker that stalls the whole worker, and this
    // corpus holds pages of 430 MB — with the prefetching the filmstrip and
    // swap both need, one reader would occupy several of them.
    //
    // `PageSource` stays synchronous deliberately. The boundary belongs here,
    // where there is exactly one of it, rather than in each implementation.
    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work(&id).ok_or_else(|| format!("no work `{id}`"))?;
        state.page(found, &edition, index)
    })
    .await;

    match fetched {
        Ok(Ok(page)) => ([(header::CONTENT_TYPE, page.content_type)], page.bytes).into_response(),
        Ok(Err(e)) => not_found(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("page read panicked: {e}"),
        )
            .into_response(),
    }
}
```

`state` is moved into the closure, so the `work()` lookup moves inside it too — which is why this handler no longer uses the shared `work()` helper. The other four routes still do.

- [ ] **Step 7: Wire the CLI flags**

In `crates/taiyaku-cli/src/main.rs`, replace `fn serve` at line 602:

```rust
fn serve(args: &[String]) -> Result<(), String> {
    let paths: Vec<PathBuf> = args
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if paths.is_empty() {
        return Err("needs at least one taiyaku.json to serve".into());
    }
    let addr = option_value(args, "--addr").unwrap_or("127.0.0.1:8484");

    // A key on the command line is visible in `ps`, so the environment is the
    // documented way and the flag is the override.
    let key = option_value(args, "--stump-key")
        .map(str::to_string)
        .or_else(|| std::env::var("TAIYAKU_STUMP_KEY").ok());
    let base = option_value(args, "--stump-url")
        .map(str::to_string)
        .or_else(|| std::env::var("TAIYAKU_STUMP_URL").ok());
    let stump = match (base, key) {
        (Some(base), Some(key)) => Some(taiyaku_serve::StumpConfig { base, key }),
        (Some(_), None) => {
            return Err("--stump-url needs a key: --stump-key, or TAIYAKU_STUMP_KEY".into());
        }
        (None, Some(_)) => {
            return Err("a stump key was given with no --stump-url or TAIYAKU_STUMP_URL".into());
        }
        (None, None) => None,
    };

    let works = taiyaku_serve::load_works(&paths)?;
    for work in &works {
        eprintln!(
            "{} | {} editions | {}",
            work.id,
            work.sidecar.editions.len(),
            work.sidecar.work.title
        );
    }
    tokio::runtime::Runtime::new()
        .map_err(|e| format!("{e}"))?
        .block_on(taiyaku_serve::serve(works, addr, stump))
}
```

Half a configuration is refused rather than silently ignored: a URL with no key would otherwise start a local-only server that looks configured.

Update the usage line at `crates/taiyaku-cli/src/main.rs:56`:

```rust
            println!(
                "       taiyaku serve  <taiyaku.json>... [--addr HOST:PORT] [--stump-url URL] [--stump-key KEY]"
            );
```

- [ ] **Step 8: Run everything**

Run: `cargo test --workspace --all-features 2>&1 | rg 'test result|^error'`

Expected: PASS, **116 total** (113 + 3). `taiyaku-serve` goes 23 → 26.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Expected: clean.

- [ ] **Step 9: Confirm the local path still works end to end**

```bash
cargo run -q -p taiyaku-cli -- serve corpus/gotoubun-v01/taiyaku.json --addr 127.0.0.1:8491 &
sleep 2
curl -s -o /dev/null -w 'works %{http_code}\n' http://127.0.0.1:8491/api/works
curl -s -o /dev/null -w 'page  %{http_code} %{content_type}\n' http://127.0.0.1:8491/api/works/taiyaku/raw-mono/pages/0
kill %1
```

Expected: `works 200`, `page 200 image/jpeg`. Skip if the corpus is absent on this machine.

- [ ] **Step 10: Commit**

```bash
git add crates/taiyaku-serve/src/composite.rs crates/taiyaku-serve/src/lib.rs \
        crates/taiyaku-serve/src/routes.rs crates/taiyaku-cli/src/main.rs
git commit -m "Serve a work whose editions live in two places

Of the three editions gotoubun-v01's sidecar records, the development Stump
holds one — it indexes the English releases, and the raws and fan translations
taiyaku aligns them against are not in it. A source answering for a whole work
would answer for no work in this library, so CompositeSource routes per member.

Stump being unreachable falls back to disk rather than failing, for members
disk also has. A reader should not stop working when the NAS reboots, for pages
it never needed the NAS to read.

Page reads move to spawn_blocking, the second deferral carried into M3. A zip
decompression or an HTTP round trip on a tokio worker stalls that worker, and
this corpus holds 430 MB pages. PageSource stays synchronous: the boundary
belongs at the router, where there is one of it.

serve takes --stump-url and --stump-key, defaulting to TAIYAKU_STUMP_URL and
TAIYAKU_STUMP_KEY because a key in argv is visible in ps. Half a configuration
is refused rather than starting a local-only server that looks configured."
```

---

### Task 6: The OPDS 1.2 + PSE tree

**Files:**
- Create: `crates/taiyaku-serve/src/opds.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (declare the module)
- Modify: `crates/taiyaku-serve/src/routes.rs` (register the routes)

**Interfaces:**
- Consumes: `Edition::total_pages` (Task 1); `ServeState::page` (Task 2).
- Produces: `opds::routes() -> Router<Arc<ServeState>>`, merged by `build_router`; `opds::escape(&str) -> String`.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/opds.rs`:

```rust
//! OPDS 1.2 with the Page Streaming Extension.
//!
//! Stump already serves OPDS-PSE, so this feed is not here to let a reader
//! fetch pages — it is here for the one thing neither Stump nor Komga can
//! express: **several editions grouped under one work**. Measured on the
//! development instance, `Go-Toubun no Hanayome` and its Full Color Edition
//! are two unrelated series of 14 books each; with the raws and the fan
//! chapters one work would occupy four, with nothing relating them.
//!
//! XML is written by hand rather than through a crate. The element set is
//! small and fixed, and the only hard part is escaping — which has its own
//! test, because this corpus holds titles with `&` and `♡` in them.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Path as AxPath, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::ServeState;

const ATOM: &str = "application/atom+xml;profile=opds-catalog";
const PSE_NS: &str = "http://vaemendis.net/opds-pse/ns";
const SIDECAR_REL: &str = "https://taiyaku.dev/rel/sidecar";

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn body_of(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// A title is not XML. This corpus holds `Bisque Doll de Chu ♡` and plenty
    /// of `&` — and an unescaped `&` does not produce a slightly wrong feed,
    /// it produces one no client can parse at all.
    #[test]
    fn markup_in_a_title_is_escaped_rather_than_emitted() {
        assert_eq!(escape("Tom & Jerry"), "Tom &amp; Jerry");
        assert_eq!(escape("<script>"), "&lt;script&gt;");
        assert_eq!(escape(r#"a "b" 'c'"#), "a &quot;b&quot; &apos;c&apos;");
        // Not markup, and not something to mangle.
        assert_eq!(escape("Bisque Doll de Chu ♡"), "Bisque Doll de Chu ♡");
    }

    /// The root has to be reachable and say what it is, or no client gets
    /// past the first request.
    #[tokio::test]
    async fn the_catalog_is_an_atom_navigation_feed() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-root"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::get("/opds/v1.2/catalog").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-root");
        assert!(content_type.starts_with(ATOM), "{content_type}");
        assert!(body.starts_with("<?xml"), "{body}");
        assert!(body.contains("/opds/v1.2/works"), "must link onward: {body}");
    }

    /// The whole justification for this feed: one entry per edition, under one
    /// work, each streamable, with the sidecar linked so a client that
    /// understands taiyaku can resolve between them.
    #[tokio::test]
    async fn a_work_lists_its_editions_with_page_counts_and_a_sidecar_link() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-work"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::get("/opds/v1.2/works/t").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-work");
        assert!(body.contains(PSE_NS), "the PSE namespace must be declared: {body}");
        assert!(
            body.contains(r#"pse:count="4""#),
            "the edition's flat page count, across both members: {body}"
        );
        assert!(
            body.contains("/opds/v1.2/works/t/raw/pages/{pageNumber}"),
            "the stream template: {body}"
        );
        assert!(body.contains(SIDECAR_REL), "the sidecar link: {body}");
        assert!(body.contains("/api/works/t"), "pointing at the sidecar: {body}");
    }

    /// PSE is 1-based and taiyaku is 0-based. The second of exactly two places
    /// this is spoken, and the one a third-party client exercises.
    #[tokio::test]
    async fn pse_page_one_is_taiyaku_page_zero() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-page"));
        let app = routes().with_state(state);

        let first = app
            .clone()
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let first = body_of(first).await;

        let third = app
            .clone()
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let third = body_of(third).await;

        // PSE has no page 0. Answering with page 0's bytes would put every
        // client one page out for the whole book.
        let zero = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let zero_status = zero.status();

        crate::test_support::cleanup("taiyaku-test-opds-page");
        assert_eq!(first, "a1.jpg", "pse page 1 is taiyaku index 0");
        assert_eq!(third, "b1.jpg", "pse page 3 is taiyaku index 2");
        assert_eq!(zero_status, StatusCode::NOT_FOUND, "pse has no page 0");
    }

    /// A feed for a work this server does not hold says so, rather than
    /// returning an empty feed a client renders as an empty library.
    #[tokio::test]
    async fn an_unknown_work_is_refused_rather_than_served_empty() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-missing"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(Request::get("/opds/v1.2/works/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-missing");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("nope"), "must name the work: {body}");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve opds 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'escape' in this scope`.

- [ ] **Step 3: Implement the feeds**

Add to `crates/taiyaku-serve/src/opds.rs`, above the tests:

```rust
/// The five characters XML cannot carry literally.
///
/// Everything else passes through unchanged: the document is UTF-8 and this
/// library's titles are full of characters that are perfectly legal and would
/// be mangled by a stricter hand.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

fn feed(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, format!("{ATOM};kind=navigation"))],
        body,
    )
        .into_response()
}

fn missing(message: String) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

async fn catalog() -> Response {
    feed(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>taiyaku:catalog</id>
  <title>taiyaku</title>
  <updated>1970-01-01T00:00:00Z</updated>
  <author><name>taiyaku</name></author>
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
  <entry>
    <id>taiyaku:works</id>
    <title>Works</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">Every work this server holds, with its editions</content>
    <link type="{ATOM};kind=navigation" href="/opds/v1.2/works"/>
  </entry>
</feed>
"#
    ))
}

async fn works(State(state): State<Arc<ServeState>>) -> Response {
    let mut entries = String::new();
    for work in &state.works {
        let title = escape(&work.sidecar.work.title);
        let id = escape(&work.id);
        let count = work.sidecar.editions.len();
        entries.push_str(&format!(
            r#"  <entry>
    <id>taiyaku:work:{id}</id>
    <title>{title}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{count} editions</content>
    <link type="{ATOM};kind=acquisition" href="/opds/v1.2/works/{id}"/>
  </entry>
"#
        ));
    }
    feed(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>taiyaku:works</id>
  <title>Works</title>
  <updated>1970-01-01T00:00:00Z</updated>
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/works"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
{entries}</feed>
"#
    ))
}

async fn work_feed(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
) -> Response {
    let Some(work) = state.work(&id) else {
        return missing(format!("no work `{id}`"));
    };
    let work_id = escape(&work.id);
    let title = escape(&work.sidecar.work.title);

    let mut entries = String::new();
    for (key, edition) in &work.sidecar.editions {
        let key_x = escape(key);
        let label = escape(&edition.label);
        // The flat page count across every member, which is exactly what PSE
        // streams. An edition of seven chapter files is one publication of 235
        // pages here, which only works because pages are addressed by flat
        // index rather than by member and entry.
        let pages = edition.total_pages();
        entries.push_str(&format!(
            r#"  <entry>
    <id>taiyaku:edition:{work_id}:{key_x}</id>
    <title>{title} — {label}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{pages} pages</content>
    <link rel="http://opds-spec.org/image/thumbnail" type="image/jpeg"
          href="/opds/v1.2/works/{work_id}/{key_x}/cover"/>
    <link rel="http://vaemendis.net/opds-pse/stream" type="image/jpeg"
          href="/opds/v1.2/works/{work_id}/{key_x}/pages/{{pageNumber}}"
          pse:count="{pages}"/>
  </entry>
"#
        ));
    }

    (
        [(header::CONTENT_TYPE, format!("{ATOM};kind=acquisition"))],
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:pse="{PSE_NS}">
  <id>taiyaku:work:{work_id}</id>
  <title>{title}</title>
  <updated>1970-01-01T00:00:00Z</updated>
  <link rel="self"  type="{ATOM};kind=acquisition" href="/opds/v1.2/works/{work_id}"/>
  <link rel="start" type="{ATOM};kind=navigation"  href="/opds/v1.2/catalog"/>
  <link rel="{SIDECAR_REL}" type="application/json" href="/api/works/{work_id}"/>
{entries}</feed>
"#
        ),
    )
        .into_response()
}

async fn stream_page(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition, number)): AxPath<(String, String, u32)>,
) -> Response {
    // PSE counts from 1. Answering for 0 would hand back page 0's bytes under
    // a number no client asked for and put the whole book one page out.
    let Some(index) = number.checked_sub(1) else {
        return missing("page streaming is 1-based; there is no page 0".to_string());
    };
    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work(&id).ok_or_else(|| format!("no work `{id}`"))?;
        state.page(found, &edition, index)
    })
    .await;

    match fetched {
        Ok(Ok(page)) => ([(header::CONTENT_TYPE, page.content_type)], page.bytes).into_response(),
        Ok(Err(e)) => missing(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("page read panicked: {e}"),
        )
            .into_response(),
    }
}

/// Every OPDS route, ready to be merged into the main router.
pub fn routes() -> Router<Arc<ServeState>> {
    Router::new()
        .route("/opds/v1.2/catalog", get(catalog))
        .route("/opds/v1.2/works", get(works))
        .route("/opds/v1.2/works/{work}", get(work_feed))
        .route(
            "/opds/v1.2/works/{work}/{edition}/pages/{number}",
            get(stream_page),
        )
}
```

Note `number.checked_sub(1)` — `number` is `u32`, so `0` yields `None` rather than wrapping to `u32::MAX`.

- [ ] **Step 4: Register the routes**

In `crates/taiyaku-serve/src/lib.rs` add `mod opds;`.

In `crates/taiyaku-serve/src/routes.rs`, extend `build_router`:

```rust
pub fn build_router(state: Arc<ServeState>) -> Router {
    Router::new()
        .route("/api/works", get(list_works))
        .route("/api/works/{work}", get(get_sidecar))
        .route("/api/works/{work}/{edition}/pages/{index}", get(get_page))
        .route(
            "/api/works/{work}/progress",
            get(get_progress).put(put_progress),
        )
        .merge(crate::opds::routes())
        .with_state(state)
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p taiyaku-serve 2>&1 | rg 'test result|^error'`

Expected: PASS, **31 tests** in `taiyaku-serve` (26 + 5). Workspace total **121**.

The cover route is referenced by the feed but not registered until Task 7; that is a 404 from a link, not a compile error, and Task 7 closes it.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/opds.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs
git commit -m "Publish a work's editions as one work, which OPDS clients can read

Stump already serves OPDS-PSE, so this feed is not here to stream pages. It is
here for the thing neither Stump nor Komga can express: several editions
grouped under one work. On the development instance Go-Toubun no Hanayome and
its Full Color Edition are two unrelated series of 14 books each, and with the
raws and fan chapters one work would occupy four.

An edition of seven chapter files streams as one publication of 235 pages,
which works only because pages are addressed by flat index — the reader
design's decision earning its keep exactly where it said it would.

PSE is 1-based and taiyaku is 0-based, so page 0 is refused rather than served
page 0's bytes, which would put a client one page out for a whole book.

XML is written by hand and escaped by a tested function: this library holds
titles with ampersands, and an unescaped one does not produce a slightly wrong
feed but one no client can parse."
```

---

### Task 7: Covers, cached on disk

**Files:**
- Create: `crates/taiyaku-serve/src/cover.rs`
- Modify: `crates/taiyaku-serve/Cargo.toml`
- Modify: `crates/taiyaku-serve/src/lib.rs`, `crates/taiyaku-serve/src/opds.rs` (register the route)

**Interfaces:**
- Consumes: `ServeState::page` (Task 2); `Edition` (Task 1).
- Produces: `cover::thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String>`, `cover::cache_path(dir: &Path, sha256: &str) -> PathBuf`, and the `GET /opds/v1.2/works/{work}/{edition}/cover` handler.

- [ ] **Step 1: Add the dependency**

In `crates/taiyaku-serve/Cargo.toml`, add to `[dependencies]`:

```toml
image = "0.25.9"
```

Same version `taiyaku-cli` already uses, so the workspace resolves one copy.

- [ ] **Step 2: Write the failing test**

Create `crates/taiyaku-serve/src/cover.rs`:

```rust
//! One cover per edition, for the OPDS entries.
//!
//! Scoped deliberately. Every OPDS entry needs a cover, so this ships now; the
//! filmstrip's thumbnail-per-page route waits for M5, when its access pattern
//! is known rather than guessed. One cover per edition and 195 thumbnails per
//! volume are different problems and solving them together on a guess about
//! the second would be the wrong order.
//!
//! Cached by the first member's `sha256`, which is content-addressed: the same
//! archive under two sidecars generates one cover, and an archive that is
//! replaced generates a new one without anything having to notice.

use std::path::{Path, PathBuf};

/// The long edge of a generated cover, in pixels.
///
/// Large enough for a grid on a tablet, small enough that generating one is
/// not the slow part of opening a feed.
const LONG_EDGE: u32 = 512;

#[cfg(test)]
mod tests {
    use super::*;

    /// A cover is smaller than the page it came from, and is a JPEG whatever
    /// the page was — an OPDS entry states one type and a client is entitled
    /// to believe it.
    #[test]
    fn a_cover_is_a_bounded_jpeg_whatever_the_page_was() {
        // A 900x1400 PNG, which is portrait like a manga page.
        let page = image::RgbImage::from_fn(900, 1400, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(page)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();

        let cover = thumbnail(&encoded.into_inner()).unwrap();
        let decoded = image::load_from_memory(&cover).unwrap();

        assert!(cover.starts_with(b"\xff\xd8\xff"), "a cover is a JPEG");
        assert_eq!(decoded.height(), LONG_EDGE, "the long edge is bounded");
        assert!(decoded.width() <= LONG_EDGE, "and so is the short one");
        // The shape, not an exact pixel count: which way `thumbnail` rounds
        // 900x1400 into a 512 box is an implementation detail of `image`, and
        // pinning it would fail on a patch release without anything being
        // wrong. What must hold is that the page was not distorted.
        let before = 900.0 / 1400.0;
        let after = f64::from(decoded.width()) / f64::from(decoded.height());
        assert!(
            (before - after).abs() < 0.01,
            "aspect ratio changed: {before} -> {after}"
        );
    }

    /// Bytes that are not an image fail with a message rather than producing
    /// a zero-byte cover an OPDS client would render as a broken tile.
    #[test]
    fn bytes_that_are_not_an_image_fail_by_name() {
        let refused = thumbnail(b"not an image at all").expect_err("must not invent a cover");
        assert!(refused.contains("cover"), "must say what failed: {refused}");
    }

    /// The cache is keyed by content, so it does not need invalidating: a
    /// replaced archive has a different hash and therefore a different path.
    #[test]
    fn the_cache_path_is_keyed_by_the_member_hash() {
        let dir = Path::new("/tmp/covers");
        let one = cache_path(dir, "abc123");
        let two = cache_path(dir, "def456");

        assert_ne!(one, two);
        assert!(one.to_string_lossy().contains("abc123"));
        assert!(one.extension().unwrap() == "jpg");
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p taiyaku-serve cover 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find function 'thumbnail' in this scope`.

- [ ] **Step 4: Implement**

Add to `crates/taiyaku-serve/src/cover.rs`, above the tests:

```rust
/// Where a cover for this member is cached.
pub fn cache_path(dir: &Path, sha256: &str) -> PathBuf {
    dir.join(format!("{sha256}.jpg"))
}

/// Shrink a page into a cover.
///
/// Always JPEG, whatever came in: an OPDS entry declares one type per link and
/// a client is entitled to believe it, so emitting whatever the page happened
/// to be would make that declaration a guess.
pub fn thumbnail(page: &[u8]) -> Result<Vec<u8>, String> {
    let decoded =
        image::load_from_memory(page).map_err(|e| format!("cover: undecodable page: {e}"))?;
    let scaled = decoded.thumbnail(LONG_EDGE, LONG_EDGE);
    let mut out = std::io::Cursor::new(Vec::new());
    scaled
        .into_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| format!("cover: encoding: {e}"))?;
    Ok(out.into_inner())
}

/// Read a cached cover, or make one and cache it.
///
/// A cache that cannot be written is not an error — the cover was generated
/// and the client should get it. Failing here would make a read-only cache
/// directory break the feed rather than merely slow it.
pub fn cached_or_generated(
    dir: &Path,
    sha256: &str,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let path = cache_path(dir, sha256);
    if let Ok(found) = std::fs::read(&path) {
        return Ok(found);
    }
    let generated = thumbnail(&page()?)?;
    if std::fs::create_dir_all(dir).is_ok() {
        let temporary = path.with_extension("jpg.tmp");
        if std::fs::write(&temporary, &generated).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
    }
    Ok(generated)
}
```

Declare it in `crates/taiyaku-serve/src/lib.rs`:

```rust
mod cover;
```

- [ ] **Step 5: Add the cover route**

`ServeState` needs to know where the cache lives. In `crates/taiyaku-serve/src/lib.rs`, add a field:

```rust
#[derive(Debug)]
pub struct ServeState {
    pub works: Vec<Work>,
    pub source: Arc<dyn PageSource>,
    /// Where generated covers are kept between runs.
    pub cache_dir: std::path::PathBuf,
}
```

Set it in `serve` (`.taiyaku-cache/covers`, matching the CLI's existing `.taiyaku-cache/` convention):

```rust
    let state = Arc::new(ServeState {
        works,
        source,
        cache_dir: std::path::PathBuf::from(".taiyaku-cache/covers"),
    });
```

and in `test_support::state`:

```rust
    pub(crate) fn state(dir: &str) -> ServeState {
        ServeState {
            works: vec![work(dir)],
            source: std::sync::Arc::new(crate::LocalLibrary),
            cache_dir: std::env::temp_dir().join(dir).join("covers"),
        }
    }
```

The corpus check in Task 2 also constructs a `ServeState`; add the same field there:

```rust
    let state = ServeState {
        works: works.clone(),
        source: Arc::new(LocalLibrary),
        cache_dir: std::env::temp_dir().join("taiyaku-corpus-covers"),
    };
```

Then in `crates/taiyaku-serve/src/opds.rs`, add the handler:

```rust
async fn cover(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition)): AxPath<(String, String)>,
) -> Response {
    let fetched = tokio::task::spawn_blocking(move || {
        let work = state.work(&id).ok_or_else(|| format!("no work `{id}`"))?;
        let found = work
            .sidecar
            .editions
            .get(&edition)
            .ok_or_else(|| format!("no edition `{edition}` in work `{id}`"))?;
        let first = found
            .members
            .first()
            .ok_or_else(|| format!("`{edition}` has no members"))?;
        crate::cover::cached_or_generated(&state.cache_dir, &first.sha256, || {
            state.page(work, &edition, 0).map(|p| p.bytes)
        })
    })
    .await;

    match fetched {
        Ok(Ok(bytes)) => (
            [(header::CONTENT_TYPE, "image/jpeg")],
            bytes,
        )
            .into_response(),
        Ok(Err(e)) => missing(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cover generation panicked: {e}"),
        )
            .into_response(),
    }
}
```

and register it in `routes()`:

```rust
        .route("/opds/v1.2/works/{work}/{edition}/cover", get(cover))
```

- [ ] **Step 6: Run the suite**

Run: `cargo test --workspace --all-features 2>&1 | rg 'test result|^error'`

Expected: PASS, **124 total** (121 + 3). `taiyaku-serve` goes 31 → 34.

The fixture's "pages" are the ASCII strings `a1.jpg` etc., not real images, so a cover request against the fixture fails by design — that is what `bytes_that_are_not_an_image_fail_by_name` covers.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/Cargo.toml crates/taiyaku-serve/src/cover.rs \
        crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/opds.rs \
        crates/taiyaku-serve/src/corpus.rs
git commit -m "Give every OPDS entry a cover, and keep it

Scoped deliberately: one cover per edition ships now because every entry needs
one, and the filmstrip's thumbnail-per-page route waits for M5 when its access
pattern is known rather than guessed. One cover and 195 thumbnails are
different problems.

Cached by the first member's sha256, which is content-addressed and so needs no
invalidation — a replaced archive hashes differently and lands at a different
path. A cache that cannot be written is not an error: the cover was generated
and the client should have it.

Always JPEG whatever the page was, because the entry declares one type per link
and a client is entitled to believe it."
```

---

### Task 8: The request-log middleware

> **Executed, then substantially corrected. The snippets below are the original brief and are wrong in several ways.** Task 8 landed at `dab2a61`; its review found the log could not answer three of the five questions the spec says it exists to answer. Corrected by the fix wave that follows it.
>
> **The brief contradicted itself** and this is the resolution: `Entry`'s concurrency field is the **arrival snapshot** — requests in flight at the instant this one arrived, counting itself — named `in_flight`, not `concurrent`. The brief's Step 1 test asserted a burst *peak*; that assertion was the typo, since the brief's own field doc, its Step 3 code, and the spec's "in-flight concurrency" all say arrival snapshot. Arrival snapshot is also strictly more informative: `max` over a burst recovers the peak exactly, while nothing recovers the arrival sequence from a repeated peak.
>
> Also corrected: an `Entry` carries `started_at` (unix micros), because lines are appended in *completion* order and without a timestamp neither re-request patterns nor feed-walking cadence — both named in the spec — can be measured. The logged path is `path_and_query()`, not `path()`, because the spec asks for **requested image dimensions** and a client asking `?width=1536` is invisible otherwise. A log file that cannot be opened fails loudly at startup rather than silently producing nothing. And `InFlight` decrements on `Drop`, so a cancelled prefetch — exactly what Panels does on a fast page turn — does not leak the in-flight count.
>
> **Do not re-derive this task from the snippets below.**

**Files:**
- Create: `crates/taiyaku-serve/src/reqlog.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs`, `crates/taiyaku-serve/src/routes.rs`, `crates/taiyaku-cli/src/main.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `reqlog::Log` (an `Arc`-able sink) and `reqlog::layer(log: Arc<Log>) -> impl Layer`, applied in `build_router`.

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-serve/src/reqlog.rs`:

```rust
//! What a client asked for, and when.
//!
//! Automated UI testing of Panels is a dead end — App Store builds are
//! FairPlay-encrypted, so the Simulator cannot install them — so the request
//! log is the instrument instead. Prefetch depth, page-turn concurrency and
//! re-request patterns are all visible in what a client asks for and when.
//!
//! **It measures how Panels fetches, not how Panels reads.** Gestures,
//! transitions, zoom and spread handling are invisible here and want a person
//! writing down what the app gets right. That is a later task.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

/// One request, as a line.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub bytes: u64,
    pub micros: u64,
    /// How many other requests were in flight when this one arrived.
    ///
    /// The number that answers "how deep does it prefetch on a page turn",
    /// which a sequence of timestamps only implies.
    pub concurrent: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The log has to record concurrency, not just order: a client fetching
    /// six pages at once and one fetching six in a row produce the same list
    /// of paths and mean entirely different things.
    #[test]
    fn a_log_records_what_was_in_flight_alongside_it() {
        let log = Log::new(None);

        let first = log.begin();
        let second = log.begin();
        log.finish(second, "GET", "/b", 200, 20);
        log.finish(first, "GET", "/a", 200, 10);

        let entries = log.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/b");
        assert_eq!(entries[0].concurrent, 2, "two were in flight");
        assert_eq!(entries[1].path, "/a");
        assert_eq!(entries[1].concurrent, 2);
        assert_eq!(entries[1].bytes, 10);
    }

    /// Written as JSON lines so a summary script is `jq`, not a parser.
    #[test]
    fn an_entry_serialises_as_one_json_line() {
        let entry = Entry {
            method: "GET".into(),
            path: "/opds/v1.2/works/t/raw/pages/7".into(),
            status: 200,
            bytes: 1180617,
            micros: 4200,
            concurrent: 3,
        };

        let line = entry.to_json_line();

        assert!(line.ends_with('\n'), "one entry, one line");
        assert!(!line[..line.len() - 1].contains('\n'), "and no more: {line}");
        assert!(line.contains(r#""concurrent":3"#), "{line}");
        assert!(line.contains(r#""path":"/opds/v1.2/works/t/raw/pages/7""#), "{line}");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-serve reqlog 2>&1 | tail -20`

Expected: FAIL to compile, `cannot find struct 'Log' in this scope`.

- [ ] **Step 3: Implement the log**

Add to `crates/taiyaku-serve/src/reqlog.rs`, above the tests:

```rust
impl Entry {
    /// One JSON object on one line, so a summary is `jq` rather than a parser.
    pub fn to_json_line(&self) -> String {
        format!(
            r#"{{"method":"{}","path":"{}","status":{},"bytes":{},"micros":{},"concurrent":{}}}"#,
            self.method,
            self.path.replace('"', "\\\""),
            self.status,
            self.bytes,
            self.micros,
            self.concurrent
        ) + "\n"
    }
}

/// A request in progress.
#[derive(Debug)]
pub struct InFlight {
    started: Instant,
    concurrent: i64,
}

/// Where request lines accumulate.
#[derive(Debug)]
pub struct Log {
    entries: Mutex<Vec<Entry>>,
    in_flight: AtomicI64,
    path: Option<std::path::PathBuf>,
}

impl Log {
    /// `path` is where lines are appended, or `None` to keep them in memory.
    pub fn new(path: Option<std::path::PathBuf>) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            in_flight: AtomicI64::new(0),
            path,
        }
    }

    /// A request arrived. The count returned includes this one.
    pub fn begin(&self) -> InFlight {
        let concurrent = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        InFlight {
            started: Instant::now(),
            concurrent,
        }
    }

    pub fn finish(&self, request: InFlight, method: &str, path: &str, status: u16, bytes: u64) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let entry = Entry {
            method: method.to_string(),
            path: path.to_string(),
            status,
            bytes,
            micros: request.started.elapsed().as_micros() as u64,
            concurrent: request.concurrent,
        };
        if let Some(destination) = &self.path {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(destination)
            {
                let _ = file.write_all(entry.to_json_line().as_bytes());
            }
        }
        self.entries.lock().unwrap().push(entry);
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().clone()
    }
}
```

- [ ] **Step 4: Apply it as middleware**

Add `mod reqlog;` and `pub use reqlog::Log;` to `crates/taiyaku-serve/src/lib.rs`, and a field on `ServeState`:

```rust
    /// Set when `--log-requests` was given; otherwise nothing is recorded.
    pub log: Option<Arc<crate::reqlog::Log>>,
```

Set it to `None` in `test_support::state` and in the corpus check, and from the CLI in `serve`.

In `crates/taiyaku-serve/src/routes.rs`, add the middleware to `build_router`:

```rust
pub fn build_router(state: Arc<ServeState>) -> Router {
    let log = state.log.clone();
    Router::new()
        .route("/api/works", get(list_works))
        .route("/api/works/{work}", get(get_sidecar))
        .route("/api/works/{work}/{edition}/pages/{index}", get(get_page))
        .route(
            "/api/works/{work}/progress",
            get(get_progress).put(put_progress),
        )
        .merge(crate::opds::routes())
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let log = log.clone();
                async move {
                    let Some(log) = log else {
                        return next.run(request).await;
                    };
                    let method = request.method().to_string();
                    let path = request.uri().path().to_string();
                    let started = log.begin();
                    let response = next.run(request).await;
                    let status = response.status().as_u16();
                    // The body's own known length, not the Content-Length
                    // header: axum sets that header when it serialises the
                    // response, which is after this middleware has already
                    // run — so reading the header here reports 0 for every
                    // request, and bytes-per-page is one of the numbers this
                    // log exists to produce.
                    let bytes = {
                        use axum::body::HttpBody;
                        response.body().size_hint().exact().unwrap_or(0)
                    };
                    log.finish(started, &method, &path, status, bytes);
                    response
                }
            },
        ))
        .with_state(state)
}
```

`axum::middleware::from_fn` is used rather than `tower-http`'s trace layer, so no dependency is added for a job this small.

In `crates/taiyaku-cli/src/main.rs`, accept the flag in `serve` and pass it through — add to the option parsing:

```rust
    let log = option_value(args, "--log-requests").map(PathBuf::from);
```

and add `log` to the `taiyaku_serve::serve(...)` call, extending that function's signature to take `Option<PathBuf>` and build the `Log`. Update the usage line to mention `[--log-requests FILE]`.

- [ ] **Step 5: Add the summary script**

Create `scripts/request-summary.sh`:

```bash
#!/usr/bin/env bash
# Summarise a taiyaku request log into the numbers docs/notes.md wants:
# prefetch depth, page-turn concurrency, image sizes and re-request patterns.
#
#   scripts/request-summary.sh panels.jsonl
set -euo pipefail

log="${1:?usage: request-summary.sh <log.jsonl>}"
[ -s "$log" ] || { echo "error: $log is empty or missing" >&2; exit 1; }

echo "requests:            $(wc -l < "$log")"
echo "max concurrency:     $(jq -s 'map(.concurrent) | max' "$log")"
echo "mean page bytes:     $(jq -s '[.[] | select(.path | test("/pages/")) | .bytes] | if length == 0 then 0 else (add / length | floor) end' "$log")"
echo "page requests:       $(jq -s '[.[] | select(.path | test("/pages/"))] | length' "$log")"
echo "distinct pages:      $(jq -s '[.[] | select(.path | test("/pages/")) | .path] | unique | length' "$log")"
echo "re-requested pages:  $(jq -s '[.[] | select(.path | test("/pages/")) | .path] | group_by(.) | map(select(length > 1)) | length' "$log")"
echo
echo "slowest five:"
jq -s 'sort_by(-.micros) | .[:5] | .[] | "  \(.micros/1000 | floor)ms  \(.status)  \(.path)"' -r "$log"
```

Then: `chmod +x scripts/request-summary.sh`

- [ ] **Step 6: Run the suite**

Run: `cargo test --workspace --all-features 2>&1 | rg 'test result|^error'`

Expected: PASS, **126 total** (124 + 2). `taiyaku-serve` goes 34 → 36.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/reqlog.rs crates/taiyaku-serve/src/lib.rs \
        crates/taiyaku-serve/src/routes.rs crates/taiyaku-cli/src/main.rs \
        crates/taiyaku-serve/src/corpus.rs scripts/request-summary.sh
git commit -m "Record what a client asked for, so Panels can be measured

Automated UI testing of Panels is a dead end — App Store builds are
FairPlay-encrypted and the Simulator cannot install them — so the request log is
the instrument. Concurrency is recorded alongside each line rather than inferred
from timestamps: six pages fetched at once and six fetched in a row produce the
same list of paths and mean entirely different things.

JSON lines, so the summary is jq rather than a parser. Off unless
--log-requests is given.

This measures how a client fetches, not how it reads. Gestures, transitions,
zoom and spread handling are invisible here and want a person writing down what
the app gets right, which is recorded in the spec as a later task."
```

---

### Task 9: The gated equivalence test, and making a skipped gate visible

**Files:**
- Modify: `crates/taiyaku-serve/src/corpus.rs`
- Modify: `scripts/check-all.sh`

**Interfaces:**
- Consumes: everything above.
- Produces: no API. One gated test and one gate change.

- [ ] **Step 1: Write the gated test**

Append to `crates/taiyaku-serve/src/corpus.rs`:

```rust
/// The third of the four seam tests the reader design names.
///
/// `LocalLibrary` and `StumpLibrary` must return the same bytes for the same
/// member and offset. If they ever disagree, a swap shows a different page
/// depending on where the server happened to get it, and nothing else notices.
///
/// Byte-for-byte rather than by `dhash`: five pages were measured identical on
/// 2026-07-31, so Stump does not transcode. If it ever starts, this is the
/// test that says so.
///
/// Gated on `TAIYAKU_CORPUS` **and** a reachable instance, because it is the
/// only test in the suite that talks to one.
#[test]
fn stump_and_local_serve_the_same_bytes_for_the_same_member() {
    let Some(root) = corpus_root() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let (Ok(base), Ok(key)) = (
        std::env::var("TAIYAKU_STUMP_URL"),
        std::env::var("TAIYAKU_STUMP_KEY"),
    ) else {
        eprintln!("TAIYAKU_STUMP_URL or TAIYAKU_STUMP_KEY unset — skipping");
        return;
    };
    let sidecar = root.join("gotoubun-v01/taiyaku.json");
    if !sidecar.exists() {
        eprintln!("{} absent — skipping", sidecar.display());
        return;
    }

    let works = load_works(std::slice::from_ref(&sidecar)).expect("sidecar loads");
    let work = &works[0];
    let dir = sidecar.parent().unwrap();
    let transport = crate::stump::http::UreqTransport::new(base.clone(), key);
    let resolutions = crate::stump::resolve::resolve_work(&transport, work, &base)
        .expect("resolution against the instance");

    let mut compared = 0usize;
    let mut skipped = 0usize;
    for edition in work.sidecar.editions.values() {
        for member in &edition.members {
            let Some(Some(id)) = resolutions.get(&member.sha256) else {
                // Expected, and the finding that shaped this milestone: Stump
                // indexes the English releases and not the raws.
                skipped += 1;
                continue;
            };
            for within in 0..member.pages {
                let ours = LocalLibrary
                    .page(dir, member, within)
                    .unwrap_or_else(|e| panic!("{} page {within}: {e}", member.path));
                let theirs = crate::stump::api::page_bytes(&transport, id, within)
                    .unwrap_or_else(|e| panic!("{} stump page {within}: {e}", member.path));
                assert_eq!(
                    ours.bytes, theirs.bytes,
                    "{} page {within} differs between disk and stump",
                    member.path
                );
                compared += 1;
            }
        }
    }

    eprintln!("{compared} pages compared against stump; {skipped} members stump does not hold");
    assert!(
        compared > 0,
        "no member of this work resolved against stump, so nothing was compared"
    );
}
```

Add the import `use crate::PageSource;` at the top of `corpus.rs` if Task 2 removed it — this test calls `LocalLibrary.page` directly.

- [ ] **Step 2: Run it, gated off and gated on**

Run: `cargo test -p taiyaku-serve corpus 2>&1 | rg 'test result'`

Expected: PASS (both corpus tests skip without the environment).

If you have the corpus and an instance:

```bash
TAIYAKU_CORPUS=corpus \
TAIYAKU_STUMP_URL=https://stump.daveynet.xyz \
TAIYAKU_STUMP_KEY=<key> \
  cargo test -p taiyaku-serve corpus:: -- --nocapture
```

Expected: `187 pages compared against stump; 8 members stump does not hold` — one 187-page member matched, and the raw and seven fan-translation members skipped. The exact numbers depend on the sidecar; the shape is what matters.

- [ ] **Step 3: Make a skipped gate distinguishable from a real one**

`status.md` records that both standing checks print their skip reason to stderr, which `cargo test` captures for passing tests, so a green gate does not say whether anything was measured. This task adds a third gated test and so makes that worse.

In `scripts/check-all.sh`, after the `# --- the `test` job ---` block, add:

```bash
# --- the standing corpus checks ---
# `cargo test` captures stderr for passing tests, so a check that skipped and one
# that measured 617 pages are indistinguishable in the output above. These run
# again with output, or say plainly that they were not run at all. A gate that
# quietly covers less than it claims is the defect this repository exists to
# avoid in the aligner; it should not ship in the gate itself.
if [ -n "${TAIYAKU_CORPUS:-}" ]; then
  run cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
else
  echo
  echo "==> NOT MEASURED: standing corpus checks (TAIYAKU_CORPUS unset)"
fi
```

- [ ] **Step 4: Confirm the gate**

Run: `scripts/check-all.sh --no-wasm 2>&1 | tail -20`

Expected: ends with `all green`, and includes either `NOT MEASURED: standing corpus checks` or the two checks' own output naming what they compared.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/corpus.rs scripts/check-all.sh
git commit -m "Check that disk and Stump serve the same page, and say when nothing was checked

The third of the four seam tests the reader design names. If the two sources
ever disagree, a swap shows a different page depending on where the server got
it, and every sha256 and page count in the sidecar still matches.

Byte-for-byte rather than by dhash: five pages measured identical on
2026-07-31, so this instance does not transcode. If it ever starts, this is
what says so. Members Stump does not hold are counted and skipped rather than
failing — that is most of this library and the finding that shaped M3.

check-all.sh now reruns the standing checks with output, or says plainly that
they were not run. cargo test captures stderr for passing tests, so a check
that skipped and one that compared 617 pages looked identical — and this branch
adds a third gated check, which made it worse. status.md called the fix the
gate's job rather than either test's."
```

---

### Task 10: Say what M3 built

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/notes.md`
- Modify: `README.md`
- Modify: `crates/taiyaku-serve/Cargo.toml` (the `description` field)

- [ ] **Step 1: Update `docs/status.md`**

Every one of these is now false and must change:

1. **"Four crates, 97 tests"** → four crates, **127 tests** (or whatever Step 5 below measures — use the real number, not this one).
2. **"Two of the 97 are standing checks gated on `TAIYAKU_CORPUS`"** → three, and the third also needs `TAIYAKU_STUMP_URL`/`TAIYAKU_STUMP_KEY`. Name what each measures.
3. **The `taiyaku-serve` row** — it now holds OPDS, a Stump client, a composite source and covers, not only "works, sidecar, page by index, progress".
4. **The command list at `:59`** — `serve` gains `[--stump-url URL] [--stump-key KEY] [--log-requests FILE]`.
5. **"A corpus check that skipped and one that ran look identical"** in the known-wrong list → **fixed**, with the fix described.
6. **"Progress is written non-atomically"** → still true for progress, but note the Stump cache does write atomically, so the pattern to copy exists in the tree.
7. **"What comes next" § 3** — "an OPDS feed so a third-party iOS reader can consume a library, and the `PageSource` that proxies Stump" are done. Both carried-forward deferrals — `Edition::locate` and `spawn_blocking` — are done. What remains is M4 (`resolve_slot` + wasm), M5 (the PWA) and M6 (corrections).

Add to the known-wrong list, honestly:

- **A cover is generated from a page that may be 430 MB.** The first request for a cover on such an edition decodes the whole thing. Cached afterwards, but the first one is slow and nothing warns.
- **The request log grows without bound in memory** as well as on disk, because entries are kept for the in-process summary. Fine for a measurement run, wrong for a long-lived server.
- **Stump resolution happens at startup for every member of every work.** A library of many works pays that cost before it serves anything, and nothing reports progress while it does.

- [ ] **Step 2: Update `docs/notes.md`**

Add a section recording the measurements this milestone rests on, in the style of the existing entries — every claim in `status.md` is supposed to trace to one here:

- Stump 0.1.4 nightly, rev `c496517b`; GraphQL at `/api/graphql`; REST bytes at `/api/v2/media/:id/page/:n`, 1-based.
- `Media.hash` and `koreaderHash` null on every book measured; neither is a SHA-256.
- Coverage: of `gotoubun-v01`'s three sidecar editions, Stump holds one; `en-color` is present but not a member; `raw-mono`, `raw-color` and all seven `en-fan` files absent.
- Page agreement: the five-row table from the spec, with the note that page 187 is a PNG served as `image/png`.
- `Media.name` is the file stem without its extension.
- Stump already serves OPDS-PSE, declaring `xmlns:pse`.
- Two series for one work on the instance, which is the organisational shortcoming the OPDS feed answers.

Then, once the Panels run has happened, the performance targets from `scripts/request-summary.sh`.

- [ ] **Step 3: Update `README.md`**

Add a `serve` example showing the Stump flags and the OPDS URL a reader points at:

```sh
# beside Stump: English editions from the instance, raws from disk
TAIYAKU_STUMP_URL=https://stump.example TAIYAKU_STUMP_KEY=… \
  taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr 0.0.0.0:8484

# point an OPDS reader at
#   http://<host>:8484/opds/v1.2/catalog
```

- [ ] **Step 4: Update the crate description**

`crates/taiyaku-serve/Cargo.toml`'s `description` says "HTTP server handing a reader the sidecar, pages, and progress". It now also serves OPDS and proxies Stump.

- [ ] **Step 5: Verify every number**

Run: `cargo test --workspace --all-features 2>&1 | rg 'test result' | awk '{s+=$4} END {print s" tests"}'`

Use that number in `status.md`. Do not copy this plan's estimate.

- [ ] **Step 6: Commit**

```bash
git add docs/status.md docs/notes.md README.md crates/taiyaku-serve/Cargo.toml
git commit -m "Say what M3 built, in the files that claim to say

This repository treats a false statement in a doc as a defect, and a milestone
that added a Stump client, a composite source, an OPDS tree and covers did not
apply that habit to its own documentation.

status.md gains the real test count and the third standing check, records the
corpus-gate fix as done rather than pending, and marks both deferrals carried
into M3 as landed. Three new thin spots go on the known-wrong list rather than
into a scratch file: a cover generated from a 430 MB page, a request log that
grows in memory, and startup resolution that reports no progress.

notes.md gains the measurements everything above traces to — what the instance
is, what it exposes, which members it holds, and the five pages that agree."
```

---

## Self-review

**Spec coverage.** Every section of `2026-07-31-m3-stump-opds-design.md` maps to a task:

| Spec section | Task |
| ------------ | ---- |
| Composite source, resolved per member | 5 |
| `PageSource` narrows; arithmetic to `taiyaku-core` | 1, 2 |
| Matched by name and size, confirmed by page hash | 3, 4 |
| Cached beside the sidecar | 4 |
| Service API key | 3, 5 |
| OPDS tree groups editions | 6 |
| Two off-by-one boundaries, each tested | 3 (Stump), 6 (PSE) |
| Covers now, filmstrip in M5 | 7 |
| `spawn_blocking` at the router | 5 |
| `PageSource` equivalence test | 9 |
| Corpus gate fix | 9 |
| Request log and its script | 8 |
| Risks 1, 3, 4 (no hash / blocking / configured base) | 3, 4, 5 |
| Risk 2 (ordering is agreement) | 9 |
| Risk 5 (coverage moves) | 4 (`base` recorded), 10 (documented) |

The Panels measurement itself (spec's "second deliverable") is deliberately **not** a task: it needs an iPad and a running server, so it is a manual step after Task 10, recorded in `notes.md` via `scripts/request-summary.sh`.

**Type consistency.** `PageSource::page(&self, root: &Path, member: &Member, within: u32)` is used identically in Tasks 2, 4, 5 and 9. `Resolutions` is `HashMap<String, Option<String>>` keyed by member `sha256` in Tasks 4, 5 and 9. **`api::page_bytes(t, id, zero_based)` takes taiyaku's own 0-based index and converts to Stump's 1-based numbering internally** — so Tasks 5 and 9 pass `within` straight through and no call site adds one. (Task 3 first shipped this taking `one_based`, with the conversion at each call site; its review found the test pinning that boundary was an identity check on `format!`, and the signature moved in the fix wave at `2a9ccff`.) `ServeState` gains `cache_dir` in Task 7 and `log` in Task 8, and both tasks list every construction site that must be updated (`serve`, `test_support::state`, `corpus.rs`).

**Known ordering constraint.** Task 7 and Task 8 each add a field to `ServeState`, so both touch `test_support::state` and `corpus.rs`. Run them in order; doing 8 before 7 means fixing the same three call sites twice.

# The Instrument: Ground Truth, Labeller and Scorer — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the instrument that can judge a text-block detector — a hand-labelled ground-truth format, a browser labeller that draws on the page image, and a scorer that reports five outcomes per truth box instead of an IoU pass/fail.

**Architecture:** The document format lives in `taiyaku-core` beside `regions.rs` and follows its conventions exactly (sibling file, content-hash staleness, absent ≠ empty). `taiyaku-serve` gains `GET`/`PUT /truth` mirroring its notes routes, including the `If-Match` requirement O35 exists for. The labeller is a **second Vite entry** in `web/`, reusing `Viewport`/`PageView`/`pageSize` unchanged, with all box-editing logic in a pure `labelling.ts` so it tests in the fast `node` project. The scorer lives in `taiyaku-cli`.

**Tech Stack:** Rust (axum, serde, serde_json) for core/cli/serve; TypeScript + React + Vite + Vitest for `web/`.

**Spec:** `docs/superpowers/specs/2026-08-11-l2-ground-truth-instrument-design.md`

## Global Constraints

- **`crates/taiyaku-cli/src/detect.rs` is not modified. Not one line.** That is sub-project B. A plan step that edits it is a plan failure.
- **No new workspace dependencies.** The dependency set is deliberately short. Everything here uses `serde`, `serde_json`, `axum` and `image`, all already present.
- **`taiyaku-core` must compile to `wasm32-unknown-unknown`.** CI enforces it. No filesystem or native-only calls in `truth.rs` beyond `std::path` types, exactly as `regions.rs` manages today.
- **A constant asserted as measured when it is not is a defect.** The scorer's `COVERAGE` constant ships with a doc comment saying it is provisional, in the same words `SMEAR_K`'s uses.
- **Commit messages carry no attribution.** No `Co-Authored-By`, no `Generated with`.
- **Ordering is deliberate:** Tasks 1–5 deliver the labeller so the human can begin the ~3 hours of drawing while Tasks 6–9 build the scorer. **Task 9 Step 1 is the one exception and depends on nothing** — it fixes which pages are the dev set, so do it as soon as Task 5 lands rather than in sequence, or the drawing has no list to follow.

---

### Task 1: The truth document format in `taiyaku-core`

**Files:**
- Create: `crates/taiyaku-core/src/truth.rs`
- Modify: `crates/taiyaku-core/src/lib.rs:11-16` (module list), `:20-22` (re-exports)

**Interfaces:**
- Consumes: `Axis` and `RegionMember` from `crates/taiyaku-core/src/regions.rs:41,69`.
- Produces: `TruthDoc`, `TruthEdition`, `TruthRegion`, `Kind`, `TRUTH_VERSION`. `TruthDoc::path_beside(&Path) -> PathBuf`, `TruthDoc::from_json(&str) -> Result<Self, String>`, `TruthDoc::to_json(&self) -> String`, `TruthEdition::is_stale_for(&[String]) -> bool`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-core/src/truth.rs` (create the file with just this `mod tests` plus `use super::*;` for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> TruthDoc {
        let mut pages = BTreeMap::new();
        pages.insert(
            7,
            vec![
                TruthRegion {
                    x: 0.10, y: 0.20, w: 0.15, h: 0.30,
                    axis: Some(Axis::Vertical),
                    kind: Kind::Dialogue,
                    ignore: false,
                    text: None,
                },
                TruthRegion {
                    x: 0.55, y: 0.40, w: 0.28, h: 0.12,
                    axis: None,
                    kind: Kind::Sfx,
                    ignore: true,
                    text: None,
                },
            ],
        );
        // Present and empty: somebody looked at page 8 and there was no text.
        pages.insert(8, Vec::new());
        let mut editions = BTreeMap::new();
        editions.insert(
            "raw-mono".to_string(),
            TruthEdition {
                members: vec![RegionMember { sha256: "aa".into() }],
                pages,
            },
        );
        TruthDoc { taiyaku_truth: VERSION, editions }
    }

    #[test]
    fn a_document_survives_a_round_trip() {
        let there_and_back = TruthDoc::from_json(&doc().to_json()).unwrap();
        assert_eq!(there_and_back.taiyaku_truth, VERSION);
        let edition = &there_and_back.editions["raw-mono"];
        assert_eq!(edition.pages[&7].len(), 2);
        assert_eq!(edition.pages[&7][0].axis, Some(Axis::Vertical));
        assert_eq!(edition.pages[&7][0].kind, Kind::Dialogue);
        assert!(!edition.pages[&7][0].ignore);
        assert_eq!(edition.pages[&7][1].axis, None);
        assert!(edition.pages[&7][1].ignore);
    }

    #[test]
    fn a_page_looked_at_and_a_page_never_opened_are_different() {
        let there_and_back = TruthDoc::from_json(&doc().to_json()).unwrap();
        let pages = &there_and_back.editions["raw-mono"].pages;
        assert!(pages.contains_key(&8), "page 8 was looked at");
        assert!(pages[&8].is_empty(), "and held no text");
        assert!(!pages.contains_key(&9), "page 9 was never opened");
    }

    #[test]
    fn an_axis_may_be_null_because_a_radial_effect_has_no_reading_direction() {
        let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":null,"kind":"sfx"}]}}}}"#;
        let parsed = TruthDoc::from_json(json).unwrap();
        assert_eq!(parsed.editions["e"].pages[&3][0].axis, None);
    }

    #[test]
    fn ignore_and_text_default_when_absent() {
        let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":"v","kind":"dialogue"}]}}}}"#;
        let region = TruthDoc::from_json(json).unwrap().editions["e"].pages[&3][0];
        assert!(!region.ignore);
        assert_eq!(region.text, None);
    }

    #[test]
    fn an_unknown_kind_is_refused_and_the_message_names_the_field() {
        let json = r#"{"taiyaku_truth":1,"editions":{"e":{"members":[],
            "pages":{"3":[{"x":0.0,"y":0.0,"w":0.1,"h":0.1,
            "axis":"v","kind":"balloon"}]}}}}"#;
        let err = TruthDoc::from_json(json).unwrap_err();
        assert!(err.contains("kind"), "message should name the field: {err}");
    }

    #[test]
    fn a_replaced_archive_makes_a_document_stale() {
        let edition = &doc().editions["raw-mono"];
        assert!(!edition.is_stale_for(&["aa".to_string()]));
        assert!(edition.is_stale_for(&["bb".to_string()]), "content changed");
        assert!(edition.is_stale_for(&[]), "member count changed");
    }

    #[test]
    fn the_document_sits_beside_the_sidecar_keeping_a_dotted_stem() {
        assert_eq!(
            TruthDoc::path_beside(Path::new("/c/taiyaku.json")),
            Path::new("/c/taiyaku.truth.json")
        );
        assert_eq!(
            TruthDoc::path_beside(Path::new("/c/go.between.json")),
            Path::new("/c/go.between.truth.json"),
            "set_extension twice would eat the dot"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-core truth`
Expected: FAIL to compile — `cannot find type TruthDoc in this scope`.

- [ ] **Step 3: Write the implementation**

Prepend to `crates/taiyaku-core/src/truth.rs`, above the `mod tests` block:

```rust
//! The `<stem>.truth.json` format: text blocks a person drew, per edition, per page.
//!
//! A sibling document, the convention `<stem>.regions.json` and
//! `<stem>.notes.json` already use, for the reason `regions.rs` gives: this is
//! two orders of magnitude more JSON than `taiyaku.json`, whose first two
//! design goals are human-readable and diffable in git.
//!
//! **This is what a person drew, not what a detector emitted.** The two are
//! different claims and this file is deliberately not `regions.rs`'s `Region`
//! with a field bolted on: a person can decline to answer the axis question —
//! a radial sound effect has no reading direction — and `Region::axis` is
//! `Axis`, not `Option<Axis>`. Widening that would change what every existing
//! regions document means.
//!
//! Nothing here scores anything. Comparing a detector's regions against these
//! is `taiyaku-cli`'s `score.rs`, which is native-only because nothing in the
//! browser has a use for it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::regions::{Axis, RegionMember};

/// Bumped when an existing field would mean something else.
///
/// Adding a field does not need it: this document obeys the same
/// ignore-unknown-keys rule the sidecar does, which is what lets `text` below
/// be filled in later by an OCR pass without a bump and without relabelling.
pub const VERSION: u32 = 1;

/// What a labelled box holds.
///
/// A closed set rather than a free string, so a typo is a parse error rather
/// than a silently new category that reports zero recall forever. `Other`
/// carries page numbers, publisher marks and anything else nobody wanted a
/// name for; the `ignore` flag, not this variant, is what excuses a box from
/// scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    #[serde(rename = "dialogue")]
    Dialogue,
    #[serde(rename = "narration")]
    Narration,
    #[serde(rename = "sfx")]
    Sfx,
    #[serde(rename = "logo")]
    Logo,
    #[serde(rename = "credits")]
    Credits,
    #[serde(rename = "other")]
    Other,
}

/// One text run a person drew, in page fractions.
///
/// Fractions rather than pixels for `Region`'s reason: page widths run 685 to
/// 5343 pixels across editions of the same work, so a pixel box means nothing
/// without the page it was measured on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TruthRegion {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// `None` where the question does not apply.
    ///
    /// A vertical column and a horizontal line have a reading direction; a
    /// radial sound effect exploding across a panel does not, and a labeller
    /// forced to pick one would be recording a coin toss as data.
    pub axis: Option<Axis>,
    pub kind: Kind,
    /// Neither a hit nor a false positive, and counted out loud.
    ///
    /// For the genuinely ambiguous tail — signage baked into background art,
    /// text on clothing, marks illegible at scan resolution. It exists because
    /// *deciding* is the expensive part of labelling, not drawing, and the
    /// scorer prints how many boxes carried this so the hatch is never silent.
    #[serde(default)]
    pub ignore: bool,
    /// Reserved. An OCR pass fills this in later.
    ///
    /// Additive, so adding it costs no version bump and no relabelling — the
    /// whole reason it is named here while nothing writes it.
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TruthEdition {
    /// In the edition's own member order, because that order is page order.
    pub members: Vec<RegionMember>,
    /// Page index in the edition's flat sequence, to its boxes.
    ///
    /// An absent key means nobody has looked at that page; a present empty
    /// value means somebody looked and found no text. Across 85 pages of
    /// manual work that difference is the progress tracker, and it is the same
    /// distinction `RegionEdition::pages` refuses to collapse.
    pub pages: BTreeMap<u32, Vec<TruthRegion>>,
}

impl TruthEdition {
    /// Whether these labels describe an archive that is no longer there.
    ///
    /// Ground truth has the sharpest form of this problem in the project. A
    /// stale alignment sends a reader to the wrong page; stale ground truth
    /// draws a box on the wrong art on the *right* page and then scores a
    /// detector against it. Content and order, not paths.
    #[must_use]
    pub fn is_stale_for(&self, digests: &[String]) -> bool {
        self.members.len() != digests.len()
            || self
                .members
                .iter()
                .zip(digests)
                .any(|(member, digest)| &member.sha256 != digest)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthDoc {
    pub taiyaku_truth: u32,
    /// Edition key, exactly as the sidecar spells it.
    #[serde(default)]
    pub editions: BTreeMap<String, TruthEdition>,
}

impl TruthDoc {
    #[must_use]
    pub fn new() -> Self {
        Self { taiyaku_truth: VERSION, editions: BTreeMap::new() }
    }

    /// Where this document lives for a given sidecar: same directory, same stem.
    ///
    /// Built from `file_stem` and a `format!`, not `set_extension` twice —
    /// which loses a dot in the stem rather than keeping it. The arithmetic
    /// `RegionDoc::path_beside` and `Work::sibling` already carry, and the
    /// reason each states.
    #[must_use]
    pub fn path_beside(sidecar: &Path) -> PathBuf {
        let stem = sidecar
            .file_stem()
            .map_or_else(|| "taiyaku".to_string(), |s| s.to_string_lossy().to_string());
        sidecar
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}.truth.json"))
    }

    /// # Errors
    ///
    /// Returns an error if `text` is not valid JSON, or is JSON that does not
    /// match this document's shape — a missing `taiyaku_truth` field, a `kind`
    /// outside the closed set, an `axis` other than `"v"`, `"h"` or `null`, a
    /// `pages` entry keyed by something other than a page number. The message
    /// embeds `serde_json`'s own, which names the field and byte offset.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("not a readable truth document: {e}"))
    }

    /// # Panics
    ///
    /// Panics if this document fails to serialise. In practice it cannot:
    /// every field is a plain number, string, `Option`, `BTreeMap` or enum
    /// with no custom `Serialize` impl, so the only way `serde_json` returns
    /// `Err` here — a non-finite float used as a map key — does not apply.
    /// The float fields are values, not keys, and `u32` page keys are finite.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("truth is always serialisable")
    }
}

impl Default for TruthDoc {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Wire the module into the crate**

In `crates/taiyaku-core/src/lib.rs`, add `mod truth;` to the module list (keep alphabetical — it goes after `mod solve;`), and add this re-export after the `regions` one:

```rust
pub use truth::{
    Kind, TruthDoc, TruthEdition, TruthRegion, VERSION as TRUTH_VERSION,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-core truth`
Expected: PASS, 7 tests.

- [ ] **Step 6: Confirm the wasm constraint still holds**

Run: `cargo check -p taiyaku-core --target wasm32-unknown-unknown`
Expected: finishes with no error. If the target is missing, `rustup target add wasm32-unknown-unknown` first.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-core/src/truth.rs crates/taiyaku-core/src/lib.rs
git commit -m "core: the truth document, which is what a person drew

Not Region with a field added: a person can decline to answer the axis
question and a detector cannot, so axis is Option<Axis> here. Carries the
staleness check and the absent-is-not-empty page map from regions.rs for
the reasons those state, and reserves a text field so a later OCR pass
costs no relabelling."
```

---

### Task 2: `GET`/`PUT /truth` in `taiyaku-serve`

**Files:**
- Create: `crates/taiyaku-serve/src/truth.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (add `mod truth;`), `crates/taiyaku-serve/src/routes.rs` (handlers + `:1082` router table)

**Interfaces:**
- Consumes: `TruthDoc` from Task 1. `Work::sibling` (`works.rs:95`), `Work::notes_guard` (`works.rs:66`), `if_match_hits`/`if_none_match_hits`/`not_modified`/`bad_request` (`routes.rs`).
- Produces: `truth::truth_path(&Work) -> PathBuf`, `truth::load(&Work) -> Result<Option<TruthDoc>, String>`, `truth::store(&Work, &TruthDoc) -> Result<(), String>`, `truth::to_json(&TruthDoc) -> String`, and the routes `GET|PUT /api/works/{work}/truth`.

- [ ] **Step 1: Write the failing route tests**

Add to the existing `mod tests` in `crates/taiyaku-serve/src/routes.rs`. The helpers `etag_of` (`:2252`) and the router-building fixture already there are what these use — follow the surrounding tests' construction of a `state` exactly.

```rust
#[tokio::test]
async fn a_work_nobody_has_labelled_answers_an_empty_document_with_a_tag() {
    let state = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/works/w/truth")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key(header::ETAG),
        "a client's first write needs a validator to be conditional on"
    );
}

#[tokio::test]
async fn writing_truth_without_if_match_is_refused() {
    let state = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let body = taiyaku_core::TruthDoc::new().to_json();
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/api/works/w/truth")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
}

#[tokio::test]
async fn a_successful_truth_write_returns_an_etag() {
    // O35: a PUT response with no ETag turns every later write into a silent
    // no-op. The notes route was fixed for exactly this; truth has the same
    // shape and must not repeat it.
    let state = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let tag = etag_of(&router, "/api/works/w/truth").await;
    let body = taiyaku_core::TruthDoc::new().to_json();
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", tag)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key(header::ETAG),
        "O35: without this every later write is a silent no-op"
    );
}

#[tokio::test]
async fn a_stale_validator_is_refused_with_the_current_tag() {
    let state = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let body = taiyaku_core::TruthDoc::new().to_json();
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", "\"not-the-current-tag\"")
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
}

#[tokio::test]
async fn a_malformed_truth_body_is_refused_in_this_servers_own_words() {
    let state = crate::test_support::state_with_real_pages();
    let router = build_router(state);
    let tag = etag_of(&router, "/api/works/w/truth").await;
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri("/api/works/w/truth")
                .header("content-type", "application/json")
                .header("if-match", tag)
                .body(axum::body::Body::from("{\"nonsense\":true}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-serve truth`
Expected: FAIL — the route is not in the table, so `GET /truth` answers 404 and the first assertion trips.

- [ ] **Step 3: Write the truth module**

Create `crates/taiyaku-serve/src/truth.rs`:

```rust
//! Reading and writing `<stem>.truth.json` for a served work.
//!
//! The shape `notes.rs` already has, and deliberately not a second convention:
//! one serialisation shared by the file, the response body and the `ETag` over
//! that body, so a validator can never be computed over bytes a client was not
//! sent — the failure that would make a conditional write silently
//! unconditional.

use std::path::PathBuf;

use taiyaku_core::TruthDoc;

use crate::works::Work;

/// Where a work's truth document sits: beside its sidecar, same stem.
pub fn truth_path(work: &Work) -> PathBuf {
    work.sibling("truth.json")
}

/// The one serialisation of a truth document.
///
/// `notes::to_json`'s reasoning, unchanged: shared by disk, wire and
/// validator so the three cannot drift.
#[must_use]
pub fn to_json(doc: &TruthDoc) -> String {
    doc.to_json()
}

/// # Errors
///
/// Returns an error if the file exists but cannot be read or parsed. A file
/// that is not there is `Ok(None)` — nobody has labelled this work yet, which
/// is not a failure.
pub fn load(work: &Work) -> Result<Option<TruthDoc>, String> {
    let path = truth_path(work);
    match std::fs::read_to_string(&path) {
        Ok(text) => TruthDoc::from_json(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// # Errors
///
/// Returns an error if the document cannot be written.
///
/// Atomic, via `crate::atomic::write`: a labelling session represents hours of
/// irreplaceable human work, and a truncated write that loses it is the defect
/// O26 was.
pub fn store(work: &Work, doc: &TruthDoc) -> Result<(), String> {
    crate::atomic::write(&truth_path(work), to_json(doc).as_bytes())
}
```

If `crate::atomic::write` is not reachable from `taiyaku-serve`, use the same
atomic write helper `notes::store` (`notes.rs:314`) calls and match its call
shape exactly rather than introducing a second one.

- [ ] **Step 4: Add the handlers and the route**

In `crates/taiyaku-serve/src/routes.rs`, add beside `get_notes`/`put_notes` (`:580`, `:636`) two handlers that mirror them exactly — same `spawn_blocking` wrapper for the reason O12 gives, same `notes_guard` held across read-compare-write, same `If-Match` arms with `PRECONDITION_REQUIRED` and `PRECONDITION_FAILED`, same `Bytes`-then-deserialise so a malformed body is refused in this server's own words (O5's rule).

Add the `ETag` helper next to `notes_etag` (`:238`):

```rust
/// A truth document's `ETag`: a SHA-256 over exactly the bytes being sent.
///
/// Takes the serialised text rather than the document, for `notes_etag`'s
/// reason: both callers already hold it, and hashing a second serialisation
/// built here could differ from the one sent.
pub(crate) fn truth_etag(json: &str) -> String {
    notes_etag(json)
}
```

Then add to the router table at `routes.rs:1082`, directly after the notes line:

```rust
        .route("/api/works/{work}/truth", get(get_truth).put(put_truth))
```

And `mod truth;` to `crates/taiyaku-serve/src/lib.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve truth`
Expected: PASS, 5 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/truth.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/lib.rs
git commit -m "serve: GET and PUT for the truth document

Mirrors the notes routes rather than inventing a second convention: one
serialisation behind disk, wire and validator, the guard held across
read-compare-write, and If-Match required. The PUT answers with an ETag,
which is O35 stated as a test rather than rediscovered."
```

---

### Task 3: The labelling logic, pure and testable

**Files:**
- Create: `web/src/labelling.ts`, `web/src/labelling.test.ts`

**Interfaces:**
- Consumes: `ulid()` from `web/src/ulid.ts`.
- Produces: `LabelBox` (`{id, x, y, w, h, kind, axis, ignore}`), `Kind`, `BoxAxis`; and `addBox`, `setKind`, `setAxis`, `toggleIgnore`, `removeBox`, `boxesAt`, `cycleAt`.

**Why pure:** this is the codebase's established split — `pairing.ts`, `keyplan.ts` and `pageSize.ts` hold logic with no DOM so they run in the fast `node` project, and the components stay thin. The `jsdom` and `browser` projects then only have to cover what genuinely needs a DOM.

- [ ] **Step 1: Write the failing tests**

Create `web/src/labelling.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import {
  addBox, boxesAt, cycleAt, removeBox, setAxis, setKind, toggleIgnore,
  type LabelBox,
} from './labelling'

const box = (id: string, x: number, y: number, w = 0.1, h = 0.1): LabelBox => ({
  id, x, y, w, h, kind: 'dialogue', axis: 'v', ignore: false,
})

describe('addBox', () => {
  it('appends a box with a fresh id and the default kind', () => {
    const after = addBox([], { x: 0.1, y: 0.2, w: 0.3, h: 0.4 })
    expect(after).toHaveLength(1)
    expect(after[0]).toMatchObject({ x: 0.1, y: 0.2, w: 0.3, h: 0.4, kind: 'dialogue', ignore: false })
    expect(after[0].id).toBeTruthy()
  })

  it('normalises a rectangle dragged up and to the left', () => {
    // Dragging from bottom-right to top-left gives a negative width. A box
    // with a negative extent would fail every containment test downstream.
    const after = addBox([], { x: 0.5, y: 0.6, w: -0.2, h: -0.3 })
    expect(after[0]).toMatchObject({ x: 0.3, y: 0.3, w: 0.2, h: 0.3 })
  })

  it('refuses a degenerate drag rather than storing a zero-area box', () => {
    expect(addBox([], { x: 0.5, y: 0.5, w: 0, h: 0.2 })).toHaveLength(0)
  })
})

describe('editing', () => {
  it('sets a kind on one box and leaves the others', () => {
    const before = [box('a', 0, 0), box('b', 0.5, 0.5)]
    const after = setKind(before, 'a', 'sfx')
    expect(after[0].kind).toBe('sfx')
    expect(after[1].kind).toBe('dialogue')
  })

  it('sets an axis, including not-applicable', () => {
    expect(setAxis([box('a', 0, 0)], 'a', null)[0].axis).toBeNull()
    expect(setAxis([box('a', 0, 0)], 'a', 'h')[0].axis).toBe('h')
  })

  it('toggles ignore both ways', () => {
    const once = toggleIgnore([box('a', 0, 0)], 'a')
    expect(once[0].ignore).toBe(true)
    expect(toggleIgnore(once, 'a')[0].ignore).toBe(false)
  })

  it('removes by id', () => {
    expect(removeBox([box('a', 0, 0), box('b', 0.5, 0.5)], 'a')).toHaveLength(1)
  })

  it('does not mutate its input', () => {
    const before = [box('a', 0, 0)]
    setKind(before, 'a', 'logo')
    expect(before[0].kind).toBe('dialogue')
  })
})

describe('cycleAt', () => {
  // Sound effects routinely sit on top of balloons. Without a way through the
  // stack the box underneath cannot be selected at all, which is why there is
  // no sidebar list to fall back on.
  const stack = [box('under', 0.1, 0.1, 0.4, 0.4), box('over', 0.2, 0.2, 0.1, 0.1)]

  it('finds every box containing the point, topmost first', () => {
    expect(boxesAt(stack, { x: 0.25, y: 0.25 }).map((b) => b.id)).toEqual(['over', 'under'])
  })

  it('starts at the topmost when nothing is selected', () => {
    expect(cycleAt(stack, { x: 0.25, y: 0.25 }, undefined)).toBe('over')
  })

  it('steps to the next box down, then wraps', () => {
    expect(cycleAt(stack, { x: 0.25, y: 0.25 }, 'over')).toBe('under')
    expect(cycleAt(stack, { x: 0.25, y: 0.25 }, 'under')).toBe('over')
  })

  it('answers undefined where no box holds the point', () => {
    expect(cycleAt(stack, { x: 0.9, y: 0.9 }, undefined)).toBeUndefined()
  })

  it('starts over when the selected box does not hold the point', () => {
    expect(cycleAt(stack, { x: 0.15, y: 0.15 }, 'over')).toBe('under')
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && pnpm vitest run --project node labelling`
Expected: FAIL — `Failed to resolve import "./labelling"`.

- [ ] **Step 3: Write the implementation**

Create `web/src/labelling.ts`:

```ts
import { ulid } from './ulid'

/** What a labelled box holds. Mirrors `taiyaku-core`'s `Kind` exactly. */
export type Kind = 'dialogue' | 'narration' | 'sfx' | 'logo' | 'credits' | 'other'

/**
 * A reading direction, or `null` where the question does not apply.
 *
 * A radial sound effect has no reading direction, and forcing a labeller to
 * pick one records a coin toss as data.
 */
export type BoxAxis = 'v' | 'h' | null

export interface LabelBox {
  id: string
  /** Page fractions, matching `TruthRegion`. */
  x: number
  y: number
  w: number
  h: number
  kind: Kind
  axis: BoxAxis
  ignore: boolean
}

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export interface Point {
  x: number
  y: number
}

/**
 * Add a box from a dragged rectangle.
 *
 * The rectangle is normalised because a drag that runs up and to the left
 * gives negative extents, and a box with a negative width fails every
 * containment test downstream while looking fine in the JSON. A drag with no
 * area at all is a click that missed and is dropped rather than stored.
 */
export function addBox(boxes: readonly LabelBox[], rect: Rect): LabelBox[] {
  const x = Math.min(rect.x, rect.x + rect.w)
  const y = Math.min(rect.y, rect.y + rect.h)
  const w = Math.abs(rect.w)
  const h = Math.abs(rect.h)
  if (w === 0 || h === 0) return [...boxes]
  return [...boxes, { id: ulid(), x, y, w, h, kind: 'dialogue', axis: 'v', ignore: false }]
}

const edit = (
  boxes: readonly LabelBox[],
  id: string,
  change: (box: LabelBox) => LabelBox,
): LabelBox[] => boxes.map((box) => (box.id === id ? change(box) : box))

export const setKind = (boxes: readonly LabelBox[], id: string, kind: Kind): LabelBox[] =>
  edit(boxes, id, (box) => ({ ...box, kind }))

export const setAxis = (boxes: readonly LabelBox[], id: string, axis: BoxAxis): LabelBox[] =>
  edit(boxes, id, (box) => ({ ...box, axis }))

export const toggleIgnore = (boxes: readonly LabelBox[], id: string): LabelBox[] =>
  edit(boxes, id, (box) => ({ ...box, ignore: !box.ignore }))

export const removeBox = (boxes: readonly LabelBox[], id: string): LabelBox[] =>
  boxes.filter((box) => box.id !== id)

const holds = (box: LabelBox, at: Point): boolean =>
  at.x >= box.x && at.x <= box.x + box.w && at.y >= box.y && at.y <= box.y + box.h

/**
 * Every box containing the point, topmost first.
 *
 * Later boxes draw on top of earlier ones, so the array is reversed: what a
 * reader sees on top is what a click should reach first.
 */
export const boxesAt = (boxes: readonly LabelBox[], at: Point): LabelBox[] =>
  boxes.filter((box) => holds(box, at)).reverse()

/**
 * The next box down the stack under the cursor, wrapping at the bottom.
 *
 * Sound effects routinely overlap balloons and panel borders, so a click that
 * always takes the topmost box makes the one underneath unreachable. This is
 * what replaces a sidebar list: the boxes stay on the image and Tab walks
 * through the ones that overlap.
 *
 * A selection that is not under the cursor starts the walk over rather than
 * answering `undefined` — the cursor has moved to a different stack, and the
 * previous selection says nothing about where in this one to begin.
 */
export function cycleAt(
  boxes: readonly LabelBox[],
  at: Point,
  selected: string | undefined,
): string | undefined {
  const under = boxesAt(boxes, at)
  if (under.length === 0) return undefined
  const index = under.findIndex((box) => box.id === selected)
  if (index === -1) return under[0].id
  return under[(index + 1) % under.length].id
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && pnpm vitest run --project node labelling`
Expected: PASS, 13 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/labelling.ts web/src/labelling.test.ts
git commit -m "web: the labelling logic, with no DOM in it

The split pairing.ts and keyplan.ts already follow, so this runs in the
fast node project. Two cases worth naming: a drag up and to the left is
normalised, because a negative width looks fine in JSON and fails every
containment test after it; and cycleAt is what makes an overlapped box
reachable without the sidebar list the design refused."
```

---

### Task 4: `RegionOverlay`, the component the reader will also use

**Files:**
- Create: `web/src/RegionOverlay.tsx`, `web/src/RegionOverlay.test.tsx`

**Interfaces:**
- Consumes: `LabelBox` from Task 3.
- Produces: `RegionOverlay` and `RegionOverlayProps` (`{boxes, selectedId, onHover, onSelect}`).

- [ ] **Step 1: Write the failing tests**

Create `web/src/RegionOverlay.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { RegionOverlay } from './RegionOverlay'
import type { LabelBox } from './labelling'

const boxes: LabelBox[] = [
  { id: 'a', x: 0.1, y: 0.2, w: 0.3, h: 0.4, kind: 'dialogue', axis: 'v', ignore: false },
  { id: 'b', x: 0.5, y: 0.5, w: 0.2, h: 0.2, kind: 'sfx', axis: null, ignore: true },
]

describe('RegionOverlay', () => {
  it('positions each box in page fractions', () => {
    render(<RegionOverlay boxes={boxes} />)
    const drawn = screen.getByTestId('region-a')
    expect(drawn.style.left).toBe('10%')
    expect(drawn.style.top).toBe('20%')
    expect(drawn.style.width).toBe('30%')
    expect(drawn.style.height).toBe('40%')
  })

  it('marks the selected box so it can be styled apart', () => {
    render(<RegionOverlay boxes={boxes} selectedId="b" />)
    expect(screen.getByTestId('region-b')).toHaveAttribute('data-selected', 'true')
    expect(screen.getByTestId('region-a')).toHaveAttribute('data-selected', 'false')
  })

  it('carries the kind and the ignore flag as data, not as colour alone', () => {
    // A colour difference is invisible to a test and to anyone who cannot see
    // it. The attribute is what both read.
    render(<RegionOverlay boxes={boxes} />)
    expect(screen.getByTestId('region-b')).toHaveAttribute('data-kind', 'sfx')
    expect(screen.getByTestId('region-b')).toHaveAttribute('data-ignore', 'true')
  })

  it('reports hover so the box can brighten out of the way of the artwork', async () => {
    const onHover = vi.fn()
    render(<RegionOverlay boxes={boxes} onHover={onHover} />)
    await userEvent.hover(screen.getByTestId('region-a'))
    expect(onHover).toHaveBeenCalledWith('a')
    await userEvent.unhover(screen.getByTestId('region-a'))
    expect(onHover).toHaveBeenCalledWith(undefined)
  })

  it('reports a click as a selection', async () => {
    const onSelect = vi.fn()
    render(<RegionOverlay boxes={boxes} onSelect={onSelect} />)
    await userEvent.click(screen.getByTestId('region-a'))
    expect(onSelect).toHaveBeenCalledWith('a')
  })

  it('draws nothing at all when there are no boxes', () => {
    const { container } = render(<RegionOverlay boxes={[]} />)
    expect(container.querySelectorAll('[data-testid^="region-"]')).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && pnpm vitest run --project jsdom RegionOverlay`
Expected: FAIL — `Failed to resolve import "./RegionOverlay"`.

- [ ] **Step 3: Write the component**

Create `web/src/RegionOverlay.tsx`:

```tsx
import type { LabelBox } from './labelling'

export interface RegionOverlayProps {
  boxes: readonly LabelBox[]
  selectedId?: string
  onHover?: (id: string | undefined) => void
  onSelect?: (id: string) => void
}

/**
 * Boxes drawn over a page image, in page fractions.
 *
 * **Two consumers, which is why this is its own component.** The labeller
 * creates and edits through it; the reader will later select through it for
 * the peek. Building it once is the reason the labeller is a second Vite
 * entry rather than a mode inside `App.tsx`.
 *
 * Percentages rather than pixels, so the overlay follows the page under zoom
 * and pan without recomputing anything — the same property that lets
 * `Annotations.tsx` position markers over artwork.
 *
 * Kind and ignore ride as data attributes rather than colour alone: a colour
 * difference is invisible to a test and to a reader who cannot distinguish
 * them.
 */
export function RegionOverlay({ boxes, selectedId, onHover, onSelect }: RegionOverlayProps) {
  return (
    <div className="region-overlay">
      {boxes.map((box) => (
        <div
          key={box.id}
          data-testid={`region-${box.id}`}
          data-kind={box.kind}
          data-ignore={String(box.ignore)}
          data-selected={String(box.id === selectedId)}
          className="region-box"
          style={{
            position: 'absolute',
            left: `${box.x * 100}%`,
            top: `${box.y * 100}%`,
            width: `${box.w * 100}%`,
            height: `${box.h * 100}%`,
          }}
          onMouseEnter={() => onHover?.(box.id)}
          onMouseLeave={() => onHover?.(undefined)}
          onClick={() => onSelect?.(box.id)}
        />
      ))}
    </div>
  )
}
```

Add to `web/src/index.css`:

```css
/* Out of the way until you go for them: the requirement that killed the
   sidebar box list. A box is a thin outline at low opacity until hovered or
   selected, so the artwork underneath stays readable while labelling. */
.region-box {
  border: 1px solid currentColor;
  opacity: 0.15;
  transition: opacity 80ms ease-out;
}
.region-box:hover,
.region-box[data-selected='true'] {
  opacity: 1;
}
.region-box[data-ignore='true'] {
  border-style: dashed;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && pnpm vitest run --project jsdom RegionOverlay`
Expected: PASS, 6 tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/RegionOverlay.tsx web/src/RegionOverlay.test.tsx web/src/index.css
git commit -m "web: the box overlay, built once for two consumers

The labeller edits through it now and the reader selects through it later,
which is the whole reason the labeller is a second entry rather than a mode
inside App.tsx. Boxes sit at low opacity until hovered -- out of the way
until you go for them."
```

---

### Task 5: The labeller, and a second Vite entry

**Files:**
- Create: `web/label.html`, `web/src/label.tsx`, `web/src/Labeller.tsx`, `web/src/Labeller.test.tsx`, `web/src/Labeller.browser.test.tsx`
- Modify: `web/vite.config.ts` (add `build.rollupOptions.input`), `web/src/api.ts` (truth client)

**Interfaces:**
- Consumes: `labelling.ts` (Task 3), `RegionOverlay` (Task 4), `Viewport`/`PageView`/`pageSize` unchanged, `GET|PUT /api/works/{work}/truth` (Task 2).
- Produces: `getTruth`/`putTruth` in `api.ts`; the `/label.html` entry.

- [ ] **Step 1: Add the truth client to `api.ts`**

Append to `web/src/api.ts`, following `getNotes`/`putNotes` (`:139`, `:171`) exactly — including the `etag` being `undefined` where the header was absent, which `writeGate` is what runs into:

```ts
export interface FetchedTruth {
  doc: TruthDoc
  /** Absent only from a server older than the truth route, or a proxy that stripped it. */
  etag?: string
}

/** Every labelled box on a work. */
export async function getTruth(work: string): Promise<FetchedTruth> {
  const res = await fetch(`/api/works/${seg(work)}/truth`)
  if (!res.ok) throw new Error(`reading truth: ${res.status} ${await res.text()}`)
  return {
    doc: (await res.json()) as TruthDoc,
    etag: res.headers.get('etag') ?? undefined,
  }
}

/**
 * Replace a work's truth document, conditionally.
 *
 * `If-Match` is required by the server rather than optional here, as for
 * `putNotes`. The stakes are higher than notes: a labelling session is hours
 * of human work that exists nowhere else, and last-write-wins would discard
 * it silently.
 */
export async function putTruth(
  work: string,
  etag: string,
  doc: TruthDoc,
): Promise<FetchedTruth> {
  const res = await fetch(`/api/works/${seg(work)}/truth`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', 'if-match': etag },
    body: JSON.stringify(doc),
  })
  if (!res.ok) throw new Error(`writing truth: ${res.status} ${await res.text()}`)
  return {
    doc: (await res.json()) as TruthDoc,
    etag: res.headers.get('etag') ?? undefined,
  }
}
```

- [ ] **Step 2: Write the failing keybinding tests**

Create `web/src/Labeller.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { Labeller } from './Labeller'
import type { LabelBox } from './labelling'

const one: LabelBox[] = [
  { id: 'a', x: 0.1, y: 0.1, w: 0.2, h: 0.2, kind: 'dialogue', axis: 'v', ignore: false },
]

describe('Labeller keys', () => {
  it('tags a kind by number', async () => {
    render(<Labeller initialBoxes={one} initialSelected="a" />)
    await userEvent.keyboard('3')
    expect(screen.getByTestId('region-a')).toHaveAttribute('data-kind', 'sfx')
  })

  it('sets an axis, including not-applicable', async () => {
    render(<Labeller initialBoxes={one} initialSelected="a" />)
    await userEvent.keyboard('n')
    expect(screen.getByTestId('axis-readout')).toHaveTextContent('—')
    await userEvent.keyboard('h')
    expect(screen.getByTestId('axis-readout')).toHaveTextContent('h')
  })

  it('toggles ignore', async () => {
    render(<Labeller initialBoxes={one} initialSelected="a" />)
    await userEvent.keyboard('i')
    expect(screen.getByTestId('region-a')).toHaveAttribute('data-ignore', 'true')
  })

  it('deletes the selected box', async () => {
    render(<Labeller initialBoxes={one} initialSelected="a" />)
    await userEvent.keyboard('{Delete}')
    expect(screen.queryByTestId('region-a')).toBeNull()
  })

  it('does nothing on a kind key with no box selected', async () => {
    render(<Labeller initialBoxes={one} />)
    await userEvent.keyboard('3')
    expect(screen.getByTestId('region-a')).toHaveAttribute('data-kind', 'dialogue')
  })

  it('shows how many boxes are on this page, so progress is visible', () => {
    render(<Labeller initialBoxes={one} />)
    expect(screen.getByTestId('box-count')).toHaveTextContent('1')
  })
})
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd web && pnpm vitest run --project jsdom Labeller`
Expected: FAIL — `Failed to resolve import "./Labeller"`.

- [ ] **Step 4: Write `Labeller.tsx`**

Create `web/src/Labeller.tsx`:

```tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import { RegionOverlay } from './RegionOverlay'
import {
  addBox, cycleAt, removeBox, setAxis, setKind, toggleIgnore,
  type BoxAxis, type Kind, type LabelBox, type Point,
} from './labelling'

/** Index order is the `1`-`6` key order, and it matches `taiyaku-core`'s `Kind`. */
const KINDS: readonly Kind[] = ['dialogue', 'narration', 'sfx', 'logo', 'credits', 'other']

export interface LabellerProps {
  initialBoxes?: readonly LabelBox[]
  initialSelected?: string
}

/**
 * Draw text-run boxes on a page.
 *
 * A second entry rather than a mode inside `App.tsx`: the extraction map
 * already calls that file too large, and none of this state is a reader's.
 *
 * Coordinates are converted against the **page element's** own rect, never the
 * window. The page is letterboxed inside the viewport, so a window-relative
 * conversion is off by the letterbox margin — which looks right at one aspect
 * ratio and is wrong at every other.
 */
export function Labeller({ initialBoxes = [], initialSelected }: LabellerProps) {
  const [boxes, setBoxes] = useState<LabelBox[]>([...initialBoxes])
  const [selected, setSelected] = useState<string | undefined>(initialSelected)
  const pageRef = useRef<HTMLDivElement>(null)
  const dragStart = useRef<Point | undefined>(undefined)
  const cursor = useRef<Point>({ x: 0, y: 0 })

  const toFractions = useCallback((clientX: number, clientY: number): Point => {
    const rect = pageRef.current?.getBoundingClientRect()
    if (!rect || rect.width === 0 || rect.height === 0) return { x: 0, y: 0 }
    return { x: (clientX - rect.left) / rect.width, y: (clientY - rect.top) / rect.height }
  }, [])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Tab') {
        event.preventDefault()
        setSelected(cycleAt(boxes, cursor.current, selected))
        return
      }
      if (selected === undefined) return
      const kind = KINDS[Number(event.key) - 1]
      if (kind) return setBoxes((current) => setKind(current, selected, kind))
      const axis: BoxAxis | undefined =
        event.key === 'v' ? 'v' : event.key === 'h' ? 'h' : event.key === 'n' ? null : undefined
      if (axis !== undefined || event.key === 'n')
        return setBoxes((current) => setAxis(current, selected, axis ?? null))
      if (event.key === 'i') return setBoxes((current) => toggleIgnore(current, selected))
      if (event.key === 'Delete') {
        setBoxes((current) => removeBox(current, selected))
        setSelected(undefined)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [boxes, selected])

  const axisOf = boxes.find((box) => box.id === selected)?.axis
  return (
    <div className="labeller">
      <div
        ref={pageRef}
        data-testid="label-page"
        className="label-page"
        style={{ position: 'relative' }}
        onPointerDown={(e) => {
          dragStart.current = toFractions(e.clientX, e.clientY)
        }}
        onPointerMove={(e) => {
          cursor.current = toFractions(e.clientX, e.clientY)
        }}
        onPointerUp={(e) => {
          const from = dragStart.current
          dragStart.current = undefined
          if (!from) return
          const to = toFractions(e.clientX, e.clientY)
          setBoxes((current) =>
            addBox(current, { x: from.x, y: from.y, w: to.x - from.x, h: to.y - from.y }),
          )
        }}
      >
        <RegionOverlay boxes={boxes} selectedId={selected} onSelect={setSelected} />
      </div>
      <div className="labeller-readout">
        <span data-testid="box-count">{boxes.length}</span>
        <span data-testid="axis-readout">{axisOf === null ? '—' : (axisOf ?? '')}</span>
      </div>
    </div>
  )
}
```

Note the `axis` arm: `null` is a legitimate value and `undefined` means "not an axis key", so the two cannot be collapsed into one falsy check — that distinction is exactly what `n` exists to set.

`PageView` is wired into the `label-page` div once a work is selected; the tests above drive the component with no page loaded, which is why the div is present and empty rather than conditional.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd web && pnpm vitest run --project jsdom Labeller`
Expected: PASS, 6 tests.

- [ ] **Step 6: Write the browser test for drag geometry**

Create `web/src/Labeller.browser.test.tsx`. This is what the `browser` project exists for: `jsdom` has no layout engine, `getBoundingClientRect` returns zeroes there, and a box drawn at the wrong place is not observable without one — the argument `vite.config.ts`'s own comment already makes.

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Labeller } from './Labeller'

describe('drag to draw, with a layout engine', () => {
  it('lands the box where the pointer went, in page fractions', async () => {
    render(<Labeller initialBoxes={[]} />)
    const page = screen.getByTestId('label-page')
    const box = page.getBoundingClientRect()
    // A quarter of the way in, to half way across and down.
    await drag(page, box.left + box.width * 0.25, box.top + box.height * 0.25,
                     box.left + box.width * 0.5,  box.top + box.height * 0.5)
    const drawn = document.querySelector('.region-box') as HTMLElement
    expect(Number.parseFloat(drawn.style.left)).toBeCloseTo(25, 0)
    expect(Number.parseFloat(drawn.style.width)).toBeCloseTo(25, 0)
  })

  it('measures against the page element, not the window', async () => {
    // The page is letterboxed inside the viewport. Converting against the
    // window puts every box off by the letterbox margin, which looks correct
    // at one aspect ratio and wrong at every other.
    render(<Labeller initialBoxes={[]} />)
    const page = screen.getByTestId('label-page')
    const box = page.getBoundingClientRect()
    expect(box.left).toBeGreaterThan(0)
    await drag(page, box.left, box.top, box.left + box.width, box.top + box.height)
    const drawn = document.querySelector('.region-box') as HTMLElement
    expect(Number.parseFloat(drawn.style.left)).toBeCloseTo(0, 0)
    expect(Number.parseFloat(drawn.style.width)).toBeCloseTo(100, 0)
  })
})

async function drag(el: Element, x0: number, y0: number, x1: number, y1: number) {
  el.dispatchEvent(new PointerEvent('pointerdown', { clientX: x0, clientY: y0, bubbles: true }))
  el.dispatchEvent(new PointerEvent('pointermove', { clientX: x1, clientY: y1, bubbles: true }))
  el.dispatchEvent(new PointerEvent('pointerup', { clientX: x1, clientY: y1, bubbles: true }))
}
```

- [ ] **Step 7: Add the second Vite entry**

Create `web/label.html`, copying `web/index.html` and changing the title to `taiyaku labeller` and the script to `/src/label.tsx`.

Create `web/src/label.tsx`, mirroring `web/src/main.tsx` but mounting `<Labeller />`.

In `web/vite.config.ts`, add a `build` block — **there is none today, so the single entry is implicit from `index.html`, and adding a second requires naming both**:

```ts
  build: {
    rollupOptions: {
      // Two entries. Without this Vite takes index.html alone and the
      // labeller silently does not ship -- there is no error, just a missing
      // page.
      input: {
        main: fileURLToPath(new URL('index.html', import.meta.url)),
        label: fileURLToPath(new URL('label.html', import.meta.url)),
      },
    },
  },
```

- [ ] **Step 8: Run the full web gate**

Run: `cd web && pnpm vitest run && pnpm exec tsc -b && pnpm build`
Expected: all three PASS, and `dist/label.html` exists after the build.

Run: `ls web/dist/label.html`
Expected: the file is listed.

- [ ] **Step 9: Commit**

```bash
git add web/label.html web/src/label.tsx web/src/Labeller.tsx web/src/Labeller.test.tsx web/src/Labeller.browser.test.tsx web/src/api.ts web/vite.config.ts
git commit -m "web: the labeller, as a second entry beside the reader

App.tsx is untouched: the extraction map already calls it too large, and a
labelling mode is state no reader needs. Drag geometry is tested in the
browser project rather than jsdom, which has no layout engine and would
report a box at the wrong place as passing.

vite.config.ts gains rollupOptions.input because it had none -- a second
entry without it is not an error, just a page that never ships."
```

**At this point the human can begin labelling** — after Task 9 Step 1, which fixes the page list and depends on nothing else. Tasks 6, 7 and 8 do not block the drawing and the drawing does not block them.

---

### Task 6: The scorer's five outcomes, on synthetic fixtures

**Files:**
- Create: `crates/taiyaku-cli/src/score.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (add `mod score;`)

**Interfaces:**
- Consumes: `TruthRegion`, `Kind` (Task 1); `Region` from `taiyaku-core`.
- Produces: `Outcome` (`OneToOne`, `Merged`, `Split`, `Missed`), `Scored` (`{outcomes: Vec<Outcome>, spurious: usize, ignored: usize}`), `score_page(&[TruthRegion], &[Region], f64) -> Scored`, `COVERAGE: f64`.

**This is the load-bearing task.** A scorer that silently cannot tell a merge from a miss would reproduce exactly the failure this whole sub-project exists to end, while looking like it worked. It is proved on hand-built fixtures before it is pointed at one real page.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-cli/src/score.rs` with only this block plus `use super::*;`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use taiyaku_core::{Axis, Kind, Region, TruthRegion};

    fn truth(x: f64, y: f64, w: f64, h: f64, kind: Kind) -> TruthRegion {
        TruthRegion { x, y, w, h, axis: Some(Axis::Vertical), kind, ignore: false, text: None }
    }

    fn ignored(x: f64, y: f64, w: f64, h: f64) -> TruthRegion {
        TruthRegion { ignore: true, ..truth(x, y, w, h, Kind::Sfx) }
    }

    fn predicted(x: f64, y: f64, w: f64, h: f64) -> Region {
        Region { x, y, w, h, axis: Axis::Vertical }
    }

    #[test]
    fn one_prediction_over_one_truth_box_is_one_to_one() {
        let scored = score_page(
            &[truth(0.1, 0.1, 0.2, 0.2, Kind::Dialogue)],
            &[predicted(0.1, 0.1, 0.2, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::OneToOne]);
        assert_eq!(scored.spurious, 0);
    }

    #[test]
    fn one_prediction_swallowing_three_truth_boxes_is_a_merge_not_three_misses() {
        // The failure `notes.md` found by cropping: the detector returns whole
        // panels. Under IoU@0.5 this is three misses and says nothing about
        // why. It must come back as three merges.
        let scored = score_page(
            &[
                truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
                truth(0.3, 0.1, 0.1, 0.1, Kind::Dialogue),
                truth(0.5, 0.1, 0.1, 0.1, Kind::Dialogue),
            ],
            &[predicted(0.05, 0.05, 0.6, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Merged; 3]);
        assert_eq!(scored.spurious, 0, "the prediction is accounted for, not spurious");
    }

    #[test]
    fn several_predictions_over_one_truth_box_is_a_split() {
        // The opposite failure on the same page: seven "blocks" that are
        // vertical slices of one word.
        let scored = score_page(
            &[truth(0.1, 0.1, 0.3, 0.1, Kind::Dialogue)],
            &[
                predicted(0.10, 0.1, 0.09, 0.1),
                predicted(0.21, 0.1, 0.09, 0.1),
                predicted(0.31, 0.1, 0.09, 0.1),
            ],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Split]);
    }

    #[test]
    fn nothing_overlapping_a_truth_box_is_a_miss() {
        let scored = score_page(
            &[truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)],
            &[predicted(0.7, 0.7, 0.1, 0.1)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Missed]);
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn a_prediction_over_nothing_is_spurious() {
        let scored = score_page(&[], &[predicted(0.7, 0.7, 0.1, 0.1)], COVERAGE);
        assert!(scored.outcomes.is_empty());
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn a_prediction_landing_on_an_ignored_box_is_neither_hit_nor_false_positive() {
        let scored = score_page(
            &[ignored(0.1, 0.1, 0.2, 0.2)],
            &[predicted(0.1, 0.1, 0.2, 0.2)],
            COVERAGE,
        );
        assert!(scored.outcomes.is_empty(), "an ignored box is not scored");
        assert_eq!(scored.spurious, 0, "and its prediction is not punished");
        assert_eq!(scored.ignored, 1, "but it is counted out loud");
    }

    #[test]
    fn a_grazing_overlap_below_the_threshold_does_not_match() {
        let scored = score_page(
            &[truth(0.1, 0.1, 0.2, 0.2, Kind::Dialogue)],
            &[predicted(0.28, 0.28, 0.2, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Missed]);
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn an_empty_page_scores_nothing_and_does_not_panic() {
        let scored = score_page(&[], &[], COVERAGE);
        assert!(scored.outcomes.is_empty());
        assert_eq!(scored.spurious, 0);
        assert_eq!(scored.ignored, 0);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli score`
Expected: FAIL to compile — `cannot find function score_page`.

- [ ] **Step 3: Write the implementation**

Prepend to `crates/taiyaku-cli/src/score.rs`:

```rust
//! What a detector found, against what a person drew.
//!
//! **Five outcomes per truth box, not an IoU pass or fail**, and the reason is
//! recorded rather than assumed. The detector's two known failure modes both
//! score as a flat miss under IoU@0.5 and it cannot tell them apart:
//! `docs/notes.md` records *both* on one edition at one gap — panels
//! over-merged, and seven "blocks" that are vertical slices of one word. A
//! single overlap number reports "recall 0.0" for each and says nothing about
//! which, which is why finding out cost ten hand-cropped blocks.
//!
//! This is the region-evaluation scheme from document-layout analysis
//! (Shafait/Breuel, ICDAR). The payoff is that "this edition is finding
//! panels" becomes a merge count on the first line of output.
//!
//! Native-only, and not in `taiyaku-core`: nothing in the browser scores
//! anything, and putting it there would be speculative.

use taiyaku_core::{Kind, Region, TruthRegion};

/// What became of one truth box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// One prediction, one truth box, and neither reaches anything else.
    OneToOne,
    /// Its prediction also swallows other truth boxes.
    Merged,
    /// Two or more predictions cover it.
    Split,
    /// Nothing significantly overlaps it.
    Missed,
}

/// One page's result. `outcomes` is parallel to the *scored* truth boxes —
/// ignored ones are not in it, and are counted separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scored {
    pub outcomes: Vec<Outcome>,
    pub spurious: usize,
    pub ignored: usize,
}

/// How much of the smaller box must lie inside the larger for the two to count
/// as describing the same thing.
///
/// **A starting value, not a measurement.** It cannot be one yet: settling it
/// needs labels and there are none. It gets settled the way
/// `scripts/region-survey.sh`'s own header says constants get settled in this
/// repository — by drawing the proposed matches over the pages and looking at
/// them, not from aggregate output alone. Asserting this as measured before
/// that is done is a defect.
pub const COVERAGE: f64 = 0.5;

/// Area of the overlap between two page-fraction boxes.
fn intersection(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = (a.0 + a.2).min(b.0 + b.2);
    let y1 = (a.1 + a.3).min(b.1 + b.3);
    (x1 - x0).max(0.0) * (y1 - y0).max(0.0)
}

/// Whether two boxes describe the same thing.
///
/// Against the **smaller** of the two areas rather than the union, which is
/// what makes a merge distinguishable from a miss: a page-spanning prediction
/// covers a small balloon almost entirely while its union with that balloon is
/// nearly all prediction, so a union-based rule scores the pair as unrelated
/// and the merge disappears into the miss column.
fn significant(t: &TruthRegion, p: &Region, coverage: f64) -> bool {
    let t_box = (t.x, t.y, t.w, t.h);
    let p_box = (p.x, p.y, p.w, p.h);
    let smaller = (t.w * t.h).min(p.w * p.h);
    if smaller <= 0.0 {
        return false;
    }
    intersection(t_box, p_box) / smaller >= coverage
}

/// Score one page.
///
/// Ignored truth boxes are removed from scoring but still absorb predictions:
/// a prediction landing on a sound effect flagged `ignore` is neither a hit
/// nor a false positive, which is the whole point of the flag.
#[must_use]
pub fn score_page(truth: &[TruthRegion], predicted: &[Region], coverage: f64) -> Scored {
    let scored: Vec<&TruthRegion> = truth.iter().filter(|t| !t.ignore).collect();
    let ignored_boxes: Vec<&TruthRegion> = truth.iter().filter(|t| t.ignore).collect();

    // For each scored truth box, which predictions cover it.
    let covers: Vec<Vec<usize>> = scored
        .iter()
        .map(|t| {
            predicted
                .iter()
                .enumerate()
                .filter(|(_, p)| significant(t, p, coverage))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();

    // For each prediction, how many scored truth boxes it covers.
    let mut reach = vec![0usize; predicted.len()];
    for hits in &covers {
        for &i in hits {
            reach[i] += 1;
        }
    }

    let outcomes = covers
        .iter()
        .map(|hits| match hits.as_slice() {
            [] => Outcome::Missed,
            [only] if reach[*only] == 1 => Outcome::OneToOne,
            [_] => Outcome::Merged,
            _ => Outcome::Split,
        })
        .collect();

    // A prediction is spurious only if it reaches no truth box at all —
    // scored or ignored.
    let spurious = predicted
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            reach[*i] == 0 && !ignored_boxes.iter().any(|t| significant(t, p, coverage))
        })
        .count();

    Scored { outcomes, spurious, ignored: ignored_boxes.len() }
}
```

Add `mod score;` to `crates/taiyaku-cli/src/main.rs` beside `mod regions;` (`:50`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli score`
Expected: PASS, 8 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/score.rs crates/taiyaku-cli/src/main.rs
git commit -m "cli: five outcomes per truth box, proved on fixtures first

IoU@0.5 scores a merged panel and a split word as the same flat miss, which
is why 'this edition is finding panels' cost ten hand-cropped blocks to
learn. Overlap is taken against the smaller area rather than the union --
under a union rule a page-spanning prediction and a small balloon look
unrelated and the merge vanishes into the miss column.

The coverage constant says it is provisional, because it is."
```

---

### Task 7: The report — per-kind recall and the ignored count

**Files:**
- Modify: `crates/taiyaku-cli/src/score.rs`

**Interfaces:**
- Consumes: `Scored`, `Outcome`, `score_page` (Task 6).
- Produces: `Report` (`{one_to_one, merged, split, missed, spurious, ignored, pages, by_kind: BTreeMap<Kind, KindTally>}`), `KindTally` (`{found, total}`), `Report::add_page(&mut self, &[TruthRegion], &Scored)`, `Report::render(&self) -> String`, `Report::spurious_per_page(&self) -> f64`.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `score.rs`:

```rust
#[test]
fn a_report_tallies_outcomes_across_pages() {
    let mut report = Report::default();
    let page = [truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)];
    report.add_page(&page, &score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE));
    report.add_page(&page, &score_page(&page, &[], COVERAGE));
    assert_eq!(report.one_to_one, 1);
    assert_eq!(report.missed, 1);
    assert_eq!(report.pages, 2);
}

#[test]
fn recall_is_reported_per_kind_so_dialogue_and_sfx_are_separable() {
    // The whole reason kinds were not collapsed into one class: a later
    // decision to stop caring about sound effects must cost no relabelling.
    let page = [
        truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
        truth(0.5, 0.5, 0.1, 0.1, Kind::Sfx),
    ];
    let mut report = Report::default();
    report.add_page(&page, &score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE));
    assert_eq!(report.by_kind[&Kind::Dialogue], KindTally { found: 1, total: 1 });
    assert_eq!(report.by_kind[&Kind::Sfx], KindTally { found: 0, total: 1 });
}

#[test]
fn only_one_to_one_counts_as_found() {
    // A merge and a split are failures, not partial credit. Counting them as
    // found would let a detector returning whole panels score as working,
    // which is the exact mistake blocks_per_page made.
    let page = [
        truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
        truth(0.3, 0.1, 0.1, 0.1, Kind::Dialogue),
    ];
    let mut report = Report::default();
    report.add_page(&page, &score_page(&page, &[predicted(0.05, 0.05, 0.4, 0.2)], COVERAGE));
    assert_eq!(report.merged, 2);
    assert_eq!(report.by_kind[&Kind::Dialogue], KindTally { found: 0, total: 2 });
}

#[test]
fn the_ignored_count_is_carried_so_the_hatch_is_never_silent() {
    let page = [ignored(0.1, 0.1, 0.1, 0.1)];
    let mut report = Report::default();
    report.add_page(&page, &score_page(&page, &[], COVERAGE));
    assert_eq!(report.ignored, 1);
    assert!(report.render().contains("ignored"), "and it is printed");
}

#[test]
fn spurious_is_a_rate_because_a_candidate_may_predict_more_in_total() {
    let mut report = Report::default();
    let page = [truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)];
    report.add_page(&page, &score_page(&page, &[predicted(0.8, 0.8, 0.1, 0.1)], COVERAGE));
    report.add_page(&page, &score_page(&page, &[], COVERAGE));
    assert!((report.spurious_per_page() - 0.5).abs() < 1e-9);
}

#[test]
fn a_report_over_no_pages_renders_without_dividing_by_zero() {
    let report = Report::default();
    assert_eq!(report.spurious_per_page(), 0.0);
    assert!(!report.render().is_empty());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli score`
Expected: FAIL to compile — `cannot find type Report`.

- [ ] **Step 3: Write the implementation**

Append to `score.rs`, above `mod tests`:

```rust
use std::collections::BTreeMap;

/// Found against drawn, for one kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindTally {
    /// Boxes that came back `OneToOne`. A merge and a split are failures, not
    /// partial credit: counting them here would let a detector returning whole
    /// panels score as working, which is the mistake `blocks_per_page` made.
    pub found: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub one_to_one: usize,
    pub merged: usize,
    pub split: usize,
    pub missed: usize,
    pub spurious: usize,
    pub ignored: usize,
    pub pages: usize,
    pub by_kind: BTreeMap<Kind, KindTally>,
}

impl Report {
    /// Fold one page's result in.
    ///
    /// Takes the page's truth alongside its `Scored` because `outcomes` is
    /// parallel to the *scored* boxes, so recovering each outcome's kind means
    /// walking the same filter — done here once rather than at every call site.
    pub fn add_page(&mut self, truth: &[TruthRegion], scored: &Scored) {
        self.pages += 1;
        self.spurious += scored.spurious;
        self.ignored += scored.ignored;
        for (region, outcome) in truth.iter().filter(|t| !t.ignore).zip(&scored.outcomes) {
            let tally = self.by_kind.entry(region.kind).or_default();
            tally.total += 1;
            match outcome {
                Outcome::OneToOne => {
                    self.one_to_one += 1;
                    tally.found += 1;
                }
                Outcome::Merged => self.merged += 1,
                Outcome::Split => self.split += 1,
                Outcome::Missed => self.missed += 1,
            }
        }
    }

    /// A rate rather than a count, because a candidate may legitimately return
    /// more predictions in total — which is why the promotion rule states this
    /// term per page and the 1:1 term as a raw count over a fixed dev set.
    #[must_use]
    pub fn spurious_per_page(&self) -> f64 {
        if self.pages == 0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "both are counts of boxes and pages in one scoring run, far below \
                      f64's 2^53 exact-integer limit; the dev set is 45 pages."
        )]
        {
            self.spurious as f64 / self.pages as f64
        }
    }

    /// The whole result, as one block a person reads.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "pages={} 1:1={} merged={} split={} missed={} spurious={} ({:.2}/page) ignored={}\n",
            self.pages,
            self.one_to_one,
            self.merged,
            self.split,
            self.missed,
            self.spurious,
            self.spurious_per_page(),
            self.ignored,
        ));
        for (kind, tally) in &self.by_kind {
            let recall = if tally.total == 0 {
                0.0
            } else {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "counts of boxes in one scoring run; see spurious_per_page above."
                )]
                {
                    tally.found as f64 / tally.total as f64
                }
            };
            out.push_str(&format!(
                "  {kind:?} recall={recall:.2} ({}/{})\n",
                tally.found, tally.total
            ));
        }
        out
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli score`
Expected: PASS, 14 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/score.rs
git commit -m "cli: the report, with recall per kind and the ignored count printed

Only 1:1 counts as found. A merge and a split are failures, not partial
credit -- counting them would let a detector returning whole panels read as
working, which is what blocks_per_page did. Spurious is a rate because a
candidate may legitimately predict more in total."
```

---

### Task 8: `taiyaku score`

**Files:**
- Modify: `crates/taiyaku-cli/src/score.rs` (add `command`), `crates/taiyaku-cli/src/main.rs` (dispatch at `:105`, usage block at `:107+`)

**Interfaces:**
- Consumes: `Report`, `score_page`, `COVERAGE` (Tasks 6–7); `TruthDoc` (Task 1); `detect::detect_page` and `crate::edition_of_digests`, following `regions::command` (`regions.rs:134`).
- Produces: `score::command(&[String]) -> Result<(), String>`.

- [ ] **Step 1: Write the failing tests**

Add to `score.rs`'s `mod tests`:

```rust
fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

#[test]
fn score_with_no_arguments_prints_its_usage() {
    let err = command(&argv(&[])).unwrap_err();
    assert!(err.contains("usage"), "{err}");
}

#[test]
fn a_misspelled_flag_is_refused_rather_than_ignored() {
    // O21's rule: a flag typed with a typo must not be accepted in silence.
    let err = command(&argv(&["--sidecarr", "x", "page.jpg"])).unwrap_err();
    assert!(err.contains("--sidecarr"), "the message names the flag: {err}");
}

#[test]
fn score_needs_a_sidecar_because_truth_is_recorded_against_an_edition() {
    let err = command(&argv(&["page.jpg"])).unwrap_err();
    assert!(err.contains("--sidecar"), "{err}");
}

#[test]
fn a_coverage_outside_zero_to_one_is_refused() {
    let err = command(&argv(&["--sidecar", "t.json", "--coverage", "1.5", "p.jpg"])).unwrap_err();
    assert!(err.contains("coverage"), "{err}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-cli score`
Expected: FAIL to compile — `cannot find function command`.

- [ ] **Step 3: Write `command`**

Append to `score.rs`. Follow `regions::command` (`regions.rs:134`) step for step — it is the closest sibling and already carries the reasoning for each guard:

```rust
const USAGE: &str = "\
usage: taiyaku score --sidecar <taiyaku.json> [--coverage F] <member>...

Scores what `detect_page` finds against what a person drew in
<stem>.truth.json. Prints five outcomes per truth box, recall per kind, and
how many boxes were flagged `ignore`.
";
```

```rust
/// Score `detect_page` against a truth document.
///
/// Follows `regions::command` step for step — it is the closest sibling and
/// already carries the reasoning for each guard.
pub fn command(args: &[String]) -> Result<(), String> {
    // No `--cache`, for `regions::command`'s reason: the fingerprint cache
    // holds hashes and dimensions, not pixels, and a flag that cannot take
    // effect is refused rather than accepted (O19, O20, O22).
    const KNOWN: &[&str] = &["--sidecar", "--coverage"];
    crate::reject_unknown_flags("score", args, KNOWN)?;

    // Every valued flag resolves before a member is opened, so a typo does not
    // cost the seconds a decode pass takes.
    let sidecar = crate::option_value(args, "--sidecar", "a path to a taiyaku.json")?;
    let coverage = match crate::option_value(args, "--coverage", "a fraction from 0 to 1")? {
        None => COVERAGE,
        Some(text) => {
            let value: f64 = text
                .parse()
                .map_err(|_| format!("--coverage wants a number from 0 to 1, not `{text}`"))?;
            if !(0.0..=1.0).contains(&value) {
                return Err(format!("--coverage must be a fraction from 0 to 1, not {value}"));
            }
            value
        }
    };

    let members: Vec<std::path::PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| Some(a.as_str()) != sidecar)
        .filter(|a| Some(a.as_str()) != crate::option_value(args, "--coverage", "").ok().flatten())
        .map(std::path::PathBuf::from)
        .collect();
    if members.is_empty() {
        return Err(USAGE.into());
    }

    // Refused rather than ignored: truth is recorded against an edition, and
    // without the document there is nothing to identify against.
    let Some(sidecar) = sidecar else {
        return Err(
            "score needs --sidecar: labels are recorded against an edition, and the \
             sidecar is what says which edition these members are"
                .into(),
        );
    };

    // Read and parse before a single page is decoded.
    let text = std::fs::read_to_string(sidecar).map_err(|e| format!("{sidecar}: {e}"))?;
    let doc = taiyaku_core::Sidecar::from_json(&text)?;

    // A missing truth document is an error naming the path, not an empty
    // report. Scoring against no labels would print perfect precision and zero
    // recall, which reads like a broken detector rather than a missing file.
    let truth_path = taiyaku_core::TruthDoc::path_beside(std::path::Path::new(sidecar));
    let truth_text = std::fs::read_to_string(&truth_path)
        .map_err(|e| format!("{}: {e} — nothing has been labelled here", truth_path.display()))?;
    let truth = taiyaku_core::TruthDoc::from_json(&truth_text)?;

    // Content, not order: the sorted sha256 set identifies the edition, so the
    // order these were typed in does not matter for identification.
    let mut digests = Vec::new();
    for member in &members {
        digests.push(taiyaku_read::member_digest(member)?);
    }
    let edition = crate::edition_of_digests(&doc, sidecar, &digests)?;

    let Some(labelled) = truth.editions.get(&edition) else {
        return Err(format!("{} holds no labels for edition `{edition}`", truth_path.display()));
    };
    // Labels drawn on a replaced archive put a box on the wrong art on the
    // *right* page, which is harder to notice than a wrong page.
    if labelled.is_stale_for(&digests) {
        return Err(format!(
            "{} was drawn against different files; re-label or restore the archive",
            truth_path.display()
        ));
    }

    // `docs/sidecar.md` rule 1 is normative: the matched edition's own
    // `members` array *is* the page order and must not be re-sorted. Paths are
    // relative to the sidecar and may point into unrelated trees, so their
    // names need not sort into reading order at all. This is the block
    // `regions::command` carries at `regions.rs:201-218`; it is restated
    // rather than shared because that one is inline in a longer function.
    let target = doc
        .editions
        .get(&edition)
        .expect("edition_of_digests matched an edition of this document");
    let by_digest: std::collections::BTreeMap<&str, &std::path::PathBuf> = digests
        .iter()
        .zip(&members)
        .map(|(sha256, path)| (sha256.as_str(), path))
        .collect();
    let ordered_members: Vec<std::path::PathBuf> = target
        .members
        .iter()
        .map(|member| {
            (*by_digest
                .get(member.sha256.as_str())
                .expect("edition_of_digests matched this exact set of digests"))
            .clone()
        })
        .collect();

    // **Only the labelled pages are decoded.** `decode_all` decodes
    // everything it is handed, and five pages of 195 are labelled — handing it
    // the whole edition would pay for 190 full-resolution decodes to score
    // five. Moved rather than cloned, so `Page` needs no `Clone`.
    let pages = taiyaku_read::read_members_in_order(&ordered_members)?;
    let (numbers, subset): (Vec<u32>, Vec<_>) = pages
        .into_iter()
        .enumerate()
        .map(|(index, page)| {
            (u32::try_from(index).expect("a page index fits in u32"), page)
        })
        // Pages absent from the truth document are skipped: nobody labelled
        // them, and scoring against no labels is not zero recall, it is no
        // measurement.
        .filter(|(number, _)| labelled.pages.contains_key(number))
        .unzip();

    let detected =
        crate::fingerprint::decode_all(&subset, |_, luma| crate::detect::detect_page(luma))?;

    let mut report = Report::default();
    for (number, found) in numbers.iter().zip(detected) {
        let drawn = &labelled.pages[number];
        report.add_page(drawn, &score_page(drawn, &found, coverage));
    }

    print!("{}", report.render());
    Ok(())
}
```

The decode path is the one `regions::command` uses at `regions.rs:220-221` —
`taiyaku_read::read_members_in_order` then
`crate::fingerprint::decode_all(&pages, |_, luma| detect_page(luma))`, whose
signature is `decode_all<T, F>(pages: &[Page], describe: F) -> Result<Vec<T>, String>`
with `F: Fn(&Page, &GrayImage) -> T + Send + Sync` (`fingerprint.rs:208`). It
partitions by page size and runs the small ones under `rayon`, which is why the
closure must be `Send + Sync` and why `detect_page` is called through it rather
than in a plain loop.

- [ ] **Step 4: Wire the dispatch**

In `crates/taiyaku-cli/src/main.rs`, add after the `regions` arm at `:105`:

```rust
        Some("score") => score::command(&args[1..]),
```

And a usage line in the `None` block beside the others:

```rust
            println!("       taiyaku score <taiyaku.json> [--coverage F] <member>...");
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli score`
Expected: PASS, 18 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/score.rs crates/taiyaku-cli/src/main.rs
git commit -m "cli: taiyaku score

Follows regions::command: flags resolve before a member is opened, an
unknown flag is refused rather than ignored (O21), and a stale truth
document is refused rather than scored -- labels drawn on a replaced
archive put a box on the wrong art on the right page.

A missing truth document is an error, not an empty report: scoring against
nothing reads like a broken detector rather than a missing file."
```

---

### Task 9: The fixed sample, and the standing corpus check

**Files:**
- Create: `crates/taiyaku-cli/src/sample.rs`, `crates/taiyaku-cli/src/score_corpus.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (add `mod sample;` and `#[cfg(test)] mod score_corpus;`)

**Interfaces:**
- Consumes: everything above; `crate::corpus::corpus` for the gated root.
- Produces: `sample::DEV_EDITIONS`, `sample::HELD_OUT_EDITIONS`, `sample::dev_pages(usize) -> Vec<usize>`.

**This task carries spec §5**, which nothing else does: which pages are the dev set has to be one fixed, machine-readable list, or "every edition contributed" in the check below asserts nothing and the human does not know what to draw.

- [ ] **Step 1: Write the sample module and its tests**

Create `crates/taiyaku-cli/src/sample.rs`:

```rust
//! Which pages are the dev set, and which are sealed.
//!
//! **Named rather than globbed.** A glob that stops matching is the
//! silent-nothing failure `region_experiment.rs` refuses to have, and the
//! assertion that every edition contributed only means something if the list
//! is fixed. The same reasoning that module's header gives for its nine.

/// The nine editions `region_experiment.rs` already samples.
///
/// Labelling these means every figure already in `docs/notes.md` stays
/// readable against the new labels, with no second sampling rule in between.
pub const DEV_EDITIONS: &[(&str, &str)] = &[
    ("gotoubun-v01", "raw-mono"),
    ("gotoubun-v01", "en-mono"),
    ("gotoubun-v01", "en-fan"),
    ("kaguya-v01", "raw-mono"),
    ("kaguya-v01", "en-mono"),
    ("oshinoko-v01", "raw-mono"),
    ("oshinoko-v01", "en-mono"),
    ("kobayashi-v01", "raw-mono"),
    ("kobayashi-v01", "en-mono"),
];

/// Four regimes the survey has never touched, all already on disk.
///
/// **Sealed.** Labelled up front, opened once at the end, reported whatever it
/// says, and never used to choose between candidates. Paths are under
/// `corpus/incoming/` and are laid out differently from the settled tier —
/// confirm each against the tree before the first run rather than assuming.
pub const HELD_OUT_EDITIONS: &[(&str, &str)] = &[
    ("incoming/berserk-cleaned", "Berserk.T01"),
    ("incoming/onepiece", "op-raw-v01"),
    ("incoming/nausicaa", "raw-v01"),
    ("incoming/bocchi", "raw-v01-src-hamaji"),
    ("incoming/bocchi", "raw-v01-src-krc"),
    ("incoming/bocchi", "raw-v01-src-mangazip"),
];

/// The dev set's page indices: every other one of `region_experiment`'s ten.
///
/// A strict subset of the 90 pages `notes.md`'s figures are computed over.
/// **Thinned rather than recomputed**: calling that module's `sample_indices`
/// with a count of 5 lands on *different* pages, because the formula divides
/// by the count, and the subset property is the whole point.
///
/// The spread is preserved deliberately. The front of a volume is colour
/// inserts, a title page and blank interstitials — the least representative
/// pages there are — which is why `sample_indices` spreads at all, and why
/// taking the first five of ten would undo it.
#[must_use]
pub fn dev_pages(available: usize) -> Vec<usize> {
    if available == 0 {
        return Vec::new();
    }
    (0..10)
        .step_by(2)
        .map(|i| i * available / 10 + available / 20)
        .filter(|page| *page < available)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dev_set_is_nine_editions_and_five_pages_each() {
        assert_eq!(DEV_EDITIONS.len(), 9);
        assert_eq!(dev_pages(200).len(), 5, "45 pages over nine editions");
    }

    #[test]
    fn the_picks_are_a_strict_subset_of_the_ten_the_survey_uses() {
        let ten: Vec<usize> = (0..10).map(|i| i * 195 / 10 + 195 / 20).collect();
        for page in dev_pages(195) {
            assert!(ten.contains(&page), "page {page} is not one of the survey's ten");
        }
    }

    #[test]
    fn the_picks_are_spread_rather_than_taken_from_the_front() {
        let picked = dev_pages(200);
        assert!(picked[0] < 40, "starts near the front");
        assert!(*picked.last().unwrap() > 150, "and reaches the back");
    }

    #[test]
    fn an_edition_shorter_than_the_sample_yields_only_pages_it_has() {
        assert!(dev_pages(3).iter().all(|page| *page < 3));
        assert!(dev_pages(0).is_empty());
    }
}
```

- [ ] **Step 2: Run the sample tests**

Run: `cargo test -p taiyaku-cli sample`
Expected: PASS, 4 tests.

- [ ] **Step 3: Write the corpus check**

Create `crates/taiyaku-cli/src/score_corpus.rs`, following `region_experiment.rs`'s shape — `#[cfg(test)]`, gated on `TAIYAKU_CORPUS`, skipping cleanly when unset:

```rust
//! Scoring against the labelled corpus — a standing check, not an experiment.
//!
//! **Asserts only that it measured something.** Never a recall figure, never a
//! spurious rate: those move when the detector changes, and a measurement that
//! fails when the thing it measures improves is a check pretending to be a
//! measurement. `region_experiment.rs`'s header carries the full argument.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli score_corpus -- --nocapture
//! ```

use crate::sample::DEV_EDITIONS;
use crate::score::{Report, score_page, COVERAGE};

#[test]
fn scoring_the_dev_set_produces_a_report_over_every_labelled_page() {
    let Some(root) = crate::corpus::corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };

    let mut report = Report::default();
    let mut contributed: Vec<&str> = Vec::new();
    let mut labelled_pages = 0usize;
    let mut scored_pages = 0usize;

    for (work, edition) in DEV_EDITIONS {
        let sidecar = root.join(work).join("taiyaku.json");
        if !sidecar.exists() {
            eprintln!("{work}: no sidecar; not yet labelled");
            continue;
        }
        let truth_path = taiyaku_core::TruthDoc::path_beside(&sidecar);
        let Ok(text) = std::fs::read_to_string(&truth_path) else {
            eprintln!("{work}/{edition}: no truth document; not yet labelled");
            continue;
        };
        let doc = taiyaku_core::TruthDoc::from_json(&text).expect("truth document parses");
        let Some(labelled) = doc.editions.get(*edition) else { continue };

        for (page, drawn) in &labelled.pages {
            labelled_pages += 1;
            let Some(image) = crate::score_corpus_page(&root, work, edition, *page) else {
                continue;
            };
            let found = crate::detect::detect_page(&image);
            report.add_page(drawn, &score_page(drawn, &found, COVERAGE));
            scored_pages += 1;
        }
        contributed.push(work);
    }

    // Skipping cleanly before any labelling exists, rather than failing: this
    // check ships in Task 9 and the drawing happens afterwards.
    if contributed.is_empty() {
        eprintln!("no truth documents found; skipping until the dev set is labelled");
        return;
    }

    print!("{}", report.render());

    assert_eq!(
        scored_pages, labelled_pages,
        "every page somebody labelled must be scored; {} were not",
        labelled_pages - scored_pages
    );
    assert!(report.spurious_per_page().is_finite(), "every figure is finite");
    assert!(report.pages > 0, "the report covers at least one page");
}
```

`score_corpus_page` resolves a work/edition/page to a `GrayImage`. Write it
beside this test using the path `region_experiment.rs` already uses to reach a
corpus page, and do not add a second way to do the same thing. Note that
`fingerprint::decode_all` (`fingerprint.rs:208`) decodes every page it is
handed, so filter to the labelled indices *before* calling it, exactly as Task
8 does — otherwise this check pays for a full-edition decode per work.

**Note the deliberate asymmetry:** a missing truth document *skips*, but a
labelled page that fails to score *fails*. The first is the ordinary state
before drawing starts; the second means labels exist and are silently not being
measured, which is the failure this whole sub-project is about.

- [ ] **Step 4: Run it with the corpus unset to check it skips**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-cli score_corpus`
Expected: PASS, with `TAIYAKU_CORPUS unset; skipping` on stderr.

- [ ] **Step 5: Run the whole gate**

Run: `scripts/check-all.sh`
Expected: PASS. This mirrors all four CI jobs since #24 — including the `web` job, whose biome config, diagnostic cap and flags fail silently if wrong.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-cli/src/sample.rs crates/taiyaku-cli/src/score_corpus.rs crates/taiyaku-cli/src/main.rs
git commit -m "cli: scoring the dev set as a standing check

Asserts that it measured something and never a quality figure, which is
region_experiment.rs's rule: a check that fails when the detector improves
is a measurement pretending to be a gate. The edition list is fixed rather
than globbed, so 'every edition contributed' means something."
```

---

## After the plan

Labelling is the next thing, and it is the human's ~3 hours, not an agent's. Once the dev set is drawn, `taiyaku score` gives the incumbent detector's baseline — **the first honest number this project has had about the detector** — and sub-project B gets its own spec and plan, measured against it.

Do not write B's spec before that baseline exists. The whole argument for A is that a candidate chosen before the instrument reads is chosen by the instrument that has already been wrong three times.

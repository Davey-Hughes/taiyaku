# M5c — annotations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A note written on the raw is readable while reading the English —
stored beside the sidecar, travelling through the alignment, marked in the
margin, reviewable in one list including the notes that cannot be reached from
where the reader is standing, and exportable as a file.

**Architecture:** Notes are content, so they live server-side, in
`<stem>.notes.json` beside the sidecar — the placement `progress` already uses,
written through the `crate::atomic::write` that O2 established and M5d
extracted. Two routes shaped like the ones that exist: `GET
/api/works/{work}/notes` and `PUT` guarded by `If-Match` against an `ETag` over
exactly the bytes served. Travel is not new arithmetic: a note is anchored to a
slot of its own edition, and showing it while reading another edition is one
`Resolver.resolve` — the same call the swap makes, so there is no second
implementation to disagree with the first. The region does **not** travel, by
design: transforming it correctly is L1 geometry, so a travelled note draws a
margin tab and never a tinted box.

**Tech Stack:** Rust (`taiyaku-serve`, axum 0.8, sha2 0.11, serde),
TypeScript/React 19 (`web/`), Vitest. **No `taiyaku-core` and no
`taiyaku-wasm` change is needed** — see "What the prototype measured" for the
measurement that settles it.

## Global Constraints

- **No new third-party dependency**, in either half. `docs/status.md` states the
  whole dependency list as a standing claim. ULID generation is ~30 lines of
  Crockford base32 in `web/src/ulid.ts`, not a package.
- **Slots, never pages.** A note is anchored to a slot; a range check is against
  `edition.slots(reading).len()`, never `total_pages()` — O3's lesson, and a
  spread-heavy edition (One Piece v092) is where getting it wrong shows.
- **Regions are fractions of the slot, never pixels**, so a re-scan or a
  different served resolution does not move a marker.
- **`#[serde(deny_unknown_fields)]` on every wire and at-rest type.** M5d's
  reasoning applies with more force here: a mistyped key in a *correction* costs
  a note nobody wrote, and a mistyped key in a *note* silently discards prose a
  reader typed.
- **Every refusal names what was missing, in this server's own words.** A bare
  status or a Rust type name in a message is a defect (O5).
- **Every handler that touches disk goes through `spawn_blocking`** (O12).
- **`App.tsx` and every `.tsx` file are Vitest-unreachable** — `resolver.ts`'s
  `taiyaku-wasm` import only resolves after `./scripts/build-wasm.sh`. All new
  logic worth testing goes in a `.ts` module; a `.tsx` file holds layout only.
- Sites are named by symbol in `docs/`, never by line number.
- Commit per task. Keep the docs commit free of behaviour changes.
- No attribution trailers in commit messages.

---

## What the prototype measured

Written into the tree, run against the real fixtures, and reverted before this
plan was written. Two things a plan asserting rather than running would have had
to guess at:

**1. A note's slot round-trips exactly, in both directions and through the
hub — so travel needs no new arithmetic and no new binding call.** A temporary
test in `crates/taiyaku-core/src/route.rs` resolved every slot of one edition
into another and straight back, over `sidecar.rs`'s `REAL` (Go-Toubun v01) and
`route.rs`'s `STAR` (three editions, only the two hub edges stored):

| Pair | Slots | Landed | Agreed | Disagreed |
| ---- | ----: | -----: | -----: | --------: |
| `raw-mono` → `en-mono` → back | 195 | 193 | 193 | **0** |
| `en-mono` → `raw-mono` → back | 194 | 193 | 193 | **0** |
| `b` → `c` → back (composed through hub `a`) | 30 | 28 | 28 | **0** |
| `c` → `b` → back (composed) | 30 | 28 | 28 | **0** |
| `a` → `c` → back | 30 | 28 | 28 | **0** |
| `b` → `a` → back | 30 | 30 | 30 | **0** |

The slots that do not land are genuine gaps — `raw-mono` slot 1 and 6 have no
counterpart at all, and `STAR`'s `b` 0/1 reach a hub gap and stop — which is
the "unreachable from here" case the review list must show rather than swallow.

**What this licenses, concretely:** `notesHere` may resolve **once per edition
that holds notes**, from the edition being viewed outward, instead of once per
note from each note's own edition. Both directions agree on every slot that
lands, so the cheaper direction is not a different answer. That also keeps
`Binding`'s single-pair route memo (O14) useful — `from` stays fixed at the
viewed edition across the whole scan — where resolving per note would alternate
pairs and thrash it, which is exactly the case O14's entry names as its
reopening condition.

**2. Nothing about a correction moves a note.** A note is anchored to a slot of
its own edition, and `Sidecar::correct` changes *pairings*, never an edition's
slot sequence. So a correction can change which slot a travelled note appears
*on* while reading another edition — which is the correct behaviour and the
reason travel is computed at render rather than stored — and can never orphan
the note itself.

## Deviation from the spec, stated rather than smuggled

Three, each with the reason:

1. **The design's `ReaderModule` carries `surface`, `transform` and `status`;
   the shipped contract (`web/src/module.ts`) carries only `id`, `label`,
   `defaults` and `keys`.** M5b built the narrower one and five modules use it:
   a module exports its prefs plus pure functions, and `App.tsx`/`Chrome.tsx`
   mount the surface. `annotations` follows the shipped pattern rather than
   widening the contract for one module — widening it is a decision worth
   taking against several callers, not one, and `module.ts`'s own doc says a
   ninth row wanting a new extension point is a signal to revisit deliberately.
2. **`GET /notes` synthesises an empty document when no file exists, rather than
   answering 204 the way `GET /progress` does.** A `PUT` requires `If-Match`,
   and a client that has never seen a document has no validator to send. Two
   alternatives were rejected: `If-None-Match: *` for creation (a second
   precondition spelling, on the one route where getting preconditions wrong
   costs a lost note), and making `If-Match` optional on the first write (which
   is precisely the lost-update hole the header exists to close, left open for
   exactly the request a second device is most likely to be racing). The
   synthesised document's `ETag` is computed over the bytes actually sent, so
   the first `PUT` is an ordinary conditional write like every later one.
3. **A region is drawn by dragging on the page while the note editor is open,
   rather than in a separate region-drawing mode.** `n` creates a whole-slot
   note — `region` absent, which the design explicitly blesses as the
   degenerate case rather than a second kind of note — and a drag refines it.
   One mode fewer to explain, and the whole-slot note stays reachable with one
   key.

## File structure

**Rust — `crates/taiyaku-serve/src/`**

| File | Responsibility |
| ---- | -------------- |
| `notes.rs` *(new)* | The document: `Note`, `Region`, `Notes`, where the file sits, load, store, canonical serialisation, and validation against a work |
| `works.rs` | Gains `Work::sibling` (shared with `progress_path`) and `Work::notes_guard`, which serialises one work's read-modify-write |
| `routes.rs` | Gains `notes_etag`, `get_notes`, `put_notes`, and two route registrations |
| `lib.rs` | `mod notes;` and the `pub use` |

**TypeScript — `web/src/`**

| File | Responsibility |
| ---- | -------------- |
| `ulid.ts` *(new)* | Sortable, collision-resistant ids, pure over an injected clock and randomness |
| `notes.ts` *(new)* | The note model and every rule about it: which notes are here, which are unreachable, where a tab sits, what a drag becomes |
| `modules/annotations.ts` *(new)* | The module: prefs, three keys, and the note-count readout's text |
| `modules/readouts.ts` | `RowLines`/`readoutRow`/`ReadoutsPrefs` gain a fifth `notes` segment |
| `api.ts` | `getNotes`, `putNotes` |
| `Annotations.tsx` *(new)* | The overlay: margin tabs, and region tints while revealed |
| `NotePanel.tsx` *(new)* | The editor and the review list, including export |
| `App.tsx` | Wiring only: load, key dispatch, save, and mounting the two components |

---

### Task 1: the notes document beside the sidecar

**Files:**
- Create: `crates/taiyaku-serve/src/notes.rs`
- Modify: `crates/taiyaku-serve/src/works.rs` (add `Work::sibling`)
- Modify: `crates/taiyaku-serve/src/progress.rs` (`progress_path` uses it)
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod notes;`, `pub use`)
- Test: in `crates/taiyaku-serve/src/notes.rs`'s own `mod tests`

**Interfaces:**
- Consumes: `crate::atomic::write`, `crate::works::Work`, `crate::test_support::{work, cleanup}`.
- Produces: `notes::{Note, Region, Notes, notes_path, empty, to_json, load, store}`;
  `Work::sibling(&self, suffix: &str) -> PathBuf`.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/notes.rs`:

```rust
//! What a reader wrote down, kept beside the sidecar rather than inside it.
//!
//! Notes are content, so they are server-side for the same reason progress is:
//! they have to survive a different machine, and a browser profile is neither
//! portable nor exportable. They are a sibling file rather than a field in
//! `taiyaku.json` because that document describes correspondence and must diff
//! meaningfully in git — human prose would be noise in it, and re-solving
//! rewrites it, which is the hazard O13 already records for hand corrections.
//!
//! A note is anchored to a **slot**, not a page: a note on the left half of a
//! joined spread must not silently become a note on the whole spread. Its
//! `region`, when it has one, is in fractions of that slot, so a re-scan or the
//! same edition served at another resolution does not move the marker.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::works::Work;

/// A rectangle on one slot, in fractions of that slot's own width and height.
///
/// Fractions rather than pixels, and the whole marker arithmetic is scale-free
/// as a result — a tab or a tint tracks correctly at any fit scale with no
/// extra work.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One note.
///
/// `region` is optional and absent means the whole slot — a degenerate case
/// rather than a second kind of note, so nothing downstream needs two code
/// paths. `created`/`updated` are RFC 3339 strings minted by the client: this
/// server has no clock in the format, and a note written offline should carry
/// the time it was written rather than the time it reached a server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub edition: String,
    pub slot: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    pub body: String,
    pub created: String,
    pub updated: String,
}

/// Every note on one work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
    pub work: String,
    pub notes: Vec<Note>,
}

/// Where a work's notes file sits: beside its sidecar, same stem.
pub fn notes_path(work: &Work) -> PathBuf {
    work.sibling("notes.json")
}

/// A work with nothing written on it yet.
///
/// The route hands this back where no file exists, so a client that has never
/// seen a document still gets one — and, with it, the `ETag` its first write
/// has to be conditional on.
pub fn empty(id: &str) -> Notes {
    Notes {
        work: id.to_string(),
        notes: Vec::new(),
    }
}

/// The one serialisation of a notes document.
///
/// Shared by the file on disk, the body of a response, and the `ETag` over
/// that body, so a validator can never be computed over bytes a client was not
/// sent — the failure mode that would make a conditional write silently
/// unconditional.
pub fn to_json(notes: &Notes) -> Result<String, String> {
    serde_json::to_string_pretty(notes).map_err(|e| format!("serialising notes: {e}"))
}

/// What is stored, or `None` where nothing ever was.
pub fn load(work: &Work) -> Result<Option<Notes>, String> {
    let path = notes_path(work);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Write the notes, atomically.
///
/// `progress::store`'s reasoning applies unchanged and with one addition: a
/// notes file holds prose a person typed, so a crash mid-write truncating it
/// destroys work that cannot be recomputed from anything, where a truncated
/// position merely loses a page number.
pub fn store(work: &Work, notes: &Notes) -> Result<(), String> {
    let path = notes_path(work);
    let text = to_json(notes)?;
    crate::atomic::write(&path, text.as_bytes(), "json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, slot: u32, region: Option<Region>) -> Note {
        Note {
            id: id.into(),
            edition: "raw".into(),
            slot,
            region,
            body: "why is this page here".into(),
            created: "2026-08-05T10:00:00Z".into(),
            updated: "2026-08-05T10:00:00Z".into(),
        }
    }

    /// The sidecar must not gain a notes field. It is an alignment document,
    /// re-solving rewrites it, and prose in it would be noise in every diff.
    #[test]
    fn notes_are_written_beside_the_sidecar_not_into_it() {
        let work = crate::test_support::work("taiyaku-test-notes-beside");
        let sidecar_before = std::fs::read_to_string(&work.path).unwrap();

        store(
            &work,
            &Notes {
                work: "t".into(),
                notes: vec![note("01J8", 12, None)],
            },
        )
        .unwrap();

        let sidecar_after = std::fs::read_to_string(&work.path).unwrap();
        let beside = notes_path(&work);
        let exists = beside.exists();
        crate::test_support::cleanup("taiyaku-test-notes-beside");
        assert_eq!(sidecar_before, sidecar_after, "the sidecar must not change");
        assert!(exists, "notes belong in {}", beside.display());
    }

    /// A note with a region and one without must both survive the trip, and
    /// the one without must not acquire a `null` region on the way — absent
    /// means the whole slot, and `"region": null` is a third spelling nothing
    /// should have to handle.
    #[test]
    fn a_note_round_trips_with_and_without_a_region() {
        let work = crate::test_support::work("taiyaku-test-notes-round-trip");
        let stored = Notes {
            work: "t".into(),
            notes: vec![
                note(
                    "01J8A",
                    3,
                    Some(Region {
                        x: 0.12,
                        y: 0.34,
                        w: 0.28,
                        h: 0.09,
                    }),
                ),
                note("01J8B", 0, None),
            ],
        };

        store(&work, &stored).unwrap();
        let text = std::fs::read_to_string(notes_path(&work)).unwrap();
        let read_back = load(&work).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-round-trip");
        assert_eq!(read_back, Some(stored));
        assert!(
            !text.contains("null"),
            "a note with no region must omit the field, not write null: {text}"
        );
    }

    /// The bytes on disk are the bytes an `ETag` is computed over and the
    /// bytes a client is sent. Anything else lets a validator describe a
    /// document nobody has, which makes a conditional write unconditional
    /// without saying so.
    #[test]
    fn what_is_stored_is_exactly_what_to_json_produces() {
        let work = crate::test_support::work("taiyaku-test-notes-canonical");
        let notes = Notes {
            work: "t".into(),
            notes: vec![note("01J8", 1, None)],
        };

        store(&work, &notes).unwrap();
        let on_disk = std::fs::read_to_string(notes_path(&work)).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-canonical");
        assert_eq!(on_disk, to_json(&notes).unwrap());
    }

    /// A field this build does not know is refused rather than dropped. The
    /// same rule M5d's `Correction` applies on the wire, applied at rest: a
    /// file written by a later build, or hand-edited with a typo, must not
    /// come back with the reader's own prose silently missing.
    #[test]
    fn a_notes_file_with_an_unknown_field_is_refused_rather_than_silently_dropped() {
        let work = crate::test_support::work("taiyaku-test-notes-unknown-field");
        std::fs::write(
            notes_path(&work),
            r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":0,
               "body":"b","created":"c","updated":"u","colour":"red"}]}"#,
        )
        .unwrap();

        let read_back = load(&work);

        crate::test_support::cleanup("taiyaku-test-notes-unknown-field");
        let message = read_back.expect_err("an unknown field must not parse");
        assert!(
            message.contains("colour"),
            "the refusal has to name the field: {message}"
        );
    }

    /// Nothing recorded is `None`, not an empty document — the file's
    /// existence is a fact this layer reports honestly. Collapsing the two is
    /// the *route*'s decision, made there and explained there.
    #[test]
    fn a_work_with_no_notes_file_loads_as_nothing_recorded() {
        let work = crate::test_support::work("taiyaku-test-notes-absent");
        let loaded = load(&work);
        crate::test_support::cleanup("taiyaku-test-notes-absent");
        assert_eq!(loaded.unwrap(), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-serve notes::` — expect a compile error: `mod notes`
is not declared, and `Work::sibling` does not exist.

- [ ] **Step 3: Add `Work::sibling` and route `progress_path` through it**

In `crates/taiyaku-serve/src/works.rs`, inside `impl Work`, after `sidecar()`:

```rust
    /// A file beside this work's sidecar, sharing its stem.
    ///
    /// Three files now live to this rule — `taiyaku.progress.json`,
    /// `taiyaku.stump.json` and `taiyaku.notes.json` — and the stem
    /// arithmetic was written twice before the third arrived, which is where
    /// a comment asking whoever changes one to keep the others in step stops
    /// being good enough. A sidecar with no stem at all falls back to
    /// `taiyaku`, the convention this project documents for the file itself.
    pub fn sibling(&self, suffix: &str) -> PathBuf {
        let stem = self
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "taiyaku".to_string());
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}.{suffix}"))
    }
```

In `crates/taiyaku-serve/src/progress.rs`, replace the body of `progress_path`:

```rust
/// Where a work's progress file sits: beside its sidecar, same stem.
pub fn progress_path(work: &Work) -> PathBuf {
    work.sibling("progress.json")
}
```

and delete the now-unused `use std::path::PathBuf;`-adjacent imports only if
the compiler says so — `PathBuf` is still the return type.

- [ ] **Step 4: Declare the module**

In `crates/taiyaku-serve/src/lib.rs`, add `mod notes;` to the module list (after
`mod cover;`, alphabetical with the rest) and, with the other re-exports:

```rust
pub use notes::{Note, Notes, Region};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve notes::`
Expected: 5 passed. The existing `progress::` tests must stay green — they are
what proves `sibling` did not move the progress file.

Run: `cargo test -p taiyaku-serve progress::`
Expected: 3 passed.

- [ ] **Step 6: Gate and commit**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
git add crates/taiyaku-serve/src/notes.rs crates/taiyaku-serve/src/works.rs \
        crates/taiyaku-serve/src/progress.rs crates/taiyaku-serve/src/lib.rs
git commit -m "Notes live beside the sidecar, written atomically"
```

---

### Task 2: `GET /api/works/{work}/notes`

**Files:**
- Modify: `crates/taiyaku-serve/src/works.rs` (add `Work::notes_guard`)
- Modify: `crates/taiyaku-serve/src/routes.rs` (`notes_etag`, `get_notes`, route)
- Test: `crates/taiyaku-serve/src/routes.rs`'s `mod tests`

**Interfaces:**
- Consumes: `notes::{load, empty, to_json}`, `routes::{if_none_match_hits, not_modified, work}`.
- Produces: `routes::notes_etag(json: &str) -> String`; `Work::notes_guard(&self) -> MutexGuard<'_, ()>`;
  the route `GET /api/works/{work}/notes`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/taiyaku-serve/src/routes.rs`'s `mod tests`:

```rust
    /// A work nobody has annotated still answers with a document, not a 204.
    ///
    /// A `PUT` requires `If-Match`, so a client that has never seen a document
    /// would have no validator to send and no way to write its first note
    /// conditionally. Synthesising an empty one — with an `ETag` over exactly
    /// the bytes sent — makes the first write an ordinary conditional write
    /// like every later one.
    #[tokio::test]
    async fn a_work_with_no_notes_answers_an_empty_document_with_a_validator() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-get"));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let tag = response
            .headers()
            .get(header::ETAG)
            .expect("a document a client may write against carries a validator")
            .to_str()
            .unwrap()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-get");
        assert_eq!(tag, notes_etag(&body), "the tag must be over what was sent");
        let parsed: crate::notes::Notes = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.work, "t");
        assert!(parsed.notes.is_empty());
    }

    /// The same validator answers `304` on the next load, which is what keeps
    /// a per-page-load read of a document that rarely changes off the wire.
    #[tokio::test]
    async fn a_matching_if_none_match_answers_304_for_notes() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-304"));
        let app = build_router(state);

        let first = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = first
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let again = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .header(header::IF_NONE_MATCH, &tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-304");
        assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    }

    /// A work this server does not hold says so, in the same words every other
    /// route addressed by work uses.
    #[tokio::test]
    async fn notes_for_an_unknown_work_are_a_404_naming_it() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-no-work"));
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::get("/api/works/nope/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-no-work");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("nope"), "the refusal must name it: {body}");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve routes::tests::a_work_with_no_notes`
Expected: FAIL — `notes_etag` is not defined, and the route answers 404 for a
path nothing registered.

- [ ] **Step 3: Add the guard, the tag and the handler**

In `crates/taiyaku-serve/src/works.rs`, add the field to `Work` and the
accessor. The struct becomes:

```rust
#[derive(Debug)]
pub struct Work {
    pub id: String,
    pub path: PathBuf,
    sidecar: std::sync::RwLock<std::sync::Arc<Sidecar>>,
    /// Serialises one work's notes read-modify-write. See `notes_guard`.
    notes: std::sync::Mutex<()>,
}
```

with `Work::new` gaining `notes: std::sync::Mutex::new(())`, and:

```rust
    /// Held across a notes read-modify-write, so `If-Match` means something.
    ///
    /// The sidecar gets this from `update` above, which can hold its own
    /// `RwLock` across the change because the document is cached in memory.
    /// Notes are not cached: they are read once per page load and written when
    /// a reader types, so keeping a second document in memory for the life of
    /// the process would buy nothing and add a second thing that can go stale.
    /// What is still needed is the *exclusion* — two requests that both read
    /// the same document, both find their `If-Match` current, and both write
    /// would lose one reader's note with nothing anywhere saying so, which is
    /// the whole failure the header exists to prevent.
    ///
    /// A poisoned lock is taken rather than propagated, the same call
    /// `sidecar()` makes and for the same reason: the file behind it is still
    /// whatever the last successful write left, and refusing every later
    /// request because one handler panicked is the worse answer.
    pub fn notes_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.notes.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
```

In `crates/taiyaku-serve/src/routes.rs`, beside `sidecar_etag`:

```rust
/// A notes document's `ETag`: a SHA-256 over exactly the bytes being sent.
///
/// Takes the serialised text rather than the document, unlike `sidecar_etag`
/// above, because both callers already hold it — `notes::to_json` is the one
/// serialisation, and hashing a second one built here would be the drift this
/// is meant to rule out.
pub(crate) fn notes_etag(json: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

/// Why a notes request could not be answered.
///
/// `NoProgress`'s split, for the same reason: a work that does not exist, or a
/// note naming an edition or slot that does not, is the client's mistake; a
/// file that will not read or write is this server's. Two cases join them
/// here — a body this server will not accept, which is a 400, and a
/// precondition answer, which carries its own status because 428 ("you sent no
/// validator") and 412 ("yours is stale") are genuinely different answers and
/// a client retries differently for each.
#[derive(Debug)]
enum NoNotes {
    NotFound(String),
    Refused(String),
    Precondition(StatusCode, String),
    Failed(String),
}

impl IntoResponse for NoNotes {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => not_found(message),
            Self::Refused(message) => bad_request(message),
            Self::Precondition(status, message) => (status, message).into_response(),
            Self::Failed(message) => (StatusCode::INTERNAL_SERVER_ERROR, message).into_response(),
        }
    }
}

async fn get_notes(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> Response {
    // A file read, on the same disk a page comes from — `get_progress`'s
    // reasoning applies unchanged (O12).
    let loaded = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoNotes::NotFound)?;
        // Absent and empty are deliberately collapsed here rather than in
        // `notes::load`, which reports the file honestly. A reader with no
        // notes and a reader who deleted their last note are in the same
        // state, and a client that had to tell them apart would need a second
        // code path to write its first note — see this plan's own note on why
        // a synthesised document is what `If-Match` needs.
        let notes = crate::notes::load(found)
            .map_err(NoNotes::Failed)?
            .unwrap_or_else(|| crate::notes::empty(&found.id));
        crate::notes::to_json(&notes).map_err(NoNotes::Failed)
    })
    .await;

    match loaded {
        Ok(Ok(json)) => {
            let tag = notes_etag(&json);
            if if_none_match_hits(&headers, &tag) {
                return not_modified(&tag);
            }
            (
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::ETAG, tag.as_str()),
                ],
                json,
            )
                .into_response()
        }
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("notes read panicked: {e}"),
        )
            .into_response(),
    }
}
```

Register it in `build_router`, after the progress route:

```rust
        .route("/api/works/{work}/notes", get(get_notes))
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-serve notes`
Expected: the three new route tests pass alongside Task 1's five.

- [ ] **Step 5: Gate and commit**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/works.rs
git commit -m "Serve a work's notes, with a validator to write against"
```

---

### Task 3: `PUT /api/works/{work}/notes`, guarded by `If-Match`

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs`
- Test: same file's `mod tests`

**Interfaces:**
- Consumes: `Work::notes_guard`, `notes::{load, empty, to_json, store}`, `if_match_hits`, `notes_etag`.
- Produces: the route `PUT /api/works/{work}/notes`, answering 200 with the
  stored document and its new `ETag`, 428 with no `If-Match`, 412 on a stale one.

- [ ] **Step 1: Write the failing tests**

```rust
    /// The three answers `If-Match` has, on the one route where getting them
    /// wrong loses prose a person typed: no header is 428 (you have not
    /// entered a race), a stale one is 412 (you lost it), a current one
    /// writes.
    #[tokio::test]
    async fn writing_notes_needs_a_current_if_match() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-put"));
        let app = build_router(state);
        let body = r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":1,
            "body":"first","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}]}"#;

        let unconditional = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unconditional.status(), StatusCode::PRECONDITION_REQUIRED);

        let stale = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, "\"nonsense\"")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);

        let current = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = current
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let written = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, &tag)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(written.status(), StatusCode::OK);
        let fresh = written
            .headers()
            .get(header::ETAG)
            .expect("a write answers with the validator for what it just wrote")
            .to_str()
            .unwrap()
            .to_string();
        assert_ne!(fresh, tag, "the document changed, so the tag must have");

        let read_back = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(read_back.into_body(), 1 << 20)
            .await
            .unwrap();
        let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-put");
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(parsed.notes[0].body, "first");
    }

    /// The lost-update case itself, which is the reason any of this exists:
    /// two devices read the same document, both write, and the second must be
    /// told rather than quietly replacing the first reader's note.
    #[tokio::test]
    async fn a_second_writer_holding_the_same_tag_is_refused_rather_than_winning() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-race"));
        let app = build_router(state);

        let read = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let shared = read
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let note = |id: &str, body: &str| {
            format!(
                r#"{{"work":"t","notes":[{{"id":"{id}","edition":"raw","slot":0,"body":"{body}",
                   "created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}}]}}"#
            )
        };

        let first = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, &shared)
                    .body(Body::from(note("01J8A", "device one")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, &shared)
                    .body(Body::from(note("01J8B", "device two")))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = second.status();
        let bytes = axum::body::to_bytes(second.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8(bytes.to_vec()).unwrap();

        let survived = app
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(survived.into_body(), 1 << 20)
            .await
            .unwrap();
        let parsed: crate::notes::Notes = serde_json::from_slice(&bytes).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-race");
        assert_eq!(status, StatusCode::PRECONDITION_FAILED);
        assert!(
            message.contains("ETag is now"),
            "a loser needs the current validator to retry against: {message}"
        );
        assert_eq!(parsed.notes[0].body, "device one", "the first write stands");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve routes::tests::writing_notes_needs`
Expected: FAIL — 405, since only `get` is registered on that path.

- [ ] **Step 3: Implement the handler**

In `routes.rs`, after `get_notes`:

```rust
/// Replace a work's notes.
///
/// A whole-document `PUT` rather than a per-note `POST`/`DELETE`, because the
/// document is small, a reader edits it from one place at a time, and
/// `If-Match` over the whole thing is one rule to hold rather than one per
/// note. The cost is stated rather than hidden: two devices adding a note each,
/// at the same time, means the second one gets a `412` and has to re-read and
/// re-apply — it does not merge. That is the same bargain `patch_alignments`
/// makes, and the alternative (a merge this server performs) would need it to
/// decide what two conflicting edits to *one* note mean, which is a question
/// nobody has had yet.
///
/// `#[allow(clippy::result_large_err)]` for the same reason `patch_alignments`
/// carries it: the closures below resolve a `Response` as their error type and
/// clippy checks each closure as its own item.
#[allow(clippy::result_large_err)]
async fn put_notes(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let incoming: crate::notes::Notes = match serde_json::from_slice(&body) {
        Ok(notes) => notes,
        Err(why) => return bad_request(format!("these are not notes: {why}")),
    };

    // Parsing, hashing and writing all block, and the read-compare-write below
    // holds an exclusive lock across a file write — `get_page`'s reasoning
    // about the async worker applies unchanged.
    let written = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id).map_err(NoNotes::NotFound)?;
        crate::notes::validate(&incoming, found).map_err(|why| match why {
            crate::notes::Invalid::Missing(message) => NoNotes::NotFound(message),
            crate::notes::Invalid::Refused(message) => NoNotes::Refused(message),
        })?;

        // Held across read, compare and write: two requests that both read the
        // same document and both find their validator current would otherwise
        // both write, and one reader's note would vanish with nothing saying
        // so. See `Work::notes_guard`.
        let _guard = found.notes_guard();

        let current = crate::notes::load(found)
            .map_err(NoNotes::Failed)?
            .unwrap_or_else(|| crate::notes::empty(&found.id));
        let tag = notes_etag(&crate::notes::to_json(&current).map_err(NoNotes::Failed)?);
        match if_match_hits(&headers, &tag) {
            None => {
                return Err(NoNotes::Precondition(
                    StatusCode::PRECONDITION_REQUIRED,
                    "writing notes must carry `If-Match` with the notes' ETag, so two devices \
                     annotating one work cannot silently overwrite each other"
                        .to_string(),
                ));
            }
            Some(false) => {
                return Err(NoNotes::Precondition(
                    StatusCode::PRECONDITION_FAILED,
                    format!(
                        "these notes have changed since you read them; their ETag is now {tag}. \
                         Re-read the notes and write again."
                    ),
                ));
            }
            Some(true) => {}
        }

        crate::notes::store(found, &incoming).map_err(NoNotes::Failed)?;
        let json = crate::notes::to_json(&incoming).map_err(NoNotes::Failed)?;
        Ok(json)
    })
    .await;

    match written {
        Ok(Ok(json)) => {
            let tag = notes_etag(&json);
            (
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::ETAG, tag.as_str()),
                ],
                json,
            )
                .into_response()
        }
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("notes write panicked: {e}"),
        )
            .into_response(),
    }
}
```

**Why the precondition arms are not `Refused`:** 428 and 412 are both
preconditions and neither is a 400, so folding them into the malformed-body
variant would either send the wrong status or smuggle one into a message. The
`Precondition` variant added to `NoNotes` in Task 2 is what carries the status
and the wording together.

Register the handler by extending the route added in Task 2:

```rust
        .route("/api/works/{work}/notes", get(get_notes).put(put_notes))
```

`crate::notes::validate` is Task 4's; for this task, add a placeholder that
accepts everything **and a `// Task 4` comment**, or implement Task 4 first if
executing out of order. Executing in order, add to `notes.rs`:

```rust
/// Why a notes document was not accepted.
#[derive(Debug)]
pub enum Invalid {
    /// Names something the work does not have — a 404's shape.
    Missing(String),
    /// Malformed in itself — a 400's shape.
    Refused(String),
}

/// Task 4 fills this in. Accepting everything here is a hole, so Tasks 3 and 4
/// land on one branch and neither is shipped alone.
pub fn validate(notes: &Notes, work: &Work) -> Result<(), Invalid> {
    let _ = (notes, work);
    Ok(())
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-serve notes`
Expected: Task 1–3's tests all pass.

- [ ] **Step 5: Commit**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/notes.rs
git commit -m "A conditional write for a work's notes"
```

---

### Task 4: what a notes document has to be true about

**Files:**
- Modify: `crates/taiyaku-serve/src/notes.rs` (`validate`, replacing Task 3's stub)
- Test: same file's `mod tests`

**Interfaces:**
- Consumes: `Work::sidecar()`, `taiyaku_core::Edition::slots`.
- Produces: `notes::validate(&Notes, &Work) -> Result<(), Invalid>` — the real one.

- [ ] **Step 1: Write the failing tests**

Add to `notes.rs`'s `mod tests`:

```rust
    fn on(edition: &str, slot: u32, region: Option<Region>) -> Notes {
        Notes {
            work: "t".into(),
            notes: vec![Note {
                id: "01J8".into(),
                edition: edition.into(),
                slot,
                region,
                body: "b".into(),
                created: "2026-08-05T10:00:00Z".into(),
                updated: "2026-08-05T10:00:00Z".into(),
            }],
        }
    }

    /// Every refusal, and the one acceptance that proves the guard is not
    /// spelled "refuse everything".
    #[test]
    fn a_note_must_name_a_slot_of_an_edition_this_work_has() {
        let work = crate::test_support::work("taiyaku-test-notes-validate");
        // The fixture is one edition, `raw`, of four pages and no spreads —
        // four slots, so slot 3 is the last and slot 4 is one past the end.
        let accepted = validate(&on("raw", 3, None), &work);
        let unknown_edition = validate(&on("nope", 0, None), &work);
        let past_the_end = validate(&on("raw", 4, None), &work);
        crate::test_support::cleanup("taiyaku-test-notes-validate");

        accepted.expect("the last slot of a real edition is a real anchor");
        let message = match unknown_edition {
            Err(Invalid::Missing(m)) => m,
            other => panic!("an unknown edition is a 404's shape: {other:?}"),
        };
        assert!(message.contains("nope"), "name it: {message}");
        let message = match past_the_end {
            Err(Invalid::Missing(m)) => m,
            other => panic!("a slot past the end is a 404's shape: {other:?}"),
        };
        assert!(
            message.contains('4') && message.contains("slot"),
            "say how many slots there are: {message}"
        );
    }

    /// Counted in slots, not pages — a joined spread is one page and two
    /// slots, so checking against `total_pages()` would refuse a real anchor
    /// on the last slot of every spread-joining edition. O3 learned this on
    /// progress; a note is anchored the same way.
    #[test]
    fn the_range_check_counts_slots_not_pages() {
        let work = crate::test_support::work("taiyaku-test-notes-spread-slot");
        // Rewrite the fixture's sidecar so page 0 is a joined spread: 4 pages,
        // 5 slots, and slot 4 must be accepted.
        let json = std::fs::read_to_string(&work.path)
            .unwrap()
            .replace(r#""spreads": []"#, r#""spreads": [0]"#);
        let work = crate::works::Work::new(
            work.id.clone(),
            work.path.clone(),
            taiyaku_core::Sidecar::from_json(&json).unwrap(),
        );

        let last = validate(&on("raw", 4, None), &work);
        let past = validate(&on("raw", 5, None), &work);

        crate::test_support::cleanup("taiyaku-test-notes-spread-slot");
        last.expect("4 pages with page 0 joined is 5 slots, and slot 4 is real");
        assert!(past.is_err(), "slot 5 is past the end even so");
    }

    /// A region outside the slot would draw a marker off the page, and a
    /// zero-sized one would draw nothing while claiming to be somewhere.
    #[test]
    fn a_region_must_lie_within_the_slot_and_have_a_size() {
        let work = crate::test_support::work("taiyaku-test-notes-region");
        let ok = Region { x: 0.1, y: 0.1, w: 0.5, h: 0.5 };
        let full = Region { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        let over = Region { x: 0.8, y: 0.1, w: 0.5, h: 0.2 };
        let empty = Region { x: 0.1, y: 0.1, w: 0.0, h: 0.2 };
        let negative = Region { x: -0.1, y: 0.1, w: 0.2, h: 0.2 };

        let results = [
            validate(&on("raw", 0, Some(ok)), &work),
            validate(&on("raw", 0, Some(full)), &work),
            validate(&on("raw", 0, Some(over)), &work),
            validate(&on("raw", 0, Some(empty)), &work),
            validate(&on("raw", 0, Some(negative)), &work),
        ];
        crate::test_support::cleanup("taiyaku-test-notes-region");

        results[0].as_ref().expect("an ordinary region");
        results[1].as_ref().expect("the whole slot, stated explicitly");
        assert!(matches!(results[2], Err(Invalid::Refused(_))), "runs off the edge");
        assert!(matches!(results[3], Err(Invalid::Refused(_))), "no size");
        assert!(matches!(results[4], Err(Invalid::Refused(_))), "negative origin");
    }

    /// Two notes sharing an id makes every later edit ambiguous — which of the
    /// two did the reader mean? — so it is refused at the boundary rather than
    /// discovered by a client that edits the wrong one.
    #[test]
    fn two_notes_cannot_share_an_id() {
        let work = crate::test_support::work("taiyaku-test-notes-dupe-id");
        let mut notes = on("raw", 0, None);
        notes.notes.push(notes.notes[0].clone());

        let refused = validate(&notes, &work);
        crate::test_support::cleanup("taiyaku-test-notes-dupe-id");

        let message = match refused {
            Err(Invalid::Refused(m)) => m,
            other => panic!("a duplicate id is malformed, not missing: {other:?}"),
        };
        assert!(message.contains("01J8"), "name the id: {message}");
    }

    /// The document says which work it belongs to, and a mismatch means a
    /// client is about to write one work's notes over another's — the one
    /// mistake here that destroys notes rather than refusing them.
    #[test]
    fn a_document_naming_a_different_work_is_refused() {
        let work = crate::test_support::work("taiyaku-test-notes-wrong-work");
        let mut notes = on("raw", 0, None);
        notes.work = "some-other-work".into();

        let refused = validate(&notes, &work);
        crate::test_support::cleanup("taiyaku-test-notes-wrong-work");

        let message = match refused {
            Err(Invalid::Refused(m)) => m,
            other => panic!("{other:?}"),
        };
        assert!(
            message.contains("some-other-work") && message.contains('t'),
            "name both, or nobody can tell which is wrong: {message}"
        );
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve notes::tests::a_note_must_name`
Expected: FAIL — the stub accepts everything, so every refusal assertion fails.

- [ ] **Step 3: Implement `validate`**

Replace the stub in `notes.rs`:

```rust
/// How far outside `[0, 1]` a region may stray before it is refused.
///
/// A region comes from a drag in a browser, divided by an element's measured
/// size, so an edge-to-edge selection lands a hair either side of exactly 1.0
/// as often as not. Refusing that would be refusing the correct answer for a
/// floating-point reason, and there is nothing a reader could do about it.
const REGION_SLACK: f64 = 1e-6;

/// Why a notes document was not accepted.
#[derive(Debug)]
pub enum Invalid {
    /// Names something the work does not have — a 404's shape.
    Missing(String),
    /// Malformed in itself — a 400's shape.
    Refused(String),
}

/// Everything that has to be true of a notes document before it replaces one.
///
/// Refusing here is what keeps a note from being written against an edition
/// that does not exist or a slot past the end of one — anchors nothing could
/// ever draw, discovered by a reader whose note simply never appears.
pub fn validate(notes: &Notes, work: &Work) -> Result<(), Invalid> {
    if notes.work != work.id {
        return Err(Invalid::Refused(format!(
            "these notes name the work `{}`, but this is `{}`",
            notes.work, work.id
        )));
    }

    let sidecar = work.sidecar();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for note in &notes.notes {
        if !seen.insert(note.id.as_str()) {
            return Err(Invalid::Refused(format!(
                "two notes share the id `{}`; every later edit of it would be ambiguous",
                note.id
            )));
        }

        let edition = sidecar.editions.get(&note.edition).ok_or_else(|| {
            Invalid::Missing(format!(
                "note `{}` names the edition `{}`, which work `{}` does not have",
                note.id, note.edition, work.id
            ))
        })?;

        // Slots, not pages: a joined spread is one page and two slots, and a
        // note on one half must not be refused because the page count is
        // smaller. O3 established this on progress for the same reason.
        let slots = edition.slots(sidecar.work.reading).len() as u32;
        if note.slot >= slots {
            return Err(Invalid::Missing(format!(
                "note `{}` names slot {} of `{}`, which has {slots} slots",
                note.id, note.slot, note.edition
            )));
        }

        if let Some(region) = note.region {
            let within = region.x >= -REGION_SLACK
                && region.y >= -REGION_SLACK
                && region.w > 0.0
                && region.h > 0.0
                && region.x + region.w <= 1.0 + REGION_SLACK
                && region.y + region.h <= 1.0 + REGION_SLACK;
            if !within {
                return Err(Invalid::Refused(format!(
                    "note `{}` has a region ({}, {}, {}×{}) that is not inside its slot; \
                     a region is fractions of the slot, so it lies within 0..1 and has a size",
                    note.id, region.x, region.y, region.w, region.h
                )));
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-serve notes`
Expected: all of Tasks 1–4's tests pass.

- [ ] **Step 5: Add the route-level test that a refusal reaches a client**

A unit-tested `validate` that no route calls would be correct and unreachable —
the shape O4 records. Add to `routes.rs`'s `mod tests`:

```rust
    /// The validation is not merely unit-tested: a bad anchor dispatched
    /// through the router comes back as a refusal a client can read.
    #[tokio::test]
    async fn a_note_on_a_slot_that_does_not_exist_is_refused_through_the_router() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-notes-bad-slot"));
        let app = build_router(state);

        let read = app
            .clone()
            .oneshot(
                Request::get("/api/works/t/notes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tag = read
            .headers()
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let response = app
            .oneshot(
                Request::put("/api/works/t/notes")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::IF_MATCH, &tag)
                    .body(Body::from(
                        r#"{"work":"t","notes":[{"id":"01J8","edition":"raw","slot":99,
                           "body":"b","created":"c","updated":"u"}]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .unwrap();
        let message = String::from_utf8(bytes.to_vec()).unwrap();

        crate::test_support::cleanup("taiyaku-test-notes-bad-slot");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            message.contains("99") && message.contains("slots"),
            "the refusal must say what was wrong: {message}"
        );
    }
```

Run: `cargo test -p taiyaku-serve notes` — expect it green.

- [ ] **Step 6: Commit**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
git add crates/taiyaku-serve/src/notes.rs crates/taiyaku-serve/src/routes.rs
git commit -m "Refuse a note anchored where nothing could draw it"
```

---

### Task 5: `ulid.ts` — ids that sort and do not collide

**Files:**
- Create: `web/src/ulid.ts`
- Test: `web/src/ulid.test.ts`

**Interfaces:**
- Produces: `ulid(now: number, random: () => number): string`, `cryptoRandom: () => number`.

- [ ] **Step 1: Write the failing test**

Create `web/src/ulid.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { ulid } from './ulid'

const CROCKFORD = /^[0123456789ABCDEFGHJKMNPQRSTVWXYZ]{26}$/

describe('ulid', () => {
  it('is 26 Crockford base32 characters', () => {
    expect(ulid(Date.parse('2026-08-05T10:00:00Z'), () => 0.5)).toMatch(CROCKFORD)
  })

  it('sorts by the time it was minted, as plain strings', () => {
    const early = ulid(Date.parse('2026-08-05T10:00:00Z'), () => 0.5)
    const later = ulid(Date.parse('2026-08-05T10:00:01Z'), () => 0.5)
    expect([later, early].sort()).toEqual([early, later])
  })

  it('differs within one millisecond, which is where two notes actually collide', () => {
    const at = Date.parse('2026-08-05T10:00:00Z')
    let n = 0
    const counter = () => ((n += 0.1), n % 1)
    expect(ulid(at, counter)).not.toBe(ulid(at, counter))
  })

  // Two devices minting an id in the same millisecond is the case the random
  // half exists for: the timestamp is identical, so only randomness separates
  // them, and a build that dropped it would collide silently.
  it('varies in its random half when the timestamp is identical', () => {
    const at = Date.parse('2026-08-05T10:00:00Z')
    const a = ulid(at, () => 0.25)
    const b = ulid(at, () => 0.75)
    expect(a.slice(0, 10)).toBe(b.slice(0, 10))
    expect(a.slice(10)).not.toBe(b.slice(10))
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd web && npx vitest run src/ulid.test.ts`
Expected: FAIL — cannot resolve `./ulid`.

- [ ] **Step 3: Implement**

Create `web/src/ulid.ts`:

```ts
/**
 * A sortable, collision-resistant id, minted client-side.
 *
 * Notes are created on whichever device the reader is holding and written as a
 * whole document, so two devices annotating one work mint ids without ever
 * seeing each other's. A server-assigned id would need a round trip before the
 * reader could type, and a counter would collide the first time two devices
 * were both offline.
 *
 * ULID's layout, in 26 Crockford base32 characters: 48 bits of milliseconds
 * first, so a plain string sort is a chronological sort and the review list
 * needs no comparator, then 80 bits of randomness, which is what separates two
 * ids minted in the same millisecond.
 *
 * Pure over an injected clock and randomness — `Date.now` and `crypto` are
 * both things a test cannot pin, and this is the one piece of the notes model
 * where "did it actually vary" is the property worth testing. `App.tsx` calls
 * `ulid(Date.now(), cryptoRandom)`.
 *
 * Written here rather than taken as a dependency: the whole implementation is
 * the thirty lines below, and `docs/status.md` states this project's
 * dependency list as a standing claim.
 */
const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'

/** Crockford base32, `length` characters, most significant first. */
function encode(value: number, length: number): string {
  let out = ''
  let left = value
  for (let i = 0; i < length; i++) {
    out = ALPHABET[left % 32] + out
    left = Math.floor(left / 32)
  }
  return out
}

export function ulid(now: number, random: () => number): string {
  const time = encode(now, 10)
  let entropy = ''
  // Sixteen characters of five bits each, drawn one at a time rather than from
  // one 80-bit number: JavaScript has no 80-bit integer, and building one out
  // of two doubles would lose bits in exactly the half that exists to be
  // unpredictable.
  for (let i = 0; i < 16; i++) {
    entropy += ALPHABET[Math.floor(random() * 32) % 32]
  }
  return time + entropy
}

/**
 * `crypto.getRandomValues`, as a `() => number` in `[0, 1)`.
 *
 * `Math.random` is not seeded per device and is explicitly not required to be
 * unpredictable; two devices starting from the same page load are exactly the
 * case the random half exists for.
 */
export const cryptoRandom = (): number => crypto.getRandomValues(new Uint32Array(1))[0] / 2 ** 32
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd web && npx vitest run src/ulid.test.ts`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
cd web && npm test && npm run typecheck && npm run lint && cd ..
git add web/src/ulid.ts web/src/ulid.test.ts
git commit -m "Sortable note ids, minted where the note is written"
```

---

### Task 6: `notes.ts` — the model, and travel through the alignment

**Files:**
- Create: `web/src/notes.ts`
- Test: `web/src/notes.test.ts`

**Interfaces:**
- Consumes: `Resolver`, `Position` from `./resolver` (types only), `fakeResolver`/`whole`/`bSlots` from `./fixtures`.
- Produces:
  - types `Region`, `Note`, `NotesDoc`, `NoteHere`, `ReviewEntry`
  - `notesHere(notes: Note[], viewing: string, slot: number, resolver: Resolver): NoteHere[]`
  - `reviewList(notes: Note[], viewing: string, resolver: Resolver): ReviewEntry[]`
  - `tabOffset(note: Note): number`
  - `regionFromDrag(a: {x: number; y: number}, b: {x: number; y: number}): Region`
  - `withNote(notes: Note[], note: Note): Note[]`
  - `withoutNote(notes: Note[], id: string): Note[]`

- [ ] **Step 1: Write the failing test**

Create `web/src/notes.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { bSlots, fakeResolver, whole } from './fixtures'
import {
  notesHere,
  regionFromDrag,
  reviewList,
  tabOffset,
  withNote,
  withoutNote,
  type Note,
} from './notes'

const note = (id: string, edition: string, slot: number, region?: Note['region']): Note => ({
  id,
  edition,
  slot,
  region,
  body: `note ${id}`,
  created: '2026-08-05T10:00:00Z',
  updated: '2026-08-05T10:00:00Z',
})

/** `a` has 8 whole slots, `b` has 9 (page 2 is a joined spread). */
const slots = { a: whole(8), b: bSlots }
/** `a` slot 3 pairs with `b` slot 4, and `a` slot 5 pairs with nothing. */
const pairs = {
  'a|b|0': 0, 'a|b|1': 1, 'a|b|2': 2, 'a|b|3': 4, 'a|b|4': 5, 'a|b|6': 7, 'a|b|7': 8,
  'b|a|0': 0, 'b|a|1': 1, 'b|a|2': 2, 'b|a|4': 3, 'b|a|5': 4, 'b|a|7': 6, 'b|a|8': 7,
}
const resolver = fakeResolver(slots, pairs)

describe('notesHere', () => {
  it('shows a note written on the edition being read, at its own slot', () => {
    const notes = [note('1', 'a', 3), note('2', 'a', 4)]
    expect(notesHere(notes, 'a', 3, resolver)).toEqual([{ note: notes[0], travelled: false }])
  })

  it('travels a note written on another edition onto the slot it resolves to', () => {
    const notes = [note('1', 'a', 3)]
    // `a` 3 pairs with `b` 4, so reading `b` at slot 4 shows it, travelled.
    expect(notesHere(notes, 'b', 4, resolver)).toEqual([{ note: notes[0], travelled: true }])
    expect(notesHere(notes, 'b', 3, resolver)).toEqual([])
  })

  it('does not travel a note whose slot has no counterpart here', () => {
    // `a` 5 pairs with nothing in `b`.
    expect(notesHere([note('1', 'a', 5)], 'b', 5, resolver)).toEqual([])
  })

  it('shows a local and a travelled note together, local first', () => {
    const local = note('local', 'b', 4)
    const far = note('far', 'a', 3)
    const here = notesHere([far, local], 'b', 4, resolver)
    expect(here.map((h) => h.note.id)).toEqual(['local', 'far'])
  })

  // The round trip the design names as a seam test, exhaustive rather than
  // sampled: a note written on any slot of `a` that has a counterpart in `b`
  // must be visible at exactly one slot of `b`, and that slot must resolve
  // back to where the note was written.
  it('round-trips every note-bearing slot of one edition into the other and back', () => {
    for (let slot = 0; slot < slots.a.length; slot++) {
      const written = note(`n${slot}`, 'a', slot)
      const landing = Array.from({ length: slots.b.length }, (_, at) => at).filter(
        (at) => notesHere([written], 'b', at, resolver).length === 1,
      )
      const counterpart = pairs[`a|b|${slot}` as keyof typeof pairs]
      if (counterpart === undefined) {
        expect(landing, `a ${slot} has no counterpart, so it must land nowhere`).toEqual([])
        continue
      }
      expect(landing, `a ${slot} must land on exactly one slot of b`).toEqual([counterpart])
      expect(notesHere([written], 'a', slot, resolver)).toHaveLength(1)
    }
  })
})

describe('reviewList', () => {
  it('gives every note a landing slot here, or says it has none', () => {
    const notes = [note('here', 'a', 3), note('gone', 'a', 5), note('local', 'b', 0)]
    expect(reviewList(notes, 'b', resolver)).toEqual([
      { note: notes[0], here: 4 },
      { note: notes[1], here: undefined },
      { note: notes[2], here: 0 },
    ])
  })

  // Silence is the failure this project is written against: a note that cannot
  // be reached from here must still be in the list, marked, not omitted.
  it('keeps an unreachable note in the list rather than dropping it', () => {
    expect(reviewList([note('gone', 'a', 5)], 'b', resolver)).toHaveLength(1)
  })
})

describe('tabOffset', () => {
  it('puts a whole-slot note at the middle of the margin', () => {
    expect(tabOffset(note('1', 'a', 0))).toBe(0.5)
  })

  it('puts a region note at the middle of its own region', () => {
    expect(tabOffset(note('1', 'a', 0, { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }))).toBeCloseTo(0.4)
  })
})

describe('regionFromDrag', () => {
  it('normalises a drag in any direction into a positive rectangle', () => {
    const forward = regionFromDrag({ x: 0.2, y: 0.3 }, { x: 0.5, y: 0.7 })
    const backward = regionFromDrag({ x: 0.5, y: 0.7 }, { x: 0.2, y: 0.3 })
    expect(forward).toEqual({ x: 0.2, y: 0.3, w: 0.3, h: 0.4 })
    expect(backward).toEqual(forward)
  })

  it('clamps to the slot, so a drag off the edge stays a legal region', () => {
    expect(regionFromDrag({ x: -0.5, y: 0.5 }, { x: 1.5, y: 1.5 })).toEqual({
      x: 0, y: 0.5, w: 1, h: 0.5,
    })
  })
})

describe('withNote / withoutNote', () => {
  it('replaces a note with the same id rather than adding a second', () => {
    const first = note('1', 'a', 0)
    const edited = { ...first, body: 'changed' }
    expect(withNote([first], edited)).toEqual([edited])
  })

  it('appends a note whose id is new', () => {
    expect(withNote([note('1', 'a', 0)], note('2', 'a', 1))).toHaveLength(2)
  })

  it('removes by id and leaves the rest alone', () => {
    const notes = [note('1', 'a', 0), note('2', 'a', 1)]
    expect(withoutNote(notes, '1')).toEqual([notes[1]])
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd web && npx vitest run src/notes.test.ts`
Expected: FAIL — cannot resolve `./notes`.

- [ ] **Step 3: Implement**

Create `web/src/notes.ts`:

```ts
import type { Resolver } from './resolver'

/** A rectangle on one slot, in fractions of that slot — never pixels. */
export interface Region {
  x: number
  y: number
  w: number
  h: number
}

/**
 * One note, exactly as `taiyaku-serve`'s `notes.rs` stores it.
 *
 * `region` absent means the whole slot — the degenerate case, not a second
 * kind of note. `created`/`updated` are RFC 3339 strings minted here: the
 * server has no clock in this format, so a note carries the time it was
 * written rather than the time it arrived.
 */
export interface Note {
  id: string
  edition: string
  slot: number
  region?: Region
  body: string
  created: string
  updated: string
}

export interface NotesDoc {
  work: string
  notes: Note[]
}

/** A note visible where the reader is standing, and whether it came from elsewhere. */
export interface NoteHere {
  note: Note
  /**
   * Written on another edition and resolved onto this slot. Its region does
   * **not** travel — transforming one correctly is L1 geometry — so a
   * travelled note draws a margin tab and never a tint. Drawing the box at an
   * untransformed position would put it over the wrong panel, which is the
   * reader lying about what it knows.
   */
  travelled: boolean
}

/** One row of the review list: a note, and where it lands here if it does. */
export interface ReviewEntry {
  note: Note
  /** The slot of the edition being read, or `undefined` — unreachable from here. */
  here: number | undefined
}

/**
 * Where each edition's slot space meets the one being read: for every edition
 * that holds notes, the slot of *that* edition which the current slot of the
 * viewed edition corresponds to.
 *
 * **Resolved outward from the viewed edition, once per edition — not once per
 * note.** The two directions agree: measured over the real sidecar and a
 * composed three-edition route, every slot that lands round-trips to exactly
 * where it started, 193 of 193 and 28 of 28, with no disagreement in either
 * direction (see the M5c plan's "What the prototype measured"). Resolving this
 * way holds `from` fixed for the whole scan, which is what `Binding`'s
 * single-pair route memo is built for — resolving per note would alternate
 * pairs and thrash it, the case O14's entry names as its reopening condition.
 */
function counterpartSlots(
  editions: string[],
  viewing: string,
  slot: number,
  resolver: Resolver,
): Map<string, number> {
  const out = new Map<string, number>()
  const at = resolver.slotAt(viewing, slot)
  if (!at) return out
  for (const edition of editions) {
    if (edition === viewing) {
      out.set(edition, slot)
      continue
    }
    const landed = resolver.resolve(viewing, edition, at)
    const index = landed && resolver.indexOf(edition, landed)
    if (index !== undefined) out.set(edition, index)
  }
  return out
}

/**
 * The notes to draw on the slot being read: those written here, and those
 * written on another edition that the alignment puts here.
 *
 * Local notes come first, so the tab order does not shuffle as a reader swaps
 * editions — the same slot in the same edition always draws its own notes in
 * the same place.
 */
export function notesHere(
  notes: Note[],
  viewing: string,
  slot: number,
  resolver: Resolver,
): NoteHere[] {
  if (notes.length === 0) return []
  const editions = [...new Set(notes.map((n) => n.edition))]
  const counterparts = counterpartSlots(editions, viewing, slot, resolver)
  const here = notes.filter((n) => counterparts.get(n.edition) === n.slot)
  return [
    ...here.filter((n) => n.edition === viewing).map((note) => ({ note, travelled: false })),
    ...here.filter((n) => n.edition !== viewing).map((note) => ({ note, travelled: true })),
  ]
}

/**
 * Every note, in the order they are held, each with the slot it lands on here
 * — or `undefined` where it lands nowhere.
 *
 * A note whose slot has no counterpart in the edition being read **stays in
 * the list, marked**, rather than disappearing: silence is the failure this
 * project is written against, and a note that vanished when you swapped
 * editions would be indistinguishable from one that was never saved.
 *
 * Resolved per note here, not per edition as `notesHere` does — the review
 * list is opened deliberately, once, where `notesHere` runs on every render of
 * every slot, and per-note resolution is the direct expression of the question
 * this answers ("where does *this* note land").
 */
export function reviewList(notes: Note[], viewing: string, resolver: Resolver): ReviewEntry[] {
  return notes.map((note) => {
    if (note.edition === viewing) return { note, here: note.slot }
    const at = resolver.slotAt(note.edition, note.slot)
    const landed = at && resolver.resolve(note.edition, viewing, at)
    const here = landed ? resolver.indexOf(viewing, landed) : undefined
    return { note, here }
  })
}

/**
 * Where a note's margin tab sits, as a fraction of the slot's height.
 *
 * The middle of its region, or the middle of the slot for a whole-slot note.
 * Fractions in, fraction out — the arithmetic is scale-free, so a tab tracks
 * at any fit scale with nothing to recompute.
 */
export function tabOffset(note: Note): number {
  return note.region ? note.region.y + note.region.h / 2 : 0.5
}

const clamp = (v: number) => Math.min(Math.max(v, 0), 1)

/**
 * A drag, in slot fractions, as a region.
 *
 * Normalised so dragging up-and-left produces the same rectangle as
 * down-and-right, and clamped to the slot so a drag that leaves the page
 * still names something inside it — the server refuses a region outside
 * `0..1`, and refusing a reader's note because their pointer left the image is
 * not an answer.
 */
export function regionFromDrag(
  a: { x: number; y: number },
  b: { x: number; y: number },
): Region {
  const x = clamp(Math.min(a.x, b.x))
  const y = clamp(Math.min(a.y, b.y))
  return { x, y, w: clamp(Math.max(a.x, b.x)) - x, h: clamp(Math.max(a.y, b.y)) - y }
}

/** Add a note, or replace the one already holding its id. */
export function withNote(notes: Note[], note: Note): Note[] {
  const at = notes.findIndex((n) => n.id === note.id)
  if (at < 0) return [...notes, note]
  return notes.map((n, i) => (i === at ? note : n))
}

/** Drop a note by id. */
export function withoutNote(notes: Note[], id: string): Note[] {
  return notes.filter((n) => n.id !== id)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd web && npx vitest run src/notes.test.ts`
Expected: all pass, including the exhaustive round-trip.

- [ ] **Step 5: Commit**

```bash
cd web && npm test && npm run typecheck && npm run lint && cd ..
git add web/src/notes.ts web/src/notes.test.ts
git commit -m "A note travels through the alignment, and says when it cannot"
```

---

### Task 7: the `annotations` module, its readout, and the API client

**Files:**
- Create: `web/src/modules/annotations.ts`
- Create: `web/src/modules/annotations.test.ts`
- Modify: `web/src/modules/registry.ts`
- Modify: `web/src/modules/readouts.ts` (a fifth segment) and `web/src/modules/readouts.test.ts`
- Modify: `web/src/api.ts` and `web/src/api.test.ts`

**Interfaces:**
- Produces:
  - `annotationsModule: ReaderModule<AnnotationsPrefs>` with `AnnotationsPrefs = { markers: boolean; travel: boolean; reveal: boolean }`
  - `NEW_INTENT = 'annotations:new'`, `REVEAL_INTENT = 'annotations:reveal'`, `REVIEW_INTENT = 'annotations:review'`
  - `noteCountLine(here: number, total: number): string | undefined`
  - `ReadoutsPrefs.notes: boolean`, `RowLines.notes?: string`
  - `getNotes(work): Promise<FetchedNotes>`, `putNotes(work, etag, doc): Promise<FetchedNotes>`

- [ ] **Step 1: Write the failing tests**

Create `web/src/modules/annotations.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { collisions } from '../module'
import { registry } from './registry'
import { annotationsModule, noteCountLine } from './annotations'

describe('annotationsModule', () => {
  it('claims three keys and collides with nothing already registered', () => {
    expect(annotationsModule.keys?.map((k) => k.key)).toEqual(['n', 'r', 'a'])
    expect(collisions(registry.all())).toEqual([])
  })

  it('is registered, so its preferences are wired and the panel shows it', () => {
    expect(registry.ids).toContain('annotations')
  })

  // Every default is a primitive, which is what makes `Settings.tsx` generate
  // a working control for each — a nested record renders read-only (see
  // `settingsPanel.ts`'s `fieldKind`), which is right for a per-edition map
  // and wrong for a plain toggle.
  it('declares only primitives, so every setting is editable in the panel', () => {
    for (const value of Object.values(annotationsModule.defaults)) {
      expect(typeof value).toBe('boolean')
    }
  })
})

describe('noteCountLine', () => {
  it('says nothing where the work has no notes at all', () => {
    expect(noteCountLine(0, 0)).toBeUndefined()
  })

  it('counts what is on this page against the work', () => {
    expect(noteCountLine(2, 7)).toBe('2 notes here of 7')
  })

  it('says none here rather than a zero that reads like a bug', () => {
    expect(noteCountLine(0, 7)).toBe('no notes here of 7')
  })

  it('does not pluralise one', () => {
    expect(noteCountLine(1, 1)).toBe('1 note here of 1')
  })
})
```

Add to `web/src/modules/readouts.test.ts`:

```ts
  it('carries a fifth segment for notes, after the four reading lines', () => {
    expect(
      readoutRow({ position: 'slot 1 / 2', notes: '1 note here of 1' }),
    ).toBe('slot 1 / 2 · 1 note here of 1')
  })
```

Add to `web/src/api.test.ts` (matching the file's existing `fetch`-stub idiom):

```ts
  it('reads notes and the validator a write has to carry', async () => {
    const calls: [string, RequestInit | undefined][] = []
    stubFetch(calls, new Response('{"work":"w","notes":[]}', { headers: { etag: '"n1"' } }))
    const fetched = await getNotes('w')
    expect(calls[0][0]).toBe('/api/works/w/notes')
    expect(fetched.etag).toBe('"n1"')
    expect(fetched.doc.notes).toEqual([])
  })

  it('writes notes conditionally, and never without a validator', async () => {
    const calls: [string, RequestInit | undefined][] = []
    stubFetch(calls, new Response('{"work":"w","notes":[]}', { headers: { etag: '"n2"' } }))
    await putNotes('w', '"n1"', { work: 'w', notes: [] })
    const [, init] = calls[0]
    expect(init?.method).toBe('PUT')
    expect(new Headers(init?.headers).get('if-match')).toBe('"n1"')
  })
```

> If `api.test.ts` has no `stubFetch` helper, use whatever that file already
> does to stand in for `fetch` — read it first and follow it exactly rather
> than introducing a second idiom.

- [ ] **Step 2: Run to verify they fail**

Run: `cd web && npx vitest run src/modules/annotations.test.ts src/modules/readouts.test.ts src/api.test.ts`
Expected: FAIL — `./annotations` does not resolve, `readoutRow` ignores `notes`,
`getNotes`/`putNotes` are not exported.

- [ ] **Step 3: Implement the module**

Create `web/src/modules/annotations.ts`:

```ts
import type { ReaderModule } from '../module'

export interface AnnotationsPrefs {
  /** Draw a margin tab per note on the page being read. */
  markers: boolean
  /** Show notes written on other editions, resolved through the alignment. */
  travel: boolean
  /** Tint every annotated region on this page at once. */
  reveal: boolean
}

/** `n` — start a note on the slot being read. */
export const NEW_INTENT = 'annotations:new'
/** `r` — tint the regions, for as long as it is switched on. */
export const REVEAL_INTENT = 'annotations:reveal'
/** `a` — the review list, every note on the work. */
export const REVIEW_INTENT = 'annotations:review'

/**
 * Notes on the page, and the three gestures they need.
 *
 * `travel` defaults on because a note written on the raw being readable while
 * reading the English is the entire feature — the one thing no other reader
 * can offer, and the sidecar earning its keep a second time. `reveal` defaults
 * off: always-tinted regions are intrusive on a busy page, which is why the
 * reveal is a key rather than a permanent state.
 */
export const annotationsModule: ReaderModule<AnnotationsPrefs> = {
  id: 'annotations',
  label: 'Annotations',
  defaults: { markers: true, travel: true, reveal: false },
  keys: [
    { key: 'n', intent: NEW_INTENT },
    { key: 'r', intent: REVEAL_INTENT },
    { key: 'a', intent: REVIEW_INTENT },
  ],
}

/**
 * The readout's note count: how many are on this page, out of the work.
 *
 * Nothing at all where the work has no notes — a reader who has never
 * annotated anything should not carry a permanent `no notes here of 0`, the
 * same reasoning `healthReadout` uses to say nothing about a sound pair.
 * `no notes here` rather than `0 notes here` once some exist, because a zero
 * in a row of numbers reads like a figure that failed to compute.
 */
export function noteCountLine(here: number, total: number): string | undefined {
  if (total === 0) return undefined
  const count = here === 0 ? 'no notes' : `${here} note${here === 1 ? '' : 's'}`
  return `${count} here of ${total}`
}
```

Register it in `web/src/modules/registry.ts` — add the import and append
`annotationsModule` to the `modules` array, before `settingsModule` (which
stays last, since it claims the panel's own key and renders no section).

- [ ] **Step 4: Add the fifth readout segment**

In `web/src/modules/readouts.ts`:

- add `notes: boolean` to `ReadoutsPrefs` and `notes: true` to
  `readoutsModule.defaults`;
- add `notes?: string` to `RowLines`;
- add it to `readoutRow`'s fixed segment order, **last** — the four reading
  lines describe where you are and whether to trust the alignment, and a note
  count is the one segment a reader can afford to lose first when the row is
  clipped.

Update `readoutRow`'s doc comment to say the order is five long and why `notes`
is last.

- [ ] **Step 5: Add the API client**

In `web/src/api.ts`, after `putProgress`:

```ts
/** The notes document and the validator a write has to be conditional on. */
export interface FetchedNotes {
  doc: NotesDoc
  /** Absent only from a server older than the notes route. */
  etag?: string
}

/**
 * Every note on a work.
 *
 * A work nobody has annotated answers with an empty document rather than a
 * 204, unlike `getProgress` — a `PUT` requires `If-Match`, and a client that
 * had never seen a document would have no validator for its first write. So
 * there is no "no notes yet" case to handle here: there is always a document
 * and always a tag.
 */
export async function getNotes(work: string): Promise<FetchedNotes> {
  const res = await fetch(`/api/works/${seg(work)}/notes`)
  if (!res.ok) throw new Error(`reading notes: ${res.status} ${await res.text()}`)
  return { doc: (await res.json()) as NotesDoc, etag: res.headers.get('etag') ?? undefined }
}

/**
 * Replace a work's notes, conditionally.
 *
 * `If-Match` is required by the server rather than optional here, the same as
 * `patchAlignment`: two devices annotating one work is the same shape as two
 * correcting it, and last-write-wins would discard a note with nothing
 * anywhere saying so. The response body is the stored document, so a caller
 * refreshes from this rather than making a second request that could already
 * be stale.
 *
 * No `keepalive`: a note is a deliberate act the reader is watching the result
 * of, not a background write like `putProgress`.
 */
export async function putNotes(
  work: string,
  etag: string,
  doc: NotesDoc,
): Promise<FetchedNotes> {
  const res = await fetch(`/api/works/${seg(work)}/notes`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json', 'if-match': etag },
    body: JSON.stringify(doc),
  })
  if (!res.ok) throw new Error(`writing notes: ${res.status} ${await res.text()}`)
  return { doc: (await res.json()) as NotesDoc, etag: res.headers.get('etag') ?? undefined }
}
```

with `import type { NotesDoc } from './notes'` at the top.

- [ ] **Step 6: Run to verify they pass**

Run: `cd web && npm test`
Expected: every suite green, including `module.test.ts`'s collision check and
`settingsPanel.test.ts` (whose generated-panel assertions now see a sixth
module).

- [ ] **Step 7: Commit**

```bash
cd web && npm test && npm run typecheck && npm run lint && cd ..
git add web/src/modules/annotations.ts web/src/modules/annotations.test.ts \
        web/src/modules/registry.ts web/src/modules/readouts.ts \
        web/src/modules/readouts.test.ts web/src/api.ts web/src/api.test.ts
git commit -m "An annotations module, its readout line, and its two routes"
```

---

### Task 8: the overlay — margin tabs, and tints while revealed

**Files:**
- Create: `web/src/Annotations.tsx`
- Modify: none yet (mounted in Task 10)

**Interfaces:**
- Consumes: `NoteHere`, `tabOffset` from `./notes`; `AnnotationsPrefs` from `./modules/annotations`.
- Produces: `<Annotations here={NoteHere[]} prefs={AnnotationsPrefs} onOpen={(id: string) => void} />`

No test file: this is layout, and a `.tsx` module is not Vitest-reachable
(`App.tsx`'s own comment records why). Everything with a rule in it —
`tabOffset`, `notesHere` — is already pure and tested in Task 6. What this file
must not do is derive anything.

- [ ] **Step 1: Write the component**

Create `web/src/Annotations.tsx`:

```tsx
import type { CSSProperties } from 'react'
import type { AnnotationsPrefs } from './modules/annotations'
import { tabOffset, type NoteHere } from './notes'

/**
 * Notes on the page: a tab in the margin for each, and — while revealed — a
 * tint over each region.
 *
 * Composited **above** the page rather than inside it, so no other module's
 * transform can move a marker away from the region it names. Nothing here is
 * derived: `here` is `notes.ts`'s `notesHere` and each tab's position is its
 * `tabOffset`, both pure and tested. This file lays them out and does not
 * decide anything.
 *
 * `pointerEvents: 'none'` on the layer, `'auto'` on the tabs themselves: the
 * overlay must never swallow a click meant for the page, and a tab must still
 * be clickable.
 *
 * **A travelled note never draws a tint.** Its region belongs to the slot it
 * was written on, and transforming one between editions is L1 geometry — a
 * *Later* item. Drawing the untransformed box here would put it over the wrong
 * panel, which is the reader lying about what it knows; the tab says the note
 * is here, which is all this build actually knows.
 */
const layerStyle: CSSProperties = {
  position: 'absolute',
  inset: 0,
  pointerEvents: 'none',
  zIndex: 2,
}

const tabStyle = (offset: number, travelled: boolean): CSSProperties => ({
  position: 'absolute',
  right: 2,
  top: `${offset * 100}%`,
  transform: 'translateY(-50%)',
  pointerEvents: 'auto',
  width: 10,
  height: 26,
  padding: 0,
  border: 'none',
  borderRadius: '3px 0 0 3px',
  // A travelled note is drawn hollow rather than in a second colour: colour
  // would have to mean something else again once a third state exists, and
  // "solid is written here, outline is written elsewhere" survives that.
  background: travelled ? 'rgba(255, 214, 102, 0.35)' : 'rgba(255, 214, 102, 0.92)',
  boxShadow: travelled ? 'inset 0 0 0 1px rgba(255, 214, 102, 0.92)' : 'none',
  cursor: 'pointer',
})

const tintStyle = (region: NonNullable<NoteHere['note']['region']>): CSSProperties => ({
  position: 'absolute',
  left: `${region.x * 100}%`,
  top: `${region.y * 100}%`,
  width: `${region.w * 100}%`,
  height: `${region.h * 100}%`,
  background: 'rgba(255, 214, 102, 0.22)',
  outline: '1px solid rgba(255, 214, 102, 0.6)',
})

export interface AnnotationsProps {
  here: NoteHere[]
  prefs: AnnotationsPrefs
  onOpen: (id: string) => void
}

export function Annotations({ here, prefs, onOpen }: AnnotationsProps) {
  if (!prefs.markers && !prefs.reveal) return null
  const shown = prefs.travel ? here : here.filter((h) => !h.travelled)
  if (shown.length === 0) return null
  return (
    <div style={layerStyle} aria-hidden={false}>
      {prefs.reveal &&
        shown
          .filter((h) => !h.travelled && h.note.region)
          .map((h) => <div key={`tint-${h.note.id}`} style={tintStyle(h.note.region!)} />)}
      {prefs.markers &&
        shown.map((h) => (
          <button
            key={h.note.id}
            type="button"
            style={tabStyle(tabOffset(h.note), h.travelled)}
            onClick={() => onOpen(h.note.id)}
            title={
              h.travelled
                ? `note on ${h.note.edition}: ${h.note.body}`
                : h.note.body
            }
            aria-label={
              h.travelled
                ? `Note written on ${h.note.edition}, ${h.note.body}`
                : `Note, ${h.note.body}`
            }
          />
        ))}
    </div>
  )
}
```

- [ ] **Step 2: Typecheck and lint**

Run: `cd web && npm run typecheck && npm run lint`
Expected: clean. There is nothing to run in Vitest — say so rather than adding a
test that imports a `.tsx`.

- [ ] **Step 3: Commit**

```bash
git add web/src/Annotations.tsx
git commit -m "Margin tabs on the page, tints when revealed"
```

---

### Task 9: the editor, the review list, and export

**Files:**
- Create: `web/src/NotePanel.tsx`
- Modify: `web/src/notes.ts` (add `exportBlob`, `exportName`) and `web/src/notes.test.ts`

**Interfaces:**
- Produces:
  - `notes.ts`: `exportName(work: string): string`, `notesJson(doc: NotesDoc): string`
  - `<NotePanel …/>` with props: `open`, `mode` (`'edit' | 'review'`), `draft`,
    `entries`, `work`, `viewing`, `busy`, `notice`, and callbacks
    `onBody`, `onSave`, `onDelete`, `onClose`, `onGo`.

- [ ] **Step 1: Write the failing test for the pure half**

Add to `web/src/notes.test.ts`:

```ts
describe('export', () => {
  it('names the file after the work, so two exports do not collide', () => {
    expect(exportName('go-toubun-v01')).toBe('go-toubun-v01.notes.json')
  })

  it('exports exactly what the server holds, formatted to be read', () => {
    const doc = { work: 'w', notes: [note('1', 'a', 0)] }
    expect(JSON.parse(notesJson(doc))).toEqual(doc)
    expect(notesJson(doc)).toContain('\n  ')
  })
})
```

with `exportName, notesJson` added to the file's import list.

- [ ] **Step 2: Run to verify it fails**

Run: `cd web && npx vitest run src/notes.test.ts`
Expected: FAIL — neither is exported.

- [ ] **Step 3: Implement the pure half**

Append to `web/src/notes.ts`:

```ts
/**
 * The exported document, formatted to be read by a person as well as parsed.
 *
 * Exactly what the server holds — the same shape, not a rendering — so an
 * export can be put back. A prose rendering would be a second format to keep
 * in step with the first, and this one is already the interchange format.
 */
export function notesJson(doc: NotesDoc): string {
  return JSON.stringify(doc, null, 2)
}

/** What an exported file is called: the work's own id, plus the suffix the server uses. */
export function exportName(work: string): string {
  return `${work}.notes.json`
}
```

- [ ] **Step 4: Write the panel**

Create `web/src/NotePanel.tsx`:

```tsx
import type { CSSProperties } from 'react'
import { exportName, notesJson, type Note, type NotesDoc, type ReviewEntry } from './notes'

const panelStyle: CSSProperties = {
  position: 'absolute',
  top: 16,
  right: 16,
  width: 360,
  maxHeight: '70vh',
  overflowY: 'auto',
  zIndex: 3,
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 12,
  borderRadius: 8,
  background: 'rgba(24, 24, 24, 0.94)',
  color: 'rgba(255, 255, 255, 0.92)',
}

const rowStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 2,
  padding: '6px 0',
  borderTop: '1px solid rgba(255, 255, 255, 0.12)',
  textAlign: 'left',
}

export interface NotePanelProps {
  mode: 'edit' | 'review' | undefined
  /** The note being written or edited, while `mode` is `'edit'`. */
  draft: Note | undefined
  /** Every note on the work, with where each lands here — `notes.ts`'s `reviewList`. */
  entries: ReviewEntry[]
  doc: NotesDoc
  work: string
  viewing: string
  /** Whether a write is on the wire. Both exits are disabled for the duration. */
  busy: boolean
  notice: string | undefined
  onBody: (body: string) => void
  onSave: () => void
  onDelete: (id: string) => void
  onClose: () => void
  /** Take the reader to a note's landing slot in the edition being read. */
  onGo: (entry: ReviewEntry) => void
}

/**
 * The two surfaces a note needs that the page itself cannot hold: writing one,
 * and finding every one.
 *
 * One component rather than two because they share the same box, the same
 * busy/notice handling and the same close, and a reader moves between them —
 * saving a note and then looking at the list is one gesture apart.
 *
 * **An unreachable note is listed, marked, and still openable.** It is the
 * case the design names explicitly: a note whose slot has no counterpart in
 * the edition being read does not disappear, because a note that vanished on a
 * swap is indistinguishable from one that was never saved.
 *
 * Export is a download of exactly what the server holds (`notesJson`), built
 * from an object URL and revoked immediately — a note is a reader's own
 * writing, and getting it out of this tool must not require the tool.
 */
export function NotePanel({
  mode,
  draft,
  entries,
  doc,
  work,
  viewing,
  busy,
  notice,
  onBody,
  onSave,
  onDelete,
  onClose,
  onGo,
}: NotePanelProps) {
  if (!mode) return null

  function download() {
    const url = URL.createObjectURL(new Blob([notesJson(doc)], { type: 'application/json' }))
    const link = document.createElement('a')
    link.href = url
    link.download = exportName(work)
    link.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div style={panelStyle} role="dialog" aria-label={mode === 'edit' ? 'Note' : 'All notes'}>
      {mode === 'edit' && draft && (
        <>
          <p style={{ margin: 0, opacity: 0.8 }}>
            {draft.edition} slot {draft.slot}
            {draft.region ? ' · region' : ' · whole page'}
          </p>
          <textarea
            value={draft.body}
            onChange={(e) => onBody(e.target.value)}
            rows={5}
            autoFocus
            aria-label="Note body"
            style={{ width: '100%', resize: 'vertical' }}
          />
          {notice && <p style={{ margin: 0, opacity: 0.85 }}>{notice}</p>}
          <div style={{ display: 'flex', gap: 8 }}>
            <button type="button" disabled={busy} onClick={onSave}>
              {busy ? 'Saving…' : 'Save'}
            </button>
            <button type="button" disabled={busy} onClick={() => onDelete(draft.id)}>
              Delete
            </button>
            <button type="button" disabled={busy} onClick={onClose}>
              Close
            </button>
          </div>
        </>
      )}
      {mode === 'review' && (
        <>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <strong>{entries.length} note{entries.length === 1 ? '' : 's'}</strong>
            <span style={{ display: 'flex', gap: 8 }}>
              <button type="button" onClick={download}>
                Export
              </button>
              <button type="button" onClick={onClose}>
                Close
              </button>
            </span>
          </div>
          {entries.length === 0 && (
            <p style={{ margin: 0, opacity: 0.8 }}>
              Nothing written yet. <em>n</em> starts a note on the page you are reading.
            </p>
          )}
          {entries.map((entry) => (
            <button
              key={entry.note.id}
              type="button"
              style={rowStyle}
              onClick={() => onGo(entry)}
              disabled={entry.here === undefined}
            >
              <span style={{ opacity: 0.75, fontSize: '0.85em' }}>
                {entry.note.edition} slot {entry.note.slot}
                {entry.here === undefined
                  ? ` · not reachable from ${viewing}`
                  : entry.note.edition === viewing
                    ? ''
                    : ` · ${viewing} slot ${entry.here}`}
              </span>
              <span>{entry.note.body || <em>empty</em>}</span>
            </button>
          ))}
        </>
      )}
    </div>
  )
}
```

- [ ] **Step 5: Run and commit**

```bash
cd web && npm test && npm run typecheck && npm run lint && cd ..
git add web/src/NotePanel.tsx web/src/notes.ts web/src/notes.test.ts
git commit -m "Write a note, find every note, take them with you"
```

---

### Task 10: wiring, a browser pass against the real corpus, and the documents

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `docs/status.md`, `docs/notes.md`

**Interfaces:**
- Consumes: everything above.
- Produces: no new exports. This task is wiring and evidence.

- [ ] **Step 1: Load notes with the session**

In `App.tsx`'s load effect, after `getProgress`, add `getNotes(work)` and store
both the document and its `ETag`:

```tsx
  const [notesDoc, setNotesDoc] = useState<NotesDoc>()
  const [notesEtag, setNotesEtag] = useState<string>()
```

Fetched **alongside** progress rather than before it — neither depends on the
other, and a work with no notes still answers a document, so there is no branch
here. A failure loading notes must not stop the reader from reading: catch it,
`console.error`, and leave `notesDoc` at an empty document, since a reader who
cannot see their notes should still get their pages.

- [ ] **Step 2: Dispatch the three keys**

Add three cases to the existing `switch (intent.intent)` in the keydown
handler, in the same shape the others take:

- `NEW_INTENT` — read `sessionRef.current`, `display` it, return if `kind !==
  'at'` (a gap has no slot to annotate), and set a draft note:
  `{ id: ulid(Date.now(), cryptoRandom), edition: shown.edition, slot: shown.slot, body: '', created: now, updated: now }`
  with `panelMode` set to `'edit'`.
- `REVEAL_INTENT` — flip `annotations.reveal` in `prefsMap`, exactly as
  `TOGGLE_INTENT` flips `direction.override`, through `setPrefsMap`.
- `REVIEW_INTENT` — set `panelMode` to `'review'` (or clear it, if already
  review — the key toggles, like `s` and `Tab` do).

`Escape` closes the panel: extend the existing `Escape` branch so it clears
`panelMode` when there is no `pending`, and leave the correction behaviour
first — a reader mid-correction pressing `Escape` means the correction.

- [ ] **Step 3: Compute and mount**

At render, after `shown.kind === 'at'` is established:

```tsx
  const allNotes = notesDoc?.notes ?? []
  const here = annotationsPrefs.travel
    ? notesHere(allNotes, shown.edition, shown.slot, resolver)
    : notesHere(allNotes.filter((n) => n.edition === shown.edition), shown.edition, shown.slot, resolver)
```

Mount `<Annotations>` inside the page-area div (which needs `position:
'relative'` for the overlay's `inset: 0` to mean the page area). Mount
`<NotePanel>` beside `<Settings>`.

Pass the note count to `Chrome` as a fifth readout, computed with
`noteCountLine(here.length, allNotes.length)` and gated by
`readoutsPrefs.notes` inside `Chrome` like the other four.

- [ ] **Step 4: Save**

`saveNote` mirrors `confirmCorrection`'s shape exactly — it is the same problem:

```tsx
  async function saveNote(next: Note) {
    if (!notesDoc || notesEtag === undefined || notesBusy) return
    setNotesBusy(true)
    try {
      const doc = { work: notesDoc.work, notes: withNote(notesDoc.notes, next) }
      const fresh = await putNotes(work, notesEtag, doc)
      setNotesDoc(fresh.doc)
      setNotesEtag(fresh.etag)
      setPanelMode(undefined)
      setNotesNotice(undefined)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      // The server's own wording for a 412 (`routes.rs`'s `put_notes`),
      // matched by substring like the correction path's. The draft survives:
      // re-reading gets the reader a current document to write against, not a
      // silently retried write that could clobber whatever arrived.
      if (message.includes('changed since you read them')) {
        try {
          const fresh = await getNotes(work)
          setNotesDoc(fresh.doc)
          setNotesEtag(fresh.etag)
        } catch (rereadErr) {
          console.error(rereadErr)
        }
        setNotesNotice('These notes changed elsewhere — they have been re-read. Save again to keep yours.')
      } else {
        setNotesNotice(message)
      }
    } finally {
      setNotesBusy(false)
    }
  }
```

Delete is the same call with `withoutNote`.

- [ ] **Step 5: Drag-to-region**

While `panelMode === 'edit'`, a pointer drag on the page area sets the draft's
region: record `pointerdown` and `pointerup` positions as fractions of the page
element's bounding box, and call `regionFromDrag`. All the arithmetic is
`notes.ts`'s and already tested; this is two event handlers and a
`getBoundingClientRect`.

- [ ] **Step 6: Drive it against the real corpus, in a real browser**

This is the step M5b's Task 11 and M5d's manual pass both earned their findings
from. Serve `corpus/gotoubun-v01` and check each of these, recording what was
observed in `docs/notes.md`:

1. `n` on `raw-mono` slot 88 → type → Save → the tab appears in the margin.
2. Reload → the tab is still there (it is on the server, not in the tab).
3. `Space` to `en-mono` → **the note is on the corresponding slot, drawn
   hollow** — the milestone's whole end state, and the first time it has ever
   been true.
4. `r` → the region tints on the edition it was written on, and **does not**
   tint on the other one.
5. `a` → the review list holds it, and holds a note written on a slot with no
   counterpart marked "not reachable from here" rather than missing.
6. Export → the downloaded file parses and matches what `GET /notes` returns.
7. Settings: switch `markers` off (tabs go), `travel` off (the note disappears
   while reading the other edition and returns on its own), `reveal` off.
   Reload after each and confirm it persisted.
8. Two tabs open on one work: save in one, then save in the other → the second
   is told, re-reads, and neither note is lost.

Record the count of notes written, the slots, and what each check showed.
**A check that fails is a finding, not a step to repeat until it passes.**

- [ ] **Step 7: The documents**

`docs/notes.md` gains a section — "M5c, measured: a note written on the raw,
read on the English" — with the browser pass's actual observations, and the
prototype table from this plan's own "What the prototype measured" (which is
evidence, not planning, and belongs in the log).

`docs/status.md`:
- header date;
- test counts, re-measured, in both places they appear;
- "Where it stands" gains a sentence on M5c;
- "What is built": the `web/` row gains the module (making it six), and the
  `taiyaku-serve` row gains the notes routes;
- §3 "The reader" gains an **M5c has landed** paragraph, saying what shipped
  *and* what it does not do: a region does not travel, a concurrent write is
  refused rather than merged, and export is the interchange format rather than
  a rendering;
- "What remains" drops `annotations` and states what is left.

If the browser pass turned up something not worth blocking on, it becomes O25
in the open-items table, with the same five-part edit every other open item
gets: the table row, the detail entry, the count sentence, the "Opened by"
column, and a `notes.md` measurement behind it.

- [ ] **Step 8: Full gate and commit**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
cd web && npm test && npm run typecheck && npm run lint && cd ..
git add web/src/App.tsx && git commit -m "Notes on the page, in the margin, and in one list"
git add docs/ && git commit -m "M5c: a note written on the raw is readable on the English"
```

---

## Self-review against the spec

**Spec coverage.** Every M5c line in
`docs/superpowers/specs/2026-08-02-reader-m5-design.md`:

| Spec requirement | Task |
| ---------------- | ---- |
| `<stem>.notes.json` beside the sidecar | 1 |
| Written atomically, temp name unique per attempt | 1 (via `crate::atomic::write`) |
| Regions are fractions, anchored to a slot, `region` optional | 1, 4, 6 |
| ULID ids | 5 |
| `GET`/`PUT /api/works/:work/notes`, `If-Match` on write | 2, 3 |
| Notes travel through `resolve_slot`; regions do not | 6, 8 |
| A note with no counterpart surfaces marked, not silently | 6 (`reviewList`), 9 |
| `annotations` module: markers, travel, reveal; new/reveal/review keys | 7 |
| Margin tabs; one key tints every region | 7, 8 |
| Note count as a status readout | 7 |
| Review-all and export | 9 |
| Note round-trip seam test, exhaustive | 6 |
| Keybinding-collision seam test | 7 |

**Deliberately not built, and why:** the design's `surface`/`transform`/`status`
fields on `ReaderModule` (deviation 1); a server-side merge of two concurrent
writers (stated as the `PUT`'s cost in Task 3, and the same bargain
`patch_alignments` already makes); region transformation on travel (L1, a
*Later* item, and the design says so in the same breath as specifying travel).

**Type consistency.** `Note`/`Region`/`NotesDoc` are declared once in Rust
(Task 1) and mirrored once in TypeScript (Task 6), field for field, with
`region` optional on both sides and omitted rather than `null` when absent — the
round-trip test in Task 1 pins that spelling, which is what keeps the two
declarations honest without a shared schema.

**One thing this plan does not answer**, recorded rather than smuggled: what a
note anchored to a slot means after that edition is *replaced* by a re-solve.
O24 asks the same question about a solver-written direct edge, and neither has
a case in hand. A note names an edition key and a slot index, both of which
survive a re-solve of the alignment; what would break it is an edition's
archive being swapped for one with a different page count, which is the case
`merge_spoke` already refuses when hand corrections are present and does not
consider notes at all. Worth an open item if the browser pass makes it concrete.

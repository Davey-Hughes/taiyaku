# M5a — Page-view core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A browser reader that displays a page from `taiyaku serve` in three reading modes and swaps it for the same slot in another edition on a keypress.

**Architecture:** Rust first, then TypeScript. Two gaps in the Rust are closed before any web code exists — the binding cannot convert a flat slot index into the `(page, half)` pair it resolves on, and the server has no thumbnail route — then `taiyaku-wasm` emits a real TS package and is executed under a JS runtime for the first time. The web reader is a Vite/React app whose entire reading model is a pure state machine behind a `Resolver` interface, so it is exhaustively unit-testable with no WASM and no network.

**Tech Stack:** Rust 2024, `wasm-bindgen`, `wasm-pack`; Vite + React 19 + TypeScript + Vitest.

## Global Constraints

- **`taiyaku-core` must keep building for `wasm32-unknown-unknown`.** Task 1 is the only task touching it and adds pure arithmetic only.
- **`wasm-bindgen` goes in `taiyaku-wasm`, never in `taiyaku-core`.** The core's public API must not be shaped by what can cross the JavaScript boundary.
- **No new Rust dependencies.** The workspace set is `image`, `zip`, `sha2`, `rayon`, `base64`, `serde`, plus `axum`, `tokio`, `tower`, `ureq` in the server and `wasm-bindgen` in the binding.
- **Rust edition 2024, `rust-version = "1.85"`, `unsafe_code = "forbid"`.** Workspace lints deny `clippy::todo` and `clippy::dbg_macro` at warn; CI runs `cargo clippy -D warnings`, so a warning is a failure.
- **The page sequence is normative.** Derive it through `taiyaku-read` or `taiyaku_core::Edition`, never by reimplementing it.
- **A slot is the addressed unit, never a page.** Progress, resolution and the reader's authority are all slots.
- **No CI job may use a JavaScript action.** `docs/ci.md`: `rust:1-bookworm` carries no node, so any `uses:` step dies before its first step. Every step is a `run:`.
- **Never crop, distort or force-fill a page.** Fit whole, aspect preserved, downscale only, no upscaling past 100%.

## Spec

[`docs/superpowers/specs/2026-08-02-reader-m5-design.md`](../specs/2026-08-02-reader-m5-design.md), the **M5a** row of its staging table.

## What this plan deliberately does not do

Each is specced and belongs to a later plan. Named so their absence is not read as an oversight.

- **The module system and the six modules** — M5b. This plan hardcodes the behaviours a module would later own (fit-whole, no animation, single-page default) and does not build the contract. A plugin interface with no plugins is an untested guess.
- **Annotations** — M5c. **Correction UI and O13** — M5d.
- **Prefetch and the double-buffered decode** — the `prefetch` module, M5b. This plan renders one page at a time and will flicker on a fast flick. That is expected here and is the defect M5b closes.
- **Thumbnail cache pruning.** Task 2 ships the cache and measures it. O11 asks for a number before a policy.
- **Running the *compiled* module in a browser under test.** Task 3 executes it under Node, which is the first execution of any kind. Playwright over a real browser arrives with the reading flow in M5b.

## File Structure

**Rust — modified:**

| File | Responsibility after this plan |
| ---- | ------------------------------ |
| `crates/taiyaku-wasm/src/binding.rs` | adds `slot_at` and `index_of` as plain Rust, tested natively |
| `crates/taiyaku-wasm/src/lib.rs` | adds the `WasmPosition` type and the two `wasm_bindgen` shells |
| `crates/taiyaku-serve/src/cover.rs` | generation parameterised by long edge; gains the per-index thumbnail key |
| `crates/taiyaku-serve/src/routes.rs` | adds `GET …/pages/{index}/thumb` |
| `.forgejo/workflows/ci.yml` | adds a `web` job |
| `docs/ci.md`, `docs/status.md` | record the new job and the thumbnail cache measurement |

**Rust — created:** none. Both changes extend files that already own the concern.

**Web — created:**

| File | Responsibility |
| ---- | -------------- |
| `web/src/api.ts` | typed fetch over the four `/api` routes and the thumb route. Knows HTTP, knows nothing about reading |
| `web/src/resolver.ts` | the `Resolver` interface, and the WASM-backed implementation |
| `web/src/reading.ts` | the authority state machine. Pure functions, no React, no fetch, no WASM |
| `web/src/pairing.ts` | slot → display-group arithmetic for the three reading modes. Pure |
| `web/src/App.tsx` | wiring: state machine ↔ components |
| `web/src/PageView.tsx` | renders one display group, fit-whole |
| `web/src/Chrome.tsx` | the floating strip container and the readout row, shown and hidden together |
| `web/src/Filmstrip.tsx` | thumbnails, click to navigate |
| `web/src/keys.ts` | keydown → intent |
| `web/src/fixtures.ts` | the sidecar fixtures the tests share |

`reading.ts` and `pairing.ts` hold every rule worth testing and import nothing. That is the decomposition this plan is built around: the reading model is provable without a browser.

---

### Task 1: Convert a flat slot index to the pair the binding resolves on

The reader's authority is `slot: number` — that is what `progress` stores and what the spec's authority model tracks. The binding resolves on `(page, half)`. **Nothing exposes the conversion between them**, so the reader as specced cannot call the binding at all. Reimplementing slot expansion in TypeScript is rejected by the design: two implementations of the format's trickiest logic, one of which has the corpus behind it.

**Files:**
- Modify: `crates/taiyaku-wasm/src/binding.rs`
- Modify: `crates/taiyaku-wasm/src/lib.rs`

**Interfaces:**
- Consumes: `taiyaku_core::Edition::slots(Direction) -> Vec<Slot>`, `taiyaku_core::Slot { page: u32, half: Option<Half> }` — both exist.
- Produces: `Binding::slot_at(&self, edition: &str, index: u32) -> Option<Slot>`; `Binding::index_of(&self, edition: &str, slot: Slot) -> Option<u32>`; and across the boundary `slotAt(edition, index) -> WasmPosition | undefined`, `indexOf(edition, page, half) -> number | undefined`.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-wasm/src/binding.rs`, inside the existing `#[cfg(test)] mod tests`:

```rust
/// `b`'s page 2 is a joined spread, so its slot sequence is
/// 0, 1, 2R, 2L, 3, 4, 5, 6, 7 under `rtl` — nine slots for eight pages.
/// Index 2 and index 3 are the two halves of one page, which is the whole
/// reason a flat index cannot be a page number.
#[test]
fn slot_at_walks_a_spread_as_two_slots() {
    let b = Binding::from_json(FIXTURE).unwrap();
    assert_eq!(b.slot_at("b", 1), Some(Slot { page: 1, half: None }));
    assert_eq!(b.slot_at("b", 2), Some(Slot { page: 2, half: Some(Half::Right) }));
    assert_eq!(b.slot_at("b", 3), Some(Slot { page: 2, half: Some(Half::Left) }));
    assert_eq!(b.slot_at("b", 4), Some(Slot { page: 3, half: None }));
}

#[test]
fn slot_at_refuses_an_index_past_the_end_and_an_unknown_edition() {
    let b = Binding::from_json(FIXTURE).unwrap();
    assert_eq!(b.slot_count("b"), Some(9));
    assert_eq!(b.slot_at("b", 9), None);
    assert_eq!(b.slot_at("nope", 0), None);
}

/// The two are inverses over every slot of every edition. This is the
/// property the reader depends on: a swap converts index → pair → resolve →
/// pair → index, and a mismatch anywhere silently moves the reader.
#[test]
fn index_of_inverts_slot_at_over_every_slot() {
    let b = Binding::from_json(FIXTURE).unwrap();
    for edition in ["a", "b"] {
        let count = b.slot_count(edition).unwrap();
        for index in 0..count {
            let slot = b.slot_at(edition, index).unwrap();
            assert_eq!(b.index_of(edition, slot), Some(index), "{edition} slot {index}");
        }
    }
}

#[test]
fn index_of_refuses_a_slot_the_edition_does_not_present() {
    let b = Binding::from_json(FIXTURE).unwrap();
    // `a` has no spreads, so it presents no halved slot at all.
    assert_eq!(b.index_of("a", Slot { page: 0, half: Some(Half::Right) }), None);
    assert_eq!(b.index_of("b", Slot { page: 99, half: None }), None);
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p taiyaku-wasm slot_at index_of
```

Expected: FAIL, `no method named 'slot_at' found`.

- [ ] **Step 3: Implement both, in `binding.rs`**

Add to the `impl Binding` block, after `slot_count`:

```rust
    /// The `(page, half)` pair at a flat slot index, or `None` for an unknown
    /// edition or an index past the end.
    ///
    /// A reader's position is a flat index — that is what `progress` stores —
    /// and `resolve` takes a pair, so something has to convert. It lives here
    /// rather than in TypeScript because the slot sequence is normative and a
    /// second derivation of it is the defect `taiyaku-read` was extracted to
    /// prevent, one language further out.
    pub fn slot_at(&self, edition: &str, index: u32) -> Option<Slot> {
        let reading = self.sidecar.work.reading;
        self.sidecar
            .editions
            .get(edition)?
            .slots(reading)
            .get(index as usize)
            .copied()
    }

    /// The flat index of a `(page, half)` pair, or `None` where this edition
    /// presents no such slot.
    ///
    /// A linear scan: the longest edition measured is 202 slots, and a map
    /// would have to be built per call or cached against a mutation this type
    /// does not have.
    pub fn index_of(&self, edition: &str, slot: Slot) -> Option<u32> {
        let reading = self.sidecar.work.reading;
        self.sidecar
            .editions
            .get(edition)?
            .slots(reading)
            .iter()
            .position(|candidate| *candidate == slot)
            .map(|found| found as u32)
    }
```

- [ ] **Step 4: Run them and watch them pass**

```bash
cargo test -p taiyaku-wasm slot_at index_of
```

Expected: PASS, 4 tests.

- [ ] **Step 5: Add the `wasm_bindgen` shells in `lib.rs`**

After the `WasmSlot` declaration:

```rust
/// Where a slot sits inside one edition: a page, and which half of it.
///
/// Distinct from `WasmSlot`, which additionally carries the `via` of a
/// resolution. A position is not the answer to a resolution and should not
/// claim a path it never walked.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct WasmPosition {
    pub page: u32,
    pub half: Option<String>,
}
```

And inside `#[wasm_bindgen] impl WasmSidecar`:

```rust
    /// The page and half at a flat slot index, or `undefined`.
    #[wasm_bindgen(js_name = slotAt)]
    pub fn slot_at(&self, edition: &str, index: u32) -> Option<WasmPosition> {
        self.inner.slot_at(edition, index).map(|slot| WasmPosition {
            page: slot.page,
            half: half_to_str(slot.half).map(str::to_string),
        })
    }

    /// The flat slot index of a page and half, or `undefined`.
    #[wasm_bindgen(js_name = indexOf)]
    pub fn index_of(
        &self,
        edition: &str,
        page: u32,
        half: Option<String>,
    ) -> Result<Option<u32>, JsError> {
        let half = half_from_str(half.as_deref()).map_err(|e| JsError::new(&e))?;
        Ok(self.inner.index_of(edition, taiyaku_core::Slot { page, half }))
    }
```

- [ ] **Step 6: Add `half_to_str` if `lib.rs` does not already have one**

`half_from_str` exists for the inbound direction. Check for its outbound partner first:

```bash
grep -n "half_to_str\|fn half_from_str" crates/taiyaku-wasm/src/lib.rs
```

If `half_to_str` is absent, add it beside `half_from_str`:

```rust
/// The wire spelling of a half. `None` is a whole page.
///
/// Spelled out rather than derived from `Debug`, for the reason `via_word`
/// gives: a `Debug` rendering is a formatting detail nothing promises to keep,
/// and this one crosses a boundary.
fn half_to_str(half: Option<Half>) -> Option<&'static str> {
    match half {
        Some(Half::Left) => Some("L"),
        Some(Half::Right) => Some("R"),
        None => None,
    }
}
```

If `resolve_slot` already builds its `WasmSlot.half` inline, replace that inline match with a call to `half_to_str` so there is one spelling of `"L"` and `"R"` in the crate.

- [ ] **Step 7: Build natively and for wasm32, and lint**

```bash
cargo test -p taiyaku-wasm
cargo build -p taiyaku-wasm --target wasm32-unknown-unknown
cargo clippy -p taiyaku-wasm --all-targets -- -D warnings
cargo fmt --check
```

Expected: all four succeed. The wasm32 build is the one that matters here — a signature that cannot cross the JS boundary compiles natively and fails only there.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-wasm/src/binding.rs crates/taiyaku-wasm/src/lib.rs
git commit -m "Convert a flat slot index to the pair the binding resolves on

A reader's position is a flat slot index, because that is what progress
stores and what the authority model tracks. resolve takes (page, half).
Nothing exposed the conversion, so the reader as specced could not call
the binding at all.

slot_at and index_of are inverses over every slot of every edition, which
is the property a swap depends on: index to pair, resolve, pair to index.
A mismatch anywhere moves the reader silently, which is the failure the
round-trip test exists to catch."
```

---

### Task 2: Serve a thumbnail per page

The filmstrip needs one thumbnail per page of the current edition. `cover.rs` already generates, caches and rate-limits exactly this operation for one page per edition; the difference is the key and the size. The earlier design left thumbnail caching open "until M5 knows its access pattern" — it now does, and this task also produces the number O11 asks for.

**Files:**
- Modify: `crates/taiyaku-serve/src/cover.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs`

**Interfaces:**
- Consumes: `cover::thumbnail`, `cover::cached_or_generated`, `cover::unique_temp_path`, `ServeState::page`, `taiyaku_core::Edition::locate(index) -> Option<(usize, u32)>` — all exist.
- Produces: `cover::thumb_cache_path(dir, sha256, offset) -> PathBuf`; `cover::cached_or_generated_thumb(dir, sha256, offset, page) -> Result<Vec<u8>, String>`; route `GET /api/works/{work}/{edition}/pages/{index}/thumb`.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-serve/src/cover.rs`, inside `#[cfg(test)] mod tests`:

```rust
/// A filmstrip tile is smaller than a cover, and the two never collide in
/// the cache even for page 0 of the same member — which is the one page
/// that is both.
#[test]
fn a_thumb_is_smaller_than_a_cover_and_keyed_apart() {
    let dir = std::path::Path::new("/tmp/does-not-matter");
    assert_ne!(cache_path(dir, "abc"), thumb_cache_path(dir, "abc", 0));

    let page = real_page_bytes();
    let cover = thumbnail(&page).unwrap();
    let tile = thumbnail_at(&page, THUMB_LONG_EDGE).unwrap();
    assert!(
        tile.len() < cover.len(),
        "tile {} should be smaller than cover {}",
        tile.len(),
        cover.len()
    );
}

/// Two pages of one member are two tiles. Keying on the member alone —
/// which is what a cover does, because a member has one cover — would serve
/// page 0's tile for every page in the volume.
#[test]
fn each_page_of_a_member_gets_its_own_tile() {
    let dir = std::path::Path::new("/tmp/does-not-matter");
    assert_ne!(
        thumb_cache_path(dir, "abc", 0),
        thumb_cache_path(dir, "abc", 1)
    );
}

/// A second request for the same tile reads the cache rather than decoding
/// again. Pinned by counting generations, not by timing.
#[test]
fn a_cached_tile_is_not_regenerated() {
    let dir = tempdir_for_test();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let page = || {
        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(real_page_bytes())
    };
    let first = cached_or_generated_thumb(&dir, "abc", 7, page).unwrap();
    let second = cached_or_generated_thumb(&dir, "abc", 7, || {
        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(real_page_bytes())
    })
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
}
```

`tempdir_for_test` — check whether this module already has such a helper before adding one:

```bash
grep -rn "fn tempdir_for_test\|std::env::temp_dir" crates/taiyaku-serve/src/
```

If none exists, add it beside `real_page_bytes`, using the same process-id-plus-counter idea `unique_temp_path` uses rather than a new dependency:

```rust
    /// A directory unique to this test, so tests sharing a name for their
    /// cache key cannot see each other's files. No `tempfile` dependency:
    /// the workspace's dependency set is deliberately short.
    fn tempdir_for_test() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("taiyaku-thumb-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p taiyaku-serve --lib cover
```

Expected: FAIL, `cannot find function 'thumb_cache_path'`.

- [ ] **Step 3: Parameterise generation by long edge, and add the thumb key**

In `cover.rs`, beside `LONG_EDGE`:

```rust
/// The long edge of a filmstrip tile, in pixels.
///
/// A cover is 512 because it fills a grid cell; a tile sits in a strip along
/// the bottom edge and there are ~195 of them per volume, so it is sized for
/// the strip rather than for a grid. Folded into the cache path exactly as
/// `LONG_EDGE` is, so changing it invalidates by construction.
const THUMB_LONG_EDGE: u32 = 160;
```

Refactor `thumbnail` so the size is an argument, keeping the existing name as the cover-sized call so no existing caller changes:

```rust
/// Shrink a page into a cover.
pub fn thumbnail(page: &[u8]) -> Result<Vec<u8>, String> {
    thumbnail_at(page, LONG_EDGE)
}

/// `thumbnail`, with the long edge as an argument.
///
/// Split out for the filmstrip, which wants the same decode limits, the same
/// always-JPEG rule and a different size. One decoder rather than two, so a
/// change to the limits cannot reach covers and miss tiles.
pub fn thumbnail_at(page: &[u8], long_edge: u32) -> Result<Vec<u8>, String> {
    // …the existing body of `thumbnail`, with `LONG_EDGE` replaced by
    // `long_edge` in the `decoded.thumbnail(…)` call, and the two error
    // strings left exactly as they are.
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(page))
        .with_guessed_format()
        .map_err(|e| format!("cover: undecodable page: {e}"))?;
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("cover: undecodable page: {e}"))?;
    let scaled = decoded.thumbnail(long_edge, long_edge);
    let mut out = std::io::Cursor::new(Vec::new());
    scaled
        .into_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| format!("cover: encoding: {e}"))?;
    Ok(out.into_inner())
}

/// Where a filmstrip tile for one page of one member is cached.
///
/// Keyed on the member's `sha256` *and the page's offset within it*, because
/// a member has one cover and 195 tiles. The offset rather than the
/// edition-flat index, so a tile survives the edition gaining a member ahead
/// of this one.
pub fn thumb_cache_path(dir: &Path, sha256: &str, offset: u32) -> PathBuf {
    dir.join(format!(
        "{sha256}-p{offset}-{THUMB_LONG_EDGE}v{FORMAT_VERSION}.jpg"
    ))
}
```

- [ ] **Step 4: Add the cached-or-generated variant**

Refactor `cached_or_generated` to delegate, so there is one copy of the semaphore, temp-file and rename logic:

```rust
/// Read a cached cover, or make one and cache it.
pub fn cached_or_generated(
    dir: &Path,
    sha256: &str,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(cache_path(dir, sha256), dir, LONG_EDGE, page)
}

/// Read a cached filmstrip tile, or make one and cache it.
pub fn cached_or_generated_thumb(
    dir: &Path,
    sha256: &str,
    offset: u32,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(thumb_cache_path(dir, sha256, offset), dir, THUMB_LONG_EDGE, page)
}

/// The shared body: cache hit outside the permit, generation inside it,
/// atomic publish via a per-attempt temp name.
///
/// Both callers get the same concurrency bound, which matters more for tiles
/// than for covers: a cold filmstrip asks for every page of a volume, and
/// unbounded decodes at `MAX_DECODE_ALLOC` each is what
/// `MAX_CONCURRENT_GENERATIONS` exists to prevent.
fn at_path(
    path: PathBuf,
    dir: &Path,
    long_edge: u32,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    if let Ok(found) = std::fs::read(&path) {
        return Ok(found);
    }
    let generated = {
        let _permit = GENERATION_LIMIT.acquire();
        thumbnail_at(&page()?, long_edge)?
    };
    if std::fs::create_dir_all(dir).is_ok() {
        let temporary = unique_temp_path(&path);
        if std::fs::write(&temporary, &generated).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
        let _ = std::fs::remove_file(&temporary);
    }
    Ok(generated)
}
```

- [ ] **Step 5: Run the cover tests and watch everything pass**

```bash
cargo test -p taiyaku-serve --lib cover
```

Expected: PASS, including every pre-existing cover test. If a pre-existing test fails, the refactor changed behaviour and the refactor is wrong — do not adjust the test.

- [ ] **Step 6: Write the failing route test**

In `crates/taiyaku-serve/src/routes.rs`'s test module, following whatever `oneshot` helper the existing route tests use — read one first:

```bash
grep -n "oneshot" crates/taiyaku-serve/src/routes.rs | head -5
```

```rust
#[tokio::test]
async fn a_thumb_route_answers_a_jpeg_and_404s_a_page_that_does_not_exist() {
    let state = crate::test_support::state_with_real_pages();
    let app = build_router(state);

    let ok = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/works/w/a/pages/0/thumb")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), axum::http::StatusCode::OK);
    assert_eq!(ok.headers()[axum::http::header::CONTENT_TYPE], "image/jpeg");

    let missing = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/works/w/a/pages/9999/thumb")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
}
```

`state_with_real_pages` may not exist. The shared `test_support::work` fixture's pages are ASCII strings and are deliberately undecodable — `cover.rs`'s own tests build real images for this reason. Build the state this test needs the same way, reusing `cover::tests::real_page_bytes`'s approach rather than weakening the shared fixture.

- [ ] **Step 7: Run it and watch it fail**

```bash
cargo test -p taiyaku-serve --lib thumb_route
```

Expected: FAIL with 404 on the first request — the route does not exist, so the router falls through.

- [ ] **Step 8: Add the handler and the route**

In `routes.rs`, beside `get_page`:

```rust
/// A filmstrip tile for one page.
///
/// Separate from `get_page` rather than a query parameter on it, because the
/// two have different cache lifetimes and different content types: a page is
/// whatever the archive holds, a tile is always JPEG.
async fn get_thumb(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition, raw)): AxPath<(String, String, String)>,
) -> Response {
    let index = match page_index(&raw, &edition) {
        Ok(index) => index,
        Err(refusal) => return refusal.into_response(),
    };
    // Decoding blocks and can allocate up to `MAX_DECODE_ALLOC`; `get_page`'s
    // reasoning applies unchanged.
    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id)?;
        let ed = found
            .sidecar
            .editions
            .get(&edition)
            .ok_or_else(|| format!("no edition `{edition}` in work `{id}`"))?;
        let (member, offset) = ed
            .locate(index)
            .ok_or_else(|| format!("`{edition}` has no page {index} in work `{id}`"))?;
        let sha = ed.members[member].sha256.clone();
        let dir = state.thumb_cache_dir();
        crate::cover::cached_or_generated_thumb(&dir, &sha, offset, || {
            state.page(found, &edition, index).map(|p| p.bytes)
        })
    })
    .await;

    match fetched {
        Ok(Ok(bytes)) => ([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response(),
        Ok(Err(e)) => not_found(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("thumbnail read panicked: {e}"),
        )
            .into_response(),
    }
}
```

Register it in `build_router`, beside the page route:

```rust
        .route(
            "/api/works/{work}/{edition}/pages/{index}/thumb",
            get(get_thumb),
        )
```

`ServeState::thumb_cache_dir` will not exist. Find how the OPDS cover handler obtains its cache directory and follow it exactly — the tiles go in a **sibling** directory, not the same one:

```bash
grep -n "covers\|cache_dir" crates/taiyaku-serve/src/opds.rs crates/taiyaku-serve/src/lib.rs
```

Add `thumb_cache_dir` next to whatever that is, returning `.taiyaku-cache/thumbs`. A sibling rather than the same directory because O11 records that two caches already share that tree and that only one of them is cheap to rebuild; a third sharing it would make "clear the cache" even blunter.

- [ ] **Step 9: Run the route test and watch it pass**

```bash
cargo test -p taiyaku-serve
cargo clippy -p taiyaku-serve --all-targets -- -D warnings
```

Expected: PASS, whole crate green.

- [ ] **Step 10: Measure the cache, because O11 asked for a number**

Against a real volume, with the corpus available:

```bash
cargo run --release -- serve /path/to/taiyaku.json --addr 127.0.0.1:8080 &
for i in $(seq 0 194); do curl -s -o /dev/null "http://127.0.0.1:8080/api/works/<work>/<edition>/pages/$i/thumb"; done
du -sh .taiyaku-cache/thumbs && ls .taiyaku-cache/thumbs | wc -l
```

Record the file count and total size in `docs/notes.md` under a new heading, and update O11 in `docs/status.md` with the figure — the covers half is recorded as 165 KB across three files, and this is the second consumer O11 names as its reopening condition. **Report the number even if it is unremarkable.** A measurement that came out boring is the evidence the item can stay closed.

- [ ] **Step 11: Commit**

```bash
git add crates/taiyaku-serve/src/cover.rs crates/taiyaku-serve/src/routes.rs docs/notes.md docs/status.md
git commit -m "Serve a filmstrip tile per page, and measure what caching them costs

cover.rs already generated, cached, rate-limited and atomically published
exactly this operation for one page per edition. What differs is the key —
member sha256 plus offset within the member, because a member has one cover
and 195 tiles — and the size.

Generation is parameterised by long edge rather than copied, so the decode
limits and the always-JPEG rule cannot change for covers and miss tiles.
Both paths share one semaphore, which matters more for tiles: a cold strip
asks for every page of a volume at once.

Tiles live in .taiyaku-cache/thumbs rather than beside the covers. O11
records that two caches already share that tree and only one is cheap to
rebuild; a third sharing it would make clearing it blunter still."
```

---

### Task 3: Emit a TypeScript package, and run the module for the first time

M4's plan states plainly that emitting the TS package was **not** in it, "because nothing consumes it until `web/` exists in M5", and that nothing has ever executed the module under a wasm runtime — the agreement check compares the binding against the core natively, which catches conversion drift and not codegen. `web/` now exists in the next task, so both come due here, and the execution comes first because a codegen failure discovered from React is a much worse place to find it.

**Files:**
- Create: `crates/taiyaku-wasm/README.md`
- Create: `scripts/build-wasm.sh`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: the `wasm_bindgen` surface from Task 1 and M4 — `fromJson`, `editions`, `slotCount`, `resolveSlot`, `slotAt`, `indexOf`.
- Produces: `crates/taiyaku-wasm/pkg/` — an npm package named `taiyaku-wasm` with `.d.ts` files, consumed by `web/` in Task 4 as a `file:` dependency.

- [ ] **Step 1: Confirm `wasm-pack` is present**

```bash
wasm-pack --version || cargo install wasm-pack --locked
```

Expected: a version prints. Measured present at 0.15.0 on this machine during planning, so the install branch should not fire; it is a several-minute compile when it does, which is why the CI job in Task 4 does not build wasm.

- [ ] **Step 2: Write the build script**

`scripts/build-wasm.sh`:

```bash
#!/usr/bin/env bash
# Build the browser binding as an npm package.
#
# `--target web` emits an ES module with an explicit `init()`, which is what
# Vite wants; `--target bundler` hides the instantiation and makes the wasm a
# bare import that Vite's dep optimiser then has to be told about. One
# explicit await at startup is cheaper to reason about than that.
#
# The output is not committed: it is a build artefact of a crate in this
# repository, and a checked-in binary would be the second source of truth for
# what the binding does.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT:?}/crates/taiyaku-wasm/pkg"

rm -rf "${OUT:?}"
wasm-pack build "${ROOT:?}/crates/taiyaku-wasm" \
  --target web \
  --out-dir pkg \
  --out-name taiyaku_wasm

echo "built: ${OUT}"
ls -la "${OUT}"
```

```bash
chmod +x scripts/build-wasm.sh
```

- [ ] **Step 3: Build, and read the generated types**

```bash
./scripts/build-wasm.sh
cat crates/taiyaku-wasm/pkg/taiyaku_wasm.d.ts
```

Expected: the `.d.ts` declares `WasmSidecar` with `fromJson`, `editions`, `slotCount`, `resolveSlot`, `slotAt`, `indexOf`, and declares `WasmSlot` and `WasmPosition`.

**Read this file rather than assuming its contents.** Every TypeScript signature in Tasks 5 to 11 must match what is actually here — `Option<T>` may surface as `T | undefined`, and `Vec<String>` as `string[]`, but confirm rather than infer. If a method is missing from the `.d.ts`, its `#[wasm_bindgen]` attribute is wrong, and that is a Task 1 defect to fix now.

- [ ] **Step 4: Write the first execution of the module, under Node**

`crates/taiyaku-wasm/pkg` is generated, so the smoke test lives beside the script rather than in it. Create `scripts/smoke-wasm.mjs`:

```javascript
// The first execution of this module under any JS runtime.
//
// M4 gated that the crate *builds* for wasm32 and tested its logic natively,
// and recorded that this leaves codegen unproven. This closes that: it loads
// the real .wasm, parses the real fixture, and checks the two properties the
// reader depends on before a single line of React exists.
import { readFile } from 'node:fs/promises'
import init, { WasmSidecar } from '../crates/taiyaku-wasm/pkg/taiyaku_wasm.js'

const wasm = await readFile(
  new URL('../crates/taiyaku-wasm/pkg/taiyaku_wasm_bg.wasm', import.meta.url),
)
await init({ module_or_path: wasm })

// The same document as `binding.rs`'s FIXTURE. `b`'s page 2 is a joined
// spread, so `b` presents nine slots for eight pages.
const FIXTURE = JSON.stringify({
  taiyaku: 1,
  work: { title: 'T', reading: 'rtl' },
  editions: {
    a: { label: 'A', members: [{ path: 'a.cbz', sha256: '00', pages: 8 }] },
    b: { label: 'B', members: [{ path: 'b.cbz', sha256: '01', pages: 8 }], spreads: [2] },
  },
  hub: 'a',
  alignments: [
    {
      from: 'a', to: 'b', by: 'solver',
      runs: [{ a: '1-3', b: '0-2R' }, { a: '5-7', b: '5-7' }],
    },
  ],
})

const sc = WasmSidecar.fromJson(FIXTURE)

const check = (label, actual, expected) => {
  const a = JSON.stringify(actual)
  const e = JSON.stringify(expected)
  if (a !== e) throw new Error(`${label}: got ${a}, want ${e}`)
  console.log(`ok  ${label}`)
}

check('editions', [...sc.editions()].sort(), ['a', 'b'])
check('slotCount a', sc.slotCount('a'), 8)
check('slotCount b', sc.slotCount('b'), 9)

const halved = sc.slotAt('b', 2)
check('slotAt b 2', { page: halved.page, half: halved.half }, { page: 2, half: 'R' })
check('indexOf b (2,R)', sc.indexOf('b', 2, 'R'), 2)

const landed = sc.resolveSlot('a', 'b', 1, undefined)
check('resolveSlot a1 to b', { page: landed.page, half: landed.half }, { page: 0, half: undefined })

console.log('\nthe module runs.')
```

- [ ] **Step 5: Run it**

```bash
node scripts/smoke-wasm.mjs
```

Expected: six `ok` lines and `the module runs.`

**If any line fails, stop and fix the binding.** This is the first evidence that the compiled artefact behaves as the native tests say, and every task after this one assumes it does. A mismatch here is a real defect, not a test to adjust — in particular, if `slotAt('b', 2)` returns `half: undefined` rather than `'R'`, `half_to_str` is wrong or is not being called.

- [ ] **Step 6: Ignore the build output and document the package**

Append to `.gitignore`:

```
# Generated by scripts/build-wasm.sh from crates/taiyaku-wasm. A build
# artefact of a crate in this repository; committing it would be a second
# source of truth for what the binding does.
crates/taiyaku-wasm/pkg/
```

`crates/taiyaku-wasm/README.md`:

```markdown
# taiyaku-wasm

The binding a browser reader loads: parse a sidecar, convert between a flat
slot index and a `(page, half)` pair, resolve a slot between two editions, and
say which path the answer walked.

## Building

```sh
./scripts/build-wasm.sh          # emits crates/taiyaku-wasm/pkg/
node scripts/smoke-wasm.mjs      # runs the module and checks six properties
```

`pkg/` is generated and not committed. `web/` depends on it as `file:` and so
must be built before `npm install` in `web/`.

## Why the logic is in `binding.rs`

`lib.rs` is a `wasm_bindgen` shell that converts types and decides nothing;
`binding.rs` is plain Rust and is what the tests exercise, so the behaviour
under test is the behaviour that ships. `scripts/smoke-wasm.mjs` covers the
part that arrangement cannot reach — that the compiled artefact behaves as the
native build does.
```

- [ ] **Step 7: Commit**

```bash
git add scripts/build-wasm.sh scripts/smoke-wasm.mjs crates/taiyaku-wasm/README.md .gitignore
git commit -m "Emit a TypeScript package, and run the binding for the first time

M4 gated that the crate builds for wasm32 and tested its logic natively, and
said plainly that this leaves codegen unproven and that no TS package was
emitted because nothing consumed one. web/ arrives next, so both come due.

The execution comes first deliberately. A codegen failure found from React
is a much worse place to find it than a six-line node script, and until this
ran, nothing anywhere had loaded the compiled artefact."
```

---

### Task 4: Scaffold `web/`, and gate it in CI

**Files:**
- Create: `web/package.json`, `web/tsconfig.json`, `web/vite.config.ts`, `web/index.html`, `web/src/main.tsx`, `web/src/App.tsx`
- Modify: `.forgejo/workflows/ci.yml`, `.gitignore`, `docs/ci.md`

**Interfaces:**
- Consumes: `crates/taiyaku-wasm/pkg` from Task 3.
- Produces: `npm --prefix web run test`, `npm --prefix web run build`, and a `web` CI job that needs neither Rust nor WASM.

- [ ] **Step 1: Scaffold and install**

```bash
cd /home/davey/projects/taiyaku
npm create vite@latest web -- --template react-ts
npm --prefix web install
npm --prefix web install -D vitest
npm --prefix web install ../crates/taiyaku-wasm/pkg
```

The last line adds the generated package as a `file:` dependency. It requires Task 3 to have been run.

- [ ] **Step 2: Add the test script and a dev proxy**

In `web/package.json`, add to `scripts`:

```json
    "test": "vitest run",
    "test:watch": "vitest"
```

`web/vite.config.ts`:

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The reader is served from Vite in development and talks to `taiyaku serve`
// on another port. Proxying rather than enabling CORS on the server: CORS
// would be a production-facing change made for a development convenience, and
// `serve` has no browser-specific assumptions today — a constraint the design
// keeps deliberately, so a native shell can use the same API.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': { target: process.env.TAIYAKU_SERVE ?? 'http://127.0.0.1:8080', changeOrigin: true },
      '/opds': { target: process.env.TAIYAKU_SERVE ?? 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
})
```

- [ ] **Step 3: Verify the toolchain runs before writing any application code**

```bash
npm --prefix web run build
npm --prefix web run test
```

Expected: the build succeeds. `vitest run` reports **no test files found** and exits non-zero — that is correct at this point and Task 5 fixes it. Confirm the message is "No test files found" rather than a configuration error.

- [ ] **Step 4: Ignore the web build output**

Append to `.gitignore`:

```
# ---- Web reader --------------------------------------------------------------
web/node_modules/
web/dist/
```

- [ ] **Step 5: Add the CI job**

Read the existing workflow first so the new job matches its clone-and-container shape exactly:

```bash
cat .forgejo/workflows/ci.yml
```

Add a fourth job, following the existing jobs' `container:`, `runs-on: docker` and authenticated-clone `run:` steps verbatim, and changing only the image and the final steps:

```yaml
  web:
    runs-on: docker
    container:
      image: node:22-bookworm
    steps:
      # …the same authenticated clone step the other three jobs use, copied
      # verbatim. It derives the URL from FORGEJO_SERVER_URL and FORGEJO_TOKEN.
      - name: Install
        run: npm --prefix web ci
      - name: Test
        run: npm --prefix web run test
```

**A node image, not a dual-toolchain one, and that is the point.** `docs/ci.md` measures `cimg/rust:1.97-node` and `catthehacker/ubuntu:rust-latest` as the images carrying both, at 0.77 GB and 1.05 GB, and notes the probe run spent five minutes pulling them. This job needs neither: everything it tests is pure TypeScript, because `reading.ts` and `pairing.ts` sit behind the `Resolver` interface and import no WASM. Building the binding in CI would mean a dual-toolchain image plus a `cargo install wasm-pack` compile, for a smoke test that is already a one-line local command.

**What that leaves ungated, stated rather than glossed:** CI never builds the wasm package and never runs `scripts/smoke-wasm.mjs`, so a change to `binding.rs` that breaks codegen goes green. `cargo test -p taiyaku-wasm` still catches conversion drift, which is the larger risk; the codegen risk is carried by running the smoke script locally after any change to `lib.rs`. Revisit if that discipline slips.

`npm ci` needs a lockfile — confirm `web/package-lock.json` exists and is committed.

**The `file:` dependency on `crates/taiyaku-wasm/pkg` will break `npm ci` in CI, because `pkg/` is gitignored.** Handle it by keeping the WASM import out of everything CI runs: `resolver.ts` is the only file that imports the package, no test imports `resolver.ts`, and `vitest` never loads it. If `npm ci` still fails resolving the `file:` dependency, move it from `dependencies` to `optionalDependencies` and confirm `npm ci --omit=optional` succeeds — record whichever was needed in `docs/ci.md`.

- [ ] **Step 6: Document the job**

In `docs/ci.md`, add to the job table:

```markdown
| `web` | `npm ci`, `vitest run` in `web/` |
```

And a short section below it recording the image choice, the reasoning above, and what is left ungated.

- [ ] **Step 7: Commit**

```bash
git add web .forgejo/workflows/ci.yml .gitignore docs/ci.md
git commit -m "Scaffold web/ and gate it on a node image, not a dual-toolchain one

Everything CI runs here is pure TypeScript: the reading model sits behind a
Resolver interface and imports no WASM, so the job needs neither Rust nor
wasm-pack. docs/ci.md measures the dual-toolchain images at 0.77 and 1.05 GB
with a five-minute cold pull, which this avoids paying every run.

What that leaves ungated is recorded rather than glossed: CI never builds the
binding, so a codegen break goes green there. cargo test -p taiyaku-wasm
still catches conversion drift, and scripts/smoke-wasm.mjs carries the rest
as a local step."
```

---

### Task 5: The `Resolver` interface and the authority state machine

The heart of the reader, and the reason CI is cheap: every rule here is a pure function over plain data.

**Files:**
- Create: `web/src/resolver.ts`, `web/src/reading.ts`, `web/src/fixtures.ts`, `web/src/reading.test.ts`

**Interfaces:**
- Consumes: `WasmSidecar` from Task 3, exactly as its generated `.d.ts` declares it.
- Produces: `Resolver`, `Position`, `ReadingState`, `swap(state, edition)`, `navigate(state, slot)`, `display(state, resolver)`.

- [ ] **Step 1: Write the interface and fixtures**

`web/src/resolver.ts`:

```typescript
/** Where a slot sits inside one edition. */
export interface Position {
  page: number
  half?: 'L' | 'R'
}

/** Where a slot landed, and which path the answer walked. */
export interface Landed extends Position {
  via: 'identity' | 'direct' | 'inverted' | 'hub'
}

/**
 * Everything the reading model needs from a sidecar.
 *
 * An interface rather than the WASM type directly, for two reasons. It lets
 * every rule in `reading.ts` and `pairing.ts` be tested with a hand-written
 * fake — no WASM, no async init, no CI toolchain. And it is the seam a native
 * shell substitutes at, which is the same reason `PageSource` is a trait.
 */
export interface Resolver {
  editions(): string[]
  slotCount(edition: string): number | undefined
  slotAt(edition: string, index: number): Position | undefined
  indexOf(edition: string, at: Position): number | undefined
  resolve(from: string, to: string, at: Position): Landed | undefined
}
```

The WASM-backed implementation goes in the same file. Write it against the real `.d.ts` read in Task 3, Step 3 — adjust the calls below if the generated signatures differ:

```typescript
import init, { WasmSidecar } from 'taiyaku-wasm'

/** Parse a sidecar into a `Resolver`. Must be awaited once per page load. */
export async function wasmResolver(json: string): Promise<Resolver> {
  await init()
  const sc = WasmSidecar.fromJson(json)
  return {
    editions: () => [...sc.editions()],
    slotCount: (edition) => sc.slotCount(edition),
    slotAt: (edition, index) => {
      const p = sc.slotAt(edition, index)
      return p && { page: p.page, half: p.half as 'L' | 'R' | undefined }
    },
    indexOf: (edition, at) => sc.indexOf(edition, at.page, at.half),
    resolve: (from, to, at) => {
      const r = sc.resolveSlot(from, to, at.page, at.half)
      return r && { page: r.page, half: r.half as 'L' | 'R' | undefined, via: r.via as Landed['via'] }
    },
  }
}
```

`web/src/fixtures.ts`:

```typescript
import type { Position, Landed, Resolver } from './resolver'

/**
 * A fake resolver over an explicit slot table.
 *
 * `slots` gives each edition's slot sequence in order; `pairs` maps a
 * `from|to|index` key to the counterpart index, and any key absent from it is
 * a gap. Explicit rather than derived, so a test states the alignment it means
 * instead of depending on run arithmetic that is already tested in Rust.
 */
export function fakeResolver(
  slots: Record<string, Position[]>,
  pairs: Record<string, number>,
): Resolver {
  const key = (from: string, to: string, index: number) => `${from}|${to}|${index}`
  return {
    editions: () => Object.keys(slots),
    slotCount: (edition) => slots[edition]?.length,
    slotAt: (edition, index) => slots[edition]?.[index],
    indexOf: (edition, at) => {
      const found = slots[edition]?.findIndex((s) => s.page === at.page && s.half === at.half)
      return found === undefined || found < 0 ? undefined : found
    },
    resolve: (from, to, at) => {
      const index = slots[from]?.findIndex((s) => s.page === at.page && s.half === at.half)
      if (index === undefined || index < 0) return undefined
      if (from === to) return { ...at, via: 'identity' }
      const landedIndex = pairs[key(from, to, index)]
      if (landedIndex === undefined) return undefined
      const landed = slots[to]?.[landedIndex]
      return landed && { ...landed, via: 'direct' }
    },
  }
}

/** Two editions; `b`'s page 2 is a joined spread, so `b` has 9 slots for 8 pages. */
export const whole = (n: number): Position[] =>
  Array.from({ length: n }, (_, page) => ({ page }))

export const bSlots: Position[] = [
  { page: 0 }, { page: 1 },
  { page: 2, half: 'R' }, { page: 2, half: 'L' },
  { page: 3 }, { page: 4 }, { page: 5 }, { page: 6 }, { page: 7 },
]
```

`findIndex` returns `-1` for "not found" and `slots[edition]` may be `undefined`, so both cases collapse to `undefined` above rather than leaking a `-1` that would index the slot table from the end.

- [ ] **Step 2: Write the failing state-machine tests**

`web/src/reading.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { fakeResolver, whole, bSlots } from './fixtures'
import { display, navigate, swap, type ReadingState } from './reading'

// `a` slots 1,2,3 pair with `b` slots 0,1,2. `a` slot 0 and 4..7 pair with
// nothing, so the gap case is in range.
const r = fakeResolver(
  { a: whole(8), b: bSlots },
  { 'a|b|1': 0, 'a|b|2': 1, 'a|b|3': 2, 'b|a|0': 1, 'b|a|1': 2, 'b|a|2': 3 },
)

const at = (slot: number): ReadingState => ({ authority: 'a', slot, viewing: 'a' })

describe('display', () => {
  it('shows the authority slot itself when viewing the authority', () => {
    expect(display(at(2), r)).toEqual({ kind: 'at', edition: 'a', slot: 2, via: 'identity' })
  })

  it('resolves into the viewed edition when they differ', () => {
    expect(display(swap(at(2), 'b'), r)).toEqual({
      kind: 'at', edition: 'b', slot: 1, via: 'direct',
    })
  })

  it('reports a gap rather than moving to the nearest slot', () => {
    expect(display(swap(at(0), 'b'), r)).toEqual({
      kind: 'gap', edition: 'b', from: { edition: 'a', slot: 0 },
    })
  })
})

describe('swap', () => {
  it('never moves authority, so a swap cannot drift', () => {
    const start = at(2)
    const there = swap(start, 'b')
    expect(there.authority).toBe('a')
    expect(there.slot).toBe(2)
    expect(there.viewing).toBe('b')
  })

  it('returns exactly where it started, over every paired slot', () => {
    for (const slot of [1, 2, 3]) {
      expect(swap(swap(at(slot), 'b'), 'a')).toEqual(at(slot))
    }
  })

  it('returns exactly where it started even from a gap', () => {
    expect(swap(swap(at(0), 'b'), 'a')).toEqual(at(0))
  })
})

describe('navigate', () => {
  it('moves authority to the edition being viewed', () => {
    const there = navigate(swap(at(2), 'b'), 1)
    expect(there).toEqual({ authority: 'b', slot: 1, viewing: 'b' })
  })
})
```

- [ ] **Step 3: Run them and watch them fail**

```bash
npm --prefix web run test
```

Expected: FAIL, cannot resolve `./reading`.

- [ ] **Step 4: Implement the state machine**

`web/src/reading.ts`:

```typescript
import type { Landed, Resolver } from './resolver'

/**
 * One authority, not a per-edition cursor.
 *
 * `notes.md` records the trap: swap onto a page with no counterpart, land on
 * the nearest, swap back, and you have silently moved one page — compounding
 * on every swap. A swap here never writes position, so there is nothing to
 * drift, and round-tripping is lossless by construction rather than by care.
 */
export interface ReadingState {
  /** The edition the position is expressed in. */
  authority: string
  /** A flat slot index within `authority`. Never a page number. */
  slot: number
  /** The edition being shown. */
  viewing: string
}

export type Display =
  | { kind: 'at'; edition: string; slot: number; via: Landed['via'] }
  | { kind: 'gap'; edition: string; from: { edition: string; slot: number } }

/** Show a different edition. Authority is untouched — that is the whole rule. */
export function swap(state: ReadingState, to: string): ReadingState {
  return { ...state, viewing: to }
}

/** Real navigation: the edition being viewed becomes the authority. */
export function navigate(state: ReadingState, slot: number): ReadingState {
  return { authority: state.viewing, slot, viewing: state.viewing }
}

/**
 * Which slot of the viewed edition to draw, or that there is no such slot.
 *
 * A gap is a view, not a position: it names what was being looked for and
 * where from, and one press of the toggle returns exactly where you were.
 */
export function display(state: ReadingState, resolver: Resolver): Display {
  if (state.viewing === state.authority) {
    return { kind: 'at', edition: state.viewing, slot: state.slot, via: 'identity' }
  }
  const from = resolver.slotAt(state.authority, state.slot)
  const landed = from && resolver.resolve(state.authority, state.viewing, from)
  const slot = landed && resolver.indexOf(state.viewing, landed)
  if (landed === undefined || slot === undefined) {
    return {
      kind: 'gap',
      edition: state.viewing,
      from: { edition: state.authority, slot: state.slot },
    }
  }
  return { kind: 'at', edition: state.viewing, slot, via: landed.via }
}
```

- [ ] **Step 5: Run them and watch them pass**

```bash
npm --prefix web run test
```

Expected: PASS, 7 tests.

- [ ] **Step 6: Add the exhaustive round-trip test**

The spec names this as a seam test and asks for it exhaustive rather than sampled. Append to `reading.test.ts`:

```typescript
describe('the round-trip seam', () => {
  it('A to B to A returns to the starting slot, for every slot of every ordered pair', () => {
    for (const from of r.editions()) {
      for (const to of r.editions()) {
        const count = r.slotCount(from)!
        for (let slot = 0; slot < count; slot++) {
          const start: ReadingState = { authority: from, slot, viewing: from }
          const back = swap(swap(start, to), from)
          expect(back, `${from} slot ${slot} via ${to}`).toEqual(start)
        }
      }
    }
  })
})
```

**This test is nearly free and nearly tautological as written, and that is worth saying.** `swap` only sets `viewing`, so it cannot fail while `swap` stays one line. It earns its place as a regression guard the day someone "optimises" `swap` to resolve eagerly and write position — which is exactly the change `notes.md` warns about. Keep it, and do not strengthen it into a test of `display`; that is what the gap tests above already do.

- [ ] **Step 7: Run, then commit**

```bash
npm --prefix web run test
git add web/src/resolver.ts web/src/reading.ts web/src/fixtures.ts web/src/reading.test.ts
git commit -m "The reading model: one authority, and a Resolver it can be tested without

A swap sets viewing and nothing else, so round-tripping is lossless by
construction rather than by care — the notes.md trap where swapping onto a
gap, landing on the nearest and swapping back moves you one page, every time.

Resolver is an interface rather than the WASM type so every rule here is
provable with a hand-written fake: no WASM, no async init, and a CI job that
needs no Rust toolchain. It is also the seam a native shell substitutes at,
which is why PageSource is a trait for the same reason."
```

---

### Task 6: Render one page, fit whole

**Files:**
- Create: `web/src/api.ts`, `web/src/PageView.tsx`, `web/src/api.test.ts`

**Interfaces:**
- Consumes: the routes verified in Task 2 — `GET /api/works` returns `string[]`; `GET /api/works/{work}` returns the sidecar JSON verbatim; `GET /api/works/{work}/{edition}/pages/{n}` returns image bytes; `…/thumb` returns JPEG; `GET /api/works/{work}/progress` returns `204` or `{edition, slot}`.
- Produces: `listWorks()`, `getSidecar(work)`, `pageUrl(work, edition, index)`, `thumbUrl(work, edition, index)`, `getProgress(work)`, `putProgress(work, p)`; and `<PageView>`.

- [ ] **Step 1: Write the failing API tests**

`web/src/api.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { pageUrl, thumbUrl } from './api'

describe('url building', () => {
  it('addresses a page by flat index', () => {
    expect(pageUrl('go-toubun-v01', 'en-mono', 47))
      .toBe('/api/works/go-toubun-v01/en-mono/pages/47')
  })

  it('escapes an edition key that needs it', () => {
    // validate_key's alphabet permits `-`, and a key reaches the URL as a
    // path segment. Encoding rather than trusting the key is the cheap half
    // of not caring what the alphabet becomes later.
    expect(pageUrl('w', 'a b', 0)).toBe('/api/works/w/a%20b/pages/0')
  })

  it('appends /thumb for a tile', () => {
    expect(thumbUrl('w', 'raw', 3)).toBe('/api/works/w/raw/pages/3/thumb')
  })
})
```

- [ ] **Step 2: Run and watch fail**

```bash
npm --prefix web run test
```

Expected: FAIL, cannot resolve `./api`.

- [ ] **Step 3: Write the client**

`web/src/api.ts`:

```typescript
/**
 * The `taiyaku serve` API. Knows HTTP; knows nothing about reading.
 *
 * Page and thumbnail are URLs rather than fetches: both go straight into an
 * `<img src>`, so the browser's own cache and decoding handle them, and
 * fetching them into memory first would defeat both.
 */
const seg = (s: string) => encodeURIComponent(s)

export const pageUrl = (work: string, edition: string, index: number) =>
  `/api/works/${seg(work)}/${seg(edition)}/pages/${index}`

export const thumbUrl = (work: string, edition: string, index: number) =>
  `${pageUrl(work, edition, index)}/thumb`

export async function listWorks(): Promise<string[]> {
  const res = await fetch('/api/works')
  if (!res.ok) throw new Error(`listing works: ${res.status} ${await res.text()}`)
  return res.json()
}

/** The sidecar, verbatim, as text — `wasmResolver` parses it, not JSON.parse. */
export async function getSidecar(work: string): Promise<string> {
  const res = await fetch(`/api/works/${seg(work)}`)
  if (!res.ok) throw new Error(`loading ${work}: ${res.status} ${await res.text()}`)
  return res.text()
}

export interface StoredProgress {
  edition: string
  slot: number
}

/** `undefined` where nothing is recorded — the server answers 204, not 404. */
export async function getProgress(work: string): Promise<StoredProgress | undefined> {
  const res = await fetch(`/api/works/${seg(work)}/progress`)
  if (res.status === 204) return undefined
  if (!res.ok) throw new Error(`reading progress: ${res.status} ${await res.text()}`)
  return res.json()
}

export async function putProgress(work: string, progress: StoredProgress): Promise<void> {
  const res = await fetch(`/api/works/${seg(work)}/progress`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(progress),
  })
  if (!res.ok) throw new Error(`writing progress: ${res.status} ${await res.text()}`)
}
```

- [ ] **Step 4: Run and watch pass**

```bash
npm --prefix web run test
```

Expected: PASS, 10 tests.

- [ ] **Step 5: Write the page view**

`web/src/PageView.tsx`:

```tsx
import type { CSSProperties } from 'react'

/**
 * One page, fit whole.
 *
 * `object-fit: contain` with `max-width`/`max-height` at 100% and no minimum:
 * aspect preserved, downscaled to fit, never cropped, never distorted, never
 * force-filled. `image-rendering: auto` and no explicit width means a page
 * smaller than the viewport is drawn at its own size rather than blown up —
 * upscaling past 100% is a `fit` module setting in M5b and is off until then.
 *
 * The letterbox around the page is background, and it is where the annotation
 * margin tabs will live in M5c.
 */
const fitWhole: CSSProperties = {
  maxWidth: '100%',
  maxHeight: '100%',
  width: 'auto',
  height: 'auto',
  objectFit: 'contain',
  display: 'block',
}

export interface PageViewProps {
  src: string
  alt: string
}

export function PageView({ src, alt }: PageViewProps) {
  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        overflow: 'hidden',
      }}
    >
      <img src={src} alt={alt} style={fitWhole} draggable={false} />
    </div>
  )
}
```

- [ ] **Step 6: Wire it up and look at a real page**

Replace `web/src/App.tsx` with a minimum that loads the first work and shows slot 0 of the first edition. Then, with a real sidecar:

```bash
cargo run --release -- serve /path/to/taiyaku.json --addr 127.0.0.1:8080 &
npm --prefix web run dev
```

Open the printed URL. Expected: a page renders, whole, letterboxed, and resizing the window never crops it.

- [ ] **Step 7: Commit**

```bash
git add web/src/api.ts web/src/api.test.ts web/src/PageView.tsx web/src/App.tsx
git commit -m "Fetch from serve, and draw a page whole

Pages and tiles are URLs rather than fetches: both go into an img src, so the
browser's cache and decoder handle them, and pulling them into memory first
would defeat both.

Fit whole means aspect preserved, downscaled to fit, never cropped, never
distorted, never force-filled, and not upscaled past 100%. Every override is
a fit-module setting in M5b. The letterbox is background, and is where the
annotation margin tabs go in M5c."
```

---

### Task 7: Three reading modes

**Files:**
- Create: `web/src/pairing.ts`, `web/src/pairing.test.ts`
- Modify: `web/src/App.tsx`

**Interfaces:**
- Produces: `type Mode = 'single' | 'two' | 'scroll'`; `groups(slots: Position[], mode: Mode, reading: 'rtl' | 'ltr'): Group[]`.

- [ ] **Step 1: Write the failing tests**

`web/src/pairing.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { groups } from './pairing'
import { bSlots, whole } from './fixtures'

describe('single', () => {
  it('is one slot per group', () => {
    expect(groups(whole(3), 'single', 'rtl')).toEqual([
      { slots: [0] }, { slots: [1] }, { slots: [2] },
    ])
  })
})

describe('two-page', () => {
  it('pairs slots (0,1), (2,3), …', () => {
    expect(groups(whole(4), 'two', 'rtl')).toEqual([{ slots: [0, 1] }, { slots: [2, 3] }])
  })

  it('leaves a trailing odd slot alone rather than pairing it with nothing', () => {
    expect(groups(whole(3), 'two', 'rtl')).toEqual([{ slots: [0, 1] }, { slots: [2] }])
  })

  it('shows a joined spread as one wide tile, not as two paired halves', () => {
    // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page.
    // The spread is its own group and it restarts pairing after itself,
    // which is why slot 4 opens a new pair rather than joining slot 3.
    expect(groups(bSlots, 'two', 'rtl')).toEqual([
      { slots: [0, 1] },
      { slots: [2, 3], spread: true },
      { slots: [4, 5] },
      { slots: [6, 7] },
      { slots: [8] },
    ])
  })
})

describe('scroll', () => {
  it('is one slot per group, like single — the difference is layout, not grouping', () => {
    expect(groups(whole(3), 'scroll', 'rtl')).toEqual([
      { slots: [0] }, { slots: [1] }, { slots: [2] },
    ])
  })
})
```

- [ ] **Step 2: Run and watch fail**

```bash
npm --prefix web run test
```

Expected: FAIL, cannot resolve `./pairing`.

- [ ] **Step 3: Implement**

`web/src/pairing.ts`:

```typescript
import type { Position } from './resolver'

export type Mode = 'single' | 'two' | 'scroll'

/** One thing drawn at once: one slot, a pair of slots, or a joined spread. */
export interface Group {
  slots: number[]
  spread?: true
}

/**
 * Which slots are drawn together.
 *
 * Two-page mode pairs `(0,1), (2,3), …`, and **a joined spread interrupts
 * that**: its two slots are one wide image, so they form their own group and
 * pairing restarts after it. Without that, every page after the first spread
 * pairs one slot out of phase, which is the parity break the `regroup` module
 * lets a reader fix by hand in M5b — this handles only the case the format
 * already states, where `spreads` makes the break derivable.
 *
 * Reading direction does not appear in the grouping. It decides which slot of
 * a pair is drawn on the right, which is the renderer's business, and the slot
 * sequence itself already accounts for direction — `Edition::slots` orders a
 * spread's halves by it.
 */
export function groups(slots: Position[], mode: Mode, _reading: 'rtl' | 'ltr'): Group[] {
  if (mode !== 'two') return slots.map((_, index) => ({ slots: [index] }))

  const out: Group[] = []
  let index = 0
  while (index < slots.length) {
    const here = slots[index]
    const next = slots[index + 1]
    const joined = here.half !== undefined && next?.half !== undefined && next.page === here.page
    if (joined) {
      out.push({ slots: [index, index + 1], spread: true })
      index += 2
      continue
    }
    // A spread starting at the *second* position of a would-be pair must not
    // be split across two groups, so the pair stops short and the spread
    // opens the next group.
    const nextIsSpreadStart =
      next?.half !== undefined && slots[index + 2]?.page === next.page
    if (next === undefined || nextIsSpreadStart) {
      out.push({ slots: [index] })
      index += 1
      continue
    }
    out.push({ slots: [index, index + 1] })
    index += 2
  }
  return out
}
```

- [ ] **Step 4: Run and watch pass**

```bash
npm --prefix web run test
```

Expected: PASS. If `shows a joined spread as one wide tile` fails, compare the actual grouping against `bSlots` by hand before touching the test — the fixture's spread sits at slots 2 and 3, which is an even boundary, and the `nextIsSpreadStart` branch is only exercised by a spread at an odd boundary. **Add that case** rather than leaving the branch untested:

```typescript
  it('does not split a spread that starts mid-pair', () => {
    // 0, 1R, 1L, 2 — the spread begins at slot 1, the second half of the
    // first would-be pair.
    const odd: Position[] = [
      { page: 0 },
      { page: 1, half: 'R' }, { page: 1, half: 'L' },
      { page: 2 },
    ]
    expect(groups(odd, 'two', 'rtl')).toEqual([
      { slots: [0] },
      { slots: [1, 2], spread: true },
      { slots: [3] },
    ])
  })
```

- [ ] **Step 5: Wire the modes into `App.tsx`**

Render a group: `single` and `scroll` draw one `<PageView>`; `two` draws two side by side, **right-hand slot first under `rtl`**; a `spread: true` group draws one `<PageView>` for the whole page rather than two halves. `scroll` stacks every group vertically in one scrolling column.

- [ ] **Step 6: Commit**

```bash
git add web/src/pairing.ts web/src/pairing.test.ts web/src/App.tsx
git commit -m "Three reading modes, and the spread that interrupts pairing

Two-page mode pairs (0,1), (2,3) and so on, and a joined spread breaks that:
its two slots are one wide image, so they form their own group and pairing
restarts after it. Without that, every page after the first spread pairs one
slot out of phase.

Only the case the format already states is handled here — spreads makes the
break derivable. Breaks a reader inserts by hand are the regroup module, M5b."
```

---

### Task 8: Swap

**Files:**
- Create: `web/src/keys.ts`, `web/src/keys.test.ts`
- Modify: `web/src/App.tsx`

**Interfaces:**
- Produces: `type Intent`; `intentOf(event: KeyboardEvent, editions: string[]): Intent | undefined`.

- [ ] **Step 1: Write the failing tests**

`web/src/keys.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { intentOf } from './keys'

const editions = ['raw', 'en-mono', 'en-colour']
const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
  ({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods }) as KeyboardEvent

describe('intentOf', () => {
  it('maps space to the toggle', () => {
    expect(intentOf(press(' '), editions)).toEqual({ kind: 'toggle' })
  })

  it('maps a digit to the edition at that position, one-based', () => {
    expect(intentOf(press('1'), editions)).toEqual({ kind: 'view', edition: 'raw' })
    expect(intentOf(press('3'), editions)).toEqual({ kind: 'view', edition: 'en-colour' })
  })

  it('ignores a digit past the end rather than wrapping', () => {
    expect(intentOf(press('4'), editions)).toBeUndefined()
    expect(intentOf(press('0'), editions)).toBeUndefined()
  })

  it('maps the arrows to navigation', () => {
    expect(intentOf(press('ArrowRight'), editions)).toEqual({ kind: 'step', by: 1 })
    expect(intentOf(press('ArrowLeft'), editions)).toEqual({ kind: 'step', by: -1 })
  })

  it('yields every key to a modifier, so browser shortcuts still work', () => {
    expect(intentOf(press('1', { metaKey: true }), editions)).toBeUndefined()
    expect(intentOf(press(' ', { ctrlKey: true }), editions)).toBeUndefined()
  })
})
```

**`ArrowRight` steps forward regardless of reading direction, and that is deliberate.** The physical key that advances a page is the one on the side you turn toward; mapping it through `rtl` would make the same key mean opposite things in two works. Revisit if it reads wrong against real material, and record the answer either way.

- [ ] **Step 2: Run and watch fail, then implement**

```bash
npm --prefix web run test
```

`web/src/keys.ts`:

```typescript
/** What a keypress asked for. */
export type Intent =
  | { kind: 'toggle' }
  | { kind: 'view'; edition: string }
  | { kind: 'step'; by: 1 | -1 }

/**
 * A keypress, as an intent, or nothing.
 *
 * The alt-tab model: `Space` flips between the current edition and the last
 * one, so the common back-and-forth stays one key and the gesture does not
 * change as editions are added; `1`..`9` jump directly. Cycling was rejected —
 * with five editions it takes up to four presses and you lose track.
 *
 * A modifier yields the key entirely. A reader that swallowed ⌘1 would break
 * tab switching to save itself a branch.
 */
export function intentOf(event: KeyboardEvent, editions: string[]): Intent | undefined {
  if (event.ctrlKey || event.metaKey || event.altKey) return undefined
  if (event.key === ' ') return { kind: 'toggle' }
  if (event.key === 'ArrowRight') return { kind: 'step', by: 1 }
  if (event.key === 'ArrowLeft') return { kind: 'step', by: -1 }
  if (/^[1-9]$/.test(event.key)) {
    const edition = editions[Number(event.key) - 1]
    return edition === undefined ? undefined : { kind: 'view', edition }
  }
  return undefined
}
```

- [ ] **Step 3: Run and watch pass**

```bash
npm --prefix web run test
```

Expected: PASS, 5 new tests.

- [ ] **Step 4: Wire it into `App.tsx`**

Hold `previous: string` alongside `ReadingState`. On `toggle`, `swap(state, state.viewing === state.authority ? previous : state.authority)` and set `previous` to what was being viewed. On `view`, `swap(state, edition)`. On `step`, `navigate(state, displayedSlot + by)` clamped to the viewed edition's `slotCount`.

**Order the edition list deterministically** — `Resolver.editions()` returns the sidecar's map keys, and a digit that means a different edition between reloads is worse than no digit at all. Sort, with the hub first.

- [ ] **Step 5: Try it against a real multi-edition sidecar, then commit**

```bash
npm --prefix web run test
git add web/src/keys.ts web/src/keys.test.ts web/src/App.tsx
git commit -m "Swap: space toggles, digits jump

The alt-tab model. Space flips between the current edition and the last one,
so the common back-and-forth stays one key and the gesture does not change as
editions are added; 1..9 jump directly. Cycling was rejected because with
five editions it takes up to four presses and you lose track of where you are.

A modifier yields the key entirely — a reader that swallowed cmd-1 would break
tab switching to save itself a branch. The edition list is sorted with the hub
first, because a digit meaning a different edition between reloads is worse
than no digit at all."
```

---

### Task 9: The gap state

**Files:**
- Modify: `web/src/App.tsx`
- Create: `web/src/Gap.tsx`, `web/src/nearest.ts`, `web/src/nearest.test.ts`

**Interfaces:**
- Produces: `nearestPaired(state, resolver, within): { before?: number; after?: number }`.

- [ ] **Step 1: Write the failing tests**

`web/src/nearest.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { fakeResolver, whole, bSlots } from './fixtures'
import { nearestPaired } from './nearest'

const r = fakeResolver(
  { a: whole(8), b: bSlots },
  { 'a|b|1': 0, 'a|b|2': 1, 'a|b|3': 2 },
)

describe('nearestPaired', () => {
  it('offers the paired slots either side of a gap', () => {
    // `a` slot 4 is unpaired; 3 is the nearest paired before it and there is
    // nothing paired after it.
    expect(nearestPaired({ authority: 'a', slot: 4, viewing: 'b' }, r, 8))
      .toEqual({ before: 3 })
  })

  it('offers only what exists at the start of a volume', () => {
    expect(nearestPaired({ authority: 'a', slot: 0, viewing: 'b' }, r, 8))
      .toEqual({ after: 1 })
  })

  it('offers nothing where the whole edition is unpaired', () => {
    const none = fakeResolver({ a: whole(3), b: whole(3) }, {})
    expect(nearestPaired({ authority: 'a', slot: 1, viewing: 'b' }, none, 3)).toEqual({})
  })
})
```

- [ ] **Step 2: Run, watch fail, implement**

`web/src/nearest.ts`:

```typescript
import { display, type ReadingState } from './reading'
import type { Resolver } from './resolver'

/**
 * The nearest slots either side of a gap that do resolve.
 *
 * Offered as explicit moves rather than jumped to. `notes.md` leans this way
 * and the design took it: landing on the nearest silently is the behaviour
 * that makes a swap lose your place, and the whole authority model exists to
 * prevent it. Taking one of these is real navigation, so authority moves —
 * visibly, and only because it was asked for.
 *
 * `within` bounds the search so a wholly unpaired edition costs one scan
 * rather than one per render; it is the authority edition's slot count.
 */
export function nearestPaired(
  state: ReadingState,
  resolver: Resolver,
  within: number,
): { before?: number; after?: number } {
  const resolves = (slot: number) => display({ ...state, slot }, resolver).kind === 'at'
  const found: { before?: number; after?: number } = {}
  for (let slot = state.slot - 1; slot >= 0; slot--) {
    if (resolves(slot)) { found.before = slot; break }
  }
  for (let slot = state.slot + 1; slot < within; slot++) {
    if (resolves(slot)) { found.after = slot; break }
  }
  return found
}
```

- [ ] **Step 3: Run and watch pass, then build the view**

`web/src/Gap.tsx` names the edition and the slot being looked for, and renders a button per offered move. It must **not** be a toast and must not auto-dismiss: the gap is a view, and one press of the toggle returns exactly where you were.

- [ ] **Step 4: Show the verdict alongside it**

The twenty-two needles established that acceptance is a statement about calibration and only the verdict is a statement about the alignment, and the design requires the verdict to travel with the swap. The sidecar's `alignments[].quality.verdict` carries it. Surface it in the gap view and beside a swap into a non-`sound` pair.

`Resolver` has no verdict accessor, and the binding has no `trust` yet. **Read it from the sidecar JSON in TypeScript for now** — it is a field lookup, not format arithmetic, so it does not violate the one-implementation rule the way reimplementing run composition would. Record it as a follow-up for M5b, where `readouts` needs the same value and a binding accessor becomes worth adding.

- [ ] **Step 5: Commit**

```bash
git add web/src/nearest.ts web/src/nearest.test.ts web/src/Gap.tsx web/src/App.tsx
git commit -m "A swap onto a gap succeeds and says so

The reader names the edition and the slot it was looking for and offers the
nearest paired slots either side as explicit moves. Not a toast, not a silent
jump to the nearest — landing on the nearest silently is what makes a swap
lose your place, and the authority model exists to prevent exactly that.

Taking an offered move is real navigation, so authority moves, visibly and
only because it was asked for. The pair's verdict travels with the swap: the
needles established that acceptance is a claim about calibration and only the
verdict is a claim about the alignment."
```

---

### Task 10: The chrome shell and the filmstrip

**Files:**
- Create: `web/src/Chrome.tsx`, `web/src/Filmstrip.tsx`
- Modify: `web/src/App.tsx`, `web/src/keys.ts`, `web/src/keys.test.ts`

- [ ] **Step 1: Add the chrome key**

Extend `Intent` with `{ kind: 'chrome' }` on `Tab`, and add the test alongside the existing `keys.test.ts` cases:

```typescript
  it('maps tab to the chrome toggle', () => {
    expect(intentOf(press('Tab'), editions)).toEqual({ kind: 'chrome' })
  })
```

- [ ] **Step 2: Build the shell**

`web/src/Chrome.tsx` renders the floating strip container and the readout row, **shown and hidden together**, and returns `null` when hidden — the page is edge-to-edge with no chrome at all while reading.

Geometry, from the mockups the design was settled against:

- The container is `position: absolute`, inset from left and right, and **lifted off the bottom edge** rather than docked to it.
- It **overlays** the page rather than displacing it, so showing and hiding never reflows what is being read. The page area's size must not depend on chrome visibility — if resizing happens on toggle, the layout is wrong.
- The readout row sits **below** the strip, on the bottom edge.
- The row is empty in this plan except for `slot N / M`. The other three readouts are the `readouts` module, M5b.

- [ ] **Step 3: Build the filmstrip, navigate job only**

`web/src/Filmstrip.tsx` renders one tile per group of the viewed edition using `thumbUrl`, laid out **as pages will be paired** — pairs read as pairs, a `spread: true` group as one wide tile — with the current group marked. Clicking a tile is real navigation.

Two jobs are deliberately absent: **regroup** is the `regroup` module (M5b) and **correct** is M5d.

Load tiles lazily with `loading="lazy"` on each `<img>`. A 195-tile strip requesting every tile at once on a cold cache is the burst O9 names as its reopening condition, and this plan has no single-flight guard; lazy loading keeps the first open to what is on screen. **Note in the commit that this is a mitigation, not the fix.**

- [ ] **Step 4: Verify the no-reflow property by hand**

```bash
npm --prefix web run dev
```

Press `Tab` repeatedly while watching the page. Expected: the page does not move or resize; only the strip and the row appear and disappear. If the page shifts, the container is in the layout flow and must be taken out of it.

- [ ] **Step 5: Run the suite and commit**

```bash
npm --prefix web run test
npm --prefix web run build
git add web/src/Chrome.tsx web/src/Filmstrip.tsx web/src/App.tsx web/src/keys.ts web/src/keys.test.ts
git commit -m "The chrome shell: a floating strip that never reflows the page

Reading is edge-to-edge with no persistent chrome. Tab brings the strip and
the readout row in together and takes them away together. The strip floats,
lifted off the bottom edge rather than docked to it, so it overlays the page
instead of displacing it and summoning it never reflows what you are reading.

The filmstrip does one of its three jobs here. Regroup is the regroup module
in M5b; correct is M5d.

Tiles load lazily. A 195-tile strip asking for every tile at once on a cold
cache is the burst O9 names as its reopening condition, and this plan has no
single-flight guard, so lazy loading is a mitigation and not the fix."
```

---

### Task 11: Restore where you were, and close the milestone

**Files:**
- Modify: `web/src/App.tsx`, `docs/status.md`

- [ ] **Step 1: Restore on load, record on navigation**

On load, `getProgress(work)`; where it returns a position, start at `{ authority: edition, slot, viewing: edition }`. Where it returns `undefined`, start at slot 0 of the hub.

On `navigate` — and **only** on `navigate`, never on `swap` — `putProgress(work, { edition: state.authority, slot: state.slot })`. A swap does not move authority, so it has nothing to record; writing on swap would put drift back into the one place the authority model exists to keep it out of.

Debounce the write. Arrowing through twenty pages should not be twenty `PUT`s.

- [ ] **Step 2: Confirm the server's slot range check agrees**

`put_progress` range-checks **in slots**, not pages, and answers 404 naming the edition and its slot count. Confirm the reader never sends a slot the server refuses:

```bash
# With serve running and a work whose edition joins spreads:
curl -i -X PUT -H 'content-type: application/json' \
  -d '{"edition":"en-mono","slot":0}' \
  http://127.0.0.1:8080/api/works/<work>/progress
```

Expected: `204`. Then arrow to the last slot in the reader and confirm no 404 appears in the console — the last slot of a spread-joining edition is the case that check was written for.

- [ ] **Step 3: Run everything**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo build -p taiyaku-wasm --target wasm32-unknown-unknown
./scripts/build-wasm.sh && node scripts/smoke-wasm.mjs
npm --prefix web run test
npm --prefix web run build
./scripts/check-all.sh
```

Expected: all green. `check-all.sh` skips its gated checks cleanly without `TAIYAKU_CORPUS`; run it **with** the corpus if it is to hand, since Task 2 touched `cover.rs`.

- [ ] **Step 4: Update `docs/status.md`**

Four edits, all of them things this plan made true or found false:

1. **M5a lands.** Add it to "What comes next" §3 alongside M1–M4, saying what shipped: the swap works, three reading modes, the gap state, a floating chrome shell, a thumbnail route.
2. **Correct line 1001.** It says the swap UX is undecided and that `notes.md` leaves it so. The reader design settled it on 2026-07-30 and re-settled it on 2026-08-01; the picker mockups belong to a future native iOS reader. Say that instead.
3. **Record the thumbnail cache figure** from Task 2, Step 10 against O11, beside the covers' 165 KB.
4. **Note what M5a leaves open**: no prefetch and no double buffer, so a fast flick flickers, closed in M5b; no single-flight guard, so O9's reopening condition is now producible by the filmstrip and is mitigated only by lazy loading.

Add a fifth if Task 3, Step 3 or Step 5 found the binding wrong — a defect found by the first execution of the compiled module is exactly the kind of finding these documents exist to carry.

- [ ] **Step 5: Commit**

```bash
git add web/src/App.tsx docs/status.md
git commit -m "Restore where you were, and record what M5a left open

Progress is written on navigation and never on a swap. A swap does not move
authority, so it has nothing to record, and writing on one would put drift
back into the one place the authority model exists to keep it out of.

status.md line 1001 is corrected: it called the swap UX undecided, and the
reader design settled it twice. What M5a leaves open is recorded rather than
discovered later — no prefetch and no double buffer, so a fast flick still
flickers, and the filmstrip can now produce the concurrent burst O9 names as
its reopening condition, mitigated by lazy loading and not fixed."
```

---

## Self-review

**Spec coverage.** The M5a row is "page-view core, three modes, swap, gap state, chrome shell" with end state *the swap works* — Tasks 6, 7, 8, 9, 10 respectively. Tasks 1, 2, 3, 4 are foundation the spec assumes rather than names, and two of them are gaps found by reading the code during planning rather than requirements taken from the spec:

- **Task 1 has no spec line and is the plan's most important task.** The spec's authority model tracks `slot: number` and the binding resolves on `(page, half)`; nothing converted between them. Without it the reader cannot call the binding at all.
- **Task 2** implements the spec's open question "filmstrip thumbnail caching… needs deciding in M5a", and produces the measurement O11 asks for.
- **Task 3** discharges what M4's plan explicitly deferred: no TS package was emitted, and the module has never been executed under any runtime.
- **Task 4** is scaffolding; its one real decision is that CI needs no Rust, which follows from Task 5's `Resolver` seam.

Spec items deliberately not covered here, each named in "What this plan does not do": the module system and six modules (M5b), annotations (M5c), correction UI and O13 (M5d), prefetch and double-buffering (M5b), thumbnail pruning (O11, wants the number first).

**Placeholder scan.** No "TBD", no "add error handling", no "similar to Task N". Three steps deliberately instruct the implementer to *read before writing* rather than giving code — Task 2 Step 6 (the `oneshot` helper), Task 2 Step 8 (`thumb_cache_dir`'s sibling), Task 4 Step 5 (the clone step). Each names the exact `grep` and what to match, because copying a guessed version of an existing pattern is how a plan introduces drift.

**Type consistency.** `Position { page, half? }` is used identically in `resolver.ts`, `fixtures.ts`, `pairing.ts` and `nearest.ts`. `ReadingState { authority, slot, viewing }` matches the spec's `{ authority, slot, viewing }` exactly. `slot` is a flat index everywhere — in `ReadingState`, in `StoredProgress`, in `slotAt`/`indexOf`, and in the server's range check. `Landed.via` and `Display.via` share the same four-word union that `via_word` produces in Rust.

**One known weakness.** Task 5's round-trip test is nearly tautological as written, and the plan says so at the point it is added rather than presenting it as stronger than it is. It guards a future change to `swap`, not the present one.

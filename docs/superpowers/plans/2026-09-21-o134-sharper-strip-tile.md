# O134 — a sharper strip tile — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The filmstrip asks the server for a tile sized to the screen it is
drawn on, instead of the one 160 px tile every screen gets today.

**Architecture:** The tile cache is already keyed by long edge
(`cover.rs`'s `thumb_keyed_path`), so the server change is threading an edge
through two functions and adding one branch to `get_thumb` that honours `?w=`
for an account that may stream. The client picks a rung from a three-value
ladder — 160, 320, 480 — using the strip's CSS height and `devicePixelRatio`,
the same shape `pageSize.ts` already uses for full pages.

**Tech Stack:** Rust (axum, `image`), TypeScript/React, vitest across three
projects (`node`, `jsdom`, `browser`).

**The design this implements:**
`docs/superpowers/specs/2026-09-21-o134-sharper-strip-tile-design.md`. Read it
first; it records which alternatives each decision beat.

## Global Constraints

- **Branch:** `o134-sharper-strip-tile`, off `main` at `db5bb80b`. The remote
  is squash-only: the PR body becomes the one commit message.
- **Never run the gate with `TAIYAKU_CORPUS` set.** `env -u TAIYAKU_CORPUS scripts/check-all.sh`,
  and quote the `all green` line when reporting it — piping it through `tail`
  masks the exit status.
- **Throttle test threads:** `NEXTEST_TEST_THREADS=8` and `RUST_TEST_THREADS=8`.
  An unthrottled `cargo test` on this machine has been killed for memory.
- **Vitest under an agent picks a reporter that hides console output.** Always
  pass `--reporter=default`.
- **No attribution of any kind** in code, comments, docs, commit messages or
  the PR body.
- **A test must be watched failing against the defect it names.** Hand-apply the
  defect, run the test, see it fail, restore. A test that passes against its own
  defect is the failure mode this repo has hit more than any other.
- **After restoring a hand-applied Rust mutation, rebuild before driving the
  binary:** `cargo nextest` rebuilds `target/debug/taiyaku`.
- **Sites are named by symbol, never by line number**, in every doc comment and
  every `docs/status.md` cell.
- **`docs/status.md` claims must trace to a measurement in `docs/notes.md`.**

---

### Task 1: The tile cache takes a long edge

No behaviour changes: every caller passes `THUMB_LONG_EDGE` and every existing
file stays at its existing path. This is the plumbing the route change needs.

**Files:**
- Modify: `crates/taiyaku-serve/src/cover.rs` — `THUMB_LONG_EDGE`,
  `TILE_LONG_EDGES`, `thumb_cache_path`, `cached_or_generated_thumb`
- Modify: `crates/taiyaku-serve/src/cover/tests.rs` — every call to the two
  above
- Modify: `crates/taiyaku-serve/src/routes.rs` — the one call in `get_thumb`
- Modify: `crates/taiyaku-serve/src/routes/tests.rs` — the one call to
  `thumb_cache_path`
- Modify: `crates/taiyaku-serve/src/tests.rs` — the one call to
  `thumb_cache_path`

**Interfaces:**
- Produces: `cover::THUMB_LONG_EDGE: u32` (now `pub`), `cover::TILE_LONG_EDGES: [u32; 3]`,
  `cover::thumb_cache_path(dir: &Path, sha256: &str, offset: u32, long_edge: u32) -> PathBuf`,
  `cover::cached_or_generated_thumb(dir: &Path, sha256: &str, offset: u32, long_edge: u32, page: impl FnOnce() -> Result<Vec<u8>, String>) -> Result<Vec<u8>, String>`

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/cover/tests.rs`, at the end of the file:

```rust
/// O134: the long edge a tile is generated at and the path it is cached under
/// come from one argument. A second size is a second file, and the first is
/// left exactly as it was — which is what lets a library that has been read
/// keep every tile it has already made.
#[test]
fn a_tile_is_cached_and_generated_at_the_long_edge_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let page = || Ok(real_page_bytes());

    let small = cached_or_generated_thumb(dir.path(), "abc", 7, THUMB_LONG_EDGE, page).unwrap();
    let large = cached_or_generated_thumb(dir.path(), "abc", 7, 480, page).unwrap();

    assert_eq!(long_edge_of(&small), THUMB_LONG_EDGE, "the 160 px tile");
    assert_eq!(long_edge_of(&large), 480, "the 480 px tile");
    assert!(
        thumb_cache_path(dir.path(), "abc", 7, THUMB_LONG_EDGE).exists(),
        "the 160 px file is still there"
    );
    assert!(
        thumb_cache_path(dir.path(), "abc", 7, 480).exists(),
        "and the 480 px file is beside it"
    );
    assert_ne!(
        std::fs::read(thumb_cache_path(dir.path(), "abc", 7, THUMB_LONG_EDGE)).unwrap(),
        std::fs::read(thumb_cache_path(dir.path(), "abc", 7, 480)).unwrap(),
        "two sizes are two images, not one file written twice"
    );
}

/// The long edge of an encoded image, for the test above.
fn long_edge_of(bytes: &[u8]) -> u32 {
    let image = image::load_from_memory(bytes).expect("a decodable image");
    image.width().max(image.height())
}
```

`real_page_bytes()` already exists in this file (used at `cover/tests.rs`'s
`a_tile_is_the_thumbnail_of_the_page`); check its name at the top of the file
before running and use whatever that helper is actually called.

- [ ] **Step 2: Run it and watch it fail to compile**

```bash
cd /home/davey/projects/taiyaku
NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve --test-threads=8 a_tile_is_cached_and_generated
```

Expected: a compile error — `this function takes 4 arguments but 5 arguments
were supplied` for `cached_or_generated_thumb`. A compile failure is the
failing state for this task's first step.

- [ ] **Step 3: Widen the two functions**

In `crates/taiyaku-serve/src/cover.rs`, make the constant public and add the
ladder beside it:

```rust
/// The long edge of a filmstrip tile, in pixels — the size every tile was
/// served at until O134, and still the one a request that does not ask for
/// another gets.
///
/// A cover is 512 because it fills a grid cell; a tile sits in a strip along
/// the bottom edge and there are ~195 of them per volume, so it is sized for
/// the strip rather than for a grid. Folded into the cache path exactly as
/// `LONG_EDGE` is, so changing it invalidates by construction.
pub const THUMB_LONG_EDGE: u32 = 160;

/// The long edges a filmstrip tile is offered at, one per screen density —
/// O134.
///
/// A tile is drawn 160 px tall in the strip, and 96 below the 900 px
/// breakpoint (`web/src/styles/reader.css`'s `--strip-tile`), so one, two and
/// three device pixels per CSS pixel want at most 480 — and a phone, whose
/// tile is the shorter one, wants 320 rather than the 480 an `srcSet`'s `3x`
/// descriptor would have fetched.
///
/// **The web holds the same three** (`web/src/tileSize.ts`'s `TILE_LADDER`),
/// the way `COVER_LONG_EDGES` is held on both sides. A rung added there alone
/// is a `w` this server does not recognise, answered with the 160 px tile —
/// soft rather than broken, and so invisible to any test that does not decode
/// what came back.
pub const TILE_LONG_EDGES: [u32; 3] = [THUMB_LONG_EDGE, 320, 480];
```

Then take a long edge in both functions:

```rust
/// Where a filmstrip tile for one page of one member is cached, at
/// `long_edge`.
///
/// Keyed on the member's `sha256` *and the page's offset within it*, because
/// a member has one cover and 195 tiles. The offset rather than the
/// edition-flat index, so a tile survives the edition gaining a member ahead
/// of this one. The long edge is an argument rather than the constant since
/// O134: one page now has a file per size a screen has asked for.
pub fn thumb_cache_path(dir: &Path, sha256: &str, offset: u32, long_edge: u32) -> PathBuf {
    thumb_keyed_path(dir, sha256, offset, long_edge, FORMAT_VERSION)
}
```

```rust
/// Read a cached filmstrip tile at `long_edge`, or make one and cache it.
///
/// **One `long_edge` feeds both the path and the generator**, deliberately as
/// a single argument: a file named for one size holding the pixels of another
/// is a cache poisoning no later request can detect, since the bytes decode
/// perfectly at the wrong size.
pub fn cached_or_generated_thumb(
    dir: &Path,
    sha256: &str,
    offset: u32,
    long_edge: u32,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(
        &thumb_cache_path(dir, sha256, offset, long_edge),
        dir,
        page,
        |bytes| thumbnail_at(&bytes, long_edge),
    )
}
```

Update the module doc at the top of `cover.rs`: the sentence "the long edge
(`LONG_EDGE` for a cover, `THUMB_LONG_EDGE` for a tile)" now reads "…for a
tile that did not ask for another — `TILE_LONG_EDGES` are the ones it may ask
for". Update `at_path`'s doc comment too, where it says
"`thumbnail_at(_, THUMB_LONG_EDGE)` for a tile".

- [ ] **Step 4: Fix every caller**

```bash
cd /home/davey/projects/taiyaku
grep -rn "cached_or_generated_thumb\|thumb_cache_path" --include='*.rs' crates/ | grep -v '/target/'
```

Every hit outside `cover.rs` passes `crate::cover::THUMB_LONG_EDGE` as the new
argument — `routes.rs`'s call inside `get_thumb`, the calls in
`cover/tests.rs`, the one in `routes/tests.rs`, the one in `tests.rs`. Nothing
else changes; no test's expected value moves.

- [ ] **Step 5: Run the crate's tests**

```bash
cd /home/davey/projects/taiyaku
NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve --test-threads=8
```

Expected: PASS, including the new test, with no other test's expectation
changed.

- [ ] **Step 6: Watch the new test fail against its own defect**

In `cached_or_generated_thumb`, change `thumbnail_at(&bytes, long_edge)` to
`thumbnail_at(&bytes, THUMB_LONG_EDGE)` — the poisoning the doc comment names.
Run the test; it must fail on `the 480 px tile`. Restore the line and re-run.

- [ ] **Step 7: Commit**

```bash
cd /home/davey/projects/taiyaku
git add crates/taiyaku-serve/src/cover.rs crates/taiyaku-serve/src/cover/tests.rs \
        crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/routes/tests.rs \
        crates/taiyaku-serve/src/tests.rs
git commit -m "O134: a tile's long edge is an argument, not a constant"
```

---

### Task 2: `can_stream` as a predicate

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs` — `can_stream` (new),
  `refuse_if_cannot_stream`
- Modify: `crates/taiyaku-serve/src/routes/tests.rs` — one new test

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `routes::can_stream(extensions: &axum::http::Extensions) -> bool`,
  `pub(crate)`.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-serve/src/routes/tests.rs`, beside the other capability
tests:

```rust
/// O134: the rule `refuse_if_cannot_stream` refuses on, read as a question.
///
/// **No `Principal` means yes**, and that is the branch this pins: a server
/// configured without auth puts none in the extensions and every route is open
/// there. `get_thumb` asks this before honouring a `?w=`, so a `false` here
/// would drop every tile in that configuration to 160 px silently — no test
/// that signs in can reach it, and a soft tile looks like a tile.
#[test]
fn can_stream_is_true_when_no_principal_is_present_and_follows_the_flag_when_one_is() {
    let empty = axum::http::Extensions::new();
    assert!(can_stream(&empty), "no principal at all may stream");

    let mut may = axum::http::Extensions::new();
    may.insert(crate::auth::Principal {
        can_stream: true,
        ..crate::test_support::principal()
    });
    assert!(can_stream(&may));

    let mut may_not = axum::http::Extensions::new();
    may_not.insert(crate::auth::Principal {
        can_stream: false,
        ..crate::test_support::principal()
    });
    assert!(!can_stream(&may_not));
    assert!(
        refuse_if_cannot_stream(&may_not).is_some(),
        "and the refusal still refuses on exactly that"
    );
}
```

`crate::test_support::principal()` may not exist under that name. Before
running, find how the existing tests build a `Principal` —
`grep -rn "Principal {" --include='*.rs' crates/taiyaku-serve/src | grep -v '/target/'`
(`auth/tests.rs` around its capability tests builds one) — and construct it the
same way, listing every field if there is no constructor. Do not add a
`Default` derive to `Principal` to make this shorter: a `Default` on a
capability struct is how a mutation survives, because every field it does not
name silently becomes `false`.

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /home/davey/projects/taiyaku
NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve --test-threads=8 can_stream_is_true_when_no_principal
```

Expected: a compile error, `cannot find function can_stream in this scope`.

- [ ] **Step 3: Extract the predicate**

In `crates/taiyaku-serve/src/routes.rs`, above `refuse_if_cannot_stream`:

```rust
/// Whether this request may be shown a page's own pixels.
///
/// The rule [`refuse_if_cannot_stream`] refuses on, as a question, so the two
/// callers that need it — that refusal and `get_thumb`'s sized tile (O134) —
/// read one statement of it rather than two that can drift apart.
///
/// **No `Principal` means yes**, which is [`refuse_if_cannot_stream`]'s own
/// behaviour preserved exactly: a server configured without auth puts none in
/// the extensions and every route here is open in that configuration. The
/// same shape as `opds.rs`'s `can_download` check.
pub(crate) fn can_stream(extensions: &axum::http::Extensions) -> bool {
    extensions
        .get::<crate::auth::Principal>()
        .is_none_or(|principal| principal.can_stream)
}
```

and rewrite the refusal to call it, keeping its doc comment and its message
byte-for-byte:

```rust
pub(crate) fn refuse_if_cannot_stream(extensions: &axum::http::Extensions) -> Option<Response> {
    (!can_stream(extensions)).then(|| {
        (
            StatusCode::FORBIDDEN,
            "this account may browse and download but not read pages here",
        )
            .into_response()
    })
}
```

- [ ] **Step 4: Run the crate's tests**

```bash
cd /home/davey/projects/taiyaku
NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve --test-threads=8
```

Expected: PASS. Every existing `can_stream` test — `download/tests.rs`'s
`without_can_stream_the_pages_are_403_and_the_download_still_works` among them
— must pass unchanged; this step changes no behaviour.

- [ ] **Step 5: Watch it fail against its own defect**

Change `is_none_or` to `is_some_and` and run the new test: it must fail on
`no principal at all may stream`. Restore and re-run.

- [ ] **Step 6: Commit**

```bash
cd /home/davey/projects/taiyaku
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/routes/tests.rs
git commit -m "O134: the stream rule, as a predicate"
```

---

### Task 3: The tile route honours `?w=` for an account that may stream

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs` — `tile_edge` (new), `get_thumb`
- Modify: `crates/taiyaku-serve/src/routes/tests.rs` — three new tests, one
  existing doc comment rewritten and renamed

**Interfaces:**
- Consumes: `cover::TILE_LONG_EDGES`, `cover::THUMB_LONG_EDGE`,
  `cover::cached_or_generated_thumb(dir, sha, offset, long_edge, page)` from
  Task 1; `can_stream` from Task 2.
- Produces: `GET /api/works/{id}/{edition}/pages/{n}/thumb?w={160|320|480}`.

- [ ] **Step 1: Write the three failing tests**

In `crates/taiyaku-serve/src/routes/tests.rs`, beside
`the_tile_route_sizes_page_zero_only_and_only_to_a_cover_size`:

```rust
/// O134: any page, at any rung of `cover::TILE_LONG_EDGES`, for a caller that
/// may stream — this fixture's router has no `Principal`, which `can_stream`
/// answers `true` for.
///
/// Page 1 rather than page 0, because page 0 at a *cover* size is O133's
/// cover path and would prove nothing about this one. The pages are 600×900
/// (`test_support::state_with_tall_pages`), so every rung is a real downward
/// resize and the served image says which.
#[tokio::test]
async fn the_tile_route_sizes_any_page_to_a_tile_rung() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);

    for edge in crate::cover::TILE_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/1/thumb?w={edge}");
        let (status, headers, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(long_edge_of(&body), edge, "{uri}: page 1 at {edge} px");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).expect("a content type"),
            "image/jpeg",
            "{uri}: a tile is always JPEG"
        );
    }
}

/// O134's line: a sharper tile is for a reader, and an account that may browse
/// and download but not read pages is not one.
///
/// It is answered with the 160 px tile rather than a `403` — the same fallback
/// `cover_edge` already makes for a `w` this server does not recognise — so
/// such an account's strip is soft rather than broken. It costs that account
/// nothing real: the page route refuses it every page, asserted below, so
/// there is nothing behind the strip to be sharp about.
///
/// **This is the test O133 did not have.** `download/tests.rs` asks only for
/// the unsized tile, which is how O133's first version reached review serving
/// every cover through the page route that refuses this account.
#[tokio::test]
async fn a_download_only_account_gets_the_unsized_tile_whatever_rung_it_asks_for() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    without_can_stream(&state);
    let app = crate::test_support::authenticating(build_router(state.clone()), &state.library);

    for edge in crate::cover::TILE_LONG_EDGES {
        let uri = format!("/api/works/w/a/pages/1/thumb?w={edge}");
        let (status, _, body) = fetch(&app, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(
            long_edge_of(&body),
            crate::cover::THUMB_LONG_EDGE,
            "{uri}: the tile, not the rung"
        );
    }

    let (status, _, _) = fetch(&app, "/api/works/w/a/pages/1?w=480", None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the page route still refuses this account a page"
    );
}
```

```rust
/// O134's fallback, for a caller that *may* stream: a `w` this server makes no
/// tile at is the 160 px tile — not a `400`, and not a resize to whatever was
/// asked for.
///
/// **The neighbouring download-only test cannot stand in for this one.** There
/// `can_stream` short-circuits the whole branch before `tile_edge`'s answer
/// matters, so it would pass against a `tile_edge` that accepted every number.
/// Here nothing short-circuits, so it is `tile_edge` itself being pinned —
/// including that a *cover* size is not a tile size on a page that is not
/// page 0.
#[tokio::test]
async fn a_w_that_is_not_a_tile_rung_is_the_tile() {
    let (state, _fixture) = crate::test_support::state_with_tall_pages();
    let app = build_router(state);

    for uri in [
        "/api/works/w/a/pages/1/thumb?w=768",
        "/api/works/w/a/pages/1/thumb?w=512",
        "/api/works/w/a/pages/1/thumb?w=321",
        "/api/works/w/a/pages/1/thumb?w=abc",
        "/api/works/w/a/pages/1/thumb?w=",
        "/api/works/w/a/pages/1/thumb",
    ] {
        let (status, _, body) = fetch(&app, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(
            long_edge_of(&body),
            crate::cover::THUMB_LONG_EDGE,
            "{uri}: not a rung, so the tile"
        );
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /home/davey/projects/taiyaku
NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve --test-threads=8 tile_route_sizes_any_page
```

Expected: FAIL — `assertion left == right failed: /api/works/w/a/pages/1/thumb?w=320: page 1 at 320 px`,
left `160`, right `320`. The second test passes already; that is correct and
expected — it is the guard that must still hold after step 3, not a red bar.

- [ ] **Step 3: Add the branch**

In `crates/taiyaku-serve/src/routes.rs`, below `cover_edge`:

```rust
/// The long edge to serve a tile at, if the request asked for one this server
/// makes: **any** page, at exactly one of `cover::TILE_LONG_EDGES` — O134.
///
/// Unlike [`cover_edge`] this is not restricted to page 0, which is why its
/// caller pairs it with [`can_stream`]: sizing any page for an account that
/// may not read pages is exactly what `cover_edge`'s own doc refuses to do.
/// Anything else — an unknown size, a `w` that is not a number, no `w` at all
/// — is `None` and so the 160 px tile.
fn tile_edge(w: Option<&str>) -> Option<u32> {
    let edge = w?.parse::<u32>().ok()?;
    crate::cover::TILE_LONG_EDGES.contains(&edge).then_some(edge)
}
```

In `get_thumb`, after the existing `cover_edge` branch and before the
`spawn_blocking`:

```rust
    // O134. Below `cover_edge` deliberately: page 0 at a *cover* size stays
    // O133's cover — the page route's own answer, validator and all — and a
    // tile rung is a tile. `can_stream` is what keeps this route's exemption
    // from `refuse_if_cannot_stream` (see `get_thumb`'s doc) from becoming a
    // way to read pages: an account without it is served the 160 px tile, the
    // same fallback an unrecognised `w` gets.
    let edge = tile_edge(first_query_value(&query, "w"))
        .filter(|_| can_stream(&request_extensions))
        .unwrap_or(crate::cover::THUMB_LONG_EDGE);
```

and pass `edge` into the closure:

```rust
        crate::cover::cached_or_generated_thumb(&dir, &sha, offset, edge, || {
            state.page(found, &edition, index).map(|p| p.bytes)
        })
```

Extend `get_thumb`'s own doc comment: the paragraph beginning "**`?w=` makes
page 0 a library cover**" gains a sentence — a `w` at one of
`cover::TILE_LONG_EDGES` on any page is that tile at that size for a caller
that may stream, and every other `w` is still the 160 px tile.

- [ ] **Step 4: Run them again**

```bash
cd /home/davey/projects/taiyaku
NEXTEST_TEST_THREADS=8 cargo nextest run -p taiyaku-serve --test-threads=8
```

Expected: PASS, whole crate.

- [ ] **Step 5: Correct the doc comment O134 falsifies**

`the_tile_route_sizes_page_zero_only_and_only_to_a_cover_size` opens "The tile
route's `w` is for covers and nothing else", which this task makes false. Its
*assertions* all still hold — it runs `without_can_stream`, and `w=768`,
`w=700`, `w=1000` and `w=abc` are none of them tile rungs — so rewrite the doc
comment rather than the test, and rename it to
`the_tile_route_sizes_page_zero_to_a_cover_size_and_refuses_the_rest_a_size`.
Say in it: since O134 a `w` may also be a tile rung, on any page, for a caller
that may stream; this test's account may not, and none of these sizes is a rung
anyway.

Then re-read O133's row in `docs/status.md` and check no other sentence in the
repo now claims the tile route sizes page 0 alone:

```bash
cd /home/davey/projects/taiyaku
grep -rn "cover_edge\|page 0 alone\|for covers and nothing else" --include='*.rs' --include='*.ts' --include='*.md' crates/ web/src/ docs/ | grep -v '/target/'
```

Read every hit. O133's own row is closed prose and is left as written; live doc
comments are not.

- [ ] **Step 6: Watch each test fail against its own defect**

Two mutations, one at a time, each restored before the next:

1. Drop the `.filter(|_| can_stream(&request_extensions))`. Expected:
   `a_download_only_account_gets_the_unsized_tile_whatever_rung_it_asks_for`
   fails on `the tile, not the rung`.
2. Change `tile_edge`'s `contains` test to `crate::cover::COVER_LONG_EDGES.contains(&edge)`.
   Expected: `the_tile_route_sizes_any_page_to_a_tile_rung` fails at `w=320`.
3. Make `tile_edge` accept anything that parses — `Some(edge)` in place of the
   `contains` test. Expected: `a_w_that_is_not_a_tile_rung_is_the_tile` fails on
   `?w=768`. **Watch which tests stay green here**: the download-only test does,
   which is the reason that third test exists.

- [ ] **Step 7: Commit**

```bash
cd /home/davey/projects/taiyaku
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/routes/tests.rs
git commit -m "O134: the tile route sizes any page for a reader"
```

---

### Task 4: `tileSize.ts` — the ladder, and the constants the CSS is pinned against

**Files:**
- Create: `web/src/tileSize.ts`
- Create: `web/src/tileSize.test.ts`
- Modify: `web/src/styles/reader.css.test.ts` — the existing
  `stands the strip's tiles at 160 px and 96 below the breakpoint` case

**Interfaces:**
- Produces: `TILE_LADDER: readonly number[]`, `STRIP_BREAKPOINT_PX: number`,
  `STRIP_TILE_PX: { wide: number; narrow: number }`,
  `requestedTileEdge(tileHeightCss: number, dpr: number): number`,
  `stripTileEdge(view: TileView | undefined): number`,
  `interface TileView { matchMedia?: (query: string) => { matches: boolean }; devicePixelRatio?: number }`

- [ ] **Step 1: Write the failing tests**

Create `web/src/tileSize.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import {
  requestedTileEdge,
  STRIP_BREAKPOINT_PX,
  STRIP_TILE_PX,
  stripTileEdge,
  TILE_LADDER,
} from './tileSize'

describe('requestedTileEdge', () => {
  it('rounds the pixels a screen wants up to a rung', () => {
    // The six cases that exist: two strip heights, three densities.
    expect(requestedTileEdge(96, 1)).toBe(160)
    expect(requestedTileEdge(96, 2)).toBe(320)
    expect(requestedTileEdge(96, 3)).toBe(320)
    expect(requestedTileEdge(160, 1)).toBe(160)
    expect(requestedTileEdge(160, 2)).toBe(320)
    expect(requestedTileEdge(160, 3)).toBe(480)
  })

  it('rounds up rather than down at a rung’s own value and one past it', () => {
    expect(requestedTileEdge(160, 1)).toBe(160)
    expect(requestedTileEdge(161, 1)).toBe(320)
    expect(requestedTileEdge(320, 1)).toBe(320)
    expect(requestedTileEdge(321, 1)).toBe(480)
  })

  it('clamps above the top rung rather than answering nothing', () => {
    // A 4× display wants 640 and there is no larger tile to offer it —
    // unlike a page, where `pageSize.ts` can fall back to the original bytes.
    expect(requestedTileEdge(160, 4)).toBe(480)
    expect(requestedTileEdge(1000, 3)).toBe(480)
  })

  it('falls to the smallest rung on any input that is not a positive number', () => {
    // The opposite direction from `pageSize.ts`'s `requestedLongEdge`, which
    // falls toward the original: soft is the failure mode for a page, and 237
    // tiles at 480 px is the failure mode here.
    for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(requestedTileEdge(bad, 3)).toBe(160)
      expect(requestedTileEdge(96, bad)).toBe(160)
    }
  })
})

describe('stripTileEdge', () => {
  const view = (matches: boolean, devicePixelRatio: number) => ({
    matchMedia: (query: string) => {
      expect(query).toBe(`(width < ${STRIP_BREAKPOINT_PX}px)`)
      return { matches }
    },
    devicePixelRatio,
  })

  it('asks for the narrow tile below the breakpoint and the wide one above', () => {
    expect(stripTileEdge(view(true, 3))).toBe(
      requestedTileEdge(STRIP_TILE_PX.narrow, 3),
    )
    expect(stripTileEdge(view(false, 3))).toBe(
      requestedTileEdge(STRIP_TILE_PX.wide, 3),
    )
    // And those two are different, or this test would pass against a constant.
    expect(stripTileEdge(view(true, 3))).not.toBe(stripTileEdge(view(false, 3)))
  })

  it('treats a view without matchMedia as the wide one', () => {
    // jsdom has no `matchMedia` at all — probed 2026-09-21, it is `undefined`
    // rather than a stub — so this is the branch every component test in that
    // project takes, not a hypothetical.
    expect(stripTileEdge({ devicePixelRatio: 1 })).toBe(160)
    expect(stripTileEdge(undefined)).toBe(160)
  })

  it('treats a missing devicePixelRatio as 1', () => {
    expect(stripTileEdge({ matchMedia: () => ({ matches: false }) })).toBe(160)
  })

  it('offers the three rungs the server holds', () => {
    // `cover.rs`'s `TILE_LONG_EDGES` is the same three. A rung added here
    // alone is a `w` the server answers with the 160 px tile.
    expect(TILE_LADDER).toEqual([160, 320, 480])
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project node --reporter=default tileSize
```

Expected: FAIL — `Failed to resolve import "./tileSize"`.

- [ ] **Step 3: Write the module**

Create `web/src/tileSize.ts`:

```ts
/**
 * How large a filmstrip tile to ask the server for — O134.
 *
 * The strip's tiles were one size, `cover.rs`'s `THUMB_LONG_EDGE` of 160 px,
 * whatever screen drew them: 1.8× upscaled on the 3× phone the row was opened
 * from, where the tile is drawn 96 CSS px tall. The server now makes a tile at
 * any rung of `TILE_LADDER` (`cover.rs`'s `TILE_LONG_EDGES` — the same three),
 * and this is the client half: which rung a given screen should ask for.
 *
 * **The same shape as `pageSize.ts`, with two rules deliberately inverted.**
 * Read that module first; the differences are marked below and each one is
 * about a tile being small and numerous where a page is large and single.
 */

/**
 * The long edges the server will make a tile at.
 *
 * Three rungs cover every case: a tile is drawn 96 or 160 CSS px tall and a
 * screen is 1×, 2× or 3×, so the pixels wanted are 96, 160, 192, 288, 320 or
 * 480. **Held identically in `cover.rs`'s `TILE_LONG_EDGES`**; a rung added
 * here alone is a `w` the server does not recognise and answers with the
 * 160 px tile, which is soft rather than broken and so invisible without a
 * test that decodes the response.
 */
export const TILE_LADDER: readonly number[] = [160, 320, 480]

/**
 * The viewport width below which the strip's tile is the shorter one, and the
 * two heights it takes — `styles/reader.css`'s `--strip-tile`.
 *
 * **Restated here rather than read from the stylesheet**, because the answer
 * is needed during the strip's first render: reading it back from
 * `getComputedStyle` needs the rail to be in the document, which is one commit
 * too late — every tile would fetch at 160 and then again at its real rung.
 * `styles/reader.css.test.ts` pins the stylesheet against these two constants
 * so the restatement cannot drift, which is the same trade `pageSize.ts` does
 * not have to make because a page's box is measured rather than declared.
 */
export const STRIP_BREAKPOINT_PX = 900
export const STRIP_TILE_PX = { wide: 160, narrow: 96 } as const

/**
 * The rung to ask for, for a tile drawn `tileHeightCss` CSS pixels tall on a
 * display of `dpr`.
 *
 * Rounding **up**, never down, exactly as `requestedLongEdge` does: the cost
 * of a rung too far is bytes, and the cost of a rung too short is the blur
 * this row exists to remove.
 *
 * **Above the top rung it clamps rather than answering `undefined`.** A page
 * has an original to fall back to and a tile does not, so a 4× display's 640
 * is served the 480 that exists.
 *
 * **Any input that is not a positive finite number falls to the *smallest*
 * rung** — the opposite direction from `requestedLongEdge`, which falls to the
 * original. A wrong size is invisible either way, so each falls toward its own
 * cheap failure: a soft page is unreadable, where a soft tile is a thumbnail
 * nobody reads, and there are 237 of them.
 */
export function requestedTileEdge(tileHeightCss: number, dpr: number): number {
  const usable = (value: number) => Number.isFinite(value) && value > 0
  if (!usable(tileHeightCss) || !usable(dpr)) return TILE_LADDER[0]
  const wanted = tileHeightCss * dpr
  return (
    TILE_LADDER.find((rung) => rung >= wanted) ??
    TILE_LADDER[TILE_LADDER.length - 1]
  )
}

/** Just enough of a `Window` to choose a rung, so a test can hand over a fake. */
export interface TileView {
  matchMedia?: (query: string) => { matches: boolean }
  devicePixelRatio?: number
}

/**
 * The rung the filmstrip should ask for in `view`.
 *
 * **`matchMedia` rather than the drawn height**, for the reason
 * `STRIP_TILE_PX` gives: this is called during the first render, before there
 * is an element to measure. Optional because jsdom does not implement
 * `matchMedia` at all — probed on 2026-09-21: it is `undefined`, not a stub
 * that answers `false` — so every component test in that project takes the
 * missing branch, and it has to mean "the wide tile" rather than throw.
 */
export function stripTileEdge(view: TileView | undefined): number {
  const query = view?.matchMedia?.(`(width < ${STRIP_BREAKPOINT_PX}px)`)
  const height = query?.matches === true ? STRIP_TILE_PX.narrow : STRIP_TILE_PX.wide
  return requestedTileEdge(height, view?.devicePixelRatio ?? 1)
}
```

- [ ] **Step 4: Run them again**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project node --reporter=default tileSize
```

Expected: PASS, 8 tests.

- [ ] **Step 5: Pin the stylesheet against the constants**

In `web/src/styles/reader.css.test.ts`, import the constants and build the
existing case's expectations from them, so a change to either side fails here:

```ts
import { STRIP_BREAKPOINT_PX, STRIP_TILE_PX } from '../tileSize'
```

```ts
  it('stands the strip’s tiles at 160 px and 96 below the breakpoint, with a matching starting --strip-unit', () => {
    // Built from `tileSize.ts`'s constants rather than written out again:
    // the filmstrip asks the server for a rung derived from those two numbers
    // during its first render, before there is an element to measure, so this
    // assertion is what keeps the restatement honest — O134.
    expect(declared(readerSource, '.reader', '--strip-tile')).toEqual([
      `${STRIP_TILE_PX.wide}px`,
      `${STRIP_TILE_PX.narrow}px`,
    ])
    expect(declared(readerSource, '.reader', '--strip-unit')).toEqual([
      '97px',
      '54px',
    ])
    // Anchored to the phone breakpoint's own `.reader` block, not a search of
    // the whole file — but not exclusive: `--strip-unit` lives in the same
    // block (O140), and this must not regress the moment it does.
    expect(withoutComments(readerSource)).toMatch(
      new RegExp(
        `@media \\(width < ${STRIP_BREAKPOINT_PX}px\\)\\s*\\{\\s*\\.reader\\s*\\{[^}]*--strip-tile:\\s*${STRIP_TILE_PX.narrow}px;`,
      ),
    )
  })
```

- [ ] **Step 6: Run the whole node project**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project node --reporter=default
```

Expected: PASS.

- [ ] **Step 7: Watch the drift guard fail against its own defect**

In `web/src/tileSize.ts` change `STRIP_TILE_PX` to `{ wide: 160, narrow: 100 }`.
Run the node project: `reader.css.test.ts`'s case must fail — on both the
`declared` assertion and the `@media` regex. Restore it. Then change
`STRIP_BREAKPOINT_PX` to `901`, run again, watch the regex fail alone, and
restore.

- [ ] **Step 8: Commit**

```bash
cd /home/davey/projects/taiyaku
git add web/src/tileSize.ts web/src/tileSize.test.ts web/src/styles/reader.css.test.ts
git commit -m "O134: the tile ladder, pinned against the stylesheet"
```

---

### Task 5: `thumbUrl` carries a long edge, and the strip asks for its rung

**Files:**
- Modify: `web/src/api.ts` — `thumbUrl`
- Modify: `web/src/api.test.ts`
- Modify: `web/src/Filmstrip.tsx` — the tile `<img>` and a new mount-time value
- Modify: `web/src/Filmstrip.test.tsx` — the existing
  `draws one thumbnail per group at the page thumbUrl, alt 1-based` case

**Interfaces:**
- Consumes: `stripTileEdge`, `requestedTileEdge` from Task 4.
- Produces: `thumbUrl(work: string, edition: string, index: number, longEdge?: number): string`.

- [ ] **Step 1: Write the failing tests**

In `web/src/api.test.ts`, beside the existing `thumbUrl` expectation:

```ts
  it('asks for a tile at a long edge when one is given, and the plain tile otherwise', () => {
    expect(thumbUrl('w', 'raw', 3)).toBe('/api/works/w/raw/pages/3/thumb')
    expect(thumbUrl('w', 'raw', 3, 320)).toBe(
      '/api/works/w/raw/pages/3/thumb?w=320',
    )
    // The query goes after `/thumb`, not on the page path underneath it:
    // `pageUrl`'s own `?w=` would land in the middle of the route and the
    // server would see no `w` at all.
    expect(thumbUrl('w', 'raw', 3, 320)).not.toContain('?w=320/thumb')
  })
```

In `web/src/Filmstrip.test.tsx`, update the existing case so it asserts the
rung **per element** rather than over the page — a whole-page `toContain`
cannot see which tile got which URL:

```ts
    // jsdom has no `matchMedia`, so `stripTileEdge` takes its wide branch at
    // `devicePixelRatio` 1 and every tile asks for 160 — today's image at an
    // explicit size. The rung that varies is the browser project's to assert
    // (`Filmstrip.browser.test.tsx`); what this pins is that the URL carries
    // one at all, on every tile, and that it is the same one.
    const images = [...host.querySelectorAll('img')]
    expect(images).toHaveLength(groups.length)
    for (const [i, img] of images.entries()) {
      expect(img.getAttribute('src')).toBe(
        `/api/works/w/e/pages/${i}/thumb?w=160`,
      )
    }
```

Match the existing test's own variable names (`host`, `groups`) when you get
there; read the case before rewriting it.

- [ ] **Step 2: Run them and watch them fail**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project node --reporter=default api
npx vitest run --project jsdom --reporter=default Filmstrip
```

Expected: both FAIL — the first on `expected '/api/works/w/raw/pages/3/thumb'
to be '/api/works/w/raw/pages/3/thumb?w=320'`, the second on a `src` without
`?w=160`.

- [ ] **Step 3: Find every other test that asserts a thumb URL**

```bash
cd /home/davey/projects/taiyaku/web/src
grep -rn "/thumb" --include='*.test.ts' --include='*.test.tsx' .
```

Checked on 2026-09-21, the hits divide three ways and the division is the
point — do not update one that did not move:

- **Moves with this task:** `Filmstrip.test.tsx:73`.
- **Moves with Task 7:** `ProposalRow.test.tsx:321-326`, six exact URLs with no
  `?w=`.
- **Stays exactly as it is:** `api.test.ts:43` (calls `thumbUrl` with three
  arguments, which is still the bare tile), `api.test.ts:62`, `Series.test.tsx`
  and `Library.test.tsx` (covers at `?w=256`, a different route parameter this
  branch does not touch), and `App.test.tsx:439,1352`, which filter on
  `.includes('/thumb')` and keep matching once a query is appended.

If this grep returns a hit not in that list, it appeared after the plan was
written — read it before assuming which group it belongs to.

- [ ] **Step 4: Widen `thumbUrl`**

In `web/src/api.ts`:

```ts
/**
 * A filmstrip tile for one page, at `longEdge` if one is given — O134.
 *
 * **The query goes after `/thumb`, and cannot come from `pageUrl`.** That
 * helper appends its own `?w=` to the page path, which this route hangs
 * `/thumb` off: reusing it would build `…/pages/3?w=320/thumb`, a path no
 * route matches. The two sizes are deliberately different parameters of
 * different routes — a page's `w` is what the reader draws, a tile's is one
 * of `tileSize.ts`'s three rungs.
 *
 * Without one it is the 160 px tile, which is what `library.ts`'s `workCover`
 * still asks for and what an account that may not stream is given whatever it
 * asks for.
 */
export const thumbUrl = (
  work: string,
  edition: string,
  index: number,
  longEdge?: number,
) => {
  const path = `${pageUrl(work, edition, index)}/thumb`
  return longEdge === undefined ? path : `${path}?w=${longEdge}`
}
```

- [ ] **Step 5: Have the strip ask for its rung**

In `web/src/Filmstrip.tsx`, import and compute once:

```ts
import { stripTileEdge } from './tileSize'
```

Inside the component, beside the other refs and state:

```ts
  // O134. The rung every tile in this strip asks the server for, read once at
  // mount.
  //
  // **Lazy state rather than an effect**, so the very first render already
  // carries it: setting it afterwards would mean every visible tile fetched at
  // 160 px and then again at its real rung, which is the whole cost this row
  // is trying to remove doubled.
  //
  // **Once, and not on resize.** `devicePixelRatio` changes when a window
  // moves between monitors and the breakpoint changes when one is dragged
  // across 900 px; re-reading either would refetch every tile in the window to
  // sharpen a strip nobody is looking at mid-drag. The strip mounts when the
  // chrome is first shown and stays mounted (O140), so this is once per
  // reading session.
  const [tileEdge] = useState(() =>
    stripTileEdge(typeof window === 'undefined' ? undefined : window),
  )
```

and pass it at the `<img>`:

```tsx
                  src={thumbUrl(work, edition, page, tileEdge)}
```

Leave the comment under that line as it is — `page` is still the zero-based
index for the same reason.

- [ ] **Step 6: Run them again**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project node --reporter=default
npx vitest run --project jsdom --reporter=default
```

Expected: PASS for both whole projects.

- [ ] **Step 7: Watch the tests fail against their own defects**

1. In `Filmstrip.tsx`, drop the fourth argument: `thumbUrl(work, edition, page)`.
   The jsdom case must fail on the missing `?w=160`. Restore.
2. In `api.ts`, build the URL as `${pageUrl(work, edition, index, longEdge)}/thumb`.
   The `api.test.ts` case must fail on `…/pages/3?w=320/thumb`. Restore.

- [ ] **Step 8: Commit**

```bash
cd /home/davey/projects/taiyaku
git add web/src/api.ts web/src/api.test.ts web/src/Filmstrip.tsx web/src/Filmstrip.test.tsx
git commit -m "O134: the strip asks for a tile its screen can use"
```

---

### Task 6: The browser test — the rung follows the real stylesheet

jsdom cannot reach this: it has no `matchMedia` and resolves no media query, so
every assertion about the narrow branch there would pass against a hardcoded
160.

**Files:**
- Modify: `web/src/Filmstrip.browser.test.tsx`

**Interfaces:**
- Consumes: everything from Tasks 4 and 5.

- [ ] **Step 1: Write the failing test**

In `web/src/Filmstrip.browser.test.tsx`, at the end:

```tsx
  /**
   * O134: what the strip asks the server for follows the stylesheet, at both
   * of its breakpoints, on the real `matchMedia` and the real
   * `devicePixelRatio`.
   *
   * **Mounted fresh at each viewport**, because the rung is read once at mount
   * by design (`Filmstrip.tsx`'s `tileEdge`) — resizing a mounted strip is
   * deliberately not expected to move it.
   *
   * **What this cannot see at `devicePixelRatio` 1**: both branches round up
   * to the same 160, so the two viewports agreeing proves the wiring, not the
   * ladder. The ladder's own discrimination is `tileSize.test.ts`'s table,
   * where the density is an argument. What is checked here and nowhere else is
   * that the height fed to it is the one the stylesheet actually drew.
   */
  it('asks for the rung its own drawn tile height calls for, at both breakpoints', async () => {
    for (const [width, height] of [
      [1440, 900],
      [390, 844],
    ] as const) {
      await page.viewport(width, height)
      const host = inReader(<Filmstrip {...props()} />)
      const rail = railOf(host)
      const drawn = Number.parseFloat(
        getComputedStyle(rail).getPropertyValue('--strip-tile'),
      )
      expect(drawn).toBe(width < STRIP_BREAKPOINT_PX ? STRIP_TILE_PX.narrow : STRIP_TILE_PX.wide)

      const expected = requestedTileEdge(drawn, window.devicePixelRatio)
      const images = [...rail.querySelectorAll('img')]
      expect(images.length).toBeGreaterThan(0)
      for (const img of images) {
        expect(img.getAttribute('src')).toContain(`/thumb?w=${expected}`)
      }
    }
  })
```

Add the imports this needs at the top of the file:

```tsx
import {
  requestedTileEdge,
  STRIP_BREAKPOINT_PX,
  STRIP_TILE_PX,
} from './tileSize'
```

`inReader`, `railOf`, `props` and `page` already exist in this file. If
`inReader` does not clean up between mounts, follow whatever the neighbouring
cases do about a second mount in one test — read them first.

- [ ] **Step 2: Run it**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project browser --reporter=default Filmstrip
```

Expected: PASS at this point, since Task 5 already shipped the behaviour. That
makes step 3 the only evidence this test is worth anything.

- [ ] **Step 3: Watch it fail against the defect it names**

Hand-apply each, run, restore:

1. In `Filmstrip.tsx`, replace `stripTileEdge(window)` with a literal `160`.
   Expected: the 390×844 pass fails **only if this browser's
   `devicePixelRatio` is above 1**. Record in the test run what
   `window.devicePixelRatio` actually is. **If it is 1, this mutation will not
   fail the test** — then instead hand-apply the defect the test is really for:
   in `tileSize.ts` change `STRIP_TILE_PX.narrow` to `480` and confirm the
   390×844 case fails on `expect(drawn).toBe(...)`, which is the stylesheet
   coupling this file exists to check.
2. In `Filmstrip.tsx`, drop `?w=` entirely (`thumbUrl(work, edition, page)`).
   Expected: FAIL on `toContain('/thumb?w=160')` at both viewports.

Write down which mutation failed which assertion; it goes in the PR body as
what this test is and is not evidence for.

- [ ] **Step 4: Commit**

```bash
cd /home/davey/projects/taiyaku
git add web/src/Filmstrip.browser.test.tsx
git commit -m "O134: the browser says the rung follows the stylesheet"
```

---

### Task 7: The proposals inbox

**Files:**
- Modify: `web/src/ProposalRow.tsx` — the page tile `<img>`
- Modify: `web/src/ProposalRow.test.tsx` — one new case, and the six exact
  URLs at `ProposalRow.test.tsx:321-326`, which have no `?w=` and will fail the
  moment step 3 lands

**Interfaces:**
- Consumes: `requestedTileEdge` from Task 4, `thumbUrl` from Task 5.

- [ ] **Step 1: Write the failing test**

In `web/src/ProposalRow.test.tsx`, beside the existing tile assertions:

```ts
  it('asks for a page tile its own box can use', () => {
    // The box is fixed by `styles/admin.css`'s `.proposal-detail .pages img` —
    // 4.5rem wide at `aspect-ratio: 2 / 3`, so 108 px tall — which is why the
    // height here is a constant and not a measurement. jsdom reports
    // `devicePixelRatio` 1, so the rung is the smallest; the arithmetic that
    // matters on a 3× screen is `tileSize.test.ts`'s.
    const expected = requestedTileEdge(108, window.devicePixelRatio)
    const images = [...host.querySelectorAll('.pages img')]
    expect(images.length).toBeGreaterThan(0)
    for (const img of images) {
      expect(img.getAttribute('src')).toContain(`/thumb?w=${expected}`)
    }
  })
```

Read the file's existing cases first and reuse their fixture and their name for
the mounted host.

And append the rung to the six exact URLs already asserted around
`ProposalRow.test.tsx:321-326`
(`'/api/works/monster-colored-v01/en-color/pages/0/thumb'` and its five
neighbours). Build the suffix from `requestedTileEdge(108, window.devicePixelRatio)`
rather than writing `?w=160` out, so that list does not silently stop matching
the component on a machine with a different density.

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project jsdom --reporter=default ProposalRow
```

Expected: FAIL — the `src` has no `?w=`.

- [ ] **Step 3: Ask for the rung**

In `web/src/ProposalRow.tsx`:

```tsx
import { requestedTileEdge } from './tileSize'

/**
 * The height an inbox page tile is drawn at, from
 * `styles/admin.css`'s `.proposal-detail .pages img`: `width: 4.5rem` at
 * `aspect-ratio: 2 / 3`, so 72×108 CSS px. A constant rather than a
 * measurement because the box is fixed on purpose — three tiles of different
 * widths would make the two sides of a proposal impossible to read across,
 * which is the one thing the row exists for.
 *
 * At 3× this box wants 324 px and was being sent 160 (O134, closing O133's
 * "the proposals inbox's page tiles keep the 160 px tile").
 */
const TILE_HEIGHT_PX = 108
```

and at the `<img>`:

```tsx
        src={thumbUrl(
          volume.work_slug,
          volume.edition_key,
          index,
          requestedTileEdge(TILE_HEIGHT_PX, window.devicePixelRatio),
        )}
```

- [ ] **Step 4: Run the jsdom project**

```bash
cd /home/davey/projects/taiyaku/web
npx vitest run --project jsdom --reporter=default
```

Expected: PASS.

- [ ] **Step 5: Watch it fail against its own defect**

Change `TILE_HEIGHT_PX` to `1` and run: the case must fail, because
`requestedTileEdge(1, 1)` is 160 while the test computes it from 108 — **it
will not**, since both are 160 at `devicePixelRatio` 1. So instead hand-apply
the defect that is visible here: drop the fourth argument to `thumbUrl`, run,
and watch it fail on the missing `?w=`. Record that the height constant itself
is **not** covered in jsdom, and say so in the PR body.

- [ ] **Step 6: Commit**

```bash
cd /home/davey/projects/taiyaku
git add web/src/ProposalRow.tsx web/src/ProposalRow.test.tsx
git commit -m "O134: the inbox's page tiles, sized for their box"
```

---

### Task 8: Measure it, then write it down

Nothing here is optional: `docs/status.md`'s preamble promises every claim in it
traces to a measurement in `docs/notes.md`, and this row's whole subject is a
cache whose size O11 records.

**Files:**
- Modify: `docs/notes.md` — a new "O134" section
- Modify: `docs/status.md` — the five-part edit, plus O11's row

- [ ] **Step 1: Run the gate**

```bash
cd /home/davey/projects/taiyaku
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh
```

Expected: the `all green` line. Quote it verbatim when reporting; do not pipe
this through `tail`, which masks the exit status.

- [ ] **Step 2: Measure the cache**

Serve the scratch library, fetch one edition's tiles at each rung, and measure
the cache directory:

```bash
cd /home/davey/projects/taiyaku
cargo build --release
# Serve the scratch copy on a spare port, with a cache directory of its own so
# the figures are this measurement's and nothing else's.
```

Drive one edition's every page at each of 160, 320 and 480 — a loop over
`/api/works/<slug>/<edition>/pages/<n>/thumb?w=<edge>` — then report, per rung:
`du -sh` of the thumb cache, the file count, the mean bytes per tile, and the
wall-clock of the cold pass. Use the same edition O140 measured
(`akame-ga-kill----v01--2015--cbz`, 237 pages) so the numbers sit beside its.
Write the numbers into `docs/notes.md` under a new `## O134` heading, with the
date, the machine, the edition, and the command.

The spec's extrapolation — about 3.6 MiB at 320 and 7.2 MiB at 480 per
195-page edition against 1.12 at 160 — is a guess from two of O133's numbers.
**Replace it with what you measured; do not reconcile the measurement to it.**

- [ ] **Step 3: Measure the phone**

Serve the production build over the tailnet, open the reader on the user's
iPhone, and record: the `w` in the thumbnail requests the server logs, and the
user's own answer to whether the strip is sharp now. This is the only evidence
the row's actual complaint is fixed — everything else in this branch is
arithmetic. Read the user-agent in the log before believing a result, and note
that the log's byte count is 0 for a download.

- [ ] **Step 4: The five-part edit**

In `docs/status.md`:

1. O134's checkbox → `[x]`.
2. Its prose, rewritten to say what was done: the ladder, the `can_stream`
   line and its fallback, the measured cache cost, what the browser test is and
   is not evidence for, and what remains (nothing evicts the cache; the credits
   panel still takes the 160 px tile at 1.05× and needs nothing).
3. The test counts — **both** sites, "Five crates, N tests" and "Nine of the N
   are standing checks", plus the web clause. The web figures are not gated by
   anything, so read them off real runs:
   `npx vitest run --project node` and `--project jsdom` and `--project browser`.
4. The "N are now closed — leaving M" sentence.
5. The "What remains … M open items" line — its count word **wraps across a
   line break**, so `grep -n -A1 '^What remains:'` and read the next line.

Then O11's row: its annotation says "filmstrip tiles at 1.12 MiB per 195-page
edition", which this branch makes incomplete. Add what a tile costs at each
rung and that a library read on a dense screen now holds more than one size per
page. O11's trigger is the filmstrip's thumbnail-per-page cache growing; this
is that, so re-read the row rather than only re-numbering it.

- [ ] **Step 5: Sweep for what the five parts miss**

```bash
cd /home/davey/projects/taiyaku
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -rn "O134" docs/ web/src/ crates/ --include='*.md' --include='*.ts' --include='*.tsx' --include='*.rs' | grep -v '/target/'
grep -rn "160 px tile\|THUMB_LONG_EDGE\|160 px thumbnail" docs/ web/src/ --include='*.md' --include='*.ts' --include='*.tsx'
```

Read **every** hit of the last two, not only the ones matching a phrase you
expected. Prose outside the table describes open items in wording no grep
predicts; closed rows are left as written, and a live doc comment is not.

Check every table row you touched has exactly six `|` characters — a `|` inside
backticks still ends a cell.

- [ ] **Step 6: Run the gate again, then sweep mutations**

```bash
cd /home/davey/projects/taiyaku
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh
scripts/mutants.sh --since main
```

`mutants.sh` diffs commits, not the worktree, so commit first. Read the
**unviable** count against what you changed, and remember what this harness
cannot see: it generates no mutant for a string literal, a struct field
initialiser or a `json!` key, so a clean run over data plumbing says nothing.
Neither a survivor list nor a clean run is a verdict on its own; an *errored*
run's verdict is fiction in both directions.

- [ ] **Step 7: Commit**

```bash
cd /home/davey/projects/taiyaku
git add docs/notes.md docs/status.md
git commit -m "O134: measured, and written down"
```

---

## Before opening the PR

- **Whole-branch review, not per-task.** Per-task review has passed on every
  task of branches that a whole-branch pass then found seven Criticals in.
  Review the fix rounds too — on #106 that found three more, one of them a
  regression the fix round introduced.
- **Then a separate pre-merge claim check**: every factual claim in the row,
  the notes and the PR body, checked against the code and the measurements.
  On #167 this found 15 findings and then 11, four of them inside the first
  round's own fixes. Prefer deleting a figure to correcting it a fourth time.
- **The PR goes to the gitea forge, not GitHub** — `gh` is authed against the
  wrong host. Never hard-wrap the PR body; it becomes the squash commit message
  verbatim.

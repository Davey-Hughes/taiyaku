# The library download's second half — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship spec C2's three deferred pieces — an OPDS-namespace download route, an acquisition link that points at it, and a download control on the series page — so that offline reading in Panels, C2's stated purpose, is actually reachable from a device.

**Architecture:** No new handler and no new stream. `download::edition` is registered a second time under `/opds/v1.2/…` so `auth::refuse`'s `WWW-Authenticate` prompt fires for an OPDS client; `opds::work_feed` repoints its acquisition link at that route, asking `download` itself what type the route would send; and `Series.tsx` gains a plain `<a download>` in each layout, conditional on `Me.can_download`, which the gate context has held since A2.

**Tech Stack:** Rust 2024 (rust-version 1.85), axum, `taiyaku-serve`; React 19 + TypeScript + vitest in `web/`.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-25-library-download-second-half-design.md`, an addendum to `2026-08-23-library-download-design.md` §§7–9. Where the two disagree, the addendum wins.
- Branch: `library-download-second-half`. Already created; the spec is committed as `b88777c`.
- **The remote is squash-only.** One branch becomes exactly one commit on `main`. Keep the branch to one checkable claim.
- The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`. **Never run it with `TAIYAKU_CORPUS` set** — it exports the variable into an unfiltered `cargo test --workspace`, which runs `geometry_experiment` in the debug profile (40+ minutes; one run was killed at 1h09m).
- The corpus job runs separately: `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1` (~85 s).
- Verify the gate by quoting its own final line. Piping `check-all.sh` through `tail`, or appending `; echo`, masks a non-zero exit.
- `docs/status.md` is authoritative for project state; measurements go in `docs/notes.md`, never in `status.md`.
- Do not add attribution trailers to commit messages.

---

## Task 0: The O55 spike — a gate, not a commit

**This is not a code task.** It produces no commit and must be settled before Task 1.

O55 is open: no OPDS client has ever been shown to accept a `tyk_` API key in the HTTP Basic password field. C2's purpose is offline reading in Panels, so if Panels will not authenticate, repointing the acquisition link proves nothing — the feed is unreachable before the link is ever read.

- [ ] **Step 1: Confirm the spike result with the owner**

The owner runs it: Panels on the iPad → Library → **+** → Add OPDS/Comic Server → `http://<LAN-IP>:8080/opds/v1.2/catalog`, username `local`, password the `tyk_…` key.

A server for it, if one is not already running:

```bash
cargo build --release --bin taiyaku
./target/release/taiyaku library key create local --name ipad-o55 --db target/o64-check/library.db
./target/release/taiyaku serve --db target/o64-check/library.db --addr 0.0.0.0:8080 \
  --log-requests "$PWD/panels-o55.jsonl"
```

`panels-o55.jsonl` is gitignored working evidence, local-only. **Check the `user-agent` in it before believing any result** — a browser fetch of the same URL proves the server half, which `curl` already proved, and says nothing about Panels.

- [ ] **Step 2: Branch on the outcome**

- **Panels lists the catalog** → O55 closes as verified. Continue to Task 1.
- **Panels refuses the key** → **stop.** O55's own entry names the fallback: a password-shaped per-device credential, with the auth spec's §2 table amended rather than the `tyk_` prefix removed. That is a separate spec and a separate branch, and it comes first. Do not build Tasks 1–7 against a feed no device can reach.

---

## Task 1: The OPDS download route

**Files:**
- Modify: `crates/taiyaku-serve/src/opds.rs` — `routes()`
- Modify: `crates/taiyaku-serve/src/routes.rs` — `grant_postures()`
- Modify: `crates/taiyaku-serve/src/auth/tests.rs` — the two count assertions in `every_route_declares_its_grant_posture`
- Test: `crates/taiyaku-serve/src/opds/tests.rs`

**Interfaces:**
- Consumes: `crate::download::edition` — `pub(crate) async fn edition(State(state): State<Arc<ServeState>>, AxPath((slug, edition_key)): AxPath<(String, String)>, headers: HeaderMap, extensions: axum::http::Extensions) -> Response` (`download.rs:458`). Its path parameters bind **positionally**, so `{work}`/`{edition}` in the OPDS namespace work unchanged.
- Produces: the route pattern string `"/opds/v1.2/works/{work}/{edition}/download"`, which Task 3 builds hrefs for.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/opds/tests.rs`:

```rust
/// The mirror exists for `auth::refuse`'s sake, not for symmetry: a 401 in
/// this namespace carries `WWW-Authenticate` and the `/api` twin's
/// deliberately does not, so an OPDS client following an acquisition link
/// into `/api` cold would get a bare 401 and never prompt. That is the
/// property this route is *for*, so it is asserted about this route rather
/// than assumed to follow from registration.
#[tokio::test]
async fn the_opds_download_401s_with_a_prompt_where_the_api_twin_does_not() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-dl-401"));
    let router = crate::build_router(state);

    let opds = router
        .clone()
        .oneshot(
            Request::get("/opds/v1.2/works/t/raw/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(opds.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        opds.headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"taiyaku\""),
        "without this header Panels never prompts, which is the whole reason \
         this registration exists"
    );

    let api = router
        .oneshot(
            Request::get("/api/works/t/raw/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
    assert!(
        api.headers().get("www-authenticate").is_none(),
        "an /api 401 carrying WWW-Authenticate raises the browser's own dialog"
    );

    crate::test_support::cleanup("taiyaku-test-opds-dl-401");
}

/// "One handler, two registrations" is a claim, so it is checked rather than
/// hoped for: the same edition through both paths is the same bytes.
#[tokio::test]
async fn the_opds_download_serves_the_same_bytes_as_the_api_route() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-dl-same"));
    let router = crate::test_support::authenticating(crate::build_router(state));

    let mut bodies = Vec::new();
    for path in [
        "/api/works/t/raw/download",
        "/opds/v1.2/works/t/raw/download",
    ] {
        let response = crate::test_support::get(&router, path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        bodies.push(
            axum::body::to_bytes(response.into_body(), 64 << 20)
                .await
                .expect("body"),
        );
    }

    crate::test_support::cleanup("taiyaku-test-opds-dl-same");
    assert_eq!(
        bodies[0], bodies[1],
        "the OPDS registration must be the same handler, not a second \
         implementation"
    );
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p taiyaku-serve opds::tests::the_opds_download -- --nocapture
```

Expected: both FAIL. The 401 test fails on status `404` (axum has no such route, and `route_layer` leaves a nonexistent path answering 404 rather than 401). The bytes test fails on the OPDS fetch's status.

- [ ] **Step 3: Register the route**

In `crates/taiyaku-serve/src/opds.rs`, in `pub fn routes()`, add a fourth entry after the cover:

```rust
        ("/opds/v1.2/works/{work}/{edition}/cover", get(cover)),
        // **The same handler as `/api/works/{work}/{edition}/download`**,
        // registered a second time rather than reimplemented — `edition`
        // binds its path parameters positionally, so `{work}`/`{edition}`
        // here are the `{slug}`/`{edition}` it expects.
        //
        // It exists in this namespace because of `auth::refuse`, not for
        // symmetry: a 401 on `/opds` carries `WWW-Authenticate` and a 401 on
        // `/api` deliberately does not, so a client following an acquisition
        // link into `/api` cold would never prompt for credentials.
        // `stream_page` above duplicates the page route for exactly this
        // reason. There is no member download here: OPDS asks for a
        // publication, and a chapter file inside one is not something a feed
        // has an entry for.
        (
            "/opds/v1.2/works/{work}/{edition}/download",
            get(crate::download::edition),
        ),
```

- [ ] **Step 4: Declare its grant posture**

In `crates/taiyaku-serve/src/routes.rs`, in `grant_postures()`, add after the OPDS cover row:

```rust
        ("/opds/v1.2/works/{work}/{edition}/cover", Refuses),
        // The same handler as the `/api` download behind the same
        // `refuse_if_hidden`, so necessarily the same posture. Driven, not
        // declared: `grant_proof_concrete` substitutes `{work}`/`{edition}`
        // and the `Refuses` loop picks this up with no further work.
        ("/opds/v1.2/works/{work}/{edition}/download", Refuses),
```

- [ ] **Step 4b: Extend the anonymous-refusal list**

**There is a third pinned list, and it is a literal.** `auth::tests::every_route_refuses_an_anonymous_request` holds `expected`, a hand-written array of every guarded pattern (`auth/tests.rs:60`), asserted set-equal to what `guarded_routes` folds. Adding a route fails it with a full two-list diff. Add the new pattern as the array's last entry, after `"/opds/v1.2/works/{work}/{edition}/cover"`:

```rust
        "/opds/v1.2/works/{work}/{edition}/cover",
        "/opds/v1.2/works/{work}/{edition}/download",
    ];
```

That list is not redundant with `grant_postures`: this one asserts the route is *guarded at all*, and drives a real anonymous request at each pattern to prove it answers 401. The posture table asserts what the guard then does about the root grant. A route can satisfy either and fail the other.

- [ ] **Step 5: Move the two counts the enumeration pins**

In `crates/taiyaku-serve/src/auth/tests.rs`, in `every_route_declares_its_grant_posture`:

```rust
    assert_eq!(
        driven, 19,
        "19 of the 29 guarded routes must be classified Refuses, Filters or \
         RefusesAndFilters and driven by the proofs below — a route's \
         posture changed without this count moving to match, so a route \
         this suite believes it proves may no longer be one it does"
    );
    assert_eq!(
        table.len() - driven,
        10,
        "the remaining 10 guarded routes must be GrantIndependent or \
         KnownGap — see the `driven == 19` assertion just above, which this \
         one is the other half of"
    );
```

Only the `18` and the two message strings change. **The `10` does not move**: 29 routes minus 19 driven is 10, the same as 28 minus 18.

- [ ] **Step 6: Run the suite**

```bash
cargo test -p taiyaku-serve opds::tests::the_opds_download -- --nocapture
cargo test -p taiyaku-serve auth::tests::every_route -- --nocapture
```

Expected: all PASS. `every_route_declares_its_grant_posture` now drives the new route against both a wholly-hidden work and a work split across roots, because its `Refuses` loops read `grant_postures` itself.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/opds.rs crates/taiyaku-serve/src/routes.rs \
        crates/taiyaku-serve/src/auth/tests.rs crates/taiyaku-serve/src/opds/tests.rs
git commit -m "The OPDS-namespace download: one handler, a second registration

A 401 here carries WWW-Authenticate and the /api twin's deliberately does
not, so a client following an acquisition link into /api cold would never
prompt. Asserted about this route rather than assumed from registration,
and the two bodies are compared so \"one handler\" is checked rather than
hoped for. The posture row makes the enumeration drive it for free."
```

---

## Task 2: The shared content-type helper

**Files:**
- Modify: `crates/taiyaku-serve/src/download.rs` — `prepare` (the `single_file` block, around `download.rs:686`)
- Test: `crates/taiyaku-serve/src/download/tests.rs`

**Interfaces:**
- Consumes: `type_for(extension: &str) -> &'static str` and `pub(crate) const CBZ_TYPE: &str` (`download.rs:243`), both already in this module.
- Produces:
  - `fn passthrough_extension(paths: &[std::path::PathBuf]) -> Option<String>` — module-private.
  - `pub(crate) fn advertised_type(work: &crate::Work, edition: &taiyaku_core::Edition) -> &'static str` — **Task 3 calls this.**

**Why this exists:** the addendum's §2.2. §9 of the source spec gives the acquisition link a flat `type="application/vnd.comicbook+zip"`, and §7 of the same document makes that wrong — a passthrough download's type follows the bytes, so a single-member PDF edition is served `application/pdf`. A feed that advertises `comicbook+zip` for it tells §7's "lie told twice" a third time. The two callers share one decision so they cannot drift.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-serve/src/download/tests.rs`:

```rust
// --- The type the feed advertises is the type the route sends (addendum §2.2). ---

/// A generated edition is always a `STORE` zip, whatever its members are.
#[test]
fn a_multi_member_edition_advertises_the_comic_type() {
    let work = crate::test_support::work("download-advertised-generated");
    let sidecar = work.sidecar();
    let edition = sidecar.editions.get("raw").expect("fixture edition");
    assert_eq!(
        crate::download::advertised_type(&work, edition),
        crate::download::CBZ_TYPE
    );
    crate::test_support::cleanup("download-advertised-generated");
}

/// A single-member edition whose member is **not** a comic archive must not
/// be advertised as one — §7's whole argument, applied to the feed.
#[test]
fn a_single_pdf_member_advertises_the_pdf_type() {
    let fixture = crate::test_support::Fixture::new("download-advertised-pdf");
    let path = fixture.path().join("volume.pdf");
    std::fs::write(&path, b"%PDF-1.4\n").expect("writes the member");

    let mut sidecar = taiyaku_core::Sidecar::default();
    sidecar.work.title = "A Volume".into();
    sidecar.editions.insert(
        "raw".into(),
        taiyaku_core::Edition {
            label: "raw".into(),
            members: vec![taiyaku_core::Member {
                path: "volume.pdf".into(),
                sha256: String::new(),
                pages: 1,
            }],
            ..Default::default()
        },
    );
    let work = crate::Work::new(
        "t".into(),
        fixture.path().join("taiyaku.json"),
        sidecar.clone(),
    );
    let edition = sidecar.editions.get("raw").expect("just inserted");

    assert_eq!(
        crate::download::advertised_type(&work, edition),
        "application/pdf",
        "a single-member PDF edition is served application/pdf by the route, \
         so the feed must not advertise it as a comic archive"
    );
}

/// A member that is not on disk falls back rather than failing: one bad
/// edition must not 500 a whole work's feed.
#[test]
fn a_missing_member_falls_back_to_the_comic_type() {
    let fixture = crate::test_support::Fixture::new("download-advertised-missing");
    let mut sidecar = taiyaku_core::Sidecar::default();
    sidecar.work.title = "A Volume".into();
    sidecar.editions.insert(
        "raw".into(),
        taiyaku_core::Edition {
            label: "raw".into(),
            members: vec![taiyaku_core::Member {
                path: "gone.pdf".into(),
                sha256: String::new(),
                pages: 1,
            }],
            ..Default::default()
        },
    );
    let work = crate::Work::new(
        "t".into(),
        fixture.path().join("taiyaku.json"),
        sidecar.clone(),
    );
    let edition = sidecar.editions.get("raw").expect("just inserted");

    assert_eq!(
        crate::download::advertised_type(&work, edition),
        crate::download::CBZ_TYPE
    );
}
```

If `taiyaku_core::Sidecar`, `Edition` or `Member` do not implement `Default` where used above, construct them field-by-field from the shapes in `crates/taiyaku-core/src/sidecar.rs:202` (`Member { path, sha256, pages }`) and `:211` (`Edition { label, members, spreads, .. }`) rather than adding a `Default` impl for a test's convenience.

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p taiyaku-serve download::tests::a_multi_member_edition_advertises \
  download::tests::a_single_pdf_member download::tests::a_missing_member -- --nocapture
```

Expected: FAIL to compile — `cannot find function 'advertised_type' in module 'crate::download'`.

- [ ] **Step 3: Extract the decision and add the helper**

In `crates/taiyaku-serve/src/download.rs`, replace the opening of the passthrough block inside `prepare`:

```rust
    let single_file = paths.len() == 1 && paths[0].is_file();
    if single_file {
        let length = std::fs::metadata(&paths[0])
            .map_err(|e| crate::routes::not_found(format!("{}: {e}", paths[0].display())))?
            .len();
        let extension = paths[0]
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("cbz")
            .to_ascii_lowercase();
        let content_type = type_for(&extension);
```

with:

```rust
    if let Some(extension) = passthrough_extension(&paths) {
        let length = std::fs::metadata(&paths[0])
            .map_err(|e| crate::routes::not_found(format!("{}: {e}", paths[0].display())))?
            .len();
        let content_type = type_for(&extension);
```

The rest of the block — `download_name(…, &extension)`, the `Shape::Passthrough` return — is unchanged.

Then add both functions beside `type_for`:

```rust
/// The extension a **passthrough** download of these members would serve, or
/// `None` where the download is generated.
///
/// One member that is a file is served byte for byte, and §7's rule is that
/// its type and its extension follow the bytes rather than the route: a
/// single-member edition can be a PDF or an EPUB, and serving one as
/// [`CBZ_TYPE`] under a `.cbz` name would be a lie told twice. Everything
/// else is generated, and generated output is always a `STORE` zip.
///
/// Extracted so [`prepare`] and [`advertised_type`] cannot disagree. The
/// feed advertising one type while the route sends another is the same
/// defect in a third place, which is the reason `refuse_if_hidden` is reused
/// rather than restated two modules away.
fn passthrough_extension(paths: &[std::path::PathBuf]) -> Option<String> {
    (paths.len() == 1 && paths[0].is_file()).then(|| {
        paths[0]
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("cbz")
            .to_ascii_lowercase()
    })
}

/// What a download of this edition would answer with as its `Content-Type`.
///
/// For [`crate::opds::work_feed`]'s acquisition link, which has to declare
/// the type the route will actually send.
///
/// **Falls back to [`CBZ_TYPE`] rather than failing.** A member missing from
/// disk must not turn one bad edition into a 500 for a whole work's feed,
/// and an entry advertising the wrong type for material that is not there is
/// no worse than the page link it replaced, which would also 404.
///
/// The cost is one `metadata` call per edition in a feed that otherwise
/// touches no disk. Bounded — one work, its editions, metadata only, no
/// archive opened — and it is what buys the feed and the route being unable
/// to drift.
pub(crate) fn advertised_type(
    work: &crate::Work,
    edition: &taiyaku_core::Edition,
) -> &'static str {
    let root = work
        .path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let paths: Vec<std::path::PathBuf> =
        edition.members.iter().map(|m| root.join(&m.path)).collect();
    match passthrough_extension(&paths) {
        Some(extension) => type_for(&extension),
        None => CBZ_TYPE,
    }
}
```

- [ ] **Step 3b: Keep the gate green while the caller does not exist yet**

`advertised_type`'s only caller is Task 3, so the lib build reports it dead and `check-all.sh` fails on it. Use **`expect`, not `allow`** — the attribute must come off when Task 3 lands, which is what A2's `Principal.roots` used it for. And scope it, because this module's own tests already call it, so an unconditional `expect` is *itself* unfulfilled under `cfg(test)`:

```rust
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "opds::work_feed calls this in the next commit")
)]
```

**Task 3 must delete these four lines.** If it does not, the build fails on an unfulfilled expectation — which is the point.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p taiyaku-serve download::
```

**Do not filter on `advertised`** — the test names read `advertises`, and a filter that matches nothing reports `0 passed; N filtered out`, which scrolls past as success. Check the count moved: 45 download tests before, 48 after.

Expected: PASS, including the 43 download tests that already existed. **Zero output bytes may change** — this task only moves a decision, it does not alter what any route sends. If a byte-exhaustive reassembly check moves, stop: the extraction is wrong.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/download.rs crates/taiyaku-serve/src/download/tests.rs
git commit -m "One decision for what type a download sends, so the feed cannot drift from the route

Spec C2 §9 gives the acquisition link a flat comicbook+zip and §7 of the
same document makes that wrong: a passthrough download's type follows the
bytes, so a single-member PDF edition is served application/pdf. The feed
asks download.rs rather than restating the rule two modules away. A member
missing from disk falls back rather than 500ing a whole work's feed."
```

---

## Task 3: The acquisition link, and the reader who may not download

**Files:**
- Modify: `crates/taiyaku-serve/src/opds.rs` — `work_feed`'s doc comment (`opds.rs:222–242`), its principal read (`opds.rs:262–278`), and the entry `writeln!` (`opds.rs:309–324`)
- Test: `crates/taiyaku-serve/src/opds/tests.rs`

**Interfaces:**
- Consumes: `crate::download::advertised_type(work, edition) -> &'static str` from Task 2; `crate::auth::Principal`'s `roots` and `can_download` fields.
- Produces: nothing later tasks read. Task 6 rewrites the `status.md` sentence this makes false.

**Two decisions this task carries** (addendum §2.2 and §2.3):

1. The link's `type` is `advertised_type`'s answer, not a constant.
2. **A reader who may not download keeps the page link.** The UI hides its control; the feed cannot, because OPDS 1.2 requires at least one acquisition link on every entry — which is the very reason this link points at a page today. So the page link is not deleted, it becomes the fallback. An acquisition link that 403s on every attempt is worse than the page that has always been there.

**And one that is easy to get backwards:** an **absent** principal means "may download", the same reading the grant filter beside it gives an absent principal. `opds::tests::feed_router` builds this feed without the guard, so a missing capability must not silently become a withheld one.

- [ ] **Step 1: Write the failing tests**

Replace the existing `the_acquisition_link_on_an_edition_entry_is_a_real_fetchable_page` in `crates/taiyaku-serve/src/opds/tests.rs` with the three tests below. The old test asserted the link is a real page; that claim is what this task deliberately changes, so it is rewritten rather than left standing to contradict the code beside it.

```rust
/// OPDS 1.2 requires at least one acquisition link
/// (`rel="http://opds-spec.org/acquisition"`) on every entry in an
/// acquisition feed, or a conformant client may render the entry as
/// unopenable. It now points at a download — proven by fetching it, not by
/// asserting the string is present, so this fails if the link is ever dead.
#[tokio::test]
async fn the_acquisition_link_is_a_real_fetchable_download() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acq-dl"));
    let app = feed_router().with_state(state);

    let feed = app
        .clone()
        .oneshot(
            Request::get("/opds/v1.2/works/t")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let feed_body = body_of(feed).await;

    assert!(
        feed_body.contains(r#"rel="http://opds-spec.org/acquisition""#),
        "must declare the acquisition rel: {feed_body}"
    );

    let href = feed_body
        .split("href=\"")
        .skip(1)
        .find_map(|rest| {
            let (path, _) = rest.split_once('"')?;
            path.ends_with("/download").then(|| path.to_string())
        })
        .unwrap_or_else(|| panic!("no download link in feed: {feed_body}"));

    let download = app
        .oneshot(Request::get(&href).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = download.status();

    // The PSE link is a different mechanism for a different intention and is
    // untouched. Asserted with its `pse:count`, because a test that would
    // still pass with the link deleted is not evidence.
    assert!(
        feed_body.contains(r#"rel="http://vaemendis.net/opds-pse/stream""#)
            && feed_body.contains(r#"pse:count="4""#),
        "streaming stays the in-app read path: {feed_body}"
    );

    crate::test_support::cleanup("taiyaku-test-opds-acq-dl");
    assert_eq!(
        status,
        StatusCode::OK,
        "the acquisition link must be a real, fetchable download: {href}"
    );
}

/// §2.2 of the addendum: the feed declares the type the route will send, and
/// a generated edition is a `STORE` zip.
#[tokio::test]
async fn the_acquisition_link_declares_the_type_the_route_will_send() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acq-type"));
    let app = feed_router().with_state(state);

    let feed = app
        .oneshot(
            Request::get("/opds/v1.2/works/t")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let feed_body = body_of(feed).await;

    crate::test_support::cleanup("taiyaku-test-opds-acq-type");
    assert!(
        feed_body.contains(
            r#"<link rel="http://opds-spec.org/acquisition" type="application/vnd.comicbook+zip""#
        ),
        "a generated edition is a STORE zip and must say so: {feed_body}"
    );
    assert!(
        !feed_body.contains(r#"rel="http://opds-spec.org/acquisition" type="image/jpeg""#),
        "the acquisition link is no longer a page: {feed_body}"
    );
}

/// §2.3: the UI hides its control for a reader who may not download, and the
/// feed cannot — OPDS 1.2 requires an acquisition link on every entry, which
/// is why this one pointed at a page in the first place. So the page link
/// becomes the fallback rather than being deleted, and the entry still
/// carries exactly one acquisition link either way.
#[tokio::test]
async fn a_reader_who_may_not_download_keeps_the_page_link() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acq-403"));
    {
        let conn = state.library.conn();
        conn.execute("UPDATE user SET can_download = 0", [])
            .expect("withholds the capability");
    }
    let router = crate::test_support::authenticating(crate::build_router(state));

    let response = crate::test_support::get(&router, "/opds/v1.2/works/t").await;
    assert_eq!(response.status(), StatusCode::OK);
    let feed_body = crate::test_support::body_string(response).await;

    crate::test_support::cleanup("taiyaku-test-opds-acq-403");
    assert_eq!(
        feed_body.matches(r#"rel="http://opds-spec.org/acquisition""#).count(),
        1,
        "exactly one acquisition link per entry, whichever it is: {feed_body}"
    );
    assert!(
        feed_body.contains(r#"/pages/1"/>"#),
        "a reader who may not download must get the page link, not a link \
         that 403s on every attempt: {feed_body}"
    );
    assert!(
        !feed_body.contains(r#"/download"/>"#),
        "and must not be offered the download: {feed_body}"
    );
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -p taiyaku-serve opds::tests::the_acquisition_link opds::tests::a_reader_who_may_not -- --nocapture
```

Expected: all three FAIL. The first two panic on "no download link in feed" / the `image/jpeg` assertion; the third finds `/download` in the feed because nothing reads `can_download` yet.

- [ ] **Step 3: Read the capability alongside the grant**

In `crates/taiyaku-serve/src/opds.rs`, in `work_feed`, immediately **before** the existing `let visible_editions` binding, add:

```rust
    // **Absent means "may download"**, which is the same reading the grant
    // filter just below gives an absent principal: `opds::tests`' own
    // `feed_router` builds this feed without the guard, and a missing
    // capability must not silently become a withheld one.
    let may_download = extensions
        .get::<crate::auth::Principal>()
        .is_none_or(|principal| principal.can_download);
```

- [ ] **Step 4: Build the link per entry**

In the `for (key, edition) in &sidecar.editions` loop, after the existing `let pages = edition.total_pages();`, add:

```rust
        // **OPDS 1.2 requires at least one acquisition link on every entry**,
        // so a reader who may not download gets the page link rather than
        // none: removing it would produce exactly the unopenable entry that
        // requirement exists to prevent, and a download link that 403s on
        // every attempt is worse than the page that has always been here.
        //
        // The type is what the route will actually send, not a constant —
        // a single-member PDF edition is served `application/pdf`, and a
        // feed advertising it as a comic archive would tell §7's "lie told
        // twice" a third time.
        let acquisition = if may_download {
            format!(
                r#"<link rel="http://opds-spec.org/acquisition" type="{}"
          href="/opds/v1.2/works/{work_href}/{key_href}/download"/>"#,
                crate::download::advertised_type(work, edition)
            )
        } else {
            format!(
                r#"<link rel="http://opds-spec.org/acquisition" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/1"/>"#
            )
        };
```

Then in the `writeln!` below it, replace these two lines:

```rust
    <link rel="http://opds-spec.org/acquisition" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/1"/>
```

with:

```rust
    {acquisition}
```

- [ ] **Step 5: Rewrite the doc comment that now argues against the code**

Replace the deferral paragraph in `work_feed`'s doc comment (`opds.rs:222–242`, from `**The acquisition link still points at the edition's first page…` through `…offers it yet.`) with:

```rust
/// **The acquisition link points at a download.** OPDS 1.2 requires at least
/// one `rel="http://opds-spec.org/acquisition"` link on every entry in an
/// acquisition feed, or a conformant client may render the entry as
/// unopenable. This link used to point at the edition's first page because
/// this server had no whole publication to point at; spec C2 built one, and
/// its second half repointed this link at the OPDS-namespace mirror of that
/// route, so the `WWW-Authenticate` prompt an OPDS client needs still fires.
///
/// Its `type` is what the download route will actually answer with rather
/// than a constant — `download::advertised_type` decides, once, for the feed
/// and the route alike, because a single-member edition can be a PDF and
/// advertising it as a comic archive would be a lie the header would then
/// contradict.
///
/// **A reader whose `can_download` is withheld keeps the page link.** The
/// conformance requirement above admits no third option: an entry with no
/// acquisition link is the unopenable one, and an entry linking a download
/// that answers 403 on every attempt is worse than the page that has always
/// been here. The web UI's own control is *absent* in that case rather than
/// disabled (spec C2 §8), and the two are not inconsistent — a feed has a
/// conformance floor a screen does not.
///
/// The PSE link is untouched. Streaming stays the in-app read path and
/// acquisition became the offline one; they are two mechanisms for two
/// different intentions and always were, and the deviation existed only
/// because one of them had no implementation.
```

- [ ] **Step 6: Run the OPDS suite**

```bash
cargo test -p taiyaku-serve opds:: -- --nocapture
```

Expected: PASS, all of it. Watch for a break in `a_percent_encoded_edition_key_in_the_feed_routes_back_to_the_same_edition` — `key_href` is used in the new `format!` exactly as it was in the old markup, so it should hold; if it does not, the encoding was dropped in the move.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/opds.rs crates/taiyaku-serve/src/opds/tests.rs
git commit -m "The acquisition link points at a download, and a stale justification is rewritten

work_feed's comment recorded, as a deliberate decision, that the link
points at a page because taiyaku serves no whole publication. That stopped
being true when C2 shipped, so it is rewritten rather than left arguing
against the code beside it.

A reader whose can_download is withheld keeps the page link. OPDS 1.2
requires an acquisition link on every entry — the reason this one pointed
at a page in the first place — so it becomes the fallback rather than being
deleted. The web UI hides its control instead, and a feed has a conformance
floor a screen does not."
```

---

## Task 4: The download control on the grid tile

**Files:**
- Modify: `web/src/library.ts` — add `editionDownload` beside `workCover` (`library.ts:112`)
- Modify: `web/src/Series.tsx` — the import block (`:21–33`), and the tile's `<li>` after `</Link>` (`:423`)
- Test: `web/src/Series.test.tsx`

**Interfaces:**
- Consumes: `useGate()` from `web/src/gate.ts:46`, returning `Gated { who: Me; signOut: () => void } | undefined`; `Me.can_download: boolean` (`api.ts:105`).
- Produces: `export const editionDownload = (slug: string, edition: string) => string` — **Task 5 uses this too.**

**Three decisions this task carries:**

1. **A plain `<a download>`, never a `fetch`.** An anchor navigation carries the session cookie, so it is authenticated like every other browser request, and the browser's own download manager takes the file where a `fetch`-to-blob would buffer 1.49 GB in the tab.
2. **`download` is written bare, with no value.** The server sends `Content-Disposition: attachment` with an ASCII `filename=` and an RFC 6266 `filename*=UTF-8''…`; a value here would override that careful name with a guess.
3. **The anchor is a sibling of the `<Link>`, not a child** (addendum §2.4). A nested anchor is invalid HTML and browsers reparent it, so it goes after `</Link>`, beside the existing `panel === 'button'` toggle, which is already there for the same structural reason.

**And the safe default:** `useGate()` returns `undefined` where no gate sits above the screen, and `gate.ts`'s own doc says the safe reading of that is "no account known", never an admin. So the control is shown only on an explicit `true`.

- [ ] **Step 1: Write the failing tests**

In `web/src/Series.test.tsx`, add to the imports:

```tsx
import type { Me } from './api'
import { GateContext } from './gate'
```

Add the account fixture beside the existing `edition`/`work`/`card` helpers:

```tsx
const who = (over: Partial<Me> = {}): Me => ({
  user_id: 1,
  admin: false,
  can_download: true,
  can_stream: true,
  ...over,
})
```

Extend `settleWith` to take an optional account — the provider wraps the router, as `Shell.test.tsx` does:

```tsx
async function settleWith(
  props: Partial<
    Pick<
      React.ContextType<typeof BrowsePrefsContext>,
      'layout' | 'editionSelector' | 'panel'
    >
  > = {},
  me?: Me,
): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <BrowsePrefsContext.Provider
        value={{ ...browseModule.defaults, ...props }}
      >
        <GateContext.Provider
          value={me ? { who: me, signOut: () => {} } : undefined}
        >
          <MemoryRouter initialEntries={['/series/7']}>
            <Routes>
              <Route path="/series/:id" element={<Series />} />
            </Routes>
          </MemoryRouter>
        </GateContext.Provider>
      </BrowsePrefsContext.Provider>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}
```

Then add to the `describe('Series, grid layout', …)` block:

```tsx
  it('offers a download per tile, for the edition the tile is showing', async () => {
    const host = await settleWith({}, who())
    const links = [...host.querySelectorAll<HTMLAnchorElement>('.volumes a.get')]
    expect(links).toHaveLength(3)
    expect(links[0].getAttribute('href')).toBe(
      '/api/works/gotoubun-v01/raw/download',
    )
    // Bare, with no value: the server's Content-Disposition carries an
    // ASCII filename and an RFC 6266 one, and a value here would override
    // both with a guess made in the browser.
    expect(links[0].getAttribute('download')).toBe('')
  })

  it('keeps the download anchor outside the tile link, not nested in it', async () => {
    const host = await settleWith({}, who())
    const get = host.querySelector('.volumes a.get')
    expect(get).not.toBeNull()
    // A nested anchor is invalid HTML and browsers reparent it, which a
    // snapshot would render identically — so this is asserted structurally.
    expect(get?.closest('a.title-link, .volumes > li > a:not(.get)')).toBeNull()
  })

  it('shows no download at all when the account may not download', async () => {
    const host = await settleWith({}, who({ can_download: false }))
    expect(host.querySelectorAll('.volumes a.get')).toHaveLength(0)
  })

  it('shows no download when no gate sits above the screen', async () => {
    // `useGate()` is `undefined` here, and `gate.ts` says the safe reading of
    // that is "no account known" rather than a permissive one.
    const host = await settle()
    expect(host.querySelectorAll('.volumes a.get')).toHaveLength(0)
  })
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && pnpm vitest run src/Series.test.tsx -t download
```

Expected: FAIL — `expect(received).toHaveLength(3)` receives `0`, because nothing renders `a.get` yet.

- [ ] **Step 3: Add the URL helper**

In `web/src/library.ts`, beside `workCover`:

```ts
export const workCover = (slug: string, edition: string) =>
  `/api/works/${encodeURIComponent(slug)}/${encodeURIComponent(edition)}/pages/0/thumb`

/**
 * One edition as a single CBZ — spec C2's download route.
 *
 * **A URL, not a fetch.** The tile links to it with a plain `<a download>`:
 * an anchor navigation carries the session cookie like every other browser
 * request, and the browser's own download manager takes the file, where a
 * `fetch`-to-blob would buffer a 1.49 GB volume in the tab.
 */
export const editionDownload = (slug: string, edition: string) =>
  `/api/works/${encodeURIComponent(slug)}/${encodeURIComponent(edition)}/download`
```

- [ ] **Step 4: Render the control**

In `web/src/Series.tsx`, add to the `./library` import block:

```tsx
  editionDownload,
```

and add beside the other imports:

```tsx
import { useGate } from './gate'
```

Inside the `Series` component, beside `const { layout, editionSelector, panel } = useBrowsePrefs()`:

```tsx
  // `undefined` where no gate sits above this screen, and `gate.ts`'s own
  // rule is that the safe reading of that is "no account known" — so the
  // control appears only on an explicit `true`. **Absent rather than
  // disabled** (spec C2 §8): a disabled button invites a reader to find out
  // why, and there is nothing on this screen that can tell them.
  const mayDownload = useGate()?.who.can_download === true
```

Then in the tile, immediately after `</Link>` and before the `{panel === 'button' && (` block:

```tsx
                    {mayDownload && shown && (
                      <a
                        className="get"
                        href={editionDownload(w.slug, shown.key)}
                        download
                        aria-label={`download ${w.title} — ${shown.label}`}
                      >
                        ↓
                      </a>
                    )}
```

**A sibling of `<Link>`, not a child.** A nested `<a>` is invalid HTML; the existing `panel-open` button is already a sibling for the same reason.

- [ ] **Step 5: Style it**

In `web/src/index.css`, beside the existing `.volumes .panel-open` rule (`index.css:622`), add a `.volumes a.get` rule. That neighbour is the model: a small pill with `border: 1px solid ButtonBorder`, `border-radius: 999px`, `background: ButtonFace`, `font-size: 0.75rem`. The two sit in the same region of a tile and must not overlap — read the neighbour and match it rather than inventing a position.

**One hazard to handle explicitly.** `.grid li a, .volumes li a` (`index.css:441`) sets `display: flex; flex-direction: column; gap: 0.35rem` on *every* anchor inside a volume tile — it was written for the tile's own `<Link>`, which is the only anchor that has ever been in there. The new `a.get` inherits it and will render as a full-width column block unless the new rule overrides `display`. Verify in the browser, not only in vitest: jsdom computes no layout, so every test in Task 4 passes with the tile visually broken.

- [ ] **Step 6: Run the tests**

```bash
cd web && pnpm vitest run src/Series.test.tsx
```

Expected: PASS, including the pre-existing grid tests. If `titles()` or the edition-selector tests move, the anchor landed inside `.title` — it belongs after `</Link>`, not inside it.

- [ ] **Step 7: Commit**

```bash
git add web/src/library.ts web/src/Series.tsx web/src/Series.test.tsx web/src/index.css
git commit -m "A download per volume tile, for the edition the tile is already showing

A plain <a download>, never a fetch: an anchor navigation carries the
session cookie, and the browser's download manager takes the file where a
fetch-to-blob would buffer a 1.49 GB volume in the tab. The download
attribute is bare so the server's Content-Disposition name wins.

A sibling of the tile's Link rather than a child, because a nested anchor is
invalid HTML and browsers reparent it — asserted structurally, since a
snapshot renders both the same. Absent, not disabled, when can_download is
false: a disabled control invites a reader to find out why and nothing on
this screen can tell them."
```

---

## Task 5: The download control in the table layout

**Files:**
- Modify: `web/src/Series.tsx` — `Table` (`:71–116`)
- Test: `web/src/Series.test.tsx` — the `describe('Series, table layout', …)` block

**Interfaces:**
- Consumes: `editionDownload` from Task 4; `mayDownload`, which must now be passed into `Table` as a prop, since `Table` is a module-level function component rather than a closure inside `Series`.

**The decision this task carries** (addendum §2.1): §8 of the source spec says "the `table` layout gets the same control as a column", and that does not survive the table's actual shape. `Table` renders one column *per edition key*, with each cell holding that edition's page count. There is no `shown` edition in this component and no single edition a row is "already showing". A separate column would have to pick one arbitrarily, inside the one view whose whole purpose is comparing editions side by side.

**So each cell's page count becomes the download anchor for that (volume, edition) pair.** No new column. A `—` cell stays `—`, because there is nothing to download. §8's "one control rather than three" was an argument about the *tile*, which has an edition selector and therefore a settled question; a table row has neither.

- [ ] **Step 1: Write the failing tests**

Add to `describe('Series, table layout', …)` in `web/src/Series.test.tsx`:

```tsx
  it('makes each held edition’s page count a download link', async () => {
    const host = await settleWith({ layout: 'table' }, who())
    const cell = host.querySelector<HTMLAnchorElement>(
      'table.editions tbody tr td a.get',
    )
    expect(cell).not.toBeNull()
    expect(cell?.getAttribute('href')).toBe(
      '/api/works/gotoubun-v01/raw/download',
    )
    expect(cell?.textContent).toBe('180')
    expect(cell?.getAttribute('download')).toBe('')
  })

  it('leaves a missing edition’s cell as plain text', async () => {
    const host = await settleWith({ layout: 'table' }, who())
    const missing = host.querySelector('table.editions td.missing')
    expect(missing).not.toBeNull()
    expect(missing?.querySelector('a')).toBeNull()
    expect(missing?.textContent).toBe('—')
  })

  it('renders page counts as plain text when the account may not download', async () => {
    const host = await settleWith({ layout: 'table' }, who({ can_download: false }))
    expect(host.querySelectorAll('table.editions a.get')).toHaveLength(0)
    // The number is still there — only the link is gone.
    const cells = [...host.querySelectorAll('table.editions tbody td')]
    expect(cells.some((c) => c.textContent === '180')).toBe(true)
  })
```

No fixture change is needed: that block's `beforeEach` already mocks two works where only the first holds the `en` edition, so the `td.missing` cell the second test reads is already rendered. Leave it alone — its doc comment explains why the two volumes deliberately disagree on the `raw` label, and that reasoning is load-bearing for the column-label test below it.

- [ ] **Step 2: Run them and watch them fail**

```bash
cd web && pnpm vitest run src/Series.test.tsx -t table
```

Expected: FAIL — `expect(received).not.toBeNull()` receives `null`, because the cells hold bare text.

- [ ] **Step 3: Thread the capability into `Table`**

In `web/src/Series.tsx`, change `Table`'s signature and its cell:

```tsx
function Table({
  works,
  mayDownload,
}: {
  works: WorkCard[]
  mayDownload: boolean
}) {
  const columns = editionColumns(works)
```

and replace the held-edition cell:

```tsx
              return held ? (
                <td key={c.key}>{held.pages}</td>
              ) : (
```

with:

```tsx
              return held ? (
                // **The page count is the control.** §8 asked for "the same
                // control as a column", which does not survive this layout:
                // there is one column per edition and no single edition a
                // row is showing, so a separate column would have to pick
                // one arbitrarily inside the one view built for comparing
                // them. The number in the cell is already the per-edition
                // fact, so linking it adds no element at all.
                <td key={c.key}>
                  {mayDownload ? (
                    <a
                      className="get"
                      href={editionDownload(w.slug, c.key)}
                      download
                      aria-label={`download ${w.title} — ${c.label}`}
                    >
                      {held.pages}
                    </a>
                  ) : (
                    held.pages
                  )}
                </td>
              ) : (
```

And at the one call site (`Series.tsx:356`):

```tsx
            <Table works={loaded.works} mayDownload={mayDownload} />
```

- [ ] **Step 4: Run the tests**

```bash
cd web && pnpm vitest run src/Series.test.tsx
```

Expected: PASS, the whole file. A pre-existing table test that reads a cell's `textContent` still passes — the anchor's text is the same number.

- [ ] **Step 5: Commit**

```bash
git add web/src/Series.tsx web/src/Series.test.tsx
git commit -m "The table's page counts become its download links

Spec C2 §8 asked for \"the same control as a column\" and that does not
survive this layout's shape: Table renders one column per edition key, so
there is no edition a row is already showing and a separate column would
have to pick one arbitrarily — inside the one view whose purpose is
comparing them side by side. The number in each cell is already the
per-edition fact, so linking it adds no element. A missing edition's cell
stays an em-dash, because there is nothing to download."
```

---

## Task 6: The documents

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/notes.md`

**Run Task 6 after the device session in Task 7's step 1**, so `notes.md` records a measurement rather than an intention. Everything else here is independent of it.

- [ ] **Step 1: Rewrite the `taiyaku-serve` deferral sentence**

In `docs/status.md`'s `taiyaku-serve` table row (line ~308), this parenthetical becomes false on this branch and must be rewritten rather than left standing — the same rule the OPDS doc comment follows:

```
(spec C2 §§1–7; its third route, the OPDS-namespace mirror at
`/opds/v1.2/works/{work}/{edition}/download`, and its §8 web UI tile control are
both deferred to a second plan, so the OPDS acquisition link still points at an
edition's first page rather than at a download)
```

Replace with a statement of what is now true: all three routes ship, the acquisition link points at the download, its type is `download::advertised_type`'s answer rather than a constant, and a reader whose `can_download` is withheld keeps the page link because OPDS 1.2 requires an acquisition link on every entry.

- [ ] **Step 2: Move the four counted route figures**

The same row carries the posture table's counts in one sentence: 28 guarded routes, "7 Refuses + 11 Filters + 9 GrantIndependent + 1 KnownGap", and "18 of the 28" driven with "the remaining 10" declared.

**Count the table; do not trust this plan's numbers.** They have drifted three times, twice within one branch. Run:

```bash
grep -c 'Refuses)' crates/taiyaku-serve/src/routes.rs
cargo test -p taiyaku-serve auth::tests::every_route_declares_its_grant_posture
```

The expected shape after Task 1 is 29 routes, 8 `Refuses`, 19 driven, 10 declared — verify each before writing it. Consider phrasing the sentence so it does not need re-measuring every time a route is reclassified.

- [ ] **Step 3: Close O55, in all five parts**

Per the project's ritual, closing an O-number is a five-part edit and missing any part leaves the document contradicting itself:

1. the table row's checkbox — `| O55 | [ ] |` → `| O55 | [x] |`
2. the row's annotation, if the spike *narrowed* rather than closed it
3. the prose bullet, rewritten to say what was done and what remains
4. the test count near the top — it appears **twice**, in `Six crates, 1529 tests` (line ~120) and again in `Nineteen of the 1529 are standing checks` (line ~178)
5. the `**Thirty-five are now closed — … — leaving thirty-one.**` sentence (line ~1342) and the `What remains:` line (line ~3723), whose count word **wraps across a line break** — grep the sentence opener and read the next line, or a grep for `thirty-one open items` finds nothing and reads as already-updated

- [ ] **Step 3b: Open O67 — the derived title that names one edition of several**

The same device session turned this up, so it is recorded with O55's closure rather than separately. **Opening drifts the opposite way from closing**: the count word moves and the table row gets forgotten, and only `grep -c '^| O[0-9]'` against the section preamble catches it. All five parts:

1. **The section head count word** (line ~1329): `Sixty-six, each found by…` → `Sixty-seven`. That sentence is a list of the ways items have been found, and none of its existing clauses covers this one — add a clause in its voice, e.g. *"turned up by driving a real OPDS client against a real library"*.
2. **The "leaving N" sentence** (line ~1342). Careful: O55 closes in Step 3 and O67 opens here, so **the open count does not move** — 66 items/35 closed/31 open becomes 67 items/36 closed/31 open. The `leaving thirty-one` wording is correct **before and after**, which reads exactly like a line somebody forgot to update. Leave it, and do not "fix" it.
3. **The table row**, as the last row of the table:

```
| O67 | [ ] | **A scanned work's title is the filename of whichever entry minted its row, so a work spanning several editions is named after one of them.** Measured against Panels on an iPad, 2026-08-25: volume 1 of Go-Toubun no Hanayome holds five editions — `en-color`, `en-fan`, `en-mono`, `raw-color`, `raw-mono`, which is `derive.rs`'s `Group` working exactly as designed — while `work.title` is `Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz`, the stem of the colour file that happened to create the row. `opds::work_feed` renders `{title} — {label}` faithfully, so a reader is shown "…Full Color Edition… — en-mono" for a monochrome edition: the feed is right and the data is wrong. Two faults stacked — edition-specific text asserted over four editions it does not describe, and a raw filename kept whole, `.cbz` and `(2021)` included. **This is larger than the cost already recorded** in the `taiyaku-library` entry, which says only that a derived title is *stuck* ("a work this scanner minted keeps the title it first derived, so a later rule edit that would rename it does not reach the row"); what a real client shows is a title wrong about its own contents. `series_id` and `volume` are both on the row and would give "Go-Toubun no Hanayome, volume 1" — the exact string that same entry records a person having written before a scan overwrote it with "Series - v01" — but **`work.slug` derives from `work.title`, and the slug is the address progress, notes and truth all hang on**, so changing the derivation moves every scanned work's URL and strands the positions on it. A decision with a migration attached rather than a patch, and its home is C3's metadata editing, which O44 and O53 record as having no route and an unreachable apply step | `taiyaku-library/src/derive.rs`'s title derivation, `work.title`, `work.slug` | the spec C2 second-half device session, 2026-08-25 |
```

4. **The prose bullet** below the table, in the paragraph grouping items by where they came from. This one and O55's closure share a source — the first time a real OPDS client has been driven against this server — so say that once rather than twice.
5. **The "What remains" line** (line ~3723). Its count word does not move, per part 2. Its *second* half does need checking: it says how many open items came from a whole-branch review, and O67 did not come from one, so that number stays — verify rather than assume.

- [ ] **Step 4: Run the sweep, on the way out**

```bash
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -nE '[0-9]{3,4} tests|Nine|Nineteen of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

Rows minus closed must equal the "What remains" count word. Then grep the **bare O-number** across `docs/` and `web/src/` and read every hit — prose outside the table also describes open items, in wording no sweep can predict:

```bash
grep -rn 'O55' docs/ web/src/ crates/
grep -rn 'O67' docs/ web/src/ crates/
```

Rows must now be 67 and closed 36. If rows came out 66, the O67 row was forgotten while its count word moved — the exact opening drift this sweep exists to catch.

- [ ] **Step 5: Write the measurements into `notes.md`**

A dated section under the existing structure, carrying: whether Panels accepted the `tyk_` key (O55), whether the acquisition link produced a readable download on the device, and **what request headers the log actually captured** (O66's trigger). Include the commands run. Measurements go here, never in `status.md`.

- [ ] **Step 6: Commit**

```bash
git add docs/status.md docs/notes.md
git commit -m "The documents: O55 closed, O67 opened, the deferral sentence retired"
```

---

## Task 7: The device session, and O66's decision point

**Files:**
- Possibly modify: `crates/taiyaku-serve/src/download.rs` — `generated`, using `cbz::Plan::payload_span`
- No files, if the trigger does not fire.

- [ ] **Step 1: Run the device session**

With the branch's code built, serve on the LAN with request logging and drive Panels through a real acquisition fetch:

```bash
cargo build --release --bin taiyaku
./target/release/taiyaku serve --db target/o64-check/library.db --addr 0.0.0.0:8080 \
  --log-requests "$PWD/panels-acquisition.jsonl"
```

In Panels: refresh the OPDS source, open a work, and take the **download** rather than reading it in place. Then read the log:

```bash
python3 -c "
import json
for line in open('panels-acquisition.jsonl'):
    r = json.loads(line)
    if 'download' in r['path']:
        print(r['status'], r['path'], r['headers'])
"
```

- [ ] **Step 2: Decide O66 on the evidence**

O66's trigger is named precisely: a `--log-requests` session against Panels showing **`Range` on an acquisition fetch**. The existing `panels*.jsonl` logs predate request-header capture and report `(not recorded)`, so their silence is an instrument's blind spot rather than evidence.

- **No `Range` header on the download fetch** → O66 stays open exactly as written. Nothing here weakens its evidence standard. **Stop; the branch is done.**
- **`Range` present** → the trigger fired. `download::generated` currently walks every member from page 0 regardless of `from`, and `Plan::payload_span` already maps every entry's `[start, end)` in the output, which is the map a true seek would walk to open each member at the right page and `Seek::seek` past the rest.

  **Before building it, stop and ask the owner.** The remote is squash-only, so this fix merges into the same commit as the feature and nobody can revert one without the other — against this project's own rule that a branch holds one checkable claim. The choice between landing it here and stacking it as a second PR is made at this point, with the log in hand. That is why this task is last.

- [ ] **Step 3: Run the full gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Quote its final line verbatim. Do not pipe it through `tail` and do not append `; echo` — both mask a non-zero exit.

Then the corpus job on its own:

```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

- [ ] **Step 4: Whole-branch review before the PR**

Per this project's own measured finding: per-task reviews passed on every task of a branch while a whole-branch review found seven Criticals. Run one over the whole diff, reproduce anything it reports, and re-review after the fix round.

- [ ] **Step 5: Open the PR**

Via the **gitea** MCP, not `gh` — `gh` is authenticated against the wrong host. The PR body *is* the squash commit message, so write it as the commit this branch should become. Never hard-wrap it.

---

## Self-review

**Spec coverage.** §1's three changes → Tasks 1, 3, 4+5. §2.1 (table cells) → Task 5. §2.2 (the type helper) → Task 2, consumed in Task 3. §2.3 (the page-link fallback) → Task 3. §2.4 (sibling anchor) → Task 4. §3 (the enumeration's forced counts) → Task 1 steps 4–5 and Task 6 step 2. §4 (the two refusals) → Task 1's 401 test; the 404 and 403 halves are already covered by `every_route_declares_its_grant_posture` and `without_can_download_the_download_is_403_and_the_shelf_still_works`, which the new posture row and route extend without new tests. §5 (O55) → Task 0 and Task 6 step 3. §6 (O66) → Task 7. §7 (testing) → the test steps throughout. §8 (branch shape) → the task order. §9 (open items) → Task 6. §10 (not done) → nothing built, correctly.

**Placeholders.** None. Two steps deliberately require reading before writing rather than supplying code — Task 4 step 5 (the CSS neighbour) and Task 6 steps 1–2 (prose whose exact wording depends on counts that must be measured, not asserted). Both say what to read and why inventing it would be wrong.

**Type consistency.** `editionDownload(slug, edition)` is defined in Task 4 step 3 and used in Task 4 step 4 and Task 5 step 3 under the same name. `advertised_type(work, edition)` is defined in Task 2 step 3 and called in Task 3 step 4 with `work` as `&Work` — which is what `state.work_or_missing` returns (`lib.rs:134`), so no deref is needed. `passthrough_extension` returns `Option<String>` and both call sites bind it as `extension: String` passed to `type_for(&extension)`. `mayDownload` is a `boolean` in `Series` and a `boolean` prop on `Table`.

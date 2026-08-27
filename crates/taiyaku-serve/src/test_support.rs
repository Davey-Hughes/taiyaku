use std::io::Write;

use crate::{ServeState, Work};

/// A work of two cbz members, four pages, on disk under a named temp dir.
///
/// Two members deliberately: the offset arithmetic that picks which member
/// holds page N is what most of these tests are about, and one member
/// cannot exercise it. Each entry's bytes are its own name, so a test can
/// tell which page it got.
///
/// Shared rather than local to one test module because Task 5's route
/// tests need the same work.
pub(crate) fn work(dir: &str) -> Work {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    for (name, entries) in [
        ("c1.cbz", ["a1.jpg", "a2.jpg"]),
        ("c2.cbz", ["b1.jpg", "b2.jpg"]),
    ] {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(name)).unwrap());
        for entry in entries {
            zip.start_file(entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(entry.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
    let path = root.join("taiyaku.json");
    std::fs::write(&path, json).unwrap();

    Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    )
}

/// A library good for satisfying `ServeState::library` in a fixture. Task 12
/// gave every `ServeState` that field; Task 13's progress, notes and truth
/// routes are what actually reach it now — through `works::row_id_for_write`,
/// which mints a bridge row on first write for a `Work` this library was
/// never told about (`state`, below, hands `library()` an empty one paired
/// with an unimported `Work`, exactly that case). An empty, in-memory library
/// is still enough: a fresh one per call, since `Library` is not `Clone` and
/// two fixtures sharing a connection would let one test's rows leak into
/// another's.
pub(crate) fn library() -> std::sync::Arc<taiyaku_library::Library> {
    let library = taiyaku_library::Library::open_in_memory().expect("in-memory library");
    // **Migration 4 leaves the seeded account locked**, which is correct for a
    // real fresh library and useless for a fixture: every route is behind the
    // guard since spec A2 and there is no bypass (§9), so a fixture library
    // that nobody can authenticate against is a fixture no route test can use.
    // Given a password here rather than at each call site, because "the fixture
    // library" and "a library with a usable account" are the same thing for
    // every test in this crate.
    {
        let conn = library.conn();
        conn.execute(
            "UPDATE user SET password_hash = ?1 WHERE id = 1",
            [FIXTURE_PHC],
        )
        .expect("gives the fixture account a password");
        conn.execute(
            "INSERT INTO api_key (user_id, name, secret_sha256) VALUES (1, 'the suite', ?1)",
            [taiyaku_library::credential::token_sha256(FIXTURE_KEY)],
        )
        .expect("gives the fixture account a key");
    }
    std::sync::Arc::new(library)
}

/// The fixture work, wired to a `LocalLibrary`, ready for `build_router`.
pub(crate) fn state(dir: &str) -> ServeState {
    ServeState {
        library: library(),
        works: vec![work(dir)],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: std::env::temp_dir().join(dir).join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    }
}

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

/// A pixel channel value synthesized for `real_page_bytes` below.
///
/// # Panics
///
/// Panics if `v` exceeds `u8::MAX`. It cannot: `real_page_bytes` multiplies
/// a coordinate `from_fn` bounds to `0..16` by 16, topping out at `15 * 16
/// == 240`.
fn channel(v: u32) -> u8 {
    u8::try_from(v).expect("channel value exceeds u8::MAX")
}

/// A small real, decodable page image — same recipe as
/// `cover::tests::real_page_bytes`, duplicated here rather than exposed
/// across the module boundary that keeps `cover`'s test helpers private
/// to `cover`.
fn real_page_bytes() -> Vec<u8> {
    let page = image::RgbImage::from_fn(16, 16, |x, y| {
        let r = channel(x * 16);
        let g = channel(y * 16);
        image::Rgb([r, g, 200])
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(page)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}

/// A work whose one page is a real, decodable image rather than the
/// shared `work` fixture's ASCII bytes, and the temp directory it was
/// built in.
///
/// `work`'s pages are deliberately undecodable (see
/// `cover.rs::bytes_that_are_not_an_image_fail_by_name`), which suits
/// every route except one that actually has to generate a thumbnail. Named
/// `w`/`a` — work id and edition key — to match the paths a thumb-route
/// test addresses.
///
/// Split out from `state_with_real_pages` for the one test that hands a
/// `Vec<Work>` to `serve` rather than driving a built `ServeState`:
/// `Work` is deliberately not `Clone` (see `works.rs`), so the work cannot
/// be taken back out of an `Arc<ServeState>`, and a second copy of this
/// fixture is the thing that goes stale.
/// A temp directory that removes itself when the test that made it ends.
///
/// **`Drop` rather than a cleanup call at the end of each test**, because
/// the runs that leak are the ones that fail: a panicking test never
/// reaches its own last line, and the fixture a failure leaves behind is
/// exactly the one nobody goes back for. Measured before this existed —
/// 9,147 `taiyaku-test-real-pages-*` directories and 1,772
/// `taiyaku-thumb-*` under `/tmp`, 106 MB and 32,427 inodes between them.
/// Both helpers already documented why their *names* had to be unique and
/// neither said anything about who removes them, which is the whole gap:
/// uniqueness stops tests colliding, it does not stop them accumulating.
pub(crate) struct Fixture {
    path: std::path::PathBuf,
}

impl Fixture {
    /// A directory unique to this test: the process id plus a monotonic
    /// per-process counter, so tests running in parallel threads cannot
    /// see or delete each other's files. One counter across every prefix
    /// is enough, since the prefix is part of the name.
    ///
    /// No `tempfile` dependency: the workspace's dependency set is
    /// deliberately short enough to quote in `docs/status.md`, and this
    /// is the whole of what that crate would be used for here.
    pub(crate) fn new(prefix: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for Fixture {
    /// Failure is ignored deliberately, and in both directions: a test
    /// that has already passed must not be failed by a directory that
    /// would not go away, and one that has already failed must not have
    /// its message replaced by this one. A panic here during unwinding
    /// would abort the process outright.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn work_with_real_pages() -> (Work, Fixture) {
    // This helper used to open a single fixed path and start by
    // `remove_dir_all`-ing it with no cleanup after; Rust runs tests in
    // parallel threads, so the moment a second test called this helper,
    // whichever ran second would delete the first's fixture out from
    // under it mid-test. `Fixture` answers both halves — a unique name,
    // and someone to remove it.
    let fixture = Fixture::new("taiyaku-test-real-pages");
    let root = fixture.path();

    let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join("a1.cbz")).unwrap());
    zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&real_page_bytes()).unwrap();
    zip.finish().unwrap();

    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "a": { "label": "A", "members": [
              { "path": "a1.cbz", "sha256": "real", "pages": 1 }
            ], "spreads": [] }
          },
          "hub": "a",
          "alignments": []
        }"#;
    let path = root.join("taiyaku.json");
    std::fs::write(&path, json).unwrap();

    let work = Work::new(
        "w".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    );
    (work, fixture)
}

/// The real-page work, wired to a `LocalLibrary` with its cache under the
/// same temp directory.
///
/// Returns the `Fixture` alongside the state rather than dropping it here:
/// dropping it at the end of this function would delete the archives the
/// state is about to read. The caller binds it — `let (state, _fixture)` —
/// and the directory goes when the test does.
pub(crate) fn state_with_real_pages() -> (std::sync::Arc<ServeState>, Fixture) {
    let (work, fixture) = work_with_real_pages();
    let state = std::sync::Arc::new(ServeState {
        library: library(),
        works: vec![work],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    (state, fixture)
}

/// The fixture work, but with its `"raw"` edition renamed to a
/// caller-chosen key — for tests about the key itself, such as OPDS
/// percent-encoding one that is not URL-safe. The archives on disk are
/// unchanged; only the in-memory sidecar's edition map is renamed, which
/// is all a router test reads.
pub(crate) fn work_with_edition_key(dir: &str, key: &str) -> Work {
    let base = work(dir);
    let mut sidecar = (*base.sidecar()).clone();
    let edition = sidecar
        .editions
        .remove("raw")
        .expect("fixture defines the `raw` edition");
    sidecar.editions.insert(key.to_string(), edition);
    sidecar.hub = key.to_string();
    Work::new(base.id, base.path, sidecar)
}

pub(crate) fn cleanup(dir: &str) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
}

/// A request log writing to its own path under the temp dir.
///
/// Named per test the way `work` is, so tests running in parallel cannot
/// read each other's lines.
pub(crate) fn log(name: &str) -> (std::sync::Arc<crate::Log>, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("{name}.jsonl"));
    let _ = std::fs::remove_file(&path);
    let log = std::sync::Arc::new(crate::Log::new(&path).unwrap());
    (log, path)
}

/// The lines a log wrote, parsed.
///
/// Tests read the *file* rather than an in-memory copy, because the file
/// is the only artefact anything outside this crate consumes —
/// `scripts/request-summary.sh` reads it and nothing reads the other. A
/// test that passes here therefore says something about what a person can
/// actually measure after a reading session.
///
/// Parsed as `Value` rather than back into `Entry` so the test is checking
/// the wire format a `jq` script sees, not a round trip through the same
/// struct definition that wrote it.
pub(crate) fn logged_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("log line must parse as JSON: {e}: {line}"))
        })
        .collect()
}

/// Every `Work` in `library`, built the way `taiyaku-cli`'s `serve_work`
/// builds one: the slug from the row, the document from
/// `taiyaku_library::project::sidecar`, so `Work.id` is the library's own slug
/// and `works::find_row` hits a row on the very first read.
///
/// Test scaffolding, and named as such. `taiyaku_serve::works::from_library`
/// used to be this, as `pub` API — but `serve` never called it (review 2,
/// Minor 4), and a projection that only tests run is worth less than the same
/// six lines declared to be a fixture. What production does is
/// `serve_work`, one path at a time, and this is that loop over every row
/// instead of over `argv`, which is exactly what a fixture wants: build the
/// state, then test the routes.
///
/// # Panics
///
/// Panics if SQLite fails, if a row has no `sidecar_path` (nothing here
/// creates one), or if a work's document cannot be projected — every one of
/// which means the fixture itself is broken.
pub(crate) fn works_in(library: &taiyaku_library::Library) -> Vec<crate::Work> {
    let conn = library.conn();
    let mut stmt = conn
        .prepare("SELECT id, slug, sidecar_path FROM work ORDER BY id")
        .expect("listing the fixture's works");
    let rows: Vec<(i64, String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("listing the fixture's works")
        .collect::<Result<_, _>>()
        .expect("reading the fixture's works");

    rows.into_iter()
        .map(|(id, slug, sidecar_path)| {
            let path = sidecar_path.expect("every fixture work is imported with a path");
            let sidecar =
                taiyaku_library::project::sidecar(&conn, id).expect("projecting a fixture work");
            crate::Work::new(slug, std::path::PathBuf::from(path), sidecar)
        })
        .collect()
}

/// A `ServeState` over a real library: one work imported through
/// `taiyaku_library::import::sidecar`, with `works` built from those rows by
/// [`works_in`] — so `Work.id` is the library's own slug and
/// `works::find_row` finds a row on the very first read rather than
/// `works::row_id_for_write` minting one. Every other
/// fixture in this module pairs a bare `Work` with an *empty* `library()`,
/// which is the other half of the bridge Task 13 has to get right (see
/// `library()`'s own doc comment); this one is for the tests that are about
/// the library-backed path specifically. Named `w`: the fixture's title is
/// `"W"`, which `import::sidecar`'s `unique_slug` lowercases to exactly that,
/// with no collision to append a suffix against in a fresh library.
///
/// No archives are written to disk — unlike `work`/`state` above, which
/// exist mainly to give the page and thumbnail routes real bytes to read.
/// Nothing that reaches this fixture (progress, notes, truth, the work list)
/// touches a member's bytes, so there is nothing here for a real file to buy.
pub(crate) async fn router_over_a_library() -> (axum::Router, std::sync::Arc<ServeState>) {
    let library = library();
    {
        let conn = library.conn();
        let json = r#"{
              "taiyaku": 1,
              "work": { "title": "W", "reading": "rtl" },
              "editions": {
                "raw": { "label": "Raw", "members": [
                  { "path": "c1.cbz", "sha256": "x", "pages": 2 }
                ], "spreads": [] }
              },
              "hub": "raw",
              "alignments": []
            }"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("fixture sidecar parses");
        taiyaku_library::import::sidecar(
            &conn,
            &doc,
            std::path::Path::new("/library/w/taiyaku.json"),
        )
        .expect("import");
    }
    let works = works_in(&library);
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: std::env::temp_dir()
            .join("taiyaku-test-router-over-a-library")
            .join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// The two-edition, one-solved-edge shape a correction needs, over a **real**
/// library: the document is imported through
/// `taiyaku_library::import::sidecar`, so `work.id` is the row's own slug and
/// the alignment rows exist before anything is corrected.
///
/// `state_with_two_editions` above is the same document paired with an *empty*
/// library, which is what every existing correction test drives; this one is
/// for the tests that are about the rows a correction leaves behind. The
/// sidecar is written to a real path under a `Fixture` directory because
/// `patch_alignments` writes the file as well as the rows, and the restart
/// half of the round trip re-reads exactly those bytes.
///
/// Returns the row id alongside the router and state so a test can read the
/// rows back through `taiyaku_library::project::sidecar` without re-deriving
/// it, and the `Fixture` so the directory outlives the test body.
///
/// # Panics
///
/// Panics if the fixture cannot be written or imported — either means the
/// fixture itself is broken.
pub(crate) fn router_over_a_library_with_two_editions()
-> (axum::Router, std::sync::Arc<ServeState>, i64, Fixture) {
    let fixture = Fixture::new("taiyaku-test-correction-rows");
    let path = fixture.path().join("taiyaku.json");
    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] },
            "en": { "label": "En", "members": [
              { "path": "d1.cbz", "sha256": "p", "pages": 2 },
              { "path": "d2.cbz", "sha256": "q", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en", "by": "solver", "runs": [{ "a": "0-3", "b": "0-3" }] }
          ]
        }"#;
    std::fs::write(&path, json).expect("writing the fixture sidecar");

    let library = library();
    let work_id = {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(json).expect("fixture sidecar parses");
        taiyaku_library::import::sidecar(&conn, &doc, &path).expect("import")
    };
    let works = works_in(&library);
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = authenticating(crate::build_router(state.clone()));
    (router, state, work_id, fixture)
}

/// The correction fixture of [`router_over_a_library_with_two_editions`], for
/// a work that has **no document** — spec C1 §5.3.
///
/// Imported through `taiyaku_library::import::scanned`, so `sidecar_path` is
/// NULL exactly as it is for everything the scanner produces, and **no
/// `taiyaku.json` is written anywhere**. That absence is the point: the test
/// this exists for asserts that correcting such a work does not mint one.
///
/// Member paths are absolute, which is what `derive` writes and what makes a
/// document-less work servable at all — `root.join(&member.path)` returns an
/// absolute path unchanged.
///
/// The `Fixture` directory is returned and is deliberately left **empty**, so
/// a test can list it before and after a correction and compare.
///
/// # Panics
///
/// Panics if the fixture cannot be imported, which means the fixture is
/// broken.
pub(crate) fn router_over_a_scanned_work() -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("taiyaku-test-scanned-correction");
    let shelf = fixture.path().to_string_lossy().into_owned();
    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Scanned", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw/c1.cbz", "sha256": "x", "pages": 2 }},
              {{ "path": "{shelf}/raw/c2.cbz", "sha256": "y", "pages": 2 }}
            ], "spreads": [] }},
            "en": {{ "label": "En", "members": [
              {{ "path": "{shelf}/en/d1.cbz", "sha256": "p", "pages": 2 }},
              {{ "path": "{shelf}/en/d2.cbz", "sha256": "q", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": [
            {{ "from": "raw", "to": "en", "by": "solver", "runs": [{{ "a": "0-3", "b": "0-3" }}] }}
          ]
        }}"#
    );

    let library = library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        // `None`: no known address, so this is a work the scan is minting.
        let work_id =
            taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");

        // **The edge is written separately, because a scan cannot know it.**
        // `import::scanned` leaves `alignment` and `run` alone by design — a
        // walk has nothing to say about how two editions correspond — so the
        // document above declares an edge and the import correctly ignores it.
        // These are the rows `taiyaku align --write` would leave behind, and
        // without them `patch_alignments` answers 404 `NoPath` and the
        // correction this fixture exists for cannot happen at all.
        conn.execute(
            "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'raw', 'en', 'solver')",
            [work_id],
        )
        .expect("writes the solved edge");
        let alignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO run (alignment_id, ordinal, a_start_page, a_end_page, b_start_page, b_end_page)
             VALUES (?1, 0, 0, 3, 0, 3)",
            [alignment_id],
        )
        .expect("writes the run");
    }
    let works = crate::works::from_library(&library).expect("projects the scanned work");
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: std::env::temp_dir()
            .join("taiyaku-test-scanned-correction")
            .join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    // `authenticating`, because `library()` seeds a claimed account and every
    // route is behind the guard since spec A2 — the same wrapper
    // `router_over_a_library` uses, for the same reason.
    let router = authenticating(crate::build_router(state.clone()));
    (router, state, fixture)
}

/// [`router_over_a_scanned_work`], with its work attached to a series row.
///
/// `import::scanned` cannot write `series_id` — that is `derive`'s column, and
/// this fixture is not running a derive — so the series is written here the
/// way derivation would. Needed by the cover route, which resolves a series to
/// the work that stands for it.
///
/// The `Fixture` is returned so its `Drop` still runs. An earlier version
/// called `std::mem::forget` on it to escape the borrow, which skipped the
/// cleanup and leaked one temp directory per test run — found by the
/// whole-branch review.
///
/// # Panics
///
/// Panics if the rows cannot be written, which means the fixture is broken.
pub(crate) fn router_over_a_scanned_work_in_a_series()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let (router, state, fixture) = router_over_a_scanned_work();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO series (id, sort_title) VALUES (1, 'Scanned')",
            [],
        )
        .expect("writes the series");
        conn.execute(
            "INSERT INTO series_title (series_id, title, kind, is_primary)
             VALUES (1, 'Scanned', 'english', 1)",
            [],
        )
        .expect("names the series");
        conn.execute("UPDATE work SET series_id = 1, volume = 1.0", [])
            .expect("files the work under it");
    }
    (router, state, fixture)
}

/// [`router_over_a_scanned_work_in_a_series`], with the fixture account
/// demoted to a reader.
///
/// **The account's `role`, not a missing credential**, which is the whole
/// point: the router is still wrapped in [`authenticating`], so a request
/// through it authenticates perfectly and simply is not an administrator's.
/// A fixture that merely failed to sign in would answer a refusal to a broken
/// admin gate exactly as readily as to a working one — see
/// `library_api::tests::get_anonymous`'s note for the same trap found from
/// the other side.
///
/// # Panics
///
/// Panics if the demotion updates no row, which would leave the fixture an
/// administrator with nothing saying so.
pub(crate) fn router_over_a_scanned_work_as_a_reader()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let (router, state, fixture) = router_over_a_scanned_work_in_a_series();
    {
        let conn = state.library.conn();
        let demoted = conn
            .execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("demotes the fixture account");
        assert_eq!(
            demoted, 1,
            "the fixture account must actually stop being an administrator"
        );
    }
    (router, state, fixture)
}

/// A directory of page files, the same recipe `taiyaku-library`'s own scan
/// fixtures use (`scan::tests::write_pages`, which is `pub(crate)` there and
/// so out of reach here).
///
/// Each page's bytes carry the directory's own name, which is what stops two
/// volumes of the same length colliding on `content_key`.
fn write_pages(dir: &std::path::Path, pages: usize) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let label = dir.to_string_lossy().into_owned();
    for page in 0..pages {
        std::fs::write(
            dir.join(format!("{page:03}.jpg")),
            format!("not really a jpeg: {label} page {page}").as_bytes(),
        )
        .expect("write");
    }
}

/// A router over a library **scanned and derived from a real tree on disk**.
///
/// No fixture above can stand in for this. `import::sidecar` and
/// `import::scanned` write `work` and `series` rows and no `scan_entry` rows
/// at all — and `scan_entry` is what every proposal is raised from, what a
/// candidate rule is evaluated against, and what an accept's collateral guard
/// sweeps. A library imported rather than scanned raises nothing, so a route
/// test over one would assert against an empty inbox whatever the handler
/// did.
///
/// `titles` names one series each, laid out `{parent}/{title}/{title} - v01/`
/// — `parent` empty puts them at the root. Page counts ascend from two so
/// that no two volumes are the same length, the same belt-and-braces
/// `taiyaku-library`'s `library_with_series` keeps.
///
/// # Panics
///
/// Panics if the tree cannot be written, or if the scan or the derive fails —
/// each of which means the fixture itself is broken.
fn router_over_a_scanned_tree(
    prefix: &str,
    parent: &str,
    titles: &[&str],
) -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new(prefix);
    for (pages, title) in (2_usize..).zip(titles) {
        let under = if parent.is_empty() {
            fixture.path().to_path_buf()
        } else {
            fixture.path().join(parent)
        };
        write_pages(&under.join(title).join(format!("{title} - v01")), pages);
    }

    let library = library();
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, fixture.path()).expect("adds the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        // A sibling of the scanned tree rather than a directory inside it:
        // nothing here creates it, and one that appeared under the root
        // would be material a later scan walked.
        cache_dir: fixture.path().with_file_name(format!("{prefix}-covers")),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = authenticating(crate::build_router(state.clone()));
    (router, state, fixture)
}

/// A base series and its colour edition, side by side at the root — the
/// `edition-of` proposal `raise` was written for, and the one shape an accept
/// can be carried all the way through.
///
/// The same layout `taiyaku-library`'s `library_with_two_suffixed_series`
/// scans, so a route test and a store test are talking about one library.
pub(crate) fn router_over_two_suffixed_series()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    router_over_a_scanned_tree(
        "taiyaku-test-two-suffixed-series",
        "",
        &[
            "Go-Toubun no Hanayome",
            "Go-Toubun no Hanayome - Full Color Edition",
        ],
    )
}

/// The same pair, plus one entirely unrelated series, all three under one
/// `Publisher/` directory.
///
/// The measured shape behind the collateral guard: a pattern widened to
/// `Publisher/` reaches "Kaguya-sama wa Kokurasetai" as surely as it reaches
/// the subject, so accepting it would move somebody else's volumes into the
/// accepted series. `taiyaku-library`'s
/// `library_with_a_shared_publisher_directory` is the same tree.
pub(crate) fn router_over_a_shared_publisher_directory()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    router_over_a_scanned_tree(
        "taiyaku-test-shared-publisher",
        "Publisher",
        &[
            "Go-Toubun no Hanayome",
            "Go-Toubun no Hanayome - Full Color Edition",
            "Kaguya-sama wa Kokurasetai",
        ],
    )
}

/// A `GET` against `router`, driven with `oneshot` and no socket. Most
/// existing tests in `routes/tests.rs` build this same request by hand — this
/// exists for the shorter tests `router_over_a_library` was added alongside,
/// not as a mandate to rewrite the others.
///
/// **It sets no `Authorization` header of its own, deliberately.** Who the
/// caller is belongs to the router: every `router_over_*` helper above wraps
/// its router in [`authenticating`], which injects that fixture's own
/// credential, so a fixture built for a *reader* answers as a reader through
/// this same function. A `get` that hard-coded [`fixture_authorization`]
/// would send the administrator's key to every router it was handed and turn
/// a reader fixture back into an administrator without saying so — which is
/// the same trap `library_api::tests::get_anonymous`'s own note records from
/// the other direction.
pub(crate) async fn get(
    router: &axum::Router,
    uri: &str,
) -> axum::http::Response<axum::body::Body> {
    use tower::ServiceExt;
    router
        .clone()
        .oneshot(
            axum::http::Request::get(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// [`get`], with the body parsed. Panics on a non-200 naming the status — a
/// helper that silently returned `null` from a 401 would be a test asserting
/// nothing.
///
/// **Here rather than private to one test module**, which is the argument
/// `api.ts` already makes for exporting `getJson` instead of letting each
/// caller re-wrap `fetch`: the panic above is load-bearing, and two copies of
/// it would be two places for it to be forgotten.
pub(crate) async fn get_json(router: &axum::Router, path: &str) -> serde_json::Value {
    let response = get(router, path).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{path}: {body}");
    serde_json::from_str(&body).expect("a JSON body")
}

/// A request carrying a JSON body — what the write routes need and what
/// [`get`] cannot express.
///
/// The status is handed back rather than asserted, because the tests this
/// exists for are as often about a refusal as about a success: [`get_json`]
/// above is the one that insists on 200, and it insists because its return
/// type leaves a caller nothing to check.
///
/// Sets no `Authorization` header, for the reason [`get`] states.
pub(crate) async fn send_json(
    router: &axum::Router,
    method: axum::http::Method,
    uri: &str,
    body: &serde_json::Value,
) -> axum::http::Response<axum::body::Body> {
    use tower::ServiceExt;
    router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// A response's body, decoded as UTF-8 text.
pub(crate) async fn body_string(response: axum::http::Response<axum::body::Body>) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// The password the fixture account holds, and the name it holds it under —
/// migration 1 seeds `local`.
///
/// A constant rather than a random string per fixture so that a failing request
/// in a test log can be reproduced by hand with `curl -u local:fixture`.
pub(crate) const FIXTURE_NAME: &str = "local";
pub(crate) const FIXTURE_PASSWORD: &str = "fixture";

/// [`FIXTURE_PASSWORD`], already hashed.
///
/// **Pinned rather than hashed per fixture**, because argon2 runs unoptimized
/// under `cargo test` — this workspace sets no `[profile.dev]` opt-level — and
/// every `state()` call was paying a full hash for a password most tests never
/// present. Measured: `every_route_refuses_an_anonymous_request` alone took
/// 7.1 s to make 21 *anonymous* requests, and `.config/nextest.toml` states as
/// fact that "nothing in 639 tests exceeds 0.75 s" and justifies its 30 s slow
/// mark as "40x the slowest test". Found by the second whole-branch review.
///
/// `credential::tests::the_fixture_hash_still_verifies` is what stops this
/// drifting from the password beside it.
/// The key the suite authenticates with, and the reason the suite is fast.
///
/// **§2's design, used on its own terms.** A key is verified by sha256 and
/// routed past argon2 by its `tyk_` prefix, so a test request costs microseconds
/// where a password costs the measured 11.9 ms — and every authenticated request
/// in this crate was paying the latter. Measured: `taiyaku-serve`'s suite went
/// from 0.66 s on `main` to 11.85 s, with one concurrency test alone at 10.8 s,
/// against a `.config/nextest.toml` that states "nothing in 639 tests exceeds
/// 0.75 s" as fact. Found by the second whole-branch review.
///
/// Tests *about* passwords still use `FIXTURE_PASSWORD`, which is the point:
/// they are the ones that should pay for argon2.
pub(crate) const FIXTURE_KEY: &str = "tyk_the-suites-own-key-not-a-real-secret";

pub(crate) const FIXTURE_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$KtgzR4TOZsatunnAHzKmig$fRrVxJJcb/LwTcYO+9XZ4C5uNgdPM131N8iXWxLeXxk";

/// Wrap a router so every request through it presents the fixture credential.
///
/// **Spec A2 §9 refuses a `--no-auth` flag and a dev bypass**, so the suite does
/// not get one either: this is a test *client* that authenticates, through the
/// real guard, with a real password that `give_the_fixture_account_a_password`
/// actually stored. Nothing here is reachable from a production binary.
///
/// One function rather than one per helper, so "how a test authenticates" has a
/// single answer — and a request that sets its own `Authorization` is left
/// alone, so a test asserting a *wrong* credential still can.
pub(crate) fn authenticating(router: axum::Router) -> axum::Router {
    let credential = fixture_authorization();
    router.layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let credential = credential.clone();
            async move {
                if !request.headers().contains_key("authorization") {
                    request.headers_mut().insert(
                        "authorization",
                        axum::http::HeaderValue::from_str(&credential).expect("ascii"),
                    );
                }
                next.run(request).await
            }
        },
    ))
}

/// The `Authorization` header value the suite presents.
///
/// The **key**, not the password — see [`FIXTURE_KEY`]. A handful of tests want
/// the password specifically and build their own header.
pub(crate) fn fixture_authorization() -> String {
    use base64::Engine as _;

    let encoded = base64::engine::general_purpose::STANDARD
        .encode(format!("{FIXTURE_NAME}:{FIXTURE_KEY}").as_bytes());
    format!("Basic {encoded}")
}

#[cfg(test)]
mod pinned {
    /// The pinned hash and the password beside it must stay one pair.
    ///
    /// A constant that stopped matching would make every fixture account
    /// unauthenticatable, and the failure would surface as dozens of unrelated
    /// route tests answering 401 rather than as anything naming the cause.
    #[test]
    fn the_fixture_hash_still_verifies() {
        assert!(taiyaku_library::credential::verify_password(
            super::FIXTURE_PASSWORD,
            super::FIXTURE_PHC
        ));
        assert!(!taiyaku_library::credential::verify_password(
            "not the fixture password",
            super::FIXTURE_PHC
        ));
    }
}

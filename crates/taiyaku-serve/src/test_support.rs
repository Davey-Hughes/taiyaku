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
    seed_fixture_account(&library);
    std::sync::Arc::new(library)
}

/// **Migration 4 leaves the seeded account locked**, which is correct for a
/// real fresh library and useless for a fixture: every route is behind the
/// guard since spec A2 and there is no bypass (§9), so a fixture library that
/// nobody can authenticate against is a fixture no route test can use.
///
/// **Extracted from [`library`] rather than copied into
/// [`router_over_a_file_backed_library`]**, for the ordinary reason: two
/// copies of a fixture's credential are two things to forget to change
/// together.
fn seed_fixture_account(library: &taiyaku_library::Library) {
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

/// A router over a library **that is a file**, which every backup test needs:
/// `Library::path` answers `None` for `:memory:` and `backup_api` refuses with
/// 409 when it does — correctly, since there is nothing to copy.
///
/// **Three directories inside one [`Fixture`], rather than siblings of it.**
/// The scan root is `tree/`, the database is `db/library.db` and covers go in
/// `covers/`, so the database and the `backups/` directory beside it are
/// outside the root — anything under a root is material the next scan walks —
/// while all three still go away with the one `Drop`.
/// [`router_over_a_scanned_tree`] puts its `cache_dir` beside the fixture with
/// `with_file_name` instead, which leaks it; that is not copied here, and the
/// 9,147 leaked directories [`Fixture`]'s own comment records are why.
///
/// # Panics
///
/// Panics if the directories cannot be created or the library cannot be
/// opened — a fixture that silently fell back to memory would make every
/// backup test assert the 409 refusal rather than the feature.
pub(crate) fn router_over_a_file_backed_library()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("taiyaku-test-file-backed-library");
    let tree = fixture.path().join("tree");
    let db_dir = fixture.path().join("db");
    write_pages(&tree.join("Scanned").join("Scanned - v01"), 2);
    std::fs::create_dir_all(&db_dir).expect("creates the database directory");

    let library = taiyaku_library::Library::open(&db_dir.join("library.db"))
        .expect("opens a file-backed library");
    seed_fixture_account(&library);
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &tree).expect("adds the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let library = std::sync::Arc::new(library);
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        cache_dir: fixture.path().join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// [`router_over_a_file_backed_library`], with the fixture account demoted to
/// a reader — the same demotion [`router_over_a_single_scanned_work_as_a_reader`]
/// applies, and for the same reason.
///
/// # Panics
///
/// Panics if the demotion updates no row.
pub(crate) fn router_over_a_file_backed_library_as_a_reader()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let (router, state, fixture) = router_over_a_file_backed_library();
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

/// The fixture work, wired to a `LocalLibrary`, ready for `build_router`.
pub(crate) fn state(dir: &str) -> ServeState {
    ServeState {
        library: library(),
        works: vec![work(dir)],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: std::env::temp_dir().join(dir).join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
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
    png(image::RgbImage::from_fn(16, 16, |x, y| {
        let r = channel(x * 16);
        let g = channel(y * 16);
        image::Rgb([r, g, 200])
    }))
}

/// `page` encoded as PNG, the format these fixtures' archives hold.
fn png(page: image::RgbImage) -> Vec<u8> {
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
    (local_state(work, &fixture), fixture)
}

/// `work` read from disk by `LocalLibrary`, caching under `fixture`'s
/// directory so that the cache goes when the fixture does.
fn local_state(work: Work, fixture: &Fixture) -> std::sync::Arc<ServeState> {
    std::sync::Arc::new(ServeState {
        library: library(),
        works: vec![work],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    })
}

/// O108's fixture: one archive that is the second member of one edition and
/// the whole of another, so a page's index in its edition and its offset in
/// its member differ.
///
/// Work `m`. Edition `ab` is `a.cbz` (two pages) then `b.cbz` (three), and
/// edition `b` is `b.cbz` alone. So page 2 of `ab` is `b.cbz`'s first page,
/// the same page as page 0 of `b`, while page 2 of `b` is `b.cbz`'s third. A
/// route that keyed a page by its index would give page 2 of `ab` the tag, the
/// cached rendering and the tile of page 2 of `b`, which is a different
/// picture. No fixture before this one put one member at two positions. `work`'s
/// edition does reach pages whose index and offset differ, but a name built
/// from the index is still unique within one edition, the one fixture with two
/// editions gives both the same members in the same order, and `work`'s pages
/// cannot be decoded, so none of its requests reached a resize or a tile.
///
/// `b.cbz`'s pages are solid red, green and blue, in that order, so a test can
/// say which one it was served after a resize or a tile's JPEG encoding
/// (`colour_of`). `a.cbz`'s first page is grey.
///
/// `a.cbz`'s second page is 4 pixels wider than `cover::MAX_FIT_LONG_EDGE`.
/// It is the one page here that a width above the ceiling and the clamped
/// width treat differently: a 16×16 page fits inside both and is served
/// untouched either way.
pub(crate) fn state_with_a_shared_member() -> (std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("taiyaku-test-shared-member");
    let root = fixture.path();

    let solid = |colour: [u8; 3]| png(image::RgbImage::from_pixel(16, 16, image::Rgb(colour)));
    let wide = png(image::RgbImage::from_pixel(
        crate::cover::MAX_FIT_LONG_EDGE + 4,
        4,
        image::Rgb([128, 128, 128]),
    ));
    for (name, pages) in [
        ("a.cbz", vec![solid([128, 128, 128]), wide]),
        (
            "b.cbz",
            vec![
                solid([220, 40, 40]),
                solid([40, 220, 40]),
                solid([40, 40, 220]),
            ],
        ),
    ] {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(name)).unwrap());
        for (n, bytes) in pages.iter().enumerate() {
            zip.start_file(
                format!("p{}.png", n + 1),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "M", "reading": "rtl" },
          "editions": {
            "ab": { "label": "AB", "members": [
              { "path": "a.cbz", "sha256": "sha-a", "pages": 2 },
              { "path": "b.cbz", "sha256": "sha-b", "pages": 3 }
            ], "spreads": [] },
            "b": { "label": "B", "members": [
              { "path": "b.cbz", "sha256": "sha-b", "pages": 3 }
            ], "spreads": [] }
          },
          "hub": "ab",
          "alignments": []
        }"#;
    let path = root.join("taiyaku.json");
    std::fs::write(&path, json).unwrap();

    let work = Work::new(
        "m".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    );
    (local_state(work, &fixture), fixture)
}

/// Which of red, green, blue and grey a served page is, read from its centre
/// pixel.
///
/// For `state_with_a_shared_member`'s solid pages, whose colour survives a
/// resize and a JPEG encoding where a finer pattern would not. A channel counts
/// only when it leads both others by more than JPEG's drift, and grey is all
/// three within a little of each other.
pub(crate) fn colour_of(bytes: &[u8]) -> &'static str {
    let page = image::load_from_memory(bytes)
        .expect("a decodable page")
        .to_rgb8();
    let image::Rgb([r, g, b]) = *page.get_pixel(page.width() / 2, page.height() / 2);
    let leads = |one: u8, other: u8, another: u8| {
        one > other.saturating_add(64) && one > another.saturating_add(64)
    };
    if leads(r, g, b) {
        "red"
    } else if leads(g, r, b) {
        "green"
    } else if leads(b, r, g) {
        "blue"
    } else if r.abs_diff(g) <= 16 && g.abs_diff(b) <= 16 && r.abs_diff(b) <= 16 {
        "grey"
    } else {
        "none of red, green, blue and grey"
    }
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
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
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
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
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
        scans: Default::default(),
        library_job: Default::default(),
    });
    // `authenticating`, because `library()` seeds a claimed account and every
    // route is behind the guard since spec A2 — the same wrapper
    // `router_over_a_library` uses, for the same reason.
    let router = authenticating(crate::build_router(state.clone()), &state.library);
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
        // **These fixtures are about the document, so they keep documents.**
        // Migration 012 made `keeps_documents` default off, which is right for
        // a person's library and wrong for a fixture whose whole subject is
        // the file — `patching_a_title_writes_the_row_and_the_document` and
        // twelve assertions like it read it off disk. A fixture that wants the
        // off case turns it off explicitly, which is the rarer thing to say.
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
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
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// One scanned, single-volume work whose member is a real **`.cbz`**, not a
/// directory of pages.
///
/// **Every other scanned fixture here writes `write_pages`**, so its member
/// sniffs as `Format::Dir`. That is right for the routes those fixtures were
/// built for and useless for spec C3 §6's write-back, which only ever touches
/// a zip: against a directory-backed fixture the stamp plan is empty, and two
/// of its route tests passed while asserting nothing until this existed.
///
/// Keeps documents, for the reason [`router_over_a_scanned_tree`] gives.
///
/// # Panics
///
/// Panics if the tree cannot be written or the scan fails.
pub(crate) fn router_over_a_scanned_cbz() -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let prefix = "taiyaku-test-scanned-cbz";
    let fixture = Fixture::new(prefix);
    // **Everything under the one fixture directory, which is unique per
    // call.** Keying a sibling path on `prefix` instead puts every test using
    // this fixture on the same database file, and they run in parallel: the
    // first `remove_dir_all` pulls the file out from under a connection
    // another test has already mmap'd, which arrives as SIGBUS rather than as
    // anything that names itself.
    let tree = fixture.path().join("tree");
    let db_dir = fixture.path().join("db");
    let dir = tree.join("Scanned");
    std::fs::create_dir_all(&dir).expect("creates the series directory");
    std::fs::create_dir_all(&db_dir).expect("creates the database directory");

    let path = dir.join("Scanned - v01.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("creates the archive"));
    for entry in ["000.jpg", "001.jpg"] {
        zip.start_file(entry, zip::write::SimpleFileOptions::default())
            .expect("starts an entry");
        zip.write_all(entry.as_bytes()).expect("writes an entry");
    }
    zip.finish().expect("finishes the archive");

    // **File-backed, unlike its neighbours, and that is the point.**
    // `Library::working()` opens a genuine second connection only where the
    // library has a file; against an in-memory one it hands back the shared
    // guard instead. Spec C3 §6's two routes both take `working()`, so an
    // in-memory fixture would exercise the shared path in every test and
    // leave the path that actually runs in production untested — a fixture
    // where right and wrong render the same.
    let library = taiyaku_library::Library::open(&db_dir.join("library.db"))
        .expect("opens a file-backed library");
    seed_fixture_account(&library);
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &tree).expect("adds the root");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let library = std::sync::Arc::new(library);
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        // Outside the scanned tree, so nothing this creates is material a
        // later scan walks — `router_over_a_scanned_tree`'s own reasoning.
        cache_dir: fixture.path().join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

pub(crate) fn router_over_a_scanned_cbz_as_a_reader()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let (router, state, fixture) = router_over_a_scanned_cbz();
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

/// One scanned, single-volume work with a root of its own — the metadata
/// PATCH routes' own fixture.
///
/// **Actually scanned**, unlike `router_over_a_scanned_work` above: that
/// helper imports a hand-authored sidecar directly through
/// `import::scanned`, which never runs a member through `scan::scan_root` and
/// so never gives it a `root_id` — `hand::path_for` requires one (its own
/// join on `member.root_id`), so a metadata edit applied against that
/// fixture always answers `persisted: false` with no document ever reaching
/// disk. Measured rather than assumed: `metadata::tests::
/// patching_a_title_writes_the_row_and_the_document` failed with exactly
/// that body the first time it ran against `router_over_a_scanned_work_in_a_series`.
/// This fixture goes through the real walk instead, the same as
/// `router_over_two_suffixed_series` and its neighbours, which is what gives
/// it an address at all.
pub(crate) fn router_over_a_single_scanned_work()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    router_over_a_scanned_tree("taiyaku-test-single-scanned-work", "", &["Scanned"])
}
/// [`router_over_a_single_scanned_work`], with the fixture account demoted to
/// a reader — the same demotion `router_over_a_scanned_work_as_a_reader`
/// applies to its own fixture and for the same reason: a router wrapped in
/// [`authenticating`] still authenticates perfectly, and simply is not an
/// administrator's.
///
/// # Panics
///
/// Panics if the demotion updates no row, which would leave the fixture an
/// administrator with nothing saying so.
pub(crate) fn router_over_a_single_scanned_work_as_a_reader()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let (router, state, fixture) = router_over_a_single_scanned_work();
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

/// Two scanned roots, each holding an unrelated, single-volume series — the
/// fixture the roots routes' own tests need: a first root to leave standing,
/// and a second whose id names a real row to patch or remove, with a real
/// work under it whose removal costs something nonzero to report.
///
/// **Two independent series, not one split across two roots** — unlike
/// [`router_over_two_scanned_volumes_of_one_series_across_two_roots`] above,
/// deliberately: removing the second root here must delete a whole work
/// (`scan.rs`'s own `DOOMED_WORKS`, "a work this root alone accounts for"),
/// not merely orphan one edition of a work that survives on the other root —
/// a removal test wants a nonzero work count it can assert on without also
/// asserting the *other* root came through unscathed.
///
/// The fixture account keeps its seeded `admin` role — every route under
/// `/api/library/roots` is admin-gated (C4's opening line), and every test
/// built over this fixture is about that gate passing, not about O86's
/// grant-dropping.
pub(crate) fn state_with_two_scanned_roots() -> (axum::Router, std::sync::Arc<ServeState>, Fixture)
{
    let fixture = Fixture::new("taiyaku-test-two-scanned-roots");
    let root_a = fixture.path().join("root-a");
    let root_b = fixture.path().join("root-b");
    write_pages(&root_a.join("Series A").join("Series A - v01"), 2);
    write_pages(&root_b.join("Series B").join("Series B - v01"), 2);

    let library = library();
    {
        let conn = library.conn();
        let a = taiyaku_library::scan::add_root(&conn, &root_a).expect("adds root a");
        taiyaku_library::scan::scan_root(&conn, &a).expect("scans root a");
        let b = taiyaku_library::scan::add_root(&conn, &root_b).expect("adds root b");
        taiyaku_library::scan::scan_root(&conn, &b).expect("scans root b");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    assert_eq!(
        works.len(),
        2,
        "two unrelated series, one work each: two works, not {}",
        works.len()
    );
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture
            .path()
            .with_file_name("taiyaku-test-two-scanned-roots-covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// The ids of every root [`state_with_two_scanned_roots`] built, ascending —
/// `scan::roots`'s own order, and the order the two `add_root` calls above
/// wrote them in.
fn root_ids(state: &ServeState) -> Vec<i64> {
    let conn = state.library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM root ORDER BY id")
        .expect("prepares the root id query");
    stmt.query_map([], |r| r.get(0))
        .expect("queries the root ids")
        .collect::<Result<Vec<i64>, _>>()
        .expect("reads the root ids")
}

/// The second root [`state_with_two_scanned_roots`] added — the one its own
/// removal test acts against, leaving the first standing so a test can
/// assert on what survived as well as on what did not.
pub(crate) fn second_root_id(state: &ServeState) -> i64 {
    let ids = root_ids(state);
    assert_eq!(
        ids.len(),
        2,
        "the two-root fixture must hold exactly two roots, not {}",
        ids.len()
    );
    ids[1]
}

/// A real, existing directory under [`state_with_two_scanned_roots`]'s first
/// root.
///
/// `scan::add_root_checked` canonicalises and lists whatever path it is
/// given before it ever checks for an overlap, so a nesting refusal can only
/// be proven against a path that genuinely exists on disk — a made-up
/// subdirectory would be refused for not existing, which proves nothing
/// about the nesting check this fixture exists for.
pub(crate) fn a_path_inside_the_first_root(state: &ServeState) -> String {
    let conn = state.library.conn();
    let root_path: String = conn
        .query_row("SELECT path FROM root ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .expect("the fixture's first root");
    format!("{root_path}/Series A")
}

/// Two scanned volumes of **one** series — the shape `metadata::patch_series`
/// needs a caller to exist over: `series`, `series_title` and `external_id`
/// are shared across every work under a series (`librarydoc.rs`'s own module
/// comment), so proving a series edit reaches every member work needs at
/// least two works sharing one `series_id`.
///
/// Built the way every other scanned fixture in this module is — real files
/// under a real root, `scan_root` then `derive` — rather than by hand-writing
/// a second `work` row under a fixture's series the way
/// `router_over_a_scanned_work_in_a_series` attaches its own one work: the
/// default rules (`003_scanner.sql`) resolve a series name from the FIRST
/// DIRECTORY under the root, so two volume directories under one title
/// directory resolve to one series holding two works with no rule ever
/// written by hand.
///
/// # Panics
///
/// Panics if the tree cannot be written, if the scan or the derive fails, or
/// if the result is not exactly two works under one series — each of which
/// means the fixture itself is broken.
pub(crate) fn router_over_two_scanned_volumes_of_one_series()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("taiyaku-test-two-volumes-one-series");
    let title = "Go-Toubun no Hanayome";
    for (pages, volume) in (2_usize..).zip(["v01", "v02"]) {
        write_pages(
            &fixture
                .path()
                .join(title)
                .join(format!("{title} - {volume}")),
            pages,
        );
    }

    let library = library();
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, fixture.path()).expect("adds the root");
        // **These fixtures are about the document, so they keep documents.**
        // Migration 012 made `keeps_documents` default off, which is right for
        // a person's library and wrong for a fixture whose whole subject is
        // the file — `patching_a_title_writes_the_row_and_the_document` and
        // twelve assertions like it read it off disk. A fixture that wants the
        // off case turns it off explicitly, which is the rarer thing to say.
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    assert_eq!(
        works.len(),
        2,
        "one series, two volumes: two works, not {}",
        works.len()
    );
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture
            .path()
            .with_file_name("taiyaku-test-two-volumes-one-series-covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// Two scanned volumes of one series, each rooted under its own directory —
/// the shape Task 6b's `patch_series` worst-case finding needs a caller to
/// exist over: a series whose works span two roots (this codebase's own
/// `auth/tests.rs` fixtures model a work or an edition split across two roots
/// for the grant-posture proofs, but none of them attaches two *works* to one
/// series, which is what a per-work document write failing independently of
/// its siblings needs).
///
/// **`root_a`'s path comes back, not `root_b`'s — deliberately the volume
/// `derive` writes (and so `works_in_series` lists) *first*.** `GroupKey`
/// orders groups by `(series, volume, …)`, so `v01`/`root_a` gets the lower
/// `work.id`, and `patch_series`'s own loop visits it before `v02`/`root_b`.
/// A test proving "the worst case, not the last one" has to break the work
/// that is *not* last, or a bug that only remembers the final iteration would
/// pass anyway — the failure would coincide with the last answer by luck of
/// the id order rather than being caught by the assertion. Made read-only
/// with `taiyaku-library`'s own
/// `hand::tests::a_root_that_cannot_be_written_still_arms_the_guard`
/// technique, so exactly one work's `hand::save` fails while its sibling's
/// still lands.
///
/// # Panics
///
/// Panics if either tree cannot be written, if either scan or the derive
/// fails, or if the result is not exactly two works under one series — each
/// of which means the fixture itself is broken.
pub(crate) fn router_over_two_scanned_volumes_of_one_series_across_two_roots() -> (
    axum::Router,
    std::sync::Arc<ServeState>,
    Fixture,
    std::path::PathBuf,
) {
    let fixture = Fixture::new("taiyaku-test-two-volumes-two-roots");
    let title = "Go-Toubun no Hanayome";
    let root_a = fixture.path().join("root-a");
    let root_b = fixture.path().join("root-b");
    write_pages(&root_a.join(title).join(format!("{title} - v01")), 2);
    write_pages(&root_b.join(title).join(format!("{title} - v02")), 3);

    let library = library();
    {
        let conn = library.conn();
        let a = taiyaku_library::scan::add_root(&conn, &root_a).expect("adds root a");
        // **These fixtures are about the document, so they keep documents.**
        // Migration 012 made `keeps_documents` default off, which is right for
        // a person's library and wrong for a fixture whose whole subject is
        // the file — `patching_a_title_writes_the_row_and_the_document` and
        // twelve assertions like it read it off disk. A fixture that wants the
        // off case turns it off explicitly, which is the rarer thing to say.
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [a.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &a).expect("scans root a");
        let b = taiyaku_library::scan::add_root(&conn, &root_b).expect("adds root b");
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [b.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &b).expect("scans root b");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    assert_eq!(
        works.len(),
        2,
        "one series, two volumes, one on each root: two works, not {}",
        works.len()
    );
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture
            .path()
            .with_file_name("taiyaku-test-two-volumes-two-roots-covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture, root_a)
}

/// [`router_over_two_scanned_volumes_of_one_series_across_two_roots`], with a
/// third, entirely unrelated series scanned from a root of its own.
///
/// **Why the plain version cannot stand in for it.** That fixture holds
/// exactly one series, so `metadata::series_of_work` returning the constant
/// `Ok(Some(1))` instead of actually reading the row agrees with reality by
/// coincidence — SQLite mints the one series it ever sees as id 1. A mutant
/// that replaces `series_of_work`'s body with that constant survives every
/// test built over that fixture. This one adds a second series, so the
/// series a work moves *out of* is not id 1, and a caller trusting the
/// constant reaches the wrong series' siblings entirely.
///
/// The unrelated series' title sorts before `"Go-Toubun no Hanayome"` —
/// `derive::derive`'s own `groups: BTreeMap<GroupKey, _>` orders series by
/// name, regardless of which root was scanned first — so it is the one
/// series row minted first and the shared series is left holding a later
/// id. Confirmed at the call site rather than trusted here: a fixture that
/// silently drifted back to coincidence would be exactly the failure this
/// exists to prevent.
///
/// # Panics
///
/// Panics if any tree cannot be written, if any scan or the derive fails, or
/// if the result is not exactly three works — two under the shared series,
/// one under the unrelated one — each of which means the fixture itself is
/// broken.
pub(crate) fn router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series()
-> (
    axum::Router,
    std::sync::Arc<ServeState>,
    Fixture,
    std::path::PathBuf,
) {
    let fixture = Fixture::new("taiyaku-test-two-volumes-plus-unrelated-series");
    let title = "Go-Toubun no Hanayome";
    let root_unrelated = fixture.path().join("root-unrelated");
    let root_a = fixture.path().join("root-a");
    let root_b = fixture.path().join("root-b");
    write_pages(
        &root_unrelated
            .join("AAA Unrelated Series")
            .join("AAA Unrelated Series - v01"),
        2,
    );
    write_pages(&root_a.join(title).join(format!("{title} - v01")), 2);
    write_pages(&root_b.join(title).join(format!("{title} - v02")), 3);

    let library = library();
    {
        let conn = library.conn();
        // Scanned *after* `root_a`/`root_b` below, deliberately: the ordering
        // this fixture's own doc comment relies on comes from `derive`'s own
        // `BTreeMap<GroupKey, _>`, not from scan order, and proving that is
        // the point of scanning it out of title order here.
        let a = taiyaku_library::scan::add_root(&conn, &root_a).expect("adds root a");
        // **These fixtures are about the document, so they keep documents.**
        // Migration 012 made `keeps_documents` default off, which is right for
        // a person's library and wrong for a fixture whose whole subject is
        // the file — `patching_a_title_writes_the_row_and_the_document` and
        // twelve assertions like it read it off disk. A fixture that wants the
        // off case turns it off explicitly, which is the rarer thing to say.
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [a.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &a).expect("scans root a");
        let b = taiyaku_library::scan::add_root(&conn, &root_b).expect("adds root b");
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [b.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &b).expect("scans root b");
        let unrelated = taiyaku_library::scan::add_root(&conn, &root_unrelated)
            .expect("adds the unrelated root");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [unrelated.id],
        )
        .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &unrelated).expect("scans the unrelated root");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    assert_eq!(
        works.len(),
        3,
        "two volumes under the shared series, one under the unrelated one: \
         three works, not {}",
        works.len()
    );
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture
            .path()
            .with_file_name("taiyaku-test-two-volumes-plus-unrelated-series-covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture, root_a)
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

/// A request carrying a **raw** body — what `config_api::import_rules` wants
/// tested against, since a rule-export document is sent as the literal bytes
/// [`taiyaku_library::rules::export_json`] produced rather than re-parsed
/// through a `serde_json::Value` first, the way a real client resending an
/// exported file would send it. [`send_json`] above always re-serialises its
/// `Value` argument, which loses nothing a JSON parser cares about but is not
/// this route's own round trip.
///
/// Sets no `Authorization` header, for the reason [`get`] states.
pub(crate) async fn send_raw(
    router: &axum::Router,
    method: axum::http::Method,
    uri: &str,
    body: &str,
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

/// Wrap a router so every request through it presents **an API key**.
///
/// **This used to be `authenticating`, and that default is what hid O93.**
/// Every route test in this crate drove a device credential, so no test could
/// see that `config_api`, `curation`, `metadata`, `users_api` and `backup_api`
/// all admitted one — the administrative routes were being *exercised* by the
/// credential they should have been refusing, through four whole-branch
/// reviews. Renamed rather than deleted: a key is the right client for OPDS
/// and for anything asserting key behaviour, and those callers now say so.
///
/// **Spec A2 §9 refuses a `--no-auth` flag and a dev bypass**, so the suite
/// does not get one either: this is a test *client* that authenticates through
/// the real guard. A request that sets its own `Authorization` is left alone,
/// so a test asserting a *wrong* credential still can.
pub(crate) fn key_authenticating(router: axum::Router) -> axum::Router {
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

/// A `Cookie` header value carrying a real session for account 1.
///
/// [`fixture_authorization`]'s counterpart for a test that builds its own
/// requests and needs to be a **browser** — which, since O93, is what every
/// administrative route requires. `auth::tests` builds its requests by hand
/// against the real `build_router`, so it cannot go through
/// [`authenticating`].
///
/// # Panics
///
/// Panics if the session cannot be opened.
pub(crate) fn fixture_session_cookie(library: &taiyaku_library::Library) -> String {
    let token = taiyaku_library::credential::mint_token();
    {
        let conn = library.conn();
        taiyaku_library::account::open_session(&conn, 1, &token, Some("the suite"), None)
            .expect("opens a fixture session");
    }
    format!("{}={token}", crate::session::COOKIE_NAME)
}

/// The `Extensions` a **browser session** arrives with, for a test that calls
/// a handler directly rather than through the router.
///
/// `session::administering_or_forbidden` asks two questions, and the second
/// reads `auth::By` out of the request's extensions — so a direct call that
/// supplies only a `Principal` is refused for the credential kind, whatever
/// role the principal carries.
#[must_use]
pub(crate) fn from_a_browser() -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(crate::auth::By::Cookie);
    extensions
}

/// Wrap a router so every request presents a real **browser session** — the
/// suite's default, and the client the web API actually has.
///
/// **The suite's default credential is the one kind an administration route
/// refuses**, and that is why this exists. `authenticating` sends
/// [`fixture_authorization`] — a `Basic name:tyk_…` key — so every route test
/// in this crate has always driven a *device* credential. Spec A2's
/// `session::refuse_a_device_credential` turns that away from anything that
/// manages accounts or credentials, so `users_api` and `backup_api` cannot be
/// tested through `authenticating` at all; a router wrapped in it answers 403
/// for a reason that has nothing to do with what the test is about.
///
/// Opens one real session against the fixture library and sends its cookie,
/// through the same guard, exactly as a browser would. A request setting its
/// own `Authorization` or `Cookie` is left alone, so a test asserting the
/// *key's* refusal still can.
///
/// # Panics
///
/// Panics if the session cannot be opened, which would leave every test using
/// it authenticating as nobody.
pub(crate) fn authenticating(
    router: axum::Router,
    library: &taiyaku_library::Library,
) -> axum::Router {
    let token = taiyaku_library::credential::mint_token();
    {
        let conn = library.conn();
        taiyaku_library::account::open_session(&conn, 1, &token, Some("the suite"), None)
            .expect("opens the fixture session");
    }
    let cookie = format!("{}={token}", crate::session::COOKIE_NAME);
    router.layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let cookie = cookie.clone();
            async move {
                let headers = request.headers();
                // **Appended to whatever cookies the request already carries,
                // not skipped when it carries any.** A browser sends every
                // cookie for the host in one header, and a test that sets its
                // own for an unrelated reason — `routes::tests`'s logging test
                // sends `cookie: session=secret` to prove an unlisted header
                // is not recorded — would otherwise be silently unauthenticated
                // and answer 401 about something it is not testing.
                let existing = headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let already_signed_in = existing.contains(crate::session::COOKIE_NAME);
                if !headers.contains_key("authorization") && !already_signed_in {
                    let joined = if existing.is_empty() {
                        cookie.clone()
                    } else {
                        format!("{existing}; {cookie}")
                    };
                    let out = request.headers_mut();
                    out.insert(
                        "cookie",
                        axum::http::HeaderValue::from_str(&joined).expect("ascii"),
                    );
                    // **A matching `host` and `origin` too, because a cookie
                    // without them is not what a browser sends.** Spec A2's
                    // CSRF check refuses a cookie-authenticated `POST`,
                    // `PATCH` or `DELETE` whose `Origin` does not match the
                    // host — correctly — so a fixture that sent only the
                    // cookie would make every write here a 403 about
                    // something the test is not asking about. The same pair
                    // `session/tests.rs` sets by hand on each request.
                    if !out.contains_key("host") {
                        out.insert("host", axum::http::HeaderValue::from_static("test"));
                    }
                    if !out.contains_key("origin") {
                        out.insert(
                            "origin",
                            axum::http::HeaderValue::from_static("http://test"),
                        );
                    }
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
/// Grant the fixture account a root, **demoting it to `user` first**.
///
/// **O86: a grant restricts a user, never an administrator.**
/// `auth::Principal::from_account` drops an admin's roots outright — role
/// dominates the other three permission columns — so granting a root to the
/// seeded account, which is an admin, restricts nothing. A grant test written
/// that way measures nothing at all: it would pass just as well against a
/// filter that had been deleted, which is the exact failure O62's
/// whole-branch review found in `unreadable` and the reason this is one
/// function rather than a line copied into eleven fixtures.
///
/// The demotion is the *point* of the helper, not an incidental step, which is
/// why granting and demoting are not offered separately.
pub(crate) fn grant_root(conn: &rusqlite::Connection, root_id: i64) {
    conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
        .expect("a grant restricts a user, never an admin");
    conn.execute(
        "INSERT INTO user_root (user_id, root_id) VALUES (1, ?1)",
        [root_id],
    )
    .expect("grants the fixture account a root");
}

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

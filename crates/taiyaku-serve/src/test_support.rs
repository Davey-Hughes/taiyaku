use std::io::Write;

use crate::{ServeState, Work};
// The library crate's, so both crates' fixtures file a planted root alike.
pub(crate) use taiyaku_library::test_support::{library_of_root, link, plant_root};

/// A work of two cbz members, four pages, on disk under `root` — a directory
/// the caller's [`Fixture`] owns, so the files go when the test ends.
///
/// Two members deliberately: the offset arithmetic that picks which member
/// holds page N is what most of these tests are about, and one member
/// cannot exercise it. Each entry's bytes are its own name, so a test can
/// tell which page it got.
///
/// Shared rather than local to one test module because several test modules
/// need the same work.
pub(crate) fn work(root: &std::path::Path) -> Work {
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

/// A library good for satisfying `ServeState::library` in a fixture. The
/// progress, notes and truth routes reach it through
/// `works::row_id_for_write`, which mints a bridge row on first write for a
/// `Work` this library was never told about (`state`, below, hands `library()`
/// an empty one paired with an unimported `Work`, exactly that case). An
/// empty, in-memory library is enough: a fresh one per call, since `Library`
/// is not `Clone` and two fixtures sharing a connection would let one test's
/// rows leak into another's.
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
/// **One function rather than a copy in each fixture**, for the ordinary
/// reason: two copies of a fixture's credential are two things to forget to
/// change together.
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
///
/// # Panics
///
/// Panics if the directories cannot be created or the library cannot be
/// opened — a fixture that silently fell back to memory would make every
/// backup test assert the 409 refusal rather than the feature.
pub(crate) fn router_over_a_file_backed_library()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("file-backed-library");
    let library = scanned_file_backed_library(&fixture);
    let works = crate::Works::fixed(
        crate::works::from_library(&library).expect("projects the scanned works"),
    );
    let (router, state) = router_over(library, works, &fixture);
    (router, state, fixture)
}

/// One root, one volume, scanned and derived from a real tree on disk into a
/// file-backed library: what `router_over_a_file_backed_library` and
/// `router_over_a_live_library` serve.
///
/// Laid out as `{fixture}/tree/Scanned/Scanned - v01/` with the database at
/// `{fixture}/db/library.db`, which is where [`add_a_volume_elsewhere`]
/// expects to find both.
///
/// # Panics
///
/// Panics if the tree cannot be written or the library cannot be opened,
/// scanned or derived.
pub(crate) fn scanned_file_backed_library(
    fixture: &Fixture,
) -> std::sync::Arc<taiyaku_library::Library> {
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
    std::sync::Arc::new(library)
}

fn router_over(
    library: std::sync::Arc<taiyaku_library::Library>,
    works: crate::Works,
    fixture: &Fixture,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        cache_dir: fixture.path().join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state)
}

/// [`router_over_a_file_backed_library`], over a **live** registry: the one
/// `serve` with no paths builds.
///
/// # Panics
///
/// As [`scanned_file_backed_library`], and if the scanned works will not
/// project.
pub(crate) fn router_over_a_live_library() -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("live-library");
    let library = scanned_file_backed_library(&fixture);
    let works = crate::Works::library(&library).expect("projects the scanned works");
    let (router, state) = router_over(library, works, &fixture);
    (router, state, fixture)
}

/// A **live** registry over a file-backed library holding one scanned work
/// with two editions and a solved edge between them: what a correction needs,
/// and what a refresh re-reads. Answers the work's slug.
///
/// # Panics
///
/// Panics if the work cannot be imported or projected.
pub(crate) fn router_over_a_live_library_with_an_edge()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture, String) {
    let fixture = Fixture::new("live-library-edge");
    let shelf = fixture.path().to_string_lossy().into_owned();
    let db_dir = fixture.path().join("db");
    std::fs::create_dir_all(&db_dir).expect("creates the database directory");
    let library = taiyaku_library::Library::open(&db_dir.join("library.db"))
        .expect("opens a file-backed library");
    seed_fixture_account(&library);
    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Edged", "reading": "rtl" }},
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
          "alignments": []
        }}"#
    );
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        let work_id =
            taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");
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
    let library = std::sync::Arc::new(library);
    let works = crate::Works::library(&library).expect("projects the work");
    let slug = works.snapshot()[0].id.clone();
    let (router, state) = router_over(library, works, &fixture);
    (router, state, fixture, slug)
}

/// Write `volume` beside the fixture's first volume, then scan and derive it
/// through a **separate** `Library` over the same file, which is another
/// process as far as the server's own connections can tell. Answers the
/// slug the new volume derived to.
///
/// # Panics
///
/// Panics if the volume cannot be written, scanned or derived, or if it
/// derives into the first volume's work rather than one of its own.
pub(crate) fn add_a_volume_elsewhere(fixture: &Fixture, volume: &str) -> String {
    let tree = fixture.path().join("tree");
    write_pages(&tree.join("Scanned").join(volume), 3);
    let other = taiyaku_library::Library::open(&fixture.path().join("db").join("library.db"))
        .expect("a second library over the same file");
    let conn = other.conn();
    let root = taiyaku_library::scan::roots(&conn)
        .expect("lists the roots")
        .into_iter()
        .next()
        .expect("the fixture's one root");
    taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree again");
    taiyaku_library::derive::derive(&conn).expect("derives the new volume");
    let slug: String = conn
        .query_row(
            "SELECT w.slug FROM work w JOIN edition e ON e.work_id = w.id \
             JOIN member m ON m.edition_id = e.id WHERE m.path LIKE ?1",
            [format!("%{volume}%")],
            |row| row.get(0),
        )
        .expect("the new volume derived to a work");
    let first: String = conn
        .query_row(
            "SELECT w.slug FROM work w JOIN edition e ON e.work_id = w.id \
             JOIN member m ON m.edition_id = e.id WHERE m.path LIKE '%Scanned - v01%'",
            [],
            |row| row.get(0),
        )
        .expect("the first volume's work");
    assert_ne!(
        slug, first,
        "the new volume must derive to a work of its own, not join the first"
    );
    slug
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
pub(crate) fn state(root: &std::path::Path) -> ServeState {
    ServeState {
        library: library(),
        works: crate::Works::fixed(vec![work(root)]),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    }
}

/// The shared fixture plus a second edition and a solved edge between
/// them, which is the minimum a correction needs to address.
///
/// `raw` is four pages across two members, as `work` builds it; `en` is
/// the same two members under a second key, so no new archives are needed
/// and the pages a correction addresses really exist.
pub(crate) fn state_with_two_editions(root: &std::path::Path) -> ServeState {
    let mut state = state(root);
    let path = state.works.snapshot()[0].path.clone();
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
    state.works = crate::Works::fixed(vec![crate::Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    )]);
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

/// A temp directory that removes itself when the test that made it ends.
///
/// **`Drop` rather than a cleanup call at the end of each test**, because
/// the runs that leak are the ones that fail: a panicking test never
/// reaches its own last line, and the fixture a failure leaves behind is
/// exactly the one nobody goes back for. A unique name stops tests colliding;
/// it does not stop their directories accumulating.
///
/// An alias of `taiyaku-read`'s `test_support::TempDir` (O117), which every
/// crate's tests share.
pub(crate) type Fixture = taiyaku_read::test_support::TempDir;

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
pub(crate) fn work_with_real_pages() -> (Work, Fixture) {
    // A fixed path cleared at the start would, with Rust running tests in
    // parallel threads, let whichever test ran second delete the first's
    // fixture out from under it mid-test. `Fixture` gives each call a unique
    // name, and someone to remove it.
    let fixture = Fixture::new("real-pages");
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

/// O133's fixture: work `w`, edition `a`, one member of two pages, each
/// 600×900.
///
/// Every library cover size — 256, 512 and 768 on the long edge — is a real
/// resize of these pages, and the filmstrip tile's 160 is a different size
/// from all three, so a test can tell from the image alone which one it was
/// served. `work_with_real_pages`' 16×16 page fits inside every one of them
/// and comes back untouched, which cannot tell them apart.
pub(crate) fn state_with_tall_pages() -> (std::sync::Arc<ServeState>, Fixture) {
    state_with_tall_pages_hashed("tall")
}

/// [`state_with_tall_pages`], its one member recorded with `sha256` as its
/// hash: a sidecar's hash is its own text, which need not be hex.
pub(crate) fn state_with_tall_pages_hashed(sha256: &str) -> (std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("tall-pages");
    let root = fixture.path();
    let page = png(image::RgbImage::from_pixel(
        600,
        900,
        image::Rgb([200, 120, 60]),
    ));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join("a1.cbz")).unwrap());
    for n in 1..=2 {
        zip.start_file(
            format!("p{n}.png"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&page).unwrap();
    }
    zip.finish().unwrap();

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "rtl" }},
          "editions": {{
            "a": {{ "label": "A", "members": [
              {{ "path": "a1.cbz", "sha256": {sha256}, "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "a",
          "alignments": []
        }}"#,
        sha256 = serde_json::to_string(sha256).expect("a hash as JSON")
    );
    let path = root.join("taiyaku.json");
    std::fs::write(&path, &json).unwrap();
    let work = Work::new(
        "w".into(),
        path,
        taiyaku_core::Sidecar::from_json(&json).unwrap(),
    );
    (local_state(work, &fixture), fixture)
}

/// `work` read from disk by `LocalLibrary`, caching under `fixture`'s
/// directory so that the cache goes when the fixture does.
fn local_state(work: Work, fixture: &Fixture) -> std::sync::Arc<ServeState> {
    state_over(work, fixture, std::sync::Arc::new(crate::LocalLibrary))
}

/// [`local_state`], reading pages through `source`.
fn state_over(
    work: Work,
    fixture: &Fixture,
    source: std::sync::Arc<dyn crate::PageSource>,
) -> std::sync::Arc<ServeState> {
    std::sync::Arc::new(ServeState {
        library: library(),
        works: crate::Works::fixed(vec![work]),
        source,
        cache_dir: fixture.path().join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    })
}

/// O108's fixture: one archive that is the second member of one edition and
/// the whole of another, so a page's index in its edition and its offset in
/// its member differ.
///
/// Work `m`. Edition `ab` is `a.cbz` (two pages) then `b.cbz` (three), and
/// edition `b` is `b.cbz` alone. So page 2 of `ab` is `b.cbz`'s first page,
/// the same page as page 0 of `b`, while page 2 of `b` is `b.cbz`'s third. A
/// route that keyed a page by its index would give page 2 of `ab` the
/// tag, the cached rendering and the tile of page 2 of `b`, which is a
/// different picture.
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
    let (work, fixture) = work_with_a_shared_member();
    (local_state(work, &fixture), fixture)
}

/// Every page a [`CountingSource`] was asked for, in order, as the member's
/// path and the page's offset within it.
pub(crate) type Reads = std::sync::Arc<std::sync::Mutex<Vec<(String, u32)>>>;

/// `LocalLibrary`, recording every page it reads.
///
/// A page read twice serves the same bytes as one read, so nothing a test
/// asserts about the response can tell them apart (O107). This records the
/// reads themselves, the member and the offset rather than a count, so a test
/// can say which page was read as well as how often.
#[derive(Debug, Default)]
pub(crate) struct CountingSource {
    reads: Reads,
}

impl crate::PageSource for CountingSource {
    fn page(
        &self,
        root: &std::path::Path,
        member: &taiyaku_core::Member,
        within: u32,
    ) -> Result<crate::ServedPage, String> {
        self.reads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((member.path.clone(), within));
        crate::PageSource::page(&crate::LocalLibrary, root, member, within)
    }
}

/// [`state_with_a_shared_member`], reading through a [`CountingSource`], and
/// the record of what it read.
pub(crate) fn state_with_a_shared_member_and_its_reads()
-> (std::sync::Arc<ServeState>, Fixture, Reads) {
    let (work, fixture) = work_with_a_shared_member();
    let source = CountingSource::default();
    let reads = std::sync::Arc::clone(&source.reads);
    (
        state_over(work, &fixture, std::sync::Arc::new(source)),
        fixture,
        reads,
    )
}

/// The work and the archives behind [`state_with_a_shared_member`].
fn work_with_a_shared_member() -> (Work, Fixture) {
    let fixture = Fixture::new("shared-member");
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
    (work, fixture)
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
pub(crate) fn work_with_edition_key(root: &std::path::Path, key: &str) -> Work {
    let base = work(root);
    let mut sidecar = (*base.sidecar()).clone();
    let edition = sidecar
        .editions
        .remove("raw")
        .expect("fixture defines the `raw` edition");
    sidecar.editions.insert(key.to_string(), edition);
    sidecar.hub = key.to_string();
    Work::new(base.id, base.path, sidecar)
}

/// A request log writing to `requests.jsonl` inside `dir`, which the caller's
/// [`Fixture`] owns, so tests running in parallel cannot read each other's
/// lines.
pub(crate) fn log(dir: &std::path::Path) -> (std::sync::Arc<crate::Log>, std::path::PathBuf) {
    let path = dir.join("requests.jsonl");
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
/// Test scaffolding, and named as such. What production does is `serve_work`,
/// one path at a time, and this is that loop over every row instead of over
/// `argv`, which is exactly what a fixture wants: build the state, then test
/// the routes.
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
/// [`works_in`] — so `Work.id` is the library's own slug and `works::find_row`
/// finds a row on the very first read rather than `works::row_id_for_write`
/// minting one. `state` and `state_over` pair a bare `Work` with an *empty*
/// `library()`, which is the other half of the bridge (see `library()`'s own
/// doc comment); this one is for the tests that are about the library-backed
/// path specifically. Named `w`: the fixture's title is `"W"`, which
/// `import::sidecar`'s `unique_slug` lowercases to exactly that, with no
/// collision to append a suffix against in a fresh library.
///
/// No archives are written to disk — unlike `work`/`state` above, which
/// exist mainly to give the page and thumbnail routes real bytes to read.
/// Nothing that reaches this fixture (progress, notes, truth, the work list)
/// touches a member's bytes, so there is nothing here for a real file to buy.
///
/// The [`Fixture`] is there for `cache_dir` alone, so a cover a test asks for
/// goes when the test does.
pub(crate) async fn router_over_a_library() -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("router-over-a-library");
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
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// The two-edition, one-solved-edge shape a correction needs, over a **real**
/// library: the document is imported through
/// `taiyaku_library::import::sidecar`, so `work.id` is the row's own slug and
/// the alignment rows exist before anything is corrected.
///
/// `state_with_two_editions` above is the same shape paired with an *empty*
/// library; this one is for the tests that are about the rows a correction
/// leaves behind. The sidecar is written to a real path under a `Fixture`
/// directory because `patch_alignments` writes the file as well as the rows,
/// and the restart half of the round trip re-reads exactly those bytes.
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
    let fixture = Fixture::new("correction-rows");
    let (state, work_id) =
        two_editions_in_a_library(&fixture, |_| std::sync::Arc::new(crate::LocalLibrary));
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, work_id, fixture)
}

/// The document, the library and the state behind
/// [`router_over_a_library_with_two_editions`], reading pages through whatever
/// `source` builds from that library.
///
/// One function rather than a copy in
/// [`state_whose_pages_wait_on_the_library`], which needs the same work with a
/// different source, for [`seed_fixture_account`]'s reason.
fn two_editions_in_a_library(
    fixture: &Fixture,
    source: impl FnOnce(
        &std::sync::Arc<taiyaku_library::Library>,
    ) -> std::sync::Arc<dyn crate::PageSource>,
) -> (std::sync::Arc<ServeState>, i64) {
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
    let source = source(&library);
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source,
        cache_dir: fixture.path().join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    (state, work_id)
}

/// [`router_over_a_library_with_two_editions`]'s work with no router, reading
/// its pages through [`PagesWaitOnTheLibrary`]: what
/// `routes::tests::no_works_route_stalls_the_runtime` calls every handler
/// against.
///
/// The two editions and their solved edge are for `patch_alignments`, which
/// needs a pairing to correct; the imported row is what progress, notes and
/// truth read and write against.
///
/// # Panics
///
/// Panics if the fixture cannot be written or imported.
pub(crate) fn state_whose_pages_wait_on_the_library() -> (std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("pages-wait-on-the-library");
    let (state, _work_id) = two_editions_in_a_library(&fixture, |library| {
        std::sync::Arc::new(PagesWaitOnTheLibrary {
            library: std::sync::Arc::clone(library),
        })
    });
    (state, fixture)
}

/// [`state_whose_pages_wait_on_the_library`]'s work under a root, with its
/// archives on disk: what `routes::tests::no_grant_check_stalls_the_runtime`
/// calls every handler that asks a grant against.
///
/// **Root 1 holds every member and root 2 holds none**, so a reader granted
/// root 1 sees the whole work and one granted root 2 sees none of it. An
/// imported member's `root_id` is NULL, which no grant admits: without the
/// `UPDATE` a reader with any grant is refused every call, and a refusal says
/// nothing about where the handler did its work.
///
/// **The archives are written because the download routes read them**, and
/// nothing else here does: pages still come through
/// [`PagesWaitOnTheLibrary`]. Each holds two entries whose bytes are their own
/// names, [`work`]'s shape.
///
/// # Panics
///
/// Panics if the fixture cannot be written or its rows updated.
pub(crate) fn state_whose_grant_waits_on_the_library() -> (std::sync::Arc<ServeState>, Fixture) {
    let (state, fixture) = state_whose_pages_wait_on_the_library();
    for name in ["c1.cbz", "c2.cbz", "d1.cbz", "d2.cbz"] {
        let file = std::fs::File::create(fixture.path().join(name)).expect("creating an archive");
        let mut zip = zip::ZipWriter::new(file);
        for entry in ["1.jpg", "2.jpg"] {
            zip.start_file(entry, zip::write::SimpleFileOptions::default())
                .expect("starting an entry");
            zip.write_all(entry.as_bytes()).expect("writing an entry");
        }
        zip.finish().expect("finishing an archive");
    }
    {
        let conn = state.library.conn();
        plant_root(&conn, 1, "/held");
        plant_root(&conn, 2, "/withheld");
        conn.execute("UPDATE member SET root_id = 1", [])
            .expect("puts every member under root 1");
    }
    (state, fixture)
}

/// A page source that waits on the library's mutex before every read, then
/// serves a real page whatever it was asked for.
///
/// **What brings `get_page` and `get_thumb` into
/// [`the_runtime_stalls_on`]'s view.** For an unrestricted reader neither
/// handler touches `state.library`: what blocks in them is the page read — a
/// zip decompression from disk, or a round trip to Stump — which a watchdog
/// holding the mutex does not reach. Waiting on the same mutex here makes a
/// page read last exactly as long as the watchdog decides.
/// See `docs/notes.md`, "O71".
///
/// The page is [`real_page_bytes`], so a tile can be decoded from it; none of
/// the fixture's archives exist on disk.
#[derive(Debug)]
pub(crate) struct PagesWaitOnTheLibrary {
    library: std::sync::Arc<taiyaku_library::Library>,
}

impl crate::PageSource for PagesWaitOnTheLibrary {
    fn page(
        &self,
        _root: &std::path::Path,
        _member: &taiyaku_core::Member,
        _within: u32,
    ) -> Result<crate::ServedPage, String> {
        drop(self.library.conn());
        Ok(crate::ServedPage {
            content_type: "image/png".to_string(),
            bytes: real_page_bytes(),
        })
    }
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
/// a test can list it before and after a correction and compare. `covers/`
/// appears in it only once something asks for a cover.
///
/// # Panics
///
/// Panics if the fixture cannot be imported, which means the fixture is
/// broken.
pub(crate) fn router_over_a_scanned_work() -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("scanned-correction");
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
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
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
/// The `Fixture` is returned so its `Drop` runs; `std::mem::forget` on it to
/// escape the borrow would skip the cleanup and leak one temp directory per
/// test run.
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
/// `library_api::tests::get_anonymous`'s note for the same trap from the
/// other side.
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
pub(crate) fn write_pages(dir: &std::path::Path, pages: usize) {
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
/// No imported fixture can stand in for this. `import::sidecar` and
/// `import::scanned` write `work` and `series` rows and no `scan_entry` rows at
/// all — and `scan_entry` is what every proposal is raised from, what a
/// candidate rule is evaluated against, and what an accept's collateral guard
/// sweeps. A library imported rather than scanned raises nothing, so a route
/// test over one would assert against an empty inbox whatever the handler did.
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
            fixture.join("tree")
        } else {
            fixture.join("tree").join(parent)
        };
        write_pages(&under.join(title).join(format!("{title} - v01")), pages);
    }
    router_over_a_written_tree(fixture)
}

/// [`router_over_a_scanned_tree`]'s second half, for a fixture that has
/// already written its tree: add the fixture's `tree/` as a root that keeps
/// documents, scan it, derive, and serve the result.
///
/// # Panics
///
/// Panics if the scan or the derive fails, which means the fixture itself is
/// broken.
fn router_over_a_written_tree(
    fixture: Fixture,
) -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let library = library();
    {
        let conn = library.conn();
        let root =
            taiyaku_library::scan::add_root(&conn, &fixture.join("tree")).expect("adds the root");
        // **These fixtures are about the document, so they keep documents.**
        // `keeps_documents` defaults off (migration 012), which is right for a
        // person's library and wrong for a fixture whose whole subject is the
        // file — `patching_a_title_writes_the_row_and_the_document` and its
        // like read it off disk. A fixture that wants the off case turns it off
        // explicitly, which is the rarer thing to say.
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
        works: crate::Works::fixed(works),
        // Beside the scanned tree rather than inside it: one that appeared
        // under the root would be material a later scan walked.
        cache_dir: fixture.join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// One scanned, single-volume work whose member is a real **`.cbz`**, not a
/// directory of pages.
///
/// **Every other scanned fixture here writes a directory of pages**, so its
/// member sniffs as `Format::Dir`. That is right for the routes those fixtures
/// were built for and useless for spec C3 §6's write-back, which only ever
/// touches a zip: against a directory-backed fixture the stamp plan is empty.
///
/// Keeps documents, for the reason [`router_over_a_scanned_tree`] gives.
///
/// # Panics
///
/// Panics if the tree cannot be written or the scan fails.
pub(crate) fn router_over_a_scanned_cbz() -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let prefix = "scanned-cbz";
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
        works: crate::Works::fixed(works),
        // Outside the scanned tree, so nothing this creates is material a
        // later scan walks — `router_over_a_scanned_tree`'s own reasoning.
        cache_dir: fixture.path().join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
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
/// disk. This fixture goes through the real walk instead, the same as
/// `router_over_two_suffixed_series` and its neighbours, which is what gives
/// it an address at all.
pub(crate) fn router_over_a_single_scanned_work()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    router_over_a_scanned_tree("single-scanned-work", "", &["Scanned"])
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
        "two-suffixed-series",
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
        "shared-publisher",
        "Publisher",
        &[
            "Go-Toubun no Hanayome",
            "Go-Toubun no Hanayome - Full Color Edition",
            "Kaguya-sama wa Kokurasetai",
        ],
    )
}

/// Three unrelated series side by side at the root, the first with two
/// volumes — what `library_api::tests`' series-title read path needs: more
/// than one series, so a read that answers some other series' titles is wrong
/// for at least one of them. The tests give each series its titles themselves.
///
/// **Two volumes of the first, so a work's id is not its series' id.**
/// Derivation mints series and works in one order, so with a volume each a
/// work's id would equal its series' id, and a read that took one for the
/// other would answer the right titles anyway. The second Go-Toubun volume
/// moves every later work's id past its series'.
///
/// Keeps documents, for the reason [`router_over_a_scanned_tree`] gives — the
/// round trip there ends in a `PATCH`, which saves every work's document.
pub(crate) fn router_over_three_unrelated_series()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let prefix = "three-unrelated-series";
    let fixture = Fixture::new(prefix);
    for (pages, (title, volume)) in (2_usize..).zip([
        ("Go-Toubun no Hanayome", "v01"),
        ("Go-Toubun no Hanayome", "v02"),
        ("Kaguya-sama wa Kokurasetai", "v01"),
        ("Oshi no Ko", "v01"),
    ]) {
        write_pages(
            &fixture
                .join("tree")
                .join(title)
                .join(format!("{title} - {volume}")),
            pages,
        );
    }
    router_over_a_written_tree(fixture)
}

/// Two scanned roots, each holding an unrelated, single-volume series — the
/// fixture the roots routes' own tests need: a first root to leave standing,
/// and a second whose id names a real row to patch or remove, with a real
/// work under it whose removal costs something nonzero to report.
///
/// **Two independent series, not one split across two roots** — unlike
/// [`router_over_two_scanned_volumes_of_one_series_across_two_roots`] below,
/// deliberately: removing the second root here must delete a whole work
/// (`scan.rs`'s own `DOOMED_WORKS`, "the works this root alone accounts for"),
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
    two_scanned_roots_over(Fixture::new("two-scanned-roots"), |_| library())
}

/// [`state_with_two_scanned_roots`], over a library in a file, so a request's
/// working connection is not the shared one a test writes on meanwhile.
pub(crate) fn state_with_two_scanned_roots_on_disk()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    two_scanned_roots_over(Fixture::new("two-scanned-roots-on-disk"), |fixture| {
        let library = taiyaku_library::Library::open(&fixture.join("library.db"))
            .expect("opens a file-backed library");
        seed_fixture_account(&library);
        std::sync::Arc::new(library)
    })
}

fn two_scanned_roots_over(
    fixture: Fixture,
    library: impl FnOnce(&Fixture) -> std::sync::Arc<taiyaku_library::Library>,
) -> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let root_a = fixture.path().join("root-a");
    let root_b = fixture.path().join("root-b");
    write_pages(&root_a.join("Series A").join("Series A - v01"), 2);
    write_pages(&root_b.join("Series B").join("Series B - v01"), 2);

    let library = library(&fixture);
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
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
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
    let fixture = Fixture::new("two-volumes-one-series");
    let title = "Go-Toubun no Hanayome";
    for (pages, volume) in (2_usize..).zip(["v01", "v02"]) {
        write_pages(
            &fixture
                .join("tree")
                .join(title)
                .join(format!("{title} - {volume}")),
            pages,
        );
    }

    let library = library();
    {
        let conn = library.conn();
        let root =
            taiyaku_library::scan::add_root(&conn, &fixture.join("tree")).expect("adds the root");
        // **These fixtures are about the document, so they keep documents.**
        // `keeps_documents` defaults off (migration 012), which is right for a
        // person's library and wrong for a fixture whose whole subject is the
        // file — `patching_a_title_writes_the_row_and_the_document` and its
        // like read it off disk. A fixture that wants the off case turns it off
        // explicitly, which is the rarer thing to say.
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
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture)
}

/// [`router_over_two_scanned_volumes_of_one_series`]'s tree, with pages that
/// decode: a 16×16 PNG each, every one different so no two members share a
/// `sha256`.
///
/// A cover size is a resize, and the route must read a page's dimensions to
/// size it, which [`write_pages`]' text is not an image to give; at 16 px every
/// page fits each of `cover::COVER_LONG_EDGES` and is served as it is.
///
/// # Panics
///
/// Panics if the tree cannot be written, or if the scan or the derive fails.
pub(crate) fn router_over_two_scanned_volumes_of_decodable_pages()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("two-volumes-decodable");
    let title = "Go-Toubun no Hanayome";
    for (pages, volume) in (2_u8..).zip(["v01", "v02"]) {
        let dir = fixture
            .join("tree")
            .join(title)
            .join(format!("{title} - {volume}"));
        std::fs::create_dir_all(&dir).expect("mkdir");
        for page in 0..pages {
            let colour = image::Rgb([pages, page, 200]);
            std::fs::write(
                dir.join(format!("{page:03}.png")),
                png(image::RgbImage::from_pixel(16, 16, colour)),
            )
            .expect("write");
        }
    }
    router_over_a_written_tree(fixture)
}

/// Two scanned volumes of one series, each rooted under its own directory —
/// the shape `patch_series`'s worst case needs a caller to exist over: a
/// series whose works span two roots (this codebase's own
/// `auth/tests.rs` fixtures model a work or an edition split across two roots
/// for the grant-posture proofs, but none of them attaches two *works* to one
/// series, which is what a per-work document write failing independently of
/// its siblings needs).
///
/// **`root_a`'s path comes back, not `root_b`'s — deliberately the volume
/// `derive` writes (and so `works_in_series` lists) *first*.** `GroupKey`
/// orders groups by `(series, volume, …)`, so `v01`/`root_a` gets the lower
/// `work.id`, and `patch_series`'s own loop visits it before `v02`/`root_b`. A
/// test proving "the worst case, not the last one" has to break the work that
/// is *not* last, or a bug that only remembers the final iteration would pass
/// anyway — the failure would coincide with the last answer by luck of the id
/// order rather than being caught by the assertion. Broken with
/// `taiyaku-library`'s own
/// `hand::tests::a_root_that_cannot_take_the_document_still_arms_the_guard`
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
    let fixture = Fixture::new("two-volumes-two-roots");
    let title = "Go-Toubun no Hanayome";
    let root_a = fixture.path().join("root-a");
    let root_b = fixture.path().join("root-b");
    write_pages(&root_a.join(title).join(format!("{title} - v01")), 2);
    write_pages(&root_b.join(title).join(format!("{title} - v02")), 3);

    let library = library();
    {
        let conn = library.conn();
        let one_library = taiyaku_library::scan::InLibrary::Named("Manga");
        let a =
            taiyaku_library::scan::add_root_in(&conn, &root_a, one_library).expect("adds root a");
        // **These fixtures are about the document, so they keep documents.**
        // `keeps_documents` defaults off (migration 012), which is right for a
        // person's library and wrong for a fixture whose whole subject is the
        // file — `patching_a_title_writes_the_row_and_the_document` and its
        // like read it off disk. A fixture that wants the off case turns it off
        // explicitly, which is the rarer thing to say.
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [a.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &a).expect("scans root a");
        let b =
            taiyaku_library::scan::add_root_in(&conn, &root_b, one_library).expect("adds root b");
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
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
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
/// coincidence — SQLite mints the one series it ever sees as id 1. This one
/// adds a second series, so the series a work moves *out of* is not id 1, and
/// a caller trusting the constant reaches the wrong series' siblings entirely.
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
    let fixture = Fixture::new("two-volumes-plus-unrelated-series");
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
        // the point of scanning it out of title order here. Its series is one
        // a document on the shared series' works could resolve onto, since
        // `librarydoc::find_series_by_title` resolves a title across the store.
        let one_library = taiyaku_library::scan::InLibrary::Named("Manga");
        let a =
            taiyaku_library::scan::add_root_in(&conn, &root_a, one_library).expect("adds root a");
        // **These fixtures are about the document, so they keep documents.**
        // `keeps_documents` defaults off (migration 012), which is right for a
        // person's library and wrong for a fixture whose whole subject is the
        // file — `patching_a_title_writes_the_row_and_the_document` and its
        // like read it off disk. A fixture that wants the off case turns it off
        // explicitly, which is the rarer thing to say.
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [a.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &a).expect("scans root a");
        let b =
            taiyaku_library::scan::add_root_in(&conn, &root_b, one_library).expect("adds root b");
        conn.execute("UPDATE root SET keeps_documents = 1 WHERE id = ?1", [b.id])
            .expect("the fixture keeps documents");
        taiyaku_library::scan::scan_root(&conn, &b).expect("scans root b");
        let unrelated = taiyaku_library::scan::add_root_in(&conn, &root_unrelated, one_library)
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
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture, root_a)
}

/// The ids [`router_over_a_series_in_two_libraries`] answers.
pub(crate) struct SeriesInTwoLibraries {
    pub(crate) series: i64,
    pub(crate) manga: i64,
    pub(crate) kids: i64,
}

/// Series `Foo`, volumes 1 to 4 under the directory of a library `Manga` and
/// volume 5 under the directory of a library `Kids`, scanned and derived as one
/// series: spec §3.6's case, where `Kids`' tile counts one volume of a series
/// that holds five. The fixture account is the administrator, granted both.
///
/// **The libraries are numbered 70 and 80**, apart from the directories, so a
/// narrowing that read a library id as a directory id finds nothing rather
/// than the directory that happens to share its number.
///
/// # Panics
///
/// Panics if either tree cannot be written, if a scan or the derive fails, or
/// if the result is not five works of one series.
pub(crate) fn router_over_a_series_in_two_libraries() -> (
    axum::Router,
    std::sync::Arc<ServeState>,
    Fixture,
    SeriesInTwoLibraries,
) {
    let fixture = Fixture::new("a-series-in-two-libraries");
    let manga_dir = fixture.path().join("manga");
    let kids_dir = fixture.path().join("kids");
    for volume in 1..=4 {
        write_pages(
            &manga_dir.join("Foo").join(format!("Foo - v{volume:02}")),
            2,
        );
    }
    write_pages(&kids_dir.join("Foo").join("Foo - v05"), 2);

    let library = library();
    let series = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'Kids')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        for (dir, name) in [(&manga_dir, "Manga"), (&kids_dir, "Kids")] {
            let root = taiyaku_library::scan::add_root_in(
                &conn,
                dir,
                taiyaku_library::scan::InLibrary::Named(name),
            )
            .unwrap_or_else(|e| panic!("adds {name}'s directory: {e}"));
            taiyaku_library::scan::scan_root(&conn, &root)
                .unwrap_or_else(|e| panic!("scans {name}'s directory: {e}"));
        }
        taiyaku_library::derive::derive(&conn).expect("derives the library");
        let ids: Vec<i64> = conn
            .prepare("SELECT id FROM series")
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .expect("lists the series");
        let [id] = <[i64; 1]>::try_from(ids)
            .unwrap_or_else(|ids| panic!("one series, `Foo`, not {ids:?}"));
        id
    };
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    assert_eq!(
        works.len(),
        5,
        "one series, five volumes: five works, not {}",
        works.len()
    );
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    let ids = SeriesInTwoLibraries {
        series,
        manga: 70,
        kids: 80,
    };
    (router, state, fixture, ids)
}

/// What [`router_over_a_library_nested_in_another`] answers.
pub(crate) struct NestedLibraries {
    /// Library `Manga`, numbered 70, and `Kids`, numbered 80.
    pub(crate) manga: i64,
    pub(crate) kids: i64,
    /// The directories `m` (Manga's) and `m/kids` (Kids').
    pub(crate) m: i64,
    pub(crate) m_kids: i64,
    /// A router presenting the session of an account granted Manga alone, and
    /// one granted Kids alone.
    pub(crate) manga_reader: axum::Router,
    pub(crate) kids_reader: axum::Router,
    /// A router presenting the fixture administrator's session, and the state
    /// all three routers serve.
    pub(crate) admin: axum::Router,
    pub(crate) state: std::sync::Arc<ServeState>,
    /// The tree on disk, removed when this is dropped.
    _fixture: Fixture,
}

/// Spec §4's N: Manga holds `m` and Kids holds `m/kids`, added through the
/// checked add in that order, with one volume in each, `Manga Book` and
/// `Kids Book`.
///
/// Kids' directory alone holds Kids' volume, which the fixture checks. Added
/// first, `m` records it, and Kids' scan takes it over; added second, `m`'s
/// walk skips Kids' directory. Manga's account then sees Kids' book only by
/// reach.
///
/// # Panics
///
/// Panics if the tree cannot be written, a directory cannot be added or
/// scanned, the derive fails, or a volume is held by any directory but its own.
pub(crate) fn router_over_a_library_nested_in_another() -> NestedLibraries {
    nested_libraries(false)
}

/// [`router_over_a_library_nested_in_another`], with Kids' `m/kids` added
/// first, so the nested directory has the lower id.
///
/// # Panics
///
/// As [`router_over_a_library_nested_in_another`].
pub(crate) fn router_over_a_library_nested_in_another_added_first() -> NestedLibraries {
    nested_libraries(true)
}

fn nested_libraries(inner_first: bool) -> NestedLibraries {
    let fixture = Fixture::new("nested-libraries");
    let m = fixture.path().join("m");
    write_pages(&m.join("Manga Book").join("Manga Book - v01"), 2);
    write_pages(&m.join("kids").join("Kids Book").join("Kids Book - v01"), 2);
    let library = library();
    let (manga_reader, kids_reader, outer, inner) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'Kids')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        let add = |dir: &std::path::Path, name: &str| -> i64 {
            let added = taiyaku_library::scan::add_root_checked_in(
                &conn,
                dir,
                taiyaku_library::scan::InLibrary::Named(name),
            )
            .unwrap_or_else(|e| panic!("adds {name}'s directory: {e:?}"));
            taiyaku_library::scan::scan_root(&conn, &added.root)
                .unwrap_or_else(|e| panic!("scans {name}'s directory: {e}"));
            added.root.id
        };
        let (outer, inner) = if inner_first {
            let inner = add(&m.join("kids"), "Kids");
            (add(&m, "Manga"), inner)
        } else {
            let outer = add(&m, "Manga");
            (outer, add(&m.join("kids"), "Kids"))
        };
        taiyaku_library::derive::derive(&conn).expect("derives the library");
        let held: Vec<(i64, i64)> = conn
            .prepare("SELECT root_id, COUNT(*) FROM member_root GROUP BY root_id ORDER BY root_id")
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect()
            })
            .expect("reads who holds what");
        let mut alone = vec![(outer, 1), (inner, 1)];
        alone.sort_unstable();
        assert_eq!(held, alone, "each directory holds its own volume alone");
        let reader = |name: &str, library_id: i64| -> i64 {
            conn.execute(
                "INSERT INTO user (name, password_hash, role) VALUES (?1, ?2, 'user')",
                [name, FIXTURE_PHC],
            )
            .expect("adds a reader");
            let id = conn.last_insert_rowid();
            taiyaku_library::account::grant_library(&conn, id, library_id)
                .expect("grants a library");
            id
        };
        (reader("manga", 70), reader("kids", 80), outer, inner)
    };
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = crate::build_router(state.clone());
    NestedLibraries {
        manga: 70,
        kids: 80,
        m: outer,
        m_kids: inner,
        manga_reader: authenticating_as(router.clone(), &state.library, manga_reader),
        kids_reader: authenticating_as(router.clone(), &state.library, kids_reader),
        admin: authenticating(router, &state.library),
        state,
        _fixture: fixture,
    }
}

/// A `GET` against `router`, driven with `oneshot` and no socket.
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

/// How many series and unfiled works `/api/library/series` lists to account
/// `id`, asked through a session of that account's own.
pub(crate) async fn seen_by(router: &axum::Router, state: &crate::ServeState, id: i64) -> usize {
    use tower::ServiceExt as _;

    let token = taiyaku_library::credential::mint_token();
    {
        let conn = state.library.conn();
        taiyaku_library::account::open_session(&conn, id, &token, Some("the suite"), None)
            .expect("opens the account's session");
    }
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::get("/api/library/series")
                .header(
                    axum::http::header::COOKIE,
                    format!("{}={token}", crate::session::COOKIE_NAME),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let text = body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("json");
    ["series", "unfiled"]
        .iter()
        .map(|key| body[key].as_array().map_or(0, Vec::len))
        .sum()
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

/// The key the suite authenticates with, and the reason the suite is fast.
///
/// **§2's design, used on its own terms.** A key is verified by sha256 and
/// routed past argon2 by its `tyk_` prefix, so a test request costs
/// microseconds where a password costs a full argon2 hash.
///
/// Tests *about* passwords still use `FIXTURE_PASSWORD`, which is the point:
/// they are the ones that should pay for argon2.
pub(crate) const FIXTURE_KEY: &str = "tyk_the-suites-own-key-not-a-real-secret";

/// [`FIXTURE_PASSWORD`], already hashed.
///
/// **Pinned rather than hashed per fixture**, because argon2 runs unoptimized
/// under `cargo test` — this workspace sets no `[profile.dev]` opt-level — so
/// every `state()` call would pay a full hash for a password most tests
/// never present.
///
/// `test_support::pinned::the_fixture_hash_still_verifies` is what stops this
/// drifting from the password beside it.
pub(crate) const FIXTURE_PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$KtgzR4TOZsatunnAHzKmig$fRrVxJJcb/LwTcYO+9XZ4C5uNgdPM131N8iXWxLeXxk";

/// Wrap a router so every request through it presents **an API key**.
///
/// **For OPDS and for anything asserting key behaviour.** Administrative
/// routes refuse a key (O93), so this is not the suite's default.
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
/// requests and needs to be a **browser**, which is what every administrative
/// route requires (O93). `auth::tests` builds its requests by hand against
/// the real `build_router`, so it cannot go through [`authenticating`].
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

/// Drive one handler, called directly, while another thread holds the
/// library's mutex, and report whether the runtime was free to poll anything
/// else meanwhile.
///
/// The watchdog holds the mutex for a controlled span: `Library::conn()` is a
/// `MutexGuard<'_, Connection>`, so a thread holding it blocks any handler
/// that calls it. The block's release is gated on the canary rather than on a
/// fixed sleep, so how long it lasts is decided by whether the reactor was
/// ever free to run anything else — the property under test — rather than by
/// a duration picked in advance.
///
/// **The handler is called directly rather than through the router** (O71), and
/// that is load-bearing. Every request through the router passes `auth::guard`
/// first, and that middleware reads the same library inside its own
/// `spawn_blocking`: it would take the mutex wait onto a blocking thread, free
/// the reactor, let the canary run and let the watchdog release the guard — all
/// before the handler under test ever touched SQLite. The measurement would
/// then come back clean for a handler doing every byte of its work on the async
/// worker, which is a fixture where right and wrong render the same.
///
/// **It sees only a handler that waits on the library's mutex.** A handler
/// whose blocking work is something else — a page read out of an archive — is
/// free whatever the watchdog holds, and passes whether it runs behind
/// `spawn_blocking` or not. [`PagesWaitOnTheLibrary`] is how the page routes
/// are brought into view.
///
/// # Panics
///
/// Panics if the handler panics, or the canary or the watchdog thread does.
pub(crate) async fn the_runtime_stalls_on(
    library: &std::sync::Arc<taiyaku_library::Library>,
    call: impl std::future::Future<Output = axum::response::Response> + Send + 'static,
) -> (bool, axum::response::Response) {
    use std::sync::atomic::{AtomicBool, Ordering};

    let canary_ran = std::sync::Arc::new(AtomicBool::new(false));
    let rescued_by_deadline = std::sync::Arc::new(AtomicBool::new(false));
    let (lock_taken_tx, lock_taken_rx) = std::sync::mpsc::channel();

    let watchdog = {
        let library = std::sync::Arc::clone(library);
        let canary_ran = std::sync::Arc::clone(&canary_ran);
        let rescued_by_deadline = std::sync::Arc::clone(&rescued_by_deadline);
        std::thread::spawn(move || {
            let guard = library.conn();
            lock_taken_tx
                .send(())
                .expect("the test thread is still waiting on this channel");
            // Half a second for a task on an idle runtime to be polled once.
            // Passing costs the first 10 ms of it; only a failure waits the
            // whole way out.
            let mut waited = 0;
            while !canary_ran.load(Ordering::SeqCst) && waited < 50 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                waited += 1;
            }
            if !canary_ran.load(Ordering::SeqCst) {
                rescued_by_deadline.store(true, Ordering::SeqCst);
            }
            drop(guard);
        })
    };
    // Do not call the handler until the guard is confirmed held — otherwise
    // its own blocking closure could win the mutex first and this would
    // measure nothing.
    lock_taken_rx
        .recv()
        .expect("the watchdog thread is still running");

    // Spawned before the canary, so the runtime polls it first: by the time
    // the canary would be scheduled, a handler behind `spawn_blocking` has
    // already blocked on the mutex and a handler on the async worker has
    // stopped the thread the canary would be polled by.
    let request = tokio::spawn(call);
    let canary = tokio::spawn({
        let canary_ran = std::sync::Arc::clone(&canary_ran);
        async move { canary_ran.store(true, Ordering::SeqCst) }
    });

    let response = request.await.expect("the handler did not panic");
    canary.await.unwrap();
    watchdog.join().unwrap();

    (rescued_by_deadline.load(Ordering::SeqCst), response)
}

/// Wrap a router so every request presents a real **browser session** — the
/// suite's default, and the client the web API actually has.
///
/// **The default credential must not be a key**, and that is why this exists.
/// Spec A2's `session::refuse_a_device_credential` turns a *device*
/// credential — a `Basic name:tyk_…` key, [`fixture_authorization`] — away
/// from anything that manages accounts or credentials, so `users_api` and
/// `backup_api` cannot be tested through [`key_authenticating`] at all; a
/// router wrapped in it answers 403 for a reason that has nothing to do with
/// what the test is about.
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
    authenticating_as(router, library, 1)
}

/// [`authenticating`], as the account `user_id` rather than the fixture's own.
///
/// # Panics
///
/// As [`authenticating`].
pub(crate) fn authenticating_as(
    router: axum::Router,
    library: &taiyaku_library::Library,
    user_id: i64,
) -> axum::Router {
    let token = taiyaku_library::credential::mint_token();
    {
        let conn = library.conn();
        taiyaku_library::account::open_session(&conn, user_id, &token, Some("the suite"), None)
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
/// filter that had been deleted, which is the reason this is one function
/// rather than a line copied into each fixture.
///
/// The demotion is the *point* of the helper, not an incidental step, which is
/// why granting and demoting are not offered separately.
///
/// A root is a directory of a library, and a grant names the library. Every
/// fixture root is a library of its own unless it was filed under a named
/// one, so for those this grants exactly that root.
///
/// **The account is set to "only these" with the row**, as
/// `account::grant_library` sets it: a row beside `every_library = 1` waits
/// and restricts nothing, so a fixture leaving the column alone would see
/// every library.
pub(crate) fn grant_root(conn: &rusqlite::Connection, root_id: i64) {
    conn.execute(
        "UPDATE user SET role = 'user', every_library = 0 WHERE id = 1",
        [],
    )
    .expect("a grant restricts a user, never an admin");
    let unlinked: Vec<i64> = conn
        .prepare(
            "SELECT id FROM root
              WHERE NOT EXISTS (SELECT 1 FROM library_root WHERE root_id = root.id)
              ORDER BY id",
        )
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .expect("lists the roots in no library");
    // Every planted root, not this one alone, as the add would file it: a
    // withheld directory is then in a library the grant does not name, so a
    // grant widened past its library is seen to.
    for planted in unlinked {
        taiyaku_library::scan::file_under_its_own_library(conn, planted).unwrap_or_else(|e| {
            panic!(
                "root {planted} cannot be filed, so the fixture would hold a directory in no \
                 library, which no writer leaves: {e}"
            )
        });
    }
    // Not an aggregate, which answers one row even for a root with no link.
    let granted = conn
        .execute(
            "INSERT INTO user_library (user_id, library_id)
             SELECT 1, library_id FROM library_root WHERE root_id = ?1
              ORDER BY library_id LIMIT 1",
            [root_id],
        )
        .expect("grants the fixture account the root's library");
    assert_eq!(
        granted, 1,
        "the fixture account is granted root {root_id}'s library"
    );
}

/// Three works: one under each directory of the two-directory library `Shelf`, and
/// one under `Other`. The fixture account is granted `Shelf` through
/// [`grant_root`] on its first directory, so a test reads both of Shelf's works
/// and neither of Other's.
///
/// **Library ids differ from root ids** (70 and 80 against 1, 2 and 3), so a
/// grant that resolved a library id as if it were a root id would see one of
/// Shelf's works and not the other.
///
/// # Panics
///
/// Panics if a work cannot be imported or filed, or the grant cannot be made.
pub(crate) fn router_over_a_granted_two_dir_library(
    dir: &std::path::Path,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    let shelf = dir.to_string_lossy().into_owned();
    let library = library();
    {
        let conn = library.conn();
        conn.execute_batch(
            "INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
             INSERT INTO root (id, path) VALUES (1, '/shelf-a'), (2, '/shelf-b'), (3, '/other');
             INSERT INTO library_root (library_id, root_id) VALUES (70, 1), (70, 2), (80, 3);",
        )
        .expect("a library of two directories and one of one");
        for (root, title, file) in [
            (1, "First Directory Work", "first"),
            (2, "Second Directory Work", "second"),
            (3, "Other Library Work", "other"),
        ] {
            let json = format!(
                r#"{{
                  "taiyaku": 1,
                  "work": {{ "title": "{title}", "reading": "rtl" }},
                  "editions": {{
                    "raw": {{ "label": "Raw", "members": [
                      {{ "path": "{shelf}/{file}.cbz", "sha256": "{file}", "pages": 2 }}
                    ], "spreads": [] }}
                  }},
                  "hub": "raw",
                  "alignments": []
                }}"#
            );
            let doc = taiyaku_core::Sidecar::from_json(&json).expect("the fixture sidecar parses");
            let work = taiyaku_library::import::scanned(&conn, &doc, None).expect("imports");
            conn.execute(
                "UPDATE member SET root_id = ?1 WHERE edition_id IN \
                 (SELECT id FROM edition WHERE work_id = ?2)",
                [root, work],
            )
            .expect("files the work under its directory");
        }
        grant_root(&conn, 1);
    }
    let works = crate::works::from_library(&library).expect("projects the imported works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: dir.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state)
}

/// One work whose editions sit in two libraries: its hub edition `en` under
/// `Manga`'s directory and `raw` under `Other`'s. The derive spans the store
/// (spec §3.2), so the two directories' volumes are one work whichever library
/// holds each directory.
///
/// The fixture account is granted `Manga` alone, so the work is partly visible
/// to it: `en` and not `raw` (spec §1, "What an account sees"). `Other` is
/// made first, as library 90, so neither library's id is a root's id and a
/// grant read as a root id would show. Answers the work's slug.
///
/// # Panics
///
/// Panics if the tree cannot be written, a scan or the derive fails, or the
/// derive does not leave the one work described.
pub(crate) fn router_over_a_work_whose_editions_sit_in_two_libraries()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture, String) {
    work_whose_editions_sit_in_two_libraries(Granted::Hub)
}

/// [`router_over_a_work_whose_editions_sit_in_two_libraries`]'s work, with the
/// fixture account granted `Other` alone: the work is reached through its
/// edition `raw`, which is not the hub, and `en` is outside the grant.
///
/// # Panics
///
/// As [`router_over_a_work_whose_editions_sit_in_two_libraries`].
pub(crate) fn router_over_a_work_granted_through_its_other_edition()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture, String) {
    work_whose_editions_sit_in_two_libraries(Granted::Other)
}

/// Which of the two libraries the fixture account is granted.
enum Granted {
    Hub,
    Other,
}

fn work_whose_editions_sit_in_two_libraries(
    granted: Granted,
) -> (axum::Router, std::sync::Arc<ServeState>, Fixture, String) {
    let fixture = Fixture::new("two-library-work");
    let hub = fixture.path().join("hub");
    let other = fixture.path().join("other");
    write_pages(&hub.join("Series").join("Series - v01"), 2);
    write_pages(&other.join("Series").join("Series - v01"), 3);

    let library = library();
    let slug = {
        let conn = library.conn();
        conn.execute("INSERT INTO library (id, name) VALUES (90, 'Other')", [])
            .expect("a library for the other directory");
        let h = taiyaku_library::scan::add_root_in(
            &conn,
            &hub,
            taiyaku_library::scan::InLibrary::Named("Manga"),
        )
        .expect("adds hub");
        let o = taiyaku_library::scan::add_root_in(
            &conn,
            &other,
            taiyaku_library::scan::InLibrary::Id(90),
        )
        .expect("adds other");
        taiyaku_library::scan::update_root(
            &conn,
            h.id,
            &taiyaku_library::scan::RootPatch {
                edition_key: Some(Some("en".to_string())),
                ..Default::default()
            },
        )
        .expect("the hub directory's edition sorts first, so it is the hub");
        for root in taiyaku_library::scan::roots(&conn).expect("lists the roots") {
            taiyaku_library::scan::scan_root(&conn, &root).expect("scans");
        }
        taiyaku_library::derive::derive(&conn).expect("derives one work of two editions");

        let slugs: Vec<String> = conn
            .prepare("SELECT slug FROM work")
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .expect("lists the works");
        let [slug] = <[String; 1]>::try_from(slugs).expect("the fixture: one work in the store");
        let placed: Vec<(String, i64, i64)> = conn
            .prepare(
                "SELECT e.key, m.root_id, r.library_id FROM edition e
                   JOIN work w ON w.id = e.work_id
                   JOIN member m ON m.edition_id = e.id
                   JOIN library_root r ON r.root_id = m.root_id
                  WHERE w.slug = ?1 ORDER BY e.key",
            )
            .and_then(|mut stmt| {
                stmt.query_map([&slug], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .expect("lists where the work's members are");
        assert_eq!(
            placed,
            [
                ("en".to_string(), h.id, h.libraries[0]),
                ("raw".to_string(), o.id, 90)
            ],
            "the fixture: `en`'s member under Manga's directory, `raw`'s under Other's"
        );
        assert_ne!(h.libraries[0], 90, "the fixture: two libraries");
        grant_root(
            &conn,
            match granted {
                Granted::Hub => h.id,
                Granted::Other => o.id,
            },
        );
        slug
    };
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture, slug)
}

/// Who [`router_over_an_edition_merged_from_two_libraries`] answers as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reader {
    /// Granted `Manga` alone: `raw`'s first and third members, `en` and `fr`.
    MangaOnly,
    /// Granted `Kids` alone: `raw`'s second member and nothing else.
    KidsOnly,
    /// Granted both libraries, so sees everything through `Access::Only`.
    Both,
    /// The fixture account, an administrator: everything through `Access::Every`.
    Administrator,
}

/// What [`router_over_an_edition_merged_from_two_libraries`] answers.
pub(crate) struct EditionMergedFromTwoLibraries {
    pub(crate) state: std::sync::Arc<ServeState>,
    pub(crate) slug: String,
    readers: Vec<(Reader, i64, axum::Router)>,
    /// The tree on disk, removed when this is dropped.
    _fixture: Fixture,
}

impl EditionMergedFromTwoLibraries {
    /// A router presenting `reader`'s session.
    pub(crate) fn router(&self, reader: Reader) -> &axum::Router {
        &self.reader(reader).2
    }

    /// `reader`'s account id.
    pub(crate) fn user_id(&self, reader: Reader) -> i64 {
        self.reader(reader).1
    }

    /// `request` as `reader`, its body read whole.
    pub(crate) async fn send(
        &self,
        reader: Reader,
        request: axum::http::Request<axum::body::Body>,
    ) -> axum::http::Response<Vec<u8>> {
        use tower::ServiceExt as _;
        let response = self
            .router(reader)
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, 1 << 20)
            .await
            .expect("reads the body");
        axum::http::Response::from_parts(parts, bytes.to_vec())
    }

    /// `GET uri` as `reader`, with `If-None-Match` when given.
    pub(crate) async fn get(
        &self,
        reader: Reader,
        uri: &str,
        if_none_match: Option<&str>,
    ) -> axum::http::Response<Vec<u8>> {
        let mut request = axum::http::Request::get(uri);
        if let Some(tag) = if_none_match {
            request = request.header(axum::http::header::IF_NONE_MATCH, tag);
        }
        self.send(reader, request.body(axum::body::Body::empty()).unwrap())
            .await
    }

    fn reader(&self, reader: Reader) -> &(Reader, i64, axum::Router) {
        self.readers
            .iter()
            .find(|(who, _, _)| *who == reader)
            .expect("every reader has an account")
    }
}

/// The solid colour of page `page` of the `archive`th archive
/// [`router_over_an_edition_merged_from_two_libraries`] writes, 40 apart in a
/// channel from every other page's, so a thumb or a cover drawn from the wrong
/// page reads as another colour.
fn merged_page_colour(archive: u8, page: u8) -> image::Rgb<u8> {
    image::Rgb([20 + 40 * archive, 20 + 40 * page, 128])
}

/// One volume, `Foo v01`, as one work of editions `raw`, `en` and `fr`, with
/// `raw` merged from two libraries (spec §3.8, O239): Manga's `m` holds part1
/// (2 pages) and part3 (4), Kids' `k` part2 (3); Manga's `m-en` and `m-fr`
/// hold `en` (5) and `fr` (6). So `raw`'s full pages are part1 0–1, part2 2–4
/// and part3 5–8, and Manga alone's hidden member is not the last. Alignments
/// `raw`↔`en` (over `raw`'s full 9 slots) and `en`↔`fr` carry counts, each page
/// is an image of its own colour, and libraries 70 and 80 are numbered apart
/// from the directories.
///
/// # Panics
///
/// Panics if the tree cannot be written, a scan or the derive fails, or the
/// store is not the shape above.
pub(crate) fn router_over_an_edition_merged_from_two_libraries() -> EditionMergedFromTwoLibraries {
    router_over_an_edition_merged_from_two_libraries_with_part2_named("Foo v01 part2.cbz")
}

/// [`router_over_an_edition_merged_from_two_libraries`] with part2's file
/// named `part2`: a zip whatever its name, which the scan reads by its bytes,
/// so a name with another extension gives Kids alone one member whose
/// passthrough is typed by that extension.
///
/// # Panics
///
/// As [`router_over_an_edition_merged_from_two_libraries`].
pub(crate) fn router_over_an_edition_merged_from_two_libraries_with_part2_named(
    part2: &str,
) -> EditionMergedFromTwoLibraries {
    let fixture = Fixture::new("merged-edition");
    let dir = |name: &str| fixture.path().join(name);
    for (archive, (root, file, pages)) in (0_u8..).zip([
        ("m", "Foo v01 part1.cbz", 2_u8),
        ("m", "Foo v01 part3.cbz", 4),
        ("m-en", "Foo v01.cbz", 5),
        ("m-fr", "Foo v01.cbz", 6),
        ("k", part2, 3),
    ]) {
        std::fs::create_dir_all(dir(root)).expect("mkdir");
        let file_handle = std::fs::File::create(dir(root).join(file)).expect("creates the archive");
        let mut zip = zip::ZipWriter::new(file_handle);
        for page in 0..pages {
            zip.start_file(
                format!("{page:03}.png"),
                zip::write::SimpleFileOptions::default(),
            )
            .expect("starts an entry");
            zip.write_all(&png(image::RgbImage::from_pixel(
                16,
                16,
                merged_page_colour(archive, page),
            )))
            .expect("writes an entry");
        }
        zip.finish().expect("finishes the archive");
    }

    let library = library();
    let (slug, accounts) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'Kids')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        for (root, library_id, key) in [
            ("m", 70, None),
            ("m-en", 70, Some("en")),
            ("m-fr", 70, Some("fr")),
            ("k", 80, None),
        ] {
            let added = taiyaku_library::scan::add_root_in(
                &conn,
                &dir(root),
                taiyaku_library::scan::InLibrary::Id(library_id),
            )
            .unwrap_or_else(|e| panic!("adds {root}: {e}"));
            if let Some(key) = key {
                taiyaku_library::scan::update_root(
                    &conn,
                    added.id,
                    &taiyaku_library::scan::RootPatch {
                        edition_key: Some(Some(key.to_string())),
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|e| panic!("keys {root} as `{key}`: {e}"));
            }
        }
        for root in taiyaku_library::scan::roots(&conn).expect("lists the roots") {
            taiyaku_library::scan::scan_root(&conn, &root).expect("scans");
        }
        taiyaku_library::derive::derive(&conn).expect("derives one work of three editions");

        let (work_id, slug): (i64, String) = conn
            .query_row("SELECT id, slug FROM work", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("the fixture: one work in the store");
        let placed: Vec<(String, String, i64, i64)> = conn
            .prepare(
                "SELECT e.key, m.path, m.pages, r.library_id FROM edition e
                   JOIN member m ON m.edition_id = e.id
                   JOIN library_root r ON r.root_id = m.root_id
                  WHERE e.work_id = ?1 ORDER BY e.key, m.ordinal",
            )
            .and_then(|mut stmt| {
                stmt.query_map([work_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?
                .collect()
            })
            .expect("lists where the work's members are");
        let placed: Vec<(&str, &str, i64, i64)> = placed
            .iter()
            .map(|(key, path, pages, library_id)| {
                let file = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a member names its file");
                (key.as_str(), file, *pages, *library_id)
            })
            .collect();
        assert_eq!(
            placed,
            [
                ("en", "Foo v01.cbz", 5, 70),
                ("fr", "Foo v01.cbz", 6, 70),
                ("raw", "Foo v01 part1.cbz", 2, 70),
                ("raw", part2, 3, 80),
                ("raw", "Foo v01 part3.cbz", 4, 70),
            ],
            "the fixture: `raw` merged from Manga's and Kids' directories, part2 in Kids"
        );

        for (from, to, slots, paired) in [("raw", "en", 9, 7), ("en", "fr", 5, 5)] {
            conn.execute(
                "INSERT INTO alignment \
                     (work_id, from_key, to_key, by, verdict, slots, paired, avg_run) \
                 VALUES (?1, ?2, ?3, 'hand', 'sound', ?4, ?5, 2.0)",
                rusqlite::params![work_id, from, to, slots, paired],
            )
            .expect("seeds an alignment");
            conn.execute(
                "INSERT INTO run \
                     (alignment_id, ordinal, a_start_page, a_end_page, b_start_page, b_end_page) \
                 VALUES (?1, 0, 0, 1, 0, 1)",
                [conn.last_insert_rowid()],
            )
            .expect("seeds a run pairing the first two pages straight across");
        }

        let mut accounts = vec![(Reader::Administrator, 1)];
        for (reader, name, libraries) in [
            (Reader::MangaOnly, "manga", &[70][..]),
            (Reader::KidsOnly, "kids", &[80][..]),
            (Reader::Both, "both", &[70, 80][..]),
        ] {
            conn.execute(
                "INSERT INTO user (name, password_hash, role) VALUES (?1, ?2, 'user')",
                [name, FIXTURE_PHC],
            )
            .expect("adds a reader");
            let id = conn.last_insert_rowid();
            for library_id in libraries {
                taiyaku_library::account::grant_library(&conn, id, *library_id)
                    .expect("grants a library");
            }
            accounts.push((reader, id));
        }
        (slug, accounts)
    };
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = crate::build_router(state.clone());
    let readers = accounts
        .into_iter()
        .map(|(reader, id)| {
            (
                reader,
                id,
                authenticating_as(router.clone(), &state.library, id),
            )
        })
        .collect();
    EditionMergedFromTwoLibraries {
        state,
        slug,
        readers,
        _fixture: fixture,
    }
}

/// Spec §1's copy: one volume's pages under directory `b`, in library `B`, and
/// the same bytes at the same place under `a`, in library `A`, scanned and
/// derived into one work of one member with a `member_root` row for each.
///
/// `b` is added first, so its copy is the primary one (`member.root_id`), and
/// the fixture account is granted `A` alone: it reaches the member only
/// through `a`'s `member_root` row. Answers the work's slug.
///
/// # Panics
///
/// Panics if the tree cannot be written, a scan or the derive fails, or the
/// derive does not leave the one member described.
pub(crate) fn router_over_a_copy_granted_through_its_second_directory()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture, String) {
    let fixture = Fixture::new("copy-in-two-libraries");
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let volume = std::path::Path::new("Series").join("Series - v01");
    write_pages(&b.join(&volume), 2);
    std::fs::create_dir_all(a.join(&volume)).expect("mkdir");
    for page in std::fs::read_dir(b.join(&volume)).expect("lists b's pages") {
        let page = page.expect("a page").path();
        std::fs::copy(
            &page,
            a.join(&volume).join(page.file_name().expect("named")),
        )
        .expect("copies a page");
    }

    let library = library();
    let slug = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        let rb = taiyaku_library::scan::add_root_in(
            &conn,
            &b,
            taiyaku_library::scan::InLibrary::Named("B"),
        )
        .expect("adds b");
        let ra = taiyaku_library::scan::add_root_in(
            &conn,
            &a,
            taiyaku_library::scan::InLibrary::Named("A"),
        )
        .expect("adds a");
        for root in [&rb, &ra] {
            taiyaku_library::scan::scan_root(&conn, root).expect("scans");
        }
        taiyaku_library::derive::derive(&conn).expect("derives one work of one member");
        let (slug, root_id, roots): (String, i64, String) = conn
            .query_row(
                "SELECT w.slug, m.root_id,
                        (SELECT group_concat(root_id) FROM
                           (SELECT root_id FROM member_root WHERE member_id = m.id
                             ORDER BY root_id))
                   FROM work w JOIN edition e ON e.work_id = w.id
                   JOIN member m ON m.edition_id = e.id",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("the fixture: one work of one member");
        assert_eq!(
            (root_id, roots),
            (rb.id, format!("{},{}", rb.id, ra.id)),
            "the fixture: the primary copy b's, held under both"
        );
        grant_root(&conn, ra.id);
        slug
    };
    let works = crate::works::from_library(&library).expect("projects the scanned works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state, fixture, slug)
}

mod pinned {
    /// The pinned hash and the password beside it must stay one pair.
    ///
    /// A constant that stopped matching would make every fixture account
    /// unauthenticatable, and the failure would surface as dozens of unrelated
    /// route tests answering 401 rather than as anything naming the cause.
    #[test]
    fn the_fixture_hash_still_verifies() {
        use taiyaku_library::credential::{WorkingMemory, verify_password_in};

        assert!(verify_password_in(
            &mut WorkingMemory::new(),
            super::FIXTURE_PASSWORD,
            super::FIXTURE_PHC
        ));
        assert!(!verify_password_in(
            &mut WorkingMemory::new(),
            "not the fixture password",
            super::FIXTURE_PHC
        ));
    }
}

/// [`crate::ServeState::library_job`], held by a thread of its own until this
/// is dropped. A library's scan takes the lock before it walks anything, so
/// one reserved and spawned meanwhile waits, and a test sees it running for as
/// long as it likes. A thread, not a guard in the test: a guard held across the
/// test's awaits is what `clippy::await_holding_lock` refuses.
pub(crate) struct HeldJob {
    release: Option<std::sync::mpsc::Sender<()>>,
    holder: Option<std::thread::JoinHandle<()>>,
}

impl Drop for HeldJob {
    fn drop(&mut self) {
        drop(self.release.take());
        if let Some(holder) = self.holder.take() {
            holder.join().expect("the holder thread ends");
        }
    }
}

pub(crate) fn holding_the_job(state: &std::sync::Arc<crate::ServeState>) -> HeldJob {
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (locked_tx, locked) = std::sync::mpsc::channel::<()>();
    let state = std::sync::Arc::clone(state);
    let holder = std::thread::spawn(move || {
        let _job = state.library_job.lock().expect("the job lock is free");
        locked_tx.send(()).expect("the test is waiting");
        let _ = released.recv();
    });
    locked.recv().expect("the holder took the lock");
    HeldJob {
        release: Some(release),
        holder: Some(holder),
    }
}

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

/// The fixture work, wired to a `LocalLibrary`, ready for `build_router`.
pub(crate) fn state(dir: &str) -> ServeState {
    ServeState {
        works: vec![work(dir)],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: std::env::temp_dir().join(dir).join("covers"),
        log: None,
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
        works: vec![work],
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        log: None,
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

use super::StumpConfig;

/// `UreqTransport::new` trims a trailing slash for the transport; the
/// resolution cache key has to agree, or `https://nas:10801/` and
/// `https://nas:10801` are one instance to every request and two to the cache
/// — silently discarding every prior resolution the first time someone happens
/// to type the slash.
#[test]
fn a_trailing_slash_is_trimmed_once_at_construction() {
    let with_slash = StumpConfig::new("https://nas:10801/", "key");
    let without = StumpConfig::new("https://nas:10801", "key");

    assert_eq!(with_slash.base, without.base);
    assert_eq!(with_slash.base, "https://nas:10801");
}

/// A one-member work whose archive name is distinct from every other
/// fixture's, so a `FakeTransport`'s recorded request bodies can tell
/// which work triggered them — `crate::test_support::work` reuses
/// `c1.cbz`/`c2.cbz` for every caller, which would make two such works
/// indistinguishable in a GraphQL request body.
fn distinct_work(dir: &std::path::Path, member_name: &str) -> crate::Work {
    use std::io::Write;

    let root = dir;

    let archive = format!("{member_name}.cbz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(&archive)).unwrap());
    zip.start_file("p1.jpg", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"p1.jpg").unwrap();
    zip.finish().unwrap();

    let json = format!(
        r#"{{
              "taiyaku": 1,
              "work": {{ "title": "{member_name}", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "Raw", "members": [
                  {{ "path": "{archive}", "sha256": "{member_name}-sha", "pages": 1 }}
                ], "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": []
            }}"#
    );
    let path = root.join("taiyaku.json");
    std::fs::write(&path, &json).unwrap();

    crate::Work::new(
        member_name.to_string(),
        path,
        taiyaku_core::Sidecar::from_json(&json).unwrap(),
    )
}

/// `--refresh-stump` applies to every work it was handed, not just the
/// first, and a work with nothing recorded is not a failure — the flag is
/// typed once and most works in a library will have no cache file, so one
/// refusing would be the worse answer.
#[test]
fn refreshing_forgets_every_work_and_tolerates_one_with_nothing_recorded() {
    let tmp = crate::test_support::Fixture::new("lib-forget-recorded");
    let recorded = distinct_work(&tmp, "recorded");
    let tmp2 = crate::test_support::Fixture::new("lib-forget-untouched");
    let untouched = distinct_work(&tmp2, "untouched");
    let mut resolutions = crate::stump::cache::Resolutions::new();
    resolutions.insert("hash".into(), Some("book-1".into()));
    crate::stump::cache::store(&recorded, "http://nas:10801", &resolutions).unwrap();

    super::forget_all_works([&recorded, &untouched]);

    let gone = !crate::stump::cache::path_for(&recorded).exists();
    let still_absent = !crate::stump::cache::path_for(&untouched).exists();
    assert!(gone, "the recorded work's cache file must be removed");
    assert!(still_absent, "and the one that never had one is untouched");
}

/// One work whose file cannot be removed must not stop the works after it.
/// A library is served from several sidecars and one read-only directory
/// among them must not silently leave every later work still believing a
/// stale resolution.
///
/// Staged by taking write permission off one work's *directory*, since on
/// Unix that is what governs deleting a file within it. Under root, or a
/// filesystem ignoring the mode bits, that does not hold and the test
/// would pass without measuring anything — so it says so and skips.
#[test]
fn one_work_that_cannot_be_forgotten_does_not_stop_the_others() {
    let tmp = crate::test_support::Fixture::new("lib-forget-stuck");
    let stuck = distinct_work(&tmp, "stuck");
    let tmp2 = crate::test_support::Fixture::new("lib-forget-after");
    let after = distinct_work(&tmp2, "after");
    let mut resolutions = crate::stump::cache::Resolutions::new();
    resolutions.insert("hash".into(), Some("book-1".into()));
    for work in [&stuck, &after] {
        crate::stump::cache::store(work, "http://nas:10801", &resolutions).unwrap();
    }

    let stuck_dir = stuck.path.parent().unwrap().to_path_buf();
    let mut locked = std::fs::metadata(&stuck_dir).unwrap().permissions();
    locked.set_readonly(true);
    std::fs::set_permissions(&stuck_dir, locked).unwrap();
    let deletable_anyway = std::fs::remove_file(crate::stump::cache::path_for(&stuck)).is_ok();

    if deletable_anyway {
        let mut open = std::fs::metadata(&stuck_dir).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        open.set_readonly(false);
        let _ = std::fs::set_permissions(&stuck_dir, open);
        eprintln!(
            "NOT MEASURED: one_work_that_cannot_be_forgotten_does_not_stop_the_others \
                 (a read-only directory still allows deletion here)"
        );
        return;
    }

    super::forget_all_works([&stuck, &after]);

    let stuck_remains = crate::stump::cache::path_for(&stuck).exists();
    let later_forgotten = !crate::stump::cache::path_for(&after).exists();
    // Restore before cleanup: `remove_dir_all` needs to unlink the files
    // inside, which the locked directory forbids.
    let mut open = std::fs::metadata(&stuck_dir).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    open.set_readonly(false);
    let _ = std::fs::set_permissions(&stuck_dir, open);

    assert!(
        stuck_remains,
        "the precondition: this work's file genuinely could not be removed"
    );
    assert!(
        later_forgotten,
        "a work after the failure must still be forgotten"
    );
}

/// O8: resolution reports each work as it finishes rather than handing
/// back one aggregate at the end.
///
/// If every work's answers arrived together, a twenty-work library would serve
/// all twenty from disk until the twentieth had been asked about. Reporting per
/// work makes the first work usable while the last is still being resolved.
#[test]
fn each_work_is_learned_as_it_finishes_rather_than_all_at_the_end() {
    use crate::stump::http::test_support::FakeTransport;

    let tmp = crate::test_support::Fixture::new("lib-progressive-first");
    let first = distinct_work(&tmp, "alpha");
    let tmp2 = crate::test_support::Fixture::new("lib-progressive-second");
    let second = distinct_work(&tmp2, "bravo");
    // Stump answers, and what it answers is "no such book" — an answer
    // rather than an outage, so the breaker does not fire and both works
    // are asked about.
    let fake = FakeTransport::new().on_post("/api/graphql", r#"{"data":{"media":{"nodes":[]}}}"#);

    let mut batches: Vec<Vec<String>> = Vec::new();
    super::resolve_all_works(
        &fake,
        &[first, second],
        "http://nas:10801",
        &mut |learned| {
            let mut keys: Vec<String> = learned.keys().cloned().collect();
            keys.sort();
            batches.push(keys);
        },
    );

    assert_eq!(
        batches.len(),
        2,
        "one report per work, not one at the end: {batches:?}"
    );
    assert_eq!(
        batches[0],
        vec!["alpha-sha".to_string()],
        "the first work's answers must arrive before the second is asked about"
    );
    assert_eq!(batches[1], vec!["bravo-sha".to_string()]);
}

/// The circuit breaker must span works, not stop at the boundary of the first
/// one. Against an unreachable instance, the second work's member must never
/// be asked about at all — proven by asserting on the fake's recorded
/// requests, not merely on what was learned.
#[test]
fn resolve_all_works_stops_asking_after_the_first_transport_failure() {
    use crate::stump::http::test_support::FakeTransport;

    let tmp = crate::test_support::Fixture::new("lib-resolve-all-first");
    let first = distinct_work(&tmp, "alpha");
    let tmp2 = crate::test_support::Fixture::new("lib-resolve-all-second");
    let second = distinct_work(&tmp2, "bravo");
    // No handlers registered, so every POST is a transport-level error —
    // standing in for an unreachable instance, the convention every
    // other test in this crate uses.
    let unreachable = FakeTransport::new();

    let mut resolutions = crate::stump::cache::Resolutions::new();
    super::resolve_all_works(
        &unreachable,
        &[first, second],
        "http://nas:10801",
        &mut |learned| resolutions.extend(learned),
    );

    let requests = unreachable.requests();
    assert!(
        resolutions.is_empty(),
        "nothing was resolved: {resolutions:?}"
    );
    assert_eq!(
        requests.len(),
        1,
        "only the first work's first (failing) member should be attempted, not one \
             lookup per work: {requests:?}"
    );
    assert!(
        !requests.iter().any(|r| r.contains("bravo")),
        "the second work must never be asked about once the first aborted by \
             transport failure: {requests:?}"
    );
}

/// The tile cache is a *sibling* of the cover cache, not the same
/// directory and not a child of it — the arrangement `thumb_cache_dir`'s
/// own doc comment argues for, on the grounds that only one of the two is
/// cheap to rebuild. `thumb_cache_dir` could return `PathBuf::default()`,
/// the empty path, and every tile in every library would share one directory
/// in the process's working directory.
#[test]
fn the_tile_cache_is_a_sibling_of_the_cover_cache() {
    let tmp = crate::test_support::Fixture::new("thumb-dir");
    let state = crate::test_support::state(&tmp);
    let covers = state.cache_dir.clone();
    let thumbs = state.thumb_cache_dir();

    assert_ne!(thumbs, covers, "not the same directory");
    assert_eq!(
        thumbs.parent(),
        covers.parent(),
        "siblings: same parent, different leaf"
    );
    assert_eq!(thumbs.file_name().unwrap(), "thumbs");
    assert!(
        !thumbs.as_os_str().is_empty(),
        "an empty path would put every library's tiles in one place"
    );
}

/// The sibling invariant above, re-asserted at a *resolved* root. The two are
/// different claims: that one holds `cache_dir` fixed from the fixture, this
/// one takes whatever `cache_root` produced and checks the tiles still land
/// beside the covers and inside the application's directory.
///
/// The derivation touches no file at all, so the work is an empty `Vec` here;
/// a work's archives are built, and the paths exercised against real bytes,
/// by `serve_caches_covers_and_tiles_under_the_resolved_root` below.
#[test]
fn a_resolved_root_keeps_covers_and_thumbs_as_siblings() {
    let state = crate::ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(Vec::new()),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: crate::cache_root::cache_root(
            None,
            Some(std::ffi::OsStr::new("/var/cache/fixture")),
            None,
        )
        .covers(),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    };
    let covers = state.cache_dir.clone();
    let thumbs = state.thumb_cache_dir();

    assert_eq!(
        covers,
        std::path::PathBuf::from("/var/cache/fixture/taiyaku/covers")
    );
    assert_eq!(
        thumbs,
        std::path::PathBuf::from("/var/cache/fixture/taiyaku/thumbs"),
        "tiles land beside the covers, inside the application's own directory"
    );
    assert_eq!(covers.parent(), thumbs.parent(), "siblings");
}

/// A fixture makes its directory up front and removes it when the scope
/// that held it ends. Both halves are asserted: a `Drop` that removed a
/// directory which had never been created would pass an
/// existence-after check and prove nothing.
#[test]
fn a_fixture_removes_its_directory_when_it_is_dropped() {
    let path = {
        let fixture = crate::test_support::Fixture::new("drop-guard");
        let path = fixture.path().to_path_buf();
        assert!(path.is_dir(), "the fixture creates its directory up front");
        path
    };
    assert!(
        !path.exists(),
        "and removes it when the scope that held it ends: {}",
        path.display()
    );
}

/// **And it removes it when the scope unwinds**, since a panicking test never
/// reaches a cleanup line.
///
/// `catch_unwind` rather than `#[should_panic]`, because the assertion
/// has to run *after* the unwind rather than being the thing that panics.
#[test]
fn a_fixture_removes_its_directory_when_the_scope_unwinds() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured = std::sync::Arc::clone(&seen);
    let outcome = std::panic::catch_unwind(move || {
        let fixture = crate::test_support::Fixture::new("drop-unwind");
        *captured.lock().unwrap() = Some(fixture.path().to_path_buf());
        panic!("as a failing assertion would");
    });

    assert!(outcome.is_err(), "the closure must actually have panicked");
    let path = seen
        .lock()
        .unwrap()
        .clone()
        .expect("the fixture was made before the panic");
    assert!(
        !path.exists(),
        "unwinding must still remove it: {}",
        path.display()
    );
}

/// Aborting on the *last* work must say nothing: there is nothing left to
/// skip, and a sentence promising to skip "0 work(s)" — or, with the
/// subtraction mutated, some other number of works that do not exist — is
/// worse than silence.
///
/// The abort test above asserts which requests were made, which is the
/// right thing for it to assert and cannot see any of this.
#[test]
fn the_skipping_notice_counts_only_the_works_actually_left() {
    assert_eq!(
        crate::skipping_notice(1, 0),
        None,
        "gave up on the only work: nothing follows it"
    );
    assert_eq!(
        crate::skipping_notice(3, 2),
        None,
        "gave up on the last of three: nothing follows it either"
    );

    let two_left = crate::skipping_notice(3, 0).expect("two works follow the first");
    assert!(
        two_left.contains("remaining 2 work(s)"),
        "must count what is left, not what there was: {two_left}"
    );
    let one_left = crate::skipping_notice(3, 1).expect("one work follows the second");
    assert!(one_left.contains("remaining 1 work(s)"), "{one_left}");
}

/// `serve` actually binds and answers.
///
/// The one thing in this crate that opens a socket, and so — by the design
/// its own doc comment states — the one thing a `oneshot` router test
/// cannot drive. `serve_caches_covers_and_tiles_under_the_resolved_root`
/// below covers the second thing about this function that only a socket
/// can observe.
///
/// The port is taken by binding and releasing, which is racy in principle;
/// the poll below is what makes it deterministic in practice, and a genuine
/// collision fails loudly rather than passing wrongly.
///
/// **`multi_thread` is required, not stylistic.** `ureq` is a blocking
/// client — deliberately, per `stump::http`'s doc — so polling with it on
/// the default current-thread runtime blocks the very runtime that is
/// supposed to be driving `serve`, and the test deadlocks rather than failing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_binds_the_address_and_answers_on_it() {
    let tmp = crate::test_support::Fixture::new("serve-bind");
    let works = vec![crate::test_support::work(&tmp)];

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);

    // The cache root is pinned by the flag rather than left to resolve,
    // so this test neither reads the environment it happens to run under
    // nor can write a cache into a real `$HOME`. It lives under the
    // fixture's own temp directory, which goes when `tmp` is dropped.
    let cache = tmp.join("cache");

    let serving = tokio::spawn({
        let addr = addr.clone();
        async move {
            crate::serve(
                crate::test_support::library(),
                crate::Works::fixed(works),
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });

    // Poll rather than sleep a fixed time: the bind is the thing under
    // test, so waiting for it is the assertion.
    let url = format!("http://{addr}/api/works");
    let mut answered = None;
    for _ in 0..100 {
        if serving.is_finished() {
            break;
        }
        if let Ok(mut r) = ureq::get(&url)
            .header(
                "authorization",
                crate::test_support::fixture_authorization(),
            )
            .call()
        {
            answered = Some(r.body_mut().read_to_string().unwrap_or_default());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    serving.abort();

    let body = answered.expect("serve must bind the address and answer on it");
    assert!(
        body.contains("\"t\""),
        "and answer with the works it was given: {body}"
    );
}

/// O186: a scan an earlier process cut during its derive reads `interrupted`
/// by the time `serve` answers, and a finished one keeps its status.
///
/// The registry is fixed, so this `serve` starts no scan of its own.
/// `multi_thread` and the poll are `serve_binds_the_address_and_answers_on_it`'s,
/// for its reasons.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_marks_a_scan_cut_during_its_derive_interrupted_before_it_listens() {
    let tmp = crate::test_support::Fixture::new("serve-interrupted");
    let works = vec![crate::test_support::work(&tmp)];
    let library = crate::test_support::library();
    let (cut, done) = {
        let conn = library.conn();
        let add = |name: &str, status: &str| {
            let dir = tmp.join(name);
            std::fs::create_dir_all(&dir).expect("makes the directory");
            let root = taiyaku_library::scan::add_root(&conn, &dir).expect("adds the directory");
            conn.execute(
                "UPDATE root SET last_scan_status = ?2 WHERE id = ?1",
                rusqlite::params![root.id, status],
            )
            .expect("sets the status");
            root.id
        };
        (add("cut", "deriving"), add("done", "ok"))
    };

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = tmp.join("cache");

    let serving = tokio::spawn({
        let addr = addr.clone();
        let library = std::sync::Arc::clone(&library);
        async move {
            crate::serve(
                library,
                crate::Works::fixed(works),
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });

    let url = format!("http://{addr}/api/works");
    let mut answered = false;
    for _ in 0..100 {
        if serving.is_finished() {
            break;
        }
        if ureq::get(&url)
            .header(
                "authorization",
                crate::test_support::fixture_authorization(),
            )
            .call()
            .is_ok()
        {
            answered = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();
    assert!(answered, "serve must bind the address and answer on it");

    let conn = library.conn();
    let status = |id: i64| -> Option<String> {
        conn.query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("reads the status")
    };
    assert_eq!(
        (status(cut), status(done)),
        (Some("interrupted".to_string()), Some("ok".to_string()))
    );
}

/// Spec §4: a live `serve` scans every directory that is not Off once it
/// listens, with no press and before any is due.
///
/// In memory, so the startup job holds the one connection from its first walk
/// to its derive, and a poll never reads a status between them. `multi_thread`
/// for the reason `serve_binds_the_address_and_answers_on_it` gives.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_scans_each_directory_but_an_off_one_once_it_listens() {
    let tmp = crate::test_support::Fixture::new("serve-startup-scan");
    let library = crate::test_support::library();
    let (a, b) = {
        let conn = library.conn();
        let add = |name: &str, series: &str| {
            let dir = tmp.join(name);
            crate::test_support::write_pages(&dir.join(series).join(format!("{series} - v01")), 2);
            taiyaku_library::scan::add_root(&conn, &dir)
                .expect("adds the directory")
                .id
        };
        let (a, b) = (add("a", "Alpha"), add("b", "Beta"));
        conn.execute("UPDATE root SET scan_interval_secs = 0 WHERE id = ?1", [b])
            .expect("sets b to Off");
        (a, b)
    };
    let works = crate::Works::library(&library).expect("projects the empty library");

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = tmp.join("cache");

    let serving = tokio::spawn({
        let library = std::sync::Arc::clone(&library);
        async move {
            crate::serve(
                library,
                works,
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });

    let status = |id: i64| -> Option<String> {
        library
            .conn()
            .query_row(
                "SELECT last_scan_status FROM root WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .expect("reads the status")
    };
    let mut scanned = false;
    for _ in 0..500 {
        if serving.is_finished() {
            break;
        }
        if status(a).as_deref() == Some("ok") {
            scanned = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();
    assert!(
        scanned,
        "serve never scanned a, which is not Off, within ten seconds"
    );
    assert_eq!(status(b), None, "b is Off, so serve never scanned it");
}

/// `serve` hands `ServeOptions::web` on to the router, which the router's own
/// tests cannot see: they call `build_router_serving` with a directory
/// themselves. Through a socket, a deep link answers the app's `index.html`.
///
/// `multi_thread` and the poll are `serve_binds_the_address_and_answers_on_it`'s,
/// for its reasons.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_serves_the_web_app_it_is_given() {
    let tmp = crate::test_support::Fixture::new("serve-web");
    let works = vec![crate::test_support::work(&tmp)];
    let dist = tmp.join("dist");
    std::fs::create_dir_all(&dist).expect("makes the app directory");
    std::fs::write(dist.join("index.html"), "<!doctype html><p>the app</p>")
        .expect("writes index.html");

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = tmp.join("cache");

    let serving = tokio::spawn({
        let addr = addr.clone();
        async move {
            crate::serve(
                crate::test_support::library(),
                crate::Works::fixed(works),
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    web: Some(dist),
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });

    let url = format!("http://{addr}/read/x");
    let mut answered = None;
    for _ in 0..100 {
        if serving.is_finished() {
            break;
        }
        if let Ok(mut r) = ureq::get(&url).call() {
            answered = Some(r.body_mut().read_to_string().unwrap_or_default());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();

    let body = answered.expect("serve must answer a deep link when it is given an app");
    assert!(body.contains("the app"), "{body}");
}

/// `serve` builds its `ServeState` from the *resolved* root, and generated
/// files really land under it.
///
/// The sibling tests assign `cache_dir` themselves, and the bind test asks for
/// `/api/works`, which writes nothing. This one goes through a socket rather
/// than a `oneshot` router: `serve` is where the resolution meets the state,
/// and a test that builds the state itself cannot see that line at all.
///
/// **Both leaves, not just the covers.** A cover exercises `cache_dir`
/// directly and a tile exercises `thumb_cache_dir`, which is derived from it —
/// so this also pins the sibling arrangement against a root that was resolved
/// rather than typed into a fixture, with real bytes on both sides.
///
/// The fixture's pages are real PNGs (`work_with_real_pages`) because both
/// routes decode: the shared two-member fixture's "jpegs" are their own
/// filenames, and every request here would 404 on the decode instead of
/// writing anything.
///
/// It also checks that `serve` *creates* the root rather than leaving the
/// first request to, which is checkable only from here, before any request
/// that would create it in passing.
///
/// `multi_thread` and the poll are `serve_binds_the_address_and_answers_on_it`'s,
/// for its reasons.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_caches_covers_and_tiles_under_the_resolved_root() {
    let (work, fixture) = crate::test_support::work_with_real_pages();
    let sha = work
        .sidecar()
        .editions
        .get("a")
        .expect("the fixture's one edition")
        .members[0]
        .sha256
        .clone();

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);

    // Pinned by the flag under the fixture's own temp directory: the test
    // must not read the environment it happens to run under, and must not
    // be able to write into a real `$HOME`.
    let cache = fixture.path().join("cache");
    let serving = tokio::spawn({
        let addr = addr.clone();
        let cache = cache.clone();
        async move {
            crate::serve(
                crate::test_support::library(),
                crate::Works::fixed(vec![work]),
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });

    let mut bound = false;
    for _ in 0..100 {
        if serving.is_finished() {
            break;
        }
        if ureq::get(&format!("http://{addr}/api/works"))
            .header(
                "authorization",
                crate::test_support::fixture_authorization(),
            )
            .call()
            .is_ok()
        {
            bound = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // Read *before* any request that would create it lazily, which is what
    // makes it an observation of `cache_root::create` being called at all
    // rather than of `cover.rs` making the directory on its way past.
    // `serve` binds after it creates, so an answered request means it ran.
    let created_at_startup = cache.join("covers").is_dir();

    // `ureq` reports a non-2xx as an `Err` by default (see
    // `stump::http::get_bytes`, which turns that off deliberately), so the
    // status is carried as the message a failure prints rather than
    // unwrapped here.
    let cover = ureq::get(&format!("http://{addr}/opds/v1.2/works/w/a/cover"))
        .header(
            "authorization",
            crate::test_support::fixture_authorization(),
        )
        .call()
        .map(|r| r.status().as_u16())
        .map_err(|e| e.to_string());
    let tile = ureq::get(&format!("http://{addr}/api/works/w/a/pages/0/thumb"))
        .header(
            "authorization",
            crate::test_support::fixture_authorization(),
        )
        .call()
        .map(|r| r.status().as_u16())
        .map_err(|e| e.to_string());

    // Read before the assertions, so a failing one still leaves no server
    // running. The temp tree is `fixture`'s business: it goes when this
    // function returns *or* unwinds, so a failing assertion does not leak it.
    let expected_cover = crate::cover::cache_path(&cache.join("covers"), &sha);
    let expected_tile = crate::cover::thumb_cache_path(
        &cache.join("thumbs"),
        &sha,
        0,
        crate::cover::THUMB_LONG_EDGE,
    );
    let cover_cached = expected_cover.is_file();
    let tile_cached = expected_tile.is_file();

    serving.abort();

    assert!(bound, "serve must bind before anything else is asserted");
    assert!(
        created_at_startup,
        "Important 5: the cache root is created at startup, not by whichever request \
             first happens to want it: {}",
        cache.join("covers").display()
    );
    assert_eq!(cover, Ok(200), "the cover route must have answered");
    assert_eq!(tile, Ok(200), "the tile route must have answered");
    assert!(
        cover_cached,
        "the generated cover must be written under the root `serve` resolved, not \
             under a path baked into the state: {}",
        expected_cover.display()
    );
    assert!(
        tile_cached,
        "and the tile beside it, in the sibling `thumbs` under the same root: {}",
        expected_tile.display()
    );
}

/// **`serve` keeps a live registry current on its own.** `/api/works` is a
/// listing, so it never takes a lookup's miss path; only the refresher
/// `serve` spawns can make a volume derived after startup appear in it.
///
/// `multi_thread` for the reason
/// `serve_binds_the_address_and_answers_on_it` gives: `ureq` blocks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_keeps_a_live_library_current_without_a_request_naming_the_work() {
    let fixture = crate::test_support::Fixture::new("serve-live");
    let library = crate::test_support::scanned_file_backed_library(&fixture);
    let works = crate::Works::library(&library).expect("projects the scanned works");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = fixture.path().join("cache");

    let serving = tokio::spawn({
        let addr = addr.clone();
        async move {
            crate::serve(
                library,
                works,
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");

    let url = format!("http://{addr}/api/works");
    let wanted = format!("\"{slug}\"");
    let mut listed = false;
    for _ in 0..250 {
        if serving.is_finished() {
            break;
        }
        if let Ok(mut r) = ureq::get(&url)
            .header(
                "authorization",
                crate::test_support::fixture_authorization(),
            )
            .call()
            && r.body_mut()
                .read_to_string()
                .unwrap_or_default()
                .contains(&wanted)
        {
            listed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();
    assert!(listed, "`/api/works` never listed {slug}");
}

/// **`serve` sweeps a live library's cache on its own** (O11): a cover no
/// member holds goes without any request, and so does the member's own cover,
/// which is over `cache_max` alone and goes only if the cap reaches the sweep.
/// Of two staging files, the one dated an hour before the start goes and the
/// one dated an hour ahead stays, so the cut-off is taken at the start.
#[tokio::test]
async fn serve_sweeps_a_live_library_s_cache_once_it_is_listening() {
    let fixture = crate::test_support::Fixture::new("serve-sweeps");
    let library = crate::test_support::scanned_file_backed_library(&fixture);
    let works = crate::Works::library(&library).expect("projects the scanned works");
    let digest: String = library
        .conn()
        .query_row("SELECT sha256 FROM member", [], |row| row.get(0))
        .expect("reads the one member's digest");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = fixture.path().join("cache");
    let covers = cache.join("covers");
    let unheld = crate::cover::cache_path(&covers, &"0123456789abcdef".repeat(4));
    let held = crate::cover::cache_path(&covers, &digest);
    std::fs::create_dir_all(&covers).expect("makes the covers directory");
    std::fs::write(&unheld, b"x").expect("plants an unheld cover");
    std::fs::write(&held, [0; 100]).expect("plants the member's cover");
    let (leftover, young) = (
        taiyaku_read::atomic::temp_path(&held, "jpg"),
        taiyaku_read::atomic::temp_path(&held, "jpg"),
    );
    let (now, hour) = (
        std::time::SystemTime::now(),
        std::time::Duration::from_secs(3600),
    );
    for (staging, modified) in [(&leftover, now - hour), (&young, now + hour)] {
        std::fs::write(staging, b"x").expect("plants a staging file");
        std::fs::File::options()
            .write(true)
            .open(staging)
            .and_then(|file| file.set_modified(modified))
            .expect("dates a staging file");
    }

    let serving = tokio::spawn(async move {
        crate::serve(
            library,
            works,
            crate::ServeOptions {
                addr,
                cache: Some(cache),
                cache_max: 99,
                ..crate::ServeOptions::default()
            },
        )
        .await
    });
    for _ in 0..250 {
        if serving.is_finished() || !(unheld.exists() || held.exists() || leftover.exists()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();
    assert!(!unheld.exists(), "{} was never swept", unheld.display());
    assert!(!held.exists(), "{} was never evicted", held.display());
    assert!(!leftover.exists(), "{} was never swept", leftover.display());
    assert!(young.exists(), "{} was swept", young.display());
}

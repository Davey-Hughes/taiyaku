use super::*;
use crate::stump::http::test_support::FakeTransport;

/// The fixture's `c1.cbz` holds `a1.jpg` whose bytes are its own name, so
/// a fake serving those bytes is a book that genuinely agrees, and one
/// serving anything else is one that does not.
fn reply_with(size: u64, pages: u32) -> String {
    format!(
        r#"{{"data":{{"media":{{"nodes":[{{"id":"book-1","name":"c1","path":"/data/c1.cbz","size":{size},"pages":{pages}}}]}}}}}}"#
    )
}

/// As `reply_with`, but naming the book and its id explicitly — for tests
/// where the second member must find a candidate that actually matches
/// its own stem, rather than being refused by `find_book`'s name check.
fn reply_for(id: &str, name: &str, size: u64, pages: u32) -> String {
    format!(
        r#"{{"data":{{"media":{{"nodes":[{{"id":"{id}","name":"{name}","path":"/data/{name}.cbz","size":{size},"pages":{pages}}}]}}}}}}"#
    )
}

/// The whole point: a candidate is not believed until a page proves it.
#[test]
fn a_candidate_confirmed_by_a_page_hash_is_recorded() {
    let work = crate::test_support::work("taiyaku-test-resolve-ok");
    let root = work.path.parent().unwrap();
    let size = std::fs::metadata(root.join("c1.cbz")).unwrap().len();

    let fake = FakeTransport::new()
        .on_post("/api/graphql", &reply_with(size, 2))
        .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a1.jpg");

    let resolved = resolve_work(&fake, &work, "http://nas:10801", &mut |_, _, _| {}).resolutions;

    let sidecar = work.sidecar();
    let first = sidecar.editions["raw"].members[0].sha256.clone();
    let second_sha = sidecar.editions["raw"].members[1].sha256.clone();
    crate::test_support::cleanup("taiyaku-test-resolve-ok");
    assert_eq!(resolved.get(&first), Some(&Some("book-1".to_string())));
    // The second member (`c2.cbz`) is asked about against the same
    // single-book reply, which names `c1` — so `find_book`'s own name
    // check refuses it before the page hash is ever reached. That is an
    // `Err`, not an answer, so it must be absent from the map rather than
    // cached as `None`.
    assert_eq!(
        resolved.get(&second_sha),
        None,
        "a candidate refused by find_book's name check is not an answer, so not cached"
    );
    assert_eq!(resolved.len(), 1, "only the confirmed member is cached");
}

/// A book that matches by name, size and page count but serves different
/// bytes is refused. This is the case the corpus's mislabelled files and
/// duplicate sets make real, and accepting it would show the wrong page
/// while every hash in the sidecar still matched.
#[test]
fn a_candidate_whose_page_disagrees_is_refused() {
    let work = crate::test_support::work("taiyaku-test-resolve-mismatch");
    let root = work.path.parent().unwrap();
    let size = std::fs::metadata(root.join("c1.cbz")).unwrap().len();

    let fake = FakeTransport::new()
        .on_post("/api/graphql", &reply_with(size, 2))
        .on_get(
            "/api/v2/media/book-1/page/1",
            "image/jpeg",
            b"a-different-page",
        );

    let resolved = resolve_work(&fake, &work, "http://nas:10801", &mut |_, _, _| {}).resolutions;

    let sidecar = work.sidecar();
    let first = sidecar.editions["raw"].members[0].sha256.clone();
    let second_sha = sidecar.editions["raw"].members[1].sha256.clone();
    let requests = fake.requests();
    crate::test_support::cleanup("taiyaku-test-resolve-mismatch");
    // Pin the path taken, not just the outcome: deleting the `on_get`
    // registration above would make `page_bytes` fail instead of
    // returning disagreeing bytes, which is a *different* outcome now
    // that a transport failure is no longer cached as `None` — so assert
    // the page comparison actually happened.
    assert!(
        requests
            .iter()
            .any(|r| r == "GET /api/v2/media/book-1/page/1"),
        "the disagreement must be reached by actually comparing page bytes: {requests:?}"
    );
    assert_eq!(
        resolved.get(&first),
        Some(&None),
        "a disagreeing page must record absence, not the book"
    );
    // As above: the second member is refused by find_book's name check
    // before any page is compared, which is not an answer either.
    assert_eq!(
        resolved.get(&second_sha),
        None,
        "a candidate refused by find_book's name check is not cached"
    );
    assert_eq!(
        resolved.len(),
        1,
        "only the genuinely-answered member is cached"
    );
}

/// Resolution runs once. A second call reads the cache and asks Stump
/// nothing, which is what stops a page turn costing a GraphQL round trip.
///
/// Both members must genuinely resolve here — one via `on_post_sequence`
/// answering each member's own query with its own book — because after
/// Important 1's fix an `Err` (such as the name-mismatch the other tests
/// exercise) is never cached, and an uncached member would be retried on
/// the second call, silently reintroducing the request this test exists
/// to prove never happens.
#[test]
fn a_second_resolution_asks_stump_nothing() {
    let work = crate::test_support::work("taiyaku-test-resolve-cached");
    let root = work.path.parent().unwrap();
    let size1 = std::fs::metadata(root.join("c1.cbz")).unwrap().len();
    let size2 = std::fs::metadata(root.join("c2.cbz")).unwrap().len();

    let fake = FakeTransport::new()
        .on_post_sequence(
            "/api/graphql",
            &[
                &reply_for("book-1", "c1", size1, 2),
                &reply_for("book-2", "c2", size2, 2),
            ],
        )
        .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a1.jpg")
        .on_get("/api/v2/media/book-2/page/1", "image/jpeg", b"b1.jpg");
    resolve_work(&fake, &work, "http://nas:10801", &mut |_, _, _| {});

    let second = FakeTransport::new();
    let resolved = resolve_work(&second, &work, "http://nas:10801", &mut |_, _, _| {}).resolutions;

    let sidecar = work.sidecar();
    let first = sidecar.editions["raw"].members[0].sha256.clone();
    let second_sha = sidecar.editions["raw"].members[1].sha256.clone();
    crate::test_support::cleanup("taiyaku-test-resolve-cached");
    assert!(
        second.requests().is_empty(),
        "a cached work must not ask again: {:?}",
        second.requests()
    );
    assert_eq!(resolved.get(&first), Some(&Some("book-1".to_string())));
    assert_eq!(resolved.get(&second_sha), Some(&Some("book-2".to_string())));
    assert_eq!(resolved.len(), 2, "both members must be cached");
}

/// A transport failure is not an answer, and must not be cached as one.
///
/// Resolving once against a `FakeTransport` with no handlers registered —
/// so every call errors — must leave the member's key absent from the
/// cache, not written as `None`. Caching a failure to ask as "Stump does
/// not have this" would permanently disable Stump for this work after one
/// bad start: the next run, with Stump reachable, would see the key
/// already present and never ask again.
#[test]
fn a_transport_failure_is_not_cached_and_is_retried() {
    let work = crate::test_support::work("taiyaku-test-resolve-transport-failure");
    let root = work.path.parent().unwrap();
    let size = std::fs::metadata(root.join("c1.cbz")).unwrap().len();
    let first = work.sidecar().editions["raw"].members[0].sha256.clone();

    let unreachable = FakeTransport::new();
    let first_pass = resolve_work(&unreachable, &work, "http://nas:10801", &mut |_, _, _| {});
    assert!(
        first_pass.aborted_by_transport,
        "an unreachable instance must be reported as a transport-level abort"
    );
    assert_eq!(
        first_pass.resolutions.get(&first),
        None,
        "a transport failure must not be cached as an answer"
    );

    let reachable = FakeTransport::new()
        .on_post("/api/graphql", &reply_with(size, 2))
        .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a1.jpg");
    let second_pass = resolve_work(&reachable, &work, "http://nas:10801", &mut |_, _, _| {});

    crate::test_support::cleanup("taiyaku-test-resolve-transport-failure");
    assert!(
        !second_pass.aborted_by_transport,
        "a successful run must not report a transport abort"
    );
    assert_eq!(
        second_pass.resolutions.get(&first),
        Some(&Some("book-1".to_string())),
        "a later start with stump reachable must still resolve the member"
    );
}

/// Important 2 (whole-branch review): a semantic refusal — Stump
/// answering ambiguously, not an outage — must not trip the circuit
/// breaker the way a transport failure does. Before this fix, `Err` from
/// `resolve_member` broke `'lookups` unconditionally, so the *first*
/// member's ambiguous match would have silently abandoned the second
/// member too, even though Stump was perfectly reachable and answered
/// the second member's own query just fine.
#[test]
fn a_semantic_refusal_does_not_abandon_the_rest_of_the_run() {
    let work = crate::test_support::work("taiyaku-test-resolve-semantic-refusal");
    let root = work.path.parent().unwrap();
    let size2 = std::fs::metadata(root.join("c2.cbz")).unwrap().len();

    // The first member's query returns two ambiguous candidates — a
    // refusal, not a transport failure. The second member's own query
    // (via `on_post_sequence`) finds and confirms a genuine match.
    let ambiguous = r#"{"data":{"media":{"nodes":[
            {"id":"one","name":"c1","path":"/a/c1.cbz","size":1,"pages":2},
            {"id":"two","name":"c1","path":"/b/c1.cbz","size":1,"pages":2}
        ]}}}"#;
    let fake = FakeTransport::new()
        .on_post_sequence(
            "/api/graphql",
            &[ambiguous, &reply_for("book-2", "c2", size2, 2)],
        )
        .on_get("/api/v2/media/book-2/page/1", "image/jpeg", b"b1.jpg");

    let resolved = resolve_work(&fake, &work, "http://nas:10801", &mut |_, _, _| {});

    let second_sha = work.sidecar().editions["raw"].members[1].sha256.clone();
    crate::test_support::cleanup("taiyaku-test-resolve-semantic-refusal");
    assert!(
        !resolved.aborted_by_transport,
        "an ambiguous match is Stump answering, not an outage"
    );
    assert_eq!(
        resolved.resolutions.get(&second_sha),
        Some(&Some("book-2".to_string())),
        "the second member must still be asked about after the first was refused, not \
             abandoned the way a transport failure abandons the run"
    );
}

/// The retry behaviour above must stay, but a whole run must not pay one
/// failed lookup per member against an unreachable Stump. Once one
/// member's resolution fails, the rest of this run's unresolved members
/// are left untried — not attempted and failed, simply never asked —
/// which is what keeps a large library's startup from costing minutes of
/// `ureq` timeouts before the listener binds.
#[test]
fn a_resolution_failure_abandons_the_rest_of_the_run() {
    let work = crate::test_support::work("taiyaku-test-resolve-circuit-breaker");
    let unreachable = FakeTransport::new();

    let resolved = resolve_work(&unreachable, &work, "http://nas:10801", &mut |_, _, _| {});

    let requests = unreachable.requests();
    crate::test_support::cleanup("taiyaku-test-resolve-circuit-breaker");
    assert!(
        resolved.aborted_by_transport,
        "an unreachable instance must be reported as a transport-level abort"
    );
    assert!(
        resolved.resolutions.is_empty(),
        "nothing was resolved: {:?}",
        resolved.resolutions
    );
    assert_eq!(
        requests.len(),
        1,
        "only the first (failing) member's lookup should be attempted, \
             not one per unresolved member: {requests:?}"
    );
}

/// O8: resolution runs before the listener binds, so a work whose members
/// all have to be asked about is time the server spends unable to answer
/// anything. It printed one summary line per work, *after* that work
/// finished, so a large work was silent for its whole duration and there
/// was no way to tell a slow resolution from a hung one.
///
/// The other half matters as much: a run that asks about nothing must say
/// nothing. A cached start is the common case — it is the whole point of
/// the cache — and a progress line per member on every start would be
/// noise that trains you to ignore it.
#[test]
fn resolution_reports_each_member_it_asks_about_and_says_nothing_when_it_asks_nothing() {
    let work = crate::test_support::work("taiyaku-test-resolve-progress");
    let root = work.path.parent().unwrap();
    let size1 = std::fs::metadata(root.join("c1.cbz")).unwrap().len();
    let size2 = std::fs::metadata(root.join("c2.cbz")).unwrap().len();

    let fake = FakeTransport::new()
        .on_post_sequence(
            "/api/graphql",
            &[
                &reply_for("book-1", "c1", size1, 2),
                &reply_for("book-2", "c2", size2, 2),
            ],
        )
        .on_get("/api/v2/media/book-1/page/1", "image/jpeg", b"a1.jpg")
        .on_get("/api/v2/media/book-2/page/1", "image/jpeg", b"b1.jpg");

    let mut cold: Vec<(String, usize, usize)> = Vec::new();
    resolve_work(
        &fake,
        &work,
        "http://nas:10801",
        &mut |member, nth, total| cold.push((member.to_string(), nth, total)),
    );

    let mut warm: Vec<(String, usize, usize)> = Vec::new();
    resolve_work(
        &FakeTransport::new(),
        &work,
        "http://nas:10801",
        &mut |member, nth, total| warm.push((member.to_string(), nth, total)),
    );

    crate::test_support::cleanup("taiyaku-test-resolve-progress");
    assert_eq!(
        cold,
        vec![("c1.cbz".to_string(), 1, 2), ("c2.cbz".to_string(), 2, 2),],
        "a cold run names each member as it asks, and how many it is asking about"
    );
    assert!(
        warm.is_empty(),
        "a run that asks nothing must report nothing: {warm:?}"
    );
}

/// As `test_support::work`, but with a second edition — so a test can
/// tell `break 'lookups` (abandons the whole run) apart from a plain
/// inner-loop `break` (abandons only the edition it happened in). A
/// single-edition fixture cannot: both stop in the same place when there
/// is only one edition to abandon.
///
/// `"edition-a"` sorts before `"edition-b"` in the `BTreeMap` iteration
/// `resolve_work` walks, so the failure lands in the first edition and
/// the second edition's member is the one that must never be asked
/// about.
fn two_edition_work(dir: &str) -> Work {
    use std::io::Write;

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
            "edition-a": { "label": "A", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 }
            ], "spreads": [] },
            "edition-b": { "label": "B", "members": [
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "edition-a",
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

/// The property Minor 3 of the review asked for pinned directly: a
/// failure in the first edition must abandon the *second* edition's
/// members too, not just the rest of its own edition. With a
/// single-edition fixture, a plain `break` and `break 'lookups` are
/// indistinguishable; this fixture has a second edition whose member
/// must never be looked up at all.
#[test]
fn a_resolution_failure_abandons_a_later_edition_too() {
    let work = two_edition_work("taiyaku-test-resolve-circuit-breaker-editions");
    let unreachable = FakeTransport::new();

    let resolved = resolve_work(&unreachable, &work, "http://nas:10801", &mut |_, _, _| {});

    let requests = unreachable.requests();
    crate::test_support::cleanup("taiyaku-test-resolve-circuit-breaker-editions");
    assert!(
        resolved.aborted_by_transport,
        "an unreachable instance must be reported as a transport-level abort"
    );
    assert!(
        resolved.resolutions.is_empty(),
        "nothing was resolved: {:?}",
        resolved.resolutions
    );
    assert_eq!(
        requests.len(),
        1,
        "the failure in edition-a must abandon edition-b too, not just the \
             rest of edition-a's own members: {requests:?}"
    );
    assert!(
        !requests.iter().any(|r| r.contains("c2")),
        "edition-b's member must never be looked up: {requests:?}"
    );
}

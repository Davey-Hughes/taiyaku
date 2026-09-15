use super::*;
use crate::stump::http::test_support::FakeTransport;

/// Real PNG magic followed by a distinguishing tail. Important 4 made
/// `CompositeSource` sniff a stump response before serving it, so a fake
/// payload has to sniff as an image now — this keeps a test able to tell
/// *which* source's bytes came back, the way a bare `b"from-stump"` did
/// before sniffing existed, while still being real image magic.
fn png_bytes(tail: &[u8]) -> Vec<u8> {
    [b"\x89PNG\r\n\x1a\n".as_slice(), tail].concat()
}

/// The property the whole milestone turns on: one work, one member from
/// Stump and one from disk, and nothing above this line can tell.
#[test]
fn each_member_is_served_by_whichever_source_has_it() {
    let tmp = crate::test_support::Fixture::new("composite");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let fake = FakeTransport::new().on_get(
        "/api/v2/media/book-1/page/1",
        "image/png",
        &png_bytes(b"from-stump"),
    );
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));
    resolutions.insert(edition.members[1].sha256.clone(), None);

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(
            Arc::new(fake),
            resolutions,
            Duration::from_secs(30),
        )),
    );

    let from_stump = composite.page(&root, &edition.members[0], 0).unwrap();
    let from_disk = composite.page(&root, &edition.members[1], 0).unwrap();

    assert_eq!(from_stump.bytes, png_bytes(b"from-stump"));
    assert_eq!(
        from_stump.content_type, "image/png",
        "the remote source's own sniffed type, not a guess from the entry name — and, \
             since Important 4, not the declared header either"
    );
    assert_eq!(
        from_disk.bytes, b"b1.jpg",
        "the unresolved member came off disk"
    );
}

/// O8: resolution now runs *behind* the bound listener, so a
/// `CompositeSource` exists and answers requests before anything has been
/// resolved at all. A member that will eventually come from Stump must be
/// served from **disk** during that window rather than waiting for it, and
/// must switch to Stump once the resolution lands.
///
/// This is the property that makes binding first safe, and it is the half
/// a test can hold. **What it does not claim is the ordering inside
/// `serve`** — that binds a socket, which nothing here drives; the
/// ordering there is a handful of lines and is visible by reading them.
#[test]
fn a_member_is_served_from_disk_until_its_resolution_lands() {
    let tmp = crate::test_support::Fixture::new("composite-late-resolution");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let fake = FakeTransport::new().on_get(
        "/api/v2/media/book-1/page/1",
        "image/png",
        &png_bytes(b"from-stump"),
    );
    // Constructed with nothing resolved, which is exactly the state
    // `serve` now binds the listener in.
    let side = StumpSide::new(Arc::new(fake), Resolutions::new(), Duration::from_secs(30));
    let composite = CompositeSource::new(LocalLibrary, Some(side.clone()));

    let before = composite.page(&root, &edition.members[0], 0).unwrap();

    let mut resolved = Resolutions::new();
    resolved.insert(edition.members[0].sha256.clone(), Some("book-1".into()));
    side.learn(resolved);

    let after = composite.page(&root, &edition.members[0], 0).unwrap();

    assert_eq!(
        before.bytes, b"a1.jpg",
        "disk must answer while resolution is still running"
    );
    assert_eq!(
        after.bytes,
        png_bytes(b"from-stump"),
        "and stump must answer once the resolution lands"
    );
}

/// With no Stump configured every member comes off disk, which is the
/// configuration `taiyaku serve` has had since M2 and must keep.
#[test]
fn with_no_stump_every_member_is_local() {
    let tmp = crate::test_support::Fixture::new("composite-local");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let composite = CompositeSource::new(LocalLibrary, None);
    let page = composite.page(&root, &edition.members[0], 0).unwrap();

    assert_eq!(page.bytes, b"a1.jpg");
}

/// Stump being unreachable must not lose a page that is also on disk.
/// The alternative is a reader that stops working when the NAS reboots,
/// for members it never needed the NAS to read.
#[test]
fn a_stump_failure_falls_back_to_disk_rather_than_failing() {
    let tmp = crate::test_support::Fixture::new("composite-fallback");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    // The fake has no reply registered, so every GET is an error.
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(
            Arc::new(FakeTransport::new()),
            resolutions,
            Duration::from_secs(30),
        )),
    );
    let page = composite.page(&root, &edition.members[0], 0).unwrap();

    assert_eq!(page.bytes, b"a1.jpg", "disk answers when stump cannot");
}

/// Important 1: without a cooldown, a member also on disk would stall on
/// the full transport timeout on *every* request, forever, once Stump
/// went down — the fallback would be unbounded rather than quick. A long
/// cooldown must suppress the second request's network attempt entirely.
#[test]
fn a_transport_failure_latches_the_cooldown_so_a_second_request_skips_the_network() {
    let tmp = crate::test_support::Fixture::new("composite-cooldown-latch");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    // No handler registered, so every GET is a transport-level error.
    let fake = Arc::new(FakeTransport::new());
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(
            fake.clone(),
            resolutions,
            Duration::from_secs(60),
        )),
    );

    let first = composite.page(&root, &edition.members[0], 0).unwrap();
    let second = composite.page(&root, &edition.members[0], 0).unwrap();

    let requests = fake.requests();
    assert_eq!(first.bytes, b"a1.jpg", "first request falls back to disk");
    assert_eq!(
        second.bytes, b"a1.jpg",
        "second request also served from disk"
    );
    assert_eq!(
        requests.len(),
        1,
        "the cooldown must suppress the second request's network attempt entirely: \
             {requests:?}"
    );
}

/// The cooldown is honoured, not a permanent latch: a zero-length one
/// expires immediately, so a second request retries the network rather
/// than being suppressed like the 60-second case above.
#[test]
fn a_zero_cooldown_expires_immediately_so_every_request_retries() {
    let tmp = crate::test_support::Fixture::new("composite-cooldown-zero");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let fake = Arc::new(FakeTransport::new());
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(fake.clone(), resolutions, Duration::ZERO)),
    );

    composite.page(&root, &edition.members[0], 0).unwrap();
    composite.page(&root, &edition.members[0], 0).unwrap();

    let requests = fake.requests();
    assert_eq!(
        requests.len(),
        2,
        "a zero cooldown must not suppress the second attempt: {requests:?}"
    );
}

/// Only a transport-level failure may latch. A 404 is Stump answering
/// "this book has no such page", not evidence the instance is down, and
/// must not suppress the next request — otherwise one stale resolution
/// would keep re-latching the whole side.
#[test]
fn a_404_does_not_latch_the_cooldown() {
    let tmp = crate::test_support::Fixture::new("composite-cooldown-404");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let fake = Arc::new(FakeTransport::new().on_get_status("/api/v2/media/book-1/page/1", 404));
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(
            fake.clone(),
            resolutions,
            Duration::from_secs(60),
        )),
    );

    composite.page(&root, &edition.members[0], 0).unwrap();
    composite.page(&root, &edition.members[0], 0).unwrap();

    let requests = fake.requests();
    assert_eq!(
        requests.len(),
        2,
        "a 404 must not latch the cooldown, so both requests reach the network: \
             {requests:?}"
    );
}

/// Important 4 (whole-branch review): the design doc records that a
/// nonexistent path on the real instance falls through to Stump's SPA
/// and returns HTML with a 200, under whatever `Content-Type` the SPA's
/// static file server happens to set. Trusting that header verbatim
/// would serve a reader the SPA's markup as a page. Sniffing the bytes —
/// the same rule `source.rs` already applies to local pages — must
/// refuse it and fall back to disk instead, exactly as a transport
/// failure already does.
#[test]
fn a_response_that_does_not_sniff_as_an_image_falls_back_to_disk_rather_than_being_served() {
    let tmp = crate::test_support::Fixture::new("composite-html-spa-fallback");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let fake = FakeTransport::new().on_get(
        "/api/v2/media/book-1/page/1",
        "image/jpeg",
        b"<!doctype html><html>this is the SPA, not a page</html>",
    );
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(
            Arc::new(fake),
            resolutions,
            Duration::from_secs(30),
        )),
    );

    let page = composite.page(&root, &edition.members[0], 0).unwrap();

    assert_eq!(
        page.bytes, b"a1.jpg",
        "the SPA's HTML must not be served; disk must answer instead"
    );
}

/// A response that fails to sniff as an image is Stump answering with
/// something unusable, not evidence the instance is unreachable — so, the
/// same rule a 404 already follows, it must not latch the cooldown
/// either.
#[test]
fn a_response_that_does_not_sniff_as_an_image_does_not_latch_the_cooldown() {
    let tmp = crate::test_support::Fixture::new("composite-html-spa-cooldown");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    let fake = Arc::new(FakeTransport::new().on_get(
        "/api/v2/media/book-1/page/1",
        "image/jpeg",
        b"<!doctype html><html>not a page</html>",
    ));
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(
            fake.clone(),
            resolutions,
            Duration::from_secs(60),
        )),
    );

    composite.page(&root, &edition.members[0], 0).unwrap();
    composite.page(&root, &edition.members[0], 0).unwrap();

    let requests = fake.requests();
    assert_eq!(
        requests.len(),
        2,
        "an unsniffable response must not latch the cooldown, so both requests reach the \
             network: {requests:?}"
    );
}

/// Recovery is visible in both directions: once a later request
/// succeeds, the latch clears and Stump is trusted again rather than
/// staying suppressed for a full cooldown it no longer needs.
#[test]
fn a_recovered_stump_clears_the_latch_and_is_served_from_again() {
    let tmp = crate::test_support::Fixture::new("composite-cooldown-recovery");
    let work = crate::test_support::work(&tmp);
    let root = work.path.parent().unwrap().to_path_buf();
    let sidecar = work.sidecar();
    let edition = sidecar.editions["raw"].clone();

    // `book-x` has no handler, so the first member's request fails.
    // `book-1` does, so the second member's request succeeds.
    let fake = Arc::new(FakeTransport::new().on_get(
        "/api/v2/media/book-1/page/1",
        "image/png",
        &png_bytes(b"from-stump"),
    ));
    let mut resolutions = Resolutions::new();
    resolutions.insert(edition.members[0].sha256.clone(), Some("book-x".into()));
    resolutions.insert(edition.members[1].sha256.clone(), Some("book-1".into()));

    let composite = CompositeSource::new(
        LocalLibrary,
        Some(StumpSide::new(fake.clone(), resolutions, Duration::ZERO)),
    );

    let first = composite.page(&root, &edition.members[0], 0).unwrap();
    assert_eq!(
        first.bytes, b"a1.jpg",
        "the failing member falls back to disk"
    );

    let second = composite.page(&root, &edition.members[1], 0).unwrap();

    let latch_clear = composite
        .stump
        .as_ref()
        .unwrap()
        .down_until
        .lock()
        .unwrap()
        .is_none();
    assert_eq!(
        second.bytes,
        png_bytes(b"from-stump"),
        "a later success must be served from stump, not disk"
    );
    assert!(latch_clear, "a successful request must clear the latch");
}

/// The cooldown must outlast the fetch timeout it guards, which is what
/// the comment on `DOWN_COOLDOWN` argues at length and nothing checked.
/// Every arithmetic mutation of `GET_TIMEOUT_SECONDS * 3` survived —
/// including `+`, which gives 23 s against a 20 s timeout and re-creates
/// exactly the "stalls dominate and the latch buys almost nothing" case
/// the constant was raised to fix.
#[test]
fn the_cooldown_outlasts_the_fetch_timeout_it_guards() {
    let timeout = std::time::Duration::from_secs(crate::stump::http::GET_TIMEOUT_SECONDS);
    assert!(
        DOWN_COOLDOWN > timeout,
        "cooldown {DOWN_COOLDOWN:?} must exceed the timeout {timeout:?}"
    );
    assert!(
        DOWN_COOLDOWN >= timeout * 2,
        "and by a margin, or a reader meets a full timeout almost continuously"
    );
    assert_eq!(DOWN_COOLDOWN, std::time::Duration::from_secs(60));
}

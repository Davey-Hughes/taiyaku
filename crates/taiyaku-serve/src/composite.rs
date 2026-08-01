//! One work, two sources, chosen per member.
//!
//! Measured on the development instance: of the three editions
//! `corpus/gotoubun-v01`'s sidecar records, Stump holds one. It indexes the
//! English releases; the raws and fan translations taiyaku aligns them
//! *against* are not in it, and a library of English releases is what a Stump
//! instance normally is.
//!
//! So a source that answered for a whole work would answer for no work here.
//! Routing per member is what makes taiyaku run beside Stump rather than
//! requiring it.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use taiyaku_core::Member;

use crate::stump::cache::Resolutions;
use crate::stump::{api, http::Transport};
use crate::{LocalLibrary, PageSource, ServedPage};

/// A configured Stump instance and what has been resolved against it.
///
/// `down_until` is the request path's circuit breaker, and it exists for the
/// same reason as the one in `resolve_work`. Falling back to disk is only
/// useful if the fallback is *quick*: with a resolution cache already on disk
/// and the instance unreachable, every page would otherwise pay the full GET
/// timeout before disk answered, on every request, forever — and a reader
/// waiting two minutes a page cannot tell that from broken.
///
/// **Only a transport-level failure latches.** Stump answering 404 for a page
/// a book does not have is an answer, not an outage, and must not demote the
/// whole instance.
#[derive(Debug, Clone)]
pub struct StumpSide {
    pub transport: Arc<dyn Transport>,
    pub resolutions: Resolutions,
    /// How long a transport failure suppresses further attempts.
    pub cooldown: Duration,
    /// When the suppression lifts, if it is in force.
    pub down_until: Arc<Mutex<Option<Instant>>>,
}

/// How long a transport failure suppresses further attempts.
///
/// **Longer than `stump::http::GET_TIMEOUT_SECONDS`, deliberately.** The latch
/// only suppresses once a failure has returned, so a reader meets one full
/// timeout per cooldown while Stump is down. A cooldown shorter than the
/// timeout — 30 s against 120 s, as this first shipped — means the stalls
/// dominate and the latch buys almost nothing.
pub const DOWN_COOLDOWN: Duration =
    Duration::from_secs(crate::stump::http::GET_TIMEOUT_SECONDS * 3);

impl StumpSide {
    /// A side with the cooldown latch clear, ready for its first request.
    pub fn new(
        transport: Arc<dyn Transport>,
        resolutions: Resolutions,
        cooldown: Duration,
    ) -> Self {
        Self {
            transport,
            resolutions,
            cooldown,
            down_until: Arc::new(Mutex::new(None)),
        }
    }

    /// Whether a prior transport failure is still suppressing attempts.
    fn is_down(&self) -> bool {
        matches!(*self.down_until.lock().unwrap(), Some(until) if Instant::now() < until)
    }

    /// A transport-level failure just happened against `id`: latch the
    /// cooldown so the next requests go straight to disk.
    ///
    /// This is only ever called after an attempt that was actually made, and
    /// an attempt is only made when `is_down` was false — so a *suppressed*
    /// request never logs, which is the noise this exists to avoid.
    ///
    /// It is not, however, one log line per outage. `is_down` and this call
    /// are a non-atomic read-modify-write, so every request already in flight
    /// when the first failure returns also attempts and also logs. Bounded by
    /// how many requests fit inside one timeout, self-healing, and not worth
    /// a lock held across the network to prevent — but the guarantee is
    /// "never on the suppressed path", not "exactly once".
    fn note_failure(&self, id: &str, why: &str) {
        eprintln!(
            "taiyaku: stump unreachable (book {id}: {why}) — suppressing further attempts for \
             {:?}",
            self.cooldown
        );
        *self.down_until.lock().unwrap() = Some(Instant::now() + self.cooldown);
    }

    /// A request just succeeded: clear the latch, and say so if it was set.
    /// A NAS reboot should be visible in both directions.
    fn note_success(&self) {
        if self.down_until.lock().unwrap().take().is_some() {
            eprintln!("taiyaku: stump reachable again");
        }
    }
}

/// Pages from Stump where it has them, from disk where it does not.
#[derive(Debug, Clone)]
pub struct CompositeSource {
    local: LocalLibrary,
    stump: Option<StumpSide>,
}

impl CompositeSource {
    pub fn new(local: LocalLibrary, stump: Option<StumpSide>) -> Self {
        Self { local, stump }
    }

    /// The Stump book this member is, if it is one.
    fn book_for(&self, member: &Member) -> Option<&str> {
        self.stump
            .as_ref()?
            .resolutions
            .get(&member.sha256)?
            .as_deref()
    }
}

impl PageSource for CompositeSource {
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String> {
        if let (Some(side), Some(id)) = (self.stump.as_ref(), self.book_for(member)) {
            if side.is_down() {
                // The latch is in force: go straight to disk without
                // touching the network. Not logging here is deliberate — one
                // log per suppressed request is exactly the noise the latch
                // exists to trade for a slow request instead.
                return self.local.page(root, member, within);
            }
            // `within` stays 0-based here. `page_bytes` owns the conversion to
            // Stump's 1-based numbering, so this call site cannot get it wrong.
            match api::page_bytes(side.transport.as_ref(), id, within) {
                // **The declared `Content-Type` is not trusted here.**
                // `source.rs` sniffs local page bytes because a declared
                // type is frequently wrong — measured, 114 of this corpus's
                // 8,637 page entries are PNGs named `.jpg` — and the design
                // doc records that a stale path on the real instance falls
                // through to Stump's SPA and returns HTML with a 200.
                // Believing the header there would serve a reader that HTML
                // as every page, and with a populated `taiyaku.stump.json`
                // the resolve-time page-hash confirmation never runs again
                // to catch it. Sniffing is applied only here, at the live
                // serving path — not inside `page_bytes` itself, which
                // `resolve_member` also calls for its own byte-for-byte
                // confirmation, a strictly stronger check that must not be
                // weakened into an unsniffable-content refusal.
                Ok(page) => match crate::source::sniffed_type(&page.bytes) {
                    Some(sniffed) => {
                        side.note_success();
                        return Ok(ServedPage {
                            content_type: sniffed.to_string(),
                            bytes: page.bytes,
                        });
                    }
                    None => {
                        eprintln!(
                            "taiyaku: stump book {id} page {within}: response declared \
                             `{}` but did not sniff as an image — falling back to disk",
                            page.content_type
                        );
                        // Stump answered, just not with a page — most likely
                        // its SPA's HTML after a moved API path. Not a
                        // transport failure, so this must not latch the
                        // cooldown; falling through to disk below.
                    }
                },
                // Disk is the fallback, not the failure. A member that is also
                // on disk must not become unreadable because the NAS rebooted,
                // and this library keeps most of its members there anyway.
                Err(err) => {
                    eprintln!(
                        "taiyaku: stump book {id} page {within}: {} — falling back to disk",
                        err.message
                    );
                    // A 404 is Stump answering "no", not Stump being down —
                    // latching on it would demote the whole instance for one
                    // member the library genuinely does not have there.
                    if err.transport {
                        side.note_failure(id, &err.message);
                    }
                }
            }
        }
        self.local.page(root, member, within)
    }
}

#[cfg(test)]
mod tests {
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
        let work = crate::test_support::work("taiyaku-test-composite");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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

        crate::test_support::cleanup("taiyaku-test-composite");
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

    /// With no Stump configured every member comes off disk, which is the
    /// configuration `taiyaku serve` has had since M2 and must keep.
    #[test]
    fn with_no_stump_every_member_is_local() {
        let work = crate::test_support::work("taiyaku-test-composite-local");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

        let composite = CompositeSource::new(LocalLibrary, None);
        let page = composite.page(&root, &edition.members[0], 0).unwrap();

        crate::test_support::cleanup("taiyaku-test-composite-local");
        assert_eq!(page.bytes, b"a1.jpg");
    }

    /// Stump being unreachable must not lose a page that is also on disk.
    /// The alternative is a reader that stops working when the NAS reboots,
    /// for members it never needed the NAS to read.
    #[test]
    fn a_stump_failure_falls_back_to_disk_rather_than_failing() {
        let work = crate::test_support::work("taiyaku-test-composite-fallback");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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

        crate::test_support::cleanup("taiyaku-test-composite-fallback");
        assert_eq!(page.bytes, b"a1.jpg", "disk answers when stump cannot");
    }

    /// Important 1: without a cooldown, a member also on disk would stall on
    /// the full transport timeout on *every* request, forever, once Stump
    /// went down — the fallback would be unbounded rather than quick. A long
    /// cooldown must suppress the second request's network attempt entirely.
    #[test]
    fn a_transport_failure_latches_the_cooldown_so_a_second_request_skips_the_network() {
        let work = crate::test_support::work("taiyaku-test-composite-cooldown-latch");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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
        crate::test_support::cleanup("taiyaku-test-composite-cooldown-latch");
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
        let work = crate::test_support::work("taiyaku-test-composite-cooldown-zero");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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
        crate::test_support::cleanup("taiyaku-test-composite-cooldown-zero");
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
        let work = crate::test_support::work("taiyaku-test-composite-cooldown-404");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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
        crate::test_support::cleanup("taiyaku-test-composite-cooldown-404");
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
        let work = crate::test_support::work("taiyaku-test-composite-html-spa-fallback");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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

        crate::test_support::cleanup("taiyaku-test-composite-html-spa-fallback");
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
        let work = crate::test_support::work("taiyaku-test-composite-html-spa-cooldown");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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
        crate::test_support::cleanup("taiyaku-test-composite-html-spa-cooldown");
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
        let work = crate::test_support::work("taiyaku-test-composite-cooldown-recovery");
        let root = work.path.parent().unwrap().to_path_buf();
        let edition = work.sidecar.editions["raw"].clone();

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
        crate::test_support::cleanup("taiyaku-test-composite-cooldown-recovery");
        assert_eq!(
            second.bytes,
            png_bytes(b"from-stump"),
            "a later success must be served from stump, not disk"
        );
        assert!(latch_clear, "a successful request must clear the latch");
    }
}

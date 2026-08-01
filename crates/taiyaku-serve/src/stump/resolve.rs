//! Deciding which Stump book a sidecar member is, and proving it.
//!
//! Matching on name and size alone would be wrong on exactly the material this
//! corpus was built to hold — deliberate duplicate sets, and at least two files
//! that are not what their names claim. So a match is a candidate, and the
//! candidate is confirmed by comparing the SHA-256 of one page fetched through
//! each source.
//!
//! One page rather than the whole file: about 1 MB against 335 MB for this
//! volume, on a corpus with 430 MB pages — and it confirms the property that
//! actually matters, which is that page N agrees, rather than that two files
//! resemble each other.

use sha2::{Digest, Sha256};

use crate::stump::cache::{self, Resolutions};
use crate::stump::{api, http::Transport};
use crate::works::Work;
use crate::{LocalLibrary, PageSource};

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    // Not `format!("{:x}", …)`: `digest` 0.11's `Output` no longer implements
    // `LowerHex`, matching the byte-by-byte formatting `taiyaku_read` already
    // uses for the same reason.
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What resolving one work against Stump produced.
///
/// Split from a bare `Resolutions` so the caller in `serve()` can tell "every
/// member got an answer, absence included" apart from "a transport failure
/// cut this run short" — which is exactly what it needs to decide whether
/// trying the *next* work is worth another 30-second timeout, or whether the
/// instance is simply unreachable and every remaining work should be skipped
/// without asking at all.
#[derive(Debug)]
pub struct Resolved {
    pub resolutions: Resolutions,
    /// Set when a transport-level failure stopped this run before every
    /// member was asked about.
    pub aborted_by_transport: bool,
}

/// A `resolve_member` failure, distinguishing a transport-level outage — the
/// only thing allowed to abandon the rest of a run — from Stump (or the
/// local filesystem) answering with a refusal that must simply leave this
/// one member unresolved, to be retried next start.
struct ResolveError {
    message: String,
    transport: bool,
}

impl From<api::StumpError> for ResolveError {
    fn from(e: api::StumpError) -> Self {
        Self {
            message: e.message,
            transport: e.transport,
        }
    }
}

/// Resolve every member of a work, using and updating the cache beside it.
///
/// Members already answered for — including those answered "not in Stump" —
/// cost nothing. Only genuinely new members reach the network.
///
/// A member that cannot be resolved is left unanswered rather than failing the
/// load, and is asked about again on the next start: the composite falls
/// through to local disk in the meantime, which is where most of this library
/// lives anyway. A failure here would refuse to serve a work that reads
/// perfectly without Stump.
///
/// Unanswered is deliberately not the same as absent. "Stump has not got this"
/// is an answer and is cached; a transport failure, a GraphQL error or an
/// ambiguous candidate is the question going unasked, and caching that would
/// let one start against an unreachable instance disable Stump for this work
/// permanently and silently.
///
/// **A circuit breaker bounds the retrying, and only a transport failure may
/// trip it.** The distinction is the same one `composite.rs`'s request-path
/// cooldown already draws: an ambiguous candidate, an unconfirmed single
/// match, a GraphQL coercion error (a member above ~2.1 GB, say), a locally
/// missing member file or a corrupt local archive are all Stump — or the
/// filesystem — *answering*, and must simply leave that one member
/// unresolved while the rest of the run continues. Only `Transport::post_json`
/// or `Transport::get_bytes` itself failing means the instance is
/// unreachable, and that is what abandons the rest of this run's unresolved
/// members — against an unreachable Stump that is one failed lookup at
/// startup instead of one per member, which on a large library is minutes of
/// `ureq` timeouts before the listener binds. Members never attempted stay
/// absent from the map exactly like a member that was attempted and refused,
/// so the next start retries all of them; nothing here changes what gets
/// retried, only how much of one run is spent finding out — and, before this
/// fix, a single semantic refusal anywhere in a work silently degraded that
/// entire work to local-only, forever, on every start.
pub fn resolve_work(transport: &dyn Transport, work: &Work, base: &str) -> Resolved {
    let mut resolutions = cache::load(work, base);
    let root = work
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut learned = false;
    let mut aborted_by_transport = false;

    'lookups: for edition in work.sidecar.editions.values() {
        for member in &edition.members {
            if resolutions.contains_key(&member.sha256) {
                continue;
            }
            match resolve_member(transport, root, member) {
                // An answer, cached: either Stump has this member or it
                // genuinely does not, and neither changes on the next run.
                Ok(answer) => {
                    resolutions.insert(member.sha256.clone(), answer);
                    learned = true;
                }
                // A transport-level failure: the instance itself could not be
                // reached, so trying every remaining member would not learn
                // anything new, only spend the same timeout again for each of
                // them. This is the only case that trips the breaker.
                Err(ResolveError {
                    message,
                    transport: true,
                }) => {
                    eprintln!(
                        "taiyaku: {} not resolved against stump: {message} — stump appears \
                         unreachable; abandoning the rest of this run's unresolved members; \
                         they will be retried next start",
                        member.path
                    );
                    aborted_by_transport = true;
                    break 'lookups;
                }
                // NOT an answer, and so NOT cached, but also NOT an outage:
                // Stump (or the local filesystem) answered — an ambiguous
                // candidate, an unconfirmed match, a GraphQL error, a missing
                // or corrupt local file — which must leave only *this*
                // member unresolved rather than abandoning every member
                // after it.
                Err(ResolveError {
                    message,
                    transport: false,
                }) => {
                    eprintln!(
                        "taiyaku: {} not resolved against stump: {message} — stump answered, \
                         which is not an outage; left unresolved and will be retried next start",
                        member.path
                    );
                }
            }
        }
    }

    if learned {
        // Not `?`. The cache is derived data — `load` already treats every
        // read failure as "resolve again" — so a full disk or a read-only
        // library directory must not refuse to serve a work that reads
        // perfectly from disk without Stump at all. This function's caller
        // runs before the listener binds, so propagating here would abort
        // startup for every work rather than slow one of them down.
        if let Err(why) = cache::store(work, base, &resolutions) {
            eprintln!(
                "taiyaku: could not remember stump resolutions for {}: {why}",
                work.path.display()
            );
        }
    }
    Resolved {
        resolutions,
        aborted_by_transport,
    }
}

/// One member: find a candidate, then make it prove itself.
fn resolve_member(
    transport: &dyn Transport,
    root: &std::path::Path,
    member: &taiyaku_core::Member,
) -> Result<Option<String>, ResolveError> {
    let path = root.join(&member.path);
    // A locally missing member, or one whose name has no stem, is Stump (or
    // rather, the filesystem) answering with a refusal — not evidence the
    // *instance* is unreachable — so this must not trip the circuit breaker.
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| ResolveError {
            message: format!("{}: no file name", path.display()),
            transport: false,
        })?;
    let size = std::fs::metadata(&path)
        .map_err(|e| ResolveError {
            message: format!("{}: {e}", path.display()),
            transport: false,
        })?
        .len();

    let Some(book) = api::find_book(transport, &stem, size, member.pages)? else {
        return Ok(None);
    };

    // The confirmation: the same page through each source. Both calls take
    // taiyaku's 0-based index; `page_bytes` speaks Stump's 1-based numbering
    // internally so no caller has to remember to.
    //
    // A corrupt local archive is a refusal too — the transport was never
    // touched — so it is wrapped the same way as the filesystem errors
    // above rather than being allowed to abandon the run.
    let ours = LocalLibrary
        .page(root, member, 0)
        .map_err(|e| ResolveError {
            message: e,
            transport: false,
        })?;
    let theirs = api::page_bytes(transport, &book.id, 0)?;
    if digest(&ours.bytes) != digest(&theirs.bytes) {
        eprintln!(
            "taiyaku: {} matches stump book {} by name and size, but their first pages differ — treating it as absent",
            member.path, book.path
        );
        return Ok(None);
    }
    Ok(Some(book.id))
}

#[cfg(test)]
mod tests {
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

        let resolved = resolve_work(&fake, &work, "http://nas:10801").resolutions;

        let first = work.sidecar.editions["raw"].members[0].sha256.clone();
        let second_sha = work.sidecar.editions["raw"].members[1].sha256.clone();
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

        let resolved = resolve_work(&fake, &work, "http://nas:10801").resolutions;

        let first = work.sidecar.editions["raw"].members[0].sha256.clone();
        let second_sha = work.sidecar.editions["raw"].members[1].sha256.clone();
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
        resolve_work(&fake, &work, "http://nas:10801");

        let second = FakeTransport::new();
        let resolved = resolve_work(&second, &work, "http://nas:10801").resolutions;

        let first = work.sidecar.editions["raw"].members[0].sha256.clone();
        let second_sha = work.sidecar.editions["raw"].members[1].sha256.clone();
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
        let first = work.sidecar.editions["raw"].members[0].sha256.clone();

        let unreachable = FakeTransport::new();
        let first_pass = resolve_work(&unreachable, &work, "http://nas:10801");
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
        let second_pass = resolve_work(&reachable, &work, "http://nas:10801");

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

        let resolved = resolve_work(&fake, &work, "http://nas:10801");

        let second_sha = work.sidecar.editions["raw"].members[1].sha256.clone();
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

        let resolved = resolve_work(&unreachable, &work, "http://nas:10801");

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

        Work {
            id: "t".into(),
            path,
            sidecar: taiyaku_core::Sidecar::from_json(json).unwrap(),
        }
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

        let resolved = resolve_work(&unreachable, &work, "http://nas:10801");

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
}

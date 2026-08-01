//! What taiyaku asks a Stump instance, and what it makes of the answers.

use serde::Deserialize;

use crate::ServedPage;
use crate::stump::http::Transport;

/// One book, as much of it as taiyaku needs.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Book {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub pages: u32,
}

#[derive(Debug, Deserialize)]
struct GraphQlReply<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct MediaData {
    media: MediaPage,
}

#[derive(Debug, Deserialize)]
struct MediaPage {
    nodes: Vec<Book>,
}

/// A Stump call failure, distinguishing a transport-level outage from Stump
/// itself answering.
///
/// Shared by `find_book` and `page_bytes`, which is what lets the
/// request-path cooldown latch (`StumpSide::down_until` in `composite.rs`)
/// and the startup resolver (`resolve_work` in `resolve.rs`) apply the same
/// rule: only `transport: true` — `Transport::post_json` or
/// `Transport::get_bytes` itself failing to reach the instance — may abandon
/// anything. An ambiguous candidate, an unconfirmed single match, a GraphQL
/// error body (a coercion failure on a member above ~2.1 GB, say) and a
/// non-200 page status are Stump *answering*, not evidence it is
/// unreachable, and treating them the same way would let one such answer
/// disable Stump permanently on the strength of a single start.
#[derive(Debug)]
pub struct StumpError {
    pub message: String,
    pub transport: bool,
}

/// The book matching this member, if Stump has exactly one.
///
/// All three predicates go in one query. `MediaFilterInput` exposes `name` and
/// `extension` as `FieldFilterString`, `size` as `NumericFilterI64` and
/// `pages` as `NumericFilterI32`, with `_and`, so there is no reason to filter
/// client-side.
///
/// `stem` is the file name **without its extension**, because that is what
/// `Media.name` holds.
pub fn find_book(
    transport: &dyn Transport,
    stem: &str,
    size: u64,
    pages: u32,
) -> Result<Option<Book>, StumpError> {
    let query = r#"query Find($stem: String!, $size: Int!, $pages: Int!) {
        media(
            filter: { _and: [
                { name:  { eq: $stem } },
                { size:  { eq: $size } },
                { pages: { eq: $pages } }
            ] },
            pagination: { offset: { page: 1, pageSize: 8 } }
        ) { nodes { id name path size pages } }
    }"#;
    let body = serde_json::json!({
        "query": query,
        "variables": { "stem": stem, "size": size, "pages": pages },
    })
    .to_string();

    // The only place in `find_book` a failure means the instance is
    // unreachable, rather than Stump having answered with something taiyaku
    // will not trust.
    let raw = transport
        .post_json("/api/graphql", &body)
        .map_err(|message| StumpError {
            message,
            transport: true,
        })?;
    let reply: GraphQlReply<MediaData> = serde_json::from_slice(&raw).map_err(|e| StumpError {
        message: format!("stump: unreadable GraphQL reply: {e}"),
        transport: false,
    })?;

    // GraphQL reports failure inside a 200 body. Treating that as "no match"
    // would silently drop a work's Stump half and look exactly like a file
    // Stump does not have. It is Stump answering, though, not an outage.
    if !reply.errors.is_empty() {
        let joined: Vec<&str> = reply.errors.iter().map(|e| e.message.as_str()).collect();
        return Err(StumpError {
            message: format!("stump: {}", joined.join("; ")),
            transport: false,
        });
    }

    let mut nodes = reply
        .data
        .ok_or_else(|| StumpError {
            message: "stump: GraphQL reply carried neither data nor errors".to_string(),
            transport: false,
        })?
        .media
        .nodes;

    match nodes.len() {
        0 => Ok(None),
        1 => {
            let found = nodes.remove(0);
            // Trusting Stump's filter alone is the same guess the ambiguity
            // arm below refuses to make with two candidates: if `name: {
            // eq: }` is case-insensitive or normalising, or `size` silently
            // no-ops, a single "match" can be the wrong book entirely. The
            // query already asked for `name size pages`, so confirming costs
            // nothing extra over the wire.
            //
            // `size` is compared via `i64::try_from` rather than `as`, so a
            // value that cannot fit is a mismatch rather than a wrapped
            // number that happens to look equal.
            let size_matches = i64::try_from(size).is_ok_and(|requested| requested == found.size);
            if found.name == stem && size_matches && found.pages == pages {
                Ok(Some(found))
            } else {
                Err(StumpError {
                    message: format!(
                        "stump: asked for `{stem}` at {size} bytes and {pages} pages, but the \
                         sole match was `{}` at {} bytes and {} pages ({}) — refusing to trust \
                         an unconfirmed filter",
                        found.name, found.size, found.pages, found.path
                    ),
                    transport: false,
                })
            }
        }
        // Guessing here is the wrong-page failure the page-hash confirmation
        // exists to prevent, and this corpus holds deliberate duplicate sets.
        _ => {
            let paths: Vec<&str> = nodes.iter().map(|b| b.path.as_str()).collect();
            Err(StumpError {
                message: format!(
                    "stump: {} books match `{stem}` at {size} bytes and {pages} pages: {}",
                    nodes.len(),
                    paths.join(", ")
                ),
                transport: false,
            })
        }
    }
}

/// One page of one book, addressed the way Stump addresses pages.
///
/// **Stump is 1-based and taiyaku is 0-based.** The caller passes taiyaku's
/// `zero_based` index; the `+ 1` conversion happens here, so this really is
/// the only place in `stump` that knows the number, rather than trusting
/// every caller to have already done it.
///
/// **The declared `Content-Type` is trusted here, deliberately.** This
/// function is the raw fetch primitive, called from two places that need
/// different guarantees: `resolve_member` uses it to fetch a page it is
/// about to compare byte-for-byte against the local copy, where a SHA-256
/// equality check is already a far stronger guard than sniffing could add;
/// `CompositeSource::page` uses it to serve a page to a client with no such
/// check running on every request, which is where a declared type matters
/// and where the sniffing actually lives — see `composite.rs`. Sniffing here
/// too would make an unsniffable confirmation fetch (a legitimate content
/// disagreement, which the digest check already turns into "not this book")
/// indistinguishable from a genuine outage-shaped failure, for no extra
/// safety on that path.
pub fn page_bytes(
    transport: &dyn Transport,
    id: &str,
    zero_based: u32,
) -> Result<ServedPage, StumpError> {
    let one_based = zero_based + 1;
    let path = format!("/api/v2/media/{id}/page/{one_based}");
    let (status, content_type, bytes) =
        transport.get_bytes(&path).map_err(|message| StumpError {
            message,
            transport: true,
        })?;
    if status != 200 {
        return Err(StumpError {
            message: format!(
                "stump: book {id} has no page {zero_based} (taiyaku); stump page {one_based} \
                 (HTTP {status})"
            ),
            transport: false,
        });
    }
    Ok(ServedPage {
        content_type,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stump::http::test_support::FakeTransport;

    /// The candidate query the design settled on: name, size and page count in
    /// one filter rather than three round trips or a client-side scan.
    ///
    /// `Media.name` is the file **stem** — measured on the live instance as
    /// `Go-Toubun no Hanayome - v01 (2018)`, with no `.cbz`. Sending the full
    /// basename matches nothing, every time, which is a failure that looks
    /// exactly like "Stump does not have this file".
    #[test]
    fn a_book_is_found_by_stem_size_and_page_count() {
        let reply = r#"{"data":{"media":{"nodes":[
            {"id":"a8ed51d8","name":"Go-Toubun no Hanayome - v01 (2018)",
             "path":"/data/Manga/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2018).cbz",
             "size":334752036,"pages":187}
        ]}}}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let found = find_book(
            &fake,
            "Go-Toubun no Hanayome - v01 (2018)",
            334_752_036,
            187,
        )
        .unwrap()
        .expect("the fixture holds one match");

        assert_eq!(found.id, "a8ed51d8");
        assert_eq!(found.pages, 187);
        assert_eq!(found.size, 334_752_036);

        // The teeth: `found.pages`/`found.size` above compare against values
        // this test itself wrote into the fixture, so that pair cannot fail.
        // This is what actually pins the query — deleting `variables` fails
        // here.
        let requests = fake.requests();
        assert_eq!(requests.len(), 1, "exactly one request: {requests:?}");
        assert!(
            requests[0].contains("Go-Toubun no Hanayome - v01 (2018)"),
            "body must carry the stem: {}",
            requests[0]
        );
        assert!(
            requests[0].contains("334752036"),
            "body must carry the size: {}",
            requests[0]
        );
        assert!(
            requests[0].contains("187"),
            "body must carry the page count: {}",
            requests[0]
        );
    }

    /// A member Stump does not have is not an error. It is the ordinary case
    /// for every raw scan in this library, and the composite falls through to
    /// local disk on it — so it must be `Ok(None)`, not `Err`.
    #[test]
    fn a_member_stump_does_not_have_is_absent_rather_than_an_error() {
        let fake =
            FakeTransport::new().on_post("/api/graphql", r#"{"data":{"media":{"nodes":[]}}}"#);

        assert_eq!(
            find_book(&fake, "Go-Toubun no Hanayome - v01", 34_527_438, 195).unwrap(),
            None
        );
    }

    /// Two books matching one member is ambiguous, and guessing which is the
    /// wrong-page failure this whole confirmation scheme exists to prevent.
    /// The corpus holds deliberate duplicate sets, so this is reachable.
    ///
    /// Important 2 (whole-branch review): Stump answering ambiguously is a
    /// refusal, not an outage — `resolve_work`'s circuit breaker may only
    /// abandon a run on `transport: true`, so this must be `false`.
    #[test]
    fn two_candidates_are_refused_rather_than_guessed_between() {
        let reply = r#"{"data":{"media":{"nodes":[
            {"id":"one","name":"v01","path":"/a/v01.cbz","size":10,"pages":2},
            {"id":"two","name":"v01","path":"/b/v01.cbz","size":10,"pages":2}
        ]}}}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let refused = find_book(&fake, "v01", 10, 2).expect_err("ambiguity must not be guessed at");
        assert!(
            !refused.transport,
            "an ambiguous match is Stump answering, not an outage: {refused:?}"
        );
        assert!(
            refused.message.contains("/a/v01.cbz"),
            "must name the candidates: {}",
            refused.message
        );
        assert!(
            refused.message.contains("/b/v01.cbz"),
            "must name the candidates: {}",
            refused.message
        );
    }

    /// A single node from Stump's own filter is not proof it's the right
    /// book. If `name: { eq: }` turns out to be case-insensitive or
    /// normalising, the filter can hand back a book with a different name
    /// than the one asked for — and trusting it shows a reader another
    /// volume's pages.
    #[test]
    fn a_lone_match_with_the_wrong_name_is_refused() {
        let reply = r#"{"data":{"media":{"nodes":[
            {"id":"wrong","name":"v02","path":"/a/v02.cbz","size":10,"pages":2}
        ]}}}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let refused =
            find_book(&fake, "v01", 10, 2).expect_err("an unconfirmed candidate must be refused");
        assert!(
            !refused.transport,
            "an unconfirmed candidate is Stump answering, not an outage: {refused:?}"
        );
        assert!(
            refused.message.contains("v01"),
            "must name what was requested: {}",
            refused.message
        );
        assert!(
            refused.message.contains("v02"),
            "must name what came back instead: {}",
            refused.message
        );
    }

    /// The same guard, tripped by `size` instead of `name` — the case the
    /// review called out specifically: `Book.size` is `i64` and the `size`
    /// parameter is `u64`, so the comparison has to be exact rather than a
    /// lossy cast that could make a mismatch look like a match.
    #[test]
    fn a_lone_match_with_the_wrong_size_is_refused() {
        let reply = r#"{"data":{"media":{"nodes":[
            {"id":"wrong","name":"v01","path":"/a/v01.cbz","size":999,"pages":2}
        ]}}}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let refused =
            find_book(&fake, "v01", 10, 2).expect_err("an unconfirmed candidate must be refused");
        assert!(
            refused.message.contains("10 bytes"),
            "must name the size that was requested: {}",
            refused.message
        );
        assert!(
            refused.message.contains("999"),
            "must name the size that came back instead: {}",
            refused.message
        );
    }

    /// GraphQL reports failure in a 200 body, so a transport that only checks
    /// the status reads an error document as an empty result — and a work
    /// silently loses its Stump half.
    ///
    /// Important 2: a GraphQL error body is Stump answering — the request
    /// reached the instance and it replied — so `resolve_work`'s circuit
    /// breaker must not treat it as an outage. A member above ~2.1 GB
    /// hitting `$size: Int!`'s 32-bit ceiling arrives exactly this way.
    #[test]
    fn a_graphql_error_is_a_failure_even_though_the_status_was_200() {
        let reply = r#"{"data":null,"errors":[{"message":"Unknown field \"pages\""}]}"#;
        let fake = FakeTransport::new().on_post("/api/graphql", reply);

        let failed = find_book(&fake, "x", 1, 1).expect_err("errors must not read as no match");
        assert!(
            !failed.transport,
            "a GraphQL error body is Stump answering, not an outage: {failed:?}"
        );
        assert!(
            failed.message.contains("Unknown field"),
            "must carry the message: {}",
            failed.message
        );
    }

    /// The one case that must be marked `transport: true`: `post_json`
    /// itself failing, the way it does against an unreachable instance.
    /// Everything else Stump can say — even an outright refusal — is Stump
    /// talking, and only this case may trip `resolve_work`'s circuit
    /// breaker or `StumpSide`'s cooldown latch.
    #[test]
    fn a_transport_level_failure_is_marked_as_one() {
        // No handler registered for this path, so `FakeTransport::post_json`
        // returns its own `Err` — standing in for the network itself
        // failing, the same convention every other test in this crate uses.
        let fake = FakeTransport::new();

        let failed = find_book(&fake, "x", 1, 1).expect_err("an unreachable fake must error");
        assert!(
            failed.transport,
            "post_json itself failing must be marked transport: true: {failed:?}"
        );
    }

    /// taiyaku is 0-based and Stump is 1-based. This is one of exactly two
    /// places in the codebase where that conversion happens, and getting it
    /// wrong shows a reader the wrong page while every hash and count in the
    /// sidecar still matches.
    ///
    /// Two calls, not one: a test that only ever passed `0` and checked for
    /// `page/1` would pass identically whether `page_bytes` added one,
    /// subtracted one, or ignored the argument outright. Passing `2` and
    /// requiring `page/3` catches an off-by-one in either direction.
    #[test]
    fn page_zero_based_is_converted_to_stump_one_based() {
        let fake = FakeTransport::new()
            .on_get("/api/v2/media/abc/page/1", "image/jpeg", b"first")
            .on_get("/api/v2/media/abc/page/3", "image/jpeg", b"third");

        let first = page_bytes(&fake, "abc", 0).unwrap();
        let third = page_bytes(&fake, "abc", 2).unwrap();

        assert_eq!(first.bytes, b"first");
        assert_eq!(third.bytes, b"third");
        assert_eq!(
            fake.requests(),
            vec![
                "GET /api/v2/media/abc/page/1".to_string(),
                "GET /api/v2/media/abc/page/3".to_string(),
            ]
        );
    }

    /// Stump answers 404 past the end. That has to become a message naming
    /// the book and page rather than an empty 200, which a reader renders as
    /// a blank page and says nothing about. Named in both taiyaku's and
    /// Stump's terms, so the failure is diagnosable from either side.
    #[test]
    fn a_page_past_the_end_names_what_was_missing() {
        let fake = FakeTransport::new().on_get_status("/api/v2/media/abc/page/999", 404);

        let missing = page_bytes(&fake, "abc", 998).expect_err("404 must not be served as bytes");
        assert!(
            !missing.transport,
            "a 404 is an answer, not a transport failure: {missing:?}"
        );
        assert!(
            missing.message.contains("abc"),
            "must name the book: {}",
            missing.message
        );
        assert!(
            missing.message.contains("998"),
            "must name taiyaku's page number: {}",
            missing.message
        );
        assert!(
            missing.message.contains("999"),
            "must name Stump's page number: {}",
            missing.message
        );
    }
}

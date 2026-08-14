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
    let query = r"query Find($stem: String!, $size: Int!, $pages: Int!) {
        media(
            filter: { _and: [
                { name:  { eq: $stem } },
                { size:  { eq: $size } },
                { pages: { eq: $pages } }
            ] },
            pagination: { offset: { page: 1, pageSize: 8 } }
        ) { nodes { id name path size pages } }
    }";
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
mod tests;

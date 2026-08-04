//! OPDS 1.2 with the Page Streaming Extension.
//!
//! Stump already serves OPDS-PSE, so this feed is not here to let a reader
//! fetch pages — it is here for the one thing neither Stump nor Komga can
//! express: **several editions grouped under one work**. Measured on the
//! development instance, `Go-Toubun no Hanayome` and its Full Color Edition
//! are two unrelated series of 14 books each; with the raws and the fan
//! chapters one work would occupy four, with nothing relating them.
//!
//! XML is written by hand rather than through a crate. The element set is
//! small and fixed, and the only hard part is escaping — which has its own
//! test, because this corpus holds titles with `&` and `♡` in them.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::ServeState;

const ATOM: &str = "application/atom+xml;profile=opds-catalog";
const PSE_NS: &str = "http://vaemendis.net/opds-pse/ns";
const SIDECAR_REL: &str = "https://taiyaku.dev/rel/sidecar";

/// RFC 4287 requires `atom:author` on a feed unless every entry carries its
/// own, and none of taiyaku's do — so it goes on the feed instead, the same
/// as `catalog`'s already does.
const AUTHOR: &str = "<author><name>taiyaku</name></author>";

/// The five characters XML cannot carry literally.
///
/// Everything else passes through unchanged: the document is UTF-8 and this
/// library's titles are full of characters that are perfectly legal and would
/// be mangled by a stricter hand.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// Percent-encode `text` for use inside a URL path segment.
///
/// A different layer from `escape`: `escape` protects XML markup and belongs
/// on *displayed* text; this protects a URL path segment and belongs on
/// text that becomes part of an `href`. An edition key can be both — shown
/// in an `<id>` and addressed in a `pse:stream` link — and each use needs
/// its own layer; using this one for the display text would show a reader
/// `raw%20color` instead of `raw color`.
///
/// Everything in the RFC 3986 "unreserved" set (`A-Za-z0-9-._~`) passes
/// through unchanged; everything else becomes `%XX`, uppercase hex, encoded
/// byte-by-byte over the UTF-8 representation so a non-ASCII key encodes the
/// same way a browser's `encodeURIComponent` would. Written by hand rather
/// than added as a dependency, matching this module's existing choice for
/// `escape` and the crate's short dependency list.
pub fn percent_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    out
}

fn feed(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, format!("{ATOM};kind=navigation"))],
        body,
    )
        .into_response()
}

fn missing(message: String) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

async fn catalog() -> Response {
    feed(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>taiyaku:catalog</id>
  <title>taiyaku</title>
  <updated>1970-01-01T00:00:00Z</updated>
  <author><name>taiyaku</name></author>
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
  <entry>
    <id>taiyaku:works</id>
    <title>Works</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">Every work this server holds, with its editions</content>
    <link rel="subsection" type="{ATOM};kind=navigation" href="/opds/v1.2/works"/>
  </entry>
</feed>
"#
    ))
}

async fn works(State(state): State<Arc<ServeState>>) -> Response {
    let mut entries = String::new();
    for work in &state.works {
        let title = escape(&work.sidecar.work.title);
        let id = escape(&work.id);
        // `work.id` is slugified by construction (see `work_id`) and needs
        // no encoding, but encoding it anyway costs nothing and does not
        // depend on a guarantee made in another module.
        let href_id = escape(&percent_encode(&work.id));
        let count = work.sidecar.editions.len();
        entries.push_str(&format!(
            r#"  <entry>
    <id>taiyaku:work:{id}</id>
    <title>{title}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{count} editions</content>
    <link rel="subsection" type="{ATOM};kind=acquisition" href="/opds/v1.2/works/{href_id}"/>
  </entry>
"#
        ));
    }
    feed(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>taiyaku:works</id>
  <title>Works</title>
  <updated>1970-01-01T00:00:00Z</updated>
  {AUTHOR}
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/works"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
{entries}</feed>
"#
    ))
}

/// One entry per edition of a work, each carrying the links OPDS 1.2
/// requires.
///
/// **The acquisition link is a deliberate deviation, decided by the owner.**
/// OPDS 1.2 requires at least one `rel="http://opds-spec.org/acquisition"`
/// link on every entry in an acquisition feed, or a conformant client may
/// render the entry as unopenable — but taiyaku does not serve a whole
/// publication a client could download; it streams pages, and PSE
/// (`rel="http://vaemendis.net/opds-pse/stream"`, below) is the real
/// acquisition mechanism a reader uses. The acquisition link here points at
/// the edition's first page — a real, fetchable image, not a dead link — so
/// a conformant client sees a satisfiable entry rather than an empty one.
async fn work_feed(State(state): State<Arc<ServeState>>, AxPath(id): AxPath<String>) -> Response {
    let work = match state.work_or_missing(&id) {
        Ok(work) => work,
        Err(message) => return missing(message),
    };
    let work_id = escape(&work.id);
    // See the note in `works`: `work.id` is already URL-safe by
    // construction, but every other `href` in this feed needs encoding, so
    // encoding this one too keeps the rule uniform rather than one route
    // relying on a guarantee made in another module.
    let work_href = escape(&percent_encode(&work.id));
    let title = escape(&work.sidecar.work.title);

    let mut entries = String::new();
    for (key, edition) in &work.sidecar.editions {
        // Two layers, two variables: `key_x` is XML-escaped display text —
        // the atom `<id>` a reader never sees rendered as a link — and
        // `key_href` is percent-encoded for the `href`s below. An edition
        // key is not sanitised on the way in (a sidecar this server did not
        // write may hold one `align` would now refuse), so both have to
        // handle whatever a key actually contains, including a space or a
        // `/`.
        let key_x = escape(key);
        let key_href = escape(&percent_encode(key));
        let label = escape(&edition.label);
        // The flat page count across every member, which is exactly what PSE
        // streams. An edition of seven chapter files is one publication of 235
        // pages here, which only works because pages are addressed by flat
        // index rather than by member and entry.
        let pages = edition.total_pages();
        entries.push_str(&format!(
            r#"  <entry>
    <id>taiyaku:edition:{work_id}:{key_x}</id>
    <title>{title} — {label}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{pages} pages</content>
    <link rel="http://opds-spec.org/image/thumbnail" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/cover"/>
    <link rel="http://opds-spec.org/acquisition" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/1"/>
    <link rel="http://vaemendis.net/opds-pse/stream" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/{{pageNumber}}"
          pse:count="{pages}"/>
  </entry>
"#
        ));
    }

    (
        [(header::CONTENT_TYPE, format!("{ATOM};kind=acquisition"))],
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:pse="{PSE_NS}">
  <id>taiyaku:work:{work_id}</id>
  <title>{title}</title>
  <updated>1970-01-01T00:00:00Z</updated>
  {AUTHOR}
  <link rel="self"  type="{ATOM};kind=acquisition" href="/opds/v1.2/works/{work_href}"/>
  <link rel="start" type="{ATOM};kind=navigation"  href="/opds/v1.2/catalog"/>
  <link rel="{SIDECAR_REL}" type="application/json" href="/api/works/{work_href}"/>
{entries}</feed>
"#
        ),
    )
        .into_response()
}

async fn stream_page(
    State(state): State<Arc<ServeState>>,
    // `String` rather than `u32` — see `routes::page_index`. A malformed
    // number on the PSE stream is refused in this server's own words, and in
    // the same words the `/api/…` route uses, rather than by axum's `Path`
    // rejection.
    AxPath((id, edition, number)): AxPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let number = match crate::routes::page_index(&number, &edition) {
        Ok(number) => number,
        Err(refusal) => return refusal.into_response(),
    };
    // **PSE counts from 0**, so this is taiyaku's own index unchanged. There
    // is no conversion here and there must not be one.
    //
    // This shipped as 1-based and was wrong. Measured against Panels on
    // 2026-07-31: it opens a book by requesting `pages/0`, retried that up to
    // nine times per edition against the 404 this used to return, showed an
    // error screen, and only advanced once tapped — at which point every page
    // it displayed was the one before the page it believed it was on, and the
    // true first page could not be reached at all. Twenty of the session's
    // thirty-nine requests were that 404.
    //
    // Stump settles the convention rather than the spec being read twice: its
    // REST API is 1-based, and its OPDS-PSE template is
    // `…/pages/{pageNumber}?zero_based=true` — it converts *for* PSE. So a
    // server whose PSE stream is 1-based disagrees with both the client and
    // the other implementation of the same feed.
    let index = number;

    // O1: the same validator the `/api/…` page route sends, checked the same
    // way and for the same reason — Panels reaches pages through *this*
    // route, not the `/api/…` one, and O5 already recorded once what fixing
    // only the other route is worth. `crate::routes::locate_member` reads
    // nothing but the in-memory sidecar, so a match is answered before the
    // decompression or Stump round trip below ever runs.
    let found = match state.work_or_missing(&id) {
        Ok(found) => found,
        Err(e) => return missing(e),
    };
    let (sha256, offset) = match crate::routes::locate_member(found, &edition, index) {
        Ok(pair) => pair,
        Err(e) => return missing(e),
    };
    let tag = crate::routes::page_etag(sha256, offset);
    if crate::routes::if_none_match_hits(&headers, &tag) {
        return crate::routes::not_modified(&tag);
    }

    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.work_or_missing(&id)?;
        state.page(found, &edition, index)
    })
    .await;

    match fetched {
        Ok(Ok(page)) => (
            [
                (header::CONTENT_TYPE, page.content_type),
                (header::ETAG, tag),
            ],
            page.bytes,
        )
            .into_response(),
        Ok(Err(e)) => missing(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("page read panicked: {e}"),
        )
            .into_response(),
    }
}

async fn cover(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition)): AxPath<(String, String)>,
) -> Response {
    let fetched = tokio::task::spawn_blocking(move || {
        let work = state.work_or_missing(&id)?;
        let found = work
            .sidecar
            .editions
            .get(&edition)
            .ok_or_else(|| format!("no edition `{edition}` in work `{id}`"))?;
        // Minor 5 (whole-branch review): keyed on whichever member
        // `Edition::locate(0)` actually resolves page 0 to, not
        // `members.first()` unconditionally. `locate` steps over a member
        // recorded with zero pages, so a first member with none would make
        // `members.first()` name a different member than the one
        // `state.page(work, &edition, 0)` below actually reads bytes from —
        // caching the generated cover under the wrong member's hash.
        let (position, _within) = found
            .locate(0)
            .ok_or_else(|| format!("`{edition}` has no pages"))?;
        let cover_member = &found.members[position];
        crate::cover::cached_or_generated(&state.cache_dir, &cover_member.sha256, || {
            state.page(work, &edition, 0).map(|p| p.bytes)
        })
    })
    .await;

    match fetched {
        Ok(Ok(bytes)) => ([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response(),
        Ok(Err(e)) => missing(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cover generation panicked: {e}"),
        )
            .into_response(),
    }
}

/// Every OPDS route, ready to be merged into the main router.
pub fn routes() -> Router<Arc<ServeState>> {
    Router::new()
        .route("/opds/v1.2/catalog", get(catalog))
        .route("/opds/v1.2/works", get(works))
        .route("/opds/v1.2/works/{work}", get(work_feed))
        .route(
            "/opds/v1.2/works/{work}/{edition}/pages/{number}",
            get(stream_page),
        )
        .route("/opds/v1.2/works/{work}/{edition}/cover", get(cover))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn body_of(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// The characters this corpus's edition keys can actually contain once
    /// hand-typed corrections are allowed: already-safe text passes through,
    /// a space and `/` — the two the review called out by name — encode, and
    /// hex digits are uppercase per RFC 3986.
    #[test]
    fn percent_encoding_covers_the_characters_a_key_can_contain() {
        assert_eq!(
            percent_encode("raw-mono"),
            "raw-mono",
            "already url-safe text is untouched"
        );
        assert_eq!(percent_encode("raw color"), "raw%20color");
        assert_eq!(percent_encode("a/b"), "a%2Fb");
        assert_eq!(percent_encode("a#b?c"), "a%23b%3Fc");
        // The rest of the unreserved set, which must survive untouched — an
        // encoder that escapes these is not wrong so much as noisy, but it
        // makes every ordinary key unreadable in a feed.
        assert_eq!(percent_encode("~-._"), "~-._");
    }

    /// The case an ASCII-only test cannot see, and the one this corpus makes
    /// real: it holds a series called `Bisque Doll de Chu ♡`, so an edition
    /// key with a multi-byte character is not hypothetical.
    ///
    /// RFC 3986 percent-encoding is defined over **bytes**. An encoder written
    /// over `char`s emits one `%XX` per character, which for `♡` would be a
    /// single escape for a three-byte sequence — a URL no client can decode,
    /// and one that every ASCII test in this module would still pass. This is
    /// what stops someone rewriting the loop over `.chars()` later.
    #[test]
    fn a_multi_byte_character_is_encoded_as_its_bytes() {
        // U+2661 is E2 99 A1 in UTF-8: three escapes, not one.
        assert_eq!(percent_encode("♡"), "%E2%99%A1");
        assert_eq!(
            percent_encode("Bisque Doll de Chu ♡"),
            "Bisque%20Doll%20de%20Chu%20%E2%99%A1"
        );
        // Every byte of a multi-byte sequence is above 0x7F, so none of them
        // can be mistaken for an unreserved ASCII character.
        assert!(
            percent_encode("日本語")
                .bytes()
                .all(|b| b == b'%' || b.is_ascii_hexdigit()),
            "a wholly non-ASCII key must encode to nothing but escapes"
        );
    }

    /// A title is not XML. This corpus holds `Bisque Doll de Chu ♡` and plenty
    /// of `&` — and an unescaped `&` does not produce a slightly wrong feed,
    /// it produces one no client can parse at all.
    #[test]
    fn markup_in_a_title_is_escaped_rather_than_emitted() {
        assert_eq!(escape("Tom & Jerry"), "Tom &amp; Jerry");
        assert_eq!(escape("<script>"), "&lt;script&gt;");
        assert_eq!(escape(r#"a "b" 'c'"#), "a &quot;b&quot; &apos;c&apos;");
        // Not markup, and not something to mangle.
        assert_eq!(escape("Bisque Doll de Chu ♡"), "Bisque Doll de Chu ♡");
    }

    /// The root has to be reachable and say what it is, or no client gets
    /// past the first request.
    #[tokio::test]
    async fn the_catalog_is_an_atom_navigation_feed() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-root"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/catalog")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-root");
        assert!(content_type.starts_with(ATOM), "{content_type}");
        assert!(body.starts_with("<?xml"), "{body}");
        assert!(
            body.contains("/opds/v1.2/works"),
            "must link onward: {body}"
        );
    }

    /// The whole justification for this feed: one entry per edition, under one
    /// work, each streamable, with the sidecar linked so a client that
    /// understands taiyaku can resolve between them.
    #[tokio::test]
    async fn a_work_lists_its_editions_with_page_counts_and_a_sidecar_link() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-work"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works/t")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-work");
        assert!(
            body.contains(PSE_NS),
            "the PSE namespace must be declared: {body}"
        );
        assert!(
            body.contains(r#"pse:count="4""#),
            "the edition's flat page count, across both members: {body}"
        );
        assert!(
            body.contains("/opds/v1.2/works/t/raw/pages/{pageNumber}"),
            "the stream template: {body}"
        );
        assert!(body.contains(SIDECAR_REL), "the sidecar link: {body}");
        assert!(
            body.contains("/api/works/t"),
            "pointing at the sidecar: {body}"
        );
    }

    /// **PSE counts from 0, and so does taiyaku, so nothing is converted.**
    ///
    /// This shipped converting, and the conversion was the bug. Panels opens a
    /// book at `pages/0`, so a server that answered 404 there showed an error
    /// screen; every page it did serve was then the one before the page the
    /// client believed it was on, and the first page could not be reached.
    /// Every assertion below fails against that version, which is the point of
    /// keeping all four: the boundary at each end is where an off-by-one hides.
    ///
    /// The fixture is four pages whose bytes are their own entry names, so a
    /// shifted answer is not merely a wrong status but visibly the wrong page.
    #[tokio::test]
    async fn pse_page_numbers_are_taiyaku_indices_unchanged() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-page"));
        let app = routes().with_state(state);

        let page = async |n: u32| {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/opds/v1.2/works/t/raw/pages/{n}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            (status, body_of(response).await)
        };

        // Page 0 exists and is the first page. Under the old conversion this
        // was a 404, which is exactly what Panels hit on opening a book.
        let (zero_status, zero) = page(0).await;
        // A middle page, to catch a shift that only the ends would otherwise
        // reveal.
        let (_, two) = page(2).await;
        // The last page is `pse:count - 1`, not `pse:count`. The old code
        // served this index for `pages/4` and nothing for `pages/3`'s true
        // content.
        let (_, last) = page(3).await;
        // One past the end. `pse:count` is 4, so 4 is out of range — under the
        // old conversion this was the last page and answered 200.
        let (past_end, _) = page(4).await;

        crate::test_support::cleanup("taiyaku-test-opds-page");
        assert_eq!(zero_status, StatusCode::OK, "pse page 0 must exist");
        assert_eq!(zero, "a1.jpg", "pse page 0 is taiyaku index 0");
        assert_eq!(two, "b1.jpg", "pse page 2 is taiyaku index 2");
        assert_eq!(last, "b2.jpg", "the last page is pse:count - 1");
        assert_eq!(
            past_end,
            StatusCode::NOT_FOUND,
            "pse:count is 4, so page 4 is one past the end"
        );
    }

    /// O5, on the other route addressed by page number. The `/api/…` route
    /// and this one are two spellings of one operation, so they must refuse a
    /// malformed index identically — that is the whole reason `page_index` is
    /// shared rather than written twice, and a shared helper nothing pins from
    /// both ends drifts the moment one caller is edited.
    ///
    /// Panels reaches pages through *this* route, not the `/api/…` one, so if
    /// only one of the two were fixed it would be the wrong one.
    #[tokio::test]
    async fn a_pse_page_number_that_is_not_a_number_is_refused_the_same_way() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-page-nan"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-page-nan");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("abc"), "must quote what was sent: {body}");
        assert!(
            body.contains("raw"),
            "must name the edition it was sent to: {body}"
        );
        assert!(
            !body.contains("u32"),
            "must not be phrased in terms of a Rust type: {body}"
        );
    }

    /// O1, on the other route addressed by page number — the one Panels
    /// actually reaches pages through. `routes.rs`'s tests pin the mechanism
    /// on the `/api/…` route; this one exists so that fixing only that
    /// route — the mistake O5 already recorded once, on this same pair of
    /// routes — cannot happen again silently here.
    #[tokio::test]
    async fn the_pse_stream_also_carries_an_etag_and_answers_a_matching_request_304() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-page-etag"));
        let app = routes().with_state(state);

        let first = app
            .clone()
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let tag = first.headers()[header::ETAG].clone();

        let second = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/0")
                    .header(header::IF_NONE_MATCH, tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = second.status();
        let bytes = axum::body::to_bytes(second.into_body(), 1 << 16)
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-opds-page-etag");
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert!(
            bytes.is_empty(),
            "a 304 must carry no body on this route either"
        );
    }

    /// A feed for a work this server does not hold says so, rather than
    /// returning an empty feed a client renders as an empty library.
    #[tokio::test]
    async fn an_unknown_work_is_refused_rather_than_served_empty() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-missing"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-missing");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("nope"), "must name the work: {body}");
    }

    /// Minor 2: `/opds/v1.2/works` was exercised only by hand before this
    /// test existed, so a regression in its escaping or content type would
    /// not be caught by `cargo test`.
    #[tokio::test]
    async fn the_works_feed_lists_every_work_with_its_edition_count() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-works-list"));
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-works-list");
        assert!(content_type.starts_with(ATOM), "{content_type}");
        assert!(body.starts_with("<?xml"), "{body}");
        assert!(
            body.contains("<content type=\"text\">1 editions</content>"),
            "must report the fixture work's edition count: {body}"
        );
        assert!(
            body.contains(r#"href="/opds/v1.2/works/t"#),
            "must link onward to the work: {body}"
        );
    }

    /// Minor 3: `ServeState::page` names what was missing for an unknown
    /// edition, but nothing exercised that message *through* the OPDS stream
    /// route before this test.
    #[tokio::test]
    async fn streaming_an_unknown_edition_names_it() {
        let state = Arc::new(crate::test_support::state(
            "taiyaku-test-opds-unknown-edition",
        ));
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/nope/pages/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-unknown-edition");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("nope"), "must name the edition: {body}");
    }

    /// The other message `ServeState::page` produces, also unexercised
    /// through this route before this test: a page past the edition's end
    /// names the edition and how many pages it actually has.
    #[tokio::test]
    async fn streaming_a_page_past_the_end_names_the_edition_and_its_length() {
        let state = Arc::new(crate::test_support::state(
            "taiyaku-test-opds-page-past-end",
        ));
        let app = routes().with_state(state);

        // The fixture edition has 4 pages; PSE page 5 is index 4, one past it.
        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/pages/5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = body_of(response).await;

        crate::test_support::cleanup("taiyaku-test-opds-page-past-end");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains("raw"), "must name the edition: {body}");
        assert!(body.contains('4'), "must say how many pages it has: {body}");
    }

    /// `stream_page` has three router-level tests for what an unknown work
    /// or edition returns; `cover` — the only other route that resolves a
    /// work and then an edition — had none. A prior attempt at this test was
    /// written, run, watched pass, and then deleted before committing to
    /// keep the test count at a predicted number; that was a mistake
    /// regardless of the count, since a route with no test protecting it can
    /// regress silently. This one stays.
    #[tokio::test]
    async fn a_cover_for_an_unknown_work_or_edition_names_what_was_missing() {
        let state = Arc::new(crate::test_support::state(
            "taiyaku-test-opds-cover-missing",
        ));
        let app = routes().with_state(state);

        let no_work = app
            .clone()
            .oneshot(
                Request::get("/opds/v1.2/works/nope/raw/cover")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let no_work_status = no_work.status();
        let no_work_body = body_of(no_work).await;

        let no_edition = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/nope/cover")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let no_edition_status = no_edition.status();
        let no_edition_body = body_of(no_edition).await;

        crate::test_support::cleanup("taiyaku-test-opds-cover-missing");
        assert_eq!(no_work_status, StatusCode::NOT_FOUND);
        assert!(
            no_work_body.contains("nope"),
            "must name the work: {no_work_body}"
        );
        assert_eq!(no_edition_status, StatusCode::NOT_FOUND);
        assert!(
            no_edition_body.contains("nope"),
            "must name the edition: {no_edition_body}"
        );
    }

    /// Important 1b: an edition key that is not URL-safe still has to reach
    /// the OPDS feed, because the format is meant to be written by other
    /// tools — `taiyaku` cannot assume every sidecar it serves was written
    /// by its own now-validating `align`. Percent-encoding the key in every
    /// `href` is what keeps the feed usable; XML-escaping alone (the
    /// pre-fix behaviour) leaves a literal space or `/` in the URL.
    ///
    /// Proven through the router rather than by inspecting a string this
    /// crate built: the `href` is pulled out of the feed this handler
    /// actually returned, then requested for real. Axum decodes a path
    /// parameter before an extractor sees it, so a correctly encoded segment
    /// must route back to the very same edition and its real page bytes —
    /// which is the property that matters, not merely that some string
    /// somewhere contains `%20`.
    #[tokio::test]
    async fn a_percent_encoded_edition_key_in_the_feed_routes_back_to_the_same_edition() {
        let state = Arc::new(ServeState {
            works: vec![crate::test_support::work_with_edition_key(
                "taiyaku-test-opds-percent",
                "raw color",
            )],
            source: Arc::new(crate::LocalLibrary),
            cache_dir: std::env::temp_dir()
                .join("taiyaku-test-opds-percent")
                .join("covers"),
            log: None,
        });
        let app = routes().with_state(state);

        let feed = app
            .clone()
            .oneshot(
                Request::get("/opds/v1.2/works/t")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let feed_body = body_of(feed).await;

        // Pulled out of the feed the handler actually returned, not
        // hand-typed, so this exercises exactly what `work_feed` emitted.
        let template = feed_body
            .split("href=\"")
            .skip(1)
            .find_map(|rest| {
                let (path, _) = rest.split_once('"')?;
                path.contains("/pages/{pageNumber}")
                    .then(|| path.to_string())
            })
            .unwrap_or_else(|| panic!("no stream link in feed: {feed_body}"));
        // `0`, because PSE counts from 0 — this substitutes into the template
        // exactly as a client does, so using any other number here would test
        // a request no client makes.
        let page_url = template.replace("{pageNumber}", "0");

        let page = app
            .oneshot(
                Request::get(&page_url)
                    .body(Body::empty())
                    .unwrap_or_else(|e| panic!("{page_url} is not a usable request: {e}")),
            )
            .await
            .unwrap();
        let status = page.status();
        let bytes = axum::body::to_bytes(page.into_body(), 1 << 20)
            .await
            .unwrap();

        crate::test_support::cleanup("taiyaku-test-opds-percent");
        assert!(
            template.contains("%20"),
            "the href must percent-encode the space: {template}"
        );
        assert_eq!(
            status,
            StatusCode::OK,
            "the encoded href must route: {page_url}"
        );
        assert_eq!(
            &bytes[..],
            b"a1.jpg",
            "the decoded key must resolve to the real edition's first page"
        );
    }

    /// Important 5 (whole-branch review): RFC 4287 requires `atom:author` on
    /// a feed unless every entry carries one, and none of taiyaku's do.
    /// `catalog` already had it; `works` and `work_feed` did not.
    #[tokio::test]
    async fn the_works_feed_and_the_work_feed_declare_an_atom_author() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-author"));
        let app = routes().with_state(state);

        let works_body = body_of(
            app.clone()
                .oneshot(
                    Request::get("/opds/v1.2/works")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        let work_body = body_of(
            app.oneshot(
                Request::get("/opds/v1.2/works/t")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;

        crate::test_support::cleanup("taiyaku-test-opds-author");
        assert!(
            works_body.contains("<author><name>taiyaku</name></author>"),
            "the works feed must declare an author: {works_body}"
        );
        assert!(
            work_body.contains("<author><name>taiyaku</name></author>"),
            "the work feed must declare an author: {work_body}"
        );
    }

    /// Important 5: navigation entry links carried no `rel`, so they
    /// defaulted to Atom's `alternate` rather than `subsection` — the `rel`
    /// OPDS catalogs use for an entry that leads to another feed.
    #[tokio::test]
    async fn navigation_entries_carry_a_subsection_rel() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-subsection"));
        let app = routes().with_state(state);

        let catalog_body = body_of(
            app.clone()
                .oneshot(
                    Request::get("/opds/v1.2/catalog")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        let works_body = body_of(
            app.oneshot(
                Request::get("/opds/v1.2/works")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        )
        .await;

        crate::test_support::cleanup("taiyaku-test-opds-subsection");
        assert!(
            catalog_body.contains(r#"rel="subsection""#),
            "the catalog's entry link to /works must carry rel=subsection: {catalog_body}"
        );
        assert!(
            works_body.contains(r#"rel="subsection""#),
            "the works feed's entry link to a work must carry rel=subsection: {works_body}"
        );
    }

    /// Important 5: OPDS 1.2 requires at least one acquisition link
    /// (`rel="http://opds-spec.org/acquisition"`) on every entry in an
    /// acquisition feed, or a conformant client may render the entry as
    /// unopenable. The owner decided it should point at the edition's first
    /// page — proven here by actually fetching it, not merely asserting the
    /// string is present, so this fails if the link is ever a dead one.
    #[tokio::test]
    async fn the_acquisition_link_on_an_edition_entry_is_a_real_fetchable_page() {
        let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acquisition"));
        let app = routes().with_state(state);

        let feed = app
            .clone()
            .oneshot(
                Request::get("/opds/v1.2/works/t")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let feed_body = body_of(feed).await;

        assert!(
            feed_body.contains(r#"rel="http://opds-spec.org/acquisition""#),
            "must declare the acquisition rel: {feed_body}"
        );

        // Pulled out of the feed the handler actually returned: the
        // acquisition link, not the pse:stream template (which contains a
        // literal `{{pageNumber}}` and is not directly fetchable) or the
        // thumbnail link.
        let href = feed_body
            .split("href=\"")
            .skip(1)
            .find_map(|rest| {
                let (path, _) = rest.split_once('"')?;
                (path.contains("/pages/1")).then(|| path.to_string())
            })
            .unwrap_or_else(|| panic!("no concrete page link in feed: {feed_body}"));

        let page = app
            .oneshot(Request::get(&href).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = page.status();

        crate::test_support::cleanup("taiyaku-test-opds-acquisition");
        assert_eq!(
            status,
            StatusCode::OK,
            "the acquisition link must be a real, fetchable page, not a dead link: {href}"
        );
    }

    /// Minor 5 (whole-branch review): a work whose edition's first member is
    /// recorded with zero pages — so `Edition::locate(0)` steps over it to
    /// the second member — must cache the generated cover under the *second*
    /// member's hash, the one `locate(0)` actually reads page 0 from, not
    /// under `members.first()` unconditionally.
    #[tokio::test]
    async fn the_cover_cache_key_follows_locate_past_a_pageless_first_member() {
        let dir = std::env::temp_dir()
            .join("taiyaku-test-opds-cover-pageless-first-member")
            .join("covers");
        let state = Arc::new(ServeState {
            works: vec![work_with_a_pageless_first_member(
                "taiyaku-test-opds-cover-pageless-first-member",
            )],
            source: Arc::new(crate::LocalLibrary),
            cache_dir: dir.clone(),
            log: None,
        });
        let app = routes().with_state(state);

        let response = app
            .oneshot(
                Request::get("/opds/v1.2/works/t/raw/cover")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        // Checked before cleanup: `cache_dir` sits inside the work's own
        // temp directory, which `cleanup` removes.
        let expected = crate::cover::cache_path(&dir, "sha-of-real-second-member");
        let cached_under_the_right_member = expected.exists();

        crate::test_support::cleanup("taiyaku-test-opds-cover-pageless-first-member");
        assert_eq!(status, StatusCode::OK);
        assert!(
            cached_under_the_right_member,
            "the cover must be cached under the second (pages-bearing) member's hash, since \
             that is where locate(0) actually reads page 0 from — not the first, pageless \
             member's: {}",
            expected.display()
        );
    }

    /// A work whose "raw" edition's first member is declared with zero
    /// pages — physically present on disk with real content, but stepped
    /// over by `Edition::locate` purely on the recorded page count, exactly
    /// as `sidecar.md` specifies. The second member's entries have to be
    /// real, decodable images (like `cover::tests::real_page_bytes`) rather
    /// than the shared fixture's plain-ASCII placeholders, because this test
    /// exercises the cover route all the way through `thumbnail`'s decode.
    fn work_with_a_pageless_first_member(dir: &str) -> crate::Work {
        use std::io::Write;

        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let page = image::RgbImage::from_fn(16, 16, |x, y| {
            image::Rgb([(x * 16) as u8, (y * 16) as u8, 200])
        });
        let mut real_image = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(page)
            .write_to(&mut real_image, image::ImageFormat::Png)
            .unwrap();
        let real_image = real_image.into_inner();

        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join("c1.cbz")).unwrap());
            for entry in ["a1.jpg", "a2.jpg"] {
                zip.start_file(entry, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(entry.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join("c2.cbz")).unwrap());
            for entry in ["b1.jpg", "b2.jpg"] {
                zip.start_file(entry, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(&real_image).unwrap();
            }
            zip.finish().unwrap();
        }

        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "sha-of-pageless-first-member", "pages": 0 },
              { "path": "c2.cbz", "sha256": "sha-of-real-second-member", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
        let path = root.join("taiyaku.json");
        std::fs::write(&path, json).unwrap();

        crate::Work {
            id: "t".into(),
            path,
            sidecar: taiyaku_core::Sidecar::from_json(json).unwrap(),
        }
    }
}

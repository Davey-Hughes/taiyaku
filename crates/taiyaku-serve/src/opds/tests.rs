use super::*;

/// `routes()` folded into a `Router`.
///
/// **Deliberately without `auth::guard`**: these tests exercise the feed's own
/// shape — its XML, its links, its page streaming — and the guard is asserted
/// over in `auth::tests::every_route_refuses_an_anonymous_request`, which walks
/// this very list. Building the guarded router here would test the guard
/// again and the feed no better.
///
/// Each request carries [`as_an_administrator`]'s principal in the guard's
/// place, because a request with none sees nothing (`auth::access_of`).
fn feed_router() -> axum::Router<std::sync::Arc<ServeState>> {
    super::routes()
        .into_iter()
        .fold(axum::Router::new(), |router, (path, handler)| {
            router.route(path, handler)
        })
        .layer(axum::Extension(an_administrator()))
}

/// The principal `auth::guard` leaves for an administrator, who sees every
/// library.
fn an_administrator() -> crate::auth::Principal {
    crate::auth::Principal {
        user_id: 1,
        role: crate::auth::Role::Admin,
        can_download: true,
        can_stream: true,
        can_stamp: false,
        access: Access::Every,
    }
}

/// [`an_administrator`], as the extensions of a handler called directly.
fn as_an_administrator() -> axum::http::Extensions {
    let mut extensions = axum::http::Extensions::new();
    extensions.insert(an_administrator());
    extensions
}

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
/// a space and `/` encode, and hex digits are uppercase per RFC 3986.
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
    let tmp = crate::test_support::Fixture::new("opds-root");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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
    let tmp = crate::test_support::Fixture::new("opds-work");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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
/// Panels opens a book at `pages/0`, so a server that answered 404 there would
/// show an error screen. Page 0, a middle page, the last page and one past the
/// end are all asked for, because the boundary at each end is where an
/// off-by-one hides.
///
/// The fixture is four pages whose bytes are their own entry names, so a
/// shifted answer is not merely a wrong status but visibly the wrong page.
#[tokio::test]
async fn pse_page_numbers_are_taiyaku_indices_unchanged() {
    let tmp = crate::test_support::Fixture::new("opds-page");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

    // Page 0 exists and is the first page.
    let (zero_status, zero) = page(0).await;
    // A middle page, to catch a shift that only the ends would otherwise
    // reveal.
    let (_, two) = page(2).await;
    // The last page is `pse:count - 1`, not `pse:count`.
    let (_, last) = page(3).await;
    // One past the end. `pse:count` is 4, so 4 is out of range.
    let (past_end, _) = page(4).await;

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
    let tmp = crate::test_support::Fixture::new("opds-page-nan");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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
/// route — the mistake O5 records, on this same pair of routes — cannot
/// happen silently here.
#[tokio::test]
async fn the_pse_stream_also_carries_an_etag_and_answers_a_matching_request_304() {
    let tmp = crate::test_support::Fixture::new("opds-page-etag");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(
        bytes.is_empty(),
        "a 304 must carry no body on this route either"
    );
}

/// O108: the PSE stream's tag names a page's member and its offset within
/// that member, as the `/api/…` page route's does and for the reason
/// `routes::tests::a_page_tag_names_its_offset_in_its_member_not_its_index_in_the_edition`
/// gives. Panels reaches pages through this route and not that one.
#[tokio::test]
async fn the_pse_stream_tag_names_a_page_offset_in_its_member_not_its_index() {
    async fn fetch(
        app: &axum::Router,
        uri: &str,
        if_none_match: Option<&str>,
    ) -> (StatusCode, Option<String>, axum::body::Bytes) {
        let mut request = Request::get(uri);
        if let Some(tag) = if_none_match {
            request = request.header(header::IF_NONE_MATCH, tag);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let tag = response
            .headers()
            .get(header::ETAG)
            .map(|tag| tag.to_str().unwrap().to_string());
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, tag, body)
    }

    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let app = feed_router().with_state(state);

    let (_, same_page, _) = fetch(&app, "/opds/v1.2/works/m/b/pages/0", None).await;
    let same_page = same_page.expect("an ETag");
    let (_, same_index, _) = fetch(&app, "/opds/v1.2/works/m/b/pages/2", None).await;
    let same_index = same_index.expect("an ETag");
    let uri = "/opds/v1.2/works/m/ab/pages/2";

    let (status, _, body) = fetch(&app, uri, Some(&same_index)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{uri} sent page 2 of `b`'s tag {same_index}, which is a different page"
    );
    assert_eq!(crate::test_support::colour_of(&body), "red", "{uri}");

    let (status, _, _) = fetch(&app, uri, Some(&same_page)).await;
    assert_eq!(
        status,
        StatusCode::NOT_MODIFIED,
        "{uri} sent page 0 of `b`'s tag {same_page}, which is the same page"
    );
}

/// O108: an edition's cover is cached under the hash of the member its
/// page 0 is in.
///
/// Page 0 of `ab` is in `a.cbz`, and page 0 of `b` is in `b.cbz`, which is the
/// last member of both editions (see `test_support::state_with_a_shared_member`).
/// The fixture of `the_cover_cache_key_follows_locate_past_a_pageless_first_member`
/// has one member with pages, and it is the last, so a hash taken from the last
/// member would pass there. Here that would cache `ab`'s grey cover under
/// `b.cbz`'s hash and serve it as `b`'s.
#[tokio::test]
async fn each_edition_s_cover_is_cached_under_the_member_its_first_page_is_in() {
    let (state, _fixture) = crate::test_support::state_with_a_shared_member();
    let dir = state.cache_dir.clone();
    let app = feed_router().with_state(state);
    let colour_of = crate::test_support::colour_of;

    for (edition, sha, colour) in [("ab", "sha-a", "grey"), ("b", "sha-b", "red")] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/opds/v1.2/works/m/{edition}/cover"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "`{edition}`'s cover");
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        assert_eq!(colour_of(&body), colour, "`{edition}`'s cover");

        let path = crate::cover::cache_path(&dir, sha);
        let cached = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(colour_of(&cached), colour, "{}", path.display());
    }
}

/// A feed for a work this server does not hold says so, rather than
/// returning an empty feed a client renders as an empty library.
#[tokio::test]
async fn an_unknown_work_is_refused_rather_than_served_empty() {
    let tmp = crate::test_support::Fixture::new("opds-missing");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("nope"), "must name the work: {body}");
}

#[tokio::test]
async fn the_works_feed_lists_every_work_with_its_edition_count() {
    let tmp = crate::test_support::Fixture::new("opds-works-list");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

/// Through the OPDS stream route, an unknown edition answers 404 and the
/// body names it.
#[tokio::test]
async fn streaming_an_unknown_edition_names_it() {
    let tmp = crate::test_support::Fixture::new("opds-unknown-edition");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("nope"), "must name the edition: {body}");
}

/// Through this route, a page past the edition's end answers 404 and the
/// body names the edition and how many pages it actually has.
#[tokio::test]
async fn streaming_a_page_past_the_end_names_the_edition_and_its_length() {
    let tmp = crate::test_support::Fixture::new("opds-page-past-end");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

    // The fixture edition has 4 pages; PSE page 5 is index 5, past its end.
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

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("raw"), "must name the edition: {body}");
    assert!(body.contains('4'), "must say how many pages it has: {body}");
}

/// `cover` resolves a work and then an edition, as `stream_page` does, so it
/// gets the same router-level test for what an unknown work or edition
/// returns. A route with no test protecting it can regress silently.
#[tokio::test]
async fn a_cover_for_an_unknown_work_or_edition_names_what_was_missing() {
    let tmp = crate::test_support::Fixture::new("opds-cover-missing");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

/// An edition key that is not URL-safe still has to reach the OPDS feed,
/// because the format is meant to be written by other tools — `taiyaku` cannot
/// assume every sidecar it serves was written by its own validating `align`.
/// Percent-encoding the key in every `href` is what keeps the feed usable;
/// XML-escaping alone leaves a literal space or `/` in the URL.
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
    let tmp = crate::test_support::Fixture::new("opds-percent");
    let state = Arc::new(ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(vec![crate::test_support::work_with_edition_key(
            &tmp,
            "raw color",
        )]),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: tmp.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let app = feed_router().with_state(state);

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

/// RFC 4287 requires `atom:author` on a feed unless every entry carries one,
/// and none of taiyaku's do.
#[tokio::test]
async fn the_works_feed_and_the_work_feed_declare_an_atom_author() {
    let tmp = crate::test_support::Fixture::new("opds-author");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

    let all_works_feed_body = body_of(
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
    let single_work_feed_body = body_of(
        app.oneshot(
            Request::get("/opds/v1.2/works/t")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;

    assert!(
        all_works_feed_body.contains("<author><name>taiyaku</name></author>"),
        "the works feed must declare an author: {all_works_feed_body}"
    );
    assert!(
        single_work_feed_body.contains("<author><name>taiyaku</name></author>"),
        "the work feed must declare an author: {single_work_feed_body}"
    );
}

/// Navigation entry links carry `rel="subsection"`, which OPDS catalogs use
/// for an entry that leads to another feed; a link with no `rel` defaults to
/// Atom's `alternate`.
#[tokio::test]
async fn navigation_entries_carry_a_subsection_rel() {
    let tmp = crate::test_support::Fixture::new("opds-subsection");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

    assert!(
        catalog_body.contains(r#"rel="subsection""#),
        "the catalog's entry link to /works must carry rel=subsection: {catalog_body}"
    );
    assert!(
        works_body.contains(r#"rel="subsection""#),
        "the works feed's entry link to a work must carry rel=subsection: {works_body}"
    );
}

/// OPDS 1.2 requires at least one acquisition link
/// (`rel="http://opds-spec.org/acquisition"`) on every entry in an
/// acquisition feed, or a conformant client may render the entry as
/// unopenable. It points at a download — proven by fetching it, not by
/// asserting the string is present, so this fails if the link is ever dead.
#[tokio::test]
async fn the_acquisition_link_is_a_real_fetchable_download() {
    let tmp = crate::test_support::Fixture::new("opds-acq-dl");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

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

    let href = feed_body
        .split("href=\"")
        .skip(1)
        .find_map(|rest| {
            let (path, _) = rest.split_once('"')?;
            path.ends_with("/download").then(|| path.to_string())
        })
        .unwrap_or_else(|| panic!("no download link in feed: {feed_body}"));

    let download = app
        .oneshot(Request::get(&href).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = download.status();

    // The PSE link is a different mechanism for a different intention.
    // Asserted with its `pse:count`, because a test that would still pass with
    // the link deleted is not evidence.
    assert!(
        feed_body.contains(r#"rel="http://vaemendis.net/opds-pse/stream""#)
            && feed_body.contains(r#"pse:count="4""#),
        "streaming stays the in-app read path: {feed_body}"
    );

    assert_eq!(
        status,
        StatusCode::OK,
        "the acquisition link must be a real, fetchable download: {href}"
    );
}

/// The feed declares the type the route will actually send, not a constant.
/// A generated edition is a `STORE` zip; `download::advertised_type` is what
/// decides, so the two cannot drift.
#[tokio::test]
async fn the_acquisition_link_declares_the_type_the_route_will_send() {
    let tmp = crate::test_support::Fixture::new("opds-acq-type");
    let state = Arc::new(crate::test_support::state(&tmp));
    let app = feed_router().with_state(state);

    let feed = app
        .oneshot(
            Request::get("/opds/v1.2/works/t")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let feed_body = body_of(feed).await;

    assert!(
        feed_body.contains(
            r#"<link rel="http://opds-spec.org/acquisition" type="application/vnd.comicbook+zip""#
        ),
        "a generated edition is a STORE zip and must say so: {feed_body}"
    );
    assert!(
        !feed_body.contains(r#"rel="http://opds-spec.org/acquisition" type="image/jpeg""#),
        "the acquisition link is no longer a page: {feed_body}"
    );
}

/// **The work feed asks the filesystem off the async worker** — O120.
///
/// `download::advertised_type` stats an edition of one member to learn what a
/// download of it will be sent as, and `work_feed` must not do that in the
/// handler, on the async worker. A `stat` of a file on this machine's disk
/// answers at once, so nothing stands still for
/// `test_support::the_runtime_stalls_on`'s watchdog to notice, and the feed is
/// the same wherever the `stat` ran. So this asks the kernel which thread
/// made it.
///
/// The test runs itself again as a child under `strace`, as O104's tests do.
/// The child builds a current-thread runtime and calls the handler through
/// `block_on`. The thread calling `block_on` is that runtime's async worker,
/// and every thread the runtime starts is its blocking pool; the child writes
/// down the ids of both. The parent then requires that each member was
/// statted at least once, and only ever by a thread the runtime started.
///
/// **Two editions of one member each, of two types**, so the child can check
/// each entry's link names its own edition's type: a feed that paired the
/// answers with the wrong entries, or dropped them for the fallback, would
/// send the right XML for a single edition of a comic archive.
#[cfg(target_os = "linux")]
#[test]
fn the_work_feed_stats_its_members_on_the_blocking_pool() {
    const CHILD: &str = "TAIYAKU_TEST_OPDS_O120_CHILD";
    const MEMBERS: [(&str, &str, &str); 2] = [
        ("raw", "volume.pdf", "application/pdf"),
        ("en", "volume.epub", "application/epub+zip"),
    ];
    if let Some(dir) = std::env::var_os(CHILD) {
        feed_on_a_current_thread_runtime(std::path::Path::new(&dir), &MEMBERS);
        return;
    }

    let scratch = crate::test_support::Fixture::new("opds-o120-stat");
    // Canonical, because strace prints the path the call was handed, and the
    // member paths are joined onto this one.
    let dir = std::fs::canonicalize(&scratch).unwrap();
    let work_dir = dir.join("work");
    std::fs::create_dir(&work_dir).unwrap();
    for (_, member, _) in MEMBERS {
        std::fs::write(work_dir.join(member), b"statted, never read").unwrap();
    }

    let log = taiyaku_read::test_support::strace(
        "opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool",
        (CHILD, dir.as_os_str()),
        "%%stat",
        &dir.join("strace.log"),
    );
    let threads =
        std::fs::read_to_string(dir.join("threads")).expect("the child wrote its threads");
    let async_worker = threads
        .lines()
        .find_map(|line| line.strip_prefix("async "))
        .expect("the child named its async worker");
    let blocking: Vec<&str> = threads
        .lines()
        .filter_map(|line| line.strip_prefix("blocking "))
        .collect();

    for (_, member, _) in MEMBERS {
        let target = work_dir.join(member);
        let by = statted_by(&log, &target);
        assert!(
            !by.is_empty(),
            "nothing statted {}, so where the feed asked proves nothing",
            target.display()
        );
        let elsewhere: Vec<&str> = by
            .into_iter()
            .filter(|thread| !blocking.contains(thread))
            .collect();
        assert!(
            elsewhere.is_empty(),
            "{} was statted by thread(s) {elsewhere:?}, which the runtime did not start: \
             the async worker is {async_worker} and the blocking pool is {blocking:?}",
            target.display()
        );
    }
}

/// The child half of [`the_work_feed_stats_its_members_on_the_blocking_pool`]:
/// the feed for a work whose editions are `members`, one file each under
/// `dir/work`, asked on a current-thread runtime, with the async worker's
/// thread id and every started thread's written to `dir/threads`.
#[cfg(target_os = "linux")]
fn feed_on_a_current_thread_runtime(dir: &std::path::Path, members: &[(&str, &str, &str)]) {
    use axum::extract::{Path as AxPath, State};

    let started = Arc::new(std::sync::Mutex::new(Vec::new()));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .on_thread_start({
            let started = Arc::clone(&started);
            move || started.lock().unwrap().push(this_thread())
        })
        .build()
        .expect("a current-thread runtime");

    let editions = members
        .iter()
        .map(|(key, member, _)| {
            format!(
                r#""{key}": {{ "label": "{key}", "members": [
                  {{ "path": "{member}", "sha256": "{key}", "pages": 1 }}
                ], "spreads": [] }}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "rtl" }},
          "editions": {{ {editions} }},
          "hub": "{hub}",
          "alignments": []
        }}"#,
        hub = members[0].0
    );
    let mut state = crate::test_support::state(dir);
    state.works = crate::Works::fixed(vec![crate::Work::new(
        "t".into(),
        dir.join("work").join("taiyaku.json"),
        taiyaku_core::Sidecar::from_json(&json).unwrap(),
    )]);
    let state = Arc::new(state);

    let async_worker = this_thread();
    let body = runtime.block_on(async {
        let response =
            crate::opds::work_feed(State(state), AxPath("t".to_string()), as_an_administrator())
                .await;
        assert_eq!(response.status(), StatusCode::OK);
        body_of(response).await
    });
    for (key, _, expected) in members {
        let entry = body
            .split("<entry>")
            .find(|entry| entry.contains(&format!("<id>taiyaku:edition:t:{key}</id>")))
            .unwrap_or_else(|| panic!("no entry for `{key}`: {body}"));
        assert!(
            entry.contains(&format!(
                r#"rel="http://opds-spec.org/acquisition" type="{expected}""#
            )),
            "`{key}`'s acquisition link must declare {expected}: {entry}"
        );
    }

    let mut threads = String::new();
    let _ = writeln!(threads, "async {async_worker}");
    for thread in started.lock().unwrap().iter() {
        let _ = writeln!(threads, "blocking {thread}");
    }
    std::fs::write(dir.join("threads"), threads).unwrap();
}

/// This thread's id as the kernel numbers it, which is what `strace -f`
/// prints at the head of each line: `/proc/thread-self` links to
/// `<pid>/task/<tid>`.
#[cfg(target_os = "linux")]
fn this_thread() -> String {
    std::fs::read_link("/proc/thread-self")
        .expect("reading /proc/thread-self")
        .file_name()
        .expect("a thread id")
        .to_string_lossy()
        .into_owned()
}

/// The ids of the threads that made a `stat`-family call naming `target`, from
/// a log `strace -f` wrote. Each line begins with the thread's id. A call
/// another thread interrupted is split across two lines, and the first, which
/// carries the path, is the one read.
#[cfg(target_os = "linux")]
fn statted_by<'a>(log: &'a str, target: &std::path::Path) -> Vec<&'a str> {
    let target = target.to_str().expect("a UTF-8 fixture path");
    log.lines()
        .filter_map(|line| {
            let (thread, call) = line.split_once(' ')?;
            let name = call.trim_start().split('(').next()?;
            let named = call.split('"').nth(1)?;
            (name.contains("stat") && named == target).then_some(thread)
        })
        .collect()
}

/// The web UI's control is *absent* for a reader who may not download, and
/// the feed cannot do the same: OPDS 1.2 requires an acquisition link on
/// every entry. So the page link is the fallback rather than being deleted,
/// and the entry carries exactly one acquisition link either way.
///
/// **Both branches are driven off one flipped column, deliberately.** An
/// assertion that a withheld reader sees the page link passes just as well
/// against a feed that only ever emits the page link. Only the pair
/// distinguishes a working branch from a missing one.
#[tokio::test]
async fn the_acquisition_link_follows_can_download_in_both_directions() {
    async fn feed_for(dir: &std::path::Path, may_download: bool) -> String {
        let state = Arc::new(crate::test_support::state(dir));
        {
            let conn = state.library.conn();
            // **Demoted in BOTH arms, which is the point.** An administrator's
            // capabilities are unconditionally true (O86), so withholding one
            // from the seeded admin withholds nothing — but demoting only the
            // withheld arm would vary the role *and* the capability between
            // the two feeds, and this test exists to attribute the difference
            // to `can_download` alone.
            conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
                .expect("a capability is withheld from a user, never an admin");
            if !may_download {
                conn.execute("UPDATE user SET can_download = 0", [])
                    .expect("withholds the capability");
            }
        }
        let router = crate::test_support::key_authenticating(crate::build_router(state));
        let response = crate::test_support::get(&router, "/opds/v1.2/works/t").await;
        assert_eq!(response.status(), StatusCode::OK);
        crate::test_support::body_string(response).await
    }

    let tmp = crate::test_support::Fixture::new("opds-acq-yes");
    let granted = feed_for(&tmp, true).await;
    let tmp2 = crate::test_support::Fixture::new("opds-acq-no");
    let withheld = feed_for(&tmp2, false).await;

    for (name, body) in [("granted", &granted), ("withheld", &withheld)] {
        assert_eq!(
            body.matches(r#"rel="http://opds-spec.org/acquisition""#)
                .count(),
            1,
            "{name}: exactly one acquisition link per entry, whichever it is: {body}"
        );
    }

    assert!(
        granted.contains(r#"/download"/>"#) && !granted.contains(r#"/pages/0"/>"#),
        "a reader who may download is offered the download: {granted}"
    );
    assert!(
        withheld.contains(r#"/pages/0"/>"#) && !withheld.contains(r#"/download"/>"#),
        "a reader who may not download keeps the page link, rather than a \
         link that 403s on every attempt: {withheld}"
    );

    // **The fallback is fetched, not merely matched.** The page link is the
    // one branch most in need of "not a dead link" proof: PSE counts from 0, so
    // `pages/1` would be the *second* page and a 404 for any one-page edition.
    let tmp3 = crate::test_support::Fixture::new("opds-acq-fetch");
    let state = Arc::new(crate::test_support::state(&tmp3));
    {
        let conn = state.library.conn();
        // Demoted first, as in `feed_for` above: an administrator's
        // capabilities are unconditionally true (O86), so this block would
        // otherwise fetch the *download* link and prove nothing about the
        // fallback it exists to prove is alive.
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a capability is withheld from a user, never an admin");
        conn.execute("UPDATE user SET can_download = 0", [])
            .expect("withholds the capability");
    }
    let router = crate::test_support::key_authenticating(crate::build_router(state));
    let feed = crate::test_support::get(&router, "/opds/v1.2/works/t").await;
    let body = crate::test_support::body_string(feed).await;
    let href = body
        .split("href=\"")
        .skip(1)
        .find_map(|rest| {
            let (path, _) = rest.split_once('"')?;
            path.ends_with("/pages/0").then(|| path.to_string())
        })
        .unwrap_or_else(|| panic!("no page link in the withheld feed: {body}"));
    let page = crate::test_support::get(&router, &href).await;
    let status = page.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "the fallback must be a real, fetchable page, not a dead link: {href}"
    );
}

/// A work whose edition's first member is recorded with zero pages — so
/// `Edition::locate(0)` steps over it to the second member — must cache the
/// generated cover under the *second* member's hash, the one `locate(0)`
/// actually reads page 0 from, not under `members.first()` unconditionally.
#[tokio::test]
async fn the_cover_cache_key_follows_locate_past_a_pageless_first_member() {
    let tmp = crate::test_support::Fixture::new("opds-cover-pageless-first-member");
    let dir = tmp.join("covers");
    let state = Arc::new(ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(vec![work_with_a_pageless_first_member(&tmp)]),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: dir.clone(),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let app = feed_router().with_state(state);

    let response = app
        .oneshot(
            Request::get("/opds/v1.2/works/t/raw/cover")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    // `cache_dir` sits inside the work's own fixture directory, which goes
    // when `tmp` is dropped at the end of the test.
    let expected = crate::cover::cache_path(&dir, "sha-of-real-second-member");
    let cached_under_the_right_member = expected.exists();

    assert_eq!(status, StatusCode::OK);
    assert!(
        cached_under_the_right_member,
        "the cover must be cached under the second (pages-bearing) member's hash, since \
             that is where locate(0) actually reads page 0 from — not the first, pageless \
             member's: {}",
        expected.display()
    );
}

/// A pixel channel value synthesized for `work_with_a_pageless_first_member`
/// below.
///
/// # Panics
///
/// Panics if `v` exceeds `u8::MAX`. It cannot: the caller multiplies a
/// coordinate `from_fn` bounds to `0..16` by 16, topping out at `15 * 16
/// == 240`.
fn channel(v: u32) -> u8 {
    u8::try_from(v).expect("channel value exceeds u8::MAX")
}

/// A work whose "raw" edition's first member is declared with zero
/// pages — physically present on disk with real content, but stepped
/// over by `Edition::locate` purely on the recorded page count, exactly
/// as `sidecar.md` specifies. The second member's entries have to be
/// real, decodable images (like `cover::tests::real_page_bytes`) rather
/// than the shared fixture's plain-ASCII placeholders, because this test
/// exercises the cover route all the way through `thumbnail`'s decode.
fn work_with_a_pageless_first_member(dir: &std::path::Path) -> crate::Work {
    use std::io::Write;

    let root = dir;

    let page = image::RgbImage::from_fn(16, 16, |x, y| {
        let r = channel(x * 16);
        let g = channel(y * 16);
        image::Rgb([r, g, 200])
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

    crate::Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    )
}

// ---------------------------------------------------------------------------
// The grant gap: `opds.rs` must apply spec C1's per-root visibility, as the
// `/api/…` side does through `view::of_edition` and `get_sidecar`'s use of
// `browse::work`. `taiyaku library user grant-library` ships grants for real,
// so a restricted account reading the bytes of a root it does not hold through
// these routes is not theoretical.
//
// Every fixture below imports through `taiyaku_library::import::scanned`
// into a real library database rather than handing `ServeState` an
// in-memory-only `Work` — `visible_members`/`browse::work` read `work`,
// `edition` and `member` rows, and a work absent from those tables cannot
// exercise the grant at all, only the "work does not exist" path. And the
// page/cover fixture writes a real, decodable image rather than reusing
// `test_support::work`'s placeholder bytes: those routes read *bytes*, not
// rows, so a 404 from a nonexistent placeholder path would be
// indistinguishable from the grant actually refusing anything.
// ---------------------------------------------------------------------------

/// A router over one work with one real, decodable page, imported into the
/// library's own database and filed entirely under a root the fixture
/// account (`user_id` 1, via `test_support::authenticating`) is granted a
/// *different* root than — so every edition of this work is wholly outside
/// the grant.
///
/// Used both for `stream_page`/`cover`'s bypass (a route that should refuse
/// the bytes) and for `work_feed`'s full-hide case (a route that should
/// refuse the feed itself), since both need the same "nothing here is
/// visible" shape.
fn router_with_a_real_page_under_a_withheld_root(
    dir: &std::path::Path,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    use std::io::Write as _;

    let root = dir;

    let page = image::RgbImage::from_fn(16, 16, |x, y| {
        let r = channel(x * 16);
        let g = channel(y * 16);
        image::Rgb([r, g, 200])
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(page)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let real_image = encoded.into_inner();

    let member_path = root.join("c1.cbz");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&member_path).unwrap());
        zip.start_file("p1.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&real_image).unwrap();
        zip.finish().unwrap();
    }

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Withheld", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{path}", "sha256": "real", "pages": 1 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#,
        path = member_path.display()
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");
        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
        // Every member of this work's one edition lives under the root the
        // reader is NOT granted.
        conn.execute("UPDATE member SET root_id = 2", [])
            .expect("files the material under the withheld root");
        crate::test_support::grant_root(&conn, 1);
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = crate::test_support::key_authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// The directories `/held` (1) and `/withheld` (2), each the one directory of a
/// library: `Manga & Comics` numbered 70 and `withheld` 80, apart from the
/// directories, so a handler that read a library's id as a directory's finds no
/// directory of that number rather than the one that happens to share it.
///
/// **The held directory's library has an `&` in its name**, so a feed that wrote
/// a library's name unescaped would be one no client can parse; a feed must
/// write it as [`HELD_LIBRARY`]. Listed by name, it still comes first.
fn two_dirs_numbered_apart_from_their_libraries(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (70, 'Manga & Comics'), (80, 'withheld');
         INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld');
         INSERT INTO library_root (library_id, root_id) VALUES (70, 1), (80, 2);",
    )
    .expect("two directories, each in a library of its own");
}

/// The held directory's library's name as a feed must write it: escaped.
const HELD_LIBRARY: &str = "Manga &amp; Comics";

/// The library `dir` is in, read from the store, and checked to be
/// numbered apart from every directory: a handler that read a library's id as a
/// directory's would otherwise find the directory of the same number and pass.
fn library_of(state: &ServeState, dir: i64) -> i64 {
    let conn = state.library.conn();
    let library = crate::test_support::library_of_root(&conn, dir);
    let dirs: Vec<i64> = conn
        .prepare("SELECT id FROM root")
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert!(
        !dirs.contains(&library),
        "library {library} shares a directory's id: {dirs:?}"
    );
    library
}

/// A router over one work with **two** editions, each filed under a
/// different root — `en` under the root the fixture account holds, `raw`
/// under the one it does not. What `work_feed`'s partial-visibility test
/// needs: a work that is itself visible, with exactly one of its editions
/// outside the grant.
///
/// Each directory is a library of its own, `Manga & Comics` and `withheld`
/// ([`two_dirs_numbered_apart_from_their_libraries`]), so the work is
/// O191's shape: one work, an edition under each library's directory.
fn router_with_two_editions_split_across_roots(
    dir: &std::path::Path,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    let root = dir;
    let shelf = root.to_string_lossy().into_owned();

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Split", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/raw/c1.cbz", "sha256": "x", "pages": 2 }}
            ], "spreads": [] }},
            "en": {{ "label": "En", "members": [
              {{ "path": "{shelf}/en/d1.cbz", "sha256": "p", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let doc = taiyaku_core::Sidecar::from_json(&json).expect("fixture sidecar parses");
        let work_id =
            taiyaku_library::import::scanned(&conn, &doc, None).expect("import as a scan");
        two_dirs_numbered_apart_from_their_libraries(&conn);
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id = \
             (SELECT id FROM edition WHERE work_id = ?1 AND key = 'raw')",
            [work_id],
        )
        .expect("files raw under the withheld root");
        conn.execute(
            "UPDATE member SET root_id = 1 WHERE edition_id = \
             (SELECT id FROM edition WHERE work_id = ?1 AND key = 'en')",
            [work_id],
        )
        .expect("files en under the held root");
        crate::test_support::grant_root(&conn, 1);
    }
    let works = crate::works::from_library(&library).expect("projects the imported work");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = crate::test_support::key_authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// A router over **two** works, one filed under the root the fixture
/// account holds and one under a root it does not. What the `works` list
/// feed's test needs: a work it must keep naming beside one it must not.
///
/// Each directory is a library of its own, `Manga & Comics` and `withheld`
/// ([`two_dirs_numbered_apart_from_their_libraries`]).
fn router_with_two_works_one_withheld(
    dir: &std::path::Path,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    let root = dir;
    let shelf = root.to_string_lossy().into_owned();

    let visible_json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Visible Work", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/v1.cbz", "sha256": "v", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );
    let hidden_json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "Hidden Work", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{shelf}/h1.cbz", "sha256": "h", "pages": 2 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );

    let library = crate::test_support::library();
    {
        let conn = library.conn();
        let visible_doc =
            taiyaku_core::Sidecar::from_json(&visible_json).expect("visible sidecar parses");
        let visible_id = taiyaku_library::import::scanned(&conn, &visible_doc, None)
            .expect("import the visible work");
        let hidden_doc =
            taiyaku_core::Sidecar::from_json(&hidden_json).expect("hidden sidecar parses");
        let hidden_id = taiyaku_library::import::scanned(&conn, &hidden_doc, None)
            .expect("import the hidden work");

        two_dirs_numbered_apart_from_their_libraries(&conn);
        conn.execute(
            "UPDATE member SET root_id = 1 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [visible_id],
        )
        .expect("files the visible work under the held root");
        conn.execute(
            "UPDATE member SET root_id = 2 WHERE edition_id IN \
             (SELECT id FROM edition WHERE work_id = ?1)",
            [hidden_id],
        )
        .expect("files the hidden work under the withheld root");
        crate::test_support::grant_root(&conn, 1);
    }
    let works = crate::works::from_library(&library).expect("projects the imported works");
    let state = std::sync::Arc::new(ServeState {
        library,
        works: crate::Works::fixed(works),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router = crate::test_support::key_authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// **The core of the gap.** `stream_page` and `cover` must ask about the grant,
/// or a reader restricted to one root could read the bytes of a work filed
/// under a different root through the OPDS routes alone.
///
/// Proven with real, decodable page bytes rather than a coincidental
/// filesystem 404: `router_with_a_real_page_under_a_withheld_root` writes an
/// actual PNG that an un-guarded route would serve, so a passing assertion
/// here cannot be satisfied by the member file simply not existing.
///
/// **Bodies are decoded with `from_utf8_lossy`, not `test_support::body_string`.**
/// An un-guarded route answers `200` with the PNG's raw bytes, which are not
/// valid UTF-8, and `body_string`'s `unwrap` would turn that into a panic
/// naming the exact byte sequence served. That is itself the bypass this test
/// exists to catch, but a panic is a worse failure message than an
/// assertion. Lossy decoding keeps the proof as a normal, readable assertion
/// failure instead.
#[tokio::test]
async fn a_restricted_reader_cannot_stream_pages_or_covers_from_a_root_they_do_not_hold() {
    let tmp = crate::test_support::Fixture::new("opds-grant-stream");
    let (router, state) = router_with_a_real_page_under_a_withheld_root(&tmp);
    let slug = state.works.snapshot()[0].id.clone();

    let page =
        crate::test_support::get(&router, &format!("/opds/v1.2/works/{slug}/raw/pages/0")).await;
    let page_status = page.status();
    let page_bytes = axum::body::to_bytes(page.into_body(), 1 << 20)
        .await
        .unwrap();
    let page_body = String::from_utf8_lossy(&page_bytes).into_owned();

    let cover =
        crate::test_support::get(&router, &format!("/opds/v1.2/works/{slug}/raw/cover")).await;
    let cover_status = cover.status();
    let cover_bytes = axum::body::to_bytes(cover.into_body(), 1 << 20)
        .await
        .unwrap();
    let cover_body = String::from_utf8_lossy(&cover_bytes).into_owned();

    assert_eq!(
        page_status,
        StatusCode::NOT_FOUND,
        "a restricted reader must not stream a page from a withheld root: {page_body}"
    );
    assert_eq!(
        page_body,
        format!("no work `{slug}`"),
        "must be refused by the grant, not by the filesystem"
    );
    assert_eq!(
        cover_status,
        StatusCode::NOT_FOUND,
        "a restricted reader must not fetch a cover from a withheld root: {cover_body}"
    );
    assert_eq!(
        cover_body,
        format!("no work `{slug}`"),
        "must be refused by the grant, not by the filesystem"
    );
}

/// A work that is itself visible must still hide the one edition outside
/// the grant — the feed's own entries are the leak, not merely the route
/// that lists works.
#[tokio::test]
async fn a_restricted_readers_work_feed_lists_only_the_editions_they_may_see() {
    let tmp = crate::test_support::Fixture::new("opds-grant-work-feed");
    let (router, state) = router_with_two_editions_split_across_roots(&tmp);
    let slug = state.works.snapshot()[0].id.clone();

    let response = crate::test_support::get(&router, &format!("/opds/v1.2/works/{slug}")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a partly-visible work must still be served: {body}"
    );
    assert!(
        body.contains("/en/"),
        "the granted edition must still be listed: {body}"
    );
    assert!(
        !body.contains("/raw/"),
        "an edition outside the grant must not be named in the work's own feed: {body}"
    );
}

/// The list feed names each work with a count of its editions, and a count
/// computed over every edition regardless of the caller's grant would tell a
/// restricted reader how many editions a work holds under a root they cannot
/// see, even though the work's own feed just above already hides every one of
/// those editions by name. A count is a smaller leak than a key or a path, and
/// it is still one.
#[tokio::test]
async fn a_restricted_readers_works_list_counts_only_the_editions_they_may_see() {
    let tmp = crate::test_support::Fixture::new("opds-grant-works-count");
    let (router, state) = router_with_two_editions_split_across_roots(&tmp);
    let slug = state.works.snapshot()[0].id.clone();

    let response = crate::test_support::get(&router, "/opds/v1.2/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains(&format!("href=\"/opds/v1.2/works/{slug}\"")),
        "the granted work must still be listed: {body}"
    );
    assert!(
        body.contains("<content type=\"text\">1 editions</content>"),
        "the edition count must be filtered to the one edition this account \
         may see, not the work's real total of two: {body}"
    );
    assert!(
        !body.contains("<content type=\"text\">2 editions</content>"),
        "the list feed named the withheld edition through its count: {body}"
    );
}

/// The full-hide case beside the partial one above: a work with no edition
/// inside the grant must answer exactly as a work that does not exist —
/// `browse::work`'s own contract.
#[tokio::test]
async fn a_restricted_readers_work_feed_404s_when_no_edition_is_visible() {
    let tmp = crate::test_support::Fixture::new("opds-grant-work-feed-hidden");
    let (router, state) = router_with_a_real_page_under_a_withheld_root(&tmp);
    let slug = state.works.snapshot()[0].id.clone();

    let response = crate::test_support::get(&router, &format!("/opds/v1.2/works/{slug}")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a wholly hidden work must not be served: {body}"
    );
    assert_eq!(body, format!("no work `{slug}`"));
}

/// The list feed's own leak: naming every work this server holds regardless of
/// the caller's grant would admit the existence of material a restricted reader
/// cannot see anywhere else.
#[tokio::test]
async fn the_works_feed_does_not_name_a_work_outside_the_grant() {
    let tmp = crate::test_support::Fixture::new("opds-grant-works-list");
    let (router, _state) = router_with_two_works_one_withheld(&tmp);

    let response = crate::test_support::get(&router, "/opds/v1.2/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Visible Work"),
        "the granted work must still be listed: {body}"
    );
    assert!(
        !body.contains("Hidden Work"),
        "a work outside the grant must not be named in the works feed: {body}"
    );
}

/// A reader granted the library `Shelf` is fed the work under each of its two
/// directories, and not the work under `Other`.
#[tokio::test]
async fn the_works_feed_names_every_dir_of_a_granted_library() {
    let tmp = crate::test_support::Fixture::new("opds-granted-library");
    let (router, _state) = crate::test_support::router_over_a_granted_two_dir_library(&tmp);

    let response = crate::test_support::get(&router, "/opds/v1.2/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    for title in ["First Directory Work", "Second Directory Work"] {
        assert!(
            body.contains(title),
            "a work under a directory of the granted library is missing: {title}: {body}"
        );
    }
    assert!(
        !body.contains("Other Library Work"),
        "a work of a library that was not granted is named: {body}"
    );
}

// --- The Libraries level, by directory and by work. ---
//
// Every fixture here numbers its libraries apart from their directories (see
// `two_dirs_numbered_apart_from_their_libraries`) and reads them back
// through `library_of`, which checks it.

/// GET `uri`, its status and its body.
async fn fetch(router: &axum::Router, uri: &str) -> (StatusCode, String) {
    let response = crate::test_support::get(router, uri).await;
    let status = response.status();
    (status, crate::test_support::body_string(response).await)
}

/// The one `<entry>` of `body` whose `<id>` is `id`.
fn entry<'a>(body: &'a str, id: &str) -> &'a str {
    let wanted = format!("<id>{id}</id>");
    let found: Vec<&str> = body
        .split("<entry>")
        .skip(1)
        .filter(|entry| entry.contains(&wanted))
        .collect();
    assert_eq!(found.len(), 1, "one entry with the id {id}: {body}");
    found[0]
}

/// What the feed at `uri` says of the work `slug`, its `<content>`, or `None`
/// where the feed does not list it. The feed must answer 200.
async fn listed_as(router: &axum::Router, uri: &str, slug: &str) -> Option<String> {
    let (status, body) = fetch(router, uri).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    let wanted = format!("<id>taiyaku:work:{slug}</id>");
    body.split("<entry>")
        .skip(1)
        .find(|entry| entry.contains(&wanted))
        .map(|entry| {
            entry
                .split(r#"<content type="text">"#)
                .nth(1)
                .and_then(|rest| rest.split("</content>").next())
                .unwrap_or_else(|| panic!("an entry with no content: {entry}"))
                .to_string()
        })
}

/// The link a navigation entry carries to the feed at `href`.
fn subsection(href: &str) -> String {
    format!(r#"<link rel="subsection" type="{ATOM};kind=navigation" href="{href}"/>"#)
}

/// The catalog leads to both library views, and to every work, and says what
/// the by-work view lists.
#[tokio::test]
async fn the_catalog_links_to_the_libraries() {
    let tmp = crate::test_support::Fixture::new("opds-catalog-libraries");
    let app = feed_router().with_state(Arc::new(crate::test_support::state(&tmp)));

    let (status, body) = fetch(&app, "/opds/v1.2/catalog").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for href in [
        "/opds/v1.2/libraries",
        "/opds/v1.2/libraries/by-work",
        "/opds/v1.2/works",
    ] {
        assert!(
            body.contains(&subsection(href)),
            "the catalog must lead to {href}: {body}"
        );
    }
    let by_work = entry(&body, "taiyaku:libraries:by-work");
    assert!(
        by_work.contains(
            r#"<content type="text">Each library, with the works it shows, each with every edition you may read</content>"#
        ),
        "{by_work}"
    );
}

/// Unrestricted: both libraries, each linking to its works, in
/// `browse::status`'s order, the held directory's library before `withheld`.
/// The held directory's library's name is written escaped, in its `<title>` and
/// its `<content>`.
#[tokio::test]
async fn the_libraries_feed_lists_each_library_with_a_link_to_its_works() {
    let tmp = crate::test_support::Fixture::new("opds-libraries");
    let (_router, state) = router_with_two_works_one_withheld(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);
    let app = feed_router().with_state(state);

    let (status, body) = fetch(&app, "/opds/v1.2/libraries").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<id>taiyaku:libraries</id>"), "{body}");
    assert!(body.contains("<title>Libraries</title>"), "{body}");
    assert!(
        body.contains(&format!(
            r#"<link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/libraries"/>"#
        )),
        "{body}"
    );
    for (library, name) in [(held, HELD_LIBRARY), (withheld, "withheld")] {
        let entry = entry(&body, &format!("taiyaku:library:{library}"));
        assert!(entry.contains(&format!("<title>{name}</title>")), "{entry}");
        assert!(
            entry.contains(&format!(
                r#"<content type="text">The works in {name}</content>"#
            )),
            "{entry}"
        );
        assert!(
            entry.contains(&subsection(&format!(
                "/opds/v1.2/libraries/{library}/works"
            ))),
            "{entry}"
        );
    }
    let at = |library: i64| {
        body.find(&format!("<id>taiyaku:library:{library}</id>"))
            .expect("listed")
    };
    assert!(at(held) < at(withheld), "held before withheld: {body}");
}

/// The account is granted the held directory's library: it is fed that library
/// and its link, and nothing of `withheld`.
#[tokio::test]
async fn a_restricted_reader_s_libraries_feed_names_only_its_libraries() {
    let tmp = crate::test_support::Fixture::new("opds-libraries-restricted");
    let (router, state) = router_with_two_works_one_withheld(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);

    let (status, body) = fetch(&router, "/opds/v1.2/libraries").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let granted = entry(&body, &format!("taiyaku:library:{held}"));
    assert!(
        granted.contains(&format!("<title>{HELD_LIBRARY}</title>")),
        "{granted}"
    );
    assert!(
        granted.contains(&subsection(&format!("/opds/v1.2/libraries/{held}/works"))),
        "{granted}"
    );
    assert!(
        !body.contains("withheld"),
        "a library outside the grant is named: {body}"
    );
    assert!(
        !body.contains(&format!("/opds/v1.2/libraries/{withheld}/")),
        "a library outside the grant is linked: {body}"
    );
}

/// Unrestricted: each library's feed lists the works under its own directories
/// and no other's, under the library's name, escaped.
#[tokio::test]
async fn a_library_s_works_feed_lists_only_the_works_under_its_dirs() {
    let tmp = crate::test_support::Fixture::new("opds-library-works");
    let (_router, state) = router_with_two_works_one_withheld(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);
    let app = feed_router().with_state(state);

    for (library, name, names, omits) in [
        (withheld, "withheld", "Hidden Work", "Visible Work"),
        (held, HELD_LIBRARY, "Visible Work", "Hidden Work"),
    ] {
        let uri = format!("/opds/v1.2/libraries/{library}/works");
        let (status, body) = fetch(&app, &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert!(
            body.contains(&format!("<id>taiyaku:library:{library}:works</id>")),
            "{uri}: {body}"
        );
        assert!(
            body.contains(&format!("<title>{name}</title>")),
            "{uri}: {body}"
        );
        assert!(
            body.contains(&format!(
                r#"<link rel="self"  type="{ATOM};kind=navigation" href="{uri}"/>"#
            )),
            "{uri}: {body}"
        );
        assert!(body.contains(names), "{uri} must name {names}: {body}");
        assert!(!body.contains(omits), "{uri} named {omits}: {body}");
    }
}

/// A library outside the grant is answered as one that does not exist: the
/// same status and the same words, but for the number. The held directory's
/// library answers 200, so the 404 is the grant's and not the route's.
#[tokio::test]
async fn a_library_outside_the_grant_is_refused_as_one_that_does_not_exist() {
    let tmp = crate::test_support::Fixture::new("opds-library-refused");
    let (router, state) = router_with_two_works_one_withheld(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);

    let (status, body) = fetch(&router, &format!("/opds/v1.2/libraries/{held}/works")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("Visible Work"), "{body}");

    let (outside_status, outside) =
        fetch(&router, &format!("/opds/v1.2/libraries/{withheld}/works")).await;
    let (unknown_status, unknown) = fetch(&router, "/opds/v1.2/libraries/999999/works").await;
    assert_eq!(outside_status, StatusCode::NOT_FOUND, "{outside}");
    assert_eq!(unknown_status, StatusCode::NOT_FOUND, "{unknown}");
    assert_eq!(
        outside.replace(&withheld.to_string(), "{library}"),
        unknown.replace("999999", "{library}"),
        "a library outside the grant must be refused in the words one that \
         does not exist is"
    );
}

/// Spec §4's N: the account granted Kids, whose directory sits inside Manga's,
/// is refused Manga's feed by id, by directory and by work, in the words a
/// library that does not exist is, and no feed it is served names Manga or
/// Manga's book. Kids' own feed opens.
#[tokio::test]
async fn a_library_around_the_grant_is_refused_and_named_in_no_feed() {
    let n = crate::test_support::router_over_a_library_nested_in_another();

    for view in ["works", "works/by-work"] {
        let (status, refused) = fetch(
            &n.kids_reader,
            &format!("/opds/v1.2/libraries/{}/{view}", n.manga),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{view}: {refused}");
        assert!(!refused.contains("Manga"), "{view} names Manga: {refused}");
        let (_, unknown) = fetch(
            &n.kids_reader,
            &format!("/opds/v1.2/libraries/999999/{view}"),
        )
        .await;
        assert_eq!(
            refused.replace(&n.manga.to_string(), "{library}"),
            unknown.replace("999999", "{library}"),
            "{view}"
        );
    }

    for (uri, names) in [
        ("/opds/v1.2/libraries".to_owned(), "<title>Kids</title>"),
        (
            "/opds/v1.2/libraries/by-work".to_owned(),
            "<title>Kids</title>",
        ),
        ("/opds/v1.2/works".to_owned(), "Kids Book"),
        (
            format!("/opds/v1.2/libraries/{}/works", n.kids),
            "Kids Book",
        ),
    ] {
        let (status, feed) = fetch(&n.kids_reader, &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {feed}");
        assert!(feed.contains(names), "{uri} must name {names}: {feed}");
        assert!(!feed.contains("Manga"), "{uri} names Manga: {feed}");
    }
}

/// Spec §4's N, the account granted Manga, whose directory holds Kids': Manga's
/// by-work feed names Kids' book beside Manga's, and neither libraries feed
/// lists Kids, which the account reaches and was not granted.
#[tokio::test]
async fn a_librarys_by_work_feed_names_what_it_reaches_and_no_feed_lists_a_library_not_granted() {
    let n = crate::test_support::router_over_a_library_nested_in_another();

    let uri = format!("/opds/v1.2/libraries/{}/works/by-work", n.manga);
    let (status, feed) = fetch(&n.manga_reader, &uri).await;
    assert_eq!(status, StatusCode::OK, "{feed}");
    for book in ["Manga Book", "Kids Book"] {
        assert!(feed.contains(book), "{uri} must name {book}: {feed}");
    }

    for uri in ["/opds/v1.2/libraries", "/opds/v1.2/libraries/by-work"] {
        let (status, feed) = fetch(&n.manga_reader, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {feed}");
        assert!(
            feed.contains("<title>Manga"),
            "{uri} must list Manga: {feed}"
        );
        assert!(!feed.contains("Kids"), "{uri} lists Kids: {feed}");
    }
}

/// O191's shape: one work, an edition under each library's directory. A
/// library's feed counts the editions under its directories, `/opds/v1.2/works`
/// every edition.
#[tokio::test]
async fn a_library_s_works_feed_counts_only_the_editions_under_its_dirs() {
    let tmp = crate::test_support::Fixture::new("opds-library-count");
    let (_router, state) = router_with_two_editions_split_across_roots(&tmp);
    let held = library_of(&state, 1);
    let slug = state.works.snapshot()[0].id.clone();
    let app = feed_router().with_state(state);

    assert_eq!(
        listed_as(&app, &format!("/opds/v1.2/libraries/{held}/works"), &slug)
            .await
            .as_deref(),
        Some("1 editions"),
        "the held library's feed counts the edition under its directory"
    );
    assert_eq!(
        listed_as(&app, "/opds/v1.2/works", &slug).await.as_deref(),
        Some("2 editions"),
        "the works feed counts every edition"
    );
}

/// Unrestricted: both libraries again, each linking to its works with every
/// edition.
#[tokio::test]
async fn the_by_work_libraries_feed_links_each_library_to_its_works_with_every_edition() {
    let tmp = crate::test_support::Fixture::new("opds-libraries-by-work");
    let (_router, state) = router_with_two_works_one_withheld(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);
    let app = feed_router().with_state(state);

    let (status, body) = fetch(&app, "/opds/v1.2/libraries/by-work").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("<id>taiyaku:libraries:by-work</id>"),
        "{body}"
    );
    assert!(
        body.contains("<title>Libraries (by work)</title>"),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            r#"<link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/libraries/by-work"/>"#
        )),
        "{body}"
    );
    for (library, name) in [(held, HELD_LIBRARY), (withheld, "withheld")] {
        let entry = entry(&body, &format!("taiyaku:library:{library}:by-work"));
        assert!(entry.contains(&format!("<title>{name}</title>")), "{entry}");
        assert!(
            entry.contains(&format!(
                r#"<content type="text">Every edition you may read of the works in {name}</content>"#
            )),
            "{entry}"
        );
        assert!(
            entry.contains(&subsection(&format!(
                "/opds/v1.2/libraries/{library}/works/by-work"
            ))),
            "{entry}"
        );
    }
    let at = |library: i64| {
        body.find(&format!("<id>taiyaku:library:{library}:by-work</id>"))
            .expect("listed")
    };
    assert!(at(held) < at(withheld), "held before withheld: {body}");
}

/// The four answers that tell the two views apart, over a work with an
/// edition under each library's directories. By work, each library lists it
/// with both editions; by directory, each lists it with the edition under its
/// own directory. The by-work feed names itself apart from the by-directory one:
/// its id, its title, with the library's name escaped, and its `self` link.
#[tokio::test]
async fn the_by_work_feed_lists_a_work_under_each_library_holding_it_with_every_edition() {
    let tmp = crate::test_support::Fixture::new("opds-by-work-held");
    let (_router, state) = router_with_two_editions_split_across_roots(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);
    let slug = state.works.snapshot()[0].id.clone();
    let app = feed_router().with_state(state);
    let by_work = |library: i64| format!("/opds/v1.2/libraries/{library}/works/by-work");
    let by_dir = |library: i64| format!("/opds/v1.2/libraries/{library}/works");

    let uri = by_work(held);
    let (status, body) = fetch(&app, &uri).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    assert!(
        body.contains(&format!("<id>taiyaku:library:{held}:works:by-work</id>")),
        "{uri}: {body}"
    );
    assert!(
        body.contains(&format!("<title>{HELD_LIBRARY} (by work)</title>")),
        "{uri}: {body}"
    );
    assert!(
        body.contains(&format!(
            r#"<link rel="self"  type="{ATOM};kind=navigation" href="{uri}"/>"#
        )),
        "{uri}: {body}"
    );
    assert_eq!(
        listed_as(&app, &by_work(held), &slug).await.as_deref(),
        Some("2 editions"),
        "a library a directory of which holds an edition lists it with every edition"
    );
    assert_eq!(
        listed_as(&app, &by_dir(held), &slug).await.as_deref(),
        Some("1 editions"),
        "by directory, the same library counts the edition under its directory"
    );
    assert_eq!(
        listed_as(&app, &by_work(withheld), &slug).await.as_deref(),
        Some("2 editions"),
        "the withheld directory holds the other edition, so its library lists the work too"
    );
    assert_eq!(
        listed_as(&app, &by_dir(withheld), &slug).await.as_deref(),
        Some("1 editions"),
        "by directory, the withheld directory's library lists the edition under it"
    );
}

/// Granted the held directory's library, which holds one of the work's
/// editions: its by-work feed counts the one edition the grant allows, and the
/// withheld directory's library is refused as by directory.
#[tokio::test]
async fn a_restricted_reader_s_by_work_feed_counts_only_the_editions_the_grant_allows() {
    let tmp = crate::test_support::Fixture::new("opds-by-work-restricted");
    let (router, state) = router_with_two_editions_split_across_roots(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);
    let slug = state.works.snapshot()[0].id.clone();

    assert_eq!(
        listed_as(
            &router,
            &format!("/opds/v1.2/libraries/{held}/works/by-work"),
            &slug
        )
        .await
        .as_deref(),
        Some("1 editions"),
        "the edition under the withheld directory is outside the grant"
    );
    let (status, body) = fetch(
        &router,
        &format!("/opds/v1.2/libraries/{withheld}/works/by-work"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body, format!("no library {withheld}"));
}

/// An imported work, its members under no directory, is fed by
/// `/opds/v1.2/works` and by no library's by-work feed. The other work is
/// under the held directory, so the held library's feed is not empty for want
/// of anything to list.
#[tokio::test]
async fn an_imported_work_is_in_no_by_work_feed() {
    let tmp = crate::test_support::Fixture::new("opds-by-work-imported");
    let (_router, state) = router_with_two_works_one_withheld(&tmp);
    let held = library_of(&state, 1);
    let withheld = library_of(&state, 2);
    {
        let conn = state.library.conn();
        conn.execute(
            "UPDATE member SET root_id = NULL WHERE edition_id IN \
             (SELECT e.id FROM edition e JOIN work w ON w.id = e.work_id \
               WHERE w.title = 'Hidden Work')",
            [],
        )
        .expect("leaves Hidden Work's members under no directory");
    }
    let app = feed_router().with_state(state);

    let (status, all) = fetch(&app, "/opds/v1.2/works").await;
    assert_eq!(status, StatusCode::OK, "{all}");
    assert!(all.contains("Hidden Work"), "{all}");
    for library in [held, withheld] {
        let uri = format!("/opds/v1.2/libraries/{library}/works/by-work");
        let (status, body) = fetch(&app, &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert!(
            !body.contains("Hidden Work"),
            "{uri} lists a work filed in no library: {body}"
        );
        assert_eq!(
            body.contains("Visible Work"),
            library == held,
            "{uri}: Visible Work is under the held directory: {body}"
        );
    }
}

// --- Spec C2's second half: the OPDS-namespace download. ---

/// The mirror exists for `auth::refuse`'s sake, not for symmetry: a 401 in
/// this namespace carries `WWW-Authenticate` and the `/api` twin's
/// deliberately does not, so an OPDS client following an acquisition link
/// into `/api` cold would get a bare 401 and never prompt for credentials.
/// That is the property this registration is *for*, so it is asserted about
/// this route rather than assumed to follow from registering it.
#[tokio::test]
async fn the_opds_download_401s_with_a_prompt_where_the_api_twin_does_not() {
    let tmp = crate::test_support::Fixture::new("opds-dl-401");
    let state = Arc::new(crate::test_support::state(&tmp));
    let router = crate::build_router(state);

    let opds = router
        .clone()
        .oneshot(
            Request::get("/opds/v1.2/works/t/raw/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(opds.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        opds.headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"taiyaku\""),
        "without this header Panels never prompts, which is the whole reason \
         this registration exists"
    );

    let api = router
        .oneshot(
            Request::get("/api/works/t/raw/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("answers");
    assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
    assert!(
        api.headers().get("www-authenticate").is_none(),
        "an /api 401 carrying WWW-Authenticate raises the browser's own dialog"
    );
}

/// "One handler, two registrations" is a claim, so it is checked rather than
/// hoped for: the same edition through both paths is the same bytes.
#[tokio::test]
async fn the_opds_download_serves_the_same_bytes_as_the_api_route() {
    let tmp = crate::test_support::Fixture::new("opds-dl-same");
    let state = Arc::new(crate::test_support::state(&tmp));
    let router = crate::test_support::key_authenticating(crate::build_router(state));

    let mut bodies = Vec::new();
    for path in [
        "/api/works/t/raw/download",
        "/opds/v1.2/works/t/raw/download",
    ] {
        let response = crate::test_support::get(&router, path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        bodies.push(
            axum::body::to_bytes(response.into_body(), 64 << 20)
                .await
                .expect("body"),
        );
    }

    assert_eq!(
        bodies[0], bodies[1],
        "the OPDS registration must be the same handler, not a second \
         implementation"
    );
}

/// Every route sits under the version the feed claims, and the catalog path
/// `/api/about` serves is one of them. A bump to [`VERSION`] that left the
/// route literals behind would send a client following About's catalog
/// address to a 404.
#[test]
fn every_route_is_under_the_version_the_feed_claims() {
    let prefix = format!("/opds/v{VERSION}/");
    let paths: Vec<&str> = routes().into_iter().map(|(path, _)| path).collect();
    for path in &paths {
        assert!(path.starts_with(&prefix), "{path} is not under {prefix}");
    }
    let catalog = catalog_path();
    assert!(
        paths.contains(&catalog.as_str()),
        "{catalog} is not one of the routes: {paths:?}"
    );
}

/// Spec §3.8's narrowing on the OPDS routes, over
/// [`crate::test_support::router_over_an_edition_merged_from_two_libraries`].
mod narrowing {
    use super::*;
    use crate::test_support::{self, EditionMergedFromTwoLibraries, Reader};

    async fn fetched(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        path: &str,
    ) -> axum::http::Response<Vec<u8>> {
        let uri = format!("/opds/v1.2/works/{}{path}", merged.slug);
        let response = merged.get(reader, &uri, None).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{reader:?}'s {uri}: {}",
            String::from_utf8_lossy(response.body())
        );
        response
    }

    /// `raw`'s `pse:count` in `reader`'s feed of the work.
    async fn raw_count(merged: &EditionMergedFromTwoLibraries, reader: Reader) -> String {
        let feed = fetched(merged, reader, "").await;
        let feed = String::from_utf8_lossy(feed.body());
        let entry = feed
            .split("<entry>")
            .find(|entry| entry.contains(":raw</id>"))
            .unwrap_or_else(|| panic!("no `raw` entry in {reader:?}'s feed: {feed}"));
        entry
            .split(r#"pse:count=""#)
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("no pse:count in {entry}"))
            .to_string()
    }

    /// The centre pixel of a decodable page or cover.
    fn centre(bytes: &[u8]) -> [u8; 3] {
        let image = image::load_from_memory(bytes)
            .expect("a decodable image")
            .to_rgb8();
        image.get_pixel(image.width() / 2, image.height() / 2).0
    }

    /// Within JPEG's drift of each other: the fixture's pages are 40 apart.
    fn near(one: [u8; 3], other: [u8; 3]) -> bool {
        one.iter().zip(other).all(|(a, b)| a.abs_diff(b) <= 12)
    }

    #[tokio::test]
    async fn a_narrowed_editions_pse_count_is_its_own() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        assert_eq!(
            raw_count(&merged, Reader::Administrator).await,
            "9",
            "the fixture: raw is 9 pages whole"
        );
        assert_eq!(raw_count(&merged, Reader::MangaOnly).await, "6");
        assert_eq!(raw_count(&merged, Reader::KidsOnly).await, "3");
    }

    #[tokio::test]
    async fn the_pse_streams_narrowed_page_is_the_full_page_it_translates_to() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let full = fetched(&merged, Reader::Administrator, "/raw/pages/5").await;
        assert!(
            full.body()
                != fetched(&merged, Reader::Administrator, "/raw/pages/2")
                    .await
                    .body(),
            "the fixture: pages 2 and 5 differ"
        );
        let narrowed = fetched(&merged, Reader::MangaOnly, "/raw/pages/2").await;
        assert!(narrowed.body() == full.body(), "Manga's page 2 is page 5");
        assert_eq!(
            narrowed.headers().get(header::ETAG),
            full.headers().get(header::ETAG)
        );
    }

    /// The type `raw`'s acquisition link in `reader`'s feed declares, and
    /// the address it links.
    async fn raw_acquisition(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
    ) -> (String, String) {
        let feed = fetched(merged, reader, "").await;
        let feed = String::from_utf8_lossy(feed.body());
        let link = feed
            .split("<entry>")
            .find(|entry| entry.contains(":raw</id>"))
            .and_then(|entry| {
                entry
                    .split(r#"rel="http://opds-spec.org/acquisition" type=""#)
                    .nth(1)
            })
            .unwrap_or_else(|| panic!("no acquisition link of `raw` in {reader:?}'s feed: {feed}"));
        let declared = link.split('"').next().expect("a type").to_string();
        let href = link
            .split(r#"href=""#)
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("an href")
            .to_string();
        (declared, href)
    }

    /// An account that sees one member of a merged edition, a file that is
    /// not a `.cbz`, is advertised that file's own type and is sent that file:
    /// the feed's type and the download it links describe one thing.
    #[tokio::test]
    async fn a_narrowed_editions_one_member_is_advertised_and_sent_as_itself() {
        let merged =
            test_support::router_over_an_edition_merged_from_two_libraries_with_part2_named(
                "Foo v01 part2.cbr",
            );
        let work = merged.state.find_blocking(&merged.slug).expect("the work");
        let sidecar = work.sidecar();
        let files: Vec<&str> = sidecar.editions["raw"]
            .members
            .iter()
            .map(|member| member.path.rsplit('/').next().expect("a file name"))
            .collect();
        assert_eq!(
            files,
            [
                "Foo v01 part1.cbz",
                "Foo v01 part2.cbr",
                "Foo v01 part3.cbz"
            ],
            "the fixture: Kids' one member of raw is not a .cbz"
        );
        let on_disk = std::fs::read(&sidecar.editions["raw"].members[1].path).expect("part2");

        let (declared, href) = raw_acquisition(&merged, Reader::KidsOnly).await;
        assert_eq!(declared, "application/vnd.comicbook-rar");
        let sent = merged.get(Reader::KidsOnly, &href, None).await;
        assert_eq!(sent.status(), StatusCode::OK, "{href}");
        assert_eq!(
            sent.headers()[header::CONTENT_TYPE].to_str().unwrap(),
            declared,
            "{href} is sent as the feed declared it"
        );
        assert!(sent.body() == &on_disk, "{href} is part2 byte for byte");
    }

    /// Kids' cover of `raw` is part2's first page, and is kept under part2:
    /// the administrator's, asked after it, is still part1's.
    #[tokio::test]
    async fn kids_alones_cover_of_raw_is_part2s_first_page() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let part1 = centre(
            fetched(&merged, Reader::Administrator, "/raw/pages/0")
                .await
                .body(),
        );
        let part2 = centre(
            fetched(&merged, Reader::Administrator, "/raw/pages/2")
                .await
                .body(),
        );
        assert!(
            !near(part1, part2),
            "the fixture: part1's and part2's first pages differ"
        );
        let cover = fetched(&merged, Reader::KidsOnly, "/raw/cover").await;
        assert!(
            near(centre(cover.body()), part2),
            "Kids' cover is part2's first page: {:?}, against {part2:?}",
            centre(cover.body())
        );
        let whole = fetched(&merged, Reader::Administrator, "/raw/cover").await;
        assert!(
            near(centre(whole.body()), part1),
            "the administrator's cover is part1's first page, not one kept under part1 \
             for Kids: {:?}, against {part1:?}",
            centre(whole.body())
        );
    }
}

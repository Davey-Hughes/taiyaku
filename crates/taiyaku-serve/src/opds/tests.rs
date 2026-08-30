use super::*;

/// `routes()` folded into a `Router`, which is what these tests drove before
/// the guard made that function return a list.
///
/// **Deliberately without `auth::guard`**: these tests exercise the feed's own
/// shape — its XML, its links, its page streaming — and the guard is asserted
/// over in `auth::tests::every_route_refuses_an_anonymous_request`, which walks
/// this very list. Building the guarded router here would test the guard
/// twenty-one more times and the feed no better.
fn feed_router() -> axum::Router<std::sync::Arc<ServeState>> {
    super::routes()
        .into_iter()
        .fold(axum::Router::new(), |router, (path, handler)| {
            router.route(path, handler)
        })
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
    let app = feed_router().with_state(state);

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
        library: crate::test_support::library(),
        works: vec![crate::test_support::work_with_edition_key(
            "taiyaku-test-opds-percent",
            "raw color",
        )],
        source: Arc::new(crate::LocalLibrary),
        cache_dir: std::env::temp_dir()
            .join("taiyaku-test-opds-percent")
            .join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
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

    crate::test_support::cleanup("taiyaku-test-opds-author");
    assert!(
        all_works_feed_body.contains("<author><name>taiyaku</name></author>"),
        "the works feed must declare an author: {all_works_feed_body}"
    );
    assert!(
        single_work_feed_body.contains("<author><name>taiyaku</name></author>"),
        "the work feed must declare an author: {single_work_feed_body}"
    );
}

/// Important 5: navigation entry links carried no `rel`, so they
/// defaulted to Atom's `alternate` rather than `subsection` — the `rel`
/// OPDS catalogs use for an entry that leads to another feed.
#[tokio::test]
async fn navigation_entries_carry_a_subsection_rel() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-subsection"));
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

/// OPDS 1.2 requires at least one acquisition link
/// (`rel="http://opds-spec.org/acquisition"`) on every entry in an
/// acquisition feed, or a conformant client may render the entry as
/// unopenable. It now points at a download — proven by fetching it, not by
/// asserting the string is present, so this fails if the link is ever dead.
///
/// *This replaces `the_acquisition_link_on_an_edition_entry_is_a_real_fetchable_page`,
/// which asserted the link is a real fetchable **page**. That claim is what
/// this branch deliberately changes, so the test is rewritten rather than
/// left to contradict the code beside it.*
#[tokio::test]
async fn the_acquisition_link_is_a_real_fetchable_download() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acq-dl"));
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

    // The PSE link is a different mechanism for a different intention and is
    // untouched. Asserted with its `pse:count`, because a test that would
    // still pass with the link deleted is not evidence.
    assert!(
        feed_body.contains(r#"rel="http://vaemendis.net/opds-pse/stream""#)
            && feed_body.contains(r#"pse:count="4""#),
        "streaming stays the in-app read path: {feed_body}"
    );

    crate::test_support::cleanup("taiyaku-test-opds-acq-dl");
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
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acq-type"));
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

    crate::test_support::cleanup("taiyaku-test-opds-acq-type");
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

/// The web UI's control is *absent* for a reader who may not download, and
/// the feed cannot do the same: OPDS 1.2 requires an acquisition link on
/// every entry, which is why this one pointed at a page in the first place.
/// So the page link becomes the fallback rather than being deleted, and the
/// entry carries exactly one acquisition link either way.
///
/// **Both branches are driven off one flipped column, deliberately.** An
/// assertion that a withheld reader sees the page link passes just as well
/// against a feed that has no branch at all — which is what it did before
/// this was written, since the page link was then the only thing emitted.
/// Only the pair distinguishes a working branch from a missing one.
#[tokio::test]
async fn the_acquisition_link_follows_can_download_in_both_directions() {
    async fn feed_for(dir: &str, may_download: bool) -> String {
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
        let router = crate::test_support::authenticating(crate::build_router(state));
        let response = crate::test_support::get(&router, "/opds/v1.2/works/t").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = crate::test_support::body_string(response).await;
        crate::test_support::cleanup(dir);
        body
    }

    let granted = feed_for("taiyaku-test-opds-acq-yes", true).await;
    let withheld = feed_for("taiyaku-test-opds-acq-no", false).await;

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

    // **The fallback is fetched, not merely matched.** The test this replaced
    // (`…_is_a_real_fetchable_page`) issued a real request against the page
    // link, and dropping that left the one branch most in need of
    // "not a dead link" proof with none — which is how `pages/1` survived as
    // the acquisition target while PSE counts from 0, making it the *second*
    // page and a 404 for any one-page edition.
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-acq-fetch"));
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
    let router = crate::test_support::authenticating(crate::build_router(state));
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
    crate::test_support::cleanup("taiyaku-test-opds-acq-fetch");
    assert_eq!(
        status,
        StatusCode::OK,
        "the fallback must be a real, fetchable page, not a dead link: {href}"
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
        library: crate::test_support::library(),
        works: vec![work_with_a_pageless_first_member(
            "taiyaku-test-opds-cover-pageless-first-member",
        )],
        source: Arc::new(crate::LocalLibrary),
        cache_dir: dir.clone(),
        log: None,
        auth: crate::session::AuthConfig::default(),
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
fn work_with_a_pageless_first_member(dir: &str) -> crate::Work {
    use std::io::Write;

    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

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
// The grant gap: `opds.rs` applied none of spec C1's per-root visibility.
// The whole-branch review closed the equivalent hole on the `/api/…` side —
// `refuse_if_hidden` and `get_sidecar`'s use of `browse::work` — and this
// mirror of those routes was never covered. `taiyaku library user
// grant-root` ships grants for real, so a restricted account reading the
// bytes of a root it does not hold through these routes is not theoretical.
//
// Every fixture below imports through `taiyaku_library::import::scanned`
// into a real library database rather than handing `ServeState` an
// in-memory-only `Work` — `edition_visible`/`browse::work` read `work`,
// `edition` and `member` rows, and a work absent from those tables cannot
// exercise the grant at all, only the "work does not exist" path every
// existing test in this file already covers. And the page/cover fixture
// writes a real, decodable image rather than reusing `test_support::work`'s
// placeholder bytes: those routes read *bytes*, not rows, so a 404 from a
// nonexistent placeholder path would be indistinguishable from the grant
// actually refusing anything.
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
    dir: &str,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    use std::io::Write as _;

    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

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
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = crate::test_support::authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// A router over one work with **two** editions, each filed under a
/// different root — `en` under the root the fixture account holds, `raw`
/// under the one it does not. What `work_feed`'s partial-visibility test
/// needs: a work that is itself visible, with exactly one of its editions
/// outside the grant.
fn router_with_two_editions_split_across_roots(
    dir: &str,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
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
        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
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
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = crate::test_support::authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// A router over **two** works, one filed under the root the fixture
/// account holds and one under a root it does not. What the `works` list
/// feed's test needs: a work it must keep naming beside one it must not.
fn router_with_two_works_one_withheld(dir: &str) -> (axum::Router, std::sync::Arc<ServeState>) {
    let root = std::env::temp_dir().join(dir);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
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

        conn.execute(
            "INSERT INTO root (id, path) VALUES (1, '/held'), (2, '/withheld')",
            [],
        )
        .expect("two roots");
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
        works,
        source: Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        log: None,
        auth: crate::session::AuthConfig::default(),
    });
    let router = crate::test_support::authenticating(crate::build_router(state.clone()));
    (router, state)
}

/// **The core of the gap.** Before the fix, `stream_page` and `cover` asked
/// nothing about the grant, so a reader restricted to one root could read
/// the bytes of a work filed under a different root through the OPDS
/// routes alone — exactly the failure spec C1's whole-branch review closed
/// on `/api/…`, unfixed here.
///
/// Proven with real, decodable page bytes rather than a coincidental
/// filesystem 404: `router_with_a_real_page_under_a_withheld_root` writes an
/// actual PNG the un-guarded routes can and — before the fix — do serve, so
/// a passing assertion here cannot be satisfied by the member file simply
/// not existing.
///
/// **Bodies are decoded with `from_utf8_lossy`, not `test_support::body_string`.**
/// Before the fix this route answers `200` with the PNG's raw bytes, which
/// are not valid UTF-8 — `body_string`'s `unwrap` turns that into a panic
/// naming the exact byte sequence served, which is itself the bypass this
/// test exists to catch, but a panic is a worse failure message than an
/// assertion. Lossy decoding keeps the proof as a normal, readable
/// assertion failure instead.
#[tokio::test]
async fn a_restricted_reader_cannot_stream_pages_or_covers_from_a_root_they_do_not_hold() {
    let (router, state) =
        router_with_a_real_page_under_a_withheld_root("taiyaku-test-opds-grant-stream");
    let slug = state.works[0].id.clone();

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

    crate::test_support::cleanup("taiyaku-test-opds-grant-stream");
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
    let (router, state) =
        router_with_two_editions_split_across_roots("taiyaku-test-opds-grant-work-feed");
    let slug = state.works[0].id.clone();

    let response = crate::test_support::get(&router, &format!("/opds/v1.2/works/{slug}")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    crate::test_support::cleanup("taiyaku-test-opds-grant-work-feed");
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

/// Finding 2: the list feed named each work with a count of its editions —
/// `sidecar.editions.len()`, computed over every edition regardless of the
/// caller's grant — so a restricted reader learned how many editions a work
/// held under a root they cannot see, even though the work's own feed just
/// above already hides every one of those editions by name. A count is a
/// smaller leak than a key or a path, and it is still one.
#[tokio::test]
async fn a_restricted_readers_works_list_counts_only_the_editions_they_may_see() {
    let (router, state) =
        router_with_two_editions_split_across_roots("taiyaku-test-opds-grant-works-count");
    let slug = state.works[0].id.clone();

    let response = crate::test_support::get(&router, "/opds/v1.2/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    crate::test_support::cleanup("taiyaku-test-opds-grant-works-count");
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
/// `browse::work`'s own contract, unread by this route before the fix.
#[tokio::test]
async fn a_restricted_readers_work_feed_404s_when_no_edition_is_visible() {
    let (router, state) =
        router_with_a_real_page_under_a_withheld_root("taiyaku-test-opds-grant-work-feed-hidden");
    let slug = state.works[0].id.clone();

    let response = crate::test_support::get(&router, &format!("/opds/v1.2/works/{slug}")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    crate::test_support::cleanup("taiyaku-test-opds-grant-work-feed-hidden");
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a wholly hidden work must not be served: {body}"
    );
    assert_eq!(body, format!("no work `{slug}`"));
}

/// The list feed's own leak: before the fix, `works` named every work this
/// server holds regardless of the caller's grant, admitting the existence
/// of material a restricted reader cannot see anywhere else.
#[tokio::test]
async fn the_works_feed_does_not_name_a_work_outside_the_grant() {
    let (router, _state) = router_with_two_works_one_withheld("taiyaku-test-opds-grant-works-list");

    let response = crate::test_support::get(&router, "/opds/v1.2/works").await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;

    crate::test_support::cleanup("taiyaku-test-opds-grant-works-list");
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

// --- Spec C2's second half: the OPDS-namespace download. ---

/// The mirror exists for `auth::refuse`'s sake, not for symmetry: a 401 in
/// this namespace carries `WWW-Authenticate` and the `/api` twin's
/// deliberately does not, so an OPDS client following an acquisition link
/// into `/api` cold would get a bare 401 and never prompt for credentials.
/// That is the property this registration is *for*, so it is asserted about
/// this route rather than assumed to follow from registering it.
#[tokio::test]
async fn the_opds_download_401s_with_a_prompt_where_the_api_twin_does_not() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-dl-401"));
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

    crate::test_support::cleanup("taiyaku-test-opds-dl-401");
}

/// "One handler, two registrations" is a claim, so it is checked rather than
/// hoped for: the same edition through both paths is the same bytes.
#[tokio::test]
async fn the_opds_download_serves_the_same_bytes_as_the_api_route() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-opds-dl-same"));
    let router = crate::test_support::authenticating(crate::build_router(state));

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

    crate::test_support::cleanup("taiyaku-test-opds-dl-same");
    assert_eq!(
        bodies[0], bodies[1],
        "the OPDS registration must be the same handler, not a second \
         implementation"
    );
}

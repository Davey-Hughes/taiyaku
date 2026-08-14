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

use super::{content_disposition, download_name, safe, volume_text};

#[test]
fn a_fraction_is_rounded_not_printed_as_a_raw_remainder() {
    assert_eq!(volume_text(1.0), "01");
    assert_eq!(volume_text(1.5), "01.5");
    assert_eq!(volume_text(2.3), "02.3");
    assert_eq!(volume_text(12.0), "12");
    assert_eq!(volume_text(105.0), "105");
    assert_eq!(volume_text(0.0), "00");
    assert_eq!(volume_text(2.25), "02.25");
}

#[test]
fn a_series_that_sanitises_away_falls_through_to_the_title() {
    assert_eq!(
        download_name(Some("..."), Some(1.0), "Real Title", "raw", "cbz"),
        "Real Title - v01 (raw).cbz"
    );
}

#[test]
fn both_series_and_title_sanitising_away_falls_back_to_a_constant() {
    assert_eq!(
        download_name(Some("..."), Some(1.0), ".", "raw", "cbz"),
        "download - v01 (raw).cbz"
    );
}

#[test]
fn no_stem_ever_starts_with_a_space_or_a_separator() {
    for (series, title) in [
        (Some("..."), "t"),
        (Some("   "), "t"),
        (None, "..."),
        (Some(""), ""),
    ] {
        let name = download_name(series, Some(1.0), title, "raw", "cbz");
        assert!(!name.starts_with([' ', '/', '\\', ':', '.']), "got {name}");
    }
}

#[test]
fn interleaved_dots_and_spaces_cannot_survive_repeated_stripping() {
    assert_eq!(safe(". .hidden"), "hidden");
    assert_eq!(safe(". . .x"), "x");
    assert_eq!(safe(" ..x"), "x");
    assert_eq!(safe(".. . x"), "x");
}

#[test]
fn a_full_name_reads_the_way_a_downloads_folder_wants() {
    assert_eq!(
        download_name(
            Some("Go-Toubun no Hanayome"),
            Some(1.0),
            "ignored",
            "raw",
            "cbz"
        ),
        "Go-Toubun no Hanayome - v01 (raw).cbz"
    );
}

#[test]
fn a_half_volume_keeps_its_fraction_and_a_big_one_its_digits() {
    assert_eq!(
        download_name(Some("S"), Some(1.5), "t", "raw", "cbz"),
        "S - v01.5 (raw).cbz"
    );
    assert_eq!(
        download_name(Some("S"), Some(12.0), "t", "raw", "cbz"),
        "S - v12 (raw).cbz"
    );
    assert_eq!(
        download_name(Some("S"), Some(105.0), "t", "raw", "cbz"),
        "S - v105 (raw).cbz"
    );
}

#[test]
fn no_series_falls_back_to_the_work_title() {
    // A hand-imported work has no series: `import` never writes `series_id`
    // and cannot, since `taiyaku.json` has no field for it.
    assert_eq!(
        download_name(None, Some(1.0), "Some Hand Import", "raw", "cbz"),
        "Some Hand Import - v01 (raw).cbz"
    );
}

#[test]
fn no_volume_drops_the_segment_rather_than_writing_v0() {
    assert_eq!(
        download_name(Some("S"), None, "t", "raw", "cbz"),
        "S (raw).cbz"
    );
}

#[test]
fn separators_and_controls_cannot_reach_the_filename() {
    // A title that arrived from a filename can hold anything a filename can.
    let name = download_name(Some("a/b\\c"), Some(1.0), "t", "d\u{7}e", "cbz");
    assert!(!name.contains('/'), "got {name}");
    assert!(!name.contains('\\'), "got {name}");
    assert!(!name.contains('\u{7}'), "got {name}");
    assert_eq!(name, "a-b-c - v01 (d-e).cbz");
}

#[test]
fn a_leading_dot_cannot_make_it_a_hidden_file() {
    assert_eq!(
        download_name(Some(".hidden"), None, "t", "raw", "cbz"),
        "hidden (raw).cbz"
    );
}

#[test]
fn the_extension_follows_the_bytes_not_the_route() {
    // A single-member edition may not be a comic archive at all: `member.format`
    // admits `pdf` and the two epub kinds. Spec §7.
    assert_eq!(
        download_name(Some("S"), Some(1.0), "t", "raw", "pdf"),
        "S - v01 (raw).pdf"
    );
}

#[test]
fn a_non_ascii_name_travels_in_both_forms() {
    let header = content_disposition("五等分の花嫁 - v01 (raw).cbz");
    // The ASCII form a client that understands only RFC 2183 will read.
    assert!(
        header.starts_with("attachment; filename=\""),
        "got {header}"
    );
    assert!(
        header.contains("filename*=UTF-8''"),
        "a client that understands RFC 6266 must get the real name: {header}"
    );
    // Percent-encoded, and the parts that are already safe left alone.
    assert!(header.contains("-%20v01%20%28raw%29.cbz"), "got {header}");
    // The quoted fallback must not contain a raw quote or backslash.
    let fallback = header
        .split_once("filename=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name.to_string())
        .expect("a quoted fallback");
    assert!(!fallback.contains('"'));
    assert!(!fallback.contains('\\'));
    assert!(
        fallback.is_ascii(),
        "the fallback must be ASCII: {fallback}"
    );
}

#[test]
fn a_quote_in_a_title_cannot_break_out_of_the_header() {
    let header = content_disposition("a\"b.cbz");
    assert!(!header.contains("\"a\"b"), "got {header}");
}

#[test]
fn rounding_the_whole_value_once_keeps_a_carry_in_the_right_half() {
    // A fraction that rounds up to `1.000` carries into the whole part.
    assert_eq!(volume_text(1.0), "01");
    assert_eq!(volume_text(1.5), "01.5");
    assert_eq!(volume_text(2.3), "02.3");
    assert_eq!(volume_text(2.25), "02.25");
    assert_eq!(volume_text(12.0), "12");
    assert_eq!(volume_text(105.0), "105");
    assert_eq!(volume_text(0.0), "00");
    assert_eq!(volume_text(1.9999), "02");
    assert_eq!(volume_text(49.99999999999999), "50");
    assert_eq!(volume_text(999.9995), "1000");
    assert_eq!(volume_text(2.9995), "03");
}

#[test]
fn negative_zero_does_not_claim_a_negative_volume() {
    assert_eq!(volume_text(-0.0), "00");
}

#[test]
fn a_non_finite_volume_drops_the_segment_like_no_volume_at_all() {
    // `f64::INFINITY` can round-trip through a SQLite REAL column; whatever
    // this does, it must not print a filename that claims volume zero.
    assert_eq!(
        download_name(Some("S"), Some(f64::INFINITY), "t", "raw", "cbz"),
        download_name(Some("S"), None, "t", "raw", "cbz"),
    );
    assert_eq!(
        download_name(Some("S"), Some(f64::NEG_INFINITY), "t", "raw", "cbz"),
        download_name(Some("S"), None, "t", "raw", "cbz"),
    );
    assert_eq!(
        download_name(Some("S"), Some(f64::NAN), "t", "raw", "cbz"),
        download_name(Some("S"), None, "t", "raw", "cbz"),
    );
}

#[test]
fn a_stem_that_sanitises_to_pure_punctuation_is_not_usable() {
    // `/` sanitises to `-`, which is non-empty, but the stem must still fall
    // through to the title beside it.
    assert_eq!(
        download_name(Some("/"), Some(1.0), "Real Title", "raw", "cbz"),
        "Real Title - v01 (raw).cbz"
    );
}

#[test]
fn both_series_and_title_pure_separators_falls_back_to_the_constant() {
    assert_eq!(
        download_name(Some("---"), Some(1.0), ". - .", "raw", "cbz"),
        "download - v01 (raw).cbz"
    );
}

#[test]
fn a_stem_that_begins_with_a_dash_after_sanitising_is_stripped() {
    // Has a usable character (`R`), so it passes the alphanumeric filter —
    // but the dash is still first, and a leading `-` is read as a flag by
    // ordinary shell tools.
    assert_eq!(
        download_name(Some("- Real Title"), Some(1.0), "t", "raw", "cbz"),
        "Real Title - v01 (raw).cbz"
    );
}

#[test]
fn a_stem_that_opens_with_a_bracket_keeps_it() {
    for (series, stem) in [
        ("[Oshi no Ko]", "[Oshi no Ko]"),
        ("(C99) Title", "(C99) Title"),
        ("【推しの子】", "【推しの子】"),
        ("'Tis", "'Tis"),
        ("- [Group] Title", "[Group] Title"),
    ] {
        assert_eq!(
            download_name(Some(series), Some(1.0), "t", "raw", "cbz"),
            format!("{stem} - v01 (raw).cbz"),
        );
    }
    // The title takes the same road when there is no series.
    assert_eq!(
        download_name(None, None, "[Oshi no Ko]", "raw", "cbz"),
        "[Oshi no Ko] (raw).cbz"
    );
}

#[test]
fn a_dash_comes_off_whatever_separator_it_is_mixed_with() {
    // Each of these fails a different half of the strip: U+3000 is whitespace
    // only to `is_whitespace`, and the dot sits behind a dash, where `safe`'s
    // own leading-dot strip cannot see it.
    for (series, stem) in [
        ("-\u{3000}Title", "Title"),
        ("-.hidden", "hidden"),
        ("- . -[x]", "[x]"),
    ] {
        assert_eq!(
            download_name(Some(series), Some(1.0), "t", "raw", "cbz"),
            format!("{stem} - v01 (raw).cbz"),
        );
    }
}

#[test]
fn a_very_large_volume_does_not_collapse_to_the_same_wrong_number() {
    // `1e300` and `f64::MAX` must come out different.
    assert_ne!(volume_text(1e300), volume_text(f64::MAX));
    // Below 2^52 every f64 can still hold a fraction; at and above it every
    // representable value is already a whole number, so the exact value
    // (not a saturated one) should come through untouched.
    assert_eq!(volume_text(1e16), "10000000000000000");
    assert_eq!(
        volume_text(4_503_599_627_370_496.0), // 2^52 exactly
        "4503599627370496"
    );
}

#[test]
fn no_download_name_output_can_start_with_dash_dot_or_space() {
    let cases: &[(Option<&str>, &str)] = &[
        (Some("/"), "Real Title"),
        (Some("---"), ". - ."),
        (Some("- - -"), "t"),
        (Some(". . ."), "  --  "),
        (None, "-.- x"),
    ];
    for (series, title) in cases {
        let name = download_name(*series, Some(1.0), title, "raw", "cbz");
        assert!(
            !name.starts_with(['-', '.', ' ', '\t']),
            "got {name:?} from series {series:?} title {title:?}"
        );
    }
}

// --- The edition download: both body shapes, streamed. ---

/// Read a whole response body, however large the test made it.
async fn all_of(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), 64 << 20)
        .await
        .expect("the body")
        .to_vec()
}

/// Read a response body to its end **or** to the point it errors, whichever
/// comes first — `(bytes collected, whether it ended in an error)`.
///
/// `all_of`'s `to_bytes` cannot be used for this: it aggregates through
/// `http-body-util`'s `collect`, which returns the stream's `Err` as its own
/// `Err`, so `.expect("the body")` panics on a mid-stream failure.
/// `Content-Length` is sent before the first byte (spec §5), so a producer that
/// discovers a problem after headers have gone out has only one way left to say
/// so: end the stream short with an error rather than the promised length. This
/// is that stream, drained by hand with `futures_core::Stream` — already a
/// direct dependency for `download::ChannelStream` — rather than pulling in
/// `http-body-util` or `futures-util` as new ones just to call `.collect()`.
async fn drain(response: axum::http::Response<axum::body::Body>) -> (Vec<u8>, bool) {
    use futures_core::Stream as _;
    let mut stream = std::pin::pin!(response.into_body().into_data_stream());
    let mut collected = Vec::new();
    let mut errored = false;
    loop {
        match std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
            Some(Ok(bytes)) => collected.extend_from_slice(&bytes),
            Some(Err(_)) => {
                errored = true;
                break;
            }
            None => break,
        }
    }
    (collected, errored)
}

#[tokio::test]
async fn a_two_member_edition_downloads_as_one_readable_cbz() {
    let tmp = crate::test_support::Fixture::new("download-two-members");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(response.status(), 200);

    let length: u64 = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .expect("an exact length before the first byte")
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::ACCEPT_RANGES)
            .unwrap(),
        "bytes"
    );
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap(),
        "application/vnd.comicbook+zip"
    );

    let bytes = all_of(response).await;
    assert_eq!(bytes.len() as u64, length, "Content-Length was a lie");

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
    // The fixture is two members of two pages each, plus the generated
    // metadata document.
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    assert_eq!(
        names,
        vec![
            "ComicInfo.xml",
            "0001.jpg",
            "0002.jpg",
            "0003.jpg",
            "0004.jpg"
        ],
        "zero-padded and sequential across the whole edition, in reading order"
    );
    // Each fixture entry's bytes are its own original name, so this asserts
    // the *order* as well as the content.
    let mut first = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("0001.jpg").unwrap(), &mut first).unwrap();
    assert_eq!(first, b"a1.jpg");
    let mut third = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("0003.jpg").unwrap(), &mut third).unwrap();
    assert_eq!(third, b"b1.jpg", "member two's first page is page three");
}

#[tokio::test]
async fn a_single_member_edition_is_served_byte_for_byte() {
    let (work, fixture) = crate::test_support::work_with_real_pages();
    let on_disk = std::fs::read(fixture.path().join("a1.cbz")).unwrap();
    let state = std::sync::Arc::new(crate::ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(vec![work]),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: fixture.path().join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/w/a/download").await;
    assert_eq!(response.status(), 200);
    let bytes = all_of(response).await;

    assert_eq!(bytes, on_disk, "passthrough must not rebuild");
    // And must not acquire metadata it did not have. This assertion is what
    // stops someone "improving" passthrough into a rebuild.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert!(archive.by_name("ComicInfo.xml").is_err());
}

#[tokio::test]
async fn a_range_of_a_generated_download_is_that_slice_of_the_whole() {
    let tmp = crate::test_support::Fixture::new("download-range");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let whole = all_of(crate::test_support::get(&router, "/api/works/t/raw/download").await).await;

    use tower::ServiceExt as _;
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::get("/api/works/t/raw/download")
                .header(axum::http::header::RANGE, "bytes=10-49")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 206);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_RANGE)
            .unwrap(),
        &format!("bytes 10-49/{}", whole.len())
    );
    let part = all_of(response).await;
    assert_eq!(
        part,
        whole[10..50],
        "the range is not that slice of the file"
    );
}

#[tokio::test]
async fn an_unsatisfiable_range_is_refused_with_the_real_length() {
    let tmp = crate::test_support::Fixture::new("download-bad-range");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    use tower::ServiceExt as _;
    let response = router
        .oneshot(
            axum::http::Request::get("/api/works/t/raw/download")
                .header(axum::http::header::RANGE, "bytes=99999999-")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 416);
    let stated = response
        .headers()
        .get(axum::http::header::CONTENT_RANGE)
        .expect("416 must say how long the thing actually is")
        .to_str()
        .unwrap()
        .to_string();
    assert!(stated.starts_with("bytes */"), "got {stated}");
}

#[tokio::test]
async fn the_filename_comes_from_the_library_rows_when_it_has_them() {
    let (router, _state, fixture) = crate::test_support::router_over_a_scanned_work_in_a_series();

    // `router_over_a_scanned_work` deliberately leaves the fixture directory
    // empty — it exists to prove a *correction* mints no file, and every
    // route that reaches it (`patch_alignments`) never opens a member. This
    // route does: `prepare` has to stat and read the archives its sidecar
    // names, so unlike every other consumer of this fixture, a download test
    // over it needs real bytes at the paths the sidecar declares.
    for (dir, names) in [("raw", ["c1.cbz", "c2.cbz"]), ("en", ["d1.cbz", "d2.cbz"])] {
        let sub = fixture.path().join(dir);
        std::fs::create_dir_all(&sub).unwrap();
        for name in names {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(sub.join(name)).unwrap());
            for page in ["p1.jpg", "p2.jpg"] {
                zip.start_file(page, zip::write::SimpleFileOptions::default())
                    .unwrap();
                std::io::Write::write_all(&mut zip, page.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
    }

    // The fixture's slug and edition key come from the helper; find them the
    // way a client would, through the browse route.
    let listed = crate::test_support::body_string(
        crate::test_support::get(&router, "/api/library/series/1").await,
    )
    .await;
    let value: serde_json::Value = serde_json::from_str(&listed).unwrap();
    let slug = value["works"][0]["slug"].as_str().unwrap().to_string();
    let key = value["works"][0]["editions"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let response =
        crate::test_support::get(&router, &format!("/api/works/{slug}/{key}/download")).await;
    assert_eq!(response.status(), 200);
    let disposition = response
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .expect("a download names itself")
        .to_str()
        .unwrap()
        .to_string();
    assert!(disposition.contains("Scanned - v01"), "got {disposition}");
    assert!(disposition.contains(".cbz"), "got {disposition}");
}

#[tokio::test]
async fn an_edition_that_does_not_exist_is_a_404_before_any_bytes() {
    let tmp = crate::test_support::Fixture::new("download-no-edition");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/nonesuch/download").await;
    assert_eq!(response.status(), 404);
    // A failure a stream cannot report must happen before the stream starts —
    // spec §5.
    assert!(
        response
            .headers()
            .get(axum::http::header::CONTENT_LENGTH)
            .is_some()
    );
}

/// A withheld `can_download` refuses the download and leaves the shelf alone.
///
/// 403 rather than 404 deliberately — the caller may *see* this edition and
/// may not have a copy of it, which a 404 would misdescribe. The root grant
/// above it is the one that answers 404, because an edition outside your grant
/// does not admit it exists.
#[tokio::test]
async fn a_withheld_download_capability_refuses_with_403() {
    let tmp = crate::test_support::Fixture::new("download-withheld");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    {
        let conn = state.library.conn();
        // Demoted first — see `withhold` below for why an admin cannot have a
        // capability withheld (O86).
        conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("a capability is withheld from a user, never from an admin");
        conn.execute("UPDATE user SET can_download = 0", [])
            .expect("withholds the capability");
    }
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(
        response.status(),
        403,
        "a withheld can_download must refuse, not serve"
    );
}

// --- The member download: one file, its own name, its own bytes. ---

#[tokio::test]
async fn a_member_downloads_as_the_file_on_disk() {
    let tmp = crate::test_support::Fixture::new("download-member");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let root = tmp.path();
    let second = std::fs::read(root.join("c2.cbz")).unwrap();
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    // Ordinal 1 is the second member — the document's order, counting from 0.
    let response = crate::test_support::get(&router, "/api/works/t/raw/members/1/download").await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap(),
        "application/vnd.comicbook+zip"
    );
    let disposition = response
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.contains("c2.cbz"),
        "a member keeps its own name: {disposition}"
    );
    assert_eq!(all_of(response).await, second);
}

#[tokio::test]
async fn a_member_ordinal_past_the_end_is_a_404() {
    let tmp = crate::test_support::Fixture::new("download-member-past");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/members/9/download").await;
    assert_eq!(response.status(), 404);
    let body = crate::test_support::body_string(response).await;
    assert!(body.contains("2 members"), "say how many there are: {body}");
}

#[tokio::test]
async fn a_member_range_is_that_slice_of_the_file() {
    let tmp = crate::test_support::Fixture::new("download-member-range");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let root = tmp.path();
    let first = std::fs::read(root.join("c1.cbz")).unwrap();
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    use tower::ServiceExt as _;
    let response = router
        .oneshot(
            axum::http::Request::get("/api/works/t/raw/members/0/download")
                .header(axum::http::header::RANGE, "bytes=5-14")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 206);
    // The member's own length, not the two-member edition's combined length —
    // `prepare_member` stats `path` itself rather than reusing whatever
    // `prepare` computed for the whole edition.
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_RANGE)
            .unwrap(),
        &format!("bytes 5-14/{}", first.len())
    );
    assert_eq!(all_of(response).await, first[5..15]);
}

/// A member that is a directory of loose pages, not an archive, is refused
/// with its actionable message rather than streamed as though it were a file.
///
/// `prepare_member`'s `path.is_dir()` check exists for exactly the shape
/// `taiyaku_read` supports beside a zip: an edition scanned from a directory of
/// image files rather than packed into one.
#[tokio::test]
async fn a_directory_member_is_refused_not_streamed() {
    let root = crate::test_support::Fixture::new("download-member-directory");
    std::fs::create_dir_all(root.join("pages")).unwrap();
    std::fs::write(root.join("pages").join("0001.jpg"), b"page one").unwrap();

    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "pages", "sha256": "x", "pages": 1 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
    let path = root.join("taiyaku.json");
    std::fs::write(&path, json).unwrap();
    let work = crate::Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    );

    let state = std::sync::Arc::new(crate::ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(vec![work]),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/members/0/download").await;
    assert_eq!(response.status(), 404);
    let body = crate::test_support::body_string(response).await;
    assert!(
        body.contains("is a directory of pages"),
        "must refuse with an actionable message, not stream it: {body}"
    );
}

/// [`a_withheld_download_capability_refuses_with_403`]'s edition-route proof,
/// on the member route: `download::member` calls `refuse_if_cannot_download`
/// separately from `download::edition`, and this pins that second call site.
#[tokio::test]
async fn a_withheld_download_capability_refuses_a_member_download_too() {
    let tmp = crate::test_support::Fixture::new("download-member-withheld");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    withhold(&state, "can_download");
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/members/0/download").await;
    assert_eq!(
        response.status(),
        403,
        "a withheld can_download must refuse a member download too, not serve"
    );
}

// --- `can_download` and `can_stream`, and the line between them. ---

/// Turn one capability off on the fixture account.
///
/// Driven over `test_support::state`, not `router_over_a_library`: that
/// fixture's own doc comment says its member has no bytes on disk ("nothing
/// here for a real file to buy"), which is fine for the routes it was built
/// for but means `/download` 404s before any guard runs — not the 200 the
/// second test below needs to prove `can_stream` really leaves it alone.
/// `state`'s fixture work `t`/`raw` (`test_support::work`) writes real `.cbz`
/// members to disk, which is what a passthrough download needs to succeed.
/// **The account is demoted first — O86.** `Principal::from_account` reads
/// `can_download` and `can_stream` as unconditionally true for an
/// administrator, on the same rule that drops an admin's root grant, so
/// withholding a capability from the seeded admin withholds nothing and the
/// test would pass against a guard that had been deleted.
fn withhold(state: &crate::ServeState, column: &str) {
    let conn = state.library.conn();
    conn.execute("UPDATE user SET role = 'user' WHERE id = 1", [])
        .expect("a capability is withheld from a user, never from an admin");
    conn.execute(&format!("UPDATE user SET {column} = 0"), [])
        .expect("withholds the capability");
}

#[tokio::test]
async fn without_can_download_the_download_is_403_and_the_shelf_still_works() {
    let tmp = crate::test_support::Fixture::new("download-no-can-download");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    withhold(&state, "can_download");
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    // 403, not 404: the caller may see this edition and may not have it. A 404
    // here would be a lie that also breaks the UI's ability to explain itself.
    for path in [
        "/api/works/t/raw/download",
        "/api/works/t/raw/members/0/download",
    ] {
        let response = crate::test_support::get(&router, path).await;
        assert_eq!(response.status(), 403, "{path}");
    }

    // Everything a download-only refusal must not take away.
    for path in ["/api/library/series", "/api/works/t/raw/pages/0/thumb"] {
        let response = crate::test_support::get(&router, path).await;
        assert_ne!(response.status(), 403, "{path} is how the shelf renders");
    }
}

#[tokio::test]
async fn without_can_stream_the_pages_are_403_and_the_download_still_works() {
    let tmp = crate::test_support::Fixture::new("download-no-can-stream");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    withhold(&state, "can_stream");
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    for path in ["/api/works/t/raw/pages/0", "/opds/v1.2/works/t/raw/pages/0"] {
        let response = crate::test_support::get(&router, path).await;
        assert_eq!(response.status(), 403, "{path}");
    }

    // The coherent half of the policy: take the volume away, not the shelf.
    let response = crate::test_support::get(&router, "/api/works/t/raw/pages/0/thumb").await;
    assert_ne!(
        response.status(),
        403,
        "a thumbnail is how the shelf renders"
    );
    let response = crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(
        response.status(),
        200,
        "can_stream must not gate a download"
    );
    // Drained rather than dropped: the body streams off a detached thread
    // (`download::edition`'s `produce`), and dropping the response here would
    // race that thread against the fixture's `Drop` at the end of the test,
    // which deletes the `.cbz` files out from under it.
    all_of(response).await;
}

/// A member that lost pages between `prepare` reading its central directory and
/// the stream reading it back — replaced by a byte-identical archive with fewer
/// pages, say — must make `generated` fail mid-stream, not finish with `Ok(())`
/// and a trailer whose central-directory offset points into the payload region
/// instead of past it.
///
/// Exercises `generated` directly rather than through the route: the plan is
/// built from a real two-page archive the way `prepare` would build it, then
/// the archive on disk is replaced with a byte-identical one-page archive
/// before the walk runs, so the second page is missing with nothing else
/// about the file lying.
#[test]
fn generated_detects_a_member_that_lost_pages_before_the_trailer() {
    let scratch = crate::test_support::Fixture::new("download-lost-page");
    let path = scratch.join("lost-page.cbz");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (name, bytes) in [
            ("0001.jpg", &b"page one"[..]),
            ("0002.jpg", &b"page two"[..]),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut zip, bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    let described = taiyaku_read::member_page_entries(&path).unwrap();
    assert_eq!(described.len(), 2, "fixture must plan two pages");

    let comic_info = b"<ComicInfo/>".to_vec();
    let mut planned = vec![crate::cbz::PlannedEntry {
        name: "ComicInfo.xml".to_string(),
        size: comic_info.len() as u64,
        crc32: crc32fast::hash(&comic_info),
    }];
    for (index, entry) in described.iter().enumerate() {
        planned.push(crate::cbz::PlannedEntry {
            name: format!("{:04}.jpg", index + 1),
            size: entry.size,
            crc32: entry.crc32,
        });
    }
    let plan = crate::cbz::Plan::of(planned);

    // Replaced with a byte-identical archive missing the second page — the
    // plan above still names both.
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("0001.jpg", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"page one").unwrap();
        zip.finish().unwrap();
    }

    let rt = tokio::runtime::Runtime::new().expect("a runtime");
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let result = super::generated(
        &plan,
        std::slice::from_ref(&path),
        &comic_info,
        0,
        plan.len(),
        &tx,
        rt.handle(),
    );
    drop(tx);
    let _ = std::fs::remove_file(&path);

    assert!(
        result.is_err(),
        "a member that lost pages between planning and streaming must fail \
         mid-stream, not finish Ok(()) with a trailer whose offsets lie"
    );

    let mut sent = 0u64;
    while let Ok(Ok(bytes)) = rx.try_recv() {
        sent += bytes.len() as u64;
    }
    assert!(
        sent < plan.len(),
        "must not have produced the full promised length: sent {sent}, \
         promised {}",
        plan.len()
    );
}

/// Build a `.cbz` whose central-directory declared uncompressed-size field
/// for its first entry is `declared`, a lie — patched after writing, the
/// technique `taiyaku_read::pages::tests::lying_size_cbz` uses (duplicated
/// here rather than exposed across the crate boundary: that helper is private
/// to its own test module). `member_page_entries` — what `prepare` plans a
/// download around — reads this declared field and believes it;
/// `for_each_member_page` reads the real bytes back, and fails once they run
/// past the declared field.
fn lying_size_cbz(path: &std::path::Path, entries: &[(&str, &[u8])], declared: u32) {
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut zip, bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    let mut raw = std::fs::read(path).unwrap();
    let central_directory = raw
        .windows(4)
        .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
        .expect("a central directory file header");
    let size_field = central_directory + 24;
    raw[size_field..size_field + 4].copy_from_slice(&declared.to_le_bytes());
    std::fs::write(path, &raw).unwrap();
}

/// A two-member edition whose first member's one page declares `declared`
/// bytes, downloaded through the route: what `Content-Length` promised, the
/// body, and whether the stream ended in an error.
async fn download_lying_edition(
    root: &crate::test_support::Fixture,
    declared: u32,
) -> (u64, Vec<u8>, bool) {
    lying_size_cbz(&root.join("c1.cbz"), &[("p1.jpg", b"page one")], declared);
    // A second, honest member — so the edition has two members and takes the
    // generated path, which is the only path that ever consults the plan.
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join("c2.cbz")).unwrap());
        zip.start_file("p2.jpg", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"page two").unwrap();
        zip.finish().unwrap();
    }

    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 1 },
              { "path": "c2.cbz", "sha256": "y", "pages": 1 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
    let path = root.join("taiyaku.json");
    std::fs::write(&path, json).unwrap();
    let work = crate::Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    );
    let state = std::sync::Arc::new(crate::ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(vec![work]),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(
        response.status(),
        200,
        "the refusal happens mid-stream, after Content-Length is already sent"
    );
    let promised: u64 = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let (bytes, errored) = drain(response).await;
    (promised, bytes, errored)
}

/// The plan-vs-stream guard at the heart of `generated` —
/// `planned.size != entry.size || planned.crc32 != entry.crc32`. The
/// **producer** half of the asymmetry it relies on is pinned in `taiyaku-read`
/// (`for_each_member_page_reports_the_real_size_while_member_page_entries_keeps_the_declared_lie`);
/// this is the **consumer** half, proven at the route with the same
/// technique that helper uses: an archive whose central directory declares
/// one size while its real, readable bytes are a different, smaller one.
#[tokio::test]
async fn a_size_lying_archive_aborts_the_download_instead_of_streaming_a_lie() {
    let root = crate::test_support::Fixture::new("download-lying-size");
    let (promised, bytes, errored) = download_lying_edition(&root, 0x7FFF_FFF0).await;

    // Both members hold exactly one page each, so the page *count* the plan
    // named and the count the stream produced agree — the page-count guard,
    // after the page loop in `generated`, has nothing to catch here. Only the
    // size/crc32 guard this test is about can. That is also why the assertion
    // has to be `errored` alone, not merely "shorter than promised":
    // `Content-Length` is already wrong the moment the lying declared size is
    // planned around, so a body shorter than that promise is true whether or
    // not the guard runs — with the guard disabled, `generated` still finishes
    // `Ok(())`, having streamed every real page in full alongside a local
    // header that states a size those bytes do not have. `errored` is the one
    // signal that only the guard produces.
    assert!(
        errored,
        "a size-lying archive must abort the stream with an error rather \
         than silently finish; got {} of {promised} promised bytes with no \
         error",
        bytes.len()
    );
}

/// The other direction of the lie: a page holding more than its entry
/// declares has to abort the stream, not end it on a page cut to the declared
/// length, whether `for_each_member_page`'s read error or `generated`'s size
/// check is what stops it.
#[tokio::test]
async fn an_understated_size_aborts_the_download_instead_of_streaming_a_short_page() {
    let root = crate::test_support::Fixture::new("download-understated-size");
    let (promised, bytes, errored) = download_lying_edition(&root, 1).await;
    assert!(
        errored,
        "a page holding more than it declares must abort the stream with an \
         error; got {} of {promised} promised bytes with no error",
        bytes.len()
    );
}

/// Drives the route's own range walk — `generated`'s loop over `plan`,
/// `members` and `send` — through the route itself, so a bug in the walk can
/// make this fail.
///
/// This one locates every local file header in a whole download — a
/// `PK\x03\x04` search rather than a hard-coded offset list — and requests
/// small windows straddling each one.
#[tokio::test]
async fn a_route_download_matches_the_whole_at_every_local_header_boundary() {
    let tmp = crate::test_support::Fixture::new("download-boundary-walk");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let whole = all_of(crate::test_support::get(&router, "/api/works/t/raw/download").await).await;
    let total = whole.len();

    let headers_at: Vec<usize> = whole
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == [0x50, 0x4B, 0x03, 0x04])
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        headers_at.len(),
        5,
        "ComicInfo.xml plus four pages should give five local headers; found {:?}",
        headers_at
    );

    use tower::ServiceExt as _;
    let mut wrong: Vec<(u64, u64)> = Vec::new();
    for &b in &headers_at {
        for delta in [-1i64, 0, 1, 4, 5] {
            let from = (b as i64 + delta).max(0) as u64;
            let upto = (from + 20).min(total as u64);
            if from >= upto {
                continue;
            }
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::get("/api/works/t/raw/download")
                        .header(
                            axum::http::header::RANGE,
                            format!("bytes={from}-{}", upto - 1),
                        )
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let part = all_of(response).await;
            if part.as_slice() != &whole[from as usize..upto as usize] {
                wrong.push((from, upto));
            }
        }
    }
    assert!(wrong.is_empty(), "wrong windows: {wrong:?}");
}

/// Spec §10's own prescription — "fetch an edition whole; fetch it in N
/// randomly-chosen ranges; assert the concatenation is byte-identical" —
/// driven at the route for the same reason the boundary test above is:
/// `generated`'s walk is the thing this needs to prove.
#[tokio::test]
async fn n_random_ranges_of_a_route_download_reassemble_into_the_whole() {
    let tmp = crate::test_support::Fixture::new("download-random-ranges");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let whole = all_of(crate::test_support::get(&router, "/api/works/t/raw/download").await).await;
    let total = whole.len() as u64;

    use tower::ServiceExt as _;
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..24 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let a = seed % (total + 1);
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let b = seed % (total + 1);
        let (from, upto) = if a <= b { (a, b) } else { (b, a) };
        if from == upto {
            continue;
        }

        let response = router
            .clone()
            .oneshot(
                axum::http::Request::get("/api/works/t/raw/download")
                    .header(
                        axum::http::header::RANGE,
                        format!("bytes={from}-{}", upto - 1),
                    )
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let part = all_of(response).await;
        assert_eq!(
            part,
            whole[from as usize..upto as usize],
            "range [{from}, {upto}) is not that slice of the whole download"
        );
    }
}

/// A single-member edition whose one member is a directory of loose pages
/// (`raw-mono` in the real corpus) must take the generated path, not
/// passthrough: passthrough would report a directory's block count as
/// `Content-Length` and then fail to read it as a file at all. The corpus
/// helper that exercises the passthrough path (`a_corpus_edition_of_one_file`)
/// filters on `path.is_file()` itself, so it can never reach this case.
#[tokio::test]
async fn a_single_member_directory_edition_is_generated_not_passed_through() {
    let root = crate::test_support::Fixture::new("download-single-dir-edition");
    std::fs::create_dir_all(root.join("pages")).unwrap();
    std::fs::write(root.join("pages").join("0001.jpg"), b"page one").unwrap();

    let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "pages", "sha256": "x", "pages": 1 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
    let path = root.join("taiyaku.json");
    std::fs::write(&path, json).unwrap();
    let work = crate::Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(json).unwrap(),
    );
    let state = std::sync::Arc::new(crate::ServeState {
        library: crate::test_support::library(),
        works: crate::Works::fixed(vec![work]),
        source: std::sync::Arc::new(crate::LocalLibrary),
        cache_dir: root.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
        dirs: Vec::new(),
    });
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let response = crate::test_support::get(&router, "/api/works/t/raw/download").await;
    assert_eq!(
        response.status(),
        200,
        "a single-member directory edition must still download"
    );
    let bytes = all_of(response).await;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect(
        "must parse as a well-formed zip; the passthrough path would have \
         streamed a directory's raw bytes instead of a cbz",
    );
    assert!(
        archive.by_name("ComicInfo.xml").is_ok(),
        "only the generated path writes a metadata document; a directory \
         member must never take the passthrough path meant for one file"
    );
}

/// The `bytes=-N` suffix branch of `download::requested_range`. A zip reader
/// fetching a suffix range to locate the end-of-central-directory record is
/// exactly the client this exists for.
#[test]
fn a_suffix_range_answers_with_the_last_n_bytes() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::RANGE,
        "bytes=-500".parse().expect("a header value"),
    );
    assert_eq!(
        super::requested_range(&headers, 2000).unwrap(),
        (1500, 2000, true)
    );
}

/// Proves the route actually waits on `DOWNLOAD_LIMIT` rather than merely
/// declaring it — exhausting every permit by hand, rather than racing real
/// concurrent requests into overlapping, because "N requests happen to overlap
/// inside a window short enough to prove a bound and long enough not to flake
/// on a loaded machine" is not a knob this test wants to own.
///
/// **The hazard this trades in exchange: `DOWNLOAD_LIMIT` is a process-wide
/// `static`, and this test holds three or four of its four permits for most
/// of its own body** — one freed by `held.pop()`, the rest not dropped until
/// the end — while `a_two_member_edition_downloads_as_one_readable_cbz` and
/// the other real downloads in this file run concurrently with it under
/// `cargo test`'s default parallelism. Those tests carry no timeout of their
/// own, so this is only a serialisation point: they simply wait behind this
/// one for a permit rather than failing.
///
/// **The second request does not queue ahead of anything — there is nothing
/// for it to have queued behind by the time it exists.** `tokio::time::timeout`
/// *drops* the first request at 200 ms, which cancels its `acquire` and
/// removes it from the semaphore's wait list before it can ever be woken;
/// the second request is not even constructed until the line after
/// `held.pop()` runs. What this test actually leans on is weaker than
/// "the freed permit goes to whoever asked first": the second request's own
/// `acquire` can lose that freed permit to another download running
/// concurrently in this binary, in which case it simply keeps waiting for
/// whichever permit frees next — plausibly that same download completing and
/// releasing it back. The 5 s timeout has to be generous enough to absorb that
/// wait, not to honour a queueing guarantee; `.config/nextest.toml`'s own SLOW
/// margin suggests it is. Left as a comment rather than fixed: a dependency
/// (`serial_test`, not in `Cargo.lock`) or a hand-rolled test-only mutex every
/// future download test would have to remember to take are both real costs paid
/// against a hazard that has not manifested, the same
/// "a guessed number is dangerous" standard `MAX_CONCURRENT_DOWNLOADS`'s own
/// doc comment already applies to the send timeout beside it.
#[tokio::test]
async fn a_download_waits_for_a_permit_once_every_slot_is_held() {
    let tmp = crate::test_support::Fixture::new("download-concurrency-cap");
    let state = std::sync::Arc::new(crate::test_support::state(&tmp));
    let router =
        crate::test_support::authenticating(crate::build_router(state.clone()), &state.library);

    let mut held = Vec::new();
    for _ in 0..super::MAX_CONCURRENT_DOWNLOADS {
        held.push(
            super::DOWNLOAD_LIMIT
                .acquire()
                .await
                .expect("DOWNLOAD_LIMIT is never closed"),
        );
    }

    let blocked = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        crate::test_support::get(&router, "/api/works/t/raw/download"),
    )
    .await;
    assert!(
        blocked.is_err(),
        "a download started while every DOWNLOAD_LIMIT permit was held ran \
         anyway instead of waiting for one to free"
    );

    // Freeing one of the four must be enough to let the queued request
    // through — not merely stop it from ever completing.
    held.pop();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crate::test_support::get(&router, "/api/works/t/raw/download"),
    )
    .await
    .expect("a download waiting on a freed permit must proceed once one is free");
    assert_eq!(response.status(), 200);

    drop(held);
}

/// `send_with_timeout` is what `IDLE_SEND_TIMEOUT` actually rests on, and this
/// is the test that has to fail against a version of it with no timeout at
/// all: a naive `handle.block_on(tx.send(item))`, with `timeout` unused,
/// blocks forever on a full channel nobody drains. Against that version this
/// test would not hang for ten minutes — `done_rx.recv_timeout` bounds the
/// *test's* wait, independent of whatever the function under test does — and
/// would fail with:
///
/// `send_with_timeout did not return within 2s of a 50ms idle timeout; a
/// version with no timeout parks on the full channel forever`
#[test]
fn a_stalled_receiver_gives_up_rather_than_parking_forever() {
    let rt = tokio::runtime::Runtime::new().expect("a runtime");
    let handle = rt.handle().clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(1);
    // Fill the channel's one slot. `rx` is held alive to the end of the test
    // and never polled again, so nothing will ever free that slot — this is
    // the shape of a client that stopped reading, not a slow one.
    handle
        .block_on(tx.send(Ok(bytes::Bytes::new())))
        .expect("the empty channel accepts one send");

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = super::send_with_timeout(
            &tx,
            Ok(bytes::Bytes::new()),
            &handle,
            std::time::Duration::from_millis(50),
        );
        let _ = done_tx.send(outcome);
    });

    // 2 s is generous next to the 50 ms timeout under test, not a
    // measurement of anything — it exists so a version with no timeout at
    // all fails this test in about two seconds instead of hanging until
    // nextest's own slow-timeout kills the binary.
    let outcome = done_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect(
            "send_with_timeout did not return within 2s of a 50ms idle timeout; \
         a version with no timeout parks on the full channel forever",
        );
    assert!(
        matches!(outcome, super::SendOutcome::TimedOut),
        "a send into a full channel nobody reads must give up, got {outcome:?}"
    );
    drop(rx);
}

/// The complement of the test above: a consumer that is slow but never stops
/// reading must never be cut off by the idle timeout, because the timeout is
/// idle — reset by every accepted send — not a budget on the whole transfer.
/// Reads a chunk just under the timeout, repeatedly; if the timeout were
/// measured from the start of the download instead of from the last accepted
/// send, or applied per-download rather than per-send, this would time out on
/// one of the later sends instead of completing.
#[test]
fn a_slow_but_progressing_consumer_is_not_cut_off() {
    let rt = tokio::runtime::Runtime::new().expect("a runtime");
    let handle = rt.handle().clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(1);
    let timeout = std::time::Duration::from_millis(150);
    const SENDS: usize = 5;

    let producer = std::thread::spawn({
        let handle = handle.clone();
        move || {
            for _ in 0..SENDS {
                match super::send_with_timeout(&tx, Ok(bytes::Bytes::new()), &handle, timeout) {
                    super::SendOutcome::Sent => {}
                    other => return other,
                }
            }
            super::SendOutcome::Sent
        }
    });

    // Well under `timeout`, so each read lands long before the send it is
    // unblocking could ever be given up on.
    let read_interval = timeout - std::time::Duration::from_millis(100);
    for _ in 0..SENDS {
        std::thread::sleep(read_interval);
        let _ = handle.block_on(async {
            rx.recv()
                .await
                .expect("the producer thread is still sending")
        });
    }

    let outcome = producer.join().expect("the producer thread must not panic");
    assert!(
        matches!(outcome, super::SendOutcome::Sent),
        "a slow but progressing consumer must not be cut off, got {outcome:?}"
    );
}

// --- What type the feed advertises is what the route sends (addendum §2.2). ---

/// A work of one member, written the way `test_support::work` writes its
/// fixture — from JSON through `Sidecar::from_json`, since none of these
/// types derives `Default`.
fn one_member_work(dir: &std::path::Path, member: &str, write_it: bool) -> crate::Work {
    let root = dir;
    if write_it {
        std::fs::write(root.join(member), b"%PDF-1.4\n").unwrap();
    }

    let json = format!(
        r#"{{
          "taiyaku": 1,
          "work": {{ "title": "T", "reading": "rtl" }},
          "editions": {{
            "raw": {{ "label": "Raw", "members": [
              {{ "path": "{member}", "sha256": "x", "pages": 1 }}
            ], "spreads": [] }}
          }},
          "hub": "raw",
          "alignments": []
        }}"#
    );
    let path = root.join("taiyaku.json");
    std::fs::write(&path, &json).unwrap();
    crate::Work::new(
        "t".into(),
        path,
        taiyaku_core::Sidecar::from_json(&json).unwrap(),
    )
}

/// A generated edition is always a `STORE` zip, whatever its members are.
#[test]
fn a_multi_member_edition_advertises_the_comic_type() {
    let tmp = crate::test_support::Fixture::new("download-advertised-generated");
    let work = crate::test_support::work(&tmp);
    let sidecar = work.sidecar();
    let edition = sidecar.editions.get("raw").expect("fixture edition");
    assert_eq!(
        crate::download::advertised_type(&work, edition),
        crate::download::CBZ_TYPE
    );
}

/// A single-member edition whose member is **not** a comic archive must not
/// be advertised as one — §7's own argument, applied to the feed. Serving a
/// PDF as `application/vnd.comicbook+zip` would be a lie told twice; a feed
/// that advertises the same thing tells it a third time.
#[test]
fn a_single_pdf_member_advertises_the_pdf_type() {
    let tmp = crate::test_support::Fixture::new("download-advertised-pdf");
    let work = one_member_work(&tmp, "volume.pdf", true);
    let sidecar = work.sidecar();
    let edition = sidecar.editions.get("raw").expect("fixture edition");
    assert_eq!(
        crate::download::advertised_type(&work, edition),
        "application/pdf",
        "the route serves this passthrough as application/pdf, so the feed \
         must not advertise it as a comic archive"
    );
}

/// A member that is not on disk falls back rather than failing: one bad
/// edition must not turn into a 500 for a whole work's feed.
#[test]
fn a_missing_member_falls_back_to_the_comic_type() {
    let tmp = crate::test_support::Fixture::new("download-advertised-missing");
    let work = one_member_work(&tmp, "gone.pdf", false);
    let sidecar = work.sidecar();
    let edition = sidecar.editions.get("raw").expect("fixture edition");
    assert_eq!(
        crate::download::advertised_type(&work, edition),
        crate::download::CBZ_TYPE
    );
}

/// Spec §3.8's downloads, over
/// [`crate::test_support::router_over_an_edition_merged_from_two_libraries`].
mod narrowing {
    use crate::test_support::{self, EditionMergedFromTwoLibraries, Reader};
    use axum::http::{StatusCode, header};

    /// `raw`'s member files on disk, in order, as every account shares them;
    /// first stating the fixture's shape: part2, which Manga does not see, is
    /// not the last.
    fn raw_members(merged: &EditionMergedFromTwoLibraries) -> Vec<std::path::PathBuf> {
        let work = merged.state.find_blocking(&merged.slug).expect("the work");
        let paths: Vec<std::path::PathBuf> = work.sidecar().editions["raw"]
            .members
            .iter()
            .map(|member| std::path::PathBuf::from(&member.path))
            .collect();
        let files: Vec<&str> = paths
            .iter()
            .map(|path| path.file_name().and_then(|f| f.to_str()).expect("named"))
            .collect();
        assert_eq!(
            files,
            [
                "Foo v01 part1.cbz",
                "Foo v01 part2.cbz",
                "Foo v01 part3.cbz"
            ],
            "the fixture: raw's hidden-to-Manga member is not the last"
        );
        paths
    }

    /// Every entry of the zip `bytes`, by name in its own order.
    fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
        (0..archive.len())
            .map(|i| {
                let mut entry = archive.by_index(i).expect("an entry");
                let mut read = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut read).expect("reads the entry");
                (entry.name().expect("a name").into_owned(), read)
            })
            .collect()
    }

    async fn downloaded(
        merged: &EditionMergedFromTwoLibraries,
        reader: Reader,
        uri: &str,
    ) -> axum::http::Response<Vec<u8>> {
        let uri = format!("/api/works/{}/raw/{uri}", merged.slug);
        merged.get(reader, &uri, None).await
    }

    /// Manga's download of `raw` is a cbz of part1's and part3's pages, six
    /// in order, and no page of part2.
    #[tokio::test]
    async fn manga_alones_download_zips_part1_and_part3() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let members = raw_members(&merged);
        let pages: Vec<Vec<u8>> = [&members[0], &members[2]]
            .into_iter()
            .flat_map(|path| entries(&std::fs::read(path).expect("reads a member")))
            .map(|(_, bytes)| bytes)
            .collect();
        assert_eq!(pages.len(), 6, "part1's 2 pages and part3's 4");

        let sent = downloaded(&merged, Reader::MangaOnly, "download").await;
        assert_eq!(sent.status(), StatusCode::OK);
        assert_eq!(
            sent.headers()[header::CONTENT_TYPE],
            crate::download::CBZ_TYPE
        );
        let zipped = entries(sent.body());
        let names: Vec<&str> = zipped.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "ComicInfo.xml",
                "0001.png",
                "0002.png",
                "0003.png",
                "0004.png",
                "0005.png",
                "0006.png"
            ]
        );
        let sent_pages: Vec<Vec<u8>> = zipped.into_iter().skip(1).map(|(_, b)| b).collect();
        assert!(sent_pages == pages, "part1's pages, then part3's, in order");
    }

    /// Kids sees one member of `raw`, so its download is that file, passed
    /// through.
    #[tokio::test]
    async fn kids_alones_download_is_part2_passed_through() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let members = raw_members(&merged);
        let sent = downloaded(&merged, Reader::KidsOnly, "download").await;
        assert_eq!(sent.status(), StatusCode::OK);
        assert!(
            *sent.body() == std::fs::read(&members[1]).expect("reads part2"),
            "part2 byte for byte"
        );
    }

    /// Manga's members are numbered as its document lists them: 1 is part3,
    /// and there is no 2.
    #[tokio::test]
    async fn manga_alones_member_ordinals_are_its_documents() {
        let merged = test_support::router_over_an_edition_merged_from_two_libraries();
        let members = raw_members(&merged);
        let second = downloaded(&merged, Reader::MangaOnly, "members/1/download").await;
        assert_eq!(second.status(), StatusCode::OK);
        assert!(
            *second.body() == std::fs::read(&members[2]).expect("reads part3"),
            "Manga's member 1 is part3"
        );
        let past = downloaded(&merged, Reader::MangaOnly, "members/2/download").await;
        assert_eq!(past.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            String::from_utf8_lossy(past.body()),
            format!(
                "`raw` of `{}` has 2 members; there is no member 2",
                merged.slug
            )
        );
    }
}

/// O267: a download whose file cannot be read is answered by a sentence naming
/// the edition or the member, and the error, which names the server's path,
/// goes to the log alone.
mod answered_without_a_server_path {
    use axum::http::StatusCode;

    use crate::test_support::{self, Fixture};

    /// `uri`'s status and body, through a session over `state`.
    async fn answered(
        state: &std::sync::Arc<crate::ServeState>,
        uri: &str,
    ) -> (StatusCode, String) {
        let router =
            test_support::authenticating(crate::build_router(state.clone()), &state.library);
        let response = test_support::get(&router, uri).await;
        let status = response.status();
        (status, test_support::body_string(response).await)
    }

    /// One member that is not a file is not passed through, so the download
    /// is generated and describing its pages fails.
    #[tokio::test]
    async fn an_edition_whose_one_member_is_gone() {
        let (state, fixture) = test_support::state_with_real_pages();
        std::fs::remove_file(fixture.path().join("a1.cbz")).expect("removes the member");
        let (status, body) = answered(&state, "/api/works/w/a/download").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            body,
            "`a` of `w` could not be read; the server's log says why"
        );
        assert!(
            !body.contains(&fixture.path().display().to_string()),
            "{body}"
        );
    }

    #[tokio::test]
    async fn an_edition_one_of_whose_two_members_is_not_an_archive() {
        let tmp = Fixture::new("download-member-not-an-archive");
        let state = std::sync::Arc::new(test_support::state(&tmp));
        std::fs::write(tmp.path().join("c2.cbz"), b"not an archive").expect("overwrites c2");
        let (status, body) = answered(&state, "/api/works/t/raw/download").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            body,
            "`raw` of `t` could not be read; the server's log says why"
        );
        assert!(!body.contains(&tmp.path().display().to_string()), "{body}");
    }

    #[tokio::test]
    async fn a_member_whose_file_is_gone() {
        let tmp = Fixture::new("download-member-gone");
        let state = std::sync::Arc::new(test_support::state(&tmp));
        std::fs::remove_file(tmp.path().join("c2.cbz")).expect("removes c2");
        let (status, body) = answered(&state, "/api/works/t/raw/members/1/download").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(
            body,
            "member 1 of `raw` of `t` could not be read; the server's log says why"
        );
        assert!(!body.contains(&tmp.path().display().to_string()), "{body}");
    }
}

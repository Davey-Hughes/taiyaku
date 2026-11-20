use super::*;
use crate::ServeState;
use crate::test_support::{self, send_json};
use axum::http::{Method, StatusCode};

/// `media/Manga/{Volumes,Chapters,.hidden}`, a file beside them, and
/// `elsewhere/Private`. Answers the canonical `media` and `elsewhere`.
fn a_share(fixture: &test_support::Fixture) -> (PathBuf, PathBuf) {
    for dir in [
        "media/Manga/Volumes",
        "media/Manga/Chapters",
        "media/Manga/.hidden",
        "elsewhere/Private",
    ] {
        std::fs::create_dir_all(fixture.path().join(dir)).expect("mkdir");
    }
    std::fs::write(fixture.path().join("media/Manga/notes.txt"), b"x").expect("a file");
    (
        fixture
            .path()
            .join("media")
            .canonicalize()
            .expect("canonical"),
        fixture
            .path()
            .join("elsewhere")
            .canonicalize()
            .expect("canonical"),
    )
}

/// The one sentence a path outside every prefix is refused with: built from
/// the path as it was sent, whatever is or is not there.
fn outside_sentence(sent: &str) -> String {
    format!("{sent} is not inside a directory this server may browse")
}

fn names(listing: &Listing) -> Vec<&str> {
    listing.dirs.iter().map(|(name, _)| name.as_str()).collect()
}

#[test]
fn with_no_path_it_lists_the_prefixes_that_exist_and_nothing_with_none() {
    let fixture = test_support::Fixture::new("dirs-prefixes");
    let (media, _elsewhere) = a_share(&fixture);

    assert_eq!(
        list(&[], None),
        Ok(Listing {
            path: None,
            parent: None,
            dirs: Vec::new()
        })
    );
    let listed = list(&[media.clone(), fixture.path().join("not-mounted")], None).expect("lists");
    assert_eq!(
        listed,
        Listing {
            path: None,
            parent: None,
            dirs: vec![(media.to_string_lossy().into_owned(), media)],
        }
    );
}

#[test]
fn it_lists_subdirectories_by_name_and_nothing_else() {
    let fixture = test_support::Fixture::new("dirs-list");
    let (media, _elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");

    let listed = list(std::slice::from_ref(&media), Some(&manga)).expect("lists");

    assert_eq!(
        names(&listed),
        vec!["Chapters", "Volumes"],
        "no file, no dot-entry"
    );
    assert_eq!(listed.dirs[0].1, manga.join("Chapters"));
    assert_eq!(listed.path.as_deref(), Some(manga.as_path()));
    assert_eq!(listed.parent.as_deref(), Some(media.as_path()));
    assert_eq!(
        list(std::slice::from_ref(&media), Some(&media))
            .expect("lists")
            .parent,
        None,
        "a prefix has no parent to go up to"
    );
}

#[test]
fn a_path_that_leaves_every_prefix_is_refused_however_it_is_spelt() {
    let fixture = test_support::Fixture::new("dirs-outside");
    let (media, elsewhere) = a_share(&fixture);
    let prefixes = [media.clone()];

    for outside in [
        elsewhere.clone(),
        media.join("Manga/../../elsewhere"),
        media.join(".."),
    ] {
        assert!(
            matches!(list(&prefixes, Some(&outside)), Err(Refusal::Outside(_))),
            "{}",
            outside.display()
        );
    }
    assert!(matches!(
        list(&prefixes, Some(&media.join("Manga/Missing"))),
        Err(Refusal::Unreadable(_))
    ));
    assert!(
        matches!(list(&[], Some(&media)), Err(Refusal::Outside(_))),
        "with no prefix configured nothing may be browsed"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_prefix_is_neither_listed_nor_followed() {
    let fixture = test_support::Fixture::new("dirs-symlink");
    let (media, elsewhere) = a_share(&fixture);
    std::os::unix::fs::symlink(&elsewhere, media.join("Manga/Escape")).expect("a link out");
    std::os::unix::fs::symlink(media.join("Manga/Volumes"), media.join("Manga/Alias"))
        .expect("a link that stays inside");
    let prefixes = [media.clone()];

    let listed = list(&prefixes, Some(&media.join("Manga"))).expect("lists");
    assert_eq!(names(&listed), vec!["Alias", "Chapters", "Volumes"]);
    assert!(matches!(
        list(&prefixes, Some(&media.join("Manga/Escape"))),
        Err(Refusal::Outside(_))
    ));
}

/// `..` is resolved before the comparison, in both directions: one that
/// climbs out of the prefix is refused, one that stays inside lists where it
/// lands and says so in `path`.
#[test]
fn dot_dot_is_resolved_before_the_comparison() {
    let fixture = test_support::Fixture::new("dirs-dotdot");
    let (media, _elsewhere) = a_share(&fixture);
    std::fs::create_dir_all(media.join("a")).expect("mkdir");
    std::fs::create_dir_all(media.join("b/Inner")).expect("mkdir");
    let prefixes = [media.clone()];

    let climbing = media.join("a/../../elsewhere");
    assert!(
        matches!(list(&prefixes, Some(&climbing)), Err(Refusal::Outside(_))),
        "{}",
        climbing.display()
    );
    let listed = list(&prefixes, Some(&media.join("a/../b"))).expect("lists");
    assert_eq!(listed.path.as_deref(), Some(media.join("b").as_path()));
    assert_eq!(names(&listed), vec!["Inner"]);
    assert_eq!(listed.parent.as_deref(), Some(media.as_path()));
}

/// A prefix is compared by whole path components: `/media2` is not under
/// `/media`, and neither is anything beneath it.
#[test]
fn a_path_that_merely_begins_with_a_prefixs_characters_is_refused() {
    let fixture = test_support::Fixture::new("dirs-lookalike");
    let (media, _elsewhere) = a_share(&fixture);
    let lookalike = fixture.path().join("media2");
    std::fs::create_dir_all(lookalike.join("y")).expect("mkdir");
    let lookalike = lookalike.canonicalize().expect("canonical");
    let prefixes = [media];

    for outside in [lookalike.clone(), lookalike.join("y")] {
        assert!(
            matches!(list(&prefixes, Some(&outside)), Err(Refusal::Outside(_))),
            "{}",
            outside.display()
        );
    }
}

/// A path with no root is not resolved against the server's working
/// directory: it is refused as unreadable. A relative prefix is, by contrast,
/// the operator's own word and is made absolute when it is read.
#[test]
fn a_relative_request_path_is_refused_and_a_relative_prefix_is_made_absolute() {
    let fixture = test_support::Fixture::new("dirs-relative");
    let (media, _elsewhere) = a_share(&fixture);
    let here = std::env::current_dir()
        .expect("a working directory")
        .canonicalize()
        .expect("canonical");

    for relative in ["", ".", "Manga", "media/Manga"] {
        assert!(
            matches!(
                list(std::slice::from_ref(&here), Some(Path::new(relative))),
                Err(Refusal::Unreadable(_))
            ),
            "{relative:?}"
        );
    }
    let listed = list(&[PathBuf::from(".")], None).expect("lists");
    assert_eq!(
        listed.dirs,
        vec![(here.to_string_lossy().into_owned(), here.clone())]
    );
    assert!(list(&[PathBuf::from(".")], Some(&here)).is_ok());
    assert!(matches!(
        list(&[PathBuf::from(".")], Some(&media)),
        Err(Refusal::Outside(_))
    ));
}

/// Prefixes are canonicalised, de-duplicated and listed by name; one that is a
/// file, a missing path or a link to either is left out.
#[cfg(unix)]
#[test]
fn the_prefixes_are_canonical_once_each_and_by_name() {
    let fixture = test_support::Fixture::new("dirs-prefix-forms");
    let (media, elsewhere) = a_share(&fixture);
    let link = fixture.path().join("link-to-elsewhere");
    std::os::unix::fs::symlink(&elsewhere, &link).expect("a link");
    let file = fixture.path().join("a-file");
    std::fs::write(&file, b"x").expect("a file");
    let dangling = fixture.path().join("dangling");
    std::os::unix::fs::symlink(fixture.path().join("gone"), &dangling).expect("a link");
    let trailing = PathBuf::from(format!("{}/", media.display()));

    let listed = list(
        &[
            trailing.clone(),
            media.clone(),
            link,
            file,
            dangling,
            elsewhere.clone(),
            media.join("Manga"),
        ],
        None,
    )
    .expect("lists");

    let mut expected = vec![media.clone(), media.join("Manga"), elsewhere.clone()];
    expected.sort();
    assert_eq!(
        listed.dirs,
        expected
            .into_iter()
            .map(|p| (p.to_string_lossy().into_owned(), p))
            .collect::<Vec<_>>(),
        "a trailing slash and a link to a prefix name it once; a file and a dangling link are no prefix"
    );
    assert_eq!(
        list(&[trailing], Some(&media)).expect("lists").parent,
        None,
        "a prefix written with a trailing slash is still a prefix"
    );
}

/// A prefix that is itself a link is followed once, when it is read: what it
/// names is the prefix, and a path beneath the link's own spelling resolves
/// beneath it.
#[cfg(unix)]
#[test]
fn a_prefix_that_is_a_symlink_is_its_target() {
    let fixture = test_support::Fixture::new("dirs-prefix-link");
    let (media, elsewhere) = a_share(&fixture);
    let link = fixture.path().join("share");
    std::os::unix::fs::symlink(&media, &link).expect("a link");
    let prefixes = [link.clone()];

    let listed = list(&prefixes, Some(&link.join("Manga"))).expect("lists");
    assert_eq!(listed.path.as_deref(), Some(media.join("Manga").as_path()));
    assert_eq!(listed.parent.as_deref(), Some(media.as_path()));
    assert_eq!(
        list(&prefixes, Some(&link)).expect("lists").parent,
        None,
        "the link is a prefix"
    );
    assert!(matches!(
        list(&prefixes, Some(&elsewhere)),
        Err(Refusal::Outside(_))
    ));
}

/// Nothing leads above a prefix that no other prefix holds, and a directory one
/// level down leads to the one above it. Where one prefix lies inside
/// another, Up from the inner one leads to the outer one's directory above it.
#[test]
fn the_parent_stops_at_a_prefix_no_other_holds() {
    let fixture = test_support::Fixture::new("dirs-parent");
    let (media, _elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");
    let prefixes = [media.clone(), manga.clone()];

    assert_eq!(list(&prefixes, Some(&media)).expect("lists").parent, None);
    assert_eq!(
        list(&prefixes, Some(&manga)).expect("lists").parent,
        Some(media.clone()),
        "the inner prefix sits inside the outer one"
    );
    assert_eq!(
        list(std::slice::from_ref(&manga), Some(&manga))
            .expect("lists")
            .parent,
        None,
        "alone, the inner prefix leads nowhere"
    );
    let volumes = manga.join("Volumes");
    assert_eq!(
        list(&prefixes, Some(&volumes)).expect("lists").parent,
        Some(manga.clone())
    );
    assert_eq!(
        list(std::slice::from_ref(&media), Some(&volumes))
            .expect("lists")
            .parent,
        Some(manga)
    );
}

/// Beneath every prefix is the same as beneath any one, for a path inside a
/// prefix that sits inside another.
#[test]
fn a_nested_prefix_is_browsable_through_either() {
    let fixture = test_support::Fixture::new("dirs-nested");
    let (media, _elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");

    let listed = list(&[manga.clone(), media.clone()], Some(&manga)).expect("lists");
    assert_eq!(names(&listed), vec!["Chapters", "Volumes"]);
    let inner_only = list(std::slice::from_ref(&manga), Some(&media));
    assert!(
        matches!(inner_only, Err(Refusal::Outside(_))),
        "the outer directory is not beneath the inner prefix"
    );
}

/// A link that stays inside the prefixes is listed and can be entered; what it
/// names is what is listed, and its parent is that directory's.
#[cfg(unix)]
#[test]
fn a_symlink_inside_the_prefixes_is_entered_as_its_target() {
    let fixture = test_support::Fixture::new("dirs-inside-link");
    let (media, elsewhere) = a_share(&fixture);
    std::fs::create_dir_all(media.join("Manga/Volumes/Vol1")).expect("mkdir");
    std::os::unix::fs::symlink(media.join("Manga/Volumes"), media.join("Alias")).expect("a link");
    std::os::unix::fs::symlink(&elsewhere, media.join("Out")).expect("a link out");
    // Two prefixes: a link from one into the other stays inside.
    let prefixes = [media.clone(), elsewhere.clone()];
    std::os::unix::fs::symlink(&elsewhere, media.join("ToSecond")).expect("a link across");

    let top = list(&prefixes, Some(&media)).expect("lists");
    assert_eq!(
        names(&top),
        vec!["Alias", "Manga", "Out", "ToSecond"],
        "a link to another prefix is inside; one prefix covers each"
    );
    let entered = list(&prefixes, Some(&media.join("Alias"))).expect("lists");
    assert_eq!(
        entered.path.as_deref(),
        Some(media.join("Manga/Volumes").as_path())
    );
    assert_eq!(names(&entered), vec!["Vol1"]);
    assert_eq!(
        entered.parent.as_deref(),
        Some(media.join("Manga").as_path())
    );
    // With only the first prefix those same links out are neither listed nor
    // entered.
    let only = std::slice::from_ref(&media);
    assert_eq!(
        names(&list(only, Some(&media)).expect("lists")),
        vec!["Alias", "Manga"]
    );
    assert!(matches!(
        list(only, Some(&media.join("Out"))),
        Err(Refusal::Outside(_))
    ));
}

/// A link that loops or dangles is not a directory.
#[cfg(unix)]
#[test]
fn a_dangling_or_looping_symlink_is_not_listed() {
    let fixture = test_support::Fixture::new("dirs-bad-links");
    let (media, _elsewhere) = a_share(&fixture);
    std::os::unix::fs::symlink(media.join("gone"), media.join("Dangling")).expect("a link");
    std::os::unix::fs::symlink(media.join("Loop"), media.join("Loop")).expect("a loop");
    std::os::unix::fs::symlink(media.join("Manga/notes.txt"), media.join("ToFile")).expect("link");

    let listed = list(std::slice::from_ref(&media), Some(&media)).expect("lists");
    assert_eq!(names(&listed), vec!["Manga"]);
}

/// Names are by byte order of the name, so a capital sorts before any lower
/// case letter; a name that is not UTF-8 is not listed, since the answer is
/// JSON and could not say it.
#[cfg(unix)]
#[test]
fn names_are_in_byte_order_and_a_name_that_is_not_utf8_is_left_out() {
    use std::os::unix::ffi::OsStrExt as _;
    let fixture = test_support::Fixture::new("dirs-order");
    let (media, _elsewhere) = a_share(&fixture);
    let dir = media.join("Mixed");
    for name in ["beta", "Zeta", "alpha", "10", "9", "_under"] {
        std::fs::create_dir_all(dir.join(name)).expect("mkdir");
    }
    std::fs::create_dir_all(dir.join(std::ffi::OsStr::from_bytes(b"bad-\xff"))).expect("mkdir");

    let listed = list(std::slice::from_ref(&media), Some(&dir)).expect("lists");
    assert_eq!(
        names(&listed),
        vec!["10", "9", "Zeta", "_under", "alpha", "beta"]
    );
}

/// A directory this process cannot list, and a path that is a file, are both
/// the caller's to fix, with the path in the sentence.
///
/// The locked-directory half cannot run where the process can read a directory
/// whatever its mode: as root, which is how CI runs, it is skipped, and only a
/// developer's own run executes it. The file half runs everywhere.
#[cfg(unix)]
#[test]
fn a_directory_that_cannot_be_read_and_a_file_are_unreadable() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = test_support::Fixture::new("dirs-unreadable");
    let (media, _elsewhere) = a_share(&fixture);
    let prefixes = [media.clone()];
    let locked = media.join("Locked");
    std::fs::create_dir_all(&locked).expect("mkdir");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let readable_anyway = std::fs::read_dir(&locked).is_ok();
    let refused = list(&prefixes, Some(&locked));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    if !readable_anyway {
        let Err(Refusal::Unreadable(why)) = refused else {
            panic!("a directory that cannot be listed: {refused:?}");
        };
        assert!(why.contains("Locked"), "{why}");
    }

    let file = media.join("Manga/notes.txt");
    let Err(Refusal::Unreadable(why)) = list(&prefixes, Some(&file)) else {
        panic!("a file is not a directory");
    };
    assert!(why.contains("notes.txt"), "{why}");
}

/// A prefix that was never given leaves nothing browsable, whatever else the
/// path is.
#[test]
fn with_no_prefix_every_path_is_refused() {
    let fixture = test_support::Fixture::new("dirs-no-prefix");
    let (media, _elsewhere) = a_share(&fixture);
    let missing = [fixture.path().join("not-mounted")];

    for prefixes in [&[][..], &missing[..]] {
        for path in [media.clone(), PathBuf::from("/"), media.join("Manga")] {
            assert!(
                matches!(list(prefixes, Some(&path)), Err(Refusal::Outside(_))),
                "{}",
                path.display()
            );
        }
    }
}

/// A signed-in administrator's router over an empty library whose server was
/// started with `dirs`, and the state behind it.
fn a_router_browsing(
    fixture: &test_support::Fixture,
    dirs: Vec<PathBuf>,
) -> (axum::Router, Arc<ServeState>) {
    let state = Arc::new(ServeState {
        library: test_support::library(),
        works: crate::Works::fixed(Vec::new()),
        source: Arc::new(crate::LocalLibrary),
        cache_dir: fixture.join("covers"),
        dictionaries: None,
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        jobs_ended: Default::default(),
        library_job: Default::default(),
        fingerprints: Default::default(),
        matches: Default::default(),
        dirs,
    });
    let router = test_support::authenticating(crate::build_router(state.clone()), &state.library);
    (router, state)
}

/// The query for `path`, with its slashes encoded.
fn at(path: &Path) -> String {
    format!(
        "/api/library/dirs?path={}",
        path.to_string_lossy().replace('/', "%2F")
    )
}

/// What the route answered: its status and its body as JSON, or as text where
/// it is not JSON.
async fn answered(router: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = test_support::get(router, uri).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
}

/// A refusal's body: exactly one key, `error`, holding a sentence.
fn the_sentence(refused: &serde_json::Value) -> &str {
    let object = refused
        .as_object()
        .unwrap_or_else(|| panic!("an object: {refused}"));
    assert_eq!(
        object.keys().collect::<Vec<_>>(),
        vec!["error"],
        "a refusal carries `error` and nothing else: {refused}"
    );
    let sentence = refused["error"].as_str().expect("an error string");
    assert!(!sentence.is_empty(), "{refused}");
    sentence
}

#[tokio::test]
async fn the_route_lists_beneath_a_prefix_and_refuses_outside_it() {
    let fixture = test_support::Fixture::new("dirs-route");
    let (media, elsewhere) = a_share(&fixture);
    let (router, _state) = a_router_browsing(&fixture, vec![media.clone()]);

    let prefixes = test_support::get_json(&router, "/api/library/dirs").await;
    assert_eq!(
        prefixes,
        serde_json::json!({
            "path": null,
            "parent": null,
            "dirs": [{ "name": media.to_string_lossy(), "path": media.to_string_lossy() }],
        })
    );

    let manga = test_support::get_json(&router, &at(&media.join("Manga"))).await;
    assert_eq!(
        manga,
        serde_json::json!({
            "path": media.join("Manga").to_string_lossy(),
            "parent": media.to_string_lossy(),
            "dirs": [
                { "name": "Chapters", "path": media.join("Manga/Chapters").to_string_lossy() },
                { "name": "Volumes", "path": media.join("Manga/Volumes").to_string_lossy() },
            ],
        })
    );
    let at_the_prefix = test_support::get_json(&router, &at(&media)).await;
    assert_eq!(
        at_the_prefix["parent"],
        serde_json::Value::Null,
        "a prefix has no parent: {at_the_prefix}"
    );

    for (path, status) in [
        (at(&elsewhere), StatusCode::FORBIDDEN),
        (
            format!("{}%2F%2e%2e%2F%2e%2e%2Felsewhere", at(&media.join("Manga"))),
            StatusCode::FORBIDDEN,
        ),
        (at(&media.join("Missing")), StatusCode::BAD_REQUEST),
        (
            "/api/library/dirs?paht=%2F".to_string(),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (got, body) = answered(&router, &path).await;
        assert_eq!(got, status, "{path}: {body}");
        the_sentence(&body);
    }
}

/// An encoded `..` and a link out are refused through the route as they are
/// through `list`, and a refusal names no entry of the directory refused.
#[cfg(unix)]
#[tokio::test]
async fn the_route_resolves_dot_dot_and_links_before_it_compares() {
    let fixture = test_support::Fixture::new("dirs-route-escape");
    let (media, elsewhere) = a_share(&fixture);
    std::os::unix::fs::symlink(&elsewhere, media.join("Escape")).expect("a link out");
    let (router, _state) = a_router_browsing(&fixture, vec![media.clone()]);

    let top = test_support::get_json(&router, &at(&media)).await;
    assert_eq!(
        top["dirs"].as_array().expect("dirs").len(),
        1,
        "the link out is not offered: {top}"
    );
    let sent_through_dots = format!("{}/../elsewhere", media.display());
    let sent_through_more = format!("{}/../../elsewhere", media.join("Manga").display());
    for (uri, sent) in [
        (
            at(&media.join("Escape")),
            media.join("Escape").display().to_string(),
        ),
        (
            format!(
                "/api/library/dirs?path={}%2F..%2Felsewhere",
                at(&media).split_once('=').unwrap().1
            ),
            sent_through_dots,
        ),
        (
            format!(
                "/api/library/dirs?path={}%2F..%2F..%2Felsewhere",
                at(&media.join("Manga")).split_once('=').unwrap().1
            ),
            sent_through_more,
        ),
    ] {
        let (status, body) = answered(&router, &uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
        assert_eq!(
            body,
            serde_json::json!({ "error": outside_sentence(&sent) }),
            "a refusal is the one sentence about the path sent: {uri}"
        );
    }
}

/// Other spellings of the query that are not one `path`: each is the
/// caller's mistake and answers 400 in JSON, as the neighbouring routes' do.
#[tokio::test]
async fn a_query_that_is_not_one_path_is_a_400_in_json() {
    let fixture = test_support::Fixture::new("dirs-route-queries");
    let (media, _elsewhere) = a_share(&fixture);
    let (router, _state) = a_router_browsing(&fixture, vec![media.clone()]);
    let one = at(&media);
    let value = one.split_once('=').expect("a value").1.to_string();

    for query in [
        "?path=".to_string(),
        "?path".to_string(),
        format!("?path={value}&path={value}"),
        format!("?path={value}&extra=1"),
        "?other=1".to_string(),
        "?path=relative%2Fmedia".to_string(),
    ] {
        let uri = format!("/api/library/dirs{query}");
        let (status, body) = answered(&router, &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        the_sentence(&body);
    }
}

#[tokio::test]
async fn with_no_prefix_the_route_lists_nothing_and_refuses_every_path() {
    let fixture = test_support::Fixture::new("dirs-route-none");
    let (media, _elsewhere) = a_share(&fixture);
    let (router, _state) = a_router_browsing(&fixture, Vec::new());

    let top = test_support::get_json(&router, "/api/library/dirs").await;
    assert_eq!(
        top,
        serde_json::json!({ "path": null, "parent": null, "dirs": [] })
    );
    let (status, body) = answered(&router, &at(&media)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    the_sentence(&body);
}

/// A reader asking for a path inside a prefix, an API key doing the same, and
/// a request with no sign-in, are each refused with the status the
/// neighbouring routes give, and are told nothing about the directory.
#[tokio::test]
async fn the_dirs_route_refuses_a_reader_a_key_and_nobody() {
    use tower::ServiceExt as _;
    let fixture = test_support::Fixture::new("dirs-route-refused");
    let (media, _elsewhere) = a_share(&fixture);
    let (router, state) = a_router_browsing(&fixture, vec![media.clone()]);
    let uri = at(&media.join("Manga"));

    // An administrator asking is answered, so what follows is the refusal's.
    assert_eq!(
        test_support::get(&router, &uri).await.status(),
        StatusCode::OK
    );

    // The same server, its one account demoted.
    {
        let conn = state.library.conn();
        let demoted = conn
            .execute("UPDATE user SET role = 'user' WHERE id = 1", [])
            .expect("demotes the fixture account");
        assert_eq!(demoted, 1);
    }
    for path in [uri.as_str(), "/api/library/dirs"] {
        let response = send_json(&router, Method::GET, path, &serde_json::json!({})).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "a reader: {path}");
        let body = test_support::body_string(response).await;
        assert!(!body.contains("Volumes"), "a reader was told: {body}");
    }
    {
        let conn = state.library.conn();
        conn.execute("UPDATE user SET role = 'admin' WHERE id = 1", [])
            .expect("restores the fixture account");
    }

    let keyed = router
        .clone()
        .oneshot(
            axum::http::Request::get(&uri)
                .header(
                    axum::http::header::AUTHORIZATION,
                    test_support::fixture_authorization(),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(keyed.status(), StatusCode::FORBIDDEN);
    let body = test_support::body_string(keyed).await;
    assert!(body.contains("API key"), "{body}");
    assert!(!body.contains("Volumes"), "{body}");

    let unsigned = crate::build_router(state.clone());
    let roots = test_support::get(&unsigned, "/api/library/roots").await;
    assert_eq!(roots.status(), StatusCode::UNAUTHORIZED);
    let response = test_support::get(&unsigned, &uri).await;
    assert_eq!(response.status(), roots.status());
    let body = test_support::body_string(response).await;
    assert!(!body.contains("Volumes"), "nobody was told: {body}");
}

/// What a path outside the prefixes is refused with, as `list` answers it.
fn refused_outside(prefixes: &[PathBuf], sent: &Path) -> String {
    match list(prefixes, Some(sent)) {
        Err(Refusal::Outside(why)) => why,
        other => panic!("{} should be refused as outside: {other:?}", sent.display()),
    }
}

/// An existing directory, an existing file and a missing path, all outside,
/// are refused alike: the answer does not say which exist, and holds no path
/// but the one sent.
#[test]
fn outside_an_existing_a_file_and_a_missing_path_are_refused_alike() {
    let fixture = test_support::Fixture::new("dirs-outside-alike");
    let (media, elsewhere) = a_share(&fixture);
    std::fs::write(elsewhere.join("secret.txt"), b"x").expect("a file");
    let prefixes = [media];

    for sent in [
        elsewhere.clone(),
        elsewhere.join("Private"),
        elsewhere.join("secret.txt"),
        elsewhere.join("Nope"),
        elsewhere.join("Nope/Deeper"),
        elsewhere.join("secret.txt/x"),
    ] {
        let why = refused_outside(&prefixes, &sent);
        assert_eq!(why, outside_sentence(&sent.display().to_string()));
    }
}

/// Beneath a link that leaves the prefixes, the link's target is never named,
/// and the link itself, a path through it and one that does not exist through
/// it are refused alike.
#[cfg(unix)]
#[test]
fn beneath_a_link_out_the_refusal_names_only_what_was_sent() {
    let fixture = test_support::Fixture::new("dirs-link-out-refusal");
    let (media, elsewhere) = a_share(&fixture);
    std::os::unix::fs::symlink(&elsewhere, media.join("Escape")).expect("a link out");
    let prefixes = [media.clone()];

    for sent in [
        media.join("Escape"),
        media.join("Escape/Private"),
        media.join("Escape/Nope"),
    ] {
        let why = refused_outside(&prefixes, &sent);
        assert_eq!(why, outside_sentence(&sent.display().to_string()));
        assert!(
            !why.contains(&elsewhere.display().to_string()),
            "the link's target was named: {why}"
        );
    }
}

/// Magic links name the server's own working directory and binary; a refusal
/// must not.
#[cfg(target_os = "linux")]
#[test]
fn a_proc_link_is_refused_without_naming_where_it_points() {
    let fixture = test_support::Fixture::new("dirs-proc");
    let (media, _elsewhere) = a_share(&fixture);
    let prefixes = [media];
    let cwd = std::env::current_dir().expect("a working directory");
    let exe = std::env::current_exe().expect("a binary");

    for (sent, hidden) in [
        ("/proc/self/cwd", cwd.display().to_string()),
        ("/proc/self/exe", exe.display().to_string()),
    ] {
        let why = refused_outside(&prefixes, Path::new(sent));
        assert_eq!(why, outside_sentence(sent));
        assert!(!why.contains(&hidden), "{why} names {hidden}");
    }
}

/// Beneath a file, and beneath a directory that cannot be searched, outside
/// the prefixes: no operating-system error text, the same refusal.
///
/// The locked-directory half degrades where the process can search a directory
/// whatever its mode: as root, which is how CI runs, `Locked` is searchable and
/// the path through it is refused for being outside all the same, so the half
/// still runs there but no longer sets the mode's own error against it.
#[cfg(unix)]
#[test]
fn beneath_a_file_or_a_locked_directory_outside_there_is_no_os_error() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = test_support::Fixture::new("dirs-outside-os");
    let (media, elsewhere) = a_share(&fixture);
    std::fs::write(elsewhere.join("f.txt"), b"x").expect("a file");
    let locked = elsewhere.join("Locked");
    std::fs::create_dir_all(locked.join("Inner")).expect("mkdir");
    let prefixes = [media];

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let answers: Vec<(PathBuf, String)> = [
        elsewhere.join("f.txt/x"),
        locked.join("Inner"),
        locked.join("Nope"),
    ]
    .into_iter()
    .map(|sent| {
        let why = refused_outside(&prefixes, &sent);
        (sent, why)
    })
    .collect();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    for (sent, why) in answers {
        assert_eq!(why, outside_sentence(&sent.display().to_string()));
        for os_text in ["Not a directory", "Permission denied", "No such file"] {
            assert!(!why.contains(os_text), "{why}");
        }
    }
}

/// Inside the prefixes a missing directory is still the caller's typo, a 400 with
/// the operating system's words: directly, beneath a link that stays inside,
/// and beneath a file.
#[cfg(unix)]
#[test]
fn inside_a_missing_dir_is_still_a_400_with_a_sentence() {
    let fixture = test_support::Fixture::new("dirs-inside-missing");
    let (media, _elsewhere) = a_share(&fixture);
    std::os::unix::fs::symlink(media.join("Manga/Volumes"), media.join("Alias")).expect("a link");
    let prefixes = [media.clone()];

    for sent in [
        media.join("Manga/Missing"),
        media.join("Manga/Missing/Deeper"),
        media.join("Alias/Missing"),
        media.join("Manga/notes.txt/x"),
    ] {
        let Err(Refusal::Unreadable(why)) = list(&prefixes, Some(&sent)) else {
            panic!("{} is a typo inside a prefix", sent.display());
        };
        assert!(why.contains(&sent.display().to_string()), "{why}");
        assert!(why.len() > sent.display().to_string().len(), "{why}");
    }
}

/// The route answers an outside path as one JSON sentence, existing or not.
#[tokio::test]
async fn the_route_refuses_outside_paths_with_one_body() {
    let fixture = test_support::Fixture::new("dirs-route-alike");
    let (media, elsewhere) = a_share(&fixture);
    let (router, _state) = a_router_browsing(&fixture, vec![media]);

    for sent in [elsewhere.join("Private"), elsewhere.join("Nope")] {
        let (status, body) = answered(&router, &at(&sent)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(
            body,
            serde_json::json!({ "error": outside_sentence(&sent.display().to_string()) })
        );
    }
}

/// A linked entry is judged by where the whole chain ends, not by one hop, and
/// a relative link is judged by where it lands, not by how it is written.
#[cfg(unix)]
#[test]
fn a_chain_of_links_and_a_relative_link_are_judged_by_where_they_end() {
    let fixture = test_support::Fixture::new("dirs-chains");
    let (media, elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");
    std::os::unix::fs::symlink(&elsewhere, manga.join("Inner")).expect("a link out");
    std::os::unix::fs::symlink(manga.join("Inner"), manga.join("Hop")).expect("a chain");
    std::os::unix::fs::symlink("../../elsewhere", manga.join("Rel")).expect("a relative link out");
    std::os::unix::fs::symlink("Volumes", manga.join("RelIn")).expect("a relative link inside");
    let prefixes = [media];

    let listed = list(&prefixes, Some(&manga)).expect("lists");
    assert_eq!(names(&listed), vec!["Chapters", "RelIn", "Volumes"]);
    for out in ["Hop", "Inner", "Rel"] {
        assert!(
            matches!(
                list(&prefixes, Some(&manga.join(out))),
                Err(Refusal::Outside(_))
            ),
            "{out}"
        );
    }
}

/// The directory is read by its canonical path, so an entry reached through a
/// link or a `..` is named beneath where it really is.
#[cfg(unix)]
#[test]
fn entries_are_named_beneath_the_canonical_path() {
    let fixture = test_support::Fixture::new("dirs-entry-paths");
    let (media, _elsewhere) = a_share(&fixture);
    std::fs::create_dir_all(media.join("Manga/Volumes/Vol1")).expect("mkdir");
    std::os::unix::fs::symlink(media.join("Manga/Volumes"), media.join("Alias")).expect("a link");
    let prefixes = [media.clone()];
    let real = media.join("Manga/Volumes/Vol1");

    for sent in [media.join("Alias"), media.join("Manga/Chapters/../Volumes")] {
        let listed = list(&prefixes, Some(&sent)).expect("lists");
        assert_eq!(
            listed.dirs,
            vec![("Vol1".to_string(), real.clone())],
            "{}",
            sent.display()
        );
    }
}

/// A prefix beneath a directory whose name is not UTF-8 cannot be sent as JSON
/// or entered by a path in a query, so it is not offered.
#[cfg(unix)]
#[test]
fn a_prefix_that_is_not_utf8_is_left_out_of_the_top_listing() {
    use std::os::unix::ffi::OsStrExt as _;
    let fixture = test_support::Fixture::new("dirs-prefix-utf8");
    let (media, _elsewhere) = a_share(&fixture);
    let odd = fixture
        .path()
        .join(std::ffi::OsStr::from_bytes(b"bad-\xff"))
        .join("Shelf");
    std::fs::create_dir_all(&odd).expect("mkdir");

    let listed = list(&[odd, media.clone()], None).expect("lists");
    assert_eq!(
        listed.dirs,
        vec![(media.to_string_lossy().into_owned(), media)]
    );
}

/// What a deployer reads on the console when a directory is not usable: one line
/// for each, naming the path whichever way it was given, and one for a directory
/// that is there but whose name is not UTF-8 (it would be an empty browser
/// with no reason).
#[test]
fn a_prefix_that_is_not_there_is_named_at_startup() {
    let fixture = test_support::Fixture::new("dirs-notice");
    let (media, _elsewhere) = a_share(&fixture);
    let gone = fixture.path().join("not-mounted");
    let file = fixture.path().join("a-file");
    std::fs::write(&file, b"x").expect("a file");
    let line = |path: &Path| {
        format!(
            "directory browser: {} is not a directory here, or its name is not UTF-8, so it lists nothing",
            path.display()
        )
    };

    assert_eq!(notices(std::slice::from_ref(&media)), Vec::<String>::new());
    assert_eq!(
        notices(&[media, gone.clone(), file.clone()]),
        vec![line(&gone), line(&file)]
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        let odd = fixture
            .path()
            .join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
        std::fs::create_dir_all(&odd).expect("mkdir");
        assert_eq!(notices(std::slice::from_ref(&odd)), vec![line(&odd)]);
    }
}

/// The lines are what `announce_dirs` says, once each. That `serve` calls it
/// is held by the CLI crate's `serve_says_which_dirs_it_cannot_browse`,
/// which reads a spawned server's console: a call dropped from `serve` passes
/// here.
#[test]
fn the_startup_lines_are_what_serve_prints() {
    let fixture = test_support::Fixture::new("dirs-startup");
    let gone = fixture.path().join("not-mounted");
    let mut printed = Vec::new();
    crate::announce_dirs(std::slice::from_ref(&gone), &mut |line| {
        printed.push(line.to_string());
    });
    assert_eq!(printed, notices(std::slice::from_ref(&gone)));
    assert_eq!(printed.len(), 1);
}

/// `a_share` plus a file and a directory that cannot be searched outside, and
/// a directory beside `media` to be reached by going out and coming back.
#[cfg(unix)]
fn a_share_with_locked_ground(fixture: &test_support::Fixture) -> (PathBuf, PathBuf) {
    let (media, elsewhere) = a_share(fixture);
    std::fs::write(elsewhere.join("secret.txt"), b"x").expect("a file");
    std::fs::create_dir_all(elsewhere.join("Locked/Inner")).expect("mkdir");
    (media, elsewhere)
}

/// A path that goes out of the prefixes and comes back in is refused the same
/// whatever it passes through: a directory, a missing name, a file, a
/// directory that cannot be searched. Otherwise "what exists out there" can be
/// read off whether the path is listed.
///
/// The locked-directory spelling degrades as root (CI): `Locked` is
/// searchable there, and the path is refused for leaving the prefixes
/// regardless, so only the mode's own error is no longer set against it.
#[cfg(unix)]
#[test]
fn a_path_that_goes_out_and_comes_back_is_refused_however_it_passes() {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = test_support::Fixture::new("dirs-out-and-back");
    let (media, elsewhere) = a_share_with_locked_ground(&fixture);
    let locked = elsewhere.join("Locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let prefixes = [media.clone()];

    let mut sent = Vec::new();
    for through in ["Private", "Nope", "secret.txt", "Locked", "Locked/Inner"] {
        sent.push(elsewhere.join(through).join("../../media"));
        sent.push(elsewhere.join(through).join("../../media/nope"));
        sent.push(
            media
                .join("../elsewhere")
                .join(through)
                .join("../../media/Manga"),
        );
    }
    sent.push(media.join(".."));
    sent.push(media.join("../media/Manga"));
    let answers: Vec<(PathBuf, Result<Listing, Refusal>)> = sent
        .into_iter()
        .map(|path| {
            let answer = list(&prefixes, Some(&path));
            (path, answer)
        })
        .collect();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    for (path, answer) in answers {
        assert_eq!(
            answer,
            Err(Refusal::Outside(outside_sentence(
                &path.display().to_string()
            ))),
            "{}",
            path.display()
        );
    }
}

/// The same through the route: one status, one whole body.
#[tokio::test]
async fn the_route_answers_out_and_back_paths_with_one_body() {
    let fixture = test_support::Fixture::new("dirs-route-out-and-back");
    let (media, elsewhere) = a_share(&fixture);
    let (router, _state) = a_router_browsing(&fixture, vec![media]);

    for sent in [
        elsewhere.join("Private/../../media"),
        elsewhere.join("Nope/../../media"),
        elsewhere.join("Private/../../media/nope"),
    ] {
        let (status, body) = answered(&router, &at(&sent)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{}: {body}", sent.display());
        assert_eq!(
            body,
            serde_json::json!({ "error": outside_sentence(&sent.display().to_string()) })
        );
    }
}

/// A link inside a prefix that points out is refused whatever it points at:
/// a directory, a file, nothing, something beneath a file, something beneath a
/// directory that cannot be searched, a loop. One refusal, no operating-system
/// text, never offered in the listing, and none lets a caller read what exists
/// outside.
///
/// The unsearchable half degrades as root (CI), where `Locked` can be searched
/// and the link is refused for pointing out regardless.
#[cfg(unix)]
#[test]
fn a_link_inside_that_points_out_is_refused_whatever_it_points_at() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let fixture = test_support::Fixture::new("dirs-link-targets");
    let (media, elsewhere) = a_share_with_locked_ground(&fixture);
    symlink(elsewhere.join("L2"), elsewhere.join("L1")).expect("a loop");
    symlink(elsewhere.join("L1"), elsewhere.join("L2")).expect("a loop");
    symlink(media.join("gone"), media.join("Dangling")).expect("a link");
    let targets = [
        ("ToDir", elsewhere.join("Private")),
        ("ToFile", elsewhere.join("secret.txt")),
        ("ToNothing", elsewhere.join("Gone")),
        ("BeneathFile", elsewhere.join("secret.txt/x")),
        ("BeneathLocked", elsewhere.join("Locked/Inner")),
        ("ToLoop", elsewhere.join("L1")),
    ];
    for (name, target) in &targets {
        symlink(target, media.join(name)).expect("a link out");
    }
    let locked = elsewhere.join("Locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let prefixes = [media.clone()];

    let mut sent: Vec<PathBuf> = targets.iter().map(|(name, _)| media.join(name)).collect();
    sent.push(media.join("Dangling"));
    let mut deeper: Vec<PathBuf> = sent.iter().map(|p| p.join("deeper")).collect();
    sent.append(&mut deeper);
    let answers: Vec<(PathBuf, Result<Listing, Refusal>)> = sent
        .into_iter()
        .map(|path| {
            let answer = list(&prefixes, Some(&path));
            (path, answer)
        })
        .collect();
    let top = list(&prefixes, Some(&media));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    for (path, answer) in answers {
        assert_eq!(
            answer,
            Err(Refusal::Outside(outside_sentence(
                &path.display().to_string()
            ))),
            "{}",
            path.display()
        );
    }
    assert_eq!(names(&top.expect("lists")), vec!["Manga"]);
}

/// A link inside whose chain goes through a link outside and comes back in is
/// not followed: each hop is judged on its own, so the walk never rests on
/// ground outside the prefixes.
#[cfg(unix)]
#[test]
fn a_chain_that_leaves_and_returns_is_not_followed() {
    use std::os::unix::fs::symlink;
    let fixture = test_support::Fixture::new("dirs-chain-returns");
    let (media, elsewhere) = a_share(&fixture);
    symlink(media.join("Manga"), elsewhere.join("Back")).expect("a link back in");
    symlink(elsewhere.join("Back"), media.join("Via")).expect("a link out and back");
    symlink("../../elsewhere/Back", media.join("Manga/RelVia")).expect("a relative one");
    let prefixes = [media.clone()];

    for sent in [media.join("Via"), media.join("Manga/RelVia")] {
        assert_eq!(
            list(&prefixes, Some(&sent)),
            Err(Refusal::Outside(outside_sentence(
                &sent.display().to_string()
            ))),
            "{}",
            sent.display()
        );
    }
    assert_eq!(
        names(&list(&prefixes, Some(&media)).expect("lists")),
        vec!["Manga"],
        "Via is not offered"
    );
    assert_eq!(
        names(&list(&prefixes, Some(&media.join("Manga"))).expect("lists")),
        vec!["Chapters", "Volumes"],
        "RelVia is not offered"
    );
}

/// A link that stays inside, written relative with `..` or absolute, is
/// offered and entered, and `..` that stays inside is resolved.
#[cfg(unix)]
#[test]
fn links_and_dot_dot_that_stay_inside_are_still_followed() {
    use std::os::unix::fs::symlink;
    let fixture = test_support::Fixture::new("dirs-stay-inside");
    let (media, _elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");
    std::fs::create_dir_all(manga.join("Volumes/Vol1")).expect("mkdir");
    symlink("../Manga/Volumes", manga.join("Up")).expect("a relative link");
    symlink(manga.join("Volumes"), manga.join("Abs")).expect("an absolute link");
    let prefixes = [media.clone()];
    let volumes = manga.join("Volumes");

    assert_eq!(
        names(&list(&prefixes, Some(&manga)).expect("lists")),
        vec!["Abs", "Chapters", "Up", "Volumes"]
    );
    for sent in [
        manga.join("Up"),
        manga.join("Abs"),
        media.join("Manga/../Manga/Volumes"),
    ] {
        let listed = list(&prefixes, Some(&sent)).expect("lists");
        assert_eq!(
            listed.path.as_deref(),
            Some(volumes.as_path()),
            "{}",
            sent.display()
        );
        assert_eq!(listed.parent.as_deref(), Some(manga.as_path()));
        assert_eq!(names(&listed), vec!["Vol1"]);
    }
}

/// A missing directory inside is still the 400 in the system's words, by its plain
/// spelling and beneath a link that stays inside; it is the only answer that
/// carries them.
#[cfg(unix)]
#[test]
fn a_missing_dir_inside_is_still_a_400_in_the_systems_words() {
    use std::os::unix::fs::symlink;
    let fixture = test_support::Fixture::new("dirs-missing-inside");
    let (media, _elsewhere) = a_share(&fixture);
    symlink(media.join("Manga/Volumes"), media.join("Alias")).expect("a link");
    symlink("Volumes", media.join("Manga/Rel")).expect("a relative link");
    let prefixes = [media.clone()];

    for sent in [
        media.join("Manga/Missing"),
        media.join("Alias/Missing"),
        media.join("Manga/Rel/Missing"),
        media.join("Manga/Missing/../Chapters"),
    ] {
        let Err(Refusal::Unreadable(why)) = list(&prefixes, Some(&sent)) else {
            panic!("{} is a typo inside a prefix", sent.display());
        };
        assert_eq!(
            why,
            format!("{}: No such file or directory (os error 2)", sent.display())
        );
    }
}

/// Every path the route answers is UTF-8: a link inside to a directory inside
/// whose name is not, is neither offered nor entered.
#[cfg(unix)]
#[test]
fn a_link_to_a_directory_whose_name_is_not_utf8_is_neither_offered_nor_entered() {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::symlink;
    let fixture = test_support::Fixture::new("dirs-link-not-utf8");
    let (media, _elsewhere) = a_share(&fixture);
    let odd = media.join(std::ffi::OsStr::from_bytes(b"odd-\xff"));
    std::fs::create_dir_all(odd.join("Inner")).expect("mkdir");
    symlink(&odd, media.join("ToOdd")).expect("a link");
    let prefixes = [media.clone()];

    assert_eq!(
        names(&list(&prefixes, Some(&media)).expect("lists")),
        vec!["Manga"]
    );
    let sent = media.join("ToOdd");
    assert_eq!(
        list(&prefixes, Some(&sent)),
        Err(Refusal::Outside(outside_sentence(
            &sent.display().to_string()
        )))
    );
}

/// The directories `serve` is started with reach the handler: through a socket, an
/// administrator's session lists what is beneath them. Every other test here
/// builds its state by hand, so none can see `serve` drop the option.
///
/// `multi_thread` and the poll are `serve_binds_the_address_and_answers_on_it`'s,
/// for its reasons.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_gives_the_browser_the_dirs_it_was_started_with() {
    let fixture = test_support::Fixture::new("dirs-serve");
    let (media, _elsewhere) = a_share(&fixture);
    let library = test_support::library();
    let token = taiyaku_library::credential::mint_token();
    {
        let conn = library.conn();
        taiyaku_library::account::open_session(&conn, 1, &token, Some("the suite"), None)
            .expect("opens a session");
    }

    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = fixture.join("cache");
    let serving = tokio::spawn({
        let addr = addr.clone();
        let dirs = vec![media.clone()];
        async move {
            crate::serve(
                library,
                crate::Works::fixed(Vec::new()),
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    dirs,
                    ..crate::ServeOptions::default()
                },
            )
            .await
        }
    });

    let url = format!("http://{addr}/api/library/dirs");
    let mut answered = None;
    for _ in 0..100 {
        if serving.is_finished() {
            break;
        }
        if let Ok(mut r) = ureq::get(&url)
            .header("cookie", format!("{}={token}", crate::session::COOKIE_NAME))
            .call()
        {
            answered = Some(r.body_mut().read_to_string().unwrap_or_default());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();

    let body = answered.expect("serve must bind the address and answer on it");
    let listed: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(
        listed,
        serde_json::json!({
            "path": null,
            "parent": null,
            "dirs": [{ "name": media.to_string_lossy(), "path": media.to_string_lossy() }],
        })
    );
}

/// Each linked entry of a listing gets a budget of links of its own: links that
/// loop, and so spend all of theirs, do not take the directories listed after them
/// out of the listing.
#[cfg(unix)]
#[test]
fn a_looping_link_does_not_spend_the_budget_of_the_entries_beside_it() {
    use std::os::unix::fs::symlink;
    let fixture = test_support::Fixture::new("dirs-budget-per-entry");
    let (media, _elsewhere) = a_share(&fixture);
    let many = media.join("Many");
    std::fs::create_dir_all(&many).expect("mkdir");
    for loop_name in ["LoopA", "LoopB", "LoopC"] {
        symlink(loop_name, many.join(loop_name)).expect("a loop");
    }
    for n in 0..60 {
        symlink(
            media.join("Manga/Volumes"),
            many.join(format!("Good{n:02}")),
        )
        .expect("a link");
    }

    let listed = list(&[media], Some(&many)).expect("lists");
    assert_eq!(listed.dirs.len(), 60, "{:?}", names(&listed));
    assert!(names(&listed).iter().all(|name| name.starts_with("Good")));
}

/// A chain of links is followed up to the bound the system's own resolution
/// has, forty, and refused beyond it: the fortieth link of a chain, all of it
/// inside, is entered and offered, the forty-first is the outside refusal and
/// is not offered.
#[cfg(unix)]
#[test]
fn a_chain_of_forty_links_is_followed_and_forty_one_is_not() {
    use std::os::unix::fs::symlink;
    let fixture = test_support::Fixture::new("dirs-chain-bound");
    let (media, _elsewhere) = a_share(&fixture);
    let chain = media.join("Chain");
    std::fs::create_dir_all(chain.join("target/Inner")).expect("mkdir");
    symlink(chain.join("target"), chain.join("l01")).expect("a link");
    for n in 2..=41 {
        symlink(
            chain.join(format!("l{:02}", n - 1)),
            chain.join(format!("l{n:02}")),
        )
        .expect("a link");
    }
    let prefixes = [media];

    let at_forty = list(&prefixes, Some(&chain.join("l40"))).expect("forty is followed");
    assert_eq!(
        at_forty.path.as_deref(),
        Some(chain.join("target").as_path())
    );
    assert_eq!(names(&at_forty), vec!["Inner"]);
    let sent = chain.join("l41");
    assert_eq!(
        list(&prefixes, Some(&sent)),
        Err(Refusal::Outside(outside_sentence(
            &sent.display().to_string()
        )))
    );
    let listed = list(&prefixes, Some(&chain)).expect("lists");
    let offered = names(&listed);
    assert!(offered.contains(&"l40"), "{offered:?}");
    assert!(!offered.contains(&"l41"), "{offered:?}");
}

/// `..` beneath a file is the system's own "Not a directory", in its words,
/// like every other answer about a path inside.
#[cfg(unix)]
#[test]
fn dot_dot_beneath_a_file_is_in_the_systems_words() {
    let fixture = test_support::Fixture::new("dirs-dotdot-file");
    let (media, _elsewhere) = a_share(&fixture);
    let sent = media.join("Manga/notes.txt/..");

    assert_eq!(
        list(&[media], Some(&sent)),
        Err(Refusal::Unreadable(format!(
            "{}: Not a directory (os error 20)",
            sent.display()
        )))
    );
}

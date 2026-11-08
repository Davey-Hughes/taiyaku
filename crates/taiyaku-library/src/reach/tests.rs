use std::path::PathBuf;

use super::*;
use crate::Library;
use crate::scan::tests::write_pages;
use crate::scan::{InLibrary, Root};

/// A root at `at`, in library `library`, excluding `globs`. Nothing else of the
/// row is read by reach.
fn root(id: i64, at: &str, library: i64, globs: Option<&str>) -> Root {
    Root {
        id,
        path: PathBuf::from(at),
        edition_key: None,
        default_kind: None,
        default_language: None,
        exclude_globs: globs.map(str::to_owned),
        scan_interval_secs: None,
        keeps_documents: false,
        libraries: vec![library],
    }
}

/// Nesting is by component, as `scan::beside` compares: `/mx` shares `/m`'s
/// first two bytes and is not inside it.
#[test]
fn a_nested_root_is_reached_and_a_sibling_sharing_its_prefix_is_not() {
    let roots = [
        root(1, "/m", 70, None),
        root(2, "/m/kids", 80, None),
        root(3, "/mx", 90, None),
    ];
    assert_eq!(reach(&roots, &[1]), vec![1, 2]);
    assert_eq!(reach(&roots, &[2]), vec![2], "nothing is nested in /m/kids");
    assert_eq!(reach(&roots, &[3]), vec![3], "nor in /mx");
}

#[test]
fn a_root_the_outer_s_glob_excludes_is_not_reached() {
    let roots = [
        root(1, "/m", 70, Some("kids/")),
        root(2, "/m/kids", 80, None),
    ];
    assert_eq!(reach(&roots, &[1]), vec![1]);
    assert_eq!(reach(&roots, &[2]), vec![2], "Kids still reaches its own");
}

/// The walk never descends into `private`, so it never reaches the
/// whitelisted `private/kids` beneath it, and neither does reach.
/// `excludes_with_parents` would answer the other way: the line `!private/kids`
/// whitelists the path itself, which the precondition shows.
#[test]
fn a_whitelisted_child_of_an_excluded_directory_is_not_reached() {
    let globs = "private/\n!private/kids";
    let mut builder = ignore::gitignore::GitignoreBuilder::new("");
    for line in globs.lines() {
        builder.add_line(None, line).expect("compiles");
    }
    let gitignore = builder.build().expect("builds");
    assert!(
        !gitignore
            .matched_path_or_any_parents("private/kids", true)
            .is_ignore(),
        "the path itself is whitelisted, so only the top-down question refuses it"
    );
    let roots = [
        root(1, "/m", 70, Some(globs)),
        root(2, "/m/private/kids", 80, None),
    ];
    assert_eq!(reach(&roots, &[1]), vec![1]);
}

/// The walk skips every name starting with `.`, at any depth: the first name
/// below the outer, a name above the nested directory's own, and the nested
/// directory's own name two deep.
#[test]
fn a_dot_named_root_is_not_reached() {
    let roots = [
        root(1, "/m", 70, None),
        root(2, "/m/.kids", 80, None),
        root(3, "/m/.hidden/kids", 90, None),
        root(4, "/m/a/.kids", 60, None),
    ];
    assert_eq!(reach(&roots, &[1]), vec![1]);
}

/// No chaining: `/m/kids/tiny` is asked of `/m` directly, by `/m`'s patterns
/// alone, whatever `/m/kids` excludes.
#[test]
fn a_root_two_deep_is_asked_of_each_outer_root_directly() {
    let outer_excludes = [
        root(1, "/m", 70, Some("kids/tiny")),
        root(2, "/m/kids", 80, None),
        root(3, "/m/kids/tiny", 90, None),
    ];
    assert_eq!(
        reach(&outer_excludes, &[1]),
        vec![1, 2],
        "Manga's own pattern leaves tiny out, though Kids reaches it"
    );
    assert_eq!(reach(&outer_excludes, &[2]), vec![2, 3]);

    let middle_excludes = [
        root(1, "/m", 70, None),
        root(2, "/m/kids", 80, Some("tiny")),
        root(3, "/m/kids/tiny", 90, None),
    ];
    assert_eq!(reach(&middle_excludes, &[2]), vec![2]);
    assert_eq!(
        reach(&middle_excludes, &[1]),
        vec![1, 2, 3],
        "Kids' pattern does not govern Manga's reach"
    );
}

/// Each library's own roots, then reach: Manga's two and the root nested in
/// one of them, and Other's alone, ascending and once each.
#[test]
fn of_libraries_reaches_from_each_librarys_own_roots() {
    let roots = [
        root(7, "/o", 90, None),
        root(5, "/m/kids", 80, None),
        root(3, "/n", 70, None),
        root(2, "/m", 70, None),
    ];
    assert_eq!(of_libraries(&roots, &[70]), vec![2, 3, 5]);
    assert_eq!(of_libraries(&roots, &[80]), vec![5]);
    assert_eq!(of_libraries(&roots, &[90, 70, 80]), vec![2, 3, 5, 7]);
    assert_eq!(of_libraries(&roots, &[60]), Vec::<i64>::new(), "no roots");
}

/// **A directory in two libraries is in each**, read from its links through
/// `scan::roots`: Kids, which holds `/m` beside Manga, shows `/m` and the
/// directory Other holds inside it, as Manga does. Each library's lowest link
/// alone would leave `/m` out of Kids.
#[test]
fn a_directory_linked_to_two_libraries_is_shown_by_each() {
    let library = Library::open_in_memory().expect("in-memory");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'Kids'), (90, 'Other');
         INSERT INTO root (id, path) VALUES (1, '/m'), (2, '/m/kids'), (3, '/o');
         INSERT INTO library_root (library_id, root_id) VALUES (70, 1), (90, 2), (90, 3);",
    )
    .expect("three libraries, numbered apart from their directories");
    crate::test_support::link(&conn, 80, 1);

    let roots = scan::roots(&conn).expect("lists the roots");

    assert_eq!(
        roots
            .iter()
            .map(|root| (root.id, root.libraries.clone()))
            .collect::<Vec<_>>(),
        [(1, vec![70, 80]), (2, vec![90]), (3, vec![90])],
        "each root once, with its links ascending"
    );
    assert_eq!(
        of_libraries(&roots, &[80]),
        vec![1, 2],
        "Kids shows /m and what it holds"
    );
    assert_eq!(of_libraries(&roots, &[70]), vec![1, 2], "and so does Manga");
    assert_eq!(of_libraries(&roots, &[90]), vec![2, 3]);
}

/// A scan of Manga walks `/m` before the directory nested in it, whatever
/// their ids, and leaves out a sibling sharing its prefix; Kids walks its own;
/// with `/m` excluding `kids/`, Manga walks `/m` alone.
#[test]
fn a_librarys_walk_is_what_it_reaches_by_path() {
    let roots = [
        root(1, "/m/kids", 80, None),
        root(2, "/mx", 90, None),
        root(3, "/m", 70, None),
    ];
    let walked = |roots: &[Root], library: i64| -> Vec<i64> {
        walked_by(roots, library)
            .into_iter()
            .map(|root| root.id)
            .collect()
    };
    assert_eq!(walked(&roots, 70), vec![3, 1]);
    assert_eq!(walked(&roots, 80), vec![1]);
    let excluding = [
        root(1, "/m/kids", 80, None),
        root(3, "/m", 70, Some("kids/")),
    ];
    assert_eq!(walked(&excluding, 70), vec![3]);
}

#[test]
fn reach_is_ascending_and_deduplicated() {
    let roots = [
        root(9, "/m/kids/tiny", 90, None),
        root(5, "/m/kids", 80, None),
        root(2, "/m", 70, None),
        root(1, "/other", 60, None),
    ];
    assert_eq!(
        reach(&roots, &[5, 2, 5]),
        vec![2, 5, 9],
        "tiny is reached through both, and named once"
    );
}

/// What [`nested_libraries`] builds.
pub(crate) struct Nested {
    pub(crate) library: Library,
    /// Library `Manga`, numbered 70, holding `m`.
    pub(crate) manga: i64,
    /// Library `Kids`, numbered 80, holding `m/kids`.
    pub(crate) kids: i64,
    /// The root ids of `m`, `m/kids` and, when asked for, `m/kids/tiny`, which
    /// library `Tiny` (numbered 90) holds.
    pub(crate) m: i64,
    pub(crate) m_kids: i64,
    pub(crate) m_kids_tiny: Option<i64>,
    /// An account granted Manga alone, and one granted Kids alone.
    pub(crate) manga_reader: i64,
    pub(crate) kids_reader: i64,
    _dir: crate::tests::TempDir,
}

/// Manga (70) holds `m`, Kids (80) `m/kids` and, with `tiny`, Tiny (90)
/// `m/kids/tiny`, added through the checked add, one book in each: `Manga Book`,
/// `Kids Book`, `Tiny Book`. Library ids sit apart from directory ids.
///
/// **Each book is held by its own directory alone**, written after the
/// directories around it were scanned, as the outer walk will leave it once it
/// skips nested directories. Checked here: a book an outer walk also held would
/// be shown by the outer library without reach.
pub(crate) fn nested_libraries(tiny: bool) -> Nested {
    let dir = crate::tests::tempdir("reach-nested");
    let at = dir.path().join("m");
    write_pages(&at.join("Manga Book").join("Manga Book - v01"), 2);
    let library = Library::open_in_memory().expect("in-memory");
    let (manga_reader, kids_reader, m, m_kids, m_kids_tiny) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'Manga'), (80, 'Kids'), (90, 'Tiny')",
            [],
        )
        .expect("three libraries, numbered apart from their directories");
        let add = |path: &std::path::Path, name: &str| -> i64 {
            let added = scan::add_root_checked_in(&conn, path, InLibrary::Named(name))
                .unwrap_or_else(|e| panic!("adds {name}'s directory: {e:?}"));
            scan::scan_root(&conn, &added.root)
                .unwrap_or_else(|e| panic!("scans {name}'s directory: {e}"));
            added.root.id
        };
        let m = add(&at, "Manga");
        write_pages(
            &at.join("kids").join("Kids Book").join("Kids Book - v01"),
            2,
        );
        let m_kids = add(&at.join("kids"), "Kids");
        let m_kids_tiny = tiny.then(|| {
            let tiny = at.join("kids").join("tiny");
            write_pages(&tiny.join("Tiny Book").join("Tiny Book - v01"), 2);
            add(&tiny, "Tiny")
        });
        crate::derive::derive(&conn).expect("derives");

        let held: Vec<(i64, i64)> = conn
            .prepare("SELECT root_id, COUNT(*) FROM member_root GROUP BY root_id ORDER BY root_id")
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect()
            })
            .expect("reads who holds what");
        let mut expected = vec![(m, 1), (m_kids, 1)];
        expected.extend(m_kids_tiny.map(|id| (id, 1)));
        assert_eq!(held, expected, "each directory holds its own book alone");

        let reader = |name: &str, library: i64| -> i64 {
            conn.execute(
                "INSERT INTO user (name, password_hash, role) VALUES (?1, '!', 'user')",
                [name],
            )
            .expect("adds a reader");
            let id = conn.last_insert_rowid();
            crate::account::grant_library(&conn, id, library).expect("grants a library");
            id
        };
        (
            reader("manga", 70),
            reader("kids", 80),
            m,
            m_kids,
            m_kids_tiny,
        )
    };
    Nested {
        library,
        manga: 70,
        kids: 80,
        m,
        m_kids,
        m_kids_tiny,
        manga_reader,
        kids_reader,
        _dir: dir,
    }
}

use rusqlite::Connection;

use super::*;
use crate::Library;
use crate::scan::{InLibrary, add_root, add_root_in};

/// Seed a work with one member per `(edition key, root)`, a root of `None`
/// being a member under no directory, as an imported work's are.
fn seed_work(conn: &Connection, key: &str, members: &[(&str, Option<i64>)]) {
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub)
         VALUES (?1, ?1, ?1, 'rtl', 'raw')",
        [key],
    )
    .expect("a work");
    let work = conn.last_insert_rowid();
    for (ordinal, (edition, root)) in (0_i64..).zip(members) {
        conn.execute(
            "INSERT OR IGNORE INTO edition (work_id, key, label) VALUES (?1, ?2, ?2)",
            rusqlite::params![work, edition],
        )
        .expect("an edition");
        let edition: i64 = conn
            .query_row(
                "SELECT id FROM edition WHERE work_id = ?1 AND key = ?2",
                rusqlite::params![work, edition],
                |r| r.get(0),
            )
            .expect("the edition");
        conn.execute(
            "INSERT INTO member (edition_id, ordinal, path, sha256, pages, root_id)
             VALUES (?1, ?2, ?3, ?3, 1, ?4)",
            rusqlite::params![edition, ordinal, format!("{key}-{ordinal}"), root],
        )
        .expect("a member");
    }
}

/// Manga is made first and Comics last, so the order by name is not also the
/// order by id. A library counts each work with a member under one of its
/// directories, once however many it has: `split` has an edition under each
/// library's, `held` is under Manga's alone, `twice` has two members under
/// one directory, and `imported` is under none.
#[test]
fn listing_names_each_library_with_its_dirs_and_works() {
    let parent = crate::tests::tempdir("libraries-list");
    let dirs: Vec<_> = ["Volumes", "Chapters", "Comics"]
        .iter()
        .map(|name| {
            let dir = parent.path().join(name);
            std::fs::create_dir_all(&dir).expect("mkdir");
            dir.canonicalize().expect("canonical")
        })
        .collect();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = add_root_in(&conn, &dirs[0], InLibrary::Named("Manga")).expect("Manga");
    let chapters = add_root_in(&conn, &dirs[1], InLibrary::Named("Manga")).expect("Manga again");
    let comics = add_root(&conn, &dirs[2]).expect("Comics, a library of its own");
    seed_work(
        &conn,
        "split",
        &[("raw", Some(chapters.id)), ("en", Some(comics.id))],
    );
    seed_work(&conn, "held", &[("raw", Some(volumes.id))]);
    seed_work(
        &conn,
        "twice",
        &[("raw", Some(volumes.id)), ("raw", Some(volumes.id))],
    );
    seed_work(&conn, "imported", &[("raw", None)]);

    let listed = list(&conn).expect("lists");

    assert_eq!(
        listed
            .iter()
            .map(|l| (l.name.as_str(), l.dirs.clone(), l.works))
            .collect::<Vec<_>>(),
        vec![
            ("Comics", vec![dirs[2].clone()], 1),
            ("Manga", vec![dirs[0].clone(), dirs[1].clone()], 3),
        ],
        "by name, each directory once, oldest first, and the works under each library's directories"
    );
}

#[test]
fn a_library_needs_a_name_and_one_of_its_own() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    assert!(create(&conn, "   ").unwrap_err().contains("needs a name"));
    assert!(
        create(&conn, "\t").unwrap_err().contains("needs a name"),
        "a name of only whitespace is no name"
    );
    create(&conn, "Manga").expect("creates");
    assert!(create(&conn, " Manga").unwrap_err().contains("UNIQUE"));
}

/// Every white-space character Unicode names, at either end, and nothing
/// inside a name or outside that set.
#[test]
fn trim_name_strips_white_space_and_nothing_else() {
    assert_eq!(trim_name("\t\u{3000}Manga\u{a0}\n "), "Manga");
    assert_eq!(
        trim_name("\u{200B}Manga"),
        "\u{200B}Manga",
        "a zero-width space is not white space"
    );
    assert_eq!(trim_name("Light novels"), "Light novels");
}

/// Every list migration 16 strips with is the set [`trim_name`] strips, so a
/// name its `UPDATE`s stored is found by a lookup made in Rust.
#[test]
fn migration_16_strips_the_characters_trim_name_strips() {
    let rust: Vec<u32> = (0..=0x10_FFFF_u32)
        .filter_map(char::from_u32)
        .filter(|c| c.is_whitespace())
        .map(u32::from)
        .collect();
    let sql = include_str!("../schema/016_library_names.sql");
    let lists: Vec<Vec<u32>> = sql
        .match_indices("char(")
        .map(|(at, _)| {
            let rest = &sql[at + "char(".len()..];
            rest[..rest.find(')').expect("a closing parenthesis")]
                .split(',')
                .map(|n| n.trim().parse().expect("a code point"))
                .collect()
        })
        .collect();
    assert!(!lists.is_empty(), "the migration strips with a list");
    for list in lists {
        assert_eq!(list, rust);
    }
}

/// A name is found stripped and in any ASCII case.
#[test]
fn a_name_finds_its_library_stripped_and_in_any_case() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (1, 'Manga'), (3, 'Comics');")
        .expect("two libraries");

    let found = |name: &str| id_of(&conn, name).expect("reads");
    assert_eq!(found("Manga"), Some(1));
    assert_eq!(found(" MANGA\t"), Some(1));
    assert_eq!(found("\u{3000}comics\n"), Some(3));
    assert_eq!(found("Zines"), None);
}

/// Another library's name in another case is refused in the holder's own
/// spelling; a library's own name in another case is a rename.
#[test]
fn a_rename_is_refused_a_name_another_library_holds_in_another_case() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (1, 'Manga'), (2, 'Comics');")
        .expect("two libraries");

    assert_eq!(
        rename(&conn, 2, "\tMANGA\n"),
        Err(RenameError::Unusable(
            "a library is already called `Manga`".to_string()
        ))
    );
    assert_eq!(rename(&conn, 1, " MANGA "), Ok("MANGA".to_string()));
    assert_eq!(
        rename(&conn, 2, "\u{3000}Light novels\n"),
        Ok("Light novels".to_string())
    );
}

/// A work is in no library where no member of it is under a directory:
/// `imported` and `imported too` are held under none, and the three held
/// under the directory, one of them also by a member under none, are in its
/// library.
#[test]
fn unfiled_works_counts_only_works_in_no_library() {
    let parent = crate::tests::tempdir("libraries-unfiled");
    let dir = parent.path().join("Manga");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root_in(&conn, &dir, InLibrary::Named("Manga")).expect("Manga");
    seed_work(&conn, "imported", &[("raw", None)]);
    seed_work(&conn, "imported too", &[("raw", None)]);
    seed_work(&conn, "held", &[("raw", Some(root.id))]);
    seed_work(&conn, "held too", &[("raw", Some(root.id))]);
    seed_work(&conn, "partly", &[("raw", None), ("en", Some(root.id))]);
    assert_eq!(unfiled_works(&conn).expect("counts"), 2);
}

#[test]
fn a_library_is_named_by_its_id() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    assert_eq!(name_of(&conn, id).expect("reads").as_deref(), Some("Manga"));
    assert_eq!(name_of(&conn, id + 1).expect("reads"), None);
}

#[test]
fn a_rename_stores_the_name_stripped_and_answers_it() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    let other = create(&conn, "Books").expect("creates");
    assert_eq!(rename(&conn, id, "  Comics ").expect("renames"), "Comics");
    assert_eq!(
        name_of(&conn, other).expect("reads").as_deref(),
        Some("Books"),
        "another library keeps its name"
    );
    assert_eq!(
        name_of(&conn, id).expect("reads").as_deref(),
        Some("Comics")
    );
    assert_eq!(
        rename(&conn, id, "Comics").expect("its own name is not taken"),
        "Comics"
    );
}

#[test]
fn a_rename_is_refused_as_the_callers_mistake_or_as_no_such_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = create(&conn, "Manga").expect("creates");
    create(&conn, "Comics").expect("creates");

    assert!(matches!(
        rename(&conn, manga, " \t"),
        Err(RenameError::Unusable(why)) if why.contains("needs a name")
    ));
    assert!(matches!(
        rename(&conn, manga, "Comics"),
        Err(RenameError::Unusable(why)) if why.contains("already called `Comics`")
    ));
    assert_eq!(
        name_of(&conn, manga).expect("reads").as_deref(),
        Some("Manga"),
        "a refused rename wrote nothing"
    );
    assert_eq!(rename(&conn, 999, "Other"), Err(RenameError::Missing(999)));
}

/// Every library's name, by name.
fn names(conn: &Connection) -> Vec<String> {
    list(conn)
        .expect("lists")
        .into_iter()
        .map(|l| l.name)
        .collect()
}

/// [`crate::has_no_address`]: no page address holds `.`. It is asked of the
/// name as it would be stored, so white space around the dot is no way past.
#[test]
fn a_library_is_not_made_called_one_dot() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in [".", " .\t"] {
        assert_eq!(
            create(&conn, name),
            Err(crate::NO_ADDRESS.to_string()),
            "{name:?}"
        );
    }
    assert_eq!(names(&conn), Vec::<String>::new(), "nothing was made");
}

#[test]
fn a_library_is_not_made_called_two_dots() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in ["..", "\u{3000}.. "] {
        assert_eq!(
            create(&conn, name),
            Err(crate::NO_ADDRESS.to_string()),
            "{name:?}"
        );
    }
    assert_eq!(names(&conn), Vec::<String>::new(), "nothing was made");
}

#[test]
fn a_library_is_not_renamed_to_one_dot() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    for name in [".", " . "] {
        assert_eq!(
            rename(&conn, id, name),
            Err(RenameError::Unusable(crate::NO_ADDRESS.to_string())),
            "{name:?}"
        );
    }
    assert_eq!(names(&conn), ["Manga"], "a refused rename wrote nothing");
}

#[test]
fn a_library_is_not_renamed_to_two_dots() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    for name in ["..", "\t..\n"] {
        assert_eq!(
            rename(&conn, id, name),
            Err(RenameError::Unusable(crate::NO_ADDRESS.to_string())),
            "{name:?}"
        );
    }
    assert_eq!(names(&conn), ["Manga"], "a refused rename wrote nothing");
}

/// Only `.` and `..`: a name holding dots beside anything else is an ordinary
/// segment of an address.
#[test]
fn a_library_is_made_and_renamed_with_any_other_name_of_dots() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in ["...", ".a", "a.b", "..a"] {
        create(&conn, name).expect(name);
    }
    let id = create(&conn, "Manga").expect("creates");
    for name in ["....", "a.", ". ."] {
        assert_eq!(rename(&conn, id, name), Ok(name.to_string()));
    }
    assert_eq!(names(&conn), [". .", "...", "..a", ".a", "a.b"]);
}

/// Only the UNIQUE constraint is the caller's mistake: any other failure of
/// the write is the store's, and is reported in the store's words.
#[test]
fn a_rename_that_fails_for_another_reason_is_the_stores_fault() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    conn.execute_batch(
        "CREATE TEMP TRIGGER planted BEFORE UPDATE ON main.library
         BEGIN SELECT RAISE(ABORT, 'planted'); END",
    )
    .expect("plants the trigger");

    match rename(&conn, id, "Comics") {
        Err(RenameError::Failed(why)) => assert!(why.contains("planted"), "{why}"),
        other => panic!("expected the store's failure, got {other:?}"),
    }
    conn.execute_batch("DROP TRIGGER planted")
        .expect("drops it");
    assert_eq!(
        name_of(&conn, id).expect("reads").as_deref(),
        Some("Manga"),
        "a failed rename wrote nothing"
    );
}

/// Two libraries, so a count that forgot the library reads the other's rows.
#[test]
fn books_counts_the_librarys_own_epub_and_pdf_entries() {
    let parent = crate::tests::tempdir("libraries-books");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let mut roots = Vec::new();
    for name in ["Books", "Manga"] {
        let dir = parent.path().join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        roots.push(add_root_in(&conn, &dir, InLibrary::Named(name)).expect("adds"));
    }
    for (root, rel_path, format) in [
        (&roots[0], "a.epub", Some("epub-reflow")),
        (&roots[0], "b.epub", Some("epub-fixed")),
        (&roots[0], "c.pdf", Some("pdf")),
        (&roots[0], "d.cbz", Some("zip")),
        (&roots[0], "e.bin", None),
        (&roots[1], "f.pdf", Some("pdf")),
    ] {
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             VALUES (?1, ?2, 'file', ?3, 1, 1)",
            rusqlite::params![root.id, rel_path, format],
        )
        .expect("a scanned entry");
    }

    assert_eq!(books(&conn, roots[0].libraries[0]).expect("counts"), 3);
    assert_eq!(books(&conn, roots[1].libraries[0]).expect("counts"), 1);
}

/// A copy held under a directory of each of two libraries is counted in both,
/// and in no library's absence.
#[test]
fn a_copy_held_in_two_libraries_is_counted_in_both() {
    let copy = crate::tests::a_volume_copied_into_two_libraries();
    let conn = copy.conn();
    let counted: Vec<(String, usize)> = list(&conn)
        .expect("lists")
        .into_iter()
        .map(|l| (l.name, l.works))
        .collect();
    assert_eq!(
        counted,
        [("A".to_string(), 1), ("B".to_string(), 1)],
        "the one work, in each library holding a copy"
    );
    assert_eq!(unfiled_works(&conn).expect("counts"), 0);
}

/// A member whose primary directory went with no removal around it has its
/// `root_id` `NULL` while a copy is still recorded under another directory,
/// and is in that directory's library.
#[test]
fn a_member_recorded_under_a_copy_alone_is_in_that_copys_library() {
    let copy = crate::tests::a_volume_copied_into_two_libraries();
    let conn = copy.conn();
    conn.execute("DELETE FROM root WHERE id = ?1", [copy.b.id])
        .expect("b's directory goes with no removal around it");
    assert_eq!(
        crate::tests::member_root_ids(&conn),
        [copy.a.id],
        "the fixture: the member is recorded under a's copy alone"
    );
    assert_eq!(unfiled_works(&conn).expect("counts"), 0);
    let counted: Vec<(String, usize)> = list(&conn)
        .expect("lists")
        .into_iter()
        .map(|l| (l.name, l.works))
        .collect();
    assert_eq!(counted, [("A".to_string(), 1)], "B went with its directory");
}

fn set_pages(conn: &Connection, path: &str, pages: i64) {
    let set = conn
        .execute(
            "UPDATE member SET pages = ?2 WHERE path = ?1",
            rusqlite::params![path, pages],
        )
        .expect("sets a member's pages");
    assert_eq!(set, 1, "one member at {path}");
}

fn held_under(root: &crate::scan::Root, library: i64) -> MemberDirectory {
    MemberDirectory {
        root: root.id,
        path: root.path.display().to_string(),
        libraries: vec![library],
    }
}

/// Manga holds Volumes and Chapters, Comics its own directory. Manga is made
/// first and `zeta` seeded before `alpha`, so neither name nor title order is
/// id order. `zeta`'s `raw` and `de` and `alpha`'s `raw` have members in both
/// libraries; `zeta`'s `raw` is renumbered against its insert and path order,
/// `zeta-0` has a copy under Comics and `alpha-1` one under a second Manga
/// directory. `zeta`'s `en` and `whole` are under Manga alone, `imported`
/// under none.
#[test]
fn split_editions_are_those_a_library_holds_part_of_and_not_all() {
    let parent = crate::tests::tempdir("libraries-split");
    let dirs: Vec<_> = ["Volumes", "Chapters", "Comics"]
        .iter()
        .map(|name| {
            let dir = parent.path().join(name);
            std::fs::create_dir_all(&dir).expect("mkdir");
            dir.canonicalize().expect("canonical")
        })
        .collect();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes = add_root_in(&conn, &dirs[0], InLibrary::Named("Manga")).expect("Manga");
    let chapters = add_root_in(&conn, &dirs[1], InLibrary::Named("Manga")).expect("Manga again");
    let comics = add_root_in(&conn, &dirs[2], InLibrary::Named("Comics")).expect("Comics");
    let manga = id_of(&conn, "Manga").expect("reads").expect("Manga");
    let comic = id_of(&conn, "Comics").expect("reads").expect("Comics");
    assert!(manga < comic, "the fixture: Manga is made first");
    seed_work(
        &conn,
        "zeta",
        &[
            ("raw", Some(volumes.id)),
            ("raw", Some(comics.id)),
            ("raw", Some(chapters.id)),
            ("en", Some(volumes.id)),
            ("de", Some(comics.id)),
            ("de", Some(chapters.id)),
        ],
    );
    seed_work(
        &conn,
        "alpha",
        &[("raw", Some(comics.id)), ("raw", Some(chapters.id))],
    );
    seed_work(
        &conn,
        "whole",
        &[("raw", Some(volumes.id)), ("raw", Some(chapters.id))],
    );
    seed_work(&conn, "imported", &[("raw", None), ("raw", None)]);
    // Before `alpha` in byte order, and after it in the UI's.
    conn.execute("UPDATE work SET title = 'Zeta' WHERE slug = 'zeta'", [])
        .expect("titles zeta");
    conn.execute(
        "INSERT INTO member_root (member_id, root_id)
         SELECT id, ?1 FROM member WHERE path = 'zeta-0'",
        [comics.id],
    )
    .expect("a copy of zeta-0 under Comics");
    conn.execute(
        "INSERT INTO member_root (member_id, root_id)
         SELECT id, ?1 FROM member WHERE path = 'alpha-1'",
        [volumes.id],
    )
    .expect("a copy of alpha-1 under Volumes");
    for (path, pages) in [
        ("zeta-0", 2),
        ("zeta-1", 3),
        ("zeta-2", 4),
        ("zeta-3", 5),
        ("zeta-4", 8),
        ("zeta-5", 9),
        ("alpha-0", 6),
        ("alpha-1", 7),
    ] {
        set_pages(&conn, path, pages);
    }
    // Moved clear first, as `(edition_id, ordinal)` is unique.
    conn.execute_batch(
        "UPDATE member SET ordinal = ordinal + 10
          WHERE path IN ('zeta-0', 'zeta-1', 'zeta-2');
         UPDATE member SET ordinal = CASE path
           WHEN 'zeta-2' THEN 0 WHEN 'zeta-0' THEN 1 WHEN 'zeta-1' THEN 2 END
          WHERE path IN ('zeta-0', 'zeta-1', 'zeta-2');",
    )
    .expect("renumbers zeta's raw");
    let renumbered: Vec<String> = conn
        .prepare(
            "SELECT m.path FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE m.path LIKE 'zeta-%' AND e.key = 'raw' ORDER BY m.ordinal",
        )
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .expect("reads zeta's raw");
    assert_eq!(
        renumbered,
        ["zeta-2", "zeta-0", "zeta-1"],
        "the fixture: zeta's raw in an order neither its ids nor its paths give"
    );
    assert!(
        comics.path < volumes.path && chapters.path < volumes.path,
        "the fixture: by path, Comics and Chapters come before Volumes, made before them"
    );

    let member = |path: &str, pages: i64, directories: Vec<MemberDirectory>| SplitMember {
        path: path.to_string(),
        pages,
        directories,
    };
    let held = |id: i64, name: &str, members: usize, pages: i64| SplitLibrary {
        id,
        name: name.to_string(),
        members,
        pages,
    };
    assert_eq!(
        split_editions(&conn).expect("lists"),
        [
            SplitEdition {
                work: "alpha".to_string(),
                title: "alpha".to_string(),
                edition: "raw".to_string(),
                pages: 13,
                members: vec![
                    member("alpha-0", 6, vec![held_under(&comics, comic)]),
                    member(
                        "alpha-1",
                        7,
                        vec![held_under(&chapters, manga), held_under(&volumes, manga)],
                    ),
                ],
                libraries: vec![held(comic, "Comics", 1, 6), held(manga, "Manga", 1, 7)],
            },
            SplitEdition {
                work: "zeta".to_string(),
                title: "Zeta".to_string(),
                edition: "de".to_string(),
                pages: 17,
                members: vec![
                    member("zeta-4", 8, vec![held_under(&comics, comic)]),
                    member("zeta-5", 9, vec![held_under(&chapters, manga)]),
                ],
                libraries: vec![held(comic, "Comics", 1, 8), held(manga, "Manga", 1, 9)],
            },
            SplitEdition {
                work: "zeta".to_string(),
                title: "Zeta".to_string(),
                edition: "raw".to_string(),
                pages: 9,
                members: vec![
                    member("zeta-2", 4, vec![held_under(&chapters, manga)]),
                    member(
                        "zeta-0",
                        2,
                        vec![held_under(&comics, comic), held_under(&volumes, manga)],
                    ),
                    member("zeta-1", 3, vec![held_under(&comics, comic)]),
                ],
                libraries: vec![held(comic, "Comics", 2, 5), held(manga, "Manga", 2, 6)],
            },
        ]
    );
}

/// A byte-identical copy in two libraries is one member held under both, so
/// each library holds all of its edition: nothing is split.
#[test]
fn a_copy_held_in_two_libraries_is_not_split() {
    let copy = crate::tests::a_volume_copied_into_two_libraries();
    let conn = copy.conn();
    let members: i64 = conn
        .query_row("SELECT count(*) FROM member", [], |r| r.get(0))
        .expect("counts the members");
    let held: Vec<String> = conn
        .prepare(
            "SELECT l.name FROM member_root mr JOIN library_root r ON r.root_id = mr.root_id
               JOIN library l ON l.id = r.library_id ORDER BY l.name",
        )
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .expect("reads where the members are held");
    assert_eq!(
        (members, held),
        (1, vec!["A".to_string(), "B".to_string()]),
        "the fixture: a copy, one member held under a directory of each library"
    );
    assert_eq!(split_editions(&conn).expect("lists"), []);
}

/// A store with no directory reaches none, so nothing is split, and the
/// answer is an empty list rather than SQL with an empty `VALUES`.
#[test]
fn a_store_with_no_directory_splits_no_edition() {
    let library = Library::open_in_memory().expect("open");
    assert_eq!(split_editions(&library.conn()).expect("lists"), []);
}

/// Spec §4's N, Manga holding `m` and Kids `m/kids`, a book in each: Manga
/// counts Kids' work beside its own, Kids its own alone, and each lists its own
/// directory alone.
#[test]
fn a_library_counts_the_works_of_the_directories_it_reaches() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    let counted: Vec<(String, usize, usize)> = list(&conn)
        .expect("lists")
        .into_iter()
        .map(|l| (l.name, l.dirs.len(), l.works))
        .collect();
    assert_eq!(
        counted,
        [
            ("Kids".to_string(), 1, 1),
            ("Manga".to_string(), 1, 2),
            ("Tiny".to_string(), 0, 0),
        ]
    );
}

/// Manga's own directory holds no EPUB or PDF entry; the PDF under Kids'
/// directory nested in it is counted for both, so Manga is not "no readable
/// files yet".
#[test]
fn books_counts_the_entries_of_the_directories_a_library_reaches() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    conn.execute(
        "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
         VALUES (?1, 'k.pdf', 'file', 'pdf', 1, 1)",
        [n.m_kids],
    )
    .expect("a PDF under Kids' directory");

    assert_eq!(books(&conn, n.manga).expect("counts"), 1);
    assert_eq!(books(&conn, n.kids).expect("counts"), 1);
}

/// Spec §4's N, each library by the directories it reaches. `nested`'s `raw`
/// has `nested-0` under `m` and `nested-1` under Kids' `m/kids`: whole for
/// Manga, which reaches both, and split for Kids. `copied`'s `raw` has
/// `copied-0` under `m` with a copy under `m/kids`, and `copied-1` under
/// `m/kids`: whole for both, so not a case, though Manga's own directory
/// holds only the first.
#[test]
fn a_library_holds_the_members_under_the_directories_it_reaches() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    seed_work(
        &conn,
        "nested",
        &[("raw", Some(n.m)), ("raw", Some(n.m_kids))],
    );
    seed_work(
        &conn,
        "copied",
        &[("raw", Some(n.m)), ("raw", Some(n.m_kids))],
    );
    conn.execute(
        "INSERT INTO member_root (member_id, root_id)
         SELECT id, ?1 FROM member WHERE path = 'copied-0'",
        [n.m_kids],
    )
    .expect("a copy of copied-0 under m/kids");
    set_pages(&conn, "nested-0", 2);
    set_pages(&conn, "nested-1", 3);
    let roots = crate::scan::roots(&conn).expect("roots");
    let dir = |id: i64| roots.iter().find(|root| root.id == id).expect("a root");

    assert_eq!(
        split_editions(&conn).expect("lists"),
        [SplitEdition {
            work: "nested".to_string(),
            title: "nested".to_string(),
            edition: "raw".to_string(),
            pages: 5,
            members: vec![
                SplitMember {
                    path: "nested-0".to_string(),
                    pages: 2,
                    directories: vec![held_under(dir(n.m), n.manga)],
                },
                SplitMember {
                    path: "nested-1".to_string(),
                    pages: 3,
                    directories: vec![held_under(dir(n.m_kids), n.kids)],
                },
            ],
            libraries: vec![
                SplitLibrary {
                    id: n.kids,
                    name: "Kids".to_string(),
                    members: 1,
                    pages: 3,
                },
                SplitLibrary {
                    id: n.manga,
                    name: "Manga".to_string(),
                    members: 2,
                    pages: 5,
                },
            ],
        }]
    );
}

/// A member's directory in two libraries names both, ascending: Kids' `m/kids`
/// linked into Tiny as well, through the add.
#[test]
fn a_member_directory_names_every_library_it_is_in() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    seed_work(
        &conn,
        "nested",
        &[("raw", Some(n.m)), ("raw", Some(n.m_kids))],
    );
    let roots = crate::scan::roots(&conn).expect("roots");
    let m_kids = roots
        .iter()
        .find(|root| root.id == n.m_kids)
        .expect("m/kids")
        .clone();
    let tiny = id_of(&conn, "Tiny").expect("reads").expect("Tiny");
    let linked = crate::scan::add_root_checked_in(&conn, &m_kids.path, InLibrary::Id(tiny))
        .expect("links m/kids into Tiny");
    assert_eq!(linked.also_in, ["Kids"], "the fixture: a link");

    let split = split_editions(&conn).expect("lists");

    let directories: Vec<&[MemberDirectory]> = split
        .iter()
        .flat_map(|edition| &edition.members)
        .map(|member| member.directories.as_slice())
        .collect();
    assert_eq!(
        directories,
        [
            &[held_under(&roots[0], n.manga)][..],
            &[MemberDirectory {
                root: n.m_kids,
                path: m_kids.path.display().to_string(),
                libraries: vec![n.kids, tiny],
            }][..],
        ],
        "{split:?}"
    );
}

#[test]
fn names_are_listed_as_a_sentence_lists_them() {
    let none: [&str; 0] = [];
    assert_eq!(listed("Manga", &none), "Manga");
    assert_eq!(listed("Manga", &["Kids"]), "Manga and Kids");
    assert_eq!(
        listed("Manga", &["Kids", "Comics"]),
        "Manga, Kids and Comics"
    );
}

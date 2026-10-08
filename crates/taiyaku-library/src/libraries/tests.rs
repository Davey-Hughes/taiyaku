use super::*;
use crate::Library;
use crate::scan::{InLibrary, add_root, add_root_in};

/// Manga is made first and Comics last, so the order by name is not also the
/// order by id.
#[test]
fn listing_names_each_library_with_its_folders_and_works() {
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
    add_root_in(&conn, &dirs[0], InLibrary::Named("Manga")).expect("Manga");
    add_root_in(&conn, &dirs[1], InLibrary::Named("Manga")).expect("Manga again");
    let comics = add_root(&conn, &dirs[2]).expect("Comics, a library of its own");
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, library_id)
         VALUES ('k', 's', 'T', 'rtl', 'raw', ?1)",
        [comics.library_id],
    )
    .expect("one work in Comics");

    let listed = list(&conn).expect("lists");

    assert_eq!(
        listed
            .iter()
            .map(|l| (l.name.as_str(), l.folders.clone(), l.works))
            .collect::<Vec<_>>(),
        vec![
            ("Comics", vec![dirs[2].clone()], 1),
            ("Manga", vec![dirs[0].clone(), dirs[1].clone()], 0),
        ],
        "by name, each folder once, oldest first, and each library's own works"
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
/// name the trigger makes is found by a lookup made in Rust.
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

#[test]
fn unfiled_works_counts_only_works_in_no_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    conn.execute_batch(&format!(
        "INSERT INTO work (content_key, slug, title, reading, hub, library_id)
         VALUES ('a', 'a', 'A', 'rtl', 'raw', {id}), ('b', 'b', 'B', 'rtl', 'raw', NULL),
                ('c', 'c', 'C', 'rtl', 'raw', NULL);"
    ))
    .expect("works");
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

    assert_eq!(books(&conn, roots[0].library_id).expect("counts"), 3);
    assert_eq!(books(&conn, roots[1].library_id).expect("counts"), 1);
}

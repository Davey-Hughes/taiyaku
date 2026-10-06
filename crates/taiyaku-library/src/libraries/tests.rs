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

/// A name is looked up as given first, and stripped of spaces second. Two
/// libraries can differ only in a trailing space, as migration 15's backfill
/// leaves folders whose whole paths differ only so.
#[test]
fn a_name_finds_the_library_holding_exactly_it_before_the_one_holding_it_stripped() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (1, '/srv/manga'), (2, '/srv/manga '),
                                               (3, 'Comics');",
    )
    .expect("two libraries a space apart, and a third");

    let found = |name: &str| id_of(&conn, name).expect("reads");
    assert_eq!(found("/srv/manga "), Some(2), "its own name, space and all");
    assert_eq!(found("/srv/manga"), Some(1));
    assert_eq!(
        found("/srv/manga  "),
        Some(1),
        "no library holds two spaces, so the stripped name answers"
    );
    assert_eq!(found(" Comics "), Some(3), "a space too many either side");
    assert_eq!(found("Zines"), None);
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

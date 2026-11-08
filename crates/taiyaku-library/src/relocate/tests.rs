use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;

use super::*;
use crate::scan::tests::write_pages;
use crate::scan::{self, InLibrary, Root, RootPatch};
use crate::tests::tempdir;
use crate::{Library, derive};

/// The one integer `sql` answers with: a count, or an id where the fixture
/// holds one row.
pub(super) fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

/// A directory at `dir` holding each `(series, volume, pages)`, filed under
/// `library`, with `edition` as its edition key where one is given.
pub(super) fn a_dir(
    conn: &Connection,
    dir: &Path,
    library: &str,
    edition: Option<&str>,
    volumes: &[(&str, &str, usize)],
) -> Root {
    for (series, volume, pages) in volumes {
        write_pages(
            &dir.join(series).join(format!("{series} - {volume}")),
            *pages,
        );
    }
    let root = scan::add_root_in(conn, dir, InLibrary::Named(library)).expect("adds");
    if let Some(key) = edition {
        scan::update_root(
            conn,
            root.id,
            &RootPatch {
                edition_key: Some(Some(key.to_string())),
                ..Default::default()
            },
        )
        .expect("sets the edition key");
    }
    root
}

pub(super) fn scanned(conn: &Connection) {
    for root in scan::roots(conn).expect("lists") {
        scan::scan_root(conn, &root).expect("scans");
    }
    derive::derive(conn).expect("derives");
}

pub(super) fn library_called(conn: &Connection, name: &str) -> i64 {
    crate::libraries::id_of(conn, name)
        .expect("reads")
        .expect("the library exists")
}

/// The text rows `sql` answers, each statement's in its own order.
fn lines(conn: &Connection, sql: &[&str]) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in sql {
        let mut statement = conn.prepare(sql).expect(sql);
        rows.extend(
            statement
                .query_map([], |r| r.get::<_, String>(0))
                .expect(sql)
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect(sql),
        );
    }
    rows
}

/// Every row a move must leave as it was, ids included: a move derives
/// nothing, so an edition or a member written again would show as a new id.
pub(super) fn kept(conn: &Connection) -> Vec<String> {
    lines(
        conn,
        &[
            "SELECT 'series ' || id || ' ' || sort_title FROM series ORDER BY id",
            "SELECT 'series_title ' || series_id || ' ' || title FROM series_title ORDER BY 1",
            "SELECT 'work ' || id || ' ' || slug || ' ' || coalesce(series_id, 'none') || ' '
                    || content_key || ' ' || hub || ' ' || title || ' ' || coalesce(title_hand, 'none')
               FROM work ORDER BY id",
            "SELECT 'edition ' || id || ' ' || work_id || ' ' || key FROM edition ORDER BY id",
            "SELECT 'member ' || id || ' ' || edition_id || ' ' || ordinal || ' ' || path || ' '
                    || coalesce(root_id, 'none')
               FROM member ORDER BY id",
            "SELECT 'progress ' || user_id || ' ' || work_id || ' ' || edition || ' ' || slot
               FROM progress ORDER BY 1",
            "SELECT 'note ' || user_id || ' ' || work_id || ' ' || edition || ' ' || id
               FROM note ORDER BY 1",
        ],
    )
}

/// The whole store a move or a rehearsal may touch, as text, for "the store
/// is as it was": [`kept`], and the libraries, directories and grants a move
/// changes.
pub(super) fn dump(conn: &Connection) -> Vec<String> {
    let mut rows = kept(conn);
    rows.extend(lines(
        conn,
        &[
            "SELECT 'library ' || id || ' ' || name FROM library ORDER BY id",
            "SELECT 'root ' || id FROM root ORDER BY id",
            "SELECT 'link ' || library_id || ' ' || root_id FROM library_root ORDER BY root_id, library_id",
            "SELECT 'grant ' || user_id || ' ' || library_id FROM user_library ORDER BY 1",
        ],
    ));
    rows
}

/// The source must hold the directory, and the target must not: the source
/// is such a library, so a move to it is refused in decision 25's words.
#[test]
fn a_dir_or_a_library_that_is_not_there_is_missing_and_its_own_library_is_a_mistake() {
    let top = tempdir("relocate-resolve");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = a_dir(
        &conn,
        &top.path().join("a"),
        "Manga",
        None,
        &[("S", "v01", 2)],
    );
    a_dir(
        &conn,
        &top.path().join("b"),
        "Comics",
        None,
        &[("T", "v01", 2)],
    );
    let (manga, comics) = (
        library_called(&conn, "Manga"),
        library_called(&conn, "Comics"),
    );
    let held = dir(&conn, root.id, manga).expect("the directory");
    assert_eq!(held.from_name, "Manga");

    assert!(
        matches!(dir(&conn, 4242, manga), Err(MoveError::Missing(why)) if why.contains("there is no directory 4242"))
    );
    assert_eq!(
        dir(&conn, root.id, 4242).err(),
        Some(MoveError::Missing("there is no library 4242".to_string()))
    );
    let not_in = MoveError::Missing(format!("`{}` is not in Comics", root.path.display()));
    assert_eq!(dir(&conn, root.id, comics).err(), Some(not_in.clone()));
    let before = dump(&conn);
    assert_eq!(
        move_root(&conn, root.id, comics, Target::New("Made")),
        Err(not_in),
        "the move asks the same, before it makes a library"
    );
    assert_eq!(dump(&conn), before);
    assert!(matches!(
        resolve(&conn, &held, Target::Library(4242)),
        Err(MoveError::Missing(why)) if why.contains("4242")
    ));
    assert_eq!(
        resolve(&conn, &held, Target::Library(held.from)),
        Err(MoveError::Unusable(format!(
            "`{}` is already in Manga",
            root.path.display()
        )))
    );
    for (name, says) in [
        ("  ", "needs a name"),
        ("comics", "already called `Comics`"),
    ] {
        assert!(
            matches!(
                resolve(&conn, &held, Target::New(name)),
                Err(MoveError::Unusable(why)) if why.contains(says)
            ),
            "{name:?}"
        );
    }
    assert_eq!(
        resolve(
            &conn,
            &held,
            Target::Library(library_called(&conn, "Comics"))
        ),
        Ok(library_called(&conn, "Comics"))
    );
    let made = resolve(&conn, &held, Target::New(" Extras ")).expect("makes it");
    assert_eq!(
        crate::libraries::name_of(&conn, made)
            .expect("reads")
            .as_deref(),
        Some("Extras")
    );
}

/// A move makes no library a page address cannot hold
/// ([`crate::has_no_address`]), and says so as the caller's mistake rather
/// than as `libraries::create`'s fault inside the insert.
#[test]
fn a_move_makes_no_library_called_a_dot_name() {
    let top = tempdir("relocate-dot-names");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = a_dir(
        &conn,
        &top.path().join("a"),
        "Manga",
        None,
        &[("S", "v01", 2)],
    );
    let held = dir(&conn, root.id, root.libraries[0]).expect("the directory");

    for name in [".", "..", " .. "] {
        assert_eq!(
            resolve(&conn, &held, Target::New(name)),
            Err(MoveError::Unusable(crate::NO_ADDRESS.to_string())),
            "{name:?}"
        );
    }
    assert_eq!(scalar(&conn, "SELECT count(*) FROM library"), 1);
}

/// The directory is its library's last and the library is `sam`'s only grant:
/// the move goes ahead, the rehearsal names `sam`, and `sam` then sees no
/// library. Moving a directory its library keeps another of names no one.
#[test]
fn moving_the_last_dir_of_an_accounts_only_library_names_who_is_left_with_none() {
    let dir = tempdir("relocate-only-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    // Shelf's two directories come first, so the moved directory's id is not its
    // library's, and a holder lookup keyed on the directory's id finds no one.
    let shelf_a = a_dir(
        &conn,
        &dir.path().join("b"),
        "Shelf",
        None,
        &[("T", "v01", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("c"),
        "Shelf",
        None,
        &[("U", "v01", 2)],
    );
    let solo = a_dir(
        &conn,
        &dir.path().join("a"),
        "Solo",
        None,
        &[("S", "v01", 2)],
    );
    assert_ne!(
        solo.id, solo.libraries[0],
        "the fixture must tell a directory's id from its library's: {solo:?}"
    );
    scanned(&conn);
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("an account");
    let sam = conn.last_insert_rowid();
    crate::account::grant_library(&conn, sam, solo.libraries[0]).expect("grants");
    let before = dump(&conn);

    let rehearsed = move_cost(
        &conn,
        solo.id,
        solo.libraries[0],
        Target::Library(shelf_a.libraries[0]),
    )
    .expect("rehearses");
    assert_eq!(rehearsed.left_with_no_library, vec!["sam"], "{rehearsed:?}");
    assert_eq!(rehearsed.loses_sight, vec!["sam"], "{rehearsed:?}");
    assert_eq!(rehearsed.library_removed.as_deref(), Some("Solo"));
    assert_eq!(dump(&conn), before, "the rehearsal changes nothing");

    let done = moved(&conn, &solo, Target::Library(shelf_a.libraries[0]));
    assert_eq!(done, rehearsed, "the move does what the rehearsal said");
    let access = crate::account::access_for(&conn, sam).expect("reads");
    assert_eq!(
        access,
        crate::account::Access::roots(Vec::new()),
        "sam sees no library, not every library"
    );
    assert_eq!(
        crate::browse::series(&conn, &access, sam, None).expect("lists"),
        Vec::new()
    );
    assert!(
        !crate::browse::series(&conn, &crate::account::Access::Every, sam, None)
            .expect("lists")
            .is_empty(),
        "the fixture: Shelf holds series, the moved directory's among them"
    );

    crate::account::grant_library(&conn, sam, shelf_a.libraries[0]).expect("grants Shelf");
    let kept = moved(&conn, &shelf_a, Target::New("Fresh"));
    assert_eq!(kept.library_removed, None, "Shelf keeps another directory");
    assert_eq!(kept.left_with_no_library, Vec::<String>::new());
}

fn a_position_and_a_note(conn: &Connection, work: i64, edition: &str) {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, ?2, 1)",
        rusqlite::params![work, edition],
    )
    .expect("a position");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'n' || ?1 || ?2, ?1, ?2, 0, 'kept', 'now', 'now')",
        rusqlite::params![work, edition],
    )
    .expect("a note");
}

fn the_work(conn: &Connection, title_like: &str) -> (i64, String) {
    conn.query_row(
        "SELECT id, slug FROM work WHERE title LIKE ?1",
        [title_like],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap_or_else(|e| panic!("one work like {title_like}: {e}"))
}

/// Move `root` out of its one library, read from the store rather than from
/// `root`, which an earlier move in the test may have made stale.
fn moved(conn: &Connection, root: &Root, target: Target<'_>) -> Moved {
    let links = scan::roots(conn)
        .expect("lists")
        .into_iter()
        .find(|held| held.id == root.id)
        .expect("the directory")
        .libraries;
    let [from] = links[..] else {
        panic!("the fixture moves a directory in one library: {links:?}");
    };
    move_root(conn, root.id, from, target).expect("moves")
}

fn an_account(conn: &Connection, name: &str, granted: &[i64]) -> i64 {
    let id = crate::account::add(conn, name, "pw", "user").expect("an account");
    for library in granted {
        crate::account::grant_library(conn, id, *library).expect("grants");
    }
    id
}

/// Case 2, the one a move exists for: the library keeps another directory, and
/// the moved directory's work keeps its row, its slug, its position, its note,
/// its series with its alternative title, and what was put on it by hand.
/// `keeps_documents` is off, so nothing but the row holds the hand title.
#[test]
fn a_dir_leaving_a_library_that_keeps_another_keeps_its_works_as_they_were() {
    let dir = tempdir("relocate-case-2");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_dir(
        &conn,
        &dir.path().join("kept"),
        "A",
        None,
        &[("Kept", "v01", 2)],
    );
    let moving = a_dir(
        &conn,
        &dir.path().join("moved"),
        "A",
        None,
        &[("Moved", "v01", 3)],
    );
    let other = a_dir(
        &conn,
        &dir.path().join("else"),
        "B",
        None,
        &[("Else", "v01", 4)],
    );
    scanned(&conn);
    let (id, slug) = the_work(&conn, "Moved%");
    a_position_and_a_note(&conn, id, "raw");
    conn.execute(
        "UPDATE work SET title = 'By Hand', title_hand = 'By Hand' WHERE id = ?1",
        [id],
    )
    .expect("a hand title");
    derive::derive(&conn).expect("derives");
    assert_eq!(
        the_work(&conn, "By Hand").0,
        id,
        "the fixture: a derive leaves a hand title standing. If it does not, set the \
         title the way `librarydoc` does and keep this assertion"
    );
    let series: i64 = scalar(
        &conn,
        &format!("SELECT series_id FROM work WHERE id = {id}"),
    );
    conn.execute(
        "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, 'Alt', 0)",
        [series],
    )
    .expect("an alternative title");

    let before = kept(&conn);

    let report = moved(&conn, &moving, Target::Library(other.libraries[0]));

    assert_eq!(kept(&conn), before, "every row as it was");
    assert_eq!(
        the_work(&conn, "By Hand"),
        (id, slug),
        "the same row, the same slug, the hand title"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM series_title WHERE series_id = {series}")
        ),
        2,
        "its series with its alternative title"
    );
    assert_eq!(
        (report.library_id, report.library.as_str()),
        (other.libraries[0], "B")
    );
    assert_eq!((report.library_removed, report.grants_removed), (None, 0));
}

/// Case 1: the directory is alone in its library, so the library goes with the
/// move, and its grant with it, and every other row stays as it was. The
/// account holds another library, so it is not left with none, and the grant
/// is counted.
#[test]
fn a_dir_alone_in_its_library_takes_its_works_and_the_library_goes() {
    let dir = tempdir("relocate-case-1");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("moved"),
        "Solo",
        None,
        &[("Moved", "v01", 3)],
    );
    let other = a_dir(
        &conn,
        &dir.path().join("else"),
        "B",
        None,
        &[("Else", "v01", 4)],
    );
    scanned(&conn);
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("an account");
    let sam = conn.last_insert_rowid();
    for library in [moving.libraries[0], other.libraries[0]] {
        crate::account::grant_library(&conn, sam, library).expect("grants");
    }
    let before = kept(&conn);

    let report = moved(&conn, &moving, Target::Library(other.libraries[0]));

    assert_eq!(kept(&conn), before, "every row as it was");
    assert_eq!(report.library_removed.as_deref(), Some("Solo"));
    assert_eq!(report.grants_removed, 1);
    assert_eq!(
        report.left_with_no_library,
        Vec::<String>::new(),
        "sam keeps B"
    );
    assert_eq!(
        (report.gains_sight, report.loses_sight),
        (Vec::<String>::new(), Vec::<String>::new()),
        "sam sees the directory through Solo before and through B after"
    );
    assert_eq!(
        crate::libraries::name_of(&conn, moving.libraries[0]).expect("reads"),
        None
    );
}

/// A rehearsal is the move rolled back: the same answer, and neither the
/// store nor either directory's `.taiyaku/` changed. The moved directory
/// keeps documents and its work's document is missing, so a rehearsal that
/// derived and reconciled would write it back.
#[test]
fn a_rehearsal_answers_what_the_move_would_and_leaves_everything_as_it_was() {
    let dir = tempdir("relocate-rehearsal");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_dir(
        &conn,
        &dir.path().join("kept"),
        "A",
        None,
        &[("Kept", "v01", 2)],
    );
    let moving = a_dir(
        &conn,
        &dir.path().join("moved"),
        "A",
        None,
        &[("Moved", "v01", 3)],
    );
    scan::update_root(
        &conn,
        moving.id,
        &RootPatch {
            keeps_documents: Some(true),
            ..Default::default()
        },
    )
    .expect("keeps documents");
    scanned(&conn);
    an_account(&conn, "sam", &[library_called(&conn, "A")]);
    let (id, _) = the_work(&conn, "Moved%");
    a_position_and_a_note(&conn, id, "raw");
    let mut doc = crate::librarydoc::of(&conn, id).expect("projects");
    doc.title = Some("By Hand".to_string());
    crate::librarydoc::apply(&conn, id, &doc).expect("applies");
    let owed = crate::hand::save(&conn, id)
        .expect("saves")
        .path
        .expect("an address");
    std::fs::remove_file(&owed).expect("a document that went missing");
    let files = |root: &Path| -> Vec<String> {
        let mut found = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in std::fs::read_dir(&next).expect("lists").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let modified = entry.metadata().expect("stats").modified().expect("mtime");
                    found.push(format!("{} {modified:?}", path.display()));
                }
            }
        }
        found.sort();
        found
    };
    let before = (dump(&conn), files(dir.path()));

    let rehearsed =
        move_cost(&conn, moving.id, moving.libraries[0], Target::New("Fresh")).expect("rehearses");

    assert_eq!(
        (dump(&conn), files(dir.path())),
        before,
        "a reconciling derive would write the missing document back"
    );
    assert_eq!(
        crate::libraries::id_of(&conn, "Fresh").expect("reads"),
        None
    );

    let done =
        move_root(&conn, moving.id, moving.libraries[0], Target::New("Fresh")).expect("moves");
    assert_eq!(
        Moved {
            library_id: 0,
            ..done.clone()
        },
        Moved {
            library_id: 0,
            ..rehearsed
        },
        "the move did what the rehearsal said, in a library made twice"
    );
    assert_eq!(done.loses_sight, vec!["sam"], "{done:?}");
}

#[test]
fn a_new_library_is_made_for_the_dir_and_named_in_the_answer() {
    let dir = tempdir("relocate-new-library");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "A",
        None,
        &[("Kept", "v01", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("b"),
        "A",
        None,
        &[("Other", "v01", 3)],
    );
    scanned(&conn);

    let report = moved(&conn, &moving, Target::New(" Extras "));

    assert_eq!(report.library, "Extras");
    assert_eq!(report.library_id, library_called(&conn, "Extras"));
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT library_id FROM library_root WHERE root_id = {}",
                moving.id
            )
        ),
        report.library_id
    );
}

/// The document is addressed off the hub's directory, which has not moved on
/// disk, so it is where it was.
#[test]
fn a_move_with_documents_kept_leaves_the_works_document_where_it_was() {
    let dir = tempdir("relocate-documents");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_dir(
        &conn,
        &dir.path().join("a"),
        "A",
        None,
        &[("Kept", "v01", 2)],
    );
    let moving = a_dir(
        &conn,
        &dir.path().join("b"),
        "A",
        None,
        &[("Moved", "v01", 3)],
    );
    scan::update_root(
        &conn,
        moving.id,
        &RootPatch {
            keeps_documents: Some(true),
            ..Default::default()
        },
    )
    .expect("keeps documents");
    scanned(&conn);
    let (id, slug) = the_work(&conn, "Moved%");
    let key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("a key");
    let mut doc = crate::librarydoc::of(&conn, id).expect("projects");
    doc.title = Some("By Hand".to_string());
    crate::librarydoc::apply(&conn, id, &doc).expect("applies");
    let document = crate::hand::save(&conn, id)
        .expect("saves")
        .path
        .expect("an address");
    assert!(document.is_file(), "the fixture: {document:?} was written");

    moved(&conn, &moving, Target::New("Elsewhere"));

    assert_eq!(
        crate::hand::path_for(&conn, id).expect("reads"),
        Some(document.clone())
    );
    assert!(document.is_file());
    let (again, same_slug) = the_work(&conn, "By Hand");
    assert_eq!((again, same_slug), (id, slug));
    let same_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("a key");
    assert_eq!(same_key, key);
}

/// O190: what a move does to an open proposal. It asserts only that the move
/// succeeds and the row stands, and prints the rest.
#[test]
fn a_proposal_across_a_moved_dir_still_exists() {
    let dir = tempdir("relocate-o190");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_dir(
        &conn,
        &dir.path().join("a"),
        "A",
        None,
        &[("Sousou no Frieren", "v01", 2)],
    );
    let moving = a_dir(
        &conn,
        &dir.path().join("b"),
        "A",
        None,
        &[("Sousou no Frieren - Fan Translated", "v01", 3)],
    );
    scanned(&conn);
    crate::proposals::raise(&conn).expect("raises");
    let proposal = crate::proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");
    println!("O190 before: {proposal:?}");

    let report = moved(&conn, &moving, Target::New("Fan"));

    println!("O190 move: {report:?}");
    let row = crate::proposals::lookup(&conn, proposal.id).expect("reads");
    println!("O190 after: {row:?}");
    let preview = crate::proposals::preview(&conn, proposal.id);
    println!("O190 preview: {preview:?}");
    if let Ok(rule) = &preview {
        println!(
            "O190 accept: {:?}",
            crate::proposals::accept_with(&conn, proposal.id, rule)
        );
    }
    assert!(row.is_some());
}

/// One library, Manga, of two directories: `/v` holds a volume and `/c` its
/// chapter release under the edition key `chapters`, so they are one work of
/// two editions, its hub `chapters` under `/c`. A position and a note on each
/// edition, and `ann`, granted Manga alone. Answers (`/v`, `/c`, the work's
/// slug).
fn a_volume_and_its_chapters(conn: &Connection, dir: &Path) -> (Root, Root, String) {
    let volume = a_dir(conn, &dir.join("v"), "Manga", None, &[("Series", "v01", 3)]);
    let chapters = a_dir(
        conn,
        &dir.join("c"),
        "Manga",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    scanned(conn);
    let (work, slug) = the_work(conn, "Series%");
    assert_eq!(
        lines(
            conn,
            &[&format!(
                "SELECT w.hub || ' ' || group_concat(e.key, ' ') FROM work w
                   JOIN (SELECT work_id, key FROM edition ORDER BY key) e ON e.work_id = w.id
                  WHERE w.id = {work}"
            )]
        ),
        ["chapters chapters raw"],
        "the fixture: one work of two editions, its hub under /c"
    );
    a_position_and_a_note(conn, work, "raw");
    a_position_and_a_note(conn, work, "chapters");
    an_account(conn, "ann", &[volume.libraries[0]]);
    (volume, chapters, slug)
}

/// §3.5: moving `/c` out of Manga relabels the directory and writes no other
/// row. The work stays one work of two editions, its row filed where it was
/// though its hub is under `/c`, and every edition, member, position and
/// note is the row it was. `ann`, granted Manga alone, loses sight of `/c`.
#[test]
fn moving_a_dir_writes_no_work_edition_member_position_or_note() {
    let dir = tempdir("relocate-relabel");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_, chapters, _) = a_volume_and_its_chapters(&conn, dir.path());
    let before = kept(&conn);

    let report = moved(&conn, &chapters, Target::New("Chapters"));

    assert_eq!(kept(&conn), before);
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT library_id FROM library_root WHERE root_id = {}",
                chapters.id
            )
        ),
        report.library_id
    );
    assert_eq!(report.loses_sight, vec!["ann"], "{report:?}");
    assert_eq!(report.gains_sight, Vec::<String>::new(), "{report:?}");
    assert_eq!((report.library_removed, report.grants_removed), (None, 0));
}

/// Who gains and who loses sight of `/a`, moved from A to B: `zed` and `cy`,
/// granted B alone, gain it, in name order; `ann`, granted A alone, loses it;
/// `both` sees it before and after. An administrator holding a grant of B and
/// an every-library account holding one of A see every directory before and
/// after, so neither is named.
#[test]
fn an_account_granted_only_the_target_gains_sight_and_one_seeing_every_library_is_named_in_neither()
{
    let dir = tempdir("relocate-sight");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(&conn, &dir.path().join("a"), "A", None, &[("S", "v01", 2)]);
    a_dir(&conn, &dir.path().join("a2"), "A", None, &[("U", "v01", 2)]);
    let b = a_dir(&conn, &dir.path().join("b"), "B", None, &[("T", "v01", 2)]);
    let (a, b) = (moving.libraries[0], b.libraries[0]);
    an_account(&conn, "zed", &[b]);
    an_account(&conn, "cy", &[b]);
    an_account(&conn, "ann", &[a]);
    an_account(&conn, "both", &[a, b]);
    let every = an_account(&conn, "every", &[a]);
    conn.execute("UPDATE user SET every_library = 1 WHERE id = ?1", [every])
        .expect("sees every library, its grant of A kept");
    let admin = crate::account::add(&conn, "admin", "pw", "admin").expect("an administrator");
    conn.execute(
        "INSERT INTO user_library (user_id, library_id) VALUES (?1, ?2)",
        [admin, b],
    )
    .expect("a grant an administrator does not need");
    conn.execute("UPDATE user SET every_library = 0 WHERE id = ?1", [admin])
        .expect("an administrator marked restricted, whom a role filter must still leave out");

    let report = moved(&conn, &moving, Target::Library(b));

    assert_eq!(report.gains_sight, vec!["cy", "zed"], "{report:?}");
    assert_eq!(report.loses_sight, vec!["ann"], "{report:?}");
}

/// Spec §4's N, Manga holding `m` and Kids `m/kids`: moving `m/kids` from Kids
/// to Other, `both`, granted Manga and Kids, still sees it through Manga, and
/// so does `manga`; neither is named. `kids` loses it and `other` gains it.
/// The rehearsal answers what the move does.
#[test]
fn an_account_whose_grant_reaches_a_moved_dir_on_both_sides_is_named_in_neither() {
    let n = crate::reach::tests::nested_libraries(false);
    let conn = n.library.conn();
    let elsewhere = tempdir("relocate-sight-reach");
    let o = elsewhere.path().join("o");
    write_pages(&o.join("Other Book").join("Other Book - v01"), 2);
    let other = scan::add_root_checked_in(&conn, &o, InLibrary::Named("Other"))
        .expect("adds Other's directory")
        .root
        .libraries[0];
    an_account(&conn, "both", &[n.manga, n.kids]);
    an_account(&conn, "other", &[other]);

    let rehearsed = move_cost(&conn, n.m_kids, n.kids, Target::Library(other)).expect("rehearses");
    let report = move_root(&conn, n.m_kids, n.kids, Target::Library(other)).expect("moves");

    assert_eq!(report, rehearsed);
    assert_eq!(report.gains_sight, vec!["other"], "{report:?}");
    assert_eq!(report.loses_sight, vec!["kids"], "{report:?}");
}

/// After `/c` moves out of Manga, the work has an edition under each
/// library's directories, so each library counts it and names it, though no
/// derive has run since.
#[test]
fn after_a_move_each_library_a_dir_of_which_holds_an_edition_counts_and_names_the_work() {
    let dir = tempdir("relocate-counts");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (volume, chapters, slug) = a_volume_and_its_chapters(&conn, dir.path());

    let report = moved(&conn, &chapters, Target::New("Chapters"));

    assert_eq!(
        crate::libraries::list(&conn)
            .expect("lists")
            .into_iter()
            .map(|listed| (listed.name, listed.works))
            .collect::<Vec<_>>(),
        [("Chapters".to_string(), 1), ("Manga".to_string(), 1)]
    );
    for library in [volume.libraries[0], report.library_id] {
        assert_eq!(
            crate::browse::held_in(&conn, library).expect("reads"),
            HashSet::from([slug.clone()]),
            "library {library}"
        );
    }
}

/// Overlapping-libraries spec §4: a directory is not moved inside or around a
/// directory of the library it would join, in the add's words, by the rehearsal
/// and the move alike, and a refused move changes nothing. Into a library whose
/// directories sit elsewhere it moves, though another library's holds it.
#[test]
fn a_dir_is_not_moved_inside_or_around_a_dir_of_the_library_it_joins() {
    let top = tempdir("relocate-nested");
    let (m, x) = (top.path().join("m"), top.path().join("x"));
    let kids_dir = m.join("kids");
    std::fs::create_dir_all(&kids_dir).expect("mkdir");
    std::fs::create_dir_all(&x).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let checked = |path: &Path, name: &str| {
        scan::add_root_checked_in(&conn, path, InLibrary::Named(name))
            .expect("adds")
            .root
    };
    let manga = checked(&m, "Manga");
    let kids = checked(&kids_dir, "Kids");
    let other = checked(&x, "Other");
    let before = dump(&conn);

    for (moving, into, said) in [
        (
            &kids,
            &manga,
            format!(
                "{} is inside {}, which is already a directory of the same library, `Manga`",
                kids.path.display(),
                manga.path.display()
            ),
        ),
        (
            &manga,
            &kids,
            format!(
                "{} holds `kids`, which is already a directory of the same library, `Kids`",
                manga.path.display()
            ),
        ),
    ] {
        let into = Target::Library(into.libraries[0]);
        assert_eq!(
            move_cost(&conn, moving.id, moving.libraries[0], into),
            Err(MoveError::Unusable(said.clone()))
        );
        assert_eq!(
            move_root(&conn, moving.id, moving.libraries[0], into),
            Err(MoveError::Unusable(said))
        );
        assert_eq!(dump(&conn), before, "a refused move changed nothing");
    }

    let moved = move_root(
        &conn,
        kids.id,
        kids.libraries[0],
        Target::Library(other.libraries[0]),
    )
    .expect("a directory moves to a library whose directories sit elsewhere");
    assert_eq!(moved.library_id, other.libraries[0]);
}

/// A move does not ask the add's directory-member refusal, which does not
/// depend on the library: Kids' directory, a chapter of a volume Manga reads as
/// one directory member, added before Manga's scan made the volume a member,
/// moves into another library as it would into a new one.
#[test]
fn a_dir_inside_another_librarys_directory_member_moves_into_a_library() {
    let top = tempdir("relocate-in-a-member");
    let (m, x) = (top.path().join("m"), top.path().join("x"));
    let volume = m.join("S").join("S - v01");
    write_pages(&volume, 2);
    let chapter = volume.join("S - v01 - c001");
    write_pages(&chapter, 2);
    std::fs::create_dir_all(&x).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let checked = |path: &Path, name: &str| {
        scan::add_root_checked_in(&conn, path, InLibrary::Named(name))
            .expect("adds")
            .root
    };
    let kids = checked(&chapter, "Kids");
    let manga = checked(&m, "Manga");
    let other = checked(&x, "Other");
    scanned(&conn);
    let members_around: i64 = conn
        .query_row(
            "SELECT count(*) FROM member WHERE root_id = ?1 AND path = ?2",
            rusqlite::params![manga.id, manga.path.join("S/S - v01").to_string_lossy()],
            |r| r.get(0),
        )
        .expect("counts");
    assert_eq!(
        members_around, 1,
        "Manga reads the volume around Kids' directory as one member"
    );

    let moved = move_root(
        &conn,
        kids.id,
        kids.libraries[0],
        Target::Library(other.libraries[0]),
    )
    .expect("a directory inside a directory member moves into a library");
    assert_eq!(moved.library_id, other.libraries[0]);
}

/// The libraries linking root `root_id`, ascending.
fn links_of(conn: &Connection, root_id: i64) -> Vec<i64> {
    let mut statement = conn
        .prepare("SELECT library_id FROM library_root WHERE root_id = ?1 ORDER BY library_id")
        .expect("prepares");
    statement
        .query_map([root_id], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("reads")
}

/// `m` in Manga and, linked through the add, in Kids, which holds `k` too;
/// Lib3 holds `c`. Manga is made first, so it is `m`'s lowest link. Answers
/// `m` and the three library ids.
fn a_dir_in_manga_and_kids(conn: &Connection, top: &Path) -> (Root, [i64; 3]) {
    let m = a_dir(conn, &top.join("m"), "Manga", None, &[("S", "v01", 2)]);
    a_dir(conn, &top.join("k"), "Kids", None, &[("T", "v01", 2)]);
    a_dir(conn, &top.join("c"), "Lib3", None, &[("U", "v01", 2)]);
    let linked = scan::add_root_checked_in(conn, &m.path, InLibrary::Named("Kids"))
        .expect("links m into Kids");
    let ids = [
        library_called(conn, "Manga"),
        library_called(conn, "Kids"),
        library_called(conn, "Lib3"),
    ];
    assert_eq!(linked.also_in, ["Manga"], "the fixture links m");
    assert_eq!(links_of(conn, m.id), vec![ids[0], ids[1]]);
    scanned(conn);
    (m, ids)
}

/// Overlapping-libraries spec §5: a move takes the directory out of the
/// library it names and no other. Moved from Kids to Lib3, `m` stays in Manga,
/// and only Kids' accounts that see it through no other library lose it.
/// Moved on from Manga, its last directory, Manga goes with its grants.
#[test]
fn moving_a_shared_dir_from_one_library_leaves_it_in_the_other() {
    let top = tempdir("relocate-shared");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (m, [manga, kids, lib3]) = a_dir_in_manga_and_kids(&conn, top.path());
    an_account(&conn, "kid", &[kids]);
    an_account(&conn, "fan", &[manga]);
    an_account(&conn, "both", &[kids, manga]);
    an_account(&conn, "three", &[lib3]);
    let works = kept(&conn);
    let before = dump(&conn);

    let rehearsed = move_cost(&conn, m.id, kids, Target::Library(lib3)).expect("rehearses");
    assert_eq!(dump(&conn), before, "the rehearsal changes nothing");
    let done = move_root(&conn, m.id, kids, Target::Library(lib3)).expect("moves");

    assert_eq!(done, rehearsed);
    assert_eq!(
        done,
        Moved {
            library_id: lib3,
            library: "Lib3".to_string(),
            gains_sight: vec!["three".to_string()],
            loses_sight: vec!["kid".to_string()],
            library_removed: None,
            grants_removed: 0,
            left_with_no_library: Vec::new(),
        }
    );
    assert_eq!(links_of(&conn, m.id), vec![manga, lib3], "Manga keeps m");
    assert_eq!(kept(&conn), works, "no work row changes");

    let emptied = move_root(&conn, m.id, manga, Target::Library(kids)).expect("moves");
    assert_eq!(
        emptied,
        Moved {
            library_id: kids,
            library: "Kids".to_string(),
            gains_sight: vec!["kid".to_string()],
            loses_sight: vec!["fan".to_string()],
            library_removed: Some("Manga".to_string()),
            grants_removed: 2,
            left_with_no_library: vec!["fan".to_string()],
        }
    );
    assert_eq!(links_of(&conn, m.id), vec![kids, lib3]);
    assert_eq!(
        crate::libraries::name_of(&conn, manga).expect("reads"),
        None
    );
}

/// Decision 25: a move to a library that already holds the directory is
/// refused, by the rehearsal and the move, and writes nothing. Without the
/// refusal the link's key refuses the write, so the sentence is asserted.
#[test]
fn a_move_to_a_library_that_already_holds_the_dir_is_refused_and_writes_nothing() {
    let top = tempdir("relocate-already-in");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (m, [manga, kids, _]) = a_dir_in_manga_and_kids(&conn, top.path());
    let before = dump(&conn);

    for (from, to, name) in [(kids, manga, "Manga"), (manga, kids, "Kids")] {
        let already = Err(MoveError::Unusable(format!(
            "`{}` is already in {name}",
            m.path.display()
        )));
        let target = Target::Library(to);
        assert_eq!(move_cost(&conn, m.id, from, target), already, "{name}");
        assert_eq!(move_root(&conn, m.id, from, target), already, "{name}");
        assert_eq!(dump(&conn), before, "a refused move changed nothing");
    }
}

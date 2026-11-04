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
            "SELECT 'root ' || id || ' ' || library_id FROM root ORDER BY id",
            "SELECT 'grant ' || user_id || ' ' || library_id FROM user_library ORDER BY 1",
        ],
    ));
    rows
}

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
    let held = dir(&conn, root.id).expect("the directory");
    assert_eq!(held.library, "Manga");

    assert!(
        matches!(dir(&conn, 4242), Err(MoveError::Missing(why)) if why.contains("there is no directory 4242"))
    );
    assert!(matches!(
        resolve(&conn, &held, Target::Library(4242)),
        Err(MoveError::Missing(why)) if why.contains("4242")
    ));
    assert!(matches!(
        resolve(&conn, &held, Target::Library(held.library_id)),
        Err(MoveError::Unusable(why)) if why.contains("already in `Manga`")
    ));
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
    let held = dir(&conn, root.id).expect("the directory");

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
        solo.id, solo.library_id,
        "the fixture must tell a directory's id from its library's: {solo:?}"
    );
    scanned(&conn);
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("an account");
    let sam = conn.last_insert_rowid();
    crate::account::grant_library(&conn, sam, solo.library_id).expect("grants");
    let before = dump(&conn);

    let rehearsed =
        move_cost(&conn, solo.id, Target::Library(shelf_a.library_id)).expect("rehearses");
    assert_eq!(rehearsed.left_with_no_library, vec!["sam"], "{rehearsed:?}");
    assert_eq!(rehearsed.loses_sight, vec!["sam"], "{rehearsed:?}");
    assert_eq!(rehearsed.library_removed.as_deref(), Some("Solo"));
    assert_eq!(dump(&conn), before, "the rehearsal changes nothing");

    let done = moved(&conn, &solo, Target::Library(shelf_a.library_id));
    assert_eq!(done, rehearsed, "the move does what the rehearsal said");
    let access = crate::account::access_for(&conn, sam).expect("reads");
    assert_eq!(
        access,
        crate::account::Access::Only(Vec::new()),
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

    crate::account::grant_library(&conn, sam, shelf_a.library_id).expect("grants Shelf");
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

fn moved(conn: &Connection, root: &Root, target: Target<'_>) -> Moved {
    move_root(conn, root.id, target).expect("moves")
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

    let report = moved(&conn, &moving, Target::Library(other.library_id));

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
        (other.library_id, "B")
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
    for library in [moving.library_id, other.library_id] {
        crate::account::grant_library(&conn, sam, library).expect("grants");
    }
    let before = kept(&conn);

    let report = moved(&conn, &moving, Target::Library(other.library_id));

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
        crate::libraries::name_of(&conn, moving.library_id).expect("reads"),
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

    let rehearsed = move_cost(&conn, moving.id, Target::New("Fresh")).expect("rehearses");

    assert_eq!(
        (dump(&conn), files(dir.path())),
        before,
        "a reconciling derive would write the missing document back"
    );
    assert_eq!(
        crate::libraries::id_of(&conn, "Fresh").expect("reads"),
        None
    );

    let done = move_root(&conn, moving.id, Target::New("Fresh")).expect("moves");
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
            &format!("SELECT library_id FROM root WHERE id = {}", moving.id)
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
    an_account(conn, "ann", &[volume.library_id]);
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
            &format!("SELECT library_id FROM root WHERE id = {}", chapters.id)
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
    let (a, b) = (moving.library_id, b.library_id);
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
    for library in [volume.library_id, report.library_id] {
        assert_eq!(
            crate::browse::held_in(&conn, library).expect("reads"),
            HashSet::from([slug.clone()]),
            "library {library}"
        );
    }
}

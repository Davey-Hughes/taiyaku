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

/// What a move may touch, as text, for "the store is as it was".
///
/// No edition or member id is in it: a derive gives each edition it writes
/// a new row, so their ids differ after any derive and say nothing.
pub(super) fn dump(conn: &Connection) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'library ' || id || ' ' || name FROM library ORDER BY id",
        "SELECT 'root ' || id || ' ' || library_id FROM root ORDER BY id",
        "SELECT 'series ' || id || ' ' || coalesce(library_id, 'none') || ' ' || sort_title
           FROM series ORDER BY id",
        "SELECT 'work ' || id || ' ' || slug || ' ' || coalesce(library_id, 'none') || ' '
                || coalesce(series_id, 'none') || ' ' || content_key || ' ' || title
           FROM work ORDER BY id",
        "SELECT 'edition ' || work_id || ' ' || key FROM edition ORDER BY 1",
        "SELECT 'member ' || e.work_id || ' ' || e.key || ' ' || m.ordinal || ' ' || m.path
           FROM member m JOIN edition e ON e.id = m.edition_id ORDER BY 1",
        "SELECT 'progress ' || user_id || ' ' || work_id || ' ' || edition || ' ' || slot
           FROM progress ORDER BY 1",
        "SELECT 'note ' || user_id || ' ' || work_id || ' ' || edition || ' ' || id
           FROM note ORDER BY 1",
        "SELECT 'grant ' || user_id || ' ' || library_id FROM user_library ORDER BY 1",
    ] {
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

fn refusal(conn: &Connection, root: &Root, target: i64) -> (String, Vec<String>) {
    let dir = dir(conn, root.id).expect("the directory");
    match refused_before(conn, &dir, target).expect("reads") {
        Some(MoveError::Refused { why, volumes }) => (why, volumes),
        other => panic!("not refused: {other:?}"),
    }
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

/// §6.2's case 4: the target holds the volume under the same edition key.
/// The other volume, which the target does not hold, is not named.
#[test]
fn the_same_volume_under_the_same_edition_key_in_the_target_refuses_the_move() {
    let dir = tempdir("relocate-same-key");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "Chapters",
        None,
        &[("Series", "v01", 2), ("Series", "v02", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("b"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);

    let (why, volumes) = refusal(&conn, &moving, library_called(&conn, "Manga"));

    assert!(why.contains("same edition key"), "{why}");
    assert!(why.contains("`Manga`"), "{why}");
    assert!(
        !why.contains("both sets of pages"),
        "no reason it cannot give: {why}"
    );
    assert_eq!(volumes.len(), 1, "{volumes:?}");
    assert!(volumes[0].ends_with("(raw)"), "{volumes:?}");
}

/// The same two directories with an edition key apiece is case 3, which a move
/// makes: nothing refuses it here.
#[test]
fn the_same_volume_under_another_edition_key_does_not_refuse_the_move() {
    let top = tempdir("relocate-other-key");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &top.path().join("a"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    a_dir(
        &conn,
        &top.path().join("b"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let held = dir(&conn, moving.id).expect("the directory");
    assert_eq!(
        refused_before(&conn, &held, library_called(&conn, "Manga")).expect("reads"),
        None
    );
}

/// One edition with a member under each of two directories of one library. Moving
/// either directory away would divide the edition itself.
#[test]
fn an_edition_with_members_on_both_sides_refuses_the_move() {
    let dir = tempdir("relocate-spanning");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "Manga",
        None,
        &[("Series", "v01", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("b"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    a_dir(
        &conn,
        &dir.path().join("c"),
        "Other",
        None,
        &[("Else", "v01", 2)],
    );
    scanned(&conn);
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM edition e JOIN work w ON w.id = e.work_id
                        WHERE w.title LIKE 'Series%'"
        ),
        1,
        "the fixture: both directories' files are one edition of one work"
    );

    let (why, volumes) = refusal(&conn, &moving, library_called(&conn, "Other"));

    assert!(why.contains("one edition"), "{why}");
    assert_eq!(volumes.len(), 1, "{volumes:?}");
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

    let (rehearsed, _) =
        move_cost(&conn, solo.id, Target::Library(shelf_a.library_id)).expect("rehearses");
    assert_eq!(rehearsed.left_with_no_library, vec!["sam"], "{rehearsed:?}");
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

fn the_work(conn: &Connection, title_like: &str) -> (i64, String, Option<i64>) {
    conn.query_row(
        "SELECT id, slug, library_id FROM work WHERE title LIKE ?1",
        [title_like],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap_or_else(|e| panic!("one work like {title_like}: {e}"))
}

fn moved(conn: &Connection, root: &Root, target: Target<'_>) -> Moved {
    move_root(conn, root.id, target).expect("moves").0
}

/// Case 2, the one a move exists for: the library keeps another directory, and
/// the moved directory's work keeps its row, its slug, its position, its note
/// and what was put on it by hand. `keeps_documents` is off, so nothing but
/// the row holds the hand title.
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
    let (id, slug, _) = the_work(&conn, "Moved%");
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

    let report = moved(&conn, &moving, Target::Library(other.library_id));

    assert_eq!(
        the_work(&conn, "By Hand"),
        (id, slug, Some(other.library_id)),
        "the same row, the same slug, the hand title, under B"
    );
    assert_eq!(
        (report.works_kept, report.works_joined, report.works_split),
        (1, 0, 0)
    );
    assert_eq!((report.positions, report.notes), (1, 1));
    assert_eq!(report.library, "B");
    assert_eq!(report.library_removed, None);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {id}")
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM note WHERE work_id = {id}")
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM series s JOIN work w ON w.series_id = s.id
                  WHERE w.id = {id} AND s.id = {series} AND s.library_id = {}",
                other.library_id
            )
        ),
        1,
        "every work of the series moved, so the series went too"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM series_title WHERE series_id = {series}")
        ),
        2,
        "with its alternative title"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        3,
        "nothing was written again"
    );

    // A second derive finds nothing to do.
    let before = dump(&conn);
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(dump(&conn), before);
    assert_eq!(
        (again.emptied_works, again.rekeyed, again.kept_editions),
        (0, 0, 0)
    );
}

/// Case 1: the directory is alone in its library, so the library goes with the
/// move, and its grant with it. The account holds another library, so that
/// is allowed, and counted.
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
    let (id, slug, _) = the_work(&conn, "Moved%");
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("an account");
    let sam = conn.last_insert_rowid();
    for library in [moving.library_id, other.library_id] {
        crate::account::grant_library(&conn, sam, library).expect("grants");
    }

    let report = moved(&conn, &moving, Target::Library(other.library_id));

    assert_eq!(
        the_work(&conn, "Moved%"),
        (id, slug, Some(other.library_id))
    );
    assert_eq!(report.library_removed.as_deref(), Some("Solo"));
    assert_eq!(report.grants_removed, 1);
    assert_eq!(
        report.left_with_no_library,
        Vec::<String>::new(),
        "sam keeps B"
    );
    assert_eq!(report.works_kept, 1);
    assert_eq!(
        crate::libraries::name_of(&conn, moving.library_id).expect("reads"),
        None
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work WHERE library_id IS NULL"),
        0
    );
    assert_eq!(
        scalar(
            &conn,
            "SELECT count(*) FROM series WHERE library_id IS NULL"
        ),
        0
    );
}

/// Case 3, Chapters into Manga: the target holds the volume under another
/// edition key, so the two are one work with two editions. The target's row
/// is the one that stands, and the moved work's position is on it.
#[test]
fn a_volume_the_target_holds_under_another_key_joins_it_as_an_edition() {
    let dir = tempdir("relocate-case-3");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("chapters"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2), ("Series", "v02", 2)],
    );
    let volumes = a_dir(
        &conn,
        &dir.path().join("volumes"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let target: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND volume = 1.0",
            volumes.library_id
        ),
    );
    let joining: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND volume = 1.0",
            moving.library_id
        ),
    );
    let staying: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND volume = 2.0",
            moving.library_id
        ),
    );
    a_position_and_a_note(&conn, joining, "chapters");

    let report = moved(&conn, &moving, Target::Library(volumes.library_id));

    assert_eq!(
        (report.works_kept, report.works_joined, report.works_split),
        (1, 1, 0),
        "v02 kept its row; v01 joined Manga's"
    );
    assert_eq!(report.series_joined, 1);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM work WHERE id = {joining}")
        ),
        0
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {target}")
        ),
        2
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM progress WHERE work_id = {target} AND edition = 'chapters'"
            )
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM work WHERE id = {staying}")
        ),
        volumes.library_id
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(DISTINCT series_id) FROM work WHERE id IN ({target}, {staying})"
            )
        ),
        1,
        "and both volumes are in Manga's series of that title"
    );
}

/// Case 5 with the hub staying: the work stays where its hub is, and the
/// moved edition is a work of the target with that edition's position and
/// note. The alignment between the two editions is dropped and counted.
#[test]
fn a_divided_work_stays_with_its_hub_and_its_other_edition_takes_its_history() {
    let dir = tempdir("relocate-split-stays");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_dir(
        &conn,
        &dir.path().join("hub"),
        "Manga",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    let moving = a_dir(
        &conn,
        &dir.path().join("raw"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let (id, slug, home) = the_work(&conn, "Series%");
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM work WHERE id = {id} AND hub = 'en'")
        ),
        1,
        "the fixture: one work, its hub the directory that stays"
    );
    for edition in ["en", "raw"] {
        a_position_and_a_note(&conn, id, edition);
    }
    // Two more rows on the edition that stays, so that counting the staying
    // side's rows instead of the moving side's cannot give the same answer.
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("a second reader");
    let sam = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'en', 1)",
        [sam, id],
    )
    .expect("a second position on `en`");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'second', ?1, 'en', 0, 'kept', 'now', 'now')",
        [id],
    )
    .expect("a second note on `en`");
    conn.execute(
        "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'en', 'raw', 'hand')",
        [id],
    )
    .expect("an alignment between the two");

    let report = moved(&conn, &moving, Target::New("Raws"));

    let raws = library_called(&conn, "Raws");
    assert_eq!(
        (report.works_kept, report.works_joined, report.works_split),
        (0, 0, 1)
    );
    assert_eq!(report.alignments_dropped, 1);
    assert_eq!(
        (report.positions, report.notes),
        (1, 1),
        "only `raw` moves, and `en`'s rows stay with the work that stays"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM work WHERE id = {id}")
        ),
        home.expect("filed")
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT slug = '{slug}' FROM work WHERE id = {id}")
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {id}")
        ),
        1,
        "it holds its hub edition and no other"
    );
    let split: i64 = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE library_id = {raws}"),
    );
    for table in ["progress", "note"] {
        assert_eq!(
            scalar(
                &conn,
                &format!(
                    "SELECT count(*) FROM {table} WHERE work_id = {split} AND edition = 'raw'"
                )
            ),
            1,
            "{table}: `raw`'s row went with `raw`"
        );
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {id}")
            ),
            2,
            "{table}: `en`'s stayed"
        );
    }
    assert_eq!(scalar(&conn, "SELECT count(*) FROM alignment"), 0);
}

/// Case 5 with the hub moving: the row, its slug and its hand title go to
/// the target with the hub, and the edition that stays is a new work where
/// it stays, with its own position.
#[test]
fn a_divided_work_goes_with_its_hub_and_the_edition_that_stays_keeps_its_history() {
    let dir = tempdir("relocate-split-moves");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("hub"),
        "Manga",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("raw"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let (id, slug, home) = the_work(&conn, "Series%");
    for edition in ["en", "raw"] {
        a_position_and_a_note(&conn, id, edition);
    }
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

    let report = moved(&conn, &moving, Target::New("English"));

    let english = library_called(&conn, "English");
    assert_eq!(report.works_split, 1);
    assert_eq!(the_work(&conn, "By Hand"), (id, slug, Some(english)));
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {id} AND key = 'en'")
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {id}")
        ),
        1
    );
    let stayed: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            home.expect("filed")
        ),
    );
    assert_ne!(stayed, id);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {stayed} AND edition = 'raw'")
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {id} AND edition = 'en'")
        ),
        1
    );
}

/// A rehearsal is the move rolled back: the same answer, and neither the
/// store nor either directory's `.taiyaku/` changed. Both directories keep
/// documents, so a derive that reconciled would write there.
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
    let (id, _, _) = the_work(&conn, "Moved%");
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

    let (rehearsed, _derived) =
        move_cost(&conn, moving.id, Target::New("Fresh")).expect("rehearses");

    assert_eq!(
        (dump(&conn), files(dir.path())),
        before,
        "a reconciling derive would write the missing document back"
    );
    assert_eq!(
        crate::libraries::id_of(&conn, "Fresh").expect("reads"),
        None
    );

    let (done, _derived) = move_root(&conn, moving.id, Target::New("Fresh")).expect("moves");
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
    assert_eq!((done.works_kept, done.positions, done.notes), (1, 1, 1));
}

/// The pre-checks are asked by the rehearsal and the move alike, and a
/// refused move leaves the store as it was, the library it would have made
/// included.
#[test]
fn a_refused_move_changes_nothing_whether_rehearsed_or_made() {
    let dir = tempdir("relocate-refused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "Chapters",
        None,
        &[("Series", "v01", 2)],
    );
    let target = a_dir(
        &conn,
        &dir.path().join("b"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let before = dump(&conn);

    for attempt in [
        move_cost(&conn, moving.id, Target::Library(target.library_id)),
        move_root(&conn, moving.id, Target::Library(target.library_id)),
    ] {
        assert!(
            matches!(&attempt, Err(MoveError::Refused { volumes, .. }) if volumes.len() == 1),
            "{attempt:?}"
        );
    }
    assert_eq!(dump(&conn), before);
    assert!(matches!(
        move_root(&conn, moving.id, Target::Library(moving.library_id)),
        Err(MoveError::Unusable(_))
    ));
    assert!(matches!(
        move_root(&conn, 4242, Target::New("X")),
        Err(MoveError::Missing(_))
    ));
    assert_eq!(dump(&conn), before);
}

/// The verdict, where no pre-check sees it coming. A position on the target's
/// work under the key the moved edition will arrive as, which no edition
/// there holds yet: the fold meets it and one of the two positions stops
/// existing. The move is refused on the count.
#[test]
fn a_move_that_would_lose_a_position_is_refused_by_the_verdict() {
    let dir = tempdir("relocate-verdict");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("chapters"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let volumes = a_dir(
        &conn,
        &dir.path().join("volumes"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let joining: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            moving.library_id
        ),
    );
    let target: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            volumes.library_id
        ),
    );
    for work in [joining, target] {
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'chapters', 1)",
            [work],
        )
        .expect("a position under that key on each");
    }
    let before = dump(&conn);

    let refused = move_root(&conn, moving.id, Target::Library(volumes.library_id));

    assert!(
        matches!(&refused, Err(MoveError::Refused { why, .. }) if why.contains("would lose a position, a note, an alignment")),
        "{refused:?}"
    );
    assert_eq!(dump(&conn), before);
}

/// `refused_before`'s second statement counts a member whose directory was
/// removed (`root_id` NULL, what `ON DELETE SET NULL` leaves) as "elsewhere".
/// `!= ?1` would call it nowhere and let the move divide the edition.
#[test]
fn an_edition_with_a_member_whose_dir_is_gone_still_refuses_the_move() {
    let dir = tempdir("relocate-spanning-null");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "Manga",
        None,
        &[("Series", "v01", 2)],
    );
    let other = a_dir(
        &conn,
        &dir.path().join("b"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    a_dir(
        &conn,
        &dir.path().join("c"),
        "Other",
        None,
        &[("Else", "v01", 2)],
    );
    scanned(&conn);
    conn.execute(
        "UPDATE member SET root_id = NULL WHERE root_id = ?1",
        [other.id],
    )
    .expect("a member whose directory is gone");
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM member WHERE root_id IS NULL"),
        1,
        "the fixture: one member under no directory, one under the moving one"
    );

    let (why, volumes) = refusal(&conn, &moving, library_called(&conn, "Other"));

    assert!(why.contains("one edition"), "{why}");
    assert_eq!(volumes.len(), 1, "{volumes:?}");
}

/// A refusal names the first few volumes and counts the rest, and carries the
/// whole list beside the sentence.
#[test]
fn a_refusal_naming_four_volumes_ends_its_list_with_how_many_more() {
    let dir = tempdir("relocate-four");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let volumes: Vec<(&str, &str, usize)> = ["v01", "v02", "v03", "v04"]
        .into_iter()
        .map(|v| ("Series", v, 2))
        .collect();
    let moving = a_dir(&conn, &dir.path().join("a"), "Chapters", None, &volumes);
    a_dir(&conn, &dir.path().join("b"), "Manga", None, &volumes);
    scanned(&conn);

    let (why, named) = refusal(&conn, &moving, library_called(&conn, "Manga"));

    assert_eq!(named.len(), 4, "{named:?}");
    assert!(why.contains(" and 1 more. Nothing was changed"), "{why}");
}

/// The target holds `Series v02` of the same title; the moved work joins that
/// series and keeps its own row.
#[test]
fn the_works_of_a_series_the_target_holds_join_it() {
    let dir = tempdir("relocate-series-joins");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "A",
        None,
        &[("Series", "v01", 2)],
    );
    let target = a_dir(
        &conn,
        &dir.path().join("b"),
        "B",
        None,
        &[("Series", "v02", 3)],
    );
    scanned(&conn);
    let (id, slug, _) = the_work(&conn, "Series%");
    let held: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            target.library_id
        ),
    );
    assert_ne!(id, held, "the fixture: two works in two libraries");

    let report = moved(&conn, &moving, Target::Library(target.library_id));

    let series_of = |work: i64| -> i64 {
        scalar(
            &conn,
            &format!("SELECT series_id FROM work WHERE id = {work}"),
        )
    };
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM work WHERE id = {id} AND slug = '{slug}'")
        ),
        1,
        "the moved work kept its id and slug"
    );
    assert_eq!(series_of(id), series_of(held));
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM series WHERE id = {}", series_of(id))
        ),
        target.library_id
    );
    assert_eq!(report.series_joined, 1);
    assert_eq!(report.works_kept, 1);
}

/// Some of the series' works stay behind, so the series stays with them and
/// the new library gets one of that title.
#[test]
fn a_series_that_keeps_works_behind_stays_and_the_target_gets_one_of_its_title() {
    let dir = tempdir("relocate-series-stays");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("a"),
        "A",
        None,
        &[("Series", "v01", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("b"),
        "A",
        None,
        &[("Series", "v02", 3)],
    );
    scanned(&conn);
    let (id, slug, _) = the_work(&conn, "Series%v01%");
    let behind: i64 = scalar(&conn, "SELECT id FROM work WHERE volume = 2.0");
    let behind_series: i64 = scalar(
        &conn,
        &format!("SELECT series_id FROM work WHERE id = {behind}"),
    );

    let report = moved(&conn, &moving, Target::New("Elsewhere"));

    let elsewhere = library_called(&conn, "Elsewhere");
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM work WHERE id = {id} AND slug = '{slug}'")
        ),
        1
    );
    let series: i64 = scalar(
        &conn,
        &format!("SELECT series_id FROM work WHERE id = {id}"),
    );
    assert_ne!(series, behind_series);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM series WHERE id = {series}")
        ),
        elsewhere
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM series a JOIN series b ON b.id = {behind_series}
                  WHERE a.id = {series} AND a.sort_title = b.sort_title"
            )
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT series_id FROM work WHERE id = {behind}")
        ),
        behind_series
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM series WHERE id = {behind_series}")
        ),
        moving.library_id
    );
    assert_eq!(report.series_joined, 0);
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
    let (id, slug, _) = the_work(&conn, "Moved%");
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
    let (again, same_slug, _) = the_work(&conn, "By Hand");
    assert_eq!((again, same_slug), (id, slug));
    let same_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("a key");
    assert_eq!(same_key, key);
}

/// The work's hub moves to a library that holds the volume, and the work's
/// other edition stays. The row would have to go and
/// be folded away at once.
#[test]
fn a_divided_work_whose_hub_moves_to_a_library_holding_its_volume_is_refused() {
    let dir = tempdir("relocate-third-refusal");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("hub"),
        "A",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    a_dir(
        &conn,
        &dir.path().join("raw"),
        "A",
        None,
        &[("Series", "v01", 3)],
    );
    let target = a_dir(
        &conn,
        &dir.path().join("held"),
        "B",
        None,
        &[("Series", "v01", 4)],
    );
    scanned(&conn);
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM work WHERE library_id = {} AND hub = 'en'",
                moving.library_id
            )
        ),
        1,
        "the fixture: one work in A, its hub the directory that moves"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM work WHERE library_id = {}",
                moving.library_id
            )
        ),
        1
    );
    let before = dump(&conn);

    for attempt in [
        move_cost(&conn, moving.id, Target::Library(target.library_id)),
        move_root(&conn, moving.id, Target::Library(target.library_id)),
    ] {
        assert!(
            matches!(&attempt, Err(MoveError::Refused { volumes, .. }) if !volumes.is_empty()),
            "{attempt:?}"
        );
        assert_eq!(dump(&conn), before);
    }
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
    let libraries = |series: i64| -> Option<i64> {
        conn.query_row(
            "SELECT library_id FROM series WHERE id = ?1",
            [series],
            |r| r.get(0),
        )
        .ok()
        .flatten()
    };
    println!(
        "O190 series libraries: subject {:?}, target {:?}",
        libraries(proposal.subject_id),
        proposal.target_id.and_then(libraries)
    );
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

/// A volume the target holds under another edition key, and a hand alignment
/// the fold cannot carry: the work cannot be joined without losing it, so
/// the move is refused and nothing changes. The alignment names a key the
/// work holds no edition of (`ghost`), the first of `plan_alignments`'s
/// left-behind cases and the one a fixture can make without a hand-corrected
/// run in a sidecar; the arm under test reads the derive's count, not which
/// case raised it.
#[test]
fn a_join_that_loses_an_alignment_the_fold_cannot_carry_is_refused() {
    let dir = tempdir("relocate-lost-alignment");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("chapters"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let volumes = a_dir(
        &conn,
        &dir.path().join("volumes"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    let joining: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            moving.library_id
        ),
    );
    conn.execute(
        "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'chapters', 'ghost', 'hand')",
        [joining],
    )
    .expect("an alignment to a key with no edition");
    let before = dump(&conn);

    for attempt in [
        move_cost(&conn, moving.id, Target::Library(volumes.library_id)),
        move_root(&conn, moving.id, Target::Library(volumes.library_id)),
    ] {
        assert!(
            matches!(&attempt, Err(MoveError::Refused { why, volumes })
                if why.contains("would lose a position, a note, an alignment")
                    && volumes.len() == 1),
            "{attempt:?}"
        );
    }
    assert_eq!(dump(&conn), before);
    assert_eq!(scalar(&conn, "SELECT count(*) FROM alignment"), 1);
}

/// A work put in a series by hand, in a series whose other work stays.
#[test]
fn a_work_put_in_a_series_by_hand_that_keeps_works_behind_refuses_the_move() {
    let top = tempdir("relocate-hand-series-behind");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &top.path().join("x"),
        "A",
        None,
        &[("Alpha", "v01", 2)],
    );
    a_dir(
        &conn,
        &top.path().join("y"),
        "A",
        None,
        &[("Beta", "v01", 3)],
    );
    scanned(&conn);
    let (alpha, _, _) = the_work(&conn, "Alpha%");
    let (beta, _, _) = the_work(&conn, "Beta%");
    let z: i64 = scalar(
        &conn,
        &format!("SELECT series_id FROM work WHERE id = {beta}"),
    );
    conn.execute(
        "UPDATE work SET series_id_hand = ?2 WHERE id = ?1",
        [alpha, z],
    )
    .expect("placed by hand");
    derive::derive(&conn).expect("derives");
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT series_id FROM work WHERE id = {alpha}")
        ),
        z,
        "the fixture: the derive leaves the hand placement standing"
    );
    let held = dir(&conn, moving.id).expect("the directory");
    let target = resolve_for_test(&conn);
    let before = dump(&conn);
    let (why, volumes) = match refused_before(&conn, &held, target).expect("reads") {
        Some(MoveError::Refused { why, volumes }) => (why, volumes),
        other => panic!("not refused: {other:?}"),
    };
    assert!(why.contains("put in a series by hand"), "{why}");
    assert_eq!(volumes.len(), 1, "{volumes:?}");
    assert!(
        volumes[0].starts_with("Alpha") && volumes[0].ends_with("(raw)"),
        "{volumes:?}"
    );
    for attempt in [
        move_cost(&conn, moving.id, Target::Library(target)),
        move_root(&conn, moving.id, Target::Library(target)),
    ] {
        assert!(
            matches!(&attempt, Err(MoveError::Refused { volumes, .. }) if volumes.len() == 1),
            "{attempt:?}"
        );
    }
    assert_eq!(dump(&conn), before);
}

/// The same placement where every work of the series moves: the series goes
/// whole, and the hand placement stands.
#[test]
fn a_work_put_in_a_series_by_hand_that_moves_whole_is_not_refused() {
    let dir = tempdir("relocate-hand-series-whole");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &dir.path().join("x"),
        "A",
        None,
        &[("Alpha", "v01", 2), ("Beta", "v01", 3)],
    );
    a_dir(
        &conn,
        &dir.path().join("y"),
        "A",
        None,
        &[("Else", "v01", 4)],
    );
    scanned(&conn);
    let (alpha, _, _) = the_work(&conn, "Alpha%");
    let (beta, _, _) = the_work(&conn, "Beta%");
    let z: i64 = scalar(
        &conn,
        &format!("SELECT series_id FROM work WHERE id = {beta}"),
    );
    conn.execute(
        "UPDATE work SET series_id_hand = ?2 WHERE id = ?1",
        [alpha, z],
    )
    .expect("placed by hand");
    derive::derive(&conn).expect("derives");

    let report = moved(&conn, &moving, Target::New("Elsewhere"));

    let elsewhere = library_called(&conn, "Elsewhere");
    assert_eq!(report.works_kept, 2);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT series_id FROM work WHERE id = {alpha}")
        ),
        z
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT series_id FROM work WHERE id = {beta}")
        ),
        z
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM series WHERE id = {z}")
        ),
        elsewhere
    );
}

/// A ground-truth page on the target's work under the key the moved edition
/// arrives as, which no edition there holds yet: the fold meets it and one
/// of the two stops existing. Nothing earlier in the verdict sees it.
#[test]
fn a_move_that_would_lose_a_ground_truth_page_is_refused_by_the_count() {
    let (dir, library) = (
        tempdir("relocate-lost-truth"),
        Library::open_in_memory().expect("open"),
    );
    let conn = library.conn();
    let (moving, volumes, joining, target) = a_chapters_and_a_volume(&conn, dir.path());
    for work in [joining, target] {
        conn.execute(
            "INSERT INTO truth_page (work_id, edition, page) VALUES (?1, 'chapters', 1)",
            [work],
        )
        .expect("a page under that key on each");
    }
    let before = dump(&conn);

    let refused = move_root(&conn, moving.id, Target::Library(volumes.library_id));

    assert!(
        matches!(&refused, Err(MoveError::Refused { why, .. }) if why.contains("1 ground-truth page(s) would stop existing")),
        "{refused:?}"
    );
    assert_eq!(dump(&conn), before);
}

/// The same for a note: one id held under the key on both works.
#[test]
fn a_move_that_would_lose_a_note_is_refused_naming_what_was_lost() {
    let (dir, library) = (
        tempdir("relocate-lost-note"),
        Library::open_in_memory().expect("open"),
    );
    let conn = library.conn();
    let (moving, volumes, joining, target) = a_chapters_and_a_volume(&conn, dir.path());
    for work in [joining, target] {
        conn.execute(
            "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
             VALUES (1, 'same', ?1, 'chapters', 0, 'k', 'now', 'now')",
            [work],
        )
        .expect("a note under that key on each");
    }
    let before = dump(&conn);

    let refused = move_root(&conn, moving.id, Target::Library(volumes.library_id));

    assert!(
        matches!(&refused, Err(MoveError::Refused { why, .. }) if why.contains("would lose a position, a note, an alignment")),
        "{refused:?}"
    );
    assert_eq!(dump(&conn), before);
}

/// A note id one reader holds on both works, under different keys: the fold
/// refuses to join them, and the move is refused by the verdict's own arm,
/// naming the work.
#[test]
fn a_work_that_cannot_be_joined_is_refused_naming_it() {
    let (dir, library) = (
        tempdir("relocate-unjoinable"),
        Library::open_in_memory().expect("open"),
    );
    let conn = library.conn();
    let (moving, volumes, joining, target) = a_chapters_and_a_volume(&conn, dir.path());
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'same', ?1, 'chapters', 0, 'k', 'now', 'now')",
        [joining],
    )
    .expect("a note");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'same', ?1, 'raw', 0, 'k', 'now', 'now')",
        [target],
    )
    .expect("a note with the same id");
    let before = dump(&conn);

    let refused = move_root(&conn, moving.id, Target::Library(volumes.library_id));

    assert!(
        matches!(&refused, Err(MoveError::Refused { why, volumes })
            if why.contains("a work could not be joined to the one that holds its volume")
                && !why.contains("nothing was deleted")
                && volumes == &vec!["Series - v01 (chapters)".to_string()]),
        "{refused:?}"
    );
    assert_eq!(dump(&conn), before);
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("makes");
    for entry in std::fs::read_dir(from).expect("lists").flatten() {
        let path = entry.path();
        let into = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &into);
        } else {
            std::fs::copy(&path, &into).expect("copies");
        }
    }
}

/// A volume-less work with the same content key in both libraries. Filing it
/// to the target would break `work`'s uniqueness on (content key, library) and
/// fail the whole move with a store error; left where it is, the derive tries
/// to join it to the target's work, cannot (the fold finds no digest-identical
/// edition) and the verdict refuses the move, naming it.
#[test]
fn a_work_with_the_same_content_key_in_the_target_is_refused_and_not_a_store_fault() {
    let dir = tempdir("relocate-same-bytes");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let a = dir.path().join("a");
    write_pages(&a.join("Oneshot").join("Some Oneshot"), 2);
    copy_tree(&a, &dir.path().join("b"));
    let moving = scan::add_root_in(&conn, &a, InLibrary::Named("A")).expect("adds");
    let target =
        scan::add_root_in(&conn, &dir.path().join("b"), InLibrary::Named("B")).expect("adds");
    scanned(&conn);
    assert_eq!(
        scalar(&conn, "SELECT count(DISTINCT content_key) FROM work"),
        1,
        "the fixture: two works, one content key"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 2);
    let before = dump(&conn);

    for attempt in [
        move_cost(&conn, moving.id, Target::Library(target.library_id)),
        move_root(&conn, moving.id, Target::Library(target.library_id)),
    ] {
        assert!(
            matches!(&attempt, Err(MoveError::Refused { why, .. })
                if why.contains("a work could not be joined")),
            "{attempt:?}"
        );
    }
    assert_eq!(dump(&conn), before);
}

/// `Census` counts a member whose directory is gone (`root_id` NULL) as
/// elsewhere, so the work is divided and not wholly under the directory.
#[test]
fn a_census_counts_a_member_under_no_dir_as_elsewhere() {
    let top = tempdir("relocate-census-null");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_dir(
        &conn,
        &top.path().join("a"),
        "Manga",
        None,
        &[("Series", "v01", 2)],
    );
    let other = a_dir(
        &conn,
        &top.path().join("b"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    let away = a_dir(
        &conn,
        &top.path().join("c"),
        "Other",
        None,
        &[("Else", "v01", 2)],
    );
    scanned(&conn);
    conn.execute(
        "UPDATE member SET root_id = NULL WHERE root_id = ?1",
        [other.id],
    )
    .expect("a member whose directory is gone");
    let held = dir(&conn, moving.id).expect("the directory");

    let census = Census::take(&conn, &held, away.library_id).expect("takes");

    assert_eq!((census.whole.len(), census.divided.len()), (0, 1));
}

/// A chapters directory and a volumes directory holding the same volume, scanned;
/// the work of each.
fn a_chapters_and_a_volume(conn: &Connection, dir: &Path) -> (Root, Root, i64, i64) {
    let moving = a_dir(
        conn,
        &dir.join("chapters"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let volumes = a_dir(
        conn,
        &dir.join("volumes"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(conn);
    let joining = scalar(
        conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            moving.library_id
        ),
    );
    let target = scalar(
        conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            volumes.library_id
        ),
    );
    (moving, volumes, joining, target)
}

fn resolve_for_test(conn: &Connection) -> i64 {
    crate::libraries::create(conn, "Elsewhere").expect("makes a library")
}

/// O191's shape: a work across two directories of one library, the directory that
/// does not hold its hub given a library of its own by hand and derived (what
/// migration 15's split leaves). W (Manga) keeps `en`, its hub over directory
/// `hub`, and a kept `raw` over directory `raw`'s files, with a position and a
/// note on each and a hand title; N (Bees) holds `raw` over the same files.
/// Answers (hub directory, raw directory, W, N, Bees).
fn o191(conn: &Connection, dir: &Path) -> (Root, Root, i64, i64, i64) {
    let hub = a_dir(
        conn,
        &dir.join("hub"),
        "Manga",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    let raw = a_dir(
        conn,
        &dir.join("raw"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(conn);
    let (w, _, _) = the_work(conn, "Series%");
    a_position_and_a_note(conn, w, "en");
    a_position_and_a_note(conn, w, "raw");
    conn.execute(
        "UPDATE work SET title = 'By Hand', title_hand = 'By Hand' WHERE id = ?1",
        [w],
    )
    .expect("a hand title");
    let bees = crate::libraries::create(conn, "Bees").expect("a library");
    conn.execute(
        "UPDATE root SET library_id = ?1 WHERE id = ?2",
        [bees, raw.id],
    )
    .expect("by hand");
    derive::derive(conn).expect("derives");
    let n: i64 = scalar(
        conn,
        &format!("SELECT id FROM work WHERE library_id = {bees}"),
    );
    (hub, raw, w, n, bees)
}

fn both_refused(conn: &Connection, root: &Root, target: Target<'_>) -> (String, Vec<String>) {
    let before = dump(conn);
    let mut answers = Vec::new();
    for attempt in [
        move_cost(conn, root.id, target),
        move_root(conn, root.id, target),
    ] {
        match attempt {
            Err(MoveError::Refused { why, volumes }) => answers.push((why, volumes)),
            other => panic!("not refused: {other:?}"),
        }
        assert_eq!(dump(conn), before, "a refused move changed the store");
    }
    assert_eq!(
        answers[0], answers[1],
        "the rehearsal and the move answer alike"
    );
    answers.remove(0)
}

/// O191, moving the directory that holds W's hub into Bees, where N holds W's
/// other edition's files: the derive would fold W into N, and W's row, slug
/// and hand title would go while the answer said "split". Refused, naming W.
#[test]
fn a_divided_work_the_derive_would_fold_away_refuses_the_move() {
    let dir = tempdir("relocate-o191-hub");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (hub, _, w, _, bees) = o191(&conn, dir.path());

    let (why, volumes) = both_refused(&conn, &hub, Target::Library(bees));

    assert!(
        why.contains("would not stand where its first edition goes"),
        "{why}"
    );
    assert_eq!(volumes, vec!["By Hand (en)".to_string()]);
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM work WHERE id = {w}")),
        1
    );
}

/// The same with documents kept: W's document is not orphaned, because
/// nothing was written.
#[test]
fn a_divided_work_the_derive_would_fold_away_keeps_its_document_when_refused() {
    let dir = tempdir("relocate-o191-docs");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let hub = a_dir(
        &conn,
        &dir.path().join("hub"),
        "Manga",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    let raw = a_dir(
        &conn,
        &dir.path().join("raw"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    for root in [&hub, &raw] {
        scan::update_root(
            &conn,
            root.id,
            &RootPatch {
                keeps_documents: Some(true),
                ..Default::default()
            },
        )
        .expect("keeps documents");
    }
    scanned(&conn);
    let (w, _, _) = the_work(&conn, "Series%");
    a_position_and_a_note(&conn, w, "en");
    let mut doc = crate::librarydoc::of(&conn, w).expect("projects");
    doc.title = Some("By Hand".to_string());
    crate::librarydoc::apply(&conn, w, &doc).expect("applies");
    let document = crate::hand::save(&conn, w)
        .expect("saves")
        .path
        .expect("an address");
    let bees = crate::libraries::create(&conn, "Bees").expect("a library");
    conn.execute(
        "UPDATE root SET library_id = ?1 WHERE id = ?2",
        [bees, raw.id],
    )
    .expect("by hand");
    derive::derive(&conn).expect("derives");

    both_refused(&conn, &hub, Target::Library(bees));

    assert!(document.is_file(), "W's document is where it was");
    assert_eq!(
        crate::hand::path_for(&conn, w).expect("reads"),
        Some(document)
    );
}

/// O191, moving the raw directory to a library of its own: N, which holds the
/// directory's files in Bees, goes with it, and W's `raw` would replace N's. N's
/// edition carries a label, kind, language and chapter number a person set,
/// which the replacement would delete. Refused, in the rehoming's words.
#[test]
fn an_edition_a_person_has_edited_is_not_replaced_by_a_split() {
    let dir = tempdir("relocate-o191-edited");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_, raw, _, n, _) = o191(&conn, dir.path());
    conn.execute(
        "UPDATE edition SET label = 'Japanese raw', label_hand = 'Japanese raw',
                kind = 'raw', kind_hand = 'raw', language = 'ja', language_hand = 'ja'
          WHERE work_id = ?1 AND key = 'raw'",
        [n],
    )
    .expect("a label");
    conn.execute(
        "UPDATE member SET chapter_declared = 7, chapter_source = 'hand'
          WHERE edition_id = (SELECT id FROM edition WHERE work_id = ?1 AND key = 'raw')",
        [n],
    )
    .expect("a chapter number by hand");
    derive::derive(&conn).expect("an ordinary derive keeps them");

    let (why, volumes) = both_refused(&conn, &raw, Target::New("Third"));

    assert!(
        why.contains("already holds a label, kind or language set by hand on edition `raw`"),
        "{why}"
    );
    assert!(!why.contains("nothing was deleted"), "said once: {why}");
    assert_eq!(volumes, vec!["By Hand (raw)".to_string()]);
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM edition WHERE work_id = {n} AND label_hand = 'Japanese raw'"
            )
        ),
        1
    );
}

/// A reader's position on N's `raw`, read since the split: the rehoming's
/// refusal, in its own words and not as "no work to go to".
#[test]
fn a_position_read_on_the_other_works_edition_is_named_in_the_refusal() {
    let dir = tempdir("relocate-o191-third-pos");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_, raw, _, n, _) = o191(&conn, dir.path());
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [n],
    )
    .expect("a position read since the split");

    let (why, _) = both_refused(&conn, &raw, Target::New("Third"));

    assert!(
        why.contains("already holds 1 progress row(s) on edition `raw`"),
        "{why}"
    );
}

/// Case 3 and case 5 at once: the hub stays, and the edition that moves goes
/// to a library that holds the volume under another key.
#[test]
fn a_divided_work_whose_other_edition_joins_a_volume_the_target_holds_under_another_key() {
    let dir = tempdir("relocate-3-and-5");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_dir(
        &conn,
        &dir.path().join("hub"),
        "Manga",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    let moving = a_dir(
        &conn,
        &dir.path().join("raw"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    let vols = a_dir(
        &conn,
        &dir.path().join("vols"),
        "Vols",
        Some("vol"),
        &[("Series", "v01", 4)],
    );
    scanned(&conn);
    let (w, _, _) = the_work(&conn, "Series%v01%");
    let w: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE id = {w} AND library_id IS NOT {}",
            vols.library_id
        ),
    );
    let t: i64 = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE library_id = {}", vols.library_id),
    );
    a_position_and_a_note(&conn, w, "en");
    a_position_and_a_note(&conn, w, "raw");
    a_position_and_a_note(&conn, t, "vol");

    let report = moved(&conn, &moving, Target::Library(vols.library_id));

    let on = |table: &str, work: i64, key: &str| {
        scalar(
            &conn,
            &format!("SELECT count(*) FROM {table} WHERE work_id = {work} AND edition = '{key}'"),
        )
    };
    assert_eq!(
        (
            report.works_split,
            report.series_joined,
            report.positions,
            report.notes
        ),
        (1, 1, 1, 1)
    );
    for table in ["progress", "note"] {
        assert_eq!(
            on(table, t, "raw"),
            1,
            "{table}: `raw` went to the target's work"
        );
        assert_eq!(on(table, t, "vol"), 1, "{table}: `vol` stayed");
        assert_eq!(on(table, w, "en"), 1, "{table}: `en` stayed");
        assert_eq!(on(table, w, "raw"), 0, "{table}: and `raw` left");
    }
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {t}")
        ),
        2
    );
}

/// A tag on the joining work refuses the move through the discarded-rows
/// count alone: no position, note or alignment is lost.
#[test]
fn a_tag_that_a_join_would_lose_refuses_the_move() {
    let (dir, library) = (
        tempdir("relocate-lost-tag"),
        Library::open_in_memory().expect("open"),
    );
    let conn = library.conn();
    let (moving, volumes, joining, _) = a_chapters_and_a_volume(&conn, dir.path());
    conn.execute("INSERT INTO tag (name) VALUES ('kept')", [])
        .expect("a tag");
    conn.execute(
        "INSERT INTO work_tag (work_id, tag_id) VALUES (?1, last_insert_rowid())",
        [joining],
    )
    .expect("a work tag");

    let (why, _) = both_refused(&conn, &moving, Target::Library(volumes.library_id));

    assert!(why.contains("another row a reader made"), "{why}");
}

/// The store holds a change a derive has not made yet, and making it would
/// lose a reader's tag: the directory being moved has nothing to do with it, and the
/// move is refused rather than commit that derive's work unreported.
#[test]
fn a_move_beside_a_derive_that_would_lose_a_row_is_refused() {
    let dir = tempdir("relocate-baseline-loss");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let chapters = a_dir(
        &conn,
        &dir.path().join("chapters"),
        "A",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let bystander = a_dir(
        &conn,
        &dir.path().join("else"),
        "A",
        None,
        &[("Else", "v01", 4)],
    );
    let volumes = a_dir(
        &conn,
        &dir.path().join("volumes"),
        "B",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(&conn);
    // Whichever of the two works the derive folds away holds a tag.
    conn.execute("INSERT INTO tag (name) VALUES ('kept')", [])
        .expect("a tag");
    let tag = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO work_tag (work_id, tag_id) SELECT id, ?1 FROM work WHERE title LIKE 'Series%'",
        [tag],
    )
    .expect("a work tag on each");
    conn.execute(
        "UPDATE root SET library_id = ?1 WHERE id = ?2",
        [chapters.library_id, volumes.id],
    )
    .expect("by hand, with no derive after it");
    let elsewhere = crate::libraries::create(&conn, "Elsewhere").expect("a library");

    let (why, _) = both_refused(&conn, &bystander, Target::Library(elsewhere));

    assert!(why.contains("not yet derived"), "{why}");
    assert!(why.contains("Scan"), "{why}");
}

/// The second pass, after the commit, reconciles documents: a document that
/// went missing is written back, which the rehearsal never does.
#[test]
fn a_move_writes_back_a_document_that_went_missing() {
    let dir = tempdir("relocate-reconciles");
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
    let (id, _, _) = the_work(&conn, "Moved%");
    let mut doc = crate::librarydoc::of(&conn, id).expect("projects");
    doc.title = Some("By Hand".to_string());
    crate::librarydoc::apply(&conn, id, &doc).expect("applies");
    let document = crate::hand::save(&conn, id)
        .expect("saves")
        .path
        .expect("an address");
    std::fs::remove_file(&document).expect("a document that went missing");

    move_cost(&conn, moving.id, Target::New("Elsewhere")).expect("rehearses");
    assert!(!document.exists(), "a rehearsal writes nothing");
    let (_, derived) = move_root(&conn, moving.id, Target::New("Elsewhere")).expect("moves");

    assert!(document.is_file(), "the move's second pass wrote it back");
    assert_eq!(derived.repaired_documents, 1);
}

/// A store whose pending derive folds one work into another with a reader's
/// row on the same key: the `chapters` directory was given the target's library
/// by hand and no derive has run since. The bystander directory is the one moved.
/// Answers the directory and the library to move it to.
fn a_pending_fold_that_loses(conn: &Connection, dir: &Path, row: &str) -> (Root, i64) {
    let chapters = a_dir(
        conn,
        &dir.join("chapters"),
        "A",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let volumes = a_dir(
        conn,
        &dir.join("volumes"),
        "B",
        None,
        &[("Series", "v01", 3)],
    );
    let bystander = a_dir(conn, &dir.join("else"), "A", None, &[("Else", "v01", 4)]);
    scanned(conn);
    let joining: i64 = scalar(
        conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND title LIKE 'Series%'",
            chapters.library_id
        ),
    );
    let target: i64 = scalar(
        conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {}",
            volumes.library_id
        ),
    );
    for work in [joining, target] {
        match row {
            "alignment" if work == target => continue,
            "alignment" => conn.execute(
                "INSERT INTO alignment (work_id, from_key, to_key, by)
                 VALUES (?1, 'chapters', 'ghost', 'hand')",
                [work],
            ),
            "position" => conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'chapters', 1)",
                [work],
            ),
            _ => conn.execute(
                "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
                 VALUES (1, 'same', ?1, 'chapters', 0, 'k', 'now', 'now')",
                [work],
            ),
        }
        .expect("a row under that key on each");
    }
    conn.execute(
        "UPDATE root SET library_id = ?1 WHERE id = ?2",
        [volumes.library_id, chapters.id],
    )
    .expect("by hand, with no derive after it");
    let elsewhere = crate::libraries::create(conn, "Elsewhere").expect("a library");
    (bystander, elsewhere)
}

#[test]
fn a_move_beside_a_derive_that_would_lose_a_position_is_refused() {
    let dir = tempdir("relocate-baseline-position");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (bystander, elsewhere) = a_pending_fold_that_loses(&conn, dir.path(), "position");

    let (why, _) = both_refused(&conn, &bystander, Target::Library(elsewhere));

    assert!(why.contains("not yet derived"), "{why}");
}

/// An alignment the baseline fold cannot carry (it names a key the work holds
/// no edition of, as in `a_join_that_loses_an_alignment_the_fold_cannot_carry_is_refused`).
#[test]
fn a_move_beside_a_derive_that_would_leave_an_alignment_behind_is_refused() {
    let dir = tempdir("relocate-baseline-alignment");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (bystander, elsewhere) = a_pending_fold_that_loses(&conn, dir.path(), "alignment");

    let (why, _) = both_refused(&conn, &bystander, Target::Library(elsewhere));

    assert!(why.contains("not yet derived"), "{why}");
}

#[test]
fn first_few_names_three_and_counts_the_rest() {
    let volumes = |n: usize| -> Vec<String> { (1..=n).map(|i| format!("v{i}")).collect() };
    assert_eq!(first_few(&volumes(1)), "v1");
    assert_eq!(first_few(&volumes(3)), "v1, v2, v3");
    assert_eq!(first_few(&volumes(4)), "v1, v2, v3 and 1 more");
    assert_eq!(first_few(&volumes(7)), "v1, v2, v3 and 4 more");
}

#[test]
fn a_move_beside_a_derive_that_would_drop_a_note_is_refused() {
    let dir = tempdir("relocate-baseline-note");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (bystander, elsewhere) = a_pending_fold_that_loses(&conn, dir.path(), "note");

    let (why, _) = both_refused(&conn, &bystander, Target::Library(elsewhere));

    assert!(why.contains("not yet derived"), "{why}");
}

/// O191's shape with a third edition: W holds `en` (its hub, directory `hub`),
/// `fr` (directory `fr`) and a kept `raw` over directory `raw`'s files, which
/// belong to Bees; N (Bees) holds `raw`. Answers (the `fr` directory, W, N).
fn o191_with_a_french_edition(conn: &Connection, dir: &Path) -> (Root, i64, i64) {
    a_dir(
        conn,
        &dir.join("hub"),
        "Manga",
        Some("en"),
        &[("Series", "v01", 2)],
    );
    let fr = a_dir(
        conn,
        &dir.join("fr"),
        "Manga",
        Some("fr"),
        &[("Series", "v01", 4)],
    );
    let raw = a_dir(
        conn,
        &dir.join("raw"),
        "Manga",
        None,
        &[("Series", "v01", 3)],
    );
    scanned(conn);
    let (w, _, _) = the_work(conn, "Series%");
    for edition in ["en", "fr", "raw"] {
        a_position_and_a_note(conn, w, edition);
    }
    let bees = crate::libraries::create(conn, "Bees").expect("a library");
    conn.execute(
        "UPDATE root SET library_id = ?1 WHERE id = ?2",
        [bees, raw.id],
    )
    .expect("by hand");
    derive::derive(conn).expect("derives");
    let n: i64 = scalar(
        conn,
        &format!("SELECT id FROM work WHERE library_id = {bees}"),
    );
    assert_eq!(
        scalar(
            conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {w}")
        ),
        3,
        "the fixture: W holds en, fr and the kept raw"
    );
    (fr, w, n)
}

/// Moving `fr` carries `fr` and nothing else: `raw` was left in Bees by an
/// earlier change, and this move is not about it.
#[test]
fn a_move_carries_only_the_editions_it_changed_the_library_of() {
    let dir = tempdir("relocate-only-fr");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (fr, w, n) = o191_with_a_french_edition(&conn, dir.path());
    let n_before = scalar(
        &conn,
        &format!("SELECT count(*) FROM edition WHERE work_id = {n}"),
    );

    let report = moved(&conn, &fr, Target::New("French"));

    let french = library_called(&conn, "French");
    assert_eq!(
        (report.works_split, report.positions, report.notes),
        (1, 1, 1),
        "one work, and the rows of the one edition that moved"
    );
    let keys = |work: i64| -> Vec<String> {
        rows(
            &conn,
            &format!("SELECT key FROM edition WHERE work_id = {work} ORDER BY key"),
        )
    };
    assert_eq!(keys(w), ["en", "raw"], "W kept `raw`");
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM edition WHERE work_id = {n}")
        ),
        n_before,
        "and N gained nothing"
    );
    for table in ["progress", "note"] {
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {w} AND edition = 'raw'")
            ),
            1,
            "{table}: `raw`'s row is where it was"
        );
        let twin: i64 = scalar(
            &conn,
            &format!("SELECT id FROM work WHERE library_id = {french}"),
        );
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {twin} AND edition = 'fr'")
            ),
            1,
            "{table}: `fr`'s went with it"
        );
    }
}

fn rows(conn: &Connection, sql: &str) -> Vec<String> {
    let mut statement = conn.prepare(sql).expect(sql);
    statement
        .query_map([], |r| r.get::<_, String>(0))
        .expect(sql)
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect(sql)
}

/// A reader's position on N's `raw` is nothing to the `fr` move: it is not
/// refused for an edition the directory does not hold.
#[test]
fn a_position_on_an_edition_the_move_does_not_touch_does_not_refuse_it() {
    let dir = tempdir("relocate-only-fr-position");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (fr, _, n) = o191_with_a_french_edition(&conn, dir.path());
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [n],
    )
    .expect("a position on N's raw");

    let report = moved(&conn, &fr, Target::New("French"));

    assert_eq!(report.works_split, 1);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {n} AND edition = 'raw'")
        ),
        1
    );
}

/// A spread flag on N's `raw` survives an ordinary derive and nothing writes
/// it again, so replacing the edition would lose it. Refused.
#[test]
fn a_spread_flag_on_the_edition_a_split_would_replace_refuses_the_move() {
    let dir = tempdir("relocate-o191-spread");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_, raw, _, n, _) = o191(&conn, dir.path());
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'spread', 1 FROM edition WHERE work_id = ?1 AND key = 'raw'",
        [n],
    )
    .expect("a spread flag");
    derive::derive(&conn).expect("an ordinary derive");
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM page_flag f JOIN edition e ON e.id = f.edition_id WHERE e.work_id = {n}"
            )
        ),
        1,
        "the fixture: the flag survives a derive"
    );

    let (why, _) = both_refused(&conn, &raw, Target::New("Third"));

    assert!(
        why.contains("already holds a flag on a page on edition `raw`"),
        "{why}"
    );
}

/// The hub's directory moves: W goes with `en`, keeps the `raw` an earlier change
/// left in Bees, and gives `fr`, whose files stay in Manga, to a new work
/// there. N is not the move's business.
#[test]
fn moving_the_hub_carries_the_work_and_gives_away_only_the_edition_left_behind() {
    let dir = tempdir("relocate-hub-moves-fr");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (fr, w, n) = o191_with_a_french_edition(&conn, dir.path());
    let hub = scan::roots(&conn)
        .expect("lists")
        .into_iter()
        .find(|root| root.path.ends_with("hub"))
        .expect("the hub's directory");
    let manga = fr.library_id;
    let n_before = dump(&conn)
        .into_iter()
        .filter(|row| {
            row.starts_with(&format!("edition {n} "))
                || row.starts_with(&format!("progress 1 {n} "))
        })
        .collect::<Vec<_>>();

    let report = moved(&conn, &hub, Target::New("English"));

    let english = library_called(&conn, "English");
    assert_eq!(report.works_split, 1);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM work WHERE id = {w}")
        ),
        english
    );
    assert_eq!(
        rows(
            &conn,
            &format!("SELECT key FROM edition WHERE work_id = {w} ORDER BY key")
        ),
        ["en", "raw"],
        "W keeps `raw`"
    );
    let left: i64 = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE library_id = {manga}"),
    );
    assert_eq!(
        rows(
            &conn,
            &format!("SELECT key FROM edition WHERE work_id = {left}")
        ),
        ["fr"]
    );
    for table in ["progress", "note"] {
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {left} AND edition = 'fr'")
            ),
            1,
            "{table}: `fr`'s row is on the new work"
        );
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {w} AND edition = 'raw'")
            ),
            1,
            "{table}: `raw`'s stayed on W"
        );
    }
    let n_after = dump(&conn)
        .into_iter()
        .filter(|row| {
            row.starts_with(&format!("edition {n} "))
                || row.starts_with(&format!("progress 1 {n} "))
        })
        .collect::<Vec<_>>();
    assert_eq!(n_after, n_before, "N is untouched");
}

/// A fold the derive already refuses before the move, and refuses the same way
/// after it, is not the move's doing: the move is made.
#[test]
fn a_move_beside_a_fold_the_derive_already_refuses_is_made() {
    let dir = tempdir("relocate-baseline-unfoldable");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let chapters = a_dir(
        &conn,
        &dir.path().join("chapters"),
        "A",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let volumes = a_dir(
        &conn,
        &dir.path().join("volumes"),
        "B",
        None,
        &[("Series", "v01", 3)],
    );
    let bystander = a_dir(
        &conn,
        &dir.path().join("else"),
        "A",
        None,
        &[("Else", "v01", 4)],
    );
    scanned(&conn);
    for work in rows(
        &conn,
        "SELECT CAST(id AS TEXT) FROM work WHERE title LIKE 'Series%'",
    ) {
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'chapters', 1)",
            [work.parse::<i64>().expect("an id")],
        )
        .expect("a position under that key on each");
    }
    conn.execute(
        "UPDATE root SET library_id = ?1 WHERE id = ?2",
        [chapters.library_id, volumes.id],
    )
    .expect("by hand, with no derive after it");
    let elsewhere = crate::libraries::create(&conn, "Elsewhere").expect("a library");

    let (_, derived) = move_root(&conn, bystander.id, Target::Library(elsewhere)).expect("moves");

    assert_eq!(
        derived.unfolded_works, 1,
        "the fixture: the fold is refused, before and after"
    );
}

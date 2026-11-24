//! The registry on its own, through the router, and kept current by its
//! refresher.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use taiyaku_library::Library;

use super::{Refresh, Works, complaint, notice};
use crate::test_support::Fixture;

/// A file-backed library. Only a file gives a second connection that is a
/// real second connection; an in-memory library's "working" connection is
/// its shared one.
fn file_library(fixture: &Fixture) -> (Arc<Library>, PathBuf) {
    let db = fixture.path().join("library.db");
    let library = Library::open(&db).expect("opens a file-backed library");
    (Arc::new(library), db)
}

/// Import one scanned work through a *separate* `Library` over the same
/// file, which is another process as far as the registry's own connections
/// can tell, and answer its row id and slug.
fn import_elsewhere(db: &Path, title: &str, sha: &str) -> (i64, String) {
    let other = Library::open(db).expect("a second library over the same file");
    let conn = other.conn();
    let doc = serde_json::json!({
        "taiyaku": 1,
        "work": { "title": title, "reading": "rtl" },
        "editions": { "raw": { "label": "raw", "members": [
            { "path": format!("/abs/shelf/{title} - v01"), "sha256": sha, "pages": 4 }
        ] } },
        "hub": "raw",
        "alignments": []
    });
    let doc = taiyaku_core::Sidecar::from_json(&doc.to_string()).expect("parses");
    let id = taiyaku_library::import::scanned(&conn, &doc, None).expect("imports as a scan");
    let slug = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .expect("reads the slug");
    (id, slug)
}

/// Run `sql`, binding `id` as `?1`, through a separate `Library` over the
/// same file.
fn write_elsewhere(db: &Path, sql: &str, id: i64) {
    let other = Library::open(db).expect("a second library over the same file");
    let changed = other.conn().execute(sql, [id]).expect(sql);
    assert_eq!(changed, 1, "`{sql}` must change one row");
}

#[test]
fn a_fixed_registry_never_reads_the_library_again() {
    let fixture = Fixture::new("registry-fixed");
    let (library, db) = file_library(&fixture);
    let works = Works::fixed(Vec::new());
    let (_, slug) = import_elsewhere(&db, "Arrived", "a1");

    assert_eq!(
        works.refresh_if_moved(&library).expect("answers"),
        Refresh::Unmoved
    );
    assert!(
        works.get(&slug).is_none(),
        "`serve foo/taiyaku.json` serves what it was handed, not what the library gained since"
    );
}

#[test]
fn len_and_is_empty_count_the_works_held() {
    let fixture = Fixture::new("registry-len");
    let (library, db) = file_library(&fixture);
    assert!(Works::fixed(Vec::new()).is_empty());
    import_elsewhere(&db, "One", "o1");
    import_elsewhere(&db, "Two", "t2");
    let works = Works::library(&library).expect("projects the library");
    assert_eq!(works.len(), 2);
    assert!(!works.is_empty());
}

#[test]
fn a_live_registry_serves_a_work_another_process_added_once_refreshed() {
    let fixture = Fixture::new("registry-added");
    let (library, db) = file_library(&fixture);
    let (_, first) = import_elsewhere(&db, "First", "f1");
    let works = Works::library(&library).expect("projects the library");
    assert!(works.get(&first).is_some());

    let (_, second) = import_elsewhere(&db, "Second", "s2");
    assert!(
        works.get(&second).is_none(),
        "`get` never reads the library; only a refresh does"
    );
    assert!(matches!(
        works.refresh_if_moved(&library).expect("refreshes"),
        Refresh::Refreshed { .. }
    ));
    assert_eq!(
        works
            .get(&second)
            .expect("served after the refresh")
            .sidecar()
            .work
            .title,
        "Second"
    );
    assert_eq!(
        works.refresh_if_moved(&library).expect("answers"),
        Refresh::Unmoved,
        "a second refresh with nothing written in between reads nothing"
    );
}

#[test]
fn a_work_deleted_from_the_library_leaves_the_registry_on_refresh() {
    let fixture = Fixture::new("registry-deleted");
    let (library, db) = file_library(&fixture);
    let (_, first) = import_elsewhere(&db, "Kept", "k1");
    let (gone_id, gone) = import_elsewhere(&db, "Gone", "g1");
    let works = Works::library(&library).expect("projects the library");
    assert!(works.get(&gone).is_some());

    write_elsewhere(&db, "DELETE FROM work WHERE id = ?1", gone_id);
    works.refresh_if_moved(&library).expect("refreshes");

    assert!(works.get(&gone).is_none(), "a deleted work is still served");
    assert!(works.get(&first).is_some(), "and the one left must stay");
}

#[test]
fn a_held_work_keeps_its_identity_and_takes_the_new_document() {
    let fixture = Fixture::new("registry-identity");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Before", "b1");
    let works = Works::library(&library).expect("projects the library");
    let held = works.get(&slug).expect("served");

    write_elsewhere(&db, "UPDATE work SET title = 'Retitled' WHERE id = ?1", id);
    works.refresh_if_moved(&library).expect("refreshes");

    let now = works.get(&slug).expect("still served");
    assert!(
        Arc::ptr_eq(&held, &now),
        "a refresh must not mint a second `Work` over one row: its notes lock and \
         its correction lock would then be two locks"
    );
    assert_eq!(held.sidecar().work.title, "Retitled");
}

/// O91: a work a derive marks missing, and one a scan finds again, is the
/// same `Work` each time, so its notes and correction locks survive both.
#[test]
fn a_held_work_takes_the_missing_mark_and_its_clearing_in_place() {
    let fixture = Fixture::new("registry-missing-held");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Marked", "m1");
    let works = Works::library(&library).expect("projects the library");
    let held = works.get(&slug).expect("served");
    assert!(!held.is_missing(), "an unmarked work is served present");

    write_elsewhere(
        &db,
        "UPDATE work SET missing_since = datetime('now') WHERE id = ?1",
        id,
    );
    works.refresh_if_moved(&library).expect("refreshes");
    let marked = works.get(&slug).expect("a missing work is still held");
    assert!(
        Arc::ptr_eq(&held, &marked),
        "a mark must not mint a second `Work` over one row"
    );
    assert!(marked.is_missing(), "the held work carries the mark");

    write_elsewhere(
        &db,
        "UPDATE work SET missing_since = NULL WHERE id = ?1",
        id,
    );
    works.refresh_if_moved(&library).expect("refreshes");
    let found = works.get(&slug).expect("still held");
    assert!(
        Arc::ptr_eq(&held, &found),
        "a work found again must not mint a second `Work` over one row"
    );
    assert!(!found.is_missing(), "the held work loses the mark");
}

/// A work first read already marked is served marked: the startup read and
/// a work a refresh reads for the first time.
#[test]
fn a_work_first_read_marked_is_served_missing() {
    let fixture = Fixture::new("registry-missing-new");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Marked", "m1");
    write_elsewhere(
        &db,
        "UPDATE work SET missing_since = datetime('now') WHERE id = ?1",
        id,
    );
    let works = Works::library(&library).expect("projects the library");
    assert!(
        works
            .get(&slug)
            .expect("a missing work is held")
            .is_missing(),
        "the startup read carries the mark"
    );
    assert!(
        crate::works::from_library(&library)
            .expect("projects the library")
            .iter()
            .find(|work| work.id == slug)
            .expect("projected")
            .is_missing(),
        "`from_library` carries the mark"
    );
}

/// A marked work whose rows will not project is kept as it was read last,
/// and takes the mark all the same: the mark comes from the `work` row,
/// which reads whatever the document does.
#[test]
fn a_work_whose_rows_will_not_project_still_takes_the_mark() {
    let fixture = Fixture::new("registry-missing-kept");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Steady", "s1");
    let works = Works::library(&library).expect("projects the library");

    write_elsewhere(
        &db,
        "UPDATE work SET reading = 'sideways', missing_since = datetime('now') WHERE id = ?1",
        id,
    );
    let refreshed = works.refresh_if_moved(&library).expect("refreshes");
    assert!(
        matches!(&refreshed, Refresh::Refreshed { kept } if kept.len() == 1),
        "the work is kept as it was: {refreshed:?}"
    );
    assert!(
        works.get(&slug).expect("still served").is_missing(),
        "a kept work carries the mark its row has"
    );
}

#[test]
fn a_write_that_lands_while_a_refresh_reads_is_read_by_the_next_one() {
    let fixture = Fixture::new("registry-ordering");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Ordered", "o1");
    let works = Works::library(&library).expect("projects the library");
    write_elsewhere(&db, "UPDATE work SET title = 'Before' WHERE id = ?1", id);

    // The first time it is asked, this reads the work and *then* writes, so
    // the write commits after the refresh has read what it serves.
    let wrote = AtomicBool::new(false);
    let project = |conn: &rusqlite::Connection, work_id: i64| {
        let read = taiyaku_library::project::sidecar(conn, work_id);
        if !wrote.swap(true, Ordering::SeqCst) {
            write_elsewhere(&db, "UPDATE work SET title = 'During' WHERE id = ?1", id);
        }
        read
    };
    works
        .refresh_if_moved_with(&library, &project)
        .expect("refreshes");
    assert_eq!(
        works.get(&slug).expect("served").sidecar().work.title,
        "Before"
    );

    assert!(
        matches!(
            works.refresh_if_moved(&library).expect("refreshes"),
            Refresh::Refreshed { .. }
        ),
        "the write that landed during the last refresh must still be ahead of what it recorded"
    );
    assert_eq!(
        works.get(&slug).expect("served").sidecar().work.title,
        "During"
    );
}

/// **One work is read from one committed state of the library.** This
/// `project` reads the work's title, lets another connection retitle it, and only
/// then projects the work. Inside the read's own transaction the projection
/// sees the state the first read did. Without it, the projection would see the
/// write that landed between the two, and one document could be built from
/// both sides of a write.
#[test]
fn a_work_is_read_from_one_committed_state_of_the_library() {
    let fixture = Fixture::new("registry-one-state");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "First", "f1");
    let works = Works::library(&library).expect("projects the library");
    write_elsewhere(&db, "UPDATE work SET title = 'Before' WHERE id = ?1", id);

    let wrote = AtomicBool::new(false);
    let project = |conn: &rusqlite::Connection, work_id: i64| {
        let _: String = conn
            .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        if !wrote.swap(true, Ordering::SeqCst) {
            write_elsewhere(&db, "UPDATE work SET title = 'After' WHERE id = ?1", id);
        }
        taiyaku_library::project::sidecar(conn, work_id)
    };
    works
        .refresh_if_moved_with(&library, &project)
        .expect("refreshes");
    assert_eq!(
        works.get(&slug).expect("served").sidecar().work.title,
        "Before",
        "the projection saw a write that committed after the work's read began"
    );
}

#[test]
fn a_work_whose_document_moved_is_a_new_work_at_the_new_path() {
    let fixture = Fixture::new("registry-moved");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Moving", "m1");
    let works = Works::library(&library).expect("projects the library");
    let held = works.get(&slug).expect("served");

    let other = Library::open(&db).expect("a second library over the same file");
    other
        .conn()
        .execute(
            "UPDATE work SET sidecar_path = '/new/home/taiyaku.json' WHERE id = ?1",
            [id],
        )
        .expect("moves the document");
    works.refresh_if_moved(&library).expect("refreshes");

    let now = works.get(&slug).expect("still served");
    assert!(
        !Arc::ptr_eq(&held, &now),
        "a moved document keeps the old Work, whose `path` is fixed for its life"
    );
    assert_eq!(now.path, Path::new("/new/home/taiyaku.json"));
}

/// **A correction racing a refresh survives it.** The correction holds the
/// work's write lock across its own write, as `patch_alignments` does. The
/// refresh starts while it is held, and has a reason to read, because an
/// unrelated write has already moved the counter.
///
/// A correct refresh waits on that work's lock *before* reading it, so it
/// reads after the correction commits. A refresh that read first would read
/// the work before the correction's row existed, then swap that stale
/// document in once the lock came free, undoing the correction in memory
/// while the rows still held it.
///
/// `projected` reports when the refresh reads. The 500 ms wait only decides
/// how long a correct refresh is given to *not* read early. The correct path
/// passes whatever the timing, and the defect is caught whenever it reads
/// inside that window.
#[test]
#[expect(
    clippy::result_large_err,
    reason = "`Work::update`'s closure returns `Result<_, Response>`, and a \
              `Response` is 128 bytes; the correction below is shaped like \
              `patch_alignments`' own"
)]
fn a_correction_racing_a_refresh_survives_it() {
    let fixture = Fixture::new("registry-race");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Raced", "r1");
    let works = Arc::new(Works::library(&library).expect("projects the library"));
    let held = works.get(&slug).expect("served");

    let (holding_tx, holding_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (projected_tx, projected_rx) = mpsc::channel::<()>();

    let correction = {
        let held = Arc::clone(&held);
        let db = db.clone();
        std::thread::spawn(move || {
            held.update(|_current| {
                holding_tx.send(()).expect("signals that it holds the lock");
                go_rx.recv().expect("waits to be let go");
                let other = Library::open(&db).expect("a second library");
                let conn = other.conn();
                conn.execute(
                    "INSERT INTO page_flag (edition_id, kind, page) \
                     SELECT id, 'spread', 1 FROM edition WHERE work_id = ?1",
                    [id],
                )
                .expect("writes the correction's row");
                let next = taiyaku_library::project::sidecar(&conn, id)
                    .expect("projects the corrected work");
                Ok((Arc::new(next), ()))
            })
            .expect("the correction lands");
        })
    };
    holding_rx
        .recv()
        .expect("the correction holds the work's lock");

    write_elsewhere(&db, "UPDATE work SET title = title WHERE id = ?1", id);

    let refresh = {
        let works = Arc::clone(&works);
        let library = Arc::clone(&library);
        std::thread::spawn(move || {
            let project = |conn: &rusqlite::Connection, work_id: i64| {
                let read = taiyaku_library::project::sidecar(conn, work_id);
                let _ = projected_tx.send(());
                read
            };
            works
                .refresh_if_moved_with(&library, &project)
                .expect("refreshes")
        })
    };

    let read_early = projected_rx
        .recv_timeout(Duration::from_millis(500))
        .is_ok();
    go_tx.send(()).expect("lets the correction go");
    correction.join().expect("the correction thread");
    let refreshed = refresh.join().expect("the refresh thread");
    assert_eq!(
        refreshed,
        Refresh::Refreshed { kept: Vec::new() },
        "the refresh must have read every work, or this test raced nothing"
    );

    let now = works.get(&slug).expect("still served");
    assert!(
        Arc::ptr_eq(&held, &now),
        "the refresh replaced the work it raced"
    );
    assert_eq!(
        now.sidecar().editions["raw"].spreads,
        vec![1],
        "the refresh undid a correction that committed after it began{}",
        if read_early {
            ": it read the work before taking the work's lock"
        } else {
            ""
        }
    );
}

#[test]
fn a_work_whose_rows_will_not_project_keeps_the_document_it_had() {
    let fixture = Fixture::new("registry-unprojectable");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Steady", "s1");
    let works = Works::library(&library).expect("projects the library");

    write_elsewhere(
        &db,
        "UPDATE work SET reading = 'sideways' WHERE id = ?1",
        id,
    );
    let refreshed = works
        .refresh_if_moved(&library)
        .expect("refreshes the rest");

    let Refresh::Refreshed { kept } = refreshed else {
        panic!("the counter moved, so the refresh must have read: {refreshed:?}");
    };
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].contains(&slug), "{kept:?}");
    assert_eq!(
        works.get(&slug).expect("still served").sidecar().work.title,
        "Steady",
        "a running server keeps serving the document it had"
    );
}

#[test]
fn a_work_that_will_not_project_is_left_out_at_startup_and_the_rest_is_served() {
    let fixture = Fixture::new("registry-startup-lenient");
    let (library, db) = file_library(&fixture);
    let (broken, broken_slug) = import_elsewhere(&db, "Broken", "b1");
    let (_, steady_slug) = import_elsewhere(&db, "Steady", "s1");
    write_elsewhere(
        &db,
        "UPDATE work SET reading = 'sideways' WHERE id = ?1",
        broken,
    );

    let works = Works::library(&library).expect("startup serves the rest");
    assert!(
        works.get(&broken_slug).is_none(),
        "the broken work is left out"
    );
    assert_eq!(
        works
            .get(&steady_slug)
            .expect("the rest is served")
            .sidecar()
            .work
            .title,
        "Steady"
    );
}

#[test]
fn a_complaint_is_printed_once_and_its_recovery_once() {
    let mut last = None;
    assert_eq!(
        notice(&mut last, Some("disk gone".to_string())).as_deref(),
        Some("taiyaku: could not refresh the served works: disk gone")
    );
    assert_eq!(notice(&mut last, Some("disk gone".to_string())), None);
    assert_eq!(
        notice(&mut last, None).as_deref(),
        Some("taiyaku: the served works are being refreshed again")
    );
    assert_eq!(notice(&mut last, None), None);
}

/// A database that stays unopenable fails every tick with the same words and
/// a different wait, because opening it says how long it waited. That is one
/// failure, said once, and the line said keeps the figure.
#[test]
fn a_failure_that_differs_only_in_how_long_it_waited_is_said_once() {
    // A real failure to open, for the library's own words around the figure:
    // a file that is not a database opens, and then refuses WAL. Its log
    // goes aside too, or the pages in it would be read in its place.
    let fixture = Fixture::new("registry-waited");
    let (library, db) = file_library(&fixture);
    let works = Works::library(&library).expect("projects an empty library");
    for suffix in ["", "-wal", "-shm"] {
        let from = format!("{}{suffix}", db.display());
        std::fs::rename(&from, format!("{from}.aside")).expect("moves it aside");
    }
    std::fs::write(&db, [b'x'; 4096]).expect("puts something else where it was");
    let real = works
        .refresh_if_moved(&library)
        .expect_err("that is not a database");
    let (head, tail) = real
        .split_once("(waited ")
        .unwrap_or_else(|| panic!("a failure to open says how long it waited: {real}"));
    let (_, rest) = tail
        .split_once(' ')
        .unwrap_or_else(|| panic!("the figure is followed by more words: {real}"));
    let waited = |figure: &str| Some(format!("{head}(waited {figure} {rest}"));

    let mut last = None;
    let first = notice(&mut last, waited("122µs"));
    assert!(
        first
            .as_deref()
            .is_some_and(|line| line.contains("(waited 122µs ")),
        "the line said keeps the figure: {first:?}"
    );
    assert_eq!(
        notice(&mut last, waited("191µs")),
        None,
        "the same failure after a different wait is not said again"
    );
    assert!(
        notice(&mut last, Some(format!("other words (waited 191µs {rest}"))).is_some(),
        "a different failure that also waited is said"
    );
    assert!(
        notice(&mut last, Some("disk gone".to_string())).is_some(),
        "a different failure is said"
    );
}

#[test]
fn a_refresh_that_read_nothing_neither_raises_nor_clears_a_complaint() {
    assert_eq!(complaint(&Ok(Refresh::Unmoved)), None);
    assert_eq!(
        complaint(&Ok(Refresh::Refreshed { kept: Vec::new() })),
        Some(None)
    );
    assert_eq!(
        complaint(&Err("locked".to_string())),
        Some(Some("locked".to_string()))
    );
    let kept = complaint(&Ok(Refresh::Refreshed {
        kept: vec!["`odd`: bad reading".to_string()],
    }));
    assert!(
        matches!(&kept, Some(Some(text)) if text.contains("`odd`")),
        "{kept:?}"
    );
}

#[test]
fn a_refresh_that_failed_is_followed_by_a_read_that_clears_its_complaint() {
    let fixture = Fixture::new("registry-failed-refresh");
    let (library, db) = file_library(&fixture);
    import_elsewhere(&db, "Steady", "st1");
    let works = Works::library(&library).expect("projects the library");

    // The database cannot be opened, so the refresh fails before it has
    // looked at the counter.
    let aside = fixture.path().join("library.aside");
    std::fs::rename(&db, &aside).expect("moves the database aside");
    std::fs::create_dir(&db).expect("puts a directory where it was");
    works
        .refresh_if_moved(&library)
        .expect_err("a directory is not a database");
    assert!(
        works.last_notice.lock().expect("the notice").is_some(),
        "the failure is what was last printed"
    );

    // Restored, with the counter where it was: this must read, not answer
    // `Unmoved`, or the failure stays on the terminal.
    std::fs::remove_dir(&db).expect("removes the directory");
    std::fs::rename(&aside, &db).expect("puts the database back");
    assert_eq!(
        works.refresh_if_moved(&library).expect("refreshes"),
        Refresh::Refreshed { kept: Vec::new() }
    );
    assert_eq!(
        *works.last_notice.lock().expect("the notice"),
        None,
        "the recovery is what was last printed"
    );
}

// --- through the router ----------------------------------------------------

#[tokio::test]
async fn a_volume_derived_after_startup_opens_on_the_first_request_for_it() {
    let (router, state, fixture) = crate::test_support::router_over_a_live_library();
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");
    assert!(
        state.works.get(&slug).is_none(),
        "no refresh has run, so only the lookup's own miss path can find it"
    );

    let response = crate::test_support::get(&router, &format!("/api/works/{slug}")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert!(body.contains("\"editions\""), "{body}");
}

#[tokio::test]
async fn a_route_that_looks_its_work_up_on_the_blocking_pool_finds_it_too() {
    let (router, state, fixture) = crate::test_support::router_over_a_live_library();
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");
    assert!(state.works.get(&slug).is_none());

    let response = crate::test_support::get(&router, &format!("/api/works/{slug}/progress")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    assert_eq!(
        status,
        axum::http::StatusCode::NO_CONTENT,
        "a known work with no position answers 204, an unknown one 404: {body}"
    );
}

#[tokio::test]
async fn a_work_deleted_after_startup_stops_answering_once_refreshed() {
    let (router, state, fixture) = crate::test_support::router_over_a_live_library();
    let slug = state.works.snapshot()[0].id.clone();
    let other = Library::open(&fixture.path().join("db").join("library.db"))
        .expect("a second library over the same file");
    let deleted = other
        .conn()
        .execute("DELETE FROM work WHERE slug = ?1", [&slug])
        .expect("deletes the work");
    assert_eq!(deleted, 1);

    state
        .works
        .refresh_if_moved(&state.library)
        .expect("refreshes");
    let response = crate::test_support::get(&router, &format!("/api/works/{slug}")).await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// **A correction moves the counter, so the next refresh re-projects the
/// work it just corrected.** The sidecar's `ETag` must come out the same, or
/// every correction is followed by a 412 for whoever corrects next.
#[tokio::test]
async fn a_refresh_after_a_correction_leaves_the_sidecars_etag_alone() {
    use axum::http::{Method, StatusCode, header};
    let (router, state, _fixture, slug) =
        crate::test_support::router_over_a_live_library_with_an_edge();
    let uri = format!("/api/works/{slug}");
    let etag_of = |response: &axum::http::Response<axum::body::Body>| {
        response.headers()[header::ETAG]
            .to_str()
            .unwrap()
            .to_owned()
    };
    let first_tag = etag_of(&crate::test_support::get(&router, &uri).await);

    let patch = |tag: String, body: &'static str| {
        let router = router.clone();
        let uri = format!("{uri}/alignments");
        async move {
            use tower::ServiceExt;
            router
                .oneshot(
                    axum::http::Request::builder()
                        .method(Method::PATCH)
                        .uri(uri)
                        .header(header::IF_MATCH, tag)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap()
        }
    };
    let corrected = patch(
        first_tag,
        r#"{"from":"raw","to":"en","a":"2","b":"1","note":"checked"}"#,
    )
    .await;
    let status = corrected.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        crate::test_support::body_string(corrected).await
    );
    let after_correction = etag_of(&crate::test_support::get(&router, &uri).await);

    let refreshed = state
        .works
        .refresh_if_moved(&state.library)
        .expect("refreshes");
    assert_eq!(
        refreshed,
        Refresh::Refreshed { kept: Vec::new() },
        "the correction must have moved the counter, or this test re-projects nothing"
    );
    let after_refresh = etag_of(&crate::test_support::get(&router, &uri).await);
    assert_eq!(
        after_correction, after_refresh,
        "a refresh changed the sidecar's ETag with nothing written"
    );

    let second = patch(
        after_correction,
        r#"{"from":"raw","to":"en","a":"3","b":"2"}"#,
    )
    .await;
    let status = second.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        crate::test_support::body_string(second).await
    );
}

// --- the refresher ---------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refresher_serves_a_new_volume_before_anything_asks_for_it() {
    let (_router, state, fixture) = crate::test_support::router_over_a_live_library();
    let refresher = super::spawn_refresher(Arc::clone(&state), Duration::from_millis(20));
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");

    // `get` never refreshes, so only the refresher can make this true.
    let mut served = false;
    for _ in 0..250 {
        if state.works.get(&slug).is_some() {
            served = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    refresher.abort();
    assert!(
        served,
        "the refresher never picked up a volume another process derived"
    );
}

/// The refresher's tick also looks for a scan another process ran. Its first
/// look takes what it reads as seen, and nothing here can tell when that look
/// has been, so each round writes marks no round wrote before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refresher_raises_ended_for_a_scan_another_handle_wrote() {
    let (_router, state, fixture) = crate::test_support::router_over_a_live_library();
    let before = state.jobs_ended.ended.load(Ordering::SeqCst);
    let refresher = super::spawn_refresher(Arc::clone(&state), Duration::from_millis(20));
    let other = Library::open(&fixture.path().join("db").join("library.db"))
        .expect("a second library over the same file");

    let mut raised = false;
    for round in 0..100 {
        let at = format!("2030-01-01 00:{:02}:{:02}", round / 60, round % 60);
        let changed = other
            .conn()
            .execute(
                "UPDATE root SET last_scan_at = ?1, last_scan_status = 'ok' \
                 WHERE id = (SELECT min(id) FROM root)",
                [&at],
            )
            .expect("writes a directory's marks");
        assert_eq!(changed, 1, "the fixture has one directory");
        tokio::time::sleep(Duration::from_millis(20)).await;
        if state.jobs_ended.ended.load(Ordering::SeqCst) != before {
            raised = true;
            break;
        }
    }
    refresher.abort();
    assert!(
        raised,
        "the refresher never moved ended for a walk another process ended"
    );
}

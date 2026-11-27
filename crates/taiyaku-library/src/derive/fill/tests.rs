use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use rusqlite::trace::{TraceEvent, TraceEventCodes};

use super::{Cadence, early_pass, pass_due, setup};
use crate::derive::joins::tests::{
    RAW, Shelf, VOLUMES, digest, editions, groups, holder, labelled, oshi, oshinoko, pair, raw_at,
    read, shelved, work,
};
use crate::derive::tests::{
    commits_during, member_paths, pause, scalar, scanned_root, standing, translations,
};
use crate::derive::{Placed, held_work, series_id_of};
use crate::progress::{Outcome, Step, Watch};
use crate::scan::tests::{fixture, write_archive, write_archive_labelled, write_pages};
use crate::state::{put_notes, put_progress};
use crate::test_support::{Recording, Report};
use crate::tests::TempDir;
use crate::{Library, NOTES_VERSION, Note, Notes, Progress, derive, scan};

const ALPHA: &str = "Alpha/Alpha v01.cbz";
const BETA: &str = "Beta/Beta v01.cbz";
const GAMMA: &str = "Gamma/Gamma v01";
const C001: &str = "Alpha/Alpha - v01 - c001.cbz";
const ALPHA_V02: &str = "Alpha/Alpha v02.cbz";
const BETA_V02: &str = "Beta/Beta v02.cbz";
const GAMMA_CBZ: &str = "Gamma/Gamma v01.cbz";
const DELTA_C001: &str = "Delta/Delta - v01 - c001.cbz";
const DELTA_C002: &str = "Delta/Delta - v01 - c002.cbz";
const ONESHOT: &str = "Oneshot/Some Oneshot.cbz";

/// Hashes one entry as the filling derive's queue will: the digest
/// `taiyaku_read::member_digest` answers, written onto its `scan_entry` row.
fn hash_now(conn: &Connection, root: &scan::Root, rel_path: &str) {
    let sha256 = taiyaku_read::member_digest(&root.path.join(rel_path))
        .unwrap_or_else(|e| panic!("{rel_path} hashes: {e}"));
    let rows = conn
        .execute(
            "UPDATE scan_entry SET sha256 = ?1 WHERE root_id = ?2 AND rel_path = ?3",
            rusqlite::params![sha256, root.id, rel_path],
        )
        .expect("writes the digest");
    assert_eq!(
        rows, 1,
        "the fixture: {rel_path} has a row under directory {}",
        root.id
    );
}

/// One early pass over every directory, none of them frozen, never cancelled.
fn pass(conn: &Connection) -> Outcome<u64> {
    early_pass(
        conn,
        derive::Documents::Reconcile,
        &setup(conn, &[]).expect("reads the setup"),
        &Recording::default(),
    )
    .expect("passes")
}

/// Every edition key, in key order.
fn edition_keys(conn: &Connection) -> Vec<String> {
    conn.prepare("SELECT key FROM edition ORDER BY key")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .expect("reads the editions")
}

/// The entries under `root` that hold a digest, in path order.
fn hashed_under(conn: &Connection, root: &scan::Root) -> Vec<String> {
    conn.prepare(
        "SELECT rel_path FROM scan_entry WHERE root_id = ?1 AND sha256 IS NOT NULL
          ORDER BY rel_path",
    )
    .and_then(|mut stmt| stmt.query_map([root.id], |r| r.get(0))?.collect())
    .expect("reads the digests")
}

/// Design §8 test 2. The hub is fixed when a work is created, so a volume
/// written with `ja`'s file alone would keep `ja` as its hub once `en`'s
/// arrived, where one derive over both gives `en`, the first-sorting key.
#[test]
fn a_pass_holds_back_a_volume_until_its_file_in_every_directory_is_hashed() {
    let ja = crate::tests::tempdir("fill-held-ja");
    let en = crate::tests::tempdir("fill-held-en");
    // Distinct bytes: `write_archive` labels each page by its path.
    write_archive(&ja.path().join(ALPHA), 2);
    write_archive(&en.path().join(ALPHA), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    // `ja` first, so the directory walked first holds the key sorting last.
    let ja_root = scanned_root(&conn, ja.path(), Some("ja"));
    let en_root = scanned_root(&conn, en.path(), Some("en"));
    hash_now(&conn, &ja_root, ALPHA);

    let first = pass(&conn);

    assert_eq!(first, Outcome::Done(0), "en's file is still to hash");
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work"),
        0,
        "no work is written with ja's file alone"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM series"),
        0,
        "no series shows early holding no work"
    );

    hash_now(&conn, &en_root, ALPHA);
    let second = pass(&conn);

    assert_eq!(second, Outcome::Done(1), "both files are hashed");
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 1, "one volume");
    assert_eq!(edition_keys(&conn), ["en", "ja"], "with both editions");
    let hub: String = conn
        .query_row("SELECT hub FROM work", [], |r| r.get(0))
        .expect("the work");
    assert_eq!(hub, "en", "the hub one derive gives");
}

/// Design §7: a pass opens no file. An entry still to hash is neither hashed
/// nor written, and the pass reports no step: it runs inside the job's
/// Hashing step, whose count its own figures would overwrite.
#[test]
fn a_pass_hashes_nothing_and_reports_nothing() {
    let tree = crate::tests::tempdir("fill-reads-nothing");
    write_archive(&tree.path().join(ALPHA), 2);
    write_archive(&tree.path().join(BETA), 3);
    write_pages(&tree.path().join(GAMMA), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    hash_now(&conn, &root, ALPHA);
    let recording = Recording::default();

    let outcome = early_pass(
        &conn,
        derive::Documents::Reconcile,
        &setup(&conn, &[]).expect("reads the setup"),
        &recording,
    )
    .expect("passes");

    assert_eq!(outcome, Outcome::Done(1), "Alpha, the one volume hashed");
    assert_eq!(
        hashed_under(&conn, &root),
        [ALPHA],
        "Beta's file and Gamma's folder are left unhashed"
    );
    assert_eq!(recording.reports(), [], "a pass reports no step");
}

/// Design §8 test 10: the pass reads a paused directory outside `fresh`
/// frozen, as the full derive does, so a new file there, which waits for that
/// directory's own Scan, holds back nothing, and the full derive leaves the
/// volume as the pass wrote it.
#[test]
fn a_pass_writes_a_volume_without_its_new_file_in_a_frozen_directory() {
    let paused = crate::tests::tempdir("fill-frozen-paused");
    let scanned = crate::tests::tempdir("fill-frozen-scanned");
    write_archive(&paused.path().join(ALPHA), 3);
    write_archive(&scanned.path().join(C001), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    // P first, so the frozen directory is the one read first.
    let p = scanned_root(&conn, paused.path(), None);
    pause(&conn, &p);
    let s = scanned_root(&conn, scanned.path(), None);
    hash_now(&conn, &s, C001);

    let outcome = early_pass(
        &conn,
        derive::Documents::Reconcile,
        &setup(&conn, &[s.id]).expect("reads the setup"),
        &Recording::default(),
    )
    .expect("passes");

    assert_eq!(outcome, Outcome::Done(1), "P's new file holds nothing back");
    assert_eq!(
        member_paths(&conn),
        [scanned.path().join(C001).to_string_lossy()],
        "S's file is the volume's only member"
    );
    let written = standing(&conn);
    derive::derive_watched(
        &conn,
        derive::Documents::Reconcile,
        &[s.id],
        &Recording::default(),
    )
    .expect("derives");
    assert_eq!(
        standing(&conn),
        written,
        "the full derive leaves the volume as the pass wrote it"
    );
}

const HAND_TITLE: &str = "Go-Toubun no Hanayome, volume 1";
const V01: &str = "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019)";
const V02: &str = "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v02 (2019)";

/// Design call 13: a hand document at a new volume's address
/// applies when a pass writes the volume, as at the job's end, so the volume
/// is never shown under the title its files give and renamed later. The
/// second store has never seen the directory, as a rebuilt database has not,
/// so no marker names the document.
#[test]
fn a_pass_applies_a_hand_document_at_a_new_volumes_address() {
    let tree = fixture();
    {
        let first = Library::open_in_memory().expect("open");
        let conn = first.conn();
        let root = scan::add_root(&conn, tree.path()).expect("add");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("first derive");
        let work_id: i64 = conn
            .query_row("SELECT id FROM work WHERE volume = 1.0", [], |r| r.get(0))
            .expect("the volume 1 work");
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
        doc.title = Some(HAND_TITLE.to_string());
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
        let saved = crate::hand::save(&conn, work_id).expect("saving");
        assert!(saved.persisted, "the fixture: the document is on disk");
    }
    let second = Library::open_in_memory().expect("open");
    let conn = second.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    hash_now(&conn, &root, V01);
    hash_now(&conn, &root, V02);

    let outcome = pass(&conn);

    assert_eq!(outcome, Outcome::Done(2), "both volumes are hashed");
    let titles: Vec<(f64, String)> = conn
        .prepare("SELECT volume, title FROM work ORDER BY volume")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the works");
    assert_eq!(
        titles[0],
        (1.0, HAND_TITLE.to_string()),
        "volume 1 takes the hand title"
    );
    assert_eq!(
        titles[1],
        (2.0, "Go-Toubun no Hanayome - v02 (2019)".to_string()),
        "volume 2 keeps its own"
    );
}

/// Each work's content key and members (path, digest), by id.
type Rows = BTreeMap<i64, (String, Vec<(String, String)>)>;

fn rows(conn: &Connection) -> Rows {
    let mut rows: Rows = conn
        .prepare("SELECT id, content_key FROM work")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, Vec::new()))))?
                .collect()
        })
        .expect("reads the works");
    let members: Vec<(i64, String, String)> = conn
        .prepare(
            "SELECT e.work_id, m.path, m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
              ORDER BY e.work_id, m.path",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("reads the members");
    for (work_id, path, sha256) in members {
        rows.get_mut(&work_id)
            .expect("a member's work")
            .1
            .push((path, sha256));
    }
    rows
}

/// The one work in `now` that `was` does not hold, taken out of `now`: its
/// id and its members.
fn the_new_work(now: &mut Rows, was: &Rows) -> (i64, Vec<(String, String)>) {
    let new: Vec<i64> = now
        .keys()
        .filter(|id| !was.contains_key(id))
        .copied()
        .collect();
    let [id] = new[..] else {
        panic!("one new work: {new:?}")
    };
    (id, now.remove(&id).expect("the new work").1)
}

/// Design §8 test 5, modelled on O272's: a pass writes only a volume no work
/// holds, so on a rescan it leaves each existing work to the full derive, its
/// file changed or not, takes no write transaction for it and does not count
/// it. On disk, as `a_derive_over_unchanged_files_writes_nothing` counts,
/// since an in-memory store has no WAL.
#[test]
fn a_pass_writes_no_volume_a_work_already_holds_and_takes_no_lock_for_it() {
    let tree = crate::tests::tempdir("fill-works-wait");
    write_archive(&tree.path().join(ALPHA), 2);
    write_archive(&tree.path().join(BETA), 3);
    let store = crate::tests::tempdir("fill-works-wait-store");
    let library = Library::open(&store.path().join("library.db")).expect("open");
    let mut conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("first derive");
    let beta = tree.path().join(BETA).to_string_lossy().into_owned();
    let old = digest(&conn, &root, BETA);
    // Other bytes, hashed, so rule 1 is not what holds Beta back.
    write_archive(&tree.path().join(BETA), 4);
    scan::scan_root(&conn, &root).expect("rescan");
    hash_now(&conn, &root, BETA);
    let new = digest(&conn, &root, BETA);
    assert_ne!(new, old, "the fixture: Beta's file changed");
    let held_by_beta = |conn: &Connection| -> String {
        conn.query_row("SELECT sha256 FROM member WHERE path = ?1", [&beta], |r| {
            r.get(0)
        })
        .expect("Beta's member")
    };

    let (commits, outcome) = commits_during(&mut conn, pass);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "no volume is new, so none counts"
    );
    assert_eq!(commits, 0, "no write transaction committed");
    assert_eq!(
        held_by_beta(&conn),
        old,
        "Beta's member keeps its old digest"
    );
    derive::derive(&conn).expect("the full derive");
    assert_eq!(held_by_beta(&conn), new, "the full derive rewrites Beta");

    let was = rows(&conn);
    write_archive(&tree.path().join(GAMMA_CBZ), 4);
    scan::scan_root(&conn, &root).expect("rescan");
    hash_now(&conn, &root, GAMMA_CBZ);

    let outcome = pass(&conn);

    assert_eq!(outcome, Outcome::Done(1), "Gamma, the one new volume");
    let mut now = rows(&conn);
    let (_, members) = the_new_work(&mut now, &was);
    assert_eq!(
        members
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>(),
        [tree.path().join(GAMMA_CBZ).to_string_lossy()],
        "Gamma's work holds Gamma's file"
    );
    assert_eq!(now, was, "Alpha's and Beta's rows as before");
}

/// Rule 2's second half: bytes a member holds wait for the full derive, whose
/// import finds a work by content before its address, so a pass writing them
/// as a new volume would take that work from under its own address. One
/// directory, so `place` makes no copy of them. Delta's held bytes are its
/// second file, so a check of the first alone writes Delta.
#[test]
fn a_pass_leaves_a_new_volume_whose_bytes_a_work_already_holds() {
    let tree = crate::tests::tempdir("fill-bytes-held");
    let alpha = tree.path().join(ALPHA);
    write_archive(&alpha, 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("first derive");
    let was = rows(&conn);
    let work_id = *was.keys().next().expect("Alpha's work");
    let placed = work(&conn, work_id);
    let label = alpha.to_string_lossy();
    write_archive_labelled(&tree.path().join(GAMMA_CBZ), 3, "jpg", &label);
    write_archive(&tree.path().join(DELTA_C001), 2);
    write_archive_labelled(&tree.path().join(DELTA_C002), 3, "jpg", &label);
    scan::scan_root(&conn, &root).expect("rescan");
    for file in [GAMMA_CBZ, DELTA_C001, DELTA_C002] {
        hash_now(&conn, &root, file);
    }
    let alpha_digest = digest(&conn, &root, ALPHA);
    assert_eq!(
        [GAMMA_CBZ, DELTA_C002].map(|file| digest(&conn, &root, file)),
        [alpha_digest.clone(), alpha_digest],
        "the fixture: Gamma and Delta's second file hold Alpha's bytes"
    );

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "Gamma's and Delta's bytes are on Alpha's work"
    );
    assert_eq!(
        (work(&conn, work_id), rows(&conn)),
        (placed, was),
        "Alpha's work keeps its series, its key and its member"
    );
}

/// Rule 2 where a group has no volume: `held_work` finds its row by the
/// group's own files, so a pass leaves a one-shot whose file changed to the
/// full derive, as test 5 leaves Beta, and counts and commits nothing. A rule
/// 2 that read only a volume's address would rewrite it here, its bytes being
/// new.
#[test]
fn a_pass_leaves_a_volume_less_work_whose_file_changed() {
    let tree = crate::tests::tempdir("fill-oneshot-waits");
    write_archive(&tree.path().join(ONESHOT), 2);
    let store = crate::tests::tempdir("fill-oneshot-waits-store");
    let library = Library::open(&store.path().join("library.db")).expect("open");
    let mut conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("first derive");
    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work", [], |r| r.get(0))
        .expect("the one-shot's work");
    assert_eq!(
        volume, None,
        "the fixture: nothing in the name says which volume it is"
    );
    let old = digest(&conn, &root, ONESHOT);
    write_archive(&tree.path().join(ONESHOT), 4);
    scan::scan_root(&conn, &root).expect("rescan");
    hash_now(&conn, &root, ONESHOT);
    assert_ne!(
        digest(&conn, &root, ONESHOT),
        old,
        "the fixture: its file changed, and is hashed"
    );
    let was = rows(&conn);

    let (commits, outcome) = commits_during(&mut conn, pass);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "no volume is new, so none counts"
    );
    assert_eq!(commits, 0, "no write transaction committed");
    assert_eq!(rows(&conn), was, "the one-shot's work keeps its old member");
}

/// Rule 2 as design §7 words it, no work at a volume's address. Alpha's file
/// is renamed to volume 2 and a new file takes volume 1, so the group at
/// volume 2 claims the content of the work still at volume 1, and
/// `held_work` passes over that work for the new volume 1. A pass leaves
/// volume 1 to the full derive, which moves the work away first; written
/// early, it would stand beside that work, two at one address, until then.
#[test]
fn a_pass_leaves_a_volume_whose_address_a_work_still_holds_until_it_moves() {
    let tree = crate::tests::tempdir("fill-address-moving");
    write_archive(&tree.path().join(ALPHA), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("first derive");
    let was = rows(&conn);
    std::fs::rename(tree.path().join(ALPHA), tree.path().join(ALPHA_V02))
        .expect("renames Alpha's file");
    write_archive(&tree.path().join(ALPHA), 3);
    scan::scan_root(&conn, &root).expect("rescan");
    for file in [ALPHA, ALPHA_V02] {
        hash_now(&conn, &root, file);
    }
    // Where `held_work` took the work at volume 1 for the new volume 1, the
    // volume arm of rule 2 would agree with it, and this test could not tell
    // the two apart.
    let groups = groups(&conn);
    let placed = Placed::of(&groups).expect("places the groups");
    let at_v1 = groups
        .values()
        .find(|group| group.volume == Some(1.0))
        .expect("the fixture: a group at volume 1");
    let alpha = series_id_of(&conn, &at_v1.series)
        .expect("reads the series")
        .expect("the fixture: Alpha has a series row");
    assert_eq!(
        held_work(&conn, alpha, at_v1, &placed).expect("looks for a work"),
        None,
        "the fixture: held_work passes over the work at volume 1, whose content volume 2 claims"
    );

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "volume 1 waits for the work at it to move, and volume 2's bytes are on that work"
    );
    assert_eq!(rows(&conn), was, "one work, Alpha's, as it was");
}

/// Rule 2's address is a volume of one series: a pass writes Alpha's new
/// volume 2 while Beta holds a volume 2 of its own. Read across series, the
/// address check would hold back every new volume whose number another series
/// has, and a rescan's new volumes would wait for the full derive.
#[test]
fn a_pass_writes_a_new_volume_whose_number_another_series_holds() {
    let tree = crate::tests::tempdir("fill-address-series");
    for file in [ALPHA, BETA, BETA_V02] {
        write_archive(&tree.path().join(file), 2);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    derive::derive(&conn).expect("first derive");
    write_archive(&tree.path().join(ALPHA_V02), 2);
    scan::scan_root(&conn, &root).expect("rescan");
    hash_now(&conn, &root, ALPHA_V02);

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(1),
        "Alpha's volume 2 is new to Alpha"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work WHERE volume = 2.0"),
        2,
        "Alpha's volume 2 stands beside Beta's"
    );
}

/// A digest below every real one, so a pair names a fixture's file as its
/// `sha_b`.
const LOW: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// A digest above every real one, so a pair names a fixture's file as its
/// `sha_a`.
const HIGH: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// One directory holding `Alpha/Alpha v01.cbz` and `Beta/Beta v01.cbz`,
/// derived; a pair split at 0.5 naming `named`'s digest and `partner`; then a
/// new `Alpha/Alpha v02.cbz`, hashed.
fn beside_a_split_pair(
    label: &str,
    named: &str,
    partner: &str,
) -> (crate::tests::TempDir, Library) {
    let tree = crate::tests::tempdir(label);
    write_archive(&tree.path().join(ALPHA), 2);
    write_archive(&tree.path().join(BETA), 3);
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scanned_root(&conn, tree.path(), None);
        derive::derive(&conn).expect("derives");
        let file = digest(&conn, &root, named);
        let (sha_a, sha_b) = if file.as_str() < partner {
            (file.as_str(), partner)
        } else {
            (partner, file.as_str())
        };
        conn.execute(
            "INSERT INTO content_match
                 (sha_a, sha_b, v, long_paired, a_slots, b_slots, a_pages, b_pages, runs, state)
             VALUES (?1, ?2, 0.5, 1, 3, 3, 3, 3, '[]', 'split')",
            [sha_a, sha_b],
        )
        .expect("a split pair under the bar");
        write_archive(&tree.path().join(ALPHA_V02), 4);
        scan::scan_root(&conn, &root).expect("rescan");
        hash_now(&conn, &root, ALPHA_V02);
    }
    (tree, library)
}

/// Design §8 test 6, its first half, and call 4: a directory holding a file a
/// stored match names waits, whole, for the full derive, whatever the pair's
/// state and score and on either side of it. The control's pair names Beta's
/// directory alone, so the directory is what holds a volume back, not a pair
/// being stored.
#[test]
fn a_pass_leaves_a_volume_in_a_directory_a_stored_match_names() {
    for (side, partner) in [("sha_b", LOW), ("sha_a", HIGH)] {
        let (_tree, library) = beside_a_split_pair("fill-named", ALPHA, partner);
        let conn = library.conn();

        let outcome = pass(&conn);

        assert_eq!(
            outcome,
            Outcome::Done(0),
            "a split pair under the bar names Alpha's file as its {side}"
        );
        derive::derive(&conn).expect("the full derive");
        assert_eq!(
            scalar(&conn, "SELECT count(*) FROM work WHERE volume = 2.0"),
            1,
            "the full derive writes volume 2 (the pair's {side})"
        );
    }
    let (_tree, library) = beside_a_split_pair("fill-named-elsewhere", BETA, LOW);
    let conn = library.conn();
    assert_eq!(
        pass(&conn),
        Outcome::Done(1),
        "a pair naming Beta's directory holds back nothing in Alpha's"
    );
}

/// Design §8 test 6, its second half: a new raw volume in a directory whose
/// volumes joined the English series follows them there at the full derive,
/// so a pass leaves it rather than show it in the raws' own series and move
/// it at the end.
#[test]
fn a_pass_leaves_a_volume_whose_directory_follows_a_join() {
    let shelf = Shelf::new("fill-follows");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let raws: Vec<String> = (1..=3).map(oshinoko).collect();
    let shelved = shelved(&shelf, &conn, &[1, 2, 3].map(oshi), &raws);
    shelved.pair_volumes(&conn, &[1, 2, 3]);
    let joined = derive::derive(&conn).expect("derives the joins");
    assert_eq!(joined.joined, 3, "the fixture: each raw joined its English");
    write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    hash_now(&conn, &shelved.raw, &oshinoko(4));

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "raw 4's directory holds files stored pairs name"
    );
    derive::derive(&conn).expect("the full derive");
    assert_eq!(
        work(&conn, holder(&conn, &shelved.ja(&conn, 4))),
        raw_at("Oshi", 4),
        "the full derive puts raw 4 in the English series, where its directory follows"
    );
}

/// Rule 1 holds a group back by the own keys of the files still to hash, and
/// a raw still to hash files under its own series' key, not the English
/// volume it joins. So rule 3 is what keeps that volume from being written
/// without its raw: the pair names the English file too.
#[test]
fn a_pass_leaves_a_volume_a_file_still_to_hash_will_join() {
    let shelf = Shelf::new("fill-joins-later");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shelved = shelved(&shelf, &conn, &[oshi(1)], &[oshinoko(1)]);
    shelved.pair_volumes(&conn, &[1]);
    derive::derive(&conn).expect("derives the join");
    write_archive(&shelf.path("Volumes", &oshi(2)), 3);
    write_archive(&shelf.path("Raw", &oshinoko(2)), 3);
    scan::scan_root(&conn, &shelved.volumes).expect("rescans the English");
    scan::scan_root(&conn, &shelved.raw).expect("rescans the raws");
    hash_now(&conn, &shelved.volumes, &oshi(2));
    let en = shelved.en(&conn, 2);
    let ja = taiyaku_read::member_digest(&shelf.path("Raw", &oshinoko(2))).expect("hashes raw 2");
    read(&conn, &en, "whole");
    read(&conn, &ja, "whole");
    let id = pair(&conn, &en, &ja, 0.95);

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "English 2 waits for the raw that joins it"
    );
    hash_now(&conn, &shelved.raw, &oshinoko(2));
    derive::derive(&conn).expect("the full derive");
    let english = holder(&conn, &en);
    assert_eq!(
        (holder(&conn, &ja), editions(&conn, english)),
        (
            english,
            vec![labelled(VOLUMES, None), labelled(RAW, Some(id))]
        ),
        "the full derive writes English 2 with its raw joined"
    );
}

/// Rule 3 reads a held reading too. A joined raw whose time changed waits to
/// be hashed again with no digest, its last one held; hashed at the full
/// derive it joins again, and its directory follows, taking the new raw
/// beside it into the English series.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_file_waiting_to_be_hashed_again() {
    let shelf = Shelf::new("fill-held-reading");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shelved = shelved(&shelf, &conn, &[oshi(1)], &[oshinoko(1)]);
    shelved.pair_volumes(&conn, &[1]);
    derive::derive(&conn).expect("derives the join");
    write_archive(&shelf.path("Raw", &oshinoko(2)), 3);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    hash_now(&conn, &shelved.raw, &oshinoko(2));
    // As a walk leaves a held file whose time changed.
    conn.execute(
        "UPDATE scan_entry SET held_sha256 = sha256, sha256 = NULL
          WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![shelved.raw.id, oshinoko(1)],
    )
    .expect("raw 1 waits to be hashed again");

    let outcome = pass(&conn);

    assert_eq!(outcome, Outcome::Done(0), "raw 2 waits beside raw 1");
    hash_now(&conn, &shelved.raw, &oshinoko(1));
    derive::derive(&conn).expect("the full derive");
    assert_eq!(
        work(&conn, holder(&conn, &shelved.ja(&conn, 2))),
        raw_at("Oshi", 2),
        "raw 2 follows its directory into the English series"
    );
}

/// Rule 3 names the directory a joined file was renamed or moved into: at its
/// new path it has no digest until the queue reaches it, and the walk drops its
/// old path, so its old digest is a stray, held by a member and by no entry. A new raw of the stray's page
/// count beside it waits; written early, it would show in the raws' series
/// and move to the English one when the moved raw joins at the full derive
/// and its directory follows.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_file_moved_into_its_directory() {
    for (moved, new) in [
        ("Oshinoko/Oshinoko v5.cbz", oshinoko(4)),
        (
            "Moved/Oshinoko v05.cbz",
            "Moved/Oshinoko v04.cbz".to_owned(),
        ),
    ] {
        let shelf = Shelf::new("fill-moved");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let shelved = shelved(&shelf, &conn, &[oshi(5)], &[oshinoko(5)]);
        shelved.pair_volumes(&conn, &[5]);
        let joined = derive::derive(&conn).expect("derives the join");
        assert_eq!(joined.joined, 1, "the fixture: raw 5 joined English 5");
        let to = shelf.path("Raw", moved);
        std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
        std::fs::rename(shelf.path("Raw", &oshinoko(5)), to).expect("moves raw 5");
        write_archive(&shelf.path("Raw", &new), 3);
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        assert_eq!(
            probe.recording.written_so_far(),
            Vec::<u64>::new(),
            "raw 4 waits beside raw 5 at {moved}"
        );
        assert_eq!(
            work(&conn, holder(&conn, &digest(&conn, &shelved.raw, &new))),
            raw_at("Oshi", 4),
            "the full derive puts raw 4 in the English series beside raw 5 at {moved}"
        );
    }
}

/// Raw 5 joined to English 5, then deleted, walked and derived: its edition
/// stays on the English work, so its digest is a stray.
fn beside_a_stray(label: &str) -> (Shelf, Library, crate::derive::joins::tests::Shelved) {
    let shelf = Shelf::new(label);
    let library = Library::open_in_memory().expect("open");
    let shelved = {
        let conn = library.conn();
        let shelved = shelved(&shelf, &conn, &[oshi(5)], &[oshinoko(5)]);
        shelved.pair_volumes(&conn, &[5]);
        derive::derive(&conn).expect("derives the join");
        std::fs::remove_file(shelf.path("Raw", &oshinoko(5))).expect("deletes raw 5");
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        derive::derive(&conn).expect("derives without raw 5");
        shelved
    };
    (shelf, library, shelved)
}

/// Stamps the file at `at` with a later time, so the next walk reads it as
/// changed whatever its size and the clock's grain.
fn stamp_changed(at: &std::path::Path) {
    let later = std::time::SystemTime::now() + Duration::from_secs(10);
    std::fs::File::options()
        .write(true)
        .open(at)
        .expect("opens it")
        .set_modified(later)
        .expect("stamps it as changed");
}

/// Rule 3 matches a file still to hash to a stray by page count, whether or
/// not its row holds a reading. Raw 5, joined and deleted, leaves its digest
/// on the English work. Raw 6 at its count holds raw 4 back, new or changed
/// in place and waiting to be hashed again; at another count it holds nothing
/// and raw 4 is written early. Wider, a store with a stray would hold back
/// every directory holding a file still to hash.
/// Raw 4 ends in the raws' series either way: a deleted file leads no follow.
#[test]
fn a_pass_leaves_a_volume_beside_a_file_of_a_gone_matched_files_page_count() {
    for (pages, held, written) in [(3, false, vec![]), (4, false, vec![1]), (3, true, vec![])] {
        let (shelf, library, shelved) = beside_a_stray("fill-stray-pages");
        let conn = library.conn();
        let raw6 = shelf.path("Raw", &oshinoko(6));
        write_archive(&raw6, pages);
        if held {
            scan::scan_root(&conn, &shelved.raw).expect("walks raw 6");
            derive::derive(&conn).expect("derives raw 6");
            write_archive_labelled(&raw6, pages, "jpg", "changed in place");
            stamp_changed(&raw6);
        }
        write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
        scan::scan_root(&conn, &shelved.raw).expect("walks raw 4");
        if held {
            assert!(
                crate::scan::tests::held_reading(&conn, shelved.raw.id, &oshinoko(6))
                    .0
                    .is_some(),
                "the fixture: the walk holds raw 6's old reading"
            );
        }
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        assert_eq!(
            probe.recording.written_so_far(),
            written,
            "raw 6 at {pages} pages (held: {held})"
        );
        assert_eq!(
            work(&conn, holder(&conn, &shelved.ja(&conn, 4))),
            raw_at("Oshinoko", 4),
            "raw 4 ends in the raws' series (raw 6 at {pages} pages, held: {held})"
        );
    }
}

/// Rule 3 reads a directory's row too: a folder volume is fingerprinted and
/// matched as an archive is, and renamed it keeps its digest and page count
/// while its new row has no digest. Raw 4, beside a joined raw 5 folder
/// renamed in its directory, waits; written early, it would show in the raws'
/// series and move to the English one when the folder joins at the full
/// derive and its directory follows.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_folder_renamed_in_its_directory() {
    let shelf = Shelf::new("fill-folder-renamed");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shelved = shelved(&shelf, &conn, &[oshi(5)], &[]);
    let folder = "Oshinoko/Oshinoko v05";
    write_pages(&shelf.path("Raw", folder), 3);
    scan::scan_root(&conn, &shelved.raw).expect("walks raw 5's folder");
    derive::derive(&conn).expect("derives raw 5");
    let ja = digest(&conn, &shelved.raw, folder);
    read(&conn, &ja, "whole");
    pair(&conn, &shelved.en(&conn, 5), &ja, 0.95);
    let joined = derive::derive(&conn).expect("derives the join");
    assert_eq!(
        joined.joined, 1,
        "the fixture: raw 5's folder joined English 5"
    );
    std::fs::rename(
        shelf.path("Raw", folder),
        shelf.path("Raw", "Oshinoko/Oshinoko v5"),
    )
    .expect("renames raw 5's folder");
    write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    let probe = Probe::new(&conn);

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        Vec::<u64>::new(),
        "raw 4 waits beside raw 5's renamed folder"
    );
    assert_eq!(
        work(&conn, holder(&conn, &shelved.ja(&conn, 4))),
        raw_at("Oshi", 4),
        "the full derive puts raw 4 in the English series beside raw 5's folder"
    );
}

/// Rule 3's stray is a matched file that may not be where its member says.
/// With the joined pair where it was, each entry holding its member's bytes,
/// a file still to hash at the pair's page count names nothing, and Zz 1
/// beside it is written early; reading every matched member as a stray would
/// hold back every directory holding a file still to hash at that page count.
#[test]
fn a_pass_writes_a_volume_beside_a_file_of_a_present_matched_files_page_count() {
    let shelf = Shelf::new("fill-present");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shelved = shelved(&shelf, &conn, &[oshi(5)], &[oshinoko(5)]);
    shelved.pair_volumes(&conn, &[5]);
    derive::derive(&conn).expect("derives the join");
    write_archive(&shelf.path("Raw", "Zz/Zz v01.cbz"), 2);
    write_archive(&shelf.path("Raw", "Zz/Zz v02.cbz"), 3);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    let probe = Probe::new(&conn);

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1],
        "Zz 1 is written early beside Zz 2"
    );
}

/// Rule 3's stray is a member's digest that a match names. A member no match
/// names is no stray, so on a rescan with no match rows a file still to hash
/// at an English volume's page count names nothing, and Zz 1 is written early
/// beside Zz 2; read as a stray, it would hold back every directory holding a
/// file still to hash at its page count.
#[test]
fn a_pass_writes_a_volume_beside_a_file_of_an_unmatched_members_page_count() {
    let shelf = Shelf::new("fill-not-stray");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shelved = shelved(&shelf, &conn, &[oshi(5)], &[]);
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM content_match"),
        0,
        "the fixture: no match rows"
    );
    write_archive(&shelf.path("Raw", "Zz/Zz v01.cbz"), 2);
    write_archive(&shelf.path("Raw", "Zz/Zz v02.cbz"), 3);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    hash_now(&conn, &shelved.raw, "Zz/Zz v01.cbz");

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(1),
        "Zz 1 beside Zz 2 at 3 pages is written early where no match names raw 5"
    );
}

/// Outside a directory the job reads frozen, a joined raw waiting to be
/// hashed again is a stray until it is: the file on its row may be another,
/// its own moved away, so Zz 1 beside a Zz 2 at its page count waits.
#[test]
fn a_pass_leaves_a_volume_beside_a_file_of_a_waiting_matched_files_page_count() {
    let shelf = Shelf::new("fill-waiting-stray");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let shelved = shelved(&shelf, &conn, &[oshi(5)], &[oshinoko(5)]);
    shelved.pair_volumes(&conn, &[5]);
    let report = derive::derive(&conn).expect("derives the join");
    assert_eq!(report.joined, 1, "the fixture: raw 5 joined English 5");
    write_archive(&shelf.path("Raw", "Zz/Zz v01.cbz"), 2);
    write_archive(&shelf.path("Raw", "Zz/Zz v02.cbz"), 3);
    stamp_changed(&shelf.path("Raw", &oshinoko(5)));
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    hash_now(&conn, &shelved.raw, "Zz/Zz v01.cbz");
    assert!(
        crate::scan::tests::held_reading(&conn, shelved.raw.id, &oshinoko(5))
            .0
            .is_some(),
        "the fixture: the walk holds raw 5's reading"
    );

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "Zz 1 beside Zz 2 at 3 pages waits while joined raw 5 waits to be hashed again"
    );
}

/// The id and row of the one work holding the file at `path` in `shot`.
fn work_holding(shot: &Shot, path: &std::path::Path) -> (i64, WorkRow) {
    let path = path.to_string_lossy();
    let holding: Vec<(&i64, &WorkRow)> = shot
        .works
        .iter()
        .filter(|(_, row)| {
            row.editions
                .values()
                .flatten()
                .any(|(_, member, _)| *member == *path)
        })
        .collect();
    let [(id, row)] = holding[..] else {
        panic!("one work holds {path}: {holding:?}")
    };
    (*id, row.clone())
}

/// Rule 3's stray holds only for an entry the pass waits on, one the queue
/// has still to hash or found gone. A chapter with no volume is not queued
/// and no derive of the job stages it, so at raw 5's page count beside raw 4
/// it names nothing and raw 4 is written early, at the pass before raw 6,
/// which is at another count. Counted, every such chapter file would hold its
/// directory back to the full derive.
#[test]
fn a_pass_writes_a_volume_beside_a_chapter_file_of_a_gone_matched_files_page_count() {
    let (shelf, library, shelved) = beside_a_stray("fill-stray-chapter");
    let conn = library.conn();
    write_archive(&shelf.path("Raw", "Oshinoko/Oshinoko - c001.cbz"), 3);
    write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
    write_archive(&shelf.path("Raw", &oshinoko(6)), 4);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    let probe = Probe::new(&conn);

    let report = derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        report.unattached_chapter_dirs, 1,
        "the fixture: a chapter with no volume"
    );
    assert_eq!(
        probe.recording.written_so_far(),
        [1],
        "raw 4 is written early beside the chapter"
    );
    let raw4 = shelf.path("Raw", &oshinoko(4));
    assert_eq!(
        work_holding(&shot(&conn), &raw4),
        work_holding(probe.shots().last().expect("a pass wrote"), &raw4),
        "raw 4's work at the job's end is the one the pass wrote beside the chapter"
    );
}

/// A chapter's folder of loose images is no volume: the queue does not hash
/// it and no derive of the job stages it, so totalling raw 5's page count
/// beside raw 4 it names nothing and raw 4 is written early. A folder volume
/// is queued and holds until hashed
/// ([`a_pass_leaves_a_volume_beside_a_joined_folder_renamed_in_its_directory`]);
/// counted, any folder at a stray's count would hold its parent back.
#[test]
fn a_pass_writes_a_volume_beside_a_chapters_folder_of_a_gone_matched_files_page_count() {
    let (shelf, library, shelved) = beside_a_stray("fill-stray-chapter-folder");
    let conn = library.conn();
    write_pages(&shelf.path("Raw", "Oshinoko/Oshinoko - c001"), 3);
    write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
    write_archive(&shelf.path("Raw", &oshinoko(6)), 4);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    let probe = Probe::new(&conn);

    let report = derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        report.unattached_chapter_dirs, 1,
        "the fixture: a chapter's folder with no volume"
    );
    assert_eq!(
        probe.recording.written_so_far(),
        [1],
        "raw 4 is written early beside the chapter's folder"
    );
    let raw4 = shelf.path("Raw", &oshinoko(4));
    assert_eq!(
        work_holding(&shot(&conn), &raw4),
        work_holding(probe.shots().last().expect("a pass wrote"), &raw4),
        "raw 4's work at the job's end is the one the pass wrote beside the chapter's folder"
    );
}

/// A file the queue finds gone is still waited on, so at a stray's page count
/// it holds its directory at every later pass: raw 3, gone before its digest,
/// keeps raw 4 back at the pass that follows raw 4. Not waited on, it would
/// release raw 4 beside a file whose bytes no pass has read.
#[test]
fn a_pass_leaves_a_volume_beside_a_file_of_a_strays_page_count_the_queue_found_gone() {
    let (shelf, library, shelved) = beside_a_stray("fill-stray-gone");
    let conn = library.conn();
    for (n, pages) in [(3, 3), (4, 3), (6, 4)] {
        write_archive(&shelf.path("Raw", &oshinoko(n)), pages);
    }
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    let gone = shelf.path("Raw", &oshinoko(3));
    std::fs::remove_file(&gone).expect("raw 3 goes");
    let probe = Probe::new(&conn);

    let report = derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        report.unreadable.vanished,
        [gone.display().to_string()],
        "the fixture: the queue found raw 3 gone"
    );
    assert_eq!(
        probe.recording.written_so_far(),
        Vec::<u64>::new(),
        "raw 4 waits beside raw 3"
    );
}

/// An entry the pass does not wait on names nothing though the job hashes it.
/// Inner, at Manga's series folder, reads no series from `S 01.cbz`, whose
/// digest is a stray once Manga excludes `S/`: the queue passes it over, so
/// volume 5 beside it is written early, and the full derive's tail, which
/// hashes an entry a member names by path, gives it the stray digest after
/// the job's last report. Held, volume 5 would wait for an entry no derive
/// of the job stages.
#[test]
fn a_pass_writes_a_volume_beside_a_file_only_the_full_derives_tail_hashes() {
    let (top, library, manga) = crate::scan::tests::manga_holding(
        "fill-stray-tail",
        &[("S/S 01.cbz", 3), ("T/T v01.cbz", 3)],
    );
    let conn = library.conn();
    let stray = digest(&conn, &manga, "S/S 01.cbz");
    pair(&conn, &stray, &digest(&conn, &manga, "T/T v01.cbz"), 0.95);
    conn.execute(
        "UPDATE root SET exclude_globs = 'S/' WHERE id = ?1",
        [manga.id],
    )
    .expect("Manga excludes S/");
    let manga = scan::Root {
        exclude_globs: Some("S/".to_string()),
        ..manga
    };
    let inner = scan::add_root_checked_in(
        &conn,
        &manga.path.join("S"),
        scan::InLibrary::Named("Inner"),
    )
    .expect("adds Inner")
    .root;
    write_archive(&top.path().join("m/S/S - v05.cbz"), 2);
    write_archive(&top.path().join("m/S/S - v06.cbz"), 4);
    scan::scan_root(&conn, &manga).expect("Manga drops S/");
    scan::scan_root(&conn, &inner).expect("Inner walks S");
    assert_eq!(
        hashed_under(&conn, &inner),
        Vec::<String>::new(),
        "the fixture: Inner's entries hold no digest"
    );
    let probe = Probe::new(&conn);

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1],
        "volume 5 is written early beside S 01.cbz"
    );
    assert_eq!(
        (probe.digested(), digest(&conn, &inner, "S 01.cbz")),
        (
            vec![
                (inner.id, "S - v05.cbz".to_owned()),
                (inner.id, "S - v06.cbz".to_owned()),
            ],
            stray,
        ),
        "the queue hashes the two volumes alone, and the tail gives S 01.cbz the stray digest"
    );
    let volume5 = top.path().join("m/S/S - v05.cbz");
    assert_eq!(
        work_holding(&shot(&conn), &volume5),
        work_holding(probe.shots().last().expect("a pass wrote"), &volume5),
        "volume 5's work at the job's end is the one the pass wrote beside S 01.cbz"
    );
}

/// `Waiting` keys an entry by its root as well as its `rel_path`. A rule on
/// the raws alone reads a chapter's number as a volume, so the raws' file of
/// a path is queued and waited on while the English file of that path, a
/// chapter with no volume, is not. Both are at raw 5's page count, and
/// English 2 beside the English one is written early, at the pass before the
/// raws' file is hashed. Keyed by path alone, where the key is built or where
/// it is compared, a file no derive of the job stages would hold its
/// directory back whenever another root's file of its path waits.
#[test]
fn a_pass_writes_a_volume_beside_a_file_whose_path_waits_under_another_root() {
    let (shelf, library, shelved) = beside_a_stray("fill-stray-roots-job");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO rule (ordinal, root_id, kind, pattern, match_on)
         VALUES (100, ?1, 'regex', 'c(?<volume>\\d+)\\.cbz$', 'path')",
        [shelved.raw.id],
    )
    .expect("a rule on the raws alone");
    let chapter = "Same/Same - c009.cbz";
    let volume = "Same/Same v02.cbz";
    write_archive(&shelf.path("Raw", chapter), 3);
    write_archive(&shelf.path("Volumes", chapter), 3);
    write_archive(&shelf.path("Volumes", volume), 4);
    scan::scan_root(&conn, &shelved.raw).expect("walks the raws");
    scan::scan_root(&conn, &shelved.volumes).expect("walks the English");
    let probe = Probe::new(&conn);

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.digested(),
        [
            (shelved.volumes.id, volume.to_owned()),
            (shelved.raw.id, chapter.to_owned()),
        ],
        "the fixture: the queue hashes English 2, then the raws' file, and never the English \
         file of its path"
    );
    assert_eq!(
        probe.recording.written_so_far(),
        [1],
        "English 2 is written early beside a file the pass does not wait on, though the raws' \
         file of its path waits"
    );
}

/// `Waiting` keys an entry by its root as well as its `rel_path`. The raws'
/// root and the English one each hold a file still to hash at one `rel_path`
/// and raw 5's page count, and rule 3 names its directory under the root whose
/// entry the pass waits on: under both only when it waits on both. Keyed by
/// path alone, an entry no derive of the job stages would hold its directory
/// back whenever another root's file of that path waits. The waiting set is
/// made by hand, to read each root's case apart; a job reaches the first in
/// [`a_pass_writes_a_volume_beside_a_file_whose_path_waits_under_another_root`].
#[test]
fn an_entry_a_pass_waits_on_names_its_directory_under_its_own_root_alone() {
    let (shelf, library, shelved) = beside_a_stray("fill-stray-roots");
    let conn = library.conn();
    let same = "Same/Same v01.cbz";
    let roots = [("Raw", &shelved.raw), ("Volumes", &shelved.volumes)];
    for (name, root) in roots {
        write_archive(&shelf.path(name, same), 3);
        scan::scan_root(&conn, root).expect("walks the new file");
    }
    for waited in [vec!["Raw"], vec!["Volumes"], vec!["Raw", "Volumes"]] {
        let waiting: std::collections::HashSet<(i64, String)> = roots
            .iter()
            .filter(|(name, _)| waited.contains(name))
            .map(|(_, root)| (root.id, same.to_owned()))
            .collect();

        let named = super::named_directories(&conn, &waiting, &std::collections::HashSet::new())
            .expect("reads what rule 3 names");

        let under: Vec<&str> = roots
            .iter()
            .filter(|(_, root)| {
                named
                    .get(&root.id)
                    .is_some_and(|directories| directories.contains("Same"))
            })
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            under, waited,
            "the roots under which Same/ is named, the pass waiting on its file under {waited:?}"
        );
    }
}

/// O550: rule 3 reads a row that holds a reading too. Joined raw 5, renamed
/// onto raw 7's path, gets no row of its own: the walk drops its old one and
/// leaves raw 7's with no digest and raw 7's reading held, so raw 5's digest
/// is a stray and that row is where raw 5 waits. Raw 4 beside it waits;
/// written early, it would show in the raws' series and move to the English
/// one when raw 5 joins at the full derive. Raw 7 at raw 5's page count and
/// at another: the row's count is that of the file now on it.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_file_renamed_onto_another_files_path() {
    let mut each = Vec::new();
    for old_pages in [3, 4] {
        let shelf = Shelf::new("fill-renamed-onto");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let shelved = shelved(&shelf, &conn, &[oshi(5)], &[oshinoko(5)]);
        let onto = shelf.path("Raw", &oshinoko(7));
        write_archive(&onto, old_pages);
        scan::scan_root(&conn, &shelved.raw).expect("walks raw 7");
        derive::derive(&conn).expect("derives raw 7");
        shelved.pair_volumes(&conn, &[5]);
        let joined = derive::derive(&conn).expect("derives the join");
        assert_eq!(joined.joined, 1, "the fixture: raw 5 joined English 5");
        let old7 = shelved.ja(&conn, 7);
        std::fs::rename(shelf.path("Raw", &oshinoko(5)), &onto).expect("moves raw 5 onto raw 7");
        stamp_changed(&onto);
        write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        assert_eq!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &oshinoko(7)).0,
            Some(old7),
            "the fixture: the row raw 5 waits at holds raw 7's reading"
        );
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        each.push((
            old_pages,
            probe.recording.written_so_far(),
            work(&conn, holder(&conn, &shelved.ja(&conn, 4))),
        ));
    }
    assert_eq!(
        each,
        [3, 4].map(|old_pages| (old_pages, vec![], raw_at("Oshi", 4))),
        "beside raw 5 on the path of a raw 7 of 3 pages and of 4, raw 4 waits, and the full \
         derive puts it in the English series"
    );
}

/// A second store holding what `conn`'s holds, over the same directories, so
/// one derive there reads the files a job over `conn` reads.
fn copy_of(conn: &Connection) -> Library {
    let mut copy = Connection::open_in_memory().expect("in memory");
    rusqlite::backup::Backup::new(conn, &mut copy)
        .and_then(|backup| backup.run_to_completion(1024, Duration::ZERO, None))
        .expect("copies the store");
    Library::from_connection(copy, None).expect("opens the copy")
}

/// What rule 3 reading a row that holds a reading costs (O550). A file
/// rewritten in place waits as a renamed-onto one does, with no digest and
/// its last reading held, so at a stray's page count it holds its directory
/// though no match names it, in the stray's old directory and in another.
/// It holds only until it is hashed: the new volume beside it is written at
/// the pass that follows, a later file at another count holding nothing, and
/// the job ends as one derive over the same files does.
#[test]
fn a_file_changed_in_place_at_a_strays_page_count_holds_its_directory_until_it_is_hashed() {
    let layouts = [
        [oshinoko(4), oshinoko(6), oshinoko(8)],
        ["Zz/Zz v01.cbz", "Zz/Zz v02.cbz", "Zz/Zz v03.cbz"].map(str::to_owned),
    ];
    let mut each = Vec::new();
    for [new, changed, later] in &layouts {
        let (shelf, library, shelved) = beside_a_stray("fill-stray-changed");
        let conn = library.conn();
        let at = shelf.path("Raw", changed);
        write_archive(&at, 3);
        scan::scan_root(&conn, &shelved.raw).expect("walks the file to change");
        derive::derive(&conn).expect("derives it");
        write_archive_labelled(&at, 3, "jpg", "changed in place");
        stamp_changed(&at);
        write_archive(&shelf.path("Raw", new), 2);
        write_archive(&shelf.path("Raw", later), 4);
        scan::scan_root(&conn, &shelved.raw).expect("walks the change");
        assert!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, changed)
                .0
                .is_some(),
            "the fixture: the walk holds {changed}'s old reading"
        );
        let once = copy_of(&conn);
        let hashed_at_each_write = RefCell::new(Vec::new());
        let probe = Probe::new(&conn).on_written(|conn| {
            hashed_at_each_write
                .borrow_mut()
                .push(hashed_under(conn, &shelved.raw));
        });

        let filled = derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        let derived = derive::derive(&once.conn()).expect("one derive");
        assert_eq!(
            settled(&conn),
            settled(&once.conn()),
            "the job leaves what one derive does beside {changed}"
        );
        assert_eq!(filled, derived, "and reports what one derive does");
        each.push((
            probe.recording.written_so_far(),
            hashed_at_each_write.take(),
        ));
    }
    assert_eq!(
        each,
        layouts
            .each_ref()
            .map(|[new, changed, _]| (vec![1], vec![vec![new.clone(), changed.clone()]])),
        "in each directory one pass writes one work, and it is the pass after the changed \
         file's digest, not the one before it"
    );
}

/// What the passes of one job over `conn`, run to its end, wrote: the count
/// at each `written` call.
fn written_in_a_job(conn: &Connection) -> Vec<u64> {
    let probe = Probe::new(conn);
    derive::done(fill(conn, &probe, Duration::ZERO)).expect("the job ends");
    probe.recording.written_so_far()
}

/// Volumes 1 and 2 of a new raw `series`, at 2 pages and at raw 5's 3. The
/// queue hashes volume 1 first and a pass follows its digest, which writes
/// volume 1 early unless rule 3 names the series' directory for volume 2.
fn two_new_raws(shelf: &Shelf, series: &str) {
    write_archive(&shelf.path("Raw", &format!("{series}/{series} v01.cbz")), 2);
    write_archive(&shelf.path("Raw", &format!("{series}/{series} v02.cbz")), 3);
}

/// A third directory, `Raw2`, keyed as the raws and holding a byte copy of
/// the raws' `volume`, an archive or a folder, at its `rel_path`: scanned and
/// derived, so the raws' file is the primary and Raw2's its copy.
fn a_copy_in_raw2(shelf: &Shelf, conn: &Connection, raw: &scan::Root, volume: &str) {
    let (from, to) = (shelf.path("Raw", volume), shelf.path("Raw2", volume));
    std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
    if from.is_dir() {
        std::fs::create_dir(&to).expect("makes the folder");
        for page in std::fs::read_dir(&from).expect("lists the folder") {
            let page = page.expect("a page");
            std::fs::copy(page.path(), to.join(page.file_name())).expect("copies a page");
        }
    } else {
        std::fs::copy(&from, &to).expect("copies the file");
    }
    let raw2 = a_raw2(shelf, conn);
    scan::scan_root(conn, &raw2).expect("scans Raw2");
    derive::derive(conn).expect("derives the copy");
    assert_eq!(
        digest(conn, &raw2, volume),
        digest(conn, raw, volume),
        "the fixture: Raw2 holds {volume}'s bytes"
    );
}

/// `Raw2`, with whatever it holds, added to the raws' library and keyed as
/// the raws; not walked.
fn a_raw2(shelf: &Shelf, conn: &Connection) -> scan::Root {
    let at = shelf.path("", "Raw2");
    std::fs::create_dir_all(&at).expect("makes Raw2");
    let added = scan::add_root_in(conn, &at, scan::InLibrary::Named("Manga")).expect("adds Raw2");
    conn.execute(
        "UPDATE root SET edition_key = ?2, default_kind = ?3, default_language = ?4
          WHERE id = ?1",
        rusqlite::params![added.id, RAW.0, RAW.1, RAW.2],
    )
    .expect("keys it");
    scan::roots(conn)
        .expect("lists the directories")
        .into_iter()
        .find(|root| root.id == added.id)
        .expect("Raw2")
}

/// The path of each member holding `sha256`, in path order.
fn members_holding(conn: &Connection, sha256: &str) -> Vec<String> {
    conn.prepare("SELECT path FROM member WHERE sha256 = ?1 ORDER BY path")
        .and_then(|mut stmt| stmt.query_map([sha256], |r| r.get(0))?.collect())
        .expect("reads the members")
}

/// English 5 and raw 5, an archive or a folder volume, joined; answers raw
/// 5's `rel_path` too. Where `copied`, a byte copy of raw 5 stands in `Raw2`
/// from before the join. One member holds raw 5's bytes, at the raws' path.
fn a_joined_raw_5(
    label: &str,
    folder: bool,
    copied: bool,
) -> (Shelf, Library, crate::derive::joins::tests::Shelved, String) {
    let home = if folder {
        "Oshinoko/Oshinoko v05".to_owned()
    } else {
        oshinoko(5)
    };
    let (shelf, library, shelved) = a_joined_raw_5_at(label, &home, folder, copied);
    (shelf, library, shelved, home)
}

/// [`a_joined_raw_5`] with raw 5 at `home` under the raws, whose name gives
/// its work's series.
fn a_joined_raw_5_at(
    label: &str,
    home: &str,
    folder: bool,
    copied: bool,
) -> (Shelf, Library, crate::derive::joins::tests::Shelved) {
    let shelf = Shelf::new(label);
    let library = Library::open_in_memory().expect("open");
    let shelved = {
        let conn = library.conn();
        let shelved = shelved(&shelf, &conn, &[oshi(5)], &[]);
        let at = shelf.path("Raw", home);
        if folder {
            write_pages(&at, 3);
        } else {
            write_archive(&at, 3);
        }
        scan::scan_root(&conn, &shelved.raw).expect("walks raw 5");
        derive::derive(&conn).expect("derives raw 5");
        let ja = digest(&conn, &shelved.raw, home);
        read(&conn, &ja, "whole");
        if copied {
            a_copy_in_raw2(&shelf, &conn, &shelved.raw, home);
        }
        pair(&conn, &shelved.en(&conn, 5), &ja, 0.95);
        let joined = derive::derive(&conn).expect("derives the join");
        assert_eq!(joined.joined, 1, "the fixture: raw 5 joined English 5");
        assert_eq!(
            members_holding(&conn, &ja),
            [at.to_string_lossy()],
            "the fixture: one member holds raw 5's bytes, at the raws' path"
        );
        shelved
    };
    (shelf, library, shelved)
}

/// O551: rule 3 asks whether a matched file is still at its member's path,
/// since a byte copy keeps its digest on an entry wherever the file goes.
/// Joined raw 5, an archive or a folder volume, has a copy in Raw2 at its own
/// `rel_path` and is moved into a new folder. Raw 4 beside it waits; written
/// early, it would show in the raws' series and move to the English one when
/// raw 5 joins at the full derive. An entry of that path under another
/// directory answers for nothing: Raw2's holds the bytes.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_volume_moved_while_its_copy_stands() {
    let mut each = Vec::new();
    for folder in [false, true] {
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-moved-copied", folder, true);
        let conn = library.conn();
        let to = shelf.path("Raw", &home.replace("Oshinoko/", "Moved/"));
        std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
        std::fs::rename(shelf.path("Raw", &home), to).expect("moves raw 5");
        let new = "Moved/Oshinoko v04.cbz";
        write_archive(&shelf.path("Raw", new), 3);
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        each.push((
            folder,
            probe.recording.written_so_far(),
            work(&conn, holder(&conn, &digest(&conn, &shelved.raw, new))),
        ));
    }
    assert_eq!(
        each,
        [false, true].map(|folder| (folder, vec![], raw_at("Oshi", 4))),
        "beside raw 5 moved while its copy stands, an archive and then a folder, raw 4 waits, \
         and the full derive puts it in the English series"
    );
}

/// Rule 3 reads a matched member at home whatever the volume is: a folder
/// volume's entry holds its digest at the member's path as a file's does.
/// Joined raw 5, a folder volume where its member says, is no stray, and Zz 1
/// is written early beside a Zz 2 at its page count. Never read as at its
/// path, every joined folder volume would hold back, in every job, each
/// directory holding a file still to hash at its page count.
#[test]
fn a_pass_writes_a_volume_beside_a_file_of_a_present_joined_folder_volumes_page_count() {
    let (shelf, library, shelved, _home) = a_joined_raw_5("fill-folder-at-home", true, false);
    let conn = library.conn();
    two_new_raws(&shelf, "Zz");
    scan::scan_root(&conn, &shelved.raw).expect("walks Zz");

    assert_eq!(
        written_in_a_job(&conn),
        [1],
        "Zz 1 is written early beside a Zz 2 at the page count of raw 5's folder at its path"
    );
}

/// What asking at the member's path costs (O551). Joined raw 5, deleted while
/// its copy stands in Raw2, is a stray though no file moved, and holds Zz 1
/// back beside a Zz 2 still to hash at its page count. For one job: the full
/// derive gives the member the copy's path, where an entry holds its bytes,
/// and the next job writes Yy 1 early beside such a Yy 2.
#[test]
fn a_joined_file_deleted_while_its_copy_stands_holds_its_page_count_back_for_one_job() {
    let (shelf, library, shelved, home) = a_joined_raw_5("fill-deleted-copied", false, true);
    let conn = library.conn();
    std::fs::remove_file(shelf.path("Raw", &home)).expect("deletes raw 5");
    two_new_raws(&shelf, "Zz");
    scan::scan_root(&conn, &shelved.raw).expect("walks the deletion");

    let deleting = written_in_a_job(&conn);

    two_new_raws(&shelf, "Yy");
    scan::scan_root(&conn, &shelved.raw).expect("walks Yy");
    assert_eq!(
        (deleting, written_in_a_job(&conn)),
        (vec![], vec![1]),
        "Zz 1 waits in the job that finds raw 5 gone, and the next job writes Yy 1 early"
    );
}

/// O550 and O551 together: joined raw 5, a byte copy of it standing in Raw2,
/// is renamed onto raw 7's path. The copy's entry keeps its digest present,
/// so only its member's path says it moved; and the row it waits at holds raw
/// 7's reading, so only a row holding one is at its page count. Raw 4 beside
/// it waits, and the full derive puts it in the English series. Either
/// reading without the other lets a pass write it in the raws'.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_file_renamed_onto_a_path_while_its_copy_stands() {
    let (shelf, library, shelved, home) = a_joined_raw_5("fill-onto-copied", false, true);
    let conn = library.conn();
    let onto = shelf.path("Raw", &oshinoko(7));
    write_archive(&onto, 3);
    scan::scan_root(&conn, &shelved.raw).expect("walks raw 7");
    derive::derive(&conn).expect("derives raw 7");
    let old7 = shelved.ja(&conn, 7);
    std::fs::rename(shelf.path("Raw", &home), &onto).expect("moves raw 5 onto raw 7");
    stamp_changed(&onto);
    write_archive(&shelf.path("Raw", &oshinoko(4)), 3);
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    assert_eq!(
        crate::scan::tests::held_reading(&conn, shelved.raw.id, &oshinoko(7)).0,
        Some(old7),
        "the fixture: the row raw 5 waits at holds raw 7's reading"
    );
    let probe = Probe::new(&conn);

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        (
            probe.recording.written_so_far(),
            work(&conn, holder(&conn, &shelved.ja(&conn, 4)))
        ),
        (vec![], raw_at("Oshi", 4)),
        "raw 4 waits beside raw 5 on raw 7's path, and the full derive puts it in the English \
         series"
    );
}

/// Rule 3 asks only a member under a directory whether its file is at its
/// path; one with no root has no path to ask. Raw 5's member, its root
/// cleared by hand, is the only member of bytes an entry holds, as its own
/// digest or as the reading of a file waiting to be hashed again: asked, with
/// no other member to answer, it would read as moved for as long as it stands.
/// It is no stray, and Aa 1 is written early beside an Aa 2 at its page count;
/// where raw 5 waits the queue hashes it last, so a pass follows Aa 2 as well.
#[test]
fn a_pass_writes_a_volume_beside_a_file_of_a_rootless_matched_members_page_count() {
    let mut each = Vec::new();
    for waiting in [false, true] {
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-rootless", false, false);
        let conn = library.conn();
        let rootless = conn
            .execute(
                "UPDATE member SET root_id = NULL WHERE sha256 = ?1",
                [digest(&conn, &shelved.raw, &home)],
            )
            .expect("clears the member's root");
        assert_eq!(rootless, 1, "the fixture: raw 5's member has no root");
        if waiting {
            stamp_changed(&shelf.path("Raw", &home));
        }
        two_new_raws(&shelf, "Aa");
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        assert_eq!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home)
                .0
                .is_some(),
            waiting,
            "the fixture: raw 5's row holds a reading only where it waits to be hashed again"
        );

        each.push((waiting, written_in_a_job(&conn)));
    }
    assert_eq!(
        each,
        [(false, vec![1]), (true, vec![1, 2])],
        "Aa 1 is written at the pass after its digest, raw 5's digest on its row or held there"
    );
}

/// Rule 3 asks whether any member of a matched file's bytes in its directory
/// is at its path, not each one. Two files of those bytes under one directory
/// are two members, and when the joined one moves its member keeps the old path
/// while the other stands: asked alone it would be a stray in every later job,
/// and for nothing, as raw 4 beside the moved raw 5 ends the job where a pass
/// writes it. With the duplicate at its path nothing is held, in that job or
/// the next.
#[test]
fn a_pass_writes_a_volume_beside_a_joined_file_moved_while_its_duplicate_stands() {
    let (shelf, library, shelved, home) = a_joined_raw_5("fill-moved-duplicate", false, false);
    let conn = library.conn();
    let (old, spare) = (
        shelf.path("Raw", &home),
        shelf.path("Raw", "Spare/Spare v05.cbz"),
    );
    std::fs::create_dir_all(spare.parent().expect("a directory")).expect("makes it");
    std::fs::copy(&old, &spare).expect("copies raw 5");
    scan::scan_root(&conn, &shelved.raw).expect("walks the duplicate");
    derive::derive(&conn).expect("derives the duplicate");
    let ja = digest(&conn, &shelved.raw, &home);
    let both = [&old, &spare].map(|at| at.to_string_lossy().into_owned());
    assert_eq!(
        members_holding(&conn, &ja),
        both,
        "the fixture: the duplicate is a member of its own"
    );
    let raw4 = shelf.path("Raw", "Moved/Oshinoko v04.cbz");
    write_archive(&raw4, 3);
    std::fs::rename(&old, shelf.path("Raw", "Moved/Oshinoko v05.cbz")).expect("moves raw 5");
    scan::scan_root(&conn, &shelved.raw).expect("rescan");
    let probe = Probe::new(&conn);

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1],
        "raw 4 is written early beside the moved raw 5"
    );
    assert_eq!(
        work_holding(&shot(&conn), &raw4),
        work_holding(probe.shots().last().expect("a pass wrote"), &raw4),
        "raw 4's work at the job's end is the one the pass wrote"
    );
    assert_eq!(
        members_holding(&conn, &ja),
        both,
        "the fixture: the moved raw 5's member keeps its old path"
    );
    two_new_raws(&shelf, "Zz");
    scan::scan_root(&conn, &shelved.raw).expect("walks Zz");
    assert_eq!(
        written_in_a_job(&conn),
        [1],
        "the next job writes Zz 1 early beside a Zz 2 at raw 5's page count"
    );
}

/// Rule 3 asks for the member's bytes at its path, not for a row there.
/// Joined raw 5 is moved away under a name that reads as volume 6 and its old
/// path keeps a row: another file is written there, or a folder volume's
/// pages are moved out and the emptied folder left. That row holds raw 5's
/// digest as its reading, so the digest is present and an entry sits at the
/// member's path. Raw 4 beside the moved raw 5 waits; written early, it would
/// show in the raws' series and move to the English one at the full derive.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_volume_moved_from_a_path_that_keeps_its_row() {
    let mut each = Vec::new();
    for folder in [false, true] {
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-old-path-kept", folder, false);
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, &home);
        let old = shelf.path("Raw", &home);
        let to = shelf.path(
            "Raw",
            &home.replace("Oshinoko/Oshinoko v05", "Moved/Oshinoko v06"),
        );
        std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
        if folder {
            std::fs::create_dir(&to).expect("makes the new folder");
            for page in std::fs::read_dir(&old).expect("lists raw 5's folder") {
                let page = page.expect("a page");
                std::fs::rename(page.path(), to.join(page.file_name())).expect("moves a page");
            }
        } else {
            std::fs::rename(&old, &to).expect("moves raw 5 away");
            write_archive_labelled(&old, 3, "jpg", "another file at raw 5's old path");
            stamp_changed(&old);
        }
        let new = "Moved/Oshinoko v04.cbz";
        write_archive(&shelf.path("Raw", new), 3);
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        assert_eq!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home).0,
            Some(ja),
            "the fixture: the row at raw 5's old path holds raw 5's reading"
        );
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        each.push((
            folder,
            probe.recording.written_so_far(),
            work(&conn, holder(&conn, &digest(&conn, &shelved.raw, new))),
        ));
    }
    assert_eq!(
        each,
        [false, true].map(|folder| (folder, vec![], raw_at("Oshi", 4))),
        "beside raw 5 moved from a path another file took, and from a folder left empty, raw 4 \
         waits, and the full derive puts it in the English series"
    );
}

/// What asking for the member's bytes at its path costs. Outside a directory
/// the job reads frozen, a joined raw 5 changed where it sits waits with no
/// digest, so it is a stray until an entry at its path holds its member's
/// bytes again. Touched, the bytes the same, that is when the queue hashes
/// it: Aa 1, queued before it, waits a pass for Aa 2, and Zz 1, queued after,
/// is written early beside Zz 2. Replaced by other bytes, it is at the job's
/// end, where the full derive rewrites the member: Zz 1 waits too, and the
/// next job writes Yy 1 early.
#[test]
fn a_joined_file_changed_where_it_sits_holds_its_page_count_back_until_its_bytes_are_there() {
    let mut each = Vec::new();
    for replaced in [false, true] {
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-changed-at-home", false, false);
        let conn = library.conn();
        let at = shelf.path("Raw", &home);
        if replaced {
            write_archive_labelled(&at, 3, "jpg", "other bytes at raw 5's path");
        }
        stamp_changed(&at);
        two_new_raws(&shelf, "Aa");
        two_new_raws(&shelf, "Zz");
        scan::scan_root(&conn, &shelved.raw).expect("walks the change");
        assert!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home)
                .0
                .is_some(),
            "the fixture: the walk holds raw 5's reading"
        );

        let changing = written_in_a_job(&conn);

        two_new_raws(&shelf, "Yy");
        scan::scan_root(&conn, &shelved.raw).expect("walks Yy");
        each.push((replaced, changing, written_in_a_job(&conn)));
    }
    assert_eq!(
        each,
        [(false, vec![2, 3], vec![1]), (true, vec![2], vec![1]),],
        "touched, then replaced: Aa 1 is written with Aa 2 and not before it, Zz 1 early only \
         beside the touched raw 5, and Yy 1 early in the next job"
    );
}

/// A directory the job reads frozen is read as its works hold it, and no
/// derive hashes its files until a Scan is pressed for it, so for every job of
/// the pause a matched file changed there has not moved. Joined raw 5 is
/// touched and walked, and its directory paused, as a cancelled scan leaves
/// it. Asked for its bytes as its row's digest alone it would be a stray while
/// the pause stands, and Zz 1 and then Yy 1 would each wait beside a volume 2
/// at its page count in a job over another directory.
#[test]
fn a_matched_file_changed_in_a_directory_the_job_reads_frozen_holds_nothing_back() {
    let (shelf, library, shelved, home) = a_joined_raw_5("fill-frozen-changed", false, false);
    let conn = library.conn();
    let ja = digest(&conn, &shelved.raw, &home);
    stamp_changed(&shelf.path("Raw", &home));
    scan::scan_root(&conn, &shelved.raw).expect("walks the change");
    pause(&conn, &shelved.raw);

    let each = ["Zz", "Yy"].map(|series| {
        write_archive(
            &shelf.path("Volumes", &format!("{series}/{series} v01.cbz")),
            2,
        );
        write_archive(
            &shelf.path("Volumes", &format!("{series}/{series} v02.cbz")),
            3,
        );
        scan::scan_root(&conn, &shelved.volumes).expect("walks the new series");
        written_in_a_job(&conn)
    });

    assert_eq!(
        each,
        [vec![1], vec![1]],
        "Zz 1 and then Yy 1 are written early, each beside a volume 2 at raw 5's page count"
    );
    assert_eq!(
        (
            hashed_under(&conn, &shelved.raw),
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home).0
        ),
        (vec![], Some(ja)),
        "the fixture: neither job hashed raw 5, whose row holds its reading still"
    );
}

/// The exemption asks whether the job reads the row's own directory frozen,
/// not what the store holds paused nor whether any directory is frozen. The
/// third layout sits in the raws, paused while the job hashes them (named
/// fresh by a pressed Scan whose unpause the store refused, or paused once the
/// job has begun, as the cancel of a scan waiting behind it pauses), or beside
/// another directory the job reads frozen. The job hashes the moved raw 5, so
/// raw 4 beside it waits as with no pause; were the row at raw 5's old path to
/// answer for it, a pass would write raw 4 in the raws' series.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_volume_moved_where_the_job_does_not_read_frozen() {
    let cases = ["named fresh", "paused once begun", "another paused"];
    let mut each = Vec::new();
    for case in cases {
        let (shelf, library, shelved, home) =
            a_joined_raw_5("fill-paused-not-frozen", false, false);
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, &home);
        let old = shelf.path("Raw", &home);
        let (moved, new) = ("Moved/Oshinoko v06.cbz", "Moved/Oshinoko v04.cbz");
        std::fs::create_dir_all(shelf.path("Raw", "Moved")).expect("makes it");
        std::fs::rename(&old, shelf.path("Raw", moved)).expect("moves raw 5 away");
        write_archive_labelled(&old, 3, "jpg", "another file at raw 5's old path");
        stamp_changed(&old);
        write_archive(&shelf.path("Raw", new), 3);
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        assert_eq!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home).0,
            Some(ja),
            "the fixture: the row at raw 5's old path holds raw 5's reading"
        );
        let raw = shelved.raw.clone();
        let (paused, fresh, probe) = match case {
            "named fresh" => {
                pause(&conn, &raw);
                (raw.id, vec![raw.id], Probe::new(&conn))
            }
            "paused once begun" => {
                let id = raw.id;
                let pausing = Probe::new(&conn).cancelling(move |conn, _| {
                    pause(conn, &raw);
                    false
                });
                (id, vec![], pausing)
            }
            _ => {
                let other = a_raw2(&shelf, &conn);
                pause(&conn, &other);
                (other.id, vec![], Probe::new(&conn))
            }
        };
        let now = || probe.now();
        let cadence = Cadence {
            floor: Duration::ZERO,
            factor: 0,
            now: &now,
        };

        let job = derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &fresh,
            &probe,
            &cadence,
        );

        derive::done(job.expect("derives")).expect("the job ends");
        each.push((
            case,
            scan::paused(&conn)
                .expect("reads the pauses")
                .contains(&paused),
            probe
                .digested()
                .contains(&(shelved.raw.id, moved.to_owned())),
            probe.recording.written_so_far(),
            work(&conn, holder(&conn, &digest(&conn, &shelved.raw, new))),
        ));
    }
    assert_eq!(
        each,
        cases.map(|case| (case, true, true, vec![], raw_at("Oshi", 4))),
        "the raws named fresh, paused once begun, then read fresh beside another directory \
         paused: the directory paused is paused at the job's end and the job hashed the moved \
         raw 5 (the fixture), raw 4 waits, and the full derive puts it in the English series"
    );
}

/// Why a reading held at a member's path in a frozen directory is the member
/// at home. Joined raw 5 is moved into Raw2 as volume 6, its old path keeping
/// a row (another file written there, or a folder's pages moved out), and the
/// raws' directory is paused once walked. The job reads that row as raw 5
/// still, so the member keeps its path and nothing follows the moved file: raw
/// 4 beside it, which a pass writes, ends the job on the work the pass wrote.
#[test]
fn a_volume_a_pass_writes_beside_a_joined_volume_moved_out_of_a_frozen_directory_ends_as_written() {
    let mut each = Vec::new();
    for folder in [false, true] {
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-frozen-moved", folder, false);
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, &home);
        let raw2 = a_raw2(&shelf, &conn);
        let old = shelf.path("Raw", &home);
        let to = shelf.path(
            "Raw2",
            &home.replace("Oshinoko/Oshinoko v05", "Moved/Oshinoko v06"),
        );
        std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
        if folder {
            std::fs::create_dir(&to).expect("makes the new folder");
            for page in std::fs::read_dir(&old).expect("lists raw 5's folder") {
                let page = page.expect("a page");
                std::fs::rename(page.path(), to.join(page.file_name())).expect("moves a page");
            }
        } else {
            std::fs::rename(&old, &to).expect("moves raw 5 away");
            write_archive_labelled(&old, 3, "jpg", "another file at raw 5's old path");
            stamp_changed(&old);
        }
        scan::scan_root(&conn, &shelved.raw).expect("walks the old path");
        assert_eq!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home).0,
            Some(ja.clone()),
            "the fixture: the row at raw 5's old path holds raw 5's reading"
        );
        pause(&conn, &shelved.raw);
        let raw4 = shelf.path("Raw2", "Moved/Oshinoko v04.cbz");
        write_archive(&raw4, 3);
        scan::scan_root(&conn, &raw2).expect("walks Raw2");
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        let written = probe.recording.written_so_far();
        let ended = work_holding(&shot(&conn), &raw4);
        each.push((
            folder,
            written,
            probe.shots().last().map(|shot| work_holding(shot, &raw4)) == Some(ended.clone()),
            ended.1.series,
            members_holding(&conn, &ja) == [old.to_string_lossy()],
        ));
    }
    assert_eq!(
        each,
        [false, true].map(|folder| (folder, vec![1], true, Some("Oshinoko".to_owned()), true)),
        "from a path another file took, then from a folder left empty: a pass writes raw 4, its \
         work at the job's end is the one the pass wrote, in the raws' series, and raw 5's \
         member keeps its path"
    );
}

/// A frozen row is the member at home only where its reading is the member's
/// bytes: another file's says nothing of where the member's file went, even
/// where a match names it, as one names the other file's here. Joined raw 5
/// moves off a path that file takes; a job is cancelled once it is hashed, or
/// once the full derive has written its work, which raw 5's name puts first,
/// dropping the row's reading while raw 5's member keeps the path. Changed
/// again, walked and paused, the row holds raw 5's reading and Zz 1 is written
/// early beside a Zz 2 at its page count, or the other file's and Zz 1 waits.
#[test]
fn a_frozen_row_holding_another_files_reading_is_not_the_member_at_home() {
    let (home, moved) = ("Aaa/Aaa v05.cbz", "Moved/Aaa v06.cbz");
    let mut each = Vec::new();
    for others in [false, true] {
        let (shelf, library, shelved) =
            a_joined_raw_5_at("fill-frozen-reading", home, false, false);
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, home);
        let old = shelf.path("Raw", home);
        std::fs::create_dir_all(shelf.path("Raw", "Moved")).expect("makes it");
        std::fs::rename(&old, shelf.path("Raw", moved)).expect("moves raw 5 away");
        write_archive_labelled(&old, 3, "jpg", "another file at raw 5's old path");
        stamp_changed(&old);
        scan::scan_root(&conn, &shelved.raw).expect("walks the move");
        let raw = shelved.raw.id;
        let probe = Probe::new(&conn).cancelling(move |conn, _| {
            let (hashed, written): (bool, bool) = conn
                .query_row(
                    "SELECT (SELECT count(*) FROM scan_entry
                              WHERE root_id = ?1 AND rel_path IN (?2, ?3)
                                AND sha256 IS NOT NULL) = 2,
                            EXISTS (SELECT 1 FROM member m
                                      JOIN scan_entry e ON e.sha256 = m.sha256
                                     WHERE e.root_id = ?1 AND e.rel_path = ?2)",
                    rusqlite::params![raw, home, moved],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .expect("reads the store");
            if others { written } else { hashed }
        });

        let stopped = fill(&conn, &probe, Duration::ZERO);

        let other = digest(&conn, &shelved.raw, home);
        pair(&conn, &shelved.en(&conn, 5), &other, 0.9);
        let dropped = crate::scan::tests::held_reading(&conn, raw, home)
            .0
            .is_none();
        write_archive_labelled(&old, 4, "jpg", "the other file changed again");
        stamp_changed(&old);
        scan::scan_root(&conn, &shelved.raw).expect("walks the change");
        assert_eq!(
            (
                matches!(stopped, Outcome::Cancelled(_)),
                dropped,
                crate::scan::tests::held_reading(&conn, raw, home).0,
                members_holding(&conn, &ja) == [old.to_string_lossy()],
            ),
            (true, others, Some(if others { other } else { ja }), true),
            "the fixture: the job is cancelled, the row's reading dropped only once the other \
             file's work is written, the walk holds raw 5's reading or the other file's, and \
             raw 5's member keeps its path"
        );
        pause(&conn, &shelved.raw);
        write_archive(&shelf.path("Volumes", "Zz/Zz v01.cbz"), 2);
        write_archive(&shelf.path("Volumes", "Zz/Zz v02.cbz"), 3);
        scan::scan_root(&conn, &shelved.volumes).expect("walks Zz");

        each.push((others, written_in_a_job(&conn)));
    }
    assert_eq!(
        each,
        [(false, vec![1]), (true, vec![])],
        "Zz 1 is written early where raw 5's frozen row holds raw 5's reading, and waits where \
         it holds another file's"
    );
}

/// Joined raw 5 with a second file of its bytes in Raw2, a member of its own,
/// and a byte copy at raw 5's own path there, no member; with `reached`, that
/// copy is on raw 5's `member_root` from before the join. Raw2 is then paused
/// with the second file's held reading (`paused`), or loses the copy. Raw 5 is
/// moved into a new folder beside a new raw 4 at its page count, and walked.
/// Answers raw 4's path, raw 5's digest and the paths its members should end at.
fn raw_5_moved_beside_a_member_of_its_bytes_in_raw2(
    label: &str,
    reached: bool,
    paused: bool,
) -> (Shelf, Library, std::path::PathBuf, String, [String; 2]) {
    let (shelf, library, shelved, home) = a_joined_raw_5(label, false, reached);
    let (raw4, ja, ends) = {
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, &home);
        let raw2 = if reached {
            scan::roots(&conn)
                .expect("lists the directories")
                .into_iter()
                .find(|root| root.path == shelf.path("", "Raw2"))
                .expect("Raw2")
        } else {
            a_raw2(&shelf, &conn)
        };
        let (old, spare) = (shelf.path("Raw", &home), "Spare/Spare v05.cbz");
        let copies: &[&str] = if reached { &[spare] } else { &[&home, spare] };
        for at in copies {
            let to = shelf.path("Raw2", at);
            std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
            std::fs::copy(&old, &to).expect("copies raw 5");
        }
        scan::scan_root(&conn, &raw2).expect("walks Raw2");
        derive::derive(&conn).expect("derives Raw2");
        let reach: i64 = conn
            .query_row(
                "SELECT count(*) FROM member_root mr JOIN member m ON m.id = mr.member_id
                  WHERE m.path = ?1",
                [old.to_string_lossy()],
                |r| r.get(0),
            )
            .expect("reads raw 5's reach");
        assert_eq!(
            (members_holding(&conn, &ja), reach),
            (
                vec![
                    old.to_string_lossy().into_owned(),
                    shelf.path("Raw2", spare).to_string_lossy().into_owned()
                ],
                if reached { 2 } else { 1 }
            ),
            "the fixture: Raw2's second file is a member of its own, and Raw2 reaches raw 5 \
             where its copy came before the join"
        );
        if paused {
            stamp_changed(&shelf.path("Raw2", spare));
            scan::scan_root(&conn, &raw2).expect("walks the change");
            assert_eq!(
                crate::scan::tests::held_reading(&conn, raw2.id, spare).0,
                Some(ja.clone()),
                "the fixture: the second file's row holds raw 5's reading"
            );
            pause(&conn, &raw2);
        } else {
            std::fs::remove_file(shelf.path("Raw2", &home)).expect("removes Raw2's copy");
            scan::scan_root(&conn, &raw2).expect("walks the removal");
        }
        let raw4 = shelf.path("Raw", "Moved/Oshinoko v04.cbz");
        write_archive(&raw4, 3);
        let moved = shelf.path("Raw", "Moved/Oshinoko v05.cbz");
        std::fs::rename(&old, &moved).expect("moves raw 5");
        scan::scan_root(&conn, &shelved.raw).expect("rescan");
        let ends = [moved, shelf.path("Raw2", spare)].map(|at| at.to_string_lossy().into_owned());
        (raw4, ja, ends)
    };
    (shelf, library, raw4, ja, ends)
}

/// A member of a matched file's bytes at home answers for it only in its own
/// directory. Raw2, paused, holds a byte copy of joined raw 5 at its path, no
/// member but on raw 5's `member_root` or not, and a second file of its bytes,
/// a member of its own read by its held reading. Raw 5 moves beside a new raw
/// 4. The frozen read sets the copy aside (O336), the second file becomes the
/// moved raw 5's copy, and raw 4 follows it into the English series: it waits,
/// and ends there. The answering row's directory is the stray member's own, so
/// the exemption may ask `r.id` or `m.root_id` alike.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_file_moved_while_a_frozen_duplicate_stands() {
    let mut each = Vec::new();
    for reached in [false, true] {
        let (_shelf, library, raw4, ja, ends) = raw_5_moved_beside_a_member_of_its_bytes_in_raw2(
            "fill-frozen-duplicate",
            reached,
            true,
        );
        let conn = library.conn();
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        each.push((
            reached,
            probe.recording.written_so_far(),
            work_holding(&shot(&conn), &raw4).1.series,
            members_holding(&conn, &ja) == ends,
        ));
    }
    assert_eq!(
        each,
        [false, true].map(|reached| (reached, vec![], Some("Oshi".to_owned()), true)),
        "Raw2 not reaching raw 5, then reaching it: no pass writes raw 4, the job ends with it \
         in the English series, and raw 5's member at its new path beside the second file's"
    );
}

/// A member at home in another directory does not answer for a matched file:
/// the full derive may make it the moved file's copy. Raw2 holds a second file
/// of joined raw 5's bytes, a member of its own, and a byte copy at raw 5's
/// path, on raw 5's `member_root` or not, removed before raw 5 moves beside a
/// new raw 4. With no pause, the second file becomes the moved raw 5's copy at
/// the full derive, and raw 4 follows it into the English series: asked of any
/// directory, a pass would write raw 4 in the raws' series first.
#[test]
fn a_pass_leaves_a_volume_beside_a_joined_file_moved_while_its_duplicate_stands_in_another_directory()
 {
    let mut each = Vec::new();
    for reached in [false, true] {
        let (_shelf, library, raw4, ja, ends) = raw_5_moved_beside_a_member_of_its_bytes_in_raw2(
            "fill-duplicate-elsewhere",
            reached,
            false,
        );
        let conn = library.conn();
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        each.push((
            reached,
            probe.recording.written_so_far(),
            work_holding(&shot(&conn), &raw4).1.series,
            members_holding(&conn, &ja) == ends,
        ));
    }
    assert_eq!(
        each,
        [false, true].map(|reached| (reached, vec![], Some("Oshi".to_owned()), true)),
        "Raw2 not reaching raw 5, then reaching it: no pass writes raw 4, the job ends with it \
         in the English series, and raw 5's member at its new path beside the second file's"
    );
}

/// Joined raw 5, an archive or a folder volume, moved in the raws to
/// `Moved/Oshinoko v06`, its old path keeping a row: a file of `pages` pages
/// written there, or the folder left empty. Walked; answers the moved path.
fn raw_5_moved_to_volume_6(
    shelf: &Shelf,
    conn: &Connection,
    shelved: &crate::derive::joins::tests::Shelved,
    home: &str,
    pages: usize,
) -> String {
    let ja = digest(conn, &shelved.raw, home);
    let old = shelf.path("Raw", home);
    let moved = home.replace("Oshinoko/Oshinoko v05", "Moved/Oshinoko v06");
    let to = shelf.path("Raw", &moved);
    std::fs::create_dir_all(to.parent().expect("a directory")).expect("makes it");
    if old.is_dir() {
        std::fs::create_dir(&to).expect("makes the new folder");
        for page in std::fs::read_dir(&old).expect("lists raw 5's folder") {
            let page = page.expect("a page");
            std::fs::rename(page.path(), to.join(page.file_name())).expect("moves a page");
        }
    } else {
        std::fs::rename(&old, &to).expect("moves raw 5 away");
        write_archive_labelled(&old, pages, "jpg", "another file at raw 5's old path");
        stamp_changed(&old);
    }
    scan::scan_root(conn, &shelved.raw).expect("walks the move");
    assert_eq!(
        crate::scan::tests::held_reading(conn, shelved.raw.id, home).0,
        Some(ja),
        "the fixture: the row at raw 5's old path holds raw 5's reading"
    );
    moved
}

/// A work as [`work`] reads it: its series, volume and hub.
type WorkAt = (String, Option<f64>, String);

/// Each member holding `sha256`, in path order: its path under the raws and
/// its work.
fn shown(shelf: &Shelf, conn: &Connection, sha256: &str) -> Vec<(String, WorkAt)> {
    let raws = shelf.path("Raw", "").to_string_lossy().into_owned();
    let held: Vec<(String, i64)> = conn
        .prepare(
            "SELECT m.path, e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE m.sha256 = ?1 ORDER BY m.path",
        )
        .and_then(|mut stmt| {
            stmt.query_map([sha256], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the members");
    held.into_iter()
        .map(|(path, id)| {
            let under = path.strip_prefix(&raws).unwrap_or(&path).to_owned();
            (under, work(conn, id))
        })
        .collect()
}

/// English 5's work as [`work`] reads it, which joined raw 5 is on.
fn english_5() -> WorkAt {
    ("Oshi".to_owned(), Some(5.0), "en".to_owned())
}

/// What a Cancel leaves, as a test makes it: the raws walked again, a job
/// over them cancelled at its first check, and the raws paused.
fn cancelled_at_once(conn: &Connection, shelved: &crate::derive::joins::tests::Shelved) {
    scan::scan_root(conn, &shelved.raw).expect("walks the raws again");
    let probe = Probe::new(conn).cancelling(|_, _| true);
    assert!(
        matches!(fill(conn, &probe, Duration::ZERO), Outcome::Cancelled(_)),
        "the fixture: the job is cancelled"
    );
    pause(conn, &shelved.raw);
}

/// A directory the job reads frozen reads a file's bytes once, at its
/// member's path. Joined raw 5 moves away as volume 6, another file taking its
/// path, and a job stops once both are hashed: cancelled and the raws paused,
/// or stopped as a crash leaves it and a later job cancelled. The old row's
/// held reading and the moved row's digest would make raw 5 two members while
/// the pause stands, one a new volume 6. Zz 1, which a pass writes beside a
/// Zz 2 at raw 5's page count, ends as written.
#[test]
fn a_frozen_directory_reads_a_moved_files_bytes_once_at_its_members_path() {
    let cases = ["cancelled", "stopped, a later job cancelled"];
    let mut each = Vec::new();
    for case in cases {
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-frozen-once", false, false);
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, &home);
        let moved = raw_5_moved_to_volume_6(&shelf, &conn, &shelved, &home, 3);
        let raw = shelved.raw.id;
        let probe = Probe::new(&conn).cancelling(|conn, _| {
            conn.query_row(
                "SELECT count(*) = 2 FROM scan_entry
                  WHERE root_id = ?1 AND rel_path IN (?2, ?3) AND sha256 IS NOT NULL",
                rusqlite::params![raw, home, moved],
                |r| r.get(0),
            )
            .expect("reads the store")
        });
        let stopped = matches!(fill(&conn, &probe, Duration::ZERO), Outcome::Cancelled(_));
        assert_eq!(
            (stopped, shown(&shelf, &conn, &ja)),
            (true, vec![(home.clone(), english_5())]),
            "the fixture: the job stops before the full derive writes raw 5's work"
        );
        if case == "cancelled" {
            pause(&conn, &shelved.raw);
        } else {
            cancelled_at_once(&conn, &shelved);
        }
        let zz1 = shelf.path("Volumes", "Zz/Zz v01.cbz");
        write_archive(&zz1, 2);
        write_archive(&shelf.path("Volumes", "Zz/Zz v02.cbz"), 3);
        scan::scan_root(&conn, &shelved.volumes).expect("walks Zz");
        let probe = Probe::new(&conn);

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job over Volumes ends");

        each.push((
            case,
            probe.recording.written_so_far(),
            probe.shots().last().map(|shot| work_holding(shot, &zz1))
                == Some(work_holding(&shot(&conn), &zz1)),
            shown(&shelf, &conn, &ja),
        ));
    }
    assert_eq!(
        each,
        cases.map(|case| (case, vec![1], true, vec![(oshinoko(5), english_5())])),
        "cancelled, then stopped and a later job cancelled: a pass writes Zz 1 and the job ends \
         with it as written, and raw 5 is one member, at its old path, on English 5's work"
    );
}

/// A frozen directory reads a moved file at the path its member was moved to
/// alone, so it stays on its work while the pause stands. Joined raw 5 moves
/// away as volume 6, its old path keeping raw 5's held reading: another file
/// there and a job cancelled once raw 5's work is written, or a file of no
/// member there (an empty archive, or the folder its pages left) and a job run
/// to its end, then a job cancelled. Read by the old row's reading as well,
/// raw 5 would leave English 5's work as a new volume 6.
#[test]
fn a_frozen_directory_keeps_a_moved_file_on_its_work_where_its_old_row_holds_its_reading() {
    let cases = [
        ("another file", "Moved/Oshinoko v06.cbz"),
        ("an empty archive", "Moved/Oshinoko v06.cbz"),
        ("an empty folder", "Moved/Oshinoko v06"),
    ];
    let mut each = Vec::new();
    for (case, _) in cases {
        let folder = case == "an empty folder";
        let (shelf, library, shelved, home) = a_joined_raw_5("fill-frozen-left", folder, false);
        let conn = library.conn();
        let ja = digest(&conn, &shelved.raw, &home);
        let pages = if case == "another file" { 3 } else { 0 };
        let moved = raw_5_moved_to_volume_6(&shelf, &conn, &shelved, &home, pages);
        if case == "another file" {
            let at = shelf.path("Raw", &moved).to_string_lossy().into_owned();
            let probe =
                Probe::new(&conn).cancelling(|conn, _| members_holding(conn, &ja) == [at.clone()]);
            assert!(
                matches!(fill(&conn, &probe, Duration::ZERO), Outcome::Cancelled(_)),
                "the fixture: the job is cancelled once raw 5's work is written"
            );
            pause(&conn, &shelved.raw);
        } else {
            derive::done(fill(&conn, &Probe::new(&conn), Duration::ZERO)).expect("the job ends");
            cancelled_at_once(&conn, &shelved);
        }
        assert_eq!(
            crate::scan::tests::held_reading(&conn, shelved.raw.id, &home).0,
            Some(ja.clone()),
            "the fixture: the row at raw 5's old path holds raw 5's reading still ({case})"
        );

        derive::derive(&conn).expect("a derive while paused");

        each.push((case, shown(&shelf, &conn, &ja)));
    }
    assert_eq!(
        each,
        cases.map(|(case, moved)| (case, vec![(moved.to_owned(), english_5())])),
        "another file at the old path, then an empty archive, then an empty folder: raw 5 is \
         one member, at the moved path, on English 5's work"
    );
}

/// An old path whose file makes no member keeps raw 5's held reading through
/// a Scan, so each Cancel after meets it again (O581). Joined raw 5 moves away
/// as volume 6, an empty archive left at its path; a job runs to its end and
/// the next is cancelled; Scan is pressed and its job runs to its end, and the
/// next is cancelled. Raw 5 is on English 5's work at the moved path after
/// each, the first frozen derive as the second.
#[test]
fn a_frozen_directory_keeps_a_moved_file_on_its_work_after_a_scan_and_a_second_cancel() {
    let (shelf, library, shelved, home) = a_joined_raw_5("fill-frozen-again", false, false);
    let conn = library.conn();
    let ja = digest(&conn, &shelved.raw, &home);
    let moved = raw_5_moved_to_volume_6(&shelf, &conn, &shelved, &home, 0);
    derive::done(fill(&conn, &Probe::new(&conn), Duration::ZERO)).expect("the job ends");
    cancelled_at_once(&conn, &shelved);
    derive::derive(&conn).expect("a derive while paused");
    let first = shown(&shelf, &conn, &ja);
    scan::unpause(&conn, std::slice::from_ref(&shelved.raw)).expect("Scan is pressed");
    scan::scan_root(&conn, &shelved.raw).expect("walks the raws");
    derive::done(fill(&conn, &Probe::new(&conn), Duration::ZERO)).expect("the Scan's job ends");
    let scanned = (
        shown(&shelf, &conn, &ja),
        crate::scan::tests::held_reading(&conn, shelved.raw.id, &home).0,
    );
    cancelled_at_once(&conn, &shelved);

    derive::derive(&conn).expect("a derive while paused again");

    let at_moved = vec![(moved, english_5())];
    assert_eq!(
        (first, scanned, shown(&shelf, &conn, &ja)),
        (at_moved.clone(), (at_moved.clone(), Some(ja)), at_moved),
        "raw 5 is one member at the moved path on English 5's work while paused, after the \
         Scan, whose job leaves the old row's reading held, and while paused again"
    );
}

/// Design §7: `Placed::of` reads the groups a pass holds back as well as those
/// it writes. A rule titles a book's Korean apart once its file has changed:
/// the English keeps the book's row and is held back (rule 2), and the Korean,
/// a group of its own holding a file of that row, is written on a work of its
/// own, where the full derive leaves it. Only the held English's files in
/// `placed` keep the Korean off the book's row.
#[test]
fn a_pass_writes_a_part_a_rule_titles_apart_on_a_work_of_its_own() {
    let (tree, library, root) = translations("fill-placed", &["en", "ko"]);
    let conn = library.conn();
    let was = rows(&conn);
    let korean = "Rowling/Stone [ko]/Stone [ko].cbz";
    write_archive(&tree.path().join(korean), 4);
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (100, 'regex', '^Rowling/(?<title>Stone \\[ko\\])/', 'path')",
        [],
    )
    .expect("the Korean titled apart");
    scan::scan_root(&conn, &root).expect("rescan");
    hash_now(&conn, &root, korean);

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(1),
        "the Korean, on a work of its own"
    );
    let mut now = rows(&conn);
    let book = *was.keys().next().expect("the book's row");
    assert_eq!(now.get(&book), was.get(&book), "the book's row as it was");
    let (written, members) = the_new_work(&mut now, &was);
    assert_eq!(
        members
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>(),
        [tree.path().join(korean).to_string_lossy()],
        "the new work holds the Korean"
    );
    derive::derive(&conn).expect("the full derive");
    assert_eq!(
        holder(&conn, &digest(&conn, &root, korean)),
        written,
        "the full derive leaves the Korean where the pass wrote it"
    );
}

/// Rule 3 reads a copy's directory as well as its primary's: a volume whose
/// copy sits beside a file a stored match names waits, though its primary
/// sits where no pair names anything.
#[test]
fn a_pass_leaves_a_volume_whose_copy_sits_in_a_directory_a_stored_match_names() {
    let first = crate::tests::tempdir("fill-copy-first");
    let second = crate::tests::tempdir("fill-copy-second");
    write_archive(&second.path().join(ALPHA), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let unnamed = scanned_root(&conn, first.path(), None);
    let named = scanned_root(&conn, second.path(), None);
    derive::derive(&conn).expect("derives");
    let file = digest(&conn, &named, ALPHA);
    conn.execute(
        "INSERT INTO content_match
             (sha_a, sha_b, v, long_paired, a_slots, b_slots, a_pages, b_pages, runs, state)
         VALUES (?1, ?2, 0.5, 1, 3, 3, 3, 3, '[]', 'split')",
        [LOW, file.as_str()],
    )
    .expect("a split pair under the bar");
    for tree in [&first, &second] {
        write_archive_labelled(&tree.path().join(ALPHA_V02), 3, "jpg", "Alpha v02, twice");
    }
    for root in [&unnamed, &named] {
        scan::scan_root(&conn, root).expect("rescan");
        hash_now(&conn, root, ALPHA_V02);
    }

    let outcome = pass(&conn);

    assert_eq!(
        outcome,
        Outcome::Done(0),
        "volume 2's copy sits beside the file the pair names"
    );
    derive::derive(&conn).expect("the full derive");
    assert_eq!(
        conn.query_row(
            "SELECT m.path, count(r.root_id) FROM member m JOIN member_root r ON r.member_id = m.id
              WHERE m.path LIKE '%Alpha v02.cbz' GROUP BY m.id",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .expect("volume 2's member"),
        (
            first.path().join(ALPHA_V02).to_string_lossy().into_owned(),
            2
        ),
        "the fixture: the primary is the unnamed directory's file, the copy the named one's"
    );
}

// --- The filling derive (O314, design §7) ------------------------------------

/// A [`Watch`] over the connection the filling derive writes through, for its
/// tests. Around a [`Recording`], it notes the order entries gain a digest in,
/// keeps the store's works as each [`Watch::written`] call finds them and then
/// acts on the store as `on_written` says, moves a test clock on by `tick` at
/// each Hashing report and by `check` at each cancel check, and answers
/// cancelled from `stop` over the store and the reports so far.
struct Probe<'c> {
    conn: &'c Connection,
    recording: Recording,
    /// Every entry holding a digest at the last report, as `(root_id, rel_path)`.
    held: RefCell<BTreeSet<(i64, String)>>,
    /// Each entry as it gained a digest.
    digested: RefCell<Vec<(i64, String)>>,
    shots: RefCell<Vec<Shot>>,
    start: Instant,
    elapsed: Cell<Duration>,
    tick: Duration,
    check: Duration,
    stop: Stop<'c>,
    on_written: Act<'c>,
}

/// When a [`Probe`] answers cancelled, from the store and the reports so far.
type Stop<'c> = Box<dyn Fn(&Connection, &[Report]) -> bool + 'c>;

/// What a [`Probe`] does to the store at each `written` call.
type Act<'c> = Box<dyn Fn(&Connection) + 'c>;

impl<'c> Probe<'c> {
    /// A probe whose clock stands still and which never cancels. A digest
    /// held before it is made is not noted.
    fn new(conn: &'c Connection) -> Self {
        Self {
            conn,
            recording: Recording::default(),
            held: RefCell::new(digests_held(conn)),
            digested: RefCell::default(),
            shots: RefCell::default(),
            start: Instant::now(),
            elapsed: Cell::new(Duration::ZERO),
            tick: Duration::ZERO,
            check: Duration::ZERO,
            stop: Box::new(|_, _| false),
            on_written: Box::new(|_| {}),
        }
    }

    /// Its clock moves on by `tick` at each Hashing report.
    fn ticking(self, tick: Duration) -> Self {
        Self { tick, ..self }
    }

    /// Its clock moves on by `check` at each cancel check too. A pass drops
    /// every report but forwards the check, so on this clock a pass takes time.
    fn checking(self, check: Duration) -> Self {
        Self { check, ..self }
    }

    /// It answers cancelled while `stop` holds over the store and the reports.
    fn cancelling(self, stop: impl Fn(&Connection, &[Report]) -> bool + 'c) -> Self {
        Self {
            stop: Box::new(stop),
            ..self
        }
    }

    /// At each `written` call, once its shot is taken, it does `act` to the
    /// store, as a reader of a volume a pass wrote would.
    fn on_written(self, act: impl Fn(&Connection) + 'c) -> Self {
        Self {
            on_written: Box::new(act),
            ..self
        }
    }

    /// The test clock.
    fn now(&self) -> Instant {
        self.start + self.elapsed.get()
    }

    /// Each entry that gained a digest, as `(root_id, rel_path)`, in the order
    /// the reports found them.
    fn digested(&self) -> Vec<(i64, String)> {
        self.digested.borrow().clone()
    }

    /// The store at each `written` call.
    fn shots(&self) -> Vec<Shot> {
        self.shots.borrow().clone()
    }
}

impl Watch for Probe<'_> {
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        let now = digests_held(self.conn);
        let mut held = self.held.borrow_mut();
        self.digested
            .borrow_mut()
            .extend(now.difference(&held).cloned());
        *held = now;
        if step == Step::Hashing {
            self.elapsed.set(self.elapsed.get() + self.tick);
        }
        self.recording.at(step, done, of);
    }

    fn cancelled(&self) -> bool {
        self.elapsed.set(self.elapsed.get() + self.check);
        (self.stop)(self.conn, &self.recording.reports())
    }

    fn written(&self, works: u64) {
        self.recording.written(works);
        self.shots.borrow_mut().push(shot(self.conn));
        (self.on_written)(self.conn);
    }
}

/// Every entry holding a digest, as `(root_id, rel_path)`.
fn digests_held(conn: &Connection) -> BTreeSet<(i64, String)> {
    conn.prepare("SELECT root_id, rel_path FROM scan_entry WHERE sha256 IS NOT NULL")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the digests")
}

/// A work as a reader sees it, by [`shot`].
#[derive(Debug, Clone, PartialEq)]
struct WorkRow {
    slug: String,
    series: Option<String>,
    volume: Option<f64>,
    hub: String,
    content_key: String,
    /// Each edition by key, its members as `(ordinal, path, digest)` in order.
    editions: BTreeMap<String, Vec<(i64, String, String)>>,
}

/// The store's works by id, and its `member_root` rows as `(work id, member
/// path, root id)`.
#[derive(Debug, Clone, PartialEq)]
struct Shot {
    works: BTreeMap<i64, WorkRow>,
    member_roots: BTreeSet<(i64, String, i64)>,
}

/// An edition's work and key, beside one member's ordinal, path and digest,
/// or none where it holds no member.
type EditionMember = (i64, String, Option<i64>, Option<String>, Option<String>);

fn shot(conn: &Connection) -> Shot {
    let mut works: BTreeMap<i64, WorkRow> = conn
        .prepare(
            "SELECT w.id, w.slug, s.sort_title, w.volume, w.hub, w.content_key
               FROM work w LEFT JOIN series s ON s.id = w.series_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    WorkRow {
                        slug: r.get(1)?,
                        series: r.get(2)?,
                        volume: r.get(3)?,
                        hub: r.get(4)?,
                        content_key: r.get(5)?,
                        editions: BTreeMap::new(),
                    },
                ))
            })?
            .collect()
        })
        .expect("reads the works");
    let members: Vec<EditionMember> = conn
        .prepare(
            "SELECT e.work_id, e.key, m.ordinal, m.path, m.sha256
               FROM edition e LEFT JOIN member m ON m.edition_id = e.id
              ORDER BY e.work_id, e.key, m.ordinal",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect()
        })
        .expect("reads the editions");
    for (work_id, key, ordinal, path, sha256) in members {
        let edition = works
            .get_mut(&work_id)
            .expect("an edition's work")
            .editions
            .entry(key)
            .or_default();
        if let (Some(ordinal), Some(path), Some(sha256)) = (ordinal, path, sha256) {
            edition.push((ordinal, path, sha256));
        }
    }
    let member_roots = conn
        .prepare(
            "SELECT e.work_id, m.path, mr.root_id FROM member_root mr
               JOIN member m ON m.id = mr.member_id JOIN edition e ON e.id = m.edition_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("reads the member_root rows");
    Shot {
        works,
        member_roots,
    }
}

/// A pass after every group that completes, on the system clock.
fn every_group() -> Cadence<'static> {
    Cadence {
        floor: Duration::ZERO,
        factor: 0,
        now: &Instant::now,
    }
}

/// The filling derive over every directory under `probe`, on its clock, at
/// `floor` and factor 0.
fn fill(conn: &Connection, probe: &Probe<'_>, floor: Duration) -> Outcome<derive::DeriveReport> {
    let now = || probe.now();
    derive::derive_filling_at(
        conn,
        derive::Documents::Reconcile,
        &[],
        probe,
        &Cadence {
            floor,
            factor: 0,
            now: &now,
        },
    )
    .expect("derives")
}

const A_V2: &str = "A/A v2.cbz";
const A_V10: &str = "A/A v10.cbz";
const B_V1: &str = "B/B v1.cbz";

/// Design §8 test 1 and call 1: the queue hashes by own key across every
/// directory, then by directory, so each volume of a series is hashed in every
/// directory, in volume order, before the next series. `ja` has the lower id
/// and the later edition key, and `v10` sorts before `v2` by path.
#[test]
fn the_queue_hashes_a_series_in_every_directory_before_the_next() {
    let ja = crate::tests::tempdir("fill-queue-ja");
    let en = crate::tests::tempdir("fill-queue-en");
    for tree in [&ja, &en] {
        for rel_path in [A_V2, A_V10, B_V1] {
            // Distinct bytes: `write_archive` labels each page by its path.
            write_archive(&tree.path().join(rel_path), 2);
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let ja_root = scanned_root(&conn, ja.path(), Some("ja"));
    let en_root = scanned_root(&conn, en.path(), Some("en"));
    assert!(ja_root.id < en_root.id, "the fixture: ja has the lower id");
    let probe = Probe::new(&conn);

    fill(&conn, &probe, Duration::ZERO);

    let (ja, en) = (ja_root.id, en_root.id);
    assert_eq!(
        probe.digested(),
        [
            (ja, A_V2),
            (en, A_V2),
            (ja, A_V10),
            (en, A_V10),
            (ja, B_V1),
            (en, B_V1)
        ]
        .map(|(root, rel_path)| (root, rel_path.to_string())),
        "A's volume 2 in both directories, then its volume 10, then B"
    );
}

/// Design call 5: a pass waits the floor, or the factor times the last pass
/// where that is longer.
#[test]
fn pass_due_waits_the_floor_or_the_factor_times_the_last_pass_whichever_is_longer() {
    let job = Cadence {
        floor: Duration::from_secs(10),
        factor: 4,
        now: &Instant::now,
    };
    let secs = Duration::from_secs;
    assert_eq!(
        [(9, 0), (10, 0), (11, 3), (12, 3)].map(|(since, last_pass)| pass_due(
            secs(since),
            secs(last_pass),
            &job
        )),
        [false, true, false, true],
        "the floor alone after an instant pass, four times a 3 s pass after one"
    );
    assert!(
        pass_due(Duration::ZERO, Duration::ZERO, &every_group()),
        "no floor and no factor: a pass is always due"
    );
}

/// One-file series, one volume each, in series order.
const SERIES: [&str; 6] = [
    "Alpha/Alpha v01.cbz",
    "Beta/Beta v01.cbz",
    "Delta/Delta v01.cbz",
    "Epsilon/Epsilon v01.cbz",
    "Gamma/Gamma v01.cbz",
    "Zeta/Zeta v01.cbz",
];

/// One directory under `label` holding `files`, scanned into a store.
fn scanned_files(label: &str, files: &[&str]) -> (TempDir, Library) {
    let tree = crate::tests::tempdir(label);
    for file in files {
        write_archive(&tree.path().join(file), 2);
    }
    let library = Library::open_in_memory().expect("open");
    scanned_root(&library.conn(), tree.path(), None);
    (tree, library)
}

/// Design call 5: a pass runs once a group completes and the cadence allows,
/// and never after the last digest, which the full derive follows at once.
#[test]
fn a_pass_runs_only_once_a_group_is_complete_and_never_after_the_last_digest() {
    {
        let (_tree, library) = scanned_files("fill-cadence-every", &SERIES[..3]);
        let conn = library.conn();
        let probe = Probe::new(&conn);

        fill(&conn, &probe, Duration::ZERO);

        assert_eq!(
            probe.recording.written_so_far(),
            [1, 2],
            "a pass after each of the first two series, none after the third"
        );
        assert_eq!(
            scalar(&conn, "SELECT count(*) FROM work"),
            3,
            "the full derive writes the third"
        );
    }
    let (_tree, library) = scanned_files("fill-cadence-floor", &SERIES);
    let conn = library.conn();
    // The opening report moves the clock to 4 s before hashing begins, so
    // the first entry's report reads 8 s and the third's 16 s, the first at
    // least 10 s after hashing began.
    let probe = Probe::new(&conn).ticking(Duration::from_secs(4));

    fill(&conn, &probe, Duration::from_secs(10));

    assert_eq!(
        probe.recording.written_so_far(),
        [3],
        "one pass, after the third series; the sixth is the last"
    );
}

/// Design call 5: after a pass, the next waits `factor` times as long as the
/// pass took, so passes keep to a share of the job as their cost grows with
/// the store. Each Hashing report moves the clock 9 s and each cancel check
/// 1 s, so a digest takes 10 s, and the first pass, which checks the flag at
/// each of the six entries it reads and the one work it writes, takes 7 s: the
/// next is due 28 s on, after the fourth digest.
#[test]
fn a_pass_waits_the_factor_times_the_last_pass_before_the_next() {
    let (_tree, library) = scanned_files("fill-cadence-factor", &SERIES);
    let conn = library.conn();
    let probe = Probe::new(&conn)
        .ticking(Duration::from_secs(9))
        .checking(Duration::from_secs(1));
    let now = || probe.now();

    derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &probe,
            &Cadence {
                floor: Duration::ZERO,
                factor: 4,
                now: &now,
            },
        )
        .expect("derives"),
    )
    .expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1, 4],
        "a pass after Alpha's digest, then none until 28 s of hashing has followed it, \
         three digests on"
    );
}

/// Design call 5's floor, in the cadence a scan job runs at: a job over a
/// small store, hashed in far less than the floor, runs no early pass, and its
/// full derive writes every volume. With no floor it would pass after the
/// first volume.
#[test]
fn a_scan_jobs_cadence_runs_no_pass_in_a_job_shorter_than_its_floor() {
    let (_tree, library) = scanned_files("fill-job-floor", &SERIES[..3]);
    let conn = library.conn();
    let probe = Probe::new(&conn);

    let outcome =
        derive::derive_filling(&conn, derive::Documents::Reconcile, &[], &probe).expect("derives");

    assert!(
        matches!(outcome, Outcome::Done(_)),
        "the job ends: {outcome:?}"
    );
    assert!(probe.shots().is_empty(), "no pass wrote a volume");
    assert_eq!(
        work_series(&conn),
        ["Alpha", "Beta", "Delta"],
        "the full derive wrote every volume"
    );
}

/// Twelve one-file series, one volume each, in series order: with [`SERIES`]'s
/// six, a factor of 3 and one of 4 pass alike.
const DOZEN: [&str; 12] = [
    "Aa/Aa v01.cbz",
    "Ab/Ab v01.cbz",
    "Ac/Ac v01.cbz",
    "Ad/Ad v01.cbz",
    "Ae/Ae v01.cbz",
    "Af/Af v01.cbz",
    "Ag/Ag v01.cbz",
    "Ah/Ah v01.cbz",
    "Ai/Ai v01.cbz",
    "Aj/Aj v01.cbz",
    "Ak/Ak v01.cbz",
    "Al/Al v01.cbz",
];

/// Design call 5's factor, in the cadence a scan job runs at. Each Hashing
/// report moves the clock 9 s and each cancel check 1 s, so a digest takes
/// 10 s and the first pass is due at the floor, after Aa's; it checks the flag
/// at each of the twelve entries it reads and the one work it writes, 13 s. The
/// next is due 52 s on, after Ag's digest, and takes 18 s for its six works;
/// the 72 s due after it outlasts the rest. A factor of 3 would pass after the
/// fifth digest, one of 5 after the eighth.
#[test]
fn a_scan_jobs_cadence_waits_four_times_the_last_pass_before_the_next() {
    let (_tree, library) = scanned_files("fill-job-factor", &DOZEN);
    let conn = library.conn();
    let probe = Probe::new(&conn)
        .ticking(Duration::from_secs(9))
        .checking(Duration::from_secs(1));
    let now = || probe.now();

    derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &probe,
            &Cadence::scan_job(&now),
        )
        .expect("derives"),
    )
    .expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1, 7],
        "a pass after Aa's digest, then none until four times its 13 s of hashing \
         has followed it, six digests on"
    );
}

const C002: &str = "Alpha/Alpha - v01 - c002.cbz";
const FAGLES: &str = "Homer/The Odyssey (Fagles)/The Odyssey (Fagles).cbz";
const LATTIMORE: &str =
    "Homer/The Odyssey of Homer (Lattimore)/The Odyssey of Homer (Lattimore).cbz";
const ZETA: &str = "Zeta/Zeta v01.cbz";

/// Test 3's two directories under `label`: in `en`, a volume of two files;
/// Beta's volume in both, of other bytes; a book with no volume in each, which
/// a rule titles alike; Gamma's volume in `ja`, and its bytes as Zeta's in
/// `en`, a copy whose own key sorts last. Each file's pages are labelled by a
/// tag naming no tempdir, so two copies of the fixture hash alike.
fn every_shape(label: &str) -> (TempDir, TempDir) {
    let ja = crate::tests::tempdir(&format!("{label}-ja"));
    let en = crate::tests::tempdir(&format!("{label}-en"));
    for (tree, rel_path, pages, tag) in [
        (&en, C001, 2, "Alpha c001"),
        (&en, C002, 3, "Alpha c002"),
        (&ja, BETA, 2, "Beta ja"),
        (&en, BETA, 3, "Beta en"),
        (&ja, FAGLES, 2, "Fagles"),
        (&en, LATTIMORE, 3, "Lattimore"),
        (&ja, GAMMA_CBZ, 4, "Gamma"),
        (&en, ZETA, 4, "Gamma"),
    ] {
        write_archive_labelled(&tree.path().join(rel_path), pages, "jpg", tag);
    }
    (ja, en)
}

/// [`every_shape`]'s directories scanned into `conn`, `ja` first so it has the
/// lower id, under the rule titling both books "The Odyssey"
/// (`volume_less_books_a_rule_titles_alike_are_one_work_an_edition_each`).
fn scanned_shapes(conn: &Connection, ja: &TempDir, en: &TempDir) -> (scan::Root, scan::Root) {
    let ja = scanned_root(conn, ja.path(), Some("ja"));
    let en = scanned_root(conn, en.path(), Some("en"));
    assert!(ja.id < en.id, "the fixture: ja has the lower id");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on)
         VALUES (99, 'regex', '^Homer/(?<title>The Odyssey)\\b', 'path')",
        [],
    )
    .expect("one title for both translations");
    (ja, en)
}

/// What a job and one derive must agree on, with no id in it: the works by
/// slug as (series, volume, hub, content key), the editions as (slug, key), the
/// members as (slug, edition key, ordinal, `rel_path`, digest), and the
/// `member_root` rows as (member `rel_path`, the root's place in id order).
#[derive(Debug, PartialEq)]
struct Settled {
    works: BTreeMap<String, (Option<String>, Option<f64>, String, String)>,
    editions: BTreeSet<(String, String)>,
    members: BTreeSet<(String, String, i64, String, String)>,
    member_roots: BTreeSet<(String, usize)>,
}

fn settled(conn: &Connection) -> Settled {
    let roots: Vec<(i64, String)> = conn
        .prepare("SELECT id, path FROM root ORDER BY id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the roots");
    let place = |root_id: i64| {
        roots
            .iter()
            .position(|(id, _)| *id == root_id)
            .expect("a root")
    };
    let rel_path = |root_id: i64, path: &str| -> String {
        path.strip_prefix(roots[place(root_id)].1.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or_else(|| panic!("{path} is under directory {root_id}"))
            .to_string()
    };
    let works = conn
        .prepare(
            "SELECT w.slug, s.sort_title, w.volume, w.hub, w.content_key
               FROM work w LEFT JOIN series s ON s.id = w.series_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            })?
            .collect()
        })
        .expect("reads the works");
    let editions = conn
        .prepare("SELECT w.slug, e.key FROM edition e JOIN work w ON w.id = e.work_id")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the editions");
    let members: Vec<(String, String, i64, i64, String, String)> = conn
        .prepare(
            "SELECT w.slug, e.key, m.ordinal, m.root_id, m.path, m.sha256 FROM member m
               JOIN edition e ON e.id = m.edition_id JOIN work w ON w.id = e.work_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect()
        })
        .expect("reads the members");
    let member_roots: Vec<(i64, String, i64)> = conn
        .prepare(
            "SELECT m.root_id, m.path, mr.root_id FROM member_root mr
               JOIN member m ON m.id = mr.member_id",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("reads the member_root rows");
    Settled {
        works,
        editions,
        members: members
            .into_iter()
            .map(|(slug, key, ordinal, root_id, path, sha256)| {
                (slug, key, ordinal, rel_path(root_id, &path), sha256)
            })
            .collect(),
        member_roots: member_roots
            .into_iter()
            .map(|(root_id, path, copy_root)| (rel_path(root_id, &path), place(copy_root)))
            .collect(),
    }
}

/// Design §8 test 3: every volume a pass writes stands
/// at the job's end as the pass wrote it, a copy hashed later adding only its
/// directory; and the job leaves the store, and counts in its report, what one
/// derive over the same files does. The titled book's two files share one own
/// key, so neither translation is written without the other.
#[test]
fn every_volume_a_pass_writes_is_the_one_the_job_ends_with() {
    let (ja, en) = every_shape("fill-same");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_, en_root) = scanned_shapes(&conn, &ja, &en);
    let probe = Probe::new(&conn);

    let filled = derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1, 2, 3, 4],
        "a pass after Alpha, Beta, Gamma and the Odyssey; none after Zeta's file, the last"
    );
    let last = shot(&conn);
    let shots = probe.shots();
    for (pass, shot) in shots.iter().enumerate() {
        for (id, row) in &shot.works {
            assert_eq!(
                Some(row),
                last.works.get(id),
                "work {id} at the job's end, as pass {pass} wrote it"
            );
        }
    }
    let gamma = ja.path().join(GAMMA_CBZ).to_string_lossy().into_owned();
    let gamma_work = *last
        .works
        .iter()
        .find(|(_, row)| {
            row.editions
                .values()
                .flatten()
                .any(|(_, path, _)| *path == gamma)
        })
        .expect("Gamma's work")
        .0;
    let before = &shots.last().expect("a pass wrote").member_roots;
    assert!(
        before.is_subset(&last.member_roots),
        "no member_root row a pass wrote goes"
    );
    assert_eq!(
        last.member_roots
            .difference(before)
            .cloned()
            .collect::<Vec<_>>(),
        [(gamma_work, gamma, en_root.id)],
        "Gamma's work gains Zeta's directory, as a copy's, and nothing else is added"
    );

    let (ja_once, en_once) = every_shape("fill-same-once");
    let once = Library::open_in_memory().expect("open");
    let once_conn = once.conn();
    scanned_shapes(&once_conn, &ja_once, &en_once);
    let derived = derive::derive(&once_conn).expect("one derive");

    assert_eq!(
        settled(&conn),
        settled(&once_conn),
        "the job leaves what one derive does"
    );
    assert_eq!(
        (filled.works, filled.editions, filled.members),
        (derived.works, derived.editions, derived.members),
        "the job's report counts every volume, those a pass wrote among them"
    );
    assert_eq!(filled, derived, "and every other figure as one derive's");
}

/// Design call 14 allows the job two differences from one derive, and a
/// report is neither: a hand document that does not read, at the address of
/// a volume a pass writes, is named in the job's report as one derive names
/// it, once. The full derive leaves no work as it stands while its document
/// is unread, so its own read-back names the file. A malformed file, and one
/// a newer taiyaku wrote (O264).
#[test]
fn a_hand_document_that_does_not_read_beside_a_volume_a_pass_writes_is_reported_once() {
    let newer = format!(
        r#"{{"taiyaku_library": {}, "updated": "2026-11-10T09:00:00Z", "title": "Newer"}}"#,
        crate::librarydoc::VERSION + 1
    );
    for (label, text) in [("malformed", "{ not json"), ("newer", newer.as_str())] {
        let tree = fixture();
        let document = {
            let first = Library::open_in_memory().expect("open");
            let conn = first.conn();
            let root = scan::add_root(&conn, tree.path()).expect("add");
            conn.execute(
                "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
                [root.id],
            )
            .expect("the fixture keeps documents");
            scan::scan_root(&conn, &root).expect("scan");
            derive::derive(&conn).expect("first derive");
            let work_id: i64 = conn
                .query_row("SELECT id FROM work WHERE volume = 1.0", [], |r| r.get(0))
                .expect("the volume 1 work");
            let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
            doc.title = Some(HAND_TITLE.to_string());
            crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
            crate::hand::save(&conn, work_id)
                .expect("saving")
                .path
                .expect("the document's address")
        };
        std::fs::write(&document, text).expect("spoils the document");
        let job = Library::open_in_memory().expect("open");
        let conn = job.conn();
        let root = scan::add_root(&conn, tree.path()).expect("add");
        scan::scan_root(&conn, &root).expect("scan");
        let recording = Recording::default();

        let filled = derive::done(
            derive::derive_filling_at(
                &conn,
                derive::Documents::Reconcile,
                &[],
                &recording,
                &every_group(),
            )
            .expect("derives"),
        )
        .expect("the job ends");

        assert_eq!(
            recording.written_so_far(),
            [1],
            "a pass writes volume 1 beside its {label} document"
        );
        let once = Library::open_in_memory().expect("open");
        let once_conn = once.conn();
        let root = scan::add_root(&once_conn, tree.path()).expect("add");
        scan::scan_root(&once_conn, &root).expect("scan");
        let derived = derive::derive(&once_conn).expect("one derive");
        assert_eq!(
            derived.unreadable_hand.len(),
            1,
            "the fixture: one derive names the {label} document: {:?}",
            derived.unreadable_hand
        );
        assert_eq!(
            filled.unreadable_hand, derived.unreadable_hand,
            "the job names the {label} document once, as one derive does"
        );
        assert_eq!(
            filled, derived,
            "the job's report is one derive's ({label})"
        );
    }
}

/// A store on disk under `label`, so its commits can be counted: Alpha's,
/// Beta's and Gamma's volume derived, then Alpha's and Beta's files rewritten
/// and the directory scanned again.
fn rewritten(label: &str) -> (TempDir, TempDir, Library) {
    let tree = crate::tests::tempdir(label);
    for file in [ALPHA, BETA, GAMMA_CBZ] {
        write_archive(&tree.path().join(file), 2);
    }
    let store = crate::tests::tempdir(&format!("{label}-store"));
    let library = Library::open(&store.path().join("library.db")).expect("open");
    {
        let conn = library.conn();
        let root = scanned_root(&conn, tree.path(), None);
        derive::derive(&conn).expect("first derive");
        for file in [ALPHA, BETA] {
            write_archive(&tree.path().join(file), 4);
        }
        scan::scan_root(&conn, &root).expect("rescan");
    }
    (tree, store, library)
}

/// On a rescan where a work holds every volume, no pass writes and none is
/// reported, and the job commits what one derive does, so a rescan takes no
/// write lock one derive would not (O272). Alpha's and Beta's files changed,
/// so both are queued and a pass runs after Alpha's.
#[test]
fn a_rescan_with_nothing_new_writes_nothing_during_its_hashing() {
    let recording = Recording::default();
    let (_tree, _store, library) = rewritten("fill-rescan");
    let (filled, _) = commits_during(&mut library.conn(), |conn| {
        derive::derive_filling_at(
            conn,
            derive::Documents::Reconcile,
            &[],
            &recording,
            &every_group(),
        )
        .expect("the job derives")
    });
    let (_tree, _store, library) = rewritten("fill-rescan-once");
    let (derived, _) = commits_during(&mut library.conn(), |conn| {
        derive::derive_watched(
            conn,
            derive::Documents::Reconcile,
            &[],
            &Recording::default(),
        )
        .expect("one derive")
    });

    assert_eq!(
        recording.written_so_far(),
        Vec::<u64>::new(),
        "no pass wrote a volume"
    );
    assert_eq!(filled, derived, "the job commits what one derive does");
}

/// Design §7, "The report and the feed": the job's line goes from Hashing to
/// Writing once, its total known from the second report on. The full derive
/// at the end has nothing to hash, and its own Hashing reports are not passed
/// on.
#[test]
fn a_job_reports_hashing_once_then_writing() {
    let (tree, library) = scanned_files("fill-reports", &SERIES[..3]);
    let total: u64 = SERIES[..3]
        .iter()
        .map(|file| {
            std::fs::metadata(tree.path().join(file))
                .expect("the file")
                .len()
        })
        .sum();
    let conn = library.conn();
    let recording = Recording::default();

    derive::derive_filling_at(
        &conn,
        derive::Documents::Reconcile,
        &[],
        &recording,
        &every_group(),
    )
    .expect("derives");

    let reports = recording.reports();
    let hashing: Vec<Report> = reports
        .iter()
        .copied()
        .filter(|(step, _, _)| *step == Step::Hashing)
        .collect();
    assert_eq!(
        hashing.first(),
        Some(&(Step::Hashing, 0, None)),
        "sizing first, its total not yet known"
    );
    assert!(
        hashing[1..].iter().all(|(_, _, of)| *of == Some(total)),
        "every later Hashing report has the total, {total}: {hashing:?}"
    );
    let last = reports
        .iter()
        .rposition(|report| *report == (Step::Hashing, total, Some(total)))
        .expect("the count reaches the total");
    assert_eq!(
        reports.get(last + 1).map(|(step, _, _)| *step),
        Some(Step::Writing),
        "Writing follows: {reports:?}"
    );
}

/// Design §7, "Cancel", and call 7: a cancel asked while a file is hashed
/// stops the job before the pass that file's group would bring on, so no pass
/// reads the store first. A pass reads the rules before anything else, so no
/// statement after the digest reads them.
#[test]
fn a_cancel_during_a_digest_stops_before_the_pass_it_would_run() {
    let (_tree, library) = scanned_files("fill-cancel-before-pass", &SERIES[..2]);
    let mut conn = library.conn();
    let statements = Arc::new(Mutex::new(Vec::<String>::new()));
    let traced = Arc::clone(&statements);
    conn.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT,
        Some(move |event: TraceEvent<'_>| {
            if let TraceEvent::Stmt(_, sql) = event {
                traced.lock().expect("the statements").push(sql.to_string());
            }
        }),
    )
    .expect("traces");
    let probe = Probe::new(&conn).cancelling(|conn, _| {
        scalar(
            conn,
            "SELECT count(*) FROM scan_entry WHERE sha256 IS NOT NULL",
        ) > 0
    });

    let outcome = fill(&conn, &probe, Duration::ZERO);

    assert_eq!(
        outcome,
        Outcome::Cancelled(Step::Hashing),
        "the job stops in the step it shows"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 0, "no work");
    let statements = statements.lock().expect("the statements");
    let digested = statements
        .iter()
        .position(|sql| sql.contains("SET sha256 ="))
        .expect("Alpha's digest");
    assert_eq!(
        statements[digested..]
            .iter()
            .filter(|sql| sql.contains("FROM rule "))
            .count(),
        0,
        "no pass ran after Alpha's digest"
    );
}

// --- The filling derive under stops (O314, design §7 "Cancel" and "A restart
// mid-job") ------------------------------------------------------------------

/// Every work's series, in order.
fn work_series(conn: &Connection) -> Vec<String> {
    conn.prepare(
        "SELECT s.sort_title FROM work w JOIN series s ON s.id = w.series_id
          ORDER BY s.sort_title",
    )
    .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
    .expect("reads the works")
}

/// The member paths of `series`'s works, in page order.
fn members_of(conn: &Connection, series: &str) -> Vec<String> {
    conn.prepare(
        "SELECT m.path FROM member m JOIN edition e ON e.id = m.edition_id
           JOIN work w ON w.id = e.work_id JOIN series s ON s.id = w.series_id
          WHERE s.sort_title = ?1 ORDER BY w.id, e.key, m.ordinal",
    )
    .and_then(|mut stmt| stmt.query_map([series], |r| r.get(0))?.collect())
    .expect("reads the members")
}

const XENO_C9: &str = "Xeno/Xeno - v01 - c9.cbz";
const XENO_C10: &str = "Xeno/Xeno - v01 - c10.cbz";

/// A reader's place or note, as `(table, work id, edition, slot)`, beside the
/// page its slot names.
type Saved = ((&'static str, i64, String, i64), Option<(String, i64)>);

/// The page `slot` names in `work_id`'s `edition`, as (member path, page
/// within it), or none past its end. Slot n is the edition's page n where no
/// spread flag stands, which is every edition a derive writes.
fn page_named(conn: &Connection, work_id: i64, edition: &str, slot: i64) -> Option<(String, i64)> {
    let members: Vec<(String, i64)> = conn
        .prepare(
            "SELECT m.path, m.pages FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE e.work_id = ?1 AND e.key = ?2 ORDER BY m.ordinal",
        )
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![work_id, edition], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect()
        })
        .expect("reads the members");
    let mut page = slot;
    for (path, pages) in members {
        if page < pages {
            return Some((path, page));
        }
        page -= pages;
    }
    None
}

/// Every place and note in the store, each beside the page its slot names, in
/// order.
fn saved(conn: &Connection) -> Vec<Saved> {
    let mut rows = Vec::new();
    for table in ["progress", "note"] {
        let read: Vec<(i64, String, i64)> = conn
            .prepare(&format!("SELECT work_id, edition, slot FROM {table}"))
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect()
            })
            .expect("reads the rows");
        rows.extend(read.into_iter().map(|(work_id, edition, slot)| {
            let page = page_named(conn, work_id, &edition, slot);
            ((table, work_id, edition, slot), page)
        }));
    }
    rows.sort();
    rows
}

/// A place and a note at slot 0, on the hub, by the schema's own user, on
/// every work holding no place; answers what each saved row names.
fn read_every_new_work(conn: &Connection) -> Vec<Saved> {
    let bare: Vec<(i64, String)> = conn
        .prepare("SELECT id, hub FROM work WHERE id NOT IN (SELECT work_id FROM progress)")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the works");
    for (work_id, hub) in &bare {
        put_progress(
            conn,
            *work_id,
            1,
            &Progress {
                edition: hub.clone(),
                slot: 0,
            },
        )
        .expect("a reader's place");
        put_notes(
            conn,
            *work_id,
            1,
            &Notes {
                taiyaku_notes: NOTES_VERSION,
                work: "ignored".to_string(),
                notes: vec![Note {
                    text: None,
                    id: "01JEXAMPLE0000000000000000".to_string(),
                    edition: hub.clone(),
                    slot: 0,
                    region: None,
                    color: None,
                    body: "the first page".to_string(),
                    created: "2026-11-22T09:00:00Z".to_string(),
                    updated: "2026-11-22T09:00:00Z".to_string(),
                    stale: false,
                    at: None,
                }],
            },
        )
        .expect("a reader's note");
    }
    saved(conn)
        .into_iter()
        .filter(|((_, work_id, _, _), _)| bare.iter().any(|(id, _)| id == work_id))
        .collect()
}

/// Design §8 test 4, the second new anti-goal: a place and a note saved on a
/// volume a pass wrote keep their work, edition and slot through the full
/// derive, and the slot names the page it named. Xeno's `c10` sorts before
/// `c9` by path and after it by page, and is hashed before the job, so a
/// Xeno written while `c9` waits would begin at `c10`'s first page.
#[test]
fn a_place_saved_on_a_volume_a_pass_wrote_names_the_same_page_after_the_job() {
    let tree = crate::tests::tempdir("fill-place");
    for (rel_path, pages) in [(ALPHA, 2), (XENO_C10, 3), (XENO_C9, 2)] {
        write_archive(&tree.path().join(rel_path), pages);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    hash_now(&conn, &root, XENO_C10);
    let at_save: RefCell<Vec<Saved>> = RefCell::default();
    let probe = Probe::new(&conn).on_written(|conn| {
        at_save.borrow_mut().extend(read_every_new_work(conn));
    });

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    let at_save = at_save.take();
    assert!(
        !at_save.is_empty(),
        "the fixture: a pass wrote a volume and a reader saved on it"
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM page_flag"),
        0,
        "the fixture: no spread flag, so a slot is a page"
    );
    let path = |rel_path: &str| tree.path().join(rel_path).to_string_lossy().into_owned();
    assert_eq!(
        members_of(&conn, "Xeno"),
        [path(XENO_C9), path(XENO_C10)],
        "the fixture: Xeno's volume begins at c9's pages"
    );
    assert_eq!(
        saved(&conn),
        at_save,
        "each place and note keeps its work, edition and slot, on the page it was saved on"
    );
}

/// A [`Watch`] that panics at the first report `stop` holds for, told whether a
/// pass has written yet: the process stopping there, so nothing of the job
/// after it runs.
struct Stopping<F> {
    stop: F,
    written: Cell<bool>,
}

impl<F: Fn(Report, bool) -> bool> Stopping<F> {
    fn new(stop: F) -> Self {
        Self {
            stop,
            written: Cell::new(false),
        }
    }
}

impl<F: Fn(Report, bool) -> bool> Watch for Stopping<F> {
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        assert!(
            !(self.stop)((step, done, of), self.written.get()),
            "the process stops at {step:?}, {done} of {of:?}"
        );
    }

    fn cancelled(&self) -> bool {
        false
    }

    fn written(&self, _works: u64) {
        self.written.set(true);
    }
}

/// The filling derive over every directory under `watch`, at a pass after
/// every group, which must stop it before its end.
fn stopped(conn: &Connection, watch: &dyn Watch) {
    let ended = catch_unwind(AssertUnwindSafe(|| {
        derive::derive_filling_at(
            conn,
            derive::Documents::Reconcile,
            &[],
            watch,
            &every_group(),
        )
    }));
    assert!(
        ended.is_err(),
        "the fixture: the job stops before its end: {ended:?}"
    );
}

const BETA_C001: &str = "Beta/Beta - v01 - c001.cbz";
const BETA_C002: &str = "Beta/Beta - v01 - c002.cbz";

/// Design §8 test 8, §7 "Cancel": a cancel after a pass, while the next
/// volume's files are hashed, leaves what a stop at that report leaves: the
/// volume the pass wrote and every digest made. Then, with `R` paused, a
/// derive of `Q` keeps that volume as it stands and writes no volume of `R`
/// hashed in part. The stop runs in a second store over the same directories,
/// since `standing` compares absolute paths and path-labelled digests.
#[test]
fn a_cancel_after_a_pass_keeps_its_volumes_as_a_stop_at_that_point_would() {
    let r = crate::tests::tempdir("fill-stops-r");
    let q = crate::tests::tempdir("fill-stops-q");
    for (tree, rel_path, pages) in [
        (&r, ALPHA, 2),
        (&r, BETA_C001, 3),
        (&r, BETA_C002, 2),
        (&q, GAMMA_CBZ, 2),
    ] {
        write_archive(&tree.path().join(rel_path), pages);
    }
    let stop_at: u64 = [ALPHA, BETA_C001]
        .iter()
        .map(|file| {
            std::fs::metadata(r.path().join(file))
                .expect("the file")
                .len()
        })
        .sum();
    let at_stop = move |(step, done, _): Report| step == Step::Hashing && done >= stop_at;
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let r_root = scanned_root(&conn, r.path(), None);
    let q_root = scanned_root(&conn, q.path(), None);
    let probe = Probe::new(&conn).cancelling(|_, reports| reports.iter().copied().any(at_stop));

    let outcome = fill(&conn, &probe, Duration::ZERO);

    assert_eq!(
        outcome,
        Outcome::Cancelled(Step::Hashing),
        "the job stops in the step it shows"
    );
    assert_eq!(
        work_series(&conn),
        ["Alpha"],
        "the pass after Alpha's file wrote its volume, and nothing since"
    );
    assert_eq!(
        digests_held(&conn),
        BTreeSet::from([
            (r_root.id, ALPHA.to_string()),
            (r_root.id, BETA_C001.to_string())
        ]),
        "every digest made: Alpha's file's and Beta's first"
    );
    let stop = Library::open_in_memory().expect("open");
    let stop_conn = stop.conn();
    scanned_root(&stop_conn, r.path(), None);
    scanned_root(&stop_conn, q.path(), None);
    stopped(&stop_conn, &Stopping::new(|report, _| at_stop(report)));
    assert_eq!(
        standing(&stop_conn),
        standing(&conn),
        "the works a stop at that report leaves"
    );
    assert_eq!(
        digests_held(&stop_conn),
        digests_held(&conn),
        "and its digests"
    );

    let cancelled = shot(&conn).works;
    pause(&conn, &r_root);
    scan::scan_root(&conn, &q_root).expect("walks Q");
    derive::done(
        derive::derive_watched(
            &conn,
            derive::Documents::Reconcile,
            &[q_root.id],
            &Recording::default(),
        )
        .expect("derives"),
    )
    .expect("the derive ends");

    let after = shot(&conn).works;
    for (id, row) in &cancelled {
        assert_eq!(
            after.get(id),
            Some(row),
            "work {id}, Alpha's, as the pass wrote it"
        );
        let missing_since: Option<String> = conn
            .query_row("SELECT missing_since FROM work WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .expect("the work");
        assert_eq!(
            missing_since, None,
            "work {id}, Alpha's, still shown to a reader: not marked missing"
        );
    }
    assert_eq!(
        work_series(&conn),
        ["Alpha", "Gamma"],
        "Beta's volume, hashed in part, waits for R's Scan; Gamma's is written"
    );
}

/// Design §7, "Cancel": the flag is read between a pass's works, so a cancel
/// while a pass writes stops it after the work it is on, and the job answers
/// the step it shows. Beta's file is hashed before the job, so the pass after
/// Alpha's group, not the last, writes Alpha's volume and then Beta's; the
/// cancel comes once Alpha's is written.
#[test]
fn a_cancel_between_a_passs_works_stops_after_the_one_it_is_on() {
    let (_tree, library) = scanned_files("fill-cancel-between", &SERIES[..3]);
    let conn = library.conn();
    let root = scan::roots(&conn).expect("lists").remove(0);
    hash_now(&conn, &root, SERIES[1]);
    let probe =
        Probe::new(&conn).cancelling(|conn, _| scalar(conn, "SELECT count(*) FROM work") >= 1);

    let outcome = fill(&conn, &probe, Duration::ZERO);

    assert_eq!(
        work_series(&conn),
        ["Alpha"],
        "the pass stops before Beta's volume"
    );
    assert_eq!(
        outcome,
        Outcome::Cancelled(Step::Hashing),
        "the job stops in the step it shows"
    );
}

/// Design §7, "Cancel": the full derive the job ends with reads the cancel
/// flag through the watch that drops its Hashing reports, so a cancel once it
/// reports Writing stops it before its next work, in that step. A watch that
/// dropped the flag too would run the derive to its end.
#[test]
fn a_cancel_while_the_full_derive_writes_stops_it() {
    let (_tree, library) = scanned_files("fill-cancel-full", &SERIES[..2]);
    let conn = library.conn();
    let probe = Probe::new(&conn)
        .cancelling(|_, reports| reports.iter().any(|(step, _, _)| *step == Step::Writing));

    let outcome = fill(&conn, &probe, Duration::ZERO);

    assert_eq!(
        outcome,
        Outcome::Cancelled(Step::Writing),
        "the full derive stops in the step it shows"
    );
    assert_eq!(
        work_series(&conn),
        ["Alpha"],
        "the pass after Alpha's file wrote its volume; Beta's waits"
    );
}

/// Four one-file series under `label`, each page labelled by its file's path
/// within the directory, so two copies hash alike.
fn four_series(label: &str) -> TempDir {
    let tree = crate::tests::tempdir(label);
    for file in &SERIES[..4] {
        write_archive_labelled(&tree.path().join(file), 2, "jpg", file);
    }
    tree
}

/// Every work's id, by slug.
fn ids(conn: &Connection) -> BTreeMap<String, i64> {
    conn.prepare("SELECT slug, id FROM work")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the works")
}

/// Design §8 test 9, §7 "A restart mid-job": a job stopped after a pass, its
/// directory walked again and the job run to its end, leaves the store a job
/// that ran through leaves, and the volume the pass wrote keeps its id. A
/// restart has no code of its own: by then that volume is a work, which the
/// passes leave and the full derive finds unchanged, so a reader's link to it
/// and place in it hold.
#[test]
fn a_job_stopped_after_a_pass_and_run_again_ends_as_one_that_ran_through() {
    let tree = four_series("fill-restart");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scanned_root(&conn, tree.path(), None);
    stopped(
        &conn,
        &Stopping::new(|(step, _, _), written| step == Step::Hashing && written),
    );
    let early = ids(&conn);
    assert_eq!(
        early.len(),
        1,
        "the fixture: the job stopped after the pass that wrote Alpha's volume: {early:?}"
    );
    scan::scan_root(&conn, &root).expect("walks again");
    derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &Recording::default(),
            &every_group(),
        )
        .expect("derives"),
    )
    .expect("the job ends");
    let through_tree = four_series("fill-restart-through");
    let through = Library::open_in_memory().expect("open");
    let through_conn = through.conn();
    scanned_root(&through_conn, through_tree.path(), None);
    derive::done(
        derive::derive_filling_at(
            &through_conn,
            derive::Documents::Reconcile,
            &[],
            &Recording::default(),
            &every_group(),
        )
        .expect("derives"),
    )
    .expect("the job ends");

    let restarted = ids(&conn);
    assert_eq!(
        restarted,
        ids(&through_conn),
        "every work has the slug and id a job that ran through gives it"
    );
    assert!(
        early
            .iter()
            .all(|(slug, id)| restarted.get(slug) == Some(id)),
        "Alpha's volume keeps its id: {early:?} then {restarted:?}"
    );
    assert_eq!(
        settled(&conn),
        settled(&through_conn),
        "and the works, editions, members and member_root rows a job that ran through leaves"
    );
}

/// A file gone between the walk and its digest, as a share
/// dropping or a deletion leaves it, holds back only its own volume. The queue
/// leaves it unhashed, so every pass finds it still to hash and leaves its
/// volume, and passes go on writing the volumes after it; the full derive
/// writes that volume without it and counts it vanished, as one derive does.
/// Gamma's volume is not the queue's last, so a pass can write it.
#[test]
fn a_file_gone_before_its_hash_holds_back_only_its_own_volume() {
    let (tree, library) =
        scanned_files("fill-gone", &[ALPHA, BETA_C001, BETA_C002, GAMMA_CBZ, ZETA]);
    let gone = tree.path().join(BETA_C001);
    std::fs::remove_file(&gone).expect("Beta's first file goes");
    let conn = library.conn();
    let recording = Recording::default();

    let report = derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &recording,
            &every_group(),
        )
        .expect("the job derives"),
    )
    .expect("the job ends");

    assert_eq!(
        recording.written_so_far(),
        [1, 2],
        "a pass wrote Alpha's volume and one Gamma's, none Beta's, whose first file has no \
         digest; Zeta's is the last"
    );
    assert_eq!(
        work_series(&conn),
        ["Alpha", "Beta", "Gamma", "Zeta"],
        "every volume at the job's end"
    );
    assert_eq!(
        members_of(&conn, "Beta"),
        [tree.path().join(BETA_C002).to_string_lossy().into_owned()],
        "Beta's volume of its second file alone"
    );
    assert_eq!(
        report.unreadable.vanished,
        [gone.display().to_string()],
        "the gone file counted vanished"
    );
}

/// O452: the queue's digest is the one open of a file it finds gone; the
/// full derive counts it vanished without opening it again.
#[test]
fn a_file_the_queue_finds_gone_is_opened_once() {
    let (tree, library) = scanned_files(
        "fill-gone-once",
        &[ALPHA, BETA_C001, BETA_C002, GAMMA_CBZ, ZETA],
    );
    std::fs::remove_file(tree.path().join(BETA_C001)).expect("Beta's first file goes");
    let conn = library.conn();
    crate::derive::DIGEST_OPENS.with_borrow_mut(Vec::clear);

    let report = derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &Recording::default(),
            &every_group(),
        )
        .expect("the job derives"),
    )
    .expect("the job ends");

    assert_eq!(
        crate::derive::tests::recorded(&crate::derive::DIGEST_OPENS, BETA_C001),
        1,
        "the queue's digest opens the gone file, and nothing after it"
    );
    assert_eq!(
        report.unreadable.vanished,
        [tree.path().join(BETA_C001).display().to_string()],
        "the gone file counted vanished, once"
    );
}

/// O452 and O320: a standing work's sole file, changed and then gone before
/// the queue's digest, is opened once in the job, and its held reading
/// stands for the next scan.
#[test]
fn a_changed_file_the_queue_finds_gone_is_opened_once_and_keeps_its_held_reading() {
    let (tree, library) = scanned_files(
        "fill-gone-held",
        &[ALPHA, BETA_C001, BETA_C002, GAMMA_CBZ, ZETA],
    );
    let conn = library.conn();
    derive::derive(&conn).expect("the first derive");
    write_archive(&tree.path().join(GAMMA_CBZ), 5);
    let root = scan::roots(&conn).expect("reads the directories").remove(0);
    scan::scan_root(&conn, &root).expect("walks the directory again");
    let held = scan::tests::held_reading(&conn, root.id, GAMMA_CBZ);
    assert!(
        held.0.is_some(),
        "the fixture: the walk holds Gamma's old reading"
    );
    std::fs::remove_file(tree.path().join(GAMMA_CBZ)).expect("Gamma's file goes");
    crate::derive::DIGEST_OPENS.with_borrow_mut(Vec::clear);

    let report = derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &Recording::default(),
            &every_group(),
        )
        .expect("the job derives"),
    )
    .expect("the job ends");

    assert_eq!(
        crate::derive::tests::recorded(&crate::derive::DIGEST_OPENS, GAMMA_CBZ),
        1,
        "the queue's digest opens the gone file, and neither classify nor the held members' \
         digest after it"
    );
    assert_eq!(
        report.unreadable.vanished,
        [tree.path().join(GAMMA_CBZ).display().to_string()],
        "the gone file counted vanished, once"
    );
    assert_eq!(
        scan::tests::held_reading(&conn, root.id, GAMMA_CBZ),
        held,
        "its held reading stands"
    );
}

/// O452: a folder the queue finds gone is listed once, by the queue's sizing,
/// and opened once, by the queue's digest.
#[test]
fn a_folder_the_queue_finds_gone_is_listed_once_and_opened_once() {
    let tree = crate::tests::tempdir("fill-gone-folder");
    write_archive(&tree.path().join(ALPHA), 2);
    write_pages(&tree.path().join(GAMMA), 2);
    write_archive(&tree.path().join(ZETA), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    scanned_root(&conn, tree.path(), None);
    std::fs::remove_dir_all(tree.path().join(GAMMA)).expect("Gamma's folder goes");
    crate::derive::DIGEST_OPENS.with_borrow_mut(Vec::clear);
    crate::derive::FOLDER_SIZINGS.with_borrow_mut(Vec::clear);

    let report = derive::done(
        derive::derive_filling_at(
            &conn,
            derive::Documents::Reconcile,
            &[],
            &Recording::default(),
            &every_group(),
        )
        .expect("the job derives"),
    )
    .expect("the job ends");

    assert_eq!(
        crate::derive::tests::recorded(&crate::derive::FOLDER_SIZINGS, GAMMA),
        1,
        "the queue's sizing lists the gone folder, and the full derive's does not"
    );
    assert_eq!(
        crate::derive::tests::recorded(&crate::derive::DIGEST_OPENS, GAMMA),
        1,
        "the queue's digest opens the gone folder, and nothing after it"
    );
    assert_eq!(
        report.unreadable.vanished,
        [tree.path().join(GAMMA).display().to_string()],
        "the gone folder counted vanished, once"
    );
}

/// Design §7, "The paused freeze": every pass and the full derive read the
/// frozen set the job began with. A pressed Scan unpauses its directory before
/// it waits for the running job, here once a pass has written Alpha's volume
/// from A's file alone. Read again at the full derive, P would be hashed with
/// no Hashing report to count it, and its file would join a volume a reader
/// has already seen whole. With no unpause the job must end the same.
#[test]
fn an_unpause_while_the_job_runs_leaves_the_directory_to_its_own_scan() {
    for unpaused in [false, true] {
        let p = crate::tests::tempdir(&format!("fill-unpause-{unpaused}-p"));
        let a = crate::tests::tempdir(&format!("fill-unpause-{unpaused}-a"));
        write_archive(&p.path().join(ALPHA), 2);
        write_archive(&a.path().join(ALPHA), 3);
        write_archive(&a.path().join(BETA), 2);
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let p_root = scanned_root(&conn, p.path(), Some("ja"));
        pause(&conn, &p_root);
        scanned_root(&conn, a.path(), Some("en"));
        let pressed = p_root.clone();
        let probe = Probe::new(&conn).on_written(move |conn| {
            if unpaused {
                scan::unpause(conn, std::slice::from_ref(&pressed)).expect("unpauses P");
            }
        });

        derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

        assert_eq!(
            probe.recording.written_so_far(),
            [1],
            "the fixture: a pass writes Alpha's volume (unpaused: {unpaused})"
        );
        let last = shot(&conn);
        for (id, row) in &probe.shots()[0].works {
            assert_eq!(
                Some(row),
                last.works.get(id),
                "work {id} at the job's end, as the pass wrote it (unpaused: {unpaused})"
            );
        }
        assert_eq!(
            hashed_under(&conn, &p_root),
            Vec::<String>::new(),
            "P's file waits for P's own scan, hashed by no step the job's line leaves \
             uncounted (unpaused: {unpaused})"
        );
    }
}

/// Design §7, "The paused freeze", the other way: a waiting job's cancel
/// pauses its directories at once, here once a pass has written Alpha's
/// volume. The job goes on reading P as it began, as one derive begun before
/// the pause would, so the pass after Beta's digests and the full derive both
/// read P's files, which the queue hashed. Read frozen, such a file is held by
/// no work and waits, and its volume is written without P's edition.
#[test]
fn a_pause_while_the_job_runs_leaves_the_job_reading_the_directory() {
    let p = crate::tests::tempdir("fill-pause-p");
    let a = crate::tests::tempdir("fill-pause-a");
    for tree in [&p, &a] {
        for rel_path in [ALPHA, BETA, GAMMA_CBZ] {
            // Distinct bytes: `write_archive` labels each page by its path.
            write_archive(&tree.path().join(rel_path), 2);
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let p_root = scanned_root(&conn, p.path(), Some("ja"));
    scanned_root(&conn, a.path(), Some("en"));
    let waited = p_root.clone();
    let probe = Probe::new(&conn).on_written(move |conn| pause(conn, &waited));

    derive::done(fill(&conn, &probe, Duration::ZERO)).expect("the job ends");

    assert_eq!(
        probe.recording.written_so_far(),
        [1, 2],
        "the fixture: a pass writes Alpha's volume before the pause, and one Beta's after it"
    );
    assert_eq!(
        scan::paused(&conn)
            .expect("reads the pauses")
            .into_iter()
            .collect::<Vec<_>>(),
        [p_root.id],
        "the fixture: P is paused at the job's end"
    );
    let last = shot(&conn);
    for (pass, shot) in probe.shots().iter().enumerate() {
        for (id, row) in &shot.works {
            assert_eq!(
                Some(row),
                last.works.get(id),
                "work {id} at the job's end, as pass {pass} wrote it"
            );
        }
    }
    let editions: Vec<(Option<String>, Vec<String>)> = last
        .works
        .into_values()
        .map(|row| (row.series, row.editions.into_keys().collect()))
        .collect();
    assert_eq!(
        editions,
        ["Alpha", "Beta", "Gamma"].map(|series| (
            Some(series.to_string()),
            vec!["en".to_string(), "ja".to_string()]
        )),
        "every volume holds both directories' editions"
    );
}

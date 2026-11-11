use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::*;
use crate::cover;
use crate::test_support::{Fixture, scanned_file_backed_library, write_pages};

/// A digest no member of the scanned store holds. Lowercase hex, so only its
/// absence from `member` sets it apart from the held one.
const UNHELD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The ruled period between sweeps, written out so that a changed [`EVERY`]
/// fails the tests that wait on it.
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// A scanned store, its one member's digest, and the two cache directories,
/// made beside the scanned tree and never inside it.
struct Store {
    fixture: Fixture,
    library: Arc<taiyaku_library::Library>,
    held: String,
    covers: PathBuf,
    thumbs: PathBuf,
}

fn store(label: &str) -> Store {
    let fixture = Fixture::new(label);
    let library = scanned_file_backed_library(&fixture);
    let held: String = library
        .conn()
        .query_row("SELECT sha256 FROM member", [], |row| row.get(0))
        .expect("reads the one member's digest");
    assert_ne!(held, UNHELD);
    let covers = fixture.path().join("covers");
    let thumbs = fixture.path().join("thumbs");
    for dir in [&covers, &thumbs] {
        std::fs::create_dir_all(dir).expect("makes a cache directory");
    }
    Store {
        fixture,
        library,
        held,
        covers,
        thumbs,
    }
}

impl Store {
    fn sweep(&self, started: SystemTime) -> Result<Swept, String> {
        self.sweep_after(started, || {})
    }

    fn sweep_after(
        &self,
        started: SystemTime,
        after_listing: impl FnOnce(),
    ) -> Result<Swept, String> {
        sweep_with(&self.dirs(), &self.library, 0, started, after_listing)
    }

    fn dirs(&self) -> [PathBuf; 2] {
        [self.covers.clone(), self.thumbs.clone()]
    }

    /// Sweeps `dirs` under `cap` on another thread, so an eviction that never
    /// ends fails within five seconds instead of hanging the run.
    fn sweep_capped_over(
        &self,
        dirs: [PathBuf; 2],
        cap: u64,
        started: SystemTime,
        after_listing: impl FnOnce() + Send + 'static,
    ) -> Swept {
        let library = Arc::clone(&self.library);
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sent.send(sweep_with(&dirs, &library, cap, started, after_listing));
        });
        received
            .recv_timeout(Duration::from_secs(5))
            .expect("the sweep ends within five seconds")
            .expect("sweeps")
    }

    fn sweep_capped(&self, cap: u64, started: SystemTime) -> Swept {
        self.sweep_capped_over(self.dirs(), cap, started, || {})
    }

    /// A cover, a tile and a reading page of `digest`.
    fn rendered(&self, digest: &str) -> [PathBuf; 3] {
        [
            cover::cache_path(&self.covers, digest),
            cover::thumb_cache_path(&self.thumbs, digest, 1, cover::THUMB_LONG_EDGE),
            cover::page_cache_path(&self.thumbs, digest, 1, 1536),
        ]
    }
}

/// Five minutes ago: the process "started" before anything a test plants.
fn started() -> SystemTime {
    SystemTime::now() - Duration::from_secs(300)
}

fn plant(path: &Path, bytes: usize) {
    std::fs::write(path, vec![b'x'; bytes]).expect("plants a cache file");
}

fn set_modified(path: &Path, modified: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("sets a file's mtime");
}

/// Plants `path` with `bytes` bytes, last used `hours` hours before `started`.
fn plant_used(path: &Path, bytes: usize, started: SystemTime, hours: u64) {
    plant(path, bytes);
    set_modified(path, started - Duration::from_secs(3600 * hours));
}

fn tally(files: u64, bytes: u64) -> Tally {
    Tally { files, bytes }
}

fn name(path: &Path) -> &str {
    path.file_name()
        .and_then(OsStr::to_str)
        .expect("a minted name is UTF-8")
}

#[test]
fn every_name_the_cache_mints_parses_with_its_digest() {
    let dir = Path::new("/cache/taiyaku/thumbs");
    let mut minted = vec![cover::cache_path(dir, UNHELD)];
    minted.extend(cover::TILE_LONG_EDGES.map(|edge| cover::thumb_cache_path(dir, UNHELD, 7, edge)));
    minted.push(cover::page_cache_path(dir, UNHELD, 7, 1536));
    for path in &minted {
        assert_eq!(
            parse(name(path)),
            Some(Parsed::Key { digest: UNHELD }),
            "{}",
            path.display()
        );
        let staging = taiyaku_read::atomic::temp_path(path, "jpg");
        assert_eq!(
            parse(name(&staging)),
            Some(Parsed::Staging),
            "{}",
            staging.display()
        );
    }
}

#[test]
fn an_unheld_digest_s_renderings_go_and_a_held_one_s_stay() {
    let store = store("sweep-orphans");
    let held = store.rendered(&store.held);
    let unheld = store.rendered(UNHELD);
    for (path, bytes) in held.iter().zip([10, 20, 40]) {
        plant(path, bytes);
    }
    for (path, bytes) in unheld.iter().zip([100, 200, 400]) {
        plant(path, bytes);
    }

    let swept = store.sweep(started()).expect("sweeps");

    assert_eq!(
        swept,
        Swept {
            orphans: tally(3, 700),
            left: tally(3, 70),
            ..Swept::default()
        }
    );
    for path in &held {
        assert!(path.exists(), "{} stays", path.display());
    }
    for path in &unheld {
        assert!(!path.exists(), "{} goes", path.display());
    }
}

/// `scan_root` forgets a volume whose directory has gone, and only a derive
/// would drop its `member` row: a missing work's renderings must stay.
#[test]
fn a_member_whose_file_has_gone_keeps_its_renderings() {
    let store = store("sweep-gone");
    let tree = store.fixture.path().join("tree");
    // A root that scans empty is refused, so a second volume keeps it from
    // being one.
    write_pages(&tree.join("Other").join("Other - v01"), 3);
    std::fs::remove_dir_all(tree.join("Scanned").join("Scanned - v01"))
        .expect("removes the volume");
    {
        let conn = store.library.conn();
        let root = taiyaku_library::scan::roots(&conn)
            .expect("lists the roots")
            .remove(0);
        let report = taiyaku_library::scan::scan_root(&conn, &root).expect("rescans");
        assert_eq!(report.refused, None);
        let count = |table: &str| -> i64 {
            conn.query_row(
                &format!("SELECT count(*) FROM {table} WHERE sha256 = ?1"),
                [&store.held],
                |row| row.get(0),
            )
            .expect("counts the rows holding the digest")
        };
        assert_eq!((count("scan_entry"), count("member")), (0, 1));
    }
    let rendered = store.rendered(&store.held);
    for (path, bytes) in rendered.iter().zip([10, 20, 40]) {
        plant(path, bytes);
    }

    let swept = store.sweep(started()).expect("sweeps");

    assert_eq!(
        swept,
        Swept {
            left: tally(3, 70),
            ..Swept::default()
        }
    );
    for path in &rendered {
        assert!(path.exists(), "{} stays", path.display());
    }
}

/// Both files carry this process's pid, so only their age tells the
/// leftover from a write in progress, as in a container where the server is
/// PID 1 on every start. The key is held, so only the age can send it.
#[test]
fn a_staging_file_older_than_the_process_goes_and_a_newer_one_stays() {
    let store = store("sweep-leftovers");
    let started = started();
    let key = cover::cache_path(&store.covers, &store.held);
    let old = taiyaku_read::atomic::temp_path(&key, "jpg");
    let young = taiyaku_read::atomic::temp_path(&key, "jpg");
    let pid = format!(".tmp.{}.", std::process::id());
    assert!(name(&old).contains(&pid) && name(&young).contains(&pid));
    plant(&old, 30);
    set_modified(&old, started - Duration::from_secs(3600));
    plant(&young, 50);

    let swept = store.sweep(started).expect("sweeps");

    assert_eq!(
        swept,
        Swept {
            leftovers: tally(1, 30),
            left: tally(1, 50),
            ..Swept::default()
        }
    );
    assert!(!old.exists(), "the leftover goes");
    assert!(young.exists(), "the write in progress stays");
}

/// Each is an hour older than the process, so one taken for a leftover
/// would go too. The unheld ones break a key in one place each, a letter past
/// `f` or a field that is empty or not all digits, so a parser that lets any
/// one rule slip turns that name into an orphan or a leftover.
#[test]
fn a_name_that_does_not_parse_stays_and_is_counted() {
    let store = store("sweep-unrecognised");
    let started = started();
    let names = [
        "notes.txt".to_owned(),
        "cover-512v1.jpg".to_owned(),
        format!("{}-512v1.jpg", &UNHELD[..63]),
        format!("{}-512v1.jpg", UNHELD.to_uppercase()),
        format!("{}-512.jpg", store.held),
        format!("{}-512v1.png", store.held),
        "cover-512v1.jpg.tmp.1.0".to_owned(),
        format!("{}g-512v1.jpg", &UNHELD[..63]),
        format!("{UNHELD}-x512v1.jpg"),
        format!("{UNHELD}-px-160v1.jpg"),
        format!("{UNHELD}-512vx.jpg"),
        format!("{UNHELD}-512v.jpg"),
        format!("{UNHELD}-512v1.jpg.tmp.x.0"),
        format!("{UNHELD}-512v1.jpg.tmp.1.x"),
    ];
    for (name, bytes) in names.iter().zip(1..) {
        let path = store.covers.join(name);
        plant(&path, bytes);
        set_modified(&path, started - Duration::from_secs(3600));
    }

    let swept = store.sweep(started).expect("sweeps");

    assert_eq!(
        swept,
        Swept {
            left: tally(14, 105),
            unrecognised: 14,
            ..Swept::default()
        }
    );
    for name in &names {
        assert!(store.covers.join(name).exists(), "{name} stays");
    }
}

#[test]
fn an_entry_that_is_not_a_regular_file_is_left_and_not_counted() {
    let store = store("sweep-not-files");
    let held = cover::cache_path(&store.covers, &store.held);
    plant(&held, 10);
    let dir = cover::thumb_cache_path(&store.thumbs, UNHELD, 0, cover::THUMB_LONG_EDGE);
    std::fs::create_dir(&dir).expect("makes a directory named as a key");
    let link = cover::cache_path(&store.thumbs, UNHELD);
    std::os::unix::fs::symlink(&held, &link).expect("links a key's name to a file");

    let swept = store.sweep(started()).expect("sweeps");

    assert_eq!(
        swept,
        Swept {
            left: tally(1, 10),
            ..Swept::default()
        }
    );
    assert!(dir.is_dir(), "the directory stays");
    assert!(link.symlink_metadata().is_ok(), "the link stays");
    assert!(held.exists(), "its target stays");
}

#[test]
fn a_file_written_after_the_listing_stays_though_its_digest_is_new() {
    const ADDED: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let store = store("sweep-listing-first");
    let written = cover::thumb_cache_path(&store.thumbs, ADDED, 0, cover::THUMB_LONG_EDGE);

    let swept = store
        .sweep_after(started(), || {
            plant(&written, 10);
            store
                .library
                .conn()
                .execute("UPDATE member SET sha256 = ?1", [ADDED])
                .expect("the member now holds the new digest");
        })
        .expect("sweeps");

    assert!(written.exists(), "the file written after the listing stays");
    assert_eq!(swept, Swept::default());
}

#[test]
fn a_cache_directory_not_made_yet_is_an_empty_one() {
    let store = store("sweep-no-thumbs");
    std::fs::remove_dir(&store.thumbs).expect("removes the empty thumbs");
    let orphan = cover::cache_path(&store.covers, UNHELD);
    plant(&orphan, 10);

    let swept = store
        .sweep(started())
        .expect("a directory not made yet is no failure");

    assert_eq!(
        swept,
        Swept {
            orphans: tally(1, 10),
            ..Swept::default()
        }
    );
    assert!(!orphan.exists(), "the orphan in covers still goes");
}

#[test]
fn a_listing_that_fails_deletes_nothing() {
    let store = store("sweep-unlistable");
    std::fs::remove_dir(&store.covers).expect("removes the empty covers");
    plant(&store.covers, 1);
    let orphan = cover::thumb_cache_path(&store.thumbs, UNHELD, 0, cover::THUMB_LONG_EDGE);
    plant(&orphan, 10);

    let error = store
        .sweep(started())
        .expect_err("a directory that will not list is a failure");

    assert_eq!(
        error,
        format!(
            "could not list {}: Not a directory (os error 20)",
            store.covers.display()
        )
    );
    assert!(orphan.exists(), "nothing is deleted");
}

#[test]
fn a_digest_read_that_fails_deletes_nothing() {
    let store = store("sweep-no-member");
    let orphan = cover::cache_path(&store.covers, UNHELD);
    plant(&orphan, 10);
    store
        .library
        .conn()
        .execute_batch("ALTER TABLE member RENAME TO member_gone")
        .expect("takes the member table away");

    let error = store
        .sweep(started())
        .expect_err("a failed read is not an empty store");

    assert_eq!(
        error,
        "could not read the held digests: no such table: member"
    );
    assert!(orphan.exists(), "nothing is deleted");
}

/// One file is gone before its delete, and a directory takes the other's
/// name, which no `remove_file` can delete, as root or not.
#[test]
fn a_delete_that_fails_leaves_its_file_counted_and_one_already_gone_counts_as_done() {
    let store = store("sweep-undeletable");
    let gone = cover::cache_path(&store.covers, UNHELD);
    let stuck = cover::thumb_cache_path(&store.thumbs, UNHELD, 0, cover::THUMB_LONG_EDGE);
    plant(&gone, 10);
    plant(&stuck, 20);

    let swept = store
        .sweep_after(started(), || {
            std::fs::remove_file(&gone).expect("deletes the first");
            std::fs::remove_file(&stuck).expect("deletes the second");
            std::fs::create_dir(&stuck).expect("puts a directory in its place");
        })
        .expect("a failed delete does not fail the sweep");

    assert_eq!(
        swept,
        Swept {
            orphans: tally(1, 10),
            left: tally(1, 20),
            undeleted: 1,
            first_failure: Some(format!("{}: Is a directory (os error 21)", stuck.display())),
            ..Swept::default()
        }
    );
}

/// The two oldest are the largest and bring the total exactly to the cap,
/// across both directories, so evicting the newest or the smallest first, or
/// one file more, shows.
#[test]
fn the_cap_evicts_the_least_recently_used_down_to_the_cap_and_no_further() {
    let store = store("sweep-cap");
    let started = started();
    let held = &store.held;
    let oldest_first = [
        cover::cache_path(&store.covers, held),
        cover::thumb_cache_path(&store.thumbs, held, 1, cover::THUMB_LONG_EDGE),
        cover::page_cache_path(&store.thumbs, held, 1, 1536),
        cover::thumb_cache_path(&store.thumbs, held, 2, cover::THUMB_LONG_EDGE),
    ];
    for (path, (bytes, hours)) in oldest_first
        .iter()
        .zip([(400, 4), (300, 3), (200, 2), (100, 1)])
    {
        plant_used(path, bytes, started, hours);
    }

    let swept = store.sweep_capped(300, started);

    assert_eq!(
        swept,
        Swept {
            evicted: tally(2, 700),
            left: tally(2, 300),
            ..Swept::default()
        }
    );
    for path in &oldest_first[..2] {
        assert!(!path.exists(), "{} goes", path.display());
    }
    for path in &oldest_first[2..] {
        assert!(path.exists(), "{} stays", path.display());
    }
}

/// The two oldest are a reading page of an old page version and a tile at a
/// rung no ladder asks for now. Taken for unrecognised, either would stay and
/// the current cover would go in its place.
#[test]
fn a_key_of_an_old_version_or_rung_is_evicted_like_any_other() {
    let store = store("sweep-old-keys");
    let started = started();
    let held = &store.held;
    let old_page = store.thumbs.join(format!("{held}-p1-r1536v1.jpg"));
    assert_ne!(
        old_page,
        cover::page_cache_path(&store.thumbs, held, 1, 1536)
    );
    assert!(!cover::TILE_LONG_EDGES.contains(&200) && !cover::COVER_LONG_EDGES.contains(&200));
    let old_rung = cover::thumb_cache_path(&store.thumbs, held, 1, 200);
    let current = cover::cache_path(&store.covers, held);
    plant_used(&old_page, 100, started, 3);
    plant_used(&old_rung, 200, started, 2);
    plant_used(&current, 300, started, 1);

    let swept = store.sweep_capped(300, started);

    assert_eq!(
        swept,
        Swept {
            evicted: tally(2, 300),
            left: tally(1, 300),
            ..Swept::default()
        }
    );
    assert!(!old_page.exists(), "the old page version goes");
    assert!(!old_rung.exists(), "the old rung goes");
    assert!(current.exists(), "the current cover stays");
}

/// The unrecognised file and the young staging file are over the cap
/// together, and either one fits beside both keys: a total that left either
/// out would keep the keys.
#[test]
fn a_cap_held_up_by_files_it_may_not_evict_evicts_every_key_and_ends() {
    let store = store("sweep-held-up");
    let started = started();
    let keys = [
        cover::cache_path(&store.covers, &store.held),
        cover::thumb_cache_path(&store.thumbs, &store.held, 1, cover::THUMB_LONG_EDGE),
    ];
    for (path, hours) in keys.iter().zip([2, 1]) {
        plant_used(path, 50, started, hours);
    }
    let unrecognised = store.covers.join("notes.txt");
    plant(&unrecognised, 400);
    let young = taiyaku_read::atomic::temp_path(&keys[0], "jpg");
    plant(&young, 400);

    let swept = store.sweep_capped(500, started);

    assert_eq!(
        swept,
        Swept {
            evicted: tally(2, 100),
            left: tally(2, 800),
            unrecognised: 1,
            ..Swept::default()
        }
    );
    for path in &keys {
        assert!(!path.exists(), "{} goes", path.display());
    }
    assert!(unrecognised.exists(), "the unrecognised file stays");
    assert!(young.exists(), "the write in progress stays");
}

#[test]
fn a_cap_of_zero_is_no_cap() {
    let store = store("sweep-no-cap");
    let started = started();
    let keys = [
        cover::cache_path(&store.covers, &store.held),
        cover::thumb_cache_path(&store.thumbs, &store.held, 1, cover::THUMB_LONG_EDGE),
    ];
    for (path, hours) in keys.iter().zip([2, 1]) {
        plant_used(path, 100, started, hours);
    }

    let swept = store.sweep_capped(0, started);

    assert_eq!(
        swept,
        Swept {
            left: tally(2, 200),
            ..Swept::default()
        }
    );
    for path in &keys {
        assert!(path.exists(), "{} stays", path.display());
    }
}

/// Two keys last used at the same instant, listed in the order their names
/// do not sort in: the one whose name sorts first goes, whatever the listing.
#[test]
fn a_tie_in_use_is_broken_by_name() {
    let store = store("sweep-tie");
    let started = started();
    let first = cover::cache_path(&store.covers, &store.held);
    let second = cover::thumb_cache_path(&store.thumbs, &store.held, 1, cover::THUMB_LONG_EDGE);
    assert!(name(&first) < name(&second));
    for path in [&first, &second] {
        plant_used(path, 100, started, 1);
    }

    let swept = store.sweep_capped_over(
        [store.thumbs.clone(), store.covers.clone()],
        100,
        started,
        || {},
    );

    assert_eq!(
        swept,
        Swept {
            evicted: tally(1, 100),
            left: tally(1, 100),
            ..Swept::default()
        }
    );
    assert!(!first.exists(), "the name that sorts first goes");
    assert!(second.exists(), "the other stays");
}

/// The oldest key turns into a directory after the listing, which no
/// `remove_file` can delete, as root or not: it stays counted, and the next
/// oldest goes in its place.
#[test]
fn an_eviction_that_fails_is_counted_and_the_next_oldest_goes() {
    let store = store("sweep-evict-fails");
    let started = started();
    let oldest_first = [
        cover::cache_path(&store.covers, &store.held),
        cover::thumb_cache_path(&store.thumbs, &store.held, 1, cover::THUMB_LONG_EDGE),
        cover::thumb_cache_path(&store.thumbs, &store.held, 2, cover::THUMB_LONG_EDGE),
    ];
    for (path, hours) in oldest_first.iter().zip([3, 2, 1]) {
        plant_used(path, 100, started, hours);
    }
    let stuck = oldest_first[0].clone();

    let swept = store.sweep_capped_over(store.dirs(), 200, started, move || {
        std::fs::remove_file(&stuck).expect("deletes the oldest");
        std::fs::create_dir(&stuck).expect("puts a directory in its place");
    });

    assert_eq!(
        swept,
        Swept {
            evicted: tally(1, 100),
            left: tally(2, 200),
            undeleted: 1,
            first_failure: Some(format!(
                "{}: Is a directory (os error 21)",
                oldest_first[0].display()
            )),
            ..Swept::default()
        }
    );
    assert!(oldest_first[0].is_dir(), "the directory stays");
    assert!(
        !oldest_first[1].exists(),
        "the next oldest goes in its place"
    );
    assert!(oldest_first[2].exists(), "the newest stays");
}

/// The first sweep runs at once, the next a whole day later and not a second
/// sooner. The paused clock runs through each wait without taking it, and
/// stands still while a tick runs on the blocking pool.
#[tokio::test(start_paused = true)]
async fn the_sweep_runs_at_once_and_then_every_day() {
    let (sent, mut ticked) = tokio::sync::mpsc::unbounded_channel();
    let begun = tokio::time::Instant::now();
    let schedule = every(EVERY, move || {
        let _ = sent.send(tokio::time::Instant::now());
    });

    let first = tokio::time::timeout(Duration::from_secs(1), ticked.recv()).await;
    let early =
        tokio::time::timeout(DAY.saturating_sub(Duration::from_secs(1)), ticked.recv()).await;
    let second = tokio::time::timeout(Duration::from_secs(2), ticked.recv()).await;
    schedule.abort();

    assert_eq!(first, Ok(Some(begun)), "the first sweep runs at once");
    assert!(
        early.is_err(),
        "no sweep comes sooner than a day: {early:?}"
    );
    assert_eq!(
        second,
        Ok(Some(begun + DAY)),
        "the next sweep comes a day after the first"
    );
}

/// Whether `path` is gone within a second of the paused clock, looked at each
/// millisecond.
async fn gone_soon(path: &Path) -> bool {
    for _ in 0..1000 {
        if !path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    !path.exists()
}

/// The sweep `spawn` starts runs at once and then a day later, not a second
/// sooner: an orphan planted again after the first sweep outlives a day less
/// a second. The paused clock stands still while a sweep runs on the blocking
/// pool, so a look after a wait sees that sweep done.
#[tokio::test(start_paused = true)]
async fn the_spawned_sweep_runs_at_once_and_then_a_day_later() {
    let (_router, state, _fixture) = crate::test_support::router_over_a_live_library();
    std::fs::create_dir_all(&state.cache_dir).expect("makes the covers directory");
    let orphan = cover::cache_path(&state.cache_dir, UNHELD);
    plant(&orphan, 1);
    let begun = tokio::time::Instant::now();

    let schedule = spawn(&state, 0).expect("a live registry is swept");
    let first = gone_soon(&orphan).await;
    plant(&orphan, 1);
    tokio::time::sleep_until(begun + DAY.saturating_sub(Duration::from_secs(1))).await;
    let early = !orphan.exists();
    tokio::time::sleep_until(begun + DAY).await;
    let second = gone_soon(&orphan).await;
    schedule.abort();

    assert!(first, "the first sweep runs at once");
    assert!(!early, "no sweep comes sooner than a day");
    assert!(second, "the next sweep comes a day after the first");
}

#[tokio::test]
async fn only_a_live_registry_is_swept() {
    let fixture = Fixture::new("sweep-fixed-registry");
    let fixed = Arc::new(crate::test_support::state(fixture.path()));
    let (_router, live, _live_fixture) = crate::test_support::router_over_a_live_library();

    let fixed_sweep = spawn(&fixed, 0);
    let live_sweep = spawn(&live, 0);
    let (fixed_swept, live_swept) = (fixed_sweep.is_some(), live_sweep.is_some());
    for schedule in [fixed_sweep, live_sweep].into_iter().flatten() {
        schedule.abort();
    }

    assert!(!fixed_swept, "a fixed registry is not swept");
    assert!(live_swept, "a live registry is");
}

#[test]
fn the_line_puts_each_figure_in_its_place_and_names_a_failed_delete() {
    let swept = Swept {
        orphans: tally(1, 2),
        leftovers: tally(3, 4),
        evicted: tally(5, 6),
        left: tally(7, 8),
        unrecognised: 9,
        ..Swept::default()
    };
    let failed = Swept {
        undeleted: 10,
        first_failure: Some("/cache/covers/a.jpg: Permission denied (os error 13)".to_string()),
        ..swept.clone()
    };

    assert_eq!(
        report(&swept),
        "cache sweep: deleted 1 orphan(s) (2 bytes), 3 leftover(s) (4 bytes) and 5 \
         eviction(s) (6 bytes); 7 file(s) (8 bytes) left, 9 of them not recognised"
    );
    assert_eq!(
        report(&failed),
        "cache sweep: deleted 1 orphan(s) (2 bytes), 3 leftover(s) (4 bytes) and 5 \
         eviction(s) (6 bytes); 7 file(s) (8 bytes) left, 9 of them not recognised; 10 \
         could not be deleted, the first: /cache/covers/a.jpg: Permission denied (os error 13)"
    );
}

/// The sweep `serve` runs, over a real state: the tile the route wrote is
/// held and stays, and so does a staging file modified between `started`, two
/// hours back, and now. What else was planted in both directories goes.
#[tokio::test]
async fn the_sweep_of_a_state_reads_both_its_cache_directories() {
    let (router, state, _fixture) =
        crate::test_support::router_over_two_scanned_volumes_of_decodable_pages();
    let works = crate::test_support::get_json(&router, "/api/works").await;
    let address = works[0]["cover"].as_str().expect("a work's tile address");
    let response = crate::test_support::get(&router, address).await;
    assert_eq!(response.status(), 200, "{address}");
    let thumbs = state.thumb_cache_dir();
    let written: Vec<PathBuf> = std::fs::read_dir(&thumbs)
        .expect("lists the tiles")
        .map(|entry| entry.expect("reads an entry").path())
        .collect();
    let [tile] = written.as_slice() else {
        panic!("the route writes one tile: {written:?}");
    };
    let started = SystemTime::now() - Duration::from_secs(2 * 3600);
    std::fs::create_dir_all(&state.cache_dir).expect("makes the covers directory");
    let orphans = [
        cover::cache_path(&state.cache_dir, UNHELD),
        cover::thumb_cache_path(&thumbs, UNHELD, 1, cover::THUMB_LONG_EDGE),
        cover::page_cache_path(&thumbs, UNHELD, 1, 1536),
    ];
    for (path, bytes) in orphans.iter().zip([100, 200, 400]) {
        plant(path, bytes);
    }
    let leftover = taiyaku_read::atomic::temp_path(tile, "jpg");
    plant_used(&leftover, 10, started, 1);
    let young = taiyaku_read::atomic::temp_path(tile, "jpg");
    plant(&young, 20);
    set_modified(&young, started + Duration::from_secs(3600));
    let tile_bytes = std::fs::metadata(tile).expect("reads the tile").len();

    let swept = sweep(&state, 0, started).expect("sweeps");

    assert_eq!(
        swept,
        Swept {
            orphans: tally(3, 700),
            leftovers: tally(1, 10),
            left: tally(2, tile_bytes + 20),
            ..Swept::default()
        }
    );
    assert!(tile.exists(), "the route's tile stays");
    assert!(young.exists(), "a staging file since `started` stays");
    for path in orphans.iter().chain([&leftover]) {
        assert!(!path.exists(), "{} goes", path.display());
    }
}

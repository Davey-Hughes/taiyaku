//! Synthetic prints over pools of hashes: a hash a pool gives two files is
//! the same page in both, and a hash drawn afresh is like no other, so which
//! pairs reach the floor, and with which runs, is known by construction.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};
use std::time::Duration;

use rusqlite::Connection;
use taiyaku_core::{Direction, PageHash, PageRange};
use taiyaku_library::matches::{self, BAR, FLOOR, NewPair};
use taiyaku_library::progress::Step;
use taiyaku_library::test_support::plant_root;
use taiyaku_library::{Library, derive, fingerprints, hand, librarydoc, scan};
use taiyaku_read::test_support::cbz_pages;

use super::*;
use crate::fingerprint::{self, PagePrint};
use crate::fingerprint_pass::priority;
use crate::scan_jobs::{JobEnded, JobKind};
use crate::test_support::{self, Fixture, POLL, SETTLE};
use crate::volume_match::{self, SCORER_VERSION, Scored, Slots};

/// A hash about half of whose 256 bits are set and unlike any other seed's:
/// every slot is informative, and two unrelated slots differ in about half
/// their bits, far under the solver's floor.
fn hash(seed: u64) -> PageHash {
    let mut state = seed;
    PageHash(std::array::from_fn(|_| {
        // splitmix64's step.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }))
}

/// Forty pages' hashes in reading order, drawn from seed `from` on.
fn pool(from: u64) -> Vec<PageHash> {
    (from..from + 40).map(hash).collect()
}

/// An ordinary page.
fn page(hash: PageHash) -> PagePrint {
    PagePrint {
        width: 1000,
        height: 1500,
        hash,
        halves: None,
    }
}

/// A joined spread, twice an ordinary page's width, its whole hash drawn from
/// `seed` and its halves' hashes `left` and `right` as the image holds them.
fn spread(seed: u64, left: PageHash, right: PageHash) -> PagePrint {
    PagePrint {
        width: 2000,
        height: 1500,
        hash: hash(seed),
        halves: Some((left, right)),
    }
}

/// A file as the pass reads it: its digest, its work's reading direction and
/// its pages.
struct File {
    digest: &'static str,
    reading: Direction,
    pages: Vec<PagePrint>,
}

impl File {
    fn slots(&self) -> Slots {
        volume_match::slots(&self.pages, self.reading)
    }
}

/// What the pass's scorer gives `x` and `y`.
fn scored(x: &File, y: &File) -> Scored {
    volume_match::score((x.digest, &x.slots()), (y.digest, &y.slots()))
}

/// The pair `x` and `y` as a row names it, the smaller digest first.
fn key(x: &File, y: &File) -> (String, String) {
    let (x, y) = (x.digest.to_owned(), y.digest.to_owned());
    if x < y { (x, y) } else { (y, x) }
}

fn range(start: u32, end: u32) -> PageRange {
    PageRange::new(start, end)
}

/// A, B, C and D, ordinary pages throughout. B is A without its page 20 and
/// with its page 30 drawn afresh: 38 of A's 40 pages pair in order (V 0.95),
/// and B, the shorter, goes first when A scores it, so a figure named by the
/// wrong side shows. C is A with its last twelve pages drawn afresh, 28
/// of 40 (V 0.7). D shares no page with any.
fn four() -> [File; 4] {
    let a = pool(1_000);
    let mut b = a.clone();
    b[30] = hash(2_000);
    b.remove(20);
    let mut c = a.clone();
    for (seed, page) in (3_000..).zip(c.iter_mut().skip(28)) {
        *page = hash(seed);
    }
    let d = pool(4_000);
    [
        ordinary("d-a", a),
        ordinary("d-b", b),
        ordinary("d-c", c),
        ordinary("d-d", d),
    ]
}

/// A right-to-left file of an ordinary page for each of `hashes`.
fn ordinary(digest: &'static str, hashes: Vec<PageHash>) -> File {
    File {
        digest,
        reading: Direction::RightToLeft,
        pages: hashes.into_iter().map(page).collect(),
    }
}

/// A store holding each of `files` as the only member of a volume of its own,
/// read whole under the layout the pass asks for, in a directory a library
/// with matching on reaches: every file takes part.
fn planted(files: &[File]) -> Arc<Library> {
    let library = test_support::library();
    {
        let conn = library.conn();
        plant_root(&conn, 1, "/srv/volumes");
        for (volume, file) in (1_u32..).zip(files) {
            plant(&conn, file, f64::from(volume));
        }
    }
    library
}

/// `file` as a work at `volume` with one edition of one member, and its
/// prints.
fn plant(conn: &Connection, file: &File, volume: f64) {
    let reading = match file.reading {
        Direction::RightToLeft => "rtl",
        Direction::LeftToRight => "ltr",
    };
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, volume)
         VALUES (?1, ?1, ?1, ?2, 'en', ?3)",
        rusqlite::params![file.digest, reading, volume],
    )
    .unwrap_or_else(|e| panic!("plants {}'s work: {e}", file.digest));
    let work = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO edition (work_id, key, label) VALUES (?1, 'en', 'en')",
        [work],
    )
    .unwrap_or_else(|e| panic!("plants {}'s edition: {e}", file.digest));
    let edition = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO member (edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (?1, 0, ?2, ?2, ?3, 1)",
        rusqlite::params![edition, file.digest, count(&file.pages)],
    )
    .unwrap_or_else(|e| panic!("plants {}'s member: {e}", file.digest));
    plant_prints(conn, file.digest, &file.pages);
}

fn count(pages: &[PagePrint]) -> u32 {
    u32::try_from(pages.len()).expect("a file's page count fits u32")
}

/// A whole read of `pages` stored under `digest`.
fn plant_prints(conn: &Connection, digest: &str, pages: &[PagePrint]) {
    let prints: Vec<Option<PagePrint>> = pages.iter().copied().map(Some).collect();
    conn.execute(
        "INSERT INTO fingerprint (sha256, version, outcome, pages, prints)
         VALUES (?1, ?2, 'whole', ?3, ?4)",
        rusqlite::params![
            digest,
            fingerprint::VERSION,
            count(pages),
            fingerprint::encode(&prints)
        ],
    )
    .unwrap_or_else(|e| panic!("plants {digest}'s prints: {e}"));
}

/// The default settings, polling every [`POLL`].
fn polling() -> PassSettings {
    PassSettings {
        poll: POLL,
        ..PassSettings::default()
    }
}

/// A hook that records each digest it is asked with.
fn noting_digests(into: &Arc<Mutex<Vec<String>>>) -> DigestHook {
    let into = Arc::clone(into);
    Arc::new(move |digest: &str| {
        into.lock()
            .expect("the digests looked up")
            .push(digest.to_owned());
    })
}

/// A hook that records each pair it is asked with.
fn noting_pairs(into: &Arc<Mutex<Vec<(String, String)>>>) -> PairHook {
    let into = Arc::clone(into);
    Arc::new(move |a: &str, b: &str| {
        into.lock()
            .expect("the pairs scored")
            .push((a.to_owned(), b.to_owned()));
    })
}

/// Kick the pass and wait for it to end.
fn kicked(state: &Arc<ServeState>) {
    kick(state);
    assert!(state.matches.settle(SETTLE), "the pass ends");
}

/// Every stored pair by its two digests, the smaller first.
fn stored(library: &Library) -> BTreeMap<(String, String), NewPair> {
    matches::pairs(&library.conn())
        .expect("reads the pairs")
        .into_iter()
        .map(|pair| ((pair.new.sha_a.clone(), pair.new.sha_b.clone()), pair.new))
        .collect()
}

/// Each digest looked up, with the fingerprint and scorer versions it was
/// looked up under.
fn lookups(library: &Library) -> Vec<(String, u32, u32)> {
    library
        .conn()
        .prepare(
            "SELECT sha256, fingerprint_version, scorer_version FROM match_lookup
              ORDER BY sha256",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("reads the lookups")
}

/// Each of `files` marked looked up under the pass's versions.
fn every_lookup(files: &[File]) -> Vec<(String, u32, u32)> {
    files
        .iter()
        .map(|file| (file.digest.to_owned(), fingerprint::VERSION, SCORER_VERSION))
        .collect()
}

/// A and B pair at 0.95 and A and C at 0.7, both from the floor up; D's pairs
/// are scored and fall under it, so the mark is all its lookup stores. On one
/// worker A, the first due, scores B, and B, the shorter, goes first: the pair
/// is stored with each figure on its own side.
#[test]
fn a_pass_stores_pairs_from_the_floor_and_marks_every_file() {
    let fixture = Fixture::new("match-pass-floor");
    let files = four();
    let library = planted(&files);
    let scored_pairs = Arc::default();
    let settings = PassSettings {
        workers: 1,
        after_scoring: Some(noting_pairs(&scored_pairs)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let [a, b, c, d] = &files;
    let (ab, ac) = (scored(a, b), scored(a, c));
    assert_eq!((ab.v, ac.v), (0.95, 0.7), "the fixture's V");
    let pairs = stored(&library);
    assert_eq!(
        pairs.get(&key(a, b)),
        Some(&NewPair {
            sha_a: "d-a".to_owned(),
            sha_b: "d-b".to_owned(),
            v: ab.v,
            long_paired: 38,
            a_slots: 40,
            b_slots: 39,
            a_pages: 40,
            b_pages: 39,
            floor: ab.floor,
            runs: vec![
                (range(0, 19), range(0, 19)),
                (range(21, 29), range(20, 28)),
                (range(31, 39), range(30, 38)),
            ],
        }),
        "A and B, each figure on its own side"
    );
    assert_eq!(
        pairs.get(&key(a, c)),
        Some(&NewPair {
            sha_a: "d-a".to_owned(),
            sha_b: "d-c".to_owned(),
            v: ac.v,
            long_paired: 28,
            a_slots: 40,
            b_slots: 40,
            a_pages: 40,
            b_pages: 40,
            floor: ac.floor,
            runs: vec![(range(0, 27), range(0, 27))],
        }),
        "A and C"
    );
    let file = |digest: &str| {
        files
            .iter()
            .find(|file| file.digest == digest)
            .unwrap_or_else(|| panic!("{digest} is planted"))
    };
    for ((x, y), pair) in &pairs {
        assert!(
            x != d.digest && y != d.digest,
            "no pair names D: {x} and {y}"
        );
        assert_eq!(pair.v, scored(file(x), file(y)).v, "{x} and {y} at their V");
    }
    let scored_pairs = scored_pairs.lock().expect("the pairs scored").clone();
    assert_eq!(
        scored_pairs.first(),
        Some(&key(a, b)),
        "the fixture: A's lookup scores B first"
    );
    assert!(
        (scored_pairs.iter()).any(|(x, y)| x == d.digest || y == d.digest),
        "the fixture: a pair of D's is scored: {scored_pairs:?}"
    );
    assert_eq!(
        lookups(&library),
        every_lookup(&files),
        "every file is marked looked up, D with no pair"
    );

    let job = state.matches.job().expect("the pass showed a job");
    assert_eq!(job.ended(), Some(JobEnded::Finished), "how the job ended");
    assert_eq!(
        (job.kind, job.kind.as_str()),
        (JobKind::Match, "match"),
        "the job's kind"
    );
    let snapshot = job.snapshot();
    assert_eq!(
        (snapshot.step, snapshot.done, snapshot.of),
        (Step::Matching, Some(4), Some(4)),
        "(step, done, of)"
    );
    assert!(job.dirs.is_empty(), "the pass holds no directory");
    let coverage = fingerprints::coverage(&library.conn()).expect("reads the coverage");
    assert_eq!(
        *job.libraries.lock().expect("the job's libraries"),
        coverage.libraries,
        "the libraries with matching on"
    );
}

/// A pair exactly at the floor is stored: E is A with its last sixteen pages
/// drawn afresh, so 24 of their 40 pages pair and V is 24 / 40, which is the
/// f64 0.6 itself.
#[test]
fn a_pair_at_exactly_the_floor_is_stored() {
    let fixture = Fixture::new("match-pass-at-the-floor");
    let a = pool(1_000);
    let mut e = a.clone();
    for (seed, page) in (5_000..).zip(e.iter_mut().skip(24)) {
        *page = hash(seed);
    }
    let files = [ordinary("d-a", a), ordinary("d-e", e)];
    let library = planted(&files);
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, polling());

    kicked(&state);

    let ae = scored(&files[0], &files[1]);
    assert_eq!(
        (ae.long_paired, ae.v),
        (24, FLOOR),
        "the fixture: 24 of 40 pages pair, V exactly the floor"
    );
    assert_eq!(
        stored(&library)
            .get(&key(&files[0], &files[1]))
            .map(|pair| pair.v),
        Some(FLOOR),
        "A and E, at the floor, are stored"
    );
    assert_eq!(lookups(&library), every_lookup(&files), "each marked");
}

/// A cancel lands as the first file's lookup is stored, on one worker; the
/// next kick looks up the other three and not that one again, and a kick with
/// nothing due shows no job.
#[test]
fn a_cancelled_pass_resumes_without_looking_up_a_file_twice() {
    let fixture = Fixture::new("match-pass-resume");
    let files = four();
    let library = planted(&files);
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let cancelling = {
        let (seen, served) = (Arc::clone(&seen), Arc::clone(&served));
        move |digest: &str| {
            let mut seen = seen.lock().expect("the digests looked up");
            seen.push(digest.to_owned());
            if seen.len() == 1
                && let Some(state) = served.get().and_then(Weak::upgrade)
                && let Some(job) = state.matches.job()
            {
                state.matches.cancel(&job);
            }
        }
    };
    let settings = PassSettings {
        workers: 1,
        after_lookup: Some(Arc::new(cancelling)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);
    assert!(served.set(Arc::downgrade(&state)).is_ok(), "set once");

    kicked(&state);

    let first = state.matches.job().expect("the first pass showed a job");
    assert_eq!(
        first.ended(),
        Some(JobEnded::Cancelled(Step::Matching)),
        "the cancel stopped the first pass"
    );
    let looked = seen.lock().expect("the digests looked up").clone();
    assert_eq!(looked.len(), 1, "one file looked up: {looked:?}");
    let marked: Vec<String> = (lookups(&library).into_iter())
        .map(|(digest, _, _)| digest)
        .collect();
    assert_eq!(marked, looked, "only its lookup is stored");

    kicked(&state);

    let second = state.matches.job().expect("the second pass showed a job");
    assert_ne!(second.id, first.id, "the second pass has a job of its own");
    assert_eq!(
        second.ended(),
        Some(JobEnded::Finished),
        "and ran to its end"
    );
    let looked = seen.lock().expect("the digests looked up").clone();
    let distinct: HashSet<&str> = looked.iter().map(String::as_str).collect();
    assert_eq!(
        distinct.len(),
        looked.len(),
        "no file looked up twice: {looked:?}"
    );
    assert_eq!(
        distinct,
        files.iter().map(|file| file.digest).collect(),
        "every file looked up"
    );
    assert_eq!(lookups(&library), every_lookup(&files), "each marked");

    kicked(&state);

    assert_eq!(
        state.matches.job().map(|job| job.id),
        Some(second.id),
        "with nothing due, a kick shows no job"
    );
}

/// A kick lands as the first file's lookup is stored, on one worker, with no
/// cancel: the pass lists again for the job it runs, which ends finished, and
/// no file is looked up twice.
#[test]
fn a_kick_during_a_pass_lists_again_for_its_own_job() {
    let fixture = Fixture::new("match-pass-kick-during");
    let files = four();
    let library = planted(&files);
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
    let kicking = {
        let (seen, served) = (Arc::clone(&seen), Arc::clone(&served));
        move |digest: &str| {
            let mut seen = seen.lock().expect("the digests looked up");
            seen.push(digest.to_owned());
            if seen.len() == 1
                && let Some(state) = served.get().and_then(Weak::upgrade)
            {
                kick(&state);
            }
        }
    };
    let settings = PassSettings {
        workers: 1,
        after_lookup: Some(Arc::new(kicking)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);
    assert!(served.set(Arc::downgrade(&state)).is_ok(), "set once");

    kicked(&state);

    let job = state.matches.job().expect("the pass showed a job");
    assert_eq!(
        job.ended(),
        Some(JobEnded::Finished),
        "the kick ended nothing"
    );
    let looked = seen.lock().expect("the digests looked up").clone();
    let distinct: HashSet<&str> = looked.iter().map(String::as_str).collect();
    assert_eq!(
        distinct.len(),
        looked.len(),
        "no file looked up twice: {looked:?}"
    );
    assert_eq!(lookups(&library), every_lookup(&files), "each marked");
}

/// A lookup panics after the first file is marked, on one worker, which takes
/// the coordinator down with it: its job ends, and a later kick starts a pass
/// that looks up what is still due.
#[test]
fn a_panic_in_a_lookup_leaves_a_later_kick_to_start_a_pass() {
    let fixture = Fixture::new("match-pass-panics");
    let files = four();
    let library = planted(&files);
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let panicking = {
        let armed = Arc::clone(&armed);
        move |_: &str| {
            if armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                panic!("a lookup bug");
            }
        }
    };
    let settings = PassSettings {
        workers: 1,
        after_lookup: Some(Arc::new(panicking)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let first = state.matches.job().expect("the first pass showed a job");
    assert_eq!(first.ended(), Some(JobEnded::Finished), "its job is ended");

    kicked(&state);

    let second = state.matches.job().expect("a later kick showed a job");
    assert_ne!(second.id, first.id, "a later kick starts a pass");
    assert_eq!(lookups(&library), every_lookup(&files), "every file marked");
}

/// A row stays as stored, so a pair already stored is not scored: one planted
/// at 0.99 where A and B score 0.95.
#[test]
fn a_pair_already_stored_is_not_scored_again() {
    let fixture = Fixture::new("match-pass-stored");
    let files = four();
    let library = planted(&files);
    library
        .conn()
        .execute(
            "INSERT INTO content_match
               (sha_a, sha_b, v, long_paired, a_slots, b_slots, a_pages, b_pages, floor, runs)
             VALUES ('d-a', 'd-b', 0.99, 39, 40, 39, 40, 39, NULL, '[]')",
            [],
        )
        .expect("plants A and B at 0.99");
    let scored_pairs = Arc::default();
    let settings = PassSettings {
        after_scoring: Some(noting_pairs(&scored_pairs)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let pairs = stored(&library);
    let ab = (pairs.get(&key(&files[0], &files[1]))).expect("A and B are still stored");
    assert_eq!(
        (ab.v, ab.long_paired, ab.floor, ab.runs.len()),
        (0.99, 39, None, 0),
        "A and B as planted"
    );
    let scored_pairs = scored_pairs.lock().expect("the pairs scored").clone();
    assert!(
        !scored_pairs.contains(&key(&files[0], &files[1])),
        "A and B are not scored: {scored_pairs:?}"
    );
    assert!(
        scored_pairs.contains(&key(&files[0], &files[2])),
        "the fixture: A's other leader is scored: {scored_pairs:?}"
    );
    assert_eq!(lookups(&library), every_lookup(&files), "each marked");
}

/// A file whose bytes change, as on a re-download, is known by its new digest,
/// which no lookup has marked: the next pass looks it up, and it alone.
#[test]
fn a_file_with_new_bytes_is_looked_up_afresh() {
    let fixture = Fixture::new("match-pass-new-bytes");
    let files = four();
    let library = planted(&files);
    let seen = Arc::default();
    let settings = PassSettings {
        after_lookup: Some(noting_digests(&seen)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);
    kicked(&state);
    assert_eq!(
        seen.lock().expect("the digests looked up").len(),
        4,
        "the fixture: the first pass looks up all four"
    );
    let renewed = File {
        digest: "d-a2",
        reading: Direction::RightToLeft,
        pages: files[0].pages.clone(),
    };
    {
        let conn = library.conn();
        conn.execute("UPDATE member SET sha256 = 'd-a2' WHERE sha256 = 'd-a'", [])
            .expect("A's bytes change");
        plant_prints(&conn, renewed.digest, &renewed.pages);
    }

    kicked(&state);

    assert_eq!(
        seen.lock().expect("the digests looked up")[4..],
        ["d-a2"],
        "the second pass looks up the new digest alone"
    );
    assert_eq!(
        stored(&library)
            .get(&key(&renewed, &files[1]))
            .map(|pair| pair.v),
        Some(scored(&renewed, &files[1]).v),
        "the new digest pairs with B"
    );
    assert!(
        lookups(&library).contains(&("d-a2".to_owned(), fingerprint::VERSION, SCORER_VERSION)),
        "the new digest is marked"
    );
}

/// `pages` as ten runs of two ordinary pages and a spread, its whole hashes
/// drawn from `seed` on: each spread's halves are the next two pages, the
/// earlier on the side a work reading `reading` reads first.
fn as_spreads(pages: &[PageHash], seed: u64, reading: Direction) -> Vec<PagePrint> {
    (seed..)
        .zip(pages.chunks(4))
        .flat_map(|(seed, four)| {
            let (earlier, later) = (four[2], four[3]);
            let (left, right) = match reading {
                Direction::LeftToRight => (earlier, later),
                Direction::RightToLeft => (later, earlier),
            };
            [page(four[0]), page(four[1]), spread(seed, left, right)]
        })
        .collect()
}

/// Forty pages, each its own page in one file and in the other ten runs of
/// two pages and a spread: the spreads' halves line up with the pages only in
/// the order their work reads them, and read the other way no run is four
/// slots long. One pair reads left to right and one right to left, so a pass
/// that read every work one way stores neither of the other way's.
#[test]
fn a_left_to_right_work_is_scored_in_its_own_direction() {
    let fixture = Fixture::new("match-pass-direction");
    let (ltr, rtl) = (pool(6_000), pool(7_000));
    let files = [
        File {
            digest: "d-ltr-pages",
            reading: Direction::LeftToRight,
            pages: ltr.iter().copied().map(page).collect(),
        },
        File {
            digest: "d-ltr-spreads",
            reading: Direction::LeftToRight,
            pages: as_spreads(&ltr, 6_100, Direction::LeftToRight),
        },
        File {
            digest: "d-rtl-pages",
            reading: Direction::RightToLeft,
            pages: rtl.iter().copied().map(page).collect(),
        },
        File {
            digest: "d-rtl-spreads",
            reading: Direction::RightToLeft,
            pages: as_spreads(&rtl, 7_100, Direction::RightToLeft),
        },
    ];
    let library = planted(&files);
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, polling());

    kicked(&state);

    let pairs = stored(&library);
    for (pages, spreads) in [(&files[0], &files[1]), (&files[2], &files[3])] {
        let named = format!("{} and {}", pages.digest, spreads.digest);
        let pair =
            (pairs.get(&key(pages, spreads))).unwrap_or_else(|| panic!("{named} are stored"));
        assert!(pair.v >= BAR, "{named} join: {}", pair.v);
        assert_eq!(pair.v, scored(pages, spreads).v, "{named} at their V");
        assert_eq!(
            (pair.a_slots, pair.b_slots, pair.a_pages, pair.b_pages),
            (40, 40, 40, 30),
            "{named}: each spread gives two slots"
        );
    }
}

/// A lookup is stored only once the job lock is free, since a rule save's
/// derive writes the whole store under it.
#[test]
fn a_lookup_is_stored_once_the_job_lock_is_free() {
    let fixture = Fixture::new("match-pass-job-lock");
    let files = four();
    let library = planted(&files);
    let (scoring, scored_one) = mpsc::channel();
    let settings = PassSettings {
        after_scoring: Some(Arc::new(move |_: &str, _: &str| {
            let _ = scoring.send(());
        })),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);
    let held = test_support::holding_the_job(&state);

    kick(&state);
    scored_one
        .recv_timeout(SETTLE)
        .expect("a worker scores a pair");
    std::thread::sleep(POLL * 4);

    assert_eq!(
        lookups(&library),
        Vec::new(),
        "no lookup stored while the lock is held"
    );
    assert_eq!(stored(&library), BTreeMap::new(), "nor any pair");
    drop(held);
    assert!(state.matches.settle(SETTLE), "the pass ends");
    assert_eq!(
        lookups(&library),
        every_lookup(&files),
        "each stored once it is free"
    );
}

/// A hook that counts each call.
fn counting(into: &Arc<AtomicUsize>) -> Hook {
    let into = Arc::clone(into);
    Arc::new(move || {
        into.fetch_add(1, Ordering::SeqCst);
    })
}

/// The feed's `ended` marker of `state`.
fn ended(state: &ServeState) -> u64 {
    state.jobs_ended.load(Ordering::SeqCst)
}

/// The title set by hand on [`planted_beside_a_document_gap`]'s volume.
const HAND_TITLE: &str = "Hand-Set Title";

/// A store holding each of `files` as [`planted`] does, beside a directory
/// that keeps hand documents, holding one scanned volume whose title was set
/// by hand and whose document has since gone: what a derive that reconciles
/// documents writes again, and nothing else does. Answers the store and the
/// document's path.
fn planted_beside_a_document_gap(fixture: &Fixture, files: &[File]) -> (Arc<Library>, PathBuf) {
    let library = test_support::library();
    let address = {
        let conn = library.conn();
        plant_root(&conn, 1, "/srv/volumes");
        let top = fixture.join("documented");
        let series = top.join("Documented");
        std::fs::create_dir_all(&series).expect("makes the series directory");
        cbz_pages(&series, "Documented v01.cbz", 3, 0);
        let root = scan::add_root(&conn, &top).expect("adds the directory");
        scan::scan_root(&conn, &root).expect("walks it");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the directory keeps hand documents");
        derive::derive(&conn).expect("derives the volume");
        let work: i64 = conn
            .query_row(
                "SELECT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
                  WHERE m.root_id = ?1",
                [root.id],
                |r| r.get(0),
            )
            .expect("the volume's work");
        let mut doc = librarydoc::of(&conn, work).expect("projects the work");
        doc.title = Some(HAND_TITLE.to_owned());
        librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
        hand::save(&conn, work).expect("saves the document");
        derive::derive(&conn).expect("settles the document");
        let address = hand::path_for(&conn, work)
            .expect("addresses the document")
            .expect("a work in a directory has an address");
        std::fs::remove_file(&address).expect("the document goes");
        for (volume, file) in (1_u32..).zip(files) {
            plant(&conn, file, f64::from(volume));
        }
        address
    };
    (library, address)
}

/// The title the document at `address` holds, or `None` where there is none.
fn documented_title(address: &Path) -> Option<String> {
    let text = std::fs::read_to_string(address).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).expect("a JSON document");
    doc["title"].as_str().map(str::to_owned)
}

/// A and B pair at 0.95, from the bar up, so the pass derives once its
/// lookups are stored, and the derive writes the missing document again. The
/// pass's own job is not counted in `ended`, so the one move is the derive's.
#[test]
fn a_pass_that_stored_a_pair_at_the_bar_derives_once_and_moves_ended() {
    let fixture = Fixture::new("match-pass-derives");
    let files = four();
    let (library, address) = planted_beside_a_document_gap(&fixture, &files);
    let derives = Arc::default();
    let settings = PassSettings {
        after_derive: Some(counting(&derives)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);
    let before = (ended(&state), documented_title(&address));

    kicked(&state);

    let ab = stored(&library)
        .get(&key(&files[0], &files[1]))
        .map(|pair| pair.v);
    assert!(
        ab.is_some_and(|v| v >= BAR),
        "the fixture: A and B are stored at the bar: {ab:?}"
    );
    assert_eq!(before.1, None, "the fixture: the document has gone");
    assert_eq!(derives.load(Ordering::SeqCst), 1, "one derive");
    assert_eq!(
        documented_title(&address).as_deref(),
        Some(HAND_TITLE),
        "the derive wrote the document again"
    );
    assert_eq!(ended(&state), before.0 + 1, "`ended` moves once");
    assert_eq!(
        state.matches.job().and_then(|job| job.ended()),
        Some(JobEnded::Finished),
        "the job ran to its end"
    );
}

/// Each work holding `digest`, by id.
fn holders(conn: &Connection, digest: &str) -> Vec<i64> {
    conn.prepare(
        "SELECT DISTINCT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE m.sha256 = ?1 ORDER BY e.work_id",
    )
    .and_then(|mut stmt| stmt.query_map([digest], |r| r.get(0))?.collect())
    .unwrap_or_else(|e| panic!("the works holding {digest}: {e}"))
}

/// An English volume and its raw, scanned from directories of library
/// `Manga`, which matches by content, each in a series of its own, carry A's
/// and B's prints (V 0.95): the pass stores their pair at the bar, and its own
/// derive puts the raw on the English volume's work, joined by that pair.
#[test]
fn a_pair_the_pass_stores_at_the_bar_joins_at_its_derive() {
    let fixture = Fixture::new("match-pass-joins");
    let [a, b, ..] = four();
    let library = test_support::library();
    let (en, ja) = {
        let conn = library.conn();
        let mut scanned = Vec::new();
        for (seed, name, key, kind, file) in [
            (10, "Volumes", "en", "official", "Oshi/v01/Oshi v01.cbz"),
            (20, "Raw", "ja", "raw", "Oshinoko/v01/Oshinoko v01.cbz"),
        ] {
            let at = fixture.join(name);
            let (dir, archive) = file.rsplit_once('/').expect("a file in a directory");
            std::fs::create_dir_all(at.join(dir)).expect("makes the volume's directory");
            cbz_pages(&at.join(dir), archive, 3, seed);
            let root = scan::add_root_in(&conn, &at, scan::InLibrary::Named("Manga"))
                .expect("adds the directory");
            conn.execute(
                "UPDATE root SET edition_key = ?2, default_kind = ?3, default_language = ?2
                  WHERE id = ?1",
                rusqlite::params![root.id, key, kind],
            )
            .expect("labels it");
            scan::scan_root(&conn, &root).expect("scans it");
            scanned.push((root.id, file));
        }
        derive::derive(&conn).expect("derives each volume a work of its own");
        let digests: Vec<String> = scanned
            .iter()
            .map(|(root, file)| {
                conn.query_row(
                    "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
                    rusqlite::params![root, file],
                    |r| r.get(0),
                )
                .expect("the file's digest")
            })
            .collect();
        plant_prints(&conn, &digests[0], &a.pages);
        plant_prints(&conn, &digests[1], &b.pages);
        (digests[0].clone(), digests[1].clone())
    };
    let apart = {
        let conn = library.conn();
        (holders(&conn, &en), holders(&conn, &ja))
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, polling());

    kicked(&state);

    let conn = library.conn();
    let (id, v): (i64, f64) = conn
        .query_row(
            "SELECT id, v FROM content_match WHERE ?1 IN (sha_a, sha_b) AND ?2 IN (sha_a, sha_b)",
            [&en, &ja],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the pass stored the pair");
    assert!(v >= BAR, "the fixture: the pair is at the bar: {v}");
    assert_ne!(apart.0, apart.1, "the fixture: two works before the pass");
    let host = holders(&conn, &en);
    assert_eq!(
        holders(&conn, &ja),
        host,
        "the raw joined the English volume's work"
    );
    let editions: Vec<(String, Option<i64>)> = conn
        .prepare("SELECT key, joined_by FROM edition WHERE work_id = ?1 ORDER BY key")
        .and_then(|mut stmt| {
            stmt.query_map([host[0]], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("the host's editions");
    assert_eq!(
        editions,
        [("en".to_owned(), None), ("ja".to_owned(), Some(id))],
        "the raw is an edition of it, joined by the pair"
    );
}

/// The line the pass says after its derive names each count of the report
/// it reads, the files joined by content among them, each distinct so one
/// read in another's place shows.
#[test]
fn the_derive_line_counts_the_files_joined_by_content() {
    let report = derive::DeriveReport {
        series: 2,
        works: 3,
        editions: 4,
        members: 5,
        joined: 1,
        ..derive::DeriveReport::default()
    };
    let more = derive::DeriveReport {
        joined: 6,
        ..report.clone()
    };

    assert_eq!(
        [
            derived_line(1, &Ok(report)),
            derived_line(7, &Ok(more)),
            derived_line(1, &Err("the store refused".to_owned())),
        ],
        [
            "stored 1 pair at the bar, then derived 2 series, 3 works, 4 editions and 5 members; \
             1 file joined by content",
            "stored 7 pairs at the bar, then derived 2 series, 3 works, 4 editions and 5 members; \
             6 files joined by content",
            "stored 1 pair at the bar, then the derive failed: the store refused",
        ]
    );
}

/// A and C pair at 0.7, which is stored and under the bar, and D pairs with
/// neither: a pair stored is not enough for a derive, so the missing document
/// stays missing.
#[test]
fn a_pass_that_stored_only_pairs_under_the_bar_derives_nothing() {
    let fixture = Fixture::new("match-pass-under-the-bar");
    let [a, _, c, d] = four();
    let files = [a, c, d];
    let (library, address) = planted_beside_a_document_gap(&fixture, &files);
    let derives = Arc::default();
    let settings = PassSettings {
        after_derive: Some(counting(&derives)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);
    let before = ended(&state);

    kicked(&state);

    let stored_v: Vec<f64> = stored(&library).values().map(|pair| pair.v).collect();
    assert_eq!(
        stored_v,
        [0.7],
        "the fixture: A and C alone are stored, under the bar"
    );
    assert_eq!(derives.load(Ordering::SeqCst), 0, "no derive");
    assert_eq!(documented_title(&address), None, "the document stays gone");
    assert_eq!(ended(&state), before, "`ended` stays put");
    assert_eq!(lookups(&library), every_lookup(&files), "each marked");
}

/// The derive holds the job lock and the lease that a pressed scan, a save and
/// a CLI derive wait for, so it runs at the server's own priority while the
/// pass's lookups run lowered.
#[test]
fn the_pass_derive_runs_at_the_servers_priority() {
    // `thread_is_lowered` reads a thread against the main thread, and nothing
    // goes below niceness 19, so a process started there (`scripts/mutants.sh`
    // starts one) has no lowered thread to find.
    let main = std::fs::read_to_string("/proc/self/stat").expect("the process's stat");
    let niceness = (main.rsplit_once(')')).and_then(|(_, rest)| rest.split_whitespace().nth(16));
    if niceness == Some("19") {
        eprintln!(
            "NOT MEASURED: the_pass_derive_runs_at_the_servers_priority (already at niceness 19)"
        );
        return;
    }
    let fixture = Fixture::new("match-pass-derive-priority");
    let files = four();
    let library = planted(&files);
    let lowered: Arc<Mutex<Vec<(&'static str, bool)>>> = Arc::default();
    let noting = |at: &'static str| {
        let lowered = Arc::clone(&lowered);
        move || {
            lowered
                .lock()
                .expect("the readings")
                .push((at, priority::thread_is_lowered()));
        }
    };
    let (looking, deriving) = (noting("lookup"), noting("derive"));
    let settings = PassSettings {
        workers: 1,
        after_lookup: Some(Arc::new(move |_: &str| looking())),
        after_derive: Some(Arc::new(deriving)),
        ..polling()
    };
    let state = test_support::with_a_match_pass(Arc::clone(&library), &fixture, settings);

    kicked(&state);

    let readings = lowered.lock().expect("the readings").clone();
    let lookups: Vec<bool> = (readings.iter())
        .filter(|(at, _)| *at == "lookup")
        .map(|(_, lowered)| *lowered)
        .collect();
    assert_eq!(
        lookups, [true; 4],
        "the fixture: each lookup runs lowered: {readings:?}"
    );
    let derives: Vec<bool> = (readings.iter())
        .filter(|(at, _)| *at == "derive")
        .map(|(_, lowered)| *lowered)
        .collect();
    assert_eq!(derives, [false], "one derive, not lowered: {readings:?}");
}

/// A pass over `four`, on one worker, whose last lookup takes the job lock as
/// it is stored, so the derive A and B call for finds it held.
struct HeldAtTheDerive {
    _fixture: Fixture,
    state: Arc<ServeState>,
    derives: Arc<AtomicUsize>,
    held: Arc<Mutex<Option<test_support::HeldJob>>>,
}

impl HeldAtTheDerive {
    /// Kicked, and returned once the lock is taken.
    fn kicked(tag: &str) -> Self {
        let fixture = Fixture::new(tag);
        let files = four();
        let library = planted(&files);
        let derives = Arc::default();
        let held: Arc<Mutex<Option<test_support::HeldJob>>> = Arc::default();
        let served: Arc<OnceLock<Weak<ServeState>>> = Arc::default();
        let (took, taken) = mpsc::channel::<()>();
        let taking = {
            let (held, served) = (Arc::clone(&held), Arc::clone(&served));
            let looked = AtomicUsize::new(0);
            let last = files.len();
            move |_: &str| {
                if looked.fetch_add(1, Ordering::SeqCst) + 1 == last
                    && let Some(state) = served.get().and_then(Weak::upgrade)
                {
                    *held.lock().expect("the held lock") =
                        Some(test_support::holding_the_job(&state));
                    let _ = took.send(());
                }
            }
        };
        let settings = PassSettings {
            workers: 1,
            after_lookup: Some(Arc::new(taking)),
            after_derive: Some(counting(&derives)),
            ..polling()
        };
        let state = test_support::with_a_match_pass(library, &fixture, settings);
        assert!(served.set(Arc::downgrade(&state)).is_ok(), "set once");
        kick(&state);
        taken
            .recv_timeout(SETTLE)
            .expect("the last lookup takes the lock");
        Self {
            _fixture: fixture,
            state,
            derives,
            held,
        }
    }

    fn derives(&self) -> usize {
        self.derives.load(Ordering::SeqCst)
    }

    fn let_go(&self) {
        drop(self.held.lock().expect("the held lock").take());
    }
}

/// The pass's derive writes the whole store, as a scan's does, so it waits
/// for the job lock as a scan does: none while the lock is held, one once it
/// is let go.
#[test]
fn the_pass_derive_waits_for_the_job_lock() {
    let pass = HeldAtTheDerive::kicked("match-pass-derive-waits");
    std::thread::sleep(POLL * 4);

    let waiting = (pass.derives(), pass.state.matches.settle(Duration::ZERO));
    pass.let_go();
    assert!(pass.state.matches.settle(SETTLE), "the pass ends");

    assert_eq!(
        waiting,
        (0, false),
        "(derives, ended) while the lock is held"
    );
    assert_eq!(pass.derives(), 1, "one derive once it is let go");
}

/// A cancel while the derive waits for the job lock ends the job there, with
/// no derive: the pairs stay stored for the next derive to read.
#[test]
fn a_cancel_while_the_pass_derive_waits_ends_the_job_with_no_derive() {
    let pass = HeldAtTheDerive::kicked("match-pass-derive-cancelled");
    let job = pass.state.matches.job().expect("the pass shows a job");
    let before = ended(&pass.state);

    pass.state.matches.cancel(&job);
    let settled = pass.state.matches.settle(SETTLE);
    pass.let_go();

    assert!(settled, "the pass ends with the lock still held");
    assert_eq!(
        job.ended(),
        Some(JobEnded::Cancelled(Step::Matching)),
        "how the job ended"
    );
    assert_eq!(pass.derives(), 0, "no derive");
    assert_eq!(ended(&pass.state), before, "`ended` stays put");
}

/// A pass's debug line says whether it is live: an inert one starts nothing.
#[test]
fn a_pass_says_in_its_debug_line_whether_it_is_live() {
    assert_eq!(format!("{:?}", Pass::default()), "Pass { live: false, .. }");
    assert_eq!(format!("{:?}", Pass::live()), "Pass { live: true, .. }");
}

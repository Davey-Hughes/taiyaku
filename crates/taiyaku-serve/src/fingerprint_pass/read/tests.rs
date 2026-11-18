use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Barrier, mpsc};
use std::time::{Duration, Instant, SystemTime};

use taiyaku_library::fingerprints::{Outcome, Row, Unit, stamp_of};
use taiyaku_library::scan;
use taiyaku_read::test_support::{
    PRINT_FIXTURE, TempDir, print_fixture_cbz, print_fixture_folder, print_fixture_pages,
    print_page,
};

use super::*;
use crate::fingerprint::{self, Border, LARGE_PAGE_BYTES, PagePrint, border_sample, decode};

/// Four workers at once, at the threshold and one byte under it. At it, a
/// closure that sleeps while it counts who else is inside sees itself alone;
/// under it, each waits for the other three and sees them. A lock taken at
/// every size fails the second half, a `>` for `>=` the first.
#[test]
fn large_pages_run_one_at_a_time() {
    let pages = LargePages::new(1000);

    let inside = AtomicUsize::new(0);
    let most = AtomicUsize::new(0);
    let start = Barrier::new(4);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                start.wait();
                pages.run(1000, || {
                    most.fetch_max(inside.fetch_add(1, SeqCst) + 1, SeqCst);
                    std::thread::sleep(Duration::from_millis(50));
                    inside.fetch_sub(1, SeqCst);
                });
            });
        }
    });
    assert_eq!(
        most.load(SeqCst),
        1,
        "a page at the threshold decodes alone"
    );

    let inside = AtomicUsize::new(0);
    let seen: Vec<usize> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    pages.run(999, || {
                        inside.fetch_add(1, SeqCst);
                        let deadline = Instant::now() + Duration::from_secs(2);
                        while inside.load(SeqCst) < 4 && Instant::now() < deadline {
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        inside.load(SeqCst)
                    })
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    assert_eq!(
        seen,
        [4, 4, 4, 4],
        "pages under the threshold decode side by side"
    );
}

/// The server's own threshold, in page sizes rather than in its name: while
/// a page the size of an archival scan's decodes under the lock, a page at the
/// top of the ordinary range decodes beside it. A threshold at or under an
/// ordinary page would hold every page of an ordinary file one at a time,
/// which only slows the pass, so nothing else fails on it.
#[test]
fn the_servers_threshold_lets_an_ordinary_page_pass_while_an_archival_one_holds_the_lock() {
    let pages = &LargePages::new(LARGE_PAGE_BYTES);
    let (held, holding) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let (done, finished) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            pages.run(430 * 1024 * 1024, move || {
                held.send(()).unwrap();
                let _ = released.recv();
            });
        });
        holding.recv().expect("the archival page holds the lock");
        scope.spawn(move || {
            pages.run(3 * 1024 * 1024, move || {
                let _ = done.send(());
            });
        });
        let passed = finished.recv_timeout(Duration::from_secs(10));
        release.send(()).unwrap();
        assert!(
            passed.is_ok(),
            "a 3 MB page decodes while a 430 MB one holds the lock"
        );
    });
}

/// A decoder that panics on one large page must not close the lock to every
/// large page after it, since the pass goes on past a file that panicked.
#[test]
fn a_panic_under_the_lock_leaves_it_open() {
    let pages = LargePages::new(1000);
    let panicked = std::panic::catch_unwind(|| pages.run(1000, || panic!("a decoder bug")));
    assert!(panicked.is_err());
    assert_eq!(pages.run(1000, || 7), 7);
}

/// The build the reads below are stamped with.
const BUILD: &str = "a-build";

/// `rel_path` under `tree` as a scan of `tree` records it, the unit a pass
/// is handed, and its digest as the scan's derive takes one.
fn scanned(tree: &Path, rel_path: &str) -> (Unit, String) {
    let library = crate::test_support::library();
    let conn = library.conn();
    let root = scan::add_root(&conn, tree).expect("adds the directory");
    scan::scan_root(&conn, &root).expect("walks it");
    let (size, mtime) = conn
        .query_row(
            "SELECT size, mtime FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, rel_path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or_else(|e| panic!("the walk recorded {rel_path}: {e}"));
    let path = root.path.join(rel_path);
    let digest = taiyaku_read::member_digest(&path).expect("hashes it");
    let unit = Unit {
        root_id: root.id,
        rel_path: rel_path.to_owned(),
        folder: path.is_dir(),
        path,
        sha256: Some(digest.clone()),
        size,
        mtime,
    };
    (unit, digest)
}

/// [`read_unit`] stamped [`BUILD`], through a lock no other read holds.
fn read(unit: &Unit, digest: &str, between: &mut dyn FnMut(Phase, usize) -> bool) -> Read {
    read_unit(
        unit,
        digest,
        Some(BUILD),
        &LargePages::new(LARGE_PAGE_BYTES),
        between,
    )
}

/// The row a read stored, or a panic naming what it came to instead.
fn stored(read: Read) -> Row {
    match read {
        Read::Stored(row) => row,
        other => panic!("stored nothing: {other:?}"),
    }
}

/// `entries` as a cbz at `path`, stored rather than compressed, so each
/// entry's bytes can be found in the archive's as they are.
fn cbz_of(path: &Path, entries: impl IntoIterator<Item = (String, Vec<u8>)>) {
    let mut zip = zip::ZipWriter::new(File::create(path).expect("creates the cbz"));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        zip.start_file(name, stored).expect("starts an entry");
        zip.write_all(&bytes).expect("writes an entry");
    }
    zip.finish().expect("finishes the cbz");
}

/// The print fixture's pages with page `bad` replaced by bytes the reader
/// lists as a page and no decoder takes.
fn fixture_with_garbage_at(bad: usize) -> Vec<(String, Vec<u8>)> {
    let mut pages = print_fixture_pages();
    pages[bad].1 = b"a png by name only".to_vec();
    pages
}

/// `entries` under `tree` as a cbz, or as a folder of files when `folder`,
/// and the path a scan of `tree` records them under.
fn plant(tree: &Path, folder: bool, entries: Vec<(String, Vec<u8>)>) -> &'static str {
    if !folder {
        cbz_of(&tree.join("pages.cbz"), entries);
        return "pages.cbz";
    }
    let dir = tree.join("pages");
    std::fs::create_dir(&dir).expect("makes the folder");
    for (name, bytes) in entries {
        std::fs::write(dir.join(name), bytes).expect("writes an entry");
    }
    "pages"
}

/// `unit`'s mtime moved a minute past the one its scan recorded.
fn move_mtime_on(unit: &Unit) {
    let later = SystemTime::UNIX_EPOCH
        + Duration::from_secs(u64::try_from(unit.mtime).expect("a recent mtime") + 60);
    File::options()
        .write(true)
        .open(&unit.path)
        .and_then(|file| file.set_modified(later))
        .expect("moves its mtime on");
}

/// `got` is the record `PRINT_FIXTURE` pins for page `i`, field by field.
fn assert_pinned(got: Option<&PagePrint>, i: usize) {
    let got = got.unwrap_or_else(|| panic!("page {i} is hashed"));
    let pinned = &PRINT_FIXTURE[i];
    assert_eq!(got.width, pinned.width, "page {i}'s width");
    assert_eq!(got.height, pinned.height, "page {i}'s height");
    assert_eq!(got.hash.0, pinned.hash, "page {i}'s hash");
    assert_eq!(
        got.halves.map(|(left, right)| (left.0, right.0)),
        pinned.halves,
        "page {i}'s halves"
    );
}

/// A fixture read whole: stored under its digest, stamped with the build and
/// this layout's version, and each record the one `PRINT_FIXTURE` pins.
fn assert_read_as_pinned(tree: &Path, rel_path: &str) {
    let (unit, digest) = scanned(tree, rel_path);
    let row = stored(read(&unit, &digest, &mut |_, _| true));
    assert_eq!(row.sha256, digest, "keyed by its digest");
    assert_eq!(row.version, fingerprint::VERSION, "this layout");
    assert_eq!(row.outcome, Outcome::Whole, "every page hashed");
    assert_eq!(row.reason, None, "nothing to explain");
    assert_eq!(row.build.as_deref(), Some(BUILD), "the build that read it");
    assert_eq!(row.pages, 26, "one per page");
    let prints = decode(&row.prints).expect("the prints read back");
    assert_eq!(prints.len(), PRINT_FIXTURE.len(), "one record per page");
    for (i, print) in prints.iter().enumerate() {
        assert_pinned(print.as_ref(), i);
    }
}

#[test]
fn a_streamed_cbz_matches_the_pinned_fixture() {
    let tree = TempDir::new("read-unit-cbz");
    print_fixture_cbz(&tree);
    assert_read_as_pinned(&tree, "print-fixture.cbz");
}

#[test]
fn a_streamed_folder_matches_the_pinned_fixture() {
    let tree = TempDir::new("read-unit-folder");
    print_fixture_folder(&tree);
    assert_read_as_pinned(&tree, "print-fixture");
}

/// Page 5 is never sampled for the border, page 4 is. A page the decoder
/// refuses keeps its place as a record not decoded, every other page reads as
/// pinned, and the file is partial: in an archive, and in a folder, whose
/// digest holds the refused page's bytes as it holds every page's.
#[test]
fn a_page_that_does_not_decode_keeps_its_place() {
    for (folder, bad) in [(false, 5), (false, 4), (true, 5), (true, 4)] {
        let tree = TempDir::new("read-unit-undecoded");
        let rel_path = plant(&tree, folder, fixture_with_garbage_at(bad));
        let (unit, digest) = scanned(&tree, rel_path);
        let row = stored(read(&unit, &digest, &mut |_, _| true));
        assert_eq!(
            row.outcome,
            Outcome::Partial,
            "{rel_path}: page {bad} refused"
        );
        assert_eq!(row.pages, 26, "{rel_path}: page {bad} refused");
        let name = format!("p{bad:02}.png");
        assert!(
            row.reason.as_deref().is_some_and(|why| why.contains(&name)),
            "the reason names {name}: {:?}",
            row.reason
        );
        let prints = decode(&row.prints).expect("the prints read back");
        assert_eq!(prints.len(), 26, "page {bad} refused");
        for (i, print) in prints.iter().enumerate() {
            if i == bad {
                assert_eq!(*print, None, "page {i} is not decoded");
            } else {
                assert_pinned(print.as_ref(), i);
            }
        }
    }
}

/// Five pages, every one sampled: two with bars of 4 a side, two of 8, and
/// one the decoder refuses. Left out, the refused sample leaves the upper
/// middle of 4, 4, 8, 8, so 8 comes off each side; counted as no border, it
/// would make the middle of 0, 4, 4, 8, 8, and 4 would.
#[test]
fn a_sample_that_does_not_decode_is_left_out_of_the_border() {
    let tree = TempDir::new("read-unit-bad-sample");
    let bars = [Some(4), Some(8), None, Some(4), Some(8)];
    let entries = (0u32..).zip(bars).map(|(seed, bar)| {
        let bytes = match bar {
            Some(bar) => print_page(96, 128, [bar; 4], seed),
            None => b"a png by name only".to_vec(),
        };
        (format!("p{seed:02}.png"), bytes)
    });
    cbz_of(&tree.join("bars.cbz"), entries);
    let (unit, digest) = scanned(&tree, "bars.cbz");
    let row = stored(read(&unit, &digest, &mut |_, _| true));
    assert_eq!(row.outcome, Outcome::Partial, "page 2 refused");
    let prints = decode(&row.prints).expect("the prints read back");
    assert_eq!(prints.len(), 5, "one record per page");
    for (i, (print, bar)) in prints.iter().zip(bars).enumerate() {
        match (print, bar) {
            (None, None) => {}
            (Some(print), Some(_)) => assert_eq!(
                (print.width, print.height),
                (96 - 16, 128 - 16),
                "page {i} less 8 a side"
            ),
            _ => panic!("page {i} is {print:?}"),
        }
    }
}

/// An entry the reader does not read leaves the pages' records as they are
/// and makes the file partial, in an archive and in a folder; a file of
/// nothing else holds no page at all.
#[test]
fn an_unread_entry_makes_a_file_partial() {
    for folder in [false, true] {
        let tree = TempDir::new("read-unit-unread");
        let mut entries = print_fixture_pages();
        entries.push(("p26.jxl".to_owned(), b"a page format not read yet".to_vec()));
        let rel_path = plant(&tree, folder, entries);
        let (unit, digest) = scanned(&tree, rel_path);
        let row = stored(read(&unit, &digest, &mut |_, _| true));
        assert_eq!(
            row.outcome,
            Outcome::Partial,
            "{rel_path}: an entry left unread"
        );
        assert_eq!(row.pages, 26, "{rel_path}: the pages alone");
        assert!(
            row.reason
                .as_deref()
                .is_some_and(|why| why.contains("p26.jxl")),
            "{rel_path}: the reason names p26.jxl: {:?}",
            row.reason
        );
        let prints = decode(&row.prints).expect("the prints read back");
        assert_eq!(prints.len(), 26, "{rel_path}: the pages alone");
        for (i, print) in prints.iter().enumerate() {
            assert_pinned(print.as_ref(), i);
        }
    }

    let tree = TempDir::new("read-unit-unread-only");
    cbz_of(
        &tree.join("jxl.cbz"),
        ["p00.jxl", "p01.jxl"].map(|name| (name.to_owned(), b"not read yet".to_vec())),
    );
    let (unit, digest) = scanned(&tree, "jxl.cbz");
    let row = stored(read(&unit, &digest, &mut |_, _| true));
    assert_eq!(row.outcome, Outcome::Unreadable, "no page");
    assert_eq!(row.pages, 0, "no page");
    assert!(row.prints.is_empty(), "no record");
    for name in ["p00.jxl", "p01.jxl"] {
        assert!(
            row.reason.as_deref().is_some_and(|why| why.contains(name)),
            "the reason names {name}: {:?}",
            row.reason
        );
    }
}

/// Page 13's bytes in the archive lose their checksum. The walk ends there:
/// the pages before it keep their records, it and every page after it are not
/// hashed, and the reader's words are the reason. Page 13 is never sampled,
/// so the border is the fixture's.
#[test]
fn a_read_error_ends_the_walk_at_its_page() {
    let tree = TempDir::new("read-unit-corrupt");
    let path = tree.join("corrupt.cbz");
    let pages = print_fixture_pages();
    let page = pages[13].1.clone();
    cbz_of(&path, pages);
    let mut archive = std::fs::read(&path).expect("reads the archive");
    let at = archive
        .windows(page.len())
        .position(|bytes| bytes == page.as_slice())
        .expect("page 13's bytes are stored as they are");
    archive[at + page.len() / 2] ^= 0xFF;
    std::fs::write(&path, archive).expect("writes the archive back");

    let (unit, digest) = scanned(&tree, "corrupt.cbz");
    let words = taiyaku_read::for_each_member_page(&unit.path, &mut |_, _| Ok(()))
        .expect_err("page 13 does not read");
    let row = stored(read(&unit, &digest, &mut |_, _| true));
    assert_eq!(row.outcome, Outcome::Partial, "13 pages hashed");
    assert_eq!(row.pages, 26, "every page has its record");
    assert_eq!(row.reason, Some(words), "the reader's words");
    let prints = decode(&row.prints).expect("the prints read back");
    assert_eq!(prints.len(), 26, "every page has its record");
    for (i, print) in prints.iter().enumerate() {
        if i < 13 {
            assert_pinned(print.as_ref(), i);
        } else {
            assert_eq!(*print, None, "page {i} is not hashed");
        }
    }
}

#[test]
fn a_file_that_is_not_an_archive_is_unreadable() {
    let tree = TempDir::new("read-unit-noise");
    std::fs::write(
        tree.join("noise.cbz"),
        b"no archive in these bytes; ".repeat(64),
    )
    .expect("writes the file");
    let (unit, digest) = scanned(&tree, "noise.cbz");
    let words = taiyaku_read::member_page_names(&unit.path).expect_err("the reader refuses it");
    let row = stored(read(&unit, &digest, &mut |_, _| true));
    assert_eq!(row.outcome, Outcome::Unreadable, "nothing listed");
    assert_eq!(row.pages, 0, "nothing listed");
    assert!(row.prints.is_empty(), "no record");
    assert_eq!(row.reason, Some(words), "the reader's words");
}

/// Mid-walk the archive takes other bytes and a later mtime: what was read
/// may be neither content, so nothing is stored under the digest.
#[test]
fn a_file_rewritten_during_its_read_stores_nothing() {
    let tree = TempDir::new("read-unit-rewritten");
    print_fixture_cbz(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture.cbz");
    let read = read(&unit, &digest, &mut |phase, i| {
        if (phase, i) == (Phase::Walk, 3) {
            std::fs::write(&unit.path, b"other bytes").expect("rewrites the archive");
            move_mtime_on(&unit);
        }
        true
    });
    assert!(matches!(read, Read::Gone), "{read:?}");
}

/// A folder's own stamp does not move when a page is rewritten in place at
/// the same length, so only the bytes the walk hashed can tell.
#[test]
fn a_folder_whose_page_changes_during_its_read_stores_nothing() {
    let tree = TempDir::new("read-unit-folder-changed");
    let folder = print_fixture_folder(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture");
    let page = folder.join("p10.png");
    let read = read(&unit, &digest, &mut |phase, i| {
        if (phase, i) == (Phase::Walk, 3) {
            let other: Vec<u8> = std::fs::read(&page)
                .expect("reads the page")
                .iter()
                .map(|byte| !byte)
                .collect();
            std::fs::write(&page, other).expect("rewrites the page");
        }
        true
    });
    assert_eq!(
        stamp_of(&unit.path),
        Some((unit.size, unit.mtime)),
        "sanity: the folder's own stamp is as scanned"
    );
    assert!(matches!(read, Read::NotItsDigest(None)), "{read:?}");
}

/// A folder's page replaced, after its scan, by a link to nothing, its own
/// stamp put back: the walk ends on the read error, so its pages hash short of
/// the digest, and the read says the error rather than a rewrite.
#[test]
fn a_folder_whose_walk_ends_on_a_read_error_carries_the_error() {
    let tree = TempDir::new("read-unit-folder-read-error");
    let folder = print_fixture_folder(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture");
    let modified = std::fs::metadata(&folder)
        .and_then(|meta| meta.modified())
        .expect("the folder's mtime");
    let page = folder.join("p10.png");
    std::fs::remove_file(&page).expect("removes the page");
    std::os::unix::fs::symlink(folder.join("nowhere"), &page).expect("links the page to nothing");
    File::open(&folder)
        .and_then(|dir| dir.set_modified(modified))
        .expect("puts the folder's mtime back");
    assert_eq!(
        stamp_of(&unit.path),
        Some((unit.size, unit.mtime)),
        "the fixture: the folder's own stamp is as scanned"
    );

    let read = read(&unit, &digest, &mut |_, _| true);

    let Read::NotItsDigest(Some(why)) = &read else {
        panic!("not the read error: {read:?}");
    };
    assert!(why.contains("p10.png"), "names the page: {why}");
}

/// A file removed or touched after the scan is not read at all, and one
/// removed partway through its read stores nothing: each stays due.
#[test]
fn a_file_gone_before_or_during_its_read_stores_nothing() {
    let tree = TempDir::new("read-unit-gone-before");
    print_fixture_cbz(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture.cbz");
    std::fs::remove_file(&unit.path).expect("removes the file");
    let mut asked = 0;
    let before = read(&unit, &digest, &mut |_, _| {
        asked += 1;
        true
    });
    assert!(matches!(before, Read::Gone), "removed before: {before:?}");
    assert_eq!(asked, 0, "removed before: no page asked about");

    let tree = TempDir::new("read-unit-touched-before");
    print_fixture_cbz(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture.cbz");
    move_mtime_on(&unit);
    let mut asked = 0;
    let touched = read(&unit, &digest, &mut |_, _| {
        asked += 1;
        true
    });
    assert!(matches!(touched, Read::Gone), "touched before: {touched:?}");
    assert_eq!(asked, 0, "touched before: no page asked about");

    let tree = TempDir::new("read-unit-gone-during");
    print_fixture_cbz(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture.cbz");
    let during = read(&unit, &digest, &mut |phase, i| {
        if (phase, i) == (Phase::Walk, 3) {
            std::fs::remove_file(&unit.path).expect("removes the file");
        }
        true
    });
    assert!(matches!(during, Read::Gone), "removed during: {during:?}");
}

/// `between` is asked before each sampled page and then before each walked
/// page is read, and a `false` from either ends the read with nothing stored.
#[test]
fn a_stop_between_pages_stores_nothing() {
    let tree = TempDir::new("read-unit-stop");
    print_fixture_cbz(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture.cbz");

    let mut asked = Vec::new();
    let walk = read(&unit, &digest, &mut |phase, i| {
        asked.push((phase, i));
        (phase, i) != (Phase::Walk, 7)
    });
    assert!(matches!(walk, Read::Stopped), "stopped at page 7: {walk:?}");
    let expected: Vec<(Phase, usize)> = border_sample(26)
        .map(|i| (Phase::Sample, i))
        .chain((0..=7).map(|i| (Phase::Walk, i)))
        .collect();
    assert_eq!(asked, expected, "the pages asked about, in order");

    // Page 7 goes when page 6 is asked about: a walk that read page 7 before
    // asking about it would fail on the missing file rather than stop.
    let tree = TempDir::new("read-unit-stop-folder");
    let folder = print_fixture_folder(&tree);
    let (unit_of_folder, folder_digest) = scanned(&tree, "print-fixture");
    let mut asked = Vec::new();
    let folder_walk = read(&unit_of_folder, &folder_digest, &mut |phase, i| {
        asked.push((phase, i));
        if (phase, i) == (Phase::Walk, 6) {
            std::fs::remove_file(folder.join("p07.png")).expect("removes page 7");
        }
        (phase, i) != (Phase::Walk, 7)
    });
    assert!(
        matches!(folder_walk, Read::Stopped),
        "a folder stopped at page 7, unread: {folder_walk:?}"
    );
    assert_eq!(asked, expected, "the folder's pages asked about, in order");

    let sample = read(&unit, &digest, &mut |phase, i| {
        (phase, i) != (Phase::Sample, 4)
    });
    assert!(
        matches!(sample, Read::Stopped),
        "stopped at sampled page 4: {sample:?}"
    );
}

/// A page added to a folder after the read listed its pages, sorting after
/// them, is never read: the walk ends at the listed count, so no page past
/// it is read unasked. The addition moves the folder's own stamp, so the
/// read stores nothing. Added as page 0 is asked about, before the walk's
/// own listing, which would otherwise hold it.
#[test]
fn a_folder_that_gains_a_page_during_its_read_is_read_no_further() {
    let tree = TempDir::new("read-unit-gains-a-page");
    let folder = print_fixture_folder(&tree);
    let added = print_page(96, 128, [6, 3, 2, 5], 99);
    let mut asked = Vec::new();
    let walked = walk(
        &folder,
        26,
        Border::default(),
        &LargePages::new(LARGE_PAGE_BYTES),
        &mut |phase, i| {
            asked.push((phase, i));
            if i == 0 {
                std::fs::write(folder.join("p99.png"), &added).expect("adds a page");
            }
            true
        },
        None,
    )
    .expect("nothing stops the walk");
    let every_page: Vec<(Phase, usize)> = (0..26).map(|i| (Phase::Walk, i)).collect();
    assert_eq!(walked.prints.len(), 26, "only the listed pages are read");
    assert_eq!(walked.failed, None, "the walk ends as a whole one does");
    assert_eq!(asked, every_page, "the pages asked about");

    let tree = TempDir::new("read-unit-gains-a-page-whole");
    let folder = print_fixture_folder(&tree);
    let (unit, digest) = scanned(&tree, "print-fixture");
    let mut asked = Vec::new();
    let read = read(&unit, &digest, &mut |phase, i| {
        asked.push((phase, i));
        if (phase, i) == (Phase::Walk, 0) {
            std::fs::write(folder.join("p99.png"), &added).expect("adds a page");
            let later = SystemTime::UNIX_EPOCH
                + Duration::from_secs(u64::try_from(unit.mtime).expect("a recent mtime") + 60);
            File::open(&folder)
                .and_then(|dir| dir.set_modified(later))
                .expect("moves the folder's mtime on, as a later addition would");
        }
        true
    });
    assert!(matches!(read, Read::Gone), "{read:?}");
    assert!(
        asked.iter().all(|&(_, i)| i < 26),
        "no page past the listing is asked about: {asked:?}"
    );
}

/// A page at the threshold waits for the lock, in the border
/// sample (page 4) and in the walk (page 5, never sampled), while the pages
/// under it before it went on. A read that decoded either outside the lock
/// would ask about the page after it while the lock is held.
#[test]
fn a_large_page_waits_for_the_lock() {
    for (big, phase, next) in [(4, Phase::Sample, 6), (5, Phase::Walk, 6)] {
        let tree = TempDir::new("read-unit-large");
        let mut pages = print_fixture_pages();
        pages[big].1 = print_page(400, 600, [6, 3, 2, 5], 99);
        let threshold = pages[big].1.len();
        for (i, (_, bytes)) in pages.iter().enumerate() {
            assert!(
                i == big || bytes.len() < threshold,
                "sanity: page {i} is under page {big}'s size"
            );
        }
        cbz_of(&tree.join("large.cbz"), pages);
        let (unit, digest) = scanned(&tree, "large.cbz");
        let large = LargePages::new(threshold);
        let (tell, asked) = mpsc::channel();
        let mut between = move |phase: Phase, i: usize| {
            let _ = tell.send((phase, i));
            true
        };
        std::thread::scope(|scope| {
            // Taken in here so a failed assertion drops it before the scope
            // joins the reader it blocks.
            let held = large.lock.lock().expect("the lock is free");
            let reader = scope.spawn(|| read_unit(&unit, &digest, None, &large, &mut between));
            while asked
                .recv_timeout(Duration::from_secs(10))
                .expect("the read reaches the large page")
                != (phase, big)
            {}
            assert!(
                asked.recv_timeout(Duration::from_millis(300)).is_err(),
                "page {big} waits: nothing asked about page {next} yet"
            );
            drop(held);
            let read = reader.join().expect("the read ends");
            assert!(matches!(read, Read::Stored(_)), "page {big}: {read:?}");
        });
    }
}

/// A file of companions alone holds no page and leaves nothing unread, so it
/// is stored unreadable for holding no page, and the reason says so.
#[test]
fn a_file_of_companions_alone_is_unreadable_for_holding_no_page() {
    let tree = TempDir::new("read-unit-companions-only");
    cbz_of(
        &tree.join("info.cbz"),
        [("ComicInfo.xml".to_owned(), b"<ComicInfo/>".to_vec())],
    );
    let (unit, digest) = scanned(&tree, "info.cbz");
    let row = stored(read(&unit, &digest, &mut |_, _| true));
    assert_eq!(row.outcome, Outcome::Unreadable);
    assert_eq!(row.pages, 0);
    assert_eq!(row.reason.as_deref(), Some("it holds no page"));
}

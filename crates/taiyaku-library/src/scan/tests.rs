use super::*;
use crate::Library;

/// A root holding two series directories, one of them chapter-organised.
///
/// **Members are directories of page images, not `.cbz` files.** Archive
/// reading is `taiyaku-read`'s to test; what this module owns is the
/// dispatch, which
/// `a_container_that_will_not_open_is_recorded_with_its_format_and_no_count`
/// reaches.
///
/// This used to say the choice was *forced* — `taiyaku-library` does not
/// depend on `zip` and must not start, its dependency list being "five
/// entries" that `docs/status.md` quotes. It is **eight**: `notify`, `regex`,
/// `rusqlite`, `serde`, `serde_json`, `sha2`, `taiyaku-core` and
/// `taiyaku-read` (`Cargo.toml`, checked 2026-08-18) — six before this
/// branch, and it was written as five then too. `docs/status.md` names the
/// crates rather than counting them, so nothing there had to move. The
/// dependency claim stands and the
/// conclusion did not: [`write_archive`] writes a stored-method archive from
/// bytes, needing no library at all. That mattered, because "every derive
/// fixture is a directory of page images" meant `pages = 0` never occurred in
/// any test, and the layout this project's own corpus uses — archives inside
/// volume directories — derived to a single page-less work with both archives
/// suppressed (whole-branch review, C1).
///
/// `pub(crate)` for `derive/tests.rs`, which has to see *these* facts rather
/// than a second tree describing the same corpus: two fixtures drift, and the
/// whole point of the derive tests is that both modules read one set of rows.
pub(crate) fn fixture() -> crate::tests::TempDir {
    let dir = crate::tests::tempdir("scan");
    let root = &dir.path();
    let series = root.join("Go-Toubun no Hanayome");
    write_pages(&series.join("Go-Toubun no Hanayome - v01 (2019)"), 3);
    write_pages(&series.join("Go-Toubun no Hanayome - v02 (2019)"), 4);
    let chapters = root.join("Kaguya-sama - Fan Translated");
    write_pages(&chapters.join("Kaguya-sama - c001"), 2);
    // Recorded, not skipped (§3): a format nothing can read. Five bytes of
    // header is all `sniff` looks at.
    std::fs::write(root.join("notes.pdf"), b"%PDF-1.7\n").expect("write");
    dir
}

/// A `.cbz` at `path` holding `pages` stored page entries.
///
/// **Written from bytes rather than through a zip library, and that is what
/// makes it affordable.** `taiyaku-library` depends on five crates and `zip`
/// is not one of them — `docs/status.md` quotes that list — so the choice was
/// between a new dependency and this. A stored-method archive is a fixed
/// 30-byte local header per entry, the entry's name and bytes, a 46-byte
/// central directory record per entry, and a 22-byte end record; nothing here
/// compresses, so nothing here needs a compressor.
///
/// **Why a fixture needed it at all.** Every derive fixture in this
/// repository built its members out of directories of page images, so
/// `scan_entry.pages` was never `0` and no test ever met a container holding
/// archives rather than pages — the shape this project's own corpus uses, and
/// the one where a volume directory took `pages = Some(0)` from the walk,
/// became a member, and suppressed the archive that actually held the pages.
/// Two archives derived to one page-less work (whole-branch review, C1).
///
/// Each page's bytes name the archive holding them, for the reason
/// [`write_pages`] gives: two archives with equal page counts must not hash
/// alike by accident.
///
/// `a_hand_written_archive_lists_its_pages` is what pins this helper. Without
/// it a fixture that produced bytes `taiyaku-read` could not parse would still
/// have reached derivation — as a container the walk refused, which is a
/// different test's scenario and passes for the wrong reason.
pub(crate) fn write_archive(path: &Path, pages: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    let label = path.to_string_lossy().into_owned();
    let mut bytes: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    for page in 0..pages {
        let name = format!("{page:03}.jpg");
        let data = format!("not really a jpeg: {label} page {page}").into_bytes();
        let crc = crc32(&data);
        let at = u32::try_from(bytes.len()).expect("a fixture archive is a few hundred bytes");
        let size = u32::try_from(data.len()).expect("a fixture page is a single line");
        let name_len = u16::try_from(name.len()).expect("`000.jpg` is seven bytes");

        // Local file header: signature, "2.0 needed", no flags, method 0
        // (stored), and a fixed 1980-01-01 timestamp so two runs of this
        // helper produce byte-identical archives.
        bytes.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        bytes.extend_from_slice(&20_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0x0021_u16.to_le_bytes());
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&name_len.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(&data);

        // Central directory record: the same fields, plus the disk and
        // attribute columns and the offset of the header just written.
        directory.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        directory.extend_from_slice(&20_u16.to_le_bytes());
        directory.extend_from_slice(&20_u16.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0x0021_u16.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&name_len.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes());
        directory.extend_from_slice(&0_u32.to_le_bytes());
        directory.extend_from_slice(&at.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }

    let directory_at = u32::try_from(bytes.len()).expect("a fixture archive is small");
    let directory_len = u32::try_from(directory.len()).expect("a fixture directory is small");
    let count = u16::try_from(pages).expect("a fixture archive holds a handful of pages");
    bytes.extend_from_slice(&directory);
    // End of central directory: one disk, every entry on it, and no comment.
    bytes.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&directory_len.to_le_bytes());
    bytes.extend_from_slice(&directory_at.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());

    std::fs::write(path, &bytes).expect("write");
}

/// CRC-32/ISO-HDLC, which is the checksum a zip entry carries.
///
/// Bit by bit rather than through a table: this hashes a few dozen bytes per
/// fixture page, and a 256-entry table would be the larger part of the
/// helper.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn a_hand_written_archive_lists_its_pages() {
    // [`write_archive`] is a zip writer in a test module, so nothing else
    // proves its bytes parse. Without this, an archive `taiyaku-read` could
    // not open would still reach derivation — as a container that would not
    // list, which is `a_container_that_will_not_open…`'s scenario, and every
    // fixture built on it would pass for the wrong reason.
    let tree = crate::tests::tempdir("scan-archive");
    let archive = tree.path().join("Series v01.cbz");
    write_archive(&archive, 3);

    assert_eq!(
        taiyaku_read::member_pages(&archive).expect("the archive lists"),
        3
    );
    let pages = taiyaku_read::read_members_in_order(std::slice::from_ref(&archive))
        .expect("and its pages read back");
    assert_eq!(pages.len(), 3);
    assert!(
        String::from_utf8_lossy(&pages[0].bytes).contains("page 0"),
        "the page bytes survive the round trip"
    );
}

#[test]
fn two_hand_written_archives_of_equal_length_do_not_hash_alike() {
    // The property [`write_pages`] learned the hard way and this helper
    // inherits: two members with the same page count must not be
    // byte-identical, or `content_key` collides and two works silently merge
    // into one row.
    let tree = crate::tests::tempdir("scan-archive-distinct");
    write_archive(&tree.path().join("v01").join("Series v01.cbz"), 3);
    write_archive(&tree.path().join("v02").join("Series v02.cbz"), 3);

    assert_ne!(
        taiyaku_read::member_digest(&tree.path().join("v01").join("Series v01.cbz"))
            .expect("digest"),
        taiyaku_read::member_digest(&tree.path().join("v02").join("Series v02.cbz"))
            .expect("digest"),
    );
}

/// A directory member holding `pages` files `is_page_name` accepts.
///
/// **Each page's bytes name the directory that holds it**, so two members are
/// never byte-identical by accident. The first version wrote the same
/// placeholder into every page, which meant any two volumes with equal page
/// counts hashed to the same member digest, produced the same `content_key`,
/// and **silently merged into one `work` row** — `derive`'s documented
/// duplicated-download case, reached by a fixture rather than by a corpus.
/// It went unnoticed until a Task 8 fixture happened to give two volumes the
/// same length, and then surfaced as a proposal count that was short by one.
///
/// Varying the counts per volume also avoids it, and that is what the fixtures
/// did first — but a helper that is only correct while every caller remembers
/// to vary something is not correct, it is lucky. `lib.rs` makes the same
/// argument about `recursive_triggers`, in the same words.
///
/// A test that *wants* two identical members writes them itself; deliberate
/// duplication should be visible at the call site, not inherited from a
/// helper's placeholder.
///
/// `pub(crate)` alongside [`fixture`], for the same reason.
pub(crate) fn write_pages(dir: &Path, pages: usize) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let label = dir.to_string_lossy().into_owned();
    for page in 0..pages {
        std::fs::write(
            dir.join(format!("{page:03}.jpg")),
            format!("not really a jpeg: {label} page {page}").as_bytes(),
        )
        .expect("write");
    }
}

#[test]
fn a_scan_records_every_entry_with_the_format_its_bytes_say() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.files, 1);
    assert_eq!(report.dirs, 6);
    assert_eq!(report.added, 7);
    assert_eq!(report.refused, None);

    let formats: Vec<(String, Option<String>, Option<i64>)> = conn
        .prepare(
            "SELECT rel_path, format, pages FROM scan_entry WHERE kind = 'file' ORDER BY rel_path",
        )
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");

    assert!(
        formats.iter().any(|(p, f, pages)| p == "notes.pdf"
            && f.as_deref() == Some("pdf")
            && pages.is_none()),
        "a PDF is recorded with its format and no page count, never skipped"
    );

    let dirs: Vec<(String, Option<String>, Option<i64>)> = conn
        .prepare(
            "SELECT rel_path, format, pages FROM scan_entry WHERE kind = 'dir' ORDER BY rel_path",
        )
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");

    assert!(
        dirs.iter().any(|(p, f, pages)| p.ends_with("v01 (2019)")
            && f.as_deref() == Some("dir")
            && *pages == Some(3)),
        "{dirs:?}"
    );
    assert!(
        dirs.iter()
            .any(|(p, _, pages)| p == "Go-Toubun no Hanayome" && *pages == Some(7)),
        "a container directory carries its recursive total, which is what lets \
         derivation pick the deepest directory that is actually a member"
    );
}

#[test]
fn the_page_images_inside_a_directory_member_are_counted_and_not_recorded() {
    // Nine page files exist on disk under this fixture. Recording one row per
    // page would put hundreds of thousands of rows in `scan_entry` for a real
    // library — 156 series of directory members — to hold facts that
    // derivation never reads one at a time. `is_page_name` is the test, and it
    // is the same function `read_edition` uses, so what counts as a page is
    // decided in one place.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(
        report.files, 1,
        "notes.pdf, and none of the nine page images"
    );
    assert_eq!(report.dirs, 6);
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM scan_entry", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 7);
}

#[test]
fn a_second_scan_of_an_unchanged_tree_re_reads_nothing() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("first");

    let report = scan_root(&conn, &root).expect("second");

    assert_eq!(report.added, 0);
    assert_eq!(report.updated, 0, "nothing moved, so nothing was reopened");
    assert_eq!(report.files, 1, "still seen, just not re-read");
}

#[test]
fn a_file_that_has_gone_is_recorded_as_gone_rather_than_left_behind() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("first");

    std::fs::remove_file(tree.path().join("notes.pdf")).expect("remove");
    let report = scan_root(&conn, &root).expect("second");

    assert_eq!(report.gone, 1);
    let left: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path = 'notes.pdf'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(left, 0);
}

#[test]
fn a_root_that_has_become_empty_is_refused_and_nothing_is_applied() {
    // §8.1. An unmounted NFS share, a container started before its bind mount
    // was ready and a renamed directory all produce exactly this scan, and
    // applying it would drop every entry, re-derive to no works, and take the
    // reader's progress and notes with it.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("first");

    std::fs::remove_dir_all(tree.path()).expect("remove");
    std::fs::create_dir_all(tree.path()).expect("recreate");
    let report = scan_root(&conn, &root).expect("second");

    let refusal = report.refused.expect("an empty root is refused");
    assert!(refusal.contains("empty"), "{refusal}");
    let kept: i64 = conn
        .query_row("SELECT count(*) FROM scan_entry", [], |r| r.get(0))
        .expect("count");
    assert_eq!(kept, 7, "nothing was applied");
}

/// `last_scan_at` and `last_scan_status` have existed since migration 1 and
/// nothing had ever written them before this. C4 §3's regime line quotes
/// both.
#[test]
fn a_scan_records_when_it_ran_and_how_it_went() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    scan_root(&conn, &root).expect("scan");

    let (at, status): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads the root row back");
    assert!(at.is_some(), "last_scan_at must be set after a scan");
    assert_eq!(status.as_deref(), Some("ok"));
}

/// Including the outcome nobody wants to write: B §8.1's refusal is a
/// status, not a silence. Same fixture shape as
/// `a_root_that_has_become_empty_is_refused_and_nothing_is_applied`, above —
/// this test is about the two columns that test does not look at.
#[test]
fn a_refused_empty_scan_records_the_refusal_rather_than_nothing() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("first");

    std::fs::remove_dir_all(tree.path()).expect("remove");
    std::fs::create_dir_all(tree.path()).expect("recreate");
    let report = scan_root(&conn, &root).expect("second");

    assert!(report.refused.is_some(), "an empty root is refused");
    let status: Option<String> = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("reads the root row back");
    assert_eq!(status.as_deref(), Some("refused"));
}

#[test]
fn a_symlink_cycle_is_an_error_naming_the_depth_rather_than_a_silent_truncation() {
    let tree = crate::tests::tempdir("cycle");
    let deep = tree.path().join("a");
    std::fs::create_dir_all(&deep).expect("mkdir");
    std::os::unix::fs::symlink(tree.path(), deep.join("loop")).expect("symlink");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    let message = scan_root(&conn, &root).expect_err("a cycle is an error");
    assert!(
        message.contains("nested past 32 directories deep"),
        "{message}"
    );
}

/// IMPORTANT — `scan_root`'s own doc comment calls this "the one case this
/// function tries hardest not to leave silent": an `Err` must still land
/// `last_scan_status = "failed"` on the root row, not `"ok"` and not
/// nothing. Root-safe like the fixture it borrows —
/// `a_symlink_cycle_is_an_error_naming_the_depth_rather_than_a_silent_truncation`,
/// above — a symlink cycle is an error `CAP_DAC_OVERRIDE` cannot bypass the
/// way a permission trick can, so this proves something under CI's own root
/// account too.
#[test]
fn a_failed_scan_records_failed_rather_than_ok_or_nothing() {
    let tree = crate::tests::tempdir("cycle-status");
    let deep = tree.path().join("a");
    std::fs::create_dir_all(&deep).expect("mkdir");
    std::os::unix::fs::symlink(tree.path(), deep.join("loop")).expect("symlink");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    scan_root(&conn, &root).expect_err("a cycle is an error");

    let status: Option<String> = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("reads the root row back");
    assert_eq!(
        status.as_deref(),
        Some("failed"),
        "an `Err` from scan_root must record \"failed\" — not \"ok\", and not a null column"
    );
}

#[test]
fn an_unreadable_directory_is_skipped_counted_and_named_rather_than_failing_the_root() {
    // I4. `lost+found` under a freshly mounted share is `0700`, owned by
    // root — the deployment shape this scanner exists for — so a non-root
    // `taiyaku library scan --add /srv/manga` met it on every mount point
    // and failed the *whole root* over one directory it was never going to
    // be able to read anyway. This module's own stated principle — one
    // unreadable file must not leave a library unscanned — had never
    // actually been applied to a directory before this test existed.
    let tree = fixture();
    let locked = tree.path().join("lost+found");
    std::fs::create_dir_all(&locked).expect("mkdir");
    let mut perms = std::fs::metadata(&locked).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o000);
    std::fs::set_permissions(&locked, perms).expect("chmod");

    // **`chmod 0000` does not stop root**, which holds `CAP_DAC_OVERRIDE` —
    // `watch/tests.rs`'s `a_root_that_cannot_be_written_falls_back_to_the_
    // short_interval` makes the identical guard for the identical reason, and
    // this follows it: measure whether the restriction actually took, and say
    // so rather than passing quietly over nothing.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!(
            "NOT MEASURED: an_unreadable_directory_is_skipped_counted_and_named_rather_than_failing_the_root (this process can list a 0000 directory)"
        );
        return;
    }

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    let report =
        scan_root(&conn, &root).expect("one unreadable directory must not fail the whole root");

    // Restored before anything else reads the tree again — `remove_dir_all`
    // (the tempdir's own `Drop`) needs to list this directory to know it is
    // empty, which `0000` refuses exactly as `read_dir` does above.
    let mut restore = std::fs::metadata(&locked).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut restore, 0o755);
    std::fs::set_permissions(&locked, restore).expect("restore for cleanup");

    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "counted: {:?}",
        report.unreadable_dirs
    );
    assert!(
        report.unreadable_dirs[0].contains("lost+found"),
        "named: {:?}",
        report.unreadable_dirs
    );

    // Sibling to the unreadable directory, and the whole of I4's claim: the
    // rest of the root was scanned, not abandoned.
    assert_eq!(report.files, 1, "notes.pdf, unaffected");
    assert_eq!(report.dirs, 6, "the fixture's own directories, unaffected");
    let go_toubun: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE 'Go-Toubun%'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert!(go_toubun > 0, "the rest of the root was not abandoned");

    // And the unreadable directory itself never entered `scan_entry` at all
    // — there is nothing honest to write for a directory whose contents were
    // never listed, and nothing above claims otherwise.
    let recorded: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path = 'lost+found'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(recorded, 0);
}

#[test]
fn a_page_added_two_levels_down_refreshes_every_ancestors_total() {
    // A directory's mtime does not move when anything happens below it, so
    // an ancestor's recursive total is only correct if the walk rewrites it
    // unconditionally. Measured: adding a page to `series/vol01/` leaves
    // `series` and the root byte-identical with unchanged mtimes.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("first");

    let series = tree.path().join("Go-Toubun no Hanayome");
    std::fs::write(
        series
            .join("Go-Toubun no Hanayome - v01 (2019)")
            .join("003.jpg"),
        b"one more page",
    )
    .expect("write");
    scan_root(&conn, &root).expect("second");

    let total: i64 = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(
        total, 8,
        "the container's total followed the page added below it"
    );
    let root_total: i64 = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE rel_path = ''",
            [],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(
        root_total, 10,
        "every ancestor, not just the immediate one, since the root is one too"
    );
}

#[test]
fn a_container_that_will_not_open_is_recorded_with_its_format_and_no_count() {
    // Reaches `pages_of`'s zip arm, which no other test in this module does
    // — the fixture holds no archive, so an implementation that reopened
    // every container on every scan would pass everything else here. Four
    // bytes of zip signature and nothing behind them: `sniff` answers
    // `Zip`, `member_pages` refuses, and the refusal is swallowed rather
    // than failing the scan.
    let tree = fixture();
    std::fs::write(tree.path().join("truncated.cbz"), b"PK\x03\x04").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    let report = scan_root(&conn, &root).expect("a bad archive is not a failed scan");

    assert_eq!(report.files, 2, "notes.pdf and the truncated archive");
    let (format, pages): (Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT format, pages FROM scan_entry WHERE rel_path = 'truncated.cbz'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select");
    // `pages == None` alone cannot distinguish "took the zip arm and
    // swallowed the refusal" from "never took the zip arm at all" — both
    // give `None`. The `format == "zip"` assertion above is the one that
    // proves `sniff` actually ran; this one only proves the refusal did not
    // propagate.
    assert_eq!(format.as_deref(), Some("zip"));
    assert_eq!(
        pages, None,
        "it opened, it refused, and the scan carried on"
    );
}

#[test]
fn an_unchanged_container_is_not_reopened() {
    // The module's headline claim, made observable. `truncated.cbz` is
    // rewritten with four different bytes of the SAME length and its mtime
    // restored, so the `(size, mtime)` gate reads it as unchanged. `%PDF` is
    // four bytes, one short of the five-byte `%PDF-` signature `from_header`
    // needs, so the rewrite matches no signature at all — if the scan
    // re-sniffed it, `format` would follow the new bytes to NULL, not `pdf`.
    // `std::fs::FileTimes` is stable since 1.75, under this workspace's 1.85.
    let tree = fixture();
    let archive = tree.path().join("truncated.cbz");
    std::fs::write(&archive, b"PK\x03\x04").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("first");

    let before = std::fs::metadata(&archive).expect("metadata");
    std::fs::write(&archive, b"%PDF").expect("rewrite, same length");
    let handle = std::fs::File::options()
        .write(true)
        .open(&archive)
        .expect("open to set times");
    handle
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(before.accessed().expect("atime"))
                .set_modified(before.modified().expect("mtime")),
        )
        .expect("restore mtime");

    let report = scan_root(&conn, &root).expect("second");

    assert_eq!(report.updated, 0, "size and mtime both unmoved");
    let format: Option<String> = conn
        .query_row(
            "SELECT format FROM scan_entry WHERE rel_path = 'truncated.cbz'",
            [],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(
        format.as_deref(),
        Some("zip"),
        "the bytes now match no signature at all, so a re-sniff would write NULL; \
         only a scan that did not reopen the file still says zip"
    );
}

/// **A file's modification time reaches the scan entry, and is the file's
/// own.**
///
/// `mtime_secs` could be replaced with the constant `0`, `1` or `-1` and every
/// test in the workspace still passed — found by the first sharded run of
/// `.forgejo/workflows/mutants.yml` on 2026-08-20, on the forge rather than
/// here.
///
/// **It was recorded as caught by the local sweep, and that was the more
/// interesting half.** Two `derive` tests
/// (`a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress`
/// and `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive`) fail
/// under the mutant when cargo-mutants runs them in its copied tree, and pass
/// under the same mutant applied in place — verified by hand both ways. They
/// are the same pair that raced a one-second clock in CI earlier that week and
/// the pair O56 is written about, so their environment-sensitivity was already
/// known; what is new is that it was **inflating the mutation score**, marking
/// a mutant caught that nothing deliberately catches.
///
/// So this asserts the property directly instead: a known mtime, set on the
/// file rather than waited for, read back through `measure`. No clock, no
/// second-resolution race, no dependence on which directory the tree was
/// copied into.
#[test]
fn a_files_modification_time_reaches_its_scan_entry() {
    let dir = crate::tests::tempdir("scan-mtime");
    // A member is a *directory* of pages, which is why its mtime is the field
    // `unchanged` compares — and why O56 exists: a directory's stamp does not
    // move when the bytes inside an existing child change.
    let volume = dir.path().join("Series").join("Series - v01");
    write_pages(&volume, 2);

    // A fixed instant well clear of both zero and now, so none of the three
    // constants the mutants use can coincide with it: 2001-09-09T01:46:40Z.
    // **Set rather than waited for** — the point of this test is that it does
    // not race a one-second clock the way the two `derive` tests did.
    const STAMPED: i64 = 1_000_000_000;
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(STAMPED as u64);
    std::fs::File::open(&volume)
        .expect("opens the member directory")
        .set_times(std::fs::FileTimes::new().set_modified(when))
        .expect("stamps it");

    let (measured, walk) =
        measure(dir.path(), 0, &Excluder::build(None)).expect("measures the tree");
    assert!(
        walk.unreadable.is_empty(),
        "unexpected unreadable dirs: {:?}",
        walk.unreadable
    );

    let entries = flatten(&measured);
    let member = entries
        .iter()
        .find(|seen| seen.rel_path.ends_with("Series - v01"))
        .unwrap_or_else(|| {
            panic!(
                "the member is not in the walk; saw {:?}",
                entries.iter().map(|e| &e.rel_path).collect::<Vec<_>>()
            )
        });

    assert_eq!(
        member.mtime, STAMPED,
        "the scan entry carries a modification time that is not the member's; a \
         constant here makes every rescan see an unchanged tree, because \
         `unchanged` compares (size, mtime)"
    );
}

/// **The default is off, and that is the whole point of migration 012.**
/// `<root>/.taiyaku/` appeared in a person's media tree without their asking;
/// a root now says so or gets nothing. A root added by `add_root` takes the
/// column's default, so this pins the default rather than any call site.
#[test]
fn a_new_root_keeps_no_documents_by_default() {
    let dir = crate::tests::tempdir("scan-keeps-documents-default");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");

    let root = super::add_root(&conn, dir.path()).expect("adding the root");
    assert!(
        !root.keeps_documents,
        "a root added without being asked keeps no documents: {root:?}"
    );

    let listed = super::roots(&conn).expect("listing roots");
    assert_eq!(listed.len(), 1);
    assert!(
        !listed[0].keeps_documents,
        "and `roots` reads the same answer back off the column, rather than \
         defaulting in Rust where the schema would not agree: {:?}",
        listed[0]
    );
}

/// A root told to keep documents keeps them. Without this the setting could
/// be hard-coded off and every other test in this plan would still pass.
#[test]
fn a_root_can_be_told_to_keep_documents() {
    let dir = crate::tests::tempdir("scan-keeps-documents-on");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let root = super::add_root(&conn, dir.path()).expect("adding the root");

    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("turning documents on");

    let listed = super::roots(&conn).expect("listing roots");
    assert!(listed[0].keeps_documents, "{:?}", listed[0]);
}

// --- add_root_checked ----------------------------------------------------

/// C4 §2: overlapping roots double-count every file underneath, and the
/// symptom — duplicate works — looks nothing like the cause. Both
/// directions, because a parent added second is the same mistake arriving
/// the other way round — see
/// `a_root_containing_an_existing_root_is_refused`, below.
#[test]
fn a_root_inside_an_existing_root_is_refused() {
    let dir = crate::tests::tempdir("scan-add-root-checked-nested-inner");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let outer = super::add_root_checked(&conn, dir.path()).expect("adding the outer root");

    let nested = dir.path().join("inner");
    std::fs::create_dir(&nested).expect("mkdir");
    let refused = super::add_root_checked(&conn, &nested).expect_err("nested root");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "an overlap is this caller's own mistake, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();

    assert!(
        refused.contains("inside"),
        "the refusal must name the overlap: {refused}"
    );
    assert!(
        refused.contains(outer.root.path.to_str().unwrap())
            || refused.contains(&outer.root.id.to_string()),
        "the message must identify the existing root by path or id: {refused}"
    );
}

/// The other direction of the same mistake: a root that would *contain* one
/// already held, added second.
#[test]
fn a_root_containing_an_existing_root_is_refused() {
    let dir = crate::tests::tempdir("scan-add-root-checked-nested-outer");
    let inner = dir.path().join("inner");
    std::fs::create_dir(&inner).expect("mkdir");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let inner_root = super::add_root_checked(&conn, &inner).expect("adding the inner root");

    let refused = super::add_root_checked(&conn, dir.path()).expect_err("containing root");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "an overlap is this caller's own mistake, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();

    assert!(
        refused.contains("contains"),
        "the refusal must name the overlap: {refused}"
    );
    assert!(
        refused.contains(inner_root.root.path.to_str().unwrap())
            || refused.contains(&inner_root.root.id.to_string()),
        "the message must identify the existing root by path or id: {refused}"
    );
}

/// Nesting is a string-prefix question and prefix comparisons on paths are a
/// classic source of false positives: `manga2` is not inside `manga` even
/// though its name starts with it. A raw-byte or raw-string comparison would
/// refuse this pair; [`Path::starts_with`] compares whole components and
/// does not.
#[test]
fn a_sibling_whose_name_merely_starts_with_an_existing_roots_name_is_not_refused() {
    let dir = crate::tests::tempdir("scan-add-root-checked-sibling-prefix");
    let manga = dir.path().join("manga");
    let manga2 = dir.path().join("manga2");
    std::fs::create_dir(&manga).expect("mkdir manga");
    std::fs::create_dir(&manga2).expect("mkdir manga2");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    super::add_root_checked(&conn, &manga).expect("adding manga");

    super::add_root_checked(&conn, &manga2)
        .expect("manga2 is a sibling of manga, not something inside it");
}

/// The nesting check must compare *canonical* paths, not the paths as
/// given, or a symlink resolves into an overlap the raw spelling never
/// mentions. `real_root` is stored as a root under its own name; a second
/// path that reaches `real_root/inner` only by way of a symlink shares not
/// one path component with `real_root` in its raw form, yet is exactly as
/// nested once resolved.
#[test]
fn a_root_reached_only_through_a_symlink_is_still_recognised_as_nested() {
    let dir = crate::tests::tempdir("scan-add-root-checked-symlink-nested");
    let real_root = dir.path().join("real_root");
    std::fs::create_dir(&real_root).expect("mkdir real_root");
    std::fs::create_dir(real_root.join("inner")).expect("mkdir inner");
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&real_root, &alias).expect("symlink");

    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    super::add_root_checked(&conn, &real_root).expect("adding the real root");

    let refused = super::add_root_checked(&conn, &alias.join("inner"))
        .expect_err("nested, reached only through the symlink");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "an overlap is this caller's own mistake, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();

    assert!(
        refused.contains("inside"),
        "the refusal must name the overlap even though the raw path never \
         mentions real_root: {refused}"
    );
}

/// "Readable by the server process" is not the same question as "exists" —
/// on a containerised deployment it is exactly the question that differs.
///
/// **Not `chmod 0000`.** CI for this repository runs as root, which holds
/// `CAP_DAC_OVERRIDE` and can list a `0000` directory anyway
/// (`an_unreadable_directory_is_skipped_counted_and_named_rather_than_failing_the_root`,
/// above, guards against exactly that). A guard that skips when the
/// restriction did not take is not enough on its own here, either: libtest
/// captures *both* stdout and stderr for a passing test, checked directly
/// rather than assumed, so a skip announced on either stream is invisible
/// under a plain `cargo test` — it would report an ordinary `ok` on the CI
/// host every single run, with nothing to show it measured nothing.
///
/// A plain **file** at the path a directory is required at needs no such
/// guard: `std::fs::read_dir` on a non-directory fails with `ENOTDIR`
/// unconditionally, a type mismatch rather than a permission check, so it
/// refuses for root exactly as it does for anyone else — confirmed directly
/// against a `rust:alpine` container running as uid 0, not assumed from
/// `chmod`'s own behaviour.
#[test]
fn a_path_the_process_cannot_read_is_refused_saying_so() {
    let dir = crate::tests::tempdir("scan-add-root-checked-unreadable");
    let closed = dir.path().join("closed");
    std::fs::write(&closed, b"not a directory").expect("planting a file where a directory must be");

    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let refused = super::add_root_checked(&conn, &closed).expect_err("not a directory");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "an unreadable path is this caller's own mistake, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();

    assert!(
        refused.contains("read"),
        "the refusal must say what failed: {refused}"
    );
}

/// C4 §2 distinguishes "does not exist" from "exists but cannot be read" in
/// the shipped error messages. This test pins that distinction so a regression
/// reusing one message for both would be caught.
#[test]
fn a_nonexistent_path_produces_a_distinct_error_from_an_unreadable_path() {
    let dir = crate::tests::tempdir("scan-add-root-checked-nonexistent");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let nonexistent = dir.path().join("does_not_exist");
    let nonexistent_err = super::add_root_checked(&conn, &nonexistent)
        .expect_err("nonexistent path fails canonicalize");
    assert!(
        matches!(nonexistent_err, AddRootError::Unusable(_)),
        "a nonexistent path is this caller's own mistake, not a store fault: {nonexistent_err:?}"
    );

    let unreadable = dir.path().join("unreadable");
    std::fs::write(&unreadable, b"not a directory")
        .expect("planting a file where a directory must be");
    let unreadable_err =
        super::add_root_checked(&conn, &unreadable).expect_err("unreadable path fails read_dir");
    assert!(
        matches!(unreadable_err, AddRootError::Unusable(_)),
        "an unreadable path is this caller's own mistake, not a store fault: {unreadable_err:?}"
    );

    assert_ne!(
        nonexistent_err, unreadable_err,
        "the two failure paths must produce distinct messages: \
         nonexistent={nonexistent_err:?}, unreadable={unreadable_err:?}"
    );
}

/// IMPORTANT 2 (task 11 fix round 1) — a store fault, not any of this
/// function's three documented caller-fault refusals, must answer
/// [`AddRootError::Failed`], not [`AddRootError::Unusable`]. `path` here
/// exists, is readable and overlaps nothing, so every caller-fault check
/// passes; `PRAGMA query_only` then makes the connection itself refuse to
/// write, which is what a disk-full or database-busy failure looks like
/// from inside `add_root` — the insert [`add_root_checked`] runs once its
/// own checks are satisfied.
#[test]
fn a_store_fault_while_adding_a_root_is_reported_as_failed_not_unusable() {
    let dir = crate::tests::tempdir("scan-add-root-checked-store-fault");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.pragma_update(None, "query_only", true)
        .expect("sets query_only");

    let refused = super::add_root_checked(&conn, dir.path()).expect_err("a read-only connection");

    assert!(
        matches!(refused, AddRootError::Failed(_)),
        "a store fault must not be answered as this caller's own mistake: {refused:?}"
    );
}

/// **The fix `mark_derive_failed` exists for** — whole-branch review,
/// Important 1: [`scan_root`] itself only ever sees the walk, and writes
/// `last_scan_status = "ok"` the moment that commits, before the derive half
/// of the job (its own caller's job, per this function's own doc comment)
/// has even started. Nothing here drove the function at all before this
/// test — the row it corrects had no test reading it back.
#[test]
fn mark_derive_failed_corrects_a_status_a_successful_scan_already_wrote() {
    let dir = crate::tests::tempdir("scan-mark-derive-failed");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = super::add_root(&conn, dir.path()).expect("adding the root");
    super::scan_root(&conn, &root).expect("scanning the (empty) root");

    let before: String = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("reading the status the scan itself wrote");
    assert_eq!(
        before, "ok",
        "sanity: the scan must have recorded \"ok\" before this test corrects it"
    );

    super::mark_derive_failed(&conn, root.id).expect("correcting the status");

    let after: String = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("reading the corrected status");
    assert_eq!(
        after, "failed",
        "a derive failure right after a successful scan must correct last_scan_status, \
         not leave the walk's own \"ok\" standing"
    );
}

/// The probe runs on adding, so the regime is known before the first scan
/// rather than discovered later — spec B §8's experiment, at C4 §2's
/// moment.
#[test]
fn adding_a_root_probes_the_watcher_before_the_first_scan() {
    let dir = crate::tests::tempdir("scan-add-root-checked-probe");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let added = super::add_root_checked(&conn, dir.path()).expect("adding the root");

    assert_eq!(
        added.interval,
        crate::watch::interval_for(&added.probe),
        "the interval must follow the probe, not a default"
    );
}

/// **`Added::root` is the exact row this call wrote, not the pre-probe row
/// [`add_root`] handed back before the probe ever ran.** The one test that
/// reads `scan_interval_secs` back — just below — reads it off a fresh
/// [`roots`] query, never off `added.root` itself, so it says nothing about
/// whether `Added::root` agrees with the row it names. Deleting
/// `scan_interval_secs: Some(interval_secs)` from `add_root_checked`'s own
/// `Ok(Added { root: Root { .., ..root }, .. })` leaves `Added::root` holding
/// whatever `add_root` returned before the probe ran — `None`, the fresh
/// row's column default — while the stored row itself still carries the real
/// interval, and the whole suite stayed green.
#[test]
fn adding_a_root_returns_the_same_interval_it_wrote_to_the_row() {
    let dir = crate::tests::tempdir("scan-add-root-checked-returned-interval");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let added = super::add_root_checked(&conn, dir.path()).expect("adding the root");

    assert_eq!(
        added.root.scan_interval_secs,
        Some(added.interval.as_secs() as i64),
        "the returned Added::root must already carry the interval this same call just probed \
         and stored, not the pre-probe default: {added:?}"
    );
}

/// The probe's answer is not just returned — it is what the stored row now
/// says, since a rescan later reads `scan_interval_secs` back off the row
/// rather than off whatever [`Added`] this call happened to return.
#[test]
fn adding_a_root_writes_the_probes_interval_onto_the_stored_row() {
    let dir = crate::tests::tempdir("scan-add-root-checked-probe-persisted");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let added = super::add_root_checked(&conn, dir.path()).expect("adding the root");

    let stored = &super::roots(&conn).expect("listing roots")[0];
    assert_eq!(
        stored.scan_interval_secs,
        Some(added.interval.as_secs() as i64),
        "the row must carry the same interval `Added` reported: {stored:?}"
    );
}

// --- update_root -------------------------------------------------------

/// The trap `rules::update`'s own test names: a patch must change only the
/// field it names, checked against a root where every one of the five
/// nullable columns already holds a distinct, non-default value.
/// `rules::update`'s equivalent test passed while still writing 9 of 11
/// columns unconditionally, because no fixture there ever varied more than
/// two of them — a fixture that leaves four columns at their column default
/// cannot tell "patched only what it named" from "overwrote everything with
/// what a fresh read already held."
#[test]
fn a_root_patch_changes_only_the_field_it_names() {
    let dir = crate::tests::tempdir("scan-root-patch");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let root = super::add_root(&conn, dir.path()).expect("adding the root");
    conn.execute(
        "UPDATE root SET edition_key = 'raw', default_kind = 'official', \
         default_language = 'ja', exclude_globs = '*.tmp', scan_interval_secs = 3600, \
         keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("giving every patchable column its own distinct starting value");

    let changed = super::update_root(
        &conn,
        root.id,
        &super::RootPatch {
            default_kind: Some(Some("fan".to_string())),
            ..Default::default()
        },
    )
    .expect("patching");
    assert!(changed);

    let after = &super::roots(&conn).expect("listing roots")[0];
    assert_eq!(
        after.default_kind.as_deref(),
        Some("fan"),
        "the one column the patch actually named"
    );
    assert_eq!(
        after.edition_key.as_deref(),
        Some("raw"),
        "untouched: edition_key"
    );
    assert_eq!(
        after.default_language.as_deref(),
        Some("ja"),
        "untouched: default_language"
    );
    assert_eq!(
        after.scan_interval_secs,
        Some(3600),
        "untouched: scan_interval_secs"
    );
    assert!(after.keeps_documents, "untouched: keeps_documents");

    assert_eq!(
        after.exclude_globs.as_deref(),
        Some("*.tmp"),
        "untouched: exclude_globs"
    );
}

/// A listed root reports `exclude_globs`, so a client can see what the
/// column holds.
///
/// **O92's readable half.** The column shipped in migration 1 and
/// [`super::update_root`] has always patched it, but `Root` did not carry it
/// and the list query did not select it — so the roots screen's box for it
/// was write-only, starting empty whatever the row held. Applying the pattern
/// to a walk was the rest of O92 and closed on 2026-09-09: `scan_root_inner`
/// compiles the column into an [`super::Excluder`] and `measure_dir` consults
/// it before descending.
///
/// **Both cases, because only the pair pins the read.** A test that checks
/// the patched value alone passes against a query hard-coding any non-`None`
/// constant, and one that checks the fresh root alone passes against a
/// `Root` that never reads the column at all.
#[test]
fn a_listed_root_reports_the_exclude_globs_it_holds() {
    let dir = crate::tests::tempdir("scan-root-exclude-globs");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let root = super::add_root(&conn, dir.path()).expect("adding the root");

    assert_eq!(
        super::roots(&conn).expect("listing roots")[0]
            .exclude_globs
            .as_deref(),
        None,
        "a root nobody has patched excludes nothing"
    );

    super::update_root(
        &conn,
        root.id,
        &super::RootPatch {
            exclude_globs: Some(Some("**/extras/**".to_string())),
            ..Default::default()
        },
    )
    .expect("patching");

    assert_eq!(
        super::roots(&conn).expect("listing roots")[0]
            .exclude_globs
            .as_deref(),
        Some("**/extras/**"),
        "the pattern the patch stored, read back off the listed row"
    );
}

/// `Some(None)` clears a nullable column back to NULL — the only way a root
/// stops naming a default it used to, the same trap `RulePatch`'s own test
/// names.
#[test]
fn clearing_a_nullable_root_column_sets_it_to_null_rather_than_leaving_it_alone() {
    let dir = crate::tests::tempdir("scan-root-patch-clear");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let root = super::add_root(&conn, dir.path()).expect("adding the root");
    conn.execute(
        "UPDATE root SET edition_key = 'raw', default_kind = 'official' WHERE id = ?1",
        [root.id],
    )
    .expect("seeding two nullable columns");

    super::update_root(
        &conn,
        root.id,
        &super::RootPatch {
            edition_key: Some(None),
            ..Default::default()
        },
    )
    .expect("clearing edition_key");

    let after = &super::roots(&conn).expect("listing roots")[0];
    assert_eq!(after.edition_key, None);
    assert_eq!(
        after.default_kind.as_deref(),
        Some("official"),
        "a patch clearing one nullable column must leave a different one exactly where it \
         found it"
    );
}

/// Patching a root that does not exist is `false`, not an error and not a
/// silently created row.
#[test]
fn patching_an_absent_root_reports_it_rather_than_creating_one() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let before = super::roots(&conn).expect("listing roots").len();

    let changed = super::update_root(
        &conn,
        9_999,
        &super::RootPatch {
            keeps_documents: Some(true),
            ..Default::default()
        },
    )
    .expect("patching an id naming no root");

    assert!(!changed);
    assert_eq!(super::roots(&conn).expect("listing roots").len(), before);
}

/// A patch naming nothing at all still has to answer whether `id` exists —
/// `update_root`'s own empty-`SET`-list case, pinned directly.
#[test]
fn an_entirely_empty_root_patch_reports_whether_the_row_exists_without_changing_anything() {
    let dir = crate::tests::tempdir("scan-root-patch-empty");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let root = super::add_root(&conn, dir.path()).expect("adding the root");
    let before = super::roots(&conn).expect("listing roots");

    let changed = super::update_root(&conn, root.id, &super::RootPatch::default())
        .expect("patching with nothing named");

    assert!(changed);
    assert_eq!(super::roots(&conn).expect("listing roots"), before);
}

// --- removal -------------------------------------------------------------

/// C4 §2 calls removal the most destructive action in the product. It states
/// counts before it acts, and the counts are what removal actually costs.
#[test]
fn removing_a_root_reports_the_works_positions_and_notes_it_will_take() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;

    let (predicted, _) = crate::scan::removal_cost(&conn, root).unwrap();
    let (actual, _) = crate::scan::remove_root(&conn, root).unwrap();

    assert_eq!(predicted, actual, "the count stated is not the count taken");
    assert!(predicted.works > 0 && predicted.positions > 0);
}

/// A bare DELETE would leave the works standing with `member.root_id` NULL —
/// which O54 records as the value that excludes a member from *every* root
/// grant. Removal is delete-plus-derive, in one transaction.
#[test]
fn removing_a_root_leaves_no_work_behind_it() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;
    let before = crate::preview::grouping(&conn).unwrap();

    crate::scan::remove_root(&conn, root).unwrap();

    let after = crate::preview::grouping(&conn).unwrap();
    assert!(after.len() < before.len());
    let stranded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM member WHERE root_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stranded, 0,
        "a member with no root is invisible to every grant"
    );
}

/// The other root is untouched — removal is scoped, and the test that would
/// pass while it was not is the one worth writing.
#[test]
fn removing_one_root_leaves_the_other_roots_works_alone() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let roots = crate::scan::roots(&conn).unwrap();
    let kept: Vec<String> = crate::preview::grouping(&conn)
        .unwrap()
        .keys()
        .filter(|p| p.starts_with(roots[0].path.to_str().unwrap()))
        .cloned()
        .collect();

    crate::scan::remove_root(&conn, roots[1].id).unwrap();

    let after = crate::preview::grouping(&conn).unwrap();
    for path in kept {
        assert!(after.contains_key(&path), "{path} went with the other root");
    }
}

/// `remove_root` must send `DELETE FROM root` *inside* an already-open
/// transaction, not as a bare, individually auto-committed statement — the
/// property that makes `apply_removal`'s three deletes and the `derive_with`
/// pass beneath them one atomic unit rather than three separate facts a
/// crash between them could tear apart. No functional test on the happy
/// path this suite otherwise exercises can tell the two implementations
/// apart: a version reduced to a bare sequence of statements against `conn`
/// directly still reaches the identical *committed* end state whenever
/// nothing fails partway, because `derive_with`'s own `import_with` call
/// still opens **a** real transaction of its own the moment nothing wraps it
/// — so counting *whether* a `BEGIN` was sent anywhere is not enough; what
/// distinguishes the two implementations is *when* it was sent relative to
/// `DELETE FROM root`. Traced the same way
/// `committing_the_real_transaction_branch_sends_no_savepoint_statement`
/// (`crate::tests`) pins `Nested`'s own two branches, extended to record
/// order rather than just a count.
#[test]
fn remove_root_sends_delete_from_root_inside_an_already_open_transaction() {
    use std::sync::Mutex;

    static STATEMENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    fn watch(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
            STATEMENTS.lock().unwrap().push(sql.to_string());
        }
    }

    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;

    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(watch),
    );
    crate::scan::remove_root(&conn, root).expect("removing the root");
    conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);

    let statements = STATEMENTS.lock().unwrap();
    let delete_root_index = statements
        .iter()
        .position(|sql| sql.contains("DELETE FROM root"))
        .expect("apply_removal must have sent DELETE FROM root");
    let real_begin_before = statements[..delete_root_index]
        .iter()
        .any(|sql| sql.trim_start().to_ascii_uppercase().starts_with("BEGIN"));

    assert!(
        real_begin_before,
        "DELETE FROM root must be sent after a real BEGIN already opened by remove_root \
         itself — a bare sequence of statements sends it with nothing open yet, and the only \
         BEGIN in the trace would come later, from derive_with's own internal work: {statements:?}"
    );
}

/// `removal_cost` must never actually take anything: it opens a transaction,
/// makes the change, and rolls back. A version that committed instead would
/// turn a cost *estimate* into the destructive action itself — silently, on
/// the call a root editor makes just to print a confirmation dialog.
#[test]
fn removal_cost_leaves_the_database_exactly_as_it_found_it() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;
    let before = crate::preview::grouping(&conn).unwrap();
    let positions_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM progress", [], |r| r.get(0))
        .unwrap();

    let (predicted, _) = crate::scan::removal_cost(&conn, root).unwrap();

    assert!(
        predicted.works > 0,
        "the estimate itself must still see something to take"
    );
    let after = crate::preview::grouping(&conn).unwrap();
    let positions_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM progress", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after, "a cost estimate must not change the library");
    assert_eq!(positions_before, positions_after);
    assert!(
        crate::scan::roots(&conn)
            .unwrap()
            .iter()
            .any(|r| r.id == root),
        "the root itself must still be there after a mere estimate"
    );
}

/// The case `unchecked_transaction` could never survive, and the one Task
/// 11's HTTP route will eventually call `removal_cost` through: a caller
/// that already holds an open transaction. `preview::preview` already proves
/// what a plain `BEGIN` does there — `import::scanned` and `fold::absorb`
/// each refused outright, on the very first work, the moment they tried to
/// open one under `preview`'s own outer transaction — which is exactly why
/// `removal_cost` and `remove_root` were switched to `crate::Nested` instead.
/// This pins the half that mattered most: `Nested`'s rollback path — a
/// `ROLLBACK TO SAVEPOINT` folded back into the caller's still-open
/// transaction, not a real `ROLLBACK` — must still leave the database
/// exactly as it found it, and still report the same counts a standalone
/// call would.
#[test]
fn removal_cost_leaves_the_database_exactly_as_it_found_it_inside_an_open_transaction() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;
    let before = crate::preview::grouping(&conn).unwrap();
    let positions_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM progress", [], |r| r.get(0))
        .unwrap();

    // Ground truth: what `removal_cost` reports with nothing else open,
    // taken before the outer transaction below so this call is standalone.
    let standalone = crate::scan::removal_cost(&conn, root).expect("estimating standalone");

    let outer = conn.unchecked_transaction().expect(
        "opening the outer transaction, so removal_cost below must take Nested's SAVEPOINT \
         branch rather than refuse",
    );

    let nested = crate::scan::removal_cost(&outer, root)
        .expect("removal_cost must not refuse merely because a transaction is already open");

    assert_eq!(
        nested, standalone,
        "the same removal, computed inside an open transaction, must report the same counts a \
         standalone call does"
    );

    outer.commit().expect(
        "committing the outer transaction for real — removal_cost must have left nothing of \
         its own still pending for this to disturb",
    );

    let after = crate::preview::grouping(&conn).unwrap();
    let positions_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM progress", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before, after,
        "a cost estimate nested inside an open transaction must still leave the library \
         untouched once the outer transaction commits"
    );
    assert_eq!(positions_before, positions_after);
    assert!(
        crate::scan::roots(&conn)
            .unwrap()
            .iter()
            .any(|r| r.id == root),
        "the root itself must still be there after a mere estimate"
    );
}

/// `removal_cost` counts only the positions and notes on works the removal
/// actually takes. A version that summed `progress`/`note` over the whole
/// library would report every reader's whole shelf as the cost of removing
/// one root — neither table carries a root, so nothing stops that query from
/// compiling; only the join back through the doomed works' own ids does.
#[test]
fn removal_cost_counts_only_what_the_removed_root_itself_takes() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let roots = crate::scan::roots(&conn).unwrap();
    let root_a_id = roots[0].id;
    let root_b_id = roots[1].id;

    let alpha_work: i64 = conn
        .query_row(
            "SELECT DISTINCT e.work_id
               FROM member m
               JOIN edition e ON e.id = m.edition_id
              WHERE m.root_id = ?1",
            [root_a_id],
            |r| r.get(0),
        )
        .expect("root a's own work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 1)",
        [alpha_work],
    )
    .expect("a position on the root that is staying");
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('note-a', 1, ?1, 'raw', 1, 'root a''s own note', datetime('now'), \
         datetime('now'))",
        [alpha_work],
    )
    .expect("a note on the root that is staying");

    let (removal, _) = crate::scan::removal_cost(&conn, root_b_id).expect("estimating");

    assert_eq!(
        removal.positions, 1,
        "not root a's own position too: {removal:?}"
    );
    assert_eq!(removal.notes, 1, "not root a's own note too: {removal:?}");
}

/// `remove_root` re-derives the whole library, not merely the root it just
/// removed. Material scanned on the *surviving* root and never yet derived
/// is exactly what a version that deleted the doomed works without deriving
/// would leave undiscovered.
#[test]
fn removing_a_root_also_derives_material_the_surviving_root_had_not_yet_had_derived() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let roots = crate::scan::roots(&conn).unwrap();
    let root_a = roots[0].clone();
    let root_b_id = roots[1].id;

    write_archive(&root_a.path.join("Gamma").join("Gamma - v01.cbz"), 2);
    crate::scan::scan_root(&conn, &root_a).expect("scanning root a's new material");
    let before = crate::preview::grouping(&conn).unwrap();
    assert!(
        !before.values().any(|p| p.series == "Gamma"),
        "not derived yet — the point of this test"
    );

    crate::scan::remove_root(&conn, root_b_id).expect("removing root b");

    let after = crate::preview::grouping(&conn).unwrap();
    assert!(
        after.values().any(|p| p.series == "Gamma"),
        "remove_root must derive the whole library, not skip straight to the delete"
    );
}

/// `removal_cost` must derive under `Documents::Skip`, the same reason
/// `preview::preview` does: a cost estimate must not write or rename a
/// document under `<root>/.taiyaku/`, since the `ROLLBACK` around it does not
/// undo a filesystem write.
#[test]
fn removal_cost_touches_no_file_on_disk() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[0].id;
    let before = crate::tests::tree_snapshot(fixture.root_path());

    let (predicted, _) = crate::scan::removal_cost(&conn, root).expect("estimating");

    let after = crate::tests::tree_snapshot(fixture.root_path());
    assert_eq!(
        predicted.works, 1,
        "the fixture's one work is entirely under its one root"
    );
    assert_eq!(
        before, after,
        "a cost estimate must not touch the library tree on disk"
    );
}

/// The byte-identical check above passes even under `Documents::Reconcile`
/// whenever the removed root's own work is the only one left to reconcile —
/// deleting it also deletes the only root reconcile would have walked, so
/// Skip and Reconcile become indistinguishable by accident. This test keeps
/// a *surviving* root in the picture, with a document gap on it — a work
/// marked as owing a document it does not yet have on disk (`hand::reconcile`'s
/// own "retry O82 says does not exist") — so `removal_cost` deriving under
/// `Documents::Reconcile` instead of `Documents::Skip` has something to
/// repair for real, and something this test can catch it doing.
#[test]
fn removal_cost_does_not_repair_a_document_gap_on_the_surviving_root() {
    let dir = crate::tests::tempdir("scan-removal-cost-skip");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    write_archive(
        &dir.path()
            .join("root-a")
            .join("Alpha")
            .join("Alpha - v01.cbz"),
        3,
    );
    write_archive(
        &dir.path()
            .join("root-b")
            .join("Beta")
            .join("Beta - v01.cbz"),
        3,
    );

    let root_a = super::add_root(&conn, &dir.path().join("root-a")).expect("root a");
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root_a.id],
    )
    .expect("root a keeps documents");
    super::scan_root(&conn, &root_a).expect("scanning root a");
    let root_b = super::add_root(&conn, &dir.path().join("root-b")).expect("root b");
    super::scan_root(&conn, &root_b).expect("scanning root b");
    crate::derive::derive(&conn).expect("the first derive");

    let alpha_work: i64 = conn
        .query_row(
            "SELECT DISTINCT e.work_id
               FROM member m
               JOIN edition e ON e.id = m.edition_id
              WHERE m.root_id = ?1",
            [root_a.id],
            |r| r.get(0),
        )
        .expect("root a's own work");
    // A hand edit, so the projection is not empty — an empty one takes
    // `hand::save`'s own "nothing there is the ordinary case" branch and
    // writes nothing regardless of `Documents::Skip` or `Documents::Reconcile`,
    // which would make this test pass for the wrong reason. `save` here
    // actually writes the file and arms the marker; the file is then removed
    // by hand, exactly the O82 gap `hand::reconcile`'s own doc comment
    // describes — present in `migrated_document`, absent on disk.
    let mut doc = crate::librarydoc::of(&conn, alpha_work).expect("projecting the work");
    doc.title = Some("Hand-Set Alpha Title".to_string());
    crate::librarydoc::apply(&conn, alpha_work, &doc).expect("applying the hand edit");
    let saved = crate::hand::save(&conn, alpha_work).expect("saving the hand document");
    let expected_path = saved.path.expect("a kept root's work has an address");
    assert!(
        expected_path.exists(),
        "save must have written the file first"
    );
    std::fs::remove_file(&expected_path).expect("reproducing the gap by hand");
    assert!(!expected_path.exists(), "the gap is the point of this test");

    crate::scan::removal_cost(&conn, root_b.id).expect("estimating root b's removal");

    assert!(
        !expected_path.exists(),
        "removal_cost must not repair a document gap on the surviving root — it must touch \
         no file at all"
    );
}

/// **Whole-branch review, Important 2 — the positive twin of the test just
/// above.** `removal_cost` and `remove_root` both run `apply_removal`, which
/// derives under `Documents::Skip` and so cannot itself have written the
/// repair — proven above by `removal_cost`, whose own transaction always
/// rolls back, leaving the gap exactly as it found it. `remove_root` commits
/// instead, and must actually repair the gap for real: it runs a second,
/// real `Documents::Reconcile` derive afterward (see that function's own
/// doc), so the repair the O82 retry promises still happens on a real
/// removal.
///
/// **The trace additionally proves *when*.** Before this fix, `remove_root`
/// ran the `Reconcile` pass *inside* the same transaction `DELETE FROM root`
/// used, so any `Err` after the repair wrote the file would have rolled the
/// database back while leaving the filesystem moved — the exact invariant
/// `write_work`'s own doc comment states a document rename must never risk.
/// Tracing the same way
/// `remove_root_sends_delete_from_root_inside_an_already_open_transaction`
/// does, this pins that the repair's own derive pass opens its own
/// transaction only *after* `remove_root`'s own `COMMIT` — never before it.
#[test]
fn remove_root_repairs_a_surviving_documents_gap_only_after_its_own_commit() {
    use std::sync::Mutex;

    let dir = crate::tests::tempdir("scan-remove-root-reconcile");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    write_archive(
        &dir.path()
            .join("root-a")
            .join("Alpha")
            .join("Alpha - v01.cbz"),
        3,
    );
    write_archive(
        &dir.path()
            .join("root-b")
            .join("Beta")
            .join("Beta - v01.cbz"),
        3,
    );

    let root_a = super::add_root(&conn, &dir.path().join("root-a")).expect("root a");
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root_a.id],
    )
    .expect("root a keeps documents");
    super::scan_root(&conn, &root_a).expect("scanning root a");
    let root_b = super::add_root(&conn, &dir.path().join("root-b")).expect("root b");
    super::scan_root(&conn, &root_b).expect("scanning root b");
    crate::derive::derive(&conn).expect("the first derive");

    let alpha_work: i64 = conn
        .query_row(
            "SELECT DISTINCT e.work_id
               FROM member m
               JOIN edition e ON e.id = m.edition_id
              WHERE m.root_id = ?1",
            [root_a.id],
            |r| r.get(0),
        )
        .expect("root a's own work");
    // Same gap as the test above: a hand edit so the projection is not
    // empty, saved once so the marker is armed and a file really exists,
    // then removed by hand to reproduce O82's own gap.
    let mut doc = crate::librarydoc::of(&conn, alpha_work).expect("projecting the work");
    doc.title = Some("Hand-Set Alpha Title".to_string());
    crate::librarydoc::apply(&conn, alpha_work, &doc).expect("applying the hand edit");
    let saved = crate::hand::save(&conn, alpha_work).expect("saving the hand document");
    let expected_path = saved.path.expect("a kept root's work has an address");
    std::fs::remove_file(&expected_path).expect("reproducing the gap by hand");
    assert!(!expected_path.exists(), "the gap is the point of this test");

    static STATEMENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    fn watch(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
            STATEMENTS.lock().unwrap().push(sql.to_string());
        }
    }
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(watch),
    );
    crate::scan::remove_root(&conn, root_b.id).expect("removing root b");
    conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);

    assert!(
        expected_path.exists(),
        "remove_root must repair the gap for real, unlike removal_cost's own rollback above"
    );

    let statements = STATEMENTS.lock().unwrap();
    let commit_index = statements
        .iter()
        .position(|sql| sql.trim_start().to_ascii_uppercase().starts_with("COMMIT"))
        .expect("remove_root's own transaction must commit");
    let begin_after_commit = statements[commit_index + 1..]
        .iter()
        .any(|sql| sql.trim_start().to_ascii_uppercase().starts_with("BEGIN"));
    assert!(
        begin_after_commit,
        "the reconcile pass that repairs the gap must open its own transaction after \
         remove_root's own COMMIT, never before it: {statements:?}"
    );
}

/// The pattern language is gitignore's, taken from the crate rather than
/// reimplemented — so this test pins the five behaviours the design chose it
/// *for*, not the crate's whole surface.
///
/// Verified against `ignore` 0.4.33 on 2026-09-09 before it was written here;
/// each row is a behaviour a hand-rolled matcher would have had to be given
/// on purpose, and four of the five are ones `globset` alone cannot express.
#[test]
fn the_pattern_language_is_gitignores() {
    let excluder = super::Excluder::build(Some(
        "**/extras/**\nextras\n*.nfo\n!keep.nfo\n# a comment\n\n/toplevel\nbuild/",
    ));

    for (path, is_dir, expected, why) in [
        (
            "Naruto/extras",
            true,
            true,
            "a bare name matches at any depth",
        ),
        (
            "Naruto/extras/art.jpg",
            false,
            true,
            "and so does what is under it",
        ),
        (
            "a/b/c.nfo",
            false,
            true,
            "a file pattern matches at any depth",
        ),
        ("a/keep.nfo", false, false, "negation re-includes"),
        (
            "toplevel",
            true,
            true,
            "a leading slash anchors, and this is the root's own",
        ),
        (
            "deep/toplevel",
            true,
            false,
            "a leading slash anchors, and this is not",
        ),
        ("build", true, true, "a trailing slash means the directory"),
        ("build", false, false, "and not a file of that name"),
        (
            "Naruto/v01.cbz",
            false,
            false,
            "an ordinary member is untouched",
        ),
    ] {
        assert_eq!(
            excluder.excludes(path, is_dir),
            expected,
            "{path} (dir={is_dir}): {why}"
        );
    }
}

/// A column holding nothing excludes nothing — the path every scan in this
/// repository takes, and the one a matcher that defaulted to matching
/// everything would destroy silently.
#[test]
fn no_pattern_excludes_nothing() {
    for patterns in [None, Some(""), Some("\n\n"), Some("# only a comment")] {
        let excluder = super::Excluder::build(patterns);
        assert!(
            !excluder.excludes("anything/at/all.cbz", false),
            "{patterns:?} must exclude nothing"
        );
        assert!(
            excluder.complaints.is_empty(),
            "{patterns:?} must complain about nothing"
        );
    }
}

/// A line that will not compile disables itself and leaves every other line
/// standing — `rules.rs`'s rule, for its reason: a scan of a real library
/// must not die over one bad pattern.
///
/// **`\` and `a{b` are the two shapes `ignore` 0.4.33 actually rejects**,
/// measured on 2026-09-09. `a[`, `[a-`, `[[:bogus:]]` and `***` all compile,
/// so this test names the two that exist rather than a category that does
/// not.
#[test]
fn a_bad_line_disables_itself_and_nothing_else() {
    let excluder = super::Excluder::build(Some("extras\n\\\n*.nfo"));

    assert_eq!(excluder.complaints.len(), 1, "{:?}", excluder.complaints);
    assert!(
        excluder.complaints[0].contains('\\'),
        "the complaint names the line: {:?}",
        excluder.complaints
    );
    assert!(
        excluder.excludes("a/extras", true),
        "the line before it applies"
    );
    assert!(
        excluder.excludes("a/b.nfo", false),
        "the line after it applies"
    );
}

/// `matched_path_or_any_parents`, which is what `forget_unseen` needs: a
/// stale row under a directory the walk never descended into.
#[test]
fn a_child_of_an_excluded_directory_is_excluded_with_parents() {
    let excluder = super::Excluder::build(Some("extras"));

    assert!(
        !excluder.excludes("Naruto/extras/art.jpg", false),
        "the path itself does not match the pattern"
    );
    assert!(
        excluder.excludes_with_parents("Naruto/extras/art.jpg", false),
        "but its parent does, which is the question forget_unseen asks"
    );
}

/// `check_exclude_globs` is what the PATCH route refuses on, so its two
/// answers are pinned in both directions.
#[test]
fn check_exclude_globs_names_the_line_it_refuses() {
    assert!(super::check_exclude_globs("extras\n*.nfo\n!keep.nfo").is_ok());
    assert!(super::check_exclude_globs("").is_ok());

    let why = super::check_exclude_globs("extras\na{b").expect_err("a{b is unclosed");
    assert!(why.contains("a{b"), "the message names the line: {why}");
}

/// A root whose fixture holds a directory worth excluding and a file worth
/// excluding, beside material that must survive both.
///
/// Built on [`fixture`] rather than beside it, so what "survives" means is
/// the same seven entries every other test in this module already pins.
fn fixture_with_junk() -> crate::tests::TempDir {
    let dir = fixture();
    write_pages(&dir.path().join("Go-Toubun no Hanayome/extras"), 2);
    std::fs::write(dir.path().join("Go-Toubun no Hanayome/series.nfo"), b"junk").expect("write");
    dir
}

/// A root with `exclude_globs` set, ready to scan.
fn root_excluding(conn: &Connection, path: &Path, patterns: &str) -> Root {
    Root {
        exclude_globs: Some(patterns.to_string()),
        ..add_root(conn, path).expect("add")
    }
}

/// An excluded directory is not descended into, and the report names it.
///
/// **Both halves, because either alone passes against a defect.** A test
/// asserting only the report passes against an implementation that names the
/// directory and walks it anyway; a test asserting only the rows passes
/// against one that skips it silently, which is the confession C4 §3 exists
/// to require.
#[test]
fn an_excluded_directory_is_not_walked_and_is_named() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "extras");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(
        report.excluded_dirs,
        vec!["Go-Toubun no Hanayome/extras".to_string()],
        "the directory the pattern named, and only it"
    );
    let under: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE '%extras%'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(under, 0, "nothing under it reached scan_entry");
    assert!(
        report.dirs >= 6,
        "the rest of the tree was still walked: {} dirs",
        report.dirs
    );
}

/// An excluded file is skipped and counted, and the count is not a list.
#[test]
fn an_excluded_file_is_skipped_and_counted() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "*.nfo");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.excluded_files, 1);
    assert!(
        report.excluded_dirs.is_empty(),
        "a file pattern names no directory"
    );
    let held: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE '%.nfo'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(held, 0, "it never became an entry");
}

/// A negation re-includes what its own pattern excluded, through the walk
/// and not merely through the matcher.
#[test]
fn a_negation_re_includes_through_the_walk() {
    let tree = fixture_with_junk();
    std::fs::write(tree.path().join("Go-Toubun no Hanayome/keep.nfo"), b"keep").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "*.nfo\n!keep.nfo");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.excluded_files, 1, "series.nfo, and not keep.nfo");
    let kept: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE '%keep.nfo'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(kept, 1, "the negated line put it back");
}

/// An excluded entry does not move `dir_state.entry_count`.
///
/// The exclusion happens before the counter, so a junk file arriving in a
/// directory is not a change this library reports. Asserted by scanning the
/// same tree twice with the file added in between.
///
/// **Pins the value, not only that it held steady.** `before == after`
/// alone passes against `entry_count` computed by any consistently wrong
/// arithmetic — `*=` leaves it `0` on both scans, `-=` leaves it the same
/// negative number on both (this field is `i64`, so that does not panic) —
/// because "wrong twice, the same way" still satisfies equality. Pinning
/// `before` (and therefore `after`, by the equality below) to the number
/// [`fixture`] actually produces rules both out.
///
/// [`fixture`]'s `Go-Toubun no Hanayome` holds exactly two direct entries —
/// the `- v01 (2019)` and `- v02 (2019)` volume directories, both real
/// directories that `read_dir` returns and neither dotfiles nor (on this
/// first, `*.nfo`-excluding scan) excluded — so a correct `entry_count` for
/// it is `2`.
#[test]
fn an_excluded_file_does_not_move_the_entry_count() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "*.nfo");
    scan_root(&conn, &root).expect("first");

    let before: i64 = conn
        .query_row(
            "SELECT entry_count FROM dir_state WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("reading dir_state");
    assert_eq!(
        before, 2,
        "the two volume directories, and only they, sit directly under it"
    );

    std::fs::write(tree.path().join("Go-Toubun no Hanayome/late.nfo"), b"junk").expect("write");
    scan_root(&conn, &root).expect("second");

    let after: i64 = conn
        .query_row(
            "SELECT entry_count FROM dir_state WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("reading dir_state");
    assert_eq!(before, after, "an excluded arrival is not a change");
}

/// A bad line reaches the report and the good lines still apply.
#[test]
fn a_bad_pattern_is_reported_by_the_scan() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "extras\n\\");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.bad_exclude_patterns.len(), 1, "{report:?}");
    assert_eq!(
        report.excluded_dirs,
        vec!["Go-Toubun no Hanayome/extras".to_string()],
        "the line before the bad one still applied"
    );
}

/// A root with no patterns reports three empty fields rather than omitting
/// them — C4 §3.4's rule, at the layer that produces the numbers.
#[test]
fn a_root_with_no_patterns_reports_the_fields_empty() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");

    let report = scan_root(&conn, &root).expect("scan");

    assert!(report.excluded_dirs.is_empty());
    assert_eq!(report.excluded_files, 0);
    assert!(report.bad_exclude_patterns.is_empty());
}

/// Excluding a page image removes it from its directory's page total, not
/// merely from `scan_entry` — `measure_dir`'s exclusion check runs before
/// `taiyaku_read::is_page_name`, so an excluded page image never reaches the
/// counter that would otherwise have added it to `here`.
///
/// **All four assertions, because any one alone passes against a defect.** A
/// test asserting only `excluded_files` passes against an implementation that
/// counts the excluded file but still tallies its page into `here`; a test
/// asserting only one directory's total cannot see that the parent's
/// recursive sum — which is just the sum of its children's totals — followed
/// the reduction up the tree. `an_excluded_directory_is_not_walked_and_is_named`,
/// above, is the only existing test that touches directory content, and it
/// excludes a whole directory wholesale rather than one page inside one.
#[test]
fn an_excluded_page_image_is_dropped_from_its_directorys_page_total() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "002.jpg");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(
        report.excluded_files, 2,
        "002.jpg exists in both v01 and v02, and a bare gitignore name \
         matches at any depth"
    );
    assert!(
        report.excluded_dirs.is_empty(),
        "a file pattern names no directory"
    );

    let v01_pages: i64 = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE rel_path = \
             'Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019)'",
            [],
            |r| r.get(0),
        )
        .expect("reading v01's pages");
    assert_eq!(
        v01_pages, 2,
        "v01 has three page images on disk; 002.jpg is one of them"
    );

    let v02_pages: i64 = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE rel_path = \
             'Go-Toubun no Hanayome/Go-Toubun no Hanayome - v02 (2019)'",
            [],
            |r| r.get(0),
        )
        .expect("reading v02's pages");
    assert_eq!(
        v02_pages, 3,
        "v02 has four page images on disk; 002.jpg is one of them"
    );

    let series_pages: i64 = conn
        .query_row(
            "SELECT pages FROM scan_entry WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("reading the series directory's pages");
    assert_eq!(
        series_pages, 5,
        "the parent's recursive total is the sum of its children's, so both \
         exclusions above must follow it up the tree — 7 minus the two \
         excluded pages"
    );
}

/// The §8.1 refusal reports what the walk excluded, rather than zeroing it
/// along with the counts it refuses to apply.
///
/// `scan_root_inner` builds the refusal's `ScanReport` as a struct literal
/// with `dirs`, `files`, `added`, `updated` and `gone` all zero — nothing was
/// applied — but `unreadable_dirs`, `excluded_dirs` and `excluded_files` are
/// filled in from `walk`, and `bad_exclude_patterns` from `excluder.complaints`
/// — **not from `walk`, whatever an earlier version of this comment claimed**
/// — because the walk that found nothing to apply still ran and still
/// excluded whatever the pattern named on the way, and the same patterns
/// still failed to compile whether or not the walk they hobbled ever produced
/// anything. An unmounted share looks exactly like a root whose own pattern
/// excludes everything beneath it — both leave the walk with only the root
/// itself — so this pins the combination directly rather than trusting that
/// the two code paths agree.
///
/// **Assertion 5 is the one that matters most.** It proves the refusal
/// protected the library from a pattern that would otherwise have forgotten
/// every entry under the root: `gone` stays `0`, and the rows the first,
/// unexcluded scan wrote are still there after the second, refused one.
///
/// **`bad_exclude_patterns` is pinned too — Minor 4.** `excluding`'s own
/// pattern list carries a second, unparseable line (`a{b`, the exact shape
/// [`check_exclude_globs_names_the_line_it_refuses`] uses) alongside `*`, so
/// this refusal's own `bad_exclude_patterns` has something in it to assert
/// against — setting that field to `Vec::new()` in `scan_root_inner`'s struct
/// literal left every test in this module passing before this addition.
#[test]
fn a_refused_scan_reports_what_it_excluded_and_applies_nothing() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let first = scan_root(&conn, &root).expect("first scan");
    assert_eq!(first.refused, None, "the library holds entries after this");

    // Excludes everything beneath the root, so the walk meets only the root
    // itself — the same shape §8.1 treats as an unmounted share — plus one
    // line that will not compile, so `bad_exclude_patterns` has something to
    // assert.
    let excluding = Root {
        exclude_globs: Some("*\na{b".to_string()),
        ..root
    };
    let report = scan_root(&conn, &excluding).expect("second scan");

    assert!(
        report.refused.is_some(),
        "the walk found nothing under the root but the root itself"
    );
    assert!(
        !report.excluded_dirs.is_empty(),
        "the walk excluded the top-level directories on the way to finding \
         nothing: {report:?}"
    );
    assert!(
        report.excluded_files > 0,
        "the walk excluded notes.pdf on the way to finding nothing: {report:?}"
    );
    assert_eq!(
        report.bad_exclude_patterns.len(),
        1,
        "a{{b will not compile, and only it: {report:?}"
    );
    assert!(
        report.bad_exclude_patterns[0].contains("a{b"),
        "the complaint names the line it refuses: {report:?}"
    );

    assert_eq!(report.gone, 0, "a refusal applies nothing");
    let kept: i64 = conn
        .query_row("SELECT count(*) FROM scan_entry", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        kept, 7,
        "the rows the first scan wrote are untouched by the refusal"
    );
}

/// The §8.1 refusal names `exclude_globs` as a fourth possible cause,
/// always — but only recommends removing the root when the walk that just
/// ran did not actually exclude anything on the way to finding nothing.
/// Important 3: an admin whose own pattern swallowed the whole root a moment
/// ago must not be told the most destructive remedy this product has (C4
/// §2) is the fix.
#[test]
fn the_refusal_offers_removal_only_when_exclusion_did_not_cause_it() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    // Case A: a root that genuinely emptied out. No `exclude_globs` at all —
    // every real file removed from disk, not matched by a pattern.
    let empty_tree = fixture();
    let empty_root = add_root(&conn, empty_tree.path()).expect("add");
    scan_root(&conn, &empty_root).expect("first scan");
    std::fs::remove_dir_all(empty_tree.path().join("Go-Toubun no Hanayome")).expect("rm");
    std::fs::remove_dir_all(empty_tree.path().join("Kaguya-sama - Fan Translated")).expect("rm");
    std::fs::remove_file(empty_tree.path().join("notes.pdf")).expect("rm");
    let genuinely_empty = scan_root(&conn, &empty_root).expect("second scan");
    let message_a = genuinely_empty
        .refused
        .as_deref()
        .expect("the library still holds the first scan's rows");
    assert!(
        message_a.contains("exclude_globs"),
        "exclusion is named as a possible cause even when it was not this one: {message_a}"
    );
    assert!(
        message_a.contains("remove it"),
        "nothing was excluded, so removing the root is still the offered remedy: {message_a}"
    );

    // Case B: the same empty-looking result, but caused by the root's own
    // exclude_globs rather than anything actually vanishing — a second,
    // independent root, so this half does not depend on case A's rows.
    let excluded_tree = fixture();
    let plain = add_root(&conn, excluded_tree.path()).expect("add");
    scan_root(&conn, &plain).expect("first scan");
    let excluding = Root {
        exclude_globs: Some("*".to_string()),
        ..plain
    };
    let excluded_report = scan_root(&conn, &excluding).expect("second scan");
    let message_b = excluded_report
        .refused
        .as_deref()
        .expect("the library still holds the first scan's rows");
    assert!(
        message_b.contains("exclude_globs"),
        "exclusion is named as a possible cause: {message_b}"
    );
    assert!(
        !message_b.contains("remove it"),
        "the patterns did this, so removing the root must not be the advice offered: {message_b}"
    );
}

/// `excluded_everything` (`scan.rs`) reads
/// `!walk.excluded_dirs.is_empty() || walk.excluded_files > 0` — and every
/// other test that reaches it excludes both a directory and a file in the
/// same walk, so `||` and `&&` agree and `> 0` could read `< 0` (always
/// false for a `usize`) without either mutant surviving a `MISSED`.
///
/// This root has no subdirectory at all: `excluded_dirs` is empty and
/// `excluded_files` is the only thing that can make `excluded_everything`
/// true. `&&` with an empty `excluded_dirs` is false, so the mutated build
/// would fall back to "remove it" — the one answer this root's own patterns
/// must not produce.
#[test]
fn a_refusal_caused_by_excluded_files_alone_still_names_exclusion() {
    let tree = crate::tests::tempdir("scan-refusal-files-only");
    std::fs::write(tree.path().join("readme.txt"), b"not a page").expect("write");
    std::fs::write(tree.path().join("notes.pdf"), b"%PDF-1.7\n").expect("write");

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let first = scan_root(&conn, &root).expect("first scan");
    assert_eq!(first.refused, None, "the library holds entries after this");

    // Excludes both plain files and nothing else — there is nothing else on
    // this tree to exclude.
    let excluding = Root {
        exclude_globs: Some("*".to_string()),
        ..root
    };
    let report = scan_root(&conn, &excluding).expect("second scan");

    assert!(
        report.excluded_dirs.is_empty(),
        "this root has no subdirectory for the pattern to exclude: {report:?}"
    );
    assert_eq!(
        report.excluded_files, 2,
        "both plain files were excluded: {report:?}"
    );
    let message = report
        .refused
        .as_deref()
        .expect("the walk found nothing under the root but the root itself");
    assert!(
        message.contains("exclude_globs"),
        "exclusion is named as a possible cause: {message}"
    );
    assert!(
        message.contains("0 directories and 2 files"),
        "the message must name the split the walk actually saw: {message}"
    );
    assert!(
        !message.contains("remove it"),
        "the patterns did this, so removing the root must not be the advice offered: {message}"
    );
}

/// The directory-only twin of
/// [`a_refusal_caused_by_excluded_files_alone_still_names_exclusion`]:
/// `excluded_dirs` is the only thing that can make `excluded_everything`
/// true here, so `> 0` reading `< 0` (always false for `usize`) cannot hide
/// behind `excluded_files` the way it does in every other test that reaches
/// this branch — including
/// `the_refusal_offers_removal_only_when_exclusion_did_not_cause_it`'s case
/// B, whose `*` pattern excludes [`fixture`]'s `notes.pdf` too and so is not
/// a directories-only case.
#[test]
fn a_refusal_caused_by_excluded_directories_alone_still_names_exclusion() {
    let tree = crate::tests::tempdir("scan-refusal-dirs-only");
    write_pages(&tree.path().join("Series One"), 2);
    write_pages(&tree.path().join("Series Two"), 3);

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let first = scan_root(&conn, &root).expect("first scan");
    assert_eq!(first.refused, None, "the library holds entries after this");

    // Excludes both top-level directories and nothing else — there is no
    // plain file on this tree for the pattern to exclude.
    let excluding = Root {
        exclude_globs: Some("*".to_string()),
        ..root
    };
    let report = scan_root(&conn, &excluding).expect("second scan");

    assert_eq!(
        report.excluded_dirs.len(),
        2,
        "both top-level directories were excluded: {report:?}"
    );
    assert_eq!(
        report.excluded_files, 0,
        "this root has no plain file for the pattern to exclude: {report:?}"
    );
    let message = report
        .refused
        .as_deref()
        .expect("the walk found nothing under the root but the root itself");
    assert!(
        message.contains("exclude_globs"),
        "exclusion is named as a possible cause: {message}"
    );
    assert!(
        message.contains("2 directories and 0 files"),
        "the message must name the split the walk actually saw: {message}"
    );
    assert!(
        !message.contains("remove it"),
        "the patterns did this, so removing the root must not be the advice offered: {message}"
    );
}

/// A row a new pattern now covers is counted apart from a file that
/// genuinely disappeared.
///
/// **One test, both assertions, because the pairing is the claim.** A test
/// asserting only `forgotten_by_exclusion` passes against an implementation
/// that counts every stale row there; a test asserting only `gone` passes
/// against one that counts every stale row there, which is what this
/// codebase did before this field existed. Neither number means anything
/// without the other.
/// **Built on [`fixture`] with its own junk, rather than on
/// [`fixture_with_junk`], and excludes `extras/` rather than `extras`.**
/// Neither substitution is cosmetic:
///
/// - `fixture_with_junk`'s `extras` holds two *page images*
///   (`write_pages`), and this module's own walk never gives a page image a
///   `scan_entry` row of its own — `measure_dir` folds it into its parent
///   directory's page total (see
///   `the_page_images_inside_a_directory_member_are_counted_and_not_recorded`).
///   `extras` would therefore hold exactly one stale row, not three, and
///   this test would assert a false count.
/// - a bare `extras` pattern is not `is_only_dir`, so `Excluder`'s `matched`
///   call — the single-path check both `excludes` and the first step of
///   `excludes_with_parents` make — passes regardless of the `is_dir` it is
///   given (`ignore` 0.4.33's own `matched_stripped`: `!glob.is_only_dir() ||
///   is_dir`). A test built on it cannot tell a correct `Held.is_dir` from
///   Task 2's Step 4 mutation, which hard-codes it `false` — the very thing
///   Step 9's table says this test catches. `extras/` is `is_only_dir`, so
///   the directory row's own match — and therefore its landing in
///   `forgotten_by_exclusion` rather than `gone` — turns on `is_dir` being
///   right.
///
/// Two ordinary files stand in for the pages for the same reason: a file
/// under `extras` must hold a `scan_entry` row that cannot match `extras/`
/// by its own name (a directory-only pattern never matches a file, whatever
/// its name), so the only way either row reaches `forgotten_by_exclusion` is
/// through `excludes_with_parents`'s ancestor walk — which is what turns
/// `excludes_with_parents` back into plain `excludes` (Step 9's third row)
/// into a real, observable difference here.
///
/// **A fourth file — plain `extras`, at the root rather than under
/// `Go-Toubun no Hanayome` — pins the mutation the three rows above cannot:
/// `held_for` handing back `is_dir: true` for every row, not just `false`
/// for every row.** `matched_path_or_any_parents` forces `is_dir = true` for
/// every *ancestor* it climbs on the way to a stale descendant, so the
/// directory row and the two files under it match the same way whatever
/// `is_dir` their own `Held` carries — none of the three can tell a forced
/// `true` from a real one. A row with no parent to fall back on can: this
/// file's own name equals `extras/`'s stem, so it is a real test of its own
/// `is_dir`, not an ancestor's. Recorded by the first, patternless scan,
/// then deleted from disk before the second, it is stale for an ordinary
/// reason and never a directory — real `is_dir == false` means the dir-only
/// pattern does not match it, so it belongs in `gone`. Force `is_dir` to
/// `true` and the same pattern claims it, moving it into
/// `forgotten_by_exclusion` and mis-splitting the totals below.
///
/// **A fifth file — a second plain `extras`, under `Kaguya-sama - Fan
/// Translated` rather than at the root — pins the one mutation the fourth
/// file cannot: `==` flipped to `!=`, inverting `is_dir` for every row
/// rather than replacing it with one constant.** With only the fourth file
/// present, inversion moves exactly one row out of each bucket: the
/// directory (real `true` → `false`) stops matching its own name and falls
/// from `forgotten_by_exclusion` into `gone`, while the lone root `extras`
/// file (real `false` → `true`) starts matching and moves the other way —
/// one row leaves each bucket and one enters each, so both totals hold
/// steady and a test built on four files alone passes against the
/// inversion. A second file whose own name is also `extras`, elsewhere on
/// the tree, breaks that balance: it inverts into `forgotten_by_exclusion`
/// exactly as the root one does, so two rows enter that bucket for the one
/// that leaves — `forgotten_by_exclusion` and `gone` both move under `==`
/// → `!=`, not only under a constant `true` or `false`. Recorded by the
/// first, patternless scan and deleted from disk before the second, the
/// same as the fourth file.
#[test]
fn an_excluded_row_is_counted_apart_from_one_that_vanished() {
    let tree = fixture();
    std::fs::create_dir_all(tree.path().join("Go-Toubun no Hanayome/extras")).expect("mkdir");
    std::fs::write(tree.path().join("Go-Toubun no Hanayome/extras/a.txt"), b"a").expect("write");
    std::fs::write(tree.path().join("Go-Toubun no Hanayome/extras/b.txt"), b"b").expect("write");
    // A *file* named `extras`, at the root — a different parent directory
    // than the `extras` above, so the two never collide on disk. `extras/`
    // is dir-only and this is not a directory, so on its real `is_dir` it
    // must never be caught by that pattern.
    std::fs::write(tree.path().join("extras"), b"not a directory").expect("write");
    // A second plain file also named `extras`, under yet another parent —
    // see the doc comment above for why one alone lets `==` → `!=` balance
    // out.
    std::fs::write(
        tree.path().join("Kaguya-sama - Fan Translated/extras"),
        b"also not a directory",
    )
    .expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    // First scan: no patterns, so the extras directory and its two files,
    // notes.pdf, and the two plain files also named extras, are all
    // recorded.
    let plain = add_root(&conn, tree.path()).expect("add");
    let first = scan_root(&conn, &plain).expect("first");
    assert_eq!(first.forgotten_by_exclusion, 0, "nothing was excluded yet");

    // Now exclude `extras/`, and delete `notes.pdf` and both plain `extras`
    // files from disk in the same pass. Three rows go — the directory and
    // its two files — for one reason, and three rows go for the other.
    std::fs::remove_file(tree.path().join("notes.pdf")).expect("rm");
    std::fs::remove_file(tree.path().join("extras")).expect("rm");
    std::fs::remove_file(tree.path().join("Kaguya-sama - Fan Translated/extras")).expect("rm");
    let root = Root {
        exclude_globs: Some("extras/".to_string()),
        ..plain
    };
    let second = scan_root(&conn, &root).expect("second");

    assert_eq!(
        second.gone, 3,
        "notes.pdf and both plain files named extras disappeared, and only they"
    );
    assert_eq!(
        second.forgotten_by_exclusion, 3,
        "the extras directory and its two files, forgotten for a different reason"
    );
}

/// A directory-only pattern must not swallow a plain file of the same name
/// that is still on disk *while the excluding scan itself runs* — Important
/// 2.
///
/// **The gap this closes is at `measure_dir`'s own `excluder.excludes(&child_rel,
/// is_dir)` call, not at `forget_unseen`'s `excludes_with_parents`.**
/// [`an_excluded_row_is_counted_apart_from_one_that_vanished`] above also
/// stages same-named files, but deletes every one of them from disk *before*
/// its excluding scan runs — so that test's own walk never meets a live file
/// this pattern could wrongly swallow, and only `forget_unseen`'s bookkeeping
/// call is exercised. Forcing `measure_dir`'s call to `true` therefore passes
/// every test in this module unchanged; forcing it to `false` is already
/// caught, by that same test's directory assertion. This test's own file
/// sits on disk during the one scan that matters, so `is_dir` at the point
/// the walk actually asks the question has to be right: forced `true`, a
/// directory-only pattern excludes the file exactly as wrongly as it excludes
/// the directory, and the file's own row would never reach `scan_entry` at
/// all.
#[test]
fn a_directory_only_pattern_does_not_swallow_a_present_file_of_the_same_name() {
    let tree = fixture();
    std::fs::create_dir_all(tree.path().join("Go-Toubun no Hanayome/extras")).expect("mkdir");
    std::fs::write(tree.path().join("Go-Toubun no Hanayome/extras/a.txt"), b"a").expect("write");
    // A *file* named `extras`, elsewhere on the tree — on disk for the one
    // scan below, unlike the same-named files in
    // `an_excluded_row_is_counted_apart_from_one_that_vanished`, which are
    // deleted before their own excluding scan runs.
    std::fs::write(
        tree.path().join("Kaguya-sama - Fan Translated/extras"),
        b"not a directory",
    )
    .expect("write");

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, tree.path(), "extras/");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(
        report.excluded_dirs,
        vec!["Go-Toubun no Hanayome/extras".to_string()],
        "the directory-only pattern excludes the directory: {report:?}"
    );
    assert_eq!(
        report.excluded_files, 0,
        "a directory-only pattern must never exclude a plain file of the same name: {report:?}"
    );
    let file_row: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path = \
             'Kaguya-sama - Fan Translated/extras'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(
        file_row, 1,
        "the file must become a scan_entry row, not be silently excluded: {report:?}"
    );
}

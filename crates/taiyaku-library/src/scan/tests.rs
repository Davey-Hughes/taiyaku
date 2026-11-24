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
/// makes it affordable.** A stored-method archive is a fixed 30-byte local
/// header per entry, the entry's name and bytes, a 46-byte central directory
/// record per entry, and a 22-byte end record; nothing here compresses, so
/// nothing here needs a compressor.
///
/// **Why a fixture needed it at all.** It gives a test a container holding
/// archives rather than pages — the shape this project's own corpus uses, and
/// the one where a volume directory takes `pages = Some(0)` from the walk.
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
    write_archive_of(path, pages, "jpg");
}

/// [`write_archive`], with every entry named with `extension` rather than
/// `jpg`.
///
/// O90: an archive whose entries are all a format `taiyaku-read` does not
/// read as a page (`heic`, say) opens cleanly and still has no page count,
/// which is a different fact from an archive that will not open.
pub(crate) fn write_archive_of(path: &Path, pages: usize, extension: &str) {
    write_archive_labelled(path, pages, extension, &path.to_string_lossy());
}

/// [`write_archive_of`], with each page's bytes naming `label` rather than the
/// archive's path, so one tree written under two directories hashes alike.
pub(crate) fn write_archive_labelled(path: &Path, pages: usize, extension: &str, label: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    let mut bytes: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    for page in 0..pages {
        let name = format!("{page:03}.{extension}");
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

/// The reading `root_id`'s row at `rel_path` holds: its `held_sha256`,
/// `held_format`, `held_pages` and `held_title`, in that order.
pub(crate) fn held_reading(
    conn: &Connection,
    root_id: i64,
    rel_path: &str,
) -> (Option<String>, Option<String>, Option<i64>, Option<String>) {
    conn.query_row(
        "SELECT held_sha256, held_format, held_pages, held_title
           FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root_id, rel_path],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
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
    // The property [`write_pages`] has and this helper inherits: two members
    // with the same page count must not be byte-identical, or `content_key`
    // collides and two works silently merge into one row.
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
/// never byte-identical by accident. Two volumes with equal page counts would
/// otherwise hash to the same member digest, produce the same `content_key`,
/// and **silently merge into one `work` row** — `derive`'s documented
/// duplicated-download case, reached by a fixture rather than by a corpus.
///
/// Varying the counts per volume would avoid it too, but a helper that is only
/// correct while every caller remembers to vary something is not correct, it
/// is lucky. `lib.rs` makes the same argument about `recursive_triggers`, in
/// the same words.
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
    // page would put a row for every page image of a real library in
    // `scan_entry`, to hold facts that derivation never reads one at a time.
    // `is_page_name` is the test, and it is the same function `read_edition`
    // uses, so what counts as a page is decided in one place.
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

/// C4 §3's regime line quotes `last_scan_at` and `last_scan_status`.
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

/// Including the outcome nobody wants to write: B §8.1's refusal is a status,
/// not a silence. Same fixture shape as
/// `a_root_that_has_become_empty_is_refused_and_nothing_is_applied`, above —
/// this test is about the `last_scan_status` column that test does not look at.
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

/// `scan_root`'s own doc comment calls this "the one case this function tries
/// hardest not to leave silent": an `Err` must still land
/// `last_scan_status = "failed"` on the root row, not `"ok"` and not nothing.
/// Root-safe like the fixture it borrows —
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
    // `taiyaku library scan --add /srv/manga` meets it on every mount point,
    // and one directory it can never read must not fail the *whole root*.
    // This module's own stated principle is that one unreadable file must not
    // leave a library unscanned.
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
    // an ancestor's recursive total is only correct if the walk rewrites
    // it unconditionally.
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
    // Reaches `pages_of`'s zip arm, which the fixture alone does not: it
    // holds no archive. Four bytes of zip signature and nothing behind them:
    // `sniff` answers `Zip`, `member_pages` refuses, and the refusal is
    // swallowed rather than failing the scan.
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
    // `std::fs::FileTimes` is stable since 1.75, below the workspace's rust-version.
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
/// Replacing `mtime_secs` with the constant `0`, `1` or `-1` must fail a test,
/// so this asserts the property directly: a known mtime, set on the file
/// rather than waited for, read back through `measure`. No clock, no
/// second-resolution race.
#[test]
fn a_files_modification_time_reaches_its_scan_entry() {
    let dir = crate::tests::tempdir("scan-mtime");
    // A member is a *directory* of pages, which is why its mtime is the field
    // `due` compares — and why O56 exists: a directory's stamp does not
    // move when the bytes inside an existing child change.
    let volume = dir.path().join("Series").join("Series - v01");
    write_pages(&volume, 2);

    // A fixed instant well clear of both zero and now, so none of the three
    // constants the mutants use can coincide with it: 2001-09-09T01:46:40Z.
    // **Set rather than waited for** — the point of this test is that it does
    // not race a one-second clock.
    const STAMPED: i64 = 1_000_000_000;
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(STAMPED as u64);
    std::fs::File::open(&volume)
        .expect("opens the member directory")
        .set_times(std::fs::FileTimes::new().set_modified(when))
        .expect("stamps it");

    let (measured, walk) = measure(
        dir.path(),
        0,
        &Excluder::build(None),
        &HashSet::new(),
        &Unwatched,
    )
    .expect("measures the tree");
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
         `due` compares (size, mtime)"
    );
}

/// **The default is off, and that is the whole point of migration 012.**
/// `<root>/.taiyaku/` must not appear in a person's media tree without their
/// asking: a root says so or gets nothing. A root added by `add_root` takes
/// the column's default, so this pins the default rather than any call site.
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
/// be hard-coded off.
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

thread_local! {
    /// The connection [`removing_at_the_first_row`] removes a directory on,
    /// and the directory's id: a trace callback is a `fn`, which captures
    /// nothing.
    static REMOVER: std::cell::RefCell<Option<(Library, i64)>> =
        const { std::cell::RefCell::new(None) };
}

/// A trace callback that removes [`REMOVER`]'s directory, on its own
/// connection, at the first row the traced connection reads: after that
/// read's snapshot is taken, and before any later statement takes its own.
fn removing_at_the_first_row(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Row(_) = event
        && let Some((other, id)) = REMOVER.with(|remover| remover.borrow_mut().take())
    {
        remove_root(&other.conn(), id).expect("the other connection removes the directory");
    }
}

/// A directory removed while `roots` reads is listed with its libraries, or
/// not at all, never in none: B removes `x` at the first row A reads, so a
/// second statement reading a snapshot of its own would find `x` with no
/// link.
#[test]
fn roots_reads_the_directories_and_their_links_from_one_snapshot() {
    let top = crate::tests::tempdir("scan-roots-one-snapshot");
    let (x, y) = (top.path().join("x"), top.path().join("y"));
    std::fs::create_dir_all(&x).expect("mkdir");
    std::fs::create_dir_all(&y).expect("mkdir");
    let db = top.path().join("library.db");
    let a = Library::open(&db).expect("the first connection");
    let gone = add_root_checked_in(&a.conn(), &x, InLibrary::Named("X"))
        .expect("X holds x")
        .root;
    let kept = add_root_checked_in(&a.conn(), &y, InLibrary::Named("Y"))
        .expect("Y holds y")
        .root;
    let b = Library::open(&db).expect("a second connection on the same file");
    REMOVER.with(|remover| *remover.borrow_mut() = Some((b, gone.id)));

    let mut conn = a.conn();
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_ROW,
        Some(removing_at_the_first_row),
    )
    .expect("installs the trace");
    let listed = roots(&conn).expect("lists");
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::empty(),
        None::<fn(rusqlite::trace::TraceEvent<'_>)>,
    )
    .expect("clears the trace");

    assert!(
        REMOVER.with(|remover| remover.borrow().is_none()),
        "the fixture: B removed x while A read"
    );
    assert_eq!(listed, [gone, kept.clone()], "the state before the removal");
    assert_eq!(
        roots(&conn).expect("lists"),
        [kept],
        "and x is gone after it"
    );
}

thread_local! {
    /// What [`at_the_scan_write`] runs, once: a trace callback is a `fn`,
    /// which captures nothing.
    static AT_THE_WRITE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// A trace callback that runs [`AT_THE_WRITE`] as the traced connection sends
/// `BEGIN IMMEDIATE`: after its walk has read what the directory held, and
/// before its write transaction takes the lock, which is where a scan on
/// another connection can commit between the two.
fn at_the_scan_write(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event
        && sql.trim_start().starts_with("BEGIN IMMEDIATE")
        && let Some(write) = AT_THE_WRITE.with(|cell| cell.borrow_mut().take())
    {
        write();
    }
}

/// Scan `root` on `second`, with `first` scanning it on its own connection
/// at [`at_the_scan_write`], then `then` run there. Answers the second scan's
/// result and the first's, which a trace callback cannot hand back by
/// failing: a panic there would be the fixture failing unseen.
fn scan_overlapped(
    first: Library,
    second: &Library,
    root: &Root,
    then: impl FnOnce(&Connection) + 'static,
) -> (Result<ScanReport, String>, Result<ScanReport, String>) {
    let firsts = std::rc::Rc::new(std::cell::RefCell::new(None));
    let landed = std::rc::Rc::clone(&firsts);
    let walked = root.clone();
    AT_THE_WRITE.with(|cell| {
        *cell.borrow_mut() = Some(Box::new(move || {
            let conn = first.conn();
            let report = scan_root(&conn, &walked);
            then(&conn);
            *landed.borrow_mut() = Some(report);
        }));
    });
    let mut conn = second.conn();
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(at_the_scan_write),
    )
    .expect("installs the trace");
    let seconds = scan_root(&conn, root);
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::empty(),
        None::<fn(rusqlite::trace::TraceEvent<'_>)>,
    )
    .expect("clears the trace");
    let firsts = firsts
        .borrow_mut()
        .take()
        .expect("the fixture: the first scan ran inside the second's");
    (seconds, firsts)
}

fn status_of(conn: &Connection, root: &Root) -> Option<String> {
    conn.query_row(
        "SELECT last_scan_status FROM root WHERE id = ?1",
        [root.id],
        |r| r.get(0),
    )
    .expect("reads the status")
}

/// The server's scheduled scan and a CLI `library scan` can walk one
/// directory at once. Both read the directory's entries before either
/// writes, so each finds every file new. The second to commit must record
/// after the first, not fail a new entry on `UNIQUE (root_id, rel_path)` and
/// leave the directory reading `failed`.
#[test]
fn a_scan_that_overlaps_another_of_its_directory_records_after_it_and_reads_ok() {
    let tree = fixture();
    let store = crate::tests::tempdir("scan-overlap-store");
    let db = store.path().join("library.db");
    let first = Library::open(&db).expect("the first connection");
    let second = Library::open(&db).expect("a second connection on the same file");
    let root = add_root(&second.conn(), tree.path()).expect("add");

    let (seconds, firsts) = scan_overlapped(first, &second, &root, |_| {});

    let firsts = firsts.expect("the first scan records the directory");
    let seconds = seconds.expect("the second scan records after it, without a UNIQUE failure");
    let conn = second.conn();
    let rows: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE root_id = ?1",
            [root.id],
            |r| r.get(0),
        )
        .expect("counts the entries");
    let rows = usize::try_from(rows).expect("a count");
    assert_eq!(
        firsts.added, rows,
        "the fixture: the first scan added every entry"
    );
    assert_eq!(
        (seconds.added, seconds.updated, seconds.dirs + seconds.files),
        (0, 0, rows),
        "the second scan sees the first's entries and touches each, adding none"
    );
    assert_eq!(status_of(&conn, &root).as_deref(), Some("ok"));
}

/// The same overlap over a file that moved: the first scan records its new
/// size, and its derive then hashes it. The second walked before either, so
/// it would write the size again and clear the digest with it. Read again
/// under the write lock, the entry matches what the second scan saw, so it
/// only touches it.
#[test]
fn a_scan_that_overlaps_another_keeps_the_entry_and_digest_the_first_wrote() {
    let tree = fixture();
    let store = crate::tests::tempdir("scan-overlap-moved-store");
    let db = store.path().join("library.db");
    let first = Library::open(&db).expect("the first connection");
    let second = Library::open(&db).expect("a second connection on the same file");
    let root = add_root(&second.conn(), tree.path()).expect("add");
    scan_root(&second.conn(), &root).expect("the scan before the overlap");
    std::fs::write(tree.path().join("notes.pdf"), b"%PDF-1.7\nlonger now\n").expect("rewrite");

    let (seconds, firsts) = scan_overlapped(first, &second, &root, |conn| {
        // Stands for the first scan's derive, which hashes what it reads.
        conn.execute(
            "UPDATE scan_entry SET sha256 = 'hashed by the first' WHERE rel_path = 'notes.pdf'",
            [],
        )
        .expect("writes the digest");
    });

    let firsts = firsts.expect("the first scan records the move");
    assert_eq!(
        firsts.updated, 1,
        "the fixture: the first scan records the move"
    );
    let seconds = seconds.expect("the second scan records after it");
    assert_eq!(
        seconds.updated, 0,
        "the second scan finds the size it saw already recorded"
    );
    let conn = second.conn();
    let digest: Option<String> = conn
        .query_row(
            "SELECT sha256 FROM scan_entry WHERE rel_path = 'notes.pdf'",
            [],
            |r| r.get(0),
        )
        .expect("reads the digest");
    assert_eq!(digest.as_deref(), Some("hashed by the first"));
    assert_eq!(status_of(&conn, &root).as_deref(), Some("ok"));
}

// --- the held reading (O320) ---------------------------------------------

/// The file the held-reading tests write, change and scan again.
const REL: &str = "Alpha/Alpha v01.cbz";

/// A directory holding [`REL`] at `pages` pages, added and scanned once.
fn scanned_alpha(label: &str, pages: usize) -> (crate::tests::TempDir, Library, Root) {
    let tree = crate::tests::tempdir(label);
    write_archive(&tree.path().join(REL), pages);
    let library = Library::open_in_memory().expect("open");
    let root = {
        let conn = library.conn();
        let root = add_root(&conn, tree.path()).expect("add");
        scan_root(&conn, &root).expect("the first scan");
        root
    };
    (tree, library, root)
}

/// [`scanned_alpha`] at 3 pages and derived, then rewritten at 4 pages and
/// scanned again; with the digest the derive took, which that walk replaced.
fn changed_alpha(label: &str) -> (crate::tests::TempDir, Library, Root, String) {
    let (tree, library, root) = scanned_alpha(label, 3);
    let digest = {
        let conn = library.conn();
        crate::derive::derive(&conn).expect("derives");
        let digest = digest_at(&conn, &root, REL).expect("the fixture: the derive hashed it");
        assert_eq!(
            held_reading(&conn, root.id, REL),
            (None, None, None, None),
            "the fixture: nothing is held before the change"
        );
        rescan_rewritten(&conn, &tree, &root, 4);
        digest
    };
    (tree, library, root, digest)
}

/// [`REL`] written again at `pages` pages, which moves its size, and scanned.
fn rescan_rewritten(conn: &Connection, tree: &crate::tests::TempDir, root: &Root, pages: usize) {
    write_archive(&tree.path().join(REL), pages);
    assert_eq!(
        scan_root(conn, root).expect("scans the change").updated,
        1,
        "the fixture: the walk sees the file changed"
    );
}

/// `root`'s row at [`REL`]: its own digest and page count.
fn own_reading(conn: &Connection, root: &Root) -> (Option<String>, Option<i64>) {
    conn.query_row(
        "SELECT sha256, pages FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root.id, REL],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("the row")
}

/// The walk clears a changed entry's digest, and keeps what it clears with
/// the format and page count read beside it: the reading the works were
/// written from, which a paused directory's derive reads until Scan.
#[test]
fn a_changed_entry_holds_the_reading_its_digest_was_taken_from() {
    let (_tree, library, root, d) = changed_alpha("held-changed");
    let conn = library.conn();

    assert_eq!(
        held_reading(&conn, root.id, REL),
        (Some(d), Some("zip".into()), Some(3), None)
    );
    assert_eq!(
        own_reading(&conn, &root),
        (None, Some(4)),
        "the entry reads as the walk found it, to be hashed again"
    );
}

/// A book's work is named by its `dc:title`, so the title read with the
/// digest is held with it, not the one the changed file now declares: a
/// paused directory's book keeps its name.
#[test]
fn a_changed_book_holds_the_title_its_digest_was_read_with() {
    let tree = crate::tests::tempdir("held-book");
    let series = tree.path().join("Story");
    std::fs::create_dir_all(&series).expect("mkdir");
    let book = "Story/Story v01.epub";
    let write = |title: &str, paragraphs: &[&str]| {
        taiyaku_read::test_support::epub_with(
            &series,
            "Story v01.epub",
            &format!("<dc:title>{title}</dc:title>"),
            paragraphs,
        );
    };
    write("The Story", &["One.", "Two.", "Three."]);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("the first scan");
    crate::derive::derive(&conn).expect("derives");
    let d = digest_at(&conn, &root, book).expect("the fixture: the derive hashed it");

    write("Another Story", &["One.", "Two."]);
    assert_eq!(
        scan_root(&conn, &root).expect("scans the change").updated,
        1,
        "the fixture: the walk sees the book changed"
    );
    let title: Option<String> = conn
        .query_row(
            "SELECT title FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, book],
            |r| r.get(0),
        )
        .expect("the row");
    assert_eq!(
        title.as_deref(),
        Some("Another Story"),
        "the fixture: the walk read the new title"
    );

    assert_eq!(
        held_reading(&conn, root.id, book),
        (
            Some(d),
            Some("epub-reflow".into()),
            Some(3),
            Some("The Story".into())
        )
    );
}

/// A digest taken after the change and never written (a cancelled derive
/// leaves one) is not what the works hold, so a second change keeps the
/// reading held already rather than that one.
#[test]
fn a_held_reading_survives_a_second_change() {
    let (tree, library, root, d) = changed_alpha("held-twice");
    let conn = library.conn();
    assert_eq!(
        conn.execute(
            "UPDATE scan_entry SET sha256 = ?3 WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, REL, "e".repeat(64)],
        ),
        Ok(1),
        "the fixture: the digest a cancelled derive leaves"
    );

    rescan_rewritten(&conn, &tree, &root, 5);

    assert_eq!(
        held_reading(&conn, root.id, REL),
        (Some(d), Some("zip".into()), Some(3), None)
    );
}

/// A walk that finds the entry unchanged leaves what it holds alone.
#[test]
fn an_unchanged_entry_keeps_its_held_reading() {
    let (_tree, library, root, d) = changed_alpha("held-unchanged");
    let conn = library.conn();
    let held = held_reading(&conn, root.id, REL);
    assert_eq!(held.0, Some(d), "the fixture: a reading is held");

    assert_eq!(
        scan_root(&conn, &root).expect("scans again").updated,
        0,
        "the fixture: nothing changed"
    );

    assert_eq!(held_reading(&conn, root.id, REL), held);
}

/// The pages held are those read with the digest held. An entry changed
/// before any digest was taken holds nothing; once one is taken, the next
/// change holds it with the page count of its own reading.
#[test]
fn a_reading_is_held_with_its_own_page_count() {
    let (tree, library, root) = scanned_alpha("held-own-pages", 3);
    let conn = library.conn();
    assert_eq!(
        own_reading(&conn, &root),
        (None, Some(3)),
        "the fixture: never hashed"
    );

    rescan_rewritten(&conn, &tree, &root, 4);
    assert_eq!(
        held_reading(&conn, root.id, REL),
        (None, None, None, None),
        "no digest was replaced"
    );

    let taken = "e".repeat(64);
    assert_eq!(
        conn.execute(
            "UPDATE scan_entry SET sha256 = ?3 WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, REL, taken],
        ),
        Ok(1),
        "the fixture: the digest taken at 4 pages"
    );
    rescan_rewritten(&conn, &tree, &root, 5);

    assert_eq!(
        held_reading(&conn, root.id, REL),
        (Some(taken), Some("zip".into()), Some(4), None)
    );
}

// --- add_root_checked ----------------------------------------------------

/// Overlapping-libraries spec §4's W: a directory inside one of the library it
/// joins is refused, as both directories' settings would govern its files.
/// Both directions, because a parent added second is the same mistake arriving
/// the other way round — see
/// `a_root_containing_a_root_of_its_own_library_is_refused`, below.
#[test]
fn a_root_inside_a_root_of_its_own_library_is_refused() {
    let dir = crate::tests::tempdir("scan-add-root-checked-nested-inner");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let outer = super::add_root_checked_in(&conn, dir.path(), InLibrary::Named("Manga"))
        .expect("adding the outer root");

    let nested = dir.path().join("inner");
    std::fs::create_dir(&nested).expect("mkdir");
    let refused =
        super::add_root_checked_in(&conn, &nested, InLibrary::Id(outer.root.libraries[0]))
            .expect_err("nested root");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "an overlap is this caller's own mistake, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();

    assert_eq!(
        refused,
        format!(
            "{} is inside {}, which is already a directory of the same library, `Manga`",
            nested.canonicalize().unwrap().display(),
            outer.root.path.display()
        ),
        "the refusal names the directory and its library, not an id"
    );
}

/// The other direction of the same mistake: a root that would *contain* one
/// of its library's, added second. By name, which finds the library inside
/// the add's transaction.
#[test]
fn a_root_containing_a_root_of_its_own_library_is_refused() {
    let dir = crate::tests::tempdir("scan-add-root-checked-nested-outer");
    let inner = dir.path().join("inner");
    std::fs::create_dir(&inner).expect("mkdir");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    super::add_root_checked_in(&conn, &inner, InLibrary::Named("Manga"))
        .expect("adding the inner root");

    let refused = super::add_root_checked_in(&conn, dir.path(), InLibrary::Named("Manga"))
        .expect_err("containing root");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "an overlap is this caller's own mistake, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();

    assert_eq!(
        refused,
        format!(
            "{} holds `inner`, which is already a directory of the same library, `Manga`",
            dir.path().canonicalize().unwrap().display(),
        ),
        "the refusal names the held directory from inside the chosen one, and its library, not an id"
    );
}

/// The held directory is named by its path from the chosen one, every component
/// of it: a last component alone would not say where under the chosen directory
/// it is.
#[test]
fn a_containing_roots_refusal_names_a_deeper_held_dir_by_its_whole_relative_path() {
    let dir = crate::tests::tempdir("scan-add-root-checked-nested-deeper");
    let inner = dir.path().join("a").join("b");
    std::fs::create_dir_all(&inner).expect("mkdir");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    super::add_root_checked_in(&conn, &inner, InLibrary::Named("Manga"))
        .expect("adding the inner root");

    let refused = super::add_root_checked_in(&conn, dir.path(), InLibrary::Named("Manga"))
        .expect_err("containing root");

    assert_eq!(
        refused.to_string(),
        format!(
            "{} holds `a/b`, which is already a directory of the same library, `Manga`",
            dir.path().canonicalize().unwrap().display(),
        ),
    );
}

/// A directory picked again is refused as already held, not as inside itself,
/// which is what the nesting check alone would say of an equal path. Asked of
/// a library of its own and of the library that holds it, where the nesting
/// check applies.
#[test]
fn a_root_already_held_is_refused_as_already_a_dir_of_its_library() {
    let dir = crate::tests::tempdir("scan-add-root-checked-again");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let held = super::add_root_checked(&conn, dir.path()).expect("adding the root");
    let library = crate::libraries::name_of(&conn, held.root.libraries[0])
        .expect("reads the name")
        .expect("the directory's library has a name");

    let refused = super::add_root_checked(&conn, dir.path()).expect_err("the same root again");
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "a directory picked twice is this caller's own mistake, not a store fault: {refused:?}"
    );

    let said = format!(
        "{} is already a directory of the library `{library}`",
        dir.path().canonicalize().unwrap().display(),
    );
    assert_eq!(refused.to_string(), said);

    assert_eq!(
        super::add_root_checked_in(&conn, dir.path(), InLibrary::Id(held.root.libraries[0]))
            .map(|added| added.root),
        Err(AddRootError::Unusable(said)),
        "into the library that holds it"
    );
}

/// A directory in no library, which no writer leaves, is a store fault where
/// an answer would name its library.
#[test]
fn a_directory_in_no_library_is_a_fault_where_its_library_is_named() {
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/m')", [])
        .expect("plants a directory in no library");

    let held = super::roots(&conn).expect("roots");
    let why = super::vetted(&conn, &held, Path::new("/m"), None).expect_err("a store fault");
    assert_eq!(why, "directory /m is in no library");
}

/// Nesting is a string-prefix question and prefix comparisons on paths are a
/// classic source of false positives: `manga2` is not inside `manga` even
/// though its name starts with it. A raw-byte or raw-string comparison would
/// refuse this pair; [`Path::starts_with`] compares whole components and
/// does not. Both in one library, where the nesting comparison applies, and
/// each added first, so neither "inside" nor "holds" may compare raw strings.
#[test]
fn a_sibling_whose_name_merely_starts_with_an_existing_roots_name_is_not_refused() {
    let dir = crate::tests::tempdir("scan-add-root-checked-sibling-prefix");
    let manga = dir.path().join("manga");
    let manga2 = dir.path().join("manga2");
    std::fs::create_dir(&manga).expect("mkdir manga");
    std::fs::create_dir(&manga2).expect("mkdir manga2");
    for (first, second) in [(&manga, &manga2), (&manga2, &manga)] {
        let library = crate::Library::open_in_memory().expect("open");
        let conn = library.conn();
        let held = super::add_root_checked(&conn, first).expect("adding the first");

        super::add_root_checked_in(&conn, second, InLibrary::Id(held.root.libraries[0]))
            .unwrap_or_else(|why| panic!("{second:?} is a sibling of {first:?}: {why}"));
    }
}

/// The nesting check must compare *canonical* paths, not the paths as
/// given, or a symlink resolves into an overlap the raw spelling never
/// mentions. `real_root` is stored as a root of Manga; a second
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
    super::add_root_checked_in(&conn, &real_root, InLibrary::Named("Manga"))
        .expect("adding the real root");

    let refused =
        super::add_root_checked_in(&conn, &alias.join("inner"), InLibrary::Named("Manga"))
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

/// Overlapping-libraries spec §4's N: a directory inside or around another
/// library's is added by every way in, whichever of the two came first.
#[test]
fn a_dir_inside_or_around_another_librarys_dir_is_added_every_way() {
    for inner_first in [false, true] {
        for way in ["by id", "by name", "of its own", "made with it"] {
            let top = crate::tests::tempdir("scan-add-nested-across");
            let outer = top.path().join("m");
            let inner = outer.join("inner");
            let elsewhere = top.path().join("x");
            std::fs::create_dir_all(&inner).expect("mkdir");
            std::fs::create_dir_all(&elsewhere).expect("mkdir");
            let library = Library::open_in_memory().expect("open");
            let conn = library.conn();
            let kids = add_root_checked_in(&conn, &elsewhere, InLibrary::Named("Kids"))
                .expect("Kids exists")
                .root
                .libraries[0];
            let (first, second) = if inner_first {
                (&inner, &outer)
            } else {
                (&outer, &inner)
            };
            let held = add_root_checked_in(&conn, first, InLibrary::Named("Manga"))
                .expect("the first directory");

            let added = match way {
                "by id" => add_root_checked_in(&conn, second, InLibrary::Id(kids)).map(|a| a.root),
                "by name" => {
                    add_root_checked_in(&conn, second, InLibrary::Named("Kids")).map(|a| a.root)
                }
                "of its own" => add_root_checked_in(&conn, second, InLibrary::Own).map(|a| a.root),
                _ => create_library(&conn, "Comics", std::slice::from_ref(second), None)
                    .map(|made| made.dirs[0].root.clone()),
            }
            .unwrap_or_else(|why| panic!("{way}, inner first {inner_first}: {why}"));

            assert_eq!(added.path, second.canonicalize().unwrap(), "{way}");
            assert_ne!(added.libraries[0], held.root.libraries[0], "{way}");
        }
    }
}

/// Every `library_root` row, as `(library, directory)`, ascending.
fn links(conn: &Connection) -> Vec<(i64, i64)> {
    conn.prepare("SELECT library_id, root_id FROM library_root ORDER BY library_id, root_id")
        .expect("prepares")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("lists the links")
        .collect::<Result<_, _>>()
        .expect("reads the links")
}

/// A directory another library holds is linked into the library it is sent
/// to, by each way that names one: one `root` row, a second link, and the
/// settings it had, since the libraries share them. The answer names the
/// library that held it, and no watch probe runs, as the probe writes a file
/// into the directory and the directory was probed when it was first added.
#[test]
fn a_dir_another_library_holds_is_linked_and_keeps_its_settings() {
    for way in ["by id", "by name", "into a library the add makes"] {
        let parent = crate::tests::tempdir("scan-add-held-elsewhere");
        let (a, b) = two_dirs(parent.path());
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let held = add_root_checked_in(&conn, &a, InLibrary::Named("Manga"))
            .expect("Manga holds a")
            .root;
        let manga = held.libraries[0];
        update_root(
            &conn,
            held.id,
            &RootPatch {
                edition_key: Some(Some("volume".to_string())),
                exclude_globs: Some(Some("extras/".to_string())),
                scan_interval_secs: Some(Some(4242)),
                ..Default::default()
            },
        )
        .expect("sets Manga's settings for a");
        let before = roots(&conn).expect("roots")[0].clone();
        let kids = add_root_checked_in(&conn, &b, InLibrary::Named("Kids"))
            .expect("Kids holds b")
            .root
            .libraries[0];

        let added = match way {
            "by id" => add_root_checked_in(&conn, &a, InLibrary::Id(kids)),
            "by name" => add_root_checked_in(&conn, &a, InLibrary::Named("Kids")),
            _ => add_root_checked_in(&conn, &a, InLibrary::Named("New")),
        }
        .unwrap_or_else(|why| panic!("{way}: {why}"));

        let into = match way {
            "into a library the add makes" => crate::libraries::id_of(&conn, "New")
                .expect("reads")
                .expect("the add made New"),
            _ => kids,
        };
        assert_eq!(added.also_in, ["Manga"], "{way}");
        assert_eq!((added.probe, added.interval), (None, None), "{way}");
        let stored = roots(&conn).expect("roots");
        assert_eq!(stored.len(), 2, "{way}: one row for a, one for b");
        assert_eq!(
            stored[0],
            Root {
                libraries: vec![manga, into],
                ..before.clone()
            },
            "{way}: the settings Manga gave a stand"
        );
        assert_eq!(added.root, stored[0], "{way}: the answer is the stored row");
        let mut linked = vec![(manga, held.id), (kids, stored[1].id), (into, held.id)];
        linked.sort_unstable();
        assert_eq!(links(&conn), linked, "{way}");
    }
}

/// Overlapping-libraries spec §5: the add asks W of the library it is sent to
/// before it links. Kids holds `m/kids` with the lower id and Manga holds `m`,
/// so a check that met the equal path first would link `m/kids` into Manga,
/// inside Manga's own `m`. The same for `m` sent to Manga where Kids holds `m`
/// with the lower id and Manga `m/kids`. Each is refused by Manga's name, and
/// writes nothing.
#[test]
fn a_dir_another_library_holds_is_not_linked_inside_or_around_one_of_the_librarys_own() {
    for inner_held_elsewhere in [true, false] {
        let top = crate::tests::tempdir("scan-link-nested-in-the-target");
        let m = top.path().join("m");
        let m_kids = m.join("kids");
        std::fs::create_dir_all(&m_kids).expect("mkdir");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let (kids_dir, manga_dir) = if inner_held_elsewhere {
            (&m_kids, &m)
        } else {
            (&m, &m_kids)
        };
        let kids = add_root_checked_in(&conn, kids_dir, InLibrary::Named("Kids"))
            .expect("Kids holds its directory first")
            .root;
        let manga = add_root_checked_in(&conn, manga_dir, InLibrary::Named("Manga"))
            .expect("Manga holds the other")
            .root;
        assert!(
            kids.id < manga.id,
            "the fixture: Kids' directory has the lower id"
        );
        let stored = (roots(&conn).expect("roots"), links(&conn));

        let refused = add_root_checked_in(&conn, kids_dir, InLibrary::Id(manga.libraries[0]))
            .map(|added| added.root);

        let (m, m_kids) = (m.canonicalize().unwrap(), m_kids.canonicalize().unwrap());
        let said = if inner_held_elsewhere {
            format!(
                "{} is inside {}, which is already a directory of the same library, `Manga`",
                m_kids.display(),
                m.display()
            )
        } else {
            format!(
                "{} holds `kids`, which is already a directory of the same library, `Manga`",
                m.display()
            )
        };
        assert_eq!(
            refused,
            Err(AddRootError::Unusable(said)),
            "inner held elsewhere: {inner_held_elsewhere}"
        );
        assert_eq!((roots(&conn).expect("roots"), links(&conn)), stored);
    }
}

/// A directory two libraries hold, sent again to one of them, is refused by
/// the name of the library it was sent to, not by the lowest of its links.
/// Sent with no library, it is refused by both names: there is no library to
/// link it into, and a library of its own cannot hold a directory another
/// already holds.
#[test]
fn a_dir_two_libraries_hold_is_refused_again_by_the_library_it_is_sent_to() {
    let parent = crate::tests::tempdir("scan-add-held-by-two");
    let (a, _) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &a, InLibrary::Named("Kids"))
        .expect("Kids holds a")
        .root
        .libraries[0];
    let manga = create_library(&conn, "Manga", std::slice::from_ref(&a), None)
        .expect("Manga links a")
        .library_id;
    assert!(kids < manga, "the fixture: Kids is a's lowest link");
    let stored = (roots(&conn).expect("roots"), links(&conn));
    let a = a.canonicalize().unwrap();

    assert_eq!(
        add_root_checked_in(&conn, &a, InLibrary::Id(manga)).map(|added| added.root),
        Err(AddRootError::Unusable(format!(
            "{} is already a directory of the library `Manga`",
            a.display()
        )))
    );
    assert_eq!(
        add_root_checked_in(&conn, &a, InLibrary::Own).map(|added| added.root),
        Err(AddRootError::Unusable(format!(
            "{} is already a directory of the libraries `Kids` and `Manga`",
            a.display()
        )))
    );
    assert_eq!((roots(&conn).expect("roots"), links(&conn)), stored);
}

/// A reflowable EPUB is scanned with its format and its text blocks as its
/// count, and derived into a work whose edition is a book, named by its
/// `dc:title`.
#[test]
fn a_reflowable_epub_becomes_a_book_counted_in_blocks() {
    let top = crate::tests::tempdir("scan-epub");
    let series = top.path().join("Story");
    std::fs::create_dir_all(&series).expect("mkdir");
    taiyaku_read::test_support::epub_with(
        &series,
        "Story v01.epub",
        "<dc:title>The Story</dc:title>",
        &["One.", "Two.", "Three."],
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, top.path()).expect("add");

    scan_root(&conn, &root).expect("scan");
    crate::derive::derive(&conn).expect("derives");

    let (format, pages): (String, i64) = conn
        .query_row(
            "SELECT format, pages FROM scan_entry WHERE rel_path = 'Story/Story v01.epub'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the entry");
    assert_eq!((format.as_str(), pages), ("epub-reflow", 3));
    let (edition, member_pages): (i64, i64) = conn
        .query_row("SELECT edition_id, pages FROM member", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("one member");
    assert_eq!(member_pages, 3);
    assert!(
        crate::browse::books_among(&conn, &[edition])
            .expect("lists")
            .contains(&edition)
    );
    let title: String = conn
        .query_row("SELECT title FROM work", [], |row| row.get(0))
        .expect("one work");
    assert_eq!(title, "The Story", "named by the book, not by its file");
}

/// A fixed-layout EPUB is recorded with its format and with neither a count
/// nor a title, though it holds text and a `dc:title`: only a reflowable one
/// is read as a book.
#[test]
fn a_fixed_layout_epub_is_recorded_with_no_count_and_no_title() {
    let top = crate::tests::tempdir("scan-epub-fixed");
    taiyaku_read::test_support::epub_with(
        top.path(),
        "Story v01.epub",
        r#"<dc:title>The Story</dc:title><meta property="rendition:layout">pre-paginated</meta>"#,
        &["One.", "Two."],
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, top.path()).expect("add");

    scan_root(&conn, &root).expect("scan");

    let entry: (String, Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT format, pages, title FROM scan_entry WHERE rel_path = 'Story v01.epub'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the entry");
    assert_eq!(entry, ("epub-fixed".to_string(), None, None));
}

/// An EPUB with no text to read, its pages pictures, is not a book of no
/// blocks: the derive names it among the files whose pages could not be
/// counted, and its library counts it among the files it cannot read.
#[test]
fn an_epub_with_no_text_is_named_unreadable_and_counted_by_its_library() {
    let top = crate::tests::tempdir("scan-epub-no-text");
    let series = top.path().join("Story");
    std::fs::create_dir_all(&series).expect("mkdir");
    taiyaku_read::test_support::epub(&series, "Story v01.epub", &[r#"<img src="p1.jpg"/>"#]);
    taiyaku_read::test_support::epub(&series, "Story v02.epub", &["One."]);
    taiyaku_read::test_support::epub(&series, "Story v03.epub", &["Two."]);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root_in(&conn, top.path(), InLibrary::Named("Books")).expect("add");

    scan_root(&conn, &root).expect("scan");
    let report = crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        (&report.unreadable.uncounted, report.unreadable.no_pages),
        (
            &vec![root.path.join("Story/Story v01.epub").display().to_string()],
            0
        ),
        "named, as a file that would not open is"
    );
    assert_eq!(report.works, 2, "each EPUB of text is a book still");
    assert_eq!(
        crate::libraries::books(&conn, root.libraries[0]).expect("counts"),
        1
    );
}

/// A book scanned before its title was read, as migration 24 leaves one, is
/// read again by the next scan and its title recorded.
#[test]
fn a_book_set_to_have_moved_is_read_again_for_its_title() {
    let top = crate::tests::tempdir("scan-epub-title");
    taiyaku_read::test_support::epub_with(
        top.path(),
        "Story v01.epub",
        "<dc:title>The Story</dc:title>",
        &["One."],
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, top.path()).expect("add");
    scan_root(&conn, &root).expect("scan");
    conn.execute("UPDATE scan_entry SET title = NULL, mtime = -1", [])
        .expect("as migration 24 leaves it");

    scan_root(&conn, &root).expect("scan again");

    let title: Option<String> = conn
        .query_row(
            "SELECT title FROM scan_entry WHERE rel_path = 'Story v01.epub'",
            [],
            |row| row.get(0),
        )
        .expect("the entry");
    assert_eq!(title.as_deref(), Some("The Story"));
}

/// O246's member check is not asked of a link: a link changes no walk and no
/// file's directory. `S - v01` is Kids' directory, added after Copies' `c` was
/// walked and before the derive made the folder a member, so the check now
/// refuses it as a new directory, and Comics links it all the same.
#[test]
fn a_link_is_not_asked_whether_the_directory_is_read_as_one_volume() {
    let top = crate::tests::tempdir("scan-link-at-a-member");
    let c = top.path().join("c");
    let volume = c.join("S").join("S - v01");
    for (page, bytes) in [("000.jpg", "one"), ("001.jpg", "two")] {
        std::fs::create_dir_all(&volume).expect("mkdir");
        std::fs::write(volume.join(page), bytes).expect("write");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let copies = add_root_checked_in(&conn, &c, InLibrary::Named("Copies"))
        .expect("Copies holds c")
        .root;
    scan_root(&conn, &copies).expect("walks c");
    let kids = add_root_checked_in(&conn, &volume, InLibrary::Named("Kids"))
        .expect("not yet a member")
        .root;
    crate::derive::derive(&conn).expect("derives");
    let volume = volume.canonicalize().unwrap();
    assert_eq!(
        at_a_member(&conn, &roots(&conn).expect("roots"), &volume),
        Ok(Some(format!(
            "{} is a folder the library `Copies` reads as the pages of one volume",
            volume.display()
        ))),
        "the fixture: the check would refuse the folder as a new directory"
    );

    let linked = add_root_checked_in(&conn, &volume, InLibrary::Named("Comics"))
        .expect("links Kids' directory into Comics");

    assert_eq!(linked.root.id, kids.id);
    assert_eq!(linked.also_in, ["Kids"]);
}

/// A directory inside or at a directory member of any root is refused, at the
/// member's own path and at a byte-identical copy's, which the derive files
/// under the member and `member.path` does not name. A series folder holding
/// the member is no member, so a directory there is N and is added.
#[test]
fn a_dir_inside_a_directory_member_of_any_root_is_refused() {
    let top = crate::tests::tempdir("scan-add-inside-a-member");
    let (m, c) = (top.path().join("m"), top.path().join("c"));
    for root in [&m, &c] {
        let volume = root.join("S").join("S - v01");
        for (page, bytes) in [
            ("000.jpg", "one"),
            ("001.jpg", "two"),
            ("002.jpg", "three"),
            ("extra/000.jpg", "four"),
            ("extra/001.jpg", "five"),
        ] {
            let at = volume.join(page);
            std::fs::create_dir_all(at.parent().unwrap()).expect("mkdir");
            std::fs::write(&at, bytes).expect("write");
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for (root, name) in [(&m, "Manga"), (&c, "Copies")] {
        let root = add_root_checked_in(&conn, root, InLibrary::Named(name))
            .expect("adds")
            .root;
        scan_root(&conn, &root).expect("scans");
        crate::derive::derive(&conn).expect("derives");
    }
    let member_paths: Vec<String> = conn
        .prepare("SELECT path FROM member")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let member = |root: &Path| root.canonicalize().unwrap().join("S").join("S - v01");
    assert_eq!(
        member_paths,
        vec![member(&m).display().to_string()],
        "the copy is filed under Manga's member"
    );

    for (root, name) in [(&m, "Manga"), (&c, "Copies")] {
        let extra = member(root).join("extra");
        assert_eq!(
            add_root_checked_in(&conn, &extra, InLibrary::Named("Kids")).map(|a| a.root),
            Err(AddRootError::Unusable(format!(
                "{} is inside {}, which the library `{name}` reads as the pages of one volume",
                extra.display(),
                member(root).display()
            )))
        );
        assert_eq!(
            add_root_checked_in(&conn, &member(root), InLibrary::Named("Kids")).map(|a| a.root),
            Err(AddRootError::Unusable(format!(
                "{} is a folder the library `{name}` reads as the pages of one volume",
                member(root).display()
            )))
        );
    }
    add_root_checked_in(&conn, &m.join("S"), InLibrary::Named("Kids"))
        .expect("a series folder holding a member is not one");
}

/// A paused directory's changed folder volume keeps its digest only as the
/// held reading, which no derive replaces until Scan, and a directory added
/// inside it is still refused: it would split the volume's pages (O91).
#[test]
fn a_dir_added_inside_a_paused_folder_volume_is_refused() {
    let top = crate::tests::tempdir("scan-add-inside-a-paused-member");
    let r = top.path().join("r");
    let volume = r.join("S").join("S - v01");
    std::fs::create_dir_all(&volume).expect("mkdir");
    for (page, bytes) in [("000.jpg", "one"), ("001.jpg", "two")] {
        std::fs::write(volume.join(page), bytes).expect("write");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = add_root_checked_in(&conn, &r, InLibrary::Named("Manga"))
        .expect("adds")
        .root;
    scan_root(&conn, &manga).expect("scans");
    crate::derive::derive(&conn).expect("derives");
    let d: String = conn
        .query_row("SELECT sha256 FROM member", [], |row| row.get(0))
        .expect("the fixture: the folder is the one member");

    let extra = volume.join("extra");
    std::fs::create_dir_all(&extra).expect("mkdir");
    std::fs::write(extra.join("000.jpg"), "three").expect("write");
    let apart = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    std::fs::File::open(&volume)
        .expect("opens the folder")
        .set_times(std::fs::FileTimes::new().set_modified(apart))
        .expect("moves its mtime");
    scan_root(&conn, &manga).expect("scans the change");
    pause(&conn, std::slice::from_ref(&manga)).expect("pauses");
    crate::derive::derive(&conn).expect("derives");
    let rel = "S/S - v01";
    assert_eq!(
        (
            digest_at(&conn, &manga, rel),
            held_reading(&conn, manga.id, rel).0
        ),
        (None, Some(d)),
        "the fixture: the walk cleared the folder's digest, and the derive left it held"
    );
    let member = volume.canonicalize().unwrap();
    let extra = member.join("extra");

    assert_eq!(
        add_root_checked_in(&conn, &extra, InLibrary::Named("Kids")).map(|a| a.root),
        Err(AddRootError::Unusable(format!(
            "{} is inside {}, which the library `Manga` reads as the pages of one volume",
            extra.display(),
            member.display()
        ))),
        "refused as when the folder held its digest"
    );
}

/// The member check names every library holding the directory the member is
/// under: Copies' `c`, linked into Comics too, through the add.
#[test]
fn a_member_refusal_names_every_library_holding_the_directory() {
    let top = crate::tests::tempdir("scan-member-in-two");
    let c = top.path().join("c");
    let volume = c.join("S").join("S - v01");
    std::fs::create_dir_all(volume.join("extra")).expect("mkdir");
    for (page, bytes) in [("000.jpg", "one"), ("extra/000.jpg", "two")] {
        std::fs::write(volume.join(page), bytes).expect("write");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let copies = add_root_checked_in(&conn, &c, InLibrary::Named("Copies"))
        .expect("Copies holds c")
        .root;
    scan_root(&conn, &copies).expect("walks c");
    crate::derive::derive(&conn).expect("derives");
    add_root_checked_in(&conn, &c, InLibrary::Named("Comics")).expect("links c into Comics");
    let volume = volume.canonicalize().unwrap();

    assert_eq!(
        add_root_checked_in(&conn, &volume, InLibrary::Named("Kids")).map(|a| a.root),
        Err(AddRootError::Unusable(format!(
            "{} is a folder the libraries `Copies` and `Comics` read as the pages of one volume",
            volume.display()
        )))
    );
    assert_eq!(
        add_root_checked_in(&conn, &volume.join("extra"), InLibrary::Named("Kids")).map(|a| a.root),
        Err(AddRootError::Unusable(format!(
            "{} is inside {}, which the libraries `Copies` and `Comics` read as the pages of \
             one volume",
            volume.join("extra").display(),
            volume.display()
        )))
    );
}

/// O204: the add compares the stored roots inside its own transaction. A
/// holds the write lock with X's directory `x` inserted and uncommitted while
/// B adds `x/inner` to X, so B must wait for A's commit and then meet `x`. A
/// defect that compares before B's transaction reads the roots without `x`
/// and accepts. Load can make this miss that defect, should B start only after
/// A commits. It cannot fail correct code, which waits on A's lock within the
/// busy timeout.
#[test]
fn the_add_compares_the_stored_roots_inside_its_transaction() {
    let top = crate::tests::tempdir("scan-add-race");
    let x = top.path().join("x");
    let inner = x.join("inner");
    let other = top.path().join("other");
    std::fs::create_dir_all(&inner).expect("mkdir");
    std::fs::create_dir_all(&other).expect("mkdir");
    let db = top.path().join("library.db");
    let a = Library::open(&db).expect("the first connection");
    let b = Library::open(&db).expect("a second connection on the same file");
    let x_id = add_root_checked_in(&a.conn(), &other, InLibrary::Named("X"))
        .expect("X exists")
        .root
        .libraries[0];

    let conn = a.conn();
    conn.execute_batch("BEGIN IMMEDIATE")
        .expect("A takes the write lock");
    let held = add_root_in(&conn, &x, InLibrary::Id(x_id)).expect("A inserts x");
    let answer = std::thread::scope(|scope| {
        let racing = scope.spawn(|| {
            add_root_checked_in(&b.conn(), &inner, InLibrary::Id(x_id)).map(|added| added.root)
        });
        std::thread::sleep(Duration::from_secs(1));
        conn.execute_batch("COMMIT").expect("A commits");
        racing.join().expect("B's add returns")
    });

    assert_eq!(
        answer,
        Err(AddRootError::Unusable(format!(
            "{} is inside {}, which is already a directory of the same library, `X`",
            inner.canonicalize().unwrap().display(),
            held.path.display()
        )))
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
/// captures *both* stdout and stderr for a passing test, so a skip announced
/// on either stream is invisible under a plain `cargo test` — it would report
/// an ordinary `ok` on the CI host every single run, with nothing to show it
/// measured nothing.
///
/// A plain **file** at the path a directory is required at needs no such
/// guard: `std::fs::read_dir` on a non-directory fails with `ENOTDIR`
/// unconditionally, a type mismatch rather than a permission check, so it
/// refuses for root exactly as it does for anyone else.
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

/// A store fault, not any of this function's three documented caller-fault
/// refusals, must answer [`AddRootError::Failed`], not
/// [`AddRootError::Unusable`]. `path` here exists, is readable and overlaps
/// nothing, so every caller-fault check passes; `PRAGMA query_only` then makes
/// the connection itself refuse to write, which is what a disk-full or
/// database-busy failure looks like from inside `add_root` — the insert
/// [`add_root_checked`] runs once its own checks are satisfied.
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

/// **The correction `mark_derive_failed` exists for.** [`scan_root`] itself
/// only ever sees the walk, and writes `last_scan_status = "ok"` the moment
/// that commits, before the derive half of the job (its own caller's job, per
/// this function's own doc comment) has even started. The server's job then
/// marks the directory `deriving` until its derive ends, as it does here.
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

    super::mark_deriving(&conn, &root).expect("marking the walk's derive owed");
    super::mark_derive_failed(&conn, &root).expect("correcting the status");

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

/// O186: at a start, a `deriving` or a `waiting` is a scan that a stop, a
/// panic or a refused write (O272) left without its last word, and every other
/// word is a scan that ended one way or another.
#[test]
fn rewriting_cut_derives_marks_only_deriving_and_waiting_interrupted() {
    let dir = crate::tests::tempdir("scan-interrupt-derives");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let ids: Vec<i64> = ["deriving", "ok", "failed", "waiting"]
        .into_iter()
        .map(|status| {
            let path = dir.path().join(status);
            std::fs::create_dir_all(&path).expect("makes the directory");
            let root = super::add_root(&conn, &path).expect("adds the directory");
            set_status(&conn, root.id, status);
            root.id
        })
        .collect();

    assert_eq!(super::interrupt_derives(&conn), Ok(2));
    let statuses: Vec<Option<String>> = ids
        .iter()
        .map(|id| status_and_entries(&conn, *id).0)
        .collect();
    assert_eq!(
        statuses,
        [
            Some("interrupted".to_string()),
            Some("ok".to_string()),
            Some("failed".to_string()),
            Some("interrupted".to_string())
        ]
    );
}

/// Whether directory `id` is marked held ([`super::mark_held`]).
fn is_held(conn: &Connection, id: i64) -> bool {
    conn.query_row(
        "SELECT scan_held_at IS NOT NULL FROM root WHERE id = ?1",
        [id],
        |r| r.get(0),
    )
    .expect("reading the mark")
}

/// Design O314 §5, ruling 10: the start marks nothing held. A `deriving` or a
/// `waiting` it rewrites may be an earlier version's, cut when this one was
/// deployed, which says nothing of whether that scan is still wanted; a scan
/// of this version holds its own mark already, a queued one's among them.
#[test]
fn rewriting_cut_derives_marks_no_directory_held() {
    let dir = crate::tests::tempdir("scan-interrupt-marks-none");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let ids: Vec<i64> = ["deriving", "ok", "cancelled", "waiting", "failed"]
        .into_iter()
        .map(|status| {
            let path = dir.path().join(status);
            std::fs::create_dir_all(&path).expect("makes the directory");
            let root = super::add_root(&conn, &path).expect("adds the directory");
            set_status(&conn, root.id, status);
            root.id
        })
        .collect();

    assert_eq!(super::interrupt_derives(&conn), Ok(2));
    assert_eq!(
        ids.iter().map(|id| is_held(&conn, *id)).collect::<Vec<_>>(),
        [false; 5],
        "none held"
    );
}

/// Design O314 §5, ruling 9: a scan queued behind another job is marked held
/// with its `waiting`, so a stop before it holds the job lock is taken up at
/// the next start; writing its last word back leaves the mark, which its job
/// clears as it ends.
#[test]
fn a_queued_scan_is_marked_held_with_its_waiting() {
    let dir = crate::tests::tempdir("scan-queued-held");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots = dirs_reading(&conn, dir.path(), &["ok", "failed"]);

    let queued = super::mark_queued(&conn, &roots);
    let while_queued: Vec<(Option<String>, bool)> = roots
        .iter()
        .map(|root| {
            (
                status_and_entries(&conn, root.id).0,
                is_held(&conn, root.id),
            )
        })
        .collect();
    super::unmark_queued(&conn, &queued).expect("writes the words back");
    let once_held: Vec<(Option<String>, bool)> = roots
        .iter()
        .map(|root| {
            (
                status_and_entries(&conn, root.id).0,
                is_held(&conn, root.id),
            )
        })
        .collect();

    let word = |w: &str| Some(w.to_string());
    assert_eq!(
        (while_queued, once_held),
        (
            vec![(word("waiting"), true), (word("waiting"), true)],
            vec![(word("ok"), true), (word("failed"), true)]
        ),
        "(while queued, once its job holds the lock): (word, held)"
    );
}

/// The held mark asks the directory's id and its path, as every status write
/// does: a stale root marks nothing on the directory that took its id, so a
/// start does not scan that one again, and clears nothing of its mark either.
#[test]
fn the_held_mark_leaves_the_dir_that_took_a_removed_ones_id_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (stale, new, _walked, _other) =
        stale_root_and_the_dir_that_took_its_id(&conn, "stale-held");

    super::mark_held(&conn, std::slice::from_ref(&stale)).expect("the write runs");
    let marked_by_the_stale_root = is_held(&conn, new.id);
    super::mark_held(&conn, std::slice::from_ref(&new)).expect("marks the new directory");
    super::unmark_held(&conn, &[(stale.id, stale.path.clone())]).expect("the write runs");
    let cleared_by_the_stale_root = !is_held(&conn, new.id);
    super::unmark_held(&conn, &[(new.id, new.path.clone())]).expect("clears the new directory");

    assert_eq!(
        (
            marked_by_the_stale_root,
            cleared_by_the_stale_root,
            is_held(&conn, new.id)
        ),
        (false, false, false),
        "(marked by the stale root, cleared by it, held once its own job ends)"
    );
}

/// Directories under `dir`, one per status, each added and set to it.
fn dirs_reading(conn: &Connection, dir: &Path, statuses: &[&str]) -> Vec<Root> {
    statuses
        .iter()
        .map(|status| {
            let path = dir.join(status);
            std::fs::create_dir_all(&path).expect("makes the directory");
            let root = super::add_root(conn, &path).expect("adds the directory");
            set_status(conn, root.id, status);
            root
        })
        .collect()
}

fn statuses_of(conn: &Connection, roots: &[Root]) -> Vec<Option<String>> {
    roots
        .iter()
        .map(|root| status_and_entries(conn, root.id).0)
        .collect()
}

/// O51: a derive that failed while it waited for the lease is as missing as
/// one that failed once it began.
#[test]
fn mark_derive_failed_marks_a_waiting_directory_failed() {
    let dir = crate::tests::tempdir("scan-waiting-failed");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots = dirs_reading(&conn, dir.path(), &["waiting"]);

    super::mark_derive_failed(&conn, &roots[0]).expect("marks it");

    assert_eq!(statuses_of(&conn, &roots), [Some("failed".to_string())]);
}

/// O51: a walk waits for the lease as `waiting` and derives as `deriving`;
/// each word is written only over the one before it, so a refused walk, or
/// another scan's `ok` written while this one waited, keeps its word.
#[test]
fn waiting_and_waited_rewrite_only_the_word_before_them() {
    let dir = crate::tests::tempdir("scan-waiting-waited");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots = dirs_reading(&conn, dir.path(), &["deriving", "refused", "ok"]);
    let word = |status: &str| Some(status.to_string());

    super::mark_waiting(&conn, &roots).expect("marks them waiting");
    assert_eq!(
        statuses_of(&conn, &roots),
        [word("waiting"), word("refused"), word("ok")]
    );

    super::mark_waited(&conn, &roots).expect("marks them deriving again");
    assert_eq!(
        statuses_of(&conn, &roots),
        [word("deriving"), word("refused"), word("ok")]
    );
}

/// O51: after the job's one derive, a `waiting` can only be its own mark whose
/// `mark_waited` did not land, so it ends `ok` as a `deriving` does; a refused
/// walk, and an `interrupted` a second server's start wrote, keep their word.
#[test]
fn marking_derived_ends_a_waiting_as_it_ends_a_deriving() {
    let dir = crate::tests::tempdir("scan-derived-waiting");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots = dirs_reading(
        &conn,
        dir.path(),
        &["deriving", "waiting", "refused", "interrupted"],
    );
    let word = |status: &str| Some(status.to_string());

    super::mark_derived(&conn, &roots).expect("marks them");

    assert_eq!(
        statuses_of(&conn, &roots),
        [word("ok"), word("ok"), word("refused"), word("interrupted")]
    );
}

/// O186: one refused write does not strand the directories after it at
/// `deriving`, which the next start would read as a cut derive; the first
/// refusal is what is answered.
#[test]
fn marking_derived_goes_on_past_a_refused_write_and_answers_the_first() {
    let dir = crate::tests::tempdir("scan-mark-derived-refused");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots: Vec<Root> = ["first", "second", "third"]
        .into_iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::create_dir_all(&path).expect("makes the directory");
            let root = super::add_root(&conn, &path).expect("adds the directory");
            set_status(&conn, root.id, "deriving");
            root
        })
        .collect();
    conn.execute_batch(&format!(
        "CREATE TEMP TRIGGER planted BEFORE UPDATE OF last_scan_status ON main.root
           WHEN old.id IN ({}, {}) BEGIN SELECT RAISE(ABORT, 'planted'); END",
        roots[0].id, roots[2].id
    ))
    .expect("plants the fault");

    let refused = super::mark_derived(&conn, &roots).expect_err("two writes are refused");
    assert_eq!(
        refused,
        format!(
            "recording that directory {} was derived: planted",
            roots[0].path.display()
        )
    );
    let statuses: Vec<Option<String>> = roots
        .iter()
        .map(|root| status_and_entries(&conn, root.id).0)
        .collect();
    assert_eq!(
        statuses,
        [
            Some("deriving".to_string()),
            Some("ok".to_string()),
            Some("deriving".to_string())
        ],
        "the directory between the two refusals is marked"
    );
}

/// O51: a scan queued for the job lock reads `waiting` over whatever its last
/// scan left, never scanned included, and reads that again once it starts;
/// a word another writer put there meanwhile is kept.
#[test]
fn a_queued_scan_reads_waiting_and_then_its_word_before() {
    let dir = crate::tests::tempdir("scan-queued");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let mut roots = dirs_reading(&conn, dir.path(), &["ok", "refused", "failed", "rewritten"]);
    let never = dir.path().join("never");
    std::fs::create_dir_all(&never).expect("makes the directory");
    roots.push(super::add_root(&conn, &never).expect("adds the directory"));
    let word = |status: &str| Some(status.to_string());

    let queued = super::mark_queued(&conn, &roots);
    let while_queued = statuses_of(&conn, &roots);
    set_status(&conn, roots[3].id, "ok");
    super::unmark_queued(&conn, &queued).expect("writes them back");

    assert_eq!(
        (while_queued, statuses_of(&conn, &roots)),
        (
            vec![word("waiting"); 5],
            vec![
                word("ok"),
                word("refused"),
                word("failed"),
                word("ok"),
                None
            ]
        ),
        "(queued, started)"
    );
}

/// Each of `roots`' `last_scan_at` and `last_scan_status`.
fn last_scan_and_status(
    conn: &Connection,
    roots: &[Root],
) -> Vec<(Option<String>, Option<String>)> {
    roots
        .iter()
        .map(|root| {
            conn.query_row(
                "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
                [root.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("reads the last scan")
        })
        .collect()
}

/// A cancelled job's walks committed and each wrote its `last_scan_at`, yet
/// the job did not finish: the stamp read before the walk goes back, so a
/// directory never scanned still reads Never, and one scanned before reads
/// that scan.
#[test]
fn a_cancelled_directory_keeps_its_last_scan_and_reads_cancelled() {
    let dir = crate::tests::tempdir("scan-cancelled");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots: Vec<Root> = ["never", "before"]
        .into_iter()
        .map(|name| {
            let path = dir.path().join(name);
            write_pages(&path.join("Series").join("c001"), 1);
            super::add_root(&conn, &path).expect("adds the directory")
        })
        .collect();
    let before = "2026-11-01 00:00:00".to_string();
    conn.execute(
        "UPDATE root SET last_scan_at = ?2, last_scan_status = 'ok' WHERE id = ?1",
        rusqlite::params![roots[1].id, before],
    )
    .expect("an earlier scan");

    let held = super::last_scans(&conn, &roots).expect("reads the last scans");
    for root in &roots {
        super::scan_root(&conn, root).expect("walks");
    }
    let walked = last_scan_and_status(&conn, &roots);
    super::mark_cancelled(&conn, &held).expect("marks them cancelled");

    assert_eq!(
        held.iter()
            .map(|one| (one.root.id, one.last_scan_at.clone()))
            .collect::<Vec<_>>(),
        [(roots[0].id, None), (roots[1].id, Some(before.clone()))],
        "the stamps read before the walks"
    );
    assert!(
        walked
            .iter()
            .all(|(at, _)| at.as_ref().is_some_and(|at| *at != before)),
        "sanity: each walk wrote a last scan of its own: {walked:?}"
    );
    let cancelled = Some("cancelled".to_string());
    assert_eq!(
        last_scan_and_status(&conn, &roots),
        [(None, cancelled.clone()), (Some(before), cancelled)]
    );
}

/// The start rewrites a word a stop left standing, and `cancelled` is the
/// last word of a job that ended: rewritten, the page would lose its cancel.
#[test]
fn a_cancelled_status_survives_the_servers_start() {
    let dir = crate::tests::tempdir("scan-cancelled-start");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let roots = dirs_reading(&conn, dir.path(), &["deriving"]);
    let held = super::last_scans(&conn, &roots).expect("reads the last scan");
    super::mark_cancelled(&conn, &held).expect("marks it cancelled");

    assert_eq!(super::interrupt_derives(&conn), Ok(0));
    assert_eq!(statuses_of(&conn, &roots), [Some("cancelled".to_string())]);
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
        Some(crate::watch::interval_for(
            added.probe.as_ref().expect("a new directory is probed")
        )),
        "the interval must follow the probe, not a default"
    );
}

/// `Added::root` is the row the add stored, which holds no interval: an
/// answer drawn from it must say what the store says (spec §4, "The setting").
#[test]
fn adding_a_root_returns_the_row_it_stored() {
    let dir = crate::tests::tempdir("scan-add-root-checked-returned-row");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let added = super::add_root_checked(&conn, dir.path()).expect("adding the root");

    assert_eq!(
        added.root.scan_interval_secs, None,
        "the returned Added::root must hold no interval, as the stored row does: {added:?}"
    );
    assert_eq!(
        added.root,
        super::roots(&conn).expect("listing roots")[0],
        "Added::root must be the stored row"
    );
}

/// The add probes, but the probe's interval is not written: a stored NULL is
/// Automatic, which the scheduler reads as the unwatched interval until the
/// watcher ships (spec §4, "The setting"). The probe's interval is still
/// answered, for the add's report (spec §4, Decision 11).
#[test]
fn adding_a_root_writes_no_interval_onto_the_stored_row() {
    let dir = crate::tests::tempdir("scan-add-root-checked-no-interval");
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();

    let added = super::add_root_checked(&conn, dir.path()).expect("adding the root");

    let stored = &super::roots(&conn).expect("listing roots")[0];
    assert_eq!(
        stored.scan_interval_secs, None,
        "the row must hold no interval, so it reads as Automatic: {stored:?}"
    );
    assert!(
        added.interval.is_some(),
        "the probe's interval is still answered: {added:?}"
    );
}

// --- the library a root is filed under -------------------------------------

/// The name of the library `root` is filed under.
fn library_of(conn: &Connection, root: &Root) -> String {
    conn.query_row(
        "SELECT name FROM library WHERE id = ?1",
        [root.libraries[0]],
        |r| r.get(0),
    )
    .expect("the root's library exists")
}

#[test]
fn a_dir_added_on_its_own_is_a_library_named_for_it() {
    let parent = crate::tests::tempdir("scan-own-library");
    let a = parent.path().join("a").join("Volumes");
    let b = parent.path().join("b").join("Volumes");
    std::fs::create_dir_all(&a).expect("mkdir");
    std::fs::create_dir_all(&b).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let first = add_root(&conn, &a).expect("adds a");
    let second = add_root(&conn, &b).expect("adds b");

    assert_ne!(
        first.libraries[0], second.libraries[0],
        "each is a library of its own"
    );
    assert_eq!(library_of(&conn, &first), "Volumes");
    assert_eq!(
        library_of(&conn, &second),
        b.canonicalize().expect("canonical").to_string_lossy(),
        "a taken name falls back to the whole path"
    );
    assert_eq!(
        roots(&conn)
            .expect("lists")
            .iter()
            .map(|r| r.libraries[0])
            .collect::<Vec<_>>(),
        vec![first.libraries[0], second.libraries[0]],
        "roots reads back what add_root returned"
    );
}

#[test]
fn a_dir_added_to_a_named_library_joins_it_or_creates_it() {
    let parent = crate::tests::tempdir("scan-named-library");
    let (volumes, chapters, other) = (
        parent.path().join("Volumes"),
        parent.path().join("Chapters"),
        parent.path().join("Other"),
    );
    for dir in [&volumes, &chapters, &other] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let first = add_root_in(&conn, &volumes, InLibrary::Named("Manga")).expect("creates Manga");
    let second = add_root_in(&conn, &chapters, InLibrary::Named(" Manga ")).expect("joins Manga");
    let third = add_root(&conn, &other).expect("a library of its own");

    assert_eq!(
        first.libraries[0], second.libraries[0],
        "the second directory joined, by trimmed name"
    );
    assert_ne!(third.libraries[0], first.libraries[0]);
    assert_eq!(library_of(&conn, &first), "Manga");
}

/// A directory's name can begin or end with spaces, and a library's name made
/// for it or given to `libraries::create` never does: both are stripped by
/// `libraries::trim_name`. Otherwise
/// `Named("Manga")` could not find the library and would make a second one.
#[test]
fn a_dir_named_with_spaces_is_a_library_its_name_without_them_joins() {
    let parent = crate::tests::tempdir("scan-library-spaces");
    let (spaced, other) = (parent.path().join(" Manga "), parent.path().join("Other"));
    for dir in [&spaced, &other] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let own = add_root(&conn, &spaced).expect("a library of its own");
    let joined = add_root_in(&conn, &other, InLibrary::Named("Manga")).expect("joins it");

    assert_eq!(
        library_of(&conn, &own),
        "Manga",
        "named for its directory, without the spaces"
    );
    assert_eq!(
        joined.libraries[0], own.libraries[0],
        "`Manga` found the directory's library"
    );
    assert_eq!(
        crate::libraries::list(&conn).expect("lists").len(),
        1,
        "and made no second library"
    );
}

/// A named add finds the library stripped and in any ASCII case, and makes no
/// second one.
#[test]
fn a_dir_joins_the_library_its_name_names_in_any_case() {
    let parent = crate::tests::tempdir("scan-library-any-case");
    let (first, second) = (
        parent.path().join("Volumes"),
        parent.path().join("Chapters"),
    );
    for dir in [&first, &second] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let made = add_root_in(&conn, &first, InLibrary::Named("Manga")).expect("makes Manga");

    let joined = add_root_in(&conn, &second, InLibrary::Named("\tMANGA ")).expect("joins it");

    assert_eq!(joined.libraries[0], made.libraries[0]);
    assert_eq!(crate::libraries::list(&conn).expect("lists").len(), 1);
}

/// A library is not made under another's name in another case, and the
/// refusal names the library as it is stored.
#[test]
fn a_library_is_not_made_under_a_name_another_holds_in_another_case() {
    let parent = crate::tests::tempdir("scan-create-case");
    let (first, second) = (
        parent.path().join("Volumes"),
        parent.path().join("Chapters"),
    );
    for dir in [&first, &second] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    create_library(&conn, "Manga", std::slice::from_ref(&first), None).expect("makes Manga");

    let why = create_library(&conn, "\tMANGA ", std::slice::from_ref(&second), None)
        .expect_err("the name is taken in another case");

    assert_eq!(
        why,
        AddRootError::Unusable(
            "a library is already called `Manga`; add the directory to it instead".to_string()
        )
    );
    assert_eq!(
        roots(&conn).expect("reads").len(),
        1,
        "the second directory was not added"
    );
}

/// Where both names a directory could be a library under are taken in another
/// case, the directory is refused as the caller's mistake, naming both, and not
/// left to the add's insert to fail on the index as a store fault.
#[test]
fn a_dir_whose_two_names_are_taken_in_another_case_is_refused_naming_both() {
    let parent = crate::tests::tempdir("scan-own-case");
    let dir = parent.path().join("Manga");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let canonical = dir.canonicalize().expect("canonical");
    let whole = canonical.to_string_lossy().to_ascii_uppercase();
    assert_ne!(
        whole,
        canonical.to_string_lossy(),
        "the fixture needs lower case in the path"
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO library (id, name) VALUES (1, 'manga'), (2, ?1)",
        [&whole],
    )
    .expect("both names, in another case");

    let why = add_root_checked(&conn, &dir).expect_err("both names are taken");

    assert!(
        matches!(&why, AddRootError::Unusable(text) if text.contains("`Manga`")),
        "{why:?}"
    );
}

/// A library of its own is named for the directory's last component stripped
/// of white space, so a directory called `. ` would make a library called `.`,
/// which no page address holds ([`crate::has_no_address`]).
#[test]
fn a_dir_called_a_dot_and_a_space_is_not_a_library_of_its_own() {
    let parent = crate::tests::tempdir("scan-own-dot");
    let dir = parent.path().join(". ");
    std::fs::create_dir(&dir).expect("mkdir");
    let canonical = dir.canonicalize().expect("canonical");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    assert_eq!(
        add_root_checked(&conn, &dir),
        Err(AddRootError::Unusable(format!(
            "{} cannot be a library of its own: a library named for it would be called `.`, \
             and no address can hold “.” or “..” as a name; add it to a library by name",
            canonical.display()
        )))
    );
    assert_eq!(libraries_and_roots(&conn), (0, 0), "nothing was added");
}

#[test]
fn a_dir_called_a_space_and_two_dots_is_not_a_library_of_its_own() {
    let parent = crate::tests::tempdir("scan-own-two-dots");
    let dir = parent.path().join(" ..");
    std::fs::create_dir(&dir).expect("mkdir");
    let canonical = dir.canonicalize().expect("canonical");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    assert_eq!(
        add_root_checked(&conn, &dir),
        Err(AddRootError::Unusable(format!(
            "{} cannot be a library of its own: a library named for it would be called `..`, \
             and no address can hold “.” or “..” as a name; add it to a library by name",
            canonical.display()
        )))
    );
    assert_eq!(libraries_and_roots(&conn), (0, 0), "nothing was added");
}

/// Refused inside the add's transaction, as a folder another directory reads
/// as one volume: a directory another library holds would be linked instead.
#[test]
fn a_refused_dir_names_no_library_into_being() {
    let dir = crate::tests::tempdir("scan-refused-library");
    let volume = dir.path().join("S").join("S - v01");
    std::fs::create_dir_all(&volume).expect("mkdir");
    std::fs::write(volume.join("000.jpg"), "one").expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, dir.path()).expect("the root");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");

    let why = add_root_checked_in(&conn, &volume, InLibrary::Named("Never"))
        .expect_err("a folder read as one volume is refused");
    assert!(
        matches!(&why, AddRootError::Unusable(text) if text.ends_with("as the pages of one volume")),
        "{why:?}"
    );

    assert_eq!(
        crate::libraries::id_of(&conn, "Never").expect("reads"),
        None,
        "the refusal left no library behind"
    );
}

/// The unchecked path's half of the same promise: the library is created in
/// the directory's transaction, so an insert refused after it leaves none.
#[test]
fn a_dir_refused_at_the_insert_leaves_no_library_behind() {
    let dir = crate::tests::tempdir("scan-refused-insert-library");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root(&conn, dir.path()).expect("the root");

    add_root_in(&conn, dir.path(), InLibrary::Named("Never"))
        .expect_err("the path is already a root");

    assert_eq!(
        crate::libraries::id_of(&conn, "Never").expect("reads"),
        None,
        "the refused insert took its new library with it"
    );
}

/// **The id the add returns is the root's own**, though a library row may be
/// inserted just before it (`libraries::create`, for a new named library) or
/// just after it (the library of its own [`InLibrary::Own`] names), and its
/// link after both. Where every root is a library of its own, a fresh database
/// gives each root and its library the same id, and a test cannot tell one
/// from the other. Two directories filed under one library put the ids out of
/// step before the roots this test reads, and it reads them back through
/// `roots` too.
#[test]
fn a_new_root_is_returned_with_its_own_id_not_its_librarys() {
    let parent = crate::tests::tempdir("scan-root-id-not-library-id");
    let dirs: Vec<PathBuf> = ["a", "b", "own", "named"]
        .iter()
        .map(|name| {
            let dir = parent.path().join(name);
            std::fs::create_dir(&dir).expect("mkdir");
            dir.canonicalize().expect("canonical")
        })
        .collect();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root_in(&conn, &dirs[0], InLibrary::Named("Shared")).expect("a");
    add_root_in(&conn, &dirs[1], InLibrary::Named("Shared")).expect("b");

    let own = add_root(&conn, &dirs[2]).expect("a library of its own");
    let named = add_root_in(&conn, &dirs[3], InLibrary::Named("Fresh")).expect("a new library");

    let id_of_root_at = |dir: &Path| -> i64 {
        conn.query_row(
            "SELECT id FROM root WHERE path = ?1",
            [dir.to_string_lossy()],
            |r| r.get(0),
        )
        .expect("the root's row")
    };
    for (added, dir, name) in [(&own, &dirs[2], "own"), (&named, &dirs[3], "Fresh")] {
        assert_ne!(
            added.id, added.libraries[0],
            "the fixture must tell a root's id from its library's: {added:?}"
        );
        assert_eq!(
            (added.id, added.path.as_path()),
            (id_of_root_at(dir), dir.as_path()),
            "the id returned is the root row's own"
        );
        assert_eq!(library_of(&conn, added), name, "and its library is its own");
    }
    assert_eq!(
        roots(&conn).expect("lists")[2..],
        [own.clone(), named.clone()],
        "roots reads back each root's own id and its library's"
    );
}

/// The checked path refuses a directory that cannot be a library of its own,
/// and only that one. The add tries the directory's last component and then
/// its whole path; with both taken it would fail inside its transaction,
/// which `add_root_checked_in` would report as a store fault. Each directory
/// here is checked against the libraries the ones before it made.
#[test]
fn a_dir_is_refused_a_library_of_its_own_only_when_both_its_names_are_taken() {
    let parent = crate::tests::tempdir("scan-own-library-clash");
    let dirs: Vec<PathBuf> = ["a/Volumes", "b/Volumes", "c/Volumes", "Other"]
        .iter()
        .map(|rel| {
            let dir = parent.path().join(rel);
            std::fs::create_dir_all(&dir).expect("mkdir");
            dir.canonicalize().expect("canonical")
        })
        .collect();
    let (a, b, c, other) = (&dirs[0], &dirs[1], &dirs[2], &dirs[3]);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let c_whole = c.to_string_lossy().into_owned();

    let first = add_root_checked(&conn, a).expect("Volumes is free");
    let second = add_root_checked(&conn, b).expect("Volumes is taken, b's whole path is free");
    add_root_checked_in(&conn, other, InLibrary::Named(&c_whole))
        .expect("a library that happens to be called c's whole path");
    let before = crate::libraries::list(&conn).expect("lists");

    let refused = add_root_checked(&conn, c).expect_err("both of c's names are taken");

    assert_eq!(library_of(&conn, &first.root), "Volumes");
    assert_eq!(library_of(&conn, &second.root), b.to_string_lossy());
    assert!(
        matches!(refused, AddRootError::Unusable(_)),
        "a clash of names is this caller's to fix, not a store fault: {refused:?}"
    );
    let refused = refused.to_string();
    assert!(
        refused.contains("`Volumes`") && refused.contains(&format!("`{c_whole}`")),
        "the refusal names both taken names: {refused}"
    );
    assert_eq!(
        crate::libraries::list(&conn).expect("lists"),
        before,
        "the refusal added no library and no directory"
    );
}

/// `/` has no last component, so the add names its library by the whole path
/// alone, and only that name can clash.
#[test]
fn the_filesystem_root_can_be_refused_on_its_whole_path_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let slash = Path::new("/");
    assert_eq!(own_library_clash(&conn, slash).expect("reads"), None);

    crate::libraries::create(&conn, "/").expect("a library called `/`");

    let why = own_library_clash(&conn, slash)
        .expect("reads")
        .expect("`/` is taken, and there is no other name to try");
    assert!(why.contains("`/`"), "{why}");
}

/// `own_library_clash` refuses exactly the directories the add cannot file
/// under a library of its own. **The hard-coded table is the oracle**: the add
/// names the library through the same `own_library_name`, so its two columns
/// agree by construction except where `libraries::create` or the
/// `library_name_folded` index refuses a name. The add is an insert rolled
/// back; the paths need not exist, since neither reads the disk. The libraries
/// are chosen so that comparing a name with its spaces, without its tabs, or
/// in its own case would answer differently.
#[test]
fn own_library_clash_refuses_exactly_what_the_trigger_cannot_name() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in ["Manga", "/c/ Manga", "/d/", "/g/\tManga"] {
        crate::libraries::create(&conn, name).expect("a library");
    }
    let refused_by_the_add = |candidate: &str| -> bool {
        conn.execute_batch("SAVEPOINT trial").expect("savepoint");
        let refused = insert_root(&conn, Path::new(candidate), InLibrary::Own, None).is_err();
        conn.execute_batch("ROLLBACK TO trial; RELEASE trial")
            .expect("rolls back");
        refused
    };

    let answers: Vec<(&str, bool, bool)> = [
        "/a/ Manga ",
        "/c/ Manga ",
        "/c/ Manga",
        "/d/  ",
        "/e/  ",
        "/f/Comics",
        "/g/\tManga\t",
        "/c/ MANGA",
    ]
    .into_iter()
    .map(|candidate| {
        let clash = own_library_clash(&conn, Path::new(candidate)).expect("reads");
        (candidate, clash.is_some(), refused_by_the_add(candidate))
    })
    .collect();

    assert_eq!(
        answers,
        vec![
            ("/a/ Manga ", false, false),
            ("/c/ Manga ", true, true),
            ("/c/ Manga", true, true),
            ("/d/  ", true, true),
            ("/e/  ", false, false),
            ("/f/Comics", false, false),
            ("/g/\tManga\t", true, true),
            ("/c/ MANGA", true, true),
        ],
        "(path, own_library_clash refuses, the add refuses)"
    );
}

/// A library named nothing is the caller's mistake: refused as `Unusable`,
/// before anything is written, rather than reaching `libraries::create`'s
/// own refusal inside the insert and coming back as a store fault.
#[test]
fn a_library_named_nothing_is_refused_as_the_callers_mistake() {
    let dir = crate::tests::tempdir("scan-library-named-nothing");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    for blank in ["\t", "\n "] {
        let invisible = add_root_checked_in(&conn, dir.path(), InLibrary::Named(blank))
            .expect_err("a name of only whitespace");
        assert!(
            matches!(invisible, AddRootError::Unusable(_)),
            "a name of only whitespace is refused like a blank one: {invisible:?}"
        );
    }
    let refused =
        add_root_checked_in(&conn, dir.path(), InLibrary::Named("  ")).expect_err("a blank name");

    assert_eq!(
        refused,
        AddRootError::Unusable(format!(
            "{}: a library needs a name",
            dir.path().canonicalize().expect("canonical").display()
        )),
        "a blank name is this caller's to fix, not a store fault"
    );
    assert_eq!(
        roots(&conn).expect("lists"),
        Vec::new(),
        "nothing was added"
    );
}

/// No page address holds `.` or `..` ([`crate::has_no_address`]), so no
/// directory is filed under a library of either name, and the refusal is this
/// caller's to fix rather than `libraries::create`'s inside the insert.
#[test]
fn a_dir_is_not_added_to_a_library_called_a_dot_name() {
    let dir = crate::tests::tempdir("scan-library-named-dots");
    let canonical = dir.path().canonicalize().expect("canonical");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    for name in [".", ".."] {
        assert_eq!(
            add_root_checked_in(&conn, dir.path(), InLibrary::Named(name)),
            Err(AddRootError::Unusable(format!(
                "{}: An address cannot hold “.” or “..” as a name.",
                canonical.display()
            ))),
            "{name:?}"
        );
    }
    assert_eq!(libraries_and_roots(&conn), (0, 0), "nothing was added");
}

// --- update_root -------------------------------------------------------

/// The trap `rules::update`'s own test names: a patch must change only the
/// field it names, checked against a root where every one of the five
/// nullable columns already holds a distinct, non-default value. A fixture
/// that leaves four columns at their column default cannot tell "patched
/// only what it named" from "overwrote everything with what a fresh read
/// already held."
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
/// **O92's readable half.** Applying the pattern to a walk is the rest of O92:
/// `scan_root_inner` compiles the column into an [`super::Excluder`] and
/// `measure_dir` consults it before descending.
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

/// A note of user 1's on the volume's `raw` edition, its anchor derived from
/// `slot` when written.
fn a_raw_note(id: &str, slot: u32) -> crate::Note {
    crate::Note {
        text: None,
        id: id.into(),
        edition: "raw".into(),
        slot,
        region: None,
        color: None,
        body: format!("{id}'s words"),
        created: "2026-11-13T10:00:00Z".into(),
        updated: "2026-11-13T10:00:00Z".into(),
        stale: false,
        at: None,
    }
}

fn hold(conn: &Connection, work_id: i64, notes: Vec<crate::Note>) {
    crate::state::put_notes(
        conn,
        work_id,
        1,
        &crate::Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "foo".into(),
            notes,
        },
    )
    .expect("writes the notes");
}

/// The volume across two directories, read by user 1 on both: a position on
/// c002's first page (slot 3), notes on c002's second and last pages (slots 4
/// and 6), one on c001's second page (slot 1), and one whose page was gone
/// before any removal. c002 holds one position and two notes, so a count that
/// swaps the two shows.
fn a_volume_read_on_both_directories() -> crate::tests::VolumeAcrossTwoDirectories {
    let volume = crate::tests::a_volume_across_two_directories();
    {
        let conn = volume.conn();
        crate::state::put_progress(
            &conn,
            volume.work_id,
            1,
            &crate::Progress {
                edition: "raw".into(),
                slot: 3,
            },
        )
        .expect("writes the position");
        hold(
            &conn,
            volume.work_id,
            vec![
                a_raw_note("on-c002", 4),
                a_raw_note("also-on-c002", 6),
                a_raw_note("on-c001", 1),
                a_raw_note("already-stale", 5),
            ],
        );
        conn.execute(
            "UPDATE note SET at_sha256 = 'held-by-no-member' WHERE id = 'already-stale'",
            [],
        )
        .expect("names a page no member holds");
        let anchored: i64 = conn
            .query_row(
                "SELECT (SELECT count(*) FROM progress WHERE at_sha256 IS NOT NULL)
                      + (SELECT count(*) FROM note WHERE at_sha256 IS NOT NULL)",
                [],
                |r| r.get(0),
            )
            .expect("counts the anchored rows");
        assert_eq!(anchored, 5, "the fixture: every row names its page");
        let read: Vec<(String, u32, bool)> = crate::state::notes(&conn, volume.work_id, 1)
            .expect("reads the notes")
            .notes
            .into_iter()
            .map(|note| (note.id, note.slot, note.stale))
            .collect();
        assert_eq!(
            read,
            vec![
                ("already-stale".to_string(), 5, true),
                ("also-on-c002".to_string(), 6, false),
                ("on-c001".to_string(), 1, false),
                ("on-c002".to_string(), 4, false),
            ],
            "the fixture: only the one note is stale before any removal"
        );
    }
    volume
}

/// O244: a removal counts the positions and notes on a work it keeps whose
/// page it takes, and no other. Removing b takes c002's pages. Removing a
/// takes c001's and moves c002's down three slots, where they still resolve.
/// The note whose page was gone already is counted by neither.
#[test]
fn a_removal_counts_the_rows_on_a_kept_work_whose_page_it_takes() {
    let volume = a_volume_read_on_both_directories();
    let conn = volume.conn();

    let (b, _) = removal_cost(&conn, volume.b.id).expect("costs b");
    assert_eq!(
        (b.works, b.positions_page_gone, b.notes_page_gone),
        (0, 1, 2),
        "b: c002's position and two notes"
    );
    let (a, _) = removal_cost(&conn, volume.a.id).expect("costs a");
    assert_eq!(
        (a.works, a.positions_page_gone, a.notes_page_gone),
        (0, 0, 1),
        "a: c001's note"
    );
    let (removed, _) = remove_root(&conn, volume.b.id).expect("removes b");
    assert_eq!(removed, b, "the removal answers what its rehearsal said");
}

/// A row on a work the removal takes is not a page taken from a kept work:
/// [`Removal::positions`] and [`Removal::notes`] count it.
#[test]
fn a_removal_counts_no_page_taken_on_a_work_it_removes() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let b = roots(&conn)
        .expect("lists the roots")
        .into_iter()
        .find(|root| root.path.ends_with("root-b"))
        .expect("root b");
    let beta = work_under(&conn, b.id);
    crate::state::put_progress(
        &conn,
        beta,
        1,
        &crate::Progress {
            edition: "raw".into(),
            slot: 2,
        },
    )
    .expect("writes the position on a page beta has");
    hold(&conn, beta, vec![a_raw_note("on-beta", 1)]);
    let anchored: (i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM progress WHERE work_id = ?1 AND at_sha256 IS NOT NULL),
                    (SELECT count(*) FROM note WHERE work_id = ?1 AND at_sha256 IS NOT NULL)",
            [beta],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("counts the anchored rows");
    assert_eq!(
        anchored,
        (1, 1),
        "the fixture: root b's position and note name their pages"
    );

    let (cost, _) = removal_cost(&conn, b.id).expect("costs b");

    assert_eq!(
        (
            cost.positions,
            cost.notes,
            cost.positions_page_gone,
            cost.notes_page_gone
        ),
        (1, 1, 0, 0)
    );
}

/// A library's removal counts the same as a directory's: b, moved into a
/// library of its own, goes with that library and takes c002's pages.
#[test]
fn a_librarys_removal_counts_the_rows_on_a_kept_work_whose_page_it_takes() {
    let volume = a_volume_read_on_both_directories();
    let conn = volume.conn();
    crate::relocate::move_root(
        &conn,
        volume.b.id,
        volume.b.libraries[0],
        crate::relocate::Target::New("Kids"),
    )
    .expect("moves b into a library of its own");
    let kids = library_called(&conn, "Kids");

    let (cost, _) = library_removal_cost(&conn, kids).expect("costs Kids");
    let (removed, _) = remove_library(&conn, kids).expect("removes Kids");

    let stands: i64 = conn
        .query_row(
            "SELECT count(*) FROM work WHERE id = ?1",
            [volume.work_id],
            |r| r.get(0),
        )
        .expect("reads the work");
    assert_eq!(
        (stands, cost.removed.clone()),
        (1, vec![volume.b.id]),
        "the fixture: Kids' removal takes b, and the volume's work stays with a"
    );
    assert_eq!(
        (cost.works, cost.positions_page_gone, cost.notes_page_gone),
        (0, 1, 2),
        "c002's position and two notes"
    );
    assert_eq!(removed, cost, "the removal answers what its rehearsal said");
}

/// A position on a half the layout no longer holds still finds its page, and
/// a note there does not (`resolve_position`, `resolve_note`), so a removal
/// that leaves the page whole counts the note and not the position. c002's
/// second page is a spread, flagged as edition page 4, and the flag stays on
/// page 4 when removing a moves c002's pages down three.
#[test]
fn a_removal_that_leaves_a_half_whole_counts_its_note_and_not_its_position() {
    let volume = crate::tests::a_volume_across_two_directories();
    let conn = volume.conn();
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page)
         SELECT id, 'spread', 4 FROM edition WHERE work_id = ?1 AND key = 'raw'",
        [volume.work_id],
    )
    .expect("makes c002's second page a spread");
    crate::state::put_progress(
        &conn,
        volume.work_id,
        1,
        &crate::Progress {
            edition: "raw".into(),
            slot: 5,
        },
    )
    .expect("writes the position on a half");
    hold(&conn, volume.work_id, vec![a_raw_note("on-a-half", 5)]);
    let halves: Vec<(u32, Option<String>)> = conn
        .prepare(
            "SELECT at_page, at_half FROM progress UNION ALL SELECT at_page, at_half FROM note",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        })
        .expect("reads the anchors");
    assert_eq!(
        halves,
        vec![(1, Some("left".to_string())), (1, Some("left".to_string()))],
        "the fixture: both on the left half of c002's second page"
    );

    let (cost, _) = removal_cost(&conn, volume.a.id).expect("costs a");

    assert_eq!(
        (cost.works, cost.positions_page_gone, cost.notes_page_gone),
        (0, 0, 1),
        "the note's half is gone; the position's page is not"
    );
}

/// Two works with one edition key, each read on a page directory b takes:
/// each row is found in its own work's layout, so all four count.
#[test]
fn a_removal_counts_the_pages_it_takes_from_two_works_with_one_edition_key() {
    let volume = crate::tests::a_volume_across_two_directories();
    let conn = volume.conn();
    write_archive(&volume.a.path.join("Bar").join("Bar - v01 - c001.cbz"), 2);
    write_archive(&volume.b.path.join("Bar").join("Bar - v01 - c002.cbz"), 2);
    scan_root(&conn, &volume.a).expect("scans a again");
    scan_root(&conn, &volume.b).expect("scans b again");
    crate::derive::derive(&conn).expect("derives Bar beside Foo");
    let bar: i64 = conn
        .query_row(
            "SELECT DISTINCT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE m.path LIKE '%Bar - v01 - c002.cbz'",
            [],
            |r| r.get(0),
        )
        .expect("Bar's work");
    let mut shape: Vec<(i64, String, i64)> = conn
        .prepare(
            "SELECT e.work_id, e.key, count(*) FROM member m JOIN edition e ON e.id = m.edition_id
              GROUP BY e.work_id, e.key",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect()
        })
        .expect("lists the editions");
    shape.sort_unstable();
    let mut wanted = vec![
        (volume.work_id, "raw".to_string(), 2),
        (bar, "raw".to_string(), 2),
    ];
    wanted.sort_unstable();
    assert_eq!(
        shape, wanted,
        "the fixture: Foo and Bar, each a `raw` edition of two chapters"
    );
    for (work, position, note) in [(volume.work_id, 3, 4), (bar, 2, 3)] {
        crate::state::put_progress(
            &conn,
            work,
            1,
            &crate::Progress {
                edition: "raw".into(),
                slot: position,
            },
        )
        .expect("writes a position on c002");
        hold(&conn, work, vec![a_raw_note(&format!("on-{work}"), note)]);
    }

    let (cost, _) = removal_cost(&conn, volume.b.id).expect("costs b");

    assert_eq!(
        (cost.works, cost.positions_page_gone, cost.notes_page_gone),
        (0, 2, 2),
        "each work's c002 position and note"
    );
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

/// O320: a removal's derives freeze a paused directory as every derive does,
/// so the removal of another directory hashes none of its files and writes
/// none of its works.
#[test]
fn removing_another_directory_hashes_none_of_a_paused_one() {
    let (t_dir, r_dir) = (
        crate::tests::tempdir("remove-other-t"),
        crate::tests::tempdir("remove-other-r"),
    );
    write_archive(&t_dir.path().join("Beta").join("Beta v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let t = add_root(&conn, t_dir.path()).expect("adds T");
    scan_root(&conn, &t).expect("scans T");
    crate::derive::derive(&conn).expect("derives T");
    write_archive(
        &r_dir.path().join("Alpha").join("Alpha - v01 - c001.cbz"),
        2,
    );
    write_archive(
        &r_dir.path().join("Alpha").join("Alpha - v01 - c002.cbz"),
        3,
    );
    let r = add_root(&conn, r_dir.path()).expect("adds R");
    scan_root(&conn, &r).expect("scans R, no derive");
    pause(&conn, std::slice::from_ref(&r)).expect("pauses R");
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |row| row.get(0)).expect("counts") };
    let r_digests = format!(
        "SELECT count(*) FROM scan_entry WHERE root_id = {} AND sha256 IS NOT NULL",
        r.id
    );
    assert_eq!(
        (count(&r_digests), count("SELECT count(*) FROM work")),
        (0, 1),
        "the fixture: R walked and not hashed, T's one work"
    );

    remove_root(&conn, t.id).expect("removes T");

    assert_eq!(
        (count(&r_digests), count("SELECT count(*) FROM work")),
        (0, 0),
        "(R's digests, works)"
    );
}

/// `remove_root` must send `DELETE FROM root` *inside* an already-open
/// transaction, one opened before the delete and still open when it is sent,
/// not as a bare, auto-committed statement. A removal split across two
/// transactions passes this test and fails
/// `a_root_removal_that_fails_in_its_derive_removes_nothing`. A `BEGIN` earlier
/// in the trace proves nothing: the removal first calls `roots`, which opens
/// and rolls back a `BEGIN DEFERRED` wherever no transaction is open.
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
    let mut conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;

    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(watch),
    )
    .expect("installs the trace");
    crate::scan::remove_root(&conn, root).expect("removing the root");
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::empty(),
        None::<fn(rusqlite::trace::TraceEvent<'_>)>,
    )
    .expect("clears the trace");

    let statements = STATEMENTS.lock().unwrap();
    let delete_root_index = statements
        .iter()
        .position(|sql| sql.contains("DELETE FROM root"))
        .expect("apply_removal must have sent DELETE FROM root");
    // The transaction state the statements before the delete leave, replayed:
    // `BEGIN` opens a transaction, `SAVEPOINT` opens or nests one, `RELEASE`
    // ends the innermost savepoint and any transaction it opened, `ROLLBACK TO`
    // leaves the transaction open, and `COMMIT`, `END` or a plain `ROLLBACK`
    // end it. SQLite's `RELEASE` also ends the savepoints inside the one it
    // names, and `ROLLBACK TO` cancels them; a depth is enough here because
    // only `Nested` sends them, each naming its own scope, the innermost when
    // it closes.
    let mut depth = 0_usize;
    for sql in &statements[..delete_root_index] {
        let head = sql.trim_start().to_ascii_uppercase();
        if head.starts_with("BEGIN") {
            depth = depth.max(1);
        } else if head.starts_with("SAVEPOINT") {
            depth += 1;
        } else if head.starts_with("RELEASE") {
            depth = depth.saturating_sub(1);
        } else if head.starts_with("COMMIT")
            || head.starts_with("END")
            || (head.starts_with("ROLLBACK") && !head.split_whitespace().any(|word| word == "TO"))
        {
            depth = 0;
        }
    }

    assert!(
        depth > 0,
        "DELETE FROM root must be sent while a transaction opened before it is still open, \
         and replaying the statements sent before it leaves none open: {statements:?}"
    );
}

/// A removal's writes and its derive commit as one, so a removal that fails in
/// the derive leaves every row as it found it. Removing Manga hands Kids, nested
/// in it, Manga's entry for `kids/extras`, which Kids' pattern, narrowed with no
/// scan since, now reaches, and Bonus's member (`hand_back`); then it deletes
/// Alpha's work, Manga's rows and its library with Pat's grant (`take_root`); the
/// derive then fails marking Bonus's work missing, at a trigger that fires only
/// once Manga's row is gone. A commit anywhere among those writes leaves part of
/// the removal standing.
#[test]
fn a_root_removal_that_fails_in_its_derive_removes_nothing() {
    let (_top, library, manga) = manga_holding(
        "remove-root-fails-in-derive",
        &[
            ("Alpha/Alpha v01.cbz", 3),
            ("kids/Bleach/v01.cbz", 3),
            ("kids/extras/Bonus/Bonus v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    let mut kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    set_patterns(&conn, &mut kids, Some("extras/"));
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    an_account_granted(&conn, "pat", "user", &["Manga"]);
    set_patterns(&conn, &mut kids, Some("extras/Bonus/"));
    conn.execute_batch(&format!(
        "CREATE TEMP TRIGGER planted BEFORE UPDATE OF missing_since ON main.work
           WHEN NOT EXISTS (SELECT 1 FROM main.root WHERE id = {})
           BEGIN SELECT RAISE(ABORT, 'planted'); END",
        manga.id
    ))
    .expect("plants the fault");
    let before = what_a_removal_touches(&conn);

    let why = remove_root(&conn, manga.id).expect_err("the derive fails");

    assert!(why.contains("planted"), "{why}");
    let after = what_a_removal_touches(&conn);
    let of = |rows: &[String], kind: &str| -> Vec<String> {
        let kind = format!("{kind} ");
        rows.iter()
            .filter(|row| row.starts_with(&kind))
            .cloned()
            .collect()
    };
    let row_kinds = [
        "root",
        "link",
        "library",
        "grant",
        "scan_entry",
        "member",
        "work",
    ];
    let changed: Vec<&str> = row_kinds
        .into_iter()
        .filter(|kind| of(&before, kind) != of(&after, kind))
        .collect();
    assert_eq!(
        changed,
        Vec::<&str>::new(),
        "the kinds of row the failed removal changed; before: {before:#?}, after: {after:#?}"
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

/// The case a plain `BEGIN` cannot survive: a caller that already holds an open
/// transaction. `removal_cost` and `remove_root` open [`crate::Nested`] rather
/// than `unchecked_transaction` directly, so this pins `Nested`'s rollback
/// path — a `ROLLBACK TO SAVEPOINT` folded back into the caller's still-open
/// transaction, not a real `ROLLBACK` — which must still leave the database
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

/// **The positive twin of the test just above.** `removal_cost` and
/// `remove_root` both run `apply_removal`, which derives under
/// `Documents::Skip` and so cannot itself have written the repair — proven
/// above by `removal_cost`, whose own transaction always rolls back, leaving
/// the gap exactly as it found it. `remove_root` commits instead, and must
/// actually repair the gap for real: it runs a second, real
/// `Documents::Reconcile` derive afterward (see that function's own doc), so
/// the repair the O82 retry promises still happens on a real removal.
///
/// **The trace additionally proves *when*.** A `Reconcile` pass inside the
/// same transaction `DELETE FROM root` uses would, on any `Err` after the
/// repair wrote the file, roll the database back while leaving the filesystem
/// moved. Tracing the same way
/// `remove_root_sends_delete_from_root_inside_an_already_open_transaction`
/// does, this pins that the repair's own derive pass opens its own
/// transaction only *after* `remove_root`'s own `COMMIT` — never before it.
#[test]
fn remove_root_repairs_a_surviving_documents_gap_only_after_its_own_commit() {
    use std::sync::Mutex;

    let dir = crate::tests::tempdir("scan-remove-root-reconcile");
    let library = Library::open_in_memory().expect("library");
    let mut conn = library.conn();
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
    )
    .expect("installs the trace");
    crate::scan::remove_root(&conn, root_b.id).expect("removing root b");
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::empty(),
        None::<fn(rusqlite::trace::TraceEvent<'_>)>,
    )
    .expect("clears the trace");

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

// --- removal, and an account's only grant --------------------------------

/// Three libraries over real directories, each directory holding one volume:
/// `Shelf` of two directories, then `Solo` and `Lone` of one each, so that the
/// last two have a library id that is not their directory's id. Answers the
/// roots in that order.
fn three_libraries(parent: &Path, conn: &Connection) -> [Root; 4] {
    let mut added = Vec::new();
    for (name, library, series) in [
        ("shelf-a", "Shelf", "Alpha"),
        ("shelf-b", "Shelf", "Gamma"),
        ("solo", "Solo", "Beta"),
        ("lone", "Lone", "Delta"),
    ] {
        let dir = parent.join(name);
        write_archive(
            &dir.join(series).join(format!("{series} - v01.cbz")),
            2 + added.len(),
        );
        let root = add_root_in(conn, &dir, InLibrary::Named(library)).expect("adds the directory");
        scan_root(conn, &root).expect("scans it");
        added.push(root);
    }
    crate::derive::derive(conn).expect("derives");
    for root in &added[2..] {
        assert_ne!(
            root.id, root.libraries[0],
            "the fixture must tell a directory's id from its library's: {root:?}"
        );
    }
    added.try_into().expect("four roots")
}

/// A new account called `name`, granted each of `libraries` by name.
fn an_account_granted(conn: &Connection, name: &str, role: &str, libraries: &[&str]) -> i64 {
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES (?1, 'x', ?2)",
        [name, role],
    )
    .expect("adds the account");
    let id = conn.last_insert_rowid();
    for library in libraries {
        let library = crate::libraries::id_of(conn, library)
            .expect("reads")
            .expect("the library exists");
        crate::account::grant_library(conn, id, library).expect("grants");
    }
    id
}

/// The roots, libraries, works, members, scan entries and grants, as text.
fn what_a_removal_touches(conn: &Connection) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'root', id, path, NULL FROM root ORDER BY id",
        "SELECT 'link', library_id, root_id, NULL FROM library_root ORDER BY 2, 3",
        "SELECT 'library', id, name, NULL FROM library ORDER BY id",
        "SELECT 'work', id, slug, NULL FROM work ORDER BY id",
        "SELECT 'member', id, path, root_id FROM member ORDER BY id",
        "SELECT 'scan_entry', id, rel_path, root_id FROM scan_entry ORDER BY id",
        "SELECT 'grant', user_id, library_id, NULL FROM user_library ORDER BY 2, 3",
    ] {
        let mut statement = conn.prepare(sql).expect(sql);
        let read = statement
            .query_map([], |r| {
                Ok(format!(
                    "{} {} {:?} {:?}",
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, rusqlite::types::Value>(2)?,
                    r.get::<_, rusqlite::types::Value>(3)?,
                ))
            })
            .expect(sql)
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect(sql);
        rows.extend(read);
    }
    rows
}

/// `Solo` is the only grant of `alex` and of `sam`, so removing its one
/// directory deletes it and leaves them seeing no library. The dry run names
/// them, in name order, and the removal does what the dry run said.
///
/// No other account loses its last library, and none is named: `zed` is an
/// administrator, which sees every library whatever its rows say; `kim` is
/// set to every library, though it keeps a row of `Solo`; `both` keeps
/// `Shelf`; `pat` never held `Solo`.
#[test]
fn removing_the_last_dir_of_an_accounts_only_library_names_who_is_left_with_none() {
    let dir = crate::tests::tempdir("scan-removal-only-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [shelf_a, shelf_b, solo, _] = three_libraries(dir.path(), &conn);
    let sam = an_account_granted(&conn, "sam", "user", &["Solo"]);
    let alex = an_account_granted(&conn, "alex", "user", &["Solo"]);
    an_account_granted(&conn, "zed", "admin", &["Solo"]);
    let kim = an_account_granted(&conn, "kim", "user", &["Solo"]);
    conn.execute("UPDATE user SET every_library = 1 WHERE id = ?1", [kim])
        .expect("kim sees every library and keeps its row of Solo");
    let both = an_account_granted(&conn, "both", "user", &["Solo", "Shelf"]);
    an_account_granted(&conn, "pat", "user", &["Shelf"]);
    let before = what_a_removal_touches(&conn);

    let (cost, _) = removal_cost(&conn, solo.id).expect("the dry run goes ahead");
    assert_eq!(cost.left_with_no_library, vec!["alex", "sam"], "{cost:?}");
    assert_eq!(
        what_a_removal_touches(&conn),
        before,
        "the dry run removes nothing"
    );

    let (removal, _) = remove_root(&conn, solo.id).expect("the removal goes ahead");

    assert_eq!(removal, cost, "the removal does what the dry run said");
    assert_eq!(crate::libraries::id_of(&conn, "Solo").expect("reads"), None);
    for account in [alex, sam] {
        sees_no_library(&conn, account);
    }
    assert_eq!(
        crate::account::access_for(&conn, both).expect("reads"),
        crate::account::Access::Only {
            roots: vec![shelf_a.id, shelf_b.id],
            libraries: vec![shelf_a.libraries[0]],
        },
        "the account that held another library keeps it"
    );
}

/// `account` is "only these" with no directory, and no series is listed to it,
/// though the libraries left still hold series.
fn sees_no_library(conn: &Connection, account: i64) {
    let access = crate::account::access_for(conn, account).expect("reads");
    assert_eq!(
        access,
        crate::account::Access::roots(Vec::new()),
        "account {account} sees no library, not every library"
    );
    assert_eq!(
        crate::browse::series(conn, &access, account, None).expect("lists"),
        Vec::new(),
        "account {account} is listed nothing"
    );
    assert!(
        !crate::browse::series(conn, &crate::account::Access::Every, account, None)
            .expect("lists")
            .is_empty(),
        "the fixture: the libraries left hold series"
    );
}

/// The same removal once the account holds another library too: the directory
/// goes, and its library and that grant with it, and no one is named.
#[test]
fn removing_the_last_dir_of_a_library_is_allowed_when_its_accounts_hold_another() {
    let dir = crate::tests::tempdir("scan-removal-second-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [shelf_a, shelf_b, solo, _] = three_libraries(dir.path(), &conn);
    let alex = an_account_granted(&conn, "alex", "user", &["Solo", "Shelf"]);

    let (removal, _) = remove_root(&conn, solo.id).expect("alex keeps Shelf");

    assert_eq!(removal.works, 1, "Solo's one volume went with it");
    assert_eq!(removal.left_with_no_library, Vec::<String>::new());
    assert_eq!(crate::libraries::id_of(&conn, "Solo").expect("reads"), None);
    assert_eq!(
        crate::account::access_for(&conn, alex).expect("reads"),
        crate::account::Access::Only {
            roots: vec![shelf_a.id, shelf_b.id],
            libraries: vec![shelf_a.libraries[0]],
        },
        "still restricted, to the library that is left"
    );
}

/// The same where the other library was made after the one removed, so its id
/// is the higher of the two.
#[test]
fn removing_the_last_dir_of_a_library_is_allowed_when_its_accounts_hold_a_later_one() {
    let dir = crate::tests::tempdir("scan-removal-later-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [_, _, solo, lone] = three_libraries(dir.path(), &conn);
    assert!(
        solo.libraries[0] < lone.libraries[0],
        "the fixture: Lone is the later library"
    );
    let alex = an_account_granted(&conn, "alex", "user", &["Solo", "Lone"]);

    let (removal, _) = remove_root(&conn, solo.id).expect("alex keeps Lone");

    assert_eq!(removal.left_with_no_library, Vec::<String>::new());
    assert_eq!(crate::libraries::id_of(&conn, "Solo").expect("reads"), None);
    assert_eq!(
        crate::account::access_for(&conn, alex).expect("reads"),
        crate::account::Access::Only {
            roots: vec![lone.id],
            libraries: vec![lone.libraries[0]],
        },
        "still restricted, to the library that is left"
    );
}

/// The same removal where no account is restricted to `Solo`, though one is
/// restricted to `Lone`, another library of one directory.
#[test]
fn removing_the_last_dir_of_a_library_nobody_is_restricted_to_is_allowed() {
    let dir = crate::tests::tempdir("scan-removal-no-only-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [_, _, solo, lone] = three_libraries(dir.path(), &conn);
    let pat = an_account_granted(&conn, "pat", "user", &["Lone"]);

    let (removal, _) = remove_root(&conn, solo.id).expect("nobody is restricted to Solo");

    assert_eq!(removal.works, 1);
    assert_eq!(removal.left_with_no_library, Vec::<String>::new());
    assert_eq!(
        crate::account::access_for(&conn, pat).expect("reads"),
        crate::account::Access::Only {
            roots: vec![lone.id],
            libraries: vec![lone.libraries[0]],
        },
        "and the account restricted elsewhere is where it was"
    );
}

/// A directory that is not its library's last goes, though the library is an
/// account's only grant: the library and the grant stay, so neither the dry
/// run nor the removal names the account.
#[test]
fn removing_one_dir_of_two_names_no_one_though_the_library_is_an_only_grant() {
    let dir = crate::tests::tempdir("scan-removal-not-last-dir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [shelf_a, shelf_b, _, _] = three_libraries(dir.path(), &conn);
    let pat = an_account_granted(&conn, "pat", "user", &["Shelf"]);

    let (cost, _) = removal_cost(&conn, shelf_a.id).expect("costs");
    let (removal, _) = remove_root(&conn, shelf_a.id).expect("Shelf keeps a directory");

    for (which, answer) in [("the dry run", cost), ("the removal", removal)] {
        assert_eq!(answer.works, 1, "{which}: the directory's one volume");
        assert_eq!(
            answer.left_with_no_library,
            Vec::<String>::new(),
            "{which}: pat still sees Shelf"
        );
    }
    assert_eq!(
        crate::account::access_for(&conn, pat).expect("reads"),
        crate::account::Access::Only {
            roots: vec![shelf_b.id],
            libraries: vec![shelf_b.libraries[0]],
        },
        "the grant resolves to the directory that is left"
    );
}

fn library_called(conn: &Connection, name: &str) -> i64 {
    crate::libraries::id_of(conn, name)
        .expect("reads")
        .expect("the library exists")
}

/// The one work with a member under directory `root_id`.
fn work_under(conn: &Connection, root_id: i64) -> i64 {
    conn.query_row(
        "SELECT DISTINCT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE m.root_id = ?1",
        [root_id],
        |r| r.get(0),
    )
    .expect("one work under the directory")
}

/// `account` has a reading position on `work`, and `notes` notes.
fn read_with_notes(conn: &Connection, account: i64, work: i64, notes: usize) {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (?1, ?2, 'en', 1)",
        [account, work],
    )
    .expect("a position");
    for n in 0..notes {
        conn.execute(
            "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
             VALUES (?1, ?2, ?3, 'en', 1, 'x', 'now', 'now')",
            rusqlite::params![account, format!("note-{account}-{work}-{n}"), work],
        )
        .expect("a note");
    }
}

/// What is left of a library's footprint in the store, as text.
fn rows_of_library(conn: &Connection, library: i64) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'library', id FROM library WHERE id = ?1",
        "SELECT 'root', root_id FROM library_root WHERE library_id = ?1",
        "SELECT 'grant', user_id FROM user_library WHERE library_id = ?1",
    ] {
        let mut statement = conn.prepare(sql).expect(sql);
        let read = statement
            .query_map([library], |r| {
                Ok(format!(
                    "{} {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?
                ))
            })
            .expect(sql)
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect(sql);
        rows.extend(read);
    }
    rows
}

#[test]
fn removing_a_library_takes_each_dirs_works_and_the_library() {
    let parent = crate::tests::tempdir("remove-library");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    let shelf = library_called(&conn, "Shelf");
    an_account_granted(&conn, "pat", "user", &["Shelf", "Solo"]);

    let (removal, _derived) = remove_library(&conn, shelf).expect("removes");

    assert_eq!(
        removal.works, 2,
        "one work under each of Shelf's directories"
    );
    assert_eq!(
        crate::libraries::name_of(&conn, shelf).expect("reads"),
        None
    );
    assert_eq!(rows_of_library(&conn, shelf), Vec::<String>::new());
    let left: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT l.name FROM work w
               JOIN edition e ON e.work_id = w.id JOIN member m ON m.edition_id = e.id
               JOIN library_root r ON r.root_id = m.root_id JOIN library l ON l.id = r.library_id
              ORDER BY 1",
        )
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(
        left,
        vec!["Lone", "Solo"],
        "the other libraries' works stand"
    );
    for (table, held) in [("work", 2), ("member", 2), ("root", 2)] {
        assert_eq!(
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .expect("counts"),
            held,
            "the {table} rows of the libraries that stand, and no more"
        );
    }
    assert_eq!(
        conn.query_row("SELECT count(*) FROM user_library", [], |r| r
            .get::<_, i64>(0))
            .expect("counts"),
        1,
        "the account keeps its grant of the library that stands"
    );
}

/// Works, positions and notes each sum across the directories, with a different
/// figure for each so that no two can be swapped for one another.
#[test]
fn removing_a_library_sums_works_positions_and_notes_across_its_dirs() {
    let parent = crate::tests::tempdir("remove-library-sums");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [shelf_a, shelf_b, _, _] = three_libraries(parent.path(), &conn);
    let (pat, sam) = (
        an_account_granted(&conn, "pat", "user", &[]),
        an_account_granted(&conn, "sam", "user", &[]),
    );
    let (alpha, gamma) = (work_under(&conn, shelf_a.id), work_under(&conn, shelf_b.id));
    read_with_notes(&conn, pat, alpha, 2);
    read_with_notes(&conn, pat, gamma, 3);
    read_with_notes(&conn, sam, gamma, 0);

    let (cost, _derived) =
        library_removal_cost(&conn, library_called(&conn, "Shelf")).expect("costs");

    assert_eq!(
        (cost.works, cost.positions, cost.notes),
        (2, 3, 5),
        "{cost:?}"
    );
}

/// A library `Manga` of two directories, `hub` and `other`, made in `order` so
/// that the one with the lower directory id differs between the two orders.
///
/// Each holds `Series` volume 1, which is one work across the two directories.
/// With `an_edition_per_dir` the first directory made is given the edition
/// key `en`, and the work has two editions, one under each directory; without,
/// it has one edition holding a member under each directory. `hub` also holds
/// a work of its own, `Extra`. Positions and notes are put on both works,
/// with a different figure for each of works, positions and notes.
///
/// Answers what the library's dry run and its removal each counted, after
/// asserting the fixture's own shape and that each directory's own dry run
/// counts only what that directory alone accounts for.
fn a_library_removed_across_two_dirs(order: [usize; 2], an_edition_per_dir: bool) -> Removal {
    let parent = crate::tests::tempdir("remove-library-spanning");
    let dirs = [parent.path().join("hub"), parent.path().join("other")];
    let pages = if an_edition_per_dir { [2, 3] } else { [2, 2] };
    for (dir, pages) in dirs.iter().zip(pages) {
        write_archive(&dir.join("Series").join("Series - v01.cbz"), pages);
    }
    write_archive(&dirs[0].join("Extra").join("Extra - v01.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let made = create_library(
        &conn,
        "Manga",
        &[dirs[order[0]].clone(), dirs[order[1]].clone()],
        None,
    )
    .expect("creates");
    if an_edition_per_dir {
        update_root(
            &conn,
            made.dirs[0].root.id,
            &RootPatch {
                edition_key: Some(Some("en".to_string())),
                ..Default::default()
            },
        )
        .expect("the first directory's volume is its own edition");
    }
    for root in roots(&conn).expect("lists") {
        scan_root(&conn, &root).expect("scans");
    }
    crate::derive::derive(&conn).expect("derives");

    let hub = made.dirs[order.iter().position(|&i| i == 0).expect("hub")]
        .root
        .id;
    let ids = |sql: &str| -> Vec<i64> {
        conn.prepare(sql)
            .expect("prepares")
            .query_map([], |r| r.get(0))
            .expect("reads")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    let spanning = ids(
        "SELECT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
                         GROUP BY e.work_id HAVING count(DISTINCT m.root_id) = 2",
    );
    assert_eq!(
        spanning.len(),
        1,
        "the fixture: one work across the two directories"
    );
    let editions: i64 = conn
        .query_row(
            "SELECT count(DISTINCT e.id) FROM edition e WHERE e.work_id = ?1",
            [spanning[0]],
            |r| r.get(0),
        )
        .expect("counts");
    assert_eq!(
        editions,
        if an_edition_per_dir { 2 } else { 1 },
        "the fixture's editions of the work"
    );
    assert_eq!(
        ids("SELECT count(*) FROM work"),
        vec![2],
        "the fixture: the spanning work and `Extra`"
    );
    let extra = work_under(&conn, hub);
    assert_ne!(extra, spanning[0], "the fixture: `Extra` is `hub`'s alone");

    let (pat, sam) = (
        an_account_granted(&conn, "pat", "user", &[]),
        an_account_granted(&conn, "sam", "user", &[]),
    );
    read_with_notes(&conn, pat, spanning[0], 2);
    read_with_notes(&conn, sam, spanning[0], 0);
    read_with_notes(&conn, pat, extra, 4);
    for dir in &made.dirs {
        let alone = removal_cost(&conn, dir.root.id).expect("costs").0;
        assert_eq!(
            alone.works,
            usize::from(dir.root.id == hub),
            "the fixture: a directory alone accounts for `Extra` if it is `hub`, and never for \
             the spanning work"
        );
    }

    let (cost, _derived) = library_removal_cost(&conn, made.library_id).expect("costs");
    let (removed, _derived) = remove_library(&conn, made.library_id).expect("removes");
    assert_eq!(removed, cost, "the dry run is what the removal then does");
    for table in ["work", "edition", "member", "progress", "note"] {
        assert_eq!(
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .expect("counts"),
            0,
            "no {table} row is left"
        );
    }
    cost
}

/// A work with an edition under each of a library's two directories: neither
/// directory alone accounts for it, so each directory's own dry run counts none of
/// it, and the library's counts it once, whichever directory is taken first.
#[test]
fn a_library_removal_counts_a_work_with_an_edition_per_dir_once() {
    let forward = a_library_removed_across_two_dirs([0, 1], true);

    assert_eq!(
        (forward.works, forward.positions, forward.notes),
        (2, 3, 6),
        "{forward:?}"
    );
    assert_eq!(
        a_library_removed_across_two_dirs([1, 0], true),
        forward,
        "whichever directory is taken first"
    );
}

/// A work with one edition holding a member under each of a library's two
/// directories: the same count, by the same rule, for a different shape.
#[test]
fn a_library_removal_counts_a_work_with_one_edition_in_both_dirs_once() {
    let forward = a_library_removed_across_two_dirs([0, 1], false);

    assert_eq!(
        (forward.works, forward.positions, forward.notes),
        (2, 3, 6),
        "{forward:?}"
    );
    assert_eq!(
        a_library_removed_across_two_dirs([1, 0], false),
        forward,
        "whichever directory is taken first"
    );
}

/// The first directory's work has a position and a note, and the second directory's
/// removal is made to fail. Neither the dry run nor the removal may leave the
/// first directory's removal behind.
#[test]
fn a_library_removal_that_fails_part_way_removes_nothing() {
    let parent = crate::tests::tempdir("remove-library-part-way");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [shelf_a, shelf_b, _, _] = three_libraries(parent.path(), &conn);
    let pat = an_account_granted(&conn, "pat", "user", &[]);
    read_with_notes(&conn, pat, work_under(&conn, shelf_a.id), 2);
    conn.execute_batch(&format!(
        "CREATE TEMP TRIGGER planted BEFORE DELETE ON main.root
           WHEN old.id = {} BEGIN SELECT RAISE(ABORT, 'planted'); END",
        shelf_b.id
    ))
    .expect("plants the fault");
    let stands = |conn: &Connection| {
        let mut rows = what_a_removal_touches(conn);
        for table in ["progress", "note"] {
            rows.push(format!(
                "{table} {}",
                conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .expect("counts")
            ));
        }
        rows
    };
    let before = stands(&conn);
    let shelf = library_called(&conn, "Shelf");

    let costed = library_removal_cost(&conn, shelf).expect_err("the second directory fails");
    assert!(costed.contains("planted"), "{costed}");
    assert_eq!(stands(&conn), before, "after the dry run");
    let removed = remove_library(&conn, shelf).expect_err("the second directory fails");
    assert!(removed.contains("planted"), "{removed}");
    assert_eq!(stands(&conn), before, "after the removal");
}

#[test]
fn the_cost_of_removing_a_library_leaves_the_database_as_it_found_it() {
    let parent = crate::tests::tempdir("remove-library-cost");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    let before = what_a_removal_touches(&conn);

    let (cost, _derived) =
        library_removal_cost(&conn, library_called(&conn, "Shelf")).expect("costs");

    assert_eq!(cost.works, 2);
    assert_eq!(what_a_removal_touches(&conn), before);
}

/// An id that names no library is not an error and takes nothing.
#[test]
fn a_library_id_that_names_no_library_removes_nothing() {
    let parent = crate::tests::tempdir("remove-library-none");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    // The derive that follows rewrites member rows under new ids, so what is
    // compared is everything else, and the members by path and root.
    let stands = |conn: &Connection| -> Vec<String> {
        let mut rows: Vec<String> = what_a_removal_touches(conn)
            .into_iter()
            .filter(|row| !row.starts_with("member "))
            .collect();
        let mut members = conn
            .prepare("SELECT path, root_id FROM member ORDER BY path")
            .expect("prepares")
            .query_map([], |r| {
                Ok(format!(
                    "{} {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?
                ))
            })
            .expect("reads")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("reads");
        rows.append(&mut members);
        rows
    };
    let before = stands(&conn);

    assert_eq!(
        library_removal_cost(&conn, 999).expect("costs").0,
        Removal::default()
    );
    assert_eq!(
        remove_library(&conn, 999).expect("removes").0,
        Removal::default()
    );
    assert_eq!(stands(&conn), before);
}

/// `Shelf`, of two directories, is the only grant of `alex` and of `bo`: removing
/// it leaves them seeing no library, and the dry run names them, in name
/// order. `both` keeps `Solo` and is not named.
#[test]
fn removing_a_library_that_is_an_accounts_only_grant_names_who_is_left_with_none() {
    let parent = crate::tests::tempdir("remove-library-only-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [_, _, solo, _] = three_libraries(parent.path(), &conn);
    let bo = an_account_granted(&conn, "bo", "user", &["Shelf"]);
    let alex = an_account_granted(&conn, "alex", "user", &["Shelf"]);
    let both = an_account_granted(&conn, "both", "user", &["Shelf", "Solo"]);
    let shelf = library_called(&conn, "Shelf");
    let before = what_a_removal_touches(&conn);

    let (cost, _) = library_removal_cost(&conn, shelf).expect("the dry run goes ahead");
    assert_eq!(cost.left_with_no_library, vec!["alex", "bo"], "{cost:?}");
    assert_eq!(
        what_a_removal_touches(&conn),
        before,
        "the dry run removes nothing"
    );

    let (removal, _) = remove_library(&conn, shelf).expect("the removal goes ahead");

    assert_eq!(removal, cost, "the removal does what the dry run said");
    assert_eq!(
        crate::libraries::name_of(&conn, shelf).expect("reads"),
        None
    );
    for account in [alex, bo] {
        sees_no_library(&conn, account);
    }
    assert_eq!(
        crate::account::access_for(&conn, both).expect("reads"),
        crate::account::Access::Only {
            roots: vec![solo.id],
            libraries: vec![solo.libraries[0]],
        },
        "the account that held another library keeps it"
    );
}

/// An administrator sees every library whatever its rows say, so removing
/// the only library it is granted leaves it with nothing to lose: the dry
/// run and the removal name no one, and the library goes.
#[test]
fn an_administrators_only_grant_is_not_named_and_the_library_goes() {
    let parent = crate::tests::tempdir("remove-library-admin");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    an_account_granted(&conn, "ada", "admin", &["Shelf"]);
    let shelf = library_called(&conn, "Shelf");

    let (cost, _) = library_removal_cost(&conn, shelf).expect("costs");
    let (removal, _) = remove_library(&conn, shelf).expect("removes");

    for (which, answer) in [("the dry run", cost), ("the removal", removal)] {
        assert_eq!(answer.works, 2, "{which}: Shelf's two volumes");
        assert_eq!(
            answer.left_with_no_library,
            Vec::<String>::new(),
            "{which}: an administrator is never left with no library"
        );
    }
    assert_eq!(
        crate::libraries::name_of(&conn, shelf).expect("reads"),
        None
    );
}

/// An account holding two libraries is left with one when either goes, so
/// removing the later-made names no one. Its grant of the earlier-made is
/// then its only one, and removing that names it.
#[test]
fn a_library_that_is_not_an_accounts_only_grant_names_no_one_whichever_was_made_first() {
    let parent = crate::tests::tempdir("remove-library-not-only");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    let dual = an_account_granted(&conn, "dual", "user", &["Shelf", "Solo"]);
    let (shelf, solo) = (
        library_called(&conn, "Shelf"),
        library_called(&conn, "Solo"),
    );
    assert!(shelf < solo, "the fixture: Shelf is the earlier-made");

    for library in [shelf, solo] {
        assert_eq!(
            library_removal_cost(&conn, library)
                .expect("costs")
                .0
                .left_with_no_library,
            Vec::<String>::new(),
            "library {library}"
        );
    }
    let (removal, _) = remove_library(&conn, solo).expect("the later-made goes");
    assert_eq!(removal.left_with_no_library, Vec::<String>::new());

    assert!(
        matches!(
            crate::account::access_for(&conn, dual).expect("reads"),
            crate::account::Access::Only { roots: dirs, .. } if dirs.len() == 2
        ),
        "the account keeps Shelf's two directories"
    );
    assert_eq!(
        library_removal_cost(&conn, shelf)
            .expect("costs")
            .0
            .left_with_no_library,
        vec!["dual"],
        "and Shelf is now its only grant"
    );
}

/// A library with no directory goes only with its last directory, so a removal of
/// it deletes nothing and names no one, though it is an account's only
/// grant. Written by hand: nothing public makes such a library.
#[test]
fn removing_a_library_with_no_dir_names_no_one() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO library (id, name) VALUES (90, 'Directoryless')",
        [],
    )
    .expect("a library with no directory");
    let alex = an_account_granted(&conn, "alex", "user", &["Directoryless"]);

    let (cost, _) = library_removal_cost(&conn, 90).expect("costs");
    let (removal, _) = remove_library(&conn, 90).expect("removes");

    for (which, answer) in [("the dry run", cost), ("the removal", removal)] {
        assert_eq!(answer, Removal::default(), "{which}");
    }
    assert_eq!(
        crate::libraries::name_of(&conn, 90).expect("reads"),
        Some("Directoryless".to_string()),
        "the library stands"
    );
    assert_eq!(
        crate::account::libraries_for(&conn, alex).expect("reads"),
        vec![90],
        "and so does alex's grant of it"
    );
}

/// What each removal reports of the derive that follows it, over a library
/// whose figures differ from one another.
#[test]
fn a_library_removal_reports_the_derive_that_follows_it() {
    let parent = crate::tests::tempdir("remove-library-report");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for volume in ["v01", "v02", "v03"] {
        write_archive(
            &parent
                .path()
                .join("kept")
                .join("Kept")
                .join(format!("Kept - {volume}.cbz")),
            2,
        );
    }
    write_archive(
        &parent
            .path()
            .join("gone")
            .join("Gone")
            .join("Gone - v01.cbz"),
        2,
    );
    let kept = create_library(&conn, "Kept", &[parent.path().join("kept")], None).expect("creates");
    let gone = create_library(&conn, "Gone", &[parent.path().join("gone")], None).expect("creates");
    for root in roots(&conn).expect("lists") {
        scan_root(&conn, &root).expect("scans");
    }
    crate::derive::derive(&conn).expect("derives");
    assert_ne!(kept.library_id, gone.library_id);

    let (_, predicted) = library_removal_cost(&conn, gone.library_id).expect("costs");
    let (_, derived) = remove_library(&conn, gone.library_id).expect("removes");

    for (which, report) in [("the dry run", predicted), ("the removal", derived)] {
        assert_eq!(
            (report.series, report.works, report.editions, report.members),
            (1, 3, 3, 3),
            "{which}: what the derive found standing"
        );
    }
}

/// A surviving library's missing hand document is repaired by a real
/// removal, which derives for real after its commit, and not by the dry run,
/// which rolls back and must write nothing to disk.
#[test]
fn removing_a_library_repairs_a_surviving_documents_gap_and_its_cost_does_not() {
    let dir = crate::tests::tempdir("remove-library-reconcile");
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    write_archive(
        &dir.path().join("a").join("Alpha").join("Alpha - v01.cbz"),
        3,
    );
    write_archive(&dir.path().join("b").join("Beta").join("Beta - v01.cbz"), 3);
    let kept = create_library(&conn, "Kept", &[dir.path().join("a")], None).expect("creates");
    let doomed = create_library(&conn, "Doomed", &[dir.path().join("b")], None).expect("creates");
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [kept.dirs[0].root.id],
    )
    .expect("the kept directory keeps documents");
    for root in roots(&conn).expect("lists") {
        scan_root(&conn, &root).expect("scans");
    }
    crate::derive::derive(&conn).expect("derives");
    let alpha = work_under(&conn, kept.dirs[0].root.id);
    let mut doc = crate::librarydoc::of(&conn, alpha).expect("projects");
    doc.title = Some("Hand-Set Alpha Title".to_string());
    crate::librarydoc::apply(&conn, alpha, &doc).expect("applies");
    let saved = crate::hand::save(&conn, alpha).expect("saves");
    let document = saved.path.expect("a kept directory's work has an address");
    std::fs::remove_file(&document).expect("the gap");

    let (_, rehearsed) = library_removal_cost(&conn, doomed.library_id).expect("costs");
    assert!(!document.exists(), "the dry run writes nothing to disk");
    let (_, removed) = remove_library(&conn, doomed.library_id).expect("removes");

    assert!(document.exists(), "the removal reconciles documents");
    assert_eq!(
        rehearsed.repaired_documents, 0,
        "the dry run's derive repairs nothing"
    );
    assert_eq!(
        removed.repaired_documents, 1,
        "the removal answers the report of the derive after its commit"
    );
}

/// A library of one directory, the only grant of one account: the library's
/// removal names that account alone, and leaves it seeing no library.
#[test]
fn removing_a_one_dir_library_names_its_one_account() {
    let parent = crate::tests::tempdir("remove-library-one-account");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    let alex = an_account_granted(&conn, "alex", "user", &["Solo"]);
    let solo = library_called(&conn, "Solo");

    let (cost, _) = library_removal_cost(&conn, solo).expect("costs");
    let (removal, _) = remove_library(&conn, solo).expect("removes");

    assert_eq!(cost.left_with_no_library, vec!["alex"]);
    assert_eq!(removal, cost);
    sees_no_library(&conn, alex);
}

/// The pattern language is gitignore's, taken from the crate rather than
/// reimplemented — so this test pins the five behaviours the design chose it
/// *for*, not the crate's whole surface.
///
/// Each row is a behaviour a hand-rolled matcher would have had to be given
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

/// A line `add_line` refuses disables itself and leaves every other line
/// standing — `rules.rs`'s rule, for its reason: a scan of a real library
/// must not die over one bad pattern.
///
/// **`\` is a line `ignore` will not compile**, one of the shapes
/// [`check_exclude_globs_refuses_only_what_ignore_will_not_compile`] pins.
/// `a[`, `[a-`, `[[:bogus:]]` and `***` all compile, so none would exercise it.
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

/// **`build` can fail where `add_line` accepted every line**: `add_line`
/// only parses a line and `build` compiles the set, and a line too large to
/// compile parses. The set then goes whole, its good lines too, under one
/// complaint that is `build`'s, rather than a panic.
#[test]
fn a_set_too_large_to_compile_complains_once_and_excludes_nothing() {
    let patterns = format!("extras\n{}", "[a-z]".repeat(200_000));
    let excluder = super::Excluder::build(Some(patterns.as_str()));

    assert_eq!(excluder.complaints.len(), 1, "one complaint, for the set");
    assert!(
        excluder.complaints[0].starts_with("compiling this directory's exclude patterns"),
        "the complaint is build's, not add_line's: {:.80}",
        excluder.complaints[0]
    );
    assert!(
        !excluder.excludes("a/extras", true),
        "a set that will not compile excludes nothing, its good lines included"
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

/// **Each shape `check_exclude_globs`' doc comment names, on its own side.**
/// The function refuses exactly what `ignore` will not compile, narrower than
/// "a malformed pattern": a refused shape is one an admin cannot save, and an
/// accepted one is saved whatever it happens to mean (O523).
#[test]
fn check_exclude_globs_refuses_only_what_ignore_will_not_compile() {
    for refused in ["\\", "a{b", "a}", "[z-a]"] {
        assert!(
            super::check_exclude_globs(refused).is_err(),
            "`{refused}` will not compile, so it must be refused"
        );
    }
    for accepted in ["a[", "[a-", "[[:bogus:]]", "[]", "***", "a**b"] {
        assert!(
            super::check_exclude_globs(accepted).is_ok(),
            "`{accepted}` compiles, so it must be accepted whatever it means"
        );
    }
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
/// above, excludes a whole directory wholesale rather than one page inside one.
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
/// — **not from `walk`** — because the walk that found nothing to apply still
/// ran and still excluded whatever the pattern named on the way, and the same
/// patterns still failed to compile whether or not the walk they hobbled ever
/// produced anything. An unmounted share looks exactly like a root whose own
/// pattern excludes everything beneath it — both leave the walk with only the
/// root itself — so this pins the combination directly rather than trusting
/// that the two code paths agree.
///
/// **The last two assertions are the ones that matter most.** They prove the
/// refusal protected the library from a pattern that would otherwise have
/// forgotten every entry under the root: `gone` stays `0`, and the rows the
/// first, unexcluded scan wrote are still there after the second, refused one.
///
/// **`bad_exclude_patterns` is pinned too.** `excluding`'s own pattern list
/// carries a second, unparseable line (`a{b`, the exact shape
/// [`check_exclude_globs_names_the_line_it_refuses`] uses) alongside `*`, so
/// this refusal's own `bad_exclude_patterns` has something in it to
/// assert against.
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
/// An admin whose own pattern swallowed the whole root a moment ago must not
/// be told the most destructive remedy this product has (C4 §2) is the fix.
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
/// other test in this module that reaches it with something excluded has a
/// non-empty `excluded_dirs`, so `> 0` could read `< 0` (always false for a
/// `usize`) with no other test failing.
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
/// `excluded_dirs` is the only thing that can make `excluded_everything` true
/// here, so its `!walk.excluded_dirs.is_empty()` half cannot hide behind
/// `excluded_files` the way it does in every other test that excludes
/// both — including
/// `the_refusal_offers_removal_only_when_exclusion_did_not_cause_it`'s case B,
/// whose `*` pattern excludes [`fixture`]'s `notes.pdf` too and so is not a
/// directories-only case.
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
/// against one that counts every stale row there. Neither number means
/// anything without the other.
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
///   is_dir`). A test built on it cannot tell a correct `Held.is_dir` from one
///   hard-coded `false`. `extras/` is `is_only_dir`, so the directory row's own
///   match — and therefore its landing in `forgotten_by_exclusion` rather than
///   `gone` — turns on `is_dir` being right.
///
/// Two ordinary files stand in for the pages for the same reason: a file
/// under `extras` must hold a `scan_entry` row that cannot match `extras/`
/// by its own name (a directory-only pattern never matches a file, whatever
/// its name), so the only way either row reaches `forgotten_by_exclusion` is
/// through `excludes_with_parents`'s ancestor walk — which is what turns
/// `excludes_with_parents` back into plain `excludes` into a real, observable
/// difference here.
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
/// that is still on disk *while the excluding scan itself runs*.
///
/// **The gap this closes is at `measure_dir`'s own `excluder.excludes(&child_rel,
/// is_dir)` call, not at `forget_unseen`'s `excludes_with_parents`.**
/// [`an_excluded_row_is_counted_apart_from_one_that_vanished`] above also
/// stages same-named files, but deletes every one of them from disk *before*
/// its excluding scan runs — so that test's own walk never meets a live file
/// this pattern could wrongly swallow, and only `forget_unseen`'s bookkeeping
/// call is exercised. Forcing `measure_dir`'s call to `true` therefore does not
/// fail that test; forcing it to `false` already does, by that same test's
/// directory assertion. This test's own file sits on disk during the one scan
/// that matters, so `is_dir` at the point the walk actually asks the question
/// has to be right: forced `true`, a directory-only pattern excludes the file
/// exactly as wrongly as it excludes the directory, and the file's own row
/// would never reach `scan_entry` at all.
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

fn two_dirs(parent: &Path) -> (PathBuf, PathBuf) {
    let (a, b) = (parent.join("Volumes"), parent.join("Chapters"));
    for dir in [&a, &b] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    (a, b)
}

#[test]
fn a_library_is_made_with_each_of_its_dirs() {
    let parent = crate::tests::tempdir("create-library");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let made = create_library(&conn, " Manga ", &[a.clone(), b.clone()], None).expect("creates");

    assert_eq!(made.name, "Manga", "the name as stored");
    assert_eq!(
        crate::libraries::name_of(&conn, made.library_id)
            .expect("reads")
            .as_deref(),
        Some("Manga")
    );
    let held: Vec<(PathBuf, i64)> = roots(&conn)
        .expect("lists")
        .into_iter()
        .map(|r| (r.path, r.libraries[0]))
        .collect();
    assert_eq!(
        held,
        vec![
            (a.canonicalize().expect("canonical"), made.library_id),
            (b.canonicalize().expect("canonical"), made.library_id),
        ]
    );
    assert_eq!(made.dirs.len(), 2);
    assert_eq!(
        made.dirs.iter().map(|f| f.root.clone()).collect::<Vec<_>>(),
        roots(&conn).expect("lists"),
        "the directories are answered in the order given, as stored"
    );
}

fn libraries_and_roots(conn: &Connection) -> (i64, i64) {
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("counts")
    };
    (count("library"), count("root"))
}

#[test]
fn a_library_is_refused_whole_for_one_bad_dir() {
    let parent = crate::tests::tempdir("create-library-refused");
    let (a, b) = two_dirs(parent.path());
    let inside = a.join("inside");
    std::fs::create_dir_all(&inside).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    for (paths, says) in [
        (vec![a.clone(), parent.path().join("missing")], "missing"),
        (vec![a.clone(), inside.clone()], "overlap"),
        (vec![inside.clone(), a.clone()], "overlap"),
        (vec![a.clone(), a.clone()], "overlap"),
        (Vec::new(), "at least one directory"),
    ] {
        let why = create_library(&conn, "Manga", &paths, None).expect_err("refuses");
        assert!(
            matches!(&why, AddRootError::Unusable(text) if text.contains(says)),
            "{why:?}"
        );
        assert_eq!(
            libraries_and_roots(&conn),
            (0, 0),
            "nothing was made: {paths:?}"
        );
    }
    assert!(matches!(
        create_library(&conn, "  ", std::slice::from_ref(&b), None),
        Err(AddRootError::Unusable(why)) if why.contains("needs a name")
    ));
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

/// The route's way to a new library refuses a name no page address holds
/// ([`crate::has_no_address`]) as the caller's mistake, and makes neither the
/// library nor its directory.
#[test]
fn a_library_called_one_dot_is_not_made_with_its_dir() {
    let dir = crate::tests::tempdir("create-library-one-dot");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in [".", " . "] {
        assert_eq!(
            create_library(&conn, name, &[dir.path().to_path_buf()], None),
            Err(AddRootError::Unusable(crate::NO_ADDRESS.to_string())),
            "{name:?}"
        );
    }
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

#[test]
fn a_library_called_two_dots_is_not_made_with_its_dir() {
    let dir = crate::tests::tempdir("create-library-two-dots");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in ["..", "\t.."] {
        assert_eq!(
            create_library(&conn, name, &[dir.path().to_path_buf()], None),
            Err(AddRootError::Unusable(crate::NO_ADDRESS.to_string())),
            "{name:?}"
        );
    }
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

/// The second directory's insert is refused by a trigger planted for it, after
/// both paths were vetted. The library and the first directory must go with it.
#[test]
fn a_library_whose_second_dir_fails_at_the_insert_leaves_nothing() {
    let parent = crate::tests::tempdir("create-library-insert");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch(
        "CREATE TEMP TRIGGER refuse_a_second_root BEFORE INSERT ON root
         WHEN (SELECT count(*) FROM root) >= 1
         BEGIN SELECT RAISE(ABORT, 'planted'); END;",
    )
    .expect("plants the refusal");

    let why = create_library(&conn, "Manga", &[a, b], None).expect_err("refuses");

    assert!(
        matches!(&why, AddRootError::Failed(text) if text.contains("planted")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

/// A new library is made over a directory another library holds, and links
/// it, as the add does: the answer names the library that held it, and the
/// settings and interval stay. It is not made under a name already taken; under
/// the name of a library that holds one of its directories, it says so.
#[test]
fn a_library_is_not_made_under_a_name_already_taken() {
    let parent = crate::tests::tempdir("create-library-taken");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let held = add_root_checked_in(&conn, &a, InLibrary::Named("Manga"))
        .expect("Manga exists")
        .root;
    update_root(
        &conn,
        held.id,
        &RootPatch {
            scan_interval_secs: Some(Some(4242)),
            ..Default::default()
        },
    )
    .expect("sets Manga's interval for a");

    let made = create_library(&conn, "Other", std::slice::from_ref(&a), None).expect("links a");
    let [linked] = &made.dirs[..] else {
        panic!("one directory: {made:?}")
    };
    assert_eq!(linked.also_in, ["Manga"]);
    assert_eq!((&linked.probe, linked.interval), (&None, None));
    assert_eq!(linked.root.id, held.id);
    assert_eq!(linked.root.scan_interval_secs, Some(4242));
    assert!(linked.schedule_kept, "Automatic was asked: {linked:?}");
    assert_eq!(
        linked.root.libraries,
        vec![held.libraries[0], made.library_id]
    );
    assert_eq!(roots(&conn).expect("roots"), vec![linked.root.clone()]);
    assert_eq!(libraries_and_roots(&conn), (2, 1));

    // Named after the library that already holds it, it is refused as in it,
    // not sent to add it there.
    for name in ["Manga", " manga "] {
        assert_eq!(
            create_library(&conn, name, std::slice::from_ref(&a), None).map(|made| made.library_id),
            Err(AddRootError::Unusable(format!(
                "{} is already a directory of the library `Manga`",
                a.canonicalize().unwrap().display()
            ))),
            "{name:?}"
        );
    }
    for name in ["Manga", " Manga "] {
        let why = create_library(&conn, name, std::slice::from_ref(&b), None).expect_err("refuses");
        assert!(
            matches!(&why, AddRootError::Unusable(text) if text.contains("already called `Manga`")),
            "{why:?}"
        );
    }
    assert_eq!(libraries_and_roots(&conn), (2, 1));
}

/// A link asked for Automatic, over a directory held at an Every of
/// Automatic's own length, keeps that Every and says so: `schedule::every`
/// reads the two alike, but they are different settings.
#[test]
fn a_link_asked_automatic_over_an_every_of_its_length_says_the_schedule_is_kept() {
    let parent = crate::tests::tempdir("link-kept-every");
    let (a, _) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let automatic = crate::watch::UNWATCHED_INTERVAL.as_secs().cast_signed();
    let held = add_root_checked_in(&conn, &a, InLibrary::Named("Manga"))
        .expect("Manga exists")
        .root;
    update_root(
        &conn,
        held.id,
        &RootPatch {
            scan_interval_secs: Some(Some(automatic)),
            ..Default::default()
        },
    )
    .expect("sets Manga's interval for a");

    let linked = add_root_checked_with(&conn, &a, InLibrary::Named("Other"), None).expect("links");

    assert_eq!(linked.also_in, ["Manga"]);
    assert_eq!(linked.root.scan_interval_secs, Some(automatic));
    assert!(linked.schedule_kept, "Automatic was asked: {linked:?}");
}

#[test]
fn a_dir_joins_a_library_by_its_id_and_no_library_by_an_id_none_has() {
    let parent = crate::tests::tempdir("add-by-id");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let first = add_root_in(&conn, &a, InLibrary::Named("Manga")).expect("Manga exists");

    let why = add_root_checked_in(&conn, &b, InLibrary::Id(first.libraries[0] + 1))
        .expect_err("no library has that id");
    assert!(
        matches!(&why, AddRootError::Unusable(text) if text.contains("there is no library")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (1, 1));

    let added = add_root_checked_in(&conn, &b, InLibrary::Id(first.libraries[0])).expect("joins");
    assert_eq!(added.root.libraries[0], first.libraries[0]);
    assert_eq!(libraries_and_roots(&conn), (1, 2));
}

/// A directory removed while its walk runs, and its id taken by a directory added
/// after it: the walk's `Root` is stale, and neither its entries nor its
/// status may land on the directory that holds the id now.
fn stale_root_and_the_dir_that_took_its_id(
    conn: &Connection,
    tag: &str,
) -> (Root, Root, crate::tests::TempDir, crate::tests::TempDir) {
    let walked = crate::tests::tempdir(&format!("{tag}-walked"));
    write_pages(&walked.path().join("Series").join("c001"), 2);
    let stale = super::add_root(conn, walked.path()).expect("adding the directory");
    super::remove_root(conn, stale.id).expect("removing it");
    let other = crate::tests::tempdir(&format!("{tag}-other"));
    let new = super::add_root(conn, other.path()).expect("adding another directory");
    assert_eq!(
        new.id, stale.id,
        "sanity: the freed id was given to the new directory"
    );
    assert_ne!(new.path, stale.path);
    (stale, new, walked, other)
}

fn status_and_entries(conn: &Connection, id: i64) -> (Option<String>, i64) {
    let status = conn
        .query_row(
            "SELECT last_scan_status FROM root WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("reading the status");
    let entries = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE root_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("counting the entries");
    (status, entries)
}

fn set_status(conn: &Connection, id: i64, status: &str) {
    conn.execute(
        "UPDATE root SET last_scan_status = ?2 WHERE id = ?1",
        rusqlite::params![id, status],
    )
    .expect("setting the status");
}

/// **A walk records nothing for a directory that is no longer the one walked.**
/// A directory removed while its walk ran, and its id given to a directory added
/// after it: the walk records neither its entries nor its status under the
/// new directory.
#[test]
fn a_walk_of_a_removed_dir_records_nothing_under_the_dir_that_took_its_id() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (stale, new, _walked, _other) =
        stale_root_and_the_dir_that_took_its_id(&conn, "stale-walk");

    let refused = super::scan_root(&conn, &stale).expect_err("the walk's directory is gone");
    assert_eq!(
        refused,
        format!(
            "{} was removed while it was being scanned, so nothing was recorded",
            stale.path.display()
        )
    );
    let (status, entries) = status_and_entries(&conn, new.id);
    assert_eq!(
        entries, 0,
        "no entry of the walk may be filed under the new directory"
    );
    assert_eq!(
        status, None,
        "the walk's status may not land on the new directory"
    );
}

/// The derive failure's correction asks the same of the directory it names.
///
/// The new directory reads `deriving`, one of the two statuses the correction
/// writes over, so the id-and-path question is what keeps it.
#[test]
fn mark_derive_failed_leaves_the_dir_that_took_a_removed_ones_id_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (stale, new, _walked, _other) =
        stale_root_and_the_dir_that_took_its_id(&conn, "stale-derive");
    set_status(&conn, new.id, "deriving");

    super::mark_derive_failed(&conn, &stale).expect("a no-op is not a fault");
    let (status, _) = status_and_entries(&conn, new.id);
    assert_eq!(
        status.as_deref(),
        Some("deriving"),
        "the correction may not land on the new directory"
    );
}

/// The job's marks around its derive ask the same, each over a status it
/// writes over.
#[test]
fn marking_deriving_or_derived_leaves_the_dir_that_took_a_removed_ones_id_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (stale, new, _walked, _other) =
        stale_root_and_the_dir_that_took_its_id(&conn, "stale-deriving");

    set_status(&conn, new.id, "ok");
    super::mark_deriving(&conn, &stale).expect("a no-op is not a fault");
    assert_eq!(
        status_and_entries(&conn, new.id).0.as_deref(),
        Some("ok"),
        "the mark before the derive may not land on the new directory"
    );

    set_status(&conn, new.id, "deriving");
    super::mark_derived(&conn, std::slice::from_ref(&stale)).expect("a no-op is not a fault");
    assert_eq!(
        status_and_entries(&conn, new.id).0.as_deref(),
        Some("deriving"),
        "the mark after the derive may not land on the new directory"
    );
}

/// A cancel's reads and marks, and the pause, ask the same of each directory
/// they name.
#[test]
fn cancelling_and_pausing_leave_the_dir_that_took_a_removed_ones_id_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (stale, new, _walked, _other) =
        stale_root_and_the_dir_that_took_its_id(&conn, "stale-cancel");
    let stale = std::slice::from_ref(&stale);
    let scanned = (
        Some("2026-11-01 00:00:00".to_string()),
        Some("ok".to_string()),
    );
    conn.execute(
        "UPDATE root SET last_scan_at = ?2, last_scan_status = ?3 WHERE id = ?1",
        rusqlite::params![new.id, scanned.0, scanned.1],
    )
    .expect("the new directory was scanned");
    let paused_at = |conn: &Connection| -> Option<String> {
        conn.query_row("SELECT paused_at FROM root WHERE id = ?1", [new.id], |r| {
            r.get(0)
        })
        .expect("reads the pause")
    };

    let held = super::last_scans(&conn, stale).expect("a read of nothing is not a fault");
    assert!(
        held.is_empty(),
        "the new directory's stamp is not the removed one's: {held:?}"
    );

    let unfinished = [super::Unfinished {
        root: stale[0].clone(),
        last_scan_at: None,
    }];
    super::mark_cancelled(&conn, &unfinished).expect("a no-op is not a fault");
    assert_eq!(
        last_scan_and_status(&conn, std::slice::from_ref(&new)),
        [scanned],
        "the cancel may not land on the new directory"
    );

    super::pause(&conn, stale).expect("a no-op is not a fault");
    assert_eq!(
        paused_at(&conn),
        None,
        "the pause may not land on the new directory"
    );

    let since = "2026-11-02 00:00:00";
    conn.execute(
        "UPDATE root SET paused_at = ?2 WHERE id = ?1",
        rusqlite::params![new.id, since],
    )
    .expect("the new directory is paused");
    super::unpause(&conn, stale).expect("a no-op is not a fault");
    assert_eq!(
        paused_at(&conn).as_deref(),
        Some(since),
        "the unpause may not land on the new directory"
    );
}

/// Spec §3.5: removing one of a copy's two directories keeps the work, its
/// position and its note and counts nothing; removing the other takes all
/// three. The dry run counts the same both times.
///
/// `b`, removed first, holds the primary copy, so the derive after it must
/// re-point the member at `a`'s. The hand-set chapter shows the member was
/// kept: one deleted with `b` is rebuilt from `a`'s scan with no hand value.
#[test]
fn removing_one_directory_of_a_copy_keeps_the_work_and_removing_the_other_takes_it() {
    let copy = crate::tests::a_volume_copied_into_two_libraries();
    let conn = copy.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the one work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [work_id],
    )
    .expect("a position");
    conn.execute(
        "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
         VALUES ('note-copy', 1, ?1, 'raw', 2, 'a note', datetime('now'), datetime('now'))",
        [work_id],
    )
    .expect("a note");
    conn.execute(
        "UPDATE member SET chapter_declared = 99.0, chapter_source = 'hand'",
        [],
    )
    .expect("a hand-set chapter");
    let held = |conn: &Connection| -> (Vec<i64>, i64, i64) {
        let works = conn
            .prepare("SELECT id FROM work")
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .expect("lists the works");
        let count = |table: &str| -> i64 {
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .expect("counts")
        };
        (works, count("progress"), count("note"))
    };

    let (cost, _) = removal_cost(&conn, copy.b.id).expect("costs b");
    let (removal, _) = remove_root(&conn, copy.b.id).expect("removes b");
    assert_eq!(
        (removal.works, removal.positions, removal.notes),
        (0, 0, 0),
        "a's copy holds the work: {removal:?}"
    );
    assert_eq!(cost, removal, "the dry run counts the same");
    assert_eq!(
        held(&conn),
        (vec![work_id], 1, 1),
        "the work stays, with its position and its note"
    );
    let chapter: (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT chapter_declared, chapter_source FROM member",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the one member");
    assert_eq!(
        chapter,
        (Some(99.0), Some("hand".to_string())),
        "the member is kept, not rebuilt"
    );
    assert_eq!(
        (
            crate::tests::the_member(&conn),
            crate::tests::member_root_ids(&conn)
        ),
        (
            (
                Some(copy.a.id),
                copy.a
                    .path
                    .join("Foo")
                    .join("Foo v01.cbz")
                    .display()
                    .to_string()
            ),
            vec![copy.a.id]
        ),
        "the member is re-pointed at a's copy, not left under no directory"
    );

    let (cost, _) = removal_cost(&conn, copy.a.id).expect("costs a");
    let (removal, _) = remove_root(&conn, copy.a.id).expect("removes a");
    assert_eq!(
        (removal.works, removal.positions, removal.notes),
        (1, 1, 1),
        "nothing else holds the work now: {removal:?}"
    );
    assert_eq!(cost, removal, "the dry run counts the same");
    assert_eq!(held(&conn), (Vec::new(), 0, 0), "the work goes, with both");
}

/// A removal that re-points a member at another directory's copy carries its
/// work's hand document there where that directory keeps documents, so no
/// derive finds an orphan or writes one afresh. Where it keeps none, the
/// document stays in `b/.taiyaku` and the removal lists it. Each case runs
/// twice, on a fresh copy: removing `b`, and removing its library `B`.
#[test]
fn a_removal_carries_the_document_of_a_work_it_re_points_at_another_copy() {
    for a_keeps in [true, false] {
        for whole_library in [false, true] {
            let case = format!("a_keeps={a_keeps}, whole_library={whole_library}");
            let copy = crate::tests::a_volume_copied_into_two_libraries();
            let conn = copy.conn();
            let work: i64 = conn
                .query_row("SELECT id FROM work", [], |r| r.get(0))
                .expect("the one work");
            keep_documents(&conn, &copy.b);
            if a_keeps {
                keep_documents(&conn, &copy.a);
            }
            let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
            doc.title = Some("Foo v01, by hand".to_string());
            crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
            let at_b = crate::hand::save(&conn, work)
                .expect("saves the title")
                .path
                .expect("the fixture: an address");
            assert!(
                at_b.is_file() && at_b.starts_with(&copy.b.path),
                "the fixture: the document is b's: {at_b:?}"
            );
            let at_a = copy
                .a
                .path
                .join(".taiyaku")
                .join(at_b.file_name().expect("a file name"));

            let derived = removed(&conn, &copy.b, whole_library);

            assert_eq!(
                crate::hand::path_for(&conn, work).expect("addresses"),
                Some(at_a.clone()),
                "{case}: the fixture: a's address, under the same key"
            );
            if a_keeps {
                assert!(
                    std::fs::read_to_string(&at_a)
                        .expect("reads it")
                        .contains("Foo v01, by hand"),
                    "{case}: the document b kept"
                );
                assert_eq!(
                    (
                        files_in(&copy.b.path.join(".taiyaku")),
                        derived.orphan_documents,
                        derived.repaired_documents
                    ),
                    (Vec::<String>::new(), Vec::new(), 0),
                    "{case}: carried out of b/.taiyaku, no orphan, and nothing written afresh"
                );
            } else {
                assert!(at_b.is_file(), "{case}: the document stays");
                assert!(
                    !copy.a.path.join(".taiyaku").exists(),
                    "{case}: and a is given no .taiyaku"
                );
                assert_eq!(
                    derived.orphan_documents,
                    [crate::hand::Orphan {
                        path: at_b.clone(),
                        title: Some("Foo v01, by hand".to_string()),
                        absorbed_into: None,
                    }],
                    "{case}: listed with its title"
                );
            }
        }
    }
}

/// A removal that deletes a work's member under the removed directory, while
/// another directory keeps the rest of the work, moves the work's address
/// there. Bleach v01's c001 is in `x` and c002 in `y`, one work addressed in
/// `x`, and `x` has a library of its own. The document is carried where `y`
/// keeps documents; where it keeps none, it stays in `x/.taiyaku` and the
/// removal lists it. Each case runs twice: removing `x`, and its library.
#[test]
fn a_removal_carries_the_document_of_a_work_whose_member_under_the_removed_directory_goes() {
    for y_keeps in [true, false] {
        for whole_library in [false, true] {
            let case = format!("y_keeps={y_keeps}, whole_library={whole_library}");
            let top = crate::tests::tempdir(&format!(
                "documents-spread-{}-{}",
                u8::from(y_keeps),
                u8::from(whole_library)
            ));
            for rel in ["x/Bleach v01 - c001.cbz", "y/Bleach v01 - c002.cbz"] {
                write_archive_labelled(&top.path().join(rel), 3, "jpg", rel);
            }
            let library = Library::open_in_memory().expect("open");
            let conn = library.conn();
            let [x, y] = ["x", "y"].map(|dir| {
                add_root_checked_in(&conn, &top.path().join(dir), InLibrary::Named("Manga"))
                    .expect("adds a directory to Manga")
                    .root
            });
            scan_root(&conn, &x).expect("scans x");
            scan_root(&conn, &y).expect("scans y");
            crate::derive::derive(&conn).expect("derives");
            let work = work_of(&conn, &x.path.join("Bleach v01 - c001.cbz"));
            assert_eq!(
                work_of(&conn, &y.path.join("Bleach v01 - c002.cbz")),
                work,
                "the fixture: one work"
            );
            keep_documents(&conn, &x);
            if y_keeps {
                keep_documents(&conn, &y);
            }
            let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
            doc.title = Some("Bleach v01, by hand".to_string());
            crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
            let at_x = crate::hand::save(&conn, work)
                .expect("saves the title")
                .path
                .expect("the fixture: an address");
            assert!(
                at_x.is_file() && at_x.starts_with(x.path.join(".taiyaku")),
                "the fixture: the document is x's: {at_x:?}"
            );
            crate::relocate::move_root(
                &conn,
                x.id,
                x.libraries[0],
                crate::relocate::Target::New("X"),
            )
            .expect("moves x to a library of its own");
            let x = roots(&conn)
                .expect("lists the directories")
                .into_iter()
                .find(|root| root.id == x.id)
                .expect("x");

            let derived = removed(&conn, &x, whole_library);

            let at = crate::hand::path_for(&conn, work)
                .expect("addresses")
                .expect("an address");
            assert!(
                at.starts_with(y.path.join(".taiyaku")),
                "{case}: the fixture: addressed in y now: {at:?}"
            );
            if y_keeps {
                assert!(
                    std::fs::read_to_string(&at)
                        .expect("reads it")
                        .contains("Bleach v01, by hand"),
                    "{case}: the document x kept"
                );
                assert_eq!(
                    (
                        files_in(&x.path.join(".taiyaku")),
                        derived.orphan_documents,
                        derived.repaired_documents
                    ),
                    (Vec::<String>::new(), Vec::new(), 0),
                    "{case}: carried out of x/.taiyaku, no orphan, and nothing written afresh"
                );
            } else {
                assert!(at_x.is_file(), "{case}: the document stays");
                assert!(
                    !y.path.join(".taiyaku").exists(),
                    "{case}: and y is given no .taiyaku"
                );
                assert_eq!(
                    derived.orphan_documents,
                    [crate::hand::Orphan {
                        path: at_x.clone(),
                        title: Some("Bleach v01, by hand".to_string()),
                        absorbed_into: None,
                    }],
                    "{case}: listed with its title"
                );
            }
        }
    }
}

/// A removal notes every work with a member under the removed directory, so a
/// work whose document is under a directory that stays is carried too, and the
/// carry leaves it there. Series v01's hub edition `en` is in `y`, which keeps
/// documents, and its `raw` edition in `x`, which has a library of its own.
/// Removing `x`, or its library, moves neither the hub nor the address, so the
/// document stays as it was and `y`'s reconcile, not the removal, answers for
/// it: nothing lists it.
#[test]
fn a_removal_leaves_unlisted_a_document_under_a_directory_that_stays() {
    for whole_library in [false, true] {
        let case = format!("whole_library={whole_library}");
        let top = crate::tests::tempdir(&format!("documents-live-{}", u8::from(whole_library)));
        write_pages(&top.path().join("y/Series/Series - v01"), 2);
        write_pages(&top.path().join("x/Series/Series - v01"), 3);
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let [x, y] = ["x", "y"].map(|dir| {
            add_root_checked_in(&conn, &top.path().join(dir), InLibrary::Named("Manga"))
                .expect("adds a directory to Manga")
                .root
        });
        update_root(
            &conn,
            y.id,
            &RootPatch {
                edition_key: Some(Some("en".to_string())),
                ..Default::default()
            },
        )
        .expect("y's edition sorts first, so it is the hub");
        keep_documents(&conn, &y);
        for root in roots(&conn).expect("lists the directories") {
            scan_root(&conn, &root).expect("scans");
        }
        crate::derive::derive(&conn).expect("derives");
        let (works, work): (i64, i64) = conn
            .query_row("SELECT count(*), max(id) FROM work", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("counts the works");
        let mut held: Vec<Option<i64>> = conn
            .prepare("SELECT m.root_id FROM member m JOIN edition e ON e.id = m.edition_id")
            .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
            .expect("lists the members' directories");
        held.sort_unstable();
        assert_eq!(
            (works, held),
            (1, vec![Some(x.id.min(y.id)), Some(x.id.max(y.id))]),
            "the fixture: one work, with a member in each directory"
        );
        let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
        doc.title = Some("Series v01, by hand".to_string());
        crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
        let at_y = crate::hand::save(&conn, work)
            .expect("saves the title")
            .path
            .expect("the fixture: an address");
        assert!(
            at_y.is_file() && at_y.starts_with(y.path.join(".taiyaku")),
            "the fixture: the document is y's: {at_y:?}"
        );
        let before = std::fs::read(&at_y).expect("reads it");
        crate::relocate::move_root(
            &conn,
            x.id,
            x.libraries[0],
            crate::relocate::Target::New("X"),
        )
        .expect("moves x to a library of its own");
        let x = roots(&conn)
            .expect("lists the directories")
            .into_iter()
            .find(|root| root.id == x.id)
            .expect("x");

        let derived = removed(&conn, &x, whole_library);

        assert_eq!(
            crate::hand::path_for(&conn, work).expect("addresses"),
            Some(at_y.clone()),
            "{case}: the fixture: the same address under y"
        );
        assert_eq!(
            std::fs::read(&at_y).expect("reads it"),
            before,
            "{case}: the document is untouched"
        );
        assert_eq!(
            (derived.orphan_documents, derived.repaired_documents),
            (Vec::new(), 0),
            "{case}: not listed, and nothing written afresh"
        );
    }
}

/// A work a removal deletes keeps its document where it was, unlisted: so when
/// a library's one directory goes, and so when its two directories holding a
/// copy each go, though removing `b` first re-points the member at `a`'s copy
/// before removing `a` takes the work.
#[test]
fn a_library_removal_that_takes_a_work_from_both_its_copies_leaves_its_document_unlisted() {
    for copied in [false, true] {
        let (b_dir, a_dir) = crate::tests::a_copy_of_one_volume();
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let b = add_root_in(&conn, b_dir.path(), InLibrary::Named("Both")).expect("adds b");
        let mut dirs = vec![b.clone()];
        if copied {
            dirs.push(add_root_in(&conn, a_dir.path(), InLibrary::Named("Both")).expect("adds a"));
        }
        for dir in &dirs {
            scan_root(&conn, dir).expect("scans");
        }
        crate::derive::derive(&conn).expect("derives");
        let mut held = dirs.iter().map(|dir| dir.id).collect::<Vec<_>>();
        held.sort_unstable();
        assert_eq!(
            (
                crate::tests::the_member(&conn).0,
                crate::tests::member_root_ids(&conn)
            ),
            (Some(b.id), held),
            "copied={copied}: the fixture: one member, b's, held under every directory"
        );
        keep_documents(&conn, &b);
        let work: i64 = conn
            .query_row("SELECT id FROM work", [], |r| r.get(0))
            .expect("the one work");
        let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
        doc.title = Some("Foo v01, by hand".to_string());
        crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
        let at_b = crate::hand::save(&conn, work)
            .expect("saves the title")
            .path
            .expect("the fixture: an address");

        let derived = removed(&conn, &b, true);

        let works: i64 = conn
            .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
            .expect("counts");
        assert_eq!(works, 0, "copied={copied}: the fixture: the work goes");
        assert!(at_b.is_file(), "copied={copied}: the document stays");
        assert_eq!(
            derived.orphan_documents,
            Vec::new(),
            "copied={copied}: unlisted"
        );
    }
}

/// A member whose primary directory `b` went with no removal around it, as
/// one can between a derive reading the roots and writing them, has its
/// `root_id` `NULL` and a copy recorded under `a` alone. It is `a`'s: removing
/// `a` counts its work and takes it, rather than leaving a work with no member.
#[test]
fn removing_the_one_directory_recording_a_member_takes_its_work_whatever_its_root_id() {
    let copy = crate::tests::a_volume_copied_into_two_libraries();
    let conn = copy.conn();
    conn.execute("DELETE FROM root WHERE id = ?1", [copy.b.id])
        .expect("b's directory goes with no removal around it");
    assert_eq!(
        crate::tests::member_root_ids(&conn),
        [copy.a.id],
        "the fixture: the member is recorded under a's copy alone"
    );
    let (cost, _) = removal_cost(&conn, copy.a.id).expect("costs a");
    let (removal, _) = remove_root(&conn, copy.a.id).expect("removes a");
    assert_eq!(removal.works, 1, "a alone held the work: {removal:?}");
    assert_eq!(cost, removal, "the dry run counts the same");
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("counts");
    assert_eq!(works, 0, "no work is left standing with no member");
}

/// A work with a member under the removed directory and an imported member,
/// which has no `member_root` row, is kept and not counted: the imported
/// member is no evidence that the work lived under the directory alone.
#[test]
fn removing_a_directory_keeps_a_work_that_also_holds_an_imported_member() {
    let dir = crate::tests::tempdir("removal-imported-member");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, dir.path()).expect("adds the directory");
    conn.execute_batch(&format!(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 'w', 'W', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id) VALUES
           (1, 1, 0, '{}/a.cbz', 'a', 1, {}),
           (2, 1, 1, '/imported/b.cbz', 'b', 1, NULL);",
        dir.path().display(),
        root.id
    ))
    .expect("a work of a member under the directory and an imported one");

    let (cost, _) = removal_cost(&conn, root.id).expect("costs");
    let (removal, _) = remove_root(&conn, root.id).expect("removes");
    assert_eq!(
        removal.works, 0,
        "the work is not the directory's alone: {removal:?}"
    );
    assert_eq!(cost, removal, "the dry run counts the same");
    let works: Vec<i64> = conn
        .prepare("SELECT id FROM work")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
        .expect("lists the works");
    assert_eq!(works, [1], "the work stays");
}

// --- spec §4: each file is scanned once, under the deepest directory --------

/// Bleach's two volumes under `m/kids`, and Naruto beside them under `m`.
pub(crate) const NEST_FILES: &[(&str, usize)] = &[
    ("kids/Bleach/v01.cbz", 3),
    ("kids/Bleach/v02.cbz", 4),
    ("Naruto/Naruto v01.cbz", 5),
];

/// Overlapping-libraries spec §4's N before anything is nested: `files` as
/// archives under `m`, which Manga holds, scanned and derived. Each test adds
/// the nested directory where its case needs it, through the checked add, as a
/// person would. Each archive's bytes name its path under `m`, so two stores
/// built from the same `files` hold the same digests.
pub(crate) fn manga_holding(
    label: &str,
    files: &[(&str, usize)],
) -> (crate::tests::TempDir, Library, Root) {
    let top = crate::tests::tempdir(label);
    for (rel, pages) in files {
        write_archive_labelled(&top.path().join("m").join(rel), *pages, "jpg", rel);
    }
    let library = Library::open_in_memory().expect("open");
    let manga = {
        let conn = library.conn();
        let manga = add_root_checked_in(&conn, &top.path().join("m"), InLibrary::Named("Manga"))
            .expect("Manga holds m")
            .root;
        scan_root(&conn, &manga).expect("scans m");
        crate::derive::derive(&conn).expect("derives");
        manga
    };
    (top, library, manga)
}

/// A reader on Bleach v01: user 1's position and a note on `raw` slot 2, and a
/// title set by hand on its work. Returns the work's id.
pub(crate) fn read_bleach_v01(conn: &Connection, manga: &Root) -> i64 {
    let work = work_of(conn, &manga.path.join("kids/Bleach/v01.cbz"));
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [work],
    )
    .expect("a position");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'n', ?1, 'raw', 2, 'a note', 'now', 'now')",
        [work],
    )
    .expect("a note");
    let mut doc = crate::librarydoc::of(conn, work).expect("projects the work");
    doc.title = Some("Bleach, by hand".to_string());
    crate::librarydoc::apply(conn, work, &doc).expect("sets the title by hand");
    work
}

/// The work holding the members at `path`.
fn work_of(conn: &Connection, path: &Path) -> i64 {
    conn.query_row(
        "SELECT DISTINCT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE m.path = ?1",
        [path.to_string_lossy()],
        |r| r.get(0),
    )
    .expect("one work holds it")
}

/// The directories holding a `scan_entry` for whatever is at `path`, by id.
fn entries_for(conn: &Connection, path: &Path) -> Vec<i64> {
    conn.prepare(
        "SELECT e.root_id FROM scan_entry e JOIN root r ON r.id = e.root_id
          WHERE CASE e.rel_path WHEN '' THEN r.path ELSE r.path || '/' || e.rel_path END = ?1
          ORDER BY e.root_id",
    )
    .and_then(|mut stmt| {
        stmt.query_map([path.to_string_lossy()], |r| r.get(0))?
            .collect()
    })
    .expect("reads the entries")
}

/// How many `scan_entry` rows `root` holds at or beneath `rel`.
fn rows_under(conn: &Connection, root: &Root, rel: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM scan_entry WHERE root_id = ?1
            AND (rel_path = ?2 OR substr(rel_path, 1, length(?2) + 1) = ?2 || '/')",
        rusqlite::params![root.id, rel],
        |r| r.get(0),
    )
    .expect("counts the rows")
}

/// Every member at `path`: its `member.root_id`, and the directories its
/// `member_root` rows name.
fn members_at(conn: &Connection, path: &Path) -> (Vec<Option<i64>>, Vec<i64>) {
    let path = path.to_string_lossy();
    let roots = conn
        .prepare("SELECT root_id FROM member WHERE path = ?1 ORDER BY id")
        .and_then(|mut stmt| stmt.query_map([&path], |r| r.get(0))?.collect())
        .expect("reads the members");
    let held = conn
        .prepare(
            "SELECT DISTINCT mr.root_id FROM member_root mr JOIN member m ON m.id = mr.member_id
              WHERE m.path = ?1 ORDER BY mr.root_id",
        )
        .and_then(|mut stmt| stmt.query_map([&path], |r| r.get(0))?.collect())
        .expect("reads the member roots");
    (roots, held)
}

/// The series names of every work, by name.
pub(crate) fn series_names(conn: &Connection) -> Vec<String> {
    conn.prepare(
        "SELECT DISTINCT s.sort_title FROM work w JOIN series s ON s.id = w.series_id
          ORDER BY s.sort_title",
    )
    .and_then(|mut stmt| stmt.query_map([], |r| r.get(0))?.collect())
    .expect("reads the series")
}

/// Spec §4: Kids, added at `m/kids` inside Manga's `m`, takes Bleach over at
/// its scan. Each file keeps one `scan_entry`, Kids', and its members move to
/// Kids with their `member_root` rows. The works, the reader's position and
/// note and the hand-set title are kept, the series is named by Kids' rules,
/// and Manga's next scan counts nothing gone.
#[test]
fn a_nested_directorys_scan_takes_over_the_rows_and_members_under_it() {
    let (_top, library, manga) = manga_holding("nest-takes-over", NEST_FILES);
    let conn = library.conn();
    let v01 = read_bleach_v01(&conn, &manga);
    let bleach = [
        manga.path.join("kids/Bleach/v01.cbz"),
        manga.path.join("kids/Bleach/v02.cbz"),
    ];
    let v02 = work_of(&conn, &bleach[1]);
    for file in &bleach {
        assert_eq!(
            entries_for(&conn, file),
            [manga.id],
            "the fixture: {file:?}"
        );
    }

    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("Kids is added inside Manga's directory")
        .root;
    let report = scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(
        (report.taken_over, report.gone),
        (4, 0),
        "two files and two directories taken, none gone: {report:?}"
    );
    for file in &bleach {
        assert_eq!(
            members_at(&conn, file),
            (vec![Some(kids.id)], vec![kids.id]),
            "{file:?}'s member moves with its row, before any derive"
        );
    }
    crate::derive::derive(&conn).expect("derives");

    for file in &bleach {
        assert_eq!(
            entries_for(&conn, file),
            [kids.id],
            "{file:?} is Kids' alone"
        );
        let (roots, held) = members_at(&conn, file);
        assert!(
            !roots.is_empty() && roots.iter().all(|root| *root == Some(kids.id)),
            "every member of {file:?} is Kids': {roots:?}"
        );
        assert_eq!(held, [kids.id], "{file:?}'s member_root");
    }
    assert_eq!(
        (work_of(&conn, &bleach[0]), work_of(&conn, &bleach[1])),
        (v01, v02),
        "the works are kept"
    );
    let reader: (String, i64, String, i64, Option<String>) = conn
        .query_row(
            "SELECT p.edition, p.slot, n.edition, n.slot, w.title_hand
               FROM progress p JOIN note n ON n.work_id = p.work_id JOIN work w ON w.id = p.work_id
              WHERE p.user_id = 1 AND p.work_id = ?1",
            [v01],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .expect("the position and the note are kept");
    assert_eq!(
        reader,
        (
            "raw".to_string(),
            2,
            "raw".to_string(),
            2,
            Some("Bleach, by hand".to_string())
        )
    );
    assert_eq!(series_names(&conn), ["Bleach", "Naruto"]);

    let again = scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(
        (again.refused.as_deref(), again.gone, again.taken_over),
        (None, 0, 0),
        "{again:?}"
    );
}

/// Order independence (the unfiltered CLI scan walks in id order): Manga
/// scanned after Kids is added and before Kids' first scan refuses nothing,
/// counts nothing gone and leaves Bleach's rows for Kids, whose scan then takes
/// them over.
#[test]
fn an_outer_scanned_before_the_nested_directorys_first_scan_leaves_its_rows_for_it() {
    let (_top, library, manga) = manga_holding("nest-order", NEST_FILES);
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;

    let first = scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(
        (
            first.refused.as_deref(),
            first.gone,
            first.forgotten_by_exclusion
        ),
        (None, 0, 0),
        "{first:?}"
    );
    assert_eq!(
        rows_under(&conn, &manga, "kids"),
        4,
        "the rows wait for Kids"
    );

    let report = scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!((report.taken_over, report.gone), (4, 0), "{report:?}");
    assert_eq!(rows_under(&conn, &manga, "kids"), 0);
    for file in ["kids/Bleach/v01.cbz", "kids/Bleach/v02.cbz"] {
        assert_eq!(
            entries_for(&conn, &manga.path.join(file)),
            [kids.id],
            "{file}"
        );
    }
}

/// The order case for §8.1: a Manga whose only content is Kids' directory,
/// scanned after Kids is added and before Kids' first scan, walks to nothing
/// but its own directory. The rows under `kids` are Kids' to take, so they are
/// not what Manga "held" and the scan is not refused as an empty one.
#[test]
fn an_outer_holding_only_a_nested_directory_scans_without_the_empty_refusal() {
    let (_top, library, manga) = manga_holding("nest-outer-only", &NEST_FILES[..2]);
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;

    let first = scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(
        (first.refused.as_deref(), first.gone),
        (None, 0),
        "{first:?}"
    );
    assert_eq!(
        rows_under(&conn, &manga, "kids"),
        4,
        "the rows wait for Kids"
    );
    assert_eq!(scan_root(&conn, &kids).expect("scans Kids").taken_over, 4);
    let again = scan_root(&conn, &manga).expect("scans Manga again");
    assert_eq!(
        (again.refused.as_deref(), again.gone),
        (None, 0),
        "{again:?}"
    );
}

/// Three levels, `m` ⊃ `m/kids` ⊃ `m/kids/little`, Kids scanned first. Kids
/// takes Bleach from Manga and leaves Tiny, which is Little's: Tiny's file stays
/// Manga's row and member until Little's scan takes it from Manga, and Kids
/// never holds it.
#[test]
fn a_directory_nested_two_deep_takes_its_rows_from_the_outermost_directory() {
    let (_top, library, manga) = manga_holding(
        "nest-three-levels",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/little/Tiny/Tiny v01.cbz", 2),
            ("Naruto/Naruto v01.cbz", 5),
        ],
    );
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    let little = add_root_checked_in(
        &conn,
        &manga.path.join("kids/little"),
        InLibrary::Named("Little"),
    )
    .expect("adds Little")
    .root;
    let tiny = manga.path.join("kids/little/Tiny/Tiny v01.cbz");

    let report = scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(report.taken_over, 3, "kids, kids/Bleach, v01: {report:?}");
    assert_eq!(
        members_at(&conn, &tiny),
        (vec![Some(manga.id)], vec![manga.id]),
        "Kids' scan leaves Tiny's member to Little, before any derive"
    );
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        entries_for(&conn, &tiny),
        [manga.id],
        "Tiny waits for Little"
    );
    assert_eq!(
        members_at(&conn, &tiny),
        (vec![Some(manga.id)], vec![manga.id])
    );
    assert_eq!(
        rows_under(&conn, &kids, "little"),
        0,
        "Kids never walks Little"
    );

    let report = scan_root(&conn, &little).expect("scans Little");
    assert_eq!(
        report.taken_over, 3,
        "kids/little, its Tiny, its file: {report:?}"
    );
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(entries_for(&conn, &tiny), [little.id]);
    assert_eq!(
        members_at(&conn, &tiny),
        (vec![Some(little.id)], vec![little.id])
    );
    assert_eq!(
        rows_under(&conn, &kids, "little"),
        0,
        "Kids never held Tiny"
    );
}

/// `m/alias` is a symlink to Kids' `m/kids`. Manga's walk compares it by its
/// target and skips it, so Bleach is read once, by Kids' rules: series
/// `Bleach`, not `alias`.
#[test]
fn a_symlink_to_a_nested_directory_is_skipped_by_the_walk_around_it() {
    let top = crate::tests::tempdir("nest-symlink");
    let m = top.path().join("m");
    for (rel, pages) in NEST_FILES {
        write_archive(&m.join(rel), *pages);
    }
    std::os::unix::fs::symlink(m.join("kids"), m.join("alias")).expect("links alias to kids");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
        .expect("adds Manga")
        .root;
    let kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;

    scan_root(&conn, &manga).expect("scans Manga");
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        rows_under(&conn, &manga, "alias"),
        0,
        "Manga skips the link"
    );
    for file in ["kids/Bleach/v01.cbz", "kids/Bleach/v02.cbz"] {
        assert_eq!(
            entries_for(&conn, &manga.path.join(file)),
            [kids.id],
            "{file}"
        );
    }
    assert_eq!(series_names(&conn), ["Bleach", "Naruto"]);
}

/// `m/kidsx` beside Kids' `m/kids` is not nested in it: Kids' scan takes none
/// of its rows or members, and Manga still walks it, counting a file gone from
/// it as gone.
#[test]
fn a_sibling_whose_name_starts_with_a_nested_directorys_is_not_nested() {
    let (_top, library, manga) = manga_holding(
        "nest-sibling",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kidsx/Other/v01.cbz", 2),
            ("kidsx/Other/v02.cbz", 2),
        ],
    );
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    let other = manga.path.join("kidsx/Other/v01.cbz");

    let report = scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(report.taken_over, 3, "kids, kids/Bleach, v01: {report:?}");
    assert_eq!(rows_under(&conn, &manga, "kidsx"), 4);
    assert_eq!(
        members_at(&conn, &other),
        (vec![Some(manga.id)], vec![manga.id])
    );

    std::fs::remove_file(manga.path.join("kidsx/Other/v02.cbz")).expect("removes v02");
    let again = scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(again.gone, 1, "kidsx is walked, so v02 is gone: {again:?}");
    assert_eq!(entries_for(&conn, &other), [manga.id]);
}

/// The Libraries page's "its scans found …": a PDF under Kids' `m/kids` that
/// Manga scanned before Kids was added is counted once for Manga after both
/// walks, not once per directory that walked it.
#[test]
fn a_book_under_a_nested_directory_is_counted_once_for_the_library_around_it() {
    let top = crate::tests::tempdir("nest-books");
    let m = top.path().join("m");
    write_archive(&m.join("Naruto/Naruto v01.cbz"), 5);
    std::fs::create_dir_all(m.join("kids")).expect("mkdir");
    std::fs::write(m.join("kids/guide.pdf"), b"%PDF-1.7\n").expect("writes the PDF");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
        .expect("adds Manga")
        .root;
    scan_root(&conn, &manga).expect("scans Manga");
    let kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;

    scan_root(&conn, &kids).expect("scans Kids");
    scan_root(&conn, &manga).expect("scans Manga again");
    assert_eq!(
        crate::libraries::books(&conn, manga.libraries[0]).expect("counts"),
        1
    );
    assert_eq!(
        crate::libraries::books(&conn, kids.libraries[0]).expect("counts"),
        1
    );
}

/// §8.1 at a nested directory's first scan: Manga holds Bleach under `m/kids`,
/// and `m/kids` reads empty when Kids is added, as a mount that is not ready
/// does. Kids holds nothing yet, but the rows it would take over count, so its
/// scan is refused and Manga keeps its rows and members. Once the directory
/// reads again, Kids takes them over.
#[test]
fn a_nested_directory_that_reads_empty_at_its_first_scan_is_refused() {
    let (top, library, manga) = manga_holding("nest-empty-read", NEST_FILES);
    let conn = library.conn();
    let (kids_dir, away) = (manga.path.join("kids"), top.path().join("away"));
    std::fs::rename(&kids_dir, &away).expect("moves Bleach away");
    std::fs::create_dir(&kids_dir).expect("leaves an empty mount point");
    let kids = add_root_checked_in(&conn, &kids_dir, InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    let v01 = manga.path.join("kids/Bleach/v01.cbz");

    let report = scan_root(&conn, &kids).expect("scans Kids");
    assert!(
        report
            .refused
            .as_deref()
            .is_some_and(|why| why.contains("scanned empty")),
        "{report:?}"
    );
    assert_eq!(report.taken_over, 0);
    assert_eq!(rows_under(&conn, &manga, "kids"), 4, "Manga keeps its rows");
    assert_eq!(
        members_at(&conn, &v01),
        (vec![Some(manga.id)], vec![manga.id])
    );

    std::fs::remove_dir(&kids_dir).expect("unmounts");
    std::fs::rename(&away, &kids_dir).expect("mounts Bleach again");
    let report = scan_root(&conn, &kids).expect("scans Kids again");
    assert_eq!(
        (report.refused.as_deref(), report.taken_over),
        (None, 4),
        "{report:?}"
    );
}

/// `m/alias` links to `m/kids` and Manga recorded Bleach through it before
/// Kids existed. No hand-over takes `alias/…`, so Manga's next scan forgets
/// those rows as gone rather than keeping them for good, and Bleach is Kids'
/// alone.
#[test]
fn rows_recorded_through_a_symlink_before_the_nested_directory_existed_are_forgotten() {
    let top = crate::tests::tempdir("nest-symlink-late");
    let m = top.path().join("m");
    for (rel, pages) in NEST_FILES {
        write_archive(&m.join(rel), *pages);
    }
    std::os::unix::fs::symlink(m.join("kids"), m.join("alias")).expect("links alias to kids");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
        .expect("adds Manga")
        .root;
    scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(rows_under(&conn, &manga, "alias"), 4, "the fixture");
    let kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;

    assert_eq!(scan_root(&conn, &kids).expect("scans Kids").taken_over, 4);
    let again = scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(
        (again.gone, again.refused.as_deref()),
        (4, None),
        "alias, alias/Bleach and its two files: {again:?}"
    );
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(rows_under(&conn, &manga, "alias"), 0);
    for file in ["kids/Bleach/v01.cbz", "kids/Bleach/v02.cbz"] {
        assert_eq!(
            entries_for(&conn, &manga.path.join(file)),
            [kids.id],
            "{file}"
        );
    }
    assert_eq!(series_names(&conn), ["Bleach", "Naruto"]);
}

/// The counts of a hand-over. Of Manga's rows under `m/kids`, those Kids' walk
/// recorded too are taken over, and one whose file went before Kids' scan is
/// gone. Those Kids' own patterns exclude stay Manga's (decision 21), counted
/// neither taken over nor forgotten.
#[test]
fn a_hand_over_counts_as_taken_only_the_rows_the_nested_walk_recorded() {
    let (_top, library, manga) = manga_holding(
        "nest-counts",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/Bleach/v02.cbz", 4),
            ("kids/extras/x.cbz", 2),
        ],
    );
    std::fs::remove_file(manga.path.join("kids/Bleach/v02.cbz")).expect("removes v02");
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    conn.execute(
        "UPDATE root SET exclude_globs = 'extras/' WHERE id = ?1",
        [kids.id],
    )
    .expect("Kids excludes extras/");
    let kids = Root {
        exclude_globs: Some("extras/".to_string()),
        ..kids
    };

    let report = scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(
        (
            report.taken_over,
            report.gone,
            report.forgotten_by_exclusion
        ),
        (3, 1, 0),
        "kids, kids/Bleach and v01 taken; v02 gone; extras and its file kept: {report:?}"
    );
    let kept: Vec<String> = conn
        .prepare(
            "SELECT rel_path FROM scan_entry WHERE root_id = ?1 AND rel_path LIKE 'kids/%'
              ORDER BY rel_path",
        )
        .and_then(|mut stmt| stmt.query_map([manga.id], |r| r.get(0))?.collect())
        .expect("reads Manga's rows under kids");
    assert_eq!(
        kept,
        ["kids/extras", "kids/extras/x.cbz"],
        "Manga keeps the rows Kids excludes and no other"
    );
}

/// Manga's `m` holding Bleach v01 and `kids/extras/Adult/Adult v01.cbz`, both
/// derived, and Kids added at `m/kids` excluding `extras/`, not yet scanned.
/// Returns the store, Manga, Kids and the excluded file's path.
fn kids_excluding_extras(label: &str) -> (crate::tests::TempDir, Library, Root, Root, PathBuf) {
    let (top, library, manga) = manga_holding(
        label,
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/extras/Adult/Adult v01.cbz", 2),
        ],
    );
    let kids = {
        let conn = library.conn();
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        conn.execute(
            "UPDATE root SET exclude_globs = 'extras/' WHERE id = ?1",
            [kids.id],
        )
        .expect("Kids excludes extras/");
        Root {
            exclude_globs: Some("extras/".to_string()),
            ..kids
        }
    };
    let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
    (top, library, manga, kids, adult)
}

/// Decision 21: the hand-over moves only the members of files the nested
/// directory's scan recorded. A file Kids' patterns exclude stays with Manga:
/// an account granted Kids alone never sees its work, before Kids' scan or
/// after it, and one granted Manga alone keeps seeing it. Bleach v01, which
/// Kids' walk recorded, moves to Kids.
#[test]
fn a_file_the_nested_directory_excludes_stays_with_the_outer_and_its_accounts() {
    let (_top, library, manga, kids, adult) = kids_excluding_extras("nest-excluded-stays");
    let conn = library.conn();
    let slug: String = conn
        .query_row(
            "SELECT slug FROM work WHERE id = ?1",
            [work_of(&conn, &adult)],
            |r| r.get(0),
        )
        .expect("the work's slug");
    let account = |name: &str, library_id: i64| {
        conn.execute(
            "INSERT INTO user (name, password_hash, role) VALUES (?1, '!', 'user')",
            [name],
        )
        .expect("an account");
        let user = conn.last_insert_rowid();
        crate::account::grant_library(&conn, user, library_id).expect("grants");
        user
    };
    let kids_only = account("k", kids.libraries[0]);
    let manga_only = account("m", manga.libraries[0]);
    let sees = |user: i64| {
        let access = crate::account::access_for(&conn, user).expect("access");
        crate::browse::visible_editions(&conn, &access)
            .expect("visible editions")
            .contains_key(&slug)
    };
    assert_eq!(
        (sees(kids_only), sees(manga_only)),
        (false, true),
        "before Kids' scan"
    );

    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        (sees(kids_only), sees(manga_only)),
        (false, true),
        "after Kids' scan"
    );
    assert_eq!(
        members_at(&conn, &adult),
        (vec![Some(manga.id)], vec![manga.id]),
        "the excluded file's member stays with Manga"
    );
    assert_eq!(
        members_at(&conn, &manga.path.join("kids/Bleach/v01.cbz")),
        (vec![Some(kids.id)], vec![kids.id]),
        "the recorded file's member moves to Kids"
    );
}

/// Whether the work `work` is marked missing.
fn marked_missing(conn: &Connection, work: i64) -> bool {
    conn.query_row(
        "SELECT missing_since IS NOT NULL FROM work WHERE id = ?1",
        [work],
        |r| r.get(0),
    )
    .expect("reads the mark")
}

/// Decision 21's rule for the entry (O91): Kids' scan keeps Manga's
/// `scan_entry` for a file Kids' patterns exclude, so Manga still records it
/// and the missing mark leaves its work present, after Kids' scan and after
/// Manga's. Bleach v01, which Kids' walk recorded, is taken over as before.
#[test]
fn a_file_the_nested_directory_excludes_keeps_the_outers_entry_and_its_work_present() {
    let (_top, library, manga, kids, adult) = kids_excluding_extras("nest-excluded-entry-kept");
    let conn = library.conn();
    let work = work_of(&conn, &adult);

    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    let after_kids = (entries_for(&conn, &adult), marked_missing(&conn, work));
    scan_root(&conn, &manga).expect("scans Manga");
    crate::derive::derive(&conn).expect("derives");
    let after_manga = (entries_for(&conn, &adult), marked_missing(&conn, work));

    assert_eq!(
        (after_kids, after_manga),
        ((vec![manga.id], false), (vec![manga.id], false)),
        "Manga's entry for the excluded file stays and its work is not marked, \
         after Kids' scan and after Manga's"
    );
    assert_eq!(
        entries_for(&conn, &manga.path.join("kids/Bleach/v01.cbz")),
        vec![kids.id],
        "the recorded file's entry is Kids'"
    );
}

/// O91: Inner, a directory at Manga's series folder `S`, reads no series from
/// `S 01.cbz`, whose name does not repeat it, so its derive makes no member of
/// the file and never hashes Inner's entry for it. The work Manga's rules made
/// of it, handed to Inner with its member, still reads present, whichever
/// directory scans first, with Manga excluding `S/` as the drive had it: the
/// derive hashes the entry a member names. `S 03.cbz`, which no work holds,
/// stays unhashed, and `S - v02.cbz`, whose name gives Inner a series, is
/// unchanged.
#[test]
fn a_file_the_nested_directorys_rules_make_no_member_of_keeps_its_work_present() {
    let digested = |conn: &Connection, root: i64, rel: &str| -> bool {
        conn.query_row(
            "SELECT sha256 IS NOT NULL FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root, rel],
            |r| r.get(0),
        )
        .expect("Inner holds an entry for it")
    };
    let after = |inner_first: bool| {
        let (top, library, manga) = manga_holding(
            &format!("nest-no-series-{inner_first}"),
            &[("S/S 01.cbz", 3), ("S/S - v02.cbz", 4)],
        );
        write_archive(&top.path().join("m/S/S 03.cbz"), 2);
        let conn = library.conn();
        let bare = work_of(&conn, &manga.path.join("S/S 01.cbz"));
        let named = work_of(&conn, &manga.path.join("S/S - v02.cbz"));
        conn.execute(
            "UPDATE root SET exclude_globs = 'S/' WHERE id = ?1",
            [manga.id],
        )
        .expect("Manga excludes S/");
        let manga = Root {
            exclude_globs: Some("S/".to_string()),
            ..manga
        };
        let inner = add_root_checked_in(&conn, &manga.path.join("S"), InLibrary::Named("Inner"))
            .expect("adds Inner")
            .root;
        let order = if inner_first {
            [&inner, &manga]
        } else {
            [&manga, &inner]
        };
        for root in order {
            scan_root(&conn, root).expect("scans");
            crate::derive::derive(&conn).expect("derives");
        }
        (
            marked_missing(&conn, bare),
            marked_missing(&conn, named),
            digested(&conn, inner.id, "S 01.cbz"),
            digested(&conn, inner.id, "S 03.cbz"),
        )
    };

    assert_eq!(
        [after(false), after(true)],
        [(false, false, true, false); 2],
        "Manga scanned first, then Inner first: both works read present, the entry \
         of S 01.cbz is hashed and that of S 03.cbz, which no work holds, is not"
    );
}

/// The owner's ruling "Path counts too" (O91): in the layout above, `S 01.cbz`
/// is replaced in place by five pages of other bytes. Inner's rescans record
/// and hash them, a digest no member has, and its rules still make no member
/// of the file, so the member keeps its digest and its three pages. The work
/// reads present through both rescans, since Inner holds an entry at its path.
#[test]
fn a_file_replaced_in_place_under_a_directory_reading_no_series_keeps_its_work_present() {
    let (top, library, manga) = manga_holding("nest-replaced-in-place", &[("S/S 01.cbz", 3)]);
    let conn = library.conn();
    let path = manga.path.join("S/S 01.cbz");
    let work = work_of(&conn, &path);
    conn.execute(
        "UPDATE root SET exclude_globs = 'S/' WHERE id = ?1",
        [manga.id],
    )
    .expect("Manga excludes S/");
    let manga = Root {
        exclude_globs: Some("S/".to_string()),
        ..manga
    };
    let inner = add_root_checked_in(&conn, &manga.path.join("S"), InLibrary::Named("Inner"))
        .expect("adds Inner")
        .root;
    for root in [&manga, &inner] {
        scan_root(&conn, root).expect("scans");
        crate::derive::derive(&conn).expect("derives");
    }
    let before = marked_missing(&conn, work);

    write_archive_labelled(&top.path().join("m/S/S 01.cbz"), 5, "jpg", "replaced");
    let mut after = Vec::new();
    for _ in 0..2 {
        scan_root(&conn, &inner).expect("rescans Inner");
        crate::derive::derive(&conn).expect("derives");
        after.push(marked_missing(&conn, work));
    }
    let (entry_is_members, pages): (bool, i64) = conn
        .query_row(
            "SELECT se.sha256 = m.sha256, m.pages FROM member m
               JOIN scan_entry se ON se.root_id = ?1 AND se.rel_path = 'S 01.cbz'
              WHERE m.path = ?2",
            rusqlite::params![inner.id, path.to_string_lossy()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads Inner's entry and the member");

    assert_eq!(
        (before, after, entry_is_members, pages),
        (false, vec![false, false], false, 3),
        "present before and through both rescans, Inner's entry holding other \
         bytes than the member, which keeps its three pages"
    );
}

/// A member's file that goes between Inner's walk and the derive is named
/// once among those gone before they could be hashed, whether Inner's rules
/// stage it (`S - v02.cbz`, which `classify` names) or not (`S 01.cbz`, which
/// only the pass hashing members' entries reaches): the entry stays at its
/// path until the next walk, so its work reads present and the report is
/// where its going shows.
#[test]
fn a_members_file_gone_before_the_derive_hashes_it_is_named_once() {
    let (top, library, manga) = manga_holding(
        "nest-gone-before-hashed",
        &[("S/S 01.cbz", 3), ("S/S - v02.cbz", 4)],
    );
    let conn = library.conn();
    let works = [
        work_of(&conn, &manga.path.join("S/S 01.cbz")),
        work_of(&conn, &manga.path.join("S/S - v02.cbz")),
    ];
    conn.execute(
        "UPDATE root SET exclude_globs = 'S/' WHERE id = ?1",
        [manga.id],
    )
    .expect("Manga excludes S/");
    let manga = Root {
        exclude_globs: Some("S/".to_string()),
        ..manga
    };
    let inner = add_root_checked_in(&conn, &manga.path.join("S"), InLibrary::Named("Inner"))
        .expect("adds Inner")
        .root;
    scan_root(&conn, &manga).expect("scans Manga");
    crate::derive::derive(&conn).expect("derives");
    // A file unchanged since Manga's walk would be taken over with its digest,
    // and the derive would hash nothing: moved, its digest is held instead.
    let moved = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    for name in ["S 01.cbz", "S - v02.cbz"] {
        std::fs::File::options()
            .write(true)
            .open(top.path().join("m/S").join(name))
            .and_then(|file| file.set_times(std::fs::FileTimes::new().set_modified(moved)))
            .expect("moves the file's mtime");
    }
    scan_root(&conn, &inner).expect("scans Inner");
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM scan_entry WHERE root_id = ?1 AND sha256 IS NULL
               AND held_sha256 IS NOT NULL",
            [inner.id],
            |r| r.get::<_, i64>(0)
        )
        .expect("counts Inner's held rows"),
        2,
        "the fixture: Inner holds both files' digests, to hash them again"
    );
    for name in ["S 01.cbz", "S - v02.cbz"] {
        std::fs::remove_file(top.path().join("m/S").join(name)).expect("the file goes");
    }

    let report = crate::derive::derive(&conn).expect("derives");

    let mut vanished = report.unreadable.vanished;
    vanished.sort();
    assert_eq!(
        (vanished, works.map(|work| marked_missing(&conn, work))),
        (
            vec![
                inner.path.join("S - v02.cbz").display().to_string(),
                inner.path.join("S 01.cbz").display().to_string(),
            ],
            [false, false]
        ),
        "each file named once, and both works present"
    );
}

/// The rule's way out: once Kids stops excluding `extras/`, Kids' next scan
/// records the file and takes over Manga's kept entries under `extras/` with
/// its member, so no file is held by both directories.
#[test]
fn a_file_the_nested_directory_stops_excluding_hands_over_at_its_next_scan() {
    let (_top, library, manga, kids, adult) = kids_excluding_extras("nest-excluded-lifted");
    let conn = library.conn();
    let work = work_of(&conn, &adult);
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    conn.execute(
        "UPDATE root SET exclude_globs = NULL WHERE id = ?1",
        [kids.id],
    )
    .expect("Kids stops excluding extras/");
    let kids = Root {
        exclude_globs: None,
        ..kids
    };

    let report = scan_root(&conn, &kids).expect("scans Kids again");
    crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            report.taken_over,
            report.gone,
            report.forgotten_by_exclusion
        ),
        (3, 0, 0),
        "extras, extras/Adult and the file taken from Manga: {report:?}"
    );
    assert_eq!(
        (
            entries_for(&conn, &adult),
            rows_under(&conn, &manga, "kids"),
            members_at(&conn, &adult),
            marked_missing(&conn, work),
        ),
        (
            vec![kids.id],
            0,
            (vec![Some(kids.id)], vec![kids.id]),
            false
        ),
        "the file's entry and member are Kids' alone, and its work is present"
    );
}

/// Kids' second scan, with Manga's kept rows under `extras/` still there,
/// counts them in none of taken over, gone and forgotten, and leaves them
/// Manga's.
#[test]
fn a_second_scan_of_the_nested_directory_counts_the_outers_kept_rows_nowhere() {
    let (_top, library, manga, kids, adult) = kids_excluding_extras("nest-excluded-rescan");
    let conn = library.conn();
    scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(rows_under(&conn, &manga, "kids"), 3, "the fixture");

    let again = scan_root(&conn, &kids).expect("scans Kids again");

    assert_eq!(
        (again.taken_over, again.gone, again.forgotten_by_exclusion),
        (0, 0, 0),
        "{again:?}"
    );
    assert_eq!(
        (
            rows_under(&conn, &manga, "kids"),
            entries_for(&conn, &adult)
        ),
        (3, vec![manga.id]),
        "Manga keeps extras, extras/Adult and the file"
    );
}

/// §8.1 does not count the outer's rows a nested directory's patterns cover,
/// since its scan keeps them (decision 21). Kids holds only Little, nested in
/// it, and `extras/`, which it excludes, so its second scan reads empty: it is
/// applied, not refused, and Manga keeps those rows. The pattern is anchored,
/// so it covers Manga's `kids/extras` only when read relative to Kids.
#[test]
fn an_empty_read_of_a_nested_directory_is_not_refused_for_the_rows_the_outer_keeps() {
    let (_top, library, manga) = manga_holding(
        "nest-excluded-empty-read",
        &[
            ("kids/little/Tiny/Tiny v01.cbz", 3),
            ("kids/extras/Adult/Adult v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    conn.execute(
        "UPDATE root SET exclude_globs = '/extras/' WHERE id = ?1",
        [kids.id],
    )
    .expect("Kids excludes /extras/");
    let kids = Root {
        exclude_globs: Some("/extras/".to_string()),
        ..kids
    };
    scan_root(&conn, &kids).expect("scans Kids");
    let little = add_root_checked_in(
        &conn,
        &manga.path.join("kids/little"),
        InLibrary::Named("Little"),
    )
    .expect("adds Little")
    .root;
    assert_eq!(
        scan_root(&conn, &little).expect("scans Little").taken_over,
        3,
        "the fixture: little, little/Tiny and its file go to Little"
    );
    assert_eq!(
        (
            rows_under(&conn, &kids, ""),
            rows_under(&conn, &manga, "kids")
        ),
        (1, 3),
        "the fixture: Kids holds its own row, Manga the three under extras"
    );

    let again = scan_root(&conn, &kids).expect("scans Kids again");

    assert_eq!(
        (again.refused.as_deref(), again.gone, again.taken_over),
        (None, 0, 0),
        "{again:?}"
    );
    assert_eq!(rows_under(&conn, &manga, "kids"), 3, "Manga keeps its rows");
}

/// Manga's `m` holding `files`, derived; Kids added at `m/kids` and scanned
/// with no pattern, so it took every file under `kids` over, then derived; and
/// Kids' pattern then set to `extras/`, not yet scanned again.
fn kids_coming_to_exclude_extras(
    label: &str,
    files: &[(&str, usize)],
) -> (crate::tests::TempDir, Library, Root, Root) {
    let (top, library, manga) = manga_holding(label, files);
    let kids = {
        let conn = library.conn();
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        conn.execute(
            "UPDATE root SET exclude_globs = 'extras/' WHERE id = ?1",
            [kids.id],
        )
        .expect("Kids comes to exclude extras/");
        Root {
            exclude_globs: Some("extras/".to_string()),
            ..kids
        }
    };
    (top, library, manga, kids)
}

/// A reader granted library `library_id` alone.
fn reader_of(conn: &Connection, name: &str, library_id: i64) -> i64 {
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES (?1, '!', 'user')",
        [name],
    )
    .expect("an account");
    let user = conn.last_insert_rowid();
    crate::account::grant_library(conn, user, library_id).expect("grants");
    user
}

/// Whether reader `user` sees work `work`.
fn reader_sees(conn: &Connection, user: i64, work: i64) -> bool {
    let slug: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [work], |r| r.get(0))
        .expect("the work's slug");
    let access = crate::account::access_for(conn, user).expect("access");
    crate::browse::visible_editions(conn, &access)
        .expect("visible editions")
        .contains_key(&slug)
}

/// Each `scan_entry` path `root` holds under `rel`, sorted.
fn paths_under(conn: &Connection, root: &Root, rel: &str) -> Vec<String> {
    conn.prepare(
        "SELECT rel_path FROM scan_entry WHERE root_id = ?1
            AND substr(rel_path, 1, length(?2) + 1) = ?2 || '/' ORDER BY rel_path",
    )
    .and_then(|mut stmt| {
        stmt.query_map(rusqlite::params![root.id, rel], |r| r.get(0))?
            .collect()
    })
    .expect("reads the paths")
}

/// Decision 21, under the owner's "Hand back at once" ruling: Kids took Adult
/// over at its first scan, and the scan that comes to exclude it hands its
/// rows and its member back to Manga, whose walk reaches it. The rows count as
/// forgotten by exclusion, Adult's work stays present for Manga's reader and
/// hidden from Kids', Manga's next scan keeps the rows, and the outcome is the
/// one Kids' pattern gives when it comes before Kids' first scan.
#[test]
fn a_file_the_nested_directory_comes_to_exclude_goes_back_to_the_outer_at_that_scan() {
    let files = [
        ("kids/Bleach/v01.cbz", 3),
        ("kids/extras/Adult/Adult v01.cbz", 2),
    ];
    let mut outcomes = Vec::new();
    for pattern_first in [true, false] {
        let (_top, library, manga, kids) = if pattern_first {
            let (top, library, manga, kids, _) = kids_excluding_extras("nest-exclude-first");
            (top, library, manga, kids)
        } else {
            kids_coming_to_exclude_extras("nest-exclude-later", &files)
        };
        let conn = library.conn();
        let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
        let work = work_of(&conn, &adult);
        let manga_reader = reader_of(&conn, "m", manga.libraries[0]);
        let kids_reader = reader_of(&conn, "k", kids.libraries[0]);
        if !pattern_first {
            assert_eq!(
                (
                    members_at(&conn, &adult),
                    reader_sees(&conn, manga_reader, work)
                ),
                ((vec![Some(kids.id)], vec![kids.id]), true),
                "the fixture: Kids took Adult over, and Manga's reader sees it"
            );
        }

        let report = scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        if !pattern_first {
            assert_eq!(
                (
                    report.taken_over,
                    report.gone,
                    report.forgotten_by_exclusion
                ),
                (0, 0, 3),
                "extras, extras/Adult and the file, forgotten by Kids: {report:?}"
            );
        }
        let again = scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (again.gone, again.forgotten_by_exclusion),
            (0, 0),
            "{again:?}"
        );

        outcomes.push((
            paths_under(&conn, &manga, "kids"),
            entries_for(&conn, &adult) == [manga.id],
            members_at(&conn, &adult) == (vec![Some(manga.id)], vec![manga.id]),
            marked_missing(&conn, work),
            reader_sees(&conn, manga_reader, work),
            reader_sees(&conn, kids_reader, work),
        ));
    }

    let manga_holds = vec![
        "kids/extras".to_string(),
        "kids/extras/Adult".to_string(),
        "kids/extras/Adult/Adult v01.cbz".to_string(),
    ];
    assert_eq!(
        outcomes,
        [
            (manga_holds.clone(), true, true, false, true, false),
            (manga_holds, true, true, false, true, false),
        ],
        "pattern first, then pattern later: Manga holds Adult's rows, its entry and its \
         member; the work is present, seen by Manga's reader and not by Kids'"
    );
}

/// The hand-back goes only where the outer's patterns admit the file. With
/// Manga excluding `kids/extras/Bonus/`, Kids' scan that comes to exclude
/// `extras/` hands Adult back to Manga and not Bonus: no walk reaches Bonus,
/// its member stays Kids' and its work is marked missing.
#[test]
fn a_file_the_nested_directory_comes_to_exclude_stays_where_the_outer_excludes_it_too() {
    let (_top, library, manga, kids) = kids_coming_to_exclude_extras(
        "nest-exclude-later-both",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/extras/Adult/Adult v01.cbz", 2),
            ("kids/extras/Bonus/Bonus v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    conn.execute(
        "UPDATE root SET exclude_globs = 'kids/extras/Bonus/' WHERE id = ?1",
        [manga.id],
    )
    .expect("Manga excludes kids/extras/Bonus/");
    let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
    let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
    let (adult_work, bonus_work) = (work_of(&conn, &adult), work_of(&conn, &bonus));

    let report = scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        report.forgotten_by_exclusion, 5,
        "extras, both directories and both files: {report:?}"
    );
    assert_eq!(
        (
            entries_for(&conn, &adult),
            members_at(&conn, &adult).0,
            marked_missing(&conn, adult_work)
        ),
        (vec![manga.id], vec![Some(manga.id)], false),
        "Adult goes back to Manga"
    );
    assert_eq!(
        (
            entries_for(&conn, &bonus),
            members_at(&conn, &bonus).0,
            marked_missing(&conn, bonus_work)
        ),
        (vec![], vec![Some(kids.id)], true),
        "Bonus stays Kids' and is marked missing"
    );
    assert_eq!(
        paths_under(&conn, &manga, "kids"),
        [
            "kids/extras",
            "kids/extras/Adult",
            "kids/extras/Adult/Adult v01.cbz"
        ],
        "nothing of Bonus goes to Manga"
    );
}

/// The way out of the hand-back: Kids lifts its pattern, and its next scan
/// takes Adult's rows and member over from Manga again, counted as taken over.
#[test]
fn a_file_handed_back_is_taken_over_again_when_the_nested_directory_stops_excluding_it() {
    let (_top, library, manga, kids) = kids_coming_to_exclude_extras(
        "nest-exclude-later-lifted",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/extras/Adult/Adult v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
    let work = work_of(&conn, &adult);
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    conn.execute(
        "UPDATE root SET exclude_globs = NULL WHERE id = ?1",
        [kids.id],
    )
    .expect("Kids stops excluding extras/");
    let kids = Root {
        exclude_globs: None,
        ..kids
    };

    let report = scan_root(&conn, &kids).expect("scans Kids again");
    crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            report.taken_over,
            report.gone,
            report.forgotten_by_exclusion
        ),
        (3, 0, 0),
        "extras, extras/Adult and the file, taken from Manga: {report:?}"
    );
    assert_eq!(
        (
            entries_for(&conn, &adult),
            members_at(&conn, &adult),
            rows_under(&conn, &manga, "kids"),
            marked_missing(&conn, work)
        ),
        (
            vec![kids.id],
            (vec![Some(kids.id)], vec![kids.id]),
            0,
            false
        ),
        "Adult's entry and member are Kids' again, and its work is present"
    );
}

/// §8.1 does not count a nested directory's own rows that go back to the
/// directory around it. Kids took over `extras/`, all it holds, and comes to
/// exclude it, so its scan reads empty. Where Manga admits `extras/`, the scan
/// is applied and hands Adult back, as the same pattern before Kids' first
/// scan would leave it; where Manga excludes it too, it would drop the rows
/// and is refused.
#[test]
fn an_empty_read_of_a_nested_directory_is_not_refused_for_the_rows_it_hands_back() {
    for outer_excludes in [false, true] {
        let (_top, library, manga, kids) = kids_coming_to_exclude_extras(
            &format!("nest-exclude-later-empty-{outer_excludes}"),
            &[("kids/extras/Adult/Adult v01.cbz", 2)],
        );
        let conn = library.conn();
        if outer_excludes {
            conn.execute(
                "UPDATE root SET exclude_globs = 'kids/extras/' WHERE id = ?1",
                [manga.id],
            )
            .expect("Manga excludes kids/extras/");
        }
        let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
        assert_eq!(
            entries_for(&conn, &adult),
            [kids.id],
            "the fixture: Kids took Adult over"
        );

        let report = scan_root(&conn, &kids).expect("scans Kids again");

        let holder = if outer_excludes { kids.id } else { manga.id };
        assert_eq!(
            (
                report.refused.is_some(),
                report.forgotten_by_exclusion,
                entries_for(&conn, &adult)
            ),
            (
                outer_excludes,
                if outer_excludes { 0 } else { 3 },
                vec![holder]
            ),
            "outer_excludes={outer_excludes}: {report:?}"
        );
    }
}

/// Sets `root`'s patterns to `globs`, in the store and in `root`.
fn set_patterns(conn: &Connection, root: &mut Root, globs: Option<&str>) {
    conn.execute(
        "UPDATE root SET exclude_globs = ?1 WHERE id = ?2",
        rusqlite::params![globs, root.id],
    )
    .expect("sets the patterns");
    root.exclude_globs = globs.map(str::to_string);
}

/// Decision 21 keeps and hands back by one rule. With Kids excluding `extras/`
/// and Manga `kids/extras/Bonus/`, whichever came first, Bonus ends with no
/// row, marked and unseen, and Adult with Manga, present: (A) both patterns
/// before Kids' first scan, (B) both after it, (C) Kids' before it and
/// Manga's after. Bonus's member stays where it was: Manga's where Kids' scan
/// never took Bonus over, Kids' where it did; removing Kids would remove
/// nothing in any order (the owner's ruling of 2026-11-10, "Never delete on
/// disk").
#[test]
fn a_file_both_directories_exclude_ends_the_same_whichever_pattern_came_first() {
    let mut outcomes = Vec::new();
    let mut bonus_member_mangas = Vec::new();
    let mut kids_counts = Vec::new();
    for order in ['A', 'B', 'C'] {
        let (_top, library, mut manga) = manga_holding(
            &format!("nest-both-exclude-{order}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Adult/Adult v01.cbz", 2),
                ("kids/extras/Bonus/Bonus v01.cbz", 4),
            ],
        );
        let conn = library.conn();
        let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
        let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
        let (adult_work, bonus_work) = (work_of(&conn, &adult), work_of(&conn, &bonus));
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        let reader = reader_of(&conn, "m", manga.libraries[0]);
        let mut counts = (0, 0);
        let mut scan_kids = |kids: &Root| {
            let report = scan_root(&conn, kids).expect("scans Kids");
            counts.0 += report.gone;
            counts.1 += report.forgotten_by_exclusion;
        };
        if order == 'B' {
            scan_kids(&kids);
            crate::derive::derive(&conn).expect("derives");
        }
        set_patterns(&conn, &mut kids, Some("extras/"));
        if order == 'C' {
            scan_kids(&kids);
            crate::derive::derive(&conn).expect("derives");
        }
        set_patterns(&conn, &mut manga, Some("kids/extras/Bonus/"));
        if order != 'C' {
            scan_kids(&kids);
        }
        crate::derive::derive(&conn).expect("derives");
        scan_root(&conn, &manga).expect("scans Manga");
        scan_kids(&kids);
        crate::derive::derive(&conn).expect("derives");
        kids_counts.push(counts);

        outcomes.push((
            paths_under(&conn, &manga, "kids"),
            entries_for(&conn, &bonus).is_empty(),
            marked_missing(&conn, bonus_work),
            reader_sees(&conn, reader, bonus_work),
            entries_for(&conn, &adult) == [manga.id],
            marked_missing(&conn, adult_work),
            reader_sees(&conn, reader, adult_work),
        ));
        bonus_member_mangas.push((
            members_at(&conn, &bonus).0 == [Some(manga.id)],
            counted(&removal_cost(&conn, kids.id).expect("rehearses")),
        ));
    }

    let manga_holds = vec![
        "kids/extras".to_string(),
        "kids/extras/Adult".to_string(),
        "kids/extras/Adult/Adult v01.cbz".to_string(),
    ];
    let expected = (manga_holds, true, true, false, true, false, true);
    assert_eq!(
        outcomes,
        [expected.clone(), expected.clone(), expected],
        "orders A, B, C: Manga holds Adult's rows, Bonus has none and is marked and \
         unseen, Adult is present and seen"
    );
    assert_eq!(
        bonus_member_mangas,
        [(true, (0, 0, 0)), (false, (0, 0, 0)), (true, (0, 0, 0))],
        "Bonus's member, Manga's in A and C and Kids' in B, and what removing Kids \
         would remove: nothing"
    );
    assert_eq!(
        kids_counts,
        [(0, 2), (0, 5), (0, 2)],
        "Kids' (gone, forgotten) over its scans: Bonus's two rows dropped in A and C, \
         and in B the three handed back besides; none gone"
    );
}

/// §8.1 at a nested directory's first scan counts the outer's rows no
/// directory would hold. Kids excludes `extras/`, all it would hold, so its
/// first scan reads empty. Where Manga admits `kids/extras/`, Manga keeps the
/// rows and the scan is applied; where Manga excludes it too, the scan would
/// drop them and is refused, and Manga still holds them.
#[test]
fn an_empty_first_read_of_a_nested_directory_counts_the_outer_rows_no_directory_would_hold() {
    for outer_excludes in [false, true] {
        let (_top, library, mut manga) = manga_holding(
            &format!("nest-empty-first-{outer_excludes}"),
            &[("kids/extras/Adult/Adult v01.cbz", 2)],
        );
        let conn = library.conn();
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        set_patterns(&conn, &mut kids, Some("extras/"));
        if outer_excludes {
            set_patterns(&conn, &mut manga, Some("kids/extras/"));
        }

        let report = scan_root(&conn, &kids).expect("scans Kids");

        assert_eq!(
            (report.refused.is_some(), paths_under(&conn, &manga, "kids")),
            (
                outer_excludes,
                vec![
                    "kids/extras".to_string(),
                    "kids/extras/Adult".to_string(),
                    "kids/extras/Adult/Adult v01.cbz".to_string()
                ]
            ),
            "outer_excludes={outer_excludes}: {report:?}"
        );
    }
}

/// Three levels, Kids excluding `little/extras/` and Little `extras/`. The
/// directory around Little is Kids, whose walk does not reach Adult, so Adult
/// ends with no row, marked and unseen, whether Little's pattern came before
/// its first scan or after: Manga's walk would reach it, but a hand-back gives
/// Little's rows to Kids alone. Tiny, which Little records, is present.
#[test]
fn three_levels_where_the_middle_directory_excludes_end_the_same_whichever_came_first() {
    let mut outcomes = Vec::new();
    let mut adult_member = Vec::new();
    for pattern_first in [true, false] {
        let (_top, library, manga) = manga_holding(
            &format!("nest-three-middle-excludes-{pattern_first}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/little/Tiny/Tiny v01.cbz", 2),
                ("kids/little/extras/Adult/Adult v01.cbz", 4),
            ],
        );
        let conn = library.conn();
        let adult = manga.path.join("kids/little/extras/Adult/Adult v01.cbz");
        let tiny = manga.path.join("kids/little/Tiny/Tiny v01.cbz");
        let work = work_of(&conn, &adult);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        set_patterns(&conn, &mut kids, Some("little/extras/"));
        let mut little = add_root_checked_in(
            &conn,
            &manga.path.join("kids/little"),
            InLibrary::Named("Little"),
        )
        .expect("adds Little")
        .root;
        let readers = [
            reader_of(&conn, "m", manga.libraries[0]),
            reader_of(&conn, "k", kids.libraries[0]),
            reader_of(&conn, "l", little.libraries[0]),
        ];
        if pattern_first {
            set_patterns(&conn, &mut little, Some("extras/"));
        }
        scan_root(&conn, &kids).expect("scans Kids");
        scan_root(&conn, &little).expect("scans Little");
        crate::derive::derive(&conn).expect("derives");
        if !pattern_first {
            set_patterns(&conn, &mut little, Some("extras/"));
            scan_root(&conn, &little).expect("scans Little again");
            crate::derive::derive(&conn).expect("derives");
        }
        scan_root(&conn, &kids).expect("scans Kids again");
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");

        outcomes.push((
            entries_for(&conn, &adult),
            marked_missing(&conn, work),
            readers.map(|reader| reader_sees(&conn, reader, work)),
            entries_for(&conn, &tiny) == [little.id],
        ));
        adult_member.push(if members_at(&conn, &adult).0 == [Some(manga.id)] {
            "Manga"
        } else if members_at(&conn, &adult).0 == [Some(little.id)] {
            "Little"
        } else {
            "other"
        });
    }

    assert_eq!(
        outcomes,
        [
            (vec![], true, [false, false, false], true),
            (vec![], true, [false, false, false], true)
        ],
        "pattern first, then pattern later: Adult has no row, is marked and unseen; \
         Tiny is Little's"
    );
    assert_eq!(
        adult_member,
        ["Manga", "Little"],
        "Adult's member stays where it was"
    );
}

/// Three levels with Kids admitting what Little excludes: Adult goes to Kids,
/// row and member, whether Little's pattern came before its first scan, when
/// Little's scan moves Manga's rows to Kids, or after, when it hands its own
/// back. Manga's and Kids' readers see it and Little's does not.
#[test]
fn three_levels_where_the_middle_directory_admits_end_the_same_whichever_came_first() {
    let mut outcomes = Vec::new();
    for pattern_first in [true, false] {
        let (_top, library, manga) = manga_holding(
            &format!("nest-three-middle-admits-{pattern_first}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/little/Tiny/Tiny v01.cbz", 2),
                ("kids/little/extras/Adult/Adult v01.cbz", 4),
            ],
        );
        let conn = library.conn();
        let adult = manga.path.join("kids/little/extras/Adult/Adult v01.cbz");
        let work = work_of(&conn, &adult);
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        let mut little = add_root_checked_in(
            &conn,
            &manga.path.join("kids/little"),
            InLibrary::Named("Little"),
        )
        .expect("adds Little")
        .root;
        let readers = [
            reader_of(&conn, "m", manga.libraries[0]),
            reader_of(&conn, "k", kids.libraries[0]),
            reader_of(&conn, "l", little.libraries[0]),
        ];
        if pattern_first {
            set_patterns(&conn, &mut little, Some("extras/"));
        }
        scan_root(&conn, &kids).expect("scans Kids");
        scan_root(&conn, &little).expect("scans Little");
        crate::derive::derive(&conn).expect("derives");
        if !pattern_first {
            set_patterns(&conn, &mut little, Some("extras/"));
            scan_root(&conn, &little).expect("scans Little again");
            crate::derive::derive(&conn).expect("derives");
        }
        scan_root(&conn, &kids).expect("scans Kids again");
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");

        outcomes.push((
            paths_under(&conn, &kids, "little"),
            paths_under(&conn, &manga, "kids"),
            entries_for(&conn, &adult) == [kids.id],
            members_at(&conn, &adult) == (vec![Some(kids.id)], vec![kids.id]),
            marked_missing(&conn, work),
            readers.map(|reader| reader_sees(&conn, reader, work)),
        ));
    }

    let kids_holds = vec![
        "little/extras".to_string(),
        "little/extras/Adult".to_string(),
        "little/extras/Adult/Adult v01.cbz".to_string(),
    ];
    let expected = (kids_holds, vec![], true, true, false, [true, true, false]);
    assert_eq!(
        outcomes,
        [expected.clone(), expected],
        "pattern first, then pattern later: Kids holds Adult's rows and member, Manga \
         holds nothing under kids, and Manga's and Kids' readers see Adult"
    );
}

/// Kids' patterns `extras/` then `!extras/keep/**`: its walk never descends
/// into `extras`, so it never reaches `extras/keep/…`, though the second line
/// names that path. Keep, on disk, stays with Manga, present and seen by
/// Manga's reader, whether the patterns came before Kids' first scan or after,
/// and neither Kids scan counts it gone.
#[test]
fn a_whitelisted_child_of_an_excluded_directory_stays_with_the_outer_whichever_came_first() {
    let mut outcomes = Vec::new();
    let mut counts = Vec::new();
    for pattern_first in [true, false] {
        let (_top, library, manga) = manga_holding(
            &format!("nest-whitelist-{pattern_first}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/keep/Keep v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let keep = manga.path.join("kids/extras/keep/Keep v01.cbz");
        let work = work_of(&conn, &keep);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        let readers = [
            reader_of(&conn, "m", manga.libraries[0]),
            reader_of(&conn, "k", kids.libraries[0]),
        ];
        if !pattern_first {
            scan_root(&conn, &kids).expect("scans Kids");
            crate::derive::derive(&conn).expect("derives");
        }
        set_patterns(&conn, &mut kids, Some("extras/\n!extras/keep/**"));
        let report = scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");

        assert!(keep.exists(), "the fixture: Keep is on disk");
        counts.push((report.gone, report.forgotten_by_exclusion));
        outcomes.push((
            paths_under(&conn, &manga, "kids"),
            entries_for(&conn, &keep) == [manga.id],
            members_at(&conn, &keep) == (vec![Some(manga.id)], vec![manga.id]),
            marked_missing(&conn, work),
            readers.map(|reader| reader_sees(&conn, reader, work)),
        ));
    }

    let manga_holds = vec![
        "kids/extras".to_string(),
        "kids/extras/keep".to_string(),
        "kids/extras/keep/Keep v01.cbz".to_string(),
    ];
    let expected = (manga_holds, true, true, false, [true, false]);
    assert_eq!(
        outcomes,
        [expected.clone(), expected],
        "pattern first, then pattern later: Manga holds Keep's rows and member, and \
         Keep is present for Manga's reader alone"
    );
    assert_eq!(
        counts,
        [(0, 0), (0, 3)],
        "Kids' (gone, forgotten): kept rows count nowhere, handed-back rows as forgotten"
    );
}

/// Decision 21, the way back: with a reading position on the work of a file
/// Kids' patterns exclude, removing Kids after its scan counts nothing, in the
/// rehearsal and the removal alike. The work keeps its id and the position,
/// and Manga's next scan meets the file again under the same work.
#[test]
fn removing_a_nested_directory_loses_nothing_its_patterns_excluded() {
    let (_top, library, manga, kids, adult) = kids_excluding_extras("nest-excluded-round-trip");
    let conn = library.conn();
    let work = work_of(&conn, &adult);
    let positions = |work: i64| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM progress WHERE work_id = ?1",
            [work],
            |r| r.get(0),
        )
        .expect("counts the positions")
    };
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 1)",
        [work],
    )
    .expect("a position");
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");

    let cost = removal_cost(&conn, kids.id).expect("rehearses");
    let removed = remove_root(&conn, kids.id).expect("removes Kids");
    assert_eq!((counted(&cost), counted(&removed)), ((0, 0, 0), (0, 0, 0)));
    assert_eq!(work_of(&conn, &adult), work, "the work keeps its id");
    assert_eq!(positions(work), 1, "and its position");

    scan_root(&conn, &manga).expect("scans Manga");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        work_of(&conn, &adult),
        work,
        "Manga's scan meets the same work"
    );
    assert_eq!(positions(work), 1);
}

/// What became of a file Kids took over at its first scan, before Kids is
/// removed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LaterOnKids {
    /// Kept out by a pattern Kids added; Kids' scan hands it back to Manga.
    Excluded,
    /// Kept out by Kids' pattern and by Manga's; no walk reaches it.
    ExcludedByBoth,
    /// Kept out by both at Kids' scan, which hands nothing back; then Manga
    /// clears its pattern, so its walk reaches the file once Kids is gone.
    OuterAdmitsLater,
    /// Gone from the disk.
    Vanished,
}

/// Decision 21 for a file Kids recorded and later stopped recording, with a
/// reading position on its work, removing Kids' directory and Kids whole.
/// Where the file is on disk, the rehearsal and the removal count nothing, its
/// member is Manga's, and Manga's next scan meets the same work, position
/// kept. Where both directories exclude it, no walk reaches it: the work is
/// kept all the same, marked missing until Manga's pattern goes and its scan
/// records the file (the owner's ruling of 2026-11-10, "Never delete on
/// disk"). Only a vanished file's work and position are counted removed.
#[test]
fn removing_a_nested_directory_hands_back_a_file_it_stopped_recording_where_the_outer_reaches_it() {
    for (case, whole_library) in [
        LaterOnKids::Excluded,
        LaterOnKids::ExcludedByBoth,
        LaterOnKids::OuterAdmitsLater,
        LaterOnKids::Vanished,
    ]
    .into_iter()
    .flat_map(|case| [(case, false), (case, true)])
    {
        let (_top, library, mut manga) = manga_holding(
            &format!("nest-later-{case:?}-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Adult/Adult v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let adult = manga.path.join("kids/extras/Adult/Adult v01.cbz");
        let work = work_of(&conn, &adult);
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 1)",
            [work],
        )
        .expect("a position");
        let positions = || -> i64 {
            conn.query_row(
                "SELECT count(*) FROM progress WHERE work_id = ?1",
                [work],
                |r| r.get(0),
            )
            .expect("counts the positions")
        };
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            members_at(&conn, &adult),
            (vec![Some(kids.id)], vec![kids.id]),
            "{case:?}: the fixture, Kids took the file over"
        );

        if case == LaterOnKids::Vanished {
            std::fs::remove_file(&adult).expect("deletes the file");
        } else {
            conn.execute(
                "UPDATE root SET exclude_globs = 'extras/' WHERE id = ?1",
                [kids.id],
            )
            .expect("Kids excludes extras/");
            kids.exclude_globs = Some("extras/".to_string());
        }
        if matches!(
            case,
            LaterOnKids::ExcludedByBoth | LaterOnKids::OuterAdmitsLater
        ) {
            set_patterns(&conn, &mut manga, Some("kids/extras/"));
        }
        scan_root(&conn, &kids).expect("scans Kids again");
        crate::derive::derive(&conn).expect("derives");
        let holder = if case == LaterOnKids::Excluded {
            (vec![manga.id], vec![Some(manga.id)])
        } else {
            (vec![], vec![Some(kids.id)])
        };
        assert_eq!(
            (entries_for(&conn, &adult), members_at(&conn, &adult).0),
            holder,
            "{case:?}: the fixture, the file's row and member after Kids' scan"
        );
        if case == LaterOnKids::OuterAdmitsLater {
            set_patterns(&conn, &mut manga, None);
        }

        let (cost, removed) = if whole_library {
            (
                library_removal_cost(&conn, kids.libraries[0]).expect("rehearses"),
                remove_library(&conn, kids.libraries[0]).expect("removes Kids"),
            )
        } else {
            (
                removal_cost(&conn, kids.id).expect("rehearses"),
                remove_root(&conn, kids.id).expect("removes Kids"),
            )
        };
        if case == LaterOnKids::Vanished {
            assert_eq!(
                (counted(&cost), counted(&removed)),
                ((1, 1, 0), (1, 1, 0)),
                "{case:?} whole_library={whole_library}"
            );
            assert_eq!(positions(), 0, "{case:?}");
        } else {
            assert_eq!(
                (counted(&cost), counted(&removed)),
                ((0, 0, 0), (0, 0, 0)),
                "{case:?} whole_library={whole_library}"
            );
            assert_eq!(
                members_at(&conn, &adult).0,
                vec![Some(manga.id)],
                "{case:?} whole_library={whole_library}"
            );
            scan_root(&conn, &manga).expect("scans Manga");
            crate::derive::derive(&conn).expect("derives");
            assert_eq!(
                (work_of(&conn, &adult), positions()),
                (work, 1),
                "{case:?} whole_library={whole_library}: Manga's scan meets the same work, \
                 position kept"
            );
            assert_eq!(
                marked_missing(&conn, work),
                case == LaterOnKids::ExcludedByBoth,
                "{case:?} whole_library={whole_library}: missing only while Manga's pattern \
                 keeps its walk out"
            );
        }
    }
}

/// The positions and notes on work `work`.
fn kept_on(conn: &Connection, work: i64) -> (i64, i64) {
    let count = |sql: &str| -> i64 {
        conn.query_row(sql, [work], |r| r.get(0))
            .expect("counts a reader's rows")
    };
    (
        count("SELECT count(*) FROM progress WHERE work_id = ?1"),
        count("SELECT count(*) FROM note WHERE work_id = ?1"),
    )
}

/// The owner's ruling of 2026-11-10, "Never delete on disk". Kids excludes
/// `extras/` and Manga `kids/extras/Bonus/`, and Bonus's work holds a position
/// and a note. Removing Kids, its directory or its library, while Manga still
/// excludes Bonus counts nothing, whether both patterns came before Kids'
/// first scan (A: Bonus's member is Manga's) or after it (B: Kids'). The work
/// is kept, its member Manga's, marked missing and unseen; once Manga lifts
/// its pattern and scans, the same work is back with its position and note.
#[test]
fn removing_a_nested_directory_keeps_the_work_of_a_file_both_directories_exclude() {
    let mut holders = Vec::new();
    let mut outcomes = Vec::new();
    for (order, whole_library) in [('A', false), ('A', true), ('B', false), ('B', true)] {
        let (_top, library, mut manga) = manga_holding(
            &format!("nest-never-delete-{order}-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Adult/Adult v01.cbz", 2),
                ("kids/extras/Bonus/Bonus v01.cbz", 4),
            ],
        );
        let conn = library.conn();
        let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
        let work = work_of(&conn, &bonus);
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
            [work],
        )
        .expect("a position");
        conn.execute(
            "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
             VALUES (1, 'n', ?1, 'raw', 2, 'a note', 'now', 'now')",
            [work],
        )
        .expect("a note");
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        let reader = reader_of(&conn, "m", manga.libraries[0]);
        if order == 'B' {
            scan_root(&conn, &kids).expect("scans Kids");
            crate::derive::derive(&conn).expect("derives");
        }
        set_patterns(&conn, &mut kids, Some("extras/"));
        set_patterns(&conn, &mut manga, Some("kids/extras/Bonus/"));
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");
        holders.push(members_at(&conn, &bonus).0 == [Some(kids.id)]);

        let (cost, removed) = if whole_library {
            (
                library_removal_cost(&conn, kids.libraries[0]).expect("rehearses"),
                remove_library(&conn, kids.libraries[0]).expect("removes Kids"),
            )
        } else {
            (
                removal_cost(&conn, kids.id).expect("rehearses"),
                remove_root(&conn, kids.id).expect("removes Kids"),
            )
        };
        let kept = conn
            .query_row("SELECT count(*) FROM work WHERE id = ?1", [work], |r| {
                r.get::<_, i64>(0)
            })
            .expect("counts the work")
            == 1;
        let after_removal = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            members_at(&conn, &bonus) == (vec![Some(manga.id)], vec![manga.id]),
            kept && marked_missing(&conn, work),
            kept && reader_sees(&conn, reader, work),
        );

        set_patterns(&conn, &mut manga, None);
        scan_root(&conn, &manga).expect("scans Manga, its pattern lifted");
        crate::derive::derive(&conn).expect("derives");
        let after_lift = (
            work_of(&conn, &bonus) == work,
            kept_on(&conn, work),
            kept && marked_missing(&conn, work),
            kept && reader_sees(&conn, reader, work),
        );
        outcomes.push((order, whole_library, after_removal, after_lift));
    }

    assert_eq!(
        holders,
        [false, false, true, true],
        "the fixture: Bonus's member is Kids' in order B alone"
    );
    let expected = |order, whole_library| {
        (
            order,
            whole_library,
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true, false),
            (true, (1, 1), false, true),
        )
    };
    assert_eq!(
        outcomes,
        [
            expected('A', false),
            expected('A', true),
            expected('B', false),
            expected('B', true)
        ],
        "after Kids goes: (rehearsal, removal, the work kept, its position and note, its \
         member Manga's, marked, seen by Manga's reader); after Manga lifts its pattern: \
         (the same work, its position and note, marked, seen)"
    );
}

/// The owner's ruling of 2026-11-11, "Fix it", on the test above's order B,
/// where Kids holds Bonus's member with no row. Removing Kids, its directory
/// or its library, deletes Bonus's work, position and note only where Kids'
/// walk would reach the file and the disk lacks it. Lifting Kids' pattern
/// runs no scan; with the file on disk, the dialog and the removal count
/// nothing and the work is Manga's, missing, until Manga lifts its pattern
/// and scans. With Kids' pattern kept, a file gone from disk is kept too, as
/// its walk may never have looked for it.
#[test]
fn removing_a_directory_whose_pattern_was_lifted_unscanned_asks_the_disk() {
    let mut outcomes = Vec::new();
    for ((lifted, on_disk), whole_library) in [(true, true), (true, false), (false, false)]
        .into_iter()
        .flat_map(|case| [(case, false), (case, true)])
    {
        let (_top, library, mut manga) = manga_holding(
            &format!("nest-lifted-unscanned-{lifted}-{on_disk}-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Adult/Adult v01.cbz", 2),
                ("kids/extras/Bonus/Bonus v01.cbz", 4),
            ],
        );
        let conn = library.conn();
        let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
        let work = work_of(&conn, &bonus);
        read_on(&conn, work);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        set_patterns(&conn, &mut kids, Some("extras/"));
        set_patterns(&conn, &mut manga, Some("kids/extras/Bonus/"));
        scan_root(&conn, &kids).expect("scans Kids again");
        crate::derive::derive(&conn).expect("derives");
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");
        let case = format!("lifted={lifted}, on_disk={on_disk}, whole_library={whole_library}");
        assert_eq!(
            (entries_for(&conn, &bonus), held_by(&conn, &bonus, &kids)),
            (vec![], true),
            "{case}: the fixture, Kids holds Bonus's member with no row"
        );
        if lifted {
            let lift = RootPatch {
                exclude_globs: Some(None),
                ..RootPatch::default()
            };
            assert!(
                update_root(&conn, kids.id, &lift).expect("lifts Kids' pattern"),
                "{case}: Kids exists"
            );
        }
        if !on_disk {
            std::fs::remove_file(&bonus).expect("deletes the file");
        }

        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        let kept = work_kept(&conn, work);
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            kept && held_by(&conn, &bonus, &manga),
            kept && marked_missing(&conn, work),
        );
        let shown = on_disk.then(|| {
            set_patterns(&conn, &mut manga, None);
            scan_root(&conn, &manga).expect("scans Manga, its pattern lifted");
            crate::derive::derive(&conn).expect("derives");
            (
                work_of(&conn, &bonus) == work,
                kept_on(&conn, work),
                kept && marked_missing(&conn, work),
            )
        });
        outcomes.push(((lifted, on_disk, whole_library), after, shown));
    }
    let kept = (0, 0, 0);
    let removed = (1, 1, 1);
    let expected = [
        (
            (true, true, false),
            (kept, kept, true, (1, 1), true, true),
            Some((true, (1, 1), false)),
        ),
        (
            (true, true, true),
            (kept, kept, true, (1, 1), true, true),
            Some((true, (1, 1), false)),
        ),
        (
            (true, false, false),
            (removed, removed, false, (0, 0), false, false),
            None,
        ),
        (
            (true, false, true),
            (removed, removed, false, (0, 0), false, false),
            None,
        ),
        (
            (false, false, false),
            (kept, kept, true, (1, 1), true, true),
            None,
        ),
        (
            (false, false, true),
            (kept, kept, true, (1, 1), true, true),
            None,
        ),
    ];
    assert_eq!(
        outcomes, expected,
        "(lifted, on disk, whole library); after Kids goes: (dialog, removal, kept, position \
         and note, Manga's, missing); after Manga lifts its pattern: (the same work, position \
         and note, missing)"
    );
}

/// What the disk did to Bonus, in the test above's layout with Kids' pattern
/// lifted, before Kids goes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum UnderKids {
    /// Kids' tree moved off, nothing left at its path.
    Absent,
    /// Kids' tree moved off, an empty directory left at its path: an
    /// unmounted share.
    Empty,
    /// As `Empty`, with only a dot-named entry, which no walk visits.
    OnlyDotNames,
    /// `extras/Bonus` a plain file: the stat fails, not with "not found".
    NotADirectory,
    /// Bonus's file deleted, Kids' tree otherwise there.
    Gone,
}

/// The owner's ruling of 2026-11-11, "Add a guard", §8.1's at a removal: a
/// file the disk answers "not found" for is gone only where each directory
/// whose tree holds it reads present and holds a name no walk skips, and a
/// stat that fails otherwise proves nothing. Removing Kids, its directory or
/// its library, then counts nothing and keeps Bonus's work, Manga's and
/// missing, until the tree is back and Manga scans; only a file gone from a
/// Kids that reads present is counted and removed.
#[test]
fn a_removal_reads_no_file_as_gone_where_its_directory_reads_absent_or_empty() {
    let mut outcomes = Vec::new();
    for (case, whole_library) in [
        UnderKids::Absent,
        UnderKids::Empty,
        UnderKids::OnlyDotNames,
        UnderKids::NotADirectory,
        UnderKids::Gone,
    ]
    .into_iter()
    .flat_map(|case| [(case, false), (case, true)])
    {
        let (top, library, mut manga) = manga_holding(
            &format!("unmounted-{case:?}-{whole_library}"),
            &[
                ("Naruto/v01.cbz", 2),
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Adult/Adult v01.cbz", 2),
                ("kids/extras/Bonus/Bonus v01.cbz", 4),
            ],
        );
        let conn = library.conn();
        let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
        let work = work_of(&conn, &bonus);
        read_on(&conn, work);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        set_patterns(&conn, &mut kids, Some("extras/"));
        set_patterns(&conn, &mut manga, Some("kids/extras/Bonus/"));
        scan_root(&conn, &kids).expect("scans Kids again");
        crate::derive::derive(&conn).expect("derives");
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (entries_for(&conn, &bonus), held_by(&conn, &bonus, &kids)),
            (vec![], true),
            "{case:?} whole_library={whole_library}: the fixture, Kids holds Bonus's member \
             with no row"
        );
        let lift = RootPatch {
            exclude_globs: Some(None),
            ..RootPatch::default()
        };
        assert!(update_root(&conn, kids.id, &lift).expect("lifts Kids' pattern"));

        let away = top.path().join("away");
        match case {
            UnderKids::Absent => std::fs::rename(&kids.path, &away).expect("moves Kids off"),
            UnderKids::Empty | UnderKids::OnlyDotNames => {
                std::fs::rename(&kids.path, &away).expect("moves Kids off");
                std::fs::create_dir(&kids.path).expect("leaves the mount point");
                if case == UnderKids::OnlyDotNames {
                    std::fs::create_dir(kids.path.join(".taiyaku")).expect("a dot name");
                }
            }
            UnderKids::NotADirectory => {
                let dir = kids.path.join("extras/Bonus");
                std::fs::remove_dir_all(&dir).expect("removes Bonus");
                std::fs::write(&dir, b"not a directory").expect("a file in its place");
            }
            UnderKids::Gone => std::fs::remove_file(&bonus).expect("deletes the file"),
        }
        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        let kept = work_kept(&conn, work);
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            kept && held_by(&conn, &bonus, &manga),
            kept && marked_missing(&conn, work),
        );
        let back = matches!(
            case,
            UnderKids::Absent | UnderKids::Empty | UnderKids::OnlyDotNames
        )
        .then(|| {
            if case != UnderKids::Absent {
                std::fs::remove_dir_all(&kids.path).expect("clears the mount point");
            }
            std::fs::rename(&away, &kids.path).expect("puts Kids back");
            set_patterns(&conn, &mut manga, None);
            scan_root(&conn, &manga).expect("scans Manga, its pattern lifted");
            crate::derive::derive(&conn).expect("derives");
            (
                work_of(&conn, &bonus) == work,
                kept_on(&conn, work),
                kept && marked_missing(&conn, work),
            )
        });
        outcomes.push((case, whole_library, after, back));
    }
    let kept = |case, whole_library, back: bool| {
        (
            case,
            whole_library,
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
            back.then_some((true, (1, 1), false)),
        )
    };
    let gone = |whole_library| {
        (
            UnderKids::Gone,
            whole_library,
            ((1, 1, 1), (1, 1, 1), false, (0, 0), false, false),
            None,
        )
    };
    assert_eq!(
        outcomes,
        [
            kept(UnderKids::Absent, false, true),
            kept(UnderKids::Absent, true, true),
            kept(UnderKids::Empty, false, true),
            kept(UnderKids::Empty, true, true),
            kept(UnderKids::OnlyDotNames, false, true),
            kept(UnderKids::OnlyDotNames, true, true),
            kept(UnderKids::NotADirectory, false, false),
            kept(UnderKids::NotADirectory, true, false),
            gone(false),
            gone(true),
        ],
        "after Kids goes: (dialog, removal, kept, position and note, Manga's, missing); \
         once the tree is back and Manga scans: (the same work, position and note, missing)"
    );
}

/// "Never delete on disk", three deep: `m` ⊃ `m/kids` ⊃ `m/kids/little`. Kids
/// took Adult over at its first scan, then Kids and Manga both came to exclude
/// it, and Little, added after, excludes it too, so no directory left reaches
/// it. Removing Kids counts nothing: Adult's member goes to Little, the deepest
/// directory left around the file, its work marked missing; once Little lifts
/// its pattern and scans, the same work is back with its position.
#[test]
fn removing_the_middle_of_three_keeps_the_work_of_a_file_no_directory_left_reaches() {
    let (_top, library, mut manga) = manga_holding(
        "nest-never-delete-three",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/little/Tiny/Tiny v01.cbz", 2),
            ("kids/little/extras/Adult/Adult v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    let adult = manga.path.join("kids/little/extras/Adult/Adult v01.cbz");
    let work = work_of(&conn, &adult);
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 1)",
        [work],
    )
    .expect("a position");
    let mut kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    set_patterns(&conn, &mut kids, Some("little/extras/"));
    set_patterns(&conn, &mut manga, Some("kids/little/extras/"));
    scan_root(&conn, &kids).expect("scans Kids again");
    crate::derive::derive(&conn).expect("derives");
    let mut little = add_root_checked_in(
        &conn,
        &manga.path.join("kids/little"),
        InLibrary::Named("Little"),
    )
    .expect("adds Little")
    .root;
    set_patterns(&conn, &mut little, Some("extras/"));
    scan_root(&conn, &little).expect("scans Little");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (
            entries_for(&conn, &adult),
            members_at(&conn, &adult).0,
            marked_missing(&conn, work)
        ),
        (vec![], vec![Some(kids.id)], true),
        "the fixture: Adult has no row, its member is Kids', and its work is missing"
    );

    let cost = removal_cost(&conn, kids.id).expect("rehearses");
    let removed = remove_root(&conn, kids.id).expect("removes Kids");
    assert_eq!((counted(&cost), counted(&removed)), ((0, 0, 0), (0, 0, 0)));
    assert_eq!(
        (members_at(&conn, &adult), marked_missing(&conn, work)),
        ((vec![Some(little.id)], vec![little.id]), true),
        "Adult's member is Little's, and its work is missing"
    );

    set_patterns(&conn, &mut little, None);
    scan_root(&conn, &little).expect("scans Little, its pattern lifted");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (
            work_of(&conn, &adult),
            kept_on(&conn, work).0,
            marked_missing(&conn, work)
        ),
        (work, 1, false),
        "Little's scan meets the same work, position kept"
    );
}

/// The ruling of 2026-11-10, "Cover both", three deep: Little, added after
/// Kids' first scan, excludes `extras/`, so Kids keeps Adult's row. Removing
/// Kids counts nothing. Where Manga, around Little once Kids is gone, admits
/// the file, it takes Adult's row and member, and keeps them at Little's next
/// scan: the work is shown. Where Manga excludes it, the member goes to Little
/// with no row: the work is missing. Once Little lifts its pattern and scans,
/// the same work is Little's and shown, its position kept.
#[test]
fn removing_the_middle_of_three_keeps_the_work_of_a_file_the_innermost_excludes() {
    let mut outcomes = Vec::new();
    for (manga_excludes, whole_library) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let (_top, library, mut manga) = manga_holding(
            &format!("nest-cover-both-{manga_excludes}-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/little/Tiny/Tiny v01.cbz", 2),
                ("kids/little/extras/Adult/Adult v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let adult = manga.path.join("kids/little/extras/Adult/Adult v01.cbz");
        let work = work_of(&conn, &adult);
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 1)",
            [work],
        )
        .expect("a position");
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        let mut little = add_root_checked_in(
            &conn,
            &manga.path.join("kids/little"),
            InLibrary::Named("Little"),
        )
        .expect("adds Little")
        .root;
        set_patterns(&conn, &mut little, Some("extras/"));
        scan_root(&conn, &little).expect("scans Little");
        crate::derive::derive(&conn).expect("derives");
        if manga_excludes {
            set_patterns(&conn, &mut manga, Some("kids/little/extras/"));
        }
        let reader = reader_of(&conn, "m", manga.libraries[0]);
        let (manga_id, little_id) = (manga.id, little.id);
        let name = move |id: i64| -> &'static str {
            if id == manga_id {
                "Manga"
            } else if id == little_id {
                "Little"
            } else {
                "another"
            }
        };
        assert_eq!(
            (
                entries_for(&conn, &adult),
                members_at(&conn, &adult).0,
                marked_missing(&conn, work)
            ),
            (vec![kids.id], vec![Some(kids.id)], false),
            "the fixture: Kids holds Adult's row and member, and the work is shown"
        );

        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        scan_root(&conn, &little).expect("scans Little");
        crate::derive::derive(&conn).expect("derives");
        let (roots, held) = members_at(&conn, &adult);
        let kept = conn
            .query_row("SELECT count(*) FROM work WHERE id = ?1", [work], |r| {
                r.get::<_, i64>(0)
            })
            .expect("counts the work")
            == 1;
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work).0,
            entries_for(&conn, &adult)
                .into_iter()
                .map(name)
                .collect::<Vec<_>>(),
            roots
                .into_iter()
                .map(|id| id.map(name))
                .chain(held.into_iter().map(|id| Some(name(id))))
                .collect::<Vec<_>>(),
            kept && marked_missing(&conn, work),
            kept && reader_sees(&conn, reader, work),
        );

        set_patterns(&conn, &mut little, None);
        scan_root(&conn, &little).expect("scans Little, its pattern lifted");
        crate::derive::derive(&conn).expect("derives");
        let lifted = (
            kept && work_of(&conn, &adult) == work,
            kept_on(&conn, work).0,
            entries_for(&conn, &adult)
                .into_iter()
                .map(name)
                .collect::<Vec<_>>(),
            kept && marked_missing(&conn, work),
        );
        outcomes.push((manga_excludes, whole_library, after, lifted));
    }

    let expected = |manga_excludes: bool, whole_library: bool| {
        let after = if manga_excludes {
            (vec![], vec![Some("Little"), Some("Little")], true, false)
        } else {
            (
                vec!["Manga"],
                vec![Some("Manga"), Some("Manga")],
                false,
                true,
            )
        };
        (
            manga_excludes,
            whole_library,
            (
                (0, 0, 0),
                (0, 0, 0),
                true,
                1,
                after.0,
                after.1,
                after.2,
                after.3,
            ),
            (true, 1, vec!["Little"], false),
        )
    };
    assert_eq!(
        outcomes,
        [
            expected(false, false),
            expected(false, true),
            expected(true, false),
            expected(true, true)
        ],
        "after Kids goes and Little scans: (rehearsal, removal, the same work, its \
         position, Adult's rows, its member's directories, missing, seen by Manga's \
         reader); after Little lifts its pattern: (the same work, its position, Adult's \
         rows, missing)"
    );
}

/// Decision 21 at a removal, four deep: `m` ⊃ `m/kids` ⊃ `m/kids/mid` ⊃
/// `m/kids/mid/little`, Mid and Little added after Kids' scan, Little
/// excluding `extras/` and Mid `little/extras/`, while Manga admits the file.
/// Removing Kids asks Little, then Mid, the directory immediately around it,
/// and no directory further out: Adult gets no row, its member goes to Little,
/// and its work is kept, marked missing.
#[test]
fn removing_a_nested_directory_asks_no_directory_further_out_than_the_one_around_the_innermost() {
    let (_top, library, manga) = manga_holding(
        "nest-cover-both-four",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/mid/Tiny/Tiny v01.cbz", 2),
            ("kids/mid/little/extras/Adult/Adult v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    let adult = manga
        .path
        .join("kids/mid/little/extras/Adult/Adult v01.cbz");
    let work = work_of(&conn, &adult);
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    scan_root(&conn, &kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    let added = |rel: &str, name: &str, globs: &str| -> Root {
        let mut root = add_root_checked_in(&conn, &manga.path.join(rel), InLibrary::Named(name))
            .unwrap_or_else(|e| panic!("adds {name}: {e:?}"))
            .root;
        set_patterns(&conn, &mut root, Some(globs));
        root
    };
    let _mid = added("kids/mid", "Mid", "little/extras/");
    let little = added("kids/mid/little", "Little", "extras/");
    assert_eq!(
        entries_for(&conn, &adult),
        [kids.id],
        "the fixture: Kids holds Adult's row"
    );

    let cost = removal_cost(&conn, kids.id).expect("rehearses");
    let removed = remove_root(&conn, kids.id).expect("removes Kids");
    assert_eq!(
        (
            counted(&cost),
            counted(&removed),
            entries_for(&conn, &adult),
            held_by(&conn, &adult, &little),
            marked_missing(&conn, work)
        ),
        ((0, 0, 0), (0, 0, 0), vec![], true, true),
        "nothing removed; Manga, further out than Mid, is not given Adult's row; its \
         member is Little's and its work missing"
    );
}

/// A file no walk recorded, under a directory nested in the one removed, is
/// read as vanished only where every walk that may last have looked for it
/// reaches it. Four deep, Mid and Little scanned before Kids goes: Little's
/// take-over dropped Kids' row for Adult (Mid, around Little, excludes it), so
/// Kids holds its member with no row, and Kids' own patterns admit the file.
/// Removing Kids, its directory or its library, counts nothing in the dialog
/// or the removal: the work is Little's, missing, its position and note kept,
/// whether the file is on disk or, while Kids goes, gone from it.
#[test]
fn removing_a_nested_directory_keeps_a_file_the_scanned_innermost_excludes_four_deep() {
    let mut outcomes = Vec::new();
    for (gone, whole_library) in [(false, false), (false, true), (true, false), (true, true)] {
        let (top, library, manga) = manga_holding(
            &format!("nest-scanned-four-{gone}-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/mid/Tiny/Tiny v01.cbz", 2),
                ("kids/mid/little/Small/Small v01.cbz", 2),
                ("kids/mid/little/extras/Adult/Adult v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let adult = manga
            .path
            .join("kids/mid/little/extras/Adult/Adult v01.cbz");
        let work = work_of(&conn, &adult);
        read_on(&conn, work);
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        scan_root(&conn, &kids).expect("scans Kids");
        let added = |rel: &str, name: &str, globs: &str| -> Root {
            let mut root =
                add_root_checked_in(&conn, &manga.path.join(rel), InLibrary::Named(name))
                    .unwrap_or_else(|e| panic!("adds {name}: {e:?}"))
                    .root;
            set_patterns(&conn, &mut root, Some(globs));
            root
        };
        let mid = added("kids/mid", "Mid", "little/extras/");
        let mut little = added("kids/mid/little", "Little", "extras/");
        for root in [&mid, &little] {
            let report = scan_root(&conn, root).expect("scans");
            assert!(report.refused.is_none(), "{report:?}");
        }
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (entries_for(&conn, &adult), held_by(&conn, &adult, &kids)),
            (vec![], true),
            "the fixture: Kids holds Adult's member with no row"
        );
        let away = top.path().join("away.cbz");
        if gone {
            std::fs::rename(&adult, &away).expect("takes the file away");
        }

        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        let kept = work_kept(&conn, work);
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            held_by(&conn, &adult, &little),
            kept && marked_missing(&conn, work),
        );
        if gone {
            std::fs::rename(&away, &adult).expect("puts the file back");
        }
        set_patterns(&conn, &mut little, None);
        scan_root(&conn, &little).expect("scans Little, its pattern lifted");
        crate::derive::derive(&conn).expect("derives");
        let lifted = (
            kept && work_of(&conn, &adult) == work,
            entries_for(&conn, &adult) == [little.id],
            kept && marked_missing(&conn, work),
        );
        outcomes.push((gone, whole_library, after, lifted));
    }
    let expected = |gone: bool, whole_library: bool| {
        (
            gone,
            whole_library,
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
            (true, true, false),
        )
    };
    assert_eq!(
        outcomes,
        [
            expected(false, false),
            expected(false, true),
            expected(true, false),
            expected(true, true)
        ],
        "(gone, whole library); after Kids goes: (dialog, removal, kept, position and note, \
         Little's, missing); after Little lifts its pattern: (the same work, Little's row, missing)"
    );
}

/// Three deep, Little (`extras/`) added after Kids' scan: Kids keeps Adult's
/// row until it comes to exclude `little/extras/` and Little's next scan drops
/// it, leaving Kids the member with no row, whether Kids then lifts its pattern
/// or not. Removing Kids, its directory or its library, counts nothing, and the
/// member goes to Little, the nearest directory left, whose patterns hide it:
/// Manga, which admits the file, has no walk under Little to show it with.
/// Once Little lifts its pattern and scans, the same work is Little's and
/// shown.
#[test]
fn removing_a_nested_directory_gives_a_file_with_no_row_to_the_nearest_directory_left() {
    let mut outcomes = Vec::new();
    for (lift, whole_library) in [(true, false), (true, true), (false, false), (false, true)] {
        let (_top, library, manga) = manga_holding(
            &format!("nest-no-row-{lift}-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/little/Tiny/Tiny v01.cbz", 2),
                ("kids/little/extras/Adult/Adult v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let adult = manga.path.join("kids/little/extras/Adult/Adult v01.cbz");
        let work = work_of(&conn, &adult);
        read_on(&conn, work);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        scan_root(&conn, &kids).expect("scans Kids");
        let mut little = add_root_checked_in(
            &conn,
            &manga.path.join("kids/little"),
            InLibrary::Named("Little"),
        )
        .expect("adds Little")
        .root;
        set_patterns(&conn, &mut little, Some("extras/"));
        scan_root(&conn, &little).expect("scans Little");
        assert_eq!(
            entries_for(&conn, &adult),
            [kids.id],
            "the fixture: Kids keeps the row"
        );
        set_patterns(&conn, &mut kids, Some("little/extras/"));
        scan_root(&conn, &little).expect("scans Little again");
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (entries_for(&conn, &adult), held_by(&conn, &adult, &kids)),
            (vec![], true),
            "the fixture: Kids holds Adult's member with no row"
        );
        if lift {
            set_patterns(&conn, &mut kids, None);
        }
        let reader = reader_of(&conn, "m", manga.libraries[0]);

        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        let kept = work_kept(&conn, work);
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            held_by(&conn, &adult, &little),
            kept && marked_missing(&conn, work),
            kept && reader_sees(&conn, reader, work),
        );
        set_patterns(&conn, &mut little, None);
        scan_root(&conn, &little).expect("scans Little, its pattern lifted");
        crate::derive::derive(&conn).expect("derives");
        let lifted = (
            kept && work_of(&conn, &adult) == work,
            entries_for(&conn, &adult) == [little.id],
            kept && marked_missing(&conn, work),
        );
        outcomes.push((lift, whole_library, after, lifted));
    }
    let expected = |lift: bool, whole_library: bool| {
        (
            lift,
            whole_library,
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true, false),
            (true, true, false),
        )
    };
    assert_eq!(
        outcomes,
        [
            expected(true, false),
            expected(true, true),
            expected(false, false),
            expected(false, true)
        ],
        "after Kids goes: (dialog, removal, kept, position and note, Little's, missing, \
         seen by Manga's reader); after Little lifts its pattern: (the same work, \
         Little's row, missing)"
    );
}

/// The ruling of 2026-11-10, "Every removal", where the outer's row was
/// dropped: Kids (`extras/`) leaves Manga Bonus's row until Manga comes to
/// exclude `kids/extras/` and Kids' next scan drops it; Manga then lifts the
/// pattern. Removing Manga, its directory or its library, counts nothing:
/// Bonus's work is Kids', missing, with its position and note, and once Kids
/// lifts its pattern and scans, the same work is Kids' and shown.
#[test]
fn removing_the_outer_keeps_a_file_whose_row_a_nested_scan_dropped() {
    let mut outcomes = Vec::new();
    for whole_library in [false, true] {
        let (_top, library, mut manga) = manga_holding(
            &format!("outer-row-dropped-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Bonus/Bonus v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
        let work = work_of(&conn, &bonus);
        read_on(&conn, work);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        set_patterns(&conn, &mut kids, Some("extras/"));
        scan_root(&conn, &kids).expect("scans Kids");
        assert_eq!(
            entries_for(&conn, &bonus),
            [manga.id],
            "the fixture: Manga keeps the row"
        );
        set_patterns(&conn, &mut manga, Some("kids/extras/"));
        scan_root(&conn, &kids).expect("scans Kids again");
        crate::derive::derive(&conn).expect("derives");
        set_patterns(&conn, &mut manga, None);
        assert_eq!(
            (entries_for(&conn, &bonus), held_by(&conn, &bonus, &manga)),
            (vec![], true),
            "the fixture: Manga holds Bonus's member with no row, its pattern lifted"
        );

        let (cost, removed) = remove_rehearsed(&conn, &manga, whole_library);
        let kept = work_kept(&conn, work);
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            held_by(&conn, &bonus, &kids),
            kept && marked_missing(&conn, work),
        );
        set_patterns(&conn, &mut kids, None);
        scan_root(&conn, &kids).expect("scans Kids, its pattern lifted");
        crate::derive::derive(&conn).expect("derives");
        let lifted = (
            kept && work_of(&conn, &bonus) == work,
            entries_for(&conn, &bonus) == [kids.id],
            kept && marked_missing(&conn, work),
        );
        outcomes.push((whole_library, after, lifted));
    }
    let expected = |whole_library: bool| {
        (
            whole_library,
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
            (true, true, false),
        )
    };
    assert_eq!(
        outcomes,
        [expected(false), expected(true)],
        "after Manga goes: (dialog, removal, kept, position and note, Kids', missing); \
         after Kids lifts its pattern: (the same work, Kids' row, missing)"
    );
}

/// The walk that last looked for a file under a directory added but not yet
/// scanned is the removed directory's own: here Manga's, which excluded
/// `kids/extras/` and forgot Bonus's row. Kids' patterns admit the file, but
/// Kids never looked, so removing Manga, its directory or its library, keeps
/// Bonus's work, Kids', missing until Kids' first scan shows it.
#[test]
fn removing_the_outer_keeps_a_file_it_excluded_under_a_nested_directory_not_yet_scanned() {
    for whole_library in [false, true] {
        let (_top, library, mut manga) = manga_holding(
            &format!("outer-excluded-unscanned-{whole_library}"),
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/extras/Bonus/Bonus v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let bonus = manga.path.join("kids/extras/Bonus/Bonus v01.cbz");
        let work = work_of(&conn, &bonus);
        read_on(&conn, work);
        set_patterns(&conn, &mut manga, Some("kids/extras/"));
        scan_root(&conn, &manga).expect("scans Manga");
        crate::derive::derive(&conn).expect("derives");
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        assert_eq!(
            (entries_for(&conn, &bonus), held_by(&conn, &bonus, &manga)),
            (vec![], true),
            "the fixture: Manga holds Bonus's member with no row"
        );

        let (cost, removed) = remove_rehearsed(&conn, &manga, whole_library);
        let kept = work_kept(&conn, work);
        assert_eq!(
            (
                counted(&cost),
                counted(&removed),
                kept,
                kept_on(&conn, work),
                held_by(&conn, &bonus, &kids),
                kept && marked_missing(&conn, work)
            ),
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
            "whole_library={whole_library}: (dialog, removal, kept, position and note, \
             Kids', missing)"
        );
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (work_of(&conn, &bonus), marked_missing(&conn, work)),
            (work, false),
            "whole_library={whole_library}: Kids' first scan shows the same work"
        );
    }
}

/// Every nested directory holding a path may have been the last to look for
/// its file, not only the deepest. Mid (`extras/`) took Kids' tree over while
/// Kids excluded `mid/extras/`, so its scan dropped Kids' row for Adult; Kids
/// then lifted its pattern, and Little was added at `mid/extras`, unscanned.
/// Mid's walk looked last and was kept out, so removing Kids keeps Adult's
/// work, Little's and missing, until Little's first scan shows it.
#[test]
fn removing_a_nested_directory_asks_every_directory_between_it_and_the_file() {
    let (_top, library, manga) = manga_holding(
        "nest-middle-looked-last",
        &[
            ("kids/Bleach/v01.cbz", 3),
            ("kids/mid/Tiny/Tiny v01.cbz", 2),
            ("kids/mid/extras/Adult/Adult v01.cbz", 2),
        ],
    );
    let conn = library.conn();
    let adult = manga.path.join("kids/mid/extras/Adult/Adult v01.cbz");
    let work = work_of(&conn, &adult);
    read_on(&conn, work);
    let mut kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    scan_root(&conn, &kids).expect("scans Kids");
    set_patterns(&conn, &mut kids, Some("mid/extras/"));
    let mut mid = add_root_checked_in(&conn, &manga.path.join("kids/mid"), InLibrary::Named("Mid"))
        .expect("adds Mid")
        .root;
    set_patterns(&conn, &mut mid, Some("extras/"));
    scan_root(&conn, &mid).expect("scans Mid");
    crate::derive::derive(&conn).expect("derives");
    set_patterns(&conn, &mut kids, None);
    let little = add_root_checked_in(
        &conn,
        &manga.path.join("kids/mid/extras"),
        InLibrary::Named("Little"),
    )
    .expect("adds Little")
    .root;
    assert_eq!(
        (entries_for(&conn, &adult), held_by(&conn, &adult, &kids)),
        (vec![], true),
        "the fixture: Kids holds Adult's member with no row"
    );

    let (cost, removed) = remove_rehearsed(&conn, &kids, false);
    let kept = work_kept(&conn, work);
    assert_eq!(
        (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            held_by(&conn, &adult, &little),
            kept && marked_missing(&conn, work)
        ),
        ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
        "(dialog, removal, kept, position and note, Little's, missing)"
    );
    scan_root(&conn, &little).expect("scans Little");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (work_of(&conn, &adult), marked_missing(&conn, work)),
        (work, false),
        "Little's first scan shows the same work"
    );
}

/// A member records no kind, so a removal asks a row-less member's path as a
/// file and as a directory, and reads it as vanished only where both reach
/// it. Kids took over the volume, the directory `Shelf/Book/v01` or the
/// archive `Shelf/Book/v01.cbz`; Little, added since at `Shelf`, keeps it out
/// with a pattern only one reading meets (`Book/v01/`, or `Book/*` with
/// `!Book/*/`); Kids' row went while Kids excluded it too. Removing Kids'
/// directory or library, the file on disk or gone, counts nothing: the work
/// is Little's, missing, until Little lifts the pattern and scans.
#[test]
fn removing_a_nested_directory_asks_a_row_less_member_as_a_file_and_as_a_directory() {
    for ((archive, whole_library), gone) in
        [(false, false), (false, true), (true, false), (true, true)]
            .into_iter()
            .flat_map(|case| [(case, false), (case, true)])
    {
        let top = crate::tests::tempdir(&format!(
            "nest-row-less-{}{}{}",
            if archive { "archive" } else { "dir-volume" },
            if whole_library { "-library" } else { "" },
            if gone { "-gone" } else { "" }
        ));
        let m = top.path().join("m");
        let book = m.join("kids/Shelf/Book");
        // Beside the volume, one of the other kind for Little to read, so
        // its scans are not refused as empty reads (§8.1). Little's pattern
        // keeps the volume out from the directory reading alone, or, for
        // the archive, from the file reading alone.
        let (volume, little_keeps_out, kids_keeps_out) = if archive {
            write_pages(&book.join("v02"), 2);
            write_archive(&book.join("v01.cbz"), 2);
            (
                book.join("v01.cbz"),
                "Book/*\n!Book/*/",
                "Shelf/Book/v01.cbz",
            )
        } else {
            write_archive(&book.join("v02.cbz"), 2);
            write_pages(&book.join("v01"), 2);
            (book.join("v01"), "Book/v01/", "Shelf/Book/v01/")
        };
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
            .expect("adds Manga")
            .root;
        scan_root(&conn, &manga).expect("scans Manga");
        let mut kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        let work = work_of(&conn, &volume);
        read_on(&conn, work);
        let mut little =
            add_root_checked_in(&conn, &m.join("kids/Shelf"), InLibrary::Named("Little"))
                .expect("adds Little")
                .root;
        set_patterns(&conn, &mut little, Some(little_keeps_out));
        scan_root(&conn, &little).expect("scans Little");
        set_patterns(&conn, &mut kids, Some(kids_keeps_out));
        scan_root(&conn, &little).expect("scans Little again");
        set_patterns(&conn, &mut kids, None);
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (
                entries_for(&conn, &volume),
                held_by(&conn, &volume, &kids),
                marked_missing(&conn, work)
            ),
            (vec![], true, true),
            "archive={archive}, whole_library={whole_library}: the fixture, Kids holds the \
             volume's member with no row and its work is missing"
        );

        let away = top.path().join("away");
        if gone {
            std::fs::rename(&volume, &away).expect("takes the volume away");
        }

        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        let kept = work_kept(&conn, work);
        assert_eq!(
            (
                counted(&cost),
                counted(&removed),
                kept,
                kept_on(&conn, work),
                held_by(&conn, &volume, &little),
                kept && marked_missing(&conn, work)
            ),
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
            "archive={archive}, whole_library={whole_library}, gone={gone}: (dialog, removal, \
             kept, position and note, Little's, missing)"
        );
        if gone {
            std::fs::rename(&away, &volume).expect("puts the volume back");
        }
        set_patterns(&conn, &mut little, None);
        scan_root(&conn, &little).expect("scans Little");
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (work_of(&conn, &volume), marked_missing(&conn, work)),
            (work, false),
            "archive={archive}, whole_library={whole_library}, gone={gone}: Little's scan \
             shows the same work"
        );
    }
}

/// The guard asks every directory whose tree holds the file, not only the
/// removed one: Little, left inside Kids at `Shelf`, is an unmounted share
/// while Kids and Manga read present. The previous test's row-less volume,
/// with Kids' and Little's patterns lifted and no scan since, so every walk
/// would reach it: removing Kids, its directory or its library, counts
/// nothing and keeps the work, Little's and missing, until Little is back and
/// scans. With Little there and the volume deleted, the work is removed.
#[test]
fn a_removal_reads_no_file_as_gone_under_an_unmounted_directory_left_inside_it() {
    let mut outcomes = Vec::new();
    for (unmounted, whole_library) in [(true, false), (true, true), (false, false), (false, true)] {
        let top = crate::tests::tempdir(&format!("unmounted-left-{unmounted}-{whole_library}"));
        let m = top.path().join("m");
        let shelf = m.join("kids/Shelf");
        write_archive(&shelf.join("Book/v02.cbz"), 2);
        write_pages(&shelf.join("Book/v01"), 2);
        let volume = shelf.join("Book/v01");
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
            .expect("adds Manga")
            .root;
        scan_root(&conn, &manga).expect("scans Manga");
        let mut kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        scan_root(&conn, &kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        let work = work_of(&conn, &volume);
        read_on(&conn, work);
        let mut little = add_root_checked_in(&conn, &shelf, InLibrary::Named("Little"))
            .expect("adds Little")
            .root;
        set_patterns(&conn, &mut little, Some("Book/v01/"));
        scan_root(&conn, &little).expect("scans Little");
        set_patterns(&conn, &mut kids, Some("Shelf/Book/v01/"));
        scan_root(&conn, &little).expect("scans Little again");
        set_patterns(&conn, &mut kids, None);
        crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            (entries_for(&conn, &volume), held_by(&conn, &volume, &kids)),
            (vec![], true),
            "unmounted={unmounted}, whole_library={whole_library}: the fixture, Kids holds \
             the volume's member with no row"
        );
        let lift = RootPatch {
            exclude_globs: Some(None),
            ..RootPatch::default()
        };
        assert!(update_root(&conn, little.id, &lift).expect("lifts Little's pattern"));
        little.exclude_globs = None;

        let away = top.path().join("away");
        if unmounted {
            std::fs::rename(&shelf, &away).expect("moves Little off");
            std::fs::create_dir(&shelf).expect("leaves the mount point");
        } else {
            std::fs::remove_dir_all(&volume).expect("deletes the volume");
        }
        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        let kept = work_kept(&conn, work);
        let after = (
            counted(&cost),
            counted(&removed),
            kept,
            kept_on(&conn, work),
            kept && held_by(&conn, &volume, &little),
            kept && marked_missing(&conn, work),
        );
        let back = unmounted.then(|| {
            std::fs::remove_dir_all(&shelf).expect("clears the mount point");
            std::fs::rename(&away, &shelf).expect("puts Little back");
            scan_root(&conn, &little).expect("scans Little");
            crate::derive::derive(&conn).expect("derives");
            (
                work_of(&conn, &volume) == work,
                kept && marked_missing(&conn, work),
            )
        });
        outcomes.push((unmounted, whole_library, after, back));
    }
    let kept = |whole_library| {
        (
            true,
            whole_library,
            ((0, 0, 0), (0, 0, 0), true, (1, 1), true, true),
            Some((true, false)),
        )
    };
    let gone = |whole_library| {
        (
            false,
            whole_library,
            ((1, 1, 1), (1, 1, 1), false, (0, 0), false, false),
            None,
        )
    };
    assert_eq!(
        outcomes,
        [kept(false), kept(true), gone(false), gone(true)],
        "(unmounted, whole library); after Kids goes: (dialog, removal, kept, position and \
         note, Little's, missing); once Little is back and scans: (the same work, missing)"
    );
}

/// P3's tree with nothing of Manga's own: `m` holds only Kids' `m/kids` and
/// `alias -> kids`, and Manga recorded `alias/…` before Kids existed. Manga's
/// walk meets the alias, so its rows are not what an unmounted share looks
/// like: the scan is not refused, and forgets them as gone.
#[test]
fn an_outer_holding_only_a_nested_directory_and_a_link_to_it_forgets_the_links_rows() {
    let top = crate::tests::tempdir("nest-alias-only");
    let m = top.path().join("m");
    for (rel, pages) in &NEST_FILES[..2] {
        write_archive(&m.join(rel), *pages);
    }
    std::os::unix::fs::symlink(m.join("kids"), m.join("alias")).expect("links alias to kids");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
        .expect("adds Manga")
        .root;
    scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(rows_under(&conn, &manga, "alias"), 4, "the fixture");
    let kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;

    assert_eq!(scan_root(&conn, &kids).expect("scans Kids").taken_over, 4);
    let again = scan_root(&conn, &manga).expect("scans Manga");
    assert_eq!(
        (again.refused.as_deref(), again.gone),
        (None, 4),
        "{again:?}"
    );
    assert_eq!(rows_under(&conn, &manga, "alias"), 0);
}

/// The other side of §8.1's count of what a nested directory's scan would
/// take: an empty `m/kids`, added as Kids, is not refused when Manga holds
/// rows only beside it, nor when Manga holds `kids`'s own row alone.
#[test]
fn an_empty_nested_directory_is_not_refused_for_what_the_outer_holds_beside_it() {
    for holds_kids in [false, true] {
        let (_top, library, manga) = manga_holding(
            if holds_kids {
                "nest-empty-own-row"
            } else {
                "nest-empty-beside"
            },
            &NEST_FILES[2..],
        );
        let conn = library.conn();
        let kids_dir = manga.path.join("kids");
        std::fs::create_dir(&kids_dir).expect("an empty directory");
        if holds_kids {
            scan_root(&conn, &manga).expect("scans Manga");
        }
        assert_eq!(
            rows_under(&conn, &manga, "kids"),
            i64::from(holds_kids),
            "the fixture"
        );
        let kids = add_root_checked_in(&conn, &kids_dir, InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;

        let report = scan_root(&conn, &kids).expect("scans Kids");
        assert_eq!(
            (report.refused.as_deref(), report.taken_over),
            (None, usize::from(holds_kids)),
            "holds_kids={holds_kids}: {report:?}"
        );
    }
}

/// §8.1's advice when the walk skipped a nested directory: Manga's own files
/// beside Kids' `m/kids` are gone, so its walk meets nothing, and it is
/// refused. It still shows Kids, so the advice names `kids`, says not to
/// remove it, and does not claim it is not empty: a nested share mounted while
/// its parent's is not looks the same.
#[test]
fn an_outer_refused_as_empty_beside_a_nested_directory_is_not_told_to_remove_itself() {
    let (_top, library, manga) = manga_holding("nest-refused-advice", NEST_FILES);
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    scan_root(&conn, &kids).expect("scans Kids");
    std::fs::remove_dir_all(manga.path.join("Naruto")).expect("removes Naruto");

    let report = scan_root(&conn, &manga).expect("scans Manga");
    let refused = report.refused.expect("refused as empty");
    assert!(
        refused.ends_with(
            "It still shows `kids`, a nested directory scanned on its own, so do not remove \
             it; check that its share is mounted and what became of everything else it held."
        ),
        "{refused}"
    );
    assert!(!refused.contains("remove it."), "{refused}");
}

/// Kids' `m/kids` holding what it took over from Manga's `m`: [`NEST_FILES`]
/// under `m`, a reader on Bleach v01, and Kids added, scanned and derived.
struct HandedOver {
    _top: crate::tests::TempDir,
    library: Library,
    manga: Root,
    kids: Root,
    /// Bleach v01's work and v02's.
    v01: i64,
    v02: i64,
    /// The series names before Kids was added.
    series_before: Vec<String>,
}

fn handed_over(label: &str) -> HandedOver {
    let (top, library, manga) = manga_holding(label, NEST_FILES);
    let (kids, v01, v02, series_before) = {
        let conn = library.conn();
        let series_before = series_names(&conn);
        let v01 = read_bleach_v01(&conn, &manga);
        let v02 = work_of(&conn, &manga.path.join("kids/Bleach/v02.cbz"));
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        assert_eq!(scan_root(&conn, &kids).expect("scans Kids").taken_over, 4);
        crate::derive::derive(&conn).expect("derives");
        (kids, v01, v02, series_before)
    };
    HandedOver {
        _top: top,
        library,
        manga,
        kids,
        v01,
        v02,
        series_before,
    }
}

/// What a removal counts: works, positions, notes.
fn counted(removal: &(Removal, crate::derive::DeriveReport)) -> (usize, usize, usize) {
    (removal.0.works, removal.0.positions, removal.0.notes)
}

/// The digest `root`'s row at `rel` holds.
fn digest_at(conn: &Connection, root: &Root, rel: &str) -> Option<String> {
    conn.query_row(
        "SELECT sha256 FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root.id, rel],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("{rel} has a row: {e}"))
}

/// User 1's position and note on `work`, and the work's hand-set title.
fn the_reader(conn: &Connection, work: i64) -> (String, i64, String, i64, Option<String>) {
    conn.query_row(
        "SELECT p.edition, p.slot, n.edition, n.slot, w.title_hand
           FROM progress p JOIN note n ON n.work_id = p.work_id JOIN work w ON w.id = p.work_id
          WHERE p.user_id = 1 AND p.work_id = ?1",
        [work],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )
    .expect("the position and the note are kept")
}

fn read_on_raw_slot_2() -> (String, i64, String, i64, Option<String>) {
    (
        "raw".to_string(),
        2,
        "raw".to_string(),
        2,
        Some("Bleach, by hand".to_string()),
    )
}

/// Spec §4 in reverse: removing Kids hands its files back to Manga's `m`
/// around it before anything is counted. The rehearsal and the removal count
/// nothing removed; the works, the reader's position and note and the
/// hand-set title are kept; each file's row is Manga's again, at `kids/…`, and
/// is copied with the digest and title Kids held; its members are Manga's; and the series
/// is Manga's `kids` again. Removing Kids' whole library does the same. Each row
/// keeps the reader it was counted by, so Manga's next scan counts neither again.
#[test]
fn removing_a_nested_directory_hands_its_files_to_the_directory_around_it() {
    for whole_library in [false, true] {
        let h = handed_over(if whole_library {
            "hand-back-library"
        } else {
            "hand-back"
        });
        let conn = h.library.conn();
        let bleach = ["Bleach/v01.cbz", "Bleach/v02.cbz"];
        let digests: Vec<Option<String>> = bleach
            .iter()
            .map(|rel| digest_at(&conn, &h.kids, rel))
            .collect();
        assert!(digests.iter().all(Option::is_some), "the fixture");
        conn.execute(
            "UPDATE scan_entry SET title = rel_path WHERE root_id = ?1",
            [h.kids.id],
        )
        .expect("titles Kids' rows");
        assert_eq!(
            conn.execute(
                "UPDATE scan_entry SET counted_by = ?2 WHERE root_id = ?1 AND kind = 'file'",
                rusqlite::params![h.kids.id, zip_signature()],
            ),
            Ok(2),
            "the fixture: Kids' two files, counted by this reader"
        );
        {
            // The digests as copied, before a derive could hash the files again.
            let tx = crate::Nested::open(&conn, "hand_back").expect("opens a transaction");
            assert_eq!(
                hand_back(&tx, h.kids.id, &HashSet::new(), &mut Carried::new())
                    .expect("hands back"),
                4,
                "kids, kids/Bleach and its two files"
            );
            for (rel, digest) in bleach.iter().zip(&digests) {
                assert_eq!(
                    digest_at(&tx, &h.manga, &format!("kids/{rel}")),
                    *digest,
                    "{rel}"
                );
                let title: Option<String> = tx
                    .query_row(
                        "SELECT title FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
                        rusqlite::params![h.manga.id, format!("kids/{rel}")],
                        |r| r.get(0),
                    )
                    .expect("the copied row");
                assert_eq!(title.as_deref(), Some(*rel), "{rel}");
            }
        }

        let (cost, removed) = if whole_library {
            (
                library_removal_cost(&conn, h.kids.libraries[0]),
                remove_library(&conn, h.kids.libraries[0]),
            )
        } else {
            (
                removal_cost(&conn, h.kids.id),
                remove_root(&conn, h.kids.id),
            )
        };
        assert_eq!(
            (
                counted(&cost.expect("rehearses")),
                counted(&removed.expect("removes"))
            ),
            ((0, 0, 0), (0, 0, 0)),
            "whole_library={whole_library}"
        );

        for rel in bleach {
            let file = h.manga.path.join("kids").join(rel);
            assert_eq!(entries_for(&conn, &file), [h.manga.id], "{rel}");
            let (roots, held) = members_at(&conn, &file);
            assert!(
                !roots.is_empty() && roots.iter().all(|root| *root == Some(h.manga.id)),
                "every member of {rel} is Manga's: {roots:?}"
            );
            assert_eq!(held, [h.manga.id], "{rel}'s member_root");
            assert_eq!(
                count_at(&conn, h.manga.id, &format!("kids/{rel}")).2,
                zip_signature(),
                "{rel}'s reader, whole_library={whole_library}"
            );
        }
        assert_eq!(
            (
                work_of(&conn, &h.manga.path.join("kids/Bleach/v01.cbz")),
                work_of(&conn, &h.manga.path.join("kids/Bleach/v02.cbz"))
            ),
            (h.v01, h.v02),
            "the works are kept"
        );
        assert_eq!(the_reader(&conn, h.v01), read_on_raw_slot_2());
        assert!(h.series_before.contains(&"kids".to_string()), "the fixture");
        assert_eq!(series_names(&conn), h.series_before);
        assert_eq!(
            scanned_reading(&conn, &h.manga).1,
            Some(0),
            "Manga's next scan counts nothing again, whole_library={whole_library}"
        );
    }
}

/// The row handed back carries the reading the works were written from (O320).
#[test]
fn a_hand_back_carries_an_entrys_held_reading() {
    let h = handed_over("hand-back-held");
    let conn = h.library.conn();
    let held = (
        Some("h".repeat(64)),
        Some("zip".to_string()),
        Some(7),
        Some("held title".to_string()),
    );
    assert_eq!(
        conn.execute(
            "UPDATE scan_entry
                SET held_sha256 = ?3, held_format = ?4, held_pages = ?5, held_title = ?6
              WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![h.kids.id, "Bleach/v01.cbz", held.0, held.1, held.2, held.3],
        ),
        Ok(1),
        "the fixture: Kids' row holds a reading"
    );

    let tx = crate::Nested::open(&conn, "hand_back").expect("opens a transaction");
    assert_eq!(
        hand_back(&tx, h.kids.id, &HashSet::new(), &mut Carried::new()).expect("hands back"),
        4,
        "the fixture: kids, kids/Bleach and its two files"
    );

    assert_eq!(held_reading(&tx, h.manga.id, "kids/Bleach/v01.cbz"), held);
}

/// A removal of `root`: its directory, or its library, rehearsed (what the
/// dialog counts) then done.
fn remove_rehearsed(
    conn: &Connection,
    root: &Root,
    whole_library: bool,
) -> (
    (Removal, crate::derive::DeriveReport),
    (Removal, crate::derive::DeriveReport),
) {
    if whole_library {
        (
            library_removal_cost(conn, root.libraries[0]).expect("rehearses"),
            remove_library(conn, root.libraries[0]).expect("removes the library"),
        )
    } else {
        (
            removal_cost(conn, root.id).expect("rehearses"),
            remove_root(conn, root.id).expect("removes the directory"),
        )
    }
}

/// User 1's position and note on `work`.
fn read_on(conn: &Connection, work: i64) {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [work],
    )
    .expect("a position");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'n', ?1, 'raw', 2, 'a note', 'now', 'now')",
        [work],
    )
    .expect("a note");
}

/// Whether work `work` still exists.
fn work_kept(conn: &Connection, work: i64) -> bool {
    conn.query_row("SELECT count(*) FROM work WHERE id = ?1", [work], |r| {
        r.get::<_, i64>(0)
    })
    .expect("counts the work")
        == 1
}

/// Whether every member at `path` is `root`'s, by `member.root_id` and by
/// `member_root`.
fn held_by(conn: &Connection, path: &Path, root: &Root) -> bool {
    let (roots, held) = members_at(conn, path);
    !roots.is_empty() && roots.iter().all(|id| *id == Some(root.id)) && held == [root.id]
}

/// With Manga's `m` excluding `kids/`, Manga's walk would not reach Kids'
/// files, so the hand-back copies none of their rows. Removing Kids, its
/// directory or its library, still counts nothing, since the files are on
/// disk: both Bleach works are kept, their members Manga's with no row,
/// marked missing, with the reader's position and note (the owner's rulings
/// of 2026-11-10, "Never delete on disk" and "Cover both"). Once Manga lifts
/// its pattern and scans, both are back.
#[test]
fn removing_a_nested_directory_the_outer_excludes_keeps_its_works_missing() {
    for whole_library in [false, true] {
        let mut h = handed_over(&format!("hand-back-excluded-{whole_library}"));
        let conn = h.library.conn();
        set_patterns(&conn, &mut h.manga, Some("kids/"));
        let bleach = [
            (h.manga.path.join("kids/Bleach/v01.cbz"), h.v01),
            (h.manga.path.join("kids/Bleach/v02.cbz"), h.v02),
        ];

        let (cost, removed) = remove_rehearsed(&conn, &h.kids, whole_library);
        assert_eq!(
            (counted(&cost), counted(&removed)),
            ((0, 0, 0), (0, 0, 0)),
            "whole_library={whole_library}"
        );
        assert_eq!(
            rows_under(&conn, &h.manga, "kids"),
            0,
            "no row is handed back"
        );
        for (file, work) in &bleach {
            assert_eq!(
                (
                    work_of(&conn, file),
                    held_by(&conn, file, &h.manga),
                    marked_missing(&conn, *work)
                ),
                (*work, true, true),
                "{} whole_library={whole_library}: the work kept, Manga's, missing",
                file.display()
            );
        }
        assert_eq!(the_reader(&conn, h.v01), read_on_raw_slot_2());

        set_patterns(&conn, &mut h.manga, None);
        scan_root(&conn, &h.manga).expect("scans Manga, its pattern lifted");
        crate::derive::derive(&conn).expect("derives");
        for (file, work) in &bleach {
            assert_eq!(
                (work_of(&conn, file), marked_missing(&conn, *work)),
                (*work, false),
                "{} whole_library={whole_library}: the same work, shown",
                file.display()
            );
        }
        assert_eq!(the_reader(&conn, h.v01), read_on_raw_slot_2());
    }
}

/// How many works are marked missing.
fn works_marked(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM work WHERE missing_since IS NOT NULL",
        [],
        |r| r.get(0),
    )
    .expect("counts the marked works")
}

/// A removal's answer counts the marks its own transaction made, as its dialog
/// does: removing Kids, its directory or its library, with Manga's `m`
/// excluding `kids/`, marks both Bleach works missing, and the dialog, the
/// answer and the store each say 2. Then, Manga's pattern lifted and Kids
/// added again and scanned with no derive after it (`library scan
/// --no-derive`, or a scan whose derive failed), removing Kids again hands its
/// rows to Manga and finds both works again, and each says so. Each phase
/// checks both counts, so answering one event as the other fails.
#[test]
fn a_removal_counts_the_works_its_own_transaction_marks_and_finds_again() {
    for whole_library in [false, true] {
        let mut h = handed_over(&format!("removal-counts-marks-{whole_library}"));
        let conn = h.library.conn();
        set_patterns(&conn, &mut h.manga, Some("kids/"));
        let before = works_marked(&conn);

        let (cost, removed) = remove_rehearsed(&conn, &h.kids, whole_library);
        assert_eq!(
            (
                before,
                (cost.1.marked_missing, cost.1.found_again),
                (removed.1.marked_missing, removed.1.found_again),
                works_marked(&conn)
            ),
            (0, (2, 0), (2, 0), 2),
            "whole_library={whole_library}: (marked before, the dialog's (marked, found \
             again), the answer's, marked after)"
        );

        set_patterns(&conn, &mut h.manga, None);
        let kids = add_root_checked_in(&conn, &h.manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids again")
            .root;
        scan_root(&conn, &kids).expect("scans Kids, deriving nothing");
        let (cost, removed) = remove_rehearsed(&conn, &kids, whole_library);
        assert_eq!(
            (
                (cost.1.marked_missing, cost.1.found_again),
                (removed.1.marked_missing, removed.1.found_again),
                works_marked(&conn)
            ),
            ((0, 2), (0, 2), 0),
            "whole_library={whole_library}: (the dialog's (marked, found again), the \
             answer's, marked after)"
        );
    }
}

/// Spec §4: reach is per directory, so a file-level pattern at Manga's `m`
/// naming `kids/Bleach/v02.cbz` governs nothing while Kids holds the file, and
/// Manga's page shows both Bleach volumes. Handed back, the file meets the
/// pattern: removing Kids, its directory or its library, counts nothing, hands
/// v01's row to Manga, and gives v02's member to Manga with no row, so v02's
/// work is kept, marked missing, and Manga's reader sees v01 alone (the
/// owner's rulings of 2026-11-10, "Never delete on disk" and "Cover both").
#[test]
fn a_file_the_outer_excludes_shows_until_the_hand_back_hides_it() {
    for whole_library in [false, true] {
        let mut h = handed_over(&format!("hand-back-file-glob-{whole_library}"));
        let conn = h.library.conn();
        set_patterns(&conn, &mut h.manga, Some("kids/Bleach/v02.cbz"));
        let shown = |conn: &Connection| -> i64 {
            crate::browse::series(
                conn,
                &crate::account::Access::Every,
                1,
                Some(h.manga.libraries[0]),
            )
            .expect("lists Manga's page")
            .iter()
            .find(|card| card.title == "Bleach")
            .expect("Manga's page shows Bleach")
            .works
        };
        assert_eq!(shown(&conn), 2, "v02 shows while Kids holds it");
        let reader = reader_of(&conn, "m", h.manga.libraries[0]);
        let (v01, v02) = (
            h.manga.path.join("kids/Bleach/v01.cbz"),
            h.manga.path.join("kids/Bleach/v02.cbz"),
        );

        let (cost, removed) = remove_rehearsed(&conn, &h.kids, whole_library);
        assert_eq!(
            (counted(&cost), counted(&removed)),
            ((0, 0, 0), (0, 0, 0)),
            "whole_library={whole_library}"
        );
        assert_eq!(
            (entries_for(&conn, &v01), entries_for(&conn, &v02)),
            (vec![h.manga.id], vec![]),
            "whole_library={whole_library}: v01 is handed back, v02 is not"
        );
        assert_eq!(
            (
                (work_of(&conn, &v01), marked_missing(&conn, h.v01)),
                (work_of(&conn, &v02), marked_missing(&conn, h.v02)),
                held_by(&conn, &v02, &h.manga),
                reader_sees(&conn, reader, h.v01),
                reader_sees(&conn, reader, h.v02)
            ),
            ((h.v01, false), (h.v02, true), true, true, false),
            "whole_library={whole_library}: both works kept, v02's Manga's and missing, \
             and Manga's reader sees v01 alone"
        );
    }
}

/// Three deep, `m` ⊃ `m/kids` ⊃ `m/kids/little`, Little added after Kids'
/// scan: removing Kids hands Kids' own files to Manga and Little's to Little,
/// whether Little has scanned and holds them or Kids still does. Nothing is
/// counted removed, and Manga is given nothing under `kids/little`.
#[test]
fn removing_the_middle_of_three_hands_its_files_out_and_the_innermost_keeps_its_own() {
    for little_scanned in [true, false] {
        let (_top, library, manga) = manga_holding(
            if little_scanned {
                "hand-back-three-scanned"
            } else {
                "hand-back-three"
            },
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/little/Tiny/Tiny v01.cbz", 2),
                ("Naruto/Naruto v01.cbz", 5),
            ],
        );
        let conn = library.conn();
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        scan_root(&conn, &kids).expect("scans Kids");
        let little = add_root_checked_in(
            &conn,
            &manga.path.join("kids/little"),
            InLibrary::Named("Little"),
        )
        .expect("adds Little")
        .root;
        if little_scanned {
            scan_root(&conn, &little).expect("scans Little");
        }
        crate::derive::derive(&conn).expect("derives");
        let bleach = manga.path.join("kids/Bleach/v01.cbz");
        let tiny = manga.path.join("kids/little/Tiny/Tiny v01.cbz");
        assert_eq!(
            entries_for(&conn, &tiny),
            [if little_scanned { little.id } else { kids.id }],
            "the fixture"
        );
        let works = (work_of(&conn, &bleach), work_of(&conn, &tiny));

        let cost = removal_cost(&conn, kids.id).expect("rehearses");
        let removed = remove_root(&conn, kids.id).expect("removes");
        assert_eq!(
            (counted(&cost), counted(&removed)),
            ((0, 0, 0), (0, 0, 0)),
            "little_scanned={little_scanned}"
        );
        assert_eq!(entries_for(&conn, &bleach), [manga.id]);
        assert_eq!(
            members_at(&conn, &bleach),
            (vec![Some(manga.id)], vec![manga.id])
        );
        assert_eq!(entries_for(&conn, &tiny), [little.id]);
        assert_eq!(
            members_at(&conn, &tiny),
            (vec![Some(little.id)], vec![little.id])
        );
        assert_eq!(rows_under(&conn, &manga, "kids/little"), 0);
        assert_eq!((work_of(&conn, &bleach), work_of(&conn, &tiny)), works);
    }
}

/// Set the directory at `rel` under `outer` back to the mtime `outer`'s scan
/// recorded for it. Adding a directory probes it by writing and removing a
/// file in it (`watch::probe`), which moves its mtime past the recorded one
/// whenever the probe lands in a later second, as mtimes are kept in whole
/// seconds. A test whose next scan compares that row with the disk puts the
/// mtime back, so its counts do not depend on the clock.
fn put_back_recorded_mtime(conn: &Connection, outer: &Root, rel: &str) {
    let recorded: i64 = conn
        .query_row(
            "SELECT mtime FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![outer.id, rel],
            |r| r.get(0),
        )
        .expect("the scan recorded the directory");
    let recorded = u64::try_from(recorded).expect("a recorded mtime after the epoch");
    std::fs::File::open(outer.path.join(rel))
        .expect("opens the directory")
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(recorded)),
        )
        .expect("sets its mtime");
}

/// Removing Manga after Kids is added at `m/kids` and before Kids' first
/// scan: Manga still holds Bleach, and the hand-back gives its rows and
/// members to Kids, so only Naruto, Manga's own, is counted removed. Kids'
/// first scan then finds its rows already there.
#[test]
fn removing_the_outer_before_the_nested_directorys_first_scan_hands_its_files_in() {
    let (_top, library, manga) = manga_holding("hand-back-inward", NEST_FILES);
    let conn = library.conn();
    let v01 = read_bleach_v01(&conn, &manga);
    let bleach = [
        manga.path.join("kids/Bleach/v01.cbz"),
        manga.path.join("kids/Bleach/v02.cbz"),
    ];
    let v02 = work_of(&conn, &bleach[1]);
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    put_back_recorded_mtime(&conn, &manga, "kids");

    let cost = removal_cost(&conn, manga.id).expect("rehearses");
    let removed = remove_root(&conn, manga.id).expect("removes");
    assert_eq!((counted(&cost), counted(&removed)), ((1, 0, 0), (1, 0, 0)));
    for file in &bleach {
        assert_eq!(entries_for(&conn, file), [kids.id], "{file:?}");
        let (roots, held) = members_at(&conn, file);
        assert!(
            !roots.is_empty() && roots.iter().all(|root| *root == Some(kids.id)),
            "every member of {file:?} is Kids': {roots:?}"
        );
        assert_eq!(held, [kids.id], "{file:?}'s member_root");
    }
    assert_eq!(
        (work_of(&conn, &bleach[0]), work_of(&conn, &bleach[1])),
        (v01, v02)
    );
    assert_eq!(the_reader(&conn, v01), read_on_raw_slot_2());

    let first = scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(
        (
            first.refused.as_deref(),
            first.added,
            first.updated,
            first.gone,
            first.taken_over
        ),
        (None, 0, 0, 0, 0),
        "{first:?}"
    );
}

/// `forget_unseen`'s guard alone: the skip set is the walk's, whatever the
/// `root` table says once the transaction opens. Given `kids` skipped where no
/// directory holds `m/kids` any more, the rows under it stay, and the rows the
/// walk did not meet elsewhere are gone.
#[test]
fn forget_unseen_leaves_the_rows_under_a_skipped_directory_that_is_no_longer_a_root() {
    let (_top, library, manga) = manga_holding("forget-skipped", NEST_FILES);
    let conn = library.conn();
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    remove_root(&conn, kids.id).expect("removes Kids");
    assert!(
        roots(&conn)
            .expect("reads")
            .iter()
            .all(|root| root.id != kids.id),
        "the fixture"
    );
    let held = held_for(&conn, manga.id).expect("reads what Manga holds");

    let forgotten = forget_unseen(
        &conn,
        &manga,
        &[],
        &held,
        &Excluder::build(None),
        &["kids".to_string()],
        &mut Carried::new(),
    )
    .expect("forgets");
    assert_eq!(forgotten, (3, 0), "the root's row and Naruto's two");
    assert_eq!(rows_under(&conn, &manga, "kids"), 4);
}

/// The alias tree with a file of Manga's own beside it: Manga recorded
/// `alias/…` and `Naruto/…`, Kids took Bleach over, and then Naruto goes.
/// Manga's walk meets only its own directory, the link and Kids' directory.
/// Its rows outside the link still count toward §8.1, so the scan is refused.
#[test]
fn rows_beside_a_link_to_a_nested_directory_still_count_toward_the_empty_refusal() {
    let top = crate::tests::tempdir("nest-alias-beside");
    let m = top.path().join("m");
    for (rel, pages) in NEST_FILES {
        write_archive(&m.join(rel), *pages);
    }
    std::os::unix::fs::symlink(m.join("kids"), m.join("alias")).expect("links alias to kids");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = add_root_checked_in(&conn, &m, InLibrary::Named("Manga"))
        .expect("adds Manga")
        .root;
    scan_root(&conn, &manga).expect("scans Manga");
    let kids = add_root_checked_in(&conn, &m.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    assert_eq!(scan_root(&conn, &kids).expect("scans Kids").taken_over, 4);
    std::fs::remove_dir_all(m.join("Naruto")).expect("removes Naruto");

    let report = scan_root(&conn, &manga).expect("scans Manga");
    assert!(report.refused.is_some(), "{report:?}");
    assert_eq!(rows_under(&conn, &manga, "Naruto"), 2, "nothing applied");
}

/// `m` ⊃ `m/kids` ⊃ `m/kids/little`, as many directories as the hand-back
/// chooses between: removing Little hands Tiny to Kids, the deepest directory
/// around it, and removing Manga before either inner directory has scanned
/// hands Bleach to Kids and Tiny to Little, the deepest directory nested in it
/// that holds each.
#[test]
fn a_hand_back_gives_each_file_to_the_deepest_directory_that_holds_it() {
    for removed_outermost in [false, true] {
        let (_top, library, manga) = manga_holding(
            if removed_outermost {
                "hand-back-deepest-in"
            } else {
                "hand-back-deepest-out"
            },
            &[
                ("kids/Bleach/v01.cbz", 3),
                ("kids/little/Tiny/Tiny v01.cbz", 2),
            ],
        );
        let conn = library.conn();
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        let little = add_root_checked_in(
            &conn,
            &manga.path.join("kids/little"),
            InLibrary::Named("Little"),
        )
        .expect("adds Little")
        .root;
        if !removed_outermost {
            scan_root(&conn, &kids).expect("scans Kids");
            scan_root(&conn, &little).expect("scans Little");
            crate::derive::derive(&conn).expect("derives");
        }
        let bleach = manga.path.join("kids/Bleach/v01.cbz");
        let tiny = manga.path.join("kids/little/Tiny/Tiny v01.cbz");

        let removed = if removed_outermost { &manga } else { &little };
        let counts = counted(&remove_root(&conn, removed.id).expect("removes"));
        assert_eq!(counts, (0, 0, 0), "removed_outermost={removed_outermost}");
        assert_eq!(entries_for(&conn, &bleach), [kids.id]);
        let tiny_held_by = if removed_outermost {
            little.id
        } else {
            kids.id
        };
        assert_eq!(entries_for(&conn, &tiny), [tiny_held_by]);
        assert_eq!(
            members_at(&conn, &tiny),
            (vec![Some(tiny_held_by)], vec![tiny_held_by]),
            "removed_outermost={removed_outermost}"
        );
    }
}

/// The ruling of 2026-11-10, "Every removal": removing Manga, its directory
/// or its library, before Kids' first scan, with Kids' patterns excluding
/// `Bleach/v02.cbz`. Kids' walk would never reach v02, so the hand-back gives
/// Kids no row for it, but v02 is on disk in Kids' tree: its member goes to
/// Kids with no row, and its work is kept, missing, with its position and
/// note. v01 goes to Kids, and Naruto's work alone is removed.
#[test]
fn removing_the_outer_hands_the_nested_directory_what_its_patterns_exclude_as_missing() {
    for whole_library in [false, true] {
        let (_top, library, manga) = manga_holding(
            &format!("hand-back-inward-excluded-{whole_library}"),
            NEST_FILES,
        );
        let conn = library.conn();
        let (v01_file, v02_file) = (
            manga.path.join("kids/Bleach/v01.cbz"),
            manga.path.join("kids/Bleach/v02.cbz"),
        );
        let (v01, v02) = (work_of(&conn, &v01_file), work_of(&conn, &v02_file));
        read_on(&conn, v02);
        let mut kids =
            add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
                .expect("adds Kids")
                .root;
        set_patterns(&conn, &mut kids, Some("Bleach/v02.cbz"));

        let (cost, removed) = remove_rehearsed(&conn, &manga, whole_library);
        assert_eq!(
            (
                counted(&cost),
                counted(&removed),
                entries_for(&conn, &v01_file),
                work_of(&conn, &v01_file),
                entries_for(&conn, &v02_file),
                work_of(&conn, &v02_file),
                held_by(&conn, &v02_file, &kids),
                marked_missing(&conn, v02),
                kept_on(&conn, v02)
            ),
            (
                (1, 0, 0),
                (1, 0, 0),
                vec![kids.id],
                v01,
                vec![],
                v02,
                true,
                true,
                (1, 1)
            ),
            "whole_library={whole_library}: (dialog, removal, v01's rows, its work, v02's \
             rows, its work, Kids', missing, its position and note)"
        );
    }
}

/// Manga's walk asks a directory-only pattern of directories alone, and so
/// does the hand-back. With `m` excluding `v02.cbz/` and `v03/`, removing Kids
/// hands back the file `v02.cbz` and not the volume directory `v03`, whose
/// member goes to Manga with no row: its work is kept, marked missing, and
/// nothing is counted removed (the owner's rulings of 2026-11-10, "Never
/// delete on disk" and "Cover both").
#[test]
fn a_directory_only_pattern_around_a_removed_directory_holds_back_directories_alone() {
    let h = handed_over("hand-back-dir-only");
    let conn = h.library.conn();
    let v03 = h.manga.path.join("kids/Bleach/v03");
    write_pages(&v03, 2);
    scan_root(&conn, &h.kids).expect("scans Kids again");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(entries_for(&conn, &v03), [h.kids.id], "the fixture");
    let v03_work = work_of(&conn, &v03);
    assert!(
        ![h.v01, h.v02].contains(&v03_work),
        "the fixture: v03 is a work of its own"
    );
    conn.execute(
        "UPDATE root SET exclude_globs = 'v02.cbz/' || char(10) || 'v03/' WHERE id = ?1",
        [h.manga.id],
    )
    .expect("excludes the directories v02.cbz and v03 from Manga's directory");

    let cost = removal_cost(&conn, h.kids.id).expect("rehearses");
    let removed = remove_root(&conn, h.kids.id).expect("removes");
    assert_eq!((counted(&cost), counted(&removed)), ((0, 0, 0), (0, 0, 0)));
    assert_eq!(
        entries_for(&conn, &h.manga.path.join("kids/Bleach/v02.cbz")),
        [h.manga.id],
        "a file is not a directory"
    );
    assert!(entries_for(&conn, &v03).is_empty(), "v03 is held back");
    assert_eq!(
        (
            work_of(&conn, &v03),
            held_by(&conn, &v03, &h.manga),
            marked_missing(&conn, v03_work)
        ),
        (v03_work, true, true),
        "v03's work is kept, its member Manga's, missing"
    );
}

// --- spec §4: a work's hand document moves with its files ------------------

/// Manga's `m` holding [`NEST_FILES`] and keeping documents, with the reader on
/// Bleach v01 and its hand-set title saved at `m/.taiyaku`, and Kids added at
/// `m/kids` before its first scan, keeping documents where `kids_keeps` says.
struct Documented {
    _top: crate::tests::TempDir,
    library: Library,
    manga: Root,
    kids: Root,
    v01: i64,
    /// Bleach v01's document under Manga, and the same name under Kids.
    at_manga: PathBuf,
    at_kids: PathBuf,
}

fn documented(label: &str, kids_keeps: bool) -> Documented {
    let (top, library, manga) = manga_holding(label, NEST_FILES);
    let (kids, v01, at_manga) = {
        let conn = library.conn();
        keep_documents(&conn, &manga);
        let v01 = read_bleach_v01(&conn, &manga);
        let saved = crate::hand::save(&conn, v01).expect("saves the title");
        let at_manga = saved.path.expect("the fixture: an address");
        assert!(
            saved.persisted && at_manga.is_file() && at_manga.starts_with(&manga.path),
            "the fixture: the document is Manga's: {at_manga:?}"
        );
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        if kids_keeps {
            keep_documents(&conn, &kids);
        }
        (kids, v01, at_manga)
    };
    let at_kids = kids
        .path
        .join(".taiyaku")
        .join(at_manga.file_name().expect("a file name"));
    Documented {
        _top: top,
        library,
        manga,
        kids,
        v01,
        at_manga,
        at_kids,
    }
}

fn keep_documents(conn: &Connection, root: &Root) {
    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("keeps documents");
}

/// The names of the files in `dir`, none where there is no `dir`.
fn files_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .map(|entry| {
                    entry
                        .expect("lists an entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        },
    )
}

/// Spec §4: a work's hand document moves with its files. Kids, keeping
/// documents, takes Bleach v01 over, and the document moves from `m/.taiyaku`
/// to `m/kids/.taiyaku`; removing Kids, or Kids' library, hands it back. Neither
/// derive finds an orphan or writes a document afresh.
#[test]
fn a_hand_document_moves_into_a_nested_directory_and_back_with_its_work() {
    for whole_library in [false, true] {
        let d = documented(
            if whole_library {
                "documents-library"
            } else {
                "documents"
            },
            true,
        );
        let conn = d.library.conn();

        scan_root(&conn, &d.kids).expect("scans Kids");
        let derived = crate::derive::derive(&conn).expect("derives");
        assert_eq!(
            crate::hand::path_for(&conn, d.v01).expect("addresses"),
            Some(d.at_kids.clone()),
            "the fixture: Kids' address, under the same key"
        );
        assert_eq!(
            (d.at_kids.is_file(), d.at_manga.exists()),
            (true, false),
            "whole_library={whole_library}: the document is Kids'"
        );
        assert!(
            std::fs::read_to_string(&d.at_kids)
                .expect("reads it")
                .contains("Bleach, by hand"),
            "and it is the one Manga kept"
        );
        assert_eq!(
            (derived.orphan_documents, derived.repaired_documents),
            (Vec::new(), 0),
            "no orphan, and nothing written afresh"
        );

        let (_, derived) = if whole_library {
            remove_library(&conn, d.kids.libraries[0])
        } else {
            remove_root(&conn, d.kids.id)
        }
        .expect("removes Kids");
        assert_eq!(
            crate::hand::path_for(&conn, d.v01).expect("addresses"),
            Some(d.at_manga.clone()),
            "the fixture: Manga's address again"
        );
        assert!(d.at_manga.is_file(), "handed back");
        assert_eq!(
            files_in(&d.kids.path.join(".taiyaku")),
            Vec::<String>::new(),
            "nothing is left under m/kids/.taiyaku"
        );
        assert_eq!(
            (derived.orphan_documents, derived.repaired_documents),
            (Vec::new(), 0),
            "no orphan, and nothing written afresh"
        );
    }
}

/// Decision 21: the scan at which Kids comes to exclude Bleach v01 hands the
/// file back to Manga with its hand document, as a removal does, and the
/// derive finds no orphan and writes nothing afresh.
#[test]
fn a_hand_document_goes_back_with_a_file_the_nested_directory_comes_to_exclude() {
    let d = documented("documents-later-excluded", true);
    let conn = d.library.conn();
    scan_root(&conn, &d.kids).expect("scans Kids");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (d.at_kids.is_file(), d.at_manga.exists()),
        (true, false),
        "the fixture: the document is Kids'"
    );
    conn.execute(
        "UPDATE root SET exclude_globs = 'Bleach/v01.cbz' WHERE id = ?1",
        [d.kids.id],
    )
    .expect("Kids comes to exclude v01");
    let kids = Root {
        exclude_globs: Some("Bleach/v01.cbz".to_string()),
        ..d.kids
    };

    let report = scan_root(&conn, &kids).expect("scans Kids again");
    let derived = crate::derive::derive(&conn).expect("derives");

    assert_eq!(report.forgotten_by_exclusion, 1, "v01: {report:?}");
    assert_eq!(
        (
            crate::hand::path_for(&conn, d.v01).expect("addresses"),
            d.at_manga.is_file(),
            d.at_kids.exists()
        ),
        (Some(d.at_manga.clone()), true, false),
        "the document is Manga's again"
    );
    assert!(
        std::fs::read_to_string(&d.at_manga)
            .expect("reads it")
            .contains("Bleach, by hand"),
        "and it is the one Kids kept"
    );
    assert_eq!(
        (derived.orphan_documents, derived.repaired_documents),
        (Vec::new(), 0),
        "no orphan, and nothing written afresh"
    );
}

/// Decision 21 across three levels: Little, at `m/kids/Bleach` and excluding
/// `v01.cbz`, is added before Kids' first scan, so Manga still holds Bleach
/// v01's rows at Little's first scan. That scan moves them to Kids, whose walk
/// reaches v01, with v01's member and its hand document, and the derive finds
/// no orphan and writes nothing afresh.
#[test]
fn a_hand_document_goes_with_a_file_moved_to_the_directory_around_the_nested_one() {
    let d = documented("documents-moved-to-the-middle", true);
    let conn = d.library.conn();
    let mut little = add_root_checked_in(
        &conn,
        &d.manga.path.join("kids/Bleach"),
        InLibrary::Named("Little"),
    )
    .expect("adds Little")
    .root;
    set_patterns(&conn, &mut little, Some("v01.cbz"));
    scan_root(&conn, &d.kids).expect("scans Kids");
    assert_eq!(
        (d.at_manga.is_file(), d.at_kids.exists()),
        (true, false),
        "the fixture: Kids' scan leaves Bleach to Little, so the document is Manga's"
    );

    scan_root(&conn, &little).expect("scans Little");
    let derived = crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        (
            entries_for(&conn, &d.manga.path.join("kids/Bleach/v01.cbz")),
            crate::hand::path_for(&conn, d.v01).expect("addresses"),
            d.at_kids.is_file(),
            d.at_manga.exists()
        ),
        (vec![d.kids.id], Some(d.at_kids.clone()), true, false),
        "v01's row and document are Kids'"
    );
    assert_eq!(
        (derived.orphan_documents, derived.repaired_documents),
        (Vec::new(), 0),
        "no orphan, and nothing written afresh"
    );
}

/// A nested directory that keeps no documents is given none: Kids takes
/// Bleach v01 over and its document stays at `m/.taiyaku`, an orphan reported
/// with its title, while the title is kept in the store.
#[test]
fn a_nested_directory_that_keeps_no_documents_leaves_the_document_where_it_was() {
    let d = documented("documents-declined", false);
    let conn = d.library.conn();

    scan_root(&conn, &d.kids).expect("scans Kids");
    let derived = crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        crate::hand::path_for(&conn, d.v01).expect("addresses"),
        Some(d.at_kids.clone()),
        "the fixture: Kids' address"
    );
    assert!(d.at_manga.is_file(), "the document stays");
    assert!(
        !d.kids.path.join(".taiyaku").exists(),
        "and Kids is given no .taiyaku"
    );
    assert_eq!(
        derived.orphan_documents,
        [crate::hand::Orphan {
            path: d.at_manga.clone(),
            title: Some("Bleach, by hand".to_string()),
            absorbed_into: None,
        }],
        "reported as an orphan with its title"
    );
    let title: Option<String> = conn
        .query_row("SELECT title_hand FROM work WHERE id = ?1", [d.v01], |r| {
            r.get(0)
        })
        .expect("reads the title");
    assert_eq!(
        title.as_deref(),
        Some("Bleach, by hand"),
        "kept in the store"
    );
}

/// An occupied address is never overwritten: with a document already at
/// Kids' address, Kids' scan leaves both where they are, and the derive
/// reports the one at `m/.taiyaku` as an orphan and the one at Kids' as stale.
#[test]
fn a_hand_over_never_overwrites_a_document_at_the_nested_address() {
    let d = documented("documents-occupied", true);
    let conn = d.library.conn();
    std::fs::create_dir_all(d.at_kids.parent().expect("a directory")).expect("makes .taiyaku");
    std::fs::write(
        &d.at_kids,
        br#"{"taiyaku_library":1,"updated":"x","title":"Squatter"}"#,
    )
    .expect("a document at Kids' address");

    scan_root(&conn, &d.kids).expect("scans Kids");
    let derived = crate::derive::derive(&conn).expect("derives");

    let read = |path: &Path| std::fs::read_to_string(path).expect("reads it");
    assert!(read(&d.at_kids).contains("Squatter"), "not overwritten");
    assert!(
        read(&d.at_manga).contains("Bleach, by hand"),
        "and not moved"
    );
    assert_eq!(
        derived
            .orphan_documents
            .iter()
            .map(|orphan| orphan.path.clone())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&d.at_manga),
        "Manga's is an orphan"
    );
    assert_eq!(
        derived.stale_documents,
        [d.at_kids.display().to_string()],
        "Kids' disagrees with the store"
    );
}

/// `root` removed, alone or, where `whole_library`, with its one library: the
/// derive report the removal answers.
fn removed(conn: &Connection, root: &Root, whole_library: bool) -> crate::derive::DeriveReport {
    if whole_library {
        remove_library(conn, root.libraries[0])
    } else {
        remove_root(conn, root.id)
    }
    .unwrap_or_else(|e| panic!("removes {}: {e}", root.path.display()))
    .1
}

/// A hand-back into a directory that keeps no documents leaves the document in
/// `m/kids/.taiyaku`, which no root's reconcile reads once Kids is gone, so the
/// removal lists it as an orphan with its title. Bleach v02 is handed back too
/// and has no document, so nothing names it.
#[test]
fn a_hand_back_into_a_directory_that_keeps_no_documents_reports_the_document_it_leaves() {
    for whole_library in [false, true] {
        let d = documented(&format!("documents-left-{}", u8::from(whole_library)), true);
        let conn = d.library.conn();
        scan_root(&conn, &d.kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        assert!(d.at_kids.is_file(), "the fixture: the document is Kids'");
        conn.execute(
            "UPDATE root SET keeps_documents = 0 WHERE id = ?1",
            [d.manga.id],
        )
        .expect("Manga keeps none");

        let derived = removed(&conn, &d.kids, whole_library);

        assert_eq!(
            (d.at_kids.is_file(), d.at_manga.exists()),
            (true, false),
            "whole_library={whole_library}: the document stays in m/kids/.taiyaku"
        );
        assert_eq!(
            derived.orphan_documents,
            [crate::hand::Orphan {
                path: d.at_kids.clone(),
                title: Some("Bleach, by hand".to_string()),
                absorbed_into: None,
            }],
            "whole_library={whole_library}: listed with its title"
        );
    }
}

/// A hand-back whose directory cannot be made leaves the document as one that
/// keeps none does: a plain file stands where `m/.taiyaku` must go. The
/// removal lists the document it left as an orphan, apart from the address
/// its derive could not write.
#[test]
fn a_hand_back_that_cannot_make_the_outer_taiyaku_reports_the_document() {
    for whole_library in [false, true] {
        let d = documented(
            &format!("documents-blocked-{}", u8::from(whole_library)),
            true,
        );
        let conn = d.library.conn();
        scan_root(&conn, &d.kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        let outer = d.manga.path.join(".taiyaku");
        assert_eq!(
            (d.at_kids.is_file(), files_in(&outer)),
            (true, Vec::<String>::new()),
            "the fixture: Kids' scan carried the document out of m/.taiyaku"
        );
        // A file where the directory must go, not `set_readonly`: CI runs as
        // root and ignores the write bit.
        std::fs::remove_dir(&outer).expect("removes the empty m/.taiyaku");
        std::fs::write(&outer, b"not a directory").expect("a file where m/.taiyaku must go");

        let derived = removed(&conn, &d.kids, whole_library);

        assert!(
            d.at_kids.is_file(),
            "whole_library={whole_library}: the document stays in m/kids/.taiyaku"
        );
        assert_eq!(
            derived.orphan_documents,
            [crate::hand::Orphan {
                path: d.at_kids.clone(),
                title: Some("Bleach, by hand".to_string()),
                absorbed_into: None,
            }],
            "whole_library={whole_library}: listed with its title"
        );
        assert_eq!(
            derived.unwritable_documents,
            [format!("{}: File exists (os error 17)", outer.display())],
            "whole_library={whole_library}: the address the derive could not write"
        );
    }
}

/// A hand-back never overwrites: with a document already at Manga's address,
/// the one in `m/kids/.taiyaku` stays, and the removal lists it as an orphan,
/// while the one at the address is left as it is.
#[test]
fn a_hand_back_never_overwrites_and_reports_the_document_it_leaves() {
    for whole_library in [false, true] {
        let d = documented(
            &format!("documents-back-occupied-{}", u8::from(whole_library)),
            true,
        );
        let conn = d.library.conn();
        scan_root(&conn, &d.kids).expect("scans Kids");
        crate::derive::derive(&conn).expect("derives");
        assert!(d.at_kids.is_file(), "the fixture: the document is Kids'");
        std::fs::write(
            &d.at_manga,
            br#"{"taiyaku_library":1,"updated":"x","title":"Squatter"}"#,
        )
        .expect("a document at Manga's address");

        let derived = removed(&conn, &d.kids, whole_library);

        assert!(
            std::fs::read_to_string(&d.at_manga)
                .expect("reads it")
                .contains("Squatter"),
            "whole_library={whole_library}: not overwritten"
        );
        assert!(
            d.at_kids.is_file(),
            "whole_library={whole_library}: and not moved"
        );
        assert_eq!(
            derived.orphan_documents,
            [crate::hand::Orphan {
                path: d.at_kids.clone(),
                title: Some("Bleach, by hand".to_string()),
                absorbed_into: None,
            }],
            "whole_library={whole_library}: listed with its title"
        );
    }
}

/// The inward hand-back: removing Manga before Kids' first scan hands Bleach to
/// Kids, which keeps no documents, so v01's document stays in `m/.taiyaku`,
/// no root's once Manga is gone, and the removal lists it.
#[test]
fn removing_the_outer_reports_the_document_a_nested_directory_that_keeps_none_did_not_take() {
    for whole_library in [false, true] {
        let (_top, library, manga) = manga_holding(
            &format!("documents-inward-{}", u8::from(whole_library)),
            NEST_FILES,
        );
        let conn = library.conn();
        keep_documents(&conn, &manga);
        let v01 = read_bleach_v01(&conn, &manga);
        let at_manga = crate::hand::save(&conn, v01)
            .expect("saves the title")
            .path
            .expect("the fixture: an address");
        assert!(at_manga.is_file(), "the fixture: the document is Manga's");
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;

        let derived = removed(&conn, &manga, whole_library);

        assert_eq!(
            crate::hand::path_for(&conn, v01).expect("addresses"),
            Some(
                kids.path
                    .join(".taiyaku")
                    .join(at_manga.file_name().expect("a file name"))
            ),
            "the fixture: Kids' address"
        );
        assert!(
            at_manga.is_file(),
            "whole_library={whole_library}: the document stays"
        );
        assert!(
            !kids.path.join(".taiyaku").exists(),
            "whole_library={whole_library}: and Kids is given no .taiyaku"
        );
        assert_eq!(
            derived.orphan_documents,
            [crate::hand::Orphan {
                path: at_manga.clone(),
                title: Some("Bleach, by hand".to_string()),
                absorbed_into: None,
            }],
            "whole_library={whole_library}: listed with its title"
        );
    }
}

/// The straddle: Bleach v01's c001 under `m/kids` and c002 under `m`, one work.
/// Kids names its editions `en`, so its scan takes c001 out of the hub edition
/// and the derive rekeys the work under Manga; removing Kids, or its library,
/// hands c001 back, and its derive rekeys it again. The document ends at each
/// new key's address under Manga, none is left in `m/kids/.taiyaku`, and no
/// derive finds an orphan or writes one afresh. Run with c002 at `m`'s top,
/// where the hub edition starts, and in `m/other`, where it starts at c001.
#[test]
fn a_hand_back_that_rekeys_its_work_carries_the_document_to_the_new_key() {
    straddle("documents-straddle", false);
}

/// The straddle with Kids keeping documents. Where the hub edition starts at
/// c001, Kids' scan carries the document into `m/kids/.taiyaku`, and the
/// derive's rekey, readdressing the work under Manga, carries it back out from
/// there in one rename.
#[test]
fn a_straddle_into_a_nested_directory_that_keeps_documents_carries_the_document_out_again() {
    straddle("documents-straddle-kept", true);
}

fn straddle(label: &str, kids_keeps: bool) {
    // `other/` sorts after `kids/`, and a file at `m`'s top before it.
    for (outer, nested_first) in [
        ("Bleach v01 - c002.cbz", false),
        ("other/Bleach v01 - c002.cbz", true),
    ] {
        for whole_library in [false, true] {
            straddle_once(label, kids_keeps, outer, nested_first, whole_library);
        }
    }
}

fn straddle_once(
    label: &str,
    kids_keeps: bool,
    outer: &str,
    nested_first: bool,
    whole_library: bool,
) {
    let (_top, library, manga) = manga_holding(
        &format!(
            "{label}-{}-{}",
            u8::from(nested_first),
            u8::from(whole_library)
        ),
        &[("kids/Bleach v01 - c001.cbz", 3), (outer, 4)],
    );
    let conn = library.conn();
    keep_documents(&conn, &manga);
    let work = work_of(&conn, &manga.path.join("kids/Bleach v01 - c001.cbz"));
    assert_eq!(
        work_of(&conn, &manga.path.join(outer)),
        work,
        "the fixture: one work"
    );
    let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
    doc.title = Some("Bleach v01, by hand".to_string());
    crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
    let first = crate::hand::save(&conn, work)
        .expect("saves the title")
        .path
        .expect("the fixture: an address");
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    conn.execute(
        "UPDATE root SET edition_key = 'en' WHERE id = ?1",
        [kids.id],
    )
    .expect("Kids names its editions en");
    if kids_keeps {
        keep_documents(&conn, &kids);
    }
    let kids = Root {
        edition_key: Some("en".to_string()),
        ..kids
    };
    let in_kids = kids.path.join(".taiyaku");
    let ends = |derived: crate::derive::DeriveReport| {
        let at = crate::hand::path_for(&conn, work)
            .expect("addresses")
            .expect("an address");
        assert_eq!(
            (at.starts_with(manga.path.join(".taiyaku")), at.is_file()),
            (true, true),
            "{outer}, whole_library={whole_library}: at the new key's address under Manga: \
                 {at:?}"
        );
        assert!(
            std::fs::read_to_string(&at)
                .expect("reads it")
                .contains("Bleach v01, by hand"),
            "the document Manga kept"
        );
        assert_eq!(
            files_in(&in_kids),
            Vec::<String>::new(),
            "nothing is left under m/kids/.taiyaku"
        );
        assert_eq!(
            (derived.orphan_documents, derived.repaired_documents),
            (Vec::new(), 0),
            "no orphan, and nothing written afresh"
        );
        at
    };

    scan_root(&conn, &kids).expect("scans Kids");
    assert_eq!(
        in_kids
            .join(first.file_name().expect("a file name"))
            .is_file(),
        kids_keeps && nested_first,
        "the fixture: the scan carries the document into Kids where Kids keeps documents \
             and the work is addressed there"
    );
    let derived = crate::derive::derive(&conn).expect("derives");
    assert_eq!(derived.rekeyed, 1, "the fixture: the hand-over rekeys");
    let taken = ends(derived);
    assert!(!first.exists(), "the hand-over's key replaced the first");

    let (_, derived) = if whole_library {
        remove_library(&conn, kids.libraries[0])
    } else {
        remove_root(&conn, kids.id)
    }
    .expect("removes Kids");
    let back = ends(derived);
    assert_ne!(back, taken, "the fixture: the hand-back rekeys");
    assert!(
        !taken.exists(),
        "the hand-back's key replaced the hand-over's"
    );
}

/// Spec §4 end to end, for `note_addresses`' first-read rule: Kids holds
/// Bleach v01 in two directories, c001 in `m/ka` and c002 in `m/kb`, both
/// inside Manga's `m`, and keeps documents, so its scans carry the document
/// into `m/ka/.taiyaku`. Removing Kids' library hands `ka` back, then `kb`.
/// The document is carried from `m/ka`, where it is, not from the address the
/// work has between the two hand-backs.
#[test]
fn a_library_removal_handing_one_work_back_from_two_directories_carries_its_document() {
    let (_top, library, manga) = manga_holding(
        "documents-two-dirs",
        &[
            ("ka/Bleach v01 - c001.cbz", 3),
            ("kb/Bleach v01 - c002.cbz", 4),
        ],
    );
    let conn = library.conn();
    keep_documents(&conn, &manga);
    let work = work_of(&conn, &manga.path.join("ka/Bleach v01 - c001.cbz"));
    assert_eq!(
        work_of(&conn, &manga.path.join("kb/Bleach v01 - c002.cbz")),
        work,
        "the fixture: one work"
    );
    let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
    doc.title = Some("Bleach v01, by hand".to_string());
    crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
    let at_manga = crate::hand::save(&conn, work)
        .expect("saves the title")
        .path
        .expect("the fixture: an address");
    let name = at_manga.file_name().expect("a file name");
    let [ka, kb] = ["ka", "kb"].map(|dir| {
        let root = add_root_checked_in(&conn, &manga.path.join(dir), InLibrary::Named("Kids"))
            .expect("adds a directory to Kids")
            .root;
        keep_documents(&conn, &root);
        root
    });
    scan_root(&conn, &ka).expect("scans ka");
    scan_root(&conn, &kb).expect("scans kb");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (
            work_of(&conn, &manga.path.join("ka/Bleach v01 - c001.cbz")),
            work_of(&conn, &manga.path.join("kb/Bleach v01 - c002.cbz")),
        ),
        (work, work),
        "the fixture: still one work, under Kids"
    );
    assert_eq!(
        crate::hand::path_for(&conn, work).expect("addresses"),
        Some(ka.path.join(".taiyaku").join(name)),
        "the fixture: addressed in ka, under the same key"
    );
    assert!(
        ka.path.join(".taiyaku").join(name).is_file(),
        "the fixture: the document is in ka"
    );
    assert!(ka.id < kb.id, "the fixture: ka is handed back first");

    let (_, derived) = remove_library(&conn, ka.libraries[0]).expect("removes Kids");

    assert_eq!(
        crate::hand::path_for(&conn, work).expect("addresses"),
        Some(at_manga.clone()),
        "the fixture: Manga's address again"
    );
    assert!(at_manga.is_file(), "handed back from ka");
    for dir in [&ka, &kb] {
        assert_eq!(
            files_in(&dir.path.join(".taiyaku")),
            Vec::<String>::new(),
            "nothing is left in {}",
            dir.path.display()
        );
    }
    assert_eq!(
        (derived.orphan_documents, derived.repaired_documents),
        (Vec::new(), 0),
        "no orphan, and nothing written afresh"
    );
}

/// A library removal hands back one directory after another, so a work met
/// again keeps the address first read for it: read again, it names where the
/// document now belongs, not where it is.
#[test]
fn a_work_noted_again_keeps_the_address_first_read() {
    let d = documented("documents-noted-again", true);
    let conn = d.library.conn();
    let mut carried = Carried::new();
    note_addresses(&conn, [d.v01], &mut carried).expect("notes the work");
    conn.execute(
        "UPDATE work SET content_key = 'moved' WHERE id = ?1",
        [d.v01],
    )
    .expect("moves the address");

    note_addresses(&conn, [d.v01], &mut carried).expect("notes it again");

    assert_eq!(carried, Carried::from([(d.v01, d.at_manga.clone())]));
}

/// One rename carries a hand document out of a nested directory and onto a
/// new key: Bleach v01's c001 and c002 are both under `m/kids`, and Kids,
/// keeping documents, has a rule naming c002's edition `en`. Its scan carries
/// the document into `m/kids/.taiyaku`, and the derive rekeys the work there.
/// Removing Kids, or its library, hands both back, and the removal's own
/// derive rekeys the work again, so the document goes from `m/kids/.taiyaku`
/// under one key to `m/.taiyaku` under another.
#[test]
fn a_hand_back_that_rekeys_a_work_wholly_inside_carries_its_directory_and_key_in_one_rename() {
    for whole_library in [false, true] {
        let (_top, library, manga) = manga_holding(
            &format!("documents-inside-{}", u8::from(whole_library)),
            &[
                ("kids/Bleach v01 - c001.cbz", 3),
                ("kids/Bleach v01 - c002.cbz", 4),
            ],
        );
        let conn = library.conn();
        keep_documents(&conn, &manga);
        let work = work_of(&conn, &manga.path.join("kids/Bleach v01 - c001.cbz"));
        assert_eq!(
            work_of(&conn, &manga.path.join("kids/Bleach v01 - c002.cbz")),
            work,
            "the fixture: one work"
        );
        let mut doc = crate::librarydoc::of(&conn, work).expect("projects the work");
        doc.title = Some("Bleach v01, by hand".to_string());
        crate::librarydoc::apply(&conn, work, &doc).expect("sets the title by hand");
        crate::hand::save(&conn, work).expect("saves the title");
        let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
            .expect("adds Kids")
            .root;
        keep_documents(&conn, &kids);
        crate::rules::create(
            &conn,
            &crate::rules::NewRule {
                ordinal: None,
                enabled: true,
                root_id: Some(kids.id),
                kind: crate::rules::RuleKind::Prefix,
                pattern: "Bleach v01 - c002".to_string(),
                match_on: crate::rules::MatchOn::Path,
                set_edition_key: Some("en".to_string()),
                set_kind: None,
                set_language: None,
                set_series: None,
                note: None,
            },
        )
        .expect("Kids names c002's edition en");
        scan_root(&conn, &kids).expect("scans Kids");
        let derived = crate::derive::derive(&conn).expect("derives");
        assert_eq!(derived.rekeyed, 1, "the fixture: the hand-over rekeys");
        let inside = crate::hand::path_for(&conn, work)
            .expect("addresses")
            .expect("an address");
        assert_eq!(
            (
                inside.starts_with(kids.path.join(".taiyaku")),
                inside.is_file()
            ),
            (true, true),
            "the fixture: the document is in m/kids/.taiyaku: {inside:?}"
        );

        let (_, derived) = if whole_library {
            remove_library(&conn, kids.libraries[0])
        } else {
            remove_root(&conn, kids.id)
        }
        .expect("removes Kids");

        let back = crate::hand::path_for(&conn, work)
            .expect("addresses")
            .expect("an address");
        assert_ne!(
            back.file_name(),
            inside.file_name(),
            "the fixture: the hand-back rekeys"
        );
        assert_eq!(
            (
                back.starts_with(manga.path.join(".taiyaku")),
                back.is_file()
            ),
            (true, true),
            "whole_library={whole_library}: at the new key's address under Manga: {back:?}"
        );
        assert!(
            std::fs::read_to_string(&back)
                .expect("reads it")
                .contains("Bleach v01, by hand"),
            "the document Kids kept"
        );
        assert_eq!(
            files_in(&kids.path.join(".taiyaku")),
            Vec::<String>::new(),
            "nothing is left under m/kids/.taiyaku"
        );
        assert_eq!(
            (derived.orphan_documents, derived.repaired_documents),
            (Vec::new(), 0),
            "no orphan, and nothing written afresh"
        );
    }
}

/// The directories with no `library_root` link, which the store cannot refuse
/// at the insert and no writer may leave.
fn directories_in_no_library(conn: &Connection) -> Vec<i64> {
    conn.prepare(
        "SELECT id FROM root r
          WHERE NOT EXISTS (SELECT 1 FROM library_root lr WHERE lr.root_id = r.id)
          ORDER BY id",
    )
    .expect("prepares")
    .query_map([], |r| r.get(0))
    .expect("reads")
    .collect::<rusqlite::Result<_>>()
    .expect("reads")
}

/// **Every public writer leaves every directory in a library**: the adds,
/// checked and unchecked, by name and of its own, a new library, a move to a
/// library and to a new one, and each removal. Migration 19 can refuse losing
/// a directory's last link, but not a directory committed with none, so each
/// writer is asked here.
#[test]
fn every_writer_leaves_every_directory_in_a_library() {
    let parent = crate::tests::tempdir("scan-every-dir-linked");
    let dir = |name: &str| -> PathBuf {
        let at = parent.path().join(name);
        std::fs::create_dir_all(&at).expect("mkdir");
        at
    };
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let unlinked = |step: &str| {
        assert_eq!(
            directories_in_no_library(&conn),
            Vec::<i64>::new(),
            "after {step}"
        );
        assert!(
            !roots(&conn)
                .expect("lists")
                .iter()
                .any(|root| root.libraries.is_empty()),
            "after {step}, roots reads a library for each"
        );
    };

    let a = add_root_in(&conn, &dir("a"), InLibrary::Named("A")).expect("adds a");
    unlinked("add_root_in, named");
    let own = add_root(&conn, &dir("own")).expect("adds own");
    unlinked("add_root, of its own");
    let c = add_root_checked_in(&conn, &dir("c"), InLibrary::Named("C")).expect("adds c");
    unlinked("add_root_checked_in, named");
    let checked_own = add_root_checked(&conn, &dir("checked-own")).expect("adds checked-own");
    unlinked("add_root_checked, of its own");
    let d = create_library(&conn, "D", &[dir("d1"), dir("d2")], None).expect("makes D");
    unlinked("create_library");
    add_root_checked_in(&conn, &a.path, InLibrary::Id(c.root.libraries[0])).expect("links a");
    unlinked("add_root_checked_in, linking");
    unlink(&conn, a.id, c.root.libraries[0], false).expect("unlinks a from C");
    unlinked("unlink");

    crate::relocate::move_root(
        &conn,
        own.id,
        own.libraries[0],
        crate::relocate::Target::Library(a.libraries[0]),
    )
    .expect("moves own into A");
    unlinked("move_root to a library");
    crate::relocate::move_root(
        &conn,
        d.dirs[0].root.id,
        d.dirs[0].root.libraries[0],
        crate::relocate::Target::New("E"),
    )
    .expect("moves d1 into a new library");
    unlinked("move_root to a new library");

    remove_root(&conn, c.root.id).expect("removes c");
    unlinked("remove_root");
    remove_library(&conn, d.library_id).expect("removes D");
    unlinked("remove_library");

    assert_eq!(
        roots(&conn)
            .expect("lists")
            .iter()
            .map(|root| root.id)
            .collect::<Vec<_>>(),
        vec![a.id, own.id, checked_own.root.id, d.dirs[0].root.id],
        "the fixture: four directories are left, each asked above"
    );
}

/// A directory `name` under `parent` holding one scanned volume of `series`,
/// added to `library` (made where there is none), and derived.
fn a_scanned_dir_in(
    conn: &Connection,
    parent: &Path,
    name: &str,
    series: &str,
    library: &str,
) -> Root {
    let dir = parent.join(name);
    write_archive(&dir.join(series).join(format!("{series} - v01.cbz")), 2);
    let root = add_root_checked_in(conn, &dir, InLibrary::Named(library))
        .expect("adds the directory")
        .root;
    scan_root(conn, &root).expect("scans it");
    crate::derive::derive(conn).expect("derives");
    root
}

/// O248's three levels under `parent`: Manga holds `m` and excludes
/// `kids/tiny`, Kids holds `m/kids` and Tiny `m/kids/tiny`, a scanned volume
/// in each. Answers the three directories, outermost first.
///
/// **Manga reaches `m/kids` and not `m/kids/tiny`**, so an account that keeps
/// `m/kids` through Manga can still gain or lose the directory inside it.
/// Without the pattern, a list that asks about `m/kids` alone would be right.
pub(crate) fn three_nested_levels(conn: &Connection, parent: &Path) -> [Root; 3] {
    let m = a_scanned_dir_in(conn, parent, "m", "Manga Book", "Manga");
    update_root(
        conn,
        m.id,
        &RootPatch {
            exclude_globs: Some(Some("kids/tiny".to_string())),
            ..Default::default()
        },
    )
    .expect("Manga keeps out kids/tiny");
    let kids = a_scanned_dir_in(conn, &m.path, "kids", "Kids Book", "Kids");
    let tiny = a_scanned_dir_in(conn, &kids.path, "tiny", "Tiny Book", "Tiny");
    assert_eq!(
        crate::reach::of_libraries(&roots(conn).expect("lists"), &m.libraries),
        vec![m.id, kids.id],
        "the fixture: Manga reaches m/kids and not m/kids/tiny"
    );
    [m, kids, tiny]
}

/// Every reading position and note, as text.
fn read_and_noted(conn: &Connection) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'progress ' || user_id || ' ' || work_id FROM progress ORDER BY 1",
        "SELECT 'note ' || id || ' ' || work_id FROM note ORDER BY 1",
    ] {
        let mut statement = conn.prepare(sql).expect(sql);
        let read = statement
            .query_map([], |r| r.get::<_, String>(0))
            .expect(sql)
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect(sql);
        rows.extend(read);
    }
    rows
}

/// [`what_a_removal_touches`] without the links, and the positions and notes.
fn all_but_the_links(conn: &Connection) -> Vec<String> {
    let mut rows: Vec<String> = what_a_removal_touches(conn)
        .into_iter()
        .filter(|row| !row.starts_with("link "))
        .collect();
    rows.extend(read_and_noted(conn));
    rows
}

/// Overlapping-libraries spec §5: Remove from this library unlinks. Kids holds
/// `k` and `m`, and Manga holds `m` too. Unlinking `m` from Kids takes away its
/// link and nothing else: every root, work, member, scan entry, grant,
/// position and note stands, and Kids keeps `k`. `kid`, granted Kids alone,
/// loses sight of `m`, its position kept; `fan`, granted Manga, and `both`
/// keep it. The rehearsal answers the same and writes nothing.
#[test]
fn unlinking_a_dir_another_library_holds_deletes_nothing_and_only_its_accounts_lose_sight() {
    let parent = crate::tests::tempdir("scan-unlink-shared");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = a_scanned_dir_in(&conn, parent.path(), "m", "Alpha", "Manga");
    let k = a_scanned_dir_in(&conn, parent.path(), "k", "Beta", "Kids");
    let (manga, kids) = (m.libraries[0], k.libraries[0]);
    add_root_checked_in(&conn, &m.path, InLibrary::Id(kids)).expect("links m into Kids");
    let kid = an_account_granted(&conn, "kid", "user", &["Kids"]);
    let fan = an_account_granted(&conn, "fan", "user", &["Manga"]);
    let both = an_account_granted(&conn, "both", "user", &["Kids", "Manga"]);
    read_with_notes(&conn, kid, work_under(&conn, m.id), 2);
    let stored = all_but_the_links(&conn);
    let linked = links(&conn);
    assert_eq!(linked.len(), 3, "the fixture: m in both, k in Kids");
    let said = Unlinked {
        loses_sight: vec!["kid".to_string()],
        library_removed: None,
        grants_removed: 0,
        left_with_no_library: Vec::new(),
    };

    assert_eq!(
        unlink(&conn, m.id, kids, true),
        Ok(said.clone()),
        "the rehearsal"
    );
    assert_eq!(links(&conn), linked, "the rehearsal unlinks nothing");
    assert_eq!(
        all_but_the_links(&conn),
        stored,
        "the rehearsal writes nothing"
    );

    assert_eq!(unlink(&conn, m.id, kids, false), Ok(said), "the unlink");
    let mut left = vec![(manga, m.id), (kids, k.id)];
    left.sort_unstable();
    assert_eq!(links(&conn), left, "only Kids' link to m went");
    assert_eq!(all_but_the_links(&conn), stored, "nothing else changed");
    let sees = |account: i64, root: i64| {
        crate::account::access_for(&conn, account)
            .expect("reads")
            .allows(root)
    };
    assert!(!sees(kid, m.id), "kid no longer sees m");
    assert!(sees(kid, k.id), "kid keeps Kids' k");
    assert!(sees(fan, m.id), "fan keeps m through Manga");
    assert!(sees(both, m.id), "both keeps m through Manga");
}

/// O248: `m/kids` is in Kids and Also, and `ann`, granted Manga and Kids, keeps
/// it through Manga when it leaves Kids, but loses `m/kids/tiny`, which only
/// Kids' `m/kids` reached. The unlink names her.
#[test]
fn an_unlink_names_who_loses_a_directory_nested_in_it() {
    let parent = crate::tests::tempdir("scan-unlink-nested");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let [_, kids_dir, tiny] = three_nested_levels(&conn, parent.path());
    add_root_checked_in(&conn, &kids_dir.path, InLibrary::Named("Also"))
        .expect("links m/kids into Also");
    let ann = an_account_granted(&conn, "ann", "user", &["Manga", "Kids"]);
    let sees = |root: i64| {
        crate::account::access_for(&conn, ann)
            .expect("reads")
            .allows(root)
    };
    assert!(
        sees(kids_dir.id) && sees(tiny.id),
        "the fixture: ann sees m/kids and m/kids/tiny"
    );

    let unlinked = unlink(&conn, kids_dir.id, library_called(&conn, "Kids"), false)
        .expect("unlinks m/kids from Kids");

    assert_eq!(unlinked.loses_sight, vec!["ann"], "{unlinked:?}");
    assert!(
        sees(kids_dir.id) && !sees(tiny.id),
        "ann keeps m/kids through Manga and loses m/kids/tiny"
    );
}

/// Kids holds only `m`, which Manga holds too, so unlinking `m` empties Kids,
/// and migration 19 deletes it with its grants. The answer names Kids, counts
/// its two grants and names `kid`, whose only grant it was; `both` keeps
/// Manga. The rehearsal says the same and Kids stands after it.
#[test]
fn unlinking_a_librarys_last_dir_removes_the_library_and_names_who_then_sees_nothing() {
    let parent = crate::tests::tempdir("scan-unlink-empties");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = a_scanned_dir_in(&conn, parent.path(), "m", "Alpha", "Manga");
    add_root_checked_in(&conn, &m.path, InLibrary::Named("Kids")).expect("links m into Kids");
    let kids = library_called(&conn, "Kids");
    let kid = an_account_granted(&conn, "kid", "user", &["Kids"]);
    let both = an_account_granted(&conn, "both", "user", &["Kids", "Manga"]);
    let said = Unlinked {
        loses_sight: vec!["kid".to_string()],
        library_removed: Some("Kids".to_string()),
        grants_removed: 2,
        left_with_no_library: vec!["kid".to_string()],
    };

    assert_eq!(
        unlink(&conn, m.id, kids, true),
        Ok(said.clone()),
        "the rehearsal"
    );
    assert_eq!(
        crate::libraries::name_of(&conn, kids).expect("reads"),
        Some("Kids".to_string()),
        "the rehearsal removes no library"
    );

    assert_eq!(unlink(&conn, m.id, kids, false), Ok(said), "the unlink");
    assert_eq!(crate::libraries::name_of(&conn, kids).expect("reads"), None);
    sees_no_library(&conn, kid);
    assert!(
        crate::account::access_for(&conn, both)
            .expect("reads")
            .allows(m.id),
        "both keeps m through Manga"
    );
}

/// Remove from this library never deletes (decision 24): on a directory's last
/// link it is refused, pointing to Remove this directory. A library that does
/// not hold the directory, and a directory or library that does not exist, are
/// refused too. Each refusal is the rehearsal's as well, and writes nothing.
#[test]
fn an_unlink_is_refused_on_a_dirs_last_link_and_where_the_library_does_not_hold_it() {
    let parent = crate::tests::tempdir("scan-unlink-refused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = a_scanned_dir_in(&conn, parent.path(), "m", "Alpha", "Manga");
    let k = a_scanned_dir_in(&conn, parent.path(), "k", "Beta", "Kids");
    let (manga, kids) = (m.libraries[0], k.libraries[0]);
    let stored = what_a_removal_touches(&conn);

    for dry_run in [true, false] {
        assert_eq!(
            unlink(&conn, m.id, manga, dry_run),
            Err(UnlinkError::Unusable(format!(
                "`{}` is in no other library, so it can only be removed: use Remove this directory.",
                m.path.display()
            ))),
            "{dry_run}"
        );
        assert_eq!(
            unlink(&conn, m.id, kids, dry_run),
            Err(UnlinkError::Missing(format!(
                "`{}` is not in Kids",
                m.path.display()
            ))),
            "{dry_run}"
        );
        assert_eq!(
            unlink(&conn, 999, manga, dry_run),
            Err(UnlinkError::Missing(
                "there is no directory 999".to_string()
            )),
            "{dry_run}"
        );
        assert_eq!(
            unlink(&conn, m.id, 999, dry_run),
            Err(UnlinkError::Missing("there is no library 999".to_string())),
            "{dry_run}"
        );
        assert_eq!(what_a_removal_touches(&conn), stored, "{dry_run}");
    }
}

/// One removal can empty two libraries: `m` is the only directory of A and of
/// B, and C holds `c`. Removing `m` names both. `left_with_no_library` is every
/// limited account whose every grant is among them: `only_a`, and `a_and_b`,
/// whom asking of A and then of B would miss. `b_and_c` keeps C, and `gone`,
/// limited with no grant at all, has nothing to lose.
#[test]
fn removing_a_dir_that_empties_two_libraries_names_both_and_whoever_held_only_them() {
    let parent = crate::tests::tempdir("scan-removal-empties-two");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = a_scanned_dir_in(&conn, parent.path(), "m", "Alpha", "A");
    add_root_checked_in(&conn, &m.path, InLibrary::Named("B")).expect("links m into B");
    let c = a_scanned_dir_in(&conn, parent.path(), "c", "Beta", "C");
    let (a, b) = (library_called(&conn, "A"), library_called(&conn, "B"));
    let only_a = an_account_granted(&conn, "only_a", "user", &["A"]);
    let a_and_b = an_account_granted(&conn, "a_and_b", "user", &["A", "B"]);
    let b_and_c = an_account_granted(&conn, "b_and_c", "user", &["B", "C"]);
    let gone = an_account_granted(&conn, "gone", "user", &[]);
    conn.execute("UPDATE user SET every_library = 0 WHERE id = ?1", [gone])
        .expect("gone is limited and granted nothing");
    let before = what_a_removal_touches(&conn);

    let (cost, _) = removal_cost(&conn, m.id).expect("the dry run goes ahead");
    assert_eq!(cost.libraries_removed, vec!["A", "B"], "{cost:?}");
    assert_eq!(
        cost.left_with_no_library,
        vec!["a_and_b", "only_a"],
        "{cost:?}"
    );
    assert_eq!(cost.kept, Vec::new(), "{cost:?}");
    assert_eq!(
        what_a_removal_touches(&conn),
        before,
        "the dry run removes nothing"
    );

    let (removal, _) = remove_root(&conn, m.id).expect("the removal goes ahead");

    assert_eq!(removal, cost, "the removal does what the dry run said");
    for gone in [a, b] {
        assert_eq!(crate::libraries::name_of(&conn, gone).expect("reads"), None);
    }
    for account in [only_a, a_and_b] {
        sees_no_library(&conn, account);
    }
    assert!(
        crate::account::access_for(&conn, b_and_c)
            .expect("reads")
            .allows(c.id),
        "b_and_c keeps C"
    );
}

/// Overlapping-libraries spec §5: removing a library unlinks the directories
/// another library holds and removes the rest. Kids holds `m`, which Manga
/// holds too, and `n`, which it alone holds. Removing Kids removes `n` with its
/// work, position and notes, keeps `m` in Manga with its own, and says it left
/// `m` to Manga. The rehearsal says the same and writes nothing.
#[test]
fn removing_a_library_unlinks_its_shared_dirs_and_removes_the_rest() {
    let parent = crate::tests::tempdir("scan-library-removal-shared");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = a_scanned_dir_in(&conn, parent.path(), "m", "Alpha", "Manga");
    let n = a_scanned_dir_in(&conn, parent.path(), "n", "Beta", "Kids");
    let (manga, kids) = (m.libraries[0], n.libraries[0]);
    add_root_checked_in(&conn, &m.path, InLibrary::Id(kids)).expect("links m into Kids");
    let reader = an_account_granted(&conn, "reader", "user", &["Kids", "Manga"]);
    let kept_work = work_under(&conn, m.id);
    read_with_notes(&conn, reader, kept_work, 1);
    read_with_notes(&conn, reader, work_under(&conn, n.id), 2);
    let before = what_a_removal_touches(&conn);
    let said = Removal {
        works: 1,
        positions: 1,
        notes: 2,
        positions_page_gone: 0,
        notes_page_gone: 0,
        left_with_no_library: Vec::new(),
        libraries_removed: vec!["Kids".to_string()],
        kept: vec![Kept {
            root_id: m.id,
            path: m.path.clone(),
            libraries: vec!["Manga".to_string()],
        }],
        removed: vec![n.id],
    };

    let (cost, _) = library_removal_cost(&conn, kids).expect("the dry run goes ahead");
    assert_eq!(cost, said, "the dry run");
    assert_eq!(
        what_a_removal_touches(&conn),
        before,
        "the dry run removes nothing"
    );

    let (removal, _) = remove_library(&conn, kids).expect("the removal goes ahead");

    assert_eq!(removal, said, "the removal");
    assert_eq!(crate::libraries::name_of(&conn, kids).expect("reads"), None);
    assert_eq!(
        links(&conn),
        vec![(manga, m.id)],
        "m stays in Manga, n is gone"
    );
    assert_eq!(
        roots(&conn)
            .expect("lists")
            .iter()
            .map(|root| root.id)
            .collect::<Vec<_>>(),
        vec![m.id]
    );
    assert_eq!(work_under(&conn, m.id), kept_work, "m's work stands");
    assert_eq!(
        read_and_noted(&conn).len(),
        2,
        "m's position and note stand: {:?}",
        read_and_noted(&conn)
    );
}

/// Every row a derive reads or writes, as text: every table but the libraries,
/// their links and their grants. Also left out are `structure_version` and the
/// search index's own segments (`*_fts_*`), which any write moves, even one of
/// the values already there; the indexed rows themselves are kept.
fn every_row_but_the_libraries(conn: &Connection) -> Vec<(String, Vec<String>)> {
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                AND name NOT IN ('library', 'library_root', 'user_library')
              ORDER BY name",
        )
        .expect("prepares the table list")
        .query_map([], |r| r.get(0))
        .expect("lists the tables")
        .collect::<Result<_, _>>()
        .expect("reads the table list");
    tables
        .into_iter()
        .filter(|table| table != "structure_version" && !table.contains("_fts_"))
        .map(|table| {
            let mut statement = conn
                .prepare(&format!("SELECT * FROM \"{table}\""))
                .expect("prepares a dump");
            let columns = statement.column_count();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|c| {
                            row.get::<_, rusqlite::types::Value>(c)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map(|cells| cells.join("|"))
                })
                .expect("dumps")
                .collect::<Result<Vec<_>, _>>()
                .expect("reads the dump");
            rows.sort();
            (table, rows)
        })
        .collect()
}

/// A library all of whose directories another library holds is only unlinked:
/// nothing is counted, `m` is left to Manga, and Kids goes with its last link.
/// Nothing is derived, for the rehearsal or the removal, and a derive run
/// afterwards changes no row, so none was owed.
#[test]
fn removing_a_library_whose_every_dir_is_shared_removes_only_the_library() {
    let parent = crate::tests::tempdir("scan-library-removal-all-shared");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = a_scanned_dir_in(&conn, parent.path(), "m", "Alpha", "Manga");
    add_root_checked_in(&conn, &m.path, InLibrary::Named("Kids")).expect("links m into Kids");
    let kids = library_called(&conn, "Kids");
    let manga = m.libraries[0];
    let before = all_but_the_links(&conn);
    let derived_before = every_row_but_the_libraries(&conn);
    let said = Removal {
        libraries_removed: vec!["Kids".to_string()],
        kept: vec![Kept {
            root_id: m.id,
            path: m.path.clone(),
            libraries: vec!["Manga".to_string()],
        }],
        ..Removal::default()
    };

    let cost = library_removal_cost(&conn, kids).expect("costs");
    assert_eq!(cost, (said.clone(), crate::derive::DeriveReport::default()));
    let removal = remove_library(&conn, kids).expect("removes Kids");

    assert_eq!(removal, (said, crate::derive::DeriveReport::default()));
    assert_eq!(every_row_but_the_libraries(&conn), derived_before);
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        every_row_but_the_libraries(&conn),
        derived_before,
        "a derive after the unlink-only removal changes nothing"
    );
    assert_eq!(links(&conn), vec![(manga, m.id)]);
    let mut wanted = before;
    wanted.retain(|row| !row.starts_with(&format!("library {kids} ")));
    assert_eq!(all_but_the_links(&conn), wanted, "only Kids' row went");
}

/// One subdirectory and three archives: `a.cbz` at the top, `b.cbz` and
/// `c.cbz` in `Series`. A walk reads five entries, the root's own among them.
fn three_archives_in_a_root_and_its_subdirectory(label: &str) -> crate::tests::TempDir {
    let dir = crate::tests::tempdir(label);
    write_archive(&dir.path().join("a.cbz"), 2);
    write_archive(&dir.path().join("Series").join("b.cbz"), 2);
    write_archive(&dir.path().join("Series").join("c.cbz"), 2);
    dir
}

#[test]
fn a_watched_walk_counts_files_found_then_entries_read_of_entries_to_read() {
    use crate::test_support::Recording;
    let tree = three_archives_in_a_root_and_its_subdirectory("scan-watched-counts");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let listing = (0..=3).map(|found| (Step::Listing, found, None));

    let first = Recording::default();
    let outcome = scan_root_watched(&conn, &root, &first).expect("first walk");
    let second = Recording::default();
    scan_root_watched(&conn, &root, &second).expect("second walk");

    assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
    let read = (0..=5).map(|read| (Step::Resolving, read, Some(5)));
    assert_eq!(
        first.reports(),
        listing.clone().chain(read).collect::<Vec<_>>(),
        "files found while listing, then entries read of the five to read"
    );
    assert_eq!(
        second.reports(),
        listing
            .chain([(Step::Resolving, 0, Some(0))])
            .collect::<Vec<_>>(),
        "every entry is unchanged, so none is to read"
    );
}

#[test]
fn a_walk_cancelled_between_entries_records_nothing() {
    use crate::test_support::Recording;
    let tree = three_archives_in_a_root_and_its_subdirectory("scan-watched-cancel");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let watch = Recording::cancelling(|(step, read, _)| step == Step::Resolving && read >= 2);

    let outcome = scan_root_watched(&conn, &root, &watch).expect("walk");

    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Resolving)),
        "{outcome:?}"
    );
    let read: Vec<_> = watch
        .reports()
        .into_iter()
        .filter(|(step, _, _)| *step == Step::Resolving)
        .collect();
    assert_eq!(
        read,
        (0..=2)
            .map(|read| (Step::Resolving, read, Some(5)))
            .collect::<Vec<_>>(),
        "the third entry is never read"
    );
    let rows: (i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM scan_entry WHERE root_id = ?1), \
                    (SELECT count(*) FROM dir_state WHERE root_id = ?1)",
            [root.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("counts");
    assert_eq!(rows, (0, 0), "no scan_entry and no dir_state row");
    let (at, status): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1",
            [root.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads the root row back");
    assert_eq!(
        (at, status),
        (None, None),
        "the walk is not recorded as run"
    );
}

/// A folder of loose page images is a scan's files too, so a library of them
/// does not read as stalled while it lists.
#[test]
fn a_listing_counts_loose_page_images_as_files_found() {
    use crate::test_support::Recording;
    let tree = crate::tests::tempdir("scan-watched-loose-pages");
    write_pages(&tree.path().join("Vol 1"), 4);
    write_archive(&tree.path().join("a.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let watch = Recording::default();

    scan_root_watched(&conn, &root, &watch).expect("walk");

    let listing: Vec<_> = watch
        .reports()
        .into_iter()
        .filter(|(step, _, _)| *step == Step::Listing)
        .collect();
    assert_eq!(
        listing,
        (0..=5)
            .map(|found| (Step::Listing, found, None))
            .collect::<Vec<_>>(),
        "four page images and one archive, each reported as it is found"
    );
}

// --- Counting again when the reader changes (O130) --------------------------

/// The signature this build counts a zip's pages under.
fn zip_signature() -> Option<String> {
    taiyaku_read::Format::Zip
        .token()
        .and_then(taiyaku_read::reader_signature)
}

/// A digest planted by hand: `digit` 64 times.
fn planted(digit: char) -> String {
    digit.to_string().repeat(64)
}

/// `root_id`'s row at `rel_path`: its `pages`, `sha256` and `counted_by`.
fn count_at(
    conn: &Connection,
    root_id: i64,
    rel_path: &str,
) -> (Option<i64>, Option<String>, Option<String>) {
    conn.query_row(
        "SELECT pages, sha256, counted_by FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
        rusqlite::params![root_id, rel_path],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
}

/// Scan `root`, and answer its report and how many entries it had to read:
/// the `of` of its first Resolving report.
fn scanned_reading(conn: &Connection, root: &Root) -> (ScanReport, Option<u64>) {
    let watch = crate::test_support::Recording::default();
    let outcome = scan_root_watched(conn, root, &watch).expect("scans");
    let Outcome::Done(report) = outcome else {
        panic!("the scan ran to the end: {outcome:?}");
    };
    let of = watch
        .reports()
        .into_iter()
        .find(|(step, _, _)| *step == Step::Resolving)
        .and_then(|(_, _, of)| of);
    (report, of)
}

/// `a.cbz` (three `.jxl` pages) and `b.cbz` (two JPEG pages), scanned, then
/// planted as counted before signatures existed: by no signature, `a.cbz`
/// with no count, and each with a digest (`planted('a')`, `planted('b')`).
fn counted_before_signatures(label: &str) -> (crate::tests::TempDir, Library, Root) {
    let tree = crate::tests::tempdir(label);
    write_archive_of(&tree.path().join("a.cbz"), 3, "jxl");
    write_archive(&tree.path().join("b.cbz"), 2);
    let library = Library::open_in_memory().expect("open");
    let root = {
        let conn = library.conn();
        let root = add_root(&conn, tree.path()).expect("add");
        scan_root(&conn, &root).expect("scans");
        conn.execute_batch(&format!(
            "UPDATE scan_entry SET pages = NULL, counted_by = NULL, sha256 = '{}'
              WHERE rel_path = 'a.cbz';
             UPDATE scan_entry SET counted_by = NULL, sha256 = '{}' WHERE rel_path = 'b.cbz';",
            planted('a'),
            planted('b'),
        ))
        .expect("plants a row counted before signatures existed");
        assert_eq!(
            (
                count_at(&conn, root.id, "a.cbz"),
                count_at(&conn, root.id, "b.cbz")
            ),
            (
                (None, Some(planted('a')), None),
                (Some(2), Some(planted('b')), None)
            ),
            "the fixture: counted before signatures existed"
        );
        root
    };
    (tree, library, root)
}

/// What a scan before signatures left is counted again by the next scan,
/// which opens each archive and hashes neither: `a.cbz` gets its count, both
/// keep their digests, and neither reads as updated or added (O130).
#[test]
fn an_archive_counted_by_another_reader_is_counted_again_and_not_hashed() {
    let (_tree, library, root) = counted_before_signatures("scan-recount");
    let conn = library.conn();

    let (report, of) = scanned_reading(&conn, &root);

    assert_eq!(of, Some(2), "both archives are read again");
    assert_eq!(
        count_at(&conn, root.id, "a.cbz"),
        (Some(3), Some(planted('a')), zip_signature()),
        "a.cbz"
    );
    assert_eq!(
        count_at(&conn, root.id, "b.cbz"),
        (Some(2), Some(planted('b')), zip_signature()),
        "b.cbz"
    );
    assert_eq!((report.updated, report.added), (0, 0), "{report:?}");
}

/// The RAR fixture these tests scan: three pages, and a `ComicInfo.xml` that is
/// no page.
pub(crate) const READABLE_RAR: &str = "rar5-compressed.rar";

/// The RAR fixture `file`, with what its manifest says it holds.
pub(crate) fn rar_fixture(file: &str) -> taiyaku_read::test_support::RarFixture {
    taiyaku_read::test_support::rar_fixtures()
        .into_iter()
        .find(|fixture| fixture.file == file)
        .unwrap_or_else(|| panic!("no RAR fixture {file}"))
}

/// What this build stamps a RAR's count with: `None` in a build without a RAR
/// backend.
fn rar_signature() -> Option<String> {
    taiyaku_read::Format::Rar
        .token()
        .and_then(taiyaku_read::reader_signature)
}

/// `root_id`'s row at `rel_path`, each column by name, all but `last_seen`,
/// which every scan moves.
fn whole_row(
    conn: &Connection,
    root_id: i64,
    rel_path: &str,
) -> Vec<(String, rusqlite::types::Value)> {
    let mut stmt = conn
        .prepare("SELECT * FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2")
        .expect("prepares");
    let names: Vec<String> = stmt.column_names().map(str::to_owned).collect();
    stmt.query_row(rusqlite::params![root_id, rel_path], |r| {
        (0..names.len())
            .map(|i| Ok((names[i].clone(), r.get(i)?)))
            .collect::<Result<Vec<_>, _>>()
    })
    .unwrap_or_else(|e| panic!("{rel_path} has a row: {e}"))
    .into_iter()
    .filter(|(name, _)| name != "last_seen")
    .collect()
}

/// What a scan of a build without a RAR backend left of a RAR, which it could
/// not count (ruling P2-R5): no count and no signature, and a digest planted
/// as `planted('r')`. A build with a RAR backend counts it at the next scan,
/// keeps its digest, and hashes nothing; a build without one has nothing to
/// count it again with and does not open it.
#[test]
fn a_rar_no_reader_counted_is_counted_again_and_not_hashed_by_a_build_that_reads_rar() {
    let fixture = rar_fixture(READABLE_RAR);
    let tree = crate::tests::tempdir("scan-recount-rar");
    std::fs::copy(&fixture.path, tree.path().join("a.cbz")).expect("copies the fixture");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("scans");
    conn.execute(
        "UPDATE scan_entry SET pages = NULL, counted_by = NULL, sha256 = ?1
          WHERE rel_path = 'a.cbz'",
        [planted('r')],
    )
    .expect("plants a row a build without a RAR backend left");

    let (report, of) = scanned_reading(&conn, &root);

    let reads_rar = taiyaku_read::Format::Rar.is_readable();
    assert_eq!(
        of,
        Some(u64::from(reads_rar)),
        "read again where RAR is read"
    );
    let pages = i64::try_from(fixture.pages.len()).expect("a few pages");
    assert_eq!(
        count_at(&conn, root.id, "a.cbz"),
        (
            reads_rar.then_some(pages),
            Some(planted('r')),
            rar_signature()
        ),
        "counted where RAR is read, under its signature, and not hashed"
    );
    assert_eq!((report.updated, report.added), (0, 0), "{report:?}");
}

/// Review Focus 2, the rollback: a RAR row a build with a RAR backend counted,
/// with the digest a derive took, is rewritten by no scan of either build. A
/// build without a RAR backend has no count to give it, so it keeps the row and
/// sniffs the file again at each scan; a build with one counted it under the
/// signature it holds and does not open it again. The test is the first of
/// those two scans in a build without the feature and the second in a build
/// with it, so the two builds' runs together take a store from the feature, to
/// none, to the feature again.
#[test]
fn a_rar_counted_by_a_build_that_reads_rar_keeps_its_row_through_any_build() {
    let fixture = rar_fixture(READABLE_RAR);
    let tree = crate::tests::tempdir("scan-rar-rollback");
    std::fs::copy(&fixture.path, tree.path().join("a.cbz")).expect("copies the fixture");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("scans");
    // A build without a RAR backend has no RAR signature of its own, and any
    // one differs from its none, as the feature's would.
    let feature = rar_signature().unwrap_or_else(|| "rar:a RAR backend".to_owned());
    conn.execute(
        "UPDATE scan_entry SET pages = ?1, counted_by = ?2, sha256 = ?3
          WHERE rel_path = 'a.cbz'",
        rusqlite::params![
            i64::try_from(fixture.pages.len()).expect("a few pages"),
            feature,
            planted('r')
        ],
    )
    .expect("plants the row a build with a RAR backend left");
    let planted_row = whole_row(&conn, root.id, "a.cbz");

    let (report, of) = scanned_reading(&conn, &root);

    let sniffs = u64::from(!taiyaku_read::Format::Rar.is_readable());
    assert_eq!(
        of,
        Some(sniffs),
        "opened only by a build without a RAR backend"
    );
    assert_eq!(
        whole_row(&conn, root.id, "a.cbz"),
        planted_row,
        "the row, its count and its digest are kept"
    );
    assert_eq!((report.updated, report.added), (0, 0), "{report:?}");
    assert_eq!(
        scanned_reading(&conn, &root).1,
        Some(sniffs),
        "and so at the scan after"
    );
}

/// Review Focus 2, the rollback of a RAR refused at the count: a build with a
/// RAR backend stamps it with its signature and no count, a build without one
/// has no signature to give it and so counts it again and takes the stamp off,
/// and the next build with one counts it again, refuses it again and stamps it
/// again. Each opens it once, keeps its digest, and opens it at no scan after.
/// The test is the scan after a feature scan in a build without the feature,
/// and the scan after that one in a build with it.
#[test]
fn a_rar_refused_at_the_count_is_counted_again_by_the_build_after_another() {
    let fixture = rar_fixture("rar5-encrypted-entry.rar");
    assert!(
        fixture.refused.is_some(),
        "the fixture: refused at the count"
    );
    let tree = crate::tests::tempdir("scan-rar-refused-rollback");
    std::fs::copy(&fixture.path, tree.path().join("a.cbz")).expect("copies the fixture");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("scans");
    // The row the scan before left. In a build without a RAR backend that scan
    // had one and stamped a signature, for which any string stands in, since
    // every one differs from this build's none; in a build with one, that scan
    // had none and stamped none.
    let reads_rar = taiyaku_read::Format::Rar.is_readable();
    let before = (!reads_rar).then(|| "rar:a RAR backend".to_owned());
    conn.execute(
        "UPDATE scan_entry SET pages = NULL, counted_by = ?1, sha256 = ?2
          WHERE rel_path = 'a.cbz'",
        rusqlite::params![before, planted('r')],
    )
    .expect("plants the row the build before left");

    let (report, of) = scanned_reading(&conn, &root);

    assert_eq!(of, Some(1), "counted again by either build");
    assert_eq!(
        count_at(&conn, root.id, "a.cbz"),
        (None, Some(planted('r')), rar_signature()),
        "no count, the digest kept, and this build's signature or none"
    );
    assert_eq!((report.updated, report.added), (0, 0), "{report:?}");
    assert_eq!(
        scanned_reading(&conn, &root).1,
        Some(0),
        "and not opened at the scan after"
    );
}

/// [`pages_of`] for the file at `rel_path` under `root`, as a scan asks it.
fn pages_counted(root: &Root, rel_path: &str) -> Result<(Option<u32>, Option<String>), String> {
    let meta = std::fs::metadata(root.path.join(rel_path)).expect("stats the file");
    let seen = Seen {
        rel_path: rel_path.to_owned(),
        kind: Kind::File,
        size: meta.len(),
        mtime: mtime_secs(&meta),
        pages: None,
        entry_count: None,
        listed_whole: true,
    };
    pages_of(root, &seen, format_of(root, &seen))
}

/// D9: the line a scan logs of a file whose count was refused names the file
/// and the refusal: a zip that will not open, and in a build that reads RAR,
/// each RAR fixture with the cause its manifest gives. A build without a RAR
/// backend counts no RAR, so it logs none, at each scan that sniffs one again.
#[test]
fn a_file_whose_count_is_refused_is_logged_with_its_cause() {
    let tree = crate::tests::tempdir("scan-uncounted-logged");
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(b"and no zip entry after the signature");
    std::fs::write(tree.path().join("broken.cbz"), bytes).expect("write");
    let refused: Vec<_> = taiyaku_read::test_support::rar_fixtures()
        .into_iter()
        .filter(|fixture| fixture.refused.is_some())
        .collect();
    assert!(refused.len() > 1, "the fixture: RARs refused at the count");
    for fixture in &refused {
        std::fs::copy(&fixture.path, tree.path().join(&fixture.file)).expect("copies");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    let logged = |rel_path: &str, cause: &str| {
        let line = pages_counted(&root, rel_path).expect_err(rel_path);
        let start = format!(
            "taiyaku: the scan left a file uncounted: {}: ",
            root.path.join(rel_path).display()
        );
        assert!(
            line.starts_with(&start) && line.contains(cause),
            "{rel_path}: {line}"
        );
    };

    logged("broken.cbz", "not a readable cbz");
    for fixture in &refused {
        let file = &fixture.file;
        if taiyaku_read::Format::Rar.is_readable() {
            logged(file, fixture.refused.as_deref().expect("refused"));
        } else {
            assert_eq!(pages_counted(&root, file), Ok((None, None)), "{file}");
        }
    }
}

/// A count taken again holds the reading it replaces as a changed file's is
/// held: the first one only, so a reading already held is kept, and only where
/// the count changes, so a count taken again at the same number holds nothing.
#[test]
fn a_count_taken_again_holds_only_a_first_reading_and_only_where_the_count_changes() {
    let (_tree, library, root) = counted_before_signatures("scan-recount-held");
    let conn = library.conn();
    let held = (
        Some(planted('h')),
        Some("zip".to_string()),
        Some(7),
        Some("held title".to_string()),
    );
    assert_eq!(
        conn.execute(
            "UPDATE scan_entry
                SET held_sha256 = ?2, held_format = ?3, held_pages = ?4, held_title = ?5
              WHERE root_id = ?1 AND rel_path = 'a.cbz'",
            rusqlite::params![root.id, held.0, held.1, held.2, held.3],
        ),
        Ok(1),
        "the fixture: a.cbz already holds a reading"
    );

    scan_root(&conn, &root).expect("scans again");

    assert_eq!(
        (
            count_at(&conn, root.id, "a.cbz").0,
            held_reading(&conn, root.id, "a.cbz")
        ),
        (Some(3), held),
        "a.cbz is counted again at three pages and keeps the reading it held"
    );
    assert_eq!(
        (
            count_at(&conn, root.id, "b.cbz").0,
            held_reading(&conn, root.id, "b.cbz")
        ),
        (Some(2), (None, None, None, None)),
        "b.cbz is counted again at the two pages it had and holds nothing"
    );
}

/// Counted again once, an archive is not opened by the scan after.
#[test]
fn an_archive_counted_by_this_reader_is_not_opened_again() {
    let (_tree, library, root) = counted_before_signatures("scan-recounted");
    let conn = library.conn();
    assert_eq!(
        scanned_reading(&conn, &root).1,
        Some(2),
        "the fixture: both archives are counted again"
    );

    let (_, of) = scanned_reading(&conn, &root);

    assert_eq!(of, Some(0));
}

/// A folder's pages are counted by every walk, so it keeps no signature, and
/// a folder of `.jxl` pages is not read again (O130).
#[test]
fn a_folder_is_not_counted_again_by_signature() {
    let tree = crate::tests::tempdir("scan-recount-folder");
    let folder = tree.path().join("Vol 1");
    std::fs::create_dir_all(&folder).expect("mkdir");
    for page in 0..3 {
        std::fs::write(
            folder.join(format!("{page:03}.jxl")),
            format!("not really a jxl: page {page}"),
        )
        .expect("write");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("scans");
    assert_eq!(
        count_at(&conn, root.id, "Vol 1"),
        (Some(3), None, None),
        "three pages, by no signature"
    );

    let (_, of) = scanned_reading(&conn, &root);

    assert_eq!(of, Some(0));
}

/// Review Focus 4: a scan cancelled while counts are due writes nothing, and
/// the next scan counts the archives again.
#[test]
fn a_cancel_while_counting_again_writes_nothing() {
    let (_tree, library, root) = counted_before_signatures("scan-recount-cancel");
    let conn = library.conn();
    let watch = crate::test_support::Recording::cancelling(|(step, _, _)| step == Step::Resolving);

    let outcome = scan_root_watched(&conn, &root, &watch).expect("walks");

    assert!(
        matches!(outcome, Outcome::Cancelled(Step::Resolving)),
        "{outcome:?}"
    );
    assert_eq!(
        (
            count_at(&conn, root.id, "a.cbz"),
            count_at(&conn, root.id, "b.cbz")
        ),
        (
            (None, Some(planted('a')), None),
            (Some(2), Some(planted('b')), None)
        ),
        "the rows are as planted"
    );
    scan_root(&conn, &root).expect("scans again");
    assert_eq!(
        count_at(&conn, root.id, "a.cbz"),
        (Some(3), Some(planted('a')), zip_signature()),
        "the next scan counts a.cbz again"
    );
}

/// A zip that will not open has no count under this reader either, and is
/// counted once: its signature is stored with no count, so the scan after it
/// does not open it again.
#[test]
fn an_archive_that_will_not_open_is_counted_once() {
    let tree = crate::tests::tempdir("scan-recount-broken");
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(b"and no zip entry after the signature");
    std::fs::write(tree.path().join("broken.cbz"), bytes).expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("scans");
    conn.execute(
        "UPDATE scan_entry SET counted_by = NULL WHERE rel_path = 'broken.cbz'",
        [],
    )
    .expect("plants a row counted before signatures existed");
    let format: Option<String> = conn
        .query_row(
            "SELECT format FROM scan_entry WHERE rel_path = 'broken.cbz'",
            [],
            |r| r.get(0),
        )
        .expect("reads the row");
    assert_eq!(
        format.as_deref(),
        taiyaku_read::Format::Zip.token(),
        "the fixture: the bytes sniff as a zip"
    );

    scan_root(&conn, &root).expect("scans again");

    assert_eq!(
        count_at(&conn, root.id, "broken.cbz"),
        (None, None, zip_signature()),
        "no count, by this reader"
    );
    assert_eq!(scanned_reading(&conn, &root).1, Some(0), "not opened again");
}

/// A count taken again whose read fails keeps the row as it was, signature
/// and all, so a reader change never costs a member its count, and the next
/// scan tries again. Each case's bytes are replaced at the size and mtime the
/// row holds, so the walk still sees the file unchanged.
#[test]
fn a_count_taken_again_whose_read_fails_keeps_the_row_and_is_tried_again() {
    for (case, page_extension, zip_signature_kept) in [
        ("a counted zip that no longer sniffs as one", "jpg", false),
        ("a counted zip that no longer opens", "jpg", true),
        (
            "an uncounted zip that no longer sniffs as one",
            "heic",
            false,
        ),
    ] {
        let tree = crate::tests::tempdir("scan-recount-unread");
        let archive = tree.path().join("a.cbz");
        write_archive_of(&archive, 2, page_extension);
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let root = add_root(&conn, tree.path()).expect("add");
        scan_root(&conn, &root).expect("scans");
        conn.execute(
            "UPDATE scan_entry SET counted_by = NULL WHERE rel_path = 'a.cbz'",
            [],
        )
        .expect("plants a row counted before signatures existed");
        let row = |conn: &Connection| -> (Option<String>, Option<i64>, Option<String>) {
            conn.query_row(
                "SELECT format, pages, counted_by FROM scan_entry WHERE rel_path = 'a.cbz'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("reads the row")
        };
        let planted = row(&conn);
        let was = std::fs::metadata(&archive).expect("stats the archive");
        let mut junk = if zip_signature_kept {
            b"PK\x03\x04".to_vec()
        } else {
            Vec::new()
        };
        junk.resize(usize::try_from(was.len()).expect("a small archive"), b'x');
        std::fs::write(&archive, &junk).expect("overwrites the archive");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&archive)
            .and_then(|file| file.set_modified(was.modified()?))
            .expect("puts the mtime back");
        let now = std::fs::metadata(&archive).expect("stats the archive again");
        assert_eq!(
            (
                (now.len(), now.modified().ok()),
                taiyaku_read::sniff(&archive)
                    .ok()
                    .and_then(taiyaku_read::Format::token),
                taiyaku_read::member_pages(&archive).is_err(),
                planted.clone(),
            ),
            (
                (was.len(), was.modified().ok()),
                taiyaku_read::Format::Zip
                    .token()
                    .filter(|_| zip_signature_kept),
                true,
                (
                    taiyaku_read::Format::Zip.token().map(str::to_string),
                    (page_extension == "jpg").then_some(2),
                    None
                ),
            ),
            "the fixture, {case}: unchanged to the walk, unreadable, and counted before signatures existed"
        );

        scan_root(&conn, &root).expect("scans again");

        assert_eq!(row(&conn), planted, "{case}: the row is kept");
        assert_eq!(
            scanned_reading(&conn, &root).1,
            Some(1),
            "{case}: the next scan tries again"
        );
    }
}

/// A file new to the store, and one whose bytes changed, are each counted by
/// this reader, so no later scan counts them again.
#[test]
fn a_new_or_changed_archive_records_this_readers_signature() {
    let tree = crate::tests::tempdir("scan-signature-written");
    let archive = tree.path().join("a.cbz");
    write_archive(&archive, 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("add");
    scan_root(&conn, &root).expect("scans");
    assert_eq!(
        count_at(&conn, root.id, "a.cbz").2,
        zip_signature(),
        "the new file's row"
    );
    conn.execute(
        "UPDATE scan_entry SET counted_by = NULL WHERE rel_path = 'a.cbz'",
        [],
    )
    .expect("plants a row counted before signatures existed");
    write_archive(&archive, 3);

    let report = scan_root(&conn, &root).expect("scans again");

    assert_eq!(report.updated, 1, "the fixture: a.cbz changed");
    assert_eq!(
        count_at(&conn, root.id, "a.cbz").2,
        zip_signature(),
        "the changed file's row"
    );
}

/// The folder volume [`counted_short_of_a_page`] leaves.
pub(crate) const SHORT_FOLDER: &str = "S/S - v01";

/// [`SHORT_FOLDER`] under `top` as a reader that did not read `.jxl` left it:
/// walked and derived over its two JPEG pages, with a third page beside them
/// that reader passed over.
pub(crate) fn counted_short_of_a_page(conn: &Connection, top: &Path) -> Root {
    write_pages(&top.join(SHORT_FOLDER), 2);
    let root = add_root(conn, top).expect("adds the directory");
    scan_root(conn, &root).expect("walks it");
    crate::derive::derive(conn).expect("derives");
    holds_a_page_it_was_not_counted_with(conn, &root, SHORT_FOLDER);
    root
}

/// A page in `folder` that the walk behind its row passed over: the row takes
/// the folder's stamp as it stands with the page there, so no stamp moves when
/// a reader first counts it.
pub(crate) fn holds_a_page_it_was_not_counted_with(conn: &Connection, root: &Root, folder: &str) {
    let dir = root.path.join(folder);
    std::fs::write(dir.join("002.jxl"), b"a page that reader passed over").expect("write");
    let (size, mtime) = crate::fingerprints::stamp_of(&dir).expect("the folder's stamp");
    conn.execute(
        "UPDATE scan_entry SET size = ?1, mtime = ?2 WHERE root_id = ?3 AND rel_path = ?4",
        rusqlite::params![size, mtime, root.id, folder],
    )
    .expect("records the stamp");
}

/// `page` of `dir` rewritten in place, which moves no stamp of the folder's, so
/// the digest a hash would find differs from `digest` from here on: a digest
/// still `digest` afterwards was not taken again.
fn rewritten_so_a_hash_would_show(dir: &Path, page: &str, digest: &str) {
    std::fs::write(dir.join(page), b"other bytes under the same name").expect("write");
    assert_ne!(
        taiyaku_read::member_digest(dir).expect("hashes the folder"),
        digest,
        "the fixture: a hash now finds other bytes"
    );
}

/// The digest hashed the pages one reader read, and this reader counts more
/// under the same stamp, so no read of the folder hashes to it again.
#[test]
fn a_folder_counted_anew_under_a_still_stamp_gives_up_its_digest_and_holds_it() {
    let tree = crate::tests::tempdir("scan-counted-anew");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = counted_short_of_a_page(&conn, tree.path());
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the derive took its digest");

    let report = scan_root(&conn, &root).expect("walks it again");

    assert_eq!(report.updated, 1, "read as a changed folder is");
    assert_eq!(
        digest_at(&conn, &root, SHORT_FOLDER),
        None,
        "its digest goes"
    );
    assert_eq!(
        held_reading(&conn, root.id, SHORT_FOLDER),
        (Some(digest), Some("dir".to_string()), Some(2), None),
        "and is held with the count it was taken under"
    );
}

/// The upgrade promises to hash only what it must.
#[test]
fn a_folder_whose_count_stands_is_not_hashed_again() {
    let tree = crate::tests::tempdir("scan-count-stands");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    write_pages(&tree.path().join(SHORT_FOLDER), 2);
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the derive took its digest");
    rewritten_so_a_hash_would_show(&root.path.join(SHORT_FOLDER), "000.jpg", &digest);

    let report = scan_root(&conn, &root).expect("walks it again");
    crate::derive::derive(&conn).expect("derives again");

    assert_eq!(report.updated, 0, "nothing the walk read as changed");
    assert_eq!(digest_at(&conn, &root, SHORT_FOLDER), Some(digest));
}

/// An archive's walk finds no page total to hold its count against.
#[test]
fn an_archive_the_walk_passes_keeps_its_digest() {
    const ARCHIVE: &str = "S/S - v01.cbz";
    let tree = crate::tests::tempdir("scan-archive-stands");
    write_archive(&tree.path().join(ARCHIVE), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, ARCHIVE).expect("the derive took its digest");

    let report = scan_root(&conn, &root).expect("walks it again");

    assert_eq!(report.updated, 0, "nothing the walk read as changed");
    assert_eq!(digest_at(&conn, &root, ARCHIVE), Some(digest));
}

/// The count is stored as the digest goes, so the next walk finds them agreeing
/// and every walk after the first costs no hash.
#[test]
fn a_walk_after_the_digest_was_taken_again_hashes_nothing() {
    let tree = crate::tests::tempdir("scan-second-walk");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = counted_short_of_a_page(&conn, tree.path());
    let earlier = digest_at(&conn, &root, SHORT_FOLDER).expect("the derive took its digest");
    scan_root(&conn, &root).expect("walks it again");
    crate::derive::derive(&conn).expect("derives again");
    let taken = digest_at(&conn, &root, SHORT_FOLDER).expect("the digest is taken again");
    assert_ne!(taken, earlier, "the fixture: it hashed the third page too");
    rewritten_so_a_hash_would_show(&root.path.join(SHORT_FOLDER), "000.jpg", &taken);

    let report = scan_root(&conn, &root).expect("walks it a third time");
    crate::derive::derive(&conn).expect("derives a third time");

    assert_eq!(report.updated, 0, "nothing the walk read as changed");
    assert_eq!(digest_at(&conn, &root, SHORT_FOLDER), Some(taken));
}

/// A series folder's total moves with every volume beneath it, and it holds no
/// digest to take again.
#[test]
fn a_total_moved_from_beneath_hashes_no_folder_that_holds_no_digest() {
    let tree = crate::tests::tempdir("scan-series-total");
    write_pages(&tree.path().join("S").join("S - v01"), 2);
    write_pages(&tree.path().join("S").join("S - v02"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let untouched = digest_at(&conn, &root, "S/S - v01").expect("the volume's digest");
    assert_eq!(digest_at(&conn, &root, "S"), None, "the fixture: no digest");
    write_pages(&tree.path().join("S").join("S - v02"), 3);

    let report = scan_root(&conn, &root).expect("walks it again");
    crate::derive::derive(&conn).expect("derives again");

    assert_eq!(
        report.updated, 1,
        "the volume that changed, and not its series"
    );
    assert_eq!(digest_at(&conn, &root, "S"), None);
    assert_eq!(digest_at(&conn, &root, "S/S - v01"), Some(untouched));
}

/// A volume's digest takes in its chapter folders, so a page added to one moves
/// the volume's total and its digest, once.
#[test]
fn a_page_added_beneath_a_volume_has_it_hashed_once() {
    let tree = crate::tests::tempdir("scan-beneath-a-volume");
    let volume = tree.path().join(SHORT_FOLDER);
    write_pages(&volume, 1);
    write_pages(&volume.join("c001"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let earlier = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    write_pages(&root.path.join(SHORT_FOLDER).join("c001"), 3);

    scan_root(&conn, &root).expect("walks it again");
    crate::derive::derive(&conn).expect("derives again");
    let taken = digest_at(&conn, &root, SHORT_FOLDER).expect("the digest is taken again");
    assert_ne!(taken, earlier, "the new page is in the digest");
    rewritten_so_a_hash_would_show(&root.path.join(SHORT_FOLDER), "000.jpg", &taken);

    scan_root(&conn, &root).expect("walks it a third time");
    crate::derive::derive(&conn).expect("derives a third time");

    assert_eq!(digest_at(&conn, &root, SHORT_FOLDER), Some(taken));
}

/// A total short of what could not be listed says nothing of the pages, and a
/// digest given up now could not be taken again while the folder stays
/// unreadable: the derive would name the volume gone.
#[test]
fn a_folder_the_walk_could_not_list_whole_keeps_its_digest() {
    use std::os::unix::fs::PermissionsExt as _;
    let tree = crate::tests::tempdir("scan-unlisted-beneath");
    let volume = tree.path().join(SHORT_FOLDER);
    write_pages(&volume, 1);
    write_pages(&volume.join("c001").join("part"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let locked = root.path.join(SHORT_FOLDER).join("c001").join("part");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // chmod does not stop root, which CI's container runs as.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!(
            "NOT MEASURED: a_folder_the_walk_could_not_list_whole_keeps_its_digest (this process can list a 0000 directory)"
        );
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        return;
    }

    let report = scan_root(&conn, &root).expect("walks what it can");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    assert_eq!(digest_at(&conn, &root, SHORT_FOLDER), Some(digest));
    assert_eq!(
        held_reading(&conn, root.id, SHORT_FOLDER).0,
        None,
        "nothing held"
    );
}

/// A page gone from a chapter moves the volume's total and no stamp of the
/// volume's, so only the count says the digest hashed a page it no longer holds.
#[test]
fn a_page_removed_beneath_a_volume_has_it_hashed_once() {
    let tree = crate::tests::tempdir("scan-removed-beneath");
    let volume = tree.path().join(SHORT_FOLDER);
    write_pages(&volume, 1);
    write_pages(&volume.join("c001"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let earlier = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let stamp = crate::fingerprints::stamp_of(&volume);
    std::fs::remove_file(volume.join("c001").join("002.jpg")).expect("removes a page");
    assert_eq!(
        crate::fingerprints::stamp_of(&volume),
        stamp,
        "the fixture: the volume's stamp stands"
    );

    scan_root(&conn, &root).expect("walks it again");
    crate::derive::derive(&conn).expect("derives again");

    let taken = digest_at(&conn, &root, SHORT_FOLDER).expect("the digest is taken again");
    assert_ne!(taken, earlier, "the removed page has left the digest");
    assert_eq!(
        taken,
        taiyaku_read::member_digest(&volume).expect("hashes the folder")
    );
}

/// The walk of a directory newly nested over a folder is the first to count it
/// anew, and every walk after it finds its own count standing, so the digest
/// has to go here or it never goes.
#[test]
fn a_folder_counted_anew_gives_up_its_digest_to_the_directory_nested_over_it() {
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-nested-counted-anew");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = counted_short_of_a_page(&conn, tree.path());
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the derive took its digest");
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;

    scan_root(&conn, &kids).expect("walks it");

    assert_eq!(digest_at(&conn, &kids, OWN), None, "its digest goes");
    assert_eq!(
        held_reading(&conn, kids.id, OWN),
        (Some(digest), Some("dir".to_string()), Some(2), None),
        "and is held with the count it was taken under"
    );
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        digest_at(&conn, &kids, OWN),
        Some(taiyaku_read::member_digest(&kids.path.join(OWN)).expect("hashes the folder")),
        "the derive takes it again over every page"
    );
}

/// The upgrade hashes only what it must, and a take-over is no reason to.
#[test]
fn a_folder_whose_count_stands_keeps_its_digest_for_the_directory_nested_over_it() {
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-nested-count-stands");
    write_pages(&tree.path().join(SHORT_FOLDER), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the derive took its digest");
    rewritten_so_a_hash_would_show(&root.path.join(SHORT_FOLDER), "000.jpg", &digest);
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;

    scan_root(&conn, &kids).expect("walks it");
    crate::derive::derive(&conn).expect("derives again");

    assert_eq!(digest_at(&conn, &kids, OWN), Some(digest));
    assert_eq!(held_reading(&conn, kids.id, OWN).0, None, "nothing held");
}

/// A nested walk short of a directory it could not list says no more of the
/// pages than the outer directory's walk would.
#[test]
fn a_folder_a_nested_walk_could_not_list_whole_keeps_its_digest() {
    use std::os::unix::fs::PermissionsExt as _;
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-nested-unlisted");
    let volume = tree.path().join(SHORT_FOLDER);
    write_pages(&volume, 1);
    write_pages(&volume.join("c001").join("part"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;
    let locked = root.path.join(SHORT_FOLDER).join("c001").join("part");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // chmod does not stop root, which CI's container runs as.
    if std::fs::read_dir(&locked).is_ok() {
        eprintln!(
            "NOT MEASURED: a_folder_a_nested_walk_could_not_list_whole_keeps_its_digest (this process can list a 0000 directory)"
        );
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        return;
    }

    let report = scan_root(&conn, &kids).expect("walks what it can");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    assert_eq!(digest_at(&conn, &kids, OWN), Some(digest));
    assert_eq!(held_reading(&conn, kids.id, OWN).0, None, "nothing held");
}

/// [`SHORT_FOLDER`] under `top`, walked: one page of its own and two in
/// `c001/part`, the directory a test goes on to make unlistable.
fn walked_volume_with_a_part(conn: &Connection, top: &Path) -> Root {
    let volume = top.join(SHORT_FOLDER);
    write_pages(&volume, 1);
    write_pages(&volume.join("c001").join("part"), 2);
    let root = add_root(conn, top).expect("adds the directory");
    scan_root(conn, &root).expect("walks it");
    root
}

/// What `walk` returns while `dir` cannot be listed, or `None` where this
/// process lists a 0000 directory all the same, as root in CI's container does.
fn while_unlistable<T>(dir: &Path, test: &str, walk: impl FnOnce() -> T) -> Option<T> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let walked = if std::fs::read_dir(dir).is_ok() {
        eprintln!("NOT MEASURED: {test} (this process can list a 0000 directory)");
        None
    } else {
        Some(walk())
    };
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    walked
}

/// The pages of the member holding `digest`.
fn member_pages_of(conn: &Connection, digest: &str) -> i64 {
    conn.query_row(
        "SELECT pages FROM member WHERE sha256 = ?1",
        [digest],
        |r| r.get(0),
    )
    .expect("the digest is a member's")
}

/// Stored short, the count would read as a change once the folder lists whole,
/// and a paused directory's derive would write the short count to the member.
#[test]
fn a_walk_short_of_a_directory_leaves_a_digested_folders_count_standing() {
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-short-walk-count");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = walked_volume_with_a_part(&conn, tree.path());
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let part = root.path.join(SHORT_FOLDER).join("c001").join("part");

    let Some(report) = while_unlistable(
        &part,
        "a_walk_short_of_a_directory_leaves_a_digested_folders_count_standing",
        || scan_root(&conn, &root).expect("walks what it can"),
    ) else {
        return;
    };

    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    assert_eq!(count_at(&conn, root.id, SHORT_FOLDER).0, Some(3));
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;
    scan_root(&conn, &kids).expect("walks it whole");
    pause(&conn, std::slice::from_ref(&kids)).expect("pauses");
    crate::derive::derive(&conn).expect("derives with Kids frozen");
    assert_eq!(member_pages_of(&conn, &digest), 3, "all three are on disk");
    assert_eq!(digest_at(&conn, &kids, OWN), Some(digest));
    assert_eq!(held_reading(&conn, kids.id, OWN).0, None, "nothing held");
}

/// A directory lost while no walk could list it moves no stamp of the volume's,
/// so only the count the digest was taken under can show the digest is stale.
#[test]
fn a_directory_lost_after_a_nested_walk_could_not_list_it_has_the_digest_taken_again() {
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-short-nested-walk-lost");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = walked_volume_with_a_part(&conn, tree.path());
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;
    let volume = root.path.join(SHORT_FOLDER);
    let part = volume.join("c001").join("part");

    let Some(report) = while_unlistable(
        &part,
        "a_directory_lost_after_a_nested_walk_could_not_list_it_has_the_digest_taken_again",
        || scan_root(&conn, &kids).expect("walks what it can"),
    ) else {
        return;
    };

    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    assert_eq!(
        count_at(&conn, kids.id, OWN),
        (Some(3), Some(digest.clone()), None),
        "the carried row keeps the count its digest was taken under"
    );
    let stamp = crate::fingerprints::stamp_of(&volume);
    std::fs::remove_dir_all(&part).expect("removes part");
    assert_eq!(
        crate::fingerprints::stamp_of(&volume),
        stamp,
        "the fixture: the volume's stamp stands"
    );

    scan_root(&conn, &kids).expect("walks it whole");
    crate::derive::derive(&conn).expect("derives again");

    let taken = digest_at(&conn, &kids, OWN).expect("the digest is taken again");
    assert_ne!(taken, digest, "the lost pages have left the digest");
    assert_eq!(
        taken,
        taiyaku_read::member_digest(&volume).expect("hashes the folder")
    );
    assert_eq!(member_pages_of(&conn, &taken), 1);
}

/// The first reading held is the one the works were written from: a digest
/// hashed since and never written must not take its place at a take-over.
#[test]
fn a_nested_directory_keeps_the_first_reading_a_row_it_takes_over_held() {
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-take-over-first-held");
    write_pages(&tree.path().join(SHORT_FOLDER), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, tree.path()).expect("adds the directory");
    scan_root(&conn, &root).expect("walks it");
    crate::derive::derive(&conn).expect("derives");
    assert!(
        digest_at(&conn, &root, SHORT_FOLDER).is_some(),
        "the fixture: a digest hashed since the held reading"
    );
    let held = (
        Some(planted('a')),
        Some("dir".to_string()),
        Some(7),
        Some("held title".to_string()),
    );
    assert_eq!(
        conn.execute(
            "UPDATE scan_entry
                SET mtime = mtime - 100,
                    held_sha256 = ?3, held_format = ?4, held_pages = ?5, held_title = ?6
              WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root.id, SHORT_FOLDER, held.0, held.1, held.2, held.3],
        ),
        Ok(1),
        "the fixture: the row changed since it held a reading"
    );
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;

    scan_root(&conn, &kids).expect("walks it");

    assert_eq!(digest_at(&conn, &kids, OWN), None, "the stamp moved");
    assert_eq!(held_reading(&conn, kids.id, OWN), held);
}

/// A folder with no digest has no count a digest was taken under to protect,
/// so the walk's total stands on it, short or not.
#[test]
fn a_short_walk_stores_its_total_on_a_folder_with_no_digest() {
    const OWN: &str = "S - v01";
    const TEST: &str = "a_short_walk_stores_its_total_on_a_folder_with_no_digest";
    let tree = crate::tests::tempdir("scan-short-walk-no-digest");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = walked_volume_with_a_part(&conn, tree.path());
    let part = root.path.join(SHORT_FOLDER).join("c001").join("part");

    let Some(_) = while_unlistable(&part, TEST, || {
        scan_root(&conn, &root).expect("walks what it can")
    }) else {
        return;
    };
    assert_eq!(count_at(&conn, root.id, SHORT_FOLDER).0, Some(1), "walked");

    scan_root(&conn, &root).expect("walks it whole");
    assert_eq!(count_at(&conn, root.id, SHORT_FOLDER).0, Some(3), "whole");
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;
    let Some(_) = while_unlistable(&part, TEST, || {
        scan_root(&conn, &kids).expect("walks what it can")
    }) else {
        return;
    };
    assert_eq!(count_at(&conn, kids.id, OWN).0, Some(1), "taken over");
}

/// The stored count is all the next whole walk has to compare with, so a short
/// walk's total stored in its place, higher as much as lower, could hide the
/// loss of the directory that walk could not list.
#[test]
fn a_short_walk_that_finds_more_pages_leaves_a_digested_folders_count_standing() {
    const OWN: &str = "S - v01";
    const TEST: &str =
        "a_short_walk_that_finds_more_pages_leaves_a_digested_folders_count_standing";
    let tree = crate::tests::tempdir("scan-short-walk-more");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = walked_volume_with_a_part(&conn, tree.path());
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let c001 = root.path.join(SHORT_FOLDER).join("c001");
    let part = c001.join("part");
    write_pages(&c001, 5);

    let Some(report) = while_unlistable(&part, TEST, || {
        scan_root(&conn, &root).expect("walks what it can")
    }) else {
        return;
    };
    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    assert_eq!(
        count_at(&conn, root.id, SHORT_FOLDER),
        (Some(3), Some(digest.clone()), None),
        "the walk found six and the digest was taken over three"
    );

    write_pages(&c001, 7);
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;
    let Some(report) = while_unlistable(&part, TEST, || {
        scan_root(&conn, &kids).expect("walks what it can")
    }) else {
        return;
    };
    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    assert_eq!(
        count_at(&conn, kids.id, OWN),
        (Some(3), Some(digest), None),
        "the walk found eight and the carried digest was taken over three"
    );
}

/// `walked` goes short of `part` with five more pages beside it in `c001`, and
/// `part` is then lost under a stamp of the volume's that stands; `None` where
/// this process lists a 0000 directory all the same.
fn short_walk_finding_more_then_part_lost(
    conn: &Connection,
    volume: &Path,
    walked: &Root,
    test: &str,
) -> Option<()> {
    let c001 = volume.join("c001");
    let part = c001.join("part");
    write_pages(&c001, 5);
    let report = while_unlistable(&part, test, || {
        scan_root(conn, walked).expect("walks what it can")
    })?;
    assert_eq!(
        report.unreadable_dirs.len(),
        1,
        "the fixture: part is not listed"
    );
    let stamp = crate::fingerprints::stamp_of(volume);
    std::fs::remove_dir_all(&part).expect("removes part");
    assert_eq!(
        crate::fingerprints::stamp_of(volume),
        stamp,
        "the fixture: the volume's stamp stands"
    );
    Some(())
}

/// With the short walk's higher total stored, the whole walk after the loss
/// would find the count it expects and leave a digest of pages that are gone.
#[test]
fn a_directory_lost_after_a_short_walk_found_more_pages_has_the_digest_taken_again() {
    let tree = crate::tests::tempdir("scan-short-walk-more-lost");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = walked_volume_with_a_part(&conn, tree.path());
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let volume = root.path.join(SHORT_FOLDER);
    let Some(()) = short_walk_finding_more_then_part_lost(
        &conn,
        &volume,
        &root,
        "a_directory_lost_after_a_short_walk_found_more_pages_has_the_digest_taken_again",
    ) else {
        return;
    };

    scan_root(&conn, &root).expect("walks it whole");
    crate::derive::derive(&conn).expect("derives again");

    let taken = digest_at(&conn, &root, SHORT_FOLDER).expect("the digest is taken again");
    assert_ne!(taken, digest, "the lost pages have left the digest");
    assert_eq!(
        taken,
        taiyaku_read::member_digest(&volume).expect("hashes the folder")
    );
    assert_eq!(member_pages_of(&conn, &taken), 6);
}

/// The same loss after a nested directory's first walk, which carries the row
/// the outer directory's walk left.
#[test]
fn a_directory_lost_after_a_nested_short_walk_found_more_pages_has_the_digest_taken_again() {
    const OWN: &str = "S - v01";
    let tree = crate::tests::tempdir("scan-short-nested-walk-more-lost");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = walked_volume_with_a_part(&conn, tree.path());
    crate::derive::derive(&conn).expect("derives");
    let digest = digest_at(&conn, &root, SHORT_FOLDER).expect("the volume's digest");
    let kids = add_root_checked_in(&conn, &root.path.join("S"), InLibrary::Named("Kids"))
        .expect("Kids is added inside the directory")
        .root;
    let volume = root.path.join(SHORT_FOLDER);
    let Some(()) = short_walk_finding_more_then_part_lost(
        &conn,
        &volume,
        &kids,
        "a_directory_lost_after_a_nested_short_walk_found_more_pages_has_the_digest_taken_again",
    ) else {
        return;
    };

    scan_root(&conn, &kids).expect("walks it whole");
    crate::derive::derive(&conn).expect("derives again");

    let taken = digest_at(&conn, &kids, OWN).expect("the digest is taken again");
    assert_ne!(taken, digest, "the lost pages have left the digest");
    assert_eq!(
        taken,
        taiyaku_read::member_digest(&volume).expect("hashes the folder")
    );
    assert_eq!(member_pages_of(&conn, &taken), 6);
}

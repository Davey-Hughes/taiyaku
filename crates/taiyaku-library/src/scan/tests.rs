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
/// read as a page (`jxl`, say) opens cleanly and still has no page count,
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
    // `unchanged` compares — and why O56 exists: a directory's stamp does not
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

    let (measured, walk) =
        measure(dir.path(), 0, &Excluder::build(None), &HashSet::new()).expect("measures the tree");
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
    let refused = super::add_root_checked_in(&conn, &nested, InLibrary::Id(outer.root.library_id))
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
    let library = crate::libraries::name_of(&conn, held.root.library_id)
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
        super::add_root_checked_in(&conn, dir.path(), InLibrary::Id(held.root.library_id))
            .map(|added| added.root),
        Err(AddRootError::Unusable(said)),
        "into the library that holds it"
    );
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

        super::add_root_checked_in(&conn, second, InLibrary::Id(held.root.library_id))
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
                .library_id;
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
                _ => create_library(&conn, "Comics", std::slice::from_ref(second))
                    .map(|made| made.dirs[0].root.clone()),
            }
            .unwrap_or_else(|why| panic!("{way}, inner first {inner_first}: {why}"));

            assert_eq!(added.path, second.canonicalize().unwrap(), "{way}");
            assert_ne!(added.library_id, held.root.library_id, "{way}");
        }
    }
}

/// A directory another library holds is refused by that library's name,
/// whichever library it was sent to: `root.path`'s UNIQUE would otherwise
/// answer it as a store fault.
#[test]
fn a_dir_another_library_holds_is_refused_by_that_librarys_name() {
    let parent = crate::tests::tempdir("scan-add-held-elsewhere");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root_checked_in(&conn, &a, InLibrary::Named("Manga")).expect("Manga holds a");
    let kids = add_root_checked_in(&conn, &b, InLibrary::Named("Kids"))
        .expect("Kids holds b")
        .root
        .library_id;
    let said = format!(
        "{} is already a directory of the library `Manga`",
        a.canonicalize().unwrap().display()
    );

    for into in [
        InLibrary::Id(kids),
        InLibrary::Named("Kids"),
        InLibrary::Named("New"),
    ] {
        assert_eq!(
            add_root_checked_in(&conn, &a, into).map(|added| added.root),
            Err(AddRootError::Unusable(said.clone())),
            "{into:?}"
        );
    }
    assert_eq!(
        create_library(&conn, "New", std::slice::from_ref(&a)).map(|made| made.library_id),
        Err(AddRootError::Unusable(said))
    );
    assert_eq!(libraries_and_roots(&conn), (2, 2));
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
        .library_id;

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
/// this function's own doc comment) has even started.
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
/// row's column default — while the stored row itself still carries the
/// real interval.
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

// --- the library a root is filed under -------------------------------------

/// The name of the library `root` is filed under.
fn library_of(conn: &Connection, root: &Root) -> String {
    conn.query_row(
        "SELECT name FROM library WHERE id = ?1",
        [root.library_id],
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
        first.library_id, second.library_id,
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
            .map(|r| r.library_id)
            .collect::<Vec<_>>(),
        vec![first.library_id, second.library_id],
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
        first.library_id, second.library_id,
        "the second directory joined, by trimmed name"
    );
    assert_ne!(third.library_id, first.library_id);
    assert_eq!(library_of(&conn, &first), "Manga");
}

/// A directory's name can begin or end with spaces, and a library's name made by
/// the trigger or `libraries::create` never does: the trigger strips them, as
/// `libraries::create` strips them from a name it is given. Otherwise
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
        joined.library_id, own.library_id,
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

    assert_eq!(joined.library_id, made.library_id);
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
    create_library(&conn, "Manga", std::slice::from_ref(&first)).expect("makes Manga");

    let why = create_library(&conn, "\tMANGA ", std::slice::from_ref(&second))
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
/// left to the trigger's insert to fail on the index as a store fault.
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

/// `root_gets_a_library` strips white space off the directory's last
/// component, so a directory called `. ` would make a library called `.`,
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

#[test]
fn a_refused_dir_names_no_library_into_being() {
    let dir = crate::tests::tempdir("scan-refused-library");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root(&conn, dir.path()).expect("the root");

    add_root_checked_in(&conn, dir.path(), InLibrary::Named("Never"))
        .expect_err("a directory already held is refused");

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

/// **`last_insert_rowid()` after `INSERT INTO root` must still be the
/// root's**, though a library row may have been inserted just before it
/// (`libraries::create`, for a new named library) or inside it (migration
/// 15's `root_gets_a_library`). Where every root is a library of its own, a
/// fresh database gives each root and its library the same id, and a test
/// cannot tell one from the other. Two directories filed under one library put
/// the ids out of step before the roots this test reads, and it reads them
/// back through `roots` too.
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
            added.id, added.library_id,
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
/// and only that one. The `root_gets_a_library` trigger tries the directory's
/// last component and then its whole path; with both taken its insert would
/// fail on `library.name`'s UNIQUE, which `add_root_checked_in` would report as
/// a store fault. Each directory here is checked against the libraries the ones
/// before it made.
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

/// `/` has no last component, so the trigger names its library by the whole
/// path alone, and only that name can clash.
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

/// `own_library_clash` refuses exactly the directories the trigger cannot file:
/// each path here is asked of both, the trigger by an insert rolled back
/// afterwards. The paths need not exist, since neither reads the disk. The
/// libraries are chosen so that comparing a name with its spaces, without its
/// tabs, or in its own case would answer differently from the trigger.
#[test]
fn own_library_clash_refuses_exactly_what_the_trigger_cannot_name() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for name in ["Manga", "/c/ Manga", "/d/", "/g/\tManga"] {
        crate::libraries::create(&conn, name).expect("a library");
    }
    let refused_by_trigger = |candidate: &str| -> bool {
        conn.execute_batch("SAVEPOINT trial").expect("savepoint");
        let refused = conn
            .execute("INSERT INTO root (path) VALUES (?1)", [candidate])
            .is_err();
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
        (candidate, clash.is_some(), refused_by_trigger(candidate))
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
        "(path, own_library_clash refuses, the trigger refuses)"
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
/// property that makes `take_root`'s three deletes and the `derive_with`
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
            root.id, root.library_id,
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
        "SELECT 'root', id, path, library_id FROM root ORDER BY id",
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
            libraries: vec![shelf_a.library_id],
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
            libraries: vec![shelf_a.library_id],
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
        solo.library_id < lone.library_id,
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
            libraries: vec![lone.library_id],
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
            libraries: vec![lone.library_id],
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
            libraries: vec![shelf_b.library_id],
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
        "SELECT 'root', id FROM root WHERE library_id = ?1",
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
               JOIN root r ON r.id = m.root_id JOIN library l ON l.id = r.library_id
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
            libraries: vec![solo.library_id],
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
    let kept = create_library(&conn, "Kept", &[parent.path().join("kept")]).expect("creates");
    let gone = create_library(&conn, "Gone", &[parent.path().join("gone")]).expect("creates");
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
    let kept = create_library(&conn, "Kept", &[dir.path().join("a")]).expect("creates");
    let doomed = create_library(&conn, "Doomed", &[dir.path().join("b")]).expect("creates");
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

/// A line that will not compile disables itself and leaves every other line
/// standing — `rules.rs`'s rule, for its reason: a scan of a real library
/// must not die over one bad pattern.
///
/// **`\` and `a{b` are the two shapes `ignore` 0.4.33 actually rejects.** `a[`,
/// `[a-`, `[[:bogus:]]` and `***` all compile, so this test names the two that
/// exist rather than a category that does not.
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

    let made = create_library(&conn, " Manga ", &[a.clone(), b.clone()]).expect("creates");

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
        .map(|r| (r.path, r.library_id))
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
        let why = create_library(&conn, "Manga", &paths).expect_err("refuses");
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
        create_library(&conn, "  ", std::slice::from_ref(&b)),
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
            create_library(&conn, name, &[dir.path().to_path_buf()]),
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
            create_library(&conn, name, &[dir.path().to_path_buf()]),
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

    let why = create_library(&conn, "Manga", &[a, b]).expect_err("refuses");

    assert!(
        matches!(&why, AddRootError::Failed(text) if text.contains("planted")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

#[test]
fn a_library_is_not_made_under_a_name_already_taken() {
    let parent = crate::tests::tempdir("create-library-taken");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root_in(&conn, &a, InLibrary::Named("Manga")).expect("Manga exists");

    let why = create_library(&conn, "Other", std::slice::from_ref(&a)).expect_err("refuses");
    assert!(
        matches!(&why, AddRootError::Unusable(text)
            if text.ends_with(" is already a directory of the library `Manga`")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (1, 1));

    for name in ["Manga", " Manga "] {
        let why = create_library(&conn, name, std::slice::from_ref(&b)).expect_err("refuses");
        assert!(
            matches!(&why, AddRootError::Unusable(text) if text.contains("already called `Manga`")),
            "{why:?}"
        );
    }
    assert_eq!(libraries_and_roots(&conn), (1, 1));
}

#[test]
fn a_dir_joins_a_library_by_its_id_and_no_library_by_an_id_none_has() {
    let parent = crate::tests::tempdir("add-by-id");
    let (a, b) = two_dirs(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let first = add_root_in(&conn, &a, InLibrary::Named("Manga")).expect("Manga exists");

    let why = add_root_checked_in(&conn, &b, InLibrary::Id(first.library_id + 1))
        .expect_err("no library has that id");
    assert!(
        matches!(&why, AddRootError::Unusable(text) if text.contains("there is no library")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (1, 1));

    let added = add_root_checked_in(&conn, &b, InLibrary::Id(first.library_id)).expect("joins");
    assert_eq!(added.root.library_id, first.library_id);
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
#[test]
fn mark_derive_failed_leaves_the_dir_that_took_a_removed_ones_id_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (stale, new, _walked, _other) =
        stale_root_and_the_dir_that_took_its_id(&conn, "stale-derive");

    super::mark_derive_failed(&conn, &stale).expect("a no-op is not a fault");
    let (status, _) = status_and_entries(&conn, new.id);
    assert_eq!(
        status, None,
        "the correction may not land on the new directory"
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
        crate::libraries::books(&conn, manga.library_id).expect("counts"),
        1
    );
    assert_eq!(
        crate::libraries::books(&conn, kids.library_id).expect("counts"),
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
/// recorded too are taken over; one whose file went before Kids' scan is gone;
/// and those Kids' own patterns exclude are forgotten by exclusion. Kids' rules
/// dropped the last two, so neither is counted as taken over.
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
        (3, 1, 2),
        "kids, kids/Bleach and v01 taken; v02 gone; extras and its file excluded: {report:?}"
    );
    assert_eq!(rows_under(&conn, &manga, "kids"), 0);
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
    let kids_only = account("k", kids.library_id);
    let manga_only = account("m", manga.library_id);
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
    /// Kept out by a pattern Kids added; Manga's walk reaches it again.
    Excluded,
    /// Kept out by Kids' pattern and by Manga's; no walk reaches it.
    ExcludedByBoth,
    /// Gone from the disk.
    Vanished,
}

/// Decision 21 for a file Kids recorded and later stopped recording, with a
/// reading position on its work, removing Kids' directory and Kids whole.
/// Where Manga's walk reaches the file, the rehearsal and the removal count
/// nothing, and Manga's next scan meets the same work, position kept; where
/// no walk does, its work and position are counted removed, as before.
#[test]
fn removing_a_nested_directory_hands_back_a_file_it_stopped_recording_where_the_outer_reaches_it() {
    for (case, whole_library) in [
        LaterOnKids::Excluded,
        LaterOnKids::ExcludedByBoth,
        LaterOnKids::Vanished,
    ]
    .into_iter()
    .flat_map(|case| [(case, false), (case, true)])
    {
        let (_top, library, manga) = manga_holding(
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
        if case == LaterOnKids::ExcludedByBoth {
            conn.execute(
                "UPDATE root SET exclude_globs = 'kids/extras/' WHERE id = ?1",
                [manga.id],
            )
            .expect("Manga excludes kids/extras/");
        }
        scan_root(&conn, &kids).expect("scans Kids again");
        crate::derive::derive(&conn).expect("derives");
        assert!(
            entries_for(&conn, &adult).is_empty(),
            "{case:?}: the fixture, no row holds the file"
        );
        assert_eq!(
            members_at(&conn, &adult).0,
            vec![Some(kids.id)],
            "{case:?}: the fixture, its member is still Kids'"
        );

        let (cost, removed) = if whole_library {
            (
                library_removal_cost(&conn, kids.library_id).expect("rehearses"),
                remove_library(&conn, kids.library_id).expect("removes Kids"),
            )
        } else {
            (
                removal_cost(&conn, kids.id).expect("rehearses"),
                remove_root(&conn, kids.id).expect("removes Kids"),
            )
        };
        if case == LaterOnKids::Excluded {
            assert_eq!(
                (counted(&cost), counted(&removed)),
                ((0, 0, 0), (0, 0, 0)),
                "whole_library={whole_library}"
            );
            assert_eq!(members_at(&conn, &adult).0, vec![Some(manga.id)]);
            scan_root(&conn, &manga).expect("scans Manga");
            crate::derive::derive(&conn).expect("derives");
            assert_eq!(
                work_of(&conn, &adult),
                work,
                "Manga's scan meets the same work"
            );
            assert_eq!(positions(), 1, "and its position");
        } else {
            assert_eq!(
                (counted(&cost), counted(&removed)),
                ((1, 1, 0), (1, 1, 0)),
                "{case:?} whole_library={whole_library}"
            );
            assert_eq!(positions(), 0, "{case:?}");
        }
    }
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
/// is copied with the digest Kids held; its members are Manga's; and the series
/// is Manga's `kids` again. Removing Kids' whole library does the same.
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
        {
            // The digests as copied, before a derive could hash the files again.
            let tx = crate::Nested::open(&conn, "hand_back").expect("opens a transaction");
            assert_eq!(
                hand_back(&tx, h.kids.id, &mut Carried::new()).expect("hands back"),
                4,
                "kids, kids/Bleach and its two files"
            );
            for (rel, digest) in bleach.iter().zip(&digests) {
                assert_eq!(
                    digest_at(&tx, &h.manga, &format!("kids/{rel}")),
                    *digest,
                    "{rel}"
                );
            }
        }

        let (cost, removed) = if whole_library {
            (
                library_removal_cost(&conn, h.kids.library_id),
                remove_library(&conn, h.kids.library_id),
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
    }
}

/// With Manga's `m` excluding `kids/`, Manga's walk would not reach Kids'
/// files, so the hand-back gives it none: removing Kids removes both Bleach
/// works, the position and the note, and the rehearsal counts what the
/// removal does.
#[test]
fn removing_a_nested_directory_the_outer_excludes_removes_its_works() {
    let h = handed_over("hand-back-excluded");
    let conn = h.library.conn();
    conn.execute(
        "UPDATE root SET exclude_globs = 'kids/' WHERE id = ?1",
        [h.manga.id],
    )
    .expect("excludes kids/ from Manga's directory");

    let cost = removal_cost(&conn, h.kids.id).expect("rehearses");
    let removed = remove_root(&conn, h.kids.id).expect("removes");
    assert_eq!((counted(&cost), counted(&removed)), ((2, 1, 1), (2, 1, 1)));
    assert_eq!(rows_under(&conn, &h.manga, "kids"), 0);
}

/// Spec §4: reach is per directory, so a file-level pattern at Manga's `m`
/// naming `kids/Bleach/v02.cbz` governs nothing while Kids holds the file, and
/// Manga's page shows both Bleach volumes. Handed back, the file meets the
/// pattern: removing Kids removes v02's work and keeps v01's, and the
/// rehearsal counts what the removal does.
#[test]
fn a_file_the_outer_excludes_shows_until_the_hand_back_removes_it() {
    let h = handed_over("hand-back-file-glob");
    let conn = h.library.conn();
    conn.execute(
        "UPDATE root SET exclude_globs = 'kids/Bleach/v02.cbz' WHERE id = ?1",
        [h.manga.id],
    )
    .expect("excludes v02 from Manga's directory");
    let page = crate::browse::series(
        &conn,
        &crate::account::Access::Every,
        1,
        Some(h.manga.library_id),
    )
    .expect("lists Manga's page");
    let bleach = page
        .iter()
        .find(|card| card.title == "Bleach")
        .expect("Manga's page shows Bleach");
    assert_eq!(bleach.works, 2, "v02 shows while Kids holds it");

    let cost = removal_cost(&conn, h.kids.id).expect("rehearses");
    let removed = remove_root(&conn, h.kids.id).expect("removes");
    assert_eq!((counted(&cost), counted(&removed)), ((1, 0, 0), (1, 0, 0)));
    assert_eq!(
        entries_for(&conn, &h.manga.path.join("kids/Bleach/v01.cbz")),
        [h.manga.id]
    );
    assert!(
        entries_for(&conn, &h.manga.path.join("kids/Bleach/v02.cbz")).is_empty(),
        "v02 is not handed back"
    );
    assert_eq!(
        work_of(&conn, &h.manga.path.join("kids/Bleach/v01.cbz")),
        h.v01
    );
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
        manga.id,
        &[],
        &held,
        &Excluder::build(None),
        &["kids".to_string()],
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

/// Removing Manga before Kids' first scan, with Kids' patterns excluding
/// `Bleach/v02.cbz`: Kids' walk would never reach v02, so the hand-back leaves
/// it on Manga and the rehearsal counts v02's work as removed with Naruto,
/// rather than Kids' first scan dropping it uncounted. v01 goes to Kids.
#[test]
fn removing_the_outer_leaves_it_what_the_nested_directorys_patterns_exclude() {
    let (_top, library, manga) = manga_holding("hand-back-inward-excluded", NEST_FILES);
    let conn = library.conn();
    let v01 = work_of(&conn, &manga.path.join("kids/Bleach/v01.cbz"));
    let kids = add_root_checked_in(&conn, &manga.path.join("kids"), InLibrary::Named("Kids"))
        .expect("adds Kids")
        .root;
    conn.execute(
        "UPDATE root SET exclude_globs = 'Bleach/v02.cbz' WHERE id = ?1",
        [kids.id],
    )
    .expect("excludes v02 from Kids' directory");

    let cost = removal_cost(&conn, manga.id).expect("rehearses");
    let removed = remove_root(&conn, manga.id).expect("removes");
    assert_eq!((counted(&cost), counted(&removed)), ((2, 0, 0), (2, 0, 0)));
    assert_eq!(
        entries_for(&conn, &manga.path.join("kids/Bleach/v01.cbz")),
        [kids.id]
    );
    assert!(
        entries_for(&conn, &manga.path.join("kids/Bleach/v02.cbz")).is_empty(),
        "v02 is not handed in"
    );
    assert_eq!(work_of(&conn, &manga.path.join("kids/Bleach/v01.cbz")), v01);
}

/// Manga's walk asks a directory-only pattern of directories alone, and so
/// does the hand-back. With `m` excluding `v02.cbz/` and `v03/`, removing Kids
/// hands back the file `v02.cbz` and not the volume directory `v03`, whose
/// work alone is removed.
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
    assert_eq!((counted(&cost), counted(&removed)), ((1, 0, 0), (1, 0, 0)));
    assert_eq!(
        entries_for(&conn, &h.manga.path.join("kids/Bleach/v02.cbz")),
        [h.manga.id],
        "a file is not a directory"
    );
    assert!(entries_for(&conn, &v03).is_empty(), "v03 is held back");
}

// --- spec §4: a work's hand document moves with its files ------------------

/// Manga's `m` holding [`NEST_FILES`] and keeping documents, with the reader on
/// Bleach v01 and its hand-set title saved at `m/.taiyaku`, and Kids added at
/// `m/kids` before its first scan, keeping documents where `kids_keeps` says.
struct Documented {
    _top: crate::tests::TempDir,
    library: Library,
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
            remove_library(&conn, d.kids.library_id)
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
        remove_library(&conn, kids.library_id)
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

    let (_, derived) = remove_library(&conn, ka.library_id).expect("removes Kids");

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
            remove_library(&conn, kids.library_id)
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

use super::*;

/// A member that yields no pages is almost always a mistake, and saying
/// nothing about it is the worst possible response. Pointing a directory of
/// archival TIFFs at the aligner reported `A: 0 pages` and carried on
/// cheerfully, because `.tif` was not in the recognised list — the files
/// were there, readable, and silently ignored. The error has to name what
/// it found instead.
#[test]
fn a_member_with_no_pages_says_what_it_found_instead() {
    let dir = std::env::temp_dir().join("taiyaku-test-nopages");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cover.psd"), b"not an image we read").unwrap();
    std::fs::write(dir.join("notes.txt"), b"hello").unwrap();

    let error = read_member(&dir).unwrap_err();

    let _ = std::fs::remove_dir_all(&dir);
    assert!(error.contains("no page images"), "unhelpful: {error}");
    assert!(
        error.contains("psd"),
        "should name what it skipped: {error}"
    );
}

/// The message someone actually sees. "invalid Zip archive: could not find
/// EOCD" tells a person holding a perfectly good 7-Zip file nothing they
/// can act on; naming the format tells them exactly what happened and that
/// converting it will work.
#[test]
fn reading_a_seven_zip_archive_says_so() {
    let path = std::env::temp_dir().join("taiyaku-test-container.cb7");
    std::fs::write(&path, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04]).unwrap();

    let error = read_member(&path).unwrap_err();

    let _ = std::fs::remove_file(&path);
    assert!(error.contains("7-Zip"), "unhelpful error: {error}");
}

/// A PDF is named rather than reported as a corrupt zip, the same way a
/// 7-Zip archive is. New with the scanner's sniffer: `unsupported_container`
/// recognised three formats before it and recognises four now.
#[test]
fn reading_a_pdf_says_so() {
    let path = std::env::temp_dir().join("taiyaku-test-container.pdf");
    std::fs::write(&path, b"%PDF-1.7\n").unwrap();

    let error = read_member(&path).unwrap_err();

    let _ = std::fs::remove_file(&path);
    assert!(error.contains("this is a PDF"), "{error}");
}

/// Extensions lie in both directions: a zip is often named `.cbr`, and a
/// real 7-Zip archive is often named `.cbz`. Reporting "invalid Zip
/// archive: could not find EOCD" to someone holding a perfectly good
/// 7-Zip file tells them nothing they can act on, so the leading bytes
/// decide what the error says.
#[test]
fn an_unreadable_container_is_named_by_its_leading_bytes() {
    let seven_zip = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04];
    let rar5 = b"Rar!\x1a\x07\x01\x00";
    let rar4 = b"Rar!\x1a\x07\x00";
    let mut tar = vec![0u8; 512];
    tar[257..262].copy_from_slice(b"ustar");
    let pdf = b"%PDF-1.7\n";
    let zip = [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00];

    assert_eq!(unsupported_container(&seven_zip), Some("a 7-Zip archive"));
    assert_eq!(unsupported_container(rar5), Some("a RAR archive"));
    assert_eq!(unsupported_container(rar4), Some("a RAR archive"));
    assert_eq!(unsupported_container(&tar), Some("a tar archive"));
    assert_eq!(unsupported_container(pdf), Some("a PDF"));
    // Zip is supported, so it is not something to complain about.
    assert_eq!(unsupported_container(&zip), None);
}

/// The signature sits at bytes 257..262, so 262 bytes is exactly enough to
/// read it. The old guard asked for 263 and called a 262-byte buffer
/// unrecognised; `get` asks for what it reads.
///
/// No real tar is this short — one header block is 512 bytes, which is why
/// this was never felt — but a bound that is one out is worth removing
/// rather than pinning, and a slice guard written as arithmetic is a thing
/// a test has to keep honest.
#[test]
fn the_tar_signature_is_read_from_the_shortest_buffer_that_holds_it() {
    let mut exact = vec![0u8; 262];
    exact[257..262].copy_from_slice(b"ustar");
    assert_eq!(unsupported_container(&exact), Some("a tar archive"));

    let mut one_short = vec![0u8; 261];
    one_short[256..261].copy_from_slice(b"ustar");
    assert_eq!(
        unsupported_container(&one_short),
        None,
        "too short to hold the signature at its offset, whatever it contains"
    );
}

/// A page prints as its name and size. Deriving `Debug` would dump several
/// megabytes of JPEG into any failure that touches one, which is why it is
/// hand-written — and a hand-written impl is code, so it can be wrong.
#[test]
fn a_page_prints_as_its_name_and_size_rather_than_its_bytes() {
    let page = Page {
        name: "p1.jpg".into(),
        bytes: vec![0xFF, 0xD8, 0xFF, 0xE0],
    };
    assert_eq!(format!("{page:?}"), "Page(p1.jpg, 4 bytes)");
}

/// Two independent reasons to skip an entry, and they must stay
/// independent. A resource fork is `__MACOSX/…` whether or not its
/// basename begins with a dot, and a dotfile is a dotfile wherever it
/// sits — so the test needs one of each that the *other* rule would keep.
///
/// `__MACOSX/p1.jpg` is the discriminating one: the usual resource fork is
/// `__MACOSX/._p1.jpg`, which both rules reject, and an archive holding
/// only those cannot tell the two conditions apart.
#[test]
fn a_resource_fork_and_a_dotfile_are_each_skipped_on_their_own_account() {
    let path = cbz(
        "taiyaku-test-skips.cbz",
        &[
            "__MACOSX/p1.jpg",
            ".hidden.jpg",
            "__MACOSX/._p2.jpg",
            "p1.jpg",
            "p2.jpg",
        ],
    );

    let pages = read_member(&path).unwrap();

    let _ = std::fs::remove_file(&path);
    let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["p1.jpg", "p2.jpg"],
        "a fork with an ordinary basename and a dotfile outside a fork are both skipped"
    );
}

/// The digest is what tells a reader the archive was replaced, so a stub
/// returning a constant would report every member as unchanged forever.
/// Pinned against a hash computed outside this crate rather than against
/// `Sha256` called a second time here, which would agree with any bug both
/// copies shared.
///
/// A directory hashes its pages' bytes in order, not the directory itself,
/// so the same bytes split across two files must not collide with them
/// concatenated in one.
#[test]
fn a_members_digest_is_the_hash_of_its_content() {
    let file = std::env::temp_dir().join("taiyaku-test-digest.bin");
    std::fs::write(&file, b"taiyaku page").unwrap();
    let got = member_digest(&file).unwrap();
    let _ = std::fs::remove_file(&file);
    assert_eq!(
        got, "e7a8b30f8c6591c8e3341e1f6b0b4f6658cfe5b09b048dafabeaf9ef113622bd",
        "sha256 of the file's bytes"
    );

    let dir = std::env::temp_dir().join("taiyaku-test-digest-dir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("1.jpg"), b"taiyaku ").unwrap();
    std::fs::write(dir.join("2.jpg"), b"pageone").unwrap();
    let got = member_digest(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        got, "ce663f7c43b7eb8989b012dfa7c8183eb4e052b8ac0f178b87132354b02b8d72",
        "sha256 over the pages' bytes in reading order, joined"
    );
}

/// Write a cbz whose entries are stored in the order given.
fn cbz(name: &str, entries: &[&str]) -> PathBuf {
    use std::io::Write;
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for entry in entries {
        zip.start_file(*entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(entry.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// Two rules a reader of the sidecar has to implement identically or every
/// lookup is wrong, so they are guarantees of the format rather than
/// details of this reader — see `docs/sidecar.md`, "How a member becomes
/// pages".
///
/// Central-directory order is whatever the packer happened to write: 32 of
/// the corpus's 57 cbz archives store their entries in an order that is not
/// the name order, so a reader iterating entries as stored gets a different
/// page sequence for over half of them. And 12 of the 57 carry a
/// `ComicInfo.xml`, which is not a page and must not shift the numbering.
#[test]
fn entries_are_pages_in_name_order_whatever_order_they_were_stored_in() {
    let path = cbz(
        "taiyaku-test-order.cbz",
        &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
    );

    let pages = read_member(&path).unwrap();

    let _ = std::fs::remove_file(&path);
    let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["p1.jpg", "p2.jpg", "p10.jpg"]);
}

/// Sorting on the basename rather than the whole path scatters a
/// chapter-foldered edition beyond recovery, and the corpus holds the case:
/// Go-Toubun's Full Colour edition nests pages under `Chapter 15 - …/` and
/// carries a `ZBlank (1).jpg` in each folder. On the basename every
/// chapter's blank sorts to the front and the chapters interleave; 2 of the
/// 7 nested archives measured come out differently the two ways.
#[test]
fn nested_pages_sort_on_the_whole_path_not_the_basename() {
    let path = cbz(
        "taiyaku-test-nested.cbz",
        &[
            "Chapter 2/001.jpg",
            "Chapter 1/ZBlank (1).jpg",
            "Chapter 1/001.jpg",
        ],
    );

    let pages = read_member(&path).unwrap();

    let _ = std::fs::remove_file(&path);
    let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Chapter 1/001.jpg",
            "Chapter 1/ZBlank (1).jpg",
            "Chapter 2/001.jpg"
        ],
        "chapter 1 must finish before chapter 2 begins"
    );
}

/// What `serve` does on every page request. Reading the whole edition to
/// answer "give me page 47" is hundreds of megabytes for one JPEG, so the
/// index has to be addressable directly — and it has to land on the same
/// page `read_member` would give at that position, or the server and the
/// aligner disagree about what page 47 is.
#[test]
fn a_page_reads_by_index_in_the_order_read_member_gives() {
    let path = cbz(
        "taiyaku-test-byindex.cbz",
        &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
    );

    let names = member_page_names(&path).unwrap();
    let second = member_page(&path, 1).unwrap();
    let whole = read_member(&path).unwrap();

    let _ = std::fs::remove_file(&path);
    assert_eq!(names, ["p1.jpg", "p2.jpg", "p10.jpg"]);
    assert_eq!(second.name, "p2.jpg");
    assert_eq!(second.bytes, b"p2.jpg");
    assert_eq!(
        names,
        whole.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "names must agree with what read_member sequences"
    );
}

/// `docs/sidecar.md` rule 1: a sidecar's `members` array *is* the page
/// order and must not be re-sorted, because its paths are relative to the
/// sidecar and may point into unrelated trees. `read_edition` sorts — it is
/// handed shell globs, which are not in reading order — so a caller holding
/// a sidecar needs the other one, and the two must be demonstrably
/// different or the second is just a renamed call.
///
/// The seam check in `taiyaku-serve` made exactly this mistake: it rebuilt
/// a sidecar's editions with `read_edition`, so on any work whose array
/// order is not its natural order it would have reported a disagreement
/// with neither side wrong.
#[test]
fn a_sidecars_member_order_is_the_page_order_and_is_not_re_sorted() {
    // Named so that natural order is the reverse of the order given.
    let b = cbz("taiyaku-test-given-b.cbz", &["b1.jpg"]);
    let a = cbz("taiyaku-test-given-a.cbz", &["a1.jpg"]);
    let as_given = vec![b.clone(), a.clone()];

    let in_order = read_members_in_order(&as_given).unwrap();
    let sorted = read_edition(&as_given).unwrap();

    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    assert_eq!(
        in_order.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["b1.jpg", "a1.jpg"],
        "the array order is the page order"
    );
    assert_eq!(
        sorted.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["a1.jpg", "b1.jpg"],
        "read_edition sorts, which is right for a glob and wrong for a sidecar"
    );
}

/// `member_pages` is the path `align` takes to record a member's page
/// count, and it lists rather than reads. Listing used to go straight to
/// `ZipArchive::new`, so a RAR file — the single most common thing to find
/// behind a `.cbr`, or behind a `.cbz` that lies — came back as "not a
/// readable cbz" while reading the same file said "this is a RAR archive".
/// Same file, same tool, two answers, and the useful one was on the path
/// nobody took.
#[test]
fn counting_a_rar_archives_pages_names_the_format() {
    let path = std::env::temp_dir().join("taiyaku-test-count-container.cbz");
    std::fs::write(&path, b"Rar!\x1a\x07\x01\x00").unwrap();

    let counted = member_pages(&path).unwrap_err();
    let listed = member_page_names(&path).unwrap_err();
    let addressed = member_page(&path, 0).unwrap_err();

    let _ = std::fs::remove_file(&path);
    for error in [&counted, &listed, &addressed] {
        assert!(error.contains("RAR"), "unhelpful error: {error}");
    }
}

/// Every ordering test above builds a cbz; the directory branches of
/// `member_page_names` and `member_page` had no direct coverage of their
/// own, including `strip_prefix`, which turns an absolute path into a
/// member-relative name and is new code rather than a move of Task 2's.
/// Without this test, `strip_prefix` regressing to leave the temp-dir
/// prefix in page names, or a nested directory sorting on the wrong key,
/// would still pass every other test in this file.
#[test]
fn a_directory_page_reads_by_index_in_the_order_read_member_gives() {
    let dir = std::env::temp_dir().join("taiyaku-test-dir-byindex");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Chapter 1")).unwrap();
    std::fs::create_dir_all(dir.join("Chapter 2")).unwrap();
    std::fs::write(dir.join("Chapter 2/001.jpg"), b"Chapter 2/001.jpg").unwrap();
    std::fs::write(
        dir.join("Chapter 1/ZBlank (1).jpg"),
        b"Chapter 1/ZBlank (1).jpg",
    )
    .unwrap();
    std::fs::write(dir.join("Chapter 1/001.jpg"), b"Chapter 1/001.jpg").unwrap();

    let names = member_page_names(&dir).unwrap();
    let second = member_page(&dir, 1).unwrap();
    let whole = read_member(&dir).unwrap();

    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        names,
        [
            "Chapter 1/001.jpg",
            "Chapter 1/ZBlank (1).jpg",
            "Chapter 2/001.jpg"
        ],
        "chapter 1 must finish before chapter 2 begins, and names must be member-relative"
    );
    assert_eq!(
        second.name, "Chapter 1/ZBlank (1).jpg",
        "a directory page's name must be member-relative too"
    );
    assert_eq!(second.bytes, b"Chapter 1/ZBlank (1).jpg");
    assert_eq!(
        names,
        whole.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        "names must agree with what read_member sequences"
    );
}

/// The signature a duplicate set is settled by is a list of page sizes, so
/// this has to agree with `read_edition` on *which* entries are pages, on
/// what order they come in, and on how big each one is. Uncompressed, not
/// as stored: an archive and a directory holding one scan must produce the
/// same list, which is the whole point of signing pages rather than files.
#[test]
fn page_sizes_are_uncompressed_in_reading_order_and_skip_non_pages() {
    let path = cbz(
        "taiyaku-test-sizes.cbz",
        &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
    );

    let pages = read_member(&path).unwrap();
    let sizes = member_page_sizes(&path).unwrap();

    let _ = std::fs::remove_file(&path);
    let expected: Vec<u64> = pages.iter().map(|p| p.bytes.len() as u64).collect();
    assert_eq!(sizes, expected, "sizes must match the bytes actually read");
    assert_eq!(sizes.len(), 3, "ComicInfo.xml is not a page");
}

/// The property the duplicate signature rests on: repackaging a scan from
/// loose images into a cbz must not change what it signs as.
#[test]
fn a_directory_and_an_archive_of_the_same_pages_size_identically() {
    let dir = std::env::temp_dir().join("taiyaku-test-sizes-dir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
        std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
    }
    // Stored in a different order, as a repacker would.
    let archive = cbz(
        "taiyaku-test-sizes-arc.cbz",
        &["p10.jpg", "p2.jpg", "p1.jpg"],
    );

    let from_dir = member_page_sizes(&dir).unwrap();
    let from_archive = member_page_sizes(&archive).unwrap();

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&archive);
    assert_eq!(from_dir, from_archive);
    assert_eq!(from_dir, vec![6, 6, 7], "p1.jpg, p2.jpg, p10.jpg by length");
}

/// Build a cbz and patch its first entry's central-directory
/// uncompressed-size field to an outrageous lie — the same thing a
/// hostile or corrupt `.cbz` would carry — rather than reimplementing
/// the read path against a mock. Shared by both capacity-ceiling tests
/// below, so `read_archive`'s and `member_page`'s identical clamp are
/// proven against identically-constructed evidence.
///
/// The central directory's uncompressed-size field sits at offset 24 of
/// the 46-byte fixed header that follows the `PK\x01\x02` signature
/// (APPNOTE.TXT §4.3.12). Patched, not the local file header, because
/// `entry.size()` reads the central directory — patching only that one
/// leaves the entry's actual stored bytes untouched, so a correct read
/// is still possible despite the lie.
fn lying_size_cbz(name: &str, entries: &[&str]) -> PathBuf {
    let path = cbz(name, entries);
    let mut raw = std::fs::read(&path).unwrap();
    let central_directory = raw
        .windows(4)
        .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
        .expect("a central directory file header");
    let size_field = central_directory + 24;
    // Same declared size as the probe docs/notes.md records: ~2 GiB for
    // an entry whose real content is a handful of bytes.
    raw[size_field..size_field + 4].copy_from_slice(&0x7FFF_FFF0u32.to_le_bytes());
    std::fs::write(&path, &raw).unwrap();
    path
}

/// A literal, not `MAX_ENTRY_CAPACITY_HINT` itself. An earlier version of
/// this test asserted `capacity() <= MAX_ENTRY_CAPACITY_HINT`, which
/// passes for *any* ceiling at or above the declared lie — raise the
/// constant to 4 GiB and the assertion still goes green, having proved
/// nothing about whether the clamp does anything useful. This bound sits
/// comfortably above today's 32 MiB ceiling (allocator rounding, and
/// room to retune the ceiling within its own order of magnitude without
/// editing this test) and 30-fold below the ~2 GiB lie, so it still
/// catches a regression that quietly enlarges the ceiling or drops the
/// clamp outright.
///
/// Verified against the failure it exists to catch: with `.min(...)`
/// deleted from the call site, both tests below failed with
/// `capacity() == 2147483632` (the declared lie itself), before the
/// clamp was restored.
const RESERVATION_CEILING: usize = 64 * 1024 * 1024;

/// The path `read_archive` takes: every page in the member, collected at
/// once. See `MAX_ENTRY_CAPACITY_HINT`'s doc comment for why that makes
/// the per-entry ceiling matter in aggregate too.
#[test]
fn a_declared_size_far_past_reality_does_not_reserve_past_the_ceiling() {
    let path = lying_size_cbz("taiyaku-test-lying-size.cbz", &["p1.jpg"]);

    let pages = read_member(&path).unwrap();

    let _ = std::fs::remove_file(&path);
    assert_eq!(pages.len(), 1);
    assert_eq!(
        pages[0].bytes, b"p1.jpg",
        "the real bytes must still be read correctly despite the lie"
    );
    assert!(
        pages[0].bytes.capacity() < RESERVATION_CEILING,
        "a declared ~2 GiB must not reserve anywhere near it; got {} bytes reserved",
        pages[0].bytes.capacity()
    );
}

/// The path `taiyaku-serve` actually calls on every page request: one
/// entry, addressed by index, never the whole archive. `read_archive`
/// and `member_page` carry the identical
/// `entry.size().min(MAX_ENTRY_CAPACITY_HINT)` clamp, but until this
/// test only the `read_archive` side had a regression test — a change
/// that broke the clamp on `member_page` alone (a copy-paste that
/// dropped the `.min()`, say) would have shipped with the suite green,
/// on the one path a server request actually reaches.
#[test]
fn a_lying_member_page_does_not_reserve_past_the_ceiling_either() {
    let path = lying_size_cbz("taiyaku-test-lying-size-member-page.cbz", &["p1.jpg"]);

    let page = member_page(&path, 0).unwrap();

    let _ = std::fs::remove_file(&path);
    assert_eq!(
        page.bytes, b"p1.jpg",
        "the real bytes must still be read correctly despite the lie"
    );
    assert!(
        page.bytes.capacity() < RESERVATION_CEILING,
        "a declared ~2 GiB must not reserve anywhere near it; got {} bytes reserved",
        page.bytes.capacity()
    );
}

/// `member_entry` is what `taiyaku-cli::credits` reads `ComicInfo.xml`
/// through, so this pins the one property that function relies on: it
/// finds a non-page entry `page_names_from` would have filtered out, and
/// reads back the bytes of that entry specifically rather than whichever
/// happened to be first.
#[test]
fn member_entry_finds_a_named_archive_entry_among_pages_and_reads_its_own_bytes() {
    let path = cbz(
        "taiyaku-test-member-entry.cbz",
        &["p1.jpg", "ComicInfo.xml", "p2.jpg"],
    );

    let found = member_entry(&path, "ComicInfo.xml").unwrap();
    let missing = member_entry(&path, "Nonexistent.xml").unwrap();

    let _ = std::fs::remove_file(&path);
    assert_eq!(
        found,
        Some(b"ComicInfo.xml".to_vec()),
        "`cbz`'s helper writes each entry's own name as its bytes, so this \
             also proves the *matched* entry was read rather than any other"
    );
    assert_eq!(
        missing, None,
        "an entry that is not there is absent, not an error"
    );
}

/// A scanning tool's own case convention for the file name is not
/// anything this crate controls, so the match must not depend on it.
#[test]
fn member_entry_matches_an_archive_entrys_name_case_insensitively() {
    let path = cbz("taiyaku-test-member-entry-case.cbz", &["comicinfo.xml"]);

    let found = member_entry(&path, "ComicInfo.xml").unwrap();

    let _ = std::fs::remove_file(&path);
    assert_eq!(found, Some(b"comicinfo.xml".to_vec()));
}

/// The directory branch of `member_entry` — a fan edition stored as loose
/// files rather than a `.cbz` — must find the same named entry the
/// archive branch does.
#[test]
fn member_entry_reads_a_directory_members_own_file() {
    let dir = std::env::temp_dir().join("taiyaku-test-member-entry-dir");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("001.jpg"), b"page").unwrap();
    std::fs::write(dir.join("ComicInfo.xml"), b"<ComicInfo/>").unwrap();

    let found = member_entry(&dir, "ComicInfo.xml").unwrap();
    let missing = member_entry(&dir, "Nonexistent.xml").unwrap();

    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found, Some(b"<ComicInfo/>".to_vec()));
    assert_eq!(missing, None);
}

/// Deliberately not a recursive walk: unlike `directory_page_paths`,
/// which descends into a chapter-foldered edition to find images,
/// `member_entry` only looks beside the pages at the member's own level —
/// every `ComicInfo.xml` this crate has read sits there, never nested.
#[test]
fn member_entry_does_not_descend_into_a_directorys_subdirectories() {
    let dir = std::env::temp_dir().join("taiyaku-test-member-entry-nested");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Chapter 1")).unwrap();
    std::fs::write(dir.join("Chapter 1").join("ComicInfo.xml"), b"nested").unwrap();

    let found = member_entry(&dir, "ComicInfo.xml").unwrap();

    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        found, None,
        "a nested ComicInfo.xml is not this member's own"
    );
}

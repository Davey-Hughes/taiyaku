use super::*;
use crate::test_support::TempDir;

/// A member that yields no pages is almost always a mistake, and saying
/// nothing about it is the worst possible response. The error has to name
/// what it found instead.
#[test]
fn a_member_with_no_pages_says_what_it_found_instead() {
    let dir = TempDir::new("nopages");
    std::fs::write(dir.join("cover.psd"), b"not an image we read").unwrap();
    std::fs::write(dir.join("notes.txt"), b"hello").unwrap();

    let error = read_member(&dir).unwrap_err();
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
    let dir = TempDir::new("container");
    let path = dir.join("container.cb7");
    std::fs::write(&path, [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04]).unwrap();

    let error = read_member(&path).unwrap_err();
    assert!(error.contains("7-Zip"), "unhelpful error: {error}");
}

/// A PDF is named rather than reported as a corrupt zip, the same way a
/// 7-Zip archive is.
#[test]
fn reading_a_pdf_says_so() {
    let dir = TempDir::new("container");
    let path = dir.join("container.pdf");
    std::fs::write(&path, b"%PDF-1.7\n").unwrap();

    let error = read_member(&path).unwrap_err();
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
/// read it. A guard asking for 263 would call a 262-byte buffer unrecognised;
/// `get` asks for what it reads.
///
/// No real tar is this short — one header block is 512 bytes — but a bound
/// that is one out is worth removing rather than pinning, and a slice guard
/// written as arithmetic is a thing a test has to keep honest.
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
    let dir = TempDir::new("skips");
    let path = cbz(
        &dir,
        "skips.cbz",
        &[
            "__MACOSX/p1.jpg",
            ".hidden.jpg",
            "__MACOSX/._p2.jpg",
            "p1.jpg",
            "p2.jpg",
        ],
    );

    let pages = read_member(&path).unwrap();
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
    let scratch = TempDir::new("digest");
    let file = scratch.join("digest.bin");
    std::fs::write(&file, b"taiyaku page").unwrap();
    let got = member_digest(&file).unwrap();
    assert_eq!(
        got, "e7a8b30f8c6591c8e3341e1f6b0b4f6658cfe5b09b048dafabeaf9ef113622bd",
        "sha256 of the file's bytes"
    );

    let dir = TempDir::new("digest-dir");
    std::fs::write(dir.join("1.jpg"), b"taiyaku ").unwrap();
    std::fs::write(dir.join("2.jpg"), b"pageone").unwrap();
    let got = member_digest(&dir).unwrap();
    assert_eq!(
        got, "ce663f7c43b7eb8989b012dfa7c8183eb4e052b8ac0f178b87132354b02b8d72",
        "sha256 over the pages' bytes in reading order, joined"
    );
}

/// Write a cbz whose entries are stored in the order given.
fn cbz(dir: &Path, name: &str, entries: &[&str]) -> PathBuf {
    use std::io::Write;
    let path = dir.join(name);
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
/// Central-directory order is whatever the packer happened to write, so a
/// reader iterating entries as stored can get a different page sequence from
/// one that sorts by name. And a `ComicInfo.xml` is not a page and must not
/// shift the numbering.
#[test]
fn entries_are_pages_in_name_order_whatever_order_they_were_stored_in() {
    let dir = TempDir::new("order");
    let path = cbz(
        &dir,
        "order.cbz",
        &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
    );

    let pages = read_member(&path).unwrap();
    let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["p1.jpg", "p2.jpg", "p10.jpg"]);
}

/// Sorting on the basename rather than the whole path scatters an edition
/// with a directory per chapter beyond recovery, and the corpus holds the case:
/// Go-Toubun's Full Colour edition nests pages under `Chapter 15 - …/` and
/// carries a `ZBlank (1).jpg` in each directory. On the basename every
/// chapter's blank sorts to the front and the chapters interleave.
#[test]
fn nested_pages_sort_on_the_whole_path_not_the_basename() {
    let dir = TempDir::new("nested");
    let path = cbz(
        &dir,
        "nested.cbz",
        &[
            "Chapter 2/001.jpg",
            "Chapter 1/ZBlank (1).jpg",
            "Chapter 1/001.jpg",
        ],
    );

    let pages = read_member(&path).unwrap();
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

/// Archive page order is the natural order of the decoded names, so a name
/// has to come back as the one the packer wrote. A valid UTF-8 name whose
/// entry leaves the UTF-8 flag clear is still that name; read through
/// codepage 437 it would sort as mojibake.
#[test]
fn a_utf8_page_name_without_the_utf8_flag_lists_under_its_real_name() {
    let dir = TempDir::new("unflagged");
    let path = cbz(&dir, "unflagged.cbz", &["カラー版.jpg"]);
    clear_utf8_flag(&path);

    let pages = read_member(&path).unwrap();
    let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["カラー版.jpg"]);
}

/// A page whose name is not UTF-8 still decodes, through codepage 437. If
/// decoding could fail, the page would drop out of the sequence and every
/// page after it would move to the index before.
#[test]
fn a_codepage_437_page_name_lists_in_its_sort_position() {
    let dir = TempDir::new("cp437");
    let path = cp437_cbz(&dir);

    let pages = read_member(&path).unwrap();
    let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["001.jpg", "002 été.jpg", "003.jpg"]);
    assert_eq!(
        pages[1].bytes, b"two",
        "the decoded name reads back its own entry"
    );
}

/// Write a three-page cbz whose middle page is named in codepage 437
/// bytes, `002 été.jpg` once decoded, with the UTF-8 flag clear.
fn cp437_cbz(dir: &Path) -> PathBuf {
    use std::io::Write as _;
    const CP437_NAME: &[u8] = b"002 \x82t\x82.jpg";
    const PLACEHOLDER: &str = "002 _t_.jpg";
    let path = dir.join("cp437.cbz");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for (name, bytes) in [
        ("001.jpg", b"one"),
        (PLACEHOLDER, b"two"),
        ("003.jpg", b"tri"),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let mut raw = std::fs::read(&path).unwrap();
    let sites: Vec<usize> = raw
        .windows(PLACEHOLDER.len())
        .enumerate()
        .filter(|(_, window)| *window == PLACEHOLDER.as_bytes())
        .map(|(at, _)| at)
        .collect();
    assert_eq!(sites.len(), 2, "the local header and the central directory");
    for at in sites {
        raw[at..at + CP437_NAME.len()].copy_from_slice(CP437_NAME);
    }
    std::fs::write(&path, &raw).unwrap();
    path
}

/// Clear the UTF-8 flag, general purpose bit 11, in a one-entry archive's
/// local header and central directory header. `ZipWriter` sets it for every
/// non-ASCII name and has no option to leave it clear. The flag word sits at
/// offset 6 of the local header (APPNOTE.TXT §4.3.7) and offset 8 of the
/// central directory header (§4.3.12).
fn clear_utf8_flag(path: &Path) {
    const UTF8: u16 = 1 << 11;
    let mut raw = std::fs::read(path).unwrap();
    for (signature, offset) in [([0x50, 0x4B, 0x03, 0x04], 6), ([0x50, 0x4B, 0x01, 0x02], 8)] {
        let field = raw
            .windows(4)
            .position(|w| w == signature)
            .expect("the header")
            + offset;
        let flags = u16::from_le_bytes([raw[field], raw[field + 1]]);
        // Without this the patch could be a no-op and the test would prove
        // nothing about an unflagged name.
        assert_ne!(flags & UTF8, 0, "the writer set the flag this clears");
        raw[field..field + 2].copy_from_slice(&(flags & !UTF8).to_le_bytes());
    }
    std::fs::write(path, &raw).unwrap();
}

/// **A scan's walk reads an archive the same for four hundred pages as for
/// four**: sniffing its format and listing its pages each read only the
/// central directory, through one buffer. A walk that opened each entry's
/// local header, or parsed the directory unbuffered, reads once or more per
/// page, a round trip each over a network share.
///
/// The test runs itself again as a child under `strace`, as O104's tests do,
/// sniffing and listing each archive, and counts the reads on its own path.
#[cfg(target_os = "linux")]
#[test]
fn listing_an_archive_reads_no_more_for_four_hundred_pages_than_for_four() {
    const CHILD: &str = "TAIYAKU_TEST_PAGES_LISTING_READS_CHILD";
    const COUNTS: [u32; 2] = [4, 400];
    if let Some(dir) = std::env::var_os(CHILD) {
        for count in COUNTS {
            let path = Path::new(&dir).join(format!("{count}.cbz"));
            assert_eq!(
                crate::sniff(&path),
                Ok(crate::Format::Zip),
                "{}",
                path.display()
            );
            assert_eq!(member_pages(&path), Ok(count), "{}", path.display());
        }
        return;
    }

    let scratch = TempDir::new("listing-reads");
    // Canonical, because strace prints the path behind each descriptor.
    let dir = std::fs::canonicalize(&scratch).unwrap();
    for count in COUNTS {
        numbered_cbz(&dir, count);
    }

    let log = crate::test_support::strace(
        "pages::tests::listing_an_archive_reads_no_more_for_four_hundred_pages_than_for_four",
        (CHILD, dir.as_os_str()),
        "read,pread64",
        &dir.join("strace.log"),
    );
    let [four, four_hundred] =
        COUNTS.map(|count| reads_of(&log, &dir.join(format!("{count}.cbz"))));
    eprintln!("reads: {four} for 4 pages, {four_hundred} for 400 pages");
    assert!(
        four > 0,
        "no read of the four-page archive was seen, so the counts prove nothing:\n{log}"
    );
    assert_eq!(
        four_hundred, four,
        "listing 400 pages read {four_hundred} times where 4 pages read {four}"
    );
}

/// Write a cbz of `count` stored pages named `p001.jpg` onward.
///
/// Each page carries 128 bytes so that even the smallest archive is longer
/// than the 512-byte header `open_archive` sniffs: a shorter file costs that
/// sniff a second read, at end of file, and the counts would differ by it.
fn numbered_cbz(dir: &Path, count: u32) -> PathBuf {
    use std::io::Write as _;
    let path = dir.join(format!("{count}.cbz"));
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for n in 1..=count {
        zip.start_file(format!("p{n:03}.jpg"), stored).unwrap();
        zip.write_all(&[0; 128]).unwrap();
    }
    zip.finish().unwrap();
    assert!(
        std::fs::metadata(&path).unwrap().len() > 512,
        "{} must outlast the header sniff",
        path.display()
    );
    path
}

/// How many `read` and `pread64` calls in an `strace -y` log were made on
/// `file`, whose path strace prints after the descriptor.
fn reads_of(log: &str, file: &Path) -> usize {
    let on_file = format!("<{}>", file.display());
    log.lines()
        .filter(|line| {
            ["read(", "pread64("].iter().any(|call| {
                line.split_once(call).is_some_and(|(before, args)| {
                    (before.is_empty() || before.ends_with(' '))
                        && args
                            .trim_start_matches(|c: char| c.is_ascii_digit())
                            .starts_with(&on_file)
                })
            })
        })
        .count()
}

/// The listing reads each entry from the central directory, never its local
/// header, so for a well-formed archive it must list exactly what reading
/// through each local header listed. Every fixture the order and skip tests
/// use is here, plus a directory entry named like an image, and a middle
/// page `by_index` refuses: flagged encrypted, AES-encrypted, AES with its
/// flag clear, and in a method `zip` cannot decode. All but the AES page are
/// each kept out by one check alone, so each fails when its check goes.
#[test]
fn a_central_directory_listing_matches_one_read_through_each_local_header() {
    let dir = TempDir::new("central");
    let unflagged = cbz(&dir, "unflagged.cbz", &["カラー版.jpg"]);
    clear_utf8_flag(&unflagged);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let aes = stored.with_aes_encryption(zip::AesMode::Aes256, "password");
    let flagged = middle_page_cbz(&dir, "flagged.cbz", stored);
    edit_middle_headers(&flagged, FLAGS, |flags| flags | ENCRYPTED);
    let aes_unflagged = middle_page_cbz(&dir, "aes-unflagged.cbz", aes);
    edit_middle_headers(&aes_unflagged, FLAGS, |flags| {
        assert_ne!(flags & ENCRYPTED, 0, "the writer flagged its AES page");
        flags & !ENCRYPTED
    });
    let jpeg_method = middle_page_cbz(&dir, "jpeg-method.cbz", stored);
    edit_middle_headers(&jpeg_method, METHOD, |_| WINZIP_JPEG);
    let archives = [
        cbz(
            &dir,
            "order.cbz",
            &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
        ),
        cbz(
            &dir,
            "nested.cbz",
            &[
                "Chapter 2/001.jpg",
                "Chapter 1/ZBlank (1).jpg",
                "Chapter 1/001.jpg",
            ],
        ),
        cbz(
            &dir,
            "skips.cbz",
            &[
                "__MACOSX/p1.jpg",
                ".hidden.jpg",
                "__MACOSX/._p2.jpg",
                "p1.jpg",
                "p2.jpg",
            ],
        ),
        unflagged,
        cp437_cbz(&dir),
        directory_entry_cbz(&dir),
        flagged,
        middle_page_cbz(&dir, "aes.cbz", aes),
        aes_unflagged,
        jpeg_method,
    ];

    let differing: Vec<String> = archives
        .iter()
        .filter_map(|path| {
            let expected = listed_through_each_local_header(path);
            assert!(!expected.is_empty(), "{} lists no page", path.display());
            let listed = member_page_names(path).unwrap();
            (listed != expected).then(|| {
                format!(
                    "{}: listed {listed:?}, by_index {expected:?}",
                    path.display()
                )
            })
        })
        .collect();
    assert!(
        differing.is_empty(),
        "the listing differs from one read through each local header:\n{}",
        differing.join("\n")
    );
}

/// Write a cbz holding a directory entry `scans.png/` beside its pages.
fn directory_entry_cbz(dir: &Path) -> PathBuf {
    use std::io::Write as _;
    let path = dir.join("directory.cbz");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.add_directory("scans.png/", options).unwrap();
    for name in ["scans.png/002.png", "001.png"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(name.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// General purpose bit 0, set on an encrypted entry (APPNOTE.TXT §4.4.4).
const ENCRYPTED: u16 = 1;
/// WinZip's JPEG compression method, one `zip` cannot decode.
const WINZIP_JPEG: u16 = 96;
/// A header field's offset from the local header's signature and from the
/// central directory header's (APPNOTE.TXT §4.3.7, §4.3.12).
const FLAGS: (usize, usize) = (6, 8);
const METHOD: (usize, usize) = (8, 10);

/// Write a cbz of `p1.jpg`, `p2.jpg` and `p3.jpg`, stored, with the middle
/// page written with `middle`.
fn middle_page_cbz(dir: &Path, name: &str, middle: zip::write::SimpleFileOptions) -> PathBuf {
    use std::io::Write as _;
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let path = dir.join(name);
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for (page, options) in [("p1.jpg", stored), ("p2.jpg", middle), ("p3.jpg", stored)] {
        zip.start_file(page, options).unwrap();
        zip.write_all(page.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// Rewrite one 16-bit field of `p2.jpg`'s local and central headers in a
/// cbz from [`middle_page_cbz`], to what `edit` makes of its value.
fn edit_middle_headers(path: &Path, (local, central): (usize, usize), edit: impl Fn(u16) -> u16) {
    let fields = {
        let zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        let entry = zip.by_index_data(1).unwrap();
        assert_eq!(entry.name().unwrap(), "p2.jpg");
        [
            usize::try_from(entry.header_start()).unwrap() + local,
            usize::try_from(entry.central_header_start()).unwrap() + central,
        ]
    };
    let mut raw = std::fs::read(path).unwrap();
    for at in fields {
        let value = u16::from_le_bytes([raw[at], raw[at + 1]]);
        raw[at..at + 2].copy_from_slice(&edit(value).to_le_bytes());
    }
    std::fs::write(path, &raw).unwrap();
}

/// An archive's page names as listing found them by opening each entry's
/// local header: `by_index`, then the filters and sort `page_names_from`
/// applies. The oracle for the central-directory listing.
fn listed_through_each_local_header(path: &Path) -> Vec<String> {
    let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| {
            let entry = zip.by_index(i).ok()?;
            if !entry.is_file() {
                return None;
            }
            let name = entry.name().ok()?.into_owned();
            if name.starts_with("__MACOSX") || name.rsplit('/').next()?.starts_with('.') {
                return None;
            }
            is_page_name(&name).then_some(name)
        })
        .collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names
}

/// An entry whose local header does not parse is still a page: it is
/// listed in its sort position, and reading it fails naming it. Dropping it
/// from the list instead would move every later page to the index before,
/// so a bookmark or an alignment past it would land on the wrong page.
#[test]
fn an_entry_whose_local_header_is_damaged_is_listed_and_fails_when_read() {
    const LOCAL_HEADER: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
    let dir = TempDir::new("damaged");
    let path = cbz(&dir, "damaged.cbz", &["p3.jpg", "p2.jpg", "p1.jpg"]);
    let mut raw = std::fs::read(&path).unwrap();
    let headers: Vec<usize> = raw
        .windows(4)
        .enumerate()
        .filter(|(_, window)| *window == LOCAL_HEADER)
        .map(|(at, _)| at)
        .collect();
    assert_eq!(headers.len(), 3, "one local header per entry");
    // The second stored entry is p2.jpg, the middle page.
    raw[headers[1]..headers[1] + 4].copy_from_slice(b"XXXX");
    std::fs::write(&path, &raw).unwrap();

    assert_eq!(
        member_page_names(&path).unwrap(),
        ["p1.jpg", "p2.jpg", "p3.jpg"],
        "the damaged page keeps its place"
    );
    let error = member_page(&path, 1).unwrap_err();
    assert!(
        error.contains(&path.display().to_string()) && error.contains("p2.jpg"),
        "the error names the archive and the page: {error}"
    );
    assert_eq!(member_page(&path, 0).unwrap().bytes, b"p1.jpg");
    assert_eq!(member_page(&path, 2).unwrap().bytes, b"p3.jpg");
}

/// What `serve` does on every page request. Reading the whole edition to
/// answer "give me page 47" is hundreds of megabytes for one JPEG, so the
/// index has to be addressable directly — and it has to land on the same
/// page `read_member` would give at that position, or the server and the
/// aligner disagree about what page 47 is.
#[test]
fn a_page_reads_by_index_in_the_order_read_member_gives() {
    let dir = TempDir::new("byindex");
    let path = cbz(
        &dir,
        "byindex.cbz",
        &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
    );

    let names = member_page_names(&path).unwrap();
    let second = member_page(&path, 1).unwrap();
    let whole = read_member(&path).unwrap();
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
#[test]
fn a_sidecars_member_order_is_the_page_order_and_is_not_re_sorted() {
    // Named so that natural order is the reverse of the order given.
    let dir = TempDir::new("given-b");
    let b = cbz(&dir, "given-b.cbz", &["b1.jpg"]);
    let a = cbz(&dir, "given-a.cbz", &["a1.jpg"]);
    let as_given = vec![b.clone(), a.clone()];

    let in_order = read_members_in_order(&as_given).unwrap();
    let sorted = read_edition(&as_given).unwrap();
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
/// count, and it lists rather than reads. Listing that went straight to
/// `ZipArchive::new` would report a RAR file — the single most common thing
/// to find behind a `.cbr`, or behind a `.cbz` that lies — as "not a
/// readable cbz" while reading the same file says "this is a RAR archive".
/// Same file, same tool, two answers, and the useful one on the path nobody
/// takes.
#[test]
fn counting_a_rar_archives_pages_names_the_format() {
    let dir = TempDir::new("count-container");
    let path = dir.join("count-container.cbz");
    std::fs::write(&path, b"Rar!\x1a\x07\x01\x00").unwrap();

    let counted = member_pages(&path).unwrap_err();
    let listed = member_page_names(&path).unwrap_err();
    let addressed = member_page(&path, 0).unwrap_err();
    for error in [&counted, &listed, &addressed] {
        assert!(error.contains("RAR"), "unhelpful error: {error}");
    }
}

/// Every ordering test above builds a cbz; this one covers the directory
/// branches of `member_page_names` and `member_page`, including
/// `strip_prefix`, which turns an absolute path into a member-relative name.
/// Without this test, `strip_prefix` regressing to leave the temp-dir
/// prefix in page names, or a nested directory sorting on the wrong key,
/// would still pass every other test in this file.
#[test]
fn a_directory_page_reads_by_index_in_the_order_read_member_gives() {
    let dir = TempDir::new("dir-byindex");
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
    let dir = TempDir::new("sizes");
    let path = cbz(
        &dir,
        "sizes.cbz",
        &["p10.jpg", "ComicInfo.xml", "p2.jpg", "p1.jpg"],
    );

    let pages = read_member(&path).unwrap();
    let sizes = member_page_sizes(&path).unwrap();
    let expected: Vec<u64> = pages.iter().map(|p| p.bytes.len() as u64).collect();
    assert_eq!(sizes, expected, "sizes must match the bytes actually read");
    assert_eq!(sizes.len(), 3, "ComicInfo.xml is not a page");
}

/// The property the duplicate signature rests on: repackaging a scan from
/// loose images into a cbz must not change what it signs as.
#[test]
fn a_directory_and_an_archive_of_the_same_pages_size_identically() {
    let dir = TempDir::new("sizes-dir");
    for entry in ["p1.jpg", "p2.jpg", "p10.jpg"] {
        std::fs::write(dir.join(entry), entry.as_bytes()).unwrap();
    }
    // Stored in a different order, as a repacker would.
    let archive_dir = TempDir::new("sizes-arc");
    let archive = cbz(
        &archive_dir,
        "sizes-arc.cbz",
        &["p10.jpg", "p2.jpg", "p1.jpg"],
    );

    let from_dir = member_page_sizes(&dir).unwrap();
    let from_archive = member_page_sizes(&archive).unwrap();
    assert_eq!(from_dir, from_archive);
    assert_eq!(from_dir, vec![6, 6, 7], "p1.jpg, p2.jpg, p10.jpg by length");
}

/// Same declared size as the probe docs/notes.md records: ~2 GiB for an
/// entry whose real content is a handful of bytes.
const OVERSTATED: u32 = 0x7FFF_FFF0;

/// Build a cbz and patch its first entry's central-directory
/// uncompressed-size field to `declared`, a lie — the same thing a
/// hostile or corrupt `.cbz` would carry — rather than reimplementing
/// the read path against a mock. Shared by both capacity-ceiling tests
/// below, so `read_archive`'s and `member_page`'s identical clamp are
/// proven against identically-constructed evidence.
///
/// The central directory's uncompressed-size field sits at offset 24 of
/// the 46-byte fixed header that follows the `PK\x01\x02` signature
/// (APPNOTE.TXT §4.3.12). Patched, not the local file header, because
/// `entry.size()` reads the central directory — patching only that one
/// leaves the entry's actual stored bytes untouched, so an overstated
/// size still reads correctly.
fn lying_size_cbz(dir: &Path, name: &str, entries: &[&str], declared: u32) -> PathBuf {
    let path = cbz(dir, name, entries);
    let mut raw = std::fs::read(&path).unwrap();
    let central_directory = raw
        .windows(4)
        .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
        .expect("a central directory file header");
    let size_field = central_directory + 24;
    raw[size_field..size_field + 4].copy_from_slice(&declared.to_le_bytes());
    std::fs::write(&path, &raw).unwrap();
    path
}

/// A literal, not `MAX_ENTRY_CAPACITY_HINT` itself: asserting
/// `capacity() <= MAX_ENTRY_CAPACITY_HINT` passes for *any* ceiling at or
/// above the declared lie — raise the constant to 4 GiB and the assertion
/// still goes green, having proved nothing about whether the clamp does
/// anything useful. This bound sits comfortably above the 32 MiB ceiling
/// (allocator rounding, and room to retune the ceiling within its own order
/// of magnitude without editing this test) and 30-fold below the ~2 GiB lie,
/// so it still catches a regression that quietly enlarges the ceiling or
/// drops the clamp outright.
const RESERVATION_CEILING: usize = 64 * 1024 * 1024;

/// The path `read_archive` takes: every page in the member, collected at
/// once. See `MAX_ENTRY_CAPACITY_HINT`'s doc comment for why that makes
/// the per-entry ceiling matter in aggregate too.
#[test]
fn a_declared_size_far_past_reality_does_not_reserve_past_the_ceiling() {
    let dir = TempDir::new("lying-size");
    let path = lying_size_cbz(&dir, "lying-size.cbz", &["p1.jpg"], OVERSTATED);

    let pages = read_member(&path).unwrap();
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
/// `entry.size().min(MAX_ENTRY_CAPACITY_HINT)` clamp, and a change that
/// broke the clamp on `member_page` alone (a copy-paste that dropped the
/// `.min()`, say) would otherwise ship with the suite green, on the one path
/// a server request actually reaches.
#[test]
fn a_lying_member_page_does_not_reserve_past_the_ceiling_either() {
    let dir = TempDir::new("lying-size-member-page");
    let path = lying_size_cbz(&dir, "lying-size-member-page.cbz", &["p1.jpg"], OVERSTATED);

    let page = member_page(&path, 0).unwrap();
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

/// The triple `for_each_member_page`'s local-header caller relies on:
/// natural order (not stored order — this archive is written 002 first),
/// the real uncompressed size of each entry, and its crc32.
///
/// Deflated deliberately, as most of this corpus's entries are: the point of
/// this function is that it never inflates one to answer.
#[test]
fn member_page_entries_reads_names_sizes_and_crcs_from_an_archive() {
    let dir = TempDir::new("entries-archive");
    let path = dir.join("entries-archive.cbz");
    {
        use std::io::Write as _;
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in [("002.jpg", &b"second page"[..]), ("001.jpg", &b"first"[..])] {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    let entries = member_page_entries(&path).unwrap();
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["001.jpg", "002.jpg"]
    );
    assert_eq!(entries[0].size, 5);
    assert_eq!(entries[1].size, 11);
    assert_eq!(entries[0].crc32, crc32fast::hash(b"first"));
    assert_eq!(entries[1].crc32, crc32fast::hash(b"second page"));
}

/// A directory member's entries, hashed and sized from the bytes actually
/// read rather than left to trust the filesystem — both fields checked on
/// both entries, since a zeroed or wrong `size` here would otherwise pass
/// undetected.
#[test]
fn member_page_entries_hashes_a_directory_members_pages() {
    let dir = TempDir::new("entries-dir");
    std::fs::create_dir_all(dir.join("member/ch1")).unwrap();
    std::fs::write(dir.join("member/ch1/002.jpg"), b"two").unwrap();
    std::fs::write(dir.join("member/ch1/001.jpg"), b"one").unwrap();
    // Not a page, and must not become an entry.
    std::fs::write(dir.join("member/ComicInfo.xml"), b"<x/>").unwrap();

    let entries = member_page_entries(&dir.join("member")).unwrap();
    // The whole path relative to the member, not the basename — `docs/sidecar.md`.
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["ch1/001.jpg", "ch1/002.jpg"]
    );
    assert_eq!(entries[0].size, 3, "\"one\" is 3 bytes");
    assert_eq!(entries[1].size, 3, "\"two\" is 3 bytes");
    assert_eq!(entries[0].crc32, crc32fast::hash(b"one"));
    assert_eq!(entries[1].crc32, crc32fast::hash(b"two"));
}

/// The path a download's local-header writer takes: one archive open, one
/// page in memory at a time, each handed to `f` with the entry that
/// describes it.
#[test]
fn for_each_member_page_visits_every_page_in_order_with_its_entry() {
    let dir = TempDir::new("each-page");
    let path = dir.join("each-page.cbz");
    {
        use std::io::Write as _;
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in [("001.jpg", &b"one"[..]), ("002.jpg", &b"two"[..])] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    let mut seen: Vec<(String, u64, Vec<u8>)> = Vec::new();
    for_each_member_page(&path, &mut |entry, bytes| {
        seen.push((entry.name.clone(), entry.size, bytes.to_vec()));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        seen,
        vec![
            ("001.jpg".to_string(), 3, b"one".to_vec()),
            ("002.jpg".to_string(), 3, b"two".to_vec()),
        ]
    );
}

/// `f`'s `Err` stops the walk on the spot: a client that hung up must not
/// cost the rest of the volume.
#[test]
fn for_each_member_page_stops_at_the_first_refusal() {
    let dir = TempDir::new("each-stop");
    let path = dir.join("each-stop.cbz");
    {
        use std::io::Write as _;
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for name in ["001.jpg", "002.jpg", "003.jpg"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
    }

    let mut count = 0usize;
    let refused = for_each_member_page(&path, &mut |_, _| {
        count += 1;
        if count == 2 {
            Err("the caller gave up".to_string())
        } else {
            Ok(())
        }
    });
    assert_eq!(refused, Err("the caller gave up".to_string()));
    // The third page was never read: a download whose client hung up must not
    // keep inflating the rest of the volume.
    assert_eq!(count, 2);
}

/// Pinned so it cannot be "tidied up" back to symmetry:
/// `for_each_member_page` must report the real byte count `read_to_end`
/// produced, not the central directory's declared `uncompressed_size`. `zip`
/// fails a read that runs past that field, but a corrupt or hand-edited
/// archive can overstate it and still read back cleanly, crc32 and all, so
/// reporting the declared size would hand `f` an entry whose `size`
/// disagreed with the `bytes` beside it.
///
/// `member_page_entries` must keep reporting the declared lie on the very
/// same file: it is the planning function, and reading page data to verify
/// the size would destroy the property it exists for. The two are supposed
/// to disagree on a lying archive — that disagreement is what lets a later
/// plan-vs-stream comparison catch an archive that changed mid-download.
#[test]
fn for_each_member_page_reports_the_real_size_while_member_page_entries_keeps_the_declared_lie() {
    let dir = TempDir::new("lying-size-for-each");
    let path = lying_size_cbz(&dir, "lying-size-for-each.cbz", &["p1.jpg"], OVERSTATED);

    let mut seen: Vec<PageEntry> = Vec::new();
    for_each_member_page(&path, &mut |entry, _bytes| {
        seen.push(entry.clone());
        Ok(())
    })
    .unwrap();
    let planned = member_page_entries(&path).unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].size,
        b"p1.jpg".len() as u64,
        "for_each_member_page must report the real byte count, not the declared lie"
    );
    assert_eq!(
        planned[0].size,
        u64::from(OVERSTATED),
        "member_page_entries is the planning function and must keep reporting \
         the declared lie, or the plan-vs-stream comparison can never catch one"
    );
}

/// The other direction of the lie: data that runs past its declared size
/// fails the read, so the walk returns the error and `f` never receives a
/// page cut to what the archive claimed.
#[test]
fn for_each_member_page_refuses_an_entry_holding_more_than_it_declares() {
    let dir = TempDir::new("understated-size-for-each");
    let path = lying_size_cbz(&dir, "understated-size-for-each.cbz", &["p1.jpg"], 1);

    let mut seen: Vec<PageEntry> = Vec::new();
    let read = for_each_member_page(&path, &mut |entry, _bytes| {
        seen.push(entry.clone());
        Ok(())
    });
    let error = read.expect_err("an entry holding more than it declares must fail the read");
    assert!(
        error.contains("p1.jpg"),
        "the error names the entry: {error}"
    );
    assert!(seen.is_empty(), "no page reaches `f`: {seen:?}");
}

/// `member_entry` is what `taiyaku-cli::credits` reads `ComicInfo.xml`
/// through, so this pins the one property that function relies on: it
/// finds a non-page entry `page_names_from` would have filtered out, and
/// reads back the bytes of that entry specifically rather than whichever
/// happened to be first.
#[test]
fn member_entry_finds_a_named_archive_entry_among_pages_and_reads_its_own_bytes() {
    let dir = TempDir::new("member-entry");
    let path = cbz(
        &dir,
        "member-entry.cbz",
        &["p1.jpg", "ComicInfo.xml", "p2.jpg"],
    );

    let found = member_entry(&path, "ComicInfo.xml").unwrap();
    let missing = member_entry(&path, "Nonexistent.xml").unwrap();
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
    let dir = TempDir::new("member-entry-case");
    let path = cbz(&dir, "member-entry-case.cbz", &["comicinfo.xml"]);

    let found = member_entry(&path, "ComicInfo.xml").unwrap();
    assert_eq!(found, Some(b"comicinfo.xml".to_vec()));
}

/// The directory branch of `member_entry` — a fan edition stored as loose
/// files rather than a `.cbz` — must find the same named entry the
/// archive branch does.
#[test]
fn member_entry_reads_a_directory_members_own_file() {
    let dir = TempDir::new("member-entry-dir");
    std::fs::write(dir.join("001.jpg"), b"page").unwrap();
    std::fs::write(dir.join("ComicInfo.xml"), b"<ComicInfo/>").unwrap();

    let found = member_entry(&dir, "ComicInfo.xml").unwrap();
    let missing = member_entry(&dir, "Nonexistent.xml").unwrap();
    assert_eq!(found, Some(b"<ComicInfo/>".to_vec()));
    assert_eq!(missing, None);
}

/// Deliberately not a recursive walk: unlike `directory_page_paths`,
/// which descends into an edition's chapter directories to find images,
/// `member_entry` only looks beside the pages at the member's own level —
/// every `ComicInfo.xml` this crate has read sits there, never nested.
#[test]
fn member_entry_does_not_descend_into_a_directorys_subdirectories() {
    let dir = TempDir::new("member-entry-nested");
    std::fs::create_dir_all(dir.join("Chapter 1")).unwrap();
    std::fs::write(dir.join("Chapter 1").join("ComicInfo.xml"), b"nested").unwrap();

    let found = member_entry(&dir, "ComicInfo.xml").unwrap();
    assert_eq!(
        found, None,
        "a nested ComicInfo.xml is not this member's own"
    );
}

/// A source that returns its bytes a few at a time must still be recognised: the
/// tar magic sits at offset 257, so a header assembled from one `read` that
/// stops short would call a tar an unreadable zip.
#[test]
fn a_tar_is_named_even_when_the_source_reads_one_byte_at_a_time() {
    let mut tar = vec![0u8; 512];
    tar[257..262].copy_from_slice(b"ustar");

    let error = refuse_unsupported_container(
        &mut crate::test_support::OneByteAtATime { rest: &tar },
        Path::new("volume.cbz"),
    )
    .unwrap_err();
    assert!(error.contains("this is a tar archive"), "{error}");
}

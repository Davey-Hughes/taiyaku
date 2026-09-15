use super::*;
use std::path::PathBuf;

use crate::test_support::TempDir;

fn tempdir() -> TempDir {
    TempDir::new("format")
}

/// A zip whose entries are `names`, each holding `bytes`.
fn zip_with(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let file = std::fs::File::create(&path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    for (entry, bytes) in entries {
        zip.start_file(*entry, zip::write::SimpleFileOptions::default())
            .expect("start");
        std::io::Write::write_all(&mut zip, bytes).expect("write");
    }
    zip.finish().expect("finish");
    path
}

/// A real, single-member tar, built from bytes.
///
/// **There was no tar fixture anywhere in the workspace**, and three
/// surviving mutants shared that one absence: the header buffer could shrink
/// from 512 bytes to 8, `Format::Tar`'s token could change, and the tar and
/// zip tests in `from_header` could swap places, with every test in the
/// repository still green. `taiyaku-read` depends on `zip` and on nothing
/// that writes tars, and the constraint on this branch is no new
/// dependency — so the header is written out field by field, which is also
/// what makes the offset the whole question turns on (`ustar` at 257)
/// visible rather than incidental.
///
/// The layout is POSIX ustar: `name` at 0, `size` and `mtime` as
/// NUL-terminated octal, the checksum computed over the header with its own
/// eight checksum bytes read as spaces, and `ustar\0` + `00` at 257. Two
/// 512-byte NUL blocks end the archive. `a_hand_written_tar_is_one_gnu_tar_can_list`
/// is what keeps this honest: a fixture that only satisfies the code under
/// test proves nothing about tars.
fn tar_with(dir: &Path, file_name: &str, member: &[u8], bytes: &[u8]) -> PathBuf {
    let mut header = [0u8; 512];
    header[..member.len()].copy_from_slice(member);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(format!("{:011o}\0", bytes.len()).as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    // The checksum is defined over a header whose own checksum field reads as
    // eight spaces, so they go in before the sum is taken.
    header[148..156].copy_from_slice(b"        ");
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());

    let mut out = header.to_vec();
    out.extend_from_slice(bytes);
    out.resize(out.len() + (512 - bytes.len() % 512) % 512, 0);
    out.resize(out.len() + 1024, 0);

    let path = dir.join(file_name);
    std::fs::write(&path, &out).expect("write");
    path
}

/// The fixture above is a tar by the only standard that matters: `tar` itself
/// lists it. Without this, every tar assertion below would be circular — a
/// blob that happens to carry `ustar` at offset 257 satisfies `from_header`
/// whether or not any tool on earth would call it an archive.
///
/// Skips, saying so, where `tar` is not installed rather than asserting
/// against a missing binary — the same `NOT MEASURED:` shape
/// `watch/tests.rs` and `taiyaku-serve/src/tests.rs` already use.
#[test]
fn a_hand_written_tar_is_one_gnu_tar_can_list() {
    let dir = tempdir();
    let path = tar_with(dir.path(), "volume.tar", b"001.jpg", b"not really a jpeg");

    let listed = std::process::Command::new("tar")
        .arg("-tf")
        .arg(&path)
        .output();
    let Ok(listed) = listed else {
        eprintln!("NOT MEASURED: a_hand_written_tar_is_one_gnu_tar_can_list (no `tar` on PATH)");
        return;
    };
    assert!(
        listed.status.success(),
        "tar refused the fixture: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout).trim(),
        "001.jpg",
        "the one member this fixture holds"
    );
}

/// The tar signature is at offset **257**, further into the file than every
/// other signature this module knows by two orders of magnitude — so this is
/// the only format whose answer depends on `sniff` actually filling its
/// header buffer. Shrinking that buffer, or accepting one short `read` as
/// the whole header, turns every tar into `Unknown` and leaves the other
/// five answers correct.
#[test]
fn a_tar_is_a_tar_from_a_signature_257_bytes_in() {
    let dir = tempdir();
    let path = tar_with(dir.path(), "volume.tar", b"001.jpg", b"not really a jpeg");
    assert_eq!(sniff(&path), Ok(Format::Tar));
    assert_eq!(Format::Tar.token(), Some("tar"));
}

/// A tar's first 100 bytes are a filename, and a tar filename may hold any
/// byte — so a tar can begin with the four bytes a zip begins with. This is
/// that file. It is the case that distinguishes `from_header`'s branch
/// order: with the zip test first it sniffs as `Zip` and is handed to the
/// zip reader, which cannot open it.
#[test]
fn a_tar_whose_first_entry_name_begins_with_a_zip_signature_is_still_a_tar() {
    let dir = tempdir();
    let path = tar_with(dir.path(), "trap.tar", b"PK\x03\x04-trap.jpg", b"x");

    let bytes = std::fs::read(&path).expect("read");
    assert_eq!(
        &bytes[..4],
        b"PK\x03\x04",
        "the fixture really does collide"
    );
    assert_eq!(&bytes[257..262], b"ustar", "and really is a tar");

    assert_eq!(sniff(&path), Ok(Format::Tar));
}

/// A reader that hands back one byte per call, however much is asked for.
///
/// Legal `Read` behaviour, and what a network filesystem does under load. No
/// file written to this host's local disk will produce it, which is why
/// `read_header` takes a `Read` at all.
struct OneByteAtATime<'a> {
    rest: &'a [u8],
}

impl std::io::Read for OneByteAtATime<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.rest.is_empty() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.rest[0];
        self.rest = &self.rest[1..];
        Ok(1)
    }
}

/// The defect this pins: with a single `read`, a source that returns one
/// byte at a time gave `from_header` a one-byte slice, and every tar became
/// `Unknown` while `%PDF-` and the rest still answered from their first six
/// bytes. Both halves are asserted, because a fix that read the tar
/// correctly and lost the short-file case would pass on the first assertion
/// alone.
#[test]
fn a_source_that_reads_one_byte_at_a_time_still_fills_the_header() {
    let dir = tempdir();
    let path = tar_with(dir.path(), "volume.tar", b"001.jpg", b"not really a jpeg");
    let bytes = std::fs::read(&path).expect("read");

    let (header, read) =
        read_header(&mut OneByteAtATime { rest: &bytes }).expect("reads the header");
    assert_eq!(read, 512, "the whole buffer, one byte per call");
    assert_eq!(from_header(&header[..read]), Format::Tar);

    // And a source with less than a header in it stops at what it has,
    // rather than spinning or reporting 512 bytes of zeroes as real.
    let (short, read) =
        read_header(&mut OneByteAtATime { rest: b"%PDF-" }).expect("reads the header");
    assert_eq!(read, 5);
    assert_eq!(from_header(&short[..read]), Format::Pdf);
}

/// A file shorter than the 512-byte header buffer is ordinary — a stub, a
/// truncated download — and must still answer, rather than failing the way
/// `read_exact` would.
#[test]
fn a_file_shorter_than_the_header_buffer_still_answers() {
    let dir = tempdir();
    let path = dir.path().join("stub.pdf");
    std::fs::write(&path, b"%PDF-").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Pdf));

    let empty = dir.path().join("empty.cbz");
    std::fs::write(&empty, b"").expect("write");
    assert_eq!(sniff(&empty), Ok(Format::Unknown));
}

/// **`Format::is_readable` had no test anywhere in the workspace**, and
/// `token` had none for either EPUB variant. Both are enumerated here rather
/// than sampled: `token`'s own doc says the strings are the schema's
/// vocabulary and "a change here is a migration, not a rename", and
/// `is_readable` decides whether a member is offered to a reader that can
/// open it — a `Pdf` slipping into the readable set is a reader failing on
/// material the library told it to expect.
#[test]
fn every_format_names_its_schema_token_and_says_whether_it_can_be_read() {
    for (format, token, readable) in [
        (Format::Dir, Some("dir"), true),
        (Format::Zip, Some("zip"), true),
        (Format::EpubFixed, Some("epub-fixed"), false),
        (Format::EpubReflow, Some("epub-reflow"), false),
        (Format::Pdf, Some("pdf"), false),
        (Format::Rar, Some("rar"), false),
        (Format::SevenZip, Some("7z"), false),
        (Format::Tar, Some("tar"), false),
        (Format::Unknown, None, false),
    ] {
        assert_eq!(format.token(), token, "{format:?}");
        assert_eq!(format.is_readable(), readable, "{format:?}");
    }
}

#[test]
fn a_plain_zip_is_a_zip_whatever_it_is_called() {
    let dir = tempdir();
    // Named .cbr, and README records that this happens constantly.
    let path = zip_with(dir.path(), "volume.cbr", &[("001.jpg", b"x")]);
    assert_eq!(sniff(&path), Ok(Format::Zip));
    assert_eq!(Format::Zip.token(), Some("zip"));
}

#[test]
fn a_directory_is_a_dir() {
    let dir = tempdir();
    assert_eq!(sniff(dir.path()), Ok(Format::Dir));
}

#[test]
fn a_rar_named_cbz_is_a_rar() {
    let dir = tempdir();
    let path = dir.path().join("volume.cbz");
    std::fs::write(&path, b"Rar!\x1a\x07\x00rest").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Rar));
}

#[test]
fn a_pdf_is_a_pdf() {
    let dir = tempdir();
    let path = dir.path().join("book.pdf");
    std::fs::write(&path, b"%PDF-1.7\nrest").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Pdf));
}

#[test]
fn an_epub_is_split_by_its_declared_layout() {
    let dir = tempdir();
    let reflow = zip_with(
        dir.path(),
        "novel.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            ("content.opf", b"<package><metadata></metadata></package>"),
        ],
    );
    assert_eq!(sniff(&reflow), Ok(Format::EpubReflow));

    let fixed = zip_with(
        dir.path(),
        "comic.epub",
        &[
            ("mimetype", b"application/epub+zip"),
            (
                "content.opf",
                b"<package><metadata><meta property=\"rendition:layout\">pre-paginated</meta></metadata></package>",
            ),
        ],
    );
    assert_eq!(sniff(&fixed), Ok(Format::EpubFixed));
}

#[test]
fn a_file_that_is_none_of_them_is_unknown_rather_than_an_error() {
    let dir = tempdir();
    let path = dir.path().join("cover.txt");
    std::fs::write(&path, b"not a container").expect("write");
    assert_eq!(sniff(&path), Ok(Format::Unknown));
    assert_eq!(Format::Unknown.token(), None);
}

#[test]
fn a_path_that_is_not_there_is_an_error_naming_it() {
    let dir = tempdir();
    let path = dir.path().join("gone.cbz");
    let message = sniff(&path).expect_err("a missing path is an error");
    assert!(message.contains("gone.cbz"), "{message}");
}

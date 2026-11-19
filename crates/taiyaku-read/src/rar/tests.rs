use std::path::{Path, PathBuf};

use super::{BACKENDS, Backend, Entry, Visitor, backend, with_backend};
use crate::test_support::{RarEntryKind, RarFixture, TempDir, rar_fixtures, sha256_hex};

/// Every fixture, after checking there is one: a loop over none passes.
fn fixtures() -> Vec<RarFixture> {
    let all = rar_fixtures();
    assert!(!all.is_empty(), "no RAR fixture found");
    all
}

fn fixture(file: &str) -> RarFixture {
    rar_fixtures()
        .into_iter()
        .find(|fixture| fixture.file == file)
        .unwrap_or_else(|| panic!("no fixture {file}"))
}

/// The error `got` must be, which must start with `path`.
fn refusal<T>(got: Result<T, String>, path: &Path, at: &str) -> String {
    let Err(error) = got else {
        panic!("{at}: succeeded where it must fail");
    };
    assert!(
        error.starts_with(&path.display().to_string()),
        "{at}: does not start with its path: {error}"
    );
    error
}

/// The manifest's sha256 for an entry read by name, where it gives one.
fn sha_of<'a>(fixture: &'a RarFixture, name: &str) -> Option<&'a str> {
    let page = fixture.pages.iter().find(|(page, _)| page == name);
    let other = fixture.others.iter().find(|(other, _)| other == name);
    page.and_then(|(_, sha)| sha.as_deref())
        .or(other.map(|(_, sha)| sha.as_str()))
}

/// One entry a visit handed over.
#[derive(Debug)]
struct Handed {
    name: String,
    len: u64,
    sha: String,
}

/// A visit's result, and what it handed over.
fn visited(
    backend: &dyn Backend,
    path: &Path,
    wanted: &dyn Fn(&str) -> bool,
) -> (Result<(), String>, Vec<Handed>) {
    let mut seen = Vec::new();
    let got = backend.visit(path, wanted, &mut |name: &str, bytes: &[u8]| {
        seen.push(Handed {
            name: name.to_owned(),
            len: bytes.len() as u64,
            sha: sha256_hex(bytes),
        });
        Ok(())
    });
    (got, seen)
}

#[test]
fn every_entry_is_listed_in_archive_order_with_its_kind() {
    for backend in BACKENDS {
        for fixture in fixtures() {
            let at = format!("{} on {}", backend.name(), fixture.file);
            let got = backend.entries(&fixture.path);
            if let Some(text) = &fixture.refused {
                let error = refusal(got, &fixture.path, &at);
                assert!(error.contains(text.as_str()), "{at}: {error}");
                continue;
            }
            let got = got.unwrap_or_else(|e| panic!("{at}: {e}"));
            assert_eq!(got.len(), fixture.entries.len(), "{at}: {got:?}");
            for (index, (entry, want)) in got.iter().zip(&fixture.entries).enumerate() {
                assert_eq!(entry.name, want.name, "{at}: entry {index}");
                // A reference's header size is its target's; libarchive gives none.
                if !matches!(want.kind, RarEntryKind::Dir | RarEntryKind::Reference) {
                    assert_eq!(entry.size, want.size, "{at}: {}", want.name);
                }
                assert_eq!(
                    entry.readable,
                    want.kind != RarEntryKind::Dir,
                    "{at}: {} is a {:?}",
                    want.name,
                    want.kind
                );
            }
        }
    }
}

#[test]
fn each_page_and_other_entry_reads_to_its_manifest_bytes() {
    let mut reads = 0;
    for backend in BACKENDS {
        for fixture in fixtures() {
            let at = format!("{} on {}", backend.name(), fixture.file);
            let pages = fixture
                .pages
                .iter()
                .filter_map(|(name, sha)| Some((name, sha.as_ref()?)));
            for (name, sha) in pages.chain(fixture.others.iter().map(|(name, sha)| (name, sha))) {
                let bytes = backend
                    .read(&fixture.path, name)
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                assert_eq!(sha256_hex(&bytes), *sha, "{at}: {name}");
                reads += 1;
            }
        }
    }
    assert!(reads > 0, "no page or other entry was read");
}

#[test]
fn an_encrypted_entry_does_not_read() {
    for backend in BACKENDS {
        for file in ["rar5-encrypted-entry.rar", "rar4-encrypted-entry.rar"] {
            let at = format!("{} on {file}", backend.name());
            let fixture = fixture(file);
            let error = refusal(backend.read(&fixture.path, "p02.png"), &fixture.path, &at);
            assert!(error.contains("p02.png"), "{at}: {error}");
        }
    }
}

#[test]
fn a_visit_decodes_wanted_readable_entries_in_archive_order() {
    for backend in BACKENDS {
        for fixture in fixtures() {
            let at = format!("{} on {}", backend.name(), fixture.file);
            let (got, seen) = visited(*backend, &fixture.path, &|_| true);
            if let Some(text) = &fixture.refused {
                let error = refusal(got, &fixture.path, &at);
                // A split entry fails by its own name, before the listing
                // reaches what refuses the archive.
                let split = fixture
                    .entries
                    .iter()
                    .find(|entry| entry.kind == RarEntryKind::Split);
                let text = split.map_or(text.as_str(), |entry| entry.name.as_str());
                assert!(error.contains(text), "{at}: {error}");
                // A visit stops at the entry that refuses the archive, and may
                // hand over those before it, as a solid RAR4's first.
                assert!(seen.len() < fixture.entries.len().max(1), "{at}: {seen:?}");
                for (got, want) in seen.iter().zip(&fixture.entries) {
                    assert_eq!(got.name, want.name, "{at}");
                    assert_eq!(got.len, want.size, "{at}: {}", got.name);
                }
                continue;
            }
            // An encrypted entry or a reference fails to read, and the visit
            // stops there.
            let readable: Vec<_> = fixture
                .entries
                .iter()
                .filter(|entry| entry.kind != RarEntryKind::Dir)
                .collect();
            let stop = readable.iter().position(|entry| {
                matches!(
                    entry.kind,
                    RarEntryKind::Encrypted
                        | RarEntryKind::Reference
                        | RarEntryKind::Empty
                        | RarEntryKind::Split
                )
            });
            let handed = &readable[..stop.unwrap_or(readable.len())];
            match stop {
                Some(at_entry) => {
                    let error = refusal(got, &fixture.path, &at);
                    let name = &readable[at_entry].name;
                    assert!(error.contains(name.as_str()), "{at}: {error}");
                }
                None => got.unwrap_or_else(|e| panic!("{at}: {e}")),
            }
            assert_eq!(seen.len(), handed.len(), "{at}: {seen:?}");
            for (got, want) in seen.iter().zip(handed) {
                assert_eq!(got.name, want.name, "{at}");
                assert_eq!(got.len, want.size, "{at}: {}", got.name);
                if let Some(sha) = sha_of(&fixture, &got.name) {
                    assert_eq!(got.sha, sha, "{at}: {}", got.name);
                }
            }
        }

        let fixture = fixture("rar5-compressed.rar");
        let (got, seen) = visited(*backend, &fixture.path, &|name| name == "p2.png");
        got.unwrap_or_else(|e| panic!("{}: {e}", backend.name()));
        let names: Vec<_> = seen.iter().map(|got| got.name.as_str()).collect();
        assert_eq!(names, ["p2.png"], "{}", backend.name());
        assert_eq!(Some(seen[0].sha.as_str()), sha_of(&fixture, "p2.png"));
    }
}

/// A file reference lists as a regular file and yields no bytes, so only its
/// refusal keeps it from being an empty page.
#[test]
fn a_file_reference_is_refused_as_no_data() {
    let fixture = fixture("rar5-file-reference.rar");
    for backend in BACKENDS {
        let at = format!("{} on {}", backend.name(), fixture.file);
        let error = refusal(backend.read(&fixture.path, "p02.png"), &fixture.path, &at);
        assert!(error.contains("p02.png: no data"), "{at}: {error}");
        let (got, seen) = visited(*backend, &fixture.path, &|_| true);
        let error = refusal(got, &fixture.path, &at);
        assert!(error.contains("p02.png: no data"), "{at}: {error}");
        let names: Vec<_> = seen.iter().map(|got| got.name.as_str()).collect();
        assert_eq!(names, ["p01.png"], "{at}");
    }
}

#[test]
fn a_visit_stops_at_the_visitors_first_refusal() {
    let fixture = fixture("rar5-stored.rar");
    for backend in BACKENDS {
        let mut calls = 0;
        let got = backend.visit(&fixture.path, &|_| true, &mut |_: &str, _: &[u8]| {
            calls += 1;
            if calls == 2 {
                Err("stop".to_owned())
            } else {
                Ok(())
            }
        });
        assert_eq!(got, Err("stop".to_owned()), "{}", backend.name());
        assert_eq!(calls, 2, "{}", backend.name());
    }
}

/// Review Focus 1. A cut inside a page leaves its header whole, and the page
/// ends early with no error from libarchive, so only its declared size shows
/// it is short.
#[test]
fn a_truncated_archive_is_refused_with_its_path() {
    let fixture = fixture("rar5-compressed.rar");
    let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
    let dir = TempDir::new("rar-truncated");
    let cut = |name: &str, len: usize| -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, &bytes[..len]).expect("the copy writes");
        path
    };
    // Half falls inside p2.png, and 16 bytes short inside p1.png, the last
    // entry.
    let half = cut("half.rar", bytes.len() / 2);
    let tail = cut("tail.rar", bytes.len() - 16);
    let declared = |name: &str| {
        fixture
            .entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.size)
    };

    for backend in BACKENDS {
        let at = format!("{} on the half copy", backend.name());
        let error = refusal(backend.entries(&half), &half, &at);
        assert!(
            error.contains("not a readable RAR archive"),
            "{at}: {error}"
        );
        refusal(backend.read(&half, "p1.png"), &half, &at);

        for (copy, page) in [(&half, "p2.png"), (&tail, "p1.png")] {
            let at = format!("{} on {}", backend.name(), copy.display());
            let error = refusal(backend.read(copy, page), copy, &at);
            assert!(error.contains(page), "{at}: {error}");
            let (got, seen) = visited(*backend, copy, &|_| true);
            refusal(got, copy, &at);
            for got in &seen {
                assert_eq!(
                    Some(got.len),
                    declared(&got.name),
                    "{at}: {} was handed over short",
                    got.name
                );
            }
        }
    }
}

#[test]
fn a_missing_file_names_its_path() {
    let dir = TempDir::new("rar-missing");
    let path = dir.join("missing.rar");
    for backend in BACKENDS {
        let at = backend.name();
        let calls = [
            backend.entries(&path).map(drop),
            backend.read(&path, "p1.png").map(drop),
            backend.visit(&path, &|_| true, &mut |_: &str, _: &[u8]| Ok(())),
        ];
        for got in calls {
            let error = refusal(got, &path, at);
            assert!(
                error.contains("not a readable RAR archive"),
                "{at}: {error}"
            );
        }
    }
}

#[test]
fn a_name_the_archive_lacks_is_no_such_entry() {
    for backend in BACKENDS {
        for fixture in fixtures().into_iter().filter(|f| f.refused.is_none()) {
            assert_eq!(
                backend.read(&fixture.path, "missing.png").map(|b| b.len()),
                Err(format!(
                    "{}: missing.png: no such entry",
                    fixture.path.display()
                )),
                "{} on {}",
                backend.name(),
                fixture.file
            );
        }
    }
}

/// Review Focus 4: the server's blocking pool reads one archive from several
/// threads at once.
#[test]
fn six_threads_reading_one_archive_each_get_their_own_page() {
    let fixture = fixture("rar5-stored.rar");
    assert_eq!(fixture.pages.len(), 6, "one thread a page");
    for backend in BACKENDS {
        let start = std::sync::Barrier::new(fixture.pages.len());
        std::thread::scope(|scope| {
            let threads: Vec<_> = fixture
                .pages
                .iter()
                .map(|(name, sha)| {
                    let (start, path) = (&start, &fixture.path);
                    scope.spawn(move || {
                        start.wait();
                        (name, sha, backend.read(path, name))
                    })
                })
                .collect();
            for thread in threads {
                let (name, sha, got) = thread.join().expect("the reading thread");
                let bytes = got.unwrap_or_else(|e| panic!("{}: {e}", backend.name()));
                assert_eq!(Some(sha256_hex(&bytes)), *sha, "{}: {name}", backend.name());
            }
        });
    }
}

/// Reads nothing, so a test can tell it from a compiled backend.
#[derive(Debug)]
struct Absent;

impl Backend for Absent {
    fn name(&self) -> &'static str {
        "absent"
    }
    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String> {
        Err(path.display().to_string())
    }
    fn read(&self, path: &Path, _: &str) -> Result<Vec<u8>, String> {
        Err(path.display().to_string())
    }
    fn visit(
        &self,
        path: &Path,
        _: &dyn Fn(&str) -> bool,
        _: &mut Visitor<'_>,
    ) -> Result<(), String> {
        Err(path.display().to_string())
    }
}

#[test]
fn page_reads_use_the_first_backend_unless_a_test_chooses_one() {
    let first = BACKENDS[0].name();
    assert_eq!(backend().name(), first);
    assert_eq!(with_backend(&Absent, || backend().name()), "absent");
    assert_eq!(backend().name(), first, "the choice ends with the call");
    let panicked = std::panic::catch_unwind(|| with_backend(&Absent, || panic!("a failing test")));
    assert!(panicked.is_err());
    assert_eq!(
        backend().name(),
        first,
        "the choice ends when the call panics"
    );
}

/// A8's end of a RAR5: an end-of-archive block, known by its type and CRC,
/// which only zero bytes may follow.
#[cfg(feature = "rar-libarchive")]
#[test]
fn a_rar5_end_block_is_known_by_its_type_and_crc() {
    use super::libarchive::ends_with_end_block;
    let bytes = std::fs::read(fixture("rar5-compressed.rar").path).expect("the fixture reads");
    let end = &bytes[bytes.len() - 8..];
    assert!(ends_with_end_block(end), "{end:02x?}");
    assert!(ends_with_end_block(&[end, &[0; 9]].concat()), "padded");
    assert!(
        !ends_with_end_block(&[end, &[1]].concat()),
        "a byte after it"
    );
    let mut crc_broken = end.to_vec();
    crc_broken[0] ^= 1;
    assert!(!ends_with_end_block(&crc_broken), "its CRC broken");
    let mut another = end[4..].to_vec();
    another[1] = 4;
    let crc = crc32fast::hash(&another).to_le_bytes();
    assert!(
        !ends_with_end_block(&[&crc[..], &another].concat()),
        "type 4"
    );
    let framed = |block: &[u8]| [&crc32fast::hash(block).to_le_bytes()[..], block].concat();
    let mut wide = vec![0x81, 0x01, 5, 1, 125, 0];
    wide.extend([0xaa; 125]);
    assert_eq!(
        (
            ends_with_end_block(&framed(&wide)),
            ends_with_end_block(&framed(&[4, 0x85, 0x01, 0, 0])),
        ),
        (true, false),
        "a size of 129 and a type of 133, each a vint of two bytes"
    );
}

/// The header walk on its own: what it reads from every fixture, and that no
/// file it is given can make it panic or run away.
#[cfg(feature = "rar-libarchive")]
mod walk {
    use std::io::{Cursor, Read, Seek, SeekFrom};

    use super::super::libarchive::headers::{Header, Walk, walk, walk_at_most};
    use super::{BACKENDS, RarEntryKind, TempDir, fixture, fixtures};

    fn walked(bytes: &[u8]) -> Walk {
        walk(&mut Cursor::new(bytes)).expect("a walk of memory does not fail")
    }

    /// Every entry of every listable fixture, in order, with the checksum of
    /// the bytes it holds and none where they cannot be checked.
    #[test]
    fn a_walk_gives_each_entry_its_own_checksum() {
        let mut checked = 0;
        // Headers encrypted: the walk has nothing of its entries to give.
        for fixture in fixtures().into_iter().filter(|f| !f.entries.is_empty()) {
            let at = fixture.file.as_str();
            let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
            let walked = walked(&bytes);
            assert_eq!(walked.stopped, None, "{at}");
            assert_eq!(walked.headers.len(), fixture.entries.len(), "{at}");
            assert_eq!(walked.solid, at == "rar5-solid.rar", "{at}");
            for (header, entry) in walked.headers.iter().zip(&fixture.entries) {
                let at = format!("{at}: {}", entry.name);
                match entry.kind {
                    RarEntryKind::File if at.starts_with("rar5-blake2.rar") => {
                        assert_eq!(header.crc, None, "{at}: BLAKE2 alone");
                    }
                    RarEntryKind::File if fixture.refused.is_some() => {
                        assert!(header.crc.is_some(), "{at}");
                    }
                    RarEntryKind::File => {
                        let read = BACKENDS[0].read(&fixture.path, &entry.name);
                        let read = read.unwrap_or_else(|e| panic!("{at}: {e}"));
                        assert_eq!(header.crc, Some(crc32fast::hash(&read)), "{at}");
                        assert_eq!(header.size, entry.size, "{at}");
                        checked += 1;
                    }
                    // An empty file's bytes are refused before any checksum.
                    RarEntryKind::Empty => assert_eq!(header.size, 0, "{at}"),
                    RarEntryKind::Dir
                    | RarEntryKind::Encrypted
                    | RarEntryKind::Reference
                    | RarEntryKind::Split => assert_eq!(header.crc, None, "{at}"),
                }
            }
        }
        assert!(checked > 0, "no entry's checksum was compared");
    }

    #[test]
    fn a_file_without_a_rar_signature_stops_the_walk() {
        let walked = walked(b"PK\x03\x04 and not a RAR at all");
        assert_eq!(
            walked.stopped,
            Some("it does not start with a RAR signature")
        );
        assert!(walked.headers.is_empty());
    }

    /// A cut anywhere leaves the headers before it, and a file cut inside a
    /// block says so.
    #[test]
    fn a_cut_walk_keeps_the_headers_before_the_cut() {
        for file in ["rar5-compressed.rar", "rar4-encrypted-entry.rar"] {
            let bytes = std::fs::read(fixture(file).path).expect("the fixture reads");
            let whole = walked(&bytes);
            for len in 0..bytes.len() {
                let cut = walked(&bytes[..len]);
                let at = format!("{file} cut to {len}");
                assert!(cut.headers.len() <= whole.headers.len(), "{at}");
                assert_eq!(cut.headers[..], whole.headers[..cut.headers.len()], "{at}");
            }
        }
    }

    /// No byte of a header, changed to anything, panics the walk or leaves it
    /// holding more headers than the file has room for.
    #[test]
    fn no_changed_byte_panics_the_walk() {
        for file in [
            "rar5-compressed.rar",
            "rar5-layout.rar",
            "rar4-encrypted-entry.rar",
        ] {
            let original = std::fs::read(fixture(file).path).expect("the fixture reads");
            for at in 0..original.len() {
                for change in [0x01, 0x10, 0x80, 0xff] {
                    let mut bytes = original.clone();
                    bytes[at] ^= change;
                    let walked = walked(&bytes);
                    assert!(walked.headers.len() <= bytes.len() / 7, "{file}: {at}");
                }
            }
        }
    }

    #[test]
    fn a_walk_stops_at_its_block_limit() {
        let bytes = std::fs::read(fixture("rar5-stored.rar").path).expect("the fixture reads");
        let mut cursor = Cursor::new(&bytes);
        let walked = walk_at_most(&mut cursor, 3).expect("a walk of memory does not fail");
        assert_eq!(walked.stopped, Some("it holds too many blocks"));
        assert_eq!(walked.headers.len(), 2, "the main block, then two files");
        assert_eq!(cursor.position(), 0, "the file is left at its start");
    }

    /// A RAR5 block with these fields after its type and flags, its header's
    /// own CRC left zero: the walk leaves that to libarchive.
    fn rar5_block(kind: u8, flags: u8, fields: &[u8]) -> Vec<u8> {
        let mut body = vec![kind, flags];
        body.extend_from_slice(fields);
        let size = u8::try_from(body.len()).expect("a small test header");
        assert!(size < 0x80, "the size fits one vint byte");
        let mut block = vec![0; 4];
        block.push(size);
        block.extend(body);
        block
    }

    fn rar5(blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = b"Rar!\x1a\x07\x01\x00".to_vec();
        bytes.extend(blocks.concat());
        bytes
    }

    const RAR5_END: (u8, u8, &[u8]) = (5, 0, &[0]);

    fn rar5_end() -> Vec<u8> {
        rar5_block(RAR5_END.0, RAR5_END.1, RAR5_END.2)
    }

    /// The fields of a file block with no data: flags with a CRC, size 5, a
    /// regular Unix file's mode, the CRC, compression, host Unix, one-byte name.
    fn rar5_file_fields(extra: &[u8]) -> Vec<u8> {
        let mut fields = Vec::new();
        if !extra.is_empty() {
            fields.push(u8::try_from(extra.len()).expect("a small extra area"));
        }
        fields.push(0); // data size
        fields.extend([0x04, 5]);
        fields.extend([0xa4, 0x83, 0x02]); // 0o100644
        fields.extend(0x1234_5678_u32.to_le_bytes());
        fields.extend([0, 1, 1, b'a']);
        fields.extend_from_slice(extra);
        fields
    }

    fn rar5_file(extra: &[u8]) -> Vec<u8> {
        let flags = if extra.is_empty() { 0x02 } else { 0x03 };
        rar5_block(2, flags, &rar5_file_fields(extra))
    }

    /// Offsets in `rar5_file_fields(&[])`: the mode's three bytes and the
    /// compression and host after the CRC.
    const RAR5_MODE: usize = 3;
    const RAR5_COMPRESSION: usize = 10;
    const RAR5_HOST: usize = 11;

    #[test]
    fn a_rar5_entry_that_is_no_regular_file_has_no_checksum() {
        let mut symlink = rar5_file_fields(&[]);
        symlink[RAR5_MODE..RAR5_MODE + 3].copy_from_slice(&[0xff, 0xc3, 0x02]); // 0o120777
        let walked_symlink = walked(&rar5(&[rar5_block(2, 0x02, &symlink), rar5_end()]));
        assert_eq!(walked_symlink.headers[0].crc, None, "a symbolic link");

        let mut windows_dir = rar5_file_fields(&[]);
        windows_dir[RAR5_MODE..RAR5_MODE + 3].copy_from_slice(&[0x90, 0x80, 0x00]); // 0x10
        windows_dir[RAR5_HOST] = 0;
        let walked_dir = walked(&rar5(&[rar5_block(2, 0x02, &windows_dir), rar5_end()]));
        assert_eq!(walked_dir.headers[0].crc, None, "a Windows directory");

        let mut windows_file = rar5_file_fields(&[]);
        windows_file[RAR5_MODE..RAR5_MODE + 3].copy_from_slice(&[0xa0, 0x80, 0x00]); // 0x20
        windows_file[RAR5_HOST] = 0;
        let walked_file = walked(&rar5(&[rar5_block(2, 0x02, &windows_file), rar5_end()]));
        assert_eq!(
            walked_file.headers[0].crc,
            Some(0x1234_5678),
            "a Windows file"
        );

        let mut other_host = rar5_file_fields(&[]);
        other_host[RAR5_HOST] = 2;
        let walked_other = walked(&rar5(&[rar5_block(2, 0x02, &other_host), rar5_end()]));
        assert_eq!(
            walked_other.headers[0].crc, None,
            "a host libarchive refuses"
        );
    }

    /// libarchive lists a split file at the block that starts it. That block
    /// holds only part of the file, so it has no checksum to meet, and a block
    /// carrying on from the previous volume holds no position at all: counting
    /// one would give every later entry its neighbour's checksum.
    #[test]
    fn a_rar5_split_block_has_no_checksum() {
        let whole = Header {
            size: 5,
            crc: Some(0x1234_5678),
        };
        let with_flag = |flag: u8| {
            let block = rar5_block(2, 0x02 | flag, &rar5_file_fields(&[]));
            walked(&rar5(&[block, rar5_file(&[]), rar5_end()])).headers
        };
        let starts = Header { size: 5, crc: None };
        assert_eq!(with_flag(0x10), [starts, whole], "goes on into the next");
        assert_eq!(with_flag(0x08), [whole], "continued from the previous");
        assert_eq!(with_flag(0x18), [whole], "continued and going on");
    }

    #[test]
    fn a_rar5_name_running_past_its_header_stops_the_walk() {
        let mut fields = rar5_file_fields(&[]);
        fields[RAR5_HOST + 1] = 0x7f; // the name's length
        let got = walked(&rar5(&[rar5_block(2, 0x02, &fields), rar5_end()]));
        assert_eq!(got.stopped, Some("a block header cannot be read"));
        assert!(got.headers.is_empty());
    }

    #[test]
    fn a_walk_knows_a_solid_archive_by_its_main_header_or_a_file() {
        let plain = walked(&rar5(&[rar5_block(1, 0, &[0]), rar5_file(&[]), rar5_end()]));
        assert!(!plain.solid, "neither says so");
        let by_main = walked(&rar5(&[rar5_block(1, 0, &[0x04]), rar5_end()]));
        assert!(by_main.solid, "the main header");
        let mut fields = rar5_file_fields(&[]);
        fields[RAR5_COMPRESSION] = 0x40;
        let by_file = walked(&rar5(&[rar5_block(2, 0x02, &fields), rar5_end()]));
        assert!(by_file.solid, "a file block");
    }

    #[test]
    fn a_rar5_file_block_gives_its_checksum_unless_its_extra_area_rules_it_out() {
        let plain = walked(&rar5(&[rar5_file(&[]), rar5_end()]));
        let want = Header {
            size: 5,
            crc: Some(0x1234_5678),
        };
        assert_eq!(plain.headers, [want]);
        assert_eq!(plain.stopped, None);

        // Records of two bytes: a size of 1, then the type.
        for (kind, crc) in [
            (1, None),
            (5, None),
            (2, Some(0x1234_5678)),
            (3, Some(0x1234_5678)),
        ] {
            let walked = walked(&rar5(&[rar5_file(&[1, kind]), rar5_end()]));
            assert_eq!(
                walked.headers,
                [Header { size: 5, crc }],
                "record type {kind}"
            );
        }
        for broken in [&[0_u8, 0][..], &[5, 1], &[0x80]] {
            let walked = walked(&rar5(&[rar5_file(broken), rar5_end()]));
            assert_eq!(
                walked.stopped,
                Some("a block header cannot be read"),
                "{broken:?}"
            );
        }
    }

    #[test]
    fn a_rar5_block_stops_or_ends_the_walk_by_its_type() {
        let stops = |block: Vec<u8>| walked(&rar5(&[block, rar5_file(&[])])).stopped;
        assert_eq!(
            stops(rar5_block(9, 0, &[])),
            Some("a block of an unknown type")
        );
        assert_eq!(stops(rar5_block(9, 0x04, &[])), None, "skip if unknown");
        assert_eq!(
            stops(rar5_block(4, 0, &[])),
            Some("a block header cannot be read")
        );
        let after_end = walked(&rar5(&[rar5_end(), rar5_file(&[])]));
        assert_eq!((after_end.stopped, after_end.headers.len()), (None, 0));
        let after_marker = walked(&rar5(&[rar5_block(0, 0, &[]), rar5_file(&[])]));
        assert_eq!(
            (after_marker.stopped, after_marker.headers.len()),
            (None, 0)
        );
    }

    #[test]
    fn hostile_rar5_sizes_stop_the_walk() {
        // A data size of 2^63, in a service block.
        let mut fields = vec![0x80; 9];
        fields.push(0x01);
        let block = rar5_block(3, 0x02, &fields);
        let walked_data = walked(&rar5(&[block, rar5_end()]));
        assert_eq!(
            walked_data.stopped,
            Some("a block runs past the end of the file")
        );

        for size in [
            // No header at all, a header past the limit, a vint that never ends.
            vec![0],
            vec![0x80, 0x80, 0x80, 0x01],
            vec![0xff; 11],
        ] {
            let mut bytes = rar5(&[]);
            bytes.extend([0; 4]);
            bytes.extend(&size);
            bytes.extend([0; 16]);
            assert_eq!(
                walked(&bytes).stopped,
                Some("a block header cannot be read"),
                "{size:?}"
            );
        }
    }

    /// libarchive's limit on a header, counted with the vint that gives its size.
    #[test]
    fn a_rar5_header_may_reach_the_limit_and_not_pass_it() {
        let header = |size: usize| {
            let size_bytes = [
                u8::try_from(size & 0x7f).expect("seven bits") | 0x80,
                u8::try_from(size >> 7 & 0x7f).expect("seven bits") | 0x80,
                u8::try_from(size >> 14 & 0x7f).expect("seven bits"),
            ];
            let mut block = vec![0; 4];
            block.extend(size_bytes);
            block.extend([3, 0]); // a service block with no flags
            block.resize(4 + 3 + size, 0);
            block
        };
        let limit = 2 * 1024 * 1024 - 3;
        let at_limit = walked(&rar5(&[header(limit), rar5_end()]));
        assert_eq!(at_limit.stopped, None);
        let past = walked(&rar5(&[header(limit + 1), rar5_end()]));
        assert_eq!(past.stopped, Some("a block header cannot be read"));
    }

    /// A RAR4 file block: the base block, then the file fields, large if asked.
    fn rar4_file(flags: u16, packed: u32, high_packed: Option<u32>) -> Vec<u8> {
        let flags = flags | if high_packed.is_some() { 0x0100 } else { 0 };
        let size: u16 = if high_packed.is_some() { 40 } else { 32 };
        let mut block = vec![0, 0, 0x74];
        block.extend(flags.to_le_bytes());
        block.extend(size.to_le_bytes());
        block.extend(packed.to_le_bytes());
        block.extend(5_u32.to_le_bytes());
        block.push(3); // host Unix
        block.extend(0x1234_5678_u32.to_le_bytes());
        block.extend([0; 4]); // time
        block.extend([29, 0x30]); // version, method
        block.extend(0_u16.to_le_bytes()); // no name
        block.extend(0o100_644_u32.to_le_bytes());
        if let Some(high) = high_packed {
            block.extend(high.to_le_bytes());
            block.extend(0_u32.to_le_bytes());
        }
        block
    }

    fn rar4(blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = b"Rar!\x1a\x07\x00".to_vec();
        bytes.extend(blocks.concat());
        bytes
    }

    #[test]
    fn a_rar4_file_block_gives_its_checksum_unless_its_flags_rule_it_out() {
        let want = |crc| [Header { size: 5, crc }];
        assert_eq!(
            walked(&rar4(&[rar4_file(0, 0, None)])).headers,
            want(Some(0x1234_5678))
        );
        for flags in [0x01, 0x02, 0x04] {
            let walked = walked(&rar4(&[rar4_file(flags, 0, None)]));
            assert_eq!(walked.headers, want(None), "flags {flags:#x}");
        }
        let mut symlink = rar4_file(0, 0, None);
        symlink[28..32].copy_from_slice(&0o120_777_u32.to_le_bytes());
        assert_eq!(
            walked(&rar4(&[symlink])).headers,
            want(None),
            "a symbolic link"
        );
        let mut directory = rar4_file(0, 0, None);
        directory[15] = 2; // host Windows
        directory[28..32].copy_from_slice(&0x10_u32.to_le_bytes());
        assert_eq!(
            walked(&rar4(&[directory])).headers,
            want(None),
            "a directory"
        );
    }

    #[test]
    fn hostile_rar4_blocks_stop_the_walk() {
        let past_end = Some("a block runs past the end of the file");
        assert_eq!(
            walked(&rar4(&[rar4_file(0, u32::MAX, None)])).stopped,
            past_end
        );
        let large = walked(&rar4(&[rar4_file(0, u32::MAX, Some(u32::MAX))]));
        assert_eq!((large.stopped, large.headers.len()), (past_end, 1));

        let bad = Some("a block header cannot be read");
        let mut short = rar4_file(0, 0, None);
        short[5..7].copy_from_slice(&31_u16.to_le_bytes());
        assert_eq!(
            walked(&rar4(&[short])).stopped,
            bad,
            "a file header too short"
        );
        assert_eq!(
            walked(&rar4(&[vec![0, 0, 0x74]])).stopped,
            bad,
            "cut in its base"
        );
        let unknown = vec![0, 0, 0x7f, 0, 0, 7, 0];
        assert_eq!(
            walked(&rar4(&[unknown])).stopped,
            Some("a block of an unknown type")
        );
        let mark = vec![b'R', 0, 0x72, 0, 0, 7, 0];
        assert_eq!(
            walked(&rar4(&[mark])).stopped,
            bad,
            "a marker that is not the signature"
        );
    }

    /// A file of `len` zero bytes but for `blocks`, each at its offset: room
    /// for an entry above 4 GiB without writing one.
    struct Sparse {
        len: u64,
        at: u64,
        blocks: Vec<(u64, Vec<u8>)>,
    }

    impl Read for Sparse {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let left = usize::try_from(self.len.saturating_sub(self.at)).unwrap_or(usize::MAX);
            let count = buf.len().min(left);
            buf[..count].fill(0);
            for (start, block) in &self.blocks {
                for (offset, byte) in (*start..).zip(block) {
                    let to = offset.checked_sub(self.at);
                    let to = to.and_then(|to| usize::try_from(to).ok());
                    if let Some(slot) = to.and_then(|to| buf[..count].get_mut(to)) {
                        *slot = *byte;
                    }
                }
            }
            self.at += count as u64;
            Ok(count)
        }
    }

    impl Seek for Sparse {
        fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
            self.at = match to {
                SeekFrom::Start(at) => at,
                SeekFrom::End(by) => self.len.saturating_add_signed(by),
                SeekFrom::Current(by) => self.at.saturating_add_signed(by),
            };
            Ok(self.at)
        }
    }

    /// Both halves of both sizes count: the next block is found past 4 GiB of
    /// data, and the entry's size is the one libarchive will list.
    #[test]
    fn a_large_rar4_entry_is_measured_by_both_halves_of_its_sizes() {
        let mut large = rar4_file(0, 8, Some(1));
        large[36..40].copy_from_slice(&2_u32.to_le_bytes());
        let next = 7 + 40 + (1_u64 << 32) + 8;
        let mut file = Sparse {
            len: next + 7,
            at: 0,
            blocks: vec![(0, rar4(&[large])), (next, vec![0, 0, 0x7b, 0, 0, 7, 0])],
        };
        let got = walk(&mut file).expect("a walk of memory does not fail");
        assert_eq!(got.stopped, None);
        assert_eq!(
            got.headers,
            [Header {
                size: (2 << 32) | 5,
                crc: Some(0x1234_5678),
            }]
        );
    }

    #[test]
    fn a_large_rar4_block_counts_the_high_half_of_its_packed_size() {
        let got = walked(&rar4(&[rar4_file(0, 0, Some(1))]));
        assert_eq!(got.stopped, Some("a block runs past the end of the file"));
    }

    /// Declared one byte too short for its high halves, with an end block
    /// where a header of that size would stop.
    #[test]
    fn a_large_rar4_header_must_hold_its_high_halves() {
        let mut short = rar4_file(0, 0, Some(0));
        short.truncate(39);
        short[5..7].copy_from_slice(&39_u16.to_le_bytes());
        short.extend([0, 0, 0x7b, 0, 0, 7, 0]);
        assert_eq!(
            walked(&rar4(&[short])).stopped,
            Some("a block header cannot be read")
        );
    }

    #[test]
    fn a_rar4_main_header_must_reach_the_size_libarchive_accepts() {
        let main = |size: u16| {
            let mut block = vec![0, 0, 0x73, 0, 0];
            block.extend(size.to_le_bytes());
            block.resize(usize::from(size), 0);
            block
        };
        assert_eq!(
            walked(&rar4(&[main(12)])).stopped,
            Some("a block header cannot be read")
        );
        assert_eq!(walked(&rar4(&[main(13)])).stopped, None);
    }

    /// libarchive takes a file's type from its attributes by the rules of the
    /// host that wrote it, so each host's regular file keeps its checksum.
    #[test]
    fn a_rar4_file_keeps_its_checksum_on_every_host_libarchive_reads() {
        let headers = |host: u8, attributes: u32| {
            let mut file = rar4_file(0, 0, None);
            file[15] = host;
            file[28..32].copy_from_slice(&attributes.to_le_bytes());
            walked(&rar4(&[file])).headers
        };
        let checked = [Header {
            size: 5,
            crc: Some(0x1234_5678),
        }];
        let unchecked = [Header { size: 5, crc: None }];
        for host in 0..=2 {
            assert_eq!(headers(host, 0x20), checked, "host {host}: a file");
            assert_eq!(headers(host, 0x10), unchecked, "host {host}: a directory");
        }
        for host in 3..=5 {
            assert_eq!(headers(host, 0o100_644), checked, "host {host}: a file");
            assert_eq!(
                headers(host, 0o040_755),
                unchecked,
                "host {host}: a directory"
            );
        }
        assert_eq!(
            headers(6, 0o100_644),
            unchecked,
            "a host libarchive refuses"
        );
    }

    /// A block of an old type, with these bytes after its seven-byte base.
    fn rar4_old_block(kind: u8, flags: u16, size: u16, rest: &[u8]) -> Vec<u8> {
        let mut block = vec![0, 0, kind];
        block.extend(flags.to_le_bytes());
        block.extend(size.to_le_bytes());
        block.extend_from_slice(rest);
        block
    }

    #[test]
    fn a_rar4_old_block_without_added_data_is_stepped_over() {
        for kind in 0x75..=0x79 {
            let old = rar4_old_block(kind, 0, 7, &[]);
            let got = walked(&rar4(&[old, rar4_file(0, 0, None)]));
            assert_eq!((got.stopped, got.headers.len()), (None, 1), "{kind:#x}");
        }
    }

    #[test]
    fn a_rar4_old_block_skips_the_data_it_declares() {
        let mut rest = 4_u32.to_le_bytes().to_vec();
        rest.extend([0xff; 4]);
        let old = rar4_old_block(0x78, 0x8000, 11, &rest);
        let got = walked(&rar4(&[old, rar4_file(0, 0, None)]));
        assert_eq!((got.stopped, got.headers.len()), (None, 1));
    }

    #[test]
    fn a_rar4_old_block_too_short_for_what_it_declares_stops_the_walk() {
        let bad = Some("a block header cannot be read");
        let no_base = rar4_old_block(0x75, 0, 6, &[0; 8]);
        assert_eq!(walked(&rar4(&[no_base])).stopped, bad, "under its base");
        let no_size = rar4_old_block(0x78, 0x8000, 7, &[0; 8]);
        assert_eq!(walked(&rar4(&[no_size])).stopped, bad, "no added size");
    }

    /// A comment or recovery record holds data the walk must not read as blocks.
    #[test]
    fn a_rar4_service_block_is_stepped_over_with_its_data() {
        let mut service = rar4_file(0, 4, None);
        service[2] = 0x7a;
        let mut bytes = rar4(&[service]);
        bytes.extend([0xff; 4]);
        bytes.extend(rar4_file(0, 0, None));
        let got = walked(&bytes);
        assert_eq!((got.stopped, got.headers.len()), (None, 1));
    }

    /// Listing past the entry read would decode the rest of a solid archive,
    /// so the walk's own verdict is all that can refuse a cut one.
    #[test]
    fn a_solid_read_refuses_a_walk_that_stopped() {
        let fixture = fixture("rar5-solid.rar");
        let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
        let dir = TempDir::new("rar-solid-cut");
        let path = dir.join("cut.rar");
        std::fs::write(&path, &bytes[..bytes.len() - 12]).expect("the copy writes");
        for backend in BACKENDS {
            let error = backend
                .read(&path, "p1.png")
                .expect_err("the walk ran past the end of the file");
            assert!(error.contains("runs past the end"), "{error}");
        }
    }

    #[test]
    fn a_listing_must_agree_with_the_walk_entry_by_entry() {
        let differ = "its block headers and its listing differ";
        let checked = Header {
            size: 5,
            crc: Some(7),
        };
        let unchecked = Header { size: 9, crc: None };
        let walk = Walk {
            headers: vec![checked, unchecked],
            ..Walk::default()
        };
        assert_eq!(walk.expected(0, Some(5)), Ok(Some(7)));
        assert_eq!(walk.expected(0, Some(4)), Err(differ), "another size");
        assert_eq!(walk.expected(0, None), Err(differ), "not a regular file");
        assert_eq!(walk.expected(1, None), Ok(None), "nothing to meet");
        assert_eq!(walk.expected(1, Some(1)), Ok(None), "nothing to meet");
        assert_eq!(
            walk.expected(2, Some(1)),
            Err(differ),
            "an entry the walk lacks"
        );
        assert_eq!(walk.finished(2), Ok(()));
        assert_eq!(walk.finished(1), Err(differ), "a listing that ended early");
        assert_eq!(walk.finished(3), Err(differ), "a listing that ran on");

        let cut = Walk {
            headers: vec![checked],
            stopped: Some("cut"),
            solid: false,
        };
        assert_eq!(cut.expected(0, Some(5)), Ok(Some(7)), "read before the cut");
        assert_eq!(
            cut.expected(1, Some(5)),
            Err("cut"),
            "the cause, past the cut"
        );
        assert_eq!(cut.finished(1), Err("cut"), "however far the listing got");
    }
}

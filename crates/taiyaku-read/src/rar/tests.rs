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
    page.or(other).map(|(_, sha)| sha.as_str())
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
                // A reference's size is its target's, as the manifest gives it.
                if want.kind != RarEntryKind::Dir {
                    assert_eq!(entry.size, want.size, "{at}: {}", want.name);
                }
                assert_eq!(
                    entry.target.is_some(),
                    want.kind == RarEntryKind::Reference,
                    "{at}: {}",
                    want.name
                );
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
            for (name, sha) in fixture.pages.iter().chain(&fixture.others) {
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

/// An archive refused at listing is refused by every call, a read of an entry
/// before the one that refuses it too, and with the same cause, so no page of
/// it reads by another way in.
#[test]
fn a_refused_fixture_is_refused_by_every_call() {
    let mut calls = 0;
    for backend in BACKENDS {
        for fixture in fixtures() {
            let Some(text) = &fixture.refused else {
                continue;
            };
            let at = format!("{} on {}", backend.name(), fixture.file);
            let path = &fixture.path;
            let mut got = vec![("listing", backend.entries(path).map(drop))];
            for entry in &fixture.entries {
                got.push((
                    entry.name.as_str(),
                    backend.read(path, &entry.name).map(drop),
                ));
            }
            got.push(("visit", visited(*backend, path, &|_| true).0));
            for (call, got) in got {
                let error = refusal(got, path, &format!("{at}: {call}"));
                assert!(error.contains(text.as_str()), "{at}: {call}: {error}");
                calls += 1;
            }
        }
    }
    assert!(calls > 0, "no refused fixture was called");
}

/// The listing gives each entry the CRC32 its header declares, so that a whole
/// read can be known without decoding: a reference's is its target's, and none
/// where the header declares a BLAKE2 hash alone or there are no bytes.
#[test]
fn a_listing_gives_each_entry_the_crc32_its_header_declares() {
    let mut checked = 0;
    for backend in BACKENDS {
        for fixture in fixtures().into_iter().filter(|f| f.refused.is_none()) {
            let at = format!("{} on {}", backend.name(), fixture.file);
            let listed = backend
                .entries(&fixture.path)
                .unwrap_or_else(|e| panic!("{at}: {e}"));
            assert_eq!(listed.len(), fixture.entries.len(), "{at}");
            for (entry, want) in listed.iter().zip(&fixture.entries) {
                let at = format!("{at}: {}", want.name);
                match want.kind {
                    RarEntryKind::File if fixture.file == "rar5-blake2.rar" => {
                        assert_eq!(entry.crc, None, "{at}: BLAKE2 alone");
                    }
                    RarEntryKind::File => {
                        let bytes = backend.read(&fixture.path, &want.name);
                        let bytes = bytes.unwrap_or_else(|e| panic!("{at}: {e}"));
                        assert_eq!(entry.crc, Some(crc32fast::hash(&bytes)), "{at}");
                        checked += 1;
                    }
                    // rar declares the CRC32 of no bytes.
                    RarEntryKind::Empty => assert_eq!(entry.crc, Some(0), "{at}"),
                    RarEntryKind::Dir => assert_eq!(entry.crc, None, "{at}"),
                    RarEntryKind::Reference => {
                        let target = entry.target.and_then(|target| listed.get(target));
                        let want = target.and_then(|target| target.crc);
                        assert!(want.is_some(), "{at}: {entry:?}");
                        assert_eq!(entry.crc, want, "{at}");
                    }
                    RarEntryKind::Encrypted | RarEntryKind::Split => {
                        panic!("{at}: listed, where its archive must be refused");
                    }
                }
            }
        }
    }
    assert!(checked > 0, "no entry's CRC32 was compared");
}

#[test]
fn a_visit_decodes_wanted_readable_entries_in_archive_order() {
    for backend in BACKENDS {
        for fixture in fixtures() {
            let at = format!("{} on {}", backend.name(), fixture.file);
            let (got, seen) = visited(*backend, &fixture.path, &|_| true);
            if let Some(text) = &fixture.refused {
                let error = refusal(got, &fixture.path, &at);
                assert!(error.contains(text.as_str()), "{at}: {error}");
                // A visit stops at the entry that refuses the archive, and may
                // hand over those before it, as a solid RAR4's first.
                assert!(seen.len() < fixture.entries.len().max(1), "{at}: {seen:?}");
                for (got, want) in seen.iter().zip(&fixture.entries) {
                    assert_eq!(got.name, want.name, "{at}");
                    assert_eq!(got.len, want.size, "{at}: {}", got.name);
                }
                continue;
            }
            got.unwrap_or_else(|e| panic!("{at}: {e}"));
            let readable: Vec<_> = fixture
                .entries
                .iter()
                .filter(|entry| entry.kind != RarEntryKind::Dir)
                .collect();
            assert_eq!(seen.len(), readable.len(), "{at}: {seen:?}");
            for (got, want) in seen.iter().zip(&readable) {
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

/// A file reference lists with the position, size and CRC32 of the earlier
/// entry whose bytes it reads.
#[test]
fn a_reference_lists_as_the_earlier_entry_it_reads() {
    let fixture = fixture("rar5-file-reference.rar");
    for backend in BACKENDS {
        let at = format!("{} on {}", backend.name(), fixture.file);
        let listed = backend
            .entries(&fixture.path)
            .unwrap_or_else(|e| panic!("{at}: {e}"));
        let targets: Vec<_> = listed.iter().map(|entry| entry.target).collect();
        assert_eq!(targets, [None, Some(0)], "{at}");
        let (reference, target) = (&listed[1], &listed[0]);
        assert_eq!(
            (reference.size, reference.crc),
            (target.size, target.crc),
            "{at}"
        );
        assert!(target.crc.is_some(), "{at}: {target:?}");
    }
}

/// A file reference or hard link, which libarchive lists as a regular file and
/// gives no bytes for, reads its target's: by name, by position, and in a visit
/// that wants it and not its target, which comes before it.
#[test]
fn a_reference_reads_its_targets_bytes() {
    for (file, reference, target) in [
        ("rar5-file-reference.rar", "p02.png", "p01.png"),
        ("rar5-hard-link.rar", "p02.png", ".p02.png"),
    ] {
        let fixture = fixture(file);
        let path = &fixture.path;
        for backend in BACKENDS {
            let at = format!("{} on {file}", backend.name());
            let want = backend.read(path, target);
            let want = want.unwrap_or_else(|e| panic!("{at}: {target}: {e}"));
            let sha = sha256_hex(&want);
            assert_eq!(sha_of(&fixture, reference), Some(sha.as_str()), "{at}");
            let by_name = backend.read(path, reference);
            assert_eq!(by_name.as_ref(), Ok(&want), "{at}: by name");
            let by_position = backend.read_listed(path, &mut |entries| {
                let position = entries.iter().position(|entry| entry.name == reference);
                Ok(position.expect("the reference is listed"))
            });
            assert_eq!(by_position.as_ref(), Ok(&want), "{at}: by position");
            let (got, seen) = visited(*backend, path, &|name| name == reference);
            got.unwrap_or_else(|e| panic!("{at}: {e}"));
            let seen: Vec<_> = seen
                .iter()
                .map(|got| (got.name.as_str(), got.sha.as_str()))
                .collect();
            assert_eq!(seen, [(reference, sha.as_str())], "{at}: a visit");
        }
    }
}

/// `read_listed` reads a regular file the listing gave: a directory's position
/// or one past the listing is refused, naming the path, and an error from the
/// pick comes back as it is.
#[test]
fn a_listed_read_refuses_a_position_that_is_no_regular_file() {
    let fixture = fixture("rar5-layout.rar");
    let path = &fixture.path;
    let directory = fixture
        .entries
        .iter()
        .position(|entry| entry.kind == RarEntryKind::Dir);
    let directory = directory.expect("a directory entry");
    let past = fixture.entries.len();
    let shown = path.display();
    let named = &fixture.entries[directory].name;
    let cases = [
        (directory, format!("{shown}: {named}: not a regular file")),
        (past, format!("{shown}: no entry at {past}")),
    ];
    for backend in BACKENDS {
        for (position, cause) in &cases {
            let at = format!("{}: {cause}", backend.name());
            let got = backend.read_listed(path, &mut |_| Ok(*position));
            assert_eq!(refusal(got, path, &at), *cause, "{at}");
        }
        let got = backend.read_listed(path, &mut |_| Err("picked none".to_owned()));
        assert_eq!(got, Err("picked none".to_owned()), "{}", backend.name());
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
                assert_eq!(sha256_hex(&bytes), *sha, "{}: {name}", backend.name());
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
    fn signature(&self) -> String {
        self.name().to_owned()
    }
    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String> {
        Err(path.display().to_string())
    }
    fn read(&self, path: &Path, _: &str) -> Result<Vec<u8>, String> {
        Err(path.display().to_string())
    }
    fn read_listed(
        &self,
        path: &Path,
        _: &mut dyn FnMut(&[Entry]) -> Result<usize, String>,
    ) -> Result<Vec<u8>, String> {
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

    const ONE_VOLUME: &str = "it is one volume of a multi-volume set";
    const ENCRYPTED: &str = "an entry is encrypted";
    const UNDECODED: &str =
        "an entry uses RAR 2.0 compression or another method libarchive does not decode";
    const DANGLING: &str =
        "an entry is a reference or hard link, and its target is not an earlier entry of that name";
    const RESIZED: &str = "an entry is a reference or hard link, and its size is not its target's";
    const UNREAD_REDIRECTION: &str =
        "an entry is a junction or a redirection of a type libarchive does not read";
    const MARKED_DIRECTORY: &str =
        "an entry's header marks it a directory and its attributes a file";

    fn walked(bytes: &[u8]) -> Walk {
        walk(&mut Cursor::new(bytes)).expect("a walk of memory does not fail")
    }

    /// Every entry of every listable fixture, in order, with the checksum of
    /// the bytes it holds and none where they cannot be checked. A fixture
    /// refused for what the walk finds stops it there, with that cause.
    #[test]
    fn a_walk_gives_each_entry_its_own_checksum() {
        let mut checked = 0;
        let mut hashed = 0;
        // Headers encrypted: the walk has nothing of its entries to give.
        for fixture in fixtures().into_iter().filter(|f| !f.entries.is_empty()) {
            let at = fixture.file.as_str();
            let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
            let walked = walked(&bytes);
            let stop = fixture.refused.as_deref().and_then(|text| {
                [ONE_VOLUME, ENCRYPTED, UNDECODED, DANGLING]
                    .into_iter()
                    .find(|cause| text.ends_with(cause))
            });
            assert_eq!(walked.stopped, stop, "{at}");
            // A walk stops where the entry that refuses the archive starts, or
            // at a volume's main header, before any entry.
            let first = |kind| fixture.entries.iter().position(|entry| entry.kind == kind);
            let kept = match stop {
                None => Some(fixture.entries.len()),
                Some(ONE_VOLUME) => Some(0),
                Some(ENCRYPTED) => first(RarEntryKind::Encrypted),
                Some(DANGLING) => first(RarEntryKind::Reference),
                // Every file of a RAR 2.0 fixture is compressed so.
                Some(UNDECODED) => first(RarEntryKind::File),
                Some(cause) => panic!("{at}: no stop position for {cause}"),
            };
            assert_eq!(Some(walked.headers.len()), kept, "{at}");
            let solid = matches!(at, "rar5-solid.rar" | "rar5-solid-empty.rar");
            assert_eq!(walked.solid, solid, "{at}");
            assert_eq!(walked.rar4, fixture.format == "rar4", "{at}");
            for (header, entry) in walked.headers.iter().zip(&fixture.entries) {
                let at = format!("{at}: {}", entry.name);
                match entry.kind {
                    // rar -htb writes a BLAKE2sp hash in place of the CRC32.
                    // The read below is held to it, so a hash the walk took
                    // from elsewhere in the header would fail it too.
                    RarEntryKind::File if at.starts_with("rar5-blake2.rar") => {
                        assert_eq!(header.crc, None, "{at}: BLAKE2 alone");
                        let read = BACKENDS[0].read(&fixture.path, &entry.name);
                        let read = read.unwrap_or_else(|e| panic!("{at}: {e}"));
                        let want = blake2s_simd::blake2sp::blake2sp(&read);
                        assert_eq!(
                            header.blake2.as_deref().map(Vec::from),
                            Some(want.as_bytes().to_vec()),
                            "{at}"
                        );
                        assert_eq!(header.size, entry.size, "{at}");
                        hashed += 1;
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
                    // rar declares the CRC32 of no bytes.
                    RarEntryKind::Empty => {
                        assert_eq!((header.size, header.crc), (0, Some(0)), "{at}");
                    }
                    RarEntryKind::Dir
                    | RarEntryKind::Encrypted
                    | RarEntryKind::Reference
                    | RarEntryKind::Split => assert_eq!(header.crc, None, "{at}"),
                }
                if !at.starts_with("rar5-blake2.rar") {
                    assert_eq!(header.blake2, None, "{at}: no BLAKE2 hash");
                }
                assert_eq!(
                    header.target.is_some(),
                    entry.kind == RarEntryKind::Reference,
                    "{at}"
                );
            }
        }
        assert!(checked > 0, "no entry's checksum was compared");
        assert!(hashed > 0, "no entry's BLAKE2 hash was compared");
    }

    #[test]
    fn a_file_without_a_rar_signature_stops_the_walk() {
        let walked = walked(b"PK\x03\x04 and not a RAR at all");
        assert_eq!(
            walked.stopped,
            Some("it does not start with a RAR signature")
        );
        assert!(walked.headers.is_empty());
        assert!(!walked.rar4, "no RAR4 either");
    }

    /// A cut anywhere leaves the headers before it, and a file cut inside a
    /// block says so.
    #[test]
    fn a_cut_walk_keeps_the_headers_before_the_cut() {
        for file in ["rar5-compressed.rar", "rar4-comment.rar"] {
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

    /// Offsets in `rar5_file_fields(&[])`: the file flags, the size, the
    /// mode's three bytes, and the compression and host after the CRC.
    const RAR5_FILE_FLAGS: usize = 1;
    const RAR5_SIZE: usize = 2;
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
            target: None,
            blake2: None,
        };
        let with_flag = |flag: u8| {
            let block = rar5_block(2, 0x02 | flag, &rar5_file_fields(&[]));
            walked(&rar5(&[block, rar5_file(&[]), rar5_end()])).headers
        };
        let starts = Header {
            size: 5,
            crc: None,
            target: None,
            blake2: None,
        };
        assert_eq!(
            with_flag(0x10),
            [starts, whole.clone()],
            "goes on into the next"
        );
        assert_eq!(
            with_flag(0x08),
            std::slice::from_ref(&whole),
            "continued from the previous"
        );
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
            target: None,
            blake2: None,
        };
        assert_eq!(plain.headers, std::slice::from_ref(&want));
        assert_eq!(plain.stopped, None);

        // A record of two bytes: a size of 1, then the type, 3, a time record
        // whose fields libarchive reads and this walk does not need. Type 1,
        // encryption, stops the walk instead.
        let timed = walked(&rar5(&[rar5_file(&[1, 3]), rar5_end()]));
        assert_eq!(timed.headers, [want], "record type 3");
        // Type 5, redirection: the entry is a link, so its bytes are not its own.
        let linked = walked(&rar5(&[
            rar5_file(&redirect(UNIX_SYMLINK, b'z')),
            rar5_end(),
        ]));
        let unchecked = Header {
            size: 5,
            crc: None,
            target: None,
            blake2: None,
        };
        assert_eq!(linked.headers, [unchecked], "a symbolic link");
        // The fourth and fifth: a redirection record of its type alone, and one
        // whose name runs past it. The last five: a hash record of its type
        // alone, one cut inside its hash, one of a hash type libarchive refuses
        // as it lists, one whose hash type does not fit it, and one with a byte
        // after its hash, which libarchive reads as the next record's start.
        let short = hash_record(BLAKE2SP, &[7; 31]);
        let unknown = hash_record(1, &[7; 32]);
        let long = hash_record(BLAKE2SP, &[7; 33]);
        let broken: [&[u8]; 10] = [
            &[0, 0],
            &[5, 1],
            &[0x80],
            &[1, 5],
            &[4, 5, 4, 0, 1],
            &[1, 2],
            &short,
            &unknown,
            &[2, 2, 0x80],
            &long,
        ];
        for broken in broken {
            let walked = walked(&rar5(&[rar5_file(broken), rar5_end()]));
            assert_eq!(
                walked.stopped,
                Some("a block header cannot be read"),
                "{broken:?}"
            );
        }
    }

    /// RAR5's file hash type for BLAKE2sp, the only one libarchive lists.
    const BLAKE2SP: u8 = 0;

    /// A file hash record of hash type `kind` holding `hash`: its size, then
    /// its type (2), the hash type and the hash.
    fn hash_record(kind: u8, hash: &[u8]) -> Vec<u8> {
        let size = u8::try_from(2 + hash.len()).expect("a small record");
        let mut record = vec![size, 2, kind];
        record.extend_from_slice(hash);
        record
    }

    /// A file made with `rar -htb` declares its BLAKE2sp hash in a record of
    /// its extra area, which applies to its bytes by the CRC32's own rule:
    /// none for a link, a reference or the part of a file a volume holds.
    #[test]
    fn a_rar5_hash_record_gives_its_blake2_hash_by_the_crc32s_rule() {
        let hash: [u8; 32] = std::array::from_fn(|at| u8::try_from(at).expect("under 32"));
        let record = hash_record(BLAKE2SP, &hash);
        let hashed = Header {
            size: 5,
            crc: Some(0x1234_5678),
            target: None,
            blake2: Some(Box::new(hash)),
        };
        let file = walked(&rar5(&[rar5_file(&record), rar5_end()]));
        assert_eq!(
            (file.headers, file.stopped),
            (vec![hashed.clone()], None),
            "a file"
        );
        // With no CRC32 too, as rar -htb writes it.
        let mut fields = rar5_file_fields(&record);
        fields[RAR5_FILE_FLAGS + 1] &= !0x04;
        fields.drain(RAR5_MODE + 4..RAR5_MODE + 8);
        let alone = walked(&rar5(&[rar5_block(2, 0x03, &fields), rar5_end()]));
        let want = Header {
            crc: None,
            ..hashed.clone()
        };
        assert_eq!(alone.headers, [want], "BLAKE2 alone");
        // Beside a record of another type, either side of it.
        for extra in [
            [&[1, 3], &record[..]].concat(),
            [&record[..], &[1, 3]].concat(),
        ] {
            let got = walked(&rar5(&[rar5_file(&extra), rar5_end()]));
            assert_eq!(got.headers, std::slice::from_ref(&hashed), "{extra:?}");
        }

        let unchecked = Header {
            crc: None,
            blake2: None,
            ..hashed
        };
        let linked = [&record[..], &redirect(UNIX_SYMLINK, b'z')].concat();
        let got = walked(&rar5(&[rar5_file(&linked), rar5_end()]));
        assert_eq!(
            got.headers,
            std::slice::from_ref(&unchecked),
            "a symbolic link"
        );
        let mut symlink = rar5_file_fields(&record);
        symlink[RAR5_MODE + 1..RAR5_MODE + 4].copy_from_slice(&[0xff, 0xc3, 0x02]); // 0o120777
        let got = walked(&rar5(&[rar5_block(2, 0x03, &symlink), rar5_end()]));
        assert_eq!(
            got.headers,
            std::slice::from_ref(&unchecked),
            "a symbolic link by its mode"
        );
        let split = rar5_block(2, 0x13, &rar5_file_fields(&record));
        let got = walked(&rar5(&[split, rar5_end()]));
        assert_eq!(
            got.headers,
            std::slice::from_ref(&unchecked),
            "goes on into the next volume"
        );
        let copied = [&record[..], &redirect(FILE_COPY, b'a')].concat();
        let got = walked(&rar5(&[
            rar5_named(b'a', &[]),
            rar5_named(b'b', &copied),
            rar5_end(),
        ]));
        let reference = Header {
            target: Some(0),
            ..unchecked
        };
        assert_eq!(got.headers[1], reference, "a reference");
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
        let want = |crc| {
            [Header {
                size: 5,
                crc,
                target: None,
                blake2: None,
            }]
        };
        assert_eq!(
            walked(&rar4(&[rar4_file(0, 0, None)])).headers,
            want(Some(0x1234_5678))
        );
        // 0x04, encryption, stops the walk instead.
        for flags in [0x01, 0x02] {
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

    /// `block` with `name` after its fields, its sizes made to fit.
    fn rar4_named(mut block: Vec<u8>, name: &[u8]) -> Vec<u8> {
        let size = u16::try_from(block.len() + name.len()).expect("a name that fits a header");
        block[5..7].copy_from_slice(&size.to_le_bytes());
        let name_size = u16::try_from(name.len()).expect("a name that fits a header");
        block[26..28].copy_from_slice(&name_size.to_le_bytes());
        block.extend(name);
        block
    }

    /// libarchive turns a RAR4 name's every 0x5C into `/` unless the name has
    /// a Unicode form, and in a legacy code page one after a byte above 0x7F
    /// may be half of a character instead: Shift-JIS's 表 is 95 5C.
    #[test]
    fn a_rar4_name_whose_backslash_may_be_half_a_character_stops_the_walk() {
        const AMBIGUOUS: Option<&str> = Some(
            "an entry's name, in a legacy code page, holds a byte that may be half of a character or a path separator",
        );
        const UNICODE: u16 = 0x0200;
        let stopped = |block: Vec<u8>| walked(&rar4(&[block])).stopped;
        let named = |flags, name: &[u8]| stopped(rar4_named(rar4_file(flags, 0, None), name));

        assert_eq!(named(0, b"\x95\\.png"), AMBIGUOUS);
        assert_eq!(named(0, b"p\x80\\.png"), AMBIGUOUS);
        // A zero byte ends a name with no Unicode flag, and what follows it is
        // no Unicode name.
        assert_eq!(named(0, b"\x95\\.png\0\x01"), AMBIGUOUS);
        assert_eq!(named(0x02, b"p\xff\\.png"), AMBIGUOUS, "a split entry");
        // Bytes that happen to be UTF-8 may still be Shift-JIS: C2 95 5C is
        // "ﾂ表" there, and E3 81 95 5C "縺表".
        assert_eq!(named(0, b"\xc2\x95\\.png"), AMBIGUOUS);
        assert_eq!(named(0, "\u{3055}\\.png".as_bytes()), AMBIGUOUS);
        let large = rar4_named(rar4_file(0, 0, Some(0)), b"\x95\\.png");
        assert_eq!(stopped(large), AMBIGUOUS, "after a large header's sizes");
        // A name past the first 512 bytes the walk reads of a header.
        let long = [&[b'a'; 600][..], b"\x95\\.png"].concat();
        assert_eq!(named(0, &long), AMBIGUOUS, "a long name");

        // Under the Unicode flag the name is Unicode, whether a Unicode name
        // follows a zero byte or the stored name is UTF-8, where 0x5C is no
        // half of a character; libarchive refuses one that is not UTF-8 itself.
        for (flags, name) in [
            (UNICODE, &b"\x95\\.png\0\x01"[..]),
            (UNICODE, "\u{8868}\\.png".as_bytes()),
            (UNICODE, b"\x95\\.png"),
            (0, b"a\\\x80.png"),
            (0, b"\\\x80.png"),
            (0, b"\x95\0\x95\\.png"),
            (0, b"\x95.png"),
            (0, b"p\x7f\\.png"),
            (0, &[&[b'a'; 600][..], b"\\.png"].concat()),
        ] {
            assert_eq!(named(flags, name), None, "{flags:#x} {name:x?}");
        }

        // A name that runs past its header, which libarchive refuses too.
        let mut past = rar4_named(rar4_file(0, 0, None), b"p.png");
        past[26..28].copy_from_slice(&6_u16.to_le_bytes());
        let end = vec![0, 0, 0x7b, 0, 0, 7, 0];
        let walked = walked(&rar4(&[past, end]));
        assert_eq!(walked.stopped, Some("a block header cannot be read"));
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
                target: None,
                blake2: None,
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

    /// The unpack version and the method of a RAR4 file block.
    const RAR4_VERSION: usize = 24;
    const RAR4_METHOD: usize = 25;

    /// Ruling P2-R3, with the set read from libarchive 3.7.4's RAR4 reader: it
    /// copies a stored entry's bytes (method 0x30) whatever version it names,
    /// decodes methods 0x31 to 0x35 as RAR 2.9 compresses them (version 29),
    /// and reads no version at all. Every other regular file with bytes stops
    /// the walk where it starts, every pair of the two bytes tried. A directory
    /// and a symbolic link are no page, and an empty file has nothing to decode.
    #[test]
    fn a_rar4_file_libarchive_does_not_decode_stops_the_walk() {
        const STORED: u8 = 0x30;
        const DECODED: [(u8, u8); 5] = [(29, 0x31), (29, 0x32), (29, 0x33), (29, 0x34), (29, 0x35)];
        let file = |version: u8, method: u8| {
            let mut file = rar4_file(0, 0, None);
            file[RAR4_VERSION] = version;
            file[RAR4_METHOD] = method;
            file
        };
        let walk = |middle: Vec<u8>| {
            let got = walked(&rar4(&[file(29, STORED), middle, file(29, STORED)]));
            (got.stopped, got.headers.len())
        };
        for version in 0..=u8::MAX {
            for method in 0..=u8::MAX {
                let want = if method == STORED || DECODED.contains(&(version, method)) {
                    (None, 3)
                } else {
                    (Some(UNDECODED), 1)
                };
                let at = format!("version {version}, method {method:#x}");
                assert_eq!(walk(file(version, method)), want, "{at}");
            }
        }
        let kinds = [
            ("a directory", 3, 0o040_755),
            ("a Windows directory", 2, 0x10),
            ("a symbolic link", 3, 0o120_777),
        ];
        for (what, host, attributes) in kinds {
            let mut entry = file(20, 0x33);
            entry[15] = host;
            entry[28..32].copy_from_slice(&u32::to_le_bytes(attributes));
            assert_eq!(walk(entry), (None, 3), "{what}");
        }
        let mut empty = file(20, 0x33);
        empty[11..15].copy_from_slice(&0_u32.to_le_bytes());
        assert_eq!(walk(empty), (None, 3), "an empty file");
    }

    /// RAR5 redirection types: a link elsewhere, and two that read another
    /// entry's bytes.
    const UNIX_SYMLINK: u8 = 1;
    const HARD_LINK: u8 = 4;
    const FILE_COPY: u8 = 5;

    /// A redirection record of `kind` naming the one byte `target`: its size,
    /// then its type (5), the kind, flags, the name's length and the name.
    fn redirect(kind: u8, target: u8) -> [u8; 6] {
        [5, 5, kind, 0, 1, target]
    }

    /// A RAR5 file named by the one byte `name`, with these extra records.
    fn rar5_named(name: u8, extra: &[u8]) -> Vec<u8> {
        rar5_sized(name, 5, extra)
    }

    /// [`rar5_named`], declaring `size` bytes.
    fn rar5_sized(name: u8, size: u8, extra: &[u8]) -> Vec<u8> {
        let mut fields = rar5_file_fields(extra);
        // An extra area's size moves every field after it on by one.
        let shift = usize::from(!extra.is_empty());
        fields[RAR5_SIZE + shift] = size;
        // The name follows the host.
        fields[RAR5_HOST + 2 + shift] = name;
        rar5_block(2, if extra.is_empty() { 0x02 } else { 0x03 }, &fields)
    }

    /// A hard link or file reference reads its target's bytes whole, so one
    /// that declares another size stops the walk where it starts, through
    /// another reference too.
    #[test]
    fn a_reference_of_another_size_than_its_target_stops_the_walk() {
        for kind in [HARD_LINK, FILE_COPY] {
            let at = |case: &str| format!("redirection type {kind}: {case}");
            let walk = |blocks: &[Vec<u8>]| {
                let mut blocks = blocks.to_vec();
                blocks.push(rar5_end());
                let got = walked(&rar5(&blocks));
                (got.stopped, got.headers.len())
            };
            let file = rar5_named(b'a', &[]);
            let to = |name, target, size| rar5_sized(name, size, &redirect(kind, target));
            assert_eq!(
                walk(&[file.clone(), to(b'b', b'a', 5)]),
                (None, 2),
                "{}",
                at("as large")
            );
            for size in [0, 4, 6] {
                let blocks = [file.clone(), to(b'b', b'a', size)];
                assert_eq!(
                    walk(&blocks),
                    (Some(RESIZED), 1),
                    "{}",
                    at(&size.to_string())
                );
            }
            let blocks = [file.clone(), to(b'b', b'a', 5), to(b'c', b'b', 6)];
            assert_eq!(
                walk(&blocks),
                (Some(RESIZED), 2),
                "{}",
                at("through another")
            );
        }
    }

    /// Ruling P2-R4: a hard link or file reference reads the first earlier
    /// regular file of the name its record gives, or what that file reads
    /// where it is one too. Any other stops the walk where it starts, as unrar
    /// resolves one only to an entry it has already extracted.
    #[test]
    fn a_reference_reads_an_earlier_regular_file_of_its_targets_name() {
        let walk = |blocks: &[Vec<u8>]| {
            let mut blocks = blocks.to_vec();
            blocks.push(rar5_end());
            let got = walked(&rar5(&blocks));
            let targets: Vec<_> = got.headers.iter().map(|header| header.target).collect();
            (got.stopped, targets)
        };
        let file = |name| rar5_named(name, &[]);
        let mut directory = rar5_file_fields(&[]);
        directory[RAR5_MODE..RAR5_MODE + 3].copy_from_slice(&[0xed, 0x83, 0x01]); // 0o40755
        let directory = rar5_block(2, 0x02, &directory);
        let symlink = rar5_named(b'a', &redirect(UNIX_SYMLINK, b'z'));
        for kind in [HARD_LINK, FILE_COPY] {
            let to = |name, target| rar5_named(name, &redirect(kind, target));
            let at = |case: &str| format!("redirection type {kind}: {case}");
            let reads = [
                (
                    vec![file(b'a'), to(b'b', b'a')],
                    vec![None, Some(0)],
                    "after it",
                ),
                (
                    vec![file(b'a'), file(b'a'), to(b'b', b'a')],
                    vec![None, None, Some(0)],
                    "the first of two",
                ),
                (
                    vec![file(b'a'), to(b'b', b'a'), to(b'c', b'b')],
                    vec![None, Some(0), Some(0)],
                    "through another",
                ),
            ];
            for (blocks, targets, case) in reads {
                assert_eq!(walk(&blocks), (None, targets), "{}", at(case));
            }
            let dangles = [
                (vec![file(b'a'), to(b'b', b'x')], 1, "no entry of that name"),
                (vec![to(b'b', b'a'), file(b'a')], 0, "a later one"),
                (vec![to(b'a', b'a')], 0, "itself"),
                (vec![file(b'A'), to(b'b', b'a')], 1, "another case"),
                (vec![directory.clone(), to(b'b', b'a')], 1, "a directory"),
                (vec![symlink.clone(), to(b'b', b'a')], 1, "a symbolic link"),
            ];
            for (blocks, kept, case) in dangles {
                assert_eq!(
                    walk(&blocks),
                    (Some(DANGLING), vec![None; kept]),
                    "{}",
                    at(case)
                );
            }
        }
    }

    /// libarchive lists a regular file redirected as a junction, or by a type
    /// it does not know, as a file it gives no bytes for, so the walk stops
    /// where it starts. A link or reference, and a junction whose attributes
    /// are a directory's, pass.
    #[test]
    fn a_file_redirected_as_no_link_or_reference_stops_the_walk() {
        const JUNCTION: u8 = 3;
        const WINDOWS_SYMLINK: u8 = 2;
        let walk = |redirected: Vec<u8>| {
            let got = walked(&rar5(&[rar5_named(b'a', &[]), redirected, rar5_end()]));
            (got.stopped, got.headers.len())
        };
        for kind in [JUNCTION, 9] {
            let redirected = rar5_named(b'b', &redirect(kind, b'a'));
            let at = format!("redirection type {kind}");
            assert_eq!(walk(redirected), (Some(UNREAD_REDIRECTION), 1), "{at}");
        }
        for kind in [UNIX_SYMLINK, WINDOWS_SYMLINK, HARD_LINK, FILE_COPY] {
            let redirected = rar5_named(b'b', &redirect(kind, b'a'));
            assert_eq!(walk(redirected), (None, 2), "redirection type {kind}");
        }
        // Each offset moves on by one for the extra area's size.
        let mut junction = rar5_file_fields(&redirect(JUNCTION, b'a'));
        junction[RAR5_MODE + 1..RAR5_MODE + 4].copy_from_slice(&[0x90, 0x80, 0x00]); // 0x10
        junction[RAR5_HOST + 1] = 0;
        junction[RAR5_HOST + 3] = b'b';
        let junction = rar5_block(2, 0x03, &junction);
        assert_eq!(walk(junction), (None, 2), "a Windows directory's junction");
    }

    /// A file block whose header marks it a directory, its attributes a
    /// regular file's, is one libarchive refuses as it lists it or cannot
    /// read, so the walk stops where it starts; a directory so marked passes.
    #[test]
    fn a_file_marked_a_directory_stops_the_walk() {
        let walk = |fields: &[u8]| {
            let marked = rar5_block(2, 0x02, fields);
            let got = walked(&rar5(&[rar5_named(b'a', &[]), marked, rar5_end()]));
            (got.stopped, got.headers.len())
        };
        let mut marked = rar5_file_fields(&[]);
        marked[RAR5_FILE_FLAGS] |= 0x01;
        marked[RAR5_HOST + 2] = b'b';
        assert_eq!(walk(&marked), (Some(MARKED_DIRECTORY), 1), "a regular file");
        marked[RAR5_MODE..RAR5_MODE + 3].copy_from_slice(&[0xed, 0x83, 0x01]); // 0o40755
        assert_eq!(walk(&marked), (None, 2), "a directory");
    }

    /// Ruling P2-R6: a main header that marks the archive one volume of a set
    /// stops the walk before any entry, in either format, and no other flag
    /// there does.
    #[test]
    fn a_volume_stops_the_walk_at_its_main_header() {
        let rar5_main = |flags: u8| {
            let blocks = [rar5_block(1, 0, &[flags]), rar5_file(&[]), rar5_end()];
            let walked = walked(&rar5(&blocks));
            (walked.stopped, walked.headers.len())
        };
        assert_eq!(rar5_main(0x01), (Some(ONE_VOLUME), 0), "RAR5");
        assert_eq!(rar5_main(0x04), (None, 1), "RAR5, solid");
        let rar4_main = |flags: u16| {
            let mut main = vec![0, 0, 0x73];
            main.extend(flags.to_le_bytes());
            main.extend(13_u16.to_le_bytes());
            main.resize(13, 0);
            let walked = walked(&rar4(&[main, rar4_file(0, 0, None)]));
            (walked.stopped, walked.headers.len())
        };
        assert_eq!(rar4_main(0x0001), (Some(ONE_VOLUME), 0), "RAR4");
        assert_eq!(rar4_main(0x0008), (None, 1), "RAR4, solid");
    }

    /// An encrypted entry stops the walk where it starts, in either format,
    /// keeping the entries before it.
    #[test]
    fn an_encrypted_entry_stops_the_walk_where_it_starts() {
        let plain = [Header {
            size: 5,
            crc: Some(0x1234_5678),
            target: None,
            blake2: None,
        }];
        // A RAR5 extra record of a size of 1, then type 1: encryption.
        let blocks = [
            rar5_file(&[]),
            rar5_file(&[1, 1]),
            rar5_file(&[]),
            rar5_end(),
        ];
        let got = walked(&rar5(&blocks));
        assert_eq!(
            (got.stopped, &got.headers[..]),
            (Some(ENCRYPTED), &plain[..]),
            "RAR5"
        );
        let files = [
            rar4_file(0, 0, None),
            rar4_file(0x04, 0, None),
            rar4_file(0, 0, None),
        ];
        let got = walked(&rar4(&files));
        assert_eq!(
            (got.stopped, &got.headers[..]),
            (Some(ENCRYPTED), &plain[..]),
            "RAR4"
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
            target: None,
            blake2: None,
        }];
        let unchecked = [Header {
            size: 5,
            crc: None,
            target: None,
            blake2: None,
        }];
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
            target: None,
            blake2: None,
        };
        let unchecked = Header {
            size: 9,
            crc: None,
            target: None,
            blake2: None,
        };
        let walk = Walk {
            headers: vec![checked.clone(), unchecked.clone()],
            ..Walk::default()
        };
        assert_eq!(walk.listed(0, Some(5)), Ok(&checked));
        assert_eq!(walk.listed(0, Some(4)), Err(differ), "another size");
        assert_eq!(walk.listed(0, None), Err(differ), "not a regular file");
        assert_eq!(walk.listed(1, None), Ok(&unchecked), "nothing to meet");
        assert_eq!(walk.listed(1, Some(1)), Ok(&unchecked), "nothing to meet");
        assert_eq!(
            walk.listed(2, Some(1)),
            Err(differ),
            "an entry the walk lacks"
        );
        assert_eq!(walk.finished(2), Ok(()));
        assert_eq!(walk.finished(1), Err(differ), "a listing that ended early");
        assert_eq!(walk.finished(3), Err(differ), "a listing that ran on");

        let hashed = Walk {
            headers: vec![Header {
                size: 5,
                crc: None,
                target: None,
                blake2: Some(Box::new([7; 32])),
            }],
            ..Walk::default()
        };
        let hashed_header = &hashed.headers[0];
        assert_eq!(hashed.listed(0, Some(5)), Ok(hashed_header), "BLAKE2 alone");
        assert_eq!(
            hashed.listed(0, Some(4)),
            Err(differ),
            "BLAKE2: another size"
        );
        assert_eq!(
            hashed.listed(0, None),
            Err(differ),
            "BLAKE2: no regular file"
        );

        let cut = Walk {
            headers: vec![checked.clone()],
            stopped: Some("cut"),
            solid: false,
            rar4: false,
        };
        assert_eq!(cut.listed(0, Some(5)), Ok(&checked), "read before the cut");
        assert_eq!(
            cut.listed(1, Some(5)),
            Err("cut"),
            "the cause, past the cut"
        );
        assert_eq!(cut.finished(1), Err("cut"), "however far the listing got");
    }
}

/// How libarchive's bytes for an entry's name are decoded: a RAR4's as code
/// page 437 where they are not UTF-8, a RAR5's as UTF-8 alone (O351).
#[cfg(feature = "rar-libarchive")]
mod names {
    use std::io::{Cursor, Write as _};

    use super::super::libarchive::decoder;
    use super::super::libarchive::headers::Walk;
    use super::super::libarchive::names::{cp437, rar4, utf8};

    /// `name` as `zip` reads it from an entry stored under it: UTF-8 where it
    /// is, else code page 437, by a table of `zip`'s own.
    fn zip_reads(name: &[u8]) -> String {
        let placeholder = "x".repeat(name.len());
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file(placeholder.as_str(), options)
            .expect("the entry starts");
        zip.write_all(b"page").expect("the entry writes");
        let mut bytes = zip.finish().expect("the zip finishes").into_inner();
        // The name is stored twice, in the local header and the central
        // directory, and an ASCII name sets no UTF-8 flag.
        let at: Vec<usize> = bytes
            .windows(name.len())
            .enumerate()
            .filter(|(_, window)| *window == placeholder.as_bytes())
            .map(|(at, _)| at)
            .collect();
        assert_eq!(at.len(), 2, "the name is stored twice");
        for at in at {
            bytes[at..at + name.len()].copy_from_slice(name);
        }
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("the zip opens");
        let entry = archive.by_index(0).expect("the entry reads");
        entry.name().expect("the name decodes").into_owned()
    }

    #[test]
    fn code_page_437_maps_every_byte_as_zip_does() {
        let high: Vec<u8> = (0x80..=0xff).collect();
        let decoded = cp437(&high);
        assert_eq!(decoded, zip_reads(&high));
        assert_eq!(decoded.chars().count(), 128, "a character for each byte");
        assert!(
            decoded.chars().all(|c| !c.is_ascii()),
            "no high byte maps into ASCII: {decoded}"
        );
        for byte in 0..0x80 {
            assert_eq!(cp437(&[byte]), char::from(byte).to_string(), "{byte:#04x}");
        }
    }

    #[test]
    fn a_rar4_name_is_utf8_where_it_can_be_and_code_page_437_where_not() {
        assert_eq!(rar4(b"p01.png").ok().as_deref(), Some("p01.png"));
        assert_eq!(
            rar4("ページ.png".as_bytes()).ok().as_deref(),
            Some("ページ.png")
        );
        // Bytes that are not UTF-8 read as zip reads them.
        for name in [&b"\x80\xe9\xff.png"[..], b"caf\xe9.png", b"\xc3"] {
            let got = rar4(name).unwrap_or_else(|e| panic!("{name:?}: {e}"));
            assert_eq!(got, zip_reads(name), "{name:?}");
        }
        assert_eq!(
            rar4(b"\x80\xe9\xff.png").ok().as_deref(),
            Some("\u{c7}\u{398}\u{a0}.png")
        );
    }

    #[test]
    fn a_rar5_name_must_be_utf8() {
        assert_eq!(
            utf8("ページ.png".as_bytes()).ok().as_deref(),
            Some("ページ.png")
        );
        assert!(utf8(b"\x80.png").is_err(), "not UTF-8");
    }

    /// The walk says which format the archive is, and so which rule its names
    /// are decoded by.
    #[test]
    fn a_walk_of_a_rar4_alone_decodes_names_as_code_page_437() {
        let rar4 = Walk {
            rar4: true,
            ..Walk::default()
        };
        let rar5 = Walk::default();
        assert_eq!(
            decoder(&rar4)(b"\x80.png").ok().as_deref(),
            Some("\u{c7}.png")
        );
        assert!(decoder(&rar5)(b"\x80.png").is_err(), "a RAR5 name");
        assert_eq!(decoder(&rar5)(b"p.png").ok().as_deref(), Some("p.png"));
    }
}

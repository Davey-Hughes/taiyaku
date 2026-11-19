use std::path::{Path, PathBuf};

use crate::rar::{BACKENDS, Backend, Entry, Visitor, with_backend};
use crate::test_support::{RarEntryKind, RarFixture, TempDir, rar_fixtures, sha256_hex};
use crate::{
    PageEntry, archive_page, for_each_member_page, member_entry, member_page, member_page_entries,
    member_page_names, member_page_sizes, member_pages, member_unread_entries, read_edition,
};

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

/// Runs `check` once for each backend, with page reads going through it.
fn each_backend(check: impl Fn(&str)) {
    for backend in BACKENDS {
        with_backend(*backend, || check(backend.name()));
    }
}

/// The error `got` must be, which must name `path` and hold `text`.
fn refusal<T: std::fmt::Debug>(got: Result<T, String>, path: &Path, text: &str, at: &str) {
    let error = got.expect_err(at);
    assert!(
        error.contains(&path.display().to_string()) && error.contains(text),
        "{at}: {error}"
    );
}

fn index(i: usize) -> u32 {
    u32::try_from(i).expect("a fixture holds few pages")
}

#[test]
fn every_fixture_reads_as_its_manifest_says() {
    each_backend(|backend| {
        for fixture in fixtures() {
            let at = format!("{backend} on {}", fixture.file);
            if fixture.refused.is_some() {
                reads_nothing_of_a_refused_fixture(&fixture, &at);
            } else {
                reads_as_its_manifest_says(&fixture, &at);
            }
        }
    });
}

/// Review Focus 2: an archive that cannot be listed has no page count, and no
/// function reads a page of it, not even one before the entry that refuses it.
fn reads_nothing_of_a_refused_fixture(fixture: &RarFixture, at: &str) {
    let path = &fixture.path;
    let text = "not a readable RAR archive";
    let at = |call: &str| format!("{at}: {call}");
    refusal(member_pages(path), path, text, &at("count"));
    refusal(member_page_names(path), path, text, &at("names"));
    refusal(member_page_sizes(path), path, text, &at("sizes"));
    refusal(member_page(path, 0), path, text, &at("page 0"));
    // make.sh puts a p01.png first in every archive it refuses.
    refusal(archive_page(path, "p01.png"), path, text, &at("p01.png"));
    refusal(member_page_entries(path), path, text, &at("entries"));
    refusal(member_unread_entries(path), path, text, &at("unread"));
    let mut handed = 0;
    let each = for_each_member_page(path, &mut |_, _| {
        handed += 1;
        Ok(())
    });
    refusal(each, path, text, &at("each page"));
    assert_eq!(handed, 0, "{}", at("each page"));
    let other = member_entry(path, "ComicInfo.xml");
    refusal(other, path, text, &at("ComicInfo.xml"));
    let edition = read_edition(std::slice::from_ref(path));
    refusal(edition, path, text, &at("edition"));
}

fn reads_as_its_manifest_says(fixture: &RarFixture, at: &str) {
    let path = &fixture.path;
    let names: Vec<String> = fixture.pages.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(member_page_names(path).as_ref(), Ok(&names), "{at}");
    // Every entry of every fixture is a page, a companion or no file at all.
    assert_eq!(member_unread_entries(path), Ok(Vec::new()), "{at}");
    if names.is_empty() {
        // Review Focus 3.
        refusal(member_pages(path), path, "no page images", at);
    } else {
        assert_eq!(member_pages(path), Ok(index(names.len())), "{at}");
    }

    // Each readable page's bytes as member_page gave them, or the name of the
    // first page whose read fails.
    let mut read = Vec::new();
    let mut fails = None;
    for (i, (name, sha)) in fixture.pages.iter().enumerate() {
        let by_index = member_page(path, index(i));
        let by_name = archive_page(path, name);
        let Some(sha) = sha else {
            refusal(by_index, path, name, &format!("{at}: page {i}"));
            refusal(by_name, path, name, &format!("{at}: {name}"));
            fails = fails.or(Some(name));
            continue;
        };
        let page = by_index.unwrap_or_else(|e| panic!("{at}: page {i}: {e}"));
        assert_eq!(page.name, *name, "{at}: page {i}");
        assert_eq!(sha256_hex(&page.bytes), *sha, "{at}: page {i}");
        let again = by_name.unwrap_or_else(|e| panic!("{at}: {name}: {e}"));
        assert_eq!(sha256_hex(&again.bytes), *sha, "{at}: {name} by name");
        read.push(page.bytes);
    }
    let past = names.len();
    refusal(
        member_page(path, index(past)),
        path,
        &format!("no page {past}"),
        at,
    );

    let sizes = member_page_sizes(path).unwrap_or_else(|e| panic!("{at}: {e}"));
    assert_eq!(sizes.len(), names.len(), "{at}: {sizes:?}");
    for (name, size) in names.iter().zip(&sizes) {
        let entry = fixture
            .entries
            .iter()
            .find(|entry| entry.name == *name)
            .unwrap_or_else(|| panic!("{at}: page {name} is no entry"));
        // A reference's header size is its target's; libarchive gives none.
        if entry.kind != RarEntryKind::Reference {
            assert_eq!(*size, entry.size, "{at}: {name}");
        }
    }

    if let Some(name) = fails {
        // A2: what reads the whole member fails on the page that does not read.
        let entries = member_page_entries(path);
        let each = for_each_member_page(path, &mut |_, _| Ok(()));
        let edition = read_edition(std::slice::from_ref(path));
        refusal(entries, path, name, &format!("{at}: entries"));
        refusal(each, path, name, &format!("{at}: each page"));
        refusal(edition, path, name, &format!("{at}: edition"));
    } else {
        reads_whole(fixture, &read, at);
    }

    for (name, sha) in &fixture.others {
        let lower = name.to_ascii_lowercase();
        let bytes = member_entry(path, &lower)
            .unwrap_or_else(|e| panic!("{at}: {lower}: {e}"))
            .unwrap_or_else(|| panic!("{at}: {lower} not found"));
        assert_eq!(sha256_hex(&bytes), *sha, "{at}: {lower}");
    }
    assert_eq!(member_entry(path, "missing.xml"), Ok(None), "{at}");
}

/// The whole-member reads of a fixture whose every page reads, against the
/// bytes `member_page` gave.
fn reads_whole(fixture: &RarFixture, read: &[Vec<u8>], at: &str) {
    let path = &fixture.path;
    let want: Vec<(PageEntry, String)> = fixture
        .pages
        .iter()
        .zip(read)
        .map(|((name, _), bytes)| {
            let size = fixture
                .entries
                .iter()
                .find(|entry| entry.name == *name)
                .map_or(0, |entry| entry.size);
            let crc32 = crc32fast::hash(bytes);
            let entry = PageEntry {
                name: name.clone(),
                size,
                crc32,
            };
            (entry, sha256_hex(bytes))
        })
        .collect();

    let entries = member_page_entries(path).unwrap_or_else(|e| panic!("{at}: {e}"));
    assert_eq!(entries.len(), want.len(), "{at}: {entries:?}");
    for (got, (want, _)) in entries.iter().zip(&want) {
        assert_eq!(got, want, "{at}: entries");
    }

    let mut handed = Vec::new();
    for_each_member_page(path, &mut |entry, bytes| {
        handed.push((entry.clone(), sha256_hex(bytes)));
        Ok(())
    })
    .unwrap_or_else(|e| panic!("{at}: {e}"));
    assert_eq!(handed.len(), want.len(), "{at}: {handed:?}");
    for (got, want) in handed.iter().zip(&want) {
        assert_eq!(got, want, "{at}: each page");
    }

    let edition = read_edition(std::slice::from_ref(path));
    if want.is_empty() {
        refusal(edition, path, "no page images", at);
        return;
    }
    let pages = edition.unwrap_or_else(|e| panic!("{at}: {e}"));
    assert_eq!(pages.len(), want.len(), "{at}: {pages:?}");
    for (page, (want, sha)) in pages.iter().zip(&want) {
        assert_eq!(page.name, want.name, "{at}: edition");
        assert_eq!(sha256_hex(&page.bytes), *sha, "{at}: edition {}", page.name);
    }
}

/// The suite above, run again with no locale in the environment but C, as in
/// the container: libarchive converts entry names through the locale, and a
/// name outside ASCII must still list and read whole. "1 passed", because a
/// filter that matched nothing passes too.
#[test]
fn the_suite_holds_under_the_c_locale() {
    let exe = std::env::current_exe().expect("the test binary's own path");
    let mut child = std::process::Command::new(exe);
    child.args([
        "--exact",
        "pages::rar_tests::every_fixture_reads_as_its_manifest_says",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LC_") {
            child.env_remove(key);
        }
    }
    let ran = child
        .env_remove("LANG")
        .env_remove("LANGUAGE")
        .env("LC_ALL", "C")
        .output()
        .expect("the test binary runs again");
    let stdout = String::from_utf8_lossy(&ran.stdout);
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(ran.status.success(), "under LC_ALL=C:\n{stdout}\n{stderr}");
    assert!(
        stdout.contains("1 passed"),
        "under LC_ALL=C:\n{stdout}\n{stderr}"
    );
}

/// Ruling R8: an entry that declares no size does not read, though a zip
/// reads an empty entry as empty. No fixture holds an empty file that is not a
/// page, so this is the file reference.
#[test]
fn an_entry_declaring_no_size_does_not_read_by_name() {
    let fixture = fixture("rar5-file-reference.rar");
    let path = &fixture.path;
    each_backend(|backend| {
        let at = format!("{backend} on {}", fixture.file);
        let declared = crate::rar::backend()
            .entries(path)
            .unwrap_or_else(|e| panic!("{at}: {e}"))
            .into_iter()
            .find(|entry| entry.name == "p02.png")
            .map(|entry| (entry.size, entry.readable));
        assert_eq!(declared, Some((0, true)), "{at}");
        refusal(member_entry(path, "P02.PNG"), path, "p02.png", &at);
    });
}

#[test]
fn a_rar_named_cbz_is_read_as_a_rar() {
    let fixture = fixture("rar5-japanese.rar");
    let dir = TempDir::new("rar-as-cbz");
    let copy = dir.join("x.cbz");
    std::fs::copy(&fixture.path, &copy).expect("the copy writes");
    let names: Vec<String> = fixture.pages.iter().map(|(name, _)| name.clone()).collect();
    each_backend(|backend| {
        assert_eq!(member_page_names(&copy).as_ref(), Ok(&names), "{backend}");
    });
}

// `cbz_pages` exists only with the feature.
#[cfg(feature = "test-support")]
#[test]
fn a_zip_member_and_a_rar_member_read_in_order() {
    let rar = fixture("rar5-compressed.rar");
    let dir = TempDir::new("zip-then-rar");
    let zip = crate::test_support::cbz_pages(&dir, "a.cbz", 2, 0);
    let mut want = vec![("000.png", None), ("001.png", None)];
    want.extend(
        rar.pages
            .iter()
            .map(|(name, sha)| (name.as_str(), sha.as_deref())),
    );
    each_backend(|backend| {
        let pages = crate::read_members_in_order(&[zip.clone(), rar.path.clone()])
            .unwrap_or_else(|e| panic!("{backend}: {e}"));
        assert_eq!(pages.len(), want.len(), "{backend}: {pages:?}");
        for (page, (name, sha)) in pages.iter().zip(&want) {
            assert_eq!(page.name, *name, "{backend}");
            if let Some(sha) = sha {
                assert_eq!(sha256_hex(&page.bytes), *sha, "{backend}: {name}");
            }
        }
    });
}

/// Review Focus 1: a download cut short and named `.cbz`.
#[test]
fn a_truncated_rar_names_rar_through_every_function() {
    let fixture = fixture("rar5-compressed.rar");
    let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
    let dir = TempDir::new("rar-half");
    let half = dir.join("half.cbz");
    std::fs::write(&half, &bytes[..bytes.len() / 2]).expect("the copy writes");
    let text = "not a readable RAR archive";
    each_backend(|backend| {
        let at = |call: &str| format!("{backend}: {call}");
        refusal(member_pages(&half), &half, text, &at("count"));
        refusal(member_page_names(&half), &half, text, &at("names"));
        refusal(member_page_sizes(&half), &half, text, &at("sizes"));
        refusal(member_page(&half, 0), &half, text, &at("page 0"));
        // The page the cut falls inside, which a read by name alone finds short.
        let page = archive_page(&half, "p2.png");
        refusal(page, &half, text, &at("p2.png"));
        refusal(member_page_entries(&half), &half, text, &at("entries"));
        refusal(member_unread_entries(&half), &half, text, &at("unread"));
        let each = for_each_member_page(&half, &mut |_, _| Ok(()));
        refusal(each, &half, text, &at("each page"));
        let other = member_entry(&half, "ComicInfo.xml");
        refusal(other, &half, text, &at("ComicInfo.xml"));
        let edition = read_edition(std::slice::from_ref(&half));
        refusal(edition, &half, text, &at("edition"));
    });
}

/// A8: a RAR5 cut anywhere short of its end, inside a block header too, is
/// refused by every listing, never counted with fewer pages. A cut that stops
/// inside the signature is not known as a RAR, so it need only be refused.
#[test]
fn a_rar5_cut_anywhere_is_refused() {
    let dir = TempDir::new("rar5-cuts");
    let cut = dir.join("cut.cbz");
    for file in ["rar5-solid.rar", "rar5-compressed.rar"] {
        let bytes = std::fs::read(fixture(file).path).expect("the fixture reads");
        each_backend(|backend| {
            for len in 0..bytes.len() {
                std::fs::write(&cut, &bytes[..len]).expect("the cut writes");
                let text = if bytes[..len].starts_with(b"Rar!\x1a\x07") {
                    "RAR"
                } else {
                    ""
                };
                let at = |call: &str| format!("{backend} on {file} cut to {len}: {call}");
                refusal(member_pages(&cut), &cut, text, &at("count"));
                refusal(member_page_names(&cut), &cut, text, &at("names"));
                let edition = read_edition(std::slice::from_ref(&cut));
                refusal(edition, &cut, text, &at("edition"));
            }
        });
    }
}

/// A8: zero bytes after the end block, which rar pads a volume with, leave a
/// RAR5 whole; any other byte there is refused.
#[test]
fn a_rar5_may_end_in_zero_padding_and_nothing_else() {
    let fixture = fixture("rar5-compressed.rar");
    let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
    let dir = TempDir::new("rar5-tails");
    let padded = dir.join("padded.cbz");
    let appended = dir.join("appended.cbz");
    std::fs::write(&padded, [bytes.as_slice(), &[0; 1023]].concat()).expect("the copy writes");
    std::fs::write(&appended, [bytes.as_slice(), &[1]].concat()).expect("the copy writes");
    each_backend(|backend| {
        let pages = member_pages(&padded);
        assert_eq!(pages, Ok(index(fixture.pages.len())), "{backend}: padded");
        let text = "not a readable RAR archive";
        refusal(member_pages(&appended), &appended, text, backend);
    });
}

/// Ruling R11: a RAR4 legacy-code-page name whose bytes are not UTF-8 reaches
/// the backend as they are, and the archive is refused, never counted short.
/// The control renames the same entry to ASCII under the same rewritten header
/// CRC, so the refusal is the name's, not the header's.
#[test]
fn a_rar4_name_that_is_not_utf8_refuses_the_archive() {
    let fixture = fixture("rar4.rar");
    let original = std::fs::read(&fixture.path).expect("the fixture reads");
    let dir = TempDir::new("rar4-name");
    let renamed = |first: u8| {
        let mut bytes = original.clone();
        let at = rar4_file_header(&bytes, "p03.png");
        bytes[at + 32] = first;
        let size = usize::from(u16::from_le_bytes([bytes[at + 5], bytes[at + 6]]));
        let crc = crc32fast::hash(&bytes[at + 2..at + size]).to_le_bytes();
        bytes[at..at + 2].copy_from_slice(&crc[..2]);
        let path = dir.join(format!("{first:02x}.cbr"));
        std::fs::write(&path, bytes).expect("the copy writes");
        path
    };
    let (control, legacy) = (renamed(b'q'), renamed(0xE9));
    let mut want: Vec<String> = fixture
        .pages
        .iter()
        .map(|(name, _)| name.replace("p03", "q03"))
        .collect();
    want.sort_by(|a, b| crate::naming::natural_cmp(a, b));
    each_backend(|backend| {
        let at = format!("{backend}: control");
        assert_eq!(member_page_names(&control).as_ref(), Ok(&want), "{at}");
        refusal(
            member_pages(&legacy),
            &legacy,
            "not a readable RAR archive",
            backend,
        );
    });
}

/// Where the RAR4 file header naming `name` starts in `bytes`, its name
/// stored as plain bytes at offset 32 (neither a large nor a Unicode header).
fn rar4_file_header(bytes: &[u8], name: &str) -> usize {
    let mut at = 0;
    while at + 7 <= bytes.len() {
        let flags = u16::from_le_bytes([bytes[at + 3], bytes[at + 4]]);
        let size = usize::from(u16::from_le_bytes([bytes[at + 5], bytes[at + 6]]));
        if bytes[at + 2] == 0x74 {
            let length = usize::from(u16::from_le_bytes([bytes[at + 26], bytes[at + 27]]));
            if bytes.get(at + 32..at + 32 + length) == Some(name.as_bytes()) {
                assert_eq!(flags & 0x300, 0, "{name}: a large or Unicode header");
                return at;
            }
        }
        // A block that carries data gives its length after the header size.
        let data = if flags & 0x8000 == 0 {
            0
        } else {
            let length: [u8; 4] = bytes[at + 7..at + 11].try_into().expect("four bytes");
            usize::try_from(u32::from_le_bytes(length)).expect("a fixture is small")
        };
        at += size + data;
    }
    panic!("no RAR4 file header named {name}");
}

/// The block starts of the RAR5 archive `bytes`: where each block's header
/// starts, where the header ends and where the data after it ends.
fn rar5_blocks(bytes: &[u8]) -> Vec<(usize, usize, usize)> {
    let vint = |at: usize| {
        let mut value = 0_u64;
        for (i, &byte) in bytes[at..].iter().enumerate() {
            value |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                let value = usize::try_from(value).expect("a fixture is small");
                return (value, i + 1);
            }
        }
        panic!("a vint runs off the end");
    };
    let mut blocks = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        let (size, size_len) = vint(at + 4);
        let header_end = at + 4 + size_len + size;
        let mut field = at + 4 + size_len;
        field += vint(field).1;
        let (flags, flags_len) = vint(field);
        field += flags_len;
        if flags & 1 != 0 {
            field += vint(field).1;
        }
        let data = if flags & 2 == 0 { 0 } else { vint(field).0 };
        blocks.push((at, header_end, header_end + data));
        at = header_end + data;
    }
    blocks
}

/// The RAR5 block whose header holds `name`.
fn rar5_block_named(bytes: &[u8], name: &str) -> (usize, usize, usize) {
    rar5_blocks(bytes)
        .into_iter()
        .find(|&(start, header_end, _)| {
            bytes[start..header_end]
                .windows(name.len())
                .any(|window| window == name.as_bytes())
        })
        .unwrap_or_else(|| panic!("no RAR5 block names {name}"))
}

/// Renames entry `from` to `to`, of the same length, under a valid header CRC.
fn rename_rar5_entry(bytes: &mut [u8], from: &str, to: &str) {
    let (start, header_end, _) = rar5_block_named(bytes, from);
    let at = start
        + bytes[start..header_end]
            .windows(from.len())
            .position(|window| window == from.as_bytes())
            .expect("the block names it");
    bytes[at..at + to.len()].copy_from_slice(to.as_bytes());
    let crc = crc32fast::hash(&bytes[start + 4..header_end]).to_le_bytes();
    bytes[start..start + 4].copy_from_slice(&crc);
}

/// Renames RAR4 entry `from` to `to`, of the same length, under a valid
/// header CRC.
fn rename_rar4_entry(bytes: &mut [u8], from: &str, to: &str) {
    let at = rar4_file_header(bytes, from);
    bytes[at + 32..at + 32 + to.len()].copy_from_slice(to.as_bytes());
    rar4_fix_header_crc(bytes, at);
}

/// Rewrites the header CRC of the RAR4 block at `at`.
fn rar4_fix_header_crc(bytes: &mut [u8], at: usize) {
    let size = usize::from(u16::from_le_bytes([bytes[at + 5], bytes[at + 6]]));
    let crc = crc32fast::hash(&bytes[at + 2..at + size]).to_le_bytes();
    bytes[at..at + 2].copy_from_slice(&crc[..2]);
}

/// `archive` with one byte of `page`'s stored data flipped, found by its bytes,
/// which a stored entry holds as they are; `skip` passes over that many earlier
/// copies of them.
fn with_stored_byte_flipped(archive: &[u8], page: &[u8], skip: usize) -> Vec<u8> {
    let at = archive
        .windows(page.len())
        .enumerate()
        .filter(|(_, window)| *window == page)
        .nth(skip)
        .map(|(at, _)| at)
        .expect("a stored page's bytes are in its archive");
    let mut damaged = archive.to_vec();
    damaged[at + page.len() / 2] ^= 0x10;
    damaged
}

/// The CRC refusal `got` must be: its path, the entry and the cause.
fn crc_refusal<T: std::fmt::Debug>(got: Result<T, String>, path: &Path, name: &str, at: &str) {
    let error = got.expect_err(at);
    assert!(
        error.contains(&path.display().to_string())
            && error.contains(name)
            && error.contains("CRC32"),
        "{at}: {error}"
    );
}

/// libarchive checks no stored entry's data, so a page damaged on disk would
/// read as good. Every function that hands back an entry's bytes refuses it,
/// naming the archive and the entry.
#[test]
fn a_damaged_stored_page_is_refused_by_every_read() {
    let dir = TempDir::new("rar-damaged");
    for file in ["rar4.rar", "rar5-stored.rar"] {
        let fixture = fixture(file);
        let original = std::fs::read(&fixture.path).expect("the fixture reads");
        let page = member_page(&fixture.path, 2).expect("the fixture's page reads");
        let path = dir.join(file);
        let damaged = with_stored_byte_flipped(&original, &page.bytes, 0);
        std::fs::write(&path, damaged).expect("the copy writes");
        let name = page.name.as_str();
        each_backend(|backend| {
            let at = |call: &str| format!("{backend} on {file}: {call}");
            crc_refusal(member_page(&path, 2), &path, name, &at("member_page"));
            crc_refusal(archive_page(&path, name), &path, name, &at("archive_page"));
            let each = for_each_member_page(&path, &mut |_, _| Ok(()));
            crc_refusal(each, &path, name, &at("for_each_member_page"));
            let edition = read_edition(std::slice::from_ref(&path));
            crc_refusal(edition, &path, name, &at("read_edition"));
            let entries = member_page_entries(&path);
            crc_refusal(entries, &path, name, &at("member_page_entries"));
            let entry = member_entry(&path, name);
            crc_refusal(entry, &path, name, &at("member_entry"));
            let members = crate::read_members_in_order(std::slice::from_ref(&path));
            crc_refusal(members, &path, name, &at("read_members_in_order"));
        });
    }
}

/// The check is the damaged entry's alone: the pages on either side of it read
/// and a whole-member read hands over those before it.
#[test]
fn the_pages_around_a_damaged_one_still_read() {
    let dir = TempDir::new("rar-around");
    for file in ["rar4.rar", "rar5-stored.rar"] {
        let fixture = fixture(file);
        let original = std::fs::read(&fixture.path).expect("the fixture reads");
        let page = member_page(&fixture.path, 2).expect("the fixture's page reads");
        let path = dir.join(file);
        let damaged = with_stored_byte_flipped(&original, &page.bytes, 0);
        std::fs::write(&path, damaged).expect("the copy writes");
        each_backend(|backend| {
            let at = format!("{backend} on {file}");
            for i in [0, 1, 3] {
                let got = member_page(&path, i).unwrap_or_else(|e| panic!("{at}: page {i}: {e}"));
                let want = fixture.pages[i as usize].1.as_deref();
                assert_eq!(
                    Some(sha256_hex(&got.bytes).as_str()),
                    want,
                    "{at}: page {i}"
                );
            }
            let mut handed = Vec::new();
            let each = for_each_member_page(&path, &mut |entry, _| {
                handed.push(entry.name.clone());
                Ok(())
            });
            crc_refusal(each, &path, &page.name, &at);
            let before: Vec<&String> = fixture.pages[..2].iter().map(|(name, _)| name).collect();
            assert_eq!(handed.iter().collect::<Vec<_>>(), before, "{at}");
        });
    }
}

/// A flip anywhere in a compressed page's data is refused or changes nothing:
/// the one rule covers a compressed entry too.
#[test]
fn a_damaged_compressed_entry_never_reads_as_good() {
    let fixture = fixture("rar5-compressed.rar");
    let original = std::fs::read(&fixture.path).expect("the fixture reads");
    let (_, data_start, data_end) = rar5_block_named(&original, "p2.png");
    let want = sha_of_page(&fixture, "p2.png");
    let dir = TempDir::new("rar-compressed-flips");
    let path = dir.join("flipped.rar");
    each_backend(|backend| {
        for at in data_start..data_end {
            let mut flipped = original.clone();
            flipped[at] ^= 0x10;
            std::fs::write(&path, &flipped).expect("the copy writes");
            if let Ok(page) = archive_page(&path, "p2.png") {
                assert_eq!(sha256_hex(&page.bytes), want, "{backend}: flip at {at}");
            }
        }
    });
}

fn sha_of_page(fixture: &RarFixture, name: &str) -> String {
    fixture
        .pages
        .iter()
        .find(|(page, _)| page == name)
        .and_then(|(_, sha)| sha.clone())
        .unwrap_or_else(|| panic!("{}: no sha for {name}", fixture.file))
}

/// Ruling R22: an entry that declares a BLAKE2 hash and no CRC32 stays unchecked,
/// so its damage reads through, as it does today.
#[test]
fn a_stored_blake2_entry_is_read_unchecked() {
    let fixture = fixture("rar5-blake2.rar");
    let original = std::fs::read(&fixture.path).expect("the fixture reads");
    let page = member_page(&fixture.path, 1).expect("the fixture's page reads");
    let dir = TempDir::new("rar-blake2");
    let path = dir.join("damaged.rar");
    let damaged = with_stored_byte_flipped(&original, &page.bytes, 0);
    std::fs::write(&path, damaged).expect("the copy writes");
    each_backend(|backend| {
        let got = member_page(&path, 1).unwrap_or_else(|e| panic!("{backend}: {e}"));
        assert_eq!(got.bytes.len(), page.bytes.len(), "{backend}");
        assert_ne!(
            got.bytes, page.bytes,
            "{backend}: the flip reached the page"
        );
        let first = member_page(&path, 0).unwrap_or_else(|e| panic!("{backend}: {e}"));
        let want = fixture.pages[0].1.as_deref();
        assert_eq!(Some(sha256_hex(&first.bytes).as_str()), want, "{backend}");
    });
}

/// Headers meet entries by position: of two entries of one name, each answers
/// for its own bytes, whichever of them is the damaged one.
#[test]
fn two_entries_of_one_name_each_meet_their_own_checksum() {
    let dir = TempDir::new("rar-same-name");
    // RAR5 keeps a repeated name however near; RAR4 is renamed two apart,
    // since libarchive reads a name repeated at once as a split file's next part.
    type Rename = fn(&mut [u8], &str, &str);
    let cases: [(&str, &str, usize, Rename); 2] = [
        ("rar5-stored.rar", "p02.png", 1, rename_rar5_entry),
        ("rar4.rar", "p03.png", 2, rename_rar4_entry),
    ];
    for (file, renamed, second, rename) in cases {
        let fixture = fixture(file);
        let mut original = std::fs::read(&fixture.path).expect("the fixture reads");
        let first_page = member_page(&fixture.path, 0).expect("page 0 reads").bytes;
        let second_page = member_page(&fixture.path, index(second))
            .expect("the renamed page reads")
            .bytes;
        rename(&mut original, renamed, "p01.png");
        let read_named = |damaged: &[u8], name: &str| {
            let path = dir.join(name);
            std::fs::write(&path, damaged).expect("the copy writes");
            path
        };

        let second_damaged = with_stored_byte_flipped(&original, &second_page, 0);
        let path = read_named(&second_damaged, "second.rar");
        for backend in BACKENDS {
            let at = format!("{} on {file}, the second damaged", backend.name());
            let bytes = backend.read(&path, "p01.png");
            assert_eq!(bytes.as_ref(), Ok(&first_page), "{at}: the first is whole");
            let mut handed = 0;
            let visit = backend.visit(&path, &|name| name == "p01.png", &mut |_, _| {
                handed += 1;
                Ok(())
            });
            crc_refusal(visit, &path, "p01.png", &at);
            assert_eq!(handed, 1, "{at}: the first was handed over");
        }

        let first_damaged = with_stored_byte_flipped(&original, &first_page, 0);
        let path = read_named(&first_damaged, "first.rar");
        for backend in BACKENDS {
            let at = format!("{} on {file}, the first damaged", backend.name());
            crc_refusal(backend.read(&path, "p01.png"), &path, "p01.png", &at);
        }
    }
}

/// libarchive reads a RAR4 name repeated at once as a split file's next part and
/// gives the entry no name, so such an archive is refused whole and no entry of
/// it goes unchecked.
#[test]
fn a_rar4_name_repeated_at_once_refuses_the_archive() {
    let fixture = fixture("rar4.rar");
    let mut bytes = std::fs::read(&fixture.path).expect("the fixture reads");
    rename_rar4_entry(&mut bytes, "p02.png", "p01.png");
    let dir = TempDir::new("rar4-repeated");
    let path = dir.join("repeated.rar");
    std::fs::write(&path, &bytes).expect("the copy writes");
    each_backend(|backend| {
        let each = for_each_member_page(&path, &mut |_, _| Ok(()));
        refusal(each, &path, "not a readable RAR archive", backend);
    });
}

/// A walk that cannot reach the end of the archive refuses every read of it,
/// whichever page is asked for. A cut exactly between two blocks leaves nothing
/// for the walk to find (O351).
#[test]
fn a_rar4_cut_short_of_its_last_block_reads_no_page() {
    let fixture = fixture("rar4.rar");
    let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
    let last = rar4_file_header(&bytes, "p04.png");
    let dir = TempDir::new("rar4-cut");
    let write = |name: &str, len: usize| {
        let path = dir.join(name);
        std::fs::write(&path, &bytes[..len]).expect("the copy writes");
        path
    };
    let in_data = write("in-data.rar", bytes.len() - 100);
    let in_header = write("in-header.rar", last + 3);
    let between = write("between.rar", last);
    each_backend(|backend| {
        for path in [&in_data, &in_header] {
            let at = format!("{backend} on {}", path.display());
            refusal(
                member_page(path, 0),
                path,
                "not a readable RAR archive",
                &at,
            );
        }
        let first = member_page(&between, 0).unwrap_or_else(|e| panic!("{backend}: {e}"));
        let want = fixture.pages[0].1.as_deref();
        assert_eq!(Some(sha256_hex(&first.bytes).as_str()), want, "{backend}");
        let each = for_each_member_page(&in_header, &mut |_, _| Ok(()));
        refusal(each, &in_header, "not a readable RAR archive", backend);
    });
    // libarchive lists this cut short with no error, so only the walk knows
    // that the entry asked for, there or missing, cannot be answered for.
    for backend in BACKENDS {
        for name in ["p01.png", "missing.png"] {
            let at = format!("{} reading {name}", backend.name());
            let got = backend.read(&in_header, name);
            refusal(got, &in_header, "not a readable RAR archive", &at);
        }
    }
}

/// The host byte decides how libarchive reads a file's attributes, and a page
/// from any host it reads must still meet its checksum.
#[test]
fn a_damaged_page_is_refused_whichever_host_wrote_it() {
    let fixture = fixture("rar4.rar");
    let original = std::fs::read(&fixture.path).expect("the fixture reads");
    let page = member_page(&fixture.path, 2).expect("the fixture's page reads");
    let header = rar4_file_header(&original, &page.name);
    let dir = TempDir::new("rar4-hosts");
    each_backend(|backend| {
        for host in 0..=5 {
            let at = format!("{backend} on host {host}");
            let mut clean = original.clone();
            clean[header + 15] = host;
            rar4_fix_header_crc(&mut clean, header);
            let path = dir.join(format!("clean-{host}.rar"));
            std::fs::write(&path, &clean).expect("the copy writes");
            let read = member_page(&path, 2).unwrap_or_else(|e| panic!("{at}: {e}"));
            assert_eq!(read.bytes, page.bytes, "{at}");

            let path = dir.join(format!("damaged-{host}.rar"));
            let damaged = with_stored_byte_flipped(&clean, &page.bytes, 0);
            std::fs::write(&path, damaged).expect("the copy writes");
            crc_refusal(member_page(&path, 2), &path, &page.name, &at);
        }
    });
}

/// A last volume opens with the end of a page begun in the volume before. That
/// block is no entry, and the whole pages after it each meet their own checksum.
#[test]
fn a_last_volumes_whole_pages_meet_their_own_checksums() {
    let fixture = fixture("rar5-split-last.part2.rar");
    let original = std::fs::read(&fixture.path).expect("the fixture reads");
    let page = member_page(&fixture.path, 0).expect("the fixture's page reads");
    let dir = TempDir::new("rar5-last-volume");
    let path = dir.join(&fixture.file);
    let damaged = with_stored_byte_flipped(&original, &page.bytes, 0);
    std::fs::write(&path, damaged).expect("the copy writes");
    each_backend(|backend| {
        let at = format!("{backend} on {}", fixture.file);
        crc_refusal(member_page(&path, 0), &path, &page.name, &at);
        crc_refusal(archive_page(&path, &page.name), &path, &page.name, &at);
        let next = member_page(&path, 1).unwrap_or_else(|e| panic!("{at}: {e}"));
        let want = fixture.pages[1].1.as_deref();
        assert_eq!(Some(sha256_hex(&next.bytes).as_str()), want, "{at}");
    });
}

/// A header declaring more data than the file holds stops the walk there and
/// refuses the read, in either format, with no panic and no allocation sized
/// from the header.
#[test]
fn a_header_declaring_more_than_the_file_holds_is_refused() {
    let dir = TempDir::new("rar-hostile-size");

    let fixture_rar4 = fixture("rar4.rar");
    let mut rar4 = std::fs::read(&fixture_rar4.path).expect("the fixture reads");
    let at = rar4_file_header(&rar4, "p03.png");
    rar4[at + 7..at + 11].copy_from_slice(&u32::MAX.to_le_bytes());
    rar4_fix_header_crc(&mut rar4, at);
    let path_rar4 = dir.join("rar4.rar");
    std::fs::write(&path_rar4, &rar4).expect("the copy writes");

    let fixture_rar5 = fixture("rar5-stored.rar");
    let mut rar5 = std::fs::read(&fixture_rar5.path).expect("the fixture reads");
    let (start, header_end, _) = rar5_block_named(&rar5, "p03.png");
    // The data size follows the type, the flags and any extra area's size; here three bytes.
    let size_len = rar5[start + 4..]
        .iter()
        .position(|b| b & 0x80 == 0)
        .expect("a vint")
        + 1;
    let mut field = start + 4 + size_len + 1;
    let flags = rar5[field];
    field += 1;
    if flags & 1 != 0 {
        field += rar5[field..]
            .iter()
            .position(|b| b & 0x80 == 0)
            .expect("a vint")
            + 1;
    }
    let len = rar5[field..]
        .iter()
        .position(|b| b & 0x80 == 0)
        .expect("a vint")
        + 1;
    rar5[field..field + len - 1].fill(0xff);
    rar5[field + len - 1] = 0x7f;
    let crc = crc32fast::hash(&rar5[start + 4..header_end]).to_le_bytes();
    rar5[start..start + 4].copy_from_slice(&crc);
    let path_rar5 = dir.join("rar5.rar");
    std::fs::write(&path_rar5, &rar5).expect("the copy writes");

    each_backend(|backend| {
        for path in [&path_rar4, &path_rar5] {
            let at = format!("{backend} on {}", path.display());
            let each = for_each_member_page(path, &mut |_, _| Ok(()));
            assert!(
                each.as_ref()
                    .is_err_and(|e| e.contains(&path.display().to_string())),
                "{at}: {each:?}"
            );
            let page = member_page(path, 0);
            refusal(page, path, "RAR", &at);
        }
    });
}

/// A RAR5 extra area the walk and libarchive read differently. It opens with a
/// hash record declaring 2 bytes: the walk steps by that size and finds one
/// record of an unknown type filling the rest, while libarchive takes 33 bytes
/// for a hash whatever the record declares, and so lands on a file-copy
/// redirection to `target` inside what the walk passed over.
fn rar5_extra_read_two_ways(target: &str) -> Vec<u8> {
    const HASH_RECORD_END: usize = 3 + 32;
    let name_len = u8::try_from(target.len()).expect("a short name");
    let mut redirection = vec![name_len + 4, 5, 5, 0, name_len];
    redirection.extend_from_slice(target.as_bytes());
    let mut area = vec![2, 2, 0];
    area.resize(HASH_RECORD_END, 0);
    area[3] = u8::try_from(HASH_RECORD_END - 4 + redirection.len()).expect("a small record");
    area[4] = 0x7f;
    area.extend(redirection);
    area
}

/// `bytes` with the extra area of the RAR5 block naming `name` replaced by
/// `area`, under a valid header CRC. The header's size and the area's size
/// each stay a vint of one byte.
fn with_rar5_extra_area(bytes: &[u8], name: &str, area: &[u8]) -> Vec<u8> {
    let (start, header_end, _) = rar5_block_named(bytes, name);
    // After the CRC: the header's size, its type, its flags, the area's size.
    let (size_at, area_size_at) = (start + 4, start + 7);
    let old_area = usize::from(bytes[area_size_at]);
    let new_size = usize::from(bytes[size_at]) - old_area + area.len();
    assert!(
        bytes[size_at] < 0x80 && bytes[area_size_at] < 0x80 && new_size < 0x80,
        "{name}: a size that takes more than one byte"
    );
    let mut header = bytes[size_at..header_end - old_area].to_vec();
    header[0] = u8::try_from(new_size).expect("under 0x80");
    header[area_size_at - size_at] = u8::try_from(area.len()).expect("under new_size");
    header.extend_from_slice(area);
    let crc = crc32fast::hash(&header).to_le_bytes();
    [&bytes[..start], &crc, &header, &bytes[header_end..]].concat()
}

/// A read lists on past its page, so that it refuses a file a visit refuses.
/// The last entry here is a checked file to the walk and an empty copy of
/// another entry to libarchive, which the entry count cannot show.
#[test]
fn a_later_entry_listed_unlike_its_header_refuses_every_page() {
    let fixture = fixture("rar5-stored.rar");
    let bytes = std::fs::read(&fixture.path).expect("the fixture reads");
    let crafted = with_rar5_extra_area(&bytes, "p06.png", &rar5_extra_read_two_ways("p01.png"));
    let dir = TempDir::new("rar-listed-as-a-copy");
    let path = dir.join("listed-as-a-copy.rar");
    std::fs::write(&path, crafted).expect("the copy writes");

    each_backend(|backend| {
        let sizes = member_page_sizes(&path).expect("the listing alone reads");
        assert_eq!(sizes.len(), fixture.pages.len(), "{backend}");
        assert_eq!(sizes.last(), Some(&0), "{backend}: listed as a copy");
        for page in 0..fixture.pages.len() {
            let at = format!("{backend}: page {page}");
            let got = member_page(&path, index(page));
            refusal(got, &path, "its block headers and its listing differ", &at);
        }
    });
}

/// Reading one entry of a solid archive stops at it: going on to list the rest
/// would decode them, and here fail on the last one's damage.
#[test]
fn reading_a_solid_entry_decodes_nothing_after_it() {
    let solid = fixture("rar5-solid.rar");
    let mut bytes = std::fs::read(&solid.path).expect("the fixture reads");
    let (_, data, end) = rar5_block_named(&bytes, "p4.png");
    for byte in &mut bytes[data..end] {
        *byte ^= 0x55;
    }
    let dir = TempDir::new("rar-solid-tail");
    let path = dir.join("tail.rar");
    std::fs::write(&path, &bytes).expect("the copy writes");
    for backend in BACKENDS {
        let first = backend.read(&path, "p1.png");
        assert!(first.is_ok(), "{}: {first:?}", backend.name());
    }
}

/// Lists these entries, each a name and whether it is a regular file, and
/// reads none of them.
#[derive(Debug)]
struct Lists(&'static [(&'static str, bool)]);

impl Backend for Lists {
    fn name(&self) -> &'static str {
        "lists"
    }
    fn entries(&self, _: &Path) -> Result<Vec<Entry>, String> {
        let entry = |&(name, readable): &(&str, bool)| Entry {
            name: name.to_owned(),
            size: 1,
            readable,
        };
        Ok(self.0.iter().map(entry).collect())
    }
    fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String> {
        Err(format!("{}: {name}: not read", path.display()))
    }
    fn visit(&self, _: &Path, _: &dyn Fn(&str) -> bool, _: &mut Visitor<'_>) -> Result<(), String> {
        Ok(())
    }
}

/// What a RAR holds that the reader does not read is named as a zip's is: each
/// regular file that is neither a page nor a companion, in natural order. The
/// directory's name would count it if its kind did not rule it out.
#[test]
fn a_rar_names_the_entries_the_reader_does_not_read() {
    static HOLDS: Lists = Lists(&[
        ("p01.png", true),
        ("extra10.xyz", true),
        ("extra2.xyz", true),
        ("deep/p03.jxl", true),
        ("ComicInfo.xml", true),
        ("Thumbs.db", true),
        ("__MACOSX/._p01.png", true),
        (".DS_Store", true),
        ("a-directory.xyz", false),
    ]);
    let path = fixture("rar5-stored.rar").path;
    let unread = with_backend(&HOLDS, || member_unread_entries(&path));
    assert_eq!(
        unread.expect("the listing is the backend's"),
        ["deep/p03.jxl", "extra2.xyz", "extra10.xyz"]
    );
}

/// Lists and reads as the first backend does; its visits hand nothing over.
#[derive(Debug)]
struct HandsNothing;

impl Backend for HandsNothing {
    fn name(&self) -> &'static str {
        "hands-nothing"
    }
    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String> {
        BACKENDS[0].entries(path)
    }
    fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String> {
        BACKENDS[0].read(path, name)
    }
    fn visit(&self, _: &Path, _: &dyn Fn(&str) -> bool, _: &mut Visitor<'_>) -> Result<(), String> {
        Ok(())
    }
}

/// A page listed by one open and missing from the next visit, as when the
/// file is replaced between them, is an error, never a short member.
#[test]
fn a_listed_page_no_visit_hands_over_is_not_read() {
    // Stored in reading order, so `for_each_member_page` visits too.
    let fixture = fixture("rar5-stored.rar");
    let path: &PathBuf = &fixture.path;
    let want = Err(format!(
        "{}: {}: not read",
        path.display(),
        fixture.pages[0].0
    ));
    with_backend(&HandsNothing, || {
        assert_eq!(
            read_edition(std::slice::from_ref(path)).map(drop),
            want,
            "edition"
        );
        assert_eq!(member_page_entries(path).map(drop), want, "entries");
        let each = for_each_member_page(path, &mut |_, _| Ok(()));
        assert_eq!(each, want, "each page");
    });
}

/// Criterion 1: one page reads the entry headers and that page's data, not the
/// data of the pages before it, and every page together reads the file once
/// plus what one page costs (its opens and listing), never a read per page.
/// The test runs itself again as a child under `strace`, as
/// `listing_an_archive_reads_no_more_for_four_hundred_pages_than_for_four`
/// does, and sums the bytes read from each copy. The skip bound is a half:
/// the binding hands libarchive 16 KiB a read, so on fixtures this small each
/// header costs at least that.
#[cfg(target_os = "linux")]
#[test]
fn reading_rar_pages_skips_what_it_does_not_need() {
    const CHILD: &str = "TAIYAKU_TEST_RAR_SKIP_CHILD";
    const FILES: [&str; 2] = ["rar5-stored.rar", "rar4.rar"];
    const READS: [&str; 4] = ["first", "last", "stream", "plan"];
    let copy = |dir: &Path, backend: &str, read: &str, file: &str| {
        dir.join(format!("{backend}-{read}-{file}"))
    };
    if let Some(dir) = std::env::var_os(CHILD) {
        let dir = Path::new(&dir);
        each_backend(|backend| {
            for file in FILES {
                let at = format!("{backend} on {file}");
                let last = index(fixture(file).pages.len() - 1);
                member_page(&copy(dir, backend, "first", file), 0)
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                member_page(&copy(dir, backend, "last", file), last)
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                for_each_member_page(&copy(dir, backend, "stream", file), &mut |_, _| Ok(()))
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                member_page_entries(&copy(dir, backend, "plan", file))
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
            }
        });
        return;
    }

    let scratch = TempDir::new("rar-skip");
    // Canonical, because strace prints the path behind each descriptor.
    let dir = std::fs::canonicalize(&scratch).expect("the scratch directory resolves");
    for backend in BACKENDS {
        for file in FILES {
            for read in READS {
                std::fs::copy(fixture(file).path, copy(&dir, backend.name(), read, file))
                    .expect("the copy writes");
            }
        }
    }
    let log = crate::test_support::strace(
        "pages::rar_tests::reading_rar_pages_skips_what_it_does_not_need",
        (CHILD, dir.as_os_str()),
        "read,pread64",
        &dir.join("strace.log"),
    );
    for backend in BACKENDS {
        for file in FILES {
            let fixture = fixture(file);
            let [first, last, stream, plan] =
                READS.map(|read| bytes_read_of(&log, &copy(&dir, backend.name(), read, file)));
            let size = std::fs::metadata(&fixture.path)
                .expect("the fixture is there")
                .len();
            let between: u64 = fixture.pages[1..fixture.pages.len() - 1]
                .iter()
                .filter_map(|(name, _)| fixture.entries.iter().find(|entry| entry.name == *name))
                .map(|entry| entry.size)
                .sum();
            let figures = format!(
                "{} on {file}: bytes read for the first page {first}, the last page {last}, \
                 every page {stream}, the entries {plan}; entries between the first page and \
                 the last {between} bytes; file {size} bytes",
                backend.name()
            );
            eprintln!("{figures}");
            assert!(
                first > 0 && last > 0,
                "nothing was read from a page's copy, so the figures prove nothing: {figures}"
            );
            assert!(2 * last.saturating_sub(first) < between, "{figures}");
            // Every entry here is a stored page, so reading them all reads the
            // file through, plus at most the opens and listing page 0 also pays.
            assert!(stream >= size, "{figures}");
            assert!(plan >= size, "{figures}");
            assert!(stream <= size + first, "{figures}");
            assert!(plan <= size + first, "{figures}");
        }
    }
}

/// The checksums and the data must come from one file: a second open of the
/// path could pair one archive's headers with another's bytes if the file were
/// replaced between the two.
#[cfg(target_os = "linux")]
#[test]
fn a_read_or_a_visit_opens_the_archive_once() {
    const CHILD: &str = "TAIYAKU_TEST_RAR_ONE_OPEN_CHILD";
    const FILE: &str = "rar5-stored.rar";
    const CALLS: [&str; 2] = ["read", "visit"];
    let copy = |dir: &Path, backend: &str, call: &str| dir.join(format!("{backend}-{call}-{FILE}"));
    if let Some(dir) = std::env::var_os(CHILD) {
        let dir = Path::new(&dir);
        let name = fixture(FILE).pages[0].0.clone();
        for backend in BACKENDS {
            backend
                .read(&copy(dir, backend.name(), "read"), &name)
                .expect("the page reads");
            backend
                .visit(
                    &copy(dir, backend.name(), "visit"),
                    &|_| true,
                    &mut |_, _| Ok(()),
                )
                .expect("the pages visit");
        }
        return;
    }

    let scratch = TempDir::new("rar-one-open");
    // Canonical, because strace prints the path each call was given.
    let dir = std::fs::canonicalize(&scratch).expect("the scratch directory resolves");
    for backend in BACKENDS {
        for call in CALLS {
            std::fs::copy(fixture(FILE).path, copy(&dir, backend.name(), call))
                .expect("the copy writes");
        }
    }
    let log = crate::test_support::strace(
        "pages::rar_tests::a_read_or_a_visit_opens_the_archive_once",
        (CHILD, dir.as_os_str()),
        "openat",
        &dir.join("strace.log"),
    );
    for backend in BACKENDS {
        for call in CALLS {
            let quoted = format!("\"{}\"", copy(&dir, backend.name(), call).display());
            let opens = log
                .lines()
                .filter(|line| line.contains("openat(") && line.contains(&quoted))
                .count();
            assert_eq!(opens, 1, "{} {call}: {log}", backend.name());
        }
    }
}

/// The bytes that `read` and `pread64` calls in an `strace -y` log returned
/// from `file`, whose path strace prints after the descriptor.
#[cfg(target_os = "linux")]
fn bytes_read_of(log: &str, file: &Path) -> u64 {
    let on_file = format!("<{}>", file.display());
    crate::test_support::rejoined(log)
        .iter()
        .filter_map(|line| {
            let args = ["read(", "pread64("].iter().find_map(|call| {
                let (before, args) = line.split_once(call)?;
                (before.is_empty() || before.ends_with(' ')).then_some(args)
            })?;
            args.trim_start_matches(|c: char| c.is_ascii_digit())
                .starts_with(&on_file)
                .then_some(())?;
            let (_, returned) = line.rsplit_once(") = ")?;
            returned.split_whitespace().next()?.parse::<u64>().ok()
        })
        .sum()
}

use super::*;

/// One scanned work, one edition, one member.
///
/// Built with `write_archive` into a `tempdir`, then `add_root`, `scan_root`
/// and `derive`, rather than by inserting rows, so the `member` row carries the
/// `sha256`, `pages` and `root_id` a real scan writes and a stamp then has
/// to move.
struct Fixture {
    library: crate::Library,
    dir: crate::tests::TempDir,
    work_id: i64,
}

impl Fixture {
    fn conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.library.conn()
    }

    fn archive(&self, name: &str) -> std::path::PathBuf {
        self.dir.path().join("Series").join(name)
    }
}

fn one_work() -> Fixture {
    let dir = crate::tests::tempdir("stamp");
    crate::scan::tests::write_archive(&dir.path().join("Series").join("Series - v01.cbz"), 2);

    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let work_id = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("the scan");
        crate::derive::derive(&conn).expect("the derive");
        conn.query_row("SELECT id FROM work", [], |row| row.get(0))
            .expect("the one work the fixture archive derived")
    };

    Fixture {
        library,
        dir,
        work_id,
    }
}

/// The `(slug, edition key)` of the fixture's one work, for rendering the
/// document a stamp would write.
fn addressed(fixture: &Fixture) -> (String, String) {
    let conn = fixture.conn();
    conn.query_row(
        "SELECT w.slug, e.key FROM work w JOIN edition e ON e.work_id = w.id
          WHERE w.id = ?1",
        [fixture.work_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .expect("the work and its one edition")
}

/// Replace the fixture's archive with one holding exactly `entries`, and
/// bring the member row's `sha256` and `pages` back in line with it.
///
/// **Entries are written with `SimpleFileOptions::default()`, which
/// DEFLATEs**, and that is load-bearing rather than incidental.
/// [`crate::scan::tests::write_archive`] writes method 0 — `STORE` — so an
/// archive straight from it cannot tell a `raw_copy_file` from a full
/// decompress-and-recompress: both produce identical stored bytes when there
/// was no compression to lose. `a_rewrite_leaves_every_page_byte_identical`
/// is the test that claim matters to, and against a `STORE` fixture it would
/// pass while proving nothing.
fn rewrite_fixture_archive(fixture: &Fixture, name: &str, entries: &[(&str, &[u8])]) {
    let path = fixture.archive(name);
    {
        let file = std::fs::File::create(&path).expect("creating the fixture archive");
        let mut zip = zip::ZipWriter::new(file);
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .expect("starting an entry");
            std::io::Write::write_all(&mut zip, bytes).expect("writing an entry");
        }
        zip.finish().expect("finishing the fixture archive");
    }

    let pages = entries
        .iter()
        .filter(|(entry, _)| !entry.eq_ignore_ascii_case("ComicInfo.xml"))
        .count();
    let conn = fixture.conn();
    conn.execute(
        "UPDATE member SET sha256 = ?1, pages = ?2",
        rusqlite::params![digest_of(&path), pages as i64],
    )
    .expect("bringing the member row back in line with the file");
}

fn digest_of(path: &std::path::Path) -> String {
    sha256_hex(&std::fs::read(path).expect("reading the archive"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

/// Replace the fixture's archive with bytes of another format entirely.
///
/// **Real bytes, not a column.** `member.format` is never written by anything
/// in this repository (see [`super::plan`]'s doc comment), so setting it
/// would exercise nothing; and `plan` sniffs the file rather than trusting a
/// token, so the only way to test the refusal is to put a file of that format
/// where the member says one is. These are the signatures
/// `taiyaku_read::sniff` matches on.
fn write_as(fixture: &Fixture, name: &str, signature: &[u8]) {
    let path = fixture.archive(name);
    std::fs::write(&path, signature).expect("writing the fixture archive");
    let conn = fixture.conn();
    conn.execute("UPDATE member SET sha256 = ?1", [digest_of(&path)])
        .expect("bringing the member row back in line with the file");
}

/// `Rar!\x1a\x07\x00` — the RAR 4 signature.
const RAR: &[u8] = b"Rar!\x1a\x07\x00padding to make a plausible file";
/// `7z\xbc\xaf\x27\x1c` — the 7-Zip signature.
const SEVEN_ZIP: &[u8] = b"7z\xbc\xaf\x27\x1cpadding to make a plausible file";

#[test]
fn an_archive_with_no_comicinfo_is_in_the_plan() {
    let fixture = one_work();

    let planned = plan(&fixture.conn(), fixture.work_id).expect("plans");

    assert_eq!(planned.len(), 1);
    assert!(
        planned[0].bytes > 0,
        "the size a person is shown before agreeing"
    );
    assert!(planned[0].path.ends_with("Series - v01.cbz"));
}

/// §1.3's whole claim: render what a stamp would write, put exactly that into
/// the archive, and the next plan is empty. This is the test that claim lives
/// or dies by, and it is what makes a re-stamp cheap.
#[test]
fn an_archive_whose_comicinfo_already_matches_is_absent_from_the_plan() {
    let fixture = one_work();
    let (slug, edition_key) = addressed(&fixture);
    let rendered = {
        let conn = fixture.conn();
        let info = crate::comicinfo::read(&conn, &slug, &edition_key, 2).expect("reads");
        crate::comicinfo::xml(&info).into_bytes()
    };

    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[
            ("000.jpg", b"one"),
            ("001.jpg", b"two"),
            ("ComicInfo.xml", &rendered),
        ],
    );

    assert!(
        plan(&fixture.conn(), fixture.work_id)
            .expect("plans")
            .is_empty(),
        "an archive already carrying exactly this document has nothing to do"
    );
}

#[test]
fn an_archive_whose_comicinfo_differs_is_in_the_plan() {
    let fixture = one_work();
    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[
            ("000.jpg", b"one"),
            ("001.jpg", b"two"),
            (
                "ComicInfo.xml",
                b"<?xml version=\"1.0\"?>\n<ComicInfo><Series>Something Else</Series></ComicInfo>\n",
            ),
        ],
    );

    assert_eq!(
        plan(&fixture.conn(), fixture.work_id).expect("plans").len(),
        1
    );
}

#[test]
fn a_rar_member_never_enters_a_plan() {
    let fixture = one_work();
    write_as(&fixture, "Series - v01.cbz", RAR);

    assert!(
        plan(&fixture.conn(), fixture.work_id)
            .expect("plans")
            .is_empty()
    );
}

#[test]
fn a_7z_member_never_enters_a_plan() {
    let fixture = one_work();
    write_as(&fixture, "Series - v01.cbz", SEVEN_ZIP);

    assert!(
        plan(&fixture.conn(), fixture.work_id)
            .expect("plans")
            .is_empty()
    );
}

/// A file matching no signature at all is not a zip either. The filter has to
/// say what a member *is* rather than exclude the two names it knows — a
/// `.cbz` holding something else is exactly what a rewrite must not touch.
#[test]
fn a_member_of_unknown_format_never_enters_a_plan() {
    let fixture = one_work();
    write_as(
        &fixture,
        "Series - v01.cbz",
        b"this is not any archive format",
    );

    assert!(
        plan(&fixture.conn(), fixture.work_id)
            .expect("plans")
            .is_empty()
    );
}

/// Every entry as `(name, compression method, compressed bytes)`.
///
/// **Compressed bytes and the method, not decompressed content.** The claim
/// under test is that a rewrite copies each entry's *stored* form untouched.
/// Comparing decompressed pages would pass against a full re-encode, which is
/// what §4 rejected — and comparing compressed bytes alone is nearly as weak,
/// because decompressing and recompressing with the same settings can produce
/// the identical output, so a bytes-only assertion over an all-`Deflated`
/// fixture can pass against the very defect it exists to catch. The method
/// is what makes the assertion bite: a naive re-encode writes everything
/// `Deflated`, so the `Stored` entry changes method whatever the compressor
/// does with the bytes.
fn raw_entries(path: &std::path::Path) -> Vec<(String, zip::CompressionMethod, Vec<u8>)> {
    let file = std::fs::File::open(path).expect("opening the archive");
    let mut archive = zip::ZipArchive::new(file).expect("reading the archive");
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index_raw(i).expect("an entry");
        let name = entry.name().expect("a name").into_owned();
        let method = entry.compression();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).expect("reading an entry");
        out.push((name, method, bytes));
    }
    out
}

/// An archive holding one `Stored` entry and one `Deflated` one.
///
/// Both halves are deliberate: `Deflated` is what a real archive's pages are,
/// and `Stored` is what makes a re-encode visible however the compressor
/// behaves.
fn mixed_methods_archive(fixture: &Fixture, name: &str) {
    let path = fixture.archive(name);
    {
        let file = std::fs::File::create(&path).expect("creating the fixture archive");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(
            "000.jpg",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .expect("starting the stored entry");
        std::io::Write::write_all(&mut zip, &vec![b'a'; 4096]).expect("writing it");
        zip.start_file(
            "001.jpg",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .expect("starting the deflated entry");
        std::io::Write::write_all(&mut zip, &vec![b'b'; 4096]).expect("writing it");
        zip.finish().expect("finishing the fixture archive");
    }
    let conn = fixture.conn();
    conn.execute(
        "UPDATE member SET sha256 = ?1, pages = 2",
        [digest_of(&path)],
    )
    .expect("bringing the member row back in line with the file");
}

#[test]
fn a_rewrite_leaves_every_page_byte_identical() {
    let fixture = one_work();
    mixed_methods_archive(&fixture, "Series - v01.cbz");
    let path = fixture.archive("Series - v01.cbz");
    let before: Vec<_> = raw_entries(&path)
        .into_iter()
        .filter(|(name, _, _)| name != DOCUMENT)
        .collect();
    assert_eq!(before.len(), 2, "the fixture holds the two pages");
    assert_eq!(
        before[0].1,
        zip::CompressionMethod::Stored,
        "and the first is stored, which is what makes a re-encode visible"
    );
    assert_eq!(before[1].1, zip::CompressionMethod::Deflated);

    super::rewrite_one(&path, b"<ComicInfo/>").expect("rewrites");

    let after: Vec<_> = raw_entries(&path)
        .into_iter()
        .filter(|(name, _, _)| name != DOCUMENT)
        .collect();
    assert_eq!(
        before, after,
        "every page's name, compression method and stored bytes survive"
    );
}

/// A page name in codepage 437 that is not valid UTF-8: `0x82` is `é` there
/// and a stray continuation byte in UTF-8.
const CP437_NAME: &[u8] = b"\x82t\x82 01.jpg";
/// An ASCII name as long as [`CP437_NAME`], which `ZipWriter` writes with the
/// UTF-8 flag clear and [`store_cp437_name`] then overwrites byte for byte.
const PLACEHOLDER: &str = "_t_ 01.jpg";
const _: () = assert!(CP437_NAME.len() == PLACEHOLDER.len());

/// Rename the fixture archive's [`PLACEHOLDER`] entry to [`CP437_NAME`] in
/// both its headers, and bring the member row's `sha256` back in line.
/// `ZipWriter` takes a name as `str`, so it cannot write these bytes itself.
fn store_cp437_name(fixture: &Fixture, name: &str) {
    let path = fixture.archive(name);
    let mut raw = std::fs::read(&path).expect("reading the fixture archive");
    let sites: Vec<usize> = raw
        .windows(PLACEHOLDER.len())
        .enumerate()
        .filter(|(_, window)| *window == PLACEHOLDER.as_bytes())
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        sites.len(),
        2,
        "the name once in the local header and once in the central directory"
    );
    for at in sites {
        raw[at..at + CP437_NAME.len()].copy_from_slice(CP437_NAME);
    }
    std::fs::write(&path, &raw).expect("writing the fixture archive");
    let conn = fixture.conn();
    conn.execute("UPDATE member SET sha256 = ?1", [digest_of(&path)])
        .expect("bringing the member row back in line with the file");
}

/// One entry's name as each header stores it: the raw bytes and whether the
/// UTF-8 flag (general purpose bit 11) is set, for its local header and then
/// for its central directory header.
type StoredName = ((Vec<u8>, bool), (Vec<u8>, bool));

/// Every entry's [`StoredName`], in central directory order, found by walking
/// the end of central directory record, the central directory and the local
/// header each entry points at (APPNOTE.TXT §4.3.16, §4.3.12, §4.3.7). Read
/// from the bytes rather than through `zip`, because a decoded name is the
/// very thing that hides a re-encoded one.
fn stored_names(path: &std::path::Path) -> Vec<StoredName> {
    let raw = std::fs::read(path).expect("reading the archive");
    let u16_at = |at: usize| usize::from(u16::from_le_bytes([raw[at], raw[at + 1]]));
    let u32_at = |at: usize| {
        usize::try_from(u32::from_le_bytes([
            raw[at],
            raw[at + 1],
            raw[at + 2],
            raw[at + 3],
        ]))
        .expect("an offset that fits")
    };
    let utf8 = |at: usize| u16_at(at) & (1 << 11) != 0;
    let end = raw
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .expect("an end of central directory record");

    let mut out = Vec::new();
    let mut at = u32_at(end + 16);
    for _ in 0..u16_at(end + 10) {
        assert_eq!(
            &raw[at..at + 4],
            b"PK\x01\x02",
            "a central directory header"
        );
        let name_len = u16_at(at + 28);
        let central = (raw[at + 46..at + 46 + name_len].to_vec(), utf8(at + 8));
        let local_at = u32_at(at + 42);
        assert_eq!(
            &raw[local_at..local_at + 4],
            b"PK\x03\x04",
            "a local header"
        );
        let local_len = u16_at(local_at + 26);
        let local = (
            raw[local_at + 30..local_at + 30 + local_len].to_vec(),
            utf8(local_at + 6),
        );
        out.push((local, central));
        at += 46 + name_len + u16_at(at + 30) + u16_at(at + 32);
    }
    out
}

/// A name stored in a legacy codepage, raw bytes under a clear UTF-8 flag, is
/// readable only by a tool that decodes it in that codepage. Re-encoding it
/// from its decoded string would replace it for good with the codepage 437
/// reading of those bytes, so a stamp has to keep the name's bytes and its
/// flag, in both headers a reader may take the name from.
#[test]
fn a_rewrite_keeps_a_non_utf8_entry_name_and_its_flag_as_stored() {
    let fixture = one_work();
    rewrite_fixture_archive(&fixture, "Series - v01.cbz", &[(PLACEHOLDER, b"one")]);
    store_cp437_name(&fixture, "Series - v01.cbz");
    let path = fixture.archive("Series - v01.cbz");
    let page = ((CP437_NAME.to_vec(), false), (CP437_NAME.to_vec(), false));
    assert_eq!(
        stored_names(&path),
        std::slice::from_ref(&page),
        "the fixture stores the raw name with the flag clear"
    );

    super::rewrite_one(&path, b"<ComicInfo/>").expect("rewrites");

    let after: Vec<_> = stored_names(&path)
        .into_iter()
        .filter(|(_, (name, _))| !name.eq_ignore_ascii_case(DOCUMENT.as_bytes()))
        .collect();
    assert_eq!(
        after,
        [page],
        "the page's raw name and UTF-8 flag survive in both headers"
    );
}

#[test]
fn a_rewrite_replaces_an_existing_comicinfo_rather_than_duplicating_it() {
    let fixture = one_work();
    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[("000.jpg", b"one"), (DOCUMENT, b"<old/>")],
    );
    let path = fixture.archive("Series - v01.cbz");

    super::rewrite_one(&path, b"<ComicInfo/>").expect("rewrites");

    let entries = raw_entries(&path);
    assert_eq!(
        entries
            .iter()
            .filter(|(name, _, _)| name == DOCUMENT)
            .count(),
        1,
        "one document, not two"
    );
    assert_eq!(
        taiyaku_read::member_entry(&path, DOCUMENT)
            .expect("reads")
            .expect("present"),
        b"<ComicInfo/>",
        "and it is the new one"
    );
}

#[test]
fn a_rewrite_adds_exactly_one_document_to_an_archive_that_had_none() {
    let fixture = one_work();
    let path = fixture.archive("Series - v01.cbz");
    let before = raw_entries(&path).len();

    super::rewrite_one(&path, b"<ComicInfo/>").expect("rewrites");

    let entries = raw_entries(&path);
    assert_eq!(entries.len(), before + 1);
    assert!(entries.iter().any(|(name, _, _)| name == DOCUMENT));
}

/// A `ComicInfo.xml` under another spelling is still the document, and a
/// rewrite must replace it rather than leave a second one beside it.
#[test]
fn a_rewrite_replaces_a_differently_cased_document() {
    let fixture = one_work();
    rewrite_fixture_archive(
        &fixture,
        "Series - v01.cbz",
        &[("000.jpg", b"one"), ("comicinfo.xml", b"<old/>")],
    );
    let path = fixture.archive("Series - v01.cbz");

    super::rewrite_one(&path, b"<ComicInfo/>").expect("rewrites");

    let entries = raw_entries(&path);
    assert_eq!(
        entries
            .iter()
            .filter(|(name, _, _)| name.eq_ignore_ascii_case(DOCUMENT))
            .count(),
        1,
        "one document whatever its spelling was: {:?}",
        entries.iter().map(|(n, _, _)| n).collect::<Vec<_>>()
    );
}

#[test]
fn a_rewrite_returns_the_new_digest_and_leaves_no_temp_behind() {
    let fixture = one_work();
    let path = fixture.archive("Series - v01.cbz");

    let digest = super::rewrite_one(&path, b"<ComicInfo/>").expect("rewrites");

    assert_eq!(digest.len(), 64, "lowercase hex sha256");
    assert_eq!(
        digest,
        digest_of(&path),
        "the digest names the file that is there now"
    );

    let strays: Vec<_> = std::fs::read_dir(path.parent().expect("a parent"))
        .expect("reading the directory")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("taiyaku-stamp"))
        .collect();
    assert!(
        strays.is_empty(),
        "the staging file was renamed, not left: {strays:?}"
    );
    // The filter above passes with or without the dot, and in any directory,
    // so the path is pinned whole: a dot name the walk skips (O185), beside
    // the archive, since a rename across file systems fails with EXDEV.
    assert_eq!(
        super::staging_path(&path),
        path.with_file_name(".Series - v01.cbz.taiyaku-stamp"),
        "the stage is a dot name in the archive's own directory"
    );
}

/// A stamp cut while staging can leave a whole, valid archive at the staging
/// name until that archive is stamped again. The next scan must not read it
/// as a second file of the work (O185).
#[test]
fn a_scan_skips_an_archive_left_at_the_staging_name() {
    let dir = crate::tests::tempdir("stamp-leftover");
    let archive = dir.path().join("v01.cbz");
    crate::scan::tests::write_archive(&archive, 2);
    crate::scan::tests::write_archive(&super::staging_path(&archive), 2);

    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
    crate::scan::scan_root(&conn, &root).expect("the scan");

    let entries: Vec<String> = conn
        .prepare(
            "SELECT rel_path FROM scan_entry WHERE root_id = ?1 AND kind = 'file'
              ORDER BY rel_path",
        )
        .expect("prepares")
        .query_map([root.id], |row| row.get(0))
        .expect("queries")
        .collect::<Result<_, _>>()
        .expect("reads");
    assert_eq!(
        entries,
        ["v01.cbz"],
        "the archive alone, not the leftover beside it"
    );
}

/// One work imported from a sidecar sitting beside its archive.
///
/// **The other half of the library.** [`one_work`] is a *scanned* work:
/// `work.sidecar_path` is NULL and its root's `keeps_documents` is 0, so
/// there is no document on disk that records a member digest. An imported
/// work has one, and it is what §13.2's "the sidecar" clause is about — a
/// stamp that moved a member's `sha256` and left `taiyaku.json` naming the
/// old one would have written the staleness it was meant to record.
fn imported_work() -> Fixture {
    imported_work_named("taiyaku.json")
}

/// [`imported_work`], with its sidecar under `name`: import takes a sidecar
/// by any name, and the export names its siblings by that name's stem.
fn imported_work_named(name: &str) -> Fixture {
    let dir = crate::tests::tempdir("stamp-imported");
    let archive = dir.path().join("v01.cbz");
    crate::scan::tests::write_archive(&archive, 2);

    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"Imported","reading":"rtl"}},
            "editions":{{"raw":{{"label":"Raw","members":[
              {{"path":"{}","sha256":"{}","pages":2}}]}}}},
            "hub":"raw","alignments":[]}}"#,
        archive.display(),
        digest_of(&archive)
    );
    let doc = taiyaku_core::Sidecar::from_json(&json).expect("the fixture document parses");

    // On disk as well as in the database: `import::sidecar` reads a document,
    // it does not write one, and the file is what this test is about.
    let sidecar = dir.path().join(name);
    std::fs::write(&sidecar, &json).expect("writing the fixture sidecar");

    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let work_id = {
        let conn = library.conn();
        crate::import::sidecar(&conn, &doc, &sidecar).expect("imports")
    };

    Fixture {
        library,
        dir,
        work_id,
    }
}

#[test]
fn apply_refuses_a_plan_that_no_longer_matches() {
    let fixture = one_work();
    let conn = fixture.conn();
    let mut stale = plan(&conn, fixture.work_id).expect("plans");
    // The archive grew between the plan and the confirmation: the same member
    // at the same path, and a different number from the one the person read
    // and agreed to.
    stale[0].bytes += 1;

    let refused = apply(&conn, fixture.work_id, 1, &stale);

    assert!(refused.is_err(), "a plan that no longer holds is refused");
    assert!(
        refused.as_ref().unwrap_err().contains("changed"),
        "and the message says why: {refused:?}"
    );
}

#[test]
fn apply_writes_the_new_digest_into_the_member_row() {
    let fixture = one_work();
    let conn = fixture.conn();
    let before: String = conn
        .query_row("SELECT sha256 FROM member LIMIT 1", [], |r| r.get(0))
        .expect("the member's digest");

    let applied = apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("applies");

    let after: String = conn
        .query_row("SELECT sha256 FROM member LIMIT 1", [], |r| r.get(0))
        .expect("the member's digest");
    assert_ne!(before, after, "the row moved");
    assert_eq!(applied.members.len(), 1);
    assert_eq!(applied.members[0].1, after, "and it is what apply reported");
    assert_eq!(
        after,
        digest_of(&fixture.archive("Series - v01.cbz")),
        "and the row names the file that is there now"
    );
}

#[test]
fn apply_moves_content_key_when_the_hub_edition_changes() {
    let fixture = one_work();
    let conn = fixture.conn();
    let before: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [fixture.work_id],
            |r| r.get(0),
        )
        .expect("the work's key");

    let applied = apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("applies");

    let after: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [fixture.work_id],
            |r| r.get(0),
        )
        .expect("the work's key");
    assert_ne!(before, after, "the work's identity moved with its hub");
    assert_eq!(applied.content_key.as_deref(), Some(after.as_str()));
}

/// §1.3's claim end to end: after a stamp, the archive carries exactly the
/// document a stamp would write, so there is nothing left to do.
#[test]
fn a_second_apply_has_nothing_to_do() {
    let fixture = one_work();
    let conn = fixture.conn();
    apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("applies");

    assert!(
        plan(&conn, fixture.work_id).expect("plans").is_empty(),
        "the archive now carries exactly the document that would be written"
    );
}

/// **A scanned work has no sidecar, and that is not an error.**
///
/// `export::work` fails when `work.sidecar_path` is NULL and no directory is
/// given — its own documented behaviour — and a scanned work's is always
/// NULL. Re-exporting unconditionally would therefore make the stamp fail on
/// exactly the works the scanner produces, which is most of a real library.
#[test]
fn applying_to_a_scanned_work_writes_no_sidecar_and_does_not_fail() {
    let fixture = one_work();
    let conn = fixture.conn();

    apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("a scanned work stamps without a sidecar to rewrite");

    assert!(
        !fixture.dir.path().join("taiyaku.json").exists(),
        "nothing invented a sidecar where the library has none"
    );
}

/// And the other half: an imported work's sidecar is brought back in line.
#[test]
fn applying_to_an_imported_work_rewrites_its_sidecar_with_the_new_digest() {
    let fixture = imported_work();
    let sidecar = fixture.dir.path().join("taiyaku.json");
    let before = std::fs::read_to_string(&sidecar).expect("the fixture wrote a sidecar");

    let applied = {
        let conn = fixture.conn();
        apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        )
        .expect("applies")
    };

    let after = std::fs::read_to_string(&sidecar).expect("the sidecar is still there");
    assert_ne!(before, after, "the sidecar was rewritten");
    assert!(
        applied.sidecar_rewritten,
        "and the stamp says so: {applied:?}"
    );
    assert!(
        after.contains(&applied.members[0].1),
        "and it names the digest the archive now has: {after}"
    );
}

/// A sidecar not named `<stem>.json` is not what the export writes: it writes
/// `taiyaku.json` beside `taiyaku.sidecar`, and the stamp must not tell the
/// person the file they keep was rewritten (O264).
#[test]
fn a_sidecar_under_another_name_is_not_reported_rewritten() {
    let fixture = imported_work_named("taiyaku.sidecar");
    let sidecar = fixture.dir.path().join("taiyaku.sidecar");
    let before = std::fs::read_to_string(&sidecar).expect("the fixture wrote a sidecar");

    let applied = {
        let conn = fixture.conn();
        apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        )
        .expect("applies")
    };

    assert!(
        !applied.sidecar_rewritten,
        "the file at sidecar_path was not written: {applied:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&sidecar).expect("still there"),
        before,
        "and it is as it was"
    );
    let beside = std::fs::read_to_string(fixture.dir.path().join("taiyaku.json"))
        .expect("the export wrote taiyaku.json beside it");
    assert!(
        beside.contains(&applied.members[0].1),
        "and that is where the new digest went: {beside}"
    );
}

/// The stamp's export keeps a document a newer taiyaku wrote, which would then
/// name the digests the stamp moved, so the stamp refuses before it touches
/// any archive (O264). Answers the refusal, the archive's digest before and
/// after, and the member row's digest after.
fn stamp_beside_a_newer(
    name: &str,
    text: &str,
) -> (Result<Applied, String>, String, String, String) {
    let fixture = imported_work();
    let archive = fixture.dir.path().join("v01.cbz");
    std::fs::write(fixture.dir.path().join(name), text).expect("a newer taiyaku's document");
    let before = digest_of(&archive);
    let conn = fixture.conn();
    let planned = plan(&conn, fixture.work_id).expect("plans");
    assert_eq!(
        planned.len(),
        1,
        "the archive must be in the plan, or nothing would change anyway"
    );

    let refused = apply(&conn, fixture.work_id, 1, &planned);

    let row: String = conn
        .query_row("SELECT sha256 FROM member LIMIT 1", [], |r| r.get(0))
        .expect("the member's digest");
    (refused, before, digest_of(&archive), row)
}

#[test]
fn a_stamp_over_a_sidecar_a_newer_taiyaku_wrote_is_refused_before_any_archive_is_touched() {
    let newer = r#"{"taiyaku": 2, "from": "newer"}"#;
    let (refused, before, after, row) = stamp_beside_a_newer("taiyaku.json", newer);

    let refusal = refused.expect_err("a newer sidecar refuses the stamp");
    assert!(
        refusal.ends_with(
            "taiyaku.json: this document declares taiyaku format version 2, and this build \
             reads 1. Refusing to read it rather than dropping the fields it does not \
             understand and writing them back out as version 1 — upgrade taiyaku instead"
        ),
        "refuse_newer's sentence, naming the file: {refusal}"
    );
    assert_eq!(after, before, "no archive's bytes changed");
    assert_eq!(row, before, "and the member row still names them");
}

#[test]
fn a_stamp_beside_a_library_document_a_newer_taiyaku_wrote_is_refused_too() {
    let newer = r#"{"taiyaku_library": 2, "from": "newer"}"#;
    let (refused, before, after, _) = stamp_beside_a_newer("taiyaku.library.json", newer);

    let refusal = refused.expect_err("a newer sibling refuses the stamp");
    assert!(
        refusal.contains(
            "taiyaku.library.json: this document declares taiyaku_library format version 2"
        ),
        "{refusal}"
    );
    assert_eq!(after, before, "no archive's bytes changed");
}

#[test]
fn a_stamp_beside_a_progress_document_a_newer_taiyaku_wrote_is_refused_too() {
    let newer = r#"{"taiyaku_progress": 2, "from": "newer"}"#;
    let (refused, before, after, _) = stamp_beside_a_newer("taiyaku.progress.json", newer);

    let refusal = refused.expect_err("a newer progress document refuses the stamp");
    assert!(
        refusal.contains(
            "taiyaku.progress.json: this document declares taiyaku_progress format version 2"
        ),
        "{refusal}"
    );
    assert_eq!(after, before, "no archive's bytes changed");
}

#[test]
fn a_stamp_beside_a_notes_document_a_newer_taiyaku_wrote_is_refused_too() {
    let newer = r#"{"taiyaku_notes": 2, "from": "newer"}"#;
    let (refused, before, after, _) = stamp_beside_a_newer("taiyaku.notes.json", newer);

    let refusal = refused.expect_err("a newer notes document refuses the stamp");
    assert!(
        refusal
            .contains("taiyaku.notes.json: this document declares taiyaku_notes format version 2"),
        "{refusal}"
    );
    assert_eq!(after, before, "no archive's bytes changed");
}

#[test]
fn a_stamp_beside_a_truth_document_a_newer_taiyaku_wrote_is_refused_too() {
    let newer = r#"{"taiyaku_truth": 2, "from": "newer"}"#;
    let (refused, before, after, _) = stamp_beside_a_newer("taiyaku.truth.json", newer);

    let refusal = refused.expect_err("a newer truth document refuses the stamp");
    assert!(
        refusal
            .contains("taiyaku.truth.json: this document declares taiyaku_truth format version 2"),
        "{refusal}"
    );
    assert_eq!(after, before, "no archive's bytes changed");
}

/// The flag comes from what the export wrote. `apply` refuses before it gets
/// here over a newer sidecar, so this pins the unit that answers it: the export
/// keeps that sidecar, and the stamp must not say it was rewritten (O264).
#[test]
fn a_sidecar_the_export_kept_is_not_reported_rewritten() {
    let fixture = imported_work();
    let sidecar = fixture.dir.path().join("taiyaku.json");
    let newer = r#"{"taiyaku": 2, "from": "newer"}"#;
    std::fs::write(&sidecar, newer).expect("a newer taiyaku's sidecar");
    let conn = fixture.conn();
    let sidecar_path: String = conn
        .query_row(
            "SELECT sidecar_path FROM work WHERE id = ?1",
            [fixture.work_id],
            |r| r.get(0),
        )
        .expect("an imported work has a sidecar path");

    let rewritten =
        rewrite_sidecar(&conn, fixture.work_id, 1, Some(&sidecar_path)).expect("exports");

    assert!(
        !rewritten,
        "the export kept the sidecar, so it was not rewritten"
    );
    assert_eq!(
        std::fs::read_to_string(&sidecar).expect("still there"),
        newer,
        "and it is as the newer taiyaku left it"
    );
}

/// What a plan and a stamp cost on real archives, printed rather than
/// asserted.
///
/// **`--ignored`, on the pattern `credential::tests::report_the_argon2_cost`
/// set** for argon2's parameters: a figure that depends on the machine is a
/// measurement to record in `docs/notes.md`, not a threshold to fail a build
/// on. Run it as
/// `TAIYAKU_CORPUS=corpus cargo test --release -p taiyaku-library \
///  stamp::tests::report -- --ignored --nocapture`.
///
/// **The plan half runs against the corpus in place, because `plan` only
/// reads.** The apply half never does: it copies one archive into a temp
/// directory first and stamps the copy, so the corpus is not rewritten by a
/// measurement. The copy is timed separately and reported, since a stamp is a
/// full copy and rename and that is most of what it costs.
#[test]
#[ignore = "needs TAIYAKU_CORPUS and rewrites a copy of one archive"]
fn report() {
    let Ok(corpus) = std::env::var("TAIYAKU_CORPUS") else {
        println!("stamp::report: TAIYAKU_CORPUS unset, measured nothing");
        return;
    };
    let root_path = std::path::Path::new(&corpus).join("gotoubun-v01");
    assert!(
        root_path.is_dir(),
        "expected a work directory at {}",
        root_path.display()
    );

    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let scan_started = std::time::Instant::now();
    let work_ids: Vec<i64> = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, &root_path).expect("adds the root");
        crate::scan::scan_root(&conn, &root).expect("scans");
        crate::derive::derive(&conn).expect("derives");
        let mut stmt = conn.prepare("SELECT id FROM work").expect("prepares");
        stmt.query_map([], |r| r.get(0))
            .expect("queries")
            .map(Result::unwrap)
            .collect()
    };
    let scanned = scan_started.elapsed();

    let conn = library.conn();
    let members: i64 = conn
        .query_row("SELECT COUNT(*) FROM member", [], |r| r.get(0))
        .expect("counts members");

    let planned_started = std::time::Instant::now();
    let mut archives = 0usize;
    let mut bytes = 0u64;
    for work_id in &work_ids {
        let planned = plan(&conn, *work_id).expect("plans");
        archives += planned.len();
        bytes += planned.iter().map(|p| p.bytes).sum::<u64>();
    }
    let planning = planned_started.elapsed();

    // The rewrite half, on a copy. The corpus is never the thing stamped: its
    // archive is copied into a temp directory first, and its digest is checked
    // afterwards to prove the measurement wrote nothing back.
    let largest: (std::path::PathBuf, u64) = {
        let mut found: Option<(std::path::PathBuf, u64)> = None;
        for work_id in &work_ids {
            for item in plan(&conn, *work_id).expect("plans") {
                if found.as_ref().is_none_or(|(_, size)| item.bytes > *size) {
                    found = Some((item.path.clone(), item.bytes));
                }
            }
        }
        found.expect("the corpus work has at least one stampable archive")
    };
    let before = digest_of(&largest.0);

    let scratch = crate::tests::tempdir("stamp-report");
    let copy = scratch.path().join("measured.cbz");
    let copy_started = std::time::Instant::now();
    std::fs::copy(&largest.0, &copy).expect("copies the archive");
    let copying = copy_started.elapsed();

    let rewrite_started = std::time::Instant::now();
    rewrite_one(
        &copy,
        b"<?xml version=\"1.0\"?>\n<ComicInfo><Series>M</Series></ComicInfo>\n",
    )
    .expect("rewrites the copy");
    let rewriting = rewrite_started.elapsed();

    assert_eq!(
        digest_of(&largest.0),
        before,
        "the measurement must not have written to the corpus"
    );

    println!(
        "stamp::report: {} work(s), {members} member(s) under {}\n  \
         scan+derive {:?}\n  \
         plan over every work: {:?} ({archives} archive(s), {:.1} MB to rewrite)\n  \
         largest archive {:.1} MB: plain copy {:?}, rewrite_one {:?}\n  \
         corpus digest unchanged after the measurement",
        work_ids.len(),
        root_path.display(),
        scanned,
        planning,
        bytes as f64 / 1_048_576.0,
        largest.1 as f64 / 1_048_576.0,
        copying,
        rewriting
    );
}

/// **A stamp must not carry another reader's notes off disk with it.**
///
/// `apply` brings the sidecar back in line because §13.2 asks it to. The
/// question this pins is *how much* it is allowed to write while doing so:
/// the gate names the member digest, `content_key` and the sidecar, and says
/// nothing about the four per-user documents `export::work` writes beside it.
#[test]
fn a_stamp_leaves_another_readers_exported_notes_alone() {
    let fixture = imported_work();
    let stem = fixture.dir.path().join("taiyaku.notes.json");

    {
        let conn = fixture.conn();
        conn.execute(
            "INSERT INTO user (id, name, role, password_hash) VALUES (2, 'reader', 'user', '!')",
            [],
        )
        .expect("a second account");
        conn.execute(
            "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
             VALUES ('n1', 2, ?1, 'raw', 0, 'a reader''s note',
                     '2026-09-07T00:00:00Z', '2026-09-07T00:00:00Z')",
            [fixture.work_id],
        )
        .expect("a note belonging to that account");
        crate::export::work(&conn, fixture.work_id, 2, None).expect("exports as the reader");
    }
    let before = std::fs::read_to_string(&stem).expect("the reader's notes are on disk");
    assert!(before.contains("a reader's note"), "{before}");

    {
        let conn = fixture.conn();
        apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        )
        .expect("an administrator stamps the work");
    }

    let after = std::fs::read_to_string(&stem);
    assert!(
        after
            .as_deref()
            .is_ok_and(|t| t.contains("a reader's note")),
        "the stamp took another reader's exported notes with it: {after:?}"
    );
}

/// **A confirmed stamp with nothing to do writes nothing at all.**
///
/// An empty plan matches an empty confirmation, so `apply` accepts it. A write
/// with no cause is still a write: `export::work` is scoped to the acting
/// user, so a no-op stamp that exported would republish one person's view of a
/// work's documents each time it was repeated.
#[test]
fn applying_an_empty_plan_touches_no_document() {
    let fixture = imported_work();
    let sidecar = fixture.dir.path().join("taiyaku.json");

    {
        let conn = fixture.conn();
        apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        )
        .expect("the first stamp does the work");
    }
    let settled = std::fs::metadata(&sidecar).expect("a sidecar").modified();
    let before = std::fs::read_to_string(&sidecar).expect("a sidecar");

    let applied = {
        let conn = fixture.conn();
        let empty = plan(&conn, fixture.work_id).expect("plans");
        assert!(empty.is_empty(), "the second plan has nothing in it");
        apply(&conn, fixture.work_id, 1, &empty).expect("and is accepted")
    };

    assert!(applied.members.is_empty());
    assert!(!applied.sidecar_rewritten, "no sidecar was republished");
    assert_eq!(
        std::fs::read_to_string(&sidecar).expect("a sidecar"),
        before,
        "and its bytes are untouched"
    );
    assert_eq!(
        std::fs::metadata(&sidecar)
            .expect("a sidecar")
            .modified()
            .ok(),
        settled.ok(),
        "not even its mtime moved"
    );
}

/// **A member the stamp would never touch must not block the work.**
///
/// `plan` sniffs every member to decide which are zips, and a sniff opens the
/// file — so a member whose bytes are gone must not fail the whole plan,
/// including for a work whose other archives are perfectly stampable. O62 is
/// the open item for members gone from disk; a rar that vanished is not a
/// reason the zip beside it cannot carry its metadata.
#[test]
fn a_member_whose_file_is_gone_does_not_block_the_rest() {
    let dir = crate::tests::tempdir("stamp-missing-member");
    let present = dir.path().join("v01.cbz");
    crate::scan::tests::write_archive(&present, 2);
    let absent = dir.path().join("v02.cbz");

    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"Partly Gone","reading":"rtl"}},
            "editions":{{"raw":{{"label":"Raw","members":[
              {{"path":"{}","sha256":"{}","pages":2}},
              {{"path":"{}","sha256":"gone","pages":2}}]}}}},
            "hub":"raw","alignments":[]}}"#,
        present.display(),
        digest_of(&present),
        absent.display()
    );
    let doc = taiyaku_core::Sidecar::from_json(&json).expect("parses");
    std::fs::write(dir.path().join("taiyaku.json"), &json).expect("writes the sidecar");

    let library = crate::Library::open_in_memory().expect("opens");
    let work_id = {
        let conn = library.conn();
        crate::import::sidecar(&conn, &doc, &dir.path().join("taiyaku.json")).expect("imports")
    };

    let conn = library.conn();
    let planned = plan(&conn, work_id).expect("a missing member does not fail the plan");

    assert_eq!(planned.len(), 1, "the archive that is there is stampable");
    assert!(planned[0].path.ends_with("v01.cbz"));
}

/// **`resolve`'s two branches, both driven.**
///
/// Every fixture in this module stores an absolute `member.path`, so none
/// of them has a relative one to join to a root. An imported document may
/// name a member relatively, and then the root is the only thing that makes
/// it findable.
#[test]
fn a_relative_member_path_is_joined_to_its_root_and_an_absolute_one_is_not() {
    assert_eq!(
        super::resolve("v01.cbz", Some("/shelf")),
        std::path::PathBuf::from("/shelf/v01.cbz"),
        "a relative path is joined to its root"
    );
    assert_eq!(
        super::resolve("/elsewhere/v01.cbz", Some("/shelf")),
        std::path::PathBuf::from("/elsewhere/v01.cbz"),
        "an absolute path is already the answer, root or no root"
    );
    assert_eq!(
        super::resolve("v01.cbz", None),
        std::path::PathBuf::from("v01.cbz"),
        "and a member with no root is left exactly as it is"
    );
}

/// **A member held under two directories never enters a plan.** Stamping the
/// copy `member.path` names would leave the other's bytes different, and the
/// next derive would split the one member into two.
#[test]
fn a_member_held_under_two_directories_never_enters_a_plan() {
    let fixture = one_work();
    let copy = crate::tests::tempdir("stamp-copy");
    std::fs::create_dir_all(copy.path().join("Series")).expect("mkdir");
    std::fs::copy(
        fixture.archive("Series - v01.cbz"),
        copy.path().join("Series").join("Series - v01.cbz"),
    )
    .expect("copies the archive");
    let conn = fixture.conn();
    assert_eq!(
        plan(&conn, fixture.work_id).expect("plans").len(),
        1,
        "the fixture: stampable while one directory holds it"
    );
    assert!(!copied(&conn, fixture.work_id).expect("reads"));

    let root = crate::scan::add_root(&conn, copy.path()).expect("adds the copy's root");
    crate::scan::scan_root(&conn, &root).expect("scans it");
    crate::derive::derive(&conn).expect("derives");

    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM member_root mr JOIN member m ON m.id = mr.member_id
               JOIN edition e ON e.id = m.edition_id WHERE e.work_id = ?1",
            [fixture.work_id],
            |r| r.get::<_, i64>(0),
        )
        .expect("counts"),
        2,
        "the fixture: one member of this work, held under both"
    );
    assert!(
        plan(&conn, fixture.work_id).expect("plans").is_empty(),
        "a copied member is left out"
    );
    assert!(
        copied(&conn, fixture.work_id).expect("reads"),
        "and a caller can say why"
    );
}

/// `member_paths` answers every member, not just the stampable ones — which
/// is what lets a caller say what a stamp is leaving alone.
#[test]
fn member_paths_answers_every_member_including_the_ones_a_plan_skips() {
    let fixture = one_work();
    write_as(&fixture, "Series - v01.cbz", RAR);
    let conn = fixture.conn();

    assert!(
        plan(&conn, fixture.work_id).expect("plans").is_empty(),
        "the rar is not stampable"
    );
    // **The path, not the count.** An empty `PathBuf` per member would satisfy
    // a length assertion.
    assert_eq!(
        member_paths(&conn, fixture.work_id).expect("resolves"),
        vec![fixture.archive("Series - v01.cbz")],
        "but it is still a member, and still has to be reportable — by name"
    );
}

/// **The hand document follows the work's identity, §1.4's last step.**
///
/// `<content_key>.library.json` is addressed by a hash over the hub edition's
/// member digests, and a stamp moves every one of them. A document left at
/// the old address is what `hand::Orphan` is: a correction nothing in the
/// library can find any more, on a work that is still right there.
#[test]
fn a_stamp_moves_the_hand_document_to_the_works_new_address() {
    let fixture = one_work();
    let (before_key, document) = {
        let conn = fixture.conn();
        conn.execute("UPDATE root SET keeps_documents = 1", [])
            .expect("the root keeps documents");

        let mut doc = crate::librarydoc::of(&conn, fixture.work_id).expect("projects");
        doc.title = Some("Hand-Set Title".to_string());
        crate::librarydoc::apply(&conn, fixture.work_id, &doc).expect("applies the hand edit");
        crate::hand::save(&conn, fixture.work_id).expect("saves the hand document");

        let key: String = conn
            .query_row(
                "SELECT content_key FROM work WHERE id = ?1",
                [fixture.work_id],
                |r| r.get(0),
            )
            .expect("the work's key");
        let path = crate::hand::path_for(&conn, fixture.work_id)
            .expect("resolves an address")
            .expect("the root keeps documents, so there is one");
        (key, path)
    };
    assert!(document.is_file(), "the document is on disk to begin with");
    assert!(
        document.to_string_lossy().contains(&before_key),
        "and it is addressed by the key the stamp is about to move"
    );

    let applied = {
        let conn = fixture.conn();
        apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        )
        .expect("applies")
    };

    assert!(applied.content_key.is_some(), "the hub edition moved");
    assert!(applied.document_moved, "and the document went with it");
    assert!(
        !document.exists(),
        "nothing is left stranded at the old address"
    );

    let conn = fixture.conn();
    let now = crate::hand::path_for(&conn, fixture.work_id)
        .expect("resolves")
        .expect("still keeps documents");
    assert!(now.is_file(), "the document is at the new address: {now:?}");
    assert!(
        std::fs::read_to_string(&now)
            .expect("reads")
            .contains("Hand-Set Title"),
        "and it still carries the hand edit"
    );
}

/// [`one_work`] in a root that keeps documents, with chapter 7 set on its
/// member by hand and saved as a metadata PATCH saves it. Answers the member's
/// digest before any stamp, which the document keys the chapter by.
fn hand_chaptered(fixture: &Fixture) -> String {
    let conn = fixture.conn();
    conn.execute("UPDATE root SET keeps_documents = 1", [])
        .expect("the root keeps documents");
    let sha256: String = conn
        .query_row("SELECT sha256 FROM member", [], |r| r.get(0))
        .expect("the member's digest");
    let mut doc = crate::librarydoc::of(&conn, fixture.work_id).expect("projects");
    doc.members = vec![crate::librarydoc::MemberMeta {
        sha256: sha256.clone(),
        chapter: Some(7.0),
        chapter_source: Some(crate::librarydoc::ChapterSource::Hand),
    }];
    crate::librarydoc::apply(&conn, fixture.work_id, &doc).expect("applies the hand edit");
    let saved = crate::hand::save(&conn, fixture.work_id).expect("saves the hand document");
    assert!(
        saved.persisted,
        "the fixture's document is on disk: {saved:?}"
    );
    sha256
}

/// Stamp the fixture's work as planned, which must succeed.
fn stamp_all(fixture: &Fixture) -> Applied {
    let conn = fixture.conn();
    apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("applies")
}

/// The hand document at the work's address now.
fn document_at_address(fixture: &Fixture) -> String {
    let conn = fixture.conn();
    let path = crate::hand::path_for(&conn, fixture.work_id)
        .expect("resolves")
        .expect("the work has an address");
    std::fs::read_to_string(&path).expect("the document is at the work's address")
}

/// The chapter a fresh library over `dir` reads back for its one member,
/// with its root keeping documents or not. The read-back ignores that
/// setting, so either way it reads the document the directory holds.
fn rebuilt_chapter(dir: &std::path::Path, keeps_documents: bool) -> (Option<f64>, Option<String>) {
    let rebuilt = crate::Library::open_in_memory().expect("an in-memory library opens");
    let conn = rebuilt.conn();
    let root = crate::scan::add_root(&conn, dir).expect("adding the root");
    conn.execute(
        "UPDATE root SET keeps_documents = ?1",
        [i64::from(keeps_documents)],
    )
    .expect("setting the root's documents");
    crate::scan::scan_root(&conn, &root).expect("the scan");
    crate::derive::derive(&conn).expect("the derive");
    conn.query_row(
        "SELECT chapter_declared, chapter_source FROM member",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .expect("the member")
}

/// The hand document keys members by `sha256` and a stamp moves those digests,
/// so the stamp re-keys it under the new ones. Left naming the old ones, a
/// library rebuilt from this directory reads it back and drops the chapter.
#[test]
fn a_stamp_rewrites_the_hand_document_under_the_new_digests_and_a_rebuild_keeps_its_chapter() {
    let fixture = one_work();
    let old = hand_chaptered(&fixture);

    let applied = stamp_all(&fixture);
    let new = applied.members[0].1.clone();
    assert!(
        applied.document_rewritten,
        "the stamp says it rewrote the document: {applied:?}"
    );

    let document = document_at_address(&fixture);
    assert!(
        document.contains(&new),
        "the document names the new digest: {document}"
    );
    assert!(!document.contains(&old), "and not the old one: {document}");
    assert_eq!(
        rebuilt_chapter(fixture.dir.path(), true),
        (Some(7.0), Some("hand".to_string())),
        "a library rebuilt from the directory reads the hand-set chapter back"
    );
}

/// An edit made in the hand document's file is the person's to settle (O44),
/// so the stamp re-keys the file as it stands rather than rewriting it from
/// the rows, and the edit survives.
#[test]
fn a_stamp_rekeys_a_hand_document_edited_on_disk_and_keeps_the_edit() {
    let fixture = one_work();
    let old = hand_chaptered(&fixture);
    let edited = edit_document(&fixture, |doc| {
        doc["title"] = serde_json::json!("Edited In The File");
    });

    let applied = stamp_all(&fixture);

    let document = document_at_address(&fixture);
    assert!(
        document.contains("Edited In The File"),
        "the edit made in the file survives the stamp: {document}"
    );
    assert!(
        document.contains(&applied.members[0].1),
        "and the document names the new digest: {document}"
    );
    assert!(!document.contains(&old), "and not the old one: {document}");
    assert_eq!(
        document,
        edited.replace(&old, &applied.members[0].1),
        "and nothing else in the file changed, its layout included"
    );
}

/// A root that declines documents can still hold one, written before it was
/// switched off, and a rebuilt library reads it back whatever the root says,
/// so the stamp re-keys it there too.
#[test]
fn a_stamp_rekeys_a_hand_document_a_declining_root_still_holds_and_a_rebuild_keeps_its_chapter() {
    let fixture = one_work();
    let old = hand_chaptered(&fixture);
    fixture
        .conn()
        .execute("UPDATE root SET keeps_documents = 0", [])
        .expect("the root declines documents from now on");

    let applied = stamp_all(&fixture);

    assert!(
        applied.document_rewritten,
        "the stamp says it rewrote the document: {applied:?}"
    );
    let document = document_at_address(&fixture);
    assert!(
        document.contains(&applied.members[0].1),
        "the document names the new digest: {document}"
    );
    assert!(!document.contains(&old), "and not the old one: {document}");
    assert_eq!(
        rebuilt_chapter(fixture.dir.path(), false),
        (Some(7.0), Some("hand".to_string())),
        "a library rebuilt from the directory reads the hand-set chapter back"
    );
}

/// One scanned work of two chaptered archives in a root that keeps documents,
/// with a hand title saved, so its document names both members by the
/// chapters the rules read from their names.
fn two_chapters() -> Fixture {
    let dir = crate::tests::tempdir("stamp-chapters");
    for name in ["Series - v01 - c1.cbz", "Series - v01 - c2.cbz"] {
        crate::scan::tests::write_archive(&dir.path().join("Series").join(name), 2);
    }
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let work_id = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        conn.execute("UPDATE root SET keeps_documents = 1", [])
            .expect("the root keeps documents");
        crate::scan::scan_root(&conn, &root).expect("the scan");
        crate::derive::derive(&conn).expect("the derive");
        let (works, members): (i64, i64) = conn
            .query_row(
                "SELECT (SELECT count(*) FROM work), (SELECT count(*) FROM member)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("counts");
        assert_eq!((works, members), (1, 2), "one work of two members");
        let work_id: i64 = conn
            .query_row("SELECT id FROM work", [], |r| r.get(0))
            .expect("the work");
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projects");
        doc.title = Some("Hand-Set Title".to_string());
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applies the hand edit");
        let saved = crate::hand::save(&conn, work_id).expect("saves the hand document");
        assert!(
            saved.persisted,
            "the fixture's document is on disk: {saved:?}"
        );
        work_id
    };
    Fixture {
        library,
        dir,
        work_id,
    }
}

/// A stamp cut after renaming some archives has stamped those files whatever
/// the rows say, and a rescan records their digests, so the document follows
/// the files at once: the archives renamed are re-keyed and no other is.
#[test]
fn a_stamp_cut_partway_rekeys_the_hand_document_for_the_archives_it_renamed_alone() {
    let fixture = two_chapters();
    let first = fixture.archive("Series - v01 - c1.cbz");
    let second = fixture.archive("Series - v01 - c2.cbz");
    let (first_old, second_old) = (digest_of(&first), digest_of(&second));
    // Where the second archive would be staged, so its rewrite fails after
    // the first archive has been renamed.
    std::fs::create_dir(super::staging_path(&second)).expect("a directory in the way");
    let before = document_at_address(&fixture);
    assert!(
        before.contains(&first_old) && before.contains(&second_old),
        "the document names both members to begin with: {before}"
    );

    let cut = {
        let conn = fixture.conn();
        let planned = plan(&conn, fixture.work_id).expect("plans");
        assert_eq!(
            planned.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
            [first.clone(), second.clone()],
            "both archives are planned, the first first"
        );
        apply(&conn, fixture.work_id, 1, &planned)
    };

    let cut = cut.expect_err("the second archive cannot be staged");
    assert!(
        cut.contains("1 of this work's archives were rewritten before this"),
        "{cut}"
    );
    let first_new = digest_of(&first);
    assert_ne!(first_new, first_old, "the first archive was stamped");
    let document = document_at_address(&fixture);
    assert!(
        document.contains(&first_new) && !document.contains(&first_old),
        "the archive it renamed is re-keyed: {document}"
    );
    assert!(
        document.contains(&second_old),
        "and the one it did not rename keeps its digest: {document}"
    );
}

/// Every row a stamp reads is read before its first rename, so a read that
/// fails for a later archive leaves every archive and the hand document as
/// they were. A second member whose digest is not text stands in for any read
/// failing: `plan` never reads the digest, so the plan still matches.
#[test]
fn a_read_that_fails_for_a_later_archive_fails_before_any_archive_is_renamed() {
    let fixture = two_chapters();
    let first = fixture.archive("Series - v01 - c1.cbz");
    let second = fixture.archive("Series - v01 - c2.cbz");
    let (first_old, second_old) = (digest_of(&first), digest_of(&second));
    let before = document_at_address(&fixture);

    let failed = {
        let conn = fixture.conn();
        let planned = plan(&conn, fixture.work_id).expect("plans");
        assert_eq!(
            planned.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
            [first.clone(), second.clone()],
            "both archives are planned, the first first"
        );
        conn.execute(
            "UPDATE member SET sha256 = X'00' WHERE id = ?1",
            [planned[1].member_id],
        )
        .expect("a digest that is not text");
        apply(&conn, fixture.work_id, 1, &planned)
    };

    let failed = failed.expect_err("the second member's row cannot be read");
    assert!(failed.starts_with("reading member "), "{failed}");
    assert_eq!(
        (digest_of(&first), digest_of(&second)),
        (first_old, second_old),
        "no archive was renamed: {failed}"
    );
    assert_eq!(
        document_at_address(&fixture),
        before,
        "and the hand document is untouched"
    );
}

/// O81: when a file already sits at the work's new address, `rename_from`
/// leaves the document at the old one. The stamp re-keys it there, never
/// writes over the file at the new address, and does not report the work's
/// document rewritten, since the copy it re-keyed is not at the work's address.
///
/// The new address hashes the stamped archive's digest, and the stamp writes
/// the current time into its document's entry at two-second resolution, so
/// the test stamps a copy first to learn it. It retries only when it measured
/// the two stamps fall either side of a tick.
#[test]
fn a_stamp_whose_new_address_is_taken_rekeys_the_document_where_it_stands_and_leaves_that_file() {
    let tick = |at: std::time::SystemTime| {
        at.duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs()
            / 2
    };
    for _attempt in 0..5 {
        let fixture = one_work();
        let old = hand_chaptered(&fixture);
        let archive = fixture.archive("Series - v01.cbz");
        let (slug, edition_key) = addressed(&fixture);
        let started = std::time::SystemTime::now();
        let (document, key, taken, orphan) = {
            let conn = fixture.conn();
            let document = crate::hand::path_for(&conn, fixture.work_id)
                .expect("resolves")
                .expect("the root keeps documents");
            let xml = crate::comicinfo::xml(
                &crate::comicinfo::read(&conn, &slug, &edition_key, 2).expect("reads"),
            );
            let elsewhere = crate::tests::tempdir("stamp-copy");
            let copy = elsewhere.path().join("copy.cbz");
            std::fs::copy(&archive, &copy).expect("copies the archive");
            let digest = super::rewrite_one(&copy, xml.as_bytes()).expect("stamps the copy");
            let key = sha256_hex(format!("{digest}\n").as_bytes());
            let taken = document.with_file_name(crate::hand::file_name(&key));
            // A document left at that address by an earlier rekey, still
            // naming this work's member by its digest before the stamp.
            let mut orphan: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&document).expect("reads"))
                    .expect("JSON");
            orphan["title"] = serde_json::json!("The Orphan");
            let orphan = orphan.to_string();
            std::fs::write(&taken, &orphan).expect("a file at the new address");
            (document, key, taken, orphan)
        };

        let applied = stamp_all(&fixture);
        let finished = std::time::SystemTime::now();
        let learnt = applied.content_key.as_deref() == Some(key.as_str());
        if !learnt && tick(started) != tick(finished) {
            continue;
        }
        assert!(
            learnt,
            "within one tick, the copy's stamp gives the work's new key: {applied:?}"
        );

        assert!(!applied.document_moved, "the rename was refused");
        assert_eq!(
            std::fs::read_to_string(&taken).expect("still there"),
            orphan,
            "the file at the new address is untouched"
        );
        let stranded = std::fs::read_to_string(&document).expect("left at the old address");
        assert!(
            stranded.contains(&applied.members[0].1) && !stranded.contains(&old),
            "the work's own document is re-keyed where it stands: {stranded}"
        );
        assert!(
            !applied.document_rewritten,
            "and is not reported as the document at the work's address: {applied:?}"
        );
        return;
    }
    panic!("five attempts in a row straddled a two-second tick");
}

/// A stamp that fails after renaming its archives, here in the transaction
/// that records their digests, has stamped those files whatever the rows say.
/// The rows roll back, the aliases of the digests it moved among them (O240).
/// The document follows the files as on a cut, so the rescan that records the
/// digests carries a document that names them, and a rebuild keeps the chapter.
#[test]
fn a_stamp_whose_transaction_fails_after_the_renames_still_rekeys_the_hand_document() {
    for (statement, trigger) in [
        (
            "recording the new digest",
            "BEFORE UPDATE OF sha256 ON member",
        ),
        (
            "recording the new content key",
            "BEFORE UPDATE OF content_key ON work",
        ),
        (
            "remembering the digests the stamp moved",
            "BEFORE INSERT ON stamp_alias",
        ),
    ] {
        let fixture = one_work();
        let old = hand_chaptered(&fixture);
        let archive = fixture.archive("Series - v01.cbz");

        let failed = {
            let conn = fixture.conn();
            conn.execute_batch(&format!(
                "CREATE TEMP TRIGGER fail_the_stamp {trigger}
                 BEGIN SELECT RAISE(ABORT, 'the transaction fails'); END;"
            ))
            .expect("a trigger that fails the transaction");
            let failed = apply(
                &conn,
                fixture.work_id,
                1,
                &plan(&conn, fixture.work_id).expect("plans"),
            );
            conn.execute_batch("DROP TRIGGER fail_the_stamp")
                .expect("drops the trigger");
            failed
        };

        let failed = failed.expect_err("the transaction fails");
        assert!(
            failed.contains(statement) && failed.contains("the transaction fails"),
            "{failed}"
        );
        assert!(
            failed.ends_with(
                " — 1 of this work's archives were rewritten before this and their rows \
                 are now stale; a rescan reconciles them"
            ),
            "and says the archive is stamped and its row is not ({statement}): {failed}"
        );
        let new = digest_of(&archive);
        assert_ne!(new, old, "the archive was stamped");
        let row: String = fixture
            .conn()
            .query_row("SELECT sha256 FROM member", [], |r| r.get(0))
            .expect("the member's digest");
        assert_eq!(row, old, "and the rows were rolled back");
        let aliases: i64 = fixture
            .conn()
            .query_row("SELECT COUNT(*) FROM stamp_alias", [], |r| r.get(0))
            .expect("counts the aliases");
        assert_eq!(aliases, 0, "and no alias stands ({statement})");
        let document = document_at_address(&fixture);
        assert!(
            document.contains(&new) && !document.contains(&old),
            "the document follows the stamped file ({statement}): {document}"
        );

        {
            let conn = fixture.conn();
            let root = crate::scan::roots(&conn).expect("roots").remove(0);
            crate::scan::scan_root(&conn, &root).expect("the rescan");
            crate::derive::derive(&conn).expect("the derive");
        }
        let carried = document_at_address(&fixture);
        assert!(
            carried.contains(&new),
            "the rescan carries it to the new key ({statement}): {carried}"
        );
        assert_eq!(
            rebuilt_chapter(fixture.dir.path(), true),
            (Some(7.0), Some("hand".to_string())),
            "a library rebuilt from the directory keeps the chapter ({statement})"
        );
    }
}

/// An error after the commit comes before the document moves. The stamp is
/// recorded, so the answer says so and names where the document stayed, and
/// the next scan reports it there as an orphan, as the answer says. A root
/// path that stops being text once the key moves stands in for any read
/// failing after the commit.
#[test]
fn an_error_after_the_commit_says_the_stamp_is_recorded_and_where_the_document_stayed() {
    let fixture = one_work();
    hand_chaptered(&fixture);
    let archive = fixture.archive("Series - v01.cbz");
    let (old, root_path) = {
        let conn = fixture.conn();
        let old = crate::hand::path_for(&conn, fixture.work_id)
            .expect("resolves")
            .expect("the root keeps documents");
        let root_path: String = conn
            .query_row("SELECT path FROM root", [], |r| r.get(0))
            .expect("the root's path");
        (old, root_path)
    };

    let failed = {
        let conn = fixture.conn();
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_after_the_commit AFTER UPDATE OF content_key ON work
             BEGIN UPDATE root SET path = X'00'; END;",
        )
        .expect("a trigger that spoils the root's path with the key");
        let failed = apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        );
        conn.execute_batch("DROP TRIGGER fail_after_the_commit")
            .expect("drops the trigger");
        conn.execute("UPDATE root SET path = ?1", [&root_path])
            .expect("restores the root's path");
        failed
    };

    let failed = failed.expect_err("the address cannot be read after the commit");
    assert!(
        failed.ends_with(&format!(
            " — the stamp itself is recorded, but this work's hand document, if it has \
             one, was not moved to its new key and stays at {}, where the next scan \
             reports it as an orphan",
            old.display()
        )),
        "{failed}"
    );
    let row: String = fixture
        .conn()
        .query_row("SELECT sha256 FROM member", [], |r| r.get(0))
        .expect("the member's digest");
    assert_eq!(row, digest_of(&archive), "the stamp is recorded");
    assert!(old.is_file(), "the document stayed at {old:?}");

    let report = {
        let conn = fixture.conn();
        let root = crate::scan::roots(&conn).expect("roots").remove(0);
        crate::scan::scan_root(&conn, &root).expect("the rescan");
        crate::derive::derive(&conn).expect("the derive")
    };
    assert!(
        report.orphan_documents.iter().any(|o| o.path == old),
        "the next scan reports it as an orphan: {:?}",
        report.orphan_documents
    );
}

/// A stamp of spoke-edition archives alone leaves the work's key, and so the
/// hand document's address, where they were. The document there still keys
/// the spoke member by its digest, so it is re-keyed in place and reported.
#[test]
fn a_stamp_of_a_spoke_edition_alone_rekeys_the_hand_document_where_it_stands() {
    let hub = crate::tests::tempdir("stamp-hub");
    crate::scan::tests::write_archive(&hub.path().join("Series").join("Series - v01.cbz"), 2);
    let spoke = crate::tests::tempdir("stamp-spoke");
    let spoke_archive = spoke.path().join("Series").join("Series - v01.cbz");
    crate::scan::tests::write_archive(&spoke_archive, 3);
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let work_id = {
        let conn = library.conn();
        for (dir, edition) in [(hub.path(), Some("en")), (spoke.path(), None)] {
            let root = crate::scan::add_root(&conn, dir).expect("adding the root");
            crate::scan::update_root(
                &conn,
                root.id,
                &crate::scan::RootPatch {
                    edition_key: Some(edition.map(str::to_string)),
                    ..Default::default()
                },
            )
            .expect("sets the root's edition");
        }
        conn.execute("UPDATE root SET keeps_documents = 1", [])
            .expect("the roots keep documents");
        for root in crate::scan::roots(&conn).expect("roots") {
            crate::scan::scan_root(&conn, &root).expect("the scan");
        }
        crate::derive::derive(&conn).expect("the derive");
        let (work_id, hub_key): (i64, String) = conn
            .query_row("SELECT id, hub FROM work", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("one work of both editions");
        assert_eq!(
            hub_key, "en",
            "the fixture: the first root's edition is the hub"
        );
        work_id
    };
    let fixture = Fixture {
        library,
        dir: hub,
        work_id,
    };
    // Both archives stamped, so a later plan names only what changes after.
    stamp_all(&fixture);
    let old = {
        let conn = fixture.conn();
        let old: String = conn
            .query_row(
                "SELECT m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
                  WHERE e.key = 'raw'",
                [],
                |r| r.get(0),
            )
            .expect("the spoke member's digest");
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projects");
        doc.members = vec![crate::librarydoc::MemberMeta {
            sha256: old.clone(),
            chapter: Some(7.0),
            chapter_source: Some(crate::librarydoc::ChapterSource::Hand),
        }];
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applies the hand edit");
        let saved = crate::hand::save(&conn, work_id).expect("saves the hand document");
        assert!(saved.persisted, "the document is on disk: {saved:?}");
        // The spoke edition's ComicInfo alone now differs from its archive's.
        conn.execute("UPDATE edition SET language = 'ja' WHERE key = 'raw'", [])
            .expect("sets the spoke edition's language");
        old
    };

    let applied = {
        let conn = fixture.conn();
        let planned = plan(&conn, work_id).expect("plans");
        assert_eq!(
            planned.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
            std::slice::from_ref(&spoke_archive),
            "only the spoke edition's archive is planned"
        );
        apply(&conn, work_id, 1, &planned).expect("applies")
    };

    assert_eq!(
        applied.content_key, None,
        "the hub held, and the key with it"
    );
    let new = digest_of(&spoke_archive);
    assert_ne!(new, old, "the spoke archive was stamped");
    let document = document_at_address(&fixture);
    assert!(
        document.contains(&new) && !document.contains(&old),
        "the document at the work's address names the spoke member's new digest: {document}"
    );
    assert!(
        applied.document_rewritten,
        "and the stamp says it rewrote the work's document: {applied:?}"
    );
}

/// The stamp could neither rewrite a newer taiyaku's hand document nor leave
/// it naming the old digests, so it refuses before it touches any archive:
/// `newer` is written at the work's address, in a root that keeps documents or
/// declines them, and the refusal must be `sentence` after the file's path.
fn a_stamp_over_a_newer_hand_document_is_refused(
    keeps_documents: bool,
    newer: &str,
    sentence: &str,
) {
    let fixture = one_work();
    let archive = fixture.archive("Series - v01.cbz");
    let document = {
        let conn = fixture.conn();
        conn.execute(
            "UPDATE root SET keeps_documents = ?1",
            [i64::from(keeps_documents)],
        )
        .expect("sets whether the root keeps documents");
        let path = crate::hand::path_for(&conn, fixture.work_id)
            .expect("resolves")
            .expect("the work has a root");
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creates .taiyaku");
        std::fs::write(&path, newer).expect("a newer taiyaku's document");
        path
    };
    let before = digest_of(&archive);

    let (refused, row) = {
        let conn = fixture.conn();
        let refused = apply(
            &conn,
            fixture.work_id,
            1,
            &plan(&conn, fixture.work_id).expect("plans"),
        );
        let row: String = conn
            .query_row("SELECT sha256 FROM member", [], |r| r.get(0))
            .expect("the member's digest");
        (refused, row)
    };

    let refusal = refused.expect_err("a newer hand document refuses the stamp");
    assert_eq!(
        refusal,
        format!("{}: {sentence}", document.display()),
        "refuse_newer's sentence, naming the file"
    );
    assert_eq!(digest_of(&archive), before, "no archive's bytes changed");
    assert_eq!(row, before, "and the member row still names them");
    assert_eq!(
        std::fs::read_to_string(&document).expect("still there"),
        newer,
        "and the document is as the newer taiyaku left it"
    );
}

#[test]
fn a_stamp_over_a_hand_document_a_newer_taiyaku_wrote_is_refused_before_any_archive_is_touched() {
    a_stamp_over_a_newer_hand_document_is_refused(
        true,
        r#"{"taiyaku_library": 2, "from": "newer"}"#,
        "this document declares taiyaku_library format version 2, and this build reads 1. \
         Refusing to read it rather than dropping the fields it does not understand and \
         writing them back out as version 1 — upgrade taiyaku instead",
    );
}

/// A stamp re-keys an existing hand document in a root that declines
/// documents too, so a newer one there would be left naming the old digests
/// just the same.
#[test]
fn a_stamp_over_a_newer_hand_document_in_a_root_that_declines_documents_is_refused_too() {
    a_stamp_over_a_newer_hand_document_is_refused(
        false,
        r#"{"taiyaku_library": 2, "from": "newer"}"#,
        "this document declares taiyaku_library format version 2, and this build reads 1. \
         Refusing to read it rather than dropping the fields it does not understand and \
         writing them back out as version 1 — upgrade taiyaku instead",
    );
}

/// A version past `u32` is newer than any build, and is named at the largest
/// rather than read as an older one.
#[test]
fn a_stamp_over_a_hand_document_declaring_a_version_past_u32_is_refused_too() {
    a_stamp_over_a_newer_hand_document_is_refused(
        true,
        r#"{"taiyaku_library": 4294967296, "from": "newer"}"#,
        "this document declares taiyaku_library format version 4294967295, and this build \
         reads 1. Refusing to read it rather than dropping the fields it does not understand \
         and writing them back out as version 1 — upgrade taiyaku instead",
    );
}

/// A root that declines documents keeps no hand document, even after a hand
/// edit, so the stamp has none to bring in line: it succeeds and writes none.
#[test]
fn a_stamp_in_a_root_that_declines_documents_succeeds_and_writes_no_hand_document() {
    let fixture = one_work();
    {
        let conn = fixture.conn();
        let mut doc = crate::librarydoc::of(&conn, fixture.work_id).expect("projects");
        doc.title = Some("Hand-Set Title".to_string());
        crate::librarydoc::apply(&conn, fixture.work_id, &doc).expect("applies the hand edit");
        let saved = crate::hand::save(&conn, fixture.work_id).expect("saves");
        assert_eq!(
            saved.keeps_documents,
            Some(false),
            "the fixture's root declines documents"
        );
    }

    let conn = fixture.conn();
    let applied = apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("a root that declines documents still stamps");

    assert!(
        !fixture.dir.path().join(".taiyaku").exists(),
        "and no hand document was written"
    );
    assert!(!applied.document_rewritten, "nor said to be: {applied:?}");
}

/// The hand document at the work's address, after `edit` has rewritten its
/// file as JSON text. Answers that text.
fn edit_document(fixture: &Fixture, edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let conn = fixture.conn();
    let path = crate::hand::path_for(&conn, fixture.work_id)
        .expect("resolves")
        .expect("the work has an address");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("reads"))
            .expect("the saved document is JSON");
    edit(&mut doc);
    let text = doc.to_string();
    std::fs::write(&path, &text).expect("an edit made in the file");
    text
}

/// A document that names no member this stamp rewrote has nothing to re-key:
/// it moves with the work's key and is otherwise left exactly as it was.
#[test]
fn a_hand_document_naming_no_stamped_member_is_left_as_it_is() {
    let fixture = one_work();
    {
        let conn = fixture.conn();
        conn.execute("UPDATE root SET keeps_documents = 1", [])
            .expect("the root keeps documents");
        let mut doc = crate::librarydoc::of(&conn, fixture.work_id).expect("projects");
        doc.title = Some("Hand-Set Title".to_string());
        crate::librarydoc::apply(&conn, fixture.work_id, &doc).expect("applies the hand edit");
        crate::hand::save(&conn, fixture.work_id).expect("saves");
    }
    let before = document_at_address(&fixture);

    let applied = stamp_all(&fixture);

    assert!(applied.document_moved, "the document moved with the key");
    assert_eq!(
        document_at_address(&fixture),
        before,
        "and is otherwise as it was"
    );
    assert!(
        !applied.document_rewritten,
        "so it is not reported rewritten: {applied:?}"
    );
}

/// A file at the address that is not a library document this build reads is
/// the person's to fix, so the stamp leaves it as it is.
#[test]
fn a_hand_document_this_build_cannot_read_is_left_as_it_is() {
    let fixture = one_work();
    let old = hand_chaptered(&fixture);
    let broken = edit_document(&fixture, |doc| doc["title"] = serde_json::json!(5));
    assert!(broken.contains(&old), "it names the member's digest");

    let applied = stamp_all(&fixture);

    assert_eq!(
        document_at_address(&fixture),
        broken,
        "the unreadable document is left as it is"
    );
    assert!(
        !applied.document_rewritten,
        "and is not reported rewritten: {applied:?}"
    );
}

/// The re-key changes members' digests alone. The same digest quoted anywhere
/// else in the file, here as a tag, is left as it was.
#[test]
fn a_digest_quoted_outside_the_members_is_not_rekeyed() {
    let fixture = one_work();
    let old = hand_chaptered(&fixture);
    let tag = old.clone();
    edit_document(&fixture, move |doc| doc["tags"] = serde_json::json!([tag]));

    let applied = stamp_all(&fixture);

    let doc: serde_json::Value =
        serde_json::from_str(&document_at_address(&fixture)).expect("JSON");
    assert_eq!(
        doc["members"][0]["sha256"],
        serde_json::json!(applied.members[0].1),
        "the member is re-keyed: {doc}"
    );
    assert_eq!(
        doc["tags"],
        serde_json::json!([old]),
        "and the tag is not: {doc}"
    );
}

/// A stamp is not an edit: a work nobody has edited has no hand document, and
/// the stamp starts none, though `librarydoc::of` projects a chapter for it.
#[test]
fn a_stamp_writes_no_hand_document_for_a_work_nobody_has_edited() {
    let fixture = one_work();
    {
        let conn = fixture.conn();
        conn.execute("UPDATE root SET keeps_documents = 1", [])
            .expect("the root keeps documents");
        // What the `builtin-chapter` rule records for a chaptered filename.
        conn.execute(
            "UPDATE member SET chapter_declared = 3.0, chapter_source = 'filename'",
            [],
        )
        .expect("a chapter the rules read");
        assert!(
            !crate::librarydoc::of(&conn, fixture.work_id)
                .expect("projects")
                .is_empty(),
            "a save would have a document to write"
        );
    }

    let conn = fixture.conn();
    let applied = apply(
        &conn,
        fixture.work_id,
        1,
        &plan(&conn, fixture.work_id).expect("plans"),
    )
    .expect("applies");

    assert!(
        !fixture.dir.path().join(".taiyaku").exists(),
        "the stamp started no hand document"
    );
    assert!(
        !applied.document_rewritten,
        "nor said it rewrote one: {applied:?}"
    );
}

/// O104, for the archive itself. `rewrite_one` stages a whole new archive
/// beside the original and renames it over the top, and its own doc promises
/// that "a crash leaves either the old archive or the new one, never a
/// half-written file". That holds only for bytes that reached the disk: the
/// rename can be journalled first, and on the NAS's XFS array nothing forces
/// the data out ahead of it.
///
/// The same ordering `atomic::write` is held to, asserted the same way — on
/// the real syscalls, in a child process under strace — because a stamp is
/// the one writer in this tree that replaces a file nothing can regenerate.
#[cfg(target_os = "linux")]
#[test]
fn a_rewrite_syncs_the_staged_archive_before_the_rename_and_the_directory_after_it() {
    use taiyaku_read::test_support::{assert_replaced_durably, traced};

    const CHILD: &str = "TAIYAKU_TEST_STAMP_O104_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        super::rewrite_one(std::path::Path::new(&path), b"<ComicInfo/>")
            .expect("the traced rewrite");
        return;
    }

    let temp = crate::tests::tempdir("stamp-o104");
    let dir = std::fs::canonicalize(temp.path()).expect("the test directory resolves");
    let path = dir.join("Series - v01.cbz");
    crate::scan::tests::write_archive(&path, 2);
    let before = digest_of(&path);

    let calls = traced(
        "stamp::tests::a_rewrite_syncs_the_staged_archive_before_the_rename_and_the_directory_after_it",
        (CHILD, path.as_os_str()),
        &dir.join("strace.log"),
    );

    assert_ne!(
        digest_of(&path),
        before,
        "the traced child must actually have rewritten the archive"
    );
    assert_replaced_durably(&calls, &path, &dir);
}

/// Note `id` on `edition`'s `slot`.
fn a_note(id: &str, edition: &str, slot: u32) -> crate::Note {
    crate::Note {
        text: None,
        id: id.into(),
        edition: edition.into(),
        slot,
        region: None,
        color: None,
        body: format!("{id}'s words"),
        created: "2026-11-10T10:00:00Z".into(),
        updated: "2026-11-10T10:00:00Z".into(),
        stale: false,
        at: None,
    }
}

/// Account 1's notes on `work_id`, through `state::`, which anchors each.
fn hold_notes(conn: &Connection, work_id: i64, notes: Vec<crate::Note>) {
    hold_notes_as(conn, work_id, 1, notes);
}

/// Account `user_id`'s notes on `work_id`, through `state::`.
fn hold_notes_as(conn: &Connection, work_id: i64, user_id: i64, notes: Vec<crate::Note>) {
    crate::state::put_notes(
        conn,
        work_id,
        user_id,
        &crate::Notes {
            taiyaku_notes: crate::NOTES_VERSION,
            work: "w".into(),
            notes,
        },
    )
    .expect("writes the notes");
}

/// Account 1's position on `edition`, through `state::`, which anchors it.
fn put_position(conn: &Connection, work_id: i64, edition: &str, slot: u32) {
    crate::state::put_progress(
        conn,
        work_id,
        1,
        &crate::Progress {
            edition: edition.into(),
            slot,
        },
    )
    .expect("writes the position");
}

/// Each of account 1's notes as `state::notes` answers it: id, edition, slot
/// and `stale`.
fn notes_on(conn: &Connection, work_id: i64) -> Vec<(String, String, u32, bool)> {
    notes_of(conn, work_id, 1)
}

/// Each of account `user_id`'s notes, as [`notes_on`] reads account 1's.
fn notes_of(conn: &Connection, work_id: i64, user_id: i64) -> Vec<(String, String, u32, bool)> {
    crate::state::notes(conn, work_id, user_id)
        .expect("reads the notes")
        .notes
        .into_iter()
        .map(|note| (note.id, note.edition, note.slot, note.stale))
        .collect()
}

fn on(id: &str, edition: &str, slot: u32, stale: bool) -> (String, String, u32, bool) {
    (id.into(), edition.into(), slot, stale)
}

/// Each position as `state::progress` answers it.
fn positions_on(conn: &Connection, work_id: i64) -> Vec<(String, u32)> {
    crate::state::progress(conn, work_id, 1)
        .expect("reads the positions")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect()
}

/// The digest each of `work_id`'s positions is anchored on, by edition: the
/// column the re-key writes, which a lost anchor's clamp can hide from a read.
fn position_anchors(conn: &Connection, work_id: i64) -> Vec<(String, String)> {
    conn.prepare("SELECT edition, at_sha256 FROM progress WHERE work_id = ?1 ORDER BY edition")
        .and_then(|mut statement| {
            statement
                .query_map([work_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect()
        })
        .expect("reads the anchors")
}

/// The digest of each of `work_id`'s members, by edition key.
fn member_digests(conn: &Connection, work_id: i64) -> Vec<(String, String)> {
    conn.prepare(
        "SELECT e.key, m.sha256 FROM member m JOIN edition e ON e.id = m.edition_id
          WHERE e.work_id = ?1 ORDER BY e.key, m.ordinal",
    )
    .and_then(|mut statement| {
        statement
            .query_map([work_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    })
    .expect("reads the members")
}

/// One edition listing one archive twice: `Series - v01 - c001.cbz` and a
/// byte-identical `c002`, three pages each, which the derive keeps as two
/// members. Account 1 holds n1 on c001's second page (slot 1), n2 on c002's
/// third (slot 5) and its position on c002's second (slot 4), so each copy
/// has rows of its own.
struct TwoCopies {
    fixture: Fixture,
    root: crate::scan::Root,
    c001: i64,
    c002: i64,
}

fn two_copies() -> TwoCopies {
    let dir = crate::tests::tempdir("stamp-copies");
    let c001 = dir.path().join("Series").join("Series - v01 - c001.cbz");
    crate::scan::tests::write_archive(&c001, 3);
    std::fs::copy(
        &c001,
        dir.path().join("Series").join("Series - v01 - c002.cbz"),
    )
    .expect("copies c001");

    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let (root, work_id, members) = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("the scan");
        crate::derive::derive(&conn).expect("the derive");
        let members: Vec<(i64, i64, String, String)> = conn
            .prepare(
                "SELECT m.id, e.work_id, e.key, m.sha256 FROM member m
                   JOIN edition e ON e.id = m.edition_id ORDER BY m.ordinal",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .collect()
            })
            .expect("reads the members");
        assert_eq!(members.len(), 2, "the fixture: two members");
        assert_eq!(members[0].1, members[1].1, "the fixture: one work");
        assert_eq!(
            (members[0].2.as_str(), members[1].2.as_str()),
            ("raw", "raw"),
            "the fixture: one edition, raw"
        );
        assert_eq!(members[0].3, members[1].3, "the fixture: one digest");
        let work_id = members[0].1;
        put_position(&conn, work_id, "raw", 4);
        hold_notes(
            &conn,
            work_id,
            vec![a_note("n1", "raw", 1), a_note("n2", "raw", 5)],
        );
        (root, work_id, members)
    };

    TwoCopies {
        fixture: Fixture {
            library,
            dir,
            work_id,
        },
        root,
        c001: members[0].0,
        c002: members[1].0,
    }
}

/// The stamp's transaction over `written`, as if its rewrites had given those
/// digests, so a test chooses which copies the stamp wrote and what each got.
fn record_digests(conn: &Connection, work_id: i64, written: &[(i64, String)]) {
    let key: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [work_id],
            |row| row.get(0),
        )
        .expect("the work's key");
    record_rows(conn, work_id, &key, written).expect("records the stamp");
}

/// c001 out of the edition, as a removal would take it: c002's pages become
/// slots 0 to 2, so a row on c002 moves there and a row on c001 has no page.
fn without_c001(conn: &Connection, copies: &TwoCopies) {
    conn.execute("DELETE FROM member WHERE id = ?1", [copies.c001])
        .expect("deletes c001's member");
}

/// The notes and the positions `state::` answers once c001 is gone.
type Answers = (Vec<(String, String, u32, bool)>, Vec<(String, u32)>);

fn once_c001_is_gone(conn: &Connection, copies: &TwoCopies) -> Answers {
    without_c001(conn, copies);
    let work_id = copies.fixture.work_id;
    (notes_on(conn, work_id), positions_on(conn, work_id))
}

/// A stamp gives the two copies different digests whenever their documents
/// are started in different two-second windows. Each copy's rows move to its
/// own copy's new digest, not to whichever copy was recorded first.
#[test]
fn a_stamp_moves_each_copys_rows_to_that_copys_new_digest() {
    let copies = two_copies();
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    record_digests(
        &conn,
        work_id,
        &[(copies.c001, "1".repeat(64)), (copies.c002, "2".repeat(64))],
    );

    assert_eq!(
        notes_on(&conn, work_id),
        vec![on("n1", "raw", 1, false), on("n2", "raw", 5, false)],
        "each note on its own copy's page"
    );
    assert_eq!(
        once_c001_is_gone(&conn, &copies),
        (
            vec![on("n1", "raw", 1, true), on("n2", "raw", 2, false)],
            vec![("raw".to_string(), 1)]
        ),
        "c002's rows on its pages, now slots 0 to 2, and c001's note stale"
    );
}

/// A stamp that rewrites c001 and not c002 leaves c002 the only copy with the
/// old digest, so c002 is now that digest's first copy; its rows go with it,
/// and none go to c001.
#[test]
fn a_copy_the_stamp_does_not_rewrite_keeps_its_rows() {
    let copies = two_copies();
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    record_digests(&conn, work_id, &[(copies.c001, "1".repeat(64))]);

    assert_eq!(
        notes_on(&conn, work_id),
        vec![on("n1", "raw", 1, false), on("n2", "raw", 5, false)],
        "each note on its own copy's page"
    );
    assert_eq!(
        once_c001_is_gone(&conn, &copies),
        (
            vec![on("n1", "raw", 1, true), on("n2", "raw", 2, false)],
            vec![("raw".to_string(), 1)]
        ),
        "c002's rows on its pages, now slots 0 to 2, and c001's note stale"
    );
}

/// c001's file is gone at the stamp, so the plan leaves it out and the stamp
/// rewrites c002 alone. c001's note stays on c001 while it is listed, and
/// c002's rows stay on c002 after the scan drops c001.
#[test]
fn a_copy_the_plan_leaves_out_keeps_its_rows_through_the_next_scan() {
    let copies = two_copies();
    std::fs::remove_file(copies.fixture.archive("Series - v01 - c001.cbz"))
        .expect("removes c001's file");
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    let planned = plan(&conn, work_id).expect("plans");
    assert_eq!(
        planned.iter().map(|p| p.member_id).collect::<Vec<_>>(),
        vec![copies.c002],
        "the fixture: the stamp rewrites c002 alone"
    );
    apply(&conn, work_id, 1, &planned).expect("stamps");

    assert_eq!(
        notes_on(&conn, work_id),
        vec![on("n1", "raw", 1, false), on("n2", "raw", 5, false)],
        "each note on its own copy's page"
    );

    crate::scan::scan_root(&conn, &copies.root).expect("scans again");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (notes_on(&conn, work_id), positions_on(&conn, work_id)),
        (
            vec![on("n1", "raw", 1, true), on("n2", "raw", 2, false)],
            vec![("raw".to_string(), 1)]
        ),
        "c002's rows on its pages, now slots 0 to 2, and c001's note stale"
    );
}

/// A stamp whose two documents start in one two-second window gives both
/// copies one new digest, so c002 is still that digest's second copy and its
/// rows stay with it rather than going to c001's pages. The position's page is
/// also its stored slot, which a lost anchor falls back to, so its digest is
/// read as well.
#[test]
fn a_stamp_giving_both_copies_one_digest_keeps_each_copys_rows() {
    let copies = two_copies();
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    record_digests(
        &conn,
        work_id,
        &[(copies.c001, "1".repeat(64)), (copies.c002, "1".repeat(64))],
    );

    assert_eq!(
        (notes_on(&conn, work_id), positions_on(&conn, work_id)),
        (
            vec![on("n1", "raw", 1, false), on("n2", "raw", 5, false)],
            vec![("raw".to_string(), 4)]
        ),
        "each row on its own copy's page"
    );
    assert_eq!(
        position_anchors(&conn, work_id),
        vec![("raw".to_string(), "1".repeat(64))],
        "the position moved to the new digest"
    );
}

/// [`one_work`]'s archive stamped, then derived before the next walk of its
/// directory, as a scan of another directory or a rule save derives. Answers
/// the archive's digest after the stamp.
fn stamp_then_derive(conn: &Connection, fixture: &Fixture) -> String {
    let work_id = fixture.work_id;
    let before = member_digests(conn, work_id);
    apply(conn, work_id, 1, &plan(conn, work_id).expect("plans")).expect("stamps");
    let new = digest_of(&fixture.archive("Series - v01.cbz"));
    assert_ne!(
        before,
        vec![("raw".to_string(), new.clone())],
        "the fixture: the stamp gave the archive a new digest"
    );
    crate::derive::derive(conn).expect("derives");
    new
}

/// O270: the derive reads the archive's directory row, which the stamp moved
/// with the member, so it leaves the member on the stamp's digest.
#[test]
fn a_derive_before_the_next_walk_keeps_a_stamps_digest() {
    let fixture = one_work();
    let conn = fixture.conn();
    let new = stamp_then_derive(&conn, &fixture);

    assert_eq!(
        member_digests(&conn, fixture.work_id),
        vec![("raw".to_string(), new)],
        "after the derive"
    );
}

/// O270: a position and a note written between that derive and the next walk
/// are anchored on the stamp's digest, which the walk's derive hashes again.
#[test]
fn the_rows_written_before_the_next_walk_stay_on_a_stamps_digest() {
    let fixture = one_work();
    let conn = fixture.conn();
    let work_id = fixture.work_id;
    let new = stamp_then_derive(&conn, &fixture);
    put_position(&conn, work_id, "raw", 1);
    hold_notes(&conn, work_id, vec![a_note("n1", "raw", 1)]);

    let roots = crate::scan::roots(&conn).expect("lists the roots");
    assert_eq!(roots.len(), 1, "the fixture: one directory");
    crate::scan::scan_root(&conn, &roots[0]).expect("walks the directory again");
    crate::derive::derive(&conn).expect("derives");
    assert_eq!(
        (position_anchors(&conn, work_id), notes_on(&conn, work_id)),
        (
            vec![("raw".to_string(), new)],
            vec![on("n1", "raw", 1, false)]
        ),
        "the rows written in the window stay on the new digest"
    );
}

/// Each archive's directory row as `(rel_path, sha256)`.
fn entry_digests(conn: &Connection) -> Vec<(String, Option<String>)> {
    conn.prepare(
        "SELECT rel_path, sha256 FROM scan_entry WHERE rel_path LIKE '%.cbz' ORDER BY rel_path",
    )
    .and_then(|mut statement| {
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    })
    .expect("reads the directory rows")
}

/// O270: c001 and c002 lie under one directory with one digest, so the stamp
/// finds c001's row by its path; by its digest it would move c002's too.
#[test]
fn a_stamp_writes_its_digest_to_the_row_at_the_stamped_path_alone() {
    let copies = two_copies();
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    let old = member_digests(&conn, work_id).remove(0).1;
    assert_eq!(
        entry_digests(&conn),
        vec![
            (
                "Series/Series - v01 - c001.cbz".to_string(),
                Some(old.clone())
            ),
            (
                "Series/Series - v01 - c002.cbz".to_string(),
                Some(old.clone())
            ),
        ],
        "the fixture: both rows on the one digest"
    );
    record_digests(&conn, work_id, &[(copies.c001, "1".repeat(64))]);

    assert_eq!(
        entry_digests(&conn),
        vec![
            (
                "Series/Series - v01 - c001.cbz".to_string(),
                Some("1".repeat(64))
            ),
            ("Series/Series - v01 - c002.cbz".to_string(), Some(old)),
        ],
        "c001's row takes the new digest; c002's keeps the old"
    );
}

/// A second directory holds other bytes at the same path within it, so the
/// stamp finds the row by the member's directory as well as by its path.
#[test]
fn a_stamp_writes_no_row_of_another_directory_at_the_same_path() {
    let fixture = one_work();
    let other = crate::tests::tempdir("stamp-other");
    let theirs = other.path().join("Series").join("Series - v01.cbz");
    crate::scan::tests::write_archive(&theirs, 3);
    let conn = fixture.conn();
    let root = crate::scan::add_root(&conn, other.path()).expect("adds the other directory");
    crate::scan::scan_root(&conn, &root).expect("scans it");
    crate::derive::derive(&conn).expect("derives");
    let rows = || -> Vec<Option<String>> {
        conn.prepare(
            "SELECT sha256 FROM scan_entry WHERE rel_path = 'Series/Series - v01.cbz'
              ORDER BY root_id",
        )
        .and_then(|mut statement| statement.query_map([], |row| row.get(0))?.collect())
        .expect("reads the directory rows")
    };
    let ours = digest_of(&fixture.archive("Series - v01.cbz"));
    let theirs = digest_of(&theirs);
    assert_eq!(
        rows(),
        vec![Some(ours.clone()), Some(theirs.clone())],
        "the fixture: a row in each directory, each on its own digest"
    );
    let first = crate::scan::roots(&conn)
        .expect("lists the roots")
        .into_iter()
        .map(|listed| listed.id)
        .find(|id| *id != root.id)
        .expect("the fixture: the first directory");
    let members: Vec<(i64, i64, Option<i64>)> = conn
        .prepare(
            "SELECT m.id, e.work_id, m.root_id FROM member m
               JOIN edition e ON e.id = m.edition_id WHERE m.sha256 = ?1",
        )
        .and_then(|mut statement| {
            statement
                .query_map([&ours], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect()
        })
        .expect("reads the members");
    assert_eq!(
        members
            .iter()
            .map(|(_, _, root_id)| *root_id)
            .collect::<Vec<_>>(),
        vec![Some(first)],
        "the fixture: one member is the first directory's archive"
    );
    let (member_id, work_id, _) = members[0];
    record_digests(&conn, work_id, &[(member_id, "1".repeat(64))]);

    assert_eq!(
        rows(),
        vec![Some("1".repeat(64)), Some(theirs)],
        "the stamped directory's row takes the new digest; the other keeps its own"
    );
}

/// A held reading is the one the works were written from. Where it is the
/// digest the stamp replaced, the works now hold the new digest, so it moves;
/// a held reading of other bytes is not the stamp's to move. Each row holds a
/// whole reading beside the digest a walk leaves: c001's taken since by a
/// derive that wrote no work, and c002's cleared, so the row's own digest
/// decides nothing.
#[test]
fn a_stamp_moves_a_held_reading_only_where_it_is_the_digest_replaced() {
    let copies = two_copies();
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    let old = member_digests(&conn, work_id).remove(0).1;
    let c001 = "Series/Series - v01 - c001.cbz";
    let c002 = "Series/Series - v01 - c002.cbz";
    for (rel_path, digest, held) in [
        (c001, Some("e".repeat(64)), old),
        (c002, None, "7".repeat(64)),
    ] {
        let set = conn
            .execute(
                "UPDATE scan_entry SET sha256 = ?1, held_sha256 = ?2, held_format = 'zip',
                        held_pages = 3, held_title = NULL
                  WHERE root_id = ?3 AND rel_path = ?4",
                rusqlite::params![digest, held, copies.root.id, rel_path],
            )
            .expect("holds a reading");
        assert_eq!(set, 1, "the fixture: one row at {rel_path}");
    }
    record_digests(
        &conn,
        work_id,
        &[(copies.c001, "1".repeat(64)), (copies.c002, "2".repeat(64))],
    );

    let held = |rel_path| crate::scan::tests::held_reading(&conn, copies.root.id, rel_path);
    let reading = |sha256: String| (Some(sha256), Some("zip".to_string()), Some(3), None);
    assert_eq!(
        (held(c001), held(c002)),
        (reading("1".repeat(64)), reading("7".repeat(64))),
        "c001's held reading moves to its new digest, the rest kept; c002's, of other bytes, stays"
    );
}

/// Two accounts can hold a note with one id on one work (migration 005): here
/// account 2's n1 is on c002's second page and account 1's on c001's. Each
/// account's note moves by its own anchor, not by the other's.
#[test]
fn a_stamp_moves_each_accounts_note_by_its_own_anchor() {
    let copies = two_copies();
    let conn = copies.fixture.conn();
    let work_id = copies.fixture.work_id;
    conn.execute(
        "INSERT INTO user (id, name, password_hash, role) VALUES (2, 'second', '!', 'user')",
        [],
    )
    .expect("a second account");
    hold_notes_as(&conn, work_id, 2, vec![a_note("n1", "raw", 4)]);
    record_digests(
        &conn,
        work_id,
        &[(copies.c001, "1".repeat(64)), (copies.c002, "2".repeat(64))],
    );

    assert_eq!(
        (notes_of(&conn, work_id, 1), notes_of(&conn, work_id, 2)),
        (
            vec![on("n1", "raw", 1, false), on("n2", "raw", 5, false)],
            vec![on("n1", "raw", 4, false)]
        ),
        "each account's n1 on its own copy's page"
    );
}

/// A work imported from a document listing `files`, three pages each, as its
/// `raw` members.
fn imported_with(
    conn: &Connection,
    dir: &std::path::Path,
    title: &str,
    files: &[&std::path::Path],
) -> i64 {
    let members: Vec<String> = files
        .iter()
        .map(|file| {
            format!(
                r#"{{"path":"{}","sha256":"{}","pages":3}}"#,
                file.display(),
                digest_of(file)
            )
        })
        .collect();
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"{title}","reading":"rtl"}},
            "editions":{{"raw":{{"label":"Raw","members":[{}]}}}},
            "hub":"raw","alignments":[]}}"#,
        members.join(",")
    );
    let doc = taiyaku_core::Sidecar::from_json(&json).expect("the fixture document parses");
    let sidecar = dir.join(format!("{title}.json"));
    std::fs::write(&sidecar, &json).expect("writes the fixture sidecar");
    crate::import::sidecar(conn, &doc, &sidecar).expect("imports")
}

/// Two imported documents can name one archive in two works: here B's second
/// member, b2, is a copy of A's first, a1. A stamp of A moves A's rows and
/// leaves B's on b2, whose digest has not moved. Both works' notes carry one
/// id, as a notes document imported against two works gives them (migration
/// 005), so a move that found its note by id alone would take B's too.
#[test]
fn a_stamp_leaves_another_works_rows_on_the_same_archive() {
    let dir = crate::tests::tempdir("stamp-two-works");
    let file = |name: &str| dir.path().join(name);
    let (a1, a2, b1, b2) = (
        file("a1.cbz"),
        file("a2.cbz"),
        file("b1.cbz"),
        file("b2.cbz"),
    );
    for each in [&a1, &a2, &b1] {
        crate::scan::tests::write_archive(each, 3);
    }
    std::fs::copy(&a1, &b2).expect("copies a1");
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let work_a = imported_with(&conn, dir.path(), "A", &[&a1, &a2]);
    let work_b = imported_with(&conn, dir.path(), "B", &[&b1, &b2]);
    let shared = digest_of(&a1);
    assert_eq!(
        (
            member_digests(&conn, work_a)[0].1.as_str(),
            member_digests(&conn, work_b)[1].1.as_str()
        ),
        (shared.as_str(), shared.as_str()),
        "the fixture: a1 and b2 are one archive"
    );
    // b2's second page is B's slot 4 and its third slot 5; a1's second is A's slot 1.
    put_position(&conn, work_b, "raw", 4);
    hold_notes(&conn, work_b, vec![a_note("n", "raw", 5)]);
    put_position(&conn, work_a, "raw", 1);
    hold_notes(&conn, work_a, vec![a_note("n", "raw", 1)]);

    apply(&conn, work_a, 1, &plan(&conn, work_a).expect("plans")).expect("stamps A");

    assert_eq!(
        notes_on(&conn, work_b),
        vec![on("n", "raw", 5, false)],
        "B's note stays on b2's page"
    );
    assert_eq!(
        position_anchors(&conn, work_b),
        vec![("raw".to_string(), shared.clone())],
        "B's position keeps b2's digest"
    );
    assert_eq!(
        notes_on(&conn, work_a),
        vec![on("n", "raw", 1, false)],
        "A's note follows a1's stamp"
    );
}

/// B's note n is on b2, a copy of a1, and A's note n is on a2, whose digest
/// the stamp leaves alone. With only a1 recorded stamped, the one move is
/// a1's pair: B's n names it and A's does not, so a read of every work's notes
/// would move A's n, which shares the id, onto a1's page.
#[test]
fn a_stamp_moves_no_note_by_another_works_anchor() {
    let dir = crate::tests::tempdir("stamp-other-works-note");
    let file = |name: &str| dir.path().join(name);
    let (a1, a2, b1, b2) = (
        file("a1.cbz"),
        file("a2.cbz"),
        file("b1.cbz"),
        file("b2.cbz"),
    );
    for each in [&a1, &a2, &b1] {
        crate::scan::tests::write_archive(each, 3);
    }
    std::fs::copy(&a1, &b2).expect("copies a1");
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let work_a = imported_with(&conn, dir.path(), "A", &[&a1, &a2]);
    let work_b = imported_with(&conn, dir.path(), "B", &[&b1, &b2]);
    // Slot 4 is a2's second page in A and b2's in B.
    hold_notes(&conn, work_a, vec![a_note("n", "raw", 4)]);
    hold_notes(&conn, work_b, vec![a_note("n", "raw", 4)]);
    let a1_member: i64 = conn
        .query_row(
            "SELECT m.id FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE e.work_id = ?1 ORDER BY m.ordinal LIMIT 1",
            [work_a],
            |row| row.get(0),
        )
        .expect("a1's member");

    record_digests(&conn, work_a, &[(a1_member, "1".repeat(64))]);

    assert_eq!(
        (notes_on(&conn, work_a), notes_on(&conn, work_b)),
        (
            vec![on("n", "raw", 4, false)],
            vec![on("n", "raw", 4, false)]
        ),
        "A's note stays on a2's page"
    );
}

/// The same for positions: A's is on a2's second page and B's on b2's, and
/// only a1 is recorded stamped. A read of every work's positions would move
/// A's onto a1's page by B's anchor.
#[test]
fn a_stamp_moves_no_position_by_another_works_anchor() {
    let dir = crate::tests::tempdir("stamp-other-works-position");
    let file = |name: &str| dir.path().join(name);
    let (a1, a2, b1, b2) = (
        file("a1.cbz"),
        file("a2.cbz"),
        file("b1.cbz"),
        file("b2.cbz"),
    );
    for each in [&a1, &a2, &b1] {
        crate::scan::tests::write_archive(each, 3);
    }
    std::fs::copy(&a1, &b2).expect("copies a1");
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let work_a = imported_with(&conn, dir.path(), "A", &[&a1, &a2]);
    let work_b = imported_with(&conn, dir.path(), "B", &[&b1, &b2]);
    put_position(&conn, work_a, "raw", 4);
    put_position(&conn, work_b, "raw", 4);
    let a1_member: i64 = conn
        .query_row(
            "SELECT m.id FROM member m JOIN edition e ON e.id = m.edition_id
              WHERE e.work_id = ?1 ORDER BY m.ordinal LIMIT 1",
            [work_a],
            |row| row.get(0),
        )
        .expect("a1's member");

    record_digests(&conn, work_a, &[(a1_member, "1".repeat(64))]);

    assert_eq!(
        (positions_on(&conn, work_a), positions_on(&conn, work_b)),
        (vec![("raw".to_string(), 4)], vec![("raw".to_string(), 4)]),
        "A's position stays on a2's page"
    );
}

/// One work whose `en` and `raw` editions each list a copy of one archive. The
/// stamp writes each a document of its own edition, so the copies take two
/// digests, and each edition's rows go to its own copy's.
#[test]
fn a_stamp_moves_each_editions_rows_to_its_own_copy() {
    let dir = crate::tests::tempdir("stamp-two-editions");
    let raw = dir.path().join("raw-v01.cbz");
    let en = dir.path().join("en-v01.cbz");
    crate::scan::tests::write_archive(&raw, 3);
    std::fs::copy(&raw, &en).expect("copies raw");
    let shared = digest_of(&raw);
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"Two","reading":"rtl"}},
            "editions":{{"raw":{{"label":"Raw","members":[
              {{"path":"{}","sha256":"{shared}","pages":3}}]}},
              "en":{{"label":"EN","members":[
              {{"path":"{}","sha256":"{shared}","pages":3}}]}}}},
            "hub":"raw","alignments":[]}}"#,
        raw.display(),
        en.display()
    );
    let doc = taiyaku_core::Sidecar::from_json(&json).expect("the fixture document parses");
    let sidecar = dir.path().join("taiyaku.json");
    std::fs::write(&sidecar, &json).expect("writes the fixture sidecar");
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let work_id = crate::import::sidecar(&conn, &doc, &sidecar).expect("imports");
    put_position(&conn, work_id, "raw", 1);
    put_position(&conn, work_id, "en", 2);
    hold_notes(
        &conn,
        work_id,
        vec![a_note("e", "en", 2), a_note("r", "raw", 1)],
    );

    apply(&conn, work_id, 1, &plan(&conn, work_id).expect("plans")).expect("stamps");
    let digests = member_digests(&conn, work_id);
    assert_ne!(
        digests[0].1, digests[1].1,
        "the fixture: the stamp gave the two copies two digests: {digests:?}"
    );

    assert_eq!(
        notes_on(&conn, work_id),
        vec![on("e", "en", 2, false), on("r", "raw", 1, false)],
        "each edition's note on its own copy's page"
    );
    assert_eq!(
        position_anchors(&conn, work_id),
        digests,
        "each edition's position on its own copy's digest"
    );
}

/// Works 1 and 2 and nothing else: the aliases' foreign key needs the works.
fn two_works() -> crate::Library {
    let library = crate::Library::open_in_memory().expect("an in-memory library opens");
    library
        .conn()
        .execute_batch(
            "INSERT INTO work (id, content_key, slug, title, reading, hub)
             VALUES (1, 'k1', 's1', 'One', 'rtl', 'raw'), (2, 'k2', 's2', 'Two', 'rtl', 'raw');",
        )
        .expect("seeds two works");
    library
}

type Pair<'a> = (&'a str, u32);

/// `pairs` as [`rekey_anchors`] hands them over, before and after.
fn moves(pairs: &[(Pair<'_>, Pair<'_>)]) -> BTreeMap<(String, u32), (String, u32)> {
    pairs
        .iter()
        .map(|((was, was_nth), (now, now_nth))| {
            (
                ((*was).to_string(), *was_nth),
                ((*now).to_string(), *now_nth),
            )
        })
        .collect()
}

/// One stamp's moves on work 1's `raw`.
fn remember_on_raw(conn: &Connection, pairs: &[(Pair<'_>, Pair<'_>)]) {
    remember_moves(conn, 1, "raw", &moves(pairs)).expect("remembers the moves");
}

/// What work 1's `raw` forwards `(sha256, nth)` to.
fn raw_alias(conn: &Connection, (sha256, nth): Pair<'_>) -> Option<(String, u32)> {
    alias_of(conn, 1, "raw", sha256, nth).expect("reads the alias")
}

fn to((sha256, nth): Pair<'_>) -> Option<(String, u32)> {
    Some((sha256.to_string(), nth))
}

/// A second stamp of a stamped archive forwards the first stamp's digest to
/// the newest as well, so one lookup reaches the member as it is now.
#[test]
fn a_second_stamp_forwards_the_first_ones_digest_to_the_newest() {
    let library = two_works();
    let conn = library.conn();
    remember_on_raw(&conn, &[(("A", 0), ("B", 0))]);
    remember_on_raw(&conn, &[(("B", 0), ("C", 0))]);

    assert_eq!(
        raw_alias(&conn, ("A", 0)),
        to(("C", 0)),
        "the first stamp's digest names the newest"
    );
    assert_eq!(
        raw_alias(&conn, ("B", 0)),
        to(("C", 0)),
        "the second stamp's digest names it too"
    );
}

/// A stamp that gives an archive back a digest it had leaves no alias from
/// that digest: a member holds it again, so it names nothing to forward.
#[test]
fn a_digest_a_stamp_gives_back_is_no_alias() {
    let library = two_works();
    let conn = library.conn();
    remember_on_raw(&conn, &[(("A", 0), ("B", 0))]);
    remember_on_raw(&conn, &[(("B", 0), ("A", 0))]);

    assert_eq!(raw_alias(&conn, ("A", 0)), None, "no alias to itself");
    assert_eq!(
        raw_alias(&conn, ("B", 0)),
        to(("A", 0)),
        "the digest it left names the one it has"
    );
}

/// A copy the stamp does not rewrite moves its occurrence when a copy before
/// it leaves the digest. Each move is remembered as it is: the pair one copy
/// leaves is the pair the other takes, and is not forwarded again.
#[test]
fn an_occurrence_that_moves_without_its_digest_is_remembered_beside_the_digest() {
    let library = two_works();
    let conn = library.conn();
    remember_on_raw(&conn, &[(("X", 0), ("Y", 0)), (("X", 1), ("X", 0))]);

    assert_eq!(
        raw_alias(&conn, ("X", 0)),
        to(("Y", 0)),
        "the rewritten copy"
    );
    assert_eq!(
        raw_alias(&conn, ("X", 1)),
        to(("X", 0)),
        "the copy after it, now the digest's first"
    );
}

/// An alias is one work's and one edition's: a digest can stand in two works,
/// and an occurrence counts one edition's members. A stamp on another work or
/// edition neither reads nor replaces it.
#[test]
fn an_alias_is_its_own_works_and_editions_alone() {
    let library = two_works();
    let conn = library.conn();
    remember_on_raw(&conn, &[(("A", 0), ("B", 0))]);

    assert_eq!(
        alias_of(&conn, 2, "raw", "A", 0),
        Ok(None),
        "work 2's raw has no alias"
    );
    assert_eq!(
        alias_of(&conn, 1, "en", "A", 0),
        Ok(None),
        "work 1's en has no alias"
    );

    let elsewhere = moves(&[(("C", 0), ("D", 0))]);
    remember_moves(&conn, 2, "raw", &elsewhere).expect("remembers work 2's stamp");
    remember_moves(&conn, 1, "en", &elsewhere).expect("remembers work 1's en stamp");
    assert_eq!(
        (
            raw_alias(&conn, ("A", 0)),
            alias_of(&conn, 2, "raw", "A", 0),
            alias_of(&conn, 1, "en", "A", 0),
        ),
        (to(("B", 0)), Ok(None), Ok(None)),
        "work 1's raw keeps its alias, and lends it to neither"
    );
}

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
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(path).expect("reading the archive"));
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
    let sidecar = dir.path().join("taiyaku.json");
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
        after.contains(&applied.members[0].1),
        "and it names the digest the archive now has: {after}"
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
        "and it is the same document, not a fresh one"
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

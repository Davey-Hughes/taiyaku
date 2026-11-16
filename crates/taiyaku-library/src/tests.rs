use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::*;

/// A directory unique to one test, removed when the test ends: O117's shared
/// [`TempDir`], under this crate's own `lib-` label.
///
/// `pub(crate)` for `schema/tests.rs`, which needs two connections onto one
/// real file to reproduce two processes migrating together.
pub(crate) use taiyaku_read::test_support::TempDir;

pub(crate) fn tempdir(label: &str) -> TempDir {
    TempDir::new(&format!("lib-{label}"))
}

/// Spec §1's copy: `Foo/Foo v01.cbz` under `a` (library `A`, id 70) and the
/// same bytes under `b` (library `B`, id 80), derived into one work of one
/// member with a `member_root` row for each. The library ids are apart from
/// the directories', so a grant reading one as the other sees nothing.
///
/// `b` is added first, so it holds the primary copy: a grant of `A` reaches the
/// member only through its `member_root` row, and removing `b` must re-point
/// it at `a`'s copy.
pub(crate) struct CopiedVolume {
    pub(crate) library: Library,
    pub(crate) a: crate::scan::Root,
    pub(crate) b: crate::scan::Root,
    _dirs: [TempDir; 2],
}

impl CopiedVolume {
    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.library.conn()
    }
}

/// `Foo/Foo v01.cbz` under `first`, and the same bytes copied to the same
/// place under `second`.
pub(crate) fn a_copy_of_one_volume() -> (TempDir, TempDir) {
    a_copy_of("Foo v01.cbz")
}

/// `Foo/<name>` under `first`, and the same bytes copied to the same place
/// under `second`.
pub(crate) fn a_copy_of(name: &str) -> (TempDir, TempDir) {
    let first = tempdir("copy-first");
    let archive = first.path().join("Foo").join(name);
    crate::scan::tests::write_archive(&archive, 3);
    let second = tempdir("copy-second");
    std::fs::create_dir_all(second.path().join("Foo")).expect("mkdir");
    std::fs::copy(&archive, second.path().join("Foo").join(name)).expect("copies");
    (first, second)
}

/// Every `member_root` row's directory, in id order.
pub(crate) fn member_root_ids(conn: &Connection) -> Vec<i64> {
    let mut stmt = conn
        .prepare("SELECT root_id FROM member_root ORDER BY root_id")
        .expect("prepare");
    stmt.query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows")
}

/// The one member's `(root_id, path)`.
pub(crate) fn the_member(conn: &Connection) -> (Option<i64>, String) {
    conn.query_row("SELECT root_id, path FROM member", [], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })
    .expect("the one member")
}

pub(crate) fn a_volume_copied_into_two_libraries() -> CopiedVolume {
    let (b, a) = a_copy_of_one_volume();
    let library = Library::open_in_memory().expect("open");
    let (ra, rb) = {
        let conn = library.conn();
        conn.execute(
            "INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B')",
            [],
        )
        .expect("two libraries, numbered apart from their directories");
        let rb = crate::scan::add_root_in(&conn, b.path(), crate::scan::InLibrary::Named("B"))
            .expect("adds b");
        let ra = crate::scan::add_root_in(&conn, a.path(), crate::scan::InLibrary::Named("A"))
            .expect("adds a");
        crate::scan::scan_root(&conn, &rb).expect("scans b");
        crate::scan::scan_root(&conn, &ra).expect("scans a");
        crate::derive::derive(&conn).expect("derives");
        (ra, rb)
    };
    let fixture = CopiedVolume {
        library,
        a: ra,
        b: rb,
        _dirs: [a, b],
    };
    {
        let conn = fixture.conn();
        let mut both = vec![fixture.a.id, fixture.b.id];
        both.sort_unstable();
        assert_eq!(
            (the_member(&conn), member_root_ids(&conn)),
            (
                (
                    Some(fixture.b.id),
                    fixture
                        .b
                        .path
                        .join("Foo")
                        .join("Foo v01.cbz")
                        .display()
                        .to_string()
                ),
                both
            ),
            "the fixture: one member, its primary copy b's, held under both"
        );
        // `the_member` reads the first row only, and `member_root_ids` every
        // member's rows, so neither alone tells one member from two.
        assert_eq!(
            conn.query_row("SELECT count(*) FROM member", [], |r| r.get::<_, i64>(0))
                .expect("counts"),
            1,
            "the fixture: one member, not one per copy"
        );
    }
    fixture
}

/// A library on disk, holding one derived work with a hand-set title recorded
/// under `<root>/.taiyaku/` — one member whose `content_key` a repack can
/// move, and a document a `Documents::Skip` derivation must leave alone.
///
/// `pub(crate)` for `derive/tests.rs`, which is where `Documents` is tested:
/// a second fixture describing the same shape is how two tests come to
/// disagree about what the library looks like.
pub(crate) struct FixtureLibrary {
    library: Library,
    dir: TempDir,
}

impl FixtureLibrary {
    /// The connection, exclusively — the same guard [`Library::conn`]
    /// returns, so a caller holds it exactly as it would hold a real one.
    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.library.conn()
    }

    pub(crate) fn root_path(&self) -> &Path {
        self.dir.path()
    }
}

/// Build [`FixtureLibrary`]: a root holding one `.cbz`
/// (`Series/Series - v01.cbz`), scanned and derived, opted into
/// `keeps_documents`, given a hand-set title through [`crate::hand::save`],
/// and derived a second time so the document a real derivation writes is
/// already sitting on disk — settled, rather than mid-write — before a test
/// takes its own snapshot of the tree.
pub(crate) fn library_with_a_kept_document() -> FixtureLibrary {
    let dir = tempdir("derive-documents-policy");
    crate::scan::tests::write_archive(&dir.path().join("Series").join("Series - v01.cbz"), 3);

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("the first scan");
        crate::derive::derive(&conn).expect("the first derive");
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("opting the root into hand documents");

        let work_id: i64 = conn
            .query_row("SELECT id FROM work", [], |r| r.get(0))
            .expect("the one work the fixture archive derived");
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting the work");
        doc.title = Some("Hand-Set Title".to_string());
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applying the hand edit");
        crate::hand::save(&conn, work_id).expect("saving the hand document");

        // Settles the document on disk and arms the `migrated_document`
        // marker through the read-back, matching what a library that had
        // already been rescanned once after the edit looks like — the state
        // a documents-policy test wants to snapshot as `before`, not the
        // moment `save` itself just wrote.
        crate::derive::derive(&conn).expect("the second derive");
    }

    FixtureLibrary { library, dir }
}

/// Rewrite the file named `name`, found anywhere under `fixture`'s root, with
/// one more page than it currently holds.
///
/// **Why a page count, not a byte appended.** [`crate::scan::tests::write_archive`]
/// names every page after the archive's own path and its index, so two calls
/// at the same page count would write byte-identical archives and move
/// nothing; changing the count is what moves `content_key`, which is the
/// point — this is `derive/tests.rs`'s "repack an archive so a rekey is what
/// the next derivation sees," reused rather than a second zip writer.
pub(crate) fn add_a_page(fixture: &FixtureLibrary, name: &str) {
    let path = find_by_name(fixture.root_path(), name)
        .unwrap_or_else(|| panic!("no file named {name} under {:?}", fixture.root_path()));
    let pages = taiyaku_read::member_pages(&path).expect("the archive lists its pages");
    crate::scan::tests::write_archive(&path, pages as usize + 1);
}

/// A library on disk holding three roots, each contributing one series of one
/// volume — for `crate::preview::tests`, which proposes a rule wide enough to
/// merge every series into one and needs the merge to actually move
/// something, plus one work the same rule must leave exactly where it is.
///
/// **Three roots, despite the name.** `Alpha` (root C, below) is the work that
/// holds still: its series already reads `Alpha` under the seeded rules — the
/// same string the widening rule in `crate::tests::a_rule_setting_series` sends
/// everything to in every test that uses this fixture — so nothing changes for
/// it, before and after. `works_rehomed` must count `Beta` and `Gamma` and must
/// not count `Alpha`.
///
/// **Three placeholder roots are added, and never scanned, before any real
/// one.** `root.id` and `work.id` are both plain `INTEGER PRIMARY KEY` columns
/// that SQLite mints upward from 1 in each table independently, so adding
/// exactly the three roots this fixture needs would number them 1, 2 and 3, the
/// ids its three works hold (`docs/status.md`, spec C1's grant tests). The
/// three placeholders cost three harmless rows in `root`, never scanned and
/// never referenced by anything a test asserts on, and they consume ids 1
/// through 3 — so the three roots this fixture's tests actually hold (and could
/// pass to a rule's `root_id`) land on 4, 5 and 6, while the three works this
/// derive writes land on 1, 2 and 3. Every id a test in this fixture's family
/// could reach for is provably distinct from its counterpart in the other
/// table, not coincidentally so.
///
/// **`series.id` is not covered by that same argument, and does coincide with
/// `work.id` here — 1:1, `Alpha`'s series and work both landing on 1,
/// `Beta`'s both on 2, `Gamma`'s both on 3.** No placeholder shifts `series`
/// the way three shift `root`, because nothing in this fixture inserts a
/// `series` row directly; `derive` mints one per distinct series it finds, in
/// title order, right after `work` has minted its own ids the same way in the
/// same pass. `preview::grouping`'s join from `work` to `series` is therefore
/// exercised by this fixture without ever being *protected* by id disjointness
/// the way the root/work join above is.
pub(crate) fn library_with_two_roots() -> FixtureLibrary {
    let dir = tempdir("preview-two-roots");
    std::fs::create_dir_all(dir.path().join("_unused_a")).expect("the first placeholder directory");
    std::fs::create_dir_all(dir.path().join("_unused_b"))
        .expect("the second placeholder directory");
    std::fs::create_dir_all(dir.path().join("_unused_c")).expect("the third placeholder directory");

    let root_a_dir = dir.path().join("root-a");
    crate::scan::tests::write_archive(&root_a_dir.join("Beta").join("Beta - v01.cbz"), 3);
    let root_b_dir = dir.path().join("root-b");
    // **`v02`, not `v01`, and `root_c`'s `Alpha` below is `v03`.** Every
    // series sets the same `series` via the widening `.*` rule the preview
    // tests append, and `derive`'s own `GroupKey` is `(series, volume, ..)`
    // — two volume-1 archives under one series would land in the *same*
    // group, concatenating two works' pages into one member rather than
    // rehoming (or holding still) separate works. Distinct volumes keep them
    // distinct works under the merged series, which is what `works_rehomed`
    // actually means to count.
    crate::scan::tests::write_archive(&root_b_dir.join("Gamma").join("Gamma - v02.cbz"), 4);
    let root_c_dir = dir.path().join("root-c");
    // **`Alpha`, not a third arbitrary name.** `crate::tests::a_rule_setting_series`
    // is always called with `"Alpha"` in this fixture's own tests — the
    // widest possible edit, applied last — so a work whose series already
    // reads `Alpha` under the seeded rules is a work the widening rule sends
    // nowhere: its placement's `series` field is `"Alpha"` both before the
    // rule is proposed and after, which is exactly what `works_rehomed`
    // must not count. `v03`, per the comment above.
    crate::scan::tests::write_archive(&root_c_dir.join("Alpha").join("Alpha - v03.cbz"), 2);

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        crate::scan::add_root(&conn, &dir.path().join("_unused_a"))
            .expect("adding the first placeholder root");
        crate::scan::add_root(&conn, &dir.path().join("_unused_b"))
            .expect("adding the second placeholder root");
        crate::scan::add_root(&conn, &dir.path().join("_unused_c"))
            .expect("adding the third placeholder root");

        // **The three real roots are directories of one library.** The widening
        // rule this fixture's tests append merges their series into one, and
        // a series is one library's: three libraries would each keep a series
        // called `Alpha`. The placeholders stay libraries of their own, which
        // costs nothing, since nothing is scanned there.
        let one_library = crate::scan::InLibrary::Named("Preview");
        let root_a =
            crate::scan::add_root_in(&conn, &root_a_dir, one_library).expect("adding root a");
        crate::scan::scan_root(&conn, &root_a).expect("scanning root a");
        let root_b =
            crate::scan::add_root_in(&conn, &root_b_dir, one_library).expect("adding root b");
        crate::scan::scan_root(&conn, &root_b).expect("scanning root b");
        let root_c =
            crate::scan::add_root_in(&conn, &root_c_dir, one_library).expect("adding root c");
        crate::scan::scan_root(&conn, &root_c).expect("scanning root c");

        // `preview::rows_for`'s provenance test needs a root whose own
        // `edition_key` column actually decides something — the one case
        // `rules::ROOT_FALLBACK` exists for, and the one a mapping bug
        // (`Decided::Rule(0)` where `Decided::RootDefault` belongs) cannot
        // fake by accident, since no rule in this fixture's seeded four ever
        // touches `edition_key` at all. All three roots get it, not just
        // whichever one a row-sort tie happens to favour — `rows_for` sorts
        // ties on `rel_path` by `root_id`, and pinning that ordering here
        // would make this fixture's contract with that test do double duty.
        //
        // `"raw"`, not some other value: it is `derive.rs`'s own
        // `DEFAULT_EDITION_KEY`, the string every one of these entries
        // already resolves to when nothing sets `edition_key` at all. Setting
        // it here changes *how* that string gets decided — the root's own
        // column, through `rules::Engine`, rather than `derive_with`'s later
        // fallback — without changing what it resolves to, so an assertion in
        // this fixture's family that reads only `series`, `work` and proposal
        // counts reads the same either way.
        conn.execute(
            "UPDATE root SET edition_key = 'raw' WHERE id IN (?1, ?2, ?3)",
            rusqlite::params![root_a.id, root_b.id, root_c.id],
        )
        .expect("giving every root its own edition_key default");

        crate::derive::derive(&conn).expect("deriving the three-root fixture");
    }
    FixtureLibrary { library, dir }
}

/// A library on disk holding two roots, each contributing one distinct,
/// non-overlapping series, with a reading position and a note recorded
/// against the second root's own work — for `crate::scan::tests`'s removal
/// tests, which need a root whose removal costs something a reader would
/// actually notice going.
///
/// **Two roots, no placeholders.** `library_with_two_roots`'s own three
/// placeholder roots exist to keep a root id and a work id from coinciding,
/// for a mutation neither of this fixture's own tests goes looking for —
/// nothing here joins a root id against a work id, so nothing here needs the
/// same insurance.
///
/// **The reading position and note are looked up by which root's `member`
/// rows they sit under, not by title.** Reaching for `work.title = 'Beta'`
/// would tie this fixture to what the seeded rules happen to name the
/// series, which is exactly the kind of fact `crate::scan::tests` should not
/// need to know to seed a reading position on "whichever work root b
/// derived."
pub(crate) fn library_with_two_roots_and_a_reading_position() -> FixtureLibrary {
    let dir = tempdir("scan-removal-two-roots");
    let root_a_dir = dir.path().join("root-a");
    crate::scan::tests::write_archive(&root_a_dir.join("Alpha").join("Alpha - v01.cbz"), 3);
    let root_b_dir = dir.path().join("root-b");
    crate::scan::tests::write_archive(&root_b_dir.join("Beta").join("Beta - v01.cbz"), 4);

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root_a = crate::scan::add_root(&conn, &root_a_dir).expect("adding root a");
        crate::scan::scan_root(&conn, &root_a).expect("scanning root a");
        let root_b = crate::scan::add_root(&conn, &root_b_dir).expect("adding root b");
        crate::scan::scan_root(&conn, &root_b).expect("scanning root b");
        crate::derive::derive(&conn).expect("deriving the two-root fixture");

        let beta_work: i64 = conn
            .query_row(
                "SELECT DISTINCT e.work_id
                   FROM member m
                   JOIN edition e ON e.id = m.edition_id
                  WHERE m.root_id = ?1",
                [root_b.id],
                |r| r.get(0),
            )
            .expect("root b derived exactly one work of its own");
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 4)",
            [beta_work],
        )
        .expect("seeding a reading position on root b's own work");
        conn.execute(
            "INSERT INTO note (id, user_id, work_id, edition, slot, body, created, updated)
             VALUES ('note-1', 1, ?1, 'raw', 1, 'a note worth losing', datetime('now'), \
             datetime('now'))",
            [beta_work],
        )
        .expect("seeding a note on root b's own work");
    }
    FixtureLibrary { library, dir }
}

/// One volume whose two chapters stand in two directories of one library, so
/// removing the first directory moves the second chapter's pages down three
/// slots, and removing the second takes its pages out of the edition (O244).
/// See [`a_volume_across_two_directories`].
pub(crate) struct VolumeAcrossTwoDirectories {
    pub(crate) library: Library,
    /// `Foo/Foo - v01 - c001.cbz`, 3 pages.
    pub(crate) a: crate::scan::Root,
    /// `Foo/Foo - v01 - c002.cbz`, 4 pages.
    pub(crate) b: crate::scan::Root,
    pub(crate) work_id: i64,
    _dir: TempDir,
}

impl VolumeAcrossTwoDirectories {
    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        self.library.conn()
    }

    /// Put a directory back after `scan::remove_root` took it: added to the
    /// same library again, scanned and derived. Its files are still on disk,
    /// so the member comes back with the digest it had.
    pub(crate) fn put_back(&self, root: &crate::scan::Root) -> crate::scan::Root {
        let conn = self.conn();
        let again = crate::scan::add_root_in(
            &conn,
            &root.path,
            crate::scan::InLibrary::Named(VOLUME_LIBRARY),
        )
        .expect("adds the directory back");
        crate::scan::scan_root(&conn, &again).expect("scans it again");
        crate::derive::derive(&conn).expect("derives with it back");
        again
    }
}

const VOLUME_LIBRARY: &str = "Manga";

/// Directory A holds `Foo/Foo - v01 - c001.cbz` (3 pages) and directory B
/// `Foo/Foo - v01 - c002.cbz` (4 pages), both in one library, scanned and
/// derived into one work whose `raw` edition is c001 then c002: slots 0-2 are
/// c001's and 3-6 c002's. Neither archive has a spread flag, so a slot is a page.
pub(crate) fn a_volume_across_two_directories() -> VolumeAcrossTwoDirectories {
    let dir = tempdir("volume-across-two-directories");
    let a_dir = dir.path().join("a");
    let b_dir = dir.path().join("b");
    crate::scan::tests::write_archive(&a_dir.join("Foo").join("Foo - v01 - c001.cbz"), 3);
    crate::scan::tests::write_archive(&b_dir.join("Foo").join("Foo - v01 - c002.cbz"), 4);

    let library = Library::open_in_memory().expect("an in-memory library opens");
    let (a, b, work_id) = {
        let conn = library.conn();
        let one_library = crate::scan::InLibrary::Named(VOLUME_LIBRARY);
        let a = crate::scan::add_root_in(&conn, &a_dir, one_library).expect("adds a");
        crate::scan::scan_root(&conn, &a).expect("scans a");
        let b = crate::scan::add_root_in(&conn, &b_dir, one_library).expect("adds b");
        crate::scan::scan_root(&conn, &b).expect("scans b");
        crate::derive::derive(&conn).expect("derives the volume");

        let members: Vec<(i64, String, String, u32)> = conn
            .prepare(
                "SELECT e.work_id, e.key, m.path, m.pages FROM member m
                   JOIN edition e ON e.id = m.edition_id
                  ORDER BY e.work_id, e.key, m.ordinal",
            )
            .and_then(|mut stmt| {
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect()
            })
            .expect("lists the members");
        let work_id = members.first().expect("the volume derived a work").0;
        let shape: Vec<(i64, &str, String, u32)> = members
            .iter()
            .map(|(work, key, path, pages)| {
                let file = Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a member names its file");
                (*work, key.as_str(), file.to_string(), *pages)
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (work_id, "raw", "Foo - v01 - c001.cbz".to_string(), 3),
                (work_id, "raw", "Foo - v01 - c002.cbz".to_string(), 4),
            ],
            "the fixture: one work, its `raw` edition c001 then c002"
        );
        (a, b, work_id)
    };
    VolumeAcrossTwoDirectories {
        library,
        a,
        b,
        work_id,
        _dir: dir,
    }
}

/// A rule that sends every path to `series`, regardless of what it already
/// resolves to — the widest possible edit, and the one
/// `crate::preview::tests` uses to prove a preview's [`crate::preview::Impact`]
/// matches what saving the same rule actually does.
///
/// A large finite ordinal — not "one past whatever is already loaded":
/// this function takes only `series`, so it cannot see the caller's rule
/// list to compute the next ordinal from, and every caller in this crate
/// wants the same thing from this rule regardless — that it is the layering
/// model's last word, applied after every seeded or accepted rule the
/// fixture already holds. A fixed sentinel above any ordinal this crate ever
/// assigns gets there without needing the list.
///
/// **Not `i64::MAX`.** A rule persisted at `i64::MAX` through
/// [`crate::rules::replace_all`] would sit in the `rule` table where
/// [`crate::proposals::accept_with`]'s `COALESCE(MAX(ordinal), 4) + 1`
/// reaches it and overflows. `1_000_000_000` is far above any ordinal this
/// crate ever assigns by hand (the seeded defaults are 1-4, an accepted rule
/// is one past whatever `MAX(ordinal)` was) while leaving `i64`'s range
/// untouched for every `+ 1` downstream code takes for granted.
const LAST_WORD_ORDINAL: i64 = 1_000_000_000;

pub(crate) fn a_rule_setting_series(series: &str) -> crate::rules::Rule {
    crate::rules::Rule {
        id: 0,
        ordinal: LAST_WORD_ORDINAL,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: Some(series.to_string()),
        note: None,
    }
}

/// A library on disk holding migration 3's own four seeded rules, plus the
/// exact pair C4 §1's own worked example describes: one rule setting
/// `edition_key = "raw"` for everything under `Manga (RAW)/`, and a later one
/// setting `edition_key = "en-color"` for anything ending "Full Color" — both
/// matching `Manga (RAW)/Go-Toubun no Hanayome - Full Color`, the later
/// winning, a Japanese colour edition reading as English. For
/// `crate::preview::tests`'s override-detection tests.
///
/// **The seeded four never touch `edition_key` at all** (migration 3 seeds
/// only series/volume/chapter rules), so the pair added here is the *only*
/// source of the override — nothing about `library_with_two_roots`' own
/// series-capturing rules (which two of the seeded four already are) is
/// entangled with it.
///
/// **A directory row, not the archive itself, is where the override lands.**
/// `scan_root` writes a `scan_entry` for every directory level, not only
/// leaf files — `Manga (RAW)/Go-Toubun no Hanayome - Full Color` is one such
/// row, and it is the only entry both new rules match: the raw-edition
/// rule's prefix matches every path under `Manga (RAW)/`, including this
/// directory's own `rel_path`, and the colour rule's regex matches any path
/// *ending* "Full Color", which this directory's name does and the archive
/// filename underneath it (`… - v01.cbz`) does not.
///
/// **Saved, not merely proposed** — through [`crate::rules::replace_all`]
/// directly on the shared connection, since `crate::preview::overrides`
/// reads `proposed` from `crate::rules::read`, which only ever sees what is
/// actually in the `rule` table.
pub(crate) fn library_with_the_seeded_defaults() -> FixtureLibrary {
    let dir = tempdir("preview-seeded-defaults");
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Manga (RAW)")
            .join("Go-Toubun no Hanayome - Full Color")
            .join("Go-Toubun no Hanayome - Full Color - v01.cbz"),
        3,
    );

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");

        let mut rules = crate::rules::read(&conn)
            .expect("the seeded defaults migration 3 installs")
            .0;
        rules.push(crate::rules::Rule {
            id: 0,
            ordinal: 5,
            enabled: true,
            root_id: None,
            kind: crate::rules::RuleKind::Prefix,
            pattern: "Manga (RAW)/".to_string(),
            match_on: crate::rules::MatchOn::Path,
            set_edition_key: Some("raw".to_string()),
            set_kind: None,
            set_language: None,
            set_series: None,
            note: Some("C4 §1: everything under Manga (RAW)/ is the raw edition".to_string()),
        });
        rules.push(crate::rules::Rule {
            id: 0,
            ordinal: 6,
            enabled: true,
            root_id: None,
            kind: crate::rules::RuleKind::Regex,
            pattern: "Full Color$".to_string(),
            match_on: crate::rules::MatchOn::Path,
            set_edition_key: Some("en-color".to_string()),
            set_kind: None,
            set_language: None,
            set_series: None,
            note: Some("C4 §1: anything ending \"Full Color\" is the colour edition".to_string()),
        });
        crate::rules::replace_all(&conn, &rules)
            .expect("saving the seeded defaults plus C4 §1's own disagreeing pair");
    }
    FixtureLibrary { library, dir }
}

/// A library shaped like the real one O95 describes: two series whose
/// **directory** carries an edition or group suffix that their **filenames**
/// do not — `Death Note (Colored Council)/Death Note v01.cbz` and
/// `Chainsaw Man - Digital Colored Comics/Chainsaw Man v01.cbz`.
///
/// **Nothing is added to the seeded four.** That is the point: migration 3's
/// rule 1 reads the series from the directory, its rule 4 reads it from the
/// filename "where it repeats it", and rule 4 wins on ordinal — the fold of a
/// colour edition into its main series that `docs/status.md`'s O95 defends.
///
/// **This shape is reconciled — `docs/status.md`, O89.** The directory
/// qualifies the filename's captured series — `Death Note` is a prefix of
/// `Death Note (Colored Council)`, and `Chainsaw Man` a prefix of
/// `Chainsaw Man - Digital Colored Comics` — so rule 1's own value stands and
/// rule 4's capture is not an override, which this fixture's test
/// `a_directory_that_qualifies_the_filenames_series_is_not_an_override` pins.
///
/// **The directory rows themselves contribute nothing**: rule 1's
/// `^(?<series>[^/]+)/` needs a separator a bare directory `rel_path` does not
/// have, and rule 4's `^(?<series>.+?)[ _-]+v` needs a `v` after a separator
/// that neither directory name carries.
pub(crate) fn library_with_colour_edition_directories() -> FixtureLibrary {
    let dir = tempdir("preview-colour-edition-directories");
    for (directory, file) in [
        ("Death Note (Colored Council)", "Death Note v01.cbz"),
        (
            "Chainsaw Man - Digital Colored Comics",
            "Chainsaw Man v01.cbz",
        ),
    ] {
        crate::scan::tests::write_archive(&dir.path().join(directory).join(file), 3);
    }

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");
    }
    FixtureLibrary { library, dir }
}

/// A library on disk holding two more real shapes from O95 — this time ones
/// O89's reconciliation leaves standing: a directory whose *filename*
/// abbreviates its series rather than *qualifying* it, so what the filename
/// captures is never a prefix of the directory's own value and the
/// reconciliation never fires.
///
/// **These are real shapes.** The real filenames also carry a year,
/// `(2017)`/`(2022)`. It is omitted here so that nothing but the series
/// pairing can decide a field.
///
/// **The directory does not *qualify* the capture**, because
/// `GTO - Paradise Lost` is not a prefix of
/// `GTO - Great Teacher Onizuka - Paradise Lost`. So O89's reconciliation
/// leaves rule 4 winning and the override standing.
///
/// **Two entries, with different values either side**, so that a row carrying
/// its own example's values can be told apart from a row carrying whichever
/// entry happened to be walked last. Neither directory name has a `[ _-]+v`,
/// so the directory rows reach neither rule.
pub(crate) fn library_with_directories_their_filenames_abbreviate() -> FixtureLibrary {
    let dir = tempdir("preview-directories-abbreviate");
    for (directory, file) in [
        (
            "GTO - Great Teacher Onizuka - 14 Days in Shonan",
            "GTO - 14 Days in Shonan - v01.cbz",
        ),
        (
            "GTO - Great Teacher Onizuka - Paradise Lost",
            "GTO - Paradise Lost - v01.cbz",
        ),
    ] {
        crate::scan::tests::write_archive(&dir.path().join(directory).join(file), 3);
    }

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");
    }
    FixtureLibrary { library, dir }
}

/// A library whose series name begins with a run of letters that
/// `str::parse::<f64>()` reads as **not a number** — `Nanatsu no Taizai`,
/// whose first three characters are `Nan`.
///
/// **Not contrived, and that is the point.** `rules::apply_captures` parses a
/// `volume` or `chapter` capture with a bare `str::parse::<f64>()`, which
/// accepts `nan`, `inf`, `infinity` and overflowing literals — so a capture
/// group aimed at a filename and slightly too wide really does produce one,
/// off a real series title, without anybody writing `nan` on purpose.
///
/// It exists for `preview::FieldValue::agrees_with`: two rules that both
/// capture `Nan` produce two `NaN`s, and `NaN != NaN`, so a *derived*
/// comparison reports them as disagreeing with each other when they read the
/// same three characters off the same file.
pub(crate) fn library_with_a_name_that_parses_as_not_a_number() -> FixtureLibrary {
    let dir = tempdir("preview-not-a-number");
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Nanatsu no Taizai")
            .join("Nanatsu no Taizai v01.cbz"),
        3,
    );

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");
    }
    FixtureLibrary { library, dir }
}

/// A library on disk holding two candidate colour editions of one series —
/// one whose directory name a colour-edition regex actually matches, and one
/// that reads as a colour edition to a person but does not — for
/// `crate::preview::tests`'s near-miss test.
///
/// **The near-miss is the fixture's whole point, not an afterthought.** C4
/// §1: "a regex's *failure* is the thing you cannot see by reading it," with
/// this exact pair as its own worked example —
/// `Yu Yu Hakusho - Colored (Colored Council)` failing to match a
/// colour-edition regex anchored on `"- Full Color"`. Neither entry's
/// `edition_key` is decided by anything else: none of the four seeded
/// default rules touch `edition_key` at all, and this fixture's root carries
/// no `edition_key` default of its own — so the miss is total, undecided
/// rather than merely outvoted, which is what makes it the near-miss
/// [`a_colour_edition_rule`]'s own field ever reaches.
pub(crate) fn library_with_two_colour_editions() -> FixtureLibrary {
    let dir = tempdir("preview-two-colour-editions");
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Yu Yu Hakusho - Full Color")
            .join("Yu Yu Hakusho - Full Color - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Yu Yu Hakusho - Colored (Colored Council)")
            .join("Yu Yu Hakusho - Colored (Colored Council) - v01.cbz"),
        3,
    );

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");
        crate::derive::derive(&conn).expect("deriving the colour-edition fixture");
    }
    FixtureLibrary { library, dir }
}

/// A rule that sets `edition_key` wherever `"- Full Color"` appears in the
/// path — matching [`library_with_two_colour_editions`]'s first pair and
/// missing its second, the one C4 §1's own example is built from.
///
/// `id: 0`, deliberately never saved through [`crate::rules::replace_all`]:
/// this is the candidate a person is still typing into C4's editor, and
/// `crate::preview::tests` previews it exactly as proposed, unsaved id and
/// all — the same "id 0 means not written yet" convention
/// [`crate::rules::replace_all`]'s own doc comment names.
pub(crate) fn a_colour_edition_rule() -> crate::rules::Rule {
    crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: "- Full Color".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: Some("full-color".to_string()),
        set_kind: None,
        set_language: None,
        set_series: None,
        note: Some("colour edition: matches \"- Full Color\"".to_string()),
    }
}

/// A library on disk holding one work matched by two custom rules that both
/// set `edition_key` for the same path — nothing seeded, so
/// [`crate::rules::read`] returns exactly these two, in a
/// caller-predictable order — for C4 §1's reordering tests: the two rules
/// disagree about what `edition_key` should read, and only their relative
/// `ordinal` decides which one wins.
///
/// **The seeded four are dropped, not kept alongside these two.**
/// [`crate::rules::replace_all`] deletes whatever a proposed list omits, and
/// this fixture hands it a list of exactly two brand-new rules (`id: 0`
/// both), so the table ends up holding only them. Kept, the seeded
/// series-capturing rule (ordinal 1) would tie the first of these two on
/// `ordinal`, and the tie-break — `id` — would make a test that reads
/// `rules[0]` and `rules[1]` reach for whichever rule happened to compile
/// first rather than the pair it actually means to swap.
///
/// **Both are `prefix` rules matching the same path, and only the first sets
/// `series`.** The first (ordinal 1) sets `series` and `edition_key =
/// "first"`; the second (ordinal 2) sets only `edition_key = "second"`. A
/// later rule overrides only the fields it sets (this module's own headline
/// rule, §4), so with the seeded ordinals the second rule's `edition_key`
/// wins — "second" — while `series` is decided once, by the first rule,
/// regardless of order. Swapping the two ordinals reverses which
/// `edition_key` wins without ever touching `series`, which is what makes
/// [`crate::preview::grouping`]'s snapshot change on exactly the field this
/// fixture's own tests are reordering — and derives cleanly either way,
/// since every entry still resolves a `series` no matter which rule runs
/// last.
pub(crate) fn library_with_two_competing_rules() -> FixtureLibrary {
    let dir = tempdir("rules-two-competing");
    crate::scan::tests::write_archive(&dir.path().join("Series").join("Series - v01.cbz"), 3);

    let library = Library::open_in_memory().expect("an in-memory library opens");
    {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");

        let rules = vec![
            crate::rules::Rule {
                id: 0,
                ordinal: 1,
                enabled: true,
                root_id: None,
                kind: crate::rules::RuleKind::Prefix,
                pattern: "Series/".to_string(),
                match_on: crate::rules::MatchOn::Path,
                set_edition_key: Some("first".to_string()),
                set_kind: None,
                set_language: None,
                set_series: Some("Series".to_string()),
                note: Some(
                    "C4 §1 reordering fixture: names the series and sets edition_key \"first\""
                        .to_string(),
                ),
            },
            crate::rules::Rule {
                id: 0,
                ordinal: 2,
                enabled: true,
                root_id: None,
                kind: crate::rules::RuleKind::Prefix,
                pattern: "Series/".to_string(),
                match_on: crate::rules::MatchOn::Path,
                set_edition_key: Some("second".to_string()),
                set_kind: None,
                set_language: None,
                set_series: None,
                note: Some(
                    "C4 §1 reordering fixture: overrides edition_key to \"second\" — the later \
                     rule, until something swaps the ordinals"
                        .to_string(),
                ),
            },
        ];
        crate::rules::replace_all(&conn, &rules)
            .expect("saving the two competing rules in place of the seeded defaults");
        crate::derive::derive(&conn).expect("deriving under the two competing rules");
    }
    FixtureLibrary { library, dir }
}

/// A library on disk holding one accepted `edition-of` proposal, alongside
/// the rule list it held the moment before that accept — the list removing
/// the accepted rule from C4's editor would send back.
///
/// **For `crate::preview::tests`'s reopen case.** [`crate::proposals::accept`]
/// writes a rule and records it in the accepted proposal's
/// `created_rule_id`. `schema/003_scanner.sql` makes that column `ON DELETE
/// SET NULL`, and [`crate::proposals::raise`] — which every derivation calls
/// last — is what notices a `NULL` there on an otherwise-`accepted` row and
/// flips it back to `open`. Previewing `base_rules`, the list returned
/// alongside the fixture, is exactly the one proposed list that removes the
/// `Go-Toubun` accept and nothing else: [`crate::rules::replace_all`] deletes
/// every rule not in the list it is given, and the `Go-Toubun` accept is the
/// only one `base_rules` omits.
///
/// **`Series C` / `Series C - Remaster` sits beside `Go-Toubun`.** It is
/// scanned after the accept, and never derived here at all — so `raise` has
/// never run over it, and `preview`'s own derivation is the first to find it.
/// That is what makes it `proposals_raised` without also being
/// `proposals_reopened`: one fresh proposal alongside one genuine reopen,
/// `raised = 2` against `reopened = 1`, is what a `diff` call swapping the two
/// arguments cannot survive.
///
/// [`library_with_a_surviving_and_a_dropped_accept`] builds two accepts, only
/// one of them dropped: [`crate::rules::replace_all`] only ever deletes the ids
/// genuinely dropped from the proposed list and updates every kept row in
/// place, so the other accepted proposal survives a preview accepted. It has
/// no need of `Series C`, since that surviving proposal answers the same
/// question on its own by staying accepted. Pinning `reopened`'s full
/// predicate — not just the reachable "stays accepted" case — wants
/// hand-built `States` maps, for the two transitions no real `preview` call
/// can produce at all: `dismissed → open` and
/// `open → dismissed`. See `proposals/tests.rs`.
pub(crate) fn library_with_an_accepted_proposal() -> (FixtureLibrary, Vec<crate::rules::Rule>) {
    let dir = tempdir("proposal-accepted");
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01.cbz"),
        3,
    );

    let library = Library::open_in_memory().expect("an in-memory library opens");
    let base_rules = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning the root");
        crate::derive::derive(&conn)
            .expect("the first derive, which raises the edition-of proposal");

        let base_rules = crate::rules::read(&conn)
            .expect("reading the seeded rules")
            .0;
        let proposal = crate::proposals::open(&conn, 0.0)
            .expect("reading open proposals")
            .into_iter()
            .find(|p| p.kind == "edition-of")
            .expect("the fixture's two titles raise exactly one edition-of proposal");
        crate::proposals::accept(&conn, proposal.id).expect("accepting the proposal");

        // Scanned, not derived: `raise` has never run over this pair, so
        // `preview`'s own derivation — the only one that ever reaches it — is
        // what opens it fresh rather than finding it already open.
        crate::scan::tests::write_archive(
            &dir.path().join("Series C").join("Series C - v01.cbz"),
            2,
        );
        crate::scan::tests::write_archive(
            &dir.path()
                .join("Series C - Remaster")
                .join("Series C - Remaster - v01.cbz"),
            2,
        );
        crate::scan::scan_root(&conn, &root).expect("scanning the fixture's fresh second pair");

        base_rules
    };

    (FixtureLibrary { library, dir }, base_rules)
}

/// A library holding two accepted `edition-of` proposals — one whose rule
/// survives into `base_rules`, one whose rule does not.
///
/// **The order is what makes the survivor reachable.** `Series One`'s accept
/// runs first, and `base_rules` — the second element of the pair this
/// returns — is read right after it and before `Series Two`'s accept ever
/// runs, so it already carries `Series One`'s rule alongside the four
/// seeded ones. `Series Two` is accepted next, entirely absent from
/// `base_rules`. Previewing `base_rules` back at the library therefore
/// drops only `Series Two`'s rule: [`crate::rules::replace_all`] deletes
/// exactly the ids a proposed list omits and updates every kept row in
/// place, so `Series One`'s rule is never touched at all, and its proposal
/// stays `accepted` straight through the preview rather than reopening
/// alongside `Series Two`'s.
///
/// **Both pairs are scanned and derived together, in one pass, before
/// either accept.** That single derive is what makes `Series One`,
/// `Series One - Full Color Edition`, `Series Two` and
/// `Series Two - Full Color Edition` read as four distinct, unmerged series
/// — [`crate::preview::grouping`]'s snapshot of exactly this state is what
/// `preview`'s own `before` reads, since neither accept below re-derives —
/// and it is what raises both `edition-of` proposals in the same call.
pub(crate) fn library_with_a_surviving_and_a_dropped_accept()
-> (FixtureLibrary, Vec<crate::rules::Rule>) {
    let dir = tempdir("proposal-surviving-and-dropped");
    crate::scan::tests::write_archive(
        &dir.path().join("Series One").join("Series One - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Series One - Full Color Edition")
            .join("Series One - Full Color Edition - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path().join("Series Two").join("Series Two - v01.cbz"),
        2,
    );
    crate::scan::tests::write_archive(
        &dir.path()
            .join("Series Two - Full Color Edition")
            .join("Series Two - Full Color Edition - v01.cbz"),
        2,
    );

    let library = Library::open_in_memory().expect("an in-memory library opens");
    let base_rules = {
        let conn = library.conn();
        let root = crate::scan::add_root(&conn, dir.path()).expect("adding the root");
        crate::scan::scan_root(&conn, &root).expect("scanning both pairs together");
        crate::derive::derive(&conn)
            .expect("the one derive, which raises both edition-of proposals at once");

        let open = crate::proposals::open(&conn, 0.0).expect("reading open proposals");
        let survivor = open
            .iter()
            .find(|p| p.reason.contains("Series One"))
            .expect("Series One's pair raises its own edition-of proposal")
            .id;
        let dropped = open
            .iter()
            .find(|p| p.reason.contains("Series Two"))
            .expect("Series Two's pair raises its own edition-of proposal")
            .id;

        crate::proposals::accept(&conn, survivor).expect("accepting the survivor");
        let base_rules = crate::rules::read(&conn)
            .expect("reading the rules right after the survivor's own accept")
            .0;
        crate::proposals::accept(&conn, dropped).expect("accepting the one to be dropped");

        base_rules
    };

    (FixtureLibrary { library, dir }, base_rules)
}

fn find_by_name(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).expect("reading a directory under the fixture root") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if let Some(found) = find_by_name(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|f| f == name) {
            return Some(path);
        }
    }
    None
}

/// Every regular file under `root`, by path, with its bytes — for asserting a
/// tree is byte-identical before and after some operation. `.taiyaku/`
/// documents are included deliberately: they are exactly the files a
/// documents policy governs.
pub(crate) fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    collect_files(root, &mut found);
    found
}

fn collect_files(dir: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).expect("reading a directory under the fixture root") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            collect_files(&path, found);
        } else {
            let bytes = std::fs::read(&path).expect("reading a file under the root");
            found.insert(path, bytes);
        }
    }
}

/// What [`tree_snapshot_stamps_normalised`] writes in place of a real stamp.
/// Deliberately not a timestamp, so a value that reached a comparison
/// unnormalised reads as one in the failure output rather than as a near-miss.
const NORMALISED_STAMP: &str = "<stamp normalised for comparison>";

/// [`tree_snapshot`], with every `.library.json`'s `updated` stamp replaced by
/// [`NORMALISED_STAMP`] — for the one comparison in this crate that spans a
/// write the document is *supposed* to receive.
///
/// **Why the stamp cannot be compared.** [`crate::librarydoc::of`] fills
/// `updated` from `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')` at projection time
/// and nothing in the schema holds it, so two projections of an *unchanged*
/// work differ whenever a wall-clock second falls between them.
/// [`crate::hand::reconcile`] normalises the same field for the same reason
/// before deciding whether a document on disk is stale, and its comment says
/// what comparing it would cost there: every reachable document reported
/// stale, including one written a moment ago. Here that would flake — O99.
///
/// **Why this is a second function rather than a change to `tree_snapshot`,
/// and has to stay one.** The crate's three other snapshot comparisons each
/// assert that something wrote *nothing* — a preview, a `Documents::Skip`
/// derivation, and `scan::removal_cost`. For those a rewrite that changed
/// only the stamp is precisely the defect they exist to catch, since the
/// file's bytes moving at all is the whole signal. Normalising inside
/// `tree_snapshot` would blind all three to close one flake.
///
/// **What this normalises is wider than the stamp, and saying only "the
/// stamp" would overstate it.** A document is compared as a `LibraryDoc`
/// parsed and re-serialised, not as the bytes on disk, so two documents whose
/// content agrees but whose formatting does not also compare equal. That
/// costs the one caller nothing — both of its documents are written by the
/// same `to_json`, so there is no formatting difference to hide — but it is
/// what the code does, and a document that will not parse is a panic here
/// rather than a comparison.
pub(crate) fn tree_snapshot_stamps_normalised(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    tree_snapshot(root)
        .into_iter()
        .map(|(path, bytes)| {
            if !path.to_string_lossy().ends_with(".library.json") {
                return (path, bytes);
            }
            let text = String::from_utf8(bytes).expect("a library document is UTF-8");
            let mut doc = crate::librarydoc::LibraryDoc::from_json(&text)
                .expect("a library document under a fixture root parses");
            NORMALISED_STAMP.clone_into(&mut doc.updated);
            (path, doc.to_json().into_bytes())
        })
        .collect()
}

/// **The flake this normalisation closes cannot be seen by running the test
/// that carried it.** What differs is two projections a second apart, and a
/// unit test's two writes land inside the same second almost every time. So
/// the normalisation is proved here directly, on two stamps a second apart
/// (`21:28:49Z` and `21:28:50Z`), rather than by a sleep in the test that
/// needs it.
///
/// **The last two assertions are what keep this honest.** A normalisation
/// that returned an empty map, or dropped documents entirely, would satisfy
/// the equality above and destroy every comparison built on it; and one that
/// tried to parse every file under the root would panic on the archive.
#[test]
fn normalising_the_stamp_hides_a_second_and_hides_nothing_else() {
    let dir = tempdir("stamp-normalisation");
    let document = dir.path().join(".taiyaku").join("abc.library.json");
    std::fs::create_dir_all(document.parent().expect("the document has a parent"))
        .expect("creating .taiyaku/");
    let archive = dir.path().join("Series - v01.cbz");
    std::fs::write(&archive, b"PK\x03\x04 not really a zip").expect("writing the archive");

    let write_document = |updated: &str, title: &str| {
        std::fs::write(
            &document,
            format!(
                "{{\n  \"taiyaku_library\": 1,\n  \"updated\": \"{updated}\",\n  \"title\": \"{title}\"\n}}"
            ),
        )
        .expect("writing the document");
    };

    write_document("2026-09-06T21:28:49Z", "Hand-Set Title");
    let earlier = tree_snapshot_stamps_normalised(dir.path());
    let earlier_strict = tree_snapshot(dir.path());

    write_document("2026-09-06T21:28:50Z", "Hand-Set Title");
    assert_ne!(
        tree_snapshot(dir.path()),
        earlier_strict,
        "the two documents must really differ on disk, or nothing below is proved"
    );
    assert_eq!(
        tree_snapshot_stamps_normalised(dir.path()),
        earlier,
        "a second between two projections of an unchanged work is not a change"
    );

    write_document("2026-09-06T21:28:50Z", "A Different Title");
    assert_ne!(
        tree_snapshot_stamps_normalised(dir.path()),
        earlier,
        "normalising the stamp must not hide the fields beside it"
    );

    assert_eq!(
        earlier.get(&archive).map(Vec::as_slice),
        Some(&b"PK\x03\x04 not really a zip"[..]),
        "a file that is not a document passes through untouched"
    );
}

/// **Two processes are expected to hold this file at once, and the second must
/// wait rather than fail on the spot with "database is locked".**
///
/// `taiyaku serve`, `taiyaku library import` and `taiyaku library export` all
/// default to the same `$XDG_DATA_HOME/taiyaku/library.db`, and `serve` writes
/// a `progress` row on every page turn. `Library`'s own doc argues one
/// connection behind a `Mutex` is enough — a correct argument about
/// contention *inside* one process that says nothing about the two-process
/// case the CLI plainly enables.
///
/// **A busy timeout alone does not fix it.** A `BEGIN DEFERRED` transaction
/// starts as a *reader* at its first `SELECT` — `import::sidecar`'s
/// `find_by_key`, here — and only becomes a writer several statements later.
/// SQLite does not run the busy handler for that promotion. It cannot: two
/// connections holding read transactions and both waiting to promote would
/// deadlock, so it returns `SQLITE_BUSY` immediately instead, and under WAL a
/// snapshot the other writer has already moved past returns
/// `SQLITE_BUSY_SNAPSHOT`, which no amount of waiting resolves.
///
/// Taking the write lock at `BEGIN` is what makes the wait possible, and that
/// is the connection-level `TransactionBehavior::Immediate` in
/// `configure`. This drives the real import path against a second library
/// holding the write lock for 250 ms.
#[test]
fn an_import_waits_for_another_writer_rather_than_failing_on_the_lock() {
    let dir = tempdir("busy");
    let db = dir.path().join("library.db");
    let holder = Library::open(&db).expect("the first library opens");
    let waiter = Library::open(&db).expect("a second library opens on the same file");
    let doc = taiyaku_core::Sidecar::from_json(
        r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
            "editions":{"raw":{"label":"R","members":[
              {"path":"raw/v01.cbz","sha256":"3f9a","pages":10}]}},
            "hub":"raw","alignments":[]}"#,
    )
    .expect("the fixture parses");

    std::thread::scope(|scope| {
        let (locked, is_locked) = std::sync::mpsc::channel();
        scope.spawn(move || {
            let held = holder.conn();
            held.execute_batch("BEGIN IMMEDIATE")
                .expect("the first writer takes the write lock");
            // A real write, so the waiter's read snapshot is genuinely behind
            // the moment this commits — the `SQLITE_BUSY_SNAPSHOT` half.
            held.execute("INSERT INTO tag (name) VALUES ('held')", [])
                .expect("the first writer writes");
            locked.send(()).expect("announcing the lock");
            std::thread::sleep(std::time::Duration::from_millis(250));
            held.execute_batch("COMMIT")
                .expect("the first writer releases the lock");
        });
        is_locked.recv().expect("the write lock is held");

        let conn = waiter.conn();
        crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("an import must wait for the lock rather than fail on it");
    });

    let conn = waiter.conn();
    let title: String = conn
        .query_row("SELECT title FROM work", [], |row| row.get(0))
        .expect("the work the waiting import wrote");
    assert_eq!(title, "T", "the waiting import must actually have landed");
}

/// The wait is **bounded**, and by a value this crate states rather than one
/// it inherits.
///
/// Read back rather than assumed, because the effect above holds for any
/// timeout over 250 ms — including one that would hang a page turn for an
/// hour. Five seconds: longer than any single write this store performs (the
/// largest is one work's import), short enough that a genuinely stuck writer
/// surfaces as an error a person can act on instead of a server that appears
/// to have hung.
///
/// **Stated plainly, since it is the kind of thing that looks like a guard and
/// is not**: `rusqlite` happens to set the same 5 000 ms on every connection
/// it opens (`inner_connection.rs`), so deleting the pragma below leaves this
/// green today. What it pins is the *value*, against a dependency that has
/// never promised it and against anyone setting a different one here. The
/// property that actually needed a failing test is the transaction behaviour,
/// which is the test above.
#[test]
fn every_connection_carries_the_busy_timeout() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let millis: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        millis, 5_000,
        "a busy writer waits five seconds before giving up"
    );
}

/// **The first open of a fresh library waits for the file.** Switching a
/// database into WAL takes a brief exclusive lock, and SQLite runs no busy
/// handler for it: while any other connection holds the file, the pragma
/// returns `SQLITE_BUSY` on the spot, however long the busy timeout is. So the
/// first open of a *fresh* library — the one open every install performs, with
/// `serve` and `library import` defaulting to the same file — must wait for
/// the file rather than fail outright with
/// `opening the library: setting WAL: database is locked`.
///
/// It is only ever the first: once the file is in WAL the pragma is a no-op
/// that takes no lock, which is why this test has to hold a lock on a database
/// that is *not* yet in WAL, exactly as a first open finds one.
///
/// Reordering the timeout ahead of the pragma does not fix this. Waiting does.
#[test]
fn a_first_open_waits_for_the_file_rather_than_failing_to_set_wal() {
    let dir = tempdir("wal-busy");
    let db = dir.path().join("library.db");
    let holder = rusqlite::Connection::open(&db).expect("a plain connection opens the file");
    holder
        .execute_batch("CREATE TABLE seed (x)")
        .expect("a real file, in the rollback-journal mode a fresh database starts in");
    let mode: String = holder
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_ne!(
        mode.to_lowercase(),
        "wal",
        "the fixture must not already be in WAL, or this test proves nothing"
    );

    std::thread::scope(|scope| {
        let (locked, is_locked) = std::sync::mpsc::channel();
        scope.spawn(move || {
            holder
                .execute_batch("BEGIN IMMEDIATE; INSERT INTO seed VALUES (1);")
                .expect("the other process takes the write lock");
            locked.send(()).expect("announcing the lock");
            std::thread::sleep(std::time::Duration::from_millis(250));
            holder
                .execute_batch("COMMIT")
                .expect("the other process releases the lock");
        });
        is_locked.recv().expect("the write lock is held");

        Library::open(&db).expect("a first open must wait for the file rather than fail on it");
    });

    let library = Library::open(&db).expect("and the library is openable afterwards");
    let conn = library.conn();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        mode.to_lowercase(),
        "wal",
        "the wait ended in WAL, not in giving up quietly"
    );
}

/// **Several processes opening one fresh library at the same moment**, which
/// is what `serve` and `taiyaku library import` sharing
/// `$XDG_DATA_HOME/taiyaku/library.db` do on the very first run. A race like
/// this is only ever possible on the first open, which is the one every
/// install performs.
///
/// Threads rather than processes: each carries its own `Connection`, which is
/// the unit SQLite locks against, and a test that spawns four copies of the
/// binary would be measuring the harness.
#[test]
fn several_first_opens_of_one_fresh_library_at_once_all_succeed() {
    let dir = tempdir("first-open-race");
    let db = dir.path().join("library.db");

    std::thread::scope(|scope| {
        let opens: Vec<_> = (0..4)
            .map(|n| {
                let db = db.clone();
                scope.spawn(move || Library::open(&db).map(|_| n))
            })
            .collect();
        for open in opens {
            match open.join().expect("no thread panicked") {
                Ok(_) => {}
                Err(e) => panic!("every concurrent first open must succeed: {e}"),
            }
        }
    });

    let library = Library::open(&db).expect("and the file is a working library afterwards");
    let conn = library.conn();
    let version: u32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("the version row")
        .parse()
        .expect("a version number");
    assert_eq!(
        version,
        crate::schema::SCHEMA_VERSION,
        "migrated exactly once, by whichever open got there first"
    );
}

/// The wait is bounded, and what it says when it runs out is the point.
///
/// A bare `setting WAL: database is locked` would name a condition and offer
/// nothing, on a database that is fine. `set_wal_within` is split out
/// from `set_wal` for the same reason `require_fts5_with_probe` is split from
/// `require_fts5`: the giving-up branch is unreachable from a test that is not
/// willing to hold a lock for the full five seconds.
#[test]
fn giving_up_on_wal_says_how_long_it_waited_and_that_nothing_was_lost() {
    let dir = tempdir("wal-gives-up");
    let db = dir.path().join("library.db");
    let holder = rusqlite::Connection::open(&db).expect("a plain connection opens the file");
    holder
        .execute_batch("CREATE TABLE seed (x); BEGIN IMMEDIATE; INSERT INTO seed VALUES (1);")
        .expect("the other process takes the write lock and keeps it");

    let waiter = rusqlite::Connection::open(&db).expect("a second connection opens");
    let refused = crate::set_wal_within(&waiter, std::time::Duration::from_millis(50))
        .expect_err("a lock nobody releases must end in an error, not a hang");
    assert!(
        refused.starts_with("setting WAL: "),
        "the message keeps the step it failed at: {refused}"
    );
    assert!(
        refused.contains("nothing has been written"),
        "and says the library is not damaged, which is what the bare SQLite \
         message left a reader to guess: {refused}"
    );
}

/// `PRAGMA foreign_keys` is a property of the **connection**, not of the
/// database, and SQLite leaves it off by default for backward compatibility.
/// A `Library::open` that forgets it turns every `REFERENCES` in the schema
/// into a comment — silently, with every constraint appearing to be there.
#[test]
fn every_connection_enforces_foreign_keys() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        on, 1,
        "foreign key enforcement must be on for this connection"
    );
}

/// FTS5 is a property of the host's SQLite build, not of the crate, because
/// this links the system library rather than bundling one. Debian and Arch both
/// build it in; "both distros I checked" is not "every host". Without this
/// check the failure surfaces as the schema migration dying part-way with
/// `no such module: fts5`, on a half-created schema.
///
/// This host has FTS5 — CI's rust:1-bookworm and this workstation both do —
/// so there is no way to reach `require_fts5`'s refusal through the real
/// probe statement.
///
/// This calls `require_fts5` itself, through `require_fts5_with_probe`,
/// handing it a probe statement engineered to fail the exact way a real
/// FTS5-less SQLite fails ("no such module") without needing a host that
/// actually lacks FTS5. It runs the same `execute_batch`,
/// same `map_err`, same message-building the real call site
/// (`configure`, `lib.rs`) uses — only the SQL differs — so gutting
/// `require_fts5_with_probe`'s refusal (for instance, ignoring the `Err` and
/// returning `Ok(())` regardless) fails this test.
///
/// This pins the refusal and its message. That `from_connection` *calls*
/// `require_fts5` at all is a separate claim, pinned separately by
/// `from_connection_runs_the_fts5_probe_before_applying_the_schema` below.
#[test]
fn opening_refuses_a_sqlite_without_fts5_and_names_what_is_missing() {
    let library = Library::open_in_memory().expect("this host has FTS5");
    let conn = library.conn();
    let err = require_fts5_with_probe(
        &conn,
        "CREATE VIRTUAL TABLE temp.taiyaku_fts5_probe USING fts5_module_that_does_not_exist(probe)",
    )
    .expect_err("a probe naming a nonexistent module must fail exactly as a missing FTS5 would");
    assert!(
        err.contains("FTS5"),
        "the message must name what is missing: {err}"
    );
    assert!(
        err.contains("libsqlite3") || err.to_lowercase().contains("install"),
        "the message must suggest a fix: {err}"
    );
}

/// The guard above is only worth anything if `from_connection` actually runs
/// it: on a host with FTS5, deleting the `require_fts5` call would change
/// nothing `open_in_memory` returns, so the probe could be removed entirely and
/// every other test would stay green.
///
/// It can be pinned without a host that lacks FTS5, and without widening the
/// public constructors. `from_connection` is private but reachable from this
/// module, and the probe names a specific table — `temp.taiyaku_fts5_probe`.
/// Occupying that name before opening makes the probe's `CREATE VIRTUAL
/// TABLE` fail on a host that has FTS5, which is exactly the branch that
/// otherwise cannot be reached.
///
/// So the assertion is about **sequencing, not about FTS5**: the schema
/// migration never touches `temp`, so with the probe wired the open fails
/// with the FTS5 refusal, and with the probe deleted the migration runs to
/// completion and the open succeeds.
///
/// Asserting the refusal's own wording rather than merely that opening failed
/// is what keeps it honest — a migration that failed for some unrelated
/// reason would also be an `Err`, and would pass a test that only checked for
/// one.
#[test]
fn from_connection_runs_the_fts5_probe_before_applying_the_schema() {
    let conn = Connection::open_in_memory().expect("an in-memory connection opens");
    conn.execute_batch("CREATE TABLE temp.taiyaku_fts5_probe (occupied)")
        .expect("the probe's table name is free until the probe takes it");

    let err = Library::from_connection(conn, None)
        .expect_err("the probe cannot create its table, so opening must refuse");

    assert!(
        err.contains("which the library's search needs"),
        "opening must fail through require_fts5's refusal, not somewhere later: {err}"
    );
}

/// `INSERT OR REPLACE` on `series_title` must not leave a phantom row in the
/// search index.
///
/// Its primary key is the composite `(series_id, title)`, not a rowid alias, so
/// REPLACE resolves a conflict by deleting and re-inserting with a **fresh
/// rowid** — and that implicit delete fires no DELETE trigger unless
/// `recursive_triggers` is on. Without the pragma this returns 2 for a title
/// that exists once. `work` is immune, because its `id` IS a rowid alias and
/// REPLACE overwrites in place; that asymmetry is why this is asserted rather
/// than assumed.
#[test]
fn replacing_a_series_title_leaves_no_phantom_in_the_search_index() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO series_title (series_id, title) VALUES (1, 'Frieren');
         INSERT OR REPLACE INTO series_title (series_id, title, kind)
             VALUES (1, 'Frieren', 'romaji');",
    )
    .expect("insert then replace");

    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM series_fts WHERE series_fts MATCH 'Frieren'",
            [],
            |row| row.get(0),
        )
        .expect("fts query");
    assert_eq!(hits, 1, "a title that exists once must match once");
}

#[test]
fn a_fresh_database_records_its_schema_version() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let version: u32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| {
                let text: String = row.get(0)?;
                Ok(text.parse().unwrap_or(0))
            },
        )
        .expect("meta carries a schema version");
    assert_eq!(version, crate::schema::SCHEMA_VERSION);
}

/// A file-backed library hands out a *second* connection, so a long job does
/// not hold the mutex every request needs.
///
/// Asserts the `Working::Owned` variant directly rather than only relying on
/// the absence of a deadlock: a `std::sync::Mutex` re-locked on the same
/// thread hangs rather than returning an error, so a `working()` that wrongly
/// handed back `Working::Shared(library.conn())` would time this test out
/// instead of failing it cleanly — a bad failure mode for CI to sit through.
///
/// Also asserts that the second connection actually ran `Self::configure`, the
/// same way `every_connection_enforces_foreign_keys` pins it for the first: a
/// version that opened the file but skipped configuring it — dropping
/// `Self::configure(&mut conn)?;` from the file-backed branch — would return an
/// `Owned` connection that still satisfies every assertion above while silently
/// missing foreign keys, the busy timeout, WAL, recursive triggers, and
/// `Immediate` transactions. Checking `foreign_keys` is enough to catch that:
/// it is off (`0`) on a freshly-opened connection `configure` never touched.
#[test]
fn a_file_backed_library_works_on_a_connection_of_its_own() {
    let dir = tempdir("library-working");
    let path = dir.path().join("library.db");
    let library = Library::open(&path).expect("opens");

    let working = library.working().expect("a working connection");
    assert!(
        matches!(working, Working::Owned(_)),
        "a file-backed library must hand out a connection of its own, not the shared one"
    );

    let on: i64 = working
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        on, 1,
        "the working connection must be configured, same as the first: foreign keys must be on"
    );
    let busy_timeout: i64 = working
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(
        busy_timeout,
        i64::try_from(BUSY_TIMEOUT.as_millis()).unwrap_or(i64::MAX),
        "the working connection must carry the same busy timeout configure sets on the first"
    );

    // The shared connection is free while the working one is held: with the
    // variant already pinned above, this proves the two are actually usable
    // at once rather than one blocking the other.
    let shared = library.conn();
    shared
        .execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("the shared connection is usable while the working one is held");
    drop(shared);

    // "A second connection was opened" and "a second connection to the same
    // file" are different claims — only the second one matters. Write
    // through the working connection and read it back through the shared
    // one to prove they see one database, not two.
    working
        .execute("INSERT INTO probe (id) VALUES (1)", [])
        .expect("the working connection writes");
    let seen: i64 = library
        .conn()
        .query_row("SELECT id FROM probe", [], |row| row.get(0))
        .expect("the shared connection reads back what the working connection wrote");
    assert_eq!(seen, 1, "both connections must see the same database");
}

/// An in-memory library has no file to open twice, so the working connection
/// *is* the shared one. Serve's fixtures are all in-memory, and this is what
/// lets them reach the preview and scan paths at all.
#[test]
fn an_in_memory_library_works_on_the_connection_it_already_has() {
    let library = Library::open_in_memory().expect("opens");
    assert!(library.path().is_none());

    let working = library.working().expect("a working connection");
    assert!(
        matches!(working, Working::Shared(_)),
        "an in-memory library has no file, so working must be the shared connection"
    );

    working
        .execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("the working connection writes");
}

/// A save writes on a connection of its own, set up by `configure` (O272):
/// foreign keys, recursive triggers, the grant's `walk_reaches`, and a
/// transaction that takes the write lock at its `BEGIN`. None of the four is
/// SQLite's default.
#[test]
fn a_save_writes_on_a_configured_connection_of_its_own() {
    let dir = tempdir("library-saving");
    let library = Library::open(&dir.path().join("library.db")).expect("opens");

    let saving = library.saving().expect("a connection for the save");
    assert!(
        matches!(saving, Working::Owned(_)),
        "a file-backed library's save shares no connection"
    );
    for pragma in ["foreign_keys", "recursive_triggers"] {
        let on: i64 = saving
            .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get(0))
            .expect("the pragma answers");
        assert_eq!(on, 1, "`{pragma}` on the save's connection");
    }
    let reaches = |patterns: Option<&str>| -> bool {
        saving
            .query_row(
                "SELECT walk_reaches('/d', ?1, '/d/v01.cbz')",
                [patterns],
                |row| row.get(0),
            )
            .expect("`walk_reaches` on the save's connection")
    };
    assert!(reaches(None), "a walk with no patterns reaches the file");
    assert!(!reaches(Some("*.cbz")), "a walk excluding it does not");
    let held = saving.unchecked_transaction().expect("begins");
    let other = rusqlite::Connection::open(library.path().expect("a file")).expect("opens");
    other
        .busy_timeout(std::time::Duration::ZERO)
        .expect("sets no wait");
    let refused = other
        .execute_batch("BEGIN IMMEDIATE")
        .expect_err("the save's BEGIN holds the write lock");
    assert!(is_busy(&refused), "{refused}");
    drop(held);
}

/// A relative path reaches [`Library::open`] only through `taiyaku-cli`'s
/// `--db` flag, which — unlike its own `$XDG_DATA_HOME`/`$HOME` fallbacks —
/// does no absolute-path check. If `open` stored the relative path verbatim,
/// the *second* connection [`Library::working`] opens against `self.path`
/// would resolve it against whatever the process's current directory is
/// *then*, not the directory `open` ran in — silently opening a different,
/// empty database file if the two differ.
///
/// This moves the process into a fresh directory, opens the library by a
/// bare relative filename, and proves `working()` still lands on the same
/// database `open` created — the same round trip
/// `a_file_backed_library_works_on_a_connection_of_its_own` uses, just
/// reached through a relative path instead of an absolute one. The body runs
/// under `catch_unwind` so a failed assertion still restores the process's
/// working directory before this test returns — nothing else in this
/// crate's suite touches the current directory, so this is otherwise the
/// only test that could leave a stale one behind for whichever test runs
/// next in the same process.
#[test]
fn a_relative_path_still_works_on_the_same_database() {
    let dir = tempdir("library-relative");
    let original = std::env::current_dir().expect("a working directory exists");
    std::env::set_current_dir(dir.path()).expect("moving into the test directory");

    let result = std::panic::catch_unwind(|| {
        let library = Library::open(Path::new("library.db")).expect("opens with a relative path");
        assert!(
            library.path().is_some_and(Path::is_absolute),
            "the stored path must be canonicalised to absolute even though the caller passed a relative one: {:?}",
            library.path()
        );

        let working = library.working().expect("a working connection");
        assert!(
            matches!(working, Working::Owned(_)),
            "a file-backed library must hand out a connection of its own here too"
        );
        working
            .execute("CREATE TABLE probe (id INTEGER)", [])
            .expect("the working connection writes");
        working
            .execute("INSERT INTO probe (id) VALUES (1)", [])
            .expect("the working connection writes");
        let seen: i64 = library
            .conn()
            .query_row("SELECT id FROM probe", [], |row| row.get(0))
            .expect("the shared connection reads back what the working connection wrote");
        assert_eq!(
            seen, 1,
            "both connections must see the same database even when open was given a relative path"
        );
    });

    std::env::set_current_dir(&original).expect("restoring the working directory");
    result.expect("the assertions above must pass");
}

/// [`Nested`] opened on a connection with nothing else in flight behaves
/// exactly like `unchecked_transaction`: `commit` lands the write and leaves
/// the connection in autocommit mode again.
#[test]
fn a_nested_scope_commits_for_real_when_nothing_else_is_open() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("creating the probe table");

    let scope = Nested::open(&conn, "test").expect("opening with nothing else in flight");
    assert!(
        !conn.is_autocommit(),
        "a real transaction must be open while the scope is held"
    );
    scope
        .execute("INSERT INTO probe (id) VALUES (1)", [])
        .expect("writing through the scope");
    scope.commit().expect("committing the scope");

    assert!(
        conn.is_autocommit(),
        "committing must leave the connection in autocommit mode, same as a plain `COMMIT`"
    );
    let seen: i64 = conn
        .query_row("SELECT id FROM probe", [], |r| r.get(0))
        .expect("the committed row");
    assert_eq!(seen, 1);
}

/// The other half: dropped without `commit`, [`Nested`] rolls back — the same
/// default `rusqlite::Transaction` has, for the same reason (an early return
/// through `?` must undo what it started).
#[test]
fn a_nested_scope_rolls_back_on_drop_when_nothing_else_is_open() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("creating the probe table");

    {
        let scope = Nested::open(&conn, "test").expect("opening with nothing else in flight");
        scope
            .execute("INSERT INTO probe (id) VALUES (1)", [])
            .expect("writing through the scope");
        // Dropped here, uncommitted.
    }

    assert!(
        conn.is_autocommit(),
        "a dropped, uncommitted scope must leave nothing open behind it"
    );
    let count: i64 = conn
        .query_row("SELECT count(*) FROM probe", [], |r| r.get(0))
        .expect("counting rows");
    assert_eq!(count, 0, "the uncommitted write must not have landed");
}

/// The property [`crate::preview::preview`] is actually built on: opened
/// while a transaction is already active, [`Nested`] nests as a `SAVEPOINT`
/// rather than refusing outright, and committing it folds its write into the
/// *caller's* transaction rather than onto disk — so rolling the outer
/// transaction back afterwards takes the nested write with it.
///
/// This is what a bare `unchecked_transaction` cannot do at all: called the
/// same way, it returns `Err` on the spot ("cannot start a transaction
/// within a transaction"), which is the failure this type exists to avoid —
/// see its own doc comment.
#[test]
fn a_nested_scope_becomes_a_savepoint_inside_an_open_transaction_and_its_commit_still_rolls_back_with_the_outer_one()
 {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("creating the probe table");

    let outer = conn
        .unchecked_transaction()
        .expect("opening the outer transaction");
    let inner = Nested::open(&outer, "inner").expect("nesting inside the open outer transaction");
    inner
        .execute("INSERT INTO probe (id) VALUES (1)", [])
        .expect("writing through the nested scope");
    inner.commit().expect("releasing the nested savepoint");

    outer
        .rollback()
        .expect("rolling back the outer transaction");

    let count: i64 = conn
        .query_row("SELECT count(*) FROM probe", [], |r| r.get(0))
        .expect("counting rows");
    assert_eq!(
        count, 0,
        "a released savepoint must still be inside the outer transaction, not committed to disk \
         on its own — this is `preview`'s whole mechanism: `rules::replace_all` and \
         `derive_with` both write through a scope exactly like this one, nested inside \
         `preview`'s own outer transaction, and its rollback must take every one of them with it"
    );
}

/// The other side of the same property: a nested scope that rolls back
/// undoes only *its own* writes, leaving the caller's transaction open and
/// usable — not a bare `ROLLBACK`, which would end the outer transaction too
/// and silently drop a write that scope never touched.
#[test]
fn a_nested_scopes_own_rollback_does_not_disturb_the_transaction_it_nests_inside() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("creating the probe table");

    let outer = conn
        .unchecked_transaction()
        .expect("opening the outer transaction");
    outer
        .execute("INSERT INTO probe (id) VALUES (1)", [])
        .expect("writing directly through the outer transaction, before the nested scope opens");
    {
        let inner =
            Nested::open(&outer, "inner").expect("nesting inside the open outer transaction");
        inner
            .execute("INSERT INTO probe (id) VALUES (2)", [])
            .expect("writing through the nested scope");
        // Dropped here, uncommitted — only this scope's own write must undo.
    }
    outer
        .execute("INSERT INTO probe (id) VALUES (3)", [])
        .expect(
            "the outer transaction must still be open and usable after the nested scope rolled \
             back its own savepoint",
        );
    outer.commit().expect("committing the outer transaction");

    let mut ids: Vec<i64> = conn
        .prepare("SELECT id FROM probe ORDER BY id")
        .expect("preparing")
        .query_map([], |r| r.get(0))
        .expect("querying")
        .collect::<rusqlite::Result<_>>()
        .expect("reading every row");
    ids.sort_unstable();
    assert_eq!(
        ids,
        vec![1, 3],
        "row 2 — written only inside the nested scope that rolled back — must be gone, while \
         row 1 (written before it opened) and row 3 (written after it dropped, through the same \
         outer transaction) must both have survived"
    );
}

/// `commit`'s real-transaction branch takes `self.tx` to `None` via
/// `Option::take` before it ever runs the real `COMMIT` — so at drop time,
/// `tx.is_none()` reads exactly the same whether this scope committed a real
/// transaction or released a `SAVEPOINT`. A `Drop` guard that checked
/// `tx.is_none()` alone could not tell the two apart, and would run
/// `ROLLBACK TO SAVEPOINT <label>; RELEASE SAVEPOINT <label>` after every
/// ordinary commit, each silently swallowed by `let _ =` and each a wasted
/// parse-and-step against a savepoint that was never opened.
///
/// **Why this watches the SQL rather than `is_autocommit()`.** A spurious
/// statement is not observable that way:
/// `a_nested_scope_commits_for_real_when_nothing_else_is_open` above checks
/// `is_autocommit()` after a real-branch commit, and the statement would
/// simply fail against an autocommit connection with no savepoint to touch,
/// leaving `is_autocommit()` reading exactly as it would have anyway.
/// Watching the statement text is what pins that the statement is not sent at
/// all, not merely that it has no visible effect when it is.
#[test]
fn committing_the_real_transaction_branch_sends_no_savepoint_statement() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SAVEPOINT_STATEMENTS: AtomicUsize = AtomicUsize::new(0);
    fn watch(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event
            && sql.contains("SAVEPOINT")
        {
            SAVEPOINT_STATEMENTS.fetch_add(1, Ordering::SeqCst);
        }
    }

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(watch),
    );

    // Nothing else is open, so `Nested::open` takes the real-transaction
    // branch — the one this scope's own `label` never becomes a `SAVEPOINT`
    // name for.
    let scope = Nested::open(&conn, "watched").expect("nothing else is open, the real branch");
    scope
        .commit()
        .expect("committing the real-transaction branch");

    conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    assert_eq!(
        SAVEPOINT_STATEMENTS.load(Ordering::SeqCst),
        0,
        "committing the real-transaction branch must send no SAVEPOINT-related statement at all \
         — a savepoint named `watched` was never opened, so `Drop` has nothing of that name to \
         roll back or release"
    );
}

/// The savepoint branch's mirror of the test just above: this scope's own
/// `commit` releases its savepoint, so `Drop` must not roll back to or
/// release it again. A `ROLLBACK TO` against an already-released savepoint
/// would either error (swallowed by `drop`'s `let _ =`) or, worse, roll a
/// *different* still-open savepoint of the same name back to a checkpoint
/// nothing here asked for. The savepoint tests above only check that the data
/// ended up right, not which statements got there.
///
/// Same harness as `committing_the_real_transaction_branch_sends_no_savepoint_statement`
/// above, extended to the branch that one does not reach: an outer
/// transaction already open, so `Nested::open` takes the `SAVEPOINT` arm, and
/// this scope's own `commit` is what releases it.
#[test]
fn committing_the_savepoint_branch_sends_no_rollback_to_savepoint_statement() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ROLLBACK_TO_STATEMENTS: AtomicUsize = AtomicUsize::new(0);
    fn watch(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event
            && sql.contains("ROLLBACK TO SAVEPOINT")
        {
            ROLLBACK_TO_STATEMENTS.fetch_add(1, Ordering::SeqCst);
        }
    }

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("creating the probe table");

    let outer = conn.unchecked_transaction().expect(
        "opening the outer transaction, so the nested scope below takes the SAVEPOINT \
                 branch rather than the real-transaction one",
    );
    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(watch),
    );

    let scope = Nested::open(&outer, "watched_savepoint")
        .expect("nesting inside the already-open outer transaction, the SAVEPOINT branch");
    scope.commit().expect(
        "releasing the nested savepoint — this scope's own `Drop` fires here too, \
                 immediately, since `commit` takes `self` by value",
    );

    conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    outer
        .commit()
        .expect("committing the outer transaction for real");

    assert_eq!(
        ROLLBACK_TO_STATEMENTS.load(Ordering::SeqCst),
        0,
        "committing the savepoint branch must send no `ROLLBACK TO SAVEPOINT` at all — the \
         savepoint named `watched_savepoint` was released, not left open, so `Drop` has nothing \
         left to roll back"
    );
}

/// A note with a region, for the `stale` and `same_anchor` tests below: each
/// changes one or more fields from here.
fn a_note() -> Note {
    Note {
        text: None,
        id: "01J8".into(),
        edition: "raw".into(),
        slot: 3,
        region: Some(Region {
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
        }),
        color: None,
        body: "why is this page here".into(),
        created: "2026-08-05T10:00:00Z".into(),
        updated: "2026-08-05T10:00:00Z".into(),
        stale: false,
        at: None,
    }
}

/// Every notes document written before `stale` existed has no `stale` key, a
/// reader echoing a read's answer sends one, and `Note` refuses a body it
/// cannot fill.
#[test]
fn a_note_with_no_stale_key_reads_as_not_stale_and_one_with_it_reads_as_stale() {
    let unmarked = r#"{"id":"01J8","edition":"raw","slot":3,"body":"why","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z"}"#;
    let note: Note = serde_json::from_str(unmarked)
        .expect("a note with no `stale` key, as every body written so far has, must read");
    assert!(
        !note.stale,
        "a note with no `stale` key must read as not stale"
    );

    let marked = r#"{"id":"01J8","edition":"raw","slot":3,"body":"why","created":"2026-08-05T10:00:00Z","updated":"2026-08-05T10:00:00Z","stale":true}"#;
    let note: Note = serde_json::from_str(marked)
        .expect("a note with `\"stale\": true`, as a reader echoes a read's answer, must read");
    assert!(
        note.stale,
        "a note with `\"stale\": true` must read as stale"
    );
}

/// `stale` is the exception, so a document carries it only on the notes it
/// marks.
#[test]
fn a_note_is_written_with_a_stale_key_only_when_it_is_stale() {
    let mut note = a_note();
    let written = serde_json::to_value(&note).expect("serialising a note");
    assert_eq!(
        written.get("stale"),
        None,
        "a note that is not stale must be written with no `stale` key"
    );

    note.stale = true;
    let written = serde_json::to_value(&note).expect("serialising a note");
    assert_eq!(
        written.get("stale"),
        Some(&serde_json::Value::Bool(true)),
        "a stale note must be written with `\"stale\": true`"
    );
}

#[test]
fn two_notes_have_the_same_anchor_when_only_their_words_colour_times_or_stale_differ() {
    let held = a_note();

    let retyped = Note {
        body: "the same page, other words".into(),
        color: Some("amber".into()),
        created: "2026-09-01T08:00:00Z".into(),
        updated: "2026-09-02T08:00:00Z".into(),
        stale: true,
        ..a_note()
    };
    assert_ne!(
        held, retyped,
        "the fixture's two notes must differ, or `same_anchor` could be `==`"
    );
    assert!(
        held.same_anchor(&retyped),
        "a note whose body, colour, times and `stale` changed has not moved"
    );

    let moved = [
        (
            "its edition changes",
            Note {
                edition: "scan".into(),
                ..a_note()
            },
        ),
        (
            "its slot changes",
            Note {
                slot: 4,
                ..a_note()
            },
        ),
        (
            "its region changes",
            Note {
                region: Some(Region {
                    x: 0.5,
                    y: 0.2,
                    w: 0.3,
                    h: 0.4,
                }),
                ..a_note()
            },
        ),
        (
            "its region becomes the whole slot",
            Note {
                region: None,
                ..a_note()
            },
        ),
        (
            "it comes to mark a run of text",
            Note {
                text: Some(crate::TextRange { start: 1, end: 5 }),
                ..a_note()
            },
        ),
    ];
    for (what, note) in moved {
        assert!(!held.same_anchor(&note), "a note has moved when {what}");
    }
}

/// A `taiyaku_notes` past `u32` is newer than any build, so it is refused as
/// newer, named at the largest `u32`. 2^32 is chosen because it truncates to 0
/// and fails `u32::try_from`, so falling back to 0 or casting would both read
/// it as a version this build reads and let it through to the parse.
#[test]
fn a_notes_document_declaring_a_version_past_u32_is_refused_as_newer() {
    let text = br#"{"taiyaku_notes":4294967296,"work":"t","notes":[]}"#;
    assert_eq!(
        Notes::refuse_newer(text),
        Err(
            "this document declares taiyaku_notes format version 4294967295, and this build \
             reads 1. Refusing to read it rather than dropping the fields it does not \
             understand and writing them back out as version 1 — upgrade taiyaku instead"
                .to_owned()
        )
    );
}

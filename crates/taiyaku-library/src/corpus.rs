//! Standing checks against the real corpus. Gated on `TAIYAKU_CORPUS` and
//! skipping cleanly without it.
//!
//! Neither of these two opens a file the scanner itself would not:
//! `scan::scan_root` and `derive::derive` are called exactly as `taiyaku
//! library scan` calls them, over the real corpus tree rather than a
//! hand-built fixture.

/// Every corpus sidecar survives import and projection **byte for byte**.
///
/// The honest bar, and it is what makes the four-slot-column and
/// never-read-verdict-back decisions testable at all: a projection that loses a
/// field passes semantic equality and fails this.
///
/// What it does not exercise: this corpus snapshot carries no `credits` list,
/// no `by: "hand"` run and no `note` field on any of its documents — every
/// edition, alignment and run here is machine-produced. Those three fields are
/// not untested, just not tested here; they are covered by the hand-written
/// fixtures in `import/tests.rs` instead.
#[test]
fn every_corpus_sidecar_survives_a_round_trip() {
    let Some(root) = std::env::var_os("TAIYAKU_CORPUS") else {
        eprintln!("TAIYAKU_CORPUS not set; skipping");
        return;
    };
    let root = std::path::PathBuf::from(root);

    let mut checked = 0usize;
    for entry in std::fs::read_dir(&root).expect("corpus root reads") {
        let dir = entry.expect("entry").path();
        let path = dir.join("taiyaku.json");
        if !path.is_file() {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("sidecar reads");
        let parsed = taiyaku_core::Sidecar::from_json(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let canonical = parsed.to_json();

        let library = crate::Library::open_in_memory().expect("library");
        let conn = library.conn();
        let id = crate::import::sidecar(&conn, &parsed, &path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let projected = crate::project::sidecar(&conn, id)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        assert_eq!(
            projected.to_json(),
            canonical,
            "{} did not survive the round trip",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 12,
        "expected at least twelve corpus works, saw {checked}"
    );
}

/// A standing check over the scanner itself — the walk, the rules and
/// derivation — against real material.
///
/// **Scoped to the corpus's *settled* material, one root per
/// `<series>-vNN/` directory, and `incoming/` is not walked at all.**
/// `corpus/README.md` draws this boundary itself: the top level is
/// "settled material used for regression", while `incoming/` is "whatever a
/// particular investigation needed and has not been tidied". `incoming/`'s many
/// unrelated investigations — `bocchi`, `chainsawman`, `onepiece`,
/// `tongari`… — share no rule beyond the four seeded defaults, which resolve
/// every path under `incoming/` to *one* series (`^(?<series>[^/]+)/` on the
/// first path component alone) and then, for any two entries that happen to
/// resolve the same volume number from their own filenames, one *work* —
/// several genuinely unrelated titles as different editions of "the same"
/// volume. That is a property of running the bare seeded rules over an
/// untidied dump, and a standing check must not fail on a fact of the fixture
/// rather than a regression. Each settled directory becoming its own
/// `root_id` also makes the third assertion below exact rather than a
/// path-prefix guess: two works can share a root only if their own material
/// genuinely does.
///
/// **What it asserts.**
///
/// - `report.works > 0` — the corpus must derive *something*, or the other
///   two assertions below are vacuously true and this test measures nothing.
/// - Every `member.pages` is at least one: a zero-page member is a container
///   standing in for the archive beneath it.
/// - No work holds members from more than one root — one `<series>-vNN/`
///   directory must never contribute to a work another one's material is
///   also on. This checks for it directly rather than re-deriving it from
///   the rules.
///
/// # Panics
///
/// Panics (failing the test) if the corpus derives no work, if any member
/// holds zero or fewer pages, or if any work's members span more than one
/// root. Also panics — rather than skipping — if `TAIYAKU_CORPUS` is set
/// but reading its own top level, or a walk or a derive, themselves return
/// an `Err`: a scanner that cannot even *run* over real material is a
/// stronger finding than this test's own three assertions, and must not be
/// read as "unset; skipping".
#[test]
fn scanning_and_deriving_the_corpus_produces_real_structure() {
    let Some(root) = std::env::var_os("TAIYAKU_CORPUS") else {
        eprintln!("TAIYAKU_CORPUS not set; skipping");
        return;
    };
    let root = std::path::PathBuf::from(root)
        .canonicalize()
        .unwrap_or_else(|e| panic!("TAIYAKU_CORPUS: {e}"));

    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();

    // One root per settled `<series>-vNN/` directory — see this test's own
    // doc comment for why `incoming/` is excluded rather than walked and
    // filtered after the fact.
    let mut settled: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("reading {root:?}: {e}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("reading an entry of {root:?}: {e}"))
                .path()
        })
        .filter(|path| path.is_dir())
        .filter(|path| path.file_name().and_then(|n| n.to_str()) != Some("incoming"))
        .collect();
    settled.sort();
    assert!(
        !settled.is_empty(),
        "expected at least one settled `<series>-vNN/` directory under {root:?}"
    );

    // `(root_id, its directory)`, so the last check can ask "which of the
    // roots this test itself added does `path` sit under" without guessing
    // at a directory boundary from the path alone.
    let mut roots: Vec<(i64, std::path::PathBuf)> = Vec::with_capacity(settled.len());
    for dir in settled {
        let scanned_root =
            crate::scan::add_root(&conn, &dir).unwrap_or_else(|e| panic!("adding {dir:?}: {e}"));
        let scan_report = crate::scan::scan_root(&conn, &scanned_root)
            .unwrap_or_else(|e| panic!("scanning {dir:?}: {e}"));
        assert!(
            scan_report.refused.is_none(),
            "{dir:?} must not be refused as empty: {:?}",
            scan_report.refused
        );
        roots.push((scanned_root.id, dir));
    }
    let report = crate::derive::derive(&conn).unwrap_or_else(|e| panic!("deriving {root:?}: {e}"));

    assert!(
        report.works > 0,
        "the corpus must derive at least one work, or this check measures nothing"
    );

    let zero_page_members: i64 = conn
        .query_row("SELECT count(*) FROM member WHERE pages <= 0", [], |r| {
            r.get(0)
        })
        .expect("counting zero-page members");
    assert_eq!(
        zero_page_members, 0,
        "every member must hold at least one page — a zero-page member is C1's own shape, \
         a container standing in for the archive suppressed beneath it"
    );

    let mut stmt = conn
        .prepare(
            "SELECT w.id, m.path FROM member m \
             JOIN edition e ON m.edition_id = e.id \
             JOIN work w ON e.work_id = w.id \
             ORDER BY w.id",
        )
        .expect("preparing the member paths");
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("querying member paths")
        .collect::<Result<_, _>>()
        .expect("collecting member paths");
    drop(stmt);

    let mut dirs_per_work: std::collections::BTreeMap<i64, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for (work_id, path) in rows {
        let member_path = std::path::Path::new(&path);
        let owning_root = roots
            .iter()
            .find(|(_, dir)| member_path.starts_with(dir))
            .map_or_else(
                || format!("(no known settled root: {path})"),
                |(_, dir)| dir.display().to_string(),
            );
        dirs_per_work
            .entry(work_id)
            .or_default()
            .insert(owning_root);
    }
    let mixed: Vec<(i64, Vec<String>)> = dirs_per_work
        .into_iter()
        .filter(|(_, dirs)| dirs.len() > 1)
        .map(|(id, dirs)| (id, dirs.into_iter().collect()))
        .collect();
    assert!(
        mixed.is_empty(),
        "work(s) hold members from more than one settled directory, which is C3's shape twice \
         over — a rule or a suppression bug swallowing material across series: {mixed:?}"
    );
}

//! Standing checks against the real corpus. Gated on `TAIYAKU_CORPUS` and
//! skipping cleanly without it, exactly as the sixteen already in this
//! workspace do.
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
/// no `by: "hand"` run and no `note` field on any of its twelve documents —
/// every edition, alignment and run here is machine-produced. Those three
/// fields are not untested, just not tested here; they are covered by the
/// hand-written fixtures in `import/tests.rs` instead.
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
/// derivation — which this corpus job never had (whole-branch review, I9).
/// `cargo test --workspace --all-features corpus::` matched twenty-one
/// checks before this one existed, spread across `taiyaku-cli`,
/// `taiyaku-serve` and this crate's own `every_corpus_sidecar_survives_a_
/// round_trip`, and every one of them exercises alignment, credits, regions
/// or the store's own round trip against real material — none calls
/// [`crate::scan::scan_root`] or [`crate::derive::derive`]. The only
/// real-data evidence the scanner had before this was an ad-hoc timing run
/// recorded by hand in `docs/notes.md` — which is exactly where both C1 (an
/// entire library derived to one zero-page work) and I1 (a proposal counter
/// that read zero unconditionally) were misread as expected behaviour,
/// because nothing automated was there to disagree.
///
/// **Scoped to the corpus's *settled* material, one root per
/// `<series>-vNN/` directory, and `incoming/` is not walked at all.**
/// `corpus/README.md` draws this boundary itself: the top level is
/// "settled material used for regression", while `incoming/` is "whatever a
/// particular investigation needed and has not been tidied". Measured
/// first, unscoped: `incoming/`'s many unrelated investigations — `bocchi`,
/// `chainsawman`, `onepiece`, `tongari`… — share no rule beyond the four
/// seeded defaults, which resolve every path under `incoming/` to *one*
/// series (`^(?<series>[^/]+)/` on the first path component alone) and
/// then, for any two entries that happen to resolve the same volume number
/// from their own filenames, one *work* — several genuinely unrelated
/// titles as different editions of "the same" volume. That is a real,
/// already-documented property of running the bare seeded rules over an
/// untidied dump — Task 11's own `merged` count over the whole corpus (42)
/// names `incoming/`'s flat layout as the reason most of them are not one —
/// not a scanner defect this branch introduced, and a standing check must
/// not fail on a fact of the fixture rather than a regression. Each settled
/// directory becoming its own `root_id` also makes the third assertion
/// below exact rather than a path-prefix guess: two works can share a root
/// only if their own material genuinely does.
///
/// **What it asserts, and why each one is C1 or C3 by name.**
///
/// - `report.works > 0` — the corpus must derive *something*, or the other
///   two assertions below are vacuously true and this test measures nothing.
/// - Every `member.pages` is at least one. C1's exact shape: a volume
///   directory holding archives took `pages = Some(0)` from the walk,
///   became a member in its own right, and suppressed the archive that
///   actually held the pages beneath it — a container standing in for its
///   own contents, holding none of them.
/// - No work holds members from more than one root — one `<series>-vNN/`
///   directory must never contribute to a work another one's material is
///   also on. C3's exact shape: the first attempt at the fix collapsed
///   `incoming/bocchi`'s five editions into one 439-page member and
///   `incoming/onepiece`'s six into one of 630, each spanning several
///   directories in a single work; a `set_series` rule broad enough to
///   reach the root did the same to the *whole* library. Neither incident
///   held its material inside one project's own directory, which is what
///   this checks for directly rather than re-deriving from the rules.
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

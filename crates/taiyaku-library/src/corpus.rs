//! Standing checks against the real corpus. Gated on `TAIYAKU_CORPUS` and
//! skipping cleanly without it, exactly as the sixteen already in this
//! workspace do.

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

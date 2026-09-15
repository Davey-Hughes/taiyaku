//! What `main` itself does, which no unit test can reach.
//!
//! Everything below `run` is tested in place. `main` is the four lines around
//! it — take the arguments, call it, print the message, set the exit code —
//! and each of those four is a contract something outside this repository
//! depends on: `scripts/margin-survey.sh` and the collectors beside it read
//! stdout and branch on the status. A `main` that did nothing at all would
//! leave every other test in the crate green.

use std::process::Command;

/// The binary this crate builds, resolved by Cargo rather than by guessing at
/// a path under `target/`, which differs by profile and by platform.
fn taiyaku() -> Command {
    Command::new(env!("CARGO_BIN_EXE_taiyaku"))
}

/// Runs the binary with `args` and returns its combined stdout and stderr, for
/// a test that cares what a command printed rather than only whether it
/// succeeded. `library import`'s "imported" line and `serve --help`'s usage
/// both go to different streams (stdout for the former, stderr for some of
/// `serve`'s own diagnostics) depending on which code path answers, so the
/// two are combined rather than making every caller guess which one to read.
fn taiyaku_output(args: &[&str]) -> String {
    let run = taiyaku().args(args).output().expect("the binary runs");
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

/// A fixture sidecar under this test binary's own scratch directory: one hub
/// edition of one member. `content_key` hashes the member's *declared*
/// `sha256`, not its actual bytes, so no archive needs to exist on disk.
/// `name` is only ever `"taiyaku.json"` today; kept as a parameter so a
/// future fixture does not have to invent a second helper.
fn fixture(name: &str) -> String {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-fixtures");
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let path = dir.join(name);
    std::fs::write(
        &path,
        r#"{"taiyaku":1,"work":{"title":"Fixture","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"ab12","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    path.to_str().expect("a utf-8 temp path").to_string()
}

/// A fresh, uniquely-named `--db` path under this test binary's own scratch
/// directory — `tag` names the caller, the same convention
/// `duplicates/tests.rs`'s own temp-path helpers use, so two tests never
/// share one database file.
///
/// **All three files go, not just the `.db`.** Every connection this workspace
/// opens sets `journal_mode = WAL` (`taiyaku_library::set_wal`), so a library
/// on disk is three files: `x.db`, `x.db-wal` and `x.db-shm`. Removing only
/// the first left a run that had crashed — or been killed mid-test — with a
/// write-ahead log beside the "fresh" database that the next open would
/// replay, so a test starting from nothing could find rows in it. Nothing has
/// been seen to go wrong; this is the kind of thing that goes wrong once and
/// takes a day (whole-branch review 3, Minor 13).
///
/// **The process id is part of the name too** (delta review M6). The `-wal`
/// removal above closed the crashed-run half of that hazard and left the
/// concurrent-run half open: the path was still fixed per `tag`, so two
/// overlapping runs of this binary shared one database file and each cleared
/// the other's out from under it — the very collision the paragraph above
/// describes, arriving live rather than from a previous run.
fn db_path(tag: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "taiyaku-cli-test-library-{tag}-{}.db",
        std::process::id()
    ));
    for suffix in ["", "-wal", "-shm"] {
        let mut sidecar = path.clone().into_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(sidecar);
    }
    path.to_str().expect("a utf-8 temp path").to_string()
}

/// A refusal exits non-zero and says why, prefixed so a message from this tool
/// is distinguishable from one from the shell.
#[test]
fn a_refused_command_exits_one_and_names_the_tool() {
    let run = taiyaku()
        .arg("no-such-command")
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("taiyaku: "),
        "the message names the tool: {stderr}"
    );
    assert!(
        stderr.contains("no-such-command"),
        "and names what it refused: {stderr}"
    );
    assert!(
        run.stdout.is_empty(),
        "a refusal writes nothing to the output stream"
    );
}

/// No arguments is the usage, and the usage is a success: someone asking what
/// this is has not made a mistake.
#[test]
fn no_arguments_prints_the_usage_to_stdout_and_succeeds() {
    let run = taiyaku().output().expect("the binary runs");

    assert_eq!(run.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.starts_with("taiyaku "),
        "the version leads the usage: {stdout}"
    );
    assert!(
        stdout.contains("usage: taiyaku align"),
        "and the commands follow it: {stdout}"
    );
}

/// `--cache` with nothing after it is a refusal rather than a silent fall
/// back to the default. The same contract every other value-taking flag in
/// this binary holds, and the reason `option_value` returns a `Result`.
///
/// Only refusal paths are exercised from here: a `serve` that parsed its
/// arguments successfully would bind a port and block forever.
#[test]
fn serve_refuses_cache_with_no_directory_after_it() {
    let run = taiyaku()
        .args(["serve", "some.json", "--cache"])
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("taiyaku: "),
        "the message names the tool: {stderr}"
    );
    assert!(
        stderr.contains("--cache needs a directory"),
        "and says what was missing: {stderr}"
    );
    assert!(
        run.stdout.is_empty(),
        "a refusal writes nothing to the output stream"
    );
}

/// Adding a flag to `serve`'s accepted list must not accept everything.
/// `reject_unknown_flags` is the guard, and a list edited by hand is exactly
/// where it could be widened by accident.
#[test]
fn serve_still_refuses_an_unknown_flag_and_lists_cache_among_the_real_ones() {
    let run = taiyaku()
        .args(["serve", "some.json", "--cash", "/tmp/x"])
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--cash"),
        "the refusal names what it refused: {stderr}"
    );
    assert!(
        stderr.contains("--cache"),
        "and the alphabet it listed includes the real flag: {stderr}"
    );
}

/// The usage names the flag. A flag that works but is not advertised is one
/// nobody finds, and this line is the only place `serve`'s options are
/// listed.
#[test]
fn the_usage_advertises_serve_cache() {
    let run = taiyaku().output().expect("the binary runs");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let serve_line = stdout
        .lines()
        .find(|l| l.contains("taiyaku serve"))
        .expect("the usage lists serve");
    assert!(
        serve_line.contains("[--cache DIR]"),
        "serve's usage line advertises the flag: {serve_line}"
    );
}

/// `library scan` and `library rules` reached `command`'s dispatch and
/// `README.md`/`docs/status.md` without ever reaching `print_usage` — a
/// person running bare `taiyaku` could not discover either. No test named
/// `print_usage` at all is what let that stand; this is that test.
#[test]
fn the_usage_lists_library_scan_and_rules() {
    let run = taiyaku().output().expect("the binary runs");

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.lines().any(|l| l.contains("taiyaku library scan")),
        "the usage lists library scan: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.contains("taiyaku library rules")),
        "the usage lists library rules: {stdout}"
    );
}

/// `taiyaku library import` reports what the merge did — spec §4.2's
/// requirement that an import say so rather than swallow `Sidecar::merge_spoke`'s
/// `Merged` outcomes, in the fixed-field style `--stats` and
/// `duplicate-survey.sh` established (`library.rs`'s own `merge_summary`).
/// This first import of a fresh fixture into a fresh database has nothing to
/// merge against, so the outcome list is empty and the summary is just the
/// word this test looks for — the fuller, non-empty case is
/// `library::tests::merge_outcomes_are_counted_by_kind_and_the_three_counted_variants_are_summed`,
/// a pure unit test over `merge_summary` itself.
#[test]
fn library_import_reports_what_the_merge_did() {
    let out = taiyaku_output(&[
        "library",
        "import",
        &fixture("taiyaku.json"),
        "--db",
        &db_path("import-reports"),
    ]);
    assert!(out.contains("imported"), "{out}");
}

/// `taiyaku serve <sidecar>...` keeps working: it imports those paths and
/// serves them (design §5). That is the migration path for an existing
/// install and what keeps the three corpus-gated tests in
/// `serve/src/corpus.rs` alive — this checks the argument *shape* survived
/// (still a positional `<taiyaku.json>...` list, not e.g. a `--sidecar`
/// flag) without actually binding a port, which is what `--help`'s early
/// return (`main.rs`'s `run`) buys: the same whole-binary usage `taiyaku`
/// with no arguments prints, and `the_usage_advertises_serve_cache` already
/// pins that its serve line names `<taiyaku.json>` positionally.
#[test]
fn serve_still_accepts_sidecar_paths() {
    let out = taiyaku_output(&["serve", "--help"]);
    assert!(out.contains("<taiyaku.json>"), "{out}");
}

/// **Does not exercise the placeholder-path hazard.** Both imports below go
/// through `taiyaku library import`, so the first call writes a real
/// `content_key` — a hash of the sidecar's declared `sha256`s — and
/// `taiyaku_library::import::sidecar`/`reimport` matches on `content_key`
/// *before* falling back to `sidecar_path`. The second import reconciles by
/// content hash regardless of how its path was spelled, which is a property
/// that predates this task and holds even with `canonical_sidecar_path`
/// reverted to a no-op (confirmed by running it that way). What this test
/// actually pins: two `taiyaku library import` calls against the identical
/// underlying file, spelled two different ways, leave exactly one `work`
/// row — reconciliation is by content, not by path string equality.
///
/// The placeholder hazard is specifically about a row whose `content_key` is
/// the `bridge:{slug}` sentinel — minted by `taiyaku serve` recording a
/// reading position *before* any import — which can never match on
/// `content_key` and so falls through to the `sidecar_path` comparison this
/// module's canonicalisation exists to fix. That is not reachable from this
/// test, because nothing here ever creates a placeholder row. The
/// discriminating coverage for that hazard lives in
/// `crates/taiyaku-cli/src/library.rs`'s own tests (`crates/taiyaku-cli/src/library/tests.rs`):
/// `importing_the_same_file_under_a_different_spelling_reuses_the_placeholder_row`
/// goes through `canonical_sidecar_path`/`import_canonical` and is confirmed
/// to fail if `canonical_sidecar_path` is reverted to a no-op;
/// `without_canonicalisation_the_same_scenario_would_orphan_the_placeholder`
/// calls `taiyaku_library::import::sidecar` directly, bypassing
/// `canonical_sidecar_path` on purpose, to show the underlying defect is real
/// rather than hypothetical (it passes regardless of the fix, since it never
/// invokes the fixed function at all — that is by design, not a gap).
#[test]
fn two_imports_of_one_file_reconcile_by_content_not_path() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-reimport-spelling");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let sidecar = dir.join("taiyaku.json");
    std::fs::write(
        &sidecar,
        r#"{"taiyaku":1,"work":{"title":"Reimport Spelling","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"cd34","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    let db = db_path("reimport-spelling");

    let first = taiyaku_output(&["library", "import", sidecar.to_str().unwrap(), "--db", &db]);
    assert!(first.contains("imported"), "{first}");

    // A different spelling of the identical file: a redundant `.` component,
    // which canonicalises to the same real path but is not the same string.
    // Irrelevant to how this reconciles, though: both sidecars declare the
    // same member `sha256`s, so `content_key` matches regardless of spelling
    // and this second import never falls through to a `sidecar_path` string
    // comparison at all.
    let dotted = dir.join(".").join("taiyaku.json");
    let second = taiyaku_output(&["library", "import", dotted.to_str().unwrap(), "--db", &db]);
    assert!(second.contains("imported"), "{second}");

    // The observable claim: reading the same database `taiyaku` just wrote,
    // exactly one work exists — not two. Printed output alone cannot tell a
    // reused row from a freshly-minted one that merely also printed
    // "imported", so this reads the row count directly.
    let conn = rusqlite::Connection::open(&db).expect("open the same database taiyaku wrote");
    let works: i64 = conn
        .query_row("SELECT COUNT(*) FROM work", [], |row| row.get(0))
        .expect("count works");
    assert_eq!(
        works, 1,
        "two spellings of one file must reconcile onto one work, not two"
    );
}

/// **Whole-branch review 3's I1.** `export_targets`' entire stdout reporting
/// surface — `wrote`, `removed`, `kept`, and the final summary line — was
/// provable by deleting all four `println!`s in `library.rs::export_targets`
/// and watching the whole suite stay green: the *arithmetic* behind the
/// totals was pinned (`export_all_into_one_dir_keeps_every_work_rather_than_overwriting`,
/// a unit test over `export_targets` directly), but nothing asserted a reader
/// running the real binary ever sees a line reporting it. This drives
/// `taiyaku library import` and `taiyaku library export --in-place` and reads
/// every one of the four kinds of line off the process's actual stdout.
///
/// The recipe behind each outcome:
/// - **`wrote`**: `taiyaku.json` is written on every in-place export
///   unconditionally (C2's refusal exists precisely because this always
///   happens), and `taiyaku.notes.json` is always written too — nothing has
///   ever recorded notes for this fixture, so there is nothing to keep.
/// - **`removed`**: a hand-written `taiyaku.library.json` holding nothing but
///   `taiyaku_library`/`updated` — the content-free "skeleton" shape
///   `export::work`'s own doc comment and whole-branch review 3's verdict
///   both describe — migrates cleanly on import (`library=migrated`, and the
///   marker records it absorbed) but stays an *empty* `LibraryDoc`, so the
///   next export finds no state behind it and deletes it.
/// - **`kept`**: a `taiyaku.truth.json` that appears only *after* the first
///   import is reported `truth=unread` on the second import — settled, but
///   never read — and the export that follows leaves it alone rather than
///   deleting or overwriting it. The same recipe as the unit test
///   `export_in_place_does_not_delete_a_sibling_the_database_never_absorbed`,
///   one layer up, through the real binary's own stdout.
#[test]
fn export_reports_wrote_removed_kept_and_the_summary_line_on_real_stdout() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-export-reporting");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    // Canonicalised up front: `export::work` derives every path it reports
    // from the sidecar's own canonicalised `sidecar_path`, so an assertion
    // built from an uncanonicalised `dir` could mismatch on a platform where
    // the temp directory is itself a symlink.
    let dir = dir
        .canonicalize()
        .expect("canonicalises the fixture directory");
    let sidecar = dir.join("taiyaku.json");
    std::fs::write(
        &sidecar,
        r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"ee91","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    let library_json = dir.join("taiyaku.library.json");
    std::fs::write(
        &library_json,
        r#"{"taiyaku_library":1,"updated":"2024-01-01T00:00:00Z"}"#,
    )
    .expect("writing the skeleton library.json");

    let db = db_path("export-reporting");

    let first = taiyaku_output(&["library", "import", sidecar.to_str().unwrap(), "--db", &db]);
    assert!(
        first.contains("library=migrated"),
        "the skeleton library.json must actually migrate, or this test proves \
         nothing about the removed case: {first}"
    );

    // A truth sibling that turns up only after the first import: never read,
    // because nothing has imported since it appeared.
    let truth_json = dir.join("taiyaku.truth.json");
    std::fs::write(
        &truth_json,
        r#"{"taiyaku_truth":1,"editions":{"raw":{"members":[],"pages":{
            "3":[{"x":0.1,"y":0.2,"w":0.3,"h":0.05,"axis":"v","kind":"dialogue","ignore":false,"text":"hours of it"}]
        }}}}"#,
    )
    .expect("writing the truth sibling");
    let second = taiyaku_output(&["library", "import", sidecar.to_str().unwrap(), "--db", &db]);
    assert!(
        second.contains("truth=unread"),
        "the truth sibling must be reported unread, or this test proves nothing \
         about the kept case: {second}"
    );

    let out = taiyaku_output(&["library", "export", "t", "--in-place", "--db", &db]);

    assert!(
        out.contains(&format!("wrote {}", sidecar.display())),
        "the sidecar itself is always written in place: {out}"
    );
    assert!(
        out.contains(&format!(
            "wrote {}",
            dir.join("taiyaku.notes.json").display()
        )),
        "notes.json is always written: {out}"
    );
    assert!(
        out.contains(&format!(
            "removed {} (no longer any state for it)",
            library_json.display()
        )),
        "the empty library.json must be reported removed: {out}"
    );
    assert!(
        out.contains(&format!(
            "kept {} (this library has never read it, so it was not \
             overwritten; move it aside and export again to replace it with \
             what this library holds — which is no file at all where it \
             holds nothing)",
            truth_json.display()
        )),
        "the unabsorbed truth.json must be reported kept: {out}"
    );
    assert!(
        out.contains("exported 2 document(s) across 1 work(s)"),
        "the summary line must count the two documents actually written: {out}"
    );
}

/// **Whole-branch review 3's I2, `library.rs`'s call site.** Gutting
/// `library.rs::import`'s `for line in malformed_sibling_warnings(...) {
/// eprintln!("{line}"); }` loop leaves the whole suite green: nothing greps a
/// real process's stderr for the sentence that tells a reader a sibling
/// document was left unread, even though `malformed_sibling_warnings` itself
/// is well tested (`library/tests.rs`'s
/// `every_malformed_sibling_gets_a_sentence_naming_the_file_and_the_reason`).
/// This drives `taiyaku library import` against a fixture with an unreadable
/// `taiyaku.notes.json` and reads the warning off the binary's real stderr.
#[test]
fn library_import_prints_the_malformed_sibling_warning_to_stderr() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-import-malformed-warning");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let dir = dir
        .canonicalize()
        .expect("canonicalises the fixture directory");
    let sidecar = dir.join("taiyaku.json");
    std::fs::write(
        &sidecar,
        r#"{"taiyaku":1,"work":{"title":"Malformed Sibling","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"ff02","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    std::fs::write(dir.join("taiyaku.notes.json"), "{ this is not valid json")
        .expect("writing the malformed sibling");

    let out = taiyaku_output(&[
        "library",
        "import",
        sidecar.to_str().unwrap(),
        "--db",
        &db_path("import-malformed-warning"),
    ]);

    assert!(
        out.contains("imported"),
        "the import itself must still succeed despite the bad sibling: {out}"
    );
    assert!(
        out.contains(&format!(
            "taiyaku: {}: notes not migrated:",
            sidecar.display()
        )),
        "the reader must be told the sibling was not read: {out}"
    );
}

/// **I1 / I2.** `derive` already calls `proposals::raise` once, at the end of
/// its own pass; `library scan` used to call it a *second* time purely to get
/// a number to print, and since the first call had already opened everything
/// raisable, the second call's own `INSERT OR IGNORE` conflicted on every row
/// and the printed count was unconditionally `0` — the failure
/// `derive_summary_prints_suppressed_and_rekeyed_when_nonzero` only ever
/// justified itself by preventing, since it exercises the pure formatter with
/// a hand-built `raised` argument and never the real command's own count.
/// Nothing before this ran `library scan` for real and read its real stdout.
///
/// A tree with a genuine colour-edition suffix — the same shape
/// `proposals::tests::library_with_two_suffixed_series` scans — is what
/// raises exactly one `edition-of` proposal.
#[test]
fn library_scan_reports_the_real_proposal_count_on_real_stdout() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-scan-proposals");
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir
        .join("Go-Toubun no Hanayome")
        .join("Go-Toubun no Hanayome - v01");
    let colour = dir
        .join("Go-Toubun no Hanayome - Full Color Edition")
        .join("Go-Toubun no Hanayome - Full Color Edition - v01");
    std::fs::create_dir_all(&base).expect("mkdir base");
    std::fs::create_dir_all(&colour).expect("mkdir colour");
    std::fs::write(base.join("000.jpg"), b"page").expect("write base page");
    std::fs::write(colour.join("000.jpg"), b"page").expect("write colour page");

    let out = taiyaku_output(&[
        "library",
        "scan",
        "--add",
        dir.to_str().unwrap(),
        "--db",
        &db_path("scan-proposals"),
    ]);

    assert!(
        out.contains("; 1 proposal(s) raised"),
        "the real command must report the one real proposal this tree raises, not zero: {out}"
    );

    // §8's per-root watch report and the walk's own added/updated/gone line
    // are two separate `println!`s in `scan`, and **deleting either left the
    // whole suite green**: "watching" and "scanning every" appear nowhere in
    // any CLI test, and `scan_summary`'s unit tests exercise the formatter
    // with a hand-built report, never the command. A counter nobody prints is
    // the failure it exists to prevent — the rule `derive_summary`'s own
    // conditional fields already follow — and that rule was never checked at
    // the one layer where a person actually reads it.
    assert!(
        out.contains(&format!("root {}", dir.display())),
        "the per-root watch report line must reach stdout: {out}"
    );
    assert!(
        out.contains(" added, ") && out.contains(" gone"),
        "the walk's own added/updated/gone summary must reach stdout: {out}"
    );

    // **And the probe must be given time to answer.** `PROBE_TIMEOUT` is two
    // seconds; at zero every root on earth reports "watch probe failed" and
    // silently drops to the 15-minute interval, with every database
    // assertion in this crate still passing. Asserting the *success* form is
    // the only thing that separates the two, so this test does depend on a
    // watcher working on this host's temp directory — the same bet
    // `taiyaku-library`'s own `a_probe_on_an_ordinary_local_directory_sees_its_own_file`
    // already makes, in the same place, for the same reason.
    assert!(
        out.contains("directories — scanning every 24h"),
        "a watchable root reports the long interval; at PROBE_TIMEOUT of zero this reads \
         `watch probe failed`: {out}"
    );
}

/// **I3, end to end.** `proposals::open`, `accept`, `unaccept` and `dismiss`
/// had no caller outside their own tests before this — `taiyaku library` had
/// only `import|export|scan|rules`, and two rounds of Critical fixes went
/// into an `accept` a person could not invoke. This drives the real binary
/// through a scan that raises one proposal, lists it, and accepts it —
/// reading everything off the process's actual stdout, the way
/// `library_import_reports_what_the_merge_did` already does for `import`.
#[test]
fn library_proposals_lists_and_accepts_over_real_stdout() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-proposals");
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir
        .join("Go-Toubun no Hanayome")
        .join("Go-Toubun no Hanayome - v01");
    let colour = dir
        .join("Go-Toubun no Hanayome - Full Color Edition")
        .join("Go-Toubun no Hanayome - Full Color Edition - v01");
    std::fs::create_dir_all(&base).expect("mkdir base");
    std::fs::create_dir_all(&colour).expect("mkdir colour");
    std::fs::write(base.join("000.jpg"), b"page").expect("write base page");
    std::fs::write(colour.join("000.jpg"), b"page").expect("write colour page");
    let db = db_path("proposals-e2e");

    let scan_out = taiyaku_output(&[
        "library",
        "scan",
        "--add",
        dir.to_str().unwrap(),
        "--db",
        &db,
    ]);
    assert!(
        scan_out.contains("; 1 proposal(s) raised"),
        "the scan must actually have raised the one proposal this test accepts: {scan_out}"
    );

    let list_out = taiyaku_output(&["library", "proposals", "--db", &db]);
    assert!(
        list_out.contains("[edition-of]"),
        "the listing names the kind: {list_out}"
    );
    assert!(
        list_out.contains("Full Color"),
        "and the reason, which names the series involved: {list_out}"
    );
    let id: &str = list_out
        .split_whitespace()
        .next()
        .expect("the listing's first token is the proposal id");

    let accept_out = taiyaku_output(&["library", "proposals", "--accept", id, "--db", &db]);
    assert!(accept_out.contains("accepted proposal"), "{accept_out}");

    // The observable claim: a real rule row exists, reachable only by
    // actually invoking `accept` through this command.
    let conn = rusqlite::Connection::open(&db).expect("open the same database taiyaku wrote");
    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(rules, 1, "the accept must have written a real rule");
}

/// A refused `--unaccept` or `--dismiss` **exits 1 with the library's own
/// sentence** on stderr, and prints no success line.
///
/// Pinned at the binary because of what changed underneath it (O70):
/// `proposals::unaccept` and `proposals::dismiss` answered a refusal as
/// `Err(String)`, which `main` prints as `taiyaku: {message}` before exiting
/// 1, and they answer it as `Ok(Offer::Refused(why))` now — so this command
/// turns the refusal back into that error itself. An arm that printed the
/// success line instead, or reworded the sentence on the way, is a changed
/// exit code or a changed message, and the exact line is what tells.
#[test]
fn library_proposals_refusals_exit_one_with_the_librarys_own_sentence() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-proposals-refusals");
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir
        .join("Go-Toubun no Hanayome")
        .join("Go-Toubun no Hanayome - v01");
    let colour = dir
        .join("Go-Toubun no Hanayome - Full Color Edition")
        .join("Go-Toubun no Hanayome - Full Color Edition - v01");
    std::fs::create_dir_all(&base).expect("mkdir base");
    std::fs::create_dir_all(&colour).expect("mkdir colour");
    std::fs::write(base.join("000.jpg"), b"page").expect("write base page");
    std::fs::write(colour.join("000.jpg"), b"page").expect("write colour page");
    let db = db_path("proposals-refusals");

    let scan_out = taiyaku_output(&[
        "library",
        "scan",
        "--add",
        dir.to_str().unwrap(),
        "--db",
        &db,
    ]);
    assert!(scan_out.contains("; 1 proposal(s) raised"), "{scan_out}");
    let list_out = taiyaku_output(&["library", "proposals", "--db", &db]);
    let id: &str = list_out
        .split_whitespace()
        .next()
        .expect("the listing's first token is the proposal id");

    // Never accepted, so there is nothing to un-accept.
    let run = taiyaku()
        .args(["library", "proposals", "--unaccept", id, "--db", &db])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let expected = format!("taiyaku: proposal {id} has no accepted rule to remove");
    assert!(
        stderr.lines().any(|line| line == expected),
        "the library's sentence, unreworded, on its own line: {stderr}"
    );
    assert!(!stdout.contains("un-accepted"), "{stdout}");

    let accept_out = taiyaku_output(&["library", "proposals", "--accept", id, "--db", &db]);
    assert!(accept_out.contains("accepted proposal"), "{accept_out}");

    // Accepted, so a dismiss is refused rather than orphaning the rule.
    let run = taiyaku()
        .args(["library", "proposals", "--dismiss", id, "--db", &db])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let expected = format!("taiyaku: proposal {id} is accepted — un-accept it first, then dismiss");
    assert!(
        stderr.lines().any(|line| line == expected),
        "the library's sentence, unreworded, on its own line: {stderr}"
    );
    assert!(!stdout.contains("dismissed proposal"), "{stdout}");
}

/// **`if !reopened.is_empty()`, not `if reopened.is_empty()`.** An import
/// that orphans an accepted proposal's rule must say so; `library.rs`'s own
/// unit tests call `rules` directly and this crate has no stdout-capturing
/// helper (deliberately, per the note above), so the only place this line
/// is reachable at all is through the real binary.
///
/// Builds the same one-proposal fixture
/// `library_proposals_lists_and_accepts_over_real_stdout` does, accepts it —
/// which writes a fifth rule, ordinal 5, beside the four seeded defaults —
/// then re-imports a rule document with that fifth rule stripped back out,
/// exactly as replacing the whole set with something that no longer mentions
/// it would leave behind. `import_json`'s own `reopen_orphaned_accepts` must
/// then find the accepted proposal's `created_rule_id` pointing at a row
/// that no longer exists and reopen it — and the CLI must print that it did.
#[test]
fn library_rules_import_reports_a_proposal_that_lost_its_accepted_rule() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-rules-reopen");
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir
        .join("Go-Toubun no Hanayome")
        .join("Go-Toubun no Hanayome - v01");
    let colour = dir
        .join("Go-Toubun no Hanayome - Full Color Edition")
        .join("Go-Toubun no Hanayome - Full Color Edition - v01");
    std::fs::create_dir_all(&base).expect("mkdir base");
    std::fs::create_dir_all(&colour).expect("mkdir colour");
    std::fs::write(base.join("000.jpg"), b"page").expect("write base page");
    std::fs::write(colour.join("000.jpg"), b"page").expect("write colour page");
    let db = db_path("rules-import-reopen");

    let scan_out = taiyaku_output(&[
        "library",
        "scan",
        "--add",
        dir.to_str().unwrap(),
        "--db",
        &db,
    ]);
    assert!(
        scan_out.contains("; 1 proposal(s) raised"),
        "the scan must have raised the one proposal this test accepts: {scan_out}"
    );

    let list_out = taiyaku_output(&["library", "proposals", "--db", &db]);
    let id: &str = list_out
        .split_whitespace()
        .next()
        .expect("the listing's first token is the proposal id");

    let accept_out = taiyaku_output(&["library", "proposals", "--accept", id, "--db", &db]);
    assert!(accept_out.contains("accepted proposal"), "{accept_out}");

    // Stdout alone, like the note test above: mixing in stderr's "library
    // database at …" line would break the JSON this is fed straight back
    // into.
    let exported = String::from_utf8_lossy(
        &taiyaku()
            .args(["library", "rules", "--db", &db])
            .output()
            .expect("the binary runs")
            .stdout,
    )
    .into_owned();
    let mut rules: Vec<serde_json::Value> =
        serde_json::from_str(&exported).expect("a JSON array of rules");
    let before = rules.len();
    rules.retain(|rule| rule["ordinal"].as_i64().is_some_and(|o| o <= 4));
    assert!(
        rules.len() < before,
        "the accept above must have added a fifth rule for this filter to drop: {exported}"
    );
    let path = std::env::temp_dir().join(format!(
        "taiyaku-cli-test-rules-reopen-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_string(&rules).expect("serialises"))
        .expect("write the trimmed rule document");

    let out = taiyaku_output(&[
        "library",
        "rules",
        "--import",
        path.to_str().unwrap(),
        "--db",
        &db,
    ]);
    assert!(
        out.contains(&format!(
            "1 accepted proposal(s) lost the rule they were accepted by and are open again: \
             [{id}]"
        )),
        "the accepted proposal must be reported reopened, not left unmentioned: {out}"
    );
}

/// **Whole-branch review 3's I2, `main.rs`'s call site.** The identical loop
/// is duplicated in `serve_work` — `taiyaku serve <sidecar>...` imports every
/// path it is given the same way `library import` does, this module's own
/// doc comment says so — and gutting *that* copy independently also leaves
/// the suite green. `serve` never returns on success, so this occupies the
/// address first with a listener of its own: the bind inside
/// `taiyaku_serve::serve` then fails immediately with `AddrInUse`, the
/// process exits instead of blocking forever, and everything printed
/// *before* that bind attempt — including this warning, which fires while
/// importing, well before the network is ever touched — is still on its
/// stderr to read.
#[test]
fn serve_prints_the_malformed_sibling_warning_before_refusing_to_bind() {
    let dir = std::env::temp_dir().join("taiyaku-cli-test-serve-malformed-warning");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let dir = dir
        .canonicalize()
        .expect("canonicalises the fixture directory");
    let sidecar = dir.join("taiyaku.json");
    std::fs::write(
        &sidecar,
        r#"{"taiyaku":1,"work":{"title":"Serve Malformed Sibling","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"ff03","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    std::fs::write(dir.join("taiyaku.notes.json"), "{ this is not valid json")
        .expect("writing the malformed sibling");

    // Bound and held for the rest of this test, so the child process below
    // can never win the race to that address — `taiyaku serve` must fail to
    // bind it, not succeed and block forever.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("binds an ephemeral port");
    let addr = occupied.local_addr().expect("reads back the bound address");

    let run = taiyaku()
        .args([
            "serve",
            sidecar.to_str().unwrap(),
            "--db",
            &db_path("serve-malformed-warning"),
            "--addr",
            &addr.to_string(),
        ])
        .output()
        .expect("the binary runs");

    assert_ne!(
        run.status.code(),
        Some(0),
        "the address is already taken, so this must refuse rather than serve: \
         {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains(&format!(
            "taiyaku: {}: notes not migrated:",
            sidecar.display()
        )),
        "the reader must be told the sibling was not read before the bind \
         failure ends the process: {stderr}"
    );
    drop(occupied);
}

/// **A rule import stores rules and derives nothing**, and the command used
/// to print only "imported rules from …". The whole claim the rule layer
/// makes is that editing a rule re-derives the library without touching the
/// filesystem — which is true, and requires something to call `derive`. A
/// person who has just replaced every rule and seen only a success line
/// reasonably believes the library already reflects them.
///
/// Driven through the real binary because that is where the line reaches
/// anybody: `library.rs`'s own unit tests call `rules` directly and this
/// crate has no stdout-capturing helper, deliberately.
#[test]
fn library_rules_import_says_a_scan_is_needed_before_the_rules_decide_anything() {
    let db = db_path("rules-import-note");
    // Stdout alone here, not `taiyaku_output`: this document is fed straight
    // back to `--import`, and the "library database at …" note the command
    // writes to stderr would not parse as JSON beside it.
    let run = taiyaku()
        .args(["library", "rules", "--db", &db])
        .output()
        .expect("the binary runs");
    let exported = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(exported.contains("\"ordinal\""), "{exported}");

    let path = std::env::temp_dir().join(format!(
        "taiyaku-cli-test-rules-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, &exported).expect("write the rule document");

    let out = taiyaku_output(&[
        "library",
        "rules",
        "--import",
        path.to_str().unwrap(),
        "--db",
        &db,
    ]);
    assert!(out.contains("imported rules from"), "{out}");
    assert!(
        out.contains("taiyaku library scan"),
        "the command that actually applies them must be named: {out}"
    );
}

/// A bare word given to `library scan` is refused by name rather than
/// ignored — checked here as well as in `library.rs`'s own unit tests
/// because the exit status is what a script sees, and a command that
/// silently ignores its argument exits 0.
#[test]
fn library_scan_refuses_a_bare_path_and_exits_nonzero() {
    let run = taiyaku()
        .args([
            "library",
            "scan",
            "/srv/manga",
            "--db",
            &db_path("scan-positional"),
        ])
        .output()
        .expect("the binary runs");
    assert!(
        !run.status.success(),
        "a silently ignored argument would exit 0"
    );
    let message = String::from_utf8_lossy(&run.stderr);
    assert!(message.contains("/srv/manga"), "{message}");
    assert!(message.contains("--add"), "{message}");
}
